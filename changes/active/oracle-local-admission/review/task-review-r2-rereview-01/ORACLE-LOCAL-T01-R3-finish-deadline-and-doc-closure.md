---
id: ORACLE-LOCAL-T01-R3
title: Finish Oracle grant-deadline and memory-comment closure
kind: remediation
status: ready
skill: wyrd-implement
spec: changes/active/oracle-local-admission/spec.md
task: changes/active/oracle-local-admission/tasks/01-simplify-oracle-admission.md
base: d3888ddae83c833c3eb85edc0ce226eb6debadce
reviewed_candidate: 6127c991004051ba3627a71a3dd091bae57b40e6
findings:
  - FIND-ORACLE-LOCAL-T01-8
  - FIND-ORACLE-LOCAL-T01-9
---

# Finish Oracle grant-deadline and memory-comment closure

Required execution skill: `$wyrd-implement`.

## Subject and authority

Remediate cumulative candidate
`6127c991004051ba3627a71a3dd091bae57b40e6` against approved specification
`changes/active/oracle-local-admission/spec.md` revision 2, original task
`changes/active/oracle-local-admission/tasks/01-simplify-oracle-admission.md`,
and the R2 task and verdict under
`changes/active/oracle-local-admission/review/task-review-r1-rereview-01/`.
A later acceptance review must reassess the complete range from base
`d3888ddae83c833c3eb85edc0ce226eb6debadce`.

## Issue diagnosis and intended correction

### Grant decisions compare deadlines to the pass-start time

FIND-ORACLE-LOCAL-T01-8 remains open. R2 correctly reuses the existing
FIFO-head pruning before each grant decision, which rejects a non-head waiter
that was already expired when the pass began. It does not make the comparison
current: `grant_waiters` captures one monotonic instant before the loop and
reuses it throughout the pass.

An arbitrary caller-derived deadline may expire while earlier iterations scan
tenants and acquire resources. A later iteration still considers that waiter
live, allocates slot and scratch ownership, increments active and tenant
counters, and schedules its notification. When the caller next polls,
`wait_for_grant` may choose the ready grant over the ready deadline branch.
This violates the fixed absolute wait bound and the requirement that deadline
eligibility be established at every decision.

Keep `OracleAdmission` as the only scheduler and reuse its existing absolute
waiter deadline and pruning/accounting mechanism. Each decision must compare
against current monotonic time so no waiter whose deadline has passed before
that decision can acquire resources or receive a notification. Do not add a
timer owner, background task, polling path, generalized queue, or scheduler
abstraction.

### A resource test still states the removed memory model

FIND-ORACLE-LOCAL-T01-9 remains open at
`crates/vala/vala-bifrost-redux/src/resources.rs:6887-6889`. The
`resource_grant_is_atomic_across_memory_and_scratch` comment says each query
charges one slot-unit memory quantum. Current admission charges no resident
memory: slot units own concurrency, scratch is leased separately, and actual
shared-pool growth performs the first governed memory charge.

This is not unrelated historical cleanup. The comment is in the Oracle
resource owner's own test and became false when the cumulative task removed
the class-memory debit. Correct the comment only; do not alter the test's
behavior or any resource expression.

## Decision-complete recommendation

Use the existing queue owner and pruning path to make expiry evaluation current
for each grant decision. This is the smallest root-cause correction because
every arrival, release, cancellation, refresh, and follower wakeup already
routes through the same grant pass. A guard in callers or a second timeout
owner would duplicate policy and leave sibling paths exposed.

Make the one remaining Oracle resource-test comment agree with the existing
runtime and corrected rustdoc. Do not broaden this into general comment cleanup.

## Constraints and preserved behavior

- Preserve per-tenant FIFO, equal-weight rotation, Interactive-floor
  protection, oldest-eligible class selection, work conservation, queue
  capacity, cancellation, and retryable `QueryAdmissionRejected` projection.
- Preserve the shared governor, shared Oracle memory root, private query
  ceilings, slots, scratch, and exact release/rollback behavior.
- Preserve all prior finding corrections and the F1 OLAP cleanup.
- Preserve authentication, authorization, tenant isolation, read-audit WAL,
  query-class derivation, result limits, terminal safety, and distributed
  cleanup.

## Non-goals

- No public contract, migration, dependency, configuration, metric,
  deployment, or distributed-protocol change.
- No generalized deadline queue, injected production clock, timer wheel,
  scheduler refactor, background task, or new fixture framework.
- No runtime memory-accounting change or unrelated documentation cleanup.

## Acceptance criteria

- AC-R3-001 / FIND-ORACLE-LOCAL-T01-8: every grant decision uses current
  monotonic deadline eligibility. A waiter that expires after a pass begins but
  before its own decision receives no slot, scratch, active-query, tenant, or
  notification ownership and is projected as the existing retryable admission
  rejection.
- AC-R3-002 / FIND-ORACLE-LOCAL-T01-8: the existing already-expired hidden-tail
  case, FIFO, tenant rotation, class floor, follower wakeup, cancellation, and
  subsequent progress remain intact.
- AC-R3-003 / FIND-ORACLE-LOCAL-T01-9: Oracle admission/resource source contains
  no claim that admission debits the 32/64 MiB class quantum; the cited test
  comment truthfully identifies scratch as its saturated resource.
- AC-R3-004: no runtime resource-accounting expression changes and idle
  leader/follower memory plus shared-root growth/release still reconcile.

## Focused proof and broader verification

Add or extend one deterministic in-module test so it distinguishes an entry
whose deadline expires between two grant decisions in the same pass from one
already expired before the pass. Prove it is never allocated or notified and
that counters and later live progress reconcile. Do not rely on a wall-clock
race.

