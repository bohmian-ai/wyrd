//! Direct exact-capacity OTLP metrics JSON construction.

use std::mem::size_of;

use vala_bifrost_redux::gate::IngestError;
use wyrd_tonic::otlp::metrics::v1::{
    Exemplar, ExponentialHistogram, ExponentialHistogramDataPoint, Gauge, Histogram,
    HistogramDataPoint, Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics, Sum, Summary,
    SummaryDataPoint, exemplar, exponential_histogram_data_point, metric, number_data_point,
    summary_data_point,
};
use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;

use crate::otlp_json::{
    Field, JsonCursor, JsonDecodeError, attributes_capacity, decode_f64_array, decode_i32,
    decode_key_value, decode_message_array, decode_optional, decode_resource, decode_scope,
    decode_u32, decode_u64_array, json_ingest_error, mark_seen, next_object_field,
    resource_capacity, scope_capacity,
};

/// Constructs a metrics export directly from preflighted JSON.
///
/// # Errors
///
/// Returns a stable decode error for malformed or invalid protobuf-JSON input,
/// trailing input, or a preflight-versus-materialized retained-capacity
/// mismatch.
pub(crate) fn decode_metrics_json(
    input: &[u8],
    expected_decode_bytes: usize,
) -> Result<ExportMetricsServiceRequest, IngestError> {
    let mut cursor = JsonCursor::new(input);
    let request = decode_request(&mut cursor).map_err(json_ingest_error)?;
    cursor.finish().map_err(json_ingest_error)?;
    if metrics_capacity(&request) != expected_decode_bytes {
        return Err(IngestError::Decode(
            "OTLP metrics JSON preflight/materialized capacity mismatch".to_owned(),
        ));
    }
    Ok(request)
}

/// Computes exact recursively retained metrics request capacity.
fn metrics_capacity(request: &ExportMetricsServiceRequest) -> usize {
    size_of::<ExportMetricsServiceRequest>()
        + request.resource_metrics.capacity() * size_of::<ResourceMetrics>()
        + request
            .resource_metrics
            .iter()
            .map(resource_metrics_capacity)
            .sum::<usize>()
}

/// Computes backing beneath one resource-metrics group.
fn resource_metrics_capacity(group: &ResourceMetrics) -> usize {
    group.schema_url.capacity()
        + group.scope_metrics.capacity() * size_of::<ScopeMetrics>()
        + group.resource.as_ref().map_or(0, resource_capacity)
        + group
            .scope_metrics
            .iter()
            .map(scope_metrics_capacity)
            .sum::<usize>()
}

/// Computes backing beneath one scope-metrics group.
fn scope_metrics_capacity(group: &ScopeMetrics) -> usize {
    group.schema_url.capacity()
        + group.metrics.capacity() * size_of::<Metric>()
        + group.scope.as_ref().map_or(0, scope_capacity)
        + group.metrics.iter().map(metric_capacity).sum::<usize>()
}

/// Computes backing beneath one metric and its selected aggregation.
fn metric_capacity(value: &Metric) -> usize {
    value.name.capacity()
        + value.description.capacity()
        + value.unit.capacity()
        + attributes_capacity(&value.metadata)
        + match value.data.as_ref() {
            Some(metric::Data::Gauge(data)) => number_points_capacity(&data.data_points),
            Some(metric::Data::Sum(data)) => number_points_capacity(&data.data_points),
            Some(metric::Data::Histogram(data)) => {
                data.data_points.capacity() * size_of::<HistogramDataPoint>()
                    + data
                        .data_points
                        .iter()
                        .map(histogram_point_capacity)
                        .sum::<usize>()
            }
            Some(metric::Data::ExponentialHistogram(data)) => {
                data.data_points.capacity() * size_of::<ExponentialHistogramDataPoint>()
                    + data
                        .data_points
                        .iter()
                        .map(exponential_point_capacity)
                        .sum::<usize>()
            }
            Some(metric::Data::Summary(data)) => {
                data.data_points.capacity() * size_of::<SummaryDataPoint>()
                    + data
                        .data_points
                        .iter()
                        .map(summary_point_capacity)
                        .sum::<usize>()
            }
            None => 0,
        }
}

