---
id: TASK-005
title: Make Bifrost telemetry truthful, small, and readable
kind: implementation
status: proposed
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
owner and meaning that a real client journey proves. Aggregate metrics show
rates and backlogs; correlated traces and durable records explain one
particular batch or query.
A human or agent can read one captured trace and follow the actual write,
streamed query, or maintenance attempt through its terminal outcome. Routine
high-rate work no longer produces several redundant production events or
updates a telemetry-only copy of service state. The existing benchmark still
reports valid read/write results.

Forge's closed 17-family catalog is the useful precedent. This task is a
cleanup of Gate, Scribe, Oracle, shared storage, and Forge telemetry, not a new
telemetry platform. Start after TASK-004 has integrated its in-progress
Scribe resource changes; preserve those edits and their tests. TASK-004's
benchmark and gate evidence is not evidence for this later cleanup.

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

For every retained dashboard series, record in the task evidence its exact
production family and bounded labels, emitting owner, event or owner state,
unit, and whether it is an attempt, newly inserted work, current backlog,
or committed outcome. Record the chart question it answers and the real
journey assertion that proves its meaning. A family definition, synthetic
metric sample, nonzero fixture value, or span name alone is not proof.

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

| Operator question | Measurements to retain and verify | Independent journey fact |
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

## Approach

1. Capture one current successful and failed write, local and remote query,
   and Forge task using the existing production-shaped test telemetry.
   Record metric families/series, their intended dashboard questions, and
   `INFO` event counts. These are the comparison, not a reason to preserve
   redundant output.
2. Remove Scribe and shared-storage shadow state and routine emission.
   Point test inspection at the production owner or durable state; keep the
   useful operational metric at its actual transition.
3. Make Oracle metrics represent real work, eliminate telemetry-only file
   scanning, and attach query/peer spans to actual streamed lifetime.
4. Keep Forge's metric catalog, trim duplicate successful tracing, and remove
   the telemetry-only settlement read without changing durable outcome
   authority.
5. Update metric/report/doc consumers. Prove the dashboard measurements with
   before/after scrapes and correlated traces from real client journeys,
   including retry, restart, stall, Degraded, and queue transitions. Run the
   existing standard benchmark. Review measured outcomes and trace readability
   before running the broader verification gate.

## Ordered Implementation Scenarios

### Scenario 1 — Scribe remains diagnosable without lifecycle mirrors

**Behavior.** A client writes, receives a durable ACK, reads live rows,
flushes and publishes, then reads the same rows. The trace identifies the
request and generation, has one real operation lifetime and terminal
outcome, and shows a clear failure reason when WAL or staging fails. Normal
success does not emit a log for each internal transition. Metrics report ACK
attempts and latency, newly inserted rows, WAL activity, real backlog and
failures from their owners. A replayed batch has a successful receipt but
does not claim a second insertion. Staged backlog survives restart; a
stalled publication makes that backlog visible while acknowledged rows
remain readable, then settles after recovery.

**RED.** Update the existing
`telemetry::scribe_hot_path_telemetry_reconciles` real-server journey to
assert the durable outcome and captured trace instead of
`ScribeStagingSnapshot` and the ingress lifecycle ledger. Include a
controlled failure and assert one correlated failure event; assert that
ordinary success emits no per-transition `INFO` series. Capture the installed
production recorder before and after write, replay, freeze, and publication;
compare emitted deltas with client receipts, owner state, and committed files.
Add `telemetry::staged_backlog_survives_abrupt_restart` to the existing
Scribe journey binary. Start a one-node `WyrdTestCluster`, ACK a known batch,
freeze it into durable staging below the publication target and dwell without
invoking the final publish, and scrape nonzero member count/bytes and a
persisted oldest timestamp. Abruptly terminate and restart that node over
its retained roots with `WyrdTestCluster::terminate_node_abruptly_for_test`
and `restart_terminated_node_at_new_address`, then scrape the production recorder
after recovery readiness but before releasing or requesting publication:
the restored member count and bytes remain nonzero, the oldest timestamp is
unchanged, outstanding claims reflect actual ownership, and a client can
still read the ACKed rows. Flush publication and scrape again: backlog and
claim gauges reach zero, committed files contain the rows, and a replayed
batch does not add a second insertion in that process. A test-only telemetry
value may not stand in for the scrape. This server journey checks actual
Prometheus exposition and readback, but its recorder is process-global and
survives the in-process node restart: an unchanged post-restart gauge alone
cannot prove the replacement emitted it.

