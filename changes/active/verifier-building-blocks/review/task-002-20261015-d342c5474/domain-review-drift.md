# TASK-002 domain review: drift and evaluation fidelity

Result: **FAIL** — one proposed material finding.

## Immutable subject and boundary

Base `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`; candidate
`d342c54749a871dcacf5b0c7ec29e3b458f2d9e2` (HEAD rechecked unchanged).
Inputs: `subject.md`, cumulative base-to-candidate diff, `original-task.md`,
`spec-snapshot.md` revision 6, and `verification-evidence.md` in this directory.
This independent discovery pass covers the statistical adapter, requirement
identity, selected samples, readiness/completeness, report projection and
verdict/provenance propagation. It does not audit concurrency mechanics or
hosted scheduling/authorization assigned to other owners.

## Authority and source coverage

| Authority/boundary | Source inspected and application |
| --- | --- |
| Repository and approved change | AGENTS.md; architecture/agent-rules.md; reference router; spec-driven-development and maintainer-style references; Wyrd design doctrine and Verifier/Drift sections; wyrd-doctrine.mdx. Approved spec explicitly supersedes the old implementation-kind split; unrelated evidence and ownership rules remain applicable. |
| Statistical integrity | domain/drift-monitoring.md; domain/evaluation.md; REQ-005/006/014/015 and INV-001/006. Incomplete windows must not pass and statistical math stays in vala-drift. |
| New adapter and consumers | Entire blocks/drift.rs, blocks/engine.rs, blocks.rs export diff, and drift engine test. Followed judge → score_drift → method scorer → scored → Judged::checked → RequirementResult → decide. |
| Existing statistical owners | vala-drift baseline/mod.rs::score_drift; custom/mod.rs::score_custom and score_custom_mean; feature.rs column extraction and target_complete; report.rs; PSI sample floors/completeness paths; SPC score_spc/SpcScorer and their existing report/completeness tests. |
| Existing alternative/workflow | Server verification/drift.rs::DriftEngine::try_verify and fold_custom; direct.rs::DirectExecutor::drift. These owners contain IO/admission and cannot replace the required IO-free adapter. Their existing completeness behavior is relevant to adapter acceptance. |
| Wire/detail preservation | wyrd-spec card/blocks::DriftCheck, verification.rs::DriftReport/FeatureDriftReport, execution.rs::RequirementDetail/CallProvenance. Inspected JSON projection compatibility including typed PSI/SPC evidence and nullable inconclusive scores. |
| Regression consumers | assertion/trace/results sharing diffs: comparison semantics and actual-value capture remain in their existing owners; no statistical algorithm changes are in the diff. Crate manifest confines the added vala-drift dependency to vala-eval. |

CodeGraph was consulted before code location/understanding, then focused source
reads and searches expanded its caller map. No other discovery reports were read.

## Domain acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Existing PSI/SPC/Custom statistical algorithms remain unchanged | DriftChecks::judge delegates at blocks/drift.rs:74 to unchanged score_drift; no vala-drift changes | Recorded 75 vala-drift tests; existing method-dispatch/report tests | PASS |
| Requirement-specific fitted state and slot-specific samples | DriftChecks baselines BTreeMap<TaskId, FittedBaseline>, samples BTreeMap<SlotName, RecordBatch>; judge uses requirement ID and on.slot | New drift engine test distinguishes ready/unready requirements using the same samples slot | PASS |
| Baseline readiness and insufficient PSI/SPC samples remain inconclusive | Missing fitted state returns inconclusive; PSI/SPC scorer completeness/sample checks return Inconclusive reports; unchanged projection maps Inconclusive honestly | New drift test covers PSI missing state and too few rows; existing SPC scorer tests cover partial/insufficient subgroups | PASS |
| Custom selected-window completeness remains honest | New adapter supplies raw batches directly to scorer that drops nulls, bypassing existing host completeness policy | New test supplies only complete Custom samples and misses this path | FAIL — DRIFT-D1 |
| Full available statistical report is retained | scored serializes engine report to matching wire report, preserving features, score/threshold, evidence; Judged::checked preserves detail | New test checks detail presence only; source compatibility plus existing scorer report tests support preservation | PASS, proof limit below |
| Verdict, exact check and provided window survive projection | NoDrift/Drift/Inconclusive mapping; RequirementResult retains check; CallProvenance::Drift retains self.window without modification | New drift test checks outcome mappings and None window; no non-None window test | PASS, proof limit below |
| Statistical IO, baseline fit policy and reactions stay outside engine | Adapter receives resolved state and samples; no server, SQL, registry, scheduler or Operator calls added | In-memory test and source inspection | PASS |

