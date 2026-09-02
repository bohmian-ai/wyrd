---
id: BIFROST-R4-T01-R01-GRAPH-LEASE-REMEDIATION
title: Close follower GraphLease readiness and deadline lifecycle gaps
kind: remediation
mode: REMEDIATE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 4
depends_on: [BIFROST-R4-T01-GRAPH-LEASE]
requirements: [REQ-004, REQ-007, REQ-009, REQ-011]
invariants: [INV-002, INV-003, INV-004, INV-005]
acceptance: [AC-001, AC-004, AC-005, AC-007]
parent_task: BIFROST-R4-T01-GRAPH-LEASE
reviewed_base: 25a3aa94e7bb782c21d41e7c5da3c0e97f8ea77a
reviewed_candidate: 11b85cddb2489f75f918a559d5b4729320ade775
remediates:
  - FIND-T01-001
  - FIND-T01-002
  - FIND-T01-003
---

# Follower GraphLease review remediation

## Outcome and value

A follower that cannot cleanly settle a graph stops advertising production
readiness, and a graph activated by `ExecuteTask` cannot retain its envelope
past the signed absolute deadline while waiting for `SetPlan`. Both conditions
reuse the GraphLease owners and the one joined settlement driver delivered by
Task 1. The task also applies the three independently reviewed ponytail
reductions: justify the existing Clippy allow instead of adding a type alias,
delegate `nested_idle` to `nested_debt`, and remove the redundant inner `Arc`
from the graph exchange registry.

Required execution skill: `$wyrd-implement`.

## Review authority and retained implementation

The immutable reviewed candidate is
`11b85cddb2489f75f918a559d5b4729320ade775` over base
`25a3aa94e7bb782c21d41e7c5da3c0e97f8ea77a`. Retain its verified binding,
two-phase activation, one `AnalyticalGraphEntry` map, `GraphLease::settle`, one
ingress-owned bounded settlement channel and joined driver, per-graph worker,
and graph-owned exchange streams. This task corrects only the three validated
findings and the two requested behavior-preserving reductions; it does not
reopen the Task 1 design.

Primary owners:

- `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs`:
  GraphLease lifecycle, follower live state, execution health, and the single
  settlement driver.
- `crates/vala/vala-bifrost-redux/src/oracle/mod.rs`: production Oracle
  readiness and its direct admission/forwarding consumers.
- `crates/vala/vala-bifrost-redux/src/oracle/analytical_transport.rs`:
  `SetPlan`/`ExecuteTask` transport lifetime and graph-owned exchanges.
- `crates/vala/vala-bifrost-redux/src/resources.rs`: query-envelope nested debt
  and idle inspection.

Affected consumers are Oracle admission, forwarding, boot and health probes,
both governed stage operations, follower shutdown, and the existing Oracle
journeys. Do not change public or private wire schemas, tickets, reservation
TTL, query deadlines, retry policy, routing, or deployment topology. Do not add
a deadline registry, task per graph, detached task, readiness type, health
probe, configuration knob, type alias, error wrapper, or dependency.

## Ordered implementation scenarios

### Scenario 1 — Retained cleanup failure fails production readiness

**Behavior.** A normal active graph does not make the node unready. A retained
`Draining` graph with `settlement_failure: Some(_)` makes
`AnalyticalExecutionHandle::is_healthy` and the existing production
`Oracle::is_ready` return false even while startup is reconciled and admission
has capacity. An error reading follower live ownership also fails health
closed. Maps REQ-007, REQ-009, REQ-011, INV-003, INV-004, INV-005, AC-001,
AC-004, AC-007 and remediates FIND-T01-001.

**RED.** Add
`oracle::tests::analytical_cleanup_failure_fails_production_readiness` in the
existing Oracle test module. Compose the existing production Oracle owners with
Analytical enabled; establish startup and admission readiness; activate one
follower graph and assert the active graph leaves `Oracle::is_ready()` true.
Retain one real nested memory child, drive the graph through the production
settlement path so it becomes `Draining` with one cleanup failure, and assert:

- startup remains reconciled and admission remains available;
- follower `cleanup_failures == 1`;
- `AnalyticalExecutionHandle::is_healthy()` is false; and
- `Oracle::is_ready()` is false.

