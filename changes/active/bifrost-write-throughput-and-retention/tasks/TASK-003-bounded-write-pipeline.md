---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-bifrost-write-throughput-and-retention
spec_revision: 4
requirements: [REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-020]
depends_on: []
---

# Keep WAL writes progressing during earlier acceptance

Implement the agreed two-group pipeline and prove its scheduling benefit before
benchmark qualification. This is the only current task; historical TASK-001/002
are archived and are not dependencies to reexecute.

## Owners, inputs and approach

Use [spec revision 4](../spec.md) and the Recommended decisions and Connected flow
in [the finalized proposal](../design/pipeline-implementation-proposal.md).
Admitted PreparedAppend values come from the real preprocessing/mailbox path.
The current `scribe/shards.rs::run` awaits the entire `process_group`, and
`append_and_sync_batch_commits` combines durable WAL and SQL work. Split that
boundary within the existing shard owner and reuse GroupWalState, the existing
`execution_lanes.rs` WAL pool, tenant fence queries and memory leases. Pending
operations must return owned state without monopolizing mutable coordinator
access. Output is canonical fenced, visible data and the existing durable ACK,
consumed by real SDK delivery and query paths. Update owning architecture prose
where serial scheduling is contradicted. No new acceptance architecture or
retention/CPU redesign; private method and module layout remain implementer-owned.

## Ordered acceptance scenarios and execution evidence

Each entry is a test-first increment; add exact assertion paths, focused nextest
commands, observed RED reason, GREEN result and limitations here. For existing
behavior use credible verification rather than manufacturing RED. New test names
are chosen by implementation, with exact focused commands recorded before RED.

1. **Streamed overlap and independent ACK — REQ-014, REQ-015, REQ-019.** Start
   the real shard coordinator with distinct admitted groups and a real WAL.
   Hold N's real PostgreSQL fence using the existing pg fixture blocking pattern;
   observe N+1's sync before releasing N. Hold N+1 IO completion, release N and
   assert N's query visibility and success ACK while that hold remains. Release
   it and repeat through N+2 and subsequent groups, observing repeated slot reuse
   and ordered fencing. Serial code must fail overlap; a join-barrier candidate
   must fail independent completion. Use reached/release signals, not sleeps or
   elapsed-time comparisons. Bounded timeouts only diagnose deadlock/inconclusive
   setup. Owner: shards inline pg_tests for real SQL, actual WAL IO boundary;
   integrate with the production run loop rather than testing a model alone.
   Evidence (commit 7814482d9):
   - Test `pg_tests::later_groups_sync_while_earlier_groups_fence_and_ack_independently` (GROUPS=4), command `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::pg_tests::later_groups_sync_while_earlier_groups_fence_and_ack_independently)'`.
     It holds each group's real fence, observes the next group's held sync, releases the fence, asserts visibility and success ACK while the next group's IO is still held, and asserts `fenced == group` each iteration.
   - RED (serial run loop): `the next group syncs while this group fences: Elapsed`. GREEN after the two-slot pipeline.
   - Diagnosis 1. Symptom: the GREEN candidate stalled. Cause: the fixture dropped its pressure `watch::Sender`, so the loop detached. Fix site: the fixture keeps its senders. Confirmed by a diagnostician.
   - Diagnosis 2. Symptom: `fenced == 0` failed. Cause: test bug, because earlier groups' fences are durable. Fix site: the assertion is now `fenced == group`. Confirmed by a diagnostician.
   - Join-barrier rejection: the same test asserts N's ACK while N+1's sync hold remains.
