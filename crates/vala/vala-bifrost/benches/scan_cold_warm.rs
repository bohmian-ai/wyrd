mod support;

use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use datafusion::datasource::TableProvider;
use support::{BenchFixture, workload};
use tokio::runtime::Runtime;
use vala_bifrost::catalog::namespaces::BifrostNamespace;
use vala_bifrost::types::TableScope;

fn bench_scan_cold_warm(c: &mut Criterion) {
    let rt = Runtime::new().expect("create tokio runtime");
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

        // Ingest 100k rows across 10 commits (one Parquet file each).
        for _ in 0..10_i64 {
            let batch = workload::make_bench_batch(10_000);
            let writer = fixture
                .catalog
                .writer(ns, "bench_scan_cw", TableScope::TenantOwned, fixture.tenant)
                .await
                .expect("open bench writer");
            writer.write(batch).await.expect("write bench batch");
            writer
                .flush(vala_bifrost::writer::BifrostWriteContext::system())
                .await
                .expect("flush bench writer");
        }
    });

    // Wrap in Arc<dyn TableProvider> so it can be cloned cheaply across iterations
    let provider: Arc<dyn TableProvider> = rt.block_on(async {
        Arc::new(
            fixture
                .catalog
                .provider(ns, "bench_scan_cw", fixture.tenant)
                .await
                .expect("get table provider"),
        )
    });

    let mut group = c.benchmark_group("scan_cold_warm");

    // Cold: fresh SessionContext per iteration
    group.bench_function("cold", |b| {
        let p = Arc::clone(&provider);
        b.to_async(&rt).iter(|| {
            let p = Arc::clone(&p);
            async move {
                let ctx = vala_bifrost::session::wyrd_session_context(fixture.tenant);
                ctx.register_table("t", p).expect("register table");
                ctx.sql("SELECT id, payload FROM t")
                    .await
                    .expect("parse SQL")
                    .collect()
                    .await
                    .expect("collect results")
            }
        });
    });

    // Warm: shared SessionContext registered once
    let warm_ctx = rt.block_on(async {
        let ctx = vala_bifrost::session::wyrd_session_context(fixture.tenant);
        ctx.register_table("t", Arc::clone(&provider))
            .expect("register table");
        ctx
    });

    group.bench_function("warm", |b| {
        b.to_async(&rt).iter(|| async {
            warm_ctx
                .sql("SELECT id, payload FROM t")
                .await
                .expect("parse SQL")
                .collect()
                .await
                .expect("collect results")
        });
    });

    group.finish();
}

criterion_group!(benches, bench_scan_cold_warm);
criterion_main!(benches);
