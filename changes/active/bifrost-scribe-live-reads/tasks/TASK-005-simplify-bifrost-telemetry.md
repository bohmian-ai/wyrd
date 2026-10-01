---
id: TASK-005
title: Make Bifrost telemetry truthful, small, and readable
kind: implementation
status: proposed
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 24
requirements: [REQ-012]
invariants: [INV-001, INV-002, INV-004, INV-006, INV-009]
acceptance: [AC-014]
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
   including retry, restart, stall, Degraded, and queue transitions.

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
Add `scribe::staging_runtime::pg_tests::restored_stage_republishes_backlog`
to prove restored staging appears in the backlog. Durably stage known members with
their persisted byte counts and ready times, then construct a replacement
`ScribeStagingRuntime` over the retained stage namespace. Install a fresh
isolated metrics recorder before calling the actual async
`ScribeStagingRuntime::restore` with repository-managed Postgres; the staged
families must be absent before restore and present afterward with count,
encoded bytes, oldest timestamp, and outstanding claims equal to the
recovered durable members. Use a current-thread async test so the local
recorder covers the awaited restore. `ScribeHotStage::recover` or
`StagingAssembler::restore` alone does not prove Scribe startup emission.
A test-injected metric cannot pass.
Retain the existing write/read restart regression. Run:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=telemetry::scribe_hot_path_telemetry_reconciles)'"
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
4. Existing write, query, and Forge journeys keep their results. The
   standard benchmark is not run by this task; AC-014's benchmark clause is
   checked by the caller's single benchmark run after the change.
5. Every row in the dashboard measurement contract has a real production
   family or trace/durable owner, a documented unit and boundary, and a
   focused assertion that compares it with the client result or owner state.
   The published Oracle journey proves that request opening precedes the
   Gate stream terminal. Test-only metric values and zero-registered
   families do not satisfy this criterion. No high-cardinality labels are
   added.

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

Run the exact focused commands in Scenarios 1–4 after adding their
assertions, then the unit tests of the touched `vala-bifrost-redux` modules.
After a fix, re-run only what failed. Finish with `mise run fmt`,
`mise run lints`, `mise run docs:check` if docs changed, and
`git diff --check`.

Do not run benchmarks, `mise run gate`, or whole journey lanes for this task.

Append a compact dashboard table to this task: for each operator question
above, the final production family and labels (or trace), its unit and
meaning, and the focused test that checks it. Add one short captured success
and failure trace per role (Scribe write, Oracle query, Forge task) so a
reader can see the story the trace tells. Record that request attempts
include retries and that process counters are not exact durable accounting
after a restart.

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
- Stop if a journey changes result, ACK, terminal, or recovery semantics. Do not weaken an assertion solely to remove telemetry.

## Authority Links

- Approved [Bifrost spec revision 24](../spec.md): REQ-012, INV-009, AC-014.
- [Bifrost telemetry architecture](../../../../architecture/bifrost-design.md).
- [Repository rules](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md),
  [testing workflows](../../../../architecture/references/languages/testing-workflows.md).

## Implementation Evidence

