//! The single authoritative physical layout of a Bifrost table.
//!
//! One resolved [`PhysicalLayout`] governs every observable and durable
//! representation of a table: the describe response, the `physical_layout`
//! JSONB control column, the Iceberg partition spec and sort order, the Parquet
//! writer's Bloom recipe, the Scribe seal key, and every Forge identity. The
//! catalog is its owner — nothing else resolves or defaults a layout.
//!
//! Callers declare intent with the Arrow-free
//! [`PhysicalLayoutWire`]; [`PhysicalLayout::resolve`] is the one entry point
//! that turns that intent plus the complete physical schema into the canonical
//! form, or fails with a public [`BifrostError`] before any durable mutation.
//! Built-in and caller declarations run the identical code path, so identical
//! declarations produce byte-identical canonical layouts.

use std::collections::BTreeSet;
use std::sync::Arc;

use arrow::array::{Array, TimestampMicrosecondArray, TimestampNanosecondArray};
use arrow::datatypes::{DataType, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Duration, DurationRound as _, Utc};
use iceberg::spec::{
    NullOrder as IcebergNullOrder, Schema as IcebergSchema, SortDirection as IcebergSortDirection,
    SortField, SortOrder, Transform, UnboundPartitionSpec,
};
use wyrd_spec::vala::api::{
    NullOrderWire, PartitionFieldWire, PartitionTransformWire, PhysicalLayoutWire,
    SortDirectionWire, SortKeyWire, TimeGranularityWire, TimePartitionWire,
};
use wyrd_spec::vala::managed_columns::WYRD_INGESTED_AT;
use wyrd_spec::vala::{BifrostError, PhysicalLayoutField, PhysicalLayoutViolation};

/// Largest number of partition fields one declaration may carry: the one
/// required time field plus at most two additional dimensions.
///
/// Every additional dimension multiplies the partitions one hour of data can
/// occupy, and files never cross a partition boundary, so the contract caps
/// the declaration instead of letting fanout grow with the field count.
pub const MAX_PARTITION_FIELDS: usize = 3;

/// Smallest and largest bucket counts a `bucket` partition field may declare.
pub const BUCKET_COUNT_RANGE: std::ops::RangeInclusive<u32> = 2..=1024;

/// Largest number of Bloom columns one declaration may carry.
///
/// Each enabled column initializes its own filter per row group, so the cap
/// bounds writer memory rather than letting a declaration grow it without
/// limit.
pub const MAX_BLOOM_COLUMNS: usize = 16;

/// Largest number of sort keys one declaration may carry.
///
/// Each additional key costs write-side sort time and buys progressively less
/// clustering, so the contract caps the declaration rather than accepting an
/// unbounded list and silently truncating it.
pub const MAX_SORT_KEYS: usize = 4;

/// Partition spec id every Bifrost physical table carries.
///
/// Table creation installs exactly one partition spec and never evolves it, so
/// the bound spec is always Iceberg's initial spec. Publication records the id
/// on every promoted file, and this constant is what makes that recorded id a
/// stated contract instead of a value copied out of a live table.
pub const BIFROST_PARTITION_SPEC_ID: i32 = 0;

/// Sort order id every Bifrost physical table carries.
///
/// [`PhysicalLayout::iceberg_sort_order`] builds the single order under this
/// id at table creation, and writers preserve it, so a promoted file that
/// names this id is asserting the order it was actually written in.
pub const BIFROST_SORT_ORDER_ID: i32 = 1;

/// Time-partition granularity owned by the engine.
///
/// Bifrost v1 admits exactly these two Iceberg-native transforms on the
/// table's one time column. Identity, bucket, truncate, month, year, and multi-column
/// partitioning are deliberately unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TimeGranularity {
    /// One partition per UTC hour.
    Hour,
    /// One partition per UTC day.
    Day,
}

impl TimeGranularity {
    /// Returns the durable one-byte tag used by WAL slices, digests, and
    /// protobuf. Tag `0` means "unspecified" and is never produced.
    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            Self::Hour => 1,
            Self::Day => 2,
        }
    }

    /// Restores a granularity from its durable tag.
    ///
    /// Returns `None` for `0` and for every unknown tag, so a decoder fails
    /// closed rather than defaulting.
    #[must_use]
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::Hour),
            2 => Some(Self::Day),
            _ => None,
        }
    }

    /// Returns the lower-case durable token (`hour` or `day`) used in object
    /// paths, control JSON, and the `vala.file_list` check constraint.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hour => "hour",
            Self::Day => "day",
        }
    }

    /// Parses the durable lower-case token.
    ///
    /// Returns `None` for every other spelling, including capitalized or
    /// pluralized forms.
    #[must_use]
    pub fn from_str_token(token: &str) -> Option<Self> {
        match token {
            "hour" => Some(Self::Hour),
            "day" => Some(Self::Day),
            _ => None,
        }
    }

    /// Buckets one instant into its exact partition at this granularity.
    ///
    /// # Errors
    /// Returns [`TimePartitionError::StartOutOfRange`] when truncation to the
    /// partition boundary is not representable, or when the partition's end
    /// boundary or Iceberg transform value is not, so every partition this
    /// returns is total over [`TimePartition::end_utc`] and
    /// [`TimePartition::iceberg_transform_value`].
    pub fn bucket(self, instant: DateTime<Utc>) -> Result<TimePartition, TimePartitionError> {
        let out_of_range = || TimePartitionError::StartOutOfRange(instant.timestamp_micros());
        let start = instant
            .duration_trunc(self.span())
            .map_err(|_| out_of_range())?;
        let partition = TimePartition::new(self, start)?;
        if start.checked_add_signed(self.span()).is_none()
            || i32::try_from(start.timestamp().div_euclid(self.span().num_seconds())).is_err()
        {
            return Err(out_of_range());
        }
        Ok(partition)
    }

    /// Returns the span of one partition at this granularity.
    #[must_use]
    fn span(self) -> Duration {
        match self {
            Self::Hour => Duration::hours(1),
            Self::Day => Duration::days(1),
        }
    }

    /// Converts to the public wire enum.
    #[must_use]
    pub const fn to_wire(self) -> TimeGranularityWire {
        match self {
            Self::Hour => TimeGranularityWire::Hour,
            Self::Day => TimeGranularityWire::Day,
        }
    }

    /// Converts from the public wire enum.
    #[must_use]
    pub const fn from_wire(wire: TimeGranularityWire) -> Self {
        match wire {
            TimeGranularityWire::Hour => Self::Hour,
            TimeGranularityWire::Day => Self::Day,
        }
    }
}

/// A table's one time axis: its time column and partition granularity.
///
/// Scribe receives this from the registered layout and uses it to bucket every
/// admitted row into its partition and to bound every file it writes. Carrying
/// the column with the granularity keeps both on one value, so no write path
/// can partition one column and bound another.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TimeAxis {
    /// Name of the time column in the table's physical schema.
    column: Arc<str>,
    /// Granularity of the one time partition.
    granularity: TimeGranularity,
}

impl TimeAxis {
    /// Builds one time axis from a column name and granularity.
    #[must_use]
    pub fn new(column: &str, granularity: TimeGranularity) -> Self {
        Self {
            column: Arc::from(column),
            granularity,
        }
    }

    /// Derives the time axis a stored or built-in layout declaration names.
    ///
    /// A declaration without `time_column` partitions on `wyrd_ingested_at`,
    /// and one without a time field partitions hourly — the same defaults
    /// [`PhysicalLayout::resolve`] applies — so ingest and publication agree on
    /// the axis without re-resolving the whole layout.
    #[must_use]
    pub fn of_wire(layout: &PhysicalLayoutWire) -> Self {
        let granularity = layout
            .partition_by
            .iter()
            .find_map(|field| PartitionTransform::from_wire(field).ok()?.granularity())
            .unwrap_or(TimeGranularity::Hour);
        Self::new(
            layout.time_column.as_deref().unwrap_or(WYRD_INGESTED_AT),
            granularity,
        )
    }

    /// Derives the time axis a published Iceberg table's default partition
    /// spec partitions on.
    ///
    /// Returns `None` when the default spec has no `hour` or `day` field or its
    /// source column is absent from the current schema, which a registered
    /// table never has.
    #[must_use]
    pub fn of_iceberg(metadata: &iceberg::spec::TableMetadata) -> Option<Self> {
        Self::of_iceberg_spec(metadata.default_partition_spec(), metadata.current_schema())
    }

    /// Derives the time axis one retained Iceberg partition spec partitions on,
    /// resolving its source column through `schema` by stable field id.
    ///
    /// Returns `None` when the spec has no `hour` or `day` field or `schema`
    /// has no field with its source id.
    #[must_use]
    pub fn of_iceberg_spec(
        spec: &iceberg::spec::PartitionSpec,
        schema: &IcebergSchema,
    ) -> Option<Self> {
        let (_, field, granularity) = time_partition_field(spec)?;
        Some(Self::new(
            &schema.field_by_id(field.source_id)?.name,
            granularity,
        ))
    }

    /// Returns the field id of the time column a partition spec partitions on.
    ///
    /// Every Bifrost spec has exactly one `hour` or `day` field, the time
    /// transform, so its source is the time column. Reading the id from the
    /// spec a file was written under, rather than resolving a name against the
    /// current schema, keeps a file's bounds attached to the column it was
    /// partitioned on. Returns `None` for a spec with no time field.
    #[must_use]
    pub fn iceberg_field_id(spec: &iceberg::spec::PartitionSpec) -> Option<i32> {
        time_partition_field(spec).map(|(_, field, _)| field.source_id)
    }

    /// Borrows the time column name.
    #[must_use]
    pub fn column(&self) -> &str {
        &self.column
    }

    /// Returns the partition granularity.
    #[must_use]
    pub const fn granularity(&self) -> TimeGranularity {
        self.granularity
    }

