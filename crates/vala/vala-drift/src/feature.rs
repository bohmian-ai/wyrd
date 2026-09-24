use arrow::array::{Array, ArrayRef, Float64Array, StringArray};
use arrow::compute::cast;
use arrow::record_batch::RecordBatch;
use arrow_schema::DataType;

use wyrd_spec::ids::FeatureName;

use crate::error::DriftFitError;

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

    /// Cast and return every row as `f64`, `None` where the row is null.
    ///
    /// Returns [`FeatureLookupError`] if the column cannot be cast to `Float64`.
    pub fn collect_f64(&self) -> Result<Vec<Option<f64>>, FeatureLookupError> {
        let casted = cast(self.array, &DataType::Float64).map_err(|_| FeatureLookupError)?;
        let arr = casted
            .as_any()
            .downcast_ref::<Float64Array>()
            .ok_or(FeatureLookupError)?;
        Ok(arr.iter().collect())
    }

    /// Cast and return every row as a string, `None` where the row is null.
    ///
    /// Returns [`FeatureLookupError`] if the column cannot be cast to `Utf8`.
    pub fn collect_string(&self) -> Result<Vec<Option<String>>, FeatureLookupError> {
        let casted = cast(self.array, &DataType::Utf8).map_err(|_| FeatureLookupError)?;
        let arr = casted
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or(FeatureLookupError)?;
        Ok(arr.iter().map(|value| value.map(str::to_owned)).collect())
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

/// One configured feature's target column, row-aligned with its batch.
///
/// Target scoring first decides which rows are observations of the monitored
/// features and whether each carries every configured feature; only then are
/// the values counted or grouped. A column absent from the batch is carried by
/// no row.
#[derive(Debug, Clone, PartialEq)]
pub enum TargetColumn {
    /// The batch has no column of this feature.
    Absent,
    /// A numeric feature's values; a non-finite value is invalid.
    Numeric(Vec<Option<f64>>),
    /// A categorical feature's labels.
    Categorical(Vec<Option<String>>),
}

impl TargetColumn {
    /// Whether `row` carries this feature at all (a null carries nothing).
    fn carried(&self, row: usize) -> bool {
        match self {
            Self::Absent => false,
            Self::Numeric(values) => values[row].is_some(),
            Self::Categorical(values) => values[row].is_some(),
        }
    }

    /// Whether `row` carries a scorable value: present and, if numeric, finite.
    fn valid(&self, row: usize) -> bool {
        match self {
            Self::Absent => false,
            Self::Numeric(values) => values[row].is_some_and(f64::is_finite),
            Self::Categorical(values) => values[row].is_some(),
        }
    }

    /// Every carried numeric value in row order.
    pub fn numeric_values(&self) -> Vec<f64> {
        match self {
            Self::Numeric(values) => values.iter().flatten().copied().collect(),
            Self::Absent | Self::Categorical(_) => Vec::new(),
        }
    }
}

/// Whether every selected row of `rows` carries a valid value in every column.
///
/// A row is selected when it carries at least one of `columns`; unrelated
/// rows carry none and never enter the comparison. A selected row missing a
/// configured feature, or holding a null or non-finite value where another
/// feature is present, makes the target incomplete: the caller reports it
/// unscored rather than dropping or imputing the row.
pub fn target_complete(rows: usize, columns: &[&TargetColumn]) -> bool {
    (0..rows).all(|row| {
        !columns.iter().any(|column| column.carried(row))
            || columns.iter().all(|column| column.valid(row))
    })
}

/// Require a non-empty baseline column with no null row.
///
/// # Errors
/// Returns [`DriftFitError::FeatureEmpty`] for no rows and
/// [`DriftFitError::NullValuesInColumn`] for any null.
pub fn required_values<T>(
    column: &ColumnRef<'_>,
    values: Vec<Option<T>>,
) -> Result<Vec<T>, DriftFitError> {
    if values.is_empty() {
        return Err(DriftFitError::FeatureEmpty {
            feature: column.name.to_string(),
        });
    }
    values
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| DriftFitError::NullValuesInColumn {
            feature: column.name.to_string(),
        })
}
