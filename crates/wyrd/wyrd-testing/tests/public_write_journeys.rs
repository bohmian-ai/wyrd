//! Primary public Bifrost write journey.
//!
//! This is deliberately an ignored e2e target. The canonical journey lane
//! selects it explicitly with Postgres and a real three-pod server topology;
//! the fast family lanes do not claim coverage from this test.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use arrow::array::{FixedSizeBinaryArray, Int32Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::forge::ForgeLifecycleEvent;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::ScribePublicationEvent;
use vala_sdk::{
    BifrostFrame, BifrostGrpcTransport, CollectedQueryLimits, CollectedQueryResult,
    IngestTransport, QueryClient,
};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_server::config::ForgeProcessRole;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, QueryTerminalOutcome, VisibilityMode,
};
use wyrd_testing::Bootstrap;
use wyrd_testing::bifrost::{
    BifrostClusterSpec, BifrostTopology, WyrdTestCluster, full_bifrost_topology,
};

const TABLE_NAME: &str = "closeout_events";
const TABLE_FQN: &str = "vala.bifrost.closeout_events";
/// Exact Prometheus rendering of the production Bifrost duration buckets.
const BIFROST_BUCKET_LABELS: &[&str] = &[
    "0.001", "0.005", "0.01", "0.025", "0.05", "0.1", "0.25", "0.5", "1", "2.5", "5", "10", "30",
    "60", "120", "300", "600", "1800", "+Inf",
];
/// Private source-derived label contract used by the RR9 completeness gate.
type MetricLabelContract = (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
);
/// Exact base contracts for every family identified by the RR9 omission audit.
const RR9_BASE_CONTRACTS: &[MetricLabelContract] = &[
    ("bifrost_gate_active_streams", &[("operation", &["query"])]),
    (
        "bifrost_gate_query_stream_duration_seconds",
        &[("outcome", &["success", "rejected", "failed", "cancelled"])],
    ),
    ("bifrost_scribe_ingress_active", &[]),
    (
        "bifrost_oracle_admission_rejections_total",
        &[
            ("scope", &["cluster", "class", "tenant"]),
            (
                "reason",
                &["pending_limit", "lease_capacity", "local_slots"],
            ),
            ("query_class", &["interactive", "analytical"]),
        ],
    ),
    (
        "bifrost_oracle_source_operation_seconds",
        &[
            ("source", &["iceberg", "hot_sealed", "live_tail"]),
            ("outcome", &["success", "failed", "cancelled"]),
        ],
    ),
];

#[tokio::test]
#[ignore = "requires the real three-pod Bifrost journey lane"]
async fn public_write_read_journey() {
    let cluster = WyrdTestCluster::start(3, full_bifrost_topology())
        .await
        .expect("three-pod WyrdTestCluster");
    let result = run_closeout_journey(&cluster).await;
    let shutdown = cluster.shutdown().await;
    shutdown.expect("three-pod cluster shutdown");
    result.expect("public Bifrost closeout journey");
}

/// Proves the public write/read contract on a production-configured Forge pod.
#[tokio::test]
#[ignore = "requires the real Postgres-backed Bifrost journey lane"]
async fn public_write_compact_read_journey() {
    run_topology_public_journey(BifrostTopology::OnePod, true)
        .await
        .expect("public write/compact/read journey");
}

