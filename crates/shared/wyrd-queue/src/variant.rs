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
use std::ops::Range;
use std::sync::Arc;

use arrow::array::{Array, ArrayRef, AsArray, BinaryBuilder, StructArray, make_array};
use arrow::buffer::NullBuffer;
use arrow::compute::cast;
use arrow::error::ArrowError;
use arrow::json::writer::{Encoder, EncoderFactory, EncoderOptions, NullableEncoder};
use arrow_schema::extension::ExtensionType;
use arrow_schema::{DataType, Field, FieldRef, Fields};
use parquet_variant::{
    ObjectFieldBuilder, Variant, VariantBuilder, VariantBuilderExt, VariantMetadata, VariantObject,
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
    /// Serialization recurses once per nesting level, so a programmatic value
    /// nested far past [`VARIANT_MAX_DEPTH`] could exhaust the stack before the
    /// walker refuses it. A depth-bounded check runs first; a value past the
    /// limit is serialized from a copy whose containers at depth 65 are
    /// emptied. The walker never builds those containers and a `Value` holds
    /// no number outside the Variant domain, so the copy yields the same
    /// error, pointer, and built bytes as the original.
    ///
    /// # Errors
    ///
    /// Returns [`VariantViolation::TooLarge`] when the encoding exceeds the
    /// size limit and [`VariantViolation::TooDeep`] for a container past the
    /// depth limit.
    pub fn from_json(value: &Value) -> Result<Self, VariantViolation> {
        if nests_past_limit(value, 0) {
            Self::from_json_text(&within_depth_limit(value, 0).to_string())
        } else {
            Self::from_json_text(&value.to_string())
        }
    }

    /// Parse JSON text and encode it under the Bifrost Variant rules.
    ///
    /// Unlike [`Self::from_json`], numbers are classified from their original
    /// tokens, so an integer beyond the 64-bit range keeps its exact digits
    /// instead of passing through `f64`. The text is validated once as a
    /// [`RawValue`], then split once into a [`JsonTree`] whose values the walk
    /// visits once each; an object repeating a key keeps its final
    /// occurrence. `serde_json` checks a raw value's syntax with an explicit
    /// stack and no depth limit, so syntax is decided at any depth, nesting is
    /// decided only by the Wyrd walk, and the work is proportional to the
    /// text however deep it nests.
    ///
    /// The accepted part of the value is built before any recorded violation
    /// is selected, so bytes that already exceed the size limit report size
    /// first. A refused number or a container past the depth limit is not
    /// built and adds nothing to that size.
    ///
    /// # Errors
    ///
    /// Returns [`VariantViolation::InvalidJson`] at the root when the text is
    /// not JSON, then [`VariantViolation::TooLarge`] when the built encoding
    /// exceeds the size limit, [`VariantViolation::NumericOutOfRange`] for a
    /// number no Variant numeric type holds, and [`VariantViolation::TooDeep`]
    /// for a container past the depth limit.
    pub fn from_json_text(text: &str) -> Result<Self, VariantViolation> {
        let mut path = JsonPointer::default();
        let raw: &RawValue = serde_json::from_str(text).map_err(|_| invalid(&path))?;
        let tree = JsonTree::split(raw.get());
        let mut builder = VariantBuilder::new();
        let mut found = VariantViolations::default();
        append_raw(&mut builder, &tree, 0, &mut path, 0, &mut found);
        // A refused root number leaves nothing built; its violation is recorded.
        let built = builder.try_finish();
        if let Ok((metadata, value)) = &built {
            Self::within_size_limit(metadata.len() + value.len())?;
        }
        found.finish()?;
        let (metadata, value) = built.map_err(|_| invalid(&path))?;
        Ok(Self { metadata, value })
    }

    /// Accept already-encoded Variant bytes after full validation.
    ///
    /// The bytes pass [`Self::validate`], the one raw Variant gate, and are
    /// then copied.
    ///
    /// # Errors
    ///
    /// Returns [`VariantViolation::TooLarge`] past the size limit, then
    /// [`VariantViolation::InvalidJson`] for bytes that are not a valid
    /// canonical Variant, [`VariantViolation::NumericOutOfRange`] for a number
    /// outside the exact domain, and [`VariantViolation::TooDeep`] for a
    /// container past the depth limit.
    pub fn from_bytes(metadata: &[u8], value: &[u8]) -> Result<Self, VariantViolation> {
        Self::validate(metadata, value)?;
        Ok(Self {
            metadata: metadata.to_vec(),
            value: value.to_vec(),
        })
    }

    /// Check borrowed Variant bytes against every Bifrost rule without copying
    /// and return the validated upstream value.
    ///
    /// Every raw entry point — [`Self::from_bytes`] and each stored-Variant
    /// renderer — calls this before upstream's recursive validation or
    /// rendering; a renderer consumes the returned value, so the bytes are
    /// fully validated once. Size is checked first, then the metadata is
    /// parsed once and shared by the scan and upstream validation. Then
    /// [`scan_encoded`] visits every
    /// node of the size-bounded value with an explicit stack, recording
    /// malformed or non-canonical encoding, numbers outside the exact domain,
    /// and the first container past [`VARIANT_MAX_DEPTH`]. Upstream's
    /// recursive [`Variant::try_new`] recurses once per nesting level, and a
    /// compact hostile value can nest far deeper than any stack allows, so it
    /// runs only when no depth violation was found; it remains the encoding
    /// authority for every accepted value and builds the returned value. The
    /// shallow accessors the scan
    /// uses panic on malformed bytes, which is contained here and reported as
    /// invalid. The recorded violations then select one error, malformed
    /// first.
    ///
    /// # Errors
    ///
    /// Returns the same violations, in the same order, as [`Self::from_bytes`].
    fn validate<'m, 'v>(
        metadata: &'m [u8],
        value: &'v [u8],
    ) -> Result<Variant<'m, 'v>, VariantViolation> {
        Self::within_size_limit(metadata.len() + value.len())?;
        let root = JsonPointer::default();
        let metadata = VariantMetadata::try_new(metadata).map_err(|_| invalid(&root))?;
        let mut found = std::panic::catch_unwind(|| scan_encoded(metadata.clone(), value))
            .unwrap_or_else(|_| {
                let mut found = VariantViolations::default();
                found.record(invalid(&root));
                found
            });
        let mut checked = None;
        if found.malformed.is_none() && found.depth.is_none() {
            match Variant::try_new_with_metadata(metadata, value) {
                Ok(variant) => checked = Some(variant),
                Err(_) => found.record(invalid(&root)),
            }
        }
        found.finish()?;
        checked.ok_or_else(|| invalid(&root))
    }

    /// Refuse an encoding of `bytes` metadata plus value bytes past the limit.
    ///
    /// # Errors
    ///
    /// Returns [`VariantViolation::TooLarge`] past [`VARIANT_MAX_ENCODED_BYTES`].
    fn within_size_limit(bytes: usize) -> Result<(), VariantViolation> {
        let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
        if bytes > VARIANT_MAX_ENCODED_BYTES {
            return Err(VariantViolation::TooLarge { bytes });
        }
        Ok(())
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
/// language's native open value. The bytes pass the same bounded gate as
/// [`EncodedVariant::from_bytes`], which returns the validated value that is
/// rendered, so a hostile stored value is an error rather than unbounded work
/// and an accepted one is validated once.
///
/// # Errors
///
/// Returns the [`EncodedVariant::from_bytes`] violation when the bytes break
/// a Bifrost Variant rule, and [`VariantViolation::InvalidJson`] when an
/// accepted value still fails to render.
pub fn variant_bytes_to_json(metadata: &[u8], value: &[u8]) -> Result<Value, VariantViolation> {
    EncodedVariant::validate(metadata, value)?
        .to_json_value()
        .map_err(|_| VariantViolation::InvalidJson {
            path: String::new(),
        })
}

/// Render one Arrow Variant column cell as JSON.
///
/// Decodes through upstream [`VariantArray`], which accepts any storage the
/// `arrow.parquet.variant` extension allows for an unshredded value: a struct
/// whose `metadata` and `value` children are `Binary`, `LargeBinary`, or
/// `BinaryView`. A null or placeholder cell renders as JSON null. The cell
/// passes the bounded gate in [`mask_placeholders`] before it is decoded.
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
/// other present cell then passes the same bounded gate as
/// [`EncodedVariant::from_bytes`]: upstream decoding validates shallowly,
/// panics on malformed bytes, and recurses once per nesting level, so every
/// reader passes the column through here first and a malformed, non-canonical,
/// or hostile stored cell becomes an error instead. A non-struct array is
/// returned unchanged.
///
/// # Errors
///
/// Returns the Arrow error raised while viewing a child as bytes or
/// reassembling the masked struct, and [`ArrowError::InvalidArgumentError`]
/// naming the row and violation of the first present cell that breaks a
/// Bifrost Variant rule.
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
            EncodedVariant::validate(metadata, value).map_err(|violation| {
                ArrowError::InvalidArgumentError(format!(
                    "row {row} is not a valid stored Variant: {violation:?}"
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

#[cfg(test)]
thread_local! {
    /// Bytes of JSON text the Variant walk has read, so tests can show each
    /// byte is read at most once.
    static WALK_READ_BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// One value inside syntax-validated JSON text.
#[derive(Debug)]
struct JsonNode {
    /// Byte range of the whole value in the text.
    span: Range<usize>,
    /// Members in text order: an object member's key range, or `None` for a
    /// list item, and the member's node index.
    members: Vec<(Option<Range<usize>>, usize)>,
}

/// Syntax-validated JSON text split into its values in one pass.
///
/// `serde_json` stays the only syntax authority: the text has already been
/// accepted as a [`RawValue`], so splitting only has to find where each
/// value, key, and container starts and ends. The Variant walk then visits
/// each value once through its node instead of re-parsing every container's
/// text, so its work is proportional to the text at any depth. Keys and
/// leaves are decoded by `serde_json` and [`raw_number_variant`] from their
/// own disjoint spans.
#[derive(Debug)]
struct JsonTree<'t> {
    /// The validated text.
    text: &'t str,
    /// Every value in text order; the root is node 0.
    nodes: Vec<JsonNode>,
}

impl<'t> JsonTree<'t> {
    /// Split validated JSON text into nodes with one left-to-right scan.
    ///
    /// Strings are skipped by their escapes, so a structural byte inside a
    /// string never opens or closes anything. Inside an object, a string
    /// with no pending key is that member's key; every other value becomes a
    /// node attached to the innermost open container.
    fn split(text: &'t str) -> Self {
        let bytes = text.as_bytes();
        let mut tree = Self {
            text,
            nodes: Vec::new(),
        };
        let mut open: Vec<usize> = Vec::new();
        let mut key = None;
        let mut at = 0;
        while let Some(&byte) = bytes.get(at) {
            let start = at;
            at += 1;
            match byte {
                b'{' | b'[' => open.push(tree.attach(start..start, &open, &mut key)),
                b'}' | b']' => {
                    if let Some(node) = open.pop() {
                        tree.nodes[node].span.end = at;
                    }
                }
                b'"' => {
                    while let Some(&byte) = bytes.get(at) {
                        at += match byte {
                            b'"' => break,
                            b'\\' => 2,
                            _ => 1,
                        };
                    }
                    at += 1;
                    let in_object = open
                        .last()
                        .is_some_and(|&node| tree.kind(node) == Some(b'{'));
                    if in_object && key.is_none() {
                        key = Some(start..at);
                    } else {
                        tree.attach(start..at, &open, &mut key);
                    }
                }
                b',' | b':' | b' ' | b'\t' | b'\n' | b'\r' => {}
                _ => {
                    while bytes
                        .get(at)
                        .is_some_and(|byte| !b",]} \t\n\r".contains(byte))
                    {
                        at += 1;
                    }
                    tree.attach(start..at, &open, &mut key);
                }
            }
        }
        tree
    }

    /// Add a value spanning `span` under the innermost open container,
    /// consuming the pending object key, and return its node index.
    fn attach(
        &mut self,
        span: Range<usize>,
        open: &[usize],
        key: &mut Option<Range<usize>>,
    ) -> usize {
        let node = self.nodes.len();
        self.nodes.push(JsonNode {
            span,
            members: Vec::new(),
        });
        if let Some(&parent) = open.last() {
            self.nodes[parent].members.push((key.take(), node));
        }
        node
    }

    /// Return the first byte of a value, which selects its kind.
    fn kind(&self, node: usize) -> Option<u8> {
        self.text
            .as_bytes()
            .get(self.nodes[node].span.start)
            .copied()
    }

    /// Return the text of one scalar value or key.
    fn read(&self, span: Range<usize>) -> &'t str {
        #[cfg(test)]
        WALK_READ_BYTES.with(|read| read.set(read.get() + span.len()));
        &self.text[span]
    }

    /// Return the text of one scalar value.
    fn token(&self, node: usize) -> &'t str {
        self.read(self.nodes[node].span.clone())
    }

    /// Return an object's members by decoded key, keeping the final
    /// occurrence of a repeated key, in sorted key order.
    ///
    /// Returns `None` when a key does not decode, which validated text never
    /// produces.
    fn entries(&self, node: usize) -> Option<BTreeMap<String, usize>> {
        let mut entries = BTreeMap::new();
        for (key, child) in &self.nodes[node].members {
            let key = serde_json::from_str::<String>(self.read(key.clone()?)).ok()?;
            entries.insert(key, *child);
        }
        Some(entries)
    }

    /// Return a list's items in order.
    fn items(&self, node: usize) -> impl Iterator<Item = usize> + '_ {
        self.nodes[node].members.iter().map(|(_, item)| *item)
    }
}

/// Append one JSON value to any Variant builder position.
///
/// The value's first byte selects its kind; containers are walked
/// recursively through their [`JsonTree`] members and numbers go to
/// [`raw_number_variant`] with their original text. `depth` is the number of
/// containers already open above this value.
///
/// Every violation is recorded in `found` rather than returned, so the walk
/// always covers the whole value and [`VariantViolations::finish`] alone
/// picks the error. Neither a refused number nor a container past the depth
/// limit is built, so neither adds bytes to the size check: [`scan_numbers`]
/// searches the deep container without recursion, so an out-of-range number
/// below the limit still outranks depth. Recursion therefore never passes the
/// limit.
fn append_raw(
    builder: &mut impl VariantBuilderExt,
    tree: &JsonTree<'_>,
    node: usize,
    path: &mut JsonPointer,
    depth: u32,
    found: &mut VariantViolations,
) {
    let kind = tree.kind(node);
    let depth = if matches!(kind, Some(b'{' | b'[')) {
        let (depth, within) = enter_container(depth, path, found);
        if !within {
            scan_numbers(tree, node, path, found);
            return;
        }
        depth
    } else {
        depth
    };
    match kind {
        Some(b'{') => {
            let Some(entries) = tree.entries(node) else {
                return found.record(invalid(path));
            };
            let Ok(mut object) = builder.try_new_object() else {
                return found.record(invalid(path));
            };
            for (key, child) in entries {
                path.push_key(&key);
                if tree.kind(child) == Some(b'n') {
                    // A field builder would treat null as an absent key.
                    object.insert(&key, Variant::Null);
                } else {
                    append_raw(
                        &mut ObjectFieldBuilder::new(&key, &mut object),
                        tree,
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
            let Ok(mut list) = builder.try_new_list() else {
                return found.record(invalid(path));
            };
            for (index, item) in tree.items(node).enumerate() {
                path.push_index(index);
                append_raw(&mut list, tree, item, path, depth, found);
                path.pop();
            }
            list.finish();
        }
        Some(b'"') => match serde_json::from_str::<String>(tree.token(node)) {
            Ok(string) => builder.append_value(string.as_str()),
            Err(_) => found.record(invalid(path)),
        },
        Some(b't') => builder.append_value(true),
        Some(b'f') => builder.append_value(false),
        Some(b'n') => builder.append_value(Variant::Null),
        _ => match raw_number_variant(tree.token(node), path) {
            Ok(number) => builder.append_value(number),
            Err(violation) => found.record(violation),
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
fn scan_numbers(
    tree: &JsonTree<'_>,
    root: usize,
    path: &mut JsonPointer,
    found: &mut VariantViolations,
) {
    let base = path.tokens.len();
    // Each pending value carries its parent's pointer depth and its token.
    let mut pending: Vec<(usize, Option<String>, usize)> = vec![(base, None, root)];
    while found.numeric.is_none() {
        let Some((parent, token, node)) = pending.pop() else {
            break;
        };
        path.truncate(parent);
        if let Some(token) = token {
            path.tokens.push(token);
        }
        let here = path.tokens.len();
        let start = pending.len();
        match tree.kind(node) {
            Some(b'{') => match tree.entries(node) {
                Some(entries) => pending.extend(
                    entries
                        .into_iter()
                        .map(|(key, child)| (here, Some(JsonPointer::escape(&key)), child)),
                ),
                None => found.record(invalid(path)),
            },
            Some(b'[') => pending.extend(
                tree.items(node)
                    .enumerate()
                    .map(|(index, item)| (here, Some(index.to_string()), item)),
            ),
            Some(b'"' | b't' | b'f' | b'n') => {}
            _ => {
                if let Err(violation) = raw_number_variant(tree.token(node), path) {
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
/// The caller has fully validated the metadata. Nodes are reached through
/// upstream's shallow, constant-time accessors from an explicit stack, so a
/// hostile depth costs heap, not call stack, and the whole size-bounded value
/// is covered. Together the shallow node checks, the object field-name order
/// check, and the metadata check are the conditions upstream full validation
/// applies; the scan adds the Bifrost canonical rules on top:
///
/// - each object field owns its own byte slot ([`object_field_slots`]), so no
///   two fields share or overlap bytes and every node owns at least one byte,
///   so a value holds no more nodes than bytes and neither this walk nor any
///   later render can be amplified beyond the size-bounded input;
/// - resolved object field names strictly increase in both metadata modes, so
///   a name occurs once and lookup and rendering agree;
/// - numbers stay in the JSON domain: Decimal4 and Decimal8 are refused, a
///   Decimal16 is accepted only in scale-zero form for `i64::MAX + 1..=
///   u64::MAX` (the encoding of a JSON integer in that range), and a Float or
///   Double must be finite.
///
/// The first container past [`VARIANT_MAX_DEPTH`] is recorded and the walk
/// continues below it; a refused number is recorded at its pointer. The walk
/// stops at the first malformed node, since malformed outranks every class.
///
/// # Panics
///
/// Panics when a node's bytes are malformed; [`EncodedVariant::validate`]
/// contains it.
fn scan_encoded(metadata: VariantMetadata<'_>, value: &[u8]) -> VariantViolations {
    let mut found = VariantViolations::default();
    let mut path = JsonPointer::default();
    let exact_unsigned = i128::from(i64::MAX) + 1..=i128::from(u64::MAX);
    // Each pending node carries its parent's pointer depth, its token, and
    // the number of containers open above it.
    let mut pending = vec![(0, None, 0, Variant::new_with_metadata(metadata, value))];
    while let Some((parent, token, depth, node)) = pending.pop() {
        path.truncate(parent);
        if let Some(token) = token {
            path.tokens.push(token);
        }
        let here = path.tokens.len();
        let start = pending.len();
        match node {
            Variant::Object(object) => {
                let (depth, _) = enter_container(depth, &path, &mut found);
                let Some(slots) = object_field_slots(&object) else {
                    found.record(invalid(&path));
                    break;
                };
                let mut previous: Option<&str> = None;
                for (index, slot) in slots.into_iter().enumerate() {
                    let name = object
                        .field_name(index)
                        .expect("index is below the field count");
                    if previous.is_some_and(|previous| name <= previous) {
                        found.record(invalid(&path));
                    }
                    previous = Some(name);
                    let child = Variant::new_with_metadata(object.metadata.clone(), slot);
                    pending.push((here, Some(JsonPointer::escape(name)), depth, child));
                }
            }
            Variant::List(list) => {
                let (depth, _) = enter_container(depth, &path, &mut found);
                for (index, child) in list.iter().enumerate() {
                    pending.push((here, Some(index.to_string()), depth, child));
                }
            }
            Variant::Decimal4(_) | Variant::Decimal8(_) => {
                found.record(out_of_range(&path, "decimal"));
            }
            Variant::Decimal16(decimal)
                if decimal.scale() != 0 || !exact_unsigned.contains(&decimal.integer()) =>
            {
                found.record(out_of_range(&path, "decimal"));
            }
            Variant::Float(float) if !float.is_finite() => {
                found.record(out_of_range(&path, "double"));
            }
            Variant::Double(double) if !double.is_finite() => {
                found.record(out_of_range(&path, "double"));
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

/// Split an object's value region into one byte slot per field, in field
/// order.
///
/// The encoding stores one start offset per field plus a final end offset,
/// and starts may come in any order, so two fields could name the same or
/// overlapping bytes: a few hundred bytes could then describe a tree with
/// exponentially many nodes, or many fields could each render one large
/// child. Each field's slot therefore runs from its start to the next larger
/// start, or to the end offset. Repeated starts return `None`; a field whose
/// value runs past its slot then fails to decode inside it. Upstream's
/// shallow object check has already bounded the header, field ids, and
/// offsets inside `object.value`, which ends at the end offset.
fn object_field_slots<'v>(object: &VariantObject<'_, 'v>) -> Option<Vec<&'v [u8]>> {
    let bytes = object.value;
    // Header bits above the two basic-type bits: offset width minus one,
    // field-id width minus one, then the large-count flag.
    let header = bytes.first()? >> 2;
    let offset_size = usize::from(header & 0b11) + 1;
    let id_size = usize::from((header >> 2) & 0b11) + 1;
    let count_size = if header & 0b1_0000 == 0 { 1 } else { 4 };
    let fields = object.len();
    let offsets_start = 1 + count_size + fields * id_size;
    let values = bytes.get(offsets_start + (fields + 1) * offset_size..)?;
    let offsets = (0..=fields)
        .map(|index| {
            let at = offsets_start + index * offset_size;
            let little_endian = bytes.get(at..at + offset_size)?;
            Some(
                little_endian
                    .iter()
                    .rev()
                    .fold(0, |offset, byte| offset << 8 | usize::from(*byte)),
            )
        })
        .collect::<Option<Vec<_>>>()?;
    let (&end, starts) = offsets.split_last()?;
    let mut sorted = starts.to_vec();
    sorted.sort_unstable();
    if sorted.windows(2).any(|pair| pair[0] == pair[1]) {
        return None;
    }
    starts
        .iter()
        .map(|&start| {
            let next = sorted.partition_point(|&other| other <= start);
            values.get(start..sorted.get(next).copied().unwrap_or(end))
        })
        .collect()
}

/// Report whether a JSON value nests a container past [`VARIANT_MAX_DEPTH`].
///
/// `depth` is the number of containers open above `value`. Recursion stops
/// at the first container past the limit, so it never exceeds
/// [`VARIANT_MAX_DEPTH`] frames however deep the value is.
fn nests_past_limit(value: &Value, depth: u32) -> bool {
    match value {
        Value::Array(items) => {
            depth == VARIANT_MAX_DEPTH || items.iter().any(|item| nests_past_limit(item, depth + 1))
        }
        Value::Object(entries) => {
            depth == VARIANT_MAX_DEPTH
                || entries
                    .values()
                    .any(|child| nests_past_limit(child, depth + 1))
        }
        _ => false,
    }
}

/// Copy a JSON value, emptying every container past [`VARIANT_MAX_DEPTH`].
///
/// `depth` is the number of containers open above `value`. An emptied
/// container keeps its kind and position, so the Variant walk still reports
/// it as the first container past the limit at the same pointer, and
/// recursion stops there.
fn within_depth_limit(value: &Value, depth: u32) -> Value {
    match value {
        Value::Array(_) if depth == VARIANT_MAX_DEPTH => Value::Array(Vec::new()),
        Value::Object(_) if depth == VARIANT_MAX_DEPTH => Value::Object(serde_json::Map::new()),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| within_depth_limit(item, depth + 1))
                .collect(),
        ),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, child)| (key.clone(), within_depth_limit(child, depth + 1)))
                .collect(),
        ),
        scalar => scalar.clone(),
    }
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

    /// Wrap JSON text `inner` in `levels` single-item arrays.
    fn nested_text(inner: &str, levels: usize) -> String {
        format!("{}{inner}{}", "[".repeat(levels), "]".repeat(levels))
    }

    /// Wrap a JSON value in `levels` single-item arrays without recursion.
    fn nested_value(levels: usize) -> Value {
        (0..levels).fold(json!(1), |value, _| Value::Array(vec![value]))
    }

    /// Drop a deeply nested JSON array without recursing once per level.
    fn drop_nested(value: Value) {
        let mut next = Some(value);
        while let Some(Value::Array(mut items)) = next {
            next = items.pop();
        }
    }

    /// Wyrd, not `serde_json`, decides nesting for text and programmatic
    /// JSON at any depth.
    ///
    /// Depths 64, 65, 128, 129, and 10,000 cover the limit, `serde_json`'s
    /// 128-level recursion guard, and a value deeper than a recursive walk
    /// survives on a test thread's stack. Both
    /// constructors must accept 64 and report depth 65 at the same pointer
    /// for the rest; deep malformed text stays invalid and an out-of-range
    /// number under a deep value stays numeric.
    ///
    /// # Panics
    ///
    /// Panics when a depth is refused for the wrong reason, either
    /// constructor exhausts the stack, or the two constructors disagree.
    #[test]
    fn json_depth_is_decided_by_wyrd_at_any_depth() {
        for levels in [64, 65, 128, 129, 10_000] {
            let expected = if levels <= 64 {
                None
            } else {
                Some(VariantViolation::TooDeep {
                    path: "/0".repeat(64),
                    depth: 65,
                })
            };
            let text = EncodedVariant::from_json_text(&nested_text("1", levels));
            assert_eq!(text.as_ref().err(), expected.as_ref(), "text {levels}");
            let value = nested_value(levels);
            let programmatic = EncodedVariant::from_json(&value);
            drop_nested(value);
            assert_eq!(programmatic, text, "value {levels}");
        }
        assert_eq!(
            EncodedVariant::from_json_text(&format!("{}1", "[".repeat(10_000))),
            Err(VariantViolation::InvalidJson {
                path: String::new()
            })
        );
        assert_eq!(
            EncodedVariant::from_json_text(&nested_text("18446744073709551616", 10_000)),
            Err(VariantViolation::NumericOutOfRange {
                path: "/0".repeat(10_000),
                numeric_kind: "integer",
            })
        );
    }

    /// Bytes already built past the size limit report size before a recorded
    /// numeric or depth violation.
    ///
    /// # Panics
    ///
    /// Panics when an oversized value beside a refused number or an
    /// over-depth container reports anything but size.
    #[test]
    fn json_size_outranks_numeric_and_depth() {
        let big = "x".repeat(usize::try_from(VARIANT_MAX_ENCODED_BYTES).expect("fits"));
        for sibling in ["18446744073709551616".to_owned(), nested_text("1", 65)] {
            assert!(
                matches!(
                    EncodedVariant::from_json_text(&format!(
                        r#"{{"big": "{big}", "other": {sibling}}}"#
                    )),
                    Err(VariantViolation::TooLarge { .. })
                ),
                "{}",
                &sibling[..20.min(sibling.len())]
            );
        }
        let mut deep = json!({"big": big});
        deep["other"] = nested_value(65);
        assert!(matches!(
            EncodedVariant::from_json(&deep),
            Err(VariantViolation::TooLarge { .. })
        ));
    }

    /// A refused number builds nothing, so it cannot push accepted bytes that
    /// fit the size limit past it.
    ///
    /// The accepted string is sized so the value lands exactly on the limit;
    /// adding a refused number beside it must report the number, not size.
    ///
    /// # Panics
    ///
    /// Panics when the accepted value misses the limit or the refused number
    /// is reported as size.
    #[test]
    fn refused_numbers_add_no_size() {
        let limit = usize::try_from(VARIANT_MAX_ENCODED_BYTES).expect("fits");
        let text =
            |length: usize, extra: &str| format!(r#"{{"big": "{}"{extra}}}"#, "x".repeat(length));
        let near = limit - 64;
        let overhead = EncodedVariant::from_json_text(&text(near, ""))
            .expect("fits")
            .encoded_bytes()
            - near;
        let exact = text(limit - overhead, "");
        assert_eq!(
            EncodedVariant::from_json_text(&exact).map(|encoded| encoded.encoded_bytes()),
            Ok(limit)
        );
        assert_eq!(
            EncodedVariant::from_json_text(&text(limit - overhead, r#", "n": 1e400"#)),
            Err(VariantViolation::NumericOutOfRange {
                path: "/n".to_owned(),
                numeric_kind: "double",
            })
        );
    }

    /// The JSON walk reads each byte of compact deep input at most once, so
    /// its work is proportional to the text at any depth.
    ///
    /// Counts the bytes the walk reads rather than timing it: re-parsing a
    /// container's text at each level would read the shared suffix once per
    /// level.
    ///
    /// # Panics
    ///
    /// Panics when the walk reads more bytes than the text holds or deep
    /// input stops classifying as before.
    #[test]
    fn json_walk_reads_each_byte_once() {
        for levels in [1_000, 10_000] {
            for text in [
                nested_text("1", levels),
                format!(r#"{}1{}"#, r#"{"a":"#.repeat(levels), "}".repeat(levels)),
            ] {
                WALK_READ_BYTES.with(|read| read.set(0));
                assert!(matches!(
                    EncodedVariant::from_json_text(&text),
                    Err(VariantViolation::TooDeep { depth: 65, .. })
                ));
                let read = WALK_READ_BYTES.with(std::cell::Cell::get);
                assert!(
                    read <= text.len(),
                    "{levels}: read {read} of {}",
                    text.len()
                );
            }
        }
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

    /// Encode Variant metadata naming `names`, with four-byte offsets.
    ///
    /// The sorted-strings flag is set when `sorted` is true; the caller keeps
    /// that claim honest.
    fn metadata(names: &[&str], sorted: bool) -> Vec<u8> {
        let mut metadata = vec![if sorted { 0xd1 } else { 0xc1 }];
        let count = u32::try_from(names.len()).expect("short dictionary");
        metadata.extend(count.to_le_bytes());
        let mut offset = 0_u32;
        metadata.extend(offset.to_le_bytes());
        for name in names {
            offset += u32::try_from(name.len()).expect("short name");
            metadata.extend(offset.to_le_bytes());
        }
        metadata.extend(names.concat().into_bytes());
        metadata
    }

    /// Encode a raw Variant object with one-byte field ids and two-byte
    /// offsets.
    ///
    /// `fields` pairs each field id with its value's start offset inside
    /// `values`, which are written after the offsets; the end offset is
    /// `values.len()`.
    fn object(fields: &[(u8, u16)], values: &[u8]) -> Vec<u8> {
        let mut object = vec![0x06, u8::try_from(fields.len()).expect("short object")];
        object.extend(fields.iter().map(|(id, _)| id));
        for (_, offset) in fields {
            object.extend(offset.to_le_bytes());
        }
        let end = u16::try_from(values.len()).expect("values fit two-byte offsets");
        object.extend(end.to_le_bytes());
        object.extend_from_slice(values);
        object
    }

    /// Object fields that share bytes are refused before any walk or render
    /// can amplify them.
    ///
    /// Two shapes: 64 levels whose fields `a` and `b` point at the same child,
    /// about 640 bytes describing 2^64 nodes; and one shallow object whose 200
    /// fields all point at one 1,000-byte string, which has fewer nodes than
    /// bytes but would render 200 copies. A field starting inside another
    /// field's value is refused too.
    ///
    /// # Panics
    ///
    /// Panics when a shared or overlapping value is accepted or reported as
    /// anything but invalid encoding.
    #[test]
    fn raw_shared_field_values_are_refused() {
        let invalid = Err(VariantViolation::InvalidJson {
            path: String::new(),
        });
        let ab = metadata(&["a", "b"], true);
        let exponential = (0..64).fold(vec![0x00_u8], |child, _| object(&[(0, 0), (1, 0)], &child));
        assert_eq!(EncodedVariant::from_bytes(&ab, &exponential), invalid);

        let names = (0..200)
            .map(|index| format!("f{index:03}"))
            .collect::<Vec<_>>();
        let names = names.iter().map(String::as_str).collect::<Vec<_>>();
        let mut string = vec![0x40];
        string.extend(1000_u32.to_le_bytes());
        string.extend(vec![b'x'; 1000]);
        let fields = (0..200).map(|id| (id, 0)).collect::<Vec<_>>();
        let shallow = object(&fields, &string);
        assert_eq!(
            EncodedVariant::from_bytes(&metadata(&names, true), &shallow),
            invalid
        );

        // `b` starts at the second byte of `a`'s two-byte Int8.
        let overlapping = object(&[(0, 0), (1, 1)], &[0x0c, 0x00]);
        assert_eq!(EncodedVariant::from_bytes(&ab, &overlapping), invalid);
        let honest = object(&[(0, 1), (1, 0)], &[0x00, 0x0c, 0x05]);
        assert_eq!(
            variant_bytes_to_json(&ab, &honest).expect("disjoint fields render"),
            json!({"a": 5, "b": null})
        );
    }

    /// A resolved field name may occur only once in an object, whether or not
    /// the metadata claims sorted names.
    ///
    /// # Panics
    ///
    /// Panics when two field ids resolving to one name are accepted.
    #[test]
    fn raw_repeated_field_names_are_refused() {
        let repeated = object(&[(0, 0), (1, 1)], &[0x00, 0x00]);
        assert_eq!(
            EncodedVariant::from_bytes(&metadata(&["a", "a"], false), &repeated),
            Err(VariantViolation::InvalidJson {
                path: String::new()
            })
        );
        EncodedVariant::from_bytes(&metadata(&["a", "b"], false), &repeated)
            .expect("distinct names in unsorted metadata are canonical");
    }

    /// Raw numbers outside the JSON domain are refused with their numeric
    /// kind, and finite floating values stay accepted.
    ///
    /// # Panics
    ///
    /// Panics when a Decimal4, Decimal8, or non-finite floating value is
    /// accepted, or a finite one is refused.
    #[test]
    fn raw_numbers_outside_the_json_domain_are_refused() {
        let encode = |number: Variant<'_, '_>| {
            let mut builder = VariantBuilder::new();
            let mut list = builder.new_list();
            list.append_value(number);
            list.finish();
            builder.finish()
        };
        let refused = [
            (
                Variant::from(parquet_variant::VariantDecimal4::try_new(5, 0).expect("decimal")),
                "decimal",
            ),
            (
                Variant::from(parquet_variant::VariantDecimal8::try_new(5, 0).expect("decimal")),
                "decimal",
            ),
            (Variant::Float(f32::NAN), "double"),
            (Variant::Float(f32::INFINITY), "double"),
            (Variant::Double(f64::NAN), "double"),
            (Variant::Double(f64::NEG_INFINITY), "double"),
        ];
        for (number, numeric_kind) in refused {
            let label = format!("{number:?}");
            let (metadata, value) = encode(number);
            assert_eq!(
                EncodedVariant::from_bytes(&metadata, &value),
                Err(VariantViolation::NumericOutOfRange {
                    path: "/0".to_owned(),
                    numeric_kind,
                }),
                "{label}"
            );
        }
        let (metadata, value) = encode(Variant::Float(1.5));
        assert_eq!(variant_bytes_to_json(&metadata, &value), Ok(json!([1.5])));
        let (metadata, value) = encode(Variant::Double(3.0));
        assert_eq!(variant_bytes_to_json(&metadata, &value), Ok(json!([3.0])));
    }

    /// Every stored-Variant renderer refuses a compact hostile-depth cell
    /// before upstream recursion, then still renders an ordinary value.
    ///
    /// # Panics
    ///
    /// Panics when a renderer accepts the hostile cell, overflows the stack
    /// on it, or stops rendering an ordinary value.
    #[test]
    fn renderers_refuse_hostile_stored_variants() {
        let hostile = nest(vec![0x00], 20_000);
        assert!(matches!(
            variant_bytes_to_json(EMPTY_METADATA, &hostile),
            Err(VariantViolation::TooDeep { depth: 65, .. })
        ));
        let column: ArrayRef = Arc::new(StructArray::new(
            variant_storage_fields(),
            vec![
                Arc::new(BinaryArray::from(vec![EMPTY_METADATA])) as ArrayRef,
                Arc::new(BinaryArray::from(vec![hostile.as_slice()])) as ArrayRef,
            ],
            None,
        ));
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

        let ordinary = json!({"k": {"n": [1, 2.5]}});
        let encoded = EncodedVariant::from_json(&ordinary).expect("encodes");
        assert_eq!(
            variant_bytes_to_json(encoded.metadata(), encoded.value()),
            Ok(ordinary)
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
