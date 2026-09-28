---
id: SPEC-bifrost-scribe-live-reads
revision: 5
status: approved
---

# Execute best-effort live reads on Scribes

## Objective and user value

A Bifrost query shall combine a pinned published cut with live rows scanned
on relevant online Scribes. Oracle coordinates one DataFusion plan and streams
the combined result. Large published scans continue to use distributed Oracle
workers. Every caller, including verification, uses this same query service and
source behavior. Live coverage is best effort.

This revision intentionally replaces the exact, leased Fused-cut promise in
`architecture/bifrost-design.md` and the corresponding OLAP reference. The
architecture and public documentation must be updated in the same change as
the implementation.

## Required behavior

### REQ-001 — One public query contract

The public query request has no `visibility` or `freshness` field. Rust,
Python, TypeScript, HTTP, CLI, MCP, and internal verification callers cannot
select published-only, Fused, Strict, or AllowDegraded behavior. Every query
uses the pinned Iceberg and committed hot-object cut plus selected online
Scribe live sources. The existing query-deadline range is unchanged.

The public terminal distinguishes `Success`, `Degraded`, and `Failed`. `Success`
means the published cut completed and each selected online live source either
finished its plan-required work or was intentionally stopped after the
DataFusion plan no longer needed that source (for example, after `LIMIT`). It
does not promise inclusion of every acknowledged write. `Degraded` means a known
live source was unavailable before yielding rows and carries
`LiveTailUnavailable`. A required published-source failure fails the query.
Live-source completion describes only selected online participants, not
undiscovered WAL owners. Remove the redundant public `freshness` result field
and its `Complete`/`BestEffort` distinction; describe best-effort live coverage
as a property of every query. Clients must wait for a valid terminal before
accepting streamed rows as a result.

### REQ-002 — Discover and select live work

After authentication, authorization, table resolution, and publication-cut
pinning, Oracle asks ready Scribes for active streams on the referenced tenant
tables before the single physical planning pass. Discovery is a lightweight
listing, not a query scan sent to every Scribe. Oracle selects reported live
table partitions by safe query pruning; uncertain predicates retain every
reported route for that table. Only selected owners receive live scan work.
Selection binds the reported node incarnation, writer epoch, table, and
partition. Live-only work participates in resource planning and admission.

A Scribe absent from the ready roster before discovery is outside the query's
known live set, even if its acknowledged, unpublished data exists.
That omission is within the stated best-effort result. A failed listing from
a ready Scribe is known live-source loss and degrades the query unless the
failure reveals a security, tenant, schema, cancellation, or deadline fault,
which fails it.

### REQ-003 — Execute the live scan where the data lives

Oracle keeps one DataFusion physical planning pass. Its published-file work
retains distributed Oracle-worker execution. Selected Scribes scan their own
memtable or staged authority, apply assigned column projection and supported
predicates, and stream bounded Arrow results to an Oracle-owned live source.
DataFusion's residual predicates remain authoritative. Live fragments may run
concurrently on multiple Scribes while Oracle combines them with published
work. This change does not promise Scribe-side general aggregation or joins.

The existing authenticated peer protocol, signed assignments, tenant and
schema checks, Scribe resource admission, and per-query deadline/cancellation
apply. A live stream validates frames as they arrive and requires a valid final
footer when consumed to its natural end. When the completed DataFusion plan no
longer needs an opened fragment, Oracle cancels and drops that child as ordinary
query-owned cleanup; no footer is required from work the plan intentionally
stopped. It must not buffer the entire fragment before Oracle can consume it.

### REQ-004 — Failure and terminal semantics

An unavailable selected Scribe before its first row may be omitted with a
`Degraded` terminal. Once that Scribe has yielded any row, its failure while
the plan still needs it fails the whole query; Oracle cannot retract rows
already combined or sent. A
missing or invalid stream footer from a still-needed fragment fails the query
even after data frames. An intentional stop initiated by the completed plan is
not a missing-footer failure and does not degrade the result.
Published-source, security, tenant, schema, protocol-integrity, resource,
cancellation, and query-deadline failures fail the query. A failed terminal
never turns prior frames into a successful partial result.

