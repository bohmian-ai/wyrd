//! Rust SDK manual verification journey through the public `wyrd_sdk` crate.
//!
//! Registers a ready Custom Drift Verifier and a Service bound to it, reads the
//! served binding ID through Cards, then acts as the Service itself: reads the
//! binding status, starts a keyed manual run, replays it, and reads the run
//! back. Negative flows cover a reused key with a different body, an invalid
//! window, and a credential without `evals:run`.

use std::path::{Path, PathBuf};

use secrecy::ExposeSecret;
use serde_json::json;
use wyrd_sdk::bifrost::client_from_options;
use wyrd_sdk::cards::{CardSelector, Cards};
use wyrd_sdk::verification::{StartVerificationRunRequest, Verification, VerifierReadiness};
use wyrd_testing::Bootstrap;
use wyrd_testing::server::WyrdTestServer;

/// Write a ready Custom Drift Verifier and a Service bound to it.
///
/// Returns the Verifier and Service paths in registration order.
///
/// # Panics
/// Panics when a fixture file cannot be written.
fn write_bound_service(root: &Path) -> (PathBuf, PathBuf) {
    let verifier = root.join("verifier.yaml");
    std::fs::write(
        &verifier,
        "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: rust-run-drift\n  version: 1.0.0\n  space: default\nspec:\n  implementation:\n    kind: drift\n    spec:\n      method: Custom\n      signal:\n        kind: Metric\n        name: score\n      condition:\n        kind: Statistical\n      profile:\n        kind: Custom\n        metric_name: score\n        baseline_value: 1.0\n        alert_threshold: 0.5\n",
    )
    .expect("verifier card writes");
    let service = root.join("service.yaml");
    std::fs::write(
        &service,
        "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: rust-run-service\n  version: 1.0.0\n  space: default\nspec:\n  verified_by:\n    - verifier:\n        kind: Verifier\n        name: rust-run-drift\n        version: 1.0.0\n        space: default\n      runs_on:\n        kind: schedule\n        cron: \"0 2 * * *\"\n",
    )
    .expect("service card writes");
    (verifier, service)
}

/// Unwrap a machine bootstrap into its raw API key.
///
/// # Panics
/// Panics when the bootstrap is a user principal.
fn api_key(bootstrap: Bootstrap) -> String {
    match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key.expose_secret().to_owned(),
        Bootstrap::User { .. } => panic!("expected a machine principal"),
    }
}

/// Build a manual run request for `binding_id` over a window ending at `end`.
///
/// # Panics
/// Panics when the JSON fixture does not match the wire contract.
fn run_request(binding_id: &str, end: &str) -> StartVerificationRunRequest {
    serde_json::from_value(json!({
        "target": { "kind": "binding", "binding_id": binding_id },
        "input": { "kind": "drift_window", "start": "2026-09-17T00:00:00Z", "end": end },
    }))
    .expect("run request matches the wire contract")
}

/// Prove the Rust SDK reads a binding, starts a keyed run, and reads it back.
///
/// # Panics
/// Panics when any journey step or structured-error expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn starts_a_keyed_manual_run_and_reads_its_status() {
    let root = tempfile::tempdir().expect("fixture root creates");
    let (verifier, service) = write_bound_service(root.path());
    let server = Box::pin(WyrdTestServer::start_bound())
        .await
        .expect("test server starts");
    let base_url = server
        .base_url()
        .expect("bound server has a URL")
        .to_owned();
    let client = |credential: &str| {
        client_from_options(Some(&base_url), Some(credential), None).expect("client builds")
    };
    let admin = api_key(
        server
            .bootstrap_service("rust_run_admin", &["admin"])
            .await
            .expect("admin bootstraps"),
    );
    let cards = Cards::with_client(client(&admin));
    Box::pin(cards.register_from_path(&verifier))
        .await
        .expect("verifier registers");
    let receipt = Box::pin(cards.register_from_path(&service))
        .await
        .expect("service registers");
    let binding_id = cards
        .get(CardSelector::exact(receipt.root.clone()))
        .await
        .expect("service reads")
        .status
        .and_then(|status| status.verification)
        .expect("a binding owner serves verification status")
        .binding_ids[0];

    let writer = Verification::with_client(client(&api_key(
        server
            .credential_registered_service(&receipt.root, &["writer"])
            .await
            .expect("service credential issues"),
    )));
    let binding = writer
        .get_binding(&binding_id)
        .await
        .expect("binding reads");
    assert_eq!(binding.binding_id, binding_id);
    assert_eq!(Some(&binding.subject_card_uid), receipt.root.uid.as_ref());
    assert_eq!(binding.readiness, VerifierReadiness::Ready);

    let request = run_request(&binding_id.to_string(), "2026-09-17T01:00:00Z");
    let run_id = writer
        .start_run(&request, Some("rust-journey-0001"))
        .await
        .expect("run starts");
    let replay = writer
        .start_run(&request, Some("rust-journey-0001"))
        .await
        .expect("keyed retry replays");
    assert_eq!(replay, run_id, "a keyed retry returns the same run");
    let run = writer.get_run(&run_id).await.expect("run reads");
    assert_eq!(run.run_id, run_id);
    assert!(run.requested_by_principal_id.is_some());
    let reused = run_request(&binding_id.to_string(), "2026-09-17T02:00:00Z");
    let conflict = writer
        .start_run(&reused, Some("rust-journey-0001"))
        .await
        .expect_err("a reused key with a different body is refused");
    assert_eq!(conflict.code(), "WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT");
    let inverted = run_request(&binding_id.to_string(), "2026-09-16T00:00:00Z");
    let invalid = writer
        .start_run(&inverted, None)
        .await
        .expect_err("an inverted window is refused");
    assert_eq!(
        invalid.code(),
        "WYRD_VERIFICATION_400_INVALID_WINDOW",
        "{invalid:?}"
    );

    let reader = Verification::with_client(client(&api_key(
        server
            .bootstrap_service("rust_run_reader", &["reader"])
            .await
            .expect("reader bootstraps"),
    )));
    let denied = reader
        .start_run(&request, None)
        .await
        .expect_err("a caller without evals:run is refused");
    assert_eq!(denied.status(), 403);

    server.shutdown().await.expect("test server shuts down");
}
