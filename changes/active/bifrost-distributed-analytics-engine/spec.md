---
id: SPEC-bifrost-distributed-analytics-engine
revision: 3
status: approved
---

# Bifrost distributed analytical queries v1

## Human intent and user value

Wyrd v1 must put useful distributed analytical queries into production without
turning Bifrost into a new distributed-database framework.

One authenticated raw-SQL query surface serves three primary workloads:

- low-latency UI reads over metrics, traces, logs, and similar operational data;
- larger exploratory joins and aggregations run by data scientists; and
- server-initiated drift, evaluation, and scheduled analytical queries.

Oracle selects either the `Interactive` or `Analytical` execution path. Callers
submit SQL and consume the same terminal-safe stream regardless of the selected
path. The server owns routing, admission, distributed execution, tenant safety,
audit, cancellation, and cleanup.

This revision replaces the remaining behavior in the original Task 1
remediation and the proposed Task 2 and Task 3 with one KISS v1 outcome. Existing
planning artifacts have no authority under this revision unless `$wyrd-plan`
later reconciles and retains them after explicit approval.

## Scope

- Close the remaining GraphLease, distributed-query cleanup, physical-execution,
  and private-deployment gaps in the inactive Analytical engine.
- Activate the Analytical path behind the existing production raw-SQL query
  surface.
- Preserve Interactive as the default and protect it from Analytical resource
  pressure.
- Route supported exchange-requiring queries to Analytical without introducing
  a second optimizer or a new exact-routing-facts subsystem.
- Support and prove the v1 distributed operator baseline: filtered and projected
  scans, fixed-width grouped aggregation, multi-input equi-join, streamed
  exchange, and one real spilling DataFusion operator.
- Project the same query request, stream, selected path, terminal evidence,
  cancellation, and structured errors through Rust, HTTP/gRPC, Python,
  TypeScript, and MCP.
- Prove the real production user journeys for UI, data-scientist, internal
  scheduled, and agent-facing queries.

## Non-goals

- A public distributed-plan or execution-path `EXPLAIN` API in this change.
- A caller field, SQL hint, separate endpoint, or deployment role that selects
  `Interactive` or `Analytical`.
- Automatic transparent retry of a distributed attempt. A failed caller may
  submit a new logical query.
- Exhaustive compatibility claims for windows, correlated subqueries,
  deduplicating sets, UDAFs, every join type, every aggregation state, or every
  DataFusion operator.
- Repeating the complete public journey at one, two, three, and six Oracle
  replicas. Evidence must use only the smallest topologies that prove local and
  real cross-process distributed behavior.
- A new routing-facts engine, optimizer, scheduler, shuffle service, query-job
  service, polling API, resource framework, registry, listener owner, or
  analytical-only pod role.
- Exact prediction of every dependency allocation or transient encoded-message
  byte.
- Separate operator-memory and exchange-memory pools.
- Forking, vendoring, locally patching, or waiting for an upstream change to
  `datafusion-distributed`.
- Cross-region distributed execution, autoscaling behavior, distributed writes,
  DML, or CTAS.

## Definitions

- **Interactive:** Oracle's default execution path for low-latency work and any
  query for which a safe supported distributed plan is not selected.
- **Analytical:** Oracle's streamed distributed execution path using the pinned
  `datafusion-distributed` dependency and one or more authenticated Oracle
  peers.
- **Analytical candidate:** a query that Oracle's existing optimized-plan
  classification identifies as potentially benefiting from distributed
  execution. Candidate status is not final path selection.
- **Analytical selection:** the irreversible point after Oracle has built and
  validated a supported distributed physical plan containing a real network
  exchange and has admitted the query for distributed execution.
- **GraphLease:** one follower-local ownership boundary connecting an admitted
  reservation to one exact distributed query graph for its complete lifetime.
- **Query envelope:** the query-owned runtime, aggregate DataFusion memory
  ceiling, slots, finite Wyrd-owned queue and fan-out controls, scratch/spill
  allocation, cancellation tree, and absolute deadline.
