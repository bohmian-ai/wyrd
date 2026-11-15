//! Verify four proposed report releases against the `finance-release`
//! Service that `finance_service.rs` declares, and print each verdict.

mod finance_service;

use std::error::Error;

use wyrd_ai::serde_json;
use wyrd_ai::state::WyrdState;
use wyrd_ai::{RequirementOutcome, VerificationResult};

use finance_service::{Destination, Recipient, Release, Report};

/// Verify one release against `release-check` in a new Run.
///
/// # Errors
/// Returns the server's refusal of the check, such as a missing permission.
async fn verify_release(
    state: &WyrdState,
    release: Release,
) -> Result<VerificationResult, Box<dyn Error>> {
    let input = serde_json::to_value(release)?;
    Ok(state.run().observe().verify("release-check", input).await?)
}

/// Declare the finance release, then verify four releases and print each
/// verdict with the reason for every requirement that did not pass.
///
/// # Errors
/// Returns the ambient client's configuration or credential error, the
/// declaration's refusal, or a refused check.
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Upload the policy, declare every Card, and get a ready state.
    let state = WyrdState::declare(&finance_service::service()?).await?;
    for (purpose, text) in [
        (
            "external audit",
            "Synthetic customer report: quarterly total $120.",
        ),
        (
            "external audit",
            "Synthetic customer report: paid from BANK-ACCOUNT-DEMO-001.",
        ),
        (
            "marketing partnership",
            "Synthetic customer report: quarterly total $120.",
        ),
        (
            "regulator request",
            "Synthetic customer report: quarterly total $120.",
        ),
    ] {
        let release = Release {
            purpose: purpose.into(),
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
        let result = verify_release(&state, release).await?;
        println!("{purpose} · {text} → {}", result.outcome.verdict);
        for requirement in &result.outcome.requirements {
            if requirement.outcome != RequirementOutcome::Passed {
                println!("  {}: {}", requirement.id.as_str(), requirement.reason);
            }
        }
    }
    Ok(())
}