2. **Collection, identities and bounded work — REQ-014–016, REQ-019.** Feed
   controlled arrivals, exhausted queues, multiple tenant/tables, byte limits and
   repeated identical/conflicting identities through the same scheduler. Assert
   fixed collection deadline, immediate idle/predecessor-complete dispatch,
   group membership, fairness/FIFO, two-slot maximum and no repeated-ID overlap.
   Keep ready head completions serviceable during collection/preparation. Use
   controlled time only for pure timer tests; use real WAL/SQL integration for
   identity arbitration including retained-version and cross-pod retries. Count
   actual submissions/syncs/transactions for known groups to catch added work;
   do not claim counts measure CPU throughput. Missing cross-slot exclusion or
   bounded collection supplies the expected RED; retain existing fairness proof.
   Owners: shard scheduler unit tests and shard pg integration. Evidence (commits 7814482d9, 668670421):
   - Exclusion. Test `pg_tests::a_retried_batch_waits_for_its_in_flight_attempt_while_other_tables_proceed`, command `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::pg_tests::a_retried_batch_waits_for_its_in_flight_attempt_while_other_tables_proceed)'`. It asserts `write_submissions_for_test() == 4` and `sync_submissions_for_test() == 2`. RED: 6 writes vs 4, because the retry overlapped its attempt. GREEN after held-identity skipping in `ShardScheduler::pop_group`.
   - Collection. Unit test `group_collection_waits_once_from_its_first_request` covers the fixed 1 ms deadline from the first request (never reset), the 32-request target and immediate idle dispatch. RED: `nothing collected dispatches nothing`. GREEN.
   - Unit test `held_identities_skip_their_table_and_keep_its_turn` covers fairness and FIFO under held identities. It was written after the code (TDD violation).
   - Existing scheduler fairness tests are retained.
   - Diagnosis. Symptom: `wal_closeout::failure_after_fsync_before_ack_reuses_stable_batch_once` failed intermittently. Cause: a retry was dispatched beside its own in-flight attempt because no cross-slot exclusion existed. Fix site: the scheduler's held-identity skip, which serves every dispatch.
   - Limitation: the submission counts catch added WAL work for known groups only. They do not measure CPU throughput. Cross-pod and retained-version identity arbitration relies on the existing fence tests in the full lib suite.
3. **Failure, custody and lifecycle — REQ-016–019.** With two occupied slots,
   trigger known pre-acceptance failure, normal conflicts, multi-tenant partial
   operational failure, unresolved SQL acceptance, post-fence insertion failure
   and later-group IO failure. Assert the specified continuation/fail-closed
   transitions, retained charges and absence of later fencing after ambiguity.
   Restart against real WAL/Postgres; accepted rows survive exactly once and
   provisional attempts restore nothing. Exercise rotation/segment crossing,
   selective seals, pressure, shutdown, client cancellation and existing FlushAll
   semantics. Assert barrier progress, complete byte projection and exactly-once
   release/transfer; receipt cleanup/frontier cannot outrun custody. Existing
   serial fault fixtures are reuse inputs; second-slot ownership and premature
   mutation should expose missing pipeline behavior. Owners: shard pg tests,
   existing recovery/WAL integration modules. Evidence (commits 749ef21e3, bda21212b, ecc293422, f000ca624, c0900bff5). All tests are in `scribe::shards::tests::pg_tests`, each run with `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::pg_tests::<name>)'`:
   - `a_later_tenant_fence_failure_after_an_earlier_commit_fails_closed`: the multi-tenant partial failure fails closed. No waiter is answered, the younger group never fences, and after restart via `replay_into_owner` exactly the accepted row is restored. RED: `the shard fails closed: Elapsed`. GREEN.
   - `a_younger_group_io_failure_leaves_the_older_group_to_commit`: later-group IO failure.
   - `an_older_group_conflict_refuses_it_while_the_younger_group_commits`: a normal conflict, which is also a pre-acceptance refusal.
   - `a_post_fence_insertion_failure_is_retried_before_the_younger_group_fences`: post-fence insertion failure. The owner is retained unanswered, and the younger group does not fence until the retry acknowledges.
   - `a_group_needing_rotation_waits_for_the_dispatched_group`: the rotation barrier.
   - `flush_all_waits_for_dispatched_groups_and_keeps_its_queue_position`: FlushAll semantics and mailbox position.
   - `shutdown_drains_both_dispatched_groups`: shutdown with two groups in flight.
   - `a_cancelled_waiter_does_not_stop_its_group`: client cancellation.
   - `pressure_seals_wait_for_the_dispatched_group`: pressure seals. RED: `(0, 2)` vs `(1, 1)`. The `select!` wake consumed the watch signal; this bug predates the pipeline. Fix site: `ShardWake::Pressure` calls `mark_changed()`. Confirmed by a diagnostician.
   - Diagnosis. Symptom: `a_later_tenant...` timed out only in the full parallel run. Cause: the `pg_locks` probe counted waiters in every test database, so `fence_blocked` returned early. Fix site: the probe in `PipelineFixture::fence_blocked` and the inline probe in `group_commit_crash_between_sync_and_fence_restores_nothing`, now scoped by `pg_stat_activity.datname = current_database()`. Confirmed by a diagnostician. A first attempt joined `pg_database` and broke every probe, because `transactionid` waits carry no database oid.
   - Full lib suite: 1008/1008, three consecutive runs.
   - TDD VIOLATED: the rotation, deferral, FlushAll, shutdown, cancellation, conflict, post-fence retry and later-IO-failure behavior was written in 7814482d9, before these tests existed. Those tests passed on first run.
   - `an_unresolved_older_commit_holds_the_younger_group_unfenced` (f000ca624): unresolved SQL acceptance with two slots. A real deferred constraint trigger fails every COMMIT carrying the older batch, and a sequence counts the failed COMMITs across rollbacks. While the COMMIT stays unresolved, the younger group syncs but has 0 fences, both waiters stay unanswered and no row is visible. After the trigger is dropped, both groups commit once each (2 fences, 2 visible rows), and the older ACK reports `first_commit`.
   - Unavailable or contradictory resolution poisons readiness and reaches the same `start_oldest_fence` check that `a_later_tenant...` drives with two slots. A real (non-injected) insertion failure does the same. Their serial poisoning is covered by the existing ambiguity tests and `post_commit_insertion_failure_*`.
   - `age_seals_wait_for_the_dispatched_group` (f000ca624): a selective age seal (`FlushExpired`) waits for the fencing group, and no later group dispatches behind it. Result `(immutable, writable) == (1, 1)`; a seal applied immediately would give `(0, 2)`. Pressure seals of named keys are covered by `pressure_seals_wait_for_the_dispatched_group`.
   - Receipt cleanup and frontier against custody:
     - In the pipeline, a generation closes only when no group is dispatched. `rotate_before_append_if_needed` runs only when the pipeline is idle (`a_group_needing_rotation_waits_for_the_dispatched_group`), so no synced-but-unfenced slice is in a closed generation the cleaner can retire.
     - Cleanup itself is unchanged and is covered by the `wyrd-testing` scribe lifecycle journeys in `mise run test:bifrost:journey:scribe` (53/53). These include `restart_restores_accepted_rows_after_handed_off_receipts_are_deleted`, `receipt_cleanup_failure_pauses_fresh_writes_until_it_recovers` and `staged_rows_restore_and_retire_once_after_their_receipt_is_deleted`.
   - Byte projection and release (c0900bff5): every two-slot test asserts `transient_bytes(&budget) == 0` (raw, decode, prepared and queued categories) once its groups settle or the owner drains. A charge released twice would panic the governor, and a charge never released would leave the sum non-zero. The fail-closed test asserts the retained groups still hold their charge.
   - Full lib suite after c0900bff5: 1010/1010 on three consecutive runs. Clippy `-D warnings` and `git diff --check` are clean.
