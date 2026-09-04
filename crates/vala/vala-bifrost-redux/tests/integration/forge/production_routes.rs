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

use super::rewrite_support::PromotedRewriteFixture;
use super::snapshot_expiration::object_exists;
use super::snapshot_expiration::{
    ExpirableTable, expirable_table, head_watermark, seed_ready_expiry_task,
};
use super::support::{
    CountingObjectStore, ForgeTelemetryCheckpoint, PromotionCatalogSeam,
    PromotionIntegrationFixture, SupervisedPromotion, manual_clock,
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
fn counter_total(
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
    promoted.fixture.config.orphan_gc_ttl = std::time::Duration::from_millis(50);
    // One entry per page and one page per pass, so the route must checkpoint a
    // durable cursor and resume it across the worker restarts below.
    promoted.fixture.config.orphan_gc_max_list_pages = 1;
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    object_store.page_listing_by(1);
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    let mut supervisor = SupervisedPromotion::start(
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

    // Identity is orphan-only: no other route wrote an operation or audit row
    // while this one ran.
    let operations = super::orphan_cleanup::orphan_operations(&promoted.fixture).await;
    assert!(
        !operations.is_empty(),
        "the collection route owns its own operation identity"
    );
    let audits = super::orphan_cleanup::orphan_gc_audits(&promoted.fixture).await;
    assert!(
        audits
            .iter()
            .all(|entry| entry.starts_with("forge.orphan_gc.")),
        "the collection route wrote an audit belonging to another owner: {audits:?}"
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
