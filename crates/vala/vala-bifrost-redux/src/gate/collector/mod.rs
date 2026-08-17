//! Legacy/test OTLP projection helpers retained for parity verification.
//!
//! Production adapters transfer fixed-capacity typed OTLP requests and their
//! move-only decode owners through routing-only Gate. Scribe owns catalog
//! resolution, projection planning and materialization, physical binding,
//! material admission, WAL durability, visibility, and acknowledgment. These
//! IO-free helpers remain only for reusable mapping and test or benchmark
//! comparisons; they do not describe the production ownership boundary.

#[cfg(any(test, feature = "test-support"))]
pub mod map;
pub(crate) mod tables;

#[cfg(any(test, feature = "test-support"))]
use arrow::record_batch::RecordBatch;
use wyrd_tonic::otlp::logs_service::ExportLogsPartialSuccess;
#[cfg(any(test, feature = "test-support"))]
use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
use wyrd_tonic::otlp::metrics_service::ExportMetricsPartialSuccess;
#[cfg(any(test, feature = "test-support"))]
use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;
use wyrd_tonic::otlp::trace_service::ExportTracePartialSuccess;
#[cfg(any(test, feature = "test-support"))]
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
#[cfg(any(test, feature = "test-support"))]
use wyrd_tonic::prost::Message;

#[cfg(any(test, feature = "test-support"))]
use super::error::IngestError;
#[cfg(any(test, feature = "test-support"))]
use map::{
    MappedLogs, MappedMetrics, MappedSpans, logs_to_record_batch, map_resource_logs,
    map_resource_metrics, map_resource_spans, metrics_to_record_batch, spans_to_record_batch,
};

/// Legacy/test result of projecting one typed OTLP request without persistence.
///
/// Production Gate never constructs this value or dispatches its projected
/// rows; Scribe performs the authoritative current-slice projection.
#[derive(Debug)]
#[cfg(any(test, feature = "test-support"))]
pub struct ProjectedExport<T> {
    pub outcome: T,
    pub batch: Option<RecordBatch>,
    pub source_bytes: usize,
}

/// Projects one OTLP trace request for legacy/test parity without writing.
#[cfg(any(test, feature = "test-support"))]
pub fn project_resource_spans(
    request: &ExportTraceServiceRequest,
) -> Result<ProjectedExport<IngestOutcome>, IngestError> {
    let source_bytes = request.encoded_len();
    let MappedSpans { records, rejected } = map_resource_spans(&request.resource_spans);
    let rejected_spans = i64::try_from(rejected.len()).unwrap_or(i64::MAX);
    let rejection_message = rejected.first().map(|first| first.reason.clone());
    if records.is_empty() {
        return Ok(ProjectedExport {
            outcome: IngestOutcome {
                accepted_spans: 0,
                rejected_spans,
                rejection_message,
            },
            batch: None,
            source_bytes,
        });
    }
    let accepted_spans = i64::try_from(records.len()).unwrap_or(i64::MAX);
    let batch = spans_to_record_batch(&records).map_err(IngestError::Decode)?;
    Ok(ProjectedExport {
        outcome: IngestOutcome {
            accepted_spans,
            rejected_spans,
            rejection_message,
        },
        batch: Some(batch),
        source_bytes,
    })
}

/// Result of one accepted OTLP export: how many spans committed and how many the
/// receiver dropped for a per-span reason.
#[derive(Debug, Clone)]
pub struct IngestOutcome {
    /// Spans committed to `traces.spans`.
    pub accepted_spans: i64,
    /// Spans dropped for a per-span reason (surfaced as `partial_success`).
    pub rejected_spans: i64,
    /// First rejection reason, for the OTLP `error_message`.
    pub rejection_message: Option<String>,
}

impl IngestOutcome {
    /// Render the OTLP `partial_success` field, or `None` on a full success.
    ///
    /// Shared by both OTLP transports so the `rejected_spans` / `error_message`
    /// contract is identical on gRPC and HTTP.
    #[must_use]
    pub fn partial_success(&self) -> Option<ExportTracePartialSuccess> {
        if self.rejected_spans == 0 {
            return None;
        }
        Some(ExportTracePartialSuccess {
            rejected_spans: self.rejected_spans,
            error_message: self
                .rejection_message
                .clone()
                .unwrap_or_else(|| "one or more spans were rejected".to_owned()),
        })
    }
}