Status: `IMPLEMENTED`. Commits on `vcc/task-004`: `6ec15730c` (Scribe),
`2abefa64d` and `c57398a49` (Gate/Oracle), `68ee0244c` (shared storage),
`04e2ab499` (Forge), `0abb76304` (single failure event per write trace).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| 1. Write, local/remote query, and Forge traces read in order; one terminal outcome; failure reason once | `bifrost.gate.write` root; `bifrost.gate.query.stream` → `bifrost.gate.query` → `bifrost.oracle.query` → `bifrost.oracle.stream`/`bifrost.oracle.peer.fragment{role,outcome}` (`oracle/query_stream.rs`, `oracle/dispatcher.rs`, DataFusion `JoinSetTracer` in `oracle/telemetry.rs`); catalog commits nest under `bifrost.forge.task.execute`; `IngestError::report_internal_at_edge` replaces the mapping-time log | `telemetry::scribe_hot_path_telemetry_reconciles` (one WARN/ERROR per failed write *trace*), `published::…` phase 1b, `peer_network::analytical::remote_live_scribe_drop_releases_query`, `live_rewrite::forge_promoted_files_rewrite_and_remain_exact_across_recovery` 3b/3d | PASS |
| 2. Metrics report owner facts; shadow, zero-only, duplicate families removed; Forge 17-family catalog and HPA metric kept; Degraded ≠ Success; waiting ≠ active; replay not new rows; restored staging in backlog | Scribe mirrors/registry removed, `memtable_rows_inserted_total` at insertion, staging gauges from `StagingAssembler`, `lane_queued` waiting-only; Oracle zero-only families and phase duplicates removed, `degraded` outcome, active gauge after admission; storage ledger removed (`storage/telemetry.rs` stateless); Forge `metrics.rs` unchanged | Scenario 1 three commands; `write_read::scribe_undialable_private_peer_degrades_live_coverage` (degraded=1, success=0); `capacity::saturated_query_waits_on_http_and_grpc` (queued=2 vs active from Oracle runtime); `forge::metrics::tests::forge_telemetry_is_closed_bounded_and_balanced` | PASS |
| 3. No telemetry ledger, pruning walk, or telemetry-only settlement read decides or delays work | `MetadataCacheSnapshot`, Scribe ingress mirror, `FilePruningSource` per-file loop deleted; Forge `record_task_execution_telemetry` takes the committed `ForgeTaskResult` | Storage unit lane (20/20), `forge::` lib lane (86), Forge journey | PASS |
| 4. Existing write, query, Forge journeys keep results | No ACK, WAL, Iceberg, admission, terminal, tenant, settlement, or SDK contract change | All Scenario 1–4 commands plus `write_read::scribe_write_flush_read_user_journey`; R1: `mise run gate` (whole Scribe, Oracle, Forge journey lanes) and `bench:bifrost:query-capacity`, see R1 verification | PASS |
| 5. Every dashboard row has a real family/trace owner and focused assertion; request opening precedes stream terminal; no high-cardinality labels | Dashboard table below; labels are bounded enums only | `published::…` phase 1b asserts request success at open with zero stream terminals, then one terminal | PASS |

