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
use uuid::Uuid;
use vala_bifrost_redux::forge::{ForgeObjectStore, ForgeUnsettledOutput};

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
