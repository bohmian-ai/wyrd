---
id: ORACLE-LOCAL-T01
title: Replace durable Oracle admission with bounded pod-local scheduling
kind: implementation
mode: DECOMPOSE
status: ready
spec: SPEC-oracle-local-admission
spec_revision: 2
depends_on: []
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-005]
---

# Simplify Oracle admission to one pod-local resource path

## Outcome and value

Oracle admission becomes ordinary per-pod OLAP admission: bounded tenant-aware
queues decide who runs, one local resource governor protects the process, and
the load balancer gains throughput by adding independently sized replicas.
PostgreSQL no longer participates in query admission, and one Analytical query
uses only its configured number of remote workers.

Required execution skill: `$wyrd-implement`.

## Current repository facts

- `OracleAdmission` already owns local Interactive and Analytical tenant queues,
  deadlines, cancellation, release, and metrics, but its two class capacities
  are independent and `grant_waiters` always scans Interactive before
  Analytical.
- `BifrostResourceGovernor` is already the sole process memory, scratch, and
  slot ledger. Query admission charges a small class quantum there, then gives
  each query an independent `FairSpillPool` ceiling; simultaneous query pools
  can therefore reserve more governed DataFusion memory than the pod owns.
- DataFusion 55 explicitly supports one `MemoryPool` shared by concurrent plans.
  Its `try_grow` is the fallible safety boundary; `grow` must always succeed, so
  the process plan must retain the existing unmanaged headroom and cannot claim
  absolute cgroup-OOM prevention.
- Boot derives local slots from Oracle memory, splits them into class values,
  and also persists those values as four canonical PostgreSQL policies.
  Differently sized replicas can consequently reject each other at startup.
- `DelegatedOracleAdmission` adds PostgreSQL allocation, renewal, continuity,
  and bounded overdraft before the same local scheduler. Strict cluster-wide
  tenant quotas are not a product requirement.
- `OracleConfig::max_workers` is projected from
  `OracleRuntimeConfig::max_workers_per_query`, but current Analytical fan-out
  can reserve every eligible remote Oracle in the pinned cut.

## Owners, scope, consumers, and prohibited changes

- `OracleAdmission` remains the sole leader-query queue and tenant-fairness
  owner.
- `BifrostResourceGovernor` remains the sole process resource authority and
  the one aggregate leader-plus-follower slot-unit ledger; `OracleResources`
  carries the immutable local class split and owns one shared Oracle DataFusion
  root issued from that governor.
- One private state-bearing per-query `MemoryPool` view owns only a query
  ceiling and delegates every consumer registration and reservation to the
  shared Oracle root. Do not add a scheduler framework, pool factory, trait, or
  dependency.
- Oracle planning/dispatch owns deterministic bounded worker selection from the
  already pinned eligible cut.
- `wyrd-server` owns local configuration derivation and boot composition.
- `vala-sql` retains historical migrations and stored rows, but no production
  query-admission consumer.

Do not alter query-class derivation, authentication, authorization, tenant
identity, audit WAL, result limits, spill formats, object-store reads,
Postgres responsibilities unrelated to admission, public client contracts, or
distributed terminal cleanup semantics. Do not replace the deleted durable
path with another lease, lock, cache, workload tree, or cluster coordinator.

## Selected implementation architecture

### Local capacity and fairness

Keep the existing `interactive_slots` and `analytical_slots` configuration but
give them one unambiguous local meaning:

```text
total local slot units       = interactive_slots + analytical_slots
protected Interactive units = interactive_slots
maximum Analytical units    = analytical_slots
maximum Interactive units   = total local slot units
```

Admission accounts physical slot units, not query counts: Interactive costs
one unit and Analytical costs two, matching `OracleWorkerClass::slot_units`.
Analytical cannot consume the protected units; Interactive may borrow unused
non-protected units. Retain independent active-query counters for telemetry.

