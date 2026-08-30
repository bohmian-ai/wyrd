//! Production-harness proofs for the Scribe promotion route.
//!
//! Every scenario drives the real scheduler and the real worker over hot
//! objects a real Scribe sealed. Nothing here fabricates a `vala.file_list`
//! row, plans a task by hand, or calls a promotion owner directly: the route
//! is only proven if the production supervisors reach it on their own.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use vala_bifrost_redux::catalog::TenantTableBinding;
use wyrd_testing::bifrost::{
    CommitUncertaintyCatalog, ForgeFixture, ForgeObjectStoreControl, seed_forge_group,
};

use crate::support::{SupervisedForge, start_engine_fixture_server};

/// One durable Forge task row as the promotion proofs read it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TaskRow {
    /// Durable strategy discriminator.
    strategy: String,
    /// Durable lifecycle state.
    state: String,
    /// Persisted plan document, including promotion parameters.
    plan: serde_json::Value,
    /// Snapshot the plan was derived against.
    base_snapshot_id: i64,
}

/// One `vala.file_list` row projected to its promotion settlement columns.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileRow {
    /// Durable row identity, which is also the promoted-file identity.
    id: uuid::Uuid,
    /// Canonical object path Scribe published.
    file_path: String,
    /// Whether the row has left the hot source.
    compacted: bool,
    /// Snapshot that represents the row, once settled.
    committed_snapshot_id: Option<i64>,
    /// Forge operation that placed it there, once settled.
    forge_publication_operation_id: Option<uuid::Uuid>,
}

/// Reads every durable Forge task for one tenant in creation order.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn task_rows(fixture: &ForgeFixture) -> Vec<TaskRow> {
    sqlx::query_as::<_, (String, String, serde_json::Value, i64)>(
        "SELECT strategy, state, plan, base_snapshot_id FROM vala.forge_tasks \
         WHERE data_tenant_id = $1 ORDER BY created_at, task_id",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("Forge task inspection")
    .into_iter()
    .map(|(strategy, state, plan, base_snapshot_id)| TaskRow {
        strategy,
        state,
        plan,
        base_snapshot_id,
    })
    .collect()
}

/// Reads the promotion settlement columns of one fixture table, in durable order.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn file_rows(fixture: &ForgeFixture) -> Vec<FileRow> {
    sqlx::query_as::<_, (uuid::Uuid, String, bool, Option<i64>, Option<uuid::Uuid>)>(
        "SELECT id, file_path, compacted, committed_snapshot_id, \
         forge_publication_operation_id FROM vala.file_list \
         WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 \
         ORDER BY created_at, file_ordinal, id",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("Forge file-list inspection")
    .into_iter()
    .map(
        |(id, file_path, compacted, committed_snapshot_id, forge_publication_operation_id)| {
            FileRow {
                id,
                file_path,
                compacted,
                committed_snapshot_id,
                forge_publication_operation_id,
            }
        },
    )
    .collect()
}

/// Collects the live data-file paths of one table's current snapshot.
///
/// The projection mirrors the catalog's own pinning rule — table-relative
/// suffix rewritten onto the tenant object prefix — so a path here is directly
/// comparable to a `vala.file_list` path.
///
/// # Panics
///
/// Panics when the table, its manifest list, or a manifest cannot be read.
async fn live_data_paths(fixture: &ForgeFixture, binding: &TenantTableBinding) -> BTreeSet<String> {
    let table = iceberg::Catalog::load_table(fixture.catalog.as_ref(), &binding.table_ident())
        .await
        .expect("promotion table load");
    let mut paths = BTreeSet::new();
    let Some(snapshot) = table.metadata().current_snapshot() else {
        return paths;
    };
    let manifests = table
        .manifest_list_reader(snapshot)
        .load()
        .await
        .expect("promotion manifest list");
    for manifest_file in manifests.entries() {
        let manifest = manifest_file
            .load_manifest(table.file_io())
            .await
            .expect("promotion manifest");
        for entry in manifest.entries().iter().filter(|entry| entry.is_alive()) {
            let path = entry.data_file().file_path().to_owned();
            let canonical = path
                .strip_prefix(table.metadata().location())
                .and_then(|suffix| suffix.strip_prefix('/'))
                .map(|suffix| format!("{}/{suffix}", binding.object_prefix))
                .unwrap_or(path);
            paths.insert(canonical);
        }
    }
    paths
}