- **Terminal-safe stream:** an incremental Arrow result stream whose preceding
  rows constitute a successful complete query only after one explicit success
  terminal.
- **Supported Analytical baseline:** filtered/projected scans, fixed-width
  `COUNT`, `SUM`, `MIN`, and `MAX` grouping, multi-input equi-join, streamed
  exchange, and a DataFusion operator capable of spilling through the admitted
  query scratch allocation.

## Required behavior

### REQ-001 — One public query contract

The existing authenticated raw-SQL query operation shall remain the only public
execution entry point. The request shall not accept an execution path, query
class, stage graph, worker set, physical plan, or tenant override.

Rust, HTTP/gRPC, Python, TypeScript, and MCP shall project the same server-owned
query semantics, deadline, incremental Arrow results, selected execution path,
terminal outcome, and structured errors.

### REQ-002 — Simple server-owned routing

Interactive shall remain the default. Oracle shall use its existing optimized
plan classification and immutable pinned-query facts to identify an Analytical
candidate; this change shall not add a second optimizer or require a new
exact-routing-facts subsystem.

An Analytical candidate shall become Analytical only when distributed physical
planning succeeds, the plan is within the supported v1 baseline, and the plan
contains a real network exchange. Before Analytical selection, unsupported
shape, unavailable safe facts, distributed-planning failure, or absence of an
exchange shall execute through Interactive when Interactive supports the query.

After Analytical selection, resource, peer, transport, protocol, deadline,
cancellation, or execution failure shall fail the logical query terminally. It
shall not rerun through Interactive.

### REQ-003 — Useful v1 Analytical execution

The production Analytical path shall correctly execute raw-SQL queries that
exercise the supported Analytical baseline across distinct Oracle processes.
The coordinator shall stream results without materializing a complete shuffle
or collecting the complete user result in memory.

Queries outside the proven baseline may remain Interactive. If neither path can
execute a query safely, Oracle shall return a stable structured failure rather
than claim broader Analytical compatibility.

### REQ-004 — Exact graph ownership

Oracle shall reserve each selected follower immediately before the first stage
dispatch and shall retain the existing two-second pending reservation TTL.

The first valid `SetPlan` or `ExecuteTask` for an exact graph may activate its
GraphLease. Message ordering and duplicate delivery shall not double-charge or
destroy the reservation: one activation owns the transition, equivalent
concurrent or later messages reuse the published graph, and mismatched messages
fail before worker, cache, provider, or source IO.

Activation shall validate the complete reservation and graph authority before
consumption, publish the graph only after all fallible activation work succeeds,
and restore or release all ownership on failure. The accepted tenant, query,
graph, participant cut, authority, deadline, runtime, memory pool, and scratch
allocation shall not be widened or overwritten by later stage messages.

### REQ-005 — One practical query envelope

Every query shall own one finite query-local DataFusion memory ceiling for its
complete lifetime. Analytical operators and streamed exchanges shall draw from
that same pool. Wyrd shall not precharge a predicted exchange allocation or
create separate operator and exchange sub-pools.

Before dispatch, Oracle shall apply checked finite limits to the resources it
owns: concurrent path slots, the protected Interactive floor, Wyrd-owned
admission queues, selected workers, admitted tasks and partition ranges,
scratch demand, result transport, cancellation, and deadline.

Pinned dependency-owned exchange queues may rely on their existing aggregate
byte backpressure where Wyrd cannot configure an item-count limit. Wyrd shall
name that boundary honestly and shall not claim exact control over every
dependency-internal item or transient encoder allocation.

### REQ-006 — Interactive protection

Interactive and Analytical shall have distinct admission accounting beneath one
aggregate Oracle capacity root. Analytical work shall never consume the
configured Interactive slot floor. Resource refusal shall not mutate another
query's grant or silently weaken a query's limits.

UI queries that qualify for Interactive shall remain serviceable while
Analytical work occupies all capacity available to the Analytical path.

