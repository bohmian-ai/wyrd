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
    set: JoinSet<TaskExit>,
    shutdown: CancellationToken,
    drain: Duration,
) -> Option<String> {
    supervise_with_shutdown(set, shutdown, drain, || async {}).await
}

/// Drive supervision while running ordered readiness removal before cancellation.
///
/// The hook runs after the first exit is classified but before transport and
/// worker cancellation. Role owners use it to become unready durably while
/// already accepted requests still receive the configured drain budget.
pub async fn supervise_with_shutdown<F, Fut>(
    mut set: JoinSet<TaskExit>,
    shutdown: CancellationToken,
    drain: Duration,
    before_cancel: F,
) -> Option<String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let terminal = classify_first_exit_with_shutdown(&mut set, &shutdown).await;
    let deadline = Instant::now() + drain;
    drain_with_shutdown(set, shutdown, deadline, before_cancel).await;
    terminal
}

/// Waits for and classifies the first supervised task exit.
///
/// This phase intentionally does not create a shutdown budget. The process
/// owner creates its one absolute deadline immediately after this function
/// returns, preserving the first-exit terminal result independently of cleanup.
pub async fn classify_first_exit(set: &mut JoinSet<TaskExit>) -> Option<String> {
    let mut terminal = None;
    if let Some(joined) = set.join_next().await {
        classify_first(joined, &mut terminal);
    }
    terminal
}

/// Waits for the first supervised task while biasing an already-requested
/// external cancellation ahead of task completion.
///
/// A caller may cancel the shared token before any worker has joined. In that
/// case the cancellation is the authoritative shutdown cause and no worker
/// result is misclassified as a terminal startup failure. When both branches
/// become ready together, Tokio's `biased` ordering preserves that same
/// cancellation precedence.
pub async fn classify_first_exit_with_shutdown(
    set: &mut JoinSet<TaskExit>,
    shutdown: &CancellationToken,
) -> Option<String> {
    tokio::select! {
        biased;
        () = shutdown.cancelled() => None,
        joined = set.join_next() => {
            let mut terminal = None;
            if let Some(joined) = joined {
                classify_first(joined, &mut terminal);
            }
            terminal
        }
    }
}

/// Removes readiness, cancels transports, and drains tasks until `deadline`.
///
/// The readiness hook is first-polled before cancellation. If it or transport
/// drain consumes the remaining budget, retained tasks are aborted and no new
/// external work is started.
pub async fn drain_with_shutdown<F, Fut>(
    set: JoinSet<TaskExit>,
    shutdown: CancellationToken,
    deadline: Instant,
    before_cancel: F,
) where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    drain_with_shutdown_hooks(set, shutdown, deadline, before_cancel, || async { false }).await;
}

