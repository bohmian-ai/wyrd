//! Pure OTLP projection helpers used by the Redux-owned Bifrost Gate.
//!
//! This module owns protocol-to-Arrow mapping and Gate-to-Scribe frame
//! projection. It does not authenticate requests, mount services, or expose a
//! legacy whole-stream writer.

pub mod map;
mod tables;

use crate::contracts::ScribeError;
use crate::schema::fingerprint::SchemaFingerprint;
use crate::scribe::execution_lanes::ScribeIngressCpuPool;
use arrow::datatypes::{Field, Schema};
use arrow::record_batch::RecordBatch;
use wyrd_tonic::otlp::logs_service::{ExportLogsPartialSuccess, ExportLogsServiceRequest};
use wyrd_tonic::otlp::metrics_service::{ExportMetricsPartialSuccess, ExportMetricsServiceRequest};
use wyrd_tonic::otlp::trace_service::{ExportTracePartialSuccess, ExportTraceServiceRequest};
use wyrd_tonic::prost::Message;

use super::error::IngestError;
use map::{
    MappedLogs, MappedMetrics, MappedSpans, logs_to_record_batch, map_resource_logs,
    map_resource_metrics, map_resource_spans, metrics_to_record_batch, spans_to_record_batch,
};

/// The result of projecting one OTLP request. The Gate owns authorization,
/// catalog resolution, audit, and dispatch of the projected rows.
#[derive(Debug)]
pub struct ProjectedExport<T> {
    pub outcome: T,
    pub batch: Option<RecordBatch>,
    pub source_bytes: usize,
}

#[async_trait::async_trait]
pub trait ProjectionExecutor: Send + Sync {
    async fn project_spans(
        &self,
        request: ExportTraceServiceRequest,
    ) -> Result<ProjectedExport<IngestOutcome>, IngestError>;

    async fn project_metrics(
        &self,
        request: ExportMetricsServiceRequest,
    ) -> Result<ProjectedExport<MetricsOutcome>, IngestError>;

    async fn project_logs(
        &self,
        request: ExportLogsServiceRequest,
    ) -> Result<ProjectedExport<LogsOutcome>, IngestError>;
}

#[derive(Debug, Default)]
pub struct InlineProjectionExecutor;

#[async_trait::async_trait]
impl ProjectionExecutor for InlineProjectionExecutor {
    async fn project_spans(
        &self,
        request: ExportTraceServiceRequest,
    ) -> Result<ProjectedExport<IngestOutcome>, IngestError> {
        project_resource_spans(&request)
    }

    async fn project_metrics(
        &self,
        request: ExportMetricsServiceRequest,
    ) -> Result<ProjectedExport<MetricsOutcome>, IngestError> {
        project_resource_metrics(&request)
    }

    async fn project_logs(
        &self,
        request: ExportLogsServiceRequest,
    ) -> Result<ProjectedExport<LogsOutcome>, IngestError> {
        project_resource_logs(&request)
    }
}

#[derive(Debug, Clone)]
pub struct IngressCpuProjection {
    pool: ScribeIngressCpuPool,
}

impl IngressCpuProjection {
    #[must_use]
    pub fn new(pool: ScribeIngressCpuPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ProjectionExecutor for IngressCpuProjection {
    async fn project_spans(
        &self,
        request: ExportTraceServiceRequest,
    ) -> Result<ProjectedExport<IngestOutcome>, IngestError> {
        self.pool
            .run(move || project_resource_spans(&request).map_err(|_| ScribeError::InvalidFrame))
            .await
            .map_err(IngestError::from_scribe)
    }

    async fn project_metrics(
        &self,
        request: ExportMetricsServiceRequest,
    ) -> Result<ProjectedExport<MetricsOutcome>, IngestError> {
        self.pool
            .run(move || project_resource_metrics(&request).map_err(|_| ScribeError::InvalidFrame))
            .await
            .map_err(IngestError::from_scribe)
    }

    async fn project_logs(
        &self,
        request: ExportLogsServiceRequest,
    ) -> Result<ProjectedExport<LogsOutcome>, IngestError> {
        self.pool
            .run(move || project_resource_logs(&request).map_err(|_| ScribeError::InvalidFrame))
            .await
            .map_err(IngestError::from_scribe)
    }
}

/// Fingerprint the user-owned columns before Scribe adds server correlation
/// columns.
pub fn source_schema_fingerprint(schema: &Schema) -> SchemaFingerprint {
    let fields: Vec<Field> = schema
        .fields()
        .iter()
        .filter(|field| {
            !matches!(
                field.name().as_str(),
                "card_ref" | "card_uid" | "principal_id" | "run_id" | "data_tenant_id"
            ) && !field.name().starts_with("wyrd_")
        })
        .map(|field| field.as_ref().clone())
        .collect();
    SchemaFingerprint::from_arrow_schema(&Schema::new(fields))
}

/// Project one OTLP trace request into Arrow rows without performing a write.
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
#[derive(Debug)]
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

/// Project one OTLP metrics request into Arrow rows without performing a write.
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
#[derive(Debug)]
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

/// Project one OTLP logs request into Arrow rows without performing a write.
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
#[derive(Debug)]
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
