mod support;

use std::sync::Arc;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use support::{workload, BenchFixture};
use tokio::runtime::Runtime;
use vala_bifrost::catalog::namespaces::BifrostNamespace;
use vala_bifrost::types::TableScope;

/// Table has 100 partitions (days 0..99), 1k rows each = 100k total.
/// Selectivity levels: 1%, 3%, 10%, 30%, 100% of files (days).
const TOTAL_DAYS: i64 = 100;
const ROWS_PER_DAY: usize = 1_000;

fn bench_selectivity(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let fixture = BenchFixture::setup(&rt);

    let ns = BifrostNamespace::Bifrost;
    rt.block_on(async {
        fixture
            .create_table(
                ns,
                "bench_selectivity",
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

        for day in 0..TOTAL_DAYS {
            let batch = workload::make_bench_batch(
                ROWS_PER_DAY,
                workload::day_us(day),
                None,
                TableScope::TenantOwned,
            );
            let writer = fixture
                .catalog
                .writer(ns, "bench_selectivity", TableScope::TenantOwned, fixture.tenant)
                .await
                .unwrap();
            writer.write(batch).await.unwrap();
            writer.flush().await.unwrap();
        }
    });

    let provider = rt.block_on(async {
        fixture
            .catalog
            .provider(ns, "bench_selectivity", fixture.tenant)
            .await
            .unwrap()
    });

    let ctx = rt.block_on(async {
        let ctx = vala_bifrost::session::wyrd_session_context(fixture.tenant);
        ctx.register_table("sel_tbl", Arc::new(provider)).unwrap();
        ctx
    });

    // Selectivity = fraction of TOTAL_DAYS covered by the time-range predicate
    let selectivity_cases: &[(&str, i64, i64)] = &[
        ("1pct", 0, 1),
        ("3pct", 0, 3),
        ("10pct", 0, 10),
        ("30pct", 0, 30),
        ("100pct", 0, TOTAL_DAYS),
    ];

    let mut group = c.benchmark_group("selectivity");

    for &(label, start_day, end_day) in selectivity_cases {
        let start_us = workload::day_us(start_day);
        let end_us = workload::day_us(end_day);
        let sql = format!(
            "SELECT id FROM sel_tbl \
             WHERE wyrd_event_time >= arrow_cast({start_us}, 'Timestamp(Microsecond, Some(\"+00:00\"))') \
             AND wyrd_event_time < arrow_cast({end_us}, 'Timestamp(Microsecond, Some(\"+00:00\"))')"
        );
        group.bench_with_input(BenchmarkId::new("sel", label), &sql, |b, sql| {
            b.to_async(&rt).iter(|| async {
                ctx.sql(sql).await.unwrap().collect().await.unwrap()
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_selectivity);
criterion_main!(benches);