### REQ-007 — Cancellation, failure, and joined cleanup

One immutable deadline and one cancellation tree shall cover the complete
distributed graph. Success, cancellation, deadline, caller drop, peer loss,
transport failure, resource exhaustion, activation failure, and server shutdown
shall stop and join all owned descendants before releasing graph, runtime,
memory, scratch, slot, task, cache, connection, spill-file, or transport
ownership.

Cleanup timeout or failure shall not be reported as successful release. The
remaining graph shall stay observable to the owning supervisor, the node shall
not claim a clean terminal state, and readiness or shutdown evidence shall
surface the failure.

V1 shall not automatically retry a failed distributed attempt. This keeps one
logical query bound to one execution attempt and prevents overlap, duplicated
rows, or retry-specific resource retention. A caller may submit a new query
after receiving the terminal failure.

### REQ-008 — Terminal and audit semantics

Both paths shall use the same terminal-safe stream. A successful terminal shall
identify the selected `Interactive` or `Analytical` path and shall be emitted
only after query execution and required cleanup have settled. A failure terminal
shall invalidate preceding frames as a complete successful result.

One logical query shall produce one Oracle read-audit acceptance before rows may
be returned. Distributed stages shall not append separate read-audit events.
Authentication, authorization, tenant isolation, immutable query cut, sensitive
column permissions, and public query identity shall remain unchanged across
paths.

### REQ-009 — Production topology and isolation

Oracle replicas shall remain horizontally interchangeable: any Oracle may
coordinate an Interactive or Analytical query and may execute follower work for
another coordinator. `Interactive` and `Analytical` are execution paths, not pod
roles.

Distributed worker and lifecycle services shall remain on the server-owned
private peer listener protected by the existing mTLS, workload authentication,
purpose tickets, immutable participant cut, fences, bounded admission, readiness,
and ordered shutdown. They shall not be reachable through the public listener.

Self-hosted, SaaS, and enterprise deployments shall expose the same query
behavior. Deployment profiles may choose different finite capacities but shall
not change query semantics.

### REQ-010 — Thin first-class projections

Python and TypeScript shall expose idiomatic incremental query streams over the
shared Rust client and server wire contract. Runtime cancellation, iterator
close/drop, malformed terminal data, and transport error shall settle the server
query and release native/runtime handles without moving routing or durable
lifecycle logic into the client.

MCP shall expose the same raw-SQL query behavior with closed input schemas,
caller ceilings for SQL, deadline, rows, and bytes beneath server hard limits,
server-owned authentication and tenant binding, and no successful truncation.

Each projection shall preserve structured errors and the terminal execution
path. It shall not reproduce the internal operator, routing, or topology matrix.

### REQ-011 — Operational evidence

Oracle shall expose bounded-cardinality production telemetry for selected path,
admission/refusal, active queries, worker fan-out, memory current/peak, exchange
activity where reported by the dependency, spill, cancellation, deadline,
failure category, cleanup, and terminal outcome.

Tenant, table, SQL, query, graph, snapshot, and task identities shall remain in
scrubbed traces or durable evidence, never metric labels. Configuration values
or plan text alone shall not count as evidence of physical distributed work.

## Invariants and prohibited outcomes

### INV-001 — Server authority

Only Oracle selects the execution path and owns query admission, the immutable
cut, audit, cancellation, and terminal settlement. DataFusion and clients are
execution consumers, not policy authorities.

### INV-002 — Tenant and snapshot integrity

Every leader and follower operation shall remain bound to the authenticated
tenant, permission digest, exact pinned cut, graph identity, participant
destination and fence, and original deadline before plan decode or source IO.

### INV-003 — No partial success

No refusal, cancellation, deadline, peer loss, resource exhaustion, malformed
terminal, or cleanup failure may be represented as successful or silently
truncated output.

### INV-004 — No live-child release

A graph lease, query envelope, spill allocation, or participant reservation
shall not be released while an owned child, driver, task, cache entry, exchange,
connection, or cleanup operation remains live.

