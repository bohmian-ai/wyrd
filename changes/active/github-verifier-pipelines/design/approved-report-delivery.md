# Approved standard report selection and delivery

Status: approved design contract; not implemented.

The user approved the complete authoring-to-delivery proposal and example.
No report selector is added to VerifierSpec or Repository PR bindings. The
previous Verifier-level report and binding-level github.report proposals remain
rejected.

## Output convention

- A normal step returns the standard code_review report envelope as its output.
- Add response_format: code_review as a named Prompt response format resolving
  to the standard report JSON Schema, using the existing named-format mechanism.
  This is new work; today's named resolver supports verdict.
- The report has top-level format: code_review and version: 1, plus summary,
  coverage (complete and limitations) and findings. Findings use the previously
  approved fields: id, severity, confidence, title, explanation, locations,
  evidence and optional remediation/suggested edit. Exact nested schemas and
  resource bounds remain to specify; sample prose/IDs are illustrative data.
- The same report can be produced by normal code or adapted from a custom
  reviewer output. It is not a special engine action or Verifier kind.
- Intermediate reviewers reuse the Finding schema in their own ordinary outputs.
  For multiple reviewers, an ordinary consolidator emits the final envelope.
  If an intermediate step emits a final envelope too, it counts as another report;
  consumption by a consolidator does not silently hide it.

## GitHub consumption

After verification settles, inspect accepted successful step outputs for the
top-level code_review envelope and validate against the supported report schema.
Failed-attempt, discarded speculative and intermediate repeat outputs are not
final report candidates. An unsupported/malformed envelope is an explicit
delivery error, not a clean review.

| Reports | Delivery |
|---|---|
| None | Ordinary execution/verdict summary without structured findings |
| Exactly one | Render that report into the Check and PR review comments |
| More than one | Visible ambiguous-report error; do not guess or publish all |

Apply this per bound Verifier execution, not across all bindings for a PR. The
Check reflects the Wyrd execution/verdict; report summary and findings supply its
review content. Authored requirements choose their severity/coverage policy;
report presence or an empty findings list does not independently imply a pass.
No report failure rewrites an already accepted verification verdict.

Validate source locations and edits against pinned evidence. Valid anchors can
produce annotations and inline comments; valid replacements can produce
suggestions. Unlocated findings stay in the summary. Retain the accepted report
for provider delivery retries, which do not rerun review. Existing supersession
and authority constraints still govern delivery; retry/reconciliation details
remain to specify under the no-new-PostgreSQL-tables constraint.

## Worked example

[Standard report example](worked-examples/standard-report/README.md) contains the
approved TOML, Verifier and Agent. It shows direct context injection and workspace
mounting, one report-producing step, and an ordinary code requirement returning
inconclusive for incomplete coverage, failed for critical/high findings, otherwise
passed. It illustrates future contracts and is not runnable on today's engine.

Verified current owner: AgentExecutor::execute returns schema-validated Agent
structured output as steps.<id>.output; existing code checks return Verdict.
New work is the standard schema/named format and GitHub consumer validation and
rendering, not another output store or report-selection configuration surface.

## Subsequently approved presentation defaults

Severity describes impact: critical is catastrophic; high is serious failure;
medium is bounded material failure; low is limited actionable impact. Confidence
is separate. Validate suggested edits against exact retained source. No extra
scoring system is selected.

Map passed to success, failed to failure, inconclusive to action_required,
cancelled to cancelled, timed_out to timed_out, and execution error to failure.
Pending evidence/approval keeps an in_progress Check with an explanatory summary.

Preserve finding IDs and reuse existing comments where possible; retries do not
post duplicates. Automatic thread resolution is excluded from this change.
Absence from an incremental report does not prove a fix.
