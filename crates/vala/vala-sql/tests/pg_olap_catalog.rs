mod pg_tests {
    //! PostgreSQL integration coverage for the operator-owned Bifrost roster.

    use vala_sql::queries::forge_catalog_operator::list_active_tables_for_operator;
    use vala_sql::queries::olap_catalog::upsert_table;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;
    use wyrd_sql::TenantConn;

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

    /// Proves Forge sees only active registrations in tenant/FQN order.
    #[tokio::test]
    async fn active_table_inventory_is_operator_visible_ordered_and_status_filtered() {
        let fixture = PgFixture::start().await.expect("fixture");
        let op = fixture.operator_pool();
        let first_tenant = DataTenantId::new_v7();
        let second_tenant = DataTenantId::new_v7();
        for tenant in [first_tenant, second_tenant] {
            fixture
                .seed_additional_tenant_with_uuid(
                    tenant,
                    &format!("test-{}", tenant.as_uuid().simple()),
                )
                .await
                .expect("seed tenant");
        }
        for (tenant, uid, fqn) in [
            (second_tenant, [2_u8; 16], "vala.bifrost.zeta"),
            (first_tenant, [1_u8; 16], "vala.bifrost.alpha"),
            (first_tenant, [3_u8; 16], "vala.bifrost.hidden"),
        ] {
            let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
                .await
                .expect("tenant connection");
            upsert_table(&mut conn, &uid, fqn, &[7_u8; 32], &layout_fixture())
                .await
                .expect("insert registration");
            conn.commit().await.expect("commit registration");
        }
        let superuser = fixture.superuser_pool().await.expect("superuser");
        sqlx::query("UPDATE vala.bifrost_tables SET status = 'deprecated' WHERE data_tenant_id = $1 AND fqn = $2")
            .bind(first_tenant.as_uuid())
            .bind("vala.bifrost.hidden")
            .execute(&superuser)
            .await
            .expect("deprecate row");
        let rows = list_active_tables_for_operator(op)
            .await
            .expect("list active rows");
        assert_eq!(rows.len(), 2);
        assert!(
            rows.windows(2)
                .all(|pair| (pair[0].data_tenant_id, &pair[0].fqn)
                    < (pair[1].data_tenant_id, &pair[1].fqn))
        );
        assert!(rows.iter().all(|row| row.status == "active"));
    }
}
