# TASK-003a / R1 cumulative re-review verdict

**FIX_REQUIRED**

## Remediation scope amendment — 2026-10-19

The immutable review below concerns 3d0b301c5 and approved revision 18. It is
historical evidence, not a verdict on current HEAD 8376bb2aa. After inspecting
later source and discussing findings 1, 7 and 8 with the owner, the linked R2
task now targets approved revision 20 and retains only FIND-TASK-003a-2,
FIND-TASK-003a-6 and FIND-TASK-003a-7. FIND-1 is resolved in source by removing
the generic-403 terminal classification; FIND-8 is superseded by revision 19's
internal snapshot-read authority. Revision 20's missing-input retry/deadline
addresses completeness and does not close FIND-7's database-shape constraint.
No new cumulative review or test run is claimed by this amendment. Follow the
updated R2 task rather than the historical correction instructions below.

## Immutable subject

- Repository/worktree: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
- Branch: `change/long-horizon-verification-task-001`.
- Cumulative base: `22335c2c40a5ad24c27bfa0d4686f4da49f922c5`.
- Prior candidate/latest-fix base: `7cda4e93159ab328de2a74068d5e1978a2590818`.
- Candidate: `3d0b301c51cfe6e50cbbde41b8b2ee3dfc2e9fb4`.
- Complete acceptance range: cumulative base through candidate (84 files, +5256/-2317, including prior reports); latest fix locator spans 56 files, +3263/-483 including reports. No source archive/diff is stored here.
- Approved specification: `changes/active/long-horizon-verification/spec.md`, revision 18.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-003a-simplify-grouped-verification.md`, approved original body at cumulative base.
- Required prior inputs: `changes/active/long-horizon-verification/review/task-003a-20261018-7cda4e931/{verdict.md,findings-validation.md,TASK-003a-R1-retain-pending-work-and-remove-stale-code.md}`.

The candidate and all implementation/spec/task/fixture/generated source stayed
immutable. Only this new review-report directory was added. No implementation,
commit, merge, push or deployment occurred. Revision-18 owner amendments override
the original blanket-retention and unrestricted-duration recommendations.

## Independent results

| Fresh required role | Report | Discovery result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | PASS; empty proposed ledger |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | PASS; empty proposed ledger |
| Repository standards | [standards-review.md](standards-review.md) | FAIL; STD-001 |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL; MREV-R1-01 |
| System resilience | [system-review.md](system-review.md) | FAIL; SYS-R1-001 |
| Data/persistence/concurrency | [domain-review-data.md](domain-review-data.md) | FAIL; DATA-1/2 |
| Security | [domain-review-security.md](domain-review-security.md) | FAIL; SEC-001 |
| Focused follow-up | [followup-review.md](followup-review.md) | RESOLVED; narrow constraint and owner-grant closure conflicts |
| Structured Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; five retained findings |

## Reconciliation and follow-up decision

[claims-comparison.md](claims-comparison.md) groups proposed findings by authority
and shared cause. General PASS/closure claims from the implementation reviewers
conflicted with concrete owner-grant and selected-schema evidence. A fresh
focused follow-up traced the unchanged journey's actual source writes, denied
handoff, readback and role swaps; it also resolved the explicit NULL-authority
constraint obligation versus unsupported privileged-mutation hardening. Both
uncertainties were source-resolved; no additional cause or architecture decision
was introduced.

The fresh validator independently read every report, original task, R1/current
spec, actual cumulative diff and affected bodies/callers/writers. It retained
only source-backed corrections and rejected broader legacy-input hardening,
new peer fault/topology machinery, a global timezone policy, optional import/
naming notes, C13 timing preferences and accepted-limit repairs. It revised the
rustdoc correction to reflect verbatim SourceText strings, not fabricate grammar
validation in the decoder. Discovery agreement was not treated as proof and
rejected proposals are not included as optional remediation advice.

## Validated finding ledger

| Stable ID | Disposition / sources | Consequence and selected correction |
|---|---|---|
| FIND-TASK-003a-1 | REVISED, partially open; SYS-R1-001 | All 403 query errors are treated as owner denial, including typed peer/fence infrastructure rejection. Match actual permission-denial variants and retain peer failures through existing Read/rollback. |
| FIND-TASK-003a-2 | CONFIRMED residual, partially open; DATA-2 | SQL adds calendar-day intervals despite fixed elapsed-day parsing; accepted 1d can differ from 24h across DST. Add elapsed seconds at the same SQL owner, preserving the one-year write bound. |
| FIND-TASK-003a-6 | REVISED, partially open; STD-001/MREV-R1-01 | New InputSlot visitor misses mandatory docs/Errors sections. Document actual formatter/map/string behavior without changing it. |
| FIND-TASK-003a-7 | REVISED, new; DATA-1/follow-up B | The explicitly selected authority-free terminal exception admits non-NULL inputs/malformed snapshot fields. Require the selected absent fields in that existing exception only; current writer is safe and no public exploit is claimed. |
| FIND-TASK-003a-8 | CONFIRMED, new; SEC-001/follow-up A | Existing owner-grant journey fails and its trace-only phase is vacuous after terminal consumption. Inspect terminal errors and author new source activity for later denial/success stages, keeping production policy. |

Exact locations, violated authority, full producer/sibling/caller traces,
observable consequences, minimal corrections, discovery dispositions and focused
closure proof are in [findings-validation.md](findings-validation.md) and the
self-contained R2 task. No new product, public API, persistent-shape or concurrency
choice is needed; all selected corrections enforce existing approved contracts.

## Prior finding and cleanup closure


| Stable finding | Closure status | Source-grounded assessment |
|---|---|---|
| FIND-TASK-003a-1 | PARTIALLY OPEN | Rev18 replaces old blanket-retention advice. Stored inactive owner identity, typed AuthorityRefusal, terminal insertion plus consumption and recording-failure rollback are implemented. Status-403 query classification still loses the required cause as above. |
| FIND-TASK-003a-2 | PARTIALLY OPEN | Shared write-time fixed-year validation, both Trigger forms, direct raw HTTP negatives, exact authored text, clamp removal and `365d` long-wait fixture are implemented. Fixed elapsed-day arithmetic remains incomplete. |
| FIND-TASK-003a-3 | CLOSED | Grouped::foreign_turn (`Rust journey:1329–1390`) executes actual uv/Python and Node public SDK processes against the host's common bundle/endpoint/key/group, with isolated config home. C1 includes their two values in the six exact facts. No embedded foreign interpreter lifetime. Supplied P3 evidence supports this observation. |
| FIND-TASK-003a-4 | CLOSED | C13 now preserves an armed query fault across starved/idle passes, observes a real source ACK's SQL upsert waiting on the held row, asserts fresh pending, races claimers, proves 32-item bounds, and runs actual scheduler.run with cron made due after observable query stall. Durable cron admission is required before cancellation, and pending rollback remains asserted. Root P1 selected/passed one. |
| FIND-TASK-003a-5 | CLOSED | Python C11 (`:1007–1100`) and TS C11 (`:1011–1121`) register eligible support before pressure, refuse/retry the same source/group and assert exactly the accepted pinned value (TS also its execution ID). Neighbor and ordinary staging/count assertions remain. Supplied runtime-owned P5/P6 evidence applies. |
| FIND-TASK-003a-6 | PARTIALLY OPEN | Original cited Firsts/Visible/activity sites repaired; new visitor remains within the same changed-item gate, not a new ID. |

The new schema exception and existing owner-grant caller regression are distinct causes and receive next unused IDs -7/-8. They are not retroactive human additions or per-caller duplicates of -1.

## Separate owner-directed cleanup closure

| Human addition | Result | Independent source evidence and preservation |
|---|---|---|
| HUMAN-CLEANUP 1 | CLOSED | DueGroup now contains binding/group, owner Card/settings/principal and active only (`verifier_executions.rs:1000–1015`). DUE_GROUP_SQL returns binding/group; GROUP_BINDING_SQL returns owner/settings. Pending due_at storage/index/filter/order, binding/source exact matching, enqueue resolve_binding/readiness, ordinary DueBinding cron state remain live. |
| HUMAN-CLEANUP 2 | CLOSED | Runner dispatch describes ordinary pinned lists and has no ceiling bypass; guide removed only-automatic-schedule assertion; MCP media describes selected provenance. Current source agrees. No retired behavior is restored. |
| HUMAN-CLEANUP 3 | CLOSED | Six fixture basenames/metadata/reference callers and refund webhook were changed together; active-source search found no remaining targeted old grouping names/helpers. Rust/Python/TS/MCP readback and negatives remain; coding-child absence uses actual Verifier kind. Three intentionally invalid fixtures remain intact in fixtures/invalid with active rejection callers. No source grammar/version/auth change was needed for renames. |


## Reconciled cumulative acceptance matrix


| Obligation | Implementation and proof assessed | Result |
|---|---|---|
| A1 after-only Trigger, refresh, exact duration, six facts, producer attribution/no SQL | Trigger/shared decoders, binding projection, Scribe activity SQL; C1 three-process six-value proof, raw HTTP bounds, P7/C7/P8 | FAIL only for fixed-day exactness (-2); other sub-obligations PASS |
| A2 visible input/result union, deterministic earliest logical dedup/latest result selection, aligned null/failed values, immutable inputs | Full Sources/Visible/Earliest/Payloads/pin bodies and queue reconstruction; C8/C3/C6 supplied evidence | PASS |
| A3 digest idempotence, equal-time/new IDs, distinct overlap, retired discovery/finality/coverage removed | PinnedGroup JCS/SHA-256, exact digest conflict lookup and index, pending-only poller, cumulative deletion/caller search; C8/P2/P4 | PASS |
| A4 settled-order baseline/equal/change/recovery and replay-safe Operators | complete, binding lock, COMPLETE/PREVIOUS_GROUP SQL and existing dispatch identity; P2/C2 supplied evidence | PASS |
| A5 original coding/no tracing, optional authorized live query and effective denial regression | Normal hosted/query boundary preserved, coding/negative fixtures and C3; existing owner-grant journey source and traced failure | FAIL for owner-grant regression (-8); coding behavior PASS |
| A6 ordinary 20/21 media evaluation, empty/failed selections, per-item retries | Ceiling config/helper/runner bypass removed; ordinary Grouped engine; retained media/MCP cases and C4/C10 evidence | PASS |
| A7 tenant/owner/group/version isolation, mixed runtimes, ordinary ungrouped | TenantConn/RLS, frozen exact tuple/owner/group predicates, C1 composed subprocesses and C5/C8 | PASS |
| A8 scheduled samples/checks, bounded staging/retry/drain and neighboring tenant progress | Unchanged cron sibling, shared ClaimLoop permit ownership, Scribe schedule-before-reservation; C11 corrected runtime-owned tails and retained C12 | PASS |
| A9 thin contracts/SDK typing/schema/MCP/docs and truthful terminal readback | Minimal GroupSnapshot, authored Python/source-generated TS, source decoding parity and status authorization; supplied codegen/runtime contract proof | FAIL only for narrow persisted exception contract (-7); ordinary projection/terminal writer behavior PASS |
| A10 original functional closure with credible focused proof, benchmarks deferred | Cumulative scenario source/evidence and strengthened P1/P2/P3/P5/P6; root exact owner-grant failure | FAIL for -8 and unclosed -1/-2/-7; no benchmark claim |
| A11 ACK scheduling, consumption/rollback/arrival, bounds/capacity/cron progress | Sole Scribe writer and queue/scheduler transaction bodies; actual C13 proof and root P1 | FAIL only for peer-error misclassification (-1); other concurrency/atomicity sub-obligations PASS |
| INV-001–005 / repository constraints | Existing server owners and authority, RLS, immutable pinned values, finite shared permits; nullable terminal persistence exception and rustdoc gate | FAIL for -1/-2/-6/-7; no separate authority bypass or new owner |
| Accepted limits and non-goals | No visibility barrier, source-terminal wait, coverage inventory, recovery ledger, hard finality, custom source ceiling, adoption/serialization or quota machinery | PASS; REQ-015/AC-009 intentionally remain pending |

PASS rows are source-level task acceptance supported by supplied focused evidence; they are not a claim of fresh full deployment qualification. General discovery PASS matrices cannot erase concrete counterexamples in FAIL rows.


## Verification and limits

[verification.md](verification.md) separates root runs, coordinated domain SQL
proof and supplied implementation evidence:

- Root exact one-year parser test selected/passed one.
- Root pending scheduling P1 and snapshot/settlement P2 selected/passed one each,
  sequentially in managed Postgres; setup migration-idempotence tests passed.
- Coordinated data reviewer ran one isolated rollback-only managed-PG session:
  actual-schema CHECK clone admitted the specified malformed exception states;
  fixed-date timezone arithmetic demonstrated 1d=82800 versus 24h=86400 seconds
  in New York spring DST. This is no public exploit/FK/RLS proof.
- Root exact existing owner-grant journey selected one and failed twice. Full-trace
  capture shows a real terminal guarded RBAC error then the actual 90-second
  completed-execution assertion; second run 109.997s, exit 100. The actual trace
  was read before its cause was correlated with the independent source diagnosis.
- Read-only cumulative diff check passed. No source/test/timeout was altered to
  obtain results. Other SDK/MCP/static results are supplied evidence compared
  with source/assertions, not fresh full-suite runs.

No broad gate, full SDK suites, live providers, host stress, rolling deployment
fault, capacity benchmark or performance qualification was run. Existing accepted
visibility/signal-loss/outbox/retention/postponement/overlap/backlog/fairness limits
remain accepted; REQ-015/AC-009 are intentionally pending. The current functional
task has not passed acceptance, so this verdict does not approve benchmarking
handoff as a completed task or finalize the integrated change.

## Single remediation handoff

[TASK-003a-R2-close-grouped-verification-gaps.md](TASK-003a-R2-close-grouped-verification-gaps.md)
packages only the five validated findings with diagnoses, selected existing owners,
constraints, acceptance criteria and exact narrow proof. Route directly to
`$wyrd-implement`, then reassess the complete cumulative candidate against original
TASK-003a plus R1/R2 and spec revision 18. Closed -3/-4/-5 and cleanup stay closed
only with source evidence; do not recreate them as new issues per caller.
