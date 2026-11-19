//! Allocation-free OTLP wire preflight for server-owned decode adapters.

use std::borrow::Cow;
use std::mem::size_of;

use crate::state::ServerGate;
use vala_bifrost_redux::gate::{AuthContext, IngestError, OtlpWireLimits, PreparedTraces};
use vala_bifrost_redux::tables::signal::{AttributePool, PoolRange, PooledEntry, PooledValue};
use vala_bifrost_redux::tables::traces::input::{
    AttributeList, EventInput, LinkInput, ResourceInput, ScopeInput, SpanInput, StatusInput,
};
use vala_bifrost_redux::tables::traces::{SpanOutputWidths, TraceInput};
use wyrd_tonic::otlp::common::v1::{
    AnyValue, ArrayValue, EntityRef, InstrumentationScope, KeyValue, KeyValueList, any_value,
};
use wyrd_tonic::otlp::resource::v1::Resource;
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span, Status, span};
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;

/// Maximum deprecated protobuf group nesting accepted by the bounded scanner.
pub(crate) const MAX_WIRE_GROUP_DEPTH: usize = 8;

/// Exact adapter facts established without constructing generated messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::struct_field_names,
    reason = "every field is a byte count; the shared suffix names the unit"
)]
pub(crate) struct OtlpDecodePlan {
    /// Encoded protobuf bytes inspected by the preflight.
    pub(crate) wire_bytes: usize,
    /// Generated request storage admitted before prost construction.
    pub(crate) decode_bytes: usize,
    /// Conservative canonical Arrow-plus-managed output the request projects to.
    pub(crate) projected_bytes: usize,
}

impl OtlpDecodePlan {
    /// Returns the generated-request backing the caller reserves before
    /// generated decode.
    ///
    /// Projected Arrow output is not charged here: Scribe charges it when it
    /// is materialized.
    pub(crate) const fn reservation_bytes(&self) -> usize {
        self.decode_bytes
    }
}

/// Resource envelope copies each descendant row may retain: the attribute
/// encoding, the promoted `service.name`, and entity-reference offsets.
pub(crate) const RESOURCE_FAN_OUT: usize = 3;

/// Scope and metric-descriptor copies each descendant row may retain.
pub(crate) const GROUP_FAN_OUT: usize = 2;

/// Record payload copies one row may retain: its canonical columns plus
/// promoted attribute values and correlation text.
const RECORD_FAN_OUT: usize = 2;

/// Maximum nesting of open resource, scope, metric, and record frames.
const MAX_OUTPUT_FRAMES: usize = 4;

/// Refuses a request whose typed backing or projected output exceeds the
/// derived expanded-data ceiling.
///
/// # Errors
///
/// Returns [`IngestError::PayloadTooLarge`] naming the first bound exceeded.
pub(crate) fn enforce_expanded_ceiling(
    decode_bytes: usize,
    projected_bytes: usize,
    limits: OtlpWireLimits,
) -> Result<(), IngestError> {
    let limit = limits.expanded_bytes();
    for bytes in [decode_bytes, projected_bytes] {
        if bytes > limit {
            return Err(IngestError::PayloadTooLarge {
                bytes: u64::try_from(bytes).unwrap_or(u64::MAX),
                limit: u64::try_from(limit).unwrap_or(u64::MAX),
            });
        }
    }
    Ok(())
}

/// One open resource, scope, metric, or record while projecting output.
#[derive(Clone, Copy, Debug, Default)]
struct OutputFrame {
    /// Typed backing bytes charged when the frame opened.
    decode_start: usize,
    /// Generated struct layout of the element, which is not projected payload.
    layout: usize,
    /// Rows projected when the frame opened.
    rows_start: usize,
    /// Typed backing bytes owned by already-closed child frames.
    children: usize,
}

/// Conservative projected canonical output of one OTLP request.
///
/// The preflight opens a frame for each resource, scope, metric, and record
/// and closes it with the typed payload backing charged meanwhile. A closed record
/// charges its fixed row width plus [`RECORD_FAN_OUT`] copies of the backing
/// it owns; a closed group charges its own backing once per descendant row,
/// times its fan-out, because every row repeats its envelope. Typed backing
/// is an upper bound on the payload bytes projection re-encodes, and charging
/// on close keeps the bound independent of protobuf field order. Arithmetic
/// saturates so an overflow reads as an over-limit request.
#[derive(Debug, Default)]
pub(crate) struct ProjectedOutput {
    /// Projected output bytes charged so far.
    bytes: usize,
    /// Records closed so far.
    rows: usize,
    /// Open frames, innermost last.
    frames: [OutputFrame; MAX_OUTPUT_FRAMES],
    /// Number of open frames.
    depth: usize,
}

impl ProjectedOutput {
    /// Returns the projected output bytes charged so far.
    pub(crate) const fn bytes(&self) -> usize {
        self.bytes
    }

    /// Charges fixed-width output that does not depend on typed backing, such
    /// as one nested event, link, or exemplar.
    pub(crate) const fn add(&mut self, bytes: usize) {
        self.bytes = self.bytes.saturating_add(bytes);
    }

    /// Opens a frame at the current typed backing total, before the caller
    /// charges the element's generated struct `layout`.
    ///
    /// # Panics
    ///
    /// Panics when more than [`MAX_OUTPUT_FRAMES`] frames are open, which the
    /// fixed OTLP message hierarchy never produces.
    pub(crate) fn open(&mut self, decode_now: usize, layout: usize) {
        self.frames[self.depth] = OutputFrame {
            decode_start: decode_now,
            layout,
            rows_start: self.rows,
            children: 0,
        };
        self.depth += 1;
    }

    /// Closes the innermost frame and returns the payload backing it owns
    /// (excluding child frames and its own struct layout), crediting its whole
    /// backing to the enclosing frame.
    ///
    /// # Panics
    ///
    /// Panics when no frame is open, which a balanced preflight never does.
    fn close(&mut self, decode_now: usize) -> (usize, OutputFrame) {
        self.depth -= 1;
        let frame = self.frames[self.depth];
        let total = decode_now.saturating_sub(frame.decode_start);
        if let Some(parent) = self.depth.checked_sub(1) {
            let parent = &mut self.frames[parent];
            parent.children = parent.children.saturating_add(total);
        }
        (
            total
                .saturating_sub(frame.children)
                .saturating_sub(frame.layout),
            frame,
        )
    }

    /// Closes one resource, scope, or metric group, charging its own backing
    /// `fan_out` times for every row projected beneath it.
    pub(crate) fn close_group(&mut self, decode_now: usize, fan_out: usize) {
        let (own, frame) = self.close(decode_now);
        let rows = self.rows - frame.rows_start;
        self.add(own.saturating_mul(fan_out).saturating_mul(rows));
    }

    /// Closes one record, charging its fixed row width plus its payload copies.
    pub(crate) fn close_record(&mut self, decode_now: usize, row_bytes: usize) {
        let (own, _) = self.close(decode_now);
        self.rows += 1;
        self.add(row_bytes.saturating_add(own.saturating_mul(RECORD_FAN_OUT)));
    }
}

/// How many of each view and pooled value a borrowed decode of one trace
/// export holds, counted by [`preflight_trace_protobuf`].
///
/// Resource and scope attributes are decoded into owned messages, so they are
/// not counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TracePoolCounts {
    /// Resource groups.
    pub(crate) resources: usize,
    /// Scope groups.
    pub(crate) scopes: usize,
    /// Spans.
    pub(crate) spans: usize,
    /// Span events.
    pub(crate) events: usize,
    /// Span links.
    pub(crate) links: usize,
    /// Span, event, and link attribute entries, nested entries included.
    pub(crate) entries: usize,
    /// Array items.
    pub(crate) values: usize,
}

/// Bounded counters used while walking one trace export.
#[derive(Debug)]
struct TraceWireFacts {
    /// Shared immutable limits selected by the Scribe runtime.
    limits: OtlpWireLimits,
    /// Fixed Arrow widths charged per projected span and nested element.
    widths: SpanOutputWidths,
    /// Public-layout and exact backing capacity required by the typed request.
    decode_bytes: usize,
    /// Conservative projected canonical output.
    output: ProjectedOutput,
    /// The views and pooled values a borrowed decode holds.
    pool: TracePoolCounts,
}

impl TraceWireFacts {
    /// Creates the fixed counter set with the root request allocation included.
    fn new(limits: OtlpWireLimits) -> Self {
        Self {
            limits,
            widths: SpanOutputWidths::new(),
            decode_bytes: size_of::<ExportTraceServiceRequest>(),
            output: ProjectedOutput::default(),
            pool: TracePoolCounts::default(),
        }
    }

    /// Adds exact generated backing bytes for one retained value.
    ///
    /// # Errors
    ///
    /// Returns the stable malformed-request error on arithmetic overflow.
    fn add_value_bytes(&mut self, amount: usize) -> Result<(), IngestError> {
        self.add_decode_bytes(amount)
    }

