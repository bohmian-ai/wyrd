mod support;

use criterion::{Criterion, criterion_group, criterion_main};
use support::BenchFixture;
use support::workload;
use tokio::runtime::Runtime;
use vala_bifrost::catalog::namespaces::BifrostNamespace;
use vala_bifrost::types::TableScope;

fn bench_commit_latency(c: &mut Criterion) {
    let rt = Runtime::new().expect("create tokio runtime");
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

    // Measure: warm catalog load + write 10k rows + Iceberg commit + Postgres finalize.
    // Each iteration produces a new snapshot; the catalog is warm after the first call.
    c.bench_function("commit_latency", |b| {
        b.to_async(&rt).iter(|| async {
            let writer = fixture
                .catalog
                .writer(
                    ns,
                    "bench_commit_latency",
                    TableScope::TenantOwned,
                    fixture.tenant,
                )
                .await
                .expect("open bench writer");
            let batch = workload::make_bench_batch(10_000);
            writer.write(batch).await.expect("write bench batch");
            writer.flush(vala_bifrost::writer::BifrostWriteContext::system()).await.expect("flush bench writer")
        });
    });
}

criterion_group!(benches, bench_commit_latency);
criterion_main!(benches);
