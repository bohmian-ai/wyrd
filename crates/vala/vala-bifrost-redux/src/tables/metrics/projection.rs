//! Table-owned `OTLP` metric projection for `vala.metrics.points`.
//!
//! [`project_resource_metrics`] is the single deterministic, IO-free conversion
//! from decoded `OTLP` resource metrics into canonical point rows. One accepted
//! row is one data point of one metric, and its declared `metric_type` decides
//! which kind-specific columns it owns. No value is narrowed: the integer and
//! double alternatives keep separate typed columns, bucket and quantile
//! collections stay nested typed collections, and every accepted double keeps
//! its exact IEEE bits.
//!
//! Column order and field meaning come exclusively from
//! [`super::points::METRIC_FIELDS`].

use arrow::array::{Array, ArrayRef, StringArray};
use arrow::datatypes::Fields;
use arrow::datatypes::Schema;
use arrow::record_batch::RecordBatch;
use std::cmp::Ordering;
use std::sync::Arc;
use wyrd_tonic::otlp::metrics::v1::{
    Exemplar, ExponentialHistogramDataPoint, HistogramDataPoint, Metric, NumberDataPoint,
    ResourceMetrics, SummaryDataPoint, exemplar, metric, number_data_point,
};

use super::points::{
    BUCKET_COUNT_ELEMENT, EXEMPLAR_ELEMENT, EXPLICIT_BOUND_ELEMENT, METRIC_ENTITY_REF_ELEMENT,
    METRIC_FIELDS, NEGATIVE_BUCKET_COUNT_ELEMENT, NEGATIVE_BUCKET_FIELDS,
    POSITIVE_BUCKET_COUNT_ELEMENT, POSITIVE_BUCKET_FIELDS, QUANTILE_VALUE_ELEMENT,
};
use crate::otlp_contract::MetricsOutcome;
use crate::tables::TableError;
use crate::tables::fields::canonical_arrow_fields;
use crate::tables::signal::{
    ResourceEnvelope, ScopeEnvelope, binary_column, bool_column, bool_opt_column,
    encode_attributes, f64_column, f64_opt_column, fixed_binary_opt_column, i32_column,
    i32_opt_column, i64_opt_column, internal, list_column, nested_fields, span_id_bytes,
    struct_column, trace_id_bytes, u32_column, u64_column, u64_opt_column, utf8_column,
    validate_canonical_user_batch,
};

/// Largest accepted metric name, in bytes.
const MAX_METRIC_NAME_BYTES: usize = 256;
/// Largest accepted metric description, in bytes.
const MAX_DESCRIPTION_BYTES: usize = 1024;
/// Largest accepted metric unit, in bytes.
const MAX_UNIT_BYTES: usize = 64;
/// Largest accepted number of explicit histogram buckets in one point.
const MAX_BUCKETS: usize = 4096;
/// Largest accepted number of quantiles in one summary point.
const MAX_QUANTILES: usize = 256;
/// Largest accepted number of exemplars attached to one point.
const MAX_EXEMPLARS: usize = 64;

/// Canonical `metric_type` discriminant of a gauge point.
pub const METRIC_TYPE_GAUGE: &str = "gauge";
/// Canonical `metric_type` discriminant of a sum point.
pub const METRIC_TYPE_SUM: &str = "sum";
/// Canonical `metric_type` discriminant of an explicit-bucket histogram point.
pub const METRIC_TYPE_HISTOGRAM: &str = "histogram";
/// Canonical `metric_type` discriminant of an exponential histogram point.
pub const METRIC_TYPE_EXPONENTIAL_HISTOGRAM: &str = "exponential_histogram";
/// Canonical `metric_type` discriminant of a summary point.
pub const METRIC_TYPE_SUMMARY: &str = "summary";

/// The columns each `metric_type` is permitted to populate.
///
/// Every kind-specific column absent from a kind's entry must be null on that
/// kind's rows. This is the one declaration of the per-kind required/null
/// shape; both the projector and [`validate_metric_points`] read it rather than
/// restating the rule.
const KIND_COLUMNS: [(&str, &[&str]); 5] = [
    (METRIC_TYPE_GAUGE, &["int_value", "double_value"]),
    (
        METRIC_TYPE_SUM,
        &[
            "int_value",
            "double_value",
            "aggregation_temporality",
            "is_monotonic",
        ],
    ),
    (
        METRIC_TYPE_HISTOGRAM,
        &[
            "aggregation_temporality",
            "histogram_count",
            "histogram_sum",
            "histogram_min",
            "histogram_max",
            "bucket_counts",
            "explicit_bounds",
        ],
    ),
    (
        METRIC_TYPE_EXPONENTIAL_HISTOGRAM,
        &[
            "aggregation_temporality",
            "histogram_count",
            "histogram_sum",
            "histogram_min",
            "histogram_max",
            "exponential_scale",
            "exponential_zero_count",
            "exponential_zero_threshold",
            "positive_buckets",
            "negative_buckets",
        ],
    ),
    (
        METRIC_TYPE_SUMMARY,
        &["summary_count", "summary_sum", "quantile_values"],
    ),
];

