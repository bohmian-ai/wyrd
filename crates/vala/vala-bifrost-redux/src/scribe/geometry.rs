//! Independent Scribe geometry controls and per-table contention reserves.
//!
//! Scribe has five geometries that used to be conflated behind two numbers, so
//! moving one silently moved another. Each is now a separate named field with
//! one meaning:
//!
//! 1. **WAL segment** — encoded bytes in one shard's WAL segment before the
//!    shard rotates (`WYRD_MAX_FILE_SIZE_ON_DISK`).
//! 2. **Active shard generation** — Arrow bytes one shard's memtable holds
//!    before the shard rotates (`WYRD_MAX_FILE_SIZE_IN_MEMORY`). This is a
//!    per-shard rotation limit, never a reservation and never an object-size
//!    promise.
//! 3. **Per-`SealKey` early seal** — an optional size or age that rotates one key
//!    *earlier* than its shard would. A key may seal earlier on size, age, or
//!    pressure; it may never seal later.
//! 4. **Parquet row group** — a soft 128 MiB estimated encoded-size target
//!    over parquet-rs row-count defaults, owned by
//!    [`crate::parquet::writer_properties`] and not restated here.
//! 5. **Scribe hot object** — the staging assembly target, the smaller of the
//!    WAL segment size and Forge's rewrite target.
//!
//! The shard count (`WYRD_MEM_TABLE_BUCKET_NUM`) is carried here too because
//! every shard applies the limits above independently.
//!
//! Forge's Iceberg rewrite target is a sixth geometry owned by `forge`, kept
//! deliberately separate from all of the above.
//!
//! On top of the geometries this module owns the contention reserve: the exact
//! per-table [`ContentionReserveVector`] that admission installs before a table
//! may serve, and the startup arithmetic that refuses to boot a pod whose global
//! capacity cannot hold one complete vector in every governed category.

use std::time::Duration;

/// Default number of Scribe shards, each with its own WAL and memtable.
pub const DEFAULT_SHARD_COUNT: usize = 1;
/// Largest shard count the WAL segment header's one-byte shard id can name.
pub const MAX_SHARD_COUNT: usize = 1 << u8::BITS;
/// Default encoded bytes in one shard's WAL segment before the shard rotates.
pub const DEFAULT_WAL_SEGMENT_BYTES: u64 = 512 * 1024 * 1024;
/// Default Arrow bytes one shard's memtable holds before the shard rotates.
pub const DEFAULT_GENERATION_ROTATION_BYTES: u64 = 512 * 1024 * 1024;
/// Default maximum age of an active shard generation before rotation.
pub const DEFAULT_GENERATION_MAX_AGE: Duration = Duration::from_mins(10);
/// Default encoded Parquet target for one assembled Scribe hot object.
///
/// The server derives the real target as the smaller of the WAL segment size
/// and Forge's rewrite target; with both defaults that is this value.
pub const DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES: u64 = 512 * 1024 * 1024;

/// One governed Scribe resource category.
///
/// The set is closed and compile-time enumerable through
/// [`ContentionCategory::ALL`] so admission, startup validation, and telemetry
/// all walk the same categories. Adding a category without extending every one
/// of those walks is a compile error rather than a silently unguarded resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContentionCategory {
    /// In-flight admitted request items.
    AdmissionItems,
    /// In-flight admitted request bytes.
    AdmissionBytes,
    /// Arrow bytes owned by an active, still-writable generation.
    Active,
    /// Arrow bytes owned by a frozen generation awaiting durable staging.
    Immutable,
    /// Local durable-staging bytes owned by fsynced sorted runs and manifests.
    DurableStage,
    /// Scratch bytes owned by one admitted external-merge lane.
    MergeScratch,
    /// Staging-claim items owned while an assembly claim is outstanding.
    StagingClaim,
    /// Upload-claim items owned while a sealed artifact is being published.
    UploadClaim,
}

impl ContentionCategory {
    /// Every governed category, in a stable order.
    ///
    /// Startup validation and telemetry iterate this rather than restating the
    /// list, so a new category cannot be added to one walk and missed by another.
    pub const ALL: [Self; 8] = [
        Self::AdmissionItems,
        Self::AdmissionBytes,
        Self::Active,
        Self::Immutable,
        Self::DurableStage,
        Self::MergeScratch,
        Self::StagingClaim,
        Self::UploadClaim,
    ];

    /// Returns the fixed-cardinality label naming this category in diagnostics.
    ///
    /// The value is a closed literal so it is safe as a metric label; it never
    /// carries tenant, table, batch, or generation identity.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AdmissionItems => "admission_items",
            Self::AdmissionBytes => "admission_bytes",
            Self::Active => "active",
            Self::Immutable => "immutable",
            Self::DurableStage => "durable_stage",
            Self::MergeScratch => "merge_scratch",
            Self::StagingClaim => "staging_claim",
            Self::UploadClaim => "upload_claim",
        }
    }

    /// Reports whether this category is counted in items rather than bytes.
    ///
    /// Item and byte categories are never summed together, so a caller that
    /// aggregates must first ask which unit it holds.
    #[must_use]
    pub const fn is_items(self) -> bool {
        matches!(
            self,
            Self::AdmissionItems | Self::StagingClaim | Self::UploadClaim
        )
    }
}

/// One complete per-table reserve across every governed category.
///
/// A table becomes active by installing this whole vector atomically. Partial
/// installation is what would let a table acknowledge an append it cannot later
/// stage, merge, or publish, so every category is reserved together or none is.
///
/// The vector is the resource quantum for one *actual* canonical table. It is
/// never installed for a hypothetical tenant or table: owners are created lazily
/// from real traffic, and capacity no live owner holds stays borrowable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentionReserveVector {
    /// Admitted in-flight items reserved for one table. Always one.
    pub admission_items: usize,
    /// Admitted in-flight bytes reserved for one maximum ingress envelope.
    pub admission_bytes: usize,
    /// Arrow bytes one table may own in a still-writable generation.
    pub active_bytes: usize,
    /// Arrow bytes one table may own in a frozen generation awaiting staging.
    pub immutable_bytes: usize,
    /// Local durable-staging bytes one table's smallest staged member needs.
    pub durable_stage_bytes: usize,
    /// Scratch bytes one admitted external-merge lane reserves before starting.
    pub merge_scratch_bytes: usize,
    /// Outstanding assembly-claim items reserved for one table. Always one.
    pub staging_claim_items: usize,
    /// Outstanding upload-claim items reserved for one table. Always one.
    pub upload_claim_items: usize,
}

