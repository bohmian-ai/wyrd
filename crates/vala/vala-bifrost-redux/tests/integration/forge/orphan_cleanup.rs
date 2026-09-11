//! Tier-2 coverage for the periodic never-published orphan cleanup route.
//!
//! Orphan collection is the one Forge protocol that deletes objects no catalog
//! snapshot, no operation row, and no audit transition names. Its safety
//! therefore comes entirely from the canonical writer grammar deciding what is
//! addressable at all and from the complete protection proof deciding what is
//! deletable. These scenarios drive the real scheduler, worker, catalog, and
//! warehouse so that both halves are exercised by production code rather than
//! asserted about it.

use std::sync::Arc;

use chrono::Duration as ChronoDuration;
use iceberg::Catalog;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_bifrost_redux::forge::{
    ForgeObjectStore, ForgeUnsettledOutput, ForgeWorker, ForgeWorkerConfig,
};

use super::rewrite_support::PromotedRewriteFixture;
use super::snapshot_expiration::object_exists;
use super::support::{
    CountingObjectStore, PromotionCatalogSeam, PromotionIntegrationFixture, SupervisedPromotion,
    manual_clock,
};

/// Returns this fixture table's current Forge recipe root as an object key.
fn forge_root(fixture: &PromotionIntegrationFixture) -> String {
    format!("{}/data/forge/v1", fixture.binding.object_prefix)
}

/// Writes one object directly into staging and returns its key.
///
/// # Panics
///
/// Panics when the staging operator rejects the write.
async fn seed_object(fixture: &PromotionIntegrationFixture, path: String) -> String {
    fixture
        .staging
        .write(&path, b"lookalike".to_vec())
        .await
        .expect("the fixture object seeds");
    path
}

/// Asserts nothing durable names the objects the refused rewrite produced.
///
/// This is what makes the scenario a genuine rowless case: with no operation
/// row, no Forge audit transition, and no Reset generation, the collector has
/// no durable evidence to consult and must reach the object through the
/// writer's own grammar and the age floor alone.
///
/// # Panics
///
/// Panics when any durable owner turns out to name the produced objects.
async fn assert_no_durable_owner_names(
    fixture: &PromotionIntegrationFixture,
    forge: &Arc<vala_bifrost_redux::forge::Forge>,
    audit_before: &[String],
) {
    assert_eq!(
        fixture.rewrite_phases().await,
        Vec::<String>::new(),
        "the refusal arrived before any Prepared operation row"
    );
    assert_eq!(
        fixture.forge_audit_operations().await,
        audit_before,
        "a refused publication appends no Forge audit transition"
    );
    assert_eq!(
        forge
            .reset_generation_paths_for_test(&fixture.binding)
            .await
            .expect("reset generations read"),
        Vec::<String>::new(),
        "a rowless output needs no Reset row to be reclaimable"
    );
}

/// Seeds every object that merely resembles a rewrite output.
///
/// Listing discovers all of these under the same table prefix, so they are the
/// direct evidence that path grammar decides reach and never safety.
///
/// # Panics
///
/// Panics when the staging operator rejects a write.
pub(super) async fn seed_lookalikes(fixture: &PromotionIntegrationFixture) -> Vec<String> {
    let root = forge_root(fixture);
    let prefix = &fixture.binding.object_prefix;
    let mut lookalikes = Vec::new();
    for path in [
        format!("{prefix}/data/forge/{}-00001.parquet", Uuid::now_v7()),
        format!(
            "{prefix}/data/forge/v2/{}-00000-{}.parquet",
            Uuid::now_v7(),
            Uuid::now_v7()
        ),
        format!("{root}/{}-00000.parquet", Uuid::now_v7()),
        format!("{root}/{}-0000x-{}.parquet", Uuid::now_v7(), Uuid::now_v7()),
        format!("{prefix}/data/pod-a-01JABCDEF.parquet"),
        format!("{prefix}/metadata/orphan-lookalike.json"),
    ] {
        lookalikes.push(seed_object(fixture, path).await);
    }
    lookalikes
}

/// Asserts one classification for every path in `paths`.
///
/// # Panics
///
/// Panics when the production classifier fails or returns another verdict.
async fn assert_eligibility(
    forge: &Arc<vala_bifrost_redux::forge::Forge>,
    fixture: &PromotionIntegrationFixture,
    paths: &[String],
    expected: &str,
    why: &str,
) {
    for path in paths {
        assert_eq!(
            forge
                .gc_eligibility_for_test(&fixture.binding, path)
                .await
                .expect("eligibility classification"),
            expected,
            "{why}: {path}"
        );
    }
}

/// A rowless Forge output is reclaimed only through canonical identity and a
/// complete protection proof.
///
/// A managed rewrite that dies after producing objects but before its `Prepared`
/// operation row commits leaves physical output that no durable state names.
/// That is precisely the object this protocol exists for, and precisely the
/// object nothing else can prove is safe to delete — there is no Reset row, no
/// operation row, and no audit transition to consult. So the proof runs the real
/// failure, confirms the durable absence it produces, and then requires the
/// collector to reach that object through the writer's own path grammar plus the
/// age floor and nothing else.
///
/// The lookalikes are the other half of the same claim. Listing discovers
/// everything under the table prefix, so an object that merely resembles a
/// rewrite output — a pre-recipe name, another recipe root, a Scribe pod object,
/// catalog metadata — must survive a real collection pass untouched. Path
/// grammar decides reach; it never decides safety.
///
/// # Panics
///
/// Panics when the fixture cannot start, the held attempt settles, or any
/// assertion about durable state, eligibility, or deletion fails.
#[tokio::test]
async fn rowless_output_uses_canonical_identity_and_full_protection() {
    let promoted = PromotedRewriteFixture::start_unpromoted("orphan_rowless").await;
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    let (clock, control) = manual_clock();
    let mut supervisor = SupervisedPromotion::start(
        &promoted.fixture,
        Arc::clone(&catalog) as Arc<dyn Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        clock,
    );
    supervisor.run_one_success().await;

    // One real managed rewrite produces its outputs and is then refused by its
    // own cancellation token, which is the last authority checked before the
    // Prepared operation row would commit.
    let audit_before = promoted.fixture.forge_audit_operations().await;
    supervisor.restart_worker();
    let worker_stop = supervisor.worker_stop();
    let error = supervisor
        .run_one_failure_holding_handoff(async {
            worker_stop.cancel();
        })
        .await;
    let outputs: Vec<ForgeUnsettledOutput> = supervisor
        .last_possible_rewrite_outputs()
        .expect("the refused rewrite reported its possible outputs");
    let forge = supervisor.forge();
    supervisor.shutdown().await;

    // Produced paths are reported as catalog locations; every durable owner
    // downstream addresses them as object keys relative to the warehouse root.
    let rowless: Vec<String> = outputs
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
        "the refused rewrite closed at least one real output: {error}"
    );
    for path in &rowless {
        assert!(
            object_exists(&promoted.fixture, path).await,
            "a refused publication leaves its produced object physically present"
        );
    }

    assert_no_durable_owner_names(&promoted.fixture, &forge, &audit_before).await;
    let lookalikes = seed_lookalikes(&promoted.fixture).await;

    // Before the age floor elapses, even a genuinely rowless output is retained.
    assert_eligibility(
        &forge,
        &promoted.fixture,
        &rowless,
        "TooYoung",
        "an output younger than the configured floor is never collectable",
    )
    .await;
    control
        .advance(ChronoDuration::hours(25))
        .expect("manual clock advance");
    assert_eligibility(
        &forge,
        &promoted.fixture,
        &rowless,
        "Eligible",
        "an aged rowless output is exactly what this protocol collects",
    )
    .await;
    assert_eligibility(
        &forge,
        &promoted.fixture,
        &lookalikes,
        "InvalidPath",
        "only the canonical recipe grammar is addressable by orphan collection",
    )
    .await;

    assert_collection_deletes_only_rowless(&forge, &promoted.fixture, &rowless, &lookalikes).await;
}

