use arrow::array::{Array, ArrayRef, Float64Array, StringArray};
use arrow::compute::cast;
use arrow::record_batch::RecordBatch;
use arrow_schema::DataType;

use wyrd_spec::ids::FeatureName;

/// Borrowed column reference returned by `resolve_column`.
pub struct ColumnRef<'a> {
    pub name: &'a str,
    pub array: &'a ArrayRef,
}

/// Phase-neutral error for any feature access failure (missing column or cast).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatureLookupError;

impl<'a> ColumnRef<'a> {
    pub fn data_type_string(&self) -> String {
        format!("{:?}", self.array.data_type())
    }

    /// Cast and return all non-null `f64` values.
    ///
    /// Returns [`FeatureLookupError`] if the column cannot be cast to `Float64`.
    /// Callers translate this to the appropriate Wyrd error variant.
    pub fn collect_f64_non_null(&self) -> Result<Vec<f64>, FeatureLookupError> {
        let casted = cast(self.array, &DataType::Float64).map_err(|_| FeatureLookupError)?;
        let arr = casted
            .as_any()
            .downcast_ref::<Float64Array>()
            .ok_or(FeatureLookupError)?;
        let mut out = Vec::with_capacity(arr.len().saturating_sub(arr.null_count()));
        for i in 0..arr.len() {
            if arr.is_valid(i) {
                out.push(arr.value(i));
            }
        }
        Ok(out)
    }

    /// Cast and return all non-null string values.
    ///
    /// Returns [`FeatureLookupError`] if the column cannot be cast to `Utf8`.
    pub fn collect_string_non_null(&self) -> Result<Vec<String>, FeatureLookupError> {
        let casted = cast(self.array, &DataType::Utf8).map_err(|_| FeatureLookupError)?;
        let arr = casted
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or(FeatureLookupError)?;
        let mut out = Vec::with_capacity(arr.len().saturating_sub(arr.null_count()));
        for i in 0..arr.len() {
            if arr.is_valid(i) {
                out.push(arr.value(i).to_string());
            }
        }
        Ok(out)
    }

    /// True if the column's logical type is floating-point or integer.
    pub fn is_numeric(&self) -> bool {
        matches!(
            self.array.data_type(),
            DataType::Float16
                | DataType::Float32
                | DataType::Float64
                | DataType::Int8
                | DataType::Int16
                | DataType::Int32
                | DataType::Int64
                | DataType::UInt8
                | DataType::UInt16
                | DataType::UInt32
                | DataType::UInt64
        )
    }
}

/// Look up a feature column in the RecordBatch schema by name.
///
/// # Errors
/// Returns [`FeatureLookupError`] when the schema does not contain a column
/// with `feature.as_str()` as its name. Callers promote this to the
/// appropriate phase-specific error variant.
pub fn resolve_column<'a>(
    batch: &'a RecordBatch,
    feature: &'a FeatureName,
) -> Result<ColumnRef<'a>, FeatureLookupError> {
    let schema = batch.schema();
    let idx = schema
        .index_of(feature.as_str())
        .map_err(|_| FeatureLookupError)?;
    Ok(ColumnRef {
        name: feature.as_str(),
        array: batch.column(idx),
    })
}
