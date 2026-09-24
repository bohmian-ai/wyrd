//! Rust SDK Operator connection journey through the public `wyrd_sdk` crate.
//!
//! A tenant administrator creates, lists, reads, rotates, and disables a
//! connection against a real server; a writer without `operators:*` is
//! refused, and another tenant's administrator sees nothing. No response or
//! error ever carries the secret.

use std::fmt::Debug;

use secrecy::ExposeSecret;
use serde_json::json;
use wyrd_sdk::bifrost::client_from_options;
use wyrd_sdk::operator_connections::{
    CreateOperatorConnectionRequest, OperatorConnectionStatus, OperatorConnections,
    UpdateOperatorConnectionRequest,
};
use wyrd_testing::Bootstrap;
use wyrd_testing::server::WyrdTestServer;

/// Secrets the journey sends; none may appear in any response.
const SECRETS: [&str; 2] = ["xoxb-rust-sdk-secret", "xoxb-rust-sdk-rotated"];

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

/// Assert `value`'s debug and JSON forms carry no journey secret.
///
/// # Panics
/// Panics when a secret appears.
fn assert_redacted(value: &impl Debug) {
    let text = format!("{value:?}");
    for secret in SECRETS {
        assert!(!text.contains(secret), "secret leaked: {text}");
    }
}

/// Prove the Rust SDK manages a redacted connection with permission and
/// tenant separation.
///
/// # Panics
/// Panics when any journey step or structured-error expectation fails.
#[tokio::test(flavor = "multi_thread")]
async fn manages_redacted_connections_with_permission_and_tenant_separation() {
    let server = Box::pin(WyrdTestServer::start_bound())
        .await
        .expect("test server starts");
    let base_url = server
        .base_url()
        .expect("bound server has a URL")
        .to_owned();
    let handle = |credential: &str| {
        OperatorConnections::with_client(
            client_from_options(Some(&base_url), Some(credential), None).expect("client builds"),
        )
    };
    let admin = handle(&api_key(
        server
            .bootstrap_service("rust_oc_admin", &["admin"])
            .await
            .expect("admin bootstraps"),
    ));
    let create: CreateOperatorConnectionRequest = serde_json::from_value(json!({
        "provider": "slack", "name": "rust-slack", "workspace_id": "T0001",
        "bot_token": SECRETS[0]
    }))
    .expect("create matches the wire contract");
    assert_redacted(&create);
    let created = admin.create(&create).await.expect("connection creates");
    assert_redacted(&created);
    assert_eq!(created.status, OperatorConnectionStatus::Active);
    let conflict = admin
        .create(&create)
        .await
        .expect_err("a duplicate name is refused");
    assert_eq!(conflict.code(), "WYRD_OPERATOR_409_CONNECTION_CONFLICT");

    let listed = admin.list().await.expect("connections list");
    assert_eq!(listed, vec![created.clone()]);
    let rotate: UpdateOperatorConnectionRequest =
        serde_json::from_value(json!({ "provider": "slack", "bot_token": SECRETS[1] }))
            .expect("update matches the wire contract");
    let rotated = admin
        .update(&created.connection_id, &rotate)
        .await
        .expect("secret rotates");
    assert_redacted(&rotated);
    assert_eq!(rotated.connection_id, created.connection_id);
    let disabled = admin
        .disable(&created.connection_id)
        .await
        .expect("connection disables");
    assert_eq!(disabled.status, OperatorConnectionStatus::Disabled);
    assert_eq!(
        admin
            .get(&created.connection_id)
            .await
            .expect("connection reads")
            .status,
        OperatorConnectionStatus::Disabled
    );

    let writer = handle(&api_key(
        server
            .bootstrap_service("rust_oc_writer", &["writer"])
            .await
            .expect("writer bootstraps"),
    ));
    let denied = writer
        .list()
        .await
        .expect_err("writer lacks operators:read");
    assert_eq!(denied.code(), "WYRD_PERMISSION_403_DENIED_RBAC");

    let other_tenant = server
        .seed_tenant("rust-oc-other")
        .await
        .expect("second tenant seeds");
    let foreign = handle(&api_key(
        server
            .bootstrap_service_in_tenant(other_tenant, "rust_oc_foreign", &["admin"])
            .await
            .expect("foreign admin bootstraps"),
    ));
    assert!(foreign.list().await.expect("foreign list").is_empty());
    let hidden = foreign
        .get(&created.connection_id)
        .await
        .expect_err("another tenant's connection is not visible");
    assert_eq!(hidden.code(), "WYRD_OPERATOR_404_CONNECTION_NOT_FOUND");
}
