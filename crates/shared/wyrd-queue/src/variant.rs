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
    BuilderSpecificState, ListBuilder, ObjectFieldBuilder, Variant, VariantBuilder,
    VariantBuilderExt,
};
use parquet_variant_compute::{VariantArray, VariantType};
use parquet_variant_json::VariantToJson;
use serde_json::value::RawValue;
use serde_json::{Number, Value};
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

/// One canonical encoded Variant value: its metadata and value bytes.
///
/// Construction always enforces the Bifrost limits, so holding an
/// `EncodedVariant` means the value is storable.
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
    /// The value is walked once into a `parquet-variant` builder; depth is
    /// checked as each container opens, and size once the encoding finishes.
    ///
    /// # Errors
    ///
    /// Returns [`VariantViolation::TooDeep`] for a container past the depth
    /// limit and [`VariantViolation::TooLarge`] when the encoding exceeds the
    /// size limit.
    pub fn from_json(value: &Value) -> Result<Self, VariantViolation> {
        let mut builder = VariantBuilder::new();
        let mut path = JsonPointer::default();
        append_json(&mut builder, value, &mut path, 0)?;
        let (metadata, value) = builder.finish();
        Self::sized(metadata, value)
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
        append_raw(&mut builder, raw, &mut path, 0)?;
        let (metadata, value) = builder.finish();
        Self::sized(metadata, value)
    }

    /// Accept already-encoded Variant bytes after full validation.
    ///
    /// Checks run in the locked order: size first, then encoding validity,
    /// then depth.
    ///
    /// # Errors
    ///
    /// Returns [`VariantViolation::TooLarge`], [`VariantViolation::InvalidJson`]
    /// for bytes that are not a valid Variant, or [`VariantViolation::TooDeep`].
    pub fn from_bytes(metadata: &[u8], value: &[u8]) -> Result<Self, VariantViolation> {
        let encoded = Self::sized(metadata.to_vec(), value.to_vec())?;
        let variant =
            Variant::try_new(metadata, value).map_err(|_| VariantViolation::InvalidJson {
                path: String::new(),
            })?;
        check_depth(&variant, &mut JsonPointer::default(), 0)?;
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

    /// Render the value as JSON.
    ///
    /// Integers stay JSON integers, decimals render as JSON numbers, and the
    /// non-JSON scalar types (binary, dates, timestamps, UUIDs) render as the
    /// strings `parquet-variant-json` defines.
    ///
    /// # Errors
    ///
    /// Returns [`VariantViolation::InvalidJson`] when the bytes do not decode
    /// or hold a non-finite float JSON cannot express.
    pub fn to_json(&self) -> Result<Value, VariantViolation> {
        variant_bytes_to_json(&self.metadata, &self.value)
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
/// Struct or List — render through upstream [`VariantArray`], so every
/// row-as-JSON surface shares one rendering. Other fields keep Arrow's
/// default encoders.
#[derive(Debug, Default)]
pub struct VariantJsonEncoderFactory;

impl EncoderFactory for VariantJsonEncoderFactory {
    /// Pre-render every non-null cell of a Variant field as JSON text.
    ///
    /// Rendering is done up front because the encoder itself cannot fail.
    /// Placeholder slots under a null parent row are masked first, so they
    /// render nothing instead of failing.
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
                    .and_then(|value| value.to_json_string())
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
            array.logical_nulls(),
        )))
    }
}

