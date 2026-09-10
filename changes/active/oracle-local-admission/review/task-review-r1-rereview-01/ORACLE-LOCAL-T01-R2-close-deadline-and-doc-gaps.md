---
id: ORACLE-LOCAL-T01-R2
title: Close Oracle queue-deadline and memory-contract documentation gaps
kind: remediation
status: ready
skill: wyrd-implement
spec: changes/active/oracle-local-admission/spec.md
task: changes/active/oracle-local-admission/tasks/01-simplify-oracle-admission.md
base: d3888ddae83c833c3eb85edc0ce226eb6debadce
reviewed_candidate: d3d379cd0b61e7dd1bb6a6e5fd227affa2823a8a
findings:
  - FIND-ORACLE-LOCAL-T01-8
  - FIND-ORACLE-LOCAL-T01-9
---

# Close Oracle queue-deadline and memory-contract documentation gaps

Required execution skill: `$wyrd-implement`.

## Subject and authority

Remediate the cumulative candidate
`d3d379cd0b61e7dd1bb6a6e5fd227affa2823a8a` against approved specification
`changes/active/oracle-local-admission/spec.md` revision 2 and original task
`changes/active/oracle-local-admission/tasks/01-simplify-oracle-admission.md`.
A later acceptance review must reassess the complete range from base
`d3888ddae83c833c3eb85edc0ce226eb6debadce`.

## Issue diagnosis and intended correction

### Expired non-head waiters can receive a grant

FIND-ORACLE-LOCAL-T01-8 violates the bounded absolute queue-wait contract.
`ClassState::prune_expired` intentionally examines only FIFO heads, and
`grant_waiters` invokes it only once before a loop that can grant multiple
entries. Because each request's deadline is
`min(caller_deadline, enqueue + max_queue_wait)`, deadlines are not monotonic
within one tenant FIFO. An older live head may conceal a later already-expired
entry. Once the live head is granted, the same scheduling pass can allocate
slot and scratch ownership to the expired entry; its oneshot and timeout are
then both ready, so the waiter may accept the late grant.

Keep the existing `OracleAdmission` owner, tenant FIFO, rotating cursor,
absolute waiter deadline, and rollback path. Make deadline eligibility part of
every grant decision in a multi-grant pass so an entry exposed after a prior
pop is removed/rejected before resource acquisition or notification. Reuse the
existing deadline field and pruning/accounting mechanism; no timer owner,
background task, polling loop, or scheduler abstraction is needed.

### Source documentation describes the deleted memory debit

FIND-ORACLE-LOCAL-T01-9 violates the required source-contract closure. The
runtime now correctly admits by slots and scratch while actual shared-pool
consumer growth performs the first governed memory charge. However,
`resources.rs` still calls `ORACLE_PARTITION_WORKING_MEMORY_BYTES` and
`OracleResourceRequest.memory_bytes` admission/root charges, the task-added
`oracle_worker_slots` rustdoc says each slot "actually charges" memory, and
`oracle/admission.rs` names "Task 01 accounting" in permanent production
documentation.

Preserve the existing public and internal shapes. Correct only the touched
documentation so the 32 MiB value is a sizing, minimum-grant, and
partition-planning quantum; slots are concurrency ownership; scratch is
admitted separately; and governed memory begins at real shared-pool growth.
Remove the ephemeral task reference. Do not reintroduce a class-memory lease or
add a replacement field, compatibility path, or abstraction.

## Constraints and preserved behavior

- Preserve per-tenant FIFO, equal-weight tenant rotation, Interactive-floor
  protection, oldest-eligible class selection, queue capacity, cancellation,
  and retryable `QueryAdmissionRejected` projection.
- Preserve the single `OracleAdmission` scheduler, one shared
  `BifrostResourceGovernor`, one `OracleMemoryRoot`, private query ceiling
  views, and exact slot/scratch release behavior.
- Preserve all seven prior-finding corrections, especially zero governed bytes
  for an idle admitted leader/follower and the governed/headroom split.
- Preserve authentication, authorization, tenant isolation, read-audit WAL,
  physical-root query-class derivation, deadlines after admission, result
  limits, terminal safety, and distributed cleanup.
