//! A tenant administrator keeps the connections its Operators deliver
//! through; no view or refusal ever carries a connection secret.

use std::fmt::Debug;

use wyrd_sdk::operator_connections::{
    ConnectionName, CreateOperatorConnectionRequest, HttpAuthScheme, HttpConnectionAuth,
    HttpsOrigin, OperatorConnectionConfig, OperatorConnectionId, OperatorConnectionStatus,
    OperatorConnections, SecretBearer, UpdateOperatorConnectionRequest,
};

use crate::support::Deployment;

/// Every secret the stories write; none may appear in a view or an error.
const SECRETS: [&str; 3] = [
    "xoxb-rust-sdk-secret",
    "xoxb-rust-sdk-rotated",
    "hook-rust-sdk-secret",
];

/// Fail when a story secret appears in `value`'s debug form.
///
/// # Panics
/// Panics when a secret appears.
fn assert_redacted(value: &impl Debug) {
    let text = format!("{value:?}");
    for secret in SECRETS {
        assert!(!text.contains(secret), "secret leaked: {text}");
    }
}

/// The `ops-slack` connection the stories share.
///
/// # Panics
/// Panics when the fixed name is invalid.
fn slack() -> CreateOperatorConnectionRequest {
    CreateOperatorConnectionRequest::Slack {
        name: ConnectionName::new("ops-slack").expect("connection name"),
        workspace_id: "T0001".to_owned(),
        bot_token: SecretBearer::new(SECRETS[0].to_owned()),
    }
}

/// The administrator's connection handle and the id of `ops-slack`, created
/// through it.
///
/// # Panics
/// Panics when the connection cannot be created.
async fn admin_with_slack(deployment: &Deployment) -> (OperatorConnections, OperatorConnectionId) {
    let admin = OperatorConnections::with_client(deployment.admin());
    let created = admin.create(&slack()).await.expect("connection creates");
    (admin, created.connection_id)
}

/// The administrator creates Slack, `PagerDuty`, and HTTP connections, rotates
/// a secret, and disables and re-enables one; a duplicate name is refused,
/// and every view and refusal is redacted.
///
/// # Panics
/// Panics when a change fails, a view differs, or a secret leaks.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn admin_manages_redacted_connections() {
    let deployment = Deployment::start().await;
    let (admin, slack_id) = admin_with_slack(&deployment).await;

    let rotated = admin
        .update(
            &slack_id,
            &UpdateOperatorConnectionRequest::Slack {
                workspace_id: None,
                bot_token: Some(SecretBearer::new(SECRETS[1].to_owned())),
                status: None,
            },
        )
        .await
        .expect("secret rotates");
    let disabled = admin.disable(&slack_id).await.expect("connection disables");
    let enabled = admin
        .update(
            &slack_id,
            &UpdateOperatorConnectionRequest::Slack {
                workspace_id: None,
                bot_token: None,
                status: Some(OperatorConnectionStatus::Active),
            },
        )
        .await
        .expect("connection re-enables");
    let http = admin
        .create(&CreateOperatorConnectionRequest::Http {
            name: ConnectionName::new("ops-http").expect("connection name"),
            origin: HttpsOrigin::parse("HTTPS://Hooks.Example.COM:443").expect("origin parses"),
            auth: HttpConnectionAuth::Header {
                name: "X-Api-Key".to_owned(),
                value: SecretBearer::new(SECRETS[2].to_owned()),
            },
        })
        .await
        .expect("HTTP connection creates");
    let pager = admin
        .create(&CreateOperatorConnectionRequest::PagerDuty {
            name: ConnectionName::new("ops-pager").expect("connection name"),
            integration_key: SecretBearer::new(SECRETS[2].to_owned()),
        })
        .await
        .expect("PagerDuty connection creates");
    let duplicate = admin
        .create(&slack())
        .await
        .expect_err("a duplicate name is refused");
    let listed = admin.list().await.expect("connections list");

    assert_redacted(&(
        &rotated, &disabled, &enabled, &http, &pager, &duplicate, &listed,
    ));
    assert_eq!(rotated.connection_id, slack_id);
    assert_eq!(
        [disabled.status, enabled.status],
        [
            OperatorConnectionStatus::Disabled,
            OperatorConnectionStatus::Active
        ]
    );
    assert_eq!(
        http.config,
        OperatorConnectionConfig::Http {
            origin: HttpsOrigin::parse("https://hooks.example.com").expect("origin parses"),
            auth: HttpAuthScheme::Header {
                name: "X-Api-Key".to_owned(),
            },
        }
    );
    assert_eq!(duplicate.code(), "WYRD_OPERATOR_409_CONNECTION_CONFLICT");
    assert_eq!(listed.len(), 3);
    deployment.shutdown().await;
}

/// A Card writer without `operators:*` cannot list connections.
///
/// # Panics
/// Panics when the listing succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn writer_is_refused() {
    let deployment = Deployment::start().await;
    let writer = OperatorConnections::with_client(
        deployment.client(
            &deployment
                .scoped_key("card_writer", &["cards:read", "cards:write"])
                .await,
        ),
    );

    let refused = writer.list().await.expect_err("a writer cannot list");

    assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    deployment.shutdown().await;
}

/// Another tenant's administrator cannot see, rotate, or disable this
/// tenant's connection, which stays active.
///
/// # Panics
/// Panics when a foreign call succeeds, is refused with another code, leaks
/// a secret, or changes the connection.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn other_tenant_sees_nothing() {
    let deployment = Deployment::start().await;
    let (admin, slack_id) = admin_with_slack(&deployment).await;
    let foreign = OperatorConnections::with_client(
        deployment.client(&deployment.other_tenant_admin("other-tenant").await),
    );

    let listed = foreign.list().await.expect("foreign list");
    let hidden = foreign
        .get(&slack_id)
        .await
        .expect_err("another tenant's connection is hidden");
    let rotate = foreign
        .update(
            &slack_id,
            &UpdateOperatorConnectionRequest::Slack {
                workspace_id: None,
                bot_token: Some(SecretBearer::new(SECRETS[1].to_owned())),
                status: None,
            },
        )
        .await
        .expect_err("another tenant's connection cannot rotate");
    let disable = foreign
        .disable(&slack_id)
        .await
        .expect_err("another tenant's connection cannot disable");

    assert!(listed.is_empty(), "{listed:?}");
    assert_redacted(&rotate);
    for refused in [hidden, rotate, disable] {
        assert_eq!(refused.code(), "WYRD_OPERATOR_404_CONNECTION_NOT_FOUND");
    }
    assert_eq!(
        admin.get(&slack_id).await.expect("connection reads").status,
        OperatorConnectionStatus::Active
    );
    deployment.shutdown().await;
}

/// A caller holding only `operators:read` reads a connection exactly as the
/// administrator does but cannot disable it.
///
/// # Panics
/// Panics when a read fails or differs, or the disable succeeds or is
/// refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn reader_cannot_disable_a_connection() {
    let deployment = Deployment::start().await;
    let (admin, slack_id) = admin_with_slack(&deployment).await;
    let reader = OperatorConnections::with_client(
        deployment.client(
            &deployment
                .scoped_key("operator_reader", &["operators:read"])
                .await,
        ),
    );

    assert_eq!(
        reader.get(&slack_id).await.expect("the reader reads"),
        admin.get(&slack_id).await.expect("the administrator reads")
    );
    let refused = reader
        .disable(&slack_id)
        .await
        .expect_err("a reader cannot disable");

    assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    deployment.shutdown().await;
}
