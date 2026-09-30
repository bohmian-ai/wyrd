---
id: SPEC-bifrost-scribe-live-reads
revision: 18
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

The authenticated mTLS peer protocol and receiver-side typed tenant, table,
query/assignment, target-node, fence, deadline, and resource checks in
`SPEC-verified-change-contract` REQ-160/REQ-161 apply to remote work. Local
work retains the same tenant, schema, resource, and query-lifetime rules
without a network hop. A live stream validates frames as they arrive and requires a valid final
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
stream discovery. A remote fragment's lifetime remains owned by its leader
query after peer authentication; no ticket expiry imposes another lifetime.

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

At minimum, report separate, correctly validated workloads for a
selective point read, a small filtered aggregate over roughly
100,000–1,000,000 rows, medium analytical reads over roughly 1–10 million
rows, a large time-window aggregate, a full scan over about 100 million rows,
and a representative analytical mix. Sweep selective and small aggregate
queries through client concurrency 1, 4, 8, 16, 32, and 64; measure the
one-million-row aggregate at eight clients so its QPS target is meaningful,
and the larger scans at one client. For
every measured workload and concurrency, record sustained
successful queries per second, client-send-to-complete p50/p95/p99 latency,
offered and rejected queries, CPU, and peak memory. Record physical bytes
scanned and rows examined where measurable, acknowledged write rows and bytes
per second, p95 write-batch latency, and the point where more concurrency
raises latency without useful throughput gain. Preserve raw samples, server
telemetry, process logs, and fixture geometry with a short report a human can
audit. An invalid workload must be reported as invalid rather than assigned a
throughput result. These are engineering targets for this machine, not
universal OLAP standards.

On the 4-CPU/8-GiB node, selective reads must reach client p50 <2 ms, p95
<5 ms, and p99 <10 ms while sustaining more than 1,000 successful reads per
second at the same stated concurrency. Small filtered aggregates must sustain
at least 100 successful queries per second with p95 <100 ms. Medium queries must
sustain at least 20 per second with p95 <300 ms at the same concurrency; the
roughly 10-million-row case also has p95 <300 ms at one client. The large
time-window aggregate and a roughly 100-million-row heavy scan must each
complete in <2 seconds; the heavy scan
must reach at least 500 MB/second in measured physical scan throughput at
one client. Higher concurrency points identify saturation; they need not
meet the single-client heavy-scan latency target. Sustained batched ingest
must durably acknowledge at least 100,000 rows per
second and report p95 batch latency, written bytes per second, CPU, and memory.

Under simultaneous sustained ingest of at least 100,000 acknowledged rows
per second, the representative read mix must sustain at least 100 successful
analytical queries per second. Small-query p95 remains <100 ms and medium-query
p95 <300 ms. At the same read offer
rate and client concurrency, each class's p95 may rise by less than 20%
against its read-only baseline. Peak node memory stays below 7 GiB with no
OOM event. A 5,000/second selective rate is reported as a stretch result;
it does not replace these required targets. CPU utilization is reported, not
assigned an invented passing floor.

The existing two-permit snapshot step must not reject ordinary queries before
Oracle's bounded query admission when the query still has time to wait.
Query deadlines and bounded waiting remain authoritative. Remote peer reads
must reuse secure transport connections rather than making a fresh TLS
connection for each fragment. Preserve the existing authorization, tenant,
snapshot, query-resource, and terminal rules. Performance claims require valid
measured results; a passing correctness gate alone is insufficient.

The standard benchmark uses one 10-million-row published table and ordinary
public queries through the configured object store, which is local by default.
Its named workloads are a selective read, a small filtered aggregate, a
one-million-row aggregate, a ten-million-row aggregate, a broad time-window
aggregate, a full-table scan, batched ingest, and analytical reads during
concurrent ingest. Include one valid live-reader case with a remote Scribe so
the common distributed read path is measured. Keep the selective read as a
serving result, separate from the analytical results. Sweep only selective
and small-aggregate client concurrency through 1, 4, 8, 16, 32, and 64;
measure the one-million-row aggregate at eight clients and the larger reads
at one client. Report client p50/p95/p99, successful
QPS at stated concurrency, physical scan bytes per second for scans, server
CPU, peak memory, refusal and wrong-result counts, acknowledged ingest rows
per second, and write-batch p95. Save raw samples, logs, and fixture geometry.
Use the existing process-cluster harness, telemetry, public client, and one
benchmark command. Do not add file-layout, cache-state, projection/predicate,
or fixed-offer cross-product benchmark suites. Those are focused diagnostics
if a measured workload needs them. The standard run should give a result in
about 10–15 minutes, excluding the release build; report actual setup and
measurement durations rather than claiming an unmeasured time bound.