/// Proves the production recorder exposes coexisting owner families after public work.
#[tokio::test]
#[ignore = "requires the real Postgres-backed Bifrost journey lane"]
async fn production_recorder_exposes_bifrost_owner_families() {
    let cluster = WyrdTestCluster::start(1, BifrostTopology::OnePod)
        .await
        .expect("one-pod telemetry cluster");
    run_closeout_journey(&cluster)
        .await
        .expect("public write and strict read");
    let rendered = cluster.telemetry().render();
    for (family, labels) in [
        (
            "bifrost_gate_requests_total",
            &["operation=\"write\"", "outcome=\"success\""][..],
        ),
        (
            "bifrost_gate_requests_total",
            &["operation=\"query\"", "outcome=\"success\""][..],
        ),
        (
            "bifrost_scribe_wal_append_total",
            &["outcome=\"success\""][..],
        ),
        (
            "bifrost_scribe_wal_fsync_total",
            &["outcome=\"success\""][..],
        ),
        ("bifrost_oracle_queries_total", &["outcome=\"success\""][..]),
        ("wyrd_postgres_pool_acquire_total", &["pool=\"app\""][..]),
        (
            "vala_postgres_pool_acquire_total",
            &["pool=\"runtime\""][..],
        ),
    ] {
        assert!(
            prometheus_sample(&rendered, family, labels).is_some_and(|value| value >= 1.0),
            "missing positive production series {family} with {labels:?}"
        );
    }
    assert!(
        prometheus_family_sum(&rendered, "wyrd_storage_bytes_total") > 0.0,
        "storage telemetry must report actual completed bytes"
    );
    for (family, labels) in [
        ("bifrost_scribe_lane_queued", &["lane=\"ingress\""][..]),
        ("bifrost_scribe_lane_active", &["lane=\"ingress\""][..]),
        ("bifrost_scribe_lane_queued", &["lane=\"persistence\""][..]),
        ("bifrost_scribe_lane_active", &["lane=\"persistence\""][..]),
        ("bifrost_scribe_lane_queued", &["lane=\"wal_io\""][..]),
        ("bifrost_scribe_lane_active", &["lane=\"wal_io\""][..]),
        ("bifrost_scribe_persistence_queue_depth", &[][..]),
        ("bifrost_scribe_persistence_queue_bytes", &[][..]),
        (
            "bifrost_oracle_in_flight",
            &[
                "query_class=\"interactive\"",
                "visibility=\"published_only\"",
            ][..],
        ),
        (
            "bifrost_oracle_in_flight",
            &["query_class=\"interactive\"", "visibility=\"fused\""][..],
        ),
        (
            "bifrost_oracle_in_flight",
            &[
                "query_class=\"analytical\"",
                "visibility=\"published_only\"",
            ][..],
        ),
        (
            "bifrost_oracle_in_flight",
            &["query_class=\"analytical\"", "visibility=\"fused\""][..],
        ),
        (
            "bifrost_oracle_slots_in_use",
            &["query_class=\"interactive\"", "role=\"leader\""][..],
        ),
        (
            "bifrost_oracle_slots_in_use",
            &["query_class=\"analytical\"", "role=\"leader\""][..],
        ),
    ] {
        assert_eq!(
            prometheus_sample(&rendered, family, labels),
            Some(0.0),
            "idle production series must be present and zero: {family} with {labels:?}"
        );
    }
    for family in [
        "bifrost_gate_active_requests",
        "bifrost_gate_active_streams",
        "bifrost_scribe_ingress_active",
        "bifrost_scribe_lane_queued",
        "bifrost_scribe_lane_active",
        "bifrost_scribe_persistence_queue_depth",
        "bifrost_scribe_persistence_queue_bytes",
        "bifrost_oracle_in_flight",
        "bifrost_oracle_slots_in_use",
        "wyrd_storage_operations_active",
    ] {
        assert_eq!(
            prometheus_family_sum(&rendered, family),
            0.0,
            "active family must drain after the journey: {family}"
        );
    }
    for family in [
        "bifrost_gate_request_duration_seconds",
        "bifrost_scribe_wal_append_seconds",
        "bifrost_scribe_wal_fsync_seconds",
        "bifrost_oracle_query_duration_seconds",
        "wyrd_postgres_pool_acquire_seconds",
        "vala_postgres_pool_acquire_seconds",
        "wyrd_storage_operation_duration_seconds",
    ] {
        for suffix in ["_bucket", "_count", "_sum"] {
            assert!(
                rendered
                    .lines()
                    .any(|line| line.starts_with(&format!("{family}{suffix}"))),
                "missing histogram series {family}{suffix}"
            );
        }
    }
    for (family, labels) in [
        (
            "bifrost_gate_request_duration_seconds",
            &[
                ("operation", &["write", "query"][..]),
                (
                    "outcome",
                    &["success", "rejected", "failed", "cancelled"][..],
                ),
            ][..],
        ),
        (
            "bifrost_scribe_wal_append_seconds",
            &[("outcome", &["success", "failed"][..])][..],
        ),
        (
            "bifrost_scribe_wal_fsync_seconds",
            &[("outcome", &["success", "failed"][..])][..],
        ),
        (
            "bifrost_oracle_query_duration_seconds",
            &[
                (
                    "outcome",
                    &["success", "rejected", "failed", "cancelled"][..],
                ),
                ("query_class", &["interactive", "analytical"][..]),
                ("visibility", &["published_only", "fused"][..]),
            ][..],
        ),
        (
            "wyrd_postgres_pool_acquire_seconds",
            &[
                ("outcome", &["success", "failed", "cancelled"][..]),
                ("pool", &["app"][..]),
            ][..],
        ),
        (
            "vala_postgres_pool_acquire_seconds",
            &[
                ("outcome", &["success", "failed", "cancelled"][..]),
                ("pool", &["runtime"][..]),
            ][..],
        ),
        (
            "wyrd_storage_operation_duration_seconds",
            &[
                ("backend", &["local", "s3", "gcs", "azure"][..]),
                ("operation", &["get", "put", "list", "delete", "head"][..]),
                ("outcome", &["success", "failed", "cancelled"][..]),
            ][..],
        ),
    ] {
        let mut bucket_labels = labels.to_vec();
        bucket_labels.push(("le", BIFROST_BUCKET_LABELS));
        assert_exact_label_contract(&rendered, &format!("{family}_bucket"), &bucket_labels);
        assert_exact_label_contract(&rendered, &format!("{family}_count"), labels);
        assert_exact_label_contract(&rendered, &format!("{family}_sum"), labels);
    }
    for (family, allowed) in [
        (
            "bifrost_gate_requests_total",
            &[
                ("operation", &["write", "query"][..]),
                (
                    "outcome",
                    &["success", "rejected", "failed", "cancelled"][..],
                ),
            ][..],
        ),
        (
            "bifrost_gate_rejections_total",
            &[
                ("operation", &["write", "query"][..]),
                (
                    "reason",
                    &[
                        "auth",
                        "permission",
                        "validation",
                        "payload_limit",
                        "catalog",
                        "role_unavailable",
                        "scribe_admission",
                        "oracle_admission",
                    ][..],
                ),
            ][..],
        ),
        (
            "bifrost_gate_active_requests",
            &[("operation", &["write", "query"][..])][..],
        ),
        (
            "bifrost_gate_query_streams_total",
            &[("outcome", &["success", "failed", "cancelled"][..])][..],
        ),
        (
            "bifrost_scribe_rejections_total",
            &[(
                "reason",
                &["in_flight", "memory", "wal", "queue", "closed", "invalid"][..],
            )][..],
        ),
        (
            "bifrost_scribe_wal_append_total",
            &[("outcome", &["success", "failed"][..])][..],
        ),
        (
            "bifrost_scribe_wal_fsync_total",
            &[("outcome", &["success", "failed"][..])][..],
        ),
        ("bifrost_scribe_wal_append_bytes_total", &[][..]),
        ("bifrost_scribe_wal_disk_bytes", &[][..]),
        (
            "bifrost_oracle_queries_total",
            &[
                (
                    "outcome",
                    &["success", "rejected", "failed", "cancelled"][..],
                ),
                ("query_class", &["interactive", "analytical"][..]),
                ("visibility", &["published_only", "fused"][..]),
            ][..],
        ),
        (
            "wyrd_postgres_pool_acquire_total",
            &[("pool", &["app"][..])][..],
        ),
        (
            "vala_postgres_pool_acquire_total",
            &[("pool", &["runtime"][..])][..],
        ),
        ("wyrd_postgres_pool_size", &[("pool", &["app"][..])][..]),
        ("wyrd_postgres_pool_idle", &[("pool", &["app"][..])][..]),
        ("vala_postgres_pool_size", &[("pool", &["runtime"][..])][..]),
        ("vala_postgres_pool_idle", &[("pool", &["runtime"][..])][..]),
        (
            "wyrd_storage_operations_total",
            &[
                ("backend", &["local", "s3", "gcs", "azure"][..]),
                ("operation", &["get", "put", "list", "delete", "head"][..]),
                ("outcome", &["success", "failed", "cancelled"][..]),
            ][..],
        ),
        (
            "wyrd_storage_operations_active",
            &[
                ("backend", &["local", "s3", "gcs", "azure"][..]),
                ("operation", &["get", "put", "list", "delete", "head"][..]),
            ][..],
        ),
        (
            "wyrd_storage_bytes_total",
            &[
                ("backend", &["local", "s3", "gcs", "azure"][..]),
                ("direction", &["read", "write"][..]),
                ("operation", &["get", "put", "list", "delete", "head"][..]),
            ][..],
        ),
    ] {
        assert_exact_label_contract(&rendered, family, allowed);
    }
    for (family, labels) in RR9_BASE_CONTRACTS {
        if !family.ends_with("_seconds") {
            assert_exact_label_contract(&rendered, family, labels);
        }
    }
    for family in [
        "bifrost_gate_query_stream_duration_seconds",
        "bifrost_oracle_source_operation_seconds",
    ] {
        let labels = RR9_BASE_CONTRACTS
            .iter()
            .find_map(|(candidate, labels)| (*candidate == family).then_some(*labels))
            .expect("RR9 histogram base contract");
        let mut bucket_labels = labels.to_vec();
        bucket_labels.push(("le", BIFROST_BUCKET_LABELS));
        assert_exact_label_contract(&rendered, &format!("{family}_bucket"), &bucket_labels);
        assert_exact_label_contract(&rendered, &format!("{family}_count"), labels);
        assert_exact_label_contract(&rendered, &format!("{family}_sum"), labels);
    }
    cluster
        .shutdown()
        .await
        .expect("telemetry cluster shutdown");
}

/// Parse one exact Prometheus sample with label matching independent of label order.
fn prometheus_sample(rendered: &str, family: &str, labels: &[&str]) -> Option<f64> {
    rendered.lines().find_map(|line| {
        let (series, value) = line.split_once(' ')?;
        (series == family || series.starts_with(&format!("{family}{{")))
            .then_some(())
            .filter(|()| labels.iter().all(|label| series.contains(label)))?;
        value.parse().ok()
    })
}

/// Sum all exact Prometheus samples in one family while excluding metadata.
fn prometheus_family_sum(rendered: &str, family: &str) -> f64 {
    rendered
        .lines()
        .filter_map(|line| {
            let (series, value) = line.split_once(' ')?;
            (series == family || series.starts_with(&format!("{family}{{")))
                .then(|| value.parse::<f64>().ok())?
        })
        .sum()
}

/// Assert every rendered sample has exactly the declared bounded label contract.
fn assert_exact_label_contract(rendered: &str, family: &str, allowed: &[(&str, &[&str])]) {
    validate_exact_label_contract(rendered, family, allowed)
        .unwrap_or_else(|error| panic!("{error}"));
}

/// Validate exact label keys and closed values for one rendered family.
fn validate_exact_label_contract(
    rendered: &str,
    family: &str,
    allowed: &[(&str, &[&str])],
) -> Result<(), String> {
    let mut observed = false;
    for line in rendered.lines() {
        let Some((series, _)) = line.split_once(' ') else {
            continue;
        };
        if series != family && !series.starts_with(&format!("{family}{{")) {
            continue;
        }
        observed = true;
        let labels = parse_prometheus_labels(series)?;
        let expected = allowed.iter().map(|(key, _)| *key).collect::<BTreeSet<_>>();
        let actual = labels.keys().map(String::as_str).collect::<BTreeSet<_>>();
        if actual != expected {
            return Err(format!("unexpected label keys for {family}: {series}"));
        }
        for (key, values) in allowed {
            let value = labels
                .get(*key)
                .ok_or_else(|| format!("missing {key} for {family}"))?;
            if !values.contains(&value.as_str()) {
                return Err(format!("unexpected {family} label {key}={value}"));
            }
        }
    }
    observed
        .then_some(())
        .ok_or_else(|| format!("missing exact label-set family {family}"))
}

