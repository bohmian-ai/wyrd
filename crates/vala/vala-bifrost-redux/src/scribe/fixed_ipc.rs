//! Exact-capacity Arrow IPC V5 encoding for one admitted Scribe slice.

use arrow::array::{
    Array, BinaryArray, BooleanArray, FixedSizeBinaryArray, LargeBinaryArray, LargeStringArray,
    NullArray, PrimitiveArray, RecordBatch, StringArray,
};
use arrow::datatypes::{
    ArrowPrimitiveType, DataType, Date32Type, Date64Type, Decimal128Type, Decimal256Type,
    DurationMicrosecondType, DurationMillisecondType, DurationNanosecondType, DurationSecondType,
    Field, Float16Type, Float32Type, Float64Type, Int8Type, Int16Type, Int32Type, Int64Type,
    IntervalDayTimeType, IntervalMonthDayNanoType, IntervalUnit, IntervalYearMonthType, Schema,
    Time32MillisecondType, Time32SecondType, Time64MicrosecondType, Time64NanosecondType, TimeUnit,
    TimestampMicrosecondType, TimestampMillisecondType, TimestampNanosecondType,
    TimestampSecondType, UInt8Type, UInt16Type, UInt32Type, UInt64Type,
};

use crate::contracts::ScribeError;

/// Maximum number of flat scalar fields accepted by the native V1 contract.
const MAX_FIELDS: usize = 256;
/// Maximum number of physical buffers emitted for one accepted flat field.
const MAX_BUFFERS: usize = MAX_FIELDS * 3;
/// Arrow stream continuation marker used before every V5 metadata message.
const CONTINUATION: [u8; 4] = [0xff; 4];

/// Exact logical buffer facts for one not-yet-materialized projected column.
///
/// A projection count pass constructs these values in schema order. Fixed-width
/// columns set `offsets_bytes` to zero. Variable-width columns provide their
/// exact terminal values length and complete offset-buffer length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FixedIpcColumnPlan {
    /// Logical null count written into the IPC field node.
    pub(crate) null_count: usize,
    /// Exact validity bitmap bytes, or zero when no bitmap is materialized.
    pub(crate) validity_bytes: usize,
    /// Exact variable-width offsets bytes, or zero for fixed-width fields.
    pub(crate) offsets_bytes: usize,
    /// Exact logical values bytes before IPC alignment padding.
    pub(crate) values_bytes: usize,
}

/// Exact immutable sizing result for one schema and one record batch.
///
/// The plan contains only fixed-bound descriptors. Constructing it allocates
/// two fixed-length boxed descriptor tables but no input-sized or growable
/// serialization storage. Encoding repeats the same checked schema walk and
/// writes directly into one exact-capacity `Vec`.
#[derive(Debug, Clone)]
pub(crate) struct FixedIpcPlan {
    /// Exact unpadded `Schema`-message `FlatBuffer` length.
    schema_metadata_bytes: usize,
    /// Exact unpadded `RecordBatch`-message `FlatBuffer` length.
    batch_metadata_bytes: usize,
    /// Exact aligned record-batch body length.
    body_bytes: usize,
    /// Exact complete stream length, including framing and EOS.
    encoded_bytes: usize,
    /// Fixed record-batch physical layout used by the write pass.
    batch: BatchFacts,
}

impl FixedIpcPlan {
    /// Counts one canonical flat scalar batch without allocating IPC storage.
    ///
    /// The accepted schema matches the native V1 subset and intentionally
    /// excludes dictionaries, nested fields, extension metadata, and sliced
    /// arrays. Those forms cannot use this current-slice encoder.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] for unsupported schemas, metadata,
    /// mismatched or sliced arrays, and malformed physical buffers. Returns
    /// [`ScribeError::DecodedPayloadTooLarge`] when checked size arithmetic
    /// overflows the platform address space.
    pub(crate) fn count(batch: &RecordBatch) -> Result<Self, ScribeError> {
        validate_schema(batch.schema_ref())?;
        if batch.num_rows() == 0 {
            return Err(ScribeError::InvalidFrame);
        }
        let batch_facts = BatchFacts::count(batch)?;
        Self::from_facts(batch.schema_ref(), batch.num_rows(), batch_facts)
    }

    /// Counts IPC output from pre-material projection facts.
    ///
    /// The iterator must yield exactly one entry per schema field. This method
    /// validates every null, validity, offsets, and values length against the
    /// canonical scalar layout before any Arrow array or IPC buffer exists.
    /// Its result is later consumed by [`Self::encode`], which recomputes and
    /// compares the materialized batch facts before writing.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] for unsupported schemas, zero
    /// rows, field-count mismatch, or physically inconsistent column facts.
    /// Returns [`ScribeError::DecodedPayloadTooLarge`] on checked arithmetic
    /// overflow.
    pub(crate) fn count_schema<I>(
        schema: &Schema,
        rows: usize,
        columns: I,
    ) -> Result<Self, ScribeError>
    where
        I: ExactSizeIterator<Item = FixedIpcColumnPlan>,
    {
        validate_schema(schema)?;
        if rows == 0 || columns.len() != schema.fields().len() {
            return Err(ScribeError::InvalidFrame);
        }
        let batch_facts = BatchFacts::from_columns(schema, rows, columns)?;
        Self::from_facts(schema, rows, batch_facts)
    }

    /// Completes exact stream framing from already validated batch facts.
    ///
    /// # Errors
    ///
    /// Returns a checked size or metadata-layout error when the stream cannot
    /// be represented in the current process address space.
    fn from_facts(
        schema: &Schema,
        rows: usize,
        batch_facts: BatchFacts,
    ) -> Result<Self, ScribeError> {
        let schema_metadata_bytes = metadata_len(|writer| write_schema_message(writer, schema))?;
        let batch_metadata_bytes =
            metadata_len(|writer| write_batch_message(writer, rows, &batch_facts))?;
        let encoded_bytes = framed_metadata_len(schema_metadata_bytes)?
            .checked_add(framed_metadata_len(batch_metadata_bytes)?)
            .and_then(|bytes| bytes.checked_add(batch_facts.body_bytes))
            .and_then(|bytes| bytes.checked_add(8))
            .ok_or_else(material_overflow)?;

        Ok(Self {
            schema_metadata_bytes,
            batch_metadata_bytes,
            body_bytes: batch_facts.body_bytes,
            encoded_bytes,
            batch: batch_facts,
        })
    }

    /// Returns the exact output allocation required by [`Self::encode`].
    #[must_use]
    pub(crate) fn encoded_bytes(&self) -> usize {
        self.encoded_bytes
    }

    /// Writes one V5 Arrow stream into an exact-capacity owned buffer.
    ///
    /// The output contains one `Schema` message, one `RecordBatch` message, and
    /// one EOS marker. The method revalidates the input against this plan so a
    /// caller cannot accidentally reuse admitted capacity for a larger batch.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when the batch no longer matches
    /// this plan or when its schema or buffers are unsupported. Returns
    /// [`ScribeError::DecodedPayloadTooLarge`] on checked arithmetic overflow.
    pub(crate) fn encode(&self, batch: &RecordBatch) -> Result<Vec<u8>, ScribeError> {
        let observed = Self::count(batch)?;
        if observed.schema_metadata_bytes != self.schema_metadata_bytes
            || observed.batch_metadata_bytes != self.batch_metadata_bytes
            || observed.body_bytes != self.body_bytes
            || observed.encoded_bytes != self.encoded_bytes
            || observed.batch != self.batch
        {
            return Err(ScribeError::InvalidFrame);
        }

        let mut output = vec![0_u8; self.encoded_bytes];
        if output.capacity() != self.encoded_bytes {
            return Err(ScribeError::InvalidFrame);
        }
        let mut cursor = 0_usize;
        cursor =
            write_framed_metadata(&mut output, cursor, self.schema_metadata_bytes, |writer| {
                write_schema_message(writer, batch.schema_ref())
            })?;
        cursor = write_framed_metadata(&mut output, cursor, self.batch_metadata_bytes, |writer| {
            write_batch_message(writer, batch.num_rows(), &self.batch)
        })?;
        write_body(
            &mut output[cursor..cursor + self.body_bytes],
            batch,
            &self.batch,
        )?;
        cursor += self.body_bytes;
        output[cursor..cursor + 4].copy_from_slice(&CONTINUATION);
        cursor += 8;
        if cursor != output.len() || output.capacity() != output.len() {
            return Err(ScribeError::InvalidFrame);
        }
        Ok(output)
    }
}