A separate heavy-scan qualification uses the same schema and generator with
100 million rows and measures the broad time-window aggregate and full scan
against the existing <2-second and >=500 MB/second physical-scan targets.
The 100-million-row seed alone takes at least 16 minutes 40 seconds at the
minimum passing 100,000-row/second ingest rate, so its duration is reported
separately and is not counted against the standard benchmark's time goal.
The heavy qualification is still required to satisfy the heavy-scan targets;
it is not repeated during every quick capacity run. Both runs use the same
normal storage and query paths. During batched ingest, report average
committed Parquet file size and committed files/second alongside acknowledged
rows/second, bytes/second, and p95 batch latency. Mixed read/write runs report
the measured read-latency change from a matching read-only baseline.

### REQ-009 — One queue and timeout policy for both query classes

After authentication, authorization, and request validation, a query that can
run on this Oracle but finds its execution slots busy waits in Oracle's
tenant-fair query queue. Ordinary saturation does not produce an immediate
planning, HTTP edge, or execution-admission refusal. The queue holds at most
1,000 waiting queries per Oracle node across both classes, separate from
running queries. Both Interactive and Analytical use the same two operator-
configurable time limits: at most one hour in the queue and a two-hour default
total query deadline. A valid caller `deadline_ms` overrides the total
deadline for that query. Oracle owns the total deadline from query acceptance
through planning, queueing, and execution; it does not restart when a query
leaves the queue. A waiter stops at the earlier of its queue-entry time plus
the queue limit and its total query deadline. If a slot opens before then, the
query runs with only the remaining total time. If either limit expires while
it waits, it receives a query timeout and owns no retained queue or snapshot
resources. Client cancellation also removes it promptly. If all 1,000 waiting
places are occupied, the next
query receives a clear, retryable queue-full overload response. Shutdown,
role loss, a class the node cannot execute at all, and genuine resource or
security faults remain distinct from temporary saturation. The same behavior
is visible through HTTP, gRPC, and the first-class SDKs.

### REQ-010 — Shared Bifrost memory and server headroom

The process or pod memory limit comes from the operating system or deployment
runtime. Wyrd leaves at least 1 GiB of that limit for `wyrd-server` work
outside governed Bifrost memory by default; the operator may increase that
minimum. It is accounting headroom, not preallocated memory or an upper limit
on other server work. The shared Bifrost limit defaults to the detected limit
minus the server minimum and may be configured lower. A configuration that
cannot leave both the server minimum and a usable Bifrost budget fails
startup. An 8-GiB pod therefore defaults to a 7-GiB Bifrost limit. Other
server work may use any memory Bifrost has not consumed. The operator settings
are WYRD_SERVER_MEMORY_MIN_BYTES for the minimum and
WYRD_BIFROST_MEMORY_LIMIT_BYTES for an optional lower Bifrost cap. The old
unmanaged-reserve setting has no compatibility alias.

Scribe, Oracle, Forge, and in-flight Bifrost transport work share one governed
memory budget and return their charges when ownership ends. An idle role
reserves no fixed share. The per-request transport size, per-query execution
ceiling, finite query queue, and Forge execution parallelism remain controls
for their separate resources; none partitions the shared memory budget.
Forge uses a bounded DataFusion memory pool with ordinary spill support. A
resource failure cannot publish a partial compaction and remains retryable
through its durable work lifecycle.

A queued Oracle query owns no execution memory. A running query charges
consumers as they grow. A fallible memory refusal or exhausted spill fails
the requesting query, drains its child work, and returns its slot without
failing siblings. An admitted query's resource exhaustion uses
WYRD_VALA_503_QUERY_RESOURCES_EXHAUSTED before a stream opens or in its
Failed terminal after streaming begins; it is never a query-queue refusal.
First-class SDKs do not automatically retry that error. DataFusion's infallible `grow()` is
accounted as headroom and released later; it does not trigger a second
cancellation policy. Untracked allocations and one infallible growth can
still exhaust an in-process pod. A separately deployed Oracle uses the
existing server target, not a new execution process or query protocol.

