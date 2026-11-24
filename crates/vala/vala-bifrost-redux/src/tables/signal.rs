//! Narrow Variant projection, resource/scope, and Arrow-column helpers shared
//! by the three canonical `OTel` signal tables.
//!
//! Everything here is deliberately signal-agnostic. The per-signal meaning —
//! which field holds which protocol value, in which order, under which stable
//! id — stays in the owning table module. What lives here is only the small
//! machinery all three need identically: the projection of `OTLP` attribute
//! collections and values into Variant, the shared resource/scope envelope and
//! entity-reference layout every signal carries, the promoted semantic
//! conventions more than one signal reads, identifier validation, and the
//! typed accumulators that turn projected rows into Arrow arrays.

use arrow::array::{
    Array, ArrayRef, AsArray, BooleanArray, FixedSizeBinaryArray, Float64Array, Int32Array,
    Int64Array, ListArray, StringBuilder, StructArray,
};
use arrow::buffer::OffsetBuffer;
use arrow::datatypes::{DataType, Field, Fields, Schema};
use arrow::record_batch::RecordBatch;

use crate::tables::TableError;
use crate::tables::fields::{self, CanonicalField, CanonicalField as F, CanonicalType as T};
use parquet_variant::{
    BuilderSpecificState, ListBuilder, ObjectFieldBuilder, Variant, VariantBuilder,
    VariantBuilderExt,
};
use std::borrow::Cow;
use std::ops::Range;
use std::str::FromStr;
use std::sync::Arc;
use wyrd_queue::variant::{
    EncodedVariant, VariantColumnBuilder, VariantColumnEncoder, VariantViolation, narrow_integer,
};
use wyrd_spec::ids::CardUid;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::ids::{RunId, SpanId, TraceId};
use wyrd_spec::vala::managed_columns::{RUN_ID, SUBJECT_CARD_UID};
use wyrd_tonic::otlp::common::v1::any_value::Value;
use wyrd_tonic::otlp::common::v1::{
    AnyValue, ArrayValue, EntityRef, InstrumentationScope, KeyValue, KeyValueList,
};
use wyrd_tonic::otlp::resource::v1::Resource;

/// Element declaration of the key-name lists inside one entity reference.
pub static ENTITY_REF_KEY_ELEMENT: F = F::sensitive(iceberg::spec::LIST_FIELD_NAME, T::Utf8, false);

/// Ordered fields of one `OTel` resource entity reference.
pub static ENTITY_REF_FIELDS: [F; 4] = [
    F::sensitive("type", T::Utf8, false),
    F::sensitive("id_keys", T::List(&ENTITY_REF_KEY_ELEMENT), false),
    F::sensitive("description_keys", T::List(&ENTITY_REF_KEY_ELEMENT), false),
    F::sensitive("schema_url", T::Utf8, false),
];

/// Element declaration of every signal's `resource_entity_refs` collection.
///
/// All three signal ledgers declare their resource entity references through
/// this one element, so the persisted layout cannot drift between signals.
pub static ENTITY_REF_ELEMENT: F = F::sensitive(
    iceberg::spec::LIST_FIELD_NAME,
    T::Struct(&ENTITY_REF_FIELDS),
    false,
);

/// Positions `start..end` of one list in an [`AttributePool`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PoolRange {
    /// First position.
    pub start: usize,
    /// One past the last position.
    pub end: usize,
}

/// One `OTLP` value stored in an [`AttributePool`], borrowing its text and
/// bytes from the request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PooledValue<'a> {
    /// An absent or unset value.
    Empty,
    /// A string value.
    String(&'a str),
    /// A boolean value.
    Bool(bool),
    /// A signed integer value.
    Int(i64),
    /// A double value.
    Double(f64),
    /// A bytes value.
    Bytes(&'a [u8]),
    /// An array whose items are [`AttributePool::values`] in this range.
    Array(PoolRange),
    /// A key/value list whose entries are [`AttributePool::entries`] in this
    /// range.
    Kvlist(PoolRange),
}

/// One `OTLP` key/value entry stored in an [`AttributePool`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PooledEntry<'a> {
    /// The entry key.
    pub key: &'a str,
    /// The entry value.
    pub value: PooledValue<'a>,
}

/// Every attribute entry and array item of one borrowed `OTLP` request,
/// flattened into two vectors.
///
/// A decoder that borrows its request appends here instead of building the
/// generated message tree, so a whole request's attributes take two
/// allocations. Each list occupies a contiguous [`PoolRange`]; nested lists
/// are referenced by range.
#[derive(Debug, Default)]
pub struct AttributePool<'a> {
    /// Key/value entries of every attribute list and key/value list.
    pub entries: Vec<PooledEntry<'a>>,
    /// Items of every array value.
    pub values: Vec<PooledValue<'a>>,
}

impl<'a> AttributePool<'a> {
    /// The pooled attribute list at `range`.
    #[must_use]
    pub fn attributes(&self, range: PoolRange) -> Attributes<'_> {
        Attributes::Pooled(self, range)
    }

    /// Read one attribute list of an input this pool belongs to.
    #[must_use]
    pub fn list(&self, list: AttributeList<'a>) -> Attributes<'_> {
        match list {
            AttributeList::Generated(entries) => Attributes::Generated(entries),
            AttributeList::Pooled(range) => self.attributes(range),
        }
    }

    /// Read the pooled value at `index` of [`Self::values`].
    ///
    /// # Panics
    ///
    /// Panics when `index` is not below the length of [`Self::values`].
    #[must_use]
    pub fn value(&self, index: usize) -> ValueRef<'_> {
        ValueRef::pooled(self, self.values[index])
    }
}

/// One attribute list of a flattened `OTLP` input, by source.
#[derive(Debug, Clone, Copy)]
pub enum AttributeList<'a> {
    /// A generated message's entries.
    Generated(&'a [KeyValue]),
    /// Entries of the input's [`AttributePool`].
    Pooled(PoolRange),
}

/// One resource group of a flattened `OTLP` input.
///
/// The resource itself is decoded once per group, so a wire decoder owns it
/// and a generated input borrows it.
#[derive(Debug)]
pub struct ResourceInput<'a> {
    /// The group's resource, when present.
    pub resource: Option<Cow<'a, Resource>>,
    /// The group's schema URL.
    pub schema_url: &'a str,
    /// The group's scope groups, as positions in the input's scope list.
    pub scopes: Range<usize>,
}

/// One scope group of a flattened `OTLP` input.
#[derive(Debug)]
pub struct ScopeInput<'a> {
    /// The group's instrumentation scope, when present.
    pub scope: Option<Cow<'a, InstrumentationScope>>,
    /// The group's schema URL.
    pub schema_url: &'a str,
    /// The group's records (spans or log records), as positions in the
    /// input's record list.
    pub records: Range<usize>,
}

/// One `OTLP` attribute collection, borrowed from either a generated message
/// or an [`AttributePool`].
///
/// Every attribute rule in this module reads collections through this handle,
/// so a decoded request and a borrowed one are projected by one
/// implementation.
#[derive(Debug, Clone, Copy)]
pub enum Attributes<'a> {
    /// Entries of a generated message.
    Generated(&'a [KeyValue]),
    /// Entries of a borrowed request's pool.
    Pooled(&'a AttributePool<'a>, PoolRange),
}

/// One attribute entry read through [`Attributes`].
#[derive(Debug, Clone, Copy)]
pub struct Attribute<'a> {
    /// The entry key.
    pub key: &'a str,
    /// The entry value; [`ValueRef::Empty`] when absent or unset.
    pub value: ValueRef<'a>,
}

/// One `OTLP` value read through [`Attributes`] or [`Values`].
#[derive(Debug, Clone, Copy)]
pub enum ValueRef<'a> {
    /// An absent or unset value.
    Empty,
    /// A string value.
    String(&'a str),
    /// A boolean value.
    Bool(bool),
    /// A signed integer value.
    Int(i64),
    /// A double value.
    Double(f64),
    /// A bytes value.
    Bytes(&'a [u8]),
    /// An array value.
    Array(Values<'a>),
    /// A key/value list value.
    Kvlist(Attributes<'a>),
}

/// The items of one `OTLP` array value.
#[derive(Debug, Clone, Copy)]
pub enum Values<'a> {
    /// Items of a generated message.
    Generated(&'a [AnyValue]),
    /// Items of a borrowed request's pool.
    Pooled(&'a AttributePool<'a>, PoolRange),
}

impl<'a> Attributes<'a> {
    /// The number of entries, duplicates included.
    #[must_use]
    pub fn len(self) -> usize {
        match self {
            Self::Generated(entries) => entries.len(),
            Self::Pooled(_, range) => range.end - range.start,
        }
    }

    /// Whether the collection has no entries.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// The entry at `index`.
    ///
    /// # Panics
    ///
    /// Panics when `index` is not below [`Self::len`].
    #[must_use]
    pub fn get(self, index: usize) -> Attribute<'a> {
        match self {
            Self::Generated(entries) => {
                let entry = &entries[index];
                Attribute {
                    key: &entry.key,
                    value: entry.value.as_ref().map_or(ValueRef::Empty, ValueRef::of),
                }
            }
            Self::Pooled(pool, range) => {
                let entry = &pool.entries[range.start..range.end][index];
                Attribute {
                    key: entry.key,
                    value: ValueRef::pooled(pool, entry.value),
                }
            }
        }
    }

    /// The entries in request order.
    pub fn iter(self) -> impl DoubleEndedIterator<Item = Attribute<'a>> + ExactSizeIterator {
        (0..self.len()).map(move |index| self.get(index))
    }
}

impl<'a> From<&'a [KeyValue]> for Attributes<'a> {
    fn from(entries: &'a [KeyValue]) -> Self {
        Self::Generated(entries)
    }
}

