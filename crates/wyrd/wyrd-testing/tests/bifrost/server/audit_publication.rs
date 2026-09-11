use vala_bifrost_redux::tables::audit::projection::project_audit_rows;
use vala_sql::queries::audit_outbox::{append_audit, list_publication_batch};
use vala_sql::row_types::audit_outbox::AuditOutboxRow;
use wyrd_runtime::permission::PermissionSet;
use wyrd_runtime::{Principal, PrincipalKind};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PLATFORM_AUDIT_PRINCIPAL, PrincipalId, PrincipalKindTag};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditEvent, AuditResult, AuthMethod, BifrostQueryRequest, FreshnessPolicy,
    VisibilityMode,
};
use wyrd_spec::vala::error::BifrostError;
use wyrd_testing::WyrdTestServer;

use super::query::{ServerJourneyError, scheduled_context};

/// Retained history this journey reads back.
const AUDIT_LOG: &str = "vala.system.audit_log";

/// Bound on every wait for the server-owned publisher to make progress.
const PUBLICATION_BUDGET: std::time::Duration = std::time::Duration::from_secs(90);

/// Builds the internal principal one tenant's publication runs as.
fn publisher_principal(tenant: DataTenantId) -> Principal {
    Principal::new(
        PLATFORM_AUDIT_PRINCIPAL,
        PrincipalKind::User,
        tenant,
        Vec::new(),
        PermissionSet::new(),
    )
}

/// Counts retained audit rows recorded under one operation name.
///
/// The read goes through the server's own scheduled caller so the count is the
/// one a caller of the public query surface would see, fused across the rows
/// Scribe still holds and anything already published.
///
/// Retained history is registered by its first publication, so a tenant that
/// has never published owns no such table yet. That is an honest zero rather
/// than a failure: the caller is polling for a move the server has not made.
///
/// # Errors
/// Returns the authorization, query, or terminal failure the caller raised.
async fn retained_rows(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    operation: &str,
) -> Result<u64, ServerJourneyError> {
    let outcome = wyrd_server::query::scheduled::ScheduledQueryCaller::new(
        server.state().clone(),
        scheduled_context(tenant)?,
        tokio_util::sync::CancellationToken::new(),
    )
    .run(BifrostQueryRequest {
        sql: format!("SELECT seq FROM {AUDIT_LOG} WHERE operation = '{operation}'"),
        visibility: VisibilityMode::Fused,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: Some(60_000),
    })
    .await;
    match outcome {
        Ok(outcome) => Ok(outcome.rows),
        Err(WyrdError::Vala {
            error: BifrostError::TableNotFound { .. },
        }) => Ok(0),
        Err(error) => Err(error.into()),
    }
}

