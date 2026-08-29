//! Canonical Scribe production workload, checkpoints, and evidence.
//!
//! This module is the single producer and shared consumer API for Scribe
//! qualification. The Scribe candidate runs it with the cache absent; the cache
//! task and Forge later run the *same serialized bytes* and add only their own
//! evidence. That is why the workload is a serializable value rather than a
//! test-local fixture: a consumer that regenerated the seed, the operations, or
//! the expectations would be proving a different run.
//!
//! The production promotion record is reexported, never reimplemented — a
//! second projection of a published object would be a second contract.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;
use vala_bifrost_redux::scribe::geometry::{ScribeGeometry, ScribeGeometryError};
use wyrd_spec::DataTenantId;

pub use vala_bifrost_redux::scribe::promotion::ScribePublishedHotFileV1;

/// Version of the canonical production-workload contract.
pub const SCRIBE_PRODUCTION_WORKLOAD_VERSION: u16 = 1;

/// Whether the run is executed with the read cache absent or present.
///
/// Scribe proves authoritative correctness with the cache absent. The variant
/// exists so the cache task can run these exact bytes twice and compare, not so
/// this crate can configure cache behavior — it owns none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScribeCacheMode {
    /// The authoritative run: no cache participates in any read.
    Disabled,
    /// The comparison run owned by the cache task.
    Enabled,
}

/// Whether a workload table is a Wyrd system table or a user's dynamic table.
///
/// Recorded because the fairness expectations are stated over the pair: the two
/// kinds must be governed identically, which is only observable if the record
/// says which is which.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScribeWorkloadTableKindV1 {
    /// A Wyrd-owned table in a reserved namespace.
    System,
    /// A user-registered table in the dynamic dataset namespace.
    Dynamic,
}

/// One canonical table the workload registers and writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScribeWorkloadTableV1 {
    /// Bifrost namespace segment of the canonical name.
    pub namespace: String,
    /// Table name inside that namespace, unique within the workload.
    pub name: String,
    /// Which governed class this table belongs to.
    pub kind: ScribeWorkloadTableKindV1,
}

impl ScribeWorkloadTableV1 {
    /// Returns the canonical fully-qualified name public ingest and query use.
    #[must_use]
    pub fn fqn(&self) -> String {
        format!("vala.{}.{}", self.namespace, self.name)
    }
}

/// One tenant of the workload and the tables it owns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScribeWorkloadTenantV1 {
    /// Deterministic tenant slug the runner seeds.
    pub slug: String,
    /// Tables this tenant registers, in registration order.
    pub tables: Vec<ScribeWorkloadTableV1>,
}

/// Serializable recipe for the runtime geometry the run installs.
///
/// [`ScribeGeometry`] is a validated value object rather than a wire type, so
/// the record carries the controls a run may legitimately move and resolves
/// them through the production constructor. A recipe that does not validate is
/// refused here rather than silently producing a second geometry policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScribeGeometryRecipeV1 {
    /// Encoded bytes in one WAL segment before rotation.
    pub wal_segment_bytes: u64,
    /// Per-shard rotation limit every one of the sixteen lanes receives.
    pub shard_rotation_bytes: u64,
    /// Maximum age of an active shard generation, in milliseconds.
    pub generation_max_age_millis: u64,
    /// Encoded Parquet target for one assembled hot object.
    pub staging_target_file_size_bytes: u64,
}

impl ScribeGeometryRecipeV1 {
    /// Resolves the recipe through the production geometry constructor.
    ///
    /// # Errors
    ///
    /// Returns the [`ScribeGeometryError`] naming the field the production
    /// validator refused.
    pub fn resolve(&self) -> Result<ScribeGeometry, ScribeGeometryError> {
        ScribeGeometry::for_uniform_shard_rotation(
            self.wal_segment_bytes,
            usize::try_from(self.shard_rotation_bytes).unwrap_or(usize::MAX),
            Duration::from_millis(self.generation_max_age_millis),
        )?
        .with_staging_target_file_size_bytes(self.staging_target_file_size_bytes)
    }
}

/// One ordered operation the runner performs through public surfaces only.
///
/// Tenants and tables are named by ordinal rather than by identifier because
/// the identifiers are assigned by the server at run time; the record fixes
/// *which* logical owner acts, and the runner binds that to the real identity
/// it was given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum ScribeWorkloadOperationV1 {
    /// Append one batch with a fixed batch id through public gRPC ingest.
    Append {
        /// Ordinal of the owning tenant in [`ScribeProductionWorkloadV1`].
        tenant: usize,
        /// Ordinal of the table inside that tenant.
        table: usize,
        /// Fixed batch identity, so a replay of this record is the same batch.
        batch_id: Uuid,
        /// Values written, in order.
        rows: Vec<i64>,
    },
    /// Drive one tenant's durable seal and publication to completion.
    Flush {
        /// Ordinal of the tenant to flush.
        tenant: usize,
    },
    /// Read one table back through the public query route.
    Read {
        /// Ordinal of the owning tenant.
        tenant: usize,
        /// Ordinal of the table to read.
        table: usize,
    },
    /// Record a named lifecycle checkpoint from the observed state.
    Checkpoint {
        /// Which lifecycle boundary this checkpoint names.
        name: ScribeCheckpointNameV1,
    },
}

/// What a checkpoint's own definition says about published hot objects.
///
/// The expectation is a property of the named boundary rather than of a run:
/// "nothing is published yet" and "the members are now published" are the two
/// facts that make `AfterAck` and `StagedToHot` different boundaries at all. A
/// comparator that did not check it would accept a run that published early or
/// one that reached a publication boundary having published nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScribePublicationExpectationV1 {
    /// The boundary is defined by nothing being published yet.
    Absent,
    /// The boundary constrains publication in neither direction.
    Unconstrained,
    /// The boundary is defined by at least one published hot object.
    Present,
}

/// The named lifecycle boundaries a production run must account for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScribeCheckpointNameV1 {
    /// Rows are acknowledged and WAL-durable; nothing is published yet.
    AfterAck,
    /// Active generations have been frozen into immutable members.
    ActiveToStaged,
    /// Staged members have been assembled and published as hot objects.
    StagedToHot,
    /// The pod recovered and replayed without changing row identity.
    RestartReplay,
    /// A reader's pinned snapshot advanced onto the published objects.
    SnapshotAdvance,
    /// The pod drained; every owned resource is released.
    TerminalDrain,
}

