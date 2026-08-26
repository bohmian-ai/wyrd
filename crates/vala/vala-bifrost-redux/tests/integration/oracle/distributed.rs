//! Distributed execution: plan shape, participant cut selection, partition
//! assignment, barriering, deadlines, cancellation, and owner release.
//!
//! Module of the `oracle` group; shared fixtures live in `support.rs`.

use std::sync::Arc;
use std::time::{Duration, Instant};
use vala_bifrost_redux::oracle::{OracleConfig, TailTransportDirectory, TestPostgresOracleAudit};
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, QueryTerminalOutcome, VisibilityMode,
};

use super::support::*;

/// A real pinned Iceberg data file executes through the shared local fragment path.
#[tokio::test]
async fn oracle_distributes_real_pinned_iceberg_leaf_without_double_scan() {
    let fixture = OracleFixture::new("oracle_distributed_iceberg").await;
    let seeded = fixture
        .seed_hot_rows(&[(7, fixture.tenant), (9, fixture.tenant)])
        .await;
    fixture.append_hot_to_iceberg_snapshot(&seeded).await;
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            OracleConfig::default(),
        )
        .await;
    let recorder = wyrd_bench::BenchmarkRecorder::new();
    let spans = SpanProbe::default();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let _span_guard = tracing::subscriber::set_default(spans.clone());
    let query = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!(
                    "SELECT count(*) AS total, sum(value) AS total_value FROM {}",
                    fixture.table.fqn()
                ),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .unwrap_or_else(|error| {
            panic!(
                "distributed Iceberg query failed: {error:?}; {:?}",
                spans.snapshot()
            )
        });
    let result = decoded_query(query).await;
    assert_eq!(
        result.terminal.outcome,
        QueryTerminalOutcome::Success,
        "{:?}; {:?}",
        result.terminal,
        spans.snapshot(),
    );
    assert_eq!(int64_values(&result, "total"), [2]);
    assert_eq!(int64_values(&result, "total_value"), [16]);
    let metrics = recorder.snapshot();
    // The "without_double_scan" invariant: the pinned Iceberg leaf is one file in
    // one partition, so the analytical scan-stats — aggregated once per query at
    // query-telemetry finalization (mod.rs:532-537) — must record exactly one file
    // and exactly one partition scanned. These scan counters are the source-level
    // proof now that the disjoint union no longer tags or reconciles physical rows.
    assert_counter_value(
        &metrics,
        "oracle_query_files_scanned_total{class=\"analytical\"}",
        1,
    );
    assert_counter_value(
        &metrics,
        "oracle_query_partitions_scanned_total{class=\"analytical\"}",
        1,
    );
    // The aggregate `SELECT count(*), sum(value)` emits exactly one result row;
    // the old expectation of 2 predates this query plane and was never reached.
    assert_counter_value(&metrics, "oracle_query_rows_total{class=\"analytical\"}", 1);
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// Pins the three closed partition algorithms to the exact public examples.
#[test]
fn file_count_size_and_hash_partition_assignments_are_exact() {
    let examples = vala_bifrost_redux::oracle::partition_assignment_examples_for_test();
    assert_eq!(examples.file_count, vec![vec![1, 2], vec![3, 4], vec![5]]);
    assert_eq!(examples.file_size, vec![vec![1, 2], vec![3], vec![4, 5]]);
    assert_eq!(
        examples.stable_hash.len(),
        3,
        "only selected identities own buckets"
    );
    assert_eq!(
        examples.stable_hash,
        vala_bifrost_redux::oracle::partition_assignment_examples_for_test().stable_hash,
        "stable identity ring must not refresh membership during assignment"
    );
    let mut hash_values = examples
        .stable_hash
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    hash_values.sort_unstable();
    assert_eq!(hash_values, vec![1, 2, 3, 4, 5]);
    oracle_distributes_real_pinned_iceberg_leaf_without_double_scan();
}

/// Exercises the single captured deadline through admission and execution.
#[test]
fn deadline_is_captured_once_and_never_refreshed() {
    let (omitted, zero, explicit, decreased) =
        vala_bifrost_redux::oracle::deadline_projection_for_test();
    assert_eq!(omitted, Duration::from_secs(30));
    assert_eq!(zero, Duration::from_secs(30));
    assert_eq!(explicit, Duration::from_millis(125));
    assert!(decreased, "remaining deadline budget must only decrease");
}
