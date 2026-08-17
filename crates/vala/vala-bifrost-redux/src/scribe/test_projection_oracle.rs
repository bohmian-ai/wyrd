//! Test-only whole-request projection oracle for direct Scribe parity.

use arrow::record_batch::RecordBatch;
use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
use wyrd_tonic::prost::Message;

use crate::gate::error::IngestError;
pub use crate::otlp_contract::{IngestOutcome, LogsOutcome, MetricsOutcome};

pub(crate) mod map;
use map::{
    MappedLogs, MappedMetrics, MappedSpans, logs_to_record_batch, map_resource_logs,
    map_resource_metrics, map_resource_spans, metrics_to_record_batch, spans_to_record_batch,
};

/// One test-only whole-request projection and its source accounting.
#[derive(Debug)]
pub struct ProjectedExport<T> {
    /// Signal-specific acceptance result.
    pub outcome: T,
    /// Projected rows, absent when every input record was rejected.
    pub batch: Option<RecordBatch>,
    /// Original protobuf length used by parity assertions.
    pub source_bytes: usize,
}

/// Projects one trace export solely as a direct-Scribe parity oracle.
///
/// # Errors
///
/// Returns [`IngestError::Decode`] when the legacy mapper cannot build Arrow.
pub fn project_resource_spans(
    request: &ExportTraceServiceRequest,
) -> Result<ProjectedExport<IngestOutcome>, IngestError> {
    let source_bytes = request.encoded_len();
    let MappedSpans { records, rejected } = map_resource_spans(&request.resource_spans);
    let rejected_spans = i64::try_from(rejected.len()).unwrap_or(i64::MAX);
    let rejection_message = rejected.first().map(|value| value.reason.clone());
    let batch = (!records.is_empty())
        .then(|| spans_to_record_batch(&records).map_err(IngestError::Decode))
        .transpose()?;
    Ok(ProjectedExport {
        outcome: IngestOutcome {
            accepted_spans: i64::try_from(records.len()).unwrap_or(i64::MAX),
            rejected_spans,
            rejection_message,
        },
        batch,
        source_bytes,
    })
}

/// Projects one metrics export solely as a direct-Scribe parity oracle.
///
/// # Errors
///
/// Returns [`IngestError::Decode`] when the legacy mapper cannot build Arrow.
pub fn project_resource_metrics(
    request: &ExportMetricsServiceRequest,
) -> Result<ProjectedExport<MetricsOutcome>, IngestError> {
    let source_bytes = request.encoded_len();
    let MappedMetrics { records, rejected } = map_resource_metrics(&request.resource_metrics);
    let rejected_points = i64::try_from(rejected.len()).unwrap_or(i64::MAX);
    let rejection_message = rejected.first().map(|value| value.reason.clone());
    let batch = (!records.is_empty())
        .then(|| metrics_to_record_batch(&records).map_err(IngestError::Decode))
        .transpose()?;
    Ok(ProjectedExport {
        outcome: MetricsOutcome {
            accepted_points: i64::try_from(records.len()).unwrap_or(i64::MAX),
            rejected_points,
            rejection_message,
        },
        batch,
        source_bytes,
    })
}

/// Projects one logs export solely as a direct-Scribe parity oracle.
///
/// # Errors
///
/// Returns [`IngestError::Decode`] when the legacy mapper cannot build Arrow.
pub fn project_resource_logs(
    request: &ExportLogsServiceRequest,
) -> Result<ProjectedExport<LogsOutcome>, IngestError> {
    let source_bytes = request.encoded_len();
    let MappedLogs { records, rejected } = map_resource_logs(&request.resource_logs);
    let rejected_records = i64::try_from(rejected.len()).unwrap_or(i64::MAX);
    let rejection_message = rejected.first().map(|value| value.reason.clone());
    let batch = (!records.is_empty())
        .then(|| logs_to_record_batch(&records).map_err(IngestError::Decode))
        .transpose()?;
    Ok(ProjectedExport {
        outcome: LogsOutcome {
            accepted_records: i64::try_from(records.len()).unwrap_or(i64::MAX),
            rejected_records,
            rejection_message,
        },
        batch,
        source_bytes,
    })
}