/// Every kind-specific column, in ledger order.
///
/// A column outside this list is common to all kinds and is never nulled by the
/// per-kind shape rule.
const KIND_SPECIFIC_COLUMNS: [&str; 20] = [
    "int_value",
    "double_value",
    "aggregation_temporality",
    "is_monotonic",
    "histogram_count",
    "histogram_sum",
    "histogram_min",
    "histogram_max",
    "bucket_counts",
    "explicit_bounds",
    "exponential_scale",
    "exponential_zero_count",
    "exponential_zero_threshold",
    "positive_buckets",
    "negative_buckets",
    "summary_count",
    "summary_sum",
    "quantile_values",
    "int_value",
    "double_value",
];

/// Project decoded `OTLP` resource metrics into one canonical point batch.
///
/// Each data point is atomic: it is accepted with its complete value shape or
/// rejected whole, and a rejected point contributes no partial row and no
/// partial nested collection. The returned [`MetricsOutcome`] carries the exact
/// counts plus the first rejection reason for the existing `OTLP`
/// partial-success response.
///
/// # Errors
///
/// Returns [`TableError::Internal`] only when the accepted rows cannot be
/// assembled into the canonical Arrow batch.
pub fn project_resource_metrics(
    resource_metrics: &[ResourceMetrics],
) -> Result<(RecordBatch, MetricsOutcome), TableError> {
    let mut columns = PointColumns::default();
    let mut rejected: i64 = 0;
    let mut rejection_message: Option<String> = None;
    let reject =
        |reason: &'static str, count: usize, rejected: &mut i64, message: &mut Option<String>| {
            *rejected = rejected.saturating_add(i64::try_from(count).unwrap_or(i64::MAX));
            message.get_or_insert_with(|| reason.to_owned());
        };

    for resource in resource_metrics {
        let envelope = ResourceEnvelope::project(resource.resource.as_ref(), &resource.schema_url);
        for scope in &resource.scope_metrics {
            let scope_envelope = ScopeEnvelope::project(scope.scope.as_ref(), &scope.schema_url);
            for metric in &scope.metrics {
                let descriptor = match MetricDescriptor::project(metric) {
                    Ok(descriptor) => descriptor,
                    Err(reason) => {
                        reject(
                            reason,
                            point_count(metric),
                            &mut rejected,
                            &mut rejection_message,
                        );
                        continue;
                    }
                };
                for row in point_rows(metric) {
                    match row {
                        Ok(row) => columns.push(&descriptor, row, &envelope, &scope_envelope),
                        Err(reason) => {
                            reject(reason, 1, &mut rejected, &mut rejection_message);
                        }
                    }
                }
            }
        }
    }

    let accepted = i64::try_from(columns.rows)
        .map_err(|_| TableError::Internal("accepted point count exceeds i64".to_owned()))?;
    let batch = columns.finish()?;
    Ok((
        batch,
        MetricsOutcome {
            accepted_points: accepted,
            rejected_points: rejected,
            rejection_message,
        },
    ))
}

/// Return the canonical user-column schema of `vala.metrics.points`.
#[must_use]
pub fn canonical_metric_schema() -> Arc<Schema> {
    Arc::new(Schema::new(canonical_arrow_fields(METRIC_FIELDS)))
}

