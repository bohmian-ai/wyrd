//! What one pull_request execution shows on GitHub: its Check and its one
//! `COMMENT` review, rendered from retained evidence only.
//!
//! [`Presentation::render`] is pure. It takes the execution's status and
//! verdict, the report its accepted outputs hold and the extraction that
//! pinned its source, and returns the GitHub request bodies delivery sends.
//! Every report anchor is checked against that retained source: either side
//! of a changed file, or a selected context span. A finding located inside
//! a head-side diff hunk becomes an inline comment, any other valid finding
//! stays in the Check summary, and an anchor the source does not hold is an
//! [`AnchorProblem`] shown in the summary, never moved to another line.
//! Without a retained extraction the report still renders in the summary and
//! every anchor is refused as unavailable. Severity
//! and confidence are shown with each finding and never decide the Check
//! conclusion, which follows the verdict alone.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use wyrd_spec::ids::ExecutionId;
use wyrd_spec::verification::execution::{
    AnchorProblem, AnchorProblemReason, GithubDeliveryReason,
};
use wyrd_spec::verification::report::{
    CodeReviewReport, Finding, ReportSelection, ReportSource, SourceAnchor,
};
use wyrd_spec::verification::{VerificationExecutionStatus, VerificationVerdict};

use super::extraction::output::{ChangeSet, FileSource, Hunk, PullRequestExtraction};

/// GitHub's limit on a Check run output summary, in characters.
const SUMMARY_LIMIT: usize = 65_535;

/// The marker a review body carries so delivery can find the review of one
/// execution again.
#[must_use]
pub(crate) fn review_marker(execution_id: ExecutionId) -> String {
    format!("<!-- wyrd:execution:{execution_id} -->")
}

/// A Check run create or update body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct CheckRun {
    /// Stable per Repository/Verifier binding.
    pub(crate) name: String,
    /// The reviewed commit.
    pub(crate) head_sha: String,
    /// The exact execution.
    pub(crate) external_id: String,
    /// `in_progress` or `completed`.
    pub(crate) status: &'static str,
    /// Present exactly when `status` is `completed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) conclusion: Option<&'static str>,
    /// Title and summary.
    pub(crate) output: CheckOutput,
}

/// A Check run's visible output. It carries no annotations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct CheckOutput {
    /// One line.
    pub(crate) title: String,
    /// Markdown, at most [`SUMMARY_LIMIT`] characters.
    pub(crate) summary: String,
}

/// A pull request review create body; always a `COMMENT`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Review {
    /// The reviewed commit.
    pub(crate) commit_id: String,
    /// Starts with [`review_marker`].
    pub(crate) body: String,
    /// Always `COMMENT`; never `APPROVE` or `REQUEST_CHANGES`.
    pub(crate) event: &'static str,
    /// One inline comment per finding anchored inside the PR diff.
    pub(crate) comments: Vec<ReviewComment>,
}

/// One inline review comment on the head side of the diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ReviewComment {
    /// Repository-relative path.
    pub(crate) path: String,
    /// First line of a multi-line comment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) start_line: Option<u32>,
    /// Head side of `start_line`, present with it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) start_side: Option<&'static str>,
    /// Last line.
    pub(crate) line: u32,
    /// Always `RIGHT`, the head side.
    pub(crate) side: &'static str,
    /// Markdown, a suggestion block included when the finding has one.
    pub(crate) body: String,
}

/// What one execution shows on GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Presentation {
    /// The Check.
    pub(crate) check: CheckRun,
    /// The review, once the execution completed with a rendered report,
    /// with or without inline comments.
    pub(crate) review: Option<Review>,
    /// Anchors refused because the retained source does not hold them.
    pub(crate) problems: Vec<AnchorProblem>,
    /// Why the selected report is not shown, when it is ambiguous or
    /// invalid.
    pub(crate) report_error: Option<GithubDeliveryReason>,
}

/// One execution's state as delivery presents it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Settled {
    /// Its status.
    pub(crate) status: VerificationExecutionStatus,
    /// Its verdict, once completed.
    pub(crate) verdict: Option<VerificationVerdict>,
}

