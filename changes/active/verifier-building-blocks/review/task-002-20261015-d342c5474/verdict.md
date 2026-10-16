# TASK-002 verdict: FIX_REQUIRED

## Immutable subject

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-002`.
Base: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`.
Candidate: `d342c54749a871dcacf5b0c7ec29e3b458f2d9e2`.
Review covers the complete three-commit cumulative range, preserved in [cumulative.diff](cumulative.diff).
Approved specification: `changes/active/verifier-building-blocks/spec.md`, revision 6, frozen in [spec-snapshot.md](spec-snapshot.md).
Original task: `changes/active/verifier-building-blocks/tasks/TASK-002-verification-engine.md`, frozen without implementation claims in [original-task.md](original-task.md); full committed evidence is in [task-snapshot.md](task-snapshot.md).

The untracked parent packet was supplied as authority and frozen for this review. Nothing was staged or committed. HEAD, index and tracked source remained unchanged throughout review; final checks reconfirmed this subject. No previous TASK-002 finding ledger or remediation requires closure. TASK-001 review history is prerequisite context, outside this candidate range.

## Independent review results

| Role | Report | Discovery result |
| --- | --- | --- |
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| repo-rev | [standards-review.md](standards-review.md) | FAIL |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | FAIL |
| system-rev | [system-review.md](system-review.md) | PASS |
| domain-rev: concurrency | [domain-review-concurrency.md](domain-review-concurrency.md) | FAIL |
| domain-rev: drift | [domain-review-drift.md](domain-review-drift.md) | FAIL |
| followup-rev | [followup-review.md](followup-review.md) | RESOLVED |
| ponytail-rev | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; five retained findings |

All roles used separate fresh agents. The orchestrator established the subject, compared claims and assembled this verdict; it did not substitute its own validation for the independent ledger.

[Claim comparison](claims-comparison.md) groups shared causes. A focused follow-up was required to distinguish unchanged Custom numerical scoring from preservation of incomplete-window refusal. It established that the standalone adapter owns its supplied selected population; hosted scheduling does not waive that requirement. The validator independently confirmed that conclusion. Other general PASS cells concerned narrower covered paths and do not contradict the concrete negative paths. No unresolved claim or missing reviewer remains.

## Reconciled acceptance matrix

These results cover TASK-002's portion of the approved specification. Other tasks retain hosted runtime, server/SDK, persistence, scheduling, reactions and integrated journey obligations.

| Obligation | Implementation and proof | Result |
| --- | --- | --- |
| REQ-001/002: no-step and multi-step reusable engine; named evidence, optional null, required missing values | Public shared seam, Scope resolution; library tests and source traces | PASS except owning-step mixed-cause path below |
| REQ-003: existing planner, action then step checks, dependent barriers, final requirements once | VerifierSpec::plan and drive ordering; scripted ordering/overlap tests | PASS for behavior; workflow ownership fails INV-001 |
| REQ-004: bounded concurrency, retry budgets, attempt timeouts and visible attempts | Shared semaphore, attempt loop and result projection; retry/timeout tests | PASS |
| REQ-005: deterministic assertion/trace reuse; isolated execution failures inconclusive | Shared comparison/projection helpers and injected check boundary; library regressions | PASS |
| REQ-005/INV-007: exhausted overall deadline yields execution error | Pending cancellation covered, but ready/no-spawn path bypasses elapsed deadline | FAIL: FIND-TASK-002-3 |
| REQ-006/INV-006: failure wins across every required evidence cause; no check runs on missing evidence | Whole-Verifier paths merge gaps; containing-step early return omits explicit binding causes | FAIL: FIND-TASK-002-2 |
| REQ-006: all/pass-rate rollup, applicable denominator, empty/all-skipped inconclusive, once-only counting | decide and result ordering; table-driven library proof | PASS once requirement classification is correct |
| REQ-014: unchanged statistical math, per-requirement fitted state, per-slot samples, full report/window projection | vala-drift remains unchanged; adapter and scorer tests | PASS for math/identity/detail |
| REQ-014/INV-006: retained insufficient-data behavior | Custom null rows are filtered before the adapter checks population completeness | FAIL: FIND-TASK-002-4 |
| REQ-015: breakdown, check configuration, evidence/provenance, actual capture | Result construction and shared capture helper; library proof | PASS for task-local composition |
| INV-001: extend complete existing execution/context/registry workflow | Two independently maintained stage/state lifecycles remain live | FAIL: FIND-TASK-002-1 |
| INV-005/008: no server/SQL/SDK/policy/reaction ownership move; unrelated scenario behavior preserved; no retired judge dependency in new engine | Complete diff and sibling callers; tests/dependency check | PASS; preservation must continue through remediation |
| AGENTS §16: complete substantive touched-item rustdoc | Independently validated omissions in assertion, constructor, shared test module/helpers and private invariants | FAIL: FIND-TASK-002-5 |
| AC-002/003/007/010 task-local proof and shared hosted seam | Same injected production seam is tested; 195 library tests and narrow checks pass | PASS for performed checks; insufficient to waive findings |

## Validated finding ledger

Only independently confirmed or revised findings are retained. Exact source traces, consequences, correction boundaries and focused proof are in [findings-validation.md](findings-validation.md), incorporated into the remediation task.

| Stable ID | Status / classification | Required correction |
| --- | --- | --- |
| FIND-TASK-002-1 | CONFIRMED / VIOLATION | Evolve existing execution/context/registry ownership; remove parallel stage/state lifecycle while preserving scenario semantics. |
| FIND-TASK-002-2 | CONFIRMED / INCORRECT | Merge containing-step and all explicit requirement-binding gaps using the existing strongest-cause rule. |
| FIND-TASK-002-3 | CONFIRMED / INCORRECT | Reject elapsed absolute deadlines before work and before publishing a verdict; retain pending-work timeout/cancellation. |
| FIND-TASK-002-4 | CONFIRMED / INCORRECT | Preserve selected Custom metric completeness at the adapter before null-filtering scoring, retaining the unscored report. |
| FIND-TASK-002-5 | REVISED / VIOLATION | Complete the individually retained touched-item rustdoc omissions in the same correction. |

Rejected fixed-fixture panic claims are excluded, not carried as optional requests. Historical test-after-draft sequencing stays candidly recorded; no retrospective RED fabrication is requested. Local import style and parent-packet tracking do not independently block this task or require a packet commit. No new specification or private-task correction is needed: approved authority resolves each retained outcome.

## Verification and limits

[Independent verification](review-verification.md) records successful reruns: 195 vala-eval/vala-drift library tests, `mise run fmt:check`, `mise run check:deps`, narrow all-features library/test Clippy with `-D warnings`, and cumulative diff whitespace checking. Logs are preserved alongside this verdict. The review reran the complete two-library set; the 15 separately recorded exact-selector runs remain implementation evidence.

No source mutation was used to reproduce findings; source paths and the pinned Tokio implementation establish the omitted cases. The Vala integration target, full journeys, providers, database and broad workspace aggregate were not run, as directed by the task. SPC-through-adapter and full report equality have limited direct new-test evidence but no separate defect was validated. Hosted integration remains for the once-integrated change review.

## Disposition

**FIX_REQUIRED.** Route [TASK-002-R1-engine-acceptance.md](TASK-002-R1-engine-acceptance.md) directly to `$wyrd-implement`. This review packages the correction; it does not execute it, merge, push or deploy. The next task review must reassess the complete original base-to-remediated candidate and explicitly close all five stable IDs.
