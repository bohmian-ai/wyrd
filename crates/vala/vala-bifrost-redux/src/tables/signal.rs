//! Narrow encode/decode, resource/scope, and Arrow-column helpers shared by the
//! three canonical OTel signal tables.
//!
//! Everything here is deliberately signal-agnostic. The per-signal meaning —
//! which field holds which protocol value, in which order, under which stable
//! id — stays in the owning table module. What lives here is only the small
//! machinery all three need identically: the pinned canonical protobuf
//! encoding for attribute collections and single values, the shared
//! resource/scope envelope every signal carries, identifier validation, and the
//! typed accumulators that turn projected rows into Arrow arrays.

use arrow::array::{
    Array, ArrayRef, BinaryArray, BinaryBuilder, BooleanArray, FixedSizeBinaryArray, Float64Array,
    Int32Array, Int64Array, ListArray, StringArray, StructArray, UInt32Array, UInt64Array,
};
use arrow::buffer::OffsetBuffer;
use arrow::datatypes::{Field, Fields};
use prost::Message;
use prost::encoding::{WireType, encode_key};
use std::sync::Arc;
use wyrd_spec::vala::ids::{SpanId, TraceId};
use wyrd_tonic::otlp::common::v1::{
    AnyValue, EntityRef, InstrumentationScope, KeyValue, KeyValueList,
};
use wyrd_tonic::otlp::resource::v1::Resource;

/// Protobuf field number of `KeyValueList.values`.
///
/// Attribute collections are stored as the canonical encoding of the
/// `KeyValueList` that wraps them, which is byte-identical to concatenating
/// each `KeyValue` under this tag.
const KEY_VALUE_LIST_VALUES_TAG: u32 = 1;

/// Encode one attribute collection as pinned canonical `KeyValueList` bytes.
///
/// The result is exactly `KeyValueList { values }.encode_to_vec()` without
/// cloning the borrowed entries. An empty collection encodes to zero bytes,
/// which is the present-but-empty form; absence is represented by a null Arrow
/// value, never by these bytes.
#[must_use]
pub fn encode_attributes(values: &[KeyValue]) -> Vec<u8> {
    let mut buffer = Vec::new();
    for value in values {
        encode_key(
            KEY_VALUE_LIST_VALUES_TAG,
            WireType::LengthDelimited,
            &mut buffer,
        );
        prost::encoding::encode_varint(value.encoded_len() as u64, &mut buffer);
        value.encode_raw(&mut buffer);
    }
    buffer
}

/// Encode one protobuf value as its pinned canonical `AnyValue` bytes.
///
/// A present but wholly unset `AnyValue` encodes to zero bytes; that is
/// distinct from absence, which the caller records as an Arrow null.
#[must_use]
pub fn encode_any_value(value: &AnyValue) -> Vec<u8> {
    value.encode_to_vec()
}

/// Encode one entity reference as its pinned canonical `EntityRef` bytes.
#[must_use]
pub fn encode_entity_ref(value: &EntityRef) -> Vec<u8> {
    value.encode_to_vec()
}

/// Verify supplied bytes are a canonical `KeyValueList` encoding.
///
/// Canonical means the bytes decode and re-encode to themselves. That rejects
/// non-minimal varints, out-of-order or repeated fields, and trailing unknown
/// fields, all of which would otherwise let two byte strings claim the same
/// logical attribute collection.
///
/// # Errors
///
/// Returns a stable reason when the bytes do not decode or do not round-trip.
pub fn verify_canonical_attributes(bytes: &[u8]) -> Result<(), &'static str> {
    let decoded = KeyValueList::decode(bytes).map_err(|_| "attributes are not a KeyValueList")?;
    (decoded.encode_to_vec() == bytes)
        .then_some(())
        .ok_or("attributes are not canonically encoded")
}

/// Verify supplied bytes are a canonical `AnyValue` encoding.
///
/// # Errors
///
/// Returns a stable reason when the bytes do not decode or do not round-trip.
pub fn verify_canonical_any_value(bytes: &[u8]) -> Result<(), &'static str> {
    let decoded = AnyValue::decode(bytes).map_err(|_| "value is not an AnyValue")?;
    (decoded.encode_to_vec() == bytes)
        .then_some(())
        .ok_or("value is not canonically encoded")
}

/// Verify supplied bytes are a canonical `EntityRef` encoding.
///
/// # Errors
///
/// Returns a stable reason when the bytes do not decode or do not round-trip.
pub fn verify_canonical_entity_ref(bytes: &[u8]) -> Result<(), &'static str> {
    let decoded = EntityRef::decode(bytes).map_err(|_| "entity ref is not an EntityRef")?;
    (decoded.encode_to_vec() == bytes)
        .then_some(())
        .ok_or("entity ref is not canonically encoded")
}