    /// Adds public-layout or backing capacity to the typed decode owner.
    ///
    /// # Errors
    ///
    /// Returns the stable malformed-request error on arithmetic overflow.
    fn add_decode_bytes(&mut self, amount: usize) -> Result<(), IngestError> {
        self.decode_bytes = self
            .decode_bytes
            .checked_add(amount)
            .ok_or_else(|| malformed("OTLP protobuf capacity overflow"))?;
        Ok(())
    }

    /// Records one repeated attribute element before visiting its payload.
    ///
    /// # Errors
    ///
    /// Returns the stable malformed-request error on arithmetic overflow.
    fn add_attribute(&mut self) -> Result<(), IngestError> {
        self.pool.entries += 1;
        self.add_decode_bytes(size_of::<KeyValue>())
    }
}

/// One parsed protobuf field borrowing its original wire storage.
#[derive(Clone, Copy, Debug)]
pub(crate) enum WireValue<'a> {
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
pub(crate) struct WireFields<'a> {
    /// Complete borrowed message bytes.
    bytes: &'a [u8],
    /// Next unread byte offset.
    pub(crate) cursor: usize,
    /// Maximum deprecated-group nesting accepted by this cursor.
    group_depth_limit: usize,
}

impl<'a> WireFields<'a> {
    /// Creates a cursor without allocating or copying the message.
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            cursor: 0,
            group_depth_limit: MAX_WIRE_GROUP_DEPTH,
        }
    }

    /// Creates a cursor using an operator-lowered group-depth ceiling.
    pub(crate) fn with_group_depth(bytes: &'a [u8], group_depth_limit: usize) -> Self {
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
    pub(crate) fn next(&mut self) -> Result<Option<(u32, WireValue<'a>)>, IngestError> {
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

    /// Reads one canonical protobuf varint from the current cursor. A
    /// single-byte varint, the common case for tags and short lengths,
    /// returns before the general loop.
    ///
    /// # Errors
    ///
    /// Returns the stable malformed-request error for truncation or a value
    /// wider than ten bytes.
    pub(crate) fn read_varint(&mut self) -> Result<u64, IngestError> {
        if let Some(&byte) = self.bytes.get(self.cursor)
            && byte < 0x80
        {
            self.cursor += 1;
            return Ok(u64::from(byte));
        }
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
) -> Result<(OtlpDecodePlan, TracePoolCounts), IngestError> {
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
            facts
                .output
                .open(facts.decode_bytes, size_of::<ResourceSpans>());
            facts.add_decode_bytes(size_of::<ResourceSpans>())?;
            facts.pool.resources += 1;
            visit_resource_spans(resource, &mut facts)?;
            facts
                .output
                .close_group(facts.decode_bytes, RESOURCE_FAN_OUT);
        }
    }
    enforce_expanded_ceiling(facts.decode_bytes, facts.output.bytes(), limits)?;
    Ok((
        OtlpDecodePlan {
            wire_bytes: bytes.len(),
            decode_bytes: facts.decode_bytes,
            projected_bytes: facts.output.bytes(),
        },
        facts.pool,
    ))
}

/// Byte lengths of a message's singular length-delimited fields, kept
/// last-wins while the message is walked once.
///
/// Protobuf retains only the final occurrence of a singular field, so the
/// preflight charges each one after the walk instead of rescanning the message
/// per field.
#[derive(Clone, Copy, Debug)]
struct LastLengths<const N: usize>([Option<usize>; N]);

impl<const N: usize> Default for LastLengths<N> {
    /// No field seen yet.
    fn default() -> Self {
        Self([None; N])
    }
}

impl<const N: usize> LastLengths<N> {
    /// Records one occurrence of the singular field at `index`.
    ///
    /// # Errors
    ///
    /// Returns the same wrong-wire refusal a rescan would, when the occurrence
    /// is not length-delimited.
    fn record(&mut self, index: usize, value: WireValue<'_>) -> Result<(), IngestError> {
        let WireValue::Bytes(value) = value else {
            return Err(wrong_wire("singular bytes field"));
        };
        self.0[index] = Some(value.len());
        Ok(())
    }

    /// Keeps every field `later` carries, as merging a later message does.
    fn merge(&mut self, later: Self) {
        for (slot, value) in self.0.iter_mut().zip(later.0) {
            if value.is_some() {
                *slot = value;
            }
        }
    }

    /// Charges every retained field's bytes.
    ///
    /// # Errors
    ///
    /// Returns the stable malformed-request error on arithmetic overflow.
    fn charge(self, facts: &mut TraceWireFacts) -> Result<(), IngestError> {
        self.0
            .into_iter()
            .flatten()
            .try_for_each(|bytes| facts.add_value_bytes(bytes))
    }
}

/// Visits one `ResourceSpans` message and its nested scope groups.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid wire shapes or limits.
fn visit_resource_spans(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<(), IngestError> {
    let mut schema_url = LastLengths::<1>::default();
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(resource)) => visit_resource(resource, facts)?,
            (2, WireValue::Bytes(scope)) => {
                facts
                    .output
                    .open(facts.decode_bytes, size_of::<ScopeSpans>());
                facts.add_decode_bytes(size_of::<ScopeSpans>())?;
                facts.pool.scopes += 1;
                visit_scope_spans(scope, facts)?;
                facts.output.close_group(facts.decode_bytes, GROUP_FAN_OUT);
            }
            (3, value) => schema_url.record(0, value)?,
            (1 | 2, _) => return Err(wrong_wire("resource spans")),
            _ => {}
        }
    }
    schema_url.charge(facts)
}

/// Visits one OTLP resource and counts attribute/entity backing storage.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid nested fields or limits.
fn visit_resource(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<(), IngestError> {
    let pool = facts.pool;
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
    facts.pool = pool;
    Ok(())
}

/// Visits one instrumentation scope and its repeated attributes.
///
/// Returns its retained name and version lengths, which merge across every
/// occurrence of the scope in its `ScopeSpans`.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid nested fields or limits.
fn visit_scope(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<LastLengths<2>, IngestError> {
    let pool = facts.pool;
    let mut text = LastLengths::<2>::default();
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (3, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
            (1 | 2, value) => text.record(usize::try_from(tag - 1).unwrap_or(0), value)?,
            (4, WireValue::Varint(_)) => {}
            (3 | 4, _) => return Err(wrong_wire("instrumentation scope")),
            _ => {}
        }
    }
    facts.pool = pool;
    Ok(text)
}

/// Visits one `ScopeSpans` message and its repeated spans.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid nested fields or limits.
fn visit_scope_spans(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<(), IngestError> {
    let mut scope_text = LastLengths::<2>::default();
    let mut schema_url = LastLengths::<1>::default();
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(scope)) => scope_text.merge(visit_scope(scope, facts)?),
            (2, WireValue::Bytes(span)) => {
                facts.output.open(facts.decode_bytes, size_of::<Span>());
                facts.add_decode_bytes(size_of::<Span>())?;
                facts.pool.spans += 1;
                visit_span(span, facts)?;
                facts
                    .output
                    .close_record(facts.decode_bytes, facts.widths.row);
            }
            (3, value) => schema_url.record(0, value)?,
            (1, _) => return Err(wrong_wire("optional OTLP message")),
            (2, _) => return Err(wrong_wire("scope spans")),
            _ => {}
        }
    }
    scope_text.charge(facts)?;
    schema_url.charge(facts)
}

/// Visits one span, including events, links, and attributes.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid nested fields or limits.
fn visit_span(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<(), IngestError> {
    let mut text = LastLengths::<5>::default();
    let mut status_message = LastLengths::<1>::default();
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1..=5, value) => text.record(usize::try_from(tag - 1).unwrap_or(0), value)?,
            (9, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
            (11, WireValue::Bytes(event)) => {
                facts.output.add(facts.widths.event);
                facts.add_decode_bytes(size_of::<wyrd_tonic::otlp::trace::v1::span::Event>())?;
                facts.pool.events += 1;
                visit_span_event(event, facts)?;
            }
            (13, WireValue::Bytes(link)) => {
                facts.output.add(facts.widths.link);
                facts.add_decode_bytes(size_of::<wyrd_tonic::otlp::trace::v1::span::Link>())?;
                facts.pool.links += 1;
                visit_span_link(link, facts)?;
            }
            (15, WireValue::Bytes(status)) => status_message.merge(visit_status(status, facts)?),
            (15, _) => return Err(wrong_wire("optional OTLP message")),
            (6 | 10 | 12 | 14, WireValue::Varint(_))
            | (7 | 8, WireValue::Fixed64(_))
            | (16, WireValue::Fixed32(_)) => {}
            (6..=16, _) => return Err(wrong_wire("span")),
            _ => {}
        }
    }
    text.charge(facts)?;
    status_message.charge(facts)
}