    /// Borrows the time column of `batch` as typed instants.
    ///
    /// Returns `None` when the batch has no such column or it is not a
    /// microsecond or nanosecond timestamp, which a registered layout's
    /// validated time column never is.
    #[must_use]
    pub fn values<'a>(&self, batch: &'a RecordBatch) -> Option<TimeColumnValues<'a>> {
        TimeColumnValues::of(batch.column_by_name(&self.column)?.as_ref())
    }
}

/// Stored precision of one time column.
///
/// Every catalog bound, partition bucket, and pruning interval is held in epoch
/// microseconds. This is the one place a nanosecond instant is narrowed to that
/// domain or a microsecond bound is widened back to it, so every reader derives
/// the same conservative interval: a lower bound floors and an upper bound
/// ceils, and the interval always contains every stored instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimePrecision {
    /// Epoch microseconds.
    Micros,
    /// Epoch nanoseconds.
    Nanos,
}

impl TimePrecision {
    /// Returns the precision of an Arrow time-column type, or `None` for any
    /// type a time column may not have.
    #[must_use]
    pub const fn of(data_type: &DataType) -> Option<Self> {
        match data_type {
            DataType::Timestamp(TimeUnit::Microsecond, _) => Some(Self::Micros),
            DataType::Timestamp(TimeUnit::Nanosecond, _) => Some(Self::Nanos),
            _ => None,
        }
    }

    /// Narrows one stored instant to the greatest microsecond not after it.
    #[must_use]
    pub const fn floor_micros(self, value: i64) -> i64 {
        match self {
            Self::Micros => value,
            Self::Nanos => value.div_euclid(1_000),
        }
    }

    /// Narrows one stored instant to the least microsecond not before it.
    #[must_use]
    pub const fn ceil_micros(self, value: i64) -> i64 {
        match self {
            Self::Micros => value,
            Self::Nanos => {
                let floor = value.div_euclid(1_000);
                if value.rem_euclid(1_000) == 0 {
                    floor
                } else {
                    floor + 1
                }
            }
        }
    }

    /// Widens a microsecond lower bound to the first stored instant it covers.
    ///
    /// Returns `None` when that instant is not representable at this precision.
    #[must_use]
    pub const fn lower_from_micros(self, micros: i64) -> Option<i64> {
        match self {
            Self::Micros => Some(micros),
            Self::Nanos => micros.checked_mul(1_000),
        }
    }

    /// Widens a microsecond upper bound to the last stored instant it covers.
    ///
    /// Returns `None` when that instant is not representable at this precision.
    #[must_use]
    pub const fn upper_from_micros(self, micros: i64) -> Option<i64> {
        match self {
            Self::Micros => Some(micros),
            Self::Nanos => match micros.checked_mul(1_000) {
                Some(nanos) => nanos.checked_add(999),
                None => None,
            },
        }
    }
}

/// The values of one time column at its stored precision.
///
/// Partition bucketing and catalog bounds work in microseconds; a nanosecond
/// column is floored for a lower bound and ceiled for an upper bound, so every
/// derived interval contains every stored instant.
#[derive(Debug, Clone, Copy)]
pub enum TimeColumnValues<'a> {
    /// A microsecond timestamp column.
    Micros(&'a TimestampMicrosecondArray),
    /// A nanosecond timestamp column.
    Nanos(&'a TimestampNanosecondArray),
}

impl<'a> TimeColumnValues<'a> {
    /// Returns the column's stored precision.
    #[must_use]
    pub const fn precision(self) -> TimePrecision {
        match self {
            Self::Micros(_) => TimePrecision::Micros,
            Self::Nanos(_) => TimePrecision::Nanos,
        }
    }

    /// Borrows `array` as time-column values, or `None` for any other type.
    #[must_use]
    pub fn of(array: &'a dyn Array) -> Option<Self> {
        let any = array.as_any();
        any.downcast_ref::<TimestampMicrosecondArray>()
            .map(Self::Micros)
            .or_else(|| any.downcast_ref().map(Self::Nanos))
    }

    /// Returns the number of values, nulls included.
    #[must_use]
    pub fn len(self) -> usize {
        match self {
            Self::Micros(array) => array.len(),
            Self::Nanos(array) => array.len(),
        }
    }

    /// Reports whether the column holds no values.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// Counts the null values.
    #[must_use]
    pub fn null_count(self) -> usize {
        match self {
            Self::Micros(array) => array.null_count(),
            Self::Nanos(array) => array.null_count(),
        }
    }

    /// Returns value `index` floored to epoch microseconds, or `None` when it
    /// is null.
    #[must_use]
    pub fn floor_micros(self, index: usize) -> Option<i64> {
        let value = match self {
            Self::Micros(array) => array.is_valid(index).then(|| array.value(index)),
            Self::Nanos(array) => array.is_valid(index).then(|| array.value(index)),
        }?;
        Some(self.precision().floor_micros(value))
    }

    /// Returns the inclusive microsecond interval covering every non-null
    /// value, or `None` when there is none.
    ///
    /// A nanosecond maximum is ceiled so the interval still contains it.
    #[must_use]
    pub fn micros_bounds(self) -> Option<(i64, i64)> {
        let (min, max) = match self {
            Self::Micros(array) => (arrow::compute::min(array)?, arrow::compute::max(array)?),
            Self::Nanos(array) => (arrow::compute::min(array)?, arrow::compute::max(array)?),
        };
        let precision = self.precision();
        Some((precision.floor_micros(min), precision.ceil_micros(max)))
    }
}

/// Sort direction of one canonical sort key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortDirection {
    /// Ascending.
    Asc,
    /// Descending.
    Desc,
}

impl SortDirection {
    /// Maps to the Iceberg sort direction written into table metadata.
    #[must_use]
    pub const fn to_iceberg(self) -> IcebergSortDirection {
        match self {
            Self::Asc => IcebergSortDirection::Ascending,
            Self::Desc => IcebergSortDirection::Descending,
        }
    }
}

/// Null placement of one canonical sort key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NullOrder {
    /// Nulls sort before non-null values.
    First,
    /// Nulls sort after non-null values.
    Last,
}

impl NullOrder {
    /// Maps to the Iceberg null order written into table metadata.
    #[must_use]
    pub const fn to_iceberg(self) -> IcebergNullOrder {
        match self {
            Self::First => IcebergNullOrder::First,
            Self::Last => IcebergNullOrder::Last,
        }
    }
}

/// Iceberg table property carrying the canonical Bloom column union.
///
/// The union is not derivable from the Iceberg schema, partition spec, or sort
/// order, so it is persisted on the table itself and every producer reads it
/// back rather than reconstructing a recipe.
pub const BLOOM_COLUMNS_PROPERTY: &str = "wyrd.bifrost.bloom-columns";

/// Iceberg table property naming the base location every Iceberg writer uses.
///
/// Bifrost sets it at registration so the only Iceberg writer in the system —
/// a Forge managed rewrite — lands its outputs under the Forge recipe segment
/// rather than at the table's default `{location}/data` root. Scribe is
/// unaffected: its hot objects are written directly and promoted at their own
/// paths, never through an Iceberg location generator.
pub const WRITE_DATA_PATH_PROPERTY: &str = "write.data.path";

/// Current Forge writer-recipe segment stamped into every rewrite output path.
///
/// The segment is what makes a rewrite output self-describing: a later
/// selection pass resolves it back out of the object path and compares it with
/// the policy's expected recipe, so an output produced under a superseded
/// recipe is reselected while an output produced under the current one is not.
/// Advancing the recipe is therefore a deliberate act — bump this constant and
/// every previously written object becomes obsolete by construction.
pub const FORGE_WRITER_RECIPE: &str = "v2";

/// Path segment separating recipe-tagged rewrite outputs from the data root.
///
/// The managed core's recipe resolver looks for exactly this marker and takes
/// the segment that follows it as the recipe, so the marker and the property
/// value below must stay in agreement.
pub const FORGE_DATA_MARKER: &str = "/data/forge/";

/// Builds the Iceberg data location Forge rewrite outputs are written beneath.
///
/// Returned as `{table_location}/data/forge/{recipe}` with any trailing slash
/// on the table location removed, which is the exact string
/// [`WRITE_DATA_PATH_PROPERTY`] carries and the exact prefix a produced object
/// path must start with.
#[must_use]
pub fn forge_data_location(table_location: &str) -> String {
    format!(
        "{}{FORGE_DATA_MARKER}{FORGE_WRITER_RECIPE}",
        table_location.trim_end_matches('/')
    )
}

/// One canonical physical sort key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LayoutSortKey {
    /// Sorted column; always present in the table's physical schema.
    pub column: String,
    /// Sort direction.
    pub direction: SortDirection,
    /// Null placement.
    pub null_order: NullOrder,
}

impl LayoutSortKey {
    /// Builds one sort key from its parts.
    #[must_use]
    pub fn new(column: impl Into<String>, direction: SortDirection, null_order: NullOrder) -> Self {
        Self {
            column: column.into(),
            direction,
            null_order,
        }
    }

    /// Returns the sorted column name.
    #[must_use]
    pub fn column(&self) -> &str {
        &self.column
    }

    /// Reports whether Arrow must sort this key in descending order.
    #[must_use]
    pub const fn is_descending(&self) -> bool {
        matches!(self.direction, SortDirection::Desc)
    }

    /// Reports whether Arrow must place nulls before non-null values.
    #[must_use]
    pub const fn nulls_first(&self) -> bool {
        matches!(self.null_order, NullOrder::First)
    }

    /// Returns the sole key a layout resolves to when no sort key is declared:
    /// the table's time column, descending with nulls last.
    ///
    /// Ordering on the time column clusters each file's rows by time, which
    /// tightens the row-group bounds Oracle prunes on, and keeps
    /// `DELTA_BINARY_PACKED` near-free on the sorted timestamp.
    #[must_use]
    pub fn default_for_time_column(time_column: &str) -> Self {
        Self::new(time_column, SortDirection::Desc, NullOrder::Last)
    }
}

