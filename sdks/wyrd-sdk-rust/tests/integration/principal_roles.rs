//! A tenant administrator discovers principals and grants and revokes their
//! Roles through `Principals`; each change reaches the principal's next token.

use serde::Deserialize;
use wyrd_sdk::cards::{CardRef, Cards};
use wyrd_sdk::cli;
use wyrd_sdk::principals::{
    PrincipalId, PrincipalKindTag, PrincipalQuery, Principals, RoleAssignment, RoleSource,
};
use wyrd_sdk::{Bifrost, WyrdError};
use wyrd_testing::Bootstrap;

use crate::support::{Deployment, coordinates, fixture, register};

/// A tenant-wide query over a built-in table, which `workload` permits.
const TENANT_WIDE: &str = "SELECT COUNT(*) AS n FROM vala.drift.observations";

/// A Card only an author can register.
const AUTHORED: &str = "cards/gateway_inference/ask-prompt.yaml";

/// One `COUNT(*)` result.
#[derive(Debug, Deserialize)]
struct Count {
    /// The counted rows.
    n: i64,
}

/// One `(role, source)` assignment.
fn assignment(role: &str, source: RoleSource) -> RoleAssignment {
    RoleAssignment {
        role: role.to_owned(),
        source,
    }
}

/// A deployment with the observed Service registered, and the principal and
/// key the `issue_key` CLI function issues for it.
///
/// # Panics
/// Panics when registration or issuance fails.
async fn service_principal() -> (Deployment, PrincipalId, String) {
    let deployment = Deployment::start().await;
    let cards = deployment.cards();
    register(&cards, "cards/observe_a_run/observed-model.yaml").await;
    let service: CardRef = register(&cards, "cards/observe_a_run/observed-service.yaml")
        .await
        .root;
    let [kind, name, version, space] = coordinates(&service);
    let issued = cli::issue_key(
        &kind,
        &name,
        &version,
        &space,
        None,
        None,
        Some(deployment.admin()),
    )
    .await
    .expect("the CLI issues the Service key");
    let key = issued.key.expose().to_owned();
    (deployment, issued.principal_id, key)
}

/// Register [`AUTHORED`] as the principal holding `key`, through a fresh
/// client so the call carries the principal's next token.
///
/// # Errors
/// Returns the registration error, `WYRD_PERMISSION_403_DENIED_RBAC` when the
/// principal's Roles do not include Card authoring.
async fn author(deployment: &Deployment, key: &str) -> Result<(), WyrdError> {
    Cards::with_client(deployment.client(key))
        .register_from_path(&fixture(AUTHORED))
        .await
        .map(|_| ())
}

/// A new Service key holds direct `workload`: it queries but cannot author
/// until an administrator grants `editor`, and the revoke reaches its next
/// token. Grants and revokes are idempotent.
///
/// # Panics
/// Panics when a step fails or an outcome differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn granted_editor_reaches_the_next_token_until_revoked() {
    let (deployment, principal, key) = service_principal().await;
    let principals = Principals::with_client(deployment.admin());

    let held = principals.roles(&principal).await.expect("roles list");
    assert_eq!(held.kind, PrincipalKindTag::Service);
    assert_eq!(held.roles, [assignment("workload", RoleSource::Direct)]);
    let counted: Vec<Count> = Bifrost::connect(&deployment.client(&key))
        .await
        .expect("Bifrost connects")
        .sql_as(TENANT_WIDE, &[])
        .await
        .expect("workload queries tenant-wide");
    assert_eq!(counted.len(), 1);
    assert!(counted[0].n >= 0);
    let refused = author(&deployment, &key)
        .await
        .expect_err("workload cannot author");
    assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");

    let granted = principals
        .grant_role(&principal, "editor")
        .await
        .expect("the administrator grants editor");
    assert!(granted.changed);
    assert_eq!(
        granted.roles,
        [
            assignment("editor", RoleSource::Direct),
            assignment("workload", RoleSource::Direct)
        ]
    );
    let again = principals
        .grant_role(&principal, "editor")
        .await
        .expect("a repeat grant succeeds");
    assert!(!again.changed);
    author(&deployment, &key)
        .await
        .expect("editor authors at its next token");

    let revoked = principals
        .revoke_role(&principal, "editor")
        .await
        .expect("the administrator revokes editor");
    assert!(revoked.changed);
    assert_eq!(revoked.roles, [assignment("workload", RoleSource::Direct)]);
    let refused = author(&deployment, &key)
        .await
        .expect_err("the revoke reaches the next token");
    assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    deployment.shutdown().await;
}

/// The Service's own key, which is no tenant administrator, can neither
/// grant nor revoke a Role.
///
/// # Panics
/// Panics when a write succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn only_a_tenant_admin_assigns_roles() {
    let (deployment, principal, key) = service_principal().await;
    let principals = Principals::with_client(deployment.client(&key));

    let granted = principals
        .grant_role(&principal, "editor")
        .await
        .expect_err("a workload cannot grant");
    let revoked = principals
        .revoke_role(&principal, "workload")
        .await
        .expect_err("a workload cannot revoke");

    assert_eq!(granted.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    assert_eq!(revoked.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    deployment.shutdown().await;
}

/// A signed-in user discovered by email keeps its identity-provider Role
/// beside a direct grant of the same Role; revoking removes only the direct
/// one.
///
/// # Panics
/// Panics when a step fails or an outcome differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn direct_and_idp_user_assignments_coexist() {
    let deployment = Deployment::start().await;
    let Bootstrap::User { id, .. } = deployment
        .server()
        .bootstrap_user("pr-person", &["viewer"])
        .await
        .expect("user signs in")
    else {
        panic!("user bootstrap returned a machine principal");
    };
    let principals = Principals::with_client(deployment.admin());

    let page = principals
        .list(&PrincipalQuery {
            email: Some("pr-person@test.wyrd".to_owned()),
            ..PrincipalQuery::default()
        })
        .await
        .expect("discovery lists");
    assert_eq!(page.principals.len(), 1);
    assert_eq!(page.principals[0].principal_id, id);
    assert_eq!(page.principals[0].kind, PrincipalKindTag::User);
    assert_eq!(page.next, None);

    let granted = principals
        .grant_role(&id, "viewer")
        .await
        .expect("the administrator grants viewer directly");
    assert!(granted.changed);
    assert_eq!(
        granted.roles,
        [
            assignment("viewer", RoleSource::Direct),
            assignment("viewer", RoleSource::Idp)
        ]
    );
    let revoked = principals
        .revoke_role(&id, "viewer")
        .await
        .expect("the administrator revokes viewer");
    assert!(revoked.changed);
    assert_eq!(revoked.roles, [assignment("viewer", RoleSource::Idp)]);
    deployment.shutdown().await;
}