/// Mark every empty-storage placeholder row of a Variant column as null.
///
/// Every encoded Variant has at least one `metadata` and one `value` byte, so
/// a cell whose two children are both empty carries no value at all. Such a
/// cell appears under a null parent struct, whose null is neither pushed down
/// by the Parquet reader nor by `get_field`, and the upstream decoder panics
/// on it, so every reader masks the column here before decoding. A non-struct
/// array is returned unchanged.
///
/// # Errors
///
/// Returns the Arrow error raised while viewing a child as bytes or
/// reassembling the masked struct.
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
    let empty = |child: &Option<ArrayRef>, row: usize| {
        child
            .as_ref()
            .is_some_and(|child| child.as_binary_view().value(row).is_empty())
    };
    let present = (0..storage.len())
        .map(|row| storage.is_valid(row) && !(empty(&metadata, row) && empty(&value, row)));
    let (fields, children, _) = columns.clone().into_parts();
    Ok(Arc::new(StructArray::try_new(
        fields,
        children,
        Some(present.collect::<NullBuffer>()),
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

/// Report whether a field carries the `arrow.parquet.variant` extension.
///
/// The extension name, not the storage struct, is what makes a column a
/// Variant: a user Struct with `metadata`/`value` children stays a Struct.
#[must_use]
pub fn is_variant(field: &Field) -> bool {
    field.extension_type_name() == Some(VariantType::NAME)
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

/// RFC 6901 JSON Pointer under construction during a walk.
#[derive(Debug, Default)]
struct JsonPointer {
    /// Escaped reference tokens from the root.
    tokens: Vec<String>,
}

impl JsonPointer {
    /// Descend into one object key, escaping `~` and `/`.
    fn push_key(&mut self, key: &str) {
        self.tokens.push(key.replace('~', "~0").replace('/', "~1"));
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

/// Fail when opening a container at `depth` would pass the limit.
///
/// # Errors
///
/// Returns [`VariantViolation::TooDeep`] naming the container's pointer.
fn enter_container(depth: u32, path: &JsonPointer) -> Result<u32, VariantViolation> {
    let depth = depth + 1;
    if depth > VARIANT_MAX_DEPTH {
        return Err(VariantViolation::TooDeep {
            path: path.render(),
            depth,
        });
    }
    Ok(depth)
}

/// Append one JSON value to any Variant builder position.
///
/// `depth` is the number of containers already open above this value.
///
/// # Errors
///
/// Returns [`VariantViolation::TooDeep`] for a container past the limit and
/// [`VariantViolation::NumericOutOfRange`] for an unrepresentable number.
fn append_json(
    builder: &mut impl VariantBuilderExt,
    value: &Value,
    path: &mut JsonPointer,
    depth: u32,
) -> Result<(), VariantViolation> {
    match value {
        Value::Null => builder.append_value(Variant::Null),
        Value::Bool(flag) => builder.append_value(*flag),
        Value::Number(number) => builder.append_value(number_variant(number, path)?),
        Value::String(text) => builder.append_value(text.as_str()),
        Value::Array(items) => {
            let depth = enter_container(depth, path)?;
            let mut list = builder.try_new_list().map_err(|_| invalid(path))?;
            append_items(&mut list, items, path, depth)?;
            list.finish();
        }
        Value::Object(entries) => {
            let depth = enter_container(depth, path)?;
            let mut object = builder.try_new_object().map_err(|_| invalid(path))?;
            for (key, child) in entries {
                path.push_key(key);
                if child.is_null() {
                    // A field builder would treat null as an absent key.
                    object.insert(key, Variant::Null);
                } else {
                    append_json(
                        &mut ObjectFieldBuilder::new(key, &mut object),
                        child,
                        path,
                        depth,
                    )?;
                }
                path.pop();
            }
            object.finish();
        }
    }
    Ok(())
}

/// Append one validated raw JSON value to any Variant builder position.
///
/// The token's first byte selects its kind; containers are split into raw
/// children and walked recursively, and numbers go to [`raw_number_variant`]
/// with their original text. `depth` is the number of containers already
/// open above this value.
///
/// # Errors
///
/// Returns [`VariantViolation::TooDeep`] for a container past the limit,
/// [`VariantViolation::NumericOutOfRange`] for an unrepresentable number, and
/// [`VariantViolation::InvalidJson`] when a builder refuses a container.
fn append_raw(
    builder: &mut impl VariantBuilderExt,
    raw: &RawValue,
    path: &mut JsonPointer,
    depth: u32,
) -> Result<(), VariantViolation> {
    let text = raw.get();
    match text.as_bytes().first() {
        Some(b'{') => {
            let depth = enter_container(depth, path)?;
            // A map keeps the final occurrence of a repeated key.
            // ponytail: each level re-scans its subtree, so cost is bounded by
            // depth (64) times size; a streaming visitor if parse_json profiles hot.
            let entries: BTreeMap<String, &RawValue> =
                serde_json::from_str(text).map_err(|_| invalid(path))?;
            let mut object = builder.try_new_object().map_err(|_| invalid(path))?;
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
                    )?;
                }
                path.pop();
            }
            object.finish();
        }
        Some(b'[') => {
            let depth = enter_container(depth, path)?;
            let items: Vec<&RawValue> = serde_json::from_str(text).map_err(|_| invalid(path))?;
            let mut list = builder.try_new_list().map_err(|_| invalid(path))?;
            for (index, item) in items.into_iter().enumerate() {
                path.push_index(index);
                append_raw(&mut list, item, path, depth)?;
                path.pop();
            }
            list.finish();
        }
        Some(b'"') => {
            let string: String = serde_json::from_str(text).map_err(|_| invalid(path))?;
            builder.append_value(string.as_str());
        }
        Some(b't') => builder.append_value(true),
        Some(b'f') => builder.append_value(false),
        Some(b'n') => builder.append_value(Variant::Null),
        _ => builder.append_value(raw_number_variant(text, path)?),
    }
    Ok(())
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

/// Append every array item to an open list builder.
///
/// # Errors
///
/// Propagates every [`append_json`] failure.
fn append_items<S: BuilderSpecificState>(
    list: &mut ListBuilder<'_, S>,
    items: &[Value],
    path: &mut JsonPointer,
    depth: u32,
) -> Result<(), VariantViolation> {
    for (index, item) in items.iter().enumerate() {
        path.push_index(index);
        append_json(list, item, path, depth)?;
        path.pop();
    }
    Ok(())
}

/// Return an invalid-JSON violation at the current pointer.
fn invalid(path: &JsonPointer) -> VariantViolation {
    VariantViolation::InvalidJson {
        path: path.render(),
    }
}

/// Convert one JSON number under the Bifrost numeric rules.
///
/// # Errors
///
/// Returns [`VariantViolation::NumericOutOfRange`] when the number is neither
/// a 64-bit integer, an unsigned integer a decimal holds, nor a finite double.
fn number_variant(
    number: &Number,
    path: &JsonPointer,
) -> Result<Variant<'static, 'static>, VariantViolation> {
    if let Some(integer) = number.as_i64() {
        return Ok(narrow_integer(integer));
    }
    if let Some(unsigned) = number.as_u64() {
        return Ok(Variant::from(unsigned));
    }
    number
        .as_f64()
        .filter(|double| double.is_finite())
        .map(Variant::from)
        .ok_or_else(|| out_of_range(path, "double"))
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

/// Walk a decoded Variant and fail past the depth limit.
///
/// # Errors
///
/// Returns [`VariantViolation::TooDeep`] for a container past the limit and
/// [`VariantViolation::InvalidJson`] for a child that does not decode.
fn check_depth(
    variant: &Variant<'_, '_>,
    path: &mut JsonPointer,
    depth: u32,
) -> Result<(), VariantViolation> {
    match variant {
        Variant::Object(object) => {
            let depth = enter_container(depth, path)?;
            for entry in object.iter_try() {
                let (key, child) = entry.map_err(|_| invalid(path))?;
                path.push_key(key);
                check_depth(&child, path, depth)?;
                path.pop();
            }
        }
        Variant::List(list) => {
            let depth = enter_container(depth, path)?;
            for (index, child) in list.iter_try().enumerate() {
                path.push_index(index);
                check_depth(&child.map_err(|_| invalid(path))?, path, depth)?;
                path.pop();
            }
        }
        _ => {}
    }
    Ok(())
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
            encoded.to_json().expect("json")["big"],
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
        let rendered = encoded.to_json().expect("json");
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

    /// Arrow's JSON writer renders top-level and nested Variants as values.
    ///
    /// # Panics
    ///
    /// Panics when a Variant renders as its storage struct, a null drifts, or
    /// the placeholder under a null parent row fails the write.
    #[test]
    fn json_writer_renders_variants_as_values() {
        let column = |values: &[Option<Value>]| {
            let mut builder = VariantColumnBuilder::with_capacity(values.len());
            for value in values {
                match value {
                    Some(value) => {
                        builder.append(&EncodedVariant::from_json(value).expect("encodes"));
                    }
                    None => builder.append_null(),
                }
            }
            builder.finish()
        };
        let nested = StructArray::new(
            Fields::from(vec![
                Field::new("method", DataType::Utf8, false),
                variant_field("features", false),
            ]),
            vec![
                Arc::new(StringArray::from(vec!["psi", "psi"])) as ArrayRef,
                column(&[Some(json!({"k": 1})), Some(json!([1, "a"]))]),
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
                {"v": {"big": 9_007_199_254_740_993_i64, "z": null}, "s": {"method": "psi", "features": {"k": 1}}},
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