Commands run (all PASS, final tree): the exact commands in Scenarios 1–4;
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib` over
`storage::`, `forge::`, `scribe::staging_runtime::`, `gate::`, `oracle::`;
`wyrd-server --lib otlp`; `wyrd-testing --lib`; `mise run fmt`;
`mise run lints`; `mise run docs:check`; `git diff --check`. Benchmark,
`mise run gate`, and whole journey lanes: see R1 verification below.

Material notes:

- RED was observed rather than produced by reverting: captured traces before
  the fixes showed the remote fragment as a trace root, a Degraded terminal
  counted as success, and a failed write trace with two failure events
  (`Bifrost write failed` WARN plus `Scribe ingest failed after transport
  validation` ERROR on the child span).
- Peer trace context is not propagated across processes (protocol change, a
  stop condition); remote work is traced by the leader's fragment span.
- Forge keeps its durable read only where this owner cannot know what
  committed: a slot-fatal error, a release matching no row, or a claim retained
  after an effect (`ShutdownRetained`). Known outcomes use the settlement.
- Storage footer decodes run outside the request guard and are not counted in
  `bifrost_storage_requests_total`; `ScribeStorageDrainObservationV1` now
  serializes lifecycle plus five live counts only.
- Request attempt counters include idempotent client retries. Process counters
  restart at zero and are not exact durable accounting after a restart;
  durable batch, file, and task rows answer that question.

### Dashboard measurement contract

| Operator question | Production family {labels} or trace | Unit and meaning | Focused test |
| --- | --- | --- | --- |
| Are writes arriving and getting durable responses? | `bifrost_gate_requests_total{operation,outcome}`, `bifrost_gate_request_duration_seconds`, `bifrost_gate_rejections_total{reason}`; `bifrost_scribe_ack_seconds`; `bifrost_scribe_wal_append_total`/`_bytes_total`/`_seconds`, `bifrost_scribe_wal_fsync_total{outcome}`/`_seconds` | Counts/seconds of request attempts (retries included) and WAL work | `telemetry::scribe_hot_path_telemetry_reconciles` |
| Is new data entering a shard and moving out of memory? | `bifrost_scribe_memtable_rows_inserted_total`; `bifrost_scribe_active_memtable_bytes`, `_immutable_memtable_bytes`, `_immutable_generation_count`; `bifrost_scribe_lane_queued{lane}` (waiting), `_lane_active{lane}` (running) | Rows newly inserted this process; bytes resident; jobs waiting vs running | `telemetry::scribe_hot_path_telemetry_reconciles`, `write_read::acknowledged_rows_survive_stage_pressure_and_restart` |
| Is staging or publication falling behind? | `bifrost_scribe_staging_live_members`, `_live_bytes`, `_oldest_member_timestamp_seconds`, `_outstanding_claims`; `bifrost_scribe_publication_files_total`, `_bytes_total`; `bifrost_scribe_seal_failed_total` | Current backlog from assembler ownership (restored included); committed publication output | `scribe::staging_runtime::pg_tests::restored_stage_republishes_backlog`, `write_read::acknowledged_rows_survive_stage_pressure_and_restart` |
| Is Forge keeping up? | `bifrost_forge_pending_tasks{task_type}`, `_oldest_pending_task_timestamp_seconds{task_type}`, `_active_tasks{task_type}`, `_task_attempts_total{task_type,result}`, `_output_files_total`/`_output_bytes_total{task_type}`, compaction debt; trace `bifrost.forge.task.execute{result}` | Backlog, age, committed attempt result, output | `live_rewrite::forge_promoted_files_rewrite_and_remain_exact_across_recovery` |
| Are clients getting answers promptly? | `bifrost_gate_query_streams_total{outcome}`, `bifrost_gate_query_stream_duration_seconds{outcome}`; trace `bifrost.gate.query.stream` | Server-edge stream terminal and lifetime; request success means stream opened | `published::published_cache_pruning_and_shutdown_are_production_governed` (phase 1b), `write_read::scribe_undialable_private_peer_degrades_live_coverage` |
| Where is query work waiting or failing? | `oracle_queries_queued`, `oracle_queries_active`, `oracle_admission_total{class,outcome,reason}`, `oracle_admission_queue_duration_seconds`, `oracle_query_duration_seconds{class,outcome}` (HPA), `oracle_query_cancellations_total{reason}`, scan files/bytes counters; storage `bifrost_storage_metadata_cache_effects_total{effect,reason}`, `bifrost_storage_requests_total{operation}`, `_request_terminals_total{operation,outcome}`, `_request_seconds`, `_request_retries_total`, `_active_requests`; trace `bifrost.oracle.peer.fragment{role,outcome}` | Waiting vs admitted work, queue wait, Oracle execution by class and true outcome, scan and storage I/O (a cache hit adds no request) | `capacity::saturated_query_waits_on_http_and_grpc`, `published::…`, `peer_network::analytical::remote_live_scribe_drop_releases_query`, `storage::cache::tests::metadata_cache_reconciles_single_flight_identity_and_bypass` |

### Captured traces (test production capture; ids truncated)

Scribe write — success (`telemetry::scribe_hot_path_telemetry_reconciles`):

```text
trace=912a18a9 bifrost.gate.write        29.7ms outcome=success
trace=912a18a9 └ dispatch_native_frame   29.1ms
```

Scribe write — failure (WAL sync fault, after `0abb76304`):

```text
trace=9efba549 bifrost.gate.write        14.1ms outcome=failed events=[WARN "Bifrost write failed" error=…]
trace=9efba549 └ dispatch_native_frame   13.3ms events=[]
```

Oracle query — local success (`published::…`):

```text
trace=5b5b2c03 bifrost.gate.query.stream 30.6ms outcome=success
trace=5b5b2c03 └ bifrost.gate.query      15.6ms operation=query
trace=5b5b2c03   └ bifrost.oracle.query  10.3ms
trace=5b5b2c03     └ bifrost.oracle.stream 18.3ms outcome=success
trace=5b5b2c03       └ bifrost.oracle.source 3.7ms outcome=success
```

Oracle query — remote failure (`remote_live_scribe_drop_releases_query`):

```text
trace=ccd400a9 bifrost.gate.query.stream 27.0ms outcome=failed
trace=ccd400a9 └ bifrost.gate.query      21.7ms
trace=ccd400a9   └ bifrost.oracle.query  17.5ms
trace=ccd400a9     ├ bifrost.oracle.peer.fragment 10.8ms role=scribe outcome=failed events=["h2 protocol error: …"]
trace=ccd400a9     └ bifrost.oracle.stream 12.8ms outcome=failed status=Error events=["Oracle query stream execution failed"]
trace=ccd400a9       └ bifrost.oracle.source 7.9ms outcome=success
```

Forge task — released failure, then recovery success
(`live_rewrite::forge_promoted_files_rewrite_and_remain_exact_across_recovery`):

```text
trace=89dcbb03 bifrost.forge.task.execute 1704ms result=<none> events=[WARN "Forge compaction plan failed"]
trace=89dcbb03 └ bifrost.forge.catalog.commit ×3 (156/1610/1637ms) result=failed
               WARN "Forge task released: an operation's acceptance is unknown…"