### REQ-011 — One capacity owner without speculative memory refusals

The shared Bifrost governor is the only authority for the process's governed
memory cap. It counts a buffer once while that buffer is held; transferring the
buffer between transport, Scribe, Oracle, or Forge transfers its charge rather
than charging it again. A request's predicted Arrow output, a future Parquet
write, a table's future lifecycle, a queued query, and a fixed footer allowance
hold no memory credit. Scribe has no separate 90-percent memory breaker or
second global byte ledger. Tenant/table ingest scheduling and bounded work
remain, without reserving future byte capacity for an active table.

Before write ACK, the existing configurable wire-request ceiling and its
derived expanded-data ceiling validate the request. Every buffer Wyrd creates
is charged while held, and a failed fallible charge refuses the write before
ACK. OTLP's opaque decoder is the sole narrow exception: its allocation-free
preflight may temporarily charge the generated-request backing and decode
scratch it is about to allocate internally. That same charge follows the
decoded request into Scribe; scratch returns when decoding ends. It does not
include projected future Arrow output. The exception does not create a second
capacity owner or a new configurable limit. A size-valid request must not be
ACKed if its materialization has failed. After ACK, a failed staging attempt
retains queryable WAL authority and retries; an estimated future writer
workspace cannot refuse or invalidate the acknowledged write.

Scribe live followers use the receiving pod's shared governed DataFusion pool
and the leader-owned stream lifetime without a separate follower permit or
estimated memory pool. Oracle metadata reads hold no fixed 40-MiB memory slot;
retained decoded metadata is charged by actual held bytes. Oracle peers keep
the receiving pod's real running-slot reservation because several leaders can
send work to one pod. The only work an Oracle peer reserves or runs is one
distributed Analytical graph; there is no separate fragment-worker path or
worker quantum. Every query holds exactly one slot unit on each node it runs
on, whatever its class, so the class selects capacity rules and never the
charge. They do not maintain another peer-waiter limit or poll
for slots: a genuine pre-accept capacity refusal carries retry timing, and
only the leader may retry it within the same query deadline. Ambiguous or
accepted work is never retried as a capacity refusal.

The existing node-wide storage-I/O concurrency bound remains a work bound,
not a query admission or memory charge. A request waits for I/O capacity under
its existing operation deadline and cancellation rather than failing
immediately when every permit is occupied. No independent storage wait
timeout, query queue, or memory reservation is added. A genuine deadline,
cancellation, or backend failure retains its existing terminal semantics.

### REQ-012 — Useful production telemetry without shadow state

Scribe, Oracle, and Forge telemetry must let an operator, human maintainer, or
agent follow a write, query, or maintenance attempt through its actual terminal
outcome. Traces describe work that really occurred and span its real lifetime,
including streamed query work and remote fragments. Normal high-rate internal
transitions do not each produce production-level success events. A failure has
one clear event with its reason and correlation context.

Prometheus reports operational demand, backlog, active work, latency, physical
data flow, and real failures from their production owners. Telemetry does not
maintain a second resource, cache, staging, or query state machine merely to
reconcile its own counters. Zero-only or no-op signals, impossible label
combinations, duplicate measurements of one event, and work performed solely
to manufacture a metric are removed. Tenant, table, query, task, and object
identities remain out of metric labels and may appear only as scrubbed trace
context. Keep Forge's earned closed metric catalog and existing tracing,
Prometheus, and OTLP infrastructure; introduce no new exporter or sampler.
Operational metrics remain usable when trace sampling is configured.

### REQ-013 — CPU-derived query parallelism, memory-only grants

Every Oracle leader,
Oracle peer, and distributed stage session sets its `DataFusion` target
partitions from the node's effective CPU and the pinned input's locality before physical planning:
one partition per core for fully local input, rising linearly to four per core
for fully remote input, never below two. Available memory, the admitted grant,
and the pinned file count do not change the partition count. Batch size is the
engine's fixed default rather than a memory-derived value, and the optimizer's
join preference is the engine default.

