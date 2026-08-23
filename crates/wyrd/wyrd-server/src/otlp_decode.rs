//! Allocation-free OTLP wire preflight for server-owned decode adapters.

use std::mem::size_of;

use vala_bifrost_redux::gate::{IngestError, OtlpWireLimits};
use wyrd_tonic::otlp::common::v1::{
    AnyValue, ArrayValue, EntityRef, InstrumentationScope, KeyValue, KeyValueList, any_value,
};
use wyrd_tonic::otlp::resource::v1::Resource;
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span, Status, span};
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;

/// Maximum deprecated protobuf group nesting accepted by the bounded scanner.
const MAX_WIRE_GROUP_DEPTH: usize = 8;

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
        Self::add_count(&mut self.decode_bytes, amount, usize::MAX)
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
    Varint(u64),
    /// Fixed-width 64-bit value.
    Fixed64(u64),
    /// Length-delimited field payload.
    Bytes(&'a [u8]),
    /// Fixed-width 32-bit value.
    Fixed32(u32),
    /// Deprecated group field whose contents were framing-validated and skipped.
    Group,
}

/// Non-materializing protobuf cursor over one message body.
#[derive(Debug)]
struct WireFields<'a> {
    /// Complete borrowed message bytes.
    bytes: &'a [u8],
    /// Next unread byte offset.
    cursor: usize,
    /// Maximum deprecated-group nesting accepted by this cursor.
    group_depth_limit: usize,
}

impl<'a> WireFields<'a> {
    /// Creates a cursor without allocating or copying the message.
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            cursor: 0,
            group_depth_limit: MAX_WIRE_GROUP_DEPTH,
        }
    }

    /// Creates a cursor using an operator-lowered group-depth ceiling.
    fn with_group_depth(bytes: &'a [u8], group_depth_limit: usize) -> Self {
        Self {
            bytes,
            cursor: 0,
            group_depth_limit: group_depth_limit.min(MAX_WIRE_GROUP_DEPTH),
        }
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
            0 => WireValue::Varint(self.read_varint()?),
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
            3 => {
                self.skip_group(tag)?;
                WireValue::Group
            }
            5 => WireValue::Fixed32(self.read_fixed_u32()?),
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
    fn read_fixed_u32(&mut self) -> Result<u32, IngestError> {
        let end = self
            .cursor
            .checked_add(4)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| malformed("truncated protobuf fixed32"))?;
        let bytes: [u8; 4] = self
            .bytes
            .get(self.cursor..end)
            .ok_or_else(|| malformed("truncated protobuf fixed32"))?
            .try_into()
            .map_err(|_| malformed("invalid protobuf fixed32"))?;
        self.cursor = end;
        Ok(u32::from_le_bytes(bytes))
    }

    /// Skips one unknown protobuf group, including nested groups.
    ///
    /// # Errors
    ///
    /// Returns the stable malformed-request error for invalid framing,
    /// truncation, or a mismatched end-group tag.
    fn skip_group(&mut self, group_tag: u32) -> Result<(), IngestError> {
        if self.group_depth_limit == 0 {
            return Err(malformed("protobuf group nesting depth exceeded"));
        }
        let mut tags = [0_u32; MAX_WIRE_GROUP_DEPTH];
        tags[0] = group_tag;
        let mut depth = 1_usize;
        loop {
            let key = self.read_varint()?;
            let tag = u32::try_from(key >> 3).map_err(|_| malformed("invalid protobuf tag"))?;
            if tag == 0 {
                return Err(malformed("protobuf tag zero is invalid"));
            }
            let wire_type = key & 7;
            if wire_type == 4 {
                let index = depth
                    .checked_sub(1)
                    .ok_or_else(|| malformed("protobuf group depth underflow"))?;
                let expected = tags
                    .get(index)
                    .ok_or_else(|| malformed("protobuf group depth overflow"))?;
                if *expected != tag {
                    return Err(malformed("mismatched protobuf end-group tag"));
                }
                depth -= 1;
                if depth == 0 {
                    return Ok(());
                }
            } else if wire_type == 3 {
                if depth == self.group_depth_limit {
                    return Err(malformed("protobuf group nesting depth exceeded"));
                }
                let slot = tags
                    .get_mut(depth)
                    .ok_or_else(|| malformed("protobuf group nesting depth exceeded"))?;
                *slot = tag;
                depth += 1;
            } else {
                self.skip_value(wire_type)?;
            }
        }
    }

    /// Skips one already-keyed protobuf value while validating its framing.
    ///
    /// # Errors
    ///
    /// Returns the stable malformed-request error for invalid wire types,
    /// lengths, varints, groups, or truncated fixed-width values.
    fn skip_value(&mut self, wire_type: u64) -> Result<(), IngestError> {
        match wire_type {
            0 => {
                self.read_varint()?;
            }
            1 => {
                self.read_fixed_u64()?;
            }
            2 => {
                let length = usize::try_from(self.read_varint()?)
                    .map_err(|_| malformed("protobuf length does not fit usize"))?;
                self.cursor = self
                    .cursor
                    .checked_add(length)
                    .filter(|end| *end <= self.bytes.len())
                    .ok_or_else(|| malformed("truncated protobuf field"))?;
            }
            5 => {
                self.read_fixed_u32()?;
            }
            _ => return Err(malformed("unsupported protobuf wire type")),
        }
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
    let mut fields = WireFields::with_group_depth(bytes, limits.value_depth);
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
    if let Some(schema_url) = last_bytes_field(bytes, 3)? {
        facts.add_value_bytes(schema_url.len())?;
    }
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(resource)) => visit_resource(resource, facts)?,
            (2, WireValue::Bytes(scope)) => {
                TraceWireFacts::add_count(&mut facts.scopes, 1, facts.limits.scopes)?;
                facts.add_decode_bytes(size_of::<ScopeSpans>())?;
                visit_scope_spans(scope, facts)?;
            }
            (3, WireValue::Bytes(_)) => {}
            (1..=3, _) => return Err(wrong_wire("resource spans")),
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
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
            (3, WireValue::Bytes(entity)) => {
                facts.add_decode_bytes(size_of::<EntityRef>())?;
                visit_entity_ref(entity, facts)?;
            }
            (2, WireValue::Varint(_)) => {}
            (1..=3, _) => return Err(wrong_wire("resource")),
            _ => {}
        }
    }
    Ok(())
}

/// Visits one instrumentation scope and its repeated attributes.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid nested fields or limits.
fn visit_scope(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<(), IngestError> {
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1 | 2, WireValue::Bytes(_)) => {}
            (3, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
            (4, WireValue::Varint(_)) => {}
            (1..=4, _) => return Err(wrong_wire("instrumentation scope")),
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
    for tag in [1, 2] {
        if let Some(text) = last_bytes_across_messages(bytes, 1, tag)? {
            facts.add_value_bytes(text.len())?;
        }
    }
    if let Some(schema_url) = last_bytes_field(bytes, 3)? {
        facts.add_value_bytes(schema_url.len())?;
    }
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(scope)) => visit_scope(scope, facts)?,
            (2, WireValue::Bytes(span)) => {
                TraceWireFacts::add_count(&mut facts.records, 1, facts.limits.records)?;
                facts.add_decode_bytes(size_of::<Span>())?;
                visit_span(span, facts)?;
            }
            (3, WireValue::Bytes(_)) => {}
            (1..=3, _) => return Err(wrong_wire("scope spans")),
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
    for tag in 1..=5 {
        if let Some(value) = last_bytes_field(bytes, tag)? {
            facts.add_value_bytes(value.len())?;
        }
    }
    if let Some(message) = last_bytes_across_messages(bytes, 15, 2)? {
        facts.add_value_bytes(message.len())?;
    }
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1..=5, WireValue::Bytes(_)) => {}
            (6 | 10 | 12 | 14, WireValue::Varint(_)) => {}
            (7 | 8, WireValue::Fixed64(_)) => {}
            (9, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
            (11, WireValue::Bytes(event)) => {
                facts.add_decode_bytes(size_of::<wyrd_tonic::otlp::trace::v1::span::Event>())?;
                visit_span_event(event, facts)?;
            }
            (13, WireValue::Bytes(link)) => {
                facts.add_decode_bytes(size_of::<wyrd_tonic::otlp::trace::v1::span::Link>())?;
                visit_span_link(link, facts)?;
            }
            (15, WireValue::Bytes(status)) => visit_status(status, facts)?,
            (16, WireValue::Fixed32(_)) => {}
            (1..=16, _) => return Err(wrong_wire("span")),
            _ => {}
        }
    }
    Ok(())
}