/// Validate one 16-byte OTLP trace identifier.
///
/// # Errors
///
/// Returns a stable reason when the value is not exactly sixteen bytes or is
/// the all-zero invalid identifier.
pub fn trace_id_bytes(value: &[u8]) -> Result<[u8; 16], &'static str> {
    let bytes: [u8; 16] = value.try_into().map_err(|_| "trace_id must be 16 bytes")?;
    TraceId::from_bytes(bytes).map_err(|_| "trace_id is not a valid trace identifier")?;
    Ok(bytes)
}

/// Validate one 8-byte OTLP span identifier.
///
/// # Errors
///
/// Returns a stable reason when the value is not exactly eight bytes or is the
/// all-zero invalid identifier.
pub fn span_id_bytes(value: &[u8]) -> Result<[u8; 8], &'static str> {
    let bytes: [u8; 8] = value.try_into().map_err(|_| "span_id must be 8 bytes")?;
    SpanId::from_bytes(bytes).map_err(|_| "span_id is not a valid span identifier")?;
    Ok(bytes)
}

/// The canonical resource envelope every signal row repeats.
///
/// `present` distinguishes an absent `Resource` message from a present but
/// empty one; the remaining scalars carry their canonical empty values when
/// absent and are interpreted only when `present` is true. `schema_url` comes
/// from the enclosing `Resource*` wrapper, not from the `Resource` message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceEnvelope {
    /// Whether the OTLP `Resource` message was present.
    pub present: bool,
    /// Canonical `KeyValueList` bytes for the resource attributes.
    pub attributes: Vec<u8>,
    /// Resource dropped-attribute count.
    pub dropped_attributes_count: u32,
    /// Resource schema URL from the enclosing wrapper.
    pub schema_url: String,
    /// Canonical `EntityRef` encodings in request order.
    pub entity_refs: Vec<Vec<u8>>,
}

impl ResourceEnvelope {
    /// Project one optional OTLP resource plus its wrapper schema URL.
    #[must_use]
    pub fn project(resource: Option<&Resource>, schema_url: &str) -> Self {
        match resource {
            None => Self {
                present: false,
                attributes: Vec::new(),
                dropped_attributes_count: 0,
                schema_url: schema_url.to_owned(),
                entity_refs: Vec::new(),
            },
            Some(resource) => Self {
                present: true,
                attributes: encode_attributes(&resource.attributes),
                dropped_attributes_count: resource.dropped_attributes_count,
                schema_url: schema_url.to_owned(),
                entity_refs: resource.entity_refs.iter().map(encode_entity_ref).collect(),
            },
        }
    }
}

/// The canonical instrumentation-scope envelope every signal row repeats.
///
/// `present` distinguishes an absent `InstrumentationScope` from a present but
/// empty one, exactly as [`ResourceEnvelope::present`] does for resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeEnvelope {
    /// Whether the OTLP `InstrumentationScope` message was present.
    pub present: bool,
    /// Scope name.
    pub name: String,
    /// Scope version.
    pub version: String,
    /// Canonical `KeyValueList` bytes for the scope attributes.
    pub attributes: Vec<u8>,
    /// Scope dropped-attribute count.
    pub dropped_attributes_count: u32,
    /// Scope schema URL from the enclosing wrapper.
    pub schema_url: String,
}

impl ScopeEnvelope {
    /// Project one optional OTLP scope plus its wrapper schema URL.
    #[must_use]
    pub fn project(scope: Option<&InstrumentationScope>, schema_url: &str) -> Self {
        match scope {
            None => Self {
                present: false,
                name: String::new(),
                version: String::new(),
                attributes: Vec::new(),
                dropped_attributes_count: 0,
                schema_url: schema_url.to_owned(),
            },
            Some(scope) => Self {
                present: true,
                name: scope.name.clone(),
                version: scope.version.clone(),
                attributes: encode_attributes(&scope.attributes),
                dropped_attributes_count: scope.dropped_attributes_count,
                schema_url: schema_url.to_owned(),
            },
        }
    }
}

