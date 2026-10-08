//! A tenant administrator grants a Service's principal a Role with the
//! `wyrd auth grant-role` CLI function; the Service's key acts with it at its
//! next key exchange.

use wyrd_sdk::cards::CardRef;
use wyrd_sdk::cli;
use wyrd_sdk::{Bifrost, WyrdError};

use crate::support::{Deployment, coordinates, register};

/// A tenant-wide query over a built-in table, which `wyrd_default` does not
/// permit and `workload` does.
const TENANT_WIDE: &str = "SELECT COUNT(*) AS n FROM vala.drift.observations";

/// A deployment with the observed Service registered, and its reference.
///
/// # Panics
/// Panics when registration fails.
async fn registered_service() -> (Deployment, CardRef) {
    let deployment = Deployment::start().await;
    let cards = deployment.cards();
    register(&cards, "cards/observe_a_run/observed-model.yaml").await;
    let service = register(&cards, "cards/observe_a_run/observed-service.yaml").await;
    (deployment, service.root)
}

/// A Service key issued with no grant is refused a tenant-wide query; once
/// the administrator grants `workload`, a fresh client from the same key runs
/// it.
///
/// # Panics
/// Panics when the first query succeeds or is refused with another code, the
/// grant fails or reports no change, or the second query fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn default_service_key_needs_workload_for_tenant_wide_queries() {
    let (deployment, service) = registered_service().await;
    let key = deployment.service_key(&service).await;

    let refused = Bifrost::connect(&deployment.client(&key))
        .await
        .expect("Bifrost connects")
        .sql(TENANT_WIDE, &[])
        .await
        .expect_err("wyrd_default cannot query tenant-wide");
    assert_eq!(
        WyrdError::from(refused).code(),
        "WYRD_PERMISSION_403_DENIED_RBAC"
    );

    let [kind, name, version, space] = coordinates(&service);
    let granted = cli::grant_role(
        &kind,
        &name,
        &version,
        &space,
        "workload",
        Some(deployment.admin()),
    )
    .await
    .expect("the administrator grants workload");
    assert!(granted.granted);
    assert!(granted.roles.iter().any(|role| role == "workload"));

    let counted = Bifrost::connect(&deployment.client(&key))
        .await
        .expect("Bifrost connects")
        .sql(TENANT_WIDE, &[])
        .await
        .expect("workload queries tenant-wide");
    assert_eq!(counted.num_rows(), 1);
    deployment.shutdown().await;
}

/// The Service's own key, which is no tenant administrator, cannot grant a
/// Role.
///
/// # Panics
/// Panics when the grant succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn only_a_tenant_admin_can_grant_a_role() {
    let (deployment, service) = registered_service().await;
    let key = deployment.service_key(&service).await;

    let [kind, name, version, space] = coordinates(&service);
    let refused = cli::grant_role(
        &kind,
        &name,
        &version,
        &space,
        "workload",
        Some(deployment.client(&key)),
    )
    .await
    .expect_err("a non-administrator cannot grant a Role");

    assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    deployment.shutdown().await;
}
