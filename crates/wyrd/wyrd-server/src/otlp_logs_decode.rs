//! Fixed-capacity OTLP logs protobuf preflight and typed construction.

use std::mem::size_of;

use vala_bifrost_redux::gate::{IngestError, OtlpWireLimits};
use wyrd_tonic::otlp::common::v1::{
    AnyValue, ArrayValue, EntityRef, InstrumentationScope, KeyValue, KeyValueList, any_value,
};
use wyrd_tonic::otlp::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
use wyrd_tonic::otlp::resource::v1::Resource;

/// Maximum deprecated protobuf group nesting accepted by the bounded scanner.
const MAX_WIRE_GROUP_DEPTH: usize = 8;

/// Exact adapter facts established before generated logs messages are allocated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OtlpLogsDecodePlan {
    /// Encoded protobuf bytes inspected by the preflight.
    pub(crate) wire_bytes: usize,
    /// Exact live typed-request capacity that the constructor will retain.
    pub(crate) decode_bytes: usize,
}

/// Bounded counters accumulated while walking one logs export.
#[derive(Debug)]
struct LogsWireFacts {
    /// Immutable limits shared by the adapter and Scribe admission.
    limits: OtlpWireLimits,
    /// Resource group count.
    resources: usize,
    /// Scope group count.
    scopes: usize,
    /// Log record count.
    records: usize,
    /// Attribute count across resources, scopes, records, and nested values.
    attributes: usize,
    /// Retained variable-width bytes across the generated request.
    value_bytes: usize,
    /// Exact public layout plus retained backing capacity.
    decode_bytes: usize,
}

impl LogsWireFacts {
    /// Starts a preflight with the root generated request layout charged.
    fn new(limits: OtlpWireLimits) -> Self {
        Self {
            limits,
            resources: 0,
            scopes: 0,
            records: 0,
            attributes: 0,
            value_bytes: 0,
            decode_bytes: size_of::<ExportLogsServiceRequest>(),
        }
    }

    /// Adds one bounded cardinality or byte fact.
    ///
    /// # Errors
    ///
    /// Returns a malformed refusal on arithmetic overflow or limit excess.
    fn add_count(current: &mut usize, amount: usize, limit: usize) -> Result<(), IngestError> {
        *current = current
            .checked_add(amount)
            .ok_or_else(|| malformed("OTLP protobuf cardinality overflow"))?;
        if *current > limit {
            return Err(malformed("OTLP protobuf cardinality limit exceeded"));
        }
        Ok(())
    }

    /// Charges retained variable-width backing to both byte ceilings.
    ///
    /// # Errors
    ///
    /// Returns a malformed refusal on overflow or configured limit excess.
    fn add_value_bytes(&mut self, amount: usize) -> Result<(), IngestError> {
        Self::add_count(&mut self.value_bytes, amount, self.limits.value_bytes)?;
        self.add_decode_bytes(amount)
    }

    /// Charges public layouts or backing capacity to the decode owner.
    ///
    /// # Errors
    ///
    /// Returns a malformed refusal on overflow or material-limit excess.
    fn add_decode_bytes(&mut self, amount: usize) -> Result<(), IngestError> {
        Self::add_count(&mut self.decode_bytes, amount, self.limits.material_bytes)
    }

    /// Charges one retained key/value element before its nested payload.
    ///
    /// # Errors
    ///
    /// Returns a malformed refusal when attribute or material limits are exceeded.
    fn add_attribute(&mut self) -> Result<(), IngestError> {
        Self::add_count(&mut self.attributes, 1, self.limits.attributes)?;
        self.add_decode_bytes(size_of::<KeyValue>())
    }
}

/// One parsed protobuf field borrowing the transport buffer.
#[derive(Clone, Copy, Debug)]
enum WireValue<'a> {
    /// Varint value.
    Varint(u64),
    /// Fixed-width 64-bit value.
    Fixed64(u64),
    /// Length-delimited bytes.
    Bytes(&'a [u8]),
    /// Fixed-width 32-bit value.
    Fixed32(u32),
    /// Deprecated group that was framing-validated and skipped.
    Group,
}

/// Allocation-free protobuf cursor with bounded unknown-group state.
#[derive(Debug)]
struct WireFields<'a> {
    /// Complete borrowed message.
    bytes: &'a [u8],
    /// Next unread offset.
    cursor: usize,
    /// Maximum unknown-group nesting.
    group_depth_limit: usize,
}

