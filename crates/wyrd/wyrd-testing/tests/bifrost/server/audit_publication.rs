use std::sync::Arc;

use vala_bifrost_redux::oracle::peer::PeerSecurityAudit;
use vala_sql::TenantConn;
use vala_sql::queries::audit_staging::{
    AuditPublicationRange, append_audit, freeze_publication_range, list_publication_batch,
};
use wyrd_server::audit::publication::AuditPublisher;
use wyrd_server::oracle::PostgresPeerSecurityAudit;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditEvent, AuditOutcome, BifrostQueryRequest, BifrostSecurityViolationKind, FreshnessPolicy,
    VisibilityMode,
};
use wyrd_spec::vala::error::BifrostError;
use wyrd_testing::WyrdTestServer;

use super::query::{ServerJourneyError, scheduled_context};

/// Retained history this journey reads back.
const AUDIT_LOG: &str = "vala.system.audit_log";

/// Bound on every wait for the server-owned publisher to make progress.
const PUBLICATION_BUDGET: std::time::Duration = std::time::Duration::from_secs(90);

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

/// Appends one distinctive authorization decision through the production writer.
///
/// # Errors
/// Returns the Postgres or RLS failure the tenant-scoped append raised.
async fn append_decision(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    operation: &str,
) -> Result<i64, ServerJourneyError> {
    let mut conn = server.tenant_conn_for(tenant).await?;
    let seq = append_audit(
        &mut conn,
        &AuditEvent::new(
            RequestId::now_v7(),
            None,
            operation.to_owned(),
            "vala.datasets.journey".to_owned(),
            None,
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKindTag::User,
            "bifrost:record:write".to_owned(),
            AuditOutcome::Allowed,
        ),
    )
    .await?;
    conn.commit().await?;
    Ok(seq)
}