Re-run the existing exact R2 hidden-tail, fairness, follower-wakeup,
absolute-deadline, idle-memory, and shared-root tests. Then run only the checks
covering the changed admission and resource-accounting surface:

```bash
mise run check:bifrost-resource-governance
mise run fmt
mise run lints
git diff --check d3888ddae83c833c3eb85edc0ce226eb6debadce..<new-candidate>
```

The full Bifrost integration and journey suites already passed on the cumulative
candidate and need not be repeated for this local timestamp correction and
comment edit. Record the focused RED/GREEN evidence, exact commands, targeted
check results, and the source search proving the stale class-memory admission
claim is gone.

## Remediation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-R3-001 — every grant decision uses current monotonic deadline eligibility | `oracle/admission.rs` — `grant_waiters` is now a one-line wrapper passing `Instant::now` to `grant_waiters_with_clock`, whose loop reads `now()` at the top of every iteration instead of once before the loop | `oracle::admission::tests::waiter_expiring_between_grant_decisions_is_never_granted` (RED/GREEN below) | PASS |
| AC-R3-002 — hidden-tail, FIFO, rotation, floor, follower wakeup, cancellation, and later progress intact | No selection, cursor, class-counter, floor, refusal, or notification logic changed; the new test ends by proving a waiter queued afterwards is still granted | `expired_waiter_behind_a_live_head_is_never_granted`, `local_admission_is_fair_and_work_conserving`, `follower_release_wakes_waiting_leader_without_reordering`, `production_admission_absolute_deadline_is_bounded`; full `vala-bifrost-redux --lib` 905/905 | PASS |
| AC-R3-003 — no source claims admission debits the class memory quantum | `resources.rs` `resource_grant_is_atomic_across_memory_and_scratch` rustdoc and inner comment now name slot units and scratch; `oracle/dispatcher.rs:1115` no longer says the participant charge is "memory and slot units together" | `resource_grant_is_atomic_across_memory_and_scratch` passes unchanged; source search below returns no stale claim | PASS |
| AC-R3-004 — no runtime resource-accounting expression changed | `resources.rs` and `dispatcher.rs` edits are comment-only; `admission.rs` changes only when the clock is read | `oracle_leader_and_follower_govern_no_memory_until_their_pools_grow`, `oracle_queries_share_one_governed_memory_root`, `mise run check:bifrost-resource-governance` | PASS |

### Focused proof

RED: with `now()` hoisted back above the loop (the reviewed candidate's shape,
the scripted clock read once), the new test fails at
`a waiter that expired mid-pass must never receive a grant` —
the tail is live at the pass-start instant and is granted.
GREEN: with the per-decision read, the tail is pruned at the second decision.
The clock is scripted (`start`, then `start + 2s`), not measured, so neither
direction depends on how long the intervening grant takes.

### Commands

```
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::admission::tests::waiter_expiring_between_grant_decisions_is_never_granted) + test(=oracle::admission::tests::expired_waiter_behind_a_live_head_is_never_granted) + test(=oracle::admission::tests::local_admission_is_fair_and_work_conserving) + test(=oracle::admission::tests::follower_release_wakes_waiting_leader_without_reordering) + test(=oracle::admission::tests::production_admission_absolute_deadline_is_bounded) + test(=resources::tests::oracle_leader_and_follower_govern_no_memory_until_their_pools_grow) + test(=resources::tests::oracle_queries_share_one_governed_memory_root) + test(=resources::tests::resource_grant_is_atomic_across_memory_and_scratch)'
  -> 8 passed
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib   -> 905 passed
mise run check:bifrost-resource-governance                            -> passed
mise run fmt                                                          -> clean
mise run lints                                                        -> clean
git diff --check d3888ddae83c833c3eb85edc0ce226eb6debadce             -> clean
git grep -n -iE "charge[sd]? .*(quantum|memory)|quantum of memory|admission (charges|debits|reserves).*memory" \
  crates/vala/vala-bifrost-redux/src/resources.rs crates/vala/vala-bifrost-redux/src/oracle/
  -> every remaining hit states the corrected model (slots/scratch charged, memory charged on pool growth)
```

### Scope

Write set is `oracle/admission.rs`, `resources.rs`, `oracle/dispatcher.rs`. No
public contract, migration, dependency, configuration, metric, deployment, or
protocol change; no timer wheel, background task, generalized deadline queue,
scheduler refactor, or fixture framework. The full Bifrost integration and
journey lanes were not rerun, per this task's verification scope.

One edit sits outside the cited location: `oracle/dispatcher.rs:1115` said the
participant reservation charges "memory and slot units together". Inspection of
`try_acquire_worker` shows it charges the slot ledger and derives a memory
ceiling without debiting it, so that comment carried the same retracted claim
AC-R3-003 exists to remove. It is corrected rather than left as a second copy.

### Material limits

- `grant_waiters_with_clock` exists only so a test can supply a clock that
  advances between two decisions in one pass. It takes `impl FnMut() -> Instant`,
  not a clock owner, trait, field, or configuration; production has one caller
  passing `Instant::now`. Without it, the mid-pass window is bounded by the real
  duration of a single grant and could only be probed by a wall-clock race,
  which this task forbids.
- Expiry is still evaluated once per decision, not continuously: a waiter that
  expires between the prune at the top of an iteration and the resource
  acquisition later in that same iteration is granted. That window is now a
  single grant decision under a held lock rather than a whole pass, and the
  caller still observes its own deadline downstream.
- `prune_expired` runs once per grant decision, so a pass issuing `g` grants
  across `t` tenants costs `O(g*t)` deadline comparisons, both bounded by
  configured slot units and the tenant ring.
