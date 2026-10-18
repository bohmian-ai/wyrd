---
id: TASK-003-R2
kind: remediation
status: ready
spec: SPEC-long-horizon-verification
spec_revision: 13
parent_task: TASK-003
remediates: [FIND-TASK-003-3, FIND-TASK-003-6]
requirements: [REQ-006, AC-005, AC-006, INV-002, INV-003, INV-005]
depends_on: []
---

# Observe durable replay processing and close HTTP diagnosis evidence

Execute directly with `$wyrd-implement` as one bounded remediation. No new design decision is required. Preserve the portions already accepted; no commit, merge, push or deployment is authorized by this task.

## Authority and immutable review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-003`.
- Specification: `changes/active/long-horizon-verification/spec.md`, current TASK-003 text explicitly authorized by Steven and reaffirmed for this review; revision 14's draft pre-release schema addition is excluded. This packet retains the original revision-13 task mapping without editing the spec.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-003-bounded-outbox-admission.md`.
- Original base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Reviewed cumulative candidate: `ea317179820b24d8b08ed499454ce2cf4675ce4e` on `change/long-horizon-verification-task-003`; range `git diff ef3a57b07...ea3171798`.
- Previous candidate: `9ffdf753ab11ce9b638c906388d3690a6aede249`. Prior review/remediation: `changes/active/long-horizon-verification/review/task-003-9ffdf753a/{verdict.md,findings-validation.md,evidence-reassessment.md,TASK-003-R1-close-admission-review.md,diagnostician-reports.md,supplemental-diagnoses.md}`.
- Current authority for correction: this directory's `verdict.md`, `findings-validation.md` and focused `followup-review.md`. Findings 1, 2, 4 and 5 close; original five diagnosis subcases under finding 6 close. Only the two narrowed findings below remain.

Read AGENTS.md, architecture/agent-rules.md and routed applicable spec-driven-development, implementation-execution, maintainer-style, Rust/error/testing references. Follow CodeGraph. Existing Wyrd/Bifrost admission, ACK, replay and shutdown boundaries remain authoritative.

## FIND-TASK-003-3 — The final queue count precedes the replay's downstream processing

**Obligation:** original Scenario 3 and AC-005 require same-identity recovery/replay after pressure; R1-AC-3 requires one durable execution after replay. Response equality alone does not establish the composed durable observation.

**Current evidence:** `crates/wyrd/wyrd-server/tests/integration/pg_verification_routes.rs:532–541` accepts the final replay, checks equal execution ID, immediately reads `fixture.queued()` and shuts down. The prior `(total inputs, recovered inputs) = (3,1)` reconciliation occurs before replay and correctly proves the refused attempt staged nothing. Retain that proof.

**Root diagnosis and consequence:** `VerificationControl::enqueue` at `components/verification/service.rs:674–685` returns after staging. Scribe ACK then hands the submission to another outbox (`scribe_outbox.rs:950–962`), whose writer separately accepts and commits through TenantConn (`verification/submissions.rs:90–119`). `fixture.queued()` is a point-in-time SQL SELECT. It can see the three already durable rows before the final replay reaches either consumer. `flush_bifrost` at `wyrd-testing/src/server.rs:1231–1253` drains source Scribe work, not the independent submission transaction. The current source's SQL replay is correct; the test can pass without observing this final replay. Existing no-pressure replay tests were already present when the composed seam was required and cannot order this test's final SELECT.

**Selected minimal correction:** extend the existing HTTP pressure/recovery test and its rustdoc using the same-tenant ordered-sentinel observation already used by `enqueued_executions_replay_conflict_and_read_back` in the same module (`:381–458`). After the final accepted replay/equal ID, accept one distinct later same-tenant request through the existing enqueue helper, prove its ID differs, wait for that sentinel via existing `visible_execution`, then assert durable execution cardinality **4**: two pressure-fill executions, the recovered identity, and the sentinel. This is a test-only observation at the existing owner, not a new receipt or drain API.

**Why this closes the source of the gap:** successful sequential sends precede HTTP responses; the existing generic Outbox appends in arrival order and serializes each tenant, retrying its older failed slice first (`outbox.rs:530–620`). Scribe enumerates original input items in order after successful slice processing. SubmissionOutbox preserves the same tenant order; `accept_submissions` processes its slice in order before commit. Sentinel visibility therefore places this replay's healthy recovery processing before the count. Waiting only for the recovered ID is insufficient because its earlier row already exists. Adding only a Scribe flush is insufficient because it does not imply SQL commit. No production change is needed.