/// Borrows the ordered promoted file identities from one persisted plan.
///
/// # Panics
///
/// Panics when the plan does not carry the locked promotion parameter shape.
fn plan_file_ids(plan: &serde_json::Value) -> Vec<uuid::Uuid> {
    let parameters = plan
        .get("parameters")
        .expect("promotion plan carries parameters");
    assert_eq!(
        parameters.get("kind").and_then(serde_json::Value::as_str),
        Some("scribe_promotion"),
        "promotion plan must carry the locked parameter kind"
    );
    parameters
        .get("file_ids")
        .and_then(serde_json::Value::as_array)
        .expect("promotion parameters carry ordered file ids")
        .iter()
        .map(|value| {
            uuid::Uuid::parse_str(value.as_str().expect("file id is a string"))
                .expect("file id is a uuid")
        })
        .collect()
}

/// The scheduler plans exactly the never-published hot rows, once.
///
/// Three properties are proven together because they are the same rule: the
/// plan names every eligible row in durable order, it names nothing that has
/// already been settled, and a table with an active promotion does not receive
/// a second one.
#[tokio::test]
#[ignore = "requires Postgres"]
async fn scribe_promotion_scheduler_claims_only_exact_eligible_hot_rows() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "promotion_eligibility").await;

    let expected = file_rows(&fixture).await;
    assert_eq!(expected.len(), 2, "two sealed hot objects");
    assert!(
        expected
            .iter()
            .all(|row| !row.compacted && row.committed_snapshot_id.is_none()),
        "sealed hot objects start unpublished"
    );

    let mut forge = SupervisedForge::start_default(&fixture, fixture.config.clone());
    forge.run_one_success().await;

    let tasks = task_rows(&fixture).await;
    let promotions = tasks
        .iter()
        .filter(|task| task.strategy == "scribe_promotion")
        .collect::<Vec<_>>();
    assert_eq!(
        promotions.len(),
        1,
        "exactly one promotion per table, got {tasks:?}"
    );
    assert_eq!(
        plan_file_ids(&promotions[0].plan),
        expected.iter().map(|row| row.id).collect::<Vec<_>>(),
        "the plan names every eligible row in durable order"
    );

    let settled = file_rows(&fixture).await;
    assert!(
        settled.iter().all(|row| row.compacted
            && row.committed_snapshot_id.is_some()
            && row.forge_publication_operation_id.is_some()),
        "a successful promotion settles every planned row: {settled:?}"
    );

    forge.shutdown().await;

    // A second supervised pass over already-settled rows plans nothing: the
    // settled columns are exactly what makes a row ineligible.
    let forge = SupervisedForge::start_default(&fixture, fixture.config.clone());
    forge.schedule_once().await;
    forge.shutdown().await;
    let after = task_rows(&fixture).await;
    assert_eq!(
        after
            .iter()
            .filter(|task| task.strategy == "scribe_promotion")
            .count(),
        1,
        "settled rows create no second promotion: {after:?}"
    );
}