/// Visits one entity reference and accounts for repeated string elements.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid framing or limits.
fn visit_entity_ref(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<(), IngestError> {
    let mut text = LastLengths::<2>::default();
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1 | 2, value) => text.record(usize::try_from(tag - 1).unwrap_or(0), value)?,
            (3 | 4, WireValue::Bytes(value)) => {
                facts.add_decode_bytes(size_of::<String>())?;
                facts.add_value_bytes(value.len())?;
            }
            (3 | 4, _) => return Err(wrong_wire("entity reference")),
            _ => {}
        }
    }
    text.charge(facts)
}

/// Visits one span status, returning its retained message length, which
/// merges across every occurrence of the status in its span.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid framing or limits.
fn visit_status(bytes: &[u8], facts: &TraceWireFacts) -> Result<LastLengths<1>, IngestError> {
    let mut message = LastLengths::<1>::default();
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (2, value) => message.record(0, value)?,
            (3, WireValue::Varint(_)) => {}
            (3, _) => return Err(wrong_wire("span status")),
            _ => {}
        }
    }
    Ok(message)
}

/// Visits one span event and its repeated attributes.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid nested fields or limits.
fn visit_span_event(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<(), IngestError> {
    let mut name = LastLengths::<1>::default();
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (2, value) => name.record(0, value)?,
            (3, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
            (1, WireValue::Fixed64(_)) | (4, WireValue::Varint(_)) => {}
            (1 | 3 | 4, _) => return Err(wrong_wire("span event")),
            _ => {}
        }
    }
    name.charge(facts)
}

/// Visits one span link and its identifier, trace-state, and attributes.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid nested fields or limits.
fn visit_span_link(bytes: &[u8], facts: &mut TraceWireFacts) -> Result<(), IngestError> {
    let mut text = LastLengths::<3>::default();
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1..=3, value) => text.record(usize::try_from(tag - 1).unwrap_or(0), value)?,
            (4, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
            (5, WireValue::Varint(_)) | (6, WireValue::Fixed32(_)) => {}
            (4..=6, _) => return Err(wrong_wire("span link")),
            _ => {}
        }
    }
    text.charge(facts)
}

/// Visits one key/value attribute and its optional recursive value.
///
/// The entry is walked once. Its value, almost always one occurrence carrying
/// one oneof field, is visited directly; anything protobuf must merge takes
/// the general [`visit_merged_any_value`] path, so both charge the same bytes.
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
    let mut key = LastLengths::<1>::default();
    let mut value_body = None;
    let mut value_count = 0_usize;
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, value) => key.record(0, value)?,
            (2, WireValue::Bytes(body)) => {
                value_body = Some(body);
                value_count = value_count.saturating_add(1);
            }
            (2, _) => return Err(wrong_wire("optional any value")),
            _ => {}
        }
    }
    key.charge(facts)?;
    if let (1, Some(body)) = (value_count, value_body) {
        return visit_any_value(body, value_depth + 1, facts);
    }
    visit_merged_any_value(
        AnyValueBodies::Repeated {
            parent: bytes,
            tag: 2,
        },
        value_depth + 1,
        facts,
    )
}

/// Visits one recursive OTLP `AnyValue` body without constructing it.
///
/// A body carrying exactly one oneof field, the shape every exporter writes,
/// is charged from that field alone; a body protobuf must merge goes through
/// [`visit_merged_any_value`].
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
    let mut only = None;
    let mut oneofs = 0_usize;
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        if (1..=7).contains(&tag) {
            only = Some((tag, value));
            oneofs = oneofs.saturating_add(1);
        }
    }
    let Some((tag, value)) = only.filter(|_| oneofs == 1) else {
        return visit_merged_any_value(AnyValueBodies::Single(bytes), depth, facts);
    };
    validate_any_value_wire(tag, value)?;
    visit_selected_value(tag, value, depth, facts)
}

