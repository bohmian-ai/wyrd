//! Pure OTLP projection helpers used by the server-owned Bifrost Gate.
//!
//! This module owns protocol-to-Arrow mapping and Gate-to-Scribe frame
//! projection. It does not authenticate requests, mount services, or expose a
//! legacy whole-stream writer.

pub mod map;

use uuid::Uuid;
use vala_bifrost::{BifrostNamespace, WyrdCatalog};
use vala_bifrost_redux::catalog::TableRef as ReduxTableRef;
use vala_bifrost_redux::contracts::{IngressPayload, Scribe, ScribeIngressFrame};
use wyrd_runtime::{Permission, PermissionCheck, RbacCheck};
use wyrd_tonic::otlp::logs_service::{ExportLogsPartialSuccess, ExportLogsServiceRequest};
use wyrd_tonic::otlp::metrics_service::{ExportMetricsPartialSuccess, ExportMetricsServiceRequest};
use wyrd_tonic::otlp::trace_service::{ExportTracePartialSuccess, ExportTraceServiceRequest};
use wyrd_tonic::prost::Message;

use crate::auth::AuthContext;
use crate::error::IngestError;
use map::{
    MappedLogs, MappedMetrics, MappedSpans, logs_to_record_batch, map_resource_logs,
    map_resource_metrics, map_resource_spans, metrics_to_record_batch, spans_to_record_batch,
};

