//! Read-only parsing and validation of production Forge telemetry windows.

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use metrics_exporter_prometheus::PrometheusHandle;
use wyrd_telemetry::{CapturedSpan, TestTraceCapture};

use crate::bifrost::BifrostTopology;
#[cfg(test)]
use crate::server::ForgeDataFileInspection;
use crate::server::{ForgeRewriteComparison, ForgeWorkflowInspection};

/// Exact Prometheus sample kind retained across family normalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BifrostMetricKind {
    /// Monotonic counter sample.
    Counter,
    /// Point-in-time gauge sample.
    Gauge,
    /// Cumulative histogram bucket sample.
    HistogramBucket,
    /// Histogram observation-count sample.
    HistogramCount,
    /// Histogram observation-sum sample.
    HistogramSum,
}

/// Prometheus family type declared by exposition metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrometheusFamilyType {
    /// Declared counter family.
    Counter,
    /// Declared gauge family.
    Gauge,
    /// Declared histogram family.
    Histogram,
    /// Exporter summary family outside the canonical binding ledger.
    Summary,
}

/// Window aggregation required by one closed telemetry binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TelemetryAggregation {
    /// Monotonic counter difference.
    Delta,
    /// Maximum sampled gauge value.
    Peak,
    /// Final sampled gauge value.
    Final,
    /// Histogram-derived ninety-ninth percentile.
    P99,
}

/// Normative unit encoded by an exact production family convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TelemetryUnit {
    /// Dimensionless operation or row count.
    Count,
    /// Bytes.
    Bytes,
    /// Seconds.
    Seconds,
    /// Dimensionless budget-pressure ratio.
    Ratio,
}

/// Closed requirement policy for the current representative workload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TelemetryRequirement {
    /// Every canonical cluster window requires the binding.
    Always,
    /// Only windows executing the named role require the binding.
    Role(&'static str),
}

/// One exact allowed value domain for a bounded metric label.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TelemetryLabelValues {
    /// Exact label key.
    key: &'static str,
    /// Closed allowed values.
    values: &'static [&'static str],
}

/// Stable identifier for one private closed telemetry binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TelemetryBindingId(pub(crate) &'static str);

/// One exact label value selected for a binding destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TelemetrySelectedLabel {
    /// Exact allowed label key used by the selector.
    key: &'static str,
    /// Exact allowed value required for destination aggregation.
    value: &'static str,
}

/// Exact production metric binding used by the canonical cluster projection.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TelemetryBinding {
    /// Stable report-local binding identity.
    pub(crate) id: TelemetryBindingId,
    /// Exact label pairs selecting the destination's subset of valid series.
    selected_label_values: &'static [TelemetrySelectedLabel],
    /// Exact normalized production family.
    pub(crate) family: &'static str,
    /// Required Prometheus sample kind.
    pub(crate) kind: BifrostMetricKind,
    /// Normative family unit.
    unit: TelemetryUnit,
    /// Required window aggregation.
    aggregation: TelemetryAggregation,
    /// Closed workload requirement.
    requirement: TelemetryRequirement,
    /// Closed categorical label domains.
    allowed_label_values: &'static [TelemetryLabelValues],
}

/// Closed production labels emitted by the Oracle admission owner.
const ORACLE_ADMISSION_LABELS: &[TelemetryLabelValues] = &[
    TelemetryLabelValues {
        key: "class",
        values: &["interactive", "analytical"],
    },
    TelemetryLabelValues {
        key: "outcome",
        values: &["admitted", "rejected"],
    },
    TelemetryLabelValues {
        key: "reason",
        values: &[
            "class_capacity",
            "tenant_budget",
            "queue_full",
            "queue_deadline",
            "memory",
            "spill",
            "audit_unavailable",
            "shutdown",
        ],
    },
];

/// Closed query terminal labels emitted by the Oracle stream owner.
#[cfg(test)]
const ORACLE_TERMINAL_LABELS: &[TelemetryLabelValues] = &[TelemetryLabelValues {
    key: "outcome",
    values: &["success", "degraded", "failed"],
}];

/// Closed query cancellation labels emitted by the Oracle stream owner.
#[cfg(test)]
const ORACLE_CANCELLATION_LABELS: &[TelemetryLabelValues] = &[TelemetryLabelValues {
    key: "reason",
    values: &["client_drop", "deadline", "shutdown", "peer_failure"],
}];

/// Closed fragment terminal labels emitted by the Oracle dispatcher.
#[cfg(test)]
const ORACLE_FRAGMENT_LABELS: &[TelemetryLabelValues] = &[TelemetryLabelValues {
    key: "outcome",
    values: &["success", "failed"],
}];

/// Closed relay labels emitted by the server audit publisher.
#[cfg(test)]
const ORACLE_AUDIT_RELAY_LABELS: &[TelemetryLabelValues] = &[
    TelemetryLabelValues {
        key: "outcome",
        values: &["committed", "retried_transient", "failed"],
    },
    TelemetryLabelValues {
        key: "reason",
        values: &["postgres", "timeout", "serialization"],
    },
];

