---
id: TASK-005-R2
kind: remediation
status: review
spec: SPEC-forge-concurrent-planning
spec_revision: 11
parent_task: TASK-005-R1
remediates: [FIND-TASK-005-R1-1, FIND-TASK-005-R1-2, FIND-TASK-005-R1-3, FIND-TASK-005-R1-4, FIND-TASK-005-R1-5]
---

# Make active-read lifetime and destructive authority continuous

## Contract and candidates

- Approved spec: `changes/active/forge-concurrent-planning/spec.md`
- Original task: `changes/active/forge-concurrent-planning/tasks/TASK-005-R1-active-table-reader-cut.md`
- Reviewed candidate/base: `7ac45dec99535c881b7a936c66623044f15d8823` / `c1508b375`

## Diagnosis

Five revision-11 gaps share the active-read boundary. The common query stream
owns Interactive execution, but Analytical transfers its graph lifecycle into
`AnalyticalSupervisor`; dropping the leader stream therefore only signals that
detached owner instead of dropping every follower with the stream. The active
claim then starts release independently. This violates the locked ownership
rule: the leader stream owns the complete query lifetime, and no follower or
query IO may exist after that stream drops. Acquisition and its permitted retry
also reuse an early duration, letting PostgreSQL rebase `abandon_after` past the
one query deadline. Forge's three destructive paths reduce exclusive authority
to a completed reader check and commit its transaction before the external
catalog/object effect, making the forbidden check/effect gap representable: a
new reader can commit the old cut after authority was surrendered. In addition,
`production_closeout` still bans unrelated sibling expiry/cleanup that revision
11 permits, and Redux duplicates raw `bifrost_tables` SQL instead of using the
SQL-layer owner.

## Intended correction outcome

The leader stream is the sole lifetime owner for Interactive and Analytical
execution alike: dropping it drops every follower and makes further query IO
impossible before its claim releases. Claims expire at exactly the existing
query deadline. Destructive work is possible only while its exclusive table
authority owner remains live, so reader-check and destructive effect cannot be
separated. Tests permit independent sibling maintenance, and durable SQL
remains owned by `vala-sql`.

## Decision-complete recommendation

- Remove the Analytical lifetime split at its source. The leader stream must
  own the active-read claim and the complete Analytical graph/follower
  lifetime in the same owner that already owns Interactive execution. The
  supervisor may index, route, and observe work, but it must not own a graph
  lifecycle that survives the leader stream. Terminal settlement consumes that
  stream-owned graph before releasing the claim; dropping the stream drops or
  revokes every follower and closes every path capable of further metadata or
  file IO before claim release begins. Do not retain, transfer, or recreate a
  background query-lifetime owner to make cleanup convenient.
- Retain the attempt's immutable local deadline and derive a positive remaining
  duration immediately before every SQL acquisition/reacquisition; PostgreSQL
  still stamps the row. Reject acquisition once no duration remains.
- Make destructive authority an owned capability, not a check result. Reuse the
  existing per-table maintenance authority, but expose destructive operations
  only while one scoped exclusive authority owner is live; that owner retains
  the conflicting PostgreSQL authority continuously from reader exclusion
  through the external effect's known outcome. No caller may obtain a boolean,
  row, or prepared value proving an earlier check and then perform destruction
  after dropping the authority owner. Oracle acquisition uses the conflicting
  shared authority, so either the reader commits first and blocks destruction,
  or destruction finishes first and the reader selects the later pointer.
  Preserve uncertain-effect evidence and idempotent recovery. Do not recreate
  epochs, IO gates, advisory locks, per-query sessions, or a second protocol.
- Delete the tenant-wide sibling-strategy ban while preserving exact orphan
  identity, terminal phase, deletion, survivor, and query assertions.
- Replace the Redux raw registry query with the existing typed `vala-sql`
  catalog owner; retain `TableAuthority` as orchestration, not SQL ownership.

## Preserved behavior and non-goals

- Preserve tenant RLS, one-statement acquisition, explicit uncapped deadlines,
  promotion/non-destructive catalog movement, exact cut reconciliation, and
  nonblocking stream drop.