/// Projects one OTLP metrics request for legacy/test parity without writing.
#[cfg(any(test, feature = "test-support"))]
pub fn project_resource_metrics(
    request: &ExportMetricsServiceRequest,
) -> Result<ProjectedExport<MetricsOutcome>, IngestError> {
    let source_bytes = request.encoded_len();
    let MappedMetrics { records, rejected } = map_resource_metrics(&request.resource_metrics);
    let rejected_points = i64::try_from(rejected.len()).unwrap_or(i64::MAX);
    let rejection_message = rejected.first().map(|first| first.reason.clone());
    if records.is_empty() {
        return Ok(ProjectedExport {
            outcome: MetricsOutcome {
                accepted_points: 0,
                rejected_points,
                rejection_message,
            },
            batch: None,
            source_bytes,
        });
    }
    let accepted_points = i64::try_from(records.len()).unwrap_or(i64::MAX);
    let batch = metrics_to_record_batch(&records).map_err(IngestError::Decode)?;
    Ok(ProjectedExport {
        outcome: MetricsOutcome {
            accepted_points,
            rejected_points,
            rejection_message,
        },
        batch: Some(batch),
        source_bytes,
    })
}

/// Result of one accepted OTLP metrics export: committed vs. dropped data points.
#[derive(Debug, Clone)]
pub struct MetricsOutcome {
    /// Data points committed to `metrics.points`.
    pub accepted_points: i64,
    /// Data points dropped for a per-point reason (surfaced as `partial_success`).
    pub rejected_points: i64,
    /// First rejection reason, for the OTLP `error_message`.
    pub rejection_message: Option<String>,
}

impl MetricsOutcome {
    /// Render the OTLP `partial_success` field, or `None` on a full success.
    #[must_use]
    pub fn partial_success(&self) -> Option<ExportMetricsPartialSuccess> {
        if self.rejected_points == 0 {
            return None;
        }
        Some(ExportMetricsPartialSuccess {
            rejected_data_points: self.rejected_points,
            error_message: self
                .rejection_message
                .clone()
                .unwrap_or_else(|| "one or more data points were rejected".to_owned()),
        })
    }
}

/// Projects one OTLP logs request for legacy/test parity without writing.
#[cfg(any(test, feature = "test-support"))]
pub fn project_resource_logs(
    request: &ExportLogsServiceRequest,
) -> Result<ProjectedExport<LogsOutcome>, IngestError> {
    let source_bytes = request.encoded_len();
    let MappedLogs { records, rejected } = map_resource_logs(&request.resource_logs);
    let rejected_records = i64::try_from(rejected.len()).unwrap_or(i64::MAX);
    let rejection_message = rejected.first().map(|first| first.reason.clone());
    if records.is_empty() {
        return Ok(ProjectedExport {
            outcome: LogsOutcome {
                accepted_records: 0,
                rejected_records,
                rejection_message,
            },
            batch: None,
            source_bytes,
        });
    }
    let accepted_records = i64::try_from(records.len()).unwrap_or(i64::MAX);
    let batch = logs_to_record_batch(&records).map_err(IngestError::Decode)?;
    Ok(ProjectedExport {
        outcome: LogsOutcome {
            accepted_records,
            rejected_records,
            rejection_message,
        },
        batch: Some(batch),
        source_bytes,
    })
}

/// Result of one accepted OTLP logs export: committed vs. dropped records.
#[derive(Debug, Clone)]
pub struct LogsOutcome {
    /// Records committed to `logs.records`.
    pub accepted_records: i64,
    /// Records dropped for a per-record reason (surfaced as `partial_success`).
    pub rejected_records: i64,
    /// First rejection reason, for the OTLP `error_message`.
    pub rejection_message: Option<String>,
}

impl LogsOutcome {
    /// Render the OTLP `partial_success` field, or `None` on a full success.
    #[must_use]
    pub fn partial_success(&self) -> Option<ExportLogsPartialSuccess> {
        if self.rejected_records == 0 {
            return None;
        }
        Some(ExportLogsPartialSuccess {
            rejected_log_records: self.rejected_records,
            error_message: self
                .rejection_message
                .clone()
                .unwrap_or_else(|| "one or more log records were rejected".to_owned()),
        })
    }
}

mod tests {}
