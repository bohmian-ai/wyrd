//! The one owner that turns a managed rewrite handoff into a catalog commit.
//!
//! The module has two layers and the split is the point. Almost everything
//! here is a pure derivation over values the caller already holds; only the
//! final [`Forge`] block below performs IO, and it executes exactly what the
//! derivation decided rather than deciding anything itself. No catalog, object
//! store, SQL handle, or clock reaches the derivation:
//! the commit request, the delete disposition, the replacement sequence, the
//! snapshot properties, and the authority decision are all decided *before*
//! any IO, so a refusal here is provably free of side effects and a mistake
//! here is falsifiable by a unit test rather than by a committed table.
//!
//! The managed core returns evidence, not instructions. Its applied-delete
//! lists say which deletes its readers materialized, never which deletes the
//! catalog may drop; that safety is derived here from the immutable base
//! snapshot and the data that survives the rewrite.

use std::collections::{BTreeMap, BTreeSet};

use iceberg::spec::{
    DataContentType, DataFile, Literal, PartitionSpec, PrimitiveLiteral, Struct, Transform,
};
use iceberg::table::Table;
use iceberg::transaction::Transaction;
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;
use uuid::Uuid;

use super::Forge;
use super::error::ForgeError;
use super::lease::ForgeLease;
use super::managed::RewriteHandoff;
use super::managed::fingerprint::ForgeRewriteEvidence;
use crate::catalog::layout::{TimeGranularity, TimePartition};

/// Value of `forge.workflow` every rewrite snapshot carries.
///
/// Reconciliation reads this before it reads any other identity property, so a
/// snapshot produced by a different Forge workflow is skipped rather than
/// misread as an uncommitted rewrite.
pub(super) const REWRITE_WORKFLOW: &str = "iceberg-rewrite";

/// Version of the canonical rewrite snapshot property set.
const REWRITE_PROPERTY_VERSION: &str = "1";

/// Every property a rewrite snapshot must carry to be reconcilable.
///
/// The list is the contract: a successor that finds a snapshot decides whether
/// it is *this* operation's commit from exactly these values, so a snapshot
/// missing any one of them cannot be classified and must not be treated as
/// evidence of anything.
pub(super) const REWRITE_SNAPSHOT_PROPERTY_KEYS: &[&str] = &[
    "forge.workflow",
    "forge.group",
    "forge.operation_id",
    "forge.task_id",
    "forge.attempt_id",
    "forge.rewrite.version",
    "forge.rewrite.base_snapshot_id",
    "forge.rewrite.plan_hash",
    "forge.rewrite.selection_fingerprint",
    "forge.rewrite.debt_fingerprint",
    "forge.rewrite.policy_fingerprint",
    "forge.rewrite.removed_data_files",
    "forge.rewrite.removed_delete_files",
    "forge.rewrite.retained_delete_files",
    "forge.rewrite.added_data_files",
    "forge.rewrite.removed_bytes",
    "forge.rewrite.added_bytes",
];

/// One live file of the immutable base snapshot and the sequence it carries.
///
/// The data sequence number lives on the manifest entry rather than the
/// descriptor, and it is the only thing that decides whether an equality
/// delete can still reach a file, so it is carried alongside rather than
/// recomputed.
#[derive(Debug, Clone)]
pub(super) struct RewriteBaseFile {
    /// Descriptor exactly as the base manifest carries it.
    pub(super) file: DataFile,
    /// Data sequence number inherited from the manifest entry.
    pub(super) sequence_number: i64,
}

/// The complete live view of the one snapshot a rewrite was planned against.
///
/// Publication derives every removal decision from this value and nothing
/// else. It is assembled once, from the base snapshot's own manifests, so a
/// table that moved after planning cannot silently widen or narrow what the
/// commit removes: the mismatch is caught by the authority decision instead.
#[derive(Debug, Clone)]
pub(super) struct RewriteBase {
    /// Snapshot the plan and every consumed path were resolved against.
    snapshot_id: i64,
    /// Sequence number of that snapshot, assigned to every replacement row.
    sequence_number: i64,
    /// Default partition spec every published output must be written under.
    partition_spec_id: i32,
    /// Live data files keyed by path.
    data: BTreeMap<String, RewriteBaseFile>,
    /// Live position-delete files keyed by path.
    position_deletes: BTreeMap<String, RewriteBaseFile>,
    /// Live equality-delete files keyed by path.
    equality_deletes: BTreeMap<String, RewriteBaseFile>,
}

impl RewriteBase {
    /// Assembles one base view after proving its entries are consistent.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the snapshot identity or its
    /// sequence number is not positive, when the same path appears twice, or
    /// when an entry carries a negative data sequence number.
    pub(super) fn try_new(
        snapshot_id: i64,
        sequence_number: i64,
        partition_spec_id: i32,
        entries: Vec<(DataFile, i64)>,
    ) -> Result<Self, ForgeError> {
        let invariant = |detail: String| ForgeError::Invariant { detail };
        if snapshot_id <= 0 || sequence_number < 0 || partition_spec_id < 0 {
            return Err(invariant(format!(
                "rewrite base snapshot {snapshot_id} carries an unusable identity"
            )));
        }
        let mut base = Self {
            snapshot_id,
            sequence_number,
            partition_spec_id,
            data: BTreeMap::new(),
            position_deletes: BTreeMap::new(),
            equality_deletes: BTreeMap::new(),
        };
        let mut seen = BTreeSet::new();
        for (file, entry_sequence) in entries {
            if entry_sequence < 0 {
                return Err(invariant(format!(
                    "rewrite base entry {} carries no data sequence number",
                    file.file_path()
                )));
            }
            if !seen.insert(file.file_path().to_owned()) {
                return Err(invariant(format!(
                    "rewrite base snapshot names {} more than once",
                    file.file_path()
                )));
            }
            let path = file.file_path().to_owned();
            let target = match file.content_type() {
                DataContentType::Data => &mut base.data,
                DataContentType::PositionDeletes => &mut base.position_deletes,
                DataContentType::EqualityDeletes => &mut base.equality_deletes,
            };
            target.insert(
                path,
                RewriteBaseFile {
                    file,
                    sequence_number: entry_sequence,
                },
            );
        }
        Ok(base)
    }

    /// Returns whether any live file of the base carries `path`.
    fn contains(&self, path: &str) -> bool {
        self.data.contains_key(path)
            || self.position_deletes.contains_key(path)
            || self.equality_deletes.contains_key(path)
    }
}

/// Whether one delete at `delete_sequence` still reaches replacement rows.
///
/// This is the whole of Iceberg's sequence rule for equality deletes, written
/// once so publication and its proof read the same predicate: a delete applies
/// only to data that is strictly older than it. Publishing replacements at the
/// base snapshot's sequence therefore excludes every delete the rewrite
/// already applied — each was live in the base, so its sequence is at or below
/// the base's — while still admitting a delete that arrived afterwards.
pub(super) const fn delete_reaches_replacement(
    delete_sequence: i64,
    replacement_sequence: i64,
) -> bool {
    replacement_sequence < delete_sequence
}

/// The exact delete files one commit may remove, and the ones it must keep.
///
/// Separating them is the point: an applied delete is *evidence* that the
/// rewrite materialized its effect into the outputs, never permission to drop
/// it, because the same file can also apply to data this rewrite did not
/// touch. Dropping such a file would resurrect rows.
#[derive(Debug, Default)]
pub(super) struct RewriteDeleteDisposition {
    /// Delete descriptors whose whole scope was rewritten.
    pub(super) removable: Vec<DataFile>,
    /// Paths of applied deletes retained because live data still needs them.
    pub(super) retained: Vec<String>,
}

impl RewriteDeleteDisposition {
    /// Derives the removable and retained delete sets from the base snapshot.
    ///
    /// Position deletes are decided by reference: a delete file that names the
    /// single data file it covers is removable exactly when that file was
    /// rewritten, and one that names nothing is never provably scoped, so it
    /// is retained. Equality deletes are decided by scope: the file remains
    /// necessary while any *surviving* live data file shares its partition and
    /// spec at a lower data sequence number, because those are the rows it
    /// would still have to delete.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Reconciliation`] when an applied delete path is
    /// not a live delete of the base snapshot, which means the evidence and
    /// the base disagree about what the rewrite read.
    pub(super) fn derive(
        base: &RewriteBase,
        rewritten_data_files: &[String],
        applied_position_delete_files: &[String],
        applied_equality_delete_files: &[String],
    ) -> Result<Self, ForgeError> {
        let rewritten: BTreeSet<&str> = rewritten_data_files.iter().map(String::as_str).collect();
        let surviving: Vec<&RewriteBaseFile> = base
            .data
            .iter()
            .filter(|(path, _)| !rewritten.contains(path.as_str()))
            .map(|(_, file)| file)
            .collect();
        let mut disposition = Self::default();
        for path in applied_position_delete_files {
            let entry =
                base.position_deletes
                    .get(path)
                    .ok_or_else(|| ForgeError::Reconciliation {
                        detail: format!(
                            "applied position delete {path} is not live in the base snapshot"
                        ),
                    })?;
            let removable = entry
                .file
                .referenced_data_file()
                .is_some_and(|target| rewritten.contains(target.as_str()));
            if removable {
                disposition.removable.push(entry.file.clone());
            } else {
                disposition.retained.push(path.clone());
            }
        }
        for path in applied_equality_delete_files {
            let entry =
                base.equality_deletes
                    .get(path)
                    .ok_or_else(|| ForgeError::Reconciliation {
                        detail: format!(
                            "applied equality delete {path} is not live in the base snapshot"
                        ),
                    })?;
            let still_applies = surviving.iter().any(|candidate| {
                candidate.file.partition_spec_id() == entry.file.partition_spec_id()
                    && candidate.file.partition() == entry.file.partition()
                    && delete_reaches_replacement(entry.sequence_number, candidate.sequence_number)
            });
            if still_applies {
                disposition.retained.push(path.clone());
            } else {
                disposition.removable.push(entry.file.clone());
            }
        }
        Ok(disposition)
    }
}

/// Every identity one rewrite publication is bound to.
///
/// The values travel together because a snapshot is only reconcilable when it
/// carries all of them: the operation identity is what a successor matches on,
/// the task identity is what recovery searches retained metadata for, and the
/// evidence fingerprints are what prove the commit describes the selection the
/// durable task authorized rather than some later one.
#[derive(Debug, Clone)]
pub(super) struct RewriteCommitIdentity {
    /// Durable task that owns this publication.
    pub(super) task_id: Uuid,
    /// Attempt whose outputs are being published.
    pub(super) attempt_id: Uuid,
    /// Operation identity every phase of this rewrite shares.
    pub(super) operation_id: Uuid,
    /// Canonical tenant/table audit resource.
    pub(super) group: String,
    /// Canonical hash of the durable plan payload.
    pub(super) plan_hash: [u8; 32],
    /// Selection, debt, and policy fingerprints the attempt produced.
    pub(super) evidence: ForgeRewriteEvidence,
}