impl<'a> From<&'a Vec<KeyValue>> for Attributes<'a> {
    fn from(entries: &'a Vec<KeyValue>) -> Self {
        Self::Generated(entries)
    }
}

impl<'a, const N: usize> From<&'a [KeyValue; N]> for Attributes<'a> {
    fn from(entries: &'a [KeyValue; N]) -> Self {
        Self::Generated(entries)
    }
}

impl<'a> Values<'a> {
    /// The number of items.
    #[must_use]
    pub fn len(self) -> usize {
        match self {
            Self::Generated(items) => items.len(),
            Self::Pooled(_, range) => range.end - range.start,
        }
    }

    /// Whether the array has no items.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// The items in request order.
    pub fn iter(self) -> impl Iterator<Item = ValueRef<'a>> {
        (0..self.len()).map(move |index| match self {
            Self::Generated(items) => ValueRef::of(&items[index]),
            Self::Pooled(pool, range) => ValueRef::pooled(pool, pool.values[range.start + index]),
        })
    }
}

impl<'a> ValueRef<'a> {
    /// Read one generated value.
    #[must_use]
    pub fn of(value: &'a AnyValue) -> Self {
        match &value.value {
            None => Self::Empty,
            Some(Value::StringValue(text)) => Self::String(text),
            Some(Value::BoolValue(flag)) => Self::Bool(*flag),
            Some(Value::IntValue(number)) => Self::Int(*number),
            Some(Value::DoubleValue(number)) => Self::Double(*number),
            Some(Value::BytesValue(bytes)) => Self::Bytes(bytes),
            Some(Value::ArrayValue(array)) => Self::Array(Values::Generated(&array.values)),
            Some(Value::KvlistValue(list)) => Self::Kvlist(Attributes::Generated(&list.values)),
        }
    }

    /// Read one pooled value of `pool`.
    fn pooled(pool: &'a AttributePool<'a>, value: PooledValue<'a>) -> Self {
        match value {
            PooledValue::Empty => Self::Empty,
            PooledValue::String(text) => Self::String(text),
            PooledValue::Bool(flag) => Self::Bool(flag),
            PooledValue::Int(number) => Self::Int(number),
            PooledValue::Double(number) => Self::Double(number),
            PooledValue::Bytes(bytes) => Self::Bytes(bytes),
            PooledValue::Array(range) => Self::Array(Values::Pooled(pool, range)),
            PooledValue::Kvlist(range) => Self::Kvlist(Attributes::Pooled(pool, range)),
        }
    }
}

/// One `OTLP` value or attribute collection that cannot be stored as Variant.
///
/// The failure keeps the logical field it was projected for; the rejected
/// record's ordinal is attached only when the rejection reason is rendered,
/// because a resource or scope value is shared by every record beneath it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantFailure {
    /// Logical column the value was projected into.
    field: &'static str,
    /// Why the encoded value cannot be stored.
    violation: VariantViolation,
}

impl VariantFailure {
    /// Render the partial-success rejection reason for one record.
    ///
    /// The reason starts with the catalogued Variant error code, followed by
    /// that error's message, so an `OTLP` client sees the same stable code a
    /// canonical Arrow writer would receive. `row` is the zero-based ordinal of
    /// the rejected record in request traversal order.
    #[must_use]
    pub fn reason(&self, row: u64) -> Cow<'static, str> {
        let error: BifrostError = self.violation.clone().into_error(self.field, row);
        Cow::Owned(format!("{}: {error}", error.code()))
    }
}

/// Project one `OTLP` attribute collection into a Variant object.
///
/// The collection is a logical map carried as repeated entries, so a repeated
/// key keeps only its final occurrence, matching the `OTel` data model and the
/// correlation rule. The result is then fully validated, so the Bifrost depth
/// and size limits apply exactly as they do to a canonical Arrow writer.
///
/// # Errors
///
/// Returns a [`VariantFailure`] for `field` when the encoded object exceeds
/// the Variant size limit or nests past the depth limit.
pub fn attributes_variant<'a>(
    field: &'static str,
    attributes: impl Into<Attributes<'a>>,
) -> Result<EncodedVariant, VariantFailure> {
    let mut builder = VariantBuilder::new();
    append_object(&mut builder, attributes.into(), &mut Vec::new());
    finish_variant(field, builder)
}

/// Encode one `OTLP` attribute collection as the next row of `column`.
///
/// The object is written straight into the column and checked there under
/// the rules [`attributes_variant`] applies, so it is never staged in its own
/// buffers. `scratch` is the request's duplicate-key stack, which it leaves
/// as it found it. Returns the row's encoded bytes.
///
/// # Errors
///
/// Returns the same [`VariantFailure`] [`attributes_variant`] returns for
/// `field`; the column is then unchanged.
pub fn append_attributes(
    column: &mut VariantColumnEncoder,
    field: &'static str,
    attributes: Attributes<'_>,
    scratch: &mut Vec<usize>,
) -> Result<usize, VariantFailure> {
    column
        .append(|row| append_object(row, attributes, scratch))
        .map_err(|violation| VariantFailure { field, violation })
}

/// Encode one `OTLP` value as the next row of `column`, typed as
/// [`any_value_variant`] types it.
///
/// The value is written straight into the column and checked there; `scratch`
/// is the request's duplicate-key stack, left as it was found. Returns the
/// row's encoded bytes.
///
/// # Errors
///
/// Returns the same [`VariantFailure`] [`any_value_variant`] returns for
/// `field`; the column is then unchanged.
pub fn append_value(
    column: &mut VariantColumnEncoder,
    field: &'static str,
    value: ValueRef<'_>,
    scratch: &mut Vec<usize>,
) -> Result<usize, VariantFailure> {
    column
        .append(|row| append_any_value(row, value, scratch))
        .map_err(|violation| VariantFailure { field, violation })
}

/// Project one `OTLP` value into a Variant of the same type.
///
/// A string, boolean, or double keeps its type, an integer is stored at its
/// narrowest Variant integer width, bytes become a Variant binary, an array
/// becomes a list, a key-value list becomes an object, and a present but unset
/// value becomes a Variant null.
///
/// # Errors
///
/// Returns a [`VariantFailure`] for `field` when the encoded value exceeds the
/// Variant size limit or nests past the depth limit.
pub fn any_value_variant(
    field: &'static str,
    value: &AnyValue,
) -> Result<EncodedVariant, VariantFailure> {
    let mut builder = VariantBuilder::new();
    append_any_value(&mut builder, ValueRef::of(value), &mut Vec::new());
    finish_variant(field, builder)
}

/// Finish one built Variant and validate it under the Bifrost limits.
///
/// [`EncodedVariant::from_builder`] applies the canonical Arrow writer's
/// limits without re-validating the builder's encoding, so the size limit is
/// applied first and the depth limit, which names the first container past it,
/// second. The protobuf decoder already bounds `OTLP` nesting, so building
/// before validating cannot recurse without bound.
///
/// # Errors
///
/// Returns the [`VariantFailure`] of the first violated limit.
fn finish_variant(
    field: &'static str,
    builder: VariantBuilder,
) -> Result<EncodedVariant, VariantFailure> {
    EncodedVariant::from_builder(builder).map_err(|violation| VariantFailure { field, violation })
}

