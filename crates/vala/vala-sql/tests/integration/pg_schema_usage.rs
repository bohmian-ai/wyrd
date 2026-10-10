mod pg_tests {
    //! The separate tenant login reaches `vala` through the documented
    //! operator setup alone.
    //!
    //! The test harness gives the tenant login only `USAGE` on the schema and
    //! default table privileges, as the self-hosting guide tells a DBA to. An
    //! INSERT into `vala.bifrost_tables` through the tenant pool proves that
    //! setup suffices; without it Postgres refuses with 42501.
    //!
    //! Run via `mise run test:sql`.

    use vala_sql::TenantConn;
    use vala_sql::queries::olap_catalog::upsert_table;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;

    /// Canonical default layout JSON stored on every registration in this file.
    ///
    /// The catalog column is opaque JSON to `vala-sql`; these tests only need a
    /// value that matches the wire contract the server persists, so they build
    /// the `hour(wyrd_event_time)` default rather than a bespoke shape.
    fn layout_fixture() -> serde_json::Value {
        serde_json::json!({
            "partition": { "column": "wyrd_event_time", "granularity": "hour" },
            "sort_keys": [],
            "bloom_columns": []
        })
    }

    #[tokio::test]
    async fn tenant_login_registers_bifrost_table() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(
                tenant,
                &format!("test-{}", tenant.as_uuid().simple()),
            )
            .await
            .unwrap();

        // `fixture.app_pool()` connects as the separate tenant login the request
        // path uses, so the insert is gated by the operator's grants.
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .unwrap();

        let table_uid = [1u8; 16];
        let fingerprint = [0u8; 32];
        upsert_table(
            &mut conn,
            &table_uid,
            "space.table",
            &fingerprint,
            &layout_fixture(),
        )
        .await
        .expect("the tenant login reaches vala.bifrost_tables");

        conn.commit().await.unwrap();
    }

    #[tokio::test]
    async fn fresh_schema_is_tenant_only_and_keys_are_tenant_qualified() {
        let fixture = PgFixture::start().await.expect("fixture");
        let superuser = fixture.superuser_pool().expect("superuser pool");
        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        for tenant in [tenant_a, tenant_b] {
            fixture
                .seed_additional_tenant_with_uuid(
                    tenant,
                    &format!("test-{}", tenant.as_uuid().simple()),
                )
                .await
                .unwrap();
        }

        for tenant in [tenant_a, tenant_b] {
            let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
                .await
                .unwrap();
            upsert_table(
                &mut conn,
                &[7_u8; 16],
                "datasets.same",
                &[3_u8; 32],
                &layout_fixture(),
            )
            .await
            .unwrap();
            conn.commit().await.unwrap();
        }

        for (table, column) in [("bifrost_tables", "scope"), ("file_list", "tenant_bucket")] {
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM information_schema.columns
                 WHERE table_schema = 'vala' AND table_name = $1 AND column_name = $2",
            )
            .bind(table)
            .bind(column)
            .fetch_one(&superuser)
            .await
            .unwrap();
            assert_eq!(count, 0, "forbidden column {table}.{column} is absent");
        }

        let system_tenant = uuid::Uuid::from(DataTenantId::SYSTEM_OWNER);
        let system_tenant_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM platform.tenants WHERE data_tenant_id = $1")
                .bind(system_tenant)
                .fetch_one(&superuser)
                .await
                .unwrap();
        assert_eq!(
            system_tenant_count, 1,
            "fresh migrations retain exactly one canonical system tenant"
        );

        let table_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM vala.bifrost_tables WHERE table_uid = $1")
                .bind([7_u8; 16].as_slice())
                .fetch_one(&superuser)
                .await
                .unwrap();
        assert_eq!(
            table_count, 2,
            "same table identity is independent per tenant"
        );

        for index_name in [
            "file_list_group_idx",
            "file_list_tenant_idx",
            "file_list_live_tail_watermark_idx",
            "file_list_unpublished_idx",
        ] {
            let definition: String = sqlx::query_scalar(
                "SELECT indexdef FROM pg_indexes WHERE schemaname = 'vala' AND indexname = $1",
            )
            .bind(index_name)
            .fetch_one(&superuser)
            .await
            .unwrap();
            assert!(
                definition.replace(' ', "").contains("(data_tenant_id,"),
                "{index_name} is tenant-first"
            );
        }
    }
}
