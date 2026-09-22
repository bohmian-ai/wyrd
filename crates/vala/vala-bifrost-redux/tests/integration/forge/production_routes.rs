//! Tier-2 coverage for Forge's independent production routes.
//!
//! One table has exactly one active-task slot, so the scheduler must choose
//! between the strategies rather than run them together. These tests pin that
//! arbitration order and the immutability of the orphan plan it produces.

use chrono::{Duration as ChronoDuration, Utc};
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;
use uuid::Uuid;
use vala_bifrost_redux::forge::{
    ForgeClock, ForgeRoleReadiness, ForgeScheduler, ForgeSchedulerTrigger, ForgeWorker,
    ForgeWorkerCompletionObserver, ForgeWorkerConfig,
};
use vala_sql::queries::forge_tasks::{ForgeEnqueueBatch, ForgeTasks};
use vala_sql::row_types::forge_tasks::{
    ForgePlanningDemand, ForgePlanningDemandSource, ForgeTaskStrategy, ForgeTaskTableIdentity,
    NewForgeTask, ORPHAN_CLEANUP_PAYLOAD_VERSION,
};

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use wyrd_bench::BenchmarkMetricSnapshot;

use super::rewrite_support::PromotedRewriteFixture;
use super::snapshot_expiration::object_exists;
use super::snapshot_expiration::{
    ExpirableTable, expirable_table, head_watermark, seed_ready_expiry_task,
};
use super::support::{
    CountingObjectStore, ForgeTelemetryCheckpoint, PromotionCatalogSeam,
    PromotionIntegrationFixture, SupervisedPromotion, manual_clock,
};

/// Maximum diagnostic wait for a claimed ownership episode.
const OWNERSHIP_BOUND: Duration = Duration::from_secs(15);

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
            },
        )
        .await
        .expect("the arbitrated task enqueues");
}

/// Reads every Forge task this fixture's tenant owns, in creation order.
///
/// # Panics
///
/// Panics when the diagnostic read fails.
async fn settled_tasks(fixture: &PromotionIntegrationFixture) -> Vec<(Uuid, String, String)> {
    sqlx::query_as(
        "SELECT task_id, strategy, state FROM vala.forge_tasks WHERE data_tenant_id = $1 \
         ORDER BY created_at, task_id",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("Forge tasks are readable")
}

/// Four strategies plan, dispatch, and settle as four independent tasks.
///
/// One supervised coordinator plans against real Postgres, Iceberg, and object
/// storage while a separately-owned worker consumes what it enqueued. Repeated
/// pass-and-settle cycles are driven until every retained production route has
/// produced its own durable task, which is only reachable when each strategy is
/// admitted, dispatched to its own owner, and settled without borrowing another
/// strategy's effect.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn four_strategies_schedule_dispatch_and_settle_independently() {
    let mut table = expirable_table("four_routes", true).await;
    // Promotion already settled twice while the fixture started, so the routes
    // still owed are compaction, expiration, expired cleanup, and orphan work.
    let mut deletes_before_expiry = None;
    for _ in 0..8 {
        let settled = settled_tasks(&table.fixture).await;
        let seen = settled
            .iter()
            .map(|(_, strategy, _)| strategy.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        if [
            "small_files",
            "snapshot_expiry",
            "expired_cleanup",
            "orphan_cleanup",
        ]
        .iter()
        .all(|strategy| seen.contains(strategy))
        {
            break;
        }
        if deletes_before_expiry.is_none() && seen.contains("small_files") {
            deletes_before_expiry = Some(table.store.deletes());
        }
        table.supervised.restart_worker();
        table.supervised.run_one_success().await;
    }

    let settled = settled_tasks(&table.fixture).await;
    for strategy in [
        "small_files",
        "snapshot_expiry",
        "expired_cleanup",
        "orphan_cleanup",
    ] {
        let rows = settled
            .iter()
            .filter(|(_, value, _)| value == strategy)
            .collect::<Vec<_>>();
        assert!(
            !rows.is_empty(),
            "{strategy} owns its own durable task: {settled:?}"
        );
        assert!(
            rows.iter().all(|(_, _, state)| state == "succeeded"),
            "{strategy} settled on its own: {settled:?}"
        );
    }
    let identities = settled
        .iter()
        .map(|(task_id, _, _)| *task_id)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        identities.len(),
        settled.len(),
        "every route owns a distinct task identity"
    );

    // Snapshot expiration leaves durable cleanup demand rather than performing
    // the deletion itself, so the exact objects it retired are only removed
    // once the separate cleanup task claims them.
    let expiry = settled
        .iter()
        .find(|(_, strategy, _)| strategy == "snapshot_expiry")
        .expect("the expiration route settled");
    assert_eq!(
        settled
            .iter()
            .filter(|(_, strategy, _)| strategy == "snapshot_expiry")
            .count(),
        1,
        "one expiration settled the retention debt: {settled:?}"
    );
    let evidence: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT evidence FROM vala.forge_tasks WHERE task_id = $1")
            .bind(expiry.0)
            .fetch_one(table.fixture.operator_pool.pool())
            .await
            .expect("expiry evidence is readable");
    let candidates = evidence
        .as_ref()
        .and_then(|value| value.get("cleanup_candidates"))
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    assert!(
        candidates > 0,
        "the expiration handed off its candidates instead of deleting them"
    );
}

/// Sums every recorded counter series of one family carrying all given labels.
pub(crate) fn counter_total(
    snapshot: &wyrd_bench::BenchmarkMetricSnapshot,
    family: &str,
    labels: &[(&str, &str)],
) -> u64 {
    snapshot
        .counters
        .iter()
        .filter(|(name, _)| name.starts_with(&format!("{family}{{")))
        .filter(|(name, _)| {
            labels
                .iter()
                .all(|(key, value)| name.contains(&format!("{key}=\"{value}\"")))
        })
        .map(|(_, value)| *value)
        .sum()
}

/// Sums the active ownership series in an already captured snapshot.
fn active_tasks(snapshot: &BenchmarkMetricSnapshot) -> f64 {
    snapshot
        .gauges
        .iter()
        .filter(|(name, _)| name.starts_with("bifrost_forge_active_tasks{"))
        .map(|(_, value)| value)
        .sum()
}

/// Counts completed ownership durations across all durable outcome labels.
fn duration_count(snapshot: &BenchmarkMetricSnapshot) -> u64 {
    snapshot
        .histograms
        .iter()
        .filter(|(name, _)| name.starts_with("bifrost_forge_task_duration_seconds{"))
        .map(|(_, value)| value.count)
        .sum()
}

/// Checks exact integer debt values representable by these small real fixtures.
///
/// # Panics
/// Panics when a fixture exceeds the exact conversion bound or a gauge differs.
fn assert_inventory(snapshot: &BenchmarkMetricSnapshot, files: u32, bytes: u64) {
    let bytes = u32::try_from(bytes).expect("fixture debt fits exact floating-point conversion");
    for (name, expected) in [
        ("bifrost_forge_compaction_debt_files", files),
        ("bifrost_forge_compaction_debt_bytes", bytes),
    ] {
        assert!(
            (snapshot.gauges[name] - f64::from(expected)).abs() < f64::EPSILON,
            "{name}: actual={}, expected={expected}",
            snapshot.gauges[name]
        );
    }
}