impl Settled {
    /// The Check conclusion, or `None` while the execution is unfinished.
    #[must_use]
    pub(crate) const fn conclusion(self) -> Option<&'static str> {
        use VerificationExecutionStatus as Status;
        match (self.status, self.verdict) {
            (Status::Completed, Some(VerificationVerdict::Passed)) => Some("success"),
            (Status::Completed, Some(VerificationVerdict::Inconclusive)) => Some("action_required"),
            (Status::Completed | Status::Errored, _) => Some("failure"),
            (Status::Cancelled, _) => Some("cancelled"),
            (Status::TimedOut, _) => Some("timed_out"),
            (Status::Pending | Status::Running | Status::Retrying | Status::Waiting, _) => None,
        }
    }
}

/// A completed execution's report selection and the extraction that pinned
/// the source it reviewed.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Completed<'a> {
    /// The report its accepted outputs hold.
    pub(crate) selection: &'a ReportSelection,
    /// The extraction among its step results; `None` when no step extracted
    /// the PR, so no anchor can be checked.
    pub(crate) extraction: Option<&'a PullRequestExtraction>,
}

/// Where a valid anchor lands.
enum Placement {
    /// Inside one diff hunk of the head side: an inline comment.
    Inline,
    /// In retained source but outside the head-side diff: summary only.
    Summary,
}

/// One retained source range an anchor may lie in.
struct Retained<'a> {
    /// The commit the range is read at.
    commit: &'a str,
    /// Its repository-relative path at that commit.
    path: &'a str,
    /// Its first and last one-based lines, or `None` when the side holds no
    /// text.
    lines: Option<(u32, u32)>,
    /// The head-side hunks where an anchor becomes inline; empty on every
    /// other range.
    hunks: &'a [Hunk],
}

/// The parts of the code-review library's `ReviewContext` an anchor is
/// checked against: the head its spans were read at and each span's lines.
#[derive(Deserialize)]
struct Context {
    /// The pinned head commit.
    head: String,
    /// The selected spans.
    spans: Vec<Span>,
}

/// One selected context span.
#[derive(Deserialize)]
struct Span {
    /// Repository-relative path.
    path: String,
    /// First one-based line.
    start: u32,
    /// Last one-based line, inclusive.
    end: u32,
}

impl Presentation {
    /// Render execution `execution_id` of the binding named `name`, which
    /// reviews commit `head`, in state `settled`; `completed` is present once
    /// it completed.
    ///
    /// A waiting execution's Check is `in_progress` and explains `waiting`.
    /// Only a completed execution renders its report.
    #[must_use]
    pub(crate) fn render(
        name: &str,
        execution_id: ExecutionId,
        head: &str,
        settled: Settled,
        waiting: &str,
        completed: Option<Completed<'_>>,
    ) -> Self {
        let head = head.to_owned();
        let conclusion = settled.conclusion();
        let mut summary = String::new();
        let mut comments = Vec::new();
        let mut problems = Vec::new();
        let title = match (conclusion, settled.verdict) {
            (None, _) if settled.status == VerificationExecutionStatus::Waiting => {
                summary.push_str(waiting);
                "Waiting".to_owned()
            }
            (None, _) => "Running".to_owned(),
            (Some(_), Some(verdict)) => format!("Verdict: {verdict}"),
            (Some(_), None) => format!("Execution {}", label(&settled.status)),
        };
        let completed = completed.filter(|_| {
            conclusion.is_some() && settled.status == VerificationExecutionStatus::Completed
        });
        let report_error = completed.and_then(|completed| {
            Self::report(completed, &mut summary, &mut comments, &mut problems)
        });
        let rendered = completed
            .is_some_and(|completed| matches!(completed.selection, ReportSelection::One { .. }));
        let review = rendered.then(|| Review {
            commit_id: head.clone(),
            body: format!(
                "{}\nWyrd review of execution `{execution_id}`.",
                review_marker(execution_id)
            ),
            event: "COMMENT",
            comments,
        });
        Self {
            check: CheckRun {
                name: name.to_owned(),
                head_sha: head,
                external_id: execution_id.to_string(),
                status: if conclusion.is_some() {
                    "completed"
                } else {
                    "in_progress"
                },
                conclusion,
                output: CheckOutput {
                    title,
                    summary: bounded(summary),
                },
            },
            review,
            problems,
            report_error,
        }
    }

