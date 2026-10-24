---
id: SPEC-benchmark-experiments
revision: 3
status: approved
---

# Benchmarks as experiments

Approved by the maintainer at revision 3. This revision fixes the behavioral
and persisted contracts that implementation tasks follow.

## Problem, proposed change, and value

Wyrd's Bifrost, Verification, and Gateway benchmarks ask related capacity and
latency questions through different measurement and report implementations.
Some steps only report numbers; some thresholds exist only as invocation flags;
results mostly disappear into `target/`. An engineer or agent cannot reliably
answer whether a result is a regression, noise, or an invalid measurement.

Treat every benchmark as an experiment: declare the question, sweep, controls,
metrics, and criteria before measurement, then produce one attributable record
and one report derived from it. One shared `experiment` module in `wyrd-testing`
owns instrumentation, measurement, judgment, and recording for all three
engines. Engine workloads retain their production workflows and fixtures.

For example, today a client-queue stress step can print a problem without
failing the command. After this change every step has declared criteria and a
verdict. A valid criterion miss exits 1; a saturated load driver discards the
measurement as INVALID and exits 2. Both leave a compact historical record.

Level 1 gives opt-in feedback within 15 minutes per engine. Level 2 measures
mixed traffic and background work together over 20–30 minutes per engine.
Relative efficiency makes hardware differences visible without dividing by
unused host cores. Absolute latency objectives still test a useful experience.

The costs are three measured repetitions per step, explicit validity checking,
portable resource accounting, and growing committed summary history. Raw
samples alone can be about 9 MB per run, so they remain outside Git in
`target/`. Normalized metrics do not remove differences in CPU architecture,
storage, caches, or fixture shape. This is performance evidence, not a promise
of equal capacity on every machine.

The approved maintainer portability decision applies to every experiment at
both levels: run unchanged on macOS arm64, Linux x86_64, and Linux arm64.
Servers run as plain child processes without OS resource limits; only Wyrd
configuration supplies server-side limits. Optional profiling uses samply
through the same `--profile` flag on every platform.

Keeping separate harnesses would preserve inconsistent judgments. Importing old
reports as new records would invent missing declarations and repetitions.
Using host CPU count as the denominator would measure spare capacity rather
than server work. These alternatives are rejected.

## Scope and ownership

This packet owns benchmark structure, recording, measurement definitions,
criteria, and command levels across `bifrost`, `verification`, and `gateway`.
Capacity coverage is kept and rebuilt, not deleted. Engine optimization and
ClickBench reproduction are not part of this change. Repository architecture
remains authoritative.

“Experiment” here means a repository performance experiment, not a new Card
kind or a requirement to register an Experiment Card. There is no new server,
SDK, HTTP, MCP, or UI product workflow. The public contributor surfaces are
`mise` commands, documentation, JSON records, and baseline configuration.

Non-goals: optimize an engine; change durability, authorization, or admission;
add benchmark CI wiring or any gate dependency; run benchmarks during drafting;
reproduce ClickBench; publish marketing rankings; add platform-specific resource
isolation; import historical results as records; commit samples or profiles;
automatically stage or commit benchmark output. L1 and L2 remain opt-in and
part of no gate or verification lane.

## Required behavior

### Declare, measure, and judge

- **REQ-001 — Declaration.** Before any measured step, the runner fixes a
  one-sentence question, engine, experiment identifier, level, variable sweep,
  controls, metrics with units, and criteria with comparison operators and
  applicability. Controls include source commit and dirty flag, profile and
  build flags, host fingerprint, fixture identity/size/seed, server limits,
  workload mix, warmup/repetition durations, validity thresholds, and budget.
  Missing declarations refuse measurement as INVALID; observed performance
  cannot choose or weaken the criteria afterward.
