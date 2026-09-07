//! Wire-shape ceilings applied to one decoded OTLP export before projection.
//!
//! These are Gate limits, not Scribe limits: they bound the resource, scope,
//! record, attribute, depth, and cumulative-byte shape of a decoded export
//! before Gate walks it into canonical Arrow, so a hostile request is refused
//! while it is still borrowed protobuf rather than after it has been
//! materialized into arrays. The counters are constant-space and borrow the
//! typed request; nothing here allocates or retains.

use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
use wyrd_tonic::otlp::metrics::v1::{
    Exemplar, ExponentialHistogramDataPoint, HistogramDataPoint, NumberDataPoint, SummaryDataPoint,
    metric,
};
use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;

use crate::contracts::ScribeError;
use crate::gate::limits::IngestLimits;

/// Refuses one decoded export whose retained wire size exceeds its ceiling.
///
/// # Errors
///
/// Returns [`ScribeError::PayloadTooLarge`] with the observed and configured
/// byte counts.
fn validate_request_bytes(bytes: usize, limits: IngestLimits) -> Result<(), ScribeError> {
    if bytes > limits.otlp.request_bytes {
        return Err(ScribeError::PayloadTooLarge {
            bytes,
            limit: limits.otlp.request_bytes,
        });
    }
    Ok(())
}

/// Constant-space typed OTLP counters.
#[derive(Debug)]
struct OtlpCounts {
    /// Immutable operator-selected ceilings for this count pass.
    limits: IngestLimits,
    /// Resource groups visited.
    resources: usize,
    /// Scope groups visited.
    scopes: usize,
    /// Signal records visited.
    records: usize,
    /// Attribute nodes visited.
    attributes: usize,
    /// Cumulative borrowed key/value/body bytes.
    value_bytes: usize,
}

impl OtlpCounts {
    /// Starts a constant-space count pass under one frozen limits snapshot.
    #[must_use]
    fn new(limits: IngestLimits) -> Self {
        Self {
            limits,
            resources: 0,
            scopes: 0,
            records: 0,
            attributes: 0,
            value_bytes: 0,
        }
    }

    /// Adds resources under the hard cap.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on overflow or cap+1.
    fn add_resources(&mut self, count: usize) -> Result<(), ScribeError> {
        self.resources = bounded_add(self.resources, count, self.limits.otlp.resources)?;
        Ok(())
    }

    /// Adds scopes under the hard cap.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on overflow or cap+1.
    fn add_scopes(&mut self, count: usize) -> Result<(), ScribeError> {
        self.scopes = bounded_add(self.scopes, count, self.limits.otlp.scopes)?;
        Ok(())
    }

    /// Adds signal records under the row cap.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::TooManyRows`] on overflow or cap+1.
    fn add_records(&mut self, count: usize) -> Result<(), ScribeError> {
        self.records = self
            .records
            .checked_add(count)
            .ok_or(ScribeError::TooManyRows {
                rows: u64::MAX,
                limit: self.limits.otlp.records as u64,
            })?;
        if self.records > self.limits.otlp.records || self.records > self.limits.rows {
            let limit = self.limits.otlp.records.min(self.limits.rows);
            return Err(ScribeError::TooManyRows {
                rows: u64::try_from(self.records).unwrap_or(u64::MAX),
                limit: limit as u64,
            });
        }
        Ok(())
    }

    /// Adds borrowed value bytes under the cumulative cap.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::DecodedPayloadTooLarge`] on overflow or cap+1.
    fn add_bytes(&mut self, bytes: usize) -> Result<(), ScribeError> {
        self.value_bytes =
            self.value_bytes
                .checked_add(bytes)
                .ok_or(ScribeError::DecodedPayloadTooLarge {
                    bytes: usize::MAX,
                    limit: self.limits.otlp.value_bytes,
                })?;
        if self.value_bytes > self.limits.otlp.value_bytes {
            return Err(ScribeError::DecodedPayloadTooLarge {
                bytes: self.value_bytes,
                limit: self.limits.otlp.value_bytes,
            });
        }
        Ok(())
    }
}

/// Adds one bounded counter.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on overflow or cap+1.
fn bounded_add(current: usize, count: usize, limit: usize) -> Result<usize, ScribeError> {
    let next = current
        .checked_add(count)
        .ok_or(ScribeError::InvalidFrame)?;
    if next > limit {
        return Err(ScribeError::InvalidFrame);
    }
    Ok(next)
}

