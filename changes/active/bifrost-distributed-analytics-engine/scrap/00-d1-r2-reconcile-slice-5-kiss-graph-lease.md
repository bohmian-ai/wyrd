---
task_id: BIFROST-T1-C05-R2
title: Finish one drain-safe GraphLease with aggregate query bounds
kind: reconciliation-remediation
status: proposed
execution_skill: wyrd-implement
implementation_style: tdd
approved_authority: SPEC-bifrost-distributed-analytics-engine
spec_id: SPEC-bifrost-distributed-analytics-engine
spec_revision: 1
requirements:
  - REQ-002
  - REQ-003
  - REQ-004
  - REQ-005
  - REQ-006
  - REQ-007
  - INV-001
  - INV-002
  - INV-003
  - INV-004
  - INV-005
  - INV-006
  - AC-002
  - AC-003
  - AC-004
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
---

# Finish one drain-safe GraphLease with aggregate query bounds

## Outcome

Finish the existing follower `GraphLease` so one distributed query owns one
exact reservation, runtime, aggregate memory pool, scratch allocation,
cancellation tree, and deadline until all graph work drains. Keep the current
upstream `datafusion-distributed` pin. Remove the fixed exchange-memory
precharge and do not replace it with operator/exchange sub-pools, a hard
per-message proof, or a dependency fork.

This is the KISS successor to superseded task `BIFROST-T1-C05-R1`. It retains
the correctness work needed to execute queries—exact ownership, duplicate
activation, rollback, retry fencing, and structured cleanup—while deleting the
unnecessary byte-perfect exchange design.

## Current-state amendment

The frozen candidate already has the basic `ReservationRegistry`, graph ticket,
`GraphLease`, `AnalyticalStageIngress`, `AnalyticalStageEgress`,
`AnalyticalSupervisor`, query-owned runtime, bounded worker configuration, and
three-Oracle/one-Scribe journey. Retain those concrete owners.

Retain these corrections from R1:

- the existing two-second pending TTL, with reservation moved immediately
  before first stage dispatch;
- a closed Fragment-versus-Graph reservation purpose and complete graph
  authority tuple;
- one graph-keyed activation owner, exact duplicate waiters,
  publish-before-wake, and owner-only synchronous rollback;
- first-write/exact-compare graph authority;
- one explicit post-drain attempt-zero to attempt-one transition seam; and
- structured cancel/join/drain before terminal lease release.

Delete these R1 requirements:

- an exact `W`/`S` exchange-byte formula;
- a strict FlightData pre-queue ceiling;
- `AnalyticalGraphMemoryPool` or any operator/exchange partition wrapper;
- a fixed per-attempt exchange-memory reservation as accounting authority;
- dependency-fork, upstream-PR, or upstream-merge work; and
- tests that claim exact prediction of dependency-owned bytes.

## Scope and owners

Production owners remain:

- `crates/wyrd-spec/src/vala/api.rs` and the existing private peer protobuf /
  `wyrd-tonic` conversion for reservation purpose and generation;
- `crates/vala/vala-bifrost-redux/src/oracle/dispatcher.rs` for pending and
  active reservations;
- `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs` for graph
  activation, immutable authority, and lifecycle;
- `crates/vala/vala-bifrost-redux/src/oracle/analytical_supervisor.rs` for the
  graph-owned runtime, attempts, cancellation, joins, and aggregate resources;
- `crates/vala/vala-bifrost-redux/src/oracle/analytical_transport.rs` for the
  existing bounded worker/client transport and exchange observation; and
- the existing process journey and its minimum test-support controls in
  `wyrd-testing`.

Do not create a new service, registry, framework, trait, resource pool, config
section, dependency revision, or public API.

## Closed implementation design

### 1. Exact reservation and authority

`ReservationRegistry` stays the sole owner of pending and active follower
capacity. Replace optional graph identity with the closed
`NodeReservationPurpose::{Fragment, Graph(GraphReservationAuthority)}` from R1.
The graph authority binds public/DataFusion query IDs, authenticated principal,
tenant/space, leader/destination node and fence, snapshot/permission/participant
digests, deadline, and one complete authority digest.

