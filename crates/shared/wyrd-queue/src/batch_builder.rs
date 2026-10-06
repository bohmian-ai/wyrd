//! JSON rows → one validated user-only `RecordBatch` plus per-row correlation.
//!
//! [`RowPreflight`] is the single prepared-input boundary for row writes: it
//! converts a complete input against the destination schema before any queue
//! or budget state is touched, and yields [`PreparedRows`] whose exact charge
//! the producer reserves once. The client batch carries **user columns plus
//! the two per-row correlation columns `card_ref` and `run_id`**; the server
//! stamps the per-request system columns.

use std::collections::BTreeMap;
use std::sync::Arc;

use arrow::array::{Array, AsArray, new_null_array};
use arrow::array::{
    ArrayRef, BooleanArray, Date32Array, FixedSizeBinaryArray, Float32Array, Float64Array,
    Int8Array, Int16Array, Int32Array, Int64Array, LargeStringArray, ListArray, RecordBatch,
    StringArray, StructArray, TimestampMicrosecondArray, UInt8Array, UInt16Array, UInt32Array,
    UInt64Array,
};
use arrow::buffer::{NullBuffer, OffsetBuffer};
use arrow::compute::cast;
use arrow_schema::{ArrowError, DataType, Field, FieldRef, Fields, Schema, SchemaRef, TimeUnit};
use serde_json::Value;
use serde_json::value::RawValue;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::BifrostTableDescription;
use wyrd_spec::vala::ids::RunId;

use crate::error::WyrdQueueError;
use crate::queue::Row;
use crate::schema::first_duplicate;
use crate::sealed_sender::encode_ipc;
use crate::variant::{EncodedVariant, VariantColumnBuilder, is_variant, variant_field};

/// Reserved per-row correlation column carrying the client's card reference.
pub const CARD_REF_COLUMN: &str = "card_ref";
/// Reserved per-row correlation column carrying the client's run identifier.
pub const RUN_ID_COLUMN: &str = "run_id";
const RESERVED_PREFIX: &str = "wyrd_";

/// Returns true when `name` is a reserved column (server-stamped `wyrd_*` prefix
/// or one of the two correlation columns presented as a payload key).
#[must_use]
pub fn is_reserved_column(name: &str) -> bool {
    name.starts_with(RESERVED_PREFIX) || name == CARD_REF_COLUMN || name == RUN_ID_COLUMN
}

/// Converts complete row inputs against one destination schema.
///
/// The preflight owns the resolved user-only schema and the output schema
/// that appends the correlation columns. It is the one owner of the column
/// rules: [`Self::prepare`] applies them to JSON rows for `insert`, and
/// [`Self::prepare_batch`] to Arrow batches for `write_batch`, through the
/// same refusal and Variant-encoding helpers. Both are synchronous and mutate
/// nothing, so every refusal happens before admission.
#[derive(Debug, Clone)]
pub struct RowPreflight {
    /// Declared user columns, in logical-schema order.
    fields: Fields,
    /// User columns followed by `card_ref` and `run_id`.
    output: SchemaRef,
}

/// A fully converted row input, ready for one reservation and one hand-over.
///
/// Holding a `PreparedRows` means every row passed every check; nothing about
/// it can fail after the producer reserves its charge.
#[derive(Debug)]
pub struct PreparedRows {
    /// The converted rows over [`RowPreflight::output_schema`].
    batch: RecordBatch,
    /// Exact admission charge: the arrays plus one row owner per row.
    charge: usize,
}

impl PreparedRows {
    /// The converted rows.
    #[must_use]
    pub fn batch(&self) -> &RecordBatch {
        &self.batch
    }

    /// The exact bytes admission reserves for these rows.
    #[must_use]
    pub fn charge(&self) -> usize {
        self.charge
    }

    /// Split into the batch and its charge for admission.
    pub(crate) fn into_parts(self) -> (RecordBatch, usize) {
        (self.batch, self.charge)
    }

    /// Encode the rows as one Arrow IPC stream for a direct send.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::SchemaParse`] when the IPC writer fails.
    pub fn to_ipc(&self) -> Result<Vec<u8>, WyrdQueueError> {
        encode_ipc(&self.batch, usize::MAX)
    }
}

impl RowPreflight {
    /// Construct a preflight over the resolved user-only Arrow schema.
    #[must_use]
    pub fn new(schema: &Schema) -> Self {
        let fields = schema.fields().clone();
        let mut output: Vec<FieldRef> = fields.iter().cloned().collect();
        output.push(Arc::new(Field::new(CARD_REF_COLUMN, DataType::Utf8, true)));
        output.push(Arc::new(Field::new(RUN_ID_COLUMN, DataType::Utf8, true)));
        Self {
            fields,
            output: Arc::new(Schema::new(output)),
        }
    }

    /// Construct a preflight from one describe response.
    ///
    /// Only `user_fields` become converted columns: the correlation columns
    /// are appended once by [`Self::new`], and the managed candidates are
    /// omitted because this JSON-row API takes no timestamp argument and lets
    /// the server stamp event time.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::SchemaParse`] when a described user column is
    /// not representable as Arrow, and [`WyrdQueueError::ReservedColumn`] when
    /// the description carries a server-owned column among its user fields.
    pub fn from_description(description: &BifrostTableDescription) -> Result<Self, WyrdQueueError> {
        if let Some(spec) = description
            .user_fields
            .iter()
            .find(|spec| is_reserved_column(&spec.name))
        {
            return Err(WyrdQueueError::ReservedColumn(format!(
                "described user column `{}` is server-owned",
                spec.name
            )));
        }
        Ok(Self::new(&crate::schema::fieldspec_to_arrow(
            &description.user_fields,
        )?))
    }

    /// The prepared batch schema: user fields followed by the two correlation
    /// columns.
    #[must_use]
    pub fn output_schema(&self) -> &SchemaRef {
        &self.output
    }