/// Computes retained backing for scalar metric points.
fn number_points_capacity(values: &Vec<NumberDataPoint>) -> usize {
    values.capacity() * size_of::<NumberDataPoint>()
        + values
            .iter()
            .map(|value| {
                attributes_capacity(&value.attributes) + exemplars_capacity(&value.exemplars)
            })
            .sum::<usize>()
}

/// Computes retained backing for one histogram point.
fn histogram_point_capacity(value: &HistogramDataPoint) -> usize {
    attributes_capacity(&value.attributes)
        + exemplars_capacity(&value.exemplars)
        + value.bucket_counts.capacity() * size_of::<u64>()
        + value.explicit_bounds.capacity() * size_of::<f64>()
}

/// Computes retained backing for one exponential histogram point.
fn exponential_point_capacity(value: &ExponentialHistogramDataPoint) -> usize {
    attributes_capacity(&value.attributes)
        + exemplars_capacity(&value.exemplars)
        + value
            .positive
            .as_ref()
            .map_or(0, |b| b.bucket_counts.capacity() * size_of::<u64>())
        + value
            .negative
            .as_ref()
            .map_or(0, |b| b.bucket_counts.capacity() * size_of::<u64>())
}

/// Computes retained backing for one summary point.
fn summary_point_capacity(value: &SummaryDataPoint) -> usize {
    attributes_capacity(&value.attributes)
        + value.quantile_values.capacity() * size_of::<summary_data_point::ValueAtQuantile>()
}

/// Computes retained backing for exemplar vectors.
fn exemplars_capacity(values: &Vec<Exemplar>) -> usize {
    values.capacity() * size_of::<Exemplar>()
        + values
            .iter()
            .map(|value| {
                attributes_capacity(&value.filtered_attributes)
                    + value.span_id.capacity()
                    + value.trace_id.capacity()
            })
            .sum::<usize>()
}

/// Decodes the metrics request root.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate
/// `resourceMetrics`, or an invalid nested resource-metrics array.
fn decode_request(
    cursor: &mut JsonCursor<'_>,
) -> Result<ExportMetricsServiceRequest, JsonDecodeError> {
    let mut output = ExportMetricsServiceRequest::default();
    object(cursor, |field, cursor| match field {
        Field::ResourceMetrics => {
            output.resource_metrics = decode_message_array(cursor, decode_resource_metrics)?;
            Ok(true)
        }
        _ => Ok(false),
    })?;
    Ok(output)
}

/// Decodes one metrics resource group.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate known
/// fields, invalid resource or scope-metrics messages, or malformed strings.
fn decode_resource_metrics(
    cursor: &mut JsonCursor<'_>,
) -> Result<ResourceMetrics, JsonDecodeError> {
    let mut output = ResourceMetrics::default();
    object(cursor, |field, cursor| match field {
        Field::Resource => {
            output.resource = decode_optional(cursor, decode_resource)?;
            Ok(true)
        }
        Field::ScopeMetrics => {
            output.scope_metrics = decode_message_array(cursor, decode_scope_metrics)?;
            Ok(true)
        }
        Field::SchemaUrl => {
            output.schema_url = cursor.owned_string()?;
            Ok(true)
        }
        _ => Ok(false),
    })?;
    Ok(output)
}

/// Decodes one metrics scope group.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate known
/// fields, invalid scope or metric messages, or malformed strings.
fn decode_scope_metrics(cursor: &mut JsonCursor<'_>) -> Result<ScopeMetrics, JsonDecodeError> {
    let mut output = ScopeMetrics::default();
    object(cursor, |field, cursor| match field {
        Field::Scope => {
            output.scope = decode_optional(cursor, decode_scope)?;
            Ok(true)
        }
        Field::Metrics => {
            output.metrics = decode_message_array(cursor, decode_metric)?;
            Ok(true)
        }
        Field::SchemaUrl => {
            output.schema_url = cursor.owned_string()?;
            Ok(true)
        }
        _ => Ok(false),
    })?;
    Ok(output)
}

