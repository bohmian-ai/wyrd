---
id: BIFROST-R4-T01-GRAPH-LEASE
title: Activate one exact rollback-safe GraphLease on each follower
kind: implementation
mode: RECONCILE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 4
depends_on: [BIFROST-R3-T00-AUTHORITY]
requirements: [REQ-004, REQ-005, REQ-007, REQ-009, REQ-011]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-007, INV-008]
acceptance: [AC-001, AC-004, AC-005, AC-007, AC-008]
parent_task: BIFROST-R3-T1-INACTIVE-CLOSEOUT
frozen_candidate: f1ac4cb01fe9ddda0a133cb58c955bab1e1cf7df
---

# Follower GraphLease activation

## Outcome and value

Each selected follower converts one pending reservation into one exact graph
owner exactly once, regardless of whether `SetPlan` or `ExecuteTask` arrives
first or concurrently. Complete authority is validated before decode/cache/
provider/source IO; activation publishes only after runtime and supervisor
registration succeed; failures restore the still-valid pending reservation or
release it. Users gain a distributed graph that cannot double-charge, widen its
authority, or disappear between owners.

Required execution skill: `$wyrd-implement`.

## Current-state amendment

- Retain the private mTLS listener, workload authentication, purpose tickets,
  immutable participant cut, `ReservationRegistry`, `OracleResources`, typed
  graph identities, two-second pending TTL, and the existing process-cluster
  graph-activation evidence.
- Replace `ReservationRegistry::lease_graph`, which destructively removes the
  pending entry before fallible registration, and remove
  `GraphLease::take_resources`, which permits an empty published shell.
- Replace the separately published registry lease and supervisor guard with one
  activation transaction owned by `AnalyticalStageIngress`.
- Replace `AnalyticalConnectionLease::drop`'s detached cleanup spawn with one
  bounded settlement driver owned and joined by `AnalyticalStageIngress`.
- Preserve the reviewed candidate and remediation ledger as evidence only; this
  task fully states the successor behavior.

## Owners, scope, consumers, and non-goals

Primary owners:

- `oracle/peer.rs`: the existing verified `StageTicketClaims` stage authority.
- `oracle/dispatcher.rs`: pending reservation and rollback-capable activation
  guard.
- `oracle/analytical.rs`: `AnalyticalStageIngress` activation state and stage
  consumer handoff plus its bounded graph-settlement driver.
- `oracle/analytical_supervisor.rs`: graph registration guard retained by the
  lease.

Direct consumers are both `SetPlan` and `ExecuteTask` ingress adapters, the
stage codec/provider path, Task 2's leader reservation owner, Task 3's physical
journey, readiness, and shutdown inspection.

Do not change public query contracts, route production traffic, introduce a
second registry/supervisor, add retry, or redesign the private peer plane.

## Ordered implementation scenarios

### Scenario 1 — Retain complete verified stage authority in the graph lease

**Behavior.** Before any plan or source IO, one immutable lease binding is the
exact union of:

- the reservation ID, graph and query IDs, original reserving leader/fence,
  original reservation expiry, and reserved resources retained from
  `PendingReservation`; and
- the authenticated tenant, public and DataFusion query IDs, destination
  node/fence, canonical participant-cut fingerprint, snapshot digest,
  permission digest, and absolute deadline captured from the first verified
  stage ticket.

The ticket's `source_node_id` and `source_fence` remain per-message coordinator
authority. The exact pair is valid when it equals the original reserving
leader node/fence or names an exact node/fence in the immutable destination
participant cut. The leader remains absent from that destination cut; the two
authorization branches are distinct and neither widens the other. Operation,
stage/task, attempt, body digest, nonce, and ticket expiry also remain
per-message fields. Maps REQ-004, REQ-009, INV-001, INV-002, AC-001, AC-005.

