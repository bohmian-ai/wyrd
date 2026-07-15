mod pg_tests {
    use std::sync::Arc;

    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use vala_bifrost::batch_builder::stamp_system_columns;
    use vala_bifrost::schema::bifrost_schema;
    use vala_bifrost::types::{SchemaFingerprint, TableScope};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::vala::system_columns::{DATA_TENANT_ID, WYRD_BATCH_ID, WYRD_INGESTED_AT};

    #[test]
    fn schema_has_correct_field_count_tenant_owned() {
        let user_fields = vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
        ];
        let schema = bifrost_schema(user_fields, TableScope::TenantOwned);
        assert_eq!(
            schema.fields().len(),
            8,
            "2 user + 3 correlation (run_id, card_uid, principal_id) + 3 system (no tenant_id)"
        );
        assert!(schema.field_with_name(WYRD_BATCH_ID).is_ok());
        assert!(schema.field_with_name(WYRD_INGESTED_AT).is_ok());
        assert!(schema.field_with_name("run_id").is_ok());
        assert!(schema.field_with_name("card_uid").is_ok());
        assert!(schema.field_with_name("principal_id").is_ok());
        assert!(schema.field_with_name(DATA_TENANT_ID).is_err());
    }

    #[test]
    fn schema_has_correct_field_count_system_shared() {
        let user_fields = vec![Field::new("metric", DataType::Int64, false)];
        let schema = bifrost_schema(user_fields, TableScope::SystemShared);
        assert_eq!(
            schema.fields().len(),
            8,
            "1 user + 3 correlation (run_id, card_uid, principal_id) + 4 system (with tenant_id)"
        );
        assert!(schema.field_with_name(DATA_TENANT_ID).is_ok());
        assert!(schema.field_with_name("run_id").is_ok());
        assert!(schema.field_with_name("card_uid").is_ok());
        assert!(schema.field_with_name("principal_id").is_ok());
    }

    #[test]
    fn fingerprint_is_deterministic() {
        let schema = Schema::new(vec![
            Field::new("a", DataType::Int64, false),
            Field::new("b", DataType::Utf8, true),
        ]);
        let f1 = SchemaFingerprint::from_arrow_schema(&schema);
        let f2 = SchemaFingerprint::from_arrow_schema(&schema);
        assert_eq!(f1, f2);
    }

    #[test]
    fn fingerprint_differs_on_field_change() {
        let s1 = Schema::new(vec![Field::new("a", DataType::Int64, false)]);
        let s2 = Schema::new(vec![Field::new("a", DataType::Utf8, false)]);
        assert_ne!(
            SchemaFingerprint::from_arrow_schema(&s1),
            SchemaFingerprint::from_arrow_schema(&s2)
        );
    }

    #[test]
    fn stamp_system_columns_appends_batch_id_and_timestamps() {
        let schema = Arc::new(Schema::new(vec![Field::new("val", DataType::Int64, false)]));
        let batch =
            RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(vec![1_i64, 2, 3]))])
                .unwrap();

        let batch_id = *uuid::Uuid::now_v7().as_bytes();
        let now_us = 1_700_000_000_000_000_i64;

        let stamped = stamp_system_columns(&batch, now_us, batch_id, None).unwrap();
        assert_eq!(stamped.num_rows(), 3);
        assert!(stamped.schema().field_with_name(WYRD_BATCH_ID).is_ok());
        assert!(stamped.schema().field_with_name(DATA_TENANT_ID).is_err());
    }

    #[test]
    fn stamp_system_columns_appends_tenant_id_when_provided() {
        use wyrd_spec::ids::DataTenantId;

        let schema = Arc::new(Schema::new(vec![Field::new("x", DataType::Int64, false)]));
        let batch =
            RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(vec![42_i64]))]).unwrap();

        let tenant = DataTenantId::new_v7();
        let stamped =
            stamp_system_columns(&batch, 0, *uuid::Uuid::now_v7().as_bytes(), Some(tenant))
                .unwrap();

        assert!(stamped.schema().field_with_name(DATA_TENANT_ID).is_ok());
        let col = stamped
            .column_by_name(DATA_TENANT_ID)
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(col.value(0), tenant.to_string());
    }

    /// Full catalog round-trip: create table, write, read back.
    #[tokio::test]
    async fn round_trip_full() {
        let fixture = PgFixture::start().await.expect("fixture");
        let catalog_uri = fixture.catalog_uri();
        let pool = Arc::new(fixture.app_pool().clone());

        let tmp = tempfile::tempdir().unwrap();
        let backend = wyrd_storage::settings::BackendConfig::Local {
            root: tmp.path().to_path_buf(),
        };
        let catalog =
            vala_bifrost::catalog::WyrdCatalog::new(&catalog_uri, &backend, pool.clone(), None)
                .await
                .unwrap();

        let tenant = wyrd_spec::ids::DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(
                tenant,
                &format!("test-{}", tenant.as_uuid().simple()),
            )
            .await
            .unwrap();

        let user_fields = vec![Field::new("val", DataType::Int64, false)];
        let uid = catalog
            .create_table(vala_bifrost::catalog::CreateTableRequest {

                ns: vala_bifrost::catalog::namespaces::BifrostNamespace::Bifrost,

                name: "rt_test",

                user_fields: user_fields,

                scope: TableScope::TenantOwned,

                tenant: tenant,

                partition_columns: &[],

                audit: None,

            })
            .await
            .unwrap();

        assert_ne!(uid.to_string(), uuid::Uuid::nil().to_string());
        let writer = catalog
            .writer(
                vala_bifrost::catalog::namespaces::BifrostNamespace::Bifrost,
                "rt_test",
                TableScope::TenantOwned,
                tenant,
            )
            .await
            .unwrap();

        // User fields only — the write path server-stamps every system column. A
        // batch carrying wyrd_* / data_tenant_id columns is rejected.
        let user_schema = Arc::new(arrow::datatypes::Schema::new(vec![Field::new(
            "val",
            DataType::Int64,
            false,
        )]));
        let batch = RecordBatch::try_new(
            user_schema,
            vec![Arc::new(Int64Array::from(vec![1_i64, 2, 3]))],
        )
        .unwrap();

        let snapshot_id = writer
            .commit_one(
                tenant,
                vec![batch],
                vala_bifrost::writer::BifrostWriteContext::system(),
            )
            .await
            .unwrap();
        assert!(
            snapshot_id > 0,
            "snapshot_id should be positive after commit"
        );
    }

    /// `register_all` runs on every server boot, so a re-boot against an already
    /// provisioned catalog must stay `Ok`. Guards against the steady-state
    /// `physical_matches_declared` check reporting spurious `PhysicalDrift` on
    /// Iceberg type normalizations (e.g. declared `UInt32` reads back as `Int64`).
    #[tokio::test]
    async fn register_all_is_idempotent_across_boots() {
        let fixture = PgFixture::start().await.expect("fixture");
        let catalog_uri = fixture.catalog_uri();
        let pool = Arc::new(fixture.app_pool().clone());
        let tmp = tempfile::tempdir().unwrap();
        let backend = wyrd_storage::settings::BackendConfig::Local {
            root: tmp.path().to_path_buf(),
        };
        let catalog = Arc::new(
            vala_bifrost::catalog::WyrdCatalog::new(&catalog_uri, &backend, pool.clone(), None)
                .await
                .unwrap(),
        );

        vala_bifrost::tables::register_all(&catalog)
            .await
            .expect("first register_all provisions all domain tables");
        vala_bifrost::tables::register_all(&catalog)
            .await
            .expect("second register_all is idempotent (steady-state)");
    }
}
