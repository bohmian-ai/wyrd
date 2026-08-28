//! Pod-level owner of the staged-member lifecycle.
//!
//! Freezing a bucket, making it durable, deciding when its rows are worth
//! publishing, merging them with their siblings, and retiring what they
//! replace are five steps with five different owners
//! ([`ScribeMemberStager`], [`ScribeHotStage`], [`StagingAssembler`],
//! [`ClaimAssembler`], [`ClaimPublisher`]). Each is independently testable and
//! none of them knows the order the others run in. This module is the one
//! owner that does: it holds the pod's single staged namespace, its single
//! ready index, and the resolved encoding context each key was staged under,
//! and it exposes the lifecycle as the operations a persistence worker
//! actually performs.
//!
//! The split between blocking and async methods is deliberate and load
//! bearing. [`ScribeStagingRuntime::encode_member`] and
//! [`ScribeStagingRuntime::assemble`] are the CPU-bound halves and belong on
//! the persistence CPU lane; every other method awaits durable IO. The runtime
//! does not own that lane, so it cannot silently move encoding onto a runtime
//! thread that a caller expected to keep responsive.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use arrow::datatypes::SchemaRef;
use chrono::{DateTime, Utc};

use crate::catalog::TenantTableBinding;
use crate::catalog::layout::PhysicalLayout;
use crate::contracts::ScribeError;
use crate::scribe::assembly::{
    ClaimCause, ScribeAssemblyKey, StagingAssembler, StagingAssemblerConfig, StagingClaim,
    StagingClaimId,
};
use crate::scribe::claim_assembly::{
    AssembleClaimRequest, AssembledClaim, ClaimAssembler, ClaimRuns,
};
use crate::scribe::claim_publication::{ClaimPublisher, PublishClaimRequest, PublishedClaim};
use crate::scribe::hot_stage::ScribeHotStage;
use crate::scribe::member_stager::{ScribeMemberStager, StageMemberRequest, StagedRuns};
use crate::scribe::seal_key::ScribeClaimIdentity;
use crate::scribe::stream_identity::StreamIdentity;

/// Everything a claim needs that its key records only as a fingerprint.
///
/// A [`ScribeAssemblyKey`] fixes *which* schema and layout a member was staged
/// under, but it stores digests, not the schema and recipe an encoder needs.
/// Rather than re-resolve the registered recipe at merge time — where a table
/// re-registration would silently change the sort order rows are merged in —
/// the runtime keeps the exact context each key was staged under and merges
/// under that.
#[derive(Debug, Clone)]
pub struct ClaimContext {
    /// Physical schema every member of the key was encoded against.
    pub schema: SchemaRef,
    /// Registered write recipe fixing sort order and partitioning.
    pub layout: PhysicalLayout,
    /// Tenant-qualified binding the merged objects publish under.
    pub binding: TenantTableBinding,
}

/// Borrowed inputs for assembling one claim into sealed objects.
pub struct AssembleRequest<'a> {
    /// Runs gathered and re-validated for the claim.
    pub runs: &'a ClaimRuns,
    /// Directory receiving the sealed objects before upload.
    pub scratch_dir: &'a Path,
    /// Move-only footer memory child retained through sealed inspection.
    pub footer_reservation: crate::scribe::memory::EncodedFooterReservation,
}

/// Owner of one pod's staged members, ready index, and claim lifecycle.
pub struct ScribeStagingRuntime {
    /// Encoder and durable-charge owner for freshly frozen buckets.
    stager: ScribeMemberStager,
    /// Merge owner that re-validates a claim's members before encoding them.
    claims: ClaimAssembler,
    /// Fenced publication and member-retirement owner.
    publisher: ClaimPublisher,
    /// Tenant-fair ready index deciding which members become one object.
    assembly: Mutex<StagingAssembler>,
    /// Encoding context recorded for every key that has staged a member.
    contexts: Mutex<HashMap<ScribeAssemblyKey, ClaimContext>>,
    /// Approximate encoded size at which one published object closes.
    target_object_bytes: u64,
}