- Do not modify the separately authorized F1 OLAP-deletion outcome.

## Non-goals

- No specification, public wire/SDK/Card, migration, dependency, configuration,
  metric, deployment, or distributed-protocol change.
- No generalized deadline queue, timer wheel, scheduler refactor, or new test
  fixture framework.
- No unrelated cleanup of pre-existing comments or warnings outside the
  corrected Oracle admission/memory contract.

## Acceptance criteria

- AC-R2-001 / FIND-ORACLE-LOCAL-T01-8: in one tenant FIFO containing an older
  live head followed by a later waiter whose absolute deadline has already
  passed, a grant pass may admit the live head but never allocates or notifies
  the expired waiter; queued, slot, scratch, active-query, and tenant counters
  reconcile, and subsequent live work progresses.
- AC-R2-002 / FIND-ORACLE-LOCAL-T01-8: the correction preserves per-tenant FIFO,
  equal tenant rotation, Interactive-floor protection, follower wakeup, queue
  capacity, cancellation, and retryable deadline rejection.
- AC-R2-003 / FIND-ORACLE-LOCAL-T01-9: Oracle resource/admission documentation
  contains no claim that slot admission reserves the 32/64 MiB class quantum,
  identifies actual shared-pool growth as the governed memory charge, and
  contains no task/plan reference.
- AC-R2-004 / FIND-ORACLE-LOCAL-T01-9: runtime resource behavior is unchanged:
  an idle leader/follower still owns zero governed query memory, while actual
  growth and shrink reconcile the shared root and headroom counters.

## Focused proof and broader verification

Add one deterministic in-module admission test for AC-R2-001 using the existing
queue owner and absolute deadline state. It must fail against candidate
`d3d379cd0` by observing the expired later entry receive a grant or resource
ownership, then pass after the correction. Do not rely on wall-clock races in a
server journey for this internal edge case.

Run the new exact test and the existing exact regression tests:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=<new exact expired-non-head test>)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::admission::tests::local_admission_is_fair_and_work_conserving)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::admission::tests::follower_release_wakes_waiting_leader_without_reordering)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::admission::tests::production_admission_absolute_deadline_is_bounded)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=resources::tests::oracle_leader_and_follower_govern_no_memory_until_their_pools_grow)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=resources::tests::oracle_queries_share_one_governed_memory_root)'
```

Then run:

```bash
mise run check:bifrost-resource-governance
mise run test:bifrost:integration:redux
mise run test:bifrost:journey:oracle
mise run verify:bifrost
mise run fmt
mise run lints
git diff --check d3888ddae83c833c3eb85edc0ce226eb6debadce..<new-candidate>
```

Record the RED/GREEN result of the new focused test and the exact verification
results before requesting cumulative re-review.

## Remediation evidence

### Acceptance matrix

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-R2-001 — an expired non-head waiter is never allocated or notified; counters reconcile; live work progresses | `oracle/admission.rs::grant_waiters` re-prunes both classes at the top of every loop iteration instead of once per pass, so an entry uncovered by a prior pop is removed before class selection, `acquire_waiter_resources`, or notification | New `oracle::admission::tests::expired_waiter_behind_a_live_head_is_never_granted` asserts one grant, a closed expired channel, `queued == 0`, `active_queries == 1`, `interactive.used == 1`, tenant `active == 1`, an empty FIFO, exactly one outstanding scratch lease returning to baseline on release, and a later live waiter still granted | PASS |
| AC-R2-002 — FIFO, rotation, floor, follower wakeup, capacity, cancellation, and retryable deadline rejection preserved | No selection, rotation, cursor, class-comparison, refusal, or rollback logic changed; only the position of the existing prune call moved | `test:bifrost:integration:redux` 973/973 (972 prior + the new case); the four named admission tests below pass unchanged | PASS |
| AC-R2-003 — no claim that slot admission reserves the class quantum; shared-pool growth named as the governed charge; no task reference | `resources.rs` doc corrections on `ORACLE_PARTITION_MEMORY_BYTES`, `ORACLE_PARTITION_WORKING_MEMORY_BYTES`, `oracle_worker_slots`, `OracleResourceRequest::memory_bytes`, `OracleResourceRequest::for_class`, and the `saturate_oracle_workers` fixture; `admission.rs::acquire_waiter_resources` rustdoc drops "Task 01 accounting" | `mise run lints` clean; `git grep -i "Task 01"` returns nothing under `crates/vala/vala-bifrost-redux/src` | PASS |
| AC-R2-004 — runtime resource behavior unchanged | Documentation-only edits in `resources.rs`; no expression, signature, or field changed | `resources::tests::oracle_leader_and_follower_govern_no_memory_until_their_pools_grow` and `resources::tests::oracle_queries_share_one_governed_memory_root` pass; `check:bifrost-resource-governance` passes | PASS |

### Focused proof

`oracle::admission::tests::expired_waiter_behind_a_live_head_is_never_granted`
was proven RED then GREEN. Against the candidate's prune-once shape it fails at
`an expired waiter must never receive a grant`; the shape was reverted in place
after the fix to confirm the same failure, then restored. It passes with the
correction.

The waiter's expired deadline is `Instant::now()` read before the grant pass
reads its own, which is deterministically in the past without clock arithmetic
(`clippy::unchecked_time_subtraction` is denied repository-wide).

### Commands

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::admission::tests::expired_waiter_behind_a_live_head_is_never_granted)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::admission::tests::local_admission_is_fair_and_work_conserving)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::admission::tests::follower_release_wakes_waiting_leader_without_reordering)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::admission::tests::production_admission_absolute_deadline_is_bounded)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=resources::tests::oracle_leader_and_follower_govern_no_memory_until_their_pools_grow)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=resources::tests::oracle_queries_share_one_governed_memory_root)'
mise run check:bifrost-resource-governance
mise run verify:bifrost
mise run fmt
mise run lints
git diff --check d3888ddae83c833c3eb85edc0ce226eb6debadce
```

