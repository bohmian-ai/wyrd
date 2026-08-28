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
                        batch_id: Uuid::from_u128(u128::from(seed) << 32 | u128::from(ordinal)),
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
    /// owner the record does not declare, and
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
    /// the run did not satisfy: a wrong contract version, a missing required
    /// checkpoint, an acknowledged-row count below what the record declares, a
    /// read-back digest that is not the record's, or a published object whose
    /// promotion record cannot rebuild the Iceberg `DataFile` it claims.
    pub fn assert_matches(
        &self,
        workload: &ScribeProductionWorkloadV1,
    ) -> Result<(), ScribeWorkloadError> {
        if self.version != workload.version {
            return Err(ScribeWorkloadError::Evidence {
                detail: "evidence version does not match the workload it claims".to_owned(),
            });
        }
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
            if observed
                .observed_row_digest
                .as_ref()
                .is_some_and(|digest| digest != &expected_digest)
            {
                return Err(ScribeWorkloadError::Evidence {
                    detail: format!(
                        "checkpoint {required:?} read back a different row set than the record declares"
                    ),
                });
            }
            for records in observed.published.values() {
                for published in records {
                    published
                        .data_file()
                        .map_err(|error| ScribeWorkloadError::Evidence {
                            detail: format!("a published promotion record is unusable: {error}"),
                        })?;
                }
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
        let mut read_back: Vec<(usize, usize, i64)> = Vec::new();
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
                        read_back.push((*tenant, *table, value));
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
                        observed_row_digest: if read_back.is_empty() {
                            None
                        } else {
                            Some(row_digest(&mut read_back.clone()))
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
            .map_err(|error| crate::WyrdTestServerError::Start(error.to_string()))?;
        let mut values = Vec::new();
        while let Some(batch) = stream
            .next_batch()
            .await
            .map_err(|error| crate::WyrdTestServerError::Start(error.to_string()))?
        {
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
    /// # Panics
    ///
    /// Panics when the record does not survive serialization, when conforming
    /// evidence is refused, or when short, silent or misread evidence is
    /// accepted.
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

        // A conforming run is accepted.
        let conforming = ScribeProductionEvidenceV1 {
            version: workload.version,
            cache_mode: ScribeCacheMode::Disabled,
            checkpoints: workload
                .required_checkpoints
                .iter()
                .map(|name| ScribeLifecycleCheckpointV1 {
                    name: *name,
                    acknowledged_rows: workload.expected_rows(),
                    published: BTreeMap::new(),
                    observed_row_digest: Some(workload.expected_row_digest()),
                })
                .collect(),
        };
        conforming
            .assert_matches(&workload)
            .expect("an exactly conforming run is accepted");

        // A missing checkpoint is refused.
        let mut short = conforming.clone();
        short.checkpoints.pop();
        assert!(
            short.assert_matches(&workload).is_err(),
            "a run that never reached a required checkpoint must be refused"
        );

        // A lost row is refused.
        let mut lossy = conforming.clone();
        lossy.checkpoints[0].acknowledged_rows -= 1;
        assert!(
            lossy.assert_matches(&workload).is_err(),
            "a run that acknowledged fewer rows than the record declares must be refused"
        );

        // A read-back of different rows is refused even when the count matches.
        let mut misread = conforming.clone();
        misread.checkpoints[0].observed_row_digest = Some("0".repeat(64));
        assert!(
            misread.assert_matches(&workload).is_err(),
            "a run that read back a different row set must be refused"
        );

        // Evidence claiming another contract version is refused.
        let mut foreign = conforming;
        foreign.version = SCRIBE_PRODUCTION_WORKLOAD_VERSION + 1;
        assert!(
            foreign.assert_matches(&workload).is_err(),
            "evidence must name the contract version it was produced under"
        );
    }
}