/// Parse one Prometheus series label set, including quoted escape sequences.
fn parse_prometheus_labels(series: &str) -> Result<BTreeMap<String, String>, String> {
    let Some((_, encoded)) = series.split_once('{') else {
        return Ok(BTreeMap::new());
    };
    let encoded = encoded
        .strip_suffix('}')
        .ok_or_else(|| "unterminated labels".to_owned())?;
    let mut labels = BTreeMap::new();
    for label in encoded.split(',') {
        let (key, quoted) = label
            .split_once('=')
            .ok_or_else(|| "invalid label".to_owned())?;
        let value = quoted
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .ok_or_else(|| "unquoted label".to_owned())?
            .replace("\\n", "\n")
            .replace("\\\"", "\"")
            .replace("\\\\", "\\");
        labels.insert(key.to_owned(), value);
    }
    Ok(labels)
}

/// Exact label validation rejects both undeclared keys and undeclared values.
#[test]
fn exact_label_contract_rejects_unexpected_keys_and_values() {
    let allowed = &[("outcome", &["success", "failed"][..])];
    assert!(
        validate_exact_label_contract("family{outcome=\"success\"} 1", "family", allowed).is_ok()
    );
    assert!(
        validate_exact_label_contract("family{outcome=\"other\"} 1", "family", allowed).is_err()
    );
    assert!(
        validate_exact_label_contract(
            "family{outcome=\"success\",tenant=\"x\"} 1",
            "family",
            allowed
        )
        .is_err()
    );
}

/// The RR9 contract table cannot silently omit any audited production family.
#[test]
fn rr9_label_contract_table_is_complete() {
    let actual = RR9_BASE_CONTRACTS
        .iter()
        .map(|(family, _)| *family)
        .collect::<BTreeSet<_>>();
    let expected = [
        "bifrost_gate_active_streams",
        "bifrost_gate_query_stream_duration_seconds",
        "bifrost_scribe_ingress_active",
        "bifrost_oracle_admission_rejections_total",
        "bifrost_oracle_source_operation_seconds",
    ]
    .into_iter()
    .collect();
    assert_eq!(actual, expected);
}

/// Proves that the public journey remains tenant-bound across compaction setup.
#[tokio::test]
#[ignore = "requires the real Postgres-backed Bifrost journey lane"]
async fn public_multitenant_write_compact_read_journey() {
    run_multitenant_public_journey()
        .await
        .expect("public multitenant write/compact/read journey");
}

/// Proves public writes and reads on the exact role-separated six-node matrix.
#[tokio::test]
#[ignore = "requires the real Postgres-backed Bifrost journey lane"]
async fn distributed_write_compact_read_journey() {
    run_topology_public_journey(BifrostTopology::ThreeServersThreeForgeWorkers, true)
        .await
        .expect("distributed write/compact/read journey");
}

/// Proves acknowledged rows survive a node replacement before the final read.
#[tokio::test]
#[ignore = "requires the real Postgres-backed Bifrost journey lane"]
async fn public_ack_restart_read_exact_once_journey() {
    let mut cluster =
        WyrdTestCluster::start_spec_with_forge_completion_observer(BifrostClusterSpec::one_mixed())
            .await
            .expect("restart journey cluster");
    let server = cluster.server(0).expect("restart journey server");
    let tenant = cluster.data_tenant_id();
    server
        .state()
        .bifrost_redux
        .as_ref()
        .expect("restart Redux catalog")
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, TABLE_NAME),
            user_fields: vec![
                Field::new("id", DataType::Int64, false),
                Field::new("value", DataType::Utf8, false),
            ],
            tenant,
            audit: None,
        })
        .await
        .expect("restart table");
    let writer = bootstrap_transport(server, "restart-writer", &["admin"])
        .await
        .expect("restart writer");
    writer
        .send_frame(frame(uuid::Uuid::now_v7().into_bytes(), &[707]))
        .await
        .expect("durable restart ACK");
    let unpublished: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(row_count), 0)::bigint FROM vala.file_list
         WHERE data_tenant_id = $1 AND namespace = 'vala.bifrost' AND table_name = $2",
    )
    .bind(tenant.as_uuid())
    .bind(TABLE_NAME)
    .fetch_one(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("restart pre-publication inspection");
    assert_eq!(unpublished, 0, "ACK precedes file-list publication");
    let node = cluster
        .configured_node_ids()
        .first()
        .copied()
        .expect("restart journey node");
    let roots = cluster
        .terminate_node_abruptly_for_test(node)
        .await
        .expect("abruptly terminate restart node");
    let wal_sizes = roots
        .wal_root
        .as_deref()
        .map(wal_file_sizes)
        .expect("restart retains a WAL root");
    assert!(
        wal_sizes.iter().any(|&size| size > 64),
        "acknowledged WAL must retain at least one complete record: {wal_sizes:?}"
    );
    let evidence = cluster
        .restart_terminated_node_at_new_address(node, roots.clone())
        .await
        .expect("restart node at a new address");
    assert_eq!(evidence.node_id, node);
    assert_ne!(evidence.http_addr, roots.previous_http_addr);
    assert_ne!(evidence.grpc_addr, roots.previous_grpc_addr);
    assert!(
        evidence
            .writer_epoch
            .zip(evidence.previous_writer_epoch)
            .is_some_and(|(new, old)| new > old),
        "restart must advance the production writer fence"
    );
    let replacement = cluster.server(0).expect("replacement restart server");
    replacement
        .flush_bifrost()
        .await
        .expect("replay and publish WAL");
    let reader = bootstrap_client(replacement, "restart-reader", &["admin"])
        .await
        .expect("restart reader");
    let query = QueryClient::new(&reader)
        .collect_bounded(
            &closeout_query(),
            CollectedQueryLimits {
                max_rows: 16,
                max_encoded_bytes: 1024 * 1024,
            },
        )
        .await
        .expect("restart exact-once read");
    assert_query_result(&query, &[707]);
    let source_epoch = roots
        .previous_writer_epoch
        .expect("acknowledged source epoch is retained");
    let recovered_identity: (uuid::Uuid, i64, i64) = sqlx::query_as(
        "SELECT node_id, writer_epoch, COUNT(*)::bigint FROM vala.file_list
         WHERE data_tenant_id=$1 AND namespace='vala.bifrost' AND table_name=$2
         GROUP BY node_id, writer_epoch",
    )
    .bind(tenant.as_uuid())
    .bind(TABLE_NAME)
    .fetch_one(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("recovered source identity");
    assert_eq!(recovered_identity.0, node.as_uuid());
    assert_eq!(recovered_identity.1, source_epoch);
    assert_eq!(recovered_identity.2, 1);
    let writer = bootstrap_transport(replacement, "restart-convergence-writer", &["admin"])
        .await
        .expect("restart convergence writer");
    for value in [708, 709] {
        writer
            .send_frame(frame(uuid::Uuid::now_v7().into_bytes(), &[value]))
            .await
            .expect("post-restart Forge input ACK");
        replacement
            .flush_bifrost()
            .await
            .expect("post-restart Forge input publication");
    }
    replacement
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("advance restart Forge clock");
    cluster.request_forge_scheduler_pass_for_test();
    let observer = cluster
        .forge_completion_observer()
        .expect("restart Forge observer");
    let lifecycle = tokio::time::timeout(
        Duration::from_secs(30),
        observer.wait_for_lifecycle(|events| {
            events
                .iter()
                .any(|event| matches!(event, ForgeLifecycleEvent::Terminal { .. }))
        }),
    )
    .await
    .expect("restart Forge convergence timeout");
    assert_forge_event_sequence(&lifecycle, Some(tenant));
    let converged = QueryClient::new(&reader)
        .collect_bounded(
            &closeout_query(),
            CollectedQueryLimits {
                max_rows: 16,
                max_encoded_bytes: 1024 * 1024,
            },
        )
        .await
        .expect("restart post-Forge read");
    assert_query_result(&converged, &[707, 708, 709]);
    replacement
        .bifrost_scribe()
        .expect("replacement Scribe")
        .retire_committed_for_test(std::time::Instant::now() + Duration::from_secs(120))
        .await
        .expect("retire source WAL segment");
    let source_wal_dir = roots
        .wal_root
        .as_deref()
        .expect("retained WAL root")
        .join(node.as_uuid().simple().to_string())
        .join(source_epoch.to_string());
    assert!(
        wal_file_sizes(&source_wal_dir).is_empty(),
        "durably published source WAL epoch must retire"
    );
    assert_drained_shutdown(
        cluster
            .shutdown_and_inspect()
            .await
            .expect("restart journey shutdown"),
    );
}

/// Collects WAL file sizes below one retained node root for crash evidence.
fn wal_file_sizes(root: &std::path::Path) -> Vec<u64> {
    let mut sizes = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|value| value.to_str()) == Some("wal")
                && let Ok(metadata) = entry.metadata()
            {
                sizes.push(metadata.len());
            }
        }
    }
    sizes
}

