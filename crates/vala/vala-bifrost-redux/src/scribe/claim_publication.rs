//! Claim publication — making one assembled claim's objects authoritative.
//!
//! Assembly produces sealed local objects; this module is what turns them into
//! the rows Oracle reads. The order is the only order that keeps both durable
//! boundaries: the objects are staged and uploaded first, then the fenced
//! `file_list` and audit transaction commits, and only after that commit do the
//! contributing members stop being the live-tail authority for their rows.
//!
//! Nothing here decides *which* members publish. The assembler chose them and
//! [`ClaimAssembler`](crate::scribe::claim_assembly::ClaimAssembler) merged
//! them; this owner carries that decision across the durable boundary and
//! reports what committed so its caller can settle the claim and retire the
//! members' staged bytes.

use std::sync::Arc;

use crate::catalog::TenantTableBinding;
use crate::contracts::ScribeError;
use crate::parquet::object_uploader::VerifiedParquetObject;
use crate::scribe::assembly::StagingClaim;
use crate::scribe::claim_assembly::{AssembledClaim, ClaimRuns};
use crate::scribe::file_list_writer::{self, FileListCommitKey};
use crate::scribe::hot_stage::{ScribeHotStage, StagedMemberState};
use crate::scribe::memory::PARQUET_TRANSFER_BUFFER_BYTES;
use crate::scribe::persistence::{
    ScribePublicationOutcome, ScribePublicationReconciler, ScribeStageMover,
};
use crate::scribe::stream_identity::StreamIdentity;

/// Borrowed inputs for publishing one assembled claim.
pub struct PublishClaimRequest<'a> {
    /// The claim whose members produced the objects.
    pub claim: &'a StagingClaim,
    /// Runs gathered for the claim, carrying its WAL span and audit envelope.
    pub runs: &'a ClaimRuns,
    /// Sealed objects the claim produced, in publication order.
    pub assembled: &'a AssembledClaim,
    /// Tenant-qualified physical binding the rows are published under.
    pub binding: &'a TenantTableBinding,
    /// Deterministic object prefix the claim's objects were sealed under.
    pub object_base: &'a str,
    /// Fenced writer identity authorizing the publication transaction.
    pub actor_stream: StreamIdentity,
}

/// What one committed claim publication left behind.
#[derive(Debug, Clone)]
pub struct PublishedClaim {
    /// Commit key of the fenced `file_list` transaction.
    pub commit_key: FileListCommitKey,
    /// Object identities published, in artifact-ordinal order.
    pub object_identities: Vec<String>,
    /// Staged bytes the retired members released back to the staging volume.
    pub released_bytes: u64,
}

/// Publishes assembled claims and retires the members they replace.
pub struct ClaimPublisher {
    /// Durable staged namespace holding the members being replaced.
    stage: Arc<ScribeHotStage>,
    /// Local election, publication manifest, and verified upload owner.
    mover: ScribeStageMover,
    /// Fenced `file_list` and audit transaction owner.
    reconciler: ScribePublicationReconciler,
    /// Pod authority registry whose leases cleanup waits on, when one is owned.
    hot_sources: Option<Arc<crate::scribe::hot_source::ScribeHotSourceRegistry>>,
    /// Pod-wide production telemetry owner, when composed by the runtime.
    telemetry: Option<Arc<crate::scribe::telemetry::ScribeTelemetry>>,
}

impl ClaimPublisher {
    /// Binds the publisher to the staged namespace, uploader, and fenced owner.
    #[must_use]
    pub const fn new(
        stage: Arc<ScribeHotStage>,
        mover: ScribeStageMover,
        reconciler: ScribePublicationReconciler,
    ) -> Self {
        Self {
            stage,
            mover,
            reconciler,
            hot_sources: None,
            telemetry: None,
        }
    }

    /// Binds publication to the pod-wide production telemetry owner.
    #[must_use]
    pub(crate) fn with_telemetry(
        mut self,
        telemetry: Arc<crate::scribe::telemetry::ScribeTelemetry>,
    ) -> Self {
        self.telemetry = Some(telemetry);
        self
    }

