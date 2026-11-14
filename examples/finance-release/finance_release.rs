//! Verify two proposed report releases against the `finance-release`
//! Service that `finance_service.rs` declares.

mod finance_service;

use std::error::Error;

use wyrd_ai::serde_json;
use wyrd_ai::state::WyrdState;

use finance_service::{Destination, Recipient, Release, Report};

/// Declare the finance release, then verify two releases and print each
/// verdict.
///
/// # Errors
/// Returns the ambient client's configuration or credential error, the
/// declaration's refusal, or a refused verification.
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Upload the policy, declare every Card, and get a ready state.
    let state = WyrdState::declare(&finance_service::service()?).await?;
    let run = state.run();

    for text in [
        "Synthetic customer report: quarterly total $120.",
        "Synthetic customer report: paid from BANK-ACCOUNT-DEMO-001.",
    ] {
        let release = Release {
            report: Report { text: text.into() },
            recipient: Recipient {
                email: "auditor@example.test".into(),
                authorized: true,
            },
            destination: Destination {
                url: "https://auditor.example.test/reports".into(),
                restricted: true,
            },
        };
        let result = run
            .observe()
            .verify("release-check", serde_json::to_value(release)?)
            .await?;
        println!("{}: {text}", result.outcome.verdict);
    }
    Ok(())
}