/// Everything the pure commit derivation reads, borrowed from its owner.
#[derive(Debug, Clone, Copy)]
pub(super) struct RewriteCommitInputs<'a> {
    /// Immutable five-field result the managed core produced.
    pub(super) handoff: &'a RewriteHandoff,
    /// Live view of the snapshot the rewrite was planned against.
    pub(super) base: &'a RewriteBase,
    /// Exact input set the durable task persisted.
    pub(super) selected_inputs: &'a [String],
    /// Identity the published snapshot is bound to.
    pub(super) identity: &'a RewriteCommitIdentity,
}

/// The exact catalog effect one rewrite publication performs.
///
/// Constructed only by [`RewriteCommitRequest::derive`], which is the single
/// place the removal set, the addition set, the delete disposition, and the
/// replacement sequence are decided. A `RewriteCommitRequest` that exists is a
/// publication whose every file identity was cross-checked against the base.
#[derive(Debug)]
pub(super) struct RewriteCommitRequest {
    /// Snapshot the request is valid against.
    pub(super) base_snapshot_id: i64,
    /// Live data descriptors this commit removes, in path order.
    pub(super) removed_data_files: Vec<DataFile>,
    /// Live delete descriptors whose whole scope was rewritten.
    pub(super) removed_delete_files: Vec<DataFile>,
    /// Core-produced output descriptors, carried through unchanged.
    pub(super) added_data_files: Vec<DataFile>,
    /// Applied delete paths retained because live data still needs them.
    pub(super) retained_delete_files: Vec<String>,
    /// Sequence assigned to every replacement row.
    pub(super) new_data_file_sequence_number: i64,
    /// Identity the published snapshot is bound to.
    identity: RewriteCommitIdentity,
}

impl RewriteCommitRequest {
    /// Cross-checks one handoff against the immutable base and its own task.
    ///
    /// The order is deliberate. Base identity is proven first because every
    /// later check reads paths out of that base; then the durable selection,
    /// because removing a file the task never authorized is a publication this
    /// task has no right to make — the selection is a ceiling, since the
    /// managed core packs what fits its admitted envelope and may honestly
    /// return less, but never something outside it; then input existence, which
    /// is what makes each removal a removal of something live; then the delete
    /// disposition, which decides safety rather than accepting the core's
    /// applied lists as instructions; and finally the outputs, which must be
    /// the core's own descriptors written under the base's own partition spec
    /// and must not collide with a file the base already carries.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Reconciliation`] when the handoff names a base,
    /// selection, input, or applied delete the base snapshot does not support,
    /// and [`ForgeError::Invariant`] when the durable selection is itself
    /// duplicated or an output descriptor is not publishable.
    pub(super) fn derive(inputs: RewriteCommitInputs<'_>) -> Result<Self, ForgeError> {
        let RewriteCommitInputs {
            handoff,
            base,
            selected_inputs,
            identity,
        } = inputs;
        let reconciliation = |detail: String| ForgeError::Reconciliation { detail };
        if handoff.base_snapshot_id != base.snapshot_id {
            return Err(reconciliation(format!(
                "rewrite handoff was planned against snapshot {} but publishes against {}",
                handoff.base_snapshot_id, base.snapshot_id
            )));
        }
        let selected: BTreeSet<&str> = selected_inputs.iter().map(String::as_str).collect();
        if selected.len() != selected_inputs.len() {
            return Err(ForgeError::Invariant {
                detail: "durable rewrite selection names one input more than once".to_owned(),
            });
        }
        let rewritten: BTreeSet<&str> = handoff
            .rewritten_data_files
            .iter()
            .map(String::as_str)
            .collect();
        if !rewritten.is_subset(&selected) {
            return Err(reconciliation(
                "rewrite handoff does not remove exactly what the durable task authorized"
                    .to_owned(),
            ));
        }
        let mut removed_data_files = Vec::with_capacity(rewritten.len());
        for path in &rewritten {
            let entry = base.data.get(*path).ok_or_else(|| {
                reconciliation(format!(
                    "rewrite input {path} is not a live data file of the base snapshot"
                ))
            })?;
            removed_data_files.push(entry.file.clone());
        }
        let disposition = RewriteDeleteDisposition::derive(
            base,
            &handoff.rewritten_data_files,
            &handoff.applied_position_delete_files,
            &handoff.applied_equality_delete_files,
        )?;
        let mut added_data_files = Vec::with_capacity(handoff.output_data_files.len());
        for file in &handoff.output_data_files {
            if file.content_type() != DataContentType::Data {
                return Err(ForgeError::Invariant {
                    detail: format!("rewrite output {} is not a data file", file.file_path()),
                });
            }
            if file.partition_spec_id() != base.partition_spec_id {
                return Err(ForgeError::Invariant {
                    detail: format!(
                        "rewrite output {} was written under partition spec {} but the base uses {}",
                        file.file_path(),
                        file.partition_spec_id(),
                        base.partition_spec_id
                    ),
                });
            }
            if file.record_count() == 0 || file.file_size_in_bytes() == 0 {
                return Err(ForgeError::Invariant {
                    detail: format!(
                        "rewrite output {} carries no measured content",
                        file.file_path()
                    ),
                });
            }
            if base.contains(file.file_path()) {
                return Err(reconciliation(format!(
                    "rewrite output {} is already live in the base snapshot",
                    file.file_path()
                )));
            }
            added_data_files.push(file.clone());
        }
        Ok(Self {
            base_snapshot_id: base.snapshot_id,
            removed_data_files,
            removed_delete_files: disposition.removable,
            added_data_files,
            retained_delete_files: disposition.retained,
            new_data_file_sequence_number: base.sequence_number,
            identity: identity.clone(),
        })
    }

    /// Builds the canonical, versioned snapshot property set for this commit.
    ///
    /// The set is the only evidence a successor has when it finds a snapshot
    /// and must decide whether that snapshot is this operation's commit, so it
    /// binds three independent things at once: the identity a match is made on,
    /// the fingerprints that prove which selection and policy produced it, and
    /// the exact live-set delta, which is what makes a partially applied or
    /// re-derived commit detectable rather than plausible.
    pub(super) fn snapshot_properties(&self) -> BTreeMap<String, String> {
        let removed_bytes: u64 = self
            .removed_data_files
            .iter()
            .map(DataFile::file_size_in_bytes)
            .sum();
        let added_bytes: u64 = self
            .added_data_files
            .iter()
            .map(DataFile::file_size_in_bytes)
            .sum();
        BTreeMap::from([
            ("forge.workflow".to_owned(), REWRITE_WORKFLOW.to_owned()),
            ("forge.group".to_owned(), self.identity.group.clone()),
            (
                "forge.operation_id".to_owned(),
                self.identity.operation_id.to_string(),
            ),
            (
                "forge.task_id".to_owned(),
                self.identity.task_id.to_string(),
            ),
            (
                "forge.attempt_id".to_owned(),
                self.identity.attempt_id.to_string(),
            ),
            (
                "forge.rewrite.version".to_owned(),
                REWRITE_PROPERTY_VERSION.to_owned(),
            ),
            (
                "forge.rewrite.base_snapshot_id".to_owned(),
                self.base_snapshot_id.to_string(),
            ),
            (
                "forge.rewrite.plan_hash".to_owned(),
                hex::encode(self.identity.plan_hash),
            ),
            (
                "forge.rewrite.selection_fingerprint".to_owned(),
                self.identity.evidence.selection_fingerprint.clone(),
            ),
            (
                "forge.rewrite.debt_fingerprint".to_owned(),
                self.identity.evidence.debt_fingerprint.clone(),
            ),
            (
                "forge.rewrite.policy_fingerprint".to_owned(),
                self.identity.evidence.policy_fingerprint.clone(),
            ),
            (
                "forge.rewrite.removed_data_files".to_owned(),
                self.removed_data_files.len().to_string(),
            ),
            (
                "forge.rewrite.removed_delete_files".to_owned(),
                self.removed_delete_files.len().to_string(),
            ),
            (
                "forge.rewrite.retained_delete_files".to_owned(),
                self.retained_delete_files.len().to_string(),
            ),
            (
                "forge.rewrite.added_data_files".to_owned(),
                self.added_data_files.len().to_string(),
            ),
            (
                "forge.rewrite.removed_bytes".to_owned(),
                removed_bytes.to_string(),
            ),
            (
                "forge.rewrite.added_bytes".to_owned(),
                added_bytes.to_string(),
            ),
        ])
    }

    /// Encodes this request as one replacement transaction against `table`.
    ///
    /// The encoding is pure: it names exactly the removals, additions, sequence
    /// number, and lineage properties the request already validated, so the
    /// only thing the caller adds is the decision to submit it. The delete
    /// filter manager is disabled because the request already resolved delete
    /// disposition itself and a second, independent resolution could silently
    /// disagree with the retained set this rewrite committed to.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Catalog`] when the transaction layer refuses to
    /// encode the replacement. That refusal happens before any catalog call, so
    /// the caller may treat it as definitely unsubmitted.
    pub(super) fn encode(&self, table: &Table) -> Result<Transaction, ForgeError> {
        let removed = self
            .removed_data_files
            .iter()
            .cloned()
            .chain(self.removed_delete_files.iter().cloned())
            .collect::<Vec<_>>();
        let transaction = Transaction::new(table);
        let mut action = transaction
            .rewrite_files()
            .set_enable_delete_filter_manager(false)
            .set_new_data_file_sequence_number(self.new_data_file_sequence_number)
            .add_data_files(self.added_data_files.iter().cloned())
            .delete_files(removed);
        action.set_snapshot_properties(self.snapshot_properties().into_iter().collect());
        iceberg::transaction::ApplyTransactionAction::apply(action, transaction)
            .map_err(ForgeError::Catalog)
    }

    /// Confirms the committed snapshot carries exactly this request's lineage.
    ///
    /// The catalog is free to normalize a snapshot summary, and every later
    /// recovery reads this rewrite's lineage back out of one. Proving the
    /// properties still parse into the same values while the committing worker
    /// is still here turns a silently unrecoverable snapshot into a loud
    /// failure on the one attempt that can still explain it. The replacement is
    /// already live at this point, so every mismatch is reported as
    /// [`RewriteSubmission::AcceptanceUnknown`]: the operation stays open for
    /// the evidence-based recovery that can still find the snapshot, never
    /// Reset.
    ///
    /// # Errors
    ///
    /// Returns the validation error only when this request's own submitted
    /// properties do not parse, which is an internal invariant rather than a
    /// catalog outcome.
    pub(super) fn confirm_committed_lineage(
        &self,
        committed: Table,
    ) -> Result<RewriteSubmission, ForgeError> {
        let Some(recorded) = committed.metadata().current_snapshot().map(|snapshot| {
            snapshot
                .summary()
                .additional_properties
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<BTreeMap<_, _>>()
        }) else {
            return Ok(RewriteSubmission::AcceptanceUnknown(
                ForgeError::Reconciliation {
                    detail: "committed rewrite produced no current snapshot".to_owned(),
                },
            ));
        };
        let submitted = RewriteSnapshotProperties::validate(&self.snapshot_properties())?;
        Ok(match RewriteSnapshotProperties::validate(&recorded) {
            Ok(recorded) if recorded == submitted => {
                RewriteSubmission::Committed(Box::new(committed))
            }
            Ok(_) => RewriteSubmission::AcceptanceUnknown(ForgeError::Reconciliation {
                detail: "committed rewrite snapshot does not carry the submitted lineage"
                    .to_owned(),
            }),
            Err(error) => RewriteSubmission::AcceptanceUnknown(error),
        })
    }
}