    /// Validate and convert a complete row input, all or none.
    ///
    /// Each row must be a JSON object. Checks run row by row in input order;
    /// within a row an undeclared or reserved key is refused first, then each
    /// declared field in logical-schema order. Struct values recurse through
    /// their declared children, List values through their elements, and a
    /// Variant takes any JSON value through [`EncodedVariant::from_json_text`],
    /// so a string stays a Variant string and integers keep their exact
    /// digits. JSON `null` or an absent key is a null value. Every row carries
    /// the same `card_ref` and `run_id`.
    ///
    /// # Errors
    ///
    /// Returns the first refusal in that order:
    /// [`WyrdQueueError::ReservedColumn`] for a `wyrd_*`, `card_ref`, or
    /// `run_id` key, [`WyrdQueueError::Contract`] with
    /// `BIFROST_UNDECLARED_FIELD` for any other key the schema does not
    /// declare or with the catalogued Variant error for an unstorable Variant
    /// value, and [`WyrdQueueError::SchemaParse`] for invalid JSON, a value
    /// that does not convert to its column type, or a null on a non-nullable
    /// column.
    pub fn prepare(
        &self,
        rows: &[impl AsRef<[u8]>],
        card_ref: Option<&CardRef>,
        run_id: Option<&RunId>,
    ) -> Result<PreparedRows, WyrdQueueError> {
        let mut cells = Vec::with_capacity(rows.len());
        let mut failure = None;
        for (row, json) in rows.iter().enumerate() {
            match serde_json::from_slice::<&RawValue>(json.as_ref()) {
                Ok(raw) => cells.push(Cell::new(row, raw, false)),
                Err(error) => {
                    failure = Some(Failure::schema(
                        row,
                        format!("row {row} is not valid JSON: {error}"),
                    ));
                    break;
                }
            }
        }
        let (mut columns, _) = build_struct(&self.fields, "", &cells).map_err(|f| f.error)?;
        if let Some(failure) = failure {
            return Err(failure.error);
        }
        let card_ref = card_ref.map(ToString::to_string);
        let run_id = run_id.map(|run| run.as_str().to_owned());
        columns.push(Arc::new(StringArray::from(vec![card_ref; rows.len()])));
        columns.push(Arc::new(StringArray::from(vec![run_id; rows.len()])));
        let batch = RecordBatch::try_new(Arc::clone(&self.output), columns).map_err(|e| {
            WyrdQueueError::SchemaParse(format!("record batch assembly failed: {e}"))
        })?;
        let charge = batch
            .get_array_memory_size()
            .saturating_add(rows.len().saturating_mul(std::mem::size_of::<Row>()));
        Ok(PreparedRows { batch, charge })
    }

    /// Validate and conform one caller Arrow batch, all or none.
    ///
    /// The Arrow counterpart of [`Self::prepare`], with the same column
    /// rules and the same refusal order. Columns match by name, so a batch
    /// that names a column twice is refused before anything else is read. The
    /// output carries every declared column in declared order, then any
    /// reserved correlation or managed column the caller supplied, in input
    /// order, for the server to judge. An omitted nullable column becomes
    /// all-null with its declared type. A declared Variant column passes when
    /// it already carries the `arrow.parquet.variant` extension (the server
    /// validates its bytes) and is encoded from `Utf8`/`LargeUtf8` JSON text
    /// exactly as a row value is; a null text is a null Variant. Every
    /// supplied column takes its declared nullability, so a writer's
    /// default-nullable Arrow field fits a required column, and otherwise
    /// keeps its type, which the server checks.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::SchemaParse`] for a duplicated column name,
    /// then [`WyrdQueueError::Contract`] with `BIFROST_UNDECLARED_FIELD` at
    /// row 0 for a non-reserved column the schema does not declare. Otherwise
    /// returns the refusal on the earliest input row, ties going to the
    /// earlier declared field, as [`Self::prepare`] does:
    /// [`WyrdQueueError::SchemaParse`] for an omitted non-nullable column
    /// (row 0) or a null in one; `BIFROST_UNSUPPORTED_TYPE` (row 0) for a
    /// declared Variant column that is neither the extension nor text; or the
    /// catalogued Variant error for unstorable text. Returns
    /// [`WyrdQueueError::SchemaParse`] if Arrow cannot view or reassemble the
    /// columns.
    pub fn prepare_batch(&self, batch: &RecordBatch) -> Result<RecordBatch, WyrdQueueError> {
        let schema = batch.schema();
        if let Some(name) = first_duplicate(schema.fields().iter().map(|f| f.name().as_str())) {
            return Err(WyrdQueueError::SchemaParse(format!(
                "batch column `{name}` is supplied more than once"
            )));
        }
        if let Some(column) = schema
            .fields()
            .iter()
            .find(|f| !is_reserved_column(f.name()) && self.fields.find(f.name()).is_none())
        {
            return Err(undeclared(column.name().clone(), 0));
        }
        let mut fields = Vec::with_capacity(schema.fields().len());
        let mut columns = Vec::with_capacity(schema.fields().len());
        let mut failure = None;
        for declared in &self.fields {
            let conformed = match schema.index_of(declared.name()) {
                Ok(index) => conform_column(declared, schema.field(index), batch.column(index)),
                Err(_) if declared.is_nullable() => Ok((
                    Arc::clone(declared),
                    new_null_array(declared.data_type(), batch.num_rows()),
                )),
                Err(_) => Err(Failure {
                    row: 0,
                    error: missing_required(declared.name(), 0),
                }),
            };
            match conformed {
                Ok((field, column)) => {
                    fields.push(field);
                    columns.push(column);
                }
                Err(refused) => failure = Some(refused.after(failure)),
            }
        }
        if let Some(failure) = failure {
            return Err(failure.error);
        }
        for (field, column) in schema.fields().iter().zip(batch.columns()) {
            if is_reserved_column(field.name()) {
                fields.push(Arc::clone(field));
                columns.push(Arc::clone(column));
            }
        }
        RecordBatch::try_new(
            Arc::new(Schema::new_with_metadata(fields, schema.metadata().clone())),
            columns,
        )
        .map_err(arrow_failure)
    }
}

