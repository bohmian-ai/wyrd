//! Shutdown signal watcher.
//!
//! - [`signal_watcher`] — parks until SIGINT or SIGTERM arrives (or the token
//!   is already cancelled), then cancels a shared [`CancellationToken`].

use tokio_util::sync::CancellationToken;

/// Wait for SIGINT or SIGTERM and cancel the shared shutdown token.
///
/// Parks until one of the two OS signals arrives **or** `shutdown` is already
/// cancelled (e.g. by a failing transport), then cancels the token and returns.
/// The cancellation-aware arm lets the supervisor drain this task immediately
/// on failure-triggered shutdown rather than waiting out the full drain budget.
#[tracing::instrument(skip(shutdown))]
pub async fn signal_watcher(shutdown: CancellationToken) {
    tokio::select! {
        () = wait_for_os_signal() => {
            shutdown.cancel();
        }
        () = shutdown.cancelled() => {}
    }
}

async fn wait_for_os_signal() {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn signal_watcher_returns_on_cancel() {
        let token = CancellationToken::new();
        let token_clone = token.clone();

        let handle = tokio::spawn(signal_watcher(token_clone));

        // Cancel the token — signal_watcher should return promptly.
        token.cancel();

        tokio::time::timeout(std::time::Duration::from_millis(200), handle)
            .await
            .expect("signal_watcher did not return within 200ms after cancel")
            .expect("signal_watcher task panicked");
    }
}