    /// Binds the authority registry whose staged leases cleanup must respect.
    ///
    /// A publisher without one still publishes correctly; it simply removes a
    /// member's directory as soon as the fenced transaction commits, which is
    /// only safe where no live-tail reader can be holding those runs.
    pub fn set_hot_sources(
        &mut self,
        hot_sources: Arc<crate::scribe::hot_source::ScribeHotSourceRegistry>,
    ) {
        self.hot_sources = Some(hot_sources);
    }

    /// Moves every published member's authority to the committed hot objects.
    ///
    /// This runs between the fenced commit and retirement, and the order is
    /// what makes retirement safe: once a member is `Published` no further
    /// staged lease can be taken for it, so waiting for the leases already held
    /// is a wait that terminates. Advancing after retirement instead would
    /// leave a window in which a reader could lease runs cleanup had removed.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the registry refuses the move,
    /// which means the member's generation is unknown to it or already past
    /// publication — either way retirement must not proceed on that evidence.
    fn advance_published(
        &self,
        request: &PublishClaimRequest<'_>,
        object_identities: &[String],
    ) -> Result<(), ScribeError> {
        let Some(hot_sources) = &self.hot_sources else {
            return Ok(());
        };
        let key = request.claim.key();
        let seal_key = crate::scribe::seal_key::SealKey::new(
            key.tenant(),
            key.table().clone(),
            key.partition(),
        );
        let object_key = object_identities
            .first()
            .cloned()
            .unwrap_or_else(|| request.object_base.to_owned());
        let transitions = request
            .claim
            .members()
            .iter()
            .map(|member| {
                (
                    crate::scribe::hot_source::GenerationOrdinal::new(
                        member.id().shard(),
                        member.id().generation(),
                    ),
                    crate::scribe::hot_source::HotAuthority::Published {
                        object_key: object_key.clone(),
                    },
                )
            })
            .collect::<Vec<_>>();
        hot_sources
            .advance_all_atomic(&seal_key, &transitions)
            .map_err(|error| ScribeError::Internal {
                detail: format!("move complete claim authority to its hot objects: {error}"),
            })
    }

    /// Releases one published member's authority from the pod registry.
    ///
    /// This is the last step of the handover the registry exists to describe:
    /// the catalog now serves the rows, the member's local runs are gone, and
    /// no reader can still be sent to them. Releasing earlier — when the
    /// memtable dropped its Arrow copy, say — would remove the staged-run
    /// authority a live-tail reader still needs.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the registry refuses the release,
    /// which means the member is unknown to it or nothing durable holds its
    /// rows — either way the publication must not report a clean handover.
    fn release_authority(
        &self,
        key: &crate::scribe::assembly::ScribeAssemblyKey,
        member: crate::scribe::assembly::StagedMemberId,
    ) -> Result<(), ScribeError> {
        let Some(hot_sources) = &self.hot_sources else {
            return Ok(());
        };
        hot_sources
            .release(
                &crate::scribe::seal_key::SealKey::new(
                    key.tenant(),
                    key.table().clone(),
                    key.partition(),
                ),
                crate::scribe::hot_source::GenerationOrdinal::new(
                    member.shard(),
                    member.generation(),
                ),
            )
            .map(|_| ())
            .map_err(|error| ScribeError::Internal {
                detail: format!(
                    "release the authority of published staged member {}-{}: {error}",
                    member.shard(),
                    member.generation()
                ),
            })
    }

    /// Waits for every live-tail read holding one member's runs to finish.
    ///
    /// The member's authority is already `Published` by the time cleanup runs,
    /// so no further lease can be taken and this waits only on reads that were
    /// already in flight.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the registry lock is poisoned,
    /// which leaves the member's directory in place rather than deleting runs
    /// a reader may still open.
    async fn await_lease_drain(
        &self,
        key: &crate::scribe::assembly::ScribeAssemblyKey,
        member: crate::scribe::assembly::StagedMemberId,
    ) -> Result<(), ScribeError> {
        let Some(hot_sources) = &self.hot_sources else {
            return Ok(());
        };
        hot_sources
            .drain_leases(
                &crate::scribe::seal_key::SealKey::new(
                    key.tenant(),
                    key.table().clone(),
                    key.partition(),
                ),
                crate::scribe::hot_source::GenerationOrdinal::new(
                    member.shard(),
                    member.generation(),
                ),
            )
            .await
            .map_err(|error| ScribeError::Internal {
                detail: format!(
                    "wait for the readers of staged member {}-{} to finish: {error}",
                    member.shard(),
                    member.generation()
                ),
            })
    }