/// One validated rewrite snapshot property set.
///
/// Parsing rather than peeking is what makes a snapshot usable as evidence: a
/// successor that has a `RewriteSnapshotProperties` knows the snapshot names a
/// complete, well-typed rewrite identity, not merely that some Forge property
/// happened to be present. Every canonical key is parsed into a field, so a
/// property this owner writes but reconciliation would ignore cannot exist.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct RewriteSnapshotProperties {
    /// Operation identity a successor matches this snapshot on.
    pub(super) operation_id: Uuid,
    /// Durable task that produced this snapshot.
    pub(super) task_id: Uuid,
    /// Attempt whose outputs this snapshot published.
    pub(super) attempt_id: Uuid,
    /// Canonical tenant/table audit resource.
    pub(super) group: String,
    /// Snapshot the published rewrite was planned against.
    pub(super) base_snapshot_id: i64,
    /// Canonical hash of the durable plan payload, hex encoded.
    pub(super) plan_hash: String,
    /// Selection fingerprint the publishing attempt produced.
    pub(super) selection_fingerprint: String,
    /// Debt fingerprint the publishing attempt produced.
    pub(super) debt_fingerprint: String,
    /// Policy fingerprint the publishing attempt produced.
    pub(super) policy_fingerprint: String,
    /// Number of live data files the commit removed.
    pub(super) removed_data_files: u64,
    /// Number of live delete files the commit removed.
    pub(super) removed_delete_files: u64,
    /// Number of applied delete files the commit deliberately kept.
    pub(super) retained_delete_files: u64,
    /// Number of data files the commit added.
    pub(super) added_data_files: u64,
    /// Total bytes of the removed data files.
    pub(super) removed_bytes: u64,
    /// Total bytes of the added data files.
    pub(super) added_bytes: u64,
}

impl RewriteSnapshotProperties {
    /// Validates and parses one snapshot's rewrite property set.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Reconciliation`] when a required property is
    /// absent or empty, when the workflow or property version is not the one
    /// this owner writes, when an identity is not a UUID, when a count or
    /// snapshot id is not a non-negative integer, when the plan hash is not 32
    /// hex-encoded bytes, or when the commit claims to have added nothing.
    pub(super) fn validate(properties: &BTreeMap<String, String>) -> Result<Self, ForgeError> {
        let reconciliation = |detail: String| ForgeError::Reconciliation { detail };
        let read = |key: &str| -> Result<&String, ForgeError> {
            properties
                .get(key)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    reconciliation(format!("rewrite snapshot has no usable property {key}"))
                })
        };
        for key in REWRITE_SNAPSHOT_PROPERTY_KEYS {
            let _ = read(key)?;
        }
        if read("forge.workflow")? != REWRITE_WORKFLOW {
            return Err(reconciliation(
                "snapshot property set does not describe a Forge rewrite".to_owned(),
            ));
        }
        if read("forge.rewrite.version")? != REWRITE_PROPERTY_VERSION {
            return Err(reconciliation(
                "rewrite snapshot property version is not the one this owner writes".to_owned(),
            ));
        }
        let uuid = |key: &str| -> Result<Uuid, ForgeError> {
            Uuid::parse_str(read(key)?).map_err(|error| {
                reconciliation(format!(
                    "rewrite snapshot property {key} is not a UUID: {error}"
                ))
            })
        };
        let count = |key: &str| -> Result<u64, ForgeError> {
            read(key)?.parse::<u64>().map_err(|error| {
                reconciliation(format!(
                    "rewrite snapshot property {key} is not a count: {error}"
                ))
            })
        };
        let plan_hash = read("forge.rewrite.plan_hash")?.clone();
        if plan_hash.len() != 64 || !plan_hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(reconciliation(
                "rewrite snapshot plan hash is not 32 hex-encoded bytes".to_owned(),
            ));
        }
        let base_snapshot_id = read("forge.rewrite.base_snapshot_id")?
            .parse::<i64>()
            .map_err(|error| {
                reconciliation(format!("rewrite snapshot base id is not an id: {error}"))
            })?;
        if base_snapshot_id <= 0 {
            return Err(reconciliation(
                "rewrite snapshot base id is not a snapshot".to_owned(),
            ));
        }
        let added_data_files = count("forge.rewrite.added_data_files")?;
        if added_data_files == 0 {
            return Err(reconciliation(
                "a rewrite snapshot that added nothing published nothing".to_owned(),
            ));
        }
        Ok(Self {
            operation_id: uuid("forge.operation_id")?,
            task_id: uuid("forge.task_id")?,
            attempt_id: uuid("forge.attempt_id")?,
            group: read("forge.group")?.clone(),
            base_snapshot_id,
            plan_hash,
            selection_fingerprint: read("forge.rewrite.selection_fingerprint")?.clone(),
            debt_fingerprint: read("forge.rewrite.debt_fingerprint")?.clone(),
            policy_fingerprint: read("forge.rewrite.policy_fingerprint")?.clone(),
            removed_data_files: count("forge.rewrite.removed_data_files")?,
            removed_delete_files: count("forge.rewrite.removed_delete_files")?,
            retained_delete_files: count("forge.rewrite.retained_delete_files")?,
            added_data_files,
            removed_bytes: count("forge.rewrite.removed_bytes")?,
            added_bytes: count("forge.rewrite.added_bytes")?,
        })
    }
}

/// Authority the table lease itself grants at the commit boundary.
#[derive(Debug, Clone, Copy)]
pub(super) struct RewriteFenceAuthority {
    /// The table lease is still held by this worker.
    pub(super) lease_held: bool,
    /// The lease fence still covers this owner.
    pub(super) fence_held: bool,
    /// The remaining lease covers a whole commit window.
    pub(super) commit_window_fits: bool,
}

/// Authority the attempt's own liveness grants at the commit boundary.
#[derive(Debug, Clone, Copy)]
pub(super) struct RewriteAttemptAuthority {
    /// Cooperative cancellation was requested.
    pub(super) cancelled: bool,
    /// The attempt's immutable deadline has elapsed.
    pub(super) deadline_passed: bool,
}

/// Authority the table still being the planned table grants.
#[derive(Debug, Clone, Copy)]
pub(super) struct RewriteTableAuthority {
    /// The recorded planning snapshot is still retained.
    ///
    /// Retention, not currency: a rewrite is a replacement of a named file
    /// set, so a head that has moved on for an unrelated reason does not
    /// invalidate it. What would is the planning snapshot being expired out
    /// from under the plan, because then nothing can say what the plan was
    /// derived from.
    pub(super) base_is_retained: bool,
    /// The current schema identity still equals the plan's.
    ///
    /// Partition spec and sort order are deliberately not here: neither
    /// changes the meaning of the rows the outputs already carry, so neither
    /// is an independent refusal. A changed schema is, because the outputs
    /// were written against the old one.
    pub(super) schema_unchanged: bool,
}

/// Authority the planned file set still being publishable grants.
#[derive(Debug, Clone, Copy)]
pub(super) struct RewriteFileAuthority {
    /// Every selected input is still live.
    pub(super) inputs_all_live: bool,
    /// Every delete this commit removes is still out of every surviving scope.
    pub(super) delete_scope_safe: bool,
}

/// Every knowable authority a publication must hold at the commit boundary.
///
/// The leaves are deliberately booleans decided by their own owners — the
/// lease, the clock, the catalog, the policy extractor — so this type owns the
/// *order and completeness* of the check rather than duplicating any of them.
/// That is what makes the decision testable without a table. They are grouped
/// by who decides them, so a reader can see at a glance which owner a refusal
/// came from.
#[derive(Debug, Clone, Copy)]
pub(super) struct RewriteCommitAuthority {
    /// What the table lease still grants.
    pub(super) fence: RewriteFenceAuthority,
    /// What this attempt's own liveness still grants.
    pub(super) attempt: RewriteAttemptAuthority,
    /// What the table's current state still grants.
    pub(super) table: RewriteTableAuthority,
    /// What the planned file set still grants.
    pub(super) files: RewriteFileAuthority,
}

/// Why one publication was refused before it reached the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RewriteRefusal {
    /// The table lease is no longer held.
    LeaseLost,
    /// The lease fence no longer covers this owner.
    FenceLost,
    /// The remaining lease cannot cover a commit window.
    CommitWindow,
    /// Cancellation was requested before the commit.
    Cancelled,
    /// The attempt's immutable deadline elapsed.
    Deadline,
    /// The recorded planning snapshot is no longer retained.
    BaseNotRetained,
    /// The table's current schema identity changed.
    SchemaChanged,
    /// A selected input is no longer live.
    InputsChanged,
    /// A delete this commit would remove still covers surviving data.
    DeleteScopeUnsafe,
}

/// The one authority verdict a publication acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RewriteCommitDecision {
    /// Every knowable authority holds; the commit may be submitted.
    Proceed,
    /// One authority failed and no catalog call may follow.
    Refuse(RewriteRefusal),
}

impl RewriteCommitAuthority {
    /// Applies the fixed refusal order and fails closed.
    ///
    /// The order runs from cheapest and most-certain to most-derived, so the
    /// reported refusal names the outermost thing that went wrong rather than
    /// a downstream symptom of it. Every dimension is checked; there is no
    /// early `Proceed`.
    pub(super) const fn decide(&self) -> RewriteCommitDecision {
        if !self.fence.lease_held {
            return RewriteCommitDecision::Refuse(RewriteRefusal::LeaseLost);
        }
        if !self.fence.fence_held {
            return RewriteCommitDecision::Refuse(RewriteRefusal::FenceLost);
        }
        if !self.fence.commit_window_fits {
            return RewriteCommitDecision::Refuse(RewriteRefusal::CommitWindow);
        }
        if self.attempt.cancelled {
            return RewriteCommitDecision::Refuse(RewriteRefusal::Cancelled);
        }
        if self.attempt.deadline_passed {
            return RewriteCommitDecision::Refuse(RewriteRefusal::Deadline);
        }
        if !self.table.base_is_retained {
            return RewriteCommitDecision::Refuse(RewriteRefusal::BaseNotRetained);
        }
        if !self.table.schema_unchanged {
            return RewriteCommitDecision::Refuse(RewriteRefusal::SchemaChanged);
        }
        if !self.files.inputs_all_live {
            return RewriteCommitDecision::Refuse(RewriteRefusal::InputsChanged);
        }
        if !self.files.delete_scope_safe {
            return RewriteCommitDecision::Refuse(RewriteRefusal::DeleteScopeUnsafe);
        }
        RewriteCommitDecision::Proceed
    }
}

