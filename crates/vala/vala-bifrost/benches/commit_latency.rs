mod support;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use support::{workload, BenchFixture};
use tokio::runtime::Runtime;
use vala_bifrost::catalog::namespaces::BifrostNamespace;
use vala_bifrost::types::TableScope;

fn bench_commit_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let fixture = BenchFixture::setup(&rt);

    let ns = BifrostNamespace::Bifrost;
    rt.block_on(fixture.create_table(
        ns,
        "bench_commit_latency",
        vec![
            arrow::datatypes::Field::new("id", arrow::datatypes::DataType::Int64, false),
            arrow::datatypes::Field::new("payload", arrow::datatypes::DataType::Utf8, false),
        ],
        TableScope::TenantOwned,
    ));

    let batch = workload::make_bench_batch(10_000, workload::day_us(1), None, TableScope::TenantOwned);

    c.bench_function("commit_latency", |b| {
        b.to_async(&rt).iter_batched(
            || {
                // setup: get a writer handle per iteration (each flush produces a new snapshot)
                rt.block_on(async {
                    fixture
                        .catalog
                        .writer(ns, "bench_commit_latency", TableScope::TenantOwned, fixture.tenant)
                        .await
                        .unwrap()
                })
            },
            |writer| async move {
                writer.write(batch.clone()).await.unwrap();
                writer.flush().await.unwrap();
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, bench_commit_latency);
criterion_main!(benches);
