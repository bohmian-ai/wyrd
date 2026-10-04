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

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use arrow::datatypes::SchemaRef;
use chrono::{DateTime, Utc};

use crate::catalog::TenantTableBinding;
use crate::catalog::layout::PhysicalLayout;
use crate::contracts::ScribeError;
use crate::scribe::assembly::{
    AssemblyError, ClaimCause, ScribeAssemblyKey, StagingAssembler, StagingAssemblerConfig,
    StagingBacklog, StagingClaim, StagingClaimId,
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
    /// Scribe capability that assembly charges materialized batches against.
    pub memory: crate::resources::ScribeResources,
}

/// Why a claim could not be taken from the ready index.
///
/// A full claim budget is backpressure, not a failure: every slot belongs to
/// an outstanding claim that a publication will settle or retry, so a caller
/// that meets it either yields until its next pass or waits for
/// [`ScribeStagingRuntime::claim_released`].
#[derive(Debug, thiserror::Error)]
pub enum ClaimTakeError {
    /// A claim is due but every claim slot is held by an outstanding claim.
    #[error("Scribe staging cannot take another claim: all {budget} claim slots are outstanding")]
    BudgetExhausted {
        /// Configured simultaneous-claim ceiling.
        budget: usize,
    },
    /// The ready index is unavailable or refused the claim for another reason.
    #[error(transparent)]
    Failed(#[from] ScribeError),
}

impl From<ClaimTakeError> for ScribeError {
    /// Collapses a refused take into the Scribe error a caller that cannot
    /// wait for a claim slot reports.
    fn from(error: ClaimTakeError) -> Self {
        match error {
            ClaimTakeError::BudgetExhausted { .. } => Self::Internal {
                detail: error.to_string(),
            },
            ClaimTakeError::Failed(error) => error,
        }
    }
}

/// Outstanding claims some caller in this process is currently publishing.
///
/// An outstanding claim outlives the publication that took it: a refused
/// publication leaves the claim, its slot, and its members in place so the
/// identical claim can run again. This set is what tells such a retry apart
/// from a claim still in flight, so no claim is ever driven by two
/// publishers at once. Entries live exactly as long as their [`DrivenClaim`].
#[derive(Debug, Default)]
struct ClaimDrivers {
    /// Identities of the claims currently held by a [`DrivenClaim`].
    driven: Mutex<HashSet<StagingClaimId>>,
    /// Wakes every publisher waiting for a claim slot when a drive ends.
    released: tokio::sync::Notify,
}

impl ClaimDrivers {
    /// Takes the exclusive right to publish `claim`, unless another caller
    /// already holds it.
    ///
    /// The set stays consistent across a panic — every critical section is a
    /// single insert, remove, or read — so a poisoned lock is recovered rather
    /// than turned into a refusal that would strand the claim.
    fn drive(self: &Arc<Self>, claim: StagingClaim) -> Option<DrivenClaim> {
        let inserted = self
            .driven
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(claim.id());
        inserted.then(|| DrivenClaim {
            claim,
            drivers: Arc::clone(self),
        })
    }

    /// Ends one drive and wakes every publisher waiting for a claim slot.
    ///
    /// A settled claim has already returned its slot when its drive ends, and a
    /// refused one has just become retryable, so either way a waiter has new
    /// work to look for.
    fn release(&self, claim: StagingClaimId) {
        self.driven
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&claim);
        self.released.notify_waiters();
    }

    /// Returns whether no caller in this process is publishing any claim.
    fn is_idle(&self) -> bool {
        self.driven
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty()
    }
}

/// An outstanding claim together with the exclusive right to publish it.
///
/// Handed out by [`ScribeStagingRuntime::take_claim`],
/// [`ScribeStagingRuntime::take_residue`], and
/// [`ScribeStagingRuntime::retryable_claims`]. Dropping it — after the claim
/// settles, after a refusal, or on cancellation — ends the drive, so a claim
/// that did not settle becomes retryable again.
#[derive(Debug)]
pub struct DrivenClaim {
    /// The claim this publisher owns.
    claim: StagingClaim,
    /// Registry the drive is released back to on drop.
    drivers: Arc<ClaimDrivers>,
}

impl std::ops::Deref for DrivenClaim {
    type Target = StagingClaim;

    /// Exposes the driven claim's identity, key, and members.
    fn deref(&self) -> &StagingClaim {
        &self.claim
    }
}

impl Drop for DrivenClaim {
    /// Releases the drive so the claim can be retried or its slot reused.
    fn drop(&mut self) {
        self.drivers.release(self.claim.id());
    }
}

/// Owner of one pod's staged members, ready index, and claim lifecycle.
pub struct ScribeStagingRuntime {
    /// Durable staged namespace this pod recovers and serves from.
    stage: Arc<ScribeHotStage>,
    /// Encoder and durable-charge owner for freshly frozen buckets.
    stager: ScribeMemberStager,
    /// Merge owner that re-validates a claim's members before encoding them.
    claims: ClaimAssembler,
    /// Fenced publication and member-retirement owner.
    publisher: ClaimPublisher,
    /// Tenant-fair ready index deciding which members become one object.
    assembly: Mutex<StagingAssembler>,
    /// Outstanding claims a publisher in this process is currently driving.
    drivers: Arc<ClaimDrivers>,
    /// Encoding context recorded for every key that has staged a member.
    contexts: Mutex<HashMap<ScribeAssemblyKey, ClaimContext>>,
    /// Approximate encoded size at which one published object closes.
    target_object_bytes: u64,
    /// Pod-wide authority registry, when this runtime belongs to a pod.
    ///
    /// Staging and publication are the two moments a generation's rows change
    /// hands, so this owner is what moves the authority forward. A fixture
    /// runtime built without a pod tracks no authority and none is asked of it.
    hot_sources: Option<Arc<crate::scribe::hot_source::ScribeHotSourceRegistry>>,
    /// Read-only record of which shards contributed to each published claim.
    ///
    /// Neither the `file_list` row nor its promotion record names the shard
    /// lanes that produced an object — publication is a fact about rows, not
    /// about which lane froze them. Harnesses that need to prove a real
    /// cross-shard merge occurred read it here instead of inferring it from
    /// object counts.
    #[cfg(any(test, feature = "test-support"))]
    published_claims: Mutex<Vec<PublishedClaimObservation>>,
}

/// Which shards one published claim drew from, and what it published.
///
/// Recorded only under test support. Every object in one entry came from the
/// same merged claim, so an entry naming more than one shard proves each of its
/// objects is the product of a real cross-shard merge.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedClaimObservation {
    /// Object keys the claim committed, in artifact-ordinal order.
    pub object_keys: Vec<String>,
    /// Distinct shard lanes that froze the claim's contributing members.
    pub member_shards: Vec<u16>,
}

impl ScribeStagingRuntime {
    /// Composes the pod's staged lifecycle over one staged namespace.
    #[must_use]
    pub fn new(
        stage: Arc<ScribeHotStage>,
        publisher: ClaimPublisher,
        config: StagingAssemblerConfig,
    ) -> Self {
        Self {
            stage: Arc::clone(&stage),
            stager: ScribeMemberStager::new(Arc::clone(&stage)),
            claims: ClaimAssembler::new(stage),
            publisher,
            assembly: Mutex::new(StagingAssembler::new(config)),
            drivers: Arc::default(),
            contexts: Mutex::new(HashMap::new()),
            target_object_bytes: config.target_file_size_bytes(),
            hot_sources: None,
            #[cfg(any(test, feature = "test-support"))]
            published_claims: Mutex::new(Vec::new()),
        }
    }

    /// Binds this runtime to its pod's hot-source authority registry.
    ///
    /// Production wiring calls this before the runtime stages anything, so
    /// every member it makes durable hands its generation's authority over from
    /// the memtable in the same step that makes the rows survivable.
    #[must_use]
    pub fn with_hot_sources(
        mut self,
        hot_sources: Arc<crate::scribe::hot_source::ScribeHotSourceRegistry>,
    ) -> Self {
        self.publisher.set_hot_sources(Arc::clone(&hot_sources));
        self.hot_sources = Some(hot_sources);
        self
    }