/// Decodes one metric descriptor and its flattened data oneof.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate known
/// fields, malformed descriptor or metadata values, invalid aggregation
/// content, or more than one metric data variant.
fn decode_metric(cursor: &mut JsonCursor<'_>) -> Result<Metric, JsonDecodeError> {
    let mut output = Metric::default();
    object(cursor, |field, cursor| match field {
        Field::Name => {
            output.name = cursor.owned_string()?;
            Ok(true)
        }
        Field::Description => {
            output.description = cursor.owned_string()?;
            Ok(true)
        }
        Field::Unit => {
            output.unit = cursor.owned_string()?;
            Ok(true)
        }
        Field::Metadata => {
            output.metadata = decode_message_array(cursor, decode_key_value)?;
            Ok(true)
        }
        Field::Gauge => {
            set_metric_data(&mut output, metric::Data::Gauge(decode_gauge(cursor)?))?;
            Ok(true)
        }
        Field::Sum => {
            set_metric_data(&mut output, metric::Data::Sum(decode_sum(cursor)?))?;
            Ok(true)
        }
        Field::Histogram => {
            set_metric_data(
                &mut output,
                metric::Data::Histogram(decode_histogram(cursor)?),
            )?;
            Ok(true)
        }
        Field::ExponentialHistogram => {
            set_metric_data(
                &mut output,
                metric::Data::ExponentialHistogram(decode_exponential_histogram(cursor)?),
            )?;
            Ok(true)
        }
        Field::Summary => {
            set_metric_data(&mut output, metric::Data::Summary(decode_summary(cursor)?))?;
            Ok(true)
        }
        _ => Ok(false),
    })?;
    Ok(output)
}

/// Sets the metric oneof once, matching derived-serde duplicate behavior.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] when the metric already contains an aggregation
/// variant.
fn set_metric_data(output: &mut Metric, value: metric::Data) -> Result<(), JsonDecodeError> {
    if output.data.is_some() {
        return Err(JsonDecodeError::at(0, "duplicate metric data field"));
    }
    output.data = Some(value);
    Ok(())
}

/// Decodes a gauge aggregation.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, a duplicate
/// `dataPoints` field, or an invalid number-point array.
fn decode_gauge(cursor: &mut JsonCursor<'_>) -> Result<Gauge, JsonDecodeError> {
    let mut output = Gauge::default();
    object(cursor, |field, cursor| match field {
        Field::DataPoints => {
            output.data_points = decode_message_array(cursor, decode_number_point)?;
            Ok(true)
        }
        _ => Ok(false),
    })?;
    Ok(output)
}

/// Decodes a sum aggregation.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate fields,
/// invalid number points, an out-of-range temporality, or a non-boolean
/// monotonicity value.
fn decode_sum(cursor: &mut JsonCursor<'_>) -> Result<Sum, JsonDecodeError> {
    let mut output = Sum::default();
    object(cursor, |field, cursor| match field {
        Field::DataPoints => {
            output.data_points = decode_message_array(cursor, decode_number_point)?;
            Ok(true)
        }
        Field::AggregationTemporality => {
            output.aggregation_temporality = decode_i32(cursor)?;
            Ok(true)
        }
        Field::IsMonotonic => {
            output.is_monotonic = cursor.bool()?;
            Ok(true)
        }
        _ => Ok(false),
    })?;
    Ok(output)
}

/// Decodes a histogram aggregation.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate fields,
/// invalid histogram points, or an out-of-range temporality.
fn decode_histogram(cursor: &mut JsonCursor<'_>) -> Result<Histogram, JsonDecodeError> {
    let mut output = Histogram::default();
    object(cursor, |field, cursor| match field {
        Field::DataPoints => {
            output.data_points = decode_message_array(cursor, decode_histogram_point)?;
            Ok(true)
        }
        Field::AggregationTemporality => {
            output.aggregation_temporality = decode_i32(cursor)?;
            Ok(true)
        }
        _ => Ok(false),
    })?;
    Ok(output)
}

/// Decodes an exponential-histogram aggregation.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate fields,
/// invalid exponential-histogram points, or an out-of-range temporality.
fn decode_exponential_histogram(
    cursor: &mut JsonCursor<'_>,
) -> Result<ExponentialHistogram, JsonDecodeError> {
    let mut output = ExponentialHistogram::default();
    object(cursor, |field, cursor| match field {
        Field::DataPoints => {
            output.data_points = decode_message_array(cursor, decode_exponential_histogram_point)?;
            Ok(true)
        }
        Field::AggregationTemporality => {
            output.aggregation_temporality = decode_i32(cursor)?;
            Ok(true)
        }
        _ => Ok(false),
    })?;
    Ok(output)
}

