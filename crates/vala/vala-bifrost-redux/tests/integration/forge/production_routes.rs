//! Tier-2 coverage for Forge's independent production routes.
//!
//! One table has exactly one active-task slot. The elected leader dispatches
//! each strategy as its own task: promotions and compaction on demand, and
//! manifest rewrite, snapshot expiry, and cleanup on its maintenance timer.

use chrono::Duration as ChronoDuration;
use iceberg::spec::{FormatVersion, Operation};
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;
use uuid::Uuid;
use vala_bifrost_redux::forge::{
    DEFAULT_REPORT_TIMEOUT, Forge, ForgeClock, ForgeCommitNotice, ForgeCompactionDispatch,
    ForgeCompactionOutcome, ForgeCompactionType, ForgeObjectStore, ForgeRoleReadiness,
    ForgeSchedulerTrigger, ForgeTableKey, ForgeTableSettings, ForgeWorker,
    ForgeWorkerCompletionObserver, ForgeWorkerConfig,
};
use vala_sql::queries::forge_tasks::ForgeTasks;
use vala_sql::row_types::forge_tasks::{ForgeTaskStrategy, ForgeTaskTableIdentity, NewForgeTask};

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use wyrd_bench::BenchmarkMetricSnapshot;

use super::rewrite_support::PromotedRewriteFixture;
use super::snapshot_expiration::object_exists;
use super::snapshot_expiration::{ExpirableTable, expirable_table, head_watermark, maintain_once};
use super::support::{
    CountingObjectStore, ForgeTelemetryCheckpoint, PromotionCatalogSeam,
    PromotionIntegrationFixture, SupervisedPromotion, manual_clock, set_table_properties,
};

/// Maximum diagnostic wait for a claimed ownership episode.
const OWNERSHIP_BOUND: Duration = Duration::from_secs(15);

/// Promotes three times under `properties` and ages the clock past retention.
///
/// Each promotion is a fast append that adds one small data manifest, so the
/// head carries three fragmented manifests and two expirable ancestors.
/// Compaction is disabled so the leader owes the table no rewrite and every
/// maintenance step of the pass runs.
///
/// # Panics
///
/// Panics when a fixture dependency, promotion, or clock advance fails.
async fn fragmented_table(name: &str, properties: &[(&str, &str)]) -> ExpirableTable {
    let fixture = PromotionIntegrationFixture::start(name).await;
    let mut all = vec![("wyrd.forge.enable-compaction", "false")];
    all.extend_from_slice(properties);
    set_table_properties(&fixture.catalog, &fixture.binding, &all).await;
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
    for _ in 0..2 {
        fixture.seal_more(2).await;
        supervised.restart_worker();
        supervised.run_one_success().await;
    }
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

/// The leader timer merges fragmented manifests, then expires the old head.
///
/// The pass rewrites the three small manifests into one `replace` snapshot
/// first, so expiry runs against that new head and can retire the pre-pass
/// head. Had expiry run first, the pre-pass head would have been current at
/// expiry and survived. A retained member whose table does not exist fails in
/// the same pass without stopping the real table's maintenance.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn leader_timer_rewrites_manifests_before_expiry() {
    let table = fragmented_table(
        "timer_rewrite",
        &[
            ("wyrd.forge.enable-manifest-rewrite", "true"),
            ("commit.manifest.min-count-to-merge", "2"),
        ],
    )
    .await;
    let forge = table.supervised.forge();
    let missing = ForgeTableKey {
        tenant: table.fixture.tenant,
        table: ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "aaa_missing")
            .expect("valid nonexistent identity"),
    };
    forge
        .held_leader_term()
        .expect("the supervisor's pass holds the leader term")
        .schedule()
        .refresh_membership(
            &missing,
            &ForgeTableSettings {
                manifest_rewrite_enabled: true,
                ..ForgeTableSettings::default()
            },
        );

    let (before, after) = maintain_once(&table).await;
    assert!(
        before.data_manifests >= 3,
        "three fast appends fragment the head: {before:?}"
    );
    assert_eq!(after.operation, Operation::Replace, "{after:?}");
    assert_eq!(after.data_manifests, 1, "small manifests merged: {after:?}");
    assert!(
        !after.snapshots.contains(&before.snapshot_id),
        "expiry ran after the rewrite and retired the pre-pass head: {before:?} -> {after:?}"
    );
    assert!(after.snapshots.contains(&after.snapshot_id));
}