/// Verifies one completed ownership episode and returns its measured nanoseconds.
///
/// # Panics
/// Panics on leaked ownership, duplicate or misclassified attempts, or short timing.
fn assert_completed_episode(
    snapshot: &BenchmarkMetricSnapshot,
    previous_attempts: u64,
    result: &str,
    held: Duration,
) -> u64 {
    assert!(
        active_tasks(snapshot).abs() < f64::EPSILON,
        "ownership is complete"
    );
    assert_eq!(
        counter_total(snapshot, "bifrost_forge_task_attempts_total", &[]) - previous_attempts,
        1
    );
    assert_eq!(
        counter_total(
            snapshot,
            "bifrost_forge_task_attempts_total",
            &[("result", result)]
        ),
        1
    );
    let durations: Vec<_> = snapshot
        .histograms
        .iter()
        .filter(|(name, _)| {
            name.starts_with("bifrost_forge_task_duration_seconds{")
                && name.contains(&format!("result=\"{result}\""))
        })
        .collect();
    assert_eq!(durations.len(), 1, "one labelled duration series");
    let duration = durations[0].1;
    assert_eq!(duration.count, 1);
    assert!(
        u128::from(duration.max) >= held.as_nanos(),
        "ownership duration={duration:?}, held={held:?}"
    );
    duration.max
}

/// Public Forge metrics describe the data flow each route actually performed.
///
/// The four retained routes are driven through the real scheduler and worker
/// until each has planned, claimed, and settled a durable task of its own,
/// including the worker restarts that cancel an in-flight attempt. Every route
/// must appear in the created and attempted counters under its own
/// `task_type`, and the RAII active-task gauge must return to zero on every
/// exit — commit, refusal, cancellation, or unwind. The same pass rejects any
/// ownership label, so an unbounded identity cannot reach production unnoticed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn forge_metrics_describe_real_data_flow() {
    let telemetry = ForgeTelemetryCheckpoint::install();
    let mut table = expirable_table("telemetry_exits", true).await;
    // Promotion settles while the fixture starts, and the checkpoint is already
    // installed, so all five production routes are observable from one capture.
    let routes = [
        "scribe_promotion",
        "small_files",
        "snapshot_expiry",
        "expired_cleanup",
        "orphan_cleanup",
    ];
    for _ in 0..8 {
        let seen = settled_tasks(&table.fixture)
            .await
            .into_iter()
            .map(|(_, strategy, _)| strategy)
            .collect::<std::collections::BTreeSet<_>>();
        if routes.iter().all(|strategy| seen.contains(*strategy)) {
            break;
        }
        table.supervised.restart_worker();
        table.supervised.run_one_success().await;
    }

    let snapshot = telemetry.snapshot();
    for task_type in routes {
        assert!(
            counter_total(
                &snapshot,
                "bifrost_forge_tasks_created_total",
                &[("task_type", task_type)],
            ) > 0,
            "{task_type} reported the work the coordinator created"
        );
        assert!(
            counter_total(
                &snapshot,
                "bifrost_forge_task_attempts_total",
                &[("task_type", task_type)],
            ) > 0,
            "{task_type} reported the attempt it settled"
        );
    }

    // Attempts carry only the six durable results, so an unlisted result is a
    // vocabulary drift rather than a new outcome.
    for name in snapshot
        .counters
        .keys()
        .filter(|name| name.starts_with("bifrost_forge_task_attempts_total{"))
    {
        assert!(
            [
                "succeeded",
                "retry",
                "failed",
                "cancelled",
                "refused",
                "uncertain",
            ]
            .iter()
            .any(|result| name.contains(&format!("result=\"{result}\""))),
            "{name} carries an unlisted result"
        );
    }

    assert_route_data_flow(&snapshot, routes, table.store.deletes());

    // The guard decrements on every exit, so a settled route that still owns
    // active work is a leaked attempt rather than a slow one.
    for (name, value) in &snapshot.gauges {
        if name.starts_with("bifrost_forge_active_tasks{") {
            assert!(
                value.abs() < f64::EPSILON,
                "{name} retained {value} active attempts"
            );
        }
    }

    // Production emission must stay inside the closed schema the owner
    // registers; an unbounded identity here is a cardinality incident.
    for name in snapshot
        .counters
        .keys()
        .chain(snapshot.gauges.keys())
        .chain(snapshot.histograms.keys())
        .filter(|name| name.starts_with("bifrost_forge_"))
    {
        for label in ["table=", "tenant=", "task=", "owner=", "prefix="] {
            assert!(!name.contains(label), "{name} carries {label}");
        }
    }
    telemetry.require_metrics(&[
        "bifrost_forge_tasks_created_total",
        "bifrost_forge_task_attempts_total",
        "bifrost_forge_task_duration_seconds",
        "bifrost_forge_active_tasks",
        "bifrost_forge_pending_tasks",
        "bifrost_forge_planning_demands",
    ]);
}

