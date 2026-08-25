//! Direct exact-capacity OTLP trace JSON construction.

use std::mem::size_of;

use vala_bifrost_redux::gate::IngestError;
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span, Status, span};
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;

use crate::otlp_json::{
    Field, JsonCursor, JsonDecodeError, attributes_capacity, decode_i32, decode_key_value,
    decode_message_array, decode_optional, decode_resource, decode_scope, decode_u32,
    json_ingest_error, mark_seen, next_object_field, resource_capacity, scope_capacity,
};

/// Constructs a trace export directly from its preflighted JSON bytes.
///
/// Every repeated field is locally rescanned for its exact length before one
/// `Vec::with_capacity` allocation; strings and identifiers are likewise exact.
///
/// # Errors
///
/// Returns a stable decode error for malformed JSON or protobuf-JSON semantics,
/// trailing input, or a preflight-versus-materialized retained-capacity
/// mismatch.
pub(crate) fn decode_trace_json(
    input: &[u8],
    expected_decode_bytes: usize,
) -> Result<ExportTraceServiceRequest, IngestError> {
    let mut cursor = JsonCursor::new(input);
    let request = decode_trace_request(&mut cursor).map_err(json_ingest_error)?;
    cursor.finish().map_err(json_ingest_error)?;
    if trace_capacity(&request) != expected_decode_bytes {
        return Err(IngestError::Decode(
            "OTLP trace JSON preflight/materialized capacity mismatch".to_owned(),
        ));
    }
    Ok(request)
}

/// Computes the exact recursively retained trace request capacity.
fn trace_capacity(request: &ExportTraceServiceRequest) -> usize {
    size_of::<ExportTraceServiceRequest>()
        + request.resource_spans.capacity() * size_of::<ResourceSpans>()
        + request
            .resource_spans
            .iter()
            .map(resource_spans_capacity)
            .sum::<usize>()
}

/// Computes backing retained beneath one resource-spans element.
fn resource_spans_capacity(group: &ResourceSpans) -> usize {
    group.schema_url.capacity()
        + group.scope_spans.capacity() * size_of::<ScopeSpans>()
        + group.resource.as_ref().map_or(0, resource_capacity)
        + group
            .scope_spans
            .iter()
            .map(scope_spans_capacity)
            .sum::<usize>()
}

/// Computes backing retained beneath one scope-spans element.
fn scope_spans_capacity(group: &ScopeSpans) -> usize {
    group.schema_url.capacity()
        + group.spans.capacity() * size_of::<Span>()
        + group.scope.as_ref().map_or(0, scope_capacity)
        + group.spans.iter().map(span_capacity).sum::<usize>()
}

/// Computes backing retained beneath one span.
fn span_capacity(value: &Span) -> usize {
    value.trace_id.capacity()
        + value.span_id.capacity()
        + value.trace_state.capacity()
        + value.parent_span_id.capacity()
        + value.name.capacity()
        + attributes_capacity(&value.attributes)
        + value.events.capacity() * size_of::<span::Event>()
        + value
            .events
            .iter()
            .map(|event| event.name.capacity() + attributes_capacity(&event.attributes))
            .sum::<usize>()
        + value.links.capacity() * size_of::<span::Link>()
        + value
            .links
            .iter()
            .map(|link| {
                link.trace_id.capacity()
                    + link.span_id.capacity()
                    + link.trace_state.capacity()
                    + attributes_capacity(&link.attributes)
            })
            .sum::<usize>()
        + value
            .status
            .as_ref()
            .map_or(0, |status| status.message.capacity())
}

/// Decodes the trace export root.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate
/// `resourceSpans`, or an invalid nested resource-spans array.
fn decode_trace_request(
    cursor: &mut JsonCursor<'_>,
) -> Result<ExportTraceServiceRequest, JsonDecodeError> {
    let mut output = ExportTraceServiceRequest::default();
    let mut first = true;
    let mut seen = 0u128;
    cursor.expect(b'{')?;
    while let Some(field) = next_object_field(cursor, &mut first)? {
        if field != Field::ResourceSpans {
            cursor.skip_value(0)?;
            continue;
        }
        mark_seen(&mut seen, field)?;
        match field {
            Field::ResourceSpans => {
                output.resource_spans = decode_message_array(cursor, decode_resource_spans)?;
            }
            _ => cursor.skip_value(0)?,
        }
    }
    Ok(output)
}