/// Run the canonical public write/read assertions for a named real topology.
async fn run_topology_public_journey(
    topology: BifrostTopology,
    wait_for_forge: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let spec = match topology {
        BifrostTopology::OnePod => BifrostClusterSpec::one_mixed(),
        BifrostTopology::ThreeServersThreeForgeWorkers => {
            BifrostClusterSpec::three_servers_three_forge_workers()
        }
        _ => BifrostClusterSpec::one_mixed(),
    };
    let cluster = WyrdTestCluster::start_spec_with_forge_completion_observer(spec).await?;
    let mut server_node_ids = BTreeSet::new();
    let mut forge_worker_node_ids = BTreeSet::new();
    if topology == BifrostTopology::ThreeServersThreeForgeWorkers {
        let roles = cluster
            .servers()
            .map(wyrd_testing::WyrdTestServer::forge_process_role)
            .collect::<Vec<_>>();
        assert_eq!(roles[..3], [ForgeProcessRole::Server; 3]);
        assert_eq!(roles[3..], [ForgeProcessRole::ForgeWorker; 3]);
        assert!(
            cluster.servers()[..3]
                .iter()
                .all(|server| server.base_url().is_some() && server.grpc_url().is_some())
        );
        server_node_ids.extend(
            cluster.servers()[..3]
                .iter()
                .map(|server| server.node_id().as_uuid()),
        );
        forge_worker_node_ids.extend(
            cluster.servers()[3..]
                .iter()
                .map(|server| server.node_id().as_uuid()),
        );
        assert!(
            cluster.servers()[3..]
                .iter()
                .all(|server| server.base_url().is_none() && server.grpc_url().is_none())
        );
        assert!(
            cluster.servers()[..3]
                .iter()
                .all(|server| { server.forge_process_role() == ForgeProcessRole::Server })
        );
    }
    let scribe_observers = cluster
        .servers()
        .filter_map(wyrd_testing::WyrdTestServer::scribe_publication_observer)
        .collect::<Vec<_>>();
    let result = run_closeout_journey(&cluster).await;
    if result.is_ok() && wait_for_forge {
        let before_physical =
            physical_state(&cluster, cluster.data_tenant_id(), TABLE_NAME).await?;
        assert!(
            before_physical.files >= 2,
            "compaction journey requires multiple physical inputs"
        );
        for observer in &scribe_observers {
            tokio::time::timeout(
                Duration::from_secs(30),
                observer
                    .wait_for(|event| matches!(event, ScribePublicationEvent::Published { .. })),
            )
            .await?;
            let events = observer.events();
            assert_scribe_event_sequence(&events);
        }
        for server in cluster.servers() {
            server
                .forge_clock()
                .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))?;
        }
        cluster.request_forge_scheduler_pass_for_test();
        if let Some(observer) = cluster.forge_completion_observer() {
            let events = tokio::time::timeout(
                Duration::from_secs(30),
                observer.wait_for_lifecycle(|events| {
                    events
                        .iter()
                        .any(|event| matches!(event, ForgeLifecycleEvent::Planned { .. }))
                        && events
                            .iter()
                            .any(|event| matches!(event, ForgeLifecycleEvent::Claimed { .. }))
                        && events
                            .iter()
                            .any(|event| matches!(event, ForgeLifecycleEvent::Rewritten { .. }))
                        && events.iter().any(|event| {
                            matches!(event, ForgeLifecycleEvent::CatalogCommitted { .. })
                        })
                        && events
                            .iter()
                            .any(|event| matches!(event, ForgeLifecycleEvent::Terminal { .. }))
                }),
            )
            .await
            .map_err(|_| "Forge lifecycle observer timed out")?;
            let attribution = assert_forge_event_sequence(&events, None);
            if topology == BifrostTopology::ThreeServersThreeForgeWorkers {
                assert!(forge_worker_node_ids.contains(&attribution.claimed_worker));
                assert!(forge_worker_node_ids.contains(&attribution.terminal_worker));
                assert!(!server_node_ids.contains(&attribution.claimed_worker));
                assert!(!server_node_ids.contains(&attribution.terminal_worker));
            }
        }
        let after_physical = physical_state(&cluster, cluster.data_tenant_id(), TABLE_NAME).await?;
        assert_eq!(after_physical.rows, before_physical.rows);
        assert!(after_physical.compacted > before_physical.compacted);
        assert!(
            after_physical.files - after_physical.compacted
                < before_physical.files - before_physical.compacted
        );
        assert_eq!(after_physical.snapshot_count, 1);
        let reader_index = usize::from(topology == BifrostTopology::ThreeServersThreeForgeWorkers);
        let server = cluster
            .server(reader_index)
            .ok_or("missing post-Forge reader")?;
        let reader = bootstrap_client(server, "post-forge-reader", &["admin"]).await?;
        let post_forge = QueryClient::new(&reader)
            .collect_bounded(
                &closeout_query(),
                CollectedQueryLimits {
                    max_rows: 1_024,
                    max_encoded_bytes: 8 * 1024 * 1024,
                },
            )
            .await?;
        let scribe_count = cluster
            .servers()
            .filter(|server| server.bifrost_scribe().is_some())
            .count();
        let mut expected = vec![1, 2, 3, 4];
        expected.extend((0..scribe_count).map(|pod| 10 + i64::try_from(pod).unwrap_or(i64::MAX)));
        expected.push(99);
        assert_query_result(&post_forge, &expected);
        if let Some(observer) = cluster.forge_completion_observer() {
            let terminal_count = observer
                .lifecycle_events()
                .iter()
                .filter(|event| matches!(event, ForgeLifecycleEvent::Terminal { .. }))
                .count();
            for server in cluster
                .servers()
                .filter(|server| server.forge_process_role() != ForgeProcessRole::ForgeWorker)
            {
                let expected_passes = server.completed_forge_scheduler_passes_for_test() + 1;
                server.request_forge_scheduler_pass_for_test();
                tokio::time::timeout(
                    Duration::from_secs(30),
                    server.wait_for_forge_scheduler_passes_for_test(expected_passes),
                )
                .await?;
            }
            assert_eq!(
                physical_state(&cluster, cluster.data_tenant_id(), TABLE_NAME).await?,
                after_physical
            );
            assert_eq!(
                observer
                    .lifecycle_events()
                    .iter()
                    .filter(|event| matches!(event, ForgeLifecycleEvent::Terminal { .. }))
                    .count(),
                terminal_count,
                "a converged second scheduler pass must not execute another rewrite"
            );
        }
        if topology == BifrostTopology::ThreeServersThreeForgeWorkers {
            let server_a = cluster.server(0).ok_or("missing Server A")?;
            bootstrap_transport(server_a, "distributed-generation-two", &["admin"])
                .await?
                .send_frame(frame(uuid::Uuid::now_v7().into_bytes(), &[777]))
                .await?;
            server_a.flush_bifrost().await?;
            expected.push(777);
            expected.sort_unstable();
            let server_c = cluster.server(2).ok_or("missing Server C")?;
            let reader_c = bootstrap_client(server_c, "distributed-reader-c", &["admin"]).await?;
            let generation_two = QueryClient::new(&reader_c)
                .collect_bounded(
                    &closeout_query(),
                    CollectedQueryLimits {
                        max_rows: 1_024,
                        max_encoded_bytes: 8 * 1024 * 1024,
                    },
                )
                .await?;
            assert_query_result(&generation_two, &expected);

            let inspection = cluster.oracle_inspection().await?;
            assert_eq!(inspection.memberships.len(), 6);
            assert!(inspection.memberships.iter().all(|row| {
                row.ready
                    && row.fencing_token > 0
                    && cluster.servers()[..3]
                        .iter()
                        .any(|server| server.node_id() == row.node_id)
            }));
            assert_eq!(inspection.forge_active_claims, 0);
            assert_eq!(inspection.forge_active_attempts, 0);
            let role_metrics = cluster.telemetry().snapshot()?;
            for (node_ids, expected_role) in [
                (&server_node_ids, "server"),
                (&forge_worker_node_ids, "forge_worker"),
            ] {
                for node_id in node_ids {
                    let node_id = node_id.to_string();
                    assert!(
                        role_metrics.iter().any(|sample| {
                            sample.family == "bifrost_forge_role_node_started_total"
                                && sample.labels.get("role").map(String::as_str)
                                    == Some(expected_role)
                                && sample.labels.get("node_id").map(String::as_str)
                                    == Some(node_id.as_str())
                                && sample.value >= 1.0
                        }),
                        "node {node_id} lacks {expected_role} production role telemetry"
                    );
                }
            }
        }
        let reconciled = cluster.oracle_inspection().await?;
        let expected_reads = if topology == BifrostTopology::ThreeServersThreeForgeWorkers {
            6
        } else {
            3
        };
        assert_eq!(reconciled.read_audit_rows, expected_reads);
        assert!(reconciled.audit_rows >= reconciled.read_audit_rows);
        assert_eq!(reconciled.active_leases, 0);
        assert_eq!(reconciled.slots_in_use, 0);
        assert_eq!(reconciled.active_tail_fences, 0);
        for owner in ["scribe", "forge", "oracle"] {
            assert!(
                reconciled
                    .metric_families
                    .iter()
                    .any(|family| family.contains(owner)),
                "journey outcome requires {owner} telemetry"
            );
        }
    }
    assert_drained_shutdown(cluster.shutdown_and_inspect().await?);
    result
}