/// Waits until one operation is retained exactly `expected` times.
///
/// Publication is a server-owned background move, so a read taken immediately
/// after an append can honestly precede it. This polls the public read rather
/// than sleeping past the worker, and fails naming the count it last observed.
///
/// # Errors
/// Returns the query failure, or a timeout naming the last observed count.
async fn await_retained(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    operation: &str,
    expected: u64,
) -> Result<(), ServerJourneyError> {
    let deadline = std::time::Instant::now() + PUBLICATION_BUDGET;
    loop {
        let observed = retained_rows(server, tenant, operation).await?;
        if observed == expected {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "`{operation}` was retained {observed} times within the bounded wait, expected {expected}"
            )
            .into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

/// Builds one synthetic outbox row outside every real sequence range.
///
/// A synthetic range lets the replay scenario publish the *identical* batch
/// twice without racing the server's own publisher for a real one. Everything
/// the deduplication decision depends on is real: the tenant and the inclusive
/// sequence range the deterministic batch id is derived from.
fn synthetic_row(tenant: DataTenantId, seq: i64, operation: &str) -> AuditOutboxRow {
    AuditOutboxRow {
        data_tenant_id: tenant.as_uuid(),
        seq,
        entry_hash: vec![0; 32],
        prev_hash: vec![0; 32],
        request_id: RequestId::now_v7().to_string(),
        trace_id: None,
        operation: operation.to_owned(),
        resource: "vala.datasets.journey".to_owned(),
        card_ref: None,
        principal_id: uuid::Uuid::nil(),
        principal_kind: "user".to_owned(),
        auth_method: "internal".to_owned(),
        permission: "bifrost:record:write".to_owned(),
        decision: "allow".to_owned(),
        result: "success".to_owned(),
        payload_summary: "journey replay".to_owned(),
        detail: None,
        created_at: chrono::Utc::now(),
    }
}

/// A replayed publication retains each audit event exactly once.
///
/// A publication that is durable but not yet retired is republished by the next
/// cycle, so the boundary's real hazard is a count: the same range arriving
/// twice in retained history. That failure is invisible to a test that
/// publishes once and looks once, which is why this journey publishes the
/// *same* range twice before it reads. The deterministic batch id derived from
/// the tenant and its inclusive sequence range is what must make the second
/// publication a no-op inside Scribe's durable dedup fence.
///
/// # Errors
/// Returns the server, projection, publication, or query failure.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn replayed_audit_publication_retains_each_event_once() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::start_bound().await?;
    let tenant = server.data_tenant_id();
    let operation = format!(
        "wyrd.journey.audit_replay.{}",
        uuid::Uuid::now_v7().simple()
    );

    let rows: Vec<AuditOutboxRow> = (0..3)
        .map(|offset| synthetic_row(tenant, 9_000_000_000 + offset, &operation))
        .collect();

    let gate = server.state().bifrost.gate().clone();
    let principal = publisher_principal(tenant);
    for _ in 0..2 {
        gate.publish_audit_projection(
            &principal,
            RequestId::now_v7(),
            project_audit_rows(tenant, &rows)?,
        )
        .await?;
    }

    await_retained(&server, tenant, &operation, rows.len() as u64).await?;

    server.shutdown().await?;
    Ok(())
}

/// An audited transition reaches retained history and only then leaves the outbox.
///
/// `vala.audit_outbox` is transient delivery state and `vala.system.audit_log`
/// is the retained authority, so a row may retire only once its content is
/// durable in the Bifrost table. This appends one distinctive event through the
/// production writer, runs the production publisher, and asserts both halves of
/// that contract: the event readable exactly once through the public query
/// surface, and the row gone from the outbox. A publisher that retired a range
/// it had not shipped loses the first assertion; one that never retires loses
/// the second.
///
/// The wait is on the server's own publication worker rather than on a
/// publisher this test drives: a booted server owes retained history without
/// anyone asking it to, and a worker that cannot read the tenant directory
/// fails exactly here while a directly driven cycle would still pass.
///
/// # Errors
/// Returns the server, Postgres, publication, or query failure.
///
/// # Panics
/// Panics when the appended event stays owed to the outbox after it reached
/// retained history.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn audited_transitions_retire_only_into_retained_history() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::start_bound().await?;
    let tenant = server.data_tenant_id();
    let operation = format!("wyrd.journey.audit_owed.{}", uuid::Uuid::now_v7().simple());

    let mut conn = server.tenant_conn_for(tenant).await?;
    append_audit(
        &mut conn,
        &AuditEvent::new(
            RequestId::now_v7(),
            None,
            operation.clone(),
            "vala.datasets.journey".to_owned(),
            None,
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKindTag::User,
            AuthMethod::Internal,
            "bifrost:record:write".to_owned(),
            AuditDecision::Allow,
            AuditResult::Success,
            "journey owed event".to_owned(),
        ),
    )
    .await?;
    conn.commit().await?;

    // The server's own publisher moves the event; this waits on that worker
    // rather than driving a cycle, so a publisher that never services the
    // tenant fails here instead of passing on a cycle the test performed.
    await_retained(&server, tenant, &operation, 1).await?;

    let deadline = std::time::Instant::now() + PUBLICATION_BUDGET;
    loop {
        let mut conn = server.tenant_conn_for(tenant).await?;
        let owed = list_publication_batch(&mut conn, 512).await?;
        conn.commit().await?;
        if !owed.iter().any(|row| row.operation == operation) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "`{operation}` stayed owed to the outbox after it reached retained history"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    server.shutdown().await?;
    Ok(())
}