### INV-005 — Bounded Wyrd ownership

Every Wyrd-owned queue, slot set, fan-out, task/partition range, result stream,
memory pool, scratch allocation, spill path, and cleanup task set shall be
finite. Dependency-owned queues retain only the guarantees provided by the
pinned dependency.

### INV-006 — Interactive floor

Analytical work shall never borrow the configured Interactive floor, including
during activation, failure cleanup, or shutdown.

### INV-007 — One dependency universe

V1 shall use the repository's single pinned DataFusion 55, Arrow/Parquet 59.2,
and `datafusion-distributed` dependency universe. It shall add no fork, vendor
copy, local patch, compatibility bridge, or duplicate native universe.

### INV-008 — KISS boundary

This change shall introduce no second query API, optimizer, scheduler, shuffle
service, analytical deployment role, resource registry, listener owner,
telemetry owner, or client-side routing implementation.

## Externally observable production behavior

### Successful Interactive query

1. A caller submits authenticated read-only SQL through a public query client.
2. Oracle authorizes the tenant and columns, pins one immutable cut, and selects
   Interactive.
3. Oracle admits the query without surrendering the protected Interactive floor
   to Analytical work.
4. The caller receives incremental Arrow batches followed by one success
   terminal identifying `Interactive`.
5. Cancellation, deadline, stream drop, and shutdown use the same structured
   terminal and cleanup semantics as Analytical.

### Successful Analytical query

1. A caller submits the same authenticated raw-SQL request; no path hint is
   supplied.
2. Oracle authorizes the query, pins one immutable cut, classifies it as an
   Analytical candidate, and builds a supported distributed physical plan with
   a real network exchange.
3. Oracle admits one query envelope, freezes the participant cut, and reserves
   selected followers immediately before dispatch.
4. Authenticated follower processes activate one exact GraphLease each and run
   assigned scan, join, aggregation, exchange, and spill work under the query's
   runtime, memory pool, scratch allocation, cancellation tree, and deadline.
5. The coordinator streams bounded Arrow batches to the caller.
6. Oracle cancels or finishes descendants, joins them, removes spill files and
   graph/cache state, releases every participant lease and query resource, and
   emits one success terminal identifying `Analytical`.

### Pre-selection fallback

If Oracle cannot construct a supported distributed plan or the candidate has no
real exchange, it may execute the query through Interactive before Analytical
selection. The successful terminal identifies `Interactive`. The server shall
not claim that an Analytical attempt ran.

### Post-selection failure

After Analytical selection, peer loss, cancellation, deadline, resource or
transport refusal, protocol failure, execution failure, or cleanup failure
produces one structured failed terminal. Oracle shall not retry automatically,
fall back to Interactive, or report preceding rows as a complete result.

## Required user journeys

### Journey A — UI operational query

A real client queries a bounded metrics or traces window through the production
server. The request is authenticated and tenant-isolated, selects Interactive,
streams exact results, reports `Interactive` in its terminal, and leaves all
query ownership at baseline while Analytical capacity is occupied.

### Journey B — Data-scientist distributed query

A real client submits raw SQL containing a supported multi-input join and
fixed-width grouped aggregation over enough published data to require a network
exchange. A coordinator Oracle sends physical work to at least one different
Oracle process. The result equals a trusted Interactive/local result; physical
evidence proves remote scan/pushdown, exchange, join/aggregation, one real
operator spill write/read/cleanup, and zero retained ownership after the
terminal.

### Journey C — Internal scheduled analytical query

A server-owned drift, evaluation, or scheduled-query caller uses the same query
contract and server routing, receives an Analytical terminal for a supported
large/global operation, and gains no private path selector or lifecycle owner.
Its cancellation or deadline stops the complete distributed graph and returns
all ownership to baseline.

### Journey D — Failure and pressure

A real distributed query is cancelled or loses a follower after Analytical
selection. The caller receives one structured failed terminal, never successful
partial rows or a silent Interactive rerun. All reachable Oracle processes
eventually report zero query graphs, attempts, leases, tasks, cache entries,
exchange ownership, and spill files. A separate admission-pressure case proves
that Analytical saturation does not consume the Interactive floor.