The follower returns a nonzero process-local `ReservationGeneration`. Tickets
and releases carry the exact reservation ID, generation, and authority digest.
Under the registry lock, compare purpose and the complete tuple before removing
anything. An exact graph activation moves the existing `OracleQueryResources`
into one `Arc<GraphLease>`; an exact duplicate returns that same lease. Any
mismatch leaves the reservation usable by its owner.

Keep `min(requested_expiry, now + PENDING_TTL)` with the existing two-second
TTL. The leader finalizes the plan, participants, and authority first, then
reserves followers immediately before minting and sending the first SetPlan or
ExecuteTask. Do not extend or refresh the pending TTL.

### 2. One activation owner

`AnalyticalStageIngress` owns
`GraphAdmissionEntry::{Activating, Active}`. `Activating` holds one concrete
graph-local result latch. The first exact SetPlan or ExecuteTask inserts it and
becomes owner; concurrent exact duplicates wait; mismatches fail without
mutation.

The owner alone transfers the registry lease, builds and installs the
query-owned runtime, records immutable egress authority, and registers the
graph with `AnalyticalSupervisor`. It replaces `Activating` with `Active`
before publishing success and waking waiters. On any fallible step it
synchronously undoes completed registrations, aborts the exact activation,
publishes one cloneable failure, and wakes all waiters. Waiters never activate
or roll back. SetPlan-first and ExecuteTask-first remain equally valid.

### 3. Structured graph lifetime

The active graph owner retains open stage-call count, current attempt, retry
hold, cancellation, and any cleanup join. A retry may advance only from attempt
zero to one after every attempt-zero driver, stream, cache entry, connection,
and spill owner is idle. This task supplies that seam; it does not implement
automatic retry policy.

Terminal cleanup cancels, joins descendants, removes runtime and egress
authority, then releases the exact registry lease. A drain timeout is a failed
terminal cleanup and retains the graph owner for shutdown/inspection. `Drop`
may signal an already retained owner; it may not spawn detached cleanup or
release capacity.

### 4. KISS aggregate resources

`OracleQueryResources` remains the graph's one aggregate resource envelope and
its existing query-local DataFusion pool is the only pool installed in the
runtime. Operators, `WorkerConnection`, and `NetworkBoundary` all use that
pool. Do not add another `MemoryPool` implementation.

Delete the per-attempt
`try_split_memory(EXCHANGE_CONSUMER, exchange_buffer_bytes)` call and the
corresponding `exchange_memory` owner from `AnalyticalAttemptState`. Remove
exchange bytes from attempt grant/release evidence where they describe a
precharge. Keep `analytical_exchange_buffer_bytes` only as the existing finite
worker-connection/backpressure and tonic message setting; it is not a reserved
or guaranteed exchange allocation. Keep `measured_exchange` as bounded
current/peak telemetry, not admission authority.

Before reservation, validate the finalized static graph against existing
limits only:

- selected remote workers do not exceed `max_workers_per_query`;
- desired tasks and every stage partition range do not exceed the admitted
  `OracleSessionShape::target_partitions`;
- all count/range conversions and allocations use checked arithmetic;
- the existing Oracle admission queue and upstream connection/network queues
  remain finite; and
- scratch demand fits the existing query scratch allocation.

Do not introduce a maximum-stage setting merely to future-proof. The bounded
decoded plan, static task count, target partitions, and worker limit are the v1
graph-shape bounds.

Apply the existing `analytical_exchange_buffer_bytes` value as finite tonic
encode/decode limits on the returned worker service and worker client through
their public generated methods. This is a transport failure boundary, not a
claim about pre-transport allocations. An oversized message fails the query
through the existing `QueryExecutionFailed` terminal mapping. Do not patch the
dependency.

The existing query pool and deployment memory limit are the hard aggregate
boundaries. Record aggregate pool current/peak plus dependency-reported
exchange current/peak through `OracleTelemetry`; do not add high-cardinality
labels or infer reservation success from metrics.

## Ordered RED -> GREEN -> REFACTOR scenarios

Execute one scenario at a time.

1. **RED:** add `graph_reservation_refusal_preserves_exact_owner`; prove early
   reservation expiry, purpose/authority/generation mismatch, and destructive
   refusal. **GREEN:** finalize before reserving, add closed purpose/generation,
   compare before removal, and preserve the two-second TTL. **REFACTOR:** keep
   exact comparison on the concrete authority type.
