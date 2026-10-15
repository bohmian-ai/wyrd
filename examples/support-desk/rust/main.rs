//! Deploy and run the support desk against the server the ambient
//! configuration names (for example `WYRD_SERVER_URL` and `WYRD_API_KEY`):
//!     cargo run -p wyrd-rust-examples --bin support_desk

/// The support-desk workflow this binary runs.
mod support_desk;

use std::time::Duration;

use wyrd_ai::observe::VerificationExecution;
use wyrd_ai::{VerificationVerdict, WyrdClient};

/// Deploys the support desk, judges one refund promise immediately, serves
/// its requests, waits for every background policy check, and prints the
/// requirement breakdown of a failing answer and the joined evidence of one
/// passing and one failing request.
///
/// # Errors
///
/// Returns [`support_desk::Error`] when the ambient client configuration is
/// missing or invalid, the temporary bundle directory cannot be created, or
/// any deploy, serve, check, wait, or explain call to the server fails.
#[tokio::main]
async fn main() -> Result<(), support_desk::Error> {
    let client = WyrdClient::from_global()?;
    let bundle = tempfile::tempdir()?;
    let desk = support_desk::deploy(&client, &bundle.path().join("bundle")).await?;
    println!("deployed support-agent {}", desk.uid("support-agent"));
    let promise = support_desk::check(&desk, "You have a guaranteed refund.").await?;
    println!("immediate check: {:?}", promise.outcome.verdict);
    let served = support_desk::serve(&desk).await?;
    let results = support_desk::wait_for_verdicts(&client, &served, Duration::from_mins(5)).await?;
    let failed = served
        .iter()
        .zip(&results)
        .find(|(_, result)| result.outcome.verdict == VerificationVerdict::Failed);
    if let Some((request, _)) = failed {
        // Any process can resume a check from its ID with the ambient client.
        let resumed = VerificationExecution::connect(request.execution_id)
            .await?
            .wait(Duration::from_secs(30))
            .await?;
        for requirement in &resumed.outcome.requirements {
            println!(
                "{}: {:?} ({})",
                requirement.id.as_str(),
                requirement.outcome,
                requirement.reason
            );
        }
    }
    for verdict in [VerificationVerdict::Passed, VerificationVerdict::Failed] {
        if let Some((request, _)) = served
            .iter()
            .zip(&results)
            .find(|(_, result)| result.outcome.verdict == verdict)
        {
            println!(
                "{:#?}",
                support_desk::explain(&client, &request.run_id).await?
            );
        }
    }
    Ok(())
}
