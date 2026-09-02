---
task_id: BIFROST-T1-C05-R1
title: Reconcile slice 5 into one exact, drain-safe follower GraphLease
kind: reconciliation-remediation
status: superseded
execution_skill: wyrd-implement
implementation_style: tdd
approved_authority: bifrost-distributed-analytics-engine-plan-v7
spec_id: bifrost-distributed-analytics-engine
spec_revision: 7
requirements:
  - R2
  - R3
  - R4
  - R7
  - AC1
  - T1-U-C05
parent_task: BIFROST-T1-UNIFIED-PEER-REMEDIATION
frozen_candidate: f1ac4cb01fe9ddda0a133cb58c955bab1e1cf7df
depends_on:
  - T1-U-C01
  - T1-U-C02
  - T1-U-C03
  - T1-U-C04
remediates:
  - S5-RF-01
  - S5-RF-02
  - S5-RF-03
  - S5-RF-04
  - S5-RF-05
  - S5-RF-06
  - S5-RF-07
  - S5-RF-08
  - S5-RF-09
superseded_by:
  - BIFROST-T1-C05-R2
---

# Reconcile slice 5 into one exact, drain-safe follower GraphLease

## Outcome

The existing `ReservationRegistry` transfers one graph-purpose reservation
into one exact follower-local `GraphLease`. The lease binds the complete graph
authority and owns the admitted query envelope until every stage call, task,
stream, cache entry, exchange buffer, spill operation, attempt, and cleanup
task has terminated. Refusals do not consume another owner's reservation,
activation failures roll back synchronously, retry can advance only after the
prior attempt drains, and exchange charges reflect bytes actually owned by the
pinned distributed engine.

This is the minimum cohesive successor to slice 5. All findings affect the
same reservation-to-graph ownership transition and its terminal release; a
split would create an intermediate state in which one owner assumes authority
or lifecycle guarantees another task has not installed.

The active packet predates the current spec-file convention. Its explicitly
approved behavioral authority is [`intent.md`](../intent.md) plus Plan v7 in
[`plan.md`](../plan.md), refined—not replaced—by the approved unified peer-plane
remediation. This task does not revise that behavior.

## Frozen candidate and current-state amendment

The candidate is commit
`f1ac4cb01fe9ddda0a133cb58c955bab1e1cf7df` (`test(bifrost): prove one graph
lease per follower and return every reservation`), whose parent is
`56252b48aabb40105278c7cbae33dca753fe254f`. At reconciliation time there are
no uncommitted changes under `crates/`; unrelated skill edits and the active
change packet are outside the frozen production candidate. No compilation or
broad test baseline was run by planning.

### Retain

- One `ReservationRegistry` and one Oracle resource root. Keep the closed
  Fragment-versus-Graph capacity distinction; do not add another registry or
  resource governor.
- The graph-qualified reservation request, one atomic pending-to-active
  transition, `Arc<GraphLease>` reuse for subsequent stage messages, and the
  deletion of follower self-grants and synthetic reservation IDs from
  `56252b48a`.
- Leader-side `AnalyticalParticipantReservations` and the explicit settled
  release path, after replacing its detached fallback as required below.
- The real three-Oracle/one-Scribe journey topology and its proof that a normal
  distributed query activates once per follower and returns to zero live
  leases.
- The existing `AnalyticalSupervisor`, `AnalyticalStageIngress`,
  `AnalyticalStageEgress`, `OracleQueryResources`, and `OracleSpillRuntime` as
  the concrete owners to repair. No new service layer is required.

### Delete or replace

- Replace `graph: Option<AnalyticalGraphRef>` with an explicit closed
  reservation purpose; absence must not be the durable discriminator between
  resource envelopes.
- Preserve the approved pending lifetime for both purposes:
  `min(requested_expiry, now + PENDING_TTL)` with the existing two-second
  `PENDING_TTL`. Move graph reservation to the completed-plan dispatch edge so
  no planning or topology work occurs between reservation and first
  activation.
- Delete fixed per-attempt exchange reservation as accounting authority and
  delete `measured_exchange` as proof of ownership. It may remain only as
  bounded telemetry after actual accounting is authoritative.
- Delete fire-and-forget cleanup from `Drop`, including
  `AnalyticalConnectionLease` and `AnalyticalParticipantReservations` paths.
  `Drop` may signal cancellation to a retained owner; it may not spawn work or
  release a lease.
- Replace the graph-drain timeout's log-and-release behavior. A timeout is a
  terminal cleanup failure and retains the owner for inspection/shutdown.

### Invalidated completion claims

- `GraphLease::matches` over reservation ID, graph IDs, and query ID is not the
  “complete ownership tuple” claimed in candidate prose.
