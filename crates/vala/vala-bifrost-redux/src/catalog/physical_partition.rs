//! Complete physical partition identity and the one scheme that computes it.
//!
//! A [`PhysicalPartition`] is the time partition plus the partition-spec id and
//! every additional dimension's typed transformed value. WAL slices, seal
//! keys, staged members, claims, published files, and Forge rewrites all carry
//! this one value, encode it through [`PhysicalPartition::encode`] or
//! [`PhysicalPartition::to_wire`], and compare it whole, so two artifacts share
//! a partition only when spec, time, and every typed value — null included —
//! agree. [`PartitionScheme`] is the one place a row is mapped to its
//! partition, applying Iceberg's own transforms so Bifrost and Iceberg never
//! disagree about a bucket or truncation.

use arrow::array::{Array, ArrayRef, Int32Array, Int64Array, StringArray};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Utc};
use iceberg::spec::{Literal, PartitionSpec, PrimitiveLiteral, Struct};
use wyrd_spec::vala::api::{PartitionValueWire, PhysicalLayoutWire, PhysicalPartitionWire};

use super::layout::{
    BIFROST_PARTITION_SPEC_ID, LayoutRevision, MAX_PARTITION_FIELDS, PartitionTransform, TimeAxis,
    TimeColumnValues, TimeGranularity, TimePartition, TimePartitionError,
};

/// Largest number of additional dimensions one identity carries.
pub const MAX_PARTITION_DIMENSIONS: usize = MAX_PARTITION_FIELDS - 1;

/// Durable tag of a null dimension value.
const VALUE_NULL: u8 = 0;
/// Durable tag of a string dimension value.
const VALUE_UTF8: u8 = 1;
/// Durable tag of a 32-bit integer dimension value.
const VALUE_INT32: u8 = 2;
/// Durable tag of a 64-bit integer dimension value.
const VALUE_INT64: u8 = 3;

/// The complete partition one slice, file, or claim belongs to.
///
/// Ordered by time first, so a sorted list keeps one time partition's
/// identities together.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysicalPartition {
    /// Required time partition.
    time: TimePartition,
    /// Iceberg partition-spec id the identity was computed under.
    spec_id: i32,
    /// Position of the time field among the spec's fields.
    time_index: u8,
    /// Additional dimension values in spec order; `None` is a null value.
    values: Vec<Option<PartitionValueWire>>,
}

impl PhysicalPartition {
    /// Constructs one identity after checking its shape.
    ///
    /// # Errors
    ///
    /// Returns [`PhysicalPartitionError::Shape`] when more than
    /// [`MAX_PARTITION_DIMENSIONS`] values are supplied or `time_index` lies
    /// beyond the field list.
    pub fn new(
        spec_id: i32,
        time: TimePartition,
        time_index: u8,
        values: Vec<Option<PartitionValueWire>>,
    ) -> Result<Self, PhysicalPartitionError> {
        if values.len() > MAX_PARTITION_DIMENSIONS || usize::from(time_index) > values.len() {
            return Err(PhysicalPartitionError::Shape);
        }
        Ok(Self {
            time,
            spec_id,
            time_index,
            values,
        })
    }

    /// The identity of a table partitioned on time alone under its initial
    /// spec.
    #[must_use]
    pub const fn time_only(time: TimePartition) -> Self {
        Self {
            time,
            spec_id: BIFROST_PARTITION_SPEC_ID,
            time_index: 0,
            values: Vec::new(),
        }
    }

    /// Returns the time partition.
    #[must_use]
    pub const fn time(&self) -> TimePartition {
        self.time
    }

    /// Returns the partition-spec id.
    #[must_use]
    pub const fn spec_id(&self) -> i32 {
        self.spec_id
    }

    /// Returns the additional dimension values in spec order.
    #[must_use]
    pub fn values(&self) -> &[Option<PartitionValueWire>] {
        &self.values
    }

    /// Appends the canonical binary encoding: the time tag and start, then
    /// [`Self::encode_dimensions`].
    ///
    /// Digests and WAL slices hash or store exactly these bytes, so the
    /// encoding is injective: a different spec, time, type, or value always
    /// encodes differently.
    pub fn encode(&self, out: &mut Vec<u8>) {
        out.push(self.time.granularity_tag());
        out.extend_from_slice(&self.time.start_unix_micros().to_be_bytes());
        self.encode_dimensions(out);
    }