/// Fixed physical facts for one current record batch.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BatchFacts {
    /// One node per flat schema field.
    nodes: Box<[FieldNodeFact]>,
    /// Number of initialized entries in `nodes`.
    node_count: usize,
    /// Physical buffers in schema order.
    buffers: Box<[BufferFact]>,
    /// Number of initialized entries in `buffers`.
    buffer_count: usize,
    /// Aligned bytes occupied by the complete batch body.
    body_bytes: usize,
}

impl BatchFacts {
    /// Counts nodes, descriptors, and exact logical buffer lengths.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] for unsupported or inconsistent
    /// arrays and [`ScribeError::DecodedPayloadTooLarge`] on overflow.
    fn count(batch: &RecordBatch) -> Result<Self, ScribeError> {
        let fields = batch.schema().fields().len();
        if fields == 0 || fields > MAX_FIELDS || fields != batch.num_columns() {
            return Err(ScribeError::InvalidFrame);
        }
        let mut facts = Self {
            nodes: fixed_box(MAX_FIELDS, FieldNodeFact::EMPTY),
            node_count: fields,
            buffers: fixed_box(MAX_BUFFERS, BufferFact::EMPTY),
            buffer_count: 0,
            body_bytes: 0,
        };
        for (column, array) in batch.columns().iter().enumerate() {
            if array.offset() != 0 || array.len() != batch.num_rows() {
                return Err(ScribeError::InvalidFrame);
            }
            if !batch.schema().field(column).is_nullable() && array.logical_null_count() != 0 {
                return Err(ScribeError::InvalidFrame);
            }
            let length = i64::try_from(array.len()).map_err(|_| material_overflow())?;
            let null_count =
                i64::try_from(array.logical_null_count()).map_err(|_| material_overflow())?;
            facts.nodes[column] = FieldNodeFact { length, null_count };
            facts.count_column(column, array.as_ref())?;
        }
        Ok(facts)
    }

    /// Builds physical facts from a projection's allocation-free count pass.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when a column contradicts its
    /// schema layout and [`ScribeError::DecodedPayloadTooLarge`] on checked
    /// arithmetic overflow.
    fn from_columns<I>(schema: &Schema, rows: usize, columns: I) -> Result<Self, ScribeError>
    where
        I: ExactSizeIterator<Item = FixedIpcColumnPlan>,
    {
        let fields = schema.fields().len();
        let mut facts = Self {
            nodes: fixed_box(MAX_FIELDS, FieldNodeFact::EMPTY),
            node_count: fields,
            buffers: fixed_box(MAX_BUFFERS, BufferFact::EMPTY),
            buffer_count: 0,
            body_bytes: 0,
        };
        for (column, (field, plan)) in schema.fields().iter().zip(columns).enumerate() {
            if plan.null_count > rows || (!field.is_nullable() && plan.null_count != 0) {
                return Err(ScribeError::InvalidFrame);
            }
            facts.nodes[column] = FieldNodeFact {
                length: i64::try_from(rows).map_err(|_| material_overflow())?,
                null_count: i64::try_from(plan.null_count).map_err(|_| material_overflow())?,
            };
            facts.push_planned_column(column, field.data_type(), rows, plan)?;
        }
        Ok(facts)
    }

    /// Validates and appends one projection-planned column layout.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] for a noncanonical fact tuple and
    /// [`ScribeError::DecodedPayloadTooLarge`] on checked arithmetic overflow.
    fn push_planned_column(
        &mut self,
        column: usize,
        data_type: &DataType,
        rows: usize,
        plan: FixedIpcColumnPlan,
    ) -> Result<(), ScribeError> {
        if matches!(data_type, DataType::Null) {
            if plan.null_count != rows
                || plan.validity_bytes != 0
                || plan.offsets_bytes != 0
                || plan.values_bytes != 0
            {
                return Err(ScribeError::InvalidFrame);
            }
            return Ok(());
        }
        let expected_validity = if plan.null_count == 0 {
            0
        } else {
            bitmap_bytes(rows)?
        };
        if plan.validity_bytes != expected_validity {
            return Err(ScribeError::InvalidFrame);
        }
        self.push_buffer(column, BufferSource::Validity, plan.validity_bytes)?;

        if matches!(data_type, DataType::Boolean) {
            if plan.offsets_bytes != 0 || plan.values_bytes != bitmap_bytes(rows)? {
                return Err(ScribeError::InvalidFrame);
            }
            self.push_buffer(column, BufferSource::Values, plan.values_bytes)?;
            return Ok(());
        }
        let fixed_width = fixed_value_width(data_type)?;
        if let Some(width) = fixed_width {
            let expected_values = rows.checked_mul(width).ok_or_else(material_overflow)?;
            if plan.offsets_bytes != 0 || plan.values_bytes != expected_values {
                return Err(ScribeError::InvalidFrame);
            }
            self.push_buffer(column, BufferSource::Values, plan.values_bytes)?;
            return Ok(());
        }

        let offset_width = variable_offset_width(data_type).ok_or(ScribeError::InvalidFrame)?;
        let expected_offsets = rows
            .checked_add(1)
            .and_then(|count| count.checked_mul(offset_width))
            .ok_or_else(material_overflow)?;
        if plan.offsets_bytes != expected_offsets {
            return Err(ScribeError::InvalidFrame);
        }
        self.push_buffer(column, BufferSource::Offsets, plan.offsets_bytes)?;
        self.push_buffer(column, BufferSource::Values, plan.values_bytes)
    }

    /// Appends the canonical buffer sequence for one flat array.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] for malformed physical storage and
    /// [`ScribeError::DecodedPayloadTooLarge`] on checked length overflow.
    fn count_column(&mut self, column: usize, array: &dyn Array) -> Result<(), ScribeError> {
        let data_type = array.data_type();
        if matches!(data_type, DataType::Null) {
            if array.logical_null_count() != array.len()
                || array.as_any().downcast_ref::<NullArray>().is_none()
            {
                return Err(ScribeError::InvalidFrame);
            }
            return Ok(());
        }

        let validity_len = if array.logical_null_count() == 0 {
            0
        } else {
            bitmap_bytes(array.len())?
        };
        if validity_len != 0
            && array
                .nulls()
                .is_none_or(|nulls| nulls.buffer().len() < validity_len)
        {
            return Err(ScribeError::InvalidFrame);
        }
        self.push_buffer(column, BufferSource::Validity, validity_len)?;

        match data_type {
            DataType::Boolean => {
                self.push_values(column, array, bitmap_bytes(array.len())?)?;
            }
            DataType::Int8 | DataType::UInt8 => {
                self.push_fixed(column, array, 1)?;
            }
            DataType::Int16 | DataType::UInt16 | DataType::Float16 => {
                self.push_fixed(column, array, 2)?;
            }
            DataType::Int32
            | DataType::UInt32
            | DataType::Float32
            | DataType::Date32
            | DataType::Time32(_)
            | DataType::Interval(IntervalUnit::YearMonth) => {
                self.push_fixed(column, array, 4)?;
            }
            DataType::Int64
            | DataType::UInt64
            | DataType::Float64
            | DataType::Date64
            | DataType::Time64(_)
            | DataType::Timestamp(_, _)
            | DataType::Duration(_)
            | DataType::Interval(IntervalUnit::DayTime) => {
                self.push_fixed(column, array, 8)?;
            }
            DataType::Decimal128(_, _) | DataType::Interval(IntervalUnit::MonthDayNano) => {
                self.push_fixed(column, array, 16)?;
            }
            DataType::Decimal256(_, _) => {
                self.push_fixed(column, array, 32)?;
            }
            DataType::FixedSizeBinary(width) if *width > 0 => {
                let width = usize::try_from(*width).map_err(|_| ScribeError::InvalidFrame)?;
                self.push_fixed(column, array, width)?;
            }
            DataType::Binary | DataType::Utf8 => {
                self.push_variable(column, array, 4)?;
            }
            DataType::LargeBinary | DataType::LargeUtf8 => {
                self.push_variable(column, array, 8)?;
            }
            _ => return Err(ScribeError::InvalidFrame),
        }
        Ok(())
    }

