# TASK-001 acceptance verdict

**FIX_REQUIRED**

Candidate `bcbe88527eccd7be4768cc39e1ba55b855300512` does not yet satisfy the
approved task. Five independently validated material findings remain. The sole
backend/startup architecture is accepted; no specification revision or redesign
is required for the bounded corrections.

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/resource-scaling`.
- Base: `8d25b13a1ededa23b9eb91973a2b489fe9224557`.
- Candidate: `bcbe88527eccd7be4768cc39e1ba55b855300512`.
- First review: complete `git diff 8d25b13a1...bcbe88527`, all five implementation
  commits, including `8387f3a05` rather than accidentally excluding it.
- Authority: [approved spec revision 2](../../spec.md) and
  [original TASK-001](../../tasks/TASK-001-sole-benchmark-recorder.md), whose
  contractual text is available at the first implementation commit. Candidate
  implementation evidence was assessed as claims, separately from the contract.
- No prior task-review acceptance/closure carried forward; no remediation round
  or convergence assessment applies. Planning comprehension is not implementation
  acceptance.
- HEAD and tracked/index source remained at the candidate throughout the review.
  Only this review's reports were written. The pre-existing untracked
  `changes/active/gateway-latency-capture/` packet was not touched.
- TASK-002 diagnosis/optimization proposals and any performance experiment are
  outside this audit. No benchmark or release benchmark build ran.

## Reviewer selection and validated coverage

The full source-backed applicability record and common navigation map were written
in [scope.md](scope.md) before delegation. Six fresh, independent discovery
reviewers occupied separate roles; a seventh fresh reviewer validated source,
materiality, correction burden, applicability and coverage. The orchestrator did
not validate its own findings. No reviewer received another's discovery conclusions
or an intended verdict before its own discovery.

| Role | Report | Discovery result and covered boundary |
|---|---|---|
| behavior-rev | [Behavior](task-review-behavior.md) | FAIL; complete task acceptance, six workflows, producers, non-goals and negative paths |
| invariant-rev | [Invariants](task-review-invariants.md) | FAIL; shared-state/value conservation, units/counts, intervals, ACK/capture populations and bounds |
| repo-rev | [Standards](standards-review.md) | FAIL; all 47 changed paths mapped to repository/architecture rules, dependency and rustdoc/test/evidence gates |
| maintainer-rev | [Maintainer](maintainer-review.md) | PASS within scope; existing-owner comparison, workflow reuse, abstraction/conversion and caller closure |
| system-rev | [System](system-review.md) | FAIL; deployed startup, listener/snapshot failure, raw setup/poll/drain and replacement/recovery paths |
| domain-rev: concurrency | [Concurrency](domain-review-concurrency.md) | PASS within scope; serialized updates/cuts, handle lifetime, install publication, atomic timing parts and lock order |
| ponytail-rev | [Independent validation](findings-validation.md) | FIX_REQUIRED recommendation; all six reports complete, five retained findings, no missing specialist or new blocker |

Separate security/RBAC and data-layer domain reviewers were not applicable: the
range changes no identity/permission/tenant selection, transaction/RLS/schema or
durable-write policy. Existing local-exposure/privacy and capture/ACK boundaries
were traced in behavior, standards, invariant and system coverage. The independent
validator confirmed these omissions and the selected specialists' completeness.
No missing selected report blocks this review.

## Reconciled acceptance matrix

The [validation report](findings-validation.md#reconciled-acceptance) provides the
source-level reconciliation; this table records the acceptance result after that
validation, not a vote among reviewers.

| Original obligation | Reconciled evidence | Result |
|---|---|---|
| REQ-001/002: sole backend, exclusive startup, benchmark opt-in/locality, normal production monitoring | Runtime/backend startup before boot, local router/config tests and separate system/concurrency tracing | PASS by source/test inspection |
| REQ-001/003: one semantic timing observation and per-request residual | Gate, Scribe, Gateway and Verification producer branches and retained producer/interval proofs | PASS by source/test inspection |
| REQ-001/004, AC-001: preserve real diagnostic histogram units/populations | Supported Scribe/Forge fractional ratios and decimal cost hit integer-only recorder rejection | FAIL — FIND-TASK-001-1 |
| REQ-003: measured interval/cohort/reset/replacement evidence | Cumulative snapshot/interval and Population validation paths, retained tests and historical serialization | PASS for implemented measured path; declared-capacity proof remains below |
| AC-002 / scenario 3: complete declared workload fits bounded storage | Overflow/small consumer/two-call journey proofs do not establish complete reachable series footprint | FAIL — FIND-TASK-001-2 |
| REQ-003/004 / scenarios 3–4: truthful raw setup/poll/drain evidence across all six workflows | Snapshot transport migration is complete, but numeric projection drops known refusal metadata before real consumers | FAIL — FIND-TASK-001-3; fractional semantics also finding 1 |
| INV-001–003: security, tenancy, capture/ACK, cancellation/settlement and accepted-work/durable ownership | Existing owners retained; instrumentation branches, production regressions and snapshot-only functional journey inspected | PASS for unchanged regression boundary; no independent runtime qualification |
| INV-004 / REQ-007: immutable readable history, declared settings | History files unchanged, new fields defaulted, retained history/report tests and README method declaration | PASS by source/test inspection |
| Mandatory changed-item rustdoc | Missing error/ownership docs and links to removed activation API | FAIL — FIND-TASK-001-4 |
| REQ-009 / AC-005: candidate disposition and required execution evidence | Inventory/command record exists; independent diagnosis of one already-applied fixture repair is absent | FAIL — FIND-TASK-001-5 |
| Non-goals and justified reuse | No optimization, new scheduler/admission/durable service or duplicate whole collection workflow; standard-recorder/HDR consolidation justified | PASS |

REQ-005/006/008 and AC-003/004 are assigned to TASK-002, not prematurely accepted
or failed here. Limited maintainer/concurrency PASS results do not override the
separate functional/proof/gate failures.

## Validated finding ledger

Only independently confirmed or revised findings are retained. Full diagnoses,
producer-to-consumer evidence, exact locations, materiality and minimum correction
burden are in [findings-validation.md](findings-validation.md).

| Stable ID | Discovery IDs | Validation | Classification and correction boundary |
|---|---|---|---|
| FIND-TASK-001-1 | B-001, I-1 | REVISED | INCORRECT: valid fractional non-time observations are rejected. Correct bounded fixed-point HDR scale/unit representation in the sole recorder/snapshot owner; preserve durations, counts and genuine invalidity. |
| FIND-TASK-001-2 | B-002, I-2 | REVISED | MISSING: required finite declared-workload capacity proof. Add source-backed, in-process per-server-lifetime footprint/no-drop proof; do not infer actual overflow or require a benchmark run. |
| FIND-TASK-001-3 | Y-001 | CONFIRMED | INCORRECT: known dropped/rejected evidence is erased by raw conversion. Reject it at the existing fallible snapshot-to-Metrics boundary and propagate through current collection Results, not per-engine guards or server policy changes. |
| FIND-TASK-001-4 | S-1 | CONFIRMED | VIOLATION: mandatory changed-item rustdoc. Correct targeted error/ownership/activation docs, comment-only. |
| FIND-TASK-001-5 | S-2 | CONFIRMED | VIOLATION: independent fixture-repair diagnosis record missing. Obtain one focused retrospective diagnosis and disclose its timing/evidence limits accurately; no weak assertion or product defect is inferred. |

The function-scoped-import observation and alternative layout/locking preferences
were not retained. They create no remediation work. No human-directed addition
was relabelled as an independently discovered finding.

## Follow-up and verification limits

[Claims comparison](claims-comparison.md) groups proposals by violated invariant.
No separate follow-up discovery was needed: corroborated proposals had traceable
sources, unique proposals were directly independently validated, and the measured
Population path versus raw setup/drain path distinction resolved the apparent
coverage difference. No validator-originated new blocker required a separate
source-validation pass. The future diagnostician in finding 5 closes a missing
execution record; it is not a vote on this review's findings.

All seven reviews were static. They inspected the implementation's claimed focused
passes, real-server journey and owner-suite result, but did not rerun them. No
runtime, build, benchmark, profiling, dependency-install or synthetic-load command
executed in this review. The actual recording bounds were not shown to overflow;
the required complete footprint proof is absent. No performance conclusion or
whole-experiment false-PASS/data-loss claim is made.

## Remediation handoff

[**TASK-001-R1 — Preserve benchmark values and evidence**](TASK-001-R1-correct-benchmark-evidence.md)
packages all five validated findings as one self-contained task for
`$wyrd-implement`. It selects bounded owner-level corrections, preserves adjacent
behavior and gives focused closure proofs. No `$wyrd-plan` redesign, new product
approval, full journey suite or benchmark run is needed to close these findings.
Review the immutable remediation candidate against this verdict, the prior
candidate and original task, carrying forward unaffected source-backed acceptance.

**Final verdict: FIX_REQUIRED.** No source change, task completion, merge, push or
deployment is authorized or performed by this review.
