//! Independent Scribe geometry controls and per-table contention reserves.
//!
//! Scribe has five geometries that used to be conflated behind two numbers, so
//! moving one silently moved another. Each is now a separate named field with
//! one meaning:
//!
//! 1. **WAL segment** — encoded bytes in one WAL segment before rotation.
//! 2. **Active shard generation** — one pod-wide budget divided across the fixed
//!    sixteen shards, capped by an absolute per-shard ceiling. This is a
//!    rotation limit, never sixteen reservations and never an object-size
//!    promise.
//! 3. **Per-`SealKey` early seal** — an optional size or age that rotates one key
//!    *earlier* than its shard would. A key may seal earlier on size, age, or
//!    pressure; it may never seal later.
//! 4. **Parquet row group** — 32 MiB logical / 131,072 rows, owned by
//!    [`crate::parquet::memory`] and not restated here.
//! 5. **Scribe hot object** — the approximately 512 MiB staging assembly target.
//!
//! Forge's Iceberg rewrite target is a sixth geometry owned by `forge`, kept
//! deliberately separate from all of the above.
//!
//! On top of the geometries this module owns the contention reserve: the exact
//! per-table [`ContentionReserveVector`] that admission installs before a table
//! may serve, and the startup arithmetic that refuses to boot a pod whose global
//! capacity cannot hold the guaranteed number of those vectors.

use std::time::Duration;

use crate::scribe::routing::SCRIBE_SHARD_COUNT;

/// Default encoded bytes in one WAL segment before rotation.
pub const DEFAULT_WAL_SEGMENT_BYTES: u64 = 512 * 1024 * 1024;
/// Default absolute per-shard active-generation rotation ceiling.
///
/// A shard never rotates later than this even when the pod-wide budget would
/// permit a larger generation, so one oversized budget cannot turn a shard into
/// an unbounded memory owner.
pub const DEFAULT_GENERATION_ROTATION_CEILING_BYTES: u64 = 512 * 1024 * 1024;
/// Default pod-wide active-generation budget shared by all sixteen shards.
///
/// Sized so the derived per-shard limit lands exactly on
/// [`DEFAULT_GENERATION_ROTATION_CEILING_BYTES`]: the default pod behaves as if
/// each shard owned its own 512 MiB rotation limit, while a smaller deployment
/// can lower one number and have every shard narrow together.
pub const DEFAULT_ACTIVE_GENERATION_BUDGET_BYTES: u64 =
    DEFAULT_GENERATION_ROTATION_CEILING_BYTES * SCRIBE_SHARD_COUNT as u64;