Repurpose `OracleConfig::tenant_interactive_slots` and
`tenant_analytical_slots` as fixed pod-local per-tenant slot-unit caps.
Approved calibration supplies them through renamed
`tenant.interactive_slot_limit` and `tenant.analytical_slot_limit` proposal
leaves; without a profile each cap equals its local class capacity. Validate
the Interactive cap as nonzero and no greater than its class maximum. When
Analytical capacity is at least two units, its default tenant cap equals that
capacity and validation requires a value from two through that capacity. When
Analytical capacity is below its two-unit query cost, fold any one-unit
remainder into Interactive, set Analytical capacity and its tenant cap to zero,
and treat `analytical_slots = 0` as a valid boot state. Analytical admission
then immediately returns `BifrostError::QueryAdmissionRejected` without
queueing, allocating scratch, constructing a query view, or contacting a peer;
do not expose a class that can never admit one query.
Delete `single_tenant_ceiling`, `multi_tenant_ceiling`, contender-dependent
ceilings, and their calibration/config projections. Within each class, retain
FIFO per tenant and rotate eligible tenants one grant at a time with equal
weight. First grant ready Interactive work until its protected floor is
satisfied. For remaining capacity, compare the monotonically increasing waiter
IDs at the eligible Interactive and Analytical class heads and grant the older
one. Continue until no eligible request fits. This is work-conserving without
letting an idle Analytical queue strand capacity or a continuously ready class
starve.

The explicit `oracle_query_slot_limit` remains the operator override. Delete
the unused `OracleRuntimeConfig::cpu_cores` setting; detected
`ResourcePlan::effective_cpu` is the sole CPU source. Without an explicit slot
limit, derive raw local slot units as:

```text
memory units = max(1, effective Oracle bytes / 32 MiB)
CPU units    = max(1, floor(2 * effective CPU cores))
raw units    = min(memory units, CPU units)
```

Use checked floating-point multiplication plus the existing
`checked_floor_u32`; reject non-finite, overflowing, or zero CPU results. Split
the raw units with the existing calibration profile when present;
otherwise preserve at least one Interactive unit and allocate the remainder to
Analytical, applying the below-two-unit disable rule above. A calibration may
set Analytical to zero or at least two, never one. This restores the already
documented two-units-per-core default while leaving one explicit
calibration/override seam.

### One governed Oracle memory root

Add one concrete `OracleMemoryRoot` inside `resources.rs`, owned as an `Arc` by
`OracleResources`; it is not a trait or factory. Its hard cooperative limit is
the checked sum `ResourcePlan::oracle_floor_bytes +
ResourcePlan::elastic_memory_bytes`, after the resource plan has subtracted the
explicit unmanaged/process-headroom reserve. It owns one tracked
`FairSpillPool`, the governor handle, aggregate bytes, and one narrow operation
mutex. The pool and governor use that same maximum; the governor also subtracts
elastic bytes currently owned by Scribe or Forge before accepting Oracle
growth. Infallible growth beyond the cooperative limit is charged only to the
explicit headroom/overshoot counter, never presented as governed free capacity.

Add exactly three crate-private governor operations; no caller mutates these
counters directly:

- `try_reserve_oracle_query_memory(bytes)` computes the Oracle floor-first
  borrow before and after growth, computes the incremental elastic borrow, and
  rejects without mutation when
  `state.elastic_memory_used_bytes + incremental_borrow >
  plan.elastic_memory_bytes`. On success it increments
  `oracle_memory_used_bytes`, `oracle_query_memory_used_bytes`, and only that
  incremental `elastic_memory_used_bytes`.
- `reserve_oracle_query_memory_infallible(bytes)` performs the same checked
  accounting without refusal. It charges the portion that fits to elastic and
  returns an `OracleMemoryCharge { governed_bytes, headroom_bytes }` whose
  remainder increments `oracle_infallible_bytes`; headroom bytes never become
  elastic/free capacity.
- `release_oracle_query_memory(charge)` uses the existing floor-first release
  calculation, decrements the exact governed, elastic, query, and headroom
  components once, advances the resource-change epoch, and poisons on any
  underflow. Fallible growth records an all-governed charge; each consumer's
  query-view entry retains its governed/headroom split, and shrink releases
  that consumer's headroom bytes first before governed bytes.