    /// Appends one fixed-width value buffer.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when storage is too short and
    /// [`ScribeError::DecodedPayloadTooLarge`] on length overflow.
    fn push_fixed(
        &mut self,
        column: usize,
        array: &dyn Array,
        width: usize,
    ) -> Result<(), ScribeError> {
        let length = array
            .len()
            .checked_mul(width)
            .ok_or_else(material_overflow)?;
        self.push_values(column, array, length)
    }

    /// Appends offsets followed by the referenced variable values.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] for malformed offsets or storage
    /// and [`ScribeError::DecodedPayloadTooLarge`] on length overflow.
    fn push_variable(
        &mut self,
        column: usize,
        array: &dyn Array,
        offset_width: usize,
    ) -> Result<(), ScribeError> {
        let offsets = array
            .len()
            .checked_add(1)
            .and_then(|count| count.checked_mul(offset_width))
            .ok_or_else(material_overflow)?;
        let offset_source = borrowed_buffer(array, BufferSource::Offsets)?;
        if offset_source.len() < offsets {
            return Err(ScribeError::InvalidFrame);
        }
        self.push_buffer(column, BufferSource::Offsets, offsets)?;
        let terminal = read_terminal_offset(offset_source, offsets, offset_width)?;
        self.push_values(column, array, terminal)
    }

    /// Appends a values-buffer descriptor after checking its borrowed source.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when the source buffer is too
    /// short and propagates descriptor arithmetic failures.
    fn push_values(
        &mut self,
        column: usize,
        array: &dyn Array,
        length: usize,
    ) -> Result<(), ScribeError> {
        if borrowed_buffer(array, BufferSource::Values)?.len() < length {
            return Err(ScribeError::InvalidFrame);
        }
        self.push_buffer(column, BufferSource::Values, length)
    }

    /// Records one physical buffer and advances the aligned body cursor.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when the fixed descriptor array is
    /// exhausted and [`ScribeError::DecodedPayloadTooLarge`] on overflow.
    fn push_buffer(
        &mut self,
        column: usize,
        source: BufferSource,
        length: usize,
    ) -> Result<(), ScribeError> {
        if self.buffer_count == MAX_BUFFERS {
            return Err(ScribeError::InvalidFrame);
        }
        self.buffers[self.buffer_count] = BufferFact {
            column,
            source,
            offset: self.body_bytes,
            length,
        };
        self.buffer_count += 1;
        self.body_bytes = self
            .body_bytes
            .checked_add(align_eight(length)?)
            .ok_or_else(material_overflow)?;
        Ok(())
    }
}

/// One Arrow IPC field-node struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FieldNodeFact {
    /// Logical array length.
    length: i64,
    /// Logical null count.
    null_count: i64,
}

impl FieldNodeFact {
    /// Zero placeholder for unused fixed array entries.
    const EMPTY: Self = Self {
        length: 0,
        null_count: 0,
    };
}

/// Selects the borrowed Arrow buffer used by one IPC descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BufferSource {
    /// Optional validity bitmap, or an empty descriptor when all values exist.
    Validity,
    /// Canonical physical values buffer for the concrete array.
    Values,
    /// Canonical physical offsets buffer for a variable-width array.
    Offsets,
}

/// One planned record-batch body buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BufferFact {
    /// Source record-batch column.
    column: usize,
    /// Borrowed physical source within the column.
    source: BufferSource,
    /// Aligned byte offset in the record-batch body.
    offset: usize,
    /// Logical buffer bytes before alignment padding.
    length: usize,
}

impl BufferFact {
    /// Zero placeholder for unused fixed array entries.
    const EMPTY: Self = Self {
        column: 0,
        source: BufferSource::Validity,
        offset: 0,
        length: 0,
    };
}

/// Allocates one fixed-length heap slice without placing the descriptor bound
/// on the thread stack.
///
/// The temporary vector is created at its final length and immediately frozen;
/// no element is pushed and no capacity growth can occur.
fn fixed_box<T: Copy>(length: usize, value: T) -> Box<[T]> {
    vec![value; length].into_boxed_slice()
}

/// Minimal forward `FlatBuffer` writer used only for the accepted IPC metadata.
///
/// `FlatBuffer` references point forward, so this writer reserves tables before
/// their children and patches checked relative offsets once targets exist. A
/// count pass uses the same layout operations with no backing slice.
struct FlatWriter<'a> {
    /// Optional exact metadata slice for the write pass.
    bytes: Option<&'a mut [u8]>,
    /// Next unreserved byte in the metadata buffer.
    position: usize,
}

impl<'a> FlatWriter<'a> {
    /// Creates a metadata count pass with no backing allocation.
    fn counter() -> Self {
        Self {
            bytes: None,
            position: 0,
        }
    }

    /// Creates a write pass over one exact metadata slice.
    fn output(bytes: &'a mut [u8]) -> Self {
        Self {
            bytes: Some(bytes),
            position: 0,
        }
    }

    /// Reserves aligned bytes and returns their start position.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::DecodedPayloadTooLarge`] on arithmetic overflow
    /// and [`ScribeError::InvalidFrame`] if the exact output slice is too short.
    fn reserve(&mut self, length: usize, alignment: usize) -> Result<usize, ScribeError> {
        let aligned = align_to(self.position, alignment)?;
        let end = aligned.checked_add(length).ok_or_else(material_overflow)?;
        if self.bytes.as_ref().is_some_and(|bytes| end > bytes.len()) {
            return Err(ScribeError::InvalidFrame);
        }
        self.position = end;
        Ok(aligned)
    }

    /// Reserves and initializes one table with its private vtable.
    ///
    /// # Errors
    ///
    /// Propagates checked reservation failures.
    fn table(
        &mut self,
        entries: &[u16],
        object_size: u16,
        object_alignment: usize,
    ) -> Result<usize, ScribeError> {
        let vtable_size = 4_usize
            .checked_add(entries.len().checked_mul(2).ok_or_else(material_overflow)?)
            .ok_or_else(material_overflow)?;
        let vtable = self.reserve(vtable_size, 2)?;
        self.put_u16(
            vtable,
            u16::try_from(vtable_size).map_err(|_| material_overflow())?,
        );
        self.put_u16(vtable + 2, object_size);
        for (index, entry) in entries.iter().enumerate() {
            self.put_u16(vtable + 4 + index * 2, *entry);
        }
        let object = self.reserve(usize::from(object_size), object_alignment)?;
        let distance = object
            .checked_sub(vtable)
            .ok_or(ScribeError::InvalidFrame)?;
        self.put_i32(
            object,
            i32::try_from(distance).map_err(|_| material_overflow())?,
        );
        Ok(object)
    }

    /// Reserves a vector whose first element must satisfy `element_alignment`.
    ///
    /// # Errors
    ///
    /// Returns a checked arithmetic error when the vector cannot be represented.
    fn vector(
        &mut self,
        elements: usize,
        element_size: usize,
        element_alignment: usize,
    ) -> Result<usize, ScribeError> {
        let length = elements
            .checked_mul(element_size)
            .and_then(|bytes| bytes.checked_add(4))
            .ok_or_else(material_overflow)?;
        let data_aligned = align_to(
            self.position.checked_add(4).ok_or_else(material_overflow)?,
            element_alignment.max(4),
        )?;
        let start = data_aligned
            .checked_sub(4)
            .ok_or(ScribeError::InvalidFrame)?;
        if start > self.position {
            self.reserve(start - self.position, 1)?;
        }
        let vector = self.reserve(length, 1)?;
        self.put_u32(
            vector,
            u32::try_from(elements).map_err(|_| material_overflow())?,
        );
        Ok(vector)
    }

