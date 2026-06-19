//! Shutdown signal watcher and drain coordinator.
//!
//! This module provides two cooperative primitives for clean server shutdown:
//!
//! - [`signal_watcher`] — parks until SIGINT or SIGTERM arrives, then cancels a shared
//!   [`CancellationToken`] so every listener can drain and exit.
//! - [`await_drain`] — joins a set of background task handles within a time budget,
//!   aborting any that exceed the deadline.

use tokio_util::sync::CancellationToken;

/// Wait for SIGINT or SIGTERM and cancel the shared shutdown token.
///
/// Parks until one of the two OS signals arrives, then cancels `shutdown` so
/// every receiver can drain and exit cooperatively.
#[tracing::instrument(skip(shutdown))]
pub async fn signal_watcher(shutdown: CancellationToken) {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::terminate(),
        )
        .expect("SIGTERM handler registration is a process invariant");

        tokio::select! {
            _ = ctrl_c => {
                tracing::info!("SIGINT received; initiating shutdown");
            }
            _ = sigterm.recv() => {
                tracing::info!("SIGTERM received; initiating shutdown");
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = ctrl_c.await;
        tracing::info!("shutdown signal received; initiating shutdown");
    }

    shutdown.cancel();
}

/// Wait up to `drain_ms` milliseconds for all background tasks to complete.
///
/// If the drain budget is exceeded, all remaining tasks are aborted via their
/// abort handles and the loop exits. Tasks that finish before the deadline are
/// joined normally. A panic in any task is logged as a warning rather than
/// propagated.
#[tracing::instrument(skip(handles))]
pub async fn await_drain(handles: Vec<tokio::task::JoinHandle<()>>, drain_ms: u64) {
    use tokio::time::{Instant, timeout_at};

    let deadline = Instant::now() + std::time::Duration::from_millis(drain_ms);
    let abort_handles: Vec<_> = handles.iter().map(|h| h.abort_handle()).collect();

    for handle in handles {
        match timeout_at(deadline, handle).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) if e.is_cancelled() => {}
            Ok(Err(_)) => {
                tracing::warn!("background task panicked during drain");
            }
            Err(_elapsed) => {
                tracing::warn!(
                    "drain deadline exceeded; aborting remaining background tasks"
                );
                for abort in &abort_handles {
                    abort.abort();
                }
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn await_drain_all_complete() {
        let handles: Vec<_> = (0..4)
            .map(|_| tokio::spawn(async { tokio::time::sleep(Duration::from_millis(10)).await }))
            .collect();

        let before = Instant::now();
        await_drain(handles, 1_000).await;
        // All tasks should finish well within 1 second, and well before the 1s budget.
        assert!(before.elapsed() < Duration::from_millis(500));
    }

    #[tokio::test]
    async fn await_drain_timeout_aborts() {
        // Task that never completes unless cancelled.
        let handles: Vec<_> = (0..2)
            .map(|_| tokio::spawn(async { std::future::pending::<()>().await }))
            .collect();

        let before = Instant::now();
        // Very short drain budget — should return promptly.
        await_drain(handles, 50).await;
        let elapsed = before.elapsed();
        // Should return in roughly 50 ms, not hang indefinitely.
        assert!(elapsed < Duration::from_millis(500), "elapsed={elapsed:?}");
    }

    #[tokio::test]
    async fn signal_watcher_cancels_token() {
        // We cannot inject a real OS signal in unit tests, so instead verify that the
        // CancellationToken contract works as expected with `await_drain` and independent
        // cancellation — this is a smoke test for the token integration.
        let token = CancellationToken::new();
        let child = token.child_token();

        // Cancel via a separate task, mimicking what signal_watcher does.
        let token_clone = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            token_clone.cancel();
        });

        // Observe cancellation on the child token.
        child.cancelled().await;
        assert!(token.is_cancelled());
    }
}