/// Default maximum age of an active shard generation before rotation.
pub const DEFAULT_GENERATION_MAX_AGE: Duration = Duration::from_mins(10);
/// Default encoded Parquet target for one assembled Scribe hot object.
pub const DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES: u64 = 512 * 1024 * 1024;
/// Default number of tenants whose complete reserve vectors are guaranteed.
pub const DEFAULT_GUARANTEED_ACTIVE_TENANTS: usize = 4;
/// Default number of tables per tenant whose reserve vectors are guaranteed.
///
/// Two is the floor rather than a preference: a tenant always has at least one
/// registered system table alongside its dynamic table, and the packet's whole
/// contention argument is that a hot dynamic table must not be able to exclude
/// its sibling system table. One guaranteed table per tenant would make that
/// exclusion legal.
pub const DEFAULT_GUARANTEED_ACTIVE_TABLES_PER_TENANT: usize = 2;

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
/// A vector's cells are protected: no other tenant or table may borrow them,
/// however idle they look. Only capacity beyond the guaranteed vectors is
/// elastic surplus.
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
/// against the guaranteed reserve vectors and refuses to serve when it falls one
/// byte or item short.
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
    /// Pod capacity cannot hold the guaranteed reserve vectors in one category.
    #[error(
        "Scribe capacity for `{category}` holds {actual} but {width} guaranteed table reserves require {required}"
    )]
    Capacity {
        /// Fixed-cardinality category label.
        category: &'static str,
        /// Total the guaranteed reserves require in this category.
        required: usize,
        /// Total the pod actually has in this category.
        actual: usize,
        /// Guaranteed table width: tenants times tables per tenant.
        width: usize,
    },
    /// A reserve vector component is zero, so a table could activate
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
    /// Encoded bytes in one WAL segment before rotation.
    wal_segment_bytes: u64,
    /// Pod-wide active-generation budget shared by all sixteen shards.
    active_generation_budget_bytes: u64,
    /// Absolute per-shard rotation ceiling applied after the budget divides.
    generation_rotation_ceiling_bytes: u64,
    /// Maximum age of an active shard generation before rotation.
    generation_max_age: Duration,
    /// Optional per-`SealKey` size that rotates one key earlier than its shard.
    seal_key_early_seal_bytes: Option<usize>,
    /// Optional per-`SealKey` age that rotates one key earlier than its shard.
    seal_key_max_age: Option<Duration>,
    /// Encoded Parquet target for one assembled hot object.
    staging_target_file_size_bytes: u64,
    /// Tenants whose complete reserve vectors are guaranteed.
    guaranteed_active_tenants: usize,
    /// Tables per tenant whose reserve vectors are guaranteed.
    guaranteed_active_tables_per_tenant: usize,
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
    /// Validation is total: every positive-valued field is checked, the derived
    /// per-shard rotation limit must be positive, and an early-seal size may not
    /// exceed the per-shard limit it is supposed to precede. A geometry that
    /// passes here cannot later produce a zero rotation limit or an early seal
    /// that fires after the shard would already have rotated.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeGeometryError::Zero`] for a zero-valued required field,
    /// and [`ScribeGeometryError::Incoherent`] when the pod-wide budget divides
    /// to a zero per-shard limit or an early-seal control cannot fire before its
    /// shard rotates.
    #[expect(
        clippy::too_many_arguments,
        reason = "one validated value object over fourteen independent geometry knobs; \
                  splitting the constructor would let a caller build a half-checked geometry"
    )]
    pub fn new(
        wal_segment_bytes: u64,
        active_generation_budget_bytes: u64,
        generation_rotation_ceiling_bytes: u64,
        generation_max_age: Duration,
        seal_key_early_seal_bytes: Option<usize>,
        seal_key_max_age: Option<Duration>,
        staging_target_file_size_bytes: u64,
        guaranteed_active_tenants: usize,
        guaranteed_active_tables_per_tenant: usize,
        maximum_ingress_envelope_bytes: usize,
        maximum_active_request_ownership_bytes: usize,
        maximum_immutable_member_ownership_bytes: usize,
        minimum_stage_member_bytes: usize,
        minimum_merge_lane_scratch_bytes: usize,
    ) -> Result<Self, ScribeGeometryError> {
        let geometry = Self {
            wal_segment_bytes,
            active_generation_budget_bytes,
            generation_rotation_ceiling_bytes,
            generation_max_age,
            seal_key_early_seal_bytes,
            seal_key_max_age,
            staging_target_file_size_bytes,
            guaranteed_active_tenants,
            guaranteed_active_tables_per_tenant,
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
    /// limit, an ownership bound, or a guaranteed width vanish, so each is
    /// rejected by name rather than defaulted.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeGeometryError::Zero`] naming the first zero-valued
    /// field, and [`ScribeGeometryError::Incoherent`] when the guaranteed table
    /// width is below the system/dynamic table pair every tenant has.
    fn check_positive(&self) -> Result<(), ScribeGeometryError> {
        let byte_fields = [
            ("wal_segment_bytes", self.wal_segment_bytes),
            (
                "active_generation_budget_bytes",
                self.active_generation_budget_bytes,
            ),
            (
                "generation_rotation_ceiling_bytes",
                self.generation_rotation_ceiling_bytes,
            ),
            (
                "staging_target_file_size_bytes",
                self.staging_target_file_size_bytes,
            ),
        ];
        if let Some((field, _)) = byte_fields.into_iter().find(|(_, value)| *value == 0) {
            return Err(ScribeGeometryError::Zero { field });
        }
        let width_fields = [
            ("guaranteed_active_tenants", self.guaranteed_active_tenants),
            (
                "guaranteed_active_tables_per_tenant",
                self.guaranteed_active_tables_per_tenant,
            ),
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
        if self.guaranteed_active_tables_per_tenant < DEFAULT_GUARANTEED_ACTIVE_TABLES_PER_TENANT {
            return Err(ScribeGeometryError::Incoherent {
                field: "guaranteed_active_tables_per_tenant",
                detail: format!(
                    "a tenant always has a registered system table beside its dynamic table, \
                     so the guaranteed table width is never below \
                     {DEFAULT_GUARANTEED_ACTIVE_TABLES_PER_TENANT}"
                ),
            });
        }
        Ok(())
    }

    /// Refuses a geometry whose derived per-shard limits cannot be served.
    ///
    /// Runs after [`Self::check_positive`] so it can rely on every input being
    /// positive and only has to judge what the division and the optional
    /// per-key controls produce.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeGeometryError::Incoherent`] when the pod-wide budget
    /// divides to no per-shard rotation limit at all, or when a per-`SealKey`
    /// control would fire after its shard has already rotated, and
    /// [`ScribeGeometryError::Zero`] for a zero early-seal size.
    fn check_derived(&self) -> Result<(), ScribeGeometryError> {
        let shard_limit = self.shard_generation_rotation_bytes();
        if shard_limit == 0 {
            return Err(ScribeGeometryError::Incoherent {
                field: "active_generation_budget_bytes",
                detail: format!(
                    "dividing it across the fixed {SCRIBE_SHARD_COUNT} shards leaves no \
                     rotation limit at all"
                ),
            });
        }
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

    /// Builds a geometry whose sixteen shards each rotate at `shard_bytes`.
    ///
    /// Embedded and test callers know the per-shard rotation limit they want
    /// directly rather than a pod-wide budget, so this scales the budget up by
    /// the shard count and pins the ceiling to the same value; the derived
    /// per-shard limit is then exactly `shard_bytes`. Every other field takes
    /// its production default, so an embedded Scribe is not silently running a
    /// second contention policy.
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
        let ceiling = u64::try_from(shard_bytes).unwrap_or(u64::MAX);
        Self::new(
            wal_segment_bytes,
            ceiling.saturating_mul(SCRIBE_SHARD_COUNT as u64),
            ceiling,
            generation_max_age,
            None,
            None,
            DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES,
            DEFAULT_GUARANTEED_ACTIVE_TENANTS,
            DEFAULT_GUARANTEED_ACTIVE_TABLES_PER_TENANT,
            crate::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES,
            DEFAULT_MAXIMUM_ACTIVE_REQUEST_OWNERSHIP_BYTES,
            DEFAULT_MAXIMUM_IMMUTABLE_MEMBER_OWNERSHIP_BYTES,
            DEFAULT_MINIMUM_STAGE_MEMBER_BYTES,
            DEFAULT_MINIMUM_MERGE_LANE_SCRATCH_BYTES,
        )
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

    /// Returns the rotation limit each of the sixteen shards receives.
    ///
    /// This is `min(ceiling, floor(budget / 16))`. It is a *limit*, not a
    /// reservation: the pod does not set aside sixteen of these, and a shard
    /// that rotates at this size makes no promise about the size of any object
    /// later assembled from what it froze.
    #[must_use]
    pub const fn shard_generation_rotation_bytes(&self) -> u64 {
        let divided = self.active_generation_budget_bytes / SCRIBE_SHARD_COUNT as u64;
        if divided < self.generation_rotation_ceiling_bytes {
            divided
        } else {
            self.generation_rotation_ceiling_bytes
        }
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

    /// Returns the guaranteed contention width: tenants times tables per tenant.
    ///
    /// This is the number of complete reserve vectors startup must be able to
    /// install before the pod serves anything.
    #[must_use]
    pub const fn guaranteed_width(&self) -> usize {
        self.guaranteed_active_tenants * self.guaranteed_active_tables_per_tenant
    }

    /// Returns the guaranteed tenant width.
    #[must_use]
    pub const fn guaranteed_active_tenants(&self) -> usize {
        self.guaranteed_active_tenants
    }

    /// Returns the guaranteed per-tenant table width.
    #[must_use]
    pub const fn guaranteed_active_tables_per_tenant(&self) -> usize {
        self.guaranteed_active_tables_per_tenant
    }
}

impl Default for ScribeGeometry {
    /// Returns the production default geometry.
    ///
    /// The defaults are chosen so a default pod behaves as if each of the
    /// sixteen shards owned an independent 512 MiB rotation limit, and so the
    /// hot-object target equals that limit without either value being derived
    /// from the other.
    ///
    /// # Panics
    ///
    /// Panics only if the defaults in this module contradict
    /// [`ScribeGeometry::new`]'s own validation, which is a bug in this file
    /// rather than an operator input.
    fn default() -> Self {
        Self::new(
            DEFAULT_WAL_SEGMENT_BYTES,
            DEFAULT_ACTIVE_GENERATION_BUDGET_BYTES,
            DEFAULT_GENERATION_ROTATION_CEILING_BYTES,
            DEFAULT_GENERATION_MAX_AGE,
            None,
            None,
            DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES,
            DEFAULT_GUARANTEED_ACTIVE_TENANTS,
            DEFAULT_GUARANTEED_ACTIVE_TABLES_PER_TENANT,
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
    /// per-table maxima and minima. Nothing here is scaled by how many tables or
    /// tenants happen to be active, because a protected cell must not shrink
    /// when a neighbour appears.
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

    /// Returns the total one category must hold for every guaranteed vector.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeGeometryError::Incoherent`] when the multiplication
    /// overflows, which means the configured width and per-table component
    /// cannot coexist on any machine rather than merely not fitting this one.
    pub fn required_for(&self, category: ContentionCategory) -> Result<usize, ScribeGeometryError> {
        let component = self.reserve_vector().component(category);
        self.geometry
            .guaranteed_width()
            .checked_mul(component)
            .ok_or_else(|| ScribeGeometryError::Incoherent {
                field: "guaranteed_active_tenants",
                detail: format!(
                    "reserving {} vectors of {component} in `{}` overflows",
                    self.geometry.guaranteed_width(),
                    category.label()
                ),
            })
    }

    /// Refuses to serve unless the pod holds every guaranteed reserve vector.
    ///
    /// Each category is checked independently against its own capacity
    /// component: byte and item categories are never summed, and a surplus in
    /// one category never covers a shortfall in another. A pod one byte or one
    /// item below any component fails here, before serving, with the category,
    /// the required total, the actual total, and the guaranteed width named.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeGeometryError::EmptyReserve`] when a vector component is
    /// zero, [`ScribeGeometryError::Incoherent`] when the required total
    /// overflows, and [`ScribeGeometryError::Capacity`] naming the first
    /// category the pod cannot hold.
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
            let required = self.required_for(category)?;
            let actual = capacity.component(category);
            if actual < required {
                return Err(ScribeGeometryError::Capacity {
                    category: category.label(),
                    required,
                    actual,
                    width: self.geometry.guaranteed_width(),
                });
            }
        }
        Ok(())
    }

    /// Returns the smallest pod capacity that satisfies this policy exactly.
    ///
    /// Used by startup diagnostics and by the minimum-geometry configuration
    /// test: a capacity built from this function passes
    /// [`Self::validate_capacity`], and lowering any single component by one
    /// makes it fail.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeGeometryError::Incoherent`] when any required total
    /// overflows.
    pub fn minimum_capacity(&self) -> Result<ScribeGlobalCapacity, ScribeGeometryError> {
        Ok(ScribeGlobalCapacity {
            admission_items: self.required_for(ContentionCategory::AdmissionItems)?,
            admission_bytes: self.required_for(ContentionCategory::AdmissionBytes)?,
            active_bytes: self.required_for(ContentionCategory::Active)?,
            immutable_bytes: self.required_for(ContentionCategory::Immutable)?,
            durable_stage_bytes: self.required_for(ContentionCategory::DurableStage)?,
            merge_scratch_bytes: self.required_for(ContentionCategory::MergeScratch)?,
            staging_claim_items: self.required_for(ContentionCategory::StagingClaim)?,
            upload_claim_items: self.required_for(ContentionCategory::UploadClaim)?,
        })
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

    /// Builds one geometry from the defaults with named overrides applied.
    ///
    /// # Panics
    ///
    /// Panics when the requested override combination is invalid, which is the
    /// point for the negative cases that assert on the error instead.
    fn geometry_with(budget: u64, ceiling: u64) -> Result<ScribeGeometry, ScribeGeometryError> {
        ScribeGeometry::new(
            DEFAULT_WAL_SEGMENT_BYTES,
            budget,
            ceiling,
            DEFAULT_GENERATION_MAX_AGE,
            None,
            None,
            DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES,
            DEFAULT_GUARANTEED_ACTIVE_TENANTS,
            DEFAULT_GUARANTEED_ACTIVE_TABLES_PER_TENANT,
            crate::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES,
            DEFAULT_MAXIMUM_ACTIVE_REQUEST_OWNERSHIP_BYTES,
            DEFAULT_MAXIMUM_IMMUTABLE_MEMBER_OWNERSHIP_BYTES,
            DEFAULT_MINIMUM_STAGE_MEMBER_BYTES,
            DEFAULT_MINIMUM_MERGE_LANE_SCRATCH_BYTES,
        )
    }

    /// Every shard's rotation limit comes from one global budget and one ceiling.
    ///
    /// # Panics
    ///
    /// Panics when a derived limit is not the exact minimum of the ceiling and
    /// the evenly divided budget, or when an incoherent budget is accepted.
    #[test]
    fn shard_generation_limits_derive_from_one_global_budget() {
        let default = ScribeGeometry::default();
        assert_eq!(
            default.shard_generation_rotation_bytes(),
            DEFAULT_GENERATION_ROTATION_CEILING_BYTES,
            "the default budget divides exactly onto the default ceiling"
        );

        // The ceiling wins when the divided budget would exceed it: one large
        // budget cannot turn a shard into an unbounded memory owner.
        let ceiling_bound = geometry_with(64 * 1024 * 1024 * 1024, 256 * 1024 * 1024)
            .expect("a budget above the ceiling is valid");
        assert_eq!(
            ceiling_bound.shard_generation_rotation_bytes(),
            256 * 1024 * 1024
        );

        // The divided budget wins when it is the smaller of the two, and every
        // shard narrows together rather than the first sixteen tenants racing.
        let budget_bound = geometry_with(1024 * 1024 * 1024, 512 * 1024 * 1024)
            .expect("a budget below sixteen ceilings is valid");
        assert_eq!(
            budget_bound.shard_generation_rotation_bytes(),
            64 * 1024 * 1024,
            "1 GiB across sixteen shards is 64 MiB each, not sixteen reservations of 512 MiB"
        );

        // A budget that cannot give every shard even one byte is incoherent.
        let starved = geometry_with(u64::from(u32::try_from(SCRIBE_SHARD_COUNT - 1).unwrap()), 1)
            .expect_err("a budget below one byte per shard must refuse");
        assert!(matches!(
            starved,
            ScribeGeometryError::Incoherent {
                field: "active_generation_budget_bytes",
                ..
            }
        ));
    }

    /// The five geometries move independently and each is bounded on its own.
    ///
    /// # Panics
    ///
    /// Panics when changing one geometry moves another, when a zero is
    /// accepted, or when an early seal is allowed to fire after its shard would
    /// already have rotated.
    #[test]
    fn scribe_geometry_controls_are_independent_and_bounded() {
        let base = ScribeGeometry::default();

        // Moving the WAL segment leaves the generation and object targets alone.
        let wal_moved = ScribeGeometry::new(
            64 * 1024 * 1024,
            DEFAULT_ACTIVE_GENERATION_BUDGET_BYTES,
            DEFAULT_GENERATION_ROTATION_CEILING_BYTES,
            DEFAULT_GENERATION_MAX_AGE,
            None,
            None,
            DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES,
            DEFAULT_GUARANTEED_ACTIVE_TENANTS,
            DEFAULT_GUARANTEED_ACTIVE_TABLES_PER_TENANT,
            crate::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES,
            DEFAULT_MAXIMUM_ACTIVE_REQUEST_OWNERSHIP_BYTES,
            DEFAULT_MAXIMUM_IMMUTABLE_MEMBER_OWNERSHIP_BYTES,
            DEFAULT_MINIMUM_STAGE_MEMBER_BYTES,
            DEFAULT_MINIMUM_MERGE_LANE_SCRATCH_BYTES,
        )
        .expect("a smaller WAL segment is valid on its own");
        assert_eq!(wal_moved.wal_segment_bytes(), 64 * 1024 * 1024);
        assert_eq!(
            wal_moved.shard_generation_rotation_bytes(),
            base.shard_generation_rotation_bytes()
        );
        assert_eq!(
            wal_moved.staging_target_file_size_bytes(),
            base.staging_target_file_size_bytes()
        );

        // Moving the generation budget leaves the object target alone: a shard
        // rotation limit is not an object-size promise.
        let generation_moved =
            geometry_with(1024 * 1024 * 1024, 512 * 1024 * 1024).expect("valid budget");
        assert_ne!(
            generation_moved.shard_generation_rotation_bytes(),
            base.shard_generation_rotation_bytes()
        );
        assert_eq!(
            generation_moved.staging_target_file_size_bytes(),
            DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES
        );

        // Zero is refused on every required field, naming the field.
        assert!(matches!(
            geometry_with(0, DEFAULT_GENERATION_ROTATION_CEILING_BYTES),
            Err(ScribeGeometryError::Zero {
                field: "active_generation_budget_bytes"
            })
        ));
        assert!(matches!(
            geometry_with(DEFAULT_ACTIVE_GENERATION_BUDGET_BYTES, 0),
            Err(ScribeGeometryError::Zero {
                field: "generation_rotation_ceiling_bytes"
            })
        ));

        // A key may seal earlier than its shard, never later.
        let early = ScribeGeometry::new(
            DEFAULT_WAL_SEGMENT_BYTES,
            DEFAULT_ACTIVE_GENERATION_BUDGET_BYTES,
            DEFAULT_GENERATION_ROTATION_CEILING_BYTES,
            DEFAULT_GENERATION_MAX_AGE,
            Some(usize::try_from(DEFAULT_GENERATION_ROTATION_CEILING_BYTES).expect("fits"))
                .map(|limit| limit + 1),
            None,
            DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES,
            DEFAULT_GUARANTEED_ACTIVE_TENANTS,
            DEFAULT_GUARANTEED_ACTIVE_TABLES_PER_TENANT,
            crate::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES,
            DEFAULT_MAXIMUM_ACTIVE_REQUEST_OWNERSHIP_BYTES,
            DEFAULT_MAXIMUM_IMMUTABLE_MEMBER_OWNERSHIP_BYTES,
            DEFAULT_MINIMUM_STAGE_MEMBER_BYTES,
            DEFAULT_MINIMUM_MERGE_LANE_SCRATCH_BYTES,
        );
        assert!(matches!(
            early,
            Err(ScribeGeometryError::Incoherent {
                field: "seal_key_early_seal_bytes",
                ..
            })
        ));

        // The guaranteed table width never drops below the system/dynamic pair.
        let narrow = ScribeGeometry::new(
            DEFAULT_WAL_SEGMENT_BYTES,
            DEFAULT_ACTIVE_GENERATION_BUDGET_BYTES,
            DEFAULT_GENERATION_ROTATION_CEILING_BYTES,
            DEFAULT_GENERATION_MAX_AGE,
            None,
            None,
            DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES,
            1,
            1,
            crate::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES,
            DEFAULT_MAXIMUM_ACTIVE_REQUEST_OWNERSHIP_BYTES,
            DEFAULT_MAXIMUM_IMMUTABLE_MEMBER_OWNERSHIP_BYTES,
            DEFAULT_MINIMUM_STAGE_MEMBER_BYTES,
            DEFAULT_MINIMUM_MERGE_LANE_SCRATCH_BYTES,
        );
        assert!(matches!(
            narrow,
            Err(ScribeGeometryError::Incoherent {
                field: "guaranteed_active_tables_per_tenant",
                ..
            })
        ));
    }

    /// Startup accepts the exact minimum capacity and refuses one unit below it.
    ///
    /// # Panics
    ///
    /// Panics when the exact minimum is refused, when a one-unit shortfall in
    /// any category is admitted, or when the refusal does not name that
    /// category, its required and actual totals, and the guaranteed width.
    #[test]
    fn startup_capacity_is_checked_per_category_at_exact_minimum() {
        let policy = ScribeArtifactPolicy::default();
        let width = policy.geometry().guaranteed_width();
        assert_eq!(width, 8, "four tenants times two tables");

        let minimum = policy
            .minimum_capacity()
            .expect("the default policy has a representable minimum");
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
                width: reported,
            } = error
            else {
                panic!("a capacity shortfall must report the capacity diagnostic");
            };
            assert_eq!(named, category.label());
            assert_eq!(required, minimum.component(category));
            assert_eq!(actual, reduced);
            assert_eq!(reported, width);
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
        let mut lopsided = policy
            .minimum_capacity()
            .expect("the default policy has a representable minimum");
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
