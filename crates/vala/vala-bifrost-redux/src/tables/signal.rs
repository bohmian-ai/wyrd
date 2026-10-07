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
    ArrayRef, BooleanArray, FixedSizeBinaryArray, Float64Array, Int32Array, Int64Array, ListArray,
    StringArray, StructArray,
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
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;
use wyrd_queue::variant::{EncodedVariant, VariantColumnBuilder, VariantViolation, narrow_integer};
use wyrd_spec::reference::{CardRef, CardRefScope};
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::ids::{RunId, SpanId, TraceId};
use wyrd_spec::vala::managed_columns::{CARD_REF, RUN_ID};
use wyrd_tonic::otlp::common::v1::any_value::Value;
use wyrd_tonic::otlp::common::v1::{AnyValue, EntityRef, InstrumentationScope, KeyValue};
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
pub fn attributes_variant(
    field: &'static str,
    attributes: &[KeyValue],
) -> Result<EncodedVariant, VariantFailure> {
    let mut builder = VariantBuilder::new();
    append_object(&mut builder, attributes);
    finish_variant(field, builder)
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
    append_any_value(&mut builder, value.value.as_ref());
    finish_variant(field, builder)
}

/// Finish one built Variant and validate it under the Bifrost limits.
///
/// Validation reuses the canonical Arrow writer's check, so the size limit is
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
    let (metadata, value) = builder.finish();
    EncodedVariant::from_bytes(&metadata, &value)
        .map_err(|violation| VariantFailure { field, violation })
}

/// Append one `OTLP` value at any Variant builder position.
fn append_any_value(builder: &mut impl VariantBuilderExt, value: Option<&Value>) {
    match value {
        None => builder.append_value(Variant::Null),
        Some(Value::StringValue(text)) => builder.append_value(text.as_str()),
        Some(Value::BoolValue(flag)) => builder.append_value(*flag),
        Some(Value::IntValue(number)) => builder.append_value(narrow_integer(*number)),
        Some(Value::DoubleValue(number)) => builder.append_value(*number),
        Some(Value::BytesValue(bytes)) => builder.append_value(Variant::Binary(bytes)),
        Some(Value::ArrayValue(array)) => {
            let mut list = builder.new_list();
            append_items(&mut list, &array.values);
            list.finish();
        }
        Some(Value::KvlistValue(list)) => append_object(builder, &list.values),
    }
}

/// Append every `OTLP` array item to an open Variant list.
fn append_items<S: BuilderSpecificState>(list: &mut ListBuilder<'_, S>, items: &[AnyValue]) {
    for item in items {
        append_any_value(list, item.value.as_ref());
    }
}

/// Append one `OTLP` key-value collection as a Variant object.
///
/// Entries are visited from the last to the first and a key already written
/// is skipped, so the final occurrence of a repeated key is the one stored.
/// Variant objects order their keys by name, so visiting order changes
/// nothing else. A present key whose value is unset is stored as a Variant
/// null, which stays distinct from an absent key.
fn append_object(builder: &mut impl VariantBuilderExt, entries: &[KeyValue]) {
    let mut object = builder.new_object();
    let mut written: HashSet<&str> = HashSet::with_capacity(entries.len());
    for entry in entries.iter().rev() {
        if !written.insert(entry.key.as_str()) {
            continue;
        }
        match entry.value.as_ref().and_then(|value| value.value.as_ref()) {
            // A field builder would treat null as an absent key.
            None => object.insert(&entry.key, Variant::Null),
            Some(value) => append_any_value(
                &mut ObjectFieldBuilder::new(&entry.key, &mut object),
                Some(value),
            ),
        }
    }
    object.finish();
}

/// Build one Variant column from encoded values; `None` is a null row.
#[must_use]
pub fn variant_column<'a>(
    values: impl ExactSizeIterator<Item = Option<&'a EncodedVariant>>,
) -> ArrayRef {
    values.collect::<VariantColumnBuilder>().finish()
}

