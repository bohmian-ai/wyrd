#[path = "auth_issue_key_journey.rs"]
mod auth_issue_key_journey;
#[path = "card_lifecycle.rs"]
mod card_lifecycle;
/// Provider-backed `wyrd auth login`, `status`, and `logout` journey.
#[path = "cli_login_journey.rs"]
mod cli_login_journey;
#[path = "eval_local_records.rs"]
mod eval_local_records;
#[path = "eval_support/mod.rs"]
mod eval_support;
#[path = "gateway_server_journey.rs"]
mod gateway_server_journey;
#[path = "loader.rs"]
mod loader;
#[path = "operator_journey.rs"]
mod operator_journey;
#[path = "principal_journey.rs"]
mod principal_journey;
#[path = "query_server_journey.rs"]
mod query_server_journey;
#[path = "secret_sources.rs"]
mod secret_sources;

/// Root of the provider-backed CLI login journey, so its exact name
/// `cli_oidc_handoff_journey` selects it; only the Keycloak identity lane
/// runs it.
///
/// # Panics
/// Panics when any step of [`cli_login_journey::cli_oidc_handoff_journey`]
/// differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the Keycloak identity lane; run via `mise run test:identity:journey`"]
async fn cli_oidc_handoff_journey() {
    cli_login_journey::cli_oidc_handoff_journey().await;
}
