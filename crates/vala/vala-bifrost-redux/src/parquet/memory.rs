//! Canonical memory contract shared by every Bifrost Parquet producer.

use arrow::array::{
    Array, BinaryArray, DictionaryArray, FixedSizeListArray, LargeBinaryArray, LargeListArray,
    LargeStringArray, ListArray, MapArray, StringArray, StructArray,
};
use std::collections::BTreeSet;
use std::num::NonZeroU64;

use arrow::datatypes::{
    ArrowDictionaryKeyType, ArrowNativeType, DataType, Field, Int8Type, Int16Type, Int32Type,
    Int64Type, Schema, UInt8Type, UInt16Type, UInt32Type, UInt64Type,
};
use arrow::record_batch::RecordBatch;
use parquet::file::metadata::{FileMetaData, KeyValue, ParquetMetaData};

use crate::schema::SchemaFingerprint;

/// Maximum canonical Arrow bytes in one row group.
pub const MAX_LOGICAL_ROW_GROUP_BYTES: u64 = 32 * 1024 * 1024;
/// Maximum decoded workspace required by one admitted row group.
pub const ROW_GROUP_DECODE_WORKSPACE_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum encoded bytes in one physical writer-v2 object.
pub const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
/// Maximum rows in one row group, independent of its byte ceiling.
pub const MAX_ROW_GROUP_ROWS: usize = 131_072;
/// Maximum row groups represented by one physical writer-v2 object.
pub const MAX_FILE_ROW_GROUPS: usize = 4;
/// Maximum normalized leaf columns represented by one physical object.
pub const MAX_FILE_LEAF_COLUMNS: usize = 64;
/// Maximum Parquet schema nodes, including nested group nodes.
pub const MAX_FILE_SCHEMA_ELEMENTS: usize = 128;
/// Maximum row-group column chunks represented by one physical object.
pub const MAX_FILE_COLUMN_CHUNKS: usize = 256;
/// Maximum encoded footer bytes reserved before writer creation.
pub const MAX_FOOTER_ENCODED_BYTES: u64 = 8 * 1024 * 1024;
/// Exact footer decoder workspace paired with the encoded footer child.
pub const FOOTER_DECODE_WORKSPACE_BYTES: u64 = 32 * 1024 * 1024;
/// Closed writer recipe stamped into every new data file.
pub const WRITER_RECIPE: &str = "bifrost-writer-v2";
/// Version of the footer memory-envelope metadata contract.
pub const PARQUET_MEMORY_ENVELOPE_VERSION: &str = "1";

/// Validates the encoded footer against the child reserved before writer creation.
///
/// # Errors
/// Returns a data refusal for an empty footer or one byte beyond 8 MiB.
pub fn validate_encoded_footer_bytes(encoded_bytes: u64) -> Result<(), String> {
    if encoded_bytes == 0 || encoded_bytes > MAX_FOOTER_ENCODED_BYTES {
        return Err("Parquet encoded footer exceeds its 8 MiB allowance".to_owned());
    }
    Ok(())
}

const KEY_RECIPE: &str = "wyrd.bifrost.writer_recipe";
const KEY_ENVELOPE_VERSION: &str = "wyrd.bifrost.memory_envelope_version";
const KEY_ROW_GROUP_LOGICAL: &str = "wyrd.bifrost.row_group_logical_bytes";
const KEY_DECODE_WORKSPACE: &str = "wyrd.bifrost.decode_workspace_bytes";
const KEY_FILE_BYTES: &str = "wyrd.bifrost.file_bytes";
const KEY_SCHEMA: &str = "wyrd.bifrost.schema_fingerprint";
const KEY_OBJECT: &str = "wyrd.bifrost.object_identity";
const KEY_MAX_ROW: &str = "wyrd.bifrost.max_logical_row_bytes";
const KEY_LEAF_PROFILE: &str = "wyrd.bifrost.leaf_width_profile";

/// One nonempty, order-preserving slice accepted by the writer contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundedRowSlice {
    /// Inclusive row offset in the source batch.
    pub offset: usize,
    /// Positive row count in this slice.
    pub len: usize,
    /// Canonical logical bytes measured for exactly these rows.
    pub logical_bytes: u64,
}

/// Typed, validated projection of the nine writer-v2 footer metadata fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BifrostParquetMemoryEnvelope {
    /// Canonical schema fingerprint expected by the caller.
    pub schema_fingerprint: SchemaFingerprint,
    /// Normalized committed object identity expected by the caller.
    pub object_identity: String,
    /// Positive largest canonical standalone row in this file.
    pub max_logical_row_bytes: u64,
    /// Canonical ordered per-leaf maxima encoding.
    pub leaf_width_profile: BifrostLeafWidthProfile,
}

/// Schema-bound ordered maxima for independently materialized Parquet leaves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BifrostLeafWidthProfile {
    /// Fingerprint whose normalized leaf order defines vector alignment.
    pub schema_fingerprint: SchemaFingerprint,
    /// Positive maximum logical bytes for each normalized leaf.
    pub max_logical_bytes: Vec<NonZeroU64>,
}

/// Closed projection selection accepted by the common profile owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BifrostLeafSelection {
    /// Select every normalized leaf.
    All,
    /// Select named top-level columns, expanding nested descendants.
    Columns(BTreeSet<String>),
}

impl BifrostLeafWidthProfile {
    /// Returns the checked maximum standalone-row width for one projection.
    ///
    /// Empty column selections are treated as [`BifrostLeafSelection::All`].
    /// Top-level nested columns expand to every normalized descendant and
    /// duplicate references are naturally deduplicated by the set.
    ///
    /// # Errors
    /// Returns a data-layout refusal when the schema fingerprint or leaf count
    /// differs, or when a requested top-level column is unknown or ambiguous.
    pub fn projected_bound(
        &self,
        schema: &Schema,
        selection: &BifrostLeafSelection,
    ) -> Result<u64, String> {
        if SchemaFingerprint::from_arrow_schema(schema) != self.schema_fingerprint {
            return Err("leaf-width profile schema fingerprint does not match".to_owned());
        }
        let leaf_paths = normalized_leaf_paths(schema)?;
        if leaf_paths.len() != self.max_logical_bytes.len() {
            return Err("leaf-width profile cardinality does not match schema order".to_owned());
        }
        let selected = match selection {
            BifrostLeafSelection::All => None,
            BifrostLeafSelection::Columns(columns) if columns.is_empty() => None,
            BifrostLeafSelection::Columns(columns) => {
                for column in columns {
                    let matches = schema
                        .fields()
                        .iter()
                        .filter(|field| field.name() == column)
                        .count();
                    if matches == 0 {
                        return Err(format!("projection column `{column}` is unknown"));
                    }
                    if matches > 1 {
                        return Err(format!("projection column `{column}` is ambiguous"));
                    }
                }
                Some(columns)
            }
        };
        leaf_paths
            .iter()
            .zip(&self.max_logical_bytes)
            .filter(|(path, _)| {
                selected.is_none_or(|columns| {
                    columns
                        .iter()
                        .any(|column| *path == column || path.starts_with(&format!("{column}.")))
                })
            })
            .try_fold(0_u64, |sum, (_, width)| {
                sum.checked_add(width.get())
                    .ok_or_else(|| "projected leaf-width bound overflows".to_owned())
            })
    }
}

