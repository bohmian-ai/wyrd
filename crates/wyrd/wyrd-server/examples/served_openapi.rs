//! Write the `OpenAPI` document a freshly composed Wyrd server serves.
//!
//! `mise run codegen:regen` runs this inside a repository-managed Postgres
//! lifecycle, writing to the ephemeral path given as the only argument, and
//! feeds that file to the UI's HTTP contract generator. A path rather than
//! stdout keeps the harness's log output out of the document. The
//! document is fetched from `GET /openapi.json` on the same composed router
//! production serves, so generation reads no second route list and no checked-in
//! export; the output is ephemeral.

use std::error::Error as StdError;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use wyrd_testing::WyrdTestServer;

/// Start an in-process server, fetch its served document, and write it to the path argument.
///
/// The server is shut down before returning so its fixture database is
/// released whether or not the fetch succeeded.
///
/// # Errors
/// Returns an error when the path argument is missing, the server cannot
/// start, the document is not served as `200`, its body cannot be collected,
/// the file cannot be written, or shutdown fails.
#[tokio::main]
async fn main() -> Result<(), Box<dyn StdError>> {
    let out = std::env::args()
        .nth(1)
        .ok_or("usage: served_openapi <output.json>")?;
    let server = WyrdTestServer::start_in_process().await?;
    let served = fetch_document(&server).await;
    server.shutdown().await?;
    std::fs::write(out, served?)?;
    Ok(())
}

/// Fetch the raw `GET /openapi.json` body through the in-process router.
///
/// # Errors
/// Returns an error when the request fails, the status is not `200`, or the
/// body cannot be collected.
async fn fetch_document(server: &WyrdTestServer) -> Result<Vec<u8>, Box<dyn StdError>> {
    let response = server
        .oneshot(
            Request::builder()
                .uri("/openapi.json")
                .body(Body::empty())?,
        )
        .await?;
    if response.status() != StatusCode::OK {
        return Err(format!("/openapi.json answered {}", response.status()).into());
    }
    Ok(to_bytes(response.into_body(), usize::MAX).await?.to_vec())
}
