---
id: TASK-002
title: Restore Bifrost read capacity and prove it with understandable OLAP benchmarks
kind: implementation
status: proposed
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 11
requirements: [REQ-008, REQ-009]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006]
acceptance: [AC-009, AC-010, AC-011]
depends_on: [TASK-001]
---

## Outcome and Value

A team can run a published, reproducible client-to-server benchmark and see what
one 4-CPU/8-GiB Bifrost node can actually serve for point reads, aggregates,
scans, live reads, and concurrent writes. The query path no longer turns away
ordinary reads because only two catalog snapshots may be opened at once.
When Oracle's execution slots are busy, up to 1,000 authorized queries wait
up to one hour, bounded by their leader-owned total deadline, instead of being
rejected after 250 ms. Remote peer work no longer opens a TLS connection per
fragment. The result is
complete only when valid measurements meet REQ-008; a passing correctness gate
or a fast percentile calculated only from the few accepted queries is not a
performance pass.

### Evidence that motivates the task

The latest local report is `target/bifrost-query-capacity/report.txt` (ignored
benchmark output, commit `e2df6e155`), with raw samples and cgroup snapshots
beside it. On 4 CPUs/8 GiB, a 500/s published-only offer completed 247.1/s
and refused 15,172; a 1,000/s offer completed 168.6/s and refused 49,884.
Successful-only p95 was 14.0/28.6 ms. The server's Oracle admission counter
showed zero refusals; the separate two-permit snapshot step rejected them
before Oracle admission. Snapshot pin means were 6.79/11.04 ms, consistent
with about 295/181 theoretical pins per second at two concurrent pins. This
is strong evidence for the first cap, not proof of later capacity.

The report's `cgroup cpu` verdict for the 1,000/s row is unsupported: CPU use
was about 2.47 of 4 CPUs over 60 seconds and only 2 of 600 periods were
throttled, for 0.389 seconds. The existing classifier calls *any* throttled
period CPU saturation. The four-live-reader rows are invalid: used Interactive
slots were zero during the supposed hold and many holder attempts were
refused. The remote row's roughly 300 ms client p95 includes an Oracle wait of
up to 242 ms; it cannot be attributed to TLS without phase evidence. The
write run came **after** reads, filled the WAL disk, and could read back none
of its acknowledged batches. It proves neither concurrent read/write capacity
nor a root cause for read-back refusal because the pod logs were not saved.

