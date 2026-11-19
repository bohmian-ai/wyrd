//! A finance API that verifies each report release before it sends the report.
//!
//! It serves the Quickstart's `finance-release` Service, pulled once at deploy
//! time with `wyrd pull --kind Service --name finance-release --version 0.1.0
//! --output-dir .wyrd-bundle`, and listens on `PORT`, by default 8000.

use std::error::Error;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use wyrd_ai::serde_json::{Value, json};
use wyrd_ai::state::WyrdState;
use wyrd_ai::{RequirementOutcome, VerificationResult};

// ANCHOR: handler
/// Verify one release, then release the report only when the verdict passed.
async fn send_report(
    State(state): State<WyrdState>,
    Json(release): Json<Value>,
) -> (StatusCode, Json<Value>) {
    match state.run().observe().verify("release-check", release).await {
        Ok(result) if result.is_passed() => (StatusCode::OK, Json(release_report(&result))),
        Ok(result) => (StatusCode::OK, Json(hold_for_review(&result))),
        // No verdict, such as a refused credential or a timeout: nothing was
        // judged, so nothing is sent.
        Err(error) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "decision": "held for review", "error": error.code() })),
        ),
    }
}
// ANCHOR_END: handler

// ANCHOR: decisions
/// Send the report; a real API sends it to its destination here, only after
/// the verdict.
fn release_report(result: &VerificationResult) -> Value {
    json!({ "decision": "released", "run_id": result.run_id, "execution_id": result.execution_id })
}

/// Hold the report for a person, who sees which requirements did not pass.
/// Failed and inconclusive verdicts both wait.
fn hold_for_review(result: &VerificationResult) -> Value {
    let requirements: wyrd_ai::serde_json::Map<String, Value> = result
        .outcome
        .requirements
        .iter()
        .filter(|requirement| requirement.outcome != RequirementOutcome::Passed)
        .map(|requirement| {
            (
                requirement.id.as_str().to_owned(),
                json!(requirement.outcome),
            )
        })
        .collect();
    json!({
        "decision": "held for review",
        "verdict": result.outcome.verdict,
        "run_id": result.run_id,
        "execution_id": result.execution_id,
        "requirements": requirements,
    })
}
// ANCHOR_END: decisions

/// Load the pulled bundle and serve `POST /reports/send` until stopped.
///
/// # Errors
/// Returns the bundle's load error or the listener's bind error.
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let state = WyrdState::from_path(".wyrd-bundle")?;
    let api = Router::new()
        .route("/reports/send", post(send_report))
        .with_state(state.clone());
    let port = std::env::var("PORT").unwrap_or_else(|_| "8000".to_owned());
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    println!("listening on http://{}", listener.local_addr()?);
    axum::serve(listener, api)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await?;
    // Drain what the state recorded before the process exits.
    state.shutdown().await?;
    Ok(())
}
