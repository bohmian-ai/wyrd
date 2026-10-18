# TASK-003a verdict

**FIX_REQUIRED**

## Immutable subject

- Worktree: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
- Branch: `change/long-horizon-verification-task-001`.
- Base: `22335c2c40a5ad24c27bfa0d4686f4da49f922c5`.
- Candidate: `7cda4e93159ab328de2a74068d5e1978a2590818`.
- Complete cumulative range: `git diff 22335c2c40a5ad24c27bfa0d4686f4da49f922c5...7cda4e93159ab328de2a74068d5e1978a2590818`, 54 files, +2214/-2055.
- Approved specification: `changes/active/long-horizon-verification/spec.md`, revision 17.
- Original approved task: `changes/active/long-horizon-verification/tasks/TASK-003a-simplify-grouped-verification.md` at base. Candidate appendix is evidence, not behavioral approval.

The architecture remains approved. The corrections are bounded implementation,
proof and explicit human-directed cleanup; no new specification decision or
safeguard is needed. Source stayed immutable. Only this report directory was
added; required deletions are packaged for implementation, not performed during
this acceptance audit.

## Subsequent owner-approved amendment (revision 18)

After this immutable candidate audit, the owner replaced recommendations for
FIND-TASK-003a-1 and -2. The original independent reports remain historical and
unchanged. For remediation, revision-18 spec and the updated R1 task govern:

- Retryable runtime/configuration/blocked-enqueue failures retain pending; actual
  owner inactivity/retirement or auth rejection records a terminal errored group
  execution and consumes pending atomically. Failed error-recording rolls back.
- Reject group_end.after above 365 fixed days at write time for standalone and
  inline Triggers; delete the hidden SQL cap and preserve accepted durations.

Independent review of the rewritten task is Ready (architecture/executability/
independent rehearsal PASS; no Critical/Major findings):
[owner-amendment readiness review](../../../../../.dev/review/task003a-r1-owner-amendment/review.md).
This approves task startability, not the unmodified implementation.

These are explicit human-directed revisions, not retroactive independently
validated findings. Findings #3–#6 and cleanup retain their existing dispositions.
The historical ledger/matrix below describes revision-17 candidate acceptance;
the updated task carries amendment proof and truthful pre-admission state/authority
constraints. No implementation has been performed by updating these documents.

## Independent review results

| Fresh role | Report | Discovery result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL: B-01–B-05 |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL: INVREV-001–003 |
| Repository standards | [standards-review.md](standards-review.md) | FAIL: STD-001 |
| Maintainer | [maintainer-review.md](maintainer-review.md) | PASS: no material findings |
| System resilience | [system-review.md](system-review.md) | FAIL: SYS-001 |
| SQL/data | [domain-review-data.md](domain-review-data.md) | FAIL: DATA-001 |
| Security | [domain-review-security.md](domain-review-security.md) | PASS: no material findings |
| Focused cleanup follow-up | [followup-review.md](followup-review.md) | RESOLVED: HUMAN-CLEANUP 1–3 |
| Structured Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED: six retained findings; separate required cleanup |

## Claim reconciliation and follow-up decision

[claims-comparison.md](claims-comparison.md) compares shared causes. The owner's
additional stale/unused/retired-code request triggered a fresh focused follow-up.
Scoped maintainer/security PASS reports do not contradict defects at other
boundaries. No remaining material conflict or untraced shared cause requires a
further discovery pass. The fresh validator read every report, complete diff,
authorities, affected full function bodies and producer/caller/sibling paths.

Validation consolidated pending-retention and C13 proof causes, and revised the
duration correction to include the live long-wait fixture while rejecting a new
infinity/extreme-duration policy. Rejected portions and non-material preferences
are omitted from remediation. Root introduces no independent findings.

## Validated ledger

| Stable finding | Disposition / discovery IDs | Consequence and correction |
|---|---|---|
| FIND-TASK-003a-1 | CONFIRMED: B-01, INVREV-001 | Unavailable owner/authority or refused enqueue deletes the only pending item without execution. Retain it through rollback; consume only new/identical successful handoff. |
| FIND-TASK-003a-2 | REVISED: B-02, INVREV-002, DATA-001 | Private cap shortens representable authored after. Delete cap/bind/shortening and adapt the live long-wait fixture, retaining no-due/neighbor-progress proof. |
| FIND-TASK-003a-3 | CONFIRMED: B-03 | Separate parity tests omit the required shared three-runtime conversation. Extend existing Rust C1 with real foreign SDK processes. |
| FIND-TASK-003a-4 | REVISED: B-04, INVREV-003, SYS-001 | C13 omits actual cron admission during blocked group read, query observations and writer/ACK race proof. Strengthen current C13 with existing controls. |
| FIND-TASK-003a-5 | CONFIRMED: B-05 | Python/TS pressure/retry source is disconnected from group readback. Use the same bound source/group following existing Rust pattern. |
| FIND-TASK-003a-6 | CONFIRMED: STD-001 | Mandatory changed-item rustdoc missing. Document existing contracts with the same remediation. |