### Journey E — First-class client and agent projections

Rust/HTTP or generated gRPC proves the complete production route and physical
topology once. Python, TypeScript, and MCP each drive a real client-to-server-to-
client query lifecycle and prove typed streaming, selected-path evidence,
structured failure, cancellation/close behavior, and cleanup. They do not each
repeat the physical operator matrix.

## Material constraints and required boundaries

- `wyrd-server` remains the only network-serving and listener-lifecycle owner.
- Vala Oracle remains the cohesive query, route, admission, distributed
  execution, and cleanup owner.
- Scribe and the immutable Iceberg/live-tail cut remain the source authority;
  distributed reads do not change ingest or catalog ownership.
- The private peer security and fenced-destination behavior already proven by
  the Task 1 remediation remains required and is not redesigned here.
- The existing two-second pending reservation TTL remains unchanged.
- Query output remains streamed Arrow with one explicit terminal; no
  asynchronous query-job or result-polling protocol is added.
- Public contracts remain language-neutral and generated from their owning
  sources. Python and TypeScript remain projections rather than durable owners.
- MCP remains agent-facing, bounded, permissioned, and tenant-safe.

## Acceptance obligations

### AC-001 — GraphLease closeout

Focused ownership evidence and a real multi-process journey shall prove
validate-before-consume, exactly-once activation under either stage-message
ordering, duplicate reuse, mismatch refusal before IO, activation rollback,
immutable graph authority, drain-before-release, and zero retained ownership on
success, cancellation, failure, and shutdown.

### AC-002 — Representative physical execution

The data-scientist journey shall prove correct cross-process scan/pushdown,
multi-input equi-join, fixed-width grouped aggregation, positive streamed
exchange, one qualified real DataFusion spill write/read/cleanup, exact result
equivalence, and a successful Analytical terminal.

No plan string, route label, manual temporary file, or test-only synthetic event
may substitute for physical runtime evidence.

### AC-003 — Production routing

Real public-client journeys shall prove a representative UI query selects
Interactive and a representative data-scientist and internal scheduled query
select Analytical without caller hints. Unsupported/no-exchange distributed
candidates shall fall back only before selection; an injected post-selection
failure shall be terminal.

### AC-004 — Bounded resources and failure cleanup

Focused evidence shall prove finite Oracle-owned slots, admission queues,
worker/task/partition fan-out, query memory, scratch, result transport,
cancellation, and deadline controls, including preservation of the Interactive
floor. A real process journey shall prove representative refusal/cancellation/
peer-loss cleanup. It need not prove dependency-internal queue item counts or an
exact per-message byte ceiling.

### AC-005 — Private production topology

Server and deployment evidence shall prove peer services are absent from the
public listener, present only for the required server targets, protected by the
existing peer trust boundary, represented by reachable deployment configuration,
included in readiness, and joined during shutdown. The same Oracle process
shape shall coordinate and follow work.

### AC-006 — Public contract and projections

Source-generated contract evidence and real Rust/HTTP or gRPC, Python,
TypeScript, and MCP journeys shall agree on request semantics, incremental Arrow
results, deadline, selected path, terminal outcome, cancellation, and structured
errors. MCP shall additionally prove closed bounded input and no successful
truncation.

### AC-007 — Operational evidence

Production telemetry observed through the journeys shall prove bounded labels,
path selection, admission/refusal, real follower and exchange activity, spill,
cancellation/failure, cleanup, terminal settlement, and every active gauge
returning to baseline. Test-only telemetry that the production owner does not
emit is insufficient.

### AC-008 — Regression and simplicity

Existing Interactive, Scribe, tenant-isolation, audit, and public-query journeys
shall remain green. Final review shall confirm that the change added no fork,
second DataFusion/Arrow universe, public path selector, EXPLAIN requirement,
automatic retry, scheduler, shuffle service, analytical pod role, duplicate
owner, or client-side durable behavior.

