use std::net::SocketAddr;

use axum::Router;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

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