[findings-validation.md](findings-validation.md) records exact source locations,
violated obligations, producer/caller traces, observable consequences, discovery
dispositions, selected minimal corrections and focused proof. The self-contained
remediation below reproduces diagnoses and recommendations.

Separate required cleanup (no retrospective FIND IDs): delete three unused
DueGroup fields and SQL projections; remove stale descriptions; atomically
replace six live group fixture names, related helper/local terms and webhook
expectations. Preserve useful assertions and live shared queue/cron/source/authz/
Scribe/query owners. These additions are explicit owner requirements.

## Reconciled acceptance matrix


| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| A1 after-only contracts, six facts, owner identity, no-IO producers | Trigger/registration and C1/C7 supplied evidence; shared SQL shortens after; mixed-runtime story absent | FAIL — FIND-2/3; other sub-obligations PASS |
| A2 visible input/result union, identity dedup/order/null alignment/failed values and immutable pin | Full Sources/Visible/Payloads/PinnedGroup, queue claim reconstruction; C3/C6/C8 supplied evidence | PASS |
| A3 digest idempotence, overlapping distinct snapshots, remove coverage/finality/discovery | Queue digest/conflict index and no outstanding gate; complete removal diff; C6/C8/C9 | PASS |
| A4 settled-order baseline/equal/change/recovery/replay | Binding lock, COMPLETE replay flag, predecessor/dispatch SQL; C2/C9 | PASS |
| A5 coding originals, no-trace source values and optional authorized live Agent queries | Existing hosted/query boundary retained; C3 | PASS |
| A6 normal 20/21-frame evaluation, failed/empty and per-item retry | Ceiling configuration/helper/runner bypass removed; C4/C10 and retained Rust queue-retry story | PASS |
| A7 tenant/owner/group/exact-source isolation, mixed runtime, ordinary ungrouped behavior | TenantConn, source tuple predicates, authenticated owner; C1/C5; missing composed C1 | FAIL — FIND-3 only; isolation/ungrouped PASS |
| A8 scheduled samples and staging/neighbor progress retained, group retry readback | ClaimLoop shared capacity and retained schedules; C11/C12; Python/TS disconnected group tails | FAIL — FIND-5; retained ordinary behaviors PASS |
| A9 minimal wire/stub/generated/OpenAPI/MCP/docs closure | Source contracts and declaration diff; C7/C10 and reported static lanes | PASS for shape/removed fields; stale descriptions/terminology separately require HUMAN-CLEANUP 2/3; no public ceiling permitted by FIND-2 |
| A10 credible original functional scenario closure, benchmark deferred | Supplied C1–C13 plus independently rerun C13; missing composed/asserted proofs | FAIL — FIND-3/4/5; benchmarking correctly pending |
| A11 ACK updates, RLS one row, transactional success/rollback, arrival and capacity/cron bounds | Scribe/queue/scheduler full paths and C8/C13; unwanted consumption plus omitted writer/read/cron assertions | FAIL — FIND-1/4; bounds/schema/normal success source-proven |
| INV-001–005 and repository gates | Existing Rust owners, accepted query authority, snapshot inputs and finite permits retained | FAIL only for FIND-1/2 and explicit rustdoc FIND-6; no separate security/tenant finding |
| Non-goals and accepted limits | No new recovery/visibility/adoption/quota engine or benchmark machinery in range | PASS; REQ-015/AC-009 intentionally unqualified |


## Verification and limits

[verification.md](verification.md) records root's independently traced C13 rerun:
managed Postgres setup, Wyrd/Vala migration-idempotence tests (one passing each),
then exactly one selected server test passing in 4.480 seconds. This proves current
assertions execute, not missing observations. Other C1–C13/static outcomes are
candidate-supplied evidence assessed against source/assertions, not fresh runs
of all SDK suites. No source edit, broad gate, provider call, stress, deployment
outage qualification or benchmark was performed. Candidate identity and report
structure were checked at completion.

Accepted ACK/visibility, post-ACK crash signal loss, outbox/retention loss,
continuous postponement, overlap/settlement-order and durable-backlog limits are
not findings. REQ-015/AC-009 remain intentionally pending. Functional acceptance
has not passed; this verdict does not hand off a passed task to benchmarking.

## Prior closure and next action

This is the first TASK-003a audit. No prior TASK-003a FIND IDs require closure;
superseded TASK-001 packets were not revived. Preserve these stable IDs on the
next review and audit the complete cumulative candidate against the original task.

Implement one task:
[TASK-003a-R1-retain-pending-work-and-remove-stale-code.md](TASK-003a-R1-retain-pending-work-and-remove-stale-code.md),
then return the committed candidate to `$wyrd-task-review`.
Review does not implement, merge, push or deploy.