/// Returns the position, field, and granularity of the `hour` or `day` field
/// of one Iceberg partition spec — the one place a spec's time field is found.
fn time_partition_field(
    spec: &iceberg::spec::PartitionSpec,
) -> Option<(usize, &iceberg::spec::PartitionField, TimeGranularity)> {
    spec.fields().iter().enumerate().find_map(|(index, field)| {
        PartitionTransform::from_iceberg(field.transform)?
            .granularity()
            .map(|granularity| (index, field, granularity))
    })
}

/// Transform one canonical partition field applies to its source column.
///
/// `Hour` and `Day` partition the table's required time column; `Identity`,
/// `Bucket`, and `Truncate` are the additional dimensions, applied with
/// Iceberg's own transform semantics so a null source stays a null partition
/// value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PartitionTransform {
    /// One partition per UTC hour of the time column.
    Hour,
    /// One partition per UTC day of the time column.
    Day,
    /// One partition per distinct source value.
    Identity,
    /// Iceberg Murmur3 bucket over this many buckets.
    Bucket(u32),
    /// Iceberg truncation at this width.
    Truncate(u32),
}

impl PartitionTransform {
    /// Adopts one declared wire field's transform and parameters.
    ///
    /// # Errors
    ///
    /// Returns the violation of [`PhysicalLayoutField::PartitionTransform`]
    /// when a required parameter is absent (`Missing`), a parameter is out of
    /// range (`OutOfRange`), or a parameter the transform does not take is
    /// supplied (`UnsupportedValue`).
    pub fn from_wire(field: &PartitionFieldWire) -> Result<Self, PhysicalLayoutViolation> {
        let parameterless = |transform| {
            if field.buckets.is_some() || field.width.is_some() {
                return Err(PhysicalLayoutViolation::UnsupportedValue);
            }
            Ok(transform)
        };
        match field.transform {
            PartitionTransformWire::Hour => parameterless(Self::Hour),
            PartitionTransformWire::Day => parameterless(Self::Day),
            PartitionTransformWire::Identity => parameterless(Self::Identity),
            PartitionTransformWire::Bucket => {
                if field.width.is_some() {
                    return Err(PhysicalLayoutViolation::UnsupportedValue);
                }
                let buckets = field.buckets.ok_or(PhysicalLayoutViolation::Missing)?;
                if !BUCKET_COUNT_RANGE.contains(&buckets) {
                    return Err(PhysicalLayoutViolation::OutOfRange);
                }
                Ok(Self::Bucket(buckets))
            }
            PartitionTransformWire::Truncate => {
                if field.buckets.is_some() {
                    return Err(PhysicalLayoutViolation::UnsupportedValue);
                }
                let width = field.width.ok_or(PhysicalLayoutViolation::Missing)?;
                if width == 0 || i32::try_from(width).is_err() {
                    return Err(PhysicalLayoutViolation::OutOfRange);
                }
                Ok(Self::Truncate(width))
            }
        }
    }

    /// Projects onto one wire field over `column`.
    #[must_use]
    pub fn to_wire(self, column: &str) -> PartitionFieldWire {
        let (transform, buckets, width) = match self {
            Self::Hour => (PartitionTransformWire::Hour, None, None),
            Self::Day => (PartitionTransformWire::Day, None, None),
            Self::Identity => (PartitionTransformWire::Identity, None, None),
            Self::Bucket(count) => (PartitionTransformWire::Bucket, Some(count), None),
            Self::Truncate(width) => (PartitionTransformWire::Truncate, None, Some(width)),
        };
        PartitionFieldWire {
            column: column.to_owned(),
            transform,
            buckets,
            width,
        }
    }

    /// Returns the time granularity of an `Hour` or `Day` transform, and
    /// `None` for an additional dimension.
    #[must_use]
    pub const fn granularity(self) -> Option<TimeGranularity> {
        match self {
            Self::Hour => Some(TimeGranularity::Hour),
            Self::Day => Some(TimeGranularity::Day),
            Self::Identity | Self::Bucket(_) | Self::Truncate(_) => None,
        }
    }

    /// Maps to the Iceberg transform written into the partition spec.
    #[must_use]
    pub const fn to_iceberg(self) -> Transform {
        match self {
            Self::Hour => Transform::Hour,
            Self::Day => Transform::Day,
            Self::Identity => Transform::Identity,
            Self::Bucket(count) => Transform::Bucket(count),
            Self::Truncate(width) => Transform::Truncate(width),
        }
    }

    /// Adopts one Iceberg transform this contract can represent, or `None`.
    #[must_use]
    pub const fn from_iceberg(transform: Transform) -> Option<Self> {
        match transform {
            Transform::Hour => Some(Self::Hour),
            Transform::Day => Some(Self::Day),
            Transform::Identity => Some(Self::Identity),
            Transform::Bucket(count) => Some(Self::Bucket(count)),
            Transform::Truncate(width) => Some(Self::Truncate(width)),
            _ => None,
        }
    }

    /// Returns the Iceberg partition-field name this transform gives
    /// `column`, following Iceberg's own naming: the column for `identity`,
    /// and `{column}_{hour|day|bucket|trunc}` otherwise.
    #[must_use]
    pub fn iceberg_field_name(self, column: &str) -> String {
        match self {
            Self::Identity => column.to_owned(),
            Self::Hour => format!("{column}_hour"),
            Self::Day => format!("{column}_day"),
            Self::Bucket(_) => format!("{column}_bucket"),
            Self::Truncate(_) => format!("{column}_trunc"),
        }
    }
}

/// One canonical partition field: a source column and its transform.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LayoutPartitionField {
    /// Top-level source column; always present in the physical schema.
    column: String,
    /// Transform applied to the column.
    transform: PartitionTransform,
}

impl LayoutPartitionField {
    /// Borrows the source column name.
    #[must_use]
    pub fn column(&self) -> &str {
        &self.column
    }

    /// Returns the transform.
    #[must_use]
    pub const fn transform(&self) -> PartitionTransform {
        self.transform
    }
}

/// The canonical, fully resolved physical layout of one table.
///
/// Construct it only through [`PhysicalLayout::resolve`] or
/// [`PhysicalLayout::from_stored_wire`]; both guarantee a valid UTC time
/// column, a bounded partition list holding exactly one time field, a
/// non-empty bounded sort order, and a bounded, typed Bloom set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalLayout {
    /// Granularity of the one time partition field.
    granularity: TimeGranularity,
    /// The table's one time column: the source of its time partition field,
    /// its default first sort key, and every tier's time bounds. A required
    /// UTC microsecond or nanosecond timestamp column of the physical schema.
    time_column: String,
    /// Partition fields in declared order, holding exactly one time field.
    partition_by: Vec<LayoutPartitionField>,
    /// Canonical sort order, carrying at least one key.
    sort_keys: Vec<LayoutSortKey>,
    /// The complete resolved Bloom set; empty disables Bloom filters.
    bloom_columns: Vec<String>,
}

impl PhysicalLayout {
    /// Resolves one declaration into the one canonical layout.
    ///
    /// This is the single resolution entry point. A built-in definition and an
    /// untrusted register request run this identical code path, so identical
    /// declarations produce byte-identical canonical layouts and the engine can
    /// never refuse to provision a table a caller could have registered.
    ///
    /// `table` is the already-validated canonical `<namespace>.<name>` used only
    /// as public error context. `schema` is the complete physical schema,
    /// including managed and correlation columns.
    ///
    /// An omitted declaration resolves exactly as an empty one: hourly
    /// partitioning on `wyrd_ingested_at`, that column descending nulls-last as
    /// the sole sort key, and no Bloom column. Within a declaration an omitted
    /// `time_column` resolves to `wyrd_ingested_at`, an empty `partition_by` to
    /// `hour(time_column)`, and an empty `sort_keys` to the time column
    /// descending nulls-last. The Bloom set is exactly the declared one; the
    /// system adds no column of its own.
    ///
    /// Validation precedence: the time column, then partition fields, then
    /// sort keys, then Bloom columns, each list checked for its count before
    /// its entries in request order. The first fault wins and nothing is
    /// mutated.
    ///
    /// # Errors
    /// Returns [`BifrostError::InvalidPhysicalLayout`] naming the exact field,
    /// violation, and offending column.
    pub fn resolve(
        table: &str,
        schema: &Schema,
        declared: Option<&PhysicalLayoutWire>,
    ) -> Result<Self, BifrostError> {
        let omitted = PhysicalLayoutWire {
            time_column: None,
            partition_by: Vec::new(),
            sort_keys: Vec::new(),
            bloom_columns: Vec::new(),
        };
        let declared = declared.unwrap_or(&omitted);
        let time_column = canonical_time_column(table, schema, declared.time_column.as_deref())?;
        let (granularity, partition_by) =
            canonical_partition_by(table, schema, &time_column, &declared.partition_by)?;
        let sort_keys = canonical_sort_keys(table, schema, &time_column, &declared.sort_keys)?;
        let bloom_columns = canonical_bloom_columns(table, schema, &declared.bloom_columns)?;

        Ok(Self {
            granularity,
            time_column,
            partition_by,
            sort_keys,
            bloom_columns,
        })
    }

    /// Re-resolves a stored declaration against the physical schema of one
    /// artifact about to be written.
    ///
    /// This is the write-side counterpart of [`Self::from_stored_wire`]. The
    /// catalog stores a layout canonicalized over the table's full registered
    /// schema, which always carries the complete managed column set. A sealed
    /// generation's physical schema may legitimately be narrower — `run_id` is
    /// only stamped for native payloads — so byte equality against the stored
    /// form is the wrong check here and would refuse every non-native seal.
    ///
    /// Resolving against the artifact's own schema is the check that actually
    /// matters: the returned recipe names only sort and Bloom columns present
    /// in the schema being written, so the footer can never advertise a column
    /// the file does not contain. Genuine drift — an unsupported partition
    /// column, a declared user column that no longer exists, a duplicate — is
    /// still refused.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::InvalidPhysicalLayout`] when the stored
    /// declaration does not resolve against `schema`.
    pub fn resolve_for_physical_schema(
        table: &str,
        schema: &Schema,
        stored: &PhysicalLayoutWire,
    ) -> Result<Self, BifrostError> {
        Self::resolve(table, schema, Some(stored))
    }

