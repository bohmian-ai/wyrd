//! Owned rows of a server-built table, staged until a writer builds many of
//! them into one batch.
//!
//! A server-internal producer validates and encodes each value when it
//! stages the row, so the row holds plain scalars and already-encoded
//! Variants rather than a one-row Arrow batch, and [`StagedRow::project`]
//! appends a run of rows sharing one schema straight into one dense batch.

use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanBuilder, RecordBatch, StringBuilder, TimestampMicrosecondBuilder,
};
use arrow::datatypes::{DataType, SchemaRef, TimeUnit};
use arrow::error::ArrowError;
use wyrd_queue::variant::{EncodedVariant, VariantColumnBuilder};
use wyrd_types::variant::is_variant;

/// One staged column value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cell {
    /// A UTF-8 value, or null.
    Text(Option<Box<str>>),
    /// A boolean value.
    Flag(bool),
    /// A microsecond timestamp, or null; the column's declared type carries
    /// its time zone.
    Micros(Option<i64>),
    /// An already-validated Variant, or null.
    Variant(Option<EncodedVariant>),
}

impl Cell {
    /// A text cell holding `value`.
    #[must_use]
    pub fn text(value: Option<impl Into<Box<str>>>) -> Self {
        Self::Text(value.map(Into::into))
    }

    /// Whether this cell holds no value.
    fn is_null(&self) -> bool {
        matches!(
            self,
            Self::Text(None) | Self::Micros(None) | Self::Variant(None)
        )
    }

    /// Whether this cell's kind fills a column declared as `field`.
    fn fits(&self, field: &arrow::datatypes::Field) -> bool {
        match self {
            Self::Text(_) => field.data_type() == &DataType::Utf8,
            Self::Flag(_) => field.data_type() == &DataType::Boolean,
            Self::Micros(_) => matches!(
                field.data_type(),
                DataType::Timestamp(TimeUnit::Microsecond, _)
            ),
            Self::Variant(_) => is_variant(field),
        }
    }

    /// Heap bytes this cell owns.
    fn owned_bytes(&self) -> usize {
        match self {
            Self::Text(value) => value.as_ref().map_or(0, |value| value.len()),
            Self::Variant(value) => value.as_ref().map_or(0, EncodedVariant::encoded_bytes),
            Self::Flag(_) | Self::Micros(_) => 0,
        }
    }
}

/// One owned row of a server-built table, its cells in schema order.
///
/// Cloning shares the cells, so a writer can hand a run of staged rows to a
/// blocking task without copying their values.
#[derive(Debug, Clone)]
pub struct StagedRow {
    /// The schema the row fills; rows sharing it by pointer group together.
    schema: SchemaRef,
    /// One cell per schema field, in field order.
    cells: Arc<[Cell]>,
}

impl StagedRow {
    /// Stage `cells` as one row of `schema`.
    ///
    /// # Errors
    ///
    /// Returns [`ArrowError::SchemaError`] when the cells do not fill the
    /// schema's fields one for one, a cell's kind does not match its field's
    /// type, or a non-nullable field is null, which only a projection that
    /// drifted from its table declaration causes.
    pub fn new(schema: SchemaRef, cells: Vec<Cell>) -> Result<Self, ArrowError> {
        if cells.len() != schema.fields().len() {
            return Err(ArrowError::SchemaError(format!(
                "{} cells for {} fields",
                cells.len(),
                schema.fields().len()
            )));
        }
        for (field, cell) in schema.fields().iter().zip(&cells) {
            if !cell.fits(field) || (cell.is_null() && !field.is_nullable()) {
                return Err(ArrowError::SchemaError(format!(
                    "column {} cannot hold {cell:?}",
                    field.name()
                )));
            }
        }
        Ok(Self {
            schema,
            cells: cells.into(),
        })
    }

    /// The schema this row fills.
    #[must_use]
    pub fn schema(&self) -> &SchemaRef {
        &self.schema
    }

    /// The value of text column `name`, when the row has that column and it
    /// is not null.
    #[must_use]
    pub fn text(&self, name: &str) -> Option<&str> {
        match self.cells.get(self.schema.index_of(name).ok()?)? {
            Cell::Text(value) => value.as_deref(),
            _ => None,
        }
    }

    /// Memory this row holds while it is staged: its cells and their values.
    #[must_use]
    pub fn owned_bytes(&self) -> usize {
        std::mem::size_of_val(&*self.cells)
            + self.cells.iter().map(Cell::owned_bytes).sum::<usize>()
    }

    /// Build `rows`, which share the first row's schema, into one batch,
    /// each column allocated once for its values.
    ///
    /// # Errors
    ///
    /// Returns [`ArrowError::SchemaError`] when `rows` is empty or a row
    /// fills another schema, and the Arrow error when the batch cannot be
    /// assembled.
    pub fn project(rows: &[Self]) -> Result<RecordBatch, ArrowError> {
        let schema = rows
            .first()
            .map(|row| Arc::clone(&row.schema))
            .ok_or_else(|| ArrowError::SchemaError("no rows to project".to_owned()))?;
        if rows
            .iter()
            .any(|row| !Arc::ptr_eq(&row.schema, &schema) && row.schema != schema)
        {
            return Err(ArrowError::SchemaError(
                "staged rows of different schemas".to_owned(),
            ));
        }
        let columns = schema
            .fields()
            .iter()
            .enumerate()
            .map(|(index, field)| Self::column(rows, index, field.data_type()))
            .collect::<Vec<_>>();
        RecordBatch::try_new(schema, columns)
    }