/// Visits one entity reference and accounts for repeated string elements.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid framing or limits.
fn visit_entity_ref(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<(), IngestError> {
    for tag in [1, 2] {
        if let Some(value) = last_bytes_field(bytes, tag)? {
            facts.add_value_bytes(value.len())?;
        }
    }
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1 | 2, WireValue::Bytes(_)) => {}
            (3 | 4, WireValue::Bytes(value)) => {
                facts.add_decode_bytes(size_of::<String>())?;
                facts.add_value_bytes(value.len())?;
            }
            (1..=4, _) => return Err(wrong_wire("entity reference")),
            _ => {}
        }
    }
    Ok(())
}

/// Visits one span status and accounts for its message backing storage.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid framing or limits.
fn visit_status(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<(), IngestError> {
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (2, WireValue::Bytes(_)) => {}
            (3, WireValue::Varint(_)) => {}
            (2 | 3, _) => return Err(wrong_wire("span status")),
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
    if let Some(name) = last_bytes_field(bytes, 2)? {
        facts.add_value_bytes(name.len())?;
    }
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (2, WireValue::Bytes(_)) => {}
            (3, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
            (1, WireValue::Fixed64(_)) | (4, WireValue::Varint(_)) => {}
            (1..=4, _) => return Err(wrong_wire("span event")),
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
    for tag in 1..=3 {
        if let Some(value) = last_bytes_field(bytes, tag)? {
            facts.add_value_bytes(value.len())?;
        }
    }
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1..=3, WireValue::Bytes(_)) => {}
            (4, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
            (5, WireValue::Varint(_)) | (6, WireValue::Fixed32(_)) => {}
            (1..=6, _) => return Err(wrong_wire("span link")),
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
fn visit_attribute(
    bytes: &[u8],
    value_depth: usize,
    facts: &mut TraceWireFacts,
) -> Result<(), IngestError> {
    facts.add_attribute()?;
    if let Some(key) = last_bytes_field(bytes, 1)? {
        facts.add_value_bytes(key.len())?;
    }
    visit_merged_any_value(
        AnyValueBodies::Repeated {
            parent: bytes,
            tag: 2,
        },
        value_depth + 1,
        facts,
    )?;
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1 | 2, WireValue::Bytes(_)) => {}
            (1 | 2, _) => return Err(wrong_wire("key/value")),
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
    visit_merged_any_value(AnyValueBodies::Single(bytes), depth, facts)
}

/// Visits only the retained final oneof run across merged `AnyValue` bodies.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid nested fields, excess
/// depth, or configured capacity excess.
fn visit_merged_any_value(
    bodies: AnyValueBodies<'_>,
    depth: usize,
    facts: &mut TraceWireFacts,
) -> Result<(), IngestError> {
    if depth > facts.limits.value_depth {
        return Err(malformed("OTLP value nesting depth exceeded"));
    }
    let selection = select_any_value_bodies(bodies)?;
    let mut ordinal = 0_usize;
    bodies.try_for_each(|body| {
        let mut fields = WireFields::with_group_depth(body, facts.limits.value_depth);
        while let Some((tag, value)) = fields.next()? {
            if !(1..=7).contains(&tag) {
                continue;
            }
            ordinal = ordinal
                .checked_add(1)
                .ok_or_else(|| malformed("OTLP oneof occurrence overflow"))?;
            if ordinal < selection.run_start || tag != selection.tag {
                continue;
            }
            match (tag, value) {
                (1 | 7, WireValue::Bytes(value)) => facts.add_value_bytes(value.len())?,
                (2 | 3, WireValue::Varint(_)) | (4, WireValue::Fixed64(_)) => {}
                (5, WireValue::Bytes(array)) => {
                    let mut values = WireFields::with_group_depth(array, facts.limits.value_depth);
                    while let Some((value_tag, value)) = values.next()? {
                        if let (1, WireValue::Bytes(value)) = (value_tag, value) {
                            facts.add_decode_bytes(size_of::<AnyValue>())?;
                            visit_any_value(value, depth + 1, facts)?;
                        } else if value_tag == 1 {
                            return Err(wrong_wire("OTLP value array"));
                        }
                    }
                }
                (6, WireValue::Bytes(list)) => {
                    let mut values = WireFields::with_group_depth(list, facts.limits.value_depth);
                    while let Some((value_tag, value)) = values.next()? {
                        if let (1, WireValue::Bytes(value)) = (value_tag, value) {
                            visit_attribute(value, depth, facts)?;
                        } else if value_tag == 1 {
                            return Err(wrong_wire("OTLP key/value list"));
                        }
                    }
                }
                _ => return Err(wrong_wire("OTLP any value")),
            }
        }
        Ok(())
    })?;
    Ok(())
}

/// Constructs one trace export using capacities derived from the preflighted wire.
///
/// Every repeated field is counted before its owning generated message is
/// created. Strings and byte fields copy into exact-capacity backing storage,
/// so the second pass cannot grow a generated collection after admission.
///
/// # Errors
///
/// Returns the stable malformed-request error when a known field has the wrong
/// wire type, text is not UTF-8, framing is invalid, or a count overflows.
pub(crate) fn decode_trace_protobuf(
    bytes: &[u8],
) -> Result<ExportTraceServiceRequest, IngestError> {
    let resource_count = repeated_message_count(bytes, 1)?;
    let mut resource_spans = Vec::with_capacity(resource_count);
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(value)) => resource_spans.push(decode_resource_spans(value)?),
            (1, _) => return Err(wrong_wire("trace resource spans")),
            _ => {}
        }
    }
    Ok(ExportTraceServiceRequest { resource_spans })
}

/// Counts repeated length-delimited messages without retaining descriptors.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, a wrong known wire
/// type, or cardinality overflow.
fn repeated_message_count(bytes: &[u8], wanted_tag: u32) -> Result<usize, IngestError> {
    let mut count = 0_usize;
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if tag == wanted_tag {
            if !matches!(value, WireValue::Bytes(_)) {
                return Err(wrong_wire("repeated OTLP message"));
            }
            count = count
                .checked_add(1)
                .ok_or_else(|| malformed("OTLP repeated-field count overflow"))?;
        }
    }
    Ok(count)
}

/// Returns the final retained bytes occurrence for one singular field.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or a wrong known wire
/// type.
fn last_bytes_field(bytes: &[u8], wanted_tag: u32) -> Result<Option<&[u8]>, IngestError> {
    let mut retained = None;
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if tag == wanted_tag {
            let WireValue::Bytes(value) = value else {
                return Err(wrong_wire("singular bytes field"));
            };
            retained = Some(value);
        }
    }
    Ok(retained)
}

/// Returns the final retained child bytes across merged parent messages.
///
/// # Errors
///
/// Returns a malformed-request error for invalid parent/child framing or wire
/// types.
fn last_bytes_across_messages(
    bytes: &[u8],
    parent_tag: u32,
    child_tag: u32,
) -> Result<Option<&[u8]>, IngestError> {
    let mut retained = None;
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if tag == parent_tag {
            let WireValue::Bytes(body) = value else {
                return Err(wrong_wire("optional OTLP message"));
            };
            if let Some(value) = last_bytes_field(body, child_tag)? {
                retained = Some(value);
            }
        }
    }
    Ok(retained)
}

