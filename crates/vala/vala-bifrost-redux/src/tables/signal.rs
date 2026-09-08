//! Narrow encode/decode, resource/scope, and Arrow-column helpers shared by the
//! three canonical `OTel` signal tables.
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
    Int32Array, Int64Array, ListArray, StringArray, StructArray,
};
use arrow::buffer::OffsetBuffer;
use arrow::datatypes::{DataType, Field, Fields, Schema};
use arrow::record_batch::RecordBatch;

use crate::tables::TableError;
use crate::tables::fields::{self, CanonicalField, CanonicalType};
use prost::Message;
use prost::encoding::{WireType, encode_key};
use std::str::FromStr;
use std::sync::Arc;
use wyrd_spec::reference::{CardRef, CardRefScope};
use wyrd_spec::vala::ids::{RunId, SpanId, TraceId};
use wyrd_spec::vala::managed_columns::{CARD_REF, RUN_ID};
use wyrd_tonic::otlp::common::v1::any_value::Value;
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

/// Record-level `OTLP` attribute carrying optional Card correlation.
///
/// The value uses the existing compact [`CardRef`] grammar. A supplied `#uid`
/// suffix parses as syntax but is untrusted: Scribe authorizes and resolves by
/// `(kind, space, name, version)` identity against the signed scope alone.
pub const CARD_REF_ATTRIBUTE: &str = "wyrd.card_ref";

/// Record-level `OTLP` attribute carrying optional run correlation.
pub const RUN_ID_ATTRIBUTE: &str = "wyrd.run_id";

/// Stable rejection reason for a wrongly typed or malformed `wyrd.card_ref`.
const CARD_REF_REJECTION: &str = "wyrd.card_ref is not a valid card correlation";

/// Stable rejection reason for a `wyrd.card_ref` the principal cannot assert.
const CARD_REF_UNAUTHORIZED: &str = "wyrd.card_ref is outside the signed Card scope";

/// Stable rejection reason for a wrongly typed `wyrd.run_id`.
const RUN_ID_REJECTION: &str = "wyrd.run_id is not a valid run correlation";

/// Optional Card and run correlation read from one record's attributes.
///
/// This is the one place the three canonical signals share the tri-state rule
/// the specification fixes: an absent attribute projects null, a present final
/// occurrence with the declared string type and valid grammar projects its
/// text, and a present final occurrence of any other protocol type or invalid
/// text rejects only its own record. Earlier duplicate keys are left entirely
/// to the lossless attribute payload — extraction never rewrites, reorders, or
/// removes a source entry.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RecordCorrelation {
    /// Client-supplied Card reference text, when present and valid.
    pub card_ref: Option<String>,
    /// Client-supplied run identifier text, when present and valid.
    pub run_id: Option<String>,
}

impl RecordCorrelation {
    /// Extract both optional correlations from one record's attributes.
    ///
    /// `card_scope` is the authenticated principal's verified signed Card
    /// scope, borrowed from the request that carried this record. It is checked
    /// here, per record, so one unauthorized reference rejects only its own
    /// OTLP record instead of failing the whole canonical batch later in
    /// Scribe. The check is decided entirely from the signed claims and
    /// performs no registry, database, or cache lookup.
    ///
    /// # Errors
    ///
    /// Returns the stable rejection reason when the final `wyrd.card_ref`
    /// occurrence is not a string or does not parse as a [`CardRef`], when a
    /// parsed reference is not authorized by `card_scope` or its matching
    /// signed member carries no UID, or when the final `wyrd.run_id`
    /// occurrence is not a string. [`RunId`] adopts any string, so a run
    /// identifier has no further grammar to fail.
    pub fn extract(
        attributes: &[KeyValue],
        card_scope: Option<&CardRefScope>,
    ) -> Result<Self, &'static str> {
        let card_ref = correlation_text(attributes, CARD_REF_ATTRIBUTE, CARD_REF_REJECTION)?;
        if let Some(text) = card_ref {
            let card = CardRef::from_str(text).map_err(|_| CARD_REF_REJECTION)?;
            authorize_card_ref(&card, card_scope)?;
        }
        let run_id = correlation_text(attributes, RUN_ID_ATTRIBUTE, RUN_ID_REJECTION)?;
        Ok(Self {
            card_ref: card_ref.map(str::to_owned),
            run_id: run_id
                .map(|text| RunId::from_string(text.to_owned()))
                .map(|run| run.as_str().to_owned()),
        })
    }
}