- The current journey proves basic multi-process activation/release but not
  exact authority, exact resource identity, actual exchange ownership,
  refusal preservation, activation rollback, retry reuse, or terminal drain.
  Its second SQL execution is a new graph and is not retry evidence.
- A graph envelope is not safely reusable merely because the coordinator body
  happens to remain open. Slice 6 may close that body after attempt-zero drain;
  retry retention must be explicit before slice 6 relies on it.
- Current cleanup is not structured: detached tasks, best-effort drops, map
  removal before drain, and fail-open timeout contradict the approved lease
  lifecycle.

### Unfinished findings

| ID | Current evidence | Required closure |
|---|---|---|
| S5-RF-01 | The leader takes graph reservations before it has finished the work needed to activate them, so the approved two-second pending window can expire during planning. | Finish graph shape/authority first, then reserve immediately before minting and sending the first stage operation; preserve the existing clamp. |
| S5-RF-02 | `take_for_execute` removes an entry before discovering `ReservedCapacity::Graph`. | Validate purpose and full tuple before removal; a refused operation leaves the exact reservation usable. |
| S5-RF-03 | `admit_graph` takes lease resources before fallible runtime construction and supervisor registration, with `?` exits that retain the registry lease. | Roll back the exact activation synchronously on either error. |
| S5-RF-04 | `GraphLeaseRequest` and `GraphLease::matches` bind only reservation ID, graph IDs, and query ID. | Bind the complete authority tuple and a follower-issued reservation generation. |
| S5-RF-05 | `AnalyticalStageEgress::record` unconditionally overwrites the participant cut, deadline, authority, and attempt on every RPC. | First authorized message fixes graph authority; later RPCs compare exactly. Only an explicit post-drain attempt transition may advance the attempt. |
| S5-RF-06 | connection and unused-reservation `Drop` paths spawn unretained tasks; `release_graph_if_idle` removes ownership before drain; drain timeout returns success. | Retain, cancel, join, and inspect cleanup; never release on an unproved drain. |
| S5-RF-07 | retry reuse depends on the coordinator request body remaining open. | Add an explicit graph retry hold and drain-before-advance seam without implementing slice 6's retry policy. |
| S5-RF-08 | supervisor reserves a configured estimate while upstream queues own differently sized buffers; `measured_exchange` observes but does not govern them. | Account the pinned dependency's actual `WorkerConnection` and `NetworkBoundary` reservations in the graph pool. |
| S5-RF-09 | the journey covers success, repetition, activation count, and final live count only. | Extend the same journey with the authority, refusal, lifecycle, resource, retry-seam, and terminal cases below. |

## Scope and consumers

Primary owners:

- `crates/wyrd-spec/src/vala/api.rs` and the matching private peer protobuf /
  `wyrd-tonic` conversion for typed reservation purpose and generation;
- `crates/vala/vala-bifrost-redux/src/oracle/dispatcher.rs` for pending and
  active reservation ownership;
- `crates/vala/vala-bifrost-redux/src/oracle/{peer,analytical_transport,analytical,analytical_supervisor}.rs`
  for ticket binding, graph admission, immutable authority, retry hold, and
  structured cleanup;
- `crates/wyrd/wyrd-testing/src/bifrost/process_cluster.rs` and
  `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/analytical.rs`
  for process-level controls and evidence.

Direct consumers that must be updated together are reservation minting and
release in the leader, `OraclePeerGrpc`, StageGraph ticket mint/verify,
SetPlan/ExecuteTask/coordinator-channel adapters, runtime registry/provider
lookup, the Analytical supervisor, child-process control protocol, generated
descriptor/schema surfaces, and resource-governance checks.

Non-goals:

- Do not implement slice 6's automatic retry classifier, egress latch policy,
  or user-facing retry behavior. This task supplies and proves the exact
  drain-before-attempt-advance ownership seam it will call.
- Do not implement slice 7 physical-operator/spill qualification or slice 8
  production-path activation and manifest cleanup.
- Do not change public production routing; StageGraph remains inactive.
- Do not fork or vendor `datafusion-distributed`, add a second resource root,
  introduce a generalized reservation framework, or add a trait around a
  single implementation. This task is blocked pending the draft KISS v1 spec;
  after approval `$wyrd-plan` will replace it with a simpler successor.

## Design-closure ledger

### 1. Exact reservation and GraphLease authority

**Owner and location.** `ReservationRegistry` remains the sole process owner in
`oracle/dispatcher.rs`. `wyrd-spec::vala::api` owns the language-neutral private
wire values. Stage ticket claims remain in `oracle/peer.rs`.