/// Counts inner repeated messages across every occurrence of one parent field.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, a wrong parent wire
/// type, or count overflow.
fn repeated_count_across_messages(
    bytes: &[u8],
    parent_tag: u32,
    child_tag: u32,
) -> Result<(usize, usize), IngestError> {
    let mut occurrences = 0_usize;
    let mut children = 0_usize;
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if tag == parent_tag {
            let WireValue::Bytes(value) = value else {
                return Err(wrong_wire("optional OTLP message"));
            };
            occurrences = occurrences
                .checked_add(1)
                .ok_or_else(|| malformed("OTLP message occurrence overflow"))?;
            children = children
                .checked_add(repeated_message_count(value, child_tag)?)
                .ok_or_else(|| malformed("OTLP repeated-field count overflow"))?;
        }
    }
    Ok((occurrences, children))
}

/// Copies UTF-8 bytes into a string whose capacity is exactly the wire length.
///
/// # Errors
///
/// Returns a malformed-request error when the bytes are not valid UTF-8.
fn fixed_string(bytes: &[u8]) -> Result<String, IngestError> {
    let text = std::str::from_utf8(bytes).map_err(|_| malformed("invalid protobuf string"))?;
    let mut value = String::with_capacity(bytes.len());
    value.push_str(text);
    Ok(value)
}

/// Copies bytes into a vector whose capacity is exactly the wire length.
fn fixed_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut value = Vec::with_capacity(bytes.len());
    value.extend_from_slice(bytes);
    value
}

/// Applies protobuf's low-32-bit unsigned varint conversion without a cast.
fn protobuf_u32(value: u64) -> u32 {
    let bytes = value.to_le_bytes();
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Applies protobuf's low-32-bit signed varint conversion without a cast.
fn protobuf_i32(value: u64) -> i32 {
    let bytes = value.to_le_bytes();
    i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Reinterprets one protobuf signed varint's complete two's-complement bits.
fn protobuf_i64(value: u64) -> i64 {
    i64::from_le_bytes(value.to_le_bytes())
}

/// Constructs one resource group and all of its scope groups.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or known wire types.
fn decode_resource_spans(bytes: &[u8]) -> Result<ResourceSpans, IngestError> {
    let resource = decode_merged_resource(bytes, 1)?;
    let scope_count = repeated_message_count(bytes, 2)?;
    let mut scope_spans = Vec::with_capacity(scope_count);
    let schema_url = last_bytes_field(bytes, 3)?
        .map(fixed_string)
        .transpose()?
        .unwrap_or_default();
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(_)) => {}
            (2, WireValue::Bytes(value)) => scope_spans.push(decode_scope_spans(value)?),
            (3, WireValue::Bytes(_)) => {}
            (1..=3, _) => return Err(wrong_wire("resource spans")),
            _ => {}
        }
    }
    Ok(ResourceSpans {
        resource,
        scope_spans,
        schema_url,
    })
}

/// Merges every occurrence of one singular resource message into one value.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or known wire types.
fn decode_merged_resource(bytes: &[u8], wanted_tag: u32) -> Result<Option<Resource>, IngestError> {
    let (occurrences, attribute_count) = repeated_count_across_messages(bytes, wanted_tag, 1)?;
    if occurrences == 0 {
        return Ok(None);
    }
    let (_, entity_count) = repeated_count_across_messages(bytes, wanted_tag, 3)?;
    let mut resource = Resource {
        attributes: Vec::with_capacity(attribute_count),
        entity_refs: Vec::with_capacity(entity_count),
        ..Resource::default()
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if tag == wanted_tag {
            let WireValue::Bytes(value) = value else {
                return Err(wrong_wire("resource"));
            };
            merge_resource(value, &mut resource)?;
        }
    }
    Ok(Some(resource))
}

/// Merges one encoded resource occurrence into its fixed-capacity destination.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or known wire types.
fn merge_resource(bytes: &[u8], resource: &mut Resource) -> Result<(), IngestError> {
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(value)) => resource.attributes.push(decode_key_value(value)?),
            (2, WireValue::Varint(value)) => {
                resource.dropped_attributes_count = protobuf_u32(value);
            }
            (3, WireValue::Bytes(value)) => resource.entity_refs.push(decode_entity_ref(value)?),
            (1..=3, _) => return Err(wrong_wire("resource")),
            _ => {}
        }
    }
    Ok(())
}

/// Constructs one entity reference, including its repeated identifying keys.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_entity_ref(bytes: &[u8]) -> Result<EntityRef, IngestError> {
    let id_count = repeated_message_count(bytes, 3)?;
    let description_count = repeated_message_count(bytes, 4)?;
    let schema_url = last_bytes_field(bytes, 1)?
        .map(fixed_string)
        .transpose()?
        .unwrap_or_default();
    let r#type = last_bytes_field(bytes, 2)?
        .map(fixed_string)
        .transpose()?
        .unwrap_or_default();
    let mut id_keys = Vec::with_capacity(id_count);
    let mut description_keys = Vec::with_capacity(description_count);
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1 | 2, WireValue::Bytes(_)) => {}
            (3, WireValue::Bytes(value)) => id_keys.push(fixed_string(value)?),
            (4, WireValue::Bytes(value)) => description_keys.push(fixed_string(value)?),
            (1..=4, _) => return Err(wrong_wire("entity reference")),
            _ => {}
        }
    }
    Ok(EntityRef {
        schema_url,
        r#type,
        id_keys,
        description_keys,
    })
}

/// Constructs one scope group and its exact-capacity span vector.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or known wire types.
fn decode_scope_spans(bytes: &[u8]) -> Result<ScopeSpans, IngestError> {
    let scope = decode_merged_scope(bytes, 1)?;
    let span_count = repeated_message_count(bytes, 2)?;
    let mut spans = Vec::with_capacity(span_count);
    let schema_url = last_bytes_field(bytes, 3)?
        .map(fixed_string)
        .transpose()?
        .unwrap_or_default();
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(_)) => {}
            (2, WireValue::Bytes(value)) => spans.push(decode_span(value)?),
            (3, WireValue::Bytes(_)) => {}
            (1..=3, _) => return Err(wrong_wire("scope spans")),
            _ => {}
        }
    }
    Ok(ScopeSpans {
        scope,
        spans,
        schema_url,
    })
}

/// Merges every occurrence of one singular instrumentation scope.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_merged_scope(
    bytes: &[u8],
    wanted_tag: u32,
) -> Result<Option<InstrumentationScope>, IngestError> {
    let (occurrences, attribute_count) = repeated_count_across_messages(bytes, wanted_tag, 3)?;
    if occurrences == 0 {
        return Ok(None);
    }
    let mut scope = InstrumentationScope {
        name: last_bytes_across_messages(bytes, wanted_tag, 1)?
            .map(fixed_string)
            .transpose()?
            .unwrap_or_default(),
        version: last_bytes_across_messages(bytes, wanted_tag, 2)?
            .map(fixed_string)
            .transpose()?
            .unwrap_or_default(),
        attributes: Vec::with_capacity(attribute_count),
        ..InstrumentationScope::default()
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if tag == wanted_tag {
            let WireValue::Bytes(value) = value else {
                return Err(wrong_wire("instrumentation scope"));
            };
            merge_scope(value, &mut scope)?;
        }
    }
    Ok(Some(scope))
}

/// Merges one encoded scope occurrence into its fixed-capacity destination.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn merge_scope(bytes: &[u8], scope: &mut InstrumentationScope) -> Result<(), IngestError> {
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1 | 2, WireValue::Bytes(_)) => {}
            (3, WireValue::Bytes(value)) => scope.attributes.push(decode_key_value(value)?),
            (4, WireValue::Varint(value)) => {
                scope.dropped_attributes_count = protobuf_u32(value);
            }
            (1..=4, _) => return Err(wrong_wire("instrumentation scope")),
            _ => {}
        }
    }
    Ok(())
}