impl ScribeCheckpointNameV1 {
    /// Reports whether reaching this boundary requires reading rows back.
    ///
    /// The boundaries after publication are the ones whose whole claim is that
    /// the rows are still exactly readable — a replay that changed nothing, a
    /// snapshot that advanced onto the published objects, a drain that lost
    /// nothing. Reaching one of those with no read-back is not evidence of
    /// anything, so the comparator requires the digest rather than treating an
    /// absent one as "not read, therefore fine".
    #[must_use]
    pub const fn requires_read_back_digest(self) -> bool {
        matches!(
            self,
            Self::RestartReplay | Self::SnapshotAdvance | Self::TerminalDrain
        )
    }

    /// Returns what this boundary's definition asserts about publication.
    #[must_use]
    pub const fn publication_expectation(self) -> ScribePublicationExpectationV1 {
        match self {
            Self::AfterAck => ScribePublicationExpectationV1::Absent,
            Self::ActiveToStaged => ScribePublicationExpectationV1::Unconstrained,
            Self::StagedToHot
            | Self::RestartReplay
            | Self::SnapshotAdvance
            | Self::TerminalDrain => ScribePublicationExpectationV1::Present,
        }
    }
}

/// The canonical serializable Scribe production workload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScribeProductionWorkloadV1 {
    /// Contract version; an unknown version fails closed.
    pub version: u16,
    /// Deterministic generator seed for every derived value.
    pub seed: u64,
    /// Runtime geometry the run installs before any append.
    pub geometry: ScribeGeometryRecipeV1,
    /// Tenants and their tables, in registration order.
    pub tenants: Vec<ScribeWorkloadTenantV1>,
    /// Ordered public operations the runner performs.
    pub operations: Vec<ScribeWorkloadOperationV1>,
    /// Checkpoints the run is required to produce, in order.
    pub required_checkpoints: Vec<ScribeCheckpointNameV1>,
}

impl ScribeProductionWorkloadV1 {
    /// Builds the canonical workload every Scribe consumer runs.
    ///
    /// Four tenants, one system and one dynamic table each, deterministic batch
    /// ids that spread across the shard lanes, and the six named lifecycle
    /// checkpoints. Every value here is fixed: a consumer that wants different
    /// coverage adds its own evidence to this run rather than editing it.
    #[must_use]
    pub fn canonical() -> Self {
        let seed = 0x5C21_BE00_0000_0001;
        let tenants: Vec<ScribeWorkloadTenantV1> = (0..4)
            .map(|tenant| ScribeWorkloadTenantV1 {
                slug: format!("scribe-workload-{tenant}"),
                tables: vec![
                    ScribeWorkloadTableV1 {
                        namespace: "bifrost".to_owned(),
                        name: format!("workload_system_{tenant}"),
                        kind: ScribeWorkloadTableKindV1::System,
                    },
                    ScribeWorkloadTableV1 {
                        namespace: "datasets".to_owned(),
                        name: format!("workload_dynamic_{tenant}"),
                        kind: ScribeWorkloadTableKindV1::Dynamic,
                    },
                ],
            })
            .collect();

        let mut operations = Vec::new();
        for tenant in 0..tenants.len() {
            for table in 0..2 {
                for round in 0..2_u64 {
                    let ordinal = (tenant * 4 + table * 2) as u64 + round;
                    operations.push(ScribeWorkloadOperationV1::Append {
                        tenant,
                        table,
                        batch_id: deterministic_batch_id(seed, ordinal),
                        rows: (0..8_i64)
                            .map(|row| i64::try_from(ordinal).unwrap_or(i64::MAX) * 1_000 + row)
                            .collect(),
                    });
                }
            }
        }
        operations.push(ScribeWorkloadOperationV1::Checkpoint {
            name: ScribeCheckpointNameV1::AfterAck,
        });
        for tenant in 0..tenants.len() {
            operations.push(ScribeWorkloadOperationV1::Flush { tenant });
        }
        operations.push(ScribeWorkloadOperationV1::Checkpoint {
            name: ScribeCheckpointNameV1::StagedToHot,
        });
        for tenant in 0..tenants.len() {
            for table in 0..2 {
                operations.push(ScribeWorkloadOperationV1::Read { tenant, table });
            }
        }
        operations.push(ScribeWorkloadOperationV1::Checkpoint {
            name: ScribeCheckpointNameV1::SnapshotAdvance,
        });

        Self {
            version: SCRIBE_PRODUCTION_WORKLOAD_VERSION,
            seed,
            geometry: ScribeGeometryRecipeV1 {
                wal_segment_bytes: 4 * 1024 * 1024,
                shard_rotation_bytes: 2 * 1024 * 1024,
                generation_max_age_millis: 60_000,
                staging_target_file_size_bytes: 8 * 1024 * 1024,
            },
            tenants,
            operations,
            required_checkpoints: vec![
                ScribeCheckpointNameV1::AfterAck,
                ScribeCheckpointNameV1::StagedToHot,
                ScribeCheckpointNameV1::SnapshotAdvance,
            ],
        }
    }

    /// Refuses a record this build cannot execute exactly.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeWorkloadError::UnsupportedVersion`] for an unknown
    /// version, [`ScribeWorkloadError::Malformed`] when an operation names an
    /// owner the record does not declare or when a read-required checkpoint is
    /// not preceded by a read of its own, and
    /// [`ScribeWorkloadError::Geometry`] when the geometry recipe does not
    /// validate.
    pub fn validate(&self) -> Result<(), ScribeWorkloadError> {
        if self.version != SCRIBE_PRODUCTION_WORKLOAD_VERSION {
            return Err(ScribeWorkloadError::UnsupportedVersion {
                version: self.version,
            });
        }
        self.geometry
            .resolve()
            .map_err(|error| ScribeWorkloadError::Geometry(error.to_string()))?;
        for operation in &self.operations {
            let (tenant, table) = match operation {
                ScribeWorkloadOperationV1::Append { tenant, table, .. }
                | ScribeWorkloadOperationV1::Read { tenant, table } => (*tenant, Some(*table)),
                ScribeWorkloadOperationV1::Flush { tenant } => (*tenant, None),
                ScribeWorkloadOperationV1::Checkpoint { .. } => continue,
            };
            let declared = self
                .tenants
                .get(tenant)
                .ok_or(ScribeWorkloadError::Malformed {
                    detail: "an operation names an undeclared tenant",
                })?;
            if table.is_some_and(|table| table >= declared.tables.len()) {
                return Err(ScribeWorkloadError::Malformed {
                    detail: "an operation names an undeclared table",
                });
            }
        }
        self.assert_read_required_checkpoints_read_for_themselves()?;
        Ok(())
    }