The test initially fails because production readiness never reads follower
cleanup failure state. Exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::tests::analytical_cleanup_failure_fails_production_readiness)'
```

**GREEN.** Change `AnalyticalExecutionHandle::is_healthy` to require both the
existing supervisor health and a successful `worker.live()` snapshot whose
`cleanup_failures` is zero. Do not use `AnalyticalLiveOwnership::is_clean`:
live, healthy graphs and attempts are allowed during service. Change
`Oracle::is_ready` to retain its current startup and admission predicates and
also require the optional Analytical handle to be healthy when it is present;
an Oracle composed without Analytical remains governed by the existing two
predicates. All server consumers already call `Oracle::is_ready`, so add no
parallel readiness state or server-specific check.

**REFACTOR.** Readiness remains a synchronous projection of existing owners.
Do not mutate admission, cancel healthy graphs, or convert cleanup failure into
a new public error merely to make readiness false.

### Scenario 2 — ExecuteTask-first activation settles at its signed deadline

**Behavior.** The first valid `ExecuteTask` may still activate a graph before
`SetPlan`, but that graph remains bounded by the exact
`GraphLeaseBinding::absolute_deadline_ms`. If `SetPlan` arrives, its existing
connection lease handles coordinator disappearance. If it never arrives, the
single ingress driver transitions the graph to `Draining` at the signed
deadline, cancels it, runs the existing joined settlement path, and releases
all graph ownership. The normal end of the unary `ExecuteTask` request is not a
departure signal because upstream may continue producing rows after consuming
that request body. Maps REQ-004, REQ-007, REQ-009, INV-002, INV-003, INV-004,
INV-005, AC-001, AC-004, AC-005 and remediates FIND-T01-002.

**RED.** Add
`oracle::analytical_transport::tests::execute_task_first_without_set_plan_settles_at_signed_deadline`.
Drive a valid `ExecuteTask` through `AnalyticalStageAuth` and the production
ingress with a near future signed deadline, allow the unary request to finish,
and never open a coordinator channel. Before the deadline, assert the graph is
still active and owns its envelope, proving request-body drop did not release
valid work. First give the same driver a separate graph whose deterministic
child gate holds settlement open beyond the target graph's deadline. Under a
bounded test timeout, wait past the target's signed deadline and assert that it
still transitions, cancels, and then releases its graph map, supervisor graph,
reservation, query memory and scratch, graph worker, and exchange ownership
while the blocked sibling remains owned. Finally release the sibling gate and
join it. Assert one cancellation outcome per graph and no detached cleanup
task. It initially fails because the driver waits only for explicit settlement
messages and then settles them serially, while no `SetPlan` connection exists
to signal the target graph.

In the same current-thread test, stage the bounded-channel scheduling case
without yielding to the spawned driver: for every permitted graph, enqueue its
activation wake and then its one terminal settlement command. Assert all
`2 * max_concurrent_graphs` commands fit and no valid settlement becomes a
retained queue-full cleanup failure. This fails if the new wake command reuses
the old one-command-per-graph capacity. Exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical_transport::tests::execute_task_first_without_set_plan_settles_at_signed_deadline)'
```

**GREEN.** Keep the absolute deadline only in the existing `GraphLeaseBinding`
and expose it to `AnalyticalStageIngress` through one private accessor. Extend
the existing bounded settlement channel with one closed command enum containing
only `Wake` and `Settle(GraphSettlement)`; do not create another channel. Size
that same channel with a checked or saturating
`2 * ReservationRegistry::max_concurrent_graphs()` derived bound: at most one
activation wake and one terminal settlement command can be outstanding per
admitted graph. Add no setting or second capacity root.
Immediately after inserting a newly activated `Active` graph, send `Wake`.
A full channel is already a wake condition and is therefore not a lost
deadline signal; a closed channel transitions the just-published graph to
`Draining` with a cleanup failure and refuses the stage instead of leaving an
unobservable active lease.

Extend `drive_graph_settlements`, still the ingress's sole joined async driver,
to select between the current receiver and the earliest absolute deadline read
from the existing graph map, plus completion of settlements the driver already
owns. On every wake, settlement completion, or timer expiry, rescan only
`Active` entries. When one or more deadlines are due, move each due entry from
`Active` to `Draining` under the existing graph mutex and insert its existing
`GraphLease::settle(Cancelled)` future into one driver-local
`futures_util::stream::FuturesUnordered`. Explicit caller-drop and shutdown
settlements enter that same set. Completion calls the existing
`record_settlement` path. This is concurrency inside the one joined driver, not
a spawned task per graph, so a slow cleanup cannot prevent the driver from
observing or cancelling another graph at its deadline. Never hold the graph
mutex across the timer wait or settlement await. Keep the deadline scan linear
because `ReservationRegistry::max_concurrent_graphs()` already bounds its size;
add the required `ponytail:` comment naming that ceiling and a priority queue
as the upgrade only if profiling later justifies it. Use no new dependency;
`futures-util` is already owned by this crate.

Existing `SetPlan` connection drop and normal terminal/shutdown paths continue
to enqueue `Settle` exactly once after the same `Active` to `Draining`
transition. A unary `ExecuteTask` completion does not enqueue settlement.

**REFACTOR.** The graph map remains the sole lifecycle/deadline registry, and
`GraphLease::settle` remains the sole release order. Do not add a per-graph
sleep, timer handle, cancellation watcher, request lease, or second state
machine. If Rust needs one stable future type for the driver-local set, use one
narrow private stateless async settlement helper; do not introduce a service,
trait, or public type.

### Scenario 3 — The modified publish boundary passes the allow audit

**Behavior.** The materially modified `publish` function uses the repository's
sanctioned Clippy-allow format. Maps the repository completion gate and
remediates FIND-T01-003 without changing runtime behavior.

