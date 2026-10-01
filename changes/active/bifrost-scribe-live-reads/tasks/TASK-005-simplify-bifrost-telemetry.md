---
id: TASK-005
title: Make Bifrost telemetry truthful, small, and readable
kind: implementation
status: ready
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 14
requirements: [REQ-008, REQ-012]
invariants: [INV-001, INV-002, INV-004, INV-006, INV-009]
acceptance: [AC-009, AC-010, AC-014]
depends_on: [TASK-004]
---

## Outcome and Value

An operator can see whether Bifrost is accepting writes, serving queries,
moving acknowledged data to published storage, and completing Forge work.
The emitted metrics support two understandable dashboards: write progress
from Gate through Scribe staging and Forge, and query demand through queue,
execution, remote reads, and terminal result. Each plotted value has a stated
owner and meaning. Aggregate metrics show
rates and backlogs; correlated traces and durable records explain one
particular batch or query.
A human or agent can read one captured trace and follow the actual write,
streamed query, or maintenance attempt through its terminal outcome. Routine
high-rate work no longer produces several redundant production events or
updates a telemetry-only copy of service state.

Forge's closed 17-family catalog is the useful precedent. This task is a
cleanup of Gate, Scribe, Oracle, shared storage, and Forge telemetry, not a new
telemetry platform. Start after TASK-004 has integrated its in-progress
Scribe resource changes; preserve those edits and their tests.

## Owners, Scope, Consumers, and Prohibited Changes

| Owner | Remove or correct | Keep |
| --- | --- | --- |
| Gate: `gate/{mod,query_stream}.rs` | Delete the stage-event counter that recounts request work. Accepted-row receipts include deduplicated retries, so remove that series as a measure of newly stored data. Request success means the query stream opened, not that it completed. | Request attempts and outcomes, rejection reasons, input bytes and latency, and client-edge query-stream terminal outcome and duration. |
| Scribe: `scribe/{telemetry,ingress,wal,shards,persistence,staging_runtime,execution_lanes,memory,mod}.rs` | Delete the 31-field ingress mirror, nine-effect staging registry, duplicate live gauges, routine per-transition `INFO` events, successful per-append/fsync `INFO` spans, last-file ratio gauge, static watermark series, and redundant persistence queue bytes. Fix the publication span, replay-inflated accepted-row series, restored staged backlog, `lane_queued` semantics, and persistence-stage histogram documentation. Remove stale contention-telemetry consumers after TASK-004. | ACK-attempt count/latency, newly inserted live rows, real refusals, WAL bytes/fsync, actual memory occupancy, waiting and active work, staged backlog and failures, committed publication output, and one correlated operation trace. |
| Oracle: `oracle/{mod,telemetry,exec,pruning,query_stream,dispatcher,analytical}.rs` | Zero-only tenant-pressure, query-spill, and analytical-active-exchange families; no-op slot/security recorders; impossible admission class/outcome/reason zero series; duplicate `bifrost_query_duration_seconds`; first-row and terminal phase duplicates; unused phase variants and excessive per-substep phase samples; the per-Iceberg-file loop run only to manufacture pruning telemetry; duplicate failure and successful attempt logs; spans that end before the streamed work they claim to describe. | `oracle_query_duration_seconds` (used by the production HPA), query outcome, queue wait, actual admission refusal, active work, physical scan facts, actual output-sort spills, and real peer failures. One query trace covers stream polling and terminal cleanup, with real peer/fragment children. |
| Shared storage: `storage/{telemetry,cache,mod}.rs` | The telemetry-only reconciliation arrays, anomaly book, mutex-held shadow totals, per-cache-effect debug event, and routine republishing of five gauges for each logical request. | Cache hit/miss, request latency/outcome, retained bytes, and any active-work value obtainable from the real owner. A diagnostic never becomes another cache or request authority. |
| Forge: `forge/{metrics,scheduler,worker,orphan_gc}.rs` | Duplicate successful claim/settlement `INFO` logs and routine per-hint/pass detail. Avoid the extra Postgres read made solely to learn each settled task's telemetry outcome: use the committed settlement result. | The existing 17-family catalog, bounded labels, task and catalog-commit spans, failure events, durable task authority, and readiness. Never infer a durable result from Rust `Ok` alone. |
| Consumers: server metrics registration, `wyrd-testing` telemetry/benchmark capture and journeys, Forge operator docs, self-hosting guide, Bifrost telemetry architecture | Retire references and assertions for deleted families, trace strings, and snapshot-only test APIs; rebind the query p99 report to `oracle_query_duration_seconds`. | Existing Prometheus, tracing, OTLP, test-capture, and `sample_ratio` infrastructure. The HPA keeps its Oracle query metric. |

Correct these misleading measurements at their owners:

- Scribe's accepted-row receipt includes replayed batches. It is not a
  newly-stored-row counter; preserve the receipt contract, and measure new
  live rows at the successful insertion transition. A process counter cannot
  prove exact durable row totals across restart; use durable batch/file facts
  for that claim. Gate's accepted-row counter has the same replay problem.