    /// Publishes one assembled claim and retires its contributing members.
    ///
    /// Every step before the fenced commit is repeatable: staging elects the
    /// same local winner, upload converges the same verified object, and the
    /// publication manifest names the same rows. An uncertain commit therefore
    /// leaves the members claimed and the WAL authoritative, which is what a
    /// retry needs to resume the identical publication.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the rows cannot be built, staging
    /// or upload fails, the fenced transaction is refused or uncertain, or a
    /// member cannot be moved forward through its durable lifecycle.
    ///
    /// # Cancellation
    ///
    /// Cancellation before the commit leaves elected stages, uploaded objects,
    /// and a publication manifest as deterministic recovery evidence.
    /// Cancellation after it leaves members published and their local files
    /// present; retirement is idempotent and resumes.
    pub async fn publish(
        &self,
        request: PublishClaimRequest<'_>,
    ) -> Result<PublishedClaim, ScribeError> {
        let rows = file_list_writer::build_claim_inserts(
            request.claim.key(),
            &request.assembled.artifacts,
            request.binding,
            request.runs.wal(),
        )?;
        let events = request
            .runs
            .publication_audit()
            .map(|event| {
                crate::scribe::audit_envelope::publication_audit_events(std::slice::from_ref(event))
            })
            .unwrap_or_default();
        let operation_id = uuid::Uuid::new_v4();
        self.move_members(
            request.claim,
            StagedMemberState::Publishing {
                claim_id: request.claim.id().to_string(),
                operation_id,
            },
        )
        .await?;
        let mut chunk = vec![0_u8; PARQUET_TRANSFER_BUFFER_BYTES];
        let upload = self.telemetry.as_ref().map(|telemetry| {
            telemetry.start_effect(
                crate::scribe::telemetry::ScribeEffect::UploadStarted,
                crate::scribe::telemetry::ScribeEffect::UploadSettled,
                crate::scribe::telemetry::ScribeEffectFacts {
                    bytes: request.claim.encoded_bytes(),
                    outcome: crate::scribe::telemetry::ScribeEffectOutcome::Started,
                    reason: crate::scribe::telemetry::ScribeEffectReason::ClaimAssembled,
                    ..crate::scribe::telemetry::ScribeEffectFacts::default()
                },
            )
        });
        let uploaded = self
            .mover
            .stage_and_upload_candidate(
                request.object_base,
                &request.assembled.artifacts,
                &mut chunk,
            )
            .await;
        let (claims, verified) = match uploaded {
            Ok(uploaded) => {
                if let Some(upload) = upload {
                    upload.settle(
                        crate::scribe::telemetry::ScribeEffectOutcome::Success,
                        crate::scribe::telemetry::ScribeEffectReason::Verified,
                        u64::try_from(uploaded.1.len()).unwrap_or(u64::MAX),
                    );
                }
                uploaded
            }
            Err(error) => {
                if let Some(upload) = upload {
                    upload.settle(
                        crate::scribe::telemetry::ScribeEffectOutcome::Failed,
                        crate::scribe::telemetry::ScribeEffectReason::UploadRefused,
                        0,
                    );
                }
                return Err(error);
            }
        };
        validate_promotion_records(&rows, &verified)?;
        self.mover
            .persist_publication(
                request.object_base,
                request.actor_stream,
                &rows,
                &events,
                &claims,
            )
            .await?;
        if let Some(telemetry) = &self.telemetry {
            telemetry.record_effect(
                crate::scribe::telemetry::ScribeEffect::PublicationManifestDurable,
                crate::scribe::telemetry::ScribeEffectFacts {
                    rows: request.assembled.rows,
                    bytes: request.claim.encoded_bytes(),
                    artifacts: u64::try_from(rows.len()).unwrap_or(u64::MAX),
                    outcome: crate::scribe::telemetry::ScribeEffectOutcome::Success,
                    reason: crate::scribe::telemetry::ScribeEffectReason::Fsynced,
                },
            );
        }
        let file_list = self.telemetry.as_ref().map(|telemetry| {
            telemetry.start_effect(
                crate::scribe::telemetry::ScribeEffect::FileListCommitStarted,
                crate::scribe::telemetry::ScribeEffect::FileListCommitSettled,
                crate::scribe::telemetry::ScribeEffectFacts {
                    rows: request.assembled.rows,
                    artifacts: u64::try_from(rows.len()).unwrap_or(u64::MAX),
                    outcome: crate::scribe::telemetry::ScribeEffectOutcome::Started,
                    reason: crate::scribe::telemetry::ScribeEffectReason::ManifestDurable,
                    ..crate::scribe::telemetry::ScribeEffectFacts::default()
                },
            )
        });
        let outcome = match self.reconciler.publish(&rows, &events).await {
            ScribePublicationOutcome::Committed(outcome) => {
                if let Some(file_list) = file_list {
                    file_list.settle(
                        crate::scribe::telemetry::ScribeEffectOutcome::Success,
                        crate::scribe::telemetry::ScribeEffectReason::Committed,
                        u64::try_from(rows.len()).unwrap_or(u64::MAX),
                    );
                }
                outcome
            }
            ScribePublicationOutcome::UnknownCommitOutcome(error)
            | ScribePublicationOutcome::KnownNotCommitted(error) => {
                if let Some(file_list) = file_list {
                    file_list.settle(
                        crate::scribe::telemetry::ScribeEffectOutcome::Failed,
                        crate::scribe::telemetry::ScribeEffectReason::Retained,
                        u64::try_from(rows.len()).unwrap_or(u64::MAX),
                    );
                }
                return Err(error);
            }
        };
        let object_identities: Vec<String> = rows.iter().map(|row| row.file_path.clone()).collect();
        self.advance_published(&request, &object_identities)?;
        let cleanup = self.telemetry.as_ref().map(|telemetry| {
            telemetry.start_effect(
                crate::scribe::telemetry::ScribeEffect::CleanupStarted,
                crate::scribe::telemetry::ScribeEffect::CleanupSettled,
                crate::scribe::telemetry::ScribeEffectFacts {
                    bytes: request.claim.encoded_bytes(),
                    artifacts: u64::try_from(request.claim.members().len()).unwrap_or(u64::MAX),
                    outcome: crate::scribe::telemetry::ScribeEffectOutcome::Started,
                    reason: crate::scribe::telemetry::ScribeEffectReason::Published,
                    ..crate::scribe::telemetry::ScribeEffectFacts::default()
                },
            )
        });
        if let Err(error) = self
            .retire_members(&request, &outcome.commit_key, &object_identities)
            .await
        {
            if let Some(cleanup) = cleanup {
                cleanup.settle(
                    crate::scribe::telemetry::ScribeEffectOutcome::Failed,
                    crate::scribe::telemetry::ScribeEffectReason::Retained,
                    0,
                );
            }
            return Err(error);
        }
        if let Some(cleanup) = cleanup {
            cleanup.settle(
                crate::scribe::telemetry::ScribeEffectOutcome::Success,
                crate::scribe::telemetry::ScribeEffectReason::Retired,
                u64::try_from(request.claim.members().len()).unwrap_or(u64::MAX),
            );
        }
        self.mover.cleanup_published(&claims).await?;
        Ok(PublishedClaim {
            commit_key: outcome.commit_key,
            object_identities,
            released_bytes: request.claim.encoded_bytes(),
        })
    }