/// Charges the retained oneof field of one `AnyValue`.
///
/// # Errors
///
/// Returns a stable malformed-request error for invalid nested fields, excess
/// depth, or configured capacity excess.
fn visit_selected_value(
    tag: u32,
    value: WireValue<'_>,
    depth: usize,
    facts: &mut TraceWireFacts,
) -> Result<(), IngestError> {
    match (tag, value) {
        (1 | 7, WireValue::Bytes(value)) => facts.add_value_bytes(value.len()),
        (2 | 3, WireValue::Varint(_)) | (4, WireValue::Fixed64(_)) => Ok(()),
        (5, WireValue::Bytes(array)) => {
            let mut values = WireFields::with_group_depth(array, facts.limits.value_depth);
            while let Some((value_tag, value)) = values.next()? {
                if let (1, WireValue::Bytes(value)) = (value_tag, value) {
                    facts.add_decode_bytes(size_of::<AnyValue>())?;
                    facts.pool.values += 1;
                    visit_any_value(value, depth + 1, facts)?;
                } else if value_tag == 1 {
                    return Err(wrong_wire("OTLP value array"));
                }
            }
            Ok(())
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
            Ok(())
        }
        _ => Err(wrong_wire("OTLP any value")),
    }
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
            visit_selected_value(tag, value, depth, facts)?;
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

/// One message's first decode pass: for each field tag below `N`, how often
/// it occurs as a length-delimited value and its last such body.
///
/// Decoding a message takes this pass and then one pass over its fields that
/// validates every known field's wire type, sets scalars, and pushes repeated
/// children into vectors sized from [`Self::count`]. A singular string or
/// message keeps protobuf's last-wins rule through [`Self::last`], and a
/// singular message occurring once decodes from its one body; one repeated
/// across occurrences merges through its multi-pass fallback.
pub(crate) struct Scanned<'a, const N: usize> {
    /// The scanned message bytes.
    bytes: &'a [u8],
    /// Length-delimited occurrences per tag.
    counts: [usize; N],
    /// Final length-delimited body per tag.
    last: [Option<&'a [u8]>; N],
}

impl<'a, const N: usize> Scanned<'a, N> {
    /// Scans `bytes` once.
    ///
    /// # Errors
    ///
    /// Returns a malformed-request error for invalid framing or count
    /// overflow.
    pub(crate) fn of(bytes: &'a [u8]) -> Result<Self, IngestError> {
        let mut scanned = Self {
            bytes,
            counts: [0; N],
            last: [None; N],
        };
        let mut fields = WireFields::new(bytes);
        while let Some((tag, value)) = fields.next()? {
            let (Some(index), WireValue::Bytes(body)) =
                (usize::try_from(tag).ok().filter(|index| *index < N), value)
            else {
                continue;
            };
            scanned.counts[index] = scanned.counts[index]
                .checked_add(1)
                .ok_or_else(|| malformed("OTLP repeated-field count overflow"))?;
            scanned.last[index] = Some(body);
        }
        Ok(scanned)
    }

    /// Length-delimited occurrences of `tag`.
    pub(crate) fn count(&self, tag: usize) -> usize {
        self.counts[tag]
    }

    /// The final length-delimited body of `tag`.
    pub(crate) fn last(&self, tag: usize) -> Option<&'a [u8]> {
        self.last[tag]
    }

    /// The final `tag` text, or empty when absent.
    ///
    /// # Errors
    ///
    /// Returns a malformed-request error when it is not UTF-8.
    pub(crate) fn string(&self, tag: usize) -> Result<String, IngestError> {
        Ok(self
            .last(tag)
            .map(fixed_string)
            .transpose()?
            .unwrap_or_default())
    }

    /// The final `tag` bytes, or empty when absent.
    pub(crate) fn owned_bytes(&self, tag: usize) -> Vec<u8> {
        self.last(tag).map_or_else(Vec::new, fixed_bytes)
    }

    /// The singular message `tag`: absent, decoded by `single` from its one
    /// body, or merged across every occurrence by `merged`.
    ///
    /// # Errors
    ///
    /// Returns the decoder's malformed-request error.
    pub(crate) fn singular<T>(
        &self,
        tag: u32,
        single: impl FnOnce(&'a [u8]) -> Result<T, IngestError>,
        merged: impl FnOnce(&'a [u8], u32) -> Result<Option<T>, IngestError>,
    ) -> Result<Option<T>, IngestError> {
        let index = usize::try_from(tag).unwrap_or(usize::MAX);
        match (self.counts[index], self.last[index]) {
            (0, _) => Ok(None),
            (1, Some(body)) => single(body).map(Some),
            _ => merged(self.bytes, tag),
        }
    }
}

/// Counts repeated length-delimited messages without retaining descriptors.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, a wrong known wire
/// type, or cardinality overflow.
pub(crate) fn repeated_message_count(bytes: &[u8], wanted_tag: u32) -> Result<usize, IngestError> {
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
pub(crate) fn last_bytes_field(
    bytes: &[u8],
    wanted_tag: u32,
) -> Result<Option<&[u8]>, IngestError> {
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
pub(crate) fn last_bytes_across_messages(
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
pub(crate) fn fixed_string(bytes: &[u8]) -> Result<String, IngestError> {
    let text = std::str::from_utf8(bytes).map_err(|_| malformed("invalid protobuf string"))?;
    let mut value = String::with_capacity(bytes.len());
    value.push_str(text);
    Ok(value)
}

/// Copies bytes into a vector whose capacity is exactly the wire length.
pub(crate) fn fixed_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut value = Vec::with_capacity(bytes.len());
    value.extend_from_slice(bytes);
    value
}

/// Applies protobuf's low-32-bit unsigned varint conversion without a cast.
pub(crate) fn protobuf_u32(value: u64) -> u32 {
    let bytes = value.to_le_bytes();
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Applies protobuf's low-32-bit signed varint conversion without a cast.
pub(crate) fn protobuf_i32(value: u64) -> i32 {
    let bytes = value.to_le_bytes();
    i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Reinterprets one protobuf signed varint's complete two's-complement bits.
pub(crate) fn protobuf_i64(value: u64) -> i64 {
    i64::from_le_bytes(value.to_le_bytes())
}

/// Constructs one resource group and all of its scope groups.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or known wire types.
fn decode_resource_spans(bytes: &[u8]) -> Result<ResourceSpans, IngestError> {
    let scanned = Scanned::<4>::of(bytes)?;
    let resource = scanned.singular(1, decode_resource, decode_merged_resource)?;
    let mut scope_spans = Vec::with_capacity(scanned.count(2));
    let schema_url = scanned.string(3)?;
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (2, WireValue::Bytes(value)) => scope_spans.push(decode_scope_spans(value)?),
            (1 | 3, WireValue::Bytes(_)) => {}
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

/// Constructs one resource from its single encoded occurrence.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or known wire types.
pub(crate) fn decode_resource(bytes: &[u8]) -> Result<Resource, IngestError> {
    let scanned = Scanned::<4>::of(bytes)?;
    let mut resource = Resource {
        attributes: Vec::with_capacity(scanned.count(1)),
        entity_refs: Vec::with_capacity(scanned.count(3)),
        ..Resource::default()
    };
    merge_resource(bytes, &mut resource)?;
    Ok(resource)
}

/// Merges every occurrence of one singular resource message into one value.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or known wire types.
pub(crate) fn decode_merged_resource(
    bytes: &[u8],
    wanted_tag: u32,
) -> Result<Option<Resource>, IngestError> {
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
    let scanned = Scanned::<5>::of(bytes)?;
    let schema_url = scanned.string(1)?;
    let r#type = scanned.string(2)?;
    let mut id_keys = Vec::with_capacity(scanned.count(3));
    let mut description_keys = Vec::with_capacity(scanned.count(4));
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
    let scanned = Scanned::<4>::of(bytes)?;
    let scope = scanned.singular(1, decode_scope, decode_merged_scope)?;
    let mut spans = Vec::with_capacity(scanned.count(2));
    let schema_url = scanned.string(3)?;
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (2, WireValue::Bytes(value)) => spans.push(decode_span(value)?),
            (1 | 3, WireValue::Bytes(_)) => {}
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

/// Constructs one instrumentation scope from its single encoded occurrence.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
pub(crate) fn decode_scope(bytes: &[u8]) -> Result<InstrumentationScope, IngestError> {
    let scanned = Scanned::<5>::of(bytes)?;
    let mut scope = InstrumentationScope {
        name: scanned.string(1)?,
        version: scanned.string(2)?,
        attributes: Vec::with_capacity(scanned.count(3)),
        ..InstrumentationScope::default()
    };
    merge_scope(bytes, &mut scope)?;
    Ok(scope)
}

/// Merges every occurrence of one singular instrumentation scope.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
pub(crate) fn decode_merged_scope(
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
    let scanned = Scanned::<17>::of(bytes)?;
    let mut span = Span {
        trace_id: scanned.owned_bytes(1),
        span_id: scanned.owned_bytes(2),
        trace_state: scanned.string(3)?,
        parent_span_id: scanned.owned_bytes(4),
        name: scanned.string(5)?,
        attributes: Vec::with_capacity(scanned.count(9)),
        events: Vec::with_capacity(scanned.count(11)),
        links: Vec::with_capacity(scanned.count(13)),
        status: scanned.singular(15, decode_status, decode_merged_status)?,
        ..Span::default()
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
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
            (1..=5 | 15, WireValue::Bytes(_)) => {}
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
    let scanned = Scanned::<5>::of(bytes)?;
    let mut event = span::Event {
        name: scanned.string(2)?,
        attributes: Vec::with_capacity(scanned.count(3)),
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
    let scanned = Scanned::<7>::of(bytes)?;
    let mut link = span::Link {
        trace_id: scanned.owned_bytes(1),
        span_id: scanned.owned_bytes(2),
        trace_state: scanned.string(3)?,
        attributes: Vec::with_capacity(scanned.count(4)),
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

/// Constructs one span status from its single encoded occurrence.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_status(bytes: &[u8]) -> Result<Status, IngestError> {
    let mut status = Status {
        message: Scanned::<3>::of(bytes)?.string(2)?,
        ..Status::default()
    };
    merge_status(bytes, &mut status)?;
    Ok(status)
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
///
/// The entry is walked once; a wrongly typed key or value is still refused
/// only after the value and key decode, the order a rescan reports them in.
pub(crate) fn decode_key_value(bytes: &[u8]) -> Result<KeyValue, IngestError> {
    let (mut key, mut value_body, mut values, mut wrong_wire_type) = (None, None, 0_usize, false);
    let mut fields = WireFields::new(bytes);
    while let Some((tag, wire_value)) = fields.next()? {
        match (tag, wire_value) {
            (1, WireValue::Bytes(body)) => key = Some(body),
            (2, WireValue::Bytes(body)) => {
                value_body = Some(body);
                values = values.saturating_add(1);
            }
            (1 | 2, _) => wrong_wire_type = true,
            _ => {}
        }
    }
    let value = match (values, value_body) {
        (0, _) => None,
        (1, Some(body)) => Some(decode_any_value(body)?),
        _ => decode_merged_any_value(bytes, 2)?,
    };
    let key = key.map(fixed_string).transpose()?.unwrap_or_default();
    if wrong_wire_type {
        return Err(wrong_wire("key/value"));
    }
    Ok(KeyValue { key, value })
}

/// Borrowed encoded bodies that merge into one singular `AnyValue` message.
#[derive(Clone, Copy)]
pub(crate) enum AnyValueBodies<'a> {
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
    pub(crate) fn try_for_each(
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
pub(crate) struct AnyValueSelection {
    /// Selected oneof field tag, or zero for an empty value.
    pub(crate) tag: u32,
    /// Ordinal of the first field in the final same-variant merge run.
    pub(crate) run_start: usize,
    /// Repeated nested elements contributed by that final message run.
    pub(crate) nested_count: usize,
}

/// Constructs one recursive OTLP value with fixed nested collection capacity.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
///
/// A body carrying exactly one oneof field, the shape every exporter writes,
/// is built from that field without a separate selection pass; any body
/// protobuf must merge takes [`decode_any_value_bodies`].
pub(crate) fn decode_any_value(bytes: &[u8]) -> Result<AnyValue, IngestError> {
    let mut only = None;
    let mut oneofs = 0_usize;
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if (1..=7).contains(&tag) {
            only = Some((tag, value));
            oneofs = oneofs.saturating_add(1);
        }
    }
    let Some((tag, value)) = only.filter(|_| oneofs == 1) else {
        return decode_any_value_bodies(AnyValueBodies::Single(bytes));
    };
    validate_any_value_wire(tag, value)?;
    let value = match (tag, value) {
        (1, WireValue::Bytes(bytes)) => any_value::Value::StringValue(fixed_string(bytes)?),
        (2, WireValue::Varint(value)) => any_value::Value::BoolValue(value != 0),
        (3, WireValue::Varint(value)) => any_value::Value::IntValue(protobuf_i64(value)),
        (4, WireValue::Fixed64(value)) => any_value::Value::DoubleValue(f64::from_bits(value)),
        (5, WireValue::Bytes(bytes)) => {
            let mut array = ArrayValue {
                values: Vec::with_capacity(repeated_message_count(bytes, 1)?),
            };
            merge_array_value(bytes, &mut array)?;
            any_value::Value::ArrayValue(array)
        }
        (6, WireValue::Bytes(bytes)) => {
            let mut list = KeyValueList {
                values: Vec::with_capacity(repeated_message_count(bytes, 1)?),
            };
            merge_key_value_list(bytes, &mut list)?;
            any_value::Value::KvlistValue(list)
        }
        (7, WireValue::Bytes(bytes)) => any_value::Value::BytesValue(fixed_bytes(bytes)),
        _ => return Err(wrong_wire("OTLP any value")),
    };
    Ok(AnyValue { value: Some(value) })
}

/// Merges every occurrence of a singular `AnyValue` field.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
pub(crate) fn decode_merged_any_value(
    bytes: &[u8],
    wanted_tag: u32,
) -> Result<Option<AnyValue>, IngestError> {
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
pub(crate) fn select_any_value_bodies(
    bodies: AnyValueBodies<'_>,
) -> Result<AnyValueSelection, IngestError> {
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
pub(crate) fn validate_any_value_wire(tag: u32, value: WireValue<'_>) -> Result<(), IngestError> {
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

/// Borrows one preflighted trace export straight from its protobuf wire.
///
/// Text and bytes borrow `bytes`; attributes land in the input's pool. The
/// export is walked once, plus one pass per span for its events and links.
/// Returns `None` when [`decode_trace_protobuf`] must decide instead: for any
/// framing, wire-type, or UTF-8 refusal, and for any encoding protobuf merges,
/// which is a singular message (resource, scope, status, attribute value)
/// occurring twice or an `AnyValue` carrying two oneof fields. Repeated
/// scalars and strings keep protobuf's last-wins rule here.
///
/// `counts` sizes every view and pool exactly, so the input never outgrows
/// the generated request [`OtlpDecodePlan::reservation_bytes`] admitted.
pub(crate) fn decode_trace_input(bytes: &[u8], counts: TracePoolCounts) -> Option<TraceInput<'_>> {
    let mut decoder = TraceInputDecoder {
        wire: bytes,
        input: TraceInput {
            resources: Vec::with_capacity(counts.resources),
            scopes: Vec::with_capacity(counts.scopes),
            spans: Vec::with_capacity(counts.spans),
            events: Vec::with_capacity(counts.events),
            links: Vec::with_capacity(counts.links),
            pool: AttributePool {
                entries: Vec::with_capacity(counts.entries),
                values: Vec::with_capacity(counts.values),
            },
        },
    };
    decoder.request(bytes)?;
    decoder.resolve_nested()?;
    Some(decoder.input)
}

/// One trace protobuf export, borrowed from its wire whenever
/// [`decode_trace_input`] can.
#[derive(Debug)]
pub(crate) enum DecodedTrace<'a> {
    /// Borrowed straight from the wire.
    Borrowed(TraceInput<'a>),
    /// Built by [`decode_trace_protobuf`] for an encoding protobuf merges.
    Generated(Box<ExportTraceServiceRequest>),
}

/// Decodes one preflighted trace export, borrowing it when possible.
///
/// # Errors
///
/// Returns the stable malformed-request error [`decode_trace_protobuf`]
/// returns for a refused export.
pub(crate) fn decode_trace(
    bytes: &[u8],
    counts: TracePoolCounts,
) -> Result<DecodedTrace<'_>, IngestError> {
    match decode_trace_input(bytes, counts) {
        Some(input) => Ok(DecodedTrace::Borrowed(input)),
        None => {
            decode_trace_protobuf(bytes).map(|request| DecodedTrace::Generated(Box::new(request)))
        }
    }
}

impl DecodedTrace<'_> {
    /// Authorize, attribute, and project the export through
    /// [`ServerGate::prepare_trace_input`].
    ///
    /// The returned batch owns its data, so the caller may release the wire
    /// before dispatching it.
    ///
    /// # Errors
    ///
    /// Returns authorization, attribution, or projection errors.
    pub(crate) async fn prepare(
        &self,
        gate: &ServerGate,
        auth: &AuthContext,
    ) -> Result<PreparedTraces, IngestError> {
        match self {
            Self::Borrowed(input) => gate.prepare_trace_input(auth, input).await,
            Self::Generated(request) => {
                gate.prepare_trace_input(auth, &TraceInput::from_generated(&request.resource_spans))
                    .await
            }
        }
    }
}

/// Fills one [`TraceInput`] from protobuf wire; see [`decode_trace_input`].
///
/// A nested array or key/value list is pooled after the list holding it, so
/// every list stays contiguous: until [`Self::resolve_nested`] reaches it, its
/// placeholder range is its body's offsets in `wire`.
#[derive(Debug)]
struct TraceInputDecoder<'a> {
    /// The whole export.
    wire: &'a [u8],
    /// The input being filled.
    input: TraceInput<'a>,
}

impl<'a> TraceInputDecoder<'a> {
    /// Walks the export's resource groups.
    fn request(&mut self, bytes: &'a [u8]) -> Option<()> {
        let mut fields = WireFields::new(bytes);
        while let Some((tag, value)) = fields.next().ok()? {
            match (tag, value) {
                (1, WireValue::Bytes(group)) => self.resource_spans(group)?,
                (1, _) => return None,
                _ => {}
            }
        }
        Some(())
    }

    /// Walks one resource group and its scope groups.
    fn resource_spans(&mut self, bytes: &'a [u8]) -> Option<()> {
        let first_scope = self.input.scopes.len();
        let (mut resource, mut resources, mut schema_url) = (None, 0_usize, "");
        let mut fields = WireFields::new(bytes);
        while let Some((tag, value)) = fields.next().ok()? {
            match (tag, value) {
                (1, WireValue::Bytes(body)) => (resource, resources) = (Some(body), resources + 1),
                (2, WireValue::Bytes(body)) => self.scope_spans(body)?,
                (3, WireValue::Bytes(body)) => schema_url = text(body)?,
                (1..=3, _) => return None,
                _ => {}
            }
        }
        if resources > 1 {
            return None;
        }
        self.input.resources.push(ResourceInput {
            resource: match resource {
                Some(body) => Some(Cow::Owned(decode_resource(body).ok()?)),
                None => None,
            },
            schema_url,
            scopes: first_scope..self.input.scopes.len(),
        });
        Some(())
    }

    /// Walks one scope group and its spans.
    fn scope_spans(&mut self, bytes: &'a [u8]) -> Option<()> {
        let first_span = self.input.spans.len();
        let (mut scope, mut scopes, mut schema_url) = (None, 0_usize, "");
        let mut fields = WireFields::new(bytes);
        while let Some((tag, value)) = fields.next().ok()? {
            match (tag, value) {
                (1, WireValue::Bytes(body)) => (scope, scopes) = (Some(body), scopes + 1),
                (2, WireValue::Bytes(body)) => self.span(body)?,
                (3, WireValue::Bytes(body)) => schema_url = text(body)?,
                (1..=3, _) => return None,
                _ => {}
            }
        }
        if scopes > 1 {
            return None;
        }
        self.input.scopes.push(ScopeInput {
            scope: match scope {
                Some(body) => Some(Cow::Owned(decode_scope(body).ok()?)),
                None => None,
            },
            schema_url,
            spans: first_span..self.input.spans.len(),
        });
        Some(())
    }

    /// Walks one span, then its events and links, so each list stays
    /// contiguous.
    fn span(&mut self, bytes: &'a [u8]) -> Option<()> {
        let first_entry = self.input.pool.entries.len();
        let mut span = SpanInput {
            trace_id: &[],
            span_id: &[],
            trace_state: "",
            parent_span_id: &[],
            flags: 0,
            name: "",
            kind: 0,
            start_time_unix_nano: 0,
            end_time_unix_nano: 0,
            attributes: AttributeList::Pooled(PoolRange::default()),
            dropped_attributes_count: 0,
            events: 0..0,
            dropped_events_count: 0,
            links: 0..0,
            dropped_links_count: 0,
            status: None,
        };
        let (mut status, mut statuses) = (None, 0_usize);
        let mut fields = WireFields::new(bytes);
        while let Some((tag, value)) = fields.next().ok()? {
            match (tag, value) {
                (1, WireValue::Bytes(body)) => span.trace_id = body,
                (2, WireValue::Bytes(body)) => span.span_id = body,
                (3, WireValue::Bytes(body)) => span.trace_state = text(body)?,
                (4, WireValue::Bytes(body)) => span.parent_span_id = body,
                (5, WireValue::Bytes(body)) => span.name = text(body)?,
                (6, WireValue::Varint(value)) => span.kind = protobuf_i32(value),
                (7, WireValue::Fixed64(value)) => span.start_time_unix_nano = value,
                (8, WireValue::Fixed64(value)) => span.end_time_unix_nano = value,
                (9, WireValue::Bytes(body)) => self.entry(body)?,
                (10, WireValue::Varint(value)) => {
                    span.dropped_attributes_count = protobuf_u32(value);
                }
                (11 | 13, WireValue::Bytes(_)) => {}
                (12, WireValue::Varint(value)) => span.dropped_events_count = protobuf_u32(value),
                (14, WireValue::Varint(value)) => span.dropped_links_count = protobuf_u32(value),
                (15, WireValue::Bytes(body)) => (status, statuses) = (Some(body), statuses + 1),
                (16, WireValue::Fixed32(value)) => span.flags = value,
                (1..=16, _) => return None,
                _ => {}
            }
        }
        if statuses > 1 {
            return None;
        }
        span.attributes = AttributeList::Pooled(self.range_from(first_entry));
        span.status = match status {
            Some(body) => Some(status_input(body)?),
            None => None,
        };
        let (first_event, first_link) = (self.input.events.len(), self.input.links.len());
        let mut fields = WireFields::new(bytes);
        while let Some((tag, value)) = fields.next().ok()? {
            match (tag, value) {
                (11, WireValue::Bytes(body)) => self.event(body)?,
                (13, WireValue::Bytes(body)) => self.link(body)?,
                _ => {}
            }
        }
        span.events = first_event..self.input.events.len();
        span.links = first_link..self.input.links.len();
        self.input.spans.push(span);
        Some(())
    }

    /// Walks one span event.
    fn event(&mut self, bytes: &'a [u8]) -> Option<()> {
        let first_entry = self.input.pool.entries.len();
        let (mut time_unix_nano, mut name, mut dropped_attributes_count) = (0, "", 0);
        let mut fields = WireFields::new(bytes);
        while let Some((tag, value)) = fields.next().ok()? {
            match (tag, value) {
                (1, WireValue::Fixed64(value)) => time_unix_nano = value,
                (2, WireValue::Bytes(body)) => name = text(body)?,
                (3, WireValue::Bytes(body)) => self.entry(body)?,
                (4, WireValue::Varint(value)) => dropped_attributes_count = protobuf_u32(value),
                (1..=4, _) => return None,
                _ => {}
            }
        }
        let attributes = AttributeList::Pooled(self.range_from(first_entry));
        self.input.events.push(EventInput {
            time_unix_nano,
            name,
            attributes,
            dropped_attributes_count,
        });
        Some(())
    }

    /// Walks one span link.
    fn link(&mut self, bytes: &'a [u8]) -> Option<()> {
        let first_entry = self.input.pool.entries.len();
        let mut link = LinkInput {
            trace_id: &[],
            span_id: &[],
            trace_state: "",
            attributes: AttributeList::Pooled(PoolRange::default()),
            dropped_attributes_count: 0,
            flags: 0,
        };
        let mut fields = WireFields::new(bytes);
        while let Some((tag, value)) = fields.next().ok()? {
            match (tag, value) {
                (1, WireValue::Bytes(body)) => link.trace_id = body,
                (2, WireValue::Bytes(body)) => link.span_id = body,
                (3, WireValue::Bytes(body)) => link.trace_state = text(body)?,
                (4, WireValue::Bytes(body)) => self.entry(body)?,
                (5, WireValue::Varint(value)) => {
                    link.dropped_attributes_count = protobuf_u32(value);
                }
                (6, WireValue::Fixed32(value)) => link.flags = value,
                (1..=6, _) => return None,
                _ => {}
            }
        }
        link.attributes = AttributeList::Pooled(self.range_from(first_entry));
        self.input.links.push(link);
        Some(())
    }

    /// Pools one key/value entry.
    fn entry(&mut self, bytes: &'a [u8]) -> Option<()> {
        let (mut key, mut value, mut values) = ("", None, 0_usize);
        let mut fields = WireFields::new(bytes);
        while let Some((tag, wire)) = fields.next().ok()? {
            match (tag, wire) {
                (1, WireValue::Bytes(body)) => key = text(body)?,
                (2, WireValue::Bytes(body)) => (value, values) = (Some(body), values + 1),
                (1 | 2, _) => return None,
                _ => {}
            }
        }
        let value = match (values, value) {
            (0, _) => PooledValue::Empty,
            (1, Some(body)) => self.value(body)?,
            _ => return None,
        };
        self.input.pool.entries.push(PooledEntry { key, value });
        Some(())
    }

    /// Reads one `AnyValue`, leaving a nested list as a placeholder.
    fn value(&self, bytes: &'a [u8]) -> Option<PooledValue<'a>> {
        let (mut only, mut oneofs) = (None, 0_usize);
        let mut fields = WireFields::new(bytes);
        while let Some((tag, value)) = fields.next().ok()? {
            if (1..=7).contains(&tag) {
                (only, oneofs) = (Some((tag, value)), oneofs + 1);
            }
        }
        let Some((tag, value)) = only else {
            return Some(PooledValue::Empty);
        };
        if oneofs > 1 {
            return None;
        }
        Some(match (tag, value) {
            (1, WireValue::Bytes(body)) => PooledValue::String(text(body)?),
            (2, WireValue::Varint(value)) => PooledValue::Bool(value != 0),
            (3, WireValue::Varint(value)) => PooledValue::Int(protobuf_i64(value)),
            (4, WireValue::Fixed64(value)) => PooledValue::Double(f64::from_bits(value)),
            (5, WireValue::Bytes(body)) => PooledValue::Array(self.wire_range(body)),
            (6, WireValue::Bytes(body)) => PooledValue::Kvlist(self.wire_range(body)),
            (7, WireValue::Bytes(body)) => PooledValue::Bytes(body),
            _ => return None,
        })
    }

    /// The offsets of `body`, a subslice of the export, in [`Self::wire`].
    fn wire_range(&self, body: &[u8]) -> PoolRange {
        let start = body.as_ptr().addr() - self.wire.as_ptr().addr();
        PoolRange {
            start,
            end: start + body.len(),
        }
    }

    /// Pools every placeholder list and points it at its items.
    ///
    /// One cursor per pool visits each value once; a list's items are
    /// appended behind the cursor, so their own placeholders are visited too.
    fn resolve_nested(&mut self) -> Option<()> {
        let (mut entry, mut item) = (0, 0);
        loop {
            if entry < self.input.pool.entries.len() {
                let value = self.resolve(self.input.pool.entries[entry].value)?;
                self.input.pool.entries[entry].value = value;
                entry += 1;
            } else if item < self.input.pool.values.len() {
                let value = self.resolve(self.input.pool.values[item])?;
                self.input.pool.values[item] = value;
                item += 1;
            } else {
                return Some(());
            }
        }
    }

    /// Pools a placeholder list's items; any other value is returned as is.
    fn resolve(&mut self, value: PooledValue<'a>) -> Option<PooledValue<'a>> {
        Some(match value {
            PooledValue::Array(body) => PooledValue::Array(self.array(body)?),
            PooledValue::Kvlist(body) => PooledValue::Kvlist(self.kvlist(body)?),
            other => other,
        })
    }

    /// Pools the items of the `ArrayValue` at `body` in the wire.
    fn array(&mut self, body: PoolRange) -> Option<PoolRange> {
        let start = self.input.pool.values.len();
        let mut fields = WireFields::new(&self.wire[body.start..body.end]);
        while let Some((tag, wire)) = fields.next().ok()? {
            match (tag, wire) {
                (1, WireValue::Bytes(item)) => {
                    let item = self.value(item)?;
                    self.input.pool.values.push(item);
                }
                (1, _) => return None,
                _ => {}
            }
        }
        Some(PoolRange {
            start,
            end: self.input.pool.values.len(),
        })
    }

    /// Pools the entries of the `KeyValueList` at `body` in the wire.
    fn kvlist(&mut self, body: PoolRange) -> Option<PoolRange> {
        let start = self.input.pool.entries.len();
        let mut fields = WireFields::new(&self.wire[body.start..body.end]);
        while let Some((tag, wire)) = fields.next().ok()? {
            match (tag, wire) {
                (1, WireValue::Bytes(entry)) => self.entry(entry)?,
                (1, _) => return None,
                _ => {}
            }
        }
        Some(self.range_from(start))
    }

    /// The pooled entries pushed since `start`.
    fn range_from(&self, start: usize) -> PoolRange {
        PoolRange {
            start,
            end: self.input.pool.entries.len(),
        }
    }
}

/// Reads one span status.
fn status_input(bytes: &[u8]) -> Option<StatusInput<'_>> {
    let mut status = StatusInput {
        message: "",
        code: 0,
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next().ok()? {
        match (tag, value) {
            (2, WireValue::Bytes(body)) => status.message = text(body)?,
            (3, WireValue::Varint(value)) => status.code = protobuf_i32(value),
            (2 | 3, _) => return None,
            _ => {}
        }
    }
    Some(status)
}

/// Borrows one protobuf string, refusing invalid UTF-8.
fn text(bytes: &[u8]) -> Option<&str> {
    std::str::from_utf8(bytes).ok()
}

/// Constructs the stable malformed error for a known field's wire mismatch.
pub(crate) fn wrong_wire(field: &str) -> IngestError {
    malformed(&format!("invalid protobuf wire type for {field}"))
}

/// Constructs the existing stable malformed OTLP refusal.
pub(crate) fn malformed(message: &str) -> IngestError {
    IngestError::Decode(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span, status};
    use wyrd_tonic::prost::Message;

    /// The trace preflight's plan alone, shaped like the other signals'.
    fn trace_plan(bytes: &[u8], limits: OtlpWireLimits) -> Result<OtlpDecodePlan, IngestError> {
        preflight_trace_protobuf(bytes, limits).map(|(plan, _)| plan)
    }

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

    /// Varints read the value prost encodes and advance past it at the one-
    /// and multi-byte boundaries, and truncation, an eleventh byte, and a
    /// tenth-byte overflow stay malformed.
    ///
    /// # Panics
    /// Panics when a value, cursor, or refusal differs.
    #[test]
    fn varints_read_boundaries_and_refuse_malformed_input() {
        for value in [
            0,
            1,
            0x7f,
            0x80,
            0x3fff,
            0x4000,
            u64::from(u32::MAX),
            u64::MAX,
        ] {
            let mut bytes = Vec::new();
            push_varint(&mut bytes, value);
            bytes.push(0x2a);
            let mut fields = WireFields::new(&bytes);
            assert_eq!(fields.read_varint().expect("canonical varint"), value);
            assert_eq!(fields.cursor, bytes.len() - 1, "{value}");
        }
        let mut overflow = vec![0xff; 9];
        overflow.push(0x02);
        let mut eleven = vec![0x80; 10];
        eleven.push(0x00);
        for bytes in [&[][..], &[0x80][..], &overflow, &eleven] {
            assert!(WireFields::new(bytes).read_varint().is_err(), "{bytes:?}");
        }
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
        let (plan, _) =
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

    /// Builds one trace export using every generated trace field.
    fn rich_trace_request() -> ExportTraceServiceRequest {
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
        ExportTraceServiceRequest {
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
        }
    }

    /// Proves the fixed constructor preserves rich generated trace semantics.
    ///
    /// # Panics
    ///
    /// Panics when the fixture fails to encode or decode identically.
    #[test]
    fn trace_fixed_decode_round_trips_all_generated_shapes() {
        let request = rich_trace_request();
        let mut bytes = request.encode_to_vec();
        bytes.extend_from_slice(&[0xa0, 0x06, 0x01]);
        bytes.extend_from_slice(&[0xab, 0x06, 0x08, 0x01, 0xac, 0x06]);
        let (plan, _) =
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
        let (plan, _) = preflight_trace_protobuf(
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

    /// Repeated singular strings keep the last value as prost does, and a
    /// singular string or message field sent with a non-length-delimited
    /// wire type is refused.
    ///
    /// # Panics
    ///
    /// Panics when a duplicate decodes differently from prost or a wrong wire
    /// type decodes.
    #[test]
    fn trace_fixed_decode_keeps_last_strings_and_refuses_wrong_wire_types() {
        let mut span_wire = Span {
            name: "first".to_owned(),
            ..Span::default()
        }
        .encode_to_vec();
        span_wire.extend(
            Span {
                name: "retained".to_owned(),
                trace_state: "state".to_owned(),
                ..Span::default()
            }
            .encode_to_vec(),
        );
        let decoded = decode_span(&span_wire).expect("duplicate span strings must decode");
        assert_eq!(
            decoded,
            Span::decode(span_wire.as_slice()).expect("prost must decode duplicates")
        );
        assert_eq!(decoded.name, "retained");

        for tag in [5_u64, 15] {
            let mut wire = Vec::new();
            push_varint(&mut wire, tag << 3);
            push_varint(&mut wire, 1);
            assert!(
                decode_span(&wire).is_err(),
                "span tag {tag} varint must be refused"
            );
        }
        let mut key_value_wire = Vec::new();
        push_varint(&mut key_value_wire, 2 << 3);
        push_varint(&mut key_value_wire, 1);
        assert!(decode_key_value(&key_value_wire).is_err());
        let mut resource_wire = Vec::new();
        push_varint(&mut resource_wire, 1 << 3);
        push_varint(&mut resource_wire, 1);
        assert!(decode_resource_spans(&resource_wire).is_err());
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

    /// One OTLP protobuf preflight under test.
    type Preflight = fn(&[u8], OtlpWireLimits) -> Result<OtlpDecodePlan, IngestError>;

    /// Wire limits whose derived expanded ceiling is 16 KiB.
    fn small_limits() -> OtlpWireLimits {
        OtlpWireLimits {
            request_bytes: 4096,
            ..vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS
        }
    }

    /// Returns a resource carrying four 400-byte attribute keys, which every
    /// projected row repeats.
    fn wide_resource() -> Resource {
        Resource {
            attributes: (0..4)
                .map(|index| KeyValue {
                    key: format!("{index}").repeat(400),
                    value: None,
                })
                .collect(),
            ..Resource::default()
        }
    }

    /// Encodes one export per signal whose resource is repeated by `rows` rows.
    fn fan_out_requests(rows: usize) -> [(Preflight, Vec<u8>); 3] {
        use wyrd_tonic::otlp::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
        use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
        use wyrd_tonic::otlp::metrics::v1::{
            Gauge, Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics, metric,
        };
        use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;

        let traces = ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: Some(wide_resource()),
                scope_spans: vec![ScopeSpans {
                    spans: vec![Span::default(); rows],
                    ..ScopeSpans::default()
                }],
                ..ResourceSpans::default()
            }],
        };
        let logs = ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                resource: Some(wide_resource()),
                scope_logs: vec![ScopeLogs {
                    log_records: vec![LogRecord::default(); rows],
                    ..ScopeLogs::default()
                }],
                ..ResourceLogs::default()
            }],
        };
        let metrics = ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(wide_resource()),
                scope_metrics: vec![ScopeMetrics {
                    metrics: vec![Metric {
                        name: "m".to_owned(),
                        data: Some(metric::Data::Gauge(Gauge {
                            data_points: vec![NumberDataPoint::default(); rows],
                        })),
                        ..Metric::default()
                    }],
                    ..ScopeMetrics::default()
                }],
                ..ResourceMetrics::default()
            }],
        };
        [
            (trace_plan as Preflight, traces.encode_to_vec()),
            (
                crate::otlp_logs_decode::preflight_logs_protobuf,
                logs.encode_to_vec(),
            ),
            (
                crate::otlp_metrics_decode::preflight_metrics_protobuf,
                metrics.encode_to_vec(),
            ),
        ]
    }

    /// Proves a small request whose empty nested messages expand past four
    /// times the wire ceiling in generated backing is refused as too large.
    ///
    /// # Panics
    ///
    /// Panics when the fixture exceeds the wire ceiling or the preflight
    /// accepts it.
    #[test]
    fn protobuf_preflight_refuses_typed_backing_above_expanded_ceiling() {
        use wyrd_tonic::otlp::logs::v1::ResourceLogs;
        use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
        use wyrd_tonic::otlp::metrics::v1::ResourceMetrics;
        use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;

        let limits = small_limits();
        let groups = 2000;
        let requests: [(Preflight, Vec<u8>, usize); 3] = [
            (
                trace_plan,
                ExportTraceServiceRequest {
                    resource_spans: vec![ResourceSpans::default(); groups],
                }
                .encode_to_vec(),
                size_of::<ResourceSpans>(),
            ),
            (
                crate::otlp_logs_decode::preflight_logs_protobuf,
                ExportLogsServiceRequest {
                    resource_logs: vec![ResourceLogs::default(); groups],
                }
                .encode_to_vec(),
                size_of::<ResourceLogs>(),
            ),
            (
                crate::otlp_metrics_decode::preflight_metrics_protobuf,
                ExportMetricsServiceRequest {
                    resource_metrics: vec![ResourceMetrics::default(); groups],
                }
                .encode_to_vec(),
                size_of::<ResourceMetrics>(),
            ),
        ];
        for (preflight, bytes, layout) in requests {
            assert!(bytes.len() <= limits.request_bytes);
            assert!(groups * layout > limits.expanded_bytes());
            assert!(matches!(
                preflight(&bytes, limits),
                Err(IngestError::PayloadTooLarge { .. })
            ));
            assert!(preflight(&bytes[..2 * 16], limits).is_ok());
        }
    }

    /// Proves repeated resource attributes are charged once per projected row:
    /// one row fits the expanded ceiling, while four rows cross it on
    /// projected output alone even though generated backing still fits.
    ///
    /// # Panics
    ///
    /// Panics when the fixture exceeds the wire ceiling, the one-row request
    /// is refused, or the four-row request is accepted.
    #[test]
    fn protobuf_preflight_refuses_projected_fan_out_above_expanded_ceiling() {
        let limits = small_limits();
        let generous = OtlpWireLimits {
            request_bytes: 1 << 20,
            ..limits
        };
        for ((preflight, one_row), (_, four_rows)) in
            fan_out_requests(1).into_iter().zip(fan_out_requests(4))
        {
            let plan = preflight(&one_row, limits).expect("one row fits");
            assert!(plan.projected_bytes <= limits.expanded_bytes());
            assert!(four_rows.len() <= limits.request_bytes);
            let unbounded = preflight(&four_rows, generous).expect("generous preflight");
            assert!(unbounded.decode_bytes <= limits.expanded_bytes());
            assert!(unbounded.projected_bytes > limits.expanded_bytes());
            assert!(matches!(
                preflight(&four_rows, limits),
                Err(IngestError::PayloadTooLarge { .. })
            ));
        }
    }
    /// Encodes one length-delimited protobuf field.
    fn field(tag: u32, body: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        wyrd_tonic::prost::encoding::encode_key(
            tag,
            wyrd_tonic::prost::encoding::WireType::LengthDelimited,
            &mut bytes,
        );
        wyrd_tonic::prost::encoding::encode_varint(body.len() as u64, &mut bytes);
        bytes.extend_from_slice(body);
        bytes
    }

    /// Wraps one encoded span body in a one-span trace export.
    fn export_of_span(span: &[u8]) -> Vec<u8> {
        field(1, &field(2, &field(2, span)))
    }

    /// Builds spans covering nesting, duplicate keys, empty values, a Card
    /// reference, and a span projection rejects.
    fn edge_trace_request() -> ExportTraceServiceRequest {
        let card = "0190f7a4-1c2b-7d3e-8f40-123456789abc";
        let good = Span {
            trace_id: vec![1; 16],
            span_id: vec![2; 8],
            name: "edge".to_owned(),
            attributes: vec![
                attribute("dup", any_value::Value::IntValue(1)),
                KeyValue {
                    key: "absent".to_owned(),
                    value: None,
                },
                KeyValue {
                    key: "unset".to_owned(),
                    value: Some(AnyValue { value: None }),
                },
                KeyValue {
                    key: "deep".to_owned(),
                    value: Some(alternating_value(7)),
                },
                attribute(
                    "wyrd.card.uid",
                    any_value::Value::StringValue(card.to_owned()),
                ),
                attribute("dup", any_value::Value::StringValue("last".to_owned())),
            ],
            events: vec![span::Event {
                name: "exception".to_owned(),
                attributes: vec![
                    attribute(
                        "exception.type",
                        any_value::Value::StringValue("Boom".to_owned()),
                    ),
                    KeyValue {
                        key: "nested".to_owned(),
                        value: Some(alternating_value(5)),
                    },
                ],
                ..span::Event::default()
            }],
            ..Span::default()
        };
        let rejected = Span {
            trace_id: vec![0; 3],
            ..good.clone()
        };
        ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                scope_spans: vec![ScopeSpans {
                    spans: vec![good.clone(), rejected, good],
                    ..ScopeSpans::default()
                }],
                ..ResourceSpans::default()
            }],
        }
    }

    /// Computes the heap one borrowed trace input holds, owned resources and
    /// scopes included.
    fn borrowed_trace_capacity(input: &TraceInput<'_>) -> usize {
        use vala_bifrost_redux::tables::traces::input::{EventInput, LinkInput, ResourceInput};
        input.resources.capacity() * size_of::<ResourceInput<'_>>()
            + input
                .resources
                .iter()
                .filter_map(|group| group.resource.as_deref())
                .map(decoded_resource_capacity)
                .sum::<usize>()
            + input.scopes.capacity() * size_of::<ScopeInput<'_>>()
            + input
                .scopes
                .iter()
                .filter_map(|group| group.scope.as_deref())
                .map(decoded_scope_capacity)
                .sum::<usize>()
            + input.spans.capacity() * size_of::<SpanInput<'_>>()
            + input.events.capacity() * size_of::<EventInput<'_>>()
            + input.links.capacity() * size_of::<LinkInput<'_>>()
            + input.pool.entries.capacity() * size_of::<PooledEntry<'_>>()
            + input.pool.values.capacity() * size_of::<PooledValue<'_>>()
    }

    /// Builds the shapes generated decode stores most compactly: empty
    /// attributes and nested one-item arrays.
    fn compact_trace_request() -> ExportTraceServiceRequest {
        let nested = AnyValue {
            value: Some(any_value::Value::ArrayValue(ArrayValue {
                values: vec![AnyValue {
                    value: Some(any_value::Value::ArrayValue(ArrayValue {
                        values: vec![AnyValue::default()],
                    })),
                }],
            })),
        };
        let mut attributes = vec![KeyValue::default(); 64];
        attributes.push(KeyValue {
            key: String::new(),
            value: Some(AnyValue {
                value: Some(any_value::Value::ArrayValue(ArrayValue {
                    values: vec![nested; 64],
                })),
            }),
        });
        ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: Some(Resource {
                    attributes: vec![KeyValue::default(); 8],
                    ..Resource::default()
                }),
                scope_spans: vec![ScopeSpans {
                    spans: vec![
                        Span {
                            attributes,
                            events: vec![span::Event::default(); 8],
                            links: vec![span::Link::default(); 8],
                            ..Span::default()
                        };
                        4
                    ],
                    ..ScopeSpans::default()
                }],
                ..ResourceSpans::default()
            }],
        }
    }

    /// Proves a borrowed decode projects and attributes exactly like the
    /// generated decode of the same wire.
    ///
    /// # Panics
    ///
    /// Panics when a fixture falls back or either projection differs.
    #[test]
    fn borrowed_trace_decode_projects_like_generated_decode() {
        use vala_bifrost_redux::gate::attribution::span_card_uids;
        use vala_bifrost_redux::tables::traces::project_trace_input;
        let mut rich = rich_trace_request().encode_to_vec();
        rich.extend_from_slice(&[0xa0, 0x06, 0x01]);
        rich.extend_from_slice(&[0xab, 0x06, 0x08, 0x01, 0xac, 0x06]);
        let many = ExportTraceServiceRequest {
            resource_spans: (0..3)
                .map(|resource| ResourceSpans {
                    scope_spans: (0..2)
                        .map(|scope| ScopeSpans {
                            spans: (0..4)
                                .map(|span| Span {
                                    trace_id: vec![resource + 1; 16],
                                    span_id: vec![scope + 1; 8],
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
        for bytes in [
            rich,
            many.encode_to_vec(),
            edge_trace_request().encode_to_vec(),
            compact_trace_request().encode_to_vec(),
            Vec::new(),
        ] {
            let generated = decode_trace_protobuf(&bytes).expect("fixture decodes");
            let generated_input = TraceInput::from_generated(&generated.resource_spans);
            let (plan, counts) = preflight_trace_protobuf(
                &bytes,
                vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS,
            )
            .expect("fixture preflights");
            let borrowed = decode_trace_input(&bytes, counts).expect("fixture borrows");
            assert!(
                borrowed_trace_capacity(&borrowed) <= plan.reservation_bytes(),
                "a borrowed decode must fit the generated reservation"
            );
            let (expected, expected_outcome) =
                project_trace_input(&generated_input, None, usize::MAX)
                    .expect("generated projects");
            let (batch, outcome) =
                project_trace_input(&borrowed, None, usize::MAX).expect("borrowed projects");
            assert_eq!(batch, expected);
            assert_eq!(outcome.accepted_spans, expected_outcome.accepted_spans);
            assert_eq!(outcome.rejected_spans, expected_outcome.rejected_spans);
            assert_eq!(
                outcome.rejection_message,
                expected_outcome.rejection_message
            );
            assert_eq!(span_card_uids(&borrowed), span_card_uids(&generated_input));
        }
    }

    /// Proves wire that protobuf must merge, or that the generated decoder
    /// refuses, is left to the generated decoder.
    ///
    /// # Panics
    ///
    /// Panics when the borrowed decoder accepts a merge or refused encoding.
    #[test]
    fn borrowed_trace_decode_defers_merges_and_refusals() {
        let span = Span {
            trace_id: vec![1; 16],
            span_id: vec![2; 8],
            name: "merge".to_owned(),
            ..Span::default()
        }
        .encode_to_vec();
        let with = |extra: &[u8]| export_of_span(&[span.as_slice(), extra].concat());
        let text = |value: &str| {
            AnyValue {
                value: Some(any_value::Value::StringValue(value.to_owned())),
            }
            .encode_to_vec()
        };
        let statuses = [
            field(
                15,
                &Status {
                    message: "a".to_owned(),
                    code: 0,
                }
                .encode_to_vec(),
            ),
            field(
                15,
                &Status {
                    message: String::new(),
                    code: 2,
                }
                .encode_to_vec(),
            ),
        ]
        .concat();
        let two_values = field(
            9,
            &[field(1, b"key"), field(2, &text("a")), field(2, &text("b"))].concat(),
        );
        let two_oneofs = field(
            9,
            &[
                field(1, b"key"),
                field(2, &[text("a"), vec![0x18, 0x03]].concat()),
            ]
            .concat(),
        );
        let resource = Resource::default().encode_to_vec();
        let two_resources = field(
            1,
            &[
                field(1, &resource),
                field(1, &resource),
                field(2, &field(2, &span)),
            ]
            .concat(),
        );
        let two_scopes = field(
            1,
            &field(
                2,
                &[
                    field(1, &InstrumentationScope::default().encode_to_vec()),
                    field(1, &InstrumentationScope::default().encode_to_vec()),
                    field(2, &span),
                ]
                .concat(),
            ),
        );
        for bytes in [
            with(&statuses),
            with(&two_values),
            with(&two_oneofs),
            two_resources,
            two_scopes,
        ] {
            decode_trace_protobuf(&bytes).expect("merge fixture decodes");
            assert!(decode_trace_input(&bytes, TracePoolCounts::default()).is_none());
        }
        let invalid_name = with(&field(5, &[0xff]));
        assert!(decode_trace_protobuf(&invalid_name).is_err());
        assert!(decode_trace_input(&invalid_name, TracePoolCounts::default()).is_none());
    }
}
