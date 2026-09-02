---
id: BIFROST-R4-T03-PHYSICAL-BASELINE
title: Qualify the minimal cross-process Analytical operator baseline
kind: implementation
mode: RECONCILE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 4
depends_on: [BIFROST-R4-T02-QUERY-ENVELOPE]
requirements: [REQ-003, REQ-005, REQ-007, REQ-009, REQ-011]
invariants: [INV-002, INV-003, INV-004, INV-005, INV-007, INV-008]
acceptance: [AC-001, AC-002, AC-004, AC-005, AC-007, AC-008]
parent_task: BIFROST-R3-T1-INACTIVE-CLOSEOUT
frozen_candidate: f1ac4cb01fe9ddda0a133cb58c955bab1e1cf7df
---

# Minimal physical Analytical baseline

## Outcome and value

The inactive Analytical engine correctly executes the minimal cross-process
baseline admitted by Task 2's closed predicate using the pinned DataFusion
universe and produces real, bounded production evidence. A
three-Oracle/one-Scribe journey proves remote
filtered/projected scans, multi-input equi-join, approved grouping, streamed
exchange, one DataFusion spill, exact results, joined cleanup, and private peer
topology. This task does not broaden compatibility or activate public routing.

Required execution skill: `$wyrd-implement`.

## Current-state amendment

Retain Task 2's closed supported-plan predicate and the current codec,
analytical scan, distributed executor, participant cut, mTLS transport,
process-cluster harness, and existing partial physical journeys. Replace the
follower join rejection and any synthetic/manual spill or test-only telemetry
evidence. Remove the old one/two/three/six-replica matrix; use the smallest
topology that produces two remote followers and a real network stage.

## Owners, scope, consumers, and non-goals

Owners are `exec.rs`, `analytical_scan.rs`, `codec.rs`, the existing DataFusion
distributed planner/worker adapter, `oracle/telemetry.rs`, `resources.rs`,
`spill.rs`, and `wyrd-testing`'s Oracle process-cluster journey. This task
consumes Task 2's closed supported-plan predicate without modifying it. Task 4
consumes the qualified handle and Task 2 predicate.

Do not add windows, correlated subqueries, deduplicating sets, UDAFs, additional
join families, a custom shuffle, a second optimizer, a dependency patch, or
production activation.

## Ordered implementation scenarios

### Scenario 1 — Correct remote join and grouped aggregation

**Behavior.** Distinct Oracle processes execute remote pushdown, equi-join, and
partial/final fixed-width aggregation, producing the trusted local result.
Maps REQ-003, INV-002, INV-003, AC-002.

**RED.** Extend the existing failing inactive analytical journey as
`peer_network::analytical::inactive_baseline_executes_remote_join_and_grouping`.
Seed asymmetric inputs so leader-local or single-input execution cannot pass;
assert child PID/node/fence scan evidence, pushdown row counts, positive exchange
bytes/batches, and exact Arrow result equivalence. Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=peer_network::analytical::inactive_baseline_executes_remote_join_and_grouping)' --run-ignored=all"
```

**GREEN.** Extend the follower codec/provider construction to support the
approved join and partial/final aggregate plan nodes under Task 1's installed
runtime and exact assignment. Preserve schema-by-name mapping, immutable source
cut, tenant tripwire, and streamed exchange; never collect the complete shuffle
or user result at the coordinator.

**REFACTOR.** Shared physical decoding stays in the existing codec/executor
owners; do not build a second follower SQL planner.

### Scenario 2 — Real DataFusion spill and cleanup

**Behavior.** A real spill-capable DataFusion operator spills through the
admitted query scratch allocation, produces the complete correct result from
that execution, then removes its files before success.
Maps REQ-003, REQ-005, REQ-007, INV-004, INV-005, AC-002, AC-004.

**RED.** Add
`peer_network::analytical::inactive_baseline_uses_and_cleans_real_operator_spill`.
Use enough deterministic data and a low admitted memory ceiling to force the
approved `SortExec` spill. Assert DataFusion's public `spill_count`,
`spilled_bytes`, and `spilled_rows` metrics are positive, the complete output
equals the trusted result, and the graph scratch directory returns to baseline.
A manual temp file, synthetic metric, or nonexistent spill-read metric must not
satisfy the test. Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=peer_network::analytical::inactive_baseline_uses_and_cleans_real_operator_spill)' --run-ignored=all"
```