/// The production coordinator and worker collect exactly one rowless generation.
///
/// A real managed rewrite is refused before its `Prepared` operation row
/// commits, so the outputs it already closed are named by no snapshot, no
/// operation row, and no audit transition. The route that removes them is the
/// ordinary one: the production scheduler plans an orphan-cleanup task and a
/// separately owned production worker claims and executes it. Nothing here
/// calls the retained collector directly, so the proof covers admission,
/// dispatch, delete, and cursor settlement rather than the collector alone.
///
/// The fixture runs on the system clock with a 50 ms age floor, so object age
/// and the SQL demand cutoff share one wall-clock basis and the young-survival
/// assertion is real rather than seeded.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn coordinator_and_worker_delete_only_exact_never_published_generation() {
    let mut promoted = PromotedRewriteFixture::start_unpromoted("orphan_route").await;
    promoted.fixture.config.orphan_gc_ttl = Duration::from_millis(50);
    // One entry per page and one page per pass, so the route must checkpoint a
    // durable cursor and resume it across the worker restarts below.
    promoted.fixture.config.orphan_gc_max_list_pages = 1;
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    object_store.page_listing_by(1);
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    let mut supervisor = SupervisedPromotion::start_serial(
        &promoted.fixture,
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn vala_bifrost_redux::forge::ForgeObjectStore>,
        vala_bifrost_redux::forge::ForgeClock::system(),
    );
    supervisor.run_one_success().await;

    // One real managed rewrite closes its outputs and is then refused by its
    // own cancellation token, the last authority checked before the `Prepared`
    // operation row would commit.
    supervisor.restart_worker();
    let worker_stop = supervisor.worker_stop();
    let refusal = supervisor
        .run_one_failure_holding_handoff(async {
            worker_stop.cancel();
        })
        .await;
    let rowless: Vec<String> = supervisor
        .last_possible_rewrite_outputs()
        .expect("the refused rewrite reported its possible outputs")
        .iter()
        .filter(|output| output.settled)
        .map(|output| {
            let prefix = &promoted.fixture.binding.object_prefix;
            let at = output
                .path
                .find(prefix.as_str())
                .expect("a produced output is inside its own table binding");
            output.path[at..].to_owned()
        })
        .collect();
    assert!(
        !rowless.is_empty(),
        "the refused rewrite closed at least one real output: {refusal}"
    );

    // Every safety root the collector must never address.
    let lookalikes = super::orphan_cleanup::seed_lookalikes(&promoted.fixture).await;
    let protected = super::orphan_cleanup::protected_forge_outputs(&promoted.fixture).await;
    let live = promoted.fixture.live_data_paths().await;
    assert!(!live.is_empty(), "the promoted live set is a safety root");

    // Drive the real coordinator and worker until an orphan-cleanup task has
    // settled. Earlier passes still owe compaction, so the loop also proves the
    // orphan route is reached without pre-empting the strategies ahead of it.
    let (task_id, cursors) = drive_until_orphan_settles(&mut supervisor, &promoted.fixture).await;

    assert_only_the_rowless_generation_is_gone(
        &promoted.fixture,
        &rowless,
        &lookalikes,
        &protected,
        &live,
    )
    .await;

    // The durable cursor survives the worker restarts the loop already made.
    let (state, evidence) =
        super::orphan_cleanup::orphan_task_row(&promoted.fixture, task_id).await;
    assert_eq!(state, "succeeded");
    assert!(
        evidence.is_none(),
        "an exhausted pass retires its cursor: {evidence:?}"
    );
    // The route checkpointed a durable resume point inside its own recipe root
    // while the worker was restarted around it, and never moved that point
    // backward. Cursor page ordering itself is owned by
    // `orphan_cleanup::bounded_retry_resumes_after_cursor_without_starvation`.
    assert!(
        !cursors.is_empty(),
        "the bounded route checkpointed no resumable cursor"
    );
    let root = format!("{}/data/forge/v1", promoted.fixture.binding.object_prefix);
    assert!(
        cursors.iter().all(|cursor| cursor.starts_with(&root)),
        "a durable cursor left the recipe root: {cursors:?}"
    );
    assert!(
        cursors.windows(2).all(|pair| pair[0] < pair[1]),
        "a restarted worker replayed or rewound its durable cursor: {cursors:?}"
    );

    // No destructive sibling route ran: the table never owed an expiration or a
    // cleanup handoff, so nothing but collection could have removed an object.
    let observed = settled_tasks(&promoted.fixture).await;
    assert!(
        observed
            .iter()
            .all(|(_, strategy, _)| strategy != "snapshot_expiry" && strategy != "expired_cleanup"),
        "a sibling destructive route ran beside collection: {observed:?}"
    );

    // Identity is orphan-only: no other route wrote an operation row while
    // this one ran.
    let operations = super::orphan_cleanup::orphan_operations(&promoted.fixture).await;
    assert!(
        !operations.is_empty(),
        "the collection route owns its own operation identity"
    );
    assert!(
        operations.iter().all(|(_, phase)| !phase.is_empty()),
        "the collection route left a settled operation phase: {operations:?}"
    );

    supervisor.shutdown().await;
}

/// Asserts each production route reported the data flow it actually performed.
///
/// Split out of the driving test so each boundary — duration, file and byte
/// throughput, expiration, deletion, and the active gauge's rise — reads as one
/// statement about one family rather than as one long block.
///
/// # Panics
///
/// Panics when a route reported no duration, no throughput, a deletion it did
/// not perform, or never held an active attempt.
fn assert_route_data_flow(
    snapshot: &wyrd_bench::BenchmarkMetricSnapshot,
    routes: [&str; 5],
    real_deletes: usize,
) {
    // Every settled route measured the episode it owned, so a missing duration
    // observation is an attempt counted without the latency it actually took.
    for task_type in routes {
        let observed: u64 = snapshot
            .histograms
            .iter()
            .filter(|(name, _)| {
                name.starts_with("bifrost_forge_task_duration_seconds{")
                    && name.contains(&format!("task_type=\"{task_type}\""))
            })
            .map(|(_, value)| value.count)
            .sum();
        assert!(observed > 0, "{task_type} observed no attempt duration");
    }

    // Physical data flow is reported only by the routes that move files, and
    // only under the durable strategy that committed the effect.
    for family in [
        "bifrost_forge_input_files_total",
        "bifrost_forge_input_bytes_total",
        "bifrost_forge_output_files_total",
        "bifrost_forge_output_bytes_total",
    ] {
        for task_type in ["scribe_promotion", "small_files"] {
            assert!(
                counter_total(snapshot, family, &[("task_type", task_type)]) > 0,
                "{family} reported nothing for {task_type}"
            );
        }
        for name in snapshot
            .counters
            .keys()
            .filter(|name| name.starts_with(&format!("{family}{{")))
        {
            assert!(
                name.contains("task_type=\"scribe_promotion\"")
                    || name.contains("task_type=\"small_files\""),
                "{name} claims data flow for a route that rewrites nothing"
            );
        }
    }

    // Expiration counts the individual snapshots it removed, unlabelled because
    // only one route can remove one.
    assert!(
        snapshot
            .counters
            .get("bifrost_forge_snapshots_expired_total")
            .copied()
            .unwrap_or_default()
            > 0,
        "the settled expiration reported no removed snapshot"
    );

    // Deletions are emitted at the delete boundary itself, so the counter can
    // never exceed the deletes the real object store was actually asked for,
    // and only the two cleanup routes delete anything.
    let deleted = counter_total(snapshot, "bifrost_forge_deleted_objects_total", &[]);
    assert!(
        deleted <= real_deletes as u64,
        "{deleted} deletions were counted for {real_deletes} real object-store deletes"
    );
    for name in snapshot
        .counters
        .keys()
        .filter(|name| name.starts_with("bifrost_forge_deleted_objects_total{"))
    {
        assert!(
            name.contains("task_type=\"expired_cleanup\"")
                || name.contains("task_type=\"orphan_cleanup\""),
            "{name} claims a deletion for a route that deletes nothing"
        );
    }

    // The active gauge rose while each route owned its claim; the balance
    // assertion below then proves it came back down.
    for task_type in routes {
        let peak = snapshot
            .gauge_peaks
            .iter()
            .filter(|(name, _)| {
                name.starts_with("bifrost_forge_active_tasks{")
                    && name.contains(&format!("task_type=\"{task_type}\""))
            })
            .fold(0.0_f64, |peak, (_, value)| peak.max(*value));
        assert!(peak > 0.0, "{task_type} never held an active attempt");
    }
}

