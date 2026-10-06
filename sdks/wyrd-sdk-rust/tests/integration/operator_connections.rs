//! Rust SDK Operator connection journey through the public `wyrd_sdk` crate.
//!
//! A tenant administrator creates Slack, `PagerDuty`, and HTTP connections,
//! lists and reads them, rotates each secret, and disables and re-enables one
//! against a real server; a writer without `operators:*` is refused, and
//! another tenant's administrator can neither see nor mutate them. No
//! response or error ever carries a secret.

use std::fmt::Debug;

use secrecy::ExposeSecret;
use serde_json::json;
use wyrd_sdk::bifrost::client_from_options;
use wyrd_sdk::operator_connections::{
    CreateOperatorConnectionRequest, OperatorConnectionId, OperatorConnectionStatus,
    OperatorConnectionView, OperatorConnections, UpdateOperatorConnectionRequest,
};
use wyrd_testing::Bootstrap;
use wyrd_testing::server::WyrdTestServer;

/// Secrets the journey sends; none may appear in any response.
const SECRETS: [&str; 6] = [
    "xoxb-rust-sdk-secret",
    "xoxb-rust-sdk-rotated",
    "pd-rust-sdk-secret",
    "pd-rust-sdk-rotated",
    "http-rust-sdk-secret",
    "http-rust-sdk-rotated",
];

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

/// Create a `PagerDuty` and an HTTP connection as `admin`, rotate each
/// secret, and return the rotated redacted views in that order.
///
/// The HTTP rotation also switches its scheme from bearer to a custom
/// header. Each rotation keeps the connection identity, and a read returns
/// exactly the rotated view.
///
/// # Panics
/// Panics when a request does not match the wire contract, a create, rotate,
/// or read fails, or any request or view carries a journey secret.
async fn create_and_rotate_pager_duty_and_http(
    admin: &OperatorConnections,
) -> Vec<OperatorConnectionView> {
    let mut others = Vec::new();
    for (create, rotate) in [
        (
            json!({ "provider": "pager_duty", "name": "rust-pagerduty",
                    "integration_key": SECRETS[2] }),
            json!({ "provider": "pager_duty", "integration_key": SECRETS[3] }),
        ),
        (
            json!({ "provider": "http", "name": "rust-http",
                    "origin": "https://hooks.example.com",
                    "auth": { "scheme": "bearer", "token": SECRETS[4] } }),
            json!({ "provider": "http",
                    "auth": { "scheme": "header", "name": "X-Api-Key", "value": SECRETS[5] } }),
        ),
    ] {
        let create: CreateOperatorConnectionRequest =
            serde_json::from_value(create).expect("create matches the wire contract");
        assert_redacted(&create);
        let view = admin.create(&create).await.expect("connection creates");
        assert_redacted(&view);
        assert_eq!(view.status, OperatorConnectionStatus::Active);
        let rotate: UpdateOperatorConnectionRequest =
            serde_json::from_value(rotate).expect("update matches the wire contract");
        assert_redacted(&rotate);
        let rotated = admin
            .update(&view.connection_id, &rotate)
            .await
            .expect("secret rotates");
        assert_redacted(&rotated);
        assert_eq!(rotated.connection_id, view.connection_id);
        assert_eq!(
            admin
                .get(&view.connection_id)
                .await
                .expect("rotated connection reads"),
            rotated
        );
        others.push(rotated);
    }
    others
}

/// Prove `foreign`, another tenant's administrator, cannot see, rotate, or
/// disable `connection`, and that `admin` still reads it active.
///
/// # Panics
/// Panics when any foreign call succeeds or refuses with another code, a
/// refusal carries a secret, or the connection is no longer active.
async fn assert_foreign_tenant_refused(
    foreign: &OperatorConnections,
    admin: &OperatorConnections,
    connection: &OperatorConnectionId,
    rotate: &UpdateOperatorConnectionRequest,
) {
    assert!(foreign.list().await.expect("foreign list").is_empty());
    let hidden = foreign
        .get(connection)
        .await
        .expect_err("another tenant's connection is not visible");
    assert_eq!(hidden.code(), "WYRD_OPERATOR_404_CONNECTION_NOT_FOUND");
    let foreign_rotate = foreign
        .update(connection, rotate)
        .await
        .expect_err("another tenant's connection cannot be rotated");
    assert_redacted(&foreign_rotate);
    assert_eq!(
        foreign_rotate.code(),
        "WYRD_OPERATOR_404_CONNECTION_NOT_FOUND"
    );
    let foreign_disable = foreign
        .disable(connection)
        .await
        .expect_err("another tenant's connection cannot be disabled");
    assert_eq!(
        foreign_disable.code(),
        "WYRD_OPERATOR_404_CONNECTION_NOT_FOUND"
    );
    assert_eq!(
        admin
            .get(connection)
            .await
            .expect("connection still reads")
            .status,
        OperatorConnectionStatus::Active,
        "a foreign disable attempt leaves the connection untouched"
    );
}

/// Prove the Rust SDK manages redacted Slack, `PagerDuty`, and HTTP connections
/// with permission and tenant separation.
///
/// Every provider is created, rotated without changing its identity, and read
/// back redacted; the Slack connection is disabled and re-enabled through the
/// status patch. A foreign tenant's administrator gets the stable not-found
/// refusal on read, rotate, and disable.
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
            client_from_options(Some(&base_url), Some(credential), None, None)
                .expect("client builds"),
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

    let others = create_and_rotate_pager_duty_and_http(&admin).await;
    let http = others.last().expect("HTTP connection was created");
    assert_eq!(
        serde_json::to_value(http).expect("view serializes")["auth"],
        json!({ "scheme": "header", "name": "X-Api-Key" }),
        "rotation may change the HTTP scheme; only its nonsecret authority is read back"
    );

    let listed = admin.list().await.expect("connections list");
    assert_redacted(&listed);
    assert_eq!(listed.len(), 3, "{listed:?}");
    assert!(listed.contains(&created));
    for other in &others {
        assert!(listed.contains(other), "{listed:?}");
    }
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
    let enable: UpdateOperatorConnectionRequest =
        serde_json::from_value(json!({ "provider": "slack", "status": "active" }))
            .expect("re-enable matches the wire contract");
    let enabled = admin
        .update(&created.connection_id, &enable)
        .await
        .expect("connection re-enables");
    assert_eq!(enabled.status, OperatorConnectionStatus::Active);
    assert_eq!(enabled.connection_id, created.connection_id);

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
    assert_foreign_tenant_refused(&foreign, &admin, &created.connection_id, &rotate).await;
}