    /// Refuses a record whose read-required boundary reuses an earlier read.
    ///
    /// A boundary that [requires a read-back
    /// digest](ScribeCheckpointNameV1::requires_read_back_digest) claims the
    /// rows are still exactly readable *after* that transition. The runner
    /// binds each checkpoint's digest to the reads observed since the previous
    /// checkpoint, so a record that places two read-required boundaries in a
    /// row with only one read between them describes a run whose second
    /// boundary can produce no digest at all. Refusing the record here names
    /// the authoring mistake instead of letting it surface later as an
    /// evidence mismatch.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeWorkloadError::Malformed`] when a read-required
    /// checkpoint has no [`ScribeWorkloadOperationV1::Read`] between it and the
    /// checkpoint before it.
    fn assert_read_required_checkpoints_read_for_themselves(
        &self,
    ) -> Result<(), ScribeWorkloadError> {
        let mut read_since_checkpoint = false;
        for operation in &self.operations {
            match operation {
                ScribeWorkloadOperationV1::Read { .. } => read_since_checkpoint = true,
                ScribeWorkloadOperationV1::Checkpoint { name } => {
                    if name.requires_read_back_digest() && !read_since_checkpoint {
                        return Err(ScribeWorkloadError::Malformed {
                            detail: "a read-required checkpoint is not preceded by a read of its own",
                        });
                    }
                    read_since_checkpoint = false;
                }
                ScribeWorkloadOperationV1::Append { .. }
                | ScribeWorkloadOperationV1::Flush { .. } => {}
            }
        }
        Ok(())
    }

    /// Returns the canonical row digest the whole run must reproduce.
    ///
    /// The digest is over the sorted `(tenant ordinal, table ordinal, value)`
    /// triples the record's appends declare, so it is a property of the record
    /// rather than of any one execution of it. A run whose read-back does not
    /// hash to this value wrote, lost, duplicated or reordered rows.
    #[must_use]
    pub fn expected_row_digest(&self) -> String {
        let mut rows: Vec<(usize, usize, i64)> = Vec::new();
        for operation in &self.operations {
            if let ScribeWorkloadOperationV1::Append {
                tenant,
                table,
                rows: values,
                ..
            } = operation
            {
                rows.extend(values.iter().map(|value| (*tenant, *table, *value)));
            }
        }
        row_digest(&mut rows)
    }

    /// Returns how many rows the record's appends declare in total.
    #[must_use]
    pub fn expected_rows(&self) -> u64 {
        self.operations
            .iter()
            .map(|operation| match operation {
                ScribeWorkloadOperationV1::Append { rows, .. } => rows.len() as u64,
                _ => 0,
            })
            .sum()
    }
}

/// Derives one deterministic `UUIDv7` batch identity from the record's seed.
///
/// The ingest contract requires a v7 batch id, and the record requires the id
/// to be fixed: a replayed record must present the same batch identity so the
/// server's exactly-once path recognises it as a retry rather than new rows.
/// The seed and ordinal fill the free bits; the version and variant nibbles are
/// stamped so the value is a legal v7 rather than an arbitrary 128-bit number.
fn deterministic_batch_id(seed: u64, ordinal: u64) -> Uuid {
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&seed.to_be_bytes());
    bytes[8..].copy_from_slice(&ordinal.to_be_bytes());
    bytes[6] = 0x70 | (bytes[6] & 0x0f);
    bytes[8] = 0x80 | (bytes[8] & 0x3f);
    Uuid::from_bytes(bytes)
}

/// Hashes a row set into the canonical digest both sides compare.
///
/// Sorting first is what makes the digest an identity of the *set* of rows: a
/// run may legitimately return them in a different physical order across
/// artifacts, and that must not read as data loss.
fn row_digest(rows: &mut [(usize, usize, i64)]) -> String {
    rows.sort_unstable();
    let mut hasher = Sha256::new();
    for (tenant, table, value) in rows.iter() {
        hasher.update(tenant.to_le_bytes());
        hasher.update(table.to_le_bytes());
        hasher.update(value.to_le_bytes());
    }
    hex::encode(hasher.finalize())
}

/// One observed lifecycle boundary of a production run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScribeLifecycleCheckpointV1 {
    /// Which named boundary this observation belongs to.
    pub name: ScribeCheckpointNameV1,
    /// Rows the run had acknowledged by this boundary.
    pub acknowledged_rows: u64,
    /// Published hot objects observed, per tenant slug, in publication order.
    pub published: BTreeMap<String, Vec<ScribePublishedHotFileV1>>,
    /// Canonical digest of everything read back at this boundary, when read.
    pub observed_row_digest: Option<String>,
}

/// Everything one production run observed, in checkpoint order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScribeProductionEvidenceV1 {
    /// Contract version of the workload that produced this evidence.
    pub version: u16,
    /// Whether the read cache participated in the run.
    pub cache_mode: ScribeCacheMode,
    /// Observed checkpoints, in the order the run reached them.
    pub checkpoints: Vec<ScribeLifecycleCheckpointV1>,
}

impl ScribeProductionEvidenceV1 {
    /// Returns the observation for one named checkpoint.
    #[must_use]
    pub fn checkpoint(&self, name: ScribeCheckpointNameV1) -> Option<&ScribeLifecycleCheckpointV1> {
        self.checkpoints
            .iter()
            .find(|checkpoint| checkpoint.name == name)
    }

