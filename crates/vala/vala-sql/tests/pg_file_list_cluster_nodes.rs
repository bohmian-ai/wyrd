mod pg_tests {
    //! Integration tests for vala.file_list and vala.cluster_nodes migrations.
    //!
    //! Tests verify:
    //! - Migration applies cleanly (columns, types, nullability)
    //! - Indexes exist (watermark, duplicate-range guard, discovery)
    //! - RLS blocks cross-tenant SELECT on file_list
    //! - TenantConn grants permit required operations
    //! - Duplicate stream range INSERT fails on unique constraint
    //!
    //! Skipped when WYRD_DATABASE_URL is unset (credential-free default suite).

    use sqlx::types::Uuid;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;

    async fn setup() -> (PgFixture, DataTenantId) {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(
                tenant,
                &format!("test-{}", tenant.as_uuid().simple()),
            )
            .await
            .unwrap();
        (fixture, tenant)
    }

    async fn count_file_list_rows(
        conn: &mut vala_sql::TenantConn<'_>,
        file_id: Uuid,
    ) -> Result<i64, vala_sql::SqlError> {
        let tx = conn.transaction();
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM vala.file_list WHERE id = $1")
            .bind(file_id)
            .fetch_one(&mut **tx)
            .await
            .map_err(vala_sql::SqlError::Query)?;
        Ok(row.0)
    }

    #[tokio::test]
    async fn vala_file_list_migration_applies_cleanly() {
        let (fixture, _tenant) = setup().await;
        let pool = fixture.superuser_pool().await.expect("superuser pool");

        // Assert every file_list column present with matching type/nullability.
        let columns: Vec<(String, String, String)> = sqlx::query_as(
            r#"
            SELECT column_name, data_type, is_nullable
            FROM information_schema.columns
            WHERE table_schema = 'vala' AND table_name = 'file_list'
            ORDER BY ordinal_position
            "#,
        )
        .fetch_all(&pool)
        .await
        .expect("column query");

        let expected = [
            ("id", "uuid", "NO"),
            ("data_tenant_id", "uuid", "NO"),
            ("namespace", "text", "NO"),
            ("table_name", "text", "NO"),
            ("file_path", "text", "NO"),
            ("file_size", "bigint", "NO"),
            ("row_count", "bigint", "NO"),
            ("min_event_time", "timestamp with time zone", "NO"),
            ("max_event_time", "timestamp with time zone", "NO"),
            ("partition_day", "date", "NO"),
            ("compacted", "boolean", "NO"),
            ("committed_snapshot_id", "bigint", "YES"),
            ("node_id", "uuid", "NO"),
            ("writer_epoch", "bigint", "NO"),
            ("wal_lsn_min", "bigint", "NO"),
            ("wal_lsn_max", "bigint", "NO"),
            ("created_at", "timestamp with time zone", "NO"),
            ("publication_operation_id", "uuid", "YES"),
            ("file_ordinal", "smallint", "NO"),
            ("file_checksum", "text", "YES"),
        ];

        assert_eq!(
            columns.len(),
            expected.len(),
            "file_list column count matches schema"
        );

        for ((col_name, col_type, nullable), (exp_name, exp_type, exp_nullable)) in
            columns.iter().zip(expected.iter())
        {
            assert_eq!(col_name, exp_name, "column name");
            assert_eq!(col_type, exp_type, "column type for {}", col_name);
            assert_eq!(nullable, exp_nullable, "nullability for {}", col_name);
        }
    }

    #[tokio::test]
    async fn vala_file_list_watermark_index_exists() {
        let (fixture, _tenant) = setup().await;
        let pool = fixture.superuser_pool().await.expect("superuser pool");

        // Verify the live-tail watermark composite index exists.
        let idx: (bool,) = sqlx::query_as(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM pg_indexes
                WHERE schemaname = 'vala'
                  AND tablename = 'file_list'
                  AND indexname = 'file_list_live_tail_watermark_idx'
            )
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("index query");

        assert!(
            idx.0,
            "file_list_live_tail_watermark_idx exists for Oracle dedup"
        );

        // Assert index columns match schema.
        let def: (String,) = sqlx::query_as(
            r#"
            SELECT indexdef FROM pg_indexes
            WHERE schemaname = 'vala' AND indexname = 'file_list_live_tail_watermark_idx'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("indexdef query");

        assert!(
            def.0.contains("namespace"),
            "watermark index includes namespace"
        );
        assert!(
            def.0.contains("table_name"),
            "watermark index includes table_name"
        );
        assert!(
            def.0.contains("data_tenant_id"),
            "watermark index is tenant-first"
        );
        assert!(
            def.0.contains("node_id"),
            "watermark index includes node_id"
        );
        assert!(
            def.0.contains("writer_epoch"),
            "watermark index includes writer_epoch"
        );
        assert!(
            def.0.contains("wal_lsn_max"),
            "watermark index includes wal_lsn_max"
        );
    }

    #[tokio::test]
    async fn vala_file_list_rejects_duplicate_stream_range() {
        let (fixture, tenant_id) = setup().await;
        let pool = fixture.superuser_pool().await.expect("superuser pool");

        // Insert one row with a given (node_id, writer_epoch, wal_lsn_min, wal_lsn_max).
        let node_id = Uuid::now_v7();
        let writer_epoch = 1i64;
        let wal_lsn_min = 100i64;
        let wal_lsn_max = 200i64;

        sqlx::query(
            r#"
            INSERT INTO vala.file_list (
                id, data_tenant_id, namespace, table_name, file_path,
                file_size, row_count, min_event_time, max_event_time,
                partition_day, node_id, writer_epoch,
                wal_lsn_min, wal_lsn_max
            ) VALUES (
                $1, $2, 'vala.traces', 'spans', '/fake/path1.parquet',
                1024, 100, now(), now(), current_date,
                $3, $4, $5, $6
            )
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(tenant_id.as_uuid())
        .bind(node_id)
        .bind(writer_epoch)
        .bind(wal_lsn_min)
        .bind(wal_lsn_max)
        .execute(&pool)
        .await
        .expect("first insert");

        // Attempt to insert a second row with the same (node_id, writer_epoch, lsn_min, lsn_max).
        let result = sqlx::query(
            r#"
            INSERT INTO vala.file_list (
                id, data_tenant_id, namespace, table_name, file_path,
                file_size, row_count, min_event_time, max_event_time,
                partition_day, node_id, writer_epoch,
                wal_lsn_min, wal_lsn_max
            ) VALUES (
                $1, $2, 'vala.traces', 'spans', '/fake/path2.parquet',
                2048, 200, now(), now(), current_date,
                $3, $4, $5, $6
            )
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(tenant_id.as_uuid())
        .bind(node_id)
        .bind(writer_epoch)
        .bind(wal_lsn_min)
        .bind(wal_lsn_max)
        .execute(&pool)
        .await;

        assert!(
            result.is_err(),
            "duplicate stream range INSERT must fail on unique constraint"
        );

        if let Err(e) = result {
            let db_err = e
                .as_database_error()
                .expect("database error for duplicate insert");
            assert_eq!(
                db_err.code().as_deref(),
                Some("23505"),
                "unique_violation SQLSTATE"
            );
            assert!(
                db_err
                    .constraint()
                    .is_some_and(|c| c.contains("file_list_stream_range_ordinal_uniq")),
                "constraint name matches unique index"
            );
        }
    }

    #[tokio::test]
    async fn vala_cluster_nodes_migration_applies_cleanly() {
        let (fixture, _tenant) = setup().await;
        let pool = fixture.superuser_pool().await.expect("superuser pool");

        // Assert every cluster_nodes column present with matching type/nullability.
        let columns: Vec<(String, String, String)> = sqlx::query_as(
            r#"
            SELECT column_name, data_type, is_nullable
            FROM information_schema.columns
            WHERE table_schema = 'vala' AND table_name = 'cluster_nodes'
            ORDER BY ordinal_position
            "#,
        )
        .fetch_all(&pool)
        .await
        .expect("column query");

        let expected = [
            ("node_id", "uuid", "NO"),
            ("role", "text", "NO"),
            ("advertise_addr", "text", "NO"),
            ("fencing_token", "bigint", "NO"),
            ("started_at", "timestamp with time zone", "NO"),
            ("heartbeat_at", "timestamp with time zone", "NO"),
            ("meta", "jsonb", "NO"),
            ("data_tenant_id", "uuid", "NO"),
            ("capability_version", "smallint", "NO"),
            ("capabilities", "jsonb", "NO"),
            ("ready", "boolean", "NO"),
        ];

        assert_eq!(
            columns.len(),
            expected.len(),
            "cluster_nodes column count matches schema"
        );

        for ((col_name, col_type, nullable), (exp_name, exp_type, exp_nullable)) in
            columns.iter().zip(expected.iter())
        {
            assert_eq!(col_name, exp_name, "column name");
            assert_eq!(col_type, exp_type, "column type for {}", col_name);
            assert_eq!(nullable, exp_nullable, "nullability for {}", col_name);
        }

        // Verify fencing_token column exists and is non-null (writer_epoch source).
        let fencing_col = columns
            .iter()
            .find(|(name, _, _)| name == "fencing_token")
            .expect("fencing_token column");
        assert_eq!(
            fencing_col.2, "NO",
            "fencing_token is NOT NULL (writer_epoch source)"
        );
    }

    #[tokio::test]
    async fn vala_file_list_rls_blocks_cross_tenant_select() {
        let (fixture, tenant_a) = setup().await;
        let superuser = fixture.superuser_pool().await.expect("superuser pool");

        let tenant_b = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(
                tenant_b,
                &format!("test-{}", tenant_b.as_uuid().simple()),
            )
            .await
            .unwrap();

        // Insert a file_list row as tenant A via superuser pool (BYPASSRLS).
        let file_id = Uuid::now_v7();
        sqlx::query(
            r#"
            INSERT INTO vala.file_list (
                id, data_tenant_id, namespace, table_name, file_path,
                file_size, row_count, min_event_time, max_event_time,
                partition_day, node_id, writer_epoch,
                wal_lsn_min, wal_lsn_max
            ) VALUES (
                $1, $2, 'vala.traces', 'spans', '/fake/pathA.parquet',
                1024, 100, now(), now(), current_date,
                $3, 1, 100, 200
            )
            "#,
        )
        .bind(file_id)
        .bind(tenant_a.as_uuid())
        .bind(Uuid::now_v7())
        .execute(&superuser)
        .await
        .expect("insert for tenant A");

        // Query via TenantConn for tenant B should return zero rows (RLS filter).
        // The wyrd.current_tenant() function used in the RLS policy reads the
        // app.current_tenant parameter that TenantConn::acquire sets.
        let mut conn_b = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant_b)
            .await
            .unwrap();

        let count = count_file_list_rows(&mut conn_b, file_id).await.unwrap();
        conn_b.commit().await.unwrap();

        assert_eq!(
            count, 0,
            "RLS blocks tenant B from seeing tenant A's file_list row"
        );
    }

    #[tokio::test]
    async fn vala_operator_pool_can_insert_and_select_file_list() {
        let (fixture, tenant_id) = setup().await;
        let pool = fixture.superuser_pool().await.expect("superuser pool");

        let file_id = Uuid::now_v7();

        // Insert via OperatorPool (wyrd_platform_admin BYPASSRLS).
        sqlx::query(
            r#"
            INSERT INTO vala.file_list (
                id, data_tenant_id, namespace, table_name, file_path,
                file_size, row_count, min_event_time, max_event_time,
                partition_day, node_id, writer_epoch,
                wal_lsn_min, wal_lsn_max
            ) VALUES (
                $1, $2, 'vala.traces', 'spans', '/fake/operator.parquet',
                1024, 100, now(), now(), current_date,
                $3, 1, 100, 200
            )
            "#,
        )
        .bind(file_id)
        .bind(tenant_id.as_uuid())
        .bind(Uuid::now_v7())
        .execute(&pool)
        .await
        .expect("OperatorPool INSERT");

        // SELECT via OperatorPool should succeed (BYPASSRLS + SELECT grant).
        let row: (Uuid,) = sqlx::query_as("SELECT id FROM vala.file_list WHERE id = $1")
            .bind(file_id)
            .fetch_one(&pool)
            .await
            .expect("OperatorPool SELECT");

        assert_eq!(row.0, file_id, "OperatorPool can SELECT file_list");
    }

    /// Proves the system tenant can UPSERT its isolated membership fence.
    ///
    /// # Panics
    /// Panics when the database fixture, tenant transaction, UPSERT, or commit fails.
    #[tokio::test]
    async fn vala_tenant_conn_can_upsert_cluster_nodes() {
        let (fixture, _tenant) = setup().await;
        let mut conn = fixture
            .vala_postgres()
            .tenant_conn(DataTenantId::SYSTEM_OWNER)
            .await
            .expect("system tenant connection");

        let node_id = Uuid::now_v7();

        sqlx::query(
            r#"
            INSERT INTO vala.cluster_nodes (
                data_tenant_id, node_id, role, advertise_addr, fencing_token,
                started_at, heartbeat_at
            ) VALUES ($1, $2, 'scribe', 'localhost:50051', 1, now(), now())
            ON CONFLICT (data_tenant_id, node_id, role) DO UPDATE
            SET fencing_token = vala.cluster_nodes.fencing_token + 1,
                heartbeat_at = now()
            RETURNING fencing_token
            "#,
        )
        .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
        .bind(node_id)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("TenantConn INSERT/UPSERT");

        // Second UPSERT should increment fencing_token.
        let token: (i64,) = sqlx::query_as(
            r#"
            INSERT INTO vala.cluster_nodes (
                data_tenant_id, node_id, role, advertise_addr, fencing_token,
                started_at, heartbeat_at
            ) VALUES ($1, $2, 'scribe', 'localhost:50051', 1, now(), now())
            ON CONFLICT (data_tenant_id, node_id, role) DO UPDATE
            SET fencing_token = vala.cluster_nodes.fencing_token + 1,
                heartbeat_at = now()
            RETURNING fencing_token
            "#,
        )
        .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
        .bind(node_id)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("second UPSERT");
        conn.commit().await.expect("commit membership UPSERTs");

        assert_eq!(
            token.0, 2,
            "fencing_token increments on conflict (writer_epoch source)"
        );
    }
}
