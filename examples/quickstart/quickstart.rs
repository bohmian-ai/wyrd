//! The docsite Quickstart in Rust: declare a Service and the Verifier it
//! references, then judge a passing and a failing refund.

use wyrd_ai::WyrdError;
use wyrd_ai::serde_json::json;
use wyrd_ai::state::WyrdState;

/// Declares `support-reply.yaml` from the working directory, verifies two
/// replies against `refund-limit`, and prints each verdict with its reason
/// and the exact subject and Verifier identities.
///
/// # Errors
///
/// Returns the [`WyrdError`] of a refused declaration or verification.
#[tokio::main]
async fn main() -> Result<(), WyrdError> {
    // Declare the Service and the Verifier it references, and get a ready state.
    let state = WyrdState::declare("support-reply.yaml").await?;
    let run = state.run();

    for reply in [json!({ "refund": 40 }), json!({ "refund": 250 })] {
        let result = run.observe().verify("refund-limit", reply).await?;
        let reason = &result.outcome.requirements[0].reason;
        println!("{}: {reason}", result.outcome.verdict);
        println!("  subject:  {}", result.subject);
        println!("  verifier: {}", result.verifier);
    }
    Ok(())
}
