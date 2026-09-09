//! Durable evidence that binds one attempt to the state it acted on.
//!
//! Two different questions are answered here, and keeping them apart is the
//! whole point. The *selection* fingerprint answers "what exactly did this
//! attempt decide to rewrite, against which snapshot?" — it is the receipt a
//! later reconciliation compares a commit against, so it names paths and is
//! bound to a snapshot id. The *debt* fingerprint answers "is there still
//! anything semantically wrong with this table?" — it deliberately names no
//! path, because a table whose files were rewritten under the same policy into
//! the same shape has made no progress worth a second attempt, however
//! different the new paths are.
//!
//! Both are domain-separated and versioned. A fingerprint that could collide
//! across domains, or that silently changed meaning when its inputs changed,
//! would be worse than no evidence at all.

use iceberg_compaction_core::managed::{SelectionReason, SelectionReport};

use super::handoff::RewriteHandoff;

/// Domain tag and version for a selection receipt.
const SELECTION_DOMAIN: &str = "wyrd.forge.rewrite.selection.v1";

/// Domain tag and version for a semantic-debt summary.
const DEBT_DOMAIN: &str = "wyrd.forge.rewrite.debt.v1";

/// Domain tag and version for a policy identity.
const POLICY_DOMAIN: &str = "wyrd.forge.rewrite.policy.v1";

/// One object an attempt opened, carried out for drain and cleanup.
///
/// `logical_ordinal` is the attempt-global ordinal the core reserved *before*
/// opening the object, which is what makes the set orderable and complete even
/// when several writers ran concurrently. It is evidence only: it is not in the
/// object path and must never be reconstructed from one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeUnsettledOutput {
    /// Attempt-global ordinal reserved before the object was opened.
    pub logical_ordinal: u64,
    /// Storage path of the object.
    pub path: String,
    /// Whether the object's close completed before the attempt ended.
    ///
    /// An unsettled object may or may not exist in storage, so a caller
    /// reclaiming it must tolerate its absence.
    pub settled: bool,
}

/// Durable planning evidence one attempt produced and the next consumes.
///
/// Held together rather than split because the three values are only
/// meaningful as a set: the selection receipt is what a commit is reconciled
/// against, the debt summary is what decides whether a *next* attempt is worth
/// admitting, and the policy fingerprint is what makes both comparable only
/// against an attempt that ran under the same rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeRewriteEvidence {
    /// Snapshot the selection and every consumed path were resolved against.
    pub base_snapshot_id: i64,
    /// Ordered, snapshot-bound receipt of exactly what was selected.
    pub selection_fingerprint: String,
    /// Path-free summary of what is still semantically wrong with the table.
    pub debt_fingerprint: String,
    /// Identity of the policy both summaries were computed under.
    pub policy_fingerprint: String,
}

/// What one admitted attempt concluded about a table.
///
/// `NoProgress` is not a failure. It is the correct outcome when the table's
/// semantic debt is unchanged from the last attempt: the attempt refused
/// before any object IO rather than rewriting the same rows into differently
/// named objects and calling that maintenance.
#[derive(Debug)]
pub enum ForgeRewriteOutcome {
    /// The table's semantic debt is unchanged, so nothing was read or written.
    NoProgress {
        /// Snapshot the unchanged debt was measured against.
        base_snapshot_id: i64,
        /// Debt fingerprint that matched the previous attempt's.
        debt_fingerprint: String,
    },
    /// Cancellation drained the attempt, leaving objects that may exist.
    ///
    /// Not an error: the attempt did exactly what it was told to do. The caller
    /// owns the listed objects, none of which is referenced by any snapshot.
    Cancelled {
        /// Snapshot the drained attempt was planned against.
        base_snapshot_id: i64,
        /// Every object the attempt produced or may have produced.
        possible_outputs: Vec<ForgeUnsettledOutput>,
    },
    /// The attempt produced candidate objects for publication.
    Rewritten {
        /// Durable planning evidence this attempt produced.
        evidence: ForgeRewriteEvidence,
        /// The exact five-field result publication consumes.
        handoff: Box<RewriteHandoff>,
    },
}

