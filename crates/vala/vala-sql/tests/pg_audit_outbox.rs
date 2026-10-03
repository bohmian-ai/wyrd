//! Postgres integration tests for the process audit outbox.
//!
//! Covers gap-free chains under concurrent outboxes (two replicas), tenant
//! independence while one tenant's chain head is contended, and shutdown
//! draining. Run via `mise run test:bifrost:integration:sql`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use sqlx::PgPool;
use sqlx::types::Uuid;
use vala_sql::ValaPostgres;
use vala_sql::audit_outbox::AuditOutbox;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditEvent, AuditOutcome};

/// Upper bound for any outbox to commit what it was handed.
const COMMIT_BUDGET: Duration = Duration::from_secs(30);

/// Starts a fixture and seeds `count` tenants, returning the superuser pool.
async fn setup(count: usize) -> (PgFixture, PgPool, Vec<DataTenantId>) {
    let fixture = PgFixture::start().await.expect("fixture");
    let superuser = fixture.superuser_pool().await.expect("superuser pool");
    let mut tenants = Vec::with_capacity(count);
    for _ in 0..count {
        let tenant = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(
                tenant,
                &format!("test-{}", tenant.as_uuid().simple()),
            )
            .await
            .expect("seed tenant");
        tenants.push(tenant);
    }
    (fixture, superuser, tenants)
}

/// One allowed decision named by its operation.
fn event(operation: &str) -> AuditEvent {
    AuditEvent {
        request_id: RequestId::parse(&Uuid::now_v7().to_string()).expect("v7 request id"),
        trace_id: None,
        operation: operation.to_owned(),
        resource: "ns.tbl".to_owned(),
        card_ref: None,
        principal_id: PrincipalId::new(Uuid::now_v7()),
        principal_kind: PrincipalKindTag::User,
        credential_id: None,
        permission: "bifrost.write".to_owned(),
        outcome: AuditOutcome::Allowed,
        detail: None,
    }
}

/// Reads one tenant's staged `(seq, prev_hash, entry_hash)` rows in order.
async fn staged(superuser: &PgPool, tenant: DataTenantId) -> Vec<(i64, Vec<u8>, Vec<u8>)> {
    sqlx::query_as(
        "SELECT seq, prev_hash, entry_hash FROM vala.audit_staging
          WHERE data_tenant_id = $1 ORDER BY seq",
    )
    .bind(tenant.as_uuid())
    .fetch_all(superuser)
    .await
    .expect("staged rows read")
}

/// Polls until `tenant` has `rows` staged rows or the budget runs out.
async fn await_staged(superuser: &PgPool, tenant: DataTenantId, rows: usize) {
    let deadline = Instant::now() + COMMIT_BUDGET;
    while staged(superuser, tenant).await.len() < rows {
        assert!(
            Instant::now() < deadline,
            "tenant {tenant} did not stage {rows} rows within {COMMIT_BUDGET:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Two outboxes, standing in for two replicas, staging concurrently for one
/// tenant commit one gap-free chain whose every row links its predecessor
/// (AC-003, INV-002), and each shutdown drains its queue (AC-007).
///
/// # Panics
///
/// Panics when the fixture fails, a shutdown leaves decisions uncommitted, or
/// the committed chain has a gap or a broken link.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_outboxes_commit_one_gap_free_chain_and_drain_on_shutdown() {
    let (fixture, superuser, tenants) = setup(1).await;
    let tenant = tenants[0];
    let first = AuditOutbox::new(ValaPostgres::from_pool(fixture.app_pool().clone()));
    let second = AuditOutbox::new(ValaPostgres::from_pool(fixture.app_pool().clone()));
    let stagers: Vec<_> = [Arc::clone(&first), Arc::clone(&second)]
        .into_iter()
        .map(|outbox| {
            tokio::spawn(async move {
                for index in 0..500 {
                    outbox.stage(tenant, event(&format!("bifrost.replica.{index}")));
                    if index % 50 == 0 {
                        tokio::task::yield_now().await;
                    }
                }
            })
        })
        .collect();
    for stager in stagers {
        stager.await.expect("stager finishes");
    }

    let deadline = Instant::now() + COMMIT_BUDGET;
    assert_eq!(first.shutdown(deadline).await, 0, "first outbox drained");
    assert_eq!(second.shutdown(deadline).await, 0, "second outbox drained");

    let rows = staged(&superuser, tenant).await;
    assert_eq!(rows.len(), 1_000, "every staged decision committed once");
    let mut prev_hash = vec![0_u8; 32];
    for (expected, (seq, prev, entry)) in (1_i64..).zip(&rows) {
        assert_eq!(*seq, expected, "the chain has no gap");
        assert_eq!(*prev, prev_hash, "row {seq} links its predecessor");
        prev_hash.clone_from(entry);
    }

    first.stage(tenant, event("bifrost.after_shutdown"));
    assert_eq!(first.pending(), 0, "a shut-down outbox accepts nothing");
}

/// A tenant whose chain head is held does not delay another tenant's audit
/// (REQ-002), and its own decisions commit once the holder releases.
///
/// # Panics
///
/// Panics when the fixture fails, the free tenant does not commit while the
/// other is held, or the held tenant does not commit after release.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_contended_tenant_does_not_delay_another_tenants_audit() {
    let (fixture, superuser, tenants) = setup(2).await;
    let (held, free) = (tenants[0], tenants[1]);
    let outbox = AuditOutbox::new(ValaPostgres::from_pool(fixture.app_pool().clone()));

    let mut holder = vala_sql::TenantConn::acquire(fixture.app_pool(), held)
        .await
        .expect("holder connection");
    sqlx::query(
        "INSERT INTO vala.audit_chain_head (data_tenant_id) VALUES (wyrd.current_tenant())",
    )
    .execute(&mut **holder.transaction())
    .await
    .expect("holder takes the chain head");

    outbox.stage(held, event("bifrost.held.1"));
    outbox.stage(free, event("bifrost.free.1"));
    outbox.stage(held, event("bifrost.held.2"));
    outbox.stage(free, event("bifrost.free.2"));
    await_staged(&superuser, free, 2).await;
    assert!(
        staged(&superuser, held).await.is_empty(),
        "the held tenant cannot commit while its chain head is held"
    );

    holder.commit().await.expect("holder releases");
    await_staged(&superuser, held, 2).await;
    assert_eq!(
        outbox.shutdown(Instant::now() + COMMIT_BUDGET).await,
        0,
        "nothing remains pending"
    );
}
