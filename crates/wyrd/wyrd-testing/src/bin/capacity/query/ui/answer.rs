//! Typed answers of the `ui-sql` statements: each column's name, Arrow type
//! and field metadata, and each cell's exact typed value, so strings, IDs,
//! nulls, timestamps, decimals and Variant values compare exactly where the
//! integer-only checker of the other read cases cannot.

use std::collections::BTreeMap;

use arrow::array::RecordBatch;
use arrow::datatypes::SchemaRef;
use arrow::error::ArrowError;
use arrow::util::display::{ArrayFormatter, FormatOptions};

/// One result column: its name, exact Arrow type and field metadata, which
/// carries a Variant's extension identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// The column name.
    pub name: String,
    /// The Arrow type, as Arrow writes it.
    pub data_type: String,
    /// The field metadata.
    pub metadata: BTreeMap<String, String>,
}

/// A whole query answer in row order: its columns and each cell's typed
/// value as Arrow displays it, `None` for null.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// The result columns.
    pub columns: Vec<Column>,
    /// The rows, in the order the stream returned them.
    pub rows: Vec<Vec<Option<String>>>,
}

impl Answer {
    /// The answer `batches` of `schema` hold, in stream order.
    ///
    /// # Errors
    ///
    /// Returns the Arrow error of a column Arrow cannot display.
    pub fn from_batches(schema: &SchemaRef, batches: &[RecordBatch]) -> Result<Self, ArrowError> {
        let columns = schema
            .fields()
            .iter()
            .map(|field| Column {
                name: field.name().clone(),
                data_type: field.data_type().to_string(),
                metadata: field.metadata().clone().into_iter().collect(),
            })
            .collect();
        let options = FormatOptions::default().with_null("\u{0}");
        let mut rows = Vec::new();
        for batch in batches {
            let formatters = batch
                .columns()
                .iter()
                .map(|column| ArrayFormatter::try_new(column.as_ref(), &options))
                .collect::<Result<Vec<_>, _>>()?;
            for row in 0..batch.num_rows() {
                rows.push(
                    batch
                        .columns()
                        .iter()
                        .zip(&formatters)
                        .map(|(column, formatter)| {
                            (!column.is_null(row)).then(|| formatter.value(row).to_string())
                        })
                        .collect(),
                );
            }
        }
        Ok(Self { columns, rows })
    }

