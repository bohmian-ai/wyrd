//! Fixed-capacity OTLP metrics protobuf preflight and construction.

use std::mem::size_of;

use vala_bifrost_redux::gate::{IngestError, OtlpWireLimits};
use wyrd_tonic::otlp::common::v1::{
    AnyValue, ArrayValue, EntityRef, InstrumentationScope, KeyValue, KeyValueList, any_value,
};
use wyrd_tonic::otlp::metrics::v1::{
    Exemplar, ExponentialHistogram, ExponentialHistogramDataPoint, Gauge, Histogram,
    HistogramDataPoint, Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics, Sum, Summary,
    SummaryDataPoint, exemplar, exponential_histogram_data_point, metric, number_data_point,
    summary_data_point,
};
use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;
use wyrd_tonic::otlp::resource::v1::Resource;

/// Maximum deprecated protobuf group nesting accepted by the bounded scanner.
const MAX_WIRE_GROUP_DEPTH: usize = 8;

/// Exact allocation facts established before constructing a metrics request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MetricsDecodePlan {
    /// Encoded bytes inspected by preflight and required by the decoder.
    pub(crate) wire_bytes: usize,
    /// Exact live generated-message capacity admitted for construction.
    pub(crate) decode_bytes: usize,
}

/// Bounded allocation and cardinality facts for one metrics export.
#[derive(Debug)]
struct MetricsWireFacts {
    /// Immutable limits shared with the transport and Scribe admission path.
    limits: OtlpWireLimits,
    /// Resource message count.
    resources: usize,
    /// Scope message count.
    scopes: usize,
    /// Metric data-point count across every aggregation shape.
    records: usize,
    /// Attribute count including metadata and exemplar filtered attributes.
    attributes: usize,
    /// Retained string, byte, and recursive value backing bytes.
    value_bytes: usize,
    /// Exact live public layouts and collection backing bytes.
    decode_bytes: usize,
}

impl MetricsWireFacts {
    /// Starts a preflight with the root request layout already charged.
    fn new(limits: OtlpWireLimits) -> Self {
        Self {
            limits,
            resources: 0,
            scopes: 0,
            records: 0,
            attributes: 0,
            value_bytes: 0,
            decode_bytes: size_of::<ExportMetricsServiceRequest>(),
        }
    }

    /// Adds a checked cardinality or byte fact and enforces its ceiling.
    ///
    /// # Errors
    ///
    /// Returns a malformed-request error on arithmetic overflow or limit excess.
    fn add_count(current: &mut usize, amount: usize, limit: usize) -> Result<(), IngestError> {
        *current = current
            .checked_add(amount)
            .ok_or_else(|| malformed("OTLP metrics cardinality overflow"))?;
        if *current > limit {
            return Err(malformed("OTLP metrics cardinality limit exceeded"));
        }
        Ok(())
    }

    /// Charges retained variable-width bytes and generated backing storage.
    ///
    /// # Errors
    ///
    /// Returns a malformed-request error on overflow or configured limit excess.
    fn add_value_bytes(&mut self, amount: usize) -> Result<(), IngestError> {
        Self::add_count(&mut self.value_bytes, amount, self.limits.value_bytes)?;
        self.add_decode_bytes(amount)
    }

    /// Charges generated public layouts or fixed-capacity collection storage.
    ///
    /// # Errors
    ///
    /// Returns a malformed-request error on overflow or material limit excess.
    fn add_decode_bytes(&mut self, amount: usize) -> Result<(), IngestError> {
        Self::add_count(&mut self.decode_bytes, amount, usize::MAX)
    }

    /// Charges one retained attribute and its `KeyValue` layout.
    ///
    /// # Errors
    ///
    /// Returns a malformed-request error when attribute or material limits fail.
    fn add_attribute(&mut self) -> Result<(), IngestError> {
        Self::add_count(&mut self.attributes, 1, self.limits.attributes)?;
        self.add_decode_bytes(size_of::<KeyValue>())
    }

    /// Charges one metric data point against the signal-record ceiling.
    ///
    /// # Errors
    ///
    /// Returns a malformed-request error when record or material limits fail.
    fn add_record<T>(&mut self) -> Result<(), IngestError> {
        Self::add_count(&mut self.records, 1, self.limits.records)?;
        self.add_decode_bytes(size_of::<T>())
    }

    /// Charges exact backing for a repeated primitive field.
    ///
    /// # Errors
    ///
    /// Returns a malformed-request error on multiplication or material overflow.
    fn add_primitive<T>(&mut self, count: usize) -> Result<(), IngestError> {
        let bytes = count
            .checked_mul(size_of::<T>())
            .ok_or_else(|| malformed("OTLP metrics primitive capacity overflow"))?;
        self.add_decode_bytes(bytes)
    }
}

/// One parsed protobuf field borrowing its original transport bytes.
#[derive(Clone, Copy, Debug)]
enum WireValue<'a> {
    /// Varint field.
    Varint(u64),
    /// Little-endian fixed64 field.
    Fixed64(u64),
    /// Length-delimited field.
    Bytes(&'a [u8]),
    /// Little-endian fixed32 field.
    Fixed32,
    /// Validated deprecated group field.
    Group,
}

/// Non-materializing cursor over one protobuf message body.
#[derive(Debug)]
struct WireFields<'a> {
    /// Complete borrowed body.
    bytes: &'a [u8],
    /// Offset of the next field key.
    cursor: usize,
    /// Maximum accepted deprecated-group depth.
    group_depth_limit: usize,
}

impl<'a> WireFields<'a> {
    /// Creates a cursor with the protocol maximum group depth.
    fn new(bytes: &'a [u8]) -> Self {
        Self::with_group_depth(bytes, MAX_WIRE_GROUP_DEPTH)
    }

    /// Creates a cursor using the lower configured group-depth ceiling.
    fn with_group_depth(bytes: &'a [u8], limit: usize) -> Self {
        Self {
            bytes,
            cursor: 0,
            group_depth_limit: limit.min(MAX_WIRE_GROUP_DEPTH),
        }
    }

    /// Parses the next field and validates its complete framing.
    ///
    /// # Errors
    ///
    /// Returns a malformed-request error for invalid tags, wire types, lengths,
    /// varints, fixed fields, or deprecated groups.
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
            5 => {
                self.read_fixed_u32()?;
                WireValue::Fixed32
            }
            _ => return Err(malformed("unsupported protobuf wire type")),
        };
        Ok(Some((tag, value)))
    }

    /// Reads one protobuf varint.
    ///
    /// # Errors
    ///
    /// Returns a malformed-request error for truncation or values over ten bytes.
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
    /// Returns a malformed-request error when eight bytes are unavailable.
    fn read_fixed_u64(&mut self) -> Result<u64, IngestError> {
        let end = self
            .cursor
            .checked_add(8)
            .ok_or_else(|| malformed("fixed64 overflow"))?;
        let bytes: [u8; 8] = self
            .bytes
            .get(self.cursor..end)
            .ok_or_else(|| malformed("truncated protobuf fixed64"))?
            .try_into()
            .map_err(|_| malformed("invalid protobuf fixed64"))?;
        self.cursor = end;
        Ok(u64::from_le_bytes(bytes))
    }

    /// Reads one little-endian fixed32 value.
    ///
    /// # Errors
    ///
    /// Returns a malformed-request error when four bytes are unavailable.
    fn read_fixed_u32(&mut self) -> Result<u32, IngestError> {
        let end = self
            .cursor
            .checked_add(4)
            .ok_or_else(|| malformed("fixed32 overflow"))?;
        let bytes: [u8; 4] = self
            .bytes
            .get(self.cursor..end)
            .ok_or_else(|| malformed("truncated protobuf fixed32"))?
            .try_into()
            .map_err(|_| malformed("invalid protobuf fixed32"))?;
        self.cursor = end;
        Ok(u32::from_le_bytes(bytes))
    }

    /// Skips one deprecated group using a fixed stack.
    ///
    /// # Errors
    ///
    /// Returns a malformed-request error for mismatched, unterminated, or too-deep groups.
    fn skip_group(&mut self, group_tag: u32) -> Result<(), IngestError> {
        let mut stack = [0_u32; MAX_WIRE_GROUP_DEPTH];
        if self.group_depth_limit == 0 {
            return Err(malformed("protobuf group depth exceeded"));
        }
        stack[0] = group_tag;
        let mut depth = 1_usize;
        while depth != 0 {
            let key = self.read_varint()?;
            let tag = u32::try_from(key >> 3).map_err(|_| malformed("invalid group tag"))?;
            if tag == 0 {
                return Err(malformed("protobuf group tag zero is invalid"));
            }
            match key & 7 {
                3 => {
                    if depth >= self.group_depth_limit {
                        return Err(malformed("protobuf group depth exceeded"));
                    }
                    stack[depth] = tag;
                    depth += 1;
                }
                4 => {
                    if stack[depth - 1] != tag {
                        return Err(malformed("mismatched protobuf end group"));
                    }
                    depth -= 1;
                }
                wire_type => self.skip_value(wire_type)?,
            }
        }
        Ok(())
    }

    /// Skips one non-group protobuf value.
    ///
    /// # Errors
    ///
    /// Returns a malformed-request error for invalid or truncated values.
    fn skip_value(&mut self, wire_type: u64) -> Result<(), IngestError> {
        match wire_type {
            0 => self.read_varint().map(|_| ()),
            1 => self.read_fixed_u64().map(|_| ()),
            2 => {
                let length = usize::try_from(self.read_varint()?)
                    .map_err(|_| malformed("protobuf length does not fit usize"))?;
                self.cursor = self
                    .cursor
                    .checked_add(length)
                    .filter(|end| *end <= self.bytes.len())
                    .ok_or_else(|| malformed("truncated protobuf field"))?;
                Ok(())
            }
            5 => self.read_fixed_u32().map(|_| ()),
            _ => Err(malformed("unsupported protobuf wire type in group")),
        }
    }
}