    /// Appends everything but the time partition: spec id, time index, value
    /// count, then each value as a type tag and payload.
    ///
    /// Durable rows that already store the time partition in their own
    /// columns store these bytes beside it.
    ///
    /// # Panics
    ///
    /// Never: [`Self::new`] bounds the value count and Arrow bounds a string
    /// below `u32::MAX` bytes.
    pub fn encode_dimensions(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.spec_id.to_be_bytes());
        out.push(self.time_index);
        out.push(u8::try_from(self.values.len()).expect("invariant: at most two dimensions"));
        for value in &self.values {
            match value {
                None => out.push(VALUE_NULL),
                Some(PartitionValueWire::Utf8(text)) => {
                    out.push(VALUE_UTF8);
                    out.extend_from_slice(
                        &u32::try_from(text.len())
                            .expect("invariant: an Arrow string fits u32")
                            .to_be_bytes(),
                    );
                    out.extend_from_slice(text.as_bytes());
                }
                Some(PartitionValueWire::Int32(value)) => {
                    out.push(VALUE_INT32);
                    out.extend_from_slice(&value.to_be_bytes());
                }
                Some(PartitionValueWire::Int64(value)) => {
                    out.push(VALUE_INT64);
                    out.extend_from_slice(&value.to_be_bytes());
                }
            }
        }
    }

    /// Returns [`Self::encode_dimensions`] as one owned buffer.
    #[must_use]
    pub fn dimension_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode_dimensions(&mut out);
        out
    }

    /// Returns the canonical binary encoding as one owned buffer.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode(&mut out);
        out
    }

    /// Restores an identity from exactly its canonical binary encoding.
    ///
    /// # Errors
    ///
    /// Returns [`PhysicalPartitionError`] for truncated or trailing bytes, an
    /// unknown tag, non-UTF-8 text, a noncanonical time start, or an invalid
    /// shape, so corrupt durable evidence fails closed.
    pub fn decode(bytes: &[u8]) -> Result<Self, PhysicalPartitionError> {
        let mut cursor = Cursor(bytes);
        let tag = cursor.take(1)?[0];
        let start = i64::from_be_bytes(fixed(cursor.take(8)?)?);
        Self::from_dimension_bytes(TimePartition::from_durable(tag, start)?, cursor.0)
    }

    /// Restores an identity from its time partition and exactly the bytes
    /// [`Self::encode_dimensions`] wrote.
    ///
    /// # Errors
    ///
    /// Returns [`PhysicalPartitionError`] for truncated or trailing bytes, an
    /// unknown tag, non-UTF-8 text, or an invalid shape.
    pub fn from_dimension_bytes(
        time: TimePartition,
        bytes: &[u8],
    ) -> Result<Self, PhysicalPartitionError> {
        let mut cursor = Cursor(bytes);
        let spec_id = i32::from_be_bytes(fixed(cursor.take(4)?)?);
        let time_index = cursor.take(1)?[0];
        let count = cursor.take(1)?[0];
        let mut values = Vec::with_capacity(usize::from(count).min(MAX_PARTITION_DIMENSIONS));
        for _ in 0..count {
            values.push(match cursor.take(1)?[0] {
                VALUE_NULL => None,
                VALUE_UTF8 => {
                    let length = u32::from_be_bytes(fixed(cursor.take(4)?)?);
                    let length =
                        usize::try_from(length).map_err(|_| PhysicalPartitionError::Truncated)?;
                    let text = std::str::from_utf8(cursor.take(length)?)
                        .map_err(|_| PhysicalPartitionError::Utf8)?;
                    Some(PartitionValueWire::Utf8(text.to_owned()))
                }
                VALUE_INT32 => Some(PartitionValueWire::Int32(i32::from_be_bytes(fixed(
                    cursor.take(4)?,
                )?))),
                VALUE_INT64 => Some(PartitionValueWire::Int64(i64::from_be_bytes(fixed(
                    cursor.take(8)?,
                )?))),
                other => return Err(PhysicalPartitionError::UnknownValueTag(other)),
            });
        }
        if !cursor.0.is_empty() {
            return Err(PhysicalPartitionError::Trailing);
        }
        Self::new(spec_id, time, time_index, values)
    }

    /// Projects the identity onto its private durable wire form.
    #[must_use]
    pub fn to_wire(&self) -> PhysicalPartitionWire {
        PhysicalPartitionWire {
            spec_id: self.spec_id,
            time: self.time.to_wire(),
            time_index: self.time_index,
            values: self.values.clone(),
        }
    }

    /// Restores an identity from its private durable wire form.
    ///
    /// # Errors
    ///
    /// Returns [`PhysicalPartitionError::Shape`] when the record carries more
    /// values than a layout allows or a time index beyond them.
    pub fn from_wire(wire: &PhysicalPartitionWire) -> Result<Self, PhysicalPartitionError> {
        Self::new(
            wire.spec_id,
            TimePartition::from_wire(wire.time),
            wire.time_index,
            wire.values.clone(),
        )
    }

    /// Builds the Iceberg partition tuple, in spec order, a data file under
    /// this identity records.
    #[must_use]
    pub fn iceberg_struct(&self) -> Struct {
        let mut fields = Vec::with_capacity(self.values.len() + 1);
        for value in &self.values {
            fields.push(value.as_ref().map(|value| {
                Literal::Primitive(match value {
                    PartitionValueWire::Utf8(text) => PrimitiveLiteral::String(text.clone()),
                    PartitionValueWire::Int32(value) => PrimitiveLiteral::Int(*value),
                    PartitionValueWire::Int64(value) => PrimitiveLiteral::Long(*value),
                })
            }));
        }
        fields.insert(
            usize::from(self.time_index),
            Some(self.time.iceberg_partition_literal()),
        );
        Struct::from_iter(fields)
    }

    /// Reads the identity one data file's partition tuple holds under the
    /// spec it was written with.
    ///
    /// Returns `None` when the spec has no time field or a tuple value is not
    /// a string, int, or long — a file Bifrost did not lay out.
    #[must_use]
    pub fn of_iceberg(spec: &PartitionSpec, partition: &Struct) -> Option<Self> {
        let time = TimePartition::of_iceberg_partition(spec, partition)?;
        let time_index = spec
            .fields()
            .iter()
            .position(|field| {
                PartitionTransform::from_iceberg(field.transform)
                    .and_then(PartitionTransform::granularity)
                    .is_some()
            })
            .and_then(|index| u8::try_from(index).ok())?;
        let values = partition
            .fields()
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != usize::from(time_index))
            .map(|(_, value)| match value {
                None => Some(None),
                Some(Literal::Primitive(PrimitiveLiteral::String(text))) => {
                    Some(Some(PartitionValueWire::Utf8(text.clone())))
                }
                Some(Literal::Primitive(PrimitiveLiteral::Int(value))) => {
                    Some(Some(PartitionValueWire::Int32(*value)))
                }
                Some(Literal::Primitive(PrimitiveLiteral::Long(value))) => {
                    Some(Some(PartitionValueWire::Int64(*value)))
                }
                Some(_) => None,
            })
            .collect::<Option<Vec<_>>>()?;
        Self::new(spec.spec_id(), time, time_index, values).ok()
    }
}