/// Correlate one public ACK through its exact immutable and published identities.
fn assert_scribe_event_sequence(events: &[ScribePublicationEvent]) {
    let (ack_index, batch_id) = events
        .iter()
        .enumerate()
        .find_map(|(index, event)| match event {
            ScribePublicationEvent::Acknowledged { batch_id, rows } if *rows > 0 => {
                Some((index, *batch_id))
            }
            _ => None,
        })
        .expect("positive Scribe acknowledgement");
    let (sealed_index, seal_id, file_list_row_id) = events
        .iter()
        .enumerate()
        .skip(ack_index + 1)
        .find_map(|(index, event)| match event {
            ScribePublicationEvent::Sealed {
                seal_id,
                file_list_row_id,
                batch_ids,
                wal_lsn_min,
                wal_lsn_max,
            } if batch_ids.contains(&batch_id) && wal_lsn_min <= wal_lsn_max => {
                Some((index, *seal_id, *file_list_row_id))
            }
            _ => None,
        })
        .expect("seal carrying acknowledged batch");
    let published = events
        .iter()
        .skip(sealed_index + 1)
        .any(|event| matches!(event, ScribePublicationEvent::Published {
            seal_id: observed_seal,
            file_list_row_id: observed_file,
            batch_ids,
            ..
        } if *observed_seal == seal_id && *observed_file == file_list_row_id && batch_ids.contains(&batch_id)));
    assert!(
        published,
        "the correlated seal must publish its exact file identity"
    );
}

/// Execute independent public writes and strict reads for two authenticated tenants.
async fn run_multitenant_public_journey() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let spec = BifrostClusterSpec::one_mixed();
    let cluster = WyrdTestCluster::start_spec_with_forge_completion_observer(spec).await?;
    let server = cluster.server(0).ok_or("missing multitenant server")?;
    let tenant_a = cluster.data_tenant_id();
    let tenant_b = cluster.add_tenant("public-compact-tenant-b").await?;
    let tenant_tables = [
        (tenant_a, "closeout_events_a", 101_i64),
        (tenant_b, "closeout_events_b", 202_i64),
    ];
    for (tenant, table_name, expected) in tenant_tables {
        let table_fqn = format!("vala.bifrost.{table_name}");
        server
            .state()
            .bifrost_redux
            .as_ref()
            .ok_or("missing Redux catalog")?
            .create_table(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, table_name),
                user_fields: vec![
                    Field::new("id", DataType::Int64, false),
                    Field::new("value", DataType::Utf8, false),
                ],
                tenant,
                audit: None,
            })
            .await?;
        let client = bootstrap_client_for_tenant(server, tenant, "public-tenant-writer").await?;
        let transport = client
            .connect_grpc()
            .await
            .map(|connection| BifrostGrpcTransport::new(connection, Default::default()))?;
        for value in [expected, expected + 1] {
            transport
                .send_frame(frame_for_table(
                    &table_fqn,
                    uuid::Uuid::now_v7().into_bytes(),
                    &[value],
                ))
                .await?;
            server.flush_bifrost_for_tenant(tenant).await?;
        }
        let query = QueryClient::new(&client)
            .collect_bounded(
                &query_for_table(&table_fqn),
                CollectedQueryLimits {
                    max_rows: 16,
                    max_encoded_bytes: 1024 * 1024,
                },
            )
            .await?;
        assert_query_result(&query, &[expected, expected + 1]);
    }
    let tenant_a_client =
        bootstrap_client_for_tenant(server, tenant_a, "wrong-tenant-reader").await?;
    let denied = match QueryClient::new(&tenant_a_client)
        .query(&query_for_table("vala.bifrost.closeout_events_b"))
        .await
    {
        Ok(_) => return Err("tenant A resolved tenant B's table".into()),
        Err(error) => error,
    };
    assert_eq!(denied.status(), 404);
    let mut before_physical = Vec::new();
    for (tenant, table_name, _) in tenant_tables {
        before_physical.push((tenant, physical_state(&cluster, tenant, table_name).await?));
    }
    for server in cluster.servers() {
        server
            .forge_clock()
            .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))?;
    }
    cluster.request_forge_scheduler_pass_for_test();
    if let Some(observer) = cluster.forge_completion_observer() {
        tokio::time::timeout(
            Duration::from_secs(30),
            observer.wait_for_lifecycle(|events| {
                [tenant_a, tenant_b].iter().all(|tenant| {
                    let task_id = events.iter().find_map(|event| match event {
                        ForgeLifecycleEvent::Planned {
                            task_id,
                            tenant: observed,
                            ..
                        } if observed == tenant => Some(*task_id),
                        _ => None,
                    });
                    task_id.is_some_and(|expected| {
                        events.iter().any(|event| {
                            matches!(event, ForgeLifecycleEvent::Terminal { task_id, .. } if *task_id == expected)
                        })
                    })
                })
            }),
        )
        .await?;
    }
    if let Some(observer) = cluster.forge_completion_observer() {
        let events = observer.lifecycle_events();
        for tenant in [tenant_a, tenant_b] {
            assert_forge_event_sequence(&events, Some(tenant));
        }
    }
    let server = cluster
        .server(0)
        .ok_or("missing multitenant post-Forge reader")?;
    for (tenant, table_name, expected) in tenant_tables {
        let reader = bootstrap_client_for_tenant(server, tenant, "public-tenant-reader").await?;
        let post_forge = QueryClient::new(&reader)
            .collect_bounded(
                &query_for_table(&format!("vala.bifrost.{table_name}")),
                CollectedQueryLimits {
                    max_rows: 16,
                    max_encoded_bytes: 1024 * 1024,
                },
            )
            .await?;
        assert_query_result(&post_forge, &[expected, expected + 1]);
        let before = before_physical
            .iter()
            .find_map(|(observed, state)| (*observed == tenant).then_some(*state))
            .ok_or("missing tenant physical baseline")?;
        let after = physical_state(&cluster, tenant, table_name).await?;
        assert_eq!(after.rows, before.rows);
        assert!(after.compacted > before.compacted);
        assert!(after.files - after.compacted < before.files - before.compacted);
        let mut conn =
            wyrd_sql::TenantConn::acquire(cluster.pg_fixture().app_pool(), tenant).await?;
        let forge_audits: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM vala.audit_outbox WHERE data_tenant_id = wyrd.current_tenant() AND operation LIKE 'forge.%'",
        )
        .fetch_one(&mut **conn.transaction())
        .await?;
        assert!(
            forge_audits > 0,
            "tenant must retain its own Forge audit trail"
        );
    }
    let inspection = cluster.oracle_inspection().await?;
    assert_eq!(inspection.forge_active_claims, 0);
    assert_eq!(inspection.forge_active_attempts, 0);
    assert!(inspection.audit_rows >= 6);
    assert!(
        inspection
            .metric_families
            .iter()
            .any(|family| family.contains("forge"))
    );
    assert_drained_shutdown(cluster.shutdown_and_inspect().await?);
    Ok(())
}