/// Preflights a protobuf metrics request without constructing generated messages.
///
/// The returned material fact is the exact live allocation of the request built
/// by [`decode_metrics_protobuf`], including public layouts, collection backing,
/// and retained variable-width values.
///
/// # Errors
///
/// Returns a stable ingest refusal for an oversized request, malformed framing,
/// a known field with the wrong wire type, invalid nesting, arithmetic overflow,
/// or any immutable OTLP cardinality/material limit excess.
pub(crate) fn preflight_metrics_protobuf(
    bytes: &[u8],
    limits: OtlpWireLimits,
) -> Result<MetricsDecodePlan, IngestError> {
    if bytes.len() > limits.request_bytes {
        return Err(IngestError::PayloadTooLarge {
            bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            limit: u64::try_from(limits.request_bytes).unwrap_or(u64::MAX),
        });
    }
    let mut facts = MetricsWireFacts::new(limits);
    let mut fields = WireFields::with_group_depth(bytes, limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(body)) => {
                MetricsWireFacts::add_count(&mut facts.resources, 1, limits.resources)?;
                facts.add_decode_bytes(size_of::<ResourceMetrics>())?;
                visit_resource_metrics(body, &mut facts)?;
            }
            (1, _) => return Err(wrong_wire("metrics resource group")),
            _ => {}
        }
    }
    Ok(MetricsDecodePlan {
        wire_bytes: bytes.len(),
        decode_bytes: facts.decode_bytes,
    })
}

/// Accounts for one resource group and its retained merged resource.
///
/// # Errors
///
/// Returns an ingest refusal for invalid framing, wire types, or bounded facts.
fn visit_resource_metrics(bytes: &[u8], facts: &mut MetricsWireFacts) -> Result<(), IngestError> {
    if let Some(value) = last_bytes_field(bytes, 3)? {
        facts.add_value_bytes(value.len())?;
    }
    visit_merged_resource(bytes, 1, facts)?;
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(_)) | (3, WireValue::Bytes(_)) => {}
            (2, WireValue::Bytes(body)) => {
                MetricsWireFacts::add_count(&mut facts.scopes, 1, facts.limits.scopes)?;
                facts.add_decode_bytes(size_of::<ScopeMetrics>())?;
                visit_scope_metrics(body, facts)?;
            }
            (1..=3, _) => return Err(wrong_wire("resource metrics")),
            _ => {}
        }
    }
    Ok(())
}

/// Accounts for one scope group and its metrics.
///
/// # Errors
///
/// Returns an ingest refusal for invalid framing, wire types, or bounded facts.
fn visit_scope_metrics(bytes: &[u8], facts: &mut MetricsWireFacts) -> Result<(), IngestError> {
    if let Some(value) = last_bytes_field(bytes, 3)? {
        facts.add_value_bytes(value.len())?;
    }
    visit_merged_scope(bytes, 1, facts)?;
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(_)) | (3, WireValue::Bytes(_)) => {}
            (2, WireValue::Bytes(body)) => {
                facts.add_decode_bytes(size_of::<Metric>())?;
                visit_metric(body, facts)?;
            }
            (1..=3, _) => return Err(wrong_wire("scope metrics")),
            _ => {}
        }
    }
    Ok(())
}

/// Accounts for the final retained data oneof and metric metadata.
///
/// # Errors
///
/// Returns an ingest refusal for invalid framing, wire types, or bounded facts.
fn visit_metric(bytes: &[u8], facts: &mut MetricsWireFacts) -> Result<(), IngestError> {
    for tag in 1..=3 {
        if let Some(value) = last_bytes_field(bytes, tag)? {
            facts.add_value_bytes(value.len())?;
        }
    }
    let selection = select_message_oneof(bytes, &[5, 7, 9, 10, 11])?;
    let mut ordinal = 0_usize;
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1..=3, WireValue::Bytes(_)) => {}
            (12, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
            (5 | 7 | 9 | 10 | 11, WireValue::Bytes(body)) => {
                ordinal = ordinal
                    .checked_add(1)
                    .ok_or_else(|| malformed("metric data occurrence overflow"))?;
                if ordinal >= selection.run_start && tag == selection.tag {
                    visit_metric_data(tag, body, facts)?;
                }
            }
            (1 | 2 | 3 | 5 | 7 | 9 | 10 | 11 | 12, _) => {
                return Err(wrong_wire("metric"));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Accounts for one retained occurrence of a metric aggregation message.
///
/// # Errors
///
/// Returns an ingest refusal for invalid framing, wire types, or bounded facts.
fn visit_metric_data(
    tag: u32,
    bytes: &[u8],
    facts: &mut MetricsWireFacts,
) -> Result<(), IngestError> {
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((field_tag, value)) = fields.next()? {
        match (tag, field_tag, value) {
            (5 | 7, 1, WireValue::Bytes(body)) => {
                facts.add_record::<NumberDataPoint>()?;
                visit_number_point(body, facts)?;
            }
            (9, 1, WireValue::Bytes(body)) => {
                facts.add_record::<HistogramDataPoint>()?;
                visit_histogram_point(body, facts)?;
            }
            (10, 1, WireValue::Bytes(body)) => {
                facts.add_record::<ExponentialHistogramDataPoint>()?;
                visit_exponential_point(body, facts)?;
            }
            (11, 1, WireValue::Bytes(body)) => {
                facts.add_record::<SummaryDataPoint>()?;
                visit_summary_point(body, facts)?;
            }
            (7 | 9 | 10, 2, WireValue::Varint(_)) | (7, 3, WireValue::Varint(_)) => {}
            (5, 1, _) | (7, 1..=3, _) | (9 | 10, 1..=2, _) | (11, 1, _) => {
                return Err(wrong_wire("metric aggregation"));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Accounts for a scalar number point and its exemplars.
///
/// # Errors
///
/// Returns an ingest refusal for invalid framing, wire types, or bounded facts.
fn visit_number_point(bytes: &[u8], facts: &mut MetricsWireFacts) -> Result<(), IngestError> {
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (7, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
            (5, WireValue::Bytes(exemplar)) => {
                facts.add_decode_bytes(size_of::<Exemplar>())?;
                visit_exemplar(exemplar, facts)?;
            }
            (2 | 3 | 4 | 6, WireValue::Fixed64(_)) | (8, WireValue::Varint(_)) => {}
            (2..=8, _) => return Err(wrong_wire("number data point")),
            _ => {}
        }
    }
    Ok(())
}

/// Accounts for an explicit histogram point, including packed numeric arrays.
///
/// # Errors
///
/// Returns an ingest refusal for invalid framing, wire types, or bounded facts.
fn visit_histogram_point(bytes: &[u8], facts: &mut MetricsWireFacts) -> Result<(), IngestError> {
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (9, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
            (8, WireValue::Bytes(exemplar)) => {
                facts.add_decode_bytes(size_of::<Exemplar>())?;
                visit_exemplar(exemplar, facts)?;
            }
            (6 | 7, WireValue::Fixed64(_)) => facts.add_primitive::<u64>(1)?,
            (6 | 7, WireValue::Bytes(packed)) => {
                let count = packed_fixed64_count(packed)?;
                facts.add_primitive::<u64>(count)?;
            }
            (2..=5 | 11 | 12, WireValue::Fixed64(_)) | (10, WireValue::Varint(_)) => {}
            (2..=12, _) => return Err(wrong_wire("histogram data point")),
            _ => {}
        }
    }
    Ok(())
}

/// Accounts for an exponential histogram point and both optional bucket ranges.
///
/// # Errors
///
/// Returns an ingest refusal for invalid framing, wire types, or bounded facts.
fn visit_exponential_point(bytes: &[u8], facts: &mut MetricsWireFacts) -> Result<(), IngestError> {
    let positive = message_occurrence_count(bytes, 8)?;
    let negative = message_occurrence_count(bytes, 9)?;
    if positive != 0 {
        visit_merged_buckets(bytes, 8, facts)?;
    }
    if negative != 0 {
        visit_merged_buckets(bytes, 9, facts)?;
    }
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
            (11, WireValue::Bytes(exemplar)) => {
                facts.add_decode_bytes(size_of::<Exemplar>())?;
                visit_exemplar(exemplar, facts)?;
            }
            (8 | 9, WireValue::Bytes(_)) => {}
            (2..=5 | 7 | 12..=14, WireValue::Fixed64(_)) | (6 | 10, WireValue::Varint(_)) => {}
            (1..=14, _) => return Err(wrong_wire("exponential histogram data point")),
            _ => {}
        }
    }
    Ok(())
}

/// Accounts for the merged repeated counts in one optional bucket range.
///
/// # Errors
///
/// Returns an ingest refusal for invalid framing, wire types, or material overflow.
fn visit_merged_buckets(
    bytes: &[u8],
    wanted_tag: u32,
    facts: &mut MetricsWireFacts,
) -> Result<(), IngestError> {
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        if tag != wanted_tag {
            continue;
        }
        let WireValue::Bytes(body) = value else {
            return Err(wrong_wire("exponential histogram buckets"));
        };
        let mut bucket_fields = WireFields::with_group_depth(body, facts.limits.value_depth);
        while let Some((bucket_tag, bucket_value)) = bucket_fields.next()? {
            match (bucket_tag, bucket_value) {
                (1, WireValue::Varint(_)) => {}
                (2, WireValue::Varint(_)) => facts.add_primitive::<u64>(1)?,
                (2, WireValue::Bytes(packed)) => {
                    facts.add_primitive::<u64>(packed_varint_count(packed)?)?;
                }
                (1 | 2, _) => return Err(wrong_wire("exponential histogram buckets")),
                _ => {}
            }
        }
    }
    Ok(())
}

/// Accounts for a summary point and every quantile layout.
///
/// # Errors
///
/// Returns an ingest refusal for invalid framing, wire types, or bounded facts.
fn visit_summary_point(bytes: &[u8], facts: &mut MetricsWireFacts) -> Result<(), IngestError> {
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (7, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
            (6, WireValue::Bytes(quantile)) => {
                facts.add_decode_bytes(size_of::<summary_data_point::ValueAtQuantile>())?;
                validate_quantile(quantile, facts.limits.value_depth)?;
            }
            (2..=5, WireValue::Fixed64(_)) | (8, WireValue::Varint(_)) => {}
            (2..=8, _) => return Err(wrong_wire("summary data point")),
            _ => {}
        }
    }
    Ok(())
}

