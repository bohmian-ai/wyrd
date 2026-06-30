mod support;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use support::{BenchFixture, workload};
use tokio::runtime::Runtime;
use vala_bifrost::catalog::namespaces::BifrostNamespace;
use vala_bifrost::types::TableScope;

fn bench_write_throughput(c: &mut Criterion) {
    let rt = Runtime::new().expect("create tokio runtime");
    let fixture = BenchFixture::setup(&rt);

    let ns = BifrostNamespace::Bifrost;
    rt.block_on(fixture.create_table(
        ns,
        "bench_write_throughput",
        vec![
            arrow::datatypes::Field::new("id", arrow::datatypes::DataType::Int64, false),
            arrow::datatypes::Field::new("payload", arrow::datatypes::DataType::Utf8, false),
        ],
        TableScope::TenantOwned,
    ));

    let mut group = c.benchmark_group("write_throughput");

    for batch_size in [1_000_usize, 10_000, 100_000] {
        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            &batch_size,
            |b, &size| {
                let batch = workload::make_bench_batch(size);

                b.to_async(&rt).iter(|| async {
                    let writer = fixture
                        .catalog
                        .writer(
                            ns,
                            "bench_write_throughput",
                            TableScope::TenantOwned,
                            fixture.tenant,
                        )
                        .await
                        .expect("open bench writer");
                    writer
                        .write(batch.clone())
                        .await
                        .expect("write bench batch");
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_write_throughput);
criterion_main!(benches);
