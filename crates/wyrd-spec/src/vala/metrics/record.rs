//! `MetricRecord` — one OTel metric data point, flattened for columnar query, with
//! full OTLP point fidelity. Scalar (`value`) for gauge/sum; explicit-boundary histogram
//! fields for `Histogram`; base-2 exponential-bucket fields for `ExponentialHistogram`;
//! quantile list for `Summary`; zero-to-many `exemplars` on any kind. Stream identity is
//! `resource` + `scope` (instrumentation library) + `metric_name`, per the OTel metrics
//! data model. Tenant is a stamped system column, not a field (C-02).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::WyrdError;
use crate::vala::ids::{SpanId, TraceId};
use crate::vala::trace::{InstrumentationScope, Resource};

/// OTLP metric point kind. Determines which fields on `MetricRecord` are required
/// and which must be absent (enforced by `validate()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MetricType {
    /// Instantaneous measurement; `value` required.
    Gauge,
    /// Cumulative or delta counter; `value` required.
    Sum,
    /// Explicit-boundary histogram; `count` + bucket fields.
    Histogram,
    /// Base-2 exponential-boundary histogram; `count` + `scale` + bucket fields.
    ExponentialHistogram,
    /// Pre-aggregated summary (client-side percentiles); `count` + `quantile_values`.
    Summary,
}

/// OTel aggregation temporality for `Sum` and `Histogram` data points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AggregationTemporality {
    /// Each reported value covers only the measurement interval since the last report.
    Delta,
    /// Each reported value accumulates from a fixed start time.
    Cumulative,
}

/// One base-2 exponential-histogram bucket run (`ExponentialHistogram` positive or
/// negative range): `bucket_counts[i]` covers index `offset + i`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ExponentialBuckets {
    /// Index of the first bucket (`bucket_counts[0]` covers index `offset`).
    pub offset: i32,
    /// Per-bucket observation counts, starting at `offset`.
    pub bucket_counts: Vec<u64>,
}

/// One OTel Summary quantile point (`Summary` only).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct QuantileValue {
    /// In `[0, 1]` (e.g. `0.99`).
    pub quantile: f64,
    /// Observed value at this quantile.
    pub value: f64,
}

/// One OTel exemplar — a sampled measurement that seeds a data point, carrying the
/// span it came from and the attributes filtered out of the point's own attribute set.
/// A data point has zero-to-many exemplars (M-01).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MetricExemplar {
    /// `time_unix_nano` of the sampled measurement.
    pub time: DateTime<Utc>,
    /// The measured value the exemplar recorded.
    pub value: f64,
    /// Correlated span, when the measurement was recorded in a sampled trace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<TraceId>,
    /// Correlated span; requires `trace_id` when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_id: Option<SpanId>,
    /// OTel `filtered_attributes` — attributes on the measurement but not on the
    /// data point's own attribute set.
    #[serde(default)]
    pub filtered_attributes: serde_json::Map<String, serde_json::Value>,
}

/// One OTel metric data point for the `metrics.points` Bifrost table. Carries full
/// OTLP fidelity — scalar (`Gauge`/`Sum`), bucket arrays (`Histogram`/
/// `ExponentialHistogram`), or quantile list (`Summary`). `validate()` enforces
/// per-type field presence rules. Tenant is a stamped system column (C-02).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MetricRecord {
    /// `time_unix_nano` of this data point.
    pub time: DateTime<Utc>,
    /// `start_time_unix_nano` (sums/histograms; the aggregation window start).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_time: Option<DateTime<Utc>>,
    /// OTLP metric stream name (e.g. `http.server.request.duration`).
    pub metric_name: String,
    /// Human-readable metric description from the OTel SDK.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// OTLP unit string (e.g. `ms`, `By`, `{request}`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Point kind — governs which fields are required (enforced by `validate()`).
    pub metric_type: MetricType,
    /// Aggregation temporality (`Delta` or `Cumulative`); `None` for `Gauge`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temporality: Option<AggregationTemporality>,
    /// Monotonicity flag for `Sum`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_monotonic: Option<bool>,
    /// OTel data-point flags (e.g. `NO_RECORDED_VALUE`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<u32>,
    /// Scalar value for `Gauge`/`Sum` number points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// Total observation count (`Histogram`/`ExponentialHistogram`/`Summary`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
    /// Sum of all observed values (`Histogram`/`ExponentialHistogram`/`Sum`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sum: Option<f64>,
    /// Minimum observed value (`Histogram`/`ExponentialHistogram`; OTLP-optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// Maximum observed value (`Histogram`/`ExponentialHistogram`; OTLP-optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// Explicit-boundary `Histogram` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bucket_counts: Option<Vec<u64>>,
    /// Boundary values; `bucket_counts.len() == explicit_bounds.len() + 1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explicit_bounds: Option<Vec<f64>>,
    /// `ExponentialHistogram` only (base-2 buckets).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<i32>,
    /// Count of observations in the zero bucket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zero_count: Option<u64>,
    /// Half-width of the zero bucket (OTLP-optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zero_threshold: Option<f64>,
    /// Positive-range exponential buckets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub positive_buckets: Option<ExponentialBuckets>,
    /// Negative-range exponential buckets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub negative_buckets: Option<ExponentialBuckets>,
    /// `Summary` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantile_values: Option<Vec<QuantileValue>>,
    /// OTel exemplars for this data point — zero-to-many, valid across every point kind.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exemplars: Vec<MetricExemplar>,
    /// Producer resource (service, host, etc.) — OTLP-required at the stream level.
    pub resource: Resource,
    /// OTel instrumentation scope of the emitting library (M-01). Reuses the landed
    /// `wyrd_spec::vala::trace::InstrumentationScope`. `None` for collector/import compat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<InstrumentationScope>,
    /// OTel data-point attributes.
    #[serde(default)]
    pub attributes: serde_json::Map<String, serde_json::Value>,
}