/// Append one `OTLP` value at any Variant builder position.
///
/// `scratch` is the duplicate-key stack every nested object of one value
/// shares; see [`push_final_entries`].
fn append_any_value(
    builder: &mut impl VariantBuilderExt,
    value: ValueRef<'_>,
    scratch: &mut Vec<usize>,
) {
    match value {
        ValueRef::Empty => builder.append_value(Variant::Null),
        ValueRef::String(text) => builder.append_value(text),
        ValueRef::Bool(flag) => builder.append_value(flag),
        ValueRef::Int(number) => builder.append_value(narrow_integer(number)),
        ValueRef::Double(number) => builder.append_value(number),
        ValueRef::Bytes(bytes) => builder.append_value(Variant::Binary(bytes)),
        ValueRef::Array(items) => {
            let mut list = builder.new_list();
            append_items(&mut list, items, scratch);
            list.finish();
        }
        ValueRef::Kvlist(entries) => append_object(builder, entries, scratch),
    }
}

/// Append every `OTLP` array item to an open Variant list.
fn append_items<S: BuilderSpecificState>(
    list: &mut ListBuilder<'_, S>,
    items: Values<'_>,
    scratch: &mut Vec<usize>,
) {
    for item in items.iter() {
        append_any_value(list, item, scratch);
    }
}

/// Push the positions of `entries` that survive duplicate-key resolution onto
/// `scratch`, returning where they start.
///
/// The final occurrence of each key wins, and the survivors are left in
/// descending original position, the order [`append_object`] has always
/// written them in, so the Variant dictionary assigns the same field ids.
/// Positions are sorted by key and then position instead of hashed, so
/// attacker-chosen keys cost `O(n log n)` comparisons and no allocation
/// beyond the stack's growth. A nested object pushes its own frame above its
/// parent's and truncates back to it, so one stack serves a whole value.
fn push_final_entries(entries: Attributes<'_>, scratch: &mut Vec<usize>) -> usize {
    let key = |position: usize| entries.get(position).key;
    let start = scratch.len();
    scratch.extend(0..entries.len());
    let frame = &mut scratch[start..];
    frame.sort_unstable_by(|left, right| key(*left).cmp(key(*right)).then(left.cmp(right)));
    let mut kept = 0;
    for position in 0..frame.len() {
        let last_of_key = frame
            .get(position + 1)
            .is_none_or(|next| key(*next) != key(frame[position]));
        if last_of_key {
            frame[kept] = frame[position];
            kept += 1;
        }
    }
    frame[..kept].sort_unstable_by(|left, right| right.cmp(left));
    scratch.truncate(start + kept);
    start
}

/// Count the distinct keys in one `OTLP` attribute collection.
///
/// OTLP carries a logical map as repeated entries, so a duplicated key is one
/// key for cardinality purposes and resolves to its final value on read.
#[must_use]
pub fn distinct_keys<'a>(entries: impl Into<Attributes<'a>>) -> usize {
    let mut scratch = Vec::new();
    push_final_entries(entries.into(), &mut scratch);
    scratch.len()
}

/// Append one `OTLP` key-value collection as a Variant object.
///
/// Only the final occurrence of a repeated key is stored, and survivors are
/// written from the last to the first, as [`push_final_entries`] orders them.
/// Variant objects order their keys by name, so visiting order changes
/// nothing else. A present key whose value is unset is stored as a Variant
/// null, which stays distinct from an absent key.
fn append_object(
    builder: &mut impl VariantBuilderExt,
    entries: Attributes<'_>,
    scratch: &mut Vec<usize>,
) {
    let start = push_final_entries(entries, scratch);
    let end = scratch.len();
    let mut object = builder.new_object();
    for position in start..end {
        let entry = entries.get(scratch[position]);
        match entry.value {
            // A field builder would treat null as an absent key.
            ValueRef::Empty => object.insert(entry.key, Variant::Null),
            value => append_any_value(
                &mut ObjectFieldBuilder::new(entry.key, &mut object),
                value,
                scratch,
            ),
        }
    }
    object.finish();
    scratch.truncate(start);
}

/// Build one Variant column from encoded values; `None` is a null row.
///
/// The values are walked once to total their `metadata` and `value` bytes, so
/// the column's child buffers are allocated once at their final size.
#[must_use]
pub fn variant_column<'a>(
    values: impl ExactSizeIterator<Item = Option<&'a EncodedVariant>> + Clone,
) -> ArrayRef {
    let (metadata_bytes, value_bytes) =
        values
            .clone()
            .flatten()
            .fold((0_usize, 0_usize), |(metadata, value), encoded| {
                (
                    metadata.saturating_add(encoded.metadata().len()),
                    value.saturating_add(encoded.value().len()),
                )
            });
    let mut builder =
        VariantColumnBuilder::with_byte_capacity(values.len(), metadata_bytes, value_bytes);
    for value in values {
        builder.append_option(value);
    }
    builder.finish()
}

/// Build one `resource_entity_refs` list column from flattened references.
///
/// `refs` yields every row's references in row order and `lengths` holds one
/// entry per row, as for [`list_column`]; every reference keeps its type, both
/// ordered key lists, and its schema URL. The references are borrowed, so a
/// resource repeated on many rows is copied only into the column.
///
/// # Errors
///
/// Returns [`TableError::Internal`] when the assembled column does not match
/// [`ENTITY_REF_ELEMENT`], which would mean the ledger and this builder drift.
pub fn entity_refs_column<'a>(
    refs: impl Iterator<Item = &'a EntityRef> + Clone,
    lengths: &[Option<usize>],
) -> Result<ArrayRef, TableError> {
    let refs: Vec<&EntityRef> = refs.collect();
    let text = |select: fn(&EntityRef) -> &str| {
        utf8_opt_column_from(refs.iter().map(|entity| Some(select(entity))))
    };
    let key_element = ENTITY_REF_KEY_ELEMENT.to_arrow();
    let keys = |select: fn(&EntityRef) -> &[String]| {
        let lengths: Vec<Option<usize>> = refs
            .iter()
            .map(|entity| Some(select(entity).len()))
            .collect();
        let names: Vec<&str> = refs
            .iter()
            .flat_map(|entity| select(entity).iter().map(String::as_str))
            .collect();
        list_column(&key_element, utf8_column(&names), &lengths).map_err(internal)
    };
    let element = ENTITY_REF_ELEMENT.to_arrow();
    let entities = struct_column(
        &nested_fields(&element)?,
        vec![
            text(|entity| &entity.r#type),
            keys(|entity| &entity.id_keys)?,
            keys(|entity| &entity.description_keys)?,
            text(|entity| &entity.schema_url),
        ],
        None,
    )
    .map_err(internal)?;
    list_column(&element, entities, lengths).map_err(internal)
}

/// Returns the variable-length Arrow bytes one entity reference retains.
///
/// Counts its strings plus one 4-byte offset per key name, matching what
/// [`entity_refs_column`] materializes.
fn entity_ref_bytes(entity: &EntityRef) -> usize {
    entity.r#type.len()
        + entity.schema_url.len()
        + entity
            .id_keys
            .iter()
            .chain(&entity.description_keys)
            .map(|key| key.len() + size_of::<i32>())
            .sum::<usize>()
}

/// Read the final occurrence of `key` as text; any other value is `None`.
///
/// Promoted semantic-convention columns are null when their source is absent
/// or carries another protocol type, so this never rejects a record.
#[must_use]
pub fn promoted_string<'a>(attributes: impl Into<Attributes<'a>>, key: &str) -> Option<String> {
    last_string_attribute(attributes, key).map(str::to_owned)
}

/// Read the final occurrence of `key` as a signed integer; any other value is
/// `None`.
#[must_use]
pub fn promoted_int<'a>(attributes: impl Into<Attributes<'a>>, key: &str) -> Option<i64> {
    match last_attribute(attributes, key)?.value {
        ValueRef::Int(number) => Some(number),
        _ => None,
    }
}

/// The promoted `exception.*` semantic conventions of one attribute set.
///
/// Spans read them from their last event named `exception`; log records read
/// them from their own attributes. Each value is null when its source is
/// absent or is not a string.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExceptionPromotions<'a> {
    /// `exception.type`.
    pub exception_type: Option<&'a str>,
    /// `exception.message`.
    pub message: Option<&'a str>,
    /// `exception.stacktrace`.
    pub stacktrace: Option<&'a str>,
}

impl<'a> ExceptionPromotions<'a> {
    /// Read the three exception conventions from one attribute set, borrowing
    /// their text from it.
    #[must_use]
    pub fn from_attributes(attributes: impl Into<Attributes<'a>>) -> Self {
        let attributes = attributes.into();
        Self {
            exception_type: last_string_attribute(attributes, "exception.type"),
            message: last_string_attribute(attributes, "exception.message"),
            stacktrace: last_string_attribute(attributes, "exception.stacktrace"),
        }
    }

    /// Whether any of the three values is present, an empty string included.
    #[must_use]
    pub fn any(&self) -> bool {
        self.exception_type.is_some() || self.message.is_some() || self.stacktrace.is_some()
    }