/// Validate a supplied canonical metric batch, including its per-kind shape.
///
/// This extends the shared ledger validation with the one rule the ledger
/// itself cannot express: a point may populate only the kind-specific columns
/// its declared `metric_type` owns, and a numeric point must carry exactly one
/// of its two value alternatives.
///
/// # Errors
///
/// Returns a describing message when the supplied schema drifts from the
/// ledger, a canonical payload is not canonical, `metric_type` is unknown, a
/// column outside the declared kind is populated, or a numeric point carries
/// both or neither value alternative.
pub fn validate_metric_points(batch: &RecordBatch) -> Result<RecordBatch, String> {
    let batch = validate_canonical_user_batch(METRIC_FIELDS, batch)?;
    let kinds = batch
        .column_by_name("metric_type")
        .and_then(|column| column.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| "metric_type is not the declared Utf8 column".to_owned())?;

    for row in 0..batch.num_rows() {
        let kind = kinds.value(row);
        let permitted = KIND_COLUMNS
            .iter()
            .find(|(name, _)| *name == kind)
            .map(|(_, columns)| *columns)
            .ok_or_else(|| format!("row {row} declares unknown metric_type {kind}"))?;
        for column in KIND_SPECIFIC_COLUMNS {
            if permitted.contains(&column) {
                continue;
            }
            let values = batch
                .column_by_name(column)
                .ok_or_else(|| format!("canonical batch lacks column {column}"))?;
            if values.is_valid(row) {
                return Err(format!("row {row} of kind {kind} populates {column}"));
            }
        }
        if matches!(kind, METRIC_TYPE_GAUGE | METRIC_TYPE_SUM) {
            let ints = batch
                .column_by_name("int_value")
                .ok_or_else(|| "canonical batch lacks int_value".to_owned())?;
            let doubles = batch
                .column_by_name("double_value")
                .ok_or_else(|| "canonical batch lacks double_value".to_owned())?;
            if ints.is_valid(row) == doubles.is_valid(row) {
                return Err(format!(
                    "row {row} of kind {kind} must carry exactly one numeric alternative"
                ));
            }
        }
    }
    Ok(batch)
}

/// Count the data points a metric declares, whatever its kind.
fn point_count(metric: &Metric) -> usize {
    match &metric.data {
        Some(metric::Data::Gauge(gauge)) => gauge.data_points.len(),
        Some(metric::Data::Sum(sum)) => sum.data_points.len(),
        Some(metric::Data::Histogram(histogram)) => histogram.data_points.len(),
        Some(metric::Data::ExponentialHistogram(histogram)) => histogram.data_points.len(),
        Some(metric::Data::Summary(summary)) => summary.data_points.len(),
        None => 1,
    }
}

/// The per-metric descriptor every one of its points repeats.
#[derive(Debug)]
struct MetricDescriptor {
    /// Metric name.
    name: String,
    /// Metric description.
    description: String,
    /// Metric unit.
    unit: String,
    /// Canonical encoding of the metric-level metadata attributes.
    metadata: Vec<u8>,
    /// Canonical `metric_type` discriminant.
    kind: &'static str,
    /// Raw aggregation temporality, when the kind owns one.
    temporality: Option<i32>,
    /// Monotonicity, when the kind owns one.
    monotonic: Option<bool>,
}

impl MetricDescriptor {
    /// Validate one metric's descriptor fields and classify its kind.
    ///
    /// # Errors
    ///
    /// Returns a stable reason when the metric carries no data oneof or when a
    /// descriptor string exceeds its accepted length.
    fn project(metric: &Metric) -> Result<Self, &'static str> {
        if metric.name.is_empty() || metric.name.len() > MAX_METRIC_NAME_BYTES {
            return Err("metric name is empty or exceeds the accepted length");
        }
        if metric.description.len() > MAX_DESCRIPTION_BYTES {
            return Err("metric description exceeds the accepted length");
        }
        if metric.unit.len() > MAX_UNIT_BYTES {
            return Err("metric unit exceeds the accepted length");
        }
        let (kind, temporality, monotonic) = match &metric.data {
            Some(metric::Data::Gauge(_)) => (METRIC_TYPE_GAUGE, None, None),
            Some(metric::Data::Sum(sum)) => (
                METRIC_TYPE_SUM,
                Some(sum.aggregation_temporality),
                Some(sum.is_monotonic),
            ),
            Some(metric::Data::Histogram(histogram)) => (
                METRIC_TYPE_HISTOGRAM,
                Some(histogram.aggregation_temporality),
                None,
            ),
            Some(metric::Data::ExponentialHistogram(histogram)) => (
                METRIC_TYPE_EXPONENTIAL_HISTOGRAM,
                Some(histogram.aggregation_temporality),
                None,
            ),
            Some(metric::Data::Summary(_)) => (METRIC_TYPE_SUMMARY, None, None),
            None => return Err("metric carries no data point collection"),
        };
        Ok(Self {
            name: metric.name.clone(),
            description: metric.description.clone(),
            unit: metric.unit.clone(),
            metadata: encode_attributes(&metric.metadata),
            kind,
            temporality,
            monotonic,
        })
    }
}

/// Validate every data point of one metric, in request order.
fn point_rows(metric: &Metric) -> Vec<Result<PointRow, &'static str>> {
    match &metric.data {
        Some(metric::Data::Gauge(gauge)) => {
            gauge.data_points.iter().map(PointRow::number).collect()
        }
        Some(metric::Data::Sum(sum)) => sum.data_points.iter().map(PointRow::number).collect(),
        Some(metric::Data::Histogram(histogram)) => histogram
            .data_points
            .iter()
            .map(PointRow::histogram)
            .collect(),
        Some(metric::Data::ExponentialHistogram(histogram)) => histogram
            .data_points
            .iter()
            .map(PointRow::exponential_histogram)
            .collect(),
        Some(metric::Data::Summary(summary)) => {
            summary.data_points.iter().map(PointRow::summary).collect()
        }
        None => Vec::new(),
    }
}