**RED.** Add
`oracle::analytical::tests::graph_lease_binding_mutation_is_refused_before_io` as a
pure owner test. Independently substitute every reservation field and mutate
every immutable first-ticket field after signing; assert the original resource
envelope, pool, scratch, and expiry are never replaced. Activate the lease with
a first ticket from the original reserving leader and assert it is accepted
even though that leader is absent from the destination cut. Send a later ticket
from a different valid middle-stage participant node/fence and assert it is
accepted, then send one whose source/fence matches neither the original leader
pair nor an exact cut participant and assert refusal. In every refusal,
decode/cache/provider/source probes remain zero. It initially fails because
`GraphLeaseRequest` currently compares only reservation, graph, and query and
does not retain the complete first-ticket graph subset or this two-branch
source authority. Exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::graph_lease_binding_mutation_is_refused_before_io)'
```

**GREEN.** Keep the reservation half in `PendingReservation` and move it
unchanged into the rollback-owning `PendingGraphActivation`; do not overwrite
its original expiry, reserving leader/fence, or resources. In
`AnalyticalStageIngress`, derive one private `GraphLeaseBinding` from that
reservation half plus the first successfully verified `StageTicketClaims`.
Store the binding in the committed `GraphLease`. Do not add a wire field: the
current reservation and ticket claims already carry the selected union.

For every later `SetPlan` or `ExecuteTask`, validate the per-message ticket
normally, then have `GraphLeaseBinding` authorize the exact
`(source_node_id, source_fence)` pair with one closed predicate:
`pair == original_reserving_leader_pair || participant_cut.contains(pair)`.
Do not insert the leader into `AnalyticalParticipantCut` or derive leader
authority from cut membership. After source authorization, compare only the
immutable graph subset with `GraphLeaseBinding`. Perform all checks before
`AnalyticalCodec` decode, cache lookup, runtime construction, provider
resolution, or source IO. A changed immutable field, a source/fence in neither
authorization branch, or a per-message ticket failure refuses the message
without mutating, widening, or releasing the live lease.

**REFACTOR.** `StageTicketClaims` remains the sole signed stage-authority
inventory. `GraphLeaseBinding` is only the typed retained projection described
above. Reuse the participant cut's existing canonical encoding to compute its
fingerprint; do not add a second cut ordering, graph digest, parallel signed
field list, or public namespace field absent from `AuthorizedQueryContext`.

### Scenario 2 — Order-independent exactly-once activation

**Behavior.** The first equivalent `SetPlan` or `ExecuteTask` activates; an
equivalent concurrent/later message waits for and reuses the same published
lease; a mismatch fails without disturbing it. Maps REQ-004, INV-002, INV-004,
AC-001.

**RED.** Add
`oracle::analytical::tests::graph_lease_activation_is_order_independent_and_shared`.
Use deterministic barriers to run both orderings and a concurrent duplicate;
assert one runtime build, one supervisor registration, one envelope charge,
identical `Arc<GraphLease>`, and immediate mismatch refusal. It fails against
the current absent/live map and destructive registry transfer. Exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::graph_lease_activation_is_order_independent_and_shared)'
```

**GREEN.** Reuse `AnalyticalStageIngress`'s existing narrow graph mutex as the
single activation serialization point. The first caller retains a private
rollback-owning activation guard while it builds and registers the runtime,
then publishes one `Arc<GraphLease>`. An equivalent duplicate entering after
the mutex is released reuses that published lease; a mismatched binding fails
under the same mutex without changing it. `GraphLease` owns the pending
activation guard, query resources, runtime, cancellation/deadline, and
supervisor graph guard; it never exposes a take-once resource hole. Keep
activation synchronous under this existing serialization seam; do not add a
five-state activation protocol or shared completion primitive.

**REFACTOR.** Keep state transitions inherent on `AnalyticalStageIngress` and
`GraphLease`; no free-function orchestration or lock held across await/IO.

### Scenario 3 — Publish-after-success and rollback

**Behavior.** Runtime creation, spill/runtime installation, and supervisor
registration all succeed before publication. Any injected failure returns to
that activator and restores the pending reservation only while its original TTL
remains valid; otherwise it releases the permit/resources. A serialized waiter
then re-evaluates the restored reservation under the unchanged original TTL and
may activate it if still valid. Maps REQ-004, REQ-007, INV-003, INV-004,
AC-001, AC-004.