impl BifrostParquetMemoryEnvelope {
    /// Constructs the exact nine footer fields for one admitted output batch.
    ///
    /// # Errors
    /// Returns a data-layout refusal when the batch contains no rows, exceeds
    /// the 64-leaf contract, uses an unsupported Arrow layout, or overflows a
    /// canonical standalone-row calculation.
    pub fn metadata_for_batch(
        batch: &RecordBatch,
        object_identity: &str,
    ) -> Result<Vec<KeyValue>, String> {
        if batch.num_rows() == 0 || object_identity.is_empty() {
            return Err("writer-v2 metadata requires rows and an object identity".to_owned());
        }
        let leaf_paths = normalized_leaf_paths(batch.schema().as_ref())?;
        let slices = BifrostArrowLogicalSizer::slice(batch)?;
        let max_logical_row_bytes = slices
            .iter()
            .flat_map(|slice| slice.offset..slice.offset + slice.len)
            .map(|row| {
                batch
                    .schema()
                    .fields()
                    .iter()
                    .zip(batch.columns())
                    .try_fold(0_u64, |sum, (field, column)| {
                        sum.checked_add(
                            field_row_leaf_bytes(field, column.as_ref(), row)?
                                .into_iter()
                                .try_fold(0_u64, |leaf_sum, value| {
                                    leaf_sum
                                        .checked_add(value)
                                        .ok_or_else(|| "canonical row size overflows".to_owned())
                                })?,
                        )
                        .ok_or_else(|| "canonical row size overflows".to_owned())
                    })
            })
            .collect::<Result<Vec<_>, String>>()?
            .into_iter()
            .max()
            .ok_or_else(|| "writer-v2 metadata requires a positive row".to_owned())?;
        let mut leaf_maxima = vec![0_u64; leaf_paths.len()];
        for row in 0..batch.num_rows() {
            let mut offset = 0;
            for (field, column) in batch.schema().fields().iter().zip(batch.columns()) {
                for value in field_row_leaf_bytes(field, column.as_ref(), row)? {
                    let slot = leaf_maxima.get_mut(offset).ok_or_else(|| {
                        "writer-v2 leaf profile exceeded normalized schema order".to_owned()
                    })?;
                    *slot = (*slot).max(value);
                    offset += 1;
                }
            }
            if offset != leaf_maxima.len() {
                return Err("writer-v2 leaf profile did not cover normalized schema".to_owned());
            }
        }
        for width in &mut leaf_maxima {
            *width = (*width).max(1);
        }
        let leaf_width_profile = format!(
            "v1:{}",
            leaf_maxima
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );
        let schema = hex::encode(SchemaFingerprint::from_arrow_schema(batch.schema().as_ref()).0);
        Ok([
            (KEY_RECIPE, WRITER_RECIPE.to_owned()),
            (
                KEY_ENVELOPE_VERSION,
                PARQUET_MEMORY_ENVELOPE_VERSION.to_owned(),
            ),
            (
                KEY_ROW_GROUP_LOGICAL,
                MAX_LOGICAL_ROW_GROUP_BYTES.to_string(),
            ),
            (
                KEY_DECODE_WORKSPACE,
                ROW_GROUP_DECODE_WORKSPACE_BYTES.to_string(),
            ),
            (KEY_FILE_BYTES, MAX_FILE_BYTES.to_string()),
            (KEY_SCHEMA, schema),
            (KEY_OBJECT, object_identity.to_owned()),
            (KEY_MAX_ROW, max_logical_row_bytes.to_string()),
            (KEY_LEAF_PROFILE, leaf_width_profile),
        ]
        .into_iter()
        .map(|(key, value)| KeyValue {
            key: key.to_owned(),
            value: Some(value),
        })
        .collect())
    }

    /// Validates and decodes the exact writer-v2 metadata contract.
    ///
    /// # Errors
    /// Returns a durable data refusal description when metadata is missing,
    /// duplicated, malformed, legacy, or bound to another schema or object.
    pub fn from_footer(
        footer: &FileMetaData,
        expected_schema: &Schema,
        expected_object_identity: &str,
    ) -> Result<Self, String> {
        let expected_fingerprint = SchemaFingerprint::from_arrow_schema(expected_schema);
        let metadata = footer
            .key_value_metadata()
            .ok_or_else(|| "writer-v2 footer metadata is missing".to_owned())?;
        let expected_keys = [
            KEY_RECIPE,
            KEY_ENVELOPE_VERSION,
            KEY_ROW_GROUP_LOGICAL,
            KEY_DECODE_WORKSPACE,
            KEY_FILE_BYTES,
            KEY_SCHEMA,
            KEY_OBJECT,
            KEY_MAX_ROW,
            KEY_LEAF_PROFILE,
        ];
        let contract_metadata = metadata
            .iter()
            .filter(|entry| entry.key.starts_with("wyrd.bifrost."))
            .collect::<Vec<_>>();
        if contract_metadata.len() != expected_keys.len()
            || contract_metadata
                .iter()
                .any(|entry| !expected_keys.contains(&entry.key.as_str()))
        {
            return Err("writer-v2 footer metadata is not the exact nine-field set".to_owned());
        }
        let value = |key: &str| -> Result<&str, String> {
            let mut matches = metadata.iter().filter(|entry| entry.key == key);
            let first = matches
                .next()
                .and_then(|entry| entry.value.as_deref())
                .ok_or_else(|| format!("writer-v2 footer field `{key}` is missing"))?;
            if matches.next().is_some() {
                return Err(format!("writer-v2 footer field `{key}` is duplicated"));
            }
            Ok(first)
        };
        if value(KEY_RECIPE)? != WRITER_RECIPE
            || value(KEY_ENVELOPE_VERSION)? != PARQUET_MEMORY_ENVELOPE_VERSION
            || parse_canonical_u64(value(KEY_ROW_GROUP_LOGICAL)?)? != MAX_LOGICAL_ROW_GROUP_BYTES
            || parse_canonical_u64(value(KEY_DECODE_WORKSPACE)?)?
                != ROW_GROUP_DECODE_WORKSPACE_BYTES
        {
            return Err("writer-v2 footer carries an unsupported memory recipe".to_owned());
        }
        let file_bytes = parse_canonical_u64(value(KEY_FILE_BYTES)?)?;
        if file_bytes == 0 || file_bytes > MAX_FILE_BYTES {
            return Err("writer-v2 file byte ceiling is invalid".to_owned());
        }
        let schema = value(KEY_SCHEMA)?;
        if schema.len() != 64
            || !schema
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || schema != hex::encode(expected_fingerprint.0)
        {
            return Err("writer-v2 schema fingerprint does not match".to_owned());
        }
        if expected_object_identity.is_empty() || value(KEY_OBJECT)? != expected_object_identity {
            return Err("writer-v2 object identity does not match".to_owned());
        }
        let max_logical_row_bytes = parse_canonical_u64(value(KEY_MAX_ROW)?)?;
        if max_logical_row_bytes == 0 || max_logical_row_bytes > MAX_LOGICAL_ROW_GROUP_BYTES {
            return Err("writer-v2 maximum row size is invalid".to_owned());
        }
        let leaf_width_profile = BifrostLeafWidthProfile {
            schema_fingerprint: expected_fingerprint,
            max_logical_bytes: parse_leaf_profile(value(KEY_LEAF_PROFILE)?)?,
        };
        if leaf_width_profile.max_logical_bytes.len()
            != normalized_leaf_paths(expected_schema)?.len()
        {
            return Err("leaf-width profile cardinality does not match schema order".to_owned());
        }
        Ok(Self {
            schema_fingerprint: expected_fingerprint,
            object_identity: expected_object_identity.to_owned(),
            max_logical_row_bytes,
            leaf_width_profile,
        })
    }
}

/// Sole canonical logical-byte measurer and deterministic row slicer.
pub struct BifrostArrowLogicalSizer;

impl BifrostArrowLogicalSizer {
    /// Splits a batch into the largest byte- and row-bounded slices.
    ///
    /// # Errors
    /// Returns a data-layout refusal for unsupported Arrow layouts, arithmetic
    /// overflow, or a single row larger than 32 MiB.
    pub fn slice(batch: &RecordBatch) -> Result<Vec<BoundedRowSlice>, String> {
        if batch.num_rows() == 0 {
            return Ok(Vec::new());
        }
        for row in 0..batch.num_rows() {
            let total = logical_slice_bytes(batch, row, 1)?;
            if total > MAX_LOGICAL_ROW_GROUP_BYTES {
                return Err(format!("row {row} exceeds the 32 MiB logical ceiling"));
            }
        }
        let mut slices = Vec::new();
        let mut offset = 0;
        while offset < batch.num_rows() {
            let max_len = MAX_ROW_GROUP_ROWS.min(batch.num_rows() - offset);
            let mut low = 1;
            let mut high = max_len;
            let mut accepted = 1;
            while low <= high {
                let mid = low + (high - low) / 2;
                let bytes = logical_slice_bytes(batch, offset, mid)?;
                if bytes <= MAX_LOGICAL_ROW_GROUP_BYTES {
                    accepted = mid;
                    low = mid + 1;
                } else {
                    high = mid - 1;
                }
            }
            let logical_bytes = logical_slice_bytes(batch, offset, accepted)?;
            slices.push(BoundedRowSlice {
                offset,
                len: accepted,
                logical_bytes,
            });
            offset += accepted;
        }
        Ok(slices)
    }

    /// Bisects one multi-row accepted slice while preserving exact accounting.
    ///
    /// # Errors
    /// Returns a data-layout refusal when the input has fewer than two rows or
    /// either child cannot be measured by the canonical logical sizer.
    pub fn bisect(
        batch: &RecordBatch,
        slice: BoundedRowSlice,
    ) -> Result<(BoundedRowSlice, BoundedRowSlice), String> {
        if slice.len < 2 {
            return Err("a one-row logical slice cannot be bisected".to_owned());
        }
        let left_len = slice.len / 2;
        let right_len = slice.len - left_len;
        let left = BoundedRowSlice {
            offset: slice.offset,
            len: left_len,
            logical_bytes: logical_slice_bytes(batch, slice.offset, left_len)?,
        };
        let right = BoundedRowSlice {
            offset: slice.offset + left_len,
            len: right_len,
            logical_bytes: logical_slice_bytes(batch, slice.offset + left_len, right_len)?,
        };
        Ok((left, right))
    }
}

/// Measures actual selected Arrow slice storage exactly once per buffer role.
fn logical_slice_bytes(batch: &RecordBatch, offset: usize, len: usize) -> Result<u64, String> {
    batch
        .schema()
        .fields()
        .iter()
        .zip(batch.columns())
        .try_fold(0_u64, |sum, (field, column)| {
            sum.checked_add(field_slice_bytes(field, column.as_ref(), offset, len)?)
                .ok_or_else(|| "canonical row-group size overflows".to_owned())
        })
}

