use arrow::datatypes::{DataType, Field};
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::tables::audit::projection::project_audit_rows;
use vala_sql::queries::audit_outbox::list_publication_batch;
use wyrd_runtime::permission::PermissionSet;
use wyrd_runtime::{Principal, PrincipalKind};
use wyrd_server::audit::publication::{AuditPublisher, PublishOutcome};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PLATFORM_AUDIT_PRINCIPAL;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};
use wyrd_testing::WyrdTestServer;

use super::query::{ServerJourneyError, scheduled_context};

/// Retained history this journey reads back.
const AUDIT_LOG: &str = "vala.system.audit_log";

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

/// Counts retained audit rows inside one inclusive outbox sequence range.
///
/// The read goes through the server's own scheduled caller so the count is the
/// one a caller of the public query surface would see, fused across the rows
/// Scribe still holds and anything already published.
///
/// # Errors
///
/// Returns the authorization, query, or terminal failure the caller raised.
async fn retained_rows(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    seq_lo: i64,
    seq_hi: i64,
) -> Result<u64, ServerJourneyError> {
    let outcome = wyrd_server::query::scheduled::ScheduledQueryCaller::new(
        server.state().clone(),
        scheduled_context(tenant)?,
        tokio_util::sync::CancellationToken::new(),
    )
    .run(BifrostQueryRequest {
        sql: format!("SELECT seq FROM {AUDIT_LOG} WHERE seq BETWEEN {seq_lo} AND {seq_hi}"),
        visibility: VisibilityMode::Fused,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: Some(60_000),
    })
    .await?;
    Ok(outcome.rows)
}

/// Bounded wait for the tenant to owe at least one audit event.
///
/// The server runs its own publisher, so a range this journey produced can be
/// shipped and retired before the journey claims it. Polling the same
/// production read the publisher uses — rather than sleeping past the worker —
/// keeps the claim a real one without racing it.
///
/// # Errors
///
/// Returns the Postgres failure, or a timeout naming the empty outbox.
async fn claim_owed(
    server: &WyrdTestServer,
    tenant: DataTenantId,
) -> Result<Vec<vala_sql::row_types::audit_outbox::AuditOutboxRow>, ServerJourneyError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let mut conn = server.tenant_conn_for(tenant).await?;
        let rows = list_publication_batch(&mut conn, 64).await?;
        conn.commit().await?;
        if !rows.is_empty() {
            return Ok(rows);
        }
        if std::time::Instant::now() >= deadline {
            return Err("the tenant owed no audit event within the bounded wait".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// Audited transitions reach retained history exactly once and retire after it.
///
/// `vala.audit_outbox` is the transactional authority and `vala.system.audit_log`
/// is the retained one, so the only thing that can go wrong at that boundary is
/// a count: a lost range, or a duplicated one. Both failures are invisible to a
/// test that publishes once and looks once, which is why this journey publishes
/// the *same* range twice before it reads. The second publication is exactly
/// what a crash between a durable append and its retirement produces, and the
/// deterministic batch id derived from the tenant and sequence range is what
/// must make it a no-op inside Scribe's dedup fence.
///
/// Retirement is then asserted against the same range: the rows leave the
/// outbox only once a publisher has moved them, and exactly the shipped range
/// leaves. The server runs its own publisher, so the range may already be
/// retired when this journey's publisher looks — that is the same observable
/// outcome and not a second contract.
///
/// # Errors
///
/// Returns the server, Postgres, projection, publication, or query failure.
///
/// # Panics
///
/// Panics when a replayed publication duplicates retained rows, when a shipped
/// batch retires nothing, or when the published range survives retirement.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn replayed_audit_publication_retains_each_event_once() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::start_bound().await?;
    let tenant = server.data_tenant_id();

    // A real audited transition, so the range this journey publishes is one a
    // caller produced rather than one the fixture synthesized.
    for ordinal in 0..2 {
        server
            .create_bifrost_table_for_test(CreateTableRequest {
                table: TableRef::new(
                    BifrostNamespace::Datasets,
                    format!(
                        "audit_publication_{ordinal}_{}",
                        uuid::Uuid::now_v7().simple()
                    ),
                ),
                user_fields: vec![Field::new("value", DataType::Int64, false)],
                tenant,
                physical_layout: None,
                audit: None,
            })
            .await?;
    }
    let rows = claim_owed(&server, tenant).await?;
    let projection = project_audit_rows(tenant, &rows)?;
    let (seq_lo, seq_hi) = (projection.seq_lo, projection.seq_hi);
    let expected = rows.len() as u64;

    // Publish the identical range twice: the second is the ambiguous replay.
    let gate = server.state().bifrost.gate().clone();
    let principal = publisher_principal(tenant);
    gate.publish_audit_projection(&principal, RequestId::now_v7(), projection)
        .await?;
    let replay = project_audit_rows(tenant, &rows)?;
    gate.publish_audit_projection(&principal, RequestId::now_v7(), replay)
        .await?;

    assert_eq!(
        retained_rows(&server, tenant, seq_lo, seq_hi).await?,
        expected,
        "a replayed publication of one range must retain each event exactly once"
    );

    // The production publisher then moves and retires its own bounded batch.
    // It may find the range already retired by the server's own worker, which
    // is the same outcome from the caller's side: the range is owed to nobody.
    let publisher = AuditPublisher::from_state(server.state())
        .ok_or("a Scribe-owning server composes an audit publisher")?;
    if let PublishOutcome::Published { retired, .. } = publisher.publish_tenant(tenant).await? {
        assert!(retired > 0, "a published batch retires the rows it carried");
    }
    let mut conn = server.tenant_conn_for(tenant).await?;
    let after = list_publication_batch(&mut conn, 64).await?;
    conn.commit().await?;
    assert!(
        !after
            .iter()
            .any(|row| row.seq >= seq_lo && row.seq <= seq_hi),
        "retirement removes exactly the range the publisher shipped"
    );

    server.shutdown().await?;
    Ok(())
}