/// What the catalog told this owner about one submitted commit.
///
/// The distinction between a definite conflict and an ambiguous outcome is the
/// whole recovery protocol. A definite conflict is an *answer*: the catalog
/// refused, so the commit certainly did not land, and retrying it against
/// refreshed metadata is safe. An ambiguous outcome is the absence of an
/// answer, and resubmitting under it is how a rewrite gets published twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RewriteAcceptance {
    /// The catalog answered with a refusal, so nothing landed.
    DefiniteConflict,
    /// Acceptance is unknown: the answer was lost, timed out, or cancelled.
    Ambiguous,
}

/// The only actions a publication may take after a non-success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RewriteConflictAction {
    /// Reload metadata, revalidate every assumption, and commit once more.
    RevalidateAndRecommit,
    /// Record the operation as definitely uncommitted without new outputs.
    ResetDefinitelyUncommitted,
    /// Leave the operation open for evidence-based reconciliation.
    ReconcileWithoutRecommit,
}

/// Retries permitted after the initial submission of one plan.
const REWRITE_CONFLICT_RETRIES: u32 = 3;

/// Delay before the first retry; each later one multiplies by the factor.
const REWRITE_CONFLICT_INITIAL_BACKOFF: std::time::Duration = std::time::Duration::from_secs(1);

/// Growth factor of the retry delay. Deliberately not configurable.
const REWRITE_CONFLICT_BACKOFF_FACTOR: u32 = 2;

/// Ceiling one retry delay may reach, regardless of the factor.
const REWRITE_CONFLICT_MAX_BACKOFF: std::time::Duration = std::time::Duration::from_secs(10);

/// Backoff schedule for the definite-conflict retries of one plan.
///
/// The schedule is a pure function of how many retries this plan has already
/// spent, so it holds no clock, no timer, and no configuration: 1s, 2s, 4s, and
/// then nothing. There is no jitter, because the contention this backs off from
/// is one worker's own plans against one table head — a spread that jitter
/// would only blur — and the absolute publication deadline, not the schedule,
/// is what bounds the total wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RewriteConflictSchedule;

/// Why a scheduled retry did not happen.
///
/// All three are definite non-acceptance — the plan is provably uncommitted —
/// but they differ in what the caller records, so they stay distinct rather
/// than collapsing into one boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RewriteRetryStop {
    /// The schedule owes no further retry.
    Exhausted,
    /// The absolute publication deadline cannot cover the delay plus a call.
    DeadlineTruncated,
    /// The attempt was cancelled while waiting.
    Cancelled,
}

impl RewriteConflictSchedule {
    /// Returns the delay owed before the retry after `retries_spent`.
    ///
    /// `None` means the plan has exhausted its retries and is definitely
    /// uncommitted.
    pub(super) fn delay_after(retries_spent: u32) -> Option<std::time::Duration> {
        if retries_spent >= REWRITE_CONFLICT_RETRIES {
            return None;
        }
        let scaled = REWRITE_CONFLICT_INITIAL_BACKOFF
            .checked_mul(REWRITE_CONFLICT_BACKOFF_FACTOR.checked_pow(retries_spent)?)?;
        Some(scaled.min(REWRITE_CONFLICT_MAX_BACKOFF))
    }

    /// Waits the delay owed after `retries_spent`, or explains why it will not.
    ///
    /// The deadline is checked *before* the sleep, against the delay the sleep
    /// would consume, because a wait that consumes the last of the budget
    /// leaves the resubmission no time to be answered in — and an unanswered
    /// resubmission is exactly the ambiguity this whole path exists to avoid.
    /// Cancellation is observed during the wait rather than only around it, so
    /// a shutdown does not have to outlast the longest backoff.
    ///
    /// # Errors
    ///
    /// Returns [`RewriteRetryStop`] when the schedule is exhausted, the
    /// deadline cannot cover the delay, or the attempt was cancelled.
    ///
    /// # Cancellation
    ///
    /// Cancellation during the wait returns [`RewriteRetryStop::Cancelled`]
    /// without resubmitting, leaving the plan definitely uncommitted.
    pub(super) async fn wait(
        retries_spent: u32,
        deadline: RewritePublicationDeadline,
        now: chrono::DateTime<chrono::Utc>,
        stop: &tokio_util::sync::CancellationToken,
    ) -> Result<(), RewriteRetryStop> {
        let delay = Self::delay_after(retries_spent).ok_or(RewriteRetryStop::Exhausted)?;
        if deadline
            .remaining(now)
            .is_none_or(|remaining| remaining <= delay)
        {
            return Err(RewriteRetryStop::DeadlineTruncated);
        }
        tokio::select! {
            () = tokio::time::sleep(delay) => Ok(()),
            () = stop.cancelled() => Err(RewriteRetryStop::Cancelled),
        }
    }
}

impl RewriteAcceptance {
    /// Chooses the one legal follow-up for this acceptance.
    ///
    /// Ambiguity always reconciles: no amount of remaining budget or unchanged
    /// authority makes resubmitting an unknown commit safe. A definite conflict
    /// buys a revalidated retry while the schedule still owes one, the original
    /// deadline still holds, and every assumption the first attempt was built
    /// on is still true.
    pub(super) fn next_action(
        self,
        retries_spent: u32,
        deadline_passed: bool,
        authority: RewriteCommitDecision,
    ) -> RewriteConflictAction {
        match self {
            Self::Ambiguous => RewriteConflictAction::ReconcileWithoutRecommit,
            Self::DefiniteConflict => {
                if RewriteConflictSchedule::delay_after(retries_spent).is_none()
                    || deadline_passed
                    || !matches!(authority, RewriteCommitDecision::Proceed)
                {
                    RewriteConflictAction::ResetDefinitelyUncommitted
                } else {
                    RewriteConflictAction::RevalidateAndRecommit
                }
            }
        }
    }
}

/// The one absolute instant every catalog call of one publication shares.
///
/// A publication is allowed at most two catalog calls, and both of them are the
/// same operation. Giving each call its own timeout would let a first call that
/// burned the whole budget hand the retry a second full one, so the budget is
/// captured once as an *instant* rather than a duration and every later call
/// derives its wait from what is left of it. Nothing may renew it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RewritePublicationDeadline {
    /// Absolute UTC instant no catalog call may start at or complete after.
    at: chrono::DateTime<chrono::Utc>,
}

impl RewritePublicationDeadline {
    /// Captures the one deadline as `now` plus the configured commit budget.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when the configured budget is not
    /// representable as a `chrono` duration or the sum overflows the calendar.
    pub(super) fn new(
        now: chrono::DateTime<chrono::Utc>,
        budget: std::time::Duration,
    ) -> Result<Self, ForgeError> {
        let budget = chrono::Duration::from_std(budget).map_err(|_| ForgeError::InvalidConfig {
            detail: "Forge Iceberg retry timeout is not representable".to_owned(),
        })?;
        let at = now
            .checked_add_signed(budget)
            .ok_or_else(|| ForgeError::InvalidConfig {
                detail: "Forge Iceberg retry timeout overflows the publication deadline".to_owned(),
            })?;
        Ok(Self { at })
    }

    /// Returns whether `now` has reached or passed the deadline.
    pub(super) fn passed(self, now: chrono::DateTime<chrono::Utc>) -> bool {
        now >= self.at
    }

    /// Returns the strictly positive budget left at `now`.
    ///
    /// `None` means no catalog call may start: either the deadline has elapsed
    /// or what remains cannot be represented as a wait, and both are refusals
    /// rather than an unbounded call.
    pub(super) fn remaining(
        self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Option<std::time::Duration> {
        self.at
            .signed_duration_since(now)
            .to_std()
            .ok()
            .filter(|remaining: &std::time::Duration| !remaining.is_zero())
    }
}

/// What one publication's catalog submission boundary actually did.
///
/// The distinction the type exists to keep is `NotSubmitted` versus
/// `AcceptanceUnknown`: the first is proof that no replacement can be live, the
/// second is the absence of proof. Deriving that later from an error string is
/// how a definitely-uncommitted operation gets stranded as Prepared, or worse,
/// an ambiguous one gets Reset.
pub(super) enum RewriteSubmission {
    /// The catalog accepted the replacement and returned the committed table.
    Committed(Box<Table>),
    /// No `update_table` call was started, so nothing landed.
    NotSubmitted(ForgeError),
    /// The catalog answered with a definite, non-retryable refusal.
    DefiniteConflict(ForgeError),
    /// A call was submitted and its acceptance is unknown.
    AcceptanceUnknown(ForgeError),
}

/// Derives the exact time partition one live file belongs to.
///
/// The partition value is read off the file itself rather than out of
/// `vala.file_list`, and that difference is load-bearing: the outputs of a
/// previous rewrite have no `file_list` row, so a `file_list`-derived partition
/// would make every already-rewritten file permanently unselectable. The table
/// spec supplies the transform and the data file supplies the transformed
/// value, both of which travel with every file forever.
///
/// Returns `None` when the spec carries no day or hour transform, or when the
/// file's partition tuple does not hold an integer at that position — a table
/// Bifrost did not lay out, which Forge declines to rewrite rather than guess
/// a partition for.
pub(super) fn rewrite_time_partition(
    spec: &PartitionSpec,
    partition: &Struct,
) -> Option<TimePartition> {
    let (index, granularity) = spec
        .fields()
        .iter()
        .enumerate()
        .find_map(|(index, field)| match field.transform {
            Transform::Day => Some((index, TimeGranularity::Day)),
            Transform::Hour => Some((index, TimeGranularity::Hour)),
            _ => None,
        })?;
    let Some(Some(Literal::Primitive(PrimitiveLiteral::Int(value)))) =
        partition.fields().get(index)
    else {
        return None;
    };
    let seconds = i64::from(*value).checked_mul(match granularity {
        TimeGranularity::Day => 86_400,
        TimeGranularity::Hour => 3_600,
    })?;
    TimePartition::new(granularity, chrono::DateTime::from_timestamp(seconds, 0)?).ok()
}

/// Derives the single time partition one publication belongs to.
///
/// The audit row for a rewrite names one partition, and so does the group fence
/// every live operation is addressed by, so a publication that spanned two
/// partitions would have no honest identity to record. The managed core plans
/// the whole table and this owner does not narrow its selection, so the
/// partition is read back off the files it actually touched and a crossing
/// attempt is refused rather than recorded under one of the partitions it
/// happened to include.
///
/// # Errors
///
/// Returns [`ForgeError::Reconciliation`] when a file carries no derivable
/// partition or when the touched files do not all share exactly one.
pub(super) fn rewrite_group_partition<'file>(
    spec: &PartitionSpec,
    files: impl IntoIterator<Item = &'file DataFile>,
) -> Result<TimePartition, ForgeError> {
    let mut partitions = BTreeMap::new();
    for file in files {
        let partition = rewrite_time_partition(spec, file.partition()).ok_or_else(|| {
            ForgeError::Reconciliation {
                detail: format!(
                    "rewrite file {} carries no derivable Bifrost time partition",
                    file.file_path()
                ),
            }
        })?;
        partitions.insert(
            (partition.granularity_str(), partition.start_utc()),
            partition,
        );
    }
    let mut found = partitions.into_values();
    match (found.next(), found.next()) {
        (Some(partition), None) => Ok(partition),
        (Some(_), Some(_)) => Err(ForgeError::Reconciliation {
            detail: "rewrite publication spans more than one time partition".to_owned(),
        }),
        (None, _) => Err(ForgeError::Reconciliation {
            detail: "rewrite publication touches no partitioned file".to_owned(),
        }),
    }
}

