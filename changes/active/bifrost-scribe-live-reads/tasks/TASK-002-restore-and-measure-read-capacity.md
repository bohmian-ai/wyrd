---
id: TASK-002
title: Restore Bifrost read capacity and prove it with understandable OLAP benchmarks
kind: implementation
status: proposed
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 5
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
until their leader deadline instead of being rejected after 250 ms. Remote
peer work no longer opens a TLS connection per fragment. The result is
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
- The Bifrost child is a **local Linux process** launched by
  `BifrostProcessCluster::start_benchmark` in the existing 4-CPU/8-GiB systemd
  user cgroup. PostgreSQL and the public client driver stay outside it. Docker
  is used only by repository-managed PostgreSQL. Verify the actual child
  cgroup before every full run. A remote-Scribe case uses another ordinary
  process-cluster child; report each child's resources separately.
- Keep `bench:bifrost:query-capacity` opt-in and outside normal CI. Keep the
  existing four short-read offers as a comparable diagnostic, but label their
  published/live placement honestly. Add the class and mixed-workload runs
  below; do not treat one query shape as an OLAP benchmark.
- No change to public query request, source-selection semantics, query class,
  write acknowledgement timing, signed peer assignments, TLS verification, or
  security checks. Do not make snapshot protection weaker to buy throughput.

## Decisions and Implementation Approach

1. **Remove the redundant snapshot admission.** Delete the
   `OraclePlanner::try_planning` immediate refusal, its semaphore and
   `planning_permits` config/default/validation/wiring. Let the already bounded
   runtime PostgreSQL pool wait for a connection within the leader's query
   deadline; retain Oracle's existing tenant-fair query admission for
   execution. A pool wait that reaches the leader deadline is a query
   timeout. Preserve the prepare/guard/
   revalidate/materialize order and the existing promotion-race proof.
2. **Use the queue for ordinary saturation.** Change the existing Oracle
   queue default from 64 to 1,000 waiting queries per node. Delete the
   separate `max_queue_wait_ms` 250 ms setting, its translation/wiring, and
   the `min(enqueue + max_queue_wait, leader_deadline)` cutoff; wait only to
   the leader deadline. The existing default is 30 seconds, and a valid
   caller `deadline_ms` replaces it for the whole query. Queue wait consumes
   that budget; dequeuing does not grant a new execution timer. A full
   1,000-place queue returns the existing
   retryable query-admission overload with an explicit queue-full reason.
   Do not add a second query queue or silently retry inside the SDK. Keep
   deadline/cancellation cleanup, fairness, and the class that cannot run on
   this node as distinct paths. Route public HTTP queries around the global
   `LoadShedLayer` and global 1,024-request `ConcurrencyLimitLayer` that can
   shed an otherwise queueable query before Oracle; keep those protections on
   unrelated routes. The query route's long-lived wait is bounded by Oracle's
   1,000 places and leader deadline. gRPC already reaches Oracle directly;
   verify its transport and client deadlines do not end a query earlier than
   the accepted leader deadline. Capture queue-full separately in telemetry.
3. **Measure snapshot cost before changing its correctness logic.** Report
   time in table identity lookup, Iceberg metadata load, reader guard,
   revalidation, hot-cut lookup, and PostgreSQL pool wait using existing
   tracing/metric conventions with bounded labels. Record calls per query and
   actual Postgres pool wait/usage. The observed 7–15 ms aggregate is not
   proof that any one substep is redundant. Remove duplicate work only when a
   trace and the promotion-race tests prove the same protected cut is kept.
   If none is redundant, retain the ordering and report its cost honestly.
4. **Reuse remote peer connections.** Replace `Endpoint::connect().await` in
   `oracle/dispatcher.rs`'s per-fragment client creation with a reusable tonic
   `Channel` for each ready peer, following existing channel reuse in Scribe
   listing. Preserve mTLS, peer endpoint/identity checks, message caps,
   deadline, and cancellation. A changed node incarnation or endpoint cannot
   inherit the prior peer's authority. Time connection establishment,
   fragment-open RPC, first remote batch, and final terminal separately.
   Do not attribute the observed 40 ms live difference to TLS until those
   measurements show it.
5. **Make the benchmark understandable and valid.** Keep its existing public
   client, `FixedRateDriver`, process-cluster launch, PostgreSQL setup, and
   production recorder. Put a short, readable case list (name, SQL,
   intended rows examined, expected result, target) and one visible run
   sequence at its entry point: start, seed, validate, warm up, measure,
   validate, report. Remove obsolete and duplicate orchestration from
   `load/capacity/run.rs`; do not add generic workload infrastructure.
   Preserve raw samples and add the actual pod logs and fixture geometry.