/// Drains supervision with ordered hooks immediately before and after cancellation.
///
/// The caller supplies one absolute deadline. Readiness is first-polled before
/// cancellation; the post-cancel hook may request immediate owned-task abort for
/// deterministic test support. Deadline expiry always aborts retained tasks,
/// starts no later await, and does not replace the separately classified first exit.
pub async fn drain_with_shutdown_hooks<F, Fut, C, CFut>(
    mut set: JoinSet<TaskExit>,
    shutdown: CancellationToken,
    deadline: Instant,
    before_cancel: F,
    after_cancel: C,
) where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
    C: FnOnce() -> CFut,
    CFut: std::future::Future<Output = bool>,
{
    match timeout_at(deadline, before_cancel()).await {
        Ok(()) => {}
        Err(_) => tracing::warn!("readiness removal hook exceeded shutdown deadline"),
    }
    shutdown.cancel();
    if Instant::now() < deadline && timeout_at(deadline, after_cancel()).await.unwrap_or(false) {
        set.abort_all();
        return;
    }

    // Phase 2 — drain within budget, then abort.
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::time::{Duration, Instant};

    use tokio::task::JoinSet;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use vala_bifrost_redux::resources::{BifrostResourceHealth, BifrostResourcePoisonReason};

    /// Resource poison becomes the first terminal worker result and cancels siblings.
    #[tokio::test]
    async fn resource_poison_triggers_bounded_terminal_supervision() {
        let health = BifrostResourceHealth::default();
        let poisoner = health.clone();
        let shutdown = CancellationToken::new();
        let sibling_shutdown = shutdown.clone();
        let mut set = JoinSet::new();
        set.spawn(fallible_task(
            TaskId::Worker("bifrost_resource_health"),
            async move { health.wait_for_poison().await },
        ));
        set.spawn(worker_task(TaskId::Worker("sibling"), async move {
            sibling_shutdown.cancelled().await;
        }));
        poisoner.poison(BifrostResourcePoisonReason::Accounting);
        let terminal = supervise(set, shutdown.clone(), Duration::from_millis(100)).await;
        assert!(
            terminal
                .as_deref()
                .is_some_and(|message| message.contains("bifrost_resource_health"))
        );
        assert!(shutdown.is_cancelled());
    }

    /// Proves readiness removal precedes cancellation of an active request.
    #[tokio::test(start_paused = true)]
    async fn shutdown_hook_runs_before_active_request_drain() {
        let shutdown = CancellationToken::new();
        let state = Arc::new(AtomicU8::new(0));
        let mut set: JoinSet<TaskExit> = JoinSet::new();
        set.spawn(worker_task(TaskId::Signal, async {}));

        let request_shutdown = shutdown.clone();
        let request_state = Arc::clone(&state);
        set.spawn(worker_task(TaskId::Worker("active_query"), async move {
            request_shutdown.cancelled().await;
            assert_eq!(
                request_state.load(Ordering::Acquire),
                1,
                "active work must observe readiness removed before cancellation"
            );
            request_state.store(2, Ordering::Release);
        }));

        let hook_state = Arc::clone(&state);
        let hook_shutdown = shutdown.clone();
        let terminal =
            supervise_with_shutdown(set, shutdown, Duration::from_secs(1), move || async move {
                assert!(
                    !hook_shutdown.is_cancelled(),
                    "transport cancellation must follow readiness removal"
                );
                hook_state.store(1, Ordering::Release);
            })
            .await;

        assert!(terminal.is_none());
        assert_eq!(state.load(Ordering::Acquire), 2);
    }

    /// Proves a hung readiness hook cannot consume more than the original shutdown budget.
    #[tokio::test(start_paused = true)]
    async fn shutdown_progresses_when_readiness_hook_never_completes() {
        let shutdown = CancellationToken::new();
        let mut set: JoinSet<TaskExit> = JoinSet::new();
        set.spawn(worker_task(TaskId::Signal, async {}));
        let worker_shutdown = shutdown.clone();
        set.spawn(worker_task(TaskId::Worker("hung_hook_probe"), async move {
            worker_shutdown.cancelled().await;
        }));
        let assertion_shutdown = shutdown.clone();

        let terminal = supervise_with_shutdown(set, shutdown, Duration::from_secs(1), || {
            std::future::pending::<()>()
        })
        .await;

        assert!(terminal.is_none());
        assert!(assertion_shutdown.is_cancelled());
    }

    /// Proves a caller cancellation wins over a concurrently ready worker exit.
    #[tokio::test]
    async fn external_cancellation_is_not_misclassified_as_worker_failure() {
        let shutdown = CancellationToken::new();
        shutdown.cancel();
        let mut set: JoinSet<TaskExit> = JoinSet::new();
        set.spawn(fallible_task(TaskId::Worker("cancelled"), async {
            Err::<(), &'static str>("worker failed after cancellation")
        }));

        let terminal = classify_first_exit_with_shutdown(&mut set, &shutdown).await;
        assert!(
            terminal.is_none(),
            "external cancellation must remain a graceful shutdown cause"
        );
        set.abort_all();
    }

    /// Proves the caller-owned deadline bounds a stalled transport drain and preserves terminal classification.
    #[tokio::test(start_paused = true)]
    async fn caller_deadline_bounds_drain_and_preserves_first_exit() {
        let shutdown = CancellationToken::new();
        let mut set: JoinSet<TaskExit> = JoinSet::new();
        set.spawn(fallible_task(TaskId::Http, async {
            Err::<(), &'static str>("terminal transport failure")
        }));
        set.spawn(worker_task(TaskId::Worker("stalled_transport"), async {
            std::future::pending::<()>().await;
        }));

        let terminal = classify_first_exit(&mut set).await;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        drain_with_shutdown(set, shutdown.clone(), deadline, || async {}).await;

        assert_eq!(tokio::time::Instant::now(), deadline);
        assert!(shutdown.is_cancelled());
        assert_eq!(
            terminal.as_deref(),
            Some("Http failed: terminal transport failure")
        );
    }

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