    /// The position of column `name`.
    #[must_use]
    pub fn position(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|column| column.name == name)
    }

    /// Every cell of column `name`, in row order; empty when absent.
    #[must_use]
    pub fn column(&self, name: &str) -> Vec<Option<&str>> {
        self.position(name).map_or_else(Vec::new, |index| {
            self.rows
                .iter()
                .map(|row| row.get(index).and_then(Option::as_deref))
                .collect()
        })
    }

    /// Checks this answer against `expected`: the same columns and types,
    /// the same row count, and every cell equal except in `approximate`
    /// columns, whose numeric values must lie within their relative
    /// tolerance of the expected value, with nulls matching exactly.
    ///
    /// # Errors
    ///
    /// Returns the first difference, named by column and row.
    pub fn matches(&self, expected: &Self, approximate: &[(&str, f64)]) -> Result<(), String> {
        fn shape(answer: &Answer) -> Vec<(&str, &str)> {
            answer
                .columns
                .iter()
                .map(|column| (column.name.as_str(), column.data_type.as_str()))
                .collect()
        }
        if shape(self) != shape(expected) {
            return Err(format!(
                "columns {:?}, expected {:?}",
                shape(self),
                shape(expected)
            ));
        }
        if let Some(column) = self
            .columns
            .iter()
            .zip(&expected.columns)
            .find(|(actual, wanted)| actual.metadata != wanted.metadata)
        {
            return Err(format!("column `{}` metadata differs", column.0.name));
        }
        if self.rows.len() != expected.rows.len() {
            return Err(format!(
                "{} rows, expected {}",
                self.rows.len(),
                expected.rows.len()
            ));
        }
        for (index, (row, wanted)) in self.rows.iter().zip(&expected.rows).enumerate() {
            if row.len() != self.columns.len() || wanted.len() != self.columns.len() {
                return Err(format!(
                    "row {index} holds {} cells, expected {} of {} columns",
                    row.len(),
                    wanted.len(),
                    self.columns.len()
                ));
            }
            for ((cell, want), column) in row.iter().zip(wanted).zip(&self.columns) {
                let tolerance = approximate
                    .iter()
                    .find(|(name, _)| *name == column.name)
                    .map(|(_, tolerance)| *tolerance);
                let equal = match (cell, want, tolerance) {
                    (Some(cell), Some(want), Some(tolerance)) => {
                        match (cell.parse::<f64>(), want.parse::<f64>()) {
                            (Ok(cell), Ok(want)) => {
                                (cell - want).abs() <= tolerance * want.abs().max(f64::MIN_POSITIVE)
                            }
                            _ => false,
                        }
                    }
                    _ => cell == want,
                };
                if !equal {
                    return Err(format!(
                        "row {index} column `{}`: {cell:?}, expected {want:?}",
                        column.name
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! The typed checker keeps what the integer checker cannot.

    use std::sync::Arc;

    use arrow::array::{
        ArrayRef, Decimal128Array, Int64Array, RecordBatch, StringArray,
        TimestampNanosecondArray,
    };
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};

    use super::Answer;

    /// A two-row answer of a string ID, a UTC timestamp, a decimal, and a
    /// nullable count, with `row` deciding the second row's values.
    fn answer(id: &str, nanos: i64, decimal: i128, count: Option<i64>) -> Answer {
        let schema = Arc::new(Schema::new(vec![
            Field::new("trace_id", DataType::Utf8, false),
            Field::new(
                "at",
                DataType::Timestamp(TimeUnit::Nanosecond, Some("UTC".into())),
                false,
            ),
            Field::new("cost", DataType::Decimal128(38, 12), false),
            Field::new("n", DataType::Int64, true),
        ]));
        let columns: Vec<ArrayRef> = vec![
            Arc::new(StringArray::from(vec!["0a", id])),
            Arc::new(
                TimestampNanosecondArray::from(vec![1, nanos]).with_timezone("UTC"),
            ),
            Arc::new(
                Decimal128Array::from(vec![5, decimal])
                    .with_precision_and_scale(38, 12)
                    .expect("decimal"),
            ),
            Arc::new(Int64Array::from(vec![Some(1), count])),
        ];
        let batch = RecordBatch::try_new(Arc::clone(&schema), columns).expect("batch");
        Answer::from_batches(&schema, &[batch]).expect("answer")
    }

    /// Identical typed answers match; a different string, timestamp,
    /// decimal, null, type, or row count each fail, and an approximate
    /// column passes only within its tolerance.
    ///
    /// # Panics
    ///
    /// Panics when the checker accepts a wrong answer or refuses a right one.
    #[test]
    fn typed_answers_compare_ids_timestamps_decimals_and_nulls() {
        let expected = answer("0b", 2, 7, None);
        assert_eq!(answer("0b", 2, 7, None).matches(&expected, &[]), Ok(()));
        assert_eq!(
            expected.column("cost"),
            vec![Some("0.000000000005"), Some("0.000000000007")]
        );
        for wrong in [
            answer("0c", 2, 7, None),
            answer("0b", 3, 7, None),
            answer("0b", 2, 8, None),
            answer("0b", 2, 7, Some(0)),
        ] {
            assert!(wrong.matches(&expected, &[]).is_err(), "{wrong:?}");
        }
        let mut retyped = expected.clone();
        retyped.columns[3].data_type = DataType::Int32.to_string();
        assert!(retyped.matches(&expected, &[]).is_err());
        let mut short = expected.clone();
        short.rows.pop();
        assert!(short.matches(&expected, &[]).is_err());
        let mut narrow = expected.clone();
        narrow.rows[1].pop();
        assert!(narrow.matches(&expected, &[]).is_err());
        assert!(expected.matches(&narrow, &[]).is_err());
        let mut near = expected.clone();
        near.rows[0][3] = Some("1.04".to_owned());
        assert!(near.matches(&expected, &[("n", 0.05)]).is_ok());
        assert!(near.matches(&expected, &[("n", 0.01)]).is_err());
    }
}
