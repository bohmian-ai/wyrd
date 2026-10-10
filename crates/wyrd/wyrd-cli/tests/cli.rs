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
/// `wyrd mcp proxy` and `wyrd mcp install` journeys.
#[path = "mcp_journey.rs"]
mod mcp_journey;
#[path = "operator_journey.rs"]
mod operator_journey;
#[path = "principal_journey.rs"]
mod principal_journey;
#[path = "query_server_journey.rs"]
mod query_server_journey;
#[path = "secret_sources.rs"]
mod secret_sources;
/// `wyrd server install` against a mock release service.
#[path = "server_install.rs"]
mod server_install;
#[path = "workflow_journey.rs"]
mod workflow_journey;

/// Root of the provider-backed CLI login journey, so its exact name
/// `cli_device_login_journey` selects it; only the Keycloak identity lane
/// runs it.
///
/// # Panics
/// Panics when any step of [`cli_login_journey::cli_device_login_journey`]
/// differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the Keycloak identity lane; run via `mise run test:identity:journey`"]
async fn cli_device_login_journey() {
    cli_login_journey::cli_device_login_journey().await;
}

/// Root of the authenticated MCP proxy journey, so its exact name selects it.
///
/// # Panics
/// Panics when any step of
/// [`mcp_journey::mcp_proxy_discovers_and_reads_with_shared_auth`] differs.
#[tokio::test(flavor = "multi_thread")]
async fn mcp_proxy_discovers_and_reads_with_shared_auth() {
    mcp_journey::mcp_proxy_discovers_and_reads_with_shared_auth().await;
}

/// Root of the selective host-installation journey, so its exact name
/// selects it.
///
/// # Panics
/// Panics when any step of
/// [`mcp_journey::mcp_install_changes_only_selected_hosts`] differs.
#[tokio::test(flavor = "multi_thread")]
async fn mcp_install_changes_only_selected_hosts() {
    mcp_journey::mcp_install_changes_only_selected_hosts().await;
}

/// Root of the external-server MCP journey, so its exact name selects it.
///
/// # Panics
/// Panics when any step of
/// [`mcp_journey::mcp_external_server_preserves_global_endpoint`] differs.
#[tokio::test(flavor = "multi_thread")]
async fn mcp_external_server_preserves_global_endpoint() {
    mcp_journey::mcp_external_server_preserves_global_endpoint().await;
}
