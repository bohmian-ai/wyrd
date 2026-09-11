---
id: SPEC-oracle-local-admission
revision: 2
status: approved
---

# Simplify Oracle admission to pod-local OLAP controls

## Human intent and user value

Oracle shall use the boring admission model proven by high-throughput OLAP
systems: every pod protects its own CPU, memory, scratch, and concurrency;
tenant-aware local queues provide fairness; and load balancing across replicas
adds throughput. PostgreSQL-coordinated cluster admission is unnecessary
because cluster-wide tenant quotas are not a product requirement.

Operators must be able to run differently sized Oracle replicas without shared
policy conflicts, and concurrent Oracle-owned DataFusion consumers must never
reserve beyond a pod's hard governed memory budget.

## Scope

- Replace durable cluster-wide Oracle admission with pod-local admission.
- Preserve independent Interactive and Analytical query classes and local
  tenant fairness.
- Make one pod-wide Oracle memory root authoritative for governed DataFusion
  reservations across every local query and distributed-query participant.
- Separate query concurrency, CPU parallelism, memory, scratch, and analytical
  participant limits.
- Enforce the configured Analytical selected-worker limit.
- Align Bifrost, DataFusion, operations, configuration, metrics, and test
  authority with the simplified model.
- Remove runtime Oracle admission ownership, policy initialization, renewal,
  delegation, and overdraft behavior backed by PostgreSQL.

## Non-goals

- Cluster-wide tenant, principal, workload, or query concurrency quotas.
- A distributed admission coordinator, global semaphore, workload tree,
  capacity lease, or replacement database protocol.
- Strong tenant isolation through dedicated Oracle pools or warehouses.
- Runtime CPU preemption or resizing an executing query's memory ceiling.
- Replacing DataFusion planning, `DistributedExec`, streamed exchanges, spill,
  query audit, authorization, deadlines, or terminal semantics.
- Dropping historical database migrations solely because current runtime code
  no longer consumes their tables.
- Adding a public wire field, SDK contract, Card kind, or client-selected query
  class.

## Definitions

- **Pod-local admission:** one Oracle process independently decides whether a
  query or distributed participant may consume that process's resources.
- **Tenant fairness:** within each local query class, waiting queries remain
  FIFO per tenant and ready tenants rotate with equal weight so one busy tenant
  cannot monopolize the pod.
- **Oracle memory root:** the hard aggregate reservation capacity beneath which
  every Oracle-owned DataFusion query, operator, and exchange consumer on one
  pod is registered.
- **Query ceiling:** the immutable maximum one admitted query may allocate from
  the shared Oracle memory root during its lifetime.
- **Selected worker:** an authenticated remote Oracle chosen for one Analytical
  query and admitted independently by that worker's pod-local controls.

## Required behavior

### REQ-001 — Admission is pod-local

Every Oracle pod shall admit leader and follower work using only its local
queues and resource owners. Query admission shall not create, read, lease,
renew, reconcile, or wait for PostgreSQL admission policy or allocation state.
Adding an Oracle replica shall add local capacity without requiring its
physical limits to equal any other replica.

Pod CPU, memory, scratch, query-slot, Interactive-floor, queue, and worker-
fanout capacities are local operational values and shall not participate in
cross-replica contract-critical equality. Protocol, security, durable-format,
and compatibility values remain subject to the existing replica-agreement
boundary.

### REQ-002 — Tenant fairness remains local and bounded

Interactive and Analytical queues shall retain FIFO ordering within each
tenant and equal-weight round-robin rotation across ready tenants. Queue length
and wait time shall remain bounded. Overload shall return the existing typed
retryable admission failure without starting query execution or retaining
resources.

Local tenant limits apply independently on every replica. Load balancing may
therefore increase a tenant's aggregate cluster throughput; Oracle shall not
claim or emulate a strict cluster-wide tenant ceiling.

### REQ-003 — One hard aggregate memory boundary

All governed DataFusion reservations for concurrent Oracle work on a pod,
including Oracle-owned operators and streamed exchanges for leader and follower
execution, shall register against one hard Oracle memory root. Every fallible
reservation that would cross the configured root limit shall be refused. Query
ceilings may subdivide that root, but independent ceilings shall never replace
the shared root as the pod safety boundary.

Memory refusal shall spill where the existing operator supports spill or fail
the owning query with the existing typed resource failure. It shall never
borrow unaccounted memory from another query or rely on independently sized
pools as the pod safety boundary.