Capacity 3 admits two reserved source items, enough for replay plus sentinel after the existing pre-replay flush; each accepted input already owns downstream capacity. Keep both writers alive through sentinel observation, release the injected stall, use the same valid small inline request and introduce no subsequent terminal failure. This tests healthy post-drain recovery. Terminal rejection, abrupt crash or expired shutdown retains accepted loss semantics; do not turn the sentinel into a stronger delivery guarantee. Do not assert deduplication of retained inputs from accepted replay: the required final count concerns durable executions, and existing source reconciliation remains before replay.

## FIND-TASK-003-6 — Two new R1 test-failure corrections lack independent diagnosis reports

**Obligation:** `$wyrd-implement` requires a fresh read-only diagnostician before failure-driven test/assertion edits, independently of its timing/ordering trigger. The task-review skill requires recorded diagnosis and diagnostician report. R1 repeats this rule.

**Exact evidence:** prior `TASK-003-R1-close-admission-review.md:151` records two unexpected failed runs and corrective test changes: queue cardinality moved after ACK/drain, and `CAST(... AS VARCHAR)` replaced by projection of the stored column. Current code is `pg_verification_routes.rs:466–480,518–541`. These were test mistakes rather than the intended RED demonstrating missing product behavior; both trigger independent diagnosis, with the first also involving ordering. Supplied A–E cover only the original five diagnoses. No independent report for these two new failures or corresponding historical trace is supplied.

**Consequence and correction boundary:** source supports the current changes, but the mandated independent audit of failure cause and preservation of acceptance proof remains unavailable. This is an evidence-production gap, not an allegation of bad fixes or a production defect. Reuse the existing enqueue/Scribe/submission lifecycle and retained-record decoding owners. `retained_records` and `audit_cell_text` at `wyrd-testing/src/server.rs:142–159,2009–2066` accept existing supported physical cell types; wider decoding, another reader or a new production API is not required.

Recover any actual independent report and original failing command/output if available. If unavailable, commission **one fresh read-only retrospective diagnostician** for this cohesive HTTP proof boundary, using available failing command/diagnostics and historical diff/source, without supplying the implementer's hypothesis. Its report must cover both the ACK/submission-commit observation mistake and projection/physical-decoder mismatch, existing correction owners, enqueue/replay helpers and retained audit/span reader siblings, affected callers, preserved assertions and explicit missing historical evidence. Record present source validation honestly; do not claim it proves pre-fix process compliance. No baseline checkout, old-code rerun, bisect or attribution exercise is needed.

Preserve A–E and their closed original subcases. Record/link the recovered or retrospective report and actual available diagnostics in **this review directory** and append compact evidence to this remediation task. Leave the prior review directory and original task unchanged. The already recorded HTTP 2/2 Green is useful context, not a diagnosis report. The focused rerun for the finding-3 proof edit can serve both criteria; no separate run solely to attach diagnosis evidence is required. Any new unexpected failure during this remediation follows the same tracing/independent-diagnosis rule before adaptation.

## Scope, preserved behavior and non-goals

Expected write set: the existing server HTTP pressure/replay test and its substantive rustdoc; evidence and this remediation task in the current review directory. Reuse current helpers and owner ordering; private implementation choices remain implementation-owned. No production source, config, SDK, schema, generator, new helper API, decoder, dependency or runtime change is required by either finding. Do not alter the spec, original task or prior review reports.

Preserve closed findings: correct single-owner shutdown accounting and unused Reservation ownership; finite whole-pending/task bounds; three SDK continuing-traffic stories; documented pressure/timing; non-blocking counted audit/capture/span loss; normative security/recovery prose; exact tenant/authority boundaries; stable retries and existing SQL replay/conflict owner. No accepted-source replay deduplication, distributed quota, strict fairness, stronger receipt/durability, producer Postgres, platform-specific mechanism, new harness or unrelated cleanup. Grouped J-005 remains deferred to TASK-001 integration and capacity benchmarks to their existing owner. Rejected navigation-only/rustdoc-calibration proposals do not enter this task.

## Acceptance and focused proof

| Criterion | Finding | Required evidence |
|---|---|---|
| R2-AC-1 | FIND-TASK-003-3 | Final same-key replay is ordered before durable cardinality via a distinct accepted same-tenant sentinel and its real execution visibility; cardinality 4; pre-replay `(3,1)` source and pressure/drain assertions preserved. Existing replay/conflict regression passes. Source ordering remains explicit, without new production guarantees. |
| R2-AC-2 | FIND-TASK-003-6 | Inspectable recovered or honest retrospective independent report covers both new R1 HTTP failures, available commands/diagnostics/diff, causes, existing fix sites, siblings/callers and limits; original A–E closures and source-supported corrections preserved. |

