//! Rust SDK platform control-plane journey through the public `wyrd_sdk` crate.
//!
//! Walks the operator path: take a platform session from the deployment's
//! administrative credential, provision a tenant, and use the credential that
//! comes back to act inside it. Then the property no other test can establish —
//! that a tenant credential reaches no platform operation through this SDK, and
//! the refusal is the stable contract error rather than a transport failure.

use secrecy::SecretString;
use wyrd_sdk::bifrost::client_from_options;
use wyrd_sdk::platform::{CreateTenantRequest, Platform, RegisterPlatformAdminRequest, WyrdError};
use wyrd_sdk::principals::{CreateServicePrincipalRequest, Principals};
use wyrd_testing::server::WyrdTestServer;

/// Initialize the deployment and connect a platform client to it.
///
/// # Panics
/// Panics when initialization or the credential exchange fails.
async fn operator(server: &WyrdTestServer, base_url: &str) -> Platform {
    let root = server
        .initialize_platform_root()
        .await
        .expect("deployment initializes");
    Platform::connect(base_url, &root)
        .await
        .expect("the administrative credential exchanges for a session")
}

/// Prove the SDK provisions a usable tenant from the platform plane.
///
/// # Panics
/// Panics when any journey step or structured-error expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn provisions_a_tenant_and_administers_it() {
    let server = Box::pin(WyrdTestServer::start_bound())
        .await
        .expect("test server starts");
    let base_url = server
        .base_url()
        .expect("bound server has a URL")
        .to_owned();
    let platform = operator(&server, &base_url).await;

    let created = Box::pin(platform.create_tenant(&CreateTenantRequest {
        slug: "rust-sdk-tenant".parse().expect("slug parses"),
        display_name: "Rust SDK Tenant".to_owned(),
    }))
    .await
    .expect("tenant provisions");
    assert_eq!(
        created.tenant.status, "active",
        "a returned tenant is usable, not half-provisioned"
    );

    // The once-returned tenant credential has to work with no further setup.
    let tenant_admin = Principals::with_client(
        client_from_options(
            Some(&base_url),
            Some(created.admin.credential.expose()),
            None,
        )
        .expect("client builds"),
    );
    let automation = Box::pin(tenant_admin.create_service_principal(
        &CreateServicePrincipalRequest {
            name: "rust-sdk-runner".to_owned(),
            roles: vec!["reader".to_owned()],
            description: None,
        },
    ))
    .await
    .expect("the tenant administrator administers its own tenant");
    assert!(
        !automation.credential.expose().is_empty(),
        "the tenant's automation receives a credential"
    );

    // Recovery restores the same principal rather than creating a second.
    let recovered = Box::pin(platform.recover_tenant_admin(created.tenant.id))
        .await
        .expect("recovery succeeds");
    assert_eq!(
        recovered.principal_id, created.admin.principal_id,
        "recovery restores the existing administrator"
    );
}

/// A tenant credential reaches no platform operation through the SDK.
///
/// This is the criterion that cannot be proved without a platform client
/// surface to try it from. The refusal must be the stable contract error, not a
/// transport failure that happens to look like one.
///
/// # Panics
/// Panics when any journey step or structured-error expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn a_tenant_credential_reaches_no_platform_operation() {
    let server = Box::pin(WyrdTestServer::start_bound())
        .await
        .expect("test server starts");
    let base_url = server
        .base_url()
        .expect("bound server has a URL")
        .to_owned();
    let platform = operator(&server, &base_url).await;

    let created = Box::pin(platform.create_tenant(&CreateTenantRequest {
        slug: "rust-sdk-outsider".parse().expect("slug parses"),
        display_name: "Outsider".to_owned(),
    }))
    .await
    .expect("tenant provisions");

    // A tenant's own administrative credential is the strongest identity that
    // tenant has, and the platform plane refuses to exchange it at all.
    let refused = Box::pin(Platform::connect(
        &base_url,
        &SecretString::from(created.admin.credential.expose().to_owned()),
    ))
    .await
    .expect_err("a tenant credential cannot become a platform session");
    assert!(
        matches!(refused, WyrdError::Unauthenticated { .. }),
        "the refusal is the stable contract error: {refused:?}"
    );
}

/// A platform session confers no authority it was not granted.
///
/// # Panics
/// Panics when any journey step or structured-error expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn registering_an_administrator_requires_a_configured_connection() {
    let server = Box::pin(WyrdTestServer::start_bound())
        .await
        .expect("test server starts");
    let base_url = server
        .base_url()
        .expect("bound server has a URL")
        .to_owned();
    let platform = operator(&server, &base_url).await;

    let refused = Box::pin(platform.register_admin(&RegisterPlatformAdminRequest {
        name: "ops-lead".to_owned(),
        match_claim: "ops@example.com".to_owned(),
    }))
    .await
    .expect_err("registration without a connection is refused");
    assert!(
        matches!(refused, WyrdError::Validation { .. }),
        "the refusal names the missing connection: {refused:?}"
    );

    // The listing still works and shows only the deployment root, so the
    // refused registration created nothing.
    let listed = Box::pin(platform.list_admins())
        .await
        .expect("listing reads");
    assert_eq!(
        listed.principals.len(),
        1,
        "a refused registration leaves no principal behind: {listed:?}"
    );
}
