---
task: ORACLE-LOCAL-T01-R3
verdict: PASS
base: d3888ddae83c833c3eb85edc0ce226eb6debadce
candidate: 6e87c3b2f04486c1ad7e2e923ffd1e0cfe84e1b5
---

# Oracle local-admission R3 cumulative re-review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`
- Approved specification: `changes/active/oracle-local-admission/spec.md`, revision 2
- Original task: `changes/active/oracle-local-admission/tasks/01-simplify-oracle-admission.md`
- Prior verdicts and remediation tasks: all artifacts under
  `changes/active/oracle-local-admission/review/task-review-01/`,
  `task-review-r1-rereview-01/`, and `task-review-r2-rereview-01/`
- Base: `d3888ddae83c833c3eb85edc0ce226eb6debadce`
- Cumulative candidate: `6e87c3b2f04486c1ad7e2e923ffd1e0cfe84e1b5`
- Reviewed range: `d3888ddae..6e87c3b2f`

The separately authorized F1 OLAP cleanup and its accepted SQL test correction
are not R3 scope drift. The implementation completion summary was not used as
evidence. The candidate remained unchanged throughout review.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-001 — PostgreSQL-free pod-local admission | The cumulative candidate retains the reviewed deletion of the durable admission owner and its production consumers | Prior boot/query evidence and cumulative source inspection | PASS |
| REQ-002 / AC-003 / AC-R3-001 — every grant decision rejects waiters already past their absolute deadline | `oracle/admission.rs:1090-1202` reads `Instant::now` at the start of every loop iteration before pruning and before selecting or acquiring resources | Direct review reran `waiter_expiring_between_grant_decisions_is_never_granted`; it passed | PASS |
| AC-R3-002 — hidden-tail expiry, FIFO, rotation, floor protection, follower wakeup, cancellation, and later progress remain intact | The correction changes only the decision-time source; queue selection, cursor movement, class accounting, rollback, and notification remain shared | Direct review reran all eight named R3 tests; 8/8 passed | PASS |
| REQ-003 / REQ-005 / AC-R3-003 / AC-R3-004 — admission charges slots and scratch, while governed memory begins at shared-pool growth | `resources.rs:6875-6891` and `oracle/dispatcher.rs:1112-1120` now describe the implemented accounting model; their R3 edits are comment-only | Idle leader/follower, shared-root, and atomic grant tests passed; recorded resource-governance, formatting, and lint checks passed | PASS |
| REQ-004, REQ-006, REQ-007, and REQ-008 — capacity sharing, bounded workers, lifetime ownership, and telemetry remain intact | R3 changes no class selection, worker selection, resource expression, terminal owner, metric, or public contract | Cumulative prior evidence plus direct focused regression run | PASS |
| R3 non-goals — no timer owner, background task, generalized deadline queue, scheduler framework, public clock/configuration, dependency, migration, metric, or protocol change | The private generic helper has exactly one production wrapper fixed to `Instant::now` and one deterministic test caller; it adds no stored, configurable, or externally injectable clock | Diff, call-path, and manifest inspection | PASS |
| Ponytail minimalism — the correction and proof are the smallest maintainable closure | All production grant paths retain the existing `grant_waiters` owner; the one-line wrapper prevents clock injection at callers, while the helper avoids a wall-clock race without a trait, field, global hook, or fixture framework | Deterministic RED/GREEN evidence is recorded and the exact test passes | PASS |

## Adjudications

### Private clock seam

`grant_waiters_with_clock(..., impl FnMut() -> Instant)` does not violate the
architectural non-goal against an injected production clock. Production has no
clock field, trait, configuration, public parameter, or alternate source:
`grant_waiters` fixes the source to `Instant::now`, and every production caller
uses that wrapper. The second caller is the in-module deterministic test. This
is the minimum proof seam for advancing time between two decisions without a
wall-clock race; global hooks, a clock owner, or duplicated grant logic would be
larger and less isolated.

### Residual post-sample interval

The fresh monotonic sample at the top of an iteration is the grant decision's
linearization point under the admission mutex. A deadline passing after that
sample races with an already-valid atomic decision; checking again later would
only move the unavoidable interval. The existing async deadline branch and
rollback path still arbitrate delivery after the lock is released. This does
not violate AC-R3-001, which requires rejection when expiry occurs before the
waiter's own decision.

### Dispatcher comment

The correction at `oracle/dispatcher.rs:1112-1120` closes the same source
contract as FIND-ORACLE-LOCAL-T01-9. The old text said reservation charged
memory and slots together, while `try_acquire_worker` charges slots and derives
an unreserved memory ceiling. Correcting that directly contradictory statement
is required closure, not unrelated documentation cleanup.

### Test size and helper necessity

The test uses the existing queue owner, waiter constructor, resource snapshot,
notification path, and grant-drain helper. Its assertions directly cover the
required absence of notification and slot, scratch, active-query, tenant, and
stranded ownership plus later progress. Deleting the helper or test would leave
the specifically required mid-pass behavior without deterministic proof.

## Prior-finding closure

| Finding | Closure evidence | Result |
|---|---|---|
| FIND-ORACLE-LOCAL-T01-1 through FIND-ORACLE-LOCAL-T01-7 | Prior cumulative reviews closed the accounting, validation, selection, metrics/journey, feature-gating, and import findings; R3 does not change those surfaces | CLOSED |
| FIND-ORACLE-LOCAL-T01-8 | The clock is sampled for each grant decision, and deterministic proof advances it between two decisions in one pass | CLOSED |
| FIND-ORACLE-LOCAL-T01-9 | The cited resource-test wording and the directly equivalent dispatcher claim now describe slot/scratch admission and deferred shared-pool memory charging | CLOSED |

## Verification limits

- Direct review ran the eight exact R3 Redux tests together; all passed.
- Direct review also ran the new deadline test alone; it passed.
- `git diff --check d3888ddae..6e87c3b2f` passed.
- The remediation artifact records the full Redux library suite at 905/905,
  `check:bifrost-resource-governance`, `fmt`, and `lints` as passing. Those
  broader commands were not repeated during this review.
- Full Bifrost integration and journey lanes were intentionally not rerun; the
  R3 task narrows verification to the local timestamp and comment changes, and
  those lanes had already passed on the cumulative predecessor.

## Material findings

None.

## Verdict

`PASS`