/// Measures one field's buffers for an exact selected row range.
fn field_slice_bytes(
    field: &Field,
    array: &dyn Array,
    offset: usize,
    len: usize,
) -> Result<u64, String> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| "Arrow slice range overflows".to_owned())?;
    if end > array.len() {
        return Err("Arrow slice range exceeds its array".to_owned());
    }
    let validity = if array.nulls().is_some() {
        u64::try_from(len.div_ceil(8)).map_err(|_| "validity bytes overflow".to_owned())?
    } else {
        0
    };
    let body = match field.data_type() {
        DataType::Boolean => {
            u64::try_from(len.div_ceil(8)).map_err(|_| "boolean bytes overflow".to_owned())?
        }
        DataType::Int8 | DataType::UInt8 => fixed_slice_bytes(len, 1)?,
        DataType::Int16 | DataType::UInt16 | DataType::Float16 => fixed_slice_bytes(len, 2)?,
        DataType::Int32
        | DataType::UInt32
        | DataType::Float32
        | DataType::Date32
        | DataType::Time32(_)
        | DataType::Interval(_) => fixed_slice_bytes(len, 4)?,
        DataType::Int64
        | DataType::UInt64
        | DataType::Float64
        | DataType::Date64
        | DataType::Time64(_)
        | DataType::Timestamp(_, _)
        | DataType::Duration(_) => fixed_slice_bytes(len, 8)?,
        DataType::Decimal128(_, _) => fixed_slice_bytes(len, 16)?,
        DataType::Decimal256(_, _) => fixed_slice_bytes(len, 32)?,
        DataType::FixedSizeBinary(width) => fixed_slice_bytes(
            len,
            usize::try_from(*width).map_err(|_| "negative fixed-binary width".to_owned())?,
        )?,
        DataType::Utf8 => variable_slice_bytes(
            array
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| "Arrow UTF-8 array type does not match schema".to_owned())?
                .value_offsets(),
            offset,
            len,
            4,
        )?,
        DataType::Binary => variable_slice_bytes(
            array
                .as_any()
                .downcast_ref::<BinaryArray>()
                .ok_or_else(|| "Arrow binary array type does not match schema".to_owned())?
                .value_offsets(),
            offset,
            len,
            4,
        )?,
        DataType::LargeUtf8 => variable_slice_bytes_i64(
            array
                .as_any()
                .downcast_ref::<LargeStringArray>()
                .ok_or_else(|| "Arrow large UTF-8 array type does not match schema".to_owned())?
                .value_offsets(),
            offset,
            len,
            8,
        )?,
        DataType::LargeBinary => variable_slice_bytes_i64(
            array
                .as_any()
                .downcast_ref::<LargeBinaryArray>()
                .ok_or_else(|| "Arrow large binary array type does not match schema".to_owned())?
                .value_offsets(),
            offset,
            len,
            8,
        )?,
        DataType::Struct(_)
        | DataType::List(_)
        | DataType::LargeList(_)
        | DataType::FixedSizeList(_, _)
        | DataType::Map(_, _)
        | DataType::Dictionary(_, _) => nested_field_slice_bytes(field, array, offset, len)?,
        DataType::Null => 0,
        DataType::Union(_, _) | DataType::RunEndEncoded(_, _) => {
            return Err(format!("unsupported Arrow layout `{}`", array.data_type()));
        }
        other => return Err(format!("unsupported Arrow layout `{other}`")),
    };
    validity
        .checked_add(body)
        .ok_or_else(|| "field slice bytes overflow".to_owned())
}

/// Measures nested and dictionary storage for one selected field slice.
///
/// # Errors
///
/// Returns layout, downcast, range, or checked-arithmetic failures.
fn nested_field_slice_bytes(
    field: &Field,
    array: &dyn Array,
    offset: usize,
    len: usize,
) -> Result<u64, String> {
    match field.data_type() {
        DataType::Struct(_) => {
            let values = array
                .as_any()
                .downcast_ref::<StructArray>()
                .ok_or_else(|| "Arrow struct array type does not match schema".to_owned())?;
            values
                .fields()
                .iter()
                .zip(values.columns())
                .try_fold(0_u64, |sum, (child, values)| {
                    sum.checked_add(field_slice_bytes(child, values.as_ref(), offset, len)?)
                        .ok_or_else(|| "struct slice bytes overflow".to_owned())
                })
        }
        DataType::List(child) => {
            let values = array
                .as_any()
                .downcast_ref::<ListArray>()
                .ok_or_else(|| "Arrow list array type does not match schema".to_owned())?;
            nested_slice_bytes(
                child,
                values.values().as_ref(),
                values.value_offsets(),
                offset,
                len,
                4,
            )
        }
        DataType::LargeList(child) => {
            let values = array
                .as_any()
                .downcast_ref::<LargeListArray>()
                .ok_or_else(|| "Arrow large-list array type does not match schema".to_owned())?;
            nested_slice_bytes_i64(
                child,
                values.values().as_ref(),
                values.value_offsets(),
                offset,
                len,
                8,
            )
        }
        DataType::FixedSizeList(child, width) => {
            let values = array
                .as_any()
                .downcast_ref::<FixedSizeListArray>()
                .ok_or_else(|| "Arrow fixed-list array type does not match schema".to_owned())?;
            let width =
                usize::try_from(*width).map_err(|_| "negative fixed-list width".to_owned())?;
            field_slice_bytes(
                child,
                values.values().as_ref(),
                offset
                    .checked_mul(width)
                    .ok_or_else(|| "fixed-list offset overflows".to_owned())?,
                len.checked_mul(width)
                    .ok_or_else(|| "fixed-list length overflows".to_owned())?,
            )
        }
        DataType::Map(entries, _) => {
            let values = array
                .as_any()
                .downcast_ref::<MapArray>()
                .ok_or_else(|| "Arrow map array type does not match schema".to_owned())?;
            nested_slice_bytes(
                entries,
                values.entries(),
                values.value_offsets(),
                offset,
                len,
                4,
            )
        }
        DataType::Dictionary(key, _) => match key.as_ref() {
            DataType::Int8 => dictionary_slice_bytes::<Int8Type>(field, array, offset, len),
            DataType::Int16 => dictionary_slice_bytes::<Int16Type>(field, array, offset, len),
            DataType::Int32 => dictionary_slice_bytes::<Int32Type>(field, array, offset, len),
            DataType::Int64 => dictionary_slice_bytes::<Int64Type>(field, array, offset, len),
            DataType::UInt8 => dictionary_slice_bytes::<UInt8Type>(field, array, offset, len),
            DataType::UInt16 => dictionary_slice_bytes::<UInt16Type>(field, array, offset, len),
            DataType::UInt32 => dictionary_slice_bytes::<UInt32Type>(field, array, offset, len),
            DataType::UInt64 => dictionary_slice_bytes::<UInt64Type>(field, array, offset, len),
            _ => Err("unsupported dictionary key layout".to_owned()),
        },
        _ => Err("nested field helper received a flat layout".to_owned()),
    }
}

/// Computes one fixed-width selected buffer length.
fn fixed_slice_bytes(len: usize, width: usize) -> Result<u64, String> {
    u64::try_from(
        len.checked_mul(width)
            .ok_or_else(|| "fixed-width slice bytes overflow".to_owned())?,
    )
    .map_err(|_| "fixed-width slice bytes overflow".to_owned())
}

/// Computes 32-bit-offset variable buffer bytes for one slice.
fn variable_slice_bytes(
    offsets: &[i32],
    offset: usize,
    len: usize,
    offset_width: usize,
) -> Result<u64, String> {
    let start = usize::try_from(offsets[offset]).map_err(|_| "negative value offset".to_owned())?;
    let end =
        usize::try_from(offsets[offset + len]).map_err(|_| "negative value offset".to_owned())?;
    variable_slice_total(start, end, len, offset_width)
}

/// Computes 64-bit-offset variable buffer bytes for one slice.
fn variable_slice_bytes_i64(
    offsets: &[i64],
    offset: usize,
    len: usize,
    offset_width: usize,
) -> Result<u64, String> {
    let start = usize::try_from(offsets[offset]).map_err(|_| "negative value offset".to_owned())?;
    let end =
        usize::try_from(offsets[offset + len]).map_err(|_| "negative value offset".to_owned())?;
    variable_slice_total(start, end, len, offset_width)
}

/// Adds one N+1 offsets buffer to its exact selected value range.
fn variable_slice_total(
    start: usize,
    end: usize,
    len: usize,
    offset_width: usize,
) -> Result<u64, String> {
    let values = end
        .checked_sub(start)
        .ok_or_else(|| "variable offsets are descending".to_owned())?;
    let offsets = len
        .checked_add(1)
        .and_then(|count| count.checked_mul(offset_width))
        .ok_or_else(|| "variable offset bytes overflow".to_owned())?;
    u64::try_from(
        values
            .checked_add(offsets)
            .ok_or_else(|| "variable slice bytes overflow".to_owned())?,
    )
    .map_err(|_| "variable slice bytes overflow".to_owned())
}

