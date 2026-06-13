use std::net::SocketAddr;

const PORT_ENV: &str = "WYRD_SERVER_PORT";
const DEFAULT_PORT: u16 = 8080;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    enterprise_on_start();

    let state = wyrd_server::build_app_state().await?;
    let port: u16 = std::env::var(PORT_ENV)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let app = wyrd_server::router(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(feature = "enterprise")]
fn enterprise_on_start() {
    wyrd_enterprise::on_server_start();
}

#[cfg(not(feature = "enterprise"))]
fn enterprise_on_start() {}