/// Counts one gauge or sum point's adapter-visible semantic fields.
///
/// # Errors
///
/// Returns a stable row, cardinality, depth, or byte refusal.
fn count_number_point(point: &NumberDataPoint, counts: &mut OtlpCounts) -> Result<(), ScribeError> {
    counts.add_records(1)?;
    count_attributes(&point.attributes, counts)?;
    count_exemplars(&point.exemplars, counts)
}

/// Counts one explicit histogram point's adapter-visible semantic fields.
///
/// # Errors
///
/// Returns a stable row, cardinality, depth, or byte refusal.
fn count_histogram_point(
    point: &HistogramDataPoint,
    counts: &mut OtlpCounts,
) -> Result<(), ScribeError> {
    counts.add_records(1)?;
    count_attributes(&point.attributes, counts)?;
    count_exemplars(&point.exemplars, counts)
}

/// Counts one exponential histogram point's adapter-visible semantic fields.
///
/// # Errors
///
/// Returns a stable row, cardinality, depth, or byte refusal.
fn count_exponential_point(
    point: &ExponentialHistogramDataPoint,
    counts: &mut OtlpCounts,
) -> Result<(), ScribeError> {
    counts.add_records(1)?;
    count_attributes(&point.attributes, counts)?;
    count_exemplars(&point.exemplars, counts)
}

/// Counts one summary point's adapter-visible semantic fields.
///
/// # Errors
///
/// Returns a stable row, cardinality, depth, or byte refusal.
fn count_summary_point(
    point: &SummaryDataPoint,
    counts: &mut OtlpCounts,
) -> Result<(), ScribeError> {
    counts.add_records(1)?;
    count_attributes(&point.attributes, counts)?;
    Ok(())
}

