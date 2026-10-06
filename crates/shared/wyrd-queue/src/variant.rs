//! Variant value preparation: JSON to Variant, Arrow Variant validation, and
//! Variant to JSON.
//!
//! Every Bifrost writer that turns open-shaped data into the Parquet/Iceberg
//! Variant encoding goes through [`EncodedVariant`], so one set of rules
//! decides number conversion, the fixed depth and size limits, and the stable
//! failure details. Encoding uses the `parquet-variant` builders directly; this
//! module adds only the Bifrost contract around them:
//!
//! - a JSON integer within the signed 64-bit range is a Variant integer of the
//!   narrowest width, a larger integer within the unsigned 64-bit range is a
//!   scale-zero Variant decimal, any other integer is refused, and a
//!   non-integer number is a double;
//! - an object key whose value is JSON `null` is stored as a Variant null, so
//!   it stays distinct from an absent key;
//! - a value nesting deeper than [`VARIANT_MAX_DEPTH`] containers, or encoding
//!   to more than [`VARIANT_MAX_ENCODED_BYTES`] bytes, is refused.
//!
//! [`VariantColumnBuilder`] assembles encoded values into the canonical
//! unshredded Arrow storage: a struct of non-null `metadata` and `value`
//! binary children under the `arrow.parquet.variant` extension.

use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::sync::Arc;

use arrow::array::{Array, ArrayRef, AsArray, BinaryBuilder, StructArray, make_array};
use arrow::buffer::NullBuffer;
use arrow::compute::cast;
use arrow::error::ArrowError;
use arrow::json::writer::{Encoder, EncoderFactory, EncoderOptions, NullableEncoder};
use arrow_schema::extension::ExtensionType;
use arrow_schema::{DataType, Field, FieldRef, Fields};
use parquet_variant::{
    ObjectFieldBuilder, Variant, VariantBuilder, VariantBuilderExt, VariantMetadata,
};
use parquet_variant_compute::{VariantArray, VariantType};
use parquet_variant_json::VariantToJson;
use serde_json::Value;
use serde_json::value::RawValue;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{VARIANT_MAX_DEPTH, VARIANT_MAX_ENCODED_BYTES};

/// Why one Variant value cannot be stored.
///
/// The violation names where inside the value it failed; the caller supplies
/// the logical field and input row through [`Self::into_error`], which keeps
/// this module independent of any table or batch shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantViolation {
    /// The input is not JSON, or the bytes are not a valid Variant encoding.
    InvalidJson {
        /// RFC 6901 JSON Pointer to the invalid value; empty for the root.
        path: String,
    },
    /// A number fits no Variant numeric type.
    NumericOutOfRange {
        /// RFC 6901 JSON Pointer to the number.
        path: String,
        /// Numeric family that could not hold it.
        numeric_kind: &'static str,
    },
    /// A container nests past [`VARIANT_MAX_DEPTH`].
    TooDeep {
        /// RFC 6901 JSON Pointer to the first container past the limit.
        path: String,
        /// Depth of that container, counting the root container as one.
        depth: u32,
    },
    /// The encoded metadata plus value exceed [`VARIANT_MAX_ENCODED_BYTES`].
    TooLarge {
        /// Encoded bytes, metadata plus value.
        bytes: u64,
    },
}

impl VariantViolation {
    /// Attach the logical field and input row, producing the public error.
    ///
    /// Each violation maps to exactly one catalogued Variant error code with
    /// the locked detail fields.
    #[must_use]
    pub fn into_error(self, field: &str, row: u64) -> BifrostError {
        let field = field.to_owned();
        match self {
            Self::InvalidJson { path } => BifrostError::VariantInvalidJson { field, row, path },
            Self::NumericOutOfRange { path, numeric_kind } => {
                BifrostError::VariantNumericOutOfRange {
                    field,
                    row,
                    path,
                    numeric_kind: numeric_kind.to_owned(),
                }
            }
            Self::TooDeep { path, depth } => BifrostError::VariantTooDeep {
                field,
                row,
                path,
                depth,
                limit: VARIANT_MAX_DEPTH,
            },
            Self::TooLarge { bytes } => BifrostError::VariantTooLarge {
                field,
                row,
                bytes,
                limit: VARIANT_MAX_ENCODED_BYTES,
            },
        }
    }
}

/// The first violation of each class found while checking one value.
///
/// Both input walkers record what they find here instead of returning at the
/// first problem, then [`Self::finish`] picks the public error in the locked
/// order, so where a problem sits in the value never changes which error is
/// reported. Size is checked before any walk and is never recorded.
#[derive(Debug, Default)]
struct VariantViolations {
    /// First malformed input, which outranks every other class.
    malformed: Option<VariantViolation>,
    /// First number outside the exact numeric domain.
    numeric: Option<VariantViolation>,
    /// First container past [`VARIANT_MAX_DEPTH`].
    depth: Option<VariantViolation>,
}

impl VariantViolations {
    /// Keep `violation` unless one of its class was already found.
    fn record(&mut self, violation: VariantViolation) {
        let slot = match violation {
            VariantViolation::NumericOutOfRange { .. } => &mut self.numeric,
            VariantViolation::TooDeep { .. } => &mut self.depth,
            VariantViolation::InvalidJson { .. } | VariantViolation::TooLarge { .. } => {
                &mut self.malformed
            }
        };
        slot.get_or_insert(violation);
    }

    /// Select the public error: malformed, then numeric range, then depth.
    ///
    /// # Errors
    ///
    /// Returns the highest-priority recorded violation, if any.
    fn finish(self) -> Result<(), VariantViolation> {
        match self.malformed.or(self.numeric).or(self.depth) {
            Some(violation) => Err(violation),
            None => Ok(()),
        }
    }
}

/// One canonical encoded Variant value: its metadata and value bytes.
///
/// Construction always enforces the Bifrost limits, the exact numeric domain,
/// and full encoding validity, so holding an `EncodedVariant` means the value
/// is storable and renders exactly in every terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedVariant {
    /// Variant metadata bytes (the field-name dictionary).
    metadata: Vec<u8>,
    /// Variant value bytes.
    value: Vec<u8>,
}

impl EncodedVariant {
    /// Encode one JSON value under the Bifrost Variant rules.
    ///
    /// The value is re-serialized and encoded by [`Self::from_json_text`], the
    /// one JSON walker. A `serde_json` number re-serializes to its exact
    /// integer or shortest round-trip double text, so the stored Variant is
    /// the same as encoding the value's own JSON text.
    ///
    /// # Errors
    ///
    /// Returns [`VariantViolation::TooDeep`] for a container past the depth
    /// limit and [`VariantViolation::TooLarge`] when the encoding exceeds the
    /// size limit.
    pub fn from_json(value: &Value) -> Result<Self, VariantViolation> {
        Self::from_json_text(&value.to_string())
    }

