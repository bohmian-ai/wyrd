//! Production-harness proof for the Forge live-rewrite publication route.
//!
//! The scenario is the whole public promise of a rewrite: rows a real Scribe
//! accepted are promoted, then physically rewritten into different objects,
//! and a public read of the table returns exactly the same rows at every step
//! — including across a commit whose acceptance the committing worker never
//! learned. Nothing here plans a task, commits a snapshot, or settles an
//! operation by hand; the production scheduler and worker reach every one of
//! those on their own.

use std::collections::BTreeSet;
use std::sync::Arc;

use vala_bifrost_redux::catalog::{BifrostCatalog, TableRef};
use wyrd_spec::DataTenantId;
use wyrd_testing::bifrost::{
    CommitUncertaintyCatalog, ForgeFixture, seed_forge_group, shared_process_telemetry_for_test,
};

use crate::support::{
    SupervisedForge, live_data_paths, reclaim_stopped_claim, start_engine_fixture_server,
};

/// Reads the durable Forge operation phases for one tenant's live rewrites.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn rewrite_phases(fixture: &ForgeFixture) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT phase FROM vala.forge_operation_state \
         WHERE data_tenant_id = $1 AND family = 'iceberg_rewrite' \
         ORDER BY prepared_at, operation_id",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("Forge operation-state inspection")
}

/// Reads the durable strategies of every Forge task this tenant planned.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn task_strategies(fixture: &ForgeFixture) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT strategy FROM vala.forge_tasks WHERE data_tenant_id = $1 \
         ORDER BY created_at, task_id",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("Forge task inspection")
}

/// Reads every sealed object path this tenant's table owns, from `vala.file_list`.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn sealed_paths(fixture: &ForgeFixture) -> BTreeSet<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT file_path FROM vala.file_list \
         WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("Forge file-list inspection")
    .into_iter()
    .collect()
}

/// Reads the sorted `value` column of every named object.
///
/// The rewrite replaces objects wholesale, so path identity proves nothing
/// about what a reader sees. The user column read back out of whatever objects
/// the cut currently names is the exactness claim itself.
///
/// # Panics
///
/// Panics when an object cannot be read or does not carry the column.
async fn object_values(fixture: &ForgeFixture, paths: &BTreeSet<String>) -> Vec<i64> {
    let mut values = Vec::new();
    for path in paths {
        let bytes = fixture
            .staging
            .read(path)
            .await
            .expect("published object read")
            .to_bytes();
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(bytes)
            .expect("published object is Parquet")
            .build()
            .expect("published object reader");
        for batch in reader {
            let batch = batch.expect("published object batch");
            let column = batch
                .column_by_name("value")
                .expect("published object carries the user column");
            values.extend(
                column
                    .as_any()
                    .downcast_ref::<arrow::array::Int64Array>()
                    .expect("the user column stays Int64")
                    .iter()
                    .flatten(),
            );
        }
    }
    values.sort_unstable();
    values
}

/// Asserts one production visibility cut names `expected` exactly once.
///
/// The cut is the same one a public query resolves against, so an object that
/// is missing from it, visible from both sources at once, or newly invented is
/// a user-visible defect regardless of what the catalog holds. Before the
/// rewrite `expected` is the sealed set, because promotion must not change
/// which objects a reader resolves; after it, `expected` is the replacement
/// set, because that is the whole point of a rewrite. Row exactness across
/// both is proven separately, by reading the objects the cut names.
///
/// # Panics
///
/// Panics when the cut cannot be pinned or does not cover `expected`.
async fn assert_exact_cut(
    catalog: &BifrostCatalog,
    table: &TableRef,
    tenant: DataTenantId,
    expected: &BTreeSet<String>,
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
        *expected,
        "{label}: the cut does not cover the expected set exactly"
    );
}

