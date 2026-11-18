//! Typed answers of the `ui-sql` statements: each column's name, Arrow type
//! and field metadata, and each cell's exact typed value, so strings, IDs,
//! nulls, timestamps, decimals and Variant values compare exactly where the
//! integer-only checker of the other read cases cannot.

use std::collections::BTreeMap;

use arrow::array::RecordBatch;
use arrow::datatypes::SchemaRef;
use arrow::error::ArrowError;
use arrow::util::display::{ArrayFormatter, FormatOptions};
use wyrd_queue::variant::variant_cell_to_json;
use wyrd_types::variant::is_variant;

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

/// How much of an answer's row order its statement's SQL decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order<'a> {
    /// The `ORDER BY` decides every row's position.
    Total,
    /// Only the sequence of these key columns is decided: rows sharing
    /// every key may come in any order, and with no keys the whole answer
    /// is unordered.
    Keys(&'a [&'a str]),
    /// As [`Order::Keys`] under a `LIMIT` of `limit` rows: an answer
    /// that reaches it may have cut its last run of equal keys, so that run
    /// may hold any of the rows sharing its keys.
    Cut {
        /// The key columns.
        keys: &'a [&'a str],
        /// The statement's `LIMIT`.
        limit: usize,
    },
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
    /// The answer `batches` of `schema` hold, in stream order, with each
    /// Variant column shown as its JSON value, so one object compares equal
    /// however its fields are laid out.
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
                let mut cells = Vec::with_capacity(batch.num_columns());
                for ((column, formatter), field) in
                    batch.columns().iter().zip(&formatters).zip(schema.fields())
                {
                    cells.push(if column.is_null(row) {
                        None
                    } else if is_variant(field) {
                        let json = variant_cell_to_json(column.as_ref(), row).map_err(|error| {
                            ArrowError::InvalidArgumentError(format!("{}: {error:?}", field.name()))
                        })?;
                        Some(json.to_string())
                    } else {
                        Some(formatter.value(row).to_string())
                    });
                }
                rows.push(cells);
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
    /// the same row count, and every cell equal, in the row order `order`
    /// says the statement decides: rows sharing their keys compare as a
    /// multiset, and a cut last run compares only its keys.
    ///
    /// # Errors
    ///
    /// Returns the first difference, named by column and row.
    pub fn matches(&self, expected: &Self, order: Order<'_>) -> Result<(), String> {
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
        let width = self.columns.len();
        if let Some(row) = self
            .rows
            .iter()
            .chain(&expected.rows)
            .find(|row| row.len() != width)
        {
            return Err(format!(
                "a row holds {} cells, expected {width} columns",
                row.len()
            ));
        }
        let keys = match order {
            Order::Total => return self.same_rows(&self.rows, &expected.rows, 0),
            Order::Keys(keys) | Order::Cut { keys, .. } => keys
                .iter()
                .map(|key| {
                    self.position(key)
                        .ok_or_else(|| format!("no key column `{key}`"))
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        let key =
            |row: &[Option<String>]| keys.iter().map(|at| row[*at].clone()).collect::<Vec<_>>();
        if let Some(index) = (0..self.rows.len())
            .find(|index| key(&self.rows[*index]) != key(&expected.rows[*index]))
        {
            return Err(format!(
                "row {index} keys {:?}, expected {:?}",
                key(&self.rows[index]),
                key(&expected.rows[index])
            ));
        }
        let mut start = 0;
        while start < expected.rows.len() {
            let run = key(&expected.rows[start]);
            let end = (start..expected.rows.len())
                .find(|index| key(&expected.rows[*index]) != run)
                .unwrap_or(expected.rows.len());
            let cut = matches!(order, Order::Cut { limit, .. } if limit == expected.rows.len());
            if !(cut && end == expected.rows.len()) {
                let mut actual: Vec<&Vec<Option<String>>> = self.rows[start..end].iter().collect();
                let mut wanted: Vec<&Vec<Option<String>>> =
                    expected.rows[start..end].iter().collect();
                actual.sort_unstable();
                wanted.sort_unstable();
                self.same_rows(actual, wanted, start)?;
            }
            start = end;
        }
        Ok(())
    }

    /// Checks `actual` against `wanted` row by row and cell by cell,
    /// naming a difference by its row offset from `first` and its column.
    ///
    /// # Errors
    ///
    /// Returns the first differing cell.
    fn same_rows<'r>(
        &self,
        actual: impl IntoIterator<Item = &'r Vec<Option<String>>>,
        wanted: impl IntoIterator<Item = &'r Vec<Option<String>>>,
        first: usize,
    ) -> Result<(), String> {
        for (index, (row, want)) in actual.into_iter().zip(wanted).enumerate() {
            if let Some(((cell, want), column)) = row
                .iter()
                .zip(want)
                .zip(&self.columns)
                .find(|((cell, want), _)| cell != want)
            {
                return Err(format!(
                    "row {} column `{}`: {cell:?}, expected {want:?}",
                    first + index,
                    column.name
                ));
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
        ArrayRef, Decimal128Array, Int64Array, RecordBatch, StringArray, TimestampNanosecondArray,
    };
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};

    use super::{Answer, Column, Order};

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
            Arc::new(TimestampNanosecondArray::from(vec![1, nanos]).with_timezone("UTC")),
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
    /// decimal, null, type, or row count each fail.
    ///
    /// # Panics
    ///
    /// Panics when the checker accepts a wrong answer or refuses a right one.
    #[test]
    fn typed_answers_compare_ids_timestamps_decimals_and_nulls() {
        let expected = answer("0b", 2, 7, None);
        assert_eq!(
            answer("0b", 2, 7, None).matches(&expected, Order::Total),
            Ok(())
        );
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
            assert!(wrong.matches(&expected, Order::Total).is_err(), "{wrong:?}");
        }
        let mut retyped = expected.clone();
        retyped.columns[3].data_type = DataType::Int32.to_string();
        assert!(retyped.matches(&expected, Order::Total).is_err());
        let mut short = expected.clone();
        short.rows.pop();
        assert!(short.matches(&expected, Order::Total).is_err());
        let mut narrow = expected.clone();
        narrow.rows[1].pop();
        assert!(narrow.matches(&expected, Order::Total).is_err());
        assert!(expected.matches(&narrow, Order::Total).is_err());
    }

    /// An answer of `at`, `level` and `n` columns holding `rows`.
    fn timeline(rows: &[(&str, &str, &str)]) -> Answer {
        Answer {
            columns: ["at", "level", "n"]
                .map(|name| Column {
                    name: name.to_owned(),
                    data_type: "Utf8".to_owned(),
                    metadata: std::collections::BTreeMap::new(),
                })
                .to_vec(),
            rows: rows
                .iter()
                .map(|(at, level, n)| {
                    vec![
                        Some((*at).to_owned()),
                        Some((*level).to_owned()),
                        Some((*n).to_owned()),
                    ]
                })
                .collect(),
        }
    }

    /// Rows sharing their keys may come in any order, but keys keep their
    /// sequence and every row's cells must still match; with no keys the
    /// whole answer is a multiset; a last run cut at the limit need only keep
    /// its keys, while one short of the limit is whole.
    ///
    /// # Panics
    ///
    /// Panics when the checker accepts a wrong answer or refuses a right one.
    #[test]
    fn ties_compare_as_multisets_within_their_keys() {
        let expected = timeline(&[("1", "info", "4"), ("1", "error", "2"), ("2", "info", "3")]);
        let swapped = timeline(&[("1", "error", "2"), ("1", "info", "4"), ("2", "info", "3")]);
        assert!(swapped.matches(&expected, Order::Total).is_err());
        assert_eq!(swapped.matches(&expected, Order::Keys(&["at"])), Ok(()));
        let crossed = timeline(&[("2", "info", "3"), ("1", "error", "2"), ("1", "info", "4")]);
        assert!(crossed.matches(&expected, Order::Keys(&["at"])).is_err());
        assert_eq!(crossed.matches(&expected, Order::Keys(&[])), Ok(()));
        let miscounted = timeline(&[("1", "error", "2"), ("1", "info", "5"), ("2", "info", "3")]);
        assert!(miscounted.matches(&expected, Order::Keys(&["at"])).is_err());
        assert!(miscounted.matches(&expected, Order::Keys(&[])).is_err());
        let cut = timeline(&[("1", "info", "4"), ("1", "error", "2"), ("2", "warn", "9")]);
        assert_eq!(
            cut.matches(
                &expected,
                Order::Cut {
                    keys: &["at"],
                    limit: 3
                }
            ),
            Ok(())
        );
        assert!(cut.matches(&expected, Order::Keys(&["at"])).is_err());
        assert!(
            cut.matches(
                &expected,
                Order::Cut {
                    keys: &["at"],
                    limit: 4
                }
            )
            .is_err()
        );
        assert!(
            miscounted
                .matches(
                    &expected,
                    Order::Cut {
                        keys: &["at"],
                        limit: 3
                    }
                )
                .is_err()
        );
        let rekeyed = timeline(&[("1", "info", "4"), ("1", "error", "2"), ("3", "info", "3")]);
        assert!(
            rekeyed
                .matches(
                    &expected,
                    Order::Cut {
                        keys: &["at"],
                        limit: 3
                    }
                )
                .is_err()
        );
    }

    /// The one-row answer of an `attributes` Variant column holding `json`.
    fn attributes(json: &str) -> Answer {
        let text: ArrayRef = Arc::new(StringArray::from(vec![json]));
        let variant = parquet_variant_compute::json_to_variant(&text).expect("variant");
        let schema = Arc::new(Schema::new(vec![variant.field("attributes")]));
        let batch = RecordBatch::try_new(Arc::clone(&schema), vec![ArrayRef::from(variant)])
            .expect("batch");
        Answer::from_batches(&schema, &[batch]).expect("answer")
    }

    /// One Variant object written with its fields in two physical orders
    /// is one answer, while a different value is not.
    ///
    /// # Panics
    ///
    /// Panics when the checker compares Variant bytes rather than values.
    #[test]
    fn variants_compare_by_value_not_layout() {
        let expected = attributes(r#"{"route":"/api/cart","status":200}"#);
        let reordered = attributes(r#"{"status":200,"route":"/api/cart"}"#);
        assert_eq!(reordered.matches(&expected, Order::Total), Ok(()));
        let wrong = attributes(r#"{"route":"/api/cart","status":404}"#);
        assert!(wrong.matches(&expected, Order::Total).is_err());
    }
}