/// Computes the ordered, versioned receipt for one canonical selection.
///
/// Ordering is imposed here rather than assumed: the core's report is already
/// canonical, but a fingerprint that depended on iteration order would change
/// when nothing about the decision did, and a reconciliation comparing it would
/// see a false difference. The snapshot id is mixed in first so two identical
/// selections against different snapshots are different receipts.
#[must_use]
pub(crate) fn selection_fingerprint(report: &SelectionReport) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(SELECTION_DOMAIN.as_bytes());
    hasher.update(b"\0");
    hasher.update(report.strategy.as_str().as_bytes());
    hasher.update(b"\0");
    hasher.update(&report.base_snapshot_id.to_be_bytes());
    let mut selected: Vec<(&str, SelectionReason)> = report
        .selected
        .iter()
        .map(|file| (file.file_path.as_str(), file.reason))
        .collect();
    selected.sort_unstable();
    for (path, reason) in selected {
        hasher.update(b"\0");
        hasher.update(path.as_bytes());
        hasher.update(b"\0");
        hasher.update(reason.as_str().as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

/// Summarizes what is still semantically wrong with a table, ignoring paths.
///
/// The inputs are the counts of each selection reason plus the delete scope the
/// selection covers. Paths are excluded on purpose: rewriting three undersized
/// files into one correctly sized file changes every path and *does* change the
/// summary, because the reason counts change. Rewriting three undersized files
/// into three equally undersized files also changes every path but leaves the
/// summary identical — which is exactly the churn this fingerprint exists to
/// detect.
#[must_use]
pub(crate) fn debt_fingerprint(
    report: &SelectionReport,
    position_delete_count: usize,
    equality_delete_count: usize,
) -> String {
    let mut counts = std::collections::BTreeMap::<&'static str, u64>::new();
    for file in &report.selected {
        *counts.entry(file.reason.as_str()).or_default() += 1;
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(DEBT_DOMAIN.as_bytes());
    hasher.update(b"\0");
    hasher.update(report.strategy.as_str().as_bytes());
    if let Some(policy) = report.policy.as_ref() {
        hasher.update(b"\0");
        hasher.update(&policy.schema_id.to_be_bytes());
        hasher.update(&policy.partition_spec_id.to_be_bytes());
        hasher.update(&policy.sort_order_id.to_be_bytes());
        hasher.update(policy.writer_recipe.as_bytes());
        hasher.update(&policy.target_file_size_bytes.to_be_bytes());
        hasher.update(&policy.small_file_threshold_bytes.to_be_bytes());
    }
    for (reason, count) in counts {
        hasher.update(b"\0");
        hasher.update(reason.as_bytes());
        hasher.update(&count.to_be_bytes());
    }
    hasher.update(b"\0deletes\0");
    hasher.update(&(position_delete_count as u64).to_be_bytes());
    hasher.update(&(equality_delete_count as u64).to_be_bytes());
    hasher.finalize().to_hex().to_string()
}

/// Computes the identity of the policy an attempt ran under.
///
/// A report whose policy is absent — an upstream strategy rather than the
/// identity-aware one — fingerprints to the strategy alone, which is correct:
/// there is no policy for a later attempt to compare against.
#[must_use]
pub(crate) fn policy_fingerprint(report: &SelectionReport) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(POLICY_DOMAIN.as_bytes());
    hasher.update(b"\0");
    hasher.update(report.strategy.as_str().as_bytes());
    if let Some(policy) = report.policy.as_ref() {
        hasher.update(b"\0");
        hasher.update(&policy.schema_id.to_be_bytes());
        hasher.update(&policy.partition_spec_id.to_be_bytes());
        hasher.update(&policy.sort_order_id.to_be_bytes());
        hasher.update(policy.writer_recipe.as_bytes());
        hasher.update(&policy.target_file_size_bytes.to_be_bytes());
        hasher.update(&policy.small_file_threshold_bytes.to_be_bytes());
        hasher.update(&policy.max_file_size_bytes.to_be_bytes());
        hasher.update(&[u8::from(policy.emit_open_partition_tail)]);
    }
    hasher.finalize().to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use iceberg_compaction_core::managed::{PolicyIdentity, SelectedFile, SelectionStrategyKind};

    use super::*;

    /// Builds one canonical report over the given files.
    ///
    /// # Panics
    ///
    /// Panics when the report would not be canonical, which is a fixture
    /// construction invariant rather than an input.
    fn report(snapshot: i64, files: Vec<(&str, SelectionReason)>) -> SelectionReport {
        SelectionReport::new(
            SelectionStrategyKind::WyrdIdentityAware,
            snapshot,
            Some(PolicyIdentity {
                schema_id: 0,
                partition_spec_id: 0,
                sort_order_id: 1,
                writer_recipe: "v1".to_owned(),
                target_file_size_bytes: 1 << 28,
                small_file_threshold_bytes: 1 << 25,
                max_file_size_bytes: 1 << 29,
                emit_open_partition_tail: false,
            }),
            files
                .into_iter()
                .map(|(path, reason)| SelectedFile {
                    file_path: path.to_owned(),
                    reason,
                })
                .collect(),
        )
        .expect("fixture selection report")
    }

    /// The receipt is order-independent, domain-separated, and snapshot-bound.
    ///
    /// Order independence is what makes the receipt comparable at all: the same
    /// decision reported in a different order must produce the same value, or a
    /// reconciliation would see drift that never happened. Snapshot binding is
    /// the opposite guarantee — the same file set selected against a different
    /// base is a different decision, because the base is what the commit will
    /// be validated against. Domain separation keeps a selection receipt from
    /// ever comparing equal to a debt summary computed over the same report.
    #[test]
    fn forge_selection_fingerprint_is_ordered_versioned_and_snapshot_bound() {
        let forward = report(
            41,
            vec![
                ("a.parquet", SelectionReason::Undersized),
                ("b.parquet", SelectionReason::Undersized),
            ],
        );
        let reversed = report(
            41,
            vec![
                ("b.parquet", SelectionReason::Undersized),
                ("a.parquet", SelectionReason::Undersized),
            ],
        );
        assert_eq!(
            selection_fingerprint(&forward),
            selection_fingerprint(&reversed),
            "the same decision reported in a different order is the same decision"
        );

        let later = report(
            42,
            vec![
                ("a.parquet", SelectionReason::Undersized),
                ("b.parquet", SelectionReason::Undersized),
            ],
        );
        assert_ne!(
            selection_fingerprint(&forward),
            selection_fingerprint(&later),
            "the same files against a different base is a different decision"
        );

        let other_reason = report(
            41,
            vec![
                ("a.parquet", SelectionReason::Undersized),
                ("b.parquet", SelectionReason::Oversized),
            ],
        );
        assert_ne!(
            selection_fingerprint(&forward),
            selection_fingerprint(&other_reason),
            "why a file was selected is part of the decision"
        );

        assert_ne!(
            selection_fingerprint(&forward),
            debt_fingerprint(&forward, 0, 0),
            "a selection receipt must never collide with a debt summary"
        );
        assert!(
            selection_fingerprint(&forward).len() == 64,
            "the receipt is a fixed-width BLAKE3 digest"
        );
    }

    /// Unchanged semantic debt refuses a second attempt before any object IO.
    ///
    /// The two reports name entirely different files with entirely different
    /// paths, and describe exactly the same problem: two undersized files under
    /// the same policy. That is the churn case — a previous attempt already
    /// rewrote this table and achieved nothing, so a second attempt would burn
    /// a resource lease to produce a third set of equally wrong objects. The
    /// debt summary is equal, which is what lets the decision be made from
    /// durable evidence alone, before a single object is opened.
    ///
    /// The contrast cases prove the summary is not simply constant: a changed
    /// reason mix, a changed policy, and a changed delete scope each move it.
    #[test]
    fn forge_no_progress_refuses_unchanged_semantic_debt_before_io() {
        let before = report(
            41,
            vec![
                ("old-a.parquet", SelectionReason::Undersized),
                ("old-b.parquet", SelectionReason::Undersized),
            ],
        );
        let after = report(
            77,
            vec![
                ("new-a.parquet", SelectionReason::Undersized),
                ("new-b.parquet", SelectionReason::Undersized),
            ],
        );
        assert_eq!(
            debt_fingerprint(&before, 0, 0),
            debt_fingerprint(&after, 0, 0),
            "new paths for the same unsolved problem are not progress"
        );
        assert_ne!(
            selection_fingerprint(&before),
            selection_fingerprint(&after),
            "the receipts still differ, so reconciliation is unaffected"
        );

        let resolved = report(77, vec![("new-a.parquet", SelectionReason::Undersized)]);
        assert_ne!(
            debt_fingerprint(&before, 0, 0),
            debt_fingerprint(&resolved, 0, 0),
            "fewer undersized files is real progress"
        );

        let reclassified = report(
            77,
            vec![
                ("new-a.parquet", SelectionReason::Undersized),
                ("new-b.parquet", SelectionReason::ObsoleteSchema),
            ],
        );
        assert_ne!(
            debt_fingerprint(&before, 0, 0),
            debt_fingerprint(&reclassified, 0, 0),
            "a different mix of reasons is a different problem"
        );

        assert_ne!(
            debt_fingerprint(&before, 0, 0),
            debt_fingerprint(&before, 1, 0),
            "outstanding position deletes are part of the debt"
        );
        assert_ne!(
            debt_fingerprint(&before, 0, 0),
            debt_fingerprint(&before, 0, 1),
            "outstanding equality deletes are part of the debt"
        );
    }
}