/// Reads a canonical encoding front to back, failing on truncation.
struct Cursor<'a>(&'a [u8]);

impl<'a> Cursor<'a> {
    /// Takes the next `length` bytes.
    ///
    /// # Errors
    ///
    /// Returns [`PhysicalPartitionError::Truncated`] when fewer remain.
    fn take(&mut self, length: usize) -> Result<&'a [u8], PhysicalPartitionError> {
        if self.0.len() < length {
            return Err(PhysicalPartitionError::Truncated);
        }
        let (head, rest) = self.0.split_at(length);
        self.0 = rest;
        Ok(head)
    }
}

impl std::fmt::Display for PhysicalPartition {
    /// Renders `<time>` for a time-only identity and
    /// `<time>/spec=<id>/<values>` otherwise, for logs and errors.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.time)?;
        if self.values.is_empty() && self.spec_id == BIFROST_PARTITION_SPEC_ID {
            return Ok(());
        }
        write!(f, "/spec={}", self.spec_id)?;
        for value in &self.values {
            match value {
                None => write!(f, "/null")?,
                Some(PartitionValueWire::Utf8(text)) => write!(f, "/utf8:{text:?}")?,
                Some(PartitionValueWire::Int32(value)) => write!(f, "/int32:{value}")?,
                Some(PartitionValueWire::Int64(value)) => write!(f, "/int64:{value}")?,
            }
        }
        Ok(())
    }
}

/// Reads a fixed-width big-endian field taken from a checked slice.
///
/// # Errors
///
/// Returns [`PhysicalPartitionError::Truncated`] when `bytes` is not exactly
/// `N` bytes long.
fn fixed<const N: usize>(bytes: &[u8]) -> Result<[u8; N], PhysicalPartitionError> {
    bytes
        .try_into()
        .map_err(|_| PhysicalPartitionError::Truncated)
}

