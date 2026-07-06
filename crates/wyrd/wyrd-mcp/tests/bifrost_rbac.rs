//! Tests verifying that Bifrost tools propagate RBAC errors from the C2 routes.
//!
//! Uses WyrdTestServer (embedded PgFixture — no external DB) to run a real
//! server instance. A service bootstrapped with no roles has no
//! `bifrost_table:read` permission and must receive WYRD_PERMISSION_403_DENIED_RBAC.

use serde_json::json;
use skald_tool::ToolRegistry;
use wyrd_client::{WyrdClient, config::ClientConfig};
use wyrd_mcp::bifrost::register_bifrost_tools;
use wyrd_testing::WyrdTestServer;

fn client_from_bootstrap(base_url: &str, api_key: secrecy::SecretString) -> WyrdClient {
    let mut config = ClientConfig::default();
    config.http.base_url = base_url.to_owned();
    config.api_key = Some(api_key);
    WyrdClient::with_config(config).expect("test client assembles")
}

#[tokio::test(flavor = "multi_thread")]
async fn bifrost_rbac_list_tables_without_permission_returns_permission_denied() {
    let srv = WyrdTestServer::start_bound()
        .await
        .expect("test server starts");
    let base_url = srv
        .base_url()
        .expect("bound server has base url")
        .to_owned();

    let bootstrap = srv
        .bootstrap_agent("bifrost-rbac-list-tables", &[])
        .await
        .expect("agent bootstraps");
    let api_key = bootstrap
        .api_key()
        .expect("agent bootstrap has api key")
        .clone();

    let client = client_from_bootstrap(&base_url, api_key);
    let registry = ToolRegistry::new();
    register_bifrost_tools(&registry, client).expect("tools register");

    let tool = registry
        .resolve("bifrost.list_tables")
        .expect("list_tables registered");
    let err = tool
        .invoke(json!({}))
        .await
        .expect_err("no bifrost_table:read permission — must fail");

    let detail = err.to_string();
    assert!(
        detail.contains("WYRD_PERMISSION_403_DENIED_RBAC"),
        "expected RBAC denial code in error detail, got: {detail}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bifrost_rbac_describe_table_without_permission_returns_permission_denied() {
    let srv = WyrdTestServer::start_bound()
        .await
        .expect("test server starts");
    let base_url = srv
        .base_url()
        .expect("bound server has base url")
        .to_owned();

    let bootstrap = srv
        .bootstrap_agent("bifrost-rbac-describe-table", &[])
        .await
        .expect("agent bootstraps");
    let api_key = bootstrap
        .api_key()
        .expect("agent bootstrap has api key")
        .clone();

    let client = client_from_bootstrap(&base_url, api_key);
    let registry = ToolRegistry::new();
    register_bifrost_tools(&registry, client).expect("tools register");

    let tool = registry
        .resolve("bifrost.describe_table")
        .expect("describe_table registered");
    let err = tool
        .invoke(json!({"namespace": "bifrost", "name": "nonexistent"}))
        .await
        .expect_err("no bifrost_table:read permission — must fail");

    let detail = err.to_string();
    assert!(
        detail.contains("WYRD_PERMISSION_403_DENIED_RBAC"),
        "expected RBAC denial code in error detail, got: {detail}"
    );
}