impl MetricRecord {
    /// Validates per-type field presence and shape.
    ///
    /// # Errors
    /// - `Gauge`/`Sum`: `value` required.
    /// - `Histogram`: `count` required; when both present, `bucket_counts.len() ==
    ///   explicit_bounds.len() + 1`; exponential/summary fields absent.
    /// - `ExponentialHistogram`: `count` + `scale` required; `bucket_counts`/
    ///   `explicit_bounds`/`quantile_values` absent.
    /// - `Summary`: `count` + `quantile_values` required (each `quantile ∈ [0,1]`);
    ///   histogram bucket fields absent.
    /// - An exemplar with a `span_id` also carries a `trace_id`.
    pub fn validate(&self) -> Result<(), WyrdError> {
        let err = |msg: &str| {
            Err(WyrdError::Validation {
                message: msg.to_string(),
                details: serde_json::Value::Null,
            })
        };

        for ex in &self.exemplars {
            if ex.span_id.is_some() && ex.trace_id.is_none() {
                return err("exemplar span_id requires trace_id");
            }
        }

        match self.metric_type {
            MetricType::Gauge | MetricType::Sum => {
                if self.value.is_none() {
                    return err("Gauge/Sum requires value");
                }
            }
            MetricType::Histogram => {
                if self.count.is_none() {
                    return err("Histogram requires count");
                }
                if let (Some(bc), Some(eb)) = (&self.bucket_counts, &self.explicit_bounds)
                    && bc.len() != eb.len() + 1
                {
                    return err(
                        "Histogram: bucket_counts.len() must equal explicit_bounds.len() + 1",
                    );
                }
                if self.scale.is_some()
                    || self.positive_buckets.is_some()
                    || self.negative_buckets.is_some()
                {
                    return err("Histogram must not carry exponential-histogram fields");
                }
                if self.quantile_values.is_some() {
                    return err("Histogram must not carry quantile_values");
                }
            }
            MetricType::ExponentialHistogram => {
                if self.count.is_none() {
                    return err("ExponentialHistogram requires count");
                }
                if self.scale.is_none() {
                    return err("ExponentialHistogram requires scale");
                }
                if self.bucket_counts.is_some() || self.explicit_bounds.is_some() {
                    return err(
                        "ExponentialHistogram must not carry explicit-boundary histogram fields",
                    );
                }
                if self.quantile_values.is_some() {
                    return err("ExponentialHistogram must not carry quantile_values");
                }
            }
            MetricType::Summary => {
                if self.count.is_none() {
                    return err("Summary requires count");
                }
                let qv = self.quantile_values.as_deref().unwrap_or(&[]);
                if qv.is_empty() {
                    return err("Summary requires quantile_values");
                }
                for q in qv {
                    if !(0.0..=1.0).contains(&q.quantile) {
                        return err("Summary quantile must be in [0, 1]");
                    }
                }
                if self.bucket_counts.is_some() || self.explicit_bounds.is_some() {
                    return err("Summary must not carry explicit-boundary histogram fields");
                }
            }
        }
        Ok(())
    }
}
