//! The Failures guide in Rust: wait for the schedule to fail the current
//! window of `latency-budget` on `checkout-alerts`, then print how each of
//! its Operator dispatches was delivered.

use std::error::Error;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use wyrd_ai::bifrost::{BifrostOptions, QueryParam};
use wyrd_ai::cards::{CardKind, CardSelector, Cards};
use wyrd_ai::verify::Execution;
use wyrd_ai::{Bifrost, ExecutionId};

/// The newest failed window that closes after `$3`, in Unix seconds.
const FAILED_SINCE: &str = "SELECT execution_id FROM bifrost.verification.results \
    WHERE verifier_card_uid = $1 AND subject_card_uid = $2 AND verdict = 'failed' \
    AND to_unixtime(window_end) > $3 \
    ORDER BY window_start DESC LIMIT 1";

/// One failed window's execution.
#[derive(Deserialize)]
struct Failed {
    /// The execution that judged the window.
    execution_id: ExecutionId,
}

/// Wait up to three minutes for the failed window and its settled
/// dispatches, then print them.
///
/// # Errors
/// Returns the client's configuration or credential error, a missing Card,
/// a refused query or status read, or a timeout.
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let since = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
    let deadline = Instant::now() + Duration::from_mins(3);
    let cards = Cards::connect().await?;
    let mut params = Vec::new();
    for (kind, name) in [
        (CardKind::Verifier, "latency-budget"),
        (CardKind::Service, "checkout-alerts"),
    ] {
        let card = cards.get(CardSelector::named(kind, name.parse()?)).await?;
        let uid = card.metadata.uid.ok_or("the Card has no uid")?;
        params.push(QueryParam::String(uid.to_string()));
    }
    params.push(QueryParam::Float(since));
    let bifrost = Bifrost::connect(BifrostOptions::default()).await?;

    // Wait for the schedule to fail the window the requests were recorded in.
    let failed = loop {
        let rows: Vec<Failed> = bifrost.sql_as(FAILED_SINCE, &params).await?;
        if let Some(row) = rows.into_iter().next() {
            break row;
        }
        if Instant::now() > deadline {
            return Err("no failed window was judged".into());
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    };
    let execution = Execution::connect(failed.execution_id).await?;
    println!("verdict: failed");

    // Each Operator run is a dispatch on the execution, delivered on its own.
    loop {
        let statuses: Vec<&str> = execution
            .status()
            .await?
            .dispatches
            .iter()
            .map(|dispatch| dispatch.status.into())
            .collect();
        if statuses
            .iter()
            .all(|s| matches!(*s, "delivered" | "failed"))
        {
            for status in statuses {
                println!("dispatch: {status}");
            }
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err("a dispatch is still pending".into());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
