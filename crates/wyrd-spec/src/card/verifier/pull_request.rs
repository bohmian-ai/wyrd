//! Locked `extract` output of `extractor: pull_request`.
//!
//! A queued `pull_request` execution's extraction step returns one
//! [`PullRequestExtraction`]: the pinned PR, the review comparison Wyrd
//! chose, the active changed files with their exact source, and the
//! independent code-review library's selected context, unchanged. Every
//! field is always present; absent values are `null` only where the type
//! allows it. The output is ordinary step output: Agents receive the whole
//! value or its fields only through the inputs and workspace they declare.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ids::{ExecutionId, RepositoryId};
use crate::reference::CardRef;
use crate::verification::execution::PullRequestRef;

/// The extractor an `extract` action runs instead of reading a media slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Extractor {
    /// The pinned source of the pull request a queued `pull_request`
    /// execution was admitted for.
    PullRequest,
}

/// The output discriminator of a pull-request extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestFormat {
    /// `pull_request`.
    PullRequest,
}

/// The output of `extract` with `extractor: pull_request`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PullRequestExtraction {
    /// Always `pull_request`.
    pub format: PullRequestFormat,
    /// The pinned pull request.
    pub pr: PullRequestMetadata,
    /// How this review compares, and the complete PR comparison.
    pub review: PullRequestReview,
    /// The active comparison only: `review.comparison_base` to the pinned
    /// head.
    pub changes: ChangeSet,
    /// The library's `ReviewContext` for the active comparison, unchanged:
    /// format and extraction versions, identity, PR, base, head and tree,
    /// selected spans with their exact source text, omissions,
    /// `not_analyzed`, budget and coverage. Its own `format_version`
    /// versions its shape.
    pub context: Value,
}

/// The pinned pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PullRequestMetadata {
    /// The Repository Card and provider repository the PR targets.
    pub repository: PullRequestRepository,
    /// Positive PR number.
    pub number: u64,
    /// Browser URL.
    pub url: String,
    /// Title.
    pub title: String,
    /// Description; empty when the author left none.
    pub body: String,
    /// Open or closed when the source was read.
    pub state: PullRequestState,
    /// Draft flag when the source was read.
    pub draft: bool,
    /// Base branch and pinned commit.
    pub base: PullRequestRef,
    /// Head repository, branch and pinned commit, a fork's for a fork PR.
    pub head: PullRequestHead,
    /// Verified merge base of the pinned base and head: the base of
    /// `review.full_pr`.
    pub merge_base: String,
}

/// The repository a pull request targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PullRequestRepository {
    /// The exact Repository Card version whose binding admitted the PR.
    pub card: CardRef,
    /// Canonical provider web host, such as `github.com`.
    pub host: String,
    /// Stable provider repository id.
    pub repository_id: RepositoryId,
}

/// A PR's pinned head.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PullRequestHead {
    /// Stable provider id of the repository holding the head commit.
    pub repository_id: RepositoryId,
    /// Head branch name.
    #[serde(rename = "ref")]
    pub ref_name: String,
    /// Pinned head commit.
    pub sha: String,
}

/// Provider pull-request state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestState {
    /// Open.
    Open,
    /// Closed, merged or not.
    Closed,
}

/// The comparison this review covers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PullRequestReview {
    /// `incremental` from the previous usable review's head, or `full` from
    /// the merge base.
    pub mode: ReviewMode,
    /// The commit `changes` compares from: the previous reviewed head, or
    /// `pr.merge_base`. Never the PR's base branch commit.
    pub comparison_base: String,
    /// The completed review this one continues; `null` for a full review.
    /// Read its report through ordinary execution reads.
    #[schemars(required)]
    pub previous: Option<PreviousReview>,
    /// Why a full review ran; `null` for an incremental one.
    #[schemars(required)]
    pub fallback_reason: Option<FallbackReason>,
    /// The complete merge-base → head comparison, in the same structure as
    /// `changes`; never part of `changes` or `context`.
    pub full_pr: ChangeSet,
}

/// Full or incremental review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewMode {
    /// Merge base → head.
    Full,
    /// Previous reviewed head → head.
    Incremental,
}

/// Why a review compares the whole PR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FallbackReason {
    /// No completed review of this PR under this Verifier exists.
    FirstReview,
    /// The provider cannot find the previous reviewed head or relate it to
    /// the head.
    HistoryUnavailable,
    /// The head no longer descends from the previous reviewed head.
    HistoryRewritten,
    /// The PR's last completed review ran under another Verifier version.
    PolicyChanged,
}

/// A retained earlier execution of the same PR and Verifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreviousReview {
    /// The completed execution.
    pub execution_id: ExecutionId,
    /// The head commit it reviewed.
    pub head: String,
}

/// Every changed file between two pinned commits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangeSet {
    /// Base commit.
    pub base: String,
    /// Head commit.
    pub head: String,
    /// Changed files in provider order; empty when nothing changed, which
    /// is evidence, never a verdict.
    pub files: Vec<ChangedFile>,
}

/// One changed file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangedFile {
    /// Head-side path, or the deleted path.
    pub path: String,
    /// Base-side path of a rename; `null` otherwise.
    #[schemars(required)]
    pub previous_path: Option<String>,
    /// How the file changed.
    pub status: ChangeKind,
    /// Base side; `null` for an added file.
    #[schemars(required)]
    pub before: Option<FileSource>,
    /// Head side; `null` for a deleted file.
    #[schemars(required)]
    pub after: Option<FileSource>,
    /// Unified-diff hunks; empty when the content did not change or a side
    /// is not text.
    pub hunks: Vec<Hunk>,
}