The resource plan shall retain explicit process headroom for DataFusion
infallible bookkeeping, dependency allocations outside cooperative reservation,
and other process memory. Oracle shall not claim that a cooperative reservation
pool alone makes operating-system or cgroup OOM impossible.

### REQ-004 — Query classes share capacity safely

Interactive and Analytical paths shall keep separate bounded queues and local
class counters beneath one atomic total capacity check. Analytical work shall
never consume the configured Interactive physical-slot floor. Idle capacity
outside that floor shall remain work-conserving, and a continuously ready class
or tenant shall not be skipped indefinitely.

### REQ-005 — CPU, concurrency, memory, and scratch remain distinct

Local query concurrency and DataFusion execution parallelism shall be bounded
independently from memory. Safe defaults and approved calibration shall account
for both effective CPU and memory rather than deriving executable query
concurrency from memory alone. Scratch remains a separately enforced local
resource.

### REQ-006 — Analytical fan-out is bounded

An Analytical query shall select no more than its configured remote-worker
limit from the pinned eligible cut. Only selected workers shall reserve query
resources or affect admission success. Each selected worker shall independently
enforce its local memory, scratch, slot, deadline, authorization, and cleanup
boundaries.

### REQ-007 — Ownership spans the complete query lifetime

Every local admission permit, memory allocation, scratch reservation, slot,
source projection, selected-worker reservation, and query runtime shall remain
owned until success, failure, cancellation, or deadline settlement. Ordinary
terminal cleanup releases each owner exactly once. Unconfirmed distributed
cleanup remains fail-closed and observable rather than being reported as free
capacity.

### REQ-008 — Operators can understand local saturation

Oracle shall expose bounded-cardinality metrics for local active and queued
queries by class, queue refusal and wait, aggregate governed memory use and
refusal, scratch and slot occupancy, selected-worker count, and terminal
cleanup health. Metrics shall not claim cluster-wide quota or capacity
enforcement.

## Invariants and prohibited outcomes

- **INV-001:** Aggregate governed DataFusion reservations on one pod never
  exceed the configured Oracle memory-root limit; every Oracle-owned fallible
  consumer participates in that root.
- **INV-002:** A tenant cannot bypass the local tenant scheduler by missing a
  cache, contacting PostgreSQL, or consuming overdraft.
- **INV-003:** Differently sized Oracle replicas never conflict merely because
  their local capacity differs.
- **INV-004:** Analytical work never consumes the Interactive physical-slot
  floor.
- **INV-005:** One Analytical query never reserves every live Oracle unless its
  configured worker limit explicitly includes every eligible worker.
- **INV-006:** Query class remains derived from the single pinned physical root
  and is never supplied by a caller.
- **INV-007:** Removing cluster admission never weakens authentication,
  authorization, tenant isolation, query audit, deadlines, result limits, or
  fail-closed distributed cleanup.
- **INV-008:** No new distributed policy, lock, lease, allocation, renewal, or
  overdraft mechanism replaces the removed PostgreSQL path.

## Externally observable behavior and failure modes

- A query admitted by one Oracle replica is governed entirely by that replica
  and, for Analytical work, by the selected workers' local resource owners.
- Two healthy replicas with different CPU or memory limits may start and serve
  simultaneously without canonical admission-policy agreement.
- Under contention, ready tenants make bounded local progress according to
  equal local shares; a saturated queue returns a retryable overload response.
- Memory, scratch, slot, participant, cancellation, and deadline failures
  remain typed terminal query failures and never produce a successful partial
  result.
- Scaling from one replica to multiple replicas increases total available
  tenant concurrency. No response or metric promises a cluster-wide tenant
  quota.

## Material constraints

- Preserve the single-build physical-root selection and the existing
  Interactive versus `DistributedExec` Analytical boundary.
- Preserve one query-owned `RuntimeEnv` and query ceiling while making the
  pod-wide aggregate root authoritative for governed reservations.
- Preserve current tenant identity, local equal-weight fairness, audit WAL,
  authorization, spill ownership, cancellation, cleanup, and terminal result
  contracts.
- Reuse current Oracle admission and Bifrost resource owners; do not add a new
  scheduler framework or dependency.
- PostgreSQL remains the durable control plane for its other responsibilities,
  but not Oracle query admission.
- Update architecture authority that currently describes durable cluster
  delegation or permits memory-only concurrency sizing.

## Required system boundaries and cross-boundary flow

1. Server boot derives each pod's Oracle resource plan solely from that pod's
   effective resources and approved local configuration.
2. The pinned planner builds one physical root and derives Interactive or
   Analytical from that root.
