//! Claim publication — making one assembled claim's objects authoritative.
//!
//! Assembly produces sealed local objects; this module is what turns them into
//! the rows Oracle reads. The order is the only order that keeps both durable
//! boundaries: the objects are staged and uploaded first, then the fenced
//! `file_list` transaction commits, and only after that commit do the
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
    /// Runs gathered for the claim, carrying its WAL span.
    pub runs: &'a ClaimRuns,
    /// Sealed objects the claim produced, in publication order.
    pub assembled: &'a AssembledClaim,
    /// Tenant-qualified physical binding the rows are published under.
    pub binding: &'a TenantTableBinding,
    /// Deterministic object prefix the claim's objects were sealed under.
    pub object_base: &'a str,
    /// Fenced writer identity of the pod authorizing the transaction.
    ///
    /// This is the publishing pod, which is not always the pod that produced
    /// the rows: a replacement pod publishes members it replayed under an
    /// earlier epoch.
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
    /// Why the commit's response was lost, when the rows reconciled as
    /// committed anyway.
    ///
    /// The publication is complete and its members are retired; only the
    /// caller's knowledge of it is uncertain, so the flush that requested it
    /// still fails with this detail after every durable transition has
    /// settled. It is carried as the detail rather than the error so a
    /// published claim stays cloneable.
    pub unacknowledged: Option<String>,
}

