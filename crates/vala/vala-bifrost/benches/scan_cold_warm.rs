mod support;

use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion};
use support::{workload, BenchFixture};
use tokio::runtime::Runtime;
use vala_bifrost::catalog::namespaces::BifrostNamespace;
use vala_bifrost::types::TableScope;

fn bench_scan_cold_warm(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let fixture = BenchFixture::setup(&rt);

    let ns = BifrostNamespace::Bifrost;
    rt.block_on(async {
        fixture
            .create_table(
                ns,
                "bench_scan_cw",
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

        // Ingest 100k rows across 10 files (10k each, different days for partition variety)
        for day in 0..10_i64 {
            let batch =
                workload::make_bench_batch(10_000, workload::day_us(day), None, TableScope::TenantOwned);
            let writer = fixture
                .catalog
                .writer(ns, "bench_scan_cw", TableScope::TenantOwned, fixture.tenant)
                .await
                .unwrap();
            writer.write(batch).await.unwrap();
            writer.flush().await.unwrap();
        }
    });

    let provider = rt.block_on(async {
        fixture
            .catalog
            .provider(ns, "bench_scan_cw", fixture.tenant)
            .await
            .unwrap()
    });

    let mut group = c.benchmark_group("scan_cold_warm");

    // Cold: fresh SessionContext per iteration
    group.bench_function("cold", |b| {
        let provider = provider.clone();
        b.to_async(&rt).iter(|| async {
            let ctx = vala_bifrost::session::wyrd_session_context(fixture.tenant);
            ctx.register_table("t", Arc::new(provider.clone())).unwrap();
            ctx.sql("SELECT id, payload FROM t")
                .await
                .unwrap()
                .collect()
                .await
                .unwrap()
        });
    });

    // Warm: shared SessionContext, registered once
    let warm_ctx = rt.block_on(async {
        let ctx = vala_bifrost::session::wyrd_session_context(fixture.tenant);
        ctx.register_table("t", Arc::new(provider.clone())).unwrap();
        ctx
    });

    group.bench_function("warm", |b| {
        b.to_async(&rt).iter(|| async {
            warm_ctx
                .sql("SELECT id, payload FROM t")
                .await
                .unwrap()
                .collect()
                .await
                .unwrap()
        });
    });

    group.finish();
}

criterion_group!(benches, bench_scan_cold_warm);
criterion_main!(benches);
