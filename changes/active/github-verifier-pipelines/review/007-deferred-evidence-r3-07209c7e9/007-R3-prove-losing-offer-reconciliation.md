---
id: TASK-007-R3
kind: remediation
status: proposed
spec: SPEC-github-verifier-pipelines
spec_revision: 7
requirements: [REQ-009]
parent_task: TASK-007
remediates: [FIND-007-3]
---

# Prove losing-offer reconciliation

Route directly to `$wyrd-implement`. Complete the one remaining regression-proof obligation; current production behavior is source-correct.

Approved spec: `changes/active/github-verifier-pipelines/spec.md`, revision 7.
Original task: `changes/active/github-verifier-pipelines/tasks/007-deferred-evidence-and-resumption.md`.
Original base: `16ea9694c34a5ec1867306bd5e34df62f0888611`.
Prior candidate: `74fc2190e255a3e3cf7515091ff45ab86a288f6b`.
Reviewed candidate: `07209c7e9af1664a30bf548e64be08937c4f88df`.
Prior remediation: `review/007-deferred-evidence-r2-74fc2190e/007-R2-finish-publication-regression-proof.md` relative to the packet root, including appended Obligations/Evidence and maintainer dispositions. Current independent evidence: this directory's discovery, validation, ledger check and verdict.

## Diagnosis

FIND-007-3 remains MISSING proof. Original REQ-009 requires identical accepted evidence to replay its canonical receipt. R2 required deterministic production-used receipt reconciliation proof, not an end-to-end timing race.

At `crates/wyrd/wyrd-server/src/components/verification/service/evidence.rs:145–162`, a publisher whose initially fresh offer loses custody rereads the canonical winner. After conflict and completeness checks, it transfers canonical acceptance into held state and clears its fresh offer. The result is Replayed at the first accepted timestamp. Both HTTP and MCP consume this service result.

The new `a_lost_race_replays_the_first_acceptance` at lines 403–442 constructs held state already containing the canonical acceptance and passes false for accepted-now. It tests final receipt formatting; it excludes the transition that repaired the defect. Removing either `held.extend(raced)` or `fresh.clear()` leaves this unit green while reintroducing the wrong offered timestamp or Accepted status in production. Its identity assertion compares fixture data with itself. Existing sequential public replay begins with held already populated and cannot cover this branch.

The current implementation is correct. The meaningful gap is regression coverage of the exact previously diagnosed supported competing-publication failure, not missing Docker execution or a new concurrency guarantee.

## Correction outcome and selected boundary

Reuse the existing publication owner, canonical Accepted records and receipt calculation. Complete deterministic owner-level proof from a losing fresh offer and canonical winner to the actual production-used reconciled receipt. Include the state transfer in the tested production boundary; do not manufacture its corrected outputs only in test setup. Keep private helper names, signatures and layout implementation-owned; a bounded local rearrangement for direct testing is sufficient if necessary.

Use distinct canonical first-accepted and later offered timestamps. Establish replay status and canonical first time while preserving source/content identity. The regression must detect omission of canonical state transfer and failure to remove the losing fresh offer. Keep the existing real custody/fence and sequential public replay evidence, rather than adding a timing-dependent connected race. Another test only of final formatting or source inspection would leave the diagnosed gap unchanged. This is one local proof correction and needs no new machinery.

Preserve adjacent conflict/completeness checks, ACK/unavailability semantics, subscriber authorization, immutable source/policy identity and wake-on-replay. No public API, dependency, new table/payload mirror, persistence owner, locking/idempotency protocol, test-only production hook, sleeps, probabilistic retries or general harness. No delegated runtime proof: FIND-007-2 is maintainer-rejected. FIND-007-1/4/5/6 stay closed. The accepted INPUT_INVALID mapping, attempt refund, CLI file behavior and restart contract are unchanged.

## Acceptance and focused verification

| Finding | Required observation |
|---|---|
| FIND-007-3 | A deterministic losing-offer regression traverses production-used reconciliation and answers Replayed at the earlier canonical first-accepted time with unchanged canonical identity. It would fail if canonical acceptance were not transferred or the losing offer remained fresh. |

Use the existing inline server unit-test owner and `mise` runner. The current exact selector is:

`mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=components::verification::service::evidence::tests::a_lost_race_replays_the_first_acceptance)'`

If the test is renamed, use its actual exact selector and record selected count. Run format and narrow applicable lint checks for the actual write set. Preserve existing connected custody/replay assertions and their recorded evidence; no full SDK, whole-task journey or broad aggregate rerun is prescribed. Docker is unavailable in the review sandbox; do not convert that environment limit into an additional requirement or claim fresh connected execution. Record source, fresh unit and carried-forward connected evidence separately. No commit, push or Git configuration.