/// Closed exact binding ledger shared by qualification and capacity projection.
const CLUSTER_BINDINGS: &[TelemetryBinding] = &[
    TelemetryBinding {
        id: TelemetryBindingId("gate.requests.success"),
        selected_label_values: &[TelemetrySelectedLabel {
            key: "outcome",
            value: "success",
        }],
        family: "bifrost_gate_requests_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Always,
        allowed_label_values: &[
            TelemetryLabelValues {
                key: "operation",
                values: &["query", "write"],
            },
            TelemetryLabelValues {
                key: "outcome",
                values: &["success", "rejected", "failed", "cancelled"],
            },
        ],
    },
    TelemetryBinding {
        id: TelemetryBindingId("gate.requests.query_success"),
        selected_label_values: &[
            TelemetrySelectedLabel {
                key: "operation",
                value: "query",
            },
            TelemetrySelectedLabel {
                key: "outcome",
                value: "success",
            },
        ],
        family: "bifrost_gate_requests_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Always,
        allowed_label_values: &[
            TelemetryLabelValues {
                key: "operation",
                values: &["query", "write"],
            },
            TelemetryLabelValues {
                key: "outcome",
                values: &["success", "rejected", "failed", "cancelled"],
            },
        ],
    },
    TelemetryBinding {
        id: TelemetryBindingId("gate.requests.rejected"),
        selected_label_values: &[TelemetrySelectedLabel {
            key: "outcome",
            value: "rejected",
        }],
        family: "bifrost_gate_requests_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Always,
        allowed_label_values: &[
            TelemetryLabelValues {
                key: "operation",
                values: &["query", "write"],
            },
            TelemetryLabelValues {
                key: "outcome",
                values: &["success", "rejected", "failed", "cancelled"],
            },
        ],
    },
    TelemetryBinding {
        id: TelemetryBindingId("gate.requests.failed"),
        selected_label_values: &[TelemetrySelectedLabel {
            key: "outcome",
            value: "failed",
        }],
        family: "bifrost_gate_requests_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Always,
        allowed_label_values: &[
            TelemetryLabelValues {
                key: "operation",
                values: &["query", "write"],
            },
            TelemetryLabelValues {
                key: "outcome",
                values: &["success", "rejected", "failed", "cancelled"],
            },
        ],
    },
    TelemetryBinding {
        id: TelemetryBindingId("gate.requests.cancelled"),
        selected_label_values: &[TelemetrySelectedLabel {
            key: "outcome",
            value: "cancelled",
        }],
        family: "bifrost_gate_requests_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Always,
        allowed_label_values: &[
            TelemetryLabelValues {
                key: "operation",
                values: &["query", "write"],
            },
            TelemetryLabelValues {
                key: "outcome",
                values: &["success", "rejected", "failed", "cancelled"],
            },
        ],
    },
    TelemetryBinding {
        id: TelemetryBindingId("gate.requests.write_cancelled"),
        selected_label_values: &[
            TelemetrySelectedLabel {
                key: "operation",
                value: "write",
            },
            TelemetrySelectedLabel {
                key: "outcome",
                value: "cancelled",
            },
        ],
        family: "bifrost_gate_requests_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Always,
        allowed_label_values: &[
            TelemetryLabelValues {
                key: "operation",
                values: &["query", "write"],
            },
            TelemetryLabelValues {
                key: "outcome",
                values: &["success", "rejected", "failed", "cancelled"],
            },
        ],
    },
    TelemetryBinding {
        id: TelemetryBindingId("gate.bytes"),
        selected_label_values: &[],
        family: "bifrost_gate_frame_bytes_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Bytes,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Always,
        allowed_label_values: &[],
    },
    TelemetryBinding {
        id: TelemetryBindingId("gate.rows"),
        selected_label_values: &[TelemetrySelectedLabel {
            key: "status",
            value: "accepted",
        }],
        family: "bifrost_gate_rows_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Always,
        allowed_label_values: &[TelemetryLabelValues {
            key: "status",
            values: &["accepted", "rejected"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("gate.active"),
        selected_label_values: &[],
        family: "bifrost_gate_active_streams",
        kind: BifrostMetricKind::Gauge,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Final,
        requirement: TelemetryRequirement::Always,
        allowed_label_values: &[TelemetryLabelValues {
            key: "operation",
            values: &["query"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("cleanup.scribe_ingress"),
        selected_label_values: &[],
        family: "bifrost_scribe_ingress_active",
        kind: BifrostMetricKind::Gauge,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Final,
        requirement: TelemetryRequirement::Role("scribe"),
        allowed_label_values: &[],
    },
    TelemetryBinding {
        id: TelemetryBindingId("cleanup.scribe_lane_active"),
        selected_label_values: &[],
        family: "bifrost_scribe_lane_active",
        kind: BifrostMetricKind::Gauge,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Final,
        requirement: TelemetryRequirement::Role("scribe"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "lane",
            values: &["ingress", "persistence", "wal_io"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("cleanup.scribe_lane_queued"),
        selected_label_values: &[],
        family: "bifrost_scribe_lane_queued",
        kind: BifrostMetricKind::Gauge,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Final,
        requirement: TelemetryRequirement::Role("scribe"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "lane",
            values: &["ingress", "persistence", "wal_io"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("cleanup.scribe_persistence_queue"),
        selected_label_values: &[],
        family: "bifrost_scribe_persistence_queue_depth",
        kind: BifrostMetricKind::Gauge,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Final,
        requirement: TelemetryRequirement::Role("scribe"),
        allowed_label_values: &[],
    },
    TelemetryBinding {
        id: TelemetryBindingId("cleanup.oracle_active"),
        selected_label_values: &[],
        family: "oracle_queries_active",
        kind: BifrostMetricKind::Gauge,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Final,
        requirement: TelemetryRequirement::Role("oracle"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "class",
            values: &["interactive", "analytical"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("cleanup.storage_active"),
        selected_label_values: &[],
        family: "wyrd_storage_operations_active",
        kind: BifrostMetricKind::Gauge,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Final,
        requirement: TelemetryRequirement::Always,
        allowed_label_values: &[
            TelemetryLabelValues {
                key: "backend",
                values: &["local", "s3", "gcs", "azure"],
            },
            TelemetryLabelValues {
                key: "operation",
                values: &["get", "put", "list", "delete", "head"],
            },
        ],
    },
    TelemetryBinding {
        id: TelemetryBindingId("gate.query_streams"),
        selected_label_values: &[],
        family: "bifrost_gate_query_streams_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Always,
        allowed_label_values: &[TelemetryLabelValues {
            key: "outcome",
            values: &["success", "rejected", "failed", "cancelled"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("gate.query_streams.cancelled"),
        selected_label_values: &[TelemetrySelectedLabel {
            key: "outcome",
            value: "cancelled",
        }],
        family: "bifrost_gate_query_streams_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Always,
        allowed_label_values: &[TelemetryLabelValues {
            key: "outcome",
            values: &["success", "rejected", "failed", "cancelled"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("gate.request_duration"),
        selected_label_values: &[],
        family: "bifrost_gate_request_duration_seconds",
        kind: BifrostMetricKind::HistogramBucket,
        unit: TelemetryUnit::Seconds,
        aggregation: TelemetryAggregation::P99,
        requirement: TelemetryRequirement::Always,
        allowed_label_values: &[
            TelemetryLabelValues {
                key: "operation",
                values: &["query", "write"],
            },
            TelemetryLabelValues {
                key: "outcome",
                values: &["success", "rejected", "failed", "cancelled"],
            },
        ],
    },
    TelemetryBinding {
        id: TelemetryBindingId("gate.query_stream_duration"),
        selected_label_values: &[],
        family: "bifrost_gate_query_stream_duration_seconds",
        kind: BifrostMetricKind::HistogramBucket,
        unit: TelemetryUnit::Seconds,
        aggregation: TelemetryAggregation::P99,
        requirement: TelemetryRequirement::Always,
        allowed_label_values: &[TelemetryLabelValues {
            key: "outcome",
            values: &["success", "rejected", "failed", "cancelled"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("scribe.rows"),
        selected_label_values: &[],
        family: "bifrost_scribe_rows_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("scribe"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "status",
            values: &["accepted"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("forge.backlog_peak"),
        selected_label_values: &[],
        family: "bifrost_forge_oldest_backlog_seconds",
        kind: BifrostMetricKind::Gauge,
        unit: TelemetryUnit::Seconds,
        aggregation: TelemetryAggregation::Peak,
        requirement: TelemetryRequirement::Role("forge"),
        allowed_label_values: &[],
    },
    TelemetryBinding {
        id: TelemetryBindingId("oracle.admission"),
        selected_label_values: &[],
        family: "oracle_admission_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("oracle"),
        allowed_label_values: ORACLE_ADMISSION_LABELS,
    },
    TelemetryBinding {
        id: TelemetryBindingId("oracle.admission_queue"),
        selected_label_values: &[],
        family: "oracle_admission_queue_duration_seconds",
        kind: BifrostMetricKind::HistogramBucket,
        unit: TelemetryUnit::Seconds,
        aggregation: TelemetryAggregation::P99,
        requirement: TelemetryRequirement::Role("oracle"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "class",
            values: &["interactive", "analytical"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("oracle.tenant_pressure_peak"),
        selected_label_values: &[],
        family: "oracle_tenant_budget_pressure",
        kind: BifrostMetricKind::Gauge,
        unit: TelemetryUnit::Ratio,
        aggregation: TelemetryAggregation::Peak,
        requirement: TelemetryRequirement::Role("oracle"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "class",
            values: &["interactive", "analytical"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("scribe.wal_bytes"),
        selected_label_values: &[],
        family: "bifrost_scribe_wal_append_bytes_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Bytes,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("scribe"),
        allowed_label_values: &[],
    },
    TelemetryBinding {
        id: TelemetryBindingId("scribe.seal_rows"),
        selected_label_values: &[TelemetrySelectedLabel {
            key: "stage",
            value: "file_list_transaction",
        }],
        family: "bifrost_scribe_seal_rows_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("scribe"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "stage",
            values: &[
                "freeze",
                "parquet_encode",
                "object_store_put",
                "file_list_transaction",
            ],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("forge.publications"),
        selected_label_values: &[],
        family: "bifrost_forge_complete_gauge_publications_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("forge"),
        allowed_label_values: &[],
    },
    TelemetryBinding {
        id: TelemetryBindingId("forge.rewrite_input_files"),
        selected_label_values: &[],
        family: "bifrost_forge_rewrite_input_files_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("forge"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "source",
            values: &["staging", "iceberg"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("forge.rewrite_input_bytes"),
        selected_label_values: &[],
        family: "bifrost_forge_rewrite_input_bytes_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Bytes,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("forge"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "source",
            values: &["staging", "iceberg"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("forge.rewrite_output_files"),
        selected_label_values: &[],
        family: "bifrost_forge_rewrite_output_files_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("forge"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "source",
            values: &["staging", "iceberg"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("forge.rewrite_output_bytes"),
        selected_label_values: &[],
        family: "bifrost_forge_rewrite_output_bytes_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Bytes,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("forge"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "source",
            values: &["staging", "iceberg"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("oracle.rows"),
        selected_label_values: &[],
        family: "oracle_query_rows_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("oracle"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "class",
            values: &["interactive", "analytical"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("oracle.stream_bytes"),
        selected_label_values: &[],
        family: "oracle_query_bytes_returned_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Bytes,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("oracle"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "class",
            values: &["interactive", "analytical"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("oracle.logical_bytes"),
        selected_label_values: &[],
        family: "oracle_query_logical_bytes_selected_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Bytes,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("oracle"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "class",
            values: &["interactive", "analytical"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("oracle.physical_bytes"),
        selected_label_values: &[],
        family: "oracle_query_bytes_scanned_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Bytes,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("oracle"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "class",
            values: &["interactive", "analytical"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("oracle.files"),
        selected_label_values: &[],
        family: "oracle_query_files_scanned_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("oracle"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "class",
            values: &["interactive", "analytical"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("oracle.partitions"),
        selected_label_values: &[],
        family: "oracle_query_partitions_scanned_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("oracle"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "class",
            values: &["interactive", "analytical"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("oracle.queued_peak"),
        selected_label_values: &[],
        family: "oracle_queries_queued",
        kind: BifrostMetricKind::Gauge,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Peak,
        requirement: TelemetryRequirement::Role("oracle"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "class",
            values: &["interactive", "analytical"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("oracle.spill_bytes"),
        selected_label_values: &[],
        family: "oracle_query_spill_bytes_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Bytes,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("oracle"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "class",
            values: &["interactive", "analytical"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("oracle.spill_files"),
        selected_label_values: &[],
        family: "oracle_query_spill_files_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("oracle"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "class",
            values: &["interactive", "analytical"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("oracle.spill_queries"),
        selected_label_values: &[],
        family: "oracle_query_spill_queries_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("oracle"),
        allowed_label_values: &[
            TelemetryLabelValues {
                key: "class",
                values: &["interactive", "analytical"],
            },
            TelemetryLabelValues {
                key: "outcome",
                values: &["success", "error", "cancelled"],
            },
        ],
    },
    TelemetryBinding {
        id: TelemetryBindingId("postgres.acquire"),
        selected_label_values: &[],
        family: "vala_postgres_pool_acquire_seconds",
        kind: BifrostMetricKind::HistogramBucket,
        unit: TelemetryUnit::Seconds,
        aggregation: TelemetryAggregation::P99,
        requirement: TelemetryRequirement::Always,
        allowed_label_values: &[
            TelemetryLabelValues {
                key: "outcome",
                values: &["success", "failed", "cancelled"],
            },
            TelemetryLabelValues {
                key: "pool",
                values: &["runtime"],
            },
        ],
    },
    TelemetryBinding {
        id: TelemetryBindingId("postgres.transactions"),
        selected_label_values: &[],
        family: "vala_postgres_pool_acquire_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Always,
        allowed_label_values: &[TelemetryLabelValues {
            key: "pool",
            values: &["runtime"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("storage.duration"),
        selected_label_values: &[],
        family: "wyrd_storage_operation_duration_seconds",
        kind: BifrostMetricKind::HistogramBucket,
        unit: TelemetryUnit::Seconds,
        aggregation: TelemetryAggregation::P99,
        requirement: TelemetryRequirement::Always,
        allowed_label_values: &[
            TelemetryLabelValues {
                key: "backend",
                values: &["local", "s3", "gcs", "azure"],
            },
            TelemetryLabelValues {
                key: "operation",
                values: &["get", "put", "list", "delete", "head"],
            },
            TelemetryLabelValues {
                key: "outcome",
                values: &["success", "failed", "cancelled"],
            },
        ],
    },
    TelemetryBinding {
        id: TelemetryBindingId("storage.bytes"),
        selected_label_values: &[],
        family: "wyrd_storage_bytes_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Bytes,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Always,
        allowed_label_values: &[
            TelemetryLabelValues {
                key: "backend",
                values: &["local", "s3", "gcs", "azure"],
            },
            TelemetryLabelValues {
                key: "direction",
                values: &["read", "write"],
            },
            TelemetryLabelValues {
                key: "operation",
                values: &["get", "put"],
            },
        ],
    },
    TelemetryBinding {
        id: TelemetryBindingId("wal.fsync"),
        selected_label_values: &[],
        family: "bifrost_scribe_wal_fsync_seconds",
        kind: BifrostMetricKind::HistogramBucket,
        unit: TelemetryUnit::Seconds,
        aggregation: TelemetryAggregation::P99,
        requirement: TelemetryRequirement::Role("scribe"),
        allowed_label_values: &[TelemetryLabelValues {
            key: "outcome",
            values: &["success", "failed", "cancelled"],
        }],
    },
];

/// Every destination required by a complete benchmark evidence window.
#[cfg(feature = "bench")]
const COMPLETE_BINDING_IDS: &[&str] = &[
    "gate.requests.success",
    "gate.requests.query_success",
    "gate.requests.rejected",
    "gate.requests.failed",
    "gate.requests.cancelled",
    "gate.requests.write_cancelled",
    "gate.bytes",
    "gate.rows",
    "gate.active",
    "cleanup.scribe_ingress",
    "cleanup.scribe_lane_active",
    "cleanup.scribe_lane_queued",
    "cleanup.scribe_persistence_queue",
    "cleanup.oracle_active",
    "cleanup.storage_active",
    "gate.query_streams",
    "gate.query_streams.cancelled",
    "gate.request_duration",
    "gate.query_stream_duration",
    "scribe.rows",
    "forge.backlog_peak",
    "oracle.admission",
    "oracle.admission_queue",
    "oracle.tenant_pressure_peak",
    "scribe.wal_bytes",
    "scribe.seal_rows",
    "forge.publications",
    "forge.rewrite_input_files",
    "forge.rewrite_input_bytes",
    "forge.rewrite_output_files",
    "forge.rewrite_output_bytes",
    "oracle.rows",
    "oracle.stream_bytes",
    "oracle.logical_bytes",
    "oracle.physical_bytes",
    "oracle.files",
    "oracle.partitions",
    "oracle.queued_peak",
    "oracle.spill_bytes",
    "postgres.acquire",
    "postgres.transactions",
    "storage.duration",
    "storage.bytes",
    "wal.fsync",
];

/// Closed dependency destinations carried by canonical evidence.
const DEPENDENCY_BINDING_IDS: &[&str] = &[
    "postgres.acquire",
    "postgres.transactions",
    "storage.duration",
    "storage.bytes",
    "wal.fsync",
];

/// Closed final-gauge destinations carried by cleanup evidence.
const CLEANUP_BINDING_IDS: &[&str] = &[
    "gate.active",
    "cleanup.scribe_ingress",
    "cleanup.scribe_lane_active",
    "cleanup.scribe_lane_queued",
    "cleanup.scribe_persistence_queue",
    "cleanup.oracle_active",
    "cleanup.storage_active",
];

/// Closed trace operations carried by canonical evidence.
const COMPLETE_TRACE_OPERATIONS: &[ClusterTraceOperation] = &[
    ClusterTraceOperation::DurableWrite,
    ClusterTraceOperation::FlushToVisible,
    ClusterTraceOperation::QueryTimeToFirstFrame,
    ClusterTraceOperation::QueryTotal,
];

/// One typed value produced by evaluating an exact closed binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvaluatedBindingValue {
    /// Window counter delta.
    Counter(u64),
    /// Sampled gauge final and peak values.
    Gauge {
        /// Sum of final values across accepted closed label series.
        final_value: u64,
        /// Maximum sampled value across accepted closed label series.
        peak: u64,
    },
    /// Checked aggregate histogram p99 in microseconds.
    DurationP99Micros(u64),
}

/// Canonical crate-local phase evidence shared by benchmark and journey consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClusterPhaseTelemetryEvidence {
    /// Gate rows accepted in the sampled phase.
    pub(crate) gate_rows: u64,
    /// Gate frame bytes received in the sampled phase.
    pub(crate) gate_bytes: u64,
    /// Aggregate successful Gate request terminals.
    pub(crate) gate_success: u64,
    /// Aggregate rejected Gate request terminals.
    pub(crate) gate_rejected: u64,
    /// Aggregate failed Gate request terminals.
    pub(crate) gate_failed: u64,
    /// Aggregate cancelled Gate request terminals.
    pub(crate) gate_cancelled: u64,
    /// Successful query-request terminals selected from exact labels.
    pub(crate) gate_query_request_success: u64,
    /// Cancelled write-request terminals selected from exact labels.
    pub(crate) gate_write_request_cancelled: u64,
    /// Cancelled returned query-stream terminals.
    pub(crate) gate_query_stream_cancelled: u64,
    /// All returned query-stream terminal outcomes.
    pub(crate) gate_query_stream_terminals: u64,
    /// Final active Gate query streams.
    pub(crate) gate_active_streams: u64,
    /// Scribe rows accepted in the sampled phase.
    pub(crate) scribe_rows: u64,
    /// Rows returned by successful Oracle streams.
    pub(crate) oracle_stream_rows: u64,
    /// Bytes returned by successful Oracle streams.
    pub(crate) oracle_stream_bytes: u64,
    /// Forge rewrite input files committed in the phase.
    pub(crate) forge_input_files: u64,
    /// Forge rewrite input bytes committed in the phase.
    pub(crate) forge_input_bytes: u64,
    /// Forge rewrite output files committed in the phase.
    pub(crate) forge_output_files: u64,
    /// Forge rewrite output bytes committed in the phase.
    pub(crate) forge_output_bytes: u64,
}

/// Dependency-neutral operation identity for exact production trace evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ClusterTraceOperation {
    /// Gate/Scribe work that durably acknowledges a write.
    DurableWrite,
    /// Forge catalog publication that makes acknowledged rows visible.
    FlushToVisible,
    /// Oracle source work up to the first returned frame.
    QueryTimeToFirstFrame,
    /// Complete Oracle query lifetime.
    QueryTotal,
}

impl ClusterTraceOperation {
    /// Return the stable private evidence identifier for this operation.
    #[must_use]
    const fn id(self) -> &'static str {
        match self {
            Self::DurableWrite => "trace.durable_write",
            Self::FlushToVisible => "trace.flush_to_visible",
            Self::QueryTimeToFirstFrame => "trace.query_time_to_first_frame",
            Self::QueryTotal => "trace.query_total",
        }
    }
}

/// Closed topology, metric, dependency, trace, and cleanup policy for one window.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ClusterTelemetryExpectation {
    /// Process topology whose logical nodes share the sampled runtime.
    pub(crate) topology: BifrostTopology,
    /// Exact metric destinations that must exist in this phase.
    pub(crate) required_binding_ids: &'static [&'static str],
    /// Exact dependency destinations that must exist in this phase.
    pub(crate) required_dependency_ids: &'static [&'static str],
    /// Exact trace operations that must occur in this phase.
    pub(crate) required_trace_operations: &'static [ClusterTraceOperation],
    /// Exact final-gauge destinations that must equal zero in this phase.
    pub(crate) required_clean_binding_ids: &'static [&'static str],
}

/// Canonical pillar counters retained independently from report-layer types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClusterPillarTelemetryEvidence {
    /// Successful Gate request terminals.
    pub(crate) gate_accepted: u64,
    /// Bytes appended through Scribe WAL ownership.
    pub(crate) scribe_wal_bytes: u64,
    /// Successful Forge publications.
    pub(crate) forge_publications: u64,
    /// Rows decoded by successful Oracle streams.
    pub(crate) oracle_decoded_rows: u64,
}

/// Canonical dependency observations without a dependency on benchmark reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClusterDependencyTelemetryEvidence {
    /// PostgreSQL pool-acquire p99 in microseconds when observed.
    pub(crate) postgres_pool_wait_us: Option<u64>,
    /// PostgreSQL transaction attempts when observed.
    pub(crate) postgres_transactions: Option<u64>,
    /// Storage bytes transferred when observed.
    pub(crate) storage_bytes: Option<u64>,
    /// Storage operation p99 in microseconds when observed.
    pub(crate) storage_p99_us: Option<u64>,
    /// Scribe WAL fsync p99 in microseconds when observed.
    pub(crate) wal_fsync_p99_us: Option<u64>,
}

/// Closed dependency-neutral runtime role retained by process evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClusterRuntimeRole {
    /// Public request admission and routing.
    Gate,
    /// Durable WAL append and acknowledgement.
    Scribe,
    /// Publication and compaction work.
    Forge,
    /// Query planning, admission, and streaming.
    Oracle,
}

/// Honest process-scoped resource evidence for one sampled cluster window.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ClusterProcessTelemetryEvidence {
    /// Stable sampled operating-system process identity.
    pub(crate) identity: String,
    /// Explicit counter epoch after any process replacement.
    pub(crate) epoch: u64,
    /// Logical nodes hosted by this one process.
    pub(crate) hosted_logical_nodes: Vec<String>,
    /// Bifrost roles hosted by this process.
    pub(crate) roles: Vec<ClusterRuntimeRole>,
    /// CPU seconds consumed in the final stable epoch.
    pub(crate) cpu_seconds: f64,
    /// Maximum sampled resident bytes.
    pub(crate) peak_rss_bytes: u64,
    /// Final sampled resident bytes.
    pub(crate) current_rss_bytes: u64,
    /// Tokio busy seconds consumed in the final stable epoch.
    pub(crate) runtime_busy_seconds: f64,
    /// Maximum sampled Tokio global queue depth.
    pub(crate) runtime_queue_peak: u64,
}

/// Exact trace distribution for one dependency-neutral operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClusterTraceTelemetryEvidence {
    /// Closed operation proved by exact span names.
    pub(crate) operation: ClusterTraceOperation,
    /// Bounded representative trace identifiers.
    pub(crate) representative_trace_ids: Vec<String>,
    /// Number of exact production spans in the window.
    pub(crate) samples: u64,
    /// Nearest-rank p95 duration in microseconds.
    pub(crate) p95_us: u64,
    /// Nearest-rank p99 duration in microseconds.
    pub(crate) p99_us: u64,
}

/// Exact counters consumed by client, audit, and durable-state reconciliation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClusterReconciliationTelemetryEvidence {
    /// Rows sealed durably by Scribe.
    pub(crate) sealed_rows: u64,
    /// Successful query request terminals.
    pub(crate) successful_queries: u64,
}

/// Exact final values for every cleanup binding in the canonical ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClusterCleanupTelemetryEvidence {
    /// Final active Gate query streams.
    pub(crate) gate_active: Option<u64>,
    /// Final active Scribe ingress operations.
    pub(crate) scribe_ingress: Option<u64>,
    /// Final active Scribe lane operations.
    pub(crate) scribe_lane_active: Option<u64>,
    /// Final queued Scribe lane operations.
    pub(crate) scribe_lane_queued: Option<u64>,
    /// Final Scribe persistence queue depth.
    pub(crate) scribe_persistence_queue: Option<u64>,
    /// Final in-flight Oracle queries.
    pub(crate) oracle_in_flight: Option<u64>,
    /// Final active storage operations.
    pub(crate) storage_active: Option<u64>,
}

impl ClusterCleanupTelemetryEvidence {
    /// Return whether every retained cleanup value is zero.
    #[must_use]
    #[cfg(feature = "bench")]
    pub(crate) fn is_clean(&self) -> bool {
        [
            self.gate_active,
            self.scribe_ingress,
            self.scribe_lane_active,
            self.scribe_lane_queued,
            self.scribe_persistence_queue,
            self.oracle_in_flight,
            self.storage_active,
        ]
        .into_iter()
        .flatten()
        .all(|value| value == 0)
    }
}

/// One complete canonical cluster projection shared by matrix and bench consumers.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ClusterTelemetryEvidence {
    /// Exact counters required by phase-level journey reconciliation.
    pub(crate) phase: ClusterPhaseTelemetryEvidence,
    /// Exact canonical pillar counters.
    pub(crate) pillars: ClusterPillarTelemetryEvidence,
    /// Exact dependency observations.
    pub(crate) dependencies: ClusterDependencyTelemetryEvidence,
    /// Honest process-scoped resource evidence.
    pub(crate) process: ClusterProcessTelemetryEvidence,
    /// Exact-name trace operation distributions.
    pub(crate) traces: Vec<ClusterTraceTelemetryEvidence>,
    /// Exact owner counters used by reconciliation.
    pub(crate) reconciliation: ClusterReconciliationTelemetryEvidence,
    /// Exact final cleanup values.
    pub(crate) cleanup: ClusterCleanupTelemetryEvidence,
}

/// Sole owner of canonical binding evaluation and complete evidence construction.
pub(crate) struct ClusterTelemetryProjection;

#[cfg(feature = "bench")]
impl ClusterTelemetryExpectation {
    /// Build the complete policy used by benchmark report assembly.
    #[must_use]
    pub(crate) const fn complete(topology: BifrostTopology) -> Self {
        Self {
            topology,
            required_binding_ids: COMPLETE_BINDING_IDS,
            required_dependency_ids: DEPENDENCY_BINDING_IDS,
            required_trace_operations: COMPLETE_TRACE_OPERATIONS,
            required_clean_binding_ids: CLEANUP_BINDING_IDS,
        }
    }
}

/// One parsed production Prometheus sample.
#[derive(Debug, Clone, PartialEq)]
pub struct BifrostMetricSample {
    /// Normalized production metric family.
    pub family: String,
    /// Exact fixed-cardinality labels emitted with the sample.
    pub labels: BTreeMap<String, String>,
    /// Absolute or window-delta value, depending on the enclosing artifact.
    pub value: f64,
    /// Exact rendered sample kind.
    pub kind: BifrostMetricKind,
}

/// One single-use marker for a production telemetry observation window.
#[derive(Debug, Clone)]
pub struct BifrostTelemetryCheckpoint {
    /// Unique process-local token rejected after one delta construction.
    id: u64,
    /// Absolute sample values at the start of the window.
    metrics: BTreeMap<String, f64>,
    /// Exact family types declared at the window baseline.
    types: BTreeMap<String, PrometheusFamilyType>,
    /// Monotonic start instant used for window rates.
    started_at: Instant,
    /// Production trace position before the window begins.
    spans: usize,
    /// Process counters and identity at the beginning of the window.
    process: ProcessSample,
}

/// One process-scoped resource observation owned by the capture sampler.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProcessSample {
    /// Stable OS identity; a change starts a new counter epoch.
    pub(crate) identity: String,
    /// Explicit replacement epoch within this capture.
    pub(crate) epoch: u64,
    /// Cumulative process CPU seconds.
    pub(crate) cpu_total: f64,
    /// Point-in-time resident bytes.
    pub(crate) rss_bytes: u64,
    /// Cumulative Tokio worker busy seconds.
    pub(crate) tokio_busy_total: f64,
    /// Point-in-time Tokio global queue depth.
    pub(crate) queue_depth: u64,
}

/// Checked process resource delta and sampled peaks for one window.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProcessWindow {
    /// Final process identity.
    pub(crate) identity: String,
    /// Final explicit replacement epoch.
    pub(crate) epoch: u64,
    /// CPU consumed within the final stable epoch.
    pub(crate) cpu_seconds: f64,
    /// Final resident bytes.
    pub(crate) current_rss_bytes: u64,
    /// Maximum sampled resident bytes.
    pub(crate) peak_rss_bytes: u64,
    /// Tokio busy time consumed within the final stable epoch.
    pub(crate) tokio_busy_seconds: f64,
    /// Maximum sampled runtime queue depth.
    pub(crate) queue_peak: u64,
}

/// Captured production telemetry emitted during one observation window.
#[derive(Debug, Clone)]
pub struct BifrostTelemetryDelta {
    /// Production family inventory declared by the closing Prometheus scrape.
    pub(crate) families: BTreeSet<String>,
    /// Counter and histogram deltas from the one production render handle.
    pub metrics: Vec<BifrostMetricSample>,
    /// Gauge maxima observed by the production capture while the window ran.
    pub gauge_maxima: Vec<BifrostMetricSample>,
    /// Final gauge values from the render that closed the capture window.
    pub gauge_final: Vec<BifrostMetricSample>,
    /// Finished production-provider spans after the checkpoint.
    pub spans: Vec<CapturedSpan>,
    /// Positive render-to-render duration used by counter-rate calculations.
    pub interval_seconds: f64,
    /// Honest sampled process resource evidence for the capture window.
    pub(crate) process: ProcessWindow,
}

/// Values retained by the single bounded gauge/process sampler.
#[derive(Debug)]
struct SamplerSnapshot {
    /// Per-series gauge maxima.
    maxima: BTreeMap<String, f64>,
    /// Most recent process observation.
    process: ProcessSample,
    /// Maximum RSS observed across all ticks in the final epoch.
    peak_rss_bytes: u64,
    /// Maximum queue depth observed across all ticks in the final epoch.
    queue_peak: u64,
}

/// Bounded background sampler for production gauges during one capture window.
pub struct BifrostTelemetrySampler {
    /// Cancellation signal for the production-render polling task.
    stop: tokio_util::sync::CancellationToken,
    /// Per-series maxima accumulated only from production-rendered gauges.
    /// Task that owns bounded polling and returns its complete snapshot.
    task: tokio::task::JoinHandle<Result<SamplerSnapshot, BifrostTelemetryReportError>>,
}

/// Failure raised when a production telemetry window cannot prove one report field.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BifrostTelemetryReportError {
    /// One exact closed binding was absent or malformed.
    #[error("invalid production telemetry binding {id}: {detail}")]
    InvalidBinding {
        /// Stable private binding identifier.
        id: String,
        /// Non-sensitive validation detail.
        detail: String,
    },
    /// A closed production family or label combination was absent.
    #[error("missing production telemetry series {family}")]
    MissingSeries {
        /// Required production metric family.
        family: String,
    },
    /// A required activity counter did not advance in the capture window.
    #[error("stale production telemetry series {family}")]
    StaleSeries {
        /// Required fresh production metric family.
        family: String,
    },
    /// A monotonic counter or histogram regressed across a capture window.
    #[error("production telemetry counter regressed for {series}")]
    CounterRegression {
        /// Exact rendered series that regressed.
        series: String,
    },
    /// A required histogram received no production observations.
    #[error("production telemetry histogram {family} has no samples")]
    EmptyHistogram {
        /// Required histogram family.
        family: String,
    },
    /// A checkpoint was supplied to delta construction more than once.
    #[error("production telemetry checkpoint was already consumed")]
    ReusedWindow,
    /// A capture interval was zero, negative, or not representable.
    #[error("production telemetry capture interval is not positive")]
    InvalidInterval,
    /// The bounded sampler task panicked or was cancelled before joining.
    #[error("production telemetry sampler join failed: {detail}")]
    SamplerJoin {
        /// Non-sensitive join failure.
        detail: String,
    },
    /// Active role topology diverged from immutable launched-role evidence.
    #[error("production role topology does not match executed roles")]
    TopologyMismatch,
    /// A required exact-run production span was absent.
    #[error("missing production Forge span {span}")]
    MissingSpan {
        /// Exact required production instrumentation name.
        span: String,
    },
    /// A production Forge span used a renamed or malformed contract.
    #[error("invalid production Forge span {span}: {detail}")]
    InvalidSpan {
        /// Exact captured instrumentation name.
        span: String,
        /// Non-sensitive contract failure description.
        detail: String,
    },
    /// Rendered Prometheus text did not use the expected restricted grammar.
    #[error("invalid Prometheus production sample: {detail}")]
    Parse {
        /// Non-sensitive parse failure detail.
        detail: String,
    },
}

/// Read-only capture over one installed production Prometheus recorder and tracer.
#[derive(Clone)]
pub struct BifrostTelemetryCapture {
    /// Production recorder render handle, never passed into Forge.
    metrics: PrometheusHandle,
    /// Same-provider trace capture installed before role composition.
    traces: TestTraceCapture,
    /// Tokens already consumed by `delta_since`.
    consumed: Arc<Mutex<BTreeSet<u64>>>,
    /// Monotonic checkpoint identity that cannot collide in this capture.
    next_checkpoint: Arc<AtomicU64>,
}

impl BifrostTelemetryCapture {
    /// Construct a capture after the process production runtime is installed.
    #[must_use]
    pub fn new(metrics: PrometheusHandle, traces: TestTraceCapture) -> Self {
        Self {
            metrics,
            traces,
            consumed: Arc::new(Mutex::new(BTreeSet::new())),
            next_checkpoint: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Record a baseline from the configured production exporters.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostTelemetryReportError::Parse`] when rendered Prometheus
    /// output cannot be parsed without losing a label or numeric value.
    pub fn checkpoint(&self) -> Result<BifrostTelemetryCheckpoint, BifrostTelemetryReportError> {
        let rendered = self.metrics.render();
        Ok(BifrostTelemetryCheckpoint {
            id: self.next_checkpoint.fetch_add(1, Ordering::AcqRel),
            metrics: rendered_values(&rendered)?,
            types: rendered_types(&rendered)?,
            started_at: Instant::now(),
            spans: self.traces.checkpoint(),
            process: process_sample(0)?,
        })
    }

    /// Render the current production Prometheus exposition for integration assertions.
    ///
    /// This returns the installed process recorder's text unchanged. It does not
    /// retain a scrape or add any fixture-derived metric values.
    #[must_use]
    pub fn render(&self) -> String {
        self.metrics.render()
    }

    /// Return the current production Prometheus series as absolute samples.
    ///
    /// # Errors
    ///
    /// Returns a parse error when the recorder emits a malformed series.
    pub fn snapshot(&self) -> Result<Vec<BifrostMetricSample>, BifrostTelemetryReportError> {
        let rendered = self.metrics.render();
        let types = rendered_types(&rendered)?;
        rendered_values(&rendered)?
            .into_iter()
            .filter(|(series, _)| supported_series(series, &types))
            .map(|(series, value)| parse_sample(&series, value, &types))
            .collect()
    }

    /// Return the metric families currently rendered by the production recorder.
    #[must_use]
    pub fn families(&self) -> BTreeSet<String> {
        let rendered = self.metrics.render();
        let types = rendered_types(&rendered).unwrap_or_default();
        rendered_values(&rendered)
            .ok()
            .into_iter()
            .flat_map(|values| values.into_iter())
            .filter_map(|(series, value)| parse_sample(&series, value, &types).ok())
            .map(|sample| sample.family)
            .collect()
    }

    /// Return a one-shot synchronous delta from a production checkpoint.
    ///
    /// # Errors
    ///
    /// Returns the same parse, reuse, regression, and interval errors as the
    /// asynchronous sampler-backed capture.
    pub fn delta_since(
        &self,
        checkpoint: &BifrostTelemetryCheckpoint,
    ) -> Result<BifrostTelemetryDelta, BifrostTelemetryReportError> {
        self.delta_without_sampler(checkpoint.clone())
    }

    /// Return the names of all finished spans in the shared production capture.
    #[must_use]
    pub fn span_names(&self) -> BTreeSet<String> {
        self.traces
            .finished_since(0)
            .into_iter()
            .map(|span| span.name)
            .collect()
    }

    /// Build one checked delta from the checkpointed production exporter state.
    ///
    /// Gauge maxima include the baseline and end snapshot. A caller that needs
    /// denser bounded sampling calls this method at its capture cadence and
    /// retains the maximum externally; no fixture value enters this path.
    ///
    /// # Errors
    ///
    /// Returns a parse, reused-window, counter-regression, or invalid-interval
    /// error when this capture cannot represent a trustworthy production window.
    fn delta_without_sampler(
        &self,
        checkpoint: BifrostTelemetryCheckpoint,
    ) -> Result<BifrostTelemetryDelta, BifrostTelemetryReportError> {
        let mut consumed = self
            .consumed
            .lock()
            .map_err(|_| BifrostTelemetryReportError::ReusedWindow)?;
        if !consumed.insert(checkpoint.id) {
            return Err(BifrostTelemetryReportError::ReusedWindow);
        }
        drop(consumed);
        let elapsed = checkpoint.started_at.elapsed();
        if elapsed.is_zero() {
            return Err(BifrostTelemetryReportError::InvalidInterval);
        }
        let rendered = self.metrics.render();
        let current_types = rendered_types(&rendered)?;
        if checkpoint.types.iter().any(|(family, baseline)| {
            current_types
                .get(family)
                .is_some_and(|current| current != baseline)
        }) {
            return Err(BifrostTelemetryReportError::Parse {
                detail: "Prometheus TYPE metadata conflicted within one window".to_owned(),
            });
        }
        let current = rendered_values(&rendered)?;
        let mut metrics = Vec::new();
        let mut gauge_maxima = Vec::new();
        let mut gauge_final = Vec::new();
        for (series, value) in &current {
            if !supported_series(series, &current_types) {
                continue;
            }
            let prior = checkpoint.metrics.get(series).copied().unwrap_or(0.0);
            let parsed = parse_sample(series, *value, &current_types)?;
            if parsed.kind != BifrostMetricKind::Gauge {
                if *value < prior {
                    return Err(BifrostTelemetryReportError::CounterRegression {
                        series: series.clone(),
                    });
                }
                let mut sample = parsed;
                sample.value = *value - prior;
                metrics.push(sample);
            } else {
                let baseline = parse_sample(series, prior, &current_types)?;
                let end = parse_sample(series, *value, &current_types)?;
                gauge_maxima.push(BifrostMetricSample {
                    family: end.family.clone(),
                    labels: end.labels.clone(),
                    value: baseline.value.max(end.value),
                    kind: end.kind,
                });
                gauge_final.push(end);
            }
        }
        Ok(BifrostTelemetryDelta {
            families: current_types.keys().cloned().collect(),
            metrics,
            gauge_maxima,
            gauge_final,
            spans: self.traces.finished_since(checkpoint.spans),
            interval_seconds: elapsed.as_secs_f64(),
            process: process_window(
                &checkpoint.process,
                process_sample(checkpoint.process.epoch)?,
                checkpoint.process.rss_bytes,
                checkpoint.process.queue_depth,
            )?,
        })
    }

    /// Start bounded fixed-interval production-gauge sampling for one window.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostTelemetryReportError::Parse`] when the initial render is
    /// malformed. The sampler never receives fixture, SQL, or scenario values.
    pub async fn begin_gauge_sampling(
        &self,
        _checkpoint: &BifrostTelemetryCheckpoint,
    ) -> Result<BifrostTelemetrySampler, BifrostTelemetryReportError> {
        let initial_rendered = self.metrics.render();
        let initial = rendered_values(&initial_rendered)?;
        let initial_types = rendered_types(&initial_rendered)?;
        let mut initial_maxima = BTreeMap::new();
        merge_gauge_maxima(&mut initial_maxima, &initial, &initial_types)?;
        let initial_process = process_sample(_checkpoint.process.epoch)?;
        let stop = tokio_util::sync::CancellationToken::new();
        let sampler_stop = stop.clone();
        let metrics = self.metrics.clone();
        let task = tokio::spawn(async move {
            let mut snapshot = SamplerSnapshot {
                maxima: initial_maxima,
                peak_rss_bytes: initial_process.rss_bytes,
                queue_peak: initial_process.queue_depth,
                process: initial_process,
            };
            // Forge's production reservation can cover a sub-25ms rewrite. Sample at a
            // millisecond cadence so the benchmark's peak remains an observed exporter
            // value rather than a coincidental final zero.
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(1));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    () = sampler_stop.cancelled() => return Ok(snapshot),
                    _ = interval.tick() => {
                        let exposition = metrics.render();
                        let rendered = rendered_values(&exposition)?;
                        let types = rendered_types(&exposition)?;
                        merge_gauge_maxima(&mut snapshot.maxima, &rendered, &types)?;
                        let next = process_sample(snapshot.process.epoch)?;
                        if next.identity != snapshot.process.identity {
                            snapshot.process = ProcessSample { epoch: snapshot.process.epoch.checked_add(1).ok_or_else(|| BifrostTelemetryReportError::Parse { detail: "process epoch overflow".to_owned() })?, ..next };
                            snapshot.peak_rss_bytes = snapshot.process.rss_bytes;
                            snapshot.queue_peak = snapshot.process.queue_depth;
                        } else {
                            if next.cpu_total < snapshot.process.cpu_total || next.tokio_busy_total < snapshot.process.tokio_busy_total {
                                return Err(BifrostTelemetryReportError::CounterRegression { series: "process cumulative counters".to_owned() });
                            }
                            snapshot.peak_rss_bytes = snapshot.peak_rss_bytes.max(next.rss_bytes);
                            snapshot.queue_peak = snapshot.queue_peak.max(next.queue_depth);
                            snapshot.process = next;
                        }
                    },
                }
            }
        });
        Ok(BifrostTelemetrySampler { stop, task })
    }

    /// Build one checked production delta after stopping its gauge sampler.
    ///
    /// # Errors
    ///
    /// Returns the same errors as checkpoint/delta construction, including
    /// single-use window and counter-regression failures.
    pub async fn delta_since_with_sampler(
        &self,
        checkpoint: BifrostTelemetryCheckpoint,
        sampler: BifrostTelemetrySampler,
    ) -> Result<BifrostTelemetryDelta, BifrostTelemetryReportError> {
        sampler.stop.cancel();
        let sampled = join_sampler(sampler.task).await?;
        let current_types = checkpoint.types.clone();
        let process_start = checkpoint.process.clone();
        let mut delta = self.delta_without_sampler(checkpoint)?;
        let mut maxima = sampled.maxima;
        for sample in &delta.gauge_maxima {
            let series = rendered_series(sample);
            maxima
                .entry(series)
                .and_modify(|maximum| *maximum = maximum.max(sample.value))
                .or_insert(sample.value);
        }
        delta.gauge_maxima = maxima
            .into_iter()
            .filter(|(series, _)| supported_series(series, &current_types))
            .map(|(series, value)| parse_sample(&series, value, &current_types))
            .collect::<Result<Vec<_>, _>>()?;
        delta.process = process_window(
            &process_start,
            sampled.process,
            sampled.peak_rss_bytes,
            sampled.queue_peak,
        )?;
        Ok(delta)
    }
}

/// Join the single sampler and preserve both task and polling failures.
async fn join_sampler(
    task: tokio::task::JoinHandle<Result<SamplerSnapshot, BifrostTelemetryReportError>>,
) -> Result<SamplerSnapshot, BifrostTelemetryReportError> {
    task.await
        .map_err(|error| BifrostTelemetryReportError::SamplerJoin {
            detail: error.to_string(),
        })?
}

/// Read one real process/runtime sample without inventing per-node resources.
fn process_sample(epoch: u64) -> Result<ProcessSample, BifrostTelemetryReportError> {
    let pid = std::process::id();
    let output = std::process::Command::new("ps")
        .args(["-o", "time=,rss=", "-p", &pid.to_string()])
        .output()
        .map_err(|error| BifrostTelemetryReportError::Parse {
            detail: format!("process sample failed: {error}"),
        })?;
    if !output.status.success() {
        return Err(BifrostTelemetryReportError::Parse {
            detail: "process sample command failed".to_owned(),
        });
    }
    let rendered =
        String::from_utf8(output.stdout).map_err(|_| BifrostTelemetryReportError::Parse {
            detail: "process sample is not UTF-8".to_owned(),
        })?;
    let mut fields = rendered.split_whitespace();
    let cpu = fields
        .next()
        .ok_or_else(|| BifrostTelemetryReportError::Parse {
            detail: "process CPU sample is absent".to_owned(),
        })?;
    let rss_kib = fields
        .next()
        .ok_or_else(|| BifrostTelemetryReportError::Parse {
            detail: "process RSS sample is absent".to_owned(),
        })?
        .parse::<u64>()
        .map_err(|_| BifrostTelemetryReportError::Parse {
            detail: "process RSS sample is invalid".to_owned(),
        })?;
    let cpu_total = parse_cpu_seconds(cpu)?;
    let runtime = tokio::runtime::Handle::current().metrics();
    let tokio_busy_total = (0..runtime.num_workers())
        .map(|worker| runtime.worker_total_busy_duration(worker).as_secs_f64())
        .sum();
    Ok(ProcessSample {
        identity: format!("pid-{pid}"),
        epoch,
        cpu_total,
        rss_bytes: rss_kib
            .checked_mul(1024)
            .ok_or_else(|| BifrostTelemetryReportError::Parse {
                detail: "process RSS overflow".to_owned(),
            })?,
        tokio_busy_total,
        queue_depth: runtime.global_queue_depth() as u64,
    })
}

/// Parse the platform process CPU clock.
fn parse_cpu_seconds(rendered: &str) -> Result<f64, BifrostTelemetryReportError> {
    let fields = rendered.split(':').collect::<Vec<_>>();
    let value = match fields.as_slice() {
        [minutes, seconds] => minutes
            .parse::<f64>()
            .ok()
            .zip(seconds.parse::<f64>().ok())
            .map(|(m, s)| m * 60.0 + s),
        [hours, minutes, seconds] => hours
            .parse::<f64>()
            .ok()
            .zip(minutes.parse::<f64>().ok())
            .zip(seconds.parse::<f64>().ok())
            .map(|((h, m), s)| h * 3_600.0 + m * 60.0 + s),
        _ => None,
    };
    value
        .filter(|value| value.is_finite())
        .ok_or_else(|| BifrostTelemetryReportError::Parse {
            detail: "process CPU sample is invalid".to_owned(),
        })
}

/// Reconcile process endpoints and sampled peaks into one checked epoch window.
fn process_window(
    start: &ProcessSample,
    end: ProcessSample,
    peak_rss_bytes: u64,
    queue_peak: u64,
) -> Result<ProcessWindow, BifrostTelemetryReportError> {
    let replaced = start.identity != end.identity || start.epoch != end.epoch;
    let (cpu_start, busy_start) = if replaced {
        (0.0, 0.0)
    } else {
        (start.cpu_total, start.tokio_busy_total)
    };
    if end.cpu_total < cpu_start || end.tokio_busy_total < busy_start {
        return Err(BifrostTelemetryReportError::CounterRegression {
            series: "process cumulative counters".to_owned(),
        });
    }
    Ok(ProcessWindow {
        identity: end.identity,
        epoch: end.epoch,
        cpu_seconds: end.cpu_total - cpu_start,
        current_rss_bytes: end.rss_bytes,
        peak_rss_bytes: peak_rss_bytes.max(end.rss_bytes),
        tokio_busy_seconds: end.tokio_busy_total - busy_start,
        queue_peak: queue_peak.max(end.queue_depth),
    })
}

/// Validate the canonical closed cluster binding ledger against one capture delta.
///
/// # Errors
/// Returns [`BifrostTelemetryReportError::InvalidBinding`] for a missing family,
/// wrong kind or unit convention, unknown label, open categorical value,
/// non-finite value, or empty required histogram.
#[cfg(test)]
#[cfg(test)]
pub(crate) fn validate_cluster_bindings(
    delta: &BifrostTelemetryDelta,
) -> Result<(), BifrostTelemetryReportError> {
    evaluate_cluster_bindings(delta).map(|_| ())
}

/// Evaluate the complete closed ledger into the only typed binding-result map.
///
/// # Errors
/// Returns the exact invalid binding ID for missing or malformed evidence.
#[cfg(test)]
fn evaluate_cluster_bindings(
    delta: &BifrostTelemetryDelta,
) -> Result<BTreeMap<&'static str, EvaluatedBindingValue>, BifrostTelemetryReportError> {
    evaluate_cluster_bindings_required(
        delta,
        &CLUSTER_BINDINGS
            .iter()
            .map(|binding| binding.id.0)
            .collect::<BTreeSet<_>>(),
    )
}

/// Evaluate the ledger using one explicit closed set of required destinations.
fn evaluate_cluster_bindings_required(
    delta: &BifrostTelemetryDelta,
    required_ids: &BTreeSet<&'static str>,
) -> Result<BTreeMap<&'static str, EvaluatedBindingValue>, BifrostTelemetryReportError> {
    let mut evaluated = BTreeMap::new();
    for binding in CLUSTER_BINDINGS {
        validate_binding_labels(binding)?;
        let _aggregation = binding.aggregation;
        let _required = match binding.requirement {
            TelemetryRequirement::Always => true,
            TelemetryRequirement::Role(role) => !role.is_empty(),
        };
        validate_binding_unit(binding)?;
        let source = match binding.aggregation {
            TelemetryAggregation::Delta | TelemetryAggregation::P99 => &delta.metrics,
            TelemetryAggregation::Peak => &delta.gauge_maxima,
            TelemetryAggregation::Final => &delta.gauge_final,
        };
        let family_samples = source
            .iter()
            .filter(|sample| sample.family == binding.family)
            .collect::<Vec<_>>();
        if family_samples.iter().any(|sample| {
            sample.kind != binding.kind
                && !(binding.kind == BifrostMetricKind::HistogramBucket
                    && matches!(
                        sample.kind,
                        BifrostMetricKind::HistogramCount | BifrostMetricKind::HistogramSum
                    ))
        }) {
            return Err(invalid_binding(
                binding,
                "family contains a sample with the wrong kind",
            ));
        }
        let samples = family_samples
            .iter()
            .copied()
            .filter(|sample| sample.kind == binding.kind)
            .collect::<Vec<_>>();
        if samples.is_empty() {
            if !required_ids.contains(binding.id.0) {
                continue;
            }
            return Err(invalid_binding(
                binding,
                "required family and kind are absent",
            ));
        }
        for sample in &family_samples {
            if !sample.value.is_finite() {
                return Err(invalid_binding(binding, "sample value is not finite"));
            }
            let bucket = sample.kind == BifrostMetricKind::HistogramBucket;
            if sample.labels.keys().any(|key| {
                !(binding
                    .allowed_label_values
                    .iter()
                    .any(|domain| domain.key == key)
                    || bucket && key == "le")
            }) {
                return Err(invalid_binding(binding, "sample contains an unknown label"));
            }
            if bucket && !sample.labels.contains_key("le") {
                return Err(invalid_binding(binding, "histogram bucket omits le"));
            }
            if !bucket && sample.labels.contains_key("le") {
                return Err(invalid_binding(
                    binding,
                    "histogram count or sum contains le",
                ));
            }
            for domain in binding.allowed_label_values {
                let Some(value) = sample.labels.get(domain.key) else {
                    return Err(invalid_binding(
                        binding,
                        &format!("sample omits required label {}", domain.key),
                    ));
                };
                if !domain.values.contains(&value.as_str()) {
                    return Err(invalid_binding(
                        binding,
                        "sample contains an open label value",
                    ));
                }
            }
        }
        let p99 = if binding.aggregation == TelemetryAggregation::P99 {
            match histogram_quantile_state_for_label(delta, binding.family, "", "", 0.99) {
                Ok(Some(value)) => Some(value),
                Ok(None) if !required_ids.contains(binding.id.0) => continue,
                Ok(None) => {
                    return Err(invalid_binding(
                        binding,
                        "required histogram has no observations",
                    ));
                }
                Err(_) => {
                    return Err(invalid_binding(
                        binding,
                        "histogram window is malformed, reset, nonmonotonic, or overflow-only",
                    ));
                }
            }
        } else {
            None
        };
        let selected = samples
            .into_iter()
            .filter(|sample| {
                binding.selected_label_values.iter().all(|selected| {
                    sample
                        .labels
                        .get(selected.key)
                        .is_some_and(|value| value == selected.value)
                })
            })
            .collect::<Vec<_>>();
        if selected.is_empty() {
            if required_ids.contains(binding.id.0) {
                return Err(invalid_binding(
                    binding,
                    "required destination label series is absent",
                ));
            }
            continue;
        }
        let value = match binding.aggregation {
            TelemetryAggregation::Delta => EvaluatedBindingValue::Counter(
                selected.iter().map(|sample| sample.value).sum::<f64>() as u64,
            ),
            TelemetryAggregation::Peak => EvaluatedBindingValue::Gauge {
                final_value: 0,
                peak: selected
                    .iter()
                    .map(|sample| sample.value)
                    .fold(0.0, f64::max) as u64,
            },
            TelemetryAggregation::Final => EvaluatedBindingValue::Gauge {
                final_value: selected.iter().map(|sample| sample.value).sum::<f64>() as u64,
                peak: 0,
            },
            TelemetryAggregation::P99 => {
                EvaluatedBindingValue::DurationP99Micros(seconds_to_micros(
                    binding.id.0,
                    p99.expect("invariant: validated p99 binding retains its quantile"),
                )?)
            }
        };
        evaluated.insert(binding.id.0, value);
    }
    Ok(evaluated)
}

/// Validate one binding's categorical domains and selectors before evaluation.
///
/// # Errors
/// Returns the binding ID when a domain or selector is not closed by the ledger.
fn validate_binding_labels(binding: &TelemetryBinding) -> Result<(), BifrostTelemetryReportError> {
    for (index, domain) in binding.allowed_label_values.iter().enumerate() {
        if domain.key == "le" {
            return Err(invalid_binding(
                binding,
                "exporter-owned le cannot be a categorical domain",
            ));
        }
        if binding.allowed_label_values[..index]
            .iter()
            .any(|prior| prior.key == domain.key)
        {
            return Err(invalid_binding(
                binding,
                "categorical label domain is declared more than once",
            ));
        }
    }
    for selector in binding.selected_label_values {
        let Some(domain) = binding
            .allowed_label_values
            .iter()
            .find(|domain| domain.key == selector.key)
        else {
            return Err(invalid_binding(
                binding,
                "selector key omits its categorical domain",
            ));
        };
        if !domain.values.contains(&selector.value) {
            return Err(invalid_binding(
                binding,
                "selector value is outside its categorical domain",
            ));
        }
    }
    Ok(())
}

/// Project the exact phase counters retained by journey assertions.
#[must_use]
fn project_cluster_phase_evidence(
    bindings: &BTreeMap<&'static str, EvaluatedBindingValue>,
) -> ClusterPhaseTelemetryEvidence {
    let counter = |id: &str| match bindings.get(id) {
        Some(EvaluatedBindingValue::Counter(value)) => *value,
        _ => 0,
    };
    let gauge_final = |id: &str| match bindings.get(id) {
        Some(EvaluatedBindingValue::Gauge { final_value, .. }) => *final_value,
        _ => 0,
    };
    ClusterPhaseTelemetryEvidence {
        gate_rows: counter("gate.rows"),
        gate_bytes: counter("gate.bytes"),
        gate_success: counter("gate.requests.success"),
        gate_rejected: counter("gate.requests.rejected"),
        gate_failed: counter("gate.requests.failed"),
        gate_cancelled: counter("gate.requests.cancelled"),
        gate_query_request_success: counter("gate.requests.query_success"),
        gate_write_request_cancelled: counter("gate.requests.write_cancelled"),
        gate_query_stream_cancelled: counter("gate.query_streams.cancelled"),
        gate_query_stream_terminals: counter("gate.query_streams"),
        gate_active_streams: gauge_final("gate.active"),
        scribe_rows: counter("scribe.rows"),
        oracle_stream_rows: counter("oracle.rows"),
        oracle_stream_bytes: counter("oracle.stream_bytes"),
        forge_input_files: counter("forge.rewrite_input_files"),
        forge_input_bytes: counter("forge.rewrite_input_bytes"),
        forge_output_files: counter("forge.rewrite_output_files"),
        forge_output_bytes: counter("forge.rewrite_output_bytes"),
    }
}

impl ClusterTelemetryProjection {
    /// Evaluate one sampled delta once and construct the complete canonical evidence.
    ///
    /// Optional inactive families remain absent in typed dependency and cleanup
    /// evidence. Any present malformed family, relevant error span, invalid
    /// process sample, missing required destination, or nonzero required cleanup
    /// final fails before evidence leaves telemetry ownership.
    ///
    /// # Errors
    /// Returns [`BifrostTelemetryReportError::InvalidInterval`],
    /// [`BifrostTelemetryReportError::TopologyMismatch`], or an exact
    /// [`BifrostTelemetryReportError::InvalidBinding`] for invalid evidence.
    pub(crate) fn from_delta(
        delta: &BifrostTelemetryDelta,
        expectation: ClusterTelemetryExpectation,
    ) -> Result<ClusterTelemetryEvidence, BifrostTelemetryReportError> {
        if !delta.interval_seconds.is_finite() || delta.interval_seconds <= 0.0 {
            return Err(BifrostTelemetryReportError::InvalidInterval);
        }
        validate_expectation(&expectation)?;
        let mut required_ids = expectation
            .required_binding_ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        required_ids.extend(expectation.required_dependency_ids.iter().copied());
        required_ids.extend(expectation.required_clean_binding_ids.iter().copied());
        let bindings = evaluate_cluster_bindings_required(delta, &required_ids)?;
        let traces = project_cluster_traces(delta, expectation.required_trace_operations)?;
        let process = project_cluster_process(delta, expectation.topology)?;
        for id in expectation.required_clean_binding_ids {
            let value = binding_gauge_final(&bindings, id).ok_or_else(|| {
                BifrostTelemetryReportError::InvalidBinding {
                    id: (*id).to_owned(),
                    detail: "required cleanup final is absent".to_owned(),
                }
            })?;
            if value != 0 {
                return Err(BifrostTelemetryReportError::InvalidBinding {
                    id: (*id).to_owned(),
                    detail: format!("required cleanup final is {value}, expected zero"),
                });
            }
        }
        Ok(ClusterTelemetryEvidence {
            phase: project_cluster_phase_evidence(&bindings),
            pillars: ClusterPillarTelemetryEvidence {
                gate_accepted: binding_counter(&bindings, "gate.requests.success").unwrap_or(0),
                scribe_wal_bytes: binding_counter(&bindings, "scribe.wal_bytes").unwrap_or(0),
                forge_publications: binding_counter(&bindings, "forge.publications").unwrap_or(0),
                oracle_decoded_rows: binding_counter(&bindings, "oracle.rows").unwrap_or(0),
            },
            dependencies: ClusterDependencyTelemetryEvidence {
                postgres_pool_wait_us: binding_p99(&bindings, "postgres.acquire"),
                postgres_transactions: binding_counter(&bindings, "postgres.transactions"),
                storage_bytes: binding_counter(&bindings, "storage.bytes"),
                storage_p99_us: binding_p99(&bindings, "storage.duration"),
                wal_fsync_p99_us: binding_p99(&bindings, "wal.fsync"),
            },
            process,
            traces,
            reconciliation: ClusterReconciliationTelemetryEvidence {
                sealed_rows: binding_counter(&bindings, "scribe.seal_rows").unwrap_or(0),
                successful_queries: binding_counter(&bindings, "gate.requests.query_success")
                    .unwrap_or(0),
            },
            cleanup: ClusterCleanupTelemetryEvidence {
                gate_active: binding_gauge_final(&bindings, "gate.active"),
                scribe_ingress: binding_gauge_final(&bindings, "cleanup.scribe_ingress"),
                scribe_lane_active: binding_gauge_final(&bindings, "cleanup.scribe_lane_active"),
                scribe_lane_queued: binding_gauge_final(&bindings, "cleanup.scribe_lane_queued"),
                scribe_persistence_queue: binding_gauge_final(
                    &bindings,
                    "cleanup.scribe_persistence_queue",
                ),
                oracle_in_flight: binding_gauge_final(&bindings, "cleanup.oracle_active"),
                storage_active: binding_gauge_final(&bindings, "cleanup.storage_active"),
            },
        })
    }
}

/// Validate that an expectation references only the closed canonical ledger.
fn validate_expectation(
    expectation: &ClusterTelemetryExpectation,
) -> Result<(), BifrostTelemetryReportError> {
    for id in expectation
        .required_binding_ids
        .iter()
        .chain(expectation.required_dependency_ids)
        .chain(expectation.required_clean_binding_ids)
    {
        if !CLUSTER_BINDINGS.iter().any(|binding| binding.id.0 == *id) {
            return Err(BifrostTelemetryReportError::InvalidBinding {
                id: (*id).to_owned(),
                detail: "expectation references an unknown binding".to_owned(),
            });
        }
    }
    for id in expectation.required_dependency_ids {
        if !DEPENDENCY_BINDING_IDS.contains(id) {
            return Err(BifrostTelemetryReportError::InvalidBinding {
                id: (*id).to_owned(),
                detail: "expectation classifies a non-dependency binding as a dependency"
                    .to_owned(),
            });
        }
    }
    for id in expectation.required_clean_binding_ids {
        if !CLEANUP_BINDING_IDS.contains(id) {
            return Err(BifrostTelemetryReportError::InvalidBinding {
                id: (*id).to_owned(),
                detail: "expectation classifies a non-final binding as cleanup".to_owned(),
            });
        }
    }
    Ok(())
}

/// Return one exact counter destination without synthesizing absence.
fn binding_counter(
    bindings: &BTreeMap<&'static str, EvaluatedBindingValue>,
    id: &str,
) -> Option<u64> {
    match bindings.get(id) {
        Some(EvaluatedBindingValue::Counter(value)) => Some(*value),
        _ => None,
    }
}

/// Return one exact histogram destination without synthesizing absence.
fn binding_p99(bindings: &BTreeMap<&'static str, EvaluatedBindingValue>, id: &str) -> Option<u64> {
    match bindings.get(id) {
        Some(EvaluatedBindingValue::DurationP99Micros(value)) => Some(*value),
        _ => None,
    }
}

/// Return one exact final gauge destination without synthesizing absence.
fn binding_gauge_final(
    bindings: &BTreeMap<&'static str, EvaluatedBindingValue>,
    id: &str,
) -> Option<u64> {
    match bindings.get(id) {
        Some(EvaluatedBindingValue::Gauge { final_value, .. }) => Some(*final_value),
        _ => None,
    }
}

/// Map one exact production span name to its canonical operation.
fn cluster_trace_operation(name: &str) -> Option<ClusterTraceOperation> {
    match name {
        "bifrost.gate.write" | "bifrost.scribe.wal.append" => {
            Some(ClusterTraceOperation::DurableWrite)
        }
        "bifrost.scribe.visibility.publish" | "bifrost.forge.catalog.commit" => {
            Some(ClusterTraceOperation::FlushToVisible)
        }
        "bifrost.oracle.source" => Some(ClusterTraceOperation::QueryTimeToFirstFrame),
        "bifrost.gate.query.stream" => Some(ClusterTraceOperation::QueryTotal),
        _ => None,
    }
}

/// Build exact trace distributions and reject required absence or error status.
fn project_cluster_traces(
    delta: &BifrostTelemetryDelta,
    required: &[ClusterTraceOperation],
) -> Result<Vec<ClusterTraceTelemetryEvidence>, BifrostTelemetryReportError> {
    if let Some((operation, _)) = delta.spans.iter().find_map(|span| {
        cluster_trace_operation(&span.name)
            .filter(|_| matches!(span.status, wyrd_telemetry::CapturedSpanStatus::Error(_)))
            .map(|operation| (operation, span))
    }) {
        return Err(BifrostTelemetryReportError::InvalidBinding {
            id: operation.id().to_owned(),
            detail: "required production operation contains an error-status span".to_owned(),
        });
    }
    let mut traces = Vec::new();
    for operation in COMPLETE_TRACE_OPERATIONS {
        let matching = delta
            .spans
            .iter()
            .filter(|span| cluster_trace_operation(&span.name) == Some(*operation))
            .collect::<Vec<_>>();
        if matching.is_empty() {
            if required.contains(operation) {
                return Err(BifrostTelemetryReportError::InvalidBinding {
                    id: operation.id().to_owned(),
                    detail: "required trace operation is absent".to_owned(),
                });
            }
            continue;
        }
        let mut durations = matching
            .iter()
            .map(|span| span.duration_nanos / 1_000)
            .collect::<Vec<_>>();
        durations.sort_unstable();
        let index = durations.len().saturating_sub(1);
        traces.push(ClusterTraceTelemetryEvidence {
            operation: *operation,
            representative_trace_ids: matching
                .iter()
                .rev()
                .take(3)
                .map(|span| span.trace_id.clone())
                .collect(),
            samples: durations.len() as u64,
            p95_us: durations.get(index * 95 / 100).copied().unwrap_or(0),
            p99_us: durations.get(index * 99 / 100).copied().unwrap_or(0),
        });
    }
    Ok(traces)
}

/// Validate and project honest process evidence for the closed matrix topologies.
fn project_cluster_process(
    delta: &BifrostTelemetryDelta,
    topology: BifrostTopology,
) -> Result<ClusterProcessTelemetryEvidence, BifrostTelemetryReportError> {
    if delta.process.identity.is_empty()
        || !delta.process.cpu_seconds.is_finite()
        || delta.process.cpu_seconds < 0.0
        || !delta.process.tokio_busy_seconds.is_finite()
        || delta.process.tokio_busy_seconds < 0.0
        || delta.process.peak_rss_bytes < delta.process.current_rss_bytes
    {
        return Err(BifrostTelemetryReportError::InvalidBinding {
            id: "process.resources".to_owned(),
            detail: "process identity, counters, or sampled peaks are invalid".to_owned(),
        });
    }
    let hosted_logical_nodes = match topology {
        BifrostTopology::OnePod => vec!["server-0".to_owned()],
        BifrostTopology::ThreeServersThreeForgeWorkers => vec![
            "server-0".to_owned(),
            "server-1".to_owned(),
            "server-2".to_owned(),
            "forge-worker-0".to_owned(),
            "forge-worker-1".to_owned(),
            "forge-worker-2".to_owned(),
        ],
        _ => return Err(BifrostTelemetryReportError::TopologyMismatch),
    };
    Ok(ClusterProcessTelemetryEvidence {
        identity: delta.process.identity.clone(),
        epoch: delta.process.epoch,
        hosted_logical_nodes,
        roles: vec![
            ClusterRuntimeRole::Gate,
            ClusterRuntimeRole::Scribe,
            ClusterRuntimeRole::Forge,
            ClusterRuntimeRole::Oracle,
        ],
        cpu_seconds: delta.process.cpu_seconds,
        peak_rss_bytes: delta.process.peak_rss_bytes,
        current_rss_bytes: delta.process.current_rss_bytes,
        runtime_busy_seconds: delta.process.tokio_busy_seconds,
        runtime_queue_peak: delta.process.queue_peak,
    })
}

/// Execute one workload inside the canonical sampled telemetry window.
///
/// # Errors
/// Returns the workload error, the sampler/capture error, or both contexts when both fail.
pub(crate) async fn run_sampled_window<T, F, Fut>(
    capture: &BifrostTelemetryCapture,
    workload: F,
) -> Result<(T, BifrostTelemetryDelta), Box<dyn std::error::Error + Send + Sync>>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>>,
{
    let checkpoint = capture.checkpoint()?;
    let sampler = capture.begin_gauge_sampling(&checkpoint).await?;
    let workload_result = workload().await;
    let telemetry_result = capture.delta_since_with_sampler(checkpoint, sampler).await;
    match (workload_result, telemetry_result) {
        (Ok(value), Ok(telemetry)) => Ok((value, telemetry)),
        (Err(workload), Ok(_)) => Err(workload),
        (Ok(_), Err(telemetry)) => Err(telemetry.into()),
        (Err(workload), Err(telemetry)) => Err(format!(
            "sampled workload failed ({workload}); sampler cleanup also failed ({telemetry})"
        )
        .into()),
    }
}

/// Validate one binding's normative unit convention.
fn validate_binding_unit(binding: &TelemetryBinding) -> Result<(), BifrostTelemetryReportError> {
    let valid = match binding.unit {
        TelemetryUnit::Count => {
            !binding.family.ends_with("_seconds") && !binding.family.ends_with("_bytes_total")
        }
        TelemetryUnit::Bytes => {
            binding.family.ends_with("_bytes_total")
                || (binding.family.contains("_bytes_") && binding.family.ends_with("_total"))
        }
        TelemetryUnit::Seconds => binding.family.ends_with("_seconds"),
        TelemetryUnit::Ratio => {
            !binding.family.ends_with("_seconds") && !binding.family.ends_with("_bytes_total")
        }
    };
    if valid {
        Ok(())
    } else {
        Err(invalid_binding(
            binding,
            "family violates its normative unit suffix",
        ))
    }
}

/// Construct one stable binding failure without exposing metric values.
fn invalid_binding(binding: &TelemetryBinding, detail: &str) -> BifrostTelemetryReportError {
    BifrostTelemetryReportError::InvalidBinding {
        id: binding.id.0.to_owned(),
        detail: detail.to_owned(),
    }
}

/// Merge one rendered production scrape into its bounded non-monotonic maxima.
///
/// Counters and histogram buckets are excluded because their window deltas are
/// calculated from the checkpoint and final scrape. The map retains one number
/// per gauge series regardless of the number of polling ticks.
fn merge_gauge_maxima(
    maxima: &mut BTreeMap<String, f64>,
    rendered: &BTreeMap<String, f64>,
    types: &BTreeMap<String, PrometheusFamilyType>,
) -> Result<(), BifrostTelemetryReportError> {
    for (series, value) in rendered {
        if !supported_series(series, types) {
            continue;
        }
        if parse_sample(series, *value, types)?.kind == BifrostMetricKind::Gauge {
            maxima
                .entry(series.clone())
                .and_modify(|maximum| *maximum = maximum.max(*value))
                .or_insert(*value);
        }
    }
    Ok(())
}

/// Forge-only report mapped solely from production capture observations.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ForgeMaintenanceTelemetryReport {
    /// Production rewrite-byte counter rate converted to MiB/s.
    pub throughput_mib_per_sec: f64,
    /// Production task duration p99 in microseconds.
    pub task_latency_p99_us: f64,
    /// Maximum complete-pass backlog age in microseconds.
    pub backlog_age_us: f64,
    /// Peak Forge memory reservation gauge.
    pub peak_parent_memory: f64,
    /// Maximum final spill observation.
    pub spill_bytes: f64,
    /// Lease-contention counter delta.
    pub lease_contention: f64,
    /// Fence-loss counter delta.
    pub fence_lost: f64,
    /// Snapshot-change counter delta.
    pub snapshot_changed: f64,
    /// Maximum complete-pass fair-admission lag.
    pub fairness_lag_tasks: f64,
    /// Maximum cleanup duration in microseconds.
    pub cleanup_delay_us: f64,
    /// Production-observed active concurrency and start history by exact role.
    pub role_topology: BTreeMap<String, ForgeRoleTopologyReport>,
}

/// Production topology evidence for one configured process role.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ForgeRoleTopologyReport {
    /// Maximum active owners sampled during the observation window.
    pub max_active: u64,
    /// Active owners in the final pre-shutdown exporter snapshot.
    pub final_active: u64,
    /// Starts observed during the window, including replacements.
    pub starts: u64,
}

/// Earliest production transition that did not complete in one Forge workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ForgeCausalDiagnosis {
    /// No staging publication entered the Forge hint channel.
    NoAcceptedHint,
    /// A hint entered the channel but no durable demand was recorded.
    AcceptedHintNotPersisted,
    /// Durable demand exists but no durable task was planned.
    PersistedDemandNotPlanned,
    /// A ready task exists but no worker claimed or completed it.
    ReadyTaskNotClaimed,
    /// The durable attempt or production terminal metric reports failure.
    AttemptFailedOrRetryable,
    /// A commit completed without authoritative evidence that file debt fell.
    CommitDidNotReduceFileDebt,
    /// The production workflow committed a replacement with lower file debt.
    Converged,
}

/// Closed task strategy projected from production Forge metric labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum ForgeTelemetryStrategy {
    /// Fold staged WAL generations into Iceberg.
    StagingFold,
    /// Rewrite current-snapshot small files.
    SmallFiles,
    /// Expire old Iceberg snapshots.
    SnapshotExpiry,
}

/// Closed terminal task result projected from production Forge metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum ForgeTelemetryTaskResult {
    /// The attempt completed successfully.
    Succeeded,
    /// The attempt remains eligible for retry.
    Retryable,
    /// The attempt failed permanently.
    Failed,
    /// The attempt was cancelled before completion.
    Cancelled,
    /// Production capacity cannot schedule the attempt.
    Unschedulable,
}

/// Closed Forge maintenance stage projected from production metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum ForgeTelemetryStage {
    /// Reconcile staged audit operations.
    ReconcileStaging,
    /// Reconcile Iceberg replacement operations.
    ReconcileIceberg,
    /// Publish staged files into Iceberg.
    StagingFold,
    /// Discover current-snapshot rewrite groups.
    ManifestDiscovery,
    /// Rewrite an Iceberg data-file group.
    IcebergRewrite,
    /// Expire retained snapshots.
    SnapshotExpiry,
    /// Remove proven orphan objects.
    OrphanGc,
}

/// One nonzero terminal task histogram projected into exact count and duration.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ForgeTerminalTaskTelemetry {
    /// Production task strategy.
    pub strategy: ForgeTelemetryStrategy,
    /// Production terminal result.
    pub result: ForgeTelemetryTaskResult,
    /// Exact completed histogram observation count.
    pub count: u64,
    /// Sum of completed attempt durations in seconds.
    pub duration_seconds_sum: f64,
}

/// One nonzero stage failure with its corresponding production duration total.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ForgeStageFailureTelemetry {
    /// Production maintenance stage.
    pub stage: ForgeTelemetryStage,
    /// Exact failed-operation counter delta.
    pub count: u64,
    /// Sum of all observed stage durations in seconds.
    pub duration_seconds_sum: f64,
}

/// Aggregated current-snapshot candidate observations for one closed strategy.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ForgeDiscoveredCandidateTelemetry {
    /// Production candidate strategy.
    pub strategy: ForgeTelemetryStrategy,
    /// Number of candidate observations in the telemetry window.
    pub count: u64,
    /// Sum of files across the observed candidates.
    pub files_sum: f64,
    /// Sum of logical candidate bytes across the observed candidates.
    pub bytes_sum: f64,
}

/// Closed Forge span name retained by causal diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ForgeCausalSpanName {
    /// Hint persistence boundary.
    HintPersist,
    /// Scheduler-pass boundary.
    SchedulerPass,
    /// Worker task execution boundary.
    TaskExecute,
    /// Iceberg catalog commit boundary.
    CatalogCommit,
    /// Snapshot or object cleanup boundary.
    Cleanup,
}

/// Validated bounded projection of one captured production Forge span.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ForgeCausalSpan {
    /// Closed instrumentation name.
    pub name: ForgeCausalSpanName,
    /// Span-specific closed terminal result.
    pub result: String,
    /// Closed runtime role.
    pub role: String,
    /// Scrubbed durable task UUID when permitted by the span schema.
    pub task_id: Option<String>,
    /// Scrubbed durable attempt UUID when permitted by the span schema.
    pub attempt_id: Option<String>,
}

/// Causal Forge report mapped only from one production telemetry window.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ForgeCausalTelemetryReport {
    /// Hints accepted by the bounded publication channel.
    pub accepted_hints: u64,
    /// Hints refused because the bounded channel was full.
    pub full_hints: u64,
    /// Hints refused because the receiver was closed.
    pub closed_hints: u64,
    /// Hints durably persisted as planning demand.
    pub persisted_hints: u64,
    /// Completed hint persistence failures.
    pub failed_hint_persistence: u64,
    /// Scheduler passes that exhausted their current work page.
    pub scheduler_complete: u64,
    /// Scheduler passes bounded before exhausting their work page.
    pub scheduler_incomplete: u64,
    /// Candidate-free demand acknowledgements.
    pub demands_drained: u64,
    /// Successful task terminals that atomically requested successor planning.
    pub demands_continued: u64,
    /// Demand acknowledgement attempts fenced by a newer generation.
    pub demand_generations_changed: u64,
    /// Completed demand-transition failures.
    pub demand_transition_failures: u64,
    /// Durable worker settlements classified as deterministic data refusals.
    pub data_refusals: u64,
    /// Durable worker settlements classified as transient object-store faults.
    pub transient_object_store_failures: u64,
    /// Durable worker settlements classified as transient coordination faults.
    pub transient_coordination_failures: u64,
    /// Durable worker settlements classified as local storage-health faults.
    pub storage_health_failures: u64,
    /// Non-consuming worker settlements classified as capacity refusals.
    pub capacity_refusals: u64,
    /// Durable worker settlements classified as invariant failures.
    pub internal_invariant_failures: u64,
    /// Maximum observed number of quarantined local workers in this process.
    pub quarantined_workers: u64,
    /// Maximum observed durable planning backlog.
    pub planning_backlog: u64,
    /// Maximum observed age of the oldest demand, in seconds.
    pub oldest_demand_seconds: f64,
    /// Nonzero candidate observations in stable strategy order.
    pub discovered_candidates: Vec<ForgeDiscoveredCandidateTelemetry>,
    /// Nonzero terminal task observations in stable label order.
    pub terminal_tasks: Vec<ForgeTerminalTaskTelemetry>,
    /// Nonzero stage failures in stable stage order.
    pub stage_failures: Vec<ForgeStageFailureTelemetry>,
    /// Rewrite input-file counter delta across both production sources.
    pub rewrite_input_files: u64,
    /// Rewrite input-byte counter delta across both production sources.
    pub rewrite_input_bytes: u64,
    /// Rewrite output-file counter delta across both production sources.
    pub rewrite_output_files: u64,
    /// Rewrite output-byte counter delta across both production sources.
    pub rewrite_output_bytes: u64,
    /// Total bounded production conflict counter delta.
    pub conflicts: u64,
    /// Validated Forge spans in capture order.
    pub spans: Vec<ForgeCausalSpan>,
}

impl ForgeCausalTelemetryReport {
    /// Build a causal report from one production exporter and tracing window.
    ///
    /// This validates telemetry-internal inventory, values, span schemas, and
    /// ordering only. Durable SQL and Iceberg facts are intentionally consumed
    /// later by [`Self::diagnose`].
    ///
    /// # Errors
    ///
    /// Returns a typed report error for a missing family, open label, invalid
    /// numeric value, malformed span, or impossible telemetry ordering.
    pub fn from_production_delta(
        delta: &BifrostTelemetryDelta,
    ) -> Result<Self, BifrostTelemetryReportError> {
        ForgeCausalReportBuilder::new(delta).build()
    }

    /// Corroborate telemetry against authoritative durable Forge state.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostTelemetryReportError::InvalidBinding`] when captured
    /// telemetry and SQL/Iceberg state cannot describe the same workflow.
    pub fn diagnose(
        &self,
        workflow: &ForgeWorkflowInspection,
        rewrite: Option<&ForgeRewriteComparison>,
    ) -> Result<ForgeCausalDiagnosis, BifrostTelemetryReportError> {
        let scheduler_span = self
            .spans
            .iter()
            .any(|span| span.name == ForgeCausalSpanName::SchedulerPass);
        let task_span = self
            .spans
            .iter()
            .any(|span| span.name == ForgeCausalSpanName::TaskExecute);
        let commit_span = self.spans.iter().any(|span| {
            span.name == ForgeCausalSpanName::CatalogCommit && span.result == "succeeded"
        });
        let has_task = !workflow.tasks.is_empty();
        let unschedulable_count = workflow
            .tasks
            .iter()
            .filter(|(_, state)| state == "unschedulable")
            .count() as u64;
        let unschedulable_metric = self
            .terminal_tasks
            .iter()
            .filter(|task| task.result == ForgeTelemetryTaskResult::Unschedulable)
            .map(|task| task.count)
            .sum::<u64>();
        let executed_state = workflow.tasks.iter().any(|(_, state)| {
            matches!(
                state.as_str(),
                "claimed"
                    | "running"
                    | "prepared"
                    | "succeeded"
                    | "retryable"
                    | "failed"
                    | "cancelled"
            )
        });
        let succeeded_state = workflow.tasks.iter().any(|(_, state)| state == "succeeded");
        if has_task && !scheduler_span {
            return Err(causal_binding("durable task has no scheduler-pass span"));
        }
        if task_span && !has_task {
            return Err(causal_binding("task execution span has no durable task"));
        }
        if unschedulable_count != unschedulable_metric {
            return Err(causal_binding(
                "durable unschedulable tasks and terminal telemetry disagree",
            ));
        }
        if unschedulable_count > 0 {
            if task_span || workflow.has_demand || self.demands_continued > 0 {
                return Err(causal_binding(
                    "terminally blocked unschedulable work has execution or successor evidence",
                ));
            }
            return Ok(ForgeCausalDiagnosis::AttemptFailedOrRetryable);
        }
        if executed_state && !task_span {
            return Err(causal_binding("executed durable task has no task span"));
        }
        if commit_span && !succeeded_state {
            return Err(causal_binding(
                "catalog commit has no succeeded durable task",
            ));
        }
        if succeeded_state && !commit_span {
            return Err(causal_binding(
                "succeeded durable task has no catalog commit",
            ));
        }
        if succeeded_state && self.demands_continued == 0 {
            return Err(causal_binding(
                "succeeded durable task has no continued demand transition",
            ));
        }
        if rewrite.is_some() && !(succeeded_state && commit_span) {
            return Err(causal_binding(
                "rewrite comparison has no committed durable task",
            ));
        }

        if self.accepted_hints == 0 {
            return Ok(ForgeCausalDiagnosis::NoAcceptedHint);
        }
        if self.persisted_hints == 0 {
            return Ok(ForgeCausalDiagnosis::AcceptedHintNotPersisted);
        }
        if workflow.has_demand && !has_task {
            return Ok(ForgeCausalDiagnosis::PersistedDemandNotPlanned);
        }
        let ready = workflow.tasks.iter().any(|(_, state)| state == "ready");
        let completed_attempt = task_span || self.terminal_tasks.iter().any(|task| task.count > 0);
        if ready
            && workflow.active_claims == 0
            && workflow.active_attempts == 0
            && !completed_attempt
        {
            return Ok(ForgeCausalDiagnosis::ReadyTaskNotClaimed);
        }
        let failed_state = workflow
            .tasks
            .iter()
            .any(|(_, state)| matches!(state.as_str(), "retryable" | "failed"));
        let failed_metric = self.terminal_tasks.iter().any(|task| {
            matches!(
                task.result,
                ForgeTelemetryTaskResult::Retryable | ForgeTelemetryTaskResult::Failed
            ) && task.count > 0
        });
        if failed_state || failed_metric {
            return Ok(ForgeCausalDiagnosis::AttemptFailedOrRetryable);
        }
        if succeeded_state || commit_span {
            return match rewrite {
                Some(comparison)
                    if !comparison.input_files.is_empty()
                        && !comparison.output_files.is_empty()
                        && comparison.output_files.len() < comparison.input_files.len()
                        && workflow.active_claims == 0
                        && workflow.active_attempts == 0 =>
                {
                    Ok(ForgeCausalDiagnosis::Converged)
                }
                _ => Ok(ForgeCausalDiagnosis::CommitDidNotReduceFileDebt),
            };
        }
        Ok(ForgeCausalDiagnosis::PersistedDemandNotPlanned)
    }
}

/// Stateful mapper that enforces the causal report's closed production schema.
struct ForgeCausalReportBuilder<'a> {
    /// Single production telemetry window being projected.
    delta: &'a BifrostTelemetryDelta,
}

impl<'a> ForgeCausalReportBuilder<'a> {
    /// Bind a builder to one immutable telemetry window.
    fn new(delta: &'a BifrostTelemetryDelta) -> Self {
        Self { delta }
    }

    /// Validate and project the complete causal report.
    ///
    /// # Errors
    ///
    /// Returns a typed report error when any metric or span violates the closed
    /// production contract.
    fn build(&self) -> Result<ForgeCausalTelemetryReport, BifrostTelemetryReportError> {
        validate_causal_metric_contract(self.delta)?;
        let spans = causal_spans(self.delta)?;
        validate_causal_span_order(&spans)?;
        let accepted_hints = causal_count(
            self.delta,
            "bifrost_forge_hints_total",
            &[("result", "accepted")],
        )?;
        let persisted_hints = causal_count(
            self.delta,
            "bifrost_forge_hint_persistence_total",
            &[("result", "succeeded")],
        )?;
        let failed_hint_persistence = causal_count(
            self.delta,
            "bifrost_forge_hint_persistence_total",
            &[("result", "failed")],
        )?;
        let persistence_spans = spans
            .iter()
            .filter(|span| span.name == ForgeCausalSpanName::HintPersist)
            .count() as u64;
        if persisted_hints + failed_hint_persistence != persistence_spans {
            return Err(causal_binding(
                "hint persistence metrics and spans disagree",
            ));
        }
        if persisted_hints + failed_hint_persistence > accepted_hints {
            return Err(causal_binding(
                "completed persistence exceeds accepted hints",
            ));
        }
        let scheduler_complete = causal_count(
            self.delta,
            "bifrost_forge_scheduling_total",
            &[("result", "complete")],
        )?;
        let scheduler_incomplete = causal_count(
            self.delta,
            "bifrost_forge_scheduling_total",
            &[("result", "incomplete")],
        )?;
        let succeeded_scheduler = spans.iter().any(|span| {
            span.name == ForgeCausalSpanName::SchedulerPass && span.result == "succeeded"
        });
        if (scheduler_complete + scheduler_incomplete > 0) != succeeded_scheduler {
            return Err(causal_binding(
                "scheduler counters and succeeded span disagree",
            ));
        }
        Ok(ForgeCausalTelemetryReport {
            accepted_hints,
            full_hints: causal_count(
                self.delta,
                "bifrost_forge_hints_total",
                &[("result", "full")],
            )?,
            closed_hints: causal_count(
                self.delta,
                "bifrost_forge_hints_total",
                &[("result", "closed")],
            )?,
            persisted_hints,
            failed_hint_persistence,
            scheduler_complete,
            scheduler_incomplete,
            demands_drained: causal_count(
                self.delta,
                "bifrost_forge_demand_transitions_total",
                &[("result", "drained")],
            )?,
            demands_continued: causal_count(
                self.delta,
                "bifrost_forge_demand_transitions_total",
                &[("result", "continued")],
            )?,
            demand_generations_changed: causal_count(
                self.delta,
                "bifrost_forge_demand_transitions_total",
                &[("result", "generation_changed")],
            )?,
            demand_transition_failures: causal_count(
                self.delta,
                "bifrost_forge_demand_transitions_total",
                &[("result", "failed")],
            )?,
            data_refusals: causal_count(
                self.delta,
                "bifrost_forge_task_failures_total",
                &[("failure_class", "data_refusal")],
            )?,
            transient_object_store_failures: causal_count(
                self.delta,
                "bifrost_forge_task_failures_total",
                &[("failure_class", "transient_object_store")],
            )?,
            transient_coordination_failures: causal_count(
                self.delta,
                "bifrost_forge_task_failures_total",
                &[("failure_class", "transient_coordination")],
            )?,
            storage_health_failures: causal_count(
                self.delta,
                "bifrost_forge_task_failures_total",
                &[("failure_class", "storage_health")],
            )?,
            capacity_refusals: causal_count(
                self.delta,
                "bifrost_forge_task_failures_total",
                &[("failure_class", "capacity_refused")],
            )?,
            internal_invariant_failures: causal_count(
                self.delta,
                "bifrost_forge_task_failures_total",
                &[("failure_class", "internal_invariant")],
            )?,
            quarantined_workers: causal_gauge_count(
                self.delta,
                "bifrost_forge_worker_quarantined",
            )?,
            planning_backlog: causal_gauge_count(self.delta, "bifrost_forge_planning_backlog")?,
            oldest_demand_seconds: causal_gauge(
                self.delta,
                "bifrost_forge_oldest_backlog_seconds",
            )?,
            discovered_candidates: causal_discovered_candidates(self.delta)?,
            terminal_tasks: causal_terminal_tasks(self.delta)?,
            stage_failures: causal_stage_failures(self.delta)?,
            rewrite_input_files: causal_source_total(
                self.delta,
                "bifrost_forge_rewrite_input_files_total",
            )?,
            rewrite_input_bytes: causal_source_total(
                self.delta,
                "bifrost_forge_rewrite_input_bytes_total",
            )?,
            rewrite_output_files: causal_source_total(
                self.delta,
                "bifrost_forge_rewrite_output_files_total",
            )?,
            rewrite_output_bytes: causal_source_total(
                self.delta,
                "bifrost_forge_rewrite_output_bytes_total",
            )?,
            conflicts: ["lease_contention", "fence_lost", "snapshot_changed"]
                .into_iter()
                .try_fold(0_u64, |total, kind| {
                    causal_count(
                        self.delta,
                        "bifrost_forge_conflicts_total",
                        &[("kind", kind)],
                    )
                    .and_then(|value| {
                        total
                            .checked_add(value)
                            .ok_or_else(|| causal_binding("conflict count overflow"))
                    })
                })?,
            spans,
        })
    }
}

/// Construct the stable private binding error used by causal corroboration.
fn causal_binding(detail: &str) -> BifrostTelemetryReportError {
    BifrostTelemetryReportError::InvalidBinding {
        id: "forge_causal_workflow".to_owned(),
        detail: detail.to_owned(),
    }
}

/// Validate labels, kinds, and finite non-negative values for causal families.
///
/// # Errors
///
/// Returns a typed report error when a required family is absent or any sample
/// violates its closed schema.
fn validate_causal_metric_contract(
    delta: &BifrostTelemetryDelta,
) -> Result<(), BifrostTelemetryReportError> {
    let required = [
        "bifrost_forge_hints_total",
        "bifrost_forge_hint_persistence_total",
        "bifrost_forge_hint_persistence_seconds",
        "bifrost_forge_scheduling_total",
        "bifrost_forge_demand_transitions_total",
        "bifrost_forge_task_failures_total",
        "bifrost_forge_worker_quarantined",
        "bifrost_forge_discovered_candidate_files",
        "bifrost_forge_discovered_candidate_bytes",
        "bifrost_forge_planning_demand_total",
        "bifrost_forge_planning_backlog",
        "bifrost_forge_oldest_backlog_seconds",
        "bifrost_forge_task_duration_seconds",
        "bifrost_forge_stage_failures_total",
        "bifrost_forge_stage_seconds",
        "bifrost_forge_rewrite_input_files_total",
        "bifrost_forge_rewrite_input_bytes_total",
        "bifrost_forge_rewrite_output_files_total",
        "bifrost_forge_rewrite_output_bytes_total",
        "bifrost_forge_conflicts_total",
    ];
    for family in required {
        if !delta.families.contains(family) {
            return Err(BifrostTelemetryReportError::MissingSeries {
                family: family.to_owned(),
            });
        }
    }
    for sample in delta
        .metrics
        .iter()
        .chain(&delta.gauge_maxima)
        .chain(&delta.gauge_final)
    {
        if !required.contains(&sample.family.as_str()) {
            continue;
        }
        if !sample.value.is_finite() || sample.value < 0.0 {
            return Err(causal_binding(
                "causal metric value is negative or non-finite",
            ));
        }
        let (allowed, categorical): (&[&str], &[(&str, &[&str])]) = match sample.family.as_str() {
            "bifrost_forge_hints_total" => {
                (&["result"], &[("result", &["accepted", "full", "closed"])])
            }
            "bifrost_forge_hint_persistence_total" | "bifrost_forge_hint_persistence_seconds" => {
                (&["result", "le"], &[("result", &["succeeded", "failed"])])
            }
            "bifrost_forge_scheduling_total" => {
                (&["result"], &[("result", &["complete", "incomplete"])])
            }
            "bifrost_forge_demand_transitions_total" => (
                &["result"],
                &[(
                    ("result"),
                    &["drained", "continued", "generation_changed", "failed"],
                )],
            ),
            "bifrost_forge_task_failures_total" => (
                &["failure_class"],
                &[(
                    "failure_class",
                    &[
                        "data_refusal",
                        "transient_object_store",
                        "transient_coordination",
                        "storage_health",
                        "capacity_refused",
                        "internal_invariant",
                    ],
                )],
            ),
            "bifrost_forge_worker_quarantined" => (&[], &[]),
            "bifrost_forge_discovered_candidate_files"
            | "bifrost_forge_discovered_candidate_bytes" => (
                &["strategy", "le"],
                &[(
                    "strategy",
                    &["staging_fold", "small_files", "snapshot_expiry"],
                )],
            ),
            "bifrost_forge_planning_demand_total" => {
                (&["source"], &[("source", &["hint", "roster_repair"])])
            }
            "bifrost_forge_task_duration_seconds" => (
                &["strategy", "result", "le"],
                &[
                    (
                        "strategy",
                        &["staging_fold", "small_files", "snapshot_expiry"],
                    ),
                    (
                        "result",
                        &[
                            "succeeded",
                            "retryable",
                            "failed",
                            "cancelled",
                            "unschedulable",
                        ],
                    ),
                ],
            ),
            "bifrost_forge_stage_failures_total" | "bifrost_forge_stage_seconds" => (
                &["stage", "le"],
                &[(
                    "stage",
                    &[
                        "reconcile_staging",
                        "reconcile_iceberg",
                        "staging_fold",
                        "manifest_discovery",
                        "iceberg_rewrite",
                        "snapshot_expiry",
                        "orphan_gc",
                    ],
                )],
            ),
            "bifrost_forge_rewrite_input_files_total"
            | "bifrost_forge_rewrite_input_bytes_total"
            | "bifrost_forge_rewrite_output_files_total"
            | "bifrost_forge_rewrite_output_bytes_total" => {
                (&["source"], &[("source", &["staging", "iceberg"])])
            }
            "bifrost_forge_conflicts_total" => (
                &["kind"],
                &[(
                    "kind",
                    &["lease_contention", "fence_lost", "snapshot_changed"],
                )],
            ),
            _ => (&[], &[]),
        };
        validate_sample_labels(sample, allowed, categorical)?;
    }
    Ok(())
}

/// Return one exact non-negative integer counter or histogram count.
///
/// # Errors
///
/// Returns a typed report error when the series is absent, non-integral, or
/// outside `u64`.
fn causal_count(
    delta: &BifrostTelemetryDelta,
    family: &str,
    labels: &[(&str, &str)],
) -> Result<u64, BifrostTelemetryReportError> {
    let value = delta
        .metrics
        .iter()
        .filter(|sample| {
            sample.family == family
                && sample.kind != BifrostMetricKind::HistogramBucket
                && sample.kind != BifrostMetricKind::HistogramSum
                && labels
                    .iter()
                    .all(|(key, value)| sample.labels.get(*key).map(String::as_str) == Some(*value))
        })
        .map(|sample| sample.value)
        .sum::<f64>();
    checked_causal_u64(value, family)
}

/// Convert one exact telemetry count to `u64`.
///
/// # Errors
///
/// Returns a typed binding error for a negative, non-finite, fractional, or
/// overflowing value.
fn checked_causal_u64(value: f64, family: &str) -> Result<u64, BifrostTelemetryReportError> {
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > u64::MAX as f64 {
        return Err(causal_binding(&format!("{family} is not an exact u64")));
    }
    Ok(value as u64)
}

/// Return the maximum production gauge as an exact count.
///
/// # Errors
///
/// Returns a typed report error when the gauge is absent or not an exact count.
fn causal_gauge_count(
    delta: &BifrostTelemetryDelta,
    family: &str,
) -> Result<u64, BifrostTelemetryReportError> {
    checked_causal_u64(causal_gauge(delta, family)?, family)
}

/// Return the maximum finite non-negative production gauge.
///
/// # Errors
///
/// Returns a typed report error when the required gauge is absent or invalid.
fn causal_gauge(
    delta: &BifrostTelemetryDelta,
    family: &str,
) -> Result<f64, BifrostTelemetryReportError> {
    let value = delta
        .gauge_maxima
        .iter()
        .filter(|sample| sample.family == family)
        .map(|sample| sample.value)
        .reduce(f64::max)
        .ok_or_else(|| BifrostTelemetryReportError::MissingSeries {
            family: family.to_owned(),
        })?;
    if !value.is_finite() || value < 0.0 {
        return Err(causal_binding("causal gauge is negative or non-finite"));
    }
    Ok(value)
}

/// Sum one source-labeled production counter across its closed inventory.
///
/// # Errors
///
/// Returns a typed report error when either source series is malformed or the
/// sum overflows.
fn causal_source_total(
    delta: &BifrostTelemetryDelta,
    family: &str,
) -> Result<u64, BifrostTelemetryReportError> {
    ["staging", "iceberg"]
        .into_iter()
        .try_fold(0_u64, |total, source| {
            causal_count(delta, family, &[("source", source)]).and_then(|value| {
                total
                    .checked_add(value)
                    .ok_or_else(|| causal_binding("rewrite counter overflow"))
            })
        })
}

/// Project nonzero task terminal histograms in stable label order.
///
/// # Errors
///
/// Returns a typed report error for malformed histogram count or sum samples.
fn causal_terminal_tasks(
    delta: &BifrostTelemetryDelta,
) -> Result<Vec<ForgeTerminalTaskTelemetry>, BifrostTelemetryReportError> {
    let strategies = [
        ("staging_fold", ForgeTelemetryStrategy::StagingFold),
        ("small_files", ForgeTelemetryStrategy::SmallFiles),
        ("snapshot_expiry", ForgeTelemetryStrategy::SnapshotExpiry),
    ];
    let results = [
        ("succeeded", ForgeTelemetryTaskResult::Succeeded),
        ("retryable", ForgeTelemetryTaskResult::Retryable),
        ("failed", ForgeTelemetryTaskResult::Failed),
        ("cancelled", ForgeTelemetryTaskResult::Cancelled),
        ("unschedulable", ForgeTelemetryTaskResult::Unschedulable),
    ];
    let mut rows = Vec::new();
    for (strategy_label, strategy) in strategies {
        for (result_label, result) in results {
            let labels = [("strategy", strategy_label), ("result", result_label)];
            let count = causal_count(delta, "bifrost_forge_task_duration_seconds", &labels)?;
            if count > 0 {
                rows.push(ForgeTerminalTaskTelemetry {
                    strategy,
                    result,
                    count,
                    duration_seconds_sum: causal_histogram_sum(
                        delta,
                        "bifrost_forge_task_duration_seconds",
                        &labels,
                    )?,
                });
            }
        }
    }
    Ok(rows)
}

/// Project nonzero candidate discovery histograms in stable strategy order.
///
/// # Errors
///
/// Returns a typed report error for malformed histogram count or sum samples.
fn causal_discovered_candidates(
    delta: &BifrostTelemetryDelta,
) -> Result<Vec<ForgeDiscoveredCandidateTelemetry>, BifrostTelemetryReportError> {
    let strategies = [
        ("staging_fold", ForgeTelemetryStrategy::StagingFold),
        ("small_files", ForgeTelemetryStrategy::SmallFiles),
        ("snapshot_expiry", ForgeTelemetryStrategy::SnapshotExpiry),
    ];
    let mut rows = Vec::new();
    for (label, strategy) in strategies {
        let labels = [("strategy", label)];
        let files_count = causal_count(delta, "bifrost_forge_discovered_candidate_files", &labels)?;
        let bytes_count = causal_count(delta, "bifrost_forge_discovered_candidate_bytes", &labels)?;
        if files_count != bytes_count {
            return Err(causal_binding(
                "candidate file and byte observation counts disagree",
            ));
        }
        if files_count > 0 {
            rows.push(ForgeDiscoveredCandidateTelemetry {
                strategy,
                count: files_count,
                files_sum: causal_histogram_sum(
                    delta,
                    "bifrost_forge_discovered_candidate_files",
                    &labels,
                )?,
                bytes_sum: causal_histogram_sum(
                    delta,
                    "bifrost_forge_discovered_candidate_bytes",
                    &labels,
                )?,
            });
        }
    }
    Ok(rows)
}

/// Project nonzero stage failure counters in stable stage order.
///
/// # Errors
///
/// Returns a typed report error for malformed failure or duration samples.
fn causal_stage_failures(
    delta: &BifrostTelemetryDelta,
) -> Result<Vec<ForgeStageFailureTelemetry>, BifrostTelemetryReportError> {
    let stages = [
        ("reconcile_staging", ForgeTelemetryStage::ReconcileStaging),
        ("reconcile_iceberg", ForgeTelemetryStage::ReconcileIceberg),
        ("staging_fold", ForgeTelemetryStage::StagingFold),
        ("manifest_discovery", ForgeTelemetryStage::ManifestDiscovery),
        ("iceberg_rewrite", ForgeTelemetryStage::IcebergRewrite),
        ("snapshot_expiry", ForgeTelemetryStage::SnapshotExpiry),
        ("orphan_gc", ForgeTelemetryStage::OrphanGc),
    ];
    let mut rows = Vec::new();
    for (label, stage) in stages {
        let labels = [("stage", label)];
        let count = causal_count(delta, "bifrost_forge_stage_failures_total", &labels)?;
        if count > 0 {
            rows.push(ForgeStageFailureTelemetry {
                stage,
                count,
                duration_seconds_sum: causal_histogram_sum(
                    delta,
                    "bifrost_forge_stage_seconds",
                    &labels,
                )?,
            });
        }
    }
    Ok(rows)
}

/// Return a finite non-negative histogram sum for exact labels.
///
/// # Errors
///
/// Returns a typed report error when the sum is absent or numerically invalid.
fn causal_histogram_sum(
    delta: &BifrostTelemetryDelta,
    family: &str,
    labels: &[(&str, &str)],
) -> Result<f64, BifrostTelemetryReportError> {
    let value = delta
        .metrics
        .iter()
        .filter(|sample| {
            sample.family == family
                && sample.kind == BifrostMetricKind::HistogramSum
                && labels
                    .iter()
                    .all(|(key, value)| sample.labels.get(*key).map(String::as_str) == Some(*value))
        })
        .map(|sample| sample.value)
        .sum::<f64>();
    if !value.is_finite() || value < 0.0 {
        return Err(causal_binding("histogram sum is negative or non-finite"));
    }
    Ok(value)
}

/// Validate and project captured Forge spans without changing capture order.
///
/// # Errors
///
/// Returns a typed span error for an unknown name, attribute, closed value, or
/// malformed UUID.
fn causal_spans(
    delta: &BifrostTelemetryDelta,
) -> Result<Vec<ForgeCausalSpan>, BifrostTelemetryReportError> {
    let mut projected = Vec::new();
    for span in &delta.spans {
        let (name, expected, results, roles, ids) = match span.name.as_str() {
            "bifrost.forge.hint.persist" => (
                ForgeCausalSpanName::HintPersist,
                &["result", "role"][..],
                &["succeeded", "failed"][..],
                &["server"][..],
                false,
            ),
            "bifrost.forge.scheduler.pass" => (
                ForgeCausalSpanName::SchedulerPass,
                &["result", "role"][..],
                &["succeeded", "failed", "standby"][..],
                &["server"][..],
                false,
            ),
            "bifrost.forge.task.execute" => (
                ForgeCausalSpanName::TaskExecute,
                &["attempt_id", "result", "role", "strategy", "task_id"][..],
                &[
                    "succeeded",
                    "retryable",
                    "failed",
                    "cancelled",
                    "unschedulable",
                ][..],
                &["forge_worker"][..],
                true,
            ),
            "bifrost.forge.catalog.commit" => (
                ForgeCausalSpanName::CatalogCommit,
                &["attempt_id", "result", "role", "strategy", "task_id"][..],
                &["succeeded", "failed", "timed_out", "cancelled"][..],
                &["forge_worker"][..],
                true,
            ),
            "bifrost.forge.cleanup" => (
                ForgeCausalSpanName::Cleanup,
                &[
                    "attempt_id",
                    "kind",
                    "result",
                    "role",
                    "strategy",
                    "task_id",
                ][..],
                &["succeeded", "failed"][..],
                &["forge_worker"][..],
                true,
            ),
            name if name.starts_with("bifrost.forge.") => {
                return Err(BifrostTelemetryReportError::InvalidSpan {
                    span: name.to_owned(),
                    detail: "unexpected Forge instrumentation name".to_owned(),
                });
            }
            _ => continue,
        };
        validate_span_attribute_set(span, expected)?;
        validate_closed_span_attribute(span, "result", results)?;
        validate_closed_span_attribute(span, "role", roles)?;
        if matches!(
            name,
            ForgeCausalSpanName::TaskExecute | ForgeCausalSpanName::CatalogCommit
        ) {
            validate_closed_span_attribute(
                span,
                "strategy",
                if name == ForgeCausalSpanName::CatalogCommit {
                    &["staging_fold", "small_files"]
                } else {
                    &["staging_fold", "small_files", "snapshot_expiry"]
                },
            )?;
        } else if name == ForgeCausalSpanName::Cleanup {
            validate_closed_span_attribute(span, "strategy", &["snapshot_expiry"])?;
            validate_closed_span_attribute(span, "kind", &["expired"])?;
        }
        let (task_id, attempt_id) = if ids {
            validate_span_uuid(span, "task_id")?;
            validate_span_uuid(span, "attempt_id")?;
            (
                span.attributes.get("task_id").cloned(),
                span.attributes.get("attempt_id").cloned(),
            )
        } else {
            (None, None)
        };
        projected.push(ForgeCausalSpan {
            name,
            result: span.attributes["result"].clone(),
            role: span.attributes["role"].clone(),
            task_id,
            attempt_id,
        });
    }
    Ok(projected)
}

/// Enforce causal prerequisites among captured production Forge spans.
///
/// Exporters may deliver concurrently completed spans in a different vector
/// order than their start times, so causality is proven by the presence of the
/// required upstream span rather than the capture vector position.
///
/// # Errors
///
/// Returns a typed binding error when a downstream span appears before its
/// required upstream production transition.
fn validate_causal_span_order(
    spans: &[ForgeCausalSpan],
) -> Result<(), BifrostTelemetryReportError> {
    let scheduler_seen = spans
        .iter()
        .any(|span| span.name == ForgeCausalSpanName::SchedulerPass);
    let succeeded_task_seen = spans
        .iter()
        .any(|span| span.name == ForgeCausalSpanName::TaskExecute && span.result == "succeeded");
    for span in spans {
        match span.name {
            ForgeCausalSpanName::TaskExecute => {
                if !scheduler_seen {
                    return Err(causal_binding("task span has no scheduler span"));
                }
            }
            ForgeCausalSpanName::CatalogCommit if !succeeded_task_seen => {
                return Err(causal_binding("catalog commit has no succeeded task span"));
            }
            ForgeCausalSpanName::HintPersist
            | ForgeCausalSpanName::SchedulerPass
            | ForgeCausalSpanName::CatalogCommit
            | ForgeCausalSpanName::Cleanup => {}
        }
    }
    Ok(())
}

impl ForgeMaintenanceTelemetryReport {
    /// Map every Forge-only R13 field from one production exporter window.
    ///
    /// # Errors
    ///
    /// Returns a typed error when any required production family, activity
    /// counter, histogram, or expected active role is absent or stale.
    pub fn from_production_delta(
        delta: &BifrostTelemetryDelta,
        expected_roles: &BTreeMap<String, u64>,
    ) -> Result<Self, BifrostTelemetryReportError> {
        validate_forge_span_contract(delta)?;
        validate_forge_label_contract(delta)?;
        let role_topology = validate_role_topology(delta, expected_roles)?;
        require_advanced(delta, "bifrost_forge_complete_gauge_publications_total")?;
        require_advanced(delta, "bifrost_memory_reservations_total")?;
        let rewrite_bytes = sum(delta, "bifrost_forge_rewrite_output_bytes_total", &[])?;
        let task_latency_p99_us = histogram_quantile_for_label(
            delta,
            "bifrost_forge_task_duration_seconds",
            "result",
            "succeeded",
            0.99,
        )? * 1_000_000.0;
        let spill_bytes = histogram_quantile(delta, "bifrost_forge_task_spill_bytes", 1.0)?;
        let cleanup_delay_us =
            histogram_quantile(delta, "bifrost_forge_cleanup_duration_seconds", 1.0)? * 1_000_000.0;
        let backlog_age_us =
            gauge(delta, "bifrost_forge_oldest_backlog_seconds", &[])? * 1_000_000.0;
        let peak_parent_memory = gauge(
            delta,
            "bifrost_memory_reserved_bytes",
            &[("consumer", "forge")],
        )?;
        let fairness_lag_tasks = gauge(delta, "bifrost_forge_fairness_lag_tasks", &[])?;
        Ok(Self {
            throughput_mib_per_sec: rewrite_bytes / (1024.0 * 1024.0 * delta.interval_seconds),
            task_latency_p99_us,
            backlog_age_us,
            peak_parent_memory,
            spill_bytes,
            lease_contention: sum(
                delta,
                "bifrost_forge_conflicts_total",
                &[("kind", "lease_contention")],
            )?,
            fence_lost: sum(
                delta,
                "bifrost_forge_conflicts_total",
                &[("kind", "fence_lost")],
            )?,
            snapshot_changed: sum(
                delta,
                "bifrost_forge_conflicts_total",
                &[("kind", "snapshot_changed")],
            )?,
            fairness_lag_tasks,
            cleanup_delay_us,
            role_topology,
        })
    }
}

/// Validate all exact-run Forge spans against their closed production schemas.
///
/// Every captured Forge-prefixed span must be one of the four normative names,
/// and every instance must carry its required owner-authored attributes. Known
/// tracing-provider semantic attributes are tolerated because they are added
/// after owner instrumentation; task and attempt UUIDs remain the sole
/// permitted owner-authored high-cardinality values.
///
/// # Errors
///
/// Returns [`BifrostTelemetryReportError::MissingSpan`] when a required owner did
/// not emit, or [`BifrostTelemetryReportError::InvalidSpan`] for renamed spans,
/// missing/extra attributes, open values, or malformed scrubbed UUIDs.
fn validate_forge_span_contract(
    delta: &BifrostTelemetryDelta,
) -> Result<(), BifrostTelemetryReportError> {
    let required = [
        "bifrost.forge.scheduler.pass",
        "bifrost.forge.task.execute",
        "bifrost.forge.catalog.commit",
        "bifrost.forge.cleanup",
    ];
    let mut seen = BTreeSet::new();
    for span in &delta.spans {
        match span.name.as_str() {
            "bifrost.forge.scheduler.pass" => {
                validate_span_attribute_set(span, &["result", "role"])?;
                validate_closed_span_attribute(span, "result", &["succeeded", "failed"])?;
                validate_closed_span_attribute(span, "role", &["server"])?;
            }
            "bifrost.forge.task.execute" => {
                validate_span_attribute_set(
                    span,
                    &["attempt_id", "result", "role", "strategy", "task_id"],
                )?;
                validate_closed_span_attribute(
                    span,
                    "strategy",
                    &["staging_fold", "small_files", "snapshot_expiry"],
                )?;
                validate_closed_span_attribute(
                    span,
                    "result",
                    &[
                        "succeeded",
                        "retryable",
                        "failed",
                        "cancelled",
                        "unschedulable",
                    ],
                )?;
                validate_closed_span_attribute(span, "role", &["forge_worker"])?;
                validate_span_uuid(span, "task_id")?;
                validate_span_uuid(span, "attempt_id")?;
            }
            "bifrost.forge.catalog.commit" => {
                validate_span_attribute_set(
                    span,
                    &["attempt_id", "result", "role", "strategy", "task_id"],
                )?;
                validate_closed_span_attribute(span, "strategy", &["staging_fold", "small_files"])?;
                validate_closed_span_attribute(
                    span,
                    "result",
                    &["succeeded", "failed", "timed_out", "cancelled"],
                )?;
                validate_closed_span_attribute(span, "role", &["forge_worker"])?;
                validate_span_uuid(span, "task_id")?;
                validate_span_uuid(span, "attempt_id")?;
            }
            "bifrost.forge.cleanup" => {
                validate_span_attribute_set(
                    span,
                    &[
                        "attempt_id",
                        "kind",
                        "result",
                        "role",
                        "strategy",
                        "task_id",
                    ],
                )?;
                validate_closed_span_attribute(span, "kind", &["expired"])?;
                validate_closed_span_attribute(span, "strategy", &["snapshot_expiry"])?;
                validate_closed_span_attribute(span, "result", &["succeeded", "failed"])?;
                validate_closed_span_attribute(span, "role", &["forge_worker"])?;
                validate_span_uuid(span, "task_id")?;
                validate_span_uuid(span, "attempt_id")?;
            }
            name if name.starts_with("bifrost.forge.") => {
                return Err(BifrostTelemetryReportError::InvalidSpan {
                    span: name.to_owned(),
                    detail: "unexpected Forge instrumentation name".to_owned(),
                });
            }
            _ => continue,
        }
        seen.insert(span.name.as_str());
    }
    for name in required {
        if !seen.contains(name) {
            return Err(BifrostTelemetryReportError::MissingSpan {
                span: name.to_owned(),
            });
        }
    }
    Ok(())
}

/// Require one captured span to expose its approved owner-authored attributes.
///
/// The tracing provider may append its standard source, thread, and timing
/// attributes. All other additions remain invalid so owner instrumentation
/// cannot leak tenant data, object paths, SQL text, or error text.
///
/// # Errors
///
/// Returns [`BifrostTelemetryReportError::InvalidSpan`] when a required field is
/// absent or any prohibited, sensitive, or otherwise unapproved field appears.
fn validate_span_attribute_set(
    span: &CapturedSpan,
    expected: &[&str],
) -> Result<(), BifrostTelemetryReportError> {
    let actual = span
        .attributes
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    let provider = [
        "busy_ns",
        "code.file.path",
        "code.filepath",
        "code.line.number",
        "code.lineno",
        "code.module.name",
        "code.namespace",
        "idle_ns",
        "thread.id",
        "thread.name",
        "target",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let missing = expected.difference(&actual).copied().collect::<Vec<_>>();
    let unexpected = actual
        .difference(&expected)
        .filter(|key| !provider.contains(**key))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() || !unexpected.is_empty() {
        return Err(BifrostTelemetryReportError::InvalidSpan {
            span: span.name.clone(),
            detail: format!(
                "closed attribute contract has missing keys {missing:?} and unexpected keys {unexpected:?}"
            ),
        });
    }
    Ok(())
}

/// Validate one fixed-cardinality span attribute against its closed values.
///
/// # Errors
///
/// Returns [`BifrostTelemetryReportError::InvalidSpan`] when the attribute is
/// absent or its value is outside the approved set.
fn validate_closed_span_attribute(
    span: &CapturedSpan,
    key: &str,
    allowed: &[&str],
) -> Result<(), BifrostTelemetryReportError> {
    let valid = span
        .attributes
        .get(key)
        .is_some_and(|value| allowed.contains(&value.as_str()));
    if !valid {
        return Err(BifrostTelemetryReportError::InvalidSpan {
            span: span.name.clone(),
            detail: format!("attribute {key} is absent or outside its closed values"),
        });
    }
    Ok(())
}

/// Validate one permitted high-cardinality field as a scrubbed UUID.
///
/// # Errors
///
/// Returns [`BifrostTelemetryReportError::InvalidSpan`] when the field is absent
/// or is not a canonical UUID value.
fn validate_span_uuid(span: &CapturedSpan, key: &str) -> Result<(), BifrostTelemetryReportError> {
    let valid = span
        .attributes
        .get(key)
        .and_then(|value| value.parse::<uuid::Uuid>().ok())
        .is_some();
    if !valid {
        return Err(BifrostTelemetryReportError::InvalidSpan {
            span: span.name.clone(),
            detail: format!("attribute {key} is not a scrubbed UUID"),
        });
    }
    Ok(())
}

/// Server-only query report mapped from its production query histogram.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BifrostQueryTelemetryReport {
    /// Successful public query duration p99 in microseconds.
    pub query_latency_p99_us: f64,
}

impl BifrostQueryTelemetryReport {
    /// Map the server-integrated query field from production telemetry only.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostTelemetryReportError::EmptyHistogram`] when no successful
    /// query completed in the capture window.
    pub fn from_server_delta(
        delta: &BifrostTelemetryDelta,
    ) -> Result<Self, BifrostTelemetryReportError> {
        validate_query_label_contract(delta)?;
        Ok(Self {
            query_latency_p99_us: histogram_quantile_for_label(
                delta,
                "bifrost_query_duration_seconds",
                "result",
                "success",
                0.99,
            )? * 1_000_000.0,
        })
    }
}

/// Reject unexpected or open-cardinality labels on the closed Forge report families.
fn validate_forge_label_contract(
    delta: &BifrostTelemetryDelta,
) -> Result<(), BifrostTelemetryReportError> {
    for sample in delta
        .metrics
        .iter()
        .chain(&delta.gauge_maxima)
        .chain(&delta.gauge_final)
    {
        let (allowed, categorical): (&[&str], &[(&str, &[&str])]) = match sample.family.as_str() {
            "bifrost_forge_rewrite_output_bytes_total" => {
                (&["source"], &[("source", &["staging", "iceberg"])])
            }
            "bifrost_forge_task_duration_seconds" => (
                &["strategy", "result", "le"],
                &[
                    (
                        "strategy",
                        &["staging_fold", "small_files", "snapshot_expiry"],
                    ),
                    (
                        "result",
                        &[
                            "succeeded",
                            "retryable",
                            "failed",
                            "cancelled",
                            "unschedulable",
                        ],
                    ),
                ],
            ),
            "bifrost_forge_oldest_backlog_seconds"
            | "bifrost_forge_fairness_lag_tasks"
            | "bifrost_forge_complete_gauge_publications_total"
            | "bifrost_forge_worker_quarantined" => (&[], &[]),
            "bifrost_forge_task_failures_total" => (
                &["failure_class"],
                &[(
                    "failure_class",
                    &[
                        "data_refusal",
                        "transient_object_store",
                        "storage_health",
                        "capacity_refused",
                    ],
                )],
            ),
            "bifrost_memory_reserved_bytes" => (&["consumer"], &[]),
            "bifrost_memory_reservations_total" => (
                &["consumer", "outcome"],
                &[("outcome", &["accepted", "rejected"])],
            ),
            "bifrost_forge_task_spill_bytes" => (
                &["strategy", "le"],
                &[(
                    "strategy",
                    &["staging_fold", "small_files", "snapshot_expiry"],
                )],
            ),
            "bifrost_forge_conflicts_total" => (
                &["kind"],
                &[(
                    "kind",
                    &["lease_contention", "fence_lost", "snapshot_changed"],
                )],
            ),
            "bifrost_forge_cleanup_duration_seconds" => (
                &["kind", "le"],
                &[("kind", &["expired", "orphan", "spill"])],
            ),
            "bifrost_forge_role_processes" | "bifrost_forge_role_process_started_total" => {
                (&["role"], &[("role", &["all", "server", "forge_worker"])])
            }
            _ => continue,
        };
        validate_sample_labels(sample, allowed, categorical)?;
    }
    Ok(())
}

/// Reject unexpected labels or categorical values on the server query histogram.
fn validate_query_label_contract(
    delta: &BifrostTelemetryDelta,
) -> Result<(), BifrostTelemetryReportError> {
    for sample in delta
        .metrics
        .iter()
        .filter(|sample| sample.family == "bifrost_query_duration_seconds")
    {
        validate_sample_labels(
            sample,
            &["result", "le"],
            &[("result", &["success", "rejected", "failed"])],
        )?;
    }
    Ok(())
}

/// Validate one parsed production sample against its exact fixed-cardinality contract.
fn validate_sample_labels(
    sample: &BifrostMetricSample,
    allowed: &[&str],
    categorical: &[(&str, &[&str])],
) -> Result<(), BifrostTelemetryReportError> {
    if sample
        .labels
        .keys()
        .any(|label| !allowed.contains(&label.as_str()))
    {
        return Err(BifrostTelemetryReportError::Parse {
            detail: format!("unexpected label on {}", sample.family),
        });
    }
    for (key, values) in categorical {
        if let Some(value) = sample.labels.get(*key)
            && !values.contains(&value.as_str())
        {
            return Err(BifrostTelemetryReportError::Parse {
                detail: format!("unexpected {key} value on {}", sample.family),
            });
        }
    }
    Ok(())
}

/// Parse every metric value from a production render.
fn rendered_values(rendered: &str) -> Result<BTreeMap<String, f64>, BifrostTelemetryReportError> {
    let mut values = BTreeMap::new();
    for line in rendered
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let (series, value) =
            line.rsplit_once(' ')
                .ok_or_else(|| BifrostTelemetryReportError::Parse {
                    detail: "metric line has no value".to_owned(),
                })?;
        let value = value
            .parse::<f64>()
            .map_err(|_| BifrostTelemetryReportError::Parse {
                detail: "metric value is not finite".to_owned(),
            })?;
        if !value.is_finite() {
            return Err(BifrostTelemetryReportError::Parse {
                detail: "metric value is not finite".to_owned(),
            });
        }
        values.insert(series.to_owned(), value);
    }
    Ok(values)
}

/// Parse exact Prometheus family TYPE declarations, rejecting conflicts.
fn rendered_types(
    rendered: &str,
) -> Result<BTreeMap<String, PrometheusFamilyType>, BifrostTelemetryReportError> {
    let mut types = BTreeMap::new();
    for line in rendered.lines().filter(|line| line.starts_with("# TYPE ")) {
        let mut fields = line.split_whitespace();
        let _hash = fields.next();
        let _type_keyword = fields.next();
        let family = fields
            .next()
            .ok_or_else(|| BifrostTelemetryReportError::Parse {
                detail: "TYPE declaration omits family".to_owned(),
            })?;
        let family_type = match fields.next() {
            Some("counter") => PrometheusFamilyType::Counter,
            Some("gauge") => PrometheusFamilyType::Gauge,
            Some("histogram") => PrometheusFamilyType::Histogram,
            Some("summary") => PrometheusFamilyType::Summary,
            _ => {
                return Err(BifrostTelemetryReportError::Parse {
                    detail: format!("unsupported TYPE for {family}"),
                });
            }
        };
        if types
            .insert(family.to_owned(), family_type)
            .is_some_and(|prior| prior != family_type)
        {
            return Err(BifrostTelemetryReportError::Parse {
                detail: format!("conflicting TYPE for {family}"),
            });
        }
    }
    Ok(types)
}

/// Return whether one rendered series belongs to a supported canonical kind.
fn supported_series(series: &str, types: &BTreeMap<String, PrometheusFamilyType>) -> bool {
    let name = series.split_once('{').map_or(series, |(name, _)| name);
    match sample_identity(name, types) {
        Ok(identity) => identity.is_some(),
        Err(_) => true,
    }
}

/// Resolve one rendered name through exact TYPE metadata and histogram bases.
fn sample_identity(
    name: &str,
    types: &BTreeMap<String, PrometheusFamilyType>,
) -> Result<Option<(String, BifrostMetricKind)>, BifrostTelemetryReportError> {
    let histogram = [
        ("_bucket", BifrostMetricKind::HistogramBucket),
        ("_count", BifrostMetricKind::HistogramCount),
        ("_sum", BifrostMetricKind::HistogramSum),
    ]
    .into_iter()
    .find_map(|(suffix, kind)| name.strip_suffix(suffix).map(|base| (base, kind)))
    .filter(|(base, _)| types.get(*base) == Some(&PrometheusFamilyType::Histogram));
    if let Some((base, kind)) = histogram {
        if types.contains_key(name) {
            return Err(BifrostTelemetryReportError::Parse {
                detail: format!("conflicting exact and histogram TYPE for {name}"),
            });
        }
        return Ok(Some((base.to_owned(), kind)));
    }
    Ok(match types.get(name) {
        Some(PrometheusFamilyType::Counter) => Some((name.to_owned(), BifrostMetricKind::Counter)),
        Some(PrometheusFamilyType::Gauge) => Some((name.to_owned(), BifrostMetricKind::Gauge)),
        Some(PrometheusFamilyType::Histogram | PrometheusFamilyType::Summary) | None => None,
    })
}

/// Parse one constrained Prometheus series into its family and labels.
fn parse_sample(
    series: &str,
    value: f64,
    types: &BTreeMap<String, PrometheusFamilyType>,
) -> Result<BifrostMetricSample, BifrostTelemetryReportError> {
    let (name, labels) =
        if let Some((name, labels)) = series.split_once('{') {
            (
                name,
                Some(labels.strip_suffix('}').ok_or_else(|| {
                    BifrostTelemetryReportError::Parse {
                        detail: "metric labels are not closed".to_owned(),
                    }
                })?),
            )
        } else {
            (series, None)
        };
    let (family, kind) =
        sample_identity(name, types)?.ok_or_else(|| BifrostTelemetryReportError::Parse {
            detail: format!("missing TYPE for {name}"),
        })?;
    let mut parsed = BTreeMap::new();
    if let Some(labels) = labels {
        for label in labels.split(',').filter(|label| !label.is_empty()) {
            let (key, quoted) =
                label
                    .split_once('=')
                    .ok_or_else(|| BifrostTelemetryReportError::Parse {
                        detail: "metric label is malformed".to_owned(),
                    })?;
            let value = quoted
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .or_else(|| (!quoted.contains('"')).then_some(quoted))
                .ok_or_else(|| BifrostTelemetryReportError::Parse {
                    detail: format!("metric label {key} is not quoted: {quoted}"),
                })?;
            if parsed.insert(key.to_owned(), value.to_owned()).is_some() {
                return Err(BifrostTelemetryReportError::Parse {
                    detail: "metric label is duplicated".to_owned(),
                });
            }
        }
    }
    Ok(BifrostMetricSample {
        family,
        labels: parsed,
        value,
        kind,
    })
}

/// Rebuild a normalized rendered-series identity from one parsed production sample.
fn rendered_series(sample: &BifrostMetricSample) -> String {
    let name = match sample.kind {
        BifrostMetricKind::HistogramBucket => format!("{}_bucket", sample.family),
        BifrostMetricKind::HistogramCount => format!("{}_count", sample.family),
        BifrostMetricKind::HistogramSum => format!("{}_sum", sample.family),
        BifrostMetricKind::Counter | BifrostMetricKind::Gauge => sample.family.clone(),
    };
    if sample.labels.is_empty() {
        return name;
    }
    let labels = sample
        .labels
        .iter()
        .map(|(key, value)| format!(r#"{key}="{value}""#))
        .collect::<Vec<_>>()
        .join(",");
    format!("{name}{{{labels}}}")
}

/// Sum one changed production family restricted by exact labels.
fn sum(
    delta: &BifrostTelemetryDelta,
    family: &str,
    labels: &[(&str, &str)],
) -> Result<f64, BifrostTelemetryReportError> {
    let samples = delta
        .metrics
        .iter()
        .filter(|sample| {
            sample.family == family
                && labels
                    .iter()
                    .all(|(key, value)| sample.labels.get(*key).map(String::as_str) == Some(*value))
        })
        .collect::<Vec<_>>();
    if samples.is_empty() {
        return Err(BifrostTelemetryReportError::MissingSeries {
            family: family.to_owned(),
        });
    }
    Ok(samples.into_iter().map(|sample| sample.value).sum())
}

/// Return an observed production gauge maximum restricted by exact labels.
fn gauge(
    delta: &BifrostTelemetryDelta,
    family: &str,
    labels: &[(&str, &str)],
) -> Result<f64, BifrostTelemetryReportError> {
    delta
        .gauge_maxima
        .iter()
        .filter(|sample| {
            sample.family == family
                && labels
                    .iter()
                    .all(|(key, value)| sample.labels.get(*key).map(String::as_str) == Some(*value))
        })
        .map(|sample| sample.value)
        .reduce(f64::max)
        .ok_or_else(|| BifrostTelemetryReportError::MissingSeries {
            family: family.to_owned(),
        })
}

/// Require that one activity counter advanced inside this telemetry window.
fn require_advanced(
    delta: &BifrostTelemetryDelta,
    family: &str,
) -> Result<(), BifrostTelemetryReportError> {
    let advanced = sum(delta, family, &[])?;
    if advanced > 0.0 {
        Ok(())
    } else {
        Err(BifrostTelemetryReportError::StaleSeries {
            family: family.to_owned(),
        })
    }
}

/// Estimate a histogram quantile from changed Prometheus cumulative buckets.
pub(crate) fn histogram_quantile(
    delta: &BifrostTelemetryDelta,
    family: &str,
    quantile: f64,
) -> Result<f64, BifrostTelemetryReportError> {
    histogram_quantile_for_label(delta, family, "", "", quantile)
}

/// Convert a finite nonnegative duration in seconds to rounded microseconds.
///
/// # Errors
/// Returns invalid binding when multiplication or rounding cannot fit `u64`.
pub(crate) fn seconds_to_micros(
    binding_id: &str,
    seconds: f64,
) -> Result<u64, BifrostTelemetryReportError> {
    let micros = seconds * 1_000_000.0;
    if !micros.is_finite() || micros < 0.0 || micros.round() > u64::MAX as f64 {
        return Err(BifrostTelemetryReportError::InvalidBinding {
            id: binding_id.to_owned(),
            detail: "duration cannot be represented in microseconds".to_owned(),
        });
    }
    Ok(micros.round() as u64)
}

/// Estimate a histogram quantile after selecting one exact closed label.
fn histogram_quantile_for_label(
    delta: &BifrostTelemetryDelta,
    family: &str,
    key: &str,
    value: &str,
    quantile: f64,
) -> Result<f64, BifrostTelemetryReportError> {
    histogram_quantile_state_for_label(delta, family, key, value, quantile)?.ok_or_else(|| {
        BifrostTelemetryReportError::EmptyHistogram {
            family: family.to_owned(),
        }
    })
}

/// Validate every categorical histogram tuple before deriving one family quantile.
///
/// A structurally complete all-zero family has no observations and returns
/// `None`. Complete idle tuples may coexist with active tuples and contribute
/// zero to the aggregate distribution.
///
/// # Errors
/// Returns a typed histogram error for missing or duplicate components,
/// mismatched bucket boundaries, invalid values, nonmonotonic buckets,
/// count mismatches, or an overflow-only positive distribution.
fn histogram_quantile_state_for_label(
    delta: &BifrostTelemetryDelta,
    family: &str,
    key: &str,
    value: &str,
    quantile: f64,
) -> Result<Option<f64>, BifrostTelemetryReportError> {
    type TupleKey = Vec<(String, String)>;
    type TupleParts = (Vec<f64>, Vec<f64>, BTreeMap<String, Vec<f64>>);

    let invalid = |detail: &str| BifrostTelemetryReportError::InvalidBinding {
        id: family.to_owned(),
        detail: detail.to_owned(),
    };
    let mut tuples = BTreeMap::<TupleKey, TupleParts>::new();
    for sample in delta.metrics.iter().filter(|sample| {
        sample.family == family
            && (key.is_empty() || sample.labels.get(key).map(String::as_str) == Some(value))
    }) {
        let tuple = sample
            .labels
            .iter()
            .filter(|(label, _)| label.as_str() != "le")
            .map(|(label, value)| (label.clone(), value.clone()))
            .collect::<Vec<_>>();
        let parts = tuples.entry(tuple).or_default();
        match sample.kind {
            BifrostMetricKind::HistogramBucket => {
                let upper = sample
                    .labels
                    .get("le")
                    .ok_or_else(|| invalid("histogram bucket omits le"))?;
                parts.2.entry(upper.clone()).or_default().push(sample.value);
            }
            BifrostMetricKind::HistogramCount => parts.0.push(sample.value),
            BifrostMetricKind::HistogramSum => parts.1.push(sample.value),
            _ => return Err(invalid("histogram family contains a non-histogram sample")),
        }
    }
    if tuples.is_empty() {
        return Err(BifrostTelemetryReportError::MissingSeries {
            family: family.to_owned(),
        });
    }

    let mut expected_boundaries: Option<Vec<String>> = None;
    let mut aggregate = BTreeMap::<String, f64>::new();
    let mut aggregate_count = 0.0;
    let mut any_active = false;
    for (counts, sums, buckets) in tuples.values() {
        if counts.len() != 1 || sums.len() != 1 || buckets.is_empty() {
            return Err(invalid(
                "each histogram tuple requires exactly one count, one sum, and buckets",
            ));
        }
        if buckets.values().any(|values| values.len() != 1) {
            return Err(invalid(
                "histogram tuple contains a duplicate bucket boundary",
            ));
        }
        let boundaries = buckets.keys().cloned().collect::<Vec<_>>();
        if expected_boundaries
            .as_ref()
            .is_some_and(|expected| expected != &boundaries)
        {
            return Err(invalid(
                "histogram tuples have mismatched bucket boundaries",
            ));
        }
        expected_boundaries.get_or_insert(boundaries);
        let count = counts[0];
        let sum = sums[0];
        if !count.is_finite() || !sum.is_finite() || count < 0.0 || sum < 0.0 {
            return Err(invalid("histogram count or sum is invalid"));
        }
        let idle = count == 0.0 && sum == 0.0 && buckets.values().all(|values| values[0] == 0.0);
        if idle {
            continue;
        }
        any_active = true;
        if count <= 0.0 {
            return Err(invalid("active histogram tuple has no positive count"));
        }
        let mut ordered = buckets
            .iter()
            .map(|(upper, values)| {
                let parsed = match upper.as_str() {
                    "+Inf" | "Inf" => Ok(f64::INFINITY),
                    _ => upper
                        .parse::<f64>()
                        .map_err(|_| invalid("histogram bucket boundary is invalid")),
                }?;
                Ok((parsed, upper, values[0]))
            })
            .collect::<Result<Vec<_>, BifrostTelemetryReportError>>()?;
        ordered.sort_by(|left, right| left.0.total_cmp(&right.0));
        let mut previous = 0.0;
        for (_, upper, cumulative) in &ordered {
            if !cumulative.is_finite() || *cumulative < previous || *cumulative < 0.0 {
                return Err(invalid("histogram buckets are nonmonotonic"));
            }
            previous = *cumulative;
            *aggregate.entry((*upper).clone()).or_default() += *cumulative;
        }
        if ordered.last().map(|(_, _, value)| *value) != Some(count) {
            return Err(invalid("histogram +Inf bucket does not equal count"));
        }
        let target = (count * quantile).ceil();
        if !ordered
            .iter()
            .any(|(upper, _, cumulative)| upper.is_finite() && *cumulative >= target)
        {
            return Err(invalid("histogram positive observations are overflow-only"));
        }
        aggregate_count += count;
    }
    if !any_active {
        return Ok(None);
    }
    let target = (aggregate_count * quantile).ceil();
    let mut ordered = aggregate
        .into_iter()
        .map(|(upper, cumulative)| {
            let parsed = match upper.as_str() {
                "+Inf" | "Inf" => Ok(f64::INFINITY),
                _ => upper
                    .parse::<f64>()
                    .map_err(|_| invalid("histogram bucket boundary is invalid")),
            }?;
            Ok((parsed, cumulative))
        })
        .collect::<Result<Vec<_>, BifrostTelemetryReportError>>()?;
    ordered.sort_by(|left, right| left.0.total_cmp(&right.0));
    ordered
        .into_iter()
        .find(|(upper, cumulative)| upper.is_finite() && *cumulative >= target)
        .map(|(upper, _)| Some(upper))
        .ok_or_else(|| invalid("histogram aggregate is overflow-only"))
}

/// Validate active-role gauges and restart activity against launched topology.
fn validate_role_topology(
    delta: &BifrostTelemetryDelta,
    expected: &BTreeMap<String, u64>,
) -> Result<BTreeMap<String, ForgeRoleTopologyReport>, BifrostTelemetryReportError> {
    let active_roles = delta
        .gauge_maxima
        .iter()
        .filter(|sample| sample.family == "bifrost_forge_role_processes" && sample.value > 0.0)
        .filter_map(|sample| sample.labels.get("role").cloned())
        .collect::<BTreeSet<_>>();
    let final_roles = delta
        .gauge_final
        .iter()
        .filter(|sample| sample.family == "bifrost_forge_role_processes" && sample.value > 0.0)
        .filter_map(|sample| sample.labels.get("role").cloned())
        .collect::<BTreeSet<_>>();
    let expected_roles = expected.keys().cloned().collect::<BTreeSet<_>>();
    if active_roles != expected_roles || final_roles != expected_roles {
        return Err(BifrostTelemetryReportError::TopologyMismatch);
    }
    let mut topology = BTreeMap::new();
    for (role, count) in expected {
        let active = gauge(delta, "bifrost_forge_role_processes", &[("role", role)])?;
        let final_active = delta
            .gauge_final
            .iter()
            .find(|sample| {
                sample.family == "bifrost_forge_role_processes"
                    && sample.labels.get("role").map(String::as_str) == Some(role)
            })
            .map(|sample| sample.value)
            .ok_or(BifrostTelemetryReportError::TopologyMismatch)?;
        let starts = sum(
            delta,
            "bifrost_forge_role_process_started_total",
            &[("role", role)],
        )?;
        if active != *count as f64 || final_active != *count as f64 || starts < *count as f64 {
            return Err(BifrostTelemetryReportError::TopologyMismatch);
        }
        let max_active = active as u64;
        let final_active = final_active as u64;
        let starts = starts as u64;
        topology.insert(
            role.clone(),
            ForgeRoleTopologyReport {
                max_active,
                final_active,
                starts,
            },
        );
    }
    Ok(topology)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Construct one normalized production sample for mapper contract tests.
    fn sample(family: &str, labels: &[(&str, &str)], value: f64) -> BifrostMetricSample {
        BifrostMetricSample {
            family: family.to_owned(),
            labels: labels
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
            value,
            kind: if labels.iter().any(|(key, _)| *key == "le") {
                BifrostMetricKind::HistogramBucket
            } else if family.ends_with("_total") && family != "oracle_queries_active" {
                BifrostMetricKind::Counter
            } else {
                BifrostMetricKind::Gauge
            },
        }
    }

    /// Construct one normalized histogram count sample for mapper tests.
    fn histogram_count(family: &str, labels: &[(&str, &str)], value: f64) -> BifrostMetricSample {
        let mut sample = sample(family, labels, value);
        sample.kind = BifrostMetricKind::HistogramCount;
        sample
    }

    /// Construct one normalized histogram sum sample for mapper tests.
    fn histogram_sum(family: &str, labels: &[(&str, &str)], value: f64) -> BifrostMetricSample {
        let mut sample = sample(family, labels, value);
        sample.kind = BifrostMetricKind::HistogramSum;
        sample
    }

    /// Construct one normalized captured production span for mapper tests.
    fn captured_span(name: &str, attributes: &[(&str, &str)]) -> CapturedSpan {
        CapturedSpan {
            trace_id: "00000000000000000000000000000001".to_owned(),
            name: name.to_owned(),
            attributes: attributes
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
            duration_nanos: 1,
            status: wyrd_telemetry::CapturedSpanStatus::Unset,
        }
    }

    /// Return the sampled window owned by one binding aggregation in test fixtures.
    fn binding_samples_mut<'a>(
        delta: &'a mut BifrostTelemetryDelta,
        binding: &TelemetryBinding,
    ) -> &'a mut Vec<BifrostMetricSample> {
        match binding.aggregation {
            TelemetryAggregation::Delta | TelemetryAggregation::P99 => &mut delta.metrics,
            TelemetryAggregation::Peak => &mut delta.gauge_maxima,
            TelemetryAggregation::Final => &mut delta.gauge_final,
        }
    }

    /// Active Oracle queries are an up/down gauge despite their bounded labels.
    #[test]
    fn oracle_active_queries_are_not_monotonic() {
        let types = BTreeMap::from([(
            "oracle_queries_active".to_owned(),
            PrometheusFamilyType::Gauge,
        )]);
        assert_eq!(
            parse_sample("oracle_queries_active{class=\"analytical\"}", 1.0, &types,)
                .unwrap()
                .kind,
            BifrostMetricKind::Gauge
        );
    }

    /// Proves histogram suffixes retain distinct kinds before family normalization.
    #[test]
    fn parser_preserves_exact_metric_kind() {
        let types = BTreeMap::from([
            (
                "bifrost_gate_query_stream_duration_seconds".to_owned(),
                PrometheusFamilyType::Histogram,
            ),
            (
                "bifrost_gate_requests_total".to_owned(),
                PrometheusFamilyType::Counter,
            ),
        ]);
        let bucket = parse_sample(
            "bifrost_gate_query_stream_duration_seconds_bucket{le=\"1\",outcome=\"success\"}",
            1.0,
            &types,
        )
        .expect("exact production bucket parses");
        assert_eq!(bucket.family, "bifrost_gate_query_stream_duration_seconds");
        assert_eq!(bucket.kind, BifrostMetricKind::HistogramBucket);
        let counter = parse_sample(
            "bifrost_gate_requests_total{operation=\"query\",outcome=\"success\"}",
            1.0,
            &types,
        )
        .expect("exact production counter parses");
        assert_eq!(counter.kind, BifrostMetricKind::Counter);
    }

    /// Histogram suffixes inherit only an explicit base histogram TYPE.
    #[test]
    fn parser_normalizes_histogram_type_without_guessing_suffixes() {
        let base = "bifrost_gate_resolution_seconds";
        let types = BTreeMap::from([(base.to_owned(), PrometheusFamilyType::Histogram)]);
        for (suffix, expected) in [
            ("_bucket{le=\"0.1\"}", BifrostMetricKind::HistogramBucket),
            ("_count", BifrostMetricKind::HistogramCount),
            ("_sum", BifrostMetricKind::HistogramSum),
        ] {
            let parsed = parse_sample(&format!("{base}{suffix}"), 1.0, &types)
                .expect("base histogram TYPE classifies rendered suffix");
            assert_eq!(parsed.family, base);
            assert_eq!(parsed.kind, expected);
        }

        let exact = BTreeMap::from([
            ("jobs_count".to_owned(), PrometheusFamilyType::Counter),
            ("queue_total".to_owned(), PrometheusFamilyType::Gauge),
        ]);
        assert_eq!(
            parse_sample("jobs_count", 1.0, &exact)
                .expect("exact count-named counter")
                .kind,
            BifrostMetricKind::Counter
        );
        assert_eq!(
            parse_sample("queue_total", 1.0, &exact)
                .expect("exact total-named gauge")
                .kind,
            BifrostMetricKind::Gauge
        );

        let missing = parse_sample("required_seconds_count", 1.0, &BTreeMap::new())
            .expect_err("required histogram suffix needs a base TYPE");
        assert!(
            missing
                .to_string()
                .contains("missing TYPE for required_seconds_count")
        );
        assert!(!supported_series(
            "unrelated_seconds_count",
            &BTreeMap::new()
        ));

        let conflicting = BTreeMap::from([
            (base.to_owned(), PrometheusFamilyType::Histogram),
            (format!("{base}_count"), PrometheusFamilyType::Counter),
        ]);
        let conflict = parse_sample(&format!("{base}_count"), 1.0, &conflicting)
            .expect_err("exact suffix TYPE conflicts with histogram base");
        assert!(
            conflict
                .to_string()
                .contains("conflicting exact and histogram TYPE")
        );
    }

    /// Proves names never override declared Prometheus family types.
    #[test]
    fn parser_uses_type_for_nonstandard_counter_and_gauge_names() {
        let rendered =
            "# TYPE queue_total gauge\nqueue_total 2\n# TYPE requests counter\nrequests 3\n";
        let types = rendered_types(rendered).expect("TYPE declarations parse");
        assert_eq!(
            parse_sample("queue_total", 2.0, &types).unwrap().kind,
            BifrostMetricKind::Gauge
        );
        assert_eq!(
            parse_sample("requests", 3.0, &types).unwrap().kind,
            BifrostMetricKind::Counter
        );
        assert!(parse_sample("missing", 1.0, &types).is_err());
        assert!(rendered_types("# TYPE requests counter\n# TYPE requests gauge\n").is_err());
    }

    /// Proves checked window buckets, rather than sum, define p99 microseconds.
    #[test]
    fn histogram_window_p99_uses_bucket_delta_and_checked_microseconds() {
        let family = "vala_postgres_pool_acquire_seconds";
        let mut delta = canonical_binding_delta();
        delta.metrics.retain(|sample| sample.family != family);
        for (le, value) in [("0.001", 98.0), ("0.025", 99.0), ("+Inf", 100.0)] {
            delta.metrics.push(BifrostMetricSample {
                family: family.to_owned(),
                labels: BTreeMap::from([
                    ("le".to_owned(), le.to_owned()),
                    ("outcome".to_owned(), "success".to_owned()),
                    ("pool".to_owned(), "runtime".to_owned()),
                ]),
                value,
                kind: BifrostMetricKind::HistogramBucket,
            });
        }
        delta.metrics.push(BifrostMetricSample {
            family: family.to_owned(),
            labels: BTreeMap::from([
                ("outcome".to_owned(), "success".to_owned()),
                ("pool".to_owned(), "runtime".to_owned()),
            ]),
            value: 100.0,
            kind: BifrostMetricKind::HistogramCount,
        });
        delta.metrics.push(BifrostMetricSample {
            family: family.to_owned(),
            labels: BTreeMap::from([
                ("outcome".to_owned(), "success".to_owned()),
                ("pool".to_owned(), "runtime".to_owned()),
            ]),
            value: 0.5,
            kind: BifrostMetricKind::HistogramSum,
        });
        let p99 = histogram_quantile(&delta, family, 0.99).unwrap();
        assert_eq!(seconds_to_micros("postgres.acquire", p99).unwrap(), 25_000);
    }

    /// Proves independent accepted label series aggregate by bucket boundary.
    #[test]
    fn histogram_quantile_aggregates_label_series_by_bucket() {
        let family = "wyrd_storage_operation_duration_seconds";
        let mut metrics = Vec::new();
        for (operation, buckets, count) in [
            (
                "get",
                [("0.001", 49.0), ("0.025", 50.0), ("+Inf", 50.0)],
                50.0,
            ),
            (
                "put",
                [("0.001", 49.0), ("0.025", 50.0), ("+Inf", 50.0)],
                50.0,
            ),
        ] {
            for (le, value) in buckets {
                metrics.push(BifrostMetricSample {
                    family: family.to_owned(),
                    labels: BTreeMap::from([
                        ("operation".to_owned(), operation.to_owned()),
                        ("le".to_owned(), le.to_owned()),
                    ]),
                    value,
                    kind: BifrostMetricKind::HistogramBucket,
                });
            }
            metrics.push(BifrostMetricSample {
                family: family.to_owned(),
                labels: BTreeMap::from([("operation".to_owned(), operation.to_owned())]),
                value: count,
                kind: BifrostMetricKind::HistogramCount,
            });
            metrics.push(BifrostMetricSample {
                family: family.to_owned(),
                labels: BTreeMap::from([("operation".to_owned(), operation.to_owned())]),
                value: 0.5,
                kind: BifrostMetricKind::HistogramSum,
            });
        }
        let delta = BifrostTelemetryDelta {
            families: BTreeSet::new(),
            metrics,
            gauge_maxima: Vec::new(),
            gauge_final: Vec::new(),
            spans: Vec::new(),
            interval_seconds: 1.0,
            process: test_process_window(),
        };
        assert_eq!(
            seconds_to_micros(
                "storage.duration",
                histogram_quantile(&delta, family, 0.99).unwrap()
            )
            .unwrap(),
            25_000
        );
    }

    /// Proves empty, nonmonotonic, reset, and infinity-only windows are rejected.
    #[test]
    fn histogram_window_rejects_invalid_bucket_shapes() {
        let family = "test_duration_seconds";
        let make = |buckets: &[(&str, f64)], count: f64| BifrostTelemetryDelta {
            families: BTreeSet::new(),
            metrics: buckets
                .iter()
                .map(|(le, value)| BifrostMetricSample {
                    family: family.to_owned(),
                    labels: BTreeMap::from([("le".to_owned(), (*le).to_owned())]),
                    value: *value,
                    kind: BifrostMetricKind::HistogramBucket,
                })
                .chain(std::iter::once(BifrostMetricSample {
                    family: family.to_owned(),
                    labels: BTreeMap::new(),
                    value: count,
                    kind: BifrostMetricKind::HistogramCount,
                }))
                .collect(),
            gauge_maxima: Vec::new(),
            gauge_final: Vec::new(),
            spans: Vec::new(),
            interval_seconds: 1.0,
            process: test_process_window(),
        };
        assert!(histogram_quantile(&make(&[], 0.0), family, 0.99).is_err());
        assert!(
            histogram_quantile(&make(&[("1", 2.0), ("+Inf", 1.0)], 1.0), family, 0.99).is_err()
        );
        assert!(
            histogram_quantile(&make(&[("1", -1.0), ("+Inf", 1.0)], 1.0), family, 0.99).is_err()
        );
        assert!(histogram_quantile(&make(&[("+Inf", 1.0)], 1.0), family, 0.99).is_err());
    }

    /// Proves stable epochs calculate deltas, replacement starts a new epoch, and reset fails.
    #[test]
    fn process_window_handles_peaks_replacement_and_reset() {
        let start = test_process_sample();
        let stable = ProcessSample {
            cpu_total: 3.0,
            tokio_busy_total: 2.5,
            rss_bytes: 2,
            queue_depth: 1,
            ..start.clone()
        };
        let window = process_window(&start, stable, 9, 7).unwrap();
        assert_eq!(
            (
                window.cpu_seconds,
                window.tokio_busy_seconds,
                window.peak_rss_bytes,
                window.queue_peak
            ),
            (2.0, 1.5, 9, 7)
        );
        let replacement = ProcessSample {
            identity: "pid-next".to_owned(),
            epoch: 1,
            cpu_total: 0.5,
            tokio_busy_total: 0.25,
            ..start.clone()
        };
        assert_eq!(process_window(&start, replacement, 4, 3).unwrap().epoch, 1);
        let reset = ProcessSample {
            cpu_total: 0.5,
            ..start.clone()
        };
        assert!(process_window(&start, reset, 1, 1).is_err());
    }

    /// Proves every required closed binding fails independently when removed.
    #[test]
    fn canonical_projection_rejects_each_missing_binding() {
        let complete = canonical_binding_delta();
        validate_cluster_bindings(&complete).expect("closed binding fixture is complete");
        for binding in CLUSTER_BINDINGS {
            let first_family_id = CLUSTER_BINDINGS
                .iter()
                .find(|candidate| candidate.family == binding.family)
                .expect("binding family has an owner")
                .id
                .0;
            let mut removed = complete.clone();
            removed
                .metrics
                .retain(|sample| sample.family != binding.family);
            removed
                .gauge_maxima
                .retain(|sample| sample.family != binding.family);
            removed
                .gauge_final
                .retain(|sample| sample.family != binding.family);
            assert!(matches!(
                validate_cluster_bindings(&removed),
                Err(BifrostTelemetryReportError::InvalidBinding { ref id, .. })
                    if id == first_family_id
            ));
        }
    }

    /// Table-drives kind, unit, label, value, and aggregation failures for every binding.
    #[test]
    fn canonical_projection_rejects_each_invalid_binding_dimension() {
        for binding in CLUSTER_BINDINGS {
            let first_family_id = CLUSTER_BINDINGS
                .iter()
                .find(|candidate| {
                    candidate.family == binding.family
                        && candidate.aggregation == binding.aggregation
                })
                .expect("binding family has an owner")
                .id
                .0;
            let assert_id = |result: Result<(), BifrostTelemetryReportError>| {
                assert!(
                    matches!(result, Err(BifrostTelemetryReportError::InvalidBinding { ref id, .. }) if id == first_family_id),
                    "binding {} expected invalid ID {first_family_id}, got {result:?}",
                    binding.id.0,
                );
            };

            let mut wrong_kind = canonical_binding_delta();
            let source = match binding.aggregation {
                TelemetryAggregation::Delta | TelemetryAggregation::P99 => &mut wrong_kind.metrics,
                TelemetryAggregation::Peak => &mut wrong_kind.gauge_maxima,
                TelemetryAggregation::Final => &mut wrong_kind.gauge_final,
            };
            let sample = source
                .iter_mut()
                .find(|sample| sample.family == binding.family && sample.kind == binding.kind);
            sample.expect("binding fixture has expected sample").kind =
                if binding.kind == BifrostMetricKind::Gauge {
                    BifrostMetricKind::Counter
                } else {
                    BifrostMetricKind::Gauge
                };
            assert_id(validate_cluster_bindings(&wrong_kind));

            let mut unknown_key = canonical_binding_delta();
            binding_samples_mut(&mut unknown_key, binding)
                .iter_mut()
                .find(|sample| sample.family == binding.family && sample.kind == binding.kind)
                .expect("binding fixture has expected sample")
                .labels
                .insert("tenant".to_owned(), "forbidden".to_owned());
            assert_id(validate_cluster_bindings(&unknown_key));

            let mut nonfinite = canonical_binding_delta();
            binding_samples_mut(&mut nonfinite, binding)
                .iter_mut()
                .find(|sample| sample.family == binding.family && sample.kind == binding.kind)
                .expect("binding fixture has expected sample")
                .value = f64::NAN;
            assert_id(validate_cluster_bindings(&nonfinite));

            let mut wrong_aggregation = canonical_binding_delta();
            let source = match binding.aggregation {
                TelemetryAggregation::Delta | TelemetryAggregation::P99 => {
                    &mut wrong_aggregation.metrics
                }
                TelemetryAggregation::Peak => &mut wrong_aggregation.gauge_maxima,
                TelemetryAggregation::Final => &mut wrong_aggregation.gauge_final,
            };
            source.retain(|sample| sample.family != binding.family);
            assert_id(validate_cluster_bindings(&wrong_aggregation));

            let mut wrong_unit = *binding;
            wrong_unit.unit = match binding.unit {
                TelemetryUnit::Count => TelemetryUnit::Seconds,
                TelemetryUnit::Bytes | TelemetryUnit::Ratio => TelemetryUnit::Seconds,
                TelemetryUnit::Seconds => TelemetryUnit::Count,
            };
            assert!(matches!(
                validate_binding_unit(&wrong_unit),
                Err(BifrostTelemetryReportError::InvalidBinding { ref id, .. })
                    if id == binding.id.0
            ));

            if let Some(domain) = binding.allowed_label_values.first() {
                let mut wrong_value = canonical_binding_delta();
                binding_samples_mut(&mut wrong_value, binding)
                    .iter_mut()
                    .find(|sample| sample.family == binding.family && sample.kind == binding.kind)
                    .expect("binding fixture has expected sample")
                    .labels
                    .insert(domain.key.to_owned(), "forbidden".to_owned());
                assert_id(validate_cluster_bindings(&wrong_value));
            }

            for domain in binding.allowed_label_values {
                let mut missing_key = canonical_binding_delta();
                for sample in binding_samples_mut(&mut missing_key, binding)
                    .iter_mut()
                    .filter(|sample| sample.family == binding.family)
                {
                    sample.labels.remove(domain.key);
                }
                assert_id(validate_cluster_bindings(&missing_key));
            }

            if binding.kind == BifrostMetricKind::HistogramBucket {
                let mut missing_le = canonical_binding_delta();
                missing_le
                    .metrics
                    .iter_mut()
                    .filter(|sample| {
                        sample.family == binding.family
                            && sample.kind == BifrostMetricKind::HistogramBucket
                    })
                    .for_each(|sample| {
                        sample.labels.remove("le");
                    });
                assert_id(validate_cluster_bindings(&missing_le));

                for kind in [
                    BifrostMetricKind::HistogramCount,
                    BifrostMetricKind::HistogramSum,
                ] {
                    let mut illegal_le = canonical_binding_delta();
                    illegal_le
                        .metrics
                        .iter_mut()
                        .filter(|sample| sample.family == binding.family && sample.kind == kind)
                        .for_each(|sample| {
                            sample.labels.insert("le".to_owned(), "1".to_owned());
                        });
                    assert_id(validate_cluster_bindings(&illegal_le));
                }
            }
        }
    }

    /// Proves a rejected Gate sample cannot omit its operation identity.
    #[test]
    fn gate_rejected_sample_without_operation_fails_closed() {
        let mut delta = canonical_binding_delta();
        let rejected = delta
            .metrics
            .iter_mut()
            .find(|sample| {
                sample.family == "bifrost_gate_requests_total"
                    && sample
                        .labels
                        .get("outcome")
                        .is_some_and(|value| value == "rejected")
            })
            .expect("fixture contains one rejected Gate request");
        rejected.labels.remove("operation");
        assert!(matches!(
            validate_cluster_bindings(&delta),
            Err(BifrostTelemetryReportError::InvalidBinding { ref id, .. })
                if id == "gate.requests.success"
        ));
    }

    /// Proves wrong kinds, open labels, and error spans invalidate clean evidence.
    #[test]
    fn canonical_projection_rejects_malformed_and_error_evidence() {
        let mut wrong_kind = canonical_binding_delta();
        wrong_kind.metrics[0].kind = BifrostMetricKind::Gauge;
        assert!(validate_cluster_bindings(&wrong_kind).is_err());

        let mut open_label = canonical_binding_delta();
        open_label.metrics[0]
            .labels
            .insert("tenant".to_owned(), "forbidden".to_owned());
        assert!(validate_cluster_bindings(&open_label).is_err());

        let mut error_span = canonical_binding_delta();
        let mut span = captured_span("bifrost.oracle.query", &[]);
        span.status = wyrd_telemetry::CapturedSpanStatus::Error("failed".to_owned());
        error_span.spans.push(span);
        assert!(
            ClusterTelemetryProjection::from_delta(
                &error_span,
                ClusterTelemetryExpectation {
                    topology: BifrostTopology::OnePod,
                    required_binding_ids: &[],
                    required_dependency_ids: &[],
                    required_trace_operations: &[ClusterTraceOperation::QueryTotal],
                    required_clean_binding_ids: &[],
                },
            )
            .is_err()
        );
    }

    /// Query-total evidence recognizes only the terminal public stream owner.
    #[test]
    fn query_total_trace_uses_stream_lifecycle_not_oracle_setup() {
        assert_eq!(
            cluster_trace_operation("bifrost.gate.query.stream"),
            Some(ClusterTraceOperation::QueryTotal)
        );
        assert_eq!(cluster_trace_operation("bifrost.oracle.query"), None);
        assert_eq!(cluster_trace_operation("bifrost.gate.query"), None);
    }

    /// Optional dependency absence stays absent while present malformed evidence fails closed.
    #[test]
    fn canonical_projection_distinguishes_absent_idle_and_malformed_storage_latency() {
        let family = "wyrd_storage_operation_duration_seconds";
        let mut absent = canonical_binding_delta();
        absent.metrics.retain(|sample| sample.family != family);
        let expectation = ClusterTelemetryExpectation {
            topology: BifrostTopology::OnePod,
            required_binding_ids: &[],
            required_dependency_ids: &[],
            required_trace_operations: &[],
            required_clean_binding_ids: &[],
        };
        let evidence = ClusterTelemetryProjection::from_delta(&absent, expectation)
            .expect("optional storage latency may be wholly absent");
        assert_eq!(evidence.dependencies.storage_p99_us, None);

        let mut idle = canonical_binding_delta();
        idle.metrics
            .iter_mut()
            .filter(|sample| sample.family == family)
            .for_each(|sample| sample.value = 0.0);
        let idle_evidence = ClusterTelemetryProjection::from_delta(&idle, expectation)
            .expect("structurally complete idle storage latency is optional");
        assert_eq!(idle_evidence.dependencies.storage_p99_us, None);

        let mut malformed = idle;
        malformed
            .metrics
            .iter_mut()
            .find(|sample| {
                sample.family == family && sample.kind == BifrostMetricKind::HistogramCount
            })
            .expect("fixture contains storage histogram count")
            .value = 1.0;
        let malformed_error = ClusterTelemetryProjection::from_delta(&malformed, expectation)
            .expect_err("positive count with zero buckets must fail closed");
        assert!(
            matches!(
                malformed_error,
                BifrostTelemetryReportError::InvalidBinding { ref id, .. }
                    if id == "storage.duration"
            ),
            "unexpected malformed storage error: {malformed_error:?}"
        );
    }

    /// Complete benchmark evidence cannot omit storage latency or synthesize a p99.
    #[cfg(feature = "bench")]
    #[test]
    fn complete_projection_requires_storage_latency() {
        let family = "wyrd_storage_operation_duration_seconds";
        let mut absent = canonical_binding_delta();
        absent.metrics.retain(|sample| sample.family != family);
        assert!(matches!(
            ClusterTelemetryProjection::from_delta(
                &absent,
                ClusterTelemetryExpectation::complete(BifrostTopology::OnePod),
            ),
            Err(BifrostTelemetryReportError::InvalidBinding { ref id, .. })
                if id == "storage.duration"
        ));

        let mut idle = canonical_binding_delta();
        idle.metrics
            .iter_mut()
            .filter(|sample| sample.family == family)
            .for_each(|sample| sample.value = 0.0);
        assert!(matches!(
            ClusterTelemetryProjection::from_delta(
                &idle,
                ClusterTelemetryExpectation::complete(BifrostTopology::OnePod),
            ),
            Err(BifrostTelemetryReportError::InvalidBinding { ref id, .. })
                if id == "storage.duration"
        ));
    }

    /// Proves the bounded sampler cancellation signal terminates and joins its owner task.
    #[tokio::test]
    async fn gauge_sampler_cancellation_cleans_up_owner_task() {
        let stop = tokio_util::sync::CancellationToken::new();
        let task_stop = stop.clone();
        let task = tokio::spawn(async move {
            task_stop.cancelled().await;
            Ok(SamplerSnapshot {
                maxima: BTreeMap::new(),
                process: test_process_sample(),
                peak_rss_bytes: 1,
                queue_peak: 1,
            })
        });
        let sampler = BifrostTelemetrySampler { stop, task };
        sampler.stop.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(1), sampler.task)
            .await
            .expect("bounded sampler joins before its cleanup deadline")
            .expect("bounded sampler task exits without panic")
            .expect("bounded sampler reports clean cancellation");
    }

    /// Proves sampler task panic and polling failures remain distinct typed errors.
    #[tokio::test]
    async fn sampler_join_propagates_panic_and_polling_failure() {
        let panic: tokio::task::JoinHandle<Result<SamplerSnapshot, BifrostTelemetryReportError>> =
            tokio::spawn(async move { panic!("injected sampler panic") });
        assert!(matches!(
            join_sampler(panic).await,
            Err(BifrostTelemetryReportError::SamplerJoin { .. })
        ));

        let polling = tokio::spawn(async move {
            Err(BifrostTelemetryReportError::Parse {
                detail: "injected malformed scrape".to_owned(),
            })
        });
        assert!(matches!(
            join_sampler(polling).await,
            Err(BifrostTelemetryReportError::Parse { .. })
        ));
    }

    /// Independent accepted-T16 emitter contract used to audit one binding.
    struct EmitterContractFixture {
        /// Stable binding identity expected to consume the emitter.
        id: &'static str,
        /// Exact destination selectors independently derived from report semantics.
        selectors: &'static [(&'static str, &'static str)],
        /// Exact family emitted by production source.
        family: &'static str,
        /// Prometheus kind declared by the emitter.
        kind: BifrostMetricKind,
        /// Exact emitted label keys.
        keys: &'static [&'static str],
        /// Closed categorical domains declared by production code.
        domains: &'static [(&'static str, &'static [&'static str])],
        /// Normative emitted unit.
        unit: TelemetryUnit,
        /// Destination aggregation required by the report.
        aggregation: TelemetryAggregation,
        /// Workload role that owns the emitter.
        requirement: TelemetryRequirement,
        /// Exact report destination fed by this binding.
        destination: &'static str,
    }

    /// Enumerate accepted-T16 emitter contracts without reading the binding ledger.
    fn emitter_contracts() -> Vec<EmitterContractFixture> {
        use BifrostMetricKind::{Counter, Gauge, HistogramBucket};
        use TelemetryAggregation::{Delta, Final, P99, Peak};
        use TelemetryRequirement::{Always, Role};
        use TelemetryUnit::{Bytes, Count, Ratio, Seconds};
        vec![
            EmitterContractFixture {
                id: "gate.requests.success",
                selectors: &[("outcome", "success")],
                family: "bifrost_gate_requests_total",
                kind: Counter,
                keys: &["operation", "outcome"],
                domains: &[
                    ("operation", &["query", "write"]),
                    ("outcome", &["success", "rejected", "failed", "cancelled"]),
                ],
                unit: Count,
                aggregation: Delta,
                requirement: Always,
                destination: "pillars.gate_accepted",
            },
            EmitterContractFixture {
                id: "gate.requests.query_success",
                selectors: &[("operation", "query"), ("outcome", "success")],
                family: "bifrost_gate_requests_total",
                kind: Counter,
                keys: &["operation", "outcome"],
                domains: &[
                    ("operation", &["query", "write"]),
                    ("outcome", &["success", "rejected", "failed", "cancelled"]),
                ],
                unit: Count,
                aggregation: Delta,
                requirement: Always,
                destination: "reconciliation.successful_queries",
            },
            EmitterContractFixture {
                id: "gate.requests.rejected",
                selectors: &[("outcome", "rejected")],
                family: "bifrost_gate_requests_total",
                kind: Counter,
                keys: &["operation", "outcome"],
                domains: &[
                    ("operation", &["query", "write"]),
                    ("outcome", &["success", "rejected", "failed", "cancelled"]),
                ],
                unit: Count,
                aggregation: Delta,
                requirement: Always,
                destination: "phase.gate_rejected",
            },
            EmitterContractFixture {
                id: "gate.requests.failed",
                selectors: &[("outcome", "failed")],
                family: "bifrost_gate_requests_total",
                kind: Counter,
                keys: &["operation", "outcome"],
                domains: &[
                    ("operation", &["query", "write"]),
                    ("outcome", &["success", "rejected", "failed", "cancelled"]),
                ],
                unit: Count,
                aggregation: Delta,
                requirement: Always,
                destination: "phase.gate_failed",
            },
            EmitterContractFixture {
                id: "gate.requests.cancelled",
                selectors: &[("outcome", "cancelled")],
                family: "bifrost_gate_requests_total",
                kind: Counter,
                keys: &["operation", "outcome"],
                domains: &[
                    ("operation", &["query", "write"]),
                    ("outcome", &["success", "rejected", "failed", "cancelled"]),
                ],
                unit: Count,
                aggregation: Delta,
                requirement: Always,
                destination: "phase.gate_cancelled",
            },
            EmitterContractFixture {
                id: "gate.requests.write_cancelled",
                selectors: &[("operation", "write"), ("outcome", "cancelled")],
                family: "bifrost_gate_requests_total",
                kind: Counter,
                keys: &["operation", "outcome"],
                domains: &[
                    ("operation", &["query", "write"]),
                    ("outcome", &["success", "rejected", "failed", "cancelled"]),
                ],
                unit: Count,
                aggregation: Delta,
                requirement: Always,
                destination: "phase.gate_write_request_cancelled",
            },
            EmitterContractFixture {
                id: "gate.bytes",
                selectors: &[],
                family: "bifrost_gate_frame_bytes_total",
                kind: Counter,
                keys: &[],
                domains: &[],
                unit: Bytes,
                aggregation: Delta,
                requirement: Always,
                destination: "phase.gate_bytes",
            },
            EmitterContractFixture {
                id: "gate.rows",
                selectors: &[("status", "accepted")],
                family: "bifrost_gate_rows_total",
                kind: Counter,
                keys: &["status"],
                domains: &[("status", &["accepted", "rejected"])],
                unit: Count,
                aggregation: Delta,
                requirement: Always,
                destination: "reconciliation.gate_rows(status=accepted)",
            },
            EmitterContractFixture {
                id: "gate.active",
                selectors: &[],
                family: "bifrost_gate_active_streams",
                kind: Gauge,
                keys: &["operation"],
                domains: &[("operation", &["query"])],
                unit: Count,
                aggregation: Final,
                requirement: Always,
                destination: "cleanup.gate_active",
            },
            EmitterContractFixture {
                id: "cleanup.scribe_ingress",
                selectors: &[],
                family: "bifrost_scribe_ingress_active",
                kind: Gauge,
                keys: &[],
                domains: &[],
                unit: Count,
                aggregation: Final,
                requirement: Role("scribe"),
                destination: "cleanup.scribe_ingress",
            },
            EmitterContractFixture {
                id: "cleanup.scribe_lane_active",
                selectors: &[],
                family: "bifrost_scribe_lane_active",
                kind: Gauge,
                keys: &["lane"],
                domains: &[("lane", &["ingress", "persistence", "wal_io"])],
                unit: Count,
                aggregation: Final,
                requirement: Role("scribe"),
                destination: "cleanup.scribe_lane_active",
            },
            EmitterContractFixture {
                id: "cleanup.scribe_lane_queued",
                selectors: &[],
                family: "bifrost_scribe_lane_queued",
                kind: Gauge,
                keys: &["lane"],
                domains: &[("lane", &["ingress", "persistence", "wal_io"])],
                unit: Count,
                aggregation: Final,
                requirement: Role("scribe"),
                destination: "cleanup.scribe_lane_queued",
            },
            EmitterContractFixture {
                id: "cleanup.scribe_persistence_queue",
                selectors: &[],
                family: "bifrost_scribe_persistence_queue_depth",
                kind: Gauge,
                keys: &[],
                domains: &[],
                unit: Count,
                aggregation: Final,
                requirement: Role("scribe"),
                destination: "cleanup.scribe_persistence_queue",
            },
            EmitterContractFixture {
                id: "cleanup.oracle_active",
                selectors: &[],
                family: "oracle_queries_active",
                kind: Gauge,
                keys: &["class"],
                domains: &[("class", &["interactive", "analytical"])],
                unit: Count,
                aggregation: Final,
                requirement: Role("oracle"),
                destination: "cleanup.oracle_active",
            },
            EmitterContractFixture {
                id: "cleanup.storage_active",
                selectors: &[],
                family: "wyrd_storage_operations_active",
                kind: Gauge,
                keys: &["backend", "operation"],
                domains: &[
                    ("backend", &["local", "s3", "gcs", "azure"]),
                    ("operation", &["get", "put", "list", "delete", "head"]),
                ],
                unit: Count,
                aggregation: Final,
                requirement: Always,
                destination: "cleanup.storage_active",
            },
            EmitterContractFixture {
                id: "gate.query_streams",
                selectors: &[],
                family: "bifrost_gate_query_streams_total",
                kind: Counter,
                keys: &["outcome"],
                domains: &[("outcome", &["success", "rejected", "failed", "cancelled"])],
                unit: Count,
                aggregation: Delta,
                requirement: Always,
                destination: "binding validation",
            },
            EmitterContractFixture {
                id: "gate.query_streams.cancelled",
                selectors: &[("outcome", "cancelled")],
                family: "bifrost_gate_query_streams_total",
                kind: Counter,
                keys: &["outcome"],
                domains: &[("outcome", &["success", "rejected", "failed", "cancelled"])],
                unit: Count,
                aggregation: Delta,
                requirement: Always,
                destination: "phase.gate_query_stream_cancelled",
            },
            EmitterContractFixture {
                id: "gate.request_duration",
                selectors: &[],
                family: "bifrost_gate_request_duration_seconds",
                kind: HistogramBucket,
                keys: &["le", "operation", "outcome"],
                domains: &[
                    ("operation", &["query", "write"]),
                    ("outcome", &["success", "rejected", "failed", "cancelled"]),
                ],
                unit: Seconds,
                aggregation: P99,
                requirement: Always,
                destination: "binding validation",
            },
            EmitterContractFixture {
                id: "gate.query_stream_duration",
                selectors: &[],
                family: "bifrost_gate_query_stream_duration_seconds",
                kind: HistogramBucket,
                keys: &["le", "outcome"],
                domains: &[("outcome", &["success", "rejected", "failed", "cancelled"])],
                unit: Seconds,
                aggregation: P99,
                requirement: Always,
                destination: "binding validation",
            },
            EmitterContractFixture {
                id: "scribe.rows",
                selectors: &[],
                family: "bifrost_scribe_rows_total",
                kind: Counter,
                keys: &["status"],
                domains: &[("status", &["accepted"])],
                unit: Count,
                aggregation: Delta,
                requirement: Role("scribe"),
                destination: "reconciliation.scribe_rows",
            },
            EmitterContractFixture {
                id: "forge.backlog_peak",
                selectors: &[],
                family: "bifrost_forge_oldest_backlog_seconds",
                kind: Gauge,
                keys: &[],
                domains: &[],
                unit: Seconds,
                aggregation: Peak,
                requirement: Role("forge"),
                destination: "binding validation",
            },
            EmitterContractFixture {
                id: "oracle.admission",
                selectors: &[],
                family: "oracle_admission_total",
                kind: Counter,
                keys: &["class", "outcome", "reason"],
                domains: &[
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
                            "shutdown",
                        ],
                    ),
                ],
                unit: Count,
                aggregation: Delta,
                requirement: Role("oracle"),
                destination: "binding validation",
            },
            EmitterContractFixture {
                id: "oracle.admission_queue",
                selectors: &[],
                family: "oracle_admission_queue_duration_seconds",
                kind: HistogramBucket,
                keys: &["class", "le"],
                domains: &[("class", &["interactive", "analytical"])],
                unit: Seconds,
                aggregation: P99,
                requirement: Role("oracle"),
                destination: "oracle admission queue",
            },
            EmitterContractFixture {
                id: "oracle.tenant_pressure_peak",
                selectors: &[],
                family: "oracle_tenant_budget_pressure",
                kind: Gauge,
                keys: &["class"],
                domains: &[("class", &["interactive", "analytical"])],
                unit: Ratio,
                aggregation: Peak,
                requirement: Role("oracle"),
                destination: "oracle tenant pressure",
            },
            EmitterContractFixture {
                id: "scribe.wal_bytes",
                selectors: &[],
                family: "bifrost_scribe_wal_append_bytes_total",
                kind: Counter,
                keys: &[],
                domains: &[],
                unit: Bytes,
                aggregation: Delta,
                requirement: Role("scribe"),
                destination: "pillars.scribe_wal_bytes",
            },
            EmitterContractFixture {
                id: "scribe.seal_rows",
                selectors: &[("stage", "file_list_transaction")],
                family: "bifrost_scribe_seal_rows_total",
                kind: Counter,
                keys: &["stage"],
                domains: &[(
                    "stage",
                    &[
                        "freeze",
                        "parquet_encode",
                        "object_store_put",
                        "file_list_transaction",
                    ],
                )],
                unit: Count,
                aggregation: Delta,
                requirement: Role("scribe"),
                destination: "reconciliation.sealed_rows(stage=file_list_transaction)",
            },
            EmitterContractFixture {
                id: "forge.publications",
                selectors: &[],
                family: "bifrost_forge_complete_gauge_publications_total",
                kind: Counter,
                keys: &[],
                domains: &[],
                unit: Count,
                aggregation: Delta,
                requirement: Role("forge"),
                destination: "pillars.forge_publications",
            },
            EmitterContractFixture {
                id: "forge.rewrite_input_files",
                selectors: &[],
                family: "bifrost_forge_rewrite_input_files_total",
                kind: Counter,
                keys: &["source"],
                domains: &[("source", &["staging", "iceberg"])],
                unit: Count,
                aggregation: Delta,
                requirement: Role("forge"),
                destination: "phase.forge_input_files",
            },
            EmitterContractFixture {
                id: "forge.rewrite_input_bytes",
                selectors: &[],
                family: "bifrost_forge_rewrite_input_bytes_total",
                kind: Counter,
                keys: &["source"],
                domains: &[("source", &["staging", "iceberg"])],
                unit: Bytes,
                aggregation: Delta,
                requirement: Role("forge"),
                destination: "phase.forge_input_bytes",
            },
            EmitterContractFixture {
                id: "forge.rewrite_output_files",
                selectors: &[],
                family: "bifrost_forge_rewrite_output_files_total",
                kind: Counter,
                keys: &["source"],
                domains: &[("source", &["staging", "iceberg"])],
                unit: Count,
                aggregation: Delta,
                requirement: Role("forge"),
                destination: "phase.forge_output_files",
            },
            EmitterContractFixture {
                id: "forge.rewrite_output_bytes",
                selectors: &[],
                family: "bifrost_forge_rewrite_output_bytes_total",
                kind: Counter,
                keys: &["source"],
                domains: &[("source", &["staging", "iceberg"])],
                unit: Bytes,
                aggregation: Delta,
                requirement: Role("forge"),
                destination: "phase.forge_output_bytes",
            },
            EmitterContractFixture {
                id: "oracle.rows",
                selectors: &[],
                family: "oracle_query_rows_total",
                kind: Counter,
                keys: &["class"],
                domains: &[("class", &["interactive", "analytical"])],
                unit: Count,
                aggregation: Delta,
                requirement: Role("oracle"),
                destination: "pillars.oracle_decoded_rows/reconciliation.oracle_rows",
            },
            EmitterContractFixture {
                id: "oracle.stream_bytes",
                selectors: &[],
                family: "oracle_query_bytes_returned_total",
                kind: Counter,
                keys: &["class"],
                domains: &[("class", &["interactive", "analytical"])],
                unit: Bytes,
                aggregation: Delta,
                requirement: Role("oracle"),
                destination: "phase.oracle_stream_bytes",
            },
            EmitterContractFixture {
                id: "oracle.logical_bytes",
                selectors: &[],
                family: "oracle_query_logical_bytes_selected_total",
                kind: Counter,
                keys: &["class"],
                domains: &[("class", &["interactive", "analytical"])],
                unit: Bytes,
                aggregation: Delta,
                requirement: Role("oracle"),
                destination: "oracle logical bytes",
            },
            EmitterContractFixture {
                id: "oracle.physical_bytes",
                selectors: &[],
                family: "oracle_query_bytes_scanned_total",
                kind: Counter,
                keys: &["class"],
                domains: &[("class", &["interactive", "analytical"])],
                unit: Bytes,
                aggregation: Delta,
                requirement: Role("oracle"),
                destination: "oracle physical bytes",
            },
            EmitterContractFixture {
                id: "oracle.files",
                selectors: &[],
                family: "oracle_query_files_scanned_total",
                kind: Counter,
                keys: &["class"],
                domains: &[("class", &["interactive", "analytical"])],
                unit: Count,
                aggregation: Delta,
                requirement: Role("oracle"),
                destination: "oracle files",
            },
            EmitterContractFixture {
                id: "oracle.partitions",
                selectors: &[],
                family: "oracle_query_partitions_scanned_total",
                kind: Counter,
                keys: &["class"],
                domains: &[("class", &["interactive", "analytical"])],
                unit: Count,
                aggregation: Delta,
                requirement: Role("oracle"),
                destination: "oracle partitions",
            },
            EmitterContractFixture {
                id: "oracle.queued_peak",
                selectors: &[],
                family: "oracle_queries_queued",
                kind: Gauge,
                keys: &["class"],
                domains: &[("class", &["interactive", "analytical"])],
                unit: Count,
                aggregation: Peak,
                requirement: Role("oracle"),
                destination: "oracle queued peak",
            },
            EmitterContractFixture {
                id: "oracle.spill_bytes",
                selectors: &[],
                family: "oracle_query_spill_bytes_total",
                kind: Counter,
                keys: &["class"],
                domains: &[("class", &["interactive", "analytical"])],
                unit: Bytes,
                aggregation: Delta,
                requirement: Role("oracle"),
                destination: "oracle spill bytes",
            },
            EmitterContractFixture {
                id: "postgres.acquire",
                selectors: &[],
                family: "vala_postgres_pool_acquire_seconds",
                kind: HistogramBucket,
                keys: &["le", "outcome", "pool"],
                domains: &[
                    ("outcome", &["success", "failed", "cancelled"]),
                    ("pool", &["runtime"]),
                ],
                unit: Seconds,
                aggregation: P99,
                requirement: Always,
                destination: "dependencies.postgres_pool_wait_us",
            },
            EmitterContractFixture {
                id: "postgres.transactions",
                selectors: &[],
                family: "vala_postgres_pool_acquire_total",
                kind: Counter,
                keys: &["pool"],
                domains: &[("pool", &["runtime"])],
                unit: Count,
                aggregation: Delta,
                requirement: Always,
                destination: "dependencies.postgres_transactions",
            },
            EmitterContractFixture {
                id: "storage.duration",
                selectors: &[],
                family: "wyrd_storage_operation_duration_seconds",
                kind: HistogramBucket,
                keys: &["backend", "le", "operation", "outcome"],
                domains: &[
                    ("backend", &["local", "s3", "gcs", "azure"]),
                    ("operation", &["get", "put", "list", "delete", "head"]),
                    ("outcome", &["success", "failed", "cancelled"]),
                ],
                unit: Seconds,
                aggregation: P99,
                requirement: Always,
                destination: "dependencies.storage_p99_us",
            },
            EmitterContractFixture {
                id: "storage.bytes",
                selectors: &[],
                family: "wyrd_storage_bytes_total",
                kind: Counter,
                keys: &["backend", "direction", "operation"],
                domains: &[
                    ("backend", &["local", "s3", "gcs", "azure"]),
                    ("direction", &["read", "write"]),
                    ("operation", &["get", "put"]),
                ],
                unit: Bytes,
                aggregation: Delta,
                requirement: Always,
                destination: "dependencies.storage_bytes",
            },
            EmitterContractFixture {
                id: "wal.fsync",
                selectors: &[],
                family: "bifrost_scribe_wal_fsync_seconds",
                kind: HistogramBucket,
                keys: &["le", "outcome"],
                domains: &[("outcome", &["success", "failed", "cancelled"])],
                unit: Seconds,
                aggregation: P99,
                requirement: Role("scribe"),
                destination: "dependencies.wal_fsync_p99_us",
            },
        ]
    }

    /// Prove every binding exactly matches an independently enumerated emitter contract.
    #[test]
    fn bindings_match_accepted_t16_emitter_contracts() {
        let contracts = emitter_contracts();
        assert_eq!(contracts.len(), CLUSTER_BINDINGS.len());
        for contract in contracts {
            assert!(!contract.destination.is_empty());
            let binding = CLUSTER_BINDINGS
                .iter()
                .find(|binding| binding.id.0 == contract.id)
                .expect("emitter contract has one binding");
            assert_eq!(
                (binding.family, binding.kind),
                (contract.family, contract.kind)
            );
            let mut keys = binding
                .allowed_label_values
                .iter()
                .map(|domain| domain.key)
                .collect::<Vec<_>>();
            if binding.kind == BifrostMetricKind::HistogramBucket {
                keys.push("le");
            }
            keys.sort_unstable();
            let mut expected_keys = contract.keys.to_vec();
            expected_keys.sort_unstable();
            assert_eq!(keys, expected_keys);
            assert_eq!(
                (binding.unit, binding.aggregation, binding.requirement),
                (contract.unit, contract.aggregation, contract.requirement)
            );
            let actual = binding
                .allowed_label_values
                .iter()
                .map(|domain| (domain.key, domain.values))
                .collect::<Vec<_>>();
            assert_eq!(actual, contract.domains);
            let selectors = binding
                .selected_label_values
                .iter()
                .map(|selected| (selected.key, selected.value))
                .collect::<Vec<_>>();
            assert_eq!(selectors, contract.selectors);
            assert!(selectors_match_allowed_domains(binding));
        }
    }

    /// Prove the projection's Oracle admission domains are sourced from production enums.
    #[test]
    fn oracle_admission_domains_match_production_closed_labels() {
        let production = vala_bifrost_redux::bench_support::oracle_telemetry_label_domains();
        let classes = CLUSTER_BINDINGS
            .iter()
            .find(|binding| binding.id.0 == "oracle.rows")
            .expect("Oracle rows binding exists")
            .allowed_label_values[0]
            .values;
        assert_eq!(classes, production.classes);
        let admission = CLUSTER_BINDINGS
            .iter()
            .find(|binding| binding.id.0 == "oracle.admission")
            .expect("Oracle admission binding exists")
            .allowed_label_values;
        assert_eq!(admission[0].values, production.classes);
        assert_eq!(admission[1].values, production.outcomes);
        assert_eq!(admission[2].values, production.reasons);

        assert_eq!(
            ORACLE_TERMINAL_LABELS[0].values,
            production.terminal_outcomes
        );
        assert_eq!(
            ORACLE_CANCELLATION_LABELS[0].values,
            production.cancellation_reasons
        );
        assert_eq!(
            ORACLE_FRAGMENT_LABELS[0].values,
            production.fragment_outcomes
        );

        let audit = wyrd_server::oracle::audit_telemetry_label_domains();
        assert_eq!(ORACLE_AUDIT_RELAY_LABELS[0].values, audit.outcomes);
        assert_eq!(ORACLE_AUDIT_RELAY_LABELS[1].values, audit.failure_reasons);

        let redux_keys = [
            ORACLE_ADMISSION_LABELS
                .iter()
                .map(|domain| domain.key)
                .collect::<Vec<_>>(),
            ORACLE_TERMINAL_LABELS
                .iter()
                .map(|domain| domain.key)
                .collect::<Vec<_>>(),
            ORACLE_CANCELLATION_LABELS
                .iter()
                .map(|domain| domain.key)
                .collect::<Vec<_>>(),
            ORACLE_FRAGMENT_LABELS
                .iter()
                .map(|domain| domain.key)
                .collect::<Vec<_>>(),
        ];
        assert_eq!(redux_keys[0], vec!["class", "outcome", "reason"]);
        assert_eq!(redux_keys[1], vec!["outcome"]);
        assert_eq!(redux_keys[2], vec!["reason"]);
        assert_eq!(redux_keys[3], vec!["outcome"]);
        let audit_keys = ORACLE_AUDIT_RELAY_LABELS
            .iter()
            .map(|domain| domain.key)
            .collect::<Vec<_>>();
        assert_eq!(audit_keys, vec!["outcome", "reason"]);
    }

    /// Return whether every selector pair belongs to its allowed emitter domain.
    fn selectors_match_allowed_domains(binding: &TelemetryBinding) -> bool {
        binding.selected_label_values.iter().all(|selected| {
            binding
                .allowed_label_values
                .iter()
                .any(|domain| domain.key == selected.key && domain.values.contains(&selected.value))
        })
    }

    /// Prove selector filtering distinguishes accepted operations from query successes.
    #[test]
    fn gate_request_destinations_use_distinct_declarative_selectors() {
        let mut delta = canonical_binding_delta();
        delta
            .metrics
            .retain(|sample| sample.family != "bifrost_gate_requests_total");
        for (operation, outcome) in [
            ("write", "success"),
            ("write", "cancelled"),
            ("query", "success"),
            ("query", "rejected"),
            ("query", "failed"),
            ("query", "cancelled"),
        ] {
            delta.metrics.push(BifrostMetricSample {
                family: "bifrost_gate_requests_total".to_owned(),
                labels: BTreeMap::from([
                    ("operation".to_owned(), operation.to_owned()),
                    ("outcome".to_owned(), outcome.to_owned()),
                ]),
                value: 1.0,
                kind: BifrostMetricKind::Counter,
            });
        }
        let evaluated = evaluate_cluster_bindings(&delta).expect("valid Gate domains evaluate");
        assert_eq!(
            evaluated.get("gate.requests.success"),
            Some(&EvaluatedBindingValue::Counter(2))
        );
        assert_eq!(
            evaluated.get("gate.requests.query_success"),
            Some(&EvaluatedBindingValue::Counter(1))
        );

        let mut missing_query_success = delta.clone();
        missing_query_success.metrics.retain(|sample| {
            sample.family != "bifrost_gate_requests_total"
                || sample.labels.get("operation").map(String::as_str) != Some("query")
                || sample.labels.get("outcome").map(String::as_str) != Some("success")
        });
        assert!(matches!(
            evaluate_cluster_bindings(&missing_query_success),
            Err(BifrostTelemetryReportError::InvalidBinding { id, .. })
                if id == "gate.requests.query_success"
        ));

        delta.metrics.push(BifrostMetricSample {
            family: "bifrost_gate_requests_total".to_owned(),
            labels: BTreeMap::from([
                ("operation".to_owned(), "write".to_owned()),
                ("outcome".to_owned(), "rejected".to_owned()),
                ("unexpected".to_owned(), "value".to_owned()),
            ]),
            value: 1.0,
            kind: BifrostMetricKind::Counter,
        });
        assert!(matches!(
            evaluate_cluster_bindings(&delta),
            Err(BifrostTelemetryReportError::InvalidBinding { id, .. })
                if id == "gate.requests.success"
        ));

        let mut inconsistent = *CLUSTER_BINDINGS
            .iter()
            .find(|binding| binding.id.0 == "gate.requests.query_success")
            .expect("query-success binding exists");
        inconsistent.selected_label_values = &[TelemetrySelectedLabel {
            key: "outcome",
            value: "unknown",
        }];
        assert!(!selectors_match_allowed_domains(&inconsistent));
    }

    /// Prove rejected Gate rows never inflate the accepted-row destination.
    #[test]
    fn gate_rows_projection_selects_only_accepted_status() {
        let mut delta = canonical_binding_delta();
        delta.metrics.push(BifrostMetricSample {
            family: "bifrost_gate_rows_total".to_owned(),
            labels: BTreeMap::from([("status".to_owned(), "rejected".to_owned())]),
            value: 99.0,
            kind: BifrostMetricKind::Counter,
        });
        assert_eq!(
            evaluate_cluster_bindings(&delta)
                .expect("emitter-aligned fixture evaluates")
                .get("gate.rows"),
            Some(&EvaluatedBindingValue::Counter(1))
        );
    }

    /// Proves every matrix phase field is read from its exact evaluated binding.
    #[test]
    fn canonical_phase_projection_retains_exact_matrix_destinations() {
        let delta = canonical_binding_delta();
        let bindings = evaluate_cluster_bindings(&delta).expect("complete bindings evaluate");
        let phase = project_cluster_phase_evidence(&bindings);
        assert_eq!(phase.gate_rows, 1);
        assert_eq!(phase.gate_bytes, 1);
        assert_eq!(phase.gate_query_request_success, 1);
        assert_eq!(phase.gate_write_request_cancelled, 1);
        assert_eq!(phase.gate_query_stream_cancelled, 1);
        assert_eq!(phase.gate_query_stream_terminals, 2);
        assert_eq!(phase.gate_active_streams, 1);
        assert_eq!(phase.scribe_rows, 1);
        assert_eq!(phase.oracle_stream_rows, 1);
        assert_eq!(phase.oracle_stream_bytes, 1);
        assert_eq!(phase.forge_input_files, 1);
        assert_eq!(phase.forge_input_bytes, 1);
        assert_eq!(phase.forge_output_files, 1);
        assert_eq!(phase.forge_output_bytes, 1);
    }

    /// Proves a newly exposed phase destination cannot disappear silently.
    #[test]
    fn canonical_phase_projection_rejects_missing_oracle_stream_bytes() {
        let mut delta = canonical_binding_delta();
        delta
            .metrics
            .retain(|sample| sample.family != "oracle_query_bytes_returned_total");
        assert!(matches!(
            evaluate_cluster_bindings(&delta),
            Err(BifrostTelemetryReportError::InvalidBinding { id, .. })
                if id == "oracle.stream_bytes"
        ));
    }

    /// Build one complete exact binding fixture from independent emitter contracts.
    fn canonical_binding_delta() -> BifrostTelemetryDelta {
        let mut delta = BifrostTelemetryDelta {
            families: BTreeSet::new(),
            metrics: Vec::new(),
            gauge_maxima: Vec::new(),
            gauge_final: Vec::new(),
            spans: Vec::new(),
            interval_seconds: 1.0,
            process: test_process_window(),
        };
        for contract in emitter_contracts() {
            let binding = CLUSTER_BINDINGS
                .iter()
                .find(|binding| binding.id.0 == contract.id)
                .expect("contract has binding");
            let mut labels = contract
                .domains
                .iter()
                .map(|(key, values)| ((*key).to_owned(), values[0].to_owned()))
                .collect::<BTreeMap<_, _>>();
            for (key, value) in contract.selectors {
                labels.insert((*key).to_owned(), (*value).to_owned());
            }
            let mut sample = BifrostMetricSample {
                family: binding.family.to_owned(),
                labels: labels.clone(),
                value: 1.0,
                kind: binding.kind,
            };
            match binding.aggregation {
                TelemetryAggregation::Delta => {
                    if !delta.metrics.iter().any(|existing| {
                        existing.family == sample.family
                            && existing.kind == sample.kind
                            && existing.labels == sample.labels
                    }) {
                        delta.metrics.push(sample);
                    }
                }
                TelemetryAggregation::P99 => {
                    sample.labels.insert("le".to_owned(), "0.001".to_owned());
                    delta.metrics.push(sample);
                    let mut infinity = labels.clone();
                    infinity.insert("le".to_owned(), "+Inf".to_owned());
                    delta.metrics.push(BifrostMetricSample {
                        family: binding.family.to_owned(),
                        labels: infinity,
                        value: 1.0,
                        kind: BifrostMetricKind::HistogramBucket,
                    });
                    delta.metrics.push(BifrostMetricSample {
                        family: binding.family.to_owned(),
                        labels: labels.clone(),
                        value: 1.0,
                        kind: BifrostMetricKind::HistogramCount,
                    });
                    delta.metrics.push(BifrostMetricSample {
                        family: binding.family.to_owned(),
                        labels,
                        value: 0.001,
                        kind: BifrostMetricKind::HistogramSum,
                    });
                }
                TelemetryAggregation::Peak => delta.gauge_maxima.push(sample),
                TelemetryAggregation::Final => delta.gauge_final.push(sample),
            }
        }
        delta
    }

    /// Construct one complete three-worker production delta without a fixture value path.
    fn complete_delta() -> BifrostTelemetryDelta {
        BifrostTelemetryDelta {
            families: BTreeSet::new(),
            metrics: vec![
                sample(
                    "bifrost_forge_rewrite_output_bytes_total",
                    &[("source", "staging")],
                    1_048_576.0,
                ),
                sample("bifrost_forge_complete_gauge_publications_total", &[], 1.0),
                sample(
                    "bifrost_memory_reservations_total",
                    &[("consumer", "forge"), ("outcome", "accepted")],
                    1.0,
                ),
                sample(
                    "bifrost_forge_task_duration_seconds",
                    &[
                        ("strategy", "staging_fold"),
                        ("result", "succeeded"),
                        ("le", "0.1"),
                    ],
                    1.0,
                ),
                histogram_count(
                    "bifrost_forge_task_duration_seconds",
                    &[("strategy", "staging_fold"), ("result", "succeeded")],
                    1.0,
                ),
                sample(
                    "bifrost_forge_task_duration_seconds",
                    &[
                        ("strategy", "staging_fold"),
                        ("result", "succeeded"),
                        ("le", "+Inf"),
                    ],
                    1.0,
                ),
                sample(
                    "bifrost_forge_task_spill_bytes",
                    &[("strategy", "staging_fold"), ("le", "65536")],
                    1.0,
                ),
                histogram_count(
                    "bifrost_forge_task_spill_bytes",
                    &[("strategy", "staging_fold")],
                    1.0,
                ),
                sample(
                    "bifrost_forge_task_spill_bytes",
                    &[("strategy", "staging_fold"), ("le", "+Inf")],
                    1.0,
                ),
                sample(
                    "bifrost_forge_cleanup_duration_seconds",
                    &[("kind", "expired"), ("le", "0.025")],
                    1.0,
                ),
                histogram_count(
                    "bifrost_forge_cleanup_duration_seconds",
                    &[("kind", "expired")],
                    1.0,
                ),
                sample(
                    "bifrost_forge_cleanup_duration_seconds",
                    &[("kind", "expired"), ("le", "+Inf")],
                    1.0,
                ),
                sample(
                    "bifrost_forge_conflicts_total",
                    &[("kind", "lease_contention")],
                    0.0,
                ),
                sample(
                    "bifrost_forge_conflicts_total",
                    &[("kind", "fence_lost")],
                    0.0,
                ),
                sample(
                    "bifrost_forge_conflicts_total",
                    &[("kind", "snapshot_changed")],
                    0.0,
                ),
                sample(
                    "bifrost_forge_role_process_started_total",
                    &[("role", "server")],
                    1.0,
                ),
                sample(
                    "bifrost_forge_role_process_started_total",
                    &[("role", "forge_worker")],
                    4.0,
                ),
                histogram_sum(
                    "bifrost_forge_task_duration_seconds",
                    &[("strategy", "staging_fold"), ("result", "succeeded")],
                    0.1,
                ),
                histogram_sum(
                    "bifrost_forge_task_spill_bytes",
                    &[("strategy", "staging_fold")],
                    65_536.0,
                ),
                histogram_sum(
                    "bifrost_forge_cleanup_duration_seconds",
                    &[("kind", "expired")],
                    0.025,
                ),
            ],
            gauge_maxima: vec![
                sample("bifrost_forge_oldest_backlog_seconds", &[], 0.01),
                sample(
                    "bifrost_memory_reserved_bytes",
                    &[("consumer", "forge")],
                    1024.0,
                ),
                sample("bifrost_forge_fairness_lag_tasks", &[], 1.0),
                sample("bifrost_forge_role_processes", &[("role", "server")], 1.0),
                sample(
                    "bifrost_forge_role_processes",
                    &[("role", "forge_worker")],
                    3.0,
                ),
            ],
            gauge_final: vec![
                sample("bifrost_forge_oldest_backlog_seconds", &[], 0.0),
                sample(
                    "bifrost_memory_reserved_bytes",
                    &[("consumer", "forge")],
                    0.0,
                ),
                sample("bifrost_forge_fairness_lag_tasks", &[], 0.0),
                sample("bifrost_forge_role_processes", &[("role", "server")], 1.0),
                sample(
                    "bifrost_forge_role_processes",
                    &[("role", "forge_worker")],
                    3.0,
                ),
            ],
            spans: vec![
                captured_span(
                    "bifrost.forge.scheduler.pass",
                    &[("result", "succeeded"), ("role", "server")],
                ),
                captured_span(
                    "bifrost.forge.task.execute",
                    &[
                        ("attempt_id", "01890f28-7c4a-7000-98e7-4f4a3c2d1b02"),
                        ("result", "succeeded"),
                        ("role", "forge_worker"),
                        ("strategy", "staging_fold"),
                        ("task_id", "01890f28-7c4a-7000-98e7-4f4a3c2d1b01"),
                    ],
                ),
                captured_span(
                    "bifrost.forge.catalog.commit",
                    &[
                        ("attempt_id", "01890f28-7c4a-7000-98e7-4f4a3c2d1b04"),
                        ("result", "succeeded"),
                        ("role", "forge_worker"),
                        ("strategy", "small_files"),
                        ("task_id", "01890f28-7c4a-7000-98e7-4f4a3c2d1b03"),
                    ],
                ),
                captured_span(
                    "bifrost.forge.cleanup",
                    &[
                        ("attempt_id", "01890f28-7c4a-7000-98e7-4f4a3c2d1b06"),
                        ("kind", "expired"),
                        ("result", "succeeded"),
                        ("role", "forge_worker"),
                        ("strategy", "snapshot_expiry"),
                        ("task_id", "01890f28-7c4a-7000-98e7-4f4a3c2d1b05"),
                    ],
                ),
            ],
            interval_seconds: 2.0,
            process: test_process_window(),
        }
    }

    /// Build deterministic process evidence for sampler-only unit fixtures.
    fn test_process_sample() -> ProcessSample {
        ProcessSample {
            identity: "pid-test".to_owned(),
            epoch: 0,
            cpu_total: 1.0,
            rss_bytes: 1,
            tokio_busy_total: 1.0,
            queue_depth: 1,
        }
    }

    /// Build deterministic process-window evidence for projection unit fixtures.
    fn test_process_window() -> ProcessWindow {
        ProcessWindow {
            identity: "pid-test".to_owned(),
            epoch: 0,
            cpu_seconds: 1.0,
            current_rss_bytes: 1,
            peak_rss_bytes: 1,
            tokio_busy_seconds: 1.0,
            queue_peak: 1,
        }
    }

    /// Retains an in-window production gauge peak in one entry per series.
    #[test]
    fn gauge_maximum_accumulator_retains_transient_peak_without_tick_history() {
        let series = "bifrost_memory_reserved_bytes{consumer=\"forge\"}";
        let mut maxima = BTreeMap::new();
        let types = BTreeMap::from([
            (
                "bifrost_memory_reserved_bytes".to_owned(),
                PrometheusFamilyType::Gauge,
            ),
            (
                "bifrost_forge_operations_total".to_owned(),
                PrometheusFamilyType::Counter,
            ),
        ]);
        merge_gauge_maxima(
            &mut maxima,
            &BTreeMap::from([(series.to_owned(), 4.0)]),
            &types,
        )
        .unwrap();
        merge_gauge_maxima(
            &mut maxima,
            &BTreeMap::from([(series.to_owned(), 32.0)]),
            &types,
        )
        .unwrap();
        merge_gauge_maxima(
            &mut maxima,
            &BTreeMap::from([(series.to_owned(), 1.0)]),
            &types,
        )
        .unwrap();
        merge_gauge_maxima(
            &mut maxima,
            &BTreeMap::from([("bifrost_forge_operations_total".to_owned(), 99.0)]),
            &types,
        )
        .unwrap();

        assert_eq!(maxima, BTreeMap::from([(series.to_owned(), 32.0)]));
    }

    /// Prove every report projection responds only to its production delta field.
    #[test]
    fn forge_telemetry_report_uses_production_delta() {
        let expected = BTreeMap::from([("server".to_owned(), 1), ("forge_worker".to_owned(), 3)]);
        let baseline =
            ForgeMaintenanceTelemetryReport::from_production_delta(&complete_delta(), &expected)
                .expect("complete production delta maps");
        macro_rules! assert_projection {
            ($delta:expr, $field:ident) => {{
                let changed =
                    ForgeMaintenanceTelemetryReport::from_production_delta(&$delta, &expected)
                        .expect("changed production delta maps");
                assert_ne!(changed.$field, baseline.$field);
                let mut normalized = changed;
                normalized.$field = baseline.$field.clone();
                assert_eq!(normalized, baseline);
            }};
        }

        let mut delta = complete_delta();
        delta.metrics[0].value *= 2.0;
        assert_projection!(delta, throughput_mib_per_sec);

        let mut delta = complete_delta();
        delta.metrics[3]
            .labels
            .insert("le".to_owned(), "0.2".to_owned());
        assert_projection!(delta, task_latency_p99_us);

        let mut delta = complete_delta();
        delta.gauge_maxima[0].value = 0.02;
        assert_projection!(delta, backlog_age_us);

        let mut delta = complete_delta();
        delta.gauge_maxima[1].value = 2048.0;
        assert_projection!(delta, peak_parent_memory);

        let mut delta = complete_delta();
        delta.metrics[6]
            .labels
            .insert("le".to_owned(), "131072".to_owned());
        assert_projection!(delta, spill_bytes);

        for (index, field) in [
            (12, "lease_contention"),
            (13, "fence_lost"),
            (14, "snapshot_changed"),
        ] {
            let mut delta = complete_delta();
            delta.metrics[index].value = 1.0;
            let changed = ForgeMaintenanceTelemetryReport::from_production_delta(&delta, &expected)
                .expect("changed production conflict maps");
            let mut normalized = changed.clone();
            match field {
                "lease_contention" => normalized.lease_contention = baseline.lease_contention,
                "fence_lost" => normalized.fence_lost = baseline.fence_lost,
                "snapshot_changed" => normalized.snapshot_changed = baseline.snapshot_changed,
                _ => unreachable!("closed conflict projection"),
            }
            assert_ne!(changed, baseline);
            assert_eq!(normalized, baseline);
        }

        let mut delta = complete_delta();
        delta.gauge_maxima[2].value = 2.0;
        assert_projection!(delta, fairness_lag_tasks);

        let mut delta = complete_delta();
        delta.metrics[9]
            .labels
            .insert("le".to_owned(), "0.05".to_owned());
        assert_projection!(delta, cleanup_delay_us);

        let mut delta = complete_delta();
        delta.metrics[16].value = 5.0;
        assert_projection!(delta, role_topology);
    }

    /// Reject benchmark-local derivation paths outside the production capture mapper.
    #[test]
    fn forge_benchmark_has_no_parallel_metric_derivation() {
        let source = include_str!("bench_forge.rs");
        for prohibited in [
            "Instant::elapsed",
            "query_latency",
            "schedule_once",
            "execute_one_for_test",
        ] {
            assert!(!source.contains(prohibited));
        }
        assert!(source.contains("ForgeMaintenanceTelemetryReport::from_production_delta"));
    }

    /// Prove replacement starts remain distinct from maximum and final concurrency.
    #[test]
    fn forge_role_topology_survives_worker_replacement() {
        let expected = BTreeMap::from([("server".to_owned(), 1), ("forge_worker".to_owned(), 3)]);
        let report =
            ForgeMaintenanceTelemetryReport::from_production_delta(&complete_delta(), &expected)
                .expect("production replacement topology maps");
        assert_eq!(report.role_topology["forge_worker"].starts, 4);
        assert_eq!(report.role_topology["forge_worker"].max_active, 3);
        assert_eq!(report.role_topology["forge_worker"].final_active, 3);
    }

    /// Prove throughput uses captured output bytes divided by the capture interval.
    #[test]
    fn forge_throughput_uses_production_counter_rate() {
        let expected = BTreeMap::from([("server".to_owned(), 1), ("forge_worker".to_owned(), 3)]);
        let report =
            ForgeMaintenanceTelemetryReport::from_production_delta(&complete_delta(), &expected)
                .expect("production counter-rate delta maps");
        assert_eq!(report.throughput_mib_per_sec, 0.5);
    }

    /// Reject missing, stale, empty, wrong-topology, and open-label report windows.
    #[test]
    fn forge_telemetry_report_rejects_incomplete_windows() {
        let expected = BTreeMap::from([("server".to_owned(), 1), ("forge_worker".to_owned(), 3)]);
        let mut missing = complete_delta();
        missing.metrics.remove(0);
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&missing, &expected),
            Err(BifrostTelemetryReportError::MissingSeries { .. })
        ));
        let mut stale = complete_delta();
        stale.metrics[1].value = 0.0;
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&stale, &expected),
            Err(BifrostTelemetryReportError::StaleSeries { .. })
        ));
        let mut stale_memory = complete_delta();
        stale_memory
            .metrics
            .iter_mut()
            .find(|sample| sample.family == "bifrost_memory_reservations_total")
            .expect("memory reservation activity series")
            .value = 0.0;
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&stale_memory, &expected),
            Err(BifrostTelemetryReportError::StaleSeries { family })
                if family == "bifrost_memory_reservations_total"
        ));
        let mut empty = complete_delta();
        empty
            .metrics
            .iter_mut()
            .filter(|sample| sample.family == "bifrost_forge_task_spill_bytes")
            .for_each(|sample| sample.value = 0.0);
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&empty, &expected),
            Err(BifrostTelemetryReportError::EmptyHistogram { .. })
        ));
        let wrong = BTreeMap::from([("all".to_owned(), 1)]);
        assert_eq!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&complete_delta(), &wrong),
            Err(BifrostTelemetryReportError::TopologyMismatch)
        );
        let mut open_label = complete_delta();
        open_label.metrics[0]
            .labels
            .insert("tenant".to_owned(), "forbidden".to_owned());
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&open_label, &expected),
            Err(BifrostTelemetryReportError::Parse { .. })
        ));
        let mut missing_span = complete_delta();
        missing_span
            .spans
            .retain(|span| span.name != "bifrost.forge.cleanup");
        assert_eq!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&missing_span, &expected),
            Err(BifrostTelemetryReportError::MissingSpan {
                span: "bifrost.forge.cleanup".to_owned(),
            })
        );
        let mut renamed_span = complete_delta();
        renamed_span.spans[1].name = "bifrost.forge.task.renamed".to_owned();
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&renamed_span, &expected),
            Err(BifrostTelemetryReportError::InvalidSpan { span, .. })
                if span == "bifrost.forge.task.renamed"
        ));
        let mut missing_attribute = complete_delta();
        missing_attribute.spans[2].attributes.remove("result");
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&missing_attribute, &expected),
            Err(BifrostTelemetryReportError::InvalidSpan { span, .. })
                if span == "bifrost.forge.catalog.commit"
        ));
        let mut prohibited_attribute = complete_delta();
        prohibited_attribute.spans[3]
            .attributes
            .insert("tenant_id".to_owned(), "forbidden".to_owned());
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(
                &prohibited_attribute,
                &expected,
            ),
            Err(BifrostTelemetryReportError::InvalidSpan { span, .. })
                if span == "bifrost.forge.cleanup"
        ));
        let mut open_attribute = complete_delta();
        open_attribute.spans[0]
            .attributes
            .insert("result".to_owned(), "unknown".to_owned());
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&open_attribute, &expected),
            Err(BifrostTelemetryReportError::InvalidSpan { span, .. })
                if span == "bifrost.forge.scheduler.pass"
        ));
    }

    /// Prove causal diagnosis uses earliest-transition precedence and rejects
    /// every telemetry-to-durable binding mismatch.
    #[test]
    fn forge_causal_report_classifies_each_transition() {
        let report = causal_report_fixture();
        let empty = ForgeWorkflowInspection {
            has_demand: false,
            tasks: Vec::new(),
            active_claims: 0,
            active_attempts: 0,
            uncompacted_staging_files: 0,
        };
        let mut no_hint = report.clone();
        no_hint.accepted_hints = 0;
        no_hint.persisted_hints = 0;
        no_hint.spans.clear();
        assert_eq!(
            no_hint.diagnose(&empty, None).expect("no-hint diagnosis"),
            ForgeCausalDiagnosis::NoAcceptedHint
        );
        let mut not_persisted = no_hint.clone();
        not_persisted.accepted_hints = 1;
        assert_eq!(
            not_persisted
                .diagnose(&empty, None)
                .expect("not-persisted diagnosis"),
            ForgeCausalDiagnosis::AcceptedHintNotPersisted
        );
        let demand = ForgeWorkflowInspection {
            has_demand: true,
            ..empty.clone()
        };
        assert_eq!(
            report
                .diagnose(&demand, None)
                .expect("unplanned demand diagnosis"),
            ForgeCausalDiagnosis::PersistedDemandNotPlanned
        );
        let ready = ForgeWorkflowInspection {
            has_demand: false,
            tasks: vec![("small_files".to_owned(), "ready".to_owned())],
            active_claims: 0,
            active_attempts: 0,
            uncompacted_staging_files: 0,
        };
        assert_eq!(
            report
                .diagnose(&ready, None)
                .expect("unclaimed task diagnosis"),
            ForgeCausalDiagnosis::ReadyTaskNotClaimed
        );
        let failed = ForgeWorkflowInspection {
            tasks: vec![("small_files".to_owned(), "failed".to_owned())],
            ..ready.clone()
        };
        let mut executed = report.clone();
        executed.spans.push(causal_span(
            ForgeCausalSpanName::TaskExecute,
            "failed",
            "forge_worker",
        ));
        assert_eq!(
            executed
                .diagnose(&failed, None)
                .expect("failed attempt diagnosis"),
            ForgeCausalDiagnosis::AttemptFailedOrRetryable
        );
        let succeeded = ForgeWorkflowInspection {
            tasks: vec![("small_files".to_owned(), "succeeded".to_owned())],
            ..ready
        };
        let mut committed = report.clone();
        committed.demands_continued = 1;
        committed.spans.push(causal_span(
            ForgeCausalSpanName::TaskExecute,
            "succeeded",
            "forge_worker",
        ));
        committed.spans.push(causal_span(
            ForgeCausalSpanName::CatalogCommit,
            "succeeded",
            "forge_worker",
        ));
        assert_eq!(
            committed
                .diagnose(&succeeded, None)
                .expect("unproven commit diagnosis"),
            ForgeCausalDiagnosis::CommitDidNotReduceFileDebt
        );
        let rewrite = ForgeRewriteComparison {
            input_files: vec![
                ForgeDataFileInspection {
                    path: "a".to_owned(),
                    bytes: 1,
                },
                ForgeDataFileInspection {
                    path: "b".to_owned(),
                    bytes: 1,
                },
            ],
            input_bytes: 2,
            output_files: vec![ForgeDataFileInspection {
                path: "c".to_owned(),
                bytes: 2,
            }],
            output_bytes: 2,
        };
        assert_eq!(
            committed
                .diagnose(&succeeded, Some(&rewrite))
                .expect("converged diagnosis"),
            ForgeCausalDiagnosis::Converged
        );
        let unschedulable = ForgeWorkflowInspection {
            tasks: vec![("staging_fold".to_owned(), "unschedulable".to_owned())],
            ..empty.clone()
        };
        let mut blocked = report.clone();
        blocked.terminal_tasks.push(ForgeTerminalTaskTelemetry {
            strategy: ForgeTelemetryStrategy::StagingFold,
            result: ForgeTelemetryTaskResult::Unschedulable,
            count: 1,
            duration_seconds_sum: 0.1,
        });
        assert_eq!(
            blocked
                .diagnose(&unschedulable, None)
                .expect("terminally blocked diagnosis"),
            ForgeCausalDiagnosis::AttemptFailedOrRetryable
        );

        let durable_without_scheduler = ForgeWorkflowInspection {
            tasks: vec![("small_files".to_owned(), "ready".to_owned())],
            ..empty.clone()
        };
        let mut no_scheduler = report.clone();
        no_scheduler.spans.clear();
        assert!(matches!(
            no_scheduler.diagnose(&durable_without_scheduler, None),
            Err(BifrostTelemetryReportError::InvalidBinding { .. })
        ));
        assert!(matches!(
            executed.diagnose(&empty, None),
            Err(BifrostTelemetryReportError::InvalidBinding { .. })
        ));
        assert!(matches!(
            report.diagnose(&failed, None),
            Err(BifrostTelemetryReportError::InvalidBinding { .. })
        ));
        assert!(matches!(
            committed.diagnose(&empty, None),
            Err(BifrostTelemetryReportError::InvalidBinding { .. })
        ));
        assert!(matches!(
            executed.diagnose(&succeeded, None),
            Err(BifrostTelemetryReportError::InvalidBinding { .. })
        ));
        assert!(matches!(
            committed.diagnose(&empty, Some(&rewrite)),
            Err(BifrostTelemetryReportError::InvalidBinding { .. })
        ));
    }

    /// Construct a valid persisted-demand report before any durable task exists.
    fn causal_report_fixture() -> ForgeCausalTelemetryReport {
        ForgeCausalTelemetryReport {
            accepted_hints: 1,
            full_hints: 0,
            closed_hints: 0,
            persisted_hints: 1,
            failed_hint_persistence: 0,
            scheduler_complete: 1,
            scheduler_incomplete: 0,
            demands_drained: 0,
            demands_continued: 0,
            demand_generations_changed: 0,
            demand_transition_failures: 0,
            data_refusals: 0,
            transient_object_store_failures: 0,
            storage_health_failures: 0,
            capacity_refusals: 0,
            quarantined_workers: 0,
            planning_backlog: 1,
            oldest_demand_seconds: 0.1,
            discovered_candidates: Vec::new(),
            terminal_tasks: Vec::new(),
            stage_failures: Vec::new(),
            rewrite_input_files: 0,
            rewrite_input_bytes: 0,
            rewrite_output_files: 0,
            rewrite_output_bytes: 0,
            conflicts: 0,
            spans: vec![causal_span(
                ForgeCausalSpanName::SchedulerPass,
                "succeeded",
                "server",
            )],
        }
    }

    /// Construct one already-validated causal span for diagnosis-only tests.
    fn causal_span(name: ForgeCausalSpanName, result: &str, role: &str) -> ForgeCausalSpan {
        ForgeCausalSpan {
            name,
            result: result.to_owned(),
            role: role.to_owned(),
            task_id: matches!(
                name,
                ForgeCausalSpanName::TaskExecute
                    | ForgeCausalSpanName::CatalogCommit
                    | ForgeCausalSpanName::Cleanup
            )
            .then(|| "00000000-0000-0000-0000-000000000001".to_owned()),
            attempt_id: matches!(
                name,
                ForgeCausalSpanName::TaskExecute
                    | ForgeCausalSpanName::CatalogCommit
                    | ForgeCausalSpanName::Cleanup
            )
            .then(|| "00000000-0000-0000-0000-000000000002".to_owned()),
        }
    }
}