/// Decodes a summary aggregation.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, a duplicate
/// `dataPoints` field, or an invalid summary-point array.
fn decode_summary(cursor: &mut JsonCursor<'_>) -> Result<Summary, JsonDecodeError> {
    let mut output = Summary::default();
    object(cursor, |field, cursor| match field {
        Field::DataPoints => {
            output.data_points = decode_message_array(cursor, decode_summary_point)?;
            Ok(true)
        }
        _ => Ok(false),
    })?;
    Ok(output)
}

/// Decodes one number data point and its flattened value oneof.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate fields,
/// invalid attributes or exemplars, out-of-range timestamps, flags, or numeric
/// values, or more than one number-point value variant.
fn decode_number_point(cursor: &mut JsonCursor<'_>) -> Result<NumberDataPoint, JsonDecodeError> {
    let mut output = NumberDataPoint::default();
    object(cursor, |field, cursor| match field {
        Field::Attributes => {
            output.attributes = decode_message_array(cursor, decode_key_value)?;
            Ok(true)
        }
        Field::StartTimeUnixNano => {
            output.start_time_unix_nano = cursor.u64()?;
            Ok(true)
        }
        Field::TimeUnixNano => {
            output.time_unix_nano = cursor.u64()?;
            Ok(true)
        }
        Field::Exemplars => {
            output.exemplars = decode_message_array(cursor, decode_exemplar)?;
            Ok(true)
        }
        Field::Flags => {
            output.flags = decode_u32(cursor)?;
            Ok(true)
        }
        Field::AsDouble => {
            set_number_value(
                &mut output,
                number_data_point::Value::AsDouble(cursor.f64()?),
            )?;
            Ok(true)
        }
        Field::AsInt => {
            set_number_value(&mut output, number_data_point::Value::AsInt(cursor.i64()?))?;
            Ok(true)
        }
        _ => Ok(false),
    })?;
    Ok(output)
}

/// Sets a number-point oneof once.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] when the number point already contains a value
/// variant.
fn set_number_value(
    output: &mut NumberDataPoint,
    value: number_data_point::Value,
) -> Result<(), JsonDecodeError> {
    if output.value.is_some() {
        return Err(JsonDecodeError::at(0, "duplicate number-point value"));
    }
    output.value = Some(value);
    Ok(())
}

/// Decodes one explicit histogram point.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate fields,
/// invalid attributes, exemplars, bucket arrays, bounds, or optional numeric
/// values, or out-of-range timestamps, counts, and flags.
fn decode_histogram_point(
    cursor: &mut JsonCursor<'_>,
) -> Result<HistogramDataPoint, JsonDecodeError> {
    let mut output = HistogramDataPoint::default();
    object(cursor, |field, cursor| match field {
        Field::Attributes => {
            output.attributes = decode_message_array(cursor, decode_key_value)?;
            Ok(true)
        }
        Field::StartTimeUnixNano => {
            output.start_time_unix_nano = cursor.u64()?;
            Ok(true)
        }
        Field::TimeUnixNano => {
            output.time_unix_nano = cursor.u64()?;
            Ok(true)
        }
        Field::Count => {
            output.count = cursor.u64()?;
            Ok(true)
        }
        Field::Sum => {
            output.sum = decode_optional(cursor, |cursor| cursor.f64())?;
            Ok(true)
        }
        Field::BucketCounts => {
            output.bucket_counts = decode_u64_array(cursor)?;
            Ok(true)
        }
        Field::ExplicitBounds => {
            output.explicit_bounds = decode_f64_array(cursor)?;
            Ok(true)
        }
        Field::Exemplars => {
            output.exemplars = decode_message_array(cursor, decode_exemplar)?;
            Ok(true)
        }
        Field::Flags => {
            output.flags = decode_u32(cursor)?;
            Ok(true)
        }
        Field::Min => {
            output.min = decode_optional(cursor, |cursor| cursor.f64())?;
            Ok(true)
        }
        Field::Max => {
            output.max = decode_optional(cursor, |cursor| cursor.f64())?;
            Ok(true)
        }
        _ => Ok(false),
    })?;
    Ok(output)
}