Every leader and follower query receives a private query view over that same
root, not a new finite pool. Each view owns a map from DataFusion's
process-unique `MemoryConsumer::id()` to that consumer's bytes plus a checked
query-total counter. `register` inserts the same consumer ID in the query view
and forwards that consumer unchanged to the root `FairSpillPool`; `unregister`
forwards removal and removes only a zero-byte consumer entry. This lets the
shared pool arbitrate every consumer while the view enforces one query ceiling.

Every `try_grow`, `grow`, and `shrink` runs through an inherent
`OracleMemoryRoot` operation while holding its operation mutex, so callers
cannot interleave the governor and pool ledgers. The root operation mutex is
always acquired first. Governor and `FairSpillPool` internal locks are never
held simultaneously: each invoked operation completes and releases its lock
before the next begins. No governor/shared-pool method may call back into the
root or acquire the operation mutex. The operation uses one non-nested
accounting order:

- forwards `register`, `unregister`, `try_grow`, `grow`, and `shrink` to the
  shared pool;
- for `try_grow`, tentatively charges the checked query-total counter, then
  charges `BifrostResourceGovernor` against Oracle's floor-first share and
  currently free process elastic bytes, then calls the shared pool; refusal
  occurs before mutation when `query_total + additional > query_ceiling`, and
  any later refusal rolls back completed steps in reverse order and retains no
  bytes;
- for infallible `grow`, accounts the query and governor bytes and then calls
  the shared pool's infallible growth; bytes beyond the cooperative root are
  recorded as headroom/overshoot and make later fallible growth refuse, but do
  not poison the process merely for using the trait's required infallible path.
  Because `grow` cannot reject, it may increase physically tracked bytes above
  the query ceiling, but it does not increase the bytes allocated *from the
  governed Oracle root*: all excess above the query ceiling or available root
  is classified in that consumer's process-headroom charge and exposed in
  metrics;
- makes `shrink` the only callback that releases successful bytes from the
  shared pool, process governor, and query counter;
- makes per-consumer `unregister` only forward removal to the shared pool,
  because sibling consumers may legitimately retain bytes;
- makes final query-pool-view and query-envelope teardown verify a zero
  aggregate query counter and poison the existing resource-health signal on
  divergence; neither may release bytes a second time;
- reports query-local `reserved()` and the query ceiling from `memory_limit()`,
  while the shared root supplies aggregate reserved/refusal metrics.
  `reserved()` includes governed and infallible/headroom bytes actually held so
  teardown can reconcile every byte. `memory_limit()` reports the immutable
  maximum governed allocation from the Oracle root; the difference, if any, is
  explicitly tracked process headroom outside that root rather than a larger
  query grant.

Successful growth order is per-consumer/query counters, governor, shared pool;
refusal rolls back completed steps in reverse. `shrink` releases shared pool,
governor, then per-consumer/query counters under the same operation lock. The
governor retains an explicit `oracle_infallible_bytes`
counter; `grow` increases it only for bytes above the fallible cooperative
ceiling, `shrink` reduces it before ordinary governed bytes, and snapshot/
metrics expose it as headroom use. Query-envelope terminal release returns slot
and scratch ownership only after all runtime consumers have drained and the
query view verifies zero bytes.
DataFusion's infallible path is covered by process headroom; it is not evidence
that cgroup OOM is impossible. The approved hard limit applies only to fallible
governed reservations; every infallible byte remains accounted and is released
through its retained `OracleMemoryCharge` split. Thus no query allocates more
than its ceiling from the governed Oracle root, while DataFusion's mandatory
infallible bookkeeping remains truthful and reconcilable outside that root.

Stop charging the 32/64 MiB class quantum as though it were resident query
memory. Slots govern concurrency; the shared pool governs actual cooperative
DataFusion reservation; scratch remains an independently retained query
lease. Keep the existing immutable per-query grant calculation only as the
query ceiling and partition-planning input. Fixed non-query Oracle allocations
may retain exact `OracleMemoryLease` ownership if a live caller still requires
it, but no leader or follower query may receive an independent finite pool.

### Bounded Analytical workers

