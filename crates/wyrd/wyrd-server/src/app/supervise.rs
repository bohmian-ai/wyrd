//! Uniform supervised task loop for server transports and workers.
//!
//! All long-lived futures run in one `JoinSet<TaskExit>`. The first task to
//! finish triggers cooperative shutdown; remaining tasks drain within a budget,
//! then are aborted. The first pre-shutdown failure (or unexpected exit) is the
//! terminal error; drain-phase exits are logged only.

use std::time::Duration;

use tokio::task::JoinSet;
use tokio::time::{Instant, timeout_at};
use tokio_util::sync::CancellationToken;

/// Identifies a supervised task for terminal-error classification and logging.
#[derive(Debug, Clone, Copy)]
pub enum TaskId {
    Http,
    Grpc,
    Metrics,
    Signal,
    Worker(&'static str),
}

/// The resolved outcome of one supervised task.
pub struct TaskExit {
    pub id: TaskId,
    /// `Ok(())` on clean completion; `Err(message)` on failure.
    pub outcome: Result<(), String>,
}

/// Wrap a `()`-producing future (worker/signal) into a `TaskExit`.
pub async fn worker_task<F>(id: TaskId, fut: F) -> TaskExit
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    fut.await;
    TaskExit {
        id,
        outcome: Ok(()),
    }
}

/// Wrap a `Result<(), E: Display>`-producing future (transport) into a `TaskExit`.
pub async fn fallible_task<F, E>(id: TaskId, fut: F) -> TaskExit
where
    F: std::future::Future<Output = Result<(), E>> + Send + 'static,
    E: std::fmt::Display,
{
    let outcome = fut.await.map_err(|e| e.to_string());
    TaskExit { id, outcome }
}

/// Drive the supervised set to completion. Returns the terminal error message,
/// or `None` on clean shutdown.
///
/// Semantics (must match the retired `select!` + `await_drain`):
/// - The first task to finish cancels `shutdown`.
/// - `Signal` finishing is the normal shutdown trigger — never terminal.
/// - Any transport or worker finishing *before* shutdown is terminal (a
///   transport returning `Ok` early still means it stopped unexpectedly).
/// - After shutdown is requested, remaining tasks drain up to `drain`; their
///   exits are logged, not treated as new terminal errors. On deadline, abort.
pub async fn supervise(
    mut set: JoinSet<TaskExit>,
    shutdown: CancellationToken,
    drain: Duration,
) -> Option<String> {
    let mut terminal: Option<String> = None;

    // Phase 1 — wait for the first exit (or an empty set).
    if let Some(joined) = set.join_next().await {
        classify_first(joined, &mut terminal);
    }
    shutdown.cancel();

    // Phase 2 — drain within budget, then abort.
    let deadline = Instant::now() + drain;
    loop {
        match timeout_at(deadline, set.join_next()).await {
            Ok(Some(joined)) => log_drain(joined),
            Ok(None) => break,
            Err(_elapsed) => {
                tracing::warn!("drain deadline exceeded; aborting remaining tasks");
                set.abort_all();
                break;
            }
        }
    }
    terminal
}

fn classify_first(joined: Result<TaskExit, tokio::task::JoinError>, terminal: &mut Option<String>) {
    match joined {
        Ok(TaskExit {
            id: TaskId::Signal, ..
        }) => {
            tracing::info!("shutdown signal received; initiating shutdown");
        }
        Ok(TaskExit {
            id,
            outcome: Err(msg),
        }) => {
            tracing::warn!(?id, error = %msg, "task failed; initiating shutdown");
            *terminal = Some(format!("{id:?} failed: {msg}"));
        }
        Ok(TaskExit {
            id,
            outcome: Ok(()),
        }) => {
            tracing::warn!(
                ?id,
                "task exited before shutdown signal; initiating shutdown"
            );
            *terminal = Some(format!("{id:?} exited before shutdown signal"));
        }
        Err(join_error) => {
            tracing::warn!(error = %join_error, "task panicked; initiating shutdown");
            *terminal = Some(format!("task panicked: {join_error}"));
        }
    }
}

