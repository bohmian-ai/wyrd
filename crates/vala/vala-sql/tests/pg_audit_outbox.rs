mod pg_tests {
    //! SQL integration tests for the transactional audit outbox.
    //!
    //! Covers the per-tenant gapless hash chain, append-only enforcement, tenant
    //! isolation, and tenant-scoped reads.
    //! Run via `mise run test:sql`.

    mod audit_outbox {
        use sqlx::PgPool;
        use sqlx::types::Uuid;
        use wyrd_dev_fixtures::pg::PgFixture;
        use wyrd_spec::DataTenantId;
        use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
        use wyrd_spec::request_id::RequestId;
        use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

        const ZERO_HASH: [u8; 32] = [0u8; 32];

        async fn setup() -> (PgFixture, PgPool, DataTenantId) {
            let fixture = PgFixture::start().await.expect("fixture");
            let superuser = fixture.superuser_pool().await.expect("superuser pool");
            let tenant = DataTenantId::new_v7();
            fixture
                .seed_additional_tenant_with_uuid(
                    tenant,
                    &format!("test-{}", tenant.as_uuid().simple()),
                )
                .await
                .unwrap();
            (fixture, superuser, tenant)
        }

        fn event(operation: &str) -> AuditEvent {
            AuditEvent {
                request_id: RequestId::parse(&Uuid::now_v7().to_string()).unwrap(),
                trace_id: None,
                operation: operation.to_string(),
                resource: "ns.tbl".to_string(),
                card_ref: None,
                principal_id: PrincipalId::new(Uuid::now_v7()),
                principal_kind: PrincipalKindTag::User,
                auth_method: AuthMethod::Internal,
                permission: "bifrost.write".to_string(),
                decision: AuditDecision::Allow,
                result: AuditResult::Success,
                payload_summary: "redacted".to_string(),
                detail: None,
            }
        }

        async fn append(pool: &PgPool, tenant: DataTenantId, operation: &str) -> i64 {
            let mut conn = vala_sql::TenantConn::acquire(pool, tenant).await.unwrap();
            let seq = vala_sql::queries::audit_outbox::append_audit(&mut conn, &event(operation))
                .await
                .unwrap();
            conn.commit().await.unwrap();
            seq
        }

        #[tokio::test]
        async fn gapless_seq_and_hash_chain() {
            let (fixture, superuser, tenant) = setup().await;

            assert_eq!(append(fixture.app_pool(), tenant, "op.a").await, 1);
            assert_eq!(append(fixture.app_pool(), tenant, "op.b").await, 2);
            assert_eq!(append(fixture.app_pool(), tenant, "op.c").await, 3);

            let rows: Vec<(i64, Vec<u8>, Vec<u8>)> = sqlx::query_as(
                "SELECT seq, prev_hash, entry_hash FROM vala.audit_outbox
              WHERE data_tenant_id = $1 ORDER BY seq",
            )
            .bind(tenant.as_uuid())
            .fetch_all(&superuser)
            .await
            .unwrap();

            assert_eq!(rows.len(), 3);
            assert_eq!(rows[0].0, 1);
            assert_eq!(rows[0].1, ZERO_HASH, "row 1 prev_hash is 32 zero bytes");
            assert_eq!(
                rows[1].1, rows[0].2,
                "row 2 prev_hash links row 1 entry_hash"
            );
            assert_eq!(
                rows[2].1, rows[1].2,
                "row 3 prev_hash links row 2 entry_hash"
            );

            let (last_seq, head_hash): (i64, Vec<u8>) = sqlx::query_as(
                "SELECT last_seq, head_hash FROM vala.audit_chain_head WHERE data_tenant_id = $1",
            )
            .bind(tenant.as_uuid())
            .fetch_one(&superuser)
            .await
            .unwrap();
            assert_eq!(last_seq, 3);
            assert_eq!(
                head_hash, rows[2].2,
                "head_hash tracks the latest entry_hash"
            );
        }

        #[tokio::test]
        async fn append_only_trigger_rejects_delete_and_content_update() {
            let (fixture, superuser, tenant) = setup().await;
            append(fixture.app_pool(), tenant, "op.a").await;

            // Probe via the BYPASSRLS migrator pool so the statement reaches the row
            // and the append-only trigger — not RLS — is what rejects it.
            let deleted =
                sqlx::query("DELETE FROM vala.audit_outbox WHERE data_tenant_id = $1 AND seq = 1")
                    .bind(tenant.as_uuid())
                    .execute(&superuser)
                    .await;
            assert!(
                deleted.is_err(),
                "DELETE must be rejected by the append-only trigger"
            );

            let tampered = sqlx::query(
                "UPDATE vala.audit_outbox SET operation = 'tampered'
              WHERE data_tenant_id = $1 AND seq = 1",
            )
            .bind(tenant.as_uuid())
            .execute(&superuser)
            .await;
            assert!(tampered.is_err(), "content UPDATE must be rejected");
        }

        #[tokio::test]
        async fn resource_reader_is_tenant_scoped_and_paginates() {
            let (fixture, _superuser, tenant_a) = setup().await;
            let tenant_b = DataTenantId::new_v7();
            fixture
                .seed_additional_tenant_with_uuid(
                    tenant_b,
                    &format!("test-{}", tenant_b.as_uuid().simple()),
                )
                .await
                .unwrap();

            append(fixture.app_pool(), tenant_a, "a.1").await;
            append(fixture.app_pool(), tenant_a, "a.2").await;
            append(fixture.app_pool(), tenant_b, "b.1").await;

            let mut conn = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant_a)
                .await
                .unwrap();
            let first = vala_sql::queries::audit_outbox::list_audit_events_for_resource(
                &mut conn, "ns.tbl", 0, 1,
            )
            .await
            .unwrap();
            assert_eq!(first.len(), 1);
            assert_eq!(first[0].seq, 1);

            let second = vala_sql::queries::audit_outbox::list_audit_events_for_resource(
                &mut conn,
                "ns.tbl",
                first[0].seq,
                10,
            )
            .await
            .unwrap();
            conn.commit().await.unwrap();
            assert_eq!(second.len(), 1);
            assert_eq!(second[0].seq, 2);
            assert_eq!(second[0].resource, "ns.tbl");
        }

        /// Two-epoch replay suppresses an exact retry and fails a contradiction without mutation.
        #[tokio::test]
        async fn replay_two_epoch_batch_fence_suppresses_exact_and_rejects_contradiction() {
            let (fixture, superuser, tenant) = setup().await;
            let batch_id = Uuid::now_v7();
            let audit = event("bifrost.append");
            let request_id = Uuid::parse_str(audit.request_id.as_str()).expect("request UUID");
            let canonical = vala_sql::queries::scribe_batch_commits::ScribeBatchCommit {
                tenant,
                logical_table_fqn: "bifrost.replay_two_epoch".to_owned(),
                batch_id,
                slice_set_digest: [7; 32],
                slice_count: 1,
                wal_node_id: Uuid::now_v7(),
                wal_writer_epoch: 1,
                wal_shard_id: 3,
                wal_segment_sequence: 4,
                wal_lsn_min: 10,
                wal_lsn_max: 11,
                request_id,
            };
            let mut conn = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant)
                .await
                .expect("tenant connection");
            vala_sql::queries::scribe_batch_commits::record(&mut conn, &canonical, &audit)
                .await
                .expect("canonical fence");
            conn.commit().await.expect("commit canonical fence");

            let retry = vala_sql::queries::scribe_batch_commits::ScribeBatchCommit {
                wal_writer_epoch: 2,
                wal_segment_sequence: 0,
                wal_lsn_min: 0,
                wal_lsn_max: 1,
                ..canonical.clone()
            };
            let mut conn = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant)
                .await
                .expect("retry connection");
            assert_eq!(
                vala_sql::queries::scribe_batch_commits::resolve_replay(&mut conn, &retry)
                    .await
                    .expect("exact retry resolution"),
                vala_sql::queries::scribe_batch_commits::ScribeBatchReplayResolution::Suppress
            );
            let contradiction = vala_sql::queries::scribe_batch_commits::ScribeBatchCommit {
                slice_set_digest: [8; 32],
                ..retry
            };
            assert!(
                vala_sql::queries::scribe_batch_commits::resolve_replay(&mut conn, &contradiction)
                    .await
                    .is_err(),
                "contradictory replay identity must fail closed"
            );
            conn.commit().await.expect("commit read-only replay checks");

            let audit_rows: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM vala.audit_outbox WHERE data_tenant_id = $1",
            )
            .bind(tenant.as_uuid())
            .fetch_one(&superuser)
            .await
            .expect("audit count");
            let publication_rows: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM vala.file_list WHERE data_tenant_id = $1")
                    .bind(tenant.as_uuid())
                    .fetch_one(&superuser)
                    .await
                    .expect("publication count");
            assert_eq!(audit_rows, 1, "replay resolution must not duplicate audit");
            assert_eq!(publication_rows, 0, "replay resolution must not publish");
        }
    }
}