    /// Compares observed evidence against everything the record requires.
    ///
    /// This comparator is owned here so that every consumer — the Scribe
    /// candidate, the cache equivalence gate, and Forge — judges the same run
    /// by the same normative fields. A consumer that wrote its own comparator
    /// could pass a run this one refuses.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeWorkloadError::Evidence`] naming the first requirement
    /// the run did not satisfy: a wrong contract version, evidence produced
    /// under a different cache mode than the caller is judging, a repeated or
    /// out-of-order checkpoint, a missing required checkpoint, an
    /// acknowledged-row count that is not what the record declares, a boundary
    /// that requires a read-back and carries none, a read-back digest that is
    /// not the record's, a publication expectation the boundary contradicts, or
    /// a published object whose promotion record cannot rebuild the Iceberg
    /// `DataFile` it claims.
    pub fn assert_matches(
        &self,
        workload: &ScribeProductionWorkloadV1,
        expected_cache_mode: ScribeCacheMode,
    ) -> Result<(), ScribeWorkloadError> {
        if self.version != workload.version {
            return Err(ScribeWorkloadError::Evidence {
                detail: "evidence version does not match the workload it claims".to_owned(),
            });
        }
        // The cache mode is the run's own authority claim. Scribe proves
        // authoritative correctness with the cache absent, so evidence produced
        // with a cache participating may not be presented as the authoritative
        // run of the same bytes.
        if self.cache_mode != expected_cache_mode {
            return Err(ScribeWorkloadError::Evidence {
                detail: format!(
                    "evidence was produced under {:?} but is being judged as {expected_cache_mode:?}",
                    self.cache_mode
                ),
            });
        }
        self.assert_checkpoint_sequence(workload)?;
        let expected_digest = workload.expected_row_digest();
        let expected_rows = workload.expected_rows();
        for required in &workload.required_checkpoints {
            let observed =
                self.checkpoint(*required)
                    .ok_or_else(|| ScribeWorkloadError::Evidence {
                        detail: format!("required checkpoint {required:?} was never reached"),
                    })?;
            if observed.acknowledged_rows != expected_rows {
                return Err(ScribeWorkloadError::Evidence {
                    detail: format!(
                        "checkpoint {required:?} acknowledged {} rows, the record declares {expected_rows}",
                        observed.acknowledged_rows
                    ),
                });
            }
            match &observed.observed_row_digest {
                Some(digest) if digest == &expected_digest => {}
                Some(_) => {
                    return Err(ScribeWorkloadError::Evidence {
                        detail: format!(
                            "checkpoint {required:?} read back a different row set than the record declares"
                        ),
                    });
                }
                None if required.requires_read_back_digest() => {
                    return Err(ScribeWorkloadError::Evidence {
                        detail: format!(
                            "checkpoint {required:?} is only reached by reading rows back, but the run recorded no digest"
                        ),
                    });
                }
                None => {}
            }
            Self::assert_publication(*required, observed)?;
        }
        Ok(())
    }

    /// Refuses a checkpoint sequence that repeats or reorders a boundary.
    ///
    /// [`Self::checkpoint`] resolves a name to its first observation, so a run
    /// that recorded a boundary twice could satisfy the comparator with the
    /// earlier of the two, and a run that reached the boundaries in a different
    /// order than the record declares would be judged as if it had not. Both are
    /// a different run from the one the record describes.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeWorkloadError::Evidence`] when a checkpoint name appears
    /// more than once, or when the required boundaries do not appear in the
    /// record's declared order.
    fn assert_checkpoint_sequence(
        &self,
        workload: &ScribeProductionWorkloadV1,
    ) -> Result<(), ScribeWorkloadError> {
        let mut seen: Vec<ScribeCheckpointNameV1> = Vec::new();
        for observed in &self.checkpoints {
            if seen.contains(&observed.name) {
                return Err(ScribeWorkloadError::Evidence {
                    detail: format!(
                        "checkpoint {:?} was recorded more than once in one run",
                        observed.name
                    ),
                });
            }
            seen.push(observed.name);
        }
        let ordered: Vec<ScribeCheckpointNameV1> = seen
            .into_iter()
            .filter(|name| workload.required_checkpoints.contains(name))
            .collect();
        if ordered != workload.required_checkpoints {
            return Err(ScribeWorkloadError::Evidence {
                detail: format!(
                    "the run reached the required boundaries as {ordered:?}, the record declares {:?}",
                    workload.required_checkpoints
                ),
            });
        }
        Ok(())
    }

    /// Judges one checkpoint's published evidence against its own definition.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeWorkloadError::Evidence`] when a boundary defined by
    /// nothing being published carries a published object, when a boundary
    /// defined by publication carries none, or when a promotion record cannot
    /// rebuild the Iceberg `DataFile` it claims.
    fn assert_publication(
        name: ScribeCheckpointNameV1,
        observed: &ScribeLifecycleCheckpointV1,
    ) -> Result<(), ScribeWorkloadError> {
        let published = observed.published.values().map(Vec::len).sum::<usize>();
        match name.publication_expectation() {
            ScribePublicationExpectationV1::Absent if published > 0 => {
                return Err(ScribeWorkloadError::Evidence {
                    detail: format!(
                        "checkpoint {name:?} is defined by nothing being published, but the run observed {published} objects"
                    ),
                });
            }
            ScribePublicationExpectationV1::Present if published == 0 => {
                return Err(ScribeWorkloadError::Evidence {
                    detail: format!(
                        "checkpoint {name:?} is defined by published hot objects, but the run observed none"
                    ),
                });
            }
            ScribePublicationExpectationV1::Absent
            | ScribePublicationExpectationV1::Present
            | ScribePublicationExpectationV1::Unconstrained => {}
        }
        for records in observed.published.values() {
            for record in records {
                record
                    .data_file()
                    .map_err(|error| ScribeWorkloadError::Evidence {
                        detail: format!("a published promotion record is unusable: {error}"),
                    })?;
            }
        }
        Ok(())
    }
}

/// Why a workload record or its evidence was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScribeWorkloadError {
    /// The record states a contract version this build cannot execute.
    #[error("unsupported Scribe workload version {version}")]
    UnsupportedVersion {
        /// The version the record declared.
        version: u16,
    },
    /// The record is internally inconsistent.
    #[error("malformed Scribe workload: {detail}")]
    Malformed {
        /// What about the record is inconsistent.
        detail: &'static str,
    },
    /// The record's geometry recipe does not validate.
    #[error("Scribe workload geometry is invalid: {0}")]
    Geometry(String),
    /// Observed evidence does not satisfy the record.
    #[error("Scribe production evidence mismatch: {detail}")]
    Evidence {
        /// The first requirement the run did not satisfy.
        detail: String,
    },
}