The admitted grant supplies only the query's memory limit; every other
execution option, including the sort-merge reservation, is the engine
default. Every query's memory
limit is half the pod's managed Bifrost budget, never below 256 MiB and never
above the budget. It is not divided by concurrent load: queries compete for
the one shared pod pool, which refuses growth once concurrent queries fill it.
Every query's spill share is half the pod's scratch limit. Planning happens
once; admission does not reshape or rebuild the physical plan. The retired
32-MiB minimum-grant planning shape has no replacement.

Every Wyrd pod has at least 4 GiB of memory. Boot refuses a detected pod
below that floor. Test and benchmark pod envelopes model that floor or more.

Every session that can spill, whether an Oracle leader, a leader-local live
fragment, or a remote Oracle follower, spills only into its node's governed
Oracle spill directory, under its query's spill share. The leader and its
live fragments share one query spill budget. A Scribe follower owns no Oracle
spill directory and never spills. A memory refusal names the consumer that
asked and reports the pod's largest current holders.

Spill merges keep `DataFusion`'s default fan-in; the per-query memory limit
is the only memory bound. A query whose sort or merge needs
more non-spillable memory than that limit fails with the typed
`QueryResourcesExhausted` error naming its largest consumers; Oracle does not
estimate data shape to avoid it.

Pinned published and hot scan leaves honor the session's partition count. The
pinned files are laid end to end and divided into contiguous, equal byte
ranges, one per partition; each row group is read by exactly the partition
whose range contains its midpoint, so a single large file and many small files
both use every partition and no row is read twice or skipped. Scan telemetry
counts each file once however many partitions read it.

## Invariants and boundaries

- **INV-001:** Write acknowledgment, WAL durability, publication order, and
  Iceberg promotion are unchanged.
- **INV-002:** Authenticated tenant authority, typed peer assignments,
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
- **INV-007:** No idle Bifrost role reserves a fixed share of the shared
  memory budget. Scribe, Oracle, Forge, and transport allocations compete
  through one owner; resource failure releases only its owning work.
- **INV-008:** Only held bytes count toward the shared memory cap, apart from
  the one transferred, temporary OTLP decoder charge for allocations Wyrd
  cannot observe before decoding. Work bounds never masquerade as memory
  charges. Accepted peer reservations retain actual receiving-node slots;
  storage-I/O waits and leader retries remain within the original operation
  or query deadline. Write ACK and query terminal rules are unchanged.
- **INV-009:** Removing telemetry cannot change authorization, durability,
  capacity decisions, query results, cancellation, or maintenance settlement.
  Traces and metrics observe actual owner state; they do not become a second
  authority or an extra read, write, or queue on a hot path.

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
  workload, held-stream, result, measurement, scan, write file-shape, and
  report rules in REQ-008.
  Every required workload has a valid measured result or an explicit failure;
  no invalid run can satisfy a performance target.
- **AC-010:** On the specified 4-CPU/8-GiB node, the measured required
  workloads meet the numeric targets in REQ-008. A missed target, a refused
  acknowledged read-back, or an unproven CPU or snapshot diagnosis prevents
  completion and is reported with its owning boundary and raw evidence.
- **AC-011:** A real-server burst that fills execution slots enters the queue
  without a capacity refusal and can wait longer than 250 ms. The same
  one-hour queue limit and two-hour default total deadline apply to both
  query classes; a caller override changes only the total deadline. A waiter
  runs if admitted before either limit, otherwise times out at the earlier
  limit. Execution uses the remaining total time. The 1,000th waiting query
  fits; the next receives queue-full overload. Cancellation and
  timeout free their places. HTTP and gRPC do not shed an otherwise queueable
  authenticated query before Oracle. A 4-CPU/8-GiB process-cluster run with
  the full queue remains under the stated memory ceiling without OOM.
- **AC-012:** An 8-GiB process limit with default configuration yields at
  least 1 GiB of non-Bifrost server headroom and a 7-GiB shared Bifrost
  limit. A lower operator cap and larger server minimum resolve predictably;
  impossible values fail boot. Idle roles hold no fixed memory share.
  Concurrent Scribe, Oracle, Forge, and transport work charge and return one
  bounded total. Resource-exhausted queries fail without becoming queue-full
  or corrupting siblings; failed compaction publishes nothing partial.
  Focused tests, real-server journeys, and the standard mixed benchmark prove
  these behaviors.