    /// Returns the promoted text bytes these values add to one row.
    #[must_use]
    pub fn bytes(&self) -> usize {
        [self.exception_type, self.message, self.stacktrace]
            .into_iter()
            .flatten()
            .map(str::len)
            .sum()
    }
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
/// The promoted resource conventions are read once here and repeated on every
/// row, so every signal promotes them identically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceEnvelope {
    /// Whether the OTLP `Resource` message was present.
    pub present: bool,
    /// Resource attributes as one Variant object; empty when absent.
    pub attributes: EncodedVariant,
    /// Resource dropped-attribute count.
    pub dropped_attributes_count: u32,
    /// Resource schema URL from the enclosing wrapper.
    pub schema_url: String,
    /// Entity references in request order.
    pub entity_refs: Vec<EntityRef>,
    /// The typed conventions promoted out of the attributes.
    pub promotions: ResourcePromotions,
}

impl ResourceEnvelope {
    /// Project one optional OTLP resource plus its wrapper schema URL.
    ///
    /// # Errors
    ///
    /// Returns the [`VariantFailure`] of the resource attributes when they
    /// cannot be stored as Variant; every record beneath the resource is then
    /// rejected with that reason.
    pub fn project(resource: Option<&Resource>, schema_url: &str) -> Result<Self, VariantFailure> {
        let attributes = resource.map_or(&[][..], |value| value.attributes.as_slice());
        Ok(Self {
            present: resource.is_some(),
            attributes: attributes_variant("resource_attributes", attributes)?,
            dropped_attributes_count: resource.map_or(0, |value| value.dropped_attributes_count),
            schema_url: schema_url.to_owned(),
            entity_refs: resource.map_or_else(Vec::new, |value| value.entity_refs.clone()),
            promotions: ResourcePromotions::extract(attributes),
        })
    }

    /// Returns the variable-length payload bytes this envelope adds to each
    /// row that repeats it, including one 4-byte offset per entity reference.
    #[must_use]
    pub fn repeated_bytes(&self) -> usize {
        let promotions = &self.promotions;
        self.attributes.encoded_bytes()
            + self.schema_url.len()
            + [
                &promotions.service_name,
                &promotions.service_version,
                &promotions.deployment_environment,
                &promotions.service_namespace,
                &promotions.service_instance_id,
                &promotions.telemetry_sdk_name,
                &promotions.telemetry_sdk_language,
                &promotions.telemetry_sdk_version,
            ]
            .into_iter()
            .flatten()
            .map(String::len)
            .sum::<usize>()
            + self
                .entity_refs
                .iter()
                .map(|entity| entity_ref_bytes(entity) + size_of::<i32>())
                .sum::<usize>()
    }
}

/// The typed resource conventions every signal row promotes.
///
/// OTLP projection and native canonical validation both read them here, so a
/// native row's promoted columns are judged by the projection's own rule.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourcePromotions {
    /// Promoted `service.name`.
    pub service_name: Option<String>,
    /// Promoted `service.version`.
    pub service_version: Option<String>,
    /// Promoted `deployment.environment.name`, else `deployment.environment`.
    pub deployment_environment: Option<String>,
    /// Promoted `service.namespace`; descriptive, never a Card identity.
    pub service_namespace: Option<String>,
    /// Promoted `service.instance.id`.
    pub service_instance_id: Option<String>,
    /// Promoted `process.pid`, as a signed integer.
    pub process_pid: Option<i64>,
    /// Promoted `telemetry.sdk.name`.
    pub telemetry_sdk_name: Option<String>,
    /// Promoted `telemetry.sdk.language`.
    pub telemetry_sdk_language: Option<String>,
    /// Promoted `telemetry.sdk.version`.
    pub telemetry_sdk_version: Option<String>,
}

impl ResourcePromotions {
    /// Read every promoted convention from one resource attribute set.
    #[must_use]
    pub fn extract<'a>(attributes: impl Into<Attributes<'a>>) -> Self {
        let attributes = attributes.into();
        Self {
            service_name: promoted_string(attributes, "service.name"),
            service_version: promoted_string(attributes, "service.version"),
            deployment_environment: promoted_string(attributes, "deployment.environment.name")
                .or_else(|| promoted_string(attributes, "deployment.environment")),
            service_namespace: promoted_string(attributes, "service.namespace"),
            service_instance_id: promoted_string(attributes, "service.instance.id"),
            process_pid: promoted_int(attributes, "process.pid"),
            telemetry_sdk_name: promoted_string(attributes, "telemetry.sdk.name"),
            telemetry_sdk_language: promoted_string(attributes, "telemetry.sdk.language"),
            telemetry_sdk_version: promoted_string(attributes, "telemetry.sdk.version"),
        }
    }
}

/// Column-wise storage of the context columns every signal ledger declares
/// directly after `deployment_environment`.
///
/// Those are the six descriptive resource promotions followed by the three
/// subject descriptors. The projection never derives a subject descriptor
/// from telemetry, so those columns are null here; they belong to the
/// server's validated subject context.
#[derive(Debug, Default)]
pub struct ResourcePromotionColumns {
    /// Promoted `service.namespace`, one entry per accepted row.
    service_namespace: Vec<Option<String>>,
    /// Promoted `service.instance.id`, one entry per accepted row.
    service_instance_id: Vec<Option<String>>,
    /// Promoted `process.pid`, one entry per accepted row; its length is the
    /// row count [`Self::finish`] sizes the null subject columns by.
    process_pid: Vec<Option<i64>>,
    /// Promoted `telemetry.sdk.name`, one entry per accepted row.
    telemetry_sdk_name: Vec<Option<String>>,
    /// Promoted `telemetry.sdk.language`, one entry per accepted row.
    telemetry_sdk_language: Vec<Option<String>>,
    /// Promoted `telemetry.sdk.version`, one entry per accepted row.
    telemetry_sdk_version: Vec<Option<String>>,
}

impl ResourcePromotionColumns {
    /// Append one accepted row's resource promotions.
    pub fn push(&mut self, resource: &ResourcePromotions) {
        self.service_namespace
            .push(resource.service_namespace.clone());
        self.service_instance_id
            .push(resource.service_instance_id.clone());
        self.process_pid.push(resource.process_pid);
        self.telemetry_sdk_name
            .push(resource.telemetry_sdk_name.clone());
        self.telemetry_sdk_language
            .push(resource.telemetry_sdk_language.clone());
        self.telemetry_sdk_version
            .push(resource.telemetry_sdk_version.clone());
    }

    /// Build the nine columns in ledger order directly from each row's
    /// promotions, borrowing them rather than staging per-row copies.
    #[must_use]
    pub fn from_rows<'a>(
        rows: impl ExactSizeIterator<Item = &'a ResourcePromotions> + Clone,
    ) -> [ArrayRef; 9] {
        let text = |pick: fn(&ResourcePromotions) -> Option<&str>| {
            utf8_opt_column_from(rows.clone().map(pick))
        };
        let subject = || arrow::array::new_null_array(&DataType::Utf8, rows.len());
        [
            text(|row| row.service_namespace.as_deref()),
            text(|row| row.service_instance_id.as_deref()),
            i64_opt_column(rows.clone().map(|row| row.process_pid).collect()),
            text(|row| row.telemetry_sdk_name.as_deref()),
            text(|row| row.telemetry_sdk_language.as_deref()),
            text(|row| row.telemetry_sdk_version.as_deref()),
            subject(),
            subject(),
            subject(),
        ]
    }

    /// Build the nine columns in ledger order: the six promotions, then the
    /// null `subject_kind`, `subject_name`, and `subject_version`.
    #[must_use]
    pub fn finish(self) -> [ArrayRef; 9] {
        let rows = self.process_pid.len();
        let subject = || arrow::array::new_null_array(&DataType::Utf8, rows);
        [
            utf8_opt_column(&self.service_namespace),
            utf8_opt_column(&self.service_instance_id),
            i64_opt_column(self.process_pid),
            utf8_opt_column(&self.telemetry_sdk_name),
            utf8_opt_column(&self.telemetry_sdk_language),
            utf8_opt_column(&self.telemetry_sdk_version),
            subject(),
            subject(),
            subject(),
        ]
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
    /// Scope attributes as one Variant object; empty when absent.
    pub attributes: EncodedVariant,
    /// Scope dropped-attribute count.
    pub dropped_attributes_count: u32,
    /// Scope schema URL from the enclosing wrapper.
    pub schema_url: String,
}

