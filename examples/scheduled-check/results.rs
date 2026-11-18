//! Print the newest three windows the scheduler judged for `latency-budget`
//! on `checkout`, times in UTC.

use std::error::Error;

use serde::Deserialize;
use wyrd_ai::Bifrost;
use wyrd_ai::bifrost::{BifrostOptions, QueryParam};
use wyrd_ai::cards::{CardKind, CardSelector, Cards};

const RECENT: &str = "SELECT to_char(window_start, '%H:%M') AS opened, \
    to_char(window_end, '%H:%M') AS closed, verdict \
    FROM bifrost.verification.results \
    WHERE verifier_card_uid = $1 AND subject_card_uid = $2 \
    ORDER BY window_start DESC LIMIT 3";

/// One judged window.
#[derive(Deserialize)]
struct Window {
    /// When the window opened.
    opened: String,
    /// When it closed and was judged.
    closed: String,
    /// `passed`, `failed`, or `inconclusive`.
    verdict: String,
}

/// Print the newest judged windows.
///
/// # Errors
/// Returns the client's configuration or credential error, a missing Card,
/// or a refused query.
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cards = Cards::connect().await?;
    let mut uids = Vec::new();
    for (kind, name) in [
        (CardKind::Verifier, "latency-budget"),
        (CardKind::Service, "checkout"),
    ] {
        let card = cards.get(CardSelector::named(kind, name.parse()?)).await?;
        let uid = card.metadata.uid.ok_or("the Card has no uid")?;
        uids.push(QueryParam::String(uid.to_string()));
    }
    let bifrost = Bifrost::connect(BifrostOptions::default()).await?;
    let windows: Vec<Window> = bifrost.sql_as(RECENT, &uids).await?;
    println!("latency-budget@1.0.0 on checkout@1.0.0");
    for window in windows {
        println!("  {}-{} {}", window.opened, window.closed, window.verdict);
    }
    Ok(())
}
