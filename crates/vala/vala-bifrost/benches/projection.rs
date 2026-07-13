mod support;

use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use support::{BenchFixture, workload};
use tokio::runtime::Runtime;
use vala_bifrost::catalog::namespaces::BifrostNamespace;
use vala_bifrost::types::TableScope;

const COL_COUNT: usize = 20;
const ROW_COUNT: usize = 100_000;
const BATCH_SIZE: usize = 10_000;

fn bench_projection(c: &mut Criterion) {
    let rt = Runtime::new().expect("create tokio runtime");
    let fixture = BenchFixture::setup(&rt);

    let ns = BifrostNamespace::Bifrost;
    rt.block_on(async {
        fixture
            .create_wide_table(ns, "bench_projection", COL_COUNT)
            .await;

        for _ in 0..(ROW_COUNT / BATCH_SIZE) {
            let batch = workload::make_wide_batch(BATCH_SIZE, COL_COUNT);
            let writer = fixture
                .catalog
                .writer(
                    ns,
                    "bench_projection",
                    TableScope::TenantOwned,
                    fixture.tenant,
                )
                .await
                .expect("open bench writer");
            writer
                .commit_one(
                    fixture.tenant,
                    vec![batch],
                    vala_bifrost::writer::BifrostWriteContext::system(),
                )
                .await
                .expect("commit bench batch");
        }
    });

    let provider = rt.block_on(async {
        fixture
            .catalog
            .provider(ns, "bench_projection", fixture.tenant)
            .await
            .expect("get table provider")
    });

    let ctx = rt.block_on(async {
        let ctx = vala_bifrost::session::wyrd_session_context(fixture.tenant);
        ctx.register_table("proj_tbl", Arc::new(provider))
            .expect("register proj table");
        ctx
    });

    // 2/N: 2 columns, N/2: half columns, N/N: all columns
    let two_cols = "SELECT col_0, col_1 FROM proj_tbl";
    let half_cols = (0..COL_COUNT / 2)
        .map(|i| format!("col_{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let half_sql = format!("SELECT {half_cols} FROM proj_tbl");
    let all_sql = format!(
        "SELECT {} FROM proj_tbl",
        (0..COL_COUNT)
            .map(|i| format!("col_{i}"))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let cases: &[(&str, &str)] = &[
        ("2_of_N", two_cols),
        ("N_div_2", &half_sql),
        ("N_of_N", &all_sql),
    ];

    let mut group = c.benchmark_group("projection");

    for &(label, sql) in cases {
        group.bench_with_input(BenchmarkId::new("cols", label), &sql, |b, sql| {
            b.to_async(&rt).iter(|| async {
                ctx.sql(sql)
                    .await
                    .expect("parse SQL")
                    .collect()
                    .await
                    .expect("collect results")
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_projection);
criterion_main!(benches);