/// Conform one supplied batch column to its declared field.
///
/// The column takes the declared nullability. A Variant declaration supplied
/// as JSON text is encoded row by row into the extension; any other column
/// keeps its own type for the server to check.
///
/// # Errors
///
/// Returns the earliest-row refusal among a null on a non-nullable field and
/// the encoding of every text value, or `BIFROST_UNSUPPORTED_TYPE` at row 0
/// for a Variant declaration supplied as neither the extension nor text.
fn conform_column(
    declared: &Field,
    supplied: &Field,
    column: &ArrayRef,
) -> Result<(FieldRef, ArrayRef), Failure> {
    let missing = (!declared.is_nullable())
        .then(|| (0..column.len()).find(|&row| column.is_null(row)))
        .flatten()
        .map(|row| Failure {
            row,
            error: missing_required(declared.name(), row),
        });
    let conformed = if is_variant(declared) && !is_variant(supplied) {
        encode_text_column(declared, supplied, column)
    } else {
        Ok((
            Arc::new(supplied.clone().with_nullable(declared.is_nullable())),
            Arc::clone(column),
        ))
    };
    match (conformed, missing) {
        (Ok(conformed), None) => Ok(conformed),
        (Ok(_), Some(missing)) => Err(missing),
        (Err(failure), missing) => Err(failure.after(missing)),
    }
}

/// Encode a JSON-text column for a declared Variant field, a null text being
/// a null Variant.
///
/// # Errors
///
/// Returns `BIFROST_UNSUPPORTED_TYPE` at row 0 when the column is not
/// `Utf8`/`LargeUtf8`, and otherwise the catalogued Variant error for the
/// first unstorable text.
fn encode_text_column(
    declared: &Field,
    supplied: &Field,
    column: &ArrayRef,
) -> Result<(FieldRef, ArrayRef), Failure> {
    if !matches!(supplied.data_type(), DataType::Utf8 | DataType::LargeUtf8) {
        return Err(Failure {
            row: 0,
            error: WyrdQueueError::Contract(BifrostError::UnsupportedType {
                field: supplied.name().clone(),
                data_type: supplied.data_type().to_string(),
            }),
        });
    }
    let text = cast(column, &DataType::Utf8View).map_err(|e| Failure {
        row: 0,
        error: arrow_failure(e),
    })?;
    let text = text.as_string_view();
    let variants = (0..text.len())
        .map(|row| {
            (!text.is_null(row))
                .then(|| encode_variant(declared.name(), row, text.value(row)))
                .transpose()
                .map_err(|error| Failure { row, error })
        })
        .collect::<Result<VariantColumnBuilder, _>>()?;
    Ok((
        Arc::new(variant_field(declared.name(), declared.is_nullable())),
        variants.finish(),
    ))
}

/// The refusal for a key or column the destination does not declare.
///
/// Row insertion and Arrow batches share it, so both name the same code,
/// dotted path, and first offending row.
fn undeclared(path: String, row: usize) -> WyrdQueueError {
    WyrdQueueError::Contract(BifrostError::UndeclaredField {
        field: path,
        row: u64::try_from(row).unwrap_or(u64::MAX),
    })
}

/// The refusal for a missing value on a non-nullable field.
///
/// Row insertion raises it for a null or absent key, and Arrow batches for an
/// omitted required column at row 0, with the same message.
fn missing_required(path: &str, row: usize) -> WyrdQueueError {
    WyrdQueueError::SchemaParse(format!(
        "row {row} field `{path}`: null/absent value on a non-nullable column"
    ))
}

/// Encode one JSON text value for a Variant field at `path` and `row`.
///
/// Row insertion and Arrow JSON-text columns share it, so a value is stored
/// identically and refused with the same catalogued Variant error.
///
/// # Errors
///
/// Returns the catalogued Variant error naming `path` and `row`.
fn encode_variant(path: &str, row: usize, text: &str) -> Result<EncodedVariant, WyrdQueueError> {
    EncodedVariant::from_json_text(text).map_err(|violation| {
        WyrdQueueError::Contract(violation.into_error(path, u64::try_from(row).unwrap_or(u64::MAX)))
    })
}

/// Report an Arrow failure while viewing or reassembling conformed columns.
fn arrow_failure(error: ArrowError) -> WyrdQueueError {
    WyrdQueueError::SchemaParse(format!("batch conformance failed: {error}"))
}

/// One value of one input row on its way into a column.
#[derive(Debug, Clone, Copy)]
struct Cell<'a> {
    /// Zero-based input row the value came from.
    row: usize,
    /// The value's JSON text; `None` for JSON `null` or an absent key.
    raw: Option<&'a RawValue>,
    /// Whether an enclosing struct is null here, which excuses a missing
    /// value on a non-nullable child.
    masked: bool,
}

impl<'a> Cell<'a> {
    /// A present value, normalizing JSON `null` to a missing one.
    fn new(row: usize, raw: &'a RawValue, masked: bool) -> Self {
        Self {
            row,
            raw: (raw.get() != "null").then_some(raw),
            masked,
        }
    }
}

/// The earliest refusal one column found, with the input row that raised it.
#[derive(Debug)]
struct Failure {
    /// Zero-based input row of the refusal.
    row: usize,
    /// The refusal itself.
    error: WyrdQueueError,
}

impl Failure {
    /// A schema-parse refusal at `row`.
    fn schema(row: usize, message: String) -> Self {
        Self {
            row,
            error: WyrdQueueError::SchemaParse(message),
        }
    }

    /// Keep `earlier` unless `self` is on a strictly earlier row, so a tie
    /// goes to the check that ran first.
    fn after(self, earlier: Option<Self>) -> Self {
        match earlier {
            Some(earlier) if earlier.row <= self.row => earlier,
            _ => self,
        }
    }
}

/// Human label for a field path; the root has none.
fn label(path: &str) -> String {
    if path.is_empty() {
        "row".to_owned()
    } else {
        format!("field `{path}`")
    }
}

/// The dotted path of `name` under `parent`.
fn child_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}.{name}")
    }
}