Work one observable proof scenario at a time under `$wyrd-implement`. Static source ordering establishes why the old test's observation is early; the corrected test is the focused behavioral proof. Do not manufacture production defects or weaken an assertion to obtain RED. Evidence-only diagnosis closure needs no artificial test. Record what was actually run and its limits, with nonzero intended selection.

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_runtime=debug,wyrd_server::verification=debug mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_verification_routes::staging_refusal_recovers_the_same_enqueue_identity) | test(=pg_verification_routes::enqueued_executions_replay_conflict_and_read_back)"'
mise run fmt
mise run lints
git diff --check
```

These are closure commands, not executions by this task author. Names are source-confirmed. The exact pair is sufficient for the expected Rust-test/rustdoc/evidence write set with required Rust format/lints. Do not rerun SDK/runtime stories, full journey suites, codegen, tenant/dependency scans, docs-site checks, broad gate or benchmarks unless an actually changed boundary newly requires them. No new boundary is expected.

Cumulative review `git diff --check ef3a57b07...ea3171798` reports two trailing spaces in prior verbatim diagnostic output, at `supplemental-diagnoses.md:72,209`. This was independently assessed as a nonblocking evidence-format limit. Do not modify immutable prior reports or silently claim that cumulative check passed; distinguish it from the working remediation diff check.

Append a compact R2-AC-1/2 implementation/evidence/result table, exact command/counts/features, diagnosis provenance/limits, non-goal audit and final tracked/untracked diff audit here. Return IMPLEMENTED with the task path and immutable candidate identity to a subsequent cumulative `$wyrd-task-review`. Implementation does not approve, commit, merge, push or deploy.

## Remediation evidence (implementer)

Candidate: `ac25d8c42..HEAD`. Commits: `ce13c1cce` for the test, then the evidence commit that adds this section.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| R2-AC-1 | `pg_verification_routes::staging_refusal_recovers_the_same_enqueue_identity` (`crates/wyrd/wyrd-server/tests/integration/pg_verification_routes.rs`). After the equal-ID replay, the test accepts a distinct unkeyed sentinel (`answer` 2) from the same tenant and asserts that its ID differs from the recovered ID. It waits for the sentinel through `visible_execution` and then asserts 4 executions. The rustdoc states the ordering the test relies on. The earlier pressure, refusal, drain, `queued()==2` and pre-replay `(3,1)` inputs assertions are unchanged. There is no production change. | `WYRD_LOG=info,wyrd_runtime=debug,wyrd_server::verification=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_verification_routes::staging_refusal_recovers_the_same_enqueue_identity) \| test(=pg_verification_routes::enqueued_executions_replay_conflict_and_read_back)"'` ran 2 tests: 2 passed, 270 skipped (default features and profile). The migration checks for wyrd-sql and vala-sql passed 1/1 each. No RED was produced: the earlier test could pass without observing the replay, and a RED would have needed a manufactured defect. | PASS |
| R2-AC-2 | [diagnostician-report-f.md](diagnostician-report-f.md) is a fresh, read-only, retrospective report, given only the command, the recovered output ([r2-http-failure-evidence.md](r2-http-failure-evidence.md)) and the commit `ba12fe8f1`. It covers both failures. **Failure 1:** the execution row is queued only after Scribe acknowledges the inputs, so the fix belongs in the test, and it is there. **Failure 2:** `CAST AS VARCHAR` plans as `Utf8View`, which the shared `audit_cell_text` decoder rejects. The caller-side fix works. The report notes that the shared decoder is the broader owner. It lists siblings, preserved assertions and limits. Reports A–E and their closures are unchanged. | No original independent report from the time of failure exists. The only failing output that survived is the session excerpts in the evidence file. The report's Utf8View cause is inferred from source and was not reproduced. | PASS |

Report F also raises points outside this task's write set. They are recorded here and not acted on:
1. The shared decoder could accept `StringViewArray`, which would remove the CAST trap for every caller of the now-public `retained_records`. R2 says wider decoding is not required.
2. In the same module, the zero-count assertions at about lines 636, 1049, 1166 and 1270 run before any settle. They could pass even if a refused submission had been wrongly staged. These belong to other tests, not to TASK-003.
3. Its sibling note on the `queued()==3` read in `ba12fe8f1` is the same gap that R2-AC-1 closes.

Static checks: `mise run fmt` and `mise run lints` exited 0. `git diff --check` on the working tree exited 0. The cumulative `git diff --check ef3a57b07...HEAD` still reports the trailing spaces at `review/task-003-9ffdf753a/supplemental-diagnoses.md:72,209`, which the reviewer assessed as non-blocking. That file is left immutable, as instructed. Report F's file has its trailing whitespace stripped.

Non-goals and diff audit. No production source, configuration, SDK, schema, decoder, helper API or dependency changed. The spec, the original task and the prior review directory are untouched. No SDK, runtime or journey suites were re-run, and neither were `gate` or the benchmarks, as the task's scope allows. Tracked changes are limited to the test file and this review directory.