All six focused tests passed. `check:bifrost-resource-governance` passed.
`verify:bifrost` reported `9/9 lanes passed` in 1837.76s: unit rust/python/
typescript; integration redux 973/973, sql 107/107, server 67/67; journeys sdk
15, forge 13, scribe 20, oracle 28, server 4, mcp 7; journey python and
typescript. `fmt` and `lints` are clean and `git diff --check` reports nothing.

`test:bifrost:integration:redux` and `test:bifrost:journey:oracle` were not run
as separate invocations because `verify:bifrost` is their parent lane and ran
both; their per-lane counts above are read from that run.

### Scope

The write set is two files: `crates/vala/vala-bifrost-redux/src/oracle/
admission.rs` and `crates/vala/vala-bifrost-redux/src/resources.rs`. No
specification, wire, SDK, Card, migration, dependency, configuration, metric,
deployment, or distributed-protocol surface changed. No generalized deadline
queue, timer owner, background task, polling loop, scheduler abstraction, or
test fixture framework was added; the existing `push_waiter` and `drain_grants`
helpers carry the new test. No replacement memory field or compatibility path
was introduced. The F1 OLAP-deletion outcome is untouched.

One documentation correction sits marginally outside the two cited FIND-9
locations: the `saturate_oracle_workers` test-fixture rustdoc in `resources.rs`,
which this task's own commit `c6e55b1f5` introduced and which stated that
"admission charges one slot-unit quantum rather than a whole grant cap". Worker
admission charges slot units and no memory, so the wording was corrected in
place rather than left as a second copy of the retracted claim.

### Material limits

`prune_expired` now runs once per grant decision rather than once per pass. Each
call is proportional to the number of tenants in the class, so a pass issuing
`g` grants across `t` tenants costs `O(g * t)` deadline comparisons instead of
`O(t)`. Both `t` and `g` are bounded by the pod's configured slot units and
tenant ring, and the work happens under a lock already held for the pass, so
this is not a new contention surface at any supported capacity. Restoring the
single-pass cost would require making expiry eligibility part of `next_eligible`
itself, which the correction deliberately did not do because it would spread
deadline policy across two owners.

The pass compares against one `Instant` captured at its start. An entry that
expires part-way through a single pass is granted rather than rejected. That
window is the duration of one lock-held scheduling pass, and the granted caller
still observes its own deadline downstream.
