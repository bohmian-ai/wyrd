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

use arrow::datatypes::Schema;
use chrono::{DateTime, Duration, DurationRound as _, Utc};
use iceberg::spec::{
    NullOrder as IcebergNullOrder, Schema as IcebergSchema, SortDirection as IcebergSortDirection,
    SortField, SortOrder, Transform, UnboundPartitionSpec,
};
use wyrd_spec::vala::api::{
    NullOrderWire, PhysicalLayoutWire, SortDirectionWire, SortKeyWire, TimeGranularityWire,
    TimePartitionWire,
};
use wyrd_spec::vala::managed_columns::{CARD_UID, PRINCIPAL_ID, RUN_ID, WYRD_EVENT_TIME};
use wyrd_spec::vala::{BifrostError, PhysicalLayoutField, PhysicalLayoutViolation};

/// Managed Bloom floor, in the stable order every writer must emit it.
///
/// A name is included only when the table's physical schema actually has the
/// column, so a table without a correlation policy does not claim Blooms it
/// cannot write.
///
/// `data_tenant_id` is deliberately absent. Every tenant owns its own Iceberg
/// namespace and object prefix, so the column is constant within any single
/// file and a Bloom filter over it can never prune one.
pub const MANAGED_BLOOM_FLOOR: [&str; 3] = [RUN_ID, CARD_UID, PRINCIPAL_ID];

/// Largest number of sort keys one declaration may carry.
///
/// Each additional key costs write-side sort time and buys progressively less
/// clustering, so the contract caps the declaration rather than accepting an
/// unbounded list and silently truncating it.
pub const MAX_SORT_KEYS: usize = 4;

/// Time-partition granularity owned by the engine.
///
/// Bifrost v1 admits exactly these two Iceberg-native transforms on
/// `wyrd_event_time`. Identity, bucket, truncate, month, year, and multi-column
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
    /// partition boundary is not representable, which no event time inside the
    /// ingest acceptance window can reach.
    pub fn bucket(self, event_time: DateTime<Utc>) -> Result<TimePartition, TimePartitionError> {
        let start = event_time
            .duration_trunc(self.span())
            .map_err(|_| TimePartitionError::StartOutOfRange(event_time.timestamp_micros()))?;
        TimePartition::new(self, start)
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

/// The one time-partition declaration of a table.
///
/// The partitioned column is the invariant [`WYRD_EVENT_TIME`] and is therefore
/// not stored; only the granularity varies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimePartitionSpec {
    /// Partition granularity.
    granularity: TimeGranularity,
}

impl TimePartitionSpec {
    /// Constructs the partition spec for one granularity.
    #[must_use]
    pub const fn new(granularity: TimeGranularity) -> Self {
        Self { granularity }
    }

    /// Returns the granularity.
    #[must_use]
    pub const fn granularity(self) -> TimeGranularity {
        self.granularity
    }

    /// Returns the invariant partitioned column name.
    #[must_use]
    pub const fn column(self) -> &'static str {
        WYRD_EVENT_TIME
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

    /// Returns the sole key a layout resolves to when no sort key is declared.
    ///
    /// Ordering on `wyrd_event_time` is retained for encoding, not pruning:
    /// `DELTA_BINARY_PACKED` is near-free on a sorted timestamp column and
    /// expensive on an unsorted one.
    #[must_use]
    pub fn default_event_time() -> Self {
        Self::new(WYRD_EVENT_TIME, SortDirection::Desc, NullOrder::Last)
    }
}