    /// Moves every member of one claim to the same next durable state.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a member's record cannot be moved
    /// forward, which leaves the claim resumable rather than half published.
    async fn move_members(
        &self,
        claim: &StagingClaim,
        next: StagedMemberState,
    ) -> Result<(), ScribeError> {
        for member in claim.members() {
            self.stage
                .transition(claim.key(), member.id(), next.clone())
                .await
                .map_err(|error| ScribeError::Internal {
                    detail: format!(
                        "move staged member {}-{} to {}: {error}",
                        member.id().shard(),
                        member.id().generation(),
                        next.label()
                    ),
                })?;
        }
        Ok(())
    }

    /// Records the commit on every member, then deletes their local files.
    ///
    /// The published state is written before anything is deleted so a crash
    /// between the two leaves members that name the object serving their rows,
    /// rather than rows with no authority at all.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a member cannot record the commit
    /// or its directory cannot be removed.
    async fn retire_members(
        &self,
        request: &PublishClaimRequest<'_>,
        commit_key: &FileListCommitKey,
        object_identities: &[String],
    ) -> Result<(), ScribeError> {
        let key = request.claim.key();
        for member in request.claim.members() {
            let staged = self.stage.member(key, member.id()).await.map_err(|error| {
                ScribeError::Internal {
                    detail: format!(
                        "reload staged member {}-{} before publishing it: {error}",
                        member.id().shard(),
                        member.id().generation()
                    ),
                }
            })?;
            let file_list_commit_key = format!(
                "{}:{}:{}",
                commit_key.node_id, commit_key.wal_lsn_min, commit_key.wal_lsn_max
            );
            let persisted_lsn_ranges = vec![staged.record().wal_range()];
            let published = StagedMemberState::Published {
                claim_id: request.claim.id().to_string(),
                file_list_commit_key: file_list_commit_key.clone(),
                published_object_identities: object_identities.to_vec(),
                persisted_lsn_ranges: persisted_lsn_ranges.clone(),
            };
            self.stage
                .transition(key, member.id(), published)
                .await
                .map_err(transition_failure(member.id()))?;
            self.stage
                .transition(
                    key,
                    member.id(),
                    StagedMemberState::CleanupPending {
                        claim_id: request.claim.id().to_string(),
                        file_list_commit_key,
                        published_object_identities: object_identities.to_vec(),
                        persisted_lsn_ranges,
                    },
                )
                .await
                .map_err(transition_failure(member.id()))?;
            self.await_lease_drain(key, member.id()).await?;
            self.stage
                .retire(key, member.id())
                .await
                .map_err(|error| ScribeError::Internal {
                    detail: format!(
                        "retire published staged member {}-{}: {error}",
                        member.id().shard(),
                        member.id().generation()
                    ),
                })?;
            self.release_authority(key, member.id())?;
        }
        Ok(())
    }