    /// Column `index` of `rows`, declared as `data_type`.
    ///
    /// [`Self::new`] checked every cell against its field, so a column's
    /// cells are all of the kind its first row's is.
    ///
    /// # Panics
    ///
    /// Panics when `rows` is empty or a row has no cell at `index`; callers
    /// pass a non-empty run of one schema's rows.
    fn column(rows: &[Self], index: usize, data_type: &DataType) -> ArrayRef {
        let cells = rows.iter().map(|row| &row.cells[index]);
        match &rows[0].cells[index] {
            Cell::Text(_) => {
                let bytes = cells.clone().map(Cell::owned_bytes).sum();
                let mut column = StringBuilder::with_capacity(rows.len(), bytes);
                for cell in cells {
                    if let Cell::Text(value) = cell {
                        column.append_option(value.as_deref());
                    }
                }
                Arc::new(column.finish())
            }
            Cell::Flag(_) => {
                let mut column = BooleanBuilder::with_capacity(rows.len());
                for cell in cells {
                    if let Cell::Flag(value) = cell {
                        column.append_value(*value);
                    }
                }
                Arc::new(column.finish())
            }
            Cell::Micros(_) => {
                let mut column = TimestampMicrosecondBuilder::with_capacity(rows.len())
                    .with_data_type(data_type.clone());
                for cell in cells {
                    if let Cell::Micros(value) = cell {
                        column.append_option(*value);
                    }
                }
                Arc::new(column.finish())
            }
            Cell::Variant(_) => {
                let (metadata, value) =
                    cells
                        .clone()
                        .fold((0, 0), |(metadata, value), cell| match cell {
                            Cell::Variant(Some(variant)) => (
                                metadata + variant.metadata().len(),
                                value + variant.value().len(),
                            ),
                            _ => (metadata, value),
                        });
                let mut column =
                    VariantColumnBuilder::with_byte_capacity(rows.len(), metadata, value);
                for cell in cells {
                    if let Cell::Variant(value) = cell {
                        column.append_option(value.as_ref());
                    }
                }
                column.finish()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arrow::array::{Array as _, AsArray as _};
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use wyrd_queue::variant::{EncodedVariant, variant_cell_to_json};
    use wyrd_types::variant::variant_field;

    use super::{Cell, StagedRow};

    /// Proves staged rows build into one batch holding every value, null,
    /// and Variant in row order, with each text column sized to its values,
    /// and that a mismatched row is refused when staged.
    ///
    /// # Panics
    ///
    /// Panics when a value, null, or capacity differs, or a mismatch is
    /// staged.
    #[test]
    fn staged_rows_project_into_one_dense_batch() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("name", DataType::Utf8, true),
            Field::new("flag", DataType::Boolean, false),
            Field::new(
                "at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                true,
            ),
            variant_field("detail", true),
        ]));
        let detail =
            EncodedVariant::from_json(&serde_json::json!({"k": ["v", 1]})).expect("encodes");
        let rows = [
            vec![
                Cell::text(Some("ünïcode")),
                Cell::Flag(true),
                Cell::Micros(Some(7)),
                Cell::Variant(Some(detail.clone())),
            ],
            vec![
                Cell::text(None::<&str>),
                Cell::Flag(false),
                Cell::Micros(None),
                Cell::Variant(None),
            ],
            vec![
                Cell::text(Some("")),
                Cell::Flag(true),
                Cell::Micros(Some(9)),
                Cell::Variant(None),
            ],
        ]
        .map(|cells| StagedRow::new(Arc::clone(&schema), cells).expect("row stages"));

        let batch = StagedRow::project(&rows).expect("rows project");

        assert_eq!(batch.schema(), schema);
        let names = batch.column(0).as_string::<i32>();
        assert_eq!(
            names.iter().collect::<Vec<_>>(),
            [Some("ünïcode"), None, Some("")]
        );
        assert!(names.get_buffer_memory_size() < 256, "sized to its values");
        let flags = batch.column(1).as_boolean();
        assert_eq!(
            flags.iter().collect::<Vec<_>>(),
            [Some(true), Some(false), Some(true)]
        );
        let at = batch
            .column(2)
            .as_primitive::<arrow::datatypes::TimestampMicrosecondType>();
        assert_eq!(at.iter().collect::<Vec<_>>(), [Some(7), None, Some(9)]);
        assert_eq!(
            variant_cell_to_json(batch.column(3).as_ref(), 0).expect("renders"),
            serde_json::json!({"k": ["v", 1]})
        );
        assert!(batch.column(3).is_null(1) && batch.column(3).is_null(2));
        assert_eq!(rows[0].text("name"), Some("ünïcode"));
        assert_eq!(rows[1].text("name"), None);
        assert!(rows[0].owned_bytes() >= "ünïcode".len() + detail.encoded_bytes());

        assert!(StagedRow::new(Arc::clone(&schema), vec![Cell::Flag(true)]).is_err());
        assert!(
            StagedRow::new(
                Arc::clone(&schema),
                vec![
                    Cell::Flag(true),
                    Cell::Flag(true),
                    Cell::Micros(None),
                    Cell::Variant(None)
                ],
            )
            .is_err(),
            "a cell of the wrong kind is refused"
        );
        assert!(
            StagedRow::new(
                schema,
                vec![
                    Cell::text(None::<&str>),
                    Cell::Micros(None),
                    Cell::Micros(None),
                    Cell::Variant(None)
                ],
            )
            .is_err(),
            "a null in a required column is refused"
        );
    }
}