/// One validated exponential bucket collection.
#[derive(Debug)]
struct BucketRow {
    /// Signed index of the first populated bucket.
    offset: i32,
    /// Ordered bucket counts.
    counts: Vec<u64>,
}

/// One validated exemplar.
#[derive(Debug)]
struct ExemplarRow {
    /// Exemplar observation time.
    time_unix_nano: u64,
    /// Integer alternative, when the exemplar carries one.
    int_value: Option<i64>,
    /// Double alternative, when the exemplar carries one.
    double_value: Option<f64>,
    /// Canonical encoding of the filtered attributes.
    filtered_attributes: Vec<u8>,
    /// Correlated trace id, when present.
    trace_id: Option<Vec<u8>>,
    /// Correlated span id, when present.
    span_id: Option<Vec<u8>>,
}

/// One validated data point, independent of which kind produced it.
#[derive(Debug, Default)]
struct PointRow {
    /// Point observation time.
    time_unix_nano: u64,
    /// Start of the point's aggregation window.
    start_time_unix_nano: u64,
    /// Raw data-point flag word.
    flags: u32,
    /// Canonical encoding of the point attributes.
    attributes: Vec<u8>,
    /// Integer numeric alternative.
    int_value: Option<i64>,
    /// Double numeric alternative.
    double_value: Option<f64>,
    /// Histogram or exponential-histogram total count.
    histogram_count: Option<u64>,
    /// Histogram or exponential-histogram sum.
    histogram_sum: Option<f64>,
    /// Histogram or exponential-histogram minimum.
    histogram_min: Option<f64>,
    /// Histogram or exponential-histogram maximum.
    histogram_max: Option<f64>,
    /// Explicit bucket counts.
    bucket_counts: Option<Vec<u64>>,
    /// Explicit bucket bounds.
    explicit_bounds: Option<Vec<f64>>,
    /// Exponential resolution scale.
    exponential_scale: Option<i32>,
    /// Exponential zero-bucket population.
    exponential_zero_count: Option<u64>,
    /// Exponential zero-bucket width.
    exponential_zero_threshold: Option<f64>,
    /// Positive exponential buckets.
    positive_buckets: Option<BucketRow>,
    /// Negative exponential buckets.
    negative_buckets: Option<BucketRow>,
    /// Summary total count.
    summary_count: Option<u64>,
    /// Summary sum.
    summary_sum: Option<f64>,
    /// Summary quantiles, in request order.
    quantile_values: Option<Vec<(f64, f64)>>,
    /// Exemplars, in request order.
    exemplars: Vec<ExemplarRow>,
}

