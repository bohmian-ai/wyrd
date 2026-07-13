mod pg_tests {
    //! Slice-12 audit sealing: outbox → warehouse → signed checkpoint → verify.
    //!
    //! Proves the sealing pipeline end-to-end against real Postgres + Iceberg:
    //! ship hash-chained audit rows into `vala.system.audit_log`, seal the shipped
    //! range into a signed `vala.audit_seal_checkpoints` row, then re-verify by
    //! recomputing every entry hash from the warehouse **content columns**. A
    //! clean history verifies; shipped rows beyond the sealed range surface as a
    //! gap.
    //!
    //! Run via `mise run test:bifrost`.

    mod audit_seal {
        use std::sync::Arc;

        use arrow::array::{ArrayRef, Int64Array, RecordBatch, StringArray};
        use arrow::datatypes::{DataType, Field, Schema};
        use sqlx::PgPool;
        use tempfile::TempDir;
        use uuid::Uuid;
        use vala_bifrost::TableScope;
        use vala_bifrost::catalog::WyrdCatalog;
        use vala_bifrost::catalog::namespaces::BifrostNamespace;
        use vala_bifrost::relay::{AUDIT_LOG_TABLE, AuditRelay};
        use vala_bifrost::serving::audit_seal::verify::{VerifyOutcome, verify_checkpoint_walk};
        use vala_bifrost::serving::audit_seal::worker::{compute_range_hash, seal_shipped_range};
        use vala_bifrost::writer::BifrostWriteContext;
        use vala_sql::TenantConn;
        use vala_sql::queries::audit_outbox::{
            AuditEntryHashInput, ShippedOutboxRef, entry_hash_from_cols,
        };
        use vala_sql::queries::audit_seal::upsert_checkpoint;
        use wyrd_auth_issue::AuditSealKey;
        use wyrd_dev_fixtures::pg::PgFixture;
        use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
        use wyrd_spec::ids::DataTenantId;
        use wyrd_spec::request_id::RequestId;
        use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};
        use wyrd_storage::settings::BackendConfig;

        struct Fixture {
            _tmp: TempDir,
            catalog: Arc<WyrdCatalog>,
            pool: Arc<PgPool>,
            db: PgFixture,
        }

        async fn setup() -> Fixture {
            let fixture = PgFixture::start().await.expect("fixture");
            let pool = Arc::new(fixture.app_pool().clone());

            let tmp = tempfile::tempdir().unwrap();
            let backend = BackendConfig::Local {
                root: tmp.path().to_path_buf(),
            };
            let catalog_uri = fixture.catalog_uri();
            let catalog = WyrdCatalog::new(&catalog_uri, &backend, pool.clone(), None)
                .await
                .unwrap();
            let catalog = Arc::new(catalog);

            AuditRelay::new(catalog.clone(), pool.clone())
                .ensure_audit_log_table()
                .await
                .unwrap();

            Fixture {
                _tmp: tmp,
                catalog,
                pool,
                db: fixture,
            }
        }

        fn relay(fx: &Fixture) -> AuditRelay {
            AuditRelay::new(fx.catalog.clone(), fx.pool.clone())
        }

        /// A `User`-principal audit event; `operation` distinguishes rows.
        fn audit_event(operation: &str) -> AuditEvent {
            AuditEvent {
                request_id: RequestId::parse(&Uuid::now_v7().to_string()).unwrap(),
                trace_id: None,
                operation: operation.to_string(),
                resource: "vala.bifrost.thing".to_string(),
                card_ref: None,
                principal_id: PrincipalId::new(Uuid::now_v7()),
                principal_kind: PrincipalKindTag::User,
                auth_method: AuthMethod::Internal,
                permission: "bifrost.record_write".to_string(),
                decision: AuditDecision::Allow,
                result: AuditResult::Success,
                payload_summary: operation.to_string(),
            }
        }

        /// Register `tenant` and append `n` hash-chained outbox rows.
        async fn seed_audit(fx: &Fixture, tenant: DataTenantId, n: usize) {
            fx.db
                .seed_additional_tenant_with_uuid(
                    tenant,
                    &format!("seal-{}", tenant.as_uuid().simple()),
                )
                .await
                .unwrap();
            append_audit(fx, tenant, n).await;
        }

        /// Append `n` more hash-chained outbox rows under an already-seeded tenant.
        async fn append_audit(fx: &Fixture, tenant: DataTenantId, n: usize) {
            let mut conn = TenantConn::acquire(&fx.pool, tenant).await.unwrap();
            for i in 0..n {
                vala_sql::queries::audit_outbox::append_audit(
                    &mut conn,
                    &audit_event(&format!("op-{i}")),
                )
                .await
                .unwrap();
            }
            conn.commit().await.unwrap();
        }

        /// Seal + re-verify a clean shipped history: every warehouse entry hash is
        /// recomputed from the content columns and matches the signed checkpoint.
        #[tokio::test]
        async fn seal_then_verify_is_clean() {
            let fx = setup().await;
            let key = AuditSealKey::generate().expect("key generates");
            let tenant = DataTenantId::new_v7();

            seed_audit(&fx, tenant, 3).await;
            let shipped = relay(&fx).tick(100).await.unwrap();
            assert_eq!(shipped, 3, "all seeded rows ship");

            let outcome = seal_shipped_range(&fx.pool, &key, tenant).await.unwrap();
            assert!(!outcome.skipped, "shipped rows must produce a checkpoint");
            assert_eq!(outcome.rows_sealed, 3);
            assert_eq!(outcome.seq_lo, Some(1));
            assert_eq!(outcome.seq_hi, Some(3));

            let verified = verify_checkpoint_walk(&fx.pool, &fx.catalog, &key, tenant)
                .await
                .unwrap();
            assert_eq!(
                verified,
                VerifyOutcome::Clean,
                "recompute-from-content over honest warehouse data must verify clean"
            );
        }

        /// Shipping rows past the sealed range leaves an unsealed tail — verify
        /// must report a gap rather than a false clean.
        #[tokio::test]
        async fn verify_reports_gap_when_shipped_beyond_sealed() {
            let fx = setup().await;
            let key = AuditSealKey::generate().expect("key generates");
            let tenant = DataTenantId::new_v7();

            seed_audit(&fx, tenant, 3).await;
            relay(&fx).tick(100).await.unwrap();
            seal_shipped_range(&fx.pool, &key, tenant).await.unwrap();

            // Two more rows ship but are never sealed.
            append_audit(&fx, tenant, 2).await;
            let shipped = relay(&fx).tick(100).await.unwrap();
            assert_eq!(shipped, 2, "the tail rows ship");

            let verified = verify_checkpoint_walk(&fx.pool, &fx.catalog, &key, tenant)
                .await
                .unwrap();
            assert_eq!(
                verified,
                VerifyOutcome::Gap,
                "shipped rows beyond the sealed range must surface as a gap"
            );
        }

        /// Tamper detection is load-bearing on *content*, not the stored hash.
        ///
        /// This writes a warehouse `audit_log` row whose content column
        /// (`decision`) is tampered while its stored `entry_hash` column stays
        /// the honest hash of the *original* content, then seals a checkpoint
        /// over that honest hash. The verifier ignores the stored `entry_hash`
        /// and recomputes each hash from the content columns, so it must catch
        /// the divergence and return `Tampered`. A verifier that regressed to
        /// trusting the stored `entry_hash` (the prior fraud) would report
        /// `Clean` here — this test would then fail, which is the point.
        #[tokio::test]
        async fn verify_reports_tampered_when_content_diverges_from_stored_hash() {
            let fx = setup().await;
            let key = AuditSealKey::generate().expect("key generates");
            let tenant = DataTenantId::new_v7();
            fx.db
                .seed_additional_tenant_with_uuid(
                    tenant,
                    &format!("seal-{}", tenant.as_uuid().simple()),
                )
                .await
                .unwrap();

            // Original (honest) content for seq 1. The stored entry_hash column
            // will carry the hash of THIS content; only `decision` is tampered
            // in the warehouse row below.
            let seq: i64 = 1;
            let prev_hash = [0u8; 32];
            let request_id = Uuid::now_v7().to_string();
            let operation = "op-0";
            let resource = "vala.bifrost.thing";
            let principal_id = Uuid::now_v7();
            let principal_kind = "user";
            let auth_method = "internal";
            let permission = "bifrost.record_write";
            let honest_decision = "allow";
            let tampered_decision = "deny";
            let result = "success";
            let payload_summary = "op-0";

            let honest_hash = entry_hash_from_cols(AuditEntryHashInput {
                prev_hash: &prev_hash,
                seq,
                request_id: &request_id,
                trace_id: None,
                operation,
                resource,
                card_ref: None,
                principal_id_bytes: principal_id.as_bytes(),
                principal_kind,
                auth_method,
                permission,
                decision: honest_decision,
                result,
                payload_summary,
            });

            // Warehouse row: honest stored entry_hash, tampered `decision`.
            let batch = audit_log_row_batch(&AuditRowCols {
                seq,
                entry_hash_hex: &hex::encode(honest_hash),
                prev_hash_hex: &hex::encode(prev_hash),
                request_id: &request_id,
                operation,
                resource,
                principal_id: &principal_id.to_string(),
                principal_kind,
                auth_method,
                permission,
                decision: tampered_decision,
                result,
                payload_summary,
            });

            let writer = fx
                .catalog
                .writer(
                    BifrostNamespace::System,
                    AUDIT_LOG_TABLE,
                    TableScope::SystemShared,
                    tenant,
                )
                .await
                .unwrap();
            let ctx = BifrostWriteContext {
                batch_id: *Uuid::now_v7().as_bytes(),
                // "audit-relay" origin skips the C5 audit self-append (M-11), so
                // this direct write does not recursively append an audit row.
                origin: "audit-relay".to_owned(),
                actor: "audit-relay".to_owned(),
                request_id: RequestId::now_v7(),
                card_ref: None,
            };
            writer.commit_one(tenant, vec![batch], ctx).await.unwrap();

            // Seal [1,1] over the HONEST hash (what the outbox would have signed).
            let range_hash = compute_range_hash(
                seq,
                seq,
                &[ShippedOutboxRef {
                    seq,
                    entry_hash: honest_hash.to_vec(),
                }],
            );
            let signature = key.sign(&range_hash);
            let mut conn = TenantConn::acquire(&fx.pool, tenant).await.unwrap();
            upsert_checkpoint(&mut conn, seq, seq, &range_hash, &signature)
                .await
                .unwrap();
            conn.commit().await.unwrap();

            let verified = verify_checkpoint_walk(&fx.pool, &fx.catalog, &key, tenant)
                .await
                .unwrap();
            assert_eq!(
                verified,
                VerifyOutcome::Tampered {
                    seq_lo: seq,
                    seq_hi: seq
                },
                "recompute-from-content must catch a tampered content column even \
                 when the stored entry_hash column is left honest"
            );
        }

        /// Column values for one `vala.system.audit_log` warehouse row.
        struct AuditRowCols<'a> {
            seq: i64,
            entry_hash_hex: &'a str,
            prev_hash_hex: &'a str,
            request_id: &'a str,
            operation: &'a str,
            resource: &'a str,
            principal_id: &'a str,
            principal_kind: &'a str,
            auth_method: &'a str,
            permission: &'a str,
            decision: &'a str,
            result: &'a str,
            payload_summary: &'a str,
        }

        /// Build the 16 content-column Arrow batch for one `audit_log` row. Mirrors
        /// the relay's `build_audit_log_batch`; the writer stamps the 4 Bifrost
        /// system columns at flush. `trace_id`/`audit_card_ref` are null.
        fn audit_log_row_batch(c: &AuditRowCols<'_>) -> RecordBatch {
            let fields = vec![
                Field::new("seq", DataType::Int64, false),
                Field::new("entry_hash", DataType::Utf8, false),
                Field::new("prev_hash", DataType::Utf8, false),
                Field::new("request_id", DataType::Utf8, false),
                Field::new("trace_id", DataType::Utf8, true),
                Field::new("operation", DataType::Utf8, false),
                Field::new("resource", DataType::Utf8, false),
                Field::new("audit_card_ref", DataType::Utf8, true),
                Field::new("principal_id", DataType::Utf8, false),
                Field::new("principal_kind", DataType::Utf8, false),
                Field::new("auth_method", DataType::Utf8, false),
                Field::new("permission", DataType::Utf8, false),
                Field::new("decision", DataType::Utf8, false),
                Field::new("result", DataType::Utf8, false),
                Field::new("payload_summary", DataType::Utf8, false),
                Field::new("created_at_us", DataType::Int64, false),
            ];
            let columns: Vec<ArrayRef> = vec![
                Arc::new(Int64Array::from(vec![c.seq])),
                Arc::new(StringArray::from(vec![c.entry_hash_hex])),
                Arc::new(StringArray::from(vec![c.prev_hash_hex])),
                Arc::new(StringArray::from(vec![c.request_id])),
                Arc::new(StringArray::from(vec![None::<&str>])),
                Arc::new(StringArray::from(vec![c.operation])),
                Arc::new(StringArray::from(vec![c.resource])),
                Arc::new(StringArray::from(vec![None::<&str>])),
                Arc::new(StringArray::from(vec![c.principal_id])),
                Arc::new(StringArray::from(vec![c.principal_kind])),
                Arc::new(StringArray::from(vec![c.auth_method])),
                Arc::new(StringArray::from(vec![c.permission])),
                Arc::new(StringArray::from(vec![c.decision])),
                Arc::new(StringArray::from(vec![c.result])),
                Arc::new(StringArray::from(vec![c.payload_summary])),
                Arc::new(Int64Array::from(vec![0i64])),
            ];
            RecordBatch::try_new(Arc::new(Schema::new(fields)), columns).expect("audit row batch")
        }
    }
}