2. **RED:** add
   `graph_activation_is_rollback_safe_and_authority_is_immutable`; race both
   first-message orders, duplicates, mismatch, owner failure, waiter
   cancellation, and shutdown. **GREEN:** implement one owner, waiters,
   publish-before-wake, owner-only rollback, and first-write/exact-compare
   authority. **REFACTOR:** keep orchestration on ingress/egress owners.
3. **RED:** add `graph_cleanup_retains_owner_until_children_join`; expose
   detached cleanup, pre-drain release, fail-open timeout, and retry overlap.
   **GREEN:** retain cancellation/join ownership and drain before retry/release;
   retain the owner on timeout. **REFACTOR:** delete obsolete Drop cleanup.
4. **RED:** add `graph_uses_one_query_pool_without_exchange_precharge` and
   `analytical_transport_is_finite_without_dependency_fork`; prove the fixed
   exchange precharge reduces operator capacity, the worker transport is
   unlimited, and graph-count overflow is accepted. **GREEN:** remove the
   precharge, install the same query pool for operators and exchanges, enforce
   existing finite graph counts and tonic limits, and map pressure to the
   existing failed terminal. **REFACTOR:** delete obsolete exchange-grant
   fields and keep measurement observational.
5. **RED:** extend
   `peer_network::analytical::graph_lease_owns_exact_resources_for_complete_graph`
   with exact refusal, duplicate activation, rollback, retry-drain, aggregate
   memory pressure, transport refusal, and terminal cleanup. **GREEN:** add
   only the minimum process controls needed to drive those cases over real peer
   transport. **REFACTOR:** remove unit-only journey duplicates.

## Exact focused commands

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
  -E 'test(=oracle::analytical_supervisor::tests::graph_uses_one_query_pool_without_exchange_precharge)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  --features test-support,bench-support \
  -E 'test(=oracle::analytical_transport::tests::analytical_transport_is_finite_without_dependency_fork)'

scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
   -p wyrd-testing --test oracle -P journey --run-ignored=all \
   -E "test(=peer_network::analytical::graph_lease_owns_exact_resources_for_complete_graph)"'
```

## Broader verification

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

`mise run gate` is not required for this bounded slice.

## Completion evidence

The implementation report must include:

- RED and GREEN output for all six exact focused commands;
- S5-RF-01 through S5-RF-09 mapped to commits and evidence;
- proof that refusal preserves the exact owner's pending reservation;
- proof of one activation owner, one shared lease/runtime, waiter publication,
  and owner-only rollback;
- proof that retry attempt one starts only after attempt zero drains;
- proof that operators and exchanges use one query pool and no fixed exchange
  reservation or partition wrapper remains;
- proof that existing worker/task/partition/queue/scratch/tonic limits are
  finite and oversized work fails without partial success;
- proof that every normal terminal path returns all owners and drain timeout
  retains a visible owner; and
- broader verification output and the updated remediation ledger.

## Stop conditions

Stop with `SPEC_REVISION_REQUIRED` only if implementation requires a public
contract change, a dependency fork, a second resource pool/root, recovery of
live graph work across process restart, more than one retry, or making
StageGraph production-reachable in this slice.

Ordinary helper names, private error plumbing, and test fixture layout remain
implementation choices. Do not stop merely because exact dependency-internal
exchange bytes cannot be predicted; revision 1 explicitly does not require
that proof.

## Authority

- approved [`SPEC-bifrost-distributed-analytics-engine` revision 1](../spec.md);
- [`AGENTS.md`](../../../../AGENTS.md);
- [`architecture/bifrost-design.md`](../../../../architecture/bifrost-design.md);
- [`architecture/references/domain/datafusion.md`](../../../../architecture/references/domain/datafusion.md);
- [`architecture/references/domain/analytical-operations-reliability.md`](../../../../architecture/references/domain/analytical-operations-reliability.md);
- original task
  [`00-d1-inactive-distributed-execution-engine.md`](00-d1-inactive-distributed-execution-engine.md);
- superseded reconciliation
  [`BIFROST-T1-C05-R1`](00-d1-r1-reconcile-slice-5-graph-lease.md); and
- approved peer-plane remediation
  [`01-t1-inactive-distributed-execution-review-remediation.md`](../remediation/01-t1-inactive-distributed-execution-review-remediation.md).