/// Runs one real collection pass and asserts what it did and did not delete.
///
/// # Panics
///
/// Panics when the pass fails, deletes the wrong count, or touches a lookalike
/// or the promoted live set.
async fn assert_collection_deletes_only_rowless(
    forge: &Arc<vala_bifrost_redux::forge::Forge>,
    fixture: &PromotionIntegrationFixture,
    rowless: &[String],
    lookalikes: &[String],
) {
    let deleted = forge
        .run_orphan_gc_for_test(&fixture.binding)
        .await
        .expect("the retained orphan owner runs");
    assert_eq!(
        deleted,
        rowless.len(),
        "the collector deletes exactly the rowless outputs it classified"
    );
    for path in rowless {
        assert!(
            !object_exists(fixture, path).await,
            "the collector did not delete the object it reported"
        );
    }
    for path in lookalikes {
        assert!(
            object_exists(fixture, path).await,
            "a lookalike path survives a real collection pass: {path}"
        );
    }
    assert!(
        !fixture.live_data_paths().await.is_empty(),
        "the promoted live set is untouched by orphan collection"
    );
}

/// Seeds one ready orphan-cleanup task carrying the exact closed plan.
///
/// The periodic planner writes this shape: one normalized Forge recipe root as
/// the plan's only input and one immutable object-age cutoff as its only
/// parameter. Seeding it directly keeps this scenario about the bounded scan
/// rather than about planning.
///
/// # Panics
///
/// Panics when the insert fails.
async fn seed_ready_orphan_task(fixture: &PromotionIntegrationFixture, cutoff_ms: i64) -> Uuid {
    seed_ready_orphan_task_at(fixture, cutoff_ms, forge_root(fixture)).await
}

/// Seeds one ready orphan-cleanup task whose scan prefix is `prefix`.
///
/// Identical to [`seed_ready_orphan_task`] except that the caller chooses the
/// plan's single input, which is what lets a scenario file a well-shaped prefix
/// that belongs to a different table under this table's task row.
///
/// # Panics
///
/// Panics when the insert fails.
async fn seed_ready_orphan_task_at(
    fixture: &PromotionIntegrationFixture,
    cutoff_ms: i64,
    prefix: String,
) -> Uuid {
    let task_id = Uuid::now_v7();
    let plan = serde_json::json!({
        "version": 1,
        "inputs": [prefix],
        "parameters": {"version": 1, "kind": "orphan_cleanup", "age_cutoff_ms": cutoff_ms},
    });
    sqlx::query(
        "INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,state,ready_at) \
         VALUES ($1,$2,'wyrd-redux',$3,$4,'orphan_cleanup',$5,$6,decode(repeat('71',32),'hex'),1,1,'ready',now())",
    )
    .bind(task_id)
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.binding.table_ref.namespace.as_str())
    .bind(&fixture.binding.table_ref.name)
    .bind(0_i64)
    .bind(plan)
    .execute(fixture.operator_pool.pool())
    .await
    .expect("ready orphan-cleanup task seeds");
    task_id
}

/// Reads one orphan task's durable state and raw evidence.
///
/// # Panics
///
/// Panics when the diagnostic read fails.
pub(super) async fn orphan_task_row(
    fixture: &PromotionIntegrationFixture,
    task_id: Uuid,
) -> (String, Option<serde_json::Value>) {
    sqlx::query_as("SELECT state,evidence FROM vala.forge_tasks WHERE task_id=$1")
        .bind(task_id)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("the orphan task is readable")
}

/// Extracts the durable resume key from one task's evidence.
///
/// # Panics
///
/// Panics when the evidence is absent or carries no cursor.
pub(super) fn cursor_of(evidence: Option<&serde_json::Value>) -> String {
    let evidence = evidence.expect("a bounded pass leaves a durable cursor");
    let mut keys: Vec<&str> = evidence
        .as_object()
        .expect("task evidence is an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["start_after", "version"],
        "orphan task evidence records traversal only, never candidate ownership"
    );
    evidence
        .get("start_after")
        .and_then(serde_json::Value::as_str)
        .expect("the cursor names one object key")
        .to_owned()
}

/// Returns every catalog-reachable object under the Forge recipe root.
///
/// These are the leading, undeletable entries a bounded scan must walk past
/// before it can ever reach an orphan behind them.
///
/// # Panics
///
/// Panics when the live set cannot be read.
pub(super) async fn protected_forge_outputs(fixture: &PromotionIntegrationFixture) -> Vec<String> {
    let root = format!("{}/", forge_root(fixture));
    fixture
        .live_data_paths()
        .await
        .into_iter()
        .filter(|path| path.starts_with(&root))
        .collect()
}

/// Runs bounded passes until the one whose delete is left ambiguous.
///
/// Every pass before that must be pure traversal: it selects nothing, deletes
/// nothing, checkpoints the single key it completely classified, and returns
/// the same task to the pool without consuming retry budget.
///
/// # Panics
///
/// Panics when a pass is not claimable, a traversal pass deletes or fails, or
/// the scan never reaches the seeded orphan.
async fn traverse_until_ambiguous_delete(
    fixture: &PromotionIntegrationFixture,
    worker: &ForgeWorker,
    store: &CountingObjectStore,
    task_id: Uuid,
) -> Vec<String> {
    let stop = CancellationToken::new();
    let mut checkpoints = Vec::new();
    for _ in 0..16 {
        fixture.clear_task_backoff().await;
        let claim = worker
            .claim_for_test()
            .await
            .expect("claim transaction runs")
            .expect("the orphan task is claimable");
        assert_eq!(claim.task_id, task_id, "no other task is claimable");
        let outcome = worker
            .execute_orphan_cleanup_claim_for_test(claim, &stop)
            .await;
        let (state, evidence) = orphan_task_row(fixture, task_id).await;
        if let Err(error) = &outcome {
            assert!(
                matches!(
                    error,
                    vala_bifrost_redux::forge::ForgeError::ShutdownRetained
                ),
                "only an unresolved prepared batch may end a bounded pass: {error}"
            );
            // Settlement belongs to the production worker loop, which this
            // phase-bypassing entrypoint deliberately does not run, so the
            // failed attempt is still the row's owner here.
            assert_eq!(
                state, "running",
                "a failed pass leaves settlement to the production loop"
            );
            return checkpoints;
        }
        assert_eq!(
            state, "retryable",
            "a bounded traversal pass returns the same task to the pool"
        );
        assert_eq!(
            store.deletes(),
            0,
            "a pass over protected entries deletes nothing"
        );
        checkpoints.push(cursor_of(evidence.as_ref()));
    }
    panic!("the bounded scan never reached the seeded orphan");
}

