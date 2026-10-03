use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use sqlx::postgres::{PgConnectOptions, PgSslMode};
use vala_bifrost_redux::oracle::peer::PeerSecurityAudit;
use vala_sql::audit_outbox::AuditSink;
use vala_sql::queries::audit_staging::{
    AuditPublicationRange, append_audit, freeze_publication_range, list_publication_batch,
};
use vala_sql::{TenantConn, ValaPostgres};
use wyrd_server::audit::publication::{AuditPublisher, PublishOutcome};
use wyrd_server::oracle::PostgresPeerSecurityAudit;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditEvent, AuditOutcome, BifrostQueryRequest, BifrostSecurityViolationKind,
};
use wyrd_spec::vala::error::BifrostError;
use wyrd_testing::WyrdTestServer;

use super::query::{ServerJourneyError, await_server_ready, scheduled_context};

/// Retained history this journey reads back.
const AUDIT_LOG: &str = "vala.system.audit_log";

/// Bound on every wait for the server-owned publisher to make progress.
const PUBLICATION_BUDGET: std::time::Duration = std::time::Duration::from_secs(90);

/// Counts retained audit rows matching one SQL `predicate`.
///
/// The read goes through the server's own scheduled caller so the count is the
/// one a caller of the public query surface would see, fused across the rows
/// Scribe still holds and anything already published.
///
/// Retained history is registered by its first publication, so a tenant that
/// has never published owns no such table yet. That is an honest zero rather
/// than a failure: the caller is polling for a move the server has not made.
/// A public read may also refuse with the retryable
/// `QueryVisibilityUnavailable` while publication moves the live cut; that
/// yields `None` so the bounded poll retries instead of failing early.
///
/// # Errors
/// Returns the authorization, query, or any non-retryable terminal failure.
async fn retained_rows(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    predicate: &str,
) -> Result<Option<u64>, ServerJourneyError> {
    let outcome = wyrd_server::query::scheduled::ScheduledQueryCaller::new(
        server.state().clone(),
        scheduled_context(tenant)?,
        tokio_util::sync::CancellationToken::new(),
    )
    .run(BifrostQueryRequest {
        sql: format!("SELECT seq FROM {AUDIT_LOG} WHERE {predicate}"),
        deadline_ms: Some(60_000),
    })
    .await;
    match outcome {
        Ok(outcome) => Ok(Some(outcome.rows)),
        Err(WyrdError::Vala {
            error: BifrostError::TableNotFound { .. },
        }) => Ok(Some(0)),
        Err(WyrdError::Vala {
            error: BifrostError::QueryVisibilityUnavailable,
        }) => Ok(None),
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
    await_retained_where(
        server,
        tenant,
        &format!("operation = '{operation}'"),
        expected,
    )
    .await
}

/// Waits until exactly `expected` retained rows match `predicate`.
///
/// Polls the public read like [`await_retained`], for a caller that needs a
/// narrower match than one operation name.
///
/// # Errors
/// Returns the query failure, or a timeout naming the last observed count.
async fn await_retained_where(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    predicate: &str,
    expected: u64,
) -> Result<(), ServerJourneyError> {
    let deadline = std::time::Instant::now() + PUBLICATION_BUDGET;
    loop {
        let observed = retained_rows(server, tenant, predicate).await?;
        if observed == Some(expected) {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "`{predicate}` was retained {observed:?} times within the bounded wait \
                 (None: visibility unavailable), expected {expected}"
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
    append_event(server, tenant, &decision(operation)).await
}

/// Appends one prepared authorization decision through the production writer.
///
/// # Errors
/// Returns the Postgres or RLS failure the tenant-scoped append raised.
async fn append_event(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    event: &AuditEvent,
) -> Result<i64, ServerJourneyError> {
    let mut conn = server.tenant_conn_for(tenant).await?;
    let seq = append_audit(&mut conn, event).await?;
    conn.commit().await?;
    Ok(seq)
}

/// Builds one distinctive allowed write decision under `operation`.
fn decision(operation: &str) -> AuditEvent {
    AuditEvent::new(
        RequestId::now_v7(),
        None,
        operation.to_owned(),
        "vala.datasets.journey".to_owned(),
        None,
        PrincipalId::new(uuid::Uuid::now_v7()),
        PrincipalKindTag::User,
        "bifrost:record:write".to_owned(),
        AuditOutcome::Allowed,
    )
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
/// Returns the tenant-connection or lock failure Postgres raised, or a failure
/// when any row of the range was already settled and so could not be held.
async fn fence_staged_rows(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    range: AuditPublicationRange,
) -> Result<TenantConn<'_>, ServerJourneyError> {
    let mut fence = server.tenant_conn_for(tenant).await?;
    sqlx::query("SAVEPOINT audit_fence")
        .execute(&mut **fence.transaction())
        .await?;
    let held = sqlx::query("DELETE FROM vala.audit_staging WHERE seq BETWEEN $1 AND $2")
        .bind(range.seq_lo)
        .bind(range.seq_hi)
        .execute(&mut **fence.transaction())
        .await?
        .rows_affected();
    let owed = u64::try_from(range.seq_hi - range.seq_lo + 1)?;
    if held != owed {
        return Err(format!(
            "only {held} of {owed} staged rows in {range:?} were still unsettled to fence"
        )
        .into());
    }
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

/// Waits until at least `waiters` backends are blocked on a lock.
///
/// Postgres queues a second waiter for a row behind the first waiter's tuple
/// lock rather than behind the row's holder, so a queue of unnamed production
/// cycles is observed as a count of every backend `pg_blocking_pids` reports
/// blocked. The journey serializes its lane, so each such backend is part of
/// this interleaving. The check runs on `conn` and takes no lock of its own;
/// it clears the activity snapshot first, because Postgres otherwise keeps the
/// backend list of the first read for the rest of the transaction and never
/// sees a cycle whose pooled connection opened later.
///
/// # Errors
/// Returns the query failure, or a timeout naming the last observed count.
async fn await_blocked_backends(
    conn: &mut TenantConn<'_>,
    waiters: i64,
) -> Result<(), ServerJourneyError> {
    let deadline = std::time::Instant::now() + PUBLICATION_BUDGET;
    loop {
        sqlx::query("SELECT pg_stat_clear_snapshot()")
            .execute(&mut **conn.transaction())
            .await?;
        let queued: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity \
              WHERE cardinality(pg_blocking_pids(pid)) > 0",
        )
        .fetch_one(&mut **conn.transaction())
        .await?;
        if queued >= waiters {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "only {queued} of {waiters} backends blocked within {PUBLICATION_BUDGET:?}"
            )
            .into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// Reads the tenant's in-flight publication bound without taking its lock.
///
/// A plain read of `audit_publication` is not blocked by a settlement holding
/// the row, so it observes the committed bound mid-interleaving.
///
/// # Errors
/// Returns the tenant-connection or query failure Postgres raised.
async fn frozen_bound(
    server: &WyrdTestServer,
    tenant: DataTenantId,
) -> Result<Option<i64>, ServerJourneyError> {
    let mut conn = server.tenant_conn_for(tenant).await?;
    let bound: Option<i64> =
        sqlx::query_scalar("SELECT publishing_seq_hi FROM vala.audit_publication")
            .fetch_one(&mut **conn.transaction())
            .await?;
    conn.commit().await?;
    Ok(bound)
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
/// test that production cannot reach. The server's own publisher sweeps every
/// few seconds and is a legitimate competitor, so the setup is shaped to be
/// indifferent to it: staging is drained first, the three real decisions are
/// appended in one transaction so any sweep freezes all of them or none, and
/// their staged rows are fenced before the range is frozen, so no cycle can
/// settle them. The range is then frozen and committed before any competitor
/// starts: the freeze takes the chain head `FOR UPDATE NOWAIT`, so competitors
/// never queue on it and each one must instead observe the committed bound.
/// The frozen range must be exactly those three rows, whichever publisher froze
/// it. A tail decision is appended above that committed bound, and a direct SQL
/// freeze must reuse the identical bound while the tail exists. `survivor`
/// freezes the same bound and is paused after reading the range, before its
/// append; only then does `crashing` run a full cycle, so both freezes read the
/// committed bound before any cycle reaches settlement. `crashing` reports the
/// range its own Scribe append returned for — the durable append, attributed to
/// that cycle even while the server's sweep competes — and retained history
/// sees it while the tail stays unretained. With every settlement held by the
/// fence, `crashing` is aborted, which is exactly the crash-after-append,
/// before-settlement state, and the bound is read back unchanged. `survivor` is
/// released only after the abort, and it must report its own Scribe append of
/// exactly that range, tail excluded, while the fence still blocks settlement
/// and the bound stays live. Releasing the fence then lets it settle the reused
/// bound. The frozen decisions must be retained exactly once, the tail must
/// then be published by its own later range, and the tenant must drain to zero.
///
/// # Errors
/// Returns the server, Postgres, projection, publication, or query failure.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn frozen_audit_range_replays_once_while_its_tail_waits() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::start_bound().await?;
    // The strict retained-history reads below need Oracle and Scribe serving,
    // which `/healthz` does not promise; querying earlier fails visibility.
    await_server_ready(server.base_url().ok_or("missing HTTP URL")?).await?;
    let tenant = server.data_tenant_id();
    let suffix = uuid::Uuid::now_v7().simple().to_string();
    let frozen_op = format!("wyrd.journey.audit_frozen.{suffix}");
    let tail_op = format!("wyrd.journey.audit_tail.{suffix}");

    await_drained(&server, tenant).await?;
    let mut appender = server.tenant_conn_for(tenant).await?;
    let seq_lo = append_audit(&mut appender, &decision(&frozen_op)).await?;
    append_audit(&mut appender, &decision(&frozen_op)).await?;
    let seq_hi = append_audit(&mut appender, &decision(&frozen_op)).await?;
    appender.commit().await?;
    let appended = AuditPublicationRange { seq_lo, seq_hi };
    let mut fence = fence_staged_rows(&server, tenant, appended).await?;

    // The bound is committed before any competitor starts, so every later
    // freeze reads it rather than contending for the progress row.
    let mut freezer = server.tenant_conn_for(tenant).await?;
    let range = freeze_publication_range(&mut freezer, 512)
        .await?
        .ok_or("three appended decisions owe a range")?;
    freezer.commit().await?;
    assert_eq!(
        range, appended,
        "a drained tenant's frozen range must be exactly the three decisions appended together"
    );
    let tail_seq = append_decision(&server, tenant, &tail_op).await?;
    assert!(
        tail_seq > range.seq_hi,
        "the tail {tail_seq} must stage above the frozen bound {range:?}"
    );
    let mut competitor = server.tenant_conn_for(tenant).await?;
    let competing = freeze_publication_range(&mut competitor, 512).await?;
    competitor.commit().await?;
    assert_eq!(
        competing,
        Some(range),
        "a competing freeze must reuse the frozen bound while the tail is staged above it"
    );

    // `survivor` freezes and reads the committed range, then pauses before its
    // append, so its own append can only follow the crash. `crashing` starts
    // only once `survivor` has read the range, so neither freeze can meet a
    // settlement already holding the progress row.
    let mut survivor_publisher = AuditPublisher::from_state(server.state())
        .ok_or("a Scribe-bearing server composes the audit publisher")?;
    let mut survivor_appended = survivor_publisher.observe_appends();
    let (mut survivor_paused, survivor_release) = survivor_publisher.pause_before_append();
    let mut crashing_publisher = AuditPublisher::from_state(server.state())
        .ok_or("a Scribe-bearing server composes the audit publisher")?;
    let mut crashing_appended = crashing_publisher.observe_appends();
    let survivor = tokio::spawn(async move { survivor_publisher.publish_tenant(tenant).await });
    tokio::time::timeout(
        PUBLICATION_BUDGET,
        survivor_paused.wait_for(|paused| *paused),
    )
    .await
    .map_err(|_| "the surviving cycle never read its frozen range")??;
    let crashing = tokio::spawn(async move { crashing_publisher.publish_tenant(tenant).await });

    // `crashing` reports only after its own Scribe append returned, so the
    // range is durable in retained history while the fence still holds every
    // settlement back and the staged tail stays out of every in-flight batch.
    tokio::time::timeout(
        PUBLICATION_BUDGET,
        crashing_appended.wait_for(|appended| appended.is_some()),
    )
    .await
    .map_err(|_| "the crashing cycle never reported its durable append")??;
    assert_eq!(
        *crashing_appended.borrow(),
        Some(range),
        "the crashing cycle must append exactly the frozen range, without the tail"
    );
    await_retained(&server, tenant, &frozen_op, 3).await?;
    await_retained(&server, tenant, &tail_op, 0).await?;
    await_blocked_backends(&mut fence, 1).await?;
    assert!(
        !crashing.is_finished() && !survivor.is_finished(),
        "no cycle may settle while the fence holds the frozen rows"
    );
    assert_eq!(
        *survivor_appended.borrow(),
        None,
        "the surviving cycle must not append before the crashing cycle is cancelled"
    );
    crashing.abort();
    assert!(
        crashing.await.is_err_and(|error| error.is_cancelled()),
        "the crashing cycle must be aborted after its append and before it settles"
    );
    assert_eq!(
        frozen_bound(&server, tenant).await?,
        Some(range.seq_hi),
        "the frozen bound must survive the aborted cycle"
    );

    // Only now may the surviving cycle append: it replays exactly the frozen
    // range into Scribe's batch fence while the fence still blocks settlement.
    survivor_release.add_permits(1);
    tokio::time::timeout(
        PUBLICATION_BUDGET,
        survivor_appended.wait_for(|appended| appended.is_some()),
    )
    .await
    .map_err(|_| "the surviving cycle never replayed its durable append")??;
    assert_eq!(
        *survivor_appended.borrow(),
        Some(range),
        "the surviving cycle must replay exactly the frozen range, without the tail"
    );
    assert!(
        !survivor.is_finished(),
        "the replaying cycle must not settle while the fence holds the frozen rows"
    );
    assert_eq!(
        frozen_bound(&server, tenant).await?,
        Some(range.seq_hi),
        "the frozen bound must stay live through the replay"
    );
    await_retained(&server, tenant, &frozen_op, 3).await?;
    await_retained(&server, tenant, &tail_op, 0).await?;
    release_fence(fence).await?;

    let survived = survivor.await??;
    assert!(
        matches!(
            survived,
            PublishOutcome::Published { seq_lo, seq_hi, .. }
                if seq_lo == range.seq_lo && seq_hi == range.seq_hi
        ),
        "the surviving cycle must settle the reused bound {range:?}, got {survived:?}"
    );
    await_retained(&server, tenant, &frozen_op, 3).await?;
    await_retained(&server, tenant, &tail_op, 1).await?;
    await_retained(&server, tenant, &frozen_op, 3).await?;
    await_drained(&server, tenant).await?;

    server.shutdown().await?;
    Ok(())
}

/// Reads stay served while their audit commits wait on a locked chain head.
///
/// Every Oracle read queues its read decision for one background writer, which
/// commits each tenant's queued decisions in one transaction that waits on the
/// tenant's `audit_chain_head` row lock. A publication settling behind a locked
/// staging row holds exactly that lock. If waiting decisions parked pooled
/// connections, a few dozen reads would exhaust the Vala pool that
/// reader-epoch renewal and query pins also use, and the Oracle would fence
/// itself once renewal missed its cutoff. The journey first publishes one
/// decision, so retained history exists and a read is audited rather than
/// refused as an unknown table. It then holds the chain head, issues twice as
/// many reads as the pool has connections, and requires every read to be
/// served, every decision to wait rather than be dropped, and the server's own
/// Vala pool to still hand out a connection while the lock is held. Waiting for
/// the lease cutoff itself would take tens of seconds; a pool with no
/// connection to lend is the cause, observed directly. Releasing the lock must
/// then let every waiting decision commit and drain to zero pending.
///
/// # Errors
/// Returns the server, Postgres, publication, or query failure, or a timeout
/// when the pool has no connection left for renewal or waiting decisions never
/// drain.
///
/// # Panics
/// Panics when fewer decisions wait than reads were served, or any decision is
/// dropped.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn reads_are_served_while_audit_commits_wait_on_the_chain_head()
-> Result<(), ServerJourneyError> {
    let failures = wyrd_testing::AuditCommitFailures::install()?;
    let server = WyrdTestServer::start_bound().await?;
    let tenant = server.data_tenant_id();
    let operation = format!(
        "wyrd.journey.audit_locked.{}",
        uuid::Uuid::now_v7().simple()
    );

    append_decision(&server, tenant, &operation).await?;
    await_retained(&server, tenant, &operation, 1).await?;

    let mut fence = server.tenant_conn_for(tenant).await?;
    sqlx::query("SELECT last_seq FROM vala.audit_chain_head FOR UPDATE")
        .fetch_all(&mut **fence.transaction())
        .await?;
    let reads = vala_sql::postgres::vala_pool_config().max_connections * 2;
    let failures_before = failures.count();
    for _ in 0..reads {
        retained_rows(&server, tenant, &format!("operation = '{operation}'")).await?;
    }
    let waiting = server.oracle_runtime_inspection()?.audit_pending;
    assert!(
        waiting >= u64::from(reads),
        "every served read's decision must wait for the chain head: {waiting} of {reads}"
    );
    assert_eq!(
        failures.count() - failures_before,
        0,
        "no audit write may fail while the chain head is held"
    );
    let spare = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        server.state().postgres.vala().pool().acquire(),
    )
    .await
    .map_err(|_| "audit commits waiting on the chain head left the Vala pool no connection")??;
    drop(spare);
    fence.commit().await?;

    let deadline = std::time::Instant::now() + PUBLICATION_BUDGET;
    while server.oracle_runtime_inspection()?.audit_pending > 0 {
        if std::time::Instant::now() >= deadline {
            return Err(
                "waiting audit decisions did not drain after the chain head was released".into(),
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    await_drained(&server, tenant).await?;
    server.shutdown().await?;
    Ok(())
}

/// One stalled tenant does not hold retained history back for another tenant.
///
/// A sweep that published tenants one after another would make every tenant
/// wait on the slowest: a tenant whose progress row is held by an unrelated
/// transaction would stall the whole directory behind it. The journey seeds a
/// second tenant, appends a decision in each, then holds the boot tenant's
/// publication progress row, as a stalled competing publisher would. A cycle that reaches that tenant afterwards cannot freeze;
/// one that froze and appended before the fence landed blocks in settlement
/// until the fence is released. The second tenant must still reach retained
/// history and drain inside the bounded wait — including the read decisions the
/// polling itself stages after its first publication — which only a publisher
/// whose later sweeps never wait on a blocked cycle can do. Releasing the fence must then let the stalled
/// tenant finish as well, proving the fence delayed rather than lost its work.
///
/// Only unordered progress is proven here. The publisher's ceiling is the
/// literal `PUBLICATION_TENANT_CONCURRENCY` bounding its running cycles; proving it
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
    sqlx::query(
        "INSERT INTO vala.audit_publication (data_tenant_id) \
         VALUES (wyrd.current_tenant()) ON CONFLICT DO NOTHING",
    )
    .execute(&mut **fence.transaction())
    .await?;
    sqlx::query("SELECT published_seq FROM vala.audit_publication FOR UPDATE")
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

/// A tenant's Service-principal decision publishes and does not hold later
/// history back.
///
/// A card-free Service principal's Gate decision stages with
/// `principal_kind = 'service'`. The publisher projects a frozen range before
/// settling it; a projection that refused the decision would fail every retry,
/// pin the watermark, and keep every later decision of the tenant out of
/// retained history. The journey stages a Service decision and then a user
/// decision through the production writer, and both must retain once while
/// the tenant's staging drains to zero.
///
/// # Errors
/// Returns the server, Postgres, publication, or query failure.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn service_decisions_publish_ahead_of_later_history() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::start_bound().await?;
    let tenant = server.data_tenant_id();
    let suffix = uuid::Uuid::now_v7().simple().to_string();
    let service_op = format!("wyrd.journey.audit_service.{suffix}");
    let user_op = format!("wyrd.journey.audit_user.{suffix}");

    let mut service = decision(&service_op);
    service.principal_id = PrincipalId::new(uuid::Uuid::now_v7());
    service.principal_kind = PrincipalKindTag::Service;
    append_event(&server, tenant, &service).await?;
    append_decision(&server, tenant, &user_op).await?;

    await_retained(&server, tenant, &service_op, 1).await?;
    await_retained(&server, tenant, &user_op, 1).await?;
    await_drained(&server, tenant).await?;

    server.shutdown().await?;
    Ok(())
}

/// Counts Scribe batch fences committed for retained audit under one tenant.
///
/// Oracle refuses system-owner reads, so system-owner retention is observed at
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
///
/// # Panics
/// Panics when the rejection is not retained in exactly one new batch.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn system_owner_security_rejections_retain_once() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::start_bound().await?;
    let system = DataTenantId::SYSTEM_OWNER;
    await_drained(&server, system).await?;
    let before = retained_audit_batches(&server, system).await?;

    PostgresPeerSecurityAudit::try_new(
        &server.state().postgres,
        std::sync::Arc::clone(&server.state().audit_outbox),
    )
    .await
    .map_err(|_| "the booted server carries the exact system sentinel")?
    .stage_unverified_ticket_rejection(BifrostSecurityViolationKind::PeerUnknownKey);
    let unsettled = server
        .state()
        .audit_outbox
        .settle(std::time::Instant::now() + PUBLICATION_BUDGET)
        .await;
    assert_eq!(
        unsettled, 0,
        "the system-owner rejection commits to staging"
    );

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

/// Counts retained rows matching one additional predicate.
///
/// The projection is what an operator actually asks after a leak — which key
/// made this decision — so the assertion filters on the public column rather
/// than only counting the operation.
///
/// # Errors
/// Returns the authorization or query failure.
async fn retained_matching(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    operation: &str,
    predicate: &str,
) -> Result<u64, ServerJourneyError> {
    let outcome = wyrd_server::query::scheduled::ScheduledQueryCaller::new(
        server.state().clone(),
        scheduled_context(tenant)?,
        tokio_util::sync::CancellationToken::new(),
    )
    .run(BifrostQueryRequest {
        sql: format!("SELECT seq FROM {AUDIT_LOG} WHERE operation = '{operation}' AND {predicate}"),
        deadline_ms: Some(60_000),
    })
    .await?;
    Ok(outcome.rows)
}

/// Where a [`CommitCutter`] breaks the next proxied `COMMIT`.
#[derive(Clone, Copy)]
enum Cut {
    /// Forwards the `COMMIT`, then drops Postgres's reply and closes the
    /// client: the transaction commits, but the writer sees its commit fail.
    AckLost,
    /// Closes both sides instead of forwarding the `COMMIT`: Postgres aborts
    /// the transaction once its client is gone, and the writer sees its
    /// commit fail.
    CommitLost,
}

/// TCP proxy in front of the test Postgres that breaks one armed commit.
///
/// It relays the Postgres wire protocol byte for byte and frames only the
/// client's messages, so it recognizes the simple-query `COMMIT` SQLx sends
/// to end a transaction. Every other connection and statement passes through
/// untouched, so the pool behind it, including the fresh connection the audit
/// writer resolves a failed commit on, behaves as a direct pool would.
struct CommitCutter {
    /// Address the proxied pool connects to.
    addr: std::net::SocketAddr,
    /// The cut the next proxied `COMMIT` takes, when armed.
    armed: Arc<Mutex<Option<Cut>>>,
    /// How many armed cuts were performed.
    fired: Arc<AtomicUsize>,
}

impl CommitCutter {
    /// Starts the proxy on a loopback port in front of `upstream`'s server.
    ///
    /// # Errors
    /// Returns the bind failure.
    async fn start(upstream: &PgConnectOptions) -> Result<Self, ServerJourneyError> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let target = (upstream.get_host().to_owned(), upstream.get_port());
        let armed = Arc::new(Mutex::new(None));
        let fired = Arc::new(AtomicUsize::new(0));
        let (accept_armed, accept_fired) = (Arc::clone(&armed), Arc::clone(&fired));
        tokio::spawn(async move {
            while let Ok((client, _)) = listener.accept().await {
                let (armed, fired) = (Arc::clone(&accept_armed), Arc::clone(&accept_fired));
                let target = target.clone();
                tokio::spawn(async move {
                    if let Ok(upstream) = tokio::net::TcpStream::connect(target).await {
                        let _closed = relay(client, upstream, &armed, &fired).await;
                    }
                });
            }
        });
        Ok(Self { addr, armed, fired })
    }

    /// Arms `cut` for the next proxied `COMMIT`.
    fn arm(&self, cut: Cut) {
        *self.armed.lock().expect("cut lock is never poisoned") = Some(cut);
    }

    /// How many armed cuts were performed so far.
    fn fired(&self) -> usize {
        self.fired.load(Ordering::Acquire)
    }

    /// A Vala handle whose pool reaches Postgres through this proxy, as the
    /// role and database of `upstream`, without TLS so the proxy can frame
    /// the protocol.
    ///
    /// # Errors
    /// Returns the connection failure.
    async fn vala(&self, upstream: &PgConnectOptions) -> Result<ValaPostgres, ServerJourneyError> {
        let options = upstream
            .clone()
            .host(&self.addr.ip().to_string())
            .port(self.addr.port())
            .ssl_mode(PgSslMode::Disable);
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        Ok(ValaPostgres::from_pool(pool))
    }
}

/// Relays one proxied connection, performing an armed cut at its `COMMIT`.
///
/// Server bytes are relayed by a separate task. For [`Cut::AckLost`] the
/// `COMMIT` is forwarded only after that task is told to swallow what follows,
/// so Postgres commits but the client reads no reply; returning then closes the
/// write side toward Postgres after the `COMMIT`, and the relay closes the
/// client once the reply arrives. For [`Cut::CommitLost`] the `COMMIT` is
/// never forwarded, so Postgres sees its client leave mid-transaction.
///
/// # Errors
/// Returns the read or write failure of either side.
async fn relay(
    client: tokio::net::TcpStream,
    upstream: tokio::net::TcpStream,
    armed: &Mutex<Option<Cut>>,
    fired: &AtomicUsize,
) -> std::io::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let (mut client_rx, mut client_tx) = client.into_split();
    let (mut upstream_rx, mut upstream_tx) = upstream.into_split();
    let swallow = Arc::new(AtomicBool::new(false));
    let replies_swallow = Arc::clone(&swallow);
    tokio::spawn(async move {
        let mut buffer = vec![0_u8; 8192];
        loop {
            let read = upstream_rx.read(&mut buffer).await?;
            if read == 0 || replies_swallow.load(Ordering::Acquire) {
                return Ok::<_, std::io::Error>(());
            }
            client_tx.write_all(&buffer[..read]).await?;
        }
    });

    // The startup message carries no type byte, only its length.
    let mut length = [0_u8; 4];
    client_rx.read_exact(&mut length).await?;
    let mut startup = vec![0_u8; (u32::from_be_bytes(length) as usize).saturating_sub(4)];
    client_rx.read_exact(&mut startup).await?;
    upstream_tx.write_all(&length).await?;
    upstream_tx.write_all(&startup).await?;

    loop {
        let mut header = [0_u8; 5];
        if client_rx.read_exact(&mut header).await.is_err() {
            return Ok(());
        }
        let length = u32::from_be_bytes([header[1], header[2], header[3], header[4]]) as usize;
        let mut body = vec![0_u8; length.saturating_sub(4)];
        client_rx.read_exact(&mut body).await?;
        if header[0] == b'Q' && body == b"COMMIT\0" {
            let cut = armed.lock().expect("cut lock is never poisoned").take();
            match cut {
                Some(Cut::AckLost) => {
                    swallow.store(true, Ordering::Release);
                    upstream_tx.write_all(&header).await?;
                    upstream_tx.write_all(&body).await?;
                    fired.fetch_add(1, Ordering::AcqRel);
                    return Ok(());
                }
                Some(Cut::CommitLost) => {
                    fired.fetch_add(1, Ordering::AcqRel);
                    return Ok(());
                }
                None => {}
            }
        }
        upstream_tx.write_all(&header).await?;
        upstream_tx.write_all(&body).await?;
    }
}

/// A failed audit commit is resolved from Postgres before any retry, so each
/// decision is staged and retained exactly once.
///
/// The production audit outbox writes through a proxy that breaks one armed
/// `COMMIT` at a time, while the server's own publisher retires staged rows.
/// Three times in a row a commit lands in Postgres but its acknowledgement is
/// lost: the writer resolves the transaction as committed, sends nothing again,
/// and the publisher retires the row before the next round. Then a commit is
/// lost before Postgres sees it: the writer resolves the transaction as
/// aborted and the outbox retries the batch, which commits once. Every
/// decision is retained exactly once, and the tenant's retained chain holds
/// every `seq` up to its head exactly once.
///
/// # Errors
/// Returns the server, proxy, Postgres, publication, or query failure.
///
/// # Panics
/// Panics when a cut does not fire or the outbox leaves work pending or lost.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn ambiguous_audit_commits_retain_each_decision_exactly_once()
-> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::start_bound().await?;
    await_server_ready(server.base_url().ok_or("missing HTTP URL")?).await?;
    let tenant = server.data_tenant_id();
    let suffix = uuid::Uuid::now_v7().simple().to_string();
    let upstream = server.state().postgres.vala().pool().connect_options();
    let cutter = CommitCutter::start(&upstream).await?;
    let outbox = AuditSink::outbox(cutter.vala(&upstream).await?);
    let settle = || std::time::Instant::now() + PUBLICATION_BUDGET;

    for round in 1..=3 {
        let operation = format!("wyrd.journey.audit_ack_lost.{round}.{suffix}");
        cutter.arm(Cut::AckLost);
        outbox.stage(tenant, decision(&operation));
        assert_eq!(outbox.settle(settle()).await, 0, "round {round} settles");
        assert_eq!(
            cutter.fired(),
            round,
            "round {round} lost its acknowledgement"
        );
        server.await_audit_published(tenant).await?;
        await_drained(&server, tenant).await?;
        await_retained(&server, tenant, &operation, 1).await?;
    }

    let aborted = format!("wyrd.journey.audit_commit_lost.{suffix}");
    cutter.arm(Cut::CommitLost);
    outbox.stage(tenant, decision(&aborted));
    assert_eq!(
        outbox.settle(settle()).await,
        0,
        "the aborted batch is retried"
    );
    assert_eq!(
        cutter.fired(),
        4,
        "the commit was lost before Postgres saw it"
    );
    server.await_audit_published(tenant).await?;
    await_retained(&server, tenant, &aborted, 1).await?;

    let mut conn = server.tenant_conn_for(tenant).await?;
    let head: i64 = sqlx::query_scalar("SELECT last_seq FROM vala.audit_chain_head")
        .fetch_one(&mut **conn.transaction())
        .await?;
    conn.commit().await?;
    server.await_audit_published(tenant).await?;
    await_retained_where(
        &server,
        tenant,
        &format!("seq <= {head}"),
        u64::try_from(head)?,
    )
    .await?;

    assert_eq!(outbox.shutdown(settle()).await, 0, "nothing is lost");
    server.shutdown().await?;
    Ok(())
}

/// Retained history carries both credential shapes in one uninterrupted read.
///
/// A decision made by a federated human names no credential and a decision
/// made with an API key names exactly one. Both travel the same canonical
/// staging-to-publisher path into the same retained table, so the proof that
/// the added column did not split history is reading both back from it —
/// through the public query surface, under one operation.
///
/// # Errors
/// Returns the server, Postgres, publication, or query failure.
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

    append_decision(&server, tenant, &operation).await?;
    let mut conn = server.tenant_conn_for(tenant).await?;
    let mut with_credential = decision(&operation);
    with_credential.credential_id = Some(credential);
    append_audit(&mut conn, &with_credential).await?;
    conn.commit().await?;

    await_retained(&server, tenant, &operation, 2).await?;
    assert_eq!(
        retained_matching(&server, tenant, &operation, "credential_id IS NULL").await?,
        1,
        "the credential-free decision is retained with no credential"
    );
    assert_eq!(
        retained_matching(
            &server,
            tenant,
            &operation,
            &format!("credential_id = '{credential}'"),
        )
        .await?,
        1,
        "the credentialed decision names the key it was made with"
    );

    server.shutdown().await?;
    Ok(())
}

/// Unwraps a machine bootstrap into its raw API key.
///
/// # Errors
/// Returns an error when the bootstrap is a user principal.
fn machine_key(bootstrap: wyrd_testing::Bootstrap) -> Result<String, ServerJourneyError> {
    match bootstrap {
        wyrd_testing::Bootstrap::Machine { api_key, .. } => {
            Ok(secrecy::ExposeSecret::expose_secret(&api_key).to_owned())
        }
        wyrd_testing::Bootstrap::User { .. } => Err("expected a machine principal".into()),
    }
}

/// A Gate write, a run start, and an Oracle query succeed while every audit
/// commit fails, and their decisions commit once the database recovers.
///
/// Permissions block and audits do not. After setup, a trigger refuses every
/// insert into `vala.audit_staging`, so each audit write the three requests
/// cause fails. Through the public client, the Gate write still becomes
/// durable, the manual run still starts, and the Oracle query returns the
/// written row while the failed writes are counted and nothing reaches
/// staging. Once the trigger is dropped, the outbox's retry commits each
/// decision exactly once, and the tenant chain publishes gap-free.
///
/// # Errors
/// Returns the server, client, or Postgres failure.
///
/// # Panics
/// Panics when any request is refused, a decision reaches staging while
/// audit fails, no failed write is counted, or a decision does not commit
/// exactly once after recovery.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn a_gate_write_run_start_and_query_succeed_while_audit_commits_fail()
-> Result<(), ServerJourneyError> {
    let failures = wyrd_testing::AuditCommitFailures::install()?;
    let root = tempfile::tempdir()?;
    let verifier = root.path().join("verifier.yaml");
    std::fs::write(
        &verifier,
        "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: audit-loss-drift\n  version: 1.0.0\n  space: default\nspec:\n  implementation:\n    kind: drift\n    spec:\n      method: Custom\n      signal:\n        kind: Metric\n        name: score\n      condition:\n        kind: Statistical\n      profile:\n        kind: Custom\n        metric_name: score\n        baseline_value: 1.0\n        alert_threshold: 0.5\n",
    )?;
    let service = root.path().join("service.yaml");
    std::fs::write(
        &service,
        "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: audit-loss-service\n  version: 1.0.0\n  space: default\nspec:\n  verified_by:\n    - verifier:\n        kind: Verifier\n        name: audit-loss-drift\n        version: 1.0.0\n        space: default\n      runs_on:\n        kind: schedule\n        cron: \"0 2 * * *\"\n",
    )?;

    let server = WyrdTestServer::start_bound().await?;
    let base_url = server
        .base_url()
        .ok_or("bound server has a URL")?
        .to_owned();
    let grpc_url = server.grpc_url();
    let client = |credential: &str| {
        wyrd_client::bifrost::client_from_options(
            Some(&base_url),
            Some(credential),
            grpc_url.as_deref(),
        )
    };
    let admin_bootstrap = server
        .bootstrap_service("audit_loss_admin", &["admin"])
        .await?;
    let admin_id = admin_bootstrap.id();
    let admin_reads =
        format!("operation = 'bifrost.query.read_decision' AND audit_principal_id = '{admin_id}'");
    let admin = client(&machine_key(admin_bootstrap)?)?;
    let cards = wyrd_client::cards::Cards::with_client(wyrd_client::WyrdClient::clone(&admin));
    Box::pin(cards.register_from_path(&verifier)).await?;
    let receipt = Box::pin(cards.register_from_path(&service)).await?;
    let binding_id = cards
        .get(wyrd_client::cards::CardSelector::exact(
            receipt.root.clone(),
        ))
        .await?
        .status
        .and_then(|status| status.verification)
        .ok_or("a binding owner serves verification status")?
        .binding_ids[0];
    let runner = wyrd_client::Verification::with_client(client(&machine_key(
        server
            .credential_registered_service(&receipt.root, &["writer"])
            .await?,
    )?)?);
    let dataset = format!("vala.datasets.audit_loss_{}", uuid::Uuid::now_v7().simple());
    let table = wyrd_client::bifrost::TableConfig::from_json_schema(
        &dataset,
        &serde_json::json!({
            "type": "object",
            "properties": { "value": { "type": "integer" } },
            "required": ["value"],
        }),
    )?;
    let writer = wyrd_client::Bifrost::connect_with_table(&admin, table).await?;
    writer.register().await?;
    let tenant = server.data_tenant_id();
    server.await_audit_published(tenant).await?;
    let writes_before = server
        .retained_audit_operation_count(tenant, "bifrost.record.write")
        .await?;
    let starts_before = server
        .retained_audit_operation_count(tenant, "verification.run.start")
        .await?;

    let superuser = server.pg_fixture().superuser_pool().await?;
    server.pg_fixture().fail_audit_staging().await?;

    writer.insert(
        serde_json::to_vec(&serde_json::json!({ "value": 1 }))?,
        wyrd_client::bifrost::Correlation::default(),
    )?;
    writer.flush().await?;
    let run_request: wyrd_client::verification::StartVerificationRunRequest =
        serde_json::from_value(serde_json::json!({
            "target": { "kind": "binding", "binding_id": binding_id.to_string() },
            "input": {
                "kind": "drift_window",
                "start": "2026-09-17T00:00:00Z",
                "end": "2026-09-17T01:00:00Z",
            },
        }))?;
    let run_id = runner.start_run(&run_request, None).await?;
    assert_eq!(
        runner.get_run(&run_id).await?.run_id,
        run_id,
        "the run started"
    );
    writer.shutdown().await?;
    failures
        .await_failure(std::time::Duration::from_secs(30))
        .await?;

    let staged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_staging \
          WHERE operation IN ('bifrost.record.write', 'verification.run.start')",
    )
    .fetch_one(&superuser)
    .await?;
    assert_eq!(staged, 0, "no decision reaches staging while audit fails");
    server.flush_bifrost().await?;
    let rows = wyrd_client::Bifrost::query_only(&admin)
        .sql(&format!("SELECT value FROM {dataset}"))
        .await?;
    assert_eq!(
        rows.num_rows(),
        1,
        "the Oracle query serves the durable Gate write while audit fails"
    );
    let staged_reads: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_staging \
          WHERE operation = 'bifrost.query.read_decision' AND principal_id = $1",
    )
    .bind(admin_id.as_uuid())
    .fetch_one(&superuser)
    .await?;
    assert_eq!(
        staged_reads, 0,
        "the query's decision waits while audit fails"
    );

    server.pg_fixture().restore_audit_staging().await?;
    server.await_audit_published(tenant).await?;
    assert_eq!(
        server
            .retained_audit_operation_count(tenant, "bifrost.record.write")
            .await?,
        writes_before + 1,
        "the Gate write's decision commits exactly once after recovery"
    );
    assert_eq!(
        server
            .retained_audit_operation_count(tenant, "verification.run.start")
            .await?,
        starts_before + 1,
        "the run start's decision commits exactly once after recovery"
    );
    await_retained_where(&server, tenant, &admin_reads, 1).await?;

    server.shutdown().await?;
    Ok(())
}