/// The canonical, fully resolved physical layout of one table.
///
/// Construct it only through [`PhysicalLayout::resolve`] or
/// [`PhysicalLayout::from_stored_wire`]; both guarantee a non-empty sort order,
/// the managed Bloom floor, and schema-validated columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalLayout {
    /// Resolved time partition.
    partition: TimePartitionSpec,
    /// Canonical sort order, carrying at least one key.
    sort_keys: Vec<LayoutSortKey>,
    /// Canonical Bloom columns, always beginning with the schema-present managed
    /// floor.
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
    /// An omitted declaration resolves to hourly `wyrd_event_time`
    /// partitioning, `wyrd_event_time` descending nulls-last as the sole sort
    /// key, and the schema-present managed Bloom floor. An explicit declaration
    /// whose `sort_keys` is empty resolves to the same sort order, because the
    /// system injects nothing of its own.
    ///
    /// Validation precedence matches the public contract: the sort-key count,
    /// then sort keys in request order, then Bloom columns in request order.
    /// The first fault wins and nothing is mutated.
    ///
    /// # Errors
    /// Returns [`BifrostError::InvalidPhysicalLayout`] naming the exact field,
    /// violation, and offending column.
    pub fn resolve(
        table: &str,
        schema: &Schema,
        declared: Option<&PhysicalLayoutWire>,
    ) -> Result<Self, BifrostError> {
        let Some(declared) = declared else {
            return Ok(Self {
                partition: TimePartitionSpec::new(TimeGranularity::Hour),
                sort_keys: vec![LayoutSortKey::default_event_time()],
                bloom_columns: managed_bloom_floor(schema),
            });
        };

        let partition =
            TimePartitionSpec::new(TimeGranularity::from_wire(declared.partition_granularity));
        let sort_keys = canonical_sort_keys(table, schema, &declared.sort_keys)?;
        let bloom_columns = canonical_bloom_columns(table, schema, &declared.bloom_columns)?;

        Ok(Self {
            partition,
            sort_keys,
            bloom_columns,
        })
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

    /// Returns the resolved partition spec.
    #[must_use]
    pub const fn partition(&self) -> TimePartitionSpec {
        self.partition
    }

    /// Returns the resolved partition granularity.
    #[must_use]
    pub const fn granularity(&self) -> TimeGranularity {
        self.partition.granularity()
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

    /// Buckets one `wyrd_event_time` value into its exact partition.
    ///
    /// # Errors
    /// Returns [`TimePartitionError::StartOutOfRange`] when the instant cannot be
    /// truncated to a representable boundary.
    pub fn partition_of(
        &self,
        event_time: DateTime<Utc>,
    ) -> Result<TimePartition, TimePartitionError> {
        self.granularity().bucket(event_time)
    }

    /// Projects the canonical layout onto the public wire shape.
    ///
    /// The result is fully populated: it is what describe returns and what the
    /// control row stores.
    #[must_use]
    pub fn to_wire(&self) -> PhysicalLayoutWire {
        PhysicalLayoutWire {
            partition_granularity: self.granularity().to_wire(),
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

    /// Builds the Iceberg partition spec for this layout.
    ///
    /// # Errors
    /// Returns a message when `wyrd_event_time` is absent from the Iceberg
    /// schema or when Iceberg rejects the partition field.
    pub fn iceberg_partition_spec(
        &self,
        iceberg_schema: &IcebergSchema,
    ) -> Result<UnboundPartitionSpec, String> {
        let field = iceberg_schema
            .field_by_name(WYRD_EVENT_TIME)
            .ok_or_else(|| format!("partition column not found: {WYRD_EVENT_TIME}"))?;
        let (transform, name) = match self.granularity() {
            TimeGranularity::Hour => (
                iceberg::spec::Transform::Hour,
                format!("{WYRD_EVENT_TIME}_hour"),
            ),
            TimeGranularity::Day => (
                iceberg::spec::Transform::Day,
                format!("{WYRD_EVENT_TIME}_day"),
            ),
        };
        Ok(UnboundPartitionSpec::builder()
            .add_partition_field(field.id, &name, transform)
            .map_err(|error| error.to_string())?
            .build())
    }

    /// Returns the Iceberg partition-field name this layout's granularity
    /// produces, so validation can compare the recorded name exactly.
    #[must_use]
    pub fn iceberg_partition_field_name(&self) -> String {
        match self.granularity() {
            TimeGranularity::Hour => format!("{WYRD_EVENT_TIME}_hour"),
            TimeGranularity::Day => format!("{WYRD_EVENT_TIME}_day"),
        }
    }

    /// Returns the Iceberg transform this layout's granularity uses.
    #[must_use]
    pub const fn iceberg_transform(&self) -> Transform {
        match self.granularity() {
            TimeGranularity::Hour => Transform::Hour,
            TimeGranularity::Day => Transform::Day,
        }
    }

    /// Builds the Forge sort order for this layout, bound to the physical
    /// Iceberg schema.
    ///
    /// The order is exactly [`PhysicalLayout::sort_keys`] — the declared keys,
    /// or the event-time default when none were declared — so Forge rewrites
    /// reproduce the same physical order the writers emit.
    ///
    /// # Errors
    /// Returns a message when a sort column is absent from the Iceberg schema
    /// or when Iceberg rejects the bound sort fields.
    pub fn iceberg_sort_order(&self, iceberg_schema: &IcebergSchema) -> Result<SortOrder, String> {
        let mut builder = SortOrder::builder();
        builder.with_order_id(1);
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

/// Returns the schema-present managed Bloom floor in its stable order.
fn managed_bloom_floor(schema: &Schema) -> Vec<String> {
    MANAGED_BLOOM_FLOOR
        .iter()
        .filter(|column| schema.field_with_name(column).is_ok())
        .map(|column| (*column).to_owned())
        .collect()
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

/// Canonicalizes declared sort keys in request order.
///
/// The sort order is entirely user-owned: nothing is injected ahead of a
/// declared key, and any column present in the physical schema is legal,
/// including `wyrd_event_time` and every other managed column. Naming
/// `wyrd_event_time` therefore replaces the default rather than duplicating it.
///
/// The count cap is checked before per-key validation so a caller that declared
/// too many keys learns the shape fault first rather than a fault in the fifth
/// key. An unknown column is rejected and a repeated column is rejected. An
/// empty declaration resolves to the event-time default, which is what makes an
/// explicit empty `sort_keys` mean exactly what an omitted one means.
///
/// # Errors
/// Returns [`BifrostError::InvalidPhysicalLayout`] with
/// [`PhysicalLayoutViolation::TooManyKeys`], `UnknownColumn`, or `Duplicate`.
fn canonical_sort_keys(
    table: &str,
    schema: &Schema,
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
        if schema.field_with_name(&key.column).is_err() {
            return Err(invalid(
                table,
                PhysicalLayoutField::SortKey,
                PhysicalLayoutViolation::UnknownColumn,
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
        keys.push(LayoutSortKey::default_event_time());
    }
    Ok(keys)
}

/// Canonicalizes declared Bloom columns in request order.
///
/// The schema-present managed floor always comes first; a caller naming a floor
/// column is deduplicated by union rather than rejected. Any other
/// schema-present column may be added, including a managed column outside the
/// floor. An unknown column is rejected and a repeated non-floor column is
/// rejected.
///
/// # Errors
/// Returns [`BifrostError::InvalidPhysicalLayout`] with `UnknownColumn` or
/// `Duplicate`.
fn canonical_bloom_columns(
    table: &str,
    schema: &Schema,
    declared: &[String],
) -> Result<Vec<String>, BifrostError> {
    let mut columns = managed_bloom_floor(schema);
    let mut seen: BTreeSet<String> = columns.iter().cloned().collect();
    for column in declared {
        if MANAGED_BLOOM_FLOOR.contains(&column.as_str()) {
            continue;
        }
        if schema.field_with_name(column).is_err() {
            return Err(invalid(
                table,
                PhysicalLayoutField::BloomColumn,
                PhysicalLayoutViolation::UnknownColumn,
                Some(column.clone()),
            ));
        }
        if !seen.insert(column.clone()) {
            return Err(invalid(
                table,
                PhysicalLayoutField::BloomColumn,
                PhysicalLayoutViolation::Duplicate,
                Some(column.clone()),
            ));
        }
        columns.push(column.clone());
    }
    Ok(columns)
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
    /// `DateTime::<Utc>::MAX`, which no admitted event time can reach.
    #[must_use]
    pub fn end_utc(self) -> DateTime<Utc> {
        self.start_utc
            .checked_add_signed(self.granularity.span())
            .expect("partition end is representable for every admitted event time")
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
    /// Panics when the transform value falls outside Iceberg's `i32` domain,
    /// which is unreachable for any partition boundary the admission window
    /// accepts.
    #[must_use]
    pub fn iceberg_transform_value(self) -> i32 {
        let seconds = self.start_utc.timestamp();
        let divisor = match self.granularity {
            TimeGranularity::Hour => 3_600,
            TimeGranularity::Day => 86_400,
        };
        i32::try_from(seconds.div_euclid(divisor))
            .expect("admitted partition boundaries fit the Iceberg i32 transform domain")
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
    /// boundary falls outside the Iceberg `i32` transform domain, which no
    /// admitted event time can reach.
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
        LayoutSortKey, MANAGED_BLOOM_FLOOR, MAX_SORT_KEYS, NullOrder, PhysicalLayout,
        SortDirection, TimeGranularity,
    };
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use wyrd_spec::vala::api::{
        NullOrderWire, PhysicalLayoutWire, SortDirectionWire, SortKeyWire, TimeGranularityWire,
    };
    use wyrd_spec::vala::managed_columns::{
        CARD_UID, DATA_TENANT_ID, PRINCIPAL_ID, RUN_ID, WYRD_EVENT_TIME,
    };
    use wyrd_spec::vala::{BifrostError, PhysicalLayoutField, PhysicalLayoutViolation};

    /// Canonical fully-qualified name every case in this module resolves under.
    const TABLE: &str = "vala.datasets.layout";

    /// Builds the fixture physical schema: the full managed floor, the tenant
    /// column, and two user columns to declare against.
    fn fixture_schema() -> Schema {
        Schema::new(vec![
            Field::new(
                WYRD_EVENT_TIME,
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
            Field::new(DATA_TENANT_ID, DataType::Utf8, false),
            Field::new(RUN_ID, DataType::Utf8, true),
            Field::new(CARD_UID, DataType::Utf8, true),
            Field::new(PRINCIPAL_ID, DataType::Utf8, true),
            Field::new("customer", DataType::Utf8, true),
            Field::new("region", DataType::Utf8, true),
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

    /// Builds one hourly declaration over the supplied sort and Bloom intent.
    fn declaration(sort_keys: Vec<SortKeyWire>, bloom_columns: Vec<String>) -> PhysicalLayoutWire {
        PhysicalLayoutWire {
            partition_granularity: TimeGranularityWire::Hour,
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

    /// Asserts one declaration is refused with the exact public triple.
    ///
    /// # Panics
    ///
    /// Panics when the declaration resolves, or fails with a different field or
    /// violation than the one named.
    fn assert_refused(
        schema: &Schema,
        declared: &PhysicalLayoutWire,
        expected_field: PhysicalLayoutField,
        expected_violation: PhysicalLayoutViolation,
    ) {
        let error = PhysicalLayout::resolve(TABLE, schema, Some(declared))
            .expect_err("declaration must be refused");
        let BifrostError::InvalidPhysicalLayout {
            field, violation, ..
        } = error
        else {
            panic!("expected an invalid physical layout, got {error:?}");
        };
        assert_eq!(field, expected_field);
        assert_eq!(violation, expected_violation);
    }

    /// The declarations the single resolution entry point accepts.
    ///
    /// Covers an omitted declaration, an explicit-empty declaration,
    /// `wyrd_event_time` declared explicitly, managed columns on both lists,
    /// floor-union dedupe, the absence of `data_tenant_id` from both canonical
    /// lists, and the canonical form's fixed point through the stored wire.
    #[test]
    fn canonical_layout_accepts_declared_shapes() {
        let schema = fixture_schema();

        // An omitted declaration resolves to hourly, one event-time key, and
        // the schema-present managed floor — with no tenant column anywhere.
        let omitted =
            PhysicalLayout::resolve(TABLE, &schema, None).expect("an omitted declaration resolves");
        assert_eq!(omitted.granularity(), TimeGranularity::Hour);
        assert_eq!(sort_columns(&omitted), vec![WYRD_EVENT_TIME.to_owned()]);
        assert!(omitted.sort_keys()[0].is_descending());
        assert!(!omitted.sort_keys()[0].nulls_first());
        assert_eq!(
            omitted.bloom_columns(),
            MANAGED_BLOOM_FLOOR
                .iter()
                .map(|column| (*column).to_owned())
                .collect::<Vec<_>>()
                .as_slice()
        );
        assert!(!omitted.bloom_columns().contains(&DATA_TENANT_ID.to_owned()));
        assert!(!sort_columns(&omitted).contains(&DATA_TENANT_ID.to_owned()));

        // An explicit empty declaration means exactly what omission means,
        // because the system injects nothing of its own.
        let explicit_empty =
            PhysicalLayout::resolve(TABLE, &schema, Some(&declaration(Vec::new(), Vec::new())))
                .expect("an explicit empty declaration resolves");
        assert_eq!(explicit_empty.sort_keys(), omitted.sort_keys());
        assert_eq!(explicit_empty.bloom_columns(), omitted.bloom_columns());

        // Naming wyrd_event_time is legal and replaces the default rather than
        // duplicating or erroring.
        let named = PhysicalLayout::resolve(
            TABLE,
            &schema,
            Some(&declaration(vec![asc(WYRD_EVENT_TIME)], Vec::new())),
        )
        .expect("wyrd_event_time is a legal declared sort key");
        assert_eq!(sort_columns(&named), vec![WYRD_EVENT_TIME.to_owned()]);
        assert_eq!(
            named.sort_keys()[0],
            LayoutSortKey::new(WYRD_EVENT_TIME, SortDirection::Asc, NullOrder::Last),
            "the declaration replaces the descending default"
        );

        // Every other managed column is legal too, including data_tenant_id if
        // a caller insists: nothing is stripped, and nothing is prepended.
        let managed = PhysicalLayout::resolve(
            TABLE,
            &schema,
            Some(&declaration(
                vec![asc(RUN_ID), asc("customer")],
                vec![CARD_UID.to_owned(), "region".to_owned()],
            )),
        )
        .expect("managed columns are legal sort and Bloom columns");
        assert_eq!(
            sort_columns(&managed),
            vec![RUN_ID.to_owned(), "customer".to_owned()]
        );

        // Floor columns dedupe by union rather than erroring, and the declared
        // additions keep their request order behind the floor.
        assert_eq!(
            managed.bloom_columns(),
            [RUN_ID, CARD_UID, PRINCIPAL_ID, "region"]
                .iter()
                .map(|column| (*column).to_owned())
                .collect::<Vec<_>>()
                .as_slice()
        );

        // The canonical form is a fixed point, and the wire it projects carries
        // the granularity directly.
        let stored = managed.to_wire();
        assert_eq!(stored.partition_granularity, TimeGranularityWire::Hour);
        assert_eq!(
            PhysicalLayout::from_stored_wire(TABLE, &schema, &stored)
                .expect("a canonical layout re-resolves to itself")
                .to_wire(),
            stored
        );
    }

    /// The shape and column faults the resolution entry point refuses.
    ///
    /// Covers the sort-key cap winning over a per-key column fault, and unknown
    /// and duplicate columns on both the sort-key and Bloom lists.
    #[test]
    fn canonical_layout_refuses_shape_and_column_faults() {
        let schema = fixture_schema();

        // The cap admits exactly MAX_SORT_KEYS and refuses the next one before
        // any per-key validation, so the shape fault wins over a column fault.
        let four = vec![
            asc(WYRD_EVENT_TIME),
            asc(RUN_ID),
            asc(CARD_UID),
            asc("customer"),
        ];
        assert_eq!(four.len(), MAX_SORT_KEYS);
        assert!(
            PhysicalLayout::resolve(TABLE, &schema, Some(&declaration(four.clone(), Vec::new())))
                .is_ok(),
            "four declared keys are admitted"
        );
        let mut five = four;
        five.push(asc("absent"));
        assert_refused(
            &schema,
            &declaration(five, Vec::new()),
            PhysicalLayoutField::SortKey,
            PhysicalLayoutViolation::TooManyKeys,
        );

        // Unknown and duplicate columns stay refused on both lists.
        assert_refused(
            &schema,
            &declaration(vec![asc("absent")], Vec::new()),
            PhysicalLayoutField::SortKey,
            PhysicalLayoutViolation::UnknownColumn,
        );
        assert_refused(
            &schema,
            &declaration(vec![asc("customer"), asc("customer")], Vec::new()),
            PhysicalLayoutField::SortKey,
            PhysicalLayoutViolation::Duplicate,
        );
        assert_refused(
            &schema,
            &declaration(Vec::new(), vec!["absent".to_owned()]),
            PhysicalLayoutField::BloomColumn,
            PhysicalLayoutViolation::UnknownColumn,
        );
        assert_refused(
            &schema,
            &declaration(
                Vec::new(),
                vec!["customer".to_owned(), "customer".to_owned()],
            ),
            PhysicalLayoutField::BloomColumn,
            PhysicalLayoutViolation::Duplicate,
        );
    }

    /// A schema missing a floor column claims no Bloom it cannot write, and the
    /// floor's stable order survives the filter.
    #[test]
    fn managed_bloom_floor_is_filtered_to_schema_present_columns() {
        let schema = Schema::new(vec![
            Field::new(
                WYRD_EVENT_TIME,
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
            Field::new(DATA_TENANT_ID, DataType::Utf8, false),
            Field::new(PRINCIPAL_ID, DataType::Utf8, true),
        ]);
        let resolved = PhysicalLayout::resolve(TABLE, &schema, None)
            .expect("a narrow schema resolves under the default");
        assert_eq!(resolved.bloom_columns(), [PRINCIPAL_ID.to_owned()]);
    }
}