    /// Locks the assembler that owns the ready index and outstanding claims.
    ///
    /// Every ownership transition — durable registration, claim take,
    /// settlement, and restoration — publishes the staged-backlog gauges from
    /// the guard it mutated through, before releasing it. Publishing under the
    /// same lock orders the gauge writes with the transitions, so a stale
    /// snapshot can never overwrite a later one and the gauges always end at
    /// the members this runtime actually holds.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the ready index is poisoned.
    fn lock_assembly(&self) -> Result<MutexGuard<'_, StagingAssembler>, ScribeError> {
        self.assembly
            .lock()
            .map_err(|_| poisoned("staged ready index"))
    }

    /// Returns the staged backlog the assembler currently owns.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the ready index is poisoned.
    pub fn backlog(&self) -> Result<StagingBacklog, ScribeError> {
        Ok(self.lock_assembly()?.backlog())
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
        let member = staged.member();
        let runs = staged
            .runs()
            .iter()
            .map(|run| {
                self.stage
                    .member_directory(&key, member)
                    .join(run.file_name())
            })
            .collect();
        let bytes = staged.staged_bytes();
        let wal = staged.wal();
        let ready = self.stager.publish_ready(staged, ready_at).await?;
        {
            let mut assembly = self.lock_assembly()?;
            assembly
                .register_ready(&key, ready)
                .map_err(|error| ScribeError::Internal {
                    detail: format!("register the staged member as ready: {error}"),
                })?;
            assembly.backlog().publish();
        }
        self.advance_authority(
            &key,
            member,
            crate::scribe::hot_source::HotAuthority::StagedRun {
                member,
                runs,
                bytes,
                wal: (
                    crate::scribe::wal::WalLsn::new(wal.min),
                    crate::scribe::wal::WalLsn::new(wal.max),
                ),
            },
        )
    }

    /// Moves one member's generation to a later authority in the pod registry.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the registry refuses the move,
    /// which means the generation is unknown to it or the move is not strictly
    /// forward — either way a reader could be sent to rows that are not there.
    fn advance_authority(
        &self,
        key: &ScribeAssemblyKey,
        member: crate::scribe::assembly::StagedMemberId,
        authority: crate::scribe::hot_source::HotAuthority,
    ) -> Result<(), ScribeError> {
        let Some(hot_sources) = &self.hot_sources else {
            return Ok(());
        };
        hot_sources
            .advance(
                &crate::scribe::seal_key::SealKey::new(
                    key.tenant(),
                    key.table().clone(),
                    key.partition(),
                ),
                crate::scribe::hot_source::GenerationOrdinal::new(
                    member.shard(),
                    member.generation(),
                ),
                authority,
            )
            .map_err(|error| ScribeError::Internal {
                detail: format!(
                    "move the authority of staged member {}-{} forward: {error}",
                    member.shard(),
                    member.generation()
                ),
            })?;
        Ok(())
    }

    /// Takes the next claim any tenant is entitled to, if one is due.
    ///
    /// The claim is returned already driven by the caller, under the same
    /// ready-index lock that created it, so no retry can observe it unowned.
    ///
    /// # Errors
    ///
    /// Returns [`ClaimTakeError::BudgetExhausted`] when a key is due while
    /// every claim slot is held, and [`ClaimTakeError::Failed`] when the ready
    /// index is unavailable or refuses the claim. A full budget with nothing
    /// due is not an error; it returns `Ok(None)` like any other idle poll.
    pub fn take_claim(&self, now: DateTime<Utc>) -> Result<Option<DrivenClaim>, ClaimTakeError> {
        let mut assembly = self.lock_assembly()?;
        let claim = assembly
            .next_claim(now)
            .map_err(|error| take_refused("take the next due staging claim", error))?;
        self.drive_taken(&assembly, claim)
    }

    /// Drives a claim the ready index just created and republishes the backlog.
    ///
    /// Runs under the ready-index guard that created the claim.
    ///
    /// # Errors
    ///
    /// Returns [`ClaimTakeError::Failed`] when the fresh claim is already
    /// driven, which would mean the ready index handed out one member set
    /// twice.
    fn drive_taken(
        &self,
        assembly: &StagingAssembler,
        claim: Option<StagingClaim>,
    ) -> Result<Option<DrivenClaim>, ClaimTakeError> {
        let Some(claim) = claim else {
            return Ok(None);
        };
        assembly.backlog().publish();
        let id = claim.id();
        self.drivers.drive(claim).map(Some).ok_or_else(|| {
            ClaimTakeError::Failed(ScribeError::Internal {
                detail: format!("freshly taken staging claim {id} is already being published"),
            })
        })
    }

    /// Returns every outstanding claim under its original durable identity.
    ///
    /// Startup uses this after stage and publication-manifest recovery so
    /// `Claimed` and `Publishing` work re-enters the production publisher
    /// before admission opens. Live callers do not poll this queue because the
    /// worker that took a new claim already owns its execution.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the ready-index owner is poisoned.
    pub fn resumable_claims(&self) -> Result<Vec<StagingClaim>, ScribeError> {
        Ok(self
            .assembly
            .lock()
            .map_err(|_| poisoned("staged ready index"))?
            .resumable_claims())
    }

    /// Drives every outstanding claim no publisher holds and publication can
    /// still run again.
    ///
    /// A refused publication leaves its claim outstanding with members still
    /// `Claimed` or already `Publishing`. Both re-run the identical claim:
    /// the claim identity, the publication operation, the uploaded objects, and
    /// the fenced `file_list` transaction are all derived from the member set,
    /// so a replay either commits or recognises its own earlier commit. A claim
    /// whose members recorded the commit (`Published` or later) has nothing
    /// left to publish and is left to startup reconciliation.
    ///
    /// Each claim is marked driven under the ready-index lock before its
    /// records are read, so a claim another publisher is driving — still in
    /// flight, not refused — is never returned. Claims that turn out not to be
    /// retryable are released again.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the ready-index owner is poisoned
    /// or a claimed member's durable record cannot be read. Every claim driven
    /// so far is released.
    pub async fn retryable_claims(&self) -> Result<Vec<DrivenClaim>, ScribeError> {
        let candidates = {
            let assembly = self.lock_assembly()?;
            assembly
                .resumable_claims()
                .into_iter()
                .filter_map(|claim| self.drivers.drive(claim))
                .collect::<Vec<_>>()
        };
        let mut retryable = Vec::with_capacity(candidates.len());
        for claim in candidates {
            if self.publication_can_rerun(&claim).await? {
                retryable.push(claim);
            }
        }
        Ok(retryable)
    }

    /// Returns whether every member of `claim` is still before its commit.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a member's durable record cannot
    /// be read.
    async fn publication_can_rerun(&self, claim: &StagingClaim) -> Result<bool, ScribeError> {
        for member in claim.members() {
            let staged = self
                .stage
                .member(claim.key(), member.id())
                .await
                .map_err(|error| ScribeError::Internal {
                    detail: format!(
                        "read staged member {}-{} before resuming its claim: {error}",
                        member.id().shard(),
                        member.id().generation()
                    ),
                })?;
            if !matches!(
                staged.record().state(),
                crate::scribe::hot_stage::StagedMemberState::Ready
                    | crate::scribe::hot_stage::StagedMemberState::Claimed { .. }
                    | crate::scribe::hot_stage::StagedMemberState::Publishing { .. }
            ) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Returns a future that completes when any claim's drive next ends.
    ///
    /// A publisher that found every claim slot held creates this *before* it
    /// asks for a claim, so a release racing that request still wakes it.
    #[must_use]
    pub fn claim_released(&self) -> tokio::sync::futures::Notified<'_> {
        self.drivers.released.notified()
    }

    /// Returns whether no publisher in this process is driving any claim.
    ///
    /// With the budget exhausted and no drive in progress, no release is
    /// coming: every slot belongs to a claim no publisher can run again.
    #[must_use]
    pub fn drives_no_claims(&self) -> bool {
        self.drivers.is_idle()
    }

    /// Takes every ready member of one key as a residue claim.
    ///
    /// Used when waiting for target can no longer pay for itself: the partition
    /// closed, the pod is draining, or pressure requires the staged bytes back.
    ///
    /// The claim is returned already driven by the caller.
    ///
    /// # Errors
    ///
    /// Returns [`ClaimTakeError::BudgetExhausted`] when the key holds ready
    /// members while every claim slot is held, and [`ClaimTakeError::Failed`]
    /// when the ready index is unavailable or refuses the claim.
    pub fn take_residue(
        &self,
        key: &ScribeAssemblyKey,
        cause: ClaimCause,
    ) -> Result<Option<DrivenClaim>, ClaimTakeError> {
        let mut assembly = self.lock_assembly()?;
        let claim = assembly
            .claim_residue(key, cause)
            .map_err(|error| take_refused("take the residue claim for a staged key", error))?;
        self.drive_taken(&assembly, claim)
    }

    /// Rebuilds the ready and claim indexes from what survived on the volume.
    ///
    /// Startup runs this before admission opens. Every recovered member has
    /// already been validated byte-for-byte by the staged namespace, so what is
    /// rebuilt here is the in-memory ownership those durable facts imply: ready
    /// members re-enter the ready index, members an unsettled claim owns are
    /// restored as that claim, and published members restore nothing because a
    /// hot object already serves their rows.
    ///
    /// The volume governor's registration scan is the one charge for staged
    /// files that survived; restore never charges them again and only releases
    /// the members it retires.
    ///
    /// Each key's encoding context is reconstructed from the same two
    /// authorities the members were staged under: the physical schema is read
    /// from the members' own Parquet runs, and the write recipe is re-resolved
    /// from the registry and then required to reproduce the exact key the
    /// record carries. A registry that no longer produces that key is
    /// contradictory lineage, not a recoverable difference, so the restore
    /// fails closed rather than merging durable rows under a recipe they were
    /// not written with.
    ///
    /// Returns the number of members restored.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the staged namespace cannot be
    /// recovered or validated, a member's binding, schema, or recipe cannot be
    /// reconstructed, the recovered layout contradicts the key, the ready index
    /// refuses a duplicate member.
    pub async fn restore(&self, pool: &sqlx::PgPool) -> Result<usize, ScribeError> {
        let recovered = self
            .stage
            .recover()
            .await
            .map_err(|error| ScribeError::Internal {
                detail: format!("recover the staged namespace: {error}"),
            })?;
        let mut restored = 0;
        for (key, members) in recovered {
            self.restore_authorities(&key, &members)?;
            let terminal = self
                .publisher
                .recover_terminal_members(&key, &members)
                .await?;
            let mut members_to_restore = Vec::with_capacity(members.len());
            for member in &members {
                if member
                    .record()
                    .state()
                    .claim_id()
                    .is_some_and(|claim| terminal.terminal_claims.contains(claim))
                {
                    continue;
                }
                let recovered = member.recovered().map_err(|error| ScribeError::Internal {
                    detail: format!("project a recovered staged member: {error}"),
                })?;
                let Some(recovered) = recovered else {
                    continue;
                };
                members_to_restore.push(recovered);
            }
            if !members_to_restore.is_empty() {
                let context = self.restore_context(pool, &key, &members).await?;
                restored += members_to_restore.len();
                self.lock_assembly()?
                    .restore(&key, members_to_restore)
                    .map_err(|error| ScribeError::Internal {
                        detail: format!("restore a recovered staged key: {error}"),
                    })?;
                self.contexts
                    .lock()
                    .map_err(|_| poisoned("staged claim context registry"))?
                    .insert(key.clone(), context);
            }
        }
        self.lock_assembly()?.backlog().publish();
        Ok(restored)
    }

    /// Installs the durable authority of every member recovery kept.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the registry refuses a restored
    /// authority, which means two durable records claim the same generation.
    fn restore_authorities(
        &self,
        key: &ScribeAssemblyKey,
        members: &[crate::scribe::hot_stage::StagedMember],
    ) -> Result<(), ScribeError> {
        let Some(hot_sources) = &self.hot_sources else {
            return Ok(());
        };
        let seal_key = crate::scribe::seal_key::SealKey::new(
            key.tenant(),
            key.table().clone(),
            key.partition(),
        );
        for member in members {
            let id = member.record().member();
            let wal = member.record().wal_range();
            let authority = match member.record().state() {
                crate::scribe::hot_stage::StagedMemberState::Published {
                    published_object_identities,
                    ..
                }
                | crate::scribe::hot_stage::StagedMemberState::CleanupPending {
                    published_object_identities,
                    ..
                } => {
                    if published_object_identities.is_empty() {
                        return Err(ScribeError::Internal {
                            detail: "recovered published member names no object identity"
                                .to_owned(),
                        });
                    }
                    crate::scribe::hot_source::HotAuthority::Published {
                        object_keys: published_object_identities.clone(),
                    }
                }
                crate::scribe::hot_stage::StagedMemberState::Ready
                | crate::scribe::hot_stage::StagedMemberState::Claimed { .. }
                | crate::scribe::hot_stage::StagedMemberState::Publishing { .. } => {
                    crate::scribe::hot_source::HotAuthority::StagedRun {
                        member: id,
                        runs: member.run_paths(),
                        bytes: member.record().encoded_bytes(),
                        wal: (
                            crate::scribe::wal::WalLsn::new(wal.min),
                            crate::scribe::wal::WalLsn::new(wal.max),
                        ),
                    }
                }
            };
            hot_sources
                .restore_durable(
                    &seal_key,
                    crate::scribe::hot_source::GenerationOrdinal::new(id.shard(), id.generation()),
                    authority,
                )
                .map_err(|error| ScribeError::Internal {
                    detail: format!(
                        "restore the authority of staged member {}-{}: {error}",
                        id.shard(),
                        id.generation()
                    ),
                })?;
        }
        Ok(())
    }

    /// Reconstructs one recovered key's encoding context, or fails closed.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the key names no run to read the
    /// schema from, the binding cannot be resolved, the schema on disk is not
    /// the schema the key fingerprints, the registry has no recipe for the
    /// table, or the resolved recipe does not reproduce the key.
    async fn restore_context(
        &self,
        pool: &sqlx::PgPool,
        key: &ScribeAssemblyKey,
        members: &[crate::scribe::hot_stage::StagedMember],
    ) -> Result<ClaimContext, ScribeError> {
        let run = members
            .iter()
            .flat_map(crate::scribe::hot_stage::StagedMember::run_paths)
            .next()
            .ok_or_else(|| ScribeError::Internal {
                detail: "a recovered staged key names no run to read its schema from".to_owned(),
            })?;
        let schema = run_schema(&run)?;
        if crate::parquet::footer::schema_fingerprint(schema.as_ref()) != key.schema_fingerprint() {
            return Err(ScribeError::Internal {
                detail: "a recovered staged run's schema is not the schema its key names"
                    .to_owned(),
            });
        }
        let binding =
            TenantTableBinding::resolve((key.tenant(), key.table().clone())).map_err(|error| {
                ScribeError::Internal {
                    detail: format!(
                        "resolve the binding a recovered staged key publishes under: {error}"
                    ),
                }
            })?;
        let layout =
            crate::scribe::write_recipe::resolve_write_recipe(pool, &binding, schema.as_ref())
                .await?;
        let resolved = ScribeAssemblyKey::new(
            key.tenant(),
            key.table().clone(),
            key.schema_fingerprint(),
            layout.as_ref(),
            key.partition(),
            key.node_id(),
            key.writer_epoch(),
        );
        if resolved != *key {
            return Err(ScribeError::Internal {
                detail: "the registered write recipe no longer reproduces a recovered staged key"
                    .to_owned(),
            });
        }
        Ok(ClaimContext {
            schema,
            layout: (*layout).clone(),
            binding,
        })
    }

    /// Returns every key that still holds ready, unpublished members.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the ready index is unavailable.
    pub fn ready_keys(&self) -> Result<Vec<ScribeAssemblyKey>, ScribeError> {
        Ok(self
            .assembly
            .lock()
            .map_err(|_| poisoned("staged ready index"))?
            .ready_keys())
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
            memory: request.memory,
            tenant: context.binding.tenant,
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
        Self::record_committed_claim(assembled);
        #[cfg(any(test, feature = "test-support"))]
        self.record_published_claim(claim, &published);
        self.settle(claim.id())?;
        Ok(published)
    }

    /// Counts one committed claim and the objects it published.
    ///
    /// Runs only after the fenced `file_list` transaction committed, so
    /// `bifrost_scribe_staging_claims_published_total`,
    /// `bifrost_scribe_publication_files_total`, and
    /// `bifrost_scribe_publication_bytes_total` describe committed output, not
    /// attempts. A claim reconciled after a lost commit response is counted
    /// once by the attempt that observed the commit.
    fn record_committed_claim(assembled: &AssembledClaim) {
        let artifacts = assembled.artifacts.as_slice();
        metrics::counter!("bifrost_scribe_staging_claims_published_total").increment(1);
        metrics::counter!("bifrost_scribe_publication_files_total")
            .increment(u64::try_from(artifacts.len()).unwrap_or(u64::MAX));
        metrics::counter!("bifrost_scribe_publication_bytes_total")
            .increment(artifacts.iter().map(|artifact| artifact.file_size).sum());
    }

    /// Records which shards produced the members of one committed claim.
    ///
    /// Deliberately infallible and lock-tolerant: a poisoned observation lock
    /// must never turn a successful publication into a failure, because the
    /// record exists only so a harness can read what already happened.
    #[cfg(any(test, feature = "test-support"))]
    fn record_published_claim(&self, claim: &StagingClaim, published: &PublishedClaim) {
        let mut member_shards: Vec<u16> = claim
            .members()
            .iter()
            .map(|member| member.id().shard())
            .collect();
        member_shards.sort_unstable();
        member_shards.dedup();
        if let Ok(mut observations) = self.published_claims.lock() {
            observations.push(PublishedClaimObservation {
                object_keys: published.object_identities.clone(),
                member_shards,
            });
        }
    }

    /// Returns every claim this runtime has published, in publication order.
    ///
    /// # Panics
    ///
    /// Panics when the observation lock is poisoned, which means a harness
    /// thread already failed while holding it.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn published_claims_for_test(&self) -> Vec<PublishedClaimObservation> {
        self.published_claims
            .lock()
            .expect("staged publication observations are not poisoned")
            .clone()
    }

    /// Returns the claim slot and republishes the staged backlog.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the ready index is unavailable
    /// or the claim is unknown.
    fn settle(&self, claim: StagingClaimId) -> Result<(), ScribeError> {
        let mut assembly = self.lock_assembly()?;
        assembly
            .settle_claim(claim)
            .map_err(|error| ScribeError::Internal {
                detail: format!("settle a published staging claim: {error}"),
            })?;
        assembly.backlog().publish();
        Ok(())
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

/// Reads the physical schema one staged run was encoded with.
///
/// The run's own footer is the authority: it describes the bytes that will be
/// merged, which is exactly what the merge must be told about.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the run cannot be opened or its
/// Parquet metadata cannot be read.
fn run_schema(run: &Path) -> Result<SchemaRef, ScribeError> {
    let file = std::fs::File::open(run).map_err(|error| ScribeError::Internal {
        detail: format!("open a recovered staged run: {error}"),
    })?;
    let builder = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(file)
        .map_err(|error| ScribeError::Internal {
            detail: format!("read a recovered staged run's metadata: {error}"),
        })?;
    Ok(Arc::clone(builder.schema()))
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

/// Classifies a ready-index refusal to hand out a claim.
///
/// An exhausted claim budget stays typed as backpressure; every other refusal
/// is an internal failure described by `action`.
fn take_refused(action: &str, error: AssemblyError) -> ClaimTakeError {
    match error {
        AssemblyError::ClaimBudgetExhausted { budget } => {
            ClaimTakeError::BudgetExhausted { budget }
        }
        error => ClaimTakeError::Failed(ScribeError::Internal {
            detail: format!("{action}: {error}"),
        }),
    }
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
    use arrow::array::{FixedSizeBinaryArray, RecordBatch, TimestampMicrosecondArray};
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

    /// Physical schema the fixture member is staged and merged under.
    pub(super) fn runtime_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
        ]))
    }

    /// Freezes one bucket of `rows` rows for the fixture tenant and shard.
    pub(super) fn frozen_member(tenant: DataTenantId, rows: i64, shard: u8) -> FrozenMemtable {
        let schema = runtime_schema();
        let record = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(TimestampMicrosecondArray::from_iter_values(
                    (0..rows).map(|row| row * 2 + i64::from(shard)),
                )),
                Arc::new(
                    FixedSizeBinaryArray::try_from_iter((0..rows).map(|_| [shard; 16]))
                        .expect("fixture batch identity"),
                ),
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
            metas: vec![],
            opened_at: std::time::Instant::now(),
            closed_at: std::time::Instant::now(),
            arrow_bytes: 0,
        }
    }

    /// Resolves the hourly layout the fixture member is encoded under.
    pub(super) fn runtime_layout(schema: &Schema) -> PhysicalLayout {
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

    /// Builds the publisher the runtime owns, over lazy and in-memory owners.
    ///
    /// The fixture never reaches the fenced transaction, so the pool is opened
    /// lazily and never connected: what the test exercises is the lifecycle up
    /// to assembly, which is exactly the part that owns no durable catalog.
    pub(super) fn publisher(
        stage: Arc<ScribeHotStage>,
        wal_root: &Path,
        node: NodeId,
    ) -> ClaimPublisher {
        let operator = opendal::Operator::new(opendal::services::Memory::default())
            .expect("memory operator")
            .finish();
        ClaimPublisher::new(
            stage,
            ScribeStageMover::new(wal_root, operator),
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
                    memory: crate::resources::ScribeResources::for_test(),
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
                    memory: crate::resources::ScribeResources::for_test(),
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

    /// What restart restores is exactly what the volume proves: a durable
    /// member reappears as ready, its runs still carry the physical schema its
    /// key fingerprints, and a fresh runtime over the same namespace can
    /// therefore rebuild the encoding context without trusting memory that did
    /// not survive.
    #[tokio::test]
    async fn a_staged_member_survives_as_the_facts_restore_rebuilds_from() {
        let root = tempfile::tempdir().expect("runtime root");
        let stage_root = root.path().join("stage");
        let wal_root = root.path().join("member-wal");
        for path in [&stage_root, &wal_root] {
            std::fs::create_dir_all(path).expect("fixture directory");
        }
        let node_id = NodeId::new(uuid::Uuid::from_u128(0xe3a3));
        let stage = Arc::new(ScribeHotStage::new(stage_root.clone()));
        let runtime = ScribeStagingRuntime::new(
            Arc::clone(&stage),
            publisher(Arc::clone(&stage), &wal_root, node_id),
            StagingAssemblerConfig::new(512 * 1024 * 1024, std::time::Duration::from_mins(5), 4)
                .expect("assembler controls"),
        );

        let tenant = DataTenantId::new_v7();
        let schema = runtime_schema();
        let layout = runtime_layout(schema.as_ref());
        let frozen = frozen_member(tenant, 512, 3);
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
                        shard: 3,
                        generation: 11,
                        wal: StagedLsnRange { min: 30, max: 39 },
                    },
                    memory: crate::resources::ScribeResources::for_test(),
                },
                ClaimContext {
                    schema: Arc::clone(&schema),
                    layout: layout.clone(),
                    binding: binding.clone(),
                },
            )
            .expect("member stages");
        let key = staged.key().clone();
        runtime
            .register_member(staged, chrono::Utc::now())
            .await
            .expect("member becomes durable and ready");
        drop(runtime);

        let recovered = ScribeHotStage::new(stage_root)
            .recover()
            .await
            .expect("the staged namespace validates");
        let members = recovered.get(&key).expect("the durable key survived");
        assert_eq!(members.len(), 1, "one member was staged under the key");
        let member = &members[0];
        assert!(
            matches!(
                member.recovered().expect("the member projects"),
                Some(crate::scribe::assembly::RecoveredMember::Ready(ready))
                    if ready.id() == StagedMemberId::new(3, 11)
            ),
            "an unclaimed durable member restores as ready"
        );
        let run = member
            .run_paths()
            .into_iter()
            .next()
            .expect("the member names a run");
        let on_disk = run_schema(&run).expect("the run carries its schema");
        assert_eq!(
            crate::parquet::footer::schema_fingerprint(on_disk.as_ref()),
            key.schema_fingerprint(),
            "the schema restore reads back is the schema the key names"
        );
    }

    /// Members that reach neither target nor dwell are invisible to
    /// [`ScribeStagingRuntime::take_claim`] but are exactly what an explicit
    /// flush must settle, so every ready key is reachable as residue and each key stops
    /// being ready once its residue is claimed.
    #[tokio::test]
    async fn drain_reaches_every_ready_key_target_and_dwell_would_hold() {
        let root = tempfile::tempdir().expect("runtime root");
        let stage_root = root.path().join("stage");
        let wal_root = root.path().join("member-wal");
        for path in [&stage_root, &wal_root] {
            std::fs::create_dir_all(path).expect("fixture directory");
        }
        let node_id = NodeId::new(uuid::Uuid::from_u128(0xd2a2));
        let stage = Arc::new(ScribeHotStage::new(stage_root.clone()));
        let runtime = ScribeStagingRuntime::new(
            Arc::clone(&stage),
            publisher(stage, &wal_root, node_id),
            StagingAssemblerConfig::new(512 * 1024 * 1024, std::time::Duration::from_mins(5), 4)
                .expect("assembler controls"),
        );

        let schema = runtime_schema();
        let layout = runtime_layout(schema.as_ref());
        let staged_at = chrono::Utc::now();
        let mut keys = Vec::new();
        for (shard, generation) in [(1_u8, 7_u64), (2, 8)] {
            let tenant = DataTenantId::new_v7();
            let frozen = frozen_member(tenant, 256, shard);
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
                            shard: shard.into(),
                            generation,
                            wal: StagedLsnRange {
                                min: u64::from(shard) * 10,
                                max: u64::from(shard) * 10 + 9,
                            },
                        },
                        memory: crate::resources::ScribeResources::for_test(),
                    },
                    ClaimContext {
                        schema: Arc::clone(&schema),
                        layout: layout.clone(),
                        binding: binding.clone(),
                    },
                )
                .expect("member stages");
            keys.push(staged.key().clone());
            runtime
                .register_member(staged, staged_at)
                .await
                .expect("member becomes durable and ready");
        }

        assert!(
            runtime
                .take_claim(staged_at)
                .expect("due claim query")
                .is_none(),
            "neither key reached its target or its dwell"
        );
        let ready = runtime.ready_keys().expect("ready keys");
        assert_eq!(ready.len(), 2, "both durable members are still unpublished");
        for key in &keys {
            assert!(ready.contains(key), "every ready key is reachable by drain");
        }

        let mut claimed_rows = 0;
        for key in &ready {
            let claim = runtime
                .take_residue(key, ClaimCause::Drain)
                .expect("residue claim")
                .expect("a ready key yields its residue");
            claimed_rows += claim.rows();
        }
        assert_eq!(claimed_rows, 512, "drain claims every staged row");
        assert!(
            runtime.ready_keys().expect("ready keys").is_empty(),
            "a swept key is no longer ready"
        );
    }

    /// The staged backlog counts ready and claimed members from real ownership.
    ///
    /// A durable member is live once registered; taking a claim moves it into
    /// the claim without shrinking the backlog, and only opens one outstanding
    /// claim. Each value is read from the assembler that owns the members.
    ///
    /// # Panics
    ///
    /// Panics when staging fails or the backlog disagrees with the members and
    /// claims the runtime actually holds.
    #[tokio::test(flavor = "current_thread")]
    async fn staged_backlog_counts_ready_and_claimed_members() {
        let root = tempfile::tempdir().expect("runtime root");
        let stage_root = root.path().join("stage");
        let wal_root = root.path().join("member-wal");
        for path in [&stage_root, &wal_root] {
            std::fs::create_dir_all(path).expect("fixture directory");
        }
        let node_id = NodeId::new(uuid::Uuid::from_u128(0xd2a3));
        let stage = Arc::new(ScribeHotStage::new(stage_root.clone()));
        let hot_sources = Arc::new(crate::scribe::hot_source::ScribeHotSourceRegistry::new());
        let runtime = ScribeStagingRuntime::new(
            Arc::clone(&stage),
            publisher(stage, &wal_root, node_id),
            StagingAssemblerConfig::new(512 * 1024 * 1024, std::time::Duration::from_mins(5), 4)
                .expect("assembler controls"),
        )
        .with_hot_sources(Arc::clone(&hot_sources));

        let schema = runtime_schema();
        let layout = runtime_layout(schema.as_ref());
        let tenant = DataTenantId::new_v7();
        let frozen = frozen_member(tenant, 256, 1);
        let binding = TenantTableBinding::resolve((tenant, frozen.seal_key.table.clone()))
            .expect("tenant binding");
        hot_sources
            .register_memtable(
                &frozen.seal_key,
                crate::scribe::hot_source::GenerationOrdinal::new(1, 7),
            )
            .expect("the generation registers as memtable-authoritative");
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
                    memory: crate::resources::ScribeResources::for_test(),
                },
                ClaimContext {
                    schema: Arc::clone(&schema),
                    layout: layout.clone(),
                    binding: binding.clone(),
                },
            )
            .expect("member stages");
        let key = staged.key().clone();
        runtime
            .register_member(staged, chrono::Utc::now())
            .await
            .expect("member becomes durable and ready");

        let staged_backlog = runtime.backlog().expect("staged backlog");
        assert_eq!(
            staged_backlog.live_members, 1,
            "one durable member is staged and none has been published"
        );
        assert!(staged_backlog.live_bytes > 0);
        assert_eq!(staged_backlog.outstanding_claims, 0);

        let claim = runtime
            .take_residue(&key, ClaimCause::Drain)
            .expect("the ready key releases a residue claim")
            .expect("a residue claim is due");
        let claimed = runtime.backlog().expect("claimed backlog");
        assert_eq!(
            claimed.live_members, 1,
            "taking a claim does not publish its member"
        );
        assert_eq!(claimed.live_bytes, staged_backlog.live_bytes);
        assert_eq!(
            claimed.outstanding_claims, 1,
            "the claim is outstanding until it settles"
        );
        assert_eq!(claim.members().len(), 1);
    }

    /// A claim is retryable only while no publisher drives it.
    ///
    /// A take hands its claim out already driven, so a concurrent retry sweep
    /// never sees a claim still in flight. Once that drive ends without
    /// settling, waiters for a claim slot are woken and the identical claim is
    /// offered for retry to exactly one sweep. A full budget meanwhile is
    /// typed backpressure, not an internal failure.
    ///
    /// # Panics
    ///
    /// Panics when staging fails, a driven claim is offered for retry, the
    /// released claim is not offered exactly once, or budget exhaustion is not
    /// reported as [`ClaimTakeError::BudgetExhausted`].
    #[tokio::test]
    async fn a_claim_is_retryable_only_while_no_publisher_drives_it() {
        let root = tempfile::tempdir().expect("runtime root");
        let stage_root = root.path().join("stage");
        let wal_root = root.path().join("member-wal");
        for path in [&stage_root, &wal_root] {
            std::fs::create_dir_all(path).expect("fixture directory");
        }
        let node_id = NodeId::new(uuid::Uuid::from_u128(0xd21e));
        let stage = Arc::new(ScribeHotStage::new(stage_root));
        let runtime = ScribeStagingRuntime::new(
            Arc::clone(&stage),
            publisher(stage, &wal_root, node_id),
            StagingAssemblerConfig::new(512 * 1024 * 1024, std::time::Duration::from_mins(5), 1)
                .expect("assembler controls"),
        );
        let now = chrono::Utc::now();
        let (first, _) =
            stage_durable_members(&runtime, DataTenantId::new_v7(), node_id, 1..=1, now).await;
        let (second, _) =
            stage_durable_members(&runtime, DataTenantId::new_v7(), node_id, 1..=1, now).await;

        let claim = runtime
            .take_residue(&first, ClaimCause::Drain)
            .expect("the ready key releases a residue claim")
            .expect("a residue claim is due");
        assert!(
            runtime
                .retryable_claims()
                .await
                .expect("retry sweep")
                .is_empty(),
            "a claim still in flight is never offered for retry"
        );
        assert!(
            matches!(
                runtime.take_residue(&second, ClaimCause::Drain),
                Err(ClaimTakeError::BudgetExhausted { budget: 1 })
            ),
            "a full claim budget is typed backpressure"
        );

        let id = claim.id();
        let released = runtime.claim_released();
        assert!(!runtime.drives_no_claims());
        drop(claim);
        released.await;
        assert!(runtime.drives_no_claims());
        let retried = runtime.retryable_claims().await.expect("retry sweep");
        assert_eq!(
            retried.iter().map(|claim| claim.id()).collect::<Vec<_>>(),
            vec![id],
            "the unsettled claim is retried under its original identity"
        );
        assert!(
            runtime
                .retryable_claims()
                .await
                .expect("second retry sweep")
                .is_empty(),
            "a retried claim is driven by its one retrier"
        );
    }

    /// Concurrent registration, claim, and settlement leave the published
    /// staged gauges equal to the assembler's final ownership.
    ///
    /// Persistence workers share one runtime, so registrations, claim takes,
    /// and settlements race. Four workers each stage members under fresh
    /// tenants and take and settle whatever claim is due, all reporting into
    /// one recorder. Once every member has been claimed and settled, the four
    /// scraped gauges must equal `backlog()` — zero members, bytes, oldest
    /// time, and claims — rather than whichever detached snapshot published
    /// last.
    ///
    /// # Panics
    ///
    /// Panics when staging, claiming, or settlement fails, or when the gauges
    /// disagree with the runtime's final backlog.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_transitions_publish_the_final_backlog() {
        const WORKERS: usize = 4;
        const ROUNDS: usize = 8;
        let root = tempfile::tempdir().expect("runtime root");
        let stage_root = root.path().join("stage");
        let wal_root = root.path().join("member-wal");
        for path in [&stage_root, &wal_root] {
            std::fs::create_dir_all(path).expect("fixture directory");
        }
        let node_id = NodeId::new(uuid::Uuid::from_u128(0xc0c0));
        let stage = Arc::new(ScribeHotStage::new(stage_root));
        let runtime = ScribeStagingRuntime::new(
            Arc::clone(&stage),
            publisher(stage, &wal_root, node_id),
            StagingAssemblerConfig::new(
                512 * 1024 * 1024,
                std::time::Duration::from_mins(5),
                WORKERS,
            )
            .expect("assembler controls"),
        );
        let ready_at = chrono::Utc::now() - chrono::Duration::minutes(10);
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let handle = tokio::runtime::Handle::current();
        let settle_due = |runtime: &ScribeStagingRuntime| -> usize {
            let mut settled = 0;
            while let Some(claim) = runtime
                .take_claim(chrono::Utc::now())
                .expect("a due claim is taken")
            {
                settled += claim.members().len();
                runtime.settle(claim.id()).expect("the claim settles");
            }
            settled
        };

        let settled = std::thread::scope(|scope| {
            let workers = (0..WORKERS)
                .map(|_| {
                    scope.spawn(|| {
                        let _metrics = metrics::set_default_local_recorder(&recorder);
                        let mut settled = 0;
                        for _ in 0..ROUNDS {
                            handle.block_on(stage_durable_members(
                                &runtime,
                                DataTenantId::new_v7(),
                                node_id,
                                1..=1,
                                ready_at,
                            ));
                            settled += settle_due(&runtime);
                        }
                        settled
                    })
                })
                .collect::<Vec<_>>();
            workers
                .into_iter()
                .map(|worker| worker.join().expect("worker completes"))
                .sum::<usize>()
        });
        let _metrics = metrics::set_default_local_recorder(&recorder);
        let settled = settled + settle_due(&runtime);
        assert_eq!(settled, WORKERS * ROUNDS, "every staged member settled");

        let backlog = runtime.backlog().expect("final backlog");
        assert_eq!(backlog, StagingBacklog::default());
        let gauges = recorder.snapshot().gauges;
        for family in [
            "bifrost_scribe_staging_live_members",
            "bifrost_scribe_staging_live_bytes",
            "bifrost_scribe_staging_oldest_member_timestamp_seconds",
            "bifrost_scribe_staging_outstanding_claims",
        ] {
            assert_eq!(
                gauges.get(family).copied(),
                Some(0.0),
                "{family} ends at the runtime's final backlog"
            );
        }
    }

    /// Stages one member per shard of one partition through `runtime` and
    /// drives each to durable, ready state at `ready_at`, returning the shared
    /// assembly key and the staged member ids in shard order.
    ///
    /// Recovery owners need a claim whose members can be moved into different
    /// terminal states independently, so the fixture stages real members rather
    /// than synthesising stage entries.
    ///
    /// # Panics
    ///
    /// Panics when a member fails to resolve its binding, encode, or register,
    /// or when the members do not share one assembly key.
    pub(super) async fn stage_durable_members(
        runtime: &ScribeStagingRuntime,
        tenant: DataTenantId,
        node_id: NodeId,
        shards: std::ops::RangeInclusive<u8>,
        ready_at: DateTime<Utc>,
    ) -> (ScribeAssemblyKey, Vec<StagedMemberId>) {
        let schema = &runtime_schema();
        let layout = &runtime_layout(schema.as_ref());
        let mut key = None;
        let mut member_ids = Vec::new();
        for shard in shards {
            let frozen = frozen_member(tenant, 64, shard);
            let binding = TenantTableBinding::resolve((tenant, frozen.seal_key.table.clone()))
                .expect("tenant binding");
            let staged = runtime
                .encode_member(
                    StageMemberRequest {
                        frozen: &frozen,
                        binding: &binding,
                        layout,
                        origin: StagedMemberOrigin {
                            node_id,
                            writer_epoch: WriterEpoch::new(1),
                            shard: u16::from(shard),
                            generation: u64::from(shard),
                            wal: StagedLsnRange {
                                min: u64::from(shard) * 10,
                                max: u64::from(shard) * 10 + 9,
                            },
                        },
                        memory: crate::resources::ScribeResources::for_test(),
                    },
                    ClaimContext {
                        schema: Arc::clone(schema),
                        layout: layout.clone(),
                        binding: binding.clone(),
                    },
                )
                .expect("member stages");
            key.get_or_insert_with(|| staged.key().clone());
            member_ids.push(staged.member());
            runtime
                .register_member(staged, ready_at)
                .await
                .expect("member becomes durable and ready");
        }
        (key.expect("one assembly key"), member_ids)
    }

    /// Leaves the claim's members in the mixed terminal states a crash can
    /// strand: one still `Publishing`, one `Published`, one `CleanupPending`,
    /// and one `Published` again, each carrying its own persisted LSN range.
    ///
    /// This is the exact shape recovery must retire without republishing, so
    /// the states are written directly onto the stage rather than reached
    /// through a publication that would also commit.
    ///
    /// # Panics
    ///
    /// Panics when the stage refuses any transition.
    async fn drive_mixed_member_states(
        stage: &ScribeHotStage,
        key: &ScribeAssemblyKey,
        member_ids: &[StagedMemberId],
        claim_id: &str,
    ) {
        let objects = vec![format!("objects/{claim_id}/hot-0.parquet")];
        stage
            .transition(
                key,
                member_ids[0],
                crate::scribe::hot_stage::StagedMemberState::Publishing {
                    claim_id: claim_id.to_owned(),
                    operation_id: uuid::Uuid::from_u128(0xc01),
                },
            )
            .await
            .expect("first member remains publishing");
        for (index, state) in [
            crate::scribe::hot_stage::StagedMemberState::Published {
                claim_id: claim_id.to_owned(),
                file_list_commit_key: "node:10:49".to_owned(),
                published_object_identities: objects.clone(),
                persisted_lsn_ranges: vec![StagedLsnRange { min: 20, max: 29 }],
            },
            crate::scribe::hot_stage::StagedMemberState::CleanupPending {
                claim_id: claim_id.to_owned(),
                file_list_commit_key: "node:10:49".to_owned(),
                published_object_identities: objects.clone(),
                persisted_lsn_ranges: vec![StagedLsnRange { min: 30, max: 39 }],
            },
            crate::scribe::hot_stage::StagedMemberState::Published {
                claim_id: claim_id.to_owned(),
                file_list_commit_key: "node:10:49".to_owned(),
                published_object_identities: objects,
                persisted_lsn_ranges: vec![StagedLsnRange { min: 40, max: 49 }],
            },
        ]
        .into_iter()
        .enumerate()
        {
            stage
                .transition(key, member_ids[index + 1], state)
                .await
                .expect("terminal member state persists");
        }
    }

    /// Asserts recovery left no hot-source authority behind for any member of
    /// the restored claim.
    ///
    /// A surviving authority would let a reader serve rows from a member the
    /// claim already retired, so every generation ordinal must resolve to
    /// `None` once restore completes.
    ///
    /// # Panics
    ///
    /// Panics when an authority lookup fails or still names a live authority.
    fn assert_no_restored_authority_survives(
        hot_sources: &crate::scribe::hot_source::ScribeHotSourceRegistry,
        key: &ScribeAssemblyKey,
        member_ids: &[StagedMemberId],
    ) {
        let seal_key = SealKey::new(key.tenant(), key.table().clone(), key.partition());
        for member in member_ids {
            assert_eq!(
                hot_sources
                    .authority(
                        &seal_key,
                        crate::scribe::hot_source::GenerationOrdinal::new(
                            member.shard(),
                            member.generation(),
                        ),
                    )
                    .expect("authority lookup"),
                None
            );
        }
    }

    /// A crash-split terminal claim recovers under its original Scribe identity.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot stage the claim, recovery derives a new
    /// claim from its lone `Publishing` survivor, republishes through Postgres,
    /// leaks staged bytes, or releases a restored authority more than once.
    #[tokio::test]
    async fn mixed_state_claim_recovery_retires_survivors_without_republication() {
        let root = tempfile::tempdir().expect("runtime root");
        let stage_root = root.path().join("stage");
        let wal_root = root.path().join("member-wal");
        for path in [&stage_root, &wal_root] {
            std::fs::create_dir_all(path).expect("fixture directory");
        }
        let node_id = NodeId::new(uuid::Uuid::from_u128(0xc01));
        let stage = Arc::new(ScribeHotStage::new(stage_root.clone()));
        let runtime = ScribeStagingRuntime::new(
            Arc::clone(&stage),
            publisher(Arc::clone(&stage), &wal_root, node_id),
            StagingAssemblerConfig::new(512 * 1024 * 1024, std::time::Duration::from_mins(5), 4)
                .expect("assembler controls"),
        );
        let tenant = DataTenantId::new_v7();
        let (key, member_ids) =
            stage_durable_members(&runtime, tenant, node_id, 1..=4, chrono::Utc::now()).await;
        let claim = runtime
            .take_residue(&key, ClaimCause::Drain)
            .expect("residue claim")
            .expect("four members form one claim");
        assert_eq!(claim.members().len(), 4);
        let claim_id = claim.id().to_string();
        drive_mixed_member_states(&stage, &key, &member_ids, &claim_id).await;
        stage
            .retire(&key, member_ids[3])
            .await
            .expect("one member retired before the crash");
        drop(runtime);

        let hot_sources = Arc::new(crate::scribe::hot_source::ScribeHotSourceRegistry::new());
        let recovered = ScribeStagingRuntime::new(
            Arc::clone(&stage),
            publisher(Arc::clone(&stage), &wal_root, node_id),
            StagingAssemblerConfig::new(512 * 1024 * 1024, std::time::Duration::from_mins(5), 4)
                .expect("assembler controls"),
        )
        .with_hot_sources(Arc::clone(&hot_sources));
        let pool = sqlx::PgPool::connect_lazy("postgres://unused/unused").expect("lazy pool");
        assert_eq!(
            recovered
                .restore(&pool)
                .await
                .expect("mixed claim recovers"),
            0
        );
        assert!(
            recovered
                .resumable_claims()
                .expect("claim index")
                .is_empty()
        );
        assert!(stage.recover().await.expect("stage rescans").is_empty());
        assert_no_restored_authority_survives(&hot_sources, &key, &member_ids);
        assert_eq!(recovered.restore(&pool).await.expect("cleanup replays"), 0);
    }

    /// Moves every member of one claim to the state `state_for` names for it,
    /// the way publication moves a claim through one batched step.
    ///
    /// # Panics
    ///
    /// Panics when the stage refuses any member's transition.
    async fn move_claim(
        stage: &ScribeHotStage,
        key: &ScribeAssemblyKey,
        member_ids: &[StagedMemberId],
        state_for: impl Fn(&StagedMemberId) -> crate::scribe::hot_stage::StagedMemberState,
    ) {
        for member in member_ids {
            stage
                .transition(key, *member, state_for(member))
                .await
                .expect("member moves to the claim's next state");
        }
    }

    /// Crashes a four-member claim at one boundary between the batched steps
    /// publication moves a claim through, then proves restart retires it.
    ///
    /// Publication moves every member durably to `Published`, then every
    /// member to `CleanupPending`, then removes them together. The fixture
    /// reproduces the durable state at the chosen boundary with the same stage
    /// operations: all members in `Published` (`cleanup_pending == false`) or
    /// all in `CleanupPending`, with the first `retired` members already
    /// removed by one batched [`ScribeHotStage::retire_all`]. With
    /// `half_removed`, the next two survivors are left the way a crash inside
    /// one member's removal leaves it: one keeps its record but has lost its
    /// runs, the other keeps its runs but has lost its record. Restart must
    /// retire every survivor under the committed facts without republishing,
    /// leave no claim to resume, no staged bytes, no member directory, and no
    /// authority.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot stage or drive the claim, or when
    /// recovery republishes, leaves a resumable claim, leaks staged bytes,
    /// leaves a member directory, or leaves a restored authority alive.
    async fn recovers_claim_interrupted_between_batched_states(
        cleanup_pending: bool,
        retired: usize,
        half_removed: bool,
    ) {
        let root = tempfile::tempdir().expect("runtime root");
        let stage_root = root.path().join("stage");
        let wal_root = root.path().join("member-wal");
        for path in [&stage_root, &wal_root] {
            std::fs::create_dir_all(path).expect("fixture directory");
        }
        let node_id = NodeId::new(uuid::Uuid::from_u128(0xba7));
        let stage = Arc::new(ScribeHotStage::new(stage_root.clone()));
        let config = || {
            StagingAssemblerConfig::new(512 * 1024 * 1024, std::time::Duration::from_mins(5), 4)
                .expect("assembler controls")
        };
        let runtime = ScribeStagingRuntime::new(
            Arc::clone(&stage),
            publisher(Arc::clone(&stage), &wal_root, node_id),
            config(),
        );
        let tenant = DataTenantId::new_v7();
        let (key, member_ids) =
            stage_durable_members(&runtime, tenant, node_id, 1..=4, chrono::Utc::now()).await;
        let claim = runtime
            .take_residue(&key, ClaimCause::Drain)
            .expect("residue claim")
            .expect("four members form one claim");
        let claim_id = claim.id().to_string();
        let objects = vec![format!("objects/{claim_id}/hot-0.parquet")];
        let ranges = |member: &StagedMemberId| {
            let shard = u64::from(member.shard());
            vec![StagedLsnRange {
                min: shard * 10,
                max: shard * 10 + 9,
            }]
        };
        move_claim(&stage, &key, &member_ids, |_| {
            crate::scribe::hot_stage::StagedMemberState::Publishing {
                claim_id: claim_id.clone(),
                operation_id: uuid::Uuid::from_u128(0xba7),
            }
        })
        .await;
        move_claim(&stage, &key, &member_ids, |member| {
            crate::scribe::hot_stage::StagedMemberState::Published {
                claim_id: claim_id.clone(),
                file_list_commit_key: "node:10:49".to_owned(),
                published_object_identities: objects.clone(),
                persisted_lsn_ranges: ranges(member),
            }
        })
        .await;
        if cleanup_pending {
            move_claim(&stage, &key, &member_ids, |member| {
                crate::scribe::hot_stage::StagedMemberState::CleanupPending {
                    claim_id: claim_id.clone(),
                    file_list_commit_key: "node:10:49".to_owned(),
                    published_object_identities: objects.clone(),
                    persisted_lsn_ranges: ranges(member),
                }
            })
            .await;
        }
        stage
            .retire_all(&key, &member_ids[..retired])
            .await
            .expect("members retired before the crash");
        if half_removed {
            half_remove(&stage.member_directory(&key, member_ids[retired]), true);
            half_remove(
                &stage.member_directory(&key, member_ids[retired + 1]),
                false,
            );
        }
        drop(runtime);

        let hot_sources = Arc::new(crate::scribe::hot_source::ScribeHotSourceRegistry::new());
        let recovered = ScribeStagingRuntime::new(
            Arc::clone(&stage),
            publisher(Arc::clone(&stage), &wal_root, node_id),
            config(),
        )
        .with_hot_sources(Arc::clone(&hot_sources));
        let pool = sqlx::PgPool::connect_lazy("postgres://unused/unused").expect("lazy pool");
        assert_eq!(
            recovered.restore(&pool).await.expect("claim recovers"),
            0,
            "a committed claim is retired, never republished"
        );
        assert!(
            recovered
                .resumable_claims()
                .expect("claim index")
                .is_empty()
        );
        assert!(stage.recover().await.expect("stage rescans").is_empty());
        for member in &member_ids {
            assert!(
                !stage.member_directory(&key, *member).exists(),
                "member {member:?} left its directory behind"
            );
        }
        assert_no_restored_authority_survives(&hot_sources, &key, &member_ids);
    }

    /// Removes part of one member directory the way a crash inside its
    /// removal can leave it: every run but not the record when `keep_record`,
    /// otherwise only the record.
    ///
    /// # Panics
    ///
    /// Panics when the directory cannot be listed or a file cannot be removed.
    fn half_remove(directory: &Path, keep_record: bool) {
        for entry in std::fs::read_dir(directory).expect("member directory lists") {
            let path = entry.expect("member directory entry").path();
            let is_record = path.file_name()
                == Some(std::ffi::OsStr::new(
                    crate::scribe::hot_stage::RECORD_FILE_NAME,
                ));
            if is_record != keep_record {
                std::fs::remove_file(&path).expect("crash removes the file");
            }
        }
    }

    /// A claim that crashed after every member recorded the commit, before
    /// any moved to cleanup, retires on restart.
    ///
    /// # Panics
    ///
    /// Panics when recovery leaves the claim, its bytes, or its authority.
    #[tokio::test]
    async fn a_claim_crashed_after_its_published_step_retires_on_restart() {
        recovers_claim_interrupted_between_batched_states(false, 0, false).await;
    }

    /// A claim that crashed after every member moved to cleanup, before any
    /// was removed, retires on restart.
    ///
    /// # Panics
    ///
    /// Panics when recovery leaves the claim, its bytes, or its authority.
    #[tokio::test]
    async fn a_claim_crashed_after_its_cleanup_step_retires_on_restart() {
        recovers_claim_interrupted_between_batched_states(true, 0, false).await;
    }

    /// A claim that crashed partway through its batched removal retires its
    /// surviving members on restart.
    ///
    /// # Panics
    ///
    /// Panics when recovery leaves the claim, its bytes, or its authority.
    #[tokio::test]
    async fn a_claim_crashed_inside_its_batched_removal_retires_on_restart() {
        recovers_claim_interrupted_between_batched_states(true, 2, false).await;
    }

    /// A claim that crashed inside one member's removal — a member left with
    /// its record but not its runs, another with its runs but not its record —
    /// retires every survivor on restart instead of refusing to start.
    ///
    /// # Panics
    ///
    /// Panics when recovery refuses a half-removed member or leaves the
    /// claim, its bytes, a member directory, or its authority.
    #[tokio::test]
    async fn a_claim_crashed_inside_one_member_removal_retires_on_restart() {
        recovers_claim_interrupted_between_batched_states(true, 1, true).await;
    }
}

