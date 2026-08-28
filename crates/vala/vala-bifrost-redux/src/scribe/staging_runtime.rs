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
    /// Encoding context recorded for every key that has staged a member.
    contexts: Mutex<HashMap<ScribeAssemblyKey, ClaimContext>>,
    /// Approximate encoded size at which one published object closes.
    target_object_bytes: u64,
    /// Pod-wide observation owner, when this runtime belongs to a pod.
    ///
    /// A fixture runtime built without one publishes no staged lifecycle
    /// effects; production always binds the same owner admission publishes to,
    /// so the pod has one set of reconcilable totals rather than two.
    telemetry: Option<Arc<crate::scribe::telemetry::ScribeTelemetry>>,
    /// Pod-wide authority registry, when this runtime belongs to a pod.
    ///
    /// Staging and publication are the two moments a generation's rows change
    /// hands, so this owner is what moves the authority forward. A fixture
    /// runtime built without a pod tracks no authority and none is asked of it.
    hot_sources: Option<Arc<crate::scribe::hot_source::ScribeHotSourceRegistry>>,
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
            stage: Arc::clone(&stage),
            stager: ScribeMemberStager::new(Arc::clone(&stage), volume),
            claims: ClaimAssembler::new(stage),
            publisher,
            assembly: Mutex::new(StagingAssembler::new(config)),
            contexts: Mutex::new(HashMap::new()),
            target_object_bytes: config.target_file_size_bytes(),
            telemetry: None,
            hot_sources: None,
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

    /// Binds this runtime to its pod's single observation owner.
    ///
    /// Production wiring calls this before the runtime stages anything, so
    /// every durable transition it performs is published through the same owner
    /// admission and contention publish through.
    #[must_use]
    pub(crate) fn with_telemetry(
        mut self,
        telemetry: Arc<crate::scribe::telemetry::ScribeTelemetry>,
    ) -> Self {
        self.telemetry = Some(telemetry);
        self
    }

    /// Publishes one staged or claim lifecycle effect, when a pod owns this runtime.
    fn observe(
        &self,
        effect: crate::scribe::telemetry::StagingEffect,
        facts: crate::scribe::telemetry::StagingFacts,
    ) {
        if let Some(telemetry) = &self.telemetry {
            telemetry.record_staging(effect, facts);
        }
    }

    /// Publishes the effect one released claim records, whatever released it.
    fn observe_claim(&self, effect: crate::scribe::telemetry::StagingEffect, claim: &StagingClaim) {
        self.observe(
            effect,
            crate::scribe::telemetry::StagingFacts {
                members: claim.members().len(),
                bytes: claim.encoded_bytes(),
                artifacts: 0,
                cause: Some(claim.cause().label()),
            },
        );
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
        self.assembly
            .lock()
            .map_err(|_| poisoned("staged ready index"))?
            .register_ready(&key, ready)
            .map_err(|error| ScribeError::Internal {
                detail: format!("register the staged member as ready: {error}"),
            })?;
        self.observe(
            crate::scribe::telemetry::StagingEffect::MemberStaged,
            crate::scribe::telemetry::StagingFacts {
                members: 1,
                bytes,
                ..crate::scribe::telemetry::StagingFacts::default()
            },
        );
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
        self.observe(
            crate::scribe::telemetry::StagingEffect::SourceTransitioned,
            crate::scribe::telemetry::StagingFacts {
                members: 1,
                ..crate::scribe::telemetry::StagingFacts::default()
            },
        );
        Ok(())
    }

    /// Takes the next claim any tenant is entitled to, if one is due.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the ready index is unavailable or
    /// a key is due while every claim slot is held. A full budget with nothing
    /// due is not an error; it returns `Ok(None)` like any other idle poll.
    pub fn take_claim(&self, now: DateTime<Utc>) -> Result<Option<StagingClaim>, ScribeError> {
        let claim = self
            .assembly
            .lock()
            .map_err(|_| poisoned("staged ready index"))?
            .next_claim(now)
            .map_err(|error| ScribeError::Internal {
                detail: format!("take the next due staging claim: {error}"),
            })?;
        if let Some(claim) = &claim {
            self.observe_claim(crate::scribe::telemetry::StagingEffect::ClaimTaken, claim);
        }
        Ok(claim)
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
        let claim = self
            .assembly
            .lock()
            .map_err(|_| poisoned("staged ready index"))?
            .claim_residue(key, cause)
            .map_err(|error| ScribeError::Internal {
                detail: format!("take the residue claim for a staged key: {error}"),
            })?;
        if let Some(claim) = &claim {
            self.observe_claim(crate::scribe::telemetry::StagingEffect::ClaimTaken, claim);
        }
        Ok(claim)
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
    /// refuses a duplicate member, or the governed volume cannot re-admit the
    /// bytes already on it.
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
            let staged_bytes = members.iter().fold(0_u64, |total, member| {
                total.saturating_add(member.record().encoded_bytes())
            });
            self.restore_authorities(&key, &members)?;
            self.stager.readmit_staged_bytes(staged_bytes)?;
            let terminal = self
                .publisher
                .recover_terminal_members(&key, &members)
                .await?;
            self.stager.release_staged_bytes(terminal.released_bytes)?;
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
                self.assembly
                    .lock()
                    .map_err(|_| poisoned("staged ready index"))?
                    .restore(&key, members_to_restore)
                    .map_err(|error| ScribeError::Internal {
                        detail: format!("restore a recovered staged key: {error}"),
                    })?;
                self.contexts
                    .lock()
                    .map_err(|_| poisoned("staged claim context registry"))?
                    .insert(key.clone(), context);
            }
            self.observe(
                crate::scribe::telemetry::StagingEffect::StagingRestored,
                crate::scribe::telemetry::StagingFacts {
                    members: members.len(),
                    bytes: staged_bytes,
                    ..crate::scribe::telemetry::StagingFacts::default()
                },
            );
        }
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
                } => crate::scribe::hot_source::HotAuthority::Published {
                    object_key: published_object_identities
                        .first()
                        .cloned()
                        .ok_or_else(|| ScribeError::Internal {
                            detail: "recovered published member names no object identity"
                                .to_owned(),
                        })?,
                },
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
        if crate::parquet::memory::schema_fingerprint(schema.as_ref()) != key.schema_fingerprint() {
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
        let published = match self
            .publisher
            .publish(PublishClaimRequest {
                claim,
                runs,
                assembled,
                binding: &context.binding,
                object_base: &object_base,
                actor_stream,
            })
            .await
        {
            Ok(published) => published,
            Err(error) => {
                self.observe_claim(crate::scribe::telemetry::StagingEffect::ClaimFailed, claim);
                return Err(error);
            }
        };
        self.observe(
            crate::scribe::telemetry::StagingEffect::ClaimPublished,
            crate::scribe::telemetry::StagingFacts {
                members: claim.members().len(),
                bytes: claim.encoded_bytes(),
                artifacts: published.object_identities.len(),
                cause: Some(claim.cause().label()),
            },
        );
        self.observe(
            crate::scribe::telemetry::StagingEffect::MemberRetired,
            crate::scribe::telemetry::StagingFacts {
                members: claim.members().len(),
                bytes: claim.encoded_bytes(),
                artifacts: 0,
                cause: Some(claim.cause().label()),
            },
        );
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
        self.stager.release_staged_bytes(released_bytes)?;
        self.observe(
            crate::scribe::telemetry::StagingEffect::ClaimSettled,
            crate::scribe::telemetry::StagingFacts {
                bytes: released_bytes,
                ..crate::scribe::telemetry::StagingFacts::default()
            },
        );
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
            staging_volume(root.path(), &stage_root),
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
                    footer_reservation: crate::scribe::memory::EncodedFooterReservation::for_test(),
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
            crate::parquet::memory::schema_fingerprint(on_disk.as_ref()),
            key.schema_fingerprint(),
            "the schema restore reads back is the schema the key names"
        );
    }

    /// Members that reach neither target nor dwell are invisible to
    /// [`ScribeStagingRuntime::take_claim`] but are exactly what drain must
    /// settle, so every ready key is reachable as residue and each key stops
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
            staging_volume(root.path(), &stage_root),
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
                        footer_reservation:
                            crate::scribe::memory::EncodedFooterReservation::for_test(),
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

    /// The staged registry is closed and its production totals reconcile.
    ///
    /// AC21's registry is only worth having if every entry is distinguishable
    /// and every emission moves a total a maintainer can check against durable
    /// state. This drives the runtime's own public surfaces and asserts what
    /// the transitions published, rather than asserting a counter directly.
    ///
    /// # Panics
    ///
    /// Panics when two registry entries share a stage/decision pair, when a
    /// durable transition publishes nothing, or when the reconcilable totals
    /// disagree with the members and claims the runtime actually holds.
    #[tokio::test(flavor = "current_thread")]
    async fn staged_lifecycle_effects_are_closed_and_totals_reconcile() {
        use crate::scribe::telemetry::{ScribeTelemetry, StagingEffect};

        let mut seen = std::collections::HashSet::new();
        for effect in StagingEffect::ALL {
            assert!(
                seen.insert((effect.stage(), effect.decision())),
                "two staged registry entries share the stage/decision pair {}/{}",
                effect.stage(),
                effect.decision()
            );
            assert_eq!(
                StagingEffect::ALL[effect.index()],
                effect,
                "every entry indexes its own position"
            );
        }

        let root = tempfile::tempdir().expect("runtime root");
        let stage_root = root.path().join("stage");
        let wal_root = root.path().join("member-wal");
        for path in [&stage_root, &wal_root] {
            std::fs::create_dir_all(path).expect("fixture directory");
        }
        let node_id = NodeId::new(uuid::Uuid::from_u128(0xd2a3));
        let stage = Arc::new(ScribeHotStage::new(stage_root.clone()));
        let telemetry = Arc::new(ScribeTelemetry::default());
        let hot_sources = Arc::new(crate::scribe::hot_source::ScribeHotSourceRegistry::new());
        let runtime = ScribeStagingRuntime::new(
            Arc::clone(&stage),
            staging_volume(root.path(), &stage_root),
            publisher(stage, &wal_root, node_id),
            StagingAssemblerConfig::new(512 * 1024 * 1024, std::time::Duration::from_mins(5), 4)
                .expect("assembler controls"),
        )
        .with_hot_sources(Arc::clone(&hot_sources))
        .with_telemetry(Arc::clone(&telemetry));

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
                    footer_reservation: crate::scribe::memory::EncodedFooterReservation::for_test(),
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

        let staged_totals = telemetry.staging_snapshot();
        assert_eq!(staged_totals.count(StagingEffect::MemberStaged), 1);
        assert_eq!(staged_totals.count(StagingEffect::SourceTransitioned), 1);
        assert_eq!(
            staged_totals.live_members(),
            1,
            "one durable member is staged and none has been retired"
        );
        assert_eq!(staged_totals.outstanding_claims(), 0);

        let claim = runtime
            .take_residue(&key, ClaimCause::Drain)
            .expect("the ready key releases a residue claim")
            .expect("a residue claim is due");
        let claimed = telemetry.staging_snapshot();
        assert_eq!(claimed.count(StagingEffect::ClaimTaken), 1);
        assert_eq!(
            claimed.outstanding_claims(),
            1,
            "the claim is outstanding until it settles or fails"
        );
        assert_eq!(claim.members().len(), 1);
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
            staging_volume(root.path(), &stage_root),
            publisher(Arc::clone(&stage), &wal_root, node_id),
            StagingAssemblerConfig::new(512 * 1024 * 1024, std::time::Duration::from_mins(5), 4)
                .expect("assembler controls"),
        );
        let tenant = DataTenantId::new_v7();
        let schema = runtime_schema();
        let layout = runtime_layout(schema.as_ref());
        let mut key = None;
        let mut member_ids = Vec::new();
        for shard in 1_u8..=4 {
            let frozen = frozen_member(tenant, 64, shard);
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
                            shard: u16::from(shard),
                            generation: u64::from(shard),
                            wal: StagedLsnRange {
                                min: u64::from(shard) * 10,
                                max: u64::from(shard) * 10 + 9,
                            },
                        },
                        footer_reservation:
                            crate::scribe::memory::EncodedFooterReservation::for_test(),
                    },
                    ClaimContext {
                        schema: Arc::clone(&schema),
                        layout: layout.clone(),
                        binding: binding.clone(),
                    },
                )
                .expect("member stages");
            key.get_or_insert_with(|| staged.key().clone());
            member_ids.push(staged.member());
            runtime
                .register_member(staged, chrono::Utc::now())
                .await
                .expect("member becomes durable and ready");
        }
        let key = key.expect("one assembly key");
        let claim = runtime
            .take_residue(&key, ClaimCause::Drain)
            .expect("residue claim")
            .expect("four members form one claim");
        assert_eq!(claim.members().len(), 4);
        let claim_id = claim.id().to_string();
        let objects = vec![format!("objects/{claim_id}/hot-0.parquet")];
        stage
            .transition(
                &key,
                member_ids[0],
                crate::scribe::hot_stage::StagedMemberState::Publishing {
                    claim_id: claim_id.clone(),
                    operation_id: uuid::Uuid::from_u128(0xc01),
                },
            )
            .await
            .expect("first member remains publishing");
        for (index, state) in [
            crate::scribe::hot_stage::StagedMemberState::Published {
                claim_id: claim_id.clone(),
                file_list_commit_key: "node:10:49".to_owned(),
                published_object_identities: objects.clone(),
                persisted_lsn_ranges: vec![StagedLsnRange { min: 20, max: 29 }],
            },
            crate::scribe::hot_stage::StagedMemberState::CleanupPending {
                claim_id: claim_id.clone(),
                file_list_commit_key: "node:10:49".to_owned(),
                published_object_identities: objects.clone(),
                persisted_lsn_ranges: vec![StagedLsnRange { min: 30, max: 39 }],
            },
            crate::scribe::hot_stage::StagedMemberState::Published {
                claim_id: claim_id.clone(),
                file_list_commit_key: "node:10:49".to_owned(),
                published_object_identities: objects,
                persisted_lsn_ranges: vec![StagedLsnRange { min: 40, max: 49 }],
            },
        ]
        .into_iter()
        .enumerate()
        {
            stage
                .transition(&key, member_ids[index + 1], state)
                .await
                .expect("terminal member state persists");
        }
        stage
            .retire(&key, member_ids[3])
            .await
            .expect("one member retired before the crash");
        drop(runtime);

        let hot_sources = Arc::new(crate::scribe::hot_source::ScribeHotSourceRegistry::new());
        let recovered = ScribeStagingRuntime::new(
            Arc::clone(&stage),
            staging_volume(root.path(), &stage_root),
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
        assert_eq!(recovered.restore(&pool).await.expect("cleanup replays"), 0);
    }
}