fn log_drain(joined: Result<TaskExit, tokio::task::JoinError>) {
    match joined {
        Ok(TaskExit {
            id,
            outcome: Ok(()),
        }) => tracing::debug!(?id, "task drained"),
        Ok(TaskExit {
            id,
            outcome: Err(msg),
        }) => {
            tracing::warn!(?id, error = %msg, "task errored during drain")
        }
        Err(e) if e.is_cancelled() => {}
        Err(e) => tracing::warn!(error = %e, "task panicked during drain"),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use tokio::task::JoinSet;
    use tokio_util::sync::CancellationToken;

    use super::*;

    #[tokio::test]
    async fn signal_completion_is_graceful() {
        let shutdown = CancellationToken::new();
        let mut set: JoinSet<TaskExit> = JoinSet::new();

        set.spawn(worker_task(TaskId::Signal, async {}));

        let shutdown_clone = shutdown.clone();
        set.spawn(worker_task(TaskId::Worker("parked"), async move {
            shutdown_clone.cancelled().await;
        }));

        let result = supervise(set, shutdown.clone(), Duration::from_millis(500)).await;
        assert!(
            result.is_none(),
            "signal completion must be graceful; got: {result:?}"
        );
        assert!(
            shutdown.is_cancelled(),
            "token must be cancelled after supervise"
        );
    }

    #[tokio::test]
    async fn transport_error_is_terminal() {
        let shutdown = CancellationToken::new();
        let mut set: JoinSet<TaskExit> = JoinSet::new();

        set.spawn(fallible_task(TaskId::Http, async {
            Err::<(), String>("bind failed".to_owned())
        }));

        let shutdown_clone = shutdown.clone();
        set.spawn(worker_task(TaskId::Worker("parked"), async move {
            shutdown_clone.cancelled().await;
        }));

        let result = supervise(set, shutdown.clone(), Duration::from_millis(500)).await;
        let msg = result.expect("Http transport error must be terminal");
        assert!(
            msg.contains("Http"),
            "terminal error must name the task: {msg}"
        );
        assert!(
            msg.contains("bind failed"),
            "terminal error must include error text: {msg}"
        );
    }

    #[tokio::test]
    async fn worker_early_exit_is_terminal() {
        let shutdown = CancellationToken::new();
        let mut set: JoinSet<TaskExit> = JoinSet::new();

        set.spawn(worker_task(TaskId::Worker("x"), async {}));

        let result = supervise(set, shutdown.clone(), Duration::from_millis(500)).await;
        let msg = result.expect("worker early exit must be terminal");
        assert!(
            msg.contains("before shutdown signal"),
            "message must describe premature exit: {msg}"
        );
    }

    #[tokio::test]
    async fn drain_timeout_aborts() {
        let shutdown = CancellationToken::new();
        let mut set: JoinSet<TaskExit> = JoinSet::new();

        set.spawn(worker_task(TaskId::Signal, async {}));
        set.spawn(worker_task(TaskId::Worker("stuck"), async {
            std::future::pending::<()>().await;
        }));

        let drain = Duration::from_millis(50);
        let start = Instant::now();
        let result = supervise(set, shutdown.clone(), drain).await;
        let elapsed = start.elapsed();

        assert!(
            result.is_none(),
            "signal completion must be graceful even with stuck worker: {result:?}"
        );
        assert!(
            elapsed < Duration::from_millis(500),
            "supervise must complete within 500ms with 50ms drain; took {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn panic_first_is_terminal() {
        let shutdown = CancellationToken::new();
        let mut set: JoinSet<TaskExit> = JoinSet::new();

        set.spawn(worker_task(TaskId::Http, async {
            panic!("test panic");
        }));

        let result = supervise(set, shutdown.clone(), Duration::from_millis(100)).await;
        let msg = result.expect("panic must produce a terminal error");
        assert!(
            msg.contains("task panicked"),
            "terminal error must describe the panic: {msg}"
        );
    }
}