    /// Reserves one UTF-8 `FlatBuffer` string.
    ///
    /// # Errors
    ///
    /// Returns a checked arithmetic error when the string cannot be represented.
    fn string(&mut self, value: &str) -> Result<usize, ScribeError> {
        let length = value.len().checked_add(5).ok_or_else(material_overflow)?;
        let start = self.reserve(length, 4)?;
        self.put_u32(
            start,
            u32::try_from(value.len()).map_err(|_| material_overflow())?,
        );
        self.put_bytes(start + 4, value.as_bytes());
        Ok(start)
    }

    /// Patches a forward `FlatBuffer` relative offset.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] for a backward reference and a
    /// material overflow error when the distance exceeds `u32`.
    fn put_offset(&mut self, at: usize, target: usize) -> Result<(), ScribeError> {
        let distance = target.checked_sub(at).ok_or(ScribeError::InvalidFrame)?;
        self.put_u32(
            at,
            u32::try_from(distance).map_err(|_| material_overflow())?,
        );
        Ok(())
    }

    /// Writes raw bytes when this is the materializing pass.
    fn put_bytes(&mut self, at: usize, value: &[u8]) {
        if let Some(bytes) = self.bytes.as_deref_mut() {
            bytes[at..at + value.len()].copy_from_slice(value);
        }
    }

    /// Writes one little-endian `u8` when materializing.
    fn put_u8(&mut self, at: usize, value: u8) {
        self.put_bytes(at, &[value]);
    }

    /// Writes one little-endian `u16` when materializing.
    fn put_u16(&mut self, at: usize, value: u16) {
        self.put_bytes(at, &value.to_le_bytes());
    }

    /// Writes one little-endian `i16` when materializing.
    fn put_i16(&mut self, at: usize, value: i16) {
        self.put_bytes(at, &value.to_le_bytes());
    }

    /// Writes one little-endian `u32` when materializing.
    fn put_u32(&mut self, at: usize, value: u32) {
        self.put_bytes(at, &value.to_le_bytes());
    }

    /// Writes one little-endian `i32` when materializing.
    fn put_i32(&mut self, at: usize, value: i32) {
        self.put_bytes(at, &value.to_le_bytes());
    }

    /// Writes one little-endian `i64` when materializing.
    fn put_i64(&mut self, at: usize, value: i64) {
        self.put_bytes(at, &value.to_le_bytes());
    }
}

