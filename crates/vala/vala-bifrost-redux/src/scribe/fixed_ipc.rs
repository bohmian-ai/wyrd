//! Exact-capacity Arrow IPC V5 encoding for one admitted Scribe slice.

use arrow::array::{
    Array, BinaryArray, BooleanArray, FixedSizeBinaryArray, LargeBinaryArray, LargeStringArray,
    ListArray, NullArray, PrimitiveArray, RecordBatch, StringArray, StructArray,
};
use arrow::datatypes::{
    ArrowPrimitiveType, DataType, Date32Type, Date64Type, Decimal128Type, Decimal256Type,
    DurationMicrosecondType, DurationMillisecondType, DurationNanosecondType, DurationSecondType,
    Field, Fields, Float16Type, Float32Type, Float64Type, Int8Type, Int16Type, Int32Type,
    Int64Type, IntervalDayTimeType, IntervalMonthDayNanoType, IntervalUnit, IntervalYearMonthType,
    Schema, Time32MillisecondType, Time32SecondType, Time64MicrosecondType, Time64NanosecondType,
    TimeUnit, TimestampMicrosecondType, TimestampMillisecondType, TimestampNanosecondType,
    TimestampSecondType, UInt8Type, UInt16Type, UInt32Type, UInt64Type,
};

use crate::contracts::ScribeError;