/// Build one column for `field` from its cells.
///
/// # Errors
///
/// Returns the earliest-row refusal among a null on a non-nullable field and
/// the conversion of every present value.
fn build_column(field: &Field, path: &str, cells: &[Cell<'_>]) -> Result<ArrayRef, Failure> {
    let missing = (!field.is_nullable())
        .then(|| cells.iter().find(|cell| cell.raw.is_none() && !cell.masked))
        .flatten()
        .map(|cell| Failure {
            row: cell.row,
            error: missing_required(path, cell.row),
        });
    let built = if is_variant(field) {
        build_variant(path, cells)
    } else {
        match field.data_type() {
            DataType::Struct(children) => {
                build_struct(children, path, cells).and_then(|(columns, nulls)| {
                    assembled(
                        cells,
                        StructArray::try_new(children.clone(), columns, nulls),
                    )
                })
            }
            DataType::List(element) => build_list(element, path, cells),
            data_type => build_scalar(data_type, path, cells),
        }
    };
    match (built, missing) {
        (Ok(array), None) => Ok(array),
        (Ok(_), Some(missing)) => Err(missing),
        (Err(failure), missing) => Err(failure.after(missing)),
    }
}

/// Wrap an assembled nested array, which every earlier check makes valid.
///
/// # Errors
///
/// Returns a schema-parse refusal at the first cell's row when Arrow rejects
/// the assembly.
fn assembled<A: arrow::array::Array + 'static>(
    cells: &[Cell<'_>],
    array: Result<A, ArrowError>,
) -> Result<ArrayRef, Failure> {
    array.map(|array| Arc::new(array) as ArrayRef).map_err(|e| {
        let row = cells.first().map_or(0, |cell| cell.row);
        Failure::schema(row, format!("nested column assembly failed: {e}"))
    })
}

/// Build the children and validity of a struct (or the root row object).
///
/// Each present cell must be a JSON object whose keys are all declared; the
/// first row breaking that stops the scan, and children are then built from
/// the earlier rows only, so a child refusal wins only on an earlier row.
///
/// # Errors
///
/// Returns the earliest-row refusal: a non-object value, an undeclared or (at
/// the root) reserved key, or any child's refusal.
fn build_struct(
    fields: &Fields,
    path: &str,
    cells: &[Cell<'_>],
) -> Result<(Vec<ArrayRef>, Option<NullBuffer>), Failure> {
    let mut failure = None;
    let mut objects = Vec::with_capacity(cells.len());
    for cell in cells {
        let Some(raw) = cell.raw else {
            objects.push(None);
            continue;
        };
        match object_of(fields, path, cell.row, raw) {
            Ok(object) => objects.push(Some(object)),
            Err(refused) => {
                failure = Some(refused);
                break;
            }
        }
    }
    let cells = &cells[..objects.len()];
    let mut columns = Vec::with_capacity(fields.len());
    for field in fields {
        let child = cells
            .iter()
            .zip(&objects)
            .map(|(cell, object)| {
                let masked = cell.masked || object.is_none();
                match object.as_ref().and_then(|o| o.get(field.name().as_str())) {
                    Some(raw) => Cell::new(cell.row, raw, masked),
                    None => Cell {
                        row: cell.row,
                        raw: None,
                        masked,
                    },
                }
            })
            .collect::<Vec<_>>();
        match build_column(field, &child_path(path, field.name()), &child) {
            Ok(column) => columns.push(column),
            Err(refused) => failure = Some(refused.after(failure)),
        }
    }
    if let Some(failure) = failure {
        return Err(failure);
    }
    let nulls = objects
        .iter()
        .any(Option::is_none)
        .then(|| NullBuffer::from(objects.iter().map(Option::is_some).collect::<Vec<_>>()));
    Ok((columns, nulls))
}

/// Parse one struct value and refuse its first undeclared key.
///
/// # Errors
///
/// Returns a schema-parse refusal for a non-object, a reserved-column refusal
/// for a reserved key at the root, and `BIFROST_UNDECLARED_FIELD` naming the
/// dotted path for any other undeclared key, in key order.
fn object_of<'a>(
    fields: &Fields,
    path: &str,
    row: usize,
    raw: &'a RawValue,
) -> Result<BTreeMap<String, &'a RawValue>, Failure> {
    let object: BTreeMap<String, &RawValue> = serde_json::from_str(raw.get()).map_err(|_| {
        Failure::schema(
            row,
            format!("row {row} {}: value is not a JSON object", label(path)),
        )
    })?;
    if let Some(key) = object.keys().find(|key| fields.find(key).is_none()) {
        let error = if path.is_empty() && is_reserved_column(key) {
            WyrdQueueError::ReservedColumn(format!("payload key `{key}` is reserved"))
        } else {
            undeclared(child_path(path, key), row)
        };
        return Err(Failure { row, error });
    }
    Ok(object)
}