**RED.** Add
`oracle::analytical::tests::graph_activation_failure_rolls_back_without_publication`.
Inject failure at runtime build and supervisor registration on both sides of
the TTL boundary; assert no active map entry, no provider/source IO, one pending
entry before expiry, and zero ownership after expiry. Gate a serialized waiter
behind the failed activator; before expiry assert it re-evaluates and can
activate the restored reservation, while after expiry it observes refusal and
cannot recreate or extend the reservation. Exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::graph_activation_failure_rolls_back_without_publication)'
```

**GREEN.** Replace `lease_graph` with `begin_graph_activation`, returning a
private `PendingGraphActivation` that retains the complete pending entry and
offers only `commit(supervisor_guard, runtime)` or `rollback(now)`. Commit
moves ownership directly into the lease and then publishes `Active`. Rollback
reinserts the unchanged entry if `expires_at > now` and the reservation slot is
still vacant; otherwise drops the exact permit/resources. A cancelled activator
executes the same rollback through an owned guard, not detached work. Mutex
waiters receive no published completion result: after acquiring the same graph
mutex they perform the normal reservation lookup and authority checks against
the restored entry and its original expiry.

**REFACTOR.** Centralize reinsertion collision/expiry handling in the registry;
the ingress owns orchestration but never edits reservation internals.

### Scenario 4 — Follower drain and shutdown visibility

**Behavior.** A follower lease is not released while coordinator connection
leases, stage drivers, tasks, cache entries, exchange streams, runtime, or spill
cleanup remain live. Coordinator disconnect/caller drop decrements
`AnalyticalStageIngress::connections`; the zero transition initiates drain.
Cleanup failure leaves `Draining`, fails readiness, and is joined/visible at
shutdown. Maps REQ-007, REQ-009, REQ-011, INV-003, INV-004, INV-005, AC-001,
AC-004, AC-005, AC-007.

**RED.** Add
`oracle::analytical::tests::follower_graph_release_waits_for_children_and_retains_cleanup_failure`.
Use deterministic connection and child gates plus injected cleanup timeout;
assert each coordinator disconnect decrements the connection map, only zero
starts drain, the permit, pool, scratch, graph map, and active gauge remain
owned until every child joins, and the failed case remains draining/readiness-
failing. Close the settlement receiver and separately fill its bounded queue;
in both cases assert the zero-connection signal failure retains the lease,
marks readiness failed, and remains visible when shutdown drains and joins the
driver. Exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::follower_graph_release_waits_for_children_and_retains_cleanup_failure)'
```

**GREEN.** Add one `AnalyticalStageIngress`-owned lifecycle driver at ingress
construction: one bounded Tokio MPSC sender/receiver and one owned join handle.
Size the channel to the same checked maximum concurrent graph capacity that
`ReservationRegistry` enforces after clamping `ANALYTICAL_GRAPH_SLOT_UNITS` to
its running capacity; add no public setting or second capacity root. Replace
the separate graph-guard and connection-count map entries with one entry under
the existing ingress graph mutex:
`Active { lease, open_connections }` or
`Draining { lease, settlement_failure }`. Activation publishes `Active`.
Normal terminal, cancellation, failure, shutdown, and the transition from one
connection to zero compete under that mutex to change `Active` to `Draining`
exactly once; later paths observe `Draining` and neither settle nor enqueue it
again.

The driver is the only asynchronous caller-drop owner. It invokes the same
explicit async `GraphLease::settle(outcome)` used by normal terminal,
cancellation, and failure paths. Its bounded message carries the graph key,
`Arc<GraphLease>`, and cancellation outcome. The driver retains only a
`Weak<AnalyticalStageIngress>` for the post-settlement map transition so the
ingress-owned join handle does not form an `Arc` cycle. Settlement closes stage
admission and coordinator connections, cancels on non-success, joins supervisor
children, cache entries, and exchanges, verifies the pool idle, removes spill
files, then releases the supervisor guard and reservation entry. Successful
settlement removes the matching `Draining` entry. Timeout/error stores the
closed production cleanup outcome in that entry and retains its lease.

`AnalyticalStageIngress::shutdown` first closes new stage/connection admission,
transitions every remaining `Active` graph to `Draining`, closes the settlement
sender, drains and joins the one driver, and only then inspects retained
`Draining` graphs for readiness/shutdown evidence. A full or closed signal
queue records cleanup failure on the graph, retains every owner, and fails
readiness; it never falls back to a spawn, synchronous cleanup, release, or a
successful terminal. `AnalyticalConnectionLease::drop` performs only the
synchronous decrement/`Active`-to-`Draining` transition and non-blocking
`try_send`. It does not spawn, block, settle, or release.

**REFACTOR.** One settlement order and one ingress-owned driver serve caller
drop, normal terminal, and shutdown paths. Readiness reads the owning
ingress/supervisor state rather than a test-only registry; no detached task may
outlive the ingress join boundary.

## Cross-scenario decisions and invariants