From the pinned, authorized, live remote Oracle roster, sort by the existing
stable node identity, rotate by
`query_id.as_uuid().as_u128() % eligible.len()`, and take at most
`max_workers_per_query`. Do not add hashing, randomness, or a dependency.
Dispatch and reserve only that selected subset; zero means local execution
only. An unselected replica receives no request and cannot affect the query
result. The existing participant-local admission, deadline, cleanup, and
fail-closed uncertainty rules remain unchanged for selected replicas.

Both leader acquisition through `OracleAdmission` and follower acquisition
through `OracleResources::try_acquire_worker` atomically charge the same
class-aware governor slot ledger. The governor rejects when total units exceed
the local total or Analytical units exceed `analytical_slots`, thereby
preserving the Interactive floor across leaders and followers. Delete
`OracleSlotManager`'s independent running semaphore; retain only its bounded
peer-waiter semaphore. Every governor slot release advances the existing
resource-change epoch and notifies waiters. A queued leader observes that
notification in its existing bounded admission wait, re-runs `grant_waiters`
under the admission lock, and preserves the existing tenant FIFO/cursor state;
do not add a polling loop or background scheduler.

### Durable-path deletion and configuration closure

Delete production construction and use of `DelegatedOracleAdmission`, its
maintenance worker, demand/continuity channels, allocation cache, renewal,
overdraft, boot policy initialization, and continuity monitor. Remove their
server configuration fields, calibration projections, SQL query/row modules,
runtime tests, metrics, `OracleAdmissionScopeKind`, `OracleAdmissionDemand`, and
`OracleAdmissionContinuityLost`. Preserve historical SQL migrations/tables and
`AuditDetail::OracleAdmissionRecovery` solely to decode previously stored
events; seed conflicting old policy rows in a journey and prove current boot
and queries ignore them.

Bump `OracleCalibrationProfile::schema_version` from 1 to 2 for the renamed
tenant leaves. Update every checked-in fixture and generated/configuration
reference, accept only v2, and reject v1 without aliases or migration logic.

Keep existing local active/queued/wait/refusal, scratch, slot, and terminal
metrics. Project shared Oracle root use/refusal, infallible headroom use,
selected-worker count, and cleanup health with bounded class/result labels
only. Delete delegated, lease, renewal, allocation, and overdraft signals rather
than renaming them.

## Ordered implementation scenarios

### Scenario 1 — PostgreSQL-free heterogeneous boot and query

**Behavior.** Differently sized replicas start and serve using only local
capacity, even when historical canonical policy rows contain incompatible
values. Covers REQ-001, REQ-002, AC-001.

**RED.** Add
`capacity::heterogeneous_oracles_ignore_historical_admission_rows`. Start from
`BifrostClusterSpec::two_mixed()`, replace its two public `nodes` entries with
explicit `BifrostNodeSpec` values preserving their generated IDs/roles, and set
each entry's `oracle` to a distinct `TestOracleResources { system_resources:
Some(snapshot), ..Default::default() }`; do not call the cluster-wide
`with_system_resources` helper. Pass that spec to
`WyrdTestCluster::start_spec`. Seed conflicting historical policy/allocation
rows through `cluster.pg_fixture().operator_pool().pool()`. Execute one
Interactive query on each node, terminate and `restart_node` them in reverse
order, repeat the queries, and assert the retained per-node resource snapshots
still differ while no allocation, renewal, or overdraft activity occurs. It
initially fails because boot validates canonical policy equality and constructs
delegated admission.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-testing --test oracle -P journey --run-ignored=all \
  -E 'test(=capacity::heterogeneous_oracles_ignore_historical_admission_rows)'"
```

**GREEN.** Remove the durable boot/query path and its dead configuration,
runtime SQL modules, state monitor, and metrics. Preserve migrations/history
decoding. Make both nodes derive only their own local plan.

**REFACTOR.** Run production-consumer search from HTTP query entry through
Oracle construction and execution. No PostgreSQL Oracle-admission symbol may
remain reachable; stale durable state is inert rather than migrated or dropped.

### Scenario 2 — One aggregate DataFusion memory root

**Behavior.** Mixed concurrent leader and follower work keeps independent
query ceilings but every governed reservation competes beneath one pod root.
Covers REQ-003, REQ-005, REQ-007, AC-002.

