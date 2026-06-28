//! Make an authenticated Wyrd request the way an adopter should: build one
//! `WyrdClient` from the environment, then call through it.
//!
//! This is the runtime counterpart to the `transport_*` examples (which only
//! serialize config). `WyrdClient::from_env` assembles the whole stack —
//! endpoint discovery, credential resolution, the shared `AuthMiddleware` token
//! exchange/refresh, and the `HttpTransport` retry loop — so the caller never
//! hand-wires those layers.
//!
//! Requires a running `wyrd dev` server and a credential in the environment
//! (e.g. `WYRD_SERVER_URL` + `WYRD_API_KEY`). Run with:
//!     cargo run -p wyrd-rust-examples --bin client_request

use wyrd_client::WyrdClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Assemble the client from the environment. Under the hood this discovers
    // endpoints (WYRD_SERVER_URL / WYRD_GRPC_URL), resolves a credential from
    // the ADC-style chain (WYRD_ACCESS_TOKEN, WYRD_WORKLOAD_TOKEN + WYRD_TENANT,
    // WYRD_API_KEY, an explicit api_key, or the credentials.toml floor), and
    // builds the shared auth + HTTP layers. No network call happens yet; the
    // first token exchange is lazy.
    let client = WyrdClient::from_env()?;

    // Send a request. `request_json` is JSON-in/JSON-out; the token is injected
    // as `x-wyrd-access-token` and a `wyrd-request-id` is attached per request.
    // The path is appended to the configured base URL. (Illustrative route —
    // swap for the one you need.)
    let response: serde_json::Value = client
        .request_json(
            reqwest::Method::GET,
            "/v1/example",
            None::<&serde_json::Value>,
        )
        .await?;

    println!("{response:#}");
    Ok(())
}