/// Constructs one span including events, links, status, and attributes.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_span(bytes: &[u8]) -> Result<Span, IngestError> {
    let attribute_count = repeated_message_count(bytes, 9)?;
    let event_count = repeated_message_count(bytes, 11)?;
    let link_count = repeated_message_count(bytes, 13)?;
    let status = decode_merged_status(bytes, 15)?;
    let mut span = Span {
        trace_id: last_bytes_field(bytes, 1)?.map_or_else(Vec::new, fixed_bytes),
        span_id: last_bytes_field(bytes, 2)?.map_or_else(Vec::new, fixed_bytes),
        trace_state: last_bytes_field(bytes, 3)?
            .map(fixed_string)
            .transpose()?
            .unwrap_or_default(),
        parent_span_id: last_bytes_field(bytes, 4)?.map_or_else(Vec::new, fixed_bytes),
        name: last_bytes_field(bytes, 5)?
            .map(fixed_string)
            .transpose()?
            .unwrap_or_default(),
        attributes: Vec::with_capacity(attribute_count),
        events: Vec::with_capacity(event_count),
        links: Vec::with_capacity(link_count),
        status,
        ..Span::default()
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1..=5, WireValue::Bytes(_)) => {}
            (6, WireValue::Varint(value)) => span.kind = protobuf_i32(value),
            (7, WireValue::Fixed64(value)) => span.start_time_unix_nano = value,
            (8, WireValue::Fixed64(value)) => span.end_time_unix_nano = value,
            (9, WireValue::Bytes(value)) => span.attributes.push(decode_key_value(value)?),
            (10, WireValue::Varint(value)) => {
                span.dropped_attributes_count = protobuf_u32(value);
            }
            (11, WireValue::Bytes(value)) => span.events.push(decode_event(value)?),
            (12, WireValue::Varint(value)) => span.dropped_events_count = protobuf_u32(value),
            (13, WireValue::Bytes(value)) => span.links.push(decode_link(value)?),
            (14, WireValue::Varint(value)) => span.dropped_links_count = protobuf_u32(value),
            (15, WireValue::Bytes(_)) => {}
            (16, WireValue::Fixed32(value)) => span.flags = value,
            (1..=16, _) => return Err(wrong_wire("span")),
            _ => {}
        }
    }
    Ok(span)
}

/// Constructs one span event.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_event(bytes: &[u8]) -> Result<span::Event, IngestError> {
    let attribute_count = repeated_message_count(bytes, 3)?;
    let mut event = span::Event {
        name: last_bytes_field(bytes, 2)?
            .map(fixed_string)
            .transpose()?
            .unwrap_or_default(),
        attributes: Vec::with_capacity(attribute_count),
        ..span::Event::default()
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Fixed64(value)) => event.time_unix_nano = value,
            (2, WireValue::Bytes(_)) => {}
            (3, WireValue::Bytes(value)) => event.attributes.push(decode_key_value(value)?),
            (4, WireValue::Varint(value)) => {
                event.dropped_attributes_count = protobuf_u32(value);
            }
            (1..=4, _) => return Err(wrong_wire("span event")),
            _ => {}
        }
    }
    Ok(event)
}

/// Constructs one span link.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_link(bytes: &[u8]) -> Result<span::Link, IngestError> {
    let attribute_count = repeated_message_count(bytes, 4)?;
    let mut link = span::Link {
        trace_id: last_bytes_field(bytes, 1)?.map_or_else(Vec::new, fixed_bytes),
        span_id: last_bytes_field(bytes, 2)?.map_or_else(Vec::new, fixed_bytes),
        trace_state: last_bytes_field(bytes, 3)?
            .map(fixed_string)
            .transpose()?
            .unwrap_or_default(),
        attributes: Vec::with_capacity(attribute_count),
        ..span::Link::default()
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1..=3, WireValue::Bytes(_)) => {}
            (4, WireValue::Bytes(value)) => link.attributes.push(decode_key_value(value)?),
            (5, WireValue::Varint(value)) => {
                link.dropped_attributes_count = protobuf_u32(value);
            }
            (6, WireValue::Fixed32(value)) => link.flags = value,
            (1..=6, _) => return Err(wrong_wire("span link")),
            _ => {}
        }
    }
    Ok(link)
}

/// Constructs one span status.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_merged_status(bytes: &[u8], wanted_tag: u32) -> Result<Option<Status>, IngestError> {
    let (occurrences, _) = repeated_count_across_messages(bytes, wanted_tag, u32::MAX)?;
    if occurrences == 0 {
        return Ok(None);
    }
    let mut status = Status {
        message: last_bytes_across_messages(bytes, wanted_tag, 2)?
            .map(fixed_string)
            .transpose()?
            .unwrap_or_default(),
        ..Status::default()
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if tag == wanted_tag {
            let WireValue::Bytes(value) = value else {
                return Err(wrong_wire("span status"));
            };
            merge_status(value, &mut status)?;
        }
    }
    Ok(Some(status))
}

/// Merges one encoded span status into its destination.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn merge_status(bytes: &[u8], status: &mut Status) -> Result<(), IngestError> {
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (2, WireValue::Bytes(_)) => {}
            (3, WireValue::Varint(value)) => status.code = protobuf_i32(value),
            (2 | 3, _) => return Err(wrong_wire("span status")),
            _ => {}
        }
    }
    Ok(())
}

/// Constructs one key/value entry and its recursive optional value.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_key_value(bytes: &[u8]) -> Result<KeyValue, IngestError> {
    let value = decode_merged_any_value(bytes, 2)?;
    let key = last_bytes_field(bytes, 1)?
        .map(fixed_string)
        .transpose()?
        .unwrap_or_default();
    let mut fields = WireFields::new(bytes);
    while let Some((tag, wire_value)) = fields.next()? {
        match (tag, wire_value) {
            (1, WireValue::Bytes(_)) => {}
            (2, WireValue::Bytes(_)) => {}
            (1 | 2, _) => return Err(wrong_wire("key/value")),
            _ => {}
        }
    }
    Ok(KeyValue { key, value })
}

/// Borrowed encoded bodies that merge into one singular `AnyValue` message.
#[derive(Clone, Copy)]
enum AnyValueBodies<'a> {
    /// One ordinary array element body.
    Single(&'a [u8]),
    /// Repeated singular-message occurrences inside a parent message.
    Repeated {
        /// Parent message containing the occurrences.
        parent: &'a [u8],
        /// Singular message field tag to merge.
        tag: u32,
    },
}

impl AnyValueBodies<'_> {
    /// Visits every encoded body in protobuf merge order without collecting it.
    ///
    /// # Errors
    ///
    /// Returns a malformed-request error for parent framing or a wrong outer
    /// wire type, or propagates an error returned by `visitor`.
    fn try_for_each(
        self,
        mut visitor: impl FnMut(&[u8]) -> Result<(), IngestError>,
    ) -> Result<usize, IngestError> {
        match self {
            Self::Single(body) => {
                visitor(body)?;
                Ok(1)
            }
            Self::Repeated { parent, tag } => {
                let mut count = 0_usize;
                let mut fields = WireFields::new(parent);
                while let Some((field_tag, value)) = fields.next()? {
                    if field_tag == tag {
                        let WireValue::Bytes(body) = value else {
                            return Err(wrong_wire("optional any value"));
                        };
                        count = count
                            .checked_add(1)
                            .ok_or_else(|| malformed("OTLP message occurrence overflow"))?;
                        visitor(body)?;
                    }
                }
                Ok(count)
            }
        }
    }
}

/// Final oneof run selected across merged `AnyValue` message occurrences.
#[derive(Clone, Copy, Default)]
struct AnyValueSelection {
    /// Selected oneof field tag, or zero for an empty value.
    tag: u32,
    /// Ordinal of the first field in the final same-variant merge run.
    run_start: usize,
    /// Repeated nested elements contributed by that final message run.
    nested_count: usize,
}

