//! Allocation-free OTLP wire preflight for server-owned decode adapters.

use std::mem::size_of;

use vala_bifrost_redux::gate::{IngestError, OtlpWireLimits};
use wyrd_tonic::otlp::common::v1::{AnyValue, EntityRef, KeyValue};
use wyrd_tonic::otlp::resource::v1::Resource;
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span};
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;

/// Exact adapter facts established without constructing generated messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OtlpDecodePlan {
    /// Encoded protobuf bytes inspected by the preflight.
    pub(crate) wire_bytes: usize,
    /// Generated request storage admitted before prost construction.
    pub(crate) decode_bytes: usize,
}

/// Bounded counters used while walking one trace export.
#[derive(Debug)]
struct TraceWireFacts {
    /// Shared immutable limits selected by the Scribe runtime.
    limits: OtlpWireLimits,
    /// Number of resource groups encountered.
    resources: usize,
    /// Number of scope groups encountered.
    scopes: usize,
    /// Number of spans encountered.
    records: usize,
    /// Number of attribute entries encountered.
    attributes: usize,
    /// Cumulative key, value, body, and identifier bytes.
    value_bytes: usize,
    /// Public-layout and exact backing capacity required by the typed request.
    decode_bytes: usize,
}

impl TraceWireFacts {
    /// Creates the fixed counter set with the root request allocation included.
    fn new(limits: OtlpWireLimits) -> Self {
        Self {
            limits,
            resources: 0,
            scopes: 0,
            records: 0,
            attributes: 0,
            value_bytes: 0,
            decode_bytes: size_of::<ExportTraceServiceRequest>(),
        }
    }

    /// Adds one bounded cardinality fact.
    ///
    /// # Errors
    ///
    /// Returns the stable malformed-request error on arithmetic overflow or
    /// when `limit` would be exceeded.
    fn add_count(current: &mut usize, amount: usize, limit: usize) -> Result<(), IngestError> {
        *current = current
            .checked_add(amount)
            .ok_or_else(|| malformed("OTLP protobuf cardinality overflow"))?;
        if *current > limit {
            return Err(malformed("OTLP protobuf cardinality limit exceeded"));
        }
        Ok(())
    }

    /// Adds exact generated backing bytes to both value and decode facts.
    ///
    /// # Errors
    ///
    /// Returns the stable malformed-request error on overflow or a configured
    /// value/decode ceiling breach.
    fn add_value_bytes(&mut self, amount: usize) -> Result<(), IngestError> {
        Self::add_count(&mut self.value_bytes, amount, self.limits.value_bytes)?;
        self.add_decode_bytes(amount)
    }

    /// Adds public-layout or backing capacity to the typed decode owner.
    ///
    /// # Errors
    ///
    /// Returns the stable malformed-request error on overflow or when the
    /// immutable material ceiling would be exceeded.
    fn add_decode_bytes(&mut self, amount: usize) -> Result<(), IngestError> {
        Self::add_count(&mut self.decode_bytes, amount, self.limits.material_bytes)
    }

    /// Records one repeated attribute element before visiting its payload.
    ///
    /// # Errors
    ///
    /// Returns a stable refusal when attribute or decode capacity is exceeded.
    fn add_attribute(&mut self) -> Result<(), IngestError> {
        Self::add_count(&mut self.attributes, 1, self.limits.attributes)?;
        self.add_decode_bytes(size_of::<KeyValue>())
    }
}

/// One parsed protobuf field borrowing its original wire storage.
#[derive(Clone, Copy, Debug)]
enum WireValue<'a> {
    /// Varint field value.
    Varint,
    /// Fixed-width 64-bit value.
    Fixed64(u64),
    /// Length-delimited field payload.
    Bytes(&'a [u8]),
    /// Fixed-width 32-bit value.
    Fixed32,
}

/// Non-materializing protobuf cursor over one message body.
#[derive(Debug)]
struct WireFields<'a> {
    /// Complete borrowed message bytes.
    bytes: &'a [u8],
    /// Next unread byte offset.
    cursor: usize,
}