**Identity.** Introduce closed `NodeReservationPurpose::{Fragment,
Graph(GraphReservationAuthority)}`. `GraphReservationAuthority` contains:

- public and DataFusion query IDs;
- authenticated source `PrincipalId`, `DataTenantId`, and `SpaceName`;
- leader node/fence and destination node/fence;
- query class;
- snapshot digest, permission digest, immutable participant/source digest,
  and one digest over this complete graph authority;
- absolute query deadline.

The request's existing query ID, slot demand, requested expiry, leader/fence,
and class remain common fields and must equal the graph authority projection.
The receiver derives the source principal from `AuthenticatedPeerContext` and
compares it; a wire value never authenticates itself. Credential and
certificate digests are audit evidence, not lease identity, so normal key or
leaf rotation cannot change an active graph's principal.
The follower returns `ReservationId`, effective expiry, and a nonzero
monotonically allocated `ReservationGeneration`. Stage tickets and releases
carry the exact ID/generation plus the authority digest. The active
`GraphLease` retains the full typed authority, generation, expiry, permit,
`OracleQueryResources`, a once-installed query runtime, actual exchange
account, spill share, cancellation token, and deadline. The supervisor's graph
state holds the same `Arc<GraphLease>`; it must not take the resources into a
second owner.

**Transitions and atomicity.** Under the registry lock:

1. `Absent -> PendingGraph` validates destination/fence, full authority,
   demand, and deadline, allocates capacity and generation, then inserts.
2. `PendingGraph -> ActiveGraph` first compares every field and purpose without
   mutation. Only an exact match removes the pending entry and inserts the
   active `Arc<GraphLease>`, moving the resource envelope into that lease. A
   second exact stage message returns the same `Arc`.
3. Fragment execution can consume only `Fragment`; graph activation can consume
   only `Graph`. Any mismatch/refusal leaves the entry unchanged.
4. Release is idempotent only for the exact ID/generation/authority. A stale or
   foreign release is refused and cannot affect a newer lease.

Allocate generation from one registry-local `AtomicU64`, refusing wrap to zero;
it is process-local fencing, not a new durable database identity. Lock order is
registry pending map before active graph map whenever both are needed; never
hold either lock across async work.

**Expiry and dispatch order.** Every pending reservation, Fragment or Graph,
keeps the approved `min(requested_expiry, now + PENDING_TTL)` calculation with
the existing two-second `PENDING_TTL`; the absolute query deadline remains the
outer bound supplied as `requested_expiry`. The leader must first finish the
distributed physical graph, freeze participants/sources, compute the checked
exchange shape, and build the complete authority digest. Only then does it
reserve every selected follower. On a complete fan-out it immediately mints
and sends the first SetPlan/ExecuteTask operation; it performs no planning,
membership read, shape calculation, or retry backoff between reservation and
activation. If the fan-out cannot complete and activate inside the pending
window, it releases exact reservations and returns the existing typed
admission/unavailable outcome. It does not lengthen or refresh them. Expiry
removes only pending entries and drops their resources. An activated lease is
no longer governed by pending TTL; its absolute query deadline cancels
descendants and enters structured drain.

**Failure/recovery.** A leader crash leaves pending capacity until bounded
expiry and active work until deadline/cancellation cleanup. Node restart loses
process-local leases and changes the destination fence, so old generations and
tickets cannot reactivate. There is no cross-restart lease recovery because no
follower process work survives restart.

**Falsification.** `graph_reservation_refusal_preserves_exact_owner` covers
wrong purpose, graph, query, tenant/space, source/destination fence, digest,
generation, expiry, and release, then activates the original exact request
within two seconds. It also proves the leader's reservation call occurs after
the finalized graph/authority/shape and immediately before first dispatch.

**Rejected alternatives.** Do not retain optional graph fields, use stringly
purpose tags, create a second graph registry, or persist generations in
Postgres. They either preserve ambiguity or add coordination without a
surviving resource to recover.

### 2. Rollback-safe admission and immutable graph authority

**Owner.** `AnalyticalStageIngress::admit_graph` owns activation orchestration;
`AnalyticalStageEgress` owns the follower's adopted outbound graph authority.
Replace the ingress `graphs` value with the closed
`GraphAdmissionEntry::{Activating, Active}`. `Activating` holds the exact
authority digest and one `Arc<GraphActivation>` containing a mutex-protected
`Building | Published | Failed(GraphActivationFailure)` result plus one Tokio
`Notify`. `Active` holds the single graph lifecycle owner, shared lease/runtime,
and supervisor guard. This is a graph-local latch, not a new service or generic
initialization framework.