trace=c54985ba bifrost.forge.task.execute 157ms result=succeeded events=[]
               INFO log for the run: only "Forge worker started" / "Forge worker stopped"
```

### R1 dashboard evidence (production samples, TASK-005-R1)

Every value below was printed by a focused journey from the installed
production recorder (`BifrostTelemetryCapture` renders the same
`PrometheusHandle` the `/metrics` route serves) and is asserted against the
independent fact in the same row. Counters and histograms are window deltas;
`peak` is the highest gauge value sampled during the window; `final` is the
gauge when the window closed. Raw `evidence …` lines (journeys run with `--no-capture`) are preserved
under `review/task-005-review-20261001/r1-outputs/`.

| Question | Family {labels} or trace, unit, boundary | Before → after (production sample) | Independent fact | Focused test |
| --- | --- | --- | --- | --- |
| Are writes arriving and getting durable responses? | `bifrost_gate_requests_total{operation="write",outcome}` (attempts), `bifrost_gate_request_duration_seconds` (server-edge seconds), `bifrost_gate_frames_total{status}`, `bifrost_gate_frame_bytes_total`; `bifrost_scribe_ack_seconds` (ACK attempts incl. replays); `bifrost_scribe_wal_append_total{outcome}`, `_append_bytes_total`, `_fsync_total{outcome}` | Write window: requests{success} +4, duration count +4 (sum 0.0677 s), frames{accepted} +4, frame bytes +2336, ACK count +4 (sum 0.0504 s), WAL append{success} +8, append bytes +15332, fsync{success} +8. Retry window: requests{success} +1, ACK count +1, rows_inserted +0, WAL +0. WAL-fault window: requests{failed} +1, frames{rejected} +1, append{success} +1, fsync{failed} +1, ACK +0 | 4 client ACKs for 64 rows; same-batch retry ACKed; faulted write returned an error to the client | `telemetry::scribe_hot_path_telemetry_reconciles` PASS |
| Is new data entering a shard and moving out of memory? | `bifrost_scribe_memtable_rows_inserted_total` (rows newly inserted, this process); `bifrost_scribe_active_memtable_bytes`, `_immutable_memtable_bytes`, `_immutable_generation_count` (age-tick sampled); `bifrost_scribe_lane_queued{lane}` (waiting), `_lane_active{lane}` (running) | Write: rows_inserted +64. Retry: +0. Abrupt-restart resend of an ACKed batch: +0. Freeze: active bytes peak 25120 → final 0; immutable bytes final 25120, generations final 1; lane queued/active peak 0, final 0 | `memtable_stats().writable_rows` = 64 after writes, 0 after freeze; readback returns the 64 rows | `telemetry::scribe_hot_path_telemetry_reconciles`, `telemetry::staged_backlog_survives_abrupt_restart` PASS |
| Is staging or publication falling behind? | `bifrost_scribe_staging_live_members`, `_live_bytes`, `_oldest_member_timestamp_seconds` (Unix s), `_outstanding_claims` (assembler ownership, pod aggregate); `bifrost_scribe_staging_claims_published_total`, `bifrost_scribe_publication_files_total`, `_bytes_total` (committed output) | Restart journey, held below target/dwell: [members, bytes, oldest, claims] = [1, 3464, 1790876177, 0] before kill → [1, 3464, 1790876177, 0] on the replacement before publication → [0, 0, 0, 0] after publication. Hot path publish window: staging peak [1, 3732, 1790876898, 0] → final [0, 0, 0, 0]; claims_published +1, files +1, bytes +3949 | Owner `StagingBacklog` equal at each scrape (same oldest persisted `ready_at` 17:36:17Z across restart); replacement serves all 48 rows before publication; committed `vala.file_list`: 1 file, 3949 bytes, 64 rows (hot path) and 48 rows (restart) | `telemetry::staged_backlog_survives_abrupt_restart`, `telemetry::scribe_hot_path_telemetry_reconciles`, `scribe::staging_runtime::pg_tests::restored_stage_republishes_backlog` (fresh recorder: absent → 4 members, durable bytes, persisted oldest, 1 claim), `scribe::staging_runtime::tests::concurrent_transitions_publish_the_final_backlog` PASS |
| Is Forge keeping up? | `bifrost_forge_pending_tasks{task_type}`, `_oldest_pending_task_timestamp_seconds{task_type}` (per planning pass), `_active_tasks{task_type}`, `_tasks_created_total`, `_task_attempts_total{task_type,result}` (committed result), `_input/_output_files_total`, `_bytes_total`, `_compaction_debt_files/_bytes`; trace `bifrost.forge.task.execute{result}` | Journey window: created scribe_promotion +4, small_files +1, orphan_cleanup +5; attempts{succeeded} the same; promotion in/out 5 files, 18175 bytes; small_files in 1 file 3615 B → out 1 file 5196 B; pending peak small_files 1, orphan_cleanup 2; debt 3 files / 10845 B. Recovery window: attempts{small_files,succeeded} +1, input 1/3615, output 1/5196, active peak 1 | Landed Iceberg snapshot of task `01a0f88f-e2c8…`: removed 1 file 3615 B, added 1 file 5196 B; durable unsettled `vala.forge_tasks` = 0 at end | `live_rewrite::forge_promoted_files_rewrite_and_remain_exact_across_recovery` PASS |
| Are clients getting answers promptly? | `bifrost_gate_query_streams_total{outcome}` (stream terminal), `bifrost_gate_query_stream_duration_seconds{outcome}` (server-edge lifetime), `bifrost_gate_requests_total{operation="query"}` (stream opened), `bifrost_gate_active_streams`; trace `bifrost.gate.query.stream` | Parked after first batch: requests{query,success} +1, streams_total +0, stream duration count +0, active streams final 1. After terminal: streams{success} +1, stream duration 0.0290 s, Oracle duration 0.0177 s | Client read 1 row and its terminal; client clock 0.0311 s ⊇ Gate 0.0290 s ⊇ Oracle 0.0177 s | `published::published_cache_pruning_and_shutdown_are_production_governed` PASS |
| Where is query work waiting or failing? | `oracle_queries_queued{class}` (waiting), `oracle_queries_active{class}` (admitted), `oracle_admission_total{class,outcome,reason}`, `oracle_admission_queue_duration_seconds{class}`, `oracle_query_duration_seconds{class,outcome}` (HPA), `oracle_query_files/bytes_scanned_total`, `oracle_query_rows_total`; trace `bifrost.oracle.peer.fragment{role,outcome}` | Held queue: scraped queued 2, active 4. Granted: admission{admitted} +2, queue-wait count 2 / sum 0.527 s (≥ 2 × 0.25 s hold), duration{success} +2. Expired: admission{rejected,queue_deadline} interactive +4, analytical +2; duration{failed} +6; queued final 0. Published query: files scanned +1, bytes +59, rows +1 | Owner `oracle_runtime_inspection`: queued 2, admitted 4 while held, then queued 0; 24 rows over HTTP and gRPC each; 6 typed `QUERY_TIMEOUT` refusals; 1 published object | `capacity::saturated_query_waits_on_http_and_grpc`, `published::…` PASS; remote failure trace under `peer_network::analytical::remote_live_scribe_drop_releases_query` |

Trace parentage captured in these runs (full ids):

```text
query  trace=2451aa9005c98d06a28f39b1db4e3235
  ff5c7b9956ef8acf bifrost.gate.query.stream  29.2ms outcome=success   parent=root
  f9f9523acb0c1338 └ bifrost.gate.query        13.5ms                    parent=ff5c7b99…
  a7468bcbf2dadcc3   └ bifrost.oracle.query     7.9ms                    parent=f9f9523a…
  000588b018c025ee     └ bifrost.oracle.stream 17.9ms outcome=success   parent=a7468bcb…
  7ee0a4f3332a333b       └ bifrost.oracle.source 2.5ms outcome=success parent=000588b0…