/// Assert every server, listener, and durable runtime owner drained at shutdown.
fn assert_drained_shutdown(inspection: wyrd_testing::bifrost::ClusterShutdownInspection) {
    assert!(inspection.servers_stopped);
    assert!(inspection.listeners_stopped);
    assert_eq!(inspection.scribe_queued, 0);
    assert_eq!(inspection.scribe_inflight, 0);
    assert_eq!(inspection.scribe_wal_streams, 0);
    assert_eq!(inspection.oracle_leases, 0);
    assert_eq!(inspection.oracle_slots, 0);
    assert_eq!(inspection.forge_active_claims, 0);
    assert_eq!(inspection.forge_active_attempts, 0);
    assert_eq!(inspection.supervised_tasks, 0);
}

/// Assert the durable production Forge lifecycle stages after observer completion.
fn assert_forge_event_sequence(
    events: &[ForgeLifecycleEvent],
    tenant: Option<DataTenantId>,
) -> ForgeWorkerAttribution {
    let (planned_index, task_id, inputs) = events
        .iter()
        .enumerate()
        .find_map(|(index, event)| match event {
            ForgeLifecycleEvent::Planned {
                task_id,
                tenant: observed,
                inputs,
                ..
            } if tenant.is_none_or(|expected| expected == *observed) => {
                Some((index, *task_id, inputs))
            }
            _ => None,
        })
        .expect("correlated Forge planned event");
    assert!(
        inputs.len() >= 2,
        "Forge plan must name every eligible input"
    );
    assert!(inputs.windows(2).all(|pair| pair[0] < pair[1]));

    let stage = |after: usize, predicate: &dyn Fn(&ForgeLifecycleEvent) -> bool| {
        events
            .iter()
            .enumerate()
            .skip(after + 1)
            .find_map(|(index, event)| predicate(event).then_some(index))
            .expect("correlated Forge lifecycle stage")
    };
    let claimed = stage(
        planned_index,
        &|event| matches!(event, ForgeLifecycleEvent::Claimed { task_id: observed, .. } if *observed == task_id),
    );
    let claimed_worker = match &events[claimed] {
        ForgeLifecycleEvent::Claimed { worker_id, .. } => *worker_id,
        _ => unreachable!("claimed index selected by typed predicate"),
    };
    let rewritten = stage(
        claimed,
        &|event| matches!(event, ForgeLifecycleEvent::Rewritten { task_id: observed, input_count } if *observed == task_id && *input_count == inputs.len()),
    );
    let committed = stage(
        rewritten,
        &|event| matches!(event, ForgeLifecycleEvent::CatalogCommitted { task_id: observed, snapshot_id } if *observed == task_id && *snapshot_id > 0),
    );
    let terminal = stage(
        committed,
        &|event| matches!(event, ForgeLifecycleEvent::Terminal { task_id: observed, worker_id } if *observed == task_id && *worker_id == claimed_worker),
    );
    assert!(terminal > committed);
    let terminal_worker = match &events[terminal] {
        ForgeLifecycleEvent::Terminal { worker_id, .. } => *worker_id,
        _ => unreachable!("terminal index selected by typed predicate"),
    };
    ForgeWorkerAttribution {
        claimed_worker,
        terminal_worker,
    }
}

/// Exact worker attribution returned by a correlated Forge lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ForgeWorkerAttribution {
    /// Durable owner that claimed the correlated task.
    claimed_worker: uuid::Uuid,
    /// Durable owner that completed the correlated task.
    terminal_worker: uuid::Uuid,
}

/// Durable physical state used only to corroborate public logical equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PhysicalState {
    /// Published Scribe input file count.
    files: i64,
    /// Input files marked replaced by Forge.
    compacted: i64,
    /// Logical rows retained across replacement.
    rows: i64,
    /// Distinct replacement snapshots committed by Forge.
    snapshot_count: i64,
}

/// Read exact file-list replacement evidence for one tenant-bound table.
///
/// # Errors
///
/// Returns a PostgreSQL error when the corroborating durable state is unavailable.
async fn physical_state(
    cluster: &WyrdTestCluster,
    tenant: wyrd_spec::DataTenantId,
    table_name: &str,
) -> Result<PhysicalState, sqlx::Error> {
    let row: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT count(*)::bigint,
                count(*) FILTER (WHERE compacted)::bigint,
                COALESCE(sum(row_count), 0)::bigint,
                count(DISTINCT committed_snapshot_id)::bigint
           FROM vala.file_list
          WHERE data_tenant_id = $1 AND namespace = 'vala.bifrost' AND table_name = $2",
    )
    .bind(tenant.as_uuid())
    .bind(table_name)
    .fetch_one(cluster.pg_fixture().operator_pool().pool())
    .await?;
    Ok(PhysicalState {
        files: row.0,
        compacted: row.1,
        rows: row.2,
        snapshot_count: row.3,
    })
}

#[tokio::test]
#[ignore = "requires the real Postgres-backed Bifrost journey lane"]
async fn public_write_read_journey_delayed_fsync_drains_without_loss() {
    let cluster = WyrdTestCluster::start_with_wal_sync_delay(
        1,
        BifrostTopology::OnePod,
        Duration::from_millis(10),
    )
    .await
    .expect("one-pod delayed-fsync WyrdTestCluster");
    let result = run_delayed_fsync_journey(&cluster).await;
    let shutdown = cluster.shutdown().await;
    shutdown.expect("delayed-fsync cluster shutdown");
    result.expect("delayed-fsync Bifrost closeout journey");
}