impl ContentionReserveVector {
    /// Returns this vector's component for one category.
    ///
    /// Categories are never summed across units, so the caller reads one
    /// component at a time against the matching global capacity component.
    #[must_use]
    pub const fn component(&self, category: ContentionCategory) -> usize {
        match category {
            ContentionCategory::AdmissionItems => self.admission_items,
            ContentionCategory::AdmissionBytes => self.admission_bytes,
            ContentionCategory::Active => self.active_bytes,
            ContentionCategory::Immutable => self.immutable_bytes,
            ContentionCategory::DurableStage => self.durable_stage_bytes,
            ContentionCategory::MergeScratch => self.merge_scratch_bytes,
            ContentionCategory::StagingClaim => self.staging_claim_items,
            ContentionCategory::UploadClaim => self.upload_claim_items,
        }
    }

    /// Reports the first category whose component is zero, if any.
    ///
    /// A zero component is an incomplete vector: the table would activate
    /// without reserving something it must later own, which is exactly the
    /// partial installation the whole-vector rule exists to prevent.
    #[must_use]
    pub fn first_empty_category(&self) -> Option<ContentionCategory> {
        ContentionCategory::ALL
            .into_iter()
            .find(|category| self.component(*category) == 0)
    }
}

/// Pod-wide capacity available to Scribe in each governed category.
///
/// This is what the node actually has, derived from the detected memory budget,
/// the staging volume, and the configured lane widths. Startup compares it
/// against one complete reserve vector and refuses to serve when any category
/// falls one byte or item short.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScribeGlobalCapacity {
    /// Total admitted in-flight items across the pod.
    pub admission_items: usize,
    /// Total admitted in-flight bytes across the pod.
    pub admission_bytes: usize,
    /// Total Arrow bytes available to active generations across the pod.
    pub active_bytes: usize,
    /// Total Arrow bytes available to frozen generations across the pod.
    pub immutable_bytes: usize,
    /// Total local durable-staging bytes available across the pod.
    pub durable_stage_bytes: usize,
    /// Total merge-scratch bytes available across the pod.
    pub merge_scratch_bytes: usize,
    /// Total outstanding assembly claims available across the pod.
    pub staging_claim_items: usize,
    /// Total outstanding upload claims available across the pod.
    pub upload_claim_items: usize,
}

impl ScribeGlobalCapacity {
    /// Returns pod capacity for one category.
    #[must_use]
    pub const fn component(&self, category: ContentionCategory) -> usize {
        match category {
            ContentionCategory::AdmissionItems => self.admission_items,
            ContentionCategory::AdmissionBytes => self.admission_bytes,
            ContentionCategory::Active => self.active_bytes,
            ContentionCategory::Immutable => self.immutable_bytes,
            ContentionCategory::DurableStage => self.durable_stage_bytes,
            ContentionCategory::MergeScratch => self.merge_scratch_bytes,
            ContentionCategory::StagingClaim => self.staging_claim_items,
            ContentionCategory::UploadClaim => self.upload_claim_items,
        }
    }
}

/// Startup refusal describing exactly which geometry or capacity value failed.
///
/// Every variant names the value an operator must change. A refusal that only
/// said "insufficient memory" would leave an operator guessing which of eight
/// categories and two widths to raise.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScribeGeometryError {
    /// A geometry field that must be positive was configured as zero.
    #[error("Scribe geometry `{field}` must be greater than zero")]
    Zero {
        /// Configuration field name as an operator would write it.
        field: &'static str,
    },
    /// A derived geometry value overflowed or divided to zero.
    #[error("Scribe geometry `{field}` is incoherent: {detail}")]
    Incoherent {
        /// Configuration field name as an operator would write it.
        field: &'static str,
        /// What the arithmetic could not produce.
        detail: String,
    },
    /// Pod capacity cannot hold even one complete lifecycle vector.
    #[error(
        "Scribe capacity for `{category}` holds {actual} but one complete lifecycle vector requires {required}"
    )]
    Capacity {
        /// Fixed-cardinality category label.
        category: &'static str,
        /// Amount one complete lifecycle vector requires in this category.
        required: usize,
        /// Total the pod actually has in this category.
        actual: usize,
    },
    /// A lifecycle vector component is zero, so a table could activate
    /// without reserving something it must later own.
    #[error("Scribe reserve vector component `{category}` must be greater than zero")]
    EmptyReserve {
        /// Fixed-cardinality category label.
        category: &'static str,
    },
}

/// Independent, validated Scribe geometry and contention widths.
///
/// Construct through [`ScribeGeometry::new`], which is the only place the fields
/// are checked; a caller cannot assemble an unvalidated geometry and reach the
/// derived rotation limits or reserve vector with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScribeGeometry {
    /// Number of shards, each with its own WAL stream and memtable.
    shard_count: usize,
    /// Encoded bytes in one WAL segment before rotation.
    wal_segment_bytes: u64,
    /// Arrow bytes one shard's memtable holds before the shard rotates.
    generation_rotation_bytes: u64,
    /// Maximum age of an active shard generation before rotation.
    generation_max_age: Duration,
    /// Optional per-`SealKey` size that rotates one key earlier than its shard.
    seal_key_early_seal_bytes: Option<usize>,
    /// Optional per-`SealKey` age that rotates one key earlier than its shard.
    seal_key_max_age: Option<Duration>,
    /// Encoded Parquet target for one assembled hot object.
    staging_target_file_size_bytes: u64,
    /// Maximum encoded bytes accepted for one ingress request.
    maximum_ingress_envelope_bytes: usize,
    /// Arrow bytes one table may own in a still-writable generation.
    maximum_active_request_ownership_bytes: usize,
    /// Arrow bytes one table may own in a frozen generation.
    maximum_immutable_member_ownership_bytes: usize,
    /// Local durable-staging bytes one table's smallest staged member needs.
    minimum_stage_member_bytes: usize,
    /// Scratch bytes one admitted merge lane reserves before starting.
    minimum_merge_lane_scratch_bytes: usize,
}