/// Counts metadata by running its deterministic layout once without storage.
///
/// # Errors
///
/// Propagates unsupported-schema and checked layout failures.
fn metadata_len(
    write: impl FnOnce(&mut FlatWriter<'_>) -> Result<(), ScribeError>,
) -> Result<usize, ScribeError> {
    let mut writer = FlatWriter::counter();
    write(&mut writer)?;
    Ok(writer.position)
}

/// Writes one schema `Message` `FlatBuffer` through the minimal fixed writer.
///
/// # Errors
///
/// Returns an error for unsupported schema types or checked layout overflow.
fn write_schema_message(writer: &mut FlatWriter<'_>, schema: &Schema) -> Result<(), ScribeError> {
    let root = writer.reserve(4, 4)?;
    let message = writer.table(&[4, 6, 8, 16, 0], 24, 8)?;
    writer.put_i16(message + 4, arrow::ipc::MetadataVersion::V5.0);
    writer.put_u8(message + 6, arrow::ipc::MessageHeader::Schema.0);
    writer.put_i64(message + 16, 0);

    let schema_table = writer.table(&[4, 8, 0, 0], 12, 4)?;
    writer.put_i16(schema_table + 4, arrow::ipc::Endianness::Little.0);
    let fields = writer.vector(schema.fields().len(), 4, 4)?;
    writer.put_offset(schema_table + 8, fields)?;
    for (index, field) in schema.fields().iter().enumerate() {
        let field_table = writer.table(&[4, 8, 9, 12, 0, 0, 0], 16, 4)?;
        writer.put_offset(fields + 4 + index * 4, field_table)?;
        writer.put_u8(field_table + 8, u8::from(field.is_nullable()));
        let name = writer.string(field.name())?;
        writer.put_offset(field_table + 4, name)?;
        let encoded_type = write_type(writer, field.data_type())?;
        writer.put_u8(field_table + 9, encoded_type.tag);
        writer.put_offset(field_table + 12, encoded_type.table)?;
    }
    writer.put_offset(message + 8, schema_table)?;
    writer.put_offset(root, message)?;
    Ok(())
}

/// Writes one `RecordBatch` `Message` `FlatBuffer` from fixed physical facts.
///
/// # Errors
///
/// Returns a checked conversion or layout error when metadata is not representable.
fn write_batch_message(
    writer: &mut FlatWriter<'_>,
    rows: usize,
    facts: &BatchFacts,
) -> Result<(), ScribeError> {
    let root = writer.reserve(4, 4)?;
    let message = writer.table(&[4, 6, 8, 16, 0], 24, 8)?;
    writer.put_i16(message + 4, arrow::ipc::MetadataVersion::V5.0);
    writer.put_u8(message + 6, arrow::ipc::MessageHeader::RecordBatch.0);
    writer.put_i64(
        message + 16,
        i64::try_from(facts.body_bytes).map_err(|_| material_overflow())?,
    );

    let batch = writer.table(&[8, 16, 20, 0, 0], 24, 8)?;
    writer.put_i64(
        batch + 8,
        i64::try_from(rows).map_err(|_| material_overflow())?,
    );
    let nodes = writer.vector(facts.node_count, 16, 8)?;
    for (index, node) in facts.nodes[..facts.node_count].iter().enumerate() {
        let at = nodes + 4 + index * 16;
        writer.put_i64(at, node.length);
        writer.put_i64(at + 8, node.null_count);
    }
    writer.put_offset(batch + 16, nodes)?;
    let buffers = writer.vector(facts.buffer_count, 16, 8)?;
    for (index, buffer) in facts.buffers[..facts.buffer_count].iter().enumerate() {
        let at = buffers + 4 + index * 16;
        writer.put_i64(
            at,
            i64::try_from(buffer.offset).map_err(|_| material_overflow())?,
        );
        writer.put_i64(
            at + 8,
            i64::try_from(buffer.length).map_err(|_| material_overflow())?,
        );
    }
    writer.put_offset(batch + 20, buffers)?;
    writer.put_offset(message + 8, batch)?;
    writer.put_offset(root, message)?;
    Ok(())
}

/// Encoded union tag and table for one accepted Arrow scalar type.
struct EncodedType {
    /// Arrow Schema.fbs `Type` union discriminator.
    tag: u8,
    /// Forward target for the union table.
    table: usize,
}

/// Writes the type table for one canonical scalar field.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for types outside the canonical flat
/// subset and propagates checked metadata-layout errors.
fn write_type(
    writer: &mut FlatWriter<'_>,
    data_type: &DataType,
) -> Result<EncodedType, ScribeError> {
    let (tag, table) = match data_type {
        DataType::Null => (arrow::ipc::Type::Null.0, empty_table(writer)?),
        DataType::Boolean => (arrow::ipc::Type::Bool.0, empty_table(writer)?),
        DataType::Int8 => (arrow::ipc::Type::Int.0, int_table(writer, 8, true)?),
        DataType::Int16 => (arrow::ipc::Type::Int.0, int_table(writer, 16, true)?),
        DataType::Int32 => (arrow::ipc::Type::Int.0, int_table(writer, 32, true)?),
        DataType::Int64 => (arrow::ipc::Type::Int.0, int_table(writer, 64, true)?),
        DataType::UInt8 => (arrow::ipc::Type::Int.0, int_table(writer, 8, false)?),
        DataType::UInt16 => (arrow::ipc::Type::Int.0, int_table(writer, 16, false)?),
        DataType::UInt32 => (arrow::ipc::Type::Int.0, int_table(writer, 32, false)?),
        DataType::UInt64 => (arrow::ipc::Type::Int.0, int_table(writer, 64, false)?),
        DataType::Float16 => (
            arrow::ipc::Type::FloatingPoint.0,
            enum_table(writer, arrow::ipc::Precision::HALF.0)?,
        ),
        DataType::Float32 => (
            arrow::ipc::Type::FloatingPoint.0,
            enum_table(writer, arrow::ipc::Precision::SINGLE.0)?,
        ),
        DataType::Float64 => (
            arrow::ipc::Type::FloatingPoint.0,
            enum_table(writer, arrow::ipc::Precision::DOUBLE.0)?,
        ),
        DataType::Date32 => (
            arrow::ipc::Type::Date.0,
            enum_table(writer, arrow::ipc::DateUnit::DAY.0)?,
        ),
        DataType::Date64 => (
            arrow::ipc::Type::Date.0,
            enum_table(writer, arrow::ipc::DateUnit::MILLISECOND.0)?,
        ),
        DataType::Time32(unit) => (arrow::ipc::Type::Time.0, time_table(writer, *unit, 32)?),
        DataType::Time64(unit) => (arrow::ipc::Type::Time.0, time_table(writer, *unit, 64)?),
        DataType::Timestamp(unit, timezone) => (
            arrow::ipc::Type::Timestamp.0,
            timestamp_table(writer, *unit, timezone.as_deref())?,
        ),
        DataType::Duration(unit) => (
            arrow::ipc::Type::Duration.0,
            enum_table(writer, ipc_time_unit(*unit))?,
        ),
        DataType::Interval(unit) => (
            arrow::ipc::Type::Interval.0,
            enum_table(writer, ipc_interval_unit(*unit))?,
        ),
        DataType::Decimal128(precision, scale) => (
            arrow::ipc::Type::Decimal.0,
            decimal_table(writer, *precision, *scale, 128)?,
        ),
        DataType::Decimal256(precision, scale) => (
            arrow::ipc::Type::Decimal.0,
            decimal_table(writer, *precision, *scale, 256)?,
        ),
        DataType::Binary => (arrow::ipc::Type::Binary.0, empty_table(writer)?),
        DataType::LargeBinary => (arrow::ipc::Type::LargeBinary.0, empty_table(writer)?),
        DataType::Utf8 => (arrow::ipc::Type::Utf8.0, empty_table(writer)?),
        DataType::LargeUtf8 => (arrow::ipc::Type::LargeUtf8.0, empty_table(writer)?),
        DataType::FixedSizeBinary(width) if *width > 0 => (
            arrow::ipc::Type::FixedSizeBinary.0,
            scalar_i32_table(writer, *width)?,
        ),
        _ => return Err(ScribeError::InvalidFrame),
    };
    Ok(EncodedType { tag, table })
}

/// Writes an empty type table.
///
/// # Errors
///
/// Propagates checked metadata-layout failures.
fn empty_table(writer: &mut FlatWriter<'_>) -> Result<usize, ScribeError> {
    writer.table(&[], 4, 4)
}

/// Writes an integer type table.
///
/// # Errors
///
/// Propagates checked metadata-layout failures.
fn int_table(writer: &mut FlatWriter<'_>, width: i32, signed: bool) -> Result<usize, ScribeError> {
    let table = writer.table(&[4, 8], 12, 4)?;
    writer.put_i32(table + 4, width);
    writer.put_u8(table + 8, u8::from(signed));
    Ok(table)
}

/// Writes a one-enum type table.
///
/// # Errors
///
/// Propagates checked metadata-layout failures.
fn enum_table(writer: &mut FlatWriter<'_>, value: i16) -> Result<usize, ScribeError> {
    let table = writer.table(&[4], 8, 4)?;
    writer.put_i16(table + 4, value);
    Ok(table)
}

/// Writes a one-`i32` type table.
///
/// # Errors
///
/// Propagates checked metadata-layout failures.
fn scalar_i32_table(writer: &mut FlatWriter<'_>, value: i32) -> Result<usize, ScribeError> {
    let table = writer.table(&[4], 8, 4)?;
    writer.put_i32(table + 4, value);
    Ok(table)
}

/// Writes a Time type table after validating its legal width/unit pair.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for an illegal pair and propagates
/// checked metadata-layout failures.
fn time_table(
    writer: &mut FlatWriter<'_>,
    unit: TimeUnit,
    width: i32,
) -> Result<usize, ScribeError> {
    if !matches!(
        (unit, width),
        (TimeUnit::Second | TimeUnit::Millisecond, 32)
            | (TimeUnit::Microsecond | TimeUnit::Nanosecond, 64)
    ) {
        return Err(ScribeError::InvalidFrame);
    }
    let table = writer.table(&[4, 8], 12, 4)?;
    writer.put_i16(table + 4, ipc_time_unit(unit));
    writer.put_i32(table + 8, width);
    Ok(table)
}

/// Writes a Timestamp type table and its optional borrowed timezone text.
///
/// # Errors
///
/// Propagates checked metadata-layout failures.
fn timestamp_table(
    writer: &mut FlatWriter<'_>,
    unit: TimeUnit,
    timezone: Option<&str>,
) -> Result<usize, ScribeError> {
    let entries = if timezone.is_some() { [4, 8] } else { [4, 0] };
    let table = writer.table(&entries, 12, 4)?;
    writer.put_i16(table + 4, ipc_time_unit(unit));
    if let Some(timezone) = timezone {
        let value = writer.string(timezone)?;
        writer.put_offset(table + 8, value)?;
    }
    Ok(table)
}

/// Writes a Decimal type table for one accepted 128- or 256-bit width.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for invalid precision/scale/width and
/// propagates checked metadata-layout failures.
fn decimal_table(
    writer: &mut FlatWriter<'_>,
    precision: u8,
    scale: i8,
    width: i32,
) -> Result<usize, ScribeError> {
    let max_precision = if width == 128 { 38 } else { 76 };
    if precision == 0
        || usize::from(precision) > max_precision
        || usize::from(scale.unsigned_abs()) > usize::from(precision)
    {
        return Err(ScribeError::InvalidFrame);
    }
    let table = writer.table(&[4, 8, 12], 16, 4)?;
    writer.put_i32(table + 4, i32::from(precision));
    writer.put_i32(table + 8, i32::from(scale));
    writer.put_i32(table + 12, width);
    Ok(table)
}

/// Maps the Arrow public time unit to its IPC enum representation.
#[must_use]
fn ipc_time_unit(unit: TimeUnit) -> i16 {
    match unit {
        TimeUnit::Second => arrow::ipc::TimeUnit::SECOND.0,
        TimeUnit::Millisecond => arrow::ipc::TimeUnit::MILLISECOND.0,
        TimeUnit::Microsecond => arrow::ipc::TimeUnit::MICROSECOND.0,
        TimeUnit::Nanosecond => arrow::ipc::TimeUnit::NANOSECOND.0,
    }
}

/// Maps the Arrow public interval unit to its IPC enum representation.
#[must_use]
fn ipc_interval_unit(unit: IntervalUnit) -> i16 {
    match unit {
        IntervalUnit::YearMonth => arrow::ipc::IntervalUnit::YEAR_MONTH.0,
        IntervalUnit::DayTime => arrow::ipc::IntervalUnit::DAY_TIME.0,
        IntervalUnit::MonthDayNano => arrow::ipc::IntervalUnit::MONTH_DAY_NANO.0,
    }
}

/// Validates schema-level restrictions shared by count and write passes.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for empty/oversized schemas, schema
/// or field metadata, or unsupported fields.
fn validate_schema(schema: &Schema) -> Result<(), ScribeError> {
    if schema.fields().is_empty()
        || schema.fields().len() > MAX_FIELDS
        || !schema.metadata().is_empty()
    {
        return Err(ScribeError::InvalidFrame);
    }
    for field in schema.fields() {
        if !field.metadata().is_empty() {
            return Err(ScribeError::InvalidFrame);
        }
        validate_field(field)?;
    }
    Ok(())
}

/// Validates one canonical flat scalar field.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for unsupported types or invalid
/// type parameters.
fn validate_field(field: &Field) -> Result<(), ScribeError> {
    match field.data_type() {
        DataType::Null
        | DataType::Boolean
        | DataType::Int8
        | DataType::Int16
        | DataType::Int32
        | DataType::Int64
        | DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32
        | DataType::UInt64
        | DataType::Float16
        | DataType::Float32
        | DataType::Float64
        | DataType::Date32
        | DataType::Date64
        | DataType::Duration(_)
        | DataType::Interval(_)
        | DataType::Binary
        | DataType::LargeBinary
        | DataType::Utf8
        | DataType::LargeUtf8
        | DataType::Time32(TimeUnit::Second | TimeUnit::Millisecond)
        | DataType::Time64(TimeUnit::Microsecond | TimeUnit::Nanosecond)
        | DataType::Timestamp(_, _) => Ok(()),
        DataType::Decimal128(precision, scale) if decimal_valid(*precision, *scale, 38) => Ok(()),
        DataType::Decimal256(precision, scale) if decimal_valid(*precision, *scale, 76) => Ok(()),
        DataType::FixedSizeBinary(width) if *width > 0 => Ok(()),
        _ => Err(ScribeError::InvalidFrame),
    }
}

/// Returns the physical byte width for one accepted fixed-width scalar.
///
/// Boolean and variable-width fields return `None`; callers handle their
/// bitmap or offsets layout separately.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for unsupported types or invalid
/// fixed-size-binary widths.
fn fixed_value_width(data_type: &DataType) -> Result<Option<usize>, ScribeError> {
    let width = match data_type {
        DataType::Boolean
        | DataType::Binary
        | DataType::LargeBinary
        | DataType::Utf8
        | DataType::LargeUtf8 => return Ok(None),
        DataType::Int8 | DataType::UInt8 => 1,
        DataType::Int16 | DataType::UInt16 | DataType::Float16 => 2,
        DataType::Int32
        | DataType::UInt32
        | DataType::Float32
        | DataType::Date32
        | DataType::Time32(_)
        | DataType::Interval(IntervalUnit::YearMonth) => 4,
        DataType::Int64
        | DataType::UInt64
        | DataType::Float64
        | DataType::Date64
        | DataType::Time64(_)
        | DataType::Timestamp(_, _)
        | DataType::Duration(_)
        | DataType::Interval(IntervalUnit::DayTime) => 8,
        DataType::Decimal128(_, _) | DataType::Interval(IntervalUnit::MonthDayNano) => 16,
        DataType::Decimal256(_, _) => 32,
        DataType::FixedSizeBinary(width) if *width > 0 => {
            usize::try_from(*width).map_err(|_| ScribeError::InvalidFrame)?
        }
        _ => return Err(ScribeError::InvalidFrame),
    };
    Ok(Some(width))
}

/// Returns the public offset width for one accepted variable-width scalar.
#[must_use]
fn variable_offset_width(data_type: &DataType) -> Option<usize> {
    match data_type {
        DataType::Binary | DataType::Utf8 => Some(4),
        DataType::LargeBinary | DataType::LargeUtf8 => Some(8),
        _ => None,
    }
}

/// Returns whether one decimal precision and scale satisfy its width limit.
#[must_use]
fn decimal_valid(precision: u8, scale: i8, maximum: u8) -> bool {
    precision != 0 && precision <= maximum && scale.unsigned_abs() <= precision
}

/// Writes framed metadata at one output cursor and returns the body cursor.
///
/// # Errors
///
/// Returns a checked framing/layout error or propagates metadata encoding errors.
fn write_framed_metadata(
    output: &mut [u8],
    cursor: usize,
    metadata_bytes: usize,
    write: impl FnOnce(&mut FlatWriter<'_>) -> Result<(), ScribeError>,
) -> Result<usize, ScribeError> {
    let padded = align_eight(metadata_bytes)?;
    let end = cursor
        .checked_add(8)
        .and_then(|value| value.checked_add(padded))
        .ok_or_else(material_overflow)?;
    if end > output.len() {
        return Err(ScribeError::InvalidFrame);
    }
    output[cursor..cursor + 4].copy_from_slice(&CONTINUATION);
    output[cursor + 4..cursor + 8].copy_from_slice(
        &i32::try_from(padded)
            .map_err(|_| material_overflow())?
            .to_le_bytes(),
    );
    let metadata_end = cursor + 8 + metadata_bytes;
    let mut writer = FlatWriter::output(&mut output[cursor + 8..metadata_end]);
    write(&mut writer)?;
    if writer.position != metadata_bytes {
        return Err(ScribeError::InvalidFrame);
    }
    Ok(end)
}

/// Copies planned Arrow buffers directly into the exact final body region.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when a source changed after planning
/// or any destination range does not match the immutable descriptors.
fn write_body(
    output: &mut [u8],
    batch: &RecordBatch,
    facts: &BatchFacts,
) -> Result<(), ScribeError> {
    if output.len() != facts.body_bytes {
        return Err(ScribeError::InvalidFrame);
    }
    for fact in &facts.buffers[..facts.buffer_count] {
        if fact.length == 0 {
            continue;
        }
        let source = borrowed_buffer(batch.column(fact.column).as_ref(), fact.source)?;
        let source = source.get(..fact.length).ok_or(ScribeError::InvalidFrame)?;
        let end = fact
            .offset
            .checked_add(fact.length)
            .ok_or_else(material_overflow)?;
        output
            .get_mut(fact.offset..end)
            .ok_or(ScribeError::InvalidFrame)?
            .copy_from_slice(source);
    }
    Ok(())
}

/// Borrows one canonical physical buffer without constructing `ArrayData`.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when the concrete array does not match
/// its declared accepted type or the requested buffer is not part of its
/// canonical physical layout.
fn borrowed_buffer(array: &dyn Array, source: BufferSource) -> Result<&[u8], ScribeError> {
    if matches!(source, BufferSource::Validity) {
        return array
            .nulls()
            .map(|nulls| nulls.buffer().as_slice())
            .ok_or(ScribeError::InvalidFrame);
    }
    match (array.data_type(), source) {
        (DataType::Boolean, BufferSource::Values) => array
            .as_any()
            .downcast_ref::<BooleanArray>()
            .map(|values| values.values().inner().as_slice())
            .ok_or(ScribeError::InvalidFrame),
        (DataType::Int8, BufferSource::Values) => primitive_values::<Int8Type>(array),
        (DataType::Int16, BufferSource::Values) => primitive_values::<Int16Type>(array),
        (DataType::Int32, BufferSource::Values) => primitive_values::<Int32Type>(array),
        (DataType::Int64, BufferSource::Values) => primitive_values::<Int64Type>(array),
        (DataType::UInt8, BufferSource::Values) => primitive_values::<UInt8Type>(array),
        (DataType::UInt16, BufferSource::Values) => primitive_values::<UInt16Type>(array),
        (DataType::UInt32, BufferSource::Values) => primitive_values::<UInt32Type>(array),
        (DataType::UInt64, BufferSource::Values) => primitive_values::<UInt64Type>(array),
        (DataType::Float16, BufferSource::Values) => primitive_values::<Float16Type>(array),
        (DataType::Float32, BufferSource::Values) => primitive_values::<Float32Type>(array),
        (DataType::Float64, BufferSource::Values) => primitive_values::<Float64Type>(array),
        (DataType::Date32, BufferSource::Values) => primitive_values::<Date32Type>(array),
        (DataType::Date64, BufferSource::Values) => primitive_values::<Date64Type>(array),
        (DataType::Time32(TimeUnit::Second), BufferSource::Values) => {
            primitive_values::<Time32SecondType>(array)
        }
        (DataType::Time32(TimeUnit::Millisecond), BufferSource::Values) => {
            primitive_values::<Time32MillisecondType>(array)
        }
        (DataType::Time64(TimeUnit::Microsecond), BufferSource::Values) => {
            primitive_values::<Time64MicrosecondType>(array)
        }
        (DataType::Time64(TimeUnit::Nanosecond), BufferSource::Values) => {
            primitive_values::<Time64NanosecondType>(array)
        }
        (DataType::Timestamp(TimeUnit::Second, _), BufferSource::Values) => {
            primitive_values::<TimestampSecondType>(array)
        }
        (DataType::Timestamp(TimeUnit::Millisecond, _), BufferSource::Values) => {
            primitive_values::<TimestampMillisecondType>(array)
        }
        (DataType::Timestamp(TimeUnit::Microsecond, _), BufferSource::Values) => {
            primitive_values::<TimestampMicrosecondType>(array)
        }
        (DataType::Timestamp(TimeUnit::Nanosecond, _), BufferSource::Values) => {
            primitive_values::<TimestampNanosecondType>(array)
        }
        (DataType::Duration(TimeUnit::Second), BufferSource::Values) => {
            primitive_values::<DurationSecondType>(array)
        }
        (DataType::Duration(TimeUnit::Millisecond), BufferSource::Values) => {
            primitive_values::<DurationMillisecondType>(array)
        }
        (DataType::Duration(TimeUnit::Microsecond), BufferSource::Values) => {
            primitive_values::<DurationMicrosecondType>(array)
        }
        (DataType::Duration(TimeUnit::Nanosecond), BufferSource::Values) => {
            primitive_values::<DurationNanosecondType>(array)
        }
        (DataType::Interval(IntervalUnit::YearMonth), BufferSource::Values) => {
            primitive_values::<IntervalYearMonthType>(array)
        }
        (DataType::Interval(IntervalUnit::DayTime), BufferSource::Values) => {
            primitive_values::<IntervalDayTimeType>(array)
        }
        (DataType::Interval(IntervalUnit::MonthDayNano), BufferSource::Values) => {
            primitive_values::<IntervalMonthDayNanoType>(array)
        }
        (DataType::Decimal128(_, _), BufferSource::Values) => {
            primitive_values::<Decimal128Type>(array)
        }
        (DataType::Decimal256(_, _), BufferSource::Values) => {
            primitive_values::<Decimal256Type>(array)
        }
        (DataType::FixedSizeBinary(_), BufferSource::Values) => array
            .as_any()
            .downcast_ref::<FixedSizeBinaryArray>()
            .map(|values| values.values().as_slice())
            .ok_or(ScribeError::InvalidFrame),
        (DataType::Binary, BufferSource::Values) => byte_values::<BinaryArray>(array),
        (DataType::Binary, BufferSource::Offsets) => byte_offsets::<BinaryArray>(array),
        (DataType::LargeBinary, BufferSource::Values) => byte_values::<LargeBinaryArray>(array),
        (DataType::LargeBinary, BufferSource::Offsets) => byte_offsets::<LargeBinaryArray>(array),
        (DataType::Utf8, BufferSource::Values) => byte_values::<StringArray>(array),
        (DataType::Utf8, BufferSource::Offsets) => byte_offsets::<StringArray>(array),
        (DataType::LargeUtf8, BufferSource::Values) => byte_values::<LargeStringArray>(array),
        (DataType::LargeUtf8, BufferSource::Offsets) => byte_offsets::<LargeStringArray>(array),
        _ => Err(ScribeError::InvalidFrame),
    }
}

/// Borrows the raw native values of one concrete primitive array.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when the dynamic array is not the
/// expected Arrow primitive representation.
fn primitive_values<T: ArrowPrimitiveType>(array: &dyn Array) -> Result<&[u8], ScribeError> {
    array
        .as_any()
        .downcast_ref::<PrimitiveArray<T>>()
        .map(|values| values.values().inner().as_slice())
        .ok_or(ScribeError::InvalidFrame)
}

/// Capability implemented by the four accepted variable-width array shapes.
trait BorrowedByteArray: Array {
    /// Borrows the physical values buffer.
    fn borrowed_values(&self) -> &[u8];
    /// Borrows the physical offsets buffer as native little-endian bytes.
    fn borrowed_offsets(&self) -> &[u8];
}

macro_rules! impl_borrowed_byte_array {
    ($array:ty) => {
        impl BorrowedByteArray for $array {
            /// Borrows the concrete array's physical values buffer.
            fn borrowed_values(&self) -> &[u8] {
                self.values().as_slice()
            }

            /// Borrows the concrete array's physical offsets buffer.
            fn borrowed_offsets(&self) -> &[u8] {
                self.offsets().inner().inner().as_slice()
            }
        }
    };
}

impl_borrowed_byte_array!(BinaryArray);
impl_borrowed_byte_array!(LargeBinaryArray);
impl_borrowed_byte_array!(StringArray);
impl_borrowed_byte_array!(LargeStringArray);

/// Downcasts and borrows one variable-width values buffer.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when the concrete representation does
/// not match the declared data type.
fn byte_values<T: BorrowedByteArray + 'static>(array: &dyn Array) -> Result<&[u8], ScribeError> {
    array
        .as_any()
        .downcast_ref::<T>()
        .map(BorrowedByteArray::borrowed_values)
        .ok_or(ScribeError::InvalidFrame)
}

/// Downcasts and borrows one variable-width offsets buffer.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when the concrete representation does
/// not match the declared data type.
fn byte_offsets<T: BorrowedByteArray + 'static>(array: &dyn Array) -> Result<&[u8], ScribeError> {
    array
        .as_any()
        .downcast_ref::<T>()
        .map(BorrowedByteArray::borrowed_offsets)
        .ok_or(ScribeError::InvalidFrame)
}