**Owner election and synchronization.** After ticket verification, the first
SetPlan or ExecuteTask may activate. `admit_graph` becomes async and performs
this exact protocol:

1. Lock the graph map. Absent inserts `Activating` and becomes the sole owner.
   An exact `Activating` duplicate clones the latch and becomes a waiter. An
   exact `Active` duplicate returns the published shared graph. Any authority
   mismatch refuses without changing the entry.
2. Release the graph-map lock. A waiter creates its `Notify::notified()` future
   before rechecking the latch result, then loops until `Published` or `Failed`
   so publication cannot be missed. It never calls registry activation,
   rollback, egress record, runtime build, or supervisor registration.
3. The owner alone calls `ReservationRegistry::lease_graph`, builds the
   partitioned query runtime from the lease's resources, installs that runtime
   into the lease exactly once, records the immutable egress authority, and
   registers the same `Arc<GraphLease>` with the supervisor. `GraphLease`
   exposes borrowed resource/runtime views; delete destructive
   `take_resources`.
4. On success, the owner reacquires the graph map, verifies by `Arc::ptr_eq`
   that its `Activating` entry is still current, replaces it with `Active`, and
   only then stores `Published` and notifies all waiters. Every waiter therefore
   observes the single shared lease/runtime/guard before proceeding to decode,
   cache, provider, or IO.
5. On runtime build, runtime installation, egress record, supervisor
   registration, poisoned-lock, or shutdown refusal, the owner synchronously
   undoes any egress/runtime registration, calls
   `ReservationRegistry::abort_graph_activation(id, generation,
   authority_digest)`, removes only its pointer-equal `Activating` entry, stores
   one cloneable closed `GraphActivationFailure`, and notifies all waiters. The
   exact pending/active resource envelope and permit drop once. Waiters return
   that same failure and never retry under the consumed reservation.

There is no cancellation point on the owner's path between entry insertion and
success/failure publication: registry transfer, runtime construction, egress
record, and supervisor registration are synchronous. Cancelling a waiter only
drops that wait; it cannot cancel or roll back the owner. Concurrent shutdown
first closes supervisor admission; an owner already elected either publishes a
fully registered graph or takes the failure path, while shutdown waits for the
latch before graph drain. Do not introduce a generic transaction or spawn an
activation task.

`AnalyticalStageEgress::record` uses `HashMap::entry`: the first verified
message installs immutable graph authority, participant cut, and absolute
deadline. Later messages compare them byte-for-byte and refuse before decode or
IO if any field differs. Attempt is mutable state separate from graph
authority. It starts at zero and advances zero-to-one only through
`advance_attempt_after_drain(graph, 0, 1)`, after the supervisor reports every
attempt-zero descendant idle. RPC arrival cannot advance or overwrite it.

SetPlan-first and ExecuteTask-first remain valid and converge on the same
activation. No registry, graph-map, egress, or supervisor lock is held while
runtime construction performs IO. The only cross-owner lock ordering is the
short publication sequence above; all component operations acquire their own
locks after the graph-map lock has been released.

**Falsification.** `graph_activation_is_rollback_safe_and_authority_is_immutable`
injects each fallible owner step, both first-message orders, many concurrent
exact duplicates, a mismatched concurrent caller, waiter cancellation, and
shutdown during activation. It proves one owner/build/register, waiters blocked
until publication, one shared lease/runtime identity, owner-only rollback, one
failure delivered to every waiter, no stranded `Activating`, and later
cut/deadline/attempt mutation refusal.

**Rejected alternatives.** Do not let “last RPC wins,” put attempt in immutable
graph identity, or rely on a connection's accidental lifetime. Do not add a
new authority service; the existing egress owner already owns this state.

### 3. Structured drain, retry hold, and terminal ownership

**Owner.** `AnalyticalStageIngress` coordinates graph lifecycle and the
existing `AnalyticalSupervisor` retains attempt drivers. Add one concrete
graph-local lifecycle record to the ingress map; do not add a framework or
trait. It owns open stage-call count, retry-hold state, cleanup cancellation,
and the one retained cleanup `JoinHandle` when asynchronous teardown is needed.

**Lifecycle.** The graph state moves:

```text
Activating -> Active(attempt 0) -> Draining(attempt 0)
                                -> RetryHeld -> Active(attempt 1)
                                -> TerminalDraining -> Released
```

- Each admitted coordinator/stage call increments the graph-local call count
  before the inner worker observes the body and decrements it in an explicit
  async settlement path after the inner call completes.
- Cancellation of the service future signals the lifecycle record. `Drop`
  performs only that synchronous signal; the ingress/supervisor-retained task
  owns and joins asynchronous cleanup.