/// Find the last string value carried by `key`, matching map-overwrite order.
///
/// OTLP attribute collections are logically maps but are carried as repeated
/// entries, so a duplicate key resolves to the final occurrence. Returning
/// `None` for a present non-string value is deliberate: the caller decides
/// whether that is a null promotion or a record rejection.
#[must_use]
pub fn last_string_attribute<'a>(attributes: &'a [KeyValue], key: &str) -> Option<&'a str> {
    last_attribute(attributes, key).and_then(|entry| {
        match entry.value.as_ref().and_then(|value| value.value.as_ref()) {
            Some(wyrd_tonic::otlp::common::v1::any_value::Value::StringValue(text)) => {
                Some(text.as_str())
            }
            _ => None,
        }
    })
}

/// Find the last attribute entry carried by `key`.
#[must_use]
pub fn last_attribute<'a>(attributes: &'a [KeyValue], key: &str) -> Option<&'a KeyValue> {
    attributes.iter().rev().find(|entry| entry.key == key)
}

/// Build a non-nullable UTF-8 column.
#[must_use]
pub fn utf8_column(values: Vec<String>) -> ArrayRef {
    Arc::new(StringArray::from(values))
}

/// Build a nullable UTF-8 column.
#[must_use]
pub fn utf8_opt_column(values: Vec<Option<String>>) -> ArrayRef {
    Arc::new(StringArray::from(values))
}

/// Build a non-nullable binary column.
#[must_use]
pub fn binary_column(values: &[Vec<u8>]) -> ArrayRef {
    let mut builder = BinaryBuilder::new();
    for value in values {
        builder.append_value(value);
    }
    Arc::new(builder.finish())
}

/// Build a nullable binary column.
#[must_use]
pub fn binary_opt_column(values: &[Option<Vec<u8>>]) -> ArrayRef {
    let mut builder = BinaryBuilder::new();
    for value in values {
        match value {
            Some(bytes) => builder.append_value(bytes),
            None => builder.append_null(),
        }
    }
    Arc::new(builder.finish())
}

/// Build a non-nullable fixed-width binary column of the declared width.
///
/// # Errors
///
/// Returns a stable reason when a value does not match the declared width.
pub fn fixed_binary_column(width: i32, values: &[Vec<u8>]) -> Result<ArrayRef, &'static str> {
    FixedSizeBinaryArray::try_from_sparse_iter_with_size(
        values.iter().map(|value| Some(value.as_slice())),
        width,
    )
    .map(|array| Arc::new(array) as ArrayRef)
    .map_err(|_| "fixed-width identifier has the wrong length")
}

/// Build a nullable fixed-width binary column of the declared width.
///
/// # Errors
///
/// Returns a stable reason when a present value does not match `width`.
pub fn fixed_binary_opt_column(
    width: i32,
    values: &[Option<Vec<u8>>],
) -> Result<ArrayRef, &'static str> {
    FixedSizeBinaryArray::try_from_sparse_iter_with_size(
        values.iter().map(|value| value.as_ref().map(Vec::as_slice)),
        width,
    )
    .map(|array| Arc::new(array) as ArrayRef)
    .map_err(|_| "fixed-width identifier has the wrong length")
}

/// Build a non-nullable boolean column.
#[must_use]
pub fn bool_column(values: Vec<bool>) -> ArrayRef {
    Arc::new(BooleanArray::from(values))
}

/// Build a nullable boolean column.
#[must_use]
pub fn bool_opt_column(values: Vec<Option<bool>>) -> ArrayRef {
    Arc::new(BooleanArray::from(values))
}

/// Build a non-nullable unsigned 32-bit column.
#[must_use]
pub fn u32_column(values: Vec<u32>) -> ArrayRef {
    Arc::new(UInt32Array::from(values))
}

/// Build a non-nullable unsigned 64-bit column.
#[must_use]
pub fn u64_column(values: Vec<u64>) -> ArrayRef {
    Arc::new(UInt64Array::from(values))
}

/// Build a nullable unsigned 64-bit column.
#[must_use]
pub fn u64_opt_column(values: Vec<Option<u64>>) -> ArrayRef {
    Arc::new(UInt64Array::from(values))
}

/// Build a non-nullable signed 32-bit column.
#[must_use]
pub fn i32_column(values: Vec<i32>) -> ArrayRef {
    Arc::new(Int32Array::from(values))
}

/// Build a nullable signed 32-bit column.
#[must_use]
pub fn i32_opt_column(values: Vec<Option<i32>>) -> ArrayRef {
    Arc::new(Int32Array::from(values))
}

/// Build a nullable signed 64-bit column.
#[must_use]
pub fn i64_opt_column(values: Vec<Option<i64>>) -> ArrayRef {
    Arc::new(Int64Array::from(values))
}