### REQ-005 — Query-owned live resources

An opened Scribe fragment retains its local snapshot references and admitted
resources until completion or stream drop. Backpressure bounds in-flight
bytes/batches; cancellation, disconnect, deadline, and ordinary completion
release ownership. A fragment remains valid beyond 30 seconds while its query
and stream remain active. Remove the acquire/page/release tail-fence protocol,
its 30-second expiry, Oracle's full leader drain, and their obsolete bindings
once the new path is the only public live reader. Keep authenticated active-
stream discovery. Peer-ticket acceptance expiry remains an authentication
replay control and does not impose a second lifetime on accepted work.

### REQ-006 — Publication overlap is explicitly best effort

No atomic handoff or durable current-owner inventory is added between the
pinned published cut and the later opening of a Scribe live scan. Publication
between those moments may briefly omit rows when the cut precedes the file
commit and live authority has retired, or count rows twice when the cut
includes the file but live authority has not retired. An already-open Scribe
snapshot remains readable until its stream ends. Neither race may be described
as globally complete.

### REQ-007 — Verification uses the same query

Production Drift verification and every other verifier that queries Bifrost
use the same query service, source selection, terminal semantics, and automatic
Interactive/Analytical classification as ordinary callers. No verification-
only published read, alternate source mode, or second query endpoint exists.
Verification consumes the query result only after a successful terminal. A
verifier must surface a failed query as a failed or inconclusive verification,
and its evidence must not claim that a successful query contains every
acknowledged write. The accepted live-read tradeoff includes an omitted or
duplicated row during the publication overlap, which may affect a judgment.

Rust, Python, TypeScript, HTTP, CLI, MCP, generated schemas, and user docs
expose one request and terminal contract. Existing tests that select Fused or
PublishedOnly are updated to exercise the one query behavior.

### REQ-008 — Measured read capacity on one modest node

The one query service must be measured and improved through the public client
on one locally launched Bifrost node limited by Linux to 4 CPUs and 8 GiB,
using local NVMe. PostgreSQL and the load driver run outside that limit;
Docker is used only by the repository-managed PostgreSQL setup. The benchmark
must distinguish a published-only read from a read with selected live Scribe
data, including a Scribe on another pod, and must actually hold the stated
number of live readers throughout a mixed window. It must include concurrent
acknowledged writes rather than measuring writes only after reads stop.

At minimum, report separate, correctly validated workloads for a trivial
lookup, a selective point read, a small filtered aggregate, medium analytical
reads, a large time-window aggregate, a full scan over about 100 million rows,
and a representative analytical mix. Sweep client concurrency through 1, 4,
8, 16, 32, 64, and 100, recording sustained successful queries per second,
client-to-client p50/p95/p99 latency, physical bytes scanned, rows examined
when the workload makes them knowable, acknowledged write rate, CPU, memory,
refusals by owning boundary, and the point where latency rises without useful
throughput gain. Preserve raw samples, server telemetry, process logs, and
fixture geometry with a short report a human can audit. An invalid workload
must be reported as invalid rather than assigned a throughput result.

The engineering targets on this envelope are more than 1,000 sustained
selective reads/second with p95 at most 25 ms; at least 100 small filtered
aggregates/second with p95 at most 100 ms; medium analytical queries at p95 at
most 200 ms, including about 10 million examined rows at p95 at most 300 ms;
a large time-window aggregate at p95 at most 500 ms; and a roughly
100-million-row full scan under 2 seconds. The representative mixed workload
must sustain at least 100 analytical queries/second while at least 100,000
rows/second are durably acknowledged, with client p50 below 50 ms, p95 at most
200 ms, p99 at most 500 ms, at least 500 MB/second physical scan throughput
when a scan workload is running, and peak node memory below 7 GiB without an
OOM event. A 5,000/second selective read rate and the higher class-specific
targets supplied by the user are stretch goals, reported separately rather
than treated as proven. CPU utilization is reported; 70–90% at saturation is
diagnostic, not a minimum utilization requirement.