/// Build one List column from JSON arrays.
///
/// # Errors
///
/// Returns the earliest-row refusal: a non-array value or any element's
/// refusal.
///
/// # Panics
///
/// Panics if the total element count overflows the 32-bit List offsets.
fn build_list(element: &FieldRef, path: &str, cells: &[Cell<'_>]) -> Result<ArrayRef, Failure> {
    let mut failure = None;
    let mut lengths = Vec::with_capacity(cells.len());
    let mut items = Vec::new();
    for cell in cells {
        let values: Vec<&RawValue> = match cell.raw {
            None => Vec::new(),
            Some(raw) => match serde_json::from_str(raw.get()) {
                Ok(values) => values,
                Err(_) => {
                    failure = Some(Failure::schema(
                        cell.row,
                        format!("row {} field `{path}`: value is not a JSON array", cell.row),
                    ));
                    break;
                }
            },
        };
        lengths.push(values.len());
        items.extend(
            values
                .into_iter()
                .map(|raw| Cell::new(cell.row, raw, false)),
        );
    }
    let cells = &cells[..lengths.len()];
    let values = build_column(element, path, &items);
    let values = match (values, failure) {
        (Ok(values), None) => values,
        (Ok(_), Some(failure)) => return Err(failure),
        (Err(refused), failure) => return Err(refused.after(failure)),
    };
    let nulls = cells.iter().any(|cell| cell.raw.is_none()).then(|| {
        NullBuffer::from(
            cells
                .iter()
                .map(|cell| cell.raw.is_some())
                .collect::<Vec<_>>(),
        )
    });
    assembled(
        cells,
        ListArray::try_new(
            Arc::clone(element),
            OffsetBuffer::from_lengths(lengths),
            values,
            nulls,
        ),
    )
}

/// Build one Variant column; a present value of any JSON type is encoded
/// from its own text.
///
/// # Errors
///
/// Returns the catalogued Variant error naming `path` and the input row of
/// the first unstorable value.
fn build_variant(path: &str, cells: &[Cell<'_>]) -> Result<ArrayRef, Failure> {
    cells
        .iter()
        .map(|cell| {
            cell.raw
                .map(|raw| encode_variant(path, cell.row, raw.get()))
                .transpose()
                .map_err(|error| Failure {
                    row: cell.row,
                    error,
                })
        })
        .collect::<Result<VariantColumnBuilder, _>>()
        .map(VariantColumnBuilder::finish)
}

/// Convert each present cell with `parse`.
///
/// # Errors
///
/// Returns a schema-parse refusal at the first value `parse` rejects.
fn collect<T>(
    path: &str,
    cells: &[Cell<'_>],
    parse: impl Fn(&Value) -> Option<T>,
) -> Result<Vec<Option<T>>, Failure> {
    cells
        .iter()
        .map(|cell| {
            cell.raw
                .map(|raw| {
                    serde_json::from_str::<Value>(raw.get())
                        .ok()
                        .as_ref()
                        .and_then(&parse)
                        .ok_or_else(|| {
                            Failure::schema(
                                cell.row,
                                format!(
                                    "row {} field `{path}`: value does not match the column type",
                                    cell.row
                                ),
                            )
                        })
                })
                .transpose()
        })
        .collect()
}

/// Build one scalar column.
///
/// `FixedSizeBinary` columns take canonical lowercase hex text and decode it
/// to exactly the declared width, so trace and span ids land as the same
/// bytes `vala.traces.spans` stores.
///
/// # Errors
///
/// Returns a schema-parse refusal for a value that does not convert to the
/// column type (including malformed or wrong-width hex) or for a type the
/// row path does not convert.
fn build_scalar(data_type: &DataType, path: &str, cells: &[Cell<'_>]) -> Result<ArrayRef, Failure> {
    let array: ArrayRef = match data_type {
        DataType::Boolean => Arc::new(BooleanArray::from(collect(path, cells, Value::as_bool)?)),
        DataType::Int8 => Arc::new(Int8Array::from(collect(path, cells, |v| {
            v.as_i64().and_then(|n| i8::try_from(n).ok())
        })?)),
        DataType::Int16 => Arc::new(Int16Array::from(collect(path, cells, |v| {
            v.as_i64().and_then(|n| i16::try_from(n).ok())
        })?)),
        DataType::Int32 => Arc::new(Int32Array::from(collect(path, cells, |v| {
            v.as_i64().and_then(|n| i32::try_from(n).ok())
        })?)),
        DataType::Int64 => Arc::new(Int64Array::from(collect(path, cells, Value::as_i64)?)),
        DataType::UInt8 => Arc::new(UInt8Array::from(collect(path, cells, |v| {
            v.as_u64().and_then(|n| u8::try_from(n).ok())
        })?)),
        DataType::UInt16 => Arc::new(UInt16Array::from(collect(path, cells, |v| {
            v.as_u64().and_then(|n| u16::try_from(n).ok())
        })?)),
        DataType::UInt32 => Arc::new(UInt32Array::from(collect(path, cells, |v| {
            v.as_u64().and_then(|n| u32::try_from(n).ok())
        })?)),
        DataType::UInt64 => Arc::new(UInt64Array::from(collect(path, cells, Value::as_u64)?)),
        DataType::Float32 => Arc::new(Float32Array::from(collect(path, cells, |v| {
            v.as_f64().map(|n| n as f32)
        })?)),
        DataType::Float64 => Arc::new(Float64Array::from(collect(path, cells, Value::as_f64)?)),
        DataType::Utf8 => Arc::new(StringArray::from(collect(path, cells, |v| {
            v.as_str().map(str::to_owned)
        })?)),
        DataType::LargeUtf8 => Arc::new(LargeStringArray::from(collect(path, cells, |v| {
            v.as_str().map(str::to_owned)
        })?)),
        DataType::Date32 => Arc::new(Date32Array::from(collect(path, cells, |v| {
            v.as_str().and_then(parse_date_days)
        })?)),
        DataType::Timestamp(TimeUnit::Microsecond, tz) => {
            let values = collect(path, cells, |v| v.as_str().and_then(parse_rfc3339_micros))?;
            Arc::new(TimestampMicrosecondArray::from(values).with_timezone_opt(tz.clone()))
        }
        DataType::FixedSizeBinary(width) => {
            let width = *width;
            let values = collect(path, cells, |v| {
                v.as_str().and_then(|text| decode_lower_hex(text, width))
            })?;
            let array =
                FixedSizeBinaryArray::try_from_sparse_iter_with_size(values.into_iter(), width);
            return assembled(cells, array);
        }
        other => {
            let row = cells.first().map_or(0, |cell| cell.row);
            return Err(Failure::schema(
                row,
                format!("row conversion does not support column type {other:?} for field `{path}`"),
            ));
        }
    };
    Ok(array)
}

/// Decode exactly `width` bytes from canonical lowercase hex.
///
/// Trace and span identities travel as the lowercase hex text every OpenTelemetry
/// surface prints, and land in Arrow as the same fixed-width bytes
/// `vala.traces.spans` stores. Uppercase hex, a `0x` prefix, non-hex characters,
/// and any text whose length is not `2 * width` return `None`, which the caller
/// reports as a column type mismatch rather than silently truncating or padding.
fn decode_lower_hex(text: &str, width: i32) -> Option<Vec<u8>> {
    let width = usize::try_from(width).ok()?;
    if text.len() != width * 2 {
        return None;
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(width);
    for pair in bytes.chunks_exact(2) {
        let hi = lower_hex_digit(pair[0])?;
        let lo = lower_hex_digit(pair[1])?;
        out.push(hi << 4 | lo);
    }
    Some(out)
}

/// Value of one canonical lowercase hex digit, or `None` for any other byte.
fn lower_hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// Days since the Unix epoch for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Parse a `YYYY-MM-DD` date into days since the Unix epoch.
fn parse_date_days(text: &str) -> Option<i32> {
    let (date, _) = text.split_once(['T', ' ']).unwrap_or((text, ""));
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    i32::try_from(days_from_civil(year, month, day)).ok()
}

/// Parse an RFC 3339 timestamp into microseconds since the Unix epoch.
///
/// Supports `YYYY-MM-DDThh:mm:ss[.frac][Z|±hh:mm]` (also a space date/time
/// separator). Dependency-free: `chrono` is not a permitted dependency here.
fn parse_rfc3339_micros(text: &str) -> Option<i64> {
    let (date, rest) = text.split_once(['T', ' '])?;
    let mut dparts = date.split('-');
    let year: i64 = dparts.next()?.parse().ok()?;
    let month: i64 = dparts.next()?.parse().ok()?;
    let day: i64 = dparts.next()?.parse().ok()?;
    if dparts.next().is_some() {
        return None;
    }

    // Split trailing timezone from the time-of-day.
    let (time, offset_secs) = if let Some(stripped) = rest.strip_suffix('Z') {
        (stripped, 0_i64)
    } else if let Some(idx) = rest.rfind(['+', '-']) {
        let (t, off) = rest.split_at(idx);
        (t, parse_offset_secs(off)?)
    } else {
        (rest, 0_i64)
    };

    let (hms, frac) = match time.split_once('.') {
        Some((hms, frac)) => (hms, frac),
        None => (time, ""),
    };
    let mut tparts = hms.split(':');
    let hour: i64 = tparts.next()?.parse().ok()?;
    let minute: i64 = tparts.next()?.parse().ok()?;
    let second: i64 = tparts.next()?.parse().ok()?;
    if tparts.next().is_some()
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return None;
    }

    let micros_frac = parse_fraction_micros(frac)?;
    let days = days_from_civil(year, month, day);
    let secs = days * 86_400 + hour * 3_600 + minute * 60 + second - offset_secs;
    Some(secs * 1_000_000 + micros_frac)
}

fn parse_offset_secs(offset: &str) -> Option<i64> {
    let (sign, body) = offset.split_at(1);
    let sign = match sign {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };
    let mut parts = body.split(':');
    let hours: i64 = parts.next()?.parse().ok()?;
    let minutes: i64 = parts.next().unwrap_or("0").parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(sign * (hours * 3_600 + minutes * 60))
}

fn parse_fraction_micros(frac: &str) -> Option<i64> {
    if frac.is_empty() {
        return Some(0);
    }
    if !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut digits = frac.to_owned();
    digits.truncate(6);
    while digits.len() < 6 {
        digits.push('0');
    }
    digits.parse().ok()
}

#[cfg(test)]
mod batch_builder_tests {
    //! `RowPreflight` proof: user cols + `card_ref`/`run_id`, nested types,
    //! reserved/undeclared/type-mismatch rejection, and Arrow IPC round-trip.

    use arrow::array::{Array, AsArray, FixedSizeBinaryArray, Int64Array, ListArray, StructArray};
    use arrow::datatypes::Int64Type;
    use arrow::ipc::reader::StreamReader;
    use arrow_schema::{DataType, Field, Fields, Schema};
    use serde_json::json;
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::vala::ids::RunId;

    use crate::variant::{variant_cell_to_json, variant_field};
    use crate::{PreparedRows, RowPreflight, WyrdQueueError};

    /// A two-column user schema: required `id`, optional `name`.
    fn user_schema() -> Schema {
        Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
        ])
    }

    /// A test card reference named `name`.
    fn card(name: &str) -> CardRef {
        format!("prod/Service/{name}@1.0.0")
            .parse()
            .expect("valid card ref")
    }

    /// Prepares `rows` against `schema` with no correlation.
    fn prepare(schema: &Schema, rows: &[&str]) -> Result<PreparedRows, WyrdQueueError> {
        RowPreflight::new(schema).prepare(rows, None, None)
    }

    /// User columns come first, then the nullable correlation columns, and an
    /// explicit null on a nullable column is preserved.
    #[test]
    fn builds_user_columns_plus_correlation_columns() {
        let prepared = RowPreflight::new(&user_schema())
            .prepare(
                &[r#"{"id": 1, "name": "a"}"#, r#"{"id": 2, "name": null}"#],
                Some(&card("alpha")),
                Some(&RunId::from_string("run-1".to_owned())),
            )
            .expect("rows prepare");
        let batch = prepared.batch();
        assert_eq!((batch.num_columns(), batch.num_rows()), (4, 2));
        let schema = batch.schema();
        assert_eq!(schema.field(2).name(), "card_ref");
        assert_eq!(schema.field(3).name(), "run_id");
        assert!(schema.field(2).is_nullable() && schema.field(3).is_nullable());
        assert_eq!(
            batch.column(0).as_primitive::<Int64Type>().values(),
            &[1, 2]
        );
        let names = batch.column(1).as_string::<i32>();
        assert_eq!(names.value(0), "a");
        assert!(names.is_null(1), "explicit null preserved");
        assert_eq!(
            batch.column(2).as_string::<i32>().value(1),
            "prod/Service/alpha@1.0.0"
        );
        assert_eq!(batch.column(3).as_string::<i32>().value(0), "run-1");
        assert!(prepared.charge() >= batch.get_array_memory_size());

        let uncorrelated = prepare(&user_schema(), &[r#"{"id": 3}"#]).expect("prepares");
        assert!(uncorrelated.batch().column(2).is_null(0));
        assert!(uncorrelated.batch().column(3).is_null(0));
    }

    /// Reserved keys, undeclared keys, type mismatches, missing required
    /// values, and invalid JSON are refused with their stable codes.
    #[test]
    fn invalid_rows_are_refused_with_stable_codes() {
        for (row, code) in [
            (
                r#"{"id": 1, "card_ref": "x"}"#,
                "WYRD_VALA_400_BIFROST_RESERVED_COLUMN",
            ),
            (
                r#"{"id": 1, "wyrd_ts": 1}"#,
                "WYRD_VALA_400_BIFROST_RESERVED_COLUMN",
            ),
            (
                r#"{"id": 1, "extra": 1}"#,
                "WYRD_VALA_400_BIFROST_UNDECLARED_FIELD",
            ),
            (r#"{"id": "not-an-int"}"#, "WYRD_VALA_400_SCHEMA_PARSE"),
            (r#"{"name": "a"}"#, "WYRD_VALA_400_SCHEMA_PARSE"),
            (r#"{"id": 1"#, "WYRD_VALA_400_SCHEMA_PARSE"),
            ("[1]", "WYRD_VALA_400_SCHEMA_PARSE"),
        ] {
            let error = prepare(&user_schema(), &[row]).expect_err("row is refused");
            assert_eq!(error.code(), code, "row {row}");
        }
    }

    /// Struct, List, and Variant values keep their shapes, nulls, and exact
    /// integers; a null struct excuses its required children, and a string
    /// in a Variant column stays a string.
    #[test]
    fn nested_values_keep_types_nulls_and_precision() {
        let point = Fields::from(vec![
            Field::new("x", DataType::Int64, false),
            variant_field("meta", true),
        ]);
        let schema = Schema::new(vec![
            Field::new("point", DataType::Struct(point), true),
            Field::new_list("tags", Field::new("item", DataType::Utf8, true), true),
            variant_field("extra", true),
        ]);
        let prepared = prepare(
            &schema,
            &[
                r#"{"point": {"x": 1, "meta": {"k": [1, null]}}, "tags": ["a", null], "extra": 18446744073709551615}"#,
                r#"{"point": null, "tags": null, "extra": "text"}"#,
                r#"{"tags": [], "extra": {"absent": null}}"#,
            ],
        )
        .expect("nested rows prepare");
        let batch = prepared.batch();
        let points = batch
            .column(0)
            .as_any()
            .downcast_ref::<StructArray>()
            .expect("struct");
        assert!(points.is_valid(0) && points.is_null(1) && points.is_null(2));
        assert_eq!(points.column(0).as_primitive::<Int64Type>().value(0), 1);
        assert_eq!(
            variant_cell_to_json(points.column(1).as_ref(), 0).expect("renders"),
            json!({"k": [1, null]})
        );
        let tags = batch
            .column(1)
            .as_any()
            .downcast_ref::<ListArray>()
            .expect("list");
        assert_eq!(tags.value_length(0), 2);
        assert!(tags.value(0).as_string::<i32>().is_null(1));
        assert!(tags.is_null(1));
        assert_eq!(tags.value_length(2), 0);
        let extra = batch.column(2).as_ref();
        assert_eq!(
            variant_cell_to_json(extra, 0).expect("renders").to_string(),
            "18446744073709551615"
        );
        assert_eq!(
            variant_cell_to_json(extra, 1).expect("renders"),
            json!("text")
        );
        assert_eq!(
            variant_cell_to_json(extra, 2).expect("renders"),
            json!({"absent": null})
        );
        let error = prepare(&schema, &[r#"{"point": {"x": 1, "y": 2}}"#]).expect_err("refused");
        assert_eq!(error.code(), "WYRD_VALA_400_BIFROST_UNDECLARED_FIELD");
        assert!(error.to_string().contains("point.y"), "{error}");
    }

    /// The prepared batch round-trips through Arrow IPC.
    #[test]
    fn ipc_round_trips() {
        let bytes = prepare(&user_schema(), &[r#"{"id": 7, "name": "seven"}"#])
            .expect("prepares")
            .to_ipc()
            .expect("ipc");
        let mut reader = StreamReader::try_new(bytes.as_slice(), None).expect("reader");
        let decoded = reader.next().expect("one batch").expect("ok");
        assert_eq!((decoded.num_rows(), decoded.num_columns()), (1, 4));
        let ids = decoded
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("int64");
        assert_eq!(ids.value(0), 7);
        assert!(reader.next().is_none(), "single batch stream");
    }

    /// Trace/span identity columns as `vala.eval.observations` and
    /// `vala.traces.spans` describe them: nullable fixed-width binary keyed by
    /// canonical lowercase hex text.
    fn trace_schema() -> Schema {
        Schema::new(vec![
            Field::new("trace_id", DataType::FixedSizeBinary(16), true),
            Field::new("span_id", DataType::FixedSizeBinary(8), true),
        ])
    }

    /// Lowercase hex trace and span ids become the exact 16- and 8-byte values,
    /// and a missing id stays null.
    #[test]
    fn fixed_size_binary_round_trips_hex_trace_and_span_ids() {
        let prepared = prepare(
            &trace_schema(),
            &[
                r#"{"trace_id": "0af7651916cd43dd8448eb211c80319c", "span_id": "b7ad6b7169203331"}"#,
                r#"{"trace_id": null, "span_id": null}"#,
            ],
        )
        .expect("prepares");
        let batch = prepared.batch();
        let traces = batch
            .column(0)
            .as_any()
            .downcast_ref::<FixedSizeBinaryArray>()
            .expect("fixed-size binary");
        assert_eq!(traces.value_length(), 16);
        assert_eq!(
            traces.value(0),
            [
                0x0a, 0xf7, 0x65, 0x19, 0x16, 0xcd, 0x43, 0xdd, 0x84, 0x48, 0xeb, 0x21, 0x1c, 0x80,
                0x31, 0x9c
            ]
        );
        assert!(traces.is_null(1), "absent trace identity stays null");
        let spans = batch
            .column(1)
            .as_any()
            .downcast_ref::<FixedSizeBinaryArray>()
            .expect("fixed-size binary");
        assert_eq!(
            spans.value(0),
            [0xb7, 0xad, 0x6b, 0x71, 0x69, 0x20, 0x33, 0x31]
        );
    }

    /// Short, long, uppercase, prefixed, and non-hex ids are refused rather
    /// than truncated or padded into a wrong identity.
    #[test]
    fn fixed_size_binary_rejects_malformed_and_wrong_width_hex() {
        for row in [
            r#"{"trace_id": "0af7651916cd43dd8448eb211c8031"}"#,
            r#"{"trace_id": "0af7651916cd43dd8448eb211c80319c00"}"#,
            r#"{"trace_id": "0AF7651916CD43DD8448EB211C80319C"}"#,
            r#"{"trace_id": "0af7651916cd43dd8448eb211c80319z"}"#,
            r#"{"trace_id": 12}"#,
        ] {
            let err = prepare(&trace_schema(), &[row])
                .expect_err("malformed fixed-size binary input is refused before admission");
            assert_eq!(err.code(), "WYRD_VALA_400_SCHEMA_PARSE", "row {row}");
        }
    }

    /// A preflight over required `id`, optional `name`, and Variant `payload`.
    fn preflight() -> RowPreflight {
        RowPreflight::new(&Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
            variant_field("payload", true),
        ]))
    }

    /// One row holding each named Int64 column.
    fn ints(columns: &[&str]) -> arrow::array::RecordBatch {
        let fields: Vec<Field> = columns
            .iter()
            .map(|name| Field::new(*name, DataType::Int64, true))
            .collect();
        let arrays = columns
            .iter()
            .map(|_| std::sync::Arc::new(Int64Array::from(vec![1])) as arrow::array::ArrayRef)
            .collect();
        arrow::array::RecordBatch::try_new(std::sync::Arc::new(Schema::new(fields)), arrays)
            .expect("batch builds")
    }

    /// Columns match by name: the output follows declared order, a supplied
    /// column takes its declared nullability, an omitted nullable column is
    /// all-null with its declared type, and a reserved column passes through
    /// after the declared ones.
    #[test]
    fn batch_columns_match_by_name_and_omitted_nullable_columns_are_null() {
        let conformed = preflight()
            .prepare_batch(&ints(&["run_id", "id"]))
            .expect("conforms");

        let schema = conformed.schema();
        let names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
        assert_eq!(names, ["id", "name", "payload", "run_id"]);
        assert!(!schema.field(0).is_nullable(), "id is declared required");
        assert_eq!(schema.field(1).data_type(), &DataType::Utf8);
        assert!(crate::variant::is_variant(schema.field(2)));
        assert!(conformed.column(1).is_null(0) && conformed.column(2).is_null(0));
    }

    /// An undeclared column, an omitted required column, and a null in a
    /// required column are refused.
    #[test]
    fn undeclared_and_missing_required_columns_are_refused() {
        let undeclared = preflight()
            .prepare_batch(&ints(&["id", "extra"]))
            .expect_err("extra is not declared");
        assert_eq!(undeclared.code(), "WYRD_VALA_400_BIFROST_UNDECLARED_FIELD");
        assert!(undeclared.to_string().contains("extra"), "{undeclared}");

        let missing = preflight()
            .prepare_batch(&ints(&["name"]))
            .expect_err("id is required");
        assert_eq!(missing.code(), "WYRD_VALA_400_SCHEMA_PARSE");
        assert!(missing.to_string().contains("id"), "{missing}");

        let null_id = arrow::array::RecordBatch::try_from_iter([(
            "id",
            std::sync::Arc::new(Int64Array::from(vec![Some(1), None])) as arrow::array::ArrayRef,
        )])
        .expect("batch builds");
        let null = preflight()
            .prepare_batch(&null_id)
            .expect_err("id is required");
        assert_eq!(null.code(), "WYRD_VALA_400_SCHEMA_PARSE");
        assert!(null.to_string().contains("row 1 field `id`"), "{null}");
    }

    /// A batch naming a column twice is refused rather than keeping one of
    /// the two supplied values.
    #[test]
    fn duplicate_batch_columns_are_refused() {
        let duplicated = arrow::array::RecordBatch::try_from_iter([
            (
                "id",
                std::sync::Arc::new(Int64Array::from(vec![1])) as arrow::array::ArrayRef,
            ),
            (
                "id",
                std::sync::Arc::new(Int64Array::from(vec![2])) as arrow::array::ArrayRef,
            ),
        ])
        .expect("Arrow allows duplicate names");

        let error = preflight()
            .prepare_batch(&duplicated)
            .expect_err("id is ambiguous");

        assert_eq!(error.code(), "WYRD_VALA_400_SCHEMA_PARSE");
        assert!(error.to_string().contains("`id`"), "{error}");
    }

    /// Rows and the equivalent Arrow batch report the same first refusal:
    /// the earliest input row wins, then the earlier declared field.
    #[test]
    fn batch_and_rows_select_the_same_first_refusal() {
        let deep = format!("{}{}", "[".repeat(65), "]".repeat(65));
        for (ids, payloads, code) in [
            // An unstorable Variant on row 0 beats a required null on row 1.
            (
                [Some(1), None],
                [Some(deep.as_str()), None],
                "WYRD_VALA_400_VARIANT_TOO_DEEP",
            ),
            // On one row the earlier declared field wins.
            (
                [None, Some(2)],
                [Some(deep.as_str()), None],
                "WYRD_VALA_400_SCHEMA_PARSE",
            ),
        ] {
            let rows: Vec<String> = ids
                .iter()
                .zip(&payloads)
                .map(|(id, payload)| {
                    let id = id.map_or("null".to_owned(), |id| id.to_string());
                    format!(
                        r#"{{"id": {id}, "payload": {}}}"#,
                        payload.unwrap_or("null")
                    )
                })
                .collect();
            let batch = arrow::array::RecordBatch::try_from_iter([
                (
                    "id",
                    std::sync::Arc::new(Int64Array::from(ids.to_vec())) as arrow::array::ArrayRef,
                ),
                (
                    "payload",
                    std::sync::Arc::new(arrow::array::StringArray::from(payloads.to_vec())),
                ),
            ])
            .expect("batch builds");

            let from_rows = preflight()
                .prepare(&rows, None, None)
                .expect_err("rows are refused");
            let from_batch = preflight()
                .prepare_batch(&batch)
                .expect_err("batch is refused");

            assert_eq!(from_rows.code(), code, "{rows:?}");
            assert_eq!(from_batch.code(), code, "{rows:?}");
            assert_eq!(from_batch.to_string(), from_rows.to_string(), "{rows:?}");
        }
    }
}
