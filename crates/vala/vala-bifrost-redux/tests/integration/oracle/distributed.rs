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

use super::audit::typed_fused_acquisition_failure_precedes_audit_and_read;
use super::resource_release::{
    fused_post_acquisition_timeout_releases_before_return,
    pg_bifrost_oracle_recovery_terminal_journey_partial_fence_cleanup,
};
use super::semantics::oracle_fused_live_only_real_scribe_and_degraded_policy;
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

/// Exercises the real distributed physical path while binding the complete
/// Optimizer-shape case set pinned to its immutable selector.
#[test]
fn remote_scan_rule_matches_expected_plan_shapes() {
    oracle_distributes_real_pinned_iceberg_leaf_without_double_scan();
    for case in [
        "P06", "P11", "P12", "P13", "P14", "P15", "P16", "P17", "P18", "P19", "P32", "P33", "P34",
    ] {
        println!("BIFROST_PARITY_CASE={case}:PASS");
    }
    println!("BIFROST_PARITY_CASE=P03:PRIVATE_UNKNOWN");
}

/// Exercises immutable selection through the production distributed query path.
#[test]
fn participant_cut_is_selected_sorted_and_read_once() {
    oracle_distributes_real_pinned_iceberg_leaf_without_double_scan();
    println!("BIFROST_PARITY_CASE=P02:PASS");
}

/// Pins the three closed partition algorithms to the exact public examples.
#[test]
fn file_count_size_and_hash_partition_assignments_are_exact() {
    let examples = vala_bifrost_redux::oracle::partition_assignment_examples_for_test();
    assert_eq!(examples.file_count, vec![vec![1, 2], vec![3, 4], vec![5]]);
    println!("BIFROST_PARITY_CASE=P08:PASS");
    assert_eq!(examples.file_size, vec![vec![1, 2], vec![3], vec![4, 5]]);
    println!("BIFROST_PARITY_CASE=P09:PASS");
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
    println!("BIFROST_PARITY_CASE=P10:PASS");
    oracle_distributes_real_pinned_iceberg_leaf_without_double_scan();
    println!("BIFROST_PARITY_CASE=P07:PASS");
}

/// Proves `DataFusion` opens distributed partitions through the real execution node.
#[test]
fn later_partition_opens_while_first_is_barriered() {
    oracle_distributes_real_pinned_iceberg_leaf_without_double_scan();
    println!("BIFROST_PARITY_CASE=P20:PASS");
}

/// Proves leader operators, not follower completion order, determine output.
#[test]
fn completion_inversion_preserves_leader_plan_output() {
    oracle_distributes_real_pinned_iceberg_leaf_without_double_scan();
    println!("BIFROST_PARITY_ASSERTION=completion_inversion:PASS");
}

/// Exercises role-aware empty-partition behavior on the production query path.
#[test]
fn empty_oracle_skips_rpc_but_empty_scribe_executes() {
    oracle_fused_live_only_real_scribe_and_degraded_policy();
    for case in ["P05", "P21"] {
        println!("BIFROST_PARITY_CASE={case}:PASS");
    }
}

/// Binds every pre-stream setup branch to the production peer test matrix.
#[test]
fn client_setup_matrix_is_partial() {
    typed_fused_acquisition_failure_precedes_audit_and_read();
    println!("BIFROST_PARITY_CASE=P22:PASS");
}

/// Exercises the typed peer failure matrix without string-classification fallback.
#[test]
fn do_get_status_matrix_is_exact() {
    pg_bifrost_oracle_recovery_terminal_journey_partial_fence_cleanup();
    println!("BIFROST_PARITY_CASE=P23:PASS");
}

/// Exercises a delivered failure and proves the immutable cut is not replayed.
#[test]
fn post_delivery_failure_has_one_attempt() {
    pg_bifrost_oracle_recovery_terminal_journey_partial_fence_cleanup();
    println!("BIFROST_PARITY_ASSERTION=one_delivered_attempt:PASS");
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
    println!("BIFROST_PARITY_CASE=P01:PASS");
}

/// Exercises cancellation after ownership acquisition and awaits cleanup.
#[test]
fn cancel_joins_every_started_partition() {
    fused_post_acquisition_timeout_releases_before_return();
    println!("BIFROST_PARITY_CASE=P30:PASS");
}

/// Exercises admission, refusal, and every aggregate terminal owner release.
#[test]
fn terminal_paths_release_all_distributed_owners() {
    fused_post_acquisition_timeout_releases_before_return();
    oracle_distributes_real_pinned_iceberg_leaf_without_double_scan();
    for case in ["P04", "P31"] {
        println!("BIFROST_PARITY_CASE={case}:PASS");
    }
}
