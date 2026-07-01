mod support;

use std::sync::Arc;

use arrow::array::{Int64Array, StringArray};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use support::{BenchFixture, workload};
use tokio::runtime::Runtime;
use vala_bifrost::catalog::namespaces::BifrostNamespace;
use vala_bifrost::types::TableScope;

/// Table has 100 files (days 0..99), 1k rows each = 100k total. Each file holds a
/// disjoint `id` range (`day*ROWS_PER_DAY .. (day+1)*ROWS_PER_DAY`) so a range
/// predicate on the user `id` column prunes a known fraction of files. (System
/// columns are server-stamped at flush, so the bench cannot key pruning on
/// `wyrd_event_time`.)
const TOTAL_DAYS: i64 = 100;
const ROWS_PER_DAY: usize = 1_000;

/// One file's worth of rows with `id` in `[day*ROWS_PER_DAY, (day+1)*ROWS_PER_DAY)`.
fn day_batch(n: usize, day: i64) -> arrow::record_batch::RecordBatch {
    let n_i64 = i64::try_from(n).expect("n fits i64");
    let base = day * n_i64;
    let ids: Int64Array = (0..n_i64).map(|i| base + i).collect();
    let payloads: StringArray = (0..n).map(|i| Some(format!("p{i}"))).collect();
    arrow::record_batch::RecordBatch::try_new(
        workload::simple_schema(),
        vec![Arc::new(ids), Arc::new(payloads)],
    )
    .expect("build day record batch")
}

fn bench_selectivity(c: &mut Criterion) {
    let rt = Runtime::new().expect("create tokio runtime");
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
            let batch = day_batch(ROWS_PER_DAY, day);
            let writer = fixture
                .catalog
                .writer(
                    ns,
                    "bench_selectivity",
                    TableScope::TenantOwned,
                    fixture.tenant,
                )
                .await
                .expect("open bench writer");
            writer.write(batch).await.expect("write bench batch");
            writer.flush(vala_bifrost::writer::BifrostWriteContext::system()).await.expect("flush bench writer");
        }
    });

    let provider = rt.block_on(async {
        fixture
            .catalog
            .provider(ns, "bench_selectivity", fixture.tenant)
            .await
            .expect("get table provider")
    });

    let ctx = rt.block_on(async {
        let ctx = vala_bifrost::session::wyrd_session_context(fixture.tenant);
        ctx.register_table("sel_tbl", Arc::new(provider))
            .expect("register sel table");
        ctx
    });

    // Selectivity = fraction of TOTAL_DAYS (files) covered by the id-range predicate.
    let selectivity_cases: &[(&str, i64, i64)] = &[
        ("1pct", 0, 1),
        ("3pct", 0, 3),
        ("10pct", 0, 10),
        ("30pct", 0, 30),
        ("100pct", 0, TOTAL_DAYS),
    ];

    let mut group = c.benchmark_group("selectivity");

    for &(label, start_day, end_day) in selectivity_cases {
        let rows = i64::try_from(ROWS_PER_DAY).expect("ROWS_PER_DAY fits i64");
        let start_id = start_day * rows;
        let end_id = end_day * rows;
        let sql = format!("SELECT id FROM sel_tbl WHERE id >= {start_id} AND id < {end_id}");
        group.bench_with_input(BenchmarkId::new("sel", label), &sql, |b, sql| {
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

criterion_group!(benches, bench_selectivity);
criterion_main!(benches);