/// The rule that maps a row of one table to its [`PhysicalPartition`].
///
/// Built from the committed revision admission writes under; every ingest
/// path plans through this one value, and every slice it plans is bound to
/// the scheme's [`LayoutRevision`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionScheme {
    /// Time column and its hour or day granularity.
    axis: TimeAxis,
    /// Iceberg partition-spec id identities are computed under.
    spec_id: i32,
    /// Schema and sort-order revision planned slices are bound to.
    revision: LayoutRevision,
    /// Position of the time field among the spec's fields.
    time_index: u8,
    /// Additional dimensions in spec order.
    dimensions: Vec<(String, PartitionTransform)>,
    /// Complete recipe planned slices are accepted under, when the scheme
    /// comes from a registered declaration.
    recipe: Option<crate::catalog::AcceptedRecipe>,
}

/// A bare time partition is the time-only identity under the initial spec.
impl From<TimePartition> for PhysicalPartition {
    /// Wraps `time` as the time-only identity under the initial spec.
    fn from(time: TimePartition) -> Self {
        Self::time_only(time)
    }
}

impl PartitionScheme {
    /// The scheme of a table partitioned on `axis` alone under its initial
    /// spec.
    #[must_use]
    pub const fn time_only(axis: TimeAxis) -> Self {
        Self {
            axis,
            spec_id: BIFROST_PARTITION_SPEC_ID,
            revision: LayoutRevision::INITIAL,
            time_index: 0,
            dimensions: Vec::new(),
            recipe: None,
        }
    }

    /// Binds the scheme's planned slices to `recipe`.
    #[must_use]
    pub fn with_recipe(mut self, recipe: crate::catalog::AcceptedRecipe) -> Self {
        self.recipe = Some(recipe);
        self
    }

    /// Borrows the recipe planned slices are accepted under.
    #[must_use]
    pub const fn recipe(&self) -> Option<&crate::catalog::AcceptedRecipe> {
        self.recipe.as_ref()
    }

    /// Binds the scheme's planned slices to `revision`.
    #[must_use]
    pub const fn with_revision(mut self, revision: LayoutRevision) -> Self {
        self.revision = revision;
        self
    }

    /// Returns the revision planned slices are bound to.
    #[must_use]
    pub const fn revision(&self) -> LayoutRevision {
        self.revision
    }

    /// Builds the scheme a registered layout declares under `spec_id`.
    ///
    /// An empty `partition_by` is the hourly default on the time column.
    ///
    /// # Errors
    ///
    /// Returns [`PhysicalPartitionError::Shape`] when a declared transform is
    /// malformed or the declaration exceeds the dimension bound; a layout the
    /// catalog resolved never is.
    pub fn of_wire(
        layout: &PhysicalLayoutWire,
        spec_id: i32,
    ) -> Result<Self, PhysicalPartitionError> {
        let axis = TimeAxis::of_wire(layout);
        let mut time_index = 0;
        let mut dimensions = Vec::new();
        for (index, field) in layout.partition_by.iter().enumerate() {
            let transform =
                PartitionTransform::from_wire(field).map_err(|_| PhysicalPartitionError::Shape)?;
            if transform.granularity().is_some() {
                time_index = u8::try_from(index).map_err(|_| PhysicalPartitionError::Shape)?;
            } else {
                dimensions.push((field.column.clone(), transform));
            }
        }
        if dimensions.len() > MAX_PARTITION_DIMENSIONS {
            return Err(PhysicalPartitionError::Shape);
        }
        Ok(Self {
            axis,
            spec_id,
            revision: LayoutRevision::INITIAL,
            time_index,
            dimensions,
            recipe: None,
        })
    }

    /// Returns the time axis.
    #[must_use]
    pub const fn axis(&self) -> &TimeAxis {
        &self.axis
    }