/// Validates one quantile message's two fixed64 fields.
///
/// # Errors
///
/// Returns a malformed-request error for a known field with the wrong wire type.
fn validate_quantile(bytes: &[u8], depth: usize) -> Result<(), IngestError> {
    let mut fields = WireFields::with_group_depth(bytes, depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1 | 2, WireValue::Fixed64(_)) => {}
            (1 | 2, _) => return Err(wrong_wire("summary quantile")),
            _ => {}
        }
    }
    Ok(())
}

/// Accounts for one exemplar, its IDs, and filtered attributes.
///
/// # Errors
///
/// Returns an ingest refusal for invalid framing, wire types, or bounded facts.
fn visit_exemplar(bytes: &[u8], facts: &mut MetricsWireFacts) -> Result<(), IngestError> {
    for tag in [4, 5] {
        if let Some(value) = last_bytes_field(bytes, tag)? {
            facts.add_value_bytes(value.len())?;
        }
    }
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (7, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
            (4 | 5, WireValue::Bytes(_)) => {}
            (2 | 3 | 6, WireValue::Fixed64(_)) => {}
            (2..=7, _) => return Err(wrong_wire("metric exemplar")),
            _ => {}
        }
    }
    Ok(())
}

/// Accounts for every occurrence merged into one optional resource.
///
/// # Errors
///
/// Returns an ingest refusal for invalid framing, wire types, or bounded facts.
fn visit_merged_resource(
    bytes: &[u8],
    wanted_tag: u32,
    facts: &mut MetricsWireFacts,
) -> Result<(), IngestError> {
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        if tag != wanted_tag {
            continue;
        }
        let WireValue::Bytes(resource) = value else {
            return Err(wrong_wire("metrics resource"));
        };
        let mut resource_fields = WireFields::with_group_depth(resource, facts.limits.value_depth);
        while let Some((resource_tag, resource_value)) = resource_fields.next()? {
            match (resource_tag, resource_value) {
                (1, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
                (2, WireValue::Varint(_)) => {}
                (3, WireValue::Bytes(entity)) => {
                    facts.add_decode_bytes(size_of::<EntityRef>())?;
                    visit_entity_ref(entity, facts)?;
                }
                (1..=3, _) => return Err(wrong_wire("metrics resource")),
                _ => {}
            }
        }
    }
    Ok(())
}

/// Accounts for one entity reference's retained singular and repeated strings.
///
/// # Errors
///
/// Returns an ingest refusal for invalid framing, UTF-8 shape, or bounded facts.
fn visit_entity_ref(bytes: &[u8], facts: &mut MetricsWireFacts) -> Result<(), IngestError> {
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

/// Accounts for every occurrence merged into one instrumentation scope.
///
/// # Errors
///
/// Returns an ingest refusal for invalid framing, wire types, or bounded facts.
fn visit_merged_scope(
    bytes: &[u8],
    wanted_tag: u32,
    facts: &mut MetricsWireFacts,
) -> Result<(), IngestError> {
    for child_tag in [1, 2] {
        if let Some(value) = last_bytes_across_messages(bytes, wanted_tag, child_tag)? {
            facts.add_value_bytes(value.len())?;
        }
    }
    let mut fields = WireFields::with_group_depth(bytes, facts.limits.value_depth);
    while let Some((tag, value)) = fields.next()? {
        if tag != wanted_tag {
            continue;
        }
        let WireValue::Bytes(scope) = value else {
            return Err(wrong_wire("instrumentation scope"));
        };
        let mut scope_fields = WireFields::with_group_depth(scope, facts.limits.value_depth);
        while let Some((scope_tag, scope_value)) = scope_fields.next()? {
            match (scope_tag, scope_value) {
                (1 | 2, WireValue::Bytes(_)) | (4, WireValue::Varint(_)) => {}
                (3, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
                (1..=4, _) => return Err(wrong_wire("instrumentation scope")),
                _ => {}
            }
        }
    }
    Ok(())
}

/// Accounts for one attribute and its recursively retained `AnyValue`.
///
/// # Errors
///
/// Returns an ingest refusal for invalid framing, nesting, or bounded facts.
fn visit_attribute(
    bytes: &[u8],
    value_depth: usize,
    facts: &mut MetricsWireFacts,
) -> Result<(), IngestError> {
    facts.add_attribute()?;
    if let Some(key) = last_bytes_field(bytes, 1)? {
        facts.add_value_bytes(key.len())?;
    }
    visit_merged_any_value(
        MessageBodies::Repeated {
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

/// Borrowed message bodies merged by protobuf singular-message semantics.
#[derive(Clone, Copy)]
enum MessageBodies<'a> {
    /// One ordinary nested message.
    Single(&'a [u8]),
    /// Repeated occurrences of one singular field within a parent.
    Repeated {
        /// Encoded parent body.
        parent: &'a [u8],
        /// Singular child tag.
        tag: u32,
    },
}

impl MessageBodies<'_> {
    /// Visits encoded bodies in merge order without collecting descriptors.
    ///
    /// # Errors
    ///
    /// Returns a malformed-request error for invalid outer framing or a visitor error.
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
                            return Err(wrong_wire("optional OTLP message"));
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

/// Final oneof merge run selected across one or more message bodies.
#[derive(Clone, Copy, Default)]
struct OneofSelection {
    /// Selected tag, or zero when no known variant occurs.
    tag: u32,
    /// One-based ordinal where the final same-variant merge run starts.
    run_start: usize,
    /// Repeated nested elements retained by that run.
    nested_count: usize,
}

/// Accounts for only the retained final oneof run of an `AnyValue`.
///
/// # Errors
///
/// Returns an ingest refusal for invalid wire types, excess depth, or capacity limits.
fn visit_merged_any_value(
    bodies: MessageBodies<'_>,
    depth: usize,
    facts: &mut MetricsWireFacts,
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
                                visit_merged_any_value(
                                    MessageBodies::Single(value),
                                    depth + 1,
                                    facts,
                                )?;
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

/// Selects the final retained oneof run across merged `AnyValue` bodies.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, wire types, or overflow.
fn select_any_value_bodies(bodies: MessageBodies<'_>) -> Result<OneofSelection, IngestError> {
    let mut selection = OneofSelection::default();
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
                selection = OneofSelection {
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

/// Validates the generated wire type for one `AnyValue` variant.
///
/// # Errors
///
/// Returns a malformed-request error for a wrong wire type.
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

/// Constructs a metrics request using only capacities proven by `plan`.
///
/// # Errors
///
/// Returns a malformed-request error if the bytes differ in length from the
/// preflighted input or contain invalid UTF-8, framing, or known wire types.
pub(crate) fn decode_metrics_protobuf(
    bytes: &[u8],
    plan: MetricsDecodePlan,
) -> Result<ExportMetricsServiceRequest, IngestError> {
    if bytes.len() != plan.wire_bytes {
        return Err(malformed("metrics decode input differs from preflight"));
    }
    let count = repeated_message_count(bytes, 1)?;
    let mut resource_metrics = Vec::with_capacity(count);
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(body)) => resource_metrics.push(decode_resource_metrics(body)?),
            (1, _) => return Err(wrong_wire("metrics resource group")),
            _ => {}
        }
    }
    Ok(ExportMetricsServiceRequest { resource_metrics })
}

/// Constructs one resource metrics group with exact repeated capacities.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_resource_metrics(bytes: &[u8]) -> Result<ResourceMetrics, IngestError> {
    let scope_count = repeated_message_count(bytes, 2)?;
    let mut scope_metrics = Vec::with_capacity(scope_count);
    let resource = decode_merged_resource(bytes, 1)?;
    let schema_url = decode_last_string(bytes, 3)?;
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(_)) | (3, WireValue::Bytes(_)) => {}
            (2, WireValue::Bytes(body)) => scope_metrics.push(decode_scope_metrics(body)?),
            (1..=3, _) => return Err(wrong_wire("resource metrics")),
            _ => {}
        }
    }
    Ok(ResourceMetrics {
        resource,
        scope_metrics,
        schema_url,
    })
}

/// Constructs one scope metrics group with exact repeated capacities.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_scope_metrics(bytes: &[u8]) -> Result<ScopeMetrics, IngestError> {
    let metric_count = repeated_message_count(bytes, 2)?;
    let mut metrics = Vec::with_capacity(metric_count);
    let scope = decode_merged_scope(bytes, 1)?;
    let schema_url = decode_last_string(bytes, 3)?;
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(_)) | (3, WireValue::Bytes(_)) => {}
            (2, WireValue::Bytes(body)) => metrics.push(decode_metric(body)?),
            (1..=3, _) => return Err(wrong_wire("scope metrics")),
            _ => {}
        }
    }
    Ok(ScopeMetrics {
        scope,
        metrics,
        schema_url,
    })
}

/// Constructs one metric and merges its final same-variant data run.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_metric(bytes: &[u8]) -> Result<Metric, IngestError> {
    let metadata_count = repeated_message_count(bytes, 12)?;
    let selection = select_message_oneof(bytes, &[5, 7, 9, 10, 11])?;
    let point_count = selected_nested_count(bytes, selection, 1)?;
    let mut metric = Metric {
        name: decode_last_string(bytes, 1)?,
        description: decode_last_string(bytes, 2)?,
        unit: decode_last_string(bytes, 3)?,
        metadata: Vec::with_capacity(metadata_count),
        data: initial_metric_data(selection.tag, point_count),
    };
    let mut ordinal = 0_usize;
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1..=3, WireValue::Bytes(_)) => {}
            (12, WireValue::Bytes(body)) => metric.metadata.push(decode_key_value(body)?),
            (5 | 7 | 9 | 10 | 11, WireValue::Bytes(body)) => {
                ordinal += 1;
                if ordinal >= selection.run_start && tag == selection.tag {
                    merge_metric_data(
                        body,
                        metric.data.as_mut().ok_or_else(|| {
                            malformed("selected metric data variant was not initialized")
                        })?,
                    )?;
                }
            }
            (1 | 2 | 3 | 5 | 7 | 9 | 10 | 11 | 12, _) => {
                return Err(wrong_wire("metric"));
            }
            _ => {}
        }
    }
    Ok(metric)
}