impl<'a> WireFields<'a> {
    /// Creates a cursor without allocating or copying the message.
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    /// Reads the next field while validating protobuf framing.
    ///
    /// # Errors
    ///
    /// Returns the stable malformed-request error for invalid tags, wire
    /// types, varints, lengths, or truncated fixed-width values.
    fn next(&mut self) -> Result<Option<(u32, WireValue<'a>)>, IngestError> {
        if self.cursor == self.bytes.len() {
            return Ok(None);
        }
        let key = self.read_varint()?;
        let tag = u32::try_from(key >> 3).map_err(|_| malformed("invalid protobuf tag"))?;
        if tag == 0 {
            return Err(malformed("protobuf tag zero is invalid"));
        }
        let value = match key & 7 {
            0 => {
                self.read_varint()?;
                WireValue::Varint
            }
            1 => WireValue::Fixed64(self.read_fixed_u64()?),
            2 => {
                let length = usize::try_from(self.read_varint()?)
                    .map_err(|_| malformed("protobuf length does not fit usize"))?;
                let end = self
                    .cursor
                    .checked_add(length)
                    .ok_or_else(|| malformed("protobuf length overflow"))?;
                let value = self
                    .bytes
                    .get(self.cursor..end)
                    .ok_or_else(|| malformed("truncated protobuf field"))?;
                self.cursor = end;
                WireValue::Bytes(value)
            }
            5 => {
                self.read_fixed_u32()?;
                WireValue::Fixed32
            }
            _ => return Err(malformed("unsupported protobuf wire type")),
        };
        Ok(Some((tag, value)))
    }

    /// Reads one canonical protobuf varint from the current cursor.
    ///
    /// # Errors
    ///
    /// Returns the stable malformed-request error for truncation or a value
    /// wider than ten bytes.
    fn read_varint(&mut self) -> Result<u64, IngestError> {
        let mut value = 0_u64;
        for shift in (0..70).step_by(7) {
            let byte = *self
                .bytes
                .get(self.cursor)
                .ok_or_else(|| malformed("truncated protobuf varint"))?;
            self.cursor += 1;
            if shift == 63 && byte > 1 {
                return Err(malformed("protobuf varint overflow"));
            }
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(malformed("protobuf varint exceeds ten bytes"))
    }

    /// Reads one little-endian fixed64 value.
    ///
    /// # Errors
    ///
    /// Returns the stable malformed-request error when eight bytes are absent.
    fn read_fixed_u64(&mut self) -> Result<u64, IngestError> {
        let end = self
            .cursor
            .checked_add(8)
            .ok_or_else(|| malformed("protobuf fixed64 overflow"))?;
        let bytes: [u8; 8] = self
            .bytes
            .get(self.cursor..end)
            .ok_or_else(|| malformed("truncated protobuf fixed64"))?
            .try_into()
            .map_err(|_| malformed("invalid protobuf fixed64"))?;
        self.cursor = end;
        Ok(u64::from_le_bytes(bytes))
    }

    /// Skips one little-endian fixed32 value.
    ///
    /// # Errors
    ///
    /// Returns the stable malformed-request error when four bytes are absent.
    fn read_fixed_u32(&mut self) -> Result<(), IngestError> {
        self.cursor = self
            .cursor
            .checked_add(4)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| malformed("truncated protobuf fixed32"))?;
        Ok(())
    }
}