- **AC-013:** A wire-valid, expanded-data-valid write is not refused by a
  second Scribe percentage or future-lifecycle byte charge; every owned
  request buffer is counted once and a failed materialization returns no ACK.
  An acknowledged write remains readable and stageable across retry and
  restart. Concurrent footer and storage reads do not fail solely because a
  fixed footer slot or momentarily occupied I/O permit refused immediately.
  Multiple leaders cannot exceed a receiving Oracle's running slots; a
  pre-accept peer refusal is retried only by its leader within the unchanged
  deadline, while ambiguous work is never replayed. Focused ownership tests,
  real-server write/read and peer journeys, and the standard mixed benchmark
  prove the rule.
- **AC-014:** Captured success and failure traces for a Scribe write and
  publication, an Oracle streamed local or peer query, and a Forge task show
  the actual parent operation, relevant child work, one terminal outcome, and
  the identities needed for human or agent diagnosis. Production metric
  snapshots contain no zero-only, impossible, or duplicate series identified
  by the telemetry audit. Scribe staging, shared storage, and Oracle pruning
  do not keep or scan shadow state for telemetry. Existing journey results
  and the standard read/write benchmark remain valid after the cleanup.

- **AC-015:** A query's session partitions equal the CPU/locality formula
  for its pinned input regardless of grant or file count. A single-file and a
  many-file published scan, a hot scan, and a distributed query over split
  leaves return exactly the same rows as the unsplit scan. Admission changes
  only the memory ceiling. The heavy full scan uses
  more than one core, and the standard and heavy benchmarks are re-run with a
  full table. A query's memory limit is half the managed budget whatever the
  concurrent load; a pod below 4 GiB is refused at boot; spill stays
  confined to the governed directory; a query whose memory exceeds its limit
  fails with typed `QueryResourcesExhausted`; and a memory refusal names the
  requesting consumer and the top holders.

## Open material decisions

None. Revision 17's single execution path and one-unit slot charge were
explicitly approved by the user on 2026-09-30. Revision 16's per-query memory limit, 4 GiB pod floor, and governed
follower spill were explicitly approved by the user on 2026-09-30.
Revision 15's CPU-derived parallelism was explicitly approved by the
user on 2026-09-29. Revision 14's telemetry simplification was explicitly approved by the
user on 2026-09-29. Revision 13's capacity-owner clarification was explicitly approved by
the user on 2026-09-29. Revision 12's memory redesign was explicitly approved by the user on
2026-09-29. The peer wording follows approved `SPEC-verified-change-contract`
revision 44. Revision 3 was explicitly approved by the user on 2026-09-26. The user
explicitly accepted the performance work, supplied its numeric targets, and
chose a finite 1,000-query queue and one timeout policy for both query classes
on 2026-09-28.

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
- Revision 6 (2026-09-28): Sets one policy for Interactive and Analytical:
  one-hour maximum queue wait, two-hour default total query deadline, and a
  caller override for total time. Queue and total clocks both apply, with no
  execution reset. Approved by the user on 2026-09-28.
- Revision 7 (2026-09-28): Records the user's tighter, explicit point-read,
  analytical, ingest, mixed-workload, and concurrency benchmark targets.
  Approved by the user on 2026-09-28.
- Revision 8 (2026-09-28): Adds the user-requested single-file/many-file
  cold/hot Parquet scans, decoded throughput and narrow-row-rate targets,
  projection/predicate and Iceberg pruning evidence, and write file-shape
  metrics. Approved by the user on 2026-09-28.
- Revision 9 (2026-09-28): Clarifies cache hot/cold versus Scribe hot data and
  makes the standard benchmark a 20–30-minute qualification using one public
  ingest, with broader sweeps reserved for diagnosis. Approved by the user on
  2026-09-28.
- Revision 10 (2026-09-28): Removes the hot/cold cache-state benchmark conditions.
  Scan benchmarks use the configured object store through the ordinary query
  path; the default remains local. Approved by the user on 2026-09-28.