/// A bounded scan resumes exactly after its durable cursor and never starves.
///
/// The failure this exists to prevent is silent and permanent. When a table's
/// leading listing pages hold only catalog-reachable objects and every attempt
/// restarts the prefix, the eligible orphan behind them is never reached no
/// matter how often the task runs. The cursor is the whole fix, so the proof is
/// about the cursor: each bounded pass checkpoints the last key it completely
/// classified, and the next attempt hands exactly that key to the listing seam
/// instead of relisting what it already proved.
///
/// The ambiguous delete is the other half. A delete whose outcome is unknown
/// must not move the frontier past the object it was about to reclaim, and it
/// must not write candidate ownership into task evidence — the prepared
/// `OrphanGc` operation owns that recovery, and task evidence records traversal
/// only. The successor therefore resumes at the unchanged cursor, replays the
/// prepared batch, and only then exhausts the prefix and clears its evidence.
///
/// # Panics
///
/// Panics when the fixture cannot start or any cursor, deletion, or evidence
/// assertion fails.
#[tokio::test]
async fn bounded_retry_resumes_after_cursor_without_starvation() {
    let mut promoted = PromotedRewriteFixture::start_unpromoted("orphan_resume").await;
    // One entry per page and one page per pass: every pass then classifies
    // exactly one key, which is what makes a leading protected run observable.
    promoted.fixture.config.orphan_gc_max_list_pages = 1;
    let store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    store.page_listing_by(1);
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        store.read_counter(),
    );
    let (clock, control) = manual_clock();
    let mut supervisor = SupervisedPromotion::start(
        &promoted.fixture,
        Arc::clone(&catalog) as Arc<dyn Catalog>,
        Arc::clone(&store) as Arc<dyn ForgeObjectStore>,
        clock,
    );
    supervisor.run_one_success().await;
    supervisor.restart_worker();
    supervisor.run_one_success().await;
    let protected = protected_forge_outputs(&promoted.fixture).await;
    assert!(
        !protected.is_empty(),
        "a published rewrite leaves catalog-reachable objects under the recipe root"
    );
    let leading = protected
        .last()
        .expect("one protected key sorts last")
        .clone();
    let directory = leading
        .rsplit_once('/')
        .expect("a recipe output has a parent directory")
        .0
        .to_owned();
    let orphan = seed_object(
        &promoted.fixture,
        format!(
            "{directory}/{}-00000-{}.parquet",
            Uuid::now_v7(),
            Uuid::now_v7()
        ),
    )
    .await;
    assert!(
        orphan > leading,
        "the seeded orphan sorts behind every protected entry"
    );

    let forge = supervisor.forge();
    supervisor.shutdown().await;
    let cutoff = control
        .advance(ChronoDuration::hours(25))
        .expect("manual clock advance")
        .timestamp_millis();
    let worker = ForgeWorker::new(forge, ForgeWorkerConfig::default(), Uuid::now_v7())
        .expect("fixture Forge worker");
    let task_id = seed_ready_orphan_task(&promoted.fixture, cutoff).await;
    let listings_before = store.list_cursors().len();
    // The pass that first selects the orphan is the pass whose delete is left
    // ambiguous, so the arming happens before any pass runs.
    store.fail_next_deletes(1);

    let checkpoints = Box::pin(traverse_until_ambiguous_delete(
        &promoted.fixture,
        &worker,
        &store,
        task_id,
    ))
    .await;
    assert_resume_never_relists(&store, listings_before, &checkpoints, &leading);
    assert_ambiguity_holds_the_frontier(&promoted.fixture, task_id, &checkpoints, &orphan).await;
    Box::pin(assert_recovered_pass_exhausts_the_prefix(
        &promoted.fixture,
        &worker,
        task_id,
        &orphan,
    ))
    .await;
    for path in &protected {
        assert!(
            object_exists(&promoted.fixture, path).await,
            "orphan collection never touches a catalog-reachable object: {path}"
        );
    }
}

/// Asserts every pass resumed at its predecessor's checkpoint.
///
/// # Panics
///
/// Panics when a cursor repeats, moves backward, or replays an earlier page.
fn assert_resume_never_relists(
    store: &CountingObjectStore,
    listings_before: usize,
    checkpoints: &[String],
    leading: &str,
) {
    assert!(
        !checkpoints.is_empty(),
        "at least one protected entry preceded the orphan"
    );
    assert_eq!(
        checkpoints.last().map(String::as_str),
        Some(leading),
        "the last traversal pass stopped exactly on the final protected entry"
    );
    assert!(
        checkpoints.windows(2).all(|pair| pair[0] < pair[1]),
        "each checkpoint advances strictly: {checkpoints:?}"
    );
    let cursors = store.list_cursors();
    let observed = &cursors[listings_before..];
    assert_eq!(
        observed.len(),
        checkpoints.len().saturating_add(1),
        "every pass listed exactly once: {observed:?}"
    );
    assert_eq!(
        observed[0], None,
        "the first pass starts at the prefix head"
    );
    for (index, checkpoint) in checkpoints.iter().enumerate() {
        assert_eq!(
            observed[index.saturating_add(1)].as_deref(),
            Some(checkpoint.as_str()),
            "a successor resumes at its predecessor's checkpoint and relists nothing"
        );
    }
}