/// Without the manifest-rewrite opt-in the pass leaves manifests alone.
///
/// Snapshot expiry is on by default, so the same pass still retires the aged
/// ancestors of the unchanged head.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn leader_timer_skips_manifest_rewrite_when_disabled() {
    let table = fragmented_table("timer_no_rewrite", &[]).await;
    let (before, after) = maintain_once(&table).await;
    assert_eq!(after.snapshot_id, before.snapshot_id, "no rewrite commit");
    assert_eq!(after.data_manifests, before.data_manifests);
    assert!(
        after.snapshots.len() < before.snapshots.len(),
        "default expiry still retired aged ancestors: {before:?} -> {after:?}"
    );
}

/// A format-v3 table is skipped by manifest rewrite, as `RisingWave` does.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn leader_timer_skips_manifest_rewrite_on_format_v3() {
    let table = fragmented_table(
        "timer_v3",
        &[
            ("wyrd.forge.enable-manifest-rewrite", "true"),
            ("commit.manifest.min-count-to-merge", "2"),
            ("wyrd.forge.enable-snapshot-expiration", "false"),
        ],
    )
    .await;
    let catalog = table.fixture.catalog.iceberg_catalog();
    let loaded = catalog
        .load_table(&table.fixture.binding.table_ident())
        .await
        .expect("fixture table loads");
    let tx = Transaction::new(&loaded);
    tx.upgrade_table_version()
        .set_format_version(FormatVersion::V3)
        .apply(tx)
        .expect("upgrade applies")
        .commit(catalog.as_ref())
        .await
        .expect("the table upgrades to v3");
    let (before, after) = maintain_once(&table).await;
    assert_eq!(after.snapshot_id, before.snapshot_id, "no rewrite commit");
    assert_eq!(after.data_manifests, before.data_manifests);
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

/// Reads the settled states of every promotion bound to one base snapshot.
///
/// # Panics
///
/// Panics when the diagnostic task read fails.
async fn promotions_on_snapshot(
    fixture: &PromotionIntegrationFixture,
    base_snapshot_id: i64,
) -> Vec<String> {
    fixture
        .forge_tasks()
        .await
        .into_iter()
        .filter(|task| task.base_snapshot_id == base_snapshot_id)
        .filter(|task| task.strategy == ForgeTaskStrategy::ScribePromotion.as_str())
        .map(|task| task.state)
        .collect()
}

/// Inserts one ready task through the production enqueue statement.
///
/// # Panics
///
/// Panics when the insert is refused.
async fn enqueue(fixture: &PromotionIntegrationFixture, task: &NewForgeTask) {
    ForgeTasks::new(fixture.operator_pool.clone())
        .enqueue(task)
        .await
        .expect("the task enqueues");
}

