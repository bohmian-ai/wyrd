use vala_sql::queries::audit_staging::{
    append_audit, freeze_publication_range, list_publication_batch,
};
use wyrd_server::audit::publication::AuditPublisher;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditEvent, AuditOutcome, BifrostQueryRequest, FreshnessPolicy, VisibilityMode,
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
/// The journey drives all three against the real server. It appends three real
/// decisions, freezes their range, and holds that freeze transaction open: the
/// lock it takes on the tenant's chain head is what fences the server's own
/// publication worker out of the window, so the replay below is deterministic
/// rather than a race the test hopes to win. Inside the window it publishes the
/// identical frozen range twice — the crash-before-settlement replay — then
/// releases the freeze, settles, appends a second operation above the old bound,
/// and requires the server's own worker to carry that tail. Both operations must
/// be retained exactly once and the tenant must drain to zero.
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

    let publisher = AuditPublisher::from_state(server.state())
        .ok_or("a Scribe-bearing server composes the audit publisher")?;

    // The freeze transaction stays open across both appends. `publish_range`
    // touches staging only, so it proceeds while every other freeze or
    // settlement — including the server worker's — waits on the chain-head row.
    let mut fence = server.tenant_conn_for(tenant).await?;
    let range = freeze_publication_range(&mut fence, 512)
        .await?
        .ok_or("three appended decisions owe a range")?;
    publisher.publish_range(tenant, range).await?;
    publisher.publish_range(tenant, range).await?;
    fence.commit().await?;

    publisher.settle(tenant, range.seq_hi).await?;
    await_retained(&server, tenant, &frozen_op, 3).await?;

    // The tail was appended above the frozen bound, so it is a separate batch
    // the server's own worker must pick up without anyone asking it to.
    append_decision(&server, tenant, &tail_op).await?;
    await_retained(&server, tenant, &tail_op, 1).await?;
    await_retained(&server, tenant, &frozen_op, 3).await?;
    await_drained(&server, tenant).await?;

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