- A retry hold is acquired before attempt zero is asked to drain. It prevents a
  zero-call/zero-attempt observation from releasing the graph. After all
  attempt-zero drivers, streams, task cache entries, exchange reservations, and
  spill operations are idle, slice 6 can either advance to attempt one or
  release the hold and terminate.
- Terminal release first proves all children idle, then removes runtime and
  egress authority, then releases the exact registry lease. It never removes
  the owner before drain.
- Drain timeout cancels the graph, records terminal failure/unhealthy state,
  and retains the lease and lifecycle record. Shutdown joins cleanup and
  reports retained owners; it may not `clear()` maps or release capacity merely
  to make gauges reach zero.
- Leader unused-reservation cleanup follows the same rule: the normal path
  awaits release; abandonment hands work to an already retained execution
  owner. No `Drop`-spawned future is allowed.

The cleanup join bound is the existing absolute query/shutdown deadline, not a
new independent lease timeout. Crash recovery is process termination: the OS
releases memory/files/sockets, and the restarted destination fence rejects old
tickets.

**Falsification.** `graph_cleanup_retains_owner_until_children_join` covers
success, stage error, caller cancellation, deadline, failed cleanup, shutdown,
and retry hold/advance. It asserts release once, joined handle counts, zero
gauge only after real idle, and retained owner on forced drain timeout.

**Rejected alternatives.** Do not use detached `tokio::spawn`, blocking async
work in `Drop`, a global cleanup queue, reference counts as proof of child
idleness, or fail-open timeout release. Each obscures the exact owner or allows
capacity to be returned while work remains.

### 4. Actual exchange-buffer accounting on the graph pool

**Dependency capability and closed provenance.** Prerequisite
`BIFROST-T1-C05-D0` pins one reviewed commit from
`https://github.com/bohmian-ai/datafusion-distributed`, descended from the
former upstream pin `4cfa166d233207ee188a277a7849bee7d9dfd2de`. The fork
registers
DataFusion `MemoryReservation`s named `WorkerConnection` in
`protocol/grpc/worker_client.rs` and `NetworkBoundary` in
`protocol/grpc/spawn_select_all.rs`, growing and shrinking them with the actual
queued FlightData/RecordBatch bytes. DataFusion 55 requires `MemoryPool::grow`
to be infallible, so enforcement must prove the maximum before dispatch rather
than react after an overrun.

The prerequisite corrects the missing seam in two distinct steps. It threads
the caller-supplied hard maximum into `FlightDataEncoderBuilder` as its
approximate chunk target, then applies a strict guard using the same
message-size calculation charged by
`NetworkBoundary` before any message can enter that queue. This second step is
required because Arrow's encoder target is approximate rather than a hard
ceiling. Matching bounded tonic server/client configuration fails closed at
the transport edge. The prerequisite's production-shaped test proves the
split path, unsplittable-message refusal before exchange charge, and immutable
pin. This task must verify and consume those facts; it must not recreate them.

No upstream approval or merge is required. An upstream contribution is
optional and non-blocking. A second fork, branch/tag pin, `[patch]`, or vendored
copy is not an implementation option.

The dependency provides the needed finite bounds:

- The reviewed dependency change makes `Worker::with_max_message_size` a hard
  pre-queue bound, using the same value as Arrow's approximate encoder target.
  Configure the Wyrd Analytical worker, bounded worker client, and both tonic
  directions with `M = 256 * 1024`.
- Set `DistributedConfig::worker_connection_buffer_budget_bytes` to
  `C = 256 * 1024`. Its documented maximum retained by each
  `WorkerConnection` is `C + M`, because it gates before pulling and admits one
  final message.
- The pinned worker service passes `RECORD_BATCH_BUFFER_SIZE = Q = 2` to
  `spawn_select_all`. That function creates one queue per input stream and
  grows before awaiting `send`, so each stream retains at most `(Q + 1) * M`:
  two queued messages plus the producer's one blocked send.

**Pre-dispatch shape and checked formula.** The existing Analytical planner,
while it owns the finalized static distributed `Stage` graph, computes one
`GraphExchangeShape` per selected follower:

- `W`: the sum of remote input-task slots in every `WorkerConnectionPool` that
  can be initialized concurrently by consumer tasks assigned to that follower;
- `S`: the sum of `target_partition_end - target_partition_start` over every
  concurrent ExecuteTask RPC the follower can serve, which is exactly the
  number of `spawn_select_all` input streams on that follower.

Dynamic task count remains disabled, and `max_tasks_per_stage` is set to the
already admitted static task ceiling, so execution cannot add fan-out after
this count. Compute with `checked_add`/`checked_mul` only:

```text
connection_peak = W * (C + M)
boundary_peak   = S * (Q + 1) * M
exchange_ceiling E = connection_peak + boundary_peak
operator_limit O   = query_grant G - E
```

Any overflow, `E > analytical_exchange_buffer_bytes`, `E > G`, or `O` below
the existing required working-memory floor for all admitted partitions rejects
the Analytical plan before follower reservation/dispatch. The exact `W`, `S`,
`E`, and the static task ceiling enter each participant's
`GraphReservationAuthority` and its digest. The follower recomputes the same
formula against its locally admitted `G` and refuses the reservation without
mutation on disagreement. This calculation is why both pinned infallible
exchange `grow` callers are safe: every possible connection, queue slot, and
one blocked send already has one `M` slot inside `E`.

**Single pool partition and actual accounting.** Add one concrete
`AnalyticalGraphMemoryPool` in `oracle/analytical_supervisor.rs` around the
exact graph's admitted DataFusion pool. It is the only pool installed in the
graph runtime. Its required `MemoryPool` implementation delegates to the one
underlying query pool and maintains one mutex-protected partition:

- consumers named exactly `WorkerConnection` or `NetworkBoundary` charge
  actual current/peak exchange bytes and may use at most `E`;
- every other consumer is operator memory; `try_grow` refuses when actual
  operator ownership would exceed `O`;
- a successful grow updates the matching counter once, shrink/unregister
  releases the same actual bytes once, and the underlying pool's total remains
  `operator_current + exchange_current <= G`;
- exchange `grow` remains infallible because the checked shape proves it cannot
  cross `E`; it is not implemented as cancel-after-overrun or borrowing from
  unused operator memory.

Remove the supervisor's fixed
`try_split_memory(EXCHANGE_CONSUMER, configured)` reservation. The configured
value is the maximum acceptable `E`, not a precharged estimate. Release
requires actual operator and exchange current bytes to be zero; their bounded
peaks remain evidence.

**Falsification.** The dependency's `bounded_message_size` integration target
first proves the public Worker/client seam and the finite `M` used by Wyrd.
`analytical_worker_message_bound_matches_exchange_ceiling` constructs Wyrd's
actual Analytical Worker and client configuration and proves the 256-KiB hard
bound used by the shape calculation is installed at both ends.
`graph_exchange_account_tracks_actual_owned_bytes` then drives
checked overflow and insufficient-grant refusals; both dependency consumer
names; exact `W * (C + M)` and `S * (Q + 1) * M` boundaries; two queued plus one
blocked NetworkBoundary message; one over-budget WorkerConnection message;
non-exchange operator `try_grow`; shrink/drop; and cancellation. It asserts the
partition sum equals the underlying graph pool, actual exchange never exceeds
`E`, operator memory never borrows `E`, and nothing is double charged.

**Rejected alternatives.** Do not cancel after an overrun, wrap every Arrow
message, infer ownership from `RecordBatch::get_array_memory_size`, reserve the
entire ceiling per attempt, allow operator/exchange borrowing, parse unbounded
consumer labels, maintain a Wyrd fork or vendored patch, pin a branch/tag, or add
a generalized transport-budget API. Keeping the current upstream revision is
also rejected because its unlimited encoder makes `M` false. These alternatives
either violate infallible `grow`, duplicate the engine's accounting, weaken
provenance, or retain the current estimate/observation defect.

### 5. Process-journey evidence

Extend the existing
`peer_network::analytical::graph_lease_owns_exact_resources_for_complete_graph`
rather than creating overlapping acceptance tests. Add only narrowly scoped
child-process controls needed to observe finalized-plan/reserve/first-dispatch
ordering, race exact first messages, mutate one authority field, inject
activation/drain failure, hold/advance an attempt, exercise bounded exchange,
and inspect graph ownership. Inspection returns IDs/digests only in
test-support builds and counters/gauges in production vocabulary; it must not
return SQL, plans, credentials, or unbounded labels.

The journey remains three Oracles plus one Scribe and proves:

- graph shape/authority are finalized before reservation, the first stage
  dispatch follows reservation without intervening planning/backoff, activation
  succeeds inside the unchanged two-second pending window, and an actually
  expired reservation is refused;
- wrong purpose, graph, principal tenant/space, destination/fence, authority
  digest, generation, or expiry is refused before decode/cache/provider/IO and
  the original exact reservation remains usable;
- SetPlan-first and ExecuteTask-first, multiple channels/tasks/partitions, and
  duplicate messages all reuse one lease and one exact resource identity;
- concurrent exact first messages elect one activation owner, wait until the
  owner publishes the shared lease/runtime, and perform one runtime build and
  supervisor registration; owner failure wakes every waiter with one result,
  while waiter cancellation and a mismatched caller cannot roll back the owner;