Warehouse precedent is narrower than an unlimited queue: Snowflake queues
when a warehouse lacks resources and exposes a queue timeout; its synchronous
Python `execute()` waits for completion. BigQuery's synchronous
`query_and_wait()` also waits, but BigQuery caps interactive pending queries at
1,000 per project and returns a quota error beyond that; Databricks documents
a 1,000-query warehouse queue cap. These are workload controls, not evidence
that 1,000 retained Wyrd plans fit in 8 GiB; Scenario 2 measures that locally.
Sources: [Snowflake warehouse](https://docs.snowflake.com/en/user-guide/warehouses-overview),
[Snowflake connector](https://docs.snowflake.com/en/developer-guide/python-connector/python-connector-example),
[BigQuery queue](https://docs.cloud.google.com/bigquery/docs/query-queues),
[BigQuery synchronous client](https://docs.cloud.google.com/python/docs/reference/bigquery/latest/google.cloud.bigquery.client.Client),
[Databricks warehouse](https://docs.databricks.com/aws/en/compute/sql-warehouse/warehouse-behavior).

Current fixture: 1,048,576 published rows and one 20-ID `ORDER BY` query. It
cannot substantiate Q1–Q5 or the requested scan rates. The current benchmark
implementation is difficult to review: `load/capacity/run.rs` alone is about
1,875 lines. Rewrite its orchestration for plain, named workloads and reports
while reusing the working launch, client, rate driver, and metric capture.
The required report columns and pass criteria are defined below so the
benchmark cannot silently substitute its old short-query target.

## Owners, Scope, Consumers, and Prohibited Changes

- `vala-bifrost-redux` Oracle owns snapshot preparation, query admission,
  peer dispatch, and phase metrics. `wyrd-server` owns configuration and the
  public route. `wyrd-testing` owns the opt-in process-cluster benchmark.
  `wyrd-client::Bifrost` remains its public caller. `vala-sql` owns its existing
  bounded PostgreSQL pool. Reuse these owners; add no second admission system,
  benchmark-only server route, synthetic in-process query, new benchmark DSL,
  or parallel telemetry catalog.
- Keep snapshot protection, authorized table resolution, tenant isolation,
  publication rules, leader deadline, memory governance, and Success/Degraded/
  Failed terminals. A Failed or wrong result never counts as throughput.
- The 1,000-waiter bound is a per-Oracle-node limit on *waiting* queries,
  shared by Interactive and Analytical classes; running queries do not use
  waiting places. Place 1,001 is a retryable queue-full overload, not an
  attack, timeout, or generic resource failure. Temporary slot saturation
  below that bound is never a 429. A class with no executable capacity on
  this pod is unavailable, rather than a query that can gain capacity by
  waiting. Preserve the existing tenant fairness and Interactive floor.
- Interactive and Analytical share one timeout policy: one-hour maximum queue
  wait and two-hour default total deadline. A valid caller `deadline_ms`
  overrides only the total deadline. No execution timer starts at dequeue.
- The Bifrost child is a **local Linux process** launched by
  `BifrostProcessCluster::start_benchmark` in the existing 4-CPU/8-GiB systemd
  user cgroup. PostgreSQL and the public client driver stay outside it. Docker
  is used only by repository-managed PostgreSQL. Verify the actual child
  cgroup before every full run. A remote-Scribe case uses another ordinary
  process-cluster child; report each child's resources separately.
- Keep `bench:bifrost:query-capacity` opt-in and outside normal CI. Replace
  the existing short-query-only run with the named analytical workloads below;
  keep its old report only as historical evidence.
- No change to public query request, source-selection semantics, query class,
  write acknowledgement timing, signed peer assignments, TLS verification, or
  security checks. Do not make snapshot protection weaker to buy throughput.

## Decisions and Implementation Approach

1. **Remove the redundant snapshot admission.** Delete the
   `OraclePlanner::try_planning` immediate refusal, its semaphore and
   `planning_permits` config/default/validation/wiring. Let the already bounded
   runtime PostgreSQL pool wait for a connection within the leader's query
   deadline; retain Oracle's existing tenant-fair query admission for
   execution. Trace the path from public query entry through snapshot pin and
   admission, and remove every caller that maps `try_planning` saturation to a
   public rejection. A pool wait that reaches the leader deadline is a query
   timeout; cancellation releases the pending pool acquisition. Preserve the
   prepare/guard/revalidate/materialize order and the existing promotion-race
   proof. No second semaphore replaces the deleted one.
2. **Use the queue for ordinary saturation.** Change the existing Oracle
   queue default from 64 to 1,000 waiting queries per node. Retain the
   existing queue-wait cutoff and set `max_queue_wait_ms` to 3,600,000 by
   default. Make Oracle's default total query deadline 7,200,000 ms; a valid
   caller `deadline_ms` replaces it for that query. Set both defaults through
   the server's Oracle runtime configuration, expose them through
   `WYRD_BIFROST_ORACLE_MAX_QUEUE_WAIT_MS` and
   `WYRD_BIFROST_ORACLE_DEFAULT_QUERY_DEADLINE_MS`, and reject zero or
   unrepresentable values at configuration load. Apply the same values to
   both classes. A waiter stops at the earlier of queue entry plus its queue
   limit and the leader's total deadline. Queue wait consumes total time;
   dequeuing does not grant a new execution timer. A full
   1,000-place queue returns the existing retryable query-admission overload
   with an explicit queue-full reason. Keep the existing queue, waiter cleanup,
   tenant fairness, and admission telemetry; change their limits and error
   classification, not their ownership. Do not add a second query queue or
   silently retry inside the SDK. Keep
   deadline/cancellation cleanup, fairness, and the class that cannot run on
   this node as distinct paths. Route public HTTP queries around the global
   `LoadShedLayer` and global 1,024-request `ConcurrencyLimitLayer` that can
   shed an otherwise queueable query before Oracle; keep those protections on
   unrelated routes. The query route's long-lived wait is bounded by Oracle's
   1,000 places and the earlier queue/total deadline. gRPC already reaches
   Oracle directly; verify its transport and client deadlines do not end a query earlier than
   the accepted leader deadline. Capture queue-full separately in telemetry.
3. **Measure snapshot cost before changing its correctness logic.** Report
   time in table identity lookup, Iceberg metadata load, reader guard,
   revalidation, hot-cut lookup, and PostgreSQL pool wait using existing
   tracing/metric conventions with bounded labels. Record calls per query and
   actual Postgres pool wait/usage. The observed 7–15 ms aggregate is not
   proof that any one substep is redundant. Remove duplicate work only when a
   trace and the promotion-race tests prove the same protected cut is kept.
   If none is redundant, retain the ordering and report its cost honestly.
   Use the same request/trace identity across these spans so one slow client
   result can be matched to its server phases. Keep metric labels bounded to
   phase and outcome, never tenant, table, SQL, or query ID.
4. **Reuse remote peer connections.** Replace `Endpoint::connect().await` in
   `oracle/dispatcher.rs`'s per-fragment client creation with a reusable tonic
   `Channel` for each ready peer, following existing channel reuse in Scribe
   listing. Preserve mTLS, peer endpoint/identity checks, message caps,
   deadline, and cancellation. A changed node incarnation or endpoint cannot
   inherit the prior peer's authority. Time connection establishment,
   fragment-open RPC, first remote batch, and final terminal separately.
   Do not attribute the observed 40 ms live difference to TLS until those
   measurements show it. Delete the old connect-per-fragment path once the
   reused transport passes the changed-peer and cancellation cases.
5. **Make the benchmark understandable and valid.** Keep its existing public
   client, `FixedRateDriver`, process-cluster launch, PostgreSQL setup, and
   production recorder. Put a short, readable case list (name, SQL,
   intended rows examined, expected result, target) and one visible run
   sequence at its entry point: start, seed, validate, warm up, measure,
   validate, report. Extend the existing `load/capacity/workload.rs` as the
   single source for fixture rows, SQL, and exact expected answers. Keep
   `schedule.rs` responsible for client launches and timings; keep
   `run.rs` responsible for the visible sequence and report. Remove obsolete
   and duplicate orchestration from `run.rs`; do not add generic workload
   infrastructure or a second percentile implementation. Generate
   deterministic rows in bounded batches through the public write API for
   each fresh process cluster. Treat the standard 10-million-row seed as the
   ingest benchmark: record acknowledged rows/bytes, batch
   latency, and completed file shape during the seed. Do not seed a second
   standalone write fixture. Do not create a cached Parquet seed
   file: it would still require public ingest, publication, and catalog setup
   on each run and would make input validation harder. Record setup/seed/
   publication time separately from measured windows. Preserve raw client
   samples and add the actual pod logs, metric snapshots, and fixture geometry.
   The report computes every field in the metric contract below from those
   saved observations; no hand-entered performance numbers.
6. **Measure normal OLAP scans.** Run the named aggregates and full scan
   through the public query path over published Parquet. Use Oracle's existing
   physical-byte counter and report the actual files and scan time; do not add
   decoded-byte counters, a forced file layout, or a second read path. Extend
   the same fixture to 100 million rows for the separate heavy qualification.
7. **Run and diagnose, then optimize only measured costs.** Execute the
   standard suite first and the heavy qualification separately.
   Attribute each failure to client scheduling, PostgreSQL wait, catalog
   pinning, Oracle admission, peer open, DataFusion execution, CPU, memory,
   storage, or result correctness using raw evidence. Replace the `any CPU
   throttle` classifier with utilization plus fraction and duration of
   throttling, and report an undetermined boundary where evidence is
   insufficient. If the target is missed after steps 1–4, profile the measured
   hot path and fix its root cause in this task; do not raise an unrelated
   permit count, claim a passed benchmark, or add a cache without evidence.
   After a fix, rerun the affected workload at the same client concurrency,
   offer rate, fixture geometry, and cgroup; compare both throughput and
   latency before accepting a claimed gain.

## Benchmark Contract

One command, `mise run bench:bifrost:query-capacity`, runs the standard suite
through `wyrd_client::Bifrost` against a local `BifrostProcessCluster` child
limited to 4 CPUs and 8 GiB. PostgreSQL and the driver remain outside that
limit. Use Wyrd's existing object-store configuration, local by default. The
same benchmark may later be rerun with an existing cloud-storage setting; no
benchmark-specific storage mode or read path is needed.

Seed one registered `vala.datasets.events` table with 10 million deterministic
events through the public write API, then publish it before read-only tests.
Fields are `event_id`, `tenant_id`, `event_time`, `service_id`, `duration_ms`,
and a varying payload. Keep SQL and exact expected answers together in the
fixture. Use a second registered table for concurrent writes so read results
stay stable. Record actual row count, file count, compressed bytes, and setup
time. The seed itself measures batched durable-acknowledgment throughput.

| Workload | What runs | What it proves |
| --- | --- | --- |
| Selective read | One existing event by ID, rotated across the table | Client round-trip cost and selective-read capacity; report separately from OLAP results. |
| Small aggregate | Filter about 100,000 events and `GROUP BY service_id` | Common dashboard query latency and QPS. |
| Medium aggregate | The same `GROUP BY` over about 1 million events at eight clients, then 10 million at one client | Analytical execution at two useful scales without a new query shape. |
| Broad time window | Group the 10-million-row day into hourly buckets | Time-range aggregation. |
| Full scan | Aggregate all 10 million events and reference the varying payload | Physical Parquet scan bytes/s and end-to-end scan time. |
| Batched ingest | The public 10-million-row seed | Durable rows/s, input bytes/s, batch p95, CPU, and memory. |
| Reads during ingest | Repeat four small aggregates and one 1-million-row aggregate while writing to the second table | Whether sustained writes degrade ordinary analytical reads. |
| Remote live read | One held-live interval with the Scribe in the second process-cluster child | Whether distributed live reading holds a real Oracle slot and affects other reads. |

Every query checks its exact result and successful terminal. Failed, wrong,
rejected, timed-out, or late results never count as successful QPS. Measure
client-send-to-complete p50/p95/p99, successful QPS with client concurrency,
physical scan bytes/s for scans, server CPU, peak memory, and rejection counts.
For ingest, measure acknowledged rows/s, input bytes/s, batch p95, and
committed file count and average size. Save raw client samples, server logs,
existing phase telemetry, cgroup CPU/memory samples, and fixture geometry.
Do not add a new telemetry catalog or duplicate counters just for the report.
The human report has one row per workload and concurrency, with a plain
PASS/FAIL/INVALID reason.

Sweep only the selective read and small aggregate at client concurrency
1, 4, 8, 16, 32, and 64. Use 2 seconds warmup plus 10 seconds measurement
for each of those 12 windows (2 minutes 24 seconds total). Run the
one-million-row aggregate at eight clients and the ten-million-row aggregate
at one client, each in one 2+10-second window; run the broad-window and full-scan
queries three times each at one client. Use one 2+10-second remote-live
window. Run the read-only mixed baseline and concurrent-write mixed case at
the same read offer and concurrency, each for 2 seconds warmup plus 30 seconds
measurement. This is about four minutes of fixed windows plus six analytical
completions; report the actual duration of each setup and measurement phase.
Aim for about 10–15 minutes for the standard run after the release binary is
built. A longer run reports its actual time and last completed phase; do not
hide setup time or pretend a partial result passed.

The separate heavy-scan qualification uses the same fixture schema and
generator in a fresh process cluster, seeds 100 million published rows
through public ingest, then runs the broad time-window
aggregate over about 70 million and the full scan over all 100 million.
It checks the existing <2-second completion and >=500 MB/s physical-scan
targets through the same public query path. Invoke the existing benchmark
command with `WYRD_BENCH_HEAVY_SCAN=1` to run this qualification instead of
the standard suite; the ordinary command runs the standard suite. The heavy
qualification is required before claiming the heavy-scan targets pass, but
is not repeated for every quick capacity check. At the minimum passing write
rate, its 100-million-row seed alone takes at least 16 minutes 40 seconds.
Report its actual seed, publication, query, and total times separately.

A missed target is a measured failure. Profile that query or write path and
fix the observed cause; a file-layout experiment, projection/pruning test, or
extra offer rate belongs in that focused diagnosis, not in every standard
benchmark run. Do not infer scanned bytes from fixture size or returned rows;
use the existing Oracle physical-byte counter. Do not call returned rows
"rows examined." For mixed mode, compare each class's p95 with its matching
read-only run at the same offer and concurrency. Held live streams must actually
remain admitted during their stated interval. A refused holder invalidates the
remote-live result.

## Ordered Implementation Scenarios

### Scenario 1 — Ordinary concurrent reads reach the real admission path

**Behavior.** More than two simultaneous authorized snapshot preparations
wait within their leader deadlines instead of receiving an immediate 429;
expired work times out, and actual execution admission remains bounded.

**RED.** Add `capacity::three_concurrent_snapshots_are_not_refused` to the
existing `wyrd-testing` Oracle real-server journey target. Hold an exclusive
transaction lock on the isolated test database's `vala.bifrost_tables`, start
three authorized queries against one table, then release the lock. All three
must reach valid terminals with no immediate planning 429. The current
two-permit path makes the third fail while the first two await the same
catalog read. The lock is test-owned database state, not a production fault
hook. Retain the existing Oracle deadline and catalog-promotion tests.
Focused command:
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -E "test(=capacity::three_concurrent_snapshots_are_not_refused)" --run-ignored=all'`.

**GREEN.** Remove the planning semaphore/config path and let the bounded
PostgreSQL pool wait under the leader deadline. Preserve the protected cut
sequence and existing Oracle execution admission.

**REFACTOR.** Delete dead permit fields, defaults, validations, and config
docs rather than keeping a second admission knob.

### Scenario 2 — Both query classes share queue and total time limits

**Behavior.** Busy execution slots place authorized, executable queries in
the existing tenant-fair queue. The queue holds 1,000 waiters. A waiter can
remain more than 250 ms. Both classes use the same one-hour queue limit and
two-hour default total deadline. A caller `deadline_ms` overrides only total
time. The earlier limit ends queueing with `QueryTimeout` and no retained
state; dequeue does not restart total time. Query 1,001 receives the explicit
queue-full overload; cancellation also frees a place. HTTP and gRPC see the
same rule without an earlier HTTP load-shed response.

**RED.** Add
`oracle::admission::tests::queued_queries_obey_queue_and_total_deadlines_and_one_thousand_places`
to the existing `vala-bifrost-redux` lib-test owner. The current 64-place
limit and 250 ms timer must fail its 1,000-place and >250 ms assertions. Use
short injected durations to prove queue expiry before total expiry, total
expiry before queue expiry, release after cancellation, and both query classes
without waiting an hour in a test. Also verify the 1-hour/2-hour production
defaults and both environment overrides in server config tests.
Focused command:
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::admission::tests::queued_queries_obey_queue_and_total_deadlines_and_one_thousand_places)'`.
Add `capacity::saturated_query_waits_on_http_and_grpc` to the existing
`wyrd-testing` Oracle real-server journey target; occupy all executable
slots, submit one more valid query through each public transport, then free
capacity after 250 ms and assert valid terminals. With small test-only
configured limits, separately force queue expiry and total deadline expiry;
both must time out without an HTTP edge 503 or a capacity 429. Confirm that
time spent queued reduces the remaining execution time and that both classes
use the same policy. Focused command:
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -E "test(=capacity::saturated_query_waits_on_http_and_grpc)" --run-ignored=all'`.

**GREEN.** Set the existing Oracle queue bound to 1,000, retain its queue-wait
cutoff, set the default queue and total limits, and make full-queue overload
distinct from either deadline expiry. Wire both settings through the existing
server configuration and Oracle boot path.
Keep other routes' edge protection but let public query requests reach the
   Oracle queue without earlier global HTTP shedding. Check the Rust, Python,
   and TypeScript clients' synchronous query timeouts against the accepted
   total deadline, including the two-hour default. A transport timeout must
   not silently end an otherwise valid queued query. Run a full-queue
   process-cluster case on 4 CPUs/8 GiB; the 1,000
waiters must remain below 7 GiB and cancel/release promptly.

**REFACTOR.** Keep the one tenant-fair Oracle queue and the existing
`min(total deadline, queue entry + max queue wait)` rule. Remove old 250 ms
and 30-second defaults and descriptions; add no class-specific timer or
second queue.

### Scenario 3 — The report names the actual bottleneck

**Behavior.** A two-or-more-query burst reports snapshot work and Postgres
pool wait separately from Oracle execution admission. A nearly unused CPU
quota with rare throttles is not labelled “CPU saturated.”

**RED.** Add
`load::capacity::run::tests::capacity_report_distinguishes_cpu_and_query_refusals`
with the previously observed 2/600 throttled periods and 2.47/4 CPU use;
the existing `any throttle` classifier must fail it. The same report test
distinguishes snapshot timeout, execution admission refusal, and memory
refusal. Focused command:
`mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=load::capacity::run::tests::capacity_report_distinguishes_cpu_and_query_refusals)'`.

**GREEN.** Use production phase metrics and pool telemetry, add only missing
substep timings, and correct the report classification. Save per-pod logs and
raw metric snapshots.

**REFACTOR.** Remove report-specific guesses about cause and redundant
metric parsing once the owning metric has a direct interpretation.

### Scenario 4 — Remote Scribe work reuses its authenticated connection

**Behavior.** Two consecutive fragments for one ready peer use the same
secure connection; a peer incarnation/endpoint change cannot reuse it.
Deadlines, cancellation, and peer identity still apply.

**RED.** Add
`oracle::dispatcher::tests::peer_transport_reuses_tls_channel_for_same_ready_node`
with a counting TLS peer that observes one connection for repeated fragments,
then a new authenticated connection after an endpoint/incarnation change.
Focused command:
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::dispatcher::tests::peer_transport_reuses_tls_channel_for_same_ready_node)'`.

**GREEN.** Reuse tonic channels in the existing Oracle peer transport and
measure connect/open/first-batch phases. Keep the existing peer protocol.

**REFACTOR.** Delete per-fragment eager connection code and any duplicate
channel setup; reuse the established Scribe-listing pattern where applicable.

### Scenario 5 — Every benchmark row means what its label says

**Behavior.** Every named workload checks its exact result, fixture size,
physical scan bytes where relevant, offered rate,
held-live occupancy, cgroup limits, and simultaneous writes are checked
before a row can be valid. A wrong result or failed read-back fails the run.
The top-level driver reads as a short sequence of named cases and stages.

**RED.** Add
`load::capacity::run::tests::invalid_capacity_rows_never_count_as_success`
for a rejected live holder, wrong aggregate, missed offer, failed read-back,
missing p50/p95/p99 or QPS concurrency, missing scan bytes, and mixed p95 at
least 20% above its matching read-only baseline. Invalid measurements must be
INVALID; a valid measurement that misses a target must be FAIL. Assert that
the human report includes the read metrics, W1 rows/s,
input bytes/s and p95 acknowledgment time, CPU/memory, and the exact PASS/
FAIL/INVALID reason. The CPU case is covered by Scenario 3. Focused command:
`mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=load::capacity::run::tests::invalid_capacity_rows_never_count_as_success)'`.
Prove the full benchmark uses the public client and local Linux child.

**GREEN.** Reuse the existing harness, driver, recorder, and cgroup capture;
replace the rejected orchestration with the named workload table and visible
run order. Generate the fixture through public writes in bounded batches;
record setup time separately. Run the selective and small-aggregate concurrency
sweeps, the named analytical queries, the public-ingest seed, one remote-live
case, a matching read-only baseline, and simultaneous reads and writes.

**REFACTOR.** Delete old one-query-only assumptions and duplicate report/
fixture logic. Keep SQL, expected result, and target together so a reviewer
can inspect a case without following a generic workload graph.

### Scenario 6 — Heavy scans use the ordinary query path

**Behavior.** The separate 100-million-row qualification reports exact
broad-window and full-scan answers, measured physical scan bytes, client
completion time, CPU, memory, and its seed/publication duration. Missing
scan bytes or a wrong answer cannot pass the scan target.

**RED.** Add
`load::capacity::run::tests::heavy_scan_requires_exact_result_and_physical_bytes`
to the existing `wyrd-testing` lib-test owner. A wrong count, missing physical
byte counter, or missing seed duration must invalidate the heavy result.
Focused command:
`mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=load::capacity::run::tests::heavy_scan_requires_exact_result_and_physical_bytes)'`.

**GREEN.** Reuse the standard fixture generator for a fresh 100-million-row
public ingest, then run the two heavy SQL
queries through the existing client and process cluster, and calculate scan
rate from Oracle's physical-byte counter and each query's completion time.
Keep the heavy timing separate from the standard suite.

**REFACTOR.** Reuse the standard report format and remove benchmark-only
layout and projection machinery.

### Scenario 7 — Meet and substantiate the performance targets

**Behavior.** The standard suite and separate heavy qualification on the
specified hardware meet AC-010 with valid results. The report identifies the first
measured limit for every miss and does not claim success from fast accepted
queries while most offers are refused.

**RED.** The current full report is red: Q1 cannot reach 500/s, held-live
rows are invalid, analytical workloads and heavy scans are absent, concurrent
read/write is absent, and read-back failed. Retain that baseline alongside
the new run. A failure of any required target remains red rather than being
recast as “benchmark implemented.”

**GREEN.** Run the standard suite and heavy qualification on this machine,
capture raw evidence, profile any measured slow path, and make the minimum root-cause
change needed for every remaining required target. Rerun affected benchmarks
after each change and both complete modes before presenting final performance
evidence. Keep all correctness journeys green.

**REFACTOR.** Delete any temporary probes or optimization bypasses. Retain
only reusable production telemetry and the readable benchmark evidence.

## Acceptance Criteria

1. `planning_permits` and its immediate `try_acquire` 429 path are gone;
   waiting uses the leader deadline and PostgreSQL's existing bounded pool.
   Snapshot promotion, tenant protection, cancellation, and execution
   admission still pass their journeys.
2. The default Oracle queue holds 1,000 waiters across classes. Both classes
   use a configurable one-hour maximum queue wait and two-hour default total
   query deadline. A caller `deadline_ms` overrides total time only. Queue
   wait and planning consume total time; execution gets what remains.
   Saturation below 1,000 produces no early HTTP, gRPC, planning, or Oracle
   429/503. Queue full is retryable
   overload, timeout is `QueryTimeout`, and both release resources. A full
   queue stays below 7 GiB on the specified node.
3. Remote Oracle transport reuses authenticated channels; changed peer
   identity does not reuse an old authority. Trace evidence assigns connect,
   open, and first-row latency correctly.
4. The standard benchmark uses the specified local child, public client, and
   one 10-million-row public-ingest fixture. It reports the selective read,
   small and medium aggregates, broad time-window aggregate, full scan,
   batched ingest, concurrent read/write, and one remote-live case with exact
   results. It sweeps selective and small-aggregate concurrency at 1, 4, 8,
   16, 32, and 64 clients. Each human-readable row states client p50/p95/p99,
   successful QPS and concurrency, physical scan bytes/s where relevant,
   server CPU, peak memory, and rejection or wrong-result counts. Ingest rows
   also state acknowledged rows/s, input bytes/s, batch p95, and completed
   file count and average size. Raw samples, server logs, telemetry, cgroup
   evidence, geometry, and phase durations are saved. The standard run aims
   for about 10–15 minutes after build; actual duration is reported.
   No cached Parquet seed or new storage mode is created.
5. Numeric REQ-008 targets pass on valid full runs, including Q1
   p50 <2 ms/p95 <5 ms/p99 <10 ms and >1,000 QPS, Q2 >=100 QPS with
   p95 <100 ms, the one-million-row aggregate >=20 QPS with p95 <300 ms,
   the ten-million-row aggregate p95 <300 ms, heavy Q4/Q5 <2 s,
   heavy Q5 physical scan >=500 MB/s,
   W1 and mixed ingest >=100,000 acknowledged rows/s, mixed analytical reads
   >=100 QPS, and mixed read p95 degradation <20% with peak memory <7 GiB
   and no OOM. A separate 100-million-row heavy qualification is
   required for the heavy targets and reports its actual duration. Otherwise
   this task remains incomplete with a named
   measured bottleneck. Never weaken the target,
   drop a query class, move the driver into the pod, use Docker for Wyrd, or
   label invalid results as capacity.

## Expected Write Set and Consumer Closure

- Oracle planner/config/callers and snapshot phase instrumentation:
  `crates/vala/vala-bifrost-redux/src/oracle/`,
  `crates/wyrd/wyrd-server/src/config.rs`, server boot/config docs.
- Existing peer dispatcher and its focused tests; existing
  `wyrd-server/src/oracle/tail_discovery.rs` is the reference for channel
  reuse, not another feature to rewrite.
- Existing `wyrd-testing/src/load/capacity/`, its binary and
  `bifrost/process_cluster.rs` only where needed, plus the one existing mise
  benchmark command. Production telemetry is reused; new phase labels are
  low-cardinality.
- Existing Oracle physical-byte scan accounting and production telemetry;
  do not add benchmark-only scan counters.
- `architecture/bifrost-design.md` and performance docs record the removed
  snapshot limit, retained resource boundaries, and measured limits. Do not
  rewrite unrelated SDK contracts or the completed TASK-001 behavior.

## Verification and Evidence

1. Implement every scenario above, using TDD and running each named focused
   test as its behavior is completed. Verify planned selectors with
   `mise exec -- cargo nextest list` before relying on them. PostgreSQL-backed
   tests use the repository setup wrapper or their owning `mise` lane. Keep
   the existing fixed-rate driver and process-cluster tests where their
   behavior still applies. The existing benchmark report is the baseline;
   do not rerun it before the remediation is implemented.
2. After all scenarios are implemented, run the new benchmarks first:
   `mise run bench:bifrost:query-capacity` for the standard OLAP suite, then
   `WYRD_BENCH_HEAVY_SCAN=1 mise run bench:bifrost:query-capacity` for the
   separate 100-million-row qualification. Run them one at a time on this
   Linux machine. Save reports, raw samples, pod logs, and cgroup evidence.
   A failed target remains a measured failure; diagnose it, make the smallest
   justified fix, and rerun the affected benchmark before presenting results.
3. Present the new benchmark results and bottleneck changes to the user for
   review. Incorporate requested revisions and rerun affected focused tests
   and benchmarks. Do not proceed to the broad gate until the user is
   satisfied with the bottleneck remediation and benchmark evidence.
4. Then run `mise run fmt`, `mise run lints`, and `mise run gate` one at a time.
   `mise run gate` already includes `verify:bifrost`; do not run that lane
   separately. Resolve any failures, complete the evidence table below, and
   close out the task. Do not check huge raw samples into Git.

The final evidence table has one row per acceptance criterion and workload:
command, commit, SQL/fixture checksum and geometry, cgroup proof, offered and
actual QPS, success/refusal/wrong-result counts, latency, physical scan rate,
planning time, write ACK/read-back, files/s and
average size, CPU/memory, validity, and PASS/FAIL. Include a separate phase
and CPU profile for any optimized path. Do not report a causal diagnosis from
one aggregate timing or a single throttled period.

## Material Stop Conditions

- Removing the redundant planning permit exposes an unbounded request path
  that the existing leader deadline, PostgreSQL pool, and query admission do
  not bound: stop and revise the architecture with evidence; do not add an
  unexplained replacement semaphore.
- A proposed speedup requires skipping snapshot protection, weakening tenant
  or peer checks, changing ACK semantics, or adding a public source mode:
  return for a specification revision.
- A required target cannot be reached without a material hardware, storage,
  public-contract, or consistency change: preserve the failing report and
  request a decision. Do not call the task complete.

## Authority Links

- [Approved revision 11](../spec.md): REQ-008–REQ-009, INV-001–INV-006,
  AC-009–AC-011.
- [Repository rules](../../../../AGENTS.md),
  [Bifrost architecture](../../../../architecture/bifrost-design.md),
  [testing map](../../../../TESTING.md).
- [Earlier benchmark scope and evidence](../review/task-001-codex-rereview-20260927/TASK-001-R2-bound-staged-cancellation-and-close-source-rules.md).