/// Decodes one exponential histogram point.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate fields,
/// invalid attributes, exemplars, bucket messages, or optional numeric values,
/// or out-of-range timestamps, counts, scale, and flags.
fn decode_exponential_histogram_point(
    cursor: &mut JsonCursor<'_>,
) -> Result<ExponentialHistogramDataPoint, JsonDecodeError> {
    let mut output = ExponentialHistogramDataPoint::default();
    object(cursor, |field, cursor| match field {
        Field::Attributes => {
            output.attributes = decode_message_array(cursor, decode_key_value)?;
            Ok(true)
        }
        Field::StartTimeUnixNano => {
            output.start_time_unix_nano = cursor.u64()?;
            Ok(true)
        }
        Field::TimeUnixNano => {
            output.time_unix_nano = cursor.u64()?;
            Ok(true)
        }
        Field::Count => {
            output.count = cursor.u64()?;
            Ok(true)
        }
        Field::Sum => {
            output.sum = decode_optional(cursor, |cursor| cursor.f64())?;
            Ok(true)
        }
        Field::Scale => {
            output.scale = decode_i32(cursor)?;
            Ok(true)
        }
        Field::ZeroCount => {
            output.zero_count = cursor.u64()?;
            Ok(true)
        }
        Field::Positive => {
            output.positive = decode_optional(cursor, decode_buckets)?;
            Ok(true)
        }
        Field::Negative => {
            output.negative = decode_optional(cursor, decode_buckets)?;
            Ok(true)
        }
        Field::Flags => {
            output.flags = decode_u32(cursor)?;
            Ok(true)
        }
        Field::Exemplars => {
            output.exemplars = decode_message_array(cursor, decode_exemplar)?;
            Ok(true)
        }
        Field::Min => {
            output.min = decode_optional(cursor, |cursor| cursor.f64())?;
            Ok(true)
        }
        Field::Max => {
            output.max = decode_optional(cursor, |cursor| cursor.f64())?;
            Ok(true)
        }
        Field::ZeroThreshold => {
            output.zero_threshold = cursor.f64()?;
            Ok(true)
        }
        _ => Ok(false),
    })?;
    Ok(output)
}

/// Decodes one exponential bucket set.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate fields,
/// an out-of-range offset, or an invalid bucket-count array.
fn decode_buckets(
    cursor: &mut JsonCursor<'_>,
) -> Result<exponential_histogram_data_point::Buckets, JsonDecodeError> {
    let mut output = exponential_histogram_data_point::Buckets::default();
    object(cursor, |field, cursor| match field {
        Field::Offset => {
            output.offset = decode_i32(cursor)?;
            Ok(true)
        }
        Field::BucketCounts => {
            output.bucket_counts = decode_u64_array(cursor)?;
            Ok(true)
        }
        _ => Ok(false),
    })?;
    Ok(output)
}

/// Decodes one summary point.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate fields,
/// invalid attributes or quantile messages, or out-of-range timestamps,
/// counts, flags, and floating-point values.
fn decode_summary_point(cursor: &mut JsonCursor<'_>) -> Result<SummaryDataPoint, JsonDecodeError> {
    let mut output = SummaryDataPoint::default();
    object(cursor, |field, cursor| match field {
        Field::Attributes => {
            output.attributes = decode_message_array(cursor, decode_key_value)?;
            Ok(true)
        }
        Field::StartTimeUnixNano => {
            output.start_time_unix_nano = cursor.u64()?;
            Ok(true)
        }
        Field::TimeUnixNano => {
            output.time_unix_nano = cursor.u64()?;
            Ok(true)
        }
        Field::Count => {
            output.count = cursor.u64()?;
            Ok(true)
        }
        Field::Sum => {
            output.sum = cursor.f64()?;
            Ok(true)
        }
        Field::QuantileValues => {
            output.quantile_values = decode_message_array(cursor, decode_quantile)?;
            Ok(true)
        }
        Field::Flags => {
            output.flags = decode_u32(cursor)?;
            Ok(true)
        }
        _ => Ok(false),
    })?;
    Ok(output)
}

/// Decodes one summary quantile/value pair.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate fields,
/// or invalid quantile and value numbers.
fn decode_quantile(
    cursor: &mut JsonCursor<'_>,
) -> Result<summary_data_point::ValueAtQuantile, JsonDecodeError> {
    let mut output = summary_data_point::ValueAtQuantile::default();
    object(cursor, |field, cursor| match field {
        Field::Quantile => {
            output.quantile = cursor.f64()?;
            Ok(true)
        }
        Field::Value => {
            output.value = cursor.f64()?;
            Ok(true)
        }
        _ => Ok(false),
    })?;
    Ok(output)
}

