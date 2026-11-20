# Benchmark experiments

Wyrd's benchmarks are opt-in experiments. Each one asks a single declared
question of a release `wyrd-server` on a quiet host, then keeps the answer as
an append-only record in [`history/`](#history). They are not part of any test
lane, `gate`, verify lane, or CI job.

```bash
mise run bench:smoke                 # every default plan at smoke scale
mise run bench:l1                    # Level 1, all three engines in order
mise run bench:l1:bifrost            # Level 1 Bifrost: writes-parquet, hot, parquet
mise run bench:l1:bifrost:writes     # one Bifrost experiment: writes-parquet
mise run bench:l1:bifrost:hot
mise run bench:l1:bifrost:parquet
mise run bench:l1:verification
mise run bench:l1:gateway
mise run bench:l2:bifrost            # Level 2, one engine
mise run bench:l2:verification
mise run bench:l2:gateway
mise run bench:l1:gateway -- --profile --steps chat.c64
mise run bench:l1:gateway -- --smoke # one plan at smoke scale
mise run bench:results               # every record as one HTML page
```

`--smoke` runs a normal command's whole workflow (fixture loading and
publication waits, every selected step and phase, measured stop, drain,
exact reconciliation, judgment, record, timing evidence and report) at
smoke-scale fixtures with one short warmup and one repetition per step. It
records `"mode": "smoke"` under `target/benchmarks/smoke/`, never in history, and a
smoke record is never a calibration wherever it is moved. A
smoke judges no criterion: every reading, errors and target misses included,
is recorded beside an inapplicable criterion, so a smoke is VALID or INVALID.
It is INVALID only when the measurement itself is broken (a missing reading
or counter, a mismatched population, zero CPU, a failed record); a busy
driver does not invalidate a smoke. A full command never runs a smoke first:
run `--smoke` on the same command, or `bench:smoke` for every default plan's
smoke. Every L1 experiment takes up to 5 minutes (Verification, Gateway,
Bifrost `hot`, `parquet` and `ui-sql`) except Bifrost `writes-parquet`,
which takes up to 8 (480 s), so `bench:l1:bifrost` takes up to 23 minutes
(1,380 s) and `bench:l1` up to 33 in all. An L2 engine takes 20 to 30 minutes.

Two extended Bifrost experiments run only when selected, never inside
`bench:l1:bifrost` or `bench:l1`, each up to 5 minutes with its own record:
`bench:l1:bifrost:ui:10x` (`ui-sql-10x`) runs `ui-sql` over ten times the
journey's requests, and `bench:l1:bifrost:ui:learned` (`ui-sql-learned`) runs
it after a published span sample taught the server a span Variant layout.
They are the volume and layout comparisons for `ui-sql`, not part of a
complete L1 suite.

## Prerequisites

The commands run on macOS arm64, Linux x86_64, and Linux arm64, with the same
command line and the same record schema on all three. Each command needs only
the repository toolchain and Docker for the repository's test Postgres.
Before the experiment clock starts, it builds the release server and the
`capacity` driver with frame pointers into `target/profiling`; nothing compiles
while measuring.

Each experiment starts its own plain-child server on the whole host, using a
`file://` store in a temporary directory and a fresh Postgres. `bench:l1` gives
each engine its own Postgres. No OS resource limits are applied: only Wyrd
configuration declares server limits.

## Measurement

- **One recorder.** A benchmark server installs `wyrd-benchmark` as its only
  metrics recorder before it registers any metric: request timings,
  counters, gauges and diagnostic histograms all land in the snapshots it
  serves at `/benchmark/snapshot` on the loopback metrics port, and no
  Prometheus recorder or exporter runs. A duration that has both a timing
  and a production histogram is recorded only as the timing. The recorder,
  every benchmark-only timing (such as an Agent check's
  `wyrd_verification_agent_phase_duration_seconds{phase}`), and the heap
  sampler exist only in a `wyrd-server` built with the `bench` feature, which
  `bench:build` uses; a shipped build keeps only operational metrics, installs
  Prometheus, and refuses `WYRD_METRICS_BENCHMARK_SNAPSHOT`. CPU and heap
  profiles and the OS's CPU and memory readings remain separate sources.
