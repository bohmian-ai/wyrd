//! 2PC idempotency FSM coverage for `run_commit`.
//!
//! Exercises the `committed` / `failed` / `precommit` dispatch branches that the
//! engine takes BEFORE writing any Parquet (`lookup_idempotent` → early return),
//! plus the corruption guard for a `committed` row with a NULL `snapshot_id`.
//!
//! Uses a bare `#[sqlx::test]` + in-body `migrate_for_test` (the vala migrations
//! depend on wyrd-sql prerequisites and cannot be applied via a `migrations=`
//! arg). Run with a live Postgres: `DATABASE_URL=... cargo test -p vala-bifrost
//! --all-features --test commit_idempotency`.

use std::sync::Arc;

use iceberg::Catalog as _;
use sqlx::PgPool;
use tempfile::TempDir;
use vala_bifrost::catalog::WyrdCatalog;
use vala_bifrost::catalog::iceberg_sql;
use vala_bifrost::catalog::namespaces::BifrostNamespace;
use vala_bifrost::error::BifrostError;
use vala_bifrost::types::{TableScope, TableUid};
use vala_bifrost::writer::commit::run_commit;
use wyrd_spec::ids::DataTenantId;
use wyrd_storage::factory::iceberg_factory::iceberg_storage_factory;
use wyrd_storage::settings::BackendConfig;

const NS: BifrostNamespace = BifrostNamespace::Bifrost;
const TABLE: &str = "idem_test";
const BATCH_ID: [u8; 16] = [0x42; 16];

/// Live-catalog fixture: a registered `TenantOwned` table plus a separately-built
/// `SqlCatalog` handle (the same catalog DB) so the test can call `run_commit`
/// directly with a controlled `batch_id`.
struct Fixture {
    _tmp: TempDir,
    catalog: iceberg_catalog_sql::SqlCatalog,
    pool: Arc<PgPool>,
    table: iceberg::table::Table,
    table_uid: TableUid,
    tenant: DataTenantId,
}

async fn setup(pool: PgPool) -> Fixture {
    let pool = Arc::new(pool);
    vala_sql::testing::migrate_for_test(&pool).await.unwrap();

    let tenant = DataTenantId::new_v7();
    vala_sql::testing::seed_tenant(&pool, tenant.as_uuid())
        .await
        .unwrap();

    let tmp = tempfile::tempdir().unwrap();
    let warehouse = format!("file://{}", tmp.path().display());
    let backend = BackendConfig::Local {
        root: tmp.path().to_path_buf(),
    };
    let (factory, props) = iceberg_storage_factory(&backend).unwrap();
    let catalog_uri = vala_sql::testing::catalog_uri(&pool);

    let wyrd_catalog = WyrdCatalog::new(
        &catalog_uri,
        &warehouse,
        pool.clone(),
        factory.clone(),
        props.clone(),
    )
    .await
    .unwrap();

    let table_uid = wyrd_catalog
        .create_table(
            NS,
            TABLE,
            vec![arrow::datatypes::Field::new(
                "val",
                arrow::datatypes::DataType::Int64,
                false,
            )],
            TableScope::TenantOwned,
            tenant,
            &[],
        )
        .await
        .unwrap();

    // A second SqlCatalog over the same catalog DB to obtain a loadable `Table`.
    let catalog = iceberg_sql::build_catalog(&catalog_uri, &warehouse, factory, props)
        .await
        .unwrap();
    let ident = iceberg::TableIdent::new(NS.to_namespace_ident(), TABLE.to_string());
    let table = catalog.load_table(&ident).await.unwrap();

    Fixture {
        _tmp: tmp,
        catalog,
        pool,
        table,
        table_uid,
        tenant,
    }
}

async fn conn(fx: &Fixture) -> vala_sql::TenantConn<'_> {
    vala_sql::TenantConn::acquire(&fx.pool, fx.tenant)
        .await
        .unwrap()
}