impl ScopeEnvelope {
    /// Project one optional OTLP scope plus its wrapper schema URL.
    ///
    /// # Errors
    ///
    /// Returns the [`VariantFailure`] of the scope attributes when they cannot
    /// be stored as Variant; every record beneath the scope is then rejected
    /// with that reason.
    pub fn project(
        scope: Option<&InstrumentationScope>,
        schema_url: &str,
    ) -> Result<Self, VariantFailure> {
        Ok(Self {
            present: scope.is_some(),
            name: scope.map_or_else(String::new, |value| value.name.clone()),
            version: scope.map_or_else(String::new, |value| value.version.clone()),
            attributes: attributes_variant(
                "scope_attributes",
                scope.map_or(&[][..], |value| value.attributes.as_slice()),
            )?,
            dropped_attributes_count: scope.map_or(0, |value| value.dropped_attributes_count),
            schema_url: schema_url.to_owned(),
        })
    }

    /// Returns the variable-length payload bytes this envelope adds to each
    /// row that repeats it.
    #[must_use]
    pub fn repeated_bytes(&self) -> usize {
        self.name.len()
            + self.version.len()
            + self.attributes.encoded_bytes()
            + self.schema_url.len()
    }
}

/// The resource and scope envelopes one accepted row repeats, as positions
/// in [`RowEnvelopes`].
#[derive(Debug, Clone, Copy)]
pub struct Context {
    /// Position of the row's resource envelope.
    pub resource: usize,
    /// Position of the row's scope envelope.
    pub scope: usize,
}

/// Each resource and scope envelope of one projection, retained once, plus
/// the envelopes every accepted row repeats.
///
/// A signal projector retains each group's envelope when it reaches the
/// group and pushes one [`Context`] per accepted row, so no envelope is
/// copied before its final Arrow column.
#[derive(Debug, Default)]
pub struct RowEnvelopes {
    /// Every projected resource envelope, in request order.
    resources: Vec<ResourceEnvelope>,
    /// Every projected scope envelope, in request order.
    scopes: Vec<ScopeEnvelope>,
    /// Each accepted row's envelopes.
    rows: Vec<Context>,
}

impl RowEnvelopes {
    /// Start empty, with room for the envelopes of `rows` accepted rows and
    /// of `resources` and `scopes` groups, so a projector that knows its
    /// export's checked counts grows none of them.
    #[must_use]
    pub fn with_capacity(rows: usize, resources: usize, scopes: usize) -> Self {
        Self {
            resources: Vec::with_capacity(resources),
            scopes: Vec::with_capacity(scopes),
            rows: Vec::with_capacity(rows),
        }
    }

    /// Retain one projected resource envelope for the rows beneath it.
    ///
    /// Returns its position and the payload bytes it adds to each row.
    pub fn retain_resource(&mut self, envelope: ResourceEnvelope) -> (usize, usize) {
        let retained = (self.resources.len(), envelope.repeated_bytes());
        self.resources.push(envelope);
        retained
    }

    /// Retain one projected scope envelope for the rows beneath it.
    ///
    /// Returns its position and the payload bytes it adds to each row.
    pub fn retain_scope(&mut self, envelope: ScopeEnvelope) -> (usize, usize) {
        let retained = (self.scopes.len(), envelope.repeated_bytes());
        self.scopes.push(envelope);
        retained
    }

    /// Record the envelopes of one accepted row.
    pub fn push(&mut self, row: Context) {
        self.rows.push(row);
    }

    /// Each accepted row's resource envelope, in row order.
    pub fn resources(&self) -> impl ExactSizeIterator<Item = &ResourceEnvelope> + Clone {
        self.rows.iter().map(|row| &self.resources[row.resource])
    }

    /// Each accepted row's scope envelope, in row order.
    pub fn scopes(&self) -> impl ExactSizeIterator<Item = &ScopeEnvelope> + Clone {
        self.rows.iter().map(|row| &self.scopes[row.scope])
    }

    /// One text column read from each row's resource envelope.
    pub fn resource_text(&self, pick: fn(&ResourceEnvelope) -> Option<&str>) -> ArrayRef {
        utf8_opt_column_from(self.resources().map(pick))
    }

    /// The eleven columns every signal ledger declares from
    /// `resource_present` through `scope_schema_url`, in ledger order.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::Internal`] when the entity-reference column does
    /// not match its declared element.
    pub fn columns(&self) -> Result<[ArrayRef; 11], TableError> {
        let scope_text = |pick: fn(&ScopeEnvelope) -> &str| {
            utf8_opt_column_from(self.scopes().map(|row| Some(pick(row))))
        };
        let entity_ref_lengths: Vec<Option<usize>> = self
            .resources()
            .map(|row| Some(row.entity_refs.len()))
            .collect();
        Ok([
            bool_column(self.resources().map(|row| row.present).collect()),
            variant_column(self.resources().map(|row| Some(&row.attributes))),
            u32_as_i64_column(
                self.resources()
                    .map(|row| row.dropped_attributes_count)
                    .collect(),
            ),
            self.resource_text(|row| Some(&row.schema_url)),
            entity_refs_column(
                self.resources().flat_map(|row| &row.entity_refs),
                &entity_ref_lengths,
            )?,
            bool_column(self.scopes().map(|row| row.present).collect()),
            scope_text(|row| &row.name),
            scope_text(|row| &row.version),
            variant_column(self.scopes().map(|row| Some(&row.attributes))),
            u32_as_i64_column(
                self.scopes()
                    .map(|row| row.dropped_attributes_count)
                    .collect(),
            ),
            scope_text(|row| &row.schema_url),
        ])
    }
}

/// Find the last string value carried by `key`, matching map-overwrite order.
///
/// OTLP attribute collections are logically maps but are carried as repeated
/// entries, so a duplicate key resolves to the final occurrence. Returning
/// `None` for a present non-string value is deliberate: the caller decides
/// whether that is a null promotion or a record rejection.
#[must_use]
pub fn last_string_attribute<'a>(
    attributes: impl Into<Attributes<'a>>,
    key: &str,
) -> Option<&'a str> {
    match last_attribute(attributes, key)?.value {
        ValueRef::String(text) => Some(text),
        _ => None,
    }
}

/// Find the last attribute entry carried by `key`.
#[must_use]
pub fn last_attribute<'a>(
    attributes: impl Into<Attributes<'a>>,
    key: &str,
) -> Option<Attribute<'a>> {
    attributes
        .into()
        .iter()
        .rev()
        .find(|entry| entry.key == key)
}

/// Record-level `OTLP` attribute carrying optional Card correlation.
///
/// The value is the observed Card's UID. Gate and Scribe store it only once it
/// is confirmed to be carried by a member of the request's Card scope.
pub const CARD_UID_ATTRIBUTE: &str = "wyrd.card_uid";

/// Record-level `OTLP` attribute carrying optional run correlation.
pub const RUN_ID_ATTRIBUTE: &str = "wyrd.run_id";

/// Stable rejection reason for a wrongly typed or malformed `wyrd.card_uid`.
const CARD_UID_REJECTION: &str = "wyrd.card_uid is not a valid card correlation";

/// Stable rejection reason for a `wyrd.card_uid` the principal cannot assert.
const CARD_UID_UNAUTHORIZED: &str = "wyrd.card_uid is outside the principal's Card scope";

/// Stable rejection reason for a wrongly typed `wyrd.run_id`.
const RUN_ID_REJECTION: &str = "wyrd.run_id is not a valid run correlation";

/// Optional Card and run correlation read from one record's attributes.
///
/// This is the one place the three canonical signals share the tri-state rule
/// the specification fixes: an absent attribute projects null, a present final
/// occurrence with the declared string type and valid grammar projects its
/// text, and a present final occurrence of any other protocol type or invalid
/// text rejects only its own record. The stored attribute object keeps the
/// same final occurrence, so a correlation always equals its stored source.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RecordCorrelation {
    /// Authorized Card UID text, canonically formatted, when present.
    pub card_uid: Option<String>,
    /// Client-supplied run identifier text, when present and valid.
    pub run_id: Option<String>,
}

