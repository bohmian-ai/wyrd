//! Timing and the one Markdown table each section prints.
//!
//! A pair is two sides running the same SQL over the same stored data. Before
//! timing, both sides must return identical rows. Then each side runs
//! [`WARM_UP`] untimed times and [`RUNS`] timed times, the sides alternating
//! every run so machine drift lands on both equally. One timed run is one query
//! end to end: plan, execute, and collect every batch.

use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::ArrayRef;
use arrow::util::display::array_value_to_string;
use datafusion::prelude::SessionContext;
use parquet_variant_compute::variant_to_json;
use wyrd_types::variant::is_variant;

/// Untimed runs of each side before timing.
const WARM_UP: usize = 3;
/// Timed runs of each side.
const RUNS: usize = 30;

/// One side of a pair: a session and the SQL it runs.
pub struct Side<'a> {
    /// The session the query runs in.
    pub context: &'a SessionContext,
    /// The query.
    pub sql: &'a str,
}

/// The p50 and p95 of one side's timed runs.
pub struct Latency {
    /// Median run.
    pub p50: Duration,
    /// 95th percentile run.
    pub p95: Duration,
}

/// One section's results: a titled table with a left and a right side.
pub struct Report {
    /// What the section measures, printed above the table.
    title: &'static str,
    /// The left side's column label.
    left: &'static str,
    /// The right side's column label.
    right: &'static str,
    /// `(case, left, right)` in run order.
    pub rows: Vec<(String, Latency, Latency)>,
}

impl Report {
    /// An empty report comparing `left` with `right`.
    pub fn new(title: &'static str, left: &'static str, right: &'static str) -> Self {
        Self {
            title,
            left,
            right,
            rows: Vec::new(),
        }
    }

    /// Asserts `left` and `right` return identical rows, then times both and
    /// records the result under `case`.
    ///
    /// # Panics
    ///
    /// Panics when either side fails to plan or execute, or the sides return
    /// different rows.
    pub async fn time(&mut self, case: &str, left: Side<'_>, right: Side<'_>) {
        let expected = rows(left.context, left.sql).await;
        assert!(
            rows(right.context, right.sql).await == expected,
            "{case}: `{}` and `{}` return different rows",
            left.sql,
            right.sql
        );
        for _ in 0..WARM_UP {
            run(&left).await;
            run(&right).await;
        }
        let (mut lefts, mut rights) = (Vec::with_capacity(RUNS), Vec::with_capacity(RUNS));
        for _ in 0..RUNS {
            lefts.push(run(&left).await);
            rights.push(run(&right).await);
        }
        eprintln!("timed {case}");
        self.rows
            .push((case.to_owned(), Latency::of(lefts), Latency::of(rights)));
    }

    /// Prints the section title and its table.
    pub fn print(&self) {
        let (left, right) = (self.left, self.right);
        println!("\n## {}\n", self.title);
        println!("| case | {left} p50 | {left} p95 | {right} p50 | {right} p95 |");
        println!("|---|---:|---:|---:|---:|");
        for (case, l, r) in &self.rows {
            println!(
                "| `{case}` | {} | {} | {} | {} |",
                micros(l.p50),
                micros(l.p95),
                micros(r.p50),
                micros(r.p95)
            );
        }
    }
}

impl Latency {
    /// The p50 and p95 of `runs`, by nearest rank.
    pub fn of(mut runs: Vec<Duration>) -> Self {
        runs.sort_unstable();
        let rank = |percent: usize| runs[(runs.len() * percent).div_ceil(100).max(1) - 1];
        Self {
            p50: rank(50),
            p95: rank(95),
        }
    }
}

/// Renders `duration` in whole microseconds.
pub fn micros(duration: Duration) -> String {
    format!("{} µs", duration.as_micros())
}

/// Runs `side` once end to end and returns how long it took.
///
/// # Panics
///
/// Panics when the query fails to plan or execute.
pub async fn run(side: &Side<'_>) -> Duration {
    let start = Instant::now();
    side.context
        .sql(side.sql)
        .await
        .expect("plans")
        .collect()
        .await
        .expect("executes");
    start.elapsed()
}

/// Every row `sql` returns, each rendered as its comma-joined columns, sorted.
///
/// A Variant column renders as JSON, so the same values compare equal
/// whether a file stored them shredded or unshredded.
///
/// # Panics
///
/// Panics when the query fails to plan, execute, or render.
pub async fn rows(context: &SessionContext, sql: &str) -> Vec<String> {
    let batches = context
        .sql(sql)
        .await
        .unwrap_or_else(|error| panic!("{sql}: plans: {error}"))
        .collect()
        .await
        .unwrap_or_else(|error| panic!("{sql}: executes: {error}"));
    let mut rows = Vec::new();
    for batch in &batches {
        let columns: Vec<ArrayRef> = batch
            .schema()
            .fields()
            .iter()
            .zip(batch.columns())
            .map(|(field, column)| {
                if is_variant(field) {
                    Arc::new(variant_to_json(column).expect("Variant renders as JSON"))
                } else {
                    Arc::clone(column)
                }
            })
            .collect();
        for row in 0..batch.num_rows() {
            let values = columns
                .iter()
                .map(|column| array_value_to_string(column, row).expect("renders"));
            rows.push(values.collect::<Vec<_>>().join(","));
        }
    }
    rows.sort();
    rows
}