/// Asserts an ambiguous delete moved neither the cursor nor ownership.
///
/// # Panics
///
/// Panics when the cursor advanced, evidence gained candidate ownership, or the
/// candidate object was reported gone.
async fn assert_ambiguity_holds_the_frontier(
    fixture: &PromotionIntegrationFixture,
    task_id: Uuid,
    checkpoints: &[String],
    orphan: &str,
) {
    let (_, evidence) = orphan_task_row(fixture, task_id).await;
    assert_eq!(
        cursor_of(evidence.as_ref()).as_str(),
        checkpoints.last().map_or("", String::as_str),
        "an ambiguous delete leaves the frontier exactly where its predecessor left it"
    );
    let open: Vec<String> = sqlx::query_scalar(
        "SELECT phase FROM vala.forge_operation_state WHERE data_tenant_id=$1 AND family='orphan_gc'",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("orphan-GC operation state is readable");
    assert_eq!(
        open,
        vec!["prepared".to_owned()],
        "the prepared batch, not task evidence, still owns the unresolved reclaim of {orphan}"
    );
}

/// Asserts the successor replays the prepared batch and then finishes.
///
/// # Panics
///
/// Panics when the pass fails, the orphan survives, or the task does not settle
/// with its evidence cleared.
async fn assert_recovered_pass_exhausts_the_prefix(
    fixture: &PromotionIntegrationFixture,
    worker: &ForgeWorker,
    task_id: Uuid,
    orphan: &str,
) {
    sqlx::query("UPDATE vala.forge_tasks SET state='ready',attempt_id=NULL,claimed_by=NULL,claim_expires_at=NULL,watermark_snapshot_id=NULL,watermark_timestamp_ms=NULL WHERE task_id=$1")
        .bind(task_id)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("the failed attempt releases its claim");
    fixture.clear_task_backoff().await;
    let claim = worker
        .claim_for_test()
        .await
        .expect("claim transaction runs")
        .expect("the released orphan task is claimable again");
    assert_eq!(claim.task_id, task_id);
    worker
        .execute_orphan_cleanup_claim_for_test(claim, &CancellationToken::new())
        .await
        .expect("the recovered pass settles");
    assert!(
        !object_exists(fixture, orphan).await,
        "the prepared batch owns the recovery of its own ambiguous delete"
    );
    let (state, evidence) = orphan_task_row(fixture, task_id).await;
    assert_eq!(state, "succeeded", "an exhausted prefix ends the task");
    assert!(
        evidence.is_none(),
        "a completed orphan task carries no resume position: {evidence:?}"
    );
}

/// A sibling table's Forge root is refused before any lease, catalog, or IO.
///
/// The plan input is a delete authority. A prefix that is well shaped — same
/// tenant, same warehouse, same `data/forge/v1` recipe root — but names a
/// different table would still authorise deleting that table's objects, and the
/// dispatch's catalog-derived comparison only catches it after the worker has
/// already taken the table fence and loaded metadata. The binding derived from
/// the task row already names the one table this task may touch, so the refusal
/// belongs before all of that: no lease row, no catalog load, no listing, no
/// stat, and no delete may exist when it returns.
///
/// # Panics
///
/// Panics when the fixture cannot start, when the cross-table prefix is
/// accepted, or when any effect was reached before the refusal.
#[tokio::test]
async fn cross_table_plan_refuses_before_lease_or_io() {
    let promoted = PromotedRewriteFixture::start_unpromoted("orphan_cross_table").await;
    let store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        store.read_counter(),
    );
    let (clock, control) = manual_clock();
    let mut supervisor = SupervisedPromotion::start(
        &promoted.fixture,
        Arc::clone(&catalog) as Arc<dyn Catalog>,
        Arc::clone(&store) as Arc<dyn ForgeObjectStore>,
        clock,
    );
    supervisor.run_one_success().await;
    let forge = supervisor.forge();
    supervisor.shutdown().await;
    let cutoff = control
        .advance(ChronoDuration::hours(25))
        .expect("manual clock advance")
        .timestamp_millis();

    let fixture = &promoted.fixture;
    let owned = forge_root(fixture);
    // Same tenant, same warehouse, same recipe root: only the table segment
    // differs, which is exactly the prefix a confused or hostile planner emits.
    let sibling = owned.replace(
        &fixture.binding.table_name,
        &format!("{}_sibling", fixture.binding.table_name),
    );
    assert_ne!(sibling, owned, "the decoy names a different table");
    let task_id = seed_ready_orphan_task_at(fixture, cutoff, sibling.clone()).await;

    let worker = ForgeWorker::new(
        Arc::clone(&forge),
        ForgeWorkerConfig::default(),
        Uuid::now_v7(),
    )
    .expect("fixture Forge worker");
    let claim = worker
        .claim_for_test()
        .await
        .expect("claim transaction runs")
        .expect("the ready orphan task is claimable");
    assert_eq!(claim.task_id, task_id, "no other task is claimable");
    let loads_before = catalog.loads();

    let refusal = worker
        .execute_claim(claim, &CancellationToken::new())
        .await
        .expect_err("a sibling table's Forge root is not this task's delete authority");
    assert!(
        matches!(
            &refusal,
            vala_bifrost_redux::forge::ForgeError::Invariant { detail }
                if detail.contains(&sibling) && detail.contains(&owned)
        ),
        "the refusal names the rejected prefix and this task's own root: {refusal}"
    );

    assert_eq!(
        catalog.loads(),
        loads_before,
        "the refusal precedes every catalog load"
    );
    assert!(
        store.list_cursors().is_empty(),
        "the refusal precedes every orphan listing"
    );
    assert_eq!(store.stats(), 0, "the refusal precedes every stat");
    assert_eq!(store.deletes(), 0, "the refusal precedes every delete");
    assert!(
        orphan_operations(fixture).await.is_empty(),
        "no orphan-GC batch was ever prepared"
    );
    assert!(
        orphan_gc_audits(fixture).await.is_empty(),
        "a refusal before any effect appends no orphan-GC audit"
    );
    let leases: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.maintenance_leases")
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("maintenance leases are readable");
    assert_eq!(leases, 0, "the refusal precedes table fence acquisition");
}

/// One durable authority whose loss must be independently observable.
///
/// Orphan collection deletes objects nothing else names, so the protected union
/// is the whole safety argument. A source that never changes a verdict on its
/// own is a source whose absence would go unnoticed, which is exactly how a
/// protection root quietly stops protecting. Each variant is therefore seeded
/// alone, over an otherwise-eligible orphan, and then removed again.
#[derive(Debug, Clone, Copy)]
enum ProtectionSource {
    /// A committed-but-unpromoted Scribe reference naming the object.
    HotFileList,
    /// An open Iceberg rewrite whose prepared outputs name the object.
    OpenRewrite,
    /// An open snapshot expiration over the same table.
    OpenExpire,
    /// An open orphan-GC batch over the same table.
    OpenOrphanGc,
    /// A prepared expired-cleanup candidate naming the object.
    PreparedCleanup,
}

impl ProtectionSource {
    /// Names this authority in assertion messages.
    const fn authority(self) -> &'static str {
        match self {
            Self::HotFileList => "a committed-but-unpromoted file_list reference",
            Self::OpenRewrite => "an open Iceberg rewrite's prepared outputs",
            Self::OpenExpire => "an open snapshot expiration",
            Self::OpenOrphanGc => "an open orphan-GC batch",
            Self::PreparedCleanup => "a prepared expired-cleanup candidate",
        }
    }

    /// Seeds exactly this authority over `orphan` and nothing else.
    ///
    /// # Panics
    ///
    /// Panics when the durable seed cannot be written.
    async fn seed(self, fixture: &PromotionIntegrationFixture, orphan: &str) {
        match self {
            Self::HotFileList => seed_hot_file_reference(fixture, orphan).await,
            Self::OpenRewrite => {
                seed_open_operation(fixture, "iceberg_rewrite", |id| {
                    rewrite_detail(fixture, id, orphan)
                })
                .await;
            }
            Self::OpenExpire => {
                seed_open_operation(fixture, "snapshot_expire", |id| expire_detail(fixture, id))
                    .await;
            }
            Self::OpenOrphanGc => {
                seed_open_operation(fixture, "orphan_gc", |id| orphan_gc_detail(fixture, id)).await;
            }
            Self::PreparedCleanup => seed_prepared_cleanup_candidate(fixture, orphan).await,
        }
    }

    /// Removes exactly this authority, restoring the bare fixture.
    ///
    /// # Panics
    ///
    /// Panics when the durable seed cannot be removed.
    async fn remove(self, fixture: &PromotionIntegrationFixture) {
        let statement = match self {
            Self::HotFileList => "DELETE FROM vala.file_list WHERE data_tenant_id=$1",
            Self::OpenRewrite | Self::OpenExpire | Self::OpenOrphanGc => {
                "DELETE FROM vala.forge_operation_state WHERE data_tenant_id=$1"
            }
            Self::PreparedCleanup => {
                "DELETE FROM vala.forge_tasks WHERE data_tenant_id=$1 AND strategy='expired_cleanup'"
            }
        };
        // The operator role may insert into these tables but does not own the
        // right to delete from every one of them, so the fixture teardown uses
        // the database owner rather than widening a production grant.
        let superuser = fixture
            .database
            .superuser_pool()
            .await
            .expect("superuser pool");
        sqlx::query(statement)
            .bind(fixture.tenant.as_uuid())
            .execute(&superuser)
            .await
            .expect("the seeded authority is removable");
    }
}