## Proposed finding

### DRIFT-D1 — INCORRECT: incomplete Custom windows can become passed judgments

**Violated obligation:** REQ-014 preserves existing insufficient-data behavior;
INV-006 prohibits missing evidence silently becoming success. Task scenario 4
requires preserved sample/readiness behavior. The drift reference explicitly
requires an incomplete window to remain indeterminate.

**Changed location:** `crates/vala/vala-eval/src/blocks/drift.rs:57-76`, especially
the unconditional raw-batch call to `score_drift` at line 74.

**Producer-to-consumer evidence:** A caller constructs a valid nullable numeric
RecordBatch for a declared samples slot and passes it to `DriftChecks::new`.
`VerificationEngine::check_bindings` permits drift to read its injected window,
then `judge` selects this batch. `score_drift` dispatches Custom to
`vala-drift/src/custom/mod.rs:29-67::score_custom`. That function calls
`ColumnRef::collect_f64_non_null`; `feature.rs:28-40` explicitly omits null rows.
For a Custom profile with baseline 100 and threshold 5, selected samples
`[Some(100.0), None]` therefore become mean 100, NoDrift. `DriftChecks::scored`
maps that to Passed; a one-requirement Verifier with decide=all also passes.
This path is supported by the task's required standalone engine API and needs
neither a server nor a malformed spec.

**Existing owner/sibling comparison:** The statistical function is intentionally
a low-level null-filtering mean implementation; both existing hosted entry
points enforce selected-window completeness before calling math.
`verification/direct.rs:161-164,208-222` states and implements that a null in
any scored feature yields an unscored report. The scheduled owner
`verification/drift.rs:628-643::fold_custom` requires `n > 0 && n == numeric_n`
and a valid finite mean before `try_verify:727` calls score_custom_mean.
PSI and SPC raw scorers already check their own target completeness. All
production score_drift callers are this new adapter and the existing direct
owner; remaining callers are scorer tests/public-surface assertions. There is
no upstream engine check or private constructor invariant that rejects the
nullable Custom batch.

**Observable consequence:** Missing selected Custom observations can be ignored
and the subject reported healthy. It can also change a failed/inconclusive
aggregate into passed under the approved decision rules.

**Smallest testable correction:** Keep math and the existing hosts unchanged.
At the adapter's selected-evidence boundary, preserve the existing Custom
completeness rule before invoking the unchanged scorer: a null in its configured
metric column must yield Inconclusive rather than a reduced-population mean.
Use the existing unscored-report projection to preserve drift detail and window
provenance; do not invent sampling, imputation, baseline, or threshold policy.
This is the correct boundary because the adapter owns the supplied whole-window
batch and the low-level mean helper does not promise complete observations.
Resolve the metric by configured name, preserving current missing-column/type
refusals and allowing unrelated columns to remain irrelevant. The installed
Arrow column API suffices; no dependency or new graph abstraction is needed.

**Focused closure proof:** Add one IO-free engine/adapter regression with valid
Custom configuration and nullable selected values `[100, null]`, asserting an
inconclusive requirement and overall verdict, not pass, with drift window
provenance retained. Keep complete `[100,100]` passing and out-of-band complete
values failing. Exercise it using an exact selector recorded after naming the
test; rerun the existing exact drift engine selector and task-local vala-eval
library lane. No journey suite or new harness is needed for this pure boundary.

## Verification limits

Static review only: this reviewer did not modify source or rerun tests. The
recorded 120 vala-eval/75 vala-drift test results and Clippy/format/dependency
checks are claims in the supplied evidence. The new engine drift test exercises
Custom and PSI but not SPC through the adapter, uses only `window: None`, and
asserts statistical detail presence rather than full report equality. Those
are explicit confidence limits, not separate findings without a demonstrated
wrong behavior. Hosted source selection, baseline persistence keys, cron
windows, SDK/server journeys and failed-only dispatch belong to TASK-005 and
the integrated review; they were not required or run here.

No statistical algorithm change, new baseline policy, or material product
decision is necessary to close DRIFT-D1. No unrelated recommendations are made.
