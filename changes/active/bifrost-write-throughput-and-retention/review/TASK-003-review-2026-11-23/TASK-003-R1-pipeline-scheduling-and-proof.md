---
id: TASK-003-R1
kind: remediation
status: proposed
spec: SPEC-bifrost-write-throughput-and-retention
spec_revision: 4
requirements: [REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019]
depends_on: []
parent_task: TASK-003
remediates: [FIND-TASK-003-1, FIND-TASK-003-2, FIND-TASK-003-3, FIND-TASK-003-4, FIND-TASK-003-5]
---

# Preserve completion priority and prove concurrent write isolation

Finish the existing pipeline's scheduling contract and its missing connected
proof. A writer whose durable result is ready must receive visibility and an
ACK before unrelated next-group preparation. Healthy writers from several
tenants and tables must retain the same interleaving and isolation guarantees
already demonstrated for one table.

Review subject: base `74034459d468fd0adf277e3b51c32b1a676e8e55`, candidate
`c0900bff523e83d3666658440daa81355067cb34`. Read the original
[TASK-003](../../tasks/TASK-003-bounded-write-pipeline.md), approved
[spec revision 4](../../spec.md), [validated findings](findings-validation.md)
and [verdict](verdict.md). This task does not replace or broaden the original
contract. FIND-TASK-003-4 records the maintainer's explicit coverage instruction
during review.

## Correction boundary

Reuse the shard run loop, scheduler/collection owner, retained group futures,
WAL lane and existing `PipelineFixture` in `scribe/shards.rs`. Inputs remain
admitted appends and existing control commands; outputs remain ordered,
tenant-bound durable acceptance, query-visible rows, ACKs and settled leases.
The only documentation correction is the collection paragraph in
`architecture/bifrost-design.md`.

- FIND-TASK-003-1: `run` drains the mailbox and dispatches/prepares before
  polling a ready older completion. Its mailbox drain lacks a service bound.
  Restore completion priority and bounded mailbox service at that owner.
- FIND-TASK-003-2: `dispatch_next` feeds all queued requests into the collection
  target before excluding held identities. Base waiting on eligible collection
  without losing the first-request deadline or table FIFO.
- FIND-TASK-003-3: architecture describes 32 as a cap. Align it with the approved
  soft target: stop waiting for new arrivals, drain ready eligible input to the
  existing byte and 64-request limits.
- FIND-TASK-003-4: scheduler units and a two-tenant failure test do not prove
  healthy multi-tenant/table interleaving through the real write pipeline.
  Add connected proof; change production only if it exposes a defect.
- FIND-TASK-003-5: the live ambiguous-COMMIT path lacks executable proof of failed
  fresh reconciliation and retained two-slot ownership. Supply an exact existing
  test plus valid evidence if available, otherwise add focused connected proof.
  This evidence gap alone does not justify production changes.

Keep the two-slot bound, one ordered WAL producer, oldest-only fencing,
independent ACKs, tenant SQL batching, canonical recovery, existing barriers,
resource accounting and public contracts. No new runtime, public setting,
pipeline framework, retry controller or benchmark substitute is authorized.

## Ordered closure scenarios

1. **Ready completion wins — FIND-TASK-003-1; REQ-015/019.** In the production
   coordinator, arrange an older completion to be ready with eligible input for
   another group queued. Observe visibility and its ACK before new preparation
   begins. Exercise a finite mailbox backlog exceeding a service chunk and
   assert completion remains serviceable while backlog remains. Use explicit
   reached/release handshakes; no elapsed-time speed assertion. The current
   prepare-before-poll ordering supplies the expected RED. Preserve cancellation,
   barrier and shutdown ownership while fixing the existing loop.

   **Evidence (PASS; TDD PASS).** Test
   `scribe::shards::tests::a_ready_head_completion_is_acknowledged_before_the_next_group_prepares`
   (`shards.rs`, unit tier, real WAL, no fence database so the head's fence is
   ready on first poll). A test-only `preparation_hold` on `ShardOwner` is the
   reached/release handshake at the start of preparation in `dispatch_next`.
   With a 129-append mailbox backlog, it asserts at that point that the head is
   already ACKed and visible and that exactly one `MAILBOX_SERVICE_CHUNK` (64)
   has left the mailbox. RED before the fix:
   `the ready head is acknowledged before the next group prepares: Empty`.
   Fix: `GroupPipeline::ready_stage` (biased select with a ready fallback,
   polled with the real waker) runs at the top of `ShardOwner::run`, before
   mailbox routing and dispatch. `drain_mailbox` now routes at most
   `MAILBOX_SERVICE_CHUNK` commands per loop turn. Cancellation, barrier,
   poison and shutdown branches are unchanged. The original independent-ACK
   test `pg_tests::later_groups_sync_while_earlier_groups_fence_and_ack_independently`
   is GREEN. Commit `80c63af48`.

