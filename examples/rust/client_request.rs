//! Make an authenticated Wyrd request the way an adopter should: build one
//! `WyrdClient` from the ambient configuration, then call through it.
//!
//! This is the runtime counterpart to the `transport_*` examples (which only
//! serialize config). `WyrdClient::from_global` assembles the whole stack —
//! endpoint discovery, credential resolution, the shared `AuthMiddleware` token
//! exchange/refresh, and the `HttpTransport` retry loop — so the caller never
//! hand-wires those layers. Capability handles such as `Cards` take that one
//! client, so every call they make shares its token cache and connection pool.
//!
//! Requires a running `wyrd dev` server and a credential in the environment
//! (e.g. `WYRD_SERVER_URL` + `WYRD_API_KEY`). Run with:
//!     cargo run -p wyrd-rust-examples --bin client_request

use wyrd_sdk::WyrdClient;
use wyrd_sdk::cards::{Cards, ListCardsRequest};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Assemble the ambient client. Under the hood this discovers endpoints
    // (the global config.toml, then WYRD_SERVER_URL / WYRD_GRPC_URL), resolves
    // a credential from the ADC-style chain (WYRD_ACCESS_TOKEN,
    // WYRD_WORKLOAD_TOKEN + WYRD_TENANT, WYRD_API_KEY, an explicit api_key, or
    // the credentials.toml floor), and builds the shared auth + HTTP layers.
    // No network call happens yet; the first token exchange is lazy.
    let client = WyrdClient::from_global()?;

    // Send a request through a capability handle. The token is injected as
    // `x-wyrd-access-token` and a `wyrd-request-id` is attached per request.
    let cards = Cards::with_client(client);
    let listed = cards
        .list(ListCardsRequest {
            kind: None,
            space: None,
            name: None,
            version_range: None,
            status: None,
            filter: None,
            include_prerelease: false,
            limit: Some(10),
            cursor: None,
        })
        .await?;

    println!("{listed:#?}");
    Ok(())
}