impl PointRow {
    /// Validate one gauge or sum data point.
    ///
    /// # Errors
    ///
    /// Returns a stable reason when the numeric oneof is absent or an exemplar
    /// is invalid.
    fn number(point: &NumberDataPoint) -> Result<Self, &'static str> {
        let (int_value, double_value) = match point.value {
            Some(number_data_point::Value::AsInt(value)) => (Some(value), None),
            Some(number_data_point::Value::AsDouble(value)) => (None, Some(value)),
            None => return Err("numeric data point carries no value"),
        };
        Ok(Self {
            time_unix_nano: point.time_unix_nano,
            start_time_unix_nano: point.start_time_unix_nano,
            flags: point.flags,
            attributes: encode_attributes(&point.attributes),
            int_value,
            double_value,
            exemplars: exemplar_rows(&point.exemplars)?,
            ..Self::default()
        })
    }

    /// Validate one explicit-bucket histogram data point.
    ///
    /// # Errors
    ///
    /// Returns a stable reason when the bucket and bound counts disagree, the
    /// bucket counts do not sum to the declared count, a bound is not finite or
    /// not strictly increasing, the collection exceeds its accepted size, or an
    /// exemplar is invalid.
    fn histogram(point: &HistogramDataPoint) -> Result<Self, &'static str> {
        if point.bucket_counts.len() > MAX_BUCKETS {
            return Err("histogram bucket collection exceeds the accepted size");
        }
        if !point.bucket_counts.is_empty()
            && point.bucket_counts.len() != point.explicit_bounds.len() + 1
        {
            return Err("histogram bucket count does not match its explicit bounds");
        }
        if point
            .explicit_bounds
            .windows(2)
            .any(|pair| pair[0].partial_cmp(&pair[1]) != Some(Ordering::Less))
        {
            return Err("histogram explicit bounds are not strictly increasing");
        }
        let observed: u64 = point
            .bucket_counts
            .iter()
            .try_fold(0u64, |total, count| total.checked_add(*count))
            .ok_or("histogram bucket counts overflow their total")?;
        if !point.bucket_counts.is_empty() && observed != point.count {
            return Err("histogram bucket counts do not sum to the declared count");
        }
        Ok(Self {
            time_unix_nano: point.time_unix_nano,
            start_time_unix_nano: point.start_time_unix_nano,
            flags: point.flags,
            attributes: encode_attributes(&point.attributes),
            histogram_count: Some(point.count),
            histogram_sum: point.sum,
            histogram_min: point.min,
            histogram_max: point.max,
            bucket_counts: Some(point.bucket_counts.clone()),
            explicit_bounds: Some(point.explicit_bounds.clone()),
            exemplars: exemplar_rows(&point.exemplars)?,
            ..Self::default()
        })
    }

    /// Validate one exponential histogram data point.
    ///
    /// # Errors
    ///
    /// Returns a stable reason when a bucket collection exceeds its accepted
    /// size, the zero threshold is negative, or an exemplar is invalid.
    fn exponential_histogram(point: &ExponentialHistogramDataPoint) -> Result<Self, &'static str> {
        if point.zero_threshold < 0.0 {
            return Err("exponential histogram zero threshold is negative");
        }
        let positive = point.positive.as_ref().map(bucket_row).transpose()?;
        let negative = point.negative.as_ref().map(bucket_row).transpose()?;
        Ok(Self {
            time_unix_nano: point.time_unix_nano,
            start_time_unix_nano: point.start_time_unix_nano,
            flags: point.flags,
            attributes: encode_attributes(&point.attributes),
            histogram_count: Some(point.count),
            histogram_sum: point.sum,
            histogram_min: point.min,
            histogram_max: point.max,
            exponential_scale: Some(point.scale),
            exponential_zero_count: Some(point.zero_count),
            exponential_zero_threshold: Some(point.zero_threshold),
            positive_buckets: positive,
            negative_buckets: negative,
            exemplars: exemplar_rows(&point.exemplars)?,
            ..Self::default()
        })
    }

    /// Validate one summary data point.
    ///
    /// # Errors
    ///
    /// Returns a stable reason when the quantile collection exceeds its
    /// accepted size or a quantile lies outside the closed unit interval.
    fn summary(point: &SummaryDataPoint) -> Result<Self, &'static str> {
        if point.quantile_values.len() > MAX_QUANTILES {
            return Err("summary quantile collection exceeds the accepted size");
        }
        let mut quantiles = Vec::with_capacity(point.quantile_values.len());
        for entry in &point.quantile_values {
            if !(0.0..=1.0).contains(&entry.quantile) {
                return Err("summary quantile lies outside the closed unit interval");
            }
            quantiles.push((entry.quantile, entry.value));
        }
        Ok(Self {
            time_unix_nano: point.time_unix_nano,
            start_time_unix_nano: point.start_time_unix_nano,
            flags: point.flags,
            attributes: encode_attributes(&point.attributes),
            summary_count: Some(point.count),
            summary_sum: Some(point.sum),
            quantile_values: Some(quantiles),
            ..Self::default()
        })
    }
}

/// Validate one exponential bucket collection.
///
/// # Errors
///
/// Returns a stable reason when the collection exceeds its accepted size.
fn bucket_row(
    buckets: &wyrd_tonic::otlp::metrics::v1::exponential_histogram_data_point::Buckets,
) -> Result<BucketRow, &'static str> {
    if buckets.bucket_counts.len() > MAX_BUCKETS {
        return Err("exponential bucket collection exceeds the accepted size");
    }
    Ok(BucketRow {
        offset: buckets.offset,
        counts: buckets.bucket_counts.clone(),
    })
}

/// Validate every exemplar attached to one data point, in request order.
///
/// # Errors
///
/// Returns a stable reason when the collection exceeds its accepted size, an
/// exemplar carries no value, or an exemplar's correlation identifier has the
/// wrong width.
fn exemplar_rows(exemplars: &[Exemplar]) -> Result<Vec<ExemplarRow>, &'static str> {
    if exemplars.len() > MAX_EXEMPLARS {
        return Err("exemplar collection exceeds the accepted size");
    }
    exemplars
        .iter()
        .map(|value| {
            let (int_value, double_value) = match value.value {
                Some(exemplar::Value::AsInt(int)) => (Some(int), None),
                Some(exemplar::Value::AsDouble(double)) => (None, Some(double)),
                None => return Err("exemplar carries no value"),
            };
            let trace_id = if value.trace_id.is_empty() {
                None
            } else {
                Some(trace_id_bytes(&value.trace_id)?.to_vec())
            };
            let span_id = if value.span_id.is_empty() {
                None
            } else {
                Some(span_id_bytes(&value.span_id)?.to_vec())
            };
            Ok(ExemplarRow {
                time_unix_nano: value.time_unix_nano,
                int_value,
                double_value,
                filtered_attributes: encode_attributes(&value.filtered_attributes),
                trace_id,
                span_id,
            })
        })
        .collect()
}

