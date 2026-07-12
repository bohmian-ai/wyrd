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

        use sqlx::PgPool;
        use tempfile::TempDir;
        use uuid::Uuid;
        use vala_bifrost::catalog::WyrdCatalog;
        use vala_bifrost::relay::AuditRelay;
        use vala_bifrost::serving::audit_seal::verify::{VerifyOutcome, verify_checkpoint_walk};
        use vala_bifrost::serving::audit_seal::worker::seal_shipped_range;
        use vala_sql::TenantConn;
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
    }
}