impl ScribeStagingRuntime {
    /// Composes the pod's staged lifecycle over one staged namespace.
    #[must_use]
    pub fn new(
        stage: Arc<ScribeHotStage>,
        volume: crate::resources::StageVolume,
        publisher: ClaimPublisher,
        config: StagingAssemblerConfig,
    ) -> Self {
        Self {
            stager: ScribeMemberStager::new(Arc::clone(&stage), volume),
            claims: ClaimAssembler::new(stage),
            publisher,
            assembly: Mutex::new(StagingAssembler::new(config)),
            contexts: Mutex::new(HashMap::new()),
            target_object_bytes: config.target_file_size_bytes(),
        }
    }

    /// Encodes one frozen bucket into durable, preflighted local runs.
    ///
    /// This is the blocking half of staging and belongs on the persistence CPU
    /// lane. The context is recorded before the runs are durable rather than
    /// after, so a member that becomes ready can never be claimed under a key
    /// whose schema and layout the runtime cannot name.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the context registry is
    /// unavailable or the member cannot be encoded, fsynced, preflighted, or
    /// charged against the staging volume.
    pub fn encode_member(
        &self,
        request: StageMemberRequest<'_>,
        context: ClaimContext,
    ) -> Result<StagedRuns, ScribeError> {
        let staged = self.stager.encode_runs(request)?;
        self.contexts
            .lock()
            .map_err(|_| poisoned("staged claim context registry"))?
            .insert(staged.key().clone(), context);
        Ok(staged)
    }

    /// Publishes the record that makes a staged member durable and claimable.
    ///
    /// Returning is the boundary the WAL retires against: the rows survive and
    /// are servable without the segments behind them. The member joins the
    /// ready index only after that record exists, so a crash between the two
    /// leaves a durable member that recovery re-registers rather than a claim
    /// over rows no record names.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the record cannot be published
    /// durably, the ready index is unavailable, or the member is already
    /// owned — registering it twice would publish its rows twice.
    pub async fn register_member(
        &self,
        staged: StagedRuns,
        ready_at: DateTime<Utc>,
    ) -> Result<(), ScribeError> {
        let key = staged.key().clone();
        let ready = self.stager.publish_ready(staged, ready_at).await?;
        self.assembly
            .lock()
            .map_err(|_| poisoned("staged ready index"))?
            .register_ready(&key, ready)
            .map_err(|error| ScribeError::Internal {
                detail: format!("register the staged member as ready: {error}"),
            })
    }

    /// Takes the next claim any tenant is entitled to, if one is due.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the ready index is unavailable or
    /// a key is due while every claim slot is held. A full budget with nothing
    /// due is not an error; it returns `Ok(None)` like any other idle poll.
    pub fn take_claim(&self, now: DateTime<Utc>) -> Result<Option<StagingClaim>, ScribeError> {
        self.assembly
            .lock()
            .map_err(|_| poisoned("staged ready index"))?
            .next_claim(now)
            .map_err(|error| ScribeError::Internal {
                detail: format!("take the next due staging claim: {error}"),
            })
    }

    /// Takes every ready member of one key as a residue claim.
    ///
    /// Used when waiting for target can no longer pay for itself: the partition
    /// closed, the pod is draining, or pressure requires the staged bytes back.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the ready index is unavailable or
    /// every claim slot is already held.
    pub fn take_residue(
        &self,
        key: &ScribeAssemblyKey,
        cause: ClaimCause,
    ) -> Result<Option<StagingClaim>, ScribeError> {
        self.assembly
            .lock()
            .map_err(|_| poisoned("staged ready index"))?
            .claim_residue(key, cause)
            .map_err(|error| ScribeError::Internal {
                detail: format!("take the residue claim for a staged key: {error}"),
            })
    }

    /// Re-validates a claim's members and returns its runs in merge order.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a member's record is missing or
    /// contradicts the bytes on the volume.
    pub async fn gather(&self, claim: &StagingClaim) -> Result<ClaimRuns, ScribeError> {
        self.claims.gather(claim).await
    }

