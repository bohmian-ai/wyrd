//! Independent Scribe geometry controls.
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

/// Startup refusal describing exactly which geometry value failed.
///
/// Every variant names the value an operator must change.
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
}

/// Independent, validated Scribe geometry.
///
/// Construct through [`ScribeGeometry::new`], which is the only place the fields
/// are checked; a caller cannot assemble an unvalidated geometry and reach the
/// derived rotation limits with it.
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
    pub fn new(
        shard_count: usize,
        wal_segment_bytes: u64,
        generation_rotation_bytes: u64,
        generation_max_age: Duration,
        seal_key_early_seal_bytes: Option<usize>,
        seal_key_max_age: Option<Duration>,
        staging_target_file_size_bytes: u64,
    ) -> Result<Self, ScribeGeometryError> {
        let geometry = Self {
            shard_count,
            wal_segment_bytes,
            generation_rotation_bytes,
            generation_max_age,
            seal_key_early_seal_bytes,
            seal_key_max_age,
            staging_target_file_size_bytes,
        };
        geometry.check_positive()?;
        geometry.check_derived()?;
        Ok(geometry)
    }

    /// Refuses any geometry field that must be positive but was left at zero.
    ///
    /// A zero here is never a "disabled" control: it would make a rotation
    /// limit vanish, so each is rejected by name rather than defaulted.
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
        if self.shard_count == 0 {
            return Err(ScribeGeometryError::Zero {
                field: "shard_count",
            });
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
    /// limits directly. Every other field takes its production default;
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
        )
        .expect("the module's own default geometry satisfies its own validation")
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
    /// their inputs here rather than restating seven arguments per case.
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
    /// rotation limit silently vanished.
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
    /// booting a pod whose rotation limit silently vanished.
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
        let zero_cases: [ZeroCase; 4] = [
            ("shard_count", |over| over.shard_count = 0),
            ("wal_segment_bytes", |over| over.wal_segment_bytes = 0),
            ("generation_rotation_bytes", |over| {
                over.generation_rotation_bytes = 0;
            }),
            ("staging_target_file_size_bytes", |over| {
                over.staging_target_file_size_bytes = 0;
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
}