**RED.** Add
`resources::tests::oracle_queries_share_one_governed_memory_root`. Admit an
Interactive query, an Analytical leader, and an Analytical worker whose
individual ceilings sum above the root. Grow named spillable and non-spillable
reservations behind a deterministic barrier; assert aggregate fallible growth
stops at the root, the refusing reservation retains no bytes, release restores
both query and process baselines, concurrent grow/refuse/shrink/drop completes
without deadlock or poison, and the next query succeeds. Extend the test through
a real DataFusion spilling operator and assert typed clean failure when scratch
is also exhausted. It initially fails because every query owns an independent
pool.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=resources::tests::oracle_queries_share_one_governed_memory_root)'
```

Add
`capacity::memory_refusal_preserves_oracle_health_and_next_query`. Through the
public server query path, arm both the existing schema stall and one
test-support-only memory-hold controller. `OracleResources` owns the controller
beside the shared root. On the next `try_acquire_query`, after constructing the
real query view, it creates and retains a named `MemoryReservation` of the armed
byte count through that view and signals `reached`; no fake pool or direct
counter mutation is allowed. `WyrdTestServer` exposes only
`hold_next_query_memory(bytes)`, `wait_query_memory_hold`, and
`release_query_memory_hold`, forwarding to that controller. Wait for both the
real reservation and schema stall, submit a query that receives the existing
typed resource refusal, release the memory reservation, release the stalled
query, then prove a smaller query succeeds on the same healthy server.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-testing --test oracle -P journey --run-ignored=all \
  -E 'test(=capacity::memory_refusal_preserves_oracle_health_and_next_query)'"
```

**GREEN.** Compose the shared tracked pool and the minimum per-query view above;
route all query, attempt, operator, exchange, and worker runtime construction
through it. Replace query admission-quanta accounting with actual shared-pool
growth accounting while retaining slots, scratch, query ceilings, and target
partitions.

**REFACTOR.** Delete independent query-pool constructors from Oracle paths;
retain `bounded_memory_pool` only for non-Oracle owners that still need it.
Assert the existing root poison and terminal ownership behavior rather than
adding another cleanup protocol.

### Scenario 3 — Fair, bounded, work-conserving local scheduling

**Behavior.** Local contention preserves the Interactive floor, rotates tenants
equally, uses idle shared capacity, and eventually advances both classes.
Covers REQ-002, REQ-004, REQ-005, AC-003.

**RED.** Replace contender-dependent scheduler tests with
`oracle::admission::tests::local_admission_is_fair_and_work_conserving`. Using
barriers and controlled permit release, queue two tenants in both classes and
assert: per-tenant FIFO; one grant per ready tenant rotation; Analytical units
never enter the Interactive floor; Interactive borrows idle non-protected
units; the oldest eligible class advances after the floor; an over-tenant-cap
head does not block another tenant; queue overflow is retryable; and every
waiter eventually settles. It initially fails because class capacities are
independent and Interactive is always scanned first.

Add
`oracle::admission::tests::follower_release_wakes_waiting_leader_without_reordering`.
Hold the remaining non-protected governor units with follower ownership, queue
two leader tenants, release the follower, and assert notification grants the
cursor-selected leader without polling or changing either tenant's FIFO order.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::admission::tests::local_admission_is_fair_and_work_conserving)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::admission::tests::follower_release_wakes_waiting_leader_without_reordering)'
```

Add
`capacity::two_tenants_make_bounded_progress_across_query_classes`. Through
real authenticated server requests, use
`WyrdTestServer::stall_next_query_after_schema`,
`wait_query_schema_stall`, and its existing release choreography to hold and
release admitted query envelopes. Prove per-tenant FIFO, equal rotation,
retryable overload, Interactive-floor protection, idle-capacity borrowing, and
eventual progress for both classes.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-testing --test oracle -P journey --run-ignored=all \
  -E 'test(=capacity::two_tenants_make_bounded_progress_across_query_classes)'"
```