impl ScribeGeometry {
    /// Validates and constructs one complete geometry.
    ///
    /// Validation is total: every positive-valued field is checked, the shard
    /// count must fit the WAL header, and an early-seal size may not exceed the
    /// per-shard limit it is supposed to precede. A geometry that passes here
    /// cannot later produce a zero rotation limit or an early seal that fires
    /// after the shard would already have rotated.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeGeometryError::Zero`] for a zero-valued required field,
    /// and [`ScribeGeometryError::Incoherent`] when the shard count exceeds
    /// [`MAX_SHARD_COUNT`] or an early-seal control cannot fire before its
    /// shard rotates.
    #[expect(
        clippy::too_many_arguments,
        reason = "one validated value object over twelve independent geometry knobs; \
                  splitting the constructor would let a caller build a half-checked geometry"
    )]
    pub fn new(
        shard_count: usize,
        wal_segment_bytes: u64,
        generation_rotation_bytes: u64,
        generation_max_age: Duration,
        seal_key_early_seal_bytes: Option<usize>,
        seal_key_max_age: Option<Duration>,
        staging_target_file_size_bytes: u64,
        maximum_ingress_envelope_bytes: usize,
        maximum_active_request_ownership_bytes: usize,
        maximum_immutable_member_ownership_bytes: usize,
        minimum_stage_member_bytes: usize,
        minimum_merge_lane_scratch_bytes: usize,
    ) -> Result<Self, ScribeGeometryError> {
        let geometry = Self {
            shard_count,
            wal_segment_bytes,
            generation_rotation_bytes,
            generation_max_age,
            seal_key_early_seal_bytes,
            seal_key_max_age,
            staging_target_file_size_bytes,
            maximum_ingress_envelope_bytes,
            maximum_active_request_ownership_bytes,
            maximum_immutable_member_ownership_bytes,
            minimum_stage_member_bytes,
            minimum_merge_lane_scratch_bytes,
        };
        geometry.check_positive()?;
        geometry.check_derived()?;
        Ok(geometry)
    }

    /// Refuses any geometry field that must be positive but was left at zero.
    ///
    /// A zero here is never a "disabled" control: it would make a rotation
    /// limit or an ownership bound vanish, so each is rejected by name rather
    /// than defaulted.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeGeometryError::Zero`] naming the first zero-valued
    /// field.
    fn check_positive(&self) -> Result<(), ScribeGeometryError> {
        let byte_fields = [
            ("wal_segment_bytes", self.wal_segment_bytes),
            ("generation_rotation_bytes", self.generation_rotation_bytes),
            (
                "staging_target_file_size_bytes",
                self.staging_target_file_size_bytes,
            ),
        ];
        if let Some((field, _)) = byte_fields.into_iter().find(|(_, value)| *value == 0) {
            return Err(ScribeGeometryError::Zero { field });
        }
        let width_fields = [
            ("shard_count", self.shard_count),
            (
                "maximum_ingress_envelope_bytes",
                self.maximum_ingress_envelope_bytes,
            ),
            (
                "maximum_active_request_ownership_bytes",
                self.maximum_active_request_ownership_bytes,
            ),
            (
                "maximum_immutable_member_ownership_bytes",
                self.maximum_immutable_member_ownership_bytes,
            ),
            (
                "minimum_stage_member_bytes",
                self.minimum_stage_member_bytes,
            ),
            (
                "minimum_merge_lane_scratch_bytes",
                self.minimum_merge_lane_scratch_bytes,
            ),
        ];
        if let Some((field, _)) = width_fields.into_iter().find(|(_, value)| *value == 0) {
            return Err(ScribeGeometryError::Zero { field });
        }
        if self.generation_max_age.is_zero() {
            return Err(ScribeGeometryError::Zero {
                field: "generation_max_age",
            });
        }
        Ok(())
    }

    /// Refuses a geometry whose shard count or per-key controls cannot be served.
    ///
    /// Runs after [`Self::check_positive`] so it can rely on every input being
    /// positive and only has to judge the shard-count bound and the optional
    /// per-key controls.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeGeometryError::Incoherent`] when the shard count exceeds
    /// what a WAL segment header can name, or when a per-`SealKey` control
    /// would fire after its shard has already rotated, and
    /// [`ScribeGeometryError::Zero`] for a zero early-seal size.
    fn check_derived(&self) -> Result<(), ScribeGeometryError> {
        if self.shard_count > MAX_SHARD_COUNT {
            return Err(ScribeGeometryError::Incoherent {
                field: "shard_count",
                detail: format!(
                    "{} exceeds the {MAX_SHARD_COUNT} shards a WAL segment header can name",
                    self.shard_count
                ),
            });
        }
        let shard_limit = self.generation_rotation_bytes;
        if let Some(early) = self.seal_key_early_seal_bytes {
            if early == 0 {
                return Err(ScribeGeometryError::Zero {
                    field: "seal_key_early_seal_bytes",
                });
            }
            if u64::try_from(early).unwrap_or(u64::MAX) > shard_limit {
                return Err(ScribeGeometryError::Incoherent {
                    field: "seal_key_early_seal_bytes",
                    detail: format!(
                        "a key may only seal earlier than its shard, but {early} is above the \
                         derived per-shard rotation limit of {shard_limit}"
                    ),
                });
            }
        }
        if self
            .seal_key_max_age
            .is_some_and(|age| age.is_zero() || age > self.generation_max_age)
        {
            return Err(ScribeGeometryError::Incoherent {
                field: "seal_key_max_age",
                detail: "a key may only seal earlier than its shard, never later".to_owned(),
            });
        }
        Ok(())
    }

    /// Builds a default-shard-count geometry whose shards rotate at `shard_bytes`.
    ///
    /// Embedded and test callers name the WAL segment, memtable, and age
    /// limits directly. Every other field takes its production default, so an
    /// embedded Scribe is not silently running a second contention policy;
    /// [`Self::with_shard_count`] widens the shard topology.
    ///
    /// # Errors
    ///
    /// Returns the [`ScribeGeometryError`] naming the field that is zero or
    /// that cannot produce a coherent per-shard rotation limit.
    pub fn for_uniform_shard_rotation(
        wal_segment_bytes: u64,
        shard_bytes: usize,
        generation_max_age: Duration,
    ) -> Result<Self, ScribeGeometryError> {
        Self::new(
            DEFAULT_SHARD_COUNT,
            wal_segment_bytes,
            u64::try_from(shard_bytes).unwrap_or(u64::MAX),
            generation_max_age,
            None,
            None,
            DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES,
            crate::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES,
            DEFAULT_MAXIMUM_ACTIVE_REQUEST_OWNERSHIP_BYTES,
            DEFAULT_MAXIMUM_IMMUTABLE_MEMBER_OWNERSHIP_BYTES,
            DEFAULT_MINIMUM_STAGE_MEMBER_BYTES,
            DEFAULT_MINIMUM_MERGE_LANE_SCRATCH_BYTES,
        )
    }

    /// Returns this geometry with a different assembled-object target.
    ///
    /// The target is the one geometry control a scaled production test needs to
    /// move: proving target roll plus residue at 512 MiB would mean writing
    /// half a gigabyte per case, while every other control must stay exactly
    /// what production uses for the proof to mean anything. Revalidating keeps
    /// a scaled target from producing a geometry `new` would have refused.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeGeometryError::Zero`] when the target is zero.
    pub fn with_staging_target_file_size_bytes(
        self,
        staging_target_file_size_bytes: u64,
    ) -> Result<Self, ScribeGeometryError> {
        Self {
            staging_target_file_size_bytes,
            ..self
        }
        .validated()
    }

    /// Returns this geometry with a different shard count.
    ///
    /// Multi-lane tests need several shards while every limit stays exactly
    /// what they configured; revalidating keeps the count inside what a WAL
    /// segment header can name.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeGeometryError::Zero`] for zero shards and
    /// [`ScribeGeometryError::Incoherent`] above [`MAX_SHARD_COUNT`].
    pub fn with_shard_count(self, shard_count: usize) -> Result<Self, ScribeGeometryError> {
        Self {
            shard_count,
            ..self
        }
        .validated()
    }

    /// Runs the constructor's complete validation over an adjusted copy.
    ///
    /// # Errors
    ///
    /// Returns whatever [`Self::new`] would refuse for the same fields.
    fn validated(self) -> Result<Self, ScribeGeometryError> {
        self.check_positive()?;
        self.check_derived()?;
        Ok(self)
    }

    /// Returns the per-shard rotation limit as a `usize` byte count.
    ///
    /// Shard owners compare against in-memory sizes, so they need the limit in
    /// the same width. A limit above `usize::MAX` saturates rather than
    /// wrapping, which on a 64-bit target is unreachable.
    #[must_use]
    pub fn shard_generation_rotation_usize(&self) -> usize {
        usize::try_from(self.shard_generation_rotation_bytes()).unwrap_or(usize::MAX)
    }

    /// Returns the memtable rotation limit every shard applies independently.
    ///
    /// It is a *limit*, not a reservation, and a shard that rotates at this
    /// size makes no promise about the size of any object later assembled from
    /// what it froze.
    #[must_use]
    pub const fn shard_generation_rotation_bytes(&self) -> u64 {
        self.generation_rotation_bytes
    }

    /// Returns the number of shards, each with its own WAL stream and memtable.
    #[must_use]
    pub const fn shard_count(&self) -> usize {
        self.shard_count
    }

    /// Returns the encoded WAL segment rotation target.
    #[must_use]
    pub const fn wal_segment_bytes(&self) -> u64 {
        self.wal_segment_bytes
    }

    /// Returns the maximum age of an active shard generation.
    #[must_use]
    pub const fn generation_max_age(&self) -> Duration {
        self.generation_max_age
    }

    /// Returns the optional per-`SealKey` early-seal size.
    #[must_use]
    pub const fn seal_key_early_seal_bytes(&self) -> Option<usize> {
        self.seal_key_early_seal_bytes
    }

    /// Returns the optional per-`SealKey` early-seal age.
    #[must_use]
    pub const fn seal_key_max_age(&self) -> Option<Duration> {
        self.seal_key_max_age
    }

    /// Returns the encoded Parquet target for one assembled hot object.
    #[must_use]
    pub const fn staging_target_file_size_bytes(&self) -> u64 {
        self.staging_target_file_size_bytes
    }
}