/// Builds one prepared rewrite detail whose only output is `path`.
///
/// # Panics
///
/// Panics when the fixed partition boundary or storage path is rejected.
fn rewrite_detail(
    fixture: &PromotionIntegrationFixture,
    operation_id: Uuid,
    path: &str,
) -> serde_json::Value {
    use wyrd_spec::vala::api::{TimeGranularityWire, TimePartitionWire};
    use wyrd_spec::vala::audit_detail::{AuditDetail, ForgeIcebergRewritePhase};

    detail_value(AuditDetail::ForgeIcebergRewrite {
        operation_id,
        phase: ForgeIcebergRewritePhase::Prepared,
        group: table_resource(fixture),
        base_snapshot_id: 1,
        committed_snapshot_id: None,
        partition_spec_id: 0,
        time_partition: TimePartitionWire::new(
            TimeGranularityWire::Day,
            chrono::DateTime::from_timestamp(0, 0).expect("epoch is a valid instant"),
        )
        .expect("the epoch is an exact day boundary"),
        target_file_size_bytes: 1,
        input_paths: Vec::new(),
        output_paths: vec![storage_path(path)],
    })
}

/// Builds one prepared snapshot-expiration detail over this table.
///
/// Expiration carries no output paths, so its protection contribution is the
/// destructive-maintenance gate its openness raises over the whole table.
///
/// # Panics
///
/// Panics when the fixed metadata location is rejected.
fn expire_detail(fixture: &PromotionIntegrationFixture, operation_id: Uuid) -> serde_json::Value {
    use wyrd_spec::vala::audit_detail::{AuditDetail, ForgeSnapshotExpirePhase};

    detail_value(AuditDetail::ForgeSnapshotExpire {
        operation_id,
        phase: ForgeSnapshotExpirePhase::Prepared,
        group: table_resource(fixture),
        base_metadata_location: storage_path(&format!(
            "{}/metadata/v1.metadata.json",
            fixture.binding.object_prefix
        )),
        current_snapshot_id: Some(1),
        retained_ref_heads: vec![1],
        cutoff_ms: 0,
        selected_snapshot_ids: Vec::new(),
    })
}

/// Builds one prepared orphan-GC detail over this table.
///
/// # Panics
///
/// Panics when the detail cannot be serialized.
fn orphan_gc_detail(
    fixture: &PromotionIntegrationFixture,
    operation_id: Uuid,
) -> serde_json::Value {
    use wyrd_spec::vala::audit_detail::{AuditDetail, ForgeOrphanGcPhase};

    detail_value(AuditDetail::ForgeOrphanGc {
        operation_id,
        phase: ForgeOrphanGcPhase::Prepared,
        group: table_resource(fixture),
        candidate_paths: Vec::new(),
        deleted_paths: Vec::new(),
        skipped_paths: Vec::new(),
    })
}

/// Serializes one audit detail into the persisted projection column shape.
///
/// # Panics
///
/// Panics when the detail cannot be serialized.
fn detail_value(detail: wyrd_spec::vala::audit_detail::AuditDetail) -> serde_json::Value {
    serde_json::to_value(detail).expect("the prepared detail serializes")
}

/// Validates one object key as an audit storage path.
///
/// # Panics
///
/// Panics when the key is not a representable storage path.
fn storage_path(path: &str) -> wyrd_spec::vala::audit_detail::StoragePath {
    wyrd_spec::vala::audit_detail::StoragePath::new(path.to_owned())
        .expect("the fixture path is a valid storage path")
}

/// Returns the canonical resource identity Forge files operations under.
fn table_resource(fixture: &PromotionIntegrationFixture) -> String {
    format!(
        "bifrost://{}/{}/{}",
        fixture.tenant, fixture.binding.table_ref.namespace, fixture.binding.table_ref.name
    )
}