/// Publishes assembled claims and retires the members they replace.
pub struct ClaimPublisher {
    /// Durable staged namespace holding the members being replaced.
    stage: Arc<ScribeHotStage>,
    /// Local election, publication manifest, and verified upload owner.
    mover: ScribeStageMover,
    /// Fenced `file_list` transaction owner.
    reconciler: ScribePublicationReconciler,
    /// Pod authority registry whose leases cleanup waits on, when one is owned.
    hot_sources: Option<Arc<crate::scribe::hot_source::ScribeHotSourceRegistry>>,
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
        }
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
        let object_keys = if object_identities.is_empty() {
            vec![request.object_base.to_owned()]
        } else {
            object_identities.to_vec()
        };
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
                        object_keys: object_keys.clone(),
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
    /// An uncertain commit is settled here rather than deferred. The identical
    /// fenced full-set transaction is replay-exact, so it is run once more to
    /// learn whether the rows landed. If they did, the claim's members advance
    /// and retire exactly as a certain commit would, because a durable object
    /// serving rows whose generations are still live in memory would let one
    /// reader see those rows from both authorities. The ambiguity is reported
    /// back through [`PublishedClaim::unacknowledged`] rather than as an error
    /// here, so the claim leaves every index and volume exactly as a certain
    /// commit leaves it and the flush still fails for its caller.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the rows cannot be built, staging
    /// or upload fails, the fenced transaction is refused, the commit remains
    /// uncertain after reconciliation, or a member cannot be moved forward
    /// through its durable lifecycle. An uncertain commit that reconciles as
    /// committed is not an error here; it is reported through the returned
    /// claim.
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
        let (claims, verified) = self
            .mover
            .stage_and_upload_candidate(
                request.object_base,
                &request.assembled.artifacts,
                &mut chunk,
            )
            .await?;
        validate_promotion_records(&rows, &verified)?;
        // The manifest records the stream that produced the rows, not the pod
        // writing it. They are the same stream for a publication this pod also
        // ingested, and they differ for members a replacement pod restored from
        // the WAL: those rows keep the epoch that acknowledged them, which is
        // what replay correlation reads them back by. The current pod's fence
        // still authorizes the transaction below.
        self.mover
            .persist_publication(
                request.object_base,
                StreamIdentity::new(
                    request.claim.key().node_id(),
                    request.claim.key().writer_epoch(),
                ),
                &rows,
                &claims,
            )
            .await?;
        let (outcome, ambiguity) = match self.reconciler.publish(&rows).await {
            ScribePublicationOutcome::Committed(outcome) => (outcome, None),
            ScribePublicationOutcome::KnownNotCommitted(error) => return Err(error),
            ScribePublicationOutcome::UnknownCommitOutcome(error) => {
                // The transaction may already hold these rows. Leaving that
                // undecided is the one outcome this pod cannot carry: a
                // committed object would serve the claim's rows while the
                // generations behind it stayed live in memory, and a reader
                // pinning both authorities would see every row twice. The
                // fenced full-set transaction is replay-exact on the identical
                // rows, so running it once more settles the fact rather than
                // guessing it. The caller still learns the publication was
                // uncertain; only the pod's own state stops being uncertain.
                match self.reconciler.publish(&rows).await {
                    ScribePublicationOutcome::Committed(outcome) => {
                        (outcome, Some(error.to_string()))
                    }
                    ScribePublicationOutcome::UnknownCommitOutcome(_)
                    | ScribePublicationOutcome::KnownNotCommitted(_) => return Err(error),
                }
            }
        };
        let object_identities: Vec<String> = rows.iter().map(|row| row.file_path.clone()).collect();
        self.advance_published(&request, &object_identities)?;
        self.retire_members(&request, &outcome.commit_key, &object_identities)
            .await?;
        self.mover.cleanup_published(&claims).await?;
        Ok(PublishedClaim {
            commit_key: outcome.commit_key,
            object_identities,
            released_bytes: request.claim.encoded_bytes(),
            unacknowledged: ambiguity,
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
        let terminal_facts = TerminalPublicationFacts::collect(members);
        let mut released_bytes = 0_u64;
        for member in members {
            let Some(plan) = terminal_facts.plan_for(member) else {
                continue;
            };
            self.retire_terminal_member(key, member.record().member(), plan)
                .await?;
            released_bytes = released_bytes
                .checked_add(member.record().encoded_bytes())
                .ok_or_else(|| ScribeError::Internal {
                    detail: "recovered published staged-byte total overflow".to_owned(),
                })?;
        }
        Ok(RecoveredTerminalCleanup {
            released_bytes,
            terminal_claims: terminal_facts.claim_ids(),
        })
    }

    /// Drives one recovered member to the end of its terminal cleanup.
    ///
    /// A member whose durable record does not yet name the committed
    /// publication is first advanced through `Published` so the facts survive a
    /// crash inside this sweep; a member already in `CleanupPending` skips
    /// straight to draining, deletion, and authority release.
    ///
    /// # Errors
    ///
    /// Returns the first durable transition, lease-drain, filesystem, or
    /// registry refusal. The member's record is left wherever that refusal
    /// found it, which the next startup re-reads.
    async fn retire_terminal_member(
        &self,
        key: &crate::scribe::assembly::ScribeAssemblyKey,
        id: crate::scribe::assembly::StagedMemberId,
        plan: TerminalMemberPlan,
    ) -> Result<(), ScribeError> {
        if plan.records_publication {
            self.stage
                .transition(
                    key,
                    id,
                    StagedMemberState::Published {
                        claim_id: plan.facts.claim_id.clone(),
                        file_list_commit_key: plan.facts.file_list_commit_key.clone(),
                        published_object_identities: plan.facts.published_object_identities.clone(),
                        persisted_lsn_ranges: plan.persisted_lsn_ranges.clone(),
                    },
                )
                .await
                .map_err(transition_failure(id))?;
        }
        if plan.needs_cleanup_transition {
            self.stage
                .transition(
                    key,
                    id,
                    StagedMemberState::CleanupPending {
                        claim_id: plan.facts.claim_id,
                        file_list_commit_key: plan.facts.file_list_commit_key,
                        published_object_identities: plan.facts.published_object_identities,
                        persisted_lsn_ranges: plan.persisted_lsn_ranges,
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
                    id.generation(),
                ),
            })?;
        self.release_authority(key, id)
    }
}

/// Publication facts recovered from the members whose claims already committed.
///
/// A claim commits for all of its members at once, so a member left in
/// `Publishing` by a crash is terminal exactly when one of its siblings carries
/// the committed facts. Collecting those facts once is what lets the sweep
/// decide each member's disposition without re-scanning the cohort.
struct TerminalPublicationFacts {
    /// Committed facts, keyed in arrival order by their owning claim.
    facts: Vec<PublishedFacts>,
}