/// Postgres-backed recovery proofs for the staged backlog gauges.
#[cfg(test)]
mod pg_tests {
    use super::tests::{publisher, runtime_layout, runtime_schema, stage_durable_members};
    use super::*;
    use crate::scribe::stream_identity::NodeId;
    use num_traits::ToPrimitive as _;

    /// Registers the control row recovery re-resolves the fixture recipe from.
    async fn register_control_row(database: &wyrd_dev_fixtures::pg::PgFixture) {
        let table = crate::catalog::TableRef::new(
            crate::namespaces::BifrostNamespace::Bifrost,
            "staged_runtime",
        );
        let fqn = table.fqn();
        let schema = runtime_schema();
        let mut conn = database
            .vala_postgres()
            .tenant_conn(database.data_tenant_id())
            .await
            .expect("fixture tenant connection");
        vala_sql::queries::olap_catalog::upsert_table(
            &mut conn,
            uuid::Uuid::now_v7().as_bytes(),
            &fqn,
            &[0_u8; 32],
            &serde_json::to_value(runtime_layout(schema.as_ref()).to_wire())
                .expect("fixture layout encodes"),
        )
        .await
        .expect("register the fixture control row");
        conn.commit().await.expect("commit the fixture control row");
    }

    /// Restoration publishes the recovered staged backlog before Scribe serves.
    ///
    /// Two members are claimed durably and two more stay ready. A replacement
    /// runtime over the retained namespace restores them under a fresh,
    /// isolated recorder: the staged gauges are absent before the actual
    /// asynchronous restore and afterwards equal the durable members' count,
    /// encoded bytes, oldest persisted ready time, and outstanding claim.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot stage, claim, or restore, or when the
    /// published gauges disagree with the durable members.
    #[tokio::test(flavor = "current_thread")]
    async fn restored_stage_republishes_backlog() {
        let database = wyrd_dev_fixtures::pg::PgFixture::start()
            .await
            .expect("Postgres fixture");
        register_control_row(&database).await;
        let tenant = database.data_tenant_id();
        let root = tempfile::tempdir().expect("runtime root");
        let stage_root = root.path().join("stage");
        let wal_root = root.path().join("member-wal");
        for path in [&stage_root, &wal_root] {
            std::fs::create_dir_all(path).expect("fixture directory");
        }
        let node_id = NodeId::new(uuid::Uuid::from_u128(0xb4c7));
        let stage = Arc::new(ScribeHotStage::new(stage_root));
        let config =
            StagingAssemblerConfig::new(512 * 1024 * 1024, std::time::Duration::from_mins(5), 4)
                .expect("assembler controls");
        let oldest = DateTime::from_timestamp(1_780_000_000, 0).expect("fixture ready time");
        let runtime = ScribeStagingRuntime::new(
            Arc::clone(&stage),
            publisher(Arc::clone(&stage), &wal_root, node_id),
            config,
        );
        let (key, _) = stage_durable_members(&runtime, tenant, node_id, 1..=2, oldest).await;
        let claim = runtime
            .take_residue(&key, ClaimCause::Drain)
            .expect("residue claim")
            .expect("two ready members form one claim");
        runtime
            .gather(&claim)
            .await
            .expect("the claim's membership becomes durable");
        stage_durable_members(
            &runtime,
            tenant,
            node_id,
            3..=4,
            oldest + chrono::Duration::seconds(60),
        )
        .await;
        drop(runtime);

        let durable = stage.recover().await.expect("durable members");
        let durable_bytes = durable
            .values()
            .flatten()
            .map(|member| member.record().encoded_bytes())
            .sum::<u64>();
        let durable_members = durable.values().map(Vec::len).sum::<usize>();
        assert_eq!(durable_members, 4);

        let replacement = ScribeStagingRuntime::new(
            Arc::clone(&stage),
            publisher(Arc::clone(&stage), &wal_root, node_id),
            config,
        );
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let _metrics = metrics::set_default_local_recorder(&recorder);
        let staged_families = [
            "bifrost_scribe_staging_live_members",
            "bifrost_scribe_staging_live_bytes",
            "bifrost_scribe_staging_oldest_member_timestamp_seconds",
            "bifrost_scribe_staging_outstanding_claims",
        ];
        let before = recorder.snapshot();
        assert!(
            staged_families
                .iter()
                .all(|family| !before.gauges.contains_key(*family)),
            "no staged gauge exists before restoration"
        );

        let restored = replacement
            .restore(database.operator_pool().pool())
            .await
            .expect("staged namespace restores");
        assert_eq!(restored, durable_members);

        let after = recorder.snapshot();
        let gauge = |family: &str| after.gauges.get(family).copied();
        assert_eq!(
            gauge("bifrost_scribe_staging_live_members"),
            durable_members.to_f64()
        );
        assert_eq!(
            gauge("bifrost_scribe_staging_live_bytes"),
            durable_bytes.to_f64()
        );
        assert_eq!(
            gauge("bifrost_scribe_staging_oldest_member_timestamp_seconds"),
            oldest.timestamp().to_f64()
        );
        assert_eq!(
            gauge("bifrost_scribe_staging_outstanding_claims"),
            Some(1.0)
        );
    }

