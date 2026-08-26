//! The recorder's owner families and the exact metric-label contract.
//!
//! Module of the `scribe` group; shared fixtures live in `support.rs`.

use std::collections::{BTreeMap, BTreeSet};
use wyrd_testing::bifrost::{BifrostTopology, WyrdTestCluster};

use super::support::*;

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
        "oracle_admission_total",
        &[
            ("class", &["interactive", "analytical"]),
            ("outcome", &["admitted", "rejected"]),
            (
                "reason",
                &[
                    "class_capacity",
                    "tenant_budget",
                    "queue_full",
                    "queue_deadline",
                    "memory",
                    "spill",
                    "audit_unavailable",
                    "membership",
                    "shutdown",
                ],
            ),
        ],
    ),
];

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
        ("oracle_queries_active", &["class=\"interactive\""][..]),
        ("oracle_queries_active", &["class=\"interactive\""][..]),
        ("oracle_queries_active", &["class=\"analytical\""][..]),
        ("oracle_queries_active", &["class=\"analytical\""][..]),
        ("oracle_queries_active", &["class=\"interactive\""][..]),
        ("oracle_queries_active", &["class=\"analytical\""][..]),
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
        "oracle_queries_active",
        "oracle_queries_active",
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
        "wyrd_postgres_pool_acquire_seconds",
        "vala_postgres_pool_acquire_seconds",
        "wyrd_storage_operation_duration_seconds",
        "oracle_query_duration_seconds",
        "oracle_query_time_to_first_batch_seconds",
        "oracle_admission_queue_duration_seconds",
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
        (
            "oracle_query_duration_seconds",
            &[
                ("class", &["interactive", "analytical"][..]),
                (
                    "outcome",
                    &["success", "rejected", "failed", "cancelled"][..],
                ),
            ][..],
        ),
        (
            "oracle_query_time_to_first_batch_seconds",
            &[("class", &["interactive", "analytical"][..])][..],
        ),
        (
            "oracle_admission_queue_duration_seconds",
            &[("class", &["interactive", "analytical"][..])][..],
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
            &[(
                "outcome",
                &["success", "rejected", "failed", "cancelled"][..],
            )][..],
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
    let family = "bifrost_gate_query_stream_duration_seconds";
    let labels = RR9_BASE_CONTRACTS
        .iter()
        .find_map(|(candidate, labels)| (*candidate == family).then_some(*labels))
        .expect("RR9 histogram base contract");
    let mut bucket_labels = labels.to_vec();
    bucket_labels.push(("le", BIFROST_BUCKET_LABELS));
    assert_exact_label_contract(&rendered, &format!("{family}_bucket"), &bucket_labels);
    assert_exact_label_contract(&rendered, &format!("{family}_count"), labels);
    assert_exact_label_contract(&rendered, &format!("{family}_sum"), labels);
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
        let actual = labels
            .keys()
            .filter(|key| {
                expected.contains(key.as_str()) || !matches!(key.as_str(), "quantile" | "le")
            })
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
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
        "oracle_admission_total",
    ]
    .into_iter()
    .collect();
    assert_eq!(actual, expected);
}