/// Decodes one resource-spans group.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate known
/// fields, invalid resource or scope-spans messages, or malformed strings.
fn decode_resource_spans(cursor: &mut JsonCursor<'_>) -> Result<ResourceSpans, JsonDecodeError> {
    let mut output = ResourceSpans::default();
    let mut first = true;
    let mut seen = 0u128;
    cursor.expect(b'{')?;
    while let Some(field) = next_object_field(cursor, &mut first)? {
        if !matches!(
            field,
            Field::Resource | Field::ScopeSpans | Field::SchemaUrl
        ) {
            cursor.skip_value(0)?;
            continue;
        }
        mark_seen(&mut seen, field)?;
        match field {
            Field::Resource => output.resource = decode_optional(cursor, decode_resource)?,
            Field::ScopeSpans => {
                output.scope_spans = decode_message_array(cursor, decode_scope_spans)?;
            }
            Field::SchemaUrl => output.schema_url = cursor.owned_string()?,
            _ => cursor.skip_value(0)?,
        }
    }
    Ok(output)
}

/// Decodes one scope-spans group.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate known
/// fields, invalid scope or span messages, or malformed strings.
fn decode_scope_spans(cursor: &mut JsonCursor<'_>) -> Result<ScopeSpans, JsonDecodeError> {
    let mut output = ScopeSpans::default();
    let mut first = true;
    let mut seen = 0u128;
    cursor.expect(b'{')?;
    while let Some(field) = next_object_field(cursor, &mut first)? {
        if !matches!(field, Field::Scope | Field::Spans | Field::SchemaUrl) {
            cursor.skip_value(0)?;
            continue;
        }
        mark_seen(&mut seen, field)?;
        match field {
            Field::Scope => output.scope = decode_optional(cursor, decode_scope)?,
            Field::Spans => output.spans = decode_message_array(cursor, decode_span)?,
            Field::SchemaUrl => output.schema_url = cursor.owned_string()?,
            _ => cursor.skip_value(0)?,
        }
    }
    Ok(output)
}

/// Decodes one span and all nested events, links, and attributes.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate known
/// fields, invalid hexadecimal identifiers, out-of-range numeric values, or
/// malformed nested attributes, events, links, and status messages.
fn decode_span(cursor: &mut JsonCursor<'_>) -> Result<Span, JsonDecodeError> {
    let mut output = Span::default();
    let mut first = true;
    let mut seen = 0u128;
    cursor.expect(b'{')?;
    while let Some(field) = next_object_field(cursor, &mut first)? {
        if !matches!(
            field,
            Field::TraceId
                | Field::SpanId
                | Field::TraceState
                | Field::ParentSpanId
                | Field::Flags
                | Field::Name
                | Field::Kind
                | Field::StartTimeUnixNano
                | Field::EndTimeUnixNano
                | Field::Attributes
                | Field::DroppedAttributesCount
                | Field::Events
                | Field::DroppedEventsCount
                | Field::Links
                | Field::DroppedLinksCount
                | Field::Status
        ) {
            cursor.skip_value(0)?;
            continue;
        }
        mark_seen(&mut seen, field)?;
        match field {
            Field::TraceId => output.trace_id = cursor.hex_bytes()?,
            Field::SpanId => output.span_id = cursor.hex_bytes()?,
            Field::TraceState => output.trace_state = cursor.owned_string()?,
            Field::ParentSpanId => output.parent_span_id = cursor.hex_bytes()?,
            Field::Flags => output.flags = decode_u32(cursor)?,
            Field::Name => output.name = cursor.owned_string()?,
            Field::Kind => output.kind = decode_i32(cursor)?,
            Field::StartTimeUnixNano => output.start_time_unix_nano = cursor.u64()?,
            Field::EndTimeUnixNano => output.end_time_unix_nano = cursor.u64()?,
            Field::Attributes => {
                output.attributes = decode_message_array(cursor, decode_key_value)?;
            }
            Field::DroppedAttributesCount => output.dropped_attributes_count = decode_u32(cursor)?,
            Field::Events => output.events = decode_message_array(cursor, decode_event)?,
            Field::DroppedEventsCount => output.dropped_events_count = decode_u32(cursor)?,
            Field::Links => output.links = decode_message_array(cursor, decode_link)?,
            Field::DroppedLinksCount => output.dropped_links_count = decode_u32(cursor)?,
            Field::Status => output.status = decode_optional(cursor, decode_status)?,
            _ => cursor.skip_value(0)?,
        }
    }
    Ok(output)
}