/// Waits until this tenant's staging table owes nothing at all.
///
/// # Errors
/// Returns the Postgres failure, or a timeout naming what stayed owed.
async fn await_drained(
    server: &WyrdTestServer,
    tenant: DataTenantId,
) -> Result<(), ServerJourneyError> {
    let deadline = std::time::Instant::now() + PUBLICATION_BUDGET;
    loop {
        let mut conn = server.tenant_conn_for(tenant).await?;
        let owed = list_publication_batch(&mut conn, 512).await?;
        conn.commit().await?;
        if owed.is_empty() {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "the idle tenant kept {} staged row(s) through {PUBLICATION_BUDGET:?}, \
                 lowest seq {:?}",
                owed.len(),
                owed.first().map(|row| row.seq)
            )
            .into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

/// Hold every staged row of one frozen range so settlement cannot drain it.
///
/// Settlement advances the watermark and deletes through it in one transaction,
/// so a row lock on the staged rows stops a cycle after its durable Scribe
/// append and before any of its Postgres effects commit. The lock is taken on
/// the staged rows rather than on the chain-head row because the chain head is
/// also locked by `freeze_publication_range`: fencing there would stop a cycle
/// before it published anything, which is the wrong half of the window.
///
/// The lock is a row-deleting statement under a savepoint rather than
/// `FOR UPDATE`: the application role may delete drained staging rows but
/// holds no UPDATE privilege, and a row-locking read would demand one. The
/// deletion is never kept — [`release_fence`] rolls back to the savepoint,
/// which releases the row locks, before committing an empty transaction.
///
/// The returned connection owns the lock until [`release_fence`].
///
/// # Errors
/// Returns the tenant-connection or lock failure Postgres raised.
async fn fence_staged_rows(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    range: AuditPublicationRange,
) -> Result<TenantConn<'_>, ServerJourneyError> {
    let mut fence = server.tenant_conn_for(tenant).await?;
    sqlx::query("SAVEPOINT audit_fence")
        .execute(&mut **fence.transaction())
        .await?;
    sqlx::query("DELETE FROM vala.audit_staging WHERE seq BETWEEN $1 AND $2")
        .bind(range.seq_lo)
        .bind(range.seq_hi)
        .execute(&mut **fence.transaction())
        .await?;
    Ok(fence)
}

/// Release a [`fence_staged_rows`] fence without removing any staged row.
///
/// Rolling back to the savepoint undoes the held deletion and releases its
/// row locks, so a blocked settlement proceeds against unchanged staging; the
/// then-empty transaction commits.
///
/// # Errors
/// Returns the rollback or commit failure Postgres raised.
async fn release_fence(mut fence: TenantConn<'_>) -> Result<(), ServerJourneyError> {
    sqlx::query("ROLLBACK TO SAVEPOINT audit_fence")
        .execute(&mut **fence.transaction())
        .await?;
    fence.commit().await?;
    Ok(())
}

/// A frozen audit range replays exactly once while its tail waits behind it.
///
/// Three hazards share this boundary and none of them is visible to a test that
/// publishes once and looks once. A publication that is durable but not yet
/// settled is republished by the next cycle, so the same range can arrive in
/// retained history twice. A staging tail that grows mid-flight can widen a
/// competing publisher's range, producing two different batch identities for
/// overlapping content that Scribe's fence then cannot absorb. And a tenant
/// that stops appending must end with an empty staging table rather than a
/// grace tail.
///
/// The journey drives all three against the real server through the only
/// callable cycle, `publish_tenant`, so no partial stage is reachable from a
/// test that production cannot reach. It appends three real decisions, freezes
/// their range and commits that bound, then fences the staged rows so the next
/// cycle blocks at settlement rather than before publication. A spawned cycle
/// therefore reaches retained history — the assertion that its Scribe append is
/// durable — and is aborted while still holding nothing committed in Postgres,
/// which is exactly the crash-before-settlement state. Releasing the fence and
/// running the cycle again replays the identical frozen range into Scribe's
/// batch fence. Both operations must be retained exactly once, the tail
/// appended above the old bound must wait for its own range, and the tenant
/// must drain to zero.
///
/// # Errors
/// Returns the server, Postgres, projection, publication, or query failure.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn frozen_audit_range_replays_once_while_its_tail_waits() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::start_bound().await?;
    let tenant = server.data_tenant_id();
    let suffix = uuid::Uuid::now_v7().simple().to_string();
    let frozen_op = format!("wyrd.journey.audit_frozen.{suffix}");
    let tail_op = format!("wyrd.journey.audit_tail.{suffix}");

    for _ in 0..3 {
        append_decision(&server, tenant, &frozen_op).await?;
    }

    let publisher = Arc::new(
        AuditPublisher::from_state(server.state())
            .ok_or("a Scribe-bearing server composes the audit publisher")?,
    );

    let mut freezer = server.tenant_conn_for(tenant).await?;
    let range = freeze_publication_range(&mut freezer, 512)
        .await?
        .ok_or("three appended decisions owe a range")?;
    freezer.commit().await?;

    let fence = fence_staged_rows(&server, tenant, range).await?;
    let blocked = tokio::spawn({
        let publisher = Arc::clone(&publisher);
        async move { publisher.publish_tenant(tenant).await }
    });
    // The append is durable before settlement is attempted, so retained history
    // sees the range while the fence still holds every Postgres effect back.
    await_retained(&server, tenant, &frozen_op, 3).await?;
    blocked.abort();
    assert!(
        blocked.await.is_err(),
        "the fenced cycle must be aborted before it settles"
    );
    release_fence(fence).await?;

    // The bound survived the abort, so this cycle replays the identical range.
    publisher.publish_tenant(tenant).await?;
    await_retained(&server, tenant, &frozen_op, 3).await?;

    append_decision(&server, tenant, &tail_op).await?;
    await_retained(&server, tenant, &tail_op, 1).await?;
    await_retained(&server, tenant, &frozen_op, 3).await?;
    await_drained(&server, tenant).await?;

    server.shutdown().await?;
    Ok(())
}