6. **Run and diagnose, then optimize only measured costs.** First rerun the
   four comparable short-read rows; then execute the class and mixed suites.
   Attribute each failure to client scheduling, PostgreSQL wait, catalog
   pinning, Oracle admission, peer open, DataFusion execution, CPU, memory,
   storage, or result correctness using raw evidence. Replace the `any CPU
   throttle` classifier with utilization plus fraction and duration of
   throttling, and report an undetermined boundary where evidence is
   insufficient. If the target is missed after steps 1–4, profile the measured
   hot path and fix its root cause in this task; do not raise an unrelated
   permit count, claim a passed benchmark, or add a cache without evidence.

## Benchmark Contract

Use one registered `vala.datasets` events table with an integer `event_id`,
tenant, event time, service, numeric duration, and a deterministic 16-byte
payload that varies by ID so the scan cannot be satisfied by a repeated
constant; seed deterministically through the public write API. The Q5 query
must actually read that payload column. Use a second registered table for
concurrent writes so the fixed read fixture and expected answers remain stable;
sample read-back from that write table. Full analytical fixture:
100,000,000 events over ten UTC days starting 2026-01-01, ten million per
day, with `event_id` from 0 to 99,999,999, `tenant_id = 1`,
`service_id = event_id % 100`, `duration_ms = event_id % 1000`, and
`event_time = start_of_its_day + floor((event_id % 10,000,000) * 86,400 /
10,000,000) seconds`. Generate the 16-byte payload as
the lower-case hexadecimal representation of a fixed, wrapping 64-bit mix of
`event_id`; its exact mix and expected digest live beside the SQL in the
fixture source. Seed in bounded batches, publish before
read-only windows, and record actual files, row groups, compressed bytes,
time ranges, and Iceberg snapshot for *each* window. A small smoke fixture uses
the same SQL and exact-result rules at reduced row count; smoke is never
performance evidence. If publication/compaction changes file geometry during
a read-only comparison, invalidate and rerun that comparison. Use private
fixture roots on local NVMe; check free space before seeding. Do not silently
shrink the full fixture to avoid a failed target.

| Case | Public SQL shape and data examined | Required result and target |
| --- | --- | --- |
| Q0 trivial | `SELECT event_id FROM vala.datasets.events WHERE event_id = 0 LIMIT 1` | Exact row 0; p95 <20 ms; report QPS, no invented scan count. |
| Q1 selective | `SELECT event_id, service_id, duration_ms FROM vala.datasets.events WHERE tenant_id = 1 AND event_id = ? LIMIT 1`; rotate existing IDs across all ten days | Exact one row; >1,000 successful/s, p95 <=25 ms. Report 5,000/s separately as stretch. |
| Q2 small aggregate | `SELECT service_id, count(*) FROM vala.datasets.events WHERE tenant_id = 1 AND event_time >= ? AND event_time < ? GROUP BY service_id`; 00:00–00:15 UTC on day one | Exact per-service counts; >=100 successful/s, p95 <=100 ms. |
| Q3 medium | `SELECT service_id, count(*), avg(duration_ms) FROM vala.datasets.events WHERE event_time >= ? AND event_time < ? GROUP BY service_id`; 00:00–02:24 UTC on day one, exactly one million rows | Exact aggregate; >=20 successful/s, p95 <=200 ms. |
| Q3b 10M | Same SQL over one complete 10-million-row day | Exact aggregate; p95 <=300 ms; report sustainable QPS. |
| Q4 large window | `SELECT date_trunc('hour', event_time), count(*), avg(duration_ms) FROM vala.datasets.events WHERE tenant_id = 1 AND event_time >= ? AND event_time < ? GROUP BY 1`; days one through seven | Exact buckets; p95 <=500 ms. |
| Q5 full scan | `SELECT count(*), sum(duration_ms), sum(length(payload)) FROM vala.datasets.events` over all 100 million events | Exact result `(100000000, 49950000000, 1600000000)`; each completion <2 s; physical scan >=500 MB/s and examined rows/sec reported from the known fixture. |
| Mixed analytical | Q1 70%, Q2 20%, Q3 9%, Q4 1%, scheduled in a deterministic repeating 100-request cycle; writes to the second table acknowledged concurrently | >=100 successful reads/s, >=100,000 acknowledged rows/s, p50 <50 ms, p95 <=200 ms, p99 <=500 ms, peak pod memory <7 GiB, no OOM; exact read-back of every acknowledged batch after the window. |