/// Decodes one span event.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate fields,
/// invalid timestamps or counts, malformed strings, or invalid attributes.
fn decode_event(cursor: &mut JsonCursor<'_>) -> Result<span::Event, JsonDecodeError> {
    let mut output = span::Event::default();
    let mut first = true;
    let mut seen = 0u128;
    cursor.expect(b'{')?;
    while let Some(field) = next_object_field(cursor, &mut first)? {
        if !matches!(
            field,
            Field::TimeUnixNano | Field::Name | Field::Attributes | Field::DroppedAttributesCount
        ) {
            cursor.skip_value(0)?;
            continue;
        }
        mark_seen(&mut seen, field)?;
        match field {
            Field::TimeUnixNano => output.time_unix_nano = cursor.u64()?,
            Field::Name => output.name = cursor.owned_string()?,
            Field::Attributes => {
                output.attributes = decode_message_array(cursor, decode_key_value)?;
            }
            Field::DroppedAttributesCount => output.dropped_attributes_count = decode_u32(cursor)?,
            _ => cursor.skip_value(0)?,
        }
    }
    Ok(output)
}

/// Decodes one span link.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate fields,
/// invalid hexadecimal identifiers, malformed strings or attributes, or
/// out-of-range flags and counts.
fn decode_link(cursor: &mut JsonCursor<'_>) -> Result<span::Link, JsonDecodeError> {
    let mut output = span::Link::default();
    let mut first = true;
    let mut seen = 0u128;
    cursor.expect(b'{')?;
    while let Some(field) = next_object_field(cursor, &mut first)? {
        if !matches!(
            field,
            Field::TraceId
                | Field::SpanId
                | Field::TraceState
                | Field::Attributes
                | Field::DroppedAttributesCount
                | Field::Flags
        ) {
            cursor.skip_value(0)?;
            continue;
        }
        mark_seen(&mut seen, field)?;
        match field {
            Field::TraceId => output.trace_id = cursor.hex_bytes()?,
            Field::SpanId => output.span_id = cursor.hex_bytes()?,
            Field::TraceState => output.trace_state = cursor.owned_string()?,
            Field::Attributes => {
                output.attributes = decode_message_array(cursor, decode_key_value)?;
            }
            Field::DroppedAttributesCount => output.dropped_attributes_count = decode_u32(cursor)?,
            Field::Flags => output.flags = decode_u32(cursor)?,
            _ => cursor.skip_value(0)?,
        }
    }
    Ok(output)
}

