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
A human or agent can read one captured trace and follow the actual write,
streamed query, or maintenance attempt through its terminal outcome. Routine
high-rate work no longer produces several redundant production events or
updates a telemetry-only copy of service state. The existing benchmark still
reports valid read/write results.

Forge's closed 17-family catalog is the useful precedent. This task is a
cleanup of Scribe, Oracle, shared storage, and Forge trace emission, not a new
telemetry platform. Start after TASK-004 has integrated its in-progress
Scribe resource changes; preserve those edits and their tests. TASK-004's
benchmark and gate evidence is not evidence for this later cleanup.

## Owners, Scope, Consumers, and Prohibited Changes

| Owner | Remove or correct | Keep |
| --- | --- | --- |
| Scribe: `scribe/{telemetry,ingress,wal,shards,persistence,execution_lanes,memory,mod}.rs` | The 31-field ingress lifecycle mirror; nine-effect staging registry, duplicate live gauges, and routine per-transition `INFO` events; successful per-append/fsync `INFO` spans; the publication span that does not enclose work; last-file compression-ratio gauge; static watermark series; persistence queue bytes where the lane queue and root already report occupancy. Correct the persistence-stage histogram: its documented `parquet/put/sql_commit` split is not what production records. Remove stale contention-telemetry consumers after TASK-004 deletes the contention ledger. | ACK/accepted-row latency and count, real refusals, WAL fault and fsync, actual memory occupancy, useful queue depth, stage/publication backlog and failure, and a concise recovery summary. One correlated ingest or generation operation and its terminal outcome remain readable. |
| Oracle: `oracle/{mod,telemetry,exec,pruning,query_stream,dispatcher,analytical}.rs` | Zero-only tenant-pressure, query-spill, and analytical-active-exchange families; no-op slot/security recorders; impossible admission class/outcome/reason zero series; duplicate `bifrost_query_duration_seconds`; first-row and terminal phase duplicates; unused phase variants and excessive per-substep phase samples; the per-Iceberg-file loop run only to manufacture pruning telemetry; duplicate failure and successful attempt logs; spans that end before the streamed work they claim to describe. | `oracle_query_duration_seconds` (used by the production HPA), query outcome, queue wait, actual admission refusal, active work, physical scan facts, actual output-sort spills, and real peer failures. One query trace covers stream polling and terminal cleanup, with real peer/fragment children. |
| Shared storage: `storage/{telemetry,cache,mod}.rs` | The telemetry-only reconciliation arrays, anomaly book, mutex-held shadow totals, per-cache-effect debug event, and routine republishing of five gauges for each logical request. | Cache hit/miss, request latency/outcome, retained bytes, and any active-work value obtainable from the real owner. A diagnostic never becomes another cache or request authority. |
| Forge: `forge/{metrics,scheduler,worker,orphan_gc}.rs` | Duplicate successful claim/settlement `INFO` logs and routine per-hint/pass detail. Avoid the extra Postgres read made solely to learn each settled task's telemetry outcome: use the committed settlement result. | The existing 17-family catalog, bounded labels, task and catalog-commit spans, failure events, durable task authority, and readiness. Never infer a durable result from Rust `Ok` alone. |
| Consumers: server metrics registration, `wyrd-testing` telemetry/benchmark capture and journeys, Forge operator docs, self-hosting guide, Bifrost telemetry architecture | Retire references and assertions for deleted families, trace strings, and snapshot-only test APIs; rebind the query p99 report to `oracle_query_duration_seconds`. | Existing Prometheus, tracing, OTLP, test-capture, and `sample_ratio` infrastructure. The HPA keeps its Oracle query metric. |

Do not change a write ACK, WAL retirement, Iceberg snapshot, admission,
query terminal, tenant check, Forge settlement, or public SDK contract to
make telemetry simpler. Do not add an exporter, sampler, instrumentation
framework, telemetry-only state machine, or compatibility metric alias.
Tenant/table/query/task/object identities may be scrubbed trace fields,
never Prometheus labels. A trace must be useful to humans and agents:
stable operation name, correlation identity, elapsed work, and one terminal
outcome; detailed successful substeps belong at `DEBUG` only when they
explain a real diagnostic.

## Approach

1. Capture one current successful and failed write, local and remote query,
   and Forge task using the existing production-shaped test telemetry.
   Record metric families/series and `INFO` event counts. These are the
   comparison, not a reason to preserve redundant output.
2. Remove Scribe and shared-storage shadow state and routine emission.
   Point test inspection at the production owner or durable state; keep the
   useful operational metric at its actual transition.
3. Make Oracle metrics represent real work, eliminate telemetry-only file
   scanning, and attach query/peer spans to actual streamed lifetime.
4. Keep Forge's metric catalog, trim duplicate successful tracing, and remove
   the telemetry-only settlement read without changing durable outcome
   authority.
5. Update metric/report/doc consumers, run the focused scenarios, then the
   existing standard benchmark. Review measured outcomes and trace readability
   before running the broader verification gate.

## Ordered Implementation Scenarios