/// Confirm one parsed record reference lies within the signed Card scope.
///
/// This mirrors Scribe's own authorization exactly — identity match on
/// `(kind, space, name, version)` plus a UID the mint signed onto that same
/// member — so a record accepted here cannot be refused again when Scribe
/// re-validates the assembled canonical batch. Scribe keeps that whole-frame
/// check as defense in depth; this one exists only so a rejection stays
/// per-record.
///
/// # Errors
///
/// Returns [`CARD_REF_UNAUTHORIZED`] when the principal carries no signed
/// scope, the reference lies outside it, or the matching signed member carries
/// no UID.
fn authorize_card_ref(
    card: &CardRef,
    card_scope: Option<&CardRefScope>,
) -> Result<(), &'static str> {
    let scope = card_scope.ok_or(CARD_REF_UNAUTHORIZED)?;
    if !scope.authorizes(card) {
        return Err(CARD_REF_UNAUTHORIZED);
    }
    scope
        .as_slice()
        .iter()
        .find(|member| member.same_identity(card))
        .and_then(|member| member.uid.as_ref())
        .ok_or(CARD_REF_UNAUTHORIZED)?;
    Ok(())
}

/// Read the final occurrence of one correlation attribute as text.
///
/// # Errors
///
/// Returns `wrong_type` when the final occurrence carries any protocol value
/// other than a string.
fn correlation_text<'a>(
    attributes: &'a [KeyValue],
    key: &str,
    wrong_type: &'static str,
) -> Result<Option<&'a str>, &'static str> {
    match last_attribute(attributes, key) {
        None => Ok(None),
        Some(entry) => match entry.value.as_ref().and_then(|value| value.value.as_ref()) {
            Some(Value::StringValue(text)) => Ok(Some(text.as_str())),
            _ => Err(wrong_type),
        },
    }
}

/// The two nullable correlation fields every canonical signal projection appends.
///
/// They are not ledger fields and carry no stable id: Scribe strips `card_ref`
/// and relinquishes `run_id` before stamping the canonical envelope, and the
/// source schema fingerprint excludes both, so appending them here cannot
/// perturb a table's catalog or canonical physical identity.
#[must_use]
pub fn correlation_fields() -> [Field; 2] {
    [
        Field::new(CARD_REF, DataType::Utf8, true),
        Field::new(RUN_ID, DataType::Utf8, true),
    ]
}

/// Build the projected schema of one canonical signal: ledger then correlation.
#[must_use]
pub fn projected_signal_schema(declared: &[CanonicalField]) -> Arc<Schema> {
    let mut schema_fields = fields::canonical_arrow_fields(declared);
    schema_fields.extend(correlation_fields());
    Arc::new(Schema::new(schema_fields))
}

