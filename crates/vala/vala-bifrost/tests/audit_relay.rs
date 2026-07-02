//! S3.C5 background audit relay: outbox → `vala.system.audit_log`.
//!
//! Proves the relay claims unshipped hash-chained rows across tenants, ships one
//! `RecordBatch` per tenant into the queryable warehouse table, and marks the
//! shipped `seq` range — without self-feeding the outbox (M-11) and idempotently
//! across a crash between flush and mark.
//!
//! Bare `#[sqlx::test]` + in-body `migrate_for_test` (the vala migrations are not
//! self-contained — no `migrations=` arg). Run with a live Postgres:
//! `DATABASE_URL=... cargo test -p vala-bifrost --all-features audit_relay`.

mod audit_relay {
    use std::sync::Arc;

    use arrow::array::Int64Array;
    use datafusion::prelude::SessionContext;
    use sqlx::PgPool;
    use tempfile::TempDir;
    use uuid::Uuid;
    use vala_bifrost::catalog::WyrdCatalog;
    use vala_bifrost::catalog::namespaces::BifrostNamespace;
    use vala_bifrost::relay::{AUDIT_LOG_TABLE, AuditRelay};
    use vala_sql::TenantConn;
    use wyrd_spec::auth::{PrincipalId, PrincipalKind};
    use wyrd_spec::ids::DataTenantId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};
    use wyrd_storage::factory::iceberg_factory::iceberg_storage_factory;
    use wyrd_storage::settings::BackendConfig;

    /// Live-catalog fixture with the audit-log table ensured and a boot-time relay.
    struct Fixture {
        _tmp: TempDir,
        catalog: Arc<WyrdCatalog>,
        pool: Arc<PgPool>,
    }

    async fn setup(pool: PgPool) -> Fixture {
        let pool = Arc::new(pool);
        vala_sql::testing::migrate_for_test(&pool).await.unwrap();

        let tmp = tempfile::tempdir().unwrap();
        let warehouse = format!("file://{}", tmp.path().display());
        let backend = BackendConfig::Local {
            root: tmp.path().to_path_buf(),
        };
        let (factory, props) = iceberg_storage_factory(&backend).unwrap();
        let catalog_uri = vala_sql::testing::catalog_uri(&pool);
        let catalog = WyrdCatalog::new(&catalog_uri, &warehouse, pool.clone(), None, factory, props)
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
        }
    }

    fn relay(fx: &Fixture) -> AuditRelay {
        AuditRelay::new(fx.catalog.clone(), fx.pool.clone())
    }

    /// A minimal `User`-principal audit event; `operation` distinguishes rows.
    fn audit_event(operation: &str) -> AuditEvent {
        AuditEvent {
            request_id: RequestId::parse(&Uuid::now_v7().to_string()).unwrap(),
            trace_id: None,
            operation: operation.to_string(),
            resource: "vala.bifrost.thing".to_string(),
            card_ref: None,
            principal_id: PrincipalId::new(Uuid::now_v7()),
            principal_kind: PrincipalKind::User,
            auth_method: AuthMethod::Internal,
            permission: "bifrost.record_write".to_string(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: operation.to_string(),
        }
    }

    /// Append `n` hash-chained outbox rows under `tenant`'s bind.
    ///
    /// `vala.audit_chain_head.data_tenant_id` is FK'd to `platform.tenants`, so
    /// the tenant is registered first (the SystemShared warehouse write, by
    /// contrast, only stamps `data_tenant_id` and needs no FK row).
    async fn seed_audit(pool: &PgPool, tenant: DataTenantId, n: usize) {
        vala_sql::testing::seed_tenant(pool, tenant.as_uuid())
            .await
            .unwrap();
        let mut conn = TenantConn::acquire(pool, tenant).await.unwrap();
        for i in 0..n {
            vala_sql::queries::audit_outbox::append_audit(&mut conn, &audit_event(&format!("op-{i}")))
                .await
                .unwrap();
        }
        conn.commit().await.unwrap();
    }

    /// Count rows in `vala.system.audit_log` visible to `tenant` (provider-isolated).
    async fn audit_log_count(catalog: &WyrdCatalog, tenant: DataTenantId) -> i64 {
        let provider = catalog
            .provider(BifrostNamespace::System, AUDIT_LOG_TABLE, tenant)
            .await
            .unwrap();
        let ctx = SessionContext::new();
        ctx.register_table("audit_log", Arc::new(provider)).unwrap();
        let batches = ctx
            .sql("SELECT count(*) AS n FROM audit_log")
            .await
            .unwrap()
            .collect()
            .await
            .unwrap();
        batches[0]
            .column_by_name("n")
            .expect("count column")
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("count is Int64")
            .value(0)
    }

    /// A full tick ships every tenant's rows into the warehouse, isolated per
    /// tenant, and drains the outbox — proving the relay never self-feeds the
    /// spine (a self-appended audit row would resurface as unshipped work).
    #[sqlx::test]
    async fn ships_all_tenants_and_drains_outbox(pool: PgPool) {
        let fx = setup(pool).await;
        let a = DataTenantId::new_v7();
        let b = DataTenantId::new_v7();
        seed_audit(&fx.pool, a, 3).await;
        seed_audit(&fx.pool, b, 2).await;

        let shipped = relay(&fx).tick(100).await.unwrap();
        assert_eq!(shipped, 5, "every seeded row must be marked shipped");

        assert_eq!(audit_log_count(&fx.catalog, a).await, 3, "tenant A ships 3");
        assert_eq!(audit_log_count(&fx.catalog, b).await, 2, "tenant B ships 2");

        // The relay's own write must NOT append an outbox row (M-11): a re-claim
        // finds no leftover work, which also proves the range was marked shipped.
        let leftover = relay(&fx).claim(100).await.unwrap();
        assert!(
            leftover.is_empty(),
            "outbox drained + no relay self-feed, got {} shipment(s)",
            leftover.len()
        );
    }

    /// Crash between flush and mark: re-shipping the same claim (deterministic
    /// `batch_id`) replays the prior `vala.olap_commits` commit, so the warehouse
    /// gains no duplicate rows. Models it by shipping without marking, re-claiming
    /// the still-unshipped range, and shipping again.
    #[sqlx::test]
    async fn reship_after_crash_is_idempotent(pool: PgPool) {
        let fx = setup(pool).await;
        let t = DataTenantId::new_v7();
        seed_audit(&fx.pool, t, 4).await;

        let relay = relay(&fx);

        // First pass: ship but crash before marking.
        let first = relay.claim(100).await.unwrap();
        assert_eq!(first.len(), 1, "one tenant, one shipment");
        relay.ship(&first[0]).await.unwrap();
        assert_eq!(audit_log_count(&fx.catalog, t).await, 4, "first ship lands 4");

        // Recovery: the range is still unshipped, so a re-claim returns the same
        // seq range and thus the same deterministic batch_id.
        let second = relay.claim(100).await.unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(
            second[0].batch_id, first[0].batch_id,
            "re-claim of the same range must derive the same batch_id"
        );

        // Re-ship replays the committed batch: no duplicate warehouse rows.
        relay.ship(&second[0]).await.unwrap();
        assert_eq!(
            audit_log_count(&fx.catalog, t).await,
            4,
            "re-ship must not duplicate warehouse rows"
        );

        // Finally mark shipped and confirm the outbox drains.
        relay.mark_shipped(&second[0]).await.unwrap();
        assert!(relay.claim(100).await.unwrap().is_empty(), "outbox drained");
    }
}
