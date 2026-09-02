# Bifrost distributed analytics engine v1

Status: Review Required
Repository origin: github.com/bohmian-ai/wyrd
Repository revision: 3978aa7a2bbe49299fc8cd50e36c2e3c8e6e9d09
REPO_ROOT: $REPO_ROOT
PLAN_PATH: $PLAN_PATH
Created: 2026-08-23
Last updated: 2026-08-31
Plan version: 7
Evidence snapshot: Wyrd 3978aa7a2bbe; current Oracle, peer, resource, test-harness, SDK, MCP, manifest, lockfile, and mise owners inspected; target repository worktree clean
Review: required

## Objective

Add genuine distributed `Analytical` execution behind Oracle's one public
raw-SQL contract while preserving the default `Interactive` path, tenant
isolation, governed storage, the protected Interactive resource floor,
WAL-before-rows audit, bounded streaming, cancellation, and complete shutdown.
Prove the feature through real Rust, HTTP, generated-gRPC, Python, TypeScript,
and MCP journeys that exercise intended distributed behavior, failures,
cancellation, and cleanup.

## Current state and evidence

The inspected revision already has the production ingress
`wyrd_spec::vala::api::BifrostQueryRequest` →
`wyrd_server::query::service::stream_query` →
`vala_bifrost_redux::oracle::Oracle::query_sql`. `Oracle` owns immutable cuts,
Interactive scatter/gather, admission, query resources, cancellation, terminal
streaming, and audit coordination. `OraclePeerRuntime`, `OraclePeerGrpc`,
`OraclePeerAuthority`, `OraclePeerWorker`, `TonicOraclePeerTransport`, signed
peer tickets, replay protection, and the six-node `WyrdTestCluster` already
provide private peer and lifecycle seams. They are extended; no second listener,
scheduler, service, storage owner, or audit path is introduced.

`datafusion-distributed` 4.0.0 is pinned in the single DataFusion 55.0.0 and
Arrow/Parquet 59.2.0 cone, but current use is compatibility coverage rather
than production Oracle execution. Current classification uses `QueryClass` in
places where the final design requires `QueryExecutionPath` to own routing and
admission; `QueryClass` may remain only as telemetry.

The current public query clients exist in `vala-sdk`,
`python/py-wyrd/python/wyrd/bifrost`, `typescript/wyrd`, the N-API binding, the
public gRPC service, and MCP. Existing Bifrost journey lanes select whole Cargo
targets and therefore fail on missing targets instead of silently selecting
zero tests.

Current Oracle production instrumentation is owned by `OracleTelemetry` in
`vala-bifrost-redux`. It emits through the recorder installed by the server,
while `WyrdTestCluster` exposes the same production `BifrostTelemetryCapture`
for deterministic test inspection. This is the existing Scribe/Forge pattern:
tests consume production telemetry rather than emitting surrogate test events.

Focused baseline evidence recorded by the readiness reviewer:

- `mise run check:object-store-pin`: pass.
- `mise run check:bifrost-oracle-deploy`: pass.
- `mise run check:bifrost-resource-governance`: pass.
- the plan-artifact validator passed for the previously reviewed artifact;
  version 7 requires fresh validation after narrowing completion evidence to
  behavioral verification.

Authority and selected expertise:

- `AGENTS.md`, `architecture/agent-rules.md`,
  `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and
  `architecture/bifrost-design.md` govern every task.
- `architecture/references/languages/implementation-execution.md` fixes
  implementation authority, bounded adaptation, test integrity, and evidence.
- `rust-core.md`, `datafusion.md`, `olap-serving.md`, `iceberg.md`, and
  `analytical-operations-reliability.md` fix owners, async boundaries, query
  runtime resources, exchange/spill behavior, and recovery.
- `wyrd-security-posture.md` fixes peer trust, tenant isolation, and audit.
- `testing-workflows.md`, `python-api-and-stubs.md`, `typescript-guide.md`, and
  `agent-harness.md` fix journey and runtime-boundary proof.
- `telemetry-observations.md` fixes trace correlation, production-owner
  instrumentation, bounded metric labels, and payload safety.
- `operations/README.md` fixes Oracle readiness and supported topology.

## Requirements

- R1. One public raw-SQL request selects Interactive or Analytical inside
  Oracle; callers cannot submit a physical plan, path, class, or stage graph.
- R2. Analytical execution performs streamed network exchange and actual
  follower joins, partial/final aggregates, partitioned windows, supported
  decorrelated subqueries, and deduplicating sets over the pinned cut.
- R3. Every stage is authorized before decode/cache/provider/IO and binds the
  exact tenant, distinct public-query and DataFusion-query identities,
  snapshot, fragment, stage, task, typed attempt, reservation, peer fence,
  deadline, and permission authority.
- R4. Oracle owns one structured lifecycle and one query-owned runtime,
  memory/exchange/scratch grant, cancellation tree, retry boundary, audit
  acceptance, terminal, readiness state, and shutdown inspection.
- R5. Optimized-plan routing uses only exact immutable post-pruning facts,
  requires a real exchange, preserves the Interactive floor, and fails safely
  for unknown or unsupported facts.
- R6. Rust, HTTP, generated gRPC, Python, TypeScript, and MCP project one typed
  contract, stable errors, deadline range, EXPLAIN result, and terminal
  execution evidence.
- R7. Each code-producing task follows the packet's named RED → smallest GREEN
  → required mutation RED → restored GREEN slices and real user journeys.
- R8. `OracleTelemetry` instruments every Interactive and Analytical hot path
  with production traces, structured logs, and bounded-cardinality metrics;
  unit, integration, and journey tests inspect those same production signals
  through the existing server/test-cluster telemetry capture.

## Non-goals

- Materialized shuffle, a second scheduler/service/listener, query jobs,
  checkpoint/resume, cross-region execution, distributed writes, or result
  caching as correctness.
- A public path/class/plan hint or client-owned durable
  routing/admission/lifecycle logic.
- Migration, backfill, compatibility aliases, old-node negotiation, or parallel
  dependency universes; Bifrost is unshipped and existing definitions move
  directly to the final contract.
- Performance measurement, comparison, thresholds, historical capture, or
  publication; these are deferred to separate future work.

## Constraints

- `wyrd-server` remains the only serving surface; Vala owns Oracle execution.
- `wyrd-spec` remains IO-, async-, and PyO3-free; client tiers cannot acquire
  DataFusion, SQLx, cloud SDKs, or durable server behavior.
- DataFusion 55.0.0, Arrow/Parquet 59.2.0, Iceberg, and the pinned
  `datafusion-distributed` revision remain one native dependency cone.
- Authentication and signed authority precede bounded typed decode, task-cache
  access, provider creation, resources, storage, or execution.
- Analytical cannot borrow the configured Interactive floor. Exchange buffers
  are a child of the query grant; follower spill uses query-owned scratch.
- One authenticated pre-egress availability failure may retry once on the same
  cut and absolute deadline after the old attempt is fully joined and released.
  No other failure retries.
- One logical query fsyncs one versioned CRC-framed acceptance before rows;
  stages emit no read audit. Successful EXPLAIN emits no read acceptance.
- Generated artifacts are regenerated from owners, never edited by hand.
- All new or materially modified Rust items and tests receive complete rustdoc.
- Oracle instrumentation extends the existing `OracleTelemetry` owner and
  installed production recorder. No Analytical-only telemetry owner, test-only
  counter/event path, or production dependency on `wyrd-testing` is allowed.
- Tenant, table, SQL, request, public/private query, stage, task, attempt, and
  object identities are scrubbed trace/log fields, never metric labels.

## Architecture and design decisions

### D1: Extend the existing Oracle peer runtime

`AnalyticalExecutionHandle` is an Oracle-owned, dependency-owning concrete
handle composed from the existing peer transport/worker, query resources,
governed `BifrostStorage`, stage authority, and an `AnalyticalSupervisor`. It is
built inactive in T1 and injected/activated only by T2. V1 uses the immutable
upstream `datafusion-distributed` pin through supported public APIs. It does not
fork, vendor, locally patch, or abstract the dependency to obtain an exact
per-message exchange-memory guarantee.

### D2: Separate execution path from telemetry class

`QueryExecutionPath { Interactive, Analytical }` owns routing, admission,
capacity policy, persisted query-path facts, EXPLAIN, and terminal evidence.
Any retained `QueryClass` is telemetry-only. Existing unshipped schema sources,
queries, fixtures, checks, and generated artifacts change directly.

### D3: Route conservatively and fall back only before selection

`OracleExecutionRouter` is synchronous and receives the optimized plan, pinned
cut, exact `OracleRoutingFacts`, and prospective Interactive grant. Missing,
inexact, incompatible, mutable, overflowing, live-tail, or unsupported facts
select Interactive. Distributed planning failure or a candidate with no real
exchange may fall back before Analytical admission. Engine or resource loss
after selection is a stable terminal failure, never successful fallback.

### D4: Bind private stage authority to the body and attempt

mTLS authenticates the peer. Existing signed peer authority is extended with a
versioned, domain-separated stage operation that binds RPC kind, peers/fences,
tenant, distinct `PublicQueryId` and `DataFusionQueryId`, snapshot,
fragment/request digest, stage/task/`AnalyticalAttemptNumber`, reservation,
permission digest, nonce, deadline, and expiry. Oracle owns the mapping:
`PublicQueryId` begins with the authenticated client-visible lifecycle;
`DataFusionQueryId` is allocated only after an exchange-bearing distributed
physical plan is validated and Analytical selection becomes irreversible. A
planning/no-exchange fallback allocates no DataFusion identity. Set-plan and
execute-task have distinct audiences/nonces/digests. A first bounded typed
message may establish a stream capability only after metadata authentication
and body-digest verification; it is not authentication.

### D5: Supervise and release one exact attempt

`AnalyticalSupervisor` owns drivers, tasks, exchanges, metrics collectors,
leases, and joins by `(PublicQueryId, DataFusionQueryId, stage, task?,
AnalyticalAttemptNumber)`. Every exit performs:
invalidate exact attempt → cancel descendants → close senders → join all work →
release leases/resources once. Public cancellation resolves the public query to
its one active private graph. Retry preserves both query IDs, plan digest, cut,
and deadline and creates attempt one only after attempt zero is fully drained
and before any result-data frame escaped.

### D6: Reuse production Oracle telemetry as the test diagnostic harness

`OracleTelemetry` remains Oracle's one concrete production instrumentation
owner across Interactive and Analytical execution. Instrument authentication,
admission and queueing, cut/planning/routing, pruning and scans, stage dispatch,
peer work, exchange, spill, retry, cancellation, terminal settlement, and
resource release at the owner performing each effect. Emit structured tracing
spans/events and logs with scrubbed correlation fields, plus counters,
histograms, and balanced gauges with closed bounded labels.

The server installs the recorder/subscriber and `WyrdTestCluster` exposes its
existing production `BifrostTelemetryCapture`. Tests take scoped baselines and
assert signal deltas from real production paths. A test-only metrics registry,
Analytical-only telemetry object, surrogate event emission, or assertion solely
against internal state is not equivalent proof. Production packages never
depend on `wyrd-testing`.

## Domain and data contracts

The public contracts are owned by `wyrd-spec::vala::api`: raw query and EXPLAIN
requests, `QueryExecutionPath`, closed route/fallback reasons, bounded stage
DAG, terminal execution evidence, and derive-backed stable errors. Public
deadline is `1..=u32::MAX` milliseconds on every surface.

The internal execution contract binds `AuthorizedQueryContext`, distinct
`PublicQueryId` and `DataFusionQueryId`, immutable `OracleQueryAttemptCut`,
optimized logical/physical plan digests, exact routing facts,
`OracleQueryResources`, runtime and scratch leases, stage authority, and
post-drain `AnalyticalExecutionEvidence`. Evidence and every private authority,
cache, exchange, frame, invalidation, trace, and cancellation key carry both
query identities. Evidence identifies operator/stage role, leader/follower
locality, input/output rows and batches, exchange bytes, spill files/bytes,
typed attempt, and outcome. A planned stage, route label, remote scan, or
EXPLAIN projection is not execution evidence.

## Interfaces and function contracts

T1 fixes compile-shaped inactive owners and private transport extension points.
T2 fixes the router, final public DTOs, Oracle activation, HTTP/gRPC EXPLAIN,
admission, and errors. T3 fixes runtime projections and cancellation state
machines. Exact signatures and caller-to-owner flow are binding in each task;
private spelling may adapt only to a current equivalent without changing the
owner, input, output, ordering, or proof seam.

## Control flow and pseudocode

```text
public query/EXPLAIN
  -> authenticate principal and bind tenant/request
  -> authorize SQL/projection/sensitive columns
  -> validate deadline and read-only/query bounds
  -> pin one immutable Oracle cut
  -> optimize against governed providers and exact post-pruning facts
  -> route(optimized plan, cut, exact facts, interactive grant)
  -> if Interactive: admit Interactive and execute existing path
  -> if Analytical candidate:
       distributed-plan; require real streamed exchange
       allocate DataFusionQueryId for this exact physical plan
       admit Analytical under aggregate root and Interactive floor
       fsync one logical read acceptance (query only)
       execute through AnalyticalExecutionHandle
       authenticate/authorize each stage before decode/cache/IO
       install query-owned runtime/memory/exchange/spill/cancellation/deadline
       supervise all descendants and stream bounded frames
       retry once only for authenticated pre-egress availability loss
  -> validate one terminal and post-drain evidence
  -> relay audit at least once and release every owner exactly once