/// The committed publication facts one terminal claim carries.
#[derive(Clone)]
struct PublishedFacts {
    /// Deterministic identity of the claim that committed the rows.
    claim_id: String,
    /// Commit key of the fenced `file_list` transaction.
    file_list_commit_key: String,
    /// Object identities the claim published, in artifact-ordinal order.
    published_object_identities: Vec<String>,
    /// WAL ranges the publishing member itself recorded, when it had them.
    persisted_lsn_ranges: Vec<crate::scribe::hot_stage::StagedLsnRange>,
}

/// One member's disposition inside a terminal-cleanup sweep.
struct TerminalMemberPlan {
    /// Committed facts the member's record must end up carrying.
    facts: PublishedFacts,
    /// WAL ranges to record, which for a resumed member are its own.
    persisted_lsn_ranges: Vec<crate::scribe::hot_stage::StagedLsnRange>,
    /// Whether the record still has to name its committed publication.
    records_publication: bool,
    /// Whether the record still has to be advanced to `CleanupPending`.
    needs_cleanup_transition: bool,
}

impl TerminalPublicationFacts {
    /// Collects the committed facts every terminal claim in the cohort carries.
    fn collect(members: &[crate::scribe::hot_stage::StagedMember]) -> Self {
        let facts = members
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
                } => Some(PublishedFacts {
                    claim_id: claim_id.clone(),
                    file_list_commit_key: file_list_commit_key.clone(),
                    published_object_identities: published_object_identities.clone(),
                    persisted_lsn_ranges: persisted_lsn_ranges.clone(),
                }),
                StagedMemberState::Ready
                | StagedMemberState::Claimed { .. }
                | StagedMemberState::Publishing { .. } => None,
            })
            .collect();
        Self { facts }
    }

    /// Returns the claim identities proven terminal by durable member state.
    fn claim_ids(&self) -> std::collections::HashSet<String> {
        self.facts
            .iter()
            .map(|facts| facts.claim_id.clone())
            .collect()
    }

    /// Decides what one member of the cohort still owes terminal cleanup.
    ///
    /// A `Published` member owes the `CleanupPending` transition, a
    /// `CleanupPending` member owes only the drain and deletion, and a
    /// `Publishing` member owes the full sequence under its sibling's committed
    /// facts — but only when such a sibling exists, because nothing else proves
    /// its claim committed. A member is never transitioned onto the state it is
    /// already in; the stage refuses that as a lifecycle contradiction.
    fn plan_for(
        &self,
        member: &crate::scribe::hot_stage::StagedMember,
    ) -> Option<TerminalMemberPlan> {
        match member.record().state() {
            StagedMemberState::Published { claim_id, .. } => {
                let facts = self.facts_for(claim_id)?;
                Some(TerminalMemberPlan {
                    persisted_lsn_ranges: facts.persisted_lsn_ranges.clone(),
                    facts,
                    records_publication: false,
                    needs_cleanup_transition: true,
                })
            }
            StagedMemberState::CleanupPending { claim_id, .. } => {
                let facts = self.facts_for(claim_id)?;
                Some(TerminalMemberPlan {
                    persisted_lsn_ranges: facts.persisted_lsn_ranges.clone(),
                    facts,
                    records_publication: false,
                    needs_cleanup_transition: false,
                })
            }
            StagedMemberState::Publishing { claim_id, .. } => Some(TerminalMemberPlan {
                facts: self.facts_for(claim_id)?,
                persisted_lsn_ranges: vec![member.record().wal_range()],
                records_publication: true,
                needs_cleanup_transition: true,
            }),
            StagedMemberState::Ready | StagedMemberState::Claimed { .. } => None,
        }
    }

    /// Returns the committed facts recorded for one claim identity.
    fn facts_for(&self, claim_id: &str) -> Option<PublishedFacts> {
        self.facts
            .iter()
            .find(|facts| facts.claim_id == claim_id)
            .cloned()
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