/// Binds one workload tenant ordinal to the identity the server assigned it.
#[derive(Debug, Clone)]
pub struct ScribeWorkloadTenantBinding {
    /// The tenant the server seeded for this ordinal.
    pub tenant: DataTenantId,
    /// The tenant's declared slug, used to key evidence.
    pub slug: String,
}

/// Runs the canonical workload against one live server through public routes.
impl crate::WyrdTestServer {
    /// Executes one canonical Scribe production workload and returns its evidence.
    ///
    /// Every operation goes through a public surface: tenant seeding and table
    /// registration through the server's own catalog, appends through the
    /// public gRPC ingest route with the record's fixed batch ids, seals
    /// through the tenant-bound flush control, and reads through the public
    /// query route. Nothing here fabricates a row, an object, or a durable
    /// record; the runner only observes what the production path produced.
    ///
    /// `cache_mode` is recorded in the evidence and nothing else: this crate
    /// owns no cache behavior, and the Scribe candidate runs with the cache
    /// absent.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdTestServerError`](crate::WyrdTestServerError) when the
    /// record does not validate, when a tenant, table, append, flush or read
    /// on a public route fails, or when a published promotion record cannot be
    /// read back from its fenced `file_list` row.
    ///
    /// # Panics
    ///
    /// Does not panic; every failure is reported as an error.
    pub async fn run_scribe_production_workload(
        &self,
        workload: &ScribeProductionWorkloadV1,
        cache_mode: ScribeCacheMode,
    ) -> Result<ScribeProductionEvidenceV1, crate::WyrdTestServerError> {
        workload
            .validate()
            .map_err(|error| crate::WyrdTestServerError::Start(error.to_string()))?;

        let mut bindings: Vec<ScribeWorkloadTenantBinding> = Vec::new();
        for (ordinal, declared) in workload.tenants.iter().enumerate() {
            let slug = format!("{}-{}", declared.slug, Uuid::now_v7().simple());
            let tenant = self.seed_tenant(&slug).await?;
            for table in &declared.tables {
                self.create_bifrost_table_for_test(
                    vala_bifrost_redux::catalog::CreateTableRequest {
                        table: vala_bifrost_redux::catalog::TableRef::new(
                            namespace_of(table)?,
                            &table.name,
                        ),
                        user_fields: vec![arrow::datatypes::Field::new(
                            "value",
                            arrow::datatypes::DataType::Int64,
                            false,
                        )],
                        tenant,
                        physical_layout: None,
                        audit: None,
                    },
                )
                .await?;
            }
            bindings.push(ScribeWorkloadTenantBinding {
                tenant,
                slug: declared.slug.clone(),
            });
            debug_assert_eq!(bindings.len(), ordinal + 1);
        }

        let mut acknowledged = 0_u64;
        // Reads observed since the *previous* checkpoint. A checkpoint consumes
        // this buffer, so every digest is evidence of a read that happened
        // after the boundary before it. Carrying reads forward would let one
        // early read vouch for every later boundary, and accumulating repeated
        // full reads would hash duplicated rows into a false mismatch.
        let mut read_since_checkpoint: Vec<(usize, usize, i64)> = Vec::new();
        let mut checkpoints = Vec::new();
        for operation in &workload.operations {
            match operation {
                ScribeWorkloadOperationV1::Append {
                    tenant,
                    table,
                    batch_id,
                    rows,
                } => {
                    let binding = binding_at(&bindings, *tenant)?;
                    let declared = table_at(workload, *tenant, *table)?;
                    self.append_workload_batch(binding.tenant, &declared.fqn(), *batch_id, rows)
                        .await?;
                    acknowledged += rows.len() as u64;
                }
                ScribeWorkloadOperationV1::Flush { tenant } => {
                    let binding = binding_at(&bindings, *tenant)?;
                    self.flush_bifrost_for_tenant(binding.tenant).await?;
                }
                ScribeWorkloadOperationV1::Read { tenant, table } => {
                    let binding = binding_at(&bindings, *tenant)?;
                    let declared = table_at(workload, *tenant, *table)?;
                    for value in self
                        .read_workload_table(binding.tenant, &declared.fqn())
                        .await?
                    {
                        read_since_checkpoint.push((*tenant, *table, value));
                    }
                }
                ScribeWorkloadOperationV1::Checkpoint { name } => {
                    let mut published: BTreeMap<String, Vec<ScribePublishedHotFileV1>> =
                        BTreeMap::new();
                    for (ordinal, binding) in bindings.iter().enumerate() {
                        let mut records = Vec::new();
                        for table in &workload.tenants[ordinal].tables {
                            records.extend(
                                self.published_hot_files_for_test(
                                    binding.tenant,
                                    namespace_of(table)?.as_str(),
                                    &table.name,
                                )
                                .await?
                                .into_iter()
                                .map(|observed| observed.promotion_record),
                            );
                        }
                        published.insert(binding.slug.clone(), records);
                    }
                    checkpoints.push(ScribeLifecycleCheckpointV1 {
                        name: *name,
                        acknowledged_rows: acknowledged,
                        published,
                        observed_row_digest: {
                            let mut observed = std::mem::take(&mut read_since_checkpoint);
                            if observed.is_empty() {
                                None
                            } else {
                                Some(row_digest(&mut observed))
                            }
                        },
                    });
                }
            }
        }

        Ok(ScribeProductionEvidenceV1 {
            version: workload.version,
            cache_mode,
            checkpoints,
        })
    }