/// Build a nullable 64-bit IEEE column preserving every supplied bit pattern.
#[must_use]
pub fn f64_opt_column(values: Vec<Option<f64>>) -> ArrayRef {
    Arc::new(Float64Array::from(values))
}

/// Assemble one `List` column from flattened child values and row lengths.
///
/// `element` is the declared element field, complete with its stable id and
/// sensitivity metadata, so the produced column matches the ledger exactly.
/// `lengths` holds one entry per row; a null row is expressed by `None`.
///
/// # Errors
///
/// Returns a stable reason when the offsets overflow Arrow's `i32` offset plan
/// or the assembled child array does not match the declared element type.
pub fn list_column(
    element: &Field,
    values: ArrayRef,
    lengths: &[Option<usize>],
) -> Result<ArrayRef, &'static str> {
    let mut offsets: Vec<i32> = Vec::with_capacity(lengths.len() + 1);
    let mut cursor: i32 = 0;
    offsets.push(0);
    for length in lengths {
        let length = length.unwrap_or(0);
        cursor = i32::try_from(length)
            .ok()
            .and_then(|length| cursor.checked_add(length))
            .ok_or("repeated collection exceeds the Arrow offset plan")?;
        offsets.push(cursor);
    }
    let validity = lengths
        .iter()
        .any(Option::is_none)
        .then(|| lengths.iter().map(Option::is_some).collect());
    ListArray::try_new(
        Arc::new(element.clone()),
        OffsetBuffer::new(offsets.into()),
        values,
        validity,
    )
    .map(|array| Arc::new(array) as ArrayRef)
    .map_err(|_| "repeated collection does not match its declared element")
}

/// Assemble one `Struct` column from its declared children and built columns.
///
/// # Errors
///
/// Returns a stable reason when the column set does not match the declared
/// children.
pub fn struct_column(
    children: &Fields,
    columns: Vec<ArrayRef>,
    validity: Option<Vec<bool>>,
) -> Result<ArrayRef, &'static str> {
    StructArray::try_new(
        children.clone(),
        columns,
        validity.map(std::convert::Into::into),
    )
    .map(|array| Arc::new(array) as ArrayRef)
    .map_err(|_| "nested record does not match its declared fields")
}

/// Read one binary array value, rejecting an unexpected null.
///
/// # Errors
///
/// Returns a stable reason when the value is null.
pub fn required_binary(array: &BinaryArray, row: usize) -> Result<&[u8], &'static str> {
    array
        .is_valid(row)
        .then(|| array.value(row))
        .ok_or("required canonical payload is null")
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyrd_tonic::otlp::common::v1::any_value::Value;

    /// The borrow-free attribute encoding must equal the pinned message form.
    #[test]
    fn attribute_encoding_matches_the_pinned_key_value_list() {
        let attributes = vec![
            KeyValue {
                key: "service.name".to_owned(),
                value: Some(AnyValue {
                    value: Some(Value::StringValue("checkout".to_owned())),
                }),
            },
            KeyValue {
                key: "retries".to_owned(),
                value: Some(AnyValue {
                    value: Some(Value::IntValue(3)),
                }),
            },
        ];
        let expected = KeyValueList {
            values: attributes.clone(),
        }
        .encode_to_vec();
        assert_eq!(encode_attributes(&attributes), expected);
        assert!(verify_canonical_attributes(&expected).is_ok());
    }

    /// Empty is a present value with zero bytes, never a decode failure.
    #[test]
    fn empty_attribute_collections_encode_to_present_empty_bytes() {
        assert!(encode_attributes(&[]).is_empty());
        assert!(verify_canonical_attributes(&[]).is_ok());
        assert!(verify_canonical_any_value(&[]).is_ok());
    }

    /// Trailing garbage is not a canonical encoding even when it decodes.
    #[test]
    fn non_canonical_attribute_bytes_are_rejected() {
        assert!(verify_canonical_attributes(&[0xff, 0xff]).is_err());
    }

    /// Duplicate attribute keys resolve to the final occurrence.
    #[test]
    fn duplicate_attribute_keys_resolve_to_the_last_value() {
        let attributes = vec![
            KeyValue {
                key: "service.name".to_owned(),
                value: Some(AnyValue {
                    value: Some(Value::StringValue("first".to_owned())),
                }),
            },
            KeyValue {
                key: "service.name".to_owned(),
                value: Some(AnyValue {
                    value: Some(Value::StringValue("last".to_owned())),
                }),
            },
        ];
        assert_eq!(
            last_string_attribute(&attributes, "service.name"),
            Some("last")
        );
    }
}