/// Column-wise accumulator for the accepted canonical point rows.
#[derive(Debug, Default)]
struct PointColumns {
    rows: usize,
    metric_name: Vec<String>,
    description: Vec<String>,
    unit: Vec<String>,
    metadata: Vec<Vec<u8>>,
    metric_type: Vec<String>,
    time_unix_nano: Vec<u64>,
    start_time_unix_nano: Vec<u64>,
    flags: Vec<u32>,
    attributes: Vec<Vec<u8>>,
    int_value: Vec<Option<i64>>,
    double_value: Vec<Option<f64>>,
    aggregation_temporality: Vec<Option<i32>>,
    is_monotonic: Vec<Option<bool>>,
    histogram_count: Vec<Option<u64>>,
    histogram_sum: Vec<Option<f64>>,
    histogram_min: Vec<Option<f64>>,
    histogram_max: Vec<Option<f64>>,
    bucket_count_lengths: Vec<Option<usize>>,
    bucket_counts: Vec<u64>,
    explicit_bound_lengths: Vec<Option<usize>>,
    explicit_bounds: Vec<f64>,
    exponential_scale: Vec<Option<i32>>,
    exponential_zero_count: Vec<Option<u64>>,
    exponential_zero_threshold: Vec<Option<f64>>,
    positive_valid: Vec<bool>,
    positive_offset: Vec<i32>,
    positive_count_lengths: Vec<Option<usize>>,
    positive_counts: Vec<u64>,
    negative_valid: Vec<bool>,
    negative_offset: Vec<i32>,
    negative_count_lengths: Vec<Option<usize>>,
    negative_counts: Vec<u64>,
    summary_count: Vec<Option<u64>>,
    summary_sum: Vec<Option<f64>>,
    quantile_lengths: Vec<Option<usize>>,
    quantile_quantiles: Vec<f64>,
    quantile_measured: Vec<f64>,
    exemplar_lengths: Vec<Option<usize>>,
    exemplar_time: Vec<u64>,
    exemplar_int: Vec<Option<i64>>,
    exemplar_double: Vec<Option<f64>>,
    exemplar_attributes: Vec<Vec<u8>>,
    exemplar_trace_id: Vec<Option<Vec<u8>>>,
    exemplar_span_id: Vec<Option<Vec<u8>>>,
    resource_present: Vec<bool>,
    resource_attributes: Vec<Vec<u8>>,
    resource_dropped_attributes_count: Vec<u32>,
    resource_schema_url: Vec<String>,
    entity_ref_lengths: Vec<Option<usize>>,
    entity_refs: Vec<Vec<u8>>,
    scope_present: Vec<bool>,
    scope_name: Vec<String>,
    scope_version: Vec<String>,
    scope_attributes: Vec<Vec<u8>>,
    scope_dropped_attributes_count: Vec<u32>,
    scope_schema_url: Vec<String>,
}