    /// Append the report section of a completed execution and its inline
    /// comments and refused anchors; returns why its report is not shown,
    /// when it is ambiguous or invalid.
    fn report(
        Completed {
            selection,
            extraction,
        }: Completed<'_>,
        summary: &mut String,
        comments: &mut Vec<ReviewComment>,
        problems: &mut Vec<AnchorProblem>,
    ) -> Option<GithubDeliveryReason> {
        let report = match selection {
            ReportSelection::None => return None,
            ReportSelection::Ambiguous { sources } => {
                let names: Vec<String> = sources.iter().map(source_name).collect();
                let _ = writeln!(
                    summary,
                    "**Report not shown:** more than one step returned a `code_review` \
                     report ({}).",
                    names.join(", ")
                );
                return Some(GithubDeliveryReason::ReportAmbiguous);
            }
            ReportSelection::Invalid { source, problem } => {
                let _ = writeln!(
                    summary,
                    "**Report not shown:** the `code_review` report of {} is unusable \
                     (`{}`).",
                    source_name(source),
                    problem.reason()
                );
                return Some(GithubDeliveryReason::ReportInvalid);
            }
            ReportSelection::One { report, .. } => report,
        };
        summary.push_str(&report.summary);
        summary.push('\n');
        if !report.coverage.complete {
            summary.push_str("\n**Coverage incomplete.**\n");
            for limitation in &report.coverage.limitations {
                let _ = writeln!(summary, "- {limitation}");
            }
        }
        render_findings(report, extraction, summary, comments, problems);
        None
    }
}

/// Render each finding: one summary line, and an inline comment when its
/// anchor lies inside the head-side diff. Without `extraction` every anchor
/// is refused as unavailable.
fn render_findings(
    report: &CodeReviewReport,
    extraction: Option<&PullRequestExtraction>,
    summary: &mut String,
    comments: &mut Vec<ReviewComment>,
    problems: &mut Vec<AnchorProblem>,
) {
    if report.findings.is_empty() {
        return;
    }
    let source = extraction.map(Source::new);
    let source = source.as_ref();
    summary.push_str("\n### Findings\n");
    for finding in &report.findings {
        let refused = finding
            .anchors()
            .find_map(|anchor| place(anchor, source).err());
        if let Some(reason) = refused {
            problems.push(AnchorProblem {
                finding: finding.id.clone(),
                reason,
            });
            let _ = writeln!(
                summary,
                "- {} — not shown inline: `{}`",
                heading(finding),
                reason.as_str()
            );
            continue;
        }
        let edit = finding
            .remediation
            .as_ref()
            .and_then(|remediation| remediation.suggested_edit.as_ref());
        let anchor = edit
            .map(|edit| &edit.location)
            .or_else(|| finding.locations.first());
        let inline = anchor.filter(|anchor| matches!(place(anchor, source), Ok(Placement::Inline)));
        if let Some(anchor) = inline {
            let _ = writeln!(summary, "- {} — {}", heading(finding), at(anchor));
            comments.push(comment(
                finding,
                anchor,
                edit.filter(|edit| &edit.location == anchor)
                    .map(|edit| edit.replacement.as_str()),
            ));
        } else {
            let place = anchor.map_or_else(String::new, |anchor| format!(" — {}", at(anchor)));
            let _ = writeln!(
                summary,
                "- {}{place}\n  {}",
                heading(finding),
                finding.explanation
            );
        }
    }
}

/// The source a report's anchors are checked against: the extraction and
/// its selected context, read once per report.
struct Source<'a> {
    /// The extraction among the step results.
    extraction: &'a PullRequestExtraction,
    /// Its selected context; `None` when the context does not decode.
    context: Option<Context>,
}

impl<'a> Source<'a> {
    /// The source `extraction` retains.
    fn new(extraction: &'a PullRequestExtraction) -> Self {
        Self {
            extraction,
            context: Context::deserialize(&extraction.context).ok(),
        }
    }