/// Creates the selected aggregation with its exact data-point capacity.
fn initial_metric_data(tag: u32, point_count: usize) -> Option<metric::Data> {
    match tag {
        5 => Some(metric::Data::Gauge(Gauge {
            data_points: Vec::with_capacity(point_count),
        })),
        7 => Some(metric::Data::Sum(Sum {
            data_points: Vec::with_capacity(point_count),
            ..Sum::default()
        })),
        9 => Some(metric::Data::Histogram(Histogram {
            data_points: Vec::with_capacity(point_count),
            ..Histogram::default()
        })),
        10 => Some(metric::Data::ExponentialHistogram(ExponentialHistogram {
            data_points: Vec::with_capacity(point_count),
            ..ExponentialHistogram::default()
        })),
        11 => Some(metric::Data::Summary(Summary {
            data_points: Vec::with_capacity(point_count),
        })),
        _ => None,
    }
}

/// Merges one encoded occurrence into its selected aggregation.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or wire types.
fn merge_metric_data(bytes: &[u8], data: &mut metric::Data) -> Result<(), IngestError> {
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (&mut *data, tag, value) {
            (metric::Data::Gauge(gauge), 1, WireValue::Bytes(body)) => {
                gauge.data_points.push(decode_number_point(body)?);
            }
            (metric::Data::Sum(sum), 1, WireValue::Bytes(body)) => {
                sum.data_points.push(decode_number_point(body)?);
            }
            (metric::Data::Sum(sum), 2, WireValue::Varint(value)) => {
                sum.aggregation_temporality = protobuf_i32(value);
            }
            (metric::Data::Sum(sum), 3, WireValue::Varint(value)) => {
                sum.is_monotonic = value != 0;
            }
            (metric::Data::Histogram(histogram), 1, WireValue::Bytes(body)) => {
                histogram.data_points.push(decode_histogram_point(body)?);
            }
            (metric::Data::Histogram(histogram), 2, WireValue::Varint(value)) => {
                histogram.aggregation_temporality = protobuf_i32(value);
            }
            (metric::Data::ExponentialHistogram(histogram), 1, WireValue::Bytes(body)) => {
                histogram.data_points.push(decode_exponential_point(body)?);
            }
            (metric::Data::ExponentialHistogram(histogram), 2, WireValue::Varint(value)) => {
                histogram.aggregation_temporality = protobuf_i32(value);
            }
            (metric::Data::Summary(summary), 1, WireValue::Bytes(body)) => {
                summary.data_points.push(decode_summary_point(body)?);
            }
            (metric::Data::Gauge(_), 1, _)
            | (metric::Data::Sum(_), 1..=3, _)
            | (metric::Data::Histogram(_), 1 | 2, _)
            | (metric::Data::ExponentialHistogram(_), 1 | 2, _)
            | (metric::Data::Summary(_), 1, _) => {
                return Err(wrong_wire("metric aggregation"));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Constructs one scalar point with exact attribute and exemplar capacities.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or wire types.
fn decode_number_point(bytes: &[u8]) -> Result<NumberDataPoint, IngestError> {
    let mut point = NumberDataPoint {
        attributes: Vec::with_capacity(repeated_message_count(bytes, 7)?),
        exemplars: Vec::with_capacity(repeated_message_count(bytes, 5)?),
        ..NumberDataPoint::default()
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (2, WireValue::Fixed64(value)) => point.start_time_unix_nano = value,
            (3, WireValue::Fixed64(value)) => point.time_unix_nano = value,
            (4, WireValue::Fixed64(value)) => {
                point.value = Some(number_data_point::Value::AsDouble(f64::from_bits(value)));
            }
            (5, WireValue::Bytes(body)) => point.exemplars.push(decode_exemplar(body)?),
            (6, WireValue::Fixed64(value)) => {
                point.value = Some(number_data_point::Value::AsInt(protobuf_i64(value)));
            }
            (7, WireValue::Bytes(body)) => point.attributes.push(decode_key_value(body)?),
            (8, WireValue::Varint(value)) => point.flags = protobuf_u32(value),
            (2..=8, _) => return Err(wrong_wire("number data point")),
            _ => {}
        }
    }
    Ok(point)
}

/// Constructs one explicit histogram point with exact primitive capacities.
///
/// # Errors
///
/// Returns a malformed-request error for invalid packed data, framing, or wire types.
fn decode_histogram_point(bytes: &[u8]) -> Result<HistogramDataPoint, IngestError> {
    let mut point = HistogramDataPoint {
        attributes: Vec::with_capacity(repeated_message_count(bytes, 9)?),
        bucket_counts: Vec::with_capacity(repeated_fixed64_count(bytes, 6)?),
        explicit_bounds: Vec::with_capacity(repeated_fixed64_count(bytes, 7)?),
        exemplars: Vec::with_capacity(repeated_message_count(bytes, 8)?),
        ..HistogramDataPoint::default()
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (2, WireValue::Fixed64(value)) => point.start_time_unix_nano = value,
            (3, WireValue::Fixed64(value)) => point.time_unix_nano = value,
            (4, WireValue::Fixed64(value)) => point.count = value,
            (5, WireValue::Fixed64(value)) => point.sum = Some(f64::from_bits(value)),
            (6, WireValue::Fixed64(value)) => point.bucket_counts.push(value),
            (6, WireValue::Bytes(packed)) => extend_fixed64(packed, &mut point.bucket_counts)?,
            (7, WireValue::Fixed64(value)) => point.explicit_bounds.push(f64::from_bits(value)),
            (7, WireValue::Bytes(packed)) => extend_doubles(packed, &mut point.explicit_bounds)?,
            (8, WireValue::Bytes(body)) => point.exemplars.push(decode_exemplar(body)?),
            (9, WireValue::Bytes(body)) => point.attributes.push(decode_key_value(body)?),
            (10, WireValue::Varint(value)) => point.flags = protobuf_u32(value),
            (11, WireValue::Fixed64(value)) => point.min = Some(f64::from_bits(value)),
            (12, WireValue::Fixed64(value)) => point.max = Some(f64::from_bits(value)),
            (2..=12, _) => return Err(wrong_wire("histogram data point")),
            _ => {}
        }
    }
    Ok(point)
}

/// Constructs one exponential histogram point with exact bucket capacities.
///
/// # Errors
///
/// Returns a malformed-request error for invalid packed data, framing, or wire types.
fn decode_exponential_point(bytes: &[u8]) -> Result<ExponentialHistogramDataPoint, IngestError> {
    let mut point = ExponentialHistogramDataPoint {
        attributes: Vec::with_capacity(repeated_message_count(bytes, 1)?),
        positive: decode_merged_buckets(bytes, 8)?,
        negative: decode_merged_buckets(bytes, 9)?,
        exemplars: Vec::with_capacity(repeated_message_count(bytes, 11)?),
        ..ExponentialHistogramDataPoint::default()
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(body)) => point.attributes.push(decode_key_value(body)?),
            (2, WireValue::Fixed64(value)) => point.start_time_unix_nano = value,
            (3, WireValue::Fixed64(value)) => point.time_unix_nano = value,
            (4, WireValue::Fixed64(value)) => point.count = value,
            (5, WireValue::Fixed64(value)) => point.sum = Some(f64::from_bits(value)),
            (6, WireValue::Varint(value)) => point.scale = decode_zigzag_i32(value),
            (7, WireValue::Fixed64(value)) => point.zero_count = value,
            (8 | 9, WireValue::Bytes(_)) => {}
            (10, WireValue::Varint(value)) => point.flags = protobuf_u32(value),
            (11, WireValue::Bytes(body)) => point.exemplars.push(decode_exemplar(body)?),
            (12, WireValue::Fixed64(value)) => point.min = Some(f64::from_bits(value)),
            (13, WireValue::Fixed64(value)) => point.max = Some(f64::from_bits(value)),
            (14, WireValue::Fixed64(value)) => point.zero_threshold = f64::from_bits(value),
            (1..=14, _) => return Err(wrong_wire("exponential histogram data point")),
            _ => {}
        }
    }
    Ok(point)
}

/// Merges every occurrence of one optional bucket message.
///
/// # Errors
///
/// Returns a malformed-request error for invalid packed data, framing, or wire types.
fn decode_merged_buckets(
    bytes: &[u8],
    wanted_tag: u32,
) -> Result<Option<exponential_histogram_data_point::Buckets>, IngestError> {
    let occurrences = message_occurrence_count(bytes, wanted_tag)?;
    if occurrences == 0 {
        return Ok(None);
    }
    let count = repeated_varint_count_across_messages(bytes, wanted_tag, 2)?;
    let mut buckets = exponential_histogram_data_point::Buckets {
        bucket_counts: Vec::with_capacity(count),
        ..exponential_histogram_data_point::Buckets::default()
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if tag != wanted_tag {
            continue;
        }
        let WireValue::Bytes(body) = value else {
            return Err(wrong_wire("exponential histogram buckets"));
        };
        let mut bucket_fields = WireFields::new(body);
        while let Some((bucket_tag, bucket_value)) = bucket_fields.next()? {
            match (bucket_tag, bucket_value) {
                (1, WireValue::Varint(value)) => buckets.offset = decode_zigzag_i32(value),
                (2, WireValue::Varint(value)) => buckets.bucket_counts.push(value),
                (2, WireValue::Bytes(packed)) => {
                    extend_varints(packed, &mut buckets.bucket_counts)?
                }
                (1 | 2, _) => return Err(wrong_wire("exponential histogram buckets")),
                _ => {}
            }
        }
    }
    Ok(Some(buckets))
}