    /// Rebuilds a layout from its persisted control JSON and re-validates it
    /// against the stored schema.
    ///
    /// Persisted JSON is always the fully populated canonical form written by
    /// the catalog itself, so it is resolved through [`Self::resolve`] and then
    /// required to canonicalize to itself. A row that does not is the
    /// physical-drift signal for a hand-edited or corrupted control row.
    ///
    /// # Errors
    /// Returns [`BifrostError::InvalidPhysicalLayout`] when the stored JSON is
    /// not a valid declaration, and [`BifrostError::PhysicalDrift`] when it is
    /// valid but not canonical.
    pub fn from_stored_wire(
        table: &str,
        schema: &Schema,
        stored: &PhysicalLayoutWire,
    ) -> Result<Self, BifrostError> {
        let resolved = Self::resolve(table, schema, Some(stored))?;
        if &resolved.to_wire() != stored {
            return Err(BifrostError::PhysicalDrift {
                detail: format!("stored physical layout for {table} is not canonical"),
            });
        }
        Ok(resolved)
    }

    /// Returns the resolved partition granularity.
    #[must_use]
    pub const fn granularity(&self) -> TimeGranularity {
        self.granularity
    }

    /// Borrows the table's one time column: the partition source, the default
    /// first sort key, and the column every tier's time bounds describe.
    #[must_use]
    pub fn time_column(&self) -> &str {
        &self.time_column
    }

    /// Returns the table's time axis for Scribe's partitioning and bounds.
    #[must_use]
    pub fn time_axis(&self) -> TimeAxis {
        TimeAxis::new(&self.time_column, self.granularity)
    }

    /// Borrows the partition fields in declared order, time field included.
    #[must_use]
    pub fn partition_by(&self) -> &[LayoutPartitionField] {
        &self.partition_by
    }

    /// Iterates the additional partition dimensions in declared order, every
    /// field except the one time field.
    pub fn dimensions(&self) -> impl Iterator<Item = &LayoutPartitionField> {
        self.partition_by
            .iter()
            .filter(|field| field.transform.granularity().is_none())
    }

    /// Borrows the canonical sort order.
    #[must_use]
    pub fn sort_keys(&self) -> &[LayoutSortKey] {
        &self.sort_keys
    }

    /// Renders the canonical Bloom union as the value of
    /// [`BLOOM_COLUMNS_PROPERTY`].
    ///
    /// The value is a JSON string array, so a reader recovers the exact ordered
    /// union without guessing a separator that a column name could contain.
    ///
    /// # Panics
    /// Never panics; a `Vec<String>` always serializes.
    #[must_use]
    pub fn bloom_columns_property(&self) -> String {
        serde_json::to_string(&self.bloom_columns)
            .expect("a string array always serializes to JSON")
    }

    /// Recovers the canonical Bloom union from an Iceberg table property.
    ///
    /// # Errors
    /// Returns a diagnostic when the property is absent or is not the JSON
    /// string array written by [`Self::bloom_columns_property`]. Callers must
    /// fail closed rather than defaulting, because writing a file with a
    /// guessed footer recipe would misrepresent it as `bifrost-writer-v2`.
    pub fn bloom_columns_from_property(
        value: Option<&String>,
    ) -> Result<std::sync::Arc<[String]>, String> {
        let value = value.ok_or_else(|| {
            format!("table metadata is missing the `{BLOOM_COLUMNS_PROPERTY}` property")
        })?;
        let columns: Vec<String> = serde_json::from_str(value)
            .map_err(|error| format!("`{BLOOM_COLUMNS_PROPERTY}` is not a JSON array: {error}"))?;
        Ok(std::sync::Arc::from(columns))
    }

    /// Borrows the canonical Bloom column list.
    #[must_use]
    pub fn bloom_columns(&self) -> &[String] {
        &self.bloom_columns
    }

    /// Buckets one time-column value into its exact partition.
    ///
    /// # Errors
    /// Returns [`TimePartitionError::StartOutOfRange`] when the instant's
    /// partition boundaries or Iceberg transform value are not representable.
    pub fn partition_of(
        &self,
        instant: DateTime<Utc>,
    ) -> Result<TimePartition, TimePartitionError> {
        self.granularity().bucket(instant)
    }

    /// Projects the canonical layout onto the public wire shape.
    ///
    /// The result is fully populated: it is what describe returns and what the
    /// control row stores.
    #[must_use]
    pub fn to_wire(&self) -> PhysicalLayoutWire {
        PhysicalLayoutWire {
            time_column: Some(self.time_column.clone()),
            partition_by: self
                .partition_by
                .iter()
                .map(|field| field.transform.to_wire(&field.column))
                .collect(),
            sort_keys: self
                .sort_keys
                .iter()
                .map(|key| SortKeyWire {
                    column: key.column.clone(),
                    direction: match key.direction {
                        SortDirection::Asc => SortDirectionWire::Asc,
                        SortDirection::Desc => SortDirectionWire::Desc,
                    },
                    null_order: match key.null_order {
                        NullOrder::First => NullOrderWire::First,
                        NullOrder::Last => NullOrderWire::Last,
                    },
                })
                .collect(),
            bloom_columns: self.bloom_columns.clone(),
        }
    }

    /// Builds the Iceberg partition spec for this layout: one field per
    /// declared partition field, in declared order, bound by source field id.
    ///
    /// # Errors
    /// Returns a message when a source column is absent from the Iceberg
    /// schema or when Iceberg rejects a partition field.
    pub fn iceberg_partition_spec(
        &self,
        iceberg_schema: &IcebergSchema,
    ) -> Result<UnboundPartitionSpec, String> {
        let mut builder = UnboundPartitionSpec::builder();
        for field in &self.partition_by {
            let source = iceberg_schema
                .field_by_name(&field.column)
                .ok_or_else(|| format!("partition column not found: {}", field.column))?;
            builder = builder
                .add_partition_field(
                    source.id,
                    field.transform.iceberg_field_name(&field.column),
                    field.transform.to_iceberg(),
                )
                .map_err(|error| error.to_string())?;
        }
        Ok(builder.build())
    }

    /// Builds the Forge sort order for this layout, bound to the physical
    /// Iceberg schema.
    ///
    /// The order is exactly [`PhysicalLayout::sort_keys`] — the declared keys,
    /// or the time-column default when none were declared — so Forge rewrites
    /// reproduce the same physical order the writers emit.
    ///
    /// # Errors
    /// Returns a message when a sort column is absent from the Iceberg schema
    /// or when Iceberg rejects the bound sort fields.
    pub fn iceberg_sort_order(&self, iceberg_schema: &IcebergSchema) -> Result<SortOrder, String> {
        let mut builder = SortOrder::builder();
        builder.with_order_id(i64::from(BIFROST_SORT_ORDER_ID));
        for key in &self.sort_keys {
            let field = iceberg_schema
                .field_by_name(&key.column)
                .ok_or_else(|| format!("sort column not found: {}", key.column))?;
            builder.with_sort_field(SortField {
                source_id: field.id,
                transform: Transform::Identity,
                direction: key.direction.to_iceberg(),
                null_order: key.null_order.to_iceberg(),
            });
        }
        builder
            .build(iceberg_schema)
            .map_err(|error| error.to_string())
    }
}

/// Builds one public layout error.
fn invalid(
    table: &str,
    field: PhysicalLayoutField,
    violation: PhysicalLayoutViolation,
    column: Option<String>,
) -> BifrostError {
    BifrostError::InvalidPhysicalLayout {
        table: table.to_owned(),
        field,
        violation,
        column,
    }
}

/// Resolves the declared time column, or `wyrd_ingested_at` when none is named.
///
/// A time column must be a required column of the physical schema typed as a
/// zoned (`TIMESTAMP_LTZ`, one UTC instant) microsecond or nanosecond Arrow
/// timestamp. A naive `TIMESTAMP_NTZ` column names no UTC instant and is
/// refused, as is `TIMESTAMP_TZ`, a struct, with every other type.
///
/// # Errors
/// Returns [`BifrostError::InvalidPhysicalLayout`] on
/// [`PhysicalLayoutField::TimeColumn`] with `UnknownColumn` for an absent
/// column and `UnsupportedColumn` for a nullable, naive, or wrongly typed one.
fn canonical_time_column(
    table: &str,
    schema: &Schema,
    declared: Option<&str>,
) -> Result<String, BifrostError> {
    let column = declared.unwrap_or(WYRD_INGESTED_AT);
    let refuse = |violation| {
        invalid(
            table,
            PhysicalLayoutField::TimeColumn,
            violation,
            Some(column.to_owned()),
        )
    };
    let field = schema
        .field_with_name(column)
        .map_err(|_| refuse(PhysicalLayoutViolation::UnknownColumn))?;
    let zoned = matches!(field.data_type(), DataType::Timestamp(_, Some(_)));
    if TimePrecision::of(field.data_type()).is_none() || !zoned || field.is_nullable() {
        return Err(refuse(PhysicalLayoutViolation::UnsupportedColumn));
    }
    Ok(column.to_owned())
}