Add `scribe::staging_runtime::pg_tests::restored_stage_republishes_backlog`
as the independent emission-origin proof. Durably stage known members with
their persisted byte counts and ready times, then construct a replacement
`ScribeStagingRuntime` over the retained stage namespace. Install a fresh
isolated metrics recorder before calling the actual async
`ScribeStagingRuntime::restore` with repository-managed Postgres; the staged
families must be absent before restore and present afterward with count,
encoded bytes, oldest timestamp, and outstanding claims equal to the
recovered durable members. Use a current-thread async test so the local
recorder covers the awaited restore. `ScribeHotStage::recover` or
`StagingAssembler::restore` alone does not prove Scribe startup emission.
This focused test and the real-server journey together prove restoration;
neither a stale shared-process sample nor a test-injected metric can pass.
Retain the existing write/read restart regression. Run:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=telemetry::scribe_hot_path_telemetry_reconciles)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=telemetry::staged_backlog_survives_abrupt_restart)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=write_read::acknowledged_rows_survive_stage_pressure_and_restart)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::staging_runtime::pg_tests::restored_stage_republishes_backlog)'"
```

**GREEN.** Remove the shadow snapshots and redundant signals named above;
measure new insertion at its actual owner, rebuild current backlog from
staging ownership, correct waiting/active lane meaning, and retain one useful
operation trace. Keep ACK-attempt measurements distinct from newly inserted
data. Existing write/read journeys continue to pass.

**REFACTOR.** Delete tests and test-server inspection APIs that exist only
to reconcile removed telemetry state. Do not rebuild that state in test
support. Delete the two seal metric claims without production recorders and
the Gate stage-event and accepted-row measurements that duplicate or
misstate physical work.

### Scenario 2 — Oracle telemetry describes the query that actually ran

**Behavior.** A local published query and a remote live query each have one
query trace that lasts until success, failure, or client drop. Remote
fragments appear as causal child work. Query duration and scan metrics
reflect actual work; pruning telemetry does not rescan the pinned file list.
Admission labels describe possible outcomes only, and resource failures are
not disguised by a permanently zero metric. A queued query contributes to
queue depth, not admitted active work; a Degraded terminal has its own
outcome. Gate duration measures the server-side client-facing stream, while Oracle
duration measures the later Oracle execution/stream boundary.

**RED.** Extend
`published::published_cache_pruning_and_shutdown_are_production_governed`
and `peer_network::analytical::remote_live_scribe_drop_releases_query`
to check trace lifetime and parentage, actual scanned files/bytes, and the
absence of duplicate/zero-only families and telemetry-only pruning work.
In the published journey, checkpoint the production recorder and a client
clock before the public SDK query; after the stream opens but before it is
consumed, require one successful Gate request-opening sample and no Gate
stream-terminal sample. Consume the terminal, then require exactly one Gate
stream outcome and one duration-histogram observation matching Success.
Record the client-to-client elapsed time separately; require it to include
the measured server-edge interval, without asserting that the two clocks are
equal. The same distinction applies to the remote query and client-drop
cases. Capture production metric deltas for Degraded, Failed, queue wait,
and remote reads; compare each to the actual terminal frame and admission
state. Extend `write_read::scribe_undialable_private_peer_degrades_live_coverage`
and `capacity::saturated_query_waits_on_http_and_grpc` for the Degraded and
queue transitions. The current constructor-only stream span, duplicate
duration, Degraded-as-Success result, pre-admission active gauge, and shadow
pruning pass fail. Run all exact selectors:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=published::published_cache_pruning_and_shutdown_are_production_governed)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=peer_network::analytical::remote_live_scribe_drop_releases_query)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=write_read::scribe_undialable_private_peer_degrades_live_coverage)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=capacity::saturated_query_waits_on_http_and_grpc)'"
```

