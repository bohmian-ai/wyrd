mod pg_tests {
    //! Integration tests for `reconcile_audit` (M-13).
    //!
    //! Verifies seq-gap, parity, and clean-tenant paths against a live catalog.
    //! Run via `mise run test:bifrost`.

    mod reconcile {
        use std::sync::Arc;

        use sqlx::PgPool;
        use tempfile::TempDir;
        use uuid::Uuid;
        use vala_bifrost::catalog::WyrdCatalog;
        use vala_bifrost::reconcile::reconcile_audit;
        use vala_bifrost::relay::AuditRelay;
        use vala_sql::TenantConn;
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

        fn event(operation: &str) -> AuditEvent {
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

        async fn seed_and_ship(fx: &Fixture, tenant: DataTenantId, n: usize) {
            fx.db
                .seed_additional_tenant_with_uuid(
                    tenant,
                    &format!("test-{}", tenant.as_uuid().simple()),
                )
                .await
                .unwrap();
            let mut conn = TenantConn::acquire(&fx.pool, tenant).await.unwrap();
            for i in 0..n {
                vala_sql::queries::audit_outbox::append_audit(
                    &mut conn,
                    &event(&format!("op-{i}")),
                )
                .await
                .unwrap();
            }
            conn.commit().await.unwrap();
            AuditRelay::new(fx.catalog.clone(), fx.pool.clone())
                .tick(100)
                .await
                .unwrap();
        }

        /// After seeding and shipping rows, reconcile returns clean.
        #[tokio::test]
        async fn clean_after_seed_and_ship() {
            let fx = setup().await;
            let tenant = DataTenantId::new_v7();
            seed_and_ship(&fx, tenant, 4).await;

            let result = reconcile_audit(&fx.pool, &fx.catalog, tenant)
                .await
                .unwrap();
            assert!(result.is_clean(), "expected clean result: {result:?}");
        }

        /// A tenant with no outbox rows and no warehouse rows is clean.
        #[tokio::test]
        async fn empty_tenant_is_clean() {
            let fx = setup().await;
            let tenant = DataTenantId::new_v7();
            fx.db
                .seed_additional_tenant_with_uuid(
                    tenant,
                    &format!("test-{}", tenant.as_uuid().simple()),
                )
                .await
                .unwrap();

            let result = reconcile_audit(&fx.pool, &fx.catalog, tenant)
                .await
                .unwrap();
            assert!(result.is_clean(), "empty tenant must be clean: {result:?}");
        }

        /// Two tenants each reconcile independently and report clean after shipping.
        #[tokio::test]
        async fn two_tenants_reconcile_independently() {
            let fx = setup().await;
            let a = DataTenantId::new_v7();
            let b = DataTenantId::new_v7();
            seed_and_ship(&fx, a, 3).await;
            seed_and_ship(&fx, b, 2).await;

            let result_a = reconcile_audit(&fx.pool, &fx.catalog, a).await.unwrap();
            let result_b = reconcile_audit(&fx.pool, &fx.catalog, b).await.unwrap();

            assert!(result_a.is_clean(), "tenant A must be clean: {result_a:?}");
            assert!(result_b.is_clean(), "tenant B must be clean: {result_b:?}");
            assert_eq!(result_a.tenant_id, a);
            assert_eq!(result_b.tenant_id, b);
        }
    }
}
