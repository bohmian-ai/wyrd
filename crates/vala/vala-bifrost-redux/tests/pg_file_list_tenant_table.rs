mod pg_tests {
    //! Postgres-backed contract tests for organization-qualified file-list identity.

    use chrono::{DateTime, NaiveDate, Utc};
    use opendal::services::Memory;
    use sqlx::types::Uuid;
    use std::sync::Arc;
    use vala_bifrost_redux::catalog::tenant_table::TenantTableBindingError;
    use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use vala_bifrost_redux::scribe::ScribeImpl;
    use vala_bifrost_redux::scribe::file_list_writer::{
        FileListInsert, FileListInsertOutcome, insert_and_audit,
    };
    use vala_bifrost_redux::scribe::seal_key::{EventDay, SealKey};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

    fn logical_table() -> TableRef {
        TableRef::new(BifrostNamespace::Traces, "spans")
    }

    async fn setup() -> (PgFixture, DataTenantId, DataTenantId) {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(
                tenant_a,
                &format!("tenant-a-{}", tenant_a.as_uuid().simple()),
            )
            .await
            .expect("tenant A");
        fixture
            .seed_additional_tenant_with_uuid(
                tenant_b,
                &format!("tenant-b-{}", tenant_b.as_uuid().simple()),
            )
            .await
            .expect("tenant B");
        (fixture, tenant_a, tenant_b)
    }

    fn insert_row(
        binding: &TenantTableBinding,
        node_id: Uuid,
        writer_epoch: i64,
        wal_lsn_min: i64,
        wal_lsn_max: i64,
    ) -> FileListInsert<'_> {
        let event_time = DateTime::<Utc>::from_timestamp(0, 0).expect("epoch");
        FileListInsert {
            id: Uuid::now_v7(),
            data_tenant_id: binding.tenant,
            namespace: &binding.logical_namespace,
            table_name: &binding.table_name,
            file_path: &binding.object_prefix,
            file_size: 128,
            row_count: 1,
            min_event_time: event_time,
            max_event_time: event_time,
            partition_day: NaiveDate::from_ymd_opt(1970, 1, 1).expect("date"),
            node_id,
            writer_epoch,
            wal_lsn_min,
            wal_lsn_max,
        }
    }

    fn audit_event() -> AuditEvent {
        AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: None,
            operation: "test.file_list_identity".to_owned(),
            resource: "vala.traces.spans".to_owned(),
            card_ref: None,
            principal_id: PrincipalId::new(Uuid::now_v7()),
            principal_kind: PrincipalKindTag::User,
            auth_method: AuthMethod::Internal,
            permission: "test:file_list".to_owned(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: "one file".to_owned(),
            detail: None,
        }
    }

    async fn index_shape(pool: &sqlx::PgPool, index_name: &str) -> (Vec<String>, Option<String>) {
        sqlx::query_as(
            r"
            SELECT ARRAY(
                       SELECT a.attname
                         FROM pg_index i2
                         CROSS JOIN LATERAL unnest(i2.indkey) WITH ORDINALITY AS key(attnum, ordinality)
                         JOIN pg_attribute a
                           ON a.attrelid = i2.indrelid
                          AND a.attnum = key.attnum
                        WHERE i2.indexrelid = i.indexrelid
                        ORDER BY key.ordinality
                   ),
                   pg_get_expr(i.indpred, i.indrelid)
              FROM pg_class c
              JOIN pg_index i ON i.indexrelid = c.oid
              JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = 'vala' AND c.relname = $1
            ",
        )
        .bind(index_name)
        .fetch_one(pool)
        .await
        .expect("index shape")
    }

    #[tokio::test]
    async fn test_migration_matches_tenant_table_contract() {
        let (fixture, _, _) = setup().await;
        let pool = fixture.superuser_pool().await.expect("superuser pool");

        let columns: Vec<String> = sqlx::query_scalar(
            "SELECT column_name FROM information_schema.columns WHERE table_schema = 'vala' AND table_name = 'file_list'",
        )
        .fetch_all(&pool)
        .await
        .expect("file_list columns");
        assert!(!columns.iter().any(|column| column == "tenant_bucket"));
        assert!(!columns.iter().any(|column| column == "scope"));
        let tenant_nullable: String = sqlx::query_scalar(
            "SELECT is_nullable FROM information_schema.columns WHERE table_schema = 'vala' AND table_name = 'file_list' AND column_name = 'data_tenant_id'",
        )
        .fetch_one(&pool)
        .await
        .expect("data_tenant_id nullability");
        assert_eq!(tenant_nullable, "NO");

        let (group_columns, group_predicate) = index_shape(&pool, "file_list_group_idx").await;
        assert_eq!(
            group_columns,
            ["data_tenant_id", "namespace", "table_name", "partition_day"]
        );
        assert!(
            group_predicate
                .as_deref()
                .is_some_and(|predicate| predicate.to_lowercase().contains("not compacted"))
        );

        let (watermark_columns, watermark_predicate) =
            index_shape(&pool, "file_list_live_tail_watermark_idx").await;
        assert_eq!(
            watermark_columns,
            [
                "data_tenant_id",
                "namespace",
                "table_name",
                "node_id",
                "writer_epoch",
                "wal_lsn_max"
            ]
        );
        assert!(watermark_predicate.is_none());

        for index_name in ["file_list_tenant_idx", "file_list_stream_range_uniq"] {
            let exists: (bool,) = sqlx::query_as(
                "SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE schemaname = 'vala' AND indexname = $1)",
            )
            .bind(index_name)
            .fetch_one(&pool)
            .await
            .expect("index existence");
            assert!(exists.0, "preserved index {index_name}");
        }
    }

    #[tokio::test]
    async fn test_two_organizations_same_logical_table_are_separate() {
        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        let table = logical_table();
        let binding_a = TenantTableBinding::resolve((tenant_a, table.clone())).expect("binding A");
        let binding_b = TenantTableBinding::resolve((tenant_b, table)).expect("binding B");

        assert_eq!(binding_a.table_name, binding_b.table_name);
        assert_eq!(binding_a.logical_namespace, binding_b.logical_namespace);
        assert_ne!(binding_a.iceberg_namespace, binding_b.iceberg_namespace);
        assert_ne!(binding_a.object_prefix, binding_b.object_prefix);
    }

    #[tokio::test]
    async fn test_file_list_row_uses_exact_tenant_table_identity() {
        let (fixture, tenant_a, _) = setup().await;
        let binding = TenantTableBinding::resolve((tenant_a, logical_table())).expect("binding");
        let node_id = Uuid::now_v7();
        let row = insert_row(&binding, node_id, 1, 100, 100);
        let event = audit_event();
        let mut conn = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant_a)
            .await
            .expect("tenant connection");
        let outcome = insert_and_audit(&mut conn, &row, &[event])
            .await
            .expect("file_list insert");
        conn.commit().await.expect("commit");

        let superuser = fixture.superuser_pool().await.expect("superuser pool");
        let stored: (Uuid, Uuid, String, String, String) = sqlx::query_as(
            "SELECT id, data_tenant_id, namespace, table_name, file_path FROM vala.file_list WHERE id = $1",
        )
        .bind(outcome.id)
        .fetch_one(&superuser)
        .await
        .expect("stored row");

        assert_eq!(stored.0, row.id);
        assert_eq!(stored.1, tenant_a.as_uuid());
        assert_eq!(stored.2, binding.logical_namespace);
        assert_eq!(stored.3, binding.table_name);
        assert!(stored.4.starts_with(&binding.object_prefix));
    }

    #[tokio::test]
    async fn test_foreign_tenant_write_fails_before_put() {
        let (fixture, tenant_a, tenant_b) = setup().await;
        let operator = Arc::new(
            opendal::Operator::new(Memory::default())
                .expect("memory backend")
                .finish(),
        );
        let temp_dir = tempfile::tempdir().expect("temp WAL dir");
        let node_id = Uuid::now_v7();
        let wal = Arc::new(
            vala_bifrost_redux::scribe::wal::WalWriter::new(
                temp_dir.path(),
                *node_id.as_bytes(),
                1,
                tenant_a,
                None,
            )
            .expect("WAL writer"),
        );
        std::mem::forget(temp_dir);
        let scribe = ScribeImpl::new_with_deps(operator.clone(), wal, node_id.to_string(), 1);
        let seal_key = SealKey::new(
            tenant_a,
            logical_table(),
            EventDay::new(NaiveDate::from_ymd_opt(1970, 1, 1).expect("date")),
        );
        let mut conn = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant_b)
            .await
            .expect("tenant connection");

        let error = scribe
            .seal_one(&seal_key, &mut conn)
            .await
            .expect_err("foreign tenant seal must fail");
        assert!(error.to_string().contains("tenant"));
        assert!(operator.list("").await.expect("object list").is_empty());
    }

    #[tokio::test]
    async fn test_replay_conflict_cannot_cross_tenant() {
        let (fixture, tenant_a, tenant_b) = setup().await;
        let table = logical_table();
        let binding_a = TenantTableBinding::resolve((tenant_a, table.clone())).expect("binding A");
        let binding_b = TenantTableBinding::resolve((tenant_b, table)).expect("binding B");
        let node_id = Uuid::now_v7();

        let first = insert_row(&binding_a, node_id, 1, 400, 450);
        let mut conn_a = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant_a)
            .await
            .expect("tenant A connection");
        let first_outcome = insert_and_audit(&mut conn_a, &first, &[audit_event()])
            .await
            .expect("first insert");
        conn_a.commit().await.expect("first commit");

        let replay = insert_row(&binding_a, node_id, 1, 400, 450);
        let mut conn_a_replay = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant_a)
            .await
            .expect("tenant A replay connection");
        let replay_outcome = insert_and_audit(&mut conn_a_replay, &replay, &[audit_event()])
            .await
            .expect("validated replay");
        conn_a_replay.commit().await.expect("replay commit");
        assert_eq!(
            replay_outcome,
            FileListInsertOutcome {
                id: first_outcome.id,
                commit_key: first.commit_key(),
                replayed: true,
            }
        );

        let foreign = insert_row(&binding_b, node_id, 1, 400, 450);
        let mut conn_b = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant_b)
            .await
            .expect("tenant B connection");
        let error = insert_and_audit(&mut conn_b, &foreign, &[])
            .await
            .expect_err("cross-tenant replay must fail closed");
        assert!(error.to_string().contains("RLS-visible"));

        let superuser = fixture.superuser_pool().await.expect("superuser pool");
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation = $2",
        )
        .bind(tenant_a.as_uuid())
        .bind("test.file_list_identity")
        .fetch_one(&superuser)
        .await
        .expect("audit count");
        assert_eq!(
            audit_count, 1,
            "replay must not append a duplicate audit row"
        );
    }

    #[test]
    fn tenant_table_binding_error_remains_typed() {
        let tenant = DataTenantId::new_v7();
        let error = TenantTableBinding::resolve((
            tenant,
            TableRef::new(BifrostNamespace::Traces, "bad/name"),
        ))
        .expect_err("unsafe local table name");
        assert!(matches!(
            error,
            TenantTableBindingError::InvalidTableName { .. }
        ));
    }
}