    /// Every source range retained: each side of every file of the full and
    /// the active comparison, a renamed file's base side under its previous
    /// path, and each selected context span. Only the full comparison's head
    /// side carries the hunks inline comments land in.
    fn retained(&self) -> impl Iterator<Item = Retained<'_>> {
        let review = &self.extraction.review;
        let spans = self.context.iter().flat_map(|context| {
            context.spans.iter().map(|span| Retained {
                commit: &context.head,
                path: &span.path,
                lines: Some((span.start, span.end)),
                hunks: &[],
            })
        });
        sides(&review.full_pr, true)
            .chain(sides(&self.extraction.changes, false))
            .chain(spans)
    }

    /// Where `anchor` lands in the retained source.
    ///
    /// # Errors
    /// Returns the most specific [`AnchorProblemReason`] across the
    /// retained ranges when none holds it.
    fn place(&self, anchor: &SourceAnchor) -> Result<Placement, AnchorProblemReason> {
        let (start, end) = (anchor.start_line.get(), anchor.end_line.get());
        let mut refused = AnchorProblemReason::AnchorCommitMismatch;
        let mut placement = None;
        for held in self.retained() {
            if held.commit != anchor.commit {
                continue;
            }
            let reason = match held.lines {
                _ if held.path != anchor.path => AnchorProblemReason::AnchorPathUnknown,
                None => AnchorProblemReason::AnchorSourceUnavailable,
                Some((first, last)) if start < first || end > last => {
                    AnchorProblemReason::AnchorOutOfRange
                }
                Some(_) => {
                    let inline = held.hunks.iter().any(|hunk| {
                        hunk.new_lines > 0
                            && start >= hunk.new_start
                            && end < hunk.new_start.saturating_add(hunk.new_lines)
                    });
                    if inline {
                        return Ok(Placement::Inline);
                    }
                    placement = Some(Placement::Summary);
                    continue;
                }
            };
            // Variants are declared least to most specific.
            refused = std::cmp::max_by_key(refused, reason, |reason| *reason as u8);
        }
        placement.ok_or(refused)
    }
}

/// Where `anchor` lands in `source`; without one, every anchor is refused
/// as unavailable.
///
/// # Errors
/// Returns the [`AnchorProblemReason`] when the retained source does not
/// hold it.
fn place(
    anchor: &SourceAnchor,
    source: Option<&Source<'_>>,
) -> Result<Placement, AnchorProblemReason> {
    source.map_or(
        Err(AnchorProblemReason::AnchorSourceUnavailable),
        |source| source.place(anchor),
    )
}

/// Both sides of every file of `set`, the head side with its hunks when
/// `full`.
fn sides(set: &ChangeSet, full: bool) -> impl Iterator<Item = Retained<'_>> {
    set.files.iter().flat_map(move |file| {
        [
            Retained {
                commit: &set.base,
                path: file.previous_path.as_deref().unwrap_or(&file.path),
                lines: file.before.as_ref().and_then(lines),
                hunks: &[],
            },
            Retained {
                commit: &set.head,
                path: &file.path,
                lines: file.after.as_ref().and_then(lines),
                hunks: if full { &file.hunks } else { &[] },
            },
        ]
    })
}

/// The line range of `source`'s text, or `None` when it holds none.
fn lines(source: &FileSource) -> Option<(u32, u32)> {
    let FileSource::Text { text, .. } = source else {
        return None;
    };
    Some((1, u32::try_from(text.lines().count()).unwrap_or(u32::MAX)))
}

/// The inline comment of `finding` at `anchor`, with `replacement` as a
/// suggestion block when it replaces exactly that range.
fn comment(finding: &Finding, anchor: &SourceAnchor, replacement: Option<&str>) -> ReviewComment {
    let mut body = format!("**{}**\n\n{}\n", heading(finding), finding.explanation);
    if let Some(remediation) = &finding.remediation {
        let _ = write!(body, "\n{}\n", remediation.explanation);
    }
    if let Some(replacement) = replacement {
        let fence = "`".repeat(longest_backtick_run(replacement).max(2) + 1);
        let replacement = replacement.strip_suffix('\n').unwrap_or(replacement);
        let _ = write!(body, "\n{fence}suggestion\n{replacement}\n{fence}\n");
    }
    let (start, end) = (anchor.start_line.get(), anchor.end_line.get());
    ReviewComment {
        path: anchor.path.clone(),
        start_line: (start < end).then_some(start),
        start_side: (start < end).then_some("RIGHT"),
        line: end,
        side: "RIGHT",
        body,
    }
}

/// `[severity/confidence] title (id)`.
fn heading(finding: &Finding) -> String {
    format!(
        "[{}/{} confidence] {} (`{}`)",
        label(&finding.severity),
        label(&finding.confidence),
        finding.title,
        finding.id
    )
}