impl<'a> WireFields<'a> {
    /// Creates a cursor using the canonical hard group ceiling.
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            cursor: 0,
            group_depth_limit: MAX_WIRE_GROUP_DEPTH,
        }
    }

    /// Creates a cursor using the lower of the configured and hard ceilings.
    fn with_group_depth(bytes: &'a [u8], limit: usize) -> Self {
        Self {
            bytes,
            cursor: 0,
            group_depth_limit: limit.min(MAX_WIRE_GROUP_DEPTH),
        }
    }

    /// Returns the next field after validating its complete framing.
    ///
    /// # Errors
    ///
    /// Returns a malformed refusal for invalid tags, wire types, lengths,
    /// varints, fixed values, or groups.
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
                let bytes = self
                    .bytes
                    .get(self.cursor..end)
                    .ok_or_else(|| malformed("truncated protobuf field"))?;
                self.cursor = end;
                WireValue::Bytes(bytes)
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

    /// Reads one canonical protobuf varint.
    ///
    /// # Errors
    ///
    /// Returns a malformed refusal for truncation or values wider than ten bytes.
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

    /// Reads one little-endian fixed64.
    ///
    /// # Errors
    ///
    /// Returns a malformed refusal when eight bytes are unavailable.
    fn read_fixed_u64(&mut self) -> Result<u64, IngestError> {
        let end = self
            .cursor
            .checked_add(8)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| malformed("truncated protobuf fixed64"))?;
        let bytes: [u8; 8] = self.bytes[self.cursor..end]
            .try_into()
            .map_err(|_| malformed("invalid protobuf fixed64"))?;
        self.cursor = end;
        Ok(u64::from_le_bytes(bytes))
    }

    /// Reads one little-endian fixed32.
    ///
    /// # Errors
    ///
    /// Returns a malformed refusal when four bytes are unavailable.
    fn read_fixed_u32(&mut self) -> Result<u32, IngestError> {
        let end = self
            .cursor
            .checked_add(4)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| malformed("truncated protobuf fixed32"))?;
        let bytes: [u8; 4] = self.bytes[self.cursor..end]
            .try_into()
            .map_err(|_| malformed("invalid protobuf fixed32"))?;
        self.cursor = end;
        Ok(u32::from_le_bytes(bytes))
    }

    /// Skips a complete unknown group using fixed stack storage.
    ///
    /// # Errors
    ///
    /// Returns a malformed refusal for truncation, mismatched tags, invalid
    /// framing, or nesting beyond the configured ceiling.
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
            match key & 7 {
                4 => {
                    let index = depth
                        .checked_sub(1)
                        .ok_or_else(|| malformed("protobuf group depth underflow"))?;
                    if tags.get(index) != Some(&tag) {
                        return Err(malformed("mismatched protobuf end-group tag"));
                    }
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                3 => {
                    if depth == self.group_depth_limit {
                        return Err(malformed("protobuf group nesting depth exceeded"));
                    }
                    let slot = tags
                        .get_mut(depth)
                        .ok_or_else(|| malformed("protobuf group nesting depth exceeded"))?;
                    *slot = tag;
                    depth += 1;
                }
                wire_type => self.skip_value(wire_type)?,
            }
        }
    }

    /// Skips one non-group protobuf value.
    ///
    /// # Errors
    ///
    /// Returns a malformed refusal for invalid wire types or truncated values.
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

/// Preflights one OTLP logs protobuf body without constructing generated types.
///
/// # Errors
///
/// Returns a stable refusal for oversized input, malformed framing, wrong known
/// wire types, excess value nesting, or any configured count/byte limit.
pub(crate) fn preflight_logs_protobuf(
    bytes: &[u8],
    limits: OtlpWireLimits,
) -> Result<OtlpLogsDecodePlan, IngestError> {
    if bytes.len() > limits.request_bytes {
        return Err(IngestError::PayloadTooLarge {
            bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            limit: u64::try_from(limits.request_bytes).unwrap_or(u64::MAX),
        });
    }
    let mut facts = LogsWireFacts::new(limits);
    let mut fields = WireFields::with_group_depth(bytes, limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(resource)) => {
                LogsWireFacts::add_count(&mut facts.resources, 1, limits.resources)?;
                facts.add_decode_bytes(size_of::<ResourceLogs>())?;
                visit_resource_logs(resource, &mut facts)?;
            }
            (1, _) => return Err(wrong_wire("logs resource group")),
            _ => {}
        }
    }
    Ok(OtlpLogsDecodePlan {
        wire_bytes: bytes.len(),
        decode_bytes: facts.decode_bytes,
    })
}

/// Visits one resource group and its merged resource plus repeated scopes.
///
/// # Errors
///
/// Returns a stable refusal for malformed known fields or configured limits.
fn visit_resource_logs(bytes: &[u8], facts: &mut LogsWireFacts) -> Result<(), IngestError> {
    if let Some(schema) = last_bytes_field(bytes, 3)? {
        facts.add_value_bytes(schema.len())?;
    }
    visit_merged_resource(bytes, 1, facts)?;
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(_)) | (3, WireValue::Bytes(_)) => {}
            (2, WireValue::Bytes(scope)) => {
                LogsWireFacts::add_count(&mut facts.scopes, 1, facts.limits.scopes)?;
                facts.add_decode_bytes(size_of::<ScopeLogs>())?;
                visit_scope_logs(scope, facts)?;
            }
            (1..=3, _) => return Err(wrong_wire("resource logs")),
            _ => {}
        }
    }
    Ok(())
}