/// Drives the real coordinator and worker until an orphan-cleanup task settles.
///
/// Earlier passes still owe compaction, so this also proves the orphan route is
/// reached without pre-empting the strategies ahead of it. Returns the settled
/// task id and every distinct durable cursor observed along the way.
///
/// # Panics
///
/// Panics when no orphan-cleanup task settles within the bounded pass budget.
async fn drive_until_orphan_settles(
    supervisor: &mut SupervisedPromotion,
    fixture: &PromotionIntegrationFixture,
) -> (uuid::Uuid, Vec<String>) {
    let mut settled_orphan = None;
    let mut cursors: Vec<String> = Vec::new();
    for _ in 0..64 {
        supervisor.schedule_only().await;
        let tasks = settled_tasks(fixture).await;
        if let Some((task_id, _, _)) = tasks
            .iter()
            .find(|(_, strategy, state)| strategy == "orphan_cleanup" && state == "succeeded")
        {
            settled_orphan = Some(*task_id);
            break;
        }
        // Retry backoff is wall-clock and unrelated to what this scenario
        // proves, so the existing fixture aging retires it instead of sleeping.
        fixture.clear_task_backoff().await;
        let claimable: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1 \
             AND state IN ('ready','retryable') AND ready_at <= now()",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("claimable Forge tasks are readable");
        if claimable > 0 {
            supervisor.restart_worker();
            supervisor.settle_some_success().await;
        }
        if let Some((orphan_id, _, _)) = tasks
            .iter()
            .find(|(_, strategy, _)| strategy == "orphan_cleanup")
        {
            let (_, evidence) = super::orphan_cleanup::orphan_task_row(fixture, *orphan_id).await;
            if evidence.is_some() {
                let cursor = super::orphan_cleanup::cursor_of(evidence.as_ref());
                if cursors.last() != Some(&cursor) {
                    cursors.push(cursor);
                }
            }
        }
    }
    let observed = settled_tasks(fixture).await;
    let task_id = settled_orphan.unwrap_or_else(|| {
        panic!("the production route settles one orphan-cleanup task; saw {observed:?}")
    });
    (task_id, cursors)
}

/// Asserts collection removed the rowless generation and nothing else.
///
/// # Panics
///
/// Panics when a never-published output survived or any protected, live, young,
/// invalid, or foreign object was collected.
async fn assert_only_the_rowless_generation_is_gone(
    fixture: &PromotionIntegrationFixture,
    rowless: &[String],
    lookalikes: &[String],
    protected: &[String],
    live: &std::collections::BTreeSet<String>,
) {
    // Only the exact rowless generation is gone.
    for path in rowless {
        assert!(
            !object_exists(fixture, path).await,
            "the never-published output {path} survived its own collection route"
        );
    }
    for path in lookalikes.iter().chain(protected.iter()).chain(live.iter()) {
        assert!(
            object_exists(fixture, path).await,
            "a protected, live, or invalid object was collected: {path}"
        );
    }
}

/// A retained nonexistent-table demand cannot hide the authoritative roster or
/// monopolize the next bounded page; a closed failed cycle retries it later.
///
/// # Panics
/// Panics when healthy discovery starves or a failed cycle publishes inventory.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn scheduler_discovers_roster_despite_failed_demand() {
    let _telemetry = ForgeTelemetryCheckpoint::install();
    let mut fixture = PromotionIntegrationFixture::start("cycle_healthy").await;
    fixture.config.max_hints_per_wake = 1;
    let store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let forge = fixture.build_forge_for_test(
        fixture.catalog.iceberg_catalog(),
        store,
        vala_bifrost_redux::forge::ForgeClock::system(),
        vala_bifrost_redux::forge::ForgeWorkerCompletionObserver::default(),
        vala_bifrost_redux::forge::ForgeSchedulerTrigger::default(),
    );
    let tasks = ForgeTasks::new(fixture.operator_pool.clone());
    let missing = ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "missing")
        .expect("valid nonexistent identity");
    tasks
        .upsert_periodic(fixture.tenant, &missing)
        .await
        .expect("failing demand");
    let mut scheduler = ForgeScheduler::new(&forge).expect("scheduler");
    let stop = tokio_util::sync::CancellationToken::new();
    let first = scheduler
        .schedule_once(&stop)
        .await
        .expect("first bounded wake");
    assert!(
        first.incomplete,
        "missing table invalidates cycle: {first:?}"
    );
    let second = scheduler
        .schedule_once(&stop)
        .await
        .expect("second bounded wake");
    assert_eq!(
        second.demands_acknowledged, 1,
        "healthy roster member must be planned: {first:?}, {second:?}"
    );
    assert!(
        second.incomplete,
        "earlier failure remains sticky: {second:?}"
    );
    let remaining: Vec<String> =
        sqlx::query_scalar("SELECT table_name FROM vala.forge_planning_demands")
            .fetch_all(fixture.operator_pool.pool())
            .await
            .expect("remaining demand");
    assert_eq!(
        remaining,
        vec!["missing"],
        "healthy demand acknowledged without deleting failure"
    );
    let third = scheduler
        .schedule_once(&stop)
        .await
        .expect("fresh retry cycle");
    assert_eq!(third.demands_seen, 1);
    assert_eq!(
        third.demands_acknowledged, 0,
        "failed demand retried in next cycle: {third:?}"
    );
    assert!(third.incomplete);
    assert_eq!(scheduler.complete_publications_for_test(), 0);
}

/// Inventory includes live rewrite inputs even while promotion has admission priority.
///
/// # Panics
/// Panics when exact production-discovered debt or its published gauge differs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn scheduler_debt_includes_rewrite_and_promotion() {
    let telemetry = ForgeTelemetryCheckpoint::install();
    let fixture = PromotionIntegrationFixture::start("cycle_debt").await;
    let store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let forge = fixture.build_forge_for_test(
        fixture.catalog.iceberg_catalog(),
        store,
        ForgeClock::system(),
        ForgeWorkerCompletionObserver::default(),
        ForgeSchedulerTrigger::default(),
    );
    let mut scheduler = ForgeScheduler::new(&forge).expect("scheduler");
    let stop = CancellationToken::new();
    let bytes: i64 = sqlx::query_scalar("SELECT sum(file_size)::bigint FROM vala.file_list")
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("exact sealed input bytes");
    let promotion = scheduler
        .schedule_once(&stop)
        .await
        .expect("promotion discovery");
    assert!(!promotion.incomplete);
    assert_eq!(
        (
            promotion.compaction_debt_files,
            promotion.compaction_debt_bytes
        ),
        (2, u64::try_from(bytes).expect("nonnegative file bytes"))
    );
    let worker = ForgeWorker::new(
        Arc::clone(&forge),
        ForgeWorkerConfig::default(),
        Uuid::now_v7(),
    )
    .expect("production worker");
    assert!(
        worker
            .execute_one_for_test(&stop)
            .await
            .expect("real promotion"),
        "the promotion task is claimed; tasks at {}: {:?}",
        chrono::Utc::now(),
        fixture.forge_tasks().await
    );
    let executed = telemetry.snapshot();
    assert_eq!(
        counter_total(&executed, "bifrost_forge_task_attempts_total", &[]),
        1,
        "the execute-one fixture adapter observes exactly one ordinary episode"
    );
    assert_eq!(
        counter_total(
            &executed,
            "bifrost_forge_task_attempts_total",
            &[("result", "succeeded")]
        ),
        1
    );
    let rewrite = scheduler
        .schedule_once(&stop)
        .await
        .expect("rewrite discovery");
    assert!(!rewrite.incomplete);
    assert_eq!(
        (rewrite.compaction_debt_files, rewrite.compaction_debt_bytes),
        (2, u64::try_from(bytes).expect("nonnegative file bytes")),
        "live rewrite debt survives hot promotion settlement"
    );
    fixture.seal_more(2).await;
    let combined_bytes: i64 =
        sqlx::query_scalar("SELECT sum(file_size)::bigint FROM vala.file_list")
            .fetch_one(fixture.operator_pool.pool())
            .await
            .expect("hot plus live bytes");
    let combined = scheduler
        .schedule_once(&stop)
        .await
        .expect("simultaneous candidates");
    assert!(!combined.incomplete);
    assert_eq!(
        (
            combined.compaction_debt_files,
            combined.compaction_debt_bytes
        ),
        (
            4,
            u64::try_from(combined_bytes).expect("nonnegative file bytes")
        )
    );
    let snapshot = telemetry.snapshot();
    assert_inventory(
        &snapshot,
        4,
        u64::try_from(combined_bytes).expect("file bytes"),
    );
    eprintln!("debt promotion/rewrite=(2,{bytes}); simultaneous=(4,{combined_bytes})");
}

