//! Find the newest failed `refund-workflow` result and read what its
//! execution judged.

use std::error::Error;
use std::time::Duration;

use serde::Deserialize;
use wyrd_ai::Bifrost;
use wyrd_ai::bifrost::{BifrostOptions, QueryParam};
use wyrd_ai::cards::{CardKind, CardSelector, Cards};
use wyrd_ai::serde_json::json;
use wyrd_ai::verify::Execution;

/// The newest failed result judged by one Verifier, by its Card uid.
const FAILED: &str = "SELECT execution_id FROM bifrost.verification.results \
    WHERE verifier_card_uid = $1 AND verdict = 'failed' ORDER BY ended_at DESC LIMIT 1";

/// One stored result row.
#[derive(Deserialize)]
struct Failed {
    /// The execution that produced the result.
    execution_id: String,
}

/// Print the failed requirements of the newest failed result.
///
/// # Errors
/// Returns the client's configuration or credential error, a refused read,
/// or `no failed refund-workflow result yet` after a minute without one.
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let bifrost = Bifrost::connect(BifrostOptions::default()).await?;

    // ANCHOR: find
    // Results are stored shortly after a check: look for up to a minute.
    let verifier = Cards::connect()
        .await?
        .get(CardSelector::named(
            CardKind::Verifier,
            "refund-workflow".parse()?,
        ))
        .await?;
    let uid = verifier
        .metadata
        .uid
        .ok_or("the Verifier has no uid")?
        .to_string();
    let mut execution_id = None;
    for _ in 0..60 {
        let rows: Vec<Failed> = bifrost
            .sql_as(FAILED, &[QueryParam::String(uid.clone())])
            .await?;
        if let Some(row) = rows.into_iter().next() {
            execution_id = Some(row.execution_id);
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let execution_id = execution_id.ok_or("no failed refund-workflow result yet")?;
    // ANCHOR_END: find

    // ANCHOR: details
    // One read: the exact versions judged, each check as written, and what it found:
    // here, the names of the spans in the trace, sorted.
    let details = json!(
        Execution::connect(execution_id.parse()?)
            .await?
            .details()
            .await?
    );
    let (judged, subject) = (&details["verifier"], &details["subject"]);
    println!(
        "{}@{} judged {}@{}: {}",
        judged["name"].as_str().unwrap_or_default(),
        judged["version"].as_str().unwrap_or_default(),
        subject["name"].as_str().unwrap_or_default(),
        subject["version"].as_str().unwrap_or_default(),
        details["outcome"]["verdict"].as_str().unwrap_or_default(),
    );
    for requirement in details["outcome"]["requirements"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if requirement["outcome"] != "passed" {
            println!(
                "  {}: {} · {}",
                requirement["id"].as_str().unwrap_or_default(),
                requirement["outcome"].as_str().unwrap_or_default(),
                requirement["reason"].as_str().unwrap_or_default(),
            );
            println!("    expected: {}", requirement["check"]["expected"]);
            let mut found: Vec<&str> = requirement["actual"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|name| name.as_str())
                .collect();
            found.sort_unstable();
            println!("    found:    {}", json!(found));
        }
    }
    // ANCHOR_END: details
    Ok(())
}