/// Visits all occurrences merged into one optional resource.
///
/// # Errors
///
/// Returns a stable refusal for malformed resource fields or configured limits.
fn visit_merged_resource(
    bytes: &[u8],
    wanted_tag: u32,
    facts: &mut LogsWireFacts,
) -> Result<(), IngestError> {
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        if tag != wanted_tag {
            continue;
        }
        let WireValue::Bytes(resource) = value else {
            return Err(wrong_wire("optional resource"));
        };
        visit_resource(resource, facts)?;
    }
    Ok(())
}

/// Visits one resource message whose repeated fields append under protobuf merge.
///
/// # Errors
///
/// Returns a stable refusal for malformed fields or configured limits.
fn visit_resource(bytes: &[u8], facts: &mut LogsWireFacts) -> Result<(), IngestError> {
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
            (2, WireValue::Varint(_)) => {}
            (3, WireValue::Bytes(entity)) => {
                facts.add_decode_bytes(size_of::<EntityRef>())?;
                visit_entity_ref(entity, facts)?;
            }
            (1..=3, _) => return Err(wrong_wire("resource")),
            _ => {}
        }
    }
    Ok(())
}

/// Visits one entity reference, retaining final singular strings and all list strings.
///
/// # Errors
///
/// Returns a stable refusal for malformed fields or configured limits.
fn visit_entity_ref(bytes: &[u8], facts: &mut LogsWireFacts) -> Result<(), IngestError> {
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

/// Visits one scope group and its merged scope plus repeated records.
///
/// # Errors
///
/// Returns a stable refusal for malformed fields or configured limits.
fn visit_scope_logs(bytes: &[u8], facts: &mut LogsWireFacts) -> Result<(), IngestError> {
    for tag in [1, 2] {
        if let Some(value) = last_bytes_across_messages(bytes, 1, tag)? {
            facts.add_value_bytes(value.len())?;
        }
    }
    if let Some(schema) = last_bytes_field(bytes, 3)? {
        facts.add_value_bytes(schema.len())?;
    }
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(scope)) => visit_scope(scope, facts)?,
            (2, WireValue::Bytes(record)) => {
                LogsWireFacts::add_count(&mut facts.records, 1, facts.limits.records)?;
                facts.add_decode_bytes(size_of::<LogRecord>())?;
                visit_log_record(record, facts)?;
            }
            (3, WireValue::Bytes(_)) => {}
            (1..=3, _) => return Err(wrong_wire("scope logs")),
            _ => {}
        }
    }
    Ok(())
}

/// Visits one scope occurrence; repeated attributes append across scope merges.
///
/// # Errors
///
/// Returns a stable refusal for malformed fields or configured limits.
fn visit_scope(bytes: &[u8], facts: &mut LogsWireFacts) -> Result<(), IngestError> {
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

/// Visits every retained variable field and attribute in one log record.
///
/// # Errors
///
/// Returns a stable refusal for malformed fields, excessive depth, or limits.
fn visit_log_record(bytes: &[u8], facts: &mut LogsWireFacts) -> Result<(), IngestError> {
    for tag in [3, 9, 10, 12] {
        if let Some(value) = last_bytes_field(bytes, tag)? {
            facts.add_value_bytes(value.len())?;
        }
    }
    visit_merged_any_value(
        AnyValueBodies::Repeated {
            parent: bytes,
            tag: 5,
        },
        1,
        facts,
    )?;
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1 | 11, WireValue::Fixed64(_)) => {}
            (2 | 7, WireValue::Varint(_)) => {}
            (3 | 5 | 9 | 10 | 12, WireValue::Bytes(_)) => {}
            (6, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
            (8, WireValue::Fixed32(_)) => {}
            (1..=12, _) => return Err(wrong_wire("log record")),
            _ => {}
        }
    }
    Ok(())
}