**GREEN.** Implement fixed local tenant caps and oldest-eligible class choice
inside `OracleAdmission`; remove dynamic ceilings. Make the process governor's
slot-unit ledger the final atomic capacity check for both leader and follower
work. Apply the CPU-and-memory fallback and validation in server boot/config.

**REFACTOR.** Keep scheduling state and transitions in the existing admission
owner. No second semaphore, policy owner, background task, or generalized
weighted scheduler survives.

### Scenario 4 — Bounded Analytical selection and ownership

**Behavior.** One Analytical query reserves and dispatches only its configured
remote-worker subset. Covers REQ-006, REQ-007, AC-004.

**RED.** Add the pure inherent-method test
`oracle::tests::analytical_worker_selection_is_bounded_and_stable` and journey
`analytical_activation::analytical_reserves_only_configured_workers`. Project
the same pinned roster and query ID twice and assert the same two-node subset.
Then start a leader plus three eligible remotes with a worker limit of two and
run one real Analytical attempt; assert only the selected nodes show
slots/memory/cleanup, combined leader-plus-follower Analytical units never
enter the Interactive floor, and an injected refusal on the unselected node
cannot fail the query. Cancel a separate logical query and assert every
confirmed selected owner returns to baseline. Re-run the existing
`selected_peer_failure_is_terminal` journey to preserve the no-successor,
one-attempt contract. They initially fail because current fan-out reserves the
full eligible roster and peers use a separate running semaphore.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::tests::analytical_worker_selection_is_bounded_and_stable)'
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-testing --test oracle -P journey --run-ignored=all \
  -E 'test(=analytical_activation::analytical_reserves_only_configured_workers)'"
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-testing --test oracle -P journey --run-ignored=all \
  -E 'test(=analytical_activation::selected_peer_failure_is_terminal)'"
```

**GREEN.** Select the bounded deterministic subset before reservation and pass
only it to placement/dispatch. Preserve the pinned cut and existing local-only
fallback when no remote is selected.

**REFACTOR.** Keep one selected-worker list as the authority for reservation,
dispatch, metrics, and cleanup; delete parallel all-member iteration.

### Scenario 5 — Production configuration, telemetry, and authority closure

**Behavior.** Operators see local resource controls and signals only; all
architecture and runtime consumers agree. Covers REQ-005, REQ-008, AC-005.

**RED.** Add
`config::tests::oracle_capacity_is_local_cpu_and_memory_bounded` and extend the
existing Oracle metrics contract test. Assert explicit slot override
precedence, CPU/memory fallback, invalid Analytical capacity rejection, fixed
tenant caps from renamed calibration leaves and class-cap defaults, acceptance
of updated calibration schema v2 fixtures and rejection of v1, rejection of the
deleted `cpu_cores` key, bounded metric labels, aggregate
memory/refusal/headroom and worker-count signals, and absence of
delegated/lease/overdraft configuration and metrics. They initially fail
against current translation and registration.

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(=config::tests::oracle_capacity_is_local_cpu_and_memory_bounded)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::tests::oracle_metrics_describe_only_local_capacity)'
```

**GREEN.** Complete configuration/schema/metrics deletion and update
`architecture/bifrost-design.md`, the DataFusion and analytical-reliability
references, and relevant deployment/recovery operations prose to describe only
the implemented local model and its cooperative-memory headroom qualification.

**REFACTOR.** Remove stale comments, test fixtures, generated schema entries,
and policy terminology after consumer search. Do not preserve compatibility
aliases for internal configuration that never shipped as a public contract.

## Cross-scenario decisions and invariants

- Query class still comes only from the single pinned physical root.
- Local tenant limits are equal-weight scheduling caps, not cluster quotas.
- Slot units, actual governed memory, scratch, CPU parallelism, queue bounds,
  and selected-worker count remain separate controls.
- Only fallible DataFusion reservation is hard-limited. Infallible/untracked
  allocation is measured and covered by explicit process headroom.
- A query permit, its query-pool view, all nested reservations, scratch, and
  selected-worker owners settle together on every terminal path.
- Historical admission tables may remain unused; no new migration drops them.

## Expected write set and consumer closure