/// Build one `resource_entity_refs` list column from flattened references.
///
/// `lengths` holds one entry per row, as for [`list_column`]; every reference
/// keeps its type, both ordered key lists, and its schema URL.
///
/// # Errors
///
/// Returns [`TableError::Internal`] when the assembled column does not match
/// [`ENTITY_REF_ELEMENT`], which would mean the ledger and this builder drift.
pub fn entity_refs_column(
    refs: &[EntityRef],
    lengths: &[Option<usize>],
) -> Result<ArrayRef, TableError> {
    let key_element = ENTITY_REF_KEY_ELEMENT.to_arrow();
    let keys = |select: fn(&EntityRef) -> &[String]| {
        let values: Vec<String> = refs
            .iter()
            .flat_map(|entity| select(entity).to_vec())
            .collect();
        let lengths: Vec<Option<usize>> = refs
            .iter()
            .map(|entity| Some(select(entity).len()))
            .collect();
        list_column(&key_element, utf8_column(values), &lengths).map_err(internal)
    };
    let element = ENTITY_REF_ELEMENT.to_arrow();
    let entities = struct_column(
        &nested_fields(&element)?,
        vec![
            utf8_column(refs.iter().map(|entity| entity.r#type.clone()).collect()),
            keys(|entity| &entity.id_keys)?,
            keys(|entity| &entity.description_keys)?,
            utf8_column(
                refs.iter()
                    .map(|entity| entity.schema_url.clone())
                    .collect(),
            ),
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
pub fn promoted_string(attributes: &[KeyValue], key: &str) -> Option<String> {
    last_string_attribute(attributes, key).map(str::to_owned)
}

/// Read the final occurrence of `key` as a signed integer; any other value is
/// `None`.
#[must_use]
pub fn promoted_int(attributes: &[KeyValue], key: &str) -> Option<i64> {
    match last_attribute(attributes, key)?
        .value
        .as_ref()
        .and_then(|value| value.value.as_ref())
    {
        Some(Value::IntValue(number)) => Some(*number),
        _ => None,
    }
}

/// The promoted `exception.*` semantic conventions of one attribute set.
///
/// Spans read them from their last event named `exception`; log records read
/// them from their own attributes. Each value is null when its source is
/// absent or is not a string.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExceptionPromotions {
    /// `exception.type`.
    pub exception_type: Option<String>,
    /// `exception.message`.
    pub message: Option<String>,
    /// `exception.stacktrace`.
    pub stacktrace: Option<String>,
}

impl ExceptionPromotions {
    /// Read the three exception conventions from one attribute set.
    #[must_use]
    pub fn from_attributes(attributes: &[KeyValue]) -> Self {
        Self {
            exception_type: promoted_string(attributes, "exception.type"),
            message: promoted_string(attributes, "exception.message"),
            stacktrace: promoted_string(attributes, "exception.stacktrace"),
        }
    }

    /// Returns the promoted text bytes these values add to one row.
    #[must_use]
    pub fn bytes(&self) -> usize {
        [&self.exception_type, &self.message, &self.stacktrace]
            .into_iter()
            .flatten()
            .map(String::len)
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
    /// Promoted `service.name`.
    pub service_name: Option<String>,
    /// Promoted `service.version`.
    pub service_version: Option<String>,
    /// Promoted `deployment.environment.name`, else `deployment.environment`.
    pub deployment_environment: Option<String>,
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
            service_name: promoted_string(attributes, "service.name"),
            service_version: promoted_string(attributes, "service.version"),
            deployment_environment: promoted_string(attributes, "deployment.environment.name")
                .or_else(|| promoted_string(attributes, "deployment.environment")),
        })
    }

    /// Returns the variable-length payload bytes this envelope adds to each
    /// row that repeats it, including one 4-byte offset per entity reference.
    #[must_use]
    pub fn repeated_bytes(&self) -> usize {
        self.attributes.encoded_bytes()
            + self.schema_url.len()
            + [
                &self.service_name,
                &self.service_version,
                &self.deployment_environment,
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
/// text rejects only its own record. The stored attribute object keeps the
/// same final occurrence, so a correlation always equals its stored source.
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
/// They are not ledger fields and carry no sensitivity tag: Scribe strips `card_ref`
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
/// `element` is the declared element field, complete with its sensitivity
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
/// compared: the sensitivity tag is the server's own physical identity,
/// re-derived here when the validated columns are reassembled under the
/// declared schema, and any caller-supplied field id is dropped, so a writer
/// neither supplies nor can be wrong about either.
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
    if let Some(undeclared) = schema
        .fields()
        .iter()
        .find(|supplied| !declared.iter().any(|field| field.name == supplied.name()))
    {
        return Err(BifrostError::UndeclaredField {
            field: undeclared.name().clone(),
            row: 0,
        });
    }
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
    let fields = fields::canonical_arrow_fields(declared);
    crate::tables::validate_declared_variants(&fields, batch)?;
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
/// out-of-scope, and in-scope-but-UID-less — so they must agree on exactly one
/// scope. Keeping it beside the check it exercises means a change to the
/// authorization rule has one fixture to update rather than three.
#[cfg(test)]
pub(crate) mod correlation_fixture {
    use super::{CardRef, CardRefScope, FromStr, KeyValue, RecordBatch, attributes_variant};
    use wyrd_queue::variant::variant_cell_to_json;

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
            let expected = attributes_variant("attributes", source)
                .expect("fixture attributes project")
                .to_json()
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