impl PointColumns {
    /// Append one already-validated point as a canonical row.
    ///
    /// Appending is infallible: every rejection is decided before this point,
    /// so no rejected point can leave a partial row or a partial nested
    /// collection behind.
    fn push(
        &mut self,
        descriptor: &MetricDescriptor,
        row: PointRow,
        resource: &ResourceEnvelope,
        scope: &ScopeEnvelope,
    ) {
        self.metric_name.push(descriptor.name.clone());
        self.description.push(descriptor.description.clone());
        self.unit.push(descriptor.unit.clone());
        self.metadata.push(descriptor.metadata.clone());
        self.metric_type.push(descriptor.kind.to_owned());
        self.aggregation_temporality.push(descriptor.temporality);
        self.is_monotonic.push(descriptor.monotonic);

        self.time_unix_nano.push(row.time_unix_nano);
        self.start_time_unix_nano.push(row.start_time_unix_nano);
        self.flags.push(row.flags);
        self.attributes.push(row.attributes);
        self.int_value.push(row.int_value);
        self.double_value.push(row.double_value);
        self.histogram_count.push(row.histogram_count);
        self.histogram_sum.push(row.histogram_sum);
        self.histogram_min.push(row.histogram_min);
        self.histogram_max.push(row.histogram_max);
        self.bucket_count_lengths
            .push(row.bucket_counts.as_ref().map(Vec::len));
        if let Some(counts) = row.bucket_counts {
            self.bucket_counts.extend(counts);
        }
        self.explicit_bound_lengths
            .push(row.explicit_bounds.as_ref().map(Vec::len));
        if let Some(bounds) = row.explicit_bounds {
            self.explicit_bounds.extend(bounds);
        }
        self.exponential_scale.push(row.exponential_scale);
        self.exponential_zero_count.push(row.exponential_zero_count);
        self.exponential_zero_threshold
            .push(row.exponential_zero_threshold);
        push_buckets(
            row.positive_buckets,
            &mut self.positive_valid,
            &mut self.positive_offset,
            &mut self.positive_count_lengths,
            &mut self.positive_counts,
        );
        push_buckets(
            row.negative_buckets,
            &mut self.negative_valid,
            &mut self.negative_offset,
            &mut self.negative_count_lengths,
            &mut self.negative_counts,
        );
        self.summary_count.push(row.summary_count);
        self.summary_sum.push(row.summary_sum);
        self.quantile_lengths
            .push(row.quantile_values.as_ref().map(Vec::len));
        if let Some(quantiles) = row.quantile_values {
            for (quantile, value) in quantiles {
                self.quantile_quantiles.push(quantile);
                self.quantile_measured.push(value);
            }
        }
        self.exemplar_lengths.push(Some(row.exemplars.len()));
        for exemplar in row.exemplars {
            self.exemplar_time.push(exemplar.time_unix_nano);
            self.exemplar_int.push(exemplar.int_value);
            self.exemplar_double.push(exemplar.double_value);
            self.exemplar_attributes.push(exemplar.filtered_attributes);
            self.exemplar_trace_id.push(exemplar.trace_id);
            self.exemplar_span_id.push(exemplar.span_id);
        }

        self.resource_present.push(resource.present);
        self.resource_attributes.push(resource.attributes.clone());
        self.resource_dropped_attributes_count
            .push(resource.dropped_attributes_count);
        self.resource_schema_url.push(resource.schema_url.clone());
        self.entity_ref_lengths
            .push(Some(resource.entity_refs.len()));
        self.entity_refs
            .extend(resource.entity_refs.iter().cloned());

        self.scope_present.push(scope.present);
        self.scope_name.push(scope.name.clone());
        self.scope_version.push(scope.version.clone());
        self.scope_attributes.push(scope.attributes.clone());
        self.scope_dropped_attributes_count
            .push(scope.dropped_attributes_count);
        self.scope_schema_url.push(scope.schema_url.clone());

        self.rows += 1;
    }

    /// Assemble the accepted rows into the canonical point batch.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::Internal`] when a column cannot be built or the
    /// assembled columns do not match the canonical schema.
    /// Assemble the nested bucket, quantile, and exemplar element columns.
    ///
    /// These are built before the row columns so `finish` stays one flat
    /// assembly of the ledger's column order.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::Internal`] when a nested column does not match its
    /// declaration.
    fn nested_columns(&mut self) -> Result<NestedColumns, TableError> {
        let positive = bucket_struct(
            &POSITIVE_BUCKET_FIELDS,
            &POSITIVE_BUCKET_COUNT_ELEMENT,
            std::mem::take(&mut self.positive_valid),
            std::mem::take(&mut self.positive_offset),
            &self.positive_count_lengths,
            std::mem::take(&mut self.positive_counts),
        )?;
        let negative = bucket_struct(
            &NEGATIVE_BUCKET_FIELDS,
            &NEGATIVE_BUCKET_COUNT_ELEMENT,
            std::mem::take(&mut self.negative_valid),
            std::mem::take(&mut self.negative_offset),
            &self.negative_count_lengths,
            std::mem::take(&mut self.negative_counts),
        )?;
        let quantiles = struct_column(
            &nested_fields(&QUANTILE_VALUE_ELEMENT.to_arrow())?,
            vec![
                f64_column(std::mem::take(&mut self.quantile_quantiles)),
                f64_column(std::mem::take(&mut self.quantile_measured)),
            ],
            None,
        )
        .map_err(internal)?;
        let exemplars = struct_column(
            &nested_fields(&EXEMPLAR_ELEMENT.to_arrow())?,
            vec![
                u64_column(std::mem::take(&mut self.exemplar_time)),
                i64_opt_column(std::mem::take(&mut self.exemplar_int)),
                f64_opt_column(std::mem::take(&mut self.exemplar_double)),
                binary_column(&self.exemplar_attributes),
                fixed_binary_opt_column(16, &self.exemplar_trace_id).map_err(internal)?,
                fixed_binary_opt_column(8, &self.exemplar_span_id).map_err(internal)?,
            ],
            None,
        )
        .map_err(internal)?;

        Ok(NestedColumns {
            positive,
            negative,
            quantiles,
            exemplars,
        })
    }

