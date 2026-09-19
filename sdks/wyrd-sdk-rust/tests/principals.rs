//! Rust SDK principal and credential journey through the public `wyrd_sdk` crate.
//!
//! Walks the path an operator automating a tenant actually takes: create a
//! restricted machine principal, use the credential it hands back, rotate that
//! credential by overlap, and retire the old one. Negative flows cover an
//! under-privileged caller and a credential addressed through the wrong
//! principal.
//!
//! The point of driving it through the SDK rather than raw HTTP is that the
//! contract has to be usable: the once-returned credential must be reachable
//! from the typed response and immediately usable to build another client.

use secrecy::ExposeSecret;
use wyrd_sdk::bifrost::client_from_options;
use wyrd_sdk::principals::{CreateServicePrincipalRequest, Principals};
use wyrd_testing::Bootstrap;
use wyrd_testing::server::WyrdTestServer;

/// Bootstrap a machine credential with the given roles.
///
/// # Panics
/// Panics when bootstrapping fails or returns a user principal.
async fn machine_key(server: &WyrdTestServer, name: &str, roles: &[&str]) -> String {
    match server
        .bootstrap_service(name, roles)
        .await
        .expect("service bootstraps")
    {
        Bootstrap::Machine { api_key, .. } => api_key.expose_secret().to_owned(),
        Bootstrap::User { .. } => panic!("service bootstrap returned a user principal"),
    }
}

/// Build a principal-administration handle from a server URL and credential.
///
/// # Panics
/// Panics when the shared client cannot be assembled.
fn connect(base_url: &str, credential: &str) -> Principals {
    Principals::with_client(
        client_from_options(Some(base_url), Some(credential), None).expect("client builds"),
    )
}

/// Prove the Rust SDK creates a principal and rotates its credential.
///
/// # Panics
/// Panics when any journey step or structured-error expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn creates_a_principal_and_rotates_its_credential() {
    let server = Box::pin(WyrdTestServer::start_bound())
        .await
        .expect("test server starts");
    let base_url = server
        .base_url()
        .expect("bound server has a URL")
        .to_owned();
    let admin = connect(
        &base_url,
        &machine_key(&server, "rust_principal_admin", &["admin"]).await,
    );

    let created = Box::pin(
        admin.create_service_principal(&CreateServicePrincipalRequest {
            name: "rust-ci-runner".to_owned(),
            roles: vec!["reader".to_owned()],
            description: Some("SDK journey automation".to_owned()),
        }),
    )
    .await
    .expect("principal creates");

    // The once-returned credential has to be usable without any further setup,
    // or "returned exactly once" would mean "lost".
    let automation = connect(&base_url, created.credential.expose());
    let listed = Box::pin(automation.list_credentials(&created.principal_id))
        .await
        .expect_err("a reader principal cannot administer principals");
    assert!(
        matches!(
            listed,
            wyrd_sdk::principals::WyrdError::PermissionDeniedRbac { .. }
        ),
        "an under-privileged credential is refused with the stable contract error: {listed:?}"
    );

    // Rotation by overlap: issue, verify, then retire.
    let replacement = Box::pin(admin.issue_credential(&created.principal_id))
        .await
        .expect("second credential issues");
    let before = Box::pin(admin.list_credentials(&created.principal_id))
        .await
        .expect("credentials list");
    assert_eq!(
        before.credentials.len(),
        2,
        "both credentials are visible during the overlap"
    );
    assert!(
        !format!("{before:?}").contains(replacement.credential.expose()),
        "a listing never carries secret material"
    );

    Box::pin(admin.revoke_credential(&created.principal_id, &replacement.id))
        .await
        .expect("the replacement revokes");
    let after = Box::pin(admin.list_credentials(&created.principal_id))
        .await
        .expect("credentials list");
    assert_eq!(
        after.credentials.len(),
        2,
        "revocation retires a credential rather than deleting its history"
    );
    assert!(
        after
            .credentials
            .iter()
            .any(|entry| entry.id == replacement.id && entry.revoked_at.is_some()),
        "the retired credential is visibly retired: {after:?}"
    );
}

/// A credential cannot be revoked by naming a different principal.
///
/// # Panics
/// Panics when any journey step or structured-error expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn a_credential_is_not_revocable_through_another_principal() {
    let server = Box::pin(WyrdTestServer::start_bound())
        .await
        .expect("test server starts");
    let base_url = server
        .base_url()
        .expect("bound server has a URL")
        .to_owned();
    let admin = connect(
        &base_url,
        &machine_key(&server, "rust_principal_admin_two", &["admin"]).await,
    );

    let owner = Box::pin(
        admin.create_service_principal(&CreateServicePrincipalRequest {
            name: "rust-owner".to_owned(),
            roles: vec!["reader".to_owned()],
            description: None,
        }),
    )
    .await
    .expect("owner creates");
    let bystander = Box::pin(
        admin.create_service_principal(&CreateServicePrincipalRequest {
            name: "rust-bystander".to_owned(),
            roles: vec!["reader".to_owned()],
            description: None,
        }),
    )
    .await
    .expect("bystander creates");

    let owner_credentials = Box::pin(admin.list_credentials(&owner.principal_id))
        .await
        .expect("credentials list");
    let owner_credential = owner_credentials
        .credentials
        .first()
        .expect("owner has a credential");

    let refused = Box::pin(admin.revoke_credential(&bystander.principal_id, &owner_credential.id))
        .await
        .expect_err("naming the wrong principal is refused");
    assert!(
        matches!(refused, wyrd_sdk::principals::WyrdError::NotFound { .. }),
        "the refusal is the stable contract error: {refused:?}"
    );

    let still_there = Box::pin(admin.list_credentials(&owner.principal_id))
        .await
        .expect("credentials list");
    assert!(
        still_there
            .credentials
            .iter()
            .all(|entry| entry.revoked_at.is_none()),
        "the owner's credential is untouched"
    );
}