/// Constructs one recursive OTLP value with fixed nested collection capacity.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_any_value(bytes: &[u8]) -> Result<AnyValue, IngestError> {
    decode_any_value_bodies(AnyValueBodies::Single(bytes))
}

/// Merges every occurrence of a singular `AnyValue` field.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_merged_any_value(bytes: &[u8], wanted_tag: u32) -> Result<Option<AnyValue>, IngestError> {
    let bodies = AnyValueBodies::Repeated {
        parent: bytes,
        tag: wanted_tag,
    };
    let present = bodies.try_for_each(|_| Ok(()))?;
    if present == 0 {
        return Ok(None);
    }
    decode_any_value_bodies(bodies).map(Some)
}

/// Selects the final protobuf oneof run and its exact nested element count.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, wire types, or
/// cardinality overflow.
fn select_any_value_bodies(bodies: AnyValueBodies<'_>) -> Result<AnyValueSelection, IngestError> {
    let mut selection = AnyValueSelection::default();
    let mut ordinal = 0_usize;
    bodies.try_for_each(|body| {
        let mut fields = WireFields::new(body);
        while let Some((tag, value)) = fields.next()? {
            if !(1..=7).contains(&tag) {
                continue;
            }
            validate_any_value_wire(tag, value)?;
            ordinal = ordinal
                .checked_add(1)
                .ok_or_else(|| malformed("OTLP oneof occurrence overflow"))?;
            if selection.tag != tag || !matches!(tag, 5 | 6) {
                selection = AnyValueSelection {
                    tag,
                    run_start: ordinal,
                    nested_count: 0,
                };
            }
            if let WireValue::Bytes(nested) = value
                && matches!(tag, 5 | 6)
            {
                selection.nested_count = selection
                    .nested_count
                    .checked_add(repeated_message_count(nested, 1)?)
                    .ok_or_else(|| malformed("OTLP nested value count overflow"))?;
            }
        }
        Ok(())
    })?;
    Ok(selection)
}

/// Decodes a protobuf-ordered sequence of merged `AnyValue` bodies.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, wire types,
/// or nested capacity overflow.
fn decode_any_value_bodies(bodies: AnyValueBodies<'_>) -> Result<AnyValue, IngestError> {
    let selection = select_any_value_bodies(bodies)?;

    let mut value = match selection.tag {
        5 => Some(any_value::Value::ArrayValue(ArrayValue {
            values: Vec::with_capacity(selection.nested_count),
        })),
        6 => Some(any_value::Value::KvlistValue(KeyValueList {
            values: Vec::with_capacity(selection.nested_count),
        })),
        _ => None,
    };
    let mut ordinal = 0_usize;
    bodies.try_for_each(|body| {
        let mut fields = WireFields::new(body);
        while let Some((tag, wire_value)) = fields.next()? {
            if !(1..=7).contains(&tag) {
                continue;
            }
            ordinal += 1;
            if ordinal < selection.run_start || tag != selection.tag {
                continue;
            }
            value = match (tag, wire_value, value.take()) {
                (1, WireValue::Bytes(bytes), _) => {
                    Some(any_value::Value::StringValue(fixed_string(bytes)?))
                }
                (2, WireValue::Varint(value), _) => Some(any_value::Value::BoolValue(value != 0)),
                (3, WireValue::Varint(value), _) => {
                    Some(any_value::Value::IntValue(protobuf_i64(value)))
                }
                (4, WireValue::Fixed64(value), _) => {
                    Some(any_value::Value::DoubleValue(f64::from_bits(value)))
                }
                (5, WireValue::Bytes(bytes), Some(any_value::Value::ArrayValue(mut array))) => {
                    merge_array_value(bytes, &mut array)?;
                    Some(any_value::Value::ArrayValue(array))
                }
                (6, WireValue::Bytes(bytes), Some(any_value::Value::KvlistValue(mut list))) => {
                    merge_key_value_list(bytes, &mut list)?;
                    Some(any_value::Value::KvlistValue(list))
                }
                (7, WireValue::Bytes(bytes), _) => {
                    Some(any_value::Value::BytesValue(fixed_bytes(bytes)))
                }
                _ => return Err(wrong_wire("OTLP any value")),
            };
        }
        Ok(())
    })?;
    Ok(AnyValue { value })
}

/// Validates the generated wire type for one `AnyValue` oneof field.
///
/// # Errors
///
/// Returns a malformed-request error when `value` has the wrong wire type.
fn validate_any_value_wire(tag: u32, value: WireValue<'_>) -> Result<(), IngestError> {
    if matches!(
        (tag, value),
        (1 | 5 | 6 | 7, WireValue::Bytes(_))
            | (2 | 3, WireValue::Varint(_))
            | (4, WireValue::Fixed64(_))
    ) {
        Ok(())
    } else {
        Err(wrong_wire("OTLP any value"))
    }
}

/// Merges one encoded array occurrence into a fixed-capacity destination.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or wire types.
fn merge_array_value(bytes: &[u8], value: &mut ArrayValue) -> Result<(), IngestError> {
    let mut fields = WireFields::new(bytes);
    while let Some((tag, wire_value)) = fields.next()? {
        match (tag, wire_value) {
            (1, WireValue::Bytes(item)) => value.values.push(decode_any_value(item)?),
            (1, _) => return Err(wrong_wire("OTLP value array")),
            _ => {}
        }
    }
    Ok(())
}

/// Merges one encoded key/value-list occurrence into a fixed destination.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or wire types.
fn merge_key_value_list(bytes: &[u8], value: &mut KeyValueList) -> Result<(), IngestError> {
    let mut fields = WireFields::new(bytes);
    while let Some((tag, wire_value)) = fields.next()? {
        match (tag, wire_value) {
            (1, WireValue::Bytes(item)) => value.values.push(decode_key_value(item)?),
            (1, _) => return Err(wrong_wire("OTLP key/value list")),
            _ => {}
        }
    }
    Ok(())
}

/// Constructs the stable malformed error for a known field's wire mismatch.
fn wrong_wire(field: &str) -> IngestError {
    malformed(&format!("invalid protobuf wire type for {field}"))
}