    fn finish(mut self) -> Result<RecordBatch, TableError> {
        let nested = self.nested_columns()?;
        let columns: Vec<ArrayRef> = vec![
            utf8_column(self.metric_name),
            utf8_column(self.description),
            utf8_column(self.unit),
            binary_column(&self.metadata),
            utf8_column(self.metric_type),
            u64_column(self.time_unix_nano),
            u64_column(self.start_time_unix_nano),
            u32_column(self.flags),
            binary_column(&self.attributes),
            i64_opt_column(self.int_value),
            f64_opt_column(self.double_value),
            i32_opt_column(self.aggregation_temporality),
            bool_opt_column(self.is_monotonic),
            u64_opt_column(self.histogram_count),
            f64_opt_column(self.histogram_sum),
            f64_opt_column(self.histogram_min),
            f64_opt_column(self.histogram_max),
            list_column(
                &BUCKET_COUNT_ELEMENT.to_arrow(),
                u64_column(self.bucket_counts),
                &self.bucket_count_lengths,
            )
            .map_err(internal)?,
            list_column(
                &EXPLICIT_BOUND_ELEMENT.to_arrow(),
                f64_column(self.explicit_bounds),
                &self.explicit_bound_lengths,
            )
            .map_err(internal)?,
            i32_opt_column(self.exponential_scale),
            u64_opt_column(self.exponential_zero_count),
            f64_opt_column(self.exponential_zero_threshold),
            nested.positive,
            nested.negative,
            u64_opt_column(self.summary_count),
            f64_opt_column(self.summary_sum),
            list_column(
                &QUANTILE_VALUE_ELEMENT.to_arrow(),
                nested.quantiles,
                &self.quantile_lengths,
            )
            .map_err(internal)?,
            list_column(
                &EXEMPLAR_ELEMENT.to_arrow(),
                nested.exemplars,
                &self.exemplar_lengths,
            )
            .map_err(internal)?,
            bool_column(self.resource_present),
            binary_column(&self.resource_attributes),
            u32_column(self.resource_dropped_attributes_count),
            utf8_column(self.resource_schema_url),
            list_column(
                &METRIC_ENTITY_REF_ELEMENT.to_arrow(),
                binary_column(&self.entity_refs),
                &self.entity_ref_lengths,
            )
            .map_err(internal)?,
            bool_column(self.scope_present),
            utf8_column(self.scope_name),
            utf8_column(self.scope_version),
            binary_column(&self.scope_attributes),
            u32_column(self.scope_dropped_attributes_count),
            utf8_column(self.scope_schema_url),
        ];
        RecordBatch::try_new(canonical_metric_schema(), columns)
            .map_err(|error| TableError::Internal(format!("canonical metric batch: {error}")))
    }
}

/// The nested element columns one canonical point batch is assembled from.
#[derive(Debug)]
struct NestedColumns {
    /// Positive exponential bucket struct column.
    positive: ArrayRef,
    /// Negative exponential bucket struct column.
    negative: ArrayRef,
    /// Summary quantile struct column.
    quantiles: ArrayRef,
    /// Exemplar struct column.
    exemplars: ArrayRef,
}

/// Stage one optional exponential bucket collection into its flat storage.
///
/// An absent collection still contributes a placeholder offset and an empty
/// count run so the struct column stays row-aligned; its validity bit, not its
/// children, expresses the absence.
fn push_buckets(
    buckets: Option<BucketRow>,
    valid: &mut Vec<bool>,
    offsets: &mut Vec<i32>,
    lengths: &mut Vec<Option<usize>>,
    counts: &mut Vec<u64>,
) {
    if let Some(row) = buckets {
        valid.push(true);
        offsets.push(row.offset);
        lengths.push(Some(row.counts.len()));
        counts.extend(row.counts);
    } else {
        valid.push(false);
        offsets.push(0);
        lengths.push(Some(0));
    }
}

/// Assemble one nullable exponential bucket struct column.
///
/// # Errors
///
/// Returns [`TableError::Internal`] when the nested count list or the struct
/// itself does not match its declaration.
fn bucket_struct(
    declared: &[crate::tables::fields::CanonicalField],
    element: &crate::tables::fields::CanonicalField,
    valid: Vec<bool>,
    offsets: Vec<i32>,
    lengths: &[Option<usize>],
    counts: Vec<u64>,
) -> Result<ArrayRef, TableError> {
    let counts = list_column(&element.to_arrow(), u64_column(counts), lengths).map_err(internal)?;
    let children = Fields::from(canonical_arrow_fields(declared));
    struct_column(&children, vec![i32_column(offsets), counts], Some(valid)).map_err(internal)
}