/// Returns the exact promotion the fixture table owes.
///
/// # Panics
///
/// Panics when the task cannot be built or nothing is owed.
async fn owed_promotion(fixture: &PromotionIntegrationFixture, forge: &Forge) -> NewForgeTask {
    forge
        .promotion_task_for_test(fixture.tenant, &identity(fixture))
        .await
        .expect("promotion task builds")
        .expect("the sealed rows are owed a promotion")
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

/// Advances the fixture table by one production step.
///
/// While the leader owes the table a compaction, a worker pulls and settles
/// it; otherwise one leader maintenance pass runs expiry and cleanup.
///
/// # Panics
///
/// Panics when the step misses its deterministic bound or fails.
async fn advance_routes(table: &mut ExpirableTable) {
    let key = ForgeTableKey {
        tenant: table.fixture.tenant,
        table: identity(&table.fixture),
    };
    let owed = table
        .supervised
        .forge()
        .held_leader_term()
        .is_some_and(|term| term.schedule().owes_compaction(&key));
    if owed {
        table.supervised.restart_worker();
        table.supervised.run_one_success().await;
    } else {
        table.supervised.maintain_only().await;
    }
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
    let mut table = expirable_table("four_routes").await;
    // Promotion already settled twice while the fixture started, so the routes
    // still owed are compaction, expiration, expired cleanup, and orphan work.
    // Stopping the worker after each held attempt releases any sibling claim
    // it took meanwhile back to `retryable`, so a route is done only once every
    // row it owns has settled, not merely once it exists.
    let mut deletes_before_expiry = None;
    for _ in 0..12 {
        let settled = settled_tasks(&table.fixture).await;
        let seen = settled
            .iter()
            .map(|(_, strategy, _)| strategy.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let routes = [
            "small_files",
            "snapshot_expiry",
            "expired_cleanup",
            "orphan_cleanup",
        ];
        if routes.iter().all(|strategy| seen.contains(strategy))
            && settled
                .iter()
                .filter(|(_, strategy, _)| routes.contains(&strategy.as_str()))
                .all(|(_, _, state)| state == "succeeded")
        {
            break;
        }
        if deletes_before_expiry.is_none() && seen.contains("small_files") {
            deletes_before_expiry = Some(table.store.deletes());
        }
        table.fixture.clear_task_backoff().await;
        advance_routes(&mut table).await;
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
    // once the separate cleanup task claims them. A compaction the leader
    // dispatches after an expiry retires the head that expiry kept, which is
    // new retention debt, so every expiry — not exactly one — hands off its own.
    for (task_id, _, _) in settled
        .iter()
        .filter(|(_, strategy, _)| strategy == "snapshot_expiry")
    {
        let evidence: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT evidence FROM vala.forge_tasks WHERE task_id = $1")
                .bind(task_id)
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
            "every expiration handed off its candidates instead of deleting them"
        );
    }
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
    let mut table = expirable_table("telemetry_exits").await;
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
        advance_routes(&mut table).await;
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
    let root = format!(
        "{}/data/forge/{}",
        promoted.fixture.binding.object_prefix,
        vala_bifrost_redux::catalog::layout::FORGE_WRITER_RECIPE
    );
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
/// reached without pre-empting the strategies ahead of it: the worker runs
/// whenever a row is claimable or the leader still owes the table a rewrite. Returns the settled
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
        supervisor.maintain_only().await;
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
        // An owed compaction lives only in the leader's schedule until a
        // worker pulls it, so it is pending work exactly like a queued row.
        let owed = supervisor.forge().held_leader_term().is_some_and(|term| {
            term.schedule().owes_compaction(&ForgeTableKey {
                tenant: fixture.tenant,
                table: identity(fixture),
            })
        });
        if claimable > 0 || owed {
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
    let stop = CancellationToken::new();
    let promotion = owed_promotion(&fixture, &forge).await;
    enqueue(&fixture, &promotion).await;
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
    let stop = CancellationToken::new();
    let promotion = owed_promotion(&fixture, &forge).await;
    enqueue(&fixture, &promotion).await;
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
    let worker = fixture.plan_worker_for_test(observer.clone()).await;
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
    let worker = fixture.plan_worker_for_test(observer.clone()).await;
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
    let promotion = owed_promotion(&fixture, &forge).await;
    enqueue(&fixture, &promotion).await;
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
    let _telemetry = ForgeTelemetryCheckpoint::install();
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
    // Captured before the promotion runs: once it settles SQL these rows are no
    // longer promotable, so arbitration would produce nothing to rebind.
    let planned = owed_promotion(&fixture, &forge).await;
    enqueue(&fixture, &planned).await;
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
    enqueue(&fixture, &in_window).await;
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
    let retired = promotions_on_snapshot(&fixture, in_window.base_snapshot_id).await;
    assert_eq!(
        retired,
        vec!["cancelled".to_owned()],
        "the stale task is retired as cancelled, not failed"
    );

    fixture.seal_more(2).await;
    let replan = owed_promotion(&fixture, &forge).await;
    enqueue(&fixture, &replan).await;
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

/// The leader counts a promotion only once its Iceberg commit has returned.
///
/// The fixture's sealed hot objects are Scribe publication alone. The new
/// leader's acquisition sweep promotes them inline, and the catalog seam parks
/// that fast append: while parked the leader holds its term but tracks
/// nothing. Releasing the append yields exactly one pending commit, and a
/// later pass with no debt left adds none.
///
/// # Panics
///
/// Panics when the leader counts hot publication, misses the committed
/// promotion, or counts it twice.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn promotion_notification_only_after_iceberg_commit() {
    let fixture = PromotionIntegrationFixture::start("notify_after_commit").await;
    let catalog = fixture.catalog.iceberg_catalog();
    let loaded = catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("fixture table");
    let tx = Transaction::new(&loaded);
    let tx = tx
        .update_table_properties()
        .set("wyrd.forge.enable-compaction".to_owned(), "true".to_owned())
        .apply(tx)
        .expect("Forge table settings");
    tx.commit_once(catalog.as_ref())
        .await
        .expect("Forge table settings commit");

    let store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let seam = PromotionCatalogSeam::new(catalog, store.read_counter());
    seam.park_next_commit();
    let supervised = SupervisedPromotion::start(
        &fixture,
        Arc::clone(&seam) as Arc<dyn iceberg::Catalog>,
        store as Arc<dyn vala_bifrost_redux::forge::ForgeObjectStore>,
        ForgeClock::system(),
    );
    let first = supervised.request_pass();
    timeout(OWNERSHIP_BOUND, seam.wait_for_parked_commit())
        .await
        .expect("the acquisition sweep reaches the promotion commit");

    let forge = supervised.forge();
    let key = ForgeTableKey {
        tenant: fixture.tenant,
        table: identity(&fixture),
    };
    let term = forge
        .held_leader_term()
        .expect("the pass acquired the term");
    assert!(
        term.schedule().track_for_test(&key).is_none(),
        "hot publication alone is not an Iceberg commit"
    );

    seam.release_parked_commit();
    supervised.await_pass(first).await;
    let counted = term
        .schedule()
        .track_for_test(&key)
        .expect("the committed promotion is counted");
    assert_eq!(
        counted.pending_commits, 1,
        "one promotion is one notification"
    );
    assert!(
        fixture
            .file_rows()
            .await
            .iter()
            .all(|row| row.committed_snapshot_id.is_some()),
        "the sweep promoted every hot object"
    );

    supervised.schedule_only().await;
    assert_eq!(
        term.schedule()
            .track_for_test(&key)
            .map(|track| track.pending_commits),
        Some(1),
        "a pass with no promotion debt counts nothing"
    );
}

/// Plans one fresh managed attempt of `compaction_type` over the fixture table.
///
/// # Panics
///
/// Panics when the attempt context cannot be built or planning fails.
async fn planned_paths(
    promoted: &PromotedRewriteFixture,
    forge: &Forge,
    compaction_type: ForgeCompactionType,
) -> (i64, Vec<BTreeSet<String>>) {
    let cancel = CancellationToken::new();
    let planned = forge
        .managed_rewrite(
            &promoted.fixture.binding,
            Uuid::now_v7(),
            Uuid::now_v7(),
            &cancel,
        )
        .expect("the attempt context builds")
        .plan(compaction_type)
        .await
        .expect("planning the current head succeeds");
    let groups = planned
        .plans
        .iter()
        .map(|planned| {
            planned
                .plan
                .file_group
                .data_files
                .iter()
                .map(|task| task.data_file_path.clone())
                .collect()
        })
        .collect();
    (planned.evidence.base_snapshot_id, groups)
}

/// A dispatched worker plans from the table's current Iceberg head.
///
/// The leader names a table and a task type, never files, so every file
/// decision is the worker's, made against the head it loads. Full, the
/// default, consumes every live file at that head; `SmallFiles` consumes the
/// small ones, which here is all of them; Auto keeps upstream's five-small-file
/// floor; `FilesWithDelete` finds nothing on a table without deletes; and a
/// copy-on-write table plans one table-wide Full group whatever type it names.
/// Through the production worker, a type that finds nothing still reports
/// success and publishes no snapshot. A real rewrite then draws on the shared
/// root: with every byte held, growth is refused or spilled, and once released
/// the rewrite completes and returns every Forge byte and spill file. The
/// held Scribe charge beside Forge growth and spill placement are proven at
/// the pool itself by `rewrite_pool_charges_root_and_releases_on_cancel`,
/// because Scribe's charge entry point is crate-private.
///
/// # Panics
///
/// Panics when planning ignores the current head or the requested type, when
/// a no-plan dispatch fails or publishes, or when a rewrite leaves governed
/// bytes or spill files behind.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn worker_selects_current_iceberg_files() {
    let _telemetry = ForgeTelemetryCheckpoint::install();
    let promoted = PromotedRewriteFixture::start("worker_selects").await;
    let store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let forge = promoted.forge(
        promoted.fixture.catalog.iceberg_catalog(),
        Arc::clone(&store) as Arc<dyn ForgeObjectStore>,
    );
    let live = promoted.fixture.live_data_paths().await;
    let head = promoted
        .load_table()
        .await
        .metadata()
        .current_snapshot()
        .expect("a promoted head")
        .snapshot_id();
    assert!(
        (2..5).contains(&live.len()),
        "the fixture offers small files below Auto's floor: {live:?}"
    );

    for compaction_type in [ForgeCompactionType::Full, ForgeCompactionType::SmallFiles] {
        let (base, groups) = planned_paths(&promoted, &forge, compaction_type).await;
        assert_eq!(base, head, "{compaction_type:?} plans the current head");
        assert_eq!(
            groups.iter().flatten().cloned().collect::<BTreeSet<_>>(),
            live,
            "{compaction_type:?} consumes every live small file"
        );
    }
    for compaction_type in [
        ForgeCompactionType::Auto,
        ForgeCompactionType::FilesWithDelete,
    ] {
        let (base, groups) = planned_paths(&promoted, &forge, compaction_type).await;
        assert_eq!(base, head, "{compaction_type:?} plans the current head");
        assert!(
            groups.is_empty(),
            "{compaction_type:?} selects nothing here: {groups:?}"
        );
    }
    set_table_properties(
        &promoted.fixture.catalog,
        &promoted.fixture.binding,
        &[("write.delete.mode", "copy-on-write")],
    )
    .await;
    let cow_head = promoted
        .load_table()
        .await
        .metadata()
        .current_snapshot()
        .expect("a head")
        .snapshot_id();
    let (base, groups) =
        planned_paths(&promoted, &forge, ForgeCompactionType::FilesWithDelete).await;
    assert_eq!(base, cow_head, "copy-on-write plans the current head");
    assert_eq!(
        groups,
        vec![live.clone()],
        "copy-on-write plans one table-wide Full group whatever type it names"
    );

    assert_rewrite_draws_on_shared_root(&promoted, &forge).await;
}

/// Runs a real rewrite under a full shared root, then under a free one.
///
/// With every byte held, the rewrite's growth is refused and the attempt
/// fails holding nothing; once released, the rewrite completes and returns
/// every Forge byte and spill file.
///
/// # Panics
///
/// Panics when the full root admits the rewrite, the free root refuses it, or
/// a finished rewrite leaves governed bytes or spill files behind.
async fn assert_rewrite_draws_on_shared_root(
    promoted: &PromotedRewriteFixture,
    forge: &Arc<Forge>,
) {
    let occupant = promoted.fixture.forge_resources.occupy_root_for_test();
    let refused = promoted
        .run_attempt(forge, Uuid::now_v7(), CancellationToken::new())
        .await;
    let occupied = promoted
        .fixture
        .forge_resources
        .snapshot()
        .expect("held snapshot");
    drop(occupant);
    assert!(
        refused.failure.is_some(),
        "a rewrite cannot grow through a full shared root: {:?}",
        refused.handoffs
    );
    assert_eq!(
        occupied.forge_memory_used_bytes, occupied.governed_memory_used_bytes,
        "the occupant is the only holder once the refused rewrite returned"
    );
    drop(refused);
    let run = promoted
        .run_attempt(forge, Uuid::now_v7(), CancellationToken::new())
        .await;
    assert!(
        run.failure.is_none(),
        "the released root admits the rewrite: {:?}",
        run.failure
    );
    drop(run);
    let released = promoted
        .fixture
        .forge_resources
        .snapshot()
        .expect("released snapshot");
    assert_eq!(
        (
            released.forge_memory_used_bytes,
            released.governed_memory_used_bytes
        ),
        (0, 0),
        "a finished rewrite returns every Forge byte"
    );
    assert_eq!(
        std::fs::read_dir(&promoted.fixture.forge_spill)
            .expect("the Forge spill root is readable")
            .count(),
        0,
        "a finished rewrite leaves nothing under the Forge spill root"
    );
}

/// A dispatched task type that finds nothing reports success and publishes nothing.
///
/// # Panics
///
/// Panics when the dispatch fails, publishes a snapshot, or stays owed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn worker_reports_no_plan_dispatch_as_success() {
    let _telemetry = ForgeTelemetryCheckpoint::install();
    let fixture = PromotionIntegrationFixture::start("no_plan_dispatch").await;
    set_table_properties(
        &fixture.catalog,
        &fixture.binding,
        &[("wyrd.forge.compaction.type", "files-with-delete")],
    )
    .await;
    let store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let mut supervisor = SupervisedPromotion::start_serial(
        &fixture,
        fixture.catalog.iceberg_catalog(),
        Arc::clone(&store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
    );
    supervisor.run_one_success().await;
    let key = ForgeTableKey {
        tenant: fixture.tenant,
        table: identity(&fixture),
    };
    let owes = |supervisor: &SupervisedPromotion| {
        supervisor
            .forge()
            .held_leader_term()
            .is_some_and(|term| term.schedule().owes_compaction(&key))
    };
    assert!(
        owes(&supervisor),
        "the promotion commit makes the table owe compaction"
    );
    let snapshots = snapshot_count(&fixture).await;

    supervisor.restart_worker();
    supervisor.settle_one_success().await;

    assert!(
        !owes(&supervisor),
        "a no-plan success settles the owed compaction"
    );
    assert_eq!(
        snapshot_count(&fixture).await,
        snapshots,
        "a dispatch that planned nothing publishes nothing"
    );
    assert_eq!(store.output_writers(), 0, "planning nothing writes nothing");
    supervisor.shutdown().await;
}

/// Counts the fixture table's Iceberg snapshots.
///
/// # Panics
///
/// Panics when the table cannot be loaded.
async fn snapshot_count(fixture: &PromotionIntegrationFixture) -> usize {
    fixture
        .catalog
        .iceberg_catalog()
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("fixture table")
        .metadata()
        .snapshots()
        .count()
}

/// Reads the leader's view of the fixture table's compaction track.
///
/// # Panics
///
/// Panics when the supervisor holds no leader term or the table has no track.
fn track(
    supervisor: &SupervisedPromotion,
    fixture: &PromotionIntegrationFixture,
) -> vala_bifrost_redux::forge::ForgeTrackView {
    let key = ForgeTableKey {
        tenant: fixture.tenant,
        table: identity(fixture),
    };
    supervisor
        .forge()
        .held_leader_term()
        .expect("the supervisor's pass holds the leader term")
        .schedule()
        .track_for_test(&key)
        .expect("the promoted table is tracked")
}

/// Reports settle only the commits a dispatch captured, and stale ones nothing.
///
/// Every step goes through the production promotion, pull, and report routes
/// under a manual clock, so a commit can land while a dispatch is in flight and
/// a report can arrive after its dispatch timed out. Success subtracts only the
/// count the dispatch captured, so a commit that arrived during execution stays
/// pending and the table is due again at once. A report naming any task but
/// the in-flight one changes nothing — neither a task the leader never issued
/// nor one whose report deadline elapsed and was redispatched.
///
/// # Panics
///
/// Panics when a later commit is lost, a stale or late report applies, or a
/// timed-out dispatch is not reconsidered on the next pull.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn report_preserves_later_commits_and_ignores_stale() {
    let _telemetry = ForgeTelemetryCheckpoint::install();
    let fixture = PromotionIntegrationFixture::start("report_accounting").await;
    let store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let (clock, control) = manual_clock();
    let supervisor = SupervisedPromotion::start_serial(
        &fixture,
        fixture.catalog.iceberg_catalog(),
        Arc::clone(&store) as Arc<dyn ForgeObjectStore>,
        clock,
    );
    let forge = supervisor.forge();

    supervisor.schedule_only().await;
    assert_eq!(track(&supervisor, &fixture).pending_commits, 1);
    let first = forge.pull_compaction(4).await.expect("leader pull");
    assert_eq!(first.len(), 1, "the promoted table is due: {first:?}");

    // A commit lands while the dispatch is in flight.
    fixture.seal_more(2).await;
    supervisor.schedule_only().await;
    assert_eq!(track(&supervisor, &fixture).pending_commits, 2);

    let unknown = ForgeCompactionDispatch {
        task_id: Uuid::now_v7(),
        ..first[0].clone()
    };
    forge
        .report_compaction(&unknown, ForgeCompactionOutcome::Succeeded)
        .await
        .expect("leader report");
    let after_unknown = track(&supervisor, &fixture);
    assert_eq!(
        (after_unknown.pending_commits, after_unknown.in_flight),
        (2, Some(first[0].task_id)),
        "a report for a task the leader never issued changes nothing"
    );

    forge
        .report_compaction(&first[0], ForgeCompactionOutcome::Succeeded)
        .await
        .expect("leader report");
    assert_eq!(
        track(&supervisor, &fixture).pending_commits,
        1,
        "success consumes only the captured commit"
    );
    let second = forge.pull_compaction(4).await.expect("leader pull");
    assert_eq!(second.len(), 1, "the surviving commit is due at once");

    // The second dispatch's report deadline elapses before it reports.
    control
        .advance(ChronoDuration::from_std(DEFAULT_REPORT_TIMEOUT).expect("report timeout fits"))
        .expect("manual clock advances");
    let redispatched = forge.pull_compaction(4).await.expect("leader pull");
    assert_eq!(
        redispatched.len(),
        1,
        "a timed-out dispatch is reconsidered on the next pull"
    );
    assert_ne!(redispatched[0].task_id, second[0].task_id);
    forge
        .report_compaction(&second[0], ForgeCompactionOutcome::Succeeded)
        .await
        .expect("leader report");
    let after_late = track(&supervisor, &fixture);
    assert_eq!(
        (after_late.pending_commits, after_late.in_flight),
        (1, Some(redispatched[0].task_id)),
        "the timed-out dispatch's late report is stale"
    );

    // A late commit during the redispatch also survives its success.
    fixture.seal_more(1).await;
    supervisor.schedule_only().await;
    forge
        .report_compaction(&redispatched[0], ForgeCompactionOutcome::Succeeded)
        .await
        .expect("leader report");
    let settled = track(&supervisor, &fixture);
    assert_eq!(
        (settled.pending_commits, settled.in_flight),
        (1, None),
        "the redispatch consumes only what it captured"
    );
    supervisor.shutdown().await;
}

/// Tables the leader tracks in the decision-cost scenario.
const DECISION_TABLES: usize = 10_000;
/// Tracked tables whose commit makes them due in that scenario.
const DECISION_DUE_TABLES: usize = 1_000;
/// Concurrent compactors pulling from the leader in that scenario.
const DECISION_PULLERS: usize = 32;

/// Returns the `permille`-th per-mille element of `samples`, sorting them in place.
fn quantile(samples: &mut [Duration], permille: usize) -> Duration {
    samples.sort_unstable();
    let last = samples.len().saturating_sub(1);
    samples[(last * permille).div_ceil(1000).min(last)]
}

/// Pulls and reports from `pullers` OS threads until nothing is due.
///
/// Each thread pulls four tasks at a time through the leader's peer-facing
/// routes and reports every dispatch it received as succeeded, so the one
/// schedule lock sees the contention of a sustained backlog.
///
/// Returns every pull's latency and every dispatched table.
///
/// # Panics
///
/// Panics when a route refuses the held term or a dispatch's own report does
/// not apply.
fn pull_until_drained(
    forge: &Forge,
    token: i64,
    pullers: usize,
) -> (Vec<Duration>, Vec<ForgeTableKey>) {
    std::thread::scope(|scope| {
        let handles = (0..pullers)
            .map(|_| {
                scope.spawn(|| {
                    let mut pulls = Vec::new();
                    let mut dispatched = Vec::new();
                    loop {
                        let pulled = Instant::now();
                        let tasks = forge
                            .serve_compaction_pull(token, 4)
                            .expect("the held term serves the pull");
                        pulls.push(pulled.elapsed());
                        if tasks.is_empty() {
                            return (pulls, dispatched);
                        }
                        for task in tasks {
                            assert!(
                                forge
                                    .serve_compaction_report(
                                        token,
                                        &task.key,
                                        task.task_id,
                                        ForgeCompactionOutcome::Succeeded,
                                    )
                                    .expect("the held term serves the report"),
                                "the dispatch's own report applies"
                            );
                            dispatched.push(task.key);
                        }
                    }
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("puller thread"))
            .fold((Vec::new(), Vec::new()), |(mut pulls, mut keys), (p, k)| {
                pulls.extend(p);
                keys.extend(k);
                (pulls, keys)
            })
    })
}

/// Names the decision scenario's tracked tables, the first due ones first.
///
/// # Panics
///
/// Panics when a generated name is not a valid table identity.
fn decision_keys(fixture: &PromotionIntegrationFixture) -> Vec<ForgeTableKey> {
    (0..DECISION_TABLES)
        .map(|index| ForgeTableKey {
            tenant: fixture.tenant,
            table: ForgeTaskTableIdentity::new(
                "wyrd-redux",
                fixture.binding.table_ref.namespace.as_str(),
                format!("decision_{index}"),
            )
            .expect("decision table identity"),
        })
        .collect()
}

/// The leader decides commits, pulls, and reports from memory alone.
///
/// The leader's routes — the ones a peer compactor reaches — are driven
/// directly, without a worker, over a fixture whose catalog and object store
/// count every load and read. Ten thousand tables are tracked; one thousand
/// are due. Thirty-two OS threads then pull and report concurrently until
/// nothing is due, which is the contention the one schedule lock sees under a
/// sustained backlog. Every due table is dispatched exactly once, and not one
/// catalog load or object read happens anywhere on the leader's side. The
/// per-stage p50/p99, table count, commit rate, and puller count are printed as
/// evidence; latency gates belong to the release-built capacity benchmark.
///
/// # Panics
///
/// Panics when a leader decision touches the catalog or object store, a due
/// table is dispatched twice or never, or a route refuses the held term.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn leader_decision_has_no_catalog_io() {
    let _telemetry = ForgeTelemetryCheckpoint::install();
    let fixture = PromotionIntegrationFixture::start("leader_decision").await;
    let store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let seam = PromotionCatalogSeam::new(fixture.catalog.iceberg_catalog(), store.read_counter());
    let supervisor = SupervisedPromotion::start_serial(
        &fixture,
        Arc::clone(&seam) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
    );
    supervisor.schedule_only().await;
    let forge = supervisor.forge();
    let token = forge
        .held_leader_term()
        .expect("the pass acquired the term")
        .fencing_token();
    let (loads, attempts, reads) = (seam.loads(), seam.attempts(), store.stats());

    let due = ForgeTableSettings {
        compaction_enabled: true,
        trigger_snapshot_count: 1,
        ..ForgeTableSettings::default()
    };
    let idle = ForgeTableSettings {
        compaction_enabled: true,
        ..ForgeTableSettings::default()
    };
    let keys = decision_keys(&fixture);
    let notify = |index: usize, snapshot_id: i64| {
        forge
            .accept_commit_notice(
                token,
                ForgeCommitNotice {
                    key: keys[index].clone(),
                    snapshot_id,
                    settings: if index < DECISION_DUE_TABLES {
                        due.clone()
                    } else {
                        idle.clone()
                    },
                },
            )
            .expect("the held term accepts the notice");
    };
    for index in DECISION_DUE_TABLES..DECISION_TABLES {
        notify(index, 1);
    }
    let started = Instant::now();
    let mut commit_updates = (DECISION_DUE_TABLES..DECISION_TABLES)
        .map(|index| {
            let warm = Instant::now();
            notify(index, 2);
            warm.elapsed()
        })
        .collect::<Vec<_>>();
    let commit_rate = f64::from(u32::try_from(commit_updates.len()).expect("bounded table count"))
        / started.elapsed().as_secs_f64();
    for index in 0..DECISION_DUE_TABLES {
        notify(index, 1);
    }

    let (mut pulls, dispatched) = pull_until_drained(&forge, token, DECISION_PULLERS);

    // The fixture's own promoted table is due as well.
    let promoted = ForgeTableKey {
        tenant: fixture.tenant,
        table: identity(&fixture),
    };
    let expected = keys[..DECISION_DUE_TABLES]
        .iter()
        .chain([&promoted])
        .collect::<BTreeSet<_>>();
    assert_eq!(
        dispatched.len(),
        expected.len(),
        "no due table is dispatched twice"
    );
    assert_eq!(
        dispatched.iter().collect::<BTreeSet<_>>(),
        expected,
        "every due table is dispatched, and only due tables"
    );
    assert_eq!(
        (seam.loads(), seam.attempts(), store.stats()),
        (loads, attempts, reads),
        "leader decisions perform no catalog or object-store IO"
    );
    eprintln!(
        "evidence forge-leader tables={DECISION_TABLES} due={DECISION_DUE_TABLES} \
         pullers={DECISION_PULLERS} commit_updates_per_s={commit_rate:.0} \
         commit_update_p50={:?} commit_update_p99={:?} pulls={} pull_p50={:?} pull_p99={:?} \
         pull_max={:?}",
        quantile(&mut commit_updates, 500),
        quantile(&mut commit_updates, 990),
        pulls.len(),
        quantile(&mut pulls, 500),
        quantile(&mut pulls, 990),
        quantile(&mut pulls, 1000),
    );
    supervisor.shutdown().await;
}

/// A table that declares no Forge property is compacted on the default interval.
///
/// The fixture's compaction opt-in and count trigger are removed before the
/// first promotion, so the table carries exactly the properties registration
/// writes. Its first commit must open a compaction track, the track must stay
/// idle for the whole default one-hour interval, and the leader must dispatch
/// it at the interval boundary.
///
/// # Panics
///
/// Panics when a fixture dependency fails, the commit opens no track, or the
/// leader dispatches before or misses the interval boundary.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn property_less_table_is_compacted_after_the_default_interval() {
    let fixture = PromotionIntegrationFixture::start("default_compaction").await;
    let iceberg = fixture.catalog.iceberg_catalog();
    let loaded = iceberg
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("fixture table load");
    let tx = Transaction::new(&loaded);
    let tx = tx
        .update_table_properties()
        .remove("wyrd.forge.enable-compaction".to_owned())
        .remove("wyrd.forge.compaction.trigger-snapshot-count".to_owned())
        .apply(tx)
        .expect("property removal applies");
    let cleared = tx
        .commit(iceberg.as_ref())
        .await
        .expect("property removal commits");
    assert!(
        !cleared
            .metadata()
            .properties()
            .keys()
            .any(|key| key.starts_with("wyrd.forge.")),
        "the table declares no Forge property: {:?}",
        cleared.metadata().properties()
    );

    let store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let seam = PromotionCatalogSeam::new(fixture.catalog.iceberg_catalog(), store.read_counter());
    let (clock, control) = manual_clock();
    let mut supervised = SupervisedPromotion::start(
        &fixture,
        Arc::clone(&seam) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&store) as Arc<dyn ForgeObjectStore>,
        clock,
    );
    supervised.join_worker().await;
    supervised.schedule_only().await;

    let forge = supervised.forge();
    let key = ForgeTableKey {
        tenant: fixture.tenant,
        table: ForgeTaskTableIdentity::new(
            "wyrd-redux",
            fixture.binding.table_ref.namespace.as_str(),
            &fixture.binding.table_ref.name,
        )
        .expect("fixture table identity"),
    };
    let track = forge
        .held_leader_term()
        .expect("the supervisor's pass holds the leader term")
        .schedule()
        .track_for_test(&key)
        .expect("the first commit opens a compaction track by default");
    assert_eq!(track.pending_commits, 1, "{track:?}");

    control
        .advance(ChronoDuration::seconds(3599))
        .expect("manual clock advance");
    assert!(
        forge
            .pull_compaction(4)
            .await
            .expect("leader pull")
            .is_empty(),
        "one commit waits out the one-hour default interval"
    );
    control
        .advance(ChronoDuration::seconds(1))
        .expect("manual clock advance");
    let due = forge.pull_compaction(4).await.expect("leader pull");
    assert_eq!(due.len(), 1, "the interval boundary dispatches: {due:?}");
    assert_eq!(due[0].compaction_type, ForgeCompactionType::SmallFiles);
}