    /// Merges one claim's runs into rolling sealed objects.
    ///
    /// This is the blocking half of assembly and belongs on the persistence CPU
    /// lane. Objects roll at the configured target, so a claim that overshoots
    /// its target publishes several objects rather than one oversized one.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the key has no recorded encoding
    /// context, a run cannot be decoded, or the merge writes a different number
    /// of rows than the claim's members promised.
    pub fn assemble(
        &self,
        claim: &StagingClaim,
        request: AssembleRequest<'_>,
    ) -> Result<AssembledClaim, ScribeError> {
        let context = self.context_for(claim.key())?;
        let object_base = claim_object_base(claim, request.runs, &context)?;
        self.claims.encode(AssembleClaimRequest {
            runs: request.runs,
            schema: Arc::clone(&context.schema),
            layout: &context.layout,
            scratch_dir: request.scratch_dir,
            object_base: &object_base,
            target_object_bytes: self.target_object_bytes,
            footer_reservation: request.footer_reservation,
        })
    }

    /// Publishes one assembled claim, retires its members, and settles it.
    ///
    /// Settlement is last and deliberately so: the claim slot returns to the
    /// budget only after the members are published, retired, and their staged
    /// bytes released, which is what keeps a restarted pod from claiming rows
    /// an interrupted publication may already have written.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the key has no recorded context,
    /// publication fails or is uncertain, the ready index is unavailable, or
    /// the released staged bytes cannot be returned to the governed volume.
    pub async fn publish(
        &self,
        claim: &StagingClaim,
        runs: &ClaimRuns,
        assembled: &AssembledClaim,
        actor_stream: StreamIdentity,
    ) -> Result<PublishedClaim, ScribeError> {
        let context = self.context_for(claim.key())?;
        let object_base = claim_object_base(claim, runs, &context)?;
        let published = self
            .publisher
            .publish(PublishClaimRequest {
                claim,
                runs,
                assembled,
                binding: &context.binding,
                object_base: &object_base,
                actor_stream,
            })
            .await?;
        self.settle(claim.id(), published.released_bytes)?;
        Ok(published)
    }

    /// Returns the claim slot and the staged bytes a settled claim released.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the ready index is unavailable,
    /// the claim is unknown, or reconciled staged ownership cannot cover the
    /// released length.
    fn settle(&self, claim: StagingClaimId, released_bytes: u64) -> Result<(), ScribeError> {
        self.assembly
            .lock()
            .map_err(|_| poisoned("staged ready index"))?
            .settle_claim(claim)
            .map_err(|error| ScribeError::Internal {
                detail: format!("settle a published staging claim: {error}"),
            })?;
        self.stager.release_staged_bytes(released_bytes)
    }

    /// Returns the encoding context recorded when the key first staged a member.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the registry is unavailable or
    /// holds no context for the key. A claim over members this pod never staged
    /// is left alone rather than merged under a re-resolved recipe that may no
    /// longer be the one its rows were written with.
    fn context_for(&self, key: &ScribeAssemblyKey) -> Result<ClaimContext, ScribeError> {
        self.contexts
            .lock()
            .map_err(|_| poisoned("staged claim context registry"))?
            .get(key)
            .cloned()
            .ok_or_else(|| ScribeError::Internal {
                detail: "staged assembly key carries no recorded encoding context".to_owned(),
            })
    }
}

impl std::fmt::Debug for ScribeStagingRuntime {
    /// Names the runtime without exposing its pooled or object-store owners.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScribeStagingRuntime")
            .field("target_object_bytes", &self.target_object_bytes)
            .finish_non_exhaustive()
    }
}

/// Builds the deterministic object base one claim's objects share.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the key's node identity is not a
/// UUID or the claim's WAL union is reversed.
fn claim_object_base(
    claim: &StagingClaim,
    runs: &ClaimRuns,
    context: &ClaimContext,
) -> Result<String, ScribeError> {
    let wal = runs.wal();
    Ok(ScribeClaimIdentity::new(
        &context.binding,
        claim.key().partition(),
        &claim.key().node_id().to_string(),
        claim.key().writer_epoch().as_i64(),
        &claim.id(),
        wal.min,
        wal.max,
    )?
    .object_base())
}

