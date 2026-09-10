---
task: ORACLE-LOCAL-T01-R2
verdict: FIX_REQUIRED
base: d3888ddae83c833c3eb85edc0ce226eb6debadce
candidate: 6127c991004051ba3627a71a3dd091bae57b40e6
---

# Oracle local-admission R2 cumulative re-review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`
- Approved specification: `changes/active/oracle-local-admission/spec.md`, revision 2
- Original task: `changes/active/oracle-local-admission/tasks/01-simplify-oracle-admission.md`
- Prior verdict: `changes/active/oracle-local-admission/review/task-review-r1-rereview-01/verdict.md`
- Remediation task: `changes/active/oracle-local-admission/review/task-review-r1-rereview-01/ORACLE-LOCAL-T01-R2-close-deadline-and-doc-gaps.md`
- Base: `d3888ddae83c833c3eb85edc0ce226eb6debadce`
- Cumulative candidate: `6127c991004051ba3627a71a3dd091bae57b40e6`
- Reviewed range: `d3888ddae..6127c9910`

The separately authorized F1 OLAP cleanup, including its accepted SQL test
correction, is not R2 scope drift. The implementation completion summary was
not used as evidence.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-001 — PostgreSQL-free pod-local admission | The cumulative candidate retains the previously reviewed durable-owner and runtime-consumer deletion | Prior focused boot/query evidence and static cumulative inspection | PASS |
| REQ-002 / AC-003 / AC-R2-001 — expired non-head entries are rejected before ownership | `oracle/admission.rs:1093-1181` re-runs the existing FIFO-head pruning on every loop iteration | `expired_waiter_behind_a_live_head_is_never_granted` passes and directly proves an entry already expired before the pass is not granted | PASS |
| REQ-002 / AC-R2-002 — absolute deadline eligibility is current at every grant decision | `oracle/admission.rs:1096-1109` captures one `Instant` before the multi-grant loop and reuses it for every iteration | No test distinguishes the pass-start time from a later decision time; FIND-ORACLE-LOCAL-T01-8 | FAIL |
| REQ-003 / AC-002 / AC-R2-004 — shared memory behavior remains unchanged | R2 changes in `resources.rs` are documentation-only; leader/follower admission still owns no governed memory before pool growth | Recorded shared-root and idle-memory tests plus `check:bifrost-resource-governance` pass | PASS |
| REQ-004 / REQ-005 — class sharing, floor protection, CPU, slots, memory, and scratch remain distinct | R2 does not change selection, cursor, class counters, floor logic, or the governor resource split | Recorded fairness, follower-wakeup, capacity, shared-root, and full Bifrost lanes pass | PASS |
| REQ-006 / REQ-007 — bounded workers and lifetime ownership remain intact | R2 changes no participant selection or terminal owner | Cumulative source inspection and recorded Oracle journey lane | PASS |
| REQ-008 / AC-005 / AC-R2-003 — source authority describes actual memory accounting | Changed rustdoc correctly describes slots as concurrency and shared-pool growth as the first memory charge, and the task reference is gone | Source-wide relevant-term inspection finds one contradictory resource-test comment at `resources.rs:6887-6889`; FIND-ORACLE-LOCAL-T01-9 | FAIL |
| R2 non-goals and Ponytail minimalism — no new scheduler, timer, framework, dependency, contract, migration, or protocol | Runtime correction reuses the existing queue owner and pruning mechanism; one in-module test reuses existing helpers | Diff and manifest inspection | PASS |

## Material findings

### FIND-ORACLE-LOCAL-T01-8 — INCORRECT: grant decisions still use stale deadline time

- Violated obligation: REQ-002, AC-003, AC-R2-002, and R2's requirement that
  deadline eligibility be established for every grant decision.
- Location: `crates/vala/vala-bifrost-redux/src/oracle/admission.rs:1096-1109`.
- Evidence: `grant_waiters` reads `Instant::now()` once before its multi-grant
  loop. Each iteration calls `prune_expired` with that same pass-start value.
  A waiter whose deadline is live at pass start but expires while earlier
  tenant scans, grants, and resource acquisitions run therefore remains
  eligible and can receive slot, scratch, active-query, and tenant ownership
  after its absolute deadline. `wait_for_grant` then has both its grant and
  deadline branches ready, and its unbiased `tokio::select!` may accept the
  late grant. The implementation artifact records this behavior as a limit.
- Observable consequence: the queue can admit work after the request's fixed
  absolute admission deadline, transiently consume capacity, and return a
  successful admission where the bounded-wait contract requires retryable
  rejection.
- Required correction: retain the existing `OracleAdmission` owner, waiter
  deadline, and pruning/accounting mechanism, but evaluate expiry against the
  current monotonic time for each grant decision. Add focused proof that
  distinguishes a deadline expiring between two decisions in one pass from a
  deadline already expired before the pass.

### FIND-ORACLE-LOCAL-T01-9 — VIOLATION: one source comment still documents the deleted memory debit

- Violated obligation: REQ-005, AC-005, AC-R2-003, and the original task's
  source-authority closure.
- Location: `crates/vala/vala-bifrost-redux/src/resources.rs:6887-6889`.
- Evidence: the `resource_grant_is_atomic_across_memory_and_scratch` test says
  a query "charges one slot-unit quantum of memory." The cumulative runtime
  deliberately removed that charge: admission owns slots and scratch, while
  actual shared-pool growth performs the first governed memory charge. The
  comment predates R2 but became false because of the cumulative task's memory
  change; it is inside the exact Oracle resource contract, not unrelated debt.
- Observable consequence: maintainers reading the resource owner's own test
  receive the opposite accounting model from the implementation and corrected
  rustdoc.
- Required correction: correct that comment to state that scratch, not memory,
  saturates the fixture and that admission itself owns slot units without
  debiting the class memory quantum. Do not change the test or runtime.

## Prior-finding closure

| Finding | Closure evidence | Result |
|---|---|---|
| FIND-ORACLE-LOCAL-T01-1 through FIND-ORACLE-LOCAL-T01-7 | The prior cumulative review found the accounting, capacity validation, class-capable selection, metrics/journey proof, feature gating, and import placement corrections sound; R2 does not regress them | CLOSED |
| FIND-ORACLE-LOCAL-T01-8 | The already-expired hidden-tail case is fixed, but a waiter expiring during the same pass is still compared to stale time | OPEN |
| FIND-ORACLE-LOCAL-T01-9 | Cited rustdoc and task reference are corrected, but one directly contradictory Oracle resource comment remains | OPEN |

## Verification limits

- This review directly ran the new exact expired-hidden-tail test; it passed.
- The remediation artifact records all six exact focused tests,
  `check:bifrost-resource-governance`, `verify:bifrost` 9/9 lanes, `fmt`,
  `lints`, and `git diff --check` as passing. Those broad lanes were not rerun
  during this review.
- The new test proves expiry before pass start. It cannot prove the remaining
  mid-pass case because it never advances the decision-time clock between the
  first and second grant decisions.
- `git diff --check d3888ddae..6127c9910` passed. The candidate commit remained
  unchanged throughout review.

## Verdict

`FIX_REQUIRED`