The existing two-permit snapshot step must not reject ordinary queries before
Oracle's bounded query admission when the query still has time to wait.
Query deadlines and bounded waiting remain authoritative. Remote peer reads
must reuse secure transport connections rather than making a fresh TLS
connection for each fragment. Preserve the existing authorization, tenant,
snapshot, query-resource, and terminal rules. Performance claims require valid
measured results; a passing correctness gate alone is insufficient.

### REQ-009 — Queue saturation until the leader deadline

After authentication, authorization, and request validation, a query that can
run on this Oracle but finds its execution slots busy waits in Oracle's
tenant-fair query queue. Ordinary saturation does not produce an immediate
planning, HTTP edge, or execution-admission refusal. The queue holds at most
1,000 waiting queries per Oracle node across both classes, separate from
running queries. Its only time limit is the one absolute leader-owned query
deadline: 30 seconds by default, or the caller's valid `deadline_ms` when
provided. That same deadline covers planning, queueing, and execution; it
does not restart when a query leaves the queue. Remove the independent 250 ms
queue timer. If a slot opens in time,
the queued query runs. If its deadline expires first, it receives a query
timeout and owns no retained queue or snapshot resources. Client cancellation
also removes it promptly. If all 1,000 waiting places are occupied, the next
query receives a clear, retryable queue-full overload response. Shutdown,
role loss, a class the node cannot execute at all, and genuine resource or
security faults remain distinct from temporary saturation. The same behavior
is visible through HTTP, gRPC, and the first-class SDKs.

## Invariants and boundaries

- **INV-001:** Write acknowledgment, WAL durability, publication order, and
  Iceberg promotion are unchanged.
- **INV-002:** Authenticated tenant authority, signed peer assignments,
  authorized projection, tenant tripwire, and sensitive-column denial remain
  effective before source IO and through execution.
- **INV-003:** One physical plan and the existing Interactive or Analytical
  selection remain authoritative; Oracle derives that class automatically
  from the returned DataFusion physical root. No caller chooses a query class
  or source mode. Published-file scans retain distributed Oracle-worker
  execution.
- **INV-004:** Oracle never opens another pod's local WAL or staged files, and
  client results are accepted only after a valid terminal.
- **INV-005:** Every retained queue, snapshot, in-flight batch, transport, and
  Scribe or Oracle resource grant remains bounded and query-owned.
- **INV-006:** A faster path may not skip tenant authorization, snapshot
  protection, query admission, memory governance, cancellation, or terminal
  validation. A saturated query waits under its own deadline; a full finite
  queue or a genuine resource fault fails without taking down the node or
  corrupting another query's result.

## Scope and non-goals

The scope is the public Bifrost query contract, Oracle live routing and
execution, Scribe live execution and resource lifetime, affected SDK and
server consumers, Drift verification, generated surfaces, architecture and
user docs, and corresponding tests.

There is no Arrow Flight service, durable batch-owner index, replacement
lease, second planner, new scheduler, Scribe analytical-stage worker role,
general aggregate/join pushdown to Scribe, public source-selection field,
verification-specific query path, or exact all-acknowledged-write promise.
Do not add new persisted state or change write ACK timing.

## Acceptance criteria and evidence

- **AC-001:** The public request, SDKs, CLI, MCP, generated schemas, and
  server route expose no visibility or freshness choice. Success, Degraded,
  and Failed terminals have the stated meanings without a public freshness
  result field. Contract and real-server journey evidence agrees.
- **AC-002:** With two relevant Scribe partition owners and an irrelevant
  third, only the two relevant owners execute live fragments. Discovery may
  contact all ready Scribes. A non-prunable predicate retains all relevant
  reported routes. Multi-node journey evidence records fragment destinations.
- **AC-003:** A query combining published files on multiple Oracle workers
  with live rows on multiple Scribes returns the correct filter and aggregate
  when publication is stable; published work remains distributed. A
  production-shaped distributed journey proves placement and result.
- **AC-004:** Larger-than-one-batch live output streams with bounded buffering
  and backpressure; cancellation and client disconnect release Scribe source
  references and admission. A controlled fragment lasting longer than 30
  seconds survives without a tail-fence expiry. Integration evidence uses no
  synthetic host load.
