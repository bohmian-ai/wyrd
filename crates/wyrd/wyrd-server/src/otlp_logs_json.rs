//! Direct fixed-capacity OTLP logs JSON construction.

use std::mem::size_of;

use vala_bifrost_redux::gate::IngestError;
use wyrd_tonic::otlp::common::v1::{AnyValue, EntityRef, KeyValue, any_value};
use wyrd_tonic::otlp::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;

use crate::otlp_json::{
    Field, JsonCursor, JsonDecodeError, decode_any_value, decode_i32, decode_key_value,
    decode_message_array, decode_optional, decode_resource, decode_scope, decode_u32,
    json_ingest_error, mark_seen, next_object_field,
};

/// Constructs one generated OTLP logs request directly from lower-camel JSON.
///
/// Repeated fields receive their exact element count from the shared borrowed
/// cursor before allocation. Retained strings, hexadecimal identifiers, and
/// recursive value bytes likewise allocate their exact decoded capacities.
/// Unknown fields are skipped without allocation, ordinary generated fields
/// reject duplicates, and `AnyValue` preserves its serde last-known-field-wins
/// behavior through the shared decoder.
///
/// # Errors
///
/// Returns [`IngestError::Decode`] when JSON framing, a known field shape,
/// numeric range, identifier encoding, recursive value depth, duplicate field,
/// trailing input, or a preflight-versus-materialized retained-capacity
/// mismatch violates the OTLP JSON contract.
pub(crate) fn decode_logs_json(
    input: &[u8],
    expected_decode_bytes: usize,
) -> Result<ExportLogsServiceRequest, IngestError> {
    let mut cursor = JsonCursor::new(input);
    let request = decode_logs_request(&mut cursor).map_err(json_ingest_error)?;
    cursor.finish().map_err(json_ingest_error)?;
    let retained = verified_logs_capacity(&request)?;
    if retained != expected_decode_bytes {
        return Err(IngestError::Decode(
            "OTLP logs JSON retained capacity diverged from preflight".to_owned(),
        ));
    }
    Ok(request)
}

