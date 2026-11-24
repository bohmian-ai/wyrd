# Two-group pipeline implementation proposal

Status: finalized — maintainer agreed and requested aligned specification and one task.

## Recommended decisions

This section resolves the earlier alternatives below. It supersedes the original
group-commit proposal and review wherever their mechanics differ.

- **Depth:** exactly two group slots per shard, including collecting/preparing
  work. One ordered WAL producer and one fencing operation at a time. No depth
  setting, new shard count, append-during-sync or cross-group sync coalescing.
- **Progress:** after N's sync, start its fencing and prepare/write N+1. After
  N's visibility/ACK, release its slot immediately. Once N+1's WAL is durable,
  start its fence and use the freed slot for N+2. Repeat for the entire stream.
  With an empty input queue there is no artificial work; with two occupied
  slots there is intentional backpressure.
- **Collection:** preserve the 64-request cap and all existing byte/fairness
  bounds. While a predecessor is active, drain ready eligible requests; when
  fewer than 32 are collected and no eligible request is ready, wait at most
  1 ms from the first collected request. Continue draining ready requests above
  32 up to existing caps. The target ends waiting for arrivals, not draining ready input. Dispatch when
  ready input is drained and the target is met, or on cap, deadline or predecessor
  completion. Idle dispatch has no collection delay. Never reset the deadline
  on arrivals. These are selected initial private constants, not measured optima.
- **Events:** apply a ready head completion before initiating more preparation.
  Keep IO/fence futures alive across event-loop turns and poll them by reference.
  Bound mailbox draining by the existing capacity; service completions between
  bounded preparation passes. Offloaded conversion continues on existing lanes.
  No unbounded synchronous preparation loop may starve completion handling.
- **Visibility:** only the oldest dispatched group can fence or publish rows.
  Publish its inspection snapshot and committed mirror before success ACK.
  The next group's write/sync never gates this ACK. Tenant SQL stays batched
  and serial, with existing conflict and retained-version reconciliation.
- **Identity:** do not remove a queued repeated identity into the next group.
  Block it behind the previous owner without bypassing existing per-table FIFO;
  other eligible tables/tenants may proceed. Release the exclusion only after
  the previous group has settled its acceptance and visible ownership.
- **Known refusal:** if a group has no accepted or uncertain batches, finish
  submitted IO, release its ownership and return the existing error; advance
  to the next group. Normal per-batch conflict classification continues through
  existing tenant rollback/reoffer logic and is not a pipeline fault.
- **Partial operational failure:** if some tenants committed before another
  tenant's operational error, stop dispatch and subsequent fencing. Retain both
  slots, finish outstanding IO, and use the existing fail-closed restart path.
  Do not add an automatic background partial-group retry controller. Accepted
  batches recover from canonical fences; unfenced N+1 remains producer-retry
  work. A missing success response does not undo durable acceptance.
- **Ambiguity/insertion failure:** retain both states and charges and fail
  closed through existing readiness/recovery behavior. The existing bounded
  SQL reconciliation runs before declaring ambiguity unresolved. Never turn
  uncertain acceptance into a known refusal. Never retract an earlier ACK.
- **Rotation and seals:** defer generation mutation until dispatched groups
  settle. Project pending bytes exactly once before allowing the second group
  to write; drain first when rotation is required. Once a lifecycle barrier is
  pending, stop filling slots so it cannot starve. Preserve FlushAll's existing
  queue semantics. Shutdown drains submitted operations and retains unresolved
  durable state for restart; client cancellation does not cancel accepted work.
- **Scope:** internal Scribe scheduling only. No public API, wire format, WAL
  format, SQL schema, receipt policy or SDK authoring change. Existing admission
  owns both groups' resource charges; durable custody still governs cleanup.

## Best-practice grounding