    /// Appends one fixed-identity batch through the public gRPC ingest route.
    ///
    /// # Errors
    ///
    /// Returns an error when the tenant credential, Arrow IPC encoding, client
    /// construction, or the public insert fails.
    async fn append_workload_batch(
        &self,
        tenant: DataTenantId,
        table_fqn: &str,
        batch_id: Uuid,
        rows: &[i64],
    ) -> Result<(), crate::WyrdTestServerError> {
        let client = self.workload_client(tenant).await?;
        let schema = std::sync::Arc::new(arrow::datatypes::Schema::new(vec![
            arrow::datatypes::Field::new("value", arrow::datatypes::DataType::Int64, false),
        ]));
        let batch = arrow::record_batch::RecordBatch::try_new(
            std::sync::Arc::clone(&schema),
            vec![std::sync::Arc::new(arrow::array::Int64Array::from(
                rows.to_vec(),
            ))],
        )
        .map_err(|error| crate::WyrdTestServerError::Start(error.to_string()))?;
        let mut ipc = Vec::new();
        let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut ipc, schema.as_ref())
            .map_err(|error| crate::WyrdTestServerError::Start(error.to_string()))?;
        writer
            .write(&batch)
            .and_then(|()| writer.finish())
            .map_err(|error| crate::WyrdTestServerError::Start(error.to_string()))?;
        vala_sdk::grpc::BifrostGrpcTransport::connect(&client)
            .await
            .map_err(|error| crate::WyrdTestServerError::Start(error.to_string()))?
            .insert_batch(table_fqn, batch_id.into_bytes(), ipc)
            .await
            .map_err(|error| crate::WyrdTestServerError::Start(error.to_string()))?;
        Ok(())
    }

    /// Reads one workload table back through the public query route.
    ///
    /// # Errors
    ///
    /// Returns an error when the tenant credential, query start, or batch
    /// streaming fails, or when the returned column is not the declared type.
    async fn read_workload_table(
        &self,
        tenant: DataTenantId,
        table_fqn: &str,
    ) -> Result<Vec<i64>, crate::WyrdTestServerError> {
        let client = self.workload_client(tenant).await?;
        let query = vala_sdk::query::QueryClient::new(&client);
        let mut stream = query
            .query(&wyrd_spec::vala::api::BifrostQueryRequest {
                sql: format!("SELECT value FROM {table_fqn}"),
                visibility: wyrd_spec::vala::api::VisibilityMode::Fused,
                freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
                deadline_ms: Some(60_000),
            })
            .await
            .map_err(|error| {
                crate::WyrdTestServerError::Start(format!("query {table_fqn}: {}", error.detail()))
            })?;
        let mut values = Vec::new();
        while let Some(batch) = stream.next_batch().await.map_err(|error| {
            crate::WyrdTestServerError::Start(format!(
                "read a batch of {table_fqn}: {}",
                error.detail()
            ))
        })? {
            let column = batch
                .column_by_name("value")
                .ok_or_else(|| {
                    crate::WyrdTestServerError::Start("query result has no value column".to_owned())
                })?
                .as_any()
                .downcast_ref::<arrow::array::Int64Array>()
                .ok_or_else(|| {
                    crate::WyrdTestServerError::Start("value column is not Int64".to_owned())
                })?;
            values.extend(column.values().iter().copied());
        }
        Ok(values)
    }

    /// Builds one authenticated SDK client bound to a workload tenant.
    ///
    /// # Errors
    ///
    /// Returns an error when the service principal, its API key, or the
    /// server's public endpoints are unavailable.
    async fn workload_client(
        &self,
        tenant: DataTenantId,
    ) -> Result<wyrd_client::WyrdClient, crate::WyrdTestServerError> {
        let bootstrap = self
            .bootstrap_service_in_tenant(
                tenant,
                &format!("scribe-workload-{}", Uuid::now_v7().simple()),
                &["admin"],
            )
            .await?;
        let api_key = bootstrap.api_key().cloned().ok_or_else(|| {
            crate::WyrdTestServerError::Auth("workload requires a service key".to_owned())
        })?;
        wyrd_client::WyrdClient::with_config(wyrd_client::config::ClientConfig {
            grpc: wyrd_client::transport::GrpcConfig {
                endpoint: self.grpc_url().ok_or_else(|| {
                    crate::WyrdTestServerError::Start("missing gRPC URL".to_owned())
                })?,
                connect_retries: 0,
                ..wyrd_client::transport::GrpcConfig::default()
            },
            http: wyrd_client::transport::HttpConfig {
                base_url: self
                    .base_url()
                    .ok_or_else(|| {
                        crate::WyrdTestServerError::Start("missing HTTP URL".to_owned())
                    })?
                    .to_owned(),
                ..wyrd_client::transport::HttpConfig::default()
            },
            api_key: Some(api_key),
            ..wyrd_client::config::ClientConfig::default()
        })
        .map_err(|error| crate::WyrdTestServerError::Start(error.to_string()))
    }
}

/// Resolves a declared table's namespace segment to the canonical enum.
///
/// # Errors
///
/// Returns an error when the record names a namespace this build does not own.
fn namespace_of(
    table: &ScribeWorkloadTableV1,
) -> Result<vala_bifrost_redux::namespaces::BifrostNamespace, crate::WyrdTestServerError> {
    vala_bifrost_redux::namespaces::BifrostNamespace::from_domain_namespace(&table.namespace)
        .ok_or_else(|| {
            crate::WyrdTestServerError::Start(format!(
                "workload names unknown namespace `{}`",
                table.namespace
            ))
        })
}

/// Returns the tenant binding for one ordinal.
///
/// # Errors
///
/// Returns an error when the ordinal has no binding, which validation should
/// already have refused.
fn binding_at(
    bindings: &[ScribeWorkloadTenantBinding],
    ordinal: usize,
) -> Result<&ScribeWorkloadTenantBinding, crate::WyrdTestServerError> {
    bindings.get(ordinal).ok_or_else(|| {
        crate::WyrdTestServerError::Start("workload names an unbound tenant".to_owned())
    })
}

/// Returns the declared table for one tenant/table ordinal pair.
///
/// # Errors
///
/// Returns an error when either ordinal is outside the record.
fn table_at(
    workload: &ScribeProductionWorkloadV1,
    tenant: usize,
    table: usize,
) -> Result<&ScribeWorkloadTableV1, crate::WyrdTestServerError> {
    workload
        .tenants
        .get(tenant)
        .and_then(|declared| declared.tables.get(table))
        .ok_or_else(|| {
            crate::WyrdTestServerError::Start("workload names an undeclared table".to_owned())
        })
}

#[cfg(test)]
mod tests {
    use vala_bifrost_redux::catalog::layout::{
        BIFROST_PARTITION_SPEC_ID, BIFROST_SORT_ORDER_ID, TimeGranularity, TimePartition,
    };
    use vala_bifrost_redux::scribe::promotion::{PublishedHotFileIdentity, ScribeDataFileV1};

    use super::*;