/// Maximum number of Arrow IPC field nodes accepted by the native V1 contract.
///
/// A nested field contributes one node per level, so this bounds the complete
/// depth-first node list rather than the record batch's top-level column count.
const MAX_FIELDS: usize = 256;
/// Maximum number of physical buffers emitted across every accepted node.
const MAX_BUFFERS: usize = MAX_FIELDS * 3;
/// Maximum accepted nesting depth of one canonical field.
///
/// The deepest shape the canonical signal ledgers declare is
/// `List<Struct<scalar>>`, so this leaves headroom while keeping the recursive
/// schema walk bounded by a constant rather than by caller input.
const MAX_NESTING_DEPTH: usize = 8;
/// Maximum accepted `custom_metadata` entries on one field.
///
/// Canonical fields carry at most a stable field id and a sensitivity tag. The
/// bound lets both metadata passes sort entries in fixed inline storage so the
/// counted and written `FlatBuffer` layouts cannot diverge on map iteration
/// order.
const MAX_FIELD_METADATA: usize = 8;
/// Arrow stream continuation marker used before every V5 metadata message.
const CONTINUATION: [u8; 4] = [0xff; 4];

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
        let schema = batch.schema();
        let fields = schema.fields().len();
        if fields == 0 || fields != batch.num_columns() {
            return Err(ScribeError::InvalidFrame);
        }
        let mut facts = Self {
            nodes: fixed_box(MAX_FIELDS, FieldNodeFact::EMPTY),
            node_count: 0,
            buffers: fixed_box(MAX_BUFFERS, BufferFact::EMPTY),
            buffer_count: 0,
            body_bytes: 0,
        };
        for (field, array) in schema.fields().iter().zip(batch.columns()) {
            if array.len() != batch.num_rows() {
                return Err(ScribeError::InvalidFrame);
            }
            visit_nodes(field, array.as_ref(), 0, &mut |array| {
                facts.push_node(array)
            })?;
        }
        Ok(facts)
    }

    /// Records one visited array's field node and its canonical buffers.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when the fixed node table is
    /// exhausted or a physical buffer contradicts its array, and
    /// [`ScribeError::DecodedPayloadTooLarge`] on checked length overflow.
    fn push_node(&mut self, array: &dyn Array) -> Result<(), ScribeError> {
        if self.node_count == MAX_FIELDS {
            return Err(ScribeError::InvalidFrame);
        }
        self.nodes[self.node_count] = FieldNodeFact {
            length: i64::try_from(array.len()).map_err(|_| material_overflow())?,
            null_count: i64::try_from(array.logical_null_count())
                .map_err(|_| material_overflow())?,
        };
        self.node_count += 1;
        array_buffers(array, &mut |source, length| {
            self.push_buffer(source, length)
        })
    }

    /// Records one physical buffer and advances the aligned body cursor.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when the fixed descriptor array is
    /// exhausted and [`ScribeError::DecodedPayloadTooLarge`] on overflow.
    fn push_buffer(&mut self, source: BufferSource, length: usize) -> Result<(), ScribeError> {
        if self.buffer_count == MAX_BUFFERS {
            return Err(ScribeError::InvalidFrame);
        }
        self.buffers[self.buffer_count] = BufferFact {
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

/// Visits one array and its canonical children in Arrow IPC field-node order.
///
/// Arrow lays a record batch out depth-first: a node's own field node and
/// buffers come first, then its children's, then its siblings'. Counting and
/// writing share this one walk so the plan's descriptor order and the body
/// writer's traversal cannot drift apart.
///
/// Slices are refused. The encoder writes each array's storage verbatim, so an
/// array with a nonzero offset — or a list whose values buffer holds slack past
/// its terminal offset — would produce a stream that decodes to a different
/// batch than the one that was counted.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when nesting exceeds
/// [`MAX_NESTING_DEPTH`], the concrete array does not match its declared field,
/// an array is sliced, a non-nullable field carries nulls, or a list's child
/// length diverges from its terminal offset. Propagates the visitor's error.
fn visit_nodes(
    field: &Field,
    array: &dyn Array,
    depth: usize,
    visit: &mut dyn FnMut(&dyn Array) -> Result<(), ScribeError>,
) -> Result<(), ScribeError> {
    if depth > MAX_NESTING_DEPTH
        || array.offset() != 0
        || array.data_type() != field.data_type()
        || (!field.is_nullable() && array.logical_null_count() != 0)
    {
        return Err(ScribeError::InvalidFrame);
    }
    visit(array)?;
    match field.data_type() {
        DataType::List(item) => {
            let list = array
                .as_any()
                .downcast_ref::<ListArray>()
                .ok_or(ScribeError::InvalidFrame)?;
            let values = list.values();
            let terminal = list
                .offsets()
                .last()
                .copied()
                .and_then(|offset| usize::try_from(offset).ok())
                .ok_or(ScribeError::InvalidFrame)?;
            if values.len() != terminal {
                return Err(ScribeError::InvalidFrame);
            }
            visit_nodes(item, values.as_ref(), depth + 1, visit)
        }
        DataType::Struct(children) => {
            let record = array
                .as_any()
                .downcast_ref::<StructArray>()
                .ok_or(ScribeError::InvalidFrame)?;
            if record.columns().len() != children.len() {
                return Err(ScribeError::InvalidFrame);
            }
            for (child, column) in children.iter().zip(record.columns()) {
                if column.len() != record.len() {
                    return Err(ScribeError::InvalidFrame);
                }
                visit_nodes(child, column.as_ref(), depth + 1, visit)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Emits the canonical physical buffers of one array in Arrow IPC order.
///
/// Nested arrays contribute only their own buffers: a `Struct` owns just its
/// validity bitmap and a `List` its validity bitmap and offsets, because their
/// values live in child nodes the caller's walk reaches separately.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for a type outside the accepted
/// subset, malformed offsets, or physical storage shorter than its logical
/// length. Returns [`ScribeError::DecodedPayloadTooLarge`] on checked length
/// overflow. Propagates the emitter's error.
fn array_buffers(
    array: &dyn Array,
    emit: &mut dyn FnMut(BufferSource, usize) -> Result<(), ScribeError>,
) -> Result<(), ScribeError> {
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
    emit(BufferSource::Validity, validity_len)?;

    match data_type {
        DataType::Struct(_) => Ok(()),
        DataType::List(_) => emit_offsets(array, 4, emit).map(|_| ()),
        DataType::Boolean => emit_values(array, bitmap_bytes(array.len())?, emit),
        DataType::Binary | DataType::Utf8 => {
            let terminal = emit_offsets(array, 4, emit)?;
            emit_values(array, terminal, emit)
        }
        DataType::LargeBinary | DataType::LargeUtf8 => {
            let terminal = emit_offsets(array, 8, emit)?;
            emit_values(array, terminal, emit)
        }
        _ => {
            let width = fixed_value_width(data_type)?.ok_or(ScribeError::InvalidFrame)?;
            let length = array
                .len()
                .checked_mul(width)
                .ok_or_else(material_overflow)?;
            emit_values(array, length, emit)
        }
    }
}

/// Emits one offsets buffer and returns the terminal offset it declares.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for storage shorter than the logical
/// offsets length or a malformed terminal offset, and
/// [`ScribeError::DecodedPayloadTooLarge`] on checked length overflow.
fn emit_offsets(
    array: &dyn Array,
    offset_width: usize,
    emit: &mut dyn FnMut(BufferSource, usize) -> Result<(), ScribeError>,
) -> Result<usize, ScribeError> {
    let offsets = array
        .len()
        .checked_add(1)
        .and_then(|count| count.checked_mul(offset_width))
        .ok_or_else(material_overflow)?;
    let source = borrowed_buffer(array, BufferSource::Offsets)?;
    if source.len() < offsets {
        return Err(ScribeError::InvalidFrame);
    }
    emit(BufferSource::Offsets, offsets)?;
    read_terminal_offset(source, offsets, offset_width)
}

/// Emits one values buffer after checking its borrowed source is long enough.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when the source buffer is shorter than
/// `length`. Propagates the emitter's error.
fn emit_values(
    array: &dyn Array,
    length: usize,
    emit: &mut dyn FnMut(BufferSource, usize) -> Result<(), ScribeError>,
) -> Result<(), ScribeError> {
    if borrowed_buffer(array, BufferSource::Values)?.len() < length {
        return Err(ScribeError::InvalidFrame);
    }
    emit(BufferSource::Values, length)
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
///
/// Buffers are identified by their position in the depth-first walk rather than
/// by a column index: a nested field contributes its own buffers between its
/// parent's and its siblings', so no single top-level column owns them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BufferFact {
    /// Borrowed physical source within the owning array.
    source: BufferSource,
    /// Aligned byte offset in the record-batch body.
    offset: usize,
    /// Logical buffer bytes before alignment padding.
    length: usize,
}

impl BufferFact {
    /// Zero placeholder for unused fixed array entries.
    const EMPTY: Self = Self {
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
        let field_table = write_field(writer, field)?;
        writer.put_offset(fields + 4 + index * 4, field_table)?;
    }
    writer.put_offset(message + 8, schema_table)?;
    writer.put_offset(root, message)?;
    Ok(())
}

/// Writes one `Field` table, recursing into its children.
///
/// The table always reserves the `children` and `custom_metadata` slots so a
/// flat and a nested field share one layout; an absent value becomes an empty
/// vector rather than a second vtable shape.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for a type outside the accepted subset
/// and propagates checked metadata-layout failures.
fn write_field(writer: &mut FlatWriter<'_>, field: &Field) -> Result<usize, ScribeError> {
    let field_table = writer.table(&[4, 8, 9, 12, 0, 16, 20], 24, 4)?;
    writer.put_u8(field_table + 8, u8::from(field.is_nullable()));
    let name = writer.string(field.name())?;
    writer.put_offset(field_table + 4, name)?;
    let encoded_type = write_type(writer, field.data_type())?;
    writer.put_u8(field_table + 9, encoded_type.tag);
    writer.put_offset(field_table + 12, encoded_type.table)?;

    let children = field_children(field.data_type());
    let children_vector = writer.vector(children.len(), 4, 4)?;
    writer.put_offset(field_table + 16, children_vector)?;
    for (index, child) in children.iter().enumerate() {
        let child_table = write_field(writer, child)?;
        writer.put_offset(children_vector + 4 + index * 4, child_table)?;
    }

    let metadata = write_custom_metadata(writer, field)?;
    writer.put_offset(field_table + 20, metadata)?;
    Ok(field_table)
}

/// Returns the declared child fields of one accepted nested type.
///
/// Scalar types have none, so the caller writes an empty children vector.
#[must_use]
fn field_children(data_type: &DataType) -> Fields {
    match data_type {
        DataType::List(item) => std::slice::from_ref(item).into(),
        DataType::Struct(children) => children.clone(),
        _ => Fields::empty(),
    }
}

/// Writes one field's `custom_metadata` vector in stable key order.
///
/// Arrow stores field metadata in a `HashMap`, whose iteration order is not a
/// contract. Both metadata passes must lay out identical bytes, so entries are
/// sorted by key in fixed inline storage before they are written.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when the field carries more than
/// [`MAX_FIELD_METADATA`] entries, and propagates checked layout failures.
fn write_custom_metadata(writer: &mut FlatWriter<'_>, field: &Field) -> Result<usize, ScribeError> {
    let mut entries: [Option<(&str, &str)>; MAX_FIELD_METADATA] = [None; MAX_FIELD_METADATA];
    let mut count = 0_usize;
    for (key, value) in field.metadata() {
        if count == MAX_FIELD_METADATA {
            return Err(ScribeError::InvalidFrame);
        }
        entries[count] = Some((key.as_str(), value.as_str()));
        count += 1;
    }
    entries[..count].sort_unstable();

    let vector = writer.vector(count, 4, 4)?;
    for (index, entry) in entries[..count].iter().enumerate() {
        let (key, value) = entry.ok_or(ScribeError::InvalidFrame)?;
        let pair = writer.table(&[4, 8], 12, 4)?;
        writer.put_offset(vector + 4 + index * 4, pair)?;
        let key = writer.string(key)?;
        writer.put_offset(pair + 4, key)?;
        let value = writer.string(value)?;
        writer.put_offset(pair + 8, value)?;
    }
    Ok(vector)
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
        DataType::List(_) => (arrow::ipc::Type::List.0, empty_table(writer)?),
        DataType::Struct(fields) if !fields.is_empty() => {
            (arrow::ipc::Type::Struct_.0, empty_table(writer)?)
        }
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
        validate_field(field, 0)?;
    }
    Ok(())
}

/// Validates one canonical field and, for `List`/`Struct`, its children.
///
/// Field `custom_metadata` is accepted and preserved: the canonical signal
/// ledgers carry their stable field ids and sensitivity tags there, and
/// dropping them at the WAL boundary would lose the identity replay needs.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for unsupported types, invalid type
/// parameters, an empty struct, metadata beyond [`MAX_FIELD_METADATA`], or
/// nesting beyond [`MAX_NESTING_DEPTH`].
fn validate_field(field: &Field, depth: usize) -> Result<(), ScribeError> {
    if depth > MAX_NESTING_DEPTH || field.metadata().len() > MAX_FIELD_METADATA {
        return Err(ScribeError::InvalidFrame);
    }
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
        DataType::List(item) => validate_field(item, depth + 1),
        DataType::Struct(children) if !children.is_empty() => {
            for child in children {
                validate_field(child, depth + 1)?;
            }
            Ok(())
        }
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
    let mut cursor = 0_usize;
    let schema = batch.schema();
    for (field, array) in schema.fields().iter().zip(batch.columns()) {
        visit_nodes(field, array.as_ref(), 0, &mut |array| {
            array_buffers(array, &mut |source, length| {
                let fact = facts
                    .buffers
                    .get(cursor)
                    .filter(|_| cursor < facts.buffer_count)
                    .ok_or(ScribeError::InvalidFrame)?;
                cursor += 1;
                if fact.source != source || fact.length != length {
                    return Err(ScribeError::InvalidFrame);
                }
                if length == 0 {
                    return Ok(());
                }
                let bytes = borrowed_buffer(array, source)?
                    .get(..length)
                    .ok_or(ScribeError::InvalidFrame)?;
                let end = fact
                    .offset
                    .checked_add(length)
                    .ok_or_else(material_overflow)?;
                output
                    .get_mut(fact.offset..end)
                    .ok_or(ScribeError::InvalidFrame)?
                    .copy_from_slice(bytes);
                Ok(())
            })
        })?;
    }
    if cursor != facts.buffer_count {
        return Err(ScribeError::InvalidFrame);
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
        (DataType::List(_), BufferSource::Offsets) => array
            .as_any()
            .downcast_ref::<ListArray>()
            .map(|list| list.offsets().inner().inner().as_slice())
            .ok_or(ScribeError::InvalidFrame),
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
    use std::collections::HashMap;
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

    /// Builds the canonical nested fixture: metadata, `List`, and `List<Struct>`.
    ///
    /// This mirrors the shape the canonical signal ledgers actually produce —
    /// per-field `PARQUET:field_id` metadata, a repeated scalar column, and a
    /// repeated record column — so the encoder is proved against the real
    /// canonical layout rather than a synthetic nested toy.
    ///
    /// # Panics
    ///
    /// Panics if the fixture arrays do not satisfy their declared schema.
    fn nested_batch() -> RecordBatch {
        use arrow::array::{Float64Array, ListArray, StructArray};
        use arrow::buffer::OffsetBuffer;
        use arrow::datatypes::Fields;

        fn with_id(field: Field, id: i32) -> Field {
            field.with_metadata(HashMap::from([(
                "PARQUET:field_id".to_owned(),
                id.to_string(),
            )]))
        }

        let bucket_item = Arc::new(with_id(Field::new("item", DataType::Int64, false), 11));
        let buckets = ListArray::new(
            Arc::clone(&bucket_item),
            OffsetBuffer::new(vec![0, 2, 2, 5].into()),
            Arc::new(Int64Array::from(vec![1_i64, 2, 3, 4, 5])) as ArrayRef,
            Some(vec![true, false, true].into()),
        );

        let quantile_fields: Fields = vec![
            Arc::new(with_id(
                Field::new("quantile", DataType::Float64, false),
                21,
            )),
            Arc::new(with_id(Field::new("value", DataType::Float64, true), 22)),
        ]
        .into();
        let quantile_struct = StructArray::new(
            quantile_fields.clone(),
            vec![
                Arc::new(Float64Array::from(vec![0.5_f64, 0.9, 0.99])) as ArrayRef,
                Arc::new(Float64Array::from(vec![Some(1.0_f64), None, Some(3.0)])) as ArrayRef,
            ],
            None,
        );
        let quantile_item = Arc::new(with_id(
            Field::new("item", DataType::Struct(quantile_fields), false),
            20,
        ));
        let quantiles = ListArray::new(
            Arc::clone(&quantile_item),
            OffsetBuffer::new(vec![0, 1, 3, 3].into()),
            Arc::new(quantile_struct) as ArrayRef,
            None,
        );

        let schema = Arc::new(Schema::new(vec![
            with_id(Field::new("metric_name", DataType::Utf8, false), 1),
            with_id(
                Field::new("bucket_counts", DataType::List(bucket_item), true),
                10,
            ),
            with_id(
                Field::new("quantile_values", DataType::List(quantile_item), false),
                19,
            ),
        ]));
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec!["a", "b", "c"])) as ArrayRef,
                Arc::new(buckets) as ArrayRef,
                Arc::new(quantiles) as ArrayRef,
            ],
        )
        .expect("valid nested batch")
    }

    /// Proves the recursive walk encodes nested, metadata-bearing canonical rows.
    ///
    /// Nested Arrow is the shape every canonical signal batch has, and this
    /// encoder is the sole WAL writer, so exact capacity and reference parity
    /// over `List` and `List<Struct>` is what makes canonical rows persistable.
    #[test]
    fn recursive_plan_roundtrips_nested_metadata_batch() {
        let batch = nested_batch();
        let plan = FixedIpcPlan::count(&batch).expect("count nested IPC");
        let encoded = plan.encode(&batch).expect("encode nested IPC");

        assert_eq!(encoded.len(), plan.encoded_bytes());
        assert_eq!(encoded.capacity(), encoded.len());
        let observed = decode_one(encoded);
        assert_eq!(observed, batch);
        assert_eq!(observed, reference_roundtrip(&batch));
    }

    /// Proves a schema outside the recursive accepted subset still fails closed.
    #[test]
    fn fixed_plan_rejects_noncanonical_schema() {
        let field = Field::new(
            "dictionary",
            DataType::Dictionary(Box::new(DataType::Int32), Box::new(DataType::Utf8)),
            true,
        );
        let schema = Arc::new(Schema::new(vec![field]));
        let mut values =
            arrow::array::StringDictionaryBuilder::<arrow::datatypes::Int32Type>::new();
        values.append_value("alpha");
        let batch = RecordBatch::try_new(schema, vec![Arc::new(values.finish()) as ArrayRef])
            .expect("valid dictionary batch");

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