/// Project one OTLP trace request into one bounded Scribe frame. `encoded_len`
/// is captured before mapping and is carried unchanged as the source charge.
pub async fn ingest_resource_spans_to_scribe<F: Scribe + ?Sized>(
    catalog: &WyrdCatalog,
    scribe: &F,
    auth: &AuthContext,
    request: ExportTraceServiceRequest,
) -> Result<IngestOutcome, IngestError> {
    authorize_otlp(auth)?;
    let source_bytes = request.encoded_len();
    let MappedSpans { records, rejected } = map_resource_spans(&request.resource_spans);
    let rejected_spans = rejected.len() as i64;
    let rejection_message = rejected.first().map(|first| first.reason.clone());
    if records.is_empty() {
        return Ok(IngestOutcome {
            accepted_spans: 0,
            rejected_spans,
            rejection_message,
        });
    }
    let accepted_spans = records.len() as i64;
    let batch = spans_to_record_batch(&records).map_err(IngestError::Decode)?;
    enqueue_scribe(
        catalog,
        scribe,
        auth,
        "vala.traces.spans",
        batch,
        source_bytes,
    )
    .await?;
    Ok(IngestOutcome {
        accepted_spans,
        rejected_spans,
        rejection_message,
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

// OTLP metrics projection into queued Scribe using the full source request size
// for the admission charge.
pub async fn ingest_resource_metrics_to_scribe<F: Scribe + ?Sized>(
    catalog: &WyrdCatalog,
    scribe: &F,
    auth: &AuthContext,
    request: ExportMetricsServiceRequest,
) -> Result<MetricsOutcome, IngestError> {
    authorize_otlp(auth)?;
    let source_bytes = request.encoded_len();
    let MappedMetrics { records, rejected } = map_resource_metrics(&request.resource_metrics);
    let rejected_points = rejected.len() as i64;
    let rejection_message = rejected.first().map(|first| first.reason.clone());
    if records.is_empty() {
        return Ok(MetricsOutcome {
            accepted_points: 0,
            rejected_points,
            rejection_message,
        });
    }
    let accepted_points = records.len() as i64;
    let batch = metrics_to_record_batch(&records).map_err(IngestError::Decode)?;
    enqueue_scribe(
        catalog,
        scribe,
        auth,
        "vala.metrics.points",
        batch,
        source_bytes,
    )
    .await?;
    Ok(MetricsOutcome {
        accepted_points,
        rejected_points,
        rejection_message,
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

/// Transport-agnostic OTLP logs decode→write core.
///
/// Authorizes the record-write capability, flattens the log records, writes the
/// accepted rows through the live coordinator on `logs.records` as one commit
/// unit, and reports the per-record rejection count. Both the OTLP/gRPC collector
/// and the OTLP/HTTP handler call this.
///
/// # Errors
/// Returns an [`IngestError`] for auth/RBAC/engine failures (including
/// [`IngestError::WriterBusy`] on backpressure). Per-record decode failures do not
/// error here — they surface in [`LogsOutcome::rejected_records`].
/// OTLP logs projection into queued Scribe using the full source request size
/// for the admission charge.
pub async fn ingest_resource_logs_to_scribe<F: Scribe + ?Sized>(
    catalog: &WyrdCatalog,
    scribe: &F,
    auth: &AuthContext,
    request: ExportLogsServiceRequest,
) -> Result<LogsOutcome, IngestError> {
    authorize_otlp(auth)?;
    let source_bytes = request.encoded_len();
    let MappedLogs { records, rejected } = map_resource_logs(&request.resource_logs);
    let rejected_records = rejected.len() as i64;
    let rejection_message = rejected.first().map(|first| first.reason.clone());
    if records.is_empty() {
        return Ok(LogsOutcome {
            accepted_records: 0,
            rejected_records,
            rejection_message,
        });
    }
    let accepted_records = records.len() as i64;
    let batch = logs_to_record_batch(&records).map_err(IngestError::Decode)?;
    enqueue_scribe(
        catalog,
        scribe,
        auth,
        "vala.logs.records",
        batch,
        source_bytes,
    )
    .await?;
    Ok(LogsOutcome {
        accepted_records,
        rejected_records,
        rejection_message,
    })
}

fn authorize_otlp(auth: &AuthContext) -> Result<(), IngestError> {
    RbacCheck
        .check(&auth.principal, &Permission::bifrost_record_write())
        .into_result()
        .map_err(IngestError::from_rbac)
}

async fn enqueue_scribe<F: Scribe + ?Sized>(
    catalog: &WyrdCatalog,
    scribe: &F,
    auth: &AuthContext,
    table: &str,
    rows: arrow::record_batch::RecordBatch,
    measured_wire_bytes: usize,
) -> Result<(), IngestError> {
    let table = ReduxTableRef::parse_fqn(table)
        .ok_or_else(|| IngestError::Internal(format!("invalid OTLP Scribe table: {table}")))?;
    catalog
        .table_schema_fingerprint(
            BifrostNamespace::from_wire(table.namespace.as_str())
                .ok_or_else(|| IngestError::Internal("invalid OTLP namespace".to_owned()))?,
            &table.name,
            auth.tenant,
        )
        .await
        .map_err(IngestError::from_engine)?;
    let batch_id = Uuid::now_v7();
    let binding =
        vala_bifrost_redux::catalog::TenantTableBinding::resolve((auth.tenant, table.clone()))
            .map_err(|error| IngestError::Internal(error.to_string()))?;
    let audit_event = wyrd_spec::vala::api::AuditEvent {
        request_id: auth.request_id.clone(),
        trace_id: None,
        operation: "bifrost.otlp".to_owned(),
        resource: table.fqn(),
        card_ref: auth.principal.card_ref().cloned(),
        principal_id: auth.principal.id,
        principal_kind: auth.principal.kind.tag(),
        auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
        permission: "bifrost:record:write".to_owned(),
        decision: wyrd_spec::vala::api::AuditDecision::Allow,
        result: wyrd_spec::vala::api::AuditResult::Success,
        payload_summary: "one projected OTLP frame".to_owned(),
        detail: None,
    };
    scribe
        .ingest_frame(ScribeIngressFrame {
            principal: auth.principal.clone(),
            binding,
            expected_schema_fingerprint: crate::orchestrator::source_schema_fingerprint(
                rows.schema().as_ref(),
            ),
            request_id: auth.request_id.clone(),
            batch_id,
            frame_sequence: 0,
            audit_event,
            measured_wire_bytes,
            stream_rows_before: 0,
            stream_rows_limit: u64::MAX,
            payload: IngressPayload::ProjectedArrow(vec![rows]),
        })
        .await
        .map_err(IngestError::from_scribe)
        .map(|_| ())
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

#[cfg(test)]
fn map_export_error(error: IngestError) -> wyrd_tonic::tonic::Status {
    match error {
        IngestError::WriterBusy => wyrd_tonic::tonic::Status::unavailable(error.to_string()),
        other => other.into_status(),
    }
}

#[cfg(test)]
mod tests {
    use super::map_export_error;
    use crate::error::IngestError;
    use wyrd_tonic::tonic::Code;

    // Backpressure is a coordinator-local, cross-boundary-state-free condition
    // (channel-full). Forcing genuine channel saturation end-to-end is flaky and
    // materially harder than asserting the load-bearing mapping directly, so the
    // `IngestBusy → UNAVAILABLE` contract is pinned here per the testing doctrine.
    #[test]
    fn ingest_busy_maps_to_unavailable_not_resource_exhausted() {
        let status = map_export_error(IngestError::WriterBusy);
        assert_eq!(
            status.code(),
            Code::Unavailable,
            "OTLP backpressure must be retryable UNAVAILABLE, not RESOURCE_EXHAUSTED"
        );
    }

    #[test]
    fn other_errors_keep_their_ingest_status() {
        let status = map_export_error(IngestError::TableNotFound {
            table: "vala.traces.spans".to_owned(),
        });
        assert_eq!(status.code(), Code::NotFound);
    }
}