4. **Connected delivery and final correctness — REQ-014–019 and repository
   journey authority.** Through real SDK → WyrdTestServer + repository Postgres
   → SDK, drive concurrent native/OTLP writes, durable flush, query visibility,
   retries and negative paths. Ensure consumers use the new coordinator path,
   not a fixture-only path. Preserve shipped Rust, Python, TypeScript and MCP/HTTP
   journeys as applicable; no public API changes. Add focused journey assertions
   only for missing behavior; run existing regression coverage for unchanged
   contracts. Complete scoped integration, journey, format/lint and applicable
   boundary checks before handing off performance pending. Evidence:
   - `mise run test:bifrost:journey:scribe`: 53 passed, 1 skipped.
   - `mise run test:bifrost:journey:otlp`: 21 passed.
   - `mise run test:bifrost:integration:redux`: 1080/1080 after feb9c594b.
     - Diagnosis. Symptom: `forge::production_routes::worker_selects_current_iceberg_files` failed deterministically. Cause: it compared Forge bytes to the governed total, which also includes Scribe's retained encoder contexts (since 3aa9b67b1). Fix site: the assertion now compares against the occupant's reservation, and the post-run check allows only Scribe-held bytes. Confirmed by a diagnostician.
   - `mise run fmt` and `mise run lints`: clean.
   - `mise run py:test:integration`: 367 passed, 3 failed. `mise run ts:test:integration`: 43 files passed, 1 failed. The failures were the same three `verification-capacity` journeys, outside the write path.
     - Diagnosis (Codex, read-only). Symptom: no hosted waiter appeared, and the 504 block passed. Cause: since 02efe81fc, admission pauses Verification only when it is over its contended share, and `Sandbox::occupy_for_test` charged its hold to no workload.
     - Fix site (789103424): `occupy_for_test` attributes the hold to Verification and samples pressure once it is taken. `Sandbox::execute` now charges retained inputs before the admission wait, so the leave-0 case is still refused at once with 503. The six controls of REQ-195 are unchanged.
     - The queued submit defers before any Python waiter exists, so each SDK copy is now `submit_under_hosted_memory_is_accepted_then_passes`. Deferral itself stays proven in `wyrd-testing` `isolated_capacity`.
     - After the fix: Python capacity journeys 3/3, `ts:test:integration` 44/44 files, Rust SDK `verification_capacity::` 3/3 (--run-ignored=all), MCP `execution_waits_and_refusals_follow_hosted_capacity`, `isolated_capacity` 5/5, `public_controls_are_the_six_sanctioned_operations`, and the `verification::hosted::` unit tests 36/36.
     - `fmt`, `py:format`, `ts:format`, `lints`, `py:lints` and `ts:lints`: clean.
   - `check:deps` and `check:tenant-isolation` are not applicable: no dependency or tenant-access boundary changed.
   - No public API changed. No journey assertion was added, because none of the existing write journeys showed missing behavior.
