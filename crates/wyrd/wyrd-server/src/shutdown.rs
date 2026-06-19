//! Shutdown signal watcher and drain coordinator.
//!
//! This module provides two cooperative primitives for clean server shutdown:
//!
//! - [`signal_watcher`] — parks until SIGINT or SIGTERM arrives, then cancels a shared
//!   [`CancellationToken`] so every listener can drain and exit.
//! - [`await_drain`] — drains background workers within a time budget, processes an
//!   already-joined gRPC result, and cleans up the signal watcher handle.

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
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
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

/// Wait up to `drain` for all background workers to complete, then abort any
/// remaining tasks and clean up the signal watcher handle.
///
/// `grpc_result` is the already-joined result of the gRPC serve task when the
/// gRPC select! arm fired first. When another arm fired, pass `None` — the gRPC
/// task is detached and will wind down via the cancellation token it holds.
///
/// Any terminal errors accumulated in the select! loop are preserved; new errors
/// from the gRPC result or panicking workers are recorded only if `terminal_error`
/// is still `None`.
#[tracing::instrument(skip(grpc_result, workers, signal_handle, terminal_error))]
pub async fn await_drain(
    grpc_result: Option<Result<Result<(), crate::grpc::GrpcError>, tokio::task::JoinError>>,
    mut workers: tokio::task::JoinSet<()>,
    signal_handle: tokio::task::JoinHandle<()>,
    drain: std::time::Duration,
    terminal_error: &mut Option<Box<dyn std::error::Error + Send + Sync>>,
) {
    use tokio::time::{Instant, timeout_at};

    if let Some(result) = grpc_result {
        capture_grpc_drain_result(&result, terminal_error);
    }

    let deadline = Instant::now() + drain;
    loop {
        match timeout_at(deadline, workers.join_next()).await {
            Ok(Some(Ok(()))) => {}
            Ok(Some(Err(e))) if e.is_cancelled() => {}
            Ok(Some(Err(_))) => {
                tracing::warn!("background task panicked during drain");
            }
            Ok(None) => break,
            Err(_elapsed) => {
                tracing::warn!("drain deadline exceeded; aborting remaining background workers");
                workers.abort_all();
                break;
            }
        }
    }

    signal_handle.abort();
    let _ = signal_handle.await;
}

fn capture_grpc_drain_result(
    result: &Result<Result<(), crate::grpc::GrpcError>, tokio::task::JoinError>,
    terminal_error: &mut Option<Box<dyn std::error::Error + Send + Sync>>,
) {
    match result {
        Ok(Ok(())) => {}
        Ok(Err(grpc_error)) => {
            terminal_error.get_or_insert_with(|| {
                Box::<dyn std::error::Error + Send + Sync>::from(format!(
                    "gRPC serve failed: {grpc_error}"
                ))
            });
        }
        Err(join_error) => {
            terminal_error.get_or_insert_with(|| {
                Box::<dyn std::error::Error + Send + Sync>::from(format!(
                    "gRPC serve task aborted: {join_error}"
                ))
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn await_drain_all_complete() {
        let mut workers = tokio::task::JoinSet::new();
        for _ in 0..4 {
            workers.spawn(async { tokio::time::sleep(Duration::from_millis(10)).await });
        }
        let signal_handle = tokio::spawn(std::future::pending::<()>());

        let before = Instant::now();
        let mut terminal_error = None;
        await_drain(
            None,
            workers,
            signal_handle,
            Duration::from_secs(1),
            &mut terminal_error,
        )
        .await;
        assert!(terminal_error.is_none());
        assert!(before.elapsed() < Duration::from_millis(500));
    }

    #[tokio::test]
    async fn await_drain_timeout_aborts() {
        let mut workers = tokio::task::JoinSet::new();
        for _ in 0..2 {
            workers.spawn(async { std::future::pending::<()>().await });
        }
        let signal_handle = tokio::spawn(std::future::pending::<()>());

        let before = Instant::now();
        let mut terminal_error = None;
        await_drain(
            None,
            workers,
            signal_handle,
            Duration::from_millis(50),
            &mut terminal_error,
        )
        .await;
        let elapsed = before.elapsed();
        assert!(elapsed < Duration::from_millis(500), "elapsed={elapsed:?}");
    }

    #[tokio::test]
    async fn signal_watcher_cancels_token() {
        let token = CancellationToken::new();
        let child = token.child_token();

        let token_clone = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            token_clone.cancel();
        });

        child.cancelled().await;
        assert!(token.is_cancelled());
    }
}
