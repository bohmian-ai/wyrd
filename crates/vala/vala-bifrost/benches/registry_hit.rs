mod support;

use std::sync::Arc;

use arrow::datatypes::{DataType, Field};
use criterion::{Criterion, criterion_group, criterion_main};
use support::BenchFixture;
use tokio::runtime::Runtime;
use vala_bifrost::catalog::namespaces::BifrostNamespace;
use vala_bifrost::types::TableScope;

const TABLE: &str = "bench_registry_hit";

fn bench_registry(c: &mut Criterion) {
    let rt = Runtime::new().expect("create tokio runtime");
    let fixture = BenchFixture::setup(&rt);

    let ns = BifrostNamespace::Bifrost;
    rt.block_on(fixture.create_table(
        ns,
        TABLE,
        vec![
            Field::new("id", DataType::Int64, false),
            Field::new("payload", DataType::Utf8, false),
        ],
        TableScope::TenantOwned,
    ));

    // Warm the cache: one `provider()` call loads the Iceberg metadata and
    // stores it in the registry at the current epoch.
    rt.block_on(async {
        fixture
            .catalog
            .provider(ns, TABLE, fixture.tenant)
            .await
            .expect("warm cache");
    });

    // Cache-hit path: one narrow refresh_epochs SELECT + no Iceberg metadata load.
    // This is the real Stage 2 behavior — there is no DB-free path (review C2).
    let mut group = c.benchmark_group("registry");
    group.bench_function("hit_with_epoch_read", |b| {
        b.to_async(&rt).iter(|| async {
            fixture
                .catalog
                .provider(ns, TABLE, fixture.tenant)
                .await
                .expect("provider on cache hit")
        });
    });

    // Cache-miss path: force a reload by committing a write (which bumps the epoch
    // and evicts the cache entry), then measuring the full Iceberg metadata load.
    let writer_handle = rt.block_on(async {
        fixture
            .catalog
            .writer(ns, TABLE, TableScope::TenantOwned, fixture.tenant)
            .await
            .expect("open bench writer for invalidation")
    });

    let user_schema = Arc::new(arrow::datatypes::Schema::new(vec![Field::new(
        "id",
        DataType::Int64,
        false,
    )]));
    let batch = arrow::record_batch::RecordBatch::try_new(
        user_schema,
        vec![Arc::new(arrow::array::Int64Array::from(vec![1_i64]))],
    )
    .expect("build invalidation batch");

    rt.block_on(async {
        writer_handle
            .write(batch)
            .await
            .expect("write invalidation batch");
        writer_handle.flush().await.expect("flush to bump epoch");
    });

    group.bench_function("miss_iceberg_reload", |b| {
        b.to_async(&rt).iter(|| async {
            fixture
                .catalog
                .provider(ns, TABLE, fixture.tenant)
                .await
                .expect("provider on cache miss")
        });
    });

    group.finish();
}

criterion_group!(benches, bench_registry);
criterion_main!(benches);
