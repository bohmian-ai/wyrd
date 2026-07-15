mod pg_tests {
    //! Tenant isolation for `SystemShared` Bifrost tables.
    //!
    //! Two layers, two jobs (warehouse-plan N-M1/N-M2/N-M8):
    //!   * the provider physical `FilterExec` is the PRIMARY, non-removable boundary —
    //!     it must isolate even with no analyzer registered;
    //!   * the analyzer `TenantPredicateRule` is a SECONDARY, scoped logical helper —
    //!     it injects only on `WyrdTableProvider` scans and leaves foreign providers
    //!     (`MemTable` / CTAS targets) untouched.
    //!
    //! The pure unit tests below need no database. Run value-level tests with
    //! `mise run test:bifrost`.

    use std::sync::Arc;

    use arrow::array::StringArray;
    use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
    use arrow::record_batch::RecordBatch;
    use datafusion::config::ConfigOptions;
    use datafusion::datasource::MemTable;
    use datafusion::optimizer::analyzer::AnalyzerRule;
    use datafusion::prelude::SessionContext;
    use tempfile::TempDir;
    use vala_bifrost::catalog::WyrdCatalog;
    use vala_bifrost::catalog::namespaces::BifrostNamespace;
    use vala_bifrost::session::{TenantPredicateRule, wyrd_session_context};
    use vala_bifrost::types::TableScope;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::ids::DataTenantId;
    use wyrd_spec::vala::system_columns::DATA_TENANT_ID;
    use wyrd_storage::settings::BackendConfig;

    const NS: BifrostNamespace = BifrostNamespace::Bifrost;
    const TABLE: &str = "shared_iso";

    // ── Pure analyzer unit tests (no database) ──────────────────────────────────

    fn no_tenant_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![Field::new("val", DataType::Int64, false)]))
    }

    fn tenant_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("metric", DataType::Int64, false),
            Field::new(DATA_TENANT_ID, DataType::Utf8, false),
        ]))
    }

    #[test]
    fn tenant_predicate_rule_name_is_stable() {
        let rule = TenantPredicateRule::new(DataTenantId::new_v7());
        assert_eq!(rule.name(), "wyrd_tenant_predicate");
    }

    #[tokio::test]
    async fn tenant_predicate_rule_ignores_table_without_tenant_column() {
        let rule = TenantPredicateRule::new(DataTenantId::new_v7());
        let mem_table = MemTable::try_new(no_tenant_schema(), vec![vec![]]).unwrap();

        let ctx = SessionContext::new();
        ctx.register_table("no_tenant_tbl", Arc::new(mem_table))
            .unwrap();
        let plan = ctx
            .sql("SELECT val FROM no_tenant_tbl")
            .await
            .unwrap()
            .into_unoptimized_plan();

        let result = rule.analyze(plan, &ConfigOptions::default()).unwrap();
        let plan_str = format!("{result:?}");
        assert!(
            !plan_str.contains(DATA_TENANT_ID),
            "no filter injected for tables without tenant column: {plan_str}"
        );
    }

    /// The rule is SCOPED to `WyrdTableProvider` scans (N-M8). A foreign provider
    /// (here a `MemTable`) that merely *exposes* a `data_tenant_id` column must NOT get
    /// a tenant filter injected — otherwise CTAS / `MemTable` targets would be corrupted
    /// in Stage 3. Isolation is unaffected: the provider `FilterExec` is the boundary.
    #[tokio::test]
    async fn tenant_predicate_rule_ignores_foreign_provider_with_tenant_column() {
        let rule = TenantPredicateRule::new(DataTenantId::new_v7());
        let mem_table = MemTable::try_new(tenant_schema(), vec![vec![]]).unwrap();

        let ctx = SessionContext::new();
        ctx.register_table("foreign_tbl", Arc::new(mem_table))
            .unwrap();
        let plan = ctx
            .sql("SELECT metric FROM foreign_tbl")
            .await
            .unwrap()
            .into_unoptimized_plan();

        let result = rule.analyze(plan, &ConfigOptions::default()).unwrap();
        let plan_str = format!("{result:?}");
        assert!(
            !plan_str.contains(&format!("{DATA_TENANT_ID} =")),
            "rule must not inject a tenant filter on a non-Wyrd provider: {plan_str}"
        );
    }

    #[test]
    fn wyrd_session_context_includes_tenant_rule() {
        let ctx = wyrd_session_context(DataTenantId::new_v7());
        let state = ctx.state();
        let has_rule = state
            .analyzer()
            .rules
            .iter()
            .any(|r| r.name() == "wyrd_tenant_predicate");
        assert!(
            has_rule,
            "TenantPredicateRule must be registered in session"
        );
    }

    // ── Value-level isolation tests (require Postgres) ──────────────────────────

    struct Seeded {
        _tmp: TempDir,
        catalog: WyrdCatalog,
        a: DataTenantId,
        _fixture: PgFixture,
    }

    /// Build a `SystemShared` table and seed two tenants' rows into it via two
    /// server-bound, single-tenant writes. The append batch carries USER FIELDS ONLY
    /// (`payload`); the server stamps `data_tenant_id` from each write's bound tenant.
    async fn seeded() -> Seeded {
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

        // SystemShared registration is the engine's privileged path — owner is
        // SYSTEM_OWNER (seeded by the vala migration). Under M02 each SystemShared
        // write binds its OWN data tenant into `olap_commits.data_tenant_id`, so A
        // and B must be real platform tenants (the `data_tenant_id` FK).
        catalog
            .create_table(vala_bifrost::catalog::CreateTableRequest {

                ns: NS,

                name: TABLE,

                user_fields: vec![Field::new("payload", DataType::Utf8, false)],

                scope: TableScope::SystemShared,

                tenant: DataTenantId::SYSTEM_OWNER,

                partition_columns: &[],

                audit: None,

            })
            .await
            .unwrap();

        let a = DataTenantId::new_v7();
        let b = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(a, &format!("a-{}", a.as_uuid().simple()))
            .await
            .unwrap();
        fixture
            .seed_additional_tenant_with_uuid(b, &format!("b-{}", b.as_uuid().simple()))
            .await
            .unwrap();
        write_rows(&catalog, a, &["a1", "a2", "a3"]).await;
        write_rows(&catalog, b, &["b1", "b2"]).await;

        Seeded {
            _tmp: tmp,
            catalog,
            a,
            _fixture: fixture,
        }
    }

    async fn write_rows(catalog: &WyrdCatalog, tenant: DataTenantId, payloads: &[&str]) {
        // Pre-stamp user+correlation columns for a SystemShared table.
        // `with_system_columns` appends run_id, card_uid, principal_id before the
        // server-stamped wyrd_* columns. The coordinator's flush() appends
        // wyrd_event_time, wyrd_ingested_at, wyrd_batch_id, data_tenant_id.
        // Positional cast in write_batches requires the batch column order and count
        // to match the physical schema exactly.
        let schema = Arc::new(Schema::new(vec![
            Field::new("payload", DataType::Utf8, false),
            Field::new("run_id", DataType::Utf8, true),
            Field::new("card_uid", DataType::Utf8, true),
            Field::new("principal_id", DataType::Utf8, true),
        ]));
        let nrows = payloads.len();
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(payloads.to_vec())),
                Arc::new(StringArray::from(vec![None::<&str>; nrows])),
                Arc::new(StringArray::from(vec![None::<&str>; nrows])),
                Arc::new(StringArray::from(vec![None::<&str>; nrows])),
            ],
        )
        .unwrap();
        let writer = catalog
            .writer(NS, TABLE, TableScope::SystemShared, tenant)
            .await
            .unwrap();
        writer
            .commit_one(
                tenant,
                vec![batch],
                vala_bifrost::writer::BifrostWriteContext::system(),
            )
            .await
            .unwrap();
    }

    async fn register_a(seeded: &Seeded, ctx: &SessionContext) {
        let provider = seeded.catalog.provider(NS, TABLE, seeded.a).await.unwrap();
        ctx.register_table("shared", Arc::new(provider)).unwrap();
    }

    async fn run_payloads(ctx: &SessionContext, sql: &str) -> Vec<String> {
        let batches = ctx.sql(sql).await.unwrap().collect().await.unwrap();
        batches
            .iter()
            .flat_map(|b| {
                b.column_by_name("payload")
                    .expect("payload column present")
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .expect("payload is Utf8")
                    .iter()
                    .flatten()
                    .map(String::from)
            })
            .collect()
    }

    fn assert_only_a(got: &[String]) {
        assert!(!got.is_empty(), "tenant A must see its own rows");
        assert!(
            got.iter().all(|p| p.starts_with('a')),
            "tenant A query returned foreign-tenant rows: {got:?}"
        );
    }

    /// Adversarial query shapes over a shared table, all bound to tenant A. Every
    /// shape's scan of `shared` must resolve to A's rows only.
    /// Adversarial shapes. Tenant A's payloads (`a*`) and B's (`b*`) are disjoint, so
    /// any foreign row reaching the result fails `assert_only_a`. The CROSS JOIN shape
    /// projects the SECOND relation's payload — a leak in a scan whose *rows* (not just
    /// a membership predicate) are returned would surface only here.
    const SHAPES: &[&str] = &[
        "SELECT payload FROM shared",
        "SELECT payload FROM shared WHERE payload IN (SELECT payload FROM shared)",
        "SELECT payload FROM shared s WHERE EXISTS \
     (SELECT 1 FROM shared t WHERE t.payload = s.payload)",
        "SELECT s.payload FROM shared s \
     WHERE (SELECT count(*) FROM shared t WHERE t.payload = s.payload) > 0",
        "SELECT t1.payload FROM shared t1, shared t2 WHERE t1.payload = t2.payload",
        "SELECT t2.payload FROM shared t1 CROSS JOIN shared t2",
        "SELECT payload FROM shared UNION ALL SELECT payload FROM shared",
        "WITH x AS (SELECT payload FROM shared) SELECT payload FROM x",
    ];

    async fn assert_shapes_isolate(ctx: &SessionContext) {
        for sql in SHAPES {
            let got = run_payloads(ctx, sql).await;
            assert_only_a(&got);
            assert!(got.len() >= 3, "shape `{sql}` lost A rows: {got:?}");
        }
    }

    /// Full shape set with the analyzer registered (both layers active).
    #[tokio::test]
    async fn value_level_isolation_with_analyzer() {
        let s = seeded().await;
        let ctx = wyrd_session_context(s.a);
        register_a(&s, &ctx).await;
        assert_shapes_isolate(&ctx).await;
    }

    /// THE security-boundary proof (MAJOR-8 / N-M1): the SAME adversarial shapes under
    /// a plain `SessionContext::new()` with NO `TenantPredicateRule` registered. They
    /// hold iff the provider physical `FilterExec` — not the analyzer — is the
    /// authoritative boundary for every shape, including the inner-relation projection.
    #[tokio::test]
    async fn value_level_isolation_provider_only_no_analyzer() {
        let s = seeded().await;
        let ctx = SessionContext::new();
        register_a(&s, &ctx).await;
        assert_shapes_isolate(&ctx).await;
    }

    /// The SECONDARY layer must actually fire. Prove the scoped analyzer injects a
    /// `data_tenant_id = <a>` filter on a REAL `WyrdTableProvider` scan — guarding
    /// against the two-hop downcast silently regressing to always-`None`, which every
    /// value-level test would survive (the provider `FilterExec` would still isolate).
    #[tokio::test]
    async fn analyzer_injects_on_real_wyrd_provider() {
        let s = seeded().await;
        let ctx = SessionContext::new();
        register_a(&s, &ctx).await;

        let plan = ctx
            .sql("SELECT payload FROM shared")
            .await
            .unwrap()
            .into_unoptimized_plan();
        let analyzed = TenantPredicateRule::new(s.a)
            .analyze(plan, &ConfigOptions::default())
            .unwrap();

        let plan_str = format!("{analyzed:?}");
        assert!(
            plan_str.contains(DATA_TENANT_ID),
            "analyzer must inject a tenant filter on a WyrdTableProvider scan: {plan_str}"
        );
        assert!(
            plan_str.contains(&s.a.to_string()),
            "injected filter must bind tenant A: {plan_str}"
        );
    }

    /// A view defined over the shared table must isolate the same way.
    #[tokio::test]
    async fn value_level_isolation_through_view() {
        let s = seeded().await;
        let ctx = wyrd_session_context(s.a);
        register_a(&s, &ctx).await;

        ctx.sql("CREATE VIEW v AS SELECT payload FROM shared")
            .await
            .unwrap()
            .collect()
            .await
            .unwrap();

        let got = run_payloads(&ctx, "SELECT payload FROM v").await;
        assert_only_a(&got);
    }

    /// CTAS exfiltration shape: the materialized target must contain only A's rows.
    /// The source scan (`shared`, a `WyrdTableProvider`) is filtered to A; the analyzer
    /// must NOT inject onto the CTAS `MemTable` target (it has no `data_tenant_id` column).
    #[tokio::test]
    async fn value_level_isolation_through_ctas() {
        let s = seeded().await;
        let ctx = wyrd_session_context(s.a);
        register_a(&s, &ctx).await;

        ctx.sql("CREATE TABLE mat AS SELECT payload FROM shared")
            .await
            .unwrap()
            .collect()
            .await
            .unwrap();

        let got = run_payloads(&ctx, "SELECT payload FROM mat").await;
        assert_only_a(&got);
    }
}
