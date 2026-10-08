//! Point a Wyrd client's HTTP plane at an explicit server.
//!
//! The server settings are a field of the `ClientConfig` that
//! `WyrdClient::with_config` takes. Building the config makes no network
//! call; validation refuses a remote cleartext `http://` URL.
//!
//! Run with:
//!     cargo run -p wyrd-rust-examples --bin transport_http

use wyrd_sdk::config::ClientConfig;
use wyrd_sdk::transport::HttpConfig;

fn main() -> anyhow::Result<()> {
    let http = HttpConfig {
        base_url: "https://wyrd.example.com".to_string(),
        timeout_ms: 30_000,
        compression: true,
    };
    http.validate()?;

    // Hand this to `WyrdClient::with_config` once a credential is set.
    let config = ClientConfig {
        http,
        ..ClientConfig::default()
    };
    println!("server URL: {}", config.http.base_url);
    Ok(())
}