    /// Drives recovered published members through terminal cleanup.
    ///
    /// `Published` records first persist the complete `CleanupPending` facts;
    /// already-pending records continue idempotently. In both cases the owner
    /// prevents new leases through the restored published authority, drains
    /// existing leases, removes member storage, and releases registry authority.
    ///
    /// # Errors
    ///
    /// Returns the first durable transition, lease, filesystem, or registry
    /// refusal. Unprocessed records remain intact for the next startup.
    pub(crate) async fn recover_terminal_members(
        &self,
        key: &crate::scribe::assembly::ScribeAssemblyKey,
        members: &[crate::scribe::hot_stage::StagedMember],
    ) -> Result<RecoveredTerminalCleanup, ScribeError> {
        let terminal_facts = members
            .iter()
            .filter_map(|member| match member.record().state() {
                StagedMemberState::Published {
                    claim_id,
                    file_list_commit_key,
                    published_object_identities,
                    persisted_lsn_ranges,
                }
                | StagedMemberState::CleanupPending {
                    claim_id,
                    file_list_commit_key,
                    published_object_identities,
                    persisted_lsn_ranges,
                } => Some((
                    claim_id.clone(),
                    file_list_commit_key.clone(),
                    published_object_identities.clone(),
                    persisted_lsn_ranges.clone(),
                )),
                StagedMemberState::Ready
                | StagedMemberState::Claimed { .. }
                | StagedMemberState::Publishing { .. } => None,
            })
            .collect::<Vec<_>>();
        let mut released_bytes = 0_u64;
        let terminal_claims = terminal_facts
            .iter()
            .map(|(claim_id, ..)| claim_id.clone())
            .collect::<std::collections::HashSet<_>>();
        for member in members {
            let id = member.record().member();
            let (
                claim_id,
                file_list_commit_key,
                published_object_identities,
                persisted_lsn_ranges,
                needs_published_transition,
            ) = match member.record().state() {
                StagedMemberState::Published {
                    claim_id,
                    file_list_commit_key,
                    published_object_identities,
                    persisted_lsn_ranges,
                }
                | StagedMemberState::CleanupPending {
                    claim_id,
                    file_list_commit_key,
                    published_object_identities,
                    persisted_lsn_ranges,
                } => (
                    claim_id.clone(),
                    file_list_commit_key.clone(),
                    published_object_identities.clone(),
                    persisted_lsn_ranges.clone(),
                    matches!(member.record().state(), StagedMemberState::Published { .. }),
                ),
                StagedMemberState::Publishing { claim_id, .. } => {
                    let Some((_, commit_key, objects, _)) = terminal_facts
                        .iter()
                        .find(|(terminal_claim, ..)| terminal_claim == claim_id)
                    else {
                        continue;
                    };
                    (
                        claim_id.clone(),
                        commit_key.clone(),
                        objects.clone(),
                        vec![member.record().wal_range()],
                        true,
                    )
                }
                StagedMemberState::Ready | StagedMemberState::Claimed { .. } => continue,
            };
            if needs_published_transition
                && matches!(
                    member.record().state(),
                    StagedMemberState::Publishing { .. }
                )
            {
                self.stage
                    .transition(
                        key,
                        id,
                        StagedMemberState::Published {
                            claim_id: claim_id.clone(),
                            file_list_commit_key: file_list_commit_key.clone(),
                            published_object_identities: published_object_identities.clone(),
                            persisted_lsn_ranges: persisted_lsn_ranges.clone(),
                        },
                    )
                    .await
                    .map_err(transition_failure(id))?;
            }
            if needs_published_transition {
                self.stage
                    .transition(
                        key,
                        id,
                        StagedMemberState::CleanupPending {
                            claim_id,
                            file_list_commit_key,
                            published_object_identities,
                            persisted_lsn_ranges,
                        },
                    )
                    .await
                    .map_err(transition_failure(id))?;
            }
            self.await_lease_drain(key, id).await?;
            self.stage
                .retire(key, id)
                .await
                .map_err(|error| ScribeError::Internal {
                    detail: format!(
                        "retire recovered published staged member {}-{}: {error}",
                        id.shard(),
                        id.generation()
                    ),
                })?;
            self.release_authority(key, id)?;
            released_bytes = released_bytes
                .checked_add(member.record().encoded_bytes())
                .ok_or_else(|| ScribeError::Internal {
                    detail: "recovered published staged-byte total overflow".to_owned(),
                })?;
        }
        Ok(RecoveredTerminalCleanup {
            released_bytes,
            terminal_claims,
        })
    }
}

