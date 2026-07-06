//! Tests verifying Bifrost tool registration and static catalog tools.
//!
//! Static tools (list_permissions, list_errors) are invoked without a server —
//! no bearer required, no DB required.

use serde_json::json;
use skald_tool::ToolRegistry;
use wyrd_client::{WyrdClient, config::ClientConfig};
use wyrd_mcp::bifrost::{bifrost_error_catalog, bifrost_permissions, register_bifrost_tools};

fn dummy_client() -> WyrdClient {
    let mut config = ClientConfig::default();
    config.api_key = Some("test_key_placeholder".to_owned().into());
    WyrdClient::with_config(config).expect("test client builds")
}

// ── Registration ─────────────────────────────────────────────────────────────

#[test]
fn bifrost_tools_register_four_tools() {
    let registry = ToolRegistry::new();
    let client = dummy_client();
    register_bifrost_tools(&registry, client).expect("registration succeeds");

    let names = registry.names();
    assert_eq!(names.len(), 4, "expected exactly 4 tools, got {names:?}");
    assert!(names.contains(&"bifrost.list_tables".to_string()));
    assert!(names.contains(&"bifrost.describe_table".to_string()));
    assert!(names.contains(&"bifrost.list_permissions".to_string()));
    assert!(names.contains(&"bifrost.list_errors".to_string()));
}

#[test]
fn bifrost_tools_duplicate_registration_returns_name_taken() {
    let registry = ToolRegistry::new();
    let client = dummy_client();
    register_bifrost_tools(&registry, client.clone()).expect("first registration succeeds");
    let err = register_bifrost_tools(&registry, client).expect_err("duplicate fails");
    let code = err.code();
    assert_eq!(code, "SKALD_TOOL_409_NAME_TAKEN");
}

// ── list_permissions (static, no bearer) ─────────────────────────────────────

#[test]
fn bifrost_tools_list_permissions_returns_five_entries() {
    let perms = bifrost_permissions();
    assert_eq!(
        perms.len(),
        5,
        "expected 5 Bifrost permissions, got {}",
        perms.len()
    );
}

#[test]
fn bifrost_tools_list_permissions_covers_all_expected() {
    let perms = bifrost_permissions();
    let names: Vec<&str> = perms.iter().map(|p| p.permission.as_str()).collect();
    assert!(
        names.contains(&"bifrost_table:read"),
        "missing bifrost_table:read"
    );
    assert!(
        names.contains(&"bifrost_table:write"),
        "missing bifrost_table:write"
    );
    assert!(
        names.contains(&"bifrost_table:install"),
        "missing bifrost_table:install"
    );
    assert!(
        names.contains(&"bifrost_record:write"),
        "missing bifrost_record:write"
    );
    assert!(
        names.contains(&"bifrost_query:read"),
        "missing bifrost_query:read"
    );
    // No bifrost_query:run — BifrostQuery only pairs with Read; nothing enforces
    // a `run` action, so projecting it would be a second source of truth.
    assert!(
        !names.contains(&"bifrost_query:run"),
        "bifrost_query:run is not an enforced permission"
    );
}

#[test]
fn bifrost_tools_list_permissions_invoke_returns_array() {
    wyrd_runtime::runtime().block_on(async {
        let registry = ToolRegistry::new();
        let client = dummy_client();
        register_bifrost_tools(&registry, client).expect("registration succeeds");
        let tool = registry
            .resolve("bifrost.list_permissions")
            .expect("tool is registered");
        let result = tool.invoke(json!({})).await.expect("invoke succeeds");
        let arr = result.as_array().expect("output is JSON array");
        assert_eq!(arr.len(), 5);
    });
}

// ── list_errors (static, no bearer) ──────────────────────────────────────────

#[test]
fn bifrost_tools_list_errors_covers_c1_c2_c5_codes() {
    let catalog = bifrost_error_catalog();
    let codes: Vec<&str> = catalog.iter().map(|e| e.code.as_str()).collect();

    // C5: WYRD_VALA_500_AUDIT_UNAVAILABLE must be present.
    assert!(
        codes.contains(&"WYRD_VALA_500_AUDIT_UNAVAILABLE"),
        "C5 code missing from error catalog"
    );
    // A representative C2 code.
    assert!(
        codes.contains(&"WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND"),
        "C2 code missing from error catalog"
    );
    // A representative C1 code (schema fingerprint).
    assert!(
        codes.contains(&"WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH"),
        "C1 code missing from error catalog"
    );
}

#[test]
fn bifrost_tools_list_errors_all_entries_have_nonempty_fields() {
    for entry in bifrost_error_catalog() {
        assert!(!entry.code.is_empty(), "code must not be empty");
        assert!(entry.status > 0, "status must be nonzero");
        assert!(!entry.title.is_empty(), "title must not be empty");
        assert!(
            !entry.remediation.is_empty(),
            "remediation must not be empty"
        );
    }
}

#[test]
fn bifrost_tools_list_errors_invoke_returns_array() {
    wyrd_runtime::runtime().block_on(async {
        let registry = ToolRegistry::new();
        let client = dummy_client();
        register_bifrost_tools(&registry, client).expect("registration succeeds");
        let tool = registry
            .resolve("bifrost.list_errors")
            .expect("tool is registered");
        let result = tool.invoke(json!({})).await.expect("invoke succeeds");
        let arr = result.as_array().expect("output is JSON array");
        assert!(!arr.is_empty(), "error catalog must not be empty");
    });
}
