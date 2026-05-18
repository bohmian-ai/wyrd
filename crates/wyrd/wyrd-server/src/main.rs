use axum::{Router, routing::get};
use std::net::SocketAddr;

const PORT_ENV: &str = "WYRD_SERVER_PORT";
const DEFAULT_PORT: u16 = 8080;

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var(PORT_ENV)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let app = Router::new().route("/healthz", get(|| async { "ok" }));
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