## Open material decisions

None in this draft. Approval would explicitly choose:

- no public distributed `EXPLAIN` requirement for this v1 change;
- no automatic distributed-query retry;
- representative rather than exhaustive operator and replica-count coverage;
- reuse of existing optimized-plan classification plus real-exchange validation
  instead of a specialized routing-facts subsystem; and
- practical aggregate resource containment without a dependency fork or exact
  dependency-internal message/queue proof.

## Planning-decision inventory

After approval, `$wyrd-plan` must decide only implementation-level details:

- the smallest existing owner and atomic boundary for GraphLease activation,
  duplicate waiting/reuse, rollback, joined cleanup, and shutdown inspection;
- the exact existing classification inputs and conservative thresholds reused
  for Analytical candidacy without creating a new routing subsystem;
- the exact supported physical-plan checks for the required operator baseline;
- the smallest topology proving one-Oracle Interactive behavior and real
  cross-process Analytical behavior, including whether the latter needs two or
  three Oracle replicas;
- the existing public contract sources that project selected-path terminal
  evidence consistently without adding a path selector or EXPLAIN;
- the narrow runtime-native cancellation/close mechanics for Python,
  TypeScript, and MCP projections;
- which existing Task 1 remediation code and evidence already satisfies this
  revision, which portions of the proposed Task 2 and Task 3 remain cohesive,
  and which current task artifacts shall be moved to scrap; and
- the focused `mise` commands and journey topology that prove AC-001 through
  AC-008 without repeating the full physical matrix in every client runtime.

## Authority reconciliation required on approval

This draft intentionally narrows behavior currently stated more broadly in
`architecture/bifrost-design.md` and the focused Bifrost references. Approval
requires the owning architecture to be reconciled before implementation tasks
are declared ready:

- make public distributed `EXPLAIN` deferred rather than required for this v1
  delivery;
- remove the mandatory transparent pre-egress retry and define peer loss as a
  terminal query failure;
- describe the representative supported Analytical baseline without promising
  the full window/subquery/deduplicating-set operator family;
- permit reuse of the existing conservative classification and real-exchange
  validation without requiring a new exact routing-facts subsystem; and
- preserve the already reconciled single aggregate query pool, practical Wyrd-
  owned bounds, dependency byte-backpressure boundary, and no-fork decision.

No other Bifrost ingest, durability, Iceberg, Forge, tenant, audit, or peer-
security behavior changes.

## Revision history

- Revision 1 (`approved`, 2026-09-01): selected practical aggregate query
  containment and removed the dependency fork and exact per-message exchange
  proof.
- Revision 2 (`approved`, 2026-09-01): limited item-count guarantees to
  Wyrd-owned queues and tonic configuration to the supported worker-server
  boundary.
- Revision 3 (`approved`, 2026-09-01): the human explicitly approved one KISS
  production closeout across the remaining inactive engine, production
  activation, and first-class client projections; narrowed routing, retry,
  EXPLAIN, operator, and topology scope; and defined the complete production
  user journeys.

## Material authority

- [`AGENTS.md`](../../../AGENTS.md)
- [`architecture/agent-rules.md`](../../../architecture/agent-rules.md)
- [`architecture/wyrd-design.md`](../../../architecture/wyrd-design.md)
- [`architecture/wyrd-doctrine.mdx`](../../../architecture/wyrd-doctrine.mdx)
- [`architecture/bifrost-design.md`](../../../architecture/bifrost-design.md)
- [`architecture/references/languages/spec-driven-development.md`](../../../architecture/references/languages/spec-driven-development.md)
- [`architecture/references/domain/olap-serving.md`](../../../architecture/references/domain/olap-serving.md)
- [`architecture/references/domain/datafusion.md`](../../../architecture/references/domain/datafusion.md)
- [`architecture/references/domain/analytical-operations-reliability.md`](../../../architecture/references/domain/analytical-operations-reliability.md)