/// Measures a 32-bit-offset nested child range and its N+1 parent offsets.
fn nested_slice_bytes(
    child: &Field,
    values: &dyn Array,
    offsets: &[i32],
    offset: usize,
    len: usize,
    offset_width: usize,
) -> Result<u64, String> {
    let start =
        usize::try_from(offsets[offset]).map_err(|_| "negative nested offset".to_owned())?;
    let end =
        usize::try_from(offsets[offset + len]).map_err(|_| "negative nested offset".to_owned())?;
    nested_slice_total(child, values, start, end, len, offset_width)
}

/// Measures a 64-bit-offset nested child range and its N+1 parent offsets.
fn nested_slice_bytes_i64(
    child: &Field,
    values: &dyn Array,
    offsets: &[i64],
    offset: usize,
    len: usize,
    offset_width: usize,
) -> Result<u64, String> {
    let start =
        usize::try_from(offsets[offset]).map_err(|_| "negative nested offset".to_owned())?;
    let end =
        usize::try_from(offsets[offset + len]).map_err(|_| "negative nested offset".to_owned())?;
    nested_slice_total(child, values, start, end, len, offset_width)
}

/// Combines a nested child slice with its parent offsets exactly once.
fn nested_slice_total(
    child: &Field,
    values: &dyn Array,
    start: usize,
    end: usize,
    len: usize,
    offset_width: usize,
) -> Result<u64, String> {
    let child_len = end
        .checked_sub(start)
        .ok_or_else(|| "nested offsets are descending".to_owned())?;
    let offset_bytes = fixed_slice_bytes(
        len.checked_add(1)
            .ok_or_else(|| "nested offset count overflows".to_owned())?,
        offset_width,
    )?;
    offset_bytes
        .checked_add(field_slice_bytes(child, values, start, child_len)?)
        .ok_or_else(|| "nested slice bytes overflow".to_owned())
}

/// Measures dictionary indices and distinct referenced logical values once.
fn dictionary_slice_bytes<K: ArrowDictionaryKeyType>(
    field: &Field,
    array: &dyn Array,
    offset: usize,
    len: usize,
) -> Result<u64, String> {
    let dictionary = array
        .as_any()
        .downcast_ref::<DictionaryArray<K>>()
        .ok_or_else(|| "Arrow dictionary array type does not match schema".to_owned())?;
    let mut indexes = BTreeSet::new();
    for row in offset..offset + len {
        if !dictionary.is_null(row) {
            indexes.insert(
                dictionary
                    .keys()
                    .value(row)
                    .to_usize()
                    .ok_or_else(|| "dictionary key is negative or overflows".to_owned())?,
            );
        }
    }
    let key_bytes = fixed_slice_bytes(len, std::mem::size_of::<K::Native>())?;
    let value_field = Field::new(
        field.name(),
        dictionary.values().data_type().clone(),
        field.is_nullable(),
    );
    indexes.into_iter().try_fold(key_bytes, |sum, index| {
        sum.checked_add(field_slice_bytes(
            &value_field,
            dictionary.values().as_ref(),
            index,
            1,
        )?)
        .ok_or_else(|| "dictionary selected values overflow".to_owned())
    })
}

/// Returns per-leaf standalone-row bytes aligned to one schema field.
fn field_row_leaf_bytes(field: &Field, array: &dyn Array, row: usize) -> Result<Vec<u64>, String> {
    let validity = u64::from(field.is_nullable());
    if array.is_null(row) {
        return null_row_leaf_bytes(field);
    }
    match field.data_type() {
        DataType::Struct(_) => {
            let values = array
                .as_any()
                .downcast_ref::<StructArray>()
                .ok_or_else(|| "Arrow struct array type does not match schema".to_owned())?;
            let mut contributions = Vec::new();
            for (child_field, child_array) in values.fields().iter().zip(values.columns()) {
                contributions.extend(field_row_leaf_bytes(
                    child_field,
                    child_array.as_ref(),
                    row,
                )?);
            }
            add_first(&mut contributions, validity)?;
            return Ok(contributions);
        }
        DataType::List(child) => {
            let values = array
                .as_any()
                .downcast_ref::<ListArray>()
                .ok_or_else(|| "Arrow list array type does not match schema".to_owned())?;
            return repeated_leaf_bytes(
                child,
                values.values().as_ref(),
                values.value_offsets(),
                row,
                validity + 8,
            );
        }
        DataType::LargeList(child) => {
            let values = array
                .as_any()
                .downcast_ref::<LargeListArray>()
                .ok_or_else(|| "Arrow large-list array type does not match schema".to_owned())?;
            return repeated_leaf_bytes_i64(
                child,
                values.values().as_ref(),
                values.value_offsets(),
                row,
                validity + 16,
            );
        }
        DataType::FixedSizeList(child, width) => {
            let values = array
                .as_any()
                .downcast_ref::<FixedSizeListArray>()
                .ok_or_else(|| "Arrow fixed-list array type does not match schema".to_owned())?;
            let start = row
                .checked_mul(
                    usize::try_from(*width).map_err(|_| "negative fixed-list width".to_owned())?,
                )
                .ok_or_else(|| "fixed-list row offset overflows".to_owned())?;
            let end = start
                .checked_add(
                    usize::try_from(*width).map_err(|_| "negative fixed-list width".to_owned())?,
                )
                .ok_or_else(|| "fixed-list row end overflows".to_owned())?;
            return repeated_leaf_range(child, values.values().as_ref(), start, end, validity);
        }
        DataType::Map(entries, _) => {
            let values = array
                .as_any()
                .downcast_ref::<MapArray>()
                .ok_or_else(|| "Arrow map array type does not match schema".to_owned())?;
            return repeated_leaf_bytes(
                entries,
                values.entries(),
                values.value_offsets(),
                row,
                validity + 8,
            );
        }
        DataType::Dictionary(key, _) => {
            return match key.as_ref() {
                DataType::Int8 => dictionary_leaf_bytes::<Int8Type>(field, array, row),
                DataType::Int16 => dictionary_leaf_bytes::<Int16Type>(field, array, row),
                DataType::Int32 => dictionary_leaf_bytes::<Int32Type>(field, array, row),
                DataType::Int64 => dictionary_leaf_bytes::<Int64Type>(field, array, row),
                DataType::UInt8 => dictionary_leaf_bytes::<UInt8Type>(field, array, row),
                DataType::UInt16 => dictionary_leaf_bytes::<UInt16Type>(field, array, row),
                DataType::UInt32 => dictionary_leaf_bytes::<UInt32Type>(field, array, row),
                DataType::UInt64 => dictionary_leaf_bytes::<UInt64Type>(field, array, row),
                _ => Err("unsupported dictionary key layout".to_owned()),
            };
        }
        DataType::Union(_, _) | DataType::RunEndEncoded(_, _) => {
            return Err(format!("unsupported Arrow layout `{}`", array.data_type()));
        }
        _ => {}
    }

    flat_row_leaf_bytes(field, array, row, validity)
}

/// Normalizes one null row across the field's physical leaves.
///
/// # Errors
///
/// Returns schema-shape errors from leaf counting.
fn null_row_leaf_bytes(field: &Field) -> Result<Vec<u64>, String> {
    let mut values = vec![u64::from(field.is_nullable()); field_leaf_count(field)?];
    if values.iter().all(|value| *value == 0) {
        values[0] = 1;
    }
    Ok(values)
}

/// Measures one non-nested field value as a single normalized leaf.
///
/// # Errors
///
/// Returns overflow, downcast, or unsupported-layout failures.
fn flat_row_leaf_bytes(
    field: &Field,
    array: &dyn Array,
    row: usize,
    validity: u64,
) -> Result<Vec<u64>, String> {
    let fixed = match array.data_type() {
        DataType::Boolean | DataType::Int8 | DataType::UInt8 => Some(1 + validity),
        DataType::Int16 | DataType::UInt16 | DataType::Float16 => Some(2 + validity),
        DataType::Int32
        | DataType::UInt32
        | DataType::Float32
        | DataType::Date32
        | DataType::Time32(_)
        | DataType::Interval(_) => Some(4 + validity),
        DataType::Int64
        | DataType::UInt64
        | DataType::Float64
        | DataType::Date64
        | DataType::Time64(_)
        | DataType::Timestamp(_, _)
        | DataType::Duration(_) => Some(8 + validity),
        DataType::Decimal128(_, _) => Some(16 + validity),
        DataType::Decimal256(_, _) => Some(32 + validity),
        DataType::FixedSizeBinary(width) => u64::try_from(*width)
            .ok()
            .and_then(|v| v.checked_add(validity)),
        _ => None,
    };
    if let Some(bytes) = fixed {
        return Ok(vec![bytes]);
    }
    let variable = match array.data_type() {
        DataType::Utf8 => array
            .as_any()
            .downcast_ref::<StringArray>()
            .map(|a| a.value(row).len() + 8 + usize::from(field.is_nullable())),
        DataType::LargeUtf8 => array
            .as_any()
            .downcast_ref::<LargeStringArray>()
            .map(|a| a.value(row).len() + 16 + usize::from(field.is_nullable())),
        DataType::Binary => array
            .as_any()
            .downcast_ref::<BinaryArray>()
            .map(|a| a.value(row).len() + 8 + usize::from(field.is_nullable())),
        DataType::LargeBinary => array
            .as_any()
            .downcast_ref::<LargeBinaryArray>()
            .map(|a| a.value(row).len() + 16 + usize::from(field.is_nullable())),
        _ => None,
    };
    if let Some(bytes) = variable {
        return u64::try_from(bytes)
            .map(|bytes| vec![bytes])
            .map_err(|_| "variable row size overflows".to_owned());
    }
    Err(format!("unsupported Arrow layout `{}`", array.data_type()))
}