/// Visits one key/value and its final merged value.
///
/// # Errors
///
/// Returns a stable refusal for malformed fields, excessive depth, or limits.
fn visit_attribute(
    bytes: &[u8],
    value_depth: usize,
    facts: &mut LogsWireFacts,
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

/// Visits one recursive value message.
///
/// # Errors
///
/// Returns a stable refusal for malformed fields, excessive depth, or limits.
fn visit_any_value(
    bytes: &[u8],
    depth: usize,
    facts: &mut LogsWireFacts,
) -> Result<(), IngestError> {
    visit_merged_any_value(AnyValueBodies::Single(bytes), depth, facts)
}

/// Charges only the final retained oneof run across merged value bodies.
///
/// # Errors
///
/// Returns a stable refusal for malformed fields, excessive depth, or limits.
fn visit_merged_any_value(
    bodies: AnyValueBodies<'_>,
    depth: usize,
    facts: &mut LogsWireFacts,
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
                        match (value_tag, value) {
                            (1, WireValue::Bytes(value)) => {
                                facts.add_decode_bytes(size_of::<AnyValue>())?;
                                visit_any_value(value, depth + 1, facts)?;
                            }
                            (1, _) => return Err(wrong_wire("OTLP value array")),
                            _ => {}
                        }
                    }
                }
                (6, WireValue::Bytes(list)) => {
                    let mut values = WireFields::with_group_depth(list, facts.limits.value_depth);
                    while let Some((value_tag, value)) = values.next()? {
                        match (value_tag, value) {
                            (1, WireValue::Bytes(value)) => visit_attribute(value, depth, facts)?,
                            (1, _) => return Err(wrong_wire("OTLP key/value list")),
                            _ => {}
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

/// Constructs one logs export using only exact capacities established from the wire.
///
/// # Errors
///
/// Returns a malformed refusal for invalid framing, UTF-8, known wire types,
/// or repeated-field count overflow.
pub(crate) fn decode_logs_protobuf(bytes: &[u8]) -> Result<ExportLogsServiceRequest, IngestError> {
    let count = repeated_message_count(bytes, 1)?;
    let mut resource_logs = Vec::with_capacity(count);
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(resource)) => resource_logs.push(decode_resource_logs(resource)?),
            (1, _) => return Err(wrong_wire("logs resource group")),
            _ => {}
        }
    }
    Ok(ExportLogsServiceRequest { resource_logs })
}

/// Constructs one resource group with exact repeated capacity.
///
/// # Errors
///
/// Returns a malformed refusal for invalid framing, UTF-8, or wire types.
fn decode_resource_logs(bytes: &[u8]) -> Result<ResourceLogs, IngestError> {
    let resource = decode_merged_resource(bytes, 1)?;
    let count = repeated_message_count(bytes, 2)?;
    let mut scope_logs = Vec::with_capacity(count);
    let schema_url = last_bytes_field(bytes, 3)?
        .map(fixed_string)
        .transpose()?
        .unwrap_or_default();
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(_)) | (3, WireValue::Bytes(_)) => {}
            (2, WireValue::Bytes(scope)) => scope_logs.push(decode_scope_logs(scope)?),
            (1..=3, _) => return Err(wrong_wire("resource logs")),
            _ => {}
        }
    }
    Ok(ResourceLogs {
        resource,
        scope_logs,
        schema_url,
    })
}

/// Constructs and merges every occurrence of an optional resource field.
///
/// # Errors
///
/// Returns a malformed refusal for invalid resource framing or fields.
fn decode_merged_resource(bytes: &[u8], wanted_tag: u32) -> Result<Option<Resource>, IngestError> {
    let (occurrences, attributes) = repeated_count_across_messages(bytes, wanted_tag, 1)?;
    let (_, entities) = repeated_count_across_messages(bytes, wanted_tag, 3)?;
    if occurrences == 0 {
        return Ok(None);
    }
    let mut resource = Resource {
        attributes: Vec::with_capacity(attributes),
        entity_refs: Vec::with_capacity(entities),
        ..Resource::default()
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if tag == wanted_tag {
            let WireValue::Bytes(body) = value else {
                return Err(wrong_wire("optional resource"));
            };
            merge_resource(body, &mut resource)?;
        }
    }
    Ok(Some(resource))
}

/// Merges one resource occurrence into its fixed-capacity destination.
///
/// # Errors
///
/// Returns a malformed refusal for invalid framing or known wire types.
fn merge_resource(bytes: &[u8], resource: &mut Resource) -> Result<(), IngestError> {
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(attribute)) => {
                resource.attributes.push(decode_key_value(attribute)?)
            }
            (2, WireValue::Varint(value)) => {
                resource.dropped_attributes_count = protobuf_u32(value)
            }
            (3, WireValue::Bytes(entity)) => resource.entity_refs.push(decode_entity_ref(entity)?),
            (1..=3, _) => return Err(wrong_wire("resource")),
            _ => {}
        }
    }
    Ok(())
}

/// Constructs one entity reference with exact string-list capacity.
///
/// # Errors
///
/// Returns a malformed refusal for invalid framing, UTF-8, or known wire types.
fn decode_entity_ref(bytes: &[u8]) -> Result<EntityRef, IngestError> {
    let mut entity = EntityRef {
        schema_url: last_bytes_field(bytes, 1)?
            .map(fixed_string)
            .transpose()?
            .unwrap_or_default(),
        r#type: last_bytes_field(bytes, 2)?
            .map(fixed_string)
            .transpose()?
            .unwrap_or_default(),
        id_keys: Vec::with_capacity(repeated_message_count(bytes, 3)?),
        description_keys: Vec::with_capacity(repeated_message_count(bytes, 4)?),
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1 | 2, WireValue::Bytes(_)) => {}
            (3, WireValue::Bytes(value)) => entity.id_keys.push(fixed_string(value)?),
            (4, WireValue::Bytes(value)) => entity.description_keys.push(fixed_string(value)?),
            (1..=4, _) => return Err(wrong_wire("entity reference")),
            _ => {}
        }
    }
    Ok(entity)
}