    /// AC22/AC23 unit owner: the canonical workload survives its own wire form
    /// and its evidence comparator is exact.
    ///
    /// The record is a handoff: Scribe, the cache task and Forge all run the
    /// same serialized bytes. If a field only exists in the producing process,
    /// or if the comparator accepts a run that lost rows or skipped a
    /// checkpoint, the handoff proves nothing. This owner round-trips the
    /// canonical record, proves the digest and row count are properties of the
    /// record rather than of an execution, accepts an exactly-conforming
    /// evidence value, and refuses each way a run can fall short.
    ///
    /// The refusals are the substance. A comparator that only counted rows
    /// would accept a run that never read anything back, that recorded a
    /// boundary twice, that reached the boundaries in another order, that ran
    /// with a cache participating, or that arrived at a publication boundary
    /// having published nothing — every one of which is a different run from the
    /// one the record describes.
    ///
    /// # Panics
    ///
    /// Panics when the record does not survive serialization, when conforming
    /// evidence is refused, or when short, silent, misread, misordered,
    /// duplicated, cache-assisted or unpublished evidence is accepted.
    #[test]
    fn scribe_production_workload_v1_round_trips_and_evidence_is_exact() {
        let workload = ScribeProductionWorkloadV1::canonical();
        workload.validate().expect("the canonical record validates");

        // Round trip: nothing about the record is process-local.
        let encoded = serde_json::to_vec(&workload).expect("the record serializes");
        let decoded: ScribeProductionWorkloadV1 =
            serde_json::from_slice(&encoded).expect("the record deserializes");
        assert_eq!(decoded, workload, "the wire form must be lossless");
        assert_eq!(
            decoded.expected_row_digest(),
            workload.expected_row_digest(),
            "the digest is a property of the record, not of the producing process"
        );
        assert_eq!(decoded.expected_rows(), workload.expected_rows());
        assert!(
            workload.expected_rows() > 0,
            "a workload that declares no rows proves nothing"
        );
        assert_eq!(
            decoded.geometry.resolve().expect("geometry resolves"),
            workload.geometry.resolve().expect("geometry resolves")
        );

        // An unknown version fails closed rather than being executed anyway.
        let mut future = workload.clone();
        future.version = SCRIBE_PRODUCTION_WORKLOAD_VERSION + 1;
        assert_eq!(
            future
                .validate()
                .expect_err("an unknown version is refused"),
            ScribeWorkloadError::UnsupportedVersion {
                version: SCRIBE_PRODUCTION_WORKLOAD_VERSION + 1
            }
        );

        // A conforming run is accepted. Each checkpoint carries exactly what its
        // own definition requires: a read-back digest where the boundary is
        // reached by reading, and published objects where the boundary is
        // defined by publication.
        let conforming = ScribeProductionEvidenceV1 {
            version: workload.version,
            cache_mode: ScribeCacheMode::Disabled,
            checkpoints: workload
                .required_checkpoints
                .iter()
                .map(|name| ScribeLifecycleCheckpointV1 {
                    name: *name,
                    acknowledged_rows: workload.expected_rows(),
                    published: conforming_publication(*name),
                    observed_row_digest: if name.requires_read_back_digest() {
                        Some(workload.expected_row_digest())
                    } else {
                        None
                    },
                })
                .collect(),
        };
        conforming
            .assert_matches(&workload, ScribeCacheMode::Disabled)
            .expect("an exactly conforming run is accepted");

        // A missing checkpoint is refused.
        let mut short = conforming.clone();
        short.checkpoints.pop();
        assert!(
            short
                .assert_matches(&workload, ScribeCacheMode::Disabled)
                .is_err(),
            "a run that never reached a required checkpoint must be refused"
        );

        // A lost row is refused.
        let mut lossy = conforming.clone();
        lossy.checkpoints[0].acknowledged_rows -= 1;
        assert!(
            lossy
                .assert_matches(&workload, ScribeCacheMode::Disabled)
                .is_err(),
            "a run that acknowledged fewer rows than the record declares must be refused"
        );

        // A read-back of different rows is refused even when the count matches.
        let mut misread = conforming.clone();
        let reading = misread
            .checkpoints
            .iter()
            .position(|checkpoint| checkpoint.name.requires_read_back_digest())
            .expect("the canonical record requires at least one read-back boundary");
        misread.checkpoints[reading].observed_row_digest = Some("0".repeat(64));
        assert!(
            misread
                .assert_matches(&workload, ScribeCacheMode::Disabled)
                .is_err(),
            "a run that read back a different row set must be refused"
        );

        // A boundary reached without reading anything back is refused. A silent
        // run is the failure this comparator exists to catch: it would otherwise
        // satisfy every count while proving nothing about row identity.
        let mut silent = conforming.clone();
        silent.checkpoints[reading].observed_row_digest = None;
        assert!(
            silent
                .assert_matches(&workload, ScribeCacheMode::Disabled)
                .is_err(),
            "a read-back boundary reached with no digest must be refused"
        );

        // A publication boundary that published nothing is refused, and a
        // pre-publication boundary that published something is too: each
        // boundary is defined by that fact, in one direction or the other.
        let mut unpublished = conforming.clone();
        let publishing = unpublished
            .checkpoints
            .iter()
            .position(|checkpoint| {
                checkpoint.name.publication_expectation() == ScribePublicationExpectationV1::Present
            })
            .expect("the canonical record requires at least one publication boundary");
        unpublished.checkpoints[publishing].published = BTreeMap::new();
        assert!(
            unpublished
                .assert_matches(&workload, ScribeCacheMode::Disabled)
                .is_err(),
            "a publication boundary observing no object must be refused"
        );
        let mut early = conforming.clone();
        let pre_publication = early
            .checkpoints
            .iter()
            .position(|checkpoint| {
                checkpoint.name.publication_expectation() == ScribePublicationExpectationV1::Absent
            })
            .expect("the canonical record requires at least one pre-publication boundary");
        early.checkpoints[pre_publication].published =
            conforming_publication(early.checkpoints[publishing].name);
        assert!(
            early
                .assert_matches(&workload, ScribeCacheMode::Disabled)
                .is_err(),
            "an object published before the acknowledgement boundary must be refused"
        );

        // A boundary recorded twice is refused: the comparator resolves a name
        // to its first observation, so a duplicate would let the earlier of two
        // contradictory observations stand in for the run.
        let mut duplicated = conforming.clone();
        duplicated
            .checkpoints
            .push(conforming.checkpoints[0].clone());
        assert!(
            duplicated
                .assert_matches(&workload, ScribeCacheMode::Disabled)
                .is_err(),
            "one run may record each boundary only once"
        );

        // Boundaries reached in another order are a different lifecycle: rows
        // published before they were acknowledged is not the run the record
        // describes, even when every count agrees.
        let mut reordered = conforming.clone();
        reordered.checkpoints.reverse();
        assert!(
            reordered
                .assert_matches(&workload, ScribeCacheMode::Disabled)
                .is_err(),
            "the required boundaries must be reached in the record's declared order"
        );

        // Evidence produced with a cache participating is not the authoritative
        // run, and may not be presented as one.
        let mut cached = conforming.clone();
        cached.cache_mode = ScribeCacheMode::Enabled;
        assert!(
            cached
                .assert_matches(&workload, ScribeCacheMode::Disabled)
                .is_err(),
            "a cache-assisted run may not be judged as the authoritative one"
        );
        cached
            .assert_matches(&workload, ScribeCacheMode::Enabled)
            .expect("the same evidence is accepted when judged as the run it was");

        // Evidence claiming another contract version is refused.
        let mut foreign = conforming;
        foreign.version = SCRIBE_PRODUCTION_WORKLOAD_VERSION + 1;
        assert!(
            foreign
                .assert_matches(&workload, ScribeCacheMode::Disabled)
                .is_err(),
            "evidence must name the contract version it was produced under"
        );
    }

