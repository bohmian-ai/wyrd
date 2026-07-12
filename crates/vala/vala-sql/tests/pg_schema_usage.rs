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
    use vala_sql::queries::olap_catalog::upsert_table;
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
            "tenant_owned",
            &partition_columns,
        )
        .await
        .expect("wyrd_app must reach vala.bifrost_tables once USAGE ON SCHEMA vala is granted");

        conn.commit().await.unwrap();
    }
}