/// One stalled tenant does not hold retained history back for another tenant.
///
/// A sweep that published tenants one after another would make every tenant
/// wait on the slowest: a tenant whose chain head is held by an unrelated
/// transaction would stall the whole directory behind it. The journey seeds a
/// second tenant, appends a decision in each, then holds the boot tenant's
/// chain-head row so its cycle cannot even freeze. The second tenant must still
/// reach retained history and drain inside the bounded wait, which only a
/// concurrent sweep can do. Releasing the fence must then let the stalled
/// tenant finish as well, proving the fence delayed rather than lost its work.
///
/// Only unordered progress is proven here. The sweep's ceiling is the literal
/// `PUBLICATION_TENANT_CONCURRENCY` handed to `for_each_concurrent`; proving it
/// end to end would need more fenced tenants than the test pool can hold.
///
/// # Errors
/// Returns the server, Postgres, publication, or query failure.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn a_stalled_tenant_does_not_block_another_tenants_history() -> Result<(), ServerJourneyError>
{
    let server = WyrdTestServer::start_bound().await?;
    let suffix = uuid::Uuid::now_v7().simple().to_string();
    let stalled = server.data_tenant_id();
    let healthy = server.seed_tenant(&format!("audit-sweep-{suffix}")).await?;
    let stalled_op = format!("wyrd.journey.audit_stalled.{suffix}");
    let healthy_op = format!("wyrd.journey.audit_healthy.{suffix}");

    append_decision(&server, stalled, &stalled_op).await?;
    append_decision(&server, healthy, &healthy_op).await?;

    // The boot tenant sorts before a freshly minted UUIDv7 tenant, so a serial
    // sweep would reach the healthy tenant only after this fence is released.
    let mut fence = server.tenant_conn_for(stalled).await?;
    sqlx::query("SELECT last_seq FROM vala.audit_chain_head FOR UPDATE")
        .fetch_all(&mut **fence.transaction())
        .await?;

    await_retained(&server, healthy, &healthy_op, 1).await?;
    await_drained(&server, healthy).await?;

    fence.commit().await?;
    await_retained(&server, stalled, &stalled_op, 1).await?;
    await_drained(&server, stalled).await?;

    server.shutdown().await?;
    Ok(())
}

/// Counts Scribe batch fences committed for retained audit under one tenant.
///
/// Oracle refuses nil-tenant reads, so system-owner retention is observed at
/// the fence Scribe commits with every durable audit-log batch instead.
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

/// A system-owner security rejection reaches retained history exactly once.
///
/// Unverified peer and tail rejections cannot name a tenant, so they stage
/// under `DataTenantId::SYSTEM_OWNER`. The server's own publisher must still
/// move them into `vala.system.audit_log` and drain system staging; before the
/// audit-only nil exception they stayed staged forever. The journey drains any
/// boot-time system rows, appends one rejection through the production peer
/// audit writer, and requires exactly one new retained batch and empty staging.
///
/// # Errors
/// Returns the server, Postgres, peer-audit, or publication failure.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn system_owner_security_rejections_retain_once() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::start_bound().await?;
    let system = DataTenantId::SYSTEM_OWNER;
    await_drained(&server, system).await?;
    let before = retained_audit_batches(&server, system).await?;

    PostgresPeerSecurityAudit::try_new(&server.state().postgres)
        .await
        .map_err(|_| "the booted server carries the exact system sentinel")?
        .append_unverified_ticket_rejection(BifrostSecurityViolationKind::PeerUnknownKey)
        .await
        .map_err(|_| "the system-owner rejection commits to staging")?;

    await_drained(&server, system).await?;
    assert_eq!(
        retained_audit_batches(&server, system).await?,
        before + 1,
        "the system-owner rejection must retain in exactly one batch"
    );

    server.shutdown().await?;
    Ok(())
}

/// An audited transition reaches retained history and only then leaves staging.
///
/// `vala.audit_staging` is transient delivery state and `vala.system.audit_log`
/// is the retained authority, so a row may retire only once its content is
/// durable in the Bifrost table. This appends one distinctive event through the
/// production writer, runs the production publisher, and asserts both halves of
/// that contract: the event readable exactly once through the public query
/// surface, and the row gone from staging. A publisher that retired a range
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
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn audited_transitions_retire_only_into_retained_history() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::start_bound().await?;
    let tenant = server.data_tenant_id();
    let operation = format!("wyrd.journey.audit_owed.{}", uuid::Uuid::now_v7().simple());

    append_decision(&server, tenant, &operation).await?;

    // The server's own publisher moves the event; this waits on that worker
    // rather than driving a cycle, so a publisher that never services the
    // tenant fails here instead of passing on a cycle the test performed.
    await_retained(&server, tenant, &operation, 1).await?;
    await_drained(&server, tenant).await?;

    server.shutdown().await?;
    Ok(())
}