- injected runtime and supervisor-registration failures leave no pending or
  active lease and return the full envelope;
- a controlled attempt-zero drain plus attempt-one advance retains the same
  lease/generation/resource envelope and never overlaps attempts (without
  implementing automatic retry policy);
- actual exchange current bytes rise and return to zero, peak bytes are
  nonzero and within the checked `W`/`S` graph ceiling, operator current stays
  within `G - E`, the pool sum stays within `G`, and no separate fixed attempt
  charge appears;
- success, early task-before-plan timeout, cancellation, absolute deadline,
  and shutdown join every child before release; a forced drain timeout reports
  terminal failure and a retained owner rather than a false zero;
- final pending/running slots, memory, exchange, scratch, runtime, task, stream,
  cache, attempt, connection, and graph gauges are zero on every normally
  drained terminal path.

## Ordered RED -> GREEN -> REFACTOR scenarios

Implement one scenario at a time; do not batch-create all failing tests.

1. **RED:** add `graph_reservation_refusal_preserves_exact_owner`; demonstrate
   early reservation, two-second expiry, destructive purpose mismatch,
   incomplete tuple, and stale release. **GREEN:** move reservation to the
   finalized-plan dispatch edge, add closed purpose/generation, preserve the
   existing TTL clamp, and validate before removal. **REFACTOR:** centralize
   exact comparison on the concrete authority value without adding a trait.
2. **RED:** add
   `graph_activation_is_rollback_safe_and_authority_is_immutable`; demonstrate
   resource leakage, duplicate construction, stranded waiters, and
   last-RPC-wins authority. **GREEN:** add the graph-keyed owner/waiter latch,
   owner-only rollback, publish-before-wake, and first-write/exact-compare
   authority with explicit post-drain attempt advancement. **REFACTOR:** keep
   fallible orchestration as inherent methods on ingress/egress owners.
3. **RED:** add `graph_cleanup_retains_owner_until_children_join`; demonstrate
   detached drop cleanup, pre-drain removal, fail-open timeout, and retry race.
   **GREEN:** introduce the graph-local lifecycle record, retained cleanup join,
   retry hold, and retain-on-timeout behavior. **REFACTOR:** remove obsolete
   leases/drop fallbacks and make the lock order explicit in module prose.
4. **EXTERNAL PREREQUISITE:** upstream and merge the bounded-message dependency
   change exactly as specified above. Record its PR, independent approval, CI,
   and merged 40-character commit; update the workspace pin and lockfile only
   after those facts exist. **RED:** add
   `analytical_worker_message_bound_matches_exchange_ceiling` and
   `graph_exchange_account_tracks_actual_owned_bytes`; demonstrate fixed
   estimate diverging from actual queued bytes and infallible grow having no
   prechecked partition. **GREEN:** configure the reviewed bounded Worker and
   client seam, compute/reject the checked `W`/`S` ceiling before reservation,
   install the single partitioned graph pool, and use the pinned dependency
   reservations. **REFACTOR:** keep one actual accounting source and bounded
   inspection.
5. **RED:** extend the named process journey, first with dispatch ordering,
   refusal, concurrent activation, and exact-identity cases, then
   lifecycle/retry/accounting cases as their seams land. **GREEN:** add the
   minimum test-support controls and drive all cases over real peer transport.
   **REFACTOR:** remove the misleading “second attempt” comment and any
   unit-only probe duplicated by the journey.

## Exact focused commands

In the upstream `datafusion-distributed` checkout, the PR must pass its exact
production-shaped target and the dependency's one-cone checks:

```bash
cargo test --features integration --test bounded_message_size \
  worker_message_size_bounds_production_flight_stream -- --exact
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --features integration
```

After upstream merge, derive and verify the immutable revision in that checkout
before editing Wyrd; `DFD_MERGED_REV` is an execution variable, not a revision
placeholder written to either manifest:

```bash
DFD_MERGED_REV="$(git rev-parse HEAD)"
test "$(git rev-parse --is-inside-work-tree)" = true
test "$(git remote get-url origin)" = \
  https://github.com/datafusion-contrib/datafusion-distributed
test "${#DFD_MERGED_REV}" = 40
git branch -r --contains "$DFD_MERGED_REV" | rg -F 'origin/'
```

Set root `Cargo.toml` to that exact value, then update and verify the one
dependency cone through Wyrd's toolchain:

```bash
mise exec -- cargo update -p datafusion-distributed --precise "$DFD_MERGED_REV"
mise exec -- cargo tree --locked -p vala-bifrost-redux \
  -i datafusion-distributed
mise exec -- cargo check --locked -p vala-bifrost-redux \
  --features test-support,bench-support
```