    /// Parse JSON text and encode it under the Bifrost Variant rules.
    ///
    /// Unlike [`Self::from_json`], numbers are classified from their original
    /// tokens, so an integer beyond the 64-bit range keeps its exact digits
    /// instead of passing through `f64`. The text is validated once as a
    /// [`RawValue`], then each container is split into raw children; an object
    /// repeating a key keeps its final occurrence.
    ///
    /// # Errors
    ///
    /// Returns [`VariantViolation::InvalidJson`] at the root when the text is
    /// not JSON, [`VariantViolation::NumericOutOfRange`] for a number no
    /// Variant numeric type holds, [`VariantViolation::TooDeep`] for a
    /// container past the depth limit, and [`VariantViolation::TooLarge`] when
    /// the encoding exceeds the size limit.
    pub fn from_json_text(text: &str) -> Result<Self, VariantViolation> {
        let mut path = JsonPointer::default();
        let raw: &RawValue = serde_json::from_str(text).map_err(|_| invalid(&path))?;
        let mut builder = VariantBuilder::new();
        let mut found = VariantViolations::default();
        append_raw(&mut builder, raw, &mut path, 0, &mut found);
        found.finish()?;
        let (metadata, value) = builder.finish();
        Self::sized(metadata, value)
    }

    /// Accept already-encoded Variant bytes after full validation.
    ///
    /// Size is checked first. Then [`scan_encoded`] visits every node of the
    /// size-bounded value with an explicit stack, recording malformed
    /// encoding, numbers outside the exact domain, and the first container
    /// past [`VARIANT_MAX_DEPTH`]. Upstream's recursive [`Variant::try_new`]
    /// recurses once per nesting level, and a compact hostile value can nest
    /// far deeper than any stack allows, so it runs only when no depth
    /// violation was found; it remains the encoding authority for every
    /// accepted value. The shallow accessors the scan uses panic on malformed
    /// bytes, which is contained here and reported as invalid. The recorded
    /// violations then select one error, malformed first.
    ///
    /// # Errors
    ///
    /// Returns [`VariantViolation::TooLarge`] past the size limit, then
    /// [`VariantViolation::InvalidJson`] for bytes that are not a valid
    /// Variant, [`VariantViolation::NumericOutOfRange`] for a number outside
    /// the exact domain, and [`VariantViolation::TooDeep`] for a container
    /// past the depth limit.
    pub fn from_bytes(metadata: &[u8], value: &[u8]) -> Result<Self, VariantViolation> {
        let encoded = Self::sized(metadata.to_vec(), value.to_vec())?;
        let root = JsonPointer::default();
        let mut found =
            std::panic::catch_unwind(|| scan_encoded(metadata, value)).unwrap_or_else(|_| {
                let mut found = VariantViolations::default();
                found.record(invalid(&root));
                found
            });
        if found.malformed.is_none()
            && found.depth.is_none()
            && Variant::try_new(metadata, value).is_err()
        {
            found.record(invalid(&root));
        }
        found.finish()?;
        Ok(encoded)
    }

    /// Wrap finished bytes after the size check.
    ///
    /// Private because size is only the first check: the public constructors
    /// call it and then add the encoding and depth guarantees the type promises.
    ///
    /// # Errors
    ///
    /// Returns [`VariantViolation::TooLarge`] past [`VARIANT_MAX_ENCODED_BYTES`].
    fn sized(metadata: Vec<u8>, value: Vec<u8>) -> Result<Self, VariantViolation> {
        let bytes = u64::try_from(metadata.len() + value.len()).unwrap_or(u64::MAX);
        if bytes > VARIANT_MAX_ENCODED_BYTES {
            return Err(VariantViolation::TooLarge { bytes });
        }
        Ok(Self { metadata, value })
    }

    /// Return the metadata bytes.
    #[must_use]
    pub fn metadata(&self) -> &[u8] {
        &self.metadata
    }

    /// Return the value bytes.
    #[must_use]
    pub fn value(&self) -> &[u8] {
        &self.value
    }

    /// Return the encoded size, metadata plus value.
    ///
    /// Producers compare it against per-field byte budgets; it is never zero
    /// because every valid Variant has metadata and value bytes.
    #[must_use]
    pub fn encoded_bytes(&self) -> usize {
        self.metadata.len() + self.value.len()
    }
}

/// Render one stored Variant's bytes as JSON without copying them.
///
/// Result terminals use this to turn an Arrow Variant column cell into the
/// language's native open value.
///
/// # Errors
///
/// Returns [`VariantViolation::InvalidJson`] when the bytes are not a valid
/// Variant or hold a non-finite float.
pub fn variant_bytes_to_json(metadata: &[u8], value: &[u8]) -> Result<Value, VariantViolation> {
    Variant::try_new(metadata, value)
        .and_then(|variant| variant.to_json_value())
        .map_err(|_| VariantViolation::InvalidJson {
            path: String::new(),
        })
}

/// Render one Arrow Variant column cell as JSON.
///
/// Decodes through upstream [`VariantArray`], which accepts any storage the
/// `arrow.parquet.variant` extension allows for an unshredded value: a struct
/// whose `metadata` and `value` children are `Binary`, `LargeBinary`, or
/// `BinaryView`. A null or placeholder cell renders as JSON null. The cell is
/// decoded with upstream's shallow validation, so it must come from storage
/// whose writers validated every value, as every Bifrost writer does.
///
/// # Errors
///
/// Returns [`VariantViolation::InvalidJson`] when the column is not an
/// unshredded Variant storage struct or the cell does not render as JSON.
pub fn variant_cell_to_json(column: &dyn Array, row: usize) -> Result<Value, VariantViolation> {
    let invalid = |_| VariantViolation::InvalidJson {
        path: String::new(),
    };
    let cell = mask_placeholders(column.slice(row, 1)).map_err(invalid)?;
    if cell.is_null(0) {
        return Ok(Value::Null);
    }
    VariantArray::try_new(&cell)
        .and_then(|variants| variants.try_value(0)?.to_json_value())
        .map_err(invalid)
}

/// Renders Arrow Variant columns as their JSON value in Arrow's JSON writer.
///
/// Arrow's JSON writer would otherwise print a Variant as its storage struct
/// of `metadata`/`value` bytes. Installing this factory on a writer's
/// [`EncoderOptions`] makes every field
/// carrying the `arrow.parquet.variant` extension — top level or nested in a
/// Struct or List — render through upstream [`VariantArray`] as the same JSON
/// value [`variant_bytes_to_json`] yields, so every row-as-JSON surface shares
/// one rendering and a double such as `3.0` stays a float. Other fields keep
/// Arrow's default encoders.
#[derive(Debug, Default)]
pub struct VariantJsonEncoderFactory;