/// Constructs one summary point with exact quantile and attribute capacities.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or wire types.
fn decode_summary_point(bytes: &[u8]) -> Result<SummaryDataPoint, IngestError> {
    let mut point = SummaryDataPoint {
        attributes: Vec::with_capacity(repeated_message_count(bytes, 7)?),
        quantile_values: Vec::with_capacity(repeated_message_count(bytes, 6)?),
        ..SummaryDataPoint::default()
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (2, WireValue::Fixed64(value)) => point.start_time_unix_nano = value,
            (3, WireValue::Fixed64(value)) => point.time_unix_nano = value,
            (4, WireValue::Fixed64(value)) => point.count = value,
            (5, WireValue::Fixed64(value)) => point.sum = f64::from_bits(value),
            (6, WireValue::Bytes(body)) => point.quantile_values.push(decode_quantile(body)?),
            (7, WireValue::Bytes(body)) => point.attributes.push(decode_key_value(body)?),
            (8, WireValue::Varint(value)) => point.flags = protobuf_u32(value),
            (2..=8, _) => return Err(wrong_wire("summary data point")),
            _ => {}
        }
    }
    Ok(point)
}

/// Constructs one summary quantile value.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or wire types.
fn decode_quantile(bytes: &[u8]) -> Result<summary_data_point::ValueAtQuantile, IngestError> {
    let mut value = summary_data_point::ValueAtQuantile::default();
    let mut fields = WireFields::new(bytes);
    while let Some((tag, wire_value)) = fields.next()? {
        match (tag, wire_value) {
            (1, WireValue::Fixed64(bits)) => value.quantile = f64::from_bits(bits),
            (2, WireValue::Fixed64(bits)) => value.value = f64::from_bits(bits),
            (1 | 2, _) => return Err(wrong_wire("summary quantile")),
            _ => {}
        }
    }
    Ok(value)
}

/// Constructs one exemplar with exact filtered-attribute and ID capacities.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or wire types.
fn decode_exemplar(bytes: &[u8]) -> Result<Exemplar, IngestError> {
    let mut exemplar = Exemplar {
        filtered_attributes: Vec::with_capacity(repeated_message_count(bytes, 7)?),
        span_id: decode_last_bytes(bytes, 4)?,
        trace_id: decode_last_bytes(bytes, 5)?,
        ..Exemplar::default()
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (2, WireValue::Fixed64(value)) => exemplar.time_unix_nano = value,
            (3, WireValue::Fixed64(value)) => {
                exemplar.value = Some(exemplar::Value::AsDouble(f64::from_bits(value)));
            }
            (4 | 5, WireValue::Bytes(_)) => {}
            (6, WireValue::Fixed64(value)) => {
                exemplar.value = Some(exemplar::Value::AsInt(protobuf_i64(value)));
            }
            (7, WireValue::Bytes(body)) => {
                exemplar.filtered_attributes.push(decode_key_value(body)?);
            }
            (2..=7, _) => return Err(wrong_wire("metric exemplar")),
            _ => {}
        }
    }
    Ok(exemplar)
}

/// Merges every occurrence of one optional resource message.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_merged_resource(bytes: &[u8], wanted_tag: u32) -> Result<Option<Resource>, IngestError> {
    let occurrences = message_occurrence_count(bytes, wanted_tag)?;
    if occurrences == 0 {
        return Ok(None);
    }
    let attribute_count = repeated_count_across_messages(bytes, wanted_tag, 1)?;
    let entity_count = repeated_count_across_messages(bytes, wanted_tag, 3)?;
    let mut resource = Resource {
        attributes: Vec::with_capacity(attribute_count),
        entity_refs: Vec::with_capacity(entity_count),
        ..Resource::default()
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if tag != wanted_tag {
            continue;
        }
        let WireValue::Bytes(body) = value else {
            return Err(wrong_wire("metrics resource"));
        };
        let mut resource_fields = WireFields::new(body);
        while let Some((resource_tag, resource_value)) = resource_fields.next()? {
            match (resource_tag, resource_value) {
                (1, WireValue::Bytes(attribute)) => {
                    resource.attributes.push(decode_key_value(attribute)?);
                }
                (2, WireValue::Varint(value)) => {
                    resource.dropped_attributes_count = protobuf_u32(value);
                }
                (3, WireValue::Bytes(entity)) => {
                    resource.entity_refs.push(decode_entity_ref(entity)?);
                }
                (1..=3, _) => return Err(wrong_wire("metrics resource")),
                _ => {}
            }
        }
    }
    Ok(Some(resource))
}

/// Constructs one entity reference with exact string capacities.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_entity_ref(bytes: &[u8]) -> Result<EntityRef, IngestError> {
    let mut entity = EntityRef {
        schema_url: decode_last_string(bytes, 1)?,
        r#type: decode_last_string(bytes, 2)?,
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

/// Merges every occurrence of one optional instrumentation scope.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_merged_scope(
    bytes: &[u8],
    wanted_tag: u32,
) -> Result<Option<InstrumentationScope>, IngestError> {
    let occurrences = message_occurrence_count(bytes, wanted_tag)?;
    if occurrences == 0 {
        return Ok(None);
    }
    let attribute_count = repeated_count_across_messages(bytes, wanted_tag, 3)?;
    let mut scope = InstrumentationScope {
        name: decode_last_string_across_messages(bytes, wanted_tag, 1)?,
        version: decode_last_string_across_messages(bytes, wanted_tag, 2)?,
        attributes: Vec::with_capacity(attribute_count),
        ..InstrumentationScope::default()
    };
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if tag != wanted_tag {
            continue;
        }
        let WireValue::Bytes(body) = value else {
            return Err(wrong_wire("instrumentation scope"));
        };
        let mut scope_fields = WireFields::new(body);
        while let Some((scope_tag, scope_value)) = scope_fields.next()? {
            match (scope_tag, scope_value) {
                (1 | 2, WireValue::Bytes(_)) => {}
                (3, WireValue::Bytes(attribute)) => {
                    scope.attributes.push(decode_key_value(attribute)?);
                }
                (4, WireValue::Varint(value)) => {
                    scope.dropped_attributes_count = protobuf_u32(value);
                }
                (1..=4, _) => return Err(wrong_wire("instrumentation scope")),
                _ => {}
            }
        }
    }
    Ok(Some(scope))
}

/// Constructs one key/value entry and its merged recursive value.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_key_value(bytes: &[u8]) -> Result<KeyValue, IngestError> {
    let key = decode_last_string(bytes, 1)?;
    let value = decode_merged_any_value(bytes, 2)?;
    let mut fields = WireFields::new(bytes);
    while let Some((tag, wire_value)) = fields.next()? {
        match (tag, wire_value) {
            (1 | 2, WireValue::Bytes(_)) => {}
            (1 | 2, _) => return Err(wrong_wire("key/value")),
            _ => {}
        }
    }
    Ok(KeyValue { key, value })
}

/// Constructs one ordinary recursive `AnyValue`.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_any_value(bytes: &[u8]) -> Result<AnyValue, IngestError> {
    decode_any_value_bodies(MessageBodies::Single(bytes))
}

/// Merges every occurrence of one singular `AnyValue` field.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_merged_any_value(bytes: &[u8], wanted_tag: u32) -> Result<Option<AnyValue>, IngestError> {
    let bodies = MessageBodies::Repeated {
        parent: bytes,
        tag: wanted_tag,
    };
    if bodies.try_for_each(|_| Ok(()))? == 0 {
        return Ok(None);
    }
    decode_any_value_bodies(bodies).map(Some)
}

/// Constructs the final retained oneof run across merged `AnyValue` bodies.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, UTF-8, or wire types.
fn decode_any_value_bodies(bodies: MessageBodies<'_>) -> Result<AnyValue, IngestError> {
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

/// Appends one encoded array body into its fixed-capacity destination.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or wire types.
fn merge_array_value(bytes: &[u8], array: &mut ArrayValue) -> Result<(), IngestError> {
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(body)) => array.values.push(decode_any_value(body)?),
            (1, _) => return Err(wrong_wire("OTLP value array")),
            _ => {}
        }
    }
    Ok(())
}

/// Appends one encoded key/value-list body into its fixed-capacity destination.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or wire types.
fn merge_key_value_list(bytes: &[u8], list: &mut KeyValueList) -> Result<(), IngestError> {
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        match (tag, value) {
            (1, WireValue::Bytes(body)) => list.values.push(decode_key_value(body)?),
            (1, _) => return Err(wrong_wire("OTLP key/value list")),
            _ => {}
        }
    }
    Ok(())
}

/// Selects the final same-tag run of a message-valued oneof.
///
/// # Errors
///
/// Returns a malformed-request error for a known tag with the wrong wire type.
fn select_message_oneof(bytes: &[u8], tags: &[u32]) -> Result<OneofSelection, IngestError> {
    let mut selection = OneofSelection::default();
    let mut ordinal = 0_usize;
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if !tags.contains(&tag) {
            continue;
        }
        if !matches!(value, WireValue::Bytes(_)) {
            return Err(wrong_wire("message oneof"));
        }
        ordinal = ordinal
            .checked_add(1)
            .ok_or_else(|| malformed("message oneof occurrence overflow"))?;
        if selection.tag != tag {
            selection = OneofSelection {
                tag,
                run_start: ordinal,
                nested_count: 0,
            };
        }
    }
    Ok(selection)
}

/// Counts one nested repeated message across the selected oneof run.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or count overflow.
fn selected_nested_count(
    bytes: &[u8],
    selection: OneofSelection,
    nested_tag: u32,
) -> Result<usize, IngestError> {
    let mut count = 0_usize;
    let mut ordinal = 0_usize;
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if !matches!(tag, 5 | 7 | 9 | 10 | 11) {
            continue;
        }
        ordinal += 1;
        let WireValue::Bytes(body) = value else {
            return Err(wrong_wire("metric data"));
        };
        if ordinal >= selection.run_start && tag == selection.tag {
            count = count
                .checked_add(repeated_message_count(body, nested_tag)?)
                .ok_or_else(|| malformed("metric data-point count overflow"))?;
        }
    }
    Ok(count)
}