- Preserve immediate maintenance after final release and all real roots.
- Do not reintroduce any deletion claim prohibited by revision 11.
- Do not solve either ownership defect with repeated cancellation checks,
  post-hoc joins, timing assumptions, or downstream reader rechecks.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-005-R1-1` | The leader stream directly owns the Analytical graph and every follower; no supervisor-owned query lifetime survives it, dropping the stream makes all descendant IO impossible, and only then may the claim release. |
| `FIND-TASK-005-R1-2` | Initial acquisition and retry both expire at the original absolute query deadline. |
| `FIND-TASK-005-R1-3` | Every destructive effect requires a live exclusive maintenance-authority owner, and the type/API provides no state in which authority was checked, dropped, and later acted upon. |
| `FIND-TASK-005-R1-4` | Exact orphan evidence passes while independently eligible sibling expiry/cleanup is allowed. |
| `FIND-TASK-005-R1-5` | No raw `bifrost_tables` query remains outside its SQL owner. |

## Focused proof and broader verification

For both local and remote Analytical execution, drop the leader stream while
followers are active and prove that every follower owner disappears, no later
metadata/file IO occurs, and the claim releases only after that synchronous
ownership collapse. Delay acquisition and forced reacquisition and assert
PostgreSQL-time expiry at the original deadline. Pause each destructive path
while its exclusive authority owner is live, race acquisition, and prove the
reader cannot commit until either destruction finishes and exposes the later
pointer or the destructive owner yields without effect; no test-only recheck may
establish this. Run closeout with an eligible sibling, tenant isolation and
authority tests, Oracle/Forge journeys, `verify:bifrost`, principals integration,
boundary checks, format, lints, and diff check.

## Implementation evidence

Decision applied to R1-1, relayed by the coordinator: "human decision
2026-10-05: revocation semantics; remote in-flight IO without a consumer may
complete after release" (spec rev 11 REQ-014). Under that decision, the focused
proof line for R1-1 reads: **every follower is revoked and every local driver
is aborted before release, and no consumer receives data after drop.**

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-005-R1-1` | `oracle/analytical.rs`: `AnalyticalGraphLifecycle` is owned inline by `AnalyticalAttemptOwnership` inside `AdmittedQueryGuard`, with no spawned lifecycle owner. Its `Drop` does the following synchronously, in order: drain, cancel, close exchanges, drop grants (revoking followers), abort local drivers (`AnalyticalAttemptState::drop`, plus `finish_attempt` joining in place), drop the fold, and only then hand the capacity-only envelope release to `AnalyticalGraphRelease::reclaim`. `analytical_supervisor.rs` no longer has any lifecycle field, owner, signals, or task (only `begin_draining`/`ensure_graph_active`). `query_stream.rs`: `LeaderStreamOwners::drop` drops `admitted` before `active_reads`, and settlement borrows the claim, taking it only after the graph settles. `dispatcher.rs`: `ParticipantGrant` stream is held in a `Mutex` so the grant is `Sync`. | `oracle::analytical::tests::leader_lifecycle_settles_every_owner_and_retains_failure` (`assert_owner_drop_revokes_before_returning` checks, synchronously after drop and before any await: both transport releases, zero live attempts, cancelled token, refusal from `ensure_graph_active`; it then checks the driver channel closed and the graph released). Also `a_dropped_owner_with_an_undrained_envelope_retains_its_graph` and the reservation tests `assert_cancellation_interrupts_a_pending_reservation`, `assert_the_deadline_interrupts_a_pending_reservation`, `assert_terminal_drops_every_grant`. Oracle unit modules 76/76. | PASS |
| `FIND-TASK-005-R1-2` | `catalog/bifrost_catalog.rs::acquire_active_cut(tenant, owner, deadline: Instant, tables)` derives the positive remaining duration immediately before each SQL acquisition or reacquisition and refuses once none remains. `oracle/planner.rs` passes the attempt's immutable deadline. | `forge::reader_expiry_ordering::delayed_and_replayed_acquisitions_expire_at_the_original_deadline`; `pg_oracle_membership::active_table_read_claim_is_atomic_tenant_scoped_and_postgres_expired` | PASS |
| `FIND-TASK-005-R1-3` | `vala-sql/queries/oracle_reader_authority.rs`: `BifrostTableMaintenanceAuthority::exclusive` returns a borrowed `ExclusiveTableAuthority<'conn,'tx>` capability that holds the conflicting row lock in the open transaction. `forge_operations`/`forge_tasks` destructive preparation requires it (`require_exclusive_authority`). `forge/expire.rs`, `orphan_gc.rs`, `worker.rs` and `table_authority.rs` perform each external effect while that owner is live, before the commit; no boolean or row result outlives it. Oracle acquisition takes the conflicting shared authority. | `reader_expiry_ordering::{reader_racing_snapshot_expiry_observes_the_committed_pointer, reader_racing_orphan_delete_commits_after_it, last_table_reader_controls_destructive_cleanup}`, `forge::expired_cleanup` (`assert_preparation_committed_under_held_authority`), `pg_forge_operations`/`pg_forge_tasks` exclusive-preparation tests, Forge journey | PASS |
| `FIND-TASK-005-R1-4` | `wyrd-testing/tests/bifrost/forge/production_closeout.rs`: removed the tenant-wide sibling-strategy ban. Exact orphan identity, terminal phase, deletion, survivor and query assertions (`assert_orphan_evidence`) are kept; `scribe_promotion.rs` was adjusted to match. | `test:bifrost` Forge journey (production closeout) | PASS |
| `FIND-TASK-005-R1-5` | Redux `scribe/write_recipe.rs`, `staging_runtime.rs` and `persistence.rs` read layout through the new `vala_sql::queries::olap_catalog::registered_physical_layout` (operator lane, explicit tenant, sanctioned `tenant-isolation: cross-tenant OperatorPool` marker). `grep bifrost_tables crates/vala/vala-bifrost-redux/src` finds only a doc comment. | `check:tenant-isolation`, `verify:bifrost` | PASS |