/// Constructs one scope group with exact repeated capacity.
///
/// # Errors
///
/// Returns a malformed refusal for invalid framing, UTF-8, or wire types.
fn decode_scope_logs(bytes: &[u8]) -> Result<ScopeLogs, IngestError> {
    let scope = decode_merged_scope(bytes, 1)?;
    let count = repeated_message_count(bytes, 2)?;
    let mut log_records = Vec::with_capacity(count);
    let schema_url = last_bytes_field(bytes, 3)?
        .map(fixed_string)
        .transpose()?
        .unwrap_or_default();
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(_)) | (3, WireValue::Bytes(_)) => {}
            (2, WireValue::Bytes(record)) => log_records.push(decode_log_record(record)?),
            (1..=3, _) => return Err(wrong_wire("scope logs")),
            _ => {}
        }
    }
    Ok(ScopeLogs {
        scope,
        log_records,
        schema_url,
    })
}

/// Constructs and merges every occurrence of an optional scope field.
///
/// # Errors
///
/// Returns a malformed refusal for invalid scope framing or fields.
fn decode_merged_scope(
    bytes: &[u8],
    wanted_tag: u32,
) -> Result<Option<InstrumentationScope>, IngestError> {
    let (occurrences, attributes) = repeated_count_across_messages(bytes, wanted_tag, 3)?;
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
        attributes: Vec::with_capacity(attributes),
        ..InstrumentationScope::default()
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if tag == wanted_tag {
            let WireValue::Bytes(body) = value else {
                return Err(wrong_wire("optional instrumentation scope"));
            };
            merge_scope(body, &mut scope)?;
        }
    }
    Ok(Some(scope))
}

/// Merges one scope occurrence after final scalar strings were constructed once.
///
/// # Errors
///
/// Returns a malformed refusal for invalid framing, UTF-8, or known wire types.
fn merge_scope(bytes: &[u8], scope: &mut InstrumentationScope) -> Result<(), IngestError> {
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1 | 2, WireValue::Bytes(_)) => {}
            (3, WireValue::Bytes(attribute)) => scope.attributes.push(decode_key_value(attribute)?),
            (4, WireValue::Varint(value)) => scope.dropped_attributes_count = protobuf_u32(value),
            (1..=4, _) => return Err(wrong_wire("instrumentation scope")),
            _ => {}
        }
    }
    Ok(())
}

/// Constructs one complete log record with exact retained backing capacity.
///
/// # Errors
///
/// Returns a malformed refusal for invalid framing, UTF-8, or known wire types.
fn decode_log_record(bytes: &[u8]) -> Result<LogRecord, IngestError> {
    let body = decode_merged_any_value(bytes, 5)?;
    let mut record = LogRecord {
        severity_text: last_bytes_field(bytes, 3)?
            .map(fixed_string)
            .transpose()?
            .unwrap_or_default(),
        body,
        attributes: Vec::with_capacity(repeated_message_count(bytes, 6)?),
        trace_id: last_bytes_field(bytes, 9)?
            .map(fixed_bytes)
            .unwrap_or_default(),
        span_id: last_bytes_field(bytes, 10)?
            .map(fixed_bytes)
            .unwrap_or_default(),
        event_name: last_bytes_field(bytes, 12)?
            .map(fixed_string)
            .transpose()?
            .unwrap_or_default(),
        ..LogRecord::default()
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Fixed64(value)) => record.time_unix_nano = value,
            (2, WireValue::Varint(value)) => record.severity_number = protobuf_i32(value),
            (3 | 5 | 9 | 10 | 12, WireValue::Bytes(_)) => {}
            (6, WireValue::Bytes(attribute)) => {
                record.attributes.push(decode_key_value(attribute)?)
            }
            (7, WireValue::Varint(value)) => record.dropped_attributes_count = protobuf_u32(value),
            (8, WireValue::Fixed32(value)) => record.flags = value,
            (11, WireValue::Fixed64(value)) => record.observed_time_unix_nano = value,
            (1..=12, _) => return Err(wrong_wire("log record")),
            _ => {}
        }
    }
    Ok(record)
}

/// Counts repeated length-delimited fields without retaining descriptors.
///
/// # Errors
///
/// Returns a malformed refusal for invalid framing, wire type, or count overflow.
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

/// Returns the final retained bytes occurrence for a singular field.
///
/// # Errors
///
/// Returns a malformed refusal for invalid framing or wire type.
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
/// Returns a malformed refusal for invalid parent or child framing.
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

