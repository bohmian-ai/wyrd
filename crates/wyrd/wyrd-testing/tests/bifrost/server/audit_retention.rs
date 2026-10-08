//! Audit decisions reach retained history through the shared Scribe outbox.
//!
//! Every audited surface stages its decision on the process Scribe outbox; the
//! outbox writes `vala.system.audit_log` through the pod's own Scribe. These
//! journeys stage decisions through the production [`AuditStage`] seam of a
//! booted server and read them back through the ordinary query path.

use std::sync::Arc;

use vala_bifrost_redux::oracle::peer::PeerSecurityAudit;
use wyrd_runtime::audit::AuditStage;
use wyrd_server::oracle::PostgresPeerSecurityAudit;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditEvent, AuditOutcome, BifrostSecurityViolationKind};
use wyrd_testing::WyrdTestServer;

use super::query::ServerJourneyError;

/// Retained history this journey reads back.
const AUDIT_LOG: &str = "vala.system.audit_log";

/// One allowed decision for `operation` by a fresh principal.
fn decision(operation: &str) -> AuditEvent {
    AuditEvent::new(
        RequestId::now_v7(),
        None,
        operation.to_owned(),
        "bifrost".to_owned(),
        None,
        PrincipalId::new(uuid::Uuid::now_v7()),
        PrincipalKindTag::Service,
        operation.to_owned(),
        AuditOutcome::Allowed,
    )
}

/// Counts the durable Scribe batches `tenant` committed into the audit log.
///
/// Oracle refuses reads of the reserved system owner, so its retention is
/// observed at the fence Scribe commits with every durable batch instead.
///
/// # Errors
/// Returns the tenant-connection or query failure Postgres raised.
async fn retained_audit_batches(
    server: &WyrdTestServer,
    tenant: DataTenantId,
) -> Result<i64, ServerJourneyError> {
    let mut conn = server.tenant_conn_for(tenant).await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.scribe_batch_commits WHERE logical_table_fqn = $1",
    )
    .bind(AUDIT_LOG)
    .fetch_one(&mut **conn.transaction())
    .await?;
    conn.commit().await?;
    Ok(count)
}

/// Retained history carries both credential shapes, each decision exactly
/// once, with its decision content and no retired chain field.
///
/// A decision made without a credential names none and one made with an API
/// key names exactly that key. Both are staged through the server's own audit
/// seam and read back through the public query path under one operation.
///
/// # Errors
/// Returns the server, settle, or query failure.
///
/// # Panics
/// Panics when a decision is missing, duplicated, or misattributed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn retained_history_carries_both_credential_shapes() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::start_bound().await?;
    let tenant = server.data_tenant_id();
    let operation = format!(
        "wyrd.journey.audit_credential.{}",
        uuid::Uuid::now_v7().simple()
    );
    let credential = uuid::Uuid::now_v7();
    let audit: &dyn AuditStage = &*server.state().scribe_outbox;
    let without = decision(&operation);
    let mut with = decision(&operation);
    with.credential_id = Some(credential);
    audit.stage(tenant, without.clone());
    audit.stage(tenant, with.clone());

    server.await_audit_retained().await?;
    server
        .await_retained_audit_count(tenant, &format!("operation = '{operation}'"), 2)
        .await?;
    let mut rows = server
        .retained_audit_records(
            tenant,
            "audit_principal_id, credential_id, outcome, permission",
            &format!("operation = '{operation}'"),
        )
        .await?;
    rows.sort();
    let mut expected = vec![
        vec![
            Some(without.principal_id.to_string()),
            None,
            Some("allowed".to_owned()),
            Some(operation.clone()),
        ],
        vec![
            Some(with.principal_id.to_string()),
            Some(credential.to_string()),
            Some("allowed".to_owned()),
            Some(operation.clone()),
        ],
    ];
    expected.sort();
    assert_eq!(rows, expected, "each decision is retained once, as decided");
    assert!(
        server
            .retained_audit_records(tenant, "seq", "true")
            .await
            .is_err(),
        "retained history carries no chain sequence"
    );

    server.shutdown().await?;
    Ok(())
}

/// A system-owner security rejection reaches retained history in exactly one
/// new batch.
///
/// Unverified peer and tail rejections cannot name a tenant, so they stage
/// under `DataTenantId::SYSTEM_OWNER`; the outbox writes them under the
/// platform audit principal, the one principal Scribe admits for the system
/// owner's audit history.
///
/// # Errors
/// Returns the server, Postgres, peer-audit, or settle failure.
///
/// # Panics
/// Panics when the rejection is not retained in exactly one new batch.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn system_owner_security_rejections_retain_once() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::start_bound().await?;
    let system = DataTenantId::SYSTEM_OWNER;
    server.await_audit_retained().await?;
    let before = retained_audit_batches(&server, system).await?;

    PostgresPeerSecurityAudit::try_new(
        &server.state().postgres,
        Arc::clone(&server.state().scribe_outbox),
    )
    .await
    .map_err(|_| "the booted server carries the exact system sentinel")?
    .stage_unverified_ticket_rejection(BifrostSecurityViolationKind::PeerUnknownKey);
    server.await_audit_retained().await?;

    assert_eq!(
        retained_audit_batches(&server, system).await?,
        before + 1,
        "the system-owner rejection must retain in exactly one batch"
    );

    server.shutdown().await?;
    Ok(())
}