/// Decodes one metric exemplar and its flattened value oneof.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate fields,
/// invalid filtered attributes, hexadecimal identifiers, timestamps, or
/// numeric values, or more than one exemplar value variant.
fn decode_exemplar(cursor: &mut JsonCursor<'_>) -> Result<Exemplar, JsonDecodeError> {
    let mut output = Exemplar::default();
    object(cursor, |field, cursor| match field {
        Field::FilteredAttributes => {
            output.filtered_attributes = decode_message_array(cursor, decode_key_value)?;
            Ok(true)
        }
        Field::TimeUnixNano => {
            output.time_unix_nano = cursor.u64()?;
            Ok(true)
        }
        Field::SpanId => {
            output.span_id = cursor.hex_bytes()?;
            Ok(true)
        }
        Field::TraceId => {
            output.trace_id = cursor.hex_bytes()?;
            Ok(true)
        }
        Field::AsDouble => {
            set_exemplar_value(&mut output, exemplar::Value::AsDouble(cursor.f64()?))?;
            Ok(true)
        }
        Field::AsInt => {
            set_exemplar_value(&mut output, exemplar::Value::AsInt(cursor.i64()?))?;
            Ok(true)
        }
        _ => Ok(false),
    })?;
    Ok(output)
}

/// Sets an exemplar oneof once.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] when the exemplar already contains a value
/// variant.
fn set_exemplar_value(
    output: &mut Exemplar,
    value: exemplar::Value,
) -> Result<(), JsonDecodeError> {
    if output.value.is_some() {
        return Err(JsonDecodeError::at(0, "duplicate exemplar value"));
    }
    output.value = Some(value);
    Ok(())
}