**GREEN.** Ensure the follower `RuntimeEnv` uses Task 2's scratch manager and
pool, configure the qualification fixture through existing finite test-support
inputs, and fold actual DataFusion spill metrics during joined settlement.
Success waits for scratch cleanup verification; exhaustion is a failed terminal.

**REFACTOR.** Test support may size inputs/capacity but cannot emit production
spill events or touch spill files directly.

### Scenario 3 — Physical telemetry and zero ownership

**Behavior.** The physical journey observes bounded production metrics for
actual fan-out, memory current/peak, exchange, and spill; all physical
active/current gauges return to baseline after the successful qualified run.
Maps REQ-011, INV-004, INV-005, AC-001, AC-004, AC-007.

**RED.** Add
`peer_network::analytical::inactive_baseline_emits_bounded_telemetry_and_returns_to_zero`.
Install the normal production recorder, checkpoint before/after, inspect
descriptor label keys, and assert positive physical deltas plus zero residue on
success. Task 4 owns routing, cancellation, peer-loss, terminal, audit, and
shutdown telemetry. Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=peer_network::analytical::inactive_baseline_emits_bounded_telemetry_and_returns_to_zero)' --run-ignored=all"
```

**GREEN.** Keep `OracleTelemetry`, the resource pool, and lease settlement as
the only producers. Measure pool current/peak from the real pool and fan-out
from the actual addressed participant set. IDs/SQL/plan text remain scrubbed
trace fields, never labels. Remove retry series.

**REFACTOR.** Do not add a telemetry registry, exporter, polling owner, or
test-only production hook.

### Scenario 4 — Private interchangeable process topology

**Behavior.** The same Oracle process shape coordinates and follows; worker and
lifecycle services exist only on the private authenticated listener and join at
shutdown. Maps REQ-009, AC-005, AC-008.

**RED.** Extend the physical journey to probe public/private listeners and swap
the coordinating Oracle; assert both can coordinate/follow and public probes
cannot reach peer services. The named tests in Scenarios 1–3 carry this proof.

**GREEN.** Wire no new listener. Close only missing server target/readiness/
shutdown composition through existing `BifrostTarget::serves_peer`,
`app::peer_plane`, and Oracle build owners. Extend the checked-in deployment
contract fixtures consumed by `wyrd-testing::bifrost::deployment_contract` so
self-hosted, SaaS, and enterprise profiles express the required private bind,
mTLS identity, target, readiness, and shutdown composition; keep
`ForgeWorker` free of the peer listener. `mise run check:bifrost-oracle-deploy`
is the static deployment proof, not full release/SLO certification.

**REFACTOR.** Keep `Interactive`/`Analytical` as execution paths, never target or
pod-role selectors.

## Broader verification

```bash
mise run fmt
mise run lints
mise run test:bifrost
mise run test:bifrost:journey:oracle
mise run test:bifrost:journey:server
mise run check:bifrost-oracle-deploy
mise run check:bifrost-resource-governance
mise run check:object-store-pin
git diff --check
```

## Completion evidence

- Child process/node/fence scan and exchange evidence plus exact result parity.
- Positive DataFusion spill count/bytes/rows, complete result parity, and
  verified scratch cleanup.
- Production metric deltas/label inventory and zero owner snapshots.
- Private listener/readiness/shutdown evidence in the three-Oracle/one-Scribe
  topology only.

## Stop conditions

Return `SPEC_REVISION_REQUIRED` if the baseline needs a dependency fork,
materialized shuffle, new planner/scheduler, broader operators, test-synthetic
physical evidence, or weakened tenant/terminal semantics.
