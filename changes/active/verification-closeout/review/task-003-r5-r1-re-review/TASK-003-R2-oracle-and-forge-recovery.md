---
id: TASK-003-R2
kind: remediation
status: proposed
remediates: [FIND-TASK-003-2, FIND-TASK-003-7, FIND-TASK-003-8]
---

# TASK-003 R2: Complete Oracle drain and Forge restart ownership

## Subject and authority

- Approved spec: `changes/active/verification-closeout/spec.md` revision 3.
- Original task: `changes/active/verification-closeout/tasks/TASK-003-r4-canonical-support-desk-closeout.md`.
- Prior review and R1 task: `changes/active/verification-closeout/review/task-003-r4-canonical-closeout/`.
- This review's source evidence: `changes/active/verification-closeout/review/task-003-r5-r1-re-review/findings-validation.md` and `verdict.md`.
- Base `7f79fb3417db651adedac194ada8908f0a0372d7`; reviewed cumulative candidate `9a8f9f7eef95f70d356c037a192b7d7b90a37f31`. Reassess the full cumulative candidate after remediation.

## Diagnoses and correction outcomes

### FIND-TASK-003-2 — Oracle infallible headroom is invisible to shutdown

`crates/vala/vala-bifrost-redux/src/oracle/admission.rs:739-790` waits on and reports `oracle_query_memory_used_bytes`. In `src/resources.rs:2678-2711`, the existing governor charges an Oracle pool's infallible `grow()` beyond available governed capacity to global `infallible_headroom_bytes`, but only governed bytes enter the Oracle counter. A child can retain that pool after its admission slot returns. With only headroom retained, shutdown sees no active query and zero Oracle memory, so it can finish while Oracle bytes remain live. The new test uses fallible governed growth and does not exercise this state. Forge can also contribute to global headroom, so adding the global headroom total at shutdown would restore the sibling-role delay that R1 fixed.

Use the existing resource governor, which owns each reservation's `MemoryHolder`, to attribute Oracle infallible headroom to Oracle. Oracle shutdown must use the resulting Oracle-owned total for both completion and residual reporting. Keep the global capacity rules and both wakeups armed before each check; continue to exclude Forge and Scribe memory. Do not add a second Oracle memory governor or a consumer-side sum of global headroom.

### FIND-TASK-003-7 — Startup recovery runs before Forge's safety guard

`crates/vala/vala-bifrost-redux/src/forge/worker.rs:2166-2266` calls `start_and_drain()` before it marks the shared `ForgeLoopQuiescence` as active or installs the child stop-token drop guard. Startup recovery can reach `execute_claim()` and prepared reconciliation, which spawn plan and heartbeat tasks. If the invocation panics or is aborted in that window, those tasks can remain detached while the shared flag still says the prior run joined. The same-owner supervisor restart then passes `Some(owner)` to `ForgeTasks::reclaim_expired_attempts` (`crates/vala/vala-sql/src/queries/forge_tasks.rs:883-903`), which may reclaim an unexpired attempt before old work stops. The R1 panic test calls `enter()` before inducing the panic, so it does not prove the startup window safe.

Extend the existing quiescence and child-cancellation ownership across the entire worker invocation, including startup recovery. Preserve the predecessor's joined state long enough to choose the initial reclaim policy, then mark this invocation active before it may spawn work. Mark it joined only after all work it spawned has stopped. An interrupted startup must use ordinary lease-expiry recovery; an orderly joined predecessor may still fast-reclaim. Preserve exact-attempt SQL transitions, periodic expired reclaim, table fences, and API availability. Do not add a persistent fence or change the shared supervisor policy.

### FIND-TASK-003-8 — Forge quiescence field lacks required rustdoc

`crates/vala/vala-bifrost-redux/src/forge/worker.rs:715` adds a private `Arc<AtomicBool>` tuple field without field-level rustdoc. `AGENTS.md` §16 requires documentation on every new Rust field, including private fields, and treats omission as a merge blocker. Document the shared field's meaning at the field declaration while correcting FIND-7; no behavior change is needed for this item.

## Acceptance and focused proof

| Finding | Observable acceptance | Narrow proof |
|---|---|---|
| FIND-TASK-003-2 | Oracle shutdown waits and reports a child holding only Oracle infallible headroom after its slot returns, then finishes after release while Forge memory remains live | Add a focused Oracle admission/resource test for that sequence and run its exact `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=...)'` selector. |
| FIND-TASK-003-7 | Interrupted startup recovery cannot authorize unexpired same-owner reclaim while its plan or heartbeat remains live; clean joined restart still fast-reclaims; expired attempts remain recoverable | Add focused worker recovery proof of interruption during startup with live child work and run the exact named selector; run the existing exact Postgres `pg_forge_tasks::pg_tests::previous_owner_reclaims_its_unexpired_attempt` selector through the repository-managed Postgres wrapper. |
| FIND-TASK-003-8 | New Forge field has meaningful item rustdoc | Source inspection and `mise run fmt`; no behavior test for the documentation line. |