    /// AC22 unit owner: every read-required boundary reads back for itself.
    ///
    /// A boundary such as `RestartReplay` or `TerminalDrain` claims the rows
    /// are still exactly readable *after* that transition. The runner binds
    /// each checkpoint's digest to the reads observed since the previous
    /// checkpoint, so an earlier read cannot vouch for a later boundary. This
    /// owner proves the record refuses the authoring shape that would ask it
    /// to: two read-required boundaries in a row with only one read between
    /// them, and a read-required boundary with no read before it at all.
    ///
    /// # Panics
    ///
    /// Panics when the canonical record is refused, when a record whose
    /// read-required boundary reuses an earlier read is accepted, or when
    /// inserting the missing read does not make the record valid again.
    #[test]
    fn scribe_workload_read_boundaries_may_not_reuse_an_earlier_read() {
        let workload = ScribeProductionWorkloadV1::canonical();
        workload
            .validate()
            .expect("the canonical record reads before its read-required boundary");

        // A read-required boundary with no read of its own is refused, even
        // though an earlier boundary in the same run was read for.
        let mut stale = workload.clone();
        stale
            .operations
            .push(ScribeWorkloadOperationV1::Checkpoint {
                name: ScribeCheckpointNameV1::TerminalDrain,
            });
        assert!(
            matches!(stale.validate(), Err(ScribeWorkloadError::Malformed { .. })),
            "a second read-required boundary may not reuse the read the first consumed"
        );

        // Giving that boundary its own read makes the same record valid.
        let mut fresh = workload.clone();
        fresh.operations.push(ScribeWorkloadOperationV1::Read {
            tenant: 0,
            table: 0,
        });
        fresh
            .operations
            .push(ScribeWorkloadOperationV1::Checkpoint {
                name: ScribeCheckpointNameV1::TerminalDrain,
            });
        fresh
            .validate()
            .expect("a read-required boundary with its own read is well formed");

        // A read-required boundary reached before any read is refused.
        let mut unread = workload;
        unread
            .operations
            .retain(|operation| !matches!(operation, ScribeWorkloadOperationV1::Read { .. }));
        assert!(
            matches!(
                unread.validate(),
                Err(ScribeWorkloadError::Malformed { .. })
            ),
            "a read-required boundary reached with no read at all is not evidence"
        );
    }

    /// Returns publication evidence satisfying one boundary's own expectation.
    ///
    /// Boundaries defined by publication need a real promotion record, and one
    /// that round-trips its Iceberg projection, or the comparator would refuse
    /// the conforming case for the wrong reason.
    ///
    /// # Panics
    ///
    /// Panics when the fixture promotion record does not rebuild its own data
    /// file, which would be a fixture bug rather than comparator behavior.
    fn conforming_publication(
        name: ScribeCheckpointNameV1,
    ) -> BTreeMap<String, Vec<ScribePublishedHotFileV1>> {
        if name.publication_expectation() != ScribePublicationExpectationV1::Present {
            return BTreeMap::new();
        }
        let record = fixture_published_hot_file();
        record
            .data_file()
            .expect("the fixture promotion record rebuilds its data file");
        BTreeMap::from([("scribe-workload-0".to_owned(), vec![record])])
    }

    /// Builds one minimal, self-consistent promotion record.
    ///
    /// The record carries only what makes it rebuildable — a canonical
    /// partition boundary, a real row count and object size, and one ascending
    /// split offset. Column statistics are deliberately absent: this fixture
    /// exists to let the comparator see *a* published object, and the exactness
    /// of footer statistics is owned by the writer's own test, not by this one.
    fn fixture_published_hot_file() -> ScribePublishedHotFileV1 {
        let partition = TimePartition::new(
            TimeGranularity::Hour,
            chrono::DateTime::from_timestamp(0, 0).expect("the epoch is a representable instant"),
        )
        .expect("the epoch is a canonical hour boundary");
        ScribePublishedHotFileV1::from_metrics(
            &PublishedHotFileIdentity {
                data_tenant_id: Uuid::nil(),
                namespace: "wyrd",
                table_name: "scribe_workload",
                file_list_id: Uuid::nil(),
                object_key: "scribe/workload/000.parquet",
                file_checksum: &"0".repeat(64),
                partition,
                schema_fingerprint: "0".repeat(64),
                partition_spec_id: BIFROST_PARTITION_SPEC_ID,
                sort_order_id: BIFROST_SORT_ORDER_ID,
            },
            ScribeDataFileV1 {
                record_count: 1,
                file_size_in_bytes: 1_024,
                column_sizes: BTreeMap::new(),
                value_counts: BTreeMap::new(),
                null_value_counts: BTreeMap::new(),
                nan_value_counts: BTreeMap::new(),
                lower_bounds: BTreeMap::new(),
                upper_bounds: BTreeMap::new(),
                split_offsets: vec![4],
            },
        )
    }
}