/// Counts repeated children across every occurrence of a singular parent.
///
/// # Errors
///
/// Returns a malformed refusal for invalid framing, wire type, or count overflow.
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
            let WireValue::Bytes(body) = value else {
                return Err(wrong_wire("optional OTLP message"));
            };
            occurrences = occurrences
                .checked_add(1)
                .ok_or_else(|| malformed("OTLP message occurrence overflow"))?;
            children = children
                .checked_add(repeated_message_count(body, child_tag)?)
                .ok_or_else(|| malformed("OTLP repeated-field count overflow"))?;
        }
    }
    Ok((occurrences, children))
}

/// Copies valid UTF-8 into exactly-sized string storage.
///
/// # Errors
///
/// Returns a malformed refusal when the bytes are not UTF-8.
fn fixed_string(bytes: &[u8]) -> Result<String, IngestError> {
    let text = std::str::from_utf8(bytes).map_err(|_| malformed("invalid protobuf string"))?;
    let mut value = String::with_capacity(bytes.len());
    value.push_str(text);
    Ok(value)
}

/// Copies bytes into exactly-sized vector storage.
fn fixed_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut value = Vec::with_capacity(bytes.len());
    value.extend_from_slice(bytes);
    value
}

/// Applies protobuf's low-32-bit unsigned varint conversion.
fn protobuf_u32(value: u64) -> u32 {
    let bytes = value.to_le_bytes();
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Applies protobuf's low-32-bit signed varint conversion.
fn protobuf_i32(value: u64) -> i32 {
    let bytes = value.to_le_bytes();
    i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Reinterprets a signed protobuf varint's two's-complement bits.
fn protobuf_i64(value: u64) -> i64 {
    i64::from_le_bytes(value.to_le_bytes())
}

/// Constructs one key/value and its singular merged value.
///
/// # Errors
///
/// Returns a malformed refusal for invalid framing, UTF-8, or wire types.
fn decode_key_value(bytes: &[u8]) -> Result<KeyValue, IngestError> {
    let value = decode_merged_any_value(bytes, 2)?;
    let key = last_bytes_field(bytes, 1)?
        .map(fixed_string)
        .transpose()?
        .unwrap_or_default();
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1 | 2, WireValue::Bytes(_)) => {}
            (1 | 2, _) => return Err(wrong_wire("key/value")),
            _ => {}
        }
    }
    Ok(KeyValue { key, value })
}

/// Borrowed encoded bodies merged into one singular value message.
#[derive(Clone, Copy)]
enum AnyValueBodies<'a> {
    /// One ordinary value body.
    Single(&'a [u8]),
    /// Repeated singular-message occurrences in a parent.
    Repeated {
        /// Parent bytes.
        parent: &'a [u8],
        /// Singular message field tag.
        tag: u32,
    },
}

impl AnyValueBodies<'_> {
    /// Visits bodies in protobuf merge order without collecting descriptors.
    ///
    /// # Errors
    ///
    /// Returns a malformed refusal for parent framing or visitor failure.
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

/// Final oneof run retained after protobuf merge semantics.
#[derive(Clone, Copy, Default)]
struct AnyValueSelection {
    /// Selected oneof field tag, or zero for empty.
    tag: u32,
    /// Ordinal beginning the final same-message-variant run.
    run_start: usize,
    /// Nested elements appended by the retained message run.
    nested_count: usize,
}

/// Constructs one recursive value with exact nested capacity.
///
/// # Errors
///
/// Returns a malformed refusal for invalid framing, UTF-8, or wire types.
fn decode_any_value(bytes: &[u8]) -> Result<AnyValue, IngestError> {
    decode_any_value_bodies(AnyValueBodies::Single(bytes))
}

/// Merges every occurrence of one singular value field.
///
/// # Errors
///
/// Returns a malformed refusal for invalid framing, UTF-8, or wire types.
fn decode_merged_any_value(bytes: &[u8], wanted_tag: u32) -> Result<Option<AnyValue>, IngestError> {
    let bodies = AnyValueBodies::Repeated {
        parent: bytes,
        tag: wanted_tag,
    };
    if bodies.try_for_each(|_| Ok(()))? == 0 {
        return Ok(None);
    }
    decode_any_value_bodies(bodies).map(Some)
}

/// Selects the final retained oneof run and exact nested element count.
///
/// # Errors
///
/// Returns a malformed refusal for invalid framing, wire types, or overflow.
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

/// Decodes a protobuf-ordered sequence of merged value bodies.
///
/// # Errors
///
/// Returns a malformed refusal for invalid framing, UTF-8, wire types, or overflow.
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
            ordinal = ordinal
                .checked_add(1)
                .ok_or_else(|| malformed("OTLP oneof occurrence overflow"))?;
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

/// Validates one generated oneof field's wire representation.
///
/// # Errors
///
/// Returns a malformed refusal when the field has the wrong wire type.
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

/// Appends one encoded array occurrence to an exact-capacity destination.
///
/// # Errors
///
/// Returns a malformed refusal for invalid framing or wire types.
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