/// Preflights one OTLP trace protobuf body before generated-type allocation.
///
/// Unknown fields are framing-validated and skipped without allocation. Known
/// trace/resource/scope/span/attribute fields contribute exact public-layout
/// and backing capacities to the returned decode plan.
///
/// # Errors
///
/// Returns the stable malformed-request error for invalid framing, nesting, or
/// configured limit excess.
pub(crate) fn preflight_trace_protobuf(
    bytes: &[u8],
    limits: OtlpWireLimits,
) -> Result<OtlpDecodePlan, IngestError> {
    if bytes.len() > limits.request_bytes {
        return Err(IngestError::PayloadTooLarge {
            bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            limit: u64::try_from(limits.request_bytes).unwrap_or(u64::MAX),
        });
    }
    let mut facts = TraceWireFacts::new(limits);
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if tag == 1 {
            let WireValue::Bytes(resource) = value else {
                return Err(malformed("trace resources must be length-delimited"));
            };
            TraceWireFacts::add_count(&mut facts.resources, 1, limits.resources)?;
            facts.add_decode_bytes(size_of::<ResourceSpans>())?;
            visit_resource_spans(resource, &mut facts)?;
        }
    }
    Ok(OtlpDecodePlan {
        wire_bytes: bytes.len(),
        decode_bytes: facts.decode_bytes,
    })
}

/// Visits one `ResourceSpans` message and its nested scope groups.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid wire shapes or limits.
fn visit_resource_spans(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<(), IngestError> {
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(resource)) => visit_resource(resource, facts)?,
            (2, WireValue::Bytes(scope)) => {
                TraceWireFacts::add_count(&mut facts.scopes, 1, facts.limits.scopes)?;
                facts.add_decode_bytes(size_of::<ScopeSpans>())?;
                visit_scope_spans(scope, facts)?;
            }
            (3, WireValue::Bytes(schema_url)) => facts.add_value_bytes(schema_url.len())?,
            _ => {}
        }
    }
    Ok(())
}

/// Visits one OTLP resource and counts attribute/entity backing storage.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid nested fields or limits.
fn visit_resource(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<(), IngestError> {
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(attribute)) => visit_attribute(attribute, facts)?,
            (3, WireValue::Bytes(entity)) => {
                facts.add_decode_bytes(size_of::<EntityRef>())?;
                visit_flat_message_bytes(entity, facts)?;
            }
            _ => {}
        }
    }
    let _ = size_of::<Resource>();
    Ok(())
}

/// Visits one instrumentation scope and its repeated attributes.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid nested fields or limits.
fn visit_scope(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<(), IngestError> {
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1 | 2, WireValue::Bytes(text)) => facts.add_value_bytes(text.len())?,
            (3, WireValue::Bytes(attribute)) => visit_attribute(attribute, facts)?,
            _ => {}
        }
    }
    Ok(())
}

/// Visits one `ScopeSpans` message and its repeated spans.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid nested fields or limits.
fn visit_scope_spans(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<(), IngestError> {
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(scope)) => visit_scope(scope, facts)?,
            (2, WireValue::Bytes(span)) => {
                TraceWireFacts::add_count(&mut facts.records, 1, facts.limits.records)?;
                facts.add_decode_bytes(size_of::<Span>())?;
                visit_span(span, facts)?;
            }
            (3, WireValue::Bytes(schema_url)) => facts.add_value_bytes(schema_url.len())?,
            _ => {}
        }
    }
    Ok(())
}

/// Visits one span, including events, links, and attributes.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid nested fields or limits.
fn visit_span(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<(), IngestError> {
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1 | 2 | 3 | 4 | 5, WireValue::Bytes(value)) => {
                facts.add_value_bytes(value.len())?;
            }
            (7, WireValue::Fixed64(_event_time)) => {}
            (9, WireValue::Bytes(attribute)) => visit_attribute(attribute, facts)?,
            (11, WireValue::Bytes(event)) => {
                facts.add_decode_bytes(size_of::<wyrd_tonic::otlp::trace::v1::span::Event>())?;
                visit_span_event(event, facts)?;
            }
            (13, WireValue::Bytes(link)) => {
                facts.add_decode_bytes(size_of::<wyrd_tonic::otlp::trace::v1::span::Link>())?;
                visit_span_link(link, facts)?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Visits one span event and its repeated attributes.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid nested fields or limits.
fn visit_span_event(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<(), IngestError> {
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (2, WireValue::Bytes(name)) => facts.add_value_bytes(name.len())?,
            (3, WireValue::Bytes(attribute)) => visit_attribute(attribute, facts)?,
            _ => {}
        }
    }
    Ok(())
}

/// Visits one span link and its identifier, trace-state, and attributes.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid nested fields or limits.
fn visit_span_link(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<(), IngestError> {
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1 | 2 | 3, WireValue::Bytes(value)) => facts.add_value_bytes(value.len())?,
            (4, WireValue::Bytes(attribute)) => visit_attribute(attribute, facts)?,
            _ => {}
        }
    }
    Ok(())
}

