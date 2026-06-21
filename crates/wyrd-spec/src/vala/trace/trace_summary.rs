//! Per-trace rollup record.
//!
//! Computed by `vala-bifrost` once a trace's spans are assembled. One row
//! per `(data_tenant_id, trace_id, bucket_time)`. Wyrd lifts `bucket_time`
//! (the `trace_bounds_lookup` index column) into the record so `vala-bifrost`
//! does not recompute it at SQL time; predecessor implementations computed it
//! inline against `start_time`.
//!
//! Root-span metadata (`root_span_id`, `root_name`, `root_kind`,
//! `root_status`, `root_scope`) is populated when a single span has no
//! `parent_span_id` within the trace. Multi-root traces get
//! `root_span_id = None` and the remaining root fields all `None` or default;
//! the OLAP layer treats these as anonymous traces.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::WyrdError;
use crate::ids::DataTenantId;
use crate::vala::ids::{SpanId, TraceId};
use crate::vala::trace::{InstrumentationScope, Resource, SpanKind, SpanStatus};

/// Per-trace rollup. One row per `(data_tenant_id, trace_id, bucket_time)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TraceSummaryRecord {
    /// 16-byte trace id.
    pub trace_id: TraceId,
    /// Tenant partition key.
    pub data_tenant_id: DataTenantId,
    /// Floor-bucketed time the trace started in.
    ///
    /// Backs the catalog's `trace_bounds_lookup` index for `trace_summaries`.
    /// Not a partition key — partitioning is the hidden Iceberg
    /// `day(wyrd_event_time)` transform. Default bucket width is 1 minute;
    /// callers must floor `start_time` to the bucket boundary before
    /// constructing.
    pub bucket_time: DateTime<Utc>,
    /// Earliest `start_time` across all spans in the trace.
    pub start_time: DateTime<Utc>,
    /// Latest `end_time` across all spans in the trace.
    pub end_time: DateTime<Utc>,
    /// `(end_time - start_time).num_milliseconds() as u64`.
    ///
    /// Consistency is asserted by [`TraceSummaryRecord::validate`].
    pub duration_ms: u64,
    /// Total span count in the trace.
    pub span_count: u32,
    /// Span count with [`SpanStatus::Error`]. Must be less than or equal to
    /// `span_count`.
    pub error_count: u32,
    /// Root span id, when a single root exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_span_id: Option<SpanId>,
    /// Root span name, when a single root exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_name: Option<String>,
    /// Root span kind, when a single root exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_kind: Option<SpanKind>,
    /// Root span status.
    ///
    /// Defaults to [`SpanStatus::Unset`] when no root is identifiable;
    /// otherwise mirrors the root span's status.
    pub root_status: SpanStatus,
    /// Root span's instrumentation scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_scope: Option<InstrumentationScope>,
    /// Resource of the root span.
    ///
    /// For multi-root traces, this is the resource selected by the downstream
    /// rollup computation.
    pub resource: Resource,
}

impl TraceSummaryRecord {
    /// Validate trace-summary invariants.
    ///
    /// Enforces:
    ///
    /// - `span_count >= 1`
    /// - `error_count <= span_count`
    /// - `end_time >= start_time`
    /// - `duration_ms == (end_time - start_time).num_milliseconds() as u64`
    /// - `bucket_time <= start_time`
    /// - `resource.validate()` succeeds
    /// - `root_scope.validate()` succeeds when present
    /// - `root_name` is non-empty and no longer than 256 characters when
    ///   present
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when any invariant fails.
    pub fn validate(&self) -> Result<(), WyrdError> {
        if self.span_count == 0 {
            return Err(WyrdError::Validation {
                message:
                    "trace_summary.span_count must be >= 1 (zero-span trace is not summarizable)"
                        .to_string(),
                details: serde_json::Value::Null,
            });
        }
        if self.error_count > self.span_count {
            return Err(WyrdError::Validation {
                message: format!(
                    "trace_summary.error_count {} > span_count {}",
                    self.error_count, self.span_count
                ),
                details: serde_json::Value::Null,
            });
        }
        if self.end_time < self.start_time {
            return Err(WyrdError::Validation {
                message: format!(
                    "trace_summary.end_time {:?} < start_time {:?}",
                    self.end_time, self.start_time
                ),
                details: serde_json::Value::Null,
            });
        }
        let derived = super::duration_ms_from_timestamps(self.start_time, self.end_time);
        if self.duration_ms != derived {
            return Err(WyrdError::Validation {
                message: format!(
                    "trace_summary.duration_ms {} != end_time - start_time = {}",
                    self.duration_ms, derived
                ),
                details: serde_json::Value::Null,
            });
        }
        if self.bucket_time > self.start_time {
            return Err(WyrdError::Validation {
                message: format!(
                    "trace_summary.bucket_time {:?} > start_time {:?} (bucket must be a floor)",
                    self.bucket_time, self.start_time
                ),
                details: serde_json::Value::Null,
            });
        }
        self.resource.validate()?;
        if let Some(scope) = &self.root_scope {
            scope.validate()?;
        }
        if let Some(name) = &self.root_name {
            if name.is_empty() {
                return Err(WyrdError::Validation {
                    message: "trace_summary.root_name must be non-empty when present".to_string(),
                    details: serde_json::Value::Null,
                });
            }
            if name.len() > 256 {
                return Err(WyrdError::Validation {
                    message: format!(
                        "trace_summary.root_name length must be <= 256, got {}",
                        name.len()
                    ),
                    details: serde_json::Value::Null,
                });
            }
        }
        Ok(())
    }
}