Run each named Rust test when its scenario becomes green:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  --features test-support,bench-support \
  -E 'test(=oracle::dispatcher::tests::graph_reservation_refusal_preserves_exact_owner)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  --features test-support,bench-support \
  -E 'test(=oracle::analytical::tests::graph_activation_is_rollback_safe_and_authority_is_immutable)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  --features test-support,bench-support \
  -E 'test(=oracle::analytical::tests::graph_cleanup_retains_owner_until_children_join)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  --features test-support,bench-support \
  -E 'test(=oracle::analytical_transport::tests::analytical_worker_message_bound_matches_exchange_ceiling)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  --features test-support,bench-support \
  -E 'test(=oracle::analytical_supervisor::tests::graph_exchange_account_tracks_actual_owned_bytes)'

scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
   -p wyrd-testing --test oracle -P journey --run-ignored=all \
   -E "test(=peer_network::analytical::graph_lease_owns_exact_resources_for_complete_graph)"'
```

The Postgres wrapper is the repository-managed environment owner used by the
canonical journey lane. The selector is exact; do not replace it with a
positional substring that can select zero tests.

## Broader verification

After the focused scenarios are green, run:

```bash
mise run fmt
mise run lints
mise run test:bifrost
mise run test:tonic
mise run test:bifrost:journey:oracle
mise run check:bifrost-resource-governance
mise run check:client-tier
mise run check:proto-drift
mise run codegen:check
git diff --check
```

Do not require `mise run gate` for this bounded successor. The remaining
remediation slices and final integrated review own their broader aggregate
evidence.

## Completion evidence

The implementation report must include:

- RED and GREEN output for all five named tests/commands;
- a mapping from S5-RF-01 through S5-RF-09 to commits and test evidence;
- one before/after snapshot for the exact follower lease identity and resource
  envelope, with sensitive values represented only by bounded digests;
- proof that refused mutations left the original reservation usable;
- proof that every injected owner failure returned slot/memory/scratch
  ownership, woke exact waiters once, and left no `Activating` entry;
- proof that final graph/authority/shape precede reservation and first dispatch
  immediately follows it inside the unchanged two-second pending window;
- proof that actual exchange current bytes returned to zero and peak stayed
  within checked `E`, operator ownership stayed within `G - E`, and the pool
  sum stayed within `G`;
- proof that retry attempt one reused the same lease/generation only after
  attempt zero drained;
- proof that cleanup timeout retained ownership and surfaced terminal state;
- broader verification output and `git diff --check`;
- an updated slice ledger marking slice 5 green only after all evidence passes.

## Stop conditions

Stop and return `SPEC_REVISION_REQUIRED` if closure would require active lease
recovery across follower process restart, a third retry attempt, changing the
30-second stage-ticket acceptance window, or making StageGraph production
reachable. Those change approved behavior rather than repair this slice.

Ordinary type placement, helper naming, and local error plumbing are
implementation choices. Authority fields, state transitions, lock/cleanup
ordering, retry hold semantics, actual exchange accounting, and journey
topology are closed by this task.

## Authority

- [`AGENTS.md`](../../../../AGENTS.md), especially struct-centered ownership,
  async lifecycle, test taxonomy, and exact focused commands;
- [`architecture/wyrd-design.md`](../../../../architecture/wyrd-design.md) and
  [`architecture/wyrd-doctrine.mdx`](../../../../architecture/wyrd-doctrine.mdx);
- [`architecture/bifrost-design.md`](../../../../architecture/bifrost-design.md),
  especially distributed graph authority, GraphLease resources, retry, actual
  exchange ownership, and terminal drain;
- [`architecture/wyrd-security-posture.md`](../../../../architecture/wyrd-security-posture.md);
- [`architecture/references/domain/datafusion.md`](../../../../architecture/references/domain/datafusion.md),
  [`olap-serving.md`](../../../../architecture/references/domain/olap-serving.md),
  and
  [`analytical-operations-reliability.md`](../../../../architecture/references/domain/analytical-operations-reliability.md);
- [`architecture/references/languages/implementation-execution.md`](../../../../architecture/references/languages/implementation-execution.md)
  and
  [`testing-workflows.md`](../../../../architecture/references/languages/testing-workflows.md);
- original task
  [`00-d1-inactive-distributed-execution-engine.md`](00-d1-inactive-distributed-execution-engine.md)
  and approved remediation
  [`01-t1-inactive-distributed-execution-review-remediation.md`](../remediation/01-t1-inactive-distributed-execution-review-remediation.md).