/// How one file changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// Present only at the head, including a copy.
    Added,
    /// Content changed in place.
    Modified,
    /// Present only at the base.
    Deleted,
    /// Moved from `previous_path`, possibly with edits.
    Renamed,
    /// Mode or object type changed.
    TypeChanged,
}

/// One side of a changed file: its exact verified text, or why it has none.
/// Absent text is never represented as empty text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FileSource {
    /// UTF-8 text of the verified blob.
    Text {
        /// Blob id the text hashes to.
        blob: String,
        /// Exact text.
        text: String,
    },
    /// The verified blob holds a NUL or is not UTF-8.
    Binary {
        /// Blob id.
        blob: String,
    },
    /// The blob exceeds the file ceiling and was not fetched.
    Unavailable {
        /// Blob id.
        blob: String,
        /// The ceiling in bytes.
        limit_bytes: u64,
    },
    /// Not reviewable source.
    Excluded {
        /// Object id of the blob or submodule commit.
        blob: String,
        /// Why.
        reason: ExclusionReason,
    },
}

/// Why a file is not reviewable source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExclusionReason {
    /// A symbolic link.
    Symlink,
    /// A submodule commit.
    Submodule,
    /// No supported language routes from the path.
    Unsupported,
    /// A path segment names a vendored or build tree.
    VendoredSegment,
    /// Minified output.
    MinifiedOutput,
    /// Marked `linguist-generated`.
    GeneratedAttribute,
    /// Marked `linguist-vendored`.
    VendoredAttribute,
    /// Larger than the file ceiling.
    Oversize,
}

/// One unified-diff hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Hunk {
    /// First base line; 0 when the hunk adds to an empty file.
    pub old_start: u32,
    /// Base lines covered.
    pub old_lines: u32,
    /// First head line; 0 when the hunk empties the file.
    pub new_start: u32,
    /// Head lines covered.
    pub new_lines: u32,
    /// Hunk text, header included.
    pub patch: String,
}

impl PullRequestExtraction {
    /// The JSON Schema of the output, against which references into a
    /// `steps.<id>.output` are validated.
    #[must_use]
    pub fn schema() -> Value {
        serde_json::to_value(schemars::schema_for!(Self)).unwrap_or(Value::Null)
    }
}

#[cfg(test)]
mod tests {
    //! The locked output shape.

    use serde_json::json;

    use super::*;

    /// A full first review with a rename, a binary side and an excluded
    /// file round-trips unchanged, and its schema requires every field.
    ///
    /// # Panics
    /// Panics when the example does not round-trip or a field is optional.
    #[test]
    fn pull_request_extraction_round_trips_and_requires_every_field() {
        let changes = json!({
            "base": "m", "head": "h",
            "files": [
                {"path": "src/b.rs", "previous_path": "src/a.rs", "status": "renamed",
                 "before": {"kind": "text", "blob": "1", "text": "fn a() {}\n"},
                 "after": {"kind": "text", "blob": "2", "text": "fn b() {}\n"},
                 "hunks": [{"old_start": 1, "old_lines": 1, "new_start": 1, "new_lines": 1,
                            "patch": "@@ -1 +1 @@\n-fn a() {}\n+fn b() {}"}]},
                {"path": "logo.png", "previous_path": null, "status": "added", "before": null,
                 "after": {"kind": "binary", "blob": "3"}, "hunks": []},
                {"path": "big.json", "previous_path": null, "status": "modified",
                 "before": {"kind": "unavailable", "blob": "4", "limit_bytes": 1024},
                 "after": {"kind": "excluded", "blob": "5", "reason": "oversize"}, "hunks": []},
            ],
        });
        let example = json!({
            "format": "pull_request",
            "pr": {
                "repository": {
                    "card": {"kind": "Repository", "name": "acme-app", "version": "1.0.0"},
                    "host": "github.com", "repository_id": "4242",
                },
                "number": 7, "url": "https://github.com/acme/app/pull/7", "title": "T", "body": "",
                "state": "open", "draft": false,
                "base": {"ref": "main", "sha": "b"},
                "head": {"repository_id": "9", "ref": "feature", "sha": "h"},
                "merge_base": "m",
            },
            "review": {
                "mode": "full", "comparison_base": "m", "previous": null,
                "fallback_reason": "first_review", "full_pr": changes,
            },
            "changes": changes,
            "context": {"format_version": 1, "spans": []},
        });
        let extraction: PullRequestExtraction =
            serde_json::from_value(example.clone()).expect("the example decodes");
        assert_eq!(
            serde_json::to_value(&extraction).expect("encodes"),
            example
        );
        let schema = PullRequestExtraction::schema();
        let required = |pointer: &str| -> Vec<String> {
            serde_json::from_value(schema.pointer(pointer).cloned().expect(pointer))
                .expect("required names")
        };
        assert_eq!(
            required("/required"),
            ["format", "pr", "review", "changes", "context"]
        );
        assert_eq!(
            required("/$defs/PullRequestReview/required"),
            [
                "mode",
                "comparison_base",
                "previous",
                "fallback_reason",
                "full_pr"
            ]
        );
        let mut unknown = example;
        unknown["changes"]["extra"] = json!(true);
        assert!(serde_json::from_value::<PullRequestExtraction>(unknown).is_err());
    }
}
