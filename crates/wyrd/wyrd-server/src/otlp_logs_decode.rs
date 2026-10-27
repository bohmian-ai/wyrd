//! Fixed-capacity OTLP logs protobuf preflight and typed construction.

use std::mem::size_of;

use vala_bifrost_redux::gate::{IngestError, OtlpWireLimits};
use vala_bifrost_redux::tables::logs::log_row_output_bytes;
use wyrd_tonic::otlp::common::v1::{AnyValue, EntityRef, KeyValue};
use wyrd_tonic::otlp::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;

use crate::otlp_decode::{
    AnyValueBodies, GROUP_FAN_OUT, OtlpDecodePlan, ProjectedOutput, RESOURCE_FAN_OUT, WireFields,
    WireValue, decode_key_value, decode_merged_any_value, decode_merged_resource,
    decode_merged_scope, enforce_expanded_ceiling, fixed_bytes, fixed_string,
    last_bytes_across_messages, last_bytes_field, malformed, protobuf_i32, protobuf_u32,
    repeated_message_count, select_any_value_bodies, wrong_wire,
};

/// Bounded counters accumulated while walking one logs export.
#[derive(Debug)]
struct LogsWireFacts {
    /// Immutable limits shared by the adapter and Scribe admission.
    limits: OtlpWireLimits,
    /// Fixed Arrow bytes charged per projected log row.
    row_bytes: usize,
    /// Exact public layout plus retained backing capacity.
    decode_bytes: usize,
    /// Conservative projected canonical output.
    output: ProjectedOutput,
}

impl LogsWireFacts {
    /// Starts a preflight with the root generated request layout charged.
    fn new(limits: OtlpWireLimits) -> Self {
        Self {
            limits,
            row_bytes: log_row_output_bytes(),
            decode_bytes: size_of::<ExportLogsServiceRequest>(),
            output: ProjectedOutput::default(),
        }
    }

    /// Charges retained variable-width backing.
    ///
    /// # Errors
    ///
    /// Returns a malformed refusal on arithmetic overflow.
    fn add_value_bytes(&mut self, amount: usize) -> Result<(), IngestError> {
        self.add_decode_bytes(amount)
    }

    /// Charges public layouts or backing capacity to the decode owner.
    ///
    /// # Errors
    ///
    /// Returns a malformed refusal on arithmetic overflow.
    fn add_decode_bytes(&mut self, amount: usize) -> Result<(), IngestError> {
        self.decode_bytes = self
            .decode_bytes
            .checked_add(amount)
            .ok_or_else(|| malformed("OTLP protobuf capacity overflow"))?;
        Ok(())
    }

    /// Charges one retained key/value element before its nested payload.
    ///
    /// # Errors
    ///
    /// Returns a malformed refusal on arithmetic overflow.
    fn add_attribute(&mut self) -> Result<(), IngestError> {
        self.add_decode_bytes(size_of::<KeyValue>())
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
) -> Result<OtlpDecodePlan, IngestError> {
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
                facts
                    .output
                    .open(facts.decode_bytes, size_of::<ResourceLogs>());
                facts.add_decode_bytes(size_of::<ResourceLogs>())?;
                visit_resource_logs(resource, &mut facts)?;
                facts
                    .output
                    .close_group(facts.decode_bytes, RESOURCE_FAN_OUT);
            }
            (1, _) => return Err(wrong_wire("logs resource group")),
            _ => {}
        }
    }
    enforce_expanded_ceiling(facts.decode_bytes, facts.output.bytes(), limits)?;
    Ok(OtlpDecodePlan {
        wire_bytes: bytes.len(),
        decode_bytes: facts.decode_bytes,
        projected_bytes: facts.output.bytes(),
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
            (1 | 3, WireValue::Bytes(_)) => {}
            (2, WireValue::Bytes(scope)) => {
                facts
                    .output
                    .open(facts.decode_bytes, size_of::<ScopeLogs>());
                facts.add_decode_bytes(size_of::<ScopeLogs>())?;
                visit_scope_logs(scope, facts)?;
                facts.output.close_group(facts.decode_bytes, GROUP_FAN_OUT);
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
                facts
                    .output
                    .open(facts.decode_bytes, size_of::<LogRecord>());
                facts.add_decode_bytes(size_of::<LogRecord>())?;
                visit_log_record(record, facts)?;
                facts
                    .output
                    .close_record(facts.decode_bytes, facts.row_bytes);
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
            (3, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
            (1 | 2, WireValue::Bytes(_)) | (4, WireValue::Varint(_)) => {}
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
            (6, WireValue::Bytes(attribute)) => visit_attribute(attribute, 0, facts)?,
            (1 | 11, WireValue::Fixed64(_))
            | (2 | 7, WireValue::Varint(_))
            | (3 | 5 | 9 | 10 | 12, WireValue::Bytes(_))
            | (8, WireValue::Fixed32(_)) => {}
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
            (1 | 3, WireValue::Bytes(_)) => {}
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
            (1 | 3, WireValue::Bytes(_)) => {}
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
                record.attributes.push(decode_key_value(attribute)?);
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

#[cfg(test)]
mod tests {
    use super::*;
    use wyrd_tonic::otlp::common::v1::{ArrayValue, InstrumentationScope, KeyValueList, any_value};
    use wyrd_tonic::otlp::resource::v1::Resource;
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

    /// Proves recursive value depth fails at the fixed depth plus one.
    ///
    /// # Panics
    ///
    /// Panics if test fixtures cannot encode.
    #[test]
    fn logs_preflight_enforces_depth_limit() {
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