5. **LAST: independent Bifrost writes closeout — REQ-020.** After entries 1–4
   and all required non-write-benchmark qualification, hand the maintainer the
   candidate identity, correctness evidence and designated baseline path. Do
   not run the benchmark automatically. The maintainer runs the full command
   below independently and supplies its records. Record validity, matching-case
   comparison, variation and outcome here; leave task open until this establishes
   the specified performance outcome. This is a measurement step, not a TDD
   cycle. Evidence: pending maintainer execution.

## Follow-up: per-request cost grows with concurrency

Open; not started. In the routine sweep `target/benchmarks/l1/bifrost-writes/20261124t010747915z` (commit 16932b9a3), every c64 write case gained throughput over the baseline but cost more CPU per row. For example, `otlp-logs-c64` went from 29.9 to 34.0 ms per 1k rows, and from 39.9 to 35.1 MiB per CPU-second. Within this sweep, `otlp-logs` costs 19.0 µs/row at c1 and 29.1 µs/row at c64, with the same request shape at both: about 512 rows and 1,252 wire bytes per row, and the same staged and published bytes per row. The same request also runs slower at c64: decode p50 goes from 0.23 to 0.69 ms, and preprocess p50 from 0.99 to 1.71 ms. The server uses about 9 of 32 cores, so it is not short of CPU.

- Read-only Codex diagnosis, from source only and unproven. Its ranked suspects:
  1. Memory and cache pressure in OTLP decode and the Arrow encode in preprocess.
  2. Rayon queue wait inside the Scribe decode/preprocess timers. This mainly affects p99: it rises from about 20 µs to about 1.2 ms.
  3. Shared locks: `resources.rs` `lock_state`, the conformance-plan `RwLock` and `committed_batches`.
- These phase timers measure wall time, so they do not show CPU spent in those phases. The extra copy at `preprocess.rs:976` (about 88 KB per request) is too small to explain the slowdown.
- Next step, on a quiet host: run `perf stat -e cycles,instructions,cache-misses,page-faults` on the server during `--cases otlp-logs-c1,otlp-logs-c64`.
  - If instructions per request rise, find the added work or the lock spinning.
  - If instructions per request stay flat but cycles per instruction rise, fix it structurally: reuse decode buffers per worker, move OTLP decode off the async workers, or limit in-flight requests per core.
- The first `--profile cpu` attempt (`20261124t014957829z`) is unusable. Other agents' cargo builds used 5.1 of the 32 CPUs, which made the c64 case INVALID and contaminated the c1 case.

## Existing runners and verification discipline

Use `mise run test:bifrost:integration:redux`,
`mise run test:bifrost:journey:scribe`, `mise run test:bifrost:journey:otlp`
and affected existing SDK/MCP journey lanes from mise.toml/TESTING.md. Inline
Postgres tests use the repository Postgres wrapper and the owning crate's exact
nextest selector; never run them in credential-free lanes. Run `mise run fmt`
and `mise run lints`, plus applicable dependency/tenancy checks for changed
boundaries. Run required journey and other qualification before final benchmark.
Do not invent new runners, synthetic host load or benchmark substitutes.

The required final command, executed independently by the maintainer:

```bash
mise run benchmark:l1:bifrost-writes
```

Only `design/l1-baseline-74034459d.md` and its underlying records supply baseline
measurements. No benchmark is needed between scenarios. Deterministic tests
establish eliminated serialization and sustained interleaving; report that
mechanism result separately from measured throughput. Busy-host test timeout
is not evidence of throughput regression. Follow benchmark rules when interpreting
final records. Do not commit to manufacture a clean candidate.

Handoff before benchmark: implementation/checks complete; performance pending.
Task remains open. Final handoff records behavioral result, TDD compliance and
benchmark qualification separately. Do not invoke reviews or commit automatically.
