//! Tier-2 coverage for Forge's independent production routes.
//!
//! One table has exactly one active-task slot, so the scheduler must choose
//! between the strategies rather than run them together. These tests pin that
//! arbitration order and the immutability of the orphan plan it produces.

use chrono::{Duration as ChronoDuration, Utc};
use uuid::Uuid;
use vala_bifrost_redux::forge::ForgeScheduler;
use vala_sql::queries::forge_tasks::{ForgeEnqueueBatch, ForgeTasks};
use vala_sql::row_types::forge_tasks::{
    ForgePlanningDemand, ForgePlanningDemandSource, ForgeTaskStrategy, ForgeTaskTableIdentity,
    NewForgeTask, ORPHAN_CLEANUP_PAYLOAD_VERSION,
};
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

use std::sync::Arc;

use super::snapshot_expiration::{ExpirableTable, head_watermark, seed_ready_expiry_task};
use super::support::{
    CountingObjectStore, PromotionCatalogSeam, PromotionIntegrationFixture, SupervisedPromotion,
    manual_clock,
};

/// Starts one expirable table whose only planner candidate is expiration.
///
/// The shared expirable fixture also owes a small-file rewrite, which would
/// make every pass in this test choose that candidate. Raising the small-file
/// threshold to one byte removes the rewrite candidate at its source without
/// disabling any route, so the arbitration order stays the thing under test.
///
/// # Panics
///
/// Panics when a fixture dependency, promotion, or clock advance fails.
async fn expirable_table_without_rewrite_debt(name: &str) -> ExpirableTable {
    let mut fixture = PromotionIntegrationFixture::start(name).await;
    fixture.config.snapshot_expiry_enabled = true;
    fixture.config.small_file_threshold_bytes = 1;
    let store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let seam = PromotionCatalogSeam::new(fixture.catalog.iceberg_catalog(), store.read_counter());
    let (clock, control) = manual_clock();
    let mut supervised = SupervisedPromotion::start(
        &fixture,
        Arc::clone(&seam) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&store) as Arc<dyn vala_bifrost_redux::forge::ForgeObjectStore>,
        clock,
    );
    supervised.run_one_success().await;
    fixture.seal_more(2).await;
    supervised.restart_worker();
    supervised.run_one_success().await;
    control
        .advance(ChronoDuration::hours(48))
        .expect("manual clock advance");
    let watermark = head_watermark(&fixture).await;
    ExpirableTable {
        fixture,
        store,
        seam,
        supervised,
        control,
        watermark,
    }
}

/// Builds the validated logical identity of the fixture's one table.
fn identity(fixture: &PromotionIntegrationFixture) -> ForgeTaskTableIdentity {
    ForgeTaskTableIdentity::new(
        "wyrd-redux",
        fixture.binding.table_ref.namespace.as_str(),
        &fixture.binding.table_ref.name,
    )
    .expect("fixture table identity")
}

/// Builds one authoritative demand generation fixed at `observed`.
///
/// Arbitration consumes a demand row, so constructing it directly is what lets
/// this test hold `last_requested_at` still while the clock moves around it.
fn demand(
    fixture: &PromotionIntegrationFixture,
    observed: chrono::DateTime<Utc>,
) -> ForgePlanningDemand {
    ForgePlanningDemand {
        data_tenant_id: fixture.tenant,
        table_ref: identity(fixture),
        first_requested_at: observed,
        last_requested_at: observed,
        last_source: ForgePlanningDemandSource::Hint,
        generation: 1,
        acknowledged_snapshot_id: None,
        acknowledged_commit_count: None,
    }
}

/// Runs one production arbitration pass and returns the single chosen task.
///
/// # Panics
///
/// Panics when arbitration fails or does not fill the one active-task slot.
async fn chosen(table: &ExpirableTable, demand: &ForgePlanningDemand) -> NewForgeTask {
    let forge = table.supervised.forge();
    let mut tasks = ForgeScheduler::with_owner_for_test(&forge, Uuid::now_v7())
        .expect("fixture scheduler")
        .arbitrate_demand_for_test(demand)
        .await
        .expect("production arbitration runs");
    assert_eq!(tasks.len(), 1, "one table fills one active-task slot");
    tasks.pop().expect("exactly one task was just asserted")
}