### Scenario 1 — Scribe remains diagnosable without lifecycle mirrors

**Behavior.** A client writes, receives a durable ACK, reads live rows,
flushes and publishes, then reads the same rows. The trace identifies the
request and generation, has one real operation lifetime and terminal
outcome, and shows a clear failure reason when WAL or staging fails. Normal
success does not emit a log for each internal transition. Metrics report ACK
latency, accepted work, real backlog and failures from their owners.

**RED.** Update the existing
`telemetry::scribe_hot_path_telemetry_reconciles` real-server journey to
assert the durable outcome and captured trace instead of
`ScribeStagingSnapshot` and the ingress lifecycle ledger. Include a
controlled failure and assert one correlated failure event; assert that
ordinary success emits no per-transition `INFO` series. The present
registry and WAL spans fail those trace expectations. Run:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=telemetry::scribe_hot_path_telemetry_reconciles)'"
```

**GREEN.** Remove the shadow snapshots and redundant signals named above;
retain the production facts and one useful operation trace. Existing
write/read journeys continue to pass.

**REFACTOR.** Delete tests and test-server inspection APIs that exist only
to reconcile removed telemetry state. Do not rebuild that state in test
support.

### Scenario 2 — Oracle telemetry describes the query that actually ran

**Behavior.** A local published query and a remote live query each have one
query trace that lasts until success, failure, or client drop. Remote
fragments appear as causal child work. Query duration and scan metrics
reflect actual work; pruning telemetry does not rescan the pinned file list.
Admission labels describe possible outcomes only, and resource failures are
not disguised by a permanently zero metric.

**RED.** Extend
`published::published_cache_pruning_and_shutdown_are_production_governed`
and `peer_network::analytical::remote_live_scribe_drop_releases_query`
to check trace lifetime and parentage, actual scanned files/bytes, and the
absence of duplicate/zero-only families and telemetry-only pruning work.
The current constructor-only stream span, duplicate duration, and shadow
pruning pass fail. Run both exact selectors:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=published::published_cache_pruning_and_shutdown_are_production_governed)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=peer_network::analytical::remote_live_scribe_drop_releases_query)'"
```

**GREEN.** Retain the production HPA's Oracle duration family; move the
generic query report to it and remove the duplicate family. Delete dead
families and no-op callers, constrain phase metrics to the named performance
boundaries, use actual scan statistics, and keep the query span with the
response stream until terminal cleanup.

**REFACTOR.** Remove obsolete phase variants and telemetry-only pruning tests;
keep one contextual failure event and the existing query/result contract.

### Scenario 3 — Shared storage reports its owner without copying it

**Behavior.** A cache hit, miss, joined load, retry, failure, and cancellation
leave the cache and request owners settled. Published metrics show real
hit/miss, latency/outcome, and retained bytes. No separate telemetry
reconciliation object or per-effect debug stream is required to prove
settlement.

**RED.** Update
`storage::cache::tests::metadata_cache_reconciles_single_flight_identity_and_bypass`
to assert real cache/request owner state and the retained useful metric
series, without `MetadataCacheSnapshot` as an authority. Assert that a
cache effect does not emit a duplicate lifecycle event. The current
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
one trace.

**RED.** Extend
`live_rewrite::forge_promoted_files_rewrite_and_remain_exact_across_recovery`
to assert the committed outcome, one task operation trace, no duplicate
success events, and no extra state lookup solely for telemetry. The current
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
3. No telemetry ledger, extra pruning walk, or telemetry-only settlement
   read decides or delays production work. Write/read results, capacity,
   cancellation, and durable outcomes remain unchanged.
4. The existing standard benchmark remains valid and reports its required
   client latency, throughput, CPU, memory, refusals, scan, and ingest
   evidence. Compare its results and emitted `INFO` events/metric series
   with the recorded pre-change run; investigate any material regression
   before closeout.

## Expected Write Set and Consumer Closure

The owner table gives likely production files. Also inspect and update
`crates/wyrd/wyrd-server/src/app/metrics.rs`,
`crates/wyrd/wyrd-testing/src/{bifrost/telemetry.rs,load/capacity/run.rs,server.rs}`,
`crates/wyrd/wyrd-testing/tests/bifrost/{scribe,oracle,forge}`,
`docs/src/content/docs/bifrost/forge.svx`,
`docs/src/content/docs/self-hosting/kubernetes-production.svx`, and
`architecture/bifrost-design.md`. Search every consumer of a retired
family, snapshot type, and trace name before deleting its definition.
The paths guide closure; they are not an implementation allowlist. Do not
change Python, TypeScript, CLI, MCP, or public wire contracts merely to
remove private telemetry.

## Verification and Evidence

Run the exact focused commands in Scenarios 1–4 after adding their
assertions. Then run the existing Scribe, Oracle, and Forge journey lanes
one at a time. Capture a human-readable success and failure trace for each
role and a Prometheus family/series inventory. The telemetry comparison is
descriptive; do not invent a pass threshold from an unmeasured baseline.

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