#[sqlx::test]
async fn idempotent_replay_committed_returns_prior_snapshot(pool: PgPool) {
    let fx = setup(pool).await;

    let mut c = conn(&fx).await;
    vala_sql::queries::olap_catalog::precommit(&mut c, fx.table_uid.as_bytes(), &BATCH_ID)
        .await
        .unwrap();
    vala_sql::queries::olap_catalog::finalize_committed(
        &mut c,
        fx.table_uid.as_bytes(),
        &BATCH_ID,
        4242,
    )
    .await
    .unwrap();
    c.commit().await.unwrap();

    let result = run_commit(
        &fx.pool,
        &fx.catalog,
        &fx.table,
        &fx.table_uid,
        Vec::new(),
        BATCH_ID,
        fx.tenant,
    )
    .await
    .unwrap();

    assert_eq!(result.0, 4242, "replay returns the prior snapshot id");
    assert!(
        result.1.is_none(),
        "replay must not return a fresh table snapshot"
    );
}

#[sqlx::test]
async fn committed_row_with_null_snapshot_is_metadata_mismatch(pool: PgPool) {
    let fx = setup(pool).await;

    let mut c = conn(&fx).await;
    vala_sql::queries::olap_catalog::precommit(&mut c, fx.table_uid.as_bytes(), &BATCH_ID)
        .await
        .unwrap();
    // Force the corruption case: committed state with snapshot_id left NULL.
    sqlx::query(
        "UPDATE vala.olap_commits SET state = 'committed', committed_at = now() \
         WHERE table_uid = $1 AND batch_id = $2",
    )
    .bind(fx.table_uid.as_bytes().as_slice())
    .bind(BATCH_ID.as_slice())
    .execute(&mut **c.transaction())
    .await
    .unwrap();
    c.commit().await.unwrap();

    let err = run_commit(
        &fx.pool,
        &fx.catalog,
        &fx.table,
        &fx.table_uid,
        Vec::new(),
        BATCH_ID,
        fx.tenant,
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err, BifrostError::MetadataMismatch(_)),
        "committed+NULL snapshot must surface as MetadataMismatch, not a silent Ok(0): {err:?}"
    );
}

#[sqlx::test]
async fn duplicate_failed_batch_returns_error(pool: PgPool) {
    let fx = setup(pool).await;

    let mut c = conn(&fx).await;
    vala_sql::queries::olap_catalog::precommit(&mut c, fx.table_uid.as_bytes(), &BATCH_ID)
        .await
        .unwrap();
    vala_sql::queries::olap_catalog::finalize_failed(
        &mut c,
        fx.table_uid.as_bytes(),
        &BATCH_ID,
        "WYRD_VALA_500_BIFROST_INTERNAL",
        "prior failure",
    )
    .await
    .unwrap();
    c.commit().await.unwrap();

    let err = run_commit(
        &fx.pool,
        &fx.catalog,
        &fx.table,
        &fx.table_uid,
        Vec::new(),
        BATCH_ID,
        fx.tenant,
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err, BifrostError::DuplicateFailedBatch(_)),
        "a prior failed batch_id must reject: {err:?}"
    );
}

#[sqlx::test]
async fn in_flight_precommit_returns_commit_conflict(pool: PgPool) {
    let fx = setup(pool).await;

    let mut c = conn(&fx).await;
    vala_sql::queries::olap_catalog::precommit(&mut c, fx.table_uid.as_bytes(), &BATCH_ID)
        .await
        .unwrap();
    c.commit().await.unwrap();

    let err = run_commit(
        &fx.pool,
        &fx.catalog,
        &fx.table,
        &fx.table_uid,
        Vec::new(),
        BATCH_ID,
        fx.tenant,
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err, BifrostError::CommitConflict(_)),
        "an in-flight precommit row must surface as CommitConflict: {err:?}"
    );
}