Run the narrowest owning `mise` checks for the Rust write set, including `mise run fmt` and `mise run lints`. Use exact selectors for every named Rust test in the implementation evidence, with the repository-managed environment wrapper where required. A broad Scribe/Forge/Oracle suite or `mise run gate` is not this remediation's iteration requirement; aggregate verification belongs at change review.

## Preserved behavior and non-goals

Keep one shared non-blocking Scribe outbox, gateway Card UID authorization, tenant isolation, the three public SDK workflows, the accepted in-memory loss window, and both Oracle shutdown wakeups. No CardRef alias, second governor, new delivery sink, persistent Forge fence, supervisor-policy change, or unrelated refactor is part of this task. Prior FIND-1, -3, -4, -5, and -6 remain closed unless source evidence in the next cumulative review shows otherwise.

## Implementation Evidence (R2)

Commits on `9a8f9f7ee`: `51dbfc1fd` (FIND-2) and `3af5bdb5b` (FIND-7, FIND-8).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-2: Oracle shutdown waits for and reports a child holding only Oracle infallible headroom, and finishes after its release while Forge memory stays live | `resources.rs`: `ResourceState`/`ResourceSnapshot.oracle_infallible_headroom_bytes` charged in `reserve_pool_memory_infallible` and returned in `release_pool_memory` (underflow poisons), Oracle holder only; `oracle/admission.rs::oracle_memory_reserved` sums Oracle governed and Oracle headroom bytes. Global capacity rules and both wakeups unchanged | `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::admission::tests::oracle_shutdown_drains_oracle_infallible_headroom) \| test(=oracle::admission::tests::oracle_shutdown_drains_only_oracle_memory)'`: 2 passed | PASS |
| FIND-7: interrupted startup cannot authorize unexpired same-owner reclaim; expired attempts stay recoverable | `forge/worker.rs::run`: `ForgeLoopQuiescence::enter(owner)` atomically reads the predecessor state and marks this invocation live before startup recovery; the child stop token and its drop guard now cover startup; `joined` runs only after a startup that returned `Ok` or a joined event loop. The startup `reclaim_owner` is passed to `reclaim_expired_attempts` and, a sibling caller of the same policy, to `ForgeTasks::claim_prepared_for_reconciliation`, whose same-owner predicate now binds `previous_owner` (`$3`) because a Prepared takeover keeps the attempt UUID and would not fence a stale holder. The event loop and `execute_one_for_test` pass `Some(owner)` as before | `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux -P journey --run-ignored=all --test integration -E 'test(=forge::production_routes::interrupted_startup_waits_for_its_prepared_lease) \| test(=forge::production_routes::worker_prepared_recovery_observes_one_ownership_episode)'`: 2 passed. The new test fails when startup passes `Some(owner)` (red-checked, then restored) | PASS |
| FIND-7: a joined restart still fast-reclaims; exact-attempt SQL transitions preserved | `ForgeLoopQuiescence::enter` returns `Some(owner)` after `joined` | `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=forge::worker::tests::only_a_joined_loop_enables_same_owner_reclaim)'`: passed. `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-sql --test-threads=1 -E 'test(=pg_forge_tasks::pg_tests::previous_owner_reclaims_its_unexpired_attempt) \| test(=pg_forge_tasks::pg_tests::prepared_reconciliation_takeover_is_at_most_once) \| test(=pg_forge_tasks::pg_tests::prepared_reconciliation_takeover_recovers_large_task)'`: 3 passed | PASS |
| FIND-8: the Forge quiescence field has rustdoc | `forge/worker.rs` `ForgeLoopQuiescence` tuple field doc | Source inspection; `mise run fmt` | PASS |
| Format, lints, boundaries | Whole write set | `mise run fmt`, `mise run lints`, `mise run check:tenant-isolation`, `git diff --check`: all pass | PASS |

Non-goals stayed excluded. There is no second governor, no consumer-side sum of global headroom, no persistent fence, and no supervisor-policy change. The shared Scribe outbox, gateway Card UID authorization, and SDK workflows are untouched. No unrelated file changed.

Limits:
- The interrupted-startup test aborts at the Prepared claim gate, which comes before the reconciliation heartbeat spawns. Work spawned later in startup is stopped by the child stop token's drop guard, the same mechanism the event loop already relies on.
- The test's negative proof is a 2-second observation window covering eight 250 ms recovery passes. It is not a synchronization sleep.
- `record_oracle_memory` still exports the global headroom in the `memory_used` gauge. That telemetry is outside this finding and was left unchanged.