/// Bounded pages replace a complete inventory only after all unequal table debts
/// are observed; failed cycles retain it, and an empty complete roster clears it.
///
/// # Panics
/// Panics on a page subtotal, lost sticky failure, or a stale empty inventory.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn scheduler_publishes_complete_cycle_inventory() {
    let telemetry = ForgeTelemetryCheckpoint::install();
    let mut fixture = PromotionIntegrationFixture::start("a_inventory").await;
    fixture.config.max_hints_per_wake = 1;
    let store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let forge = fixture.build_forge_for_test(
        fixture.catalog.iceberg_catalog(),
        store,
        ForgeClock::system(),
        ForgeWorkerCompletionObserver::default(),
        ForgeSchedulerTrigger::default(),
    );
    let mut scheduler = ForgeScheduler::new(&forge).expect("scheduler");
    let stop = CancellationToken::new();
    let previous = scheduler
        .schedule_once(&stop)
        .await
        .expect("initial complete inventory");
    assert!(!previous.incomplete);
    assert_eq!(previous.compaction_debt_files, 2);
    fixture.seal_more(1).await;
    fixture.register_and_seal_table("b_inventory", 4).await;
    let expected_bytes: i64 =
        sqlx::query_scalar("SELECT sum(file_size)::bigint FROM vala.file_list")
            .fetch_one(fixture.operator_pool.pool())
            .await
            .expect("all seven real input sizes");
    let first = scheduler.schedule_once(&stop).await.expect("first page");
    assert!(first.incomplete);
    assert_eq!(first.demands_acknowledged, 1);
    assert_inventory(&telemetry.snapshot(), 2, previous.compaction_debt_bytes);
    let second = scheduler.schedule_once(&stop).await.expect("second page");
    assert!(!second.incomplete);
    assert_eq!(second.demands_acknowledged, 1);
    assert_eq!(
        (second.compaction_debt_files, second.compaction_debt_bytes),
        (
            7,
            u64::try_from(expected_bytes).expect("nonnegative file bytes")
        )
    );
    assert_inventory(
        &telemetry.snapshot(),
        7,
        u64::try_from(expected_bytes).expect("file bytes"),
    );
    // A missing table remains demanded across closed failed cycles. Its absence
    // from the roster must not remove it from the attempted exclusion set.
    let tasks = ForgeTasks::new(fixture.operator_pool.clone());
    let missing = ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "missing_inventory")
        .expect("identity");
    tasks
        .upsert_periodic(fixture.tenant, &missing)
        .await
        .expect("missing demand");
    for _ in 0..3 {
        let failed = scheduler
            .schedule_once(&stop)
            .await
            .expect("failed cycle page");
        assert!(
            failed.incomplete,
            "failure sticks through exhaustion: {failed:?}"
        );
        assert_inventory(
            &telemetry.snapshot(),
            7,
            u64::try_from(expected_bytes).expect("file bytes"),
        );
    }
    // Retire the tables and the intentionally nonexistent fixture demand to
    // present a genuinely empty authoritative roster to a new complete cycle.
    let admin = fixture
        .database
        .superuser_pool()
        .await
        .expect("fixture administrator");
    sqlx::query("UPDATE vala.bifrost_tables SET status='deprecated'")
        .execute(&admin)
        .await
        .expect("retired roster");
    sqlx::query("DELETE FROM vala.forge_planning_demands")
        .execute(fixture.operator_pool.pool())
        .await
        .expect("retire fixture demands");
    let empty = scheduler
        .schedule_once(&stop)
        .await
        .expect("empty complete cycle");
    assert!(!empty.incomplete);
    assert_eq!(
        (empty.compaction_debt_files, empty.compaction_debt_bytes),
        (0, 0)
    );
    assert_inventory(&telemetry.snapshot(), 0, 0);
    eprintln!(
        "inventory previous=(2,{}); first page retains previous; complete=(7,{expected_bytes}); failed pages retain complete; empty=(0,0)",
        previous.compaction_debt_bytes
    );
}

/// A malformed known payload terminalizes and reports exactly one durable refusal
/// through the same entrypoint used by fixture adapters.
///
/// # Panics
/// Panics when qualification, failure count, attempt count, or duration is absent.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn worker_malformed_known_payload_observes_one_refusal() {
    let telemetry = ForgeTelemetryCheckpoint::install();
    let fixture = PromotionIntegrationFixture::start("malformed_episode").await;
    let store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let forge = fixture.build_forge_for_test(
        fixture.catalog.iceberg_catalog(),
        store,
        ForgeClock::system(),
        ForgeWorkerCompletionObserver::default(),
        ForgeSchedulerTrigger::default(),
    );
    let mut scheduler = ForgeScheduler::new(&forge).expect("scheduler");
    let stop = CancellationToken::new();
    scheduler
        .schedule_once(&stop)
        .await
        .expect("real promotion plan");
    sqlx::query("UPDATE vala.forge_tasks SET plan=jsonb_set(plan,'{inputs}','[]')")
        .execute(fixture.operator_pool.pool())
        .await
        .expect("malformed known plan");
    let worker =
        ForgeWorker::new(forge, ForgeWorkerConfig::default(), Uuid::now_v7()).expect("worker");
    assert!(
        worker
            .execute_one_for_test(&stop)
            .await
            .expect("terminalized payload"),
        "the malformed task is claimed; tasks at {}: {:?}",
        chrono::Utc::now(),
        fixture.forge_tasks().await
    );
    let state: (String, Option<String>, i32) =
        sqlx::query_as("SELECT state,failure_class,attempt_count FROM vala.forge_tasks")
            .fetch_one(fixture.operator_pool.pool())
            .await
            .expect("durable classification");
    assert_eq!(
        state,
        ("failed".to_owned(), Some("data_refusal".to_owned()), 1)
    );
    let snapshot = telemetry.snapshot();
    assert_eq!(
        counter_total(
            &snapshot,
            "bifrost_forge_task_failures_total",
            &[
                ("task_type", "scribe_promotion"),
                ("reason", "data_refusal")
            ]
        ),
        1
    );
    assert_eq!(
        counter_total(
            &snapshot,
            "bifrost_forge_task_attempts_total",
            &[("task_type", "scribe_promotion"), ("result", "refused")]
        ),
        1
    );
    assert_eq!(
        snapshot
            .histograms
            .iter()
            .filter(|(name, _)| name.starts_with("bifrost_forge_task_duration_seconds{"))
            .map(|(_, value)| value.count)
            .sum::<u64>(),
        1
    );
    assert!(
        active_tasks(&snapshot).abs() < f64::EPSILON,
        "ownership is complete"
    );
}