**RED/static proof.** Run `mise run check:clippy-allow-audit`; the reviewed
candidate is named because `publish` has no immediately preceding
`// justification:`. The repository currently also reports unrelated baseline
violations outside this task's write set, so closure requires that the command
no longer name `oracle/analytical.rs`; it does not require this remediation to
edit those unrelated files.

**GREEN.** Add exactly one `// justification:` line immediately before the
existing `#[allow(clippy::type_complexity)]`, explaining that the private error
must return the rollback-owning `PendingGraphActivation` together with the
activation error. Keep the existing result type. Do not add a one-use alias,
wrapper, trait, or allow.

**REFACTOR.** The comment documents this exact boundary only and must not
justify future unrelated complexity.

### Scenario 4 — Remove duplicate and redundant ownership wrappers

**Behavior.** Nested-idle inspection and graph exchange closing retain exactly
their current observable behavior with fewer ownership layers. This is a
requested ponytail reduction and changes no approved contract.

**RED/existing proofs.** Before refactoring, keep the Scenario 1 retained-child
assertions green and run the existing exchange close proof:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical_transport::tests::a_graph_closes_an_exchange_its_consumer_stopped_polling)'
```

These proofs must remain green after both reductions; no new test is required
for the one-line delegation or removal of an ownership wrapper.

**GREEN.** Implement `OracleQueryResources::nested_idle` solely as
`self.nested_debt().map(...)`, returning true only when both returned values are
zero. This preserves the same scratch lock and memory snapshot while deleting
the duplicated lock/error block. In `AnalyticalGraphExchanges`, store the
registry as `std::sync::Mutex<Vec<ExchangeSlot>>` rather than
`Arc<std::sync::Mutex<Vec<ExchangeSlot>>>`; all long-lived consumers already
share the owning `AnalyticalGraphExchanges` through `Arc`, while each
`ExchangeSlot` remains shared between graph and consumer exactly as today.

**REFACTOR.** Do not change the mutex type, exchange polling protocol, close
ordering, poison behavior, `ExchangeSlot`, or graph-level `Arc` ownership.

## Cross-scenario decisions and authority

Cleanup failure and deadline expiry converge on the same existing `Draining`
state and settlement order. A graph is never declared released until all
descendants join; a failed settlement remains owned and makes readiness false.
The signed wall-clock deadline is immutable authority already present in the
lease, not a new setting. The driver is node-owned and joined at shutdown, and
no async work may outlive it.

Authority:

- `AGENTS.md`, especially sections 5, 6, 11, 12, and 15.
- `architecture/wyrd-design.md` and `architecture/wyrd-doctrine.mdx`.
- `architecture/bifrost-design.md`, especially distributed execution's one
  immutable deadline/cancellation tree and joined cleanup rules.
- `architecture/wyrd-security-posture.md` for verified signed deadlines and
  pre-IO stage authority.
- `architecture/operations/reliability-and-recovery.md` for deadline,
  cancellation, readiness, and shutdown evidence.
- `architecture/references/domain/datafusion.md` and
  `architecture/references/domain/analytical-operations-reliability.md`.
- `architecture/references/languages/rust-core.md` and
  `architecture/references/languages/testing-workflows.md`.

## Broader verification

Run each named test at its scenario's exact command, then:

```bash
mise run fmt
mise run lints
mise run test:bifrost
mise run test:bifrost:journey:oracle
mise run check:bifrost-resource-governance
mise run check:clippy-allow-audit
git diff --check
```

The Clippy allow audit must not name a file in this task's write set. If its
known unrelated baseline violations remain, record their exact paths and the
non-zero aggregate result rather than expanding this remediation or claiming a
pass. No contract, proto, generated artifact, client-tier, or PyO3 surface is
in the write set, so their checks are not required for this bounded task.

## Completion evidence

- The production Oracle readiness test proves an active graph remains ready
  and a retained follower cleanup failure alone turns the same Oracle unready.
- The transport-lifecycle test proves `ExecuteTask`-first work survives normal
  unary request completion but settles no later than its signed deadline when
  no `SetPlan` channel arrives.
- Driver inspection proves one bounded channel, one joined driver, no graph
  timer task, no detached cleanup, capacity for one wake plus one settlement
  per admitted graph, and the existing graph map as the only deadline
  inventory.
- The allow audit no longer reports `oracle/analytical.rs`.
- `nested_idle` delegates to `nested_debt`, the exchange registry has no inner
  `Arc`, and the retained-child and stopped-consumer proofs remain green.
- Broader Bifrost tests and the production Oracle journey lane pass, apart from
  any explicitly recorded pre-existing aggregate allow-audit violations outside
  this task's write set.

## Stop conditions

Return `SPEC_REVISION_REQUIRED` if remediation would weaken signed deadline or
stage authority, extend the reservation or query deadline, report failed
cleanup as release, alter retry behavior, or add/change a wire field. Return
`PLAN_BLOCKED` if the existing single driver cannot observe a newly activated
graph without another async owner; the bounded channel and graph map provide
that wake and inventory today, so no block is known.