3. The local tenant-aware class queue admits the leader beneath one atomic
   local capacity root.
4. The admitted query receives a query-owned runtime and ceiling backed by the
   pod-wide Oracle memory root.
5. Interactive executes locally. Analytical selects a bounded worker subset;
   each selected worker independently admits its participant beneath its own
   local root.
6. Terminal ownership releases locally on every participant. PostgreSQL does
   not participate in this flow.

This is internal server behavior and deployment configuration, not a public
client or Card contract.

## Acceptance obligations and evidence classes

### AC-001 — PostgreSQL-free admission

Focused boot and query tests prove differently sized Oracle replicas start,
admit, execute, and restart without Oracle admission policy rows, allocation
blocks, leases, renewals, or overdraft. Static consumer evidence proves no
production Oracle query path reaches the retired SQL admission owner. A stale,
conflicting historical admission row is seeded and demonstrably ignored.
Covers REQ-001 and INV-002, INV-003, INV-008.

### AC-002 — Hard aggregate memory qualification

Deterministic resource tests admit sequential and concurrent mixed-class
queries whose individual ceilings would previously exceed the pod budget and
prove governed reservations cannot cross the root. DataFusion execution
evidence proves fallible allocation refusal spills or fails cleanly without
cross-query borrowing and the tested server remains healthy. Configuration
evidence proves explicit non-DataFusion process headroom remains outside the
root. Covers REQ-003, REQ-007 and INV-001, INV-007.

### AC-003 — Local multi-tenant fairness and overload

Focused admission tests and a deterministic barrier-controlled multi-tenant
server journey prove per-tenant FIFO, equal-weight rotation, bounded queueing,
retryable overload, Interactive-floor protection, work conservation, and
eventual progress for both classes. Covers REQ-002, REQ-004, REQ-005 and
INV-004.

### AC-004 — Bounded Analytical participants

A multi-node Analytical journey proves selected workers never exceed the
configured limit, unselected replicas reserve no resources and cannot fail the
query, selected workers enforce their local roots, and terminal cleanup returns
all confirmed ownership. Covers REQ-006, REQ-007 and INV-005, INV-006,
INV-007.

### AC-005 — Operational and architecture closure

Focused configuration, metrics, SQL consumer-closure, and documentation checks
prove CPU-aware local sizing, bounded-cardinality local saturation signals,
removal of cluster-quota claims, and alignment of Bifrost, DataFusion,
operations, and testing authority. The complete Bifrost verification lane,
formatting, and linting pass. Covers REQ-005, REQ-008 and all invariants.

## Open material decisions

None.

## Planning-decision inventory

`$wyrd-plan` must fix the exact local admission and aggregate-memory owners,
safe CPU/memory sizing formula and configuration closure, tenant/class
scheduling state transitions, Interactive-floor accounting, DataFusion shared
memory-pool composition, selected-worker subset rule, removal and migration
closure for durable admission code, metrics projection, test topology, exact
RED commands, and focused verification while preserving current query and
cleanup protocols.

## Revision history

- 2026-09-07 — Revision 2 approved under the user's authorized one-round
  workflow after independent Ponytail review: scoped memory safety to governed
  DataFusion reservations plus process headroom, fixed tenant fairness to
  equal-weight rotation, and separated local capacity from replica agreement.
- 2026-09-07 — Revision 1 drafted from the user's decision to adopt the
  ClickHouse-style per-host methodology, retain pod-local tenant fairness, and
  remove cluster-wide Oracle quotas and PostgreSQL admission coordination.

## Authority links

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/wyrd-security-posture.md`
- `architecture/operations/README.md`
- `architecture/operations/deployment-and-release.md`
- `architecture/operations/reliability-and-recovery.md`
- `architecture/references/doctrine/architecture-constraints.md`
- `architecture/references/domain/olap-serving.md`
- `architecture/references/domain/datafusion.md`
- `architecture/references/domain/analytical-operations-reliability.md`
- `architecture/references/languages/spec-driven-development.md`
- [ClickHouse workload scheduling](https://clickhouse.com/docs/concepts/features/configuration/server-config/workload-scheduling)
- [ClickHouse memory overcommit](https://clickhouse.com/docs/concepts/features/configuration/settings/memory-overcommit)
- [ClickHouse high-concurrency guidance](https://clickhouse.com/resources/engineering/high-concurrency-sizing-user-analytics)
- [ClickHouse parallel replicas](https://clickhouse.com/blog/clickhouse-parallel-replicas)