/// Prepared recovery holds active ownership at the claim gate and emits exactly
/// one completed episode after reconciliation and release.
///
/// # Panics
/// Panics on missing ownership, duplicate/missing telemetry, or lost durable evidence.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn worker_prepared_recovery_observes_one_ownership_episode() {
    let telemetry = ForgeTelemetryCheckpoint::install();
    let fixture = PromotionIntegrationFixture::start("prepared_episode").await;
    let observer = ForgeWorkerCompletionObserver::default();
    let store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let forge = fixture.build_forge_for_test(
        fixture.catalog.iceberg_catalog(),
        store,
        ForgeClock::system(),
        observer.clone(),
        ForgeSchedulerTrigger::default(),
    );
    let mut scheduler = ForgeScheduler::new(&forge).expect("scheduler");
    let stop = CancellationToken::new();
    scheduler
        .schedule_once(&stop)
        .await
        .expect("real promotion plan");
    fixture.prepare_recovery_episode(&forge, &stop).await;
    let state = "prepared";
    observer.hold_after_claims_for_test(1);
    observer.hold_after_next_attempt_for_test();
    let before = telemetry.snapshot();
    let worker = ForgeWorker::new(forge, ForgeWorkerConfig::default(), Uuid::now_v7())
        .expect("recovering worker");
    let work_stop = stop.clone();
    let mut work = AbortOnDropHandle::new(tokio::spawn(
        worker.run(work_stop, ForgeRoleReadiness::default()),
    ));
    if timeout(OWNERSHIP_BOUND, observer.wait_for_claims_for_test())
        .await
        .is_err()
    {
        stop.cancel();
        work.abort();
        panic!(
            "Prepared claim gate missed; state={state}, attempts={}",
            observer.attempts()
        );
    }
    let held_at = Instant::now();
    let active = active_tasks(&telemetry.snapshot());
    let claimed: String = sqlx::query_scalar("SELECT state FROM vala.forge_tasks")
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("owned Prepared state");
    let held = held_at.elapsed();
    observer.release_claims_for_test();
    if timeout(OWNERSHIP_BOUND, observer.wait_for_held_attempt_for_test())
        .await
        .is_err()
    {
        stop.cancel();
        work.abort();
        panic!(
            "Prepared completion gate missed; last state={claimed}, attempts={}",
            observer.attempts()
        );
    }
    let after = telemetry.snapshot();
    stop.cancel();
    observer.release_held_attempt_for_test();
    if let Ok(result) = timeout(OWNERSHIP_BOUND, &mut work).await {
        result.expect("worker join").expect("recovery drains");
    } else {
        work.abort();
        panic!("Prepared worker failed to drain; last state={claimed}");
    }
    assert!(
        (active - 1.0).abs() < f64::EPSILON,
        "claimed ownership is active"
    );
    assert_eq!(claimed, "prepared");
    assert_completed_episode(
        &after,
        counter_total(&before, "bifrost_forge_task_attempts_total", &[]),
        "succeeded",
        held,
    );
}

/// Shutdown before execution balances ownership and reports the durable Retryable
/// release, even though that transition clears the attempt identity.
///
/// # Panics
/// Panics on unbalanced ownership or a fabricated cancelled outcome.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn worker_pre_execution_cancellation_reports_durable_retry() {
    let telemetry = ForgeTelemetryCheckpoint::install();
    let fixture = PromotionIntegrationFixture::start("cancel_episode").await;
    let observer = ForgeWorkerCompletionObserver::default();
    observer.hold_after_claims_for_test(1);
    let stop = CancellationToken::new();
    let worker = fixture.plan_worker_for_test(observer.clone(), &stop).await;
    let mut work = AbortOnDropHandle::new(tokio::spawn(
        worker.run(stop.clone(), ForgeRoleReadiness::default()),
    ));
    if timeout(OWNERSHIP_BOUND, observer.wait_for_claims_for_test())
        .await
        .is_err()
    {
        stop.cancel();
        work.abort();
        panic!(
            "ordinary claim gate missed; last task state=ready, attempts={}",
            observer.attempts()
        );
    }
    let active = active_tasks(&telemetry.snapshot());
    stop.cancel();
    observer.release_claims_for_test();
    if let Ok(result) = timeout(OWNERSHIP_BOUND, &mut work).await {
        result.expect("worker join").expect("cancellation release");
    } else {
        work.abort();
        panic!("cancelled claim failed to drain; last task state=claimed, active={active}");
    }
    let state: (String, Option<Uuid>) =
        sqlx::query_as("SELECT state,attempt_id FROM vala.forge_tasks")
            .fetch_one(fixture.operator_pool.pool())
            .await
            .expect("released claim");
    assert_eq!(state, ("retryable".to_owned(), None));
    assert!(
        (active - 1.0).abs() < f64::EPSILON,
        "claimed ownership is active"
    );
    let after = telemetry.snapshot();
    assert!(
        active_tasks(&after).abs() < f64::EPSILON,
        "ownership is complete"
    );
    assert_eq!(
        counter_total(&after, "bifrost_forge_task_attempts_total", &[]),
        1
    );
    assert_eq!(
        counter_total(
            &after,
            "bifrost_forge_task_attempts_total",
            &[("result", "retry")]
        ),
        1
    );
    assert_eq!(
        counter_total(
            &after,
            "bifrost_forge_task_attempts_total",
            &[("result", "cancelled")]
        ),
        0
    );
    assert_eq!(
        after
            .histograms
            .iter()
            .filter(|(name, _)| name.starts_with("bifrost_forge_task_duration_seconds{"))
            .map(|(_, value)| value.count)
            .sum::<u64>(),
        1
    );
}