/// Promoted rows survive a rewrite whose acceptance the committer never learned.
///
/// The route runs end to end: Scribe's sealed objects are promoted by the
/// production scheduler and worker, the settled table then plans its own
/// rewrite, and that rewrite's commit is accepted by the real catalog which
/// then loses the response. The worker therefore claims nothing — the
/// operation stays open — even though the replacement is already live. The
/// successor recognises its predecessor's own snapshot from retained evidence,
/// settles that one operation, and publishes nothing a second time. What must
/// hold at every step, before promotion, after promotion, across the
/// uncertain commit, and after recovery, is that a public read of the table
/// returns exactly the rows Scribe accepted.
#[tokio::test]
#[ignore = "requires Postgres"]
async fn forge_promoted_files_rewrite_and_remain_exact_across_recovery() {
    let (_telemetry_guard, telemetry) =
        shared_process_telemetry_for_test().expect("process production telemetry");
    let checkpoint = telemetry
        .checkpoint()
        .expect("production telemetry baseline");
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "rewrite_recovery_journey").await;
    let catalog_owner = server.bifrost_catalog();
    let table_ref = fixture.binding.table_ref.clone();
    let tenant = fixture.tenant;

    let sealed = sealed_paths(&fixture).await;
    assert!(
        sealed.len() >= 2,
        "a rewrite needs more than one sealed object: {sealed:?}"
    );
    let expected = object_values(&fixture, &sealed).await;
    assert!(!expected.is_empty(), "the fixture accepted real rows");
    assert_exact_cut(&catalog_owner, &table_ref, tenant, &sealed, "sealed").await;

    // Promotion: the sealed objects become the table's live set, unchanged.
    let catalog = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    let mut forge = SupervisedForge::start_with_seams(
        &fixture,
        fixture.config.clone(),
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&fixture.object_store),
    );
    forge.run_one_success().await;
    let promoted = live_data_paths(&fixture, &fixture.binding).await;
    assert_eq!(
        promoted, sealed,
        "promotion published the writer's own objects"
    );
    assert_exact_cut(&catalog_owner, &table_ref, tenant, &sealed, "promoted").await;
    assert_eq!(
        object_values(&fixture, &promoted).await,
        expected,
        "a public read after promotion returns exactly the accepted rows"
    );

    // Rewrite: the commit lands and the catalog then loses its response.
    catalog.fail_after_next_commit();
    forge.restart_worker();
    let mut forge = forge.run_one_failure().await;
    assert_eq!(
        rewrite_phases(&fixture).await,
        vec!["prepared".to_owned()],
        "an uncertain rewrite claims no outcome ({} catalog attempts, errors {:?})",
        catalog.update_attempts(),
        forge.returned_errors()
    );

    let rewritten = live_data_paths(&fixture, &fixture.binding).await;
    assert!(
        rewritten.is_disjoint(&promoted),
        "the lost response still replaced every promoted object: {rewritten:?}"
    );
    assert_eq!(
        object_values(&fixture, &rewritten).await,
        expected,
        "the rewritten cut reads exactly the rows the promoted cut did"
    );
    assert_exact_cut(&catalog_owner, &table_ref, tenant, &rewritten, "rewritten").await;

    // Recovery: the successor settles its predecessor's operation from the
    // snapshot that predecessor left behind, and publishes nothing new.
    reclaim_stopped_claim(&fixture).await;
    forge.restart_worker();
    forge.run_one_success().await;
    assert_eq!(
        rewrite_phases(&fixture).await,
        vec!["recovered".to_owned()],
        "the successor settles the one uncertain operation, not a second"
    );
    assert_eq!(
        live_data_paths(&fixture, &fixture.binding).await,
        rewritten,
        "recovery rewrote nothing a second time"
    );

    // Replay: a further production pass over the settled table adds nothing.
    let strategies = task_strategies(&fixture).await;
    forge.schedule_once().await;
    forge.shutdown().await;
    assert_eq!(
        task_strategies(&fixture).await,
        strategies,
        "a settled table plans no duplicate publication"
    );
    assert_eq!(
        rewrite_phases(&fixture).await,
        vec!["recovered".to_owned()],
        "a settled operation is never settled twice"
    );
    let final_cut = live_data_paths(&fixture, &fixture.binding).await;
    assert_eq!(final_cut, rewritten, "the rewritten cut is stable");
    assert_eq!(
        object_values(&fixture, &final_cut).await,
        expected,
        "the public read after recovery still returns exactly the accepted rows"
    );
    assert_exact_cut(&catalog_owner, &table_ref, tenant, &final_cut, "recovered").await;

    let delta = telemetry
        .delta_since(&checkpoint)
        .expect("production telemetry delta");
    let families = delta
        .metrics
        .iter()
        .map(|sample| sample.family.clone())
        .collect::<BTreeSet<_>>();
    assert!(
        families.contains("bifrost_forge_operations"),
        "the production route reported its operation telemetry: {families:?}"
    );
}