For Q1–Q4, do not call returned rows “rows examined.” Use known fixture
selectivity only where exact, or report the value unavailable. Use the existing
`oracle_query_bytes_scanned_total` for physical bytes; never infer physical
scan throughput from logical table size. Q5's >=500 MB/s target applies to a
scan window, not to a point lookup or idle interval. The report must also
state the user-supplied good/excellent reference ranges for the other classes
without converting them into fabricated passes.

Use 5 seconds warmup and 30 seconds measurement for each class/concurrency
pair, except Q5 uses 60 seconds measurement to obtain useful single-client
samples. The full sweep is about 40 minutes of timed windows before seeding,
the four comparison rows, and live/mixed runs; state the total expected time
before launch. Run each read class at client concurrency 1, 4, 8, 16, 32, 64,
and 100. For each, record sent,
completed Success/Degraded/Failed, wrong result, deadline, refusal by source,
missed launches, client-send-to-first-row and client-send-to-terminal
p50/p95/p99, actual success QPS, physical scan bytes/second, CPU usage and
throttle fraction, memory peak/OOM, pool wait, and Oracle slot occupancy.
State the last useful concurrency before p95 rises sharply without material
QPS gain; show the raw curve, not just a winner. The fixed-rate 500/1,000/s
offers remain a 15-second-warmup/60-second-measurement comparison. Add a
1,500/s Q1 offer to substantiate the >1,000/s target, and attempt 5,000/s
only if the driver can offer it without missed launches.

For live rows, preflight the exact live query and verify that each held stream
occupies an Interactive slot for the whole 5-second interval; rejected or
already completed holders invalidate the row. Test both local and remote
Scribe placement with the same data and query. Separate client time, snapshot
time, Oracle admission wait, peer connection, stream-open, and first-batch
time. Run mixed read/write *simultaneously* on the one limited node, with
staging/publication active and enough local volume capacity to complete the
window. If WAL fills, query read-back fails, or writes miss their required
rate, mark the run failed and save the pod logs. Do not retry refusals in the
driver or turn post-window drain completions into in-window QPS.

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

### Scenario 2 — A busy Oracle queues valid work until the leader deadline

**Behavior.** Busy execution slots place authorized, executable queries in
the existing tenant-fair queue. The queue holds 1,000 waiters. A waiter can
remain more than 250 ms if its leader deadline allows; at that deadline it
receives `QueryTimeout` and leaves no retained state. Query 1,001 receives
the explicit queue-full overload; cancellation also frees a place. HTTP and
gRPC see the same rule without an earlier HTTP load-shed response.

**RED.** Add
`oracle::admission::tests::queued_queries_use_leader_deadline_and_one_thousand_places`
to the existing `vala-bifrost-redux` lib-test owner. The current 64-place
limit and 250 ms timer must fail its 1,000-place and >250 ms assertions.
Focused command:
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::admission::tests::queued_queries_use_leader_deadline_and_one_thousand_places)'`.
Add `capacity::saturated_query_waits_on_http_and_grpc` to the existing
`wyrd-testing` Oracle real-server journey target; occupy all executable
slots, submit one more valid query through each public transport, then free
capacity after 250 ms and assert valid terminals. Its no-slot-before-deadline
variant must time out; neither variant may return an HTTP edge 503 or a
capacity 429. Focused command:
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -E "test(=capacity::saturated_query_waits_on_http_and_grpc)" --run-ignored=all'`.

**GREEN.** Set the existing Oracle queue bound to 1,000, remove the separate
queue timer, and make full-queue overload distinct from deadline expiry.
Keep other routes' edge protection but let public query requests reach the
Oracle queue without earlier global HTTP shedding. Check the Rust, Python,
and TypeScript clients' synchronous query timeouts against the leader
deadline. Run a full-queue process-cluster case on 4 CPUs/8 GiB; the 1,000
waiters must remain below 7 GiB and cancel/release promptly.

**REFACTOR.** Delete `max_queue_wait_ms` and its tests/docs/config plumbing;
keep the one tenant-fair Oracle queue. Do not replace the 250 ms cap with a
different hidden timer or a second queue.

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

**Behavior.** Q0–Q5 exact results, fixture size, file geometry, offered rate,
held-live occupancy, cgroup limits, and simultaneous writes are checked
before a row can be valid. A wrong result or failed read-back fails the run.
The top-level driver reads as a short sequence of named cases and stages.

**RED.** Add
`load::capacity::run::tests::invalid_capacity_rows_never_count_as_success`
for a rejected live holder, changed file geometry, wrong aggregate, missed
offer, and failed read-back; each must invalidate its row. The CPU case is
covered by Scenario 2. Focused command:
`mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=load::capacity::run::tests::invalid_capacity_rows_never_count_as_success)'`.
Exercise a short process-cluster smoke through
`WYRD_BENCH_WARMUP_SECONDS=2 WYRD_BENCH_MEASURE_SECONDS=5 mise run bench:bifrost:query-capacity`;
prove it uses the public client and local Linux child.