2. **Only eligible requests end collection waiting — FIND-TASK-003-2;
   REQ-015/016/019.** Hold an older identity A, queue at least 31 retries on its
   table and one eligible B on another table. Before the fixed deadline, the
   excluded A requests must not satisfy the overlapping group's target. Add
   eligible arrivals, prove the first deadline does not reset, and prove target,
   predecessor completion and ready-drain behavior. Preserve held-table FIFO and
   other-table progress. Use controlled time for pure collection logic and a
   coordinator assertion for the connected dispatch decision. The current raw
   queued-count decision supplies RED. Correct architecture's cap wording in
   the same bounded increment; content review closes FIND-TASK-003-3.

   **Evidence (PASS; TDD PASS).** Tests
   `scribe::shards::tests::held_retries_do_not_satisfy_the_collection_target`
   and `scribe::shards::tests::a_predecessor_completion_dispatches_a_short_eligible_group`
   drive coordinator `dispatch_next` with paused tokio time and a head that
   holds identity A, plus 31 queued A retries and eligible B requests.
   Assertions: no dispatch with 1 eligible; the deadline is unchanged after
   arrivals at +½ wait; 40 eligible dispatch as one 40-request group (draining
   ready input above the target) with no A retry; 31 retries stay queued; the
   predecessor's completion dispatches `[A retry, B]` at once before the
   deadline. RED before the fix:
   `held retries do not complete the eligible group` (both tests). Fix:
   `TenantTableRoundRobin::eligible(held, COLLECTION_TARGET)` counts each
   table's requests ahead of its first held or repeated identity, and that
   count feeds `GroupCollection::due`. The pure timer test
   `group_collection_waits_once_from_its_first_request` stays GREEN.
   FIND-TASK-003-3: `architecture/bifrost-design.md` (Scribe write path
   paragraph) now describes 32 as a soft target that ends waiting while ready
   eligible input drains to the byte/64-request limits (content review). Commit
   `80c63af48`.

3. **Interleaving across tenants and tables — FIND-TASK-003-4;
   REQ-015/016/018/019.** Drive healthy concurrent appends for several tenants
   and multiple tables through the real production shard loop with actual WAL
   and PostgreSQL. At least one shard must contain competing tenants/tables;
   separate independent shards alone cannot establish shared-pipeline behavior.
   Stream enough groups to reuse both slots repeatedly. Hold an older fence,
   observe a later group's WAL sync, hold that later completion and release
   the older fence. Prove independent ACKs and repeat the interleaving with
   competing tenant/table inputs. Assert table FIFO and tenant/table progress,
   oldest-only fencing/visibility, exact rows under their owning tenant/table,
   no duplicated identity across slots, and eventual release of all per-request
   global/tenant/table ownership. Reuse stage holds and fixture setup; keep
   setup/actions/assertions readable. A scheduler-only model, single-table
   stream or failure-only two-tenant case cannot close this finding.

   **Evidence (PASS; evidence-only, no production change).** Test
   `scribe::shards::tests::pg_tests::tenants_and_tables_interleave_through_both_slots_with_independent_acks`
   (real WAL + PostgreSQL, one production shard loop). It streams 4 waves of 8
   appends across 2 tenants × 2 tables on one shard. Each wave is queued at
   once, so it is one mixed group, with `sync_submissions == 4`. Each wave
   blocks in its real fence while the next wave syncs (`hold_next_sync_for_test`),
   then is ACKed while the next wave's IO is held. Assertions per wave:
   - earlier waves are pending;
   - no later wave is fenced (oldest-only fencing);
   - all wave ACKs have `first_commit`;
   - each tenant/table `row_count == (wave+1)·2`;
   - settled waves are fully fenced.

   The last wave carries a retry of the previous wave's held batch, queued
   behind its table's fresh appends. It stays pending, then converges with
   `!first_commit`. Every wave's synced WAL growth is equal, so the retry
   never joins the other slot's group. Afterwards transient bytes are 0 and the
   accepted-request count is 0. Assertion strength: a mutation passing an empty
   held set to `pop_group` fails with
   `[8736, 8736, 9825]`. Limit: admission at this owner is pod-global, so
   per-request ownership is proven by the global ledger and drain count; no
   separate tenant/table admission counter exists here. Commit `3526cc663`.