- The staged-backlog gauge must include members restored after restart.
  Derive current backlog and age from staging ownership rather than
  process-local staged-minus-retired events. The Scribe `lane_queued` gauge
  currently includes running jobs; report waiting and active work truthfully.
- A Degraded Oracle terminal must not increment Success. The Oracle active
  gauge must represent admitted work, separate from waiting queries. Query
  phase measurements with overlapping or cumulative intervals cannot be
  presented as additive steps. Gate duration measures the client-facing
  stream; Oracle duration starts later and measures Oracle work. Retain both
  meanings and the HPA's Oracle family.
- Keep logical cached storage requests distinct from physical backend I/O.
  Remove duplicate memory occupancy views and repeated acquisition/release
  counters that answer no separate operator question. Forge's existing
  metric catalog remains the reference; no new Forge family is required.
- `bifrost_scribe_seal_rows_total` has a test binding but no production
  emitter, and `bifrost_scribe_seal_stage_seconds` has server histogram
  registration but no producer. Remove those claims and consumers. A
  configured or zero-only series is not evidence of a completed stage.

Do not change a write ACK, WAL retirement, Iceberg snapshot, admission,
query terminal, tenant check, Forge settlement, or public SDK contract to
make telemetry simpler. Do not add an exporter, sampler, instrumentation
framework, telemetry-only state machine, or compatibility metric alias.
Tenant/table/query/task/object identities may be scrubbed trace fields,
never Prometheus labels. A trace must be useful to humans and agents:
stable operation name, correlation identity, elapsed work, and one terminal
outcome; detailed successful substeps belong at `DEBUG` only when they
explain a real diagnostic.
Do not put tenant, table, batch, SQL, query, task, or object identifiers in
metric labels.
Follow one operation with scrubbed, correlated traces and authoritative
durable state. A bounded shard label may be retained only if it answers an
existing operational question; default dashboards aggregate by pod.

### Dashboard questions and measurement contract

Each retained series is emitted by the owner of the fact it measures and
documented with its unit and whether it counts attempts, newly inserted
work, current backlog, or committed outcomes.

The changed Scribe chart contract is fixed here; implementation may choose
how the existing `StagingAssembler` exposes its snapshot, but may not create
a telemetry-only member ledger. All five series below are pod aggregates
without tenant, table, batch, or shard labels:

| Production family | Unit and exact meaning | Source and publication boundary |
| --- | --- | --- |
| `bifrost_scribe_memtable_rows_inserted_total` | Counter of rows successfully inserted into the live memtable during this process lifetime. An idempotent same-process retry adds zero; replay after a process restart can insert rows into the new process and its counter starts over. It is not an exact durable lifetime total. | The shard owner records successful insertion, not the request receipt. Delete `bifrost_scribe_rows_total{status="accepted"}` rather than present receipt rows as new-data volume. |
| `bifrost_scribe_staging_live_members` | Gauge counting durable, unpublished members, both ready and held by outstanding claims; zero when none. | Read the `StagingAssembler` ready and outstanding ownership after a durable stage, claim settlement, or recovery restore. Claim take does not reduce this count. |
| `bifrost_scribe_staging_live_bytes` | Gauge summing the encoded bytes of those same ready and claimed members; zero when none. | Use the members' already-owned encoded byte facts from `StagingAssembler`, not a second byte ledger or a filesystem walk on every scrape. |
| `bifrost_scribe_staging_oldest_member_timestamp_seconds` | Gauge holding the oldest persisted `ready_at` as Unix seconds among the same members; zero when none. Dashboard age is `time() - timestamp` only while live members are positive. No timer updates an age gauge. | Use the persisted ready times held by the assembler, including recovered ready and claimed members. |
| `bifrost_scribe_staging_outstanding_claims` | Gauge counting outstanding claims, zero when none. | Use `StagingAssembler`'s existing claim ownership, including restored claims. |

Publish the owner snapshot after durable member registration, claim take,
claim settlement, and completed staging restoration. Restoration's snapshot
must be visible before Scribe becomes ready to serve writes or reads. A failed restoration
keeps Scribe unready; it must not present a zero backlog as a healthy
recovery. After a successful publish and retirement, the owner snapshot
settles the backlog and claim gauges to zero. Keep the existing
`bifrost_scribe_lane_queued{lane}` name only for jobs waiting to start:
decrement it on worker start and use existing `lane_active` for running jobs.
The existing ACK histogram count means successful ACK attempts, including
replays. No new ACK counter is needed.