The activation transaction and bounded settlement queue are process-local and
in-memory; they change no durable or wire schema. The two-second TTL is measured
from follower reservation acceptance and never extended. The original
reservation leader/fence establishes reservation ownership; a verified
per-message coordinator source/fence establishes current stage authority only
when it is either that exact original leader pair or an exact member of the
immutable destination participant cut. The leader is not added to the cut, and
cut membership does not replace reservation-leader authority. Transport
authentication and purpose tickets remain preconditions. Graph identity and
authority never become metric labels.

Authority: `architecture/bifrost-design.md`,
`architecture/wyrd-security-posture.md`,
`architecture/operations/reliability-and-recovery.md`,
`architecture/references/architecture/patterns.md`,
`architecture/references/domain/datafusion.md`,
`architecture/references/domain/analytical-operations-reliability.md`, and
`architecture/references/languages/rust-core.md`.

## Broader verification

```bash
mise run fmt
mise run lints
mise run test:bifrost
mise run test:bifrost:journey:oracle
mise run check:bifrost-resource-governance
mise run check:proto-drift
mise run codegen:check
git diff --check
```

## Completion evidence

- Mutation-sensitive retained-binding test against the exact reservation plus
  first-ticket union, including acceptance of the original leader outside the
  destination cut, acceptance of a valid middle-stage participant, and
  pre-IO refusal of a source/fence in neither authorization branch.
- Both stage orderings, a true concurrent duplicate, one activation count, and
  identical reused lease identity.
- Runtime/supervisor failure proof on both sides of the unchanged TTL.
- Connection-map transitions, exactly-once driver signaling, bounded
  queue/receiver failure, joined child/resource snapshots, and readiness-visible
  retained `Draining` ownership.
- Evidence that no private wire or generated contract changed.

## Stop conditions

Return `SPEC_REVISION_REQUIRED` if exact activation requires a public field,
weaker tenant/peer authority, a longer TTL, a second registry/supervisor, or
best-effort cleanup. Return `PLAN_BLOCKED` if the pinned dependency prevents
the existing stage ingress from awaiting an activation before decode without a
fork; current adapters provide this interception point, so no block is known.

## Execution evidence

### Scenario 1 — retained binding, pre-IO refusal

RED: `graph_lease_binding_mutation_is_refused_before_io` failed to compile —
no `GraphLeaseBinding` owner existed on `AnalyticalStageIngress`.

GREEN: added `GraphLeaseBinding` in `oracle/analytical.rs` holding the
reservation half (`reservation_id`, `graph`, `query_id`,
`reserving_leader_node_id`, `reserving_leader_fence`, `reservation_expires_at`)
unioned with the first verified `StageTicketClaims` half (`tenant_id`,
`public_query_id`, `datafusion_query_id`, `destination_node_id`,
`destination_fence`, `participant_cut_fingerprint`, `participant_cut`,
`snapshot_digest`, `permission_digest`, `absolute_deadline_ms`). Per-message
source authority is `pair == original_reserving_leader_pair ||
participant_cut.contains(pair)`, evaluated in `authorize` before any decode.
`AnalyticalParticipantCut` gained `contains` and a `fingerprint` over
`b"wyrd-oracle-analytical-participant-cut-v1"` plus canonically ordered
participants.

