---
id: BIFROST-R3-T01-GRAPH-LEASE
title: Activate one exact rollback-safe GraphLease on each follower
kind: implementation
mode: RECONCILE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 3
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
- Preserve the reviewed candidate and remediation ledger as evidence only; this
  task fully states the successor behavior.

## Owners, scope, consumers, and non-goals

Primary owners:

- `oracle/peer.rs`: one Rust-native `GraphAuthority` and canonical digest.
- `oracle/dispatcher.rs`: pending reservation and rollback-capable activation
  guard.
- `oracle/analytical.rs`: `AnalyticalStageIngress` activation state and stage
  consumer handoff.
- `oracle/analytical_supervisor.rs`: graph registration guard retained by the
  lease.
- Existing private proto/spec conversions only for authority fields genuinely
  missing from the current reservation/ticket tuple.

Direct consumers are both `SetPlan` and `ExecuteTask` ingress adapters, the
stage codec/provider path, Task 2's leader reservation owner, Task 3's physical
journey, readiness, and shutdown inspection.

Do not change public query contracts, route production traffic, introduce a
second registry/supervisor, add retry, or redesign the private peer plane.

## Ordered implementation scenarios

### Scenario 1 — Complete immutable graph authority

**Behavior.** Reservation and stage authority bind tenant, public and
DataFusion query IDs, graph, leader node/fence, destination node/fence,
participant-cut digest, snapshot digest, permission digest, absolute deadline,
and reservation ID before any plan or source IO. Maps REQ-004, REQ-009,
INV-001, INV-002, AC-001, AC-005.

**RED.** Add
`oracle::analytical::tests::graph_authority_mutation_is_refused_before_io` as a
pure owner test. It mutates each field independently after signing and uses
decode/cache/provider/source probes that must remain zero. It initially fails
because `GraphLeaseRequest` currently compares only reservation, graph, and
query. Exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::graph_authority_mutation_is_refused_before_io)'
```

**GREEN.** Add `GraphAuthority` beside existing peer authority types. Canonical
SHA-256 uses a versioned length-prefixed field sequence under
`wyrd.oracle.graph.authority.v1\0`; the participant cut is already sorted.
Reserve mint/verify and stage mint/verify call this one digest owner. Persist
the authority in `PendingReservation`; project it through the private request
and ticket conversions; compare it before `AnalyticalCodec` decode, cache
lookup, runtime construction, provider resolution, or source IO. Later messages
may add operation/body/stage/task binding but may not replace graph authority.

**REFACTOR.** One digest helper owns the field sequence; do not maintain
parallel reserve/stage lists or add public namespace fields absent from
`AuthorizedQueryContext`.

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

**GREEN.** Make `AnalyticalStageIngress` own a per-graph state machine:
`Absent -> Activating{authority, shared completion} -> Active{Arc<GraphLease>}
-> Draining -> Released`. Install `Activating` atomically under its narrow map
lock, release the lock before fallible work, and let equivalent callers await a
watch/oneshot-backed shared completion. Mismatches compare authority under the
lock and fail immediately. `GraphLease` owns the pending activation guard,
query resources, runtime, cancellation/deadline, and supervisor graph guard; it
never exposes a take-once resource hole.

**REFACTOR.** Keep state transitions inherent on `AnalyticalStageIngress` and
`GraphLease`; no free-function orchestration or lock held across await/IO.

### Scenario 3 — Publish-after-success and rollback

**Behavior.** Runtime creation, spill/runtime installation, and supervisor
registration all succeed before publication. Any injected failure wakes all
waiters with the same error and restores the pending reservation only while its
original TTL remains valid; otherwise it releases the permit/resources. Maps
REQ-004, REQ-007, INV-003, INV-004, AC-001, AC-004.

**RED.** Add
`oracle::analytical::tests::graph_activation_failure_rolls_back_without_publication`.
Inject failure at runtime build and supervisor registration on both sides of
the TTL boundary; assert no active map entry, no provider/source IO, one pending
entry before expiry, zero ownership after expiry, and equal waiter results.
Exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::graph_activation_failure_rolls_back_without_publication)'
```

**GREEN.** Replace `lease_graph` with `begin_graph_activation`, returning a
private `PendingGraphActivation` that retains the complete pending entry and
offers only `commit(supervisor_guard, runtime)` or `rollback(now)`. Commit
moves ownership directly into the lease and then publishes `Active`. Rollback
reinserts the unchanged entry if `expires_at > now` and the reservation slot is
still vacant; otherwise drops the exact permit/resources. A cancelled activator
executes the same rollback through an owned guard, not detached work.

**REFACTOR.** Centralize reinsertion collision/expiry handling in the registry;
the ingress owns orchestration but never edits reservation internals.

### Scenario 4 — Follower drain and shutdown visibility

**Behavior.** A follower lease is not released while stage drivers, tasks,
cache entries, exchange streams, runtime, or spill cleanup remain live. Cleanup
failure leaves `Draining`, fails readiness, and is joined/visible at shutdown.
Maps REQ-007, REQ-009, REQ-011, INV-003, INV-004, INV-005, AC-001, AC-004,
AC-005, AC-007.

**RED.** Add
`oracle::analytical::tests::follower_graph_release_waits_for_children_and_retains_cleanup_failure`.
Use deterministic child gates and injected cleanup timeout; assert the permit,
pool, scratch, graph map, and active gauge remain owned until every child
joins, and the failed case remains draining/readiness-failing. Exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::follower_graph_release_waits_for_children_and_retains_cleanup_failure)'
```

**GREEN.** Add explicit async `GraphLease::settle(outcome)` invoked by normal
terminal, cancellation, failure, and `AnalyticalStageIngress::shutdown`.
Settlement closes stage admission, cancels on non-success, joins supervisor
children/cache/exchanges, verifies the pool idle, removes spill files, then
releases the supervisor guard and reservation entry. Timeout/error records a
closed production cleanup outcome and retains the lease in `Draining`; `Drop`
only signals cancellation/leak telemetry and never spawns cleanup or claims
release.

**REFACTOR.** One settlement order serves normal and shutdown paths; readiness
reads the owning ingress/supervisor state rather than a test-only registry.

## Cross-scenario decisions and invariants

The activation transaction is process-local and in-memory; it changes no
durable schema. The two-second TTL is measured from follower reservation
acceptance and never extended. Transport authentication and purpose tickets
remain preconditions. Graph identity and authority never become metric labels.

Authority: `architecture/bifrost-design.md`,
`architecture/wyrd-security-posture.md`,
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

- Mutation-sensitive authority test and canonical field inventory.
- Both stage orderings, a true concurrent duplicate, one activation count, and
  identical reused lease identity.
- Runtime/supervisor failure proof on both sides of the unchanged TTL.
- Joined child/resource snapshots and readiness-visible draining failure.
- Generated private-contract provenance if authority fields change.

## Stop conditions

Return `SPEC_REVISION_REQUIRED` if exact activation requires a public field,
weaker tenant/peer authority, a longer TTL, a second registry/supervisor, or
best-effort cleanup. Return `PLAN_BLOCKED` if the pinned dependency prevents
the existing stage ingress from awaiting an activation before decode without a
fork; current adapters provide this interception point, so no block is known.