- `crates/vala/vala-bifrost-redux/src/oracle/admission.rs` — one local leader
  scheduler and fixed tenant caps.
- `crates/vala/vala-bifrost-redux/src/oracle/mod.rs`, `dispatcher.rs`, and the
  existing placement/attempt/execution modules that construct runtimes — delete
  delegated admission, select bounded workers, and use one selected list.
- `crates/vala/vala-bifrost-redux/src/oracle/ownership.rs` — delete after all
  production consumers are removed.
- `crates/vala/vala-bifrost-redux/src/resources.rs` — shared Oracle pool,
  query views, actual reservation accounting, the sole class-aware aggregate
  leader/follower slot ledger, CPU/memory sizing, metrics, and lifecycle tests.
- `crates/wyrd/wyrd-server/src/config.rs`, `boot/mod.rs`, `state.rs`, and
  `oracle/mod.rs` — local config/boot only and no continuity monitor.
- `crates/vala/vala-sql/src/queries/oracle_admission.rs`, its row types,
  exports, and current runtime tests — delete after consumer closure; retain
  historical migrations.
- `crates/wyrd-spec/src/vala/api.rs` and generated artifacts — delete
  `OracleAdmissionScopeKind`, `OracleAdmissionDemand`, and
  `OracleAdmissionContinuityLost`; retain `AuditDetail::OracleAdmissionRecovery`
  only for historical decoding.
- `crates/wyrd/wyrd-testing/tests/bifrost/oracle/capacity.rs` and
  `analytical_activation.rs`, plus existing in-process cluster support —
  deterministic heterogeneous/fairness/fan-out journeys. The heterogeneous
  journey uses two explicit `BifrostNodeSpec` entries with distinct
  `oracle.system_resources`, `WyrdTestCluster::restart_node`, and its shared
  `PgFixture`; it does not call `with_system_resources` or extend the
  child-process harness.
- `crates/wyrd/wyrd-testing/src/server.rs` and the existing
  `wyrd-server/src/state.rs` test-control composition seam — expose the narrow
  test-support memory hold owned by `OracleResources`; production builds have
  no controller or branch.
- Bifrost/DataFusion/analytical reliability and operations authority — local
  capacity, shared cooperative memory, bounded fan-out, and no cluster-quota
  claims.

No new dependency, database migration, public SDK surface, Card contract, UI,
or deployment topology belongs in the implementation diff.

## Verification

Run every named RED command sequentially, then:

```bash
mise run check:bifrost-resource-governance
mise run test:bifrost:integration:redux
mise run test:bifrost:integration:sql
mise run test:bifrost:integration:server
mise run test:bifrost:journey:oracle
mise run codegen:check
mise run verify:bifrost
mise run fmt
mise run lints
git diff --check
```

`mise run gate` is not required because `verify:bifrost` plus the SQL and
codegen lanes cover this bounded cross-owner removal.

## Completion evidence

- Record RED and GREEN results for every exact named test command.
- Record heterogeneous restart and stale-row evidence.
- Record aggregate shared-pool peak/refusal and post-terminal baselines.
- Record deterministic tenant/class grant order and overload projection.
- Record selected versus unselected worker ownership for success and cancel.
- Record static consumer searches proving the durable runtime path is gone
  while historical migrations/decoding remain readable.
- Record focused verification, formatting, linting, codegen, and clean-diff
  results.

## Material stop conditions

- A strict cluster-wide quota or admission guarantee becomes a product
  requirement.
- DataFusion cannot share registered consumers through a query-capped view
  without violating its `MemoryPool` contract.
- Safe local admission requires changing caller-selected query class, auth,
  audit, result, or terminal semantics.
- Historical durable rows cannot remain readable without keeping a production
  admission producer.
- Any condition returns to `$wyrd-spec`; it is not solved by adding a
  coordinator, compatibility path, second task, or new dependency.

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
- `architecture/references/languages/implementation-execution.md`
- `architecture/references/languages/testing-workflows.md`
- `changes/active/oracle-local-admission/spec.md`

## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-001 — PostgreSQL-free admission | `194b37c73`: deleted `DelegatedOracleAdmission`, its maintenance worker, demand/continuity channels, allocation cache, renewal, overdraft, boot policy initialization, and continuity monitor across `oracle/mod.rs`, `oracle/ownership.rs`, `wyrd-server/src/{config.rs,boot/mod.rs,state.rs,oracle/mod.rs}`, `vala-sql/src/queries/oracle_admission.rs`, and `wyrd-spec/src/vala/api.rs`; migrations and `AuditDetail::OracleAdmissionRecovery` retained for historical decoding only | `capacity::heterogeneous_oracles_ignore_historical_admission_rows` (`0d0bcb985`) — seeds conflicting historical policy/allocation rows, queries and restarts two differently sized replicas, asserts per-node snapshots stay distinct with no allocation, renewal, or overdraft activity | PASS |
| AC-002 — Hard aggregate memory qualification | `c218ff967` + `c561f4fca`: `resources.rs` owns one shared tracked pool, per-query views over it, actual reservation accounting, and the sole class-aware aggregate slot ledger; explicit non-DataFusion process headroom stays outside the root | `resources::tests::oracle_queries_share_one_governed_memory_root` (`c218ff967`); `capacity::memory_refusal_preserves_oracle_health_and_next_query` (`b27956d89`) | PASS |
| AC-003 — Local multi-tenant fairness and overload | `c561f4fca` + `5e0651d7c`: `oracle/admission.rs` is one pod-local leader scheduler with fixed equal-weight tenant caps, bounded queueing, retryable overload, and Interactive-floor protection | `oracle::admission::tests::local_admission_is_fair_and_work_conserving` and `oracle::admission::tests::follower_release_wakes_waiting_leader_without_reordering` (`5e0651d7c`); `capacity::two_tenants_make_bounded_progress_across_query_classes` (`4efc5fa92`). Exact per-tenant FIFO ordering stays with the admission unit tests: `max_queue_wait = 250 ms` makes queue order unobservable through the public path, so the journey proves bounded progress for both classes instead | PASS |
| AC-004 — Bounded Analytical participants | `612feefbe`: the participant cut selects the bounded deterministic worker subset before reservation and every consumer takes that one list | `oracle::tests::analytical_worker_selection_is_bounded_and_stable` (`612feefbe`); `analytical_activation::analytical_reserves_only_configured_workers` (`f4f7e7275`); `analytical_activation::selected_peer_failure_is_terminal` (`fe2fb5f8b`) | PASS |
| AC-005 — Operational and architecture closure | `e30e78a07`: CPU/memory-aware local sizing in `wyrd-server/src/config.rs`, bounded-label local saturation metrics, calibration `schema_version` 2 with v1 rejected and no aliases, delegated/lease/overdraft configuration and metrics deleted, and `architecture/bifrost-design.md` plus the DataFusion, analytical-reliability, deployment, and recovery references updated to the local model | `config::tests::oracle_capacity_is_local_cpu_and_memory_bounded`; `oracle::tests::oracle_metrics_describe_only_local_capacity`; `mise run verify:bifrost` — 9/9 lanes passed; `mise run fmt`, `mise run lints`, `mise run codegen:check`, `git diff --check`, and `mise run check:bifrost-resource-governance` all clean | PASS |

### Verification commands

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run verify:bifrost
mise run check:bifrost-resource-governance
git diff --check
```

### Scope

No non-goal was implemented and no unrelated file was changed by the task
commits (`194b37c73`..`fe2fb5f8b`).

Two follow-on commits were required to make `verify:bifrost` green and are
recorded separately because they are not part of the task's write set:

- `bdbaee4c0` deletes three lanes that selected no tests
  (`test:bifrost:journey:otlp` over an emptied binary, `test:sql:forge-scale`
  whose only test was erased with the legacy Forge compaction route, and a
  `wyrd-mcp` selector in `unit:rust` that matched nothing). Each exited 4 on
  every run.
- `c3db2fc9b` fixes a pre-existing production defect the task's shared query
  journey exposed: the Forge scheduler deferred its first tick a whole
  maintenance interval while `/readyz` gates the coordinator bit on a completed
  planning pass, so a freshly started pod advertised itself unready for a
  minute after every restart.