/// Appends one encoded key/value-list occurrence to an exact destination.
///
/// # Errors
///
/// Returns a malformed refusal for invalid framing or wire types.
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

/// Constructs a stable malformed refusal for a known wire mismatch.
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
    use wyrd_tonic::prost::Message;

    /// Builds one test attribute without hiding production allocation behavior.
    fn attribute(key: &str, value: any_value::Value) -> KeyValue {
        KeyValue {
            key: key.to_owned(),
            value: Some(AnyValue { value: Some(value) }),
        }
    }

    /// Computes exact live capacity retained below a generated logs request.
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

    /// Computes retained backing beneath one resource.
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

    /// Computes vector layouts, keys, and recursive values for attributes.
    fn decoded_attributes_capacity(attributes: &[KeyValue]) -> usize {
        attributes.len() * size_of::<KeyValue>()
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

    /// Computes recursive backing storage below one inlined value.
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

    /// Asserts every recursive repeated collection has no spare capacity.
    fn assert_fixed_any_value(value: &AnyValue) {
        match value.value.as_ref() {
            Some(any_value::Value::ArrayValue(array)) => {
                assert_eq!(array.values.len(), array.values.capacity());
                for value in &array.values {
                    assert_fixed_any_value(value);
                }
            }
            Some(any_value::Value::KvlistValue(list)) => {
                assert_eq!(list.values.len(), list.values.capacity());
                for attribute in &list.values {
                    if let Some(value) = &attribute.value {
                        assert_fixed_any_value(value);
                    }
                }
            }
            _ => {}
        }
    }

    /// Asserts all logs collections and variable fields retain exact capacity.
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

    /// Builds alternating recursive values for exact depth-boundary tests.
    fn alternating_value(depth: usize) -> AnyValue {
        if depth == 1 {
            return AnyValue {
                value: Some(any_value::Value::StringValue("leaf".to_owned())),
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

    /// Proves rich logs decode matches Prost and exact owner accounting.
    ///
    /// # Panics
    ///
    /// Panics if the valid fixture cannot encode or decode.
    #[test]
    fn logs_fixed_decode_matches_prost_and_exact_capacity() {
        let request = ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                resource: Some(Resource {
                    attributes: vec![attribute(
                        "resource",
                        any_value::Value::StringValue("service".to_owned()),
                    )],
                    dropped_attributes_count: 1,
                    entity_refs: vec![EntityRef {
                        schema_url: "entity/v1".to_owned(),
                        r#type: "service".to_owned(),
                        id_keys: vec!["service.name".to_owned()],
                        description_keys: vec!["service.version".to_owned()],
                    }],
                }),
                scope_logs: vec![ScopeLogs {
                    scope: Some(InstrumentationScope {
                        name: "logger".to_owned(),
                        version: "1.0".to_owned(),
                        attributes: vec![attribute("scope", any_value::Value::BoolValue(true))],
                        dropped_attributes_count: 2,
                    }),
                    log_records: vec![LogRecord {
                        time_unix_nano: 10,
                        observed_time_unix_nano: 11,
                        severity_number: 17,
                        severity_text: "ERROR".to_owned(),
                        body: Some(AnyValue {
                            value: Some(any_value::Value::KvlistValue(KeyValueList {
                                values: vec![attribute(
                                    "items",
                                    any_value::Value::ArrayValue(ArrayValue {
                                        values: vec![AnyValue {
                                            value: Some(any_value::Value::BytesValue(vec![
                                                1, 2, 3,
                                            ])),
                                        }],
                                    }),
                                )],
                            })),
                        }),
                        attributes: vec![attribute("attempt", any_value::Value::IntValue(3))],
                        dropped_attributes_count: 4,
                        flags: 0x101,
                        trace_id: vec![5; 16],
                        span_id: vec![6; 8],
                        event_name: "failure".to_owned(),
                    }],
                    schema_url: "scope/v1".to_owned(),
                }],
                schema_url: "resource/v1".to_owned(),
            }],
        };
        let mut bytes = request.encode_to_vec();
        bytes.extend_from_slice(&[0xa0, 0x06, 0x01]);
        let expected = ExportLogsServiceRequest::decode(bytes.as_slice()).expect("prost fixture");
        let plan =
            preflight_logs_protobuf(&bytes, vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS)
                .expect("logs preflight");
        let decoded = decode_logs_protobuf(&bytes).expect("fixed logs decode");
        assert_eq!(decoded, expected);
        assert_eq!(decoded, request);
        assert_eq!(plan.decode_bytes, decoded_logs_capacity(&decoded));
        assert_fixed_logs_capacity(&decoded);
    }

    /// Proves duplicate singular messages and scalar fields follow Prost merge semantics.
    ///
    /// # Panics
    ///
    /// Panics if the handcrafted valid fixture cannot decode.
    #[test]
    fn logs_fixed_decode_merges_duplicate_singular_fields() {
        let mut record = Vec::new();
        push_raw_message_field(&mut record, 3, b"discarded");
        push_raw_message_field(&mut record, 3, b"retained");
        let first_body = AnyValue {
            value: Some(any_value::Value::ArrayValue(ArrayValue {
                values: vec![AnyValue {
                    value: Some(any_value::Value::IntValue(1)),
                }],
            })),
        };
        let second_body = AnyValue {
            value: Some(any_value::Value::ArrayValue(ArrayValue {
                values: vec![AnyValue {
                    value: Some(any_value::Value::IntValue(2)),
                }],
            })),
        };
        push_message_field(&mut record, 5, &first_body);
        push_message_field(&mut record, 5, &second_body);
        push_raw_message_field(&mut record, 9, &[1; 16]);
        push_raw_message_field(&mut record, 9, &[2; 16]);

        let mut scope = Vec::new();
        push_message_field(
            &mut scope,
            1,
            &InstrumentationScope {
                name: "first".to_owned(),
                attributes: vec![attribute("one", any_value::Value::IntValue(1))],
                ..InstrumentationScope::default()
            },
        );
        push_message_field(
            &mut scope,
            1,
            &InstrumentationScope {
                name: "retained".to_owned(),
                attributes: vec![attribute("two", any_value::Value::IntValue(2))],
                ..InstrumentationScope::default()
            },
        );
        push_raw_message_field(&mut scope, 2, &record);
        let mut resource = Vec::new();
        push_raw_message_field(&mut resource, 2, &scope);
        let mut request = Vec::new();
        push_raw_message_field(&mut request, 1, &resource);

        let expected = ExportLogsServiceRequest::decode(request.as_slice()).expect("prost fixture");
        let plan =
            preflight_logs_protobuf(&request, vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS)
                .expect("logs preflight");
        let decoded = decode_logs_protobuf(&request).expect("fixed logs decode");
        assert_eq!(decoded, expected);
        assert_eq!(plan.decode_bytes, decoded_logs_capacity(&decoded));
        assert_fixed_logs_capacity(&decoded);
    }

    /// Proves record cardinality cap and recursive value depth fail at cap plus one.
    ///
    /// # Panics
    ///
    /// Panics if test fixtures cannot encode.
    #[test]
    fn logs_preflight_enforces_record_and_depth_limits() {
        let mut limits = vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS;
        limits.records = 1;
        let request = ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                scope_logs: vec![ScopeLogs {
                    log_records: vec![LogRecord::default(), LogRecord::default()],
                    ..ScopeLogs::default()
                }],
                ..ResourceLogs::default()
            }],
        };
        assert!(preflight_logs_protobuf(&request.encode_to_vec(), limits).is_err());

        for (depth, accepted) in [(8, true), (9, false)] {
            let request = ExportLogsServiceRequest {
                resource_logs: vec![ResourceLogs {
                    scope_logs: vec![ScopeLogs {
                        log_records: vec![LogRecord {
                            body: Some(alternating_value(depth)),
                            ..LogRecord::default()
                        }],
                        ..ScopeLogs::default()
                    }],
                    ..ResourceLogs::default()
                }],
            };
            let result = preflight_logs_protobuf(
                &request.encode_to_vec(),
                vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS,
            );
            assert_eq!(result.is_ok(), accepted, "depth {depth}");
        }
    }

    /// Proves unknown groups use the fixed depth stack and reject cap plus one.
    #[test]
    fn logs_preflight_bounds_unknown_groups() {
        let mut bytes = Vec::new();
        for tag in 100_u32..109 {
            push_varint(&mut bytes, u64::from(tag) << 3 | 3);
        }
        for tag in (100_u32..109).rev() {
            push_varint(&mut bytes, u64::from(tag) << 3 | 4);
        }
        assert!(
            preflight_logs_protobuf(&bytes, vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS,)
                .is_err()
        );
    }

    /// Appends one canonical protobuf varint.
    fn push_varint(bytes: &mut Vec<u8>, mut value: u64) {
        while value >= 0x80 {
            bytes.push(value.to_le_bytes()[0] | 0x80);
            value >>= 7;
        }
        bytes.push(value.to_le_bytes()[0]);
    }

    /// Appends a generated message as one length-delimited field.
    ///
    /// # Panics
    ///
    /// Panics if an encoded length cannot fit `u64`.
    fn push_message_field(bytes: &mut Vec<u8>, tag: u32, message: &impl Message) {
        push_raw_message_field(bytes, tag, &message.encode_to_vec());
    }

    /// Appends an encoded body as one length-delimited field.
    ///
    /// # Panics
    ///
    /// Panics if an encoded length cannot fit `u64`.
    fn push_raw_message_field(bytes: &mut Vec<u8>, tag: u32, message: &[u8]) {
        push_varint(bytes, u64::from(tag) << 3 | 2);
        push_varint(
            bytes,
            u64::try_from(message.len()).expect("invariant: encoded length fits u64"),
        );
        bytes.extend_from_slice(message);
    }
}
