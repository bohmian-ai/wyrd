mod pg_tests {
    //! SQL integration tests for the transactional audit outbox.
    //!
    //! Covers the per-tenant gapless hash chain, append-only enforcement, tenant
    //! isolation, and tenant-scoped reads.
    //! Run via `mise run test:sql`.

    mod audit_staging {
        use sqlx::PgPool;
        use sqlx::types::Uuid;
        use wyrd_dev_fixtures::pg::PgFixture;
        use wyrd_spec::DataTenantId;
        use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
        use wyrd_spec::request_id::RequestId;
        use wyrd_spec::vala::api::{AuditEvent, AuditOutcome};

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
                permission: "bifrost.write".to_string(),
                outcome: AuditOutcome::Allowed,
                detail: None,
            }
        }

        async fn append(pool: &PgPool, tenant: DataTenantId, operation: &str) -> i64 {
            let mut conn = vala_sql::TenantConn::acquire(pool, tenant).await.unwrap();
            let seq = vala_sql::queries::audit_staging::append_audit(&mut conn, &event(operation))
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
                "SELECT seq, prev_hash, entry_hash FROM vala.audit_staging
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

        /// A written audit row can be retired, but never rewritten.
        #[tokio::test]
        async fn immutability_trigger_rejects_content_update() {
            let (fixture, superuser, tenant) = setup().await;
            append(fixture.app_pool(), tenant, "op.a").await;

            // Probe via the BYPASSRLS migrator pool so the statement reaches the row
            // and the immutability trigger — not RLS — is what rejects it.
            let tampered = sqlx::query(
                "UPDATE vala.audit_staging SET operation = 'tampered'
              WHERE data_tenant_id = $1 AND seq = 1",
            )
            .bind(tenant.as_uuid())
            .execute(&superuser)
            .await;
            assert!(tampered.is_err(), "content UPDATE must be rejected");
        }

        /// The publisher reads the oldest bounded run and settles exactly it.
        ///
        /// Settlement is the only removal path, and repeating it after an
        /// uncertain outcome removes nothing more — the property audit
        /// recovery depends on.
        #[tokio::test]
        async fn publication_batch_is_bounded_and_settlement_is_idempotent() {
            let (fixture, _superuser, tenant) = setup().await;
            append(fixture.app_pool(), tenant, "op.a").await;
            append(fixture.app_pool(), tenant, "op.b").await;
            append(fixture.app_pool(), tenant, "op.c").await;

            let mut conn = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant)
                .await
                .unwrap();
            let range = vala_sql::queries::audit_staging::freeze_publication_range(&mut conn, 2)
                .await
                .unwrap()
                .expect("three staged rows owe a range");
            assert_eq!(
                (range.seq_lo, range.seq_hi),
                (1, 2),
                "the frozen range is the oldest contiguous run, bounded by the limit"
            );
            let batch = vala_sql::queries::audit_staging::list_publication_range(&mut conn, range)
                .await
                .unwrap();
            assert_eq!(
                batch.iter().map(|row| row.seq).collect::<Vec<_>>(),
                vec![1, 2],
                "reading the frozen range returns exactly it"
            );

            let drained = vala_sql::queries::audit_staging::settle_publication(&mut conn, 2)
                .await
                .unwrap();
            assert_eq!(drained, 2);
            let replayed = vala_sql::queries::audit_staging::settle_publication(&mut conn, 2)
                .await
                .unwrap();
            assert_eq!(replayed, 0, "a replayed settlement removes nothing more");

            let remaining = vala_sql::queries::audit_staging::list_publication_batch(&mut conn, 10)
                .await
                .unwrap();
            conn.commit().await.unwrap();
            assert_eq!(
                remaining.iter().map(|row| row.seq).collect::<Vec<_>>(),
                vec![3],
                "unpublished events survive the settlement of the published prefix"
            );
        }

        /// A growing tail and a competing publisher cannot move a frozen range.
        ///
        /// This is the whole point of persisting one in-flight upper bound: two
        /// publishers — or one publisher restarted mid-flight — must project the
        /// same rows, otherwise they derive two different batch identities for
        /// overlapping content and Scribe's batch fence cannot absorb the
        /// second. The scenario freezes `1..=3`, appends row `4` above the
        /// bound, re-freezes from a second connection, and requires the identical
        /// range; only after settlement may a publisher see `4`.
        ///
        /// It also pins the stale-completion guard: a settlement carrying the
        /// already-settled bound must neither advance the watermark nor clear
        /// the newer bound that replaced it.
        #[tokio::test]
        async fn frozen_range_survives_tail_growth_competition_and_stale_settlement() {
            let (fixture, superuser, tenant) = setup().await;
            for operation in ["op.a", "op.b", "op.c"] {
                append(fixture.app_pool(), tenant, operation).await;
            }

            let mut first = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant)
                .await
                .unwrap();
            let frozen =
                vala_sql::queries::audit_staging::freeze_publication_range(&mut first, 512)
                    .await
                    .unwrap()
                    .expect("three staged rows owe a range");
            first.commit().await.unwrap();
            assert_eq!((frozen.seq_lo, frozen.seq_hi), (1, 3));

            // The tail grows while the batch is in flight.
            append(fixture.app_pool(), tenant, "op.d").await;

            let mut competing = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant)
                .await
                .unwrap();
            let observed =
                vala_sql::queries::audit_staging::freeze_publication_range(&mut competing, 512)
                    .await
                    .unwrap()
                    .expect("the in-flight bound is still owed");
            assert_eq!(
                observed, frozen,
                "a competing publisher must reuse the frozen range, not widen it to 1..=4"
            );
            let rows =
                vala_sql::queries::audit_staging::list_publication_range(&mut competing, observed)
                    .await
                    .unwrap();
            assert_eq!(
                rows.iter().map(|row| row.seq).collect::<Vec<_>>(),
                vec![1, 2, 3],
                "row 4 arrived above the frozen bound and stays staged"
            );
            let retired = vala_sql::queries::audit_staging::settle_publication(
                &mut competing,
                observed.seq_hi,
            )
            .await
            .unwrap();
            competing.commit().await.unwrap();
            assert_eq!(retired, 3);

            let mut next = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant)
                .await
                .unwrap();
            let second = vala_sql::queries::audit_staging::freeze_publication_range(&mut next, 512)
                .await
                .unwrap()
                .expect("row 4 is owed once the first batch settled");
            assert_eq!(
                (second.seq_lo, second.seq_hi),
                (4, 4),
                "only a settled batch releases the tail into the next range"
            );
            // The slow first publisher finally reports the range already settled.
            let stale =
                vala_sql::queries::audit_staging::settle_publication(&mut next, frozen.seq_hi)
                    .await
                    .unwrap();
            next.commit().await.unwrap();
            assert_eq!(stale, 0, "a stale settlement removes nothing");

            let (published, in_flight): (i64, Option<i64>) = sqlx::query_as(
                "SELECT published_seq, publishing_seq_hi FROM vala.audit_chain_head
                  WHERE data_tenant_id = $1",
            )
            .bind(tenant.as_uuid())
            .fetch_one(&superuser)
            .await
            .unwrap();
            assert_eq!(
                published, 3,
                "a stale settlement must not advance past the newer in-flight bound"
            );
            assert_eq!(
                in_flight,
                Some(4),
                "a stale settlement must not clear the newer in-flight bound"
            );
        }

        /// An idle tenant owes nothing and its staging table drains to zero.
        #[tokio::test]
        async fn settled_tenant_drains_to_zero_and_owes_nothing() {
            let (fixture, superuser, tenant) = setup().await;
            append(fixture.app_pool(), tenant, "op.a").await;

            let mut conn = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant)
                .await
                .unwrap();
            let range = vala_sql::queries::audit_staging::freeze_publication_range(&mut conn, 512)
                .await
                .unwrap()
                .expect("one staged row is owed");
            assert_eq!(
                vala_sql::queries::audit_staging::settle_publication(&mut conn, range.seq_hi)
                    .await
                    .unwrap(),
                1
            );
            let idle = vala_sql::queries::audit_staging::freeze_publication_range(&mut conn, 512)
                .await
                .unwrap();
            conn.commit().await.unwrap();
            assert_eq!(idle, None, "a fully settled tenant owes no range");

            let staged: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM vala.audit_staging WHERE data_tenant_id = $1",
            )
            .bind(tenant.as_uuid())
            .fetch_one(&superuser)
            .await
            .unwrap();
            assert_eq!(staged, 0, "no grace tail survives an idle tenant");
        }

        /// Settlement never reaches another tenant's rows.
        #[tokio::test]
        async fn settlement_is_tenant_scoped() {
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
            append(fixture.app_pool(), tenant_b, "b.1").await;

            let mut conn = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant_a)
                .await
                .unwrap();
            let drained = vala_sql::queries::audit_staging::settle_publication(&mut conn, 1)
                .await
                .unwrap();
            conn.commit().await.unwrap();
            assert_eq!(drained, 1);

            let mut other = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant_b)
                .await
                .unwrap();
            let surviving =
                vala_sql::queries::audit_staging::list_publication_batch(&mut other, 10)
                    .await
                    .unwrap();
            other.commit().await.unwrap();
            assert_eq!(
                surviving.len(),
                1,
                "one tenant's settlement leaves another tenant's chain intact"
            );
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
            let first = vala_sql::queries::audit_staging::list_audit_events_for_resource(
                &mut conn, "ns.tbl", 0, 1,
            )
            .await
            .unwrap();
            assert_eq!(first.len(), 1);
            assert_eq!(first[0].seq, 1);

            let second = vala_sql::queries::audit_staging::list_audit_events_for_resource(
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
            assert_eq!(
                vala_sql::queries::scribe_batch_commits::record(&mut conn, &canonical)
                    .await
                    .expect("canonical fence"),
                vala_sql::queries::scribe_batch_commits::ScribeBatchCommitResolution::Committed,
                "a first observation of a batch identity commits it"
            );
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
            // A client re-sending the same rows under a fresh request lands on
            // new WAL coordinates and a new correlation id. That is the same
            // batch, so it must be acknowledged as already committed rather
            let resend = vala_sql::queries::scribe_batch_commits::ScribeBatchCommit {
                request_id: Uuid::now_v7(),
                ..retry.clone()
            };
            assert_eq!(
                vala_sql::queries::scribe_batch_commits::record(&mut conn, &resend)
                    .await
                    .expect("re-sent identical batch resolves"),
                vala_sql::queries::scribe_batch_commits::ScribeBatchCommitResolution::AlreadyCommitted,
                "the same rows from a later attempt are already committed, not contradictory"
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
                "SELECT COUNT(*) FROM vala.audit_staging WHERE data_tenant_id = $1",
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
            assert_eq!(
                audit_rows, 0,
                "a batch commit evaluates no permission, so it appends no audit"
            );
            assert_eq!(publication_rows, 0, "replay resolution must not publish");
        }
    }
}