/// Visits one key/value attribute and its optional recursive value.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid nested fields or limits.
fn visit_attribute(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<(), IngestError> {
    facts.add_attribute()?;
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(key)) => facts.add_value_bytes(key.len())?,
            (2, WireValue::Bytes(value)) => visit_any_value(value, 1, facts)?,
            _ => {}
        }
    }
    Ok(())
}

/// Visits one recursive OTLP `AnyValue` without constructing it.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid nested fields, excess
/// depth, or configured capacity excess.
fn visit_any_value(
    bytes: &[u8],
    depth: usize,
    facts: &mut TraceWireFacts,
) -> Result<(), IngestError> {
    if depth > facts.limits.value_depth {
        return Err(malformed("OTLP value nesting depth exceeded"));
    }
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1 | 7, WireValue::Bytes(value)) => facts.add_value_bytes(value.len())?,
            (5, WireValue::Bytes(array)) => {
                let mut values = WireFields::new(array);
                while let Some((value_tag, value)) = values.next()? {
                    if let (1, WireValue::Bytes(value)) = (value_tag, value) {
                        facts.add_decode_bytes(size_of::<AnyValue>())?;
                        visit_any_value(value, depth + 1, facts)?;
                    }
                }
            }
            (6, WireValue::Bytes(list)) => {
                let mut values = WireFields::new(list);
                while let Some((value_tag, value)) = values.next()? {
                    if let (1, WireValue::Bytes(value)) = (value_tag, value) {
                        visit_attribute(value, facts)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Counts string/bytes fields in a bounded flat generated message.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid framing or limits.
fn visit_flat_message_bytes(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<(), IngestError> {
    let mut fields = WireFields::new(bytes);
    while let Some((_tag, value)) = fields.next()? {
        if let WireValue::Bytes(value) = value {
            facts.add_value_bytes(value.len())?;
        }
    }
    Ok(())
}

/// Constructs the existing stable malformed OTLP refusal.
fn malformed(message: &str) -> IngestError {
    IngestError::Decode(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span};
    use wyrd_tonic::prost::Message;

    /// Proves trace preflight derives typed capacity without decoding.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot encode or the preflight rejects it.
    #[test]
    fn trace_preflight_counts_generated_capacity() {
        let request = ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                scope_spans: vec![ScopeSpans {
                    spans: vec![Span {
                        trace_id: vec![1; 16],
                        span_id: vec![2; 8],
                        name: "bounded".to_owned(),
                        ..Span::default()
                    }],
                    ..ScopeSpans::default()
                }],
                ..ResourceSpans::default()
            }],
        };
        let bytes = request.encode_to_vec();
        let plan =
            preflight_trace_protobuf(&bytes, vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS)
                .expect("valid trace wire preflight");
        assert_eq!(plan.wire_bytes, bytes.len());
        assert!(plan.decode_bytes >= size_of::<ExportTraceServiceRequest>());
        assert!(plan.decode_bytes <= plan.wire_bytes + 1024);
    }

    /// Proves malformed length-delimited fields fail before prost construction.
    #[test]
    fn trace_preflight_rejects_truncated_nested_message() {
        let error = preflight_trace_protobuf(
            &[0x0a, 0x04, 0x12],
            vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS,
        )
        .expect_err("truncated resource must fail");
        assert!(matches!(error, IngestError::Decode(_)));
    }
}