/// Return one projected signal batch without its appended correlation columns.
///
/// Every authority that compares a batch against its canonical ledger — schema
/// validation, physical identity, and storage round trips — reads this
/// projection, because the correlation columns belong to Scribe's stamping
/// contract rather than to the table's declared schema.
///
/// # Errors
///
/// Returns [`TableError::Internal`] when the remaining columns do not assemble.
pub fn without_correlation_columns(batch: &RecordBatch) -> Result<RecordBatch, TableError> {
    let appended = correlation_fields();
    let schema = batch.schema();
    let kept: Vec<usize> = schema
        .fields()
        .iter()
        .enumerate()
        .filter(|(_, field)| {
            !appended
                .iter()
                .any(|correlation| correlation.name() == field.name())
        })
        .map(|(index, _)| index)
        .collect();
    let retained = Schema::new(
        kept.iter()
            .map(|index| schema.field(*index).clone())
            .collect::<Vec<_>>(),
    );
    RecordBatch::try_new(
        Arc::new(retained),
        kept.iter()
            .map(|index| Arc::clone(batch.column(*index)))
            .collect(),
    )
    .map_err(|error| TableError::Internal(format!("ledger projection: {error}")))
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

/// Build a non-nullable signed 64-bit column from unsigned protocol counters.
///
/// Unsigned 32-bit protocol values widen losslessly, and signed 64 is the only
/// integer width the installed Iceberg conversion accepts on both legs of a
/// round trip, so the canonical ledger declares no unsigned column.
#[must_use]
pub fn u32_as_i64_column(values: Vec<u32>) -> ArrayRef {
    Arc::new(Int64Array::from(
        values.into_iter().map(i64::from).collect::<Vec<_>>(),
    ))
}

/// Build a non-nullable signed 64-bit column.
#[must_use]
pub fn i64_column(values: Vec<i64>) -> ArrayRef {
    Arc::new(Int64Array::from(values))
}

/// Widen one unsigned 64-bit protocol value into the declared signed column.
///
/// # Errors
///
/// Returns a stable reason when the value exceeds [`i64::MAX`], which no real
/// nanosecond timestamp or observation count reaches and which the durable
/// column cannot represent.
pub fn checked_i64(value: u64) -> Result<i64, &'static str> {
    i64::try_from(value).map_err(|_| "unsigned protocol value exceeds the durable column width")
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

/// Borrow the declared children of one nested struct element.
///
/// # Errors
///
/// Returns [`TableError::Internal`] when the declaration is not a struct, which
/// would mean the ledger and its projector disagree.
pub fn nested_fields(element: &Field) -> Result<Fields, TableError> {
    match element.data_type() {
        DataType::Struct(children) => Ok(children.clone()),
        other => Err(TableError::Internal(format!(
            "canonical element {} is {other}, expected a struct",
            element.name()
        ))),
    }
}

/// Wrap a stable assembly reason as an internal projection defect.
#[must_use]
pub fn internal(reason: &'static str) -> TableError {
    TableError::Internal(reason.to_owned())
}

/// Assemble one non-null `Float64` column.
#[must_use]
pub fn f64_column(values: Vec<f64>) -> ArrayRef {
    Arc::new(Float64Array::from(values))
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

/// Validate one supplied canonical user-column batch against a ledger.
///
/// Binding is by field *name*, never by position: a caller may present the
/// canonical columns in any order and still be accepted, and the returned batch
/// is that input projected back into declared ledger order so every downstream
/// authority sees one canonical column order. For each declared field the
/// supplied field must agree on stable id, Arrow type, nullability, and
/// sensitivity metadata, recursively through every nested child. Canonical
/// binary payloads are additionally decoded and re-encoded so a malformed or
/// non-canonical protobuf value is refused before any row is accepted.
///
/// # Errors
///
/// Returns a stable reason when a declared field is missing, an extra column is
/// present, an identity or type check fails, or a canonical payload value is
/// not canonically encoded.
pub fn validate_canonical_user_batch(
    declared: &[CanonicalField],
    batch: &RecordBatch,
) -> Result<RecordBatch, String> {
    let schema = batch.schema();
    if schema.fields().len() != declared.len() {
        return Err(format!(
            "canonical batch declares {} columns, expected {}",
            schema.fields().len(),
            declared.len()
        ));
    }

    let mut columns = Vec::with_capacity(declared.len());
    for field in declared {
        let index = schema
            .index_of(field.name)
            .map_err(|_| format!("canonical batch is missing column {}", field.name))?;
        let supplied = schema.field(index);
        validate_field_identity(field, supplied)?;
        let column = batch.column(index);
        validate_column_values(field, column.as_ref())?;
        columns.push(std::sync::Arc::clone(column));
    }

    let canonical = Schema::new(
        declared
            .iter()
            .map(CanonicalField::to_arrow)
            .collect::<Vec<_>>(),
    );
    RecordBatch::try_new(std::sync::Arc::new(canonical), columns)
        .map_err(|error| format!("canonical batch does not assemble: {error}"))
}

/// Verify one supplied Arrow field against its ledger declaration.
///
/// # Errors
///
/// Returns a stable reason naming the first disagreement in name, stable id,
/// type, nullability, sensitivity, or any nested child.
fn validate_field_identity(declared: &CanonicalField, supplied: &Field) -> Result<(), String> {
    if supplied.name() != declared.name {
        return Err(format!(
            "canonical field {} was supplied as {}",
            declared.name,
            supplied.name()
        ));
    }
    let expected = declared.to_arrow();
    if supplied.data_type() != expected.data_type() {
        return Err(format!(
            "canonical field {} has type {}, expected {}",
            declared.name,
            supplied.data_type(),
            expected.data_type()
        ));
    }
    if supplied.is_nullable() != declared.nullable {
        return Err(format!(
            "canonical field {} nullability is {}, expected {}",
            declared.name,
            supplied.is_nullable(),
            declared.nullable
        ));
    }
    for key in [fields::PARQUET_FIELD_ID, fields::WYRD_SENSITIVE] {
        if supplied.metadata().get(key) != expected.metadata().get(key) {
            return Err(format!(
                "canonical field {} carries the wrong {key}",
                declared.name
            ));
        }
    }
    Ok(())
}

/// Verify one supplied column's canonical payload values.
///
/// Only binary payloads carry an encoding contract Arrow cannot express, so
/// this walks into lists and structs to reach every `Binary` leaf and leaves
/// every other type to the identity check above.
///
/// # Errors
///
/// Returns a stable reason when a payload value is not canonically encoded.
fn validate_column_values(declared: &CanonicalField, column: &dyn Array) -> Result<(), String> {
    match declared.ty {
        CanonicalType::Binary => {
            let array = column
                .as_any()
                .downcast_ref::<BinaryArray>()
                .ok_or_else(|| format!("canonical field {} is not binary", declared.name))?;
            let verify = canonical_binary_verifier(declared.name);
            for row in 0..array.len() {
                if array.is_null(row) {
                    continue;
                }
                verify(array.value(row))
                    .map_err(|reason| format!("canonical field {}: {reason}", declared.name))?;
            }
            Ok(())
        }
        CanonicalType::List(element) => {
            let array = column
                .as_any()
                .downcast_ref::<ListArray>()
                .ok_or_else(|| format!("canonical field {} is not a list", declared.name))?;
            validate_column_values(element, array.values().as_ref())
        }
        CanonicalType::Struct(children) => {
            let array = column
                .as_any()
                .downcast_ref::<StructArray>()
                .ok_or_else(|| format!("canonical field {} is not a struct", declared.name))?;
            for (index, child) in children.iter().enumerate() {
                validate_column_values(child, array.column(index).as_ref())?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Select the canonical protobuf verifier for one declared binary field.
///
/// A log body and a metric or attribute payload are different pinned messages,
/// so the verifier is chosen by the declared field name rather than by type
/// alone. Any other binary field is an opaque canonical `EntityRef`.
fn canonical_binary_verifier(name: &str) -> fn(&[u8]) -> Result<(), &'static str> {
    match name {
        "body" => verify_canonical_any_value,
        "entity_ref" => verify_canonical_entity_ref,
        _ => verify_canonical_attributes,
    }
}

/// The one signed-scope fixture the three signal correlation tests share.
///
/// Each signal asserts the same four record shapes — missing, in-scope,
/// out-of-scope, and in-scope-but-UID-less — so they must agree on exactly one
/// scope. Keeping it beside the check it exercises means a change to the
/// authorization rule has one fixture to update rather than three.
#[cfg(test)]
pub(crate) mod correlation_fixture {
    use super::{CardRef, CardRefScope, FromStr};

    /// A signed scope member whose mint-resolved UID makes it assertable.
    pub(crate) const IN_SCOPE: &str = "prod/Service/checkout@1.0.0";

    /// A Card the principal's signed scope does not name at all.
    pub(crate) const OUT_OF_SCOPE: &str = "prod/Service/foreign@1.0.0";

    /// A signed scope member the mint could not resolve to a UID.
    pub(crate) const WITHOUT_UID: &str = "prod/Service/unresolved@1.0.0";

    /// Build the shared scope: a UID-bearing root plus one UID-less member.
    pub(crate) fn scope() -> CardRefScope {
        let root = CardRef {
            uid: Some(
                wyrd_spec::ids::CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b22")
                    .expect("fixture card uid"),
            ),
            ..CardRef::from_str(IN_SCOPE).expect("fixture card ref")
        };
        CardRefScope::from_root_and_members(
            &root,
            [CardRef::from_str(WITHOUT_UID).expect("fixture card ref")],
        )
    }
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