- **Latency is server-side.** Each judged request is timed inside the server
  from handler entry until its result is ready, its write is acknowledged, or
  its response is handed off, into bounded HDR histograms (3 significant
  digits) the benchmark server serves as cumulative snapshots on its
  loopback metrics port. A window is the bucket difference of two snapshots;
  its counts must match the work it sent. Each percentile is the
  nearest-rank bucket, and a limit passes or fails only when the whole
  bucket is on one side of it. L2 takes a snapshot at every phase boundary
  and keeps interleaved query classes apart by a closed benchmark case
  label; native writes cannot carry one, so L2 write latency includes the
  client-queue lanes' writes. Client round-trip times are recorded as
  diagnostics and never judged.
- **Overhead is per request.** It is the request's elapsed time minus the
  union of its own provider or Verifier waits, recorded into its own
  histogram per request.
- **Throughput is reported twice.** `(W/T)/P` is work per wall second per
  provisioned vCPU (`work/s/vCPU`, P = logical CPUs available to the server).
  `W/C` is work per server CPU-second used (`work/cpu-s`), and `C/W` is CPU
  per operation. The report's server CPU column reads `C s / C/T cores used /
  P vCPU`: cores used is measured, P is what the host offered. Only work
  completed before the window end counts as W, and later completions are
  reported apart. In L2, C is the server's total CPU, so every class's `W/C`
  also pays for the competing work. Each report row stands on its own:
  its rate cell carries every `(W/T)/P`, `W/C`, and `C/W` metric, and its
  latency cell the server and client p50/p95/p99.
- **Bytes have declared sources.** An OTLP write's headline is
  `input_mib_per_s_per_vcpu` and `input_mib_per_cpu_s`: the uncompressed
  OTLP protobuf the clients sent and the server acknowledged, the bytes
  ingest comparisons count. Rows per vCPU and per CPU-second follow. The
  uncompressed Arrow MiB the server stored (`stored_mib_*`) come last: they
  fall whenever the schema stores less per row, so they never measure ingest
  speed. Queries report the mean scanned
  rows and bytes of each query's own executed plan, from the same snapshots.
  Those are physical Parquet facts; a memtable batch is never one. `ui-sql`
  also reports `memtable_rows_per_query` and `memtable_bytes_per_query`: the
  captured memtable rows Scribe live reads examined after batch pruning and
  before any row filter, and the recorded decoded Arrow bytes of the columns
  they read, from the window's `bifrost_scribe_live_rows_examined_total` and
  `bifrost_scribe_live_bytes_examined_total` movement per answered query;
  missing, never zero, without the counter or an answered query. Each OTLP
  path's L1 `compression` is the input bytes of one settled sample per
  published Parquet byte of it, input over stored; its `storage_ratio`, which
  native also reports, is published Parquet bytes over retained Arrow bytes.
  Both exclude WAL, staging, and catalog bytes.
- The `timing` control records the method, precision, range, series bounds,
  the server's benchmark snapshot setting, and the host's measured timer
  resolution. Operator span export stays disabled.

Run on a quiet machine, with no other build, test, server, or benchmark
running. The run samples the whole host for 10 s before and after. If
processes outside the experiment use more than 10% of the logical CPUs, the
run is discarded.

## Levels

| Level | Shape | Windows | Budget |
|---|---|---|---|
| L1 | Isolated closed-loop work at 1 and 64 clients per operation | 5 s warmup, then 3 × 5 s per step; 3 × 10 s for a step whose 5 s window cannot meet its sample floor, declared in its `window_seconds` | 300 s per experiment; 480 s for Bifrost `writes-parquet` |
| L2 | Competing open-loop work: every repetition runs capacity → overload → recovery from equivalent state | 120 s warmup, then 3 × 360 s | 1,800 s, at least 1,200 s |

What each engine covers:

- **Bifrost.**
  - L1 is three experiments at 1 and 64 clients, each with its own record:
    - `writes-parquet`: 2M-row OTLP spans of about 340 B (`otlp-spans`)
      and about 1 KiB (`otlp-spans-1k`) of wire each, OTLP logs, and
      native writes under the production rotation and
      a 30 s retention, complete at their durable acknowledgement with no
      wait for publication. Each path after the first starts once every
      earlier row has published, outside every window, so no path's server
      CPU pays another path's backlog; native runs last because its backlog
      is the largest. Each window records the files and MiB it published
      and the memtable and staged backlog at its stop, and each OTLP
      window its measured `wire_bytes_per_row`; it is not a
      publication-latency or sustained-throughput measurement. Each write
      window also reports server phase p50/p99 (`phase_<phase>_*_ms`, with
      sample counts) from the `bifrost.scribe.write.*` timings, plus OTLP
      protobuf decode, which is labelled unavailable for native writes.
      Group stages are shared by every append of a group, so phases overlap
      and are never summed.
    - `hot`: the rows a query reads before they are published, in a
      memtable or a staged run. Nothing moves by itself (16,384 MB rotation
      sizes, 24 h retention): before any window the driver places the
      fixture through the `bench` build's platform-admin Scribe control
      (`POST /platform/benchmark/scribe/stage` and `.../publish`). It
      publishes 500k day-2 spans, which must teach the server a span Variant
      layout, then loads the first half of 10M events rows and 1M day-1
      spans and stages it, and loads the second half into memtables. Point
      lookups at 1/64 (p99 below 5 ms at 64 on the memtable half) and a small
      aggregate run against each half, and so do `parquet`'s shredded and
      long-tail span filters, bounded to each half's start times.
    - `parquet`: the same fixtures and a 200,000-row native and logs
      storage sample, settled once as Parquet; point lookups at 1/64, a
      small aggregate and a full scan at 1 (scans report p95), span
      `trace_id` Bloom lookups at 1/64, and shredded and long-tail
      attribute filters at 1. Each settled query must carry its own scan
      facts. After settlement, outside every window, it checks that span
      files shred exactly the keys over the 10% threshold and that a
      span-time window prunes, and reads each write path's storage ratio and each OTLP path's compression.
    - Scaling is reported for reads measured at both 1 and 64.
  - L1 sanity floors are the only absolute throughput limits: the native
    peak must reach at least 200k rows/s, and the OTLP peak at least 30 input
    MiB per server CPU-second.
  - L2 `competing` runs the 100M-row fixture with Forge compaction on:
    - open-loop ingest split 50% spans, 30% logs, 20% native;
    - mixed queries offered open loop at 12/s with at most 32 in flight
      (60% point, 25% dashboard, 10% table aggregate, 5% scan);
    - a full query queue, and three client-queue lanes.
  - L2 heavy scans must hold a server p95 below 2 s, with at least 200
    samples per repetition.
  - The L2 ingest rate is 70% of the mixed capacity `1/Σ(f_i/r_i)` times
    `--server-cores` (default 8). It is calibrated from the newest valid
    `writes-parquet` record with the same host, build, fixture, and
    limits.
    Without such a record, L2 is INVALID before any work.
- **Verification.** L1 `direct-verify` asks one question: how much time
  does the server add to a direct `verify`, apart from the Verifier's own
  work?
  - **The workload.** Every call verifies the canonical Verifier: one
    Assert check and two Agent checks. Each Agent check makes two model
    exchanges with a local provider and one `code.run`.
  - **The steps.** Each SDK runs three steps, nine in all:

    | Step | Clients | Load |
    |---|---|---|
    | `direct-verify-c1` | 1 | closed loop: call, wait for the result, call again |
    | `direct-verify-c64` | 64 | closed loop |
    | `direct-verify-r3000` | 64 | paced: 3,000 verifications/s in all |

    The Python and TypeScript steps carry the same names with `python-` or
    `typescript-` after `direct-verify-`. Rust also has diagnostic
    closed-loop steps `direct-verify-c4`, `-c16` and `-c32`, declared only
    when `--steps` names them, to read throughput and CPU per verification
    against `server_concurrency` as load rises.
  - **The headline number: overhead.** Overhead is the server's elapsed
    time for one execution, less the union of the Verifier's blocks (model
    calls, tools, sandbox). It is what Wyrd itself adds. Its p99 must be
    below 500 µs in `direct-verify-c64`. That is the only target.
  - **How to read a row.** Overhead varies with the load the server
    carries, and a closed loop fixes only the client count. Each step
    records `server_concurrency`, the average number of executions in
    progress (their summed time over the window). A client that spends
    longer in its own SDK between calls keeps fewer executions on the
    server, so compare closed-loop rows within one SDK only. To compare
    SDKs, use the `-r3000` rows: they offer the same arrival rate from
    every SDK. The report opens with a `By language` summary: one table
    per workload, with the three SDKs side by side and a note on whether
    that workload compares them.
  - **The SDK drivers.** Every SDK calls its public `observe().verify`,
    which runs the shared Rust `wyrd-client`.
    - Rust clients are async tasks inside the benchmark controller, which
      also hosts the local provider.
    - Python clients are threads in one child Python process
      (`sdks/wyrd-sdk-python/tests/benchmark_driver.py`).
    - TypeScript clients are Promise loops in one child Node process
      (`sdks/wyrd-sdk-ts/wyrd/tests/profile/verification-driver.mjs`).

    The controller drives a child process over stdin with one line per
    window (`{"run": {"clients", "seconds", "rate"}}`). The child times each
    call with its own clock and answers with every call. Each child starts
    and loads its applications before the first window.
  - **Paced steps.** In a paced step, client `i`'s `k`-th call waits for
    slot `i + k × clients`, due `slot / rate` seconds after the window
    opened. The window opens only once the driver's setup is done, and
    every slot due before it ends is sent, however late. Each call records
    how late it started, measured immediately before `verify`. A slot never
    sent is missed. A start-lateness p99 over 5 ms is late. Either makes the
    repetition INVALID, so a client that cannot keep up never passes as a
    lighter load. `arrivals_late` counts the calls that started more than
    5 ms late, and `arrivals_late_first_ms` and `arrivals_late_last_ms` give
    when the first and last of them were due. Late calls bunched at the
    start point at the window's setup; spread across it, at a recurring
    pause. `arrivals_wake_p99_us` is how late callers woke, before
    preparing their calls. The Python driver also reports its
    interpreter's garbage collections (`driver_gc_collections`,
    `driver_gc_max_ms`), and `arrivals_late_in_gc` counts the late calls
    whose wait overlapped one. Paced steps carry the correctness criteria, not the overhead
    target.
  - **Readings beside overhead.** None of these judge or invalidate.
    - `client_p*_ms`: end-to-end latency from just before the public
      `verify` until the decoded result returns. `client_ready_p*_ms` runs
      until every outcome, citation and data value is materialized, and
      `client_materialize_p*_ms` is the difference. Client latency has no
      target, but a missing or inverted sample makes the repetition
      INVALID.
    - `phase_<phase>_p99_ms` and `_max_ms`: where the overhead goes. The
      phases are `admit`, `load`, `prepare`, `engine` and `stage`.
      `outside` is the time in no phase, split into `outside_before` (before
      the first phase), `outside_between` (between phases) and
      `outside_after` (after the last phase).
    - `overhead_over_500us`, `_1ms`, `_5ms`, `_10ms`: how many executions
      exceeded each bound.
    - `diagnostic.server.{workspace,tool,sandbox,capacity,runnable,model}`:
      where the engine's time goes, as per-execution unions of workspace
      setup, tool calls, sandbox operations, waits for sandbox memory,
      waits for a runnable Python turn, and model calls. They are nested
      inside the engine and overlap across concurrent Agents, so never add
      or subtract them. The capacity and runnable waits sit inside the
      Verifier's blocks, so they are never counted as overhead.
    - `diagnostic.<python|typescript|driver>.*`: the client's own
      timings. These cover the shared client's `client.verify`,
      `client.http` and `client.http.attempt`, and the binding's
      `sdk.verify` (input conversion, shared core, output conversion, and
      for TypeScript the dispatch until the native future first runs).
      Python also splits that core as `sdk.verify.native`: `trace` (reading
      the caller's span), `run` (the shared client with the GIL released)
      and `attach` (waiting to hold the GIL again).
      They need the `bench` native modules: run `mise run
      bench:build:sdks`, then `py:setup` and `ts:build` again afterwards. A
      driver without them reports no window and leaves attribution
      unfinished.
    - `client_cpu_seconds` and `client_peak_memory_bytes` belong to the
      client process. For Rust, that process also holds the controller
      and the provider.
  - **Driver budget.** The driver may use at most 80% of its 8-core
    budget. For a Python or TypeScript step that budget covers the
    controller's CPU plus the child's, since both generate load.
  - L2 `competing` overlaps direct and background verification with their
    evidence writes. Submission, queue-wait and durable-ack means and the
    component breakdown come from the same benchmark histograms. Means
    retain bucket bounds rather than claiming exact precision.
- **Gateway.**
  - L1 `overhead` runs buffered, streamed, and 64 KiB capture calls at
    1 and 64 clients. Added overhead p99 must be below 500 µs at 64.
  - L2 `capacity` runs four contending tenants. Default p99 limits:
    completion ≤ 200 ms, first token ≤ 350 ms, full stream ≤ 1,100 ms.
    Override them with `--max-*-ms`. Inter-chunk lag is diagnostic only.

In every L2 run, three limits apply to each phase:

- backlog may grow by at most one second of offered work;
- drain may take at most 60 s;
- recovery must restore the SLOs within 30 s.

Declared admission refusals are allowed only during overload. A lost capture
always fails, and accounted shedding fails outside overload.

Flags:

| Flag | Applies to | Effect |
|---|---|---|
| `--profile` | every command | Adds optional samply CPU captures and per-step server heap profiles under `target/benchmarks/` (see [Profiling](#profiling)). |
| `--allow-dirty` | every command | Records a dirty source (see [Source](#source)). |
| `--steps a,b` | engine commands | Runs a declared subset; the record lists every omitted step and never claims full coverage. |
| `--pace <requests/s>` | `bench:l1:gateway` | Offers every window open loop at that rate, at most one open call per step client, with each missed or capped arrival counted as an error. It is a matched-load diagnostic: the record names the rate in its `workload` control, adds post-drain resident and governed memory by owner to each repetition's observations, and stays under `target/benchmarks/`. |

`bench:l1` accepts only `--profile`, `--allow-dirty`, and `--smoke`, and
passes them to every engine.

The Bifrost experiments' server inherits `WYRD_SCRIBE_SHARD_COUNT` from the
command's environment (default 1 shard). A set value is named in the record's
`limits` control, so L2 calibration reads only records run at the same shard
count. For example,
`WYRD_SCRIBE_SHARD_COUNT=4 mise run bench:l1:bifrost:writes -- --steps native-write-c64`.

## Verdicts

Benchmarks are measurements, not CI: a slower result is evidence, never a
failure. Every run ends with one verdict and exit code:

- **VALID (0):** the measurement is trustworthy and complete, and every
  correctness and integrity criterion holds in every repetition.
- **INVALID (2):** the evidence cannot answer the question, so the run does
  not count. Causes include:
  - a correctness or integrity criterion (errors, wrong answers, lost or
    duplicated rows, an undrained queue) failing in any repetition;
  - a busy host, or a driver above 80% of its 8-core budget;
  - an open-loop miss, or send lateness p99 above 5 ms;
  - too few samples: under 1,000 per repetition behind a p99, or 200 behind
    a p95;
  - a server timing observation that is missing, extra, duplicated, or
    malformed;
  - a missing or reset CPU reading;
  - an overrun budget, an interruption, or a failed workload;
  - a dirty source without `--allow-dirty`.

Nothing exits 1. A performance target (an SLO, a sanity floor, a backlog or
byte bound) judges the median of the three repetitions and is reported as
`met`, `missed`, or `unsettled` when a reading's precision cannot decide it;
a target never changes the verdict or the exit code. Strict targets (`<`)
are missed on equality. `bench:l1` and `bench:l1:bifrost` run every
experiment even after one is INVALID, keep each record, and exit 0 only when
every experiment exits 0, else 2.

## Records and reports

Each run appends one JSON record, schema version 3; a reader refuses any
other version. A clean, unprofiled full run of the whole plan, VALID or
INVALID, appends to history:

```text
benchmarks/history/<engine>/<experiment>/YYYYMMDDTHHMMSSZ-<sha9>.json
```

A `--steps`, `--profile`, `--pace` or dirty `--allow-dirty` run writes the same record under
`target/benchmarks/<engine>/<experiment>/` and a smoke under
`target/benchmarks/smoke/`; neither ever enters history.

The file is published atomically and never overwritten; a name collision gets
a `-<n>` suffix. A record holds:

- the command and non-secret configuration;
- the host, build, and source;
- the declared controls and criteria;
- every repetition's metrics, units, sample counts, server and driver CPU,
  and the server's opening and peak resident memory;
- every criterion's outcome, every reason, and the verdict.

Raw evidence stays under `target/benchmarks/<engine>/<experiment>/<UTC
start>/`: the server log sits beside any profiles. The report printed at the end is regenerated from the record alone, so it
can be reproduced after `target/` is gone:

```bash
target/profiling/release/capacity report benchmarks/history/bifrost/parquet/<record>.json
```

A step's detail ends with its `diagnostics`: readings stored in each
repetition as `diagnostic.<observation>.<stat>` (`count`, and `p50_us`,
`p95_us`, `p99_us`, `mean_us`), judged by no criterion. Each observation is
listed with its declared boundary and population, then one line per
repetition with its count, nearest-rank bucket bounds, and mean bounds, so a
slow repetition is never hidden behind a median. All are wall time, not CPU
time; observations overlap (Gateway `queue_delay` includes `phase.admit`, and
`server_elapsed` contains every request phase), so their values are never
added, and percentiles are never subtracted. An inapplicable observation,
such as Gateway `phase.relay` for a buffered call, says so; a missing
required one makes the repetition INVALID; a record that predates
diagnostics says they are unavailable. Gateway L1 records each phase of
`wyrd_gateway_phase_duration_seconds`, `queue_delay`, the answer handoff's
`server_elapsed`, `provider_excluded`, and `residual`, and the Scribe
`outbox_write.scribe` batch writes, each over its own window population.

After the diagnostics, `capacity report` lists each measured repetition's
server window: every timing series by boundary, outcome, case, and phase;
every scan series; and every counter that moved, gauge, and histogram with an
observation, read from the window's two snapshots the run keeps under
`timings/<step>/repetition-N.json` in its raw evidence directory. Counts,
nearest-rank bucket bounds, and mean bounds are of that window alone.
Oracle's `oracle_query_profile_*_total{event}` counters break a query's time
into planning and operator stages, such as `Oracle.physical_planning`,
`LiveScribeExec.fragment_open_time`, and `DataSourceExec.time_elapsed_opening`.

When reporting a run, paste this report, fresh or regenerated with
`capacity report <record>`, and append interpretation below it. Do not
replace it with hand-built tables or extract results from raw snapshots.

To compare two VALID runs of one experiment, such as a change against its
baseline:

```bash
target/profiling/release/capacity compare <before.json> <after.json>
```

Each step both runs measured gets one table, with a row for every metric
(diagnostics aside): `before` and `after` are each run's median with its
repetition range as `[min–max]`, `change` is the change of the medians, and
`outside spread` says `outside: higher` or `outside: lower` when the two
ranges do not overlap, adding `better` or `worse` for a metric that declares
a direction, and `no measured change` when they overlap. A histogram median
shows its bucket; its change compares the bucket upper bounds and it has no
spread verdict. Runs of different experiments, or an INVALID run, are
refused with exit 2.

## History

History is the `benchmark-records` branch, which shares no commits with the
code. A checkout that runs benchmarks mounts it as a worktree at
`benchmarks/history/`, which code branches ignore:

```bash
git worktree add benchmarks/history benchmark-records
```

Git checks a branch out in one worktree at a time; to use history from
another checkout, move it there with `git worktree move`. The recorder writes
history and `bench:results` and the Bifrost L2 calibration read it only
there; neither reads any other location.

To keep a run's record, commit it inside `benchmarks/history/` (on the
`benchmark-records` branch) and push that branch. Never commit records on a
code branch.

`mise run bench:results` writes every record in history into one self-contained
page, `target/benchmarks/results.html`, read from the records alone. For
each experiment it shows the question, the method (host, build, timing,
validity rules, workload), every step's results with each repetition, the
criteria outcomes and why the verdict is what it is. Its history charts any metric of
any step across runs in chronological order against its limit, and ticking
runs compares them side by side. A file that does not read as a current
record is shown as stored and marked.

## Source

A record names the measured commit. A dirty tracked tree refuses the run as
INVALID. `--allow-dirty` measures it anyway and records the SHA-256 of
`git diff HEAD`.

## Profiling

`--profile [cpu|heap|both]` selects the captures; bare `--profile` is
`both`. A CPU capture runs `samply record` against each server process with the same
invocation on every platform, and saves the captures under the run's
`target/benchmarks/` artifacts. A profile is diagnostic only. When samply is
missing or a capture fails, the record carries a note instead, and the
verdict is unchanged.

Run a profiled experiment with its normal task, narrowed to the steps in
question:

```bash
mise run bench:l1:gateway -- --profile cpu --steps buffered-64kib-capture-metadata.c64
```

The captures land in the run's `profiles/` directory,
`target/benchmarks/<engine>/<experiment>/<UTC start>/profiles/`, and the
printed report lists each one. Read a CPU capture with the harness:

```bash
target/profiling/release/capacity profile profiles/<step>/cpu-server-<pid>.json.gz              # every thread group
target/profiling/release/capacity profile profiles/<step>/cpu-server-<pid>.json.gz wyrd-scribe  # threads whose name contains it
```

For each thread group it prints the samples (about 1 ms of CPU each), then
the top functions by self and by inclusive samples. Thread names are cut to
15 characters (`wyrd-scribe-ing`, `tokio-rt-worker`), so filter by a prefix. `samply load` opens the
same capture as a flame graph.

A heap capture (`heap` or `both`) profiles the server. Once the fixtures are
loaded, the run asks the `bench` server build to start sampling its heap:
about one allocation per 512 KiB allocated, recorded with its call stack,
through the same mimalloc allocator the server always uses. After each
measured step, the run saves `profiles/heap-<role>-<step>.pb.gz` for the
server and the driver, a symbolized pprof
profile with Go's four heap sample types: `alloc_space` and `alloc_objects`
count everything the steps so far allocated, freed or not, and `inuse_space`
and `inuse_objects` what they still hold. Memory allocated before sampling
started is in neither.

Resident memory can grow far past `inuse_space`: memory a step allocates and
frees before it ends is gone from `inuse_space`, but the allocator may keep
its pages. `alloc_space` shows which code allocated it.

```bash
pprof -top profiles/heap-server-<step>.pb.gz                                      # what is held after <step>
pprof -top -base profiles/heap-server-<earlier>.pb.gz profiles/heap-server-<step>.pb.gz  # what <step> still holds beyond <earlier>
pprof -top -sample_index=alloc_space -base profiles/heap-server-<earlier>.pb.gz profiles/heap-server-<step>.pb.gz  # what <step> allocated
```

`pprof` is Google's profile viewer, installed locally (`go install github.com/google/pprof@latest`);
`-http=:8080` opens a flame graph. Each stack starts at the code that asked
for memory, with the allocator and the standard library's `Vec` and `Box`
plumbing trimmed. Inlined frames carry short names in the bench build's
line-table debug info; `-lines` shows their files and lines.

### Python and TypeScript clients

A profiled Verification L1 run starts its Python and TypeScript drivers
before the first step, so every step's captures include them under the
`python` and `typescript` roles. Build their diagnostic native modules first
with `mise run bench:build:sdks`: a `bench` Python extension and napi addon,
which record the shared client's timings and the binding's `sdk.verify`
boundary and sample their own native allocations. They are never published;
run `mise run py:setup` and `mise run ts:build` again afterwards.

CPU: the same samply capture as the server's
(`profiles/<step>/cpu-<language>-<pid>.json.gz`). A CPU-profiled driver starts
with its runtime's perf map, Python's `-X perf` and Node's
`--perf-basic-prof`, from which samply names Python and JavaScript frames
beside the native module's. Node then also writes a V8 log, kept as
`v8-typescript.log` beside the server log, never in the package directory.
A frame shows CPU only; waits come from the recorded phases.

Heap, saved after each step as `profiles/heap-<language>-<step>.*`:

| File | Python | TypeScript |
|---|---|---|
| Managed | `.tracemalloc`: every managed allocation still held, traced since the driver started | `.heapprofile`: V8's sampling heap profile since the driver started, collected objects included, so allocation volume; `.heapsnapshot`: what is still live |
| Native | `.pb.gz`: the extension's sampled allocations, allocated and still live | `.pb.gz`: the addon's sampled allocations, allocated and still live |

A driver whose native module is not a `bench` build saves no native file;
the record notes it and marks the profile incomplete. `tracemalloc` records
what the managed heap still holds and its peak, not the volume a window
allocated and freed.

```bash
python -c 'import tracemalloc; [print(s) for s in tracemalloc.Snapshot.load("profiles/heap-python-<step>.tracemalloc").statistics("lineno")[:20]]'
pprof -top -sample_index=alloc_space profiles/heap-python-<step>.pb.gz
```

Open a `.heapprofile` or `.heapsnapshot` in Chrome DevTools (Memory panel).

Every repetition also records `opening_memory_bytes`, the server's resident
memory as its window opened, beside `peak_memory_bytes`. Peak minus opening
is the memory that window added, in profiled and unprofiled runs alike.

Server CPU and memory are the server process's own. Its Python worker
children are recorded apart, in every verification step, as
`worker_cpu_seconds` and `worker_peak_memory_bytes`. Worker CPU is the growth,
over the measured window, of the CPU of the children the server has reaped
(the `bench` snapshot's `wyrd_process_reaped_children_cpu_seconds`, from
`getrusage(RUSAGE_CHILDREN)`) plus the growth of its live children's CPU. A
worker that starts or exits inside the window therefore counts only its CPU
inside it. Worker memory is the sampled peak of the live children's summed
resident memory; new children are found every 100 ms. Either reading is
missing, never zero, when a counter was unavailable or no worker was
sampled. Records written before these readings existed keep server CPU as
the parent's alone. `worker_spawns`, `worker_resets` and `worker_reaps` count the
workers started, reset for reuse and reaped inside the window, from the
server's `wyrd_verification_python_worker_events_total` at the window's open
and stop scrapes.