impl Default for ScribeGeometry {
    /// Returns the production default geometry.
    ///
    /// The defaults are one shard, 512 MiB on disk, 512 MiB in
    /// memory, ten minutes of age, and a 512 MiB hot-object target.
    ///
    /// # Panics
    ///
    /// Panics only if the defaults in this module contradict
    /// [`ScribeGeometry::new`]'s own validation, which is a bug in this file
    /// rather than an operator input.
    fn default() -> Self {
        Self::new(
            DEFAULT_SHARD_COUNT,
            DEFAULT_WAL_SEGMENT_BYTES,
            DEFAULT_GENERATION_ROTATION_BYTES,
            DEFAULT_GENERATION_MAX_AGE,
            None,
            None,
            DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES,
            crate::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES,
            DEFAULT_MAXIMUM_ACTIVE_REQUEST_OWNERSHIP_BYTES,
            DEFAULT_MAXIMUM_IMMUTABLE_MEMBER_OWNERSHIP_BYTES,
            DEFAULT_MINIMUM_STAGE_MEMBER_BYTES,
            DEFAULT_MINIMUM_MERGE_LANE_SCRATCH_BYTES,
        )
        .expect("the module's own default geometry satisfies its own validation")
    }
}

/// Default Arrow bytes one table may own in a still-writable generation.
pub const DEFAULT_MAXIMUM_ACTIVE_REQUEST_OWNERSHIP_BYTES: usize = 64 * 1024 * 1024;
/// Default Arrow bytes one table may own in a frozen generation.
pub const DEFAULT_MAXIMUM_IMMUTABLE_MEMBER_OWNERSHIP_BYTES: usize = 64 * 1024 * 1024;
/// Default local durable-staging bytes one table's smallest staged member needs.
pub const DEFAULT_MINIMUM_STAGE_MEMBER_BYTES: usize = 32 * 1024 * 1024;
/// Default scratch bytes one admitted merge lane reserves before starting.
///
/// A lane holds one cursor batch per run, one 32 MiB output builder, and the
/// sealed-artifact plus footer/upload workspace, so its floor is several times
/// the row-group size rather than equal to it.
pub const DEFAULT_MINIMUM_MERGE_LANE_SCRATCH_BYTES: usize = 128 * 1024 * 1024;

