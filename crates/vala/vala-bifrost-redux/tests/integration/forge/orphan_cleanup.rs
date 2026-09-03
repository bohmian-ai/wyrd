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
    audit_before: i64,
) {
    assert_eq!(
        fixture.rewrite_phases().await,
        Vec::<String>::new(),
        "the refusal arrived before any Prepared operation row"
    );
    assert_eq!(
        fixture.forge_audit_count().await,
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
async fn seed_lookalikes(fixture: &PromotionIntegrationFixture) -> Vec<String> {
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
    let audit_before = promoted.fixture.forge_audit_count().await;
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

    assert_no_durable_owner_names(&promoted.fixture, &forge, audit_before).await;
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
    let task_id = Uuid::now_v7();
    let plan = serde_json::json!({
        "version": 1,
        "inputs": [forge_root(fixture)],
        "parameters": {"version": 1, "kind": "orphan_cleanup", "age_cutoff_ms": cutoff_ms},
    });
    sqlx::query(
        "INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,lane,base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,estimated_parallelism,estimated_memory_bytes,estimated_spill_bytes,large_task_ceiling_bytes,state,ready_at,envelope_version,decoded_batch_bytes,decoded_input_bytes,sort_working_bytes,sort_merge_reservation_bytes,encoder_buffer_bytes,upload_chunk_bytes,footer_encoded_bytes,footer_decode_workspace_bytes,sort_spill_bytes) \
         VALUES ($1,$2,'wyrd-redux',$3,$4,'orphan_cleanup','ordinary',$5,$6,decode(repeat('71',32),'hex'),1,1,1,41943040,1024,1,'ready',now(),2,1024,1024,3072,1024,1024,1024,8388608,33554432,1024)",
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
async fn orphan_task_row(
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
fn cursor_of(evidence: Option<&serde_json::Value>) -> String {
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
async fn protected_forge_outputs(fixture: &PromotionIntegrationFixture) -> Vec<String> {
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
                    vala_bifrost_redux::forge::ForgeError::ObjectDelete(_)
                ),
                "only the armed delete may end a bounded pass: {error}"
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
