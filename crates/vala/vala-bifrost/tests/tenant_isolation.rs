use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use datafusion::config::ConfigOptions;
use datafusion::datasource::MemTable;
use datafusion::optimizer::analyzer::AnalyzerRule;
use datafusion::prelude::SessionContext;
use vala_bifrost::session::TenantPredicateRule;
use vala_bifrost::types::TableScope;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::system_columns::DATA_TENANT_ID;

fn shared_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("metric", DataType::Int64, false),
        Field::new(DATA_TENANT_ID, DataType::Utf8, false),
    ]))
}

fn no_tenant_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![Field::new("val", DataType::Int64, false)]))
}

#[test]
fn tenant_predicate_rule_name_is_stable() {
    let tenant = DataTenantId::new_v7();
    let rule = TenantPredicateRule::new(tenant);
    assert_eq!(rule.name(), "wyrd_tenant_predicate");
}

#[tokio::test]
async fn tenant_predicate_rule_ignores_table_without_tenant_column() {
    let tenant = DataTenantId::new_v7();
    let rule = TenantPredicateRule::new(tenant);

    let schema = no_tenant_schema();
    let mem_table = MemTable::try_new(schema, vec![vec![]]).unwrap();

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

#[tokio::test]
async fn tenant_predicate_rule_adds_filter_for_shared_table() {
    let tenant = DataTenantId::new_v7();
    let rule = TenantPredicateRule::new(tenant);

    let schema = shared_schema();
    let mem_table = MemTable::try_new(schema, vec![vec![]]).unwrap();

    let ctx = SessionContext::new();
    ctx.register_table("shared_tbl", Arc::new(mem_table))
        .unwrap();

    let plan = ctx
        .sql("SELECT metric FROM shared_tbl")
        .await
        .unwrap()
        .into_unoptimized_plan();

    let result = rule.analyze(plan, &ConfigOptions::default()).unwrap();

    let plan_str = format!("{result:?}");
    assert!(
        plan_str.contains(DATA_TENANT_ID),
        "filter on data_tenant_id must be injected: {plan_str}"
    );
    assert!(
        plan_str.contains(&tenant.to_string()),
        "filter must contain the tenant value: {plan_str}"
    );
}

#[test]
fn wyrd_session_context_includes_tenant_rule() {
    let tenant = DataTenantId::new_v7();
    let ctx = vala_bifrost::session::wyrd_session_context(tenant);
    let state = ctx.state();
    let analyzer = state.analyzer();
    let has_rule = analyzer
        .rules
        .iter()
        .any(|r| r.name() == "wyrd_tenant_predicate");
    assert!(has_rule, "TenantPredicateRule must be registered in session");
}

/// Full isolation test: tenant A cannot read tenant B's rows from a SystemShared table.
/// Requires embedded Postgres and a writable temp directory.
/// Run with: BIFROST_TEST_DB_URL=postgres://... cargo test -p vala-bifrost --all-features tenant_isolation_zero_leak -- --ignored
#[tokio::test]
#[ignore]
async fn tenant_isolation_zero_leak() {
    let db_url = std::env::var("BIFROST_TEST_DB_URL")
        .expect("BIFROST_TEST_DB_URL must be set to run this test");

    let tmp = tempfile::tempdir().unwrap();
    let warehouse = format!("file://{}", tmp.path().display());

    let pool = Arc::new(sqlx::PgPool::connect(&db_url).await.unwrap());
    let backend = wyrd_storage::settings::BackendConfig::Local {
        root: tmp.path().to_path_buf(),
    };
    let factory = wyrd_storage::factory::iceberg_factory::iceberg_storage_factory(&backend)
        .unwrap();

    let catalog = vala_bifrost::catalog::WyrdCatalog::new(
        &db_url,
        &warehouse,
        pool.clone(),
        factory.0,
        factory.1,
    )
    .await
    .unwrap();

    let user_fields = vec![Field::new("payload", DataType::Utf8, false)];
    catalog
        .create_table(
            vala_bifrost::catalog::namespaces::BifrostNamespace::Bifrost,
            "shared_isolation",
            user_fields,
            TableScope::SystemShared,
            wyrd_spec::ids::DataTenantId::SYSTEM_OWNER,
            &[],
        )
        .await
        .unwrap();

    let tenant_a = DataTenantId::new_v7();
    let tenant_b = DataTenantId::new_v7();

    for (tenant, payload) in [(tenant_a, "from_a"), (tenant_b, "from_b")] {
        let full_schema = vala_bifrost::schema::bifrost_schema(
            vec![Field::new("payload", DataType::Utf8, false)],
            TableScope::SystemShared,
        );
        let batch = arrow::record_batch::RecordBatch::try_new(
            full_schema,
            vec![
                Arc::new(arrow::array::StringArray::from(vec![payload])),
                Arc::new(
                    arrow::array::TimestampMicrosecondArray::from(vec![0_i64])
                        .with_timezone("UTC".to_string()),
                ),
                Arc::new(
                    arrow::array::TimestampMicrosecondArray::from(vec![0_i64])
                        .with_timezone("UTC".to_string()),
                ),
                {
                    let mut b = arrow::array::FixedSizeBinaryBuilder::with_capacity(1, 16);
                    b.append_value(*uuid::Uuid::now_v7().as_bytes()).unwrap();
                    Arc::new(b.finish())
                },
                Arc::new(arrow::array::StringArray::from(vec![tenant.to_string()])),
            ],
        )
        .unwrap();

        let writer = catalog
            .writer(
                vala_bifrost::catalog::namespaces::BifrostNamespace::Bifrost,
                "shared_isolation",
                TableScope::SystemShared,
                tenant,
            )
            .await
            .unwrap();

        writer.write(batch).await.unwrap();
        writer.flush().await.unwrap();
    }

    let ctx = vala_bifrost::session::wyrd_session_context(tenant_a);
    let provider = catalog
        .provider(
            vala_bifrost::catalog::namespaces::BifrostNamespace::Bifrost,
            "shared_isolation",
            tenant_a,
        )
        .await
        .unwrap();

    ctx.register_table("shared_isolation", Arc::new(provider))
        .unwrap();

    let df = ctx
        .sql("SELECT payload FROM shared_isolation")
        .await
        .unwrap();
    let results = df.collect().await.unwrap();

    let payloads: Vec<String> = results
        .iter()
        .flat_map(|b| {
            b.column_by_name("payload")
                .unwrap()
                .as_any()
                .downcast_ref::<arrow::array::StringArray>()
                .unwrap()
                .iter()
                .flatten()
                .map(String::from)
        })
        .collect();

    assert!(
        payloads.iter().all(|p| p == "from_a"),
        "tenant A query must not return tenant B rows: {payloads:?}"
    );
    assert!(!payloads.is_empty(), "tenant A must see its own rows");
}