- **REQ-002 — Validity.** The shared runner checks quiet-host eligibility before
  workload execution, generalizing Forge's free-CPU discard to all engines.
  It also records driver CPU/scheduling health, scheduled/sent/completed/missed
  arrivals, sample sufficiency, source cleanliness, measurement availability,
  and elapsed budget. Busy host, CPU-bound driver, missed open-loop arrivals,
  too few samples, dirty tree without explicit override, missing required
  resource readings, or over-budget execution is INVALID. A dirty override is
  declared before execution and stored; no other validity check is bypassed.
  Sample host CPU for 10 seconds before the run and again after it. Either
  sample showing non-benchmark processes using more than 10% of logical CPUs
  invalidates the run. Driver use above 80% of its declared CPU budget is
  INVALID. Open-loop traffic is INVALID on any missed arrival or send-lateness
  p99 above 5 ms (Gateway's existing `LATENESS_LIMIT_US`). Each repetition needs
  ≥1,000 samples for each p99 criterion and ≥200 for each p95 criterion;
  fewer is INVALID. Slow steps, including ordinary scans, use p95. The explicit
  L2 heavy-scan p99 criterion below is an exception requiring ≥1,000 samples.
- **REQ-003 — Repetitions.** Every step has a discarded warmup followed by three
  measured repetitions with the same controls. L1 discards 5 seconds of warmup
  then measures 3 × 10 seconds; L2 discards 2 minutes of warmup then measures
  3 × 6 minutes. Preserve each repetition's
  metrics, median, minimum, maximum, and sample counts; never merge the samples
  into a percentile and call it the median repetition. Warmup contributes no
  measured work, CPU, latency, or sample count. A missing/invalid repetition
  invalidates the step; do not silently select the fastest repetition.
- **REQ-004 — Judgment.** Every measured step and every applicable criterion
  yields PASS, FAIL, or INVALID. PASS means valid evidence meets its criteria;
  FAIL means valid evidence misses at least one criterion; INVALID means the
  experiment did not measure what it claims. An invalid step invalidates the
  run, even if another valid step fails; otherwise any failed step fails the
  run. Exit codes are PASS=0, FAIL=1, INVALID/discarded=2. An aggregate L1
  invocation uses the same precedence across engines and preserves their
  individual records. Invalidity reasons and any valid failures remain visible.
- **REQ-005 — Complete steps.** No sustained-only gate or report-only stress
  step remains. Overload steps declare expected rejections, queue/capture loss
  accounting, recovery, and bounds in advance; a successfully measured overload
  is not inherently INVALID or FAIL. Accidental request errors, wrong results,
  or undeclared loss in a valid run fail correctness criteria. Expected server
  admission rejection differs from a driver that failed to issue arrivals.

A step's numeric criteria use the median repetition metric unless explicitly
declared otherwise; correctness and integrity criteria must hold in all three
repetitions. Repetition spread never excuses an absolute SLO miss. Conditions
such as capture saturation are recorded observations, not additional verdicts.
A runner failure after declaration retains partial evidence as INVALID and
marks unmeasured declared steps explicitly; it never fabricates zero metrics.

### Metrics and their meanings

**REQ-006 — Portable accounting.** Measure server user plus system CPU-seconds
actually consumed during each repetition. Sum the declared server processes,
including server workers/replicas, over the same window. Do not include the
driver, upstream simulator, Postgres, or object-store service; report those
separately when relevant. In L2, server background work is included rather than
subtracted or attributed arbitrarily to an individual traffic class. Store
which processes are included and the window boundaries. Counter reset, process
loss, unavailable accounting, or a zero denominator makes derived efficiency
unavailable and the required measurement INVALID.

Use `sysinfo`, already a workspace dependency, for process CPU time and sampled
peak resident memory. Quiet-host and driver-saturation checks use the same
portable readings. Reuse the process readings in `release_server/os.rs` and
host sampling in Forge's `host.rs`; missing readings must not count as zero use.
Use one portable implementation on macOS arm64 and Linux x86_64/arm64. No
cgroups, namespaces, `/proc`, `cfg(target_os)` gating, credential switching,
`prlimit`, seccomp, Landlock, or sandbox-exec. No systemd resource scopes. Server
limits come only from Wyrd configuration, not OS isolation. Every server,
including Forge leaders and workers, runs as a plain child without OS CPU or
memory limits. Forge metrics use summed server CPU-seconds actually consumed,
never the old leader/worker quotas. Scoped resource paths are not authority.

Let `T` be measured wall-seconds, `C` server CPU-seconds, `W` successful work,
`B` measured bytes, and `Q_N` completed queries/second at N clients:

| Metric kind | Definition and units |
|---|---|
| Efficiency, “per core” | Mean used cores = `C/T`; work/s per core = `(W/T)/(C/T) = W/C`. This is work per server CPU-second, never work divided by host core count. |
| CPU per operation | `C/W`, converted to microseconds or milliseconds as named. |
| Byte efficiency | `B/C`, with MB=10^6 bytes, MiB=2^20 bytes, GB=10^9 bytes. |
| SLO | Absolute percentile for the declared timing boundary and client count; the fixed L1 objectives use p99 at 64 clients. No CPU normalization. |
| Scaling | `Q_N / (N × Q_1)` for the same fixture, controls, and query class; report N=64 and preserve both inputs. |
| Storage ratio | Published stored bytes / ingested uncompressed bytes for the same data after declared publication/compaction settlement; lower is better. State excluded WAL/staging/catalog bytes separately. |
| Memory intensity | Peak server resident bytes / achieved uncompressed MB/s. Store sampling interval and scope; a sampled peak is not an exact OS high-water mark. |

**REQ-007 — Engine metrics.** Each engine records the following applicable
metrics; an inapplicable metric carries an explicit reason rather than zero.
Latency records identify client or server boundary and histogram resolution.
Histogram bounds must resolve the strict SLO boundary; an unresolved tail is
not evidence of meeting it.

| Engine / operation | Required metrics and fixed SLO |
|---|---|
| Bifrost write | rows/s per core; uncompressed MB/s per core; OTLP wire MiB/s per core for OTLP; CPU µs/row; stored/ingested bytes; peak memory per ingested MB/s; write-to-query visibility p99. |
| Bifrost query | Point-lookup p99 **<5 ms at 64 clients**; qps/core; CPU ms/query; scanned GB/s/core; 1→64 scaling by query class. |
| Verification | Server overhead p99 **<500 µs at 64 clients**; verifications/s/core; CPU µs/verification. |
| Gateway | Added overhead p99 **<500 µs at 64 clients**; requests/s/core; CPU µs/request; capture bytes/s/core. |

Uncompressed Bifrost ingest counts retained Arrow bytes inserted, consistent
with the Scribe telemetry authority; successful work excludes retry duplicates.
OTLP wire counts encoded protobuf payload bytes sent, before transport framing,
with compression settings declared. Scanned bytes are executed-plan scan bytes,
not logical dataset size. Stored bytes count published table data, not input
wire bytes; do not label its inverse as the same ratio.

Point-lookup latency covers the public client's request through validated
complete result. Visibility covers acknowledged write to first correct query
result and declares probe cadence. Verification overhead covers server arrival
to result staging less Verifier-block time; preserve both component timings.
Gateway added overhead excludes provider exchange from server request handling
through each applicable handoff; preserve upstream and client timings separately.
Do not subtract independently aggregated p99s. Mixed-workload efficiency reports
its total CPU denominator and work by class; it is not isolated-operation CPU
attribution. Additional absolute criteria are declared below.

### Levels and commands

**REQ-008 — Level 1, fast.** Each engine completes its full default suite in
at most 15 minutes. Build binaries and prepare external prerequisites before
starting the experiment clock; the clock includes quiet-host checking, server
startup, fixture loading, warmups, repetitions, and drain. An overrun is stopped
and recorded INVALID. Artifact finalization after stopping must still retain
the overrun and actual finish time. No compilation during execution.

Use closed-loop clients (a client sends its next operation after completion),
one fixed fixture configuration per engine, and isolated operations:

| Engine | Required L1 coverage |
|---|---|
| Bifrost | Native and OTLP write sweeps at 1/4/16/64 clients with a 2M-row fixture configuration; one deterministic fixture configuration with the required signal projections. Parquet point lookup at 1/4/16/64; small aggregate and scan at 1 and 64. |
| Verification | Direct verify at 1/4/16/64 clients. |
| Gateway | Added overhead at 1/4/16/64 clients, including buffered/streaming and capture shapes as declared. |

Reset or otherwise establish equivalent declared data state between repetitions;
repeated writes must not turn later repetitions into duplicate acknowledgments.
The Parquet query fixture and its fixed size/seed are declared before measuring.
Measured windows remain 10 seconds; the 2M-row configuration does not end
measurement early, and writes use fresh identities as needed. Throughput uses
the baseline regression rule with no new absolute floors. Retain only native
write ≥200k total rows/s and OTLP wire ≥30 MiB/s per measured server core as sanity
floors on each respective sweep's peak capacity; record the peak client count.
Only full default coverage counts as an engine's completed L1 suite; a selected
subset records its selection and cannot claim the omitted coverage passed.

**REQ-009 — Level 2, workload.** Each engine's complete workload targets 20–30
minutes with a hard 30-minute budget under the same clock rules; exceeding it
is INVALID. A successful complete workload includes at least 20 minutes of
experiment execution. Use open-loop traffic (arrivals follow the declared rate,
independent of response completion), a declared target rate, background work,
and concurrent observation of all applicable SLOs in each of three repetitions.
Record each repetition's SLO observations; judge each final numeric SLO against
its median repetition value, as defined above. Correctness/integrity criteria
still must hold in every repetition.
Do not combine separately measured best-case read/write windows as workload
proof. Warmup is discarded for each independent step; the stateful
overload/recovery sequence follows the repetition rule below.

| Engine | Required L2 coverage |
|---|---|
| Bifrost | 100M-row heavy fixture (today's `--heavy`); ingest at 70% of L1-measured per-core capacity, split by rows 50% OTLP spans / 30% logs / 20% native. 32 query clients: 60% point lookups, 25% dashboard aggregates (small and 1m), 10% table aggregates, 5% scans. Forge compaction on; reads while writing and full-queue behavior. Record the L1 calibration and declared server CPU budget; the budget sets load, never the measured CPU denominator. |
| Verification | Direct and background verification together, Bifrost evidence writes, backlog progression and bounded drain. |
| Gateway | Open-loop capacity with captures, declared overload, then recovery; judge completion, first-token, chunk-lag, full-stream, overhead, capture accounting, and recovery concurrently where applicable. |

For stateful overload/recovery workloads, each of the three repetitions runs
the complete capacity → overload → recovery sequence from equivalent initial
state. Its phases have separate criteria and metrics; compare the matching phase
across repetitions. Warm up the complete sequence, not recovery in isolation,
which would erase the backlog recovery is intended to measure. Other L2 steps
also declare and preserve their initial state across repetitions.

Bifrost workload load must identify an L1 run with compatible host, build,
fixture projection, and limits; missing/incompatible calibration refuses the
workload as INVALID rather than silently using a historical total rows/s.
The fixed-client L1 SLO and L2's 32 query clients have different applicability:
L2 uses the concurrent criteria below; it cannot claim a 64-client SLO
from a 32-client observation. Calibrate each signal from its matching L1 write
sweep's peak valid rows/CPU-second at 1/4/16/64 clients. For row fractions `f_i`
and capacities `r_i`, mixed capacity is `1 / Σ(f_i/r_i)` rows/CPU-second.
Initial ingest is 70% of that capacity times the declared server CPU budget.
That budget and the small/1m split within the 25% dashboard share are run
controls fixed before execution.

Concurrent Bifrost L2 criteria: point lookup p99 <5 ms; small aggregate
p95 <100 ms; 1m aggregate p95 <300 ms; heavy scan p99 <2 s; write
acknowledgement p99 ≤100 ms; write-to-query visibility p99 ≤5 s. The latter
two restore gates deleted in `97c120cc0`. Table aggregates retain correctness
and efficiency/regression criteria without a new absolute latency floor.
Backlog may grow by at most one second of offered work over the measured
window; drain completes ≤60 seconds after load stops. After overload ends,
SLOs must be met again within 30 seconds. Apply these backlog/drain/recovery
bounds to corresponding Verification and Gateway L2 phases too. The capture `lost` count must be zero always. Explicitly accounted shedding
is allowed only in the deliberate overload phase and is recorded separately
from `lost`; it cannot excuse any lost capture. Outside that phase, shedding
fails the run.

Gateway capacity constants: completion p99 ≤200 ms, first token p99 ≤350 ms,
chunk lag p99 ≤50 ms, full stream p99 ≤1,100 ms. They are declared defaults,
not merely flags; any explicit override is fixed and recorded before measuring.
The strict <500 µs overhead SLO at 64 clients remains unchanged.

**REQ-010 — Invocation and documentation.** The new public commands are:

```text
mise run bench:l1:bifrost
mise run bench:l1:verification
mise run bench:l1:gateway
mise run bench:l1
mise run bench:l2:bifrost
mise run bench:l2:verification
mise run bench:l2:gateway
```

`bench:l1` runs all three sequentially so they do not contaminate one another.
All old `bench:*` entry points are replaced without aliases. No L2 aggregate
is required. Every experiment and both levels use identical commands and record
schema on all three platforms. Each engine command accepts optional `--profile`,
forwarded through mise (for example, `mise run bench:l1:bifrost -- --profile`);
`bench:l1` forwards it to each sequential engine run. There is no platform-only
experiment or alternative command. `benchmarks/README.md` explains levels, budgets, prerequisites,
validity, metrics/units, baseline eligibility, exits, examples, and how to read
a record, with a pointer from `TESTING.md`. Commands and documentation above
are future implementation obligations, not edits authorized during this draft.

### Historical baselines and regression

**REQ-011 — Baselines.** Configure eligible historical per-core and
per-operation reference values in `benchmarks/<engine>/baselines.toml`, not as
new-format run records. Each entry identifies its source report and commit,
host, metric/unit/direction, experiment and matching step/parameters, fixture,
build/limits where known, and any known limitations.
Per metric, use the newest clean source with measured server CPU and an
established source commit. Primary candidates are the preserved 2026-10-21
reports; PR 150 pre-merge `cfe21dc90` supplies metrics without eligible primary
evidence. The [eligible/excluded value mapping](research.md#eligible-and-excluded-baseline-mapping)
is incorporated into REQ-011: its selected values, units, source identities and
eligibility/exclusion statuses specify the initial `baselines.toml` entries
being approved. Its provenance narrative remains supporting evidence. Do not
create TOML in this draft. Exclude merged `5c775110f` telemetry (uncommitted fork patch);
its standard reads are superseded. Never divide by host core count. Preserve
absent/incompatible evidence explicitly; do not invent, overwrite, or average
baselines from different setups. Historical latency and total-rate figures
remain context, not baseline criterion values.

Select the comparison baseline before measurement and store its identity and
value in the record. Cross-host and cross-OS normalized comparisons are allowed
as advisory evidence with both fingerprints visible, including OS and
architecture; neither proves a same-host code regression. Only an
explicitly declared eligible comparison judges a baseline criterion; advisory
cross-host comparisons do not silently fail an unrelated run. When a declared
baseline criterion has no usable reference, the experiment is INVALID. Dirty
or differently patched historical sources are excluded, not attributed to a clean commit. Newly recorded dirty-override runs
are exploratory and ineligible as reference baselines; they may be compared
against an eligible baseline, but cannot replace it. The mapping governs initial historical entries. This change does not introduce
automatic promotion or replacement of baselines from newly recorded runs;
dirty runs remain ineligible references.

**REQ-012 — Noise-aware regression.** A baseline criterion fails only when
per-core efficiency is more than 10% below its reference, or CPU/operation is
more than 15% above, **and** the difference is beyond the run's own repetition
spread. The maintainer-approved comparison rule: for three valid values, let `m`
be median, `lo` minimum, `hi` maximum, `s = hi-lo`, and `b` the baseline. For
higher-is-better efficiency, fail only if `hi < 0.90b` and `b-m > s`. For
lower-is-better CPU/operation, fail only if `lo > 1.15b` and `m-b > s`.
Exact equality at either percentage boundary does not fail. This conservative
rule requires all three repetitions beyond the percentage limit and a shift
larger than their spread. It does not establish statistical confidence or hide
an absolute SLO/correctness failure. Record the operands and both comparisons.

Example, baseline 100 work/CPU-second: `[75,80,85]` fails (20% median drop,
spread 10, even the best repetition below 90); `[70,85,100]` passes the
regression criterion (15% drop, spread 30, best repetition recovers). The latter
means “no demonstrated regression,” not “equal speed.” Repeat a suspect
complete experiment under matching controls before attributing a code regression;
three repetitions are mandatory even on the first run. The historical ~25%
write swing and ~2.5× fsync swing motivate preserving spread rather than judging
a single throughput number. This resolves D-4's comparison decision.

### Record and report contract

**REQ-013 — One persisted record.** The shared recorder writes each completed,
failed, or discarded experiment to:

```text
benchmarks/<engine>/<experiment>/<UTC timestamp>-<short sha>.json
```

Engines are exactly `bifrost`, `verification`, `gateway`. Experiment identifiers
are stable lowercase path-safe names. Filenames are
`YYYYMMDDTHHMMSSZ-<9-char sha>.json`, using UTC and the first nine commit hex
digits; the full SHA is authoritative in JSON. Never overwrite a run: append
`-2`, then `-3`, etc. before `.json` on collision, retaining the actual start.
Records are intended to be committed
for historical audit through normal contributor review; running a benchmark
never stages or commits automatically. Raw samples, logs, and profiles remain
under `target/` with optional record references, not embedded in the summary.

Schema version 1 is an explicit persisted contract. The following field names
and meanings are proposed for approval; nested metric/criterion entries carry
stable identifiers and explicit units rather than formatted table strings.

| Required JSON field | Meaning |
|---|---|
| `schema_version` | Integer 1. Breaking semantic/shape changes require a new version; readers reject unsupported versions explicitly. |
| `engine`, `experiment`, `level`, `question` | Declared identity, level integer 1/2, and question. |
| `started_at`, `finished_at` | UTC RFC 3339 instants, including partial/INVALID runs. Elapsed budgeting uses monotonic time, also stored. |
| `commit`, `dirty`, `dirty_override` | Full measured-source SHA, source dirtiness at declaration, explicit override state. Dirty runs store `dirty: true` and `diff_sha256`, SHA-256 of the bytes from `git diff HEAD`. Output created by this run does not retroactively dirty its source. |
| `host` | OS/version, architecture, CPU model/logical count, physical memory, and declared storage context; counts are metadata, not efficiency denominators. |
| `build` | Profile, feature/compiler flags, compiler and binary identities; detect a stale binary rather than attribute it to the current tree. Existing build stale-binary detection applies; no additional source-to-binary correspondence validity rule is required. The diff digest does not make a dirty run baseline-eligible. |
| `command` | Exact invocation as argument values plus the non-secret configuration needed to reproduce it. |
| `variables`, `controls`, `criteria` | Sweep and fixed controls, validity/budget declarations, metric definitions and criterion IDs/operators/limits/units/applicability, chosen baseline snapshots. |
| `steps` | Ordered declared steps: IDs/params, timings, metrics with units, all three repetition summaries, median/min/max spread, per-criterion observed values/verdicts/reasons, step verdict. Missing observations are null with reasons, never NaN or infinity. |
| `validity`, `verdict`, `reasons` | Check observations, overall PASS/FAIL/INVALID, structured reasons for a literal agent reader. |
| `artifacts` | Optional local `target/` references and capture completeness; historical summaries remain readable without them. |

Recording failure cannot produce a successful result: exit 2 with an explicit
persistence error and any salvageable local evidence. Complete records are
published by writing a temporary file then renaming to the collision-safe destination. A caught interruption retains partial INVALID evidence;
an abrupt kill/power loss cannot guarantee a final record and must not leave
a partial JSON document masquerading as complete evidence.

**REQ-014 — One report.** Render every human report from the persisted record,
including invalidity reasons, spread, and overall verdict. The common columns
are exactly:

```text
| step | params | rate | p50/p95/p99 ms | errors | server CPU | peak mem | driver CPU | criterion | observed | verdict |
```

Use one row per step/criterion when needed. Units accompany values; server CPU
shows consumed seconds and mean used cores, and driver CPU is separate. A
non-applicable value is `—` with a reason. Additional engine metrics and spread
are rendered from the same record as supporting detail. Reports can be
regenerated without rerunning the experiment or loading raw samples. Console
and regenerated summaries must agree with JSON, including FAIL and INVALID.

### Shared owner and retirement

**REQ-015 — Reuse and consolidation.** Use one `experiment` module in
`wyrd-testing` for the complete declaration → validity → warmup/repetitions →
measurement → criteria → record → report workflow. Build from existing
`capacity.rs` measurement/binary identity, `release_server.rs` host/plain-child launch,
`release_server/os.rs` portable process readings, and `profile.rs` capture;
Gateway's `Report`/`Row` supplies the base for the unified report. Reuse engine
production setup and client workflows rather than introduce another server
launcher. Private helper shapes remain implementation choices. Profiling is
optional diagnostic evidence requested with `--profile`, captured by external
`samply` with identical invocation on every platform. Samply samples through
perf events on Linux and natively on macOS; the harness has no direct `perf`
dependency. Profiles never participate in criteria or verdicts and never supply
the CPU denominator. A missing profiler or unusable capture only disables
profiling with a recorded summary/report note; it never makes a run INVALID or
FAIL and cannot waive required measurements. The runner never changes OS
permissions or limits to obtain a profile. See [portability research](research.md#os-agnostic-benchmarks-approved-maintainer-decision)
for current-source evidence and diagnostic limitations.

Fold query's private Usage and percentile calculation, client-queue quantile,
Forge's Host, repeated memory-headroom rules, report writers, and workload flag
parsing into the shared owners. Inspection already finds shared `flags.rs`;
reuse it rather than assume four wholly independent parsers still exist.

**REQ-016 — Retire obsolete paths.** Remove `benches/thresholds.toml`,
`wyrd-bench::SloGate` and its dead loader/tests/export, unused
`wyrd-testing/src/load/` and its export, and superseded duplicate measurement,
host, percentile, reporting, and parsing code. Remove stale references without
widening gates. Remove direct `perf record`, `perf report`, and `perf script`
capture/report processing in `profile.rs`, replacing it with optional samply
diagnostics. Remove `systemd-run --scope` placement, cgroup-v2 discovery through
`/proc/<pid>/cgroup`, and cgroup CPU/memory/limit reads in `release_server.rs`.
Remove Forge capacity's systemd user-scope envelopes (leader 1 CPU / 4 GiB,
each worker 7/3 CPU / 4 GiB), their enforcement and scope-derived accounting.
These are removals, not retained optional Linux modes.
Preserve still-used benchmark primitives and production
correctness coverage. Evidence for current consumers is in [research](research.md).

| Current coverage | New owner / level |
|---|---|
| query-capacity standard + write sweeps | Bifrost L1 |
| heavy fixture, reads-while-writing, full queue, forge-capacity, client-queue | Bifrost L2 |
| verification case 1 / case 2 | Verification L1 / L2 |
| gateway-overhead / gateway-capacity | Gateway L1 / L2 |

This migration retains the workload purposes, not every historical flag,
verdict label, absolute floor, scoped launcher, or report layout. Historical
criteria are reconciled explicitly: L2 restores write p99 ≤100 ms and visibility
p99 ≤5 s. Historical first-frame p99 ≤500 ms and raw latency compare gates
−10% throughput/+15% p95/p99 are not reactivated. The new declared criteria own judgment; relative
regression compares efficiency and CPU/operation, not raw p95/p99. Point-lookup
<5 ms at 64 clients replaces selective targets at 1 client, including the later
≤1 ms target. Gateway's historical MIN_CALLS 100→30 is not silently adopted as
p99 sufficiency. Capture shedding is counted separately and allowed only in deliberate overload; any nonzero `lost` count always fails.
Gateway capacity SLOs become declared constants with recorded explicit overrides,
rather than required ad hoc CLI flags alone. The constants above resolve D-1/D-2.

## Invariants and constraints

| ID | Promise and failure prevented |
|---|---|
| INV-001 | Production server/client paths, exact result checks, authentication, tenant binding and permission checks stay enabled. A benchmark cannot obtain speed by bypassing real work or reading another tenant's data. |
| INV-002 | Acknowledged durability and publication semantics remain unchanged. Report accepted work, durable publication, and visibility distinctly; benchmark convenience cannot weaken storage guarantees. |
| INV-003 | Authorization still stages non-blocking audit at each permission decision; its production cost stays measured. Benchmark records are repository evidence, not canonical `vala.system.audit_log` rows. |
| INV-004 | Server/driver accounting is disjoint and portable on all supported platforms. Host core count, missing CPU, or an unavailable profiler cannot fabricate favorable efficiency. |
| INV-005 | Declared controls/criteria cannot change after observing performance. Invalid measurements never become baseline evidence or a performance FAIL. |
| INV-006 | Committed records contain synthetic-fixture summaries only: no keys, tokens, private endpoints, tenant payloads, or sensitive host identifiers. Reproducibility records symbolic secret inputs, never secret values. |
| INV-007 | No compilation or synthetic host load runs during an experiment. A busy real host is discarded; validity tests inject observations rather than deliberately saturate the host. |
| INV-008 | Summary history is append-only and attributable; no fabricated historical run, overwritten record, or silent unknown schema acceptance. |
| INV-009 | Every experiment at L1 and L2 runs unchanged on macOS arm64, Linux x86_64, and Linux arm64 with identical commands and record schema. The harness uses no cgroups, namespaces, `/proc`, `prlimit`, systemd scopes, credential switching, seccomp, Landlock, sandbox-exec, or `cfg(target_os)` gating; a platform-specific request is invalid. This prevents platform-only workloads and accounting. External samply owns its native sampling backend; optional diagnostics do not relax the harness rule. |

Fixtures use disposable benchmark-owned state. Cleanup cannot touch an operator's
unrelated data. Cancellation stops admission, drains/cancels owned work under
bounds, and reports incomplete evidence. Existing production queue, resource,
and tenancy limits remain effective; a full queue is measured, not unbounded.
The shared benchmark owner does not migrate durable server behavior into an SDK
or into `wyrd-spec`. No new dependency is required: sysinfo is already in the
workspace and samply is an optional external tool. The approved tools are
constraints; private implementation choices remain open.

## Expensive-to-reverse decisions proposed for approval

1. **Ownership:** one `wyrd-testing::experiment` workflow for all three engines;
   engine-specific workload behavior remains with existing owners.
2. **Persistent evidence:** versioned JSON schema v1, exact three-engine directory
   layout, timestamp/SHA identity, committed summaries and external raw artifacts.
3. **Levels:** L1 ≤15 minutes, L2 20–30 minutes per engine, three repetitions per
   step, defined clock boundaries, opt-in commands replacing old tasks without aliases.
4. **Metric semantics:** server CPU-seconds as denominator, explicit byte units,
   timing boundaries, process scope, spread, fixed-client SLOs, portable readings.
5. **Baseline contract:** TOML references with provenance; no historical record
   import; eligibility visible; conservative percentage-plus-spread regression.
6. **Portability:** identical experiments, commands and schema on all three
   platforms; plain-child servers, sysinfo readings and optional samply
   diagnostics, without OS resource envelopes.
7. **Validity versus failure:** INVALID has precedence and exit 2; valid criterion
   misses exit 1; every step is judged, including overload/recovery.

The maintainer approved the D-1–D-4 decisions and OS-agnostic benchmark decision
reflected here; the whole revision still awaits separate approval. Changing these meanings after approval needs a spec
revision; choosing private functions, algorithms, and fixture file structure does not.

## Acceptance: user and agent journeys

These are future evidence obligations, not claims that verification ran while
writing the draft. Stable IDs map promises once in the final column. Real
workload journeys use the actual release server through public SDK/OTLP/Gateway
surfaces and repository Postgres/storage dependencies. Where suitable, existing
`WyrdTestServer` journeys verify production seams; they do not substitute for
release-process benchmark evidence. This change adds no Python, TypeScript, or
MCP API that would require a new product-surface projection.

| ID / situation and actor | Expected observable result / credible evidence | Obligations |
|---|---|---|
| AC-001 — Engineer runs one engine's L1 | Each of the three engine commands is exercised separately. Within its budget the full matrix runs with discarded warmup and three repetitions. Engineer reads question, controls, units, per-step criteria and verdict; opens the printed JSON path and regenerates an identical report. Real release-process journey, record and report comparison. | REQ-001, REQ-003, REQ-007, REQ-008, REQ-010, REQ-013, REQ-014 |
| AC-002 — Engineer compares a valid regression with its baseline | Efficiency baseline 100, repetitions 75/80/85: criterion FAIL and exit 1; a CPU/operation case beyond +15% and spread also fails. Exact percentage boundaries pass. Deterministic judgment cases plus an eligible baseline-to-run integration journey preserve operands and provenance. | REQ-011, REQ-012 |
| AC-003 — Engineer encounters noisy results | Baseline 100, repetitions 70/85/100: regression criterion PASS with spread visible; another absolute SLO miss still makes the run FAIL. Shared-judgment cases prove noise cannot excuse latency/correctness. | REQ-004, REQ-012 |
| AC-004 — Engineer cannot obtain valid evidence | Independently exercise busy preflight/post-run host (10-second samples, >10% non-benchmark CPU), driver above 80% budget, any missed arrival or p99 send lateness >5 ms, insufficient samples (<1,000 p99 / <200 p95), dirty source without override, missing/reset CPU and budget overrun: INVALID, exit 2, reasons and partial record, no fabricated successful step. Declared dirty override is visible. Combine an invalid step and valid failure: overall INVALID retains both. Injected resource/clock observations and runner integration; no synthetic host load. | REQ-002, REQ-004, REQ-006, INV-005, INV-007 |
| AC-005 — Engineer runs and compares supported platforms | Prove real CPU accounting and every L1 experiment for all three engines on macOS arm64, Linux x86_64, and Linux arm64, using the same commands and schema v1 on each. Recompute W/C and C/W from records; results agree and never divide by host count or removed quotas. Cross-host and cross-OS comparisons show both OS/architecture fingerprints and remain advisory, never same-host regression proof. Exercise `--profile` on each platform and missing/unusable profiler cases: capture or an explicit note is recorded without affecting judgment; required accounting stays mandatory. Real release-process journeys/records plus deterministic arithmetic, provenance and diagnostic-failure checks. Static command/source review confirms both levels and all experiments lack OS-specific paths and resource envelopes. | REQ-006, REQ-010, REQ-011, REQ-013, REQ-015, REQ-016, INV-004, INV-009 |
| AC-006 — Agent reads history without prose parsing | Agent discovers engine/experiment records, validates v1 fields, reads numeric values/units, repetitions, baseline snapshot and typed verdict/reasons, and identifies omitted/invalid steps. Unsupported version refuses interpretation. Report regeneration works without target artifacts; same-second/SHA collisions use -2/-3 and cannot overwrite history; temp-write/rename failure cannot return PASS. A dirty override records the SHA-256 of `git diff HEAD`. Record-reader/writer integration and malformed/unsupported input cases. | REQ-013, REQ-014, INV-008 |
| AC-007 — Engineer runs L2 with competing work | For each engine, observe real overlapping foreground/background work during each repetition, offered versus achieved arrivals, concurrently evaluated SLOs, bounded backlog and drain. Bifrost records compatible L1-derived initial load and 32 query clients; Verification includes evidence writes; Gateway overload/recovery judges capture accounting. A missed concurrent criterion fails despite an isolated case passing. Real workload journeys, timelines and records. | REQ-005, REQ-009 |
| AC-008 — Contributor adds an experiment | Contributor declares question/variables/controls/metrics/criteria and supplies workload behavior to the existing shared experiment owner. The same validity, repetitions, report, record and verdict behavior applies without another measurement/report implementation. One small representative integration experiment and source review demonstrate reuse. | REQ-015 |
| AC-009 — Maintainer checks migration and discoverability | README and TESTING explain invocation and records; coverage mapping has no lost workload; old tasks/aliases, dead thresholds/SloGate/load exports and duplicate code are gone. Every client-queue step judges criteria; Gateway has declared capacity SLO constants. Static consumer/task/docs inventory plus relevant existing correctness lanes; no benchmark added to CI/gates. | REQ-010, REQ-016 |
| AC-010 — Operator checks preserved production guarantees | Wrong results and unexpected loss fail valid runs; tenant-negative journeys cannot expose data; acknowledged writes survive existing durability checks; audit stays non-blocking and accounted; fixture cleanup remains scoped and credentials never enter summaries. Existing security/storage journeys and focused record privacy/cancellation checks are credible evidence. | INV-001, INV-002, INV-003, INV-006 |

No benchmark execution is needed to review this draft. Implementation planning
must select exact focused commands and evidence under approved decisions; no
new test names or speculative task packet is prescribed here.

## Open material decisions

D-1, D-2 and D-4 are resolved. D-3's selection/exclusion policy is resolved,
but primary provenance remains incomplete: historical cleanliness of the four
Bifrost-worktree reports and exact source/cleanliness of the Gateway report
could not be established. Research records these exclusions and eligible
fallback entries. No missing Verification or Gateway baseline is invented.
Remaining D-3 question: can contemporaneous clean-source/build evidence be
recovered for these primary reports? Until then they are excluded.

The maintainer approved revision 3 as a whole, including the selected
baseline values and eligibility statuses incorporated from the research
mapping, and its explicit provenance exclusions. The D-3 primary reports stay
excluded unless clean-source evidence is recovered.

## Revision history and authority

- Revision 1 — 2026-10-23: Initial draft from decided maintainer intent; current
  owners and historical provenance checked, with unresolved constants and
  source eligibility identified explicitly. No implementation tasks created.

- Revision 2 — 2026-10-23: Applied maintainer D-1–D-4 decisions for validity,
  workload/SLOs, baseline eligibility, noise and persisted records. Preserved
  five original reports, corrected the missing-report claim and mapped fallback
  values and primary provenance exclusions. Draft awaiting separate approval;
  D-3 evidence gap remains explicit. No code or benchmarks changed/run.

- Revision 3 — 2026-10-23: Folded in the approved OS-agnostic benchmark
  decision: plain-child servers, sysinfo resource/validity readings, optional
  samply diagnostics, explicit Linux-only retirement, advisory cross-OS
  comparison and three-platform L1 proof. Whole-spec status remains draft;
  the existing D-3 evidence gap is unchanged. No code or benchmarks changed/run.
  Approved by the maintainer as a whole on 2026-10-23; the older optimization
  draft packet was deleted.

Authority: [AGENTS.md](../../../AGENTS.md),
[spec-driven development](../../spec-driven-development.md),
[Wyrd Doctrine](../../../architecture/wyrd-design.md#doctrine) and
[engine boundaries](../../../architecture/wyrd-design.md#bifrost),
[Bifrost system boundary](../../../architecture/bifrost-design.md#system-boundary),
[measurement meanings](../../../architecture/bifrost-design.md#measurement-meanings),
[resource invariants](../../../architecture/bifrost-design.md#resource-and-failure-invariants),
[security posture](../../../architecture/wyrd-security-posture.md#security-principles),
[tenant isolation](../../../architecture/wyrd-security-posture.md#tenant-and-data-isolation),
[audit privacy](../../../architecture/wyrd-security-posture.md#audit-integrity-and-privacy),
and [agent surfaces](../../../agent_docs/agent-surfaces.md).
[Research and historical criteria](research.md) support this specification.
Only the selected values, source identities and eligibility statuses in its
[baseline mapping](research.md#eligible-and-excluded-baseline-mapping) are
incorporated as normative data under REQ-011; the remaining research does not
create parallel requirements.