forge  trace=80dc9285c65d2a381c26b08fdbb9cf46 (released, uncertain commit)
  fc563754112b3ff3 bifrost.forge.task.execute 1702ms result=<none> events=[WARN]
  f14020baa9f6ff68 └ bifrost.forge.catalog.commit 1614ms result=failed
  7d2914ee3eabf268 └ bifrost.forge.catalog.commit 1635ms result=failed
forge  trace=0b4520e73cf8656c1dc3b47d90cb08c4 (recovery)
  2b1bc7eb41c31087 bifrost.forge.task.execute 171ms result=succeeded events=[]
write  trace=9b1d86f5e48b003d99eb890a00dc7ac1 (success)
  6f694da8ab9f14e4 bifrost.gate.write    26.4ms outcome=success     parent=root
  271d463246fdfe00 └ dispatch_native_frame 25.7ms batch_id=01a0f894-d1c2-…
write  trace=7b0dacc6c0d25485b8b3250e6102b07f (WAL fault)
  de7b731c81659915 bifrost.gate.write    13.3ms outcome=failed events=[WARN] parent=root
  dfd823cf5996d1be └ dispatch_native_frame 12.5ms batch_id=01a0f894-d584-…
```

Timing and accounting gaps, stated rather than reconciled:

- Gate stream duration starts at the server edge when the stream opens;
  Oracle duration starts later at admission and ends inside it; the SDK's
  client clock adds transport and decode. Measured 0.0311 ≥ 0.0290 ≥ 0.0177 s.
- Request and ACK counters are attempts: a same-batch retry adds one request
  and one ACK but no inserted row. Process counters restart at zero, so they
  are not durable lifetime totals; `vala.file_list` and `vala.forge_tasks` are.
- Memtable bytes and Forge pending/age gauges are sampled state. The memtable
  gauges move on the Scribe age tick, so the freeze sample waits for that
  tick. The Forge pending gauges move after each complete fenced planning
  pass (`forge.svx`), so the journey's final pending 1 + 2 was the last
  pass's view while durable unsettled tasks were already 0.
- The short persistence jobs in the hot path never showed waiting or running
  work in the sampler (lane peaks 0). The held-queue proof is the Oracle
  queue (2 waiting, 4 active) and the held stage backlog above.
- The recorder survives the in-process restart, so the unchanged restored
  value proves exposition, not re-emission; the fresh-recorder restore test
  proves the replacement emits it.

Production family inventory (hot-path journey exposition; series count
excludes histogram buckets): 105 families. Bifrost and Oracle families:
`bifrost_cluster_roles_live`(2), `bifrost_forge_*` active_tasks(5),
compaction_debt_bytes(1), compaction_debt_files(1),
oldest_pending_task_timestamp_seconds(5),
oldest_planning_demand_timestamp_seconds(1), pending_tasks(5),
planning_demands(1); `bifrost_gate_*` active_requests(2), active_streams(1),
frame_bytes_total(1), frames_total(2), query_stream_duration_seconds(2),
query_streams_total(5), rejections_total(18), request_duration_seconds(6),
requests_total(8); `bifrost_oracle_analytical_*` attempts_active(1),
attempts_total(3), exchange_batches_total(1), exchange_bytes_total(1),
output_sort_spilled_bytes_total(1), output_sort_spilled_rows_total(1),
output_sort_spills_total(1), stage_authority_total(10),
stage_operations_total(2); `bifrost_oracle_files_pruned_total`(1),
`bifrost_oracle_local_bytes`(2), `bifrost_oracle_local_slot_units`(2);
`bifrost_parquet_upload_bytes`(1), `_outcomes_total`(1);
`bifrost_resource_*` acquisitions_total(6), current_bytes(3),
memory_bytes(7), planned_bytes(3), scratch_bytes(1); `bifrost_role_ready`(5);
`bifrost_scribe_*` ack_seconds(2), active_memtable_bytes(1),
immutable_generation_count(1), immutable_memtable_bytes(1),
ingress_active(1), lane_active(3), lane_job_seconds(6), lane_jobs_total(4),
lane_queued(3), memtable_rows_inserted_total(1),
persistence_encoded_bytes_total(1), persistence_jobs_total(1),
persistence_publication_seconds(2), persistence_queue_depth(1),
publication_bytes_total(1), publication_files_total(1),
queue_wait_seconds(2), rejections_total(6), retired_bytes_total(1),
retirements_total(1), staging_claims_published_total(1),
staging_live_bytes(1), staging_live_members(1),
staging_oldest_member_timestamp_seconds(1), staging_outstanding_claims(1),
wal_append_bytes_total(1), wal_append_seconds(2), wal_append_total(1),
wal_fault_total(1), wal_fsync_seconds(4), wal_fsync_total(2);
`bifrost_storage_*` active_requests(1), metadata_cache_effects_total(2),
metadata_cache_inflight_loads(1), metadata_cache_loads_total(1),
metadata_cache_resident_bytes(1), metadata_cache_resident_entries(1),
request_terminals_total(3), requests_total(3); `oracle_*`
admission_queue_duration_seconds(2), admission_total(12),
queries_active(2), queries_queued(2), query_bytes_returned_total(2),
query_bytes_scanned_total(2), query_cancellations_total(2),
query_duration_seconds(2), query_files_scanned_total(2),
query_partitions_scanned_total(2), query_phase_seconds(10),
query_row_groups_pruned_total(2), query_row_groups_scanned_total(2),
query_rows_total(2), query_time_to_first_batch_seconds(2). The remaining 16
are server, pool, and storage-adapter families (`wyrd_http_*`,
`wyrd_postgres_pool_*`, `vala_postgres_pool_*`, `wyrd_storage_*`). No family
carries a tenant, table, batch, shard, task, or query identity label.

Removed families (present as emitters at base `05d7d7413`, absent from
source emitters and from the exposition above): `bifrost_gate_events_total`,
`bifrost_gate_rows_total`, `bifrost_scribe_frames_total`,
`bifrost_scribe_rows_total`, `bifrost_scribe_ingress_watermark_bytes`,
`bifrost_scribe_persistence_compression_ratio`,
`bifrost_scribe_persistence_queue_bytes`,
`bifrost_scribe_seal_stage_seconds`, `bifrost_scribe_staging_effects_total`,
`bifrost_query_duration_seconds`, `bifrost_oracle_analytical_exchanges_active`,
`oracle_query_logical_bytes_selected_total`, `oracle_query_spill_bytes_total`,
`oracle_query_spill_files_total`, `oracle_query_spill_queries_total`,
`oracle_tenant_budget_pressure`,
`bifrost_storage_metadata_cache_transition_anomalies_total`,
`bifrost_storage_metadata_cache_waiters`. Added:
`bifrost_scribe_memtable_rows_inserted_total`,
`bifrost_scribe_publication_files_total`,
`bifrost_scribe_publication_bytes_total`,
`bifrost_scribe_staging_live_bytes`,
`bifrost_scribe_staging_oldest_member_timestamp_seconds`.