/// Reads the last offset from a canonical variable-width offset buffer.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for truncated, negative, or unsupported
/// offsets.
fn read_terminal_offset(
    offsets: &[u8],
    logical_bytes: usize,
    width: usize,
) -> Result<usize, ScribeError> {
    let start = logical_bytes
        .checked_sub(width)
        .ok_or(ScribeError::InvalidFrame)?;
    let value = offsets
        .get(start..logical_bytes)
        .ok_or(ScribeError::InvalidFrame)?;
    match width {
        4 => {
            let bytes: [u8; 4] = value.try_into().map_err(|_| ScribeError::InvalidFrame)?;
            usize::try_from(i32::from_le_bytes(bytes)).map_err(|_| ScribeError::InvalidFrame)
        }
        8 => {
            let bytes: [u8; 8] = value.try_into().map_err(|_| ScribeError::InvalidFrame)?;
            usize::try_from(i64::from_le_bytes(bytes)).map_err(|_| ScribeError::InvalidFrame)
        }
        _ => Err(ScribeError::InvalidFrame),
    }
}

/// Returns the bytes required for a bitmap with `bits` logical values.
///
/// # Errors
///
/// Returns [`ScribeError::DecodedPayloadTooLarge`] on checked overflow.
fn bitmap_bytes(bits: usize) -> Result<usize, ScribeError> {
    bits.checked_add(7)
        .map(|value| value / 8)
        .ok_or_else(material_overflow)
}