/// Decodes one object, rejecting duplicate recognized fields while skipping unknowns.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate
/// recognized fields, malformed skipped values or excessive nesting, or any
/// error propagated by `field_decoder`.
fn object(
    cursor: &mut JsonCursor<'_>,
    mut field_decoder: impl FnMut(Field, &mut JsonCursor<'_>) -> Result<bool, JsonDecodeError>,
) -> Result<(), JsonDecodeError> {
    let mut first = true;
    let mut seen = 0u128;
    cursor.expect(b'{')?;
    while let Some(field) = next_object_field(cursor, &mut first)? {
        if field_decoder(field, cursor)? {
            mark_seen(&mut seen, field)?;
        } else {
            cursor.skip_value(0)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::decode_metrics_json;

    /// Proves every aggregation shape constructs exact repeated capacity.
    #[test]
    fn metrics_json_constructs_exact_aggregation_vectors() {
        let input = br#"{"resourceMetrics":[{"scopeMetrics":[{"metrics":[{"name":"g","gauge":{"dataPoints":[{"timeUnixNano":"1","asInt":2}]}}]}]}]}"#;
        let plan = crate::otlp_json::preflight_metrics_json(input, test_limits())
            .expect("metrics JSON preflights");
        let request = decode_metrics_json(input, plan.decode_bytes).expect("metrics JSON decodes");
        assert_eq!(
            request.resource_metrics.len(),
            request.resource_metrics.capacity()
        );
        let metrics = &request.resource_metrics[0].scope_metrics[0].metrics;
        assert_eq!(metrics.len(), metrics.capacity());
        assert_eq!(metrics[0].name.len(), metrics[0].name.capacity());
    }

    /// Proves mutually exclusive metric data fields are rejected.
    #[test]
    fn metrics_json_rejects_multiple_data_variants() {
        let input =
            br#"{"resourceMetrics":[{"scopeMetrics":[{"metrics":[{"gauge":{},"sum":{}}]}]}]}"#;
        assert!(crate::otlp_json::preflight_metrics_json(input, test_limits()).is_err());
    }

    /// Proves metric resource, scope, record, attribute, and value-byte caps reject cap plus one.
    #[test]
    fn metrics_json_enforces_signal_preflight_caps() {
        for (field, accepted, refused) in [
            (
                "resources",
                br#"{"resourceMetrics":[{}]}"#.as_slice(),
                br#"{"resourceMetrics":[{},{}]}"#.as_slice(),
            ),
            (
                "scopes",
                br#"{"resourceMetrics":[{"scopeMetrics":[{}]}]}"#.as_slice(),
                br#"{"resourceMetrics":[{"scopeMetrics":[{},{}]}]}"#.as_slice(),
            ),
            (
                "records",
                br#"{"resourceMetrics":[{"scopeMetrics":[{"metrics":[{"gauge":{"dataPoints":[{}]}}]}]}]}"#.as_slice(),
                br#"{"resourceMetrics":[{"scopeMetrics":[{"metrics":[{"gauge":{"dataPoints":[{},{}]}}]}]}]}"#.as_slice(),
            ),
            (
                "attributes",
                br#"{"resourceMetrics":[{"resource":{"attributes":[{}]}}]}"#.as_slice(),
                br#"{"resourceMetrics":[{"resource":{"attributes":[{},{}]}}]}"#.as_slice(),
            ),
        ] {
            let mut limits = test_limits();
            match field {
                "resources" => limits.resources = 1,
                "scopes" => limits.scopes = 1,
                "records" => limits.records = 1,
                "attributes" => limits.attributes = 1,
                _ => unreachable!("closed test limit field"),
            }
            assert!(crate::otlp_json::preflight_metrics_json(accepted, limits).is_ok());
            assert!(crate::otlp_json::preflight_metrics_json(refused, limits).is_err());
        }

        let mut limits = test_limits();
        limits.value_bytes = 1;
        assert!(
            crate::otlp_json::preflight_metrics_json(
                br#"{"resourceMetrics":[{"schemaUrl":"a"}]}"#,
                limits,
            )
            .is_ok()
        );
        assert!(
            crate::otlp_json::preflight_metrics_json(
                br#"{"resourceMetrics":[{"schemaUrl":"ab"}]}"#,
                limits,
            )
            .is_err()
        );
    }

    /// Proves metric point attributes accept value depth eight and reject depth nine.
    #[test]
    fn metrics_json_enforces_any_value_depth_eight() {
        for (depth, accepted) in [(8, true), (9, false)] {
            let input = nested_metric_attribute_request(depth);
            let result = crate::otlp_json::preflight_metrics_json(input.as_bytes(), test_limits());
            assert_eq!(result.is_ok(), accepted, "depth {depth}");
        }
    }

    /// Proves escaped strings, IDs, and unpadded bytes preserve the exact owner plan.
    #[test]
    fn metrics_json_decodes_escaped_tokens_with_exact_owner() {
        let input = br#"{"resourceMetrics":[{"scopeMetrics":[{"metrics":[{"name":"m\u0065tric","gauge":{"dataPoints":[{"attributes":[{"value":{"bytesValue":"AQ\u0049"}}],"exemplars":[{"traceId":"00112233445566778899aabbccddeef\u0066","spanId":"001122334455667\u0037"}] }]}}]}]}]}"#;
        let plan = crate::otlp_json::preflight_metrics_json(input, test_limits())
            .expect("escaped metrics JSON preflights");
        let request =
            decode_metrics_json(input, plan.decode_bytes).expect("escaped metrics JSON decodes");
        let metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];
        assert_eq!(metric.name, "metric");
        assert_eq!(metric.name.len(), metric.name.capacity());
    }

    /// Builds a metrics request containing exactly `depth` nested value messages.
    fn nested_metric_attribute_request(depth: usize) -> String {
        let mut value = String::new();
        for _ in 1..depth {
            value.push_str(r#"{"arrayValue":{"values":["#);
        }
        value.push_str(r#"{"intValue":"1"}"#);
        for _ in 1..depth {
            value.push_str("]}}");
        }
        format!(
            r#"{{"resourceMetrics":[{{"scopeMetrics":[{{"metrics":[{{"gauge":{{"dataPoints":[{{"attributes":[{{"value":{value}}}]}}]}}}}]}}]}}]}}"#
        )
    }

    /// Builds permissive immutable limits for focused decoder tests.
    fn test_limits() -> vala_bifrost_redux::gate::OtlpWireLimits {
        vala_bifrost_redux::gate::OtlpWireLimits {
            request_bytes: 1 << 20,
            resources: 16,
            scopes: 16,
            records: 128,
            attributes: 1024,
            value_bytes: 1 << 20,
            value_depth: 8,
            event_days: 32,
        }
    }
}