impl EncoderFactory for VariantJsonEncoderFactory {
    /// Pre-render every non-null cell of a Variant field as JSON text.
    ///
    /// Rendering is done up front because the encoder itself cannot fail.
    /// Placeholder slots under a null parent row are masked first and the
    /// encoder takes the masked nulls, so they render as JSON null.
    ///
    /// # Errors
    ///
    /// Returns [`ArrowError::JsonError`] when a cell with storage
    /// is not a decodable unshredded Variant.
    fn make_default_encoder<'a>(
        &self,
        field: &'a FieldRef,
        array: &'a dyn Array,
        _options: &'a EncoderOptions,
    ) -> Result<Option<NullableEncoder<'a>>, ArrowError> {
        if !is_variant(field) {
            return Ok(None);
        }
        let storage = mask_placeholders(make_array(array.to_data()))?;
        let variants = VariantArray::try_new(&storage)?;
        let rendered = (0..storage.len())
            .map(|row| {
                if storage.is_null(row) {
                    return Ok(String::new());
                }
                variants
                    .try_value(row)
                    .and_then(|value| value.to_json_value())
                    .map(|json| json.to_string())
                    .map_err(|_| {
                        ArrowError::JsonError(format!(
                            "field {} row {row} is not a decodable Variant",
                            field.name()
                        ))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(NullableEncoder::new(
            Box::new(RenderedJson(rendered)),
            storage.logical_nulls(),
        )))
    }
}

/// Prepare one stored Variant column for upstream decoding.
///
/// Every encoded Variant has at least one `metadata` and one `value` byte, so
/// a cell whose two children are both empty carries no value at all. Such a
/// cell appears under a null parent struct, whose null is neither pushed down
/// by the Parquet reader nor by `get_field`, so it is marked null here. Every
/// other present cell is then fully validated with [`Variant::try_new`]:
/// upstream decoding validates shallowly and panics on malformed bytes, so
/// every reader passes the column through here first and malformed stored
/// bytes become an error instead. A non-struct array is returned unchanged.
///
/// # Errors
///
/// Returns the Arrow error raised while viewing a child as bytes or
/// reassembling the masked struct, and [`ArrowError::InvalidArgumentError`]
/// naming the row of the first present cell that is not a valid Variant.
pub fn mask_placeholders(storage: ArrayRef) -> Result<ArrayRef, ArrowError> {
    let Some(columns) = storage.as_struct_opt() else {
        return Ok(storage);
    };
    let bytes = |name: &str| {
        columns
            .column_by_name(name)
            .map(|child| cast(child, &DataType::BinaryView))
            .transpose()
    };
    let (metadata, value) = (bytes("metadata")?, bytes("value")?);
    let mut present = Vec::with_capacity(storage.len());
    for row in 0..storage.len() {
        let cell = match (&metadata, &value) {
            (Some(metadata), Some(value)) => Some((
                metadata.as_binary_view().value(row),
                value.as_binary_view().value(row),
            )),
            _ => None,
        };
        let valid = storage.is_valid(row) && !matches!(cell, Some(([], [])));
        if let (true, Some((metadata, value))) = (valid, cell) {
            Variant::try_new(metadata, value).map_err(|error| {
                ArrowError::InvalidArgumentError(format!(
                    "row {row} is not a valid stored Variant: {error}"
                ))
            })?;
        }
        present.push(valid);
    }
    let (fields, children, _) = columns.clone().into_parts();
    Ok(Arc::new(StructArray::try_new(
        fields,
        children,
        Some(NullBuffer::from(present)),
    )?))
}

/// Pre-rendered JSON text of each row of one Variant column.
struct RenderedJson(Vec<String>);

impl Encoder for RenderedJson {
    /// Copy row `idx`'s rendered JSON into the writer's buffer.
    fn encode(&mut self, idx: usize, out: &mut Vec<u8>) {
        out.extend_from_slice(self.0[idx].as_bytes());
    }
}

/// Return the canonical unshredded Variant storage type.
///
/// Two non-null `Binary` children, `metadata` then `value`: the exact shape
/// the Iceberg schema converter produces for an Iceberg `variant`, so writers
/// and a catalog round trip agree.
#[must_use]
pub fn variant_storage_type() -> DataType {
    DataType::Struct(variant_storage_fields())
}

/// Return the two children of the canonical Variant storage struct.
fn variant_storage_fields() -> Fields {
    Fields::from(vec![
        Field::new("metadata", DataType::Binary, false),
        Field::new("value", DataType::Binary, false),
    ])
}

/// Declare one Variant field: canonical storage under the
/// `arrow.parquet.variant` extension.
#[must_use]
pub fn variant_field(name: &str, nullable: bool) -> Field {
    Field::new(name, variant_storage_type(), nullable).with_extension_type(VariantType)
}

/// Report whether a field carries the canonical `arrow.parquet.variant`
/// extension.
///
/// The extension, not the storage struct, is what makes a column a Variant: a
/// user Struct with `metadata`/`value` children stays a Struct. The canonical
/// extension takes no parameters, so its metadata must be empty; Arrow writes
/// it as `""` and the Iceberg schema converter omits the key, and both are
/// accepted. A field naming the extension with any other metadata is not the
/// canonical Variant, so every wire comparison refuses it rather than
/// normalizing it away.
#[must_use]
pub fn is_variant(field: &Field) -> bool {
    field.extension_type_name() == Some(VariantType::NAME)
        && matches!(field.extension_type_metadata(), None | Some(""))
}

/// Accumulates encoded Variant values into one canonical Arrow column.
///
/// The column is the unshredded storage struct of [`variant_storage_type`];
/// a null row keeps empty child bytes under the struct's validity bit, as the
/// Parquet Variant layout requires for a null top-level value.
#[derive(Debug)]
pub struct VariantColumnBuilder {
    /// `metadata` child bytes.
    metadata: BinaryBuilder,
    /// `value` child bytes.
    value: BinaryBuilder,
    /// Row validity of the enclosing struct.
    validity: Vec<bool>,
}

impl VariantColumnBuilder {
    /// Start an empty column sized for `rows` values.
    #[must_use]
    pub fn with_capacity(rows: usize) -> Self {
        Self {
            metadata: BinaryBuilder::with_capacity(rows, 0),
            value: BinaryBuilder::with_capacity(rows, 0),
            validity: Vec::with_capacity(rows),
        }
    }

    /// Encode each optional input into one column; `None` is a null row.
    ///
    /// Every producer of a Variant column from open values goes through this
    /// one loop, passing [`EncodedVariant::from_json`] or
    /// [`EncodedVariant::from_json_text`] as `encode`. Encoding stops at the
    /// first refused value, so no partial column escapes.
    ///
    /// # Errors
    ///
    /// Returns the catalogued Variant error naming `field` and the zero-based
    /// row of the first value `encode` refuses.
    pub fn encode<T>(
        field: &str,
        values: impl IntoIterator<Item = Option<T>>,
        encode: impl Fn(T) -> Result<EncodedVariant, VariantViolation>,
    ) -> Result<ArrayRef, BifrostError> {
        values
            .into_iter()
            .enumerate()
            .map(|(row, value)| {
                value.map(&encode).transpose().map_err(|violation| {
                    violation.into_error(field, u64::try_from(row).unwrap_or(u64::MAX))
                })
            })
            .collect::<Result<Self, _>>()
            .map(Self::finish)
    }

    /// Append one present value.
    pub fn append(&mut self, value: &EncodedVariant) {
        self.metadata.append_value(&value.metadata);
        self.value.append_value(&value.value);
        self.validity.push(true);
    }

    /// Append one optional value; `None` is a null row.
    pub fn append_option(&mut self, value: Option<&EncodedVariant>) {
        match value {
            Some(value) => self.append(value),
            None => self.append_null(),
        }
    }

    /// Append one null row.
    pub fn append_null(&mut self) {
        self.metadata.append_value([]);
        self.value.append_value([]);
        self.validity.push(false);
    }

    /// Finish the canonical storage struct column.
    ///
    /// A column with no null rows carries no validity buffer.
    #[must_use]
    pub fn finish(mut self) -> ArrayRef {
        let nulls = self
            .validity
            .contains(&false)
            .then(|| NullBuffer::from(self.validity));
        Arc::new(StructArray::new(
            variant_storage_fields(),
            vec![
                Arc::new(self.metadata.finish()) as ArrayRef,
                Arc::new(self.value.finish()) as ArrayRef,
            ],
            nulls,
        ))
    }
}

/// Collects owned or borrowed encoded values; `None` is a null row.
impl<V: Borrow<EncodedVariant>> FromIterator<Option<V>> for VariantColumnBuilder {
    fn from_iter<I: IntoIterator<Item = Option<V>>>(values: I) -> Self {
        let values = values.into_iter();
        let mut builder = Self::with_capacity(values.size_hint().0);
        for value in values {
            builder.append_option(value.as_ref().map(Borrow::borrow));
        }
        builder
    }
}

/// RFC 6901 JSON Pointer under construction during a walk.
#[derive(Debug, Default)]
struct JsonPointer {
    /// Escaped reference tokens from the root.
    tokens: Vec<String>,
}

impl JsonPointer {
    /// Descend into one object key, escaping `~` and `/`.
    fn push_key(&mut self, key: &str) {
        self.tokens.push(Self::escape(key));
    }

    /// Return the reference token of one object key, escaping `~` and `/`.
    fn escape(key: &str) -> String {
        key.replace('~', "~0").replace('/', "~1")
    }

    /// Return to the ancestor `depth` tokens below the root.
    ///
    /// The explicit-stack walks record each pending value's parent depth and
    /// truncate to it, since they cannot pop in step with recursion.
    fn truncate(&mut self, depth: usize) {
        self.tokens.truncate(depth);
    }

    /// Descend into one array index.
    fn push_index(&mut self, index: usize) {
        self.tokens.push(index.to_string());
    }

    /// Return to the parent.
    fn pop(&mut self) {
        self.tokens.pop();
    }

    /// Render the pointer; the root is the empty string.
    fn render(&self) -> String {
        self.tokens.iter().fold(String::new(), |mut out, token| {
            out.push('/');
            out.push_str(token);
            out
        })
    }
}

/// Open a container below `depth` others, recording it when past the limit.
///
/// Returns the container's own depth, counting the root container as one,
/// and whether it is within [`VARIANT_MAX_DEPTH`].
fn enter_container(depth: u32, path: &JsonPointer, found: &mut VariantViolations) -> (u32, bool) {
    let depth = depth + 1;
    let within = depth <= VARIANT_MAX_DEPTH;
    if !within {
        found.record(VariantViolation::TooDeep {
            path: path.render(),
            depth,
        });
    }
    (depth, within)
}

/// Append one validated raw JSON value to any Variant builder position.
///
/// The token's first byte selects its kind; containers are split into raw
/// children and walked recursively, and numbers go to [`raw_number_variant`]
/// with their original text. `depth` is the number of containers already
/// open above this value.
///
/// Every violation is recorded in `found` rather than returned, so the walk
/// always covers the whole value and [`VariantViolations::finish`] alone
/// picks the error. A refused number is built as a placeholder null. A
/// container past the depth limit is not built: [`scan_numbers`] searches it
/// without recursion, so an out-of-range number below the limit still
/// outranks depth. Recursion therefore never passes the limit.
fn append_raw(
    builder: &mut impl VariantBuilderExt,
    raw: &RawValue,
    path: &mut JsonPointer,
    depth: u32,
    found: &mut VariantViolations,
) {
    let text = raw.get();
    let first = text.as_bytes().first();
    let depth = if matches!(first, Some(b'{' | b'[')) {
        let (depth, within) = enter_container(depth, path, found);
        if !within {
            scan_numbers(raw, path, found);
            return;
        }
        depth
    } else {
        depth
    };
    match first {
        Some(b'{') => {
            // A map keeps the final occurrence of a repeated key.
            // ponytail: each level re-scans its subtree, so cost is bounded by
            // depth (64) times size; a streaming visitor if parse_json profiles hot.
            let Ok(entries) = serde_json::from_str::<BTreeMap<String, &RawValue>>(text) else {
                return found.record(invalid(path));
            };
            let Ok(mut object) = builder.try_new_object() else {
                return found.record(invalid(path));
            };
            for (key, child) in entries {
                path.push_key(&key);
                if child.get() == "null" {
                    // A field builder would treat null as an absent key.
                    object.insert(&key, Variant::Null);
                } else {
                    append_raw(
                        &mut ObjectFieldBuilder::new(&key, &mut object),
                        child,
                        path,
                        depth,
                        found,
                    );
                }
                path.pop();
            }
            object.finish();
        }
        Some(b'[') => {
            let Ok(items) = serde_json::from_str::<Vec<&RawValue>>(text) else {
                return found.record(invalid(path));
            };
            let Ok(mut list) = builder.try_new_list() else {
                return found.record(invalid(path));
            };
            for (index, item) in items.into_iter().enumerate() {
                path.push_index(index);
                append_raw(&mut list, item, path, depth, found);
                path.pop();
            }
            list.finish();
        }
        Some(b'"') => match serde_json::from_str::<String>(text) {
            Ok(string) => builder.append_value(string.as_str()),
            Err(_) => found.record(invalid(path)),
        },
        Some(b't') => builder.append_value(true),
        Some(b'f') => builder.append_value(false),
        Some(b'n') => builder.append_value(Variant::Null),
        _ => match raw_number_variant(text, path) {
            Ok(number) => builder.append_value(number),
            Err(violation) => {
                found.record(violation);
                builder.append_value(Variant::Null);
            }
        },
    }
}

/// Record the first out-of-range number inside a JSON container past the
/// depth limit, without building it.
///
/// The subtree is walked with an explicit stack in the same order as
/// [`append_raw`] — object keys sorted with the final occurrence of a repeated
/// key kept, then list items — so the reported pointer is the one the normal
/// walk would name. Nothing is searched once a numeric violation is known,
/// since only the first one is reported. `path` is restored on return.
fn scan_numbers(raw: &RawValue, path: &mut JsonPointer, found: &mut VariantViolations) {
    let base = path.tokens.len();
    // Each pending value carries its parent's pointer depth and its token.
    let mut pending: Vec<(usize, Option<String>, &RawValue)> = vec![(base, None, raw)];
    while found.numeric.is_none() {
        let Some((parent, token, raw)) = pending.pop() else {
            break;
        };
        path.truncate(parent);
        if let Some(token) = token {
            path.tokens.push(token);
        }
        let text = raw.get();
        let here = path.tokens.len();
        let start = pending.len();
        match text.as_bytes().first() {
            Some(b'{') => match serde_json::from_str::<BTreeMap<String, &RawValue>>(text) {
                Ok(entries) => pending.extend(
                    entries
                        .into_iter()
                        .map(|(key, child)| (here, Some(JsonPointer::escape(&key)), child)),
                ),
                Err(_) => found.record(invalid(path)),
            },
            Some(b'[') => match serde_json::from_str::<Vec<&RawValue>>(text) {
                Ok(items) => pending.extend(
                    items
                        .into_iter()
                        .enumerate()
                        .map(|(index, item)| (here, Some(index.to_string()), item)),
                ),
                Err(_) => found.record(invalid(path)),
            },
            Some(b'"' | b't' | b'f' | b'n') => {}
            _ => {
                if let Err(violation) = raw_number_variant(text, path) {
                    found.record(violation);
                }
            }
        }
        pending[start..].reverse();
    }
    path.truncate(base);
}

/// Convert one JSON number token under the Bifrost numeric rules.
///
/// A token without a fraction or exponent is an integer: within `i64` it is
/// the narrowest Variant integer, above `i64` but within `u64` it is a
/// scale-zero decimal, and anything else is refused, so every accepted
/// integer reads back exactly as a `serde_json::Number`. Any other token is a
/// double.
///
/// # Errors
///
/// Returns [`VariantViolation::NumericOutOfRange`] for an integer outside the
/// 64-bit ranges and for a double that is not finite.
fn raw_number_variant(
    token: &str,
    path: &JsonPointer,
) -> Result<Variant<'static, 'static>, VariantViolation> {
    if !token.contains(['.', 'e', 'E']) {
        if let Ok(integer) = token.parse::<i64>() {
            return Ok(narrow_integer(integer));
        }
        return token
            .parse::<u64>()
            .map(Variant::from)
            .map_err(|_| out_of_range(path, "integer"));
    }
    token
        .parse::<f64>()
        .ok()
        .filter(|double| double.is_finite())
        .map(Variant::from)
        .ok_or_else(|| out_of_range(path, "double"))
}

/// Return a numeric-range violation at the current pointer.
fn out_of_range(path: &JsonPointer, numeric_kind: &'static str) -> VariantViolation {
    VariantViolation::NumericOutOfRange {
        path: path.render(),
        numeric_kind,
    }
}

/// Return an invalid-JSON violation at the current pointer.
fn invalid(path: &JsonPointer) -> VariantViolation {
    VariantViolation::InvalidJson {
        path: path.render(),
    }
}

/// Store a signed integer at the narrowest Variant integer width.
///
/// Every Bifrost producer that writes an integer into a Variant uses this
/// one rule, so JSON input and protocol integers read back identically.
#[must_use]
pub fn narrow_integer(integer: i64) -> Variant<'static, 'static> {
    if let Ok(narrow) = i8::try_from(integer) {
        Variant::from(narrow)
    } else if let Ok(narrow) = i16::try_from(integer) {
        Variant::from(narrow)
    } else if let Ok(narrow) = i32::try_from(integer) {
        Variant::from(narrow)
    } else {
        Variant::from(integer)
    }
}

/// Visit every node of raw Variant bytes and record what is wrong with them.
///
/// Metadata is fully validated first. Nodes are then reached through
/// upstream's shallow, constant-time accessors from an explicit stack, so a
/// hostile depth costs heap, not call stack, and the whole size-bounded value
/// is covered. Together the shallow node checks, the object field-name order
/// check, and the metadata check are the conditions upstream full validation
/// applies. The first container past [`VARIANT_MAX_DEPTH`] is recorded and the
/// walk continues below it, and a Decimal16 outside the exact numeric domain is recorded at
/// its pointer: only the scale-zero form of `i64::MAX + 1..=u64::MAX`, the
/// encoding of a JSON integer in that range, is accepted.
///
/// Object field offsets may point at shared bytes, so a few hundred bytes can
/// describe a tree with exponentially many nodes. Every honestly encoded node
/// owns at least one byte, so a walk that visits more nodes than the value has
/// bytes records the value as malformed and stops, keeping the scan linear in
/// the input.
///
/// # Panics
///
/// Panics when a node's bytes are malformed; [`EncodedVariant::from_bytes`]
/// contains it.
fn scan_encoded(metadata: &[u8], value: &[u8]) -> VariantViolations {
    let mut found = VariantViolations::default();
    let mut path = JsonPointer::default();
    let Ok(metadata) = VariantMetadata::try_new(metadata) else {
        found.record(invalid(&path));
        return found;
    };
    let exact_unsigned = i128::from(i64::MAX) + 1..=i128::from(u64::MAX);
    // Each pending node carries its parent's pointer depth, its token, and
    // the number of containers open above it.
    let mut pending = vec![(0, None, 0, Variant::new_with_metadata(metadata, value))];
    let mut visited = 0_usize;
    while let Some((parent, token, depth, node)) = pending.pop() {
        visited += 1;
        if visited > value.len() {
            found.record(invalid(&JsonPointer::default()));
            break;
        }
        path.truncate(parent);
        if let Some(token) = token {
            path.tokens.push(token);
        }
        let here = path.tokens.len();
        let start = pending.len();
        match node {
            Variant::Object(object) => {
                let (depth, _) = enter_container(depth, &path, &mut found);
                let sorted = object.metadata.is_sorted();
                let mut previous: Option<&str> = None;
                for (name, child) in object.iter() {
                    if previous.is_some_and(|previous| {
                        if sorted {
                            name <= previous
                        } else {
                            name < previous
                        }
                    }) {
                        found.record(invalid(&path));
                    }
                    previous = Some(name);
                    pending.push((here, Some(JsonPointer::escape(name)), depth, child));
                }
            }
            Variant::List(list) => {
                let (depth, _) = enter_container(depth, &path, &mut found);
                for (index, child) in list.iter().enumerate() {
                    pending.push((here, Some(index.to_string()), depth, child));
                }
            }
            Variant::Decimal16(decimal)
                if decimal.scale() != 0 || !exact_unsigned.contains(&decimal.integer()) =>
            {
                found.record(out_of_range(&path, "decimal"));
            }
            _ => {}
        }
        pending[start..].reverse();
        if found.malformed.is_some() {
            break;
        }
    }
    found
}

/// Contract tests for Variant encoding, validation, and JSON rendering.
#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{BinaryArray, RecordBatch, StringArray};
    use arrow::json::WriterBuilder;
    use arrow::json::writer::JsonArray;
    use arrow_schema::Schema;
    use serde_json::json;

    /// JSON converts with exact integers, null-vs-missing, and limits.
    ///
    /// # Panics
    ///
    /// Panics when a conversion rule or a limit drifts from the contract.
    #[test]
    fn json_converts_under_the_variant_contract() {
        let encoded = EncodedVariant::from_json(&json!({
            "small": 7, "big": 9_007_199_254_740_993_i64, "huge": u64::MAX,
            "ratio": 0.5, "flag": true, "gone": null, "list": [1, "a"], "nested": {"k": "v"}
        }))
        .expect("encodes");
        let variant = Variant::try_new(encoded.metadata(), encoded.value()).expect("valid");
        let object = variant.as_object().expect("object");
        assert_eq!(object.get("small"), Some(Variant::Int8(7)));
        assert_eq!(
            object.get("big"),
            Some(Variant::Int64(9_007_199_254_740_993))
        );
        assert!(matches!(object.get("huge"), Some(Variant::Decimal16(_))));
        assert_eq!(object.get("gone"), Some(Variant::Null));
        assert_eq!(object.get("absent"), None);
        assert_eq!(
            variant_bytes_to_json(encoded.metadata(), encoded.value()).expect("json")["big"],
            json!(9_007_199_254_740_993_i64)
        );

        let mut deep = json!(1);
        for _ in 0..VARIANT_MAX_DEPTH {
            deep = json!([deep]);
        }
        EncodedVariant::from_json(&deep).expect("exactly the depth limit encodes");
        let too_deep = json!([deep]);
        let violation = EncodedVariant::from_json(&too_deep).expect_err("one past refuses");
        assert!(matches!(
            violation,
            VariantViolation::TooDeep { depth: 65, .. }
        ));
        let error = violation.into_error("payload", 3);
        assert_eq!(error.code(), "WYRD_VALA_400_VARIANT_TOO_DEEP");

        let big =
            Value::String("x".repeat(usize::try_from(VARIANT_MAX_ENCODED_BYTES).expect("fits")));
        assert!(matches!(
            EncodedVariant::from_json(&big),
            Err(VariantViolation::TooLarge { .. })
        ));
        assert_eq!(
            EncodedVariant::from_json_text("{nope"),
            Err(VariantViolation::InvalidJson {
                path: String::new()
            })
        );
    }

    /// JSON text classifies integers by the 64-bit ranges and keeps the final
    /// duplicate key.
    ///
    /// Covers `i64::MAX`, `i64::MAX + 1`, `u64::MAX`, and `i64::MIN` as exact
    /// values that render back as the same JSON number, `u64::MAX + 1` and
    /// `i64::MIN - 1` as refused, fraction and exponent tokens as doubles,
    /// and a repeated object key.
    ///
    /// # Panics
    ///
    /// Panics when a token converts to another Variant type, renders other
    /// digits, or is not refused with the exact range violation.
    #[test]
    fn json_text_classifies_integers_from_their_tokens() {
        let text = format!(
            r#"{{"i64_max": {}, "past_i64": {}, "u64_max": {}, "i64_min": {},
                "fraction": 2.5, "exponent": 1e3, "dup": 1, "dup": "last"}}"#,
            i64::MAX,
            i64::MAX.unsigned_abs() + 1,
            u64::MAX,
            i64::MIN
        );
        let encoded = EncodedVariant::from_json_text(&text).expect("encodes");
        let variant = Variant::try_new(encoded.metadata(), encoded.value()).expect("valid");
        let object = variant.as_object().expect("object");
        let decimal = |integer: u64| {
            Some(Variant::from(
                parquet_variant::VariantDecimal16::try_new(i128::from(integer), 0)
                    .expect("in range"),
            ))
        };
        assert_eq!(object.get("i64_max"), Some(Variant::Int64(i64::MAX)));
        assert_eq!(object.get("past_i64"), decimal(i64::MAX.unsigned_abs() + 1));
        assert_eq!(object.get("u64_max"), decimal(u64::MAX));
        assert_eq!(object.get("i64_min"), Some(Variant::Int64(i64::MIN)));
        assert_eq!(object.get("fraction"), Some(Variant::Double(2.5)));
        assert_eq!(object.get("exponent"), Some(Variant::Double(1000.0)));
        assert_eq!(object.get("dup"), Some(Variant::from("last")));
        let rendered = variant_bytes_to_json(encoded.metadata(), encoded.value()).expect("json");
        assert_eq!(rendered["i64_max"], json!(i64::MAX));
        assert_eq!(rendered["past_i64"], json!(i64::MAX.unsigned_abs() + 1));
        assert_eq!(rendered["u64_max"], json!(u64::MAX));
        assert_eq!(rendered["i64_min"], json!(i64::MIN));

        for token in ["18446744073709551616", "-9223372036854775809"] {
            assert_eq!(
                EncodedVariant::from_json_text(&format!(r#"{{"n": [{token}]}}"#)),
                Err(VariantViolation::NumericOutOfRange {
                    path: "/n/0".to_owned(),
                    numeric_kind: "integer",
                }),
                "{token}"
            );
        }
    }

    /// Numeric range outranks depth whatever the object key order, and also
    /// when the number sits inside the container past the limit.
    ///
    /// # Panics
    ///
    /// Panics when either key order or the nested number reports depth, or
    /// depth alone stops reporting depth.
    #[test]
    fn numeric_range_outranks_depth_in_any_key_order() {
        let nested = format!(
            r#"{{"a": {}18446744073709551616{}}}"#,
            "[".repeat(65),
            "]".repeat(65)
        );
        assert_eq!(
            EncodedVariant::from_json_text(&nested),
            Err(VariantViolation::NumericOutOfRange {
                path: format!("/a{}", "/0".repeat(65)),
                numeric_kind: "integer",
            })
        );
        let deep = format!("{}1{}", "[".repeat(65), "]".repeat(65));
        for (numeric, nested) in [("a", "b"), ("b", "a")] {
            assert_eq!(
                EncodedVariant::from_json_text(&format!(
                    r#"{{"{numeric}": 18446744073709551616, "{nested}": {deep}}}"#
                )),
                Err(VariantViolation::NumericOutOfRange {
                    path: format!("/{numeric}"),
                    numeric_kind: "integer",
                }),
                "numeric key {numeric}"
            );
        }
        assert!(matches!(
            EncodedVariant::from_json_text(&format!(r#"{{"a": 1, "b": {deep}}}"#)),
            Err(VariantViolation::TooDeep { depth: 65, .. })
        ));
    }

    /// A compact raw Variant nested far past the limit is refused for depth
    /// before upstream's recursive validation can exhaust the stack, and
    /// malformed bytes stay a typed error.
    ///
    /// # Panics
    ///
    /// Panics when the deep value is not refused at depth 65 or malformed
    /// bytes are accepted.
    #[test]
    fn raw_depth_is_bounded_before_full_validation() {
        assert!(matches!(
            EncodedVariant::from_bytes(EMPTY_METADATA, &nest(vec![0x00], 20_000)),
            Err(VariantViolation::TooDeep { depth: 65, .. })
        ));
        assert_eq!(
            EncodedVariant::from_bytes(&[0xff], &[0x0f, 1, 0xff]),
            Err(VariantViolation::InvalidJson {
                path: String::new()
            })
        );
    }

    /// Variant metadata with an empty field-name dictionary.
    const EMPTY_METADATA: &[u8] = &[0x01, 0x00, 0x00];

    /// Encode a raw Variant list of `items` with four-byte offsets.
    fn list(items: &[Vec<u8>]) -> Vec<u8> {
        let mut list = vec![0x0f, u8::try_from(items.len()).expect("short list")];
        let mut offset = 0_u32;
        list.extend(offset.to_le_bytes());
        for item in items {
            offset += u32::try_from(item.len()).expect("fits");
            list.extend(offset.to_le_bytes());
        }
        list.extend(items.concat());
        list
    }

    /// Wrap raw Variant `value` in `levels` single-item lists.
    fn nest(value: Vec<u8>, levels: usize) -> Vec<u8> {
        (0..levels).fold(value, |value, _| list(&[value]))
    }

    /// Malformed encoding outranks depth wherever it sits in the value.
    ///
    /// The malformed node is placed below a container past the limit, and
    /// beside an over-depth sibling in both orders; each must be refused as
    /// invalid rather than too deep.
    ///
    /// # Panics
    ///
    /// Panics when any placement reports depth or is accepted.
    #[test]
    fn raw_malformed_outranks_depth_in_any_position() {
        // A primitive header naming no primitive type.
        let malformed = vec![0x7c_u8];
        let deep = nest(vec![0x00], 70);
        for value in [
            nest(malformed.clone(), 70),
            list(&[deep.clone(), malformed.clone()]),
            list(&[malformed, deep]),
        ] {
            assert_eq!(
                EncodedVariant::from_bytes(EMPTY_METADATA, &value),
                Err(VariantViolation::InvalidJson {
                    path: String::new()
                })
            );
        }
    }

    /// Object fields that share bytes are refused before the walk explodes.
    ///
    /// Each of 64 levels holds fields `a` and `b` pointing at the same child,
    /// about 640 bytes describing 2^64 nodes. It must be refused as invalid
    /// encoding at once, not walked node by node.
    ///
    /// # Panics
    ///
    /// Panics when the shared value is accepted or reported as anything but
    /// invalid encoding.
    #[test]
    fn raw_shared_field_values_are_refused() {
        let metadata = [0x11_u8, 2, 0, 1, 2, b'a', b'b'];
        let mut value = vec![0x00_u8];
        for _ in 0..64 {
            let end = u16::try_from(value.len()).expect("the chain fits two-byte offsets");
            // Object, two-byte offsets: fields a and b both start at offset 0.
            let mut node = vec![0x06, 2, 0, 1];
            for offset in [0, 0, end] {
                node.extend_from_slice(&offset.to_le_bytes());
            }
            node.extend_from_slice(&value);
            value = node;
        }
        assert_eq!(
            EncodedVariant::from_bytes(&metadata, &value),
            Err(VariantViolation::InvalidJson {
                path: String::new()
            })
        );
    }

    /// Raw Decimal16 values are accepted only as the exact JSON integers
    /// above `i64::MAX`, and a refused one outranks depth.
    ///
    /// # Panics
    ///
    /// Panics when a decimal outside the domain is accepted, a refusal loses
    /// its pointer, or `u64::MAX` stops rendering with every digit.
    #[test]
    fn raw_decimals_outside_the_exact_domain_are_refused() {
        let encode = |integer: i128, scale: u8| {
            let decimal =
                parquet_variant::VariantDecimal16::try_new(integer, scale).expect("decimal");
            let mut builder = VariantBuilder::new();
            let mut object = builder.new_object();
            let mut list = object.new_list("n");
            list.append_value(decimal);
            list.finish();
            object.finish();
            builder.finish()
        };
        for (integer, scale) in [
            (12_345, 2),
            (i128::from(u64::MAX) + 1, 0),
            (5, 0),
            (-i128::from(u64::MAX), 0),
        ] {
            let (metadata, value) = encode(integer, scale);
            assert_eq!(
                EncodedVariant::from_bytes(&metadata, &value),
                Err(VariantViolation::NumericOutOfRange {
                    path: "/n/0".to_owned(),
                    numeric_kind: "decimal",
                }),
                "{integer} scale {scale}"
            );
        }
        let (metadata, value) = encode(i128::from(u64::MAX), 0);
        let accepted = EncodedVariant::from_bytes(&metadata, &value).expect("u64::MAX is exact");
        assert_eq!(
            variant_bytes_to_json(accepted.metadata(), accepted.value()).expect("renders"),
            json!({"n": [u64::MAX]})
        );

        let (metadata, decimal) = encode(5, 0);
        let value = list(&[nest(vec![0x00], 70), decimal]);
        assert_eq!(
            EncodedVariant::from_bytes(&metadata, &value),
            Err(VariantViolation::NumericOutOfRange {
                path: "/1/n/0".to_owned(),
                numeric_kind: "decimal",
            })
        );
    }

    /// Only the canonical extension with empty or absent metadata is a
    /// Variant.
    ///
    /// # Panics
    ///
    /// Panics when foreign extension metadata is accepted as a Variant.
    #[test]
    fn variant_extension_requires_empty_metadata() {
        let field = variant_field("v", false);
        assert!(is_variant(&field));
        let mut metadata = field.metadata().clone();
        metadata.remove(arrow_schema::extension::EXTENSION_TYPE_METADATA_KEY);
        assert!(is_variant(&field.clone().with_metadata(metadata.clone())));
        metadata.insert(
            arrow_schema::extension::EXTENSION_TYPE_METADATA_KEY.to_owned(),
            "{\"shredded\":true}".to_owned(),
        );
        assert!(!is_variant(&field.with_metadata(metadata)));
    }

    /// The column builder writes canonical storage that decodes per cell.
    ///
    /// # Panics
    ///
    /// Panics when a cell does not decode back to its JSON value.
    #[test]
    fn column_builder_round_trips_cells() {
        let mut builder = VariantColumnBuilder::with_capacity(2);
        builder.append(&EncodedVariant::from_json(&json!({"a": [1, 2]})).expect("encodes"));
        builder.append_null();
        let column = builder.finish();
        assert_eq!(column.data_type(), &variant_storage_type());
        assert_eq!(
            variant_cell_to_json(column.as_ref(), 0).expect("decodes"),
            json!({"a": [1, 2]})
        );
        assert_eq!(
            variant_cell_to_json(column.as_ref(), 1).expect("null"),
            Value::Null
        );
    }

    /// Column encoding names the field and row of the first refused value.
    ///
    /// # Panics
    ///
    /// Panics when the refusal loses its field or row, or a null drifts.
    #[test]
    fn column_encoding_names_the_refused_row() {
        let column = VariantColumnBuilder::encode(
            "detail",
            [Some("[1]"), None],
            EncodedVariant::from_json_text,
        )
        .expect("encodes");
        assert!(column.is_null(1));
        assert_eq!(
            VariantColumnBuilder::encode(
                "detail",
                [Some("[1]"), None, Some("{bad")],
                EncodedVariant::from_json_text,
            )
            .expect_err("refuses"),
            BifrostError::VariantInvalidJson {
                field: "detail".to_owned(),
                row: 2,
                path: String::new(),
            }
        );
    }

    /// Malformed stored Variant bytes are an error at every reader, not a panic.
    ///
    /// # Panics
    ///
    /// Panics when a reader accepts the malformed cell or upstream decoding
    /// panics on it.
    #[test]
    fn malformed_stored_variant_is_an_error_not_a_panic() {
        let column: ArrayRef = Arc::new(StructArray::new(
            variant_storage_fields(),
            vec![
                Arc::new(BinaryArray::from(vec![&[0xff_u8][..]])) as ArrayRef,
                Arc::new(BinaryArray::from(vec![&[0xff_u8][..]])) as ArrayRef,
            ],
            None,
        ));
        assert!(mask_placeholders(Arc::clone(&column)).is_err());
        assert!(variant_cell_to_json(column.as_ref(), 0).is_err());
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![variant_field("v", false)])),
            vec![column],
        )
        .expect("batch");
        let mut writer = WriterBuilder::new()
            .with_encoder_factory(Arc::new(VariantJsonEncoderFactory))
            .build::<_, JsonArray>(Vec::new());
        assert!(writer.write(&batch).is_err());
    }

    /// Arrow's JSON writer renders top-level and nested Variants as values.
    ///
    /// # Panics
    ///
    /// Panics when a Variant renders as its storage struct, a null drifts, or
    /// the placeholder under a null parent row fails the write.
    #[test]
    fn json_writer_renders_variants_as_values() {
        let column = |values: &[Option<Value>]| {
            VariantColumnBuilder::encode(
                "v",
                values.iter().map(Option::as_ref),
                EncodedVariant::from_json,
            )
            .expect("encodes")
        };
        let nested = StructArray::new(
            Fields::from(vec![
                Field::new("method", DataType::Utf8, false),
                variant_field("features", false),
            ]),
            vec![
                Arc::new(StringArray::from(vec!["psi", "psi"])) as ArrayRef,
                column(&[Some(json!({"k": 1, "score": 3.0})), Some(json!([1, "a"]))]),
            ],
            None,
        );
        let schema = Arc::new(Schema::new(vec![
            variant_field("v", true),
            Field::new("s", nested.data_type().clone(), false),
        ]));
        let batch = RecordBatch::try_new(
            schema,
            vec![
                column(&[
                    Some(json!({"big": 9_007_199_254_740_993_i64, "z": null})),
                    None,
                ]),
                Arc::new(nested),
            ],
        )
        .expect("batch");
        let mut writer = WriterBuilder::new()
            .with_explicit_nulls(true)
            .with_encoder_factory(Arc::new(VariantJsonEncoderFactory))
            .build::<_, JsonArray>(Vec::new());
        writer.write(&batch).expect("writes");
        writer.finish().expect("finishes");
        let rows: Value = serde_json::from_slice(&writer.into_inner()).expect("json");
        assert_eq!(
            rows,
            json!([
                {"v": {"big": 9_007_199_254_740_993_i64, "z": null}, "s": {"method": "psi", "features": {"k": 1, "score": 3.0}}},
                {"v": null, "s": {"method": "psi", "features": [1, "a"]}},
            ])
        );

        // A null parent row leaves its non-null Variant child an empty
        // placeholder, which the parent's null hides rather than fails on.
        let placeholder = StructArray::new(
            variant_storage_fields(),
            vec![
                Arc::new(BinaryArray::from(vec![&b""[..]])) as ArrayRef,
                Arc::new(BinaryArray::from(vec![&b""[..]])) as ArrayRef,
            ],
            None,
        );
        // Projected alone, as a published `get_field` returns it, the same
        // placeholder is a valid slot that must still render as JSON null.
        let projected = RecordBatch::try_new(
            Arc::new(Schema::new(vec![variant_field("f", false)])),
            vec![Arc::new(placeholder.clone()) as ArrayRef],
        )
        .expect("projected batch");
        let mut writer = WriterBuilder::new()
            .with_explicit_nulls(true)
            .with_encoder_factory(Arc::new(VariantJsonEncoderFactory))
            .build::<_, JsonArray>(Vec::new());
        writer.write(&projected).expect("a placeholder renders");
        writer.finish().expect("finishes");
        let rows: Value = serde_json::from_slice(&writer.into_inner()).expect("json");
        assert_eq!(rows, json!([{"f": null}]));

        let report = StructArray::new(
            Fields::from(vec![variant_field("features", false)]),
            vec![Arc::new(placeholder) as ArrayRef],
            Some(NullBuffer::from(vec![false])),
        );
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "r",
                report.data_type().clone(),
                true,
            )])),
            vec![Arc::new(report)],
        )
        .expect("batch");
        let mut writer = WriterBuilder::new()
            .with_explicit_nulls(true)
            .with_encoder_factory(Arc::new(VariantJsonEncoderFactory))
            .build::<_, JsonArray>(Vec::new());
        writer
            .write(&batch)
            .expect("a null parent hides its placeholder child");
        writer.finish().expect("finishes");
        let rows: Value = serde_json::from_slice(&writer.into_inner()).expect("json");
        assert_eq!(rows, json!([{"r": null}]));
    }
}