/// Counts raw scalable fields retained by one OTLP resource.
///
/// # Errors
///
/// Returns a stable cardinality, depth, or cumulative-byte refusal.
fn count_resource(
    resource: Option<&wyrd_tonic::otlp::resource::v1::Resource>,
    counts: &mut OtlpCounts,
) -> Result<(), ScribeError> {
    let Some(resource) = resource else {
        return Ok(());
    };
    count_attributes(&resource.attributes, counts)?;
    for entity in &resource.entity_refs {
        counts.add_bytes(entity.schema_url.len())?;
        counts.add_bytes(entity.r#type.len())?;
        for key in &entity.id_keys {
            counts.add_bytes(key.len())?;
        }
        for key in &entity.description_keys {
            counts.add_bytes(key.len())?;
        }
    }
    Ok(())
}

/// Counts exemplar identifiers and filtered attributes exactly once.
///
/// # Errors
///
/// Returns a stable cardinality, depth, or cumulative-byte refusal.
fn count_exemplars(values: &[Exemplar], counts: &mut OtlpCounts) -> Result<(), ScribeError> {
    for exemplar in values {
        count_attributes(&exemplar.filtered_attributes, counts)?;
        counts.add_bytes(exemplar.trace_id.len())?;
        counts.add_bytes(exemplar.span_id.len())?;
    }
    Ok(())
}

/// Counts an attribute list and recursively nested values.
///
/// # Errors
///
/// Returns a stable cardinality, depth, or byte refusal.
fn count_attributes(attributes: &[KeyValue], counts: &mut OtlpCounts) -> Result<(), ScribeError> {
    count_attribute_nodes(attributes, 1, counts)
}

/// Counts attributes while preserving their current recursive value depth.
///
/// # Errors
///
/// Returns a stable cardinality, depth, or cumulative-byte refusal.
fn count_attribute_nodes(
    attributes: &[KeyValue],
    depth: usize,
    counts: &mut OtlpCounts,
) -> Result<(), ScribeError> {
    if depth > counts.limits.otlp.value_depth {
        return Err(ScribeError::InvalidFrame);
    }
    counts.attributes = bounded_add(
        counts.attributes,
        attributes.len(),
        counts.limits.otlp.attributes,
    )?;
    for attribute in attributes {
        counts.add_bytes(attribute.key.len())?;
        if let Some(value) = &attribute.value {
            count_value_nodes(value, depth, counts)?;
        }
    }
    Ok(())
}

/// Counts one nested OTLP value without constructing JSON.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] beyond the depth cap and a stable
/// material refusal beyond the cumulative key/value cap.
fn count_value(value: &AnyValue, depth: usize, counts: &mut OtlpCounts) -> Result<(), ScribeError> {
    count_value_nodes(value, depth, counts)
}

/// Traverses recursive OTLP values while charging only retained raw bytes.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] beyond the depth or nested attribute cap.
fn count_value_nodes(
    value: &AnyValue,
    depth: usize,
    counts: &mut OtlpCounts,
) -> Result<(), ScribeError> {
    if depth > counts.limits.otlp.value_depth {
        return Err(ScribeError::InvalidFrame);
    }
    match value.value.as_ref() {
        Some(any_value::Value::ArrayValue(values)) => {
            for value in &values.values {
                count_value_nodes(value, depth + 1, counts)?;
            }
        }
        Some(any_value::Value::KvlistValue(values)) => {
            count_attribute_nodes(&values.values, depth + 1, counts)?;
        }
        Some(any_value::Value::StringValue(value)) => counts.add_bytes(value.len())?,
        Some(any_value::Value::BytesValue(value)) => counts.add_bytes(value.len())?,
        Some(
            any_value::Value::BoolValue(_)
            | any_value::Value::IntValue(_)
            | any_value::Value::DoubleValue(_),
        )
        | None => {}
    }
    Ok(())
}

/// Enforces every wire ceiling on one decoded trace export.
///
/// # Errors
///
/// Returns a stable request-size, cardinality, depth, row, or cumulative-byte
/// refusal identical to the one this walk produced before projection moved to
/// Gate.
pub(crate) fn enforce_trace_limits(
    request: &ExportTraceServiceRequest,
    request_bytes: usize,
    limits: IngestLimits,
) -> Result<(), ScribeError> {
    validate_request_bytes(request_bytes, limits)?;
    let mut counts = OtlpCounts::new(limits);
    for resource in &request.resource_spans {
        counts.add_resources(1)?;
        counts.add_bytes(resource.schema_url.len())?;
        count_resource(resource.resource.as_ref(), &mut counts)?;
        for scope in &resource.scope_spans {
            counts.add_scopes(1)?;
            counts.add_bytes(scope.schema_url.len())?;
            if let Some(identity) = &scope.scope {
                counts.add_bytes(identity.name.len())?;
                counts.add_bytes(identity.version.len())?;
            }
            count_attributes(
                scope.scope.as_ref().map_or(&[], |value| &value.attributes),
                &mut counts,
            )?;
            for span in &scope.spans {
                counts.add_records(1)?;
                counts.add_bytes(span.name.len())?;
                counts.add_bytes(span.trace_state.len())?;
                counts.add_bytes(span.trace_id.len())?;
                counts.add_bytes(span.span_id.len())?;
                counts.add_bytes(span.parent_span_id.len())?;
                if let Some(status) = &span.status {
                    counts.add_bytes(status.message.len())?;
                }
                count_attributes(&span.attributes, &mut counts)?;
                for event in &span.events {
                    counts.add_bytes(event.name.len())?;
                    count_attributes(&event.attributes, &mut counts)?;
                }
                for link in &span.links {
                    counts.add_bytes(link.trace_state.len())?;
                    counts.add_bytes(link.trace_id.len())?;
                    counts.add_bytes(link.span_id.len())?;
                    count_attributes(&link.attributes, &mut counts)?;
                }
            }
        }
    }
    Ok(())
}

/// Enforces every wire ceiling on one decoded metrics export.
///
/// # Errors
///
/// Returns a stable request-size, cardinality, depth, row, or cumulative-byte
/// refusal.
pub(crate) fn enforce_metric_limits(
    request: &ExportMetricsServiceRequest,
    request_bytes: usize,
    limits: IngestLimits,
) -> Result<(), ScribeError> {
    validate_request_bytes(request_bytes, limits)?;
    let mut counts = OtlpCounts::new(limits);
    for resource in &request.resource_metrics {
        counts.add_resources(1)?;
        counts.add_bytes(resource.schema_url.len())?;
        count_resource(resource.resource.as_ref(), &mut counts)?;
        for scope in &resource.scope_metrics {
            counts.add_scopes(1)?;
            counts.add_bytes(scope.schema_url.len())?;
            if let Some(identity) = &scope.scope {
                counts.add_bytes(identity.name.len())?;
                counts.add_bytes(identity.version.len())?;
            }
            count_attributes(
                scope.scope.as_ref().map_or(&[], |value| &value.attributes),
                &mut counts,
            )?;
            for metric in &scope.metrics {
                counts.add_bytes(metric.name.len())?;
                counts.add_bytes(metric.description.len())?;
                counts.add_bytes(metric.unit.len())?;
                count_attributes(&metric.metadata, &mut counts)?;
                match metric.data.as_ref() {
                    Some(metric::Data::Gauge(value)) => {
                        for point in &value.data_points {
                            count_number_point(point, &mut counts)?;
                        }
                    }
                    Some(metric::Data::Sum(value)) => {
                        for point in &value.data_points {
                            count_number_point(point, &mut counts)?;
                        }
                    }
                    Some(metric::Data::Histogram(value)) => {
                        for point in &value.data_points {
                            count_histogram_point(point, &mut counts)?;
                        }
                    }
                    Some(metric::Data::ExponentialHistogram(value)) => {
                        for point in &value.data_points {
                            count_exponential_point(point, &mut counts)?;
                        }
                    }
                    Some(metric::Data::Summary(value)) => {
                        for point in &value.data_points {
                            count_summary_point(point, &mut counts)?;
                        }
                    }
                    None => {}
                }
            }
        }
    }
    Ok(())
}

/// Enforces every wire ceiling on one decoded logs export.
///
/// # Errors
///
/// Returns a stable request-size, cardinality, depth, row, or cumulative-byte
/// refusal.
pub(crate) fn enforce_log_limits(
    request: &ExportLogsServiceRequest,
    request_bytes: usize,
    limits: IngestLimits,
) -> Result<(), ScribeError> {
    validate_request_bytes(request_bytes, limits)?;
    let mut counts = OtlpCounts::new(limits);
    for resource in &request.resource_logs {
        counts.add_resources(1)?;
        counts.add_bytes(resource.schema_url.len())?;
        count_resource(resource.resource.as_ref(), &mut counts)?;
        for scope in &resource.scope_logs {
            counts.add_scopes(1)?;
            counts.add_bytes(scope.schema_url.len())?;
            if let Some(identity) = &scope.scope {
                counts.add_bytes(identity.name.len())?;
                counts.add_bytes(identity.version.len())?;
            }
            count_attributes(
                scope.scope.as_ref().map_or(&[], |value| &value.attributes),
                &mut counts,
            )?;
            for record in &scope.log_records {
                counts.add_records(1)?;
                counts.add_bytes(record.severity_text.len())?;
                counts.add_bytes(record.trace_id.len())?;
                counts.add_bytes(record.span_id.len())?;
                counts.add_bytes(record.event_name.len())?;
                count_attributes(&record.attributes, &mut counts)?;
                if let Some(body) = &record.body {
                    count_value(body, 1, &mut counts)?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use wyrd_tonic::otlp::common::v1::{ArrayValue, KeyValueList};
    use wyrd_tonic::otlp::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
    use wyrd_tonic::otlp::metrics::v1::{exponential_histogram_data_point, summary_data_point};

    use super::{
        IngestLimits, OtlpCounts, ScribeError, count_exponential_point, count_histogram_point,
        count_number_point, count_summary_point, count_value, enforce_log_limits,
    };
    use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
    use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
    use wyrd_tonic::otlp::metrics::v1::{
        Exemplar, ExponentialHistogramDataPoint, HistogramDataPoint, NumberDataPoint,
        SummaryDataPoint,
    };

    /// Builds OTLP counters with focused value and node limits.
    fn otlp_counts(value_bytes: usize, attributes: usize) -> OtlpCounts {
        let mut limits = IngestLimits::default();
        limits.otlp.value_bytes = value_bytes;
        limits.otlp.attributes = attributes;
        OtlpCounts::new(limits)
    }

    /// OTLP nesting beyond the closed depth is refused by the count pass.
    #[test]
    fn otlp_count_pass_rejects_excess_depth() {
        let mut value = AnyValue {
            value: Some(any_value::Value::StringValue("leaf".to_owned())),
        };
        for _ in 0..crate::gate::limits::OTLP_WIRE_LIMITS.value_depth {
            value = AnyValue {
                value: Some(any_value::Value::ArrayValue(ArrayValue {
                    values: vec![value],
                })),
            };
        }
        let request = ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                scope_logs: vec![ScopeLogs {
                    log_records: vec![LogRecord {
                        attributes: vec![KeyValue {
                            key: "nested".to_owned(),
                            value: Some(value),
                        }],
                        ..LogRecord::default()
                    }],
                    ..ScopeLogs::default()
                }],
                ..ResourceLogs::default()
            }],
        };
        assert!(matches!(
            enforce_log_limits(&request, 1, IngestLimits::default()),
            Err(ScribeError::InvalidFrame)
        ));
    }

    /// Nested `KeyValue` entries consume the same cardinality budget as top-level attributes.
    #[test]
    fn otlp_count_pass_rejects_nested_attribute_cap_plus_one() {
        let value = AnyValue {
            value: Some(any_value::Value::KvlistValue(KeyValueList {
                values: vec![KeyValue::default(); 3],
            })),
        };
        let mut counts = otlp_counts(usize::MAX, 2);
        assert!(matches!(
            count_value(&value, 1, &mut counts),
            Err(ScribeError::InvalidFrame)
        ));
    }

    /// `AnyValue` admission charges raw retained string and byte values exactly once.
    #[test]
    fn otlp_count_pass_uses_exact_raw_any_value_bytes() {
        let value = AnyValue {
            value: Some(any_value::Value::ArrayValue(ArrayValue {
                values: vec![
                    AnyValue {
                        value: Some(any_value::Value::StringValue("quoted \"value\"".to_owned())),
                    },
                    AnyValue {
                        value: Some(any_value::Value::BytesValue(vec![1, 2, 3, 4])),
                    },
                    AnyValue {
                        value: Some(any_value::Value::IntValue(-12)),
                    },
                ],
            })),
        };
        let exact = "quoted \"value\"".len() + 4;
        let mut boundary = otlp_counts(exact, 0);
        count_value(&value, 1, &mut boundary).expect("exact byte boundary");
        assert_eq!(boundary.value_bytes, exact);
        let mut over = otlp_counts(exact - 1, 0);
        assert!(matches!(
            count_value(&value, 1, &mut over),
            Err(ScribeError::DecodedPayloadTooLarge { .. })
        ));
    }

    /// Every metric point shape contributes records while semantic bytes stay adapter-parity exact.
    #[test]
    fn otlp_count_pass_covers_every_metric_shape() {
        let exemplar = Exemplar {
            filtered_attributes: vec![KeyValue {
                key: "filtered".to_owned(),
                value: Some(AnyValue {
                    value: Some(any_value::Value::StringValue("yes".to_owned())),
                }),
            }],
            trace_id: vec![1; 16],
            span_id: vec![2; 8],
            ..Exemplar::default()
        };
        let number = NumberDataPoint {
            exemplars: vec![exemplar.clone()],
            ..NumberDataPoint::default()
        };
        let histogram = HistogramDataPoint {
            bucket_counts: vec![1, 2],
            explicit_bounds: vec![0.5],
            ..HistogramDataPoint::default()
        };
        let exponential = ExponentialHistogramDataPoint {
            positive: Some(exponential_histogram_data_point::Buckets {
                offset: -2,
                bucket_counts: vec![3, 4],
            }),
            negative: Some(exponential_histogram_data_point::Buckets {
                offset: 1,
                bucket_counts: vec![5],
            }),
            ..ExponentialHistogramDataPoint::default()
        };
        let summary = SummaryDataPoint {
            quantile_values: vec![summary_data_point::ValueAtQuantile {
                quantile: 0.5,
                value: 9.0,
            }],
            ..SummaryDataPoint::default()
        };
        let mut counts = otlp_counts(usize::MAX, usize::MAX);
        count_number_point(&number, &mut counts).expect("number point");
        count_number_point(&NumberDataPoint::default(), &mut counts).expect("sum point");
        count_histogram_point(&histogram, &mut counts).expect("histogram point");
        count_exponential_point(&exponential, &mut counts).expect("exponential point");
        count_summary_point(&summary, &mut counts).expect("summary point");
        let expected =
            "filtered".len() + "yes".len() + exemplar.trace_id.len() + exemplar.span_id.len();
        assert_eq!(counts.records, 5);
        assert_eq!(counts.attributes, 1);
        assert_eq!(counts.value_bytes, expected);
    }
}