| Operator question | Measurements to retain | What it should match |
| --- | --- | --- |
| Are writes arriving and getting durable responses? | Gate request rate, wire bytes, refusal, and duration; Scribe ACK-attempt count/latency and WAL append/fsync. These are request attempts, including idempotent retries. | Actual client responses and WAL-backed durable write behavior. |
| Is new data entering a shard and moving out of memory? | Newly inserted live rows, active/immutable memtable occupancy, and waiting versus running staging work, sampled before and after freeze. Do not infer insertion from a replayed receipt. | Client readback plus the Scribe owner state at each transition. |
| Is staging or publication falling behind? | Current staged-member/byte backlog and age, real stage/claim failures, and committed publication files/bytes; show backlog rising during a controlled stall and settling after recovery. | Recovered staged state and committed file/claim state, including after restart. |
| Is Forge keeping up? | Existing promotion/compaction task backlog, age, outcome, output files/bytes, and debt. | Committed task result and published file state. |
| Are clients getting answers promptly? | Gate query-stream terminal rate and server-edge stream lifetime by outcome; distinguish stream opening from completion. | SDK stream's terminal frame and separately measured client-observed elapsed time. |
| Where is query work waiting or failing? | Oracle queue depth/wait, admitted active work, execution duration by Interactive/Analytical class and true terminal outcome, actual peer failures, scan files/bytes/rows and returned rows. | Admission/terminal result and actual local or remote scan. Only the existing two query classes are production query-type labels; do not invent SQL-shape classification. |

These are aggregate operational charts, not a row-conservation equation:
bytes, rows, files, attempts, and publication can occur at different times
and across restarts. One batch's or query's causal path is checked with a
trace rooted at server ingress for the client operation and correlated child
work, then checked against the durable result. Correlation belongs in traces,
not metric labels.
Server Gate latency omits the client's network and SDK time; do not label it
client-to-client latency or expect it to equal the journey clock.

## Work

The goal is less telemetry code, not more test code. Delete first; correct
only the misleading measurements listed above.

1. **Scribe.** Delete the shadow snapshots, the ingress lifecycle ledger, and
   the redundant signals in the owner table. Count newly inserted rows where
   the insertion happens, rebuild staged backlog from staging ownership so it
   includes restored members, and report waiting and active lane work
   separately. Keep one operation trace per write. Delete the Gate
   stage-event counter and the accepted-row series that counts retries.
2. **Oracle.** Remove the duplicate `bifrost_query_duration_seconds` and move
   its consumers to `oracle_query_duration_seconds` (the HPA metric). Delete
   the zero-only families, no-op recorders, unused phase variants, and the
   per-file loop that exists only for pruning telemetry. Count Degraded on its
   own, count active work only after admission, and keep the query span alive
   until the stream ends.
3. **Shared storage.** Delete the reconciliation arrays, anomaly book, shadow
   totals, per-effect debug event, and per-request gauge republishing. Keep
   cache hit/miss, request latency/outcome, and retained bytes.
4. **Forge.** Delete the duplicate claim/settlement `INFO` logs and the
   telemetry-only Postgres read; take the outcome from the committed
   settlement result. Keep the 17-family catalog.
5. **Consumers.** Update every consumer of a deleted family, snapshot type, or
   trace string: server metrics registration, `wyrd-testing` telemetry
   capture and journeys, `docs/src/content/docs/bifrost/forge.svx`,
   `docs/src/content/docs/self-hosting/kubernetes-production.svx`, and
   `architecture/bifrost-design.md`. Delete tests and test-server inspection
   APIs that exist only to check removed telemetry state; do not rebuild that
   state in test support. Do not change Python, TypeScript, CLI, MCP, or
   public wire contracts.

## Acceptance Criteria

1. The removals in the owner table are done, and no consumer references a
   deleted family, snapshot type, or trace string.
2. The four corrections hold: a replayed batch is not counted as a new
   insertion, restored staging appears in backlog, waiting work is not shown
   as active, and a Degraded query is not counted as Success.
3. Forge's 17-family catalog and the Oracle HPA metric remain. No
   high-cardinality labels are added.
4. Write/read results, capacity, cancellation, and durable outcomes are
   unchanged: the existing tests for the touched code still pass.

## Verification

Run only what the change touches:

- One small unit test at the owner for each of the four corrections in
  acceptance criterion 2.
- The existing unit tests of the touched `vala-bifrost-redux` modules.
- The existing journeys that referenced removed telemetry, after updating
  them, each by its exact nextest command (Postgres-backed ones through
  `scripts/postgres/with-test-postgres.sh`).
- `mise run fmt`, `mise run lints`, `mise run docs:check` if docs changed,
  and `git diff --check`.

Do not run benchmarks or `mise run gate` for this task. Record each command
and its result in a short evidence table below.

## Material Stop Conditions

- Stop if a removed signal is the only production indicator for a durable
  failure or live backlog; retain or replace that one operational fact at
  its actual owner before deletion.
- Stop if tracing the streamed query through terminal requires changing
  leader ownership, peer protocol, result framing, or cancellation semantics
  instead of instrumentation lifetime.
- Stop if Forge settlement cannot expose its committed outcome without a
  new durable write or changed task state; keep the current read and report
  the finding rather than infer an outcome.
- Stop if a change alters result, ACK, terminal, or recovery semantics. Do not weaken an assertion solely to remove telemetry.

## Authority Links

- Approved [Bifrost spec revision 14](../spec.md): REQ-012, INV-009, AC-014.
- [Bifrost telemetry architecture](../../../../architecture/bifrost-design.md).
- [Repository rules](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md),
  [testing workflows](../../../../architecture/references/languages/testing-workflows.md).