### Commands

- `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/oracle::(analytical|analytical_supervisor|query_stream|admission|dispatcher)::/)'`: 76/76.
- `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::analytical::tests::leader_lifecycle_settles_every_owner_and_retains_failure)'`, and the same exact form for `a_dropped_owner_with_an_undrained_envelope_retains_its_graph`: PASS.
- `scripts/postgres/with-test-postgres.sh mise exec -- cargo nextest run --locked -p vala-bifrost-redux --test integration -E 'test(/reader_expiry_ordering::/) | test(/expired_cleanup::/)'`: PASS.
- `mise run test:sql` (vala-sql 102/102): PASS.
- `mise run verify:bifrost` (check:bifrost plus test:bifrost, which includes the Oracle and Forge journeys): PASS.
- `mise run test:principals:integration` (19/19 OpenAPI contract): PASS.
- Boundary checks `mise run check:tenant-isolation`, `check:client-tier`, `check:pyo3-scope` and `check:unwrap-audit`: PASS.
- `mise run fmt`, `mise run lints` and `git diff --check`: PASS.

### Scope and material limits

- Remote in-flight IO without a consumer may finish after release, per the decision above. Local drivers are aborted and remote followers are revoked synchronously before the claim release begins.
- `AnalyticalGraphRelease::reclaim` is still a spawned task, but it owns no grant, driver, claim or consumer. It only drains the supervisor's capacity envelope within the original deadline. Without a runtime, or on drain failure, the graph is retained as an unhealthy cleanup failure, never released early.
- The claim-after-revocation ordering in `query_stream.rs` is structural (the explicit `LeaderStreamOwners` drop order plus a `&mut` borrow of the claim). It has no dedicated unit test, because an `ActiveReadClaim` needs a real catalog. The synchronous revocation it depends on is unit-proven.
- Supervisor shutdown now drains graphs instead of joining lifecycle tasks. Production Oracle shutdown never invoked leader supervisor shutdown.
- Test-only raw `bifrost_tables` SQL remains in integration support and diagnostics, outside production code.
- `forge/expire.rs::retry_committed_expiry` was extracted only to satisfy `clippy::too_many_lines`, with no behavior change.
- Non-goals stayed excluded: no epochs, IO gates, advisory locks, per-query sessions or second protocol, no reintroduced deletion claims, and no gc.rs changes (TASK-001-R1 owns it).

## Follow-up evidence (coordinator review before task review)

### 1. Claim-after-revocation ordering is now tested

- Seam (test-support only): `oracle/planner.rs` adds `LeaderOwnershipEvent` and
  `take_leader_ownership_order_for_test`. Each transition is appended
  synchronously where it happens:
  - `GraphRevoked` is appended in `AnalyticalGraphLifecycle::settle`, after
    grants drop and the attempt join, and in its `Drop`, after grants drop and
    drivers abort.
  - `ClaimReleaseStarted` is appended in `ActiveReadClaim::release` and in
    `ActiveReadClaim::drop`.
- Journey: `distributed::held_cut_owns_active_reads_until_all_descendants_settle`
  now asserts, per query id, that the graph revocation precedes the claim
  release in each case:
  - settled query;
  - dropped caller;
  - new: a returned leader stream dropped unread;
  - dropped pin, where no graph is expected.
- Red/green: with `LeaderStreamOwners::drop` inverted, the journey failed with
  `a dropped stream: query … released its claim before revoking its graph:
  [(…, ClaimReleaseStarted), (…, GraphRevoked)]`. With the correct order it
  passed (3/3 runs).
- The red run exposed a real gap, now fixed. The owners were built inside the
  `stream!` generator, so a stream dropped before its first poll dropped
  `admitted` and `active_reads` as separate captures in unspecified order.
  `build_frames` now constructs `LeaderStreamOwners` before the generator, so
  its `Drop` fixes the order on every drop path.

### 2. `ParticipantGrant` Mutex removed

- The grants are now held by value:
  `AnalyticalGraphLifecycle.grants: Option<AnalyticalParticipantGrants>` plus a
  `reserved: bool` (a completed round, granted or refused; avoids
  `clippy::option_option`).