impl RecordCorrelation {
    /// Extract both optional correlations from one record's attributes.
    ///
    /// `card_scope` is the request's Card scope: a Card-bound principal's
    /// verified signed scope, or, for an unbound principal, the registered
    /// observation-target Cards Gate resolved for this export. It is checked
    /// here, per record, so one unauthorized UID rejects only its own OTLP
    /// record instead of failing the whole canonical batch later in Scribe.
    /// The check itself performs no registry, database, or cache lookup, and
    /// mirrors Scribe's own, so a record accepted here is not refused again
    /// when Scribe re-validates the assembled canonical batch.
    ///
    /// # Errors
    ///
    /// Returns the stable rejection reason when the final `wyrd.card_uid`
    /// occurrence is not a string or not a [`CardUid`], when `card_scope` does
    /// not carry that UID, or when the final `wyrd.run_id`
    /// occurrence is not a string. [`RunId`] adopts any string, so a run
    /// identifier has no further grammar to fail.
    pub fn extract<'a>(
        attributes: impl Into<Attributes<'a>>,
        card_scope: Option<&[CardUid]>,
    ) -> Result<Self, &'static str> {
        let attributes = attributes.into();
        let card_uid = correlation_text(attributes, CARD_UID_ATTRIBUTE, CARD_UID_REJECTION)?
            .map(|text| {
                let uid = CardUid::from_str(text).map_err(|_| CARD_UID_REJECTION)?;
                if !card_scope.is_some_and(|scope| scope.contains(&uid)) {
                    return Err(CARD_UID_UNAUTHORIZED);
                }
                Ok::<_, &'static str>(uid.to_string())
            })
            .transpose()?;
        let run_id = correlation_text(attributes, RUN_ID_ATTRIBUTE, RUN_ID_REJECTION)?;
        Ok(Self {
            card_uid,
            run_id: run_id
                .map(|text| RunId::from_string(text.to_owned()))
                .map(|run| run.as_str().to_owned()),
        })
    }
}

/// The final `wyrd.card_uid` text of one record, when it is a string.
///
/// Gate reads this to learn which Cards an unbound writer's export names
/// before projection; a wrongly typed occurrence yields nothing here and is
/// rejected by [`RecordCorrelation::extract`] on its own record.
#[must_use]
pub fn card_uid_text<'a>(attributes: impl Into<Attributes<'a>>) -> Option<&'a str> {
    correlation_text(attributes, CARD_UID_ATTRIBUTE, CARD_UID_REJECTION)
        .ok()
        .flatten()
}

/// Read the final occurrence of one correlation attribute as text.
///
/// # Errors
///
/// Returns `wrong_type` when the final occurrence carries any protocol value
/// other than a string.
fn correlation_text<'a>(
    attributes: impl Into<Attributes<'a>>,
    key: &str,
    wrong_type: &'static str,
) -> Result<Option<&'a str>, &'static str> {
    match last_attribute(attributes, key) {
        None => Ok(None),
        Some(entry) => match entry.value {
            ValueRef::String(text) => Ok(Some(text)),
            _ => Err(wrong_type),
        },
    }
}

