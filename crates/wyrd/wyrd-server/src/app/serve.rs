use std::net::SocketAddr;

use axum::Router;
use axum::serve::ListenerExt as _;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

/// Serve `router` on `listener` until `shutdown` is cancelled.
///
/// The router is served with peer connect info so handlers can read the
/// client address; cancellation stops accepting and drains in-flight
/// requests before returning. Every accepted connection disables Nagle's
/// algorithm: a streamed response ends in small writes, and Nagle holds the
/// last one until the client acknowledges the previous, which a delayed ACK
/// stretches to about 40 ms per response.
///
/// # Errors
/// Returns the listener's IO error when accepting connections fails.
pub async fn serve(
    router: Router,
    listener: TcpListener,
    shutdown: CancellationToken,
) -> std::io::Result<()> {
    let listener = listener.tap_io(|stream| {
        if let Err(error) = stream.set_nodelay(true) {
            tracing::warn!(%error, "could not disable Nagle on an accepted connection");
        }
    });
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move { shutdown.cancelled().await })
    .await
}