- `publish_participants`, `reserve`, `reserve_round` and
  `fold_physical_metrics` take `&mut self`. A `Send` future then needs only
  `Self: Send`, not `Sync`.
- The production caller uses `admitted.analytical.as_mut()`.
- `dispatcher.rs` is back to its base `_stream: Box<dyn Send>`.

### 3. Raw `bifrost_tables` SQL outside `vala-sql`

Every remaining hit is test-only:

- `wyrd-server/tests/*` and `pg_invocation_tests.rs`;
- `vala-bifrost-redux/tests/integration/forge/support.rs`;
- `wyrd-testing` (both `src/server.rs` harness helpers and `tests/…`).

`wyrd-testing` is consumed only as a dev-dependency, or behind the Python SDK
`testing` feature. No production code changed for this item.

### 4. Exclusive-authority hold bound

| Path | Objects deleted per hold | Hold scope |
|---|---|---|
| Expired cleanup (`worker.rs::drain_expired_cleanup`) | 1 | The authority is taken per candidate. It covers one prove (protection load, one stat, fence) and one delete, then is committed before settlement. |
| Orphan GC (`orphan_gc.rs::delete_gc_batch`) | at most `max_gc_candidates_per_batch` (default 256) | One hold per prepared batch, covering at most 2 object-store calls (stat + delete) and 2 lease fences per candidate. Fresh runs also stop at the run deadline at a candidate boundary. |
| Snapshot expiry (`expire.rs`) | 0 objects | One Iceberg metadata commit. File deletion is deferred to expired cleanup. |

- The orphan-GC hold is already bounded per batch by the existing cap; no
  config knob was added.
- Worst case for one orphan hold: 256 × (stat + delete + 2 fences).
  `wyrd-storage` sets no per-request object-store timeout, so wall time is
  256 × the backend's per-request latency. That is about 1–2 s at typical
  ~2–5 ms S3 latencies, but it is unbounded if the backend hangs.

### 5. Capacity journey spill-directory flake

- Symptom: `capacity::lowest_rung_analytical_contention_preserves_two_interactive_tenants`
  intermittently ends with `spill_directories` 3 → 4 after the clean-node poll.
- Evidence (WYRD_LOG plus temporary `strong_count` probes, since removed):
  - every release in passing runs logged exactly 3 strong refs on the graph's
    `Arc<RuntimeEnv>`, on all three nodes;
  - those 3 are the graph's own: `AnalyticalGraphState::runtime`, the
    envelope's `OracleQueryResources::execution`, and the follower
    `AnalyticalRuntimeRegistry` entry;
  - the leader's settle probes also logged 3 at every stage, so no leader
    holder was involved in this journey.
- Cause: release was gated only on the envelope's nested memory
  (`graph_children_debt`). The per-query `datafusion-*` spill directory belongs
  to the `RuntimeEnv`'s `DiskManager` and is removed only when its last `Arc`
  drops. A follower's upstream task context (`TaskData.task_ctx`, kept in the
  coordinator-channel task, a pending cache invalidation, or a late
  `ExecuteTask` resolve) can hold the runtime after every reservation has
  returned. The graph then released and left the directory to trail it. The
  read-only diagnostician's ranked report gives this as the first cause.
- Rejected fix (tried first, then reverted): gating graph release on
  `Arc::strong_count` beyond the graph's own 3 references. It turned three
  Oracle journeys red: two `analytical_lifecycle` journeys and
  `distributed::published_workers_and_live_scribes_share_one_plan`. Each logged
  "not confirmed drained … memory_bytes=0". Legitimate holders, such as the
  leader's executing plan and upstream task caches, outlive settlement by
  design, so waiting for them retains healthy graphs as residue.
- Fix site: the query envelope, `OracleQueryResources` (`resources.rs`).
  - Its `Drop` now calls `OracleExecution::remove_spill_directories`, which
    removes the runtime's `DiskManager::temp_dir_paths()` synchronously.
  - The graph owns this envelope, so the directory is removed in the same
    `release_graph` collapse that returns the envelope, whoever still holds
    the `RuntimeEnv`. Interactive queries get the same at their envelope drop.
  - A late holder that spills afterwards gets an IO error on a query that is
    already over. `TempDir`'s own later drop ignores the missing path.
  - No poll bound changed. The capacity comment that blamed trailing cache
    invalidation was corrected.
- Proof:
  - New `resources::tests::query_spill_directory_ends_with_its_envelope_not_its_runtime`.
    It is red with the removal disabled (the directory survives while a
    runtime holder lives) and green with it.
  - All 231 `oracle::` and `resources::` unit tests pass.
  - `mise run test:bifrost:journey:oracle` passed 49/49, including the held-cut
    and capacity journeys.
  - The exact capacity journey passed 6/6 on the normal lane with WYRD_LOG on.
  - `mise run test:bifrost:journey:forge` passed 21/21.