/// Returns `total * weight / sum`, the part of one memory ceiling a category owns.
///
/// The multiplication runs in `u128` because a large ceiling times a category
/// weight overflows `usize` on a 64-bit target long before either input is
/// unreasonable. The quotient is never larger than `total`, so narrowing back to
/// `usize` cannot truncate. A zero `sum` yields zero, which
/// [`ScribeArtifactPolicy::validate_capacity`] then refuses as an empty reserve
/// rather than dividing by it here.
const fn weighted_share(total: usize, weight: usize, sum: usize) -> usize {
    if sum == 0 {
        return 0;
    }
    ((total as u128) * (weight as u128) / (sum as u128)) as usize
}

/// Derives per-table contention reserves and validates pod startup capacity.
///
/// The policy owns one validated [`ScribeGeometry`] and is the single place the
/// reserve vector is computed. Admission, startup, and telemetry all read it
/// from here rather than restating the components, so the vector installed at
/// activation is by construction the vector startup proved the pod can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScribeArtifactPolicy {
    /// The validated geometry every derived value comes from.
    geometry: ScribeGeometry,
}

impl ScribeArtifactPolicy {
    /// Wraps one already-validated geometry.
    #[must_use]
    pub const fn new(geometry: ScribeGeometry) -> Self {
        Self { geometry }
    }

    /// Borrows the geometry this policy derives from.
    #[must_use]
    pub const fn geometry(&self) -> &ScribeGeometry {
        &self.geometry
    }

    /// Returns the exact complete reserve vector one table installs to activate.
    ///
    /// Item categories are always one: a table reserves the right to have one
    /// request in flight, one outstanding assembly claim, and one outstanding
    /// upload claim. Byte categories come straight from the geometry's declared
    /// per-table maxima and minima. Nothing here is scaled by a tenant count or
    /// a table count: the vector is the resource quantum one actual canonical
    /// table needs to travel admission, active, immutable, durable staging,
    /// merge scratch, staging claim, upload claim, publication and release.
    #[must_use]
    pub const fn reserve_vector(&self) -> ContentionReserveVector {
        ContentionReserveVector {
            admission_items: 1,
            admission_bytes: self.geometry.maximum_ingress_envelope_bytes,
            active_bytes: self.geometry.maximum_active_request_ownership_bytes,
            immutable_bytes: self.geometry.maximum_immutable_member_ownership_bytes,
            durable_stage_bytes: self.geometry.minimum_stage_member_bytes,
            merge_scratch_bytes: self.geometry.minimum_merge_lane_scratch_bytes,
            staging_claim_items: 1,
            upload_claim_items: 1,
        }
    }

    /// Returns what one category must hold to admit one more canonical table.
    ///
    /// This is exactly the reserve vector's component for that category. There
    /// is no tenant or table multiplier: capacity is a measured resource, and
    /// how many tables it supports is derived from it by
    /// [`Self::max_active_tables`] rather than configured ahead of traffic.
    #[must_use]
    pub const fn required_for(&self, category: ContentionCategory) -> usize {
        self.reserve_vector().component(category)
    }

    /// Refuses to serve unless every category holds one complete vector.
    ///
    /// Each category is checked independently against its own capacity
    /// component: byte and item categories are never summed, and a surplus in
    /// one category never covers a shortfall in another. A pod one byte or one
    /// item below any component fails here, before serving, naming the
    /// category, the required total and the actual total.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeGeometryError::EmptyReserve`] when a vector component is
    /// zero, and [`ScribeGeometryError::Capacity`] naming the first category the
    /// pod cannot hold one complete vector in.
    pub fn validate_capacity(
        &self,
        capacity: &ScribeGlobalCapacity,
    ) -> Result<(), ScribeGeometryError> {
        let vector = self.reserve_vector();
        if let Some(category) = vector.first_empty_category() {
            return Err(ScribeGeometryError::EmptyReserve {
                category: category.label(),
            });
        }
        for category in ContentionCategory::ALL {
            let required = vector.component(category);
            let actual = capacity.component(category);
            if actual < required {
                return Err(ScribeGeometryError::Capacity {
                    category: category.label(),
                    required,
                    actual,
                });
            }
        }
        Ok(())
    }

    /// Returns how many canonical tables this pod can own simultaneously.
    ///
    /// Every category independently supports `capacity / component` complete
    /// vectors, and a table needs one vector in *every* category, so the pod's
    /// ownership ceiling is the minimum across categories. This is the only
    /// place a table count comes from, and it is derived from measured
    /// resources rather than configured.
    ///
    /// A zero component yields a zero ceiling rather than an unbounded one:
    /// [`Self::validate_capacity`] already refuses such a policy before serving,
    /// and reporting "no tables" is the safe reading if one ever reached here.
    #[must_use]
    pub fn max_active_tables(&self, capacity: &ScribeGlobalCapacity) -> usize {
        let vector = self.reserve_vector();
        ContentionCategory::ALL
            .into_iter()
            .map(|category| {
                capacity
                    .component(category)
                    .checked_div(vector.component(category))
                    .unwrap_or(0)
            })
            .min()
            .unwrap_or(0)
    }

    /// Returns the smallest Scribe memory ceiling that can complete one table.
    ///
    /// The four memory-backed categories share one ceiling, so a pod can carry
    /// a table only once that ceiling covers all four of its reserve components
    /// at once. [`Self::pod_capacity`] splits by these same weights, which makes
    /// this value the exact point at which every memory category holds one
    /// complete vector: one byte less and the first category truncates below its
    /// component.
    #[must_use]
    pub const fn minimum_scribe_memory_bytes(&self) -> usize {
        let vector = self.reserve_vector();
        vector.admission_bytes
            + vector.active_bytes
            + vector.immutable_bytes
            + vector.merge_scratch_bytes
    }

