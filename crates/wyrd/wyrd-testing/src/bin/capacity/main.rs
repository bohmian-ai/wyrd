//! Wyrd's opt-in capacity benchmarks, one named workload per first argument.
//!
//! - `query-capacity` ([`query`]): Bifrost reads and writes, through `mise run
//!   bench:bifrost:query-capacity` and `bench:bifrost:write-capacity`.
//! - `client-queue` ([`client_queue`]): the client observation queue, through
//!   `mise run bench:bifrost:client-queue`.
//! - `verification-capacity` ([`verification`]): the verification runtime and
//!   engine, through `mise run bench:verification:capacity`.
//! - `gateway-overhead` and `gateway-capacity` ([`gateway`]): the AI
//!   gateway's server overhead and its sustained open-loop capacity, through
//!   `mise run bench:gateway:overhead` and `bench:gateway:capacity`.
//!
//! Each workload starts its own release server, keeps its own flags, report,
//! and output directory, and exits nonzero when a row fails.

mod client_queue;
mod flags;
mod gateway;
mod query;
mod verification;

use std::process::ExitCode;

/// Runs the workload the first argument names, with that argument removed so
/// each workload reads its own flags.
#[tokio::main]
async fn main() -> ExitCode {
    let mut arguments: Vec<String> = std::env::args().collect();
    let workload = (arguments.len() > 1).then(|| arguments.remove(1));
    match workload.as_deref() {
        Some("query-capacity") => Box::pin(query::main(arguments)).await,
        Some("client-queue") => Box::pin(client_queue::main(arguments)).await,
        Some("verification-capacity") => Box::pin(verification::main(arguments)).await,
        Some("gateway-overhead") => Box::pin(gateway::overhead(arguments)).await,
        Some("gateway-capacity") => Box::pin(gateway::capacity(arguments)).await,
        other => {
            eprintln!(
                "capacity: expected query-capacity, client-queue, verification-capacity, gateway-overhead, or gateway-capacity, got {other:?}"
            );
            ExitCode::FAILURE
        }
    }
}
