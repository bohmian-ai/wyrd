use std::net::SocketAddr;

use axum::Router;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

/// Serve `router` on `listener` until `shutdown` is cancelled.
///
/// The router is served with peer connect info so handlers can read the
/// client address; cancellation stops accepting and drains in-flight
/// requests before returning.
///
/// # Errors
/// Returns the listener's IO error when accepting connections fails.
pub async fn serve(
    router: Router,
    listener: TcpListener,
    shutdown: CancellationToken,
) -> std::io::Result<()> {
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move { shutdown.cancelled().await })
    .await
}