    /// Derives the pod capacity this policy is checked against.
    ///
    /// The three inputs are the only ones the node actually measures: the Scribe
    /// memory ceiling the resource governor granted, the local staging volume,
    /// and the pod-global in-flight item ceiling. Everything else is a split of
    /// those, and each split is named rather than implied:
    ///
    /// - Admitted request bytes, active generations, immutable generations, and
    ///   merge scratch all come out of the one memory ceiling, so each gets the
    ///   share of it that its own reserve component is of the four components
    ///   summed. The weights are the geometry's declared per-table needs rather
    ///   than a guess about which phase dominates, which is what makes the
    ///   smallest bootable Scribe memory ceiling equal to one table's total
    ///   memory need instead of four times its largest single phase.
    /// - Durable staging comes from the staging volume, which is disk and shares
    ///   nothing with memory.
    /// - Claim items are counted, not sized. An outstanding staging or upload
    ///   claim is one queued unit of persistence work, so both categories are
    ///   bounded by the same pod-global in-flight item ceiling the admission
    ///   category uses. Lane width governs how fast those claims drain, not how
    ///   many may be outstanding, and the lane's own workspace is the separately
    ///   governed merge-scratch category.
    #[must_use]
    pub const fn pod_capacity(
        &self,
        scribe_memory_bytes: usize,
        staging_volume_bytes: usize,
        global_inflight_items: usize,
    ) -> ScribeGlobalCapacity {
        let vector = self.reserve_vector();
        let need = self.minimum_scribe_memory_bytes();
        ScribeGlobalCapacity {
            admission_items: global_inflight_items,
            admission_bytes: weighted_share(scribe_memory_bytes, vector.admission_bytes, need),
            active_bytes: weighted_share(scribe_memory_bytes, vector.active_bytes, need),
            immutable_bytes: weighted_share(scribe_memory_bytes, vector.immutable_bytes, need),
            durable_stage_bytes: staging_volume_bytes,
            merge_scratch_bytes: weighted_share(
                scribe_memory_bytes,
                vector.merge_scratch_bytes,
                need,
            ),
            staging_claim_items: global_inflight_items,
            upload_claim_items: global_inflight_items,
        }
    }

    /// Returns the smallest pod capacity that satisfies this policy exactly.
    ///
    /// That is one complete lifecycle vector: a capacity built from this
    /// function passes [`Self::validate_capacity`] and owns exactly one table,
    /// and lowering any single component by one makes it fail.
    #[must_use]
    pub const fn minimum_capacity(&self) -> ScribeGlobalCapacity {
        let vector = self.reserve_vector();
        ScribeGlobalCapacity {
            admission_items: vector.admission_items,
            admission_bytes: vector.admission_bytes,
            active_bytes: vector.active_bytes,
            immutable_bytes: vector.immutable_bytes,
            durable_stage_bytes: vector.durable_stage_bytes,
            merge_scratch_bytes: vector.merge_scratch_bytes,
            staging_claim_items: vector.staging_claim_items,
            upload_claim_items: vector.upload_claim_items,
        }
    }
}

