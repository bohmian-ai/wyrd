# TASK-003 R2 re-review verdict

**Verdict: FIX_REQUIRED**

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-verification-closeout-task3`.
- Base: `7f79fb3417db651adedac194ada8908f0a0372d7`.
- Candidate: `e2d324a916cfff25e2d362c7d7d2ab1c6aec74d9`.
- Approved spec: `changes/active/verification-closeout/spec.md` revision 3.
- Original task: `changes/active/verification-closeout/tasks/TASK-003-r4-canonical-support-desk-closeout.md`.
- Prior reviews and remediation: `review/task-003-r4-canonical-closeout/` and `review/task-003-r5-r1-re-review/`, including their verdicts, validated findings, and R1/R2 remediation tasks.
- The complete base-to-candidate diff was reviewed; `9a8f9f7ee..e2d324a91` located R2 changes. Candidate HEAD stayed fixed.

## Reconciled acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| Declared Service tables, optional activation, direct and continuous results, one shared non-blocking Scribe outbox | Cumulative Card, verification, result, outbox and SDK owners; original green gate and task journeys remain recorded | PASS |
| Gateway Run/Card UID scope or registry authorization before dispatch, capture by UID, no CardRef alias | Cumulative gateway ingress/invocation/capture; security domain review found no R2 regression | PASS |
| Exact Prompt gateway deployment, actionable refusal, same support-desk workflow in three SDKs | Three checked-in example deploy functions and collision journeys; R2 does not change them | PASS; FIND-1 stays closed |
| Late Scribe delivery from an audit staged before Scribe boots; Postgres roles rerun fails on SQL error; active UID prose | Previously validated journey, `ON_ERROR_STOP=1`, and active design passages remain unchanged | PASS; FIND-4, -5, -6 stay closed |
| Oracle shutdown waits for Oracle-owned governed and infallible child memory without waiting for sibling roles | `resources.rs` attributes Oracle headroom within the existing governor; admission shutdown sums Oracle governed and headroom bytes after arming both wakeups; focused headroom test recorded green | PASS; FIND-2 closed |
| Forge restart does not immediately reclaim unexpired same-owner work after an interrupted startup | Atomic quiescence entry and stop-token guard now precede startup; startup passes the captured predecessor policy to Claimed/Running and Prepared SQL; source reviewers found no surviving unsafe path | PASS in inspected source; required proof below remains open |
| Focused Forge recovery proof interrupts startup with a live plan or heartbeat and observes an unexpired recovery pass before asserting no renewal | New integration test aborts at the Prepared claim gate before heartbeat/plan spawn; a two-second wait does not observe a restarted recovery pass | FAIL; FIND-7 remains open as a proof gap |
| Mandatory rustdoc on changed Forge field and prior Rust items | Quiescence tuple field and prior fixture/entry point are documented | PASS; FIND-3 and -8 closed |
| No second governor, persistent Forge fence, supervisor policy change, compatibility alias or unrelated feature | R2 diff retains existing owners and changes no public contract | PASS |

## Independent review results and reconciliation

| Report | Result | Reconciliation |
|---|---|---|
| `task-review-behavior.md` | FAIL | Live-child interruption required by R2 is not exercised. |
| `task-review-invariants.md` | PASS | Source closes Oracle and Forge state invariants; test limit noted. |
| `standards-review.md` | FAIL | Fixed sleep can pass without observing a recovery pass; Rustdoc and SQL capability pass. |
| `maintainer-review.md` | PASS | No material maintainability finding. |
| `system-review.md` | PASS | No source-demonstrated runtime failure; test limit noted. |
| `domain-review-data.md` | PASS | Oracle attribution and Forge SQL/lease paths close in source. |
| `domain-review-security.md` | PASS | No security or tenant-authority regression. |
| `followup-review.md` | RESOLVED | Confirmed one combined Forge proof gap, not a new production defect. |
| `findings-validation.md` | Complete | Fresh Ponytail review independently retained only prior FIND-7, revised to a focused-proof finding. |

The follow-up was required because behavior and standards reviewers treated the proof gap as blocking, while invariant, system, and domain reviewers found production source closure. Independent validation reconciled those positions: the code appears correct, but the explicit R2 recovery proof remains unmet. The function-local Oracle test import is a non-blocking placement note.

## Validated finding ledger and prior closure

| ID | Status | Evidence or correction boundary |
|---|---|---|
| FIND-TASK-003-1 | Closed | All three example deploys and journeys check the exact gateway pair. |
| FIND-TASK-003-2 | Closed | Oracle headroom is attributed and included in shutdown, with focused child-memory proof. |
| FIND-TASK-003-3 | Closed | Prior required item docs remain present. |
| FIND-TASK-003-4 | Closed | Active observation-correlation prose uses Card UID. |
| FIND-TASK-003-5 | Closed | The roles rerun stops on SQL error. |
| FIND-TASK-003-6 | Closed | Audit is staged before Scribe joins in the late-Scribe journey. |
| FIND-TASK-003-7 | **Open, REVISED: MISSING focused proof and deterministic-test VIOLATION** | Interrupt startup after a plan or heartbeat is live; confirm a restarted worker completes an unexpired recovery pass, then assert no renewal and later lease-expiry recovery. Keep production recovery policy. |
| FIND-TASK-003-8 | Closed | The shared Forge quiescence field has rustdoc. |

Exact paths, producer-to-consumer tracing, proposal dispositions and the smallest proof boundary are in `findings-validation.md`. The bounded remediation task is `TASK-003-R3-forge-recovery-proof.md` in this directory.

## Verification limits

This read-only review did not rerun the recorded exact Oracle, Forge, SQL, format, lint or tenant-isolation checks. The R2 test's red mutation demonstrates it caught a bad owner policy in one scheduled run, but a passing run can still miss the unexpired recovery pass, and no recorded test combines live startup child work with interruption and same-owner restart.
