//! Neutral production OTLP outcome contracts shared by Gate and Scribe.

use wyrd_tonic::otlp::logs_service::ExportLogsPartialSuccess;
use wyrd_tonic::otlp::metrics_service::ExportMetricsPartialSuccess;
use wyrd_tonic::otlp::trace_service::ExportTracePartialSuccess;

/// Accepted and rejected trace counts returned after durable Scribe ingestion.
#[derive(Debug, Clone)]
pub struct IngestOutcome {
    /// Spans committed to `traces.spans`.
    pub accepted_spans: i64,
    /// Spans rejected before commit.
    pub rejected_spans: i64,
    /// First rejection reason surfaced through OTLP partial success.
    pub rejection_message: Option<String>,
}

impl IngestOutcome {
    /// Converts rejected trace counts into the standard OTLP response field.
    #[must_use]
    pub fn partial_success(&self) -> Option<ExportTracePartialSuccess> {
        (self.rejected_spans != 0).then(|| ExportTracePartialSuccess {
            rejected_spans: self.rejected_spans,
            error_message: self
                .rejection_message
                .clone()
                .unwrap_or_else(|| "one or more spans were rejected".to_owned()),
        })
    }
}

/// Accepted and rejected metric counts returned after durable Scribe ingestion.
#[derive(Debug, Clone)]
pub struct MetricsOutcome {
    /// Metric points committed to `metrics.points`.
    pub accepted_points: i64,
    /// Metric points rejected before commit.
    pub rejected_points: i64,
    /// First rejection reason surfaced through OTLP partial success.
    pub rejection_message: Option<String>,
}

impl MetricsOutcome {
    /// Converts rejected metric counts into the standard OTLP response field.
    #[must_use]
    pub fn partial_success(&self) -> Option<ExportMetricsPartialSuccess> {
        (self.rejected_points != 0).then(|| ExportMetricsPartialSuccess {
            rejected_data_points: self.rejected_points,
            error_message: self
                .rejection_message
                .clone()
                .unwrap_or_else(|| "one or more data points were rejected".to_owned()),
        })
    }
}

/// Accepted and rejected log counts returned after durable Scribe ingestion.
#[derive(Debug, Clone)]
pub struct LogsOutcome {
    /// Log records committed to `logs.records`.
    pub accepted_records: i64,
    /// Log records rejected before commit.
    pub rejected_records: i64,
    /// First rejection reason surfaced through OTLP partial success.
    pub rejection_message: Option<String>,
}

impl LogsOutcome {
    /// Converts rejected log counts into the standard OTLP response field.
    #[must_use]
    pub fn partial_success(&self) -> Option<ExportLogsPartialSuccess> {
        (self.rejected_records != 0).then(|| ExportLogsPartialSuccess {
            rejected_log_records: self.rejected_records,
            error_message: self
                .rejection_message
                .clone()
                .unwrap_or_else(|| "one or more log records were rejected".to_owned()),
        })
    }
}
