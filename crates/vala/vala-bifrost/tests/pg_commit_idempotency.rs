mod pg_tests {
    //! 2PC idempotency FSM coverage for `run_commit`.
    //!
    //! Exercises the `committed` / `failed` / `precommit` dispatch branches that the
    //! engine takes BEFORE writing any Parquet (`lookup_idempotent` → early return),
    //! plus the corruption guard for a `committed` row with a NULL `snapshot_id`.
    //!
    //! Run with `mise run test:bifrost`.

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
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::ids::DataTenantId;
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
        _db: PgFixture,
    }

    async fn setup() -> Fixture {
        let fixture = PgFixture::start().await.expect("fixture");
        let pool = Arc::new(fixture.app_pool().clone());

        let tenant = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(
                tenant,
                &format!("test-{}", tenant.as_uuid().simple()),
            )
            .await
            .unwrap();

        let tmp = tempfile::tempdir().unwrap();
        let backend = BackendConfig::Local {
            root: tmp.path().to_path_buf(),
        };
        let catalog_uri = fixture.catalog_uri();

        let wyrd_catalog = WyrdCatalog::new(&catalog_uri, &backend, pool.clone(), None)
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
                None,
            )
            .await
            .unwrap();

        // A second SqlCatalog over the same catalog DB to obtain a loadable `Table`.
        let (factory, props) = vala_bifrost::catalog::storage::iceberg_storage_factory(&backend);
        let warehouse = vala_bifrost::catalog::storage::warehouse_uri(&backend);
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
            _db: fixture,
        }
    }

    async fn conn(fx: &Fixture) -> vala_sql::TenantConn<'_> {
        vala_sql::TenantConn::acquire(&fx.pool, fx.tenant)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn idempotent_replay_committed_returns_prior_snapshot() {
        let fx = setup().await;

        let mut c = conn(&fx).await;
        vala_sql::queries::olap_catalog::precommit(
            &mut c,
            fx.table_uid.as_bytes(),
            &BATCH_ID,
            "system",
            "system",
        )
        .await
        .unwrap();
        // Stamp owner + token within the same conn to avoid row-lock deadlock.
        let owner = sqlx::types::Uuid::new_v4();
        sqlx::query(
            "UPDATE vala.olap_commits SET writer_owner = $1, writer_fencing_token = 1 \
         WHERE table_uid = $2 AND batch_id = $3",
        )
        .bind(owner)
        .bind(fx.table_uid.as_bytes().as_slice())
        .bind(BATCH_ID.as_slice())
        .execute(&mut **c.transaction())
        .await
        .unwrap();
        vala_sql::queries::olap_catalog::finalize_committed(
            &mut c,
            fx.table_uid.as_bytes(),
            &BATCH_ID,
            4242,
            owner,
            1,
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
            "system",
            "system",
            fx.tenant,
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.0, 4242, "replay returns the prior snapshot id");
        assert!(
            result.1.is_none(),
            "replay must not return a fresh table snapshot"
        );
    }

    #[tokio::test]
    async fn committed_row_with_null_snapshot_is_metadata_mismatch() {
        let fx = setup().await;

        let mut c = conn(&fx).await;
        vala_sql::queries::olap_catalog::precommit(
            &mut c,
            fx.table_uid.as_bytes(),
            &BATCH_ID,
            "system",
            "system",
        )
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
            "system",
            "system",
            fx.tenant,
            None,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(err, BifrostError::MetadataMismatch(_)),
            "committed+NULL snapshot must surface as MetadataMismatch, not a silent Ok(0): {err:?}"
        );
    }

    #[tokio::test]
    async fn duplicate_failed_batch_returns_error() {
        let fx = setup().await;

        let mut c = conn(&fx).await;
        vala_sql::queries::olap_catalog::precommit(
            &mut c,
            fx.table_uid.as_bytes(),
            &BATCH_ID,
            "system",
            "system",
        )
        .await
        .unwrap();
        // Stamp owner + token within the same conn to avoid row-lock deadlock.
        let owner = sqlx::types::Uuid::new_v4();
        sqlx::query(
            "UPDATE vala.olap_commits SET writer_owner = $1, writer_fencing_token = 1 \
         WHERE table_uid = $2 AND batch_id = $3",
        )
        .bind(owner)
        .bind(fx.table_uid.as_bytes().as_slice())
        .bind(BATCH_ID.as_slice())
        .execute(&mut **c.transaction())
        .await
        .unwrap();
        vala_sql::queries::olap_catalog::finalize_failed(
            &mut c,
            fx.table_uid.as_bytes(),
            &BATCH_ID,
            "WYRD_VALA_500_BIFROST_INTERNAL",
            "prior failure",
            owner,
            1,
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
            "system",
            "system",
            fx.tenant,
            None,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(err, BifrostError::DuplicateFailedBatch(_)),
            "a prior failed batch_id must reject: {err:?}"
        );
    }

    #[tokio::test]
    async fn in_flight_precommit_returns_commit_conflict() {
        let fx = setup().await;

        let mut c = conn(&fx).await;
        vala_sql::queries::olap_catalog::precommit(
            &mut c,
            fx.table_uid.as_bytes(),
            &BATCH_ID,
            "system",
            "system",
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
            "system",
            "system",
            fx.tenant,
            None,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(err, BifrostError::CommitConflict(_)),
            "an in-flight precommit row must surface as CommitConflict: {err:?}"
        );
    }
}
