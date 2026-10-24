# Benchmark experiments

Wyrd's benchmarks are opt-in experiments. Each one asks a single declared
question of a release `wyrd-server` on a quiet host, then keeps the answer as
an append-only record in this directory. They are not part of any test lane,
`gate`, verify lane, or CI job.

```bash
mise run bench:l1                    # Level 1, all three engines in order
mise run bench:l1:bifrost            # Level 1, one engine
mise run bench:l1:verification
mise run bench:l1:gateway
mise run bench:l2:bifrost            # Level 2, one engine
mise run bench:l2:verification
mise run bench:l2:gateway
mise run bench:l1:gateway -- --profile --steps chat.c64
```

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
  its response is handed off. The server exports one `wyrd.timing` span per
  request with exact integer nanoseconds, and the driver collects them over
  OTLP. Percentiles are nearest-rank over those raw samples; there are no
  histograms or buckets. Client round-trip times are recorded as diagnostics
  and never judged.
- **Overhead is per request.** It is the request's elapsed time minus the
  union of its own provider or Verifier waits, from the same span.
- **Throughput is reported twice.** `(W/T)/P` is work per wall second per
  provisioned vCPU (`work/s/vCPU`, P = logical CPUs available to the server).
  `W/C` is work per server CPU-second used (`work/cpu-s`). Writes report rows
  and uncompressed MiB; queries report queries plus scanned rows and bytes per
  query. Baselines compare `W/C`.
- The `timing` control records the method, the server's span exporter
  settings, and the host's measured timer resolution.

Run on a quiet machine, with no other build, test, server, or benchmark
running. The run samples the whole host for 10 s before and after. If
processes outside the experiment use more than 10% of the logical CPUs, the
run is discarded.

## Levels

| Level | Shape | Windows | Budget |
|---|---|---|---|
| L1 | Isolated closed-loop work: a client sweep (1/4/16/64) per operation | 5 s warmup, then 3 × 10 s per step | 900 s |
| L2 | Competing open-loop work: every repetition runs capacity → overload → recovery from equivalent state | 120 s warmup, then 3 × 360 s | 1,800 s, at least 1,200 s |

What each engine covers:

- **Bifrost.**
  - L1 `isolated` uses the 10M-row events fixture published as Parquet:
    - point lookups at 1/4/16/64 clients, with p99 below 5 ms at 64;
    - small aggregates and full scans at 1/64; scans report p95;
    - 2M-row write sweeps for native writes, OTLP spans, and OTLP logs.
  - L1 sanity floors are the only absolute throughput limits: the native
    peak must reach at least 200k rows/s, and the OTLP peak at least 30 wire
    MiB per server CPU-second.
  - L2 `competing` runs the 100M-row fixture with Forge compaction on:
    - open-loop ingest split 50% spans, 30% logs, 20% native;
    - 32 query clients (60% point, 25% dashboard, 10% table aggregate, 5%
      scan);
    - a full query queue, and three client-queue lanes.
  - L2 heavy scans must hold a server p95 below 2 s, with at least 200
    samples per repetition.
  - The L2 ingest rate is 70% of the mixed capacity `1/Σ(f_i/r_i)` times
    `--server-cores` (default 8). It is calibrated from the newest valid
    `bench:l1:bifrost` record with the same host, build, fixture, and limits.
    Without such a record, L2 is INVALID before any work.
- **Verification.**
  - L1 `direct-verify` runs direct verify at 1/4/16/64 clients. Server
    overhead p99 must be below 500 µs at 64.
  - L2 `competing` overlaps direct and background verification with their
    evidence writes.
- **Gateway.**
  - L1 `overhead` runs buffered, streamed, and 64 KiB capture calls at
    1/4/16/64 clients. Added overhead p99 must be below 500 µs at 64.
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

`bench:l1` accepts only `--profile` and `--allow-dirty`, and passes them to
every engine.

## Verdicts

Every run ends with one verdict and exit code:

- **PASS (0):** every applicable criterion holds.
- **FAIL (1):** valid evidence missed a declared criterion: an SLO, a sanity
  floor, a correctness or integrity check, or a baseline regression.
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
  - a dirty source without `--allow-dirty`.

INVALID takes precedence over FAIL, and FAIL over PASS. A FAIL found in an
otherwise invalid run still appears among the reasons. `bench:l1` runs every
engine even after one fails, and exits with the worst code.

Strict limits (`<`) fail on equality. Numeric criteria judge the median of the
three repetitions. Correctness and integrity criteria judge every repetition.

## Records and reports

Each run appends one JSON record, schema version 1:

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
- every reason, and the verdict.

The report printed at the end is regenerated from the record alone, so it can
be reproduced after `target/` is gone:

```bash
target/profiling/release/capacity report benchmarks/bifrost/isolated/<record>.json
```

Commit records worth keeping. The history in this directory is how results
are compared over time.

## Source

A record names the measured commit. A dirty tracked tree refuses the run as
INVALID. `--allow-dirty` measures it anyway and records the SHA-256 of
`git diff HEAD`. Such a record is never eligible as a baseline.

## Baselines

`benchmarks/<engine>/baselines.toml` holds the historical references that
criteria name by `id`. Each entry carries:

- its value and unit, and the original operands;
- its fixture, source report, and source commit;
- its host fingerprint, and its limitations.

A criterion's baseline is fixed when the run is declared:

- An eligible entry from the same host fingerprint and fixture judges with
  the noise-aware regression rule. Higher-is-better fails when the upper
  repetition is below 90% of the baseline and the drop exceeds the spread.
  Lower-is-better fails when the lower repetition is above 115% and the rise
  exceeds the spread.
- An eligible entry from another host, OS, or fixture is advisory. The
  record keeps both fingerprints, and the entry never judges.
- A missing, excluded, or unit-incompatible entry makes the run INVALID.

Today's eligible entries are all Bifrost. They come from the clean pre-merge
`cfe21dc90` runs, recorded on an AMD Ryzen 9 9950X Linux host. The 2026-10-21
candidates stay excluded until their clean source is established. The L1
native-write fixture (2M rows) differs from its 10M-row baseline, so native
comparisons are advisory. Verification and Gateway have no eligible baseline,
so their experiments judge only their declared limits. Their files list the
excluded sources and the reasons for excluding them.

A baseline changes only by editing `baselines.toml` in review. No run promotes
itself.

## Profiling

`--profile` runs `samply record` against each server process with the same
invocation on every platform, and saves the captures under the run's
`target/benchmarks/` artifacts. A profile is diagnostic only. When samply is
missing or a capture fails, the record carries a note instead, and the
verdict is unchanged.