/// Constructs the existing stable malformed OTLP refusal.
fn malformed(message: &str) -> IngestError {
    IngestError::Decode(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span, status};
    use wyrd_tonic::prost::Message;

    /// Builds one attribute for rich trace decode fixtures.
    fn attribute(key: &str, value: any_value::Value) -> KeyValue {
        KeyValue {
            key: key.to_owned(),
            value: Some(AnyValue { value: Some(value) }),
        }
    }

    /// Builds alternating array/key-value nesting with one value node per level.
    fn alternating_value(depth: usize) -> AnyValue {
        if depth == 1 {
            return AnyValue {
                value: Some(any_value::Value::IntValue(1)),
            };
        }
        let nested = alternating_value(depth - 1);
        let value = if depth.is_multiple_of(2) {
            any_value::Value::ArrayValue(ArrayValue {
                values: vec![nested],
            })
        } else {
            any_value::Value::KvlistValue(KeyValueList {
                values: vec![KeyValue {
                    key: "nested".to_owned(),
                    value: Some(nested),
                }],
            })
        };
        AnyValue { value: Some(value) }
    }

    /// Asserts that a generated vector has no unused growth capacity.
    ///
    /// # Panics
    ///
    /// Panics when `capacity` differs from the populated length.
    fn assert_fixed_vec<T>(values: &[T], capacity: usize) {
        assert_eq!(values.len(), capacity);
    }

    /// Asserts exact capacities throughout one recursive attribute value.
    ///
    /// # Panics
    ///
    /// Panics when any nested value retains spare capacity.
    fn assert_fixed_any_value(value: &AnyValue) {
        match value.value.as_ref() {
            Some(any_value::Value::StringValue(value)) => {
                assert_eq!(value.len(), value.capacity());
            }
            Some(any_value::Value::BytesValue(value)) => {
                assert_fixed_vec(value, value.capacity());
            }
            Some(any_value::Value::ArrayValue(array)) => {
                assert_fixed_vec(&array.values, array.values.capacity());
                for value in &array.values {
                    assert_fixed_any_value(value);
                }
            }
            Some(any_value::Value::KvlistValue(list)) => {
                assert_fixed_vec(&list.values, list.values.capacity());
                assert_fixed_attributes(&list.values);
            }
            _ => {}
        }
    }

    /// Asserts exact key, value, and collection capacities for attributes.
    ///
    /// # Panics
    ///
    /// Panics when a key or recursive value retains spare capacity.
    fn assert_fixed_attributes(attributes: &[KeyValue]) {
        for attribute in attributes {
            assert_eq!(attribute.key.len(), attribute.key.capacity());
            if let Some(value) = &attribute.value {
                assert_fixed_any_value(value);
            }
        }
    }

    /// Asserts that every scalable collection in a decoded trace is full.
    ///
    /// # Panics
    ///
    /// Panics when any generated collection retains spare capacity.
    fn assert_fixed_trace_capacity(request: &ExportTraceServiceRequest) {
        assert_fixed_vec(&request.resource_spans, request.resource_spans.capacity());
        for resource_spans in &request.resource_spans {
            assert_eq!(
                resource_spans.schema_url.len(),
                resource_spans.schema_url.capacity()
            );
            assert_fixed_vec(
                &resource_spans.scope_spans,
                resource_spans.scope_spans.capacity(),
            );
            if let Some(resource) = &resource_spans.resource {
                assert_fixed_vec(&resource.attributes, resource.attributes.capacity());
                assert_fixed_attributes(&resource.attributes);
                assert_fixed_vec(&resource.entity_refs, resource.entity_refs.capacity());
                for entity in &resource.entity_refs {
                    assert_eq!(entity.schema_url.len(), entity.schema_url.capacity());
                    assert_eq!(entity.r#type.len(), entity.r#type.capacity());
                    assert_fixed_vec(&entity.id_keys, entity.id_keys.capacity());
                    assert_fixed_vec(&entity.description_keys, entity.description_keys.capacity());
                    for key in entity.id_keys.iter().chain(&entity.description_keys) {
                        assert_eq!(key.len(), key.capacity());
                    }
                }
            }
            for scope_spans in &resource_spans.scope_spans {
                assert_eq!(
                    scope_spans.schema_url.len(),
                    scope_spans.schema_url.capacity()
                );
                assert_fixed_vec(&scope_spans.spans, scope_spans.spans.capacity());
                if let Some(scope) = &scope_spans.scope {
                    assert_eq!(scope.name.len(), scope.name.capacity());
                    assert_eq!(scope.version.len(), scope.version.capacity());
                    assert_fixed_vec(&scope.attributes, scope.attributes.capacity());
                    assert_fixed_attributes(&scope.attributes);
                }
                for span in &scope_spans.spans {
                    assert_fixed_vec(&span.trace_id, span.trace_id.capacity());
                    assert_fixed_vec(&span.span_id, span.span_id.capacity());
                    assert_fixed_vec(&span.parent_span_id, span.parent_span_id.capacity());
                    assert_eq!(span.trace_state.len(), span.trace_state.capacity());
                    assert_eq!(span.name.len(), span.name.capacity());
                    assert_fixed_vec(&span.attributes, span.attributes.capacity());
                    assert_fixed_attributes(&span.attributes);
                    assert_fixed_vec(&span.events, span.events.capacity());
                    for event in &span.events {
                        assert_eq!(event.name.len(), event.name.capacity());
                        assert_fixed_vec(&event.attributes, event.attributes.capacity());
                        assert_fixed_attributes(&event.attributes);
                    }
                    assert_fixed_vec(&span.links, span.links.capacity());
                    for link in &span.links {
                        assert_fixed_vec(&link.trace_id, link.trace_id.capacity());
                        assert_fixed_vec(&link.span_id, link.span_id.capacity());
                        assert_eq!(link.trace_state.len(), link.trace_state.capacity());
                        assert_fixed_vec(&link.attributes, link.attributes.capacity());
                        assert_fixed_attributes(&link.attributes);
                    }
                    if let Some(status) = &span.status {
                        assert_eq!(status.message.len(), status.message.capacity());
                    }
                }
            }
        }
    }

    /// Computes the live generated-layout and backing capacity of one trace.
    ///
    /// This mirrors the preflight accounting contract from the constructed
    /// value rather than from the wire, including recursive `AnyValue` storage.
    fn decoded_trace_capacity(request: &ExportTraceServiceRequest) -> usize {
        size_of::<ExportTraceServiceRequest>()
            + request.resource_spans.capacity() * size_of::<ResourceSpans>()
            + request
                .resource_spans
                .iter()
                .map(decoded_resource_spans_capacity)
                .sum::<usize>()
    }

    /// Computes storage owned below one inlined resource-spans element.
    fn decoded_resource_spans_capacity(resource_spans: &ResourceSpans) -> usize {
        resource_spans.schema_url.capacity()
            + resource_spans.scope_spans.capacity() * size_of::<ScopeSpans>()
            + resource_spans
                .resource
                .as_ref()
                .map_or(0, decoded_resource_capacity)
            + resource_spans
                .scope_spans
                .iter()
                .map(decoded_scope_spans_capacity)
                .sum::<usize>()
    }

    /// Computes storage owned below one optional resource value.
    fn decoded_resource_capacity(resource: &Resource) -> usize {
        decoded_attributes_capacity(&resource.attributes, resource.attributes.capacity())
            + resource.entity_refs.capacity() * size_of::<EntityRef>()
            + resource
                .entity_refs
                .iter()
                .map(decoded_entity_ref_capacity)
                .sum::<usize>()
    }

    /// Computes storage owned below one inlined entity-reference element.
    fn decoded_entity_ref_capacity(entity: &EntityRef) -> usize {
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
    }

    /// Computes storage owned below one inlined scope-spans element.
    fn decoded_scope_spans_capacity(scope_spans: &ScopeSpans) -> usize {
        scope_spans.schema_url.capacity()
            + scope_spans.spans.capacity() * size_of::<Span>()
            + scope_spans.scope.as_ref().map_or(0, decoded_scope_capacity)
            + scope_spans
                .spans
                .iter()
                .map(decoded_span_capacity)
                .sum::<usize>()
    }

    /// Computes storage owned below one optional instrumentation scope.
    fn decoded_scope_capacity(scope: &InstrumentationScope) -> usize {
        scope.name.capacity()
            + scope.version.capacity()
            + decoded_attributes_capacity(&scope.attributes, scope.attributes.capacity())
    }

    /// Computes storage owned below one inlined span element.
    fn decoded_span_capacity(span: &Span) -> usize {
        span.trace_id.capacity()
            + span.span_id.capacity()
            + span.trace_state.capacity()
            + span.parent_span_id.capacity()
            + span.name.capacity()
            + decoded_attributes_capacity(&span.attributes, span.attributes.capacity())
            + span.events.capacity() * size_of::<span::Event>()
            + span
                .events
                .iter()
                .map(decoded_event_capacity)
                .sum::<usize>()
            + span.links.capacity() * size_of::<span::Link>()
            + span.links.iter().map(decoded_link_capacity).sum::<usize>()
            + span
                .status
                .as_ref()
                .map_or(0, |status| status.message.capacity())
    }

    /// Computes storage owned below one inlined span event.
    fn decoded_event_capacity(event: &span::Event) -> usize {
        event.name.capacity()
            + decoded_attributes_capacity(&event.attributes, event.attributes.capacity())
    }

    /// Computes storage owned below one inlined span link.
    fn decoded_link_capacity(link: &span::Link) -> usize {
        link.trace_id.capacity()
            + link.span_id.capacity()
            + link.trace_state.capacity()
            + decoded_attributes_capacity(&link.attributes, link.attributes.capacity())
    }

    /// Computes generated element layouts and backing storage for attributes.
    fn decoded_attributes_capacity(attributes: &[KeyValue], capacity: usize) -> usize {
        capacity * size_of::<KeyValue>()
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

    /// Computes recursive backing storage below one inlined `AnyValue`.
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
            Some(any_value::Value::KvlistValue(list)) => {
                decoded_attributes_capacity(&list.values, list.values.capacity())
            }
            _ => 0,
        }
    }

    /// Appends one canonical protobuf varint to a test wire buffer.
    fn push_varint(bytes: &mut Vec<u8>, mut value: u64) {
        while value >= 0x80 {
            bytes.push(value.to_le_bytes()[0] | 0x80);
            value >>= 7;
        }
        bytes.push(value.to_le_bytes()[0]);
    }

    /// Appends one length-delimited message field to a test wire buffer.
    ///
    /// # Panics
    ///
    /// Panics on a platform whose `usize` cannot be represented by `u64`.
    fn push_message_field(bytes: &mut Vec<u8>, tag: u32, message: &impl Message) {
        push_varint(bytes, u64::from(tag) << 3 | 2);
        let encoded = message.encode_to_vec();
        push_varint(
            bytes,
            u64::try_from(encoded.len()).expect("invariant: encoded length fits u64"),
        );
        bytes.extend_from_slice(&encoded);
    }

    /// Appends an already encoded length-delimited message to a test wire.
    ///
    /// # Panics
    ///
    /// Panics on a platform whose `usize` cannot be represented by `u64`.
    fn push_raw_message_field(bytes: &mut Vec<u8>, tag: u32, message: &[u8]) {
        push_varint(bytes, u64::from(tag) << 3 | 2);
        push_varint(
            bytes,
            u64::try_from(message.len()).expect("invariant: encoded length fits u64"),
        );
        bytes.extend_from_slice(message);
    }

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

    /// Proves the fixed constructor preserves rich generated trace semantics.
    ///
    /// # Panics
    ///
    /// Panics when the fixture fails to encode or decode identically.
    #[test]
    fn trace_fixed_decode_round_trips_all_generated_shapes() {
        let nested = any_value::Value::KvlistValue(KeyValueList {
            values: vec![attribute(
                "array",
                any_value::Value::ArrayValue(ArrayValue {
                    values: vec![
                        AnyValue {
                            value: Some(any_value::Value::StringValue("nested".to_owned())),
                        },
                        AnyValue {
                            value: Some(any_value::Value::BytesValue(vec![9, 8, 7])),
                        },
                        AnyValue {
                            value: Some(any_value::Value::BoolValue(true)),
                        },
                        AnyValue {
                            value: Some(any_value::Value::IntValue(-7)),
                        },
                        AnyValue {
                            value: Some(any_value::Value::DoubleValue(1.25)),
                        },
                    ],
                }),
            )],
        });
        let request = ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: Some(Resource {
                    attributes: vec![attribute("resource", nested)],
                    dropped_attributes_count: 2,
                    entity_refs: vec![EntityRef {
                        schema_url: "https://entity/v1".to_owned(),
                        r#type: "service".to_owned(),
                        id_keys: vec!["service.name".to_owned()],
                        description_keys: vec!["service.version".to_owned()],
                    }],
                }),
                scope_spans: vec![ScopeSpans {
                    scope: Some(InstrumentationScope {
                        name: "sdk".to_owned(),
                        version: "1.2.3".to_owned(),
                        attributes: vec![attribute(
                            "scope",
                            any_value::Value::StringValue("value".to_owned()),
                        )],
                        dropped_attributes_count: 3,
                    }),
                    spans: vec![Span {
                        trace_id: vec![1; 16],
                        span_id: vec![2; 8],
                        trace_state: "vendor=value".to_owned(),
                        parent_span_id: vec![3; 8],
                        flags: 0x301,
                        name: "operation".to_owned(),
                        kind: span::SpanKind::Server as i32,
                        start_time_unix_nano: 10,
                        end_time_unix_nano: 20,
                        attributes: vec![attribute(
                            "span",
                            any_value::Value::BytesValue(vec![4, 5, 6]),
                        )],
                        dropped_attributes_count: 4,
                        events: vec![span::Event {
                            time_unix_nano: 15,
                            name: "event".to_owned(),
                            attributes: vec![attribute("event", any_value::Value::IntValue(42))],
                            dropped_attributes_count: 5,
                        }],
                        dropped_events_count: 6,
                        links: vec![span::Link {
                            trace_id: vec![7; 16],
                            span_id: vec![8; 8],
                            trace_state: "link=value".to_owned(),
                            attributes: vec![attribute("link", any_value::Value::BoolValue(false))],
                            dropped_attributes_count: 7,
                            flags: 0x201,
                        }],
                        dropped_links_count: 8,
                        status: Some(Status {
                            message: "complete".to_owned(),
                            code: status::StatusCode::Ok as i32,
                        }),
                    }],
                    schema_url: "https://scope/v1".to_owned(),
                }],
                schema_url: "https://resource/v1".to_owned(),
            }],
        };
        let mut bytes = request.encode_to_vec();
        bytes.extend_from_slice(&[0xa0, 0x06, 0x01]);
        bytes.extend_from_slice(&[0xab, 0x06, 0x08, 0x01, 0xac, 0x06]);
        let plan =
            preflight_trace_protobuf(&bytes, vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS)
                .expect("rich trace preflight must succeed");
        let decoded = decode_trace_protobuf(&bytes).expect("rich trace must decode");
        assert_eq!(decoded, request);
        assert_eq!(plan.decode_bytes, decoded_trace_capacity(&decoded));
        assert_fixed_trace_capacity(&decoded);
    }

    /// Proves every repeated trace container is allocated once at its wire count.
    ///
    /// # Panics
    ///
    /// Panics when the fixed constructor rejects the bounded multi-item fixture.
    #[test]
    fn trace_fixed_decode_has_no_spare_collection_capacity() {
        let request = ExportTraceServiceRequest {
            resource_spans: (0..3)
                .map(|resource| ResourceSpans {
                    scope_spans: (0..2)
                        .map(|scope| ScopeSpans {
                            spans: (0..4)
                                .map(|span| Span {
                                    trace_id: vec![resource; 16],
                                    span_id: vec![scope; 8],
                                    name: format!("span-{span}"),
                                    attributes: vec![attribute(
                                        "index",
                                        any_value::Value::IntValue(span),
                                    )],
                                    ..Span::default()
                                })
                                .collect(),
                            ..ScopeSpans::default()
                        })
                        .collect(),
                    ..ResourceSpans::default()
                })
                .collect(),
        };
        let decoded = decode_trace_protobuf(&request.encode_to_vec())
            .expect("bounded multi-item trace must decode");
        assert_eq!(decoded, request);
        assert_fixed_trace_capacity(&decoded);
    }

    /// Proves split singular messages merge exactly like generated prost decode.
    ///
    /// # Panics
    ///
    /// Panics when a split resource, scope, status, or recursive value differs
    /// from prost or leaves spare collection capacity.
    #[test]
    fn trace_fixed_decode_merges_split_singular_messages() {
        let mut resource_wire = Vec::new();
        push_message_field(
            &mut resource_wire,
            1,
            &Resource {
                attributes: vec![attribute("first", any_value::Value::IntValue(1))],
                dropped_attributes_count: 1,
                ..Resource::default()
            },
        );
        push_message_field(
            &mut resource_wire,
            1,
            &Resource {
                attributes: vec![attribute("second", any_value::Value::IntValue(2))],
                dropped_attributes_count: 2,
                ..Resource::default()
            },
        );
        let expected_resource = ResourceSpans::decode(resource_wire.as_slice())
            .expect("prost must decode split resource");
        let decoded_resource =
            decode_resource_spans(&resource_wire).expect("fixed decoder must merge resource");
        assert_eq!(decoded_resource, expected_resource);
        let resource = decoded_resource.resource.expect("merged resource");
        assert_fixed_vec(&resource.attributes, resource.attributes.capacity());

        let mut scope_wire = Vec::new();
        push_message_field(
            &mut scope_wire,
            1,
            &InstrumentationScope {
                name: "first".to_owned(),
                attributes: vec![attribute("first", any_value::Value::BoolValue(true))],
                ..InstrumentationScope::default()
            },
        );
        push_message_field(
            &mut scope_wire,
            1,
            &InstrumentationScope {
                name: "retained".to_owned(),
                version: "second".to_owned(),
                attributes: vec![attribute("second", any_value::Value::BoolValue(false))],
                ..InstrumentationScope::default()
            },
        );
        let expected_scope =
            ScopeSpans::decode(scope_wire.as_slice()).expect("prost must decode split scope");
        let decoded_scope =
            decode_scope_spans(&scope_wire).expect("fixed decoder must merge scope");
        assert_eq!(decoded_scope, expected_scope);
        let scope = decoded_scope.scope.expect("merged scope");
        assert_fixed_vec(&scope.attributes, scope.attributes.capacity());

        let mut span_wire = Vec::new();
        push_message_field(
            &mut span_wire,
            15,
            &Status {
                message: "first".to_owned(),
                ..Status::default()
            },
        );
        push_message_field(
            &mut span_wire,
            15,
            &Status {
                message: "retained".to_owned(),
                code: status::StatusCode::Error as i32,
            },
        );
        let expected_span = Span::decode(span_wire.as_slice()).expect("prost must decode status");
        let decoded_span = decode_span(&span_wire).expect("fixed decoder must merge status");
        assert_eq!(decoded_span, expected_span);
        let status = decoded_span.status.expect("merged status");
        assert_eq!(status.message.len(), status.message.capacity());

        let mut key_value_wire = Vec::new();
        push_message_field(
            &mut key_value_wire,
            2,
            &AnyValue {
                value: Some(any_value::Value::ArrayValue(ArrayValue {
                    values: vec![AnyValue {
                        value: Some(any_value::Value::IntValue(1)),
                    }],
                })),
            },
        );
        push_message_field(
            &mut key_value_wire,
            2,
            &AnyValue {
                value: Some(any_value::Value::ArrayValue(ArrayValue {
                    values: vec![AnyValue {
                        value: Some(any_value::Value::IntValue(2)),
                    }],
                })),
            },
        );
        let expected_key =
            KeyValue::decode(key_value_wire.as_slice()).expect("prost must merge any value");
        let decoded_key =
            decode_key_value(&key_value_wire).expect("fixed decoder must merge any value");
        assert_eq!(decoded_key, expected_key);
        assert_fixed_any_value(decoded_key.value.as_ref().expect("merged any value"));

        let mut string_value_wire = Vec::new();
        push_raw_message_field(&mut string_value_wire, 1, b"discarded string");
        push_raw_message_field(&mut string_value_wire, 1, b"retained string");
        let mut string_attribute_wire = Vec::new();
        push_raw_message_field(&mut string_attribute_wire, 1, b"discarded key");
        push_raw_message_field(&mut string_attribute_wire, 1, b"string key");
        push_raw_message_field(&mut string_attribute_wire, 2, &string_value_wire);

        let mut bytes_value_wire = Vec::new();
        push_raw_message_field(&mut bytes_value_wire, 7, &[1; 17]);
        push_raw_message_field(&mut bytes_value_wire, 7, &[2; 3]);
        let mut bytes_attribute_wire = Vec::new();
        push_raw_message_field(&mut bytes_attribute_wire, 1, b"bytes key");
        push_raw_message_field(&mut bytes_attribute_wire, 2, &bytes_value_wire);

        let mut scalar_value_wire = Vec::new();
        push_varint(&mut scalar_value_wire, 3 << 3);
        push_varint(&mut scalar_value_wire, 7);
        push_varint(&mut scalar_value_wire, 3 << 3);
        push_varint(&mut scalar_value_wire, 9);
        let mut scalar_attribute_wire = Vec::new();
        push_raw_message_field(&mut scalar_attribute_wire, 1, b"scalar key");
        push_raw_message_field(&mut scalar_attribute_wire, 2, &scalar_value_wire);

        let mut third_resource_wire = Vec::new();
        push_raw_message_field(&mut third_resource_wire, 1, &key_value_wire);
        push_raw_message_field(&mut third_resource_wire, 1, &string_attribute_wire);
        push_raw_message_field(&mut third_resource_wire, 1, &bytes_attribute_wire);
        push_raw_message_field(&mut third_resource_wire, 1, &scalar_attribute_wire);
        push_raw_message_field(&mut resource_wire, 1, &third_resource_wire);

        push_raw_message_field(&mut span_wire, 1, &[1; 16]);
        push_raw_message_field(&mut span_wire, 1, &[2; 16]);
        push_raw_message_field(&mut span_wire, 5, b"discarded span");
        push_raw_message_field(&mut span_wire, 5, b"retained span");
        push_varint(&mut span_wire, 6 << 3);
        push_varint(&mut span_wire, 1);
        push_varint(&mut span_wire, 6 << 3);
        push_varint(&mut span_wire, 2);
        push_raw_message_field(&mut scope_wire, 2, &span_wire);
        push_raw_message_field(&mut resource_wire, 2, &scope_wire);
        push_raw_message_field(&mut resource_wire, 3, b"discarded schema");
        push_raw_message_field(&mut resource_wire, 3, b"retained schema");

        let mut request_wire = Vec::new();
        push_raw_message_field(&mut request_wire, 1, &resource_wire);
        let expected_request = ExportTraceServiceRequest::decode(request_wire.as_slice())
            .expect("prost must decode the complete split fixture");
        let plan = preflight_trace_protobuf(
            &request_wire,
            vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS,
        )
        .expect("split fixture preflight must succeed");
        let decoded_request =
            decode_trace_protobuf(&request_wire).expect("fixed decoder must decode split fixture");
        assert_eq!(decoded_request, expected_request);
        assert_eq!(plan.decode_bytes, decoded_trace_capacity(&decoded_request));
        assert_fixed_trace_capacity(&decoded_request);
    }

    /// Proves unknown groups are bounded by the fixed scanner depth.
    #[test]
    fn trace_preflight_rejects_excess_unknown_group_depth() {
        let mut bytes = Vec::new();
        for tag in 100_u32..109 {
            push_varint(&mut bytes, u64::from(tag) << 3 | 3);
        }
        for tag in (100_u32..109).rev() {
            push_varint(&mut bytes, u64::from(tag) << 3 | 4);
        }
        let error =
            preflight_trace_protobuf(&bytes, vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS)
                .expect_err("excess group depth must fail");
        assert!(matches!(error, IngestError::Decode(_)));
    }

    /// Proves alternating arrays and key/value lists honor the same depth ceiling.
    #[test]
    fn trace_preflight_enforces_alternating_value_depth() {
        for (depth, accepted) in [(8, true), (9, false)] {
            let request = ExportTraceServiceRequest {
                resource_spans: vec![ResourceSpans {
                    resource: Some(Resource {
                        attributes: vec![KeyValue {
                            key: "root".to_owned(),
                            value: Some(alternating_value(depth)),
                        }],
                        ..Resource::default()
                    }),
                    ..ResourceSpans::default()
                }],
            };
            let result = preflight_trace_protobuf(
                &request.encode_to_vec(),
                vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS,
            );
            assert_eq!(result.is_ok(), accepted, "depth {depth}");
        }
    }
}