/// Canonicalizes declared partition fields in request order.
///
/// An empty declaration resolves to `hour(time_column)`. Otherwise the list
/// holds at most [`MAX_PARTITION_FIELDS`] fields naming each source column
/// once: exactly one `hour` or `day` field over `time_column`, plus
/// `identity`, `bucket`, or `truncate` dimensions over top-level `Utf8`,
/// `Int32`, or `Int64` columns, nullable or not.
///
/// # Errors
/// Returns [`BifrostError::InvalidPhysicalLayout`] on
/// [`PhysicalLayoutField::Partition`] with `TooManyKeys`, `UnknownColumn`,
/// `Duplicate`, `UnsupportedColumn` (a time transform off the time column or
/// a dimension over an ineligible type), or `Missing` (no time field); and on
/// [`PhysicalLayoutField::PartitionTransform`] for a missing, out-of-range, or
/// unexpected transform parameter.
fn canonical_partition_by(
    table: &str,
    schema: &Schema,
    time_column: &str,
    declared: &[PartitionFieldWire],
) -> Result<(TimeGranularity, Vec<LayoutPartitionField>), BifrostError> {
    if declared.is_empty() {
        let field = LayoutPartitionField {
            column: time_column.to_owned(),
            transform: PartitionTransform::Hour,
        };
        return Ok((TimeGranularity::Hour, vec![field]));
    }
    let refuse =
        |field, violation, column: &str| invalid(table, field, violation, Some(column.to_owned()));
    if declared.len() > MAX_PARTITION_FIELDS {
        return Err(invalid(
            table,
            PhysicalLayoutField::Partition,
            PhysicalLayoutViolation::TooManyKeys,
            None,
        ));
    }
    let mut seen = BTreeSet::new();
    let mut granularity = None;
    let mut fields = Vec::with_capacity(declared.len());
    for declared_field in declared {
        let column = declared_field.column.as_str();
        let source = schema.field_with_name(column).map_err(|_| {
            refuse(
                PhysicalLayoutField::Partition,
                PhysicalLayoutViolation::UnknownColumn,
                column,
            )
        })?;
        if !seen.insert(column) {
            return Err(refuse(
                PhysicalLayoutField::Partition,
                PhysicalLayoutViolation::Duplicate,
                column,
            ));
        }
        let transform = PartitionTransform::from_wire(declared_field).map_err(|violation| {
            refuse(PhysicalLayoutField::PartitionTransform, violation, column)
        })?;
        let eligible = match transform.granularity() {
            Some(time) => {
                granularity = Some(time);
                column == time_column
            }
            None => matches!(
                source.data_type(),
                DataType::Utf8 | DataType::Int32 | DataType::Int64
            ),
        };
        if !eligible {
            return Err(refuse(
                PhysicalLayoutField::Partition,
                PhysicalLayoutViolation::UnsupportedColumn,
                column,
            ));
        }
        fields.push(LayoutPartitionField {
            column: column.to_owned(),
            transform,
        });
    }
    let granularity = granularity.ok_or_else(|| {
        refuse(
            PhysicalLayoutField::Partition,
            PhysicalLayoutViolation::Missing,
            time_column,
        )
    })?;
    Ok((granularity, fields))
}

/// Canonicalizes declared sort keys in request order.
///
/// The sort order is entirely user-owned: nothing is injected ahead of a
/// declared key, and any column present in the physical schema is legal,
/// including the time column and every managed column. Naming the time column
/// therefore replaces the default rather than duplicating it.
///
/// The count cap is checked before per-key validation so a caller that declared
/// too many keys learns the shape fault first rather than a fault in the fifth
/// key. An unknown column, a nested (Variant, list, map, or struct) column,
/// and a repeated column are rejected. An
/// empty declaration resolves to `time_column` descending nulls-last, which is
/// what makes an explicit empty `sort_keys` mean exactly what an omitted one
/// means.
///
/// # Errors
/// Returns [`BifrostError::InvalidPhysicalLayout`] with
/// [`PhysicalLayoutViolation::TooManyKeys`], `UnknownColumn`,
/// `UnsupportedColumn`, or `Duplicate`.
fn canonical_sort_keys(
    table: &str,
    schema: &Schema,
    time_column: &str,
    declared: &[SortKeyWire],
) -> Result<Vec<LayoutSortKey>, BifrostError> {
    if declared.len() > MAX_SORT_KEYS {
        return Err(invalid(
            table,
            PhysicalLayoutField::SortKey,
            PhysicalLayoutViolation::TooManyKeys,
            None,
        ));
    }
    let mut seen = BTreeSet::new();
    let mut keys = Vec::with_capacity(declared.len().max(1));
    for key in declared {
        let field = schema.field_with_name(&key.column).map_err(|_| {
            invalid(
                table,
                PhysicalLayoutField::SortKey,
                PhysicalLayoutViolation::UnknownColumn,
                Some(key.column.clone()),
            )
        })?;
        if field.data_type().is_nested() {
            return Err(invalid(
                table,
                PhysicalLayoutField::SortKey,
                PhysicalLayoutViolation::UnsupportedColumn,
                Some(key.column.clone()),
            ));
        }
        if !seen.insert(key.column.clone()) {
            return Err(invalid(
                table,
                PhysicalLayoutField::SortKey,
                PhysicalLayoutViolation::Duplicate,
                Some(key.column.clone()),
            ));
        }
        keys.push(LayoutSortKey::new(
            key.column.clone(),
            match key.direction {
                SortDirectionWire::Asc => SortDirection::Asc,
                SortDirectionWire::Desc => SortDirection::Desc,
            },
            match key.null_order {
                NullOrderWire::First => NullOrder::First,
                NullOrderWire::Last => NullOrder::Last,
            },
        ));
    }
    if keys.is_empty() {
        keys.push(LayoutSortKey::default_for_time_column(time_column));
    }
    Ok(keys)
}

/// Canonicalizes declared Bloom columns in request order.
///
/// The declared list is the complete set: nothing is added to it, and an empty
/// list disables Bloom filters. At most [`MAX_BLOOM_COLUMNS`] columns may be
/// declared, each a schema-present `Utf8`, `Int32`, `Int64`, `Binary`, or
/// `FixedSizeBinary` column named once. List order carries no meaning beyond
/// the stored description: each column has its own independent filter.
///
/// # Errors
/// Returns [`BifrostError::InvalidPhysicalLayout`] with `TooManyKeys`,
/// `UnknownColumn`, `UnsupportedColumn`, or `Duplicate`.
fn canonical_bloom_columns(
    table: &str,
    schema: &Schema,
    declared: &[String],
) -> Result<Vec<String>, BifrostError> {
    let refuse = |violation, column: Option<&String>| {
        invalid(
            table,
            PhysicalLayoutField::BloomColumn,
            violation,
            column.cloned(),
        )
    };
    if declared.len() > MAX_BLOOM_COLUMNS {
        return Err(refuse(PhysicalLayoutViolation::TooManyKeys, None));
    }
    let mut seen = BTreeSet::new();
    for column in declared {
        let field = schema
            .field_with_name(column)
            .map_err(|_| refuse(PhysicalLayoutViolation::UnknownColumn, Some(column)))?;
        if !matches!(
            field.data_type(),
            DataType::Utf8
                | DataType::Int32
                | DataType::Int64
                | DataType::Binary
                | DataType::FixedSizeBinary(_)
        ) {
            return Err(refuse(
                PhysicalLayoutViolation::UnsupportedColumn,
                Some(column),
            ));
        }
        if !seen.insert(column) {
            return Err(refuse(PhysicalLayoutViolation::Duplicate, Some(column)));
        }
    }
    Ok(declared.to_vec())
}

/// One exact physical partition: a granularity plus its UTC start boundary.
///
/// This is the single durable partition identity. WAL slices, seal keys, object
/// paths, `vala.file_list` rows, Forge audit detail, Iceberg partition tuples,
/// and tail fences all carry exactly this value, so two artifacts belong to the
/// same partition if and only if their `TimePartition`s are equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimePartition {
    /// Granularity of the partition.
    granularity: TimeGranularity,
    /// Exact UTC start boundary.
    start_utc: DateTime<Utc>,
}

impl TimePartition {
    /// Constructs one exact partition.
    ///
    /// # Errors
    /// Returns [`TimePartitionError::NonCanonicalStart`] when `start_utc` is not
    /// the exact boundary of `granularity`.
    pub fn new(
        granularity: TimeGranularity,
        start_utc: DateTime<Utc>,
    ) -> Result<Self, TimePartitionError> {
        if !wyrd_spec::vala::api::partition_start_is_canonical(granularity.to_wire(), start_utc) {
            return Err(TimePartitionError::NonCanonicalStart {
                granularity: granularity.as_str(),
                start_utc,
            });
        }
        Ok(Self {
            granularity,
            start_utc,
        })
    }

    /// Restores a partition from its durable tag and epoch-microsecond start.
    ///
    /// # Errors
    /// Returns [`TimePartitionError::UnknownGranularityTag`] for tag `0` or any
    /// unknown tag, [`TimePartitionError::StartOutOfRange`] when the micros are
    /// not a representable instant, and
    /// [`TimePartitionError::NonCanonicalStart`] when the instant is not on its
    /// boundary.
    pub fn from_durable(tag: u8, start_unix_micros: i64) -> Result<Self, TimePartitionError> {
        let granularity =
            TimeGranularity::from_tag(tag).ok_or(TimePartitionError::UnknownGranularityTag(tag))?;
        let start = DateTime::from_timestamp_micros(start_unix_micros)
            .ok_or(TimePartitionError::StartOutOfRange(start_unix_micros))?;
        Self::new(granularity, start)
    }

    /// Returns the granularity.
    #[must_use]
    pub const fn granularity(self) -> TimeGranularity {
        self.granularity
    }

    /// Rebuilds a partition from its two durable `vala.file_list` columns.
    ///
    /// # Errors
    /// Returns [`TimePartitionError::UnknownGranularityToken`] when the stored
    /// text is not `hour` or `day`, and
    /// [`TimePartitionError::NonCanonicalStart`] when the stored instant is not
    /// on its boundary — the database CHECK constraint enforces the same rule,
    /// so either failure means the row was written outside Bifrost.
    pub fn from_durable_columns(
        granularity: &str,
        start_utc: DateTime<Utc>,
    ) -> Result<Self, TimePartitionError> {
        let granularity = TimeGranularity::from_str_token(granularity)
            .ok_or_else(|| TimePartitionError::UnknownGranularityToken(granularity.to_owned()))?;
        Self::new(granularity, start_utc)
    }