/// Ordinary ownership timing includes held claim and settled-but-unreleased lease
/// intervals; a release failure preserves the known successful durable result.
///
/// # Panics
/// Panics when duration omits either boundary, ownership leaks, or emission duplicates.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn worker_duration_includes_claim_and_failed_release() {
    let telemetry = ForgeTelemetryCheckpoint::install();
    let fixture = PromotionIntegrationFixture::start("duration_episode").await;
    let observer = ForgeWorkerCompletionObserver::default();
    observer.hold_after_claims_for_test(1);
    observer.hold_before_next_lease_release_for_test();
    observer.hold_after_next_attempt_for_test();
    observer.fail_next_lease_release();
    let stop = CancellationToken::new();
    let worker = fixture.plan_worker_for_test(observer.clone(), &stop).await;
    let mut work = AbortOnDropHandle::new(tokio::spawn(
        worker.run(stop.clone(), ForgeRoleReadiness::default()),
    ));
    if timeout(OWNERSHIP_BOUND, observer.wait_for_claims_for_test())
        .await
        .is_err()
    {
        stop.cancel();
        work.abort();
        panic!(
            "claim gate missed; last task state=ready, attempts={}",
            observer.attempts()
        );
    }
    let claim_at = Instant::now();
    let active_claim = active_tasks(&telemetry.snapshot());
    let claimed: String = sqlx::query_scalar("SELECT state FROM vala.forge_tasks")
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("claimed state");
    let claim_held = claim_at.elapsed();
    observer.release_claims_for_test();
    if timeout(
        OWNERSHIP_BOUND,
        observer.wait_for_held_lease_release_for_test(),
    )
    .await
    .is_err()
    {
        stop.cancel();
        work.abort();
        panic!("release gate missed; last state={claimed}");
    }
    let release_at = Instant::now();
    let execution_bound = claim_at.elapsed();
    let active_release = active_tasks(&telemetry.snapshot());
    let mut settled = String::new();
    // Hold release longer than the observed claim-to-release work, using real
    // state reads and elapsed comparison rather than a fixed sleep duration.
    if timeout(OWNERSHIP_BOUND, async {
        loop {
            settled = sqlx::query_scalar("SELECT state FROM vala.forge_tasks")
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("settled state");
            if release_at.elapsed() > execution_bound {
                break;
            }
        }
    })
    .await
    .is_err()
    {
        stop.cancel();
        work.abort();
        panic!("release observation timed out; last state={settled}");
    }
    let release_held = release_at.elapsed();
    let observed_ownership = claim_at.elapsed();
    observer.release_held_lease_release_for_test();
    if timeout(OWNERSHIP_BOUND, observer.wait_for_held_attempt_for_test())
        .await
        .is_err()
    {
        stop.cancel();
        work.abort();
        panic!("completion gate missed; last state={settled}");
    }
    let after = telemetry.snapshot();
    observer.release_held_attempt_for_test();
    let error = if let Ok(result) = timeout(OWNERSHIP_BOUND, &mut work).await {
        result
            .expect("worker join")
            .expect_err("release failure must remain fatal")
    } else {
        stop.cancel();
        work.abort();
        panic!("release failure did not drain; last state={settled}");
    };
    assert!(
        error.to_string().contains("lease release failure"),
        "{error}"
    );
    assert_eq!(claimed, "claimed");
    assert_eq!(settled, "succeeded");
    assert_eq!((active_claim, active_release), (1.0, 1.0));
    let duration = assert_completed_episode(&after, 0, "succeeded", observed_ownership);
    eprintln!(
        "duration={}ns, claim gate={}ns, release gate={}ns; attempts=1 succeeded; active=1/1/0",
        duration,
        claim_held.as_nanos(),
        release_held.as_nanos()
    );
}

/// A complete roster observation removes disappeared debt and admits new tables
/// into an open cycle without replaying its already attempted demand identities.
///
/// # Panics
/// Panics when an open cycle publishes missing-table debt or omits a new member.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn scheduler_open_cycle_tracks_roster_changes() {
    let mut fixture = PromotionIntegrationFixture::start("a_disappears").await;
    fixture.config.max_hints_per_wake = 1;
    fixture.register_and_seal_table("b_remains", 2).await;
    let store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let forge = fixture.build_forge_for_test(
        fixture.catalog.iceberg_catalog(),
        store,
        ForgeClock::system(),
        ForgeWorkerCompletionObserver::default(),
        ForgeSchedulerTrigger::default(),
    );
    let mut scheduler = ForgeScheduler::new(&forge).expect("scheduler");
    let stop = CancellationToken::new();
    let first = scheduler
        .schedule_once(&stop)
        .await
        .expect("first roster page");
    assert!(first.incomplete);
    assert_eq!(first.compaction_debt_files, 2);
    let admin = fixture
        .database
        .superuser_pool()
        .await
        .expect("fixture administrator");
    sqlx::query(
        "UPDATE vala.bifrost_tables SET status='deprecated' WHERE fqn='vala.bifrost.a_disappears'",
    )
    .execute(&admin)
    .await
    .expect("retire observed table");
    fixture.register_and_seal_table("c_joins", 3).await;
    let second = scheduler
        .schedule_once(&stop)
        .await
        .expect("new roster member joins open cycle");
    assert!(second.incomplete);
    assert_eq!(
        second.compaction_debt_files, 2,
        "disappeared observation removed before publication"
    );
    let third = scheduler
        .schedule_once(&stop)
        .await
        .expect("new member completes cycle");
    assert!(!third.incomplete);
    let bytes: i64 = sqlx::query_scalar("SELECT sum(file_size)::bigint FROM vala.file_list WHERE table_name IN ('b_remains','c_joins')")
        .fetch_one(fixture.operator_pool.pool()).await.expect("surviving roster bytes");
    assert_eq!(
        (third.compaction_debt_files, third.compaction_debt_bytes),
        (5, u64::try_from(bytes).expect("nonnegative file bytes"))
    );
    eprintln!("roster debt pages: 2, 2 (removed old member), 5/{bytes} bytes complete");
}