/// Seeds one open operation row of `family` carrying `detail`.
///
/// # Panics
///
/// Panics when the insert fails.
async fn seed_open_operation(
    fixture: &PromotionIntegrationFixture,
    family: &str,
    detail: impl FnOnce(Uuid) -> serde_json::Value,
) {
    // The projection requires the row's identity and its detail's identity to
    // agree, so the operation id is minted once and used for both.
    let operation_id = Uuid::now_v7();
    let detail = detail(operation_id);
    sqlx::query(
        "INSERT INTO vala.forge_operation_state \
         (data_tenant_id,resource,family,operation_id,phase,prepared_detail,current_detail,\
          prepared_audit_seq,prepared_at,updated_at) \
         VALUES ($1,$2,$3,$4,'prepared',$5,$5,1,statement_timestamp(),statement_timestamp())",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(table_resource(fixture))
    .bind(family)
    .bind(operation_id)
    .bind(&detail)
    .execute(fixture.operator_pool.pool())
    .await
    .expect("the open operation row seeds");
}

/// Seeds one non-terminal `file_list` row naming `path`.
///
/// This stands for a reference, not for a file: nothing reads its size, row
/// count, or LSN range. Scribe has no API for publishing a reference to an
/// object it did not itself write, so a seal cannot produce this shape.
///
/// # Panics
///
/// Panics when the insert fails.
async fn seed_hot_file_reference(fixture: &PromotionIntegrationFixture, path: &str) {
    let start = super::support::fixture_day()
        .and_hms_opt(0, 0, 0)
        .expect("midnight is a valid time")
        .and_utc();
    sqlx::query(
        "INSERT INTO vala.file_list \
         (id,data_tenant_id,namespace,table_name,file_path,file_size,row_count,min_event_time,\
          max_event_time,partition_granularity,partition_start,node_id,writer_epoch,wal_lsn_min,\
          wal_lsn_max,promotion_record) \
         VALUES ($1,$2,$3,$4,$5,1,1,$6,$7,'day',$8,$9,1,1,1,'{\"fixture\":\"orphan-sources\"}'::jsonb)",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .bind(path)
    .bind(start + ChronoDuration::hours(12))
    .bind(start + ChronoDuration::hours(12) + ChronoDuration::seconds(1))
    .bind(start)
    .bind(Uuid::now_v7())
    .execute(fixture.operator_pool.pool())
    .await
    .expect("the hot reference seeds");
}

/// Seeds one prepared expired-cleanup task whose unresolved candidate is `path`.
///
/// # Panics
///
/// Panics when the insert fails.
async fn seed_prepared_cleanup_candidate(fixture: &PromotionIntegrationFixture, path: &str) {
    let candidate = serde_json::json!({
        "category": "data",
        "catalog": "wyrd-redux",
        "namespace": fixture.binding.table_ref.namespace.as_str(),
        "table": fixture.binding.table_ref.name,
        "path": path,
    });
    let evidence = serde_json::json!({
        "version": 2,
        "committed_snapshot_id": 1,
        "committed_metadata_location": "s3://fixture/metadata/v1.metadata.json",
        "committed_metadata_digest": "0".repeat(64),
        "cleanup_candidates": [candidate],
        "deleted_candidate_count": 0,
        "prepared_candidate_index": 0,
    });
    sqlx::query(
        "INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,state,ready_at,attempt_id,claimed_by,claim_expires_at,watermark_snapshot_id,watermark_timestamp_ms,evidence) \
         VALUES ($1,$2,'wyrd-redux',$3,$4,'expired_cleanup',$5,$6,decode(repeat('72',32),'hex'),1,1,'prepared',now(),$7,$7,now()+interval '10 minutes',1,0,$8)",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.binding.table_ref.namespace.as_str())
    .bind(&fixture.binding.table_ref.name)
    .bind(1_i64)
    .bind(serde_json::json!({
        "version": 1,
        "inputs": [],
        "parameters": {
            "version": 1,
            "kind": "expired_cleanup",
            "source_task_id": Uuid::now_v7(),
            "committed_snapshot_id": 1,
            "committed_metadata_location": "s3://fixture/metadata/v1.metadata.json",
            "committed_metadata_digest": "0".repeat(64),
            "cleanup_candidates": [candidate],
        },
    }))
    .bind(Uuid::now_v7())
    .bind(&evidence)
    .execute(fixture.operator_pool.pool())
    .await
    .expect("the prepared cleanup task seeds");
}

/// Every durable protection authority fails closed on its own.
///
/// The pure-policy matrix proves each composed root is load-bearing, but it
/// composes those roots by hand. This proves the other half: that each real
/// durable source production reads actually reaches the union, one at a time,
/// through the production loader and the production predicate. A source that is
/// silently no longer read would leave its object eligible here even though the
/// policy test still passes.
///
/// Lease and fence loss are the two authorities that are not rows in the union
/// at all — they are the gate the worker holds over the table — so they are
/// proven through the real claim workflow rather than through the loader.
///
/// # Panics
///
/// Panics when the fixture cannot start, when a seeded authority does not
/// protect the orphan, when removing it does not restore eligibility, or when a
/// refused fence still deletes.
#[tokio::test]
async fn orphan_protection_sources_fail_closed_independently() {
    let batch = one_eligible_orphan("orphan_sources").await;
    let fixture = &batch.promoted.fixture;
    let orphan = batch.orphan.clone();

    for source in [
        ProtectionSource::HotFileList,
        ProtectionSource::OpenRewrite,
        ProtectionSource::OpenExpire,
        ProtectionSource::OpenOrphanGc,
        ProtectionSource::PreparedCleanup,
    ] {
        let authority = source.authority();
        assert_eligibility(
            &batch.forge,
            fixture,
            std::slice::from_ref(&orphan),
            "Eligible",
            &format!("nothing protects the orphan before {authority} is seeded"),
        )
        .await;
        source.seed(fixture, &orphan).await;
        assert_eligibility(
            &batch.forge,
            fixture,
            std::slice::from_ref(&orphan),
            "Protected",
            &format!("{authority} alone protects the orphan"),
        )
        .await;
        source.remove(fixture).await;
        assert_eligibility(
            &batch.forge,
            fixture,
            std::slice::from_ref(&orphan),
            "Eligible",
            &format!("removing {authority} removes the only thing protecting the orphan"),
        )
        .await;
    }

    assert_lease_and_fence_refuse_deletion(&batch).await;
}

/// Proves lease loss and fence loss each refuse a real claim without deleting.
///
/// # Panics
///
/// Panics when either refusal is not raised, or when the orphan is deleted.
async fn assert_lease_and_fence_refuse_deletion(batch: &OrphanBatch) {
    let fixture = &batch.promoted.fixture;
    let lease_key = vala_bifrost_redux::forge::forge_lease_key(
        fixture.tenant,
        &fixture.binding.logical_namespace,
        &fixture.binding.table_name,
    );

    // Lease loss: a live peer already owns this table's exclusive fence, so the
    // claim is refused before it can classify or delete anything.
    let peer = vala_bifrost_redux::forge::ForgeLease::acquire(
        &fixture.operator_pool,
        lease_key.clone(),
        Uuid::now_v7(),
        std::time::Duration::from_mins(5),
    )
    .await
    .expect("the peer lease transaction runs")
    .expect("an unheld table lease is acquirable");
    fixture.clear_task_backoff().await;
    let worker = ForgeWorker::new(
        Arc::clone(&batch.forge),
        ForgeWorkerConfig::default(),
        Uuid::now_v7(),
    )
    .expect("fixture Forge worker");
    let claim = worker
        .claim_for_test()
        .await
        .expect("claim transaction runs")
        .expect("the ready orphan task is claimable");
    let refusal = worker
        .execute_orphan_cleanup_claim_for_test(claim, &CancellationToken::new())
        .await
        .expect_err("a table whose fence a peer holds cannot be collected");
    assert!(
        matches!(
            refusal,
            vala_bifrost_redux::forge::ForgeError::FenceLost { .. }
        ),
        "lease loss refuses the claim: {refusal}"
    );
    assert_eq!(batch.store.deletes(), 0, "a refused lease deletes nothing");
    peer.release(&fixture.operator_pool)
        .await
        .expect("the peer lease releases");

    // Fence loss: this worker takes the lease, and a peer bumps its fencing
    // token while the first candidate is being stat'ed. The per-candidate fence
    // recheck sits between that stat and the delete, so the refusal lands with
    // the object still intact.
    //
    // The refused attempt above still owns the row, so the production reclaim
    // pass returns it to the pool rather than the test resetting durable state.
    fixture.expire_claims().await;
    worker
        .reclaim_expired_attempts_for_test(16)
        .await
        .expect("the refused attempt is reclaimed");
    fixture.clear_task_backoff().await;
    let claim = worker
        .claim_for_test()
        .await
        .expect("claim transaction runs")
        .expect("the released task is claimable again");
    batch.store.pause_stat_at(1);
    let stop = CancellationToken::new();
    let (result, ()) = tokio::join!(
        worker.execute_orphan_cleanup_claim_for_test(claim, &stop),
        async {
            batch.store.stat_paused().await;
            sqlx::query(
                "UPDATE vala.maintenance_leases SET fencing_token = fencing_token + 1 \
                 WHERE lease_key = $1",
            )
            .bind(&lease_key)
            .execute(fixture.operator_pool.pool())
            .await
            .expect("a peer bumps the fence");
            batch.store.release_stat();
        }
    );
    // The batch was already prepared when the fence moved, so the attempt ends
    // in the retained shape an unresolved preparation always takes: its work is
    // left for the successor that reclaims the lapsed lease. Nothing else about
    // the pass changed between the paused stat and the release, so the bump is
    // the only thing that could have stopped it.
    let refusal = result.expect_err("a lost fence cannot complete a deletion");
    assert!(
        matches!(
            refusal,
            vala_bifrost_redux::forge::ForgeError::ShutdownRetained
        ),
        "a fence lost mid-batch retains the attempt rather than settling it: {refusal}"
    );
    assert_eq!(batch.store.deletes(), 0, "a lost fence deletes nothing");
    assert!(
        object_exists(fixture, &batch.orphan).await,
        "the orphan survives every refused authority"
    );
}

/// One promoted table carrying exactly one aged, eligible rowless orphan.
struct OrphanBatch {
    /// Live Scribe/Forge fixture over one repository-managed database.
    promoted: PromotedRewriteFixture,
    /// Delete-counting, pausable object store every effect goes through.
    store: Arc<CountingObjectStore>,
    /// Forge graph retained after the supervisor shut down.
    forge: Arc<vala_bifrost_redux::forge::Forge>,
    /// Durable identity of the ready orphan-cleanup task.
    task_id: Uuid,
    /// Object key of the single eligible orphan.
    orphan: String,
}

/// Promotes one table, seeds one aged orphan, and enqueues its cleanup task.
///
/// # Panics
///
/// Panics when the fixture cannot start or the clock cannot advance.
async fn one_eligible_orphan(name: &str) -> OrphanBatch {
    let promoted = PromotedRewriteFixture::start_unpromoted(name).await;
    let store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        store.read_counter(),
    );
    let (clock, control) = manual_clock();
    let mut supervisor = SupervisedPromotion::start(
        &promoted.fixture,
        Arc::clone(&catalog) as Arc<dyn Catalog>,
        Arc::clone(&store) as Arc<dyn ForgeObjectStore>,
        clock,
    );
    supervisor.run_one_success().await;
    let orphan = seed_object(
        &promoted.fixture,
        format!(
            "{}/{}-00000-{}.parquet",
            forge_root(&promoted.fixture),
            Uuid::now_v7(),
            Uuid::now_v7()
        ),
    )
    .await;
    let forge = supervisor.forge();
    supervisor.shutdown().await;
    let cutoff = control
        .advance(ChronoDuration::hours(25))
        .expect("manual clock advance")
        .timestamp_millis();
    let task_id = seed_ready_orphan_task(&promoted.fixture, cutoff).await;
    OrphanBatch {
        promoted,
        store,
        forge,
        task_id,
        orphan,
    }
}

/// Reads the durable identity and phase of every orphan-GC operation row.
///
/// # Panics
///
/// Panics when the diagnostic read fails.
pub(super) async fn orphan_operations(
    fixture: &PromotionIntegrationFixture,
) -> Vec<(Uuid, String)> {
    sqlx::query_as(
        "SELECT operation_id,phase FROM vala.forge_operation_state WHERE data_tenant_id=$1 AND family='orphan_gc' ORDER BY prepared_at,operation_id",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("orphan-GC operation state is readable")
}

/// Lists every orphan-GC audit operation this tenant appended, in sequence.
///
/// # Panics
///
/// Panics when the diagnostic read fails.
pub(super) async fn orphan_gc_audits(fixture: &PromotionIntegrationFixture) -> Vec<String> {
    tenant_audits(fixture, "forge.orphan_gc.%", None).await
}

/// Lists this tenant's audit operations matching one `LIKE` pattern.
///
/// The audit outbox is readable only through a tenant connection, which is the
/// same route the production appenders take.
///
/// # Panics
///
/// Panics when the tenant connection or the diagnostic read fails.
async fn tenant_audits(
    fixture: &PromotionIntegrationFixture,
    pattern: &str,
    resource: Option<&str>,
) -> Vec<String> {
    let mut conn = fixture
        .vala
        .tenant_conn(fixture.tenant)
        .await
        .expect("fixture tenant connection");
    let operations: Vec<String> = sqlx::query_scalar(
        "SELECT operation FROM vala.audit_outbox WHERE operation LIKE $1 AND ($2::text IS NULL OR resource=$2) ORDER BY seq",
    )
    .bind(pattern)
    .bind(resource)
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("tenant audits are readable");
    conn.commit().await.expect("audit read commit");
    operations
}

/// Asserts the delete gate holds no Postgres transaction or advisory lock.
///
/// The preparation must commit and close before the first object call, because
/// an object store call is unbounded and a transaction held across it would
/// pin a connection and block every competing authority for its duration.
///
/// The advisory probe is scoped to this batch's own operation key because
/// `pg_locks` reports the whole cluster and other tests share this Postgres.
///
/// # Panics
///
/// Panics when this operation's advisory lock is held or the prepared row is
/// still locked by an open transaction.
async fn assert_delete_gate_is_sql_free(batch: &OrphanBatch) -> Uuid {
    let fixture = &batch.promoted.fixture;
    let operations = orphan_operations(fixture).await;
    let [(operation_id, phase)] = operations.as_slice() else {
        panic!("exactly one prepared orphan-GC batch exists: {operations:?}");
    };
    assert_eq!(phase, "prepared", "the batch is durable before its delete");
    let resource: String =
        sqlx::query_scalar("SELECT resource FROM vala.forge_operation_state WHERE operation_id=$1")
            .bind(operation_id)
            .fetch_one(fixture.operator_pool.pool())
            .await
            .expect("the prepared batch names its own resource");
    // pg_locks is cluster-wide, so an unfiltered advisory count also observes
    // every other test sharing this Postgres. Reconstruct exactly the key
    // `ForgeOperations::acquire_operation_lock` hashes and match only that
    // lock, split across the two halves pg_locks exposes.
    let advisory: i64 = sqlx::query_scalar(
        "WITH lock_key AS ( \
             SELECT hashtextextended( \
                 jsonb_build_array($1::uuid::text, $2::text, 'orphan_gc', $3::uuid::text)::text, \
                 0 \
             ) AS key \
         ) \
         SELECT count(*) FROM pg_locks, lock_key \
          WHERE locktype = 'advisory' \
            AND objsubid = 1 \
            AND classid = ((lock_key.key >> 32) & 4294967295)::oid \
            AND objid = (lock_key.key & 4294967295)::oid",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&resource)
    .bind(operation_id)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("advisory locks are readable");
    assert_eq!(advisory, 0, "no advisory lock spans the object delete");
    let mut probe = fixture
        .operator_pool
        .pool()
        .begin()
        .await
        .expect("independent transaction");
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        sqlx::query(
            "SELECT 1 FROM vala.forge_operation_state WHERE operation_id=$1 FOR UPDATE NOWAIT",
        )
        .bind(operation_id)
        .fetch_one(&mut *probe),
    )
    .await
    .expect("the independent lock is not blocked")
    .expect("the preparation transaction released the prepared row");
    probe.rollback().await.expect("release the probe lock");
    *operation_id
}

/// Asserts a post-preparation exit retained the attempt exactly as it stood.
///
/// # Panics
///
/// Panics when the task left `Running`, its attempt or cursor moved, its
/// failure budget was consumed, or a terminal task audit was appended.
async fn assert_attempt_is_retained(
    fixture: &PromotionIntegrationFixture,
    task_id: Uuid,
    attempt: Uuid,
) {
    let (state, evidence, attempts, current): (
        String,
        Option<serde_json::Value>,
        i32,
        Option<Uuid>,
    ) = sqlx::query_as(
        "SELECT state,evidence,attempt_count,attempt_id FROM vala.forge_tasks WHERE task_id=$1",
    )
    .bind(task_id)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("the orphan task is readable");
    assert_eq!(state, "running", "the prepared attempt is left standing");
    assert_eq!(
        current,
        Some(attempt),
        "the attempt generation is untouched"
    );
    assert_eq!(attempts, 0, "a retained exit consumes no failure budget");
    assert!(
        evidence.is_none(),
        "an unresolved batch writes no cursor: {evidence:?}"
    );
    assert_eq!(
        tenant_audits(
            fixture,
            "forge.task.%",
            Some(&format!("forge-task:{task_id}"))
        )
        .await,
        Vec::<String>::new(),
        "a retained attempt appends no terminal task audit"
    );
}

/// A prepared batch owns its own recovery across takeover and replay.
///
/// The delete gate is the dangerous point of this protocol: the batch is
/// durable, the object call is unbounded, and the attempt may die at any
/// instant. This proves the three properties that make that survivable. No
/// Postgres transaction or advisory lock spans the delete, so a stuck object
/// store cannot pin a connection or block a competing authority. A
/// post-preparation exit leaves the attempt exactly as it stood — same
/// generation, same empty cursor, no consumed budget, no terminal audit — so
/// nothing observes a settlement that did not happen. And the successor that
/// reclaims the lapsed lease replays the same batch identity rather than
/// listing a fresh one, so an object already deleted by the dead attempt is
/// recovered as deleted instead of being re-selected or lost.
///
/// # Panics
///
/// Panics when the fixture cannot start or any state, audit, or identity
/// assertion fails.
#[tokio::test]
async fn prepared_batch_takeover_is_replay_safe_and_sql_free_during_delete() {
    let batch = one_eligible_orphan("orphan_takeover").await;
    let fixture = &batch.promoted.fixture;
    let owner = ForgeWorker::new(
        Arc::clone(&batch.forge),
        ForgeWorkerConfig::default(),
        Uuid::now_v7(),
    )
    .expect("first fixture Forge worker");
    let claim = owner
        .claim_for_test()
        .await
        .expect("claim transaction runs")
        .expect("the ready orphan task is claimable");
    assert_eq!(claim.task_id, batch.task_id);
    let attempt = claim.attempt_id.expect("a claim carries its attempt");

    batch.store.pause_delete_at(1);
    let stop = CancellationToken::new();
    let mut prepared = Uuid::nil();
    let (result, ()) = tokio::join!(
        owner.execute_orphan_cleanup_claim_for_test(claim, &stop),
        async {
            batch.store.delete_paused().await;
            prepared = assert_delete_gate_is_sql_free(&batch).await;
            // The owner dies after its delete is submitted but before it can
            // resolve the batch: the worst survivable moment in the protocol.
            stop.cancel();
            batch.store.release_delete();
        }
    );
    let retained = result.expect_err("a post-preparation exit does not settle the attempt");
    assert!(
        matches!(
            retained,
            vala_bifrost_redux::forge::ForgeError::ShutdownRetained
        ),
        "a prepared batch retains its attempt for reclaim: {retained}"
    );
    assert_attempt_is_retained(fixture, batch.task_id, attempt).await;
    assert_eq!(
        orphan_gc_audits(fixture).await,
        vec!["forge.orphan_gc.prepared".to_owned()],
        "an unresolved batch has exactly one prepared audit and no terminal one"
    );

    Box::pin(assert_takeover_replays_the_same_batch(
        &batch, prepared, attempt,
    ))
    .await;
}

/// Asserts the successor reclaims the task and replays the same batch.
///
/// # Panics
///
/// Panics when reclaim does not move the task, the replay creates a second
/// batch identity, the recovery is not audited exactly once, or the task does
/// not settle with its prefix exhausted.
async fn assert_takeover_replays_the_same_batch(
    batch: &OrphanBatch,
    prepared: Uuid,
    attempt: Uuid,
) {
    let fixture = &batch.promoted.fixture;
    fixture.expire_claims().await;
    let reclaimed = vala_sql::queries::forge_tasks::ForgeTasks::new(fixture.operator_pool.clone())
        .reclaim_expired(8)
        .await
        .expect("expired claims reclaim");
    assert_eq!(reclaimed, 1, "the lapsed lease returns exactly this task");
    fixture.clear_task_backoff().await;

    let taker = ForgeWorker::new(
        Arc::clone(&batch.forge),
        ForgeWorkerConfig::default(),
        Uuid::now_v7(),
    )
    .expect("takeover fixture Forge worker");
    let claim = taker
        .claim_for_test()
        .await
        .expect("claim transaction runs")
        .expect("the reclaimed orphan task is claimable");
    assert_eq!(
        claim.task_id, batch.task_id,
        "takeover resumes the same task"
    );
    assert_ne!(
        claim.attempt_id,
        Some(attempt),
        "takeover runs under its own attempt generation"
    );
    taker
        .execute_orphan_cleanup_claim_for_test(claim, &CancellationToken::new())
        .await
        .expect("the takeover settles the prepared batch");

    let operations = orphan_operations(fixture).await;
    let [(identity, phase)] = operations.as_slice() else {
        panic!("recovery replays one batch instead of listing another: {operations:?}");
    };
    assert_eq!(
        *identity, prepared,
        "the batch identity is stable across takeover"
    );
    assert_ne!(
        phase, "prepared",
        "the replayed batch reaches a terminal phase"
    );
    assert_eq!(
        orphan_gc_audits(fixture).await,
        vec![
            "forge.orphan_gc.prepared".to_owned(),
            "forge.orphan_gc.recovered".to_owned()
        ],
        "one prepared and one terminal audit describe the whole batch"
    );
    assert!(
        !object_exists(fixture, &batch.orphan).await,
        "the reclaimed orphan is gone once its batch resolves"
    );
    let (state, evidence) = orphan_task_row(fixture, batch.task_id).await;
    assert_eq!(state, "succeeded", "the exhausted prefix ends the task");
    assert!(
        evidence.is_none(),
        "a completed orphan task carries no resume position: {evidence:?}"
    );
}