/// Decodes one span status.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] for malformed object framing, duplicate fields,
/// malformed status text, or an out-of-range status code.
fn decode_status(cursor: &mut JsonCursor<'_>) -> Result<Status, JsonDecodeError> {
    let mut output = Status::default();
    let mut first = true;
    let mut seen = 0u128;
    cursor.expect(b'{')?;
    while let Some(field) = next_object_field(cursor, &mut first)? {
        if !matches!(field, Field::Message | Field::Code) {
            cursor.skip_value(0)?;
            continue;
        }
        mark_seen(&mut seen, field)?;
        match field {
            Field::Message => output.message = cursor.owned_string()?,
            Field::Code => output.code = decode_i32(cursor)?,
            _ => cursor.skip_value(0)?,
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::decode_trace_json;

    /// Proves lower-camel fields, integer enums, quoted timestamps, hex IDs,
    /// unknown skipping, and exact repeated/string capacities together.
    #[test]
    fn trace_json_builds_exact_generated_capacity() {
        let input = br#"{"resourceSpans":[{"schemaUrl":"v1","scopeSpans":[{"spans":[{"traceId":"00112233445566778899aabbccddeeff","spanId":"0011223344556677","name":"work","kind":2,"startTimeUnixNano":"1","endTimeUnixNano":2,"unknown":{"nested":[1]}}]}]}]}"#;
        let plan = crate::otlp_json::preflight_trace_json(input, test_limits())
            .expect("trace JSON preflights");
        let request = decode_trace_json(input, plan.decode_bytes).expect("trace JSON decodes");
        assert_eq!(
            request.resource_spans.len(),
            request.resource_spans.capacity()
        );
        let resource = &request.resource_spans[0];
        assert_eq!(resource.schema_url.len(), resource.schema_url.capacity());
        assert_eq!(resource.scope_spans.len(), resource.scope_spans.capacity());
        let span = &resource.scope_spans[0].spans[0];
        assert_eq!(span.name.len(), span.name.capacity());
        assert_eq!(span.trace_id.len(), span.trace_id.capacity());
        assert_eq!(span.span_id.len(), span.span_id.capacity());
    }

    /// Proves ordinary generated fields reject duplicates.
    #[test]
    fn trace_json_rejects_duplicate_generated_fields() {
        let input = br#"{"resourceSpans":[],"resourceSpans":[]}"#;
        assert!(crate::otlp_json::preflight_trace_json(input, test_limits()).is_err());
    }

    /// Proves trace resource, scope, record, attribute, and value-byte caps reject cap plus one.
    #[test]
    fn trace_json_enforces_signal_preflight_caps() {
        for (field, accepted, refused) in [
            (
                "resources",
                br#"{"resourceSpans":[{}]}"#.as_slice(),
                br#"{"resourceSpans":[{},{}]}"#.as_slice(),
            ),
            (
                "scopes",
                br#"{"resourceSpans":[{"scopeSpans":[{}]}]}"#.as_slice(),
                br#"{"resourceSpans":[{"scopeSpans":[{},{}]}]}"#.as_slice(),
            ),
            (
                "records",
                br#"{"resourceSpans":[{"scopeSpans":[{"spans":[{}]}]}]}"#.as_slice(),
                br#"{"resourceSpans":[{"scopeSpans":[{"spans":[{},{}]}]}]}"#.as_slice(),
            ),
            (
                "attributes",
                br#"{"resourceSpans":[{"resource":{"attributes":[{}]}}]}"#.as_slice(),
                br#"{"resourceSpans":[{"resource":{"attributes":[{},{}]}}]}"#.as_slice(),
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
            assert!(crate::otlp_json::preflight_trace_json(accepted, limits).is_ok());
            assert!(crate::otlp_json::preflight_trace_json(refused, limits).is_err());
        }

        let mut limits = test_limits();
        limits.value_bytes = 1;
        assert!(
            crate::otlp_json::preflight_trace_json(
                br#"{"resourceSpans":[{"schemaUrl":"a"}]}"#,
                limits,
            )
            .is_ok()
        );
        assert!(
            crate::otlp_json::preflight_trace_json(
                br#"{"resourceSpans":[{"schemaUrl":"ab"}]}"#,
                limits,
            )
            .is_err()
        );
    }

    /// Proves trace attributes accept value depth eight and reject depth nine.
    #[test]
    fn trace_json_enforces_any_value_depth_eight() {
        for (depth, accepted) in [(8, true), (9, false)] {
            let input = nested_trace_attribute_request(depth);
            let result = crate::otlp_json::preflight_trace_json(input.as_bytes(), test_limits());
            assert_eq!(result.is_ok(), accepted, "depth {depth}");
        }
    }

    /// Proves escaped retained strings and hexadecimal IDs preserve the exact owner plan.
    #[test]
    fn trace_json_decodes_escaped_tokens_with_exact_owner() {
        let input = br#"{"resourceSpans":[{"scopeSpans":[{"spans":[{"traceId":"00112233445566778899aabbccddeef\u0066","spanId":"001122334455667\u0037","name":"w\u006frk"}]}]}]}"#;
        let plan = crate::otlp_json::preflight_trace_json(input, test_limits())
            .expect("escaped trace JSON preflights");
        let request =
            decode_trace_json(input, plan.decode_bytes).expect("escaped trace JSON decodes");
        let span = &request.resource_spans[0].scope_spans[0].spans[0];
        assert_eq!(span.name, "work");
        assert_eq!(span.name.len(), span.name.capacity());
        assert_eq!(span.trace_id.len(), span.trace_id.capacity());
        assert_eq!(span.span_id.len(), span.span_id.capacity());
    }

    /// Builds a trace request containing exactly `depth` nested value messages.
    fn nested_trace_attribute_request(depth: usize) -> String {
        let mut value = String::new();
        for _ in 1..depth {
            value.push_str(r#"{"arrayValue":{"values":["#);
        }
        value.push_str(r#"{"intValue":"1"}"#);
        for _ in 1..depth {
            value.push_str("]}}");
        }
        format!(
            r#"{{"resourceSpans":[{{"scopeSpans":[{{"spans":[{{"attributes":[{{"value":{value}}}]}}]}}]}}]}}"#
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
            time_partitions: 32,
        }
    }
}