/// The snake_case name of a report enum value.
fn label<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// `path:start-end`.
fn at(anchor: &SourceAnchor) -> String {
    format!(
        "`{}:{}-{}`",
        anchor.path, anchor.start_line, anchor.end_line
    )
}

/// `step` or `step[item]`.
fn source_name(source: &ReportSource) -> String {
    match source.item {
        Some(item) => format!("`{}[{item}]`", source.step.as_str()),
        None => format!("`{}`", source.step.as_str()),
    }
}

/// The longest run of backticks in `text`.
fn longest_backtick_run(text: &str) -> usize {
    text.split(|ch| ch != '`').map(str::len).max().unwrap_or(0)
}

/// `summary` cut to GitHub's limit, saying so when cut; the conclusion
/// stays the verdict's, so a cut never reads as a clean review.
fn bounded(summary: String) -> String {
    const NOTE: &str = "\n\n… summary truncated; read the full report in Wyrd.";
    if summary.chars().count() <= SUMMARY_LIMIT {
        return summary;
    }
    let keep = SUMMARY_LIMIT - NOTE.chars().count();
    let mut cut: String = summary.chars().take(keep).collect();
    cut.push_str(NOTE);
    cut
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const HEAD: &str = "1111111111111111111111111111111111111111";

    /// The merge base the extraction compares from.
    const BASE: &str = "3333333333333333333333333333333333333333";

    /// An extraction whose head adds `src/lib.rs` lines 1–4, with lines
    /// 2–3 inside the diff, holds binary `logo.png`, renames `old.rs` to
    /// `new.rs`, and selects lines 10–12 of unchanged `src/util.rs` as
    /// context.
    ///
    /// # Panics
    /// Panics when the fixture does not decode as an extraction.
    fn extraction() -> PullRequestExtraction {
        let files = json!([
            {"path": "src/lib.rs", "previous_path": null, "status": "modified",
             "before": {"kind": "text", "blob": "a", "text": "a\nb\n"},
             "after": {"kind": "text", "blob": "b", "text": "a\nx\ny\nb\n"},
             "hunks": [{"old_start": 1, "old_lines": 2, "new_start": 2, "new_lines": 2,
                        "patch": "@@ -1,2 +2,2 @@"}]},
            {"path": "logo.png", "previous_path": null, "status": "added", "before": null,
             "after": {"kind": "binary", "blob": "c"}, "hunks": []},
            {"path": "new.rs", "previous_path": "old.rs", "status": "renamed",
             "before": {"kind": "text", "blob": "d", "text": "p\nq\n"},
             "after": {"kind": "text", "blob": "d", "text": "p\nq\n"}, "hunks": []}
        ]);
        let changes = json!({"base": BASE, "head": HEAD, "files": files});
        serde_json::from_value(json!({
            "format": "pull_request",
            "pr": {
                "repository": {
                    "card": {"kind": "Repository", "name": "app", "version": "1.0.0"},
                    "host": "github.com", "repository_id": "4242",
                },
                "number": 7, "url": "u", "title": "T", "body": "", "state": "open",
                "draft": false, "base": {"ref": "main", "sha": "b"},
                "head": {"repository_id": "4242", "ref": "f", "sha": HEAD},
                "merge_base": "m",
            },
            "review": {"mode": "full", "comparison_base": "m", "previous": null,
                       "fallback_reason": "first_review", "full_pr": changes},
            "changes": changes,
            "context": {"head": HEAD, "spans": [
                {"path": "src/util.rs", "start": 10, "end": 12, "text": "u\nv\nw\n"}]},
        }))
        .expect("extraction decodes")
    }

    /// An anchor in `path` at `start..=end` on the head.
    fn anchor(path: &str, start: u32, end: u32) -> Value {
        json!({"path": path, "commit": HEAD, "start_line": start, "end_line": end})
    }

    /// An anchor in `path` at `start..=end` on the merge base.
    fn base(path: &str, start: u32, end: u32) -> Value {
        json!({"path": path, "commit": BASE, "start_line": start, "end_line": end})
    }

    /// A finding `id` of `severity` at `locations`, with an optional edit.
    fn finding(id: &str, severity: &str, locations: Value, edit: Option<Value>) -> Value {
        let remediation =
            edit.map(|edit| json!({"explanation": "Fix it.", "suggested_edit": edit}));
        json!({"id": id, "severity": severity, "confidence": "high", "title": id,
               "explanation": "Why.", "locations": locations, "evidence": [],
               "remediation": remediation})
    }

    /// Render a completed execution with `verdict` and `selection` against
    /// `extraction`, or without one.
    fn shown(
        verdict: VerificationVerdict,
        selection: &ReportSelection,
        extraction: Option<&PullRequestExtraction>,
    ) -> Presentation {
        let settled = Settled {
            status: VerificationExecutionStatus::Completed,
            verdict: Some(verdict),
        };
        Presentation::render(
            "wyrd/review",
            ExecutionId::new_v7(),
            HEAD,
            settled,
            "",
            Some(Completed {
                selection,
                extraction,
            }),
        )
    }

    /// One valid report holding `findings`.
    ///
    /// # Panics
    /// Panics when the report does not decode.
    fn one(findings: Vec<Value>) -> ReportSelection {
        let report = json!({"format": "code_review", "version": 1, "summary": "Reviewed.",
                            "coverage": {"complete": true, "limitations": []},
                            "findings": findings});
        ReportSelection::One {
            source: serde_json::from_value(json!({"step": "report"})).expect("source"),
            report: Box::new(serde_json::from_value(report).expect("report")),
        }
    }

    /// Render a completed execution with `verdict` and `selection` against
    /// [`extraction`].
    fn completed(verdict: VerificationVerdict, selection: &ReportSelection) -> Presentation {
        shown(verdict, selection, Some(&extraction()))
    }

    /// Every status maps to its approved conclusion; unfinished work has
    /// none.
    #[test]
    fn conclusions_follow_the_verdict_and_status() {
        use VerificationExecutionStatus as Status;
        let cases = [
            (
                Status::Completed,
                Some(VerificationVerdict::Passed),
                Some("success"),
            ),
            (
                Status::Completed,
                Some(VerificationVerdict::Failed),
                Some("failure"),
            ),
            (
                Status::Completed,
                Some(VerificationVerdict::Inconclusive),
                Some("action_required"),
            ),
            (Status::Cancelled, None, Some("cancelled")),
            (Status::TimedOut, None, Some("timed_out")),
            (Status::Errored, None, Some("failure")),
            (Status::Waiting, None, None),
            (Status::Running, None, None),
        ];
        for (status, verdict, conclusion) in cases {
            assert_eq!(
                Settled { status, verdict }.conclusion(),
                conclusion,
                "{status:?}"
            );
        }
    }

    /// A waiting execution is in_progress with its explanation and renders
    /// no review, whatever its outputs hold.
    #[test]
    fn waiting_is_in_progress_with_an_explanation() {
        let settled = Settled {
            status: VerificationExecutionStatus::Waiting,
            verdict: None,
        };
        let selection = one(vec![finding(
            "F",
            "high",
            json!([anchor("src/lib.rs", 2, 2)]),
            None,
        )]);
        let shown = Presentation::render(
            "wyrd/review",
            ExecutionId::new_v7(),
            "abc",
            settled,
            "Waiting for approval `ship`.",
            Some(Completed {
                selection: &selection,
                extraction: Some(&extraction()),
            }),
        );
        assert_eq!(shown.check.status, "in_progress");
        assert_eq!(shown.check.conclusion, None);
        assert!(shown.check.output.summary.contains("approval `ship`"));
        assert!(shown.review.is_none());
    }

    /// Each severity is shown, a diff anchor is an inline comment pinned to
    /// the head, an unlocated finding and an anchor outside the diff stay
    /// in the summary, and a suggestion replaces exactly its range. Severity
    /// never changes the conclusion.
    #[test]
    fn findings_render_inline_or_in_the_summary() {
        let edit = json!({"location": anchor("src/lib.rs", 2, 3), "replacement": "z\n"});
        let selection = one(vec![
            finding(
                "crit",
                "critical",
                json!([anchor("src/lib.rs", 2, 3)]),
                Some(edit),
            ),
            finding("high", "high", json!([anchor("src/lib.rs", 3, 3)]), None),
            finding("med", "medium", json!([anchor("src/lib.rs", 4, 4)]), None),
            finding("low", "low", json!([]), None),
        ]);
        let shown = completed(VerificationVerdict::Passed, &selection);
        assert_eq!(shown.check.conclusion, Some("success"));
        assert!(shown.problems.is_empty());
        let review = shown.review.expect("review");
        assert_eq!(review.event, "COMMENT");
        assert_eq!(review.commit_id, HEAD);
        assert!(review.body.starts_with("<!-- wyrd:execution:"));
        assert_eq!(review.comments.len(), 2);
        let first = &review.comments[0];
        assert_eq!(
            (first.start_line, first.line, first.side),
            (Some(2), 3, "RIGHT")
        );
        assert!(
            first.body.contains("```suggestion\nz\n```"),
            "{}",
            first.body
        );
        assert!(first.body.contains("[critical/high confidence]"));
        assert_eq!(
            (review.comments[1].start_line, review.comments[1].line),
            (None, 3)
        );
        let summary = &shown.check.output.summary;
        for severity in ["critical", "high", "medium", "low"] {
            assert!(
                summary.contains(&format!("[{severity}/")),
                "{severity}: {summary}"
            );
        }
        assert!(summary.contains("`src/lib.rs:4-4`"), "{summary}");
    }

    /// An anchor the retained source does not hold is a problem in the
    /// summary, never an inline comment on another line: a wrong commit, a
    /// path neither side holds, a side without text, lines past a file or
    /// a context span, and a renamed file's head path on its base side.
    ///
    /// # Panics
    /// Panics when a refusal is missing, inline, or wrongly reasoned.
    #[test]
    fn anchors_outside_the_retained_source_are_problems() {
        let mut stale = anchor("src/lib.rs", 2, 2);
        stale["commit"] = json!("2".repeat(40));
        let cases = [
            (stale, AnchorProblemReason::AnchorCommitMismatch),
            (
                anchor("src/other.rs", 1, 1),
                AnchorProblemReason::AnchorPathUnknown,
            ),
            (
                anchor("logo.png", 1, 1),
                AnchorProblemReason::AnchorSourceUnavailable,
            ),
            (
                anchor("src/lib.rs", 4, 5),
                AnchorProblemReason::AnchorOutOfRange,
            ),
            (
                base("src/lib.rs", 2, 3),
                AnchorProblemReason::AnchorOutOfRange,
            ),
            (base("new.rs", 1, 1), AnchorProblemReason::AnchorPathUnknown),
            (
                anchor("src/util.rs", 12, 13),
                AnchorProblemReason::AnchorOutOfRange,
            ),
        ];
        for (bad, reason) in cases {
            let edit = json!({"location": bad, "replacement": "q\n"});
            let selection = one(vec![finding("F", "high", json!([]), Some(edit))]);
            let shown = completed(VerificationVerdict::Failed, &selection);
            assert_eq!(
                shown.problems,
                vec![AnchorProblem {
                    finding: "F".to_owned(),
                    reason
                }]
            );
            let review = shown.review.expect("a rendered report is reviewed");
            assert!(review.comments.is_empty(), "{reason:?}");
            assert!(shown.check.output.summary.contains(reason.as_str()));
            assert_eq!(shown.check.conclusion, Some("failure"));
        }
    }

    /// No report renders only the verdict; an ambiguous or invalid report
    /// is named in the summary and as the report error, and renders no
    /// review.
    #[test]
    fn missing_ambiguous_and_invalid_reports_render_no_review() {
        let none = completed(VerificationVerdict::Passed, &ReportSelection::None);
        assert!(none.review.is_none());
        assert_eq!(none.report_error, None);
        assert_eq!(none.check.output.title, "Verdict: passed");
        let source = |step: &str| -> ReportSource {
            serde_json::from_value(json!({"step": step})).expect("source")
        };
        let ambiguous = ReportSelection::Ambiguous {
            sources: vec![source("a"), source("b")],
        };
        let shown = completed(VerificationVerdict::Passed, &ambiguous);
        assert!(shown.review.is_none());
        assert!(shown.check.output.summary.contains("more than one step"));
        assert_eq!(
            shown.report_error,
            Some(GithubDeliveryReason::ReportAmbiguous)
        );
        let invalid = ReportSelection::Invalid {
            source: source("a"),
            problem: wyrd_spec::verification::report::ReportProblem::EmptyFindingId,
        };
        let shown = completed(VerificationVerdict::Passed, &invalid);
        assert!(shown.check.output.summary.contains("finding_id_empty"));
        assert_eq!(
            shown.report_error,
            Some(GithubDeliveryReason::ReportInvalid)
        );
        assert_eq!(shown.check.conclusion, Some("success"));
    }

    /// Base-side evidence, a renamed file's base under its previous path
    /// and an unchanged selected context span all hold a finding's anchors:
    /// the finding stays inline at its head location, and a finding located
    /// only in context stays in the summary without a problem.
    ///
    /// # Panics
    /// Panics when valid retained evidence is refused or misplaced.
    #[test]
    fn evidence_in_base_or_context_source_is_valid() {
        let mut supported = finding("F", "high", json!([anchor("src/lib.rs", 2, 2)]), None);
        supported["evidence"] = json!([
            base("src/lib.rs", 1, 2),
            base("old.rs", 2, 2),
            anchor("src/util.rs", 10, 12)
        ]);
        let in_context = finding("C", "low", json!([anchor("src/util.rs", 11, 11)]), None);
        let shown = completed(
            VerificationVerdict::Failed,
            &one(vec![supported, in_context]),
        );
        assert!(shown.problems.is_empty(), "{:?}", shown.problems);
        let review = shown.review.expect("review");
        assert_eq!(review.comments.len(), 1);
        assert_eq!(
            (review.comments[0].path.as_str(), review.comments[0].line),
            ("src/lib.rs", 2)
        );
        assert!(shown.check.output.summary.contains("`src/util.rs:11-11`"));
    }

    /// An empty report and an unlocated-only report each get one `COMMENT`
    /// review on the reviewed commit with no inline comments.
    ///
    /// # Panics
    /// Panics when a rendered report has no review or an inline comment.
    #[test]
    fn every_rendered_report_has_a_review() {
        for findings in [vec![], vec![finding("U", "medium", json!([]), None)]] {
            let shown = completed(VerificationVerdict::Passed, &one(findings));
            let review = shown.review.expect("review");
            assert_eq!((review.event, review.commit_id.as_str()), ("COMMENT", HEAD));
            assert!(review.body.starts_with("<!-- wyrd:execution:"));
            assert!(review.comments.is_empty());
        }
    }

    /// Without an extraction a report still renders: its summary and
    /// unlocated finding are shown, an anchored finding is refused as
    /// unavailable and never inline, and an ambiguous or invalid report
    /// keeps its reason. The conclusion stays the verdict's.
    ///
    /// # Panics
    /// Panics when report content or a reason is lost without extraction.
    #[test]
    fn reports_render_without_an_extraction() {
        let selection = one(vec![
            finding("U", "medium", json!([]), None),
            finding("A", "high", json!([anchor("src/lib.rs", 2, 2)]), None),
        ]);
        let rendered = shown(VerificationVerdict::Failed, &selection, None);
        let summary = &rendered.check.output.summary;
        assert!(summary.starts_with("Reviewed."), "{summary}");
        assert!(summary.contains("(`U`)") && summary.contains("anchor_source_unavailable"));
        assert_eq!(
            rendered.problems,
            vec![AnchorProblem {
                finding: "A".to_owned(),
                reason: AnchorProblemReason::AnchorSourceUnavailable
            }]
        );
        assert!(rendered.review.expect("review").comments.is_empty());
        assert_eq!(rendered.check.conclusion, Some("failure"));
        let source: ReportSource = serde_json::from_value(json!({"step": "a"})).expect("source");
        let ambiguous = ReportSelection::Ambiguous {
            sources: vec![source.clone(), source.clone()],
        };
        let invalid = ReportSelection::Invalid {
            source,
            problem: wyrd_spec::verification::report::ReportProblem::EmptyFindingId,
        };
        for (selection, reason) in [
            (ambiguous, GithubDeliveryReason::ReportAmbiguous),
            (invalid, GithubDeliveryReason::ReportInvalid),
        ] {
            let rendered = shown(VerificationVerdict::Passed, &selection, None);
            assert_eq!(rendered.report_error, Some(reason));
            assert!(rendered.check.output.summary.contains("Report not shown"));
            assert!(rendered.review.is_none());
        }
    }

    /// A summary past GitHub's limit is cut with an explicit note.
    #[test]
    fn long_summaries_are_cut_visibly() {
        let cut = bounded("x".repeat(SUMMARY_LIMIT + 10));
        assert_eq!(cut.chars().count(), SUMMARY_LIMIT);
        assert!(cut.ends_with("read the full report in Wyrd."));
    }
}