    /// Builds a publisher whose fenced transaction runs against `pool`.
    fn fenced_publisher(
        stage: Arc<ScribeHotStage>,
        wal_root: &Path,
        node: NodeId,
        pool: vala_sql::OperatorPool,
    ) -> ClaimPublisher {
        let operator = opendal::Operator::new(opendal::services::Memory::default())
            .expect("memory operator")
            .finish();
        ClaimPublisher::new(
            stage,
            crate::scribe::persistence::ScribeStageMover::new(wal_root, operator),
            crate::scribe::persistence::ScribePublicationReconciler::new(
                pool,
                StreamIdentity::new(node, crate::scribe::stream_identity::WriterEpoch::new(1)),
                crate::scribe::persistence::PersistenceFaults::default(),
            ),
        )
    }

    /// Gathers, merges, and publishes one claim the way persistence does.
    ///
    /// # Errors
    ///
    /// Returns the first gather, merge, or publication refusal.
    async fn publish_claim_once(
        runtime: &ScribeStagingRuntime,
        claim: &StagingClaim,
        scratch: &Path,
        node: NodeId,
    ) -> Result<PublishedClaim, ScribeError> {
        let runs = runtime.gather(claim).await?;
        std::fs::create_dir_all(scratch).expect("claim scratch");
        let assembled = runtime.assemble(
            claim,
            AssembleRequest {
                runs: &runs,
                scratch_dir: scratch,
                memory: crate::resources::ScribeResources::for_test(),
            },
        )?;
        runtime
            .publish(
                claim,
                &runs,
                &assembled,
                StreamIdentity::new(node, crate::scribe::stream_identity::WriterEpoch::new(1)),
            )
            .await
    }