/// Returns the complete framed metadata length for an unpadded `FlatBuffer`.
///
/// # Errors
///
/// Returns [`ScribeError::DecodedPayloadTooLarge`] on checked overflow.
fn framed_metadata_len(metadata: usize) -> Result<usize, ScribeError> {
    align_eight(metadata)?
        .checked_add(8)
        .ok_or_else(material_overflow)
}

/// Aligns one length to the Arrow IPC eight-byte boundary.
///
/// # Errors
///
/// Returns [`ScribeError::DecodedPayloadTooLarge`] on checked overflow.
fn align_eight(value: usize) -> Result<usize, ScribeError> {
    align_to(value, 8)
}

/// Aligns one position to a nonzero power-of-two boundary.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for an invalid alignment and
/// [`ScribeError::DecodedPayloadTooLarge`] on checked overflow.
fn align_to(value: usize, alignment: usize) -> Result<usize, ScribeError> {
    if alignment == 0 || !alignment.is_power_of_two() {
        return Err(ScribeError::InvalidFrame);
    }
    value
        .checked_add(alignment - 1)
        .map(|aligned| aligned & !(alignment - 1))
        .ok_or_else(material_overflow)
}

/// Creates the stable internal overflow result used by private IPC planning.
#[must_use]
fn material_overflow() -> ScribeError {
    ScribeError::DecodedPayloadTooLarge {
        bytes: usize::MAX,
        limit: usize::MAX - 1,
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::mem::size_of;
    use std::sync::Arc;

    use arrow::array::{ArrayRef, BinaryArray, BooleanArray, Int64Array, NullArray, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::reader::StreamReader;
    use arrow::ipc::writer::StreamWriter;

    use super::*;

    /// Decodes one IPC stream and returns its only record batch.
    ///
    /// # Panics
    ///
    /// Panics when a test encoder emits invalid framing or more than one batch.
    fn decode_one(bytes: Vec<u8>) -> RecordBatch {
        let mut reader = StreamReader::try_new(Cursor::new(bytes), None).expect("valid IPC stream");
        let batch = reader
            .next()
            .expect("one record batch")
            .expect("decodable record batch");
        assert!(reader.next().is_none());
        batch
    }

    /// Encodes through Arrow's reference writer and decodes its semantic batch.
    ///
    /// # Panics
    ///
    /// Panics if Arrow's reference writer rejects the valid fixture.
    fn reference_roundtrip(batch: &RecordBatch) -> RecordBatch {
        let mut bytes = Vec::new();
        {
            let mut writer =
                StreamWriter::try_new(&mut bytes, batch.schema_ref()).expect("reference writer");
            writer.write(batch).expect("reference record batch");
            writer.finish().expect("reference EOS");
        }
        decode_one(bytes)
    }

    /// Builds a mixed scalar/null/variable fixture with punctuation in names.
    ///
    /// # Panics
    ///
    /// Panics if the fixture arrays do not satisfy their declared schema.
    fn mixed_batch() -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id.with/slash", DataType::Int64, true),
            Field::new("enabled?", DataType::Boolean, true),
            Field::new("message 🙂", DataType::Utf8, true),
            Field::new("payload", DataType::Binary, true),
            Field::new("nothing", DataType::Null, true),
        ]));
        let columns: Vec<ArrayRef> = vec![
            Arc::new(Int64Array::from(vec![Some(7), None, Some(-9)])),
            Arc::new(BooleanArray::from(vec![Some(true), Some(false), None])),
            Arc::new(StringArray::from(vec![Some("alpha"), None, Some("βeta")])),
            Arc::new(BinaryArray::from(vec![
                Some(&b"x"[..]),
                Some(&b""[..]),
                None,
            ])),
            Arc::new(NullArray::new(3)),
        ];
        RecordBatch::try_new(schema, columns).expect("valid mixed batch")
    }

    /// Proves exact count, exact capacity, V5 readability, and semantic parity.
    #[test]
    fn exact_fixed_stream_roundtrips_mixed_scalar_batch() {
        let batch = mixed_batch();
        let plan = FixedIpcPlan::count(&batch).expect("count fixed IPC");
        let encoded = plan.encode(&batch).expect("encode fixed IPC");

        assert_eq!(encoded.len(), plan.encoded_bytes());
        assert_eq!(encoded.capacity(), encoded.len());
        let observed = decode_one(encoded);
        let reference = reference_roundtrip(&batch);
        assert_eq!(observed, batch);
        assert_eq!(observed, reference);
    }

    /// Proves projection facts produce the same exact plan before arrays exist.
    #[test]
    fn pre_material_column_facts_encode_the_observed_batch() {
        let batch = mixed_batch();
        let columns = [
            FixedIpcColumnPlan {
                null_count: 1,
                validity_bytes: 1,
                offsets_bytes: 0,
                values_bytes: 24,
            },
            FixedIpcColumnPlan {
                null_count: 1,
                validity_bytes: 1,
                offsets_bytes: 0,
                values_bytes: 1,
            },
            FixedIpcColumnPlan {
                null_count: 1,
                validity_bytes: 1,
                offsets_bytes: 16,
                values_bytes: 10,
            },
            FixedIpcColumnPlan {
                null_count: 1,
                validity_bytes: 1,
                offsets_bytes: 16,
                values_bytes: 1,
            },
            FixedIpcColumnPlan {
                null_count: 3,
                validity_bytes: 0,
                offsets_bytes: 0,
                values_bytes: 0,
            },
        ];
        let plan = FixedIpcPlan::count_schema(batch.schema_ref(), 3, columns.into_iter())
            .expect("pre-material IPC plan");
        let encoded = plan.encode(&batch).expect("planned batch encoding");

        assert_eq!(encoded.len(), plan.encoded_bytes());
        assert_eq!(encoded.capacity(), encoded.len());
        assert_eq!(decode_one(encoded), batch);
    }

    /// Proves projection facts with a noncanonical offsets length fail closed.
    #[test]
    fn pre_material_column_facts_reject_wrong_offsets() {
        let schema = Schema::new(vec![Field::new("message", DataType::Utf8, false)]);
        let columns = [FixedIpcColumnPlan {
            null_count: 0,
            validity_bytes: 0,
            offsets_bytes: 8,
            values_bytes: 5,
        }];

        assert!(matches!(
            FixedIpcPlan::count_schema(&schema, 3, columns.into_iter()),
            Err(ScribeError::InvalidFrame)
        ));
    }

    /// Proves changed batch facts cannot consume a previously counted plan.
    #[test]
    fn fixed_plan_rejects_changed_batch() {
        let batch = mixed_batch();
        let plan = FixedIpcPlan::count(&batch).expect("count fixed IPC");
        let changed = RecordBatch::try_new(
            batch.schema(),
            vec![
                Arc::new(Int64Array::from(vec![Some(7), None])) as ArrayRef,
                Arc::new(BooleanArray::from(vec![Some(true), Some(false)])),
                Arc::new(StringArray::from(vec![Some("alpha"), None])),
                Arc::new(BinaryArray::from(vec![Some(&b"x"[..]), Some(&b""[..])])),
                Arc::new(NullArray::new(2)),
            ],
        )
        .expect("valid changed batch");

        assert!(matches!(
            plan.encode(&changed),
            Err(ScribeError::InvalidFrame)
        ));
    }

    /// Proves nested and metadata-bearing schemas fail before IPC allocation.
    #[test]
    fn fixed_plan_rejects_noncanonical_schema() {
        let field = Field::new(
            "nested",
            DataType::List(Arc::new(Field::new("item", DataType::Int64, true))),
            true,
        );
        let schema = Arc::new(Schema::new(vec![field]));
        let array = arrow::array::ListArray::from_iter_primitive::<arrow::datatypes::Int64Type, _, _>(
            vec![Some(vec![Some(1_i64)])],
        );
        let batch = RecordBatch::try_new(schema, vec![Arc::new(array)]).expect("valid list batch");

        assert!(matches!(
            FixedIpcPlan::count(&batch),
            Err(ScribeError::InvalidFrame)
        ));
    }

    /// Proves type layout constants retain their expected wire sizes.
    #[test]
    fn ipc_inline_struct_assumptions_match_arrow() {
        assert_eq!(size_of::<arrow::ipc::FieldNode>(), 16);
        assert_eq!(size_of::<arrow::ipc::Buffer>(), 16);
    }
}