/// Orphan cleanup is chosen last and carries one immutable planning cut.
///
/// Three passes over the same table exercise the fixed arbitration order: a
/// committed expiration handoff, then the existing planner's own candidate,
/// then the always-due orphan fallback. The third pass proves the orphan plan
/// is a pure function of the demand generation the pass observed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn orphan_cleanup_is_last_and_uses_one_demand_cutoff() {
    let table = expirable_table_without_rewrite_debt("orphan_last").await;
    let observed = Utc::now();

    // Pass one: the existing planner still owes this table a snapshot
    // expiration, so its own candidate fills the slot and orphan work is not
    // considered.
    let planner_choice = chosen(&table, &demand(&table.fixture, observed)).await;
    assert_eq!(planner_choice.strategy, ForgeTaskStrategy::SnapshotExpiry);

    // Pass two: a real expiration commits and leaves an unconsumed cleanup
    // handoff, which outranks both fresh planning and orphan work.
    let expiry = seed_ready_expiry_task(&table.fixture, table.watermark, "55").await;
    let worker = vala_bifrost_redux::forge::ForgeWorker::new(
        table.supervised.forge(),
        vala_bifrost_redux::forge::ForgeWorkerConfig::default(),
        Uuid::now_v7(),
    )
    .expect("fixture Forge worker");
    let claim = worker
        .claim_for_test()
        .await
        .expect("claim transaction runs")
        .expect("the ready expiry task is claimable");
    assert_eq!(claim.task_id, expiry);
    worker
        .execute_snapshot_expiry_claim_for_test(claim, &tokio_util::sync::CancellationToken::new())
        .await
        .expect("the expiration settles");
    let cleanup_choice = chosen(&table, &demand(&table.fixture, observed)).await;
    assert_eq!(cleanup_choice.strategy, ForgeTaskStrategy::ExpiredCleanup);

    // Consume the handoff exactly the way production does, so the third pass
    // has neither a handoff nor a planner candidate left.
    enqueue(&table.fixture, &cleanup_choice, observed).await;

    // Pass three: the clock moves after the demand is observed, and the plan
    // must not move with it.
    let observed_demand = demand(&table.fixture, observed);
    table
        .control
        .advance(ChronoDuration::hours(6))
        .expect("manual clock advance");
    let orphan = chosen(&table, &observed_demand).await;
    assert_eq!(orphan.strategy, ForgeTaskStrategy::OrphanCleanup);

    let cutoff = observed
        .checked_sub_signed(
            ChronoDuration::from_std(table.fixture.config.orphan_gc_ttl).expect("TTL converts"),
        )
        .expect("cutoff is representable")
        .timestamp_millis();
    assert_eq!(
        orphan.plan.parameters,
        serde_json::json!({
            "version": ORPHAN_CLEANUP_PAYLOAD_VERSION,
            "kind": "orphan_cleanup",
            "age_cutoff_ms": cutoff,
        })
    );

    // The worker's own validators accept the plan, and repeated hashing of the
    // same observed demand is byte-identical.
    let payload = orphan
        .plan
        .orphan_cleanup_payload(ForgeTaskStrategy::OrphanCleanup, false)
        .expect("the worker validator accepts the plan");
    assert_eq!(payload.age_cutoff_ms, cutoff);
    orphan
        .plan
        .orphan_cleanup_prefix(false)
        .expect("the plan names one normalized scan prefix");
    let replanned = chosen(&table, &observed_demand).await;
    assert_eq!(replanned.plan_hash, orphan.plan_hash);
    assert_eq!(replanned.plan.parameters, orphan.plan.parameters);
    assert_eq!(replanned.plan.inputs, orphan.plan.inputs);
}

/// Enqueues one arbitrated task through the production transaction.
///
/// # Panics
///
/// Panics when the scheduler fence cannot be taken or the insert is refused.
async fn enqueue(
    fixture: &PromotionIntegrationFixture,
    task: &NewForgeTask,
    observed: chrono::DateTime<Utc>,
) {
    let tasks = ForgeTasks::new(fixture.operator_pool.clone());
    sqlx::query("UPDATE vala.forge_scheduler_state SET expires_at = now() - interval '1 hour'")
        .execute(fixture.operator_pool.pool())
        .await
        .expect("the live scheduler fence ages out");
    let owner = Uuid::now_v7();
    let fence = tasks
        .acquire_scheduler(owner, 30)
        .await
        .expect("scheduler lease")
        .expect("uncontended fence");
    let (demands, _) = tasks
        .planning_demands(owner, fence, 8)
        .await
        .expect("demands");
    let acknowledged = demands
        .into_iter()
        .find(|value| value.table_ref == identity(fixture))
        .unwrap_or_else(|| demand(fixture, observed));
    tasks
        .enqueue_and_acknowledge(
            owner,
            fence,
            &acknowledged,
            ForgeEnqueueBatch {
                executable: std::slice::from_ref(task),
                unschedulable: &[],
            },
            |id| {
                AuditEvent::new(
                    RequestId::now_v7(),
                    None,
                    "forge.task.unschedulable".to_owned(),
                    format!("forge-task:{id}"),
                    None,
                    PrincipalId::new(Uuid::nil()),
                    PrincipalKindTag::Service,
                    AuthMethod::Internal,
                    "bifrost:forge".to_owned(),
                    AuditDecision::Allow,
                    AuditResult::Success,
                    "unschedulable".to_owned(),
                )
            },
        )
        .await
        .expect("the arbitrated task enqueues");
}