**GREEN.** Reuse the existing harness, driver, recorder, and cgroup capture;
replace the rejected orchestration with the named workload table and visible
run order. Run the full concurrency sweep, four comparable fixed-rate rows,
valid local and remote live cases, and simultaneous read/write window.

**REFACTOR.** Delete old one-query-only assumptions and duplicate report/
fixture logic. Keep SQL, expected result, and target together so a reviewer
can inspect a case without following a generic workload graph.

### Scenario 6 — Meet and substantiate the performance targets

**Behavior.** The full fixture on the specified hardware meets AC-010 with
valid results and a bottleneck profile. The report identifies the first
measured limit for every miss and does not claim success from fast accepted
queries while most offers are refused.

**RED.** The current full report is red: Q1 cannot reach 500/s, held-live
rows are invalid, Q2–Q5 are absent, concurrent read/write is absent, and
read-back failed. Retain that baseline alongside the new run. A failure of
any required target remains red rather than being recast as “benchmark
implemented.”

**GREEN.** Run the complete opt-in benchmark on this machine, capture raw
evidence, profile any measured hot path, and make the minimum root-cause
change needed for every remaining required target. Rerun the affected case
and full suite after each change. Keep all correctness journeys green.

**REFACTOR.** Delete any temporary probes or optimization bypasses. Retain
only reusable production telemetry and the readable benchmark evidence.

## Acceptance Criteria

1. `planning_permits` and its immediate `try_acquire` 429 path are gone;
   waiting uses the leader deadline and PostgreSQL's existing bounded pool.
   Snapshot promotion, tenant protection, cancellation, and execution
   admission still pass their journeys.
2. The default Oracle queue holds 1,000 waiters across classes; its only
   waiting clock is the leader deadline. Saturation below 1,000 produces no
   early HTTP, gRPC, planning, or Oracle 429/503. Queue full is retryable
   overload, timeout is `QueryTimeout`, and both release resources. A full
   queue stays below 7 GiB on the specified node.
3. Remote Oracle transport reuses authenticated channels; changed peer
   identity does not reuse an old authority. Trace evidence assigns connect,
   open, and first-row latency correctly.
4. The benchmark uses the specified local child and public client; Q0–Q5,
   concurrency sweep, fixed-rate comparison, remote/local live, and concurrent
   writes have exact-result and run-validity proof. It saves raw samples,
   server logs, production metric snapshots, cgroup evidence, geometry,
   driver placement/lag, and a concise report.
5. Numeric REQ-008 targets pass on valid full runs, or this task remains
   incomplete with a named measured bottleneck. Never weaken the target,
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
- `architecture/bifrost-design.md` and performance docs record the removed
  snapshot limit, retained resource boundaries, and measured limits. Do not
  rewrite unrelated SDK contracts or the completed TASK-001 behavior.

## Verification and Evidence

Use TDD in the scenario order above. The commands above name planned tests
within existing package/module owners; verify their final selectors with
`mise exec -- cargo nextest list` before relying on a run. Postgres-backed
tests use the repository setup wrapper or their owning `mise` lane. Preserve the
existing `wyrd-testing` tests for the fixed-rate driver, exact fixture,
process-cluster limits, and prior run order, updating expectations only when
the behavior intentionally changes. Run `mise run fmt`, `mise run lints`,
`mise run verify:bifrost`, and `mise run gate` one at a time because this task
changes shared benchmark infrastructure and Oracle behavior. Run the short
smoke, then full opt-in benchmark on this Linux machine, one suite at a time;
attach reports and raw output without checking huge samples into Git.

The final evidence table has one row per acceptance criterion and workload:
command, commit, SQL/fixture checksum and geometry, cgroup proof, offered and
actual QPS, success/refusal/wrong-result counts, latency, scan rates, write
ACK/read-back, CPU/memory, validity, and PASS/FAIL. Include a separate phase
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

- [Approved revision 5](../spec.md): REQ-008–REQ-009, INV-001–INV-006,
  AC-009–AC-011.
- [Repository rules](../../../../AGENTS.md),
  [Bifrost architecture](../../../../architecture/bifrost-design.md),
  [testing map](../../../../TESTING.md).
- [Earlier benchmark scope and evidence](../review/task-001-codex-rereview-20260927/TASK-001-R2-bound-staged-cancellation-and-close-source-rules.md).