Command: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib
--features test-support,bench-support -E
'test(=oracle::analytical::tests::graph_lease_binding_mutation_is_refused_before_io)'`
— PASS.

### Scenario 2 — exactly-once activation, order independent

RED: `graph_lease_activation_is_order_independent_and_shared` failed — the old
`lease_graph`/`release_graph` pair on `ReservationRegistry` admitted a second
activation for the same graph.

GREEN: replaced it with a two-phase activation transaction in
`oracle/dispatcher.rs`: `begin_graph_activation` → `PendingGraphActivation`
(`commit(register)` / `rollback(now)`) → `CommittedGraphActivation`, which owns
the running permit and releases it on drop. `AnalyticalStageIngress::publish`
installs exactly one `Arc<GraphLease>` per `AnalyticalGraphKey` under the
existing graph mutex; the loser of a race reuses the published lease and its
pending activation rolls back. No second registry or supervisor was added.

Command: as above with
`test(=oracle::analytical::tests::graph_lease_activation_is_order_independent_and_shared)`
— PASS.

### Scenario 3 — failed activation rolls back without publication

RED: `graph_activation_failure_rolls_back_without_publication` failed — a
supervisor registration failure left the reservation consumed.

GREEN: `AnalyticalGraphSupervisor::register_graph` now returns
`Result<AnalyticalGraphGuard, (Box<OracleQueryResources>, BifrostError)>` so the
resources return to the caller, and `PendingGraphActivation::commit` restores
the reservation entry on the register closure's failure. The helper
`graph_rollback_restores_a_reservation_only_before_its_expiry` proves the
restore is refused past the unchanged TTL rather than extending it.

Command: as above with
`test(=oracle::analytical::tests::graph_activation_failure_rolls_back_without_publication)`
— PASS.

### Scenario 4 — settlement driver and retained draining ownership

RED: `follower_graph_release_waits_for_children_and_retains_cleanup_failure`
failed — release force-dropped the supervisor entry beneath live nested
children, so nothing was retained.

GREEN: added `enum AnalyticalGraphEntry { Active { lease, open_connections },
Draining { lease, settlement_failure } }` under the existing graph mutex, one
ingress-owned bounded MPSC settlement driver sized to
`ReservationRegistry::max_concurrent_graphs()`, and `drive_graph_settlements`
holding only `Weak<AnalyticalStageIngress>`. `AnalyticalConnectionLease::drop`
does only the synchronous decrement plus a non-blocking `try_send`.
`AnalyticalGraphSupervisor::release_graph` now refuses to remove an entry whose
nested children are not idle, and `graph_children_debt` (backed by
`OracleQueryResources::nested_debt`) reports the exact retained scratch and
memory bytes. `shutdown` closes admission, transitions `Active` → `Draining`,
closes the sender, drains and joins the driver, and only then inspects retained
`Draining` entries; `AnalyticalLiveOwnership` gained `cleanup_failures` so the
retention is readiness-visible.

Command: as above with
`test(=oracle::analytical::tests::follower_graph_release_waits_for_children_and_retains_cleanup_failure)`
— PASS.

### Broader verification

- `mise run fmt` — PASS.
- `mise run lints` — PASS (required boxing four `result_large_err` payloads,
  splitting two `too_many_lines` bodies, and introducing
  `AnalyticalExecutionOwners` for a pre-existing `too_many_arguments`).
- `mise run test:bifrost` — 961/961 PASS.
- `mise run check:bifrost-resource-governance` — PASS.
- `mise run check:proto-drift` — PASS.
- `mise run codegen:check` — PASS (no private wire or generated contract
  changed).
- `git diff --check` — clean.
- `mise run test:bifrost:journey:oracle` — 14/15 PASS. See the limitation
  below.

### Material limitation — blocks Scenario 4's required join

`analytical_inactive::pg_inactive_analytical_raw_sql_executes_join_and_partial_final_aggregate_on_followers`
fails at server shutdown with `Oracle shutdown retained admission, resource,
peer, or audit state`, because the follower graph's envelope permanently
retains nested memory after every attempt has settled:

```
WARN Oracle analytical graph did not drain before its follower release
     scratch_bytes=0 memory_bytes=615
WARN Oracle analytical follower could not settle a closed graph
     error=... Oracle analytical graph cleanup did not complete
WARN Oracle analytical follower shutdown retains unsettled graph ownership
     graphs=1 attempts=0 cleanup_failures=1
ERROR Bifrost resource accounting poisoned
      detail="Oracle query owner outlived a nested resource child"
```

A `TrackConsumersPool` probe named the holders exactly — four
`WorkerConnection#NNN` consumers at 205 B each. These are the pinned
`datafusion-distributed` rev's `WorkerConnectionPool` reservations, registered
against the graph's query memory pool at
`src/protocol/grpc/worker_client.rs:102`, owned by
`NetworkShuffleExec`/`NetworkCoalesceExec` plan nodes that the process-wide
`Worker` retains inside `task_data_entries` — a `moka` cache with a 10-minute
`time_to_idle`, `pub(crate)`, with no eviction API in the pinned rev. Raising
the drain budget from 5 s to 40 s changed nothing (exactly 615 B throughout).

Before this task, `drain_graph` warned and released anyway, masking the leak;
the task's required join surfaces it as a shutdown failure. Settlement's
required "joins ... cache entries ... verifies the pool idle" step therefore
cannot complete against a process-wide `Worker`, and the gap is an ownership
decision the task does not settle. See the returned
`TASK_REVISION_REQUIRED` for the two reachable designs.