#[tokio::test]
#[ignore = "requires the real Postgres-backed Bifrost journey lane"]
async fn fresh_boot_provisions_redux_before_first_write() {
    let cluster = WyrdTestCluster::start(1, BifrostTopology::OnePod)
        .await
        .expect("fresh WyrdTestCluster boot");
    let server = cluster.server(0).expect("booted Bifrost server");
    let tenant = cluster.data_tenant_id();
    let redux = server
        .state()
        .bifrost_redux
        .as_ref()
        .expect("server boot provisions Redux catalog");
    redux
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, TABLE_NAME),
            user_fields: vec![
                Field::new("id", DataType::Int64, false),
                Field::new("value", DataType::Utf8, false),
            ],
            tenant,
            audit: None,
        })
        .await
        .expect("first Redux table resolution after boot");
    let transport = bootstrap_transport(server, "fresh-boot-writer", &["admin"])
        .await
        .expect("first writer after boot");
    transport
        .insert_batch(TABLE_FQN, uuid::Uuid::now_v7().into_bytes(), ipc(&[1]))
        .await
        .expect("first Redux write after boot");
    server.flush_bifrost().await.expect("first write flush");
    let rows: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(row_count), 0)::bigint FROM vala.file_list
         WHERE data_tenant_id = $1 AND namespace = 'vala.bifrost' AND table_name = $2",
    )
    .bind(tenant.as_uuid())
    .bind(TABLE_NAME)
    .fetch_one(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("first Redux file-list read");
    assert_eq!(rows, 1);
    cluster.shutdown().await.expect("cluster shutdown");
}

/// Exercises the complete multi-pod ingest, rejection, flush, query, and audit journey.
///
/// Every accepted write is flushed before each independently authenticated
/// Oracle entrypoint must return the same exact ordered rows and terminal
/// metadata. Returning early leaves already acknowledged and flushed test data
/// durable until the caller shuts down the cluster.
///
/// # Errors
///
/// Returns an error when table creation, client bootstrap, ingest, flush,
/// query collection, SQL inspection, or tenant connection acquisition fails.
///
/// # Cancellation
///
/// Cancelling this future stops the current client or SQL operation. Accepted
/// writes and completed flushes are not rolled back; the caller retains cluster
/// ownership and is responsible for bounded shutdown.
async fn run_closeout_journey(
    cluster: &WyrdTestCluster,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tenant = cluster.data_tenant_id();
    let first = cluster.server(0).ok_or("missing first Bifrost pod")?;
    first
        .state()
        .bifrost_redux
        .as_ref()
        .ok_or("missing Redux catalog")?
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, TABLE_NAME),
            user_fields: vec![
                Field::new("id", DataType::Int64, false),
                Field::new("value", DataType::Utf8, false),
            ],
            tenant,
            audit: None,
        })
        .await?;

    let admin = bootstrap_transport(first, "closeout-admin", &["admin"]).await?;
    let scribe_count = cluster
        .servers()
        .filter(|server| server.bifrost_scribe().is_some())
        .count();
    let mut expected_ids = vec![1, 2, 3, 4];
    expected_ids.extend((0..scribe_count).map(|pod| 10 + i64::try_from(pod).unwrap_or(i64::MAX)));
    expected_ids.push(99);
    let batch_id = uuid::Uuid::now_v7().into_bytes();
    admin.send_frame(frame(batch_id, &[1, 2])).await?;
    admin
        .send_frame(frame(uuid::Uuid::now_v7().into_bytes(), &[3, 4]))
        .await?;

    // Exercise symmetric traffic: every independently bound server receives a
    // frame through its own public SDK connection and server-owned Gate.
    for (pod, server) in cluster
        .servers()
        .filter(|server| server.bifrost_scribe().is_some())
        .enumerate()
    {
        let transport =
            bootstrap_transport(server, &format!("closeout-pod-{pod}"), &["admin"]).await?;
        let id = uuid::Uuid::now_v7().into_bytes();
        transport
            .insert_batch(TABLE_FQN, id, ipc(&[10 + pod as i64]))
            .await?;
        server.flush_bifrost().await?;
    }

    // A retry of an already admitted frame is idempotent at the durable slice
    // boundary. The duplicate is intentionally flushed after the first frame.
    let retry_id = uuid::Uuid::now_v7().into_bytes();
    let retry_frame = frame(retry_id, &[99]);
    admin
        .insert_batch(
            TABLE_FQN,
            retry_frame.batch_id,
            retry_frame.arrow_ipc.to_vec(),
        )
        .await?;
    admin
        .insert_batch(
            TABLE_FQN,
            retry_frame.batch_id,
            retry_frame.arrow_ipc.to_vec(),
        )
        .await?;
    for server in cluster
        .servers()
        .filter(|server| server.bifrost_scribe().is_some())
    {
        server.flush_bifrost().await?;
    }

    // Negative auth and resolution paths are exercised through the same public
    // SDK transport. Neither request is allowed to create durable file-list
    // state because Gate rejects before Scribe admission.
    let underprivileged = bootstrap_transport(first, "closeout-underprivileged", &[]).await?;
    let denied = underprivileged
        .insert_batch(TABLE_FQN, uuid::Uuid::now_v7().into_bytes(), ipc(&[200]))
        .await
        .expect_err("under-privileged token must be rejected");
    assert_eq!(denied.status(), 403);

    let unknown = admin
        .insert_batch(
            "vala.bifrost.unknown_closeout_table",
            uuid::Uuid::now_v7().into_bytes(),
            ipc(&[201]),
        )
        .await
        .expect_err("unknown table must be rejected before Scribe");
    assert_eq!(unknown.status(), 404);

    let conflict = admin
        .insert_batch(
            TABLE_FQN,
            uuid::Uuid::now_v7().into_bytes(),
            conflicting_ipc(),
        )
        .await
        .expect_err("schema conflict must be rejected before ACK");
    assert_eq!(conflict.status(), 409);

    for (pod, server) in cluster
        .servers()
        .filter(|server| server.base_url().is_some())
        .enumerate()
    {
        let query_client =
            bootstrap_client(server, &format!("closeout-query-{pod}"), &["admin"]).await?;
        let query = QueryClient::new(&query_client)
            .collect_bounded(
                &closeout_query(),
                CollectedQueryLimits {
                    max_rows: 1_024,
                    max_encoded_bytes: 8 * 1024 * 1024,
                },
            )
            .await?;
        assert_query_result(&query, &expected_ids);
    }

    let identity_client = bootstrap_client(first, "closeout-identity-query", &["admin"]).await?;
    let identity_query = QueryClient::new(&identity_client)
        .collect_bounded(
            &BifrostQueryRequest {
                sql: format!(
                    "SELECT id, wyrd_batch_id, wyrd_row_ordinal FROM {TABLE_FQN} ORDER BY id"
                ),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: None,
            },
            CollectedQueryLimits {
                max_rows: 1_024,
                max_encoded_bytes: 8 * 1024 * 1024,
            },
        )
        .await?;
    let mut retry_identities = Vec::new();
    for batch in &identity_query.batches {
        let ids = batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .ok_or("identity query id column must match the authoritative schema")?;
        let batch_ids = batch
            .column(1)
            .as_any()
            .downcast_ref::<FixedSizeBinaryArray>()
            .ok_or("identity query batch column must be fixed-size binary")?;
        let ordinals = batch
            .column(2)
            .as_any()
            .downcast_ref::<Int32Array>()
            .ok_or("identity query ordinal column must be int32")?;
        retry_identities.extend((0..batch.num_rows()).filter_map(|row| {
            (batch_ids.value(row) == retry_id).then_some((ids.value(row), ordinals.value(row)))
        }));
    }
    assert_eq!(retry_identities, vec![(99, 0)]);

    let rows: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(row_count), 0)::bigint
           FROM vala.file_list
          WHERE data_tenant_id = $1 AND namespace = 'vala.bifrost' AND table_name = $2",
    )
    .bind(tenant.as_uuid())
    .bind(TABLE_NAME)
    .fetch_one(cluster.pg_fixture().operator_pool().pool())
    .await?;
    assert_eq!(
        rows,
        i64::try_from(expected_ids.len()).unwrap_or(i64::MAX),
        "durable readback must include every unique frame row"
    );

    let mut conn = wyrd_sql::TenantConn::acquire(cluster.pg_fixture().app_pool(), tenant).await?;
    let audit_principals: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT principal_id)::bigint
           FROM vala.audit_outbox
          WHERE data_tenant_id = wyrd.current_tenant() AND resource = $1",
    )
    .bind(TABLE_FQN)
    .fetch_one(&mut **conn.transaction())
    .await?;
    assert!(
        audit_principals >= i64::try_from(scribe_count.saturating_add(1)).unwrap_or(i64::MAX),
        "audit rows retain every successful public writer identity"
    );
    Ok(())
}