/// One sealed hot object is promoted unchanged and stays exactly visible.
///
/// This is the tier-1 shape of the route: the object Scribe published is the
/// object the snapshot references, at the same path, and no row is ever
/// visible twice or not at all across the transition.
#[tokio::test]
#[ignore = "requires Postgres"]
async fn forge_hot_object_promotes_unchanged_and_remains_exact() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "promotion_journey").await;

    let published = server
        .published_hot_files_for_test(
            fixture.tenant,
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        )
        .await
        .expect("published hot files");
    assert!(!published.is_empty(), "the fixture sealed hot objects");
    let sealed_paths = published
        .iter()
        .map(|file| file.object_key.clone())
        .collect::<BTreeSet<_>>();
    assert!(
        live_data_paths(&fixture, &fixture.binding).await.is_empty(),
        "a hot object is not yet referenced by the table"
    );

    let mut forge = SupervisedForge::start_default(&fixture, fixture.config.clone());
    forge.run_one_success().await;
    forge.shutdown().await;

    let live = live_data_paths(&fixture, &fixture.binding).await;
    assert_eq!(
        live, sealed_paths,
        "promotion references the writer's own objects at their own paths"
    );

    let settled = file_rows(&fixture).await;
    assert_eq!(
        settled.len(),
        published.len(),
        "promotion adds no durable row"
    );
    assert!(
        settled
            .iter()
            .all(|row| row.compacted && row.committed_snapshot_id.is_some()),
        "every promoted row left the hot source: {settled:?}"
    );

    let republished = server
        .published_hot_files_for_test(
            fixture.tenant,
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        )
        .await
        .expect("published hot files after promotion");
    assert_eq!(
        republished
            .iter()
            .map(|file| (
                file.object_key.clone(),
                file.file_size,
                file.file_checksum.clone()
            ))
            .collect::<Vec<_>>(),
        published
            .iter()
            .map(|file| (
                file.object_key.clone(),
                file.file_size,
                file.file_checksum.clone()
            ))
            .collect::<Vec<_>>(),
        "promotion rewrites no object"
    );
}

/// Snapshots every object under one table's prefix with its exact content hash.
///
/// Comparing this map across a promotion is the direct proof that no data
/// object was created, rewritten, or deleted: a PUT anywhere under the prefix
/// changes either the key set or one object's digest.
///
/// # Panics
///
/// Panics when the staging operator cannot be listed or read.
async fn object_digests(fixture: &ForgeFixture) -> BTreeMap<String, String> {
    let prefix = format!("{}/", fixture.binding.object_prefix);
    let entries = fixture
        .staging
        .list_with(&prefix)
        .recursive(true)
        .await
        .expect("promotion object listing");
    let mut digests = BTreeMap::new();
    for entry in entries {
        if entry.metadata().is_dir() {
            continue;
        }
        let bytes = fixture
            .staging
            .read(entry.path())
            .await
            .expect("promotion object read");
        digests.insert(entry.path().to_owned(), {
            use sha2::Digest as _;
            hex::encode(sha2::Sha256::digest(bytes.to_bytes()))
        });
    }
    digests
}