/// Measures one dictionary index plus its selected logical value.
fn dictionary_leaf_bytes<K: ArrowDictionaryKeyType>(
    field: &Field,
    array: &dyn Array,
    row: usize,
) -> Result<Vec<u64>, String> {
    let dictionary = array
        .as_any()
        .downcast_ref::<DictionaryArray<K>>()
        .ok_or_else(|| "Arrow dictionary array type does not match schema".to_owned())?;
    let index = dictionary
        .keys()
        .value(row)
        .to_usize()
        .ok_or_else(|| "dictionary key is negative or overflows".to_owned())?;
    let value_field = Field::new(
        field.name(),
        dictionary.values().data_type().clone(),
        field.is_nullable(),
    );
    let mut contributions =
        field_row_leaf_bytes(&value_field, dictionary.values().as_ref(), index)?;
    let key_bytes = u64::try_from(std::mem::size_of::<K::Native>())
        .map_err(|_| "dictionary key width overflows".to_owned())?;
    add_first(&mut contributions, key_bytes)?;
    Ok(contributions)
}

/// Adds shared parent overhead to the first normalized descendant.
fn add_first(values: &mut [u64], overhead: u64) -> Result<(), String> {
    let first = values
        .first_mut()
        .ok_or_else(|| "nested Arrow field has no normalized descendant".to_owned())?;
    *first = first
        .checked_add(overhead)
        .ok_or_else(|| "nested logical overhead overflows".to_owned())?;
    Ok(())
}

/// Measures one list/map row from signed 32-bit value offsets.
fn repeated_leaf_bytes(
    child: &Field,
    values: &dyn Array,
    offsets: &[i32],
    row: usize,
    overhead: u64,
) -> Result<Vec<u64>, String> {
    let start = usize::try_from(offsets[row]).map_err(|_| "negative nested offset".to_owned())?;
    let end = usize::try_from(offsets[row + 1]).map_err(|_| "negative nested offset".to_owned())?;
    repeated_leaf_range(child, values, start, end, overhead)
}

/// Measures one large-list row from signed 64-bit value offsets.
fn repeated_leaf_bytes_i64(
    child: &Field,
    values: &dyn Array,
    offsets: &[i64],
    row: usize,
    overhead: u64,
) -> Result<Vec<u64>, String> {
    let start = usize::try_from(offsets[row]).map_err(|_| "negative nested offset".to_owned())?;
    let end = usize::try_from(offsets[row + 1]).map_err(|_| "negative nested offset".to_owned())?;
    repeated_leaf_range(child, values, start, end, overhead)
}

/// Accumulates selected repeated children without duplicating parent overhead.
fn repeated_leaf_range(
    child: &Field,
    values: &dyn Array,
    start: usize,
    end: usize,
    overhead: u64,
) -> Result<Vec<u64>, String> {
    let mut contributions = vec![0_u64; field_leaf_count(child)?];
    for index in start..end {
        let child_values = field_row_leaf_bytes(child, values, index)?;
        for (sum, value) in contributions.iter_mut().zip(child_values) {
            *sum = sum
                .checked_add(value)
                .ok_or_else(|| "repeated logical width overflows".to_owned())?;
        }
    }
    add_first(&mut contributions, overhead)?;
    Ok(contributions)
}

/// Counts normalized descendants for null and empty nested rows.
fn field_leaf_count(field: &Field) -> Result<usize, String> {
    let schema = Schema::new(vec![field.clone()]);
    normalized_leaf_paths(&schema).map(|paths| paths.len())
}

/// Parses one positive canonical base-10 integer without alternate spellings.
fn parse_canonical_u64(value: &str) -> Result<u64, String> {
    if value.is_empty()
        || value.starts_with('0')
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("writer-v2 integer metadata is not canonical".to_owned());
    }
    value
        .parse::<u64>()
        .map_err(|_| "writer-v2 integer metadata overflows".to_owned())
}

/// Validates the canonical nonempty `v1:` leaf-width vector encoding.
fn parse_leaf_profile(value: &str) -> Result<Vec<NonZeroU64>, String> {
    let values = value
        .strip_prefix("v1:")
        .ok_or_else(|| "leaf-width profile version is unsupported".to_owned())?;
    let entries: Vec<_> = values.split(',').collect();
    if entries.is_empty() || entries.len() > 64 {
        return Err("leaf-width profile cardinality is invalid".to_owned());
    }
    let mut parsed_entries = Vec::with_capacity(entries.len());
    for entry in entries {
        let parsed = parse_canonical_u64(entry)?;
        parsed_entries.push(
            NonZeroU64::new(parsed).ok_or_else(|| "leaf-width profile contains zero".to_owned())?,
        );
    }
    Ok(parsed_entries)
}

/// Derives deterministic dot-separated leaf paths in Arrow schema order.
fn normalized_leaf_paths(schema: &Schema) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    for field in schema.fields() {
        append_leaf_paths(field, field.name(), &mut paths)?;
    }
    if paths.is_empty() || paths.len() > 64 {
        return Err("writer-v2 leaf cardinality is outside 1..=64".to_owned());
    }
    Ok(paths)
}

/// Recursively expands one Arrow field into normalized Parquet leaf order.
fn append_leaf_paths(field: &Field, path: &str, paths: &mut Vec<String>) -> Result<(), String> {
    match field.data_type() {
        DataType::Struct(fields) => {
            for child in fields {
                append_leaf_paths(child, &format!("{path}.{}", child.name()), paths)?;
            }
        }
        DataType::List(child) | DataType::LargeList(child) | DataType::FixedSizeList(child, _) => {
            append_leaf_paths(child, &format!("{path}.{}", child.name()), paths)?;
        }
        DataType::Map(entries, _) => {
            append_leaf_paths(entries, &format!("{path}.{}", entries.name()), paths)?;
        }
        DataType::Dictionary(_, value) => {
            let synthetic = Field::new(field.name(), value.as_ref().clone(), field.is_nullable());
            append_leaf_paths(&synthetic, path, paths)?;
        }
        DataType::Union(_, _) | DataType::RunEndEncoded(_, _) => {
            return Err(format!(
                "unsupported writer-v2 nested layout `{}`",
                field.data_type()
            ));
        }
        _ => paths.push(path.to_owned()),
    }
    Ok(())
}

/// Returns the schema fingerprint used by producer metadata construction.
#[must_use]
pub fn schema_fingerprint(schema: &Schema) -> SchemaFingerprint {
    SchemaFingerprint::from_arrow_schema(schema)
}

/// Validates the conjunctive decoded writer-v2 footer ceilings.
///
/// # Errors
/// Returns a data-layout refusal when row groups, leaves, schema nodes, column
/// chunks, aggregate structure, or encoded row-group bytes exceed the closed
/// producer contract.
pub fn validate_writer_v2_structure(metadata: &ParquetMetaData) -> Result<(), String> {
    let groups = metadata.row_groups();
    let leaf_columns = metadata.file_metadata().schema_descr().num_columns();
    let schema_elements =
        parquet_schema_elements(metadata.file_metadata().schema_descr().root_schema())?;
    let chunks = groups
        .len()
        .checked_mul(leaf_columns)
        .ok_or_else(|| "Parquet footer structural count overflows".to_owned())?;
    let elements = groups
        .len()
        .checked_add(leaf_columns)
        .and_then(|value| value.checked_add(schema_elements))
        .and_then(|value| value.checked_add(chunks))
        .ok_or_else(|| "Parquet footer structural count overflows".to_owned())?;
    validate_structural_counts(
        groups.len(),
        leaf_columns,
        schema_elements,
        chunks,
        elements,
    )?;
    if groups.iter().any(|group| {
        u64::try_from(group.compressed_size()).unwrap_or(u64::MAX) > MAX_LOGICAL_ROW_GROUP_BYTES
    }) {
        return Err("Parquet writer-v2 structural footer ceiling exceeded".to_owned());
    }
    Ok(())
}