[RocksDB's pipelined-write design](https://github.com/meta/rocksdb/wiki/Pipelined-Write)
uses the same causal principle: release the WAL stage to the next group while
the previous group completes its later stage. Wyrd's later stage additionally
requires PostgreSQL acceptance; RocksDB is precedent for stage overlap, not a
replacement for Wyrd durability or evidence for a predicted gain.

[Tokio's select guidance](https://tokio.rs/tokio/tutorial/select) explains that
dropping a future cancels its polling, while background operations may continue.
This motivates retained operation ownership and completion handles. The
[select reference](https://docs.rs/tokio/latest/tokio/macro.select.html) also
distinguishes concurrency from parallelism: blocking preparation must remain on
existing execution lanes, not inside a branch that prevents other futures polling.
The two-slot bound and shared byte accounting are repository-specific design
choices to retain backpressure without introducing another resource system.

## Outcome and evidence

Maintainer clarification: the motivation is observed waiting caused by serial
write execution. The selected problem is that later WAL work waits unnecessarily
for earlier acceptance work. Interleave those stages so the writer continues
making progress while an earlier group waits. This is the causal design premise,
not a request to rediscover the bottleneck through another benchmark. The later
benchmark quantifies the resulting end-to-end improvement.

Mechanism proof must exercise a stream of groups, not just one overlapping pair:
with backlog available, each freed slot admits the next eligible group while
the preceding group fences. Assert progress through repeated slot reuse and no
dependency on an unrelated later group's completion. Legitimate stops remain
when both slots are occupied, admission refuses work, rotation requires draining,
or acceptance authority is unresolved. Two slots hide a fence wait only while
there is capacity for later WAL work; they do not promise an always-busy disk
under arbitrarily slow fencing.

Allow the next admitted group to complete WAL work while the preceding group
awaits PostgreSQL acceptance, without delaying the preceding ACK on later IO.
Keep the existing durable acceptance, tenant isolation, resource admission,
read visibility and recovery contracts. No production changes or experiments
were performed for this proposal.

The maintainer requires the full Bifrost writes benchmark at task closeout and
wants implementation verification to proceed while other worktrees are busy.
Only [l1-baseline-74034459d.md](l1-baseline-74034459d.md) supplies benchmark
baseline evidence. Older performance numbers and target calculations are excluded.

Deterministic tests can prove eliminated dependencies, bounded work and correct
overlap. They cannot prove higher whole-server throughput or lower CPU cost on
real storage. Contention, group formation and scheduling overhead can outweigh
the removed wait. Report implementation verification separately from performance
qualification; defer the latter without marking the task closed.

## Existing owners and proposed changes

`crates/vala/vala-bifrost-redux/src/scribe/shards.rs::run` currently awaits
`process_group` end to end. `append_and_sync_batch_commits` combines COMMIT
construction, WAL submission, sync, tenant SQL and conflict resolution. Split
at the successfully synced WAL boundary; retain the existing `GroupWalState`
ownership rather than duplicating the write path.

Keep one shard coordinator, one ordered WAL producer and at most two dispatched
groups. Extract owned pending IO/fence operations from methods borrowing the
whole coordinator, so it can poll completions without conflicting mutable
borrows. Reuse `execution_lanes.rs::ScribeWalIoPool` and existing tenant fence
queries. Do not spawn two copies of `process_group`, add a runtime, introduce
a generic pipeline framework or change WAL/SQL formats.

The coordinator owns all reservations and generation mutations. Pending jobs
own only the material they need; results return their owned state. No dropped
future may abandon submitted WAL work. One persistent pending future/handle per
stage is sufficient; a select branch must not recreate operations each poll.

## Connected flow

1. Collect through the existing tenant/table scheduler and check admission,
   duplicate identities and complete generation projection.
2. Write SLICE/COMMIT and sync all touched segments for N through the WAL lane.
3. Start N's existing serial tenant fencing. While it waits, collect and prepare
   N+1 and dispatch its ordered WAL work when rotation and memory permit.
4. Apply N's fence resolutions in the coordinator, insert accepted rows, update
   the committed mirror, publish the inspection snapshot, then ACK. Ready N
   completion is serviced even while N+1 IO or collection is pending.
5. Once N is safely terminal, N+1 may fence. Reuse the free slot for the next
   group; never dispatch a third group while both slots are occupied.

Collection retains the existing request/byte limits and hierarchical fairness.
Use the original proposal's bounded soft-target collection only for an overlapping
slot; immediate idle dispatch remains. Its suggested target/interval are initial
private values, not demonstrated optima. A ready predecessor completion takes
precedence over waiting for collection; deadline expiry permits an underfilled
group. Test deadlines using controlled time, not wall-clock speed assertions.
Do not add public tuning options for this experiment.

## Failure and lifecycle policy proposed for adoption

| State | Required action |
|---|---|
| Known failure before any acceptance | Finish any submitted IO, refuse affected waiters, release only safely settled ownership; allow the next group after cleanup. |
| Normal conflicting or retained-version retry | Preserve existing per-batch classification and tenant rollback/reoffer behavior; settle accepted rows before advancing. |
| Earlier tenants accepted, later tenant has an operational failure | Retain accepted material and exact outcomes, drain submitted IO and fail closed for restart. Do not advance subsequent fencing or use pre-acceptance cleanup. |
| Ambiguous acceptance or post-fence insertion failure | Stop new dispatch/fencing, retain unresolved states and charges, drain already submitted IO, use existing reconciliation/restart authority. N+1 remains provisional until allowed to fence. |
| N+1 fails after N is accepted | Preserve N's visibility/ACK and recoverability; handle N+1 independently. |

Identity exclusion spans both slots using `(tenant, table, batch_id)`. Repeated
identities stay queued until the earlier owner settles, then use existing
deduplication/conflict resolution. Local exclusion does not replace cross-pod SQL
arbitration. Track outcomes explicitly enough to distinguish accepted subsets
from pending or refused work; do not invent another durable receipt ledger.

Before dispatch, rotation projection includes pending JSON/Arrow ownership not
yet in the memtable, counting each byte once. If rotation is needed, drain the
dispatched generation, close/freeze it through the existing owner, then dispatch
the next group. Selective seals, pressure-driven freezes and retirement must
respect the same ownership barrier. Shutdown stops new dispatch according to
the existing lifecycle, drains submitted work and preserves unresolved durable
ownership. Preserve existing FlushAll semantics.

Both slots and conversion overlap remain charged through existing global,
tenant and table accounting. Cleanup stays background-owned. A synced group is
provisional and cannot advance the custody frontier across unresolved accepted
ownership. No new acceptance roundtrip, per-receipt update or quota is introduced.

## Implementation sequence and observable proof

Treat this as one cohesive pipeline task with successive test-first scenarios,
subject to later reconciliation with the existing packet's delivery structure.

1. **Expose the stage boundary.** Preserve serial behavior while separating
   WAL durability from fence resolution and visibility. Reuse group state,
   canonical coordinates, segment ownership and existing failure tests. This
   is preparatory work within the first behavioral scenario, not a new public
   capability or separate framework.
2. **Prove overlap and independent completion through the real coordinator.**
   Block N's real tenant fence using the existing Postgres fixture pattern;
   submit distinct N+1 and observe its sync completion before releasing N.
   Separately hold N+1's IO completion, release N and observe N's visible rows
   and successful ACK while the hold remains. Serial code fails the first
   ordering; a join-barrier pipeline fails the second. Use explicit reached/
   release handshakes and bounded deadlock timeouts, never “faster than X ms.”
3. **Prove bounded scheduling and collection.** Preload known queues and assert
   membership, request/byte bounds, tenant/table fairness, two-slot maximum,
   same-ID exclusion and immediate idle dispatch. Exercise queue exhaustion,
   arrivals and deadline expiry with controlled time. Count real WAL submissions,
   syncs and SQL transactions for fixed groups to detect extra work. These counts
   protect mechanism behavior, not production occupancy or CPU claims.
4. **Prove failure, rotation and recovery together.** Exercise the policy table,
   multi-tenant partial success, duplicate/conflicting cross-pod retries,
   retained versions, segment boundaries, both-slot memory pressure, sealing,
   shutdown, restart and custody/receipt cleanup. Accepted rows survive once;
   unfenced attempts do not become visible or replay as accepted. All retained
   charges have an owner and release exactly once after settlement.
5. **Exercise shipped journeys and qualify later.** Real SDK → WyrdTestServer
   and repository Postgres → SDK proves durable completion and read visibility.
   Use existing Scribe/OTLP and SDK journey owners, including shipped Python,
   TypeScript and MCP/HTTP paths as applicable. Run required format/lints and
   scoped integration lanes. Leave performance qualification pending until the
   maintainer's other work is done.

Existing Postgres group tests in `shards.rs` already block a fence key and replay
the WAL; extend that fixture pattern. Keep new hooks test-only and narrow, at
actual stage transitions. Poll the production coordinator and real WAL owner;
a simulated scheduling model alone is insufficient. Paused time belongs in
collection tests, not real Postgres/IO tests. Busy-host timeouts indicate an
inconclusive run, not a throughput regression.

Existing runners: `mise run test:bifrost:integration:redux`,
`mise run test:bifrost:journey:scribe`, `mise run test:bifrost:journey:otlp`
and the existing SDK journey lanes. Implementation records exact nextest
selectors once new tests exist. No tests were executed during this design.

## Deferred performance closeout

Run `mise run benchmark:l1:bifrost-writes` (all eight routine write cases) on
the completed candidate after competing work is finished. Compare matching
cases/settings against the designated baseline and retain full records. A
VALID sweep establishes measurement/integrity validity, not improvement.
Report throughput, CPU cost and regressions separately; do not revive obsolete
targets. Read-protection and previously required sustained-load evidence remain
part of their existing qualification scope and may also be scheduled later.

This latest maintainer direction supersedes intermediate benchmark-before-next-
scenario checkpoints for this proposed pipeline work. Deterministic tests permit
continued implementation; the required closeout benchmark remains outstanding.
If results are inconclusive or regress, do not claim the performance objective
achieved. Revisit batching/overhead or reject the overlap based on the evidence.

## Handoff and decisions

The maintainer selected the mechanism and failure policy. [Spec revision 4](../spec.md)
and [the single pipeline task](../tasks/TASK-003-bounded-write-pipeline.md) record
this agreement and replace per-scenario benchmarks with deferred closeout.
Exact helper placement and test-hook shape remain implementation-owned.
Performance acceptance: all eight write cases
VALID, repeatable concurrent-write throughput improvement beyond observed run
variation, and no repeatable regression in single-client behavior, CPU efficiency
or existing read protections. Mixed or inconclusive evidence leaves qualification
pending for maintainer disposition. Use the designated baseline's underlying
records to assess variation; repeat candidate measurements when necessary.
Incompatible measurement settings make comparison inconclusive, not grounds to
silently replace the baseline. Historical numerical targets and a successful
benchmark exit alone do not establish improvement.