    /// Applies every dimension transform to `batch` once, ready for per-row
    /// identity lookups.
    ///
    /// # Errors
    ///
    /// Returns [`PhysicalPartitionError::Column`] when the time column or a
    /// dimension source is missing or of a type its transform does not
    /// produce a string, int, or long from.
    pub fn rows<'a>(
        &self,
        batch: &'a RecordBatch,
    ) -> Result<RowPartitions<'a>, PhysicalPartitionError> {
        let time = self
            .axis
            .values(batch)
            .ok_or_else(|| PhysicalPartitionError::Column(self.axis.column().to_owned()))?;
        let dimensions = self
            .dimensions
            .iter()
            .map(|(column, transform)| {
                let source = batch
                    .column_by_name(column)
                    .ok_or_else(|| PhysicalPartitionError::Column(column.clone()))?;
                let transformed =
                    iceberg::transform::create_transform_function(&transform.to_iceberg())
                        .and_then(|function| function.transform(std::sync::Arc::clone(source)))
                        .map_err(|_| PhysicalPartitionError::Column(column.clone()))?;
                DimensionValues::of(&transformed)
                    .ok_or_else(|| PhysicalPartitionError::Column(column.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(RowPartitions {
            granularity: self.axis.granularity(),
            spec_id: self.spec_id,
            time_index: self.time_index,
            time,
            dimensions,
        })
    }
}

/// One batch's transformed partition sources, ready for per-row lookups.
pub struct RowPartitions<'a> {
    /// Time granularity rows are bucketed by.
    granularity: TimeGranularity,
    /// Spec id every identity carries.
    spec_id: i32,
    /// Position of the time field among the spec's fields.
    time_index: u8,
    /// Borrowed time column.
    time: TimeColumnValues<'a>,
    /// Transformed dimension values in spec order.
    dimensions: Vec<DimensionValues>,
}

impl RowPartitions<'_> {
    /// Returns the batch's row count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.time.len()
    }

    /// Reports whether the batch has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.time.is_empty()
    }

    /// Returns the identity of row `index`, or `None` for a null instant or
    /// one whose time partition is not representable.
    ///
    /// # Panics
    ///
    /// Panics when `index` is not a row of the batch the columns were taken
    /// from.
    #[must_use]
    pub fn partition(&self, index: usize) -> Option<PhysicalPartition> {
        let micros = self.time.floor_micros(index)?;
        let time = self
            .granularity
            .bucket(DateTime::<Utc>::from_timestamp_micros(micros)?)
            .ok()?;
        Some(PhysicalPartition {
            time,
            spec_id: self.spec_id,
            time_index: self.time_index,
            values: self
                .dimensions
                .iter()
                .map(|values| values.value(index))
                .collect(),
        })
    }
}

/// One transformed dimension column, typed by its partition value type.
enum DimensionValues {
    /// String values.
    Utf8(StringArray),
    /// 32-bit integer values, including bucket ordinals.
    Int32(Int32Array),
    /// 64-bit integer values.
    Int64(Int64Array),
}

impl DimensionValues {
    /// Adopts a transformed column of a supported partition value type.
    fn of(array: &ArrayRef) -> Option<Self> {
        let any = array.as_any();
        if let Some(values) = any.downcast_ref::<StringArray>() {
            return Some(Self::Utf8(values.clone()));
        }
        if let Some(values) = any.downcast_ref::<Int32Array>() {
            return Some(Self::Int32(values.clone()));
        }
        any.downcast_ref::<Int64Array>()
            .map(|values| Self::Int64(values.clone()))
    }

    /// Returns row `index`'s typed value, `None` when it is null.
    ///
    /// # Panics
    ///
    /// Panics when `index` is not a row of the adopted column.
    fn value(&self, index: usize) -> Option<PartitionValueWire> {
        match self {
            Self::Utf8(values) => (!values.is_null(index))
                .then(|| PartitionValueWire::Utf8(values.value(index).to_owned())),
            Self::Int32(values) => {
                (!values.is_null(index)).then(|| PartitionValueWire::Int32(values.value(index)))
            }
            Self::Int64(values) => {
                (!values.is_null(index)).then(|| PartitionValueWire::Int64(values.value(index)))
            }
        }
    }
}

/// Failures restoring or computing a [`PhysicalPartition`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PhysicalPartitionError {
    /// The encoding ended before a field it declared.
    #[error("physical partition encoding is truncated")]
    Truncated,
    /// The encoding has bytes after its last field.
    #[error("physical partition encoding has trailing bytes")]
    Trailing,
    /// A dimension value carries an unknown type tag.
    #[error("unknown physical partition value tag: {0}")]
    UnknownValueTag(u8),
    /// A string dimension value is not UTF-8.
    #[error("physical partition string value is not UTF-8")]
    Utf8,
    /// The value count or time position is outside the layout bound.
    #[error("physical partition shape exceeds the layout bound")]
    Shape,
    /// The time partition is invalid.
    #[error(transparent)]
    Time(#[from] TimePartitionError),
    /// A partition source column is missing or of an unsupported type.
    #[error("partition source column {0} is missing or unsupported")]
    Column(String),
}