/// Counts occurrences of one length-delimited field.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, wire type, or overflow.
fn repeated_message_count(bytes: &[u8], wanted_tag: u32) -> Result<usize, IngestError> {
    let mut count = 0_usize;
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if tag == wanted_tag {
            if !matches!(value, WireValue::Bytes(_)) {
                return Err(wrong_wire("repeated length-delimited field"));
            }
            count = count
                .checked_add(1)
                .ok_or_else(|| malformed("repeated-field count overflow"))?;
        }
    }
    Ok(count)
}

/// Counts occurrences of one singular message field.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or wire type.
fn message_occurrence_count(bytes: &[u8], wanted_tag: u32) -> Result<usize, IngestError> {
    repeated_message_count(bytes, wanted_tag)
}

/// Counts a repeated child across all occurrences of a singular parent message.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, wire types, or overflow.
fn repeated_count_across_messages(
    bytes: &[u8],
    parent_tag: u32,
    child_tag: u32,
) -> Result<usize, IngestError> {
    let mut count = 0_usize;
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if tag == parent_tag {
            let WireValue::Bytes(body) = value else {
                return Err(wrong_wire("optional message"));
            };
            count = count
                .checked_add(repeated_message_count(body, child_tag)?)
                .ok_or_else(|| malformed("nested repeated-field count overflow"))?;
        }
    }
    Ok(count)
}

/// Returns the last retained length-delimited occurrence of one field.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or wire type.
fn last_bytes_field(bytes: &[u8], wanted_tag: u32) -> Result<Option<&[u8]>, IngestError> {
    let mut retained = None;
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if tag == wanted_tag {
            let WireValue::Bytes(value) = value else {
                return Err(wrong_wire("singular length-delimited field"));
            };
            retained = Some(value);
        }
    }
    Ok(retained)
}

/// Returns the last child bytes across merged parent message occurrences.
///
/// # Errors
///
/// Returns a malformed-request error for invalid parent/child framing or wire types.
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
                return Err(wrong_wire("optional message"));
            };
            if let Some(value) = last_bytes_field(body, child_tag)? {
                retained = Some(value);
            }
        }
    }
    Ok(retained)
}

/// Decodes the final singular string into exact-capacity backing.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, wire type, or UTF-8.
fn decode_last_string(bytes: &[u8], tag: u32) -> Result<String, IngestError> {
    last_bytes_field(bytes, tag)?
        .map(fixed_string)
        .transpose()
        .map(Option::unwrap_or_default)
}

/// Decodes the final child string across merged parent message occurrences.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, wire type, or UTF-8.
fn decode_last_string_across_messages(
    bytes: &[u8],
    parent_tag: u32,
    child_tag: u32,
) -> Result<String, IngestError> {
    last_bytes_across_messages(bytes, parent_tag, child_tag)?
        .map(fixed_string)
        .transpose()
        .map(Option::unwrap_or_default)
}

/// Decodes the final singular bytes field into exact-capacity backing.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing or wire type.
fn decode_last_bytes(bytes: &[u8], tag: u32) -> Result<Vec<u8>, IngestError> {
    Ok(last_bytes_field(bytes, tag)?.map_or_else(Vec::new, fixed_bytes))
}

/// Copies UTF-8 bytes into a string with exact capacity.
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

/// Copies bytes into a vector with exact capacity.
fn fixed_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut value = Vec::with_capacity(bytes.len());
    value.extend_from_slice(bytes);
    value
}

/// Counts fixed64 values across packed and unpacked occurrences.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, wire type, or packed length.
fn repeated_fixed64_count(bytes: &[u8], wanted_tag: u32) -> Result<usize, IngestError> {
    let mut count = 0_usize;
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if tag != wanted_tag {
            continue;
        }
        let amount = match value {
            WireValue::Fixed64(_) => 1,
            WireValue::Bytes(packed) => packed_fixed64_count(packed)?,
            _ => return Err(wrong_wire("repeated fixed64 field")),
        };
        count = count
            .checked_add(amount)
            .ok_or_else(|| malformed("repeated fixed64 count overflow"))?;
    }
    Ok(count)
}

/// Counts fixed64 elements in one packed payload.
///
/// # Errors
///
/// Returns a malformed-request error when the payload length is not divisible by eight.
fn packed_fixed64_count(bytes: &[u8]) -> Result<usize, IngestError> {
    if !bytes.len().is_multiple_of(8) {
        return Err(malformed("packed fixed64 payload has a partial value"));
    }
    Ok(bytes.len() / 8)
}

/// Counts protobuf varints in one packed payload.
///
/// # Errors
///
/// Returns a malformed-request error for a truncated or over-wide varint.
fn packed_varint_count(bytes: &[u8]) -> Result<usize, IngestError> {
    let mut count = 0_usize;
    let mut fields = WireFields::new(bytes);
    while fields.cursor < bytes.len() {
        fields.read_varint()?;
        count = count
            .checked_add(1)
            .ok_or_else(|| malformed("packed varint count overflow"))?;
    }
    Ok(count)
}

/// Counts repeated varints across merged singular message occurrences.
///
/// # Errors
///
/// Returns a malformed-request error for invalid framing, wire types, or overflow.
fn repeated_varint_count_across_messages(
    bytes: &[u8],
    parent_tag: u32,
    child_tag: u32,
) -> Result<usize, IngestError> {
    let mut count = 0_usize;
    let mut fields = WireFields::new(bytes);
    while let Some((tag, value)) = fields.next()? {
        if tag != parent_tag {
            continue;
        }
        let WireValue::Bytes(body) = value else {
            return Err(wrong_wire("optional repeated-varint message"));
        };
        let mut child_fields = WireFields::new(body);
        while let Some((tag, value)) = child_fields.next()? {
            if tag != child_tag {
                continue;
            }
            let amount = match value {
                WireValue::Varint(_) => 1,
                WireValue::Bytes(packed) => packed_varint_count(packed)?,
                _ => return Err(wrong_wire("repeated varint field")),
            };
            count = count
                .checked_add(amount)
                .ok_or_else(|| malformed("repeated varint count overflow"))?;
        }
    }
    Ok(count)
}

/// Appends packed fixed64 values without changing destination capacity.
///
/// # Errors
///
/// Returns a malformed-request error for a partial trailing value.
fn extend_fixed64(bytes: &[u8], values: &mut Vec<u64>) -> Result<(), IngestError> {
    packed_fixed64_count(bytes)?;
    for chunk in bytes.chunks_exact(8) {
        let raw: [u8; 8] = chunk
            .try_into()
            .map_err(|_| malformed("invalid packed fixed64 value"))?;
        values.push(u64::from_le_bytes(raw));
    }
    Ok(())
}

/// Appends packed doubles without changing destination capacity.
///
/// # Errors
///
/// Returns a malformed-request error for a partial trailing value.
fn extend_doubles(bytes: &[u8], values: &mut Vec<f64>) -> Result<(), IngestError> {
    packed_fixed64_count(bytes)?;
    for chunk in bytes.chunks_exact(8) {
        let raw: [u8; 8] = chunk
            .try_into()
            .map_err(|_| malformed("invalid packed double value"))?;
        values.push(f64::from_bits(u64::from_le_bytes(raw)));
    }
    Ok(())
}

