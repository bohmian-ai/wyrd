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
//!   narrowest width, a larger integer is a scale-zero Variant decimal, and a
//!   non-integer number is a double;
//! - an object key whose value is JSON `null` is stored as a Variant null, so
//!   it stays distinct from an absent key;
//! - a value nesting deeper than [`VARIANT_MAX_DEPTH`] containers, or encoding
//!   to more than [`VARIANT_MAX_ENCODED_BYTES`] bytes, is refused.
//!
//! [`VariantColumnBuilder`] assembles encoded values into the canonical
//! unshredded Arrow storage: a struct of non-null `metadata` and `value`
//! binary children under the `arrow.parquet.variant` extension.

use std::collections::HashMap;
use std::sync::Arc;

use arrow::array::{
    Array, ArrayRef, BinaryArray, BinaryBuilder, BinaryViewArray, LargeBinaryArray, StructArray,
};
use arrow::buffer::NullBuffer;
use arrow::error::ArrowError;
use arrow::json::writer::{Encoder, EncoderFactory, EncoderOptions, NullableEncoder};
use arrow_schema::extension::{EXTENSION_TYPE_METADATA_KEY, EXTENSION_TYPE_NAME_KEY};
use arrow_schema::{DataType, Field, FieldRef, Fields};
use parquet_variant::{
    BuilderSpecificState, ListBuilder, ObjectFieldBuilder, Variant, VariantBuilder,
    VariantBuilderExt, VariantDecimal16,
};
use parquet_variant_json::VariantToJson;
use serde_json::{Number, Value};
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{VARIANT_EXTENSION_NAME, VARIANT_MAX_DEPTH, VARIANT_MAX_ENCODED_BYTES};

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
    /// # Errors
    ///
    /// Returns [`VariantViolation::InvalidJson`] at the root when the text is
    /// not JSON, then every [`Self::from_json`] error.
    pub fn from_json_text(text: &str) -> Result<Self, VariantViolation> {
        let value: Value =
            serde_json::from_str(text).map_err(|_| VariantViolation::InvalidJson {
                path: String::new(),
            })?;
        Self::from_json(&value)
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
/// Accepts any storage the `arrow.parquet.variant` extension allows for an
/// unshredded value: a struct whose `metadata` and `value` children are
/// `Binary`, `LargeBinary`, or `BinaryView`. A null cell renders as JSON null.
///
/// # Errors
///
/// Returns [`VariantViolation::InvalidJson`] when the column is not an
/// unshredded Variant storage struct or the cell does not decode.
pub fn variant_cell_to_json(column: &dyn Array, row: usize) -> Result<Value, VariantViolation> {
    let invalid = || VariantViolation::InvalidJson {
        path: String::new(),
    };
    if column.is_null(row) {
        return Ok(Value::Null);
    }
    let storage = column
        .as_any()
        .downcast_ref::<StructArray>()
        .ok_or_else(invalid)?;
    let metadata = binary_cell(storage.column_by_name("metadata").ok_or_else(invalid)?, row)
        .ok_or_else(invalid)?;
    let value = binary_cell(storage.column_by_name("value").ok_or_else(invalid)?, row)
        .ok_or_else(invalid)?;
    variant_bytes_to_json(metadata, value)
}

/// Renders Arrow Variant columns as their JSON value in Arrow's JSON writer.
///
/// Arrow's JSON writer would otherwise print a Variant as its storage struct
/// of `metadata`/`value` bytes. Installing this factory on a writer's
/// [`EncoderOptions`] makes every field
/// carrying the `arrow.parquet.variant` extension — top level or nested in a
/// Struct or List — render through [`variant_cell_to_json`], so every
/// row-as-JSON surface shares one rendering. Other fields keep Arrow's
/// default encoders.
#[derive(Debug, Default)]
pub struct VariantJsonEncoderFactory;

impl EncoderFactory for VariantJsonEncoderFactory {
    /// Pre-render every non-null cell of a Variant field as JSON text.
    ///
    /// Rendering is done up front because the encoder itself cannot fail. A
    /// Variant nested in a Struct sees only its own nulls, so the slot under a
    /// null parent row holds empty `metadata` and `value` placeholders; that
    /// slot is never encoded and renders nothing instead of failing.
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
        if field.extension_type_name() != Some(VARIANT_EXTENSION_NAME) {
            return Ok(None);
        }
        let rendered = (0..array.len())
            .map(|row| {
                if array.is_null(row) || is_placeholder(array, row) {
                    return Ok(String::new());
                }
                variant_cell_to_json(array, row)
                    .map(|value| value.to_string())
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

/// Reports whether one Variant cell is an empty-storage placeholder.
///
/// Every encoded Variant has at least one `metadata` and one `value` byte, so
/// a cell whose two children are both empty carries no value at all. Such a
/// cell appears under a null parent struct, whose null is not pushed down into
/// the child storage, so every Variant reader treats it as SQL null.
pub fn is_placeholder(column: &dyn Array, row: usize) -> bool {
    let empty = |name: &str| {
        column
            .as_any()
            .downcast_ref::<StructArray>()
            .and_then(|storage| storage.column_by_name(name))
            .and_then(|child| binary_cell(child, row))
            .is_some_and(<[u8]>::is_empty)
    };
    empty("metadata") && empty("value")
}

/// Pre-rendered JSON text of each row of one Variant column.
struct RenderedJson(Vec<String>);

impl Encoder for RenderedJson {
    /// Copy row `idx`'s rendered JSON into the writer's buffer.
    fn encode(&mut self, idx: usize, out: &mut Vec<u8>) {
        out.extend_from_slice(self.0[idx].as_bytes());
    }
}

/// Return one binary child cell regardless of its offset width or view form.
fn binary_cell(array: &ArrayRef, row: usize) -> Option<&[u8]> {
    if array.is_null(row) {
        return None;
    }
    let any = array.as_any();
    any.downcast_ref::<BinaryArray>()
        .map(|values| values.value(row))
        .or_else(|| {
            any.downcast_ref::<LargeBinaryArray>()
                .map(|values| values.value(row))
        })
        .or_else(|| {
            any.downcast_ref::<BinaryViewArray>()
                .map(|values| values.value(row))
        })
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
    Field::new(name, variant_storage_type(), nullable).with_metadata(HashMap::from([
        (
            EXTENSION_TYPE_NAME_KEY.to_owned(),
            VARIANT_EXTENSION_NAME.to_owned(),
        ),
        (EXTENSION_TYPE_METADATA_KEY.to_owned(), String::new()),
    ]))
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
        return VariantDecimal16::try_new(i128::from(unsigned), 0)
            .map(Variant::from)
            .map_err(|_| VariantViolation::NumericOutOfRange {
                path: path.render(),
                numeric_kind: "integer",
            });
    }
    number
        .as_f64()
        .filter(|double| double.is_finite())
        .map(Variant::from)
        .ok_or_else(|| VariantViolation::NumericOutOfRange {
            path: path.render(),
            numeric_kind: "double",
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{RecordBatch, StringArray};
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