```

## Failure and edge-case matrix

| Condition | Required result | Owner/evidence |
|---|---|---|
| Missing/inexact/overflowing routing fact | Interactive with typed reason | T2 router unit + journey |
| Distributed plan fails or has no exchange | pre-admission Interactive fallback | T2 route/EXPLAIN journey |
| Engine/resource lost after selection | stable terminal error; no fallback | T2 public journey |
| Ticket/auth/body/nonce/fence mismatch | reject before decode/cache/provider/IO | T1 private transport tests |
| Public-query or DataFusion-query mismatch | reject before decode/cache/provider/IO; no sibling cache/frame/cancel access | T1 auth, replay, retry, stale-frame, cancellation tests |
| First authenticated peer availability loss before egress | drain attempt 0, one retry on same cut/deadline | T1/T2 journey |
| Second loss or any post-egress loss | failed terminal, no partial success | T2 journey |
| Slow consumer/cancel/deadline | bounded backpressure, joined descendants, one release | T1/T2/T3 journeys |
| Tenant mismatch | whole-query tripwire failure | T1/T2 journey |
| Missing or malformed terminal | incomplete-stream error in each runtime | T2/T3 runtime tests |

## Milestones

- M1 — Inactive engine feasibility: T1 proves the dependency/extension gate,
  authenticated stage execution, query-owned runtime, supervision, real
  follower operators, and cleanup without changing production routing.
- M2 — Production activation and projections: T2 activates the final contract
  and Rust/HTTP/gRPC journeys; T3 projects Python, TypeScript, and MCP.

## Task inventory

| Task | Packet | Outcome | Direct dependencies |
|---|---|---|---|
| T1 | [Inactive distributed execution engine](tasks/00-d1-inactive-distributed-execution-engine.md) | Inactive authenticated follower execution with structured ownership | None |
| T2 | [Route and activate production analytics](tasks/01-d2-route-activate-production-analytics.md) | Final contract, conservative routing, activation, Rust/HTTP/gRPC journeys | T1 |
| T3 | [Python, TypeScript, and MCP projections](tasks/02-d3-python-typescript-mcp-projections.md) | Runtime-faithful first-class client and agent surfaces | T2 |

T1 is the only initially executable task. T2 consumes T1's inactive engine;
T3 consumes T2's integrated final contract.

## Global acceptance criteria

- AC1 (`PlatformDeterministic`, R1-R5, R8): both paths execute under one Oracle
  lifecycle, with actual follower/exchange evidence, exact tenant/cut/attempt
  binding, conservative routing, protected capacity, terminal safety, retry,
  audit, and six-node cleanup.
- AC2 (`CustomerDeterministic`, R6-R8): real Rust, HTTP, generated-gRPC,
  Python, TypeScript, and MCP clients prove Interactive, Analytical, EXPLAIN,
  structured failure, cancellation/ceiling behavior, and runtime settlement.
  Each journey also proves the intended production `OracleTelemetry` metric
  deltas and correlated trace/log events through the existing capture.
## Verification strategy

Completion evidence is behavioral only and follows Wyrd's three test tiers:
Tier 1 user journeys, Tier 2 integration tests, and Tier 3 unit tests. These
test tiers are distinct from the T1-T3 implementation task identifiers above.
No latency, throughput, resource-efficiency, scaling, or comparative-performance
measurement is an acceptance criterion for this plan.

Instrumentation evidence is behavioral verification: every task drives the
real production path, snapshots the existing production telemetry capture, and
asserts the expected bounded metrics plus correlated trace/log lifecycle.
Tests may use deterministic scoped capture and fault injection, but they may
not create a second instrumentation path or emit the signal they assert.

Each task packet contains binding TDD slices. A production behavior begins with
the named failing unit/integration/journey test at the stated production seam;
the implementer makes the smallest owner change, applies the named mutation to
prove the test fails for the intended reason, restores GREEN, then runs the
whole owning `mise` lane. Characterization tests begin GREEN only where they
pin inherited behavior, followed immediately by mutation RED before new work.

Progressive integrated proof:

1. T1: `test:bifrost`, `test:bifrost:journey:oracle`, dependency and resource
   boundary checks, format, lints.
2. T2: contract/codegen/SQL, Oracle/server journeys, proto drift, format,
   lints.
3. T3: Python/TypeScript unit, typing, generated binding, integration, MCP,
   boundary, format, and lint lanes.
4. Integrated closeout after T3: every public runtime journey and `mise run
   gate` because the completed feature crosses server, contract, SDK, binding,
   and MCP boundaries.

No zero-test selector, direct physical-plan injection as a journey, ignored
required test, live credential, weakened gate, or hand-edited generated
artifact counts as evidence.

## Closeout verification

Before implementation handoff, run from the planning repository:

```bash
python3 scripts/validate_plan_artifacts.py wyrd/active/bifrost-distributed-analytics-engine
```

Before release closeout, preserve sequential outputs for the integrated
verification commands, `git diff --check`, final diff inspection, and the
tested source/build identity. Completion requires no required claim unverified,
no prohibited change, and no unjoined resource owner.

## Risks, migration, and rollout

The leading feasibility risk is whether pinned `datafusion-distributed` exposes
the early-authentication, runtime injection, supervised-future, and
attempt-invalidation seams required for correct query lifecycle. T1 uses the
upstream pin unchanged; stop for a fork or dependency-cone change.

Distributed cancellation, backpressure, and spill can leak ownership even when
results are correct; post-drain evidence and mutation-sensitive lifecycle tests
are release obligations.

There is no database migration or compatibility rollout. Activation is one
code transition after T1 evidence is green. Readiness fails closed when the
Analytical handle, peer ingress, authority, or supervisor is unhealthy.
Rollback disables Analytical activation and returns all queries to the existing
Interactive path before selection; it never reinterprets an already selected
Analytical attempt or changes stored evidence.

## Execution handoff

Every task requires the `wyrd-implement` skill. Implement one task against the
recorded revision or a reviewed descendant, preserving its named requirements,
decisions, owners, tests, mutations, and commands. Private paths may be updated
to exact current equivalents only when the task's owner, behavior, caller flow,
and proof seam remain unchanged; record that bounded correction. Stop for any
public/durable contract, dependency, feature, tenancy, security, audit,
migration, or acceptance change not already locked here.

Plan version 7 requires independent readiness review. Tasks remain `Planned`
and may enter implementation only through their declared dependencies and
required `wyrd-implement` handoff.