/// Appends packed varints without changing destination capacity.
///
/// # Errors
///
/// Returns a malformed-request error for a truncated or over-wide varint.
fn extend_varints(bytes: &[u8], values: &mut Vec<u64>) -> Result<(), IngestError> {
    let mut fields = WireFields::new(bytes);
    while fields.cursor < bytes.len() {
        values.push(fields.read_varint()?);
    }
    Ok(())
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

/// Reinterprets a signed fixed64/varint's two's-complement bits.
fn protobuf_i64(value: u64) -> i64 {
    i64::from_le_bytes(value.to_le_bytes())
}

/// Decodes a protobuf zig-zag signed 32-bit value.
fn decode_zigzag_i32(value: u64) -> i32 {
    let low = protobuf_u32(value);
    let magnitude = low >> 1;
    let sign = 0_u32.wrapping_sub(low & 1);
    i32::from_le_bytes((magnitude ^ sign).to_le_bytes())
}

/// Constructs a stable wrong-wire-type refusal for a known field.
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
    use vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS;
    use wyrd_tonic::prost::Message;

    /// Creates one recursive attribute used across all metric shapes.
    fn attribute(key: &str) -> KeyValue {
        KeyValue {
            key: key.to_owned(),
            value: Some(AnyValue {
                value: Some(any_value::Value::KvlistValue(KeyValueList {
                    values: vec![KeyValue {
                        key: "nested".to_owned(),
                        value: Some(AnyValue {
                            value: Some(any_value::Value::ArrayValue(ArrayValue {
                                values: vec![AnyValue {
                                    value: Some(any_value::Value::BytesValue(vec![1, 2, 3])),
                                }],
                            })),
                        }),
                    }],
                })),
            }),
        }
    }

    /// Creates an exemplar with IDs and filtered attributes.
    fn exemplar_fixture() -> Exemplar {
        Exemplar {
            filtered_attributes: vec![attribute("filtered")],
            time_unix_nano: 44,
            span_id: vec![4; 8],
            trace_id: vec![5; 16],
            value: Some(exemplar::Value::AsDouble(2.5)),
        }
    }

    /// Creates a request covering every generated metric aggregation shape.
    fn metrics_fixture() -> ExportMetricsServiceRequest {
        let number = NumberDataPoint {
            attributes: vec![attribute("number")],
            start_time_unix_nano: 1,
            time_unix_nano: 2,
            exemplars: vec![exemplar_fixture()],
            flags: 1,
            value: Some(number_data_point::Value::AsInt(-7)),
        };
        let histogram = HistogramDataPoint {
            attributes: vec![attribute("histogram")],
            start_time_unix_nano: 3,
            time_unix_nano: 4,
            count: 3,
            sum: Some(6.0),
            bucket_counts: vec![1, 2],
            explicit_bounds: vec![3.5],
            exemplars: vec![exemplar_fixture()],
            flags: 1,
            min: Some(1.0),
            max: Some(4.0),
        };
        let exponential = ExponentialHistogramDataPoint {
            attributes: vec![attribute("exponential")],
            start_time_unix_nano: 5,
            time_unix_nano: 6,
            count: 4,
            sum: Some(9.0),
            scale: -2,
            zero_count: 1,
            positive: Some(exponential_histogram_data_point::Buckets {
                offset: -3,
                bucket_counts: vec![1, 0, 2],
            }),
            negative: Some(exponential_histogram_data_point::Buckets {
                offset: 1,
                bucket_counts: vec![1],
            }),
            flags: 1,
            exemplars: vec![exemplar_fixture()],
            min: Some(-2.0),
            max: Some(8.0),
            zero_threshold: 0.01,
        };
        let summary = SummaryDataPoint {
            attributes: vec![attribute("summary")],
            start_time_unix_nano: 7,
            time_unix_nano: 8,
            count: 2,
            sum: 3.0,
            quantile_values: vec![summary_data_point::ValueAtQuantile {
                quantile: 0.5,
                value: 1.5,
            }],
            flags: 1,
        };
        let metrics = vec![
            Metric {
                name: "gauge".to_owned(),
                description: "current".to_owned(),
                unit: "1".to_owned(),
                metadata: vec![attribute("metadata")],
                data: Some(metric::Data::Gauge(Gauge {
                    data_points: vec![number.clone()],
                })),
            },
            Metric {
                name: "sum".to_owned(),
                data: Some(metric::Data::Sum(Sum {
                    data_points: vec![number],
                    aggregation_temporality: 2,
                    is_monotonic: true,
                })),
                ..Metric::default()
            },
            Metric {
                name: "histogram".to_owned(),
                data: Some(metric::Data::Histogram(Histogram {
                    data_points: vec![histogram],
                    aggregation_temporality: 1,
                })),
                ..Metric::default()
            },
            Metric {
                name: "exponential".to_owned(),
                data: Some(metric::Data::ExponentialHistogram(ExponentialHistogram {
                    data_points: vec![exponential],
                    aggregation_temporality: 2,
                })),
                ..Metric::default()
            },
            Metric {
                name: "summary".to_owned(),
                data: Some(metric::Data::Summary(Summary {
                    data_points: vec![summary],
                })),
                ..Metric::default()
            },
        ];
        ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(Resource {
                    attributes: vec![attribute("resource")],
                    dropped_attributes_count: 2,
                    entity_refs: vec![EntityRef {
                        schema_url: "entity-schema".to_owned(),
                        r#type: "service".to_owned(),
                        id_keys: vec!["service.name".to_owned()],
                        description_keys: vec!["service.version".to_owned()],
                    }],
                }),
                scope_metrics: vec![ScopeMetrics {
                    scope: Some(InstrumentationScope {
                        name: "sdk".to_owned(),
                        version: "1.2.3".to_owned(),
                        attributes: vec![attribute("scope")],
                        dropped_attributes_count: 3,
                    }),
                    metrics,
                    schema_url: "scope-schema".to_owned(),
                }],
                schema_url: "resource-schema".to_owned(),
            }],
        }
    }

    /// Asserts that all generated variable-width storage has exact capacity.
    ///
    /// # Panics
    ///
    /// Panics when any decoded collection retains spare capacity.
    fn assert_exact_capacities(request: &ExportMetricsServiceRequest) {
        assert_eq!(
            request.resource_metrics.len(),
            request.resource_metrics.capacity()
        );
        for resource_group in &request.resource_metrics {
            assert_eq!(
                resource_group.schema_url.len(),
                resource_group.schema_url.capacity()
            );
            assert_eq!(
                resource_group.scope_metrics.len(),
                resource_group.scope_metrics.capacity()
            );
            if let Some(resource) = &resource_group.resource {
                assert_attributes_exact(&resource.attributes);
                assert_eq!(resource.attributes.len(), resource.attributes.capacity());
                assert_eq!(resource.entity_refs.len(), resource.entity_refs.capacity());
                for entity in &resource.entity_refs {
                    assert_eq!(entity.schema_url.len(), entity.schema_url.capacity());
                    assert_eq!(entity.r#type.len(), entity.r#type.capacity());
                    assert_eq!(entity.id_keys.len(), entity.id_keys.capacity());
                    assert_eq!(
                        entity.description_keys.len(),
                        entity.description_keys.capacity()
                    );
                    for value in entity.id_keys.iter().chain(&entity.description_keys) {
                        assert_eq!(value.len(), value.capacity());
                    }
                }
            }
            for scope_group in &resource_group.scope_metrics {
                assert_eq!(
                    scope_group.schema_url.len(),
                    scope_group.schema_url.capacity()
                );
                assert_eq!(scope_group.metrics.len(), scope_group.metrics.capacity());
                if let Some(scope) = &scope_group.scope {
                    assert_eq!(scope.name.len(), scope.name.capacity());
                    assert_eq!(scope.version.len(), scope.version.capacity());
                    assert_eq!(scope.attributes.len(), scope.attributes.capacity());
                    assert_attributes_exact(&scope.attributes);
                }
                for metric in &scope_group.metrics {
                    assert_eq!(metric.name.len(), metric.name.capacity());
                    assert_eq!(metric.description.len(), metric.description.capacity());
                    assert_eq!(metric.unit.len(), metric.unit.capacity());
                    assert_eq!(metric.metadata.len(), metric.metadata.capacity());
                    assert_attributes_exact(&metric.metadata);
                    assert_metric_data_exact(metric.data.as_ref());
                }
            }
        }
    }

    /// Asserts exact storage recursively for attributes and `AnyValue` trees.
    ///
    /// # Panics
    ///
    /// Panics when a key or nested collection retains spare capacity.
    fn assert_attributes_exact(attributes: &[KeyValue]) {
        for attribute in attributes {
            assert_eq!(attribute.key.len(), attribute.key.capacity());
            if let Some(value) = &attribute.value {
                assert_any_value_exact(value);
            }
        }
    }

    /// Asserts exact storage recursively for one `AnyValue`.
    ///
    /// # Panics
    ///
    /// Panics when any nested value retains spare capacity.
    fn assert_any_value_exact(value: &AnyValue) {
        match value.value.as_ref() {
            Some(any_value::Value::StringValue(value)) => assert_eq!(value.len(), value.capacity()),
            Some(any_value::Value::BytesValue(value)) => assert_eq!(value.len(), value.capacity()),
            Some(any_value::Value::ArrayValue(array)) => {
                assert_eq!(array.values.len(), array.values.capacity());
                for value in &array.values {
                    assert_any_value_exact(value);
                }
            }
            Some(any_value::Value::KvlistValue(list)) => {
                assert_eq!(list.values.len(), list.values.capacity());
                assert_attributes_exact(&list.values);
            }
            _ => {}
        }
    }

    /// Asserts exact point, primitive, exemplar, and quantile capacities.
    ///
    /// # Panics
    ///
    /// Panics when any aggregation storage retains spare capacity.
    fn assert_metric_data_exact(data: Option<&metric::Data>) {
        match data {
            Some(metric::Data::Gauge(value)) => {
                assert_eq!(value.data_points.len(), value.data_points.capacity());
                value.data_points.iter().for_each(assert_number_exact);
            }
            Some(metric::Data::Sum(value)) => {
                assert_eq!(value.data_points.len(), value.data_points.capacity());
                value.data_points.iter().for_each(assert_number_exact);
            }
            Some(metric::Data::Histogram(value)) => {
                assert_eq!(value.data_points.len(), value.data_points.capacity());
                for point in &value.data_points {
                    assert_point_common(
                        &point.attributes,
                        point.attributes.capacity(),
                        &point.exemplars,
                        point.exemplars.capacity(),
                    );
                    assert_eq!(point.bucket_counts.len(), point.bucket_counts.capacity());
                    assert_eq!(
                        point.explicit_bounds.len(),
                        point.explicit_bounds.capacity()
                    );
                }
            }
            Some(metric::Data::ExponentialHistogram(value)) => {
                assert_eq!(value.data_points.len(), value.data_points.capacity());
                for point in &value.data_points {
                    assert_point_common(
                        &point.attributes,
                        point.attributes.capacity(),
                        &point.exemplars,
                        point.exemplars.capacity(),
                    );
                    for buckets in [point.positive.as_ref(), point.negative.as_ref()]
                        .into_iter()
                        .flatten()
                    {
                        assert_eq!(
                            buckets.bucket_counts.len(),
                            buckets.bucket_counts.capacity()
                        );
                    }
                }
            }
            Some(metric::Data::Summary(value)) => {
                assert_eq!(value.data_points.len(), value.data_points.capacity());
                for point in &value.data_points {
                    assert_eq!(point.attributes.len(), point.attributes.capacity());
                    assert_attributes_exact(&point.attributes);
                    assert_eq!(
                        point.quantile_values.len(),
                        point.quantile_values.capacity()
                    );
                }
            }
            None => {}
        }
    }

    /// Asserts exact capacities below one number point.
    ///
    /// # Panics
    ///
    /// Panics when attributes or exemplars retain spare capacity.
    fn assert_number_exact(point: &NumberDataPoint) {
        assert_point_common(
            &point.attributes,
            point.attributes.capacity(),
            &point.exemplars,
            point.exemplars.capacity(),
        );
    }

    /// Asserts exact capacities common to points carrying exemplars.
    ///
    /// # Panics
    ///
    /// Panics when attributes, exemplars, IDs, or filtered attributes have spare capacity.
    fn assert_point_common(
        attributes: &[KeyValue],
        attribute_capacity: usize,
        exemplars: &[Exemplar],
        exemplar_capacity: usize,
    ) {
        assert_eq!(attributes.len(), attribute_capacity);
        assert_attributes_exact(attributes);
        assert_eq!(exemplars.len(), exemplar_capacity);
        for exemplar in exemplars {
            assert_eq!(
                exemplar.filtered_attributes.len(),
                exemplar.filtered_attributes.capacity()
            );
            assert_attributes_exact(&exemplar.filtered_attributes);
            assert_eq!(exemplar.span_id.len(), exemplar.span_id.capacity());
            assert_eq!(exemplar.trace_id.len(), exemplar.trace_id.capacity());
        }
    }

    /// Computes exact live allocation beneath one decoded metrics request.
    fn decoded_capacity(request: &ExportMetricsServiceRequest) -> usize {
        size_of::<ExportMetricsServiceRequest>()
            + request.resource_metrics.capacity() * size_of::<ResourceMetrics>()
            + request
                .resource_metrics
                .iter()
                .map(resource_group_capacity)
                .sum::<usize>()
    }

    /// Computes owned allocation beneath one resource group.
    fn resource_group_capacity(group: &ResourceMetrics) -> usize {
        group.schema_url.capacity()
            + group.scope_metrics.capacity() * size_of::<ScopeMetrics>()
            + group.resource.as_ref().map_or(0, resource_capacity)
            + group
                .scope_metrics
                .iter()
                .map(scope_group_capacity)
                .sum::<usize>()
    }

    /// Computes owned allocation beneath one resource.
    fn resource_capacity(resource: &Resource) -> usize {
        attributes_capacity(&resource.attributes, resource.attributes.capacity())
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

    /// Computes owned allocation beneath one scope group.
    fn scope_group_capacity(group: &ScopeMetrics) -> usize {
        group.schema_url.capacity()
            + group.metrics.capacity() * size_of::<Metric>()
            + group.scope.as_ref().map_or(0, |scope| {
                scope.name.capacity()
                    + scope.version.capacity()
                    + attributes_capacity(&scope.attributes, scope.attributes.capacity())
            })
            + group.metrics.iter().map(metric_capacity).sum::<usize>()
    }

    /// Computes owned allocation beneath one metric.
    fn metric_capacity(metric: &Metric) -> usize {
        metric.name.capacity()
            + metric.description.capacity()
            + metric.unit.capacity()
            + attributes_capacity(&metric.metadata, metric.metadata.capacity())
            + match metric.data.as_ref() {
                Some(metric::Data::Gauge(value)) => {
                    number_points_capacity(&value.data_points, value.data_points.capacity())
                }
                Some(metric::Data::Sum(value)) => {
                    number_points_capacity(&value.data_points, value.data_points.capacity())
                }
                Some(metric::Data::Histogram(value)) => {
                    value.data_points.capacity() * size_of::<HistogramDataPoint>()
                        + value
                            .data_points
                            .iter()
                            .map(|point| {
                                attributes_capacity(&point.attributes, point.attributes.capacity())
                                    + exemplars_capacity(
                                        &point.exemplars,
                                        point.exemplars.capacity(),
                                    )
                                    + point.bucket_counts.capacity() * size_of::<u64>()
                                    + point.explicit_bounds.capacity() * size_of::<f64>()
                            })
                            .sum::<usize>()
                }
                Some(metric::Data::ExponentialHistogram(value)) => {
                    value.data_points.capacity() * size_of::<ExponentialHistogramDataPoint>()
                        + value
                            .data_points
                            .iter()
                            .map(|point| {
                                attributes_capacity(&point.attributes, point.attributes.capacity())
                                    + exemplars_capacity(
                                        &point.exemplars,
                                        point.exemplars.capacity(),
                                    )
                                    + point.positive.as_ref().map_or(0, |b| {
                                        b.bucket_counts.capacity() * size_of::<u64>()
                                    })
                                    + point.negative.as_ref().map_or(0, |b| {
                                        b.bucket_counts.capacity() * size_of::<u64>()
                                    })
                            })
                            .sum::<usize>()
                }
                Some(metric::Data::Summary(value)) => {
                    value.data_points.capacity() * size_of::<SummaryDataPoint>()
                        + value
                            .data_points
                            .iter()
                            .map(|point| {
                                attributes_capacity(&point.attributes, point.attributes.capacity())
                                    + point.quantile_values.capacity()
                                        * size_of::<summary_data_point::ValueAtQuantile>()
                            })
                            .sum::<usize>()
                }
                None => 0,
            }
    }

    /// Computes allocation beneath a vector of number points.
    fn number_points_capacity(points: &[NumberDataPoint], capacity: usize) -> usize {
        capacity * size_of::<NumberDataPoint>()
            + points
                .iter()
                .map(|point| {
                    attributes_capacity(&point.attributes, point.attributes.capacity())
                        + exemplars_capacity(&point.exemplars, point.exemplars.capacity())
                })
                .sum::<usize>()
    }

    /// Computes allocation beneath a vector of exemplars.
    fn exemplars_capacity(exemplars: &[Exemplar], capacity: usize) -> usize {
        capacity * size_of::<Exemplar>()
            + exemplars
                .iter()
                .map(|value| {
                    attributes_capacity(
                        &value.filtered_attributes,
                        value.filtered_attributes.capacity(),
                    ) + value.span_id.capacity()
                        + value.trace_id.capacity()
                })
                .sum::<usize>()
    }

    /// Computes allocation beneath a vector of key/value entries.
    fn attributes_capacity(attributes: &[KeyValue], capacity: usize) -> usize {
        capacity * size_of::<KeyValue>()
            + attributes
                .iter()
                .map(|attribute| {
                    attribute.key.capacity()
                        + attribute.value.as_ref().map_or(0, any_value_capacity)
                })
                .sum::<usize>()
    }

    /// Computes recursive backing allocation beneath one `AnyValue`.
    fn any_value_capacity(value: &AnyValue) -> usize {
        match value.value.as_ref() {
            Some(any_value::Value::StringValue(value)) => value.capacity(),
            Some(any_value::Value::BytesValue(value)) => value.capacity(),
            Some(any_value::Value::ArrayValue(array)) => {
                array.values.capacity() * size_of::<AnyValue>()
                    + array.values.iter().map(any_value_capacity).sum::<usize>()
            }
            Some(any_value::Value::KvlistValue(list)) => {
                attributes_capacity(&list.values, list.values.capacity())
            }
            _ => 0,
        }
    }

    /// Appends one protobuf varint to a raw test buffer.
    fn push_varint(bytes: &mut Vec<u8>, mut value: u64) {
        loop {
            let mut byte = value.to_le_bytes()[0] & 0x7f;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            bytes.push(byte);
            if value == 0 {
                break;
            }
        }
    }

    /// Appends one length-delimited field to a raw test buffer.
    fn push_bytes_field(bytes: &mut Vec<u8>, tag: u32, value: &[u8]) {
        push_varint(bytes, u64::from(tag) << 3 | 2);
        push_varint(
            bytes,
            u64::try_from(value.len()).expect("test payload length fits u64"),
        );
        bytes.extend_from_slice(value);
    }

    /// Verifies round-trip parity, exact capacities, and exact admitted bytes.
    #[test]
    fn metrics_fixed_decode_covers_all_generated_shapes() {
        let request = metrics_fixture();
        let bytes = request.encode_to_vec();
        let plan = preflight_metrics_protobuf(&bytes, OTLP_WIRE_LIMITS).expect("preflight");
        let decoded = decode_metrics_protobuf(&bytes, plan).expect("decode");
        assert_eq!(decoded, request);
        assert_exact_capacities(&decoded);
        assert_eq!(plan.decode_bytes, decoded_capacity(&decoded));
    }

    /// Verifies duplicate singular strings allocate only their final retained value.
    #[test]
    fn metrics_fixed_decode_retains_only_final_singular_string() {
        let mut metric = Vec::new();
        push_bytes_field(&mut metric, 1, b"discarded-name");
        push_bytes_field(&mut metric, 1, b"kept");
        let mut scope = Vec::new();
        push_bytes_field(&mut scope, 2, &metric);
        let mut resource = Vec::new();
        push_bytes_field(&mut resource, 2, &scope);
        let mut bytes = Vec::new();
        push_bytes_field(&mut bytes, 1, &resource);

        let plan = preflight_metrics_protobuf(&bytes, OTLP_WIRE_LIMITS).expect("preflight");
        let decoded = decode_metrics_protobuf(&bytes, plan).expect("decode");
        assert_eq!(
            decoded.resource_metrics[0].scope_metrics[0].metrics[0].name,
            "kept"
        );
        assert_exact_capacities(&decoded);
        assert_eq!(plan.decode_bytes, decoded_capacity(&decoded));
    }

    /// Verifies the record ceiling rejects exactly the first excess point.
    #[test]
    fn metrics_preflight_rejects_record_cap_plus_one() {
        let request = metrics_fixture();
        let bytes = request.encode_to_vec();
        let limits = OtlpWireLimits {
            records: 4,
            ..OTLP_WIRE_LIMITS
        };
        assert!(preflight_metrics_protobuf(&bytes, limits).is_err());
    }

    /// Verifies alternating array/list recursion is rejected beyond the configured depth.
    #[test]
    fn metrics_preflight_rejects_value_depth_plus_one() {
        let mut value = AnyValue {
            value: Some(any_value::Value::StringValue("leaf".to_owned())),
        };
        for depth in 0..=OTLP_WIRE_LIMITS.value_depth {
            value = if depth % 2 == 0 {
                AnyValue {
                    value: Some(any_value::Value::ArrayValue(ArrayValue {
                        values: vec![value],
                    })),
                }
            } else {
                AnyValue {
                    value: Some(any_value::Value::KvlistValue(KeyValueList {
                        values: vec![KeyValue {
                            key: "k".to_owned(),
                            value: Some(value),
                        }],
                    })),
                }
            };
        }
        let mut request = metrics_fixture();
        request.resource_metrics[0]
            .resource
            .as_mut()
            .expect("resource")
            .attributes[0]
            .value = Some(value);
        assert!(preflight_metrics_protobuf(&request.encode_to_vec(), OTLP_WIRE_LIMITS).is_err());
    }

    /// Verifies deprecated unknown groups use a fixed bounded stack.
    #[test]
    fn metrics_preflight_rejects_unknown_group_depth_plus_one() {
        let mut bytes = Vec::new();
        for _ in 0..=MAX_WIRE_GROUP_DEPTH {
            push_varint(&mut bytes, (100_u64 << 3) | 3);
        }
        for _ in 0..=MAX_WIRE_GROUP_DEPTH {
            push_varint(&mut bytes, (100_u64 << 3) | 4);
        }
        assert!(preflight_metrics_protobuf(&bytes, OTLP_WIRE_LIMITS).is_err());
    }
}