/// The two nullable correlation fields every canonical signal projection appends.
///
/// They are not ledger fields: Scribe relinquishes
/// `card_uid` and `run_id` before stamping the canonical envelope, and the
/// source schema fingerprint excludes both, so appending them here cannot
/// perturb a table's catalog or canonical physical identity.
#[must_use]
pub fn correlation_fields() -> [Field; 2] {
    [
        Field::new(SUBJECT_CARD_UID, DataType::Utf8, true),
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

/// Build a non-nullable UTF-8 column from owned or borrowed text.
#[must_use]
pub fn utf8_column(values: &[impl AsRef<str>]) -> ArrayRef {
    utf8_opt_column_from(values.iter().map(|value| Some(value.as_ref())))
}

/// Build a nullable UTF-8 column from owned or borrowed text.
#[must_use]
pub fn utf8_opt_column(values: &[Option<impl AsRef<str>>]) -> ArrayRef {
    utf8_opt_column_from(values.iter().map(|value| value.as_ref().map(AsRef::as_ref)))
}

/// Build a UTF-8 column whose offsets and bytes are allocated once.
///
/// The values are walked once to total their bytes, then appended into a
/// builder of that exact capacity; a column without nulls carries no validity
/// buffer, as Arrow's own constructors produce. Projections that repeat one
/// envelope's text per row pass a borrowing iterator, so the text is copied
/// only into the column.
pub fn utf8_opt_column_from<'a>(
    values: impl ExactSizeIterator<Item = Option<&'a str>> + Clone,
) -> ArrayRef {
    let bytes = values
        .clone()
        .flatten()
        .fold(0_usize, |total, value| total.saturating_add(value.len()));
    let mut builder = StringBuilder::with_capacity(values.len(), bytes);
    for value in values {
        builder.append_option(value);
    }
    Arc::new(builder.finish())
}

/// Build a non-nullable fixed-width binary column of the declared width.
///
/// # Errors
///
/// Returns a stable reason when a value does not match the declared width.
pub fn fixed_binary_column(
    width: i32,
    values: &[impl AsRef<[u8]>],
) -> Result<ArrayRef, &'static str> {
    FixedSizeBinaryArray::try_from_sparse_iter_with_size(
        values.iter().map(|value| Some(value.as_ref())),
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
    values: &[Option<impl AsRef<[u8]>>],
) -> Result<ArrayRef, &'static str> {
    FixedSizeBinaryArray::try_from_sparse_iter_with_size(
        values.iter().map(|value| value.as_ref().map(AsRef::as_ref)),
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

/// The canonical type of every instant `OTLP` carries as Unix nanoseconds.
///
/// `TIMESTAMP_LTZ` at nanosecond precision in Iceberg's own `+00:00` spelling,
/// so a span start, log time, or point time is a time column the layout can
/// partition, sort, and prune on. Durations are not instants and stay `Int64`.
pub const OTLP_INSTANT: T = T::Timestamp(
    arrow::datatypes::TimeUnit::Nanosecond,
    Some(iceberg::arrow::UTC_TIME_ZONE),
);

/// Build a non-nullable [`OTLP_INSTANT`] column from Unix nanoseconds.
#[must_use]
pub fn otlp_instant_column(values: Vec<i64>) -> ArrayRef {
    Arc::new(
        arrow::array::TimestampNanosecondArray::from(values)
            .with_timezone(iceberg::arrow::UTC_TIME_ZONE),
    )
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
/// `element` is the declared element field, complete with its declared
/// metadata, so the produced column matches the ledger exactly.
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

/// Validate one supplied canonical user-column batch against a ledger.
///
/// Binding is by field *name*, never by position: a caller may present the
/// canonical columns in any order and still be accepted, and the returned batch
/// is that input projected back into declared ledger order so every downstream
/// authority sees one canonical column order. For each declared field the
/// supplied field must agree on Arrow type shape and nullability, recursively
/// through every nested child, and every Variant, top level or nested, must
/// carry the `arrow.parquet.variant` extension. Other field metadata is deliberately not
/// compared: the declared metadata is re-derived here when the validated
/// columns are reassembled under the declared schema, and any caller-supplied
/// field id or metadata entry is dropped, so a writer neither supplies nor can
/// be wrong about either.
///
/// Checks run in the locked write order: an undeclared column first, then
/// each declared field's presence, type, and nullability, then the Variant
/// contract through [`crate::tables::validate_declared_variants`], so every
/// catalogued refusal reaches the caller with its own code.
///
/// # Errors
///
/// Returns [`BifrostError::UndeclaredField`] for an undeclared column,
/// [`BifrostError::UnsupportedType`] for a declared field supplied in another
/// type, the catalogued Variant error for a Variant that cannot be stored, and
/// [`BifrostError::SchemaParse`] naming the column when a declared field is
/// missing or its nullability differs.
pub fn validate_canonical_user_batch(
    declared: &[CanonicalField],
    batch: &RecordBatch,
) -> Result<RecordBatch, BifrostError> {
    let schema = batch.schema();
    let fields = fields::canonical_arrow_fields(declared);
    crate::tables::refuse_undeclared(&fields, &schema)?;
    if schema.fields().len() != declared.len() {
        return Err(BifrostError::SchemaParse {
            detail: format!(
                "canonical batch declares {} columns, expected {}",
                schema.fields().len(),
                declared.len()
            ),
        });
    }

    let mut supplied = Vec::with_capacity(declared.len());
    for field in declared {
        let index = schema
            .index_of(field.name)
            .map_err(|_| BifrostError::SchemaParse {
                detail: format!("canonical batch is missing column {}", field.name),
            })?;
        validate_field_identity(field, schema.field(index))?;
        supplied.push(Arc::clone(batch.column(index)));
    }
    crate::tables::validate_declared_variants(&fields, batch)?;
    refuse_forged_resource_promotions(batch)?;
    let canonical = Schema::new(fields);

    let columns = canonical
        .fields()
        .iter()
        .zip(&supplied)
        .map(|(field, column)| {
            crate::tables::restamp_field_identity(column.as_ref(), field.data_type()).map_err(
                |error| BifrostError::SchemaParse {
                    detail: format!(
                        "canonical field {} does not carry its declared identity: {error}",
                        field.name()
                    ),
                },
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    RecordBatch::try_new(Arc::new(canonical), columns).map_err(|error| BifrostError::SchemaParse {
        detail: format!("canonical batch does not assemble: {error}"),
    })
}

/// Read row `row` of a stored Variant attribute column back as OTLP
/// key-values.
///
/// A native canonical batch re-derives its promoted columns from these with
/// the projection's own extractors. A promotion reads only the top-level type
/// of a value, so nested lists and objects come back empty and a Variant type
/// OTLP cannot carry comes back unset; neither satisfies a string, integer, or
/// boolean promotion, exactly as on the OTLP path. A stored object already
/// holds only the final occurrence of each key, and any other stored Variant
/// carries no keys, so it derives no promotion.
///
/// # Errors
///
/// Returns [`BifrostError::SchemaParse`] when the column is not Variant
/// storage or the row does not decode.
pub fn stored_attributes(column: &dyn Array, row: usize) -> Result<Vec<KeyValue>, BifrostError> {
    stored_variant(column, row, |variant| {
        variant.as_object().map_or_else(Vec::new, |object| {
            object
                .iter()
                .map(|(key, value)| KeyValue {
                    key: key.to_owned(),
                    value: stored_value(&value).map(|value| AnyValue { value: Some(value) }),
                })
                .collect()
        })
    })
}

/// Read row `row` of a nullable stored Variant column as text, when it holds a
/// string.
///
/// A null row or any other Variant type is `None`, the projection's rule for
/// a column promoted from a string value.
///
/// # Errors
///
/// Returns [`BifrostError::SchemaParse`] when the column is not Variant
/// storage or a present row does not decode.
pub fn stored_text(column: &dyn Array, row: usize) -> Result<Option<String>, BifrostError> {
    if column.is_null(row) {
        return Ok(None);
    }
    stored_variant(column, row, |variant| match stored_value(variant) {
        Some(Value::StringValue(text)) => Some(text),
        _ => None,
    })
}

/// Decode row `row` of a stored Variant column and map it with `read`.
///
/// # Errors
///
/// Returns [`BifrostError::SchemaParse`] when the column is not Variant
/// storage or the row does not decode.
pub(crate) fn stored_variant<T>(
    column: &dyn Array,
    row: usize,
    read: impl FnOnce(&Variant<'_, '_>) -> T,
) -> Result<T, BifrostError> {
    let malformed = || BifrostError::SchemaParse {
        detail: "a canonical Variant column does not decode".to_owned(),
    };
    let storage = column.as_struct_opt().ok_or_else(malformed)?;
    let child = |name: &str| {
        storage
            .column_by_name(name)
            .and_then(|bytes| bytes.as_binary_opt::<i32>())
            .map(|bytes| bytes.value(row))
    };
    let (Some(metadata), Some(value)) = (child("metadata"), child("value")) else {
        return Err(malformed());
    };
    Variant::try_new(metadata, value)
        .map(|variant| read(&variant))
        .map_err(|_| malformed())
}

/// The OTLP value of one stored top-level Variant, as [`stored_attributes`]
/// documents.
fn stored_value(value: &Variant<'_, '_>) -> Option<Value> {
    Some(match value {
        Variant::String(text) => Value::StringValue((*text).to_owned()),
        Variant::ShortString(text) => Value::StringValue(text.to_string()),
        Variant::BooleanTrue => Value::BoolValue(true),
        Variant::BooleanFalse => Value::BoolValue(false),
        Variant::Int8(number) => Value::IntValue(i64::from(*number)),
        Variant::Int16(number) => Value::IntValue(i64::from(*number)),
        Variant::Int32(number) => Value::IntValue(i64::from(*number)),
        Variant::Int64(number) => Value::IntValue(*number),
        Variant::Double(number) => Value::DoubleValue(*number),
        Variant::Binary(bytes) => Value::BytesValue(bytes.to_vec()),
        Variant::List(_) => Value::ArrayValue(ArrayValue::default()),
        Variant::Object(_) => Value::KvlistValue(KeyValueList::default()),
        _ => return None,
    })
}

/// Refuse a native canonical batch whose `column` differs from `derived`.
///
/// `derived` is what the table's projection computes from the row's source
/// fields; a native writer may compute it too, but a different value would be
/// a forged filter value and refuses the whole batch.
///
/// # Errors
///
/// Returns [`BifrostError::SchemaParse`] naming `column` when it is missing or
/// any row differs.
pub fn refuse_forged(
    batch: &RecordBatch,
    column: &str,
    derived: &ArrayRef,
) -> Result<(), BifrostError> {
    if batch
        .column_by_name(column)
        .is_some_and(|supplied| supplied.to_data() == derived.to_data())
    {
        return Ok(());
    }
    Err(BifrostError::SchemaParse {
        detail: format!("canonical field {column} differs from the value its source fields derive"),
    })
}

/// Refuse a native canonical signal batch whose resource promotions or
/// subject descriptors differ from what the projection derives.
///
/// Every signal promotes the same resource conventions from
/// `resource_attributes`, and the projection leaves the three subject
/// descriptors null for the server's validated subject context to fill.
///
/// # Errors
///
/// Returns the [`refuse_forged`] or [`stored_attributes`] refusal of the first
/// differing column.
fn refuse_forged_resource_promotions(batch: &RecordBatch) -> Result<(), BifrostError> {
    let column =
        batch
            .column_by_name("resource_attributes")
            .ok_or_else(|| BifrostError::SchemaParse {
                detail: "canonical batch is missing column resource_attributes".to_owned(),
            })?;
    let rows = (0..batch.num_rows())
        .map(|row| {
            stored_attributes(column.as_ref(), row).map(|set| ResourcePromotions::extract(&set))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let text = |pick: fn(&ResourcePromotions) -> &Option<String>| {
        utf8_opt_column(&rows.iter().map(|row| pick(row).clone()).collect::<Vec<_>>())
    };
    let null = arrow::array::new_null_array(&DataType::Utf8, rows.len());
    for (name, derived) in [
        ("service_name", text(|row| &row.service_name)),
        ("service_version", text(|row| &row.service_version)),
        (
            "deployment_environment",
            text(|row| &row.deployment_environment),
        ),
        ("service_namespace", text(|row| &row.service_namespace)),
        ("service_instance_id", text(|row| &row.service_instance_id)),
        (
            "process_pid",
            i64_opt_column(rows.iter().map(|row| row.process_pid).collect()),
        ),
        ("telemetry_sdk_name", text(|row| &row.telemetry_sdk_name)),
        (
            "telemetry_sdk_language",
            text(|row| &row.telemetry_sdk_language),
        ),
        (
            "telemetry_sdk_version",
            text(|row| &row.telemetry_sdk_version),
        ),
        ("subject_kind", Arc::clone(&null)),
        ("subject_name", Arc::clone(&null)),
        ("subject_version", null),
    ] {
        refuse_forged(batch, name, &derived)?;
    }
    Ok(())
}

/// Verify one supplied Arrow field's storage type and nullability.
///
/// The Variant extension is not compared here: the Variant contract walk that
/// follows owns it, top level and nested.
///
/// # Errors
///
/// Returns [`BifrostError::UnsupportedType`] when the storage type differs and
/// [`BifrostError::SchemaParse`] when the nullability differs.
fn validate_field_identity(
    declared: &CanonicalField,
    supplied: &Field,
) -> Result<(), BifrostError> {
    let expected = declared.to_arrow();
    if !supplied.data_type().equals_datatype(expected.data_type()) {
        return Err(BifrostError::UnsupportedType {
            field: declared.name.to_owned(),
            data_type: supplied.data_type().to_string(),
        });
    }
    if supplied.is_nullable() != declared.nullable {
        return Err(BifrostError::SchemaParse {
            detail: format!(
                "canonical field {} nullability is {}, expected {}",
                declared.name,
                supplied.is_nullable(),
                declared.nullable
            ),
        });
    }
    Ok(())
}

/// The one signed-scope fixture the three signal correlation tests share.
///
/// Each signal asserts the same four record shapes — missing, in-scope,
/// out-of-scope, and a Card reference instead of a UID — so they must agree on
/// exactly one scope. Keeping it beside the check it exercises means a change to the
/// authorization rule has one fixture to update rather than three.
#[cfg(test)]
pub(crate) mod correlation_fixture {
    use super::{CardUid, FromStr, KeyValue, RecordBatch, attributes_variant};
    use wyrd_queue::variant::{variant_bytes_to_json, variant_cell_to_json};

    /// The UID of a signed scope member, which makes it assertable.
    pub(crate) const IN_SCOPE: &str = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b22";

    /// A UID the principal's signed scope does not carry at all.
    pub(crate) const OUT_OF_SCOPE: &str = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b33";

    /// The signed member's Card reference text, which is not a UID.
    pub(crate) const REFERENCE_TEXT: &str = "Service/checkout@1.0.0";

    /// Build the shared scope: the one in-scope Card UID.
    pub(crate) fn scope() -> Vec<CardUid> {
        vec![CardUid::from_str(IN_SCOPE).expect("fixture card uid")]
    }

    /// Assert each leading `attributes` row holds its source attribute set.
    ///
    /// Correlation never rewrites the payload, so row `n` must decode to the
    /// Variant object `sources[n]` projects, the final duplicate winning.
    ///
    /// # Panics
    ///
    /// Panics when a row is missing, does not decode, or differs.
    pub(crate) fn assert_attribute_rows(batch: &RecordBatch, sources: &[&[KeyValue]]) {
        let attributes = batch
            .column_by_name("attributes")
            .expect("the batch has an attributes column");
        for (row, source) in sources.iter().enumerate() {
            let expected =
                attributes_variant("attributes", *source).expect("fixture attributes project");
            let expected = variant_bytes_to_json(expected.metadata(), expected.value())
                .expect("fixture attributes decode");
            assert_eq!(
                variant_cell_to_json(attributes.as_ref(), row).expect("row decodes"),
                expected,
                "row {row} retains every source attribute, the final duplicate winning"
            );
        }
    }
}

/// Running Arrow output charge for one projected OTLP request.
///
/// Each projector charges every accepted row's retained buffer bytes as it is
/// appended and stops the request once the running total crosses the
/// expanded-data limit, before any Arrow batch is built. The charge mirrors the
/// value, offset, and validity buffers `finish` materializes; Scribe re-checks
/// the assembled batch exactly before the WAL.
#[derive(Debug)]
pub(crate) struct OutputBudget {
    /// Bytes charged so far.
    used: usize,
    /// Expanded-data limit in bytes.
    limit: usize,
}

impl OutputBudget {
    /// Starts an empty budget bounded by `limit` bytes.
    pub(crate) const fn new(limit: usize) -> Self {
        Self { used: 0, limit }
    }

    /// Adds `bytes` of appended output to the running total.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::OutputTooLarge`] when the total exceeds the limit
    /// or overflows `usize`.
    pub(crate) fn charge(&mut self, bytes: usize) -> Result<(), TableError> {
        self.used = self.used.saturating_add(bytes);
        if self.used > self.limit {
            return Err(TableError::OutputTooLarge {
                bytes: self.used,
                limit: self.limit,
            });
        }
        Ok(())
    }
}

/// Returns the fixed-width Arrow bytes one row of `fields` retains.
///
/// Counts each value slot (fixed width, or the 4-byte offset of a UTF-8,
/// binary, or list value), recurses into struct children, and charges one byte
/// per nullable field for its validity bit. Variable-length payloads and list
/// elements are charged separately by the projector that appends them.
pub(crate) fn fixed_row_bytes(fields: &Fields) -> usize {
    fields
        .iter()
        .map(|field| {
            let value = match field.data_type() {
                DataType::Utf8 | DataType::Binary | DataType::List(_) => size_of::<i32>(),
                DataType::Struct(children) => fixed_row_bytes(children),
                DataType::FixedSizeBinary(width) => usize::try_from(*width).unwrap_or(0),
                DataType::Boolean => 1,
                other => other.primitive_width().unwrap_or(size_of::<i64>()),
            };
            value + usize::from(field.is_nullable())
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value as JsonValue, json};
    use wyrd_queue::variant::variant_bytes_to_json;
    use wyrd_spec::vala::api::VARIANT_MAX_DEPTH;
    use wyrd_tonic::otlp::common::v1::{ArrayValue, KeyValueList};

    /// A pooled list cannot expose an entry from the following list.
    ///
    /// # Panics
    ///
    /// Panics when the index reaches the list length, even with more pooled entries.
    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn pooled_attributes_get_panics_at_list_boundary() {
        let pool = AttributePool {
            entries: vec![
                PooledEntry {
                    key: "before",
                    value: PooledValue::Empty,
                },
                PooledEntry {
                    key: "inside",
                    value: PooledValue::Empty,
                },
                PooledEntry {
                    key: "after",
                    value: PooledValue::Empty,
                },
            ],
            values: vec![],
        };
        let attributes = pool.attributes(PoolRange { start: 1, end: 2 });
        assert_eq!(attributes.get(0).key, "inside");
        let _ = attributes.get(1);
    }

    /// Build one attribute entry with the supplied protocol value.
    fn attribute(key: &str, value: Value) -> KeyValue {
        KeyValue {
            key: key.to_owned(),
            value: Some(AnyValue { value: Some(value) }),
        }
    }

    /// Decode one encoded Variant back to JSON.
    ///
    /// # Panics
    ///
    /// Panics when the bytes are not a valid Variant.
    fn json_of(encoded: &EncodedVariant) -> JsonValue {
        variant_bytes_to_json(encoded.metadata(), encoded.value()).expect("valid Variant")
    }

    /// Every `OTLP` value form keeps its type, and duplicate keys keep the
    /// final occurrence.
    ///
    /// # Panics
    ///
    /// Panics when a value changes type, an integer narrows lossily, a present
    /// unset value is dropped, or an earlier duplicate wins.
    #[test]
    fn otlp_attributes_project_into_typed_variant_objects() {
        let encoded = attributes_variant(
            "attributes",
            &[
                attribute("service.name", Value::StringValue("first".to_owned())),
                attribute("retries", Value::IntValue(9_007_199_254_740_993)),
                attribute("ratio", Value::DoubleValue(0.5)),
                attribute("flag", Value::BoolValue(true)),
                attribute("raw", Value::BytesValue(vec![0x00, 0xff])),
                attribute(
                    "list",
                    Value::ArrayValue(ArrayValue {
                        values: vec![
                            AnyValue {
                                value: Some(Value::IntValue(1)),
                            },
                            AnyValue { value: None },
                        ],
                    }),
                ),
                attribute(
                    "nested",
                    Value::KvlistValue(KeyValueList {
                        values: vec![attribute("inner", Value::StringValue("v".to_owned()))],
                    }),
                ),
                KeyValue {
                    key: "unset".to_owned(),
                    value: None,
                },
                attribute("service.name", Value::StringValue("last".to_owned())),
            ],
        )
        .expect("attributes project");
        let variant = Variant::try_new(encoded.metadata(), encoded.value()).expect("valid");
        let object = variant.as_object().expect("an attribute set is an object");
        assert_eq!(
            object.get("retries"),
            Some(Variant::Int64(9_007_199_254_740_993))
        );
        assert_eq!(object.get("raw"), Some(Variant::Binary(&[0x00, 0xff])));
        assert_eq!(object.get("unset"), Some(Variant::Null));
        assert_eq!(object.get("absent"), None);
        assert_eq!(
            json_of(&encoded),
            json!({
                "service.name": "last", "retries": 9_007_199_254_740_993_i64, "ratio": 0.5,
                "flag": true, "raw": "AP8=", "list": [1, null], "nested": {"inner": "v"},
                "unset": null
            })
        );
        assert_eq!(
            json_of(&attributes_variant("attributes", &[]).expect("empty projects")),
            json!({})
        );
    }

    /// A value nested past the depth limit is refused with its catalogue code.
    ///
    /// # Panics
    ///
    /// Panics when the limit is off by one or the reason lacks the code.
    #[test]
    fn over_deep_otlp_values_are_refused_with_the_catalogued_code() {
        let mut value = AnyValue {
            value: Some(Value::IntValue(1)),
        };
        // The attribute object is the first container, so 63 nested arrays
        // reach exactly the limit.
        for _ in 1..VARIANT_MAX_DEPTH {
            value = AnyValue {
                value: Some(Value::ArrayValue(ArrayValue {
                    values: vec![value],
                })),
            };
        }
        let at_limit = vec![KeyValue {
            key: "deep".to_owned(),
            value: Some(value.clone()),
        }];
        attributes_variant("attributes", &at_limit).expect("exactly the limit projects");
        let past = vec![KeyValue {
            key: "deep".to_owned(),
            value: Some(AnyValue {
                value: Some(Value::ArrayValue(ArrayValue {
                    values: vec![value],
                })),
            }),
        }];
        let failure = attributes_variant("attributes", &past).expect_err("one past refuses");
        assert!(
            failure
                .reason(4)
                .starts_with("WYRD_VALA_400_VARIANT_TOO_DEEP"),
            "the reason leads with the catalogue code"
        );
    }

    /// Duplicate attribute keys resolve to the final occurrence.
    #[test]
    fn duplicate_attribute_keys_resolve_to_the_last_value() {
        let attributes = vec![
            attribute("service.name", Value::StringValue("first".to_owned())),
            attribute("service.name", Value::StringValue("last".to_owned())),
        ];
        assert_eq!(
            last_string_attribute(&attributes, "service.name"),
            Some("last")
        );
    }
}