/// Builds the failure describing one poisoned staged-lifecycle owner.
///
/// A poisoned lock means a previous caller panicked while holding staged
/// ownership, so the index or registry may name members that no longer match
/// the volume. Refusing is the conservative outcome: the WAL behind those
/// members is still authoritative.
fn poisoned(owner: &'static str) -> ScribeError {
    ScribeError::Internal {
        detail: format!("{owner} is poisoned and cannot be trusted"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{
        FixedSizeBinaryArray, Int32Array, RecordBatch, StringArray, TimestampMicrosecondArray,
    };
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use wyrd_spec::ids::DataTenantId;

    use crate::catalog::TableRef;
    use crate::namespaces::BifrostNamespace;
    use crate::scribe::assembly::StagedMemberId;
    use crate::scribe::hot_stage::StagedLsnRange;
    use crate::scribe::member_stager::StagedMemberOrigin;
    use crate::scribe::memtable::FrozenMemtable;
    use crate::scribe::persistence::{
        PersistenceFaults, ScribePublicationReconciler, ScribeStageMover,
    };
    use crate::scribe::seal_key::SealKey;
    use crate::scribe::stream_identity::{NodeId, WriterEpoch};
    use crate::scribe::wal::{WalConfig, WalWriter};

    /// Physical schema the fixture member is staged and merged under.
    fn runtime_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("data_tenant_id", DataType::Utf8, false),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
            Field::new("wyrd_row_ordinal", DataType::Int32, false),
        ]))
    }

    /// Freezes one bucket of `rows` rows for the fixture tenant and shard.
    fn frozen_member(tenant: DataTenantId, rows: i64, shard: u8) -> FrozenMemtable {
        let schema = runtime_schema();
        let tenant_value = tenant.to_string();
        let count = usize::try_from(rows).expect("fixture row count fits usize");
        let record = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(StringArray::from(vec![tenant_value.as_str(); count])),
                Arc::new(TimestampMicrosecondArray::from_iter_values(
                    (0..rows).map(|row| row * 2 + i64::from(shard)),
                )),
                Arc::new(
                    FixedSizeBinaryArray::try_from_iter((0..rows).map(|_| [shard; 16]))
                        .expect("fixture batch identity"),
                ),
                Arc::new(Int32Array::from_iter_values(
                    (0..rows).map(|row| i32::try_from(row).unwrap_or(i32::MAX)),
                )),
            ],
        )
        .expect("fixture member batch");
        FrozenMemtable {
            seal_id: u64::from(shard),
            seal_key: SealKey::new(
                tenant,
                TableRef::new(BifrostNamespace::Bifrost, "staged_runtime"),
                crate::test_support::day_partition(2026, 7, 14),
            ),
            shard_id: usize::from(shard),
            schema,
            batches: vec![record],
            events: vec![],
            metas: vec![],
            opened_at: std::time::Instant::now(),
            closed_at: std::time::Instant::now(),
            arrow_bytes: 0,
        }
    }

    /// Resolves the hourly layout the fixture member is encoded under.
    fn runtime_layout(schema: &Schema) -> PhysicalLayout {
        PhysicalLayout::resolve(
            "vala.bifrost.test",
            schema,
            Some(&crate::tables::hourly_layout(
                vec![crate::tables::sort_asc("wyrd_event_time")],
                &["wyrd_event_time"],
            )),
        )
        .expect("fixture schema carries the built-in hourly layout")
    }

    /// Registers a governed staging volume rooted at the stage's own root.
    fn staging_volume(base: &Path, stage_root: &Path) -> crate::resources::StageVolume {
        let wal = base.join("wal");
        let scribe_output = base.join("scribe-output-scratch");
        let forge = base.join("forge");
        let oracle = base.join("oracle");
        for path in [&wal, &scribe_output, &forge, &oracle] {
            std::fs::create_dir_all(path).expect("registered volume root");
        }
        crate::resources::BifrostVolumeGovernor::register(
            crate::resources::BifrostVolumeRoots {
                wal,
                scribe_stage: stage_root.to_owned(),
                scribe_output_scratch: scribe_output,
                forge_scratch: forge,
                oracle_scratch: oracle,
            },
            1024 * 1024 * 1024,
            crate::resources::BifrostResourceHealth::default(),
        )
        .expect("staging volume registration")
        .capabilities()
        .scribe_stage
    }

    /// Builds the publisher the runtime owns, over lazy and in-memory owners.
    ///
    /// The fixture never reaches the fenced transaction, so the pool is opened
    /// lazily and never connected: what the test exercises is the lifecycle up
    /// to assembly, which is exactly the part that owns no durable catalog.
    fn publisher(stage: Arc<ScribeHotStage>, wal_root: &Path, node: NodeId) -> ClaimPublisher {
        let wal = Arc::new(
            WalWriter::new(wal_root, *node.as_bytes(), 1, WalConfig::default())
                .expect("fixture WAL writer"),
        );
        let operator = opendal::Operator::new(opendal::services::Memory::default())
            .expect("memory operator")
            .finish();
        ClaimPublisher::new(
            stage,
            ScribeStageMover::new(wal, operator),
            ScribePublicationReconciler::new(
                sqlx::PgPool::connect_lazy("postgres://unused/unused")
                    .expect("lazy pool")
                    .into(),
                StreamIdentity::new(node, WriterEpoch::new(1)),
                PersistenceFaults::default(),
            ),
        )
    }

    /// A frozen bucket becomes a durable member, that member becomes a claim
    /// this runtime can merge under the context it was staged with, and taking
    /// the claim removes the member from the ready index so no second claim can
    /// publish the same rows.
    #[tokio::test]
    async fn a_staged_member_becomes_a_claim_the_runtime_assembles() {
        let root = tempfile::tempdir().expect("runtime root");
        let stage_root = root.path().join("stage");
        let wal_root = root.path().join("member-wal");
        let scratch = root.path().join("objects");
        for path in [&stage_root, &wal_root, &scratch] {
            std::fs::create_dir_all(path).expect("fixture directory");
        }
        let node_id = NodeId::new(uuid::Uuid::from_u128(0xc1a1));
        let stage = Arc::new(ScribeHotStage::new(stage_root.clone()));
        let runtime = ScribeStagingRuntime::new(
            Arc::clone(&stage),
            staging_volume(root.path(), &stage_root),
            publisher(stage, &wal_root, node_id),
            StagingAssemblerConfig::new(512 * 1024 * 1024, std::time::Duration::from_mins(5), 4)
                .expect("assembler controls"),
        );

        let tenant = DataTenantId::new_v7();
        let schema = runtime_schema();
        let layout = runtime_layout(schema.as_ref());
        let frozen = frozen_member(tenant, 1_024, 1);
        let binding = TenantTableBinding::resolve((tenant, frozen.seal_key.table.clone()))
            .expect("tenant binding");
        let staged = runtime
            .encode_member(
                StageMemberRequest {
                    frozen: &frozen,
                    binding: &binding,
                    layout: &layout,
                    origin: StagedMemberOrigin {
                        node_id,
                        writer_epoch: WriterEpoch::new(1),
                        shard: 1,
                        generation: 7,
                        wal: StagedLsnRange { min: 10, max: 19 },
                    },
                    footer_reservation: crate::scribe::memory::EncodedFooterReservation::for_test(),
                },
                ClaimContext {
                    schema: Arc::clone(&schema),
                    layout: layout.clone(),
                    binding: binding.clone(),
                },
            )
            .expect("member stages");
        assert_eq!(staged.member(), StagedMemberId::new(1, 7));
        let key = staged.key().clone();
        runtime
            .register_member(staged, chrono::Utc::now())
            .await
            .expect("member becomes durable and ready");

        let claim = runtime
            .take_residue(&key, ClaimCause::PartitionClosed)
            .expect("residue claim")
            .expect("the ready member is claimable");
        assert_eq!(claim.rows(), 1_024);
        assert!(
            runtime
                .take_residue(&key, ClaimCause::PartitionClosed)
                .expect("second residue claim")
                .is_none(),
            "a claimed member must not be claimable a second time"
        );

        let runs = runtime.gather(&claim).await.expect("claim runs");
        let assembled = runtime
            .assemble(
                &claim,
                AssembleRequest {
                    runs: &runs,
                    scratch_dir: &scratch,
                    footer_reservation: crate::scribe::memory::EncodedFooterReservation::for_test(),
                },
            )
            .expect("claim assembles under its staged context");
        assert_eq!(assembled.rows, 1_024);
        assert_eq!(
            assembled.artifacts.iter().count(),
            1,
            "a claim under the target closes as one object"
        );
        assert!(
            claim_object_base(&claim, &runs, &runtime.context_for(&key).expect("context"))
                .expect("claim object base")
                .contains(&claim.id().to_string()),
            "objects are named after the claim that produced them"
        );
    }
}