**GREEN.** Retain the production HPA's Oracle duration family; move the
generic query report to it and remove the duplicate family. Delete dead
families and no-op callers, constrain phase metrics to the named performance
boundaries, use actual scan statistics, count Degraded independently, and
measure active work only after admission. Keep the query span with the
response stream until terminal cleanup. Retain both Gate server-edge and
Oracle execution durations with their distinct meanings.

**REFACTOR.** Remove obsolete phase variants and telemetry-only pruning tests;
keep one contextual failure event and the existing query/result contract.

### Scenario 3 — Shared storage reports its owner without copying it

**Behavior.** A cache hit, miss, joined load, retry, failure, and cancellation
leave the cache and request owners settled. Published metrics show real
hit/miss, latency/outcome, retained bytes, and physical backend I/O distinct
from logical cached requests. No separate telemetry
reconciliation object or per-effect debug stream is required to prove
settlement.

**RED.** Update
`storage::cache::tests::metadata_cache_reconciles_single_flight_identity_and_bypass`
to assert real cache/request owner state and the retained useful metric
series, without `MetadataCacheSnapshot` as an authority. Assert that a
cache hit does not claim a backend read and a cache effect does not emit a
duplicate lifecycle event. The current
telemetry state machine fails. Run:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=storage::cache::tests::metadata_cache_reconciles_single_flight_identity_and_bypass)'
```

**GREEN.** Remove the shadow ledger and publish only measurements attached
to actual cache/request transitions. Preserve storage retry and close
semantics.

**REFACTOR.** Retire snapshot reconciliation consumers in the Oracle journey,
test server, and workload capture; use the owning cache/request state for
their assertions.

### Scenario 4 — Forge keeps its catalog and loses redundant work

**Behavior.** A claimed task settles against its committed durable state;
the existing 17-family catalog and task/catalog-commit trace remain correct.
Normal success has no duplicate claimed/settled `INFO` records or
telemetry-only Postgres read. Failure and recovery remain explainable in
one trace. Promotion/compaction backlog, outcome, and output files/bytes
match the committed task and published files.

**RED.** Extend
`live_rewrite::forge_promoted_files_rewrite_and_remain_exact_across_recovery`
to assert the committed outcome, one task operation trace, no duplicate
success events, no extra state lookup solely for telemetry, and production
metric deltas at claim and settlement. Compare them with committed task and
file state; a zero-initialized family alone cannot pass. The current
worker logs and lookup fail. Run:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test forge -P journey --run-ignored=all -E 'test(=live_rewrite::forge_promoted_files_rewrite_and_remain_exact_across_recovery)'"
```

**GREEN.** Remove repeated successful events and derive telemetry from the
committed settlement result. Keep failure/recovery visibility and every
earned metric family.

**REFACTOR.** Delete trace-only tests of removed periodic-pass/per-hint
events; leave durable task and public metric assertions intact.

## Acceptance Criteria

1. AC-014's successful and failed write, local/remote streamed query, and
   Forge trace can each be read in order by a person or agent; the top
   operation spans the real work through terminal cleanup. Captured failures
   retain reason and correlation identity exactly once.
2. Scribe, Oracle, and shared-storage metrics report real owner facts.
   Zero-only, impossible, duplicate, and shadow-pass measurements identified
   above are gone; useful ACK, queue, query, scan, storage, and Forge metrics
   remain. Forge's closed 17-family catalog and the Oracle HPA metric remain.
   Gate request and stream outcomes retain their distinct meanings.
   Idempotent replay does not appear as another newly inserted row; restored
   staging appears in backlog; waiting work is not shown as active; and a
   Degraded query is not shown as Success.
3. No telemetry ledger, extra pruning walk, or telemetry-only settlement
   read decides or delays production work. Write/read results, capacity,
   cancellation, and durable outcomes remain unchanged.
4. The existing standard benchmark remains valid and reports its required
   client latency, throughput, CPU, memory, refusals, scan, and ingest
   evidence. Compare its results and emitted `INFO` events/metric series
   with the recorded pre-change run; investigate any material regression
   before closeout.
