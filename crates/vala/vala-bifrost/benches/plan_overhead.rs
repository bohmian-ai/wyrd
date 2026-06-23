mod support;

use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use support::BenchFixture;
use tokio::runtime::Runtime;
use vala_bifrost::catalog::namespaces::BifrostNamespace;
use vala_bifrost::types::TableScope;

fn bench_plan_overhead(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let fixture = BenchFixture::setup(&rt);

    let ns = BifrostNamespace::Bifrost;
    rt.block_on(async {
        fixture
            .create_table(
                ns,
                "bench_plan_overhead",
                vec![
                    arrow::datatypes::Field::new("id", arrow::datatypes::DataType::Int64, false),
                    arrow::datatypes::Field::new(
                        "payload",
                        arrow::datatypes::DataType::Utf8,
                        false,
                    ),
                ],
                TableScope::TenantOwned,
            )
            .await;
    });

    let provider = rt.block_on(async {
        fixture
            .catalog
            .provider(ns, "bench_plan_overhead", fixture.tenant)
            .await
            .unwrap()
    });

    let ctx = rt.block_on(async {
        let ctx = vala_bifrost::session::wyrd_session_context(fixture.tenant);
        ctx.register_table("plan_tbl", Arc::new(provider)).unwrap();
        ctx
    });

    // Measure logical → physical planning, no execution
    c.bench_function("plan_overhead", |b| {
        b.to_async(&rt).iter(|| async {
            ctx.sql("SELECT id, payload FROM plan_tbl WHERE id > 0")
                .await
                .unwrap()
                .create_physical_plan()
                .await
                .unwrap()
        });
    });
}

criterion_group!(benches, bench_plan_overhead);
criterion_main!(benches);