/// A running slot discovering invalid Prepared evidence closes readiness before
/// held lease cleanup, preserves reconciliation over release, and observes uncertainty.
///
/// # Panics
/// Panics on late readiness loss, error replacement, or missing ownership observation.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn worker_prepared_fatal_closes_before_release_and_observation() {
    let telemetry = ForgeTelemetryCheckpoint::install();
    let fixture = PromotionIntegrationFixture::start("prepared_fatal").await;
    let observer = ForgeWorkerCompletionObserver::default();
    let forge = fixture.build_forge_for_test(
        fixture.catalog.iceberg_catalog(),
        CountingObjectStore::new(Arc::clone(&fixture.staging)),
        ForgeClock::system(),
        observer.clone(),
        ForgeSchedulerTrigger::default(),
    );
    let stop = CancellationToken::new();
    ForgeScheduler::new(&forge)
        .expect("scheduler")
        .schedule_once(&stop)
        .await
        .expect("plan");
    fixture.prepare_recovery_episode(&forge, &stop).await;
    sqlx::query("UPDATE vala.forge_tasks SET claim_expires_at=now()+interval '1 hour', evidence=jsonb_set(evidence,'{committed_snapshot_id}','999999')")
        .execute(fixture.operator_pool.pool()).await.expect("live foreign Prepared claim");
    let before = telemetry.snapshot();
    observer.hold_before_next_lease_release_for_test();
    observer.hold_before_fatal_observation_for_test();
    observer.fail_next_lease_release();
    let worker =
        ForgeWorker::new(forge, ForgeWorkerConfig::default(), Uuid::now_v7()).expect("worker");
    let readiness = ForgeRoleReadiness::default();
    let mut work =
        AbortOnDropHandle::new(tokio::spawn(worker.run(stop.clone(), readiness.clone())));
    let progress = timeout(OWNERSHIP_BOUND, async {
        while !readiness.is_ready() {
            tokio::task::yield_now().await;
        }
        sqlx::query("UPDATE vala.forge_tasks SET claim_expires_at=now()-interval '1 hour'")
            .execute(fixture.operator_pool.pool())
            .await
            .expect("recovery becomes eligible after startup");
        observer.wait_for_held_lease_release_for_test().await;
        assert!(
            !readiness.is_ready(),
            "known reconciliation failure closes before release"
        );
        assert!((active_tasks(&telemetry.snapshot()) - 1.0).abs() < f64::EPSILON);
        observer.release_held_lease_release_for_test();
        observer.wait_for_fatal_observation_for_test().await;
        assert!(!readiness.is_ready());
        assert!(!work.is_finished());
        observer.release_fatal_observation_for_test();
    })
    .await;
    if progress.is_err() {
        stop.cancel();
        work.abort();
        panic!(
            "Prepared fatal boundary timeout: ready={}, attempts={}, errors={:?}",
            readiness.is_ready(),
            observer.attempts(),
            observer.returned_errors()
        );
    }
    let error = if let Ok(result) = timeout(OWNERSHIP_BOUND, &mut work).await {
        result
            .expect("join")
            .expect_err("reconciliation remains fatal")
    } else {
        stop.cancel();
        work.abort();
        panic!(
            "Prepared observation did not drain: ready={}, attempts={}",
            readiness.is_ready(),
            observer.attempts()
        );
    };
    stop.cancel();
    assert!(
        error
            .to_string()
            .contains("Prepared evidence snapshot is no longer retained"),
        "{error}"
    );
    let after = telemetry.snapshot();
    assert!(active_tasks(&after).abs() < f64::EPSILON);
    assert_eq!(
        counter_total(
            &after,
            "bifrost_forge_task_attempts_total",
            &[("result", "uncertain")]
        ) - counter_total(
            &before,
            "bifrost_forge_task_attempts_total",
            &[("result", "uncertain")]
        ),
        1
    );
    assert_eq!(duration_count(&after) - duration_count(&before), 1);
    eprintln!(
        "Prepared fatal: ready=false while release held; reconciliation error preserved; uncertain attempt/duration delta=1; active=0"
    );
}

/// Reads the snapshot the fixture's table currently serves.
///
/// A promotion task is bound to the snapshot its planning pass observed, so a
/// scenario that has to build the task a *later* pass would have produced needs
/// the current one rather than the one it started from.
///
/// # Panics
/// Panics when the table cannot be loaded or serves no snapshot yet.
async fn current_snapshot_id(fixture: &PromotionIntegrationFixture) -> i64 {
    fixture
        .catalog
        .iceberg_catalog()
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("fixture table load")
        .metadata()
        .current_snapshot()
        .expect("the table serves a snapshot")
        .snapshot_id()
}

/// A promotion planned inside a sibling's settlement window is superseded.
///
/// Promotion lands in two durable steps — the Iceberg append, then the SQL
/// settlement that stops Oracle scanning those rows as hot — and a planning
/// pass can fall between them. The pass then observes rows that are still
/// promotable and a base snapshot that already contains them, and enqueues a
/// task whose whole group has in fact already been published.
///
/// The task built here is exactly that: the plan production arbitration
/// produced for the sealed rows, bound to the snapshot the first promotion
/// created. Claiming it must retire it as superseded before it promises
/// anything, because the alternative observed in production was a
/// reconciliation failure — the demand its plan names is gone, having been
/// settled by the sibling that already promoted it.
///
/// # Panics
/// Panics if the superseded claim raises a worker error, if any object is
/// appended twice, if the stale task is not retired, or if demand sealed
/// afterwards is not replanned and promoted.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn a_promotion_planned_in_the_settlement_window_is_superseded() {
    let fixture = PromotionIntegrationFixture::start("settle_window").await;
    let store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let forge = fixture.build_forge_for_test(
        fixture.catalog.iceberg_catalog(),
        store,
        ForgeClock::system(),
        ForgeWorkerCompletionObserver::default(),
        ForgeSchedulerTrigger::default(),
    );
    let stop = CancellationToken::new();
    let observed = Utc::now();
    // Captured before the promotion runs: once it settles SQL these rows are no
    // longer promotable, so arbitration would produce nothing to rebind.
    let planned = ForgeScheduler::with_owner_for_test(&forge, Uuid::now_v7())
        .expect("fixture scheduler")
        .arbitrate_demand_for_test(&demand(&fixture, observed))
        .await
        .expect("production arbitration runs")
        .into_iter()
        .find(|task| task.strategy == ForgeTaskStrategy::ScribePromotion)
        .expect("the sealed rows are owed a promotion");

    let mut scheduler = ForgeScheduler::new(&forge).expect("scheduler");
    scheduler
        .schedule_once(&stop)
        .await
        .expect("promotion discovery");
    let worker = ForgeWorker::new(
        Arc::clone(&forge),
        ForgeWorkerConfig::default(),
        Uuid::now_v7(),
    )
    .expect("production worker");
    assert!(
        worker
            .execute_one_for_test(&stop)
            .await
            .expect("the first promotion commits"),
        "the planned promotion is claimed"
    );
    let promoted = fixture.live_data_paths().await;
    assert!(
        !promoted.is_empty(),
        "the first promotion referenced its objects"
    );

    let in_window = NewForgeTask {
        base_snapshot_id: current_snapshot_id(&fixture).await,
        ..planned
    };
    enqueue(&fixture, &in_window, observed).await;
    assert!(
        worker
            .execute_one_for_test(&stop)
            .await
            .expect("a superseded promotion is retired without a worker error"),
        "the stale in-window task is claimed"
    );
    assert_eq!(
        fixture.live_data_paths().await,
        promoted,
        "a superseded promotion appends no object a second time"
    );
    let retired = fixture
        .forge_tasks()
        .await
        .into_iter()
        .filter(|task| task.base_snapshot_id == in_window.base_snapshot_id)
        .filter(|task| task.strategy == ForgeTaskStrategy::ScribePromotion.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        retired
            .iter()
            .map(|task| task.state.as_str())
            .collect::<Vec<_>>(),
        vec!["cancelled"],
        "the stale task is retired as cancelled, not failed: {retired:?}"
    );

    fixture.seal_more(2).await;
    scheduler
        .schedule_once(&stop)
        .await
        .expect("the remaining demand is replanned");
    assert!(
        worker
            .execute_one_for_test(&stop)
            .await
            .expect("the replanned promotion commits"),
        "the replanned promotion is claimed"
    );
    assert!(
        fixture.live_data_paths().await.len() > promoted.len(),
        "demand sealed after the superseded task is promoted"
    );
}