- **AC-005:** An absent-before-discovery Scribe can yield Success with
  best-effort coverage; a failed ready-Scribe listing or selected source before
  rows yields Degraded; failure after rows while the source is needed, or a
  missing/invalid footer at natural stream end, yields Failed. Published, security,
  tenant, schema, resource, cancellation, and deadline faults fail. Focused
  tests and real-server journeys inspect terminal metadata and row acceptance.
  A `LIMIT` query may succeed after the completed plan intentionally stops an
  unneeded live child; it releases that child's resources without waiting for
  a footer or draining the rest of its rows. Unexpected EOF while a child is
  still needed fails.
- **AC-006:** Drift verification uses the same query service and live-source
  behavior as other callers, accepts only a successful terminal as evidence,
  and reports query failure without a judgment based on partial rows.
  Documentation states both publication races, their possible effect on
  verification judgments, and the scope of selected-source completion.
- **AC-007:** The old public acquire/page/release tail path and 30-second
  lifetime are gone; active-stream listing and its authentication remain.
  Source inspection, regression coverage, and the Bifrost gate prove closure.
- **AC-008:** Format, lints, codegen, docs check, `verify:bifrost`, and the broad
  `gate` pass because this change crosses the query contract, execution,
  verification, and first-class client boundaries.
- **AC-009:** The benchmark and its focused tests prove the deployment,
  workload, held-stream, result, measurement, and report rules in REQ-008.
  Every required workload has a valid measured result or an explicit failure;
  no invalid run can satisfy a performance target.
- **AC-010:** On the specified 4-CPU/8-GiB node, the measured required
  workloads meet the numeric targets in REQ-008. A missed target, a refused
  acknowledged read-back, or an unproven CPU or snapshot diagnosis prevents
  completion and is reported with its owning boundary and raw evidence.
- **AC-011:** A real-server burst that fills execution slots enters the queue
  without a capacity refusal, can wait longer than 250 ms when its leader
  deadline permits, and either runs or times out at that deadline. The 1,000th
  waiting query fits; the next receives queue-full overload. Cancellation and
  timeout free their places. HTTP and gRPC do not shed an otherwise queueable
  authenticated query before Oracle. A 4-CPU/8-GiB process-cluster run with
  the full queue remains under the stated memory ceiling without OOM.

## Open material decisions

None. Revision 3 was explicitly approved by the user on 2026-09-26. The user
explicitly accepted the performance work, supplied its numeric targets, and
chose a finite 1,000-query queue on 2026-09-28.

## Revision history and authority

- Revision 1 (2026-09-26): Drafted from the user's detailed Scribe live-read
  rewrite decisions.
- Revision 2 (2026-09-26): User removed all public visibility/freshness
  choices and all verification query exceptions. Every query uses the same
  published-plus-live source behavior; Oracle alone selects Interactive or
  Analytical from the DataFusion plan. This supersedes revision 1's
  published-only verification and two-mode request contract. Approved by the
  user on 2026-09-26.
- Revision 3 (2026-09-26): Drafted after independent readiness review found
  that DataFusion may finish a valid `LIMIT` query before consuming an opened
  Scribe fragment to its footer. Distinguishes owner-initiated early stop from
  unexpected stream truncation without adding a lease, guard, or source mode.
  Approved by the user on 2026-09-26.
- Revision 4 (2026-09-28): Records the user-approved read-performance and
  benchmark requirements after the first capacity measurements revealed
  premature snapshot refusals, invalid held-live rows, and inadequate workload
  coverage. Approved by the user on 2026-09-28.
- Revision 5 (2026-09-28): Records the user's queue-until-deadline rule and
  1,000-waiter limit. A full finite queue returns overload; temporary slot
  saturation does not. Approved by the user on 2026-09-28.
- [Repository rules](../../../AGENTS.md),
  [agent rules](../../../architecture/agent-rules.md),
  [Wyrd design](../../../architecture/wyrd-design.md),
  [Wyrd doctrine](../../../architecture/wyrd-doctrine.mdx), and
  [Bifrost design](../../../architecture/bifrost-design.md).