/// Exercises delayed WAL fsync through flush and exact public query readback.
///
/// The flush boundary must retain all eight acknowledged frames before the
/// terminal-safe query validates their exact ordered contents. Returning early
/// may leave already acknowledged test writes durable until cluster shutdown.
///
/// # Errors
///
/// Returns an error when table creation, client bootstrap, ingest, flush, or
/// bounded query collection fails.
///
/// # Cancellation
///
/// Cancelling this future stops the active operation but does not retract
/// acknowledged WAL appends or completed flush work. The caller retains the
/// cluster and must perform bounded shutdown.
async fn run_delayed_fsync_journey(
    cluster: &WyrdTestCluster,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tenant = cluster.data_tenant_id();
    let server = cluster.server(0).ok_or("missing delayed-fsync pod")?;
    server
        .state()
        .bifrost_redux
        .as_ref()
        .ok_or("missing Redux catalog")?
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, TABLE_NAME),
            user_fields: vec![
                Field::new("id", DataType::Int64, false),
                Field::new("value", DataType::Utf8, false),
            ],
            tenant,
            audit: None,
        })
        .await?;

    let transport = bootstrap_transport(server, "delayed-fsync-admin", &["admin"]).await?;
    for id in 0..8_i64 {
        transport
            .insert_batch(TABLE_FQN, uuid::Uuid::now_v7().into_bytes(), ipc(&[id]))
            .await?;
    }
    server.flush_bifrost().await?;

    let query_client = bootstrap_client(server, "delayed-fsync-query", &["admin"]).await?;
    let query = QueryClient::new(&query_client)
        .collect_bounded(
            &closeout_query(),
            CollectedQueryLimits {
                max_rows: 1_024,
                max_encoded_bytes: 8 * 1024 * 1024,
            },
        )
        .await?;
    assert_query_result(&query, &[0, 1, 2, 3, 4, 5, 6, 7]);
    Ok(())
}

/// Verifies the authoritative schema, decoded rows, and successful terminal.
///
/// # Panics
///
/// Panics when schema, terminal, row or byte counts, batch presence, decoded
/// identifiers, or decoded values differ from the expected query result.
fn assert_query_result(query: &CollectedQueryResult, expected_ids: &[i64]) {
    assert_eq!(query.schema.fields().len(), 2);
    assert_eq!(query.schema.field(0).name(), "id");
    assert_eq!(query.schema.field(0).data_type(), &DataType::Int64);
    assert!(!query.schema.field(0).is_nullable());
    assert_eq!(query.schema.field(1).name(), "value");
    assert_eq!(query.schema.field(1).data_type(), &DataType::Utf8);
    assert!(!query.schema.field(1).is_nullable());
    assert_eq!(query.rows, expected_ids.len());
    assert!(query.encoded_bytes > 0);
    assert_eq!(query.terminal.outcome, QueryTerminalOutcome::Success);
    assert_eq!(
        query.terminal.row_count,
        u64::try_from(expected_ids.len()).unwrap_or(u64::MAX)
    );
    assert!(!query.batches.is_empty(), "Oracle must return Arrow data");

    let mut ids = Vec::new();
    let mut values = Vec::new();
    for batch in &query.batches {
        let batch_ids = batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("query id column must match the authoritative schema");
        let batch_values = batch
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("query value column must match the authoritative schema");
        ids.extend((0..batch.num_rows()).map(|row| batch_ids.value(row)));
        values.extend((0..batch.num_rows()).map(|row| batch_values.value(row)));
    }
    assert_eq!(ids, expected_ids);
    assert_eq!(values, vec!["journey"; expected_ids.len()]);
}

/// Builds the explicit public Oracle policy used by closeout readback.
fn closeout_query() -> BifrostQueryRequest {
    query_for_table(TABLE_FQN)
}

/// Build the strict public Oracle query for one tenant-bound table.
fn query_for_table(table: &str) -> BifrostQueryRequest {
    BifrostQueryRequest {
        sql: format!("SELECT id, value FROM {table} ORDER BY id"),
        visibility: VisibilityMode::PublishedOnly,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: None,
    }
}

async fn bootstrap_transport(
    server: &wyrd_testing::WyrdTestServer,
    name: &str,
    roles: &[&str],
) -> Result<BifrostGrpcTransport, Box<dyn std::error::Error + Send + Sync>> {
    let client = bootstrap_client(server, name, roles).await?;
    Ok(BifrostGrpcTransport::connect(&client).await?)
}

async fn bootstrap_client(
    server: &wyrd_testing::WyrdTestServer,
    name: &str,
    roles: &[&str],
) -> Result<WyrdClient, Box<dyn std::error::Error + Send + Sync>> {
    let bootstrap = server.bootstrap_service(name, roles).await?;
    let key = match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key,
        Bootstrap::User { .. } => return Err("service bootstrap returned a user".into()),
    };
    let mut config = ClientConfig {
        grpc: GrpcConfig {
            endpoint: server.grpc_url().ok_or("missing gRPC endpoint")?,
            connect_retries: 0,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: server.base_url().ok_or("missing HTTP endpoint")?.to_owned(),
            ..HttpConfig::default()
        },
        api_key: Some(key),
        ..ClientConfig::default()
    };
    config.grpc.max_message_bytes = 32 * 1024 * 1024;
    Ok(WyrdClient::with_config(config)?)
}

/// Bootstrap one public client under an explicit data tenant.
async fn bootstrap_client_for_tenant(
    server: &wyrd_testing::WyrdTestServer,
    tenant: wyrd_spec::DataTenantId,
    name: &str,
) -> Result<WyrdClient, Box<dyn std::error::Error + Send + Sync>> {
    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, name, &["admin"])
        .await?;
    let key = match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key,
        Bootstrap::User { .. } => return Err("tenant bootstrap returned a user".into()),
    };
    let mut config = ClientConfig {
        grpc: GrpcConfig {
            endpoint: server.grpc_url().ok_or("missing gRPC endpoint")?,
            connect_retries: 0,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: server.base_url().ok_or("missing HTTP endpoint")?.to_owned(),
            ..HttpConfig::default()
        },
        api_key: Some(key),
        ..ClientConfig::default()
    };
    config.grpc.max_message_bytes = 32 * 1024 * 1024;
    Ok(WyrdClient::with_config(config)?)
}

fn frame(batch_id: [u8; 16], ids: &[i64]) -> BifrostFrame {
    frame_for_table(TABLE_FQN, batch_id, ids)
}

/// Build one exact public ingest frame for a selected table.
fn frame_for_table(table: &str, batch_id: [u8; 16], ids: &[i64]) -> BifrostFrame {
    BifrostFrame {
        table: table.to_owned(),
        batch_id,
        arrow_ipc: ipc(ids).into(),
    }
}

fn ipc(ids: &[i64]) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    let rows = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(ids.to_vec())),
            Arc::new(StringArray::from(vec!["journey"; ids.len()])),
        ],
    )
    .expect("valid closeout batch");
    encode(rows, schema)
}

fn conflicting_ipc() -> Vec<u8> {
    let ids = &[1_i64];
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    let rows = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(StringArray::from(
                ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(vec!["conflict"; ids.len()])),
        ],
    )
    .expect("valid conflicting batch");
    encode(rows, schema)
}

fn encode(rows: RecordBatch, schema: Arc<Schema>) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("IPC writer");
    writer.write(&rows).expect("IPC batch");
    writer.finish().expect("IPC stream");
    bytes
}