/// Promotion reads every hot footer and writes no data object at all.
///
/// The proof is bidirectional. The object store is byte-identical across the
/// whole route, so nothing was written; and the production object-store seam
/// records reads but no output notification and no delete, so the reads that
/// did happen are revalidation rather than a rewrite in disguise.
#[tokio::test]
#[ignore = "requires Postgres"]
async fn scribe_promotion_revalidates_footer_and_appends_without_data_put() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "promotion_no_put").await;
    let before = object_digests(&fixture).await;
    assert!(
        before.keys().any(|path| !path.contains("/metadata/")),
        "the fixture sealed real data objects"
    );

    let object_store = ForgeObjectStoreControl::new(Arc::clone(&fixture.staging));
    let mut forge = SupervisedForge::start_with_seams(
        &fixture,
        fixture.config.clone(),
        Arc::clone(&fixture.catalog),
        Arc::clone(&object_store) as Arc<dyn vala_bifrost_redux::forge::ForgeObjectStore>,
    );
    forge.run_one_success().await;
    forge.shutdown().await;

    let sealed = file_rows(&fixture).await;
    assert!(
        sealed.iter().all(|row| row.committed_snapshot_id.is_some()),
        "the promotion committed: {sealed:?}"
    );
    let after = object_digests(&fixture).await;
    let data_object = |path: &str| !path.contains("/metadata/");
    assert_eq!(
        after
            .iter()
            .filter(|(path, _)| data_object(path))
            .collect::<BTreeMap<_, _>>(),
        before
            .iter()
            .filter(|(path, _)| data_object(path))
            .collect::<BTreeMap<_, _>>(),
        "promotion left every data object byte-identical"
    );
    assert!(
        after
            .keys()
            .filter(|path| !before.contains_key(path.as_str()))
            .all(|path| !data_object(path)),
        "promotion added only catalog metadata, never a data object: {:?}",
        after
            .keys()
            .filter(|path| !before.contains_key(path.as_str()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        object_store.output_put_calls(),
        0,
        "promotion produced no rewrite output"
    );
    assert_eq!(
        object_store.delete_calls(),
        0,
        "promotion deleted no object"
    );
    assert!(
        object_store.object_io_calls() >= before.len(),
        "promotion revalidated every hot object it appended"
    );
}

/// Every sealed row stays visible exactly once across the catalog-to-SQL window.
///
/// The catalog commit is held open after acceptance, which is the only interval
/// in which a row could be counted twice — referenced by the new snapshot and
/// still unsettled in SQL. The production `pin_sealed_table` cut is taken at
/// that instant, and before and after it, and each cut must partition the
/// sealed set exactly.
#[tokio::test]
#[ignore = "requires Postgres"]
async fn scribe_promotion_catalog_sql_window_preserves_exact_visibility() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "promotion_window").await;
    let catalog_owner = server.bifrost_catalog();
    let table_ref = fixture.binding.table_ref.clone();
    let tenant = fixture.tenant;

    let sealed = file_rows(&fixture)
        .await
        .into_iter()
        .map(|row| row.file_path)
        .collect::<BTreeSet<_>>();
    assert!(!sealed.is_empty(), "the fixture sealed real objects");

    /// Asserts one production cut sees every sealed path exactly once.
    async fn assert_exact_cut(
        catalog: &vala_bifrost_redux::catalog::BifrostCatalog,
        table: &vala_bifrost_redux::catalog::TableRef,
        tenant: wyrd_spec::DataTenantId,
        sealed: &BTreeSet<String>,
        label: &str,
    ) {
        let pinned = catalog
            .pin_sealed_table(table, tenant)
            .await
            .unwrap_or_else(|error| panic!("{label} cut: {error}"));
        let hot = pinned
            .hot_files
            .iter()
            .map(|row| row.file_path.clone())
            .collect::<BTreeSet<_>>();
        let promoted = pinned.iceberg_file_paths;
        assert!(
            hot.is_disjoint(&promoted),
            "{label}: a row is visible from both sources: hot={hot:?} promoted={promoted:?}"
        );
        assert_eq!(
            hot.union(&promoted).cloned().collect::<BTreeSet<_>>(),
            *sealed,
            "{label}: the cut does not cover the sealed set exactly"
        );
    }

    assert_exact_cut(&catalog_owner, &table_ref, tenant, &sealed, "before").await;

    let catalog = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    catalog.pause_after_commit();
    let mut forge = SupervisedForge::start_with_seams(
        &fixture,
        fixture.config.clone(),
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&fixture.object_store),
    );
    let held = tokio::spawn(async move {
        forge.run_one_success().await;
        forge
    });

    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        catalog.wait_for_commit(),
    )
    .await
    .expect("promotion commit reached the catalog boundary");
    assert_exact_cut(&catalog_owner, &table_ref, tenant, &sealed, "mid-window").await;
    catalog.release_paused_commit();

    let forge = tokio::time::timeout(std::time::Duration::from_secs(60), held)
        .await
        .expect("held promotion completes")
        .expect("held promotion task");
    forge.shutdown().await;

    assert_exact_cut(&catalog_owner, &table_ref, tenant, &sealed, "after").await;
    assert_eq!(
        file_rows(&fixture)
            .await
            .into_iter()
            .filter(|row| row.committed_snapshot_id.is_some())
            .count(),
        sealed.len(),
        "every sealed row settled exactly once"
    );
}