- Revision 11 (2026-09-28): Makes common OLAP workloads the standard benchmark,
  removes the file-layout and projection cross-products, and separates the
  100-million-row heavy-scan qualification from the quick capacity run.
  Approved by the user on 2026-09-28.
- Revision 12 (2026-09-29): Records the maintainer-approved shared Bifrost
  memory and server-headroom redesign, removes idle role partitions, and
  aligns private-peer wording with the approved mTLS and typed-context
  contract. Approved by the user on 2026-09-29.
- Revision 13 (2026-09-29): Makes the single held-byte owner explicit,
  removes speculative Scribe and footer memory refusals, retains only the
  narrow opaque-decoder charge, and distinguishes receiving-node running
  slots and storage-I/O backpressure from duplicate query admission.
  Approved by the user on 2026-09-29.
- Revision 14 (2026-09-29): Requires production telemetry to describe actual
  owner state and operation lifetimes, removes duplicate and misleading
  signals, and keeps traces readable by humans and agents. Approved by the
  user on 2026-09-29.
- Revision 15 (2026-09-29): Replaces the memory-derived 32-MiB, two-partition
  planning shape with CPU/locality session partitions, a fixed
  batch size, memory-only grants, and partition-honoring byte-range scan
  leaves, after the heavy full scan measured about one core on a four-core
  node. Approved by the user on 2026-09-29 ("I approve.").
- Revision 16 (2026-09-30): Replaces the load-divided grant with
  a fixed per-query limit (half the managed budget, 256 MiB floor),
  sets the per-query spill share to half the scratch limit, removes any
  memory term from partitions, adds the 4 GiB hard pod floor, routes every
  spilling session through the governed spill directory, and requires
  consumer-named memory refusals. After the 4 GiB spill journey showed four
  merging partitions holding about 288 MB each and refusing the query, it
  caps spill-merge fan-in per query from its memory limit and partition
  count (user chose "Add cap" and required it to scale with pod size on
  2026-09-30). Follows the diagnosis that the Analytical spill journey
  failed because unbounded multi-level spill merges held non-spillable
  reservations that starved sibling sorters, not because of fair vs greedy
  pooling. Approved by the user on 2026-09-30 ("the hard requirement for
  running wyrd/bifrost is to always use at minimum 4 Gib", "ok go ahead. do
  not add the cap for now. address all other directives and findings").
- Revision 17 (2026-09-30): Deletes the unused Oracle fragment-worker path
  (its attempt buffer, worker grant, and worker quantum): an Oracle peer
  reserves and runs only distributed Analytical graphs, and a peer
  reservation must name its graph. Every query now charges one slot unit per
  node regardless of class, replacing the two-unit Analytical weight; memory
  remains bounded by the per-query grant and the shared root. Approved by the
  user on 2026-09-30 ("roll this deletion in with all other consoldiation
  work"; "yes. drop it. our aim is to simplify without degrading
  performance").
- Revision 18 (2026-09-30): Deletes the spill-merge fan-in cap that
  revision 16 recorded and 16224df5b implemented against the user's recorded
  "do not add the cap for now". The cap turned a file count into bytes with a
  fixed batch-size guess, which is wrong for data shapes Bifrost cannot know.
  Spill runtimes use `DataFusion` defaults, bounded only by the
  per-query memory limit. A query exceeding that limit fails with a typed
  resource error. Approved by the user on 2026-09-30 ("agree on 1 and 2. and
  for 3 the decision is "fail with a typed resource error"). The
  grant-sized sort-merge reservation (`OracleSessionShape::for_grant`) is
  deleted with it; the reservation is the `DataFusion` default ("delete"). AC-015
  no longer requires an over-limit sort to complete; the peer baseline's
  forced-spill fixture (6 KiB keys, sort-input lower bound, spill counters)
  is deleted ("delete it"; "i want all invented complexity gone").
- [Repository rules](../../../AGENTS.md),
  [agent rules](../../../architecture/agent-rules.md),
  [Wyrd design](../../../architecture/wyrd-design.md),
  [Wyrd doctrine](../../../architecture/wyrd-doctrine.mdx), and
  [Bifrost design](../../../architecture/bifrost-design.md).