    /// Returns the stored `partition_granularity` text for durable columns.
    #[must_use]
    pub const fn granularity_str(self) -> &'static str {
        self.granularity.as_str()
    }

    /// Returns the exact UTC start boundary.
    #[must_use]
    pub const fn start_utc(self) -> DateTime<Utc> {
        self.start_utc
    }

    /// Returns the exclusive UTC end boundary.
    ///
    /// # Panics
    /// Panics only if the partition start is within one span of
    /// `DateTime::<Utc>::MAX`. [`TimeGranularity::bucket`] refuses such an
    /// instant, so no written partition reaches it.
    #[must_use]
    pub fn end_utc(self) -> DateTime<Utc> {
        self.start_utc
            .checked_add_signed(self.granularity.span())
            .expect("TimeGranularity::bucket refuses partitions without a representable end")
    }

    /// Reports whether this partition is still open at `now`.
    ///
    /// A partition is open while wall-clock time has not yet passed its
    /// exclusive end boundary, so Scribe may still publish new files into it.
    /// Forge uses this instead of a caller-threaded "current day" so the
    /// predicate stays correct for every registered granularity.
    ///
    /// # Panics
    /// Panics under the same unreachable condition as [`Self::end_utc`].
    #[must_use]
    pub fn is_open_at(self, now: DateTime<Utc>) -> bool {
        now < self.end_utc()
    }

    /// Returns the signed epoch-microsecond start used by durable encodings.
    #[must_use]
    pub fn start_unix_micros(self) -> i64 {
        self.start_utc.timestamp_micros()
    }

    /// Returns the durable one-byte granularity tag.
    #[must_use]
    pub const fn granularity_tag(self) -> u8 {
        self.granularity.tag()
    }

    /// Returns the Iceberg partition-transform literal for this partition:
    /// hours since epoch for [`TimeGranularity::Hour`], days since epoch for
    /// [`TimeGranularity::Day`].
    ///
    /// # Panics
    ///
    /// Panics when the transform value falls outside Iceberg's `i32` domain.
    /// [`TimeGranularity::bucket`] refuses such an instant, so no written
    /// partition reaches it.
    #[must_use]
    pub fn iceberg_transform_value(self) -> i32 {
        i32::try_from(
            self.start_utc
                .timestamp()
                .div_euclid(self.granularity.span().num_seconds()),
        )
        .expect("TimeGranularity::bucket refuses partitions outside the Iceberg i32 domain")
    }

    /// Returns the typed Iceberg partition literal for this partition.
    ///
    /// Iceberg types the `day` transform result as `date` and the `hour`
    /// transform result as `int`, so the shared integer from
    /// [`Self::iceberg_transform_value`] is wrapped in the primitive the
    /// destination partition field actually declares.
    ///
    /// # Panics
    ///
    /// Panics through [`Self::iceberg_transform_value`] when the partition
    /// boundary falls outside the Iceberg `i32` transform domain, which
    /// [`TimeGranularity::bucket`] refuses.
    #[must_use]
    pub fn iceberg_partition_literal(self) -> iceberg::spec::Literal {
        let value = self.iceberg_transform_value();
        match self.granularity {
            TimeGranularity::Hour => iceberg::spec::Literal::int(value),
            TimeGranularity::Day => iceberg::spec::Literal::date(value),
        }
    }

    /// Rebuilds a partition from an Iceberg transform literal.
    ///
    /// # Errors
    /// Returns [`TimePartitionError::StartOutOfRange`] when the literal does not
    /// name a representable instant.
    pub fn from_iceberg_transform_value(
        granularity: TimeGranularity,
        value: i32,
    ) -> Result<Self, TimePartitionError> {
        let multiplier: i64 = match granularity {
            TimeGranularity::Hour => 3_600,
            TimeGranularity::Day => 86_400,
        };
        let seconds = i64::from(value)
            .checked_mul(multiplier)
            .ok_or(TimePartitionError::StartOutOfRange(i64::from(value)))?;
        let start = DateTime::from_timestamp(seconds, 0)
            .ok_or(TimePartitionError::StartOutOfRange(seconds))?;
        Self::new(granularity, start)
    }

    /// Reads the time partition one data file's partition tuple holds under
    /// the spec it was written with.
    ///
    /// The spec supplies the time transform and its position; the tuple
    /// supplies the transformed value. Returns `None` when the spec has no
    /// time field or the tuple holds no integer at its position — a file
    /// Bifrost did not lay out.
    #[must_use]
    pub fn of_iceberg_partition(
        spec: &iceberg::spec::PartitionSpec,
        partition: &iceberg::spec::Struct,
    ) -> Option<Self> {
        let (index, _, granularity) = time_partition_field(spec)?;
        let Some(Some(iceberg::spec::Literal::Primitive(iceberg::spec::PrimitiveLiteral::Int(
            value,
        )))) = partition.fields().get(index)
        else {
            return None;
        };
        Self::from_iceberg_transform_value(granularity, *value).ok()
    }

    /// Renders the canonical object-path segment pair.
    #[must_use]
    pub fn as_path_components(&self) -> String {
        format!(
            "partition_granularity={}/partition_start={}",
            self.granularity.as_str(),
            self.start_utc.format("%Y-%m-%dT%HZ")
        )
    }

    /// Projects onto the public wire value.
    ///
    /// # Panics
    /// Never panics: a `TimePartition` is canonical by construction, which is
    /// exactly the invariant [`TimePartitionWire`] enforces.
    #[must_use]
    pub fn to_wire(self) -> TimePartitionWire {
        TimePartitionWire::new(self.granularity.to_wire(), self.start_utc)
            .expect("an engine partition is always canonical on the wire")
    }

    /// Adopts one public wire value.
    #[must_use]
    pub fn from_wire(wire: TimePartitionWire) -> Self {
        Self {
            granularity: TimeGranularity::from_wire(wire.granularity()),
            start_utc: wire.start_utc(),
        }
    }
}

impl PartialOrd for TimePartition {
    /// Delegates to the total order defined by [`Ord`].
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TimePartition {
    /// Orders by granularity tag first, then start instant, so a sorted list
    /// never interleaves granularities. A range is only meaningful once both
    /// endpoints are proven to share a granularity.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.granularity
            .tag()
            .cmp(&other.granularity.tag())
            .then(self.start_utc.cmp(&other.start_utc))
    }
}

impl std::fmt::Display for TimePartition {
    /// Renders `<granularity>:<RFC 3339 start>` for logs, metrics, and errors.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}",
            self.granularity.as_str(),
            self.start_utc.to_rfc3339()
        )
    }
}

/// Failures from constructing or restoring a [`TimePartition`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TimePartitionError {
    /// The durable granularity tag was `0` or otherwise unknown.
    #[error("unknown time-partition granularity tag: {0}")]
    UnknownGranularityTag(u8),
    /// The stored `partition_granularity` text is not a known granularity.
    #[error("unknown time-partition granularity token: {0}")]
    UnknownGranularityToken(String),
    /// The encoded start value is not a representable UTC instant.
    #[error("time-partition start is out of range: {0}")]
    StartOutOfRange(i64),
    /// The instant is not the exact boundary of its granularity.
    #[error("time-partition start {start_utc} is not a {granularity} boundary")]
    NonCanonicalStart {
        /// Granularity whose boundary was violated.
        granularity: &'static str,
        /// The offending instant.
        start_utc: DateTime<Utc>,
    },
}

#[cfg(test)]
mod tests {
    use super::{
        LayoutSortKey, MAX_BLOOM_COLUMNS, MAX_SORT_KEYS, NullOrder, PartitionTransform,
        PhysicalLayout, SortDirection, TimeGranularity,
    };
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use wyrd_spec::vala::api::{
        NullOrderWire, PartitionFieldWire, PartitionTransformWire, PhysicalLayoutWire,
        SortDirectionWire, SortKeyWire,
    };
    use wyrd_spec::vala::managed_columns::{RUN_ID, SUB_PRINCIPAL_ID, WYRD_INGESTED_AT};
    use wyrd_spec::vala::{BifrostError, PhysicalLayoutField, PhysicalLayoutViolation};

    /// Canonical fully-qualified name every case in this module resolves under.
    const TABLE: &str = "bifrost.datasets.layout";

    /// A zoned UTC timestamp at `unit`.
    fn utc(unit: TimeUnit) -> DataType {
        DataType::Timestamp(unit, Some("+00:00".into()))
    }