impl Default for ScribeArtifactPolicy {
    /// Returns the policy over the production default geometry.
    fn default() -> Self {
        Self::new(ScribeGeometry::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shard count is bounded by what a WAL segment header can name.
    ///
    /// # Panics
    ///
    /// Panics when the default is not one shard, when the header bound is
    /// refused, or when one shard past it is accepted.
    #[test]
    fn shard_count_is_bounded_by_the_wal_header() {
        assert_eq!(ScribeGeometry::default().shard_count(), DEFAULT_SHARD_COUNT);
        let widest = ScribeGeometry::default()
            .with_shard_count(MAX_SHARD_COUNT)
            .expect("every shard id a WAL header can name is valid");
        assert_eq!(widest.shard_count(), MAX_SHARD_COUNT);
        assert!(matches!(
            ScribeGeometry::default()
                .with_shard_count(MAX_SHARD_COUNT + 1)
                .expect_err("a shard id the WAL header cannot name must refuse"),
            ScribeGeometryError::Incoherent {
                field: "shard_count",
                ..
            }
        ));
    }

    /// Every geometry field with the production default in every other slot.
    ///
    /// Independence and zero-refusal are both properties of *one* field moving
    /// while the rest hold their defaults, so both halves of the owner build
    /// their inputs here rather than restating twelve arguments per case.
    #[derive(Debug, Clone, Copy)]
    struct GeometryOverride {
        /// Number of shards, each with its own WAL stream and memtable.
        shard_count: usize,
        /// Encoded bytes in one WAL segment before rotation.
        wal_segment_bytes: u64,
        /// Arrow bytes one shard's memtable holds before the shard rotates.
        generation_rotation_bytes: u64,
        /// Maximum age of an active shard generation before rotation.
        generation_max_age: Duration,
        /// Optional per-`SealKey` early-seal size.
        seal_key_early_seal_bytes: Option<usize>,
        /// Optional per-`SealKey` early-seal age.
        seal_key_max_age: Option<Duration>,
        /// Encoded Parquet target for one assembled hot object.
        staging_target_file_size_bytes: u64,
        /// Maximum encoded bytes accepted for one ingress request.
        maximum_ingress_envelope_bytes: usize,
        /// Arrow bytes one table may own in a still-writable generation.
        maximum_active_request_ownership_bytes: usize,
        /// Arrow bytes one table may own in a frozen generation.
        maximum_immutable_member_ownership_bytes: usize,
        /// Local durable-staging bytes one table's smallest staged member needs.
        minimum_stage_member_bytes: usize,
        /// Scratch bytes one admitted merge lane reserves before starting.
        minimum_merge_lane_scratch_bytes: usize,
    }

    impl GeometryOverride {
        /// Returns every field at its production default.
        fn defaults() -> Self {
            Self {
                shard_count: DEFAULT_SHARD_COUNT,
                wal_segment_bytes: DEFAULT_WAL_SEGMENT_BYTES,
                generation_rotation_bytes: DEFAULT_GENERATION_ROTATION_BYTES,
                generation_max_age: DEFAULT_GENERATION_MAX_AGE,
                seal_key_early_seal_bytes: None,
                seal_key_max_age: None,
                staging_target_file_size_bytes: DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES,
                maximum_ingress_envelope_bytes:
                    crate::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES,
                maximum_active_request_ownership_bytes:
                    DEFAULT_MAXIMUM_ACTIVE_REQUEST_OWNERSHIP_BYTES,
                maximum_immutable_member_ownership_bytes:
                    DEFAULT_MAXIMUM_IMMUTABLE_MEMBER_OWNERSHIP_BYTES,
                minimum_stage_member_bytes: DEFAULT_MINIMUM_STAGE_MEMBER_BYTES,
                minimum_merge_lane_scratch_bytes: DEFAULT_MINIMUM_MERGE_LANE_SCRATCH_BYTES,
            }
        }

        /// Runs this override through the production constructor.
        ///
        /// # Errors
        ///
        /// Returns whatever [`ScribeGeometry::new`] refuses, which is the
        /// outcome every negative case asserts on.
        fn build(self) -> Result<ScribeGeometry, ScribeGeometryError> {
            ScribeGeometry::new(
                self.shard_count,
                self.wal_segment_bytes,
                self.generation_rotation_bytes,
                self.generation_max_age,
                self.seal_key_early_seal_bytes,
                self.seal_key_max_age,
                self.staging_target_file_size_bytes,
                self.maximum_ingress_envelope_bytes,
                self.maximum_active_request_ownership_bytes,
                self.maximum_immutable_member_ownership_bytes,
                self.minimum_stage_member_bytes,
                self.minimum_merge_lane_scratch_bytes,
            )
        }
    }

    /// The five geometries move independently and each is bounded on its own.
    ///
    /// Independence means one knob moving leaves every other derived value
    /// exactly where it was: a WAL segment is not a rotation limit, a rotation
    /// limit is not an object-size promise, and an object target is neither.
    /// Bounded means *every* required field is refused at zero by its own name,
    /// not only the two the derivation happens to divide, and that both
    /// per-`SealKey` controls may only fire earlier than the shard they precede.
    /// The zero half walks the complete field inventory so a validator that
    /// stopped checking one field fails here rather than booting a pod whose
    /// ownership bound silently vanished.
    ///
    /// # Panics
    ///
    /// Panics when changing one geometry moves another, when any required field
    /// is accepted at zero or is refused under another field's name, or when a
    /// per-key control is allowed to fire after its shard would already have
    /// rotated.
    #[test]
    fn scribe_geometry_controls_are_independent_and_bounded() {
        let base = ScribeGeometry::default();

        // Moving the WAL segment leaves the generation and object targets alone.
        let mut moved = GeometryOverride::defaults();
        moved.wal_segment_bytes = 64 * 1024 * 1024;
        let wal_moved = moved.build().expect("a smaller WAL segment is valid alone");
        assert_eq!(wal_moved.wal_segment_bytes(), 64 * 1024 * 1024);
        assert_eq!(
            wal_moved.shard_generation_rotation_bytes(),
            base.shard_generation_rotation_bytes()
        );
        assert_eq!(
            wal_moved.staging_target_file_size_bytes(),
            base.staging_target_file_size_bytes()
        );

        // Moving the memtable limit leaves the object target alone: a shard
        // rotation limit is not an object-size promise.
        let mut moved = GeometryOverride::defaults();
        moved.generation_rotation_bytes = 64 * 1024 * 1024;
        let generation_moved = moved
            .build()
            .expect("a smaller memtable limit is valid alone");
        assert_ne!(
            generation_moved.shard_generation_rotation_bytes(),
            base.shard_generation_rotation_bytes()
        );
        assert_eq!(
            generation_moved.staging_target_file_size_bytes(),
            DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES
        );
        assert_eq!(
            generation_moved.wal_segment_bytes(),
            base.wal_segment_bytes()
        );

        // Moving the object target moves neither the WAL segment nor the shard
        // rotation limit: the assembled object is the fifth, separate geometry.
        let target_moved = base
            .with_staging_target_file_size_bytes(64 * 1024 * 1024)
            .expect("a smaller object target is valid alone");
        assert_eq!(
            target_moved.staging_target_file_size_bytes(),
            64 * 1024 * 1024
        );
        assert_eq!(target_moved.wal_segment_bytes(), base.wal_segment_bytes());
        assert_eq!(
            target_moved.shard_generation_rotation_bytes(),
            base.shard_generation_rotation_bytes()
        );
        assert_eq!(target_moved.generation_max_age(), base.generation_max_age());

        assert_every_required_field_is_refused_at_zero();
        assert_per_key_controls_may_only_fire_earlier();
    }

    /// One zeroed field and its expected refusal name.
    ///
    /// Named because the inventory is an array of function pointers: spelling
    /// the pair out once keeps the walk readable and satisfies the workspace
    /// bar on inline type complexity.
    type ZeroCase = (&'static str, fn(&mut GeometryOverride));

    /// Asserts every required geometry field is refused at zero, by its own name.
    ///
    /// Walking the complete inventory is what makes the bound a property of the
    /// validator rather than of the two fields a derivation happens to divide:
    /// a validator that stopped checking one field fails here instead of
    /// booting a pod whose ownership bound silently vanished.
    ///
    /// # Panics
    ///
    /// Panics when any required field is accepted at zero or is refused under
    /// another field's name.
    fn assert_every_required_field_is_refused_at_zero() {
        // Bounded: every required field is refused at zero, under its own name.
        // Each case zeroes exactly one field and leaves the rest at production
        // defaults, so a refusal proves that field is checked rather than that
        // some other field happened to fail first.
        let zero_cases: [ZeroCase; 9] = [
            ("shard_count", |over| over.shard_count = 0),
            ("wal_segment_bytes", |over| over.wal_segment_bytes = 0),
            ("generation_rotation_bytes", |over| {
                over.generation_rotation_bytes = 0;
            }),
            ("staging_target_file_size_bytes", |over| {
                over.staging_target_file_size_bytes = 0;
            }),
            ("maximum_ingress_envelope_bytes", |over| {
                over.maximum_ingress_envelope_bytes = 0;
            }),
            ("maximum_active_request_ownership_bytes", |over| {
                over.maximum_active_request_ownership_bytes = 0;
            }),
            ("maximum_immutable_member_ownership_bytes", |over| {
                over.maximum_immutable_member_ownership_bytes = 0;
            }),
            ("minimum_stage_member_bytes", |over| {
                over.minimum_stage_member_bytes = 0;
            }),
            ("minimum_merge_lane_scratch_bytes", |over| {
                over.minimum_merge_lane_scratch_bytes = 0;
            }),
        ];
        for (field, zero) in zero_cases {
            let mut over = GeometryOverride::defaults();
            zero(&mut over);
            assert_eq!(
                over.build()
                    .expect_err("a zero required field must refuse by name"),
                ScribeGeometryError::Zero { field },
                "zeroing {field} must be refused under its own name"
            );
        }

        // A zero generation age is a vanished rotation control, not a disabled
        // one, so it is refused by name alongside the byte-valued fields.
        let mut ageless = GeometryOverride::defaults();
        ageless.generation_max_age = Duration::ZERO;
        assert_eq!(
            ageless
                .build()
                .expect_err("a zero generation age must refuse"),
            ScribeGeometryError::Zero {
                field: "generation_max_age"
            }
        );
    }

    /// Asserts each per-`SealKey` control may only fire earlier than its shard.
    ///
    /// The per-key controls exist to seal a hot key ahead of the shard that
    /// contains it. A control permitted to fire *after* its shard would already
    /// have rotated is not an early seal at all, and one permitted at zero
    /// would rotate every key on its first row, so both directions are refused
    /// and the coherent interior is proven to be accepted.
    ///
    /// # Panics
    ///
    /// Panics when a late or zero per-key control is admitted, or when a
    /// control strictly inside its shard's own limit is refused.
    fn assert_per_key_controls_may_only_fire_earlier() {
        // A key may seal earlier than its shard, never later — on size.
        let ceiling = usize::try_from(DEFAULT_GENERATION_ROTATION_BYTES).expect("ceiling fits");
        let mut late_size = GeometryOverride::defaults();
        late_size.seal_key_early_seal_bytes = Some(ceiling + 1);
        assert!(matches!(
            late_size.build().expect_err("a late size seal must refuse"),
            ScribeGeometryError::Incoherent {
                field: "seal_key_early_seal_bytes",
                ..
            }
        ));
        // Exactly at the shard limit is the boundary that still fires with it.
        let mut at_limit = GeometryOverride::defaults();
        at_limit.seal_key_early_seal_bytes = Some(ceiling);
        assert_eq!(
            at_limit
                .build()
                .expect("a seal exactly at the shard limit is coherent")
                .seal_key_early_seal_bytes(),
            Some(ceiling)
        );
        // A zero early seal would rotate every key on its first row.
        let mut zero_size = GeometryOverride::defaults();
        zero_size.seal_key_early_seal_bytes = Some(0);
        assert_eq!(
            zero_size
                .build()
                .expect_err("a zero early seal must refuse"),
            ScribeGeometryError::Zero {
                field: "seal_key_early_seal_bytes"
            }
        );

        // And never later on age either, in both incoherent directions.
        for late_age in [
            Duration::ZERO,
            DEFAULT_GENERATION_MAX_AGE + Duration::from_secs(1),
        ] {
            let mut over = GeometryOverride::defaults();
            over.seal_key_max_age = Some(late_age);
            assert!(
                matches!(
                    over.build()
                        .expect_err("a key may only seal earlier than its shard"),
                    ScribeGeometryError::Incoherent {
                        field: "seal_key_max_age",
                        ..
                    }
                ),
                "a per-key age of {late_age:?} must be refused"
            );
        }
        // An age strictly inside the shard's own is the control's whole point.
        let mut early_age = GeometryOverride::defaults();
        early_age.seal_key_max_age = Some(DEFAULT_GENERATION_MAX_AGE / 2);
        assert_eq!(
            early_age
                .build()
                .expect("a key sealing earlier than its shard is coherent")
                .seal_key_max_age(),
            Some(DEFAULT_GENERATION_MAX_AGE / 2)
        );
    }

    /// Startup accepts the exact minimum capacity and refuses one unit below it.
    ///
    /// # Panics
    ///
    /// Panics when the exact minimum is refused, when a one-unit shortfall in
    /// any category is admitted, or when the refusal does not name that
    /// category and its required and actual totals.
    #[test]
    fn startup_capacity_is_checked_per_category_at_exact_minimum() {
        let policy = ScribeArtifactPolicy::default();
        let minimum = policy.minimum_capacity();
        assert_eq!(
            policy.max_active_tables(&minimum),
            1,
            "the exact minimum owns exactly one canonical table"
        );
        policy
            .validate_capacity(&minimum)
            .expect("the exact minimum capacity serves");

        for category in ContentionCategory::ALL {
            let mut short = minimum;
            let reduced = short.component(category) - 1;
            match category {
                ContentionCategory::AdmissionItems => short.admission_items = reduced,
                ContentionCategory::AdmissionBytes => short.admission_bytes = reduced,
                ContentionCategory::Active => short.active_bytes = reduced,
                ContentionCategory::Immutable => short.immutable_bytes = reduced,
                ContentionCategory::DurableStage => short.durable_stage_bytes = reduced,
                ContentionCategory::MergeScratch => short.merge_scratch_bytes = reduced,
                ContentionCategory::StagingClaim => short.staging_claim_items = reduced,
                ContentionCategory::UploadClaim => short.upload_claim_items = reduced,
            }
            let error = policy
                .validate_capacity(&short)
                .expect_err("one unit below any category must refuse before serving");
            let ScribeGeometryError::Capacity {
                category: named,
                required,
                actual,
            } = error
            else {
                panic!("a capacity shortfall must report the capacity diagnostic");
            };
            assert_eq!(named, category.label());
            assert_eq!(required, minimum.component(category));
            assert_eq!(actual, reduced);
        }
    }

    /// A surplus in one category never covers a shortfall in another.
    ///
    /// # Panics
    ///
    /// Panics when byte and item categories are summed or traded against each
    /// other rather than checked independently.
    #[test]
    fn contention_categories_are_never_summed_across_units() {
        let policy = ScribeArtifactPolicy::default();
        let mut lopsided = policy.minimum_capacity();
        lopsided.active_bytes = lopsided.active_bytes.saturating_mul(64);
        lopsided.staging_claim_items -= 1;
        let error = policy
            .validate_capacity(&lopsided)
            .expect_err("abundant memory must not cover a missing claim item");
        assert!(matches!(
            error,
            ScribeGeometryError::Capacity {
                category: "staging_claim",
                ..
            }
        ));
    }

    /// The reserve vector reserves one item in each item category.
    ///
    /// # Panics
    ///
    /// Panics when a vector component is zero or when an item category is
    /// scaled by a byte-valued geometry knob.
    #[test]
    fn reserve_vector_is_complete_and_item_categories_are_one() {
        let policy = ScribeArtifactPolicy::default();
        let vector = policy.reserve_vector();
        assert_eq!(vector.first_empty_category(), None);
        assert_eq!(vector.admission_items, 1);
        assert_eq!(vector.staging_claim_items, 1);
        assert_eq!(vector.upload_claim_items, 1);
        for category in ContentionCategory::ALL {
            if category.is_items() {
                assert_eq!(vector.component(category), 1, "{}", category.label());
            } else {
                assert!(vector.component(category) > 1, "{}", category.label());
            }
        }
    }
}