    /// A claim whose members reached `Publishing` and whose commit never
    /// landed publishes once a restarted runtime resumes it.
    ///
    /// The first runtime's node holds no publication fence, so its fenced
    /// transaction is refused after every member durably entered
    /// `Publishing` — the state a crash between that step and the commit
    /// leaves. A restarted runtime over the same namespace, now fenced,
    /// restores the claim under its original identity and must carry it
    /// through the commit rather than refusing to move its members.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot stage or claim, when the unfenced attempt
    /// commits, or when the resumed claim does not publish and retire.
    #[tokio::test(flavor = "current_thread")]
    async fn a_claim_stranded_in_publishing_publishes_after_restart() {
        let database = wyrd_dev_fixtures::pg::PgFixture::start()
            .await
            .expect("Postgres fixture");
        register_control_row(&database).await;
        let tenant = database.data_tenant_id();
        let root = tempfile::tempdir().expect("runtime root");
        let stage_root = root.path().join("stage");
        let wal_root = root.path().join("member-wal");
        for path in [&stage_root, &wal_root] {
            std::fs::create_dir_all(path).expect("fixture directory");
        }
        let node_id = NodeId::generate();
        let stage = Arc::new(ScribeHotStage::new(stage_root));
        let config =
            StagingAssemblerConfig::new(512 * 1024 * 1024, std::time::Duration::from_mins(5), 4)
                .expect("assembler controls");
        let runtime = ScribeStagingRuntime::new(
            Arc::clone(&stage),
            fenced_publisher(
                Arc::clone(&stage),
                &wal_root,
                node_id,
                database.operator_pool().clone(),
            ),
            config,
        );
        let (key, member_ids) =
            stage_durable_members(&runtime, tenant, node_id, 1..=2, chrono::Utc::now()).await;
        let claim = runtime
            .take_residue(&key, ClaimCause::Drain)
            .expect("residue claim")
            .expect("two ready members form one claim");
        publish_claim_once(&runtime, &claim, &root.path().join("scratch-0"), node_id)
            .await
            .expect_err("an unfenced node cannot commit the claim");
        for member in &member_ids {
            assert_eq!(
                stage
                    .member(&key, *member)
                    .await
                    .expect("member survives the refusal")
                    .record()
                    .state()
                    .label(),
                "publishing"
            );
        }
        drop(runtime);

        let superuser = database.superuser_pool().await.expect("superuser pool");
        sqlx::query(
            "INSERT INTO vala.cluster_nodes (data_tenant_id,node_id,role,advertise_addr,fencing_token,started_at,heartbeat_at) VALUES ($1,$2,'scribe','127.0.0.1:1',$3,now(),now())",
        )
        .bind(wyrd_spec::DataTenantId::SYSTEM_OWNER.as_uuid())
        .bind(node_id.as_uuid())
        .bind(1_i64)
        .execute(&superuser)
        .await
        .expect("register the restarted node's publication fence");
        let restarted = ScribeStagingRuntime::new(
            Arc::clone(&stage),
            fenced_publisher(
                Arc::clone(&stage),
                &wal_root,
                node_id,
                database.operator_pool().clone(),
            ),
            config,
        );
        restarted
            .restore(database.operator_pool().pool())
            .await
            .expect("the stranded claim restores");
        let resumed = restarted
            .resumable_claims()
            .expect("claim index")
            .pop()
            .expect("the stranded claim is outstanding");
        assert_eq!(resumed.id(), claim.id(), "the claim resumes as itself");
        publish_claim_once(
            &restarted,
            &resumed,
            &root.path().join("scratch-1"),
            node_id,
        )
        .await
        .expect("the resumed claim publishes");
        assert!(stage.recover().await.expect("stage rescans").is_empty());
        assert!(
            restarted
                .resumable_claims()
                .expect("claim index")
                .is_empty()
        );
    }
}