/// The exact terms of one fenced rewrite publication.
///
/// Borrowed rather than owned so the caller keeps the table it revalidated
/// against: a retry after a definite conflict reloads the table and submits the
/// same request against the newer one, and nothing here may outlive that.
pub(super) struct ForgeRewriteCommit<'commit> {
    /// Table state the request was derived and revalidated against.
    pub(super) table: &'commit Table,
    /// Fully derived replacement this commit executes without further decisions.
    pub(super) request: &'commit RewriteCommitRequest,
    /// The one absolute budget this call shares with the whole publication.
    pub(super) deadline: RewritePublicationDeadline,
}

impl Forge {
    /// Reads the immutable live file set of one exact snapshot into a base.
    ///
    /// The snapshot is addressed by identity rather than by "whatever is
    /// current", which is what lets a publication re-read freshly loaded
    /// metadata after its managed execution without silently re-planning
    /// against a head a concurrent writer moved: the claimed base either is
    /// still retained and reads back byte-identically, or it is gone and the
    /// authority decision refuses. Alive manifest entries carry the data
    /// sequence number that decides which deletes a replacement can still be
    /// reached by, so it is captured here alongside the file.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when `snapshot_id` is not retained by
    /// `table` or an alive entry carries no data sequence number,
    /// [`ForgeError::Catalog`] when the manifest list or a manifest cannot be
    /// read, and the duplicate and identity failures [`RewriteBase::try_new`]
    /// raises.
    pub(super) async fn rewrite_base_at(
        &self,
        table: &Table,
        snapshot_id: i64,
    ) -> Result<RewriteBase, ForgeError> {
        let snapshot = table
            .metadata()
            .snapshot_by_id(snapshot_id)
            .ok_or_else(|| ForgeError::Invariant {
                detail: format!("rewrite base snapshot {snapshot_id} is no longer retained"),
            })?;
        let manifests = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .map_err(ForgeError::Catalog)?;
        let mut entries = Vec::new();
        for manifest_file in manifests.entries() {
            let manifest = manifest_file
                .load_manifest(table.file_io())
                .await
                .map_err(ForgeError::Catalog)?;
            for entry in manifest.entries().iter().filter(|entry| entry.is_alive()) {
                let sequence_number =
                    entry
                        .sequence_number()
                        .ok_or_else(|| ForgeError::Invariant {
                            detail: format!(
                                "live manifest entry {} carries no data sequence number",
                                entry.data_file().file_path()
                            ),
                        })?;
                entries.push((entry.data_file().clone(), sequence_number));
            }
        }
        RewriteBase::try_new(
            snapshot.snapshot_id(),
            snapshot.sequence_number(),
            table.metadata().default_partition_spec_id(),
            entries,
        )
    }

    /// Replaces one derived file set under the publication fence.
    ///
    /// Every decision this commit encodes was made before it was called: which
    /// files leave, which arrive, which deletes are provably obsolete, what
    /// sequence number the replacements carry, and what the snapshot records
    /// about its own lineage. This method chooses nothing — it submits.
    ///
    /// The obsolete-delete filter the Iceberg action would run for us is turned
    /// off deliberately. Forge already derived the disposition from the
    /// immutable base and can defend it file by file; letting a second owner
    /// drop delete files on a rule Forge did not evaluate would make the safety
    /// of a rewrite depend on two derivations agreeing.
    ///
    /// Cancellation and timeout are raced against the in-flight commit and are
    /// never read as proof of rejection: both surface as
    /// [`RewriteSubmission::AcceptanceUnknown`] so the caller's Prepared
    /// operation stays open for evidence-based recovery.
    ///
    /// Every pre-submission refusal — a lost fence, a cancelled attempt, a
    /// deadline that already elapsed, a replacement the transaction layer will
    /// not even encode — is reported as [`RewriteSubmission::NotSubmitted`]
    /// instead. That is knowledge, not an outcome: no catalog call started, so
    /// the caller may close the operation as definitely uncommitted rather than
    /// stranding it for a successor to reconcile a commit that never happened.
    ///
    /// The wait itself is derived from the publication's one absolute deadline
    /// immediately before the call, so the initial commit and its single
    /// permitted retry share one budget and neither can outlive it.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Lease`] or [`ForgeError::Sql`] when the lease
    /// cannot be renewed or fenced against the operator pool, and
    /// [`ForgeError::InvalidConfig`] when the clock cannot report the current
    /// instant. Every publication outcome is reported through the returned
    /// [`RewriteSubmission`] rather than as an error.
    ///
    /// # Cancellation
    ///
    /// Cancellation observed before submission is `NotSubmitted`; cancellation
    /// after submission leaves acceptance unknown and is never a clean stop.
    pub(super) async fn commit_rewrite(
        &self,
        lease: &mut ForgeLease,
        commit: ForgeRewriteCommit<'_>,
        stop: &CancellationToken,
    ) -> Result<RewriteSubmission, ForgeError> {
        let ForgeRewriteCommit {
            table,
            request,
            deadline,
        } = commit;
        let span = catalog_commit_span(
            "iceberg_rewrite",
            Some((request.identity.task_id, request.identity.attempt_id)),
        );
        if !lease.renew(&self.core.operator_pool).await?
            || !lease.commit_window_fits(self.core.config.commit_window())
        {
            return Ok(RewriteSubmission::NotSubmitted(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            }));
        }
        let transaction = match request.encode(table) {
            Ok(transaction) => transaction,
            Err(error) => return Ok(RewriteSubmission::NotSubmitted(error)),
        };
        lease.require_fence(&self.core.operator_pool).await?;
        if !lease.commit_window_fits(self.core.config.commit_window()) {
            return Ok(RewriteSubmission::NotSubmitted(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            }));
        }
        if stop.is_cancelled() {
            return Ok(RewriteSubmission::NotSubmitted(ForgeError::Shutdown));
        }
        // The wait is what is left of the publication's one deadline, taken
        // from the same clock authority that created it and read here rather
        // than at construction, so a slow first call shortens the retry instead
        // of the configured budget silently restarting.
        let Some(remaining) = deadline.remaining(self.core.clock.now()?) else {
            return Ok(RewriteSubmission::NotSubmitted(ForgeError::Timeout {
                operation: "rewrite publication",
            }));
        };
        let catalog = self.core.catalog.as_ref();
        let outcome = async move {
            let commit = transaction.commit(catalog);
            tokio::pin!(commit);
            tokio::select! {
                response = tokio::time::timeout(remaining, &mut commit) => match response {
                    Ok(Ok(committed)) => Ok(committed),
                    // A retryable catalog answer invites another call rather
                    // than closing this one, so it is not proof of rejection:
                    // it reconciles rather than resets.
                    Ok(Err(error)) if error.retryable() => {
                        Err(RewriteSubmission::AcceptanceUnknown(ForgeError::Catalog(error)))
                    }
                    Ok(Err(error)) => {
                        Err(RewriteSubmission::DefiniteConflict(ForgeError::Catalog(error)))
                    }
                    Err(_) => Err(RewriteSubmission::AcceptanceUnknown(ForgeError::Reconciliation {
                        detail: "Forge rewrite commit ran out of publication budget with unknown acceptance".to_owned(),
                    })),
                },
                () = stop.cancelled() => Err(RewriteSubmission::AcceptanceUnknown(ForgeError::Reconciliation {
                    detail: "Forge rewrite commit was cancelled with unknown acceptance".to_owned(),
                })),
            }
        }
        .instrument(span.clone())
        .await;
        span.record(
            "result",
            if outcome.is_ok() {
                "committed"
            } else {
                "failed"
            },
        );
        match outcome {
            Ok(committed) => request.confirm_committed_lineage(committed),
            Err(submission) => Ok(submission),
        }
    }
}