4. **Unavailable live reconciliation retains ownership — FIND-TASK-003-5;
   REQ-017/019.** With younger WAL work submitted, drive an older ambiguous fence
   COMMIT followed by unavailable fresh reconciliation through the production
   coordinator and real SQL/WAL boundaries. Assert fail-closed readiness,
   retained uncertain owner and charges, both groups' waiters remaining
   unanswered, no younger fence, and
   submitted IO drained rather than abandoned. Reuse the existing poison and
   restart fixtures; the existing accepted-versus-provisional recovery contract
   remains fixed. One focused unavailable-reconciliation scenario suffices;
   do not add a matrix of every outage mechanism. An exact existing executable
   test and candidate-bound result may close this gap without a new test.

   **Evidence (PASS; evidence-only, no production change).** No existing test
   took the failed-reconciliation branch, so I added
   `scribe::shards::tests::pg_tests::an_unavailable_commit_reconciliation_retains_both_groups_for_restart`
   (real WAL + PostgreSQL, production run loop, existing restart/replay
   fixture). A deferred trigger holds the older COMMIT on a test-owned
   advisory lock, and the younger group syncs (IO completion held). The test
   then revokes SELECT on `vala.scribe_batch_commits` and releases the lock,
   so the COMMIT fails and the fresh `resolve` is denied. Assertions:
   - readiness is poisoned;
   - both waiters are pending, then `Closed` (never answered);
   - transient charge stays > 0;
   - neither batch is fenced;
   - Shutdown stays in the mailbox and the task stays running while the
     younger IO is held, then the owner exits after release;
   - restart replay restores 0 rows.

   Assertion strength: removing `ownership.poison()` from the `resolve`
   error branch fails with `the unresolved COMMIT fails the shard closed`.
   Diagnosis of a first fixture attempt: a restrictive RLS policy keyed on a
   sequence. **Symptom:** no poison within the bound. **Evidence:** the sequence
   was called once and the older ACK was `Err(trigger exception)`. **Cause:**
   the empty reconciliation lookup never evaluated the row policy, so the
   lookup resolved Absent and the retried INSERT ... RETURNING failed as a known
   pre-COMMIT error. **Fix site:** test fixture only, switched to a
   statement-level privilege revocation. Commits `ab0e26d62`, `0d8c3fed6`.

Inline owner tests and `mod pg_tests` are the narrowest tiers for these internal
state transitions; keep real dependencies at the connected boundaries above.
For already-correct behavior, record credible evidence instead of manufacturing
RED. Preserve the original disclosed TDD violations; later tests cannot change
that history.

### Verification record

- Focused (11 selected, all PASS): the five tests above plus
  `group_collection_waits_once_from_its_first_request` and the pg_tests
  `later_groups_sync_while_earlier_groups_fence_and_ack_independently`,
  `a_retried_batch_waits_for_its_in_flight_attempt_while_other_tables_proceed`,
  `a_later_tenant_fence_failure_after_an_earlier_commit_fails_closed`,
  `an_unresolved_older_commit_holds_the_younger_group_unfenced` and
  `shutdown_drains_both_dispatched_groups`. Each was run as
  `mise exec -- scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --no-fail-fast -E 'test(=scribe::shards::tests::<name>)'`, with the
  selectors joined by `|` in one run.
- `mise exec -- scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --no-fail-fast`: 1015/1015 PASS.
- `mise run lints`: PASS, after fixing two clippy findings in the new test.
  `cargo fmt -p vala-bifrost-redux --check` and `git diff --check`: clean.
- Not run, by task instruction: `test:bifrost:integration:redux` and journey
  suites (left for change review), and the REQ-020 benchmark (still pending).
- TDD: scenarios 1–2 were RED then GREEN. Scenarios 3–4 are evidence-only and
  use mutation checks instead of manufactured RED. The original task's
  disclosed TDD violations remain unchanged.

## Verification and handoff

Confirm new test names from source, then record each exact selector before its
test-first increment. PostgreSQL tests use the existing runner shape:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=<actual-qualified-test-name>)'
```

Pure unit tests use the same exact nextest selector without the PG wrapper.
Run focused affected overlap, identity, lifecycle and failure regressions,
including the original independent-ACK assertion when changing loop ordering.
The owning lane is `mise run test:bifrost:integration:redux`; use focused tests
for task review rather than automatically rerunning full journey suites. Run
required format/lint checks and record actual evidence on these scenarios.
Cargo-backed checks in this shared checkout must run sequentially.

Return the immutable remediation candidate, proof and remaining limits for
maintainer-initiated review. Do not implement unrelated collateral changes or
run a benchmark. REQ-020 stays pending: only after code review passes and all
required correctness checks finish does the maintainer independently run
`mise run benchmark:l1:bifrost-writes` as the last required execution step,
using only `design/l1-baseline-74034459d.md` for baseline comparison. The original
task remains open until that qualification is accepted.
