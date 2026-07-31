mod pg_tests {
    //! Integration coverage for Forge's exact nonterminal staging predicate.

    use sqlx::types::Uuid;
    use vala_sql::TenantConn;
    use vala_sql::queries::file_list::list_nonterminal_file_paths;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;

    /// Inserts one staging row with the requested lifecycle markers.
    ///
    /// # Panics
    ///
    /// Panics when fixture SQL cannot arrange the test row.
    async fn insert_row(
        pool: &sqlx::PgPool,
        tenant: DataTenantId,
        path: &str,
        compacted: bool,
        committed_snapshot_id: Option<i64>,
    ) {
        sqlx::query(
            r#"
            INSERT INTO vala.file_list (
                id, data_tenant_id, namespace, table_name, file_path,
                file_size, row_count, min_event_time, max_event_time,
                partition_day, node_id, writer_epoch, wal_lsn_min, wal_lsn_max,
                compacted, committed_snapshot_id
            ) VALUES (
                $1, $2, 'vala.traces', 'spans', $3,
                1024, 100, now(), now(), current_date,
                $4, 1, 100, 200, $5, $6
            )
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(tenant.as_uuid())
        .bind(path)
        .bind(Uuid::now_v7())
        .bind(compacted)
        .bind(committed_snapshot_id)
        .execute(pool)
        .await
        .expect("insert staging row");
    }

    /// Proves the exact lifecycle conjunction, focused narrowing, ordering, and RLS scope.
    ///
    /// # Panics
    ///
    /// Panics when the PostgreSQL fixture or asserted tenant query fails.
    #[tokio::test]
    async fn nonterminal_file_list_paths_are_tenant_scoped() {
        let fixture = PgFixture::start().await.expect("fixture");
        let pool = fixture.superuser_pool().await.expect("superuser pool");
        let tenant_a = fixture.data_tenant_id();
        let tenant_b = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(tenant_b, "forge-file-list-b")
            .await
            .expect("seed second tenant");

        insert_row(&pool, tenant_a, "table/a.parquet", false, None).await;
        insert_row(&pool, tenant_a, "table/b.parquet", false, Some(1)).await;
        insert_row(&pool, tenant_a, "table/c.parquet", true, None).await;
        insert_row(&pool, tenant_a, "table/d.parquet", true, Some(1)).await;
        insert_row(&pool, tenant_b, "table/other.parquet", false, None).await;

        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant_a)
            .await
            .expect("tenant connection");
        let broad = list_nonterminal_file_paths(&mut conn, "vala.traces", "spans", None)
            .await
            .expect("broad nonterminal read");
        assert_eq!(
            broad,
            vec![
                "table/a.parquet".to_owned(),
                "table/b.parquet".to_owned(),
                "table/c.parquet".to_owned()
            ]
        );
        let focused =
            list_nonterminal_file_paths(&mut conn, "vala.traces", "spans", Some("table/c.parquet"))
                .await
                .expect("focused nonterminal read");
        assert_eq!(focused, vec!["table/c.parquet".to_owned()]);
        let terminal =
            list_nonterminal_file_paths(&mut conn, "vala.traces", "spans", Some("table/d.parquet"))
                .await
                .expect("focused terminal read");
        assert!(terminal.is_empty());
        conn.commit().await.expect("commit read transaction");
    }
}