5. Every row in the dashboard measurement contract has a real production
   family or trace/durable owner, a documented unit and boundary, and a
   focused journey assertion comparing its change with the client result or
   authoritative owner state. The evidence includes rendered production
   samples before and after each transition, relevant trace IDs and parentage,
   and an explanation of any expected attempt-versus-data or timing gap.
   The published Oracle journey proves that request opening precedes the
   Gate stream terminal and that server-edge time sits inside the separately
   measured client journey. The abrupt-restart Scribe journey proves
   production exposition, client readback, and backlog settlement. The
   fresh-recorder staging recovery test proves the replacement actually
   emits the restored backlog before its first post-restore scrape.
   Test-only metric values and zero-registered families do not satisfy this
   criterion. No high-cardinality labels are added.

## Expected Write Set and Consumer Closure

The owner table gives likely production files. Also inspect and update
`crates/wyrd/wyrd-server/src/app/metrics.rs`,
`crates/wyrd/wyrd-testing/src/{bifrost/telemetry.rs,load/capacity/run.rs,server.rs}`,
`crates/wyrd/wyrd-testing/tests/bifrost/{scribe,oracle,forge}`,
`crates/vala/vala-bifrost-redux/src/gate/{mod,query_stream}.rs`,
`docs/src/content/docs/bifrost/forge.svx`,
`docs/src/content/docs/self-hosting/kubernetes-production.svx`, and
`architecture/bifrost-design.md`. Search every consumer of a retired
family, snapshot type, and trace name before deleting its definition.
The paths guide closure; they are not an implementation allowlist. Do not
change Python, TypeScript, CLI, MCP, or public wire contracts merely to
remove private telemetry.

## Verification and Evidence

Run every exact focused command in Scenarios 1–4 after adding its assertions.
Then run the existing Scribe, Oracle, and Forge journey lanes one at a time.
Use the installed production Prometheus recorder and trace capture in the
real-server harness: checkpoint before an action, scrape after the actual
owner transition, and compare the series delta with the client response and
durable or owner fact. During a deliberately held queue or stage backlog,
scrape while work is waiting as well as after it settles; a final zero alone
does not prove the chart can show the problem. Check trace parentage,
correlation, actual span lifetime, terminal outcome, and failure reason
against the same client action. The captured evidence must be readable as a
short ordered write and query story, not only machine assertions.

Append a compact dashboard evidence table to this task. For each question
above, give the final production family/labels or trace, unit and semantic
boundary, before/after sample, independent expected fact, and focused test
result. Mark any unavailable measurement as a task failure rather than
fabricating it from a test fixture. Explicitly record that Gate stream
duration starts at the server edge, Oracle duration begins later, and the
SDK's client-to-client clock includes transport. Record that request attempts
include retries and that process counters are not exact
durable accounting after a restart. Capture a human-readable success and
failure trace for each role and a Prometheus family/series inventory. The
telemetry comparison is descriptive; do not invent a pass threshold from an
unmeasured baseline.
For restored Scribe backlog, cite both the real-server scrape and the
fresh-recorder `ScribeStagingRuntime::restore` test; the shared in-process
recorder's unchanged value is not proof of re-emission.

Run `mise run bench:bifrost:query-capacity` **before** the broad gate, using
the existing local process-cluster setup and normal configured object store.
Do not add a benchmark matrix or run the separate 100M-row qualification
solely for telemetry cleanup. Preserve the raw samples and report. If the
standard benchmark misses an approved REQ-008 target, diagnose it before
declaring the task complete; telemetry savings are not a substitute for
correctness or throughput.

After benchmark acceptance, run `mise run fmt`, `mise run lints`,
`mise run docs:check` if docs change, and `mise run gate` because this
task crosses the Scribe, Oracle, Forge, shared-storage, server, and testing
owners. Gate already includes Bifrost verification; do not repeat
`verify:bifrost`. Record command exits and map each acceptance criterion
to the focused journey, metric inventory, trace capture, and benchmark
evidence.

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
- Stop if a benchmark or journey changes result, ACK, terminal, or recovery
  semantics. Do not weaken an assertion solely to remove telemetry.

## Authority Links

- Approved [Bifrost spec revision 14](../spec.md): REQ-012, INV-009, AC-014.
- [Bifrost telemetry architecture](../../../../architecture/bifrost-design.md).
- [Repository rules](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md),
  [testing workflows](../../../../architecture/references/languages/testing-workflows.md).