/// Verifies no generated collection retains spare capacity and totals live bytes.
///
/// # Errors
///
/// Returns a stable decode refusal when a collection has spare capacity or
/// exact retained-byte arithmetic overflows.
fn verified_logs_capacity(request: &ExportLogsServiceRequest) -> Result<usize, IngestError> {
    let mut bytes = size_of::<ExportLogsServiceRequest>();
    add_vec_layout(
        &mut bytes,
        request.resource_logs.len(),
        request.resource_logs.capacity(),
        size_of::<ResourceLogs>(),
    )?;
    for resource in &request.resource_logs {
        add_string(&mut bytes, &resource.schema_url)?;
        if let Some(value) = &resource.resource {
            add_attributes(&mut bytes, &value.attributes, value.attributes.capacity())?;
            add_vec_layout(
                &mut bytes,
                value.entity_refs.len(),
                value.entity_refs.capacity(),
                size_of::<EntityRef>(),
            )?;
            for entity in &value.entity_refs {
                add_string(&mut bytes, &entity.schema_url)?;
                add_string(&mut bytes, &entity.r#type)?;
                add_strings(&mut bytes, &entity.id_keys, entity.id_keys.capacity())?;
                add_strings(
                    &mut bytes,
                    &entity.description_keys,
                    entity.description_keys.capacity(),
                )?;
            }
        }
        add_vec_layout(
            &mut bytes,
            resource.scope_logs.len(),
            resource.scope_logs.capacity(),
            size_of::<ScopeLogs>(),
        )?;
        for scope in &resource.scope_logs {
            add_string(&mut bytes, &scope.schema_url)?;
            if let Some(value) = &scope.scope {
                add_string(&mut bytes, &value.name)?;
                add_string(&mut bytes, &value.version)?;
                add_attributes(&mut bytes, &value.attributes, value.attributes.capacity())?;
            }
            add_vec_layout(
                &mut bytes,
                scope.log_records.len(),
                scope.log_records.capacity(),
                size_of::<LogRecord>(),
            )?;
            for record in &scope.log_records {
                add_string(&mut bytes, &record.severity_text)?;
                add_byte_backing(
                    &mut bytes,
                    record.trace_id.len(),
                    record.trace_id.capacity(),
                )?;
                add_byte_backing(&mut bytes, record.span_id.len(), record.span_id.capacity())?;
                add_string(&mut bytes, &record.event_name)?;
                if let Some(body) = &record.body {
                    add_any_value(&mut bytes, body)?;
                }
                add_attributes(&mut bytes, &record.attributes, record.attributes.capacity())?;
            }
        }
    }
    Ok(bytes)
}

/// Adds one exact repeated-vector layout.
///
/// # Errors
///
/// Returns a decode refusal for spare capacity or byte arithmetic overflow.
fn add_vec_layout(
    bytes: &mut usize,
    len: usize,
    capacity: usize,
    element_bytes: usize,
) -> Result<(), IngestError> {
    if len != capacity {
        return Err(IngestError::Decode(
            "OTLP logs JSON collection retained spare capacity".to_owned(),
        ));
    }
    let backing = capacity
        .checked_mul(element_bytes)
        .ok_or_else(capacity_overflow)?;
    add_capacity(bytes, backing)
}

/// Adds an exact generated string backing allocation.
///
/// # Errors
///
/// Returns a decode refusal for spare capacity or byte arithmetic overflow.
fn add_string(bytes: &mut usize, value: &String) -> Result<(), IngestError> {
    if value.len() != value.capacity() {
        return Err(IngestError::Decode(
            "OTLP logs JSON string retained spare capacity".to_owned(),
        ));
    }
    add_capacity(bytes, value.capacity())
}

/// Adds an exact generated byte-vector backing allocation.
///
/// # Errors
///
/// Returns a decode refusal for spare capacity or byte arithmetic overflow.
fn add_byte_backing(bytes: &mut usize, len: usize, capacity: usize) -> Result<(), IngestError> {
    if len != capacity {
        return Err(IngestError::Decode(
            "OTLP logs JSON bytes retained spare capacity".to_owned(),
        ));
    }
    add_capacity(bytes, capacity)
}

/// Adds one exact vector of generated strings and their backing allocations.
///
/// # Errors
///
/// Returns a decode refusal for spare capacity or byte arithmetic overflow.
fn add_strings(bytes: &mut usize, values: &[String], capacity: usize) -> Result<(), IngestError> {
    add_vec_layout(bytes, values.len(), capacity, size_of::<String>())?;
    for value in values {
        add_string(bytes, value)?;
    }
    Ok(())
}

/// Adds one exact attribute vector and every recursive value allocation.
///
/// # Errors
///
/// Returns a decode refusal for spare capacity or byte arithmetic overflow.
fn add_attributes(
    bytes: &mut usize,
    values: &[KeyValue],
    capacity: usize,
) -> Result<(), IngestError> {
    add_vec_layout(bytes, values.len(), capacity, size_of::<KeyValue>())?;
    for value in values {
        add_string(bytes, &value.key)?;
        if let Some(value) = &value.value {
            add_any_value(bytes, value)?;
        }
    }
    Ok(())
}

/// Adds recursive backing retained by one inlined generated value.
///
/// # Errors
///
/// Returns a decode refusal for spare capacity or byte arithmetic overflow.
fn add_any_value(bytes: &mut usize, value: &AnyValue) -> Result<(), IngestError> {
    match value.value.as_ref() {
        Some(any_value::Value::StringValue(value)) => add_string(bytes, value),
        Some(any_value::Value::BytesValue(value)) => {
            add_byte_backing(bytes, value.len(), value.capacity())
        }
        Some(any_value::Value::ArrayValue(array)) => {
            add_vec_layout(
                bytes,
                array.values.len(),
                array.values.capacity(),
                size_of::<AnyValue>(),
            )?;
            for value in &array.values {
                add_any_value(bytes, value)?;
            }
            Ok(())
        }
        Some(any_value::Value::KvlistValue(list)) => {
            add_attributes(bytes, &list.values, list.values.capacity())
        }
        _ => Ok(()),
    }
}

/// Adds one checked capacity amount.
///
/// # Errors
///
/// Returns a decode refusal when the retained-byte total overflows.
fn add_capacity(bytes: &mut usize, amount: usize) -> Result<(), IngestError> {
    *bytes = bytes.checked_add(amount).ok_or_else(capacity_overflow)?;
    Ok(())
}

/// Constructs the stable retained-capacity arithmetic refusal.
fn capacity_overflow() -> IngestError {
    IngestError::Decode("OTLP logs JSON retained capacity overflow".to_owned())
}

/// Decodes the logs request root and its exact resource-group vector.
///
/// # Errors
///
/// Returns a cursor error for malformed framing, duplicate `resourceLogs`, or
/// an invalid resource-group array.
fn decode_logs_request(
    cursor: &mut JsonCursor<'_>,
) -> Result<ExportLogsServiceRequest, JsonDecodeError> {
    let mut output = ExportLogsServiceRequest::default();
    let mut first = true;
    let mut seen = 0_u128;
    cursor.consume_expected(b'{')?;
    while let Some(field) = next_object_field(cursor, &mut first)? {
        match field {
            Field::ResourceLogs => {
                mark_seen(&mut seen, field)?;
                output.resource_logs = decode_message_array(cursor, decode_resource_logs)?;
            }
            _ => cursor.skip_value(0)?,
        }
    }
    Ok(output)
}

/// Decodes one resource group with optional Resource and exact scope capacity.
///
/// # Errors
///
/// Returns a cursor error for malformed framing, duplicate generated fields,
/// or invalid resource, scope, or schema values.
fn decode_resource_logs(cursor: &mut JsonCursor<'_>) -> Result<ResourceLogs, JsonDecodeError> {
    let mut output = ResourceLogs::default();
    let mut first = true;
    let mut seen = 0_u128;
    cursor.consume_expected(b'{')?;
    while let Some(field) = next_object_field(cursor, &mut first)? {
        match field {
            Field::Resource => {
                mark_seen(&mut seen, field)?;
                output.resource = decode_optional(cursor, decode_resource)?;
            }
            Field::ScopeLogs => {
                mark_seen(&mut seen, field)?;
                output.scope_logs = decode_message_array(cursor, decode_scope_logs)?;
            }
            Field::SchemaUrl => {
                mark_seen(&mut seen, field)?;
                output.schema_url = cursor.owned_string()?;
            }
            _ => cursor.skip_value(0)?,
        }
    }
    Ok(output)
}

/// Decodes one scope group with optional scope and exact log-record capacity.
///
/// # Errors
///
/// Returns a cursor error for malformed framing, duplicate generated fields,
/// or invalid scope, record, or schema values.
fn decode_scope_logs(cursor: &mut JsonCursor<'_>) -> Result<ScopeLogs, JsonDecodeError> {
    let mut output = ScopeLogs::default();
    let mut first = true;
    let mut seen = 0_u128;
    cursor.consume_expected(b'{')?;
    while let Some(field) = next_object_field(cursor, &mut first)? {
        match field {
            Field::Scope => {
                mark_seen(&mut seen, field)?;
                output.scope = decode_optional(cursor, decode_scope)?;
            }
            Field::LogRecords => {
                mark_seen(&mut seen, field)?;
                output.log_records = decode_message_array(cursor, decode_log_record)?;
            }
            Field::SchemaUrl => {
                mark_seen(&mut seen, field)?;
                output.schema_url = cursor.owned_string()?;
            }
            _ => cursor.skip_value(0)?,
        }
    }
    Ok(output)
}

/// Decodes every generated logs field into its native scalar or exact backing.
///
/// # Errors
///
/// Returns a cursor error for malformed framing, duplicate generated fields,
/// invalid integer ranges, malformed identifiers, or invalid nested values.
fn decode_log_record(cursor: &mut JsonCursor<'_>) -> Result<LogRecord, JsonDecodeError> {
    let mut output = LogRecord::default();
    let mut first = true;
    let mut seen = 0_u128;
    cursor.consume_expected(b'{')?;
    while let Some(field) = next_object_field(cursor, &mut first)? {
        match field {
            Field::TimeUnixNano => {
                mark_seen(&mut seen, field)?;
                output.time_unix_nano = cursor.u64()?;
            }
            Field::ObservedTimeUnixNano => {
                mark_seen(&mut seen, field)?;
                output.observed_time_unix_nano = cursor.u64()?;
            }
            Field::SeverityNumber => {
                mark_seen(&mut seen, field)?;
                output.severity_number = decode_i32(cursor)?;
            }
            Field::SeverityText => {
                mark_seen(&mut seen, field)?;
                output.severity_text = cursor.owned_string()?;
            }
            Field::Body => {
                mark_seen(&mut seen, field)?;
                output.body = decode_optional(cursor, |cursor| decode_any_value(cursor, 0))?;
            }
            Field::Attributes => {
                mark_seen(&mut seen, field)?;
                output.attributes = decode_message_array(cursor, decode_key_value)?;
            }
            Field::DroppedAttributesCount => {
                mark_seen(&mut seen, field)?;
                output.dropped_attributes_count = decode_u32(cursor)?;
            }
            Field::Flags => {
                mark_seen(&mut seen, field)?;
                output.flags = decode_u32(cursor)?;
            }
            Field::TraceId => {
                mark_seen(&mut seen, field)?;
                output.trace_id = cursor.hex_bytes()?;
            }
            Field::SpanId => {
                mark_seen(&mut seen, field)?;
                output.span_id = cursor.hex_bytes()?;
            }
            Field::EventName => {
                mark_seen(&mut seen, field)?;
                output.event_name = cursor.owned_string()?;
            }
            _ => cursor.skip_value(0)?,
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use wyrd_tonic::otlp::common::v1::{AnyValue, EntityRef, KeyValue, any_value};
    use wyrd_tonic::otlp::resource::v1::Resource;

    use super::*;
    use crate::otlp_json::{decode_json_exact, preflight_logs_json};

    /// Computes exact live capacity retained below one generated logs request.
    fn decoded_logs_capacity(request: &ExportLogsServiceRequest) -> usize {
        size_of::<ExportLogsServiceRequest>()
            + request.resource_logs.capacity() * size_of::<ResourceLogs>()
            + request
                .resource_logs
                .iter()
                .map(decoded_resource_logs_capacity)
                .sum::<usize>()
    }

    /// Computes retained backing beneath one resource group.
    fn decoded_resource_logs_capacity(resource: &ResourceLogs) -> usize {
        resource.schema_url.capacity()
            + resource
                .resource
                .as_ref()
                .map_or(0, decoded_resource_capacity)
            + resource.scope_logs.capacity() * size_of::<ScopeLogs>()
            + resource
                .scope_logs
                .iter()
                .map(decoded_scope_logs_capacity)
                .sum::<usize>()
    }

    /// Computes retained backing beneath one shared resource.
    fn decoded_resource_capacity(resource: &Resource) -> usize {
        decoded_attributes_capacity(&resource.attributes)
            + resource.entity_refs.capacity() * size_of::<EntityRef>()
            + resource
                .entity_refs
                .iter()
                .map(|entity| {
                    entity.schema_url.capacity()
                        + entity.r#type.capacity()
                        + entity.id_keys.capacity() * size_of::<String>()
                        + entity.id_keys.iter().map(String::capacity).sum::<usize>()
                        + entity.description_keys.capacity() * size_of::<String>()
                        + entity
                            .description_keys
                            .iter()
                            .map(String::capacity)
                            .sum::<usize>()
                })
                .sum::<usize>()
    }

    /// Computes retained backing beneath one scope group.
    fn decoded_scope_logs_capacity(scope: &ScopeLogs) -> usize {
        scope.schema_url.capacity()
            + scope.scope.as_ref().map_or(0, |scope| {
                scope.name.capacity()
                    + scope.version.capacity()
                    + decoded_attributes_capacity(&scope.attributes)
            })
            + scope.log_records.capacity() * size_of::<LogRecord>()
            + scope
                .log_records
                .iter()
                .map(decoded_log_record_capacity)
                .sum::<usize>()
    }

    /// Computes retained backing beneath one log record.
    fn decoded_log_record_capacity(record: &LogRecord) -> usize {
        record.severity_text.capacity()
            + record.trace_id.capacity()
            + record.span_id.capacity()
            + record.event_name.capacity()
            + record.body.as_ref().map_or(0, decoded_any_value_capacity)
            + decoded_attributes_capacity(&record.attributes)
    }

    /// Computes exact vector layouts and recursive backing for attributes.
    fn decoded_attributes_capacity(attributes: &[KeyValue]) -> usize {
        size_of_val(attributes)
            + attributes
                .iter()
                .map(|attribute| {
                    attribute.key.capacity()
                        + attribute
                            .value
                            .as_ref()
                            .map_or(0, decoded_any_value_capacity)
                })
                .sum::<usize>()
    }

    /// Computes recursive retained backing below one inlined value.
    fn decoded_any_value_capacity(value: &AnyValue) -> usize {
        match value.value.as_ref() {
            Some(any_value::Value::StringValue(value)) => value.capacity(),
            Some(any_value::Value::BytesValue(value)) => value.capacity(),
            Some(any_value::Value::ArrayValue(array)) => {
                array.values.capacity() * size_of::<AnyValue>()
                    + array
                        .values
                        .iter()
                        .map(decoded_any_value_capacity)
                        .sum::<usize>()
            }
            Some(any_value::Value::KvlistValue(list)) => decoded_attributes_capacity(&list.values),
            _ => 0,
        }
    }

    /// Asserts exact capacities across every repeated and variable-width field.
    fn assert_fixed_logs_capacity(request: &ExportLogsServiceRequest) {
        assert_eq!(
            request.resource_logs.len(),
            request.resource_logs.capacity()
        );
        for resource in &request.resource_logs {
            assert_eq!(resource.schema_url.len(), resource.schema_url.capacity());
            if let Some(value) = &resource.resource {
                assert_eq!(value.attributes.len(), value.attributes.capacity());
                assert_eq!(value.entity_refs.len(), value.entity_refs.capacity());
                for entity in &value.entity_refs {
                    assert_eq!(entity.schema_url.len(), entity.schema_url.capacity());
                    assert_eq!(entity.r#type.len(), entity.r#type.capacity());
                    assert_eq!(entity.id_keys.len(), entity.id_keys.capacity());
                    assert_eq!(
                        entity.description_keys.len(),
                        entity.description_keys.capacity()
                    );
                }
            }
            assert_eq!(resource.scope_logs.len(), resource.scope_logs.capacity());
            for scope in &resource.scope_logs {
                assert_eq!(scope.schema_url.len(), scope.schema_url.capacity());
                if let Some(value) = &scope.scope {
                    assert_eq!(value.name.len(), value.name.capacity());
                    assert_eq!(value.version.len(), value.version.capacity());
                    assert_eq!(value.attributes.len(), value.attributes.capacity());
                }
                assert_eq!(scope.log_records.len(), scope.log_records.capacity());
                for record in &scope.log_records {
                    assert_eq!(record.severity_text.len(), record.severity_text.capacity());
                    assert_eq!(record.trace_id.len(), record.trace_id.capacity());
                    assert_eq!(record.span_id.len(), record.span_id.capacity());
                    assert_eq!(record.event_name.len(), record.event_name.capacity());
                    assert_eq!(record.attributes.len(), record.attributes.capacity());
                    if let Some(body) = &record.body {
                        assert_fixed_any_value(body);
                    }
                }
            }
        }
    }

    /// Asserts exact capacity through recursive generated values.
    fn assert_fixed_any_value(value: &AnyValue) {
        match value.value.as_ref() {
            Some(any_value::Value::StringValue(value)) => {
                assert_eq!(value.len(), value.capacity());
            }
            Some(any_value::Value::BytesValue(value)) => {
                assert_eq!(value.len(), value.capacity());
            }
            Some(any_value::Value::ArrayValue(array)) => {
                assert_eq!(array.values.len(), array.values.capacity());
                for value in &array.values {
                    assert_fixed_any_value(value);
                }
            }
            Some(any_value::Value::KvlistValue(list)) => {
                assert_eq!(list.values.len(), list.values.capacity());
                for attribute in &list.values {
                    assert_eq!(attribute.key.len(), attribute.key.capacity());
                    if let Some(value) = &attribute.value {
                        assert_fixed_any_value(value);
                    }
                }
            }
            _ => {}
        }
    }

    /// Builds one logs request containing exactly `depth` nested value messages.
    fn nested_body_request(depth: usize) -> String {
        let mut body = String::new();
        for _ in 1..depth {
            body.push_str(r#"{"arrayValue":{"values":["#);
        }
        body.push_str(r#"{"intValue":"1"}"#);
        for _ in 1..depth {
            body.push_str("]}}");
        }
        format!(r#"{{"resourceLogs":[{{"scopeLogs":[{{"logRecords":[{{"body":{body}}}]}}]}}]}}"#)
    }

    /// Proves the logs constructor matches serde and the preflight owner exactly.
    ///
    /// # Panics
    ///
    /// Panics if the rich valid fixture does not decode.
    #[test]
    fn direct_logs_json_matches_serde_and_exact_owner() {
        let input = br#"{
          "resourceLogs": [{
            "resource": {
              "attributes": [{"key":"service.name","value":{"stringValue":"wyrd"}}],
              "droppedAttributesCount": 1,
              "entityRefs": [{
                "schemaUrl":"entity/v1","type":"service",
                "idKeys":["service.name"],"descriptionKeys":["service.version"]
              }]
            },
            "scopeLogs": [{
              "scope": {
                "name":"logger","version":"1.0",
                "attributes":[{"key":"scope","value":{"boolValue":true}}],
                "droppedAttributesCount":2
              },
              "logRecords": [{
                "timeUnixNano":"10","observedTimeUnixNano":"11",
                "severityNumber":17,"severityText":"ERR\u004fR",
                "body":{"kvlistValue":{"values":[{
                  "key":"payload","value":{"bytesValue":"AQID"}
                }]}},
                "attributes":[{"key":"attempt","value":{"intValue":"3"}}],
                "droppedAttributesCount":4,"flags":257,
                "traceId":"01010101010101010101010101010101",
                "spanId":"0202020202020202","eventName":"failure",
                "unknown":{"nested":[1,2,3]}
              }],
              "schemaUrl":"scope/v1"
            }],
            "schemaUrl":"resource/v1"
          }]
        }"#;
        let expected: ExportLogsServiceRequest =
            decode_json_exact(input).expect("serde-compatible logs fixture");
        let plan = preflight_logs_json(input, vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS)
            .expect("logs JSON preflight");
        let decoded = decode_logs_json(input, plan.decode_bytes).expect("direct logs JSON fixture");
        assert_eq!(decoded, expected);
        assert_eq!(plan.decode_bytes, decoded_logs_capacity(&decoded));
        assert_fixed_logs_capacity(&decoded);
    }

    /// Proves ordinary duplicates reject while recursive values retain the final key.
    ///
    /// # Panics
    ///
    /// Panics if the valid last-known-field fixture does not decode.
    #[test]
    fn direct_logs_json_preserves_duplicate_semantics() {
        let duplicate = br#"{"resourceLogs":[],"resourceLogs":[]}"#;
        assert!(decode_logs_json(duplicate, 0).is_err());

        let last_wins = br#"{
          "resourceLogs":[{"scopeLogs":[{"logRecords":[{
            "ignored":1,"ignored":2,
            "body":{"stringValue":"discarded","intValue":"7"}
          }]}]}]
        }"#;
        let plan = preflight_logs_json(
            last_wins,
            vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS,
        )
        .expect("last-known value preflights");
        let decoded =
            decode_logs_json(last_wins, plan.decode_bytes).expect("last-known value decodes");
        let body = decoded.resource_logs[0].scope_logs[0].log_records[0]
            .body
            .as_ref()
            .and_then(|body| body.value.as_ref());
        assert!(matches!(body, Some(any_value::Value::IntValue(7))));
    }

    /// Proves the direct constructor accepts depth eight and rejects depth nine.
    #[test]
    fn direct_logs_json_enforces_value_depth_eight() {
        for (depth, accepted) in [(8, true), (9, false)] {
            let input = nested_body_request(depth);
            let plan = preflight_logs_json(
                input.as_bytes(),
                vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS,
            );
            let decoded = match plan {
                Ok(plan) => decode_logs_json(input.as_bytes(), plan.decode_bytes),
                Err(error) => Err(error),
            };
            assert_eq!(decoded.is_ok(), accepted, "depth {depth}");
        }
    }

    /// Proves production decode refuses a preflight/constructor owner mismatch.
    #[test]
    fn direct_logs_json_rejects_decode_owner_divergence() {
        let input = br#"{"resourceLogs":[]}"#;
        let plan = preflight_logs_json(input, vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS)
            .expect("empty logs request preflights");
        assert!(decode_logs_json(input, plan.decode_bytes + 1).is_err());
    }

    /// Proves log resource, scope, record, attribute, and value-byte caps reject cap plus one.
    #[test]
    fn direct_logs_json_enforces_signal_preflight_caps() {
        for (field, accepted, refused) in [
            (
                "resources",
                br#"{"resourceLogs":[{}]}"#.as_slice(),
                br#"{"resourceLogs":[{},{}]}"#.as_slice(),
            ),
            (
                "scopes",
                br#"{"resourceLogs":[{"scopeLogs":[{}]}]}"#.as_slice(),
                br#"{"resourceLogs":[{"scopeLogs":[{},{}]}]}"#.as_slice(),
            ),
            (
                "records",
                br#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[{}]}]}]}"#.as_slice(),
                br#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[{},{}]}]}]}"#.as_slice(),
            ),
            (
                "attributes",
                br#"{"resourceLogs":[{"resource":{"attributes":[{}]}}]}"#.as_slice(),
                br#"{"resourceLogs":[{"resource":{"attributes":[{},{}]}}]}"#.as_slice(),
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
            assert!(preflight_logs_json(accepted, limits).is_ok());
            assert!(preflight_logs_json(refused, limits).is_err());
        }

        let mut limits = test_limits();
        limits.value_bytes = 1;
        assert!(preflight_logs_json(br#"{"resourceLogs":[{"schemaUrl":"a"}]}"#, limits).is_ok());
        assert!(preflight_logs_json(br#"{"resourceLogs":[{"schemaUrl":"ab"}]}"#, limits).is_err());
    }

    /// Proves escaped strings, IDs, and unpadded bytes preserve the exact owner plan.
    #[test]
    fn direct_logs_json_decodes_escaped_tokens_with_exact_owner() {
        let input = br#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[{"severityText":"ER\u0052OR","body":{"bytesValue":"AQ\u0049"},"traceId":"00112233445566778899aabbccddeef\u0066","spanId":"001122334455667\u0037"}]}]}]}"#;
        let plan = preflight_logs_json(input, test_limits()).expect("escaped logs JSON preflights");
        let request =
            decode_logs_json(input, plan.decode_bytes).expect("escaped logs JSON decodes");
        assert_eq!(plan.decode_bytes, decoded_logs_capacity(&request));
        assert_fixed_logs_capacity(&request);
        let record = &request.resource_logs[0].scope_logs[0].log_records[0];
        assert_eq!(record.severity_text, "ERROR");
    }

    /// Proves malformed hex, integer ranges, and trailing input fail closed.
    #[test]
    fn direct_logs_json_rejects_invalid_scalars_and_trailing_input() {
        for input in [
            br#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[{"traceId":"xyz"}]}]}]}"#.as_slice(),
            br#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[{"flags":4294967296}]}]}]}"#
                .as_slice(),
            br#"{"resourceLogs":[]} true"#.as_slice(),
        ] {
            assert!(decode_logs_json(input, 0).is_err());
        }
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
            time_partitions: 32,
        }
    }
}
