mod pg_tests {
    //! Regression test for S3.C2perm: wyrd_app must hold USAGE ON SCHEMA vala.
    //!
    //! Migration `20260801000002_vala_app_schema_usage` grants USAGE so the existing
    //! per-table DML grants on `vala.*` become reachable. Without it, an INSERT into
    //! `vala.bifrost_tables` through the request-path `wyrd_app` role fails with
    //! Postgres 42501 (permission denied for schema vala) -- the exact gap the
    //! shipped migrations left. This test uses the shared-DB `db.app` pool, which
    //! connects as the runtime `wyrd_app` role, so the insert is gated by the same
    //! grants production enforces; it would fail 42501 on base.
    //!
    //! Run via `mise run test:sql`.

    use vala_sql::TenantConn;
    use vala_sql::queries::olap_catalog::{precommit, upsert_table};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;

    #[tokio::test]
    async fn schema_usage_grant_lets_wyrd_app_register_bifrost_table() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(
                tenant,
                &format!("test-{}", tenant.as_uuid().simple()),
            )
            .await
            .unwrap();

        // `fixture.app_pool()` connects as the runtime `wyrd_app` role — the same
        // RLS-enforced, non-BYPASSRLS identity the request path uses. No `SET ROLE`
        // dance is needed: the insert is naturally gated by the grants production
        // enforces, and without USAGE ON SCHEMA vala it 42501s.
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .unwrap();

        let table_uid = [1u8; 16];
        let fingerprint = [0u8; 32];
        let partition_columns = vec!["day".to_string()];

        upsert_table(
            &mut conn,
            &table_uid,
            "space.table",
            &fingerprint,
            &partition_columns,
        )
        .await
        .expect("wyrd_app must reach vala.bifrost_tables once USAGE ON SCHEMA vala is granted");

        conn.commit().await.unwrap();
    }

    #[tokio::test]
    async fn fresh_schema_is_tenant_only_and_keys_are_tenant_qualified() {
        let fixture = PgFixture::start().await.expect("fixture");
        let superuser = fixture.superuser_pool().await.expect("superuser pool");
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
            upsert_table(&mut conn, &[7; 16], "datasets.same", &[3; 32], &[])
                .await
                .unwrap();
            precommit(&mut conn, &[7; 16], &[9; 16], "test", "test")
                .await
                .unwrap();
            precommit(&mut conn, &[7; 16], &[9; 16], "test", "test")
                .await
                .unwrap();
            conn.commit().await.unwrap();
        }

        for (table, column) in [
            ("bifrost_tables", "scope"),
            ("olap_commits", "control_bind"),
            ("file_list", "tenant_bucket"),
        ] {
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

        let nil_tenant = uuid::Uuid::nil();
        let nil_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM platform.tenants WHERE data_tenant_id = $1")
                .bind(nil_tenant)
                .fetch_one(&superuser)
                .await
                .unwrap();
        assert_eq!(nil_count, 0, "fresh migrations do not create a nil tenant");

        let table_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM vala.bifrost_tables WHERE table_uid = $1")
                .bind([7; 16].as_slice())
                .fetch_one(&superuser)
                .await
                .unwrap();
        let commit_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.olap_commits WHERE table_uid = $1 AND batch_id = $2",
        )
        .bind([7; 16].as_slice())
        .bind([9; 16].as_slice())
        .fetch_one(&superuser)
        .await
        .unwrap();
        assert_eq!(
            table_count, 2,
            "same table identity is independent per tenant"
        );
        assert_eq!(
            commit_count, 2,
            "same batch identity is independent per tenant"
        );

        for index_name in [
            "file_list_group_idx",
            "file_list_tenant_idx",
            "file_list_live_tail_watermark_idx",
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