/// Result of driving all surviving members of terminal claims to retirement.
pub(crate) struct RecoveredTerminalCleanup {
    /// Exact surviving staged bytes removed and eligible for release.
    pub(crate) released_bytes: u64,
    /// Scribe claim identities proven terminal by their durable member state.
    pub(crate) terminal_claims: std::collections::HashSet<String>,
}

impl std::fmt::Debug for ClaimPublisher {
    /// Names the publisher without exposing its pooled or object-store owners.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ClaimPublisher")
            .field("stage", &self.stage.root())
            .finish_non_exhaustive()
    }
}

/// Builds the failure describing one member's refused durable transition.
fn transition_failure(
    member: crate::scribe::assembly::StagedMemberId,
) -> impl Fn(crate::scribe::hot_stage::HotStageError) -> ScribeError {
    move |error| ScribeError::Internal {
        detail: format!(
            "record the publication of staged member {}-{}: {error}",
            member.shard(),
            member.generation()
        ),
    }
}

/// Checks every publication row's promotion evidence against the live object.
///
/// The verifying upload re-stats and re-digests each object after writing it,
/// so this compares the record Scribe is about to make durable with what a
/// reader would actually find. Refusing here — before the local publication
/// manifest and the fenced transaction — leaves the members staged and the WAL
/// authoritative, which is the same position a failed upload leaves them in.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the counts differ, a row has no
/// verified object, or a record contradicts its object's key, size, or digest.
fn validate_promotion_records(
    rows: &[file_list_writer::FileListArtifactInsert],
    verified: &[VerifiedParquetObject],
) -> Result<(), ScribeError> {
    if rows.len() != verified.len() {
        return Err(ScribeError::Internal {
            detail: "publication rows and verified objects disagree in count".to_owned(),
        });
    }
    for row in rows {
        let object = verified
            .iter()
            .find(|object| object.identity.as_str() == row.file_path)
            .ok_or_else(|| ScribeError::Internal {
                detail: format!("publication row {} has no verified object", row.file_path),
            })?;
        row.promotion_record
            .validate_object(&crate::scribe::promotion::ObservedHotObject {
                object_key: object.identity.as_str(),
                file_size: object.length,
                file_checksum: &hex::encode(object.sha256),
            })?;
    }
    Ok(())
}