/// Constructs the shared closed-schema span for one catalog commit future.
///
/// Catalog commit detail is protocol evidence rather than a public metric, so
/// it lives here as a structured trace. Only the strategy, result, role, and
/// scrubbed durable task UUIDs are owner-authored: tenant, table, SQL,
/// object-path, and error details must never be added by callers.
pub(super) fn catalog_commit_span(
    strategy: &'static str,
    task_identity: Option<(Uuid, Uuid)>,
) -> tracing::Span {
    let span = tracing::info_span!(
        "bifrost.forge.catalog.commit",
        strategy,
        result = tracing::field::Empty,
        role = "forge_worker",
        task_id = tracing::field::Empty,
        attempt_id = tracing::field::Empty,
    );
    if let Some((task_id, attempt_id)) = task_identity {
        span.record("task_id", tracing::field::display(task_id));
        span.record("attempt_id", tracing::field::display(attempt_id));
    }
    span
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use iceberg::spec::{
        DataContentType, DataFile, DataFileBuilder, DataFileFormat, Literal, PrimitiveLiteral,
        Struct,
    };
    use uuid::Uuid;

    use super::*;
    use crate::forge::managed::RewriteHandoff;
    use crate::forge::managed::fingerprint::ForgeRewriteEvidence;

    /// Partition spec every fixture file is written under.
    const SPEC: i32 = 0;

    /// Builds one live data descriptor at `path` in `partition`.
    ///
    /// # Panics
    ///
    /// Panics when the descriptor cannot be built, which is a fixture
    /// construction invariant rather than an input.
    fn data(path: &str, partition: i32) -> DataFile {
        DataFileBuilder::default()
            .content(DataContentType::Data)
            .file_path(path.to_owned())
            .file_format(DataFileFormat::Parquet)
            .partition(bucket(partition))
            .record_count(10)
            .file_size_in_bytes(1_024)
            .partition_spec_id(SPEC)
            .build()
            .expect("fixture data descriptor")
    }

    /// Builds one position-delete descriptor, optionally naming its target.
    ///
    /// # Panics
    ///
    /// Panics when the descriptor cannot be built.
    fn position_delete(path: &str, partition: i32, referenced: Option<&str>) -> DataFile {
        DataFileBuilder::default()
            .content(DataContentType::PositionDeletes)
            .file_path(path.to_owned())
            .file_format(DataFileFormat::Parquet)
            .partition(bucket(partition))
            .record_count(1)
            .file_size_in_bytes(64)
            .partition_spec_id(SPEC)
            .referenced_data_file(referenced.map(str::to_owned))
            .build()
            .expect("fixture position delete descriptor")
    }

    /// Builds one equality-delete descriptor scoped to `partition`.
    ///
    /// # Panics
    ///
    /// Panics when the descriptor cannot be built.
    fn equality_delete(path: &str, partition: i32) -> DataFile {
        DataFileBuilder::default()
            .content(DataContentType::EqualityDeletes)
            .file_path(path.to_owned())
            .file_format(DataFileFormat::Parquet)
            .partition(bucket(partition))
            .record_count(1)
            .file_size_in_bytes(64)
            .partition_spec_id(SPEC)
            .equality_ids(Some(vec![1]))
            .build()
            .expect("fixture equality delete descriptor")
    }

    /// Builds the one-field partition tuple every fixture file carries.
    fn bucket(partition: i32) -> Struct {
        Struct::from_iter([Some(Literal::Primitive(PrimitiveLiteral::Int(partition)))])
    }

    /// Assembles a base snapshot from `(descriptor, data sequence number)` pairs.
    ///
    /// # Panics
    ///
    /// Panics when the fixture entries contradict each other, which would make
    /// every case built on them meaningless.
    fn base_view(
        snapshot_id: i64,
        sequence_number: i64,
        entries: Vec<(DataFile, i64)>,
    ) -> RewriteBase {
        RewriteBase::try_new(snapshot_id, sequence_number, SPEC, entries)
            .expect("fixture base snapshot")
    }

    /// Builds the identity every fixture commit request is bound to.
    fn identity() -> RewriteCommitIdentity {
        RewriteCommitIdentity {
            task_id: Uuid::from_u128(1),
            attempt_id: Uuid::from_u128(2),
            operation_id: Uuid::from_u128(1),
            group: "tenant/table".to_owned(),
            plan_hash: [7_u8; 32],
            evidence: ForgeRewriteEvidence {
                base_snapshot_id: 100,
                selection_fingerprint: "selection".to_owned(),
                debt_fingerprint: "debt".to_owned(),
                policy_fingerprint: "policy".to_owned(),
            },
        }
    }

    /// Builds the handoff the canonical fixture rewrite produced.
    ///
    /// # Panics
    ///
    /// Panics when the handoff is self-contradictory.
    fn handoff(rewritten: &[&str], position: &[&str], equality: &[&str]) -> RewriteHandoff {
        RewriteHandoff::try_new(
            100,
            rewritten.iter().map(|path| (*path).to_owned()).collect(),
            position.iter().map(|path| (*path).to_owned()).collect(),
            equality.iter().map(|path| (*path).to_owned()).collect(),
            vec![data("out-1.parquet", 0)],
        )
        .expect("fixture handoff")
    }

    /// The live entries every base fixture in this module is built from.
    ///
    /// Keeping them in one place is what lets the wrong-base case differ from
    /// the canonical base in the snapshot identity and nothing else, so a
    /// refusal there proves the identity check rather than some path that
    /// happens to be missing too.
    fn canonical_entries() -> Vec<(DataFile, i64)> {
        vec![
            (data("a.parquet", 0), 3),
            (data("b.parquet", 0), 4),
            (position_delete("p.parquet", 0, Some("a.parquet")), 5),
            (equality_delete("e.parquet", 0), 6),
        ]
    }

    /// The canonical base every request case starts from.
    fn canonical_base() -> RewriteBase {
        base_view(100, 9, canonical_entries())
    }

    /// Builds the exact inputs one canonical request is derived from.
    fn inputs<'a>(
        handoff: &'a RewriteHandoff,
        base: &'a RewriteBase,
        selected: &'a [String],
        identity: &'a RewriteCommitIdentity,
    ) -> RewriteCommitInputs<'a> {
        RewriteCommitInputs {
            handoff,
            base,
            selected_inputs: selected,
            identity,
        }
    }

    /// Every way an output descriptor can fail to be the core's own work.
    ///
    /// Output validation is separated from input validation because it asks a
    /// different question: not whether the base supports the removal, but
    /// whether the added descriptors are the ones the rewrite actually
    /// produced against this base's partition spec and live file set.
    fn assert_edited_outputs_are_refused(
        base: &RewriteBase,
        identity: &RewriteCommitIdentity,
        selected: &[String],
    ) {
        // Edited output metadata: an output that claims another partition spec
        // than the base's default is a descriptor this publication did not get
        // from the core.
        let edited = RewriteHandoff::try_new(
            100,
            selected.to_vec(),
            Vec::new(),
            Vec::new(),
            vec![
                DataFileBuilder::default()
                    .content(DataContentType::Data)
                    .file_path("out-1.parquet".to_owned())
                    .file_format(DataFileFormat::Parquet)
                    .partition(bucket(0))
                    .record_count(10)
                    .file_size_in_bytes(1_024)
                    .partition_spec_id(SPEC + 1)
                    .build()
                    .expect("edited output descriptor"),
            ],
        )
        .expect("edited handoff is internally consistent");
        assert!(
            RewriteCommitRequest::derive(inputs(&edited, base, selected, identity)).is_err(),
            "an output must carry the base snapshot's own partition spec"
        );

        // An output that is already live in the base would republish a file the
        // table already references.
        let colliding = RewriteHandoff::try_new(
            100,
            vec!["a.parquet".to_owned()],
            Vec::new(),
            Vec::new(),
            vec![data("b.parquet", 0)],
        )
        .expect("colliding handoff is internally consistent");
        assert!(
            RewriteCommitRequest::derive(inputs(
                &colliding,
                base,
                &["a.parquet".to_owned()],
                identity
            ))
            .is_err(),
            "an output path must not already be live in the base snapshot"
        );
    }

    /// Synchronous validation cross-checks the handoff against the base.
    ///
    /// Each case is a distinct way one publication could describe work the base
    /// snapshot does not support, and every one of them must be refused before
    /// the catalog is touched: an unknown input would remove a file this
    /// rewrite never read, a duplicated input would remove one file twice, a
    /// selection mismatch would publish work the durable task never authorized,
    /// and an output whose descriptor was edited would publish measurements the
    /// core never made.
    #[test]
    fn rewrite_commit_request_validates_identity_base_and_outputs() {
        let base = canonical_base();
        let identity = identity();
        let selected = vec!["a.parquet".to_owned(), "b.parquet".to_owned()];
        let canonical = handoff(&["a.parquet", "b.parquet"], &["p.parquet"], &["e.parquet"]);
        let request = RewriteCommitRequest::derive(inputs(&canonical, &base, &selected, &identity))
            .expect("the canonical publication is derivable");
        assert_eq!(request.base_snapshot_id, 100);
        assert_eq!(
            request
                .removed_data_files
                .iter()
                .map(DataFile::file_path)
                .collect::<Vec<_>>(),
            vec!["a.parquet", "b.parquet"]
        );
        assert_eq!(
            request
                .added_data_files
                .iter()
                .map(DataFile::file_path)
                .collect::<Vec<_>>(),
            vec!["out-1.parquet"]
        );

        // Wrong base: the handoff was resolved against a snapshot this base is
        // not, so every path identity it names is unverified.
        let wrong_base = base_view(101, 9, canonical_entries());
        assert!(
            RewriteCommitRequest::derive(inputs(&canonical, &wrong_base, &selected, &identity))
                .is_err(),
            "a handoff must not publish against a base it was not planned on"
        );

        // Unknown input: a consumed path that is not live in the base.
        let unknown = handoff(&["a.parquet", "ghost.parquet"], &[], &[]);
        let unknown_selected = vec!["a.parquet".to_owned(), "ghost.parquet".to_owned()];
        assert!(
            RewriteCommitRequest::derive(inputs(&unknown, &base, &unknown_selected, &identity))
                .is_err(),
            "publication cannot remove a file the base snapshot does not carry"
        );

        // Partial input: the core packed less of the bound than was offered.
        // The selection is a ceiling, so publishing less of it is honest work.
        let partial = handoff(&["a.parquet"], &[], &[]);
        assert!(
            RewriteCommitRequest::derive(inputs(&partial, &base, &selected, &identity)).is_ok(),
            "the durable selection bounds the rewrite rather than mandating it"
        );

        // Unauthorized input: a live file outside the selection the task bound.
        let unauthorized = handoff(&["a.parquet", "b.parquet"], &[], &[]);
        assert!(
            RewriteCommitRequest::derive(inputs(
                &unauthorized,
                &base,
                &["a.parquet".to_owned()],
                &identity
            ))
            .is_err(),
            "publication cannot remove a file the durable task never authorized"
        );

        // Duplicate input: the same live file removed twice.
        let duplicated = RewriteCommitInputs {
            handoff: &canonical,
            base: &base,
            selected_inputs: &[
                "a.parquet".to_owned(),
                "a.parquet".to_owned(),
                "b.parquet".to_owned(),
            ],
            identity: &identity,
        };
        assert!(
            RewriteCommitRequest::derive(duplicated).is_err(),
            "a duplicated selection cannot describe one exact removal set"
        );

        // Unknown applied delete: an applied path the base does not carry.
        let ghost_delete = handoff(&["a.parquet", "b.parquet"], &["ghost-p.parquet"], &[]);
        assert!(
            RewriteCommitRequest::derive(inputs(&ghost_delete, &base, &selected, &identity))
                .is_err(),
            "an applied delete must be a live delete of the base snapshot"
        );

        assert_edited_outputs_are_refused(&base, &identity, &selected);
    }

    /// One delete-disposition case: a base state, a rewrite, and the deletes
    /// the commit is then allowed to remove.
    struct Case {
        /// What the case proves.
        name: &'static str,
        /// Live base entries and their data sequence numbers.
        entries: Vec<(DataFile, i64)>,
        /// Paths the rewrite consumed and replaced.
        rewritten: Vec<&'static str>,
        /// Applied position-delete paths reported by the core.
        position: Vec<&'static str>,
        /// Applied equality-delete paths reported by the core.
        equality: Vec<&'static str>,
        /// Delete paths that may be removed by the commit.
        removable: Vec<&'static str>,
    }

    /// Every scope dimension the delete policy has to separate.
    ///
    /// The cases are data, not behavior; keeping them here leaves the policy
    /// test as the assertion loop it describes.
    fn delete_disposition_cases() -> Vec<Case> {
        vec![
            Case {
                name: "a position delete naming a rewritten file is removable",
                entries: vec![
                    (data("a.parquet", 0), 3),
                    (position_delete("p.parquet", 0, Some("a.parquet")), 5),
                ],
                rewritten: vec!["a.parquet"],
                position: vec!["p.parquet"],
                equality: vec![],
                removable: vec!["p.parquet"],
            },
            Case {
                name: "a position delete naming a surviving file is retained",
                entries: vec![
                    (data("a.parquet", 0), 3),
                    (data("b.parquet", 0), 3),
                    (position_delete("p.parquet", 0, Some("b.parquet")), 5),
                ],
                rewritten: vec!["a.parquet"],
                position: vec!["p.parquet"],
                equality: vec![],
                removable: vec![],
            },
            Case {
                name: "a position delete naming nothing is never provably scoped",
                entries: vec![
                    (data("a.parquet", 0), 3),
                    (position_delete("p.parquet", 0, None), 5),
                ],
                rewritten: vec!["a.parquet"],
                position: vec!["p.parquet"],
                equality: vec![],
                removable: vec![],
            },
            Case {
                name: "an equality delete whose whole partition was rewritten is removable",
                entries: vec![
                    (data("a.parquet", 0), 3),
                    (equality_delete("e.parquet", 0), 6),
                ],
                rewritten: vec!["a.parquet"],
                position: vec![],
                equality: vec!["e.parquet"],
                removable: vec!["e.parquet"],
            },
            Case {
                name: "an equality delete is retained for a surviving lower-sequence file",
                entries: vec![
                    (data("a.parquet", 0), 3),
                    (data("b.parquet", 0), 4),
                    (equality_delete("e.parquet", 0), 6),
                ],
                rewritten: vec!["a.parquet"],
                position: vec![],
                equality: vec!["e.parquet"],
                removable: vec![],
            },
            Case {
                name: "an equality delete ignores a surviving file it cannot apply to",
                entries: vec![
                    (data("a.parquet", 0), 3),
                    (data("b.parquet", 0), 7),
                    (equality_delete("e.parquet", 0), 6),
                ],
                rewritten: vec!["a.parquet"],
                position: vec![],
                equality: vec!["e.parquet"],
                removable: vec!["e.parquet"],
            },
            Case {
                name: "an equality delete ignores a surviving file in another partition",
                entries: vec![
                    (data("a.parquet", 0), 3),
                    (data("c.parquet", 1), 3),
                    (equality_delete("e.parquet", 0), 6),
                ],
                rewritten: vec!["a.parquet"],
                position: vec![],
                equality: vec!["e.parquet"],
                removable: vec!["e.parquet"],
            },
            Case {
                name: "an unapplied delete is never removed even when its scope is empty",
                entries: vec![
                    (data("a.parquet", 0), 3),
                    (equality_delete("e.parquet", 0), 6),
                ],
                rewritten: vec!["a.parquet"],
                position: vec![],
                equality: vec![],
                removable: vec![],
            },
        ]
    }

    /// One delete policy, covering both delete kinds and every scope dimension.
    ///
    /// A position delete is removable only when every data file it can apply to
    /// was rewritten, and it can only be *proven* to apply to one file when it
    /// names that file. An equality delete is removable only when no surviving
    /// data file is inside its partition and spec scope at a lower data
    /// sequence number, because those are exactly the rows it would still have
    /// to delete.
    #[test]
    fn rewrite_delete_disposition_preserves_shared_position_and_equality_scope() {
        for case in delete_disposition_cases() {
            let base = base_view(100, 9, case.entries);
            let rewritten = case
                .rewritten
                .iter()
                .map(|path| (*path).to_owned())
                .collect::<Vec<_>>();
            let applied_position = case
                .position
                .iter()
                .map(|path| (*path).to_owned())
                .collect::<Vec<_>>();
            let applied_equality = case
                .equality
                .iter()
                .map(|path| (*path).to_owned())
                .collect::<Vec<_>>();
            let disposition = RewriteDeleteDisposition::derive(
                &base,
                &rewritten,
                &applied_position,
                &applied_equality,
            )
            .expect(case.name);
            assert_eq!(
                disposition
                    .removable
                    .iter()
                    .map(DataFile::file_path)
                    .collect::<Vec<_>>(),
                case.removable,
                "{}",
                case.name
            );
        }
    }

    /// Replacement rows are published at the base snapshot's own sequence.
    ///
    /// Assigning the base sequence is what keeps an already-applied equality
    /// delete from deleting the rows that now materialize its effect: such a
    /// delete was live in the base, so its own sequence is at or below the
    /// base's, and an equality delete only reaches strictly older data. A
    /// delete that arrived *after* the base carries a higher sequence and must
    /// still reach the replacements, which is why the new snapshot's sequence
    /// is the wrong choice.
    #[test]
    fn rewrite_replacement_sequences_prevent_delete_reapplication() {
        let base = base_view(
            100,
            9,
            vec![
                (data("a.parquet", 0), 3),
                (equality_delete("e.parquet", 0), 6),
            ],
        );
        let identity = identity();
        let selected = vec!["a.parquet".to_owned()];
        let handoff = handoff(&["a.parquet"], &[], &["e.parquet"]);
        let request = RewriteCommitRequest::derive(inputs(&handoff, &base, &selected, &identity))
            .expect("the canonical publication is derivable");

        assert_eq!(
            request.new_data_file_sequence_number, 9,
            "replacements are published at the base snapshot's sequence"
        );
        assert!(
            !delete_reaches_replacement(6, request.new_data_file_sequence_number),
            "a delete the rewrite already applied must not reach the replacement rows"
        );
        assert!(
            !delete_reaches_replacement(9, request.new_data_file_sequence_number),
            "a delete at the base sequence must not reach the replacement rows"
        );
        assert!(
            delete_reaches_replacement(10, request.new_data_file_sequence_number),
            "a delete committed after the base must still reach the replacement rows"
        );
    }

    /// Every canonical property is present, bound, and individually required.
    ///
    /// The property set is the only evidence a successor has when it finds a
    /// snapshot and has to decide whether that snapshot is *this* operation's
    /// commit, so each field is checked by removing or altering it and
    /// requiring validation to refuse the result.
    #[test]
    fn rewrite_snapshot_properties_bind_lineage_and_fingerprints() {
        let base = canonical_base();
        let identity = identity();
        let selected = vec!["a.parquet".to_owned(), "b.parquet".to_owned()];
        let handoff = handoff(&["a.parquet", "b.parquet"], &["p.parquet"], &["e.parquet"]);
        let request = RewriteCommitRequest::derive(inputs(&handoff, &base, &selected, &identity))
            .expect("the canonical publication is derivable");
        let properties = request.snapshot_properties();

        assert_eq!(
            properties.get("forge.workflow").map(String::as_str),
            Some(REWRITE_WORKFLOW),
            "reconciliation recognizes a rewrite snapshot by its workflow property"
        );
        assert_eq!(
            properties.get("forge.task_id").map(String::as_str),
            Some(identity.task_id.to_string().as_str())
        );
        assert_eq!(
            properties.get("forge.operation_id").map(String::as_str),
            Some(identity.operation_id.to_string().as_str())
        );
        assert_eq!(
            properties.get("forge.group").map(String::as_str),
            Some(identity.group.as_str())
        );
        assert_eq!(
            properties
                .get("forge.rewrite.debt_fingerprint")
                .map(String::as_str),
            Some("debt")
        );

        assert_eq!(
            properties.len(),
            REWRITE_SNAPSHOT_PROPERTY_KEYS.len(),
            "the derivation writes exactly the canonical property set"
        );
        let canonical = RewriteSnapshotProperties::validate(&properties)
            .expect("the canonical property set validates");

        for key in REWRITE_SNAPSHOT_PROPERTY_KEYS {
            let mut absent = properties.clone();
            absent.remove(*key);
            assert!(
                RewriteSnapshotProperties::validate(&absent).is_err(),
                "a rewrite snapshot missing {key} cannot be reconciled"
            );
            let mut empty = properties.clone();
            empty.insert((*key).to_owned(), String::new());
            assert!(
                RewriteSnapshotProperties::validate(&empty).is_err(),
                "an empty {key} is not evidence of anything"
            );
            let mut altered = properties.clone();
            altered.insert((*key).to_owned(), "altered".to_owned());
            let reparsed = RewriteSnapshotProperties::validate(&altered);
            assert!(
                reparsed.is_err_and(|_| true)
                    || RewriteSnapshotProperties::validate(&altered)
                        .is_ok_and(|parsed| parsed != canonical),
                "reconciliation must not ignore {key}"
            );
        }

        let mut partial: BTreeMap<String, String> = BTreeMap::new();
        partial.insert("forge.workflow".to_owned(), REWRITE_WORKFLOW.to_owned());
        assert!(
            RewriteSnapshotProperties::validate(&partial).is_err(),
            "a partially written property set is not a rewrite commit"
        );
    }

    /// One authority dimension broken, paired with the refusal it must produce.
    type RefusalCase = (fn(&mut RewriteCommitAuthority), RewriteRefusal);

    /// Sets one authority dimension of `authority` by its matrix position.
    ///
    /// The position order is the refusal order, which is what lets the
    /// exhaustive sweep below predict the reported refusal from the lowest
    /// broken bit rather than restating the decision it is checking.
    fn set_authority_dimension(
        authority: &mut RewriteCommitAuthority,
        position: usize,
        held: bool,
    ) {
        match position {
            0 => authority.fence.lease_held = held,
            1 => authority.fence.fence_held = held,
            2 => authority.fence.commit_window_fits = held,
            3 => authority.attempt.cancelled = !held,
            4 => authority.attempt.deadline_passed = !held,
            5 => authority.table.base_is_retained = held,
            6 => authority.table.schema_unchanged = held,
            7 => authority.files.inputs_all_live = held,
            8 => authority.files.delete_scope_safe = held,
            _ => unreachable!("the authority matrix has exactly nine dimensions"),
        }
    }

    /// Builds one authority with every dimension held.
    ///
    /// Each matrix test starts from this complete authority and breaks only the
    /// dimensions it is asserting about, so a refusal is always attributable.
    fn authorized_rewrite_commit_authority() -> RewriteCommitAuthority {
        RewriteCommitAuthority {
            fence: RewriteFenceAuthority {
                lease_held: true,
                fence_held: true,
                commit_window_fits: true,
            },
            attempt: RewriteAttemptAuthority {
                cancelled: false,
                deadline_passed: false,
            },
            table: RewriteTableAuthority {
                base_is_retained: true,
                schema_unchanged: true,
            },
            files: RewriteFileAuthority {
                inputs_all_live: true,
                delete_scope_safe: true,
            },
        }
    }

    /// The nine single-dimension breaks in exact refusal order.
    ///
    /// Each case breaks exactly one dimension of an otherwise complete
    /// authority, so the expected refusal is also a proof of the order: an
    /// earlier check would have reported a different one.
    fn single_dimension_refusals() -> [RefusalCase; 9] {
        [
            (
                |authority| authority.fence.lease_held = false,
                RewriteRefusal::LeaseLost,
            ),
            (
                |authority| authority.fence.fence_held = false,
                RewriteRefusal::FenceLost,
            ),
            (
                |authority| authority.fence.commit_window_fits = false,
                RewriteRefusal::CommitWindow,
            ),
            (
                |authority| authority.attempt.cancelled = true,
                RewriteRefusal::Cancelled,
            ),
            (
                |authority| authority.attempt.deadline_passed = true,
                RewriteRefusal::Deadline,
            ),
            (
                |authority| authority.table.base_is_retained = false,
                RewriteRefusal::BaseNotRetained,
            ),
            (
                |authority| authority.table.schema_unchanged = false,
                RewriteRefusal::SchemaChanged,
            ),
            (
                |authority| authority.files.inputs_all_live = false,
                RewriteRefusal::InputsChanged,
            ),
            (
                |authority| authority.files.delete_scope_safe = false,
                RewriteRefusal::DeleteScopeUnsafe,
            ),
        ]
    }

    /// Every knowable authority failure refuses before publication.
    #[test]
    fn rewrite_commit_decision_matrix_fails_closed() {
        let authorized = authorized_rewrite_commit_authority();
        assert_eq!(authorized.decide(), RewriteCommitDecision::Proceed);
        for (break_one, expected) in single_dimension_refusals() {
            let mut authority = authorized;
            break_one(&mut authority);
            assert_eq!(
                authority.decide(),
                RewriteCommitDecision::Refuse(expected),
                "one invalid authority dimension must refuse publication"
            );
        }

        // Uncertain acceptance never recommits, whatever else is true.
        for retries_spent in 0..=super::REWRITE_CONFLICT_RETRIES {
            assert_eq!(
                RewriteAcceptance::Ambiguous.next_action(retries_spent, false, authorized.decide()),
                RewriteConflictAction::ReconcileWithoutRecommit
            );
        }
        // A definite conflict buys each scheduled retry and no more.
        for retries_spent in 0..super::REWRITE_CONFLICT_RETRIES {
            assert_eq!(
                RewriteAcceptance::DefiniteConflict.next_action(
                    retries_spent,
                    false,
                    authorized.decide()
                ),
                RewriteConflictAction::RevalidateAndRecommit
            );
        }
        assert_eq!(
            RewriteAcceptance::DefiniteConflict.next_action(
                super::REWRITE_CONFLICT_RETRIES,
                false,
                authorized.decide()
            ),
            RewriteConflictAction::ResetDefinitelyUncommitted
        );
        assert_eq!(
            RewriteAcceptance::DefiniteConflict.next_action(0, true, authorized.decide()),
            RewriteConflictAction::ResetDefinitelyUncommitted
        );
        assert_eq!(
            RewriteAcceptance::DefiniteConflict.next_action(
                0,
                false,
                RewriteCommitDecision::Refuse(RewriteRefusal::InputsChanged)
            ),
            RewriteConflictAction::ResetDefinitelyUncommitted,
            "a changed assumption ends the attempt instead of buying a retry"
        );
    }

    /// The retry schedule and unknown acceptance are exact, not approximate.
    ///
    /// Every branch of the definite-conflict path is here because each one is a
    /// way to publish twice or to strand an operation: a delay that drifts, a
    /// wait the deadline cannot cover, a cancellation that resubmits anyway, or
    /// an ambiguous answer treated as a refusal. Time is paused, so the elapsed
    /// figures are the schedule's own and not a timing artefact.
    ///
    /// # Panics
    ///
    /// Panics when a delay, the exhaustion point, the deadline truncation, the
    /// cancellation behaviour, or the ambiguous action changes.
    #[tokio::test(start_paused = true)]
    async fn definite_conflict_retry_schedule_and_unknown_acceptance_are_exact() {
        use std::time::Duration;
        use tokio_util::sync::CancellationToken;

        // Exactly three retries at 1s, 2s, 4s — factor two, no jitter, and
        // nothing after the third.
        assert_eq!(
            (0..=super::REWRITE_CONFLICT_RETRIES)
                .map(RewriteConflictSchedule::delay_after)
                .collect::<Vec<_>>(),
            vec![
                Some(Duration::from_secs(1)),
                Some(Duration::from_secs(2)),
                Some(Duration::from_secs(4)),
                None,
            ]
        );
        assert!(
            RewriteConflictSchedule::delay_after(u32::MAX).is_none(),
            "an impossible retry count is exhaustion, never an overflow"
        );
        assert!(
            super::REWRITE_CONFLICT_INITIAL_BACKOFF.saturating_mul(
                super::REWRITE_CONFLICT_BACKOFF_FACTOR.pow(super::REWRITE_CONFLICT_RETRIES - 1)
            ) <= super::REWRITE_CONFLICT_MAX_BACKOFF,
            "the retained ceiling must not silently reshape the schedule"
        );

        let stop = CancellationToken::new();
        let now = chrono::Utc::now();
        let generous = RewritePublicationDeadline::new(now, Duration::from_secs(600))
            .expect("a representable deadline");

        // Each wait consumes exactly its scheduled delay.
        for retries_spent in 0..super::REWRITE_CONFLICT_RETRIES {
            let before = tokio::time::Instant::now();
            RewriteConflictSchedule::wait(retries_spent, generous, now, &stop)
                .await
                .expect("a scheduled retry waits");
            assert_eq!(
                before.elapsed(),
                RewriteConflictSchedule::delay_after(retries_spent).expect("scheduled"),
                "the wait is the schedule's delay, exactly"
            );
        }
        assert_eq!(
            RewriteConflictSchedule::wait(super::REWRITE_CONFLICT_RETRIES, generous, now, &stop)
                .await,
            Err(RewriteRetryStop::Exhausted)
        );

        // A deadline that cannot cover the delay plus an answer truncates
        // before sleeping rather than shortening the wait.
        let tight = RewritePublicationDeadline::new(now, Duration::from_millis(1_000))
            .expect("a representable deadline");
        let before = tokio::time::Instant::now();
        assert_eq!(
            RewriteConflictSchedule::wait(0, tight, now, &stop).await,
            Err(RewriteRetryStop::DeadlineTruncated)
        );
        assert_eq!(
            before.elapsed(),
            Duration::ZERO,
            "truncation does not sleep"
        );
        assert_eq!(
            RewriteConflictSchedule::wait(0, tight, now + chrono::Duration::seconds(5), &stop)
                .await,
            Err(RewriteRetryStop::DeadlineTruncated),
            "an elapsed deadline is also a truncation, never an unbounded wait"
        );

        // Cancellation interrupts the backoff instead of outlasting it.
        let cancelled = CancellationToken::new();
        let before = tokio::time::Instant::now();
        let waiting = tokio::spawn({
            let cancelled = cancelled.clone();
            async move { RewriteConflictSchedule::wait(2, generous, now, &cancelled).await }
        });
        tokio::time::advance(Duration::from_secs(1)).await;
        cancelled.cancel();
        assert_eq!(
            waiting.await.expect("the waiter joins"),
            Err(RewriteRetryStop::Cancelled)
        );
        assert!(
            before.elapsed() < Duration::from_secs(4),
            "cancellation must not wait out the full backoff"
        );

        // Ambiguity never resubmits, at any point in the schedule.
        let authorized = authorized_rewrite_commit_authority();
        for retries_spent in 0..=super::REWRITE_CONFLICT_RETRIES {
            assert_eq!(
                RewriteAcceptance::Ambiguous.next_action(retries_spent, false, authorized.decide()),
                RewriteConflictAction::ReconcileWithoutRecommit,
                "an unknown acceptance leaves the operation Prepared for reconciliation"
            );
        }
    }

    /// The closed matrix always reports its outermost broken dimension.
    ///
    /// The single-break cases prove the mapping; this proves the matrix is
    /// closed. Every one of the 512 combinations must proceed only when
    /// nothing is broken, and must report the outermost broken dimension, so a
    /// later reordering or an added early `Proceed` cannot hide a refusal
    /// behind a dimension that happens to be checked first.
    #[test]
    fn rewrite_commit_decision_matrix_reports_the_outermost_refusal() {
        let authorized = authorized_rewrite_commit_authority();
        let ordered = single_dimension_refusals().map(|(_, refusal)| refusal);
        for (position, refusal) in ordered.iter().enumerate() {
            assert!(
                !ordered[..position].contains(refusal),
                "each refusal reason belongs to exactly one authority dimension"
            );
        }
        for combination in 0_u16..1 << ordered.len() {
            let mut authority = authorized;
            for position in 0..ordered.len() {
                set_authority_dimension(
                    &mut authority,
                    position,
                    combination & (1 << position) == 0,
                );
            }
            let expected = (0..ordered.len())
                .find(|position| combination & (1 << position) != 0)
                .map_or(RewriteCommitDecision::Proceed, |position| {
                    RewriteCommitDecision::Refuse(ordered[position])
                });
            assert_eq!(
                authority.decide(),
                expected,
                "authority combination {combination:#b} decided out of order"
            );
        }
    }

    /// One shared deadline never renews, and never permits a call past itself.
    ///
    /// The boundary is exclusive on both sides for a reason. A call that starts
    /// exactly at the deadline has no budget to wait with, so it must not start
    /// at all; and the remaining budget is always measured from the deadline
    /// rather than from the configured timeout, so a first call that consumed
    /// most of it leaves the retry only what is left.
    #[test]
    fn rewrite_publication_deadline_never_renews_its_budget() {
        let start = chrono::DateTime::from_timestamp(1_700_000_000, 0).expect("a valid instant");
        let budget = std::time::Duration::from_secs(30);
        let deadline =
            RewritePublicationDeadline::new(start, budget).expect("a representable deadline");

        assert!(!deadline.passed(start), "a fresh deadline has not elapsed");
        assert_eq!(deadline.remaining(start), Some(budget));

        // A slow first call shortens the retry rather than restarting the clock.
        let late = start + chrono::Duration::seconds(29);
        assert!(!deadline.passed(late));
        assert_eq!(
            deadline.remaining(late),
            Some(std::time::Duration::from_secs(1)),
            "the retry inherits what the first call left, never a fresh budget"
        );

        // Exactly at the deadline there is nothing to wait with.
        let at = start + chrono::Duration::seconds(30);
        assert!(deadline.passed(at));
        assert_eq!(
            deadline.remaining(at),
            None,
            "a call may not start with zero remaining budget"
        );

        let after = start + chrono::Duration::seconds(31);
        assert!(deadline.passed(after));
        assert_eq!(deadline.remaining(after), None);

        // The deadline is an instant, so re-deriving it from the same instant
        // and budget is the only way to get the same value; nothing about the
        // type can extend one that already exists.
        assert_eq!(
            deadline,
            RewritePublicationDeadline::new(start, budget).expect("a representable deadline"),
        );
        assert!(
            RewritePublicationDeadline::new(start, std::time::Duration::MAX).is_err(),
            "an unrepresentable budget refuses instead of producing an unbounded call"
        );
    }
}