/// Validates the scalar structural axes before decoder or encoder construction.
///
/// # Errors
/// Returns a data-layout refusal when any exact conjunctive ceiling is exceeded.
fn validate_structural_counts(
    groups: usize,
    leaf_columns: usize,
    schema_elements: usize,
    chunks: usize,
    elements: usize,
) -> Result<(), String> {
    if groups == 0
        || groups > MAX_FILE_ROW_GROUPS
        || leaf_columns == 0
        || leaf_columns > MAX_FILE_LEAF_COLUMNS
        || schema_elements > MAX_FILE_SCHEMA_ELEMENTS
        || chunks > MAX_FILE_COLUMN_CHUNKS
        || elements > crate::parquet::footer_preflight::MAX_FOOTER_STRUCTURAL_ELEMENTS
    {
        return Err("Parquet writer-v2 structural footer ceiling exceeded".to_owned());
    }
    Ok(())
}

/// Counts nested Parquet schema nodes with checked recursion.
fn parquet_schema_elements(schema: &parquet::schema::types::Type) -> Result<usize, String> {
    if schema.is_primitive() {
        return Ok(1);
    }
    schema
        .get_fields()
        .iter()
        .try_fold(1_usize, |count, child| {
            count
                .checked_add(parquet_schema_elements(child.as_ref())?)
                .ok_or_else(|| "Parquet schema-element count overflows".to_owned())
        })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arrow::array::{
        Int64Array, StringArray, StringDictionaryBuilder, StructArray, TimestampMicrosecondArray,
    };
    use arrow::datatypes::{DataType, Field, Int8Type, Schema, TimeUnit};
    use parquet::arrow::ArrowWriter;
    use parquet::file::reader::{FileReader, SerializedFileReader};

    use super::*;

    /// Environment key carrying the isolated decoder fixture path.
    const DECODER_PEAK_FILE_ENV: &str = "WYRD_BIFROST_DECODER_PEAK_FILE";
    /// Environment key selecting the isolated control or decoder operation.
    const DECODER_PEAK_MODE_ENV: &str = "WYRD_BIFROST_DECODER_PEAK_MODE";

    /// Runs the exact helper test under the OS peak-resident-set observer.
    ///
    /// Returns the child's peak resident set size in bytes. Both platforms report
    /// the same underlying `getrusage` maximum-RSS figure, but through different
    /// tools: GNU `time` writes one caller-selected field to a file and reports
    /// KiB, while the BSD `time` shipped on macOS accepts neither `-f` nor `-o`
    /// and prints a fixed table to stderr in bytes. Normalizing here keeps the
    /// assertion in the caller expressed in bytes on every platform.
    ///
    /// # Panics
    /// Panics when the platform timing tool, child test, or peak report fails.
    fn isolated_decoder_peak(path: &std::path::Path, mode: &str) -> usize {
        let executable = std::env::current_exe().expect("current test executable");
        let child_args = [
            "--exact",
            "parquet::memory::tests::bifrost_standard_decoder_peak_child",
        ];

        #[cfg(target_os = "linux")]
        {
            let report = tempfile::NamedTempFile::new().expect("decoder peak report");
            let output = std::process::Command::new("/usr/bin/time")
                .args(["-f", "%M", "-o"])
                .arg(report.path())
                .arg(&executable)
                .args(child_args)
                .env(DECODER_PEAK_FILE_ENV, path)
                .env(DECODER_PEAK_MODE_ENV, mode)
                .output()
                .expect("run isolated decoder peak child");
            assert!(
                output.status.success(),
                "decoder peak child failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let peak_kib = std::fs::read_to_string(report.path())
                .expect("read decoder peak report")
                .trim()
                .parse::<usize>()
                .expect("parse decoder peak KiB");
            peak_kib.checked_mul(1024).expect("decoder peak bytes")
        }

        #[cfg(not(target_os = "linux"))]
        {
            let output = std::process::Command::new("/usr/bin/time")
                .arg("-l")
                .arg(&executable)
                .args(child_args)
                .env(DECODER_PEAK_FILE_ENV, path)
                .env(DECODER_PEAK_MODE_ENV, mode)
                .output()
                .expect("run isolated decoder peak child");
            assert!(
                output.status.success(),
                "decoder peak child failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .find_map(|line| {
                    line.strip_suffix("maximum resident set size")?
                        .trim()
                        .parse::<usize>()
                        .ok()
                })
                .expect("parse decoder peak bytes from the BSD time report")
        }
    }

    /// Isolated child used to compare standard-decoder peak RSS with a control.
    #[test]
    fn bifrost_standard_decoder_peak_child() {
        let Ok(path) = std::env::var(DECODER_PEAK_FILE_ENV) else {
            return;
        };
        let file = std::fs::File::open(path).expect("open isolated decoder fixture");
        if std::env::var(DECODER_PEAK_MODE_ENV).as_deref() == Ok("decode") {
            let reader = SerializedFileReader::new(file)
                .expect("isolated standard decoder accepts maximum footer");
            std::hint::black_box(reader.metadata().memory_size());
        } else {
            std::hint::black_box(file);
        }
    }

    /// Builds one two-column batch for deterministic byte-slicer assertions.
    fn batch(values: Vec<String>) -> RecordBatch {
        let rows = values.len();
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                Field::new("id", DataType::Int64, false),
                Field::new("payload", DataType::Utf8, false),
            ])),
            vec![
                Arc::new(Int64Array::from_iter_values(
                    0..i64::try_from(rows).expect("row count fits i64"),
                )),
                Arc::new(StringArray::from(values)),
            ],
        )
        .expect("test batch is structurally valid")
    }

    /// Exact logical boundary stays whole and one byte over splits in order.
    #[test]
    fn bifrost_parquet_memory_slices_exact_boundary_and_one_over() {
        let overhead = 16_usize;
        let exact = batch(vec!["x".repeat(
            usize::try_from(MAX_LOGICAL_ROW_GROUP_BYTES).expect("constant fits usize") - overhead,
        )]);
        assert_eq!(
            BifrostArrowLogicalSizer::slice(&exact).expect("exact boundary is accepted"),
            vec![BoundedRowSlice {
                offset: 0,
                len: 1,
                logical_bytes: MAX_LOGICAL_ROW_GROUP_BYTES,
            }]
        );

        let split = batch(vec![
            "x".repeat(16 * 1024 * 1024 - 14),
            "y".repeat(16 * 1024 * 1024 - 13),
        ]);
        let slices = BifrostArrowLogicalSizer::slice(&split).expect("two legal rows split");
        assert_eq!(
            slices.iter().map(|slice| slice.len).collect::<Vec<_>>(),
            [1, 1]
        );
        assert_eq!(
            slices.iter().map(|slice| slice.offset).collect::<Vec<_>>(),
            [0, 1]
        );
    }

    /// Empty batches have no artifacts and an indivisible oversized row refuses.
    #[test]
    fn bifrost_parquet_memory_refuses_single_oversized_row() {
        let empty = batch(Vec::new());
        assert!(
            BifrostArrowLogicalSizer::slice(&empty)
                .expect("empty is valid")
                .is_empty()
        );

        let oversized = batch(vec!["x".repeat(
            usize::try_from(MAX_LOGICAL_ROW_GROUP_BYTES).expect("constant fits usize"),
        )]);
        assert!(BifrostArrowLogicalSizer::slice(&oversized).is_err());
    }

    /// The independent row-count cap slices even a tiny logical batch.
    #[test]
    fn bifrost_parquet_memory_enforces_row_count_cap() {
        let values = (0..=MAX_ROW_GROUP_ROWS).map(|_| String::new()).collect();
        let slices = BifrostArrowLogicalSizer::slice(&batch(values)).expect("tiny rows fit");
        assert_eq!(slices.len(), 2);
        assert_eq!(slices[0].len, MAX_ROW_GROUP_ROWS);
        assert_eq!(slices[1].len, 1);
    }

    /// Writes one metadata variant and returns its decoded Parquet footer.
    fn footer_with_metadata(
        batch: &RecordBatch,
        metadata: Vec<KeyValue>,
    ) -> (tempfile::TempDir, parquet::file::metadata::ParquetMetaData) {
        let directory = tempfile::tempdir().expect("footer test directory");
        let path = directory.path().join("footer.parquet");
        let file = std::fs::File::create(&path).expect("footer test file");
        let mut writer = ArrowWriter::try_new(
            file,
            batch.schema(),
            Some(
                crate::parquet::writer_properties::bifrost_writer_properties_with_metadata(
                    batch.num_rows(),
                    metadata,
                    &[],
                ),
            ),
        )
        .expect("footer writer");
        writer.write(batch).expect("footer row group");
        writer.close().expect("footer close");
        let reader =
            SerializedFileReader::new(std::fs::File::open(path).expect("open footer test file"))
                .expect("decode footer");
        (directory, reader.metadata().clone())
    }

    /// Builds real Parquet metadata at caller-selected leaf and row-group counts.
    fn structural_footer(
        leaf_columns: usize,
        row_groups: usize,
    ) -> (tempfile::TempDir, parquet::file::metadata::ParquetMetaData) {
        let schema = Arc::new(Schema::new(
            (0..leaf_columns)
                .map(|index| Field::new(format!("c{index}"), DataType::Int64, false))
                .collect::<Vec<_>>(),
        ));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            (0..leaf_columns)
                .map(|_| Arc::new(Int64Array::from(vec![1])) as Arc<dyn Array>)
                .collect(),
        )
        .expect("structural batch");
        let directory = tempfile::tempdir().expect("structural footer directory");
        let path = directory.path().join("structural.parquet");
        let mut writer = ArrowWriter::try_new(
            std::fs::File::create(&path).expect("structural file"),
            schema,
            Some(
                crate::parquet::writer_properties::bifrost_writer_properties_with_metadata(
                    1,
                    Vec::new(),
                    &[],
                ),
            ),
        )
        .expect("structural writer");
        for _ in 0..row_groups {
            writer.write(&batch).expect("structural row group");
            writer.flush().expect("flush structural row group");
        }
        writer.close().expect("structural close");
        let reader =
            SerializedFileReader::new(std::fs::File::open(path).expect("open structural file"))
                .expect("standard structural decoder");
        (directory, reader.metadata().clone())
    }

    /// Writes a real Parquet footer with caller-selected synthetic metadata.
    fn synthetic_metadata_footer(
        key_count: usize,
    ) -> (tempfile::TempDir, std::path::PathBuf, Vec<u8>) {
        let directory = tempfile::tempdir().expect("synthetic footer directory");
        let path = directory.path().join("synthetic.parquet");
        let batch = batch(vec!["value".to_owned()]);
        let metadata = (0..key_count)
            .map(|index| KeyValue {
                key: format!("synthetic-{index:04}"),
                value: Some("v".to_owned()),
            })
            .collect();
        let mut writer = ArrowWriter::try_new(
            std::fs::File::create(&path).expect("synthetic footer file"),
            batch.schema(),
            Some(
                crate::parquet::writer_properties::bifrost_writer_properties_with_metadata(
                    batch.num_rows(),
                    metadata,
                    &[],
                ),
            ),
        )
        .expect("synthetic footer writer");
        writer.write(&batch).expect("synthetic footer row group");
        writer.close().expect("synthetic footer close");
        let file = std::fs::read(&path).expect("read synthetic test file");
        let trailer = file
            .get(file.len().saturating_sub(8)..)
            .expect("Parquet trailer");
        let footer_len = usize::try_from(u32::from_le_bytes(
            trailer[..4].try_into().expect("footer length trailer"),
        ))
        .expect("footer length fits address space");
        let footer_start = file
            .len()
            .checked_sub(8 + footer_len)
            .expect("footer lies inside test file");
        (directory, path, file[footer_start..file.len() - 8].to_vec())
    }

    /// Joint writer-v2 structural caps accept the maximum grid and reject each excess axis.
    #[test]
    fn bifrost_parquet_joint_structural_cap_boundaries() {
        let (_directory, maximum) = structural_footer(64, 4);
        assert_eq!(maximum.row_groups().len(), 4);
        assert_eq!(maximum.file_metadata().schema_descr().num_columns(), 64);
        assert_eq!(
            maximum
                .row_groups()
                .iter()
                .map(|group| group.columns().len())
                .sum::<usize>(),
            256
        );
        validate_writer_v2_structure(&maximum).expect("4 x 64 structural grid");
        assert!(
            maximum.memory_size()
                <= usize::try_from(FOOTER_DECODE_WORKSPACE_BYTES)
                    .expect("workspace fits address space"),
            "standard-decoder metadata remains inside the 32 MiB workspace"
        );

        let (_directory, fifth_group) = structural_footer(64, 5);
        assert!(validate_writer_v2_structure(&fifth_group).is_err());
        let (_directory, sixty_fifth_leaf) = structural_footer(65, 4);
        assert_eq!(
            sixty_fifth_leaf
                .row_groups()
                .iter()
                .map(|group| group.columns().len())
                .sum::<usize>(),
            260
        );
        assert!(validate_writer_v2_structure(&sixty_fifth_leaf).is_err());
        assert!(validate_structural_counts(4, 64, 65, 257, 390).is_err());
        assert!(validate_structural_counts(4, 64, 65, 256, 4_097).is_err());
    }

    /// The largest real footer admitted by preflight decodes inside 32 MiB.
    #[test]
    fn bifrost_maximum_synthetic_footer_uses_bounded_standard_decoder() {
        let mut low = 0_usize;
        let mut high = crate::parquet::footer_preflight::MAX_FOOTER_STRUCTURAL_ELEMENTS;
        while low < high {
            let middle = low + (high - low).div_ceil(2);
            let (_directory, _path, footer) = synthetic_metadata_footer(middle);
            if crate::parquet::footer_preflight::preflight_compact_thrift(&footer).is_ok() {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        let (_directory, path, footer) = synthetic_metadata_footer(low);
        let elements =
            crate::parquet::footer_preflight::compact_thrift_structural_elements(&footer)
                .expect("maximum accepted real footer");
        let (_rejected_directory, _rejected_path, rejected) = synthetic_metadata_footer(low + 1);
        assert!(crate::parquet::footer_preflight::preflight_compact_thrift(&rejected).is_err());
        assert_eq!(
            elements,
            crate::parquet::footer_preflight::MAX_FOOTER_STRUCTURAL_ELEMENTS,
            "maximum accepted real footer reaches the exact structural ceiling"
        );
        let control_peak = isolated_decoder_peak(&path, "control");
        let decode_process_peak = isolated_decoder_peak(&path, "decode");
        // Guard the subtraction before trusting it. The two peaks come from
        // separate process launches, so an incoherent pair (decode measuring
        // below the control baseline) means the measurement captured process
        // noise rather than decoder growth. Saturating that case to zero would
        // let the ceiling assertion below pass without measuring anything, so
        // refuse the reading instead of reporting a fabricated pass.
        assert!(
            decode_process_peak >= control_peak,
            "decoder peak measurement is incoherent (decode {decode_process_peak} < control \
             {control_peak}); the resident-set delta cannot be trusted"
        );
        let decoder_peak = decode_process_peak - control_peak;
        assert!(
            decoder_peak
                <= usize::try_from(FOOTER_DECODE_WORKSPACE_BYTES)
                    .expect("workspace fits address space"),
            "standard decoder peak {decoder_peak} exceeds the 32 MiB workspace"
        );
        let reader = SerializedFileReader::new(
            std::fs::File::open(path).expect("open maximum accepted footer"),
        )
        .expect("standard decoder accepts maximum preflighted footer");
        assert!(
            reader.metadata().memory_size()
                <= usize::try_from(FOOTER_DECODE_WORKSPACE_BYTES)
                    .expect("workspace fits address space")
        );
    }

    /// The pre-writer footer child admits exactly 8 MiB and refuses one byte more.
    #[test]
    fn bifrost_parquet_footer_allowance_exact_boundary() {
        validate_encoded_footer_bytes(MAX_FOOTER_ENCODED_BYTES).expect("exact footer allowance");
        assert!(validate_encoded_footer_bytes(MAX_FOOTER_ENCODED_BYTES + 1).is_err());
        assert!(validate_encoded_footer_bytes(0).is_err());
    }

    /// Asserts every canonical metadata key is required exactly once.
    ///
    /// Each key is dropped and then duplicated in turn, and both mutations must
    /// refuse. Requiring presence alone would let a writer append a second,
    /// conflicting copy of a field and have the reader silently pick one, so
    /// exactly-once is the property under test, not merely presence.
    ///
    /// # Panics
    ///
    /// Panics if any key can be dropped or duplicated without refusal.
    fn assert_missing_and_duplicated_fields_refuse(
        batch: &RecordBatch,
        object: &str,
        metadata: &[KeyValue],
    ) {
        let metadata = metadata.to_vec();
        for key in [
            KEY_RECIPE,
            KEY_ENVELOPE_VERSION,
            KEY_ROW_GROUP_LOGICAL,
            KEY_DECODE_WORKSPACE,
            KEY_FILE_BYTES,
            KEY_SCHEMA,
            KEY_OBJECT,
            KEY_MAX_ROW,
            KEY_LEAF_PROFILE,
        ] {
            let missing = metadata
                .iter()
                .filter(|entry| entry.key != key)
                .cloned()
                .collect();
            let (_directory, footer) = footer_with_metadata(batch, missing);
            assert!(
                BifrostParquetMemoryEnvelope::from_footer(
                    footer.file_metadata(),
                    batch.schema().as_ref(),
                    object,
                )
                .is_err(),
                "missing `{key}` must refuse"
            );

            let mut duplicated = metadata.clone();
            duplicated.push(
                metadata
                    .iter()
                    .find(|entry| entry.key == key)
                    .expect("duplicated metadata field")
                    .clone(),
            );
            let (_directory, footer) = footer_with_metadata(batch, duplicated);
            assert!(
                BifrostParquetMemoryEnvelope::from_footer(
                    footer.file_metadata(),
                    batch.schema().as_ref(),
                    object,
                )
                .is_err(),
                "duplicated `{key}` must refuse"
            );
        }
    }

    /// Asserts every canonical metadata value is validated, not merely parsed.
    ///
    /// The cases cover the boundaries each field owns: an unknown recipe or
    /// envelope version, one byte past each size ceiling, a zero where a
    /// positive count is required, a `u64` overflow, wrong-case hex, a mismatched
    /// object key, and leaf-profile encodings that differ from the canonical one
    /// only by version, arity, leading zero, or whitespace. Every one must
    /// refuse.
    ///
    /// # Panics
    ///
    /// Panics if any malformed value is accepted.
    fn assert_malformed_values_refuse(batch: &RecordBatch, object: &str, metadata: &[KeyValue]) {
        let metadata = metadata.to_vec();
        for (key, malformed) in [
            (KEY_RECIPE, "bifrost-writer-v1"),
            (KEY_ENVELOPE_VERSION, "2"),
            (KEY_ROW_GROUP_LOGICAL, "33554433"),
            (KEY_DECODE_WORKSPACE, "33554431"),
            (KEY_FILE_BYTES, "0"),
            (KEY_FILE_BYTES, "134217729"),
            (KEY_SCHEMA, "A"),
            (KEY_SCHEMA, "a"),
            (KEY_OBJECT, "s3://bucket/wrong.parquet"),
            (KEY_MAX_ROW, "0"),
            (KEY_MAX_ROW, "33554433"),
            (KEY_MAX_ROW, "18446744073709551616"),
            (KEY_LEAF_PROFILE, "v2:8,15"),
            (KEY_LEAF_PROFILE, "v1:8"),
            (KEY_LEAF_PROFILE, "v1:0,15"),
            (KEY_LEAF_PROFILE, "v1:08,15"),
            (KEY_LEAF_PROFILE, "v1:8, 15"),
            (KEY_LEAF_PROFILE, "v1:8,18446744073709551616"),
        ] {
            let mut invalid = metadata.clone();
            invalid
                .iter_mut()
                .find(|entry| entry.key == key)
                .expect("metadata mutation field")
                .value = Some(malformed.to_owned());
            let (_directory, footer) = footer_with_metadata(batch, invalid);
            assert!(
                BifrostParquetMemoryEnvelope::from_footer(
                    footer.file_metadata(),
                    batch.schema().as_ref(),
                    object,
                )
                .is_err(),
                "malformed `{key}` value `{malformed}` must refuse"
            );
        }
    }

    /// Nine metadata fields round-trip and every malformed contract shape refuses.
    #[test]
    fn bifrost_leaf_width_profile_is_schema_bound_and_canonical() {
        let batch = batch(vec!["payload".to_owned()]);
        let object = "s3://bucket/table/day=2026-08-14/part-00000.parquet";
        let metadata = BifrostParquetMemoryEnvelope::metadata_for_batch(&batch, object)
            .expect("canonical metadata");
        assert_eq!(metadata.len(), 9);
        let (_directory, footer) = footer_with_metadata(&batch, metadata.clone());
        let envelope = BifrostParquetMemoryEnvelope::from_footer(
            footer.file_metadata(),
            batch.schema().as_ref(),
            object,
        )
        .expect("canonical footer");
        assert_eq!(envelope.leaf_width_profile.max_logical_bytes.len(), 2);

        assert_missing_and_duplicated_fields_refuse(&batch, object, &metadata);

        let mut extra = metadata.clone();
        extra.push(KeyValue {
            key: "wyrd.bifrost.unexpected".to_owned(),
            value: Some("1".to_owned()),
        });
        let (_directory, footer) = footer_with_metadata(&batch, extra);
        assert!(
            BifrostParquetMemoryEnvelope::from_footer(
                footer.file_metadata(),
                batch.schema().as_ref(),
                object,
            )
            .is_err(),
            "an extra metadata field must refuse"
        );

        assert_malformed_values_refuse(&batch, object, &metadata);

        let other_schema = Schema::new(vec![Field::new("other", DataType::Int64, false)]);
        assert!(
            BifrostParquetMemoryEnvelope::from_footer(
                footer.file_metadata(),
                &other_schema,
                object,
            )
            .is_err(),
            "caller schema mismatch must refuse"
        );
    }

    /// Writer-v2 footer identity remains exact across UTC spelling aliases.
    #[test]
    fn bifrost_footer_fingerprint_does_not_alias_utc_spellings() {
        let utc_schema = Arc::new(Schema::new(vec![Field::new(
            "observed_at",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        )]));
        let batch = RecordBatch::try_new(
            Arc::clone(&utc_schema),
            vec![Arc::new(
                TimestampMicrosecondArray::from(vec![1]).with_timezone("UTC"),
            )],
        )
        .expect("UTC writer-v2 batch");
        let object = "tenants/test/table/day=2026-08-17/source.parquet";
        let metadata = BifrostParquetMemoryEnvelope::metadata_for_batch(&batch, object)
            .expect("UTC writer-v2 metadata");
        let (_directory, footer) = footer_with_metadata(&batch, metadata);
        let offset_schema = Schema::new(vec![Field::new(
            "observed_at",
            DataType::Timestamp(TimeUnit::Microsecond, Some("+00:00".into())),
            false,
        )]);

        BifrostParquetMemoryEnvelope::from_footer(
            footer.file_metadata(),
            utc_schema.as_ref(),
            object,
        )
        .expect("footer matches its exact source schema");
        assert!(
            BifrostParquetMemoryEnvelope::from_footer(
                footer.file_metadata(),
                &offset_schema,
                object,
            )
            .is_err(),
            "footer fingerprint must not alias a different Arrow schema spelling"
        );
    }

    /// Standalone profiles keep dictionary and nested leaf widths independent.
    #[test]
    fn bifrost_leaf_width_profile_is_standalone_row_canonical() {
        let mut dictionary = StringDictionaryBuilder::<Int8Type>::new();
        dictionary.append("thin").expect("dictionary value");
        dictionary
            .append("independently-fat")
            .expect("dictionary value");
        let dictionary = Arc::new(dictionary.finish());
        let struct_fields = vec![
            Arc::new(Field::new("small", DataType::Int64, false)),
            Arc::new(Field::new("large", DataType::Utf8, false)),
        ];
        let nested = Arc::new(StructArray::new(
            struct_fields.clone().into(),
            vec![
                Arc::new(Int64Array::from(vec![1, 2])),
                Arc::new(StringArray::from(vec!["x", "fat-unselected-leaf"])),
            ],
            None,
        ));
        let schema = Arc::new(Schema::new(vec![
            Field::new("dict", dictionary.data_type().clone(), false),
            Field::new("nested", DataType::Struct(struct_fields.into()), false),
        ]));
        let batch = RecordBatch::try_new(schema.clone(), vec![dictionary, nested])
            .expect("nested profile batch");
        let metadata = BifrostParquetMemoryEnvelope::metadata_for_batch(&batch, "object")
            .expect("nested profile metadata");
        let encoded = metadata
            .iter()
            .find(|entry| entry.key == KEY_LEAF_PROFILE)
            .and_then(|entry| entry.value.as_deref())
            .expect("encoded profile");
        let widths = parse_leaf_profile(encoded).expect("canonical profile");
        assert_eq!(widths.len(), 3);
        assert!(widths[2] > widths[1], "fat nested leaf stays independent");
    }

    /// Projection alignment expands nested columns and rejects schema drift.
    #[test]
    fn bifrost_leaf_projection_alignment_is_owner_checked() {
        let nested_fields = vec![
            Arc::new(Field::new("a", DataType::Int64, false)),
            Arc::new(Field::new("b", DataType::Utf8, false)),
        ];
        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("nested", DataType::Struct(nested_fields.into()), true),
        ]);
        let profile = BifrostLeafWidthProfile {
            schema_fingerprint: SchemaFingerprint::from_arrow_schema(&schema),
            max_logical_bytes: [8_u64, 9, 10]
                .into_iter()
                .map(|value| NonZeroU64::new(value).expect("positive width"))
                .collect(),
        };
        assert_eq!(
            profile
                .projected_bound(&schema, &BifrostLeafSelection::All)
                .expect("all projection"),
            27
        );
        assert_eq!(
            profile
                .projected_bound(
                    &schema,
                    &BifrostLeafSelection::Columns(BTreeSet::from(["nested".to_owned()])),
                )
                .expect("nested expansion"),
            19
        );
        assert!(
            profile
                .projected_bound(
                    &schema,
                    &BifrostLeafSelection::Columns(BTreeSet::from(["missing".to_owned()])),
                )
                .is_err()
        );
        let reordered = Schema::new(vec![
            Field::new("nested", schema.field(1).data_type().clone(), true),
            Field::new("id", DataType::Int64, false),
        ]);
        assert!(
            profile
                .projected_bound(&reordered, &BifrostLeafSelection::All)
                .is_err()
        );
    }
}
