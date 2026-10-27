# Benchmark experiments

Wyrd's benchmarks are opt-in experiments. Each one asks a single declared
question of a release `wyrd-server` on a quiet host, then keeps the answer as
an append-only record in this directory. They are not part of any test lane,
`gate`, verify lane, or CI job.

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
mise run bench:l1:gateway -- --qualify-baseline   # a reference candidate
target/profiling/release/capacity reference <a.json> <b.json>  # reviewed references
mise run bench:results               # every record as one HTML page
```

`--smoke` runs a normal command's whole workflow (fixture loading and
publication waits, every selected step and phase, measured stop, drain,
exact reconciliation, judgment, record, timing evidence and report) at
smoke-scale fixtures with one short warmup and one repetition per step. It
records `"mode": "smoke"` under `target/benchmarks/smoke/`, never here, and a
smoke record is never a calibration or baseline wherever it is moved. A
smoke judges no criterion: every reading, errors and target misses included,
is recorded beside an inapplicable criterion, so a smoke is PASS or INVALID.
It is INVALID only when the measurement itself is broken (a missing reading
or counter, a mismatched population, zero CPU, a failed record); a busy
driver does not invalidate a smoke. A full command never runs a smoke first:
run `--smoke` on the same command, or `bench:smoke` for every default plan's
smoke. Every L1 experiment takes up to 5 minutes: Verification, Gateway,
and each of the three Bifrost experiments, so `bench:l1:bifrost` takes up to
15 and `bench:l1` up to 25 in all. An L2 engine takes 20 to 30 minutes.

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
  also pays for the competing work. References compare `W/C`,
  `C/W`, and `(W/T)/P`.
- **Bytes have declared sources.** Writes report rows, retained Arrow MiB
  (uncompressed), and encoded OTLP wire MiB. Queries report the mean scanned
  rows and bytes of each query's own executed plan, from the same snapshots. The L1
  storage ratio is published Parquet bytes over retained Arrow bytes of one
  settled sample per path, excluding WAL, staging, and catalog bytes.
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
| L1 | Isolated closed-loop work at 1 and 64 clients per operation | 5 s warmup, then 3 × 5 s per step; 3 × 10 s for a step whose 5 s window cannot meet its sample floor, declared in its `window_seconds` | 300 s per experiment |
| L2 | Competing open-loop work: every repetition runs capacity → overload → recovery from equivalent state | 120 s warmup, then 3 × 360 s | 1,800 s, at least 1,200 s |

What each engine covers:

- **Bifrost.**
  - L1 is three experiments at 1 and 64 clients, each with its own record:
    - `writes-parquet`: 2M-row OTLP spans, OTLP logs, and native writes
      under the production rotation and
      a 30 s retention, complete at their durable acknowledgement with no
      wait for publication. Each path after the first starts once every
      earlier row has published, outside every window, so no path's server
      CPU pays another path's backlog; native runs last because its backlog
      is the largest. Each window records the files and MiB it published
      and the memtable and staged backlog at its stop; it is not a
      publication-latency or sustained-throughput measurement.
    - `hot`: 10M events rows and 1M spans held in memtables (a staged or
      published row is INVALID); point lookups at 1/64, with p99 below 5 ms
      at 64, a small aggregate and a shredded-attribute span filter at 1.
    - `parquet`: the same fixtures and a 200,000-row native and logs
      storage sample, settled once as Parquet; point lookups at 1/64, a
      small aggregate and a full scan at 1 (scans report p95), span
      `trace_id` Bloom lookups at 1/64, and shredded and long-tail
      attribute filters at 1. Each settled query must carry its own scan
      facts. After settlement, outside every window, it checks that span
      files shred exactly the keys over the 10% threshold and that a
      span-time window prunes, and reads each write path's storage ratio.
    - Scaling is reported for reads measured at both 1 and 64.
  - L1 sanity floors are the only absolute throughput limits: the native
    peak must reach at least 200k rows/s, and the OTLP peak at least 30 wire
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
- **Verification.**
  - L1 `direct-verify` runs direct verify at 1 and 64 clients. Server
    overhead p99 must be below 500 µs at 64. Each repetition also records
    how many executions exceeded 0.5, 1, 5, and 10 ms of overhead, and the
    p99 and maximum of each execution phase (`admit`, `load`, `prepare`,
    `engine`, `stage`, and `outside`, the time outside every phase) from
    the same server timing histograms, to locate a slow tail. Phase
    readings are diagnostic: they never judge and never invalidate.
  - L2 `competing` overlaps direct and background verification with their
    evidence writes.
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
| `--profile` | every command | Adds optional samply captures under `target/benchmarks/` (see [Profiling](#profiling)). |
| `--allow-dirty` | every command | Records a dirty source (see [Source](#source)). |
| `--steps a,b` | engine commands | Runs a declared subset; the record lists every omitted step and never claims full coverage. |
| `--qualify-baseline` | L1 commands | Records a reference candidate judged only by its limits (see [References](#references)). |

`bench:l1` accepts only `--profile`, `--allow-dirty`, `--smoke`, and
`--qualify-baseline`, and passes them to every engine.

## Verdicts

Every run ends with one verdict and exit code:

- **PASS (0):** every applicable criterion holds.
- **FAIL (1):** valid evidence missed a declared criterion: an SLO, a sanity
  floor, a correctness or integrity check, or a reference regression.
- **INVALID (2):** the evidence cannot answer the question, so the run does
  not count. Causes include:
  - a busy host, or a driver above 80% of its 8-core budget;
  - an open-loop miss, or send lateness p99 above 5 ms;
  - too few samples: under 1,000 per repetition behind a p99, or 200 behind
    a p95;
  - a server timing observation that is missing, extra, duplicated, or
    malformed;
  - a missing or reset CPU reading;
  - an overrun budget, an interruption, or a failed workload;
  - a declared baseline that is missing or excluded;
  - a full L1 case with no reviewed reference of the same method (see
    [References](#references));
  - a dirty source without `--allow-dirty`.

INVALID takes precedence over FAIL, and FAIL over PASS. A FAIL found in an
otherwise invalid run still appears among the reasons. `bench:l1` runs every
engine even after one fails, and exits with the worst code.

Strict limits (`<`) fail on equality. Numeric criteria judge the median of the
three repetitions. Correctness and integrity criteria judge every repetition.

## Records and reports

Each run appends one JSON record, schema version 2 (version 1 records still
read back):

```text
benchmarks/<engine>/<experiment>/YYYYMMDDTHHMMSSZ-<sha9>.json
```

The file is published atomically and never overwritten; a name collision gets
a `-<n>` suffix. A record holds:

- the command and non-secret configuration;
- the host, build, and source;
- the declared controls and criteria, with baseline snapshots;
- every repetition's metrics, units, sample counts, and server and driver
  CPU;
- every reason, and the verdict;
- for a full L1 run, its regression coverage: `covered` when every case was
  judged against a same-method reference, `uncovered` otherwise, and
  `baseline-unavailable` for a qualification run.

Raw evidence stays under `target/benchmarks/<engine>/<experiment>/`:
the server log sits beside any profiles. The next run of that experiment replaces
it. The report printed at the end is regenerated from the record alone, so it
can be reproduced after `target/` is gone:

```bash
target/profiling/release/capacity report benchmarks/bifrost/parquet/<record>.json
```

Commit records worth keeping. The history in this directory is how results
are compared over time.

`mise run bench:results` writes every record here into one self-contained
page, `target/benchmarks/results.html`, read from the records alone. For
each experiment it shows the question, the method (host, build, timing,
validity rules, workload), every step's results with each repetition, the
criteria and why the verdict is what it is. Its history charts any metric of
any step across runs in chronological order against its limit, and ticking
runs compares them side by side. A file that does not read as a current
record is shown as stored and marked.

## Source

A record names the measured commit. A dirty tracked tree refuses the run as
INVALID. `--allow-dirty` measures it anyway and records the SHA-256 of
`git diff HEAD`. Such a record is never eligible as a baseline.

## Baselines

`benchmarks/<engine>/baselines.toml` holds two kinds of entry that criteria
name by `id`.

Historical entries come from earlier reports. Each carries its value and
unit, the original operands, its fixture, source report and commit, its host
fingerprint, and its limitations. They predate method identity, so they
never judge: an eligible one is advisory, and the record keeps both
fingerprints. A missing, excluded, or unit-incompatible one makes the run
INVALID. The Bifrost entries come from the clean pre-merge `cfe21dc90` runs
on an AMD Ryzen 9 9950X Linux host; Verification and Gateway have none.

## References

Every full L1 case is judged against a reviewed reference
`reference.<experiment>.<step>.<metric>` for its latency (the declared
percentile; a full scan has none), CPU per unit of work, work per
server CPU-second, and work per second per vCPU. A reference carries the
median of six repetitions, the low and high bounds of a latency, and its
method: a digest of the engine, experiment, level, mode, timing, validity,
variables, controls, step, build, host, the `WYRD_*` configuration, and
`--profile`. The commit and binaries are not part of it.

- A reference of the same method judges with the noise-aware rule.
  Higher-is-better fails when the upper repetition is below 90% of the
  reference and the drop exceeds the spread; lower-is-better fails when the
  lower repetition is above 115% and the rise exceeds the spread. A latency
  compares bounds: it fails only when even its best bound fails against the
  reference's worst, and an overlap that decides neither is INVALID.
- A missing reference, or one of another method, makes the case INVALID.
- A diagnostic run (`--profile`, an OTLP trace endpoint, sample ratio, or
  export timeout, a trace filter, or `WYRD_LOG` other than `info`) only advises against a
  reference of another method.

To establish references for a host:

1. From clean committed source, run the full suite twice with
   `--qualify-baseline`. Such a run judges only its declared limits, records
   `baseline-unavailable`, and is refused (INVALID) for L2, a smoke, `--steps`,
   a dirty source, or a diagnostic configuration.
2. Run `capacity reference` over both records of each experiment. It checks
   that every record qualified, that they share engine, experiment, and
   method, and that each metric has at least six bounded repetitions, then
   prints the `[[entries]]` TOML. It writes nothing.
3. Review the TOML and commit it to `baselines.toml`.

A reference changes only by editing `baselines.toml` in review. No run
promotes itself.

## Profiling

`--profile` runs `samply record` against each server process with the same
invocation on every platform, and saves the captures under the run's
`target/benchmarks/` artifacts. A profile is diagnostic only. When samply is
missing or a capture fails, the record carries a note instead, and the
verdict is unchanged. A profiled run is a diagnostic configuration: it
never qualifies as a reference, and it only advises against a reference of
another method.