    /// Builds the fixture physical schema: managed columns, one candidate time
    /// column per accepted and refused shape, and one column per eligible and
    /// ineligible partition, sort, and Bloom type.
    fn fixture_schema() -> Schema {
        Schema::new(vec![
            Field::new(WYRD_INGESTED_AT, utc(TimeUnit::Microsecond), false),
            Field::new("started_at", utc(TimeUnit::Nanosecond), false),
            Field::new(
                "local_at",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
            Field::new("maybe_at", utc(TimeUnit::Microsecond), true),
            Field::new(
                "seconds_at",
                DataType::Timestamp(TimeUnit::Second, None),
                false,
            ),
            Field::new(RUN_ID, DataType::Utf8, true),
            Field::new(SUB_PRINCIPAL_ID, DataType::Utf8, false),
            Field::new("customer", DataType::Utf8, true),
            Field::new("region", DataType::Utf8, true),
            Field::new("shard", DataType::Int32, true),
            Field::new("account", DataType::Int64, false),
            Field::new("digest", DataType::Binary, true),
            Field::new("hash", DataType::FixedSizeBinary(16), true),
            Field::new("score", DataType::Float64, true),
            Field::new("flag", DataType::Boolean, true),
            Field::new("day", DataType::Date32, true),
            Field::new(
                "tags",
                DataType::List(std::sync::Arc::new(Field::new(
                    "item",
                    DataType::Utf8,
                    true,
                ))),
                true,
            ),
        ])
    }

    /// Builds one ascending nulls-last declared sort key.
    fn asc(column: &str) -> SortKeyWire {
        SortKeyWire {
            column: column.to_owned(),
            direction: SortDirectionWire::Asc,
            null_order: NullOrderWire::Last,
        }
    }

    /// Builds one partition field with optional bucket and width parameters.
    fn part(
        column: &str,
        transform: PartitionTransformWire,
        buckets: Option<u32>,
        width: Option<u32>,
    ) -> PartitionFieldWire {
        PartitionFieldWire {
            buckets,
            width,
            ..PartitionFieldWire::new(column, transform)
        }
    }

    /// Builds one declaration over the supplied partition, sort, and Bloom
    /// intent, with the default time column.
    fn declaration(
        partition_by: Vec<PartitionFieldWire>,
        sort_keys: Vec<SortKeyWire>,
        bloom_columns: Vec<String>,
    ) -> PhysicalLayoutWire {
        PhysicalLayoutWire {
            time_column: None,
            partition_by,
            sort_keys,
            bloom_columns,
        }
    }

    /// Projects a resolved layout's sort order onto its column names.
    fn sort_columns(layout: &PhysicalLayout) -> Vec<String> {
        layout
            .sort_keys()
            .iter()
            .map(|key| key.column().to_owned())
            .collect()
    }

    /// Asserts one declaration is refused with the exact public error.
    ///
    /// # Panics
    ///
    /// Panics when the declaration resolves, or fails with a different field,
    /// violation, or column than the one named.
    fn assert_refused(
        schema: &Schema,
        declared: &PhysicalLayoutWire,
        field: PhysicalLayoutField,
        violation: PhysicalLayoutViolation,
        column: Option<&str>,
    ) {
        let error = PhysicalLayout::resolve(TABLE, schema, Some(declared))
            .expect_err("declaration must be refused");
        assert_eq!(
            error,
            BifrostError::InvalidPhysicalLayout {
                table: TABLE.to_owned(),
                field,
                violation,
                column: column.map(str::to_owned),
            },
            "{declared:?}"
        );
    }

    /// The bounded composite layout matrix the single validator enforces.
    ///
    /// Accepts the defaults and every eligible hour/day plus identity, bucket,
    /// and truncate shape over `Utf8`, `Int32`, and `Int64` sources, nullable
    /// or not, with the time field at any position; each canonical form is a
    /// fixed point through the stored wire and builds a bound Iceberg spec.
    /// Refuses every unsupported type, transform, parameter, count, and
    /// duplicate with its exact public error, including a nullable, naive, or
    /// coarse time axis.
    ///
    /// # Panics
    ///
    /// Panics when an eligible declaration is refused, an ineligible one
    /// resolves, or any refusal names the wrong field, violation, or column.
    #[test]
    fn bounded_composite_layout_validation() {
        use PartitionTransformWire::{Bucket, Day, Hour, Identity, Truncate};
        let schema = fixture_schema();
        let convertible = Schema::new(
            schema
                .fields()
                .iter()
                .filter(|field| field.name() != "seconds_at")
                .cloned()
                .collect::<Vec<_>>(),
        );
        let iceberg_schema =
            crate::tables::iceberg_schema_for(&convertible).expect("fixture has an Iceberg form");

        // An omitted declaration and an explicit empty one resolve identically:
        // hourly on ingestion time, one descending key, no Bloom column.
        let omitted =
            PhysicalLayout::resolve(TABLE, &schema, None).expect("an omitted declaration resolves");
        let empty = PhysicalLayout::resolve(
            TABLE,
            &schema,
            Some(&declaration(Vec::new(), Vec::new(), Vec::new())),
        )
        .expect("an empty declaration resolves");
        assert_eq!(omitted, empty);
        assert_eq!(omitted.granularity(), TimeGranularity::Hour);
        assert_eq!(omitted.time_column(), WYRD_INGESTED_AT);
        assert_eq!(
            omitted.to_wire().partition_by,
            vec![part(WYRD_INGESTED_AT, Hour, None, None)]
        );
        assert_eq!(omitted.dimensions().count(), 0);
        assert_eq!(sort_columns(&omitted), vec![WYRD_INGESTED_AT.to_owned()]);
        assert!(omitted.bloom_columns().is_empty(), "no hidden Bloom floor");

        // Every eligible composite shape resolves, keeps its declared order, and
        // is a canonical fixed point that builds an Iceberg spec.
        let accepted: Vec<(Option<&str>, Vec<PartitionFieldWire>)> = vec![
            (None, vec![part(WYRD_INGESTED_AT, Day, None, None)]),
            (
                Some("started_at"),
                vec![part("started_at", Hour, None, None)],
            ),
            (
                None,
                vec![
                    part(WYRD_INGESTED_AT, Hour, None, None),
                    part("customer", Identity, None, None),
                    part("shard", Bucket, Some(2), None),
                ],
            ),
            (
                None,
                vec![
                    part("account", Truncate, None, Some(1)),
                    part(WYRD_INGESTED_AT, Day, None, None),
                    part("region", Bucket, Some(1024), None),
                ],
            ),
            (
                None,
                vec![
                    part(WYRD_INGESTED_AT, Hour, None, None),
                    part("customer", Truncate, None, Some(3)),
                    part("account", Identity, None, None),
                ],
            ),
            (
                None,
                vec![
                    part(WYRD_INGESTED_AT, Hour, None, None),
                    part("shard", Identity, None, None),
                ],
            ),
        ];
        for (time_column, partition_by) in accepted {
            let declared = PhysicalLayoutWire {
                time_column: time_column.map(str::to_owned),
                ..declaration(partition_by.clone(), Vec::new(), Vec::new())
            };
            let layout = PhysicalLayout::resolve(TABLE, &schema, Some(&declared))
                .unwrap_or_else(|error| panic!("{partition_by:?} must resolve: {error:?}"));
            let stored = layout.to_wire();
            assert_eq!(stored.partition_by, partition_by);
            assert_eq!(layout.dimensions().count(), partition_by.len() - 1);
            assert_eq!(
                PhysicalLayout::from_stored_wire(TABLE, &schema, &stored)
                    .expect("canonical layout re-resolves")
                    .to_wire(),
                stored
            );
            let spec = layout
                .iceberg_partition_spec(&iceberg_schema)
                .expect("the layout binds an Iceberg spec");
            assert_eq!(spec.fields().len(), partition_by.len());
            for (field, declared) in spec.fields().iter().zip(&partition_by) {
                let transform = PartitionTransform::from_wire(declared).expect("valid");
                assert_eq!(field.transform, transform.to_iceberg());
                assert_eq!(field.name, transform.iceberg_field_name(&declared.column));
            }
        }

        let time = |transform| part(WYRD_INGESTED_AT, transform, None, None);
        let refused: Vec<(
            Vec<PartitionFieldWire>,
            PhysicalLayoutField,
            PhysicalLayoutViolation,
            Option<&str>,
        )> = vec![
            // Count: a fourth field is refused before any per-field fault.
            (
                vec![
                    time(Hour),
                    part("customer", Identity, None, None),
                    part("region", Identity, None, None),
                    part("absent", Identity, None, None),
                ],
                PhysicalLayoutField::Partition,
                PhysicalLayoutViolation::TooManyKeys,
                None,
            ),
            // No time field, or a second one.
            (
                vec![part("customer", Identity, None, None)],
                PhysicalLayoutField::Partition,
                PhysicalLayoutViolation::Missing,
                Some(WYRD_INGESTED_AT),
            ),
            (
                vec![time(Hour), time(Day)],
                PhysicalLayoutField::Partition,
                PhysicalLayoutViolation::Duplicate,
                Some(WYRD_INGESTED_AT),
            ),
            // A source column named twice is redundant.
            (
                vec![
                    time(Hour),
                    part("customer", Identity, None, None),
                    part("customer", Bucket, Some(8), None),
                ],
                PhysicalLayoutField::Partition,
                PhysicalLayoutViolation::Duplicate,
                Some("customer"),
            ),
            (
                vec![time(Hour), part("absent", Identity, None, None)],
                PhysicalLayoutField::Partition,
                PhysicalLayoutViolation::UnknownColumn,
                Some("absent"),
            ),
            // A time transform off the time column.
            (
                vec![part("started_at", Hour, None, None)],
                PhysicalLayoutField::Partition,
                PhysicalLayoutViolation::UnsupportedColumn,
                Some("started_at"),
            ),
            // A dimension transform over the time column.
            (
                vec![part(WYRD_INGESTED_AT, Identity, None, None)],
                PhysicalLayoutField::Partition,
                PhysicalLayoutViolation::UnsupportedColumn,
                Some(WYRD_INGESTED_AT),
            ),
            // Bucket parameters.
            (
                vec![time(Hour), part("customer", Bucket, None, None)],
                PhysicalLayoutField::PartitionTransform,
                PhysicalLayoutViolation::Missing,
                Some("customer"),
            ),
            (
                vec![time(Hour), part("customer", Bucket, Some(1), None)],
                PhysicalLayoutField::PartitionTransform,
                PhysicalLayoutViolation::OutOfRange,
                Some("customer"),
            ),
            (
                vec![time(Hour), part("customer", Bucket, Some(1025), None)],
                PhysicalLayoutField::PartitionTransform,
                PhysicalLayoutViolation::OutOfRange,
                Some("customer"),
            ),
            (
                vec![time(Hour), part("customer", Bucket, Some(8), Some(2))],
                PhysicalLayoutField::PartitionTransform,
                PhysicalLayoutViolation::UnsupportedValue,
                Some("customer"),
            ),
            // Truncate parameters.
            (
                vec![time(Hour), part("customer", Truncate, None, None)],
                PhysicalLayoutField::PartitionTransform,
                PhysicalLayoutViolation::Missing,
                Some("customer"),
            ),
            (
                vec![time(Hour), part("customer", Truncate, None, Some(0))],
                PhysicalLayoutField::PartitionTransform,
                PhysicalLayoutViolation::OutOfRange,
                Some("customer"),
            ),
            (
                vec![
                    time(Hour),
                    part(
                        "customer",
                        Truncate,
                        None,
                        Some(u32::try_from(i32::MAX).expect("fits") + 1),
                    ),
                ],
                PhysicalLayoutField::PartitionTransform,
                PhysicalLayoutViolation::OutOfRange,
                Some("customer"),
            ),
            (
                vec![time(Hour), part("customer", Identity, Some(4), None)],
                PhysicalLayoutField::PartitionTransform,
                PhysicalLayoutViolation::UnsupportedValue,
                Some("customer"),
            ),
            (
                vec![part(WYRD_INGESTED_AT, Day, None, Some(4))],
                PhysicalLayoutField::PartitionTransform,
                PhysicalLayoutViolation::UnsupportedValue,
                Some(WYRD_INGESTED_AT),
            ),
        ];
        let mut refused = refused;
        // Every ineligible dimension type, under each dimension transform.
        for column in ["digest", "hash", "score", "flag", "day", "tags", "local_at"] {
            for (transform, buckets, width) in [
                (Identity, None, None),
                (Bucket, Some(4), None),
                (Truncate, None, Some(4)),
            ] {
                refused.push((
                    vec![time(Hour), part(column, transform, buckets, width)],
                    PhysicalLayoutField::Partition,
                    PhysicalLayoutViolation::UnsupportedColumn,
                    Some(column),
                ));
            }
        }
        for (partition_by, field, violation, column) in refused {
            assert_refused(
                &schema,
                &declaration(partition_by, Vec::new(), Vec::new()),
                field,
                violation,
                column,
            );
        }

        // The time axis must be a required, zoned microsecond or nanosecond
        // timestamp.
        for (column, violation) in [
            ("absent", PhysicalLayoutViolation::UnknownColumn),
            ("maybe_at", PhysicalLayoutViolation::UnsupportedColumn),
            ("local_at", PhysicalLayoutViolation::UnsupportedColumn),
            ("seconds_at", PhysicalLayoutViolation::UnsupportedColumn),
            ("customer", PhysicalLayoutViolation::UnsupportedColumn),
        ] {
            assert_refused(
                &schema,
                &PhysicalLayoutWire {
                    time_column: Some(column.to_owned()),
                    ..declaration(Vec::new(), Vec::new(), Vec::new())
                },
                PhysicalLayoutField::TimeColumn,
                violation,
                Some(column),
            );
        }

        // Bloom: exactly the eligible types, at most sixteen, each once, kept
        // in request order with nothing added.
        let blooms = ["customer", "shard", "account", "digest", "hash"]
            .map(str::to_owned)
            .to_vec();
        let resolved = PhysicalLayout::resolve(
            TABLE,
            &schema,
            Some(&declaration(Vec::new(), Vec::new(), blooms.clone())),
        )
        .expect("every eligible Bloom type resolves");
        assert_eq!(resolved.bloom_columns(), blooms.as_slice());
        let too_many = (0..=MAX_BLOOM_COLUMNS)
            .map(|index| format!("c{index}"))
            .collect();
        assert_refused(
            &schema,
            &declaration(Vec::new(), Vec::new(), too_many),
            PhysicalLayoutField::BloomColumn,
            PhysicalLayoutViolation::TooManyKeys,
            None,
        );
        for column in ["score", "flag", "day", "tags", WYRD_INGESTED_AT] {
            assert_refused(
                &schema,
                &declaration(Vec::new(), Vec::new(), vec![column.to_owned()]),
                PhysicalLayoutField::BloomColumn,
                PhysicalLayoutViolation::UnsupportedColumn,
                Some(column),
            );
        }
        for (blooms, violation, column) in [
            (
                vec!["absent"],
                PhysicalLayoutViolation::UnknownColumn,
                "absent",
            ),
            (
                vec!["customer", "customer"],
                PhysicalLayoutViolation::Duplicate,
                "customer",
            ),
        ] {
            assert_refused(
                &schema,
                &declaration(
                    Vec::new(),
                    Vec::new(),
                    blooms.into_iter().map(str::to_owned).collect(),
                ),
                PhysicalLayoutField::BloomColumn,
                violation,
                Some(column),
            );
        }

        // Sort: scalar sources only, at most four, each once.
        assert_refused(
            &schema,
            &declaration(Vec::new(), vec![asc("tags")], Vec::new()),
            PhysicalLayoutField::SortKey,
            PhysicalLayoutViolation::UnsupportedColumn,
            Some("tags"),
        );
        assert!(
            PhysicalLayout::resolve(
                TABLE,
                &schema,
                Some(&declaration(
                    Vec::new(),
                    vec![asc("score"), asc("flag"), asc("day"), asc("digest")],
                    Vec::new()
                )),
            )
            .is_ok(),
            "every scalar type is an eligible sort source"
        );
    }

    /// The declarations the single resolution entry point accepts for the sort
    /// order: the time column declared explicitly replaces the default, managed
    /// columns are legal, and nothing is prepended.
    #[test]
    fn canonical_layout_accepts_declared_shapes() {
        let schema = fixture_schema();
        let named = PhysicalLayout::resolve(
            TABLE,
            &schema,
            Some(&declaration(
                Vec::new(),
                vec![asc(WYRD_INGESTED_AT)],
                Vec::new(),
            )),
        )
        .expect("the time column is a legal declared sort key");
        assert_eq!(
            named.sort_keys(),
            [LayoutSortKey::new(
                WYRD_INGESTED_AT,
                SortDirection::Asc,
                NullOrder::Last
            )],
            "the declaration replaces the descending default"
        );
        let managed = PhysicalLayout::resolve(
            TABLE,
            &schema,
            Some(&declaration(
                Vec::new(),
                vec![asc(RUN_ID), asc("customer")],
                vec![RUN_ID.to_owned(), "region".to_owned()],
            )),
        )
        .expect("managed columns are legal sort and Bloom columns");
        assert_eq!(
            sort_columns(&managed),
            vec![RUN_ID.to_owned(), "customer".to_owned()]
        );
        assert_eq!(managed.bloom_columns(), [RUN_ID, "region"]);
    }

    /// The sort-key cap admits exactly `MAX_SORT_KEYS` and refuses the next
    /// one before any per-key validation; unknown and duplicate keys stay
    /// refused.
    #[test]
    fn canonical_layout_refuses_shape_and_column_faults() {
        let schema = fixture_schema();
        let four = vec![
            asc(WYRD_INGESTED_AT),
            asc(RUN_ID),
            asc("region"),
            asc("customer"),
        ];
        assert_eq!(four.len(), MAX_SORT_KEYS);
        assert!(
            PhysicalLayout::resolve(
                TABLE,
                &schema,
                Some(&declaration(Vec::new(), four.clone(), Vec::new()))
            )
            .is_ok(),
            "four declared keys are admitted"
        );
        let mut five = four;
        five.push(asc("absent"));
        assert_refused(
            &schema,
            &declaration(Vec::new(), five, Vec::new()),
            PhysicalLayoutField::SortKey,
            PhysicalLayoutViolation::TooManyKeys,
            None,
        );
        assert_refused(
            &schema,
            &declaration(Vec::new(), vec![asc("absent")], Vec::new()),
            PhysicalLayoutField::SortKey,
            PhysicalLayoutViolation::UnknownColumn,
            Some("absent"),
        );
        assert_refused(
            &schema,
            &declaration(
                Vec::new(),
                vec![asc("customer"), asc("customer")],
                Vec::new(),
            ),
            PhysicalLayoutField::SortKey,
            PhysicalLayoutViolation::Duplicate,
            Some("customer"),
        );
    }

    /// A declared time column is the source of the time partition field and
    /// the default first sort key, at either precision, and the canonical wire
    /// names it so a repeat registration compares it.
    #[test]
    fn time_column_drives_partition_and_sort() {
        let schema = fixture_schema();
        let declared = PhysicalLayoutWire {
            time_column: Some("started_at".to_owned()),
            ..declaration(Vec::new(), Vec::new(), Vec::new())
        };
        let layout = PhysicalLayout::resolve(TABLE, &schema, Some(&declared))
            .expect("a required zoned timestamp column is a legal time column");
        assert_eq!(layout.time_column(), "started_at");
        assert_eq!(layout.time_axis().column(), "started_at");
        assert_eq!(
            layout.sort_keys(),
            [LayoutSortKey::new(
                "started_at",
                SortDirection::Desc,
                NullOrder::Last
            )]
        );
        let stored = layout.to_wire();
        assert_eq!(stored.time_column.as_deref(), Some("started_at"));
        assert_eq!(
            stored.partition_by,
            vec![part("started_at", PartitionTransformWire::Hour, None, None)]
        );
        assert_eq!(super::TimeAxis::of_wire(&stored), layout.time_axis());
    }

    /// Nanosecond instants narrow to a microsecond interval that contains
    /// them, and widen back to the full nanosecond span of that interval.
    ///
    /// A lower bound floors and an upper bound ceils on both sides of the
    /// epoch, so no stored nanosecond falls outside the interval a reader
    /// prunes with.
    ///
    /// # Panics
    ///
    /// Panics when a nanosecond bound is narrowed or widened inexactly.
    #[test]
    fn nanosecond_bounds_contain_every_stored_instant() {
        let nanos = super::TimePrecision::of(&DataType::Timestamp(
            TimeUnit::Nanosecond,
            Some("+00:00".into()),
        ))
        .expect("a nanosecond timestamp is a time column");
        assert_eq!(nanos.floor_micros(1_500), 1);
        assert_eq!(nanos.ceil_micros(1_500), 2);
        assert_eq!(nanos.ceil_micros(2_000), 2);
        assert_eq!(nanos.floor_micros(-1_500), -2);
        assert_eq!(nanos.ceil_micros(-1_500), -1);
        assert_eq!(nanos.lower_from_micros(1), Some(1_000));
        assert_eq!(nanos.upper_from_micros(2), Some(2_999));
        assert_eq!(nanos.upper_from_micros(i64::MAX / 1_000 + 1), None);
        assert_eq!(
            super::TimePrecision::of(&DataType::Timestamp(TimeUnit::Second, None)),
            None
        );
    }
}
