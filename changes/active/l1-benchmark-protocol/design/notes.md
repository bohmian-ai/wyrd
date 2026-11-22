# Independent L1 benchmark protocol

Status: exploring — awaiting maintainer approval

Repository inspected: `d2f37d07b4c6e595ecdc9f59bb27cfa467be16b0`, 2026-11-22.
This is a new design packet. Benchmark execution remains on hold. No builds,
benchmarks, specification, tasks, or changes outside this packet were made.

## Problem

A maintainer comparing a change needs to know whether the operation improved.
Today, a named L1 step can inherit data, caches, clients and background work
from earlier steps. Selecting fewer steps can change the work being measured.
Three windows in that same environment do not measure variation between fresh
executions. A comparison of their ranges cannot establish repeatability.

This affects **all L1 families**, not just writes. The shared runner prepares
one workload before iterating over its steps. Bifrost UI SQL moves one fixture
forward through residences; Gateway changes capture policy in one server;
Verification retains its server and SDK drivers. The current reporting contract
also depends on implementors assembling the answer correctly. Relevant source
locations are collected under Evidence below.

## Desired outcome

Running one named benchmark alone, in a subset, or inside a suite executes the
same declared workload and lifecycle. For example, `otlp-logs-c64` gets its own
fresh environment whether or not spans or `otlp-logs-c1` were selected. Its
result identifies exactly what completed, what CPU was charged, and what work
remained afterward. `capacity` computes and prints the report and comparison;
the implementor pastes the output unchanged.

An experiment name such as `writes-parquet` becomes a **collection of independent
benchmarks**. One benchmark is one measured operation/fixture/client-count/load
combination, formerly a measured step. A suite is just a selection of those
benchmarks. Collection and suite elapsed times are sums, not ten-minute budgets.

## Constraints and non-goals

### Binding maintainer decisions

- "each step is in and of itself a benchmark." Every measured L1 step gets
  fresh server process, PostgreSQL data and processes, WAL, object storage and
  client state. Nothing from another benchmark carries over. Selection and
  order must not determine its result. A subset is fewer benchmarks, not a
  diagnostic mode.
- "Warmup should be 30 seconds"; measurement is "3x30". "Drain, capture,
  teardown can be longer if they need".
- "L1 benchmarks should be under 10 minutes" applies to **each benchmark**.
- "it should be simple and standardized". The proposed twenty-run comparison
  and 300-second warmup were rejected.
- "we should never rely on an agent to create a summary table. it should be
  standardized so its calculated the same each time with no hallucinations".
- "number change based on sequence of steps or what is run (red flag)" and
  "implementing agent never follows benchmark/AGENTS.md": move enforceable
  rules into the tool.
- Existing history is in question; benchmarking stays on hold until the rebuild.
  Preserve measurable writes-parquet throughput **and** input MiB/server
  CPU-second improvement, and the **80** target on `otlp-logs-c64`.

Everything below that is not one of these decisions or an existing authority
is a proposal for approval. In particular, the number of independent comparison
executions, phase allowances and statistical policy are not already approved.

### Limits

This changes the benchmark harness and its evidence contract, not server
durability, production rotation, compression, concurrency or workload semantics.
It does not turn L1 into a sustained-load experiment or replace L2. No automatic
steady-state search, adaptive warmup, favorable-run retry loop, new telemetry
pipeline, or general experiment service is proposed.

Fresh application environments cannot reset host device caches, Docker's VM,
temperature or unrelated host activity. The enforceable guarantee is no inherited
application work plus identical setup. Numerical repeatability and absence of
systematic order effects need measured evidence. Unexplained order effects remain
a qualification blocker; ordinary random variation is not bitwise inequality
of results. No OS cache flushing or platform-specific isolation is proposed.

## Proposed implementation

### 1. What the number means

One result measures a fixed workload after its own 30-second warmup, in three
30-second admission windows. Keep its environment through all four windows;
reset it only when beginning another benchmark execution. Warmup intentionally
warms this benchmark's caches and creates this benchmark's background work.
It is fixed preconditioning, not proof of steady state.

For window `j`, retain raw successful work `Wj`, acknowledged input bytes `Bj`
where applicable, elapsed wall seconds `Tj`, and whole-server CPU seconds `Cj`.
Successful work finishes before that window's admission stop. Errors, retries,
refusals and completions after the stop have separate counts. A retry never
increases successful logical work twice. Preserve the exact fixture and response
checks; an incorrectly answered operation is not useful throughput.

The benchmark's rate and efficiency are ratios of totals, never averages of
window ratios:

```text
work/s                   = sum(Wj) / sum(Tj)
work/server_CPU-s        = sum(Wj) / sum(Cj)
server_CPU_us/work       = 1,000,000 * sum(Cj) / sum(Wj)
input_MiB/s              = sum(Bj) / 1,048,576 / sum(Tj)
input_MiB/server_CPU-s   = sum(Bj) / 1,048,576 / sum(Cj)
work/s/provisioned_vCPU  = work/s / P
```

Print all three paired window readings as well as the combined result. `Cj`
includes every server thread and the existing accounting for server-owned
workers, including background work executing during that interval. PostgreSQL,
the controller, SDK client processes and local provider CPU are identified
separately, not silently added to server CPU. Keep the existing resource owner
in `capacity.rs` / `release_server` rather than substituting a sampler.

OTLP input bytes mean the uncompressed protobuf message length acknowledged by
the server, before transport framing, counted from the actual request. Retained
Arrow, WAL, staging and published Parquet bytes are distinct diagnostics. Native
currently has **no input-byte measurement**; its row throughput/efficiency remains
meaningful. Do not relabel retained Arrow as native input or print zero input
efficiency. The native mandate question is explicit below.

Keep server latency and per-request overhead boundaries. Overhead subtracts the
union of that request's provider/Verifier waits, never differences of aggregate
percentiles. Combine histogram counts to obtain a benchmark percentile; never
average window percentiles. Keep the three window percentiles, sample counts and
bucket bounds. Existing p99/p95 floors (1,000/200 per window when judged) remain;
insufficient samples make the affected benchmark INVALID, without extending its
window. A purely diagnostic percentile below its floor says unavailable.

Proposed per-run performance targets use these combined totals or pooled
histograms, with unchanged threshold values and strict/non-strict operators.
This deliberately replaces the current median-of-three target aggregation;
it is part of the new protocol and baseline, not an assertion that the old
and new judgments are interchangeable. Correctness and sample-floor checks
still apply to every window.

**Boundary separation:** at each stop, freeze successful work, CPU and interval
counters against timestamped boundaries. Then drain in-flight requests before
opening the next window. These gaps are recorded and excluded from throughput
and CPU denominators. They do not reset data, processes or caches, nor wait for
all publication between windows. Record snapshot acquisition offsets so delayed
scrapes cannot masquerade as simultaneous samples. Missing or inconsistent
required boundaries invalidate the benchmark.

Keep two explicitly labelled evidence populations: (a) interval CPU/counters
between open and stop; (b) request-cohort latency/integrity, collected through
request drain, including separately counted late completions. The latter cannot
explain the exact CPU cost inside the former. This reuses the existing `Window`
and `Population` distinction and corrects its reporting ambiguity. All timing
series and diagnostics carry their boundary and population.

The main write result therefore means **short-window durable-acknowledgement
throughput and CPU efficiency**. It includes background work actually executing
during those windows, not necessarily all work caused by their writes. Print
publication movement, backlog start/end and post-stop work separately. Zero
completed files is never described as zero publication CPU. A supplemental
warmup-through-final-drain total may expose deferred cost, but is labelled with
that larger population and never substituted for the headline.

Final drain means closing workload admission, completing in-flight calls and
producer queues, and checking the workload's existing durability/integrity
boundary: acknowledged append custody for writes, execution/evidence accounting
for Verification, capture accounting for Gateway, and exact answers for reads.
Required fixture publication happens in setup. L1 final drain does not require
every later compaction or maintenance job to finish; record remaining publication
and maintenance work before disposal. This distinction prevents “drained” from
being mistaken for completed lifetime storage cost.

### 2. Lifecycle and under-ten-minute bound

Reuse the existing supervisor, PostgreSQL wrapper and `LocalServer`. Move their
ownership to one measured benchmark per child execution. The parent holds the
schedule and final record; it has no workload clients or provider. Each child
has fresh Rust runtime/recorder/client state, and fresh Python/Node/provider
processes when required. It receives one complete declaration, never the
previous child's mutable state. Public commands continue to enter through mise.

Build the server, driver and required SDK modules **before** starting a timed
benchmark, as today. Prepare both comparison builds before executing either;
never compile between paired measurements. Print build preparation time
separately. Missing/stale binaries refuse execution; compilation is not hidden
inside setup. The ten-minute clock starts before PostgreSQL creation and ends
after teardown and durable result capture, not at the first request.

Proposed uniform maximum allowances:

| Phase | Allowance per benchmark | Work included |
|---|---:|---|
| Setup | 240 s | Preflight/quiet-host sample, PostgreSQL creation, migrations, fresh server and clients, fixture loading, required learning/placement, exact fixture checks |
| Warmup | 30 s | Only this benchmark's declared workload |
| Measurement | 90 s | Exactly three 30-second admission windows |
| Drain and capture | 150 s total | All inter-window request drains, final request/outbox reconciliation, required storage settlement, post-host sample and evidence capture |
| Teardown | 60 s | Stop/reap server, providers and SDK children; remove PostgreSQL processes/volumes and disposable WAL/object storage |
| Persist and report | 20 s | Finalize lifecycle evidence, atomically keep record, render the fixed result |
| **Maximum** | **590 s** | **9 minutes 50 seconds, leaving 10 seconds below ten minutes** |

These are maximum allowances, not sleeps. Drain can take much longer than a
window. Allowances are shared across their phase occurrences, not renewed after
each window. An absolute monotonic deadline prevents a nested retry or grace
period from extending them. Preserve the existing 45-second server stop allowance
inside teardown. Cancellation begins while cleanup time remains, and every
blocking subprocess has a timeout and an owned cleanup path.

This arithmetic proves the **admitted execution budget**, not that all fixtures
will complete successfully in it. Current PostgreSQL creation/teardown sits
outside the runner's clock, and `finish` adds its own grace; both must move under
supervision. A deadline exceeded, incomplete cleanup or failed record makes the
attempt INVALID. Stop the entire selection if cleanup cannot be proven; do not
start the next benchmark beside surviving work. A hung OS/Docker can defeat a
wall-clock completion guarantee: report the overrun and preserve the failure,
never claim that a timeout proves successful teardown.

**Proof required after execution is resumed:** deterministic clock tests exercise
setup, drain and teardown timeouts; real lifecycle tests confirm process/volume
removal; full-size runs on supported benchmark hosts record every phase and finish
below 600 seconds. The highest-risk cases are repeated 10M-row hot/Parquet setup,
UI learning/publication and 120 seconds of c64 write/capture backlog. Their
successful runtime is presently **unproven**. If one cannot fit, it remains a
reported unsupported/INVALID case pending a design decision; do not shrink its
fixture, reduce warmup/windows, reuse another case's data, or relabel it silently.

### 3. Checked isolation invariant

Before warmup, the tool must establish all of the following, and retain the
evidence in the result:

1. The preceding environment's owned processes are reaped and its PostgreSQL
   project and volumes disposed. This environment has a distinct ownership ID,
   fresh storage roots and newly created PostgreSQL data. PID numbers alone are
   insufficient because the OS may reuse them.
2. Before fixture loading, workload tables/catalog, WAL and object roots contain
   no earlier workload state. Expected bootstrap/system records are declared;
   “empty” does not prohibit migrations or tenant setup.
3. The selected benchmark's fixture manifest accounts for all intentional data,
   including learning samples and seeded gateway/verification records. Declared
   rows, identities, layout and residence are checked before warmup. No cached
   fixture snapshot, published object or learned layout is imported from another
   benchmark. Immutable generator code and built binaries may be reused.
4. Only this benchmark's clients and declared fixture/background producers exist.
   Their sequence cursors, credentials, connection pools, counters, provider
   ledgers and SDK interpreters start afresh. A server restart or unexpected
   replacement during measurement invalidates the result.
5. Final reconciliation accounts for acknowledgements/captures/executions against
   their declared durable ownership and checks exact results. Inspect durable
   rows/files where authority is needed; zero gauges alone do not prove custody.
   Remaining background work is captured before disposal. Teardown completion
   belongs to validity, not a best-effort message after a VALID record.

Any missing proof or violation is **INVALID**, with a stable reason and retained
attempt. The environment ID stays in benchmark evidence, not production metric
labels. A fixture may deliberately contain multiple tables and a learned layout;
it must rebuild them for every benchmark. Isolation does not mean deleting the
current benchmark's warmup effects.

### 4. Complete L1 inventory and mapping

Source enumeration finds **48 default measured benchmarks**: Bifrost 31,
Verification 9, Gateway 8. Explicit-only choices add **17**: UI arms 12,
Verification concurrency 3 and model 2, for **65 named measured benchmarks**.
Gateway `--pace` produces rate-parameterized variants, not a finite extra list.
This corrects stale prose describing only three Bifrost families and three
Gateway cases. Braces below expand to each listed value; no combination is implied
unless explicitly stated. Every measured entry uses the lifecycle above.

| Existing command/family | Exact measured IDs and count | Fresh preparation for **each** ID | Result and byte source |
|---|---|---|---|
| `bench:l1:bifrost:writes` / `capacity l1 bifrost writes-parquet` | `otlp-spans-c{1,64}`, `otlp-spans-1k-c{1,64}`, `otlp-logs-c{1,64}`, `native-write-c{1,64}` — 8 | Only the chosen sender, its existing generator, batch size, production rotation and 30 s retention. Recreate 2M-row span identity population; retain existing logs/native definitions. No previous path or c1 backlog. | OTLP input MiB/s, input MiB/server CPU-s; all paths rows/s, rows/CPU-s, CPU/row, ACK latency, memory, errors, publication/backlog. OTLP actual uncompressed protobuf length. Native input bytes unavailable today; retained Arrow and Parquet never substitutes. [I1] |
| `bench:l1:bifrost:hot` / `l1 bifrost hot` | `point-memtable-c{1,64}`; `point-staged-c1`; `small-aggregate-{memtable,staged}-c1`; `span-shredded-{memtable,staged}-c1`; `span-long-tail-{memtable,staged}-c1` — 9 | Rebuild the **whole declared** 10M-event/1M-span fixture, half staged/half memtable, after the separate published 500k-span learning sample. Existing large rotation/24 h retention pins residence; check it through the run. Do not load just the half queried. | Queries/s, queries/CPU-s, CPU/query, server/client latency, exact answers; memtable point c64 p99 <5 ms. Physical scan bytes only when there is a physical scan; live decoded Arrow bytes labelled separately. No ingest input MiB rate. [I1, D1] |
| `bench:l1:bifrost:parquet` / `l1 bifrost parquet` | `point-c{1,64}`, `small-aggregate-c1`, `full-scan-c1`, `span-trace-c{1,64}`, `span-shredded-c1`, `span-long-tail-c1` — 8 | Rebuild 10M events, 1M spans, 200k-row native/log storage samples and settle; perform existing layout/pruning checks before warmup. Recreate the same fixture even for one selected query. | Queries/s, queries/CPU-s, CPU/query, latency (full scan p95), exact answers; point c64 p99 <5 ms. Executed-plan scanned rows/bytes/query are physical facts. Compression and storage ratios are setup facts, described below. [I1, I2] |
| `bench:l1:bifrost:ui` / `l1 bifrost ui-sql` | `{memtable,staged,published}-ui-c{1,8}` — 6 | Rebuild default 500-request journey and seeded gateway/verifications. Establish its own memtable reference answers, then place directly into the requested residence before warmup. No dependence on a previous residence benchmark. | Query rate/CPU cost and server latency; page/statement client latency and counts for traces, verification, logs and metrics. Frozen SQL corpus digest and exact typed answers. Physical Parquet scan bytes distinct from live examined rows/decoded-column Arrow bytes; fixture payload volume is not input throughput. [I3] |
| `bench:l1:bifrost:ui:10x` / `l1 bifrost ui-sql-10x` | Same six IDs under the distinct family — 6 | Same procedure with 5,000-request journey and declared seeding. Fresh preparation for every residence/count. | Same metrics/sources as default UI; workload size is part of identity. Cross-arm volume comparison is a labelled workload comparison, not a before/after optimization verdict. [I3] |
| `bench:l1:bifrost:ui:learned` / `l1 bifrost ui-sql-learned` | Same six IDs under the distinct family — 6 | Publish its own 10,000-span learning sample and verify a learned layout, then load the default journey and place it. The learning sample is declared fixture data, not contamination. | Same UI metrics/sources, with layout fingerprint and learning evidence. Canonical versus learned is an explicitly different treatment, never silently a matching baseline. [I3] |
| `bench:l1:verification` / `l1 verification`, `direct-verify` | `direct-verify-c{1,64}`, `direct-verify-python-c{1,64}`, `direct-verify-typescript-c{1,64}` — 6 | Fresh canonical Verifier (one Assert/two Agent checks), applications, local provider and only the selected SDK driver. Each Agent makes two exchanges and one `code.run`. Fresh credentials, provider ledger and SDK process per benchmark. | Verifications/s and /CPU-s, CPU/verification, server overhead and total latency, client call-return/result-ready/materialization latency, server concurrency and phases. c64 overhead p99 <500 µs. The 2 KiB context is a fixture size, not measured wire/input MiB. Compare closed-loop results within the same SDK. [I4] |
| Same Verification command, declared paced cases | `direct-verify-r3000`, `direct-verify-python-r3000`, `direct-verify-typescript-r3000` — 3 | Same fresh fixture; 64 clients offer 3,000/s with the existing scheduled-arrival rule in every window. | Same readings plus offered/sent/missed/late arrivals; misses or start-lateness p99 >5 ms invalidate. Matched-rate language comparison allowed; no c64 overhead target on these rows. These **fit** the lifecycle: standardized timing does not require closed-loop load. [I4, D1] |
| Same Verification command, explicit `--steps` concurrency choices | `direct-verify-c4`, `direct-verify-c16`, `direct-verify-c32` — 3 | Same canonical Rust fixture, recreated for chosen count. Register these as explicit available benchmarks, without selector-dependent workload construction. | Same Verification metrics, no invented c64 target. Eligible for their own comparisons despite being optional. [I4] |
| Same Verification command, explicit model choices | `direct-verify-model-c{1,64}` — 2 | Fresh Rust Agent-only model fixture: one iteration through local Gateway to immediate OpenAI-compatible provider with fixed usage and cited verdict. Do not use the canonical two-Agent fixture. | Verification/overhead/client/CPU readings, model exchange accounting; no remote-provider/TLS/retry/fallback/stream/tool performance claim. Context/token constants are fixture facts, not byte throughput. Selecting these must not alter another benchmark's fixture. [I4] |
| `bench:l1:gateway` / `l1 gateway`, `overhead` | `buffered-1kib-capture-metadata.c{1,64}`, `streamed-1kib-capture-metadata.c{1,64}`, `buffered-64kib-capture-metadata.c{1,64}`, `buffered-64kib-capture-payload.c{1,64}` — 8 | Fresh tenant, per-client key/connections, local upstream and chosen capture policy. Preserve 1 KiB/64 KiB buffered bodies and 32-event stream. | Requests/s and /CPU-s, CPU/request, overhead p99 (<500 µs at c64, including end handoff for streaming), completion/first-token/end timings, capture reconciliation and memory. Payload sizes are generator body sizes, not full serialized input traffic; captured/stored bytes remain storage diagnostics. [I5] |
| Gateway `--pace <requests/s>` | Any of the eight IDs, with offered rate in identity | Same isolated setup and fixed timing; use declared offered rate and existing missed/capped-arrival checks. | Same readings plus pacing evidence. A paced run is a changed-load diagnostic, not evidence for the unpaced benchmark's mandate. It can compare with exactly the same paced declaration. [I5] |

`bench:l1`, `bench:l1:bifrost`, and bare `capacity l1` / `l1 bifrost`
are selectors over the default entries, not additional workloads. Extended UI,
concurrency and model benchmarks remain explicitly selectable. `--steps` changes
only membership/order and the coverage list, never fixture/timing/eligibility.

**Existing rows that are not independent timed benchmarks:**

| Existing item | Treatment and reason |
|---|---|
| Four write `*-peak` rows | Derived maximum across the declared c1/c64 results. Print source record IDs and winning concurrency. Not another load run or an independent sample. If an input is absent, print incomplete, not the maximum of whichever subset happened to run. [I1] |
| `point-memtable-scaling`, Parquet `point-scaling` and `span-trace-scaling` | Derived c1/c64 rate scaling, preserving the declared formula. Both inputs must be compatible independent benchmark records. Missing c1 never triggers hidden execution or degrades the c64 benchmark's validity. [I1] |
| Parquet shredding, time-window pruning, `compression.otlp-{spans,logs}`, `storage_ratio.{otlp-spans,otlp-logs,native-write}` | One-time fixture checks/diagnostics, not three repeated observations. Compression is sample input protobuf bytes / table's published Parquet growth; storage ratio is Parquet growth / retained Arrow bytes and currently includes internal inserted rows. Label that scope. Keep them attached to their existing semantic owners, but recompute inside each owner's fresh setup and never copy across windows as independent measurements. [I2] |
| UI `first_request_ms`, page/statement diagnostics and standalone catalog checks | First request is a setup diagnostic; pages/statements are components of one mixed-page benchmark. Do not split them into newly invented benchmarks or imply the first request is cold-disk latency. Every fresh residence benchmark regenerates/checks its own references. [I3] |
| `--profile`, `--allow-dirty`, `--smoke` variants | Same per-benchmark isolation. Proposed L1 rule keeps 30 s + 3×30 s even for these flags; smoke changes fixture scale and judgment eligibility, not the fixed L1 clock. Profiles/dirty/smoke remain labelled and excluded from clean performance qualification. The current 1 s + 1×3 s L1 smoke timing cannot satisfy the binding timing; it must be removed from L1 or moved to separately named harness tests by a later approved decision. L2 smoke is unchanged. [D1, D2] |

No measured workload is inherently excluded by its shape. Paced Verification,
streaming Gateway, learned fixtures and multi-statement UI work fit the same
lifecycle. Full-size setup/drain feasibility is unverified, not an authorized
exception. Peak/scaling rows and setup facts cannot honestly fit a timed-window
protocol because they are calculations or checks, not independently driven work.

### 5. Repeatability and comparison

**Recommendation for approval:** a single benchmark invocation produces one
result. A qualification comparison executes **three fresh baseline/candidate
pairs** for that benchmark: three independent executions per build, six total.
Its three windows are internal diagnostics, not three independent experiments.
There is no mandatory full-suite campaign to compare one selected benchmark.

Extend existing `capacity` orchestration to record a fixed three-pair schedule
before starting. Randomize the first pair's build order and alternate thereafter
(AB, BA, AB or BA, AB, BA); record the seed and order. Pair adjacent executions
on the same host and controls. Every execution still has a separate environment.
Run only prebuilt worktrees/binaries and SDK artifacts bound to their source;
the parent never copies binaries or asks an implementor to write loops. Store
the schedule, attempted record IDs and results in a small tool-produced record
set. `compare` remains a reader, never unexpectedly launches work.

The execution entry accepts two prepared worktree/build identities and a
selection of benchmark keys. It validates their declarations before launching
either arm, then produces the two arm record sets and their shared schedule.
`capacity report <set.json>` prints every member; `capacity compare <before-set.json>
<after-set.json>` checks their shared pairing and prints the comparison. Existing
single-record reader inputs remain useful for descriptive readings. Exact names
of the mise execution task and its flags are implementation-owned; the six-run
schedule and immutable input/output identities are the proposed behavior.

Use the standard **paired Student t confidence interval of the run-level
differences** for exact-valued metrics. For three pairs, `di = candidate_i −
baseline_i`; print the mean difference and
`mean(d) ± t(0.975, 2) × sample_sd(d) / sqrt(3)`. Also print baseline/candidate
means, all six results, min–max and SD, and
`100 × (candidate_mean / baseline_mean − 1)` when the denominator is nonzero.
Give equal weight to independent executions; each execution's rate is already a
ratio of its raw totals. [R4]

- Higher is better: interval entirely above zero → **IMPROVED**; entirely below
  zero → **REGRESSED**; otherwise → **INCONCLUSIVE**. Reverse the rule for lower
  is better. A metric without a declared direction is diagnostic only.
- A single before/after pair may show raw changes, but the tool prints
  INCONCLUSIVE for qualification: independent repeat count not met.
- Candidate logs target: a separate 95% one-sample t interval for the mean of
  its three run-level `input_MiB/server_CPU-s` results. Lower bound ≥80 → **MET**;
  upper bound <80 → **MISSED**; otherwise → **INCONCLUSIVE**. A one-run point
  reading above 80 is labelled an observed reading, not target qualification.
- Preserve histogram bucket uncertainty for latency criteria. Print pooled
  percentile bounds per execution and conservative between-build bounds; do
  not insert bucket endpoints into an exact-value t test and invent precision.
  For the first implementation, give a latency direction only if all candidate
  run buckets are strictly beyond all baseline run buckets in that direction;
  otherwise INCONCLUSIVE, explicitly a descriptive range rule, not a 95% test.
  Existing strict per-run SLO judgments retain their declared bucket semantics.

Three pairs are an economical **proposal**, not a statistically guaranteed
sample size. The t interval assumes independent pairs and approximately normal
differences; with n=3 those assumptions cannot be established from the sample.
Small or noisy changes will often remain inconclusive. Show drift and raw
values, never discard outliers or automatically add runs until the interval
passes. All failed/invalid attempts remain in the set; an incomplete pair blocks
qualification rather than being silently replaced. A new set is a new recorded
decision, not a repair of the old evidence.

Before establishing the new baseline, use the same comparison mechanism for
same-build normal/reversed and standalone/subset checks after resume. Structural
tests cover all 65 registrations; physical checks prioritize logs-c64, native
c64, hot/Parquet setup, UI residences, Verification SDKs and Gateway capture.
Compare phase timings and background evidence as well as headline numbers.
Do not infer equivalence merely because a difference interval crosses zero.
The acceptable order-effect/equivalence tolerance remains a maintainer decision;
unresolved systematic effects prevent qualification. There is no proposed
arbitrary 3% improvement gate or 5% CV cutoff.

Illustrative output only, **not repository measurements**:

```text
otlp-logs-c64 | throughput | before 100 | after 120 input MiB/s
  95% paired difference interval [+12, +28] input MiB/s | IMPROVED
otlp-logs-c64 | efficiency | before 70 | after 84 input MiB/server_CPU-s
  95% paired difference interval [+8, +20] input MiB/server_CPU-s | IMPROVED
target | candidate mean interval [81, 87] input MiB/server_CPU-s
  threshold >=80 | MET
L1 mandate components | PASS
REQ-013 overall | NOT ESTABLISHED: competing-load/read/recovery evidence required
```

Plain meaning: both L1 readings improved beyond the estimated run-to-run
uncertainty, and the candidate mean clears 80 under this short-window protocol.
If the efficiency difference interval crosses zero, the combined L1 improvement
is INCONCLUSIVE even when throughput improves. If either required metric
regresses or the target is demonstrably missed, the L1 mandate fails. These are
performance decisions, separate from VALID/INVALID measurement integrity.
Per-metric intervals are not simultaneous confidence coverage for all 65
benchmarks; do not market a collection of exploratory results as a suite-wide
95% guarantee. Only explicitly declared mandate components determine its result.

### 6. One fixed tool-owned output

Extend `Record::render`, `Record::compare` and their existing CLI readers. Live
output and a regenerated report use the same persisted input, arithmetic,
sorting, units and renderer. The record must contain all evidence needed after
`target/` is deleted. Do not build another record parser or agent-generated
summary path. History HTML consumes the same calculated values and eligibility.

Both readers use the existing six-section reporting order. `report` fills
Comparison with “not supplied” when appropriate; `compare` prints both arms and
their comparison through this same format:

```text
Question
  declared question; benchmark key; intended comparison/mandate
Run
  VALID / INVALID first, reasons; protocol and record schema
  exact mise command; record/set paths; source and artifact identities
  host/storage/config/fixture digest; selected and omitted keys; schedule
  eligibility and reason; isolation and teardown evidence; phase elapsed times
Report
  each execution and each measured window, in fixed order:
  duration | successful work/rows | input bytes and source or N/A |
  server CPU-s | driver/client/provider CPU | rates and CPU/work |
  latency buckets/counts | errors/refusals/retries | late work |
  opening/peak memory | publication | backlog start/end | window validity
  benchmark aggregate from raw totals
  request-drain and final-drain time/CPU, durable reconciliation, remaining work
  setup facts once; all recorded series and diagnostics with boundary/population
Comparison
  compatibility checks and all excluded/invalid attempts
  metric [unit/direction] | every baseline/candidate run | mean [min,max; SD] |
  absolute/percent change | interval or bucket-range method | decision/reason
  derived peak/scaling with source keys; missing inputs explicitly listed
  throughput improvement | input efficiency improvement | logs >=80
Answer
  deterministic verdict sentence for the declared question/mandate
Unproven
  missing coverage, sustained delivery, downstream costs, other hosts,
  insufficient repetition, unresolved order effects, REQ-013 evidence gaps
```

Units and byte sources are printed in each relevant row. Use unrounded values
for decisions. Missing, inapplicable and zero are distinct. Lifetime gauge
high-water marks must say “since process start”; interval memory peaks must be
measured as such. Never truncate retained diagnostics to make a summary look
complete. An INVALID record still prints its evidence for diagnosis, but no
performance conclusion is calculated from it.

An eligibility check is shared by comparison, baseline lookup, history admission
classification and L2 calibration. Match benchmark identity, protocol, fixture,
load, SDK/language, host/storage, server controls, measurement boundaries and
build recipe. The code/artifact digest is intentionally different across the
before/after arms; within each arm it must be identical. Selection membership
and order are metadata, not compatibility fields. Optional workload comparisons
(UI volume/layout, languages at matched rate, Gateway capture policy) are labelled
as different treatments and never qualify an unchanged-population mandate.

### 7. Enforceable benchmark rules

| Current rule | Tool ownership in the proposal |
|---|---|
| Mise entry, declared timing/order, no ad hoc loops [D2:10–20] | Mise tasks call one supervisor which enumerates benchmarks, owns children, timing and comparison schedules. Direct entry cannot bypass those checks. Record the invocation. Do not invent an environment-variable token claiming to prove that a human typed mise. |
| Quiet host, no concurrent work [D2:21] | One cross-worktree benchmark lock plus existing portable process/host sampling; record owned processes and refuse observed competing build/test/benchmark work. Retain before/after samples and observe during measured windows, since endpoint samples alone miss overlap. Missing required observation invalidates. A lock cannot prevent arbitrary external work. |
| Same task/flags/host for comparison [D2:60] | Compare normalized benchmark controls, not literal selection lists. Reject mismatched workload, protocol, host/storage, SDK or measurement settings. Hash server/driver and required SDK artifacts and bind them to clean source/build metadata; current name/size/mtime is insufficient. |
| Correctness, floors, CPU validity, driver budget [D1:333] | Retain exact answers/custody, error and sample checks, positive/nonreset CPU and current driver ≤80% of eight cores; extend to isolation, timing and final cleanup. Preserve paced arrival checks. Performance target misses do not turn a valid measurement INVALID. |
| Always paste the standard report; no private arithmetic [D2:28–46,50–74] | `capacity` prints the whole six-section result, all retained readings, computed differences, verdicts and limitations. Golden-output tests ensure record → report regeneration. Software cannot force an agent to paste honestly, but no computation is left to it. |
| Selected steps are diagnostic [D2:72] | **Replace**, as explicitly directed: subset selection has no eligibility penalty. A partial suite cannot claim missing coverage, but each selected benchmark can qualify independently. |
| Smoke/profile/pace/dirty and history rules [D2:72,100] | Retain mode/provenance exclusions for changed instrumentation, smoke fixture and dirty source. Gateway `--pace` changes load and cannot qualify unpaced results; declared Verification r3000 is its own regular benchmark. Selection alone never excludes history. |
| One recorder; declared metric boundary/direction [D2:81–95] | Reuse `wyrd-benchmark`, `Metric`, `Population`, typed records. Require registered units/population and direction for judged metrics, and report coverage for every stored reading. Bench-only observations remain behind `bench`; no sidecar/exporter. |

“Read existing records first”, reasoning from diagnostics before profiling, no
synthetic host load, and truthful interpretation remain contributor obligations.
Do not claim the harness can enforce a person's narrative or every action on a
shared host. Thresholds and missing-evidence rules are fixed before execution,
never chosen after seeing a favorable result.

### 8. History and the new baseline

Preserve every old record and ledger unchanged. The new reader identifies them
as **legacy L1 protocol / not eligible for rebuilt qualification**, without
retroactively declaring every measurement false. Exclude **all old L1 families**,
not just writes, from new before/after qualification and L2 calibration. Old
records can still explain the original incidents within their recorded scope.

Use a new protocol identity and record schema for per-benchmark lifecycle and
comparison evidence. Recognize schema-3 history for labelled historical display
inside the existing reader; unsupported older schemas remain visibly unsupported.
This is an archival reader path, not a production compatibility API or a conversion
of old measurements. Separate history chart series at the protocol boundary.

Clean, unprofiled, non-smoke benchmark attempts go to the existing
`benchmark-records` history, including valid independent selections and retained
INVALID attempts. Ineligible diagnostic modes stay under `target/benchmarks`.
Publication/commit rules remain unchanged. A tool-produced collection/set records
coverage and references; an implementor never hand-selects successful records into
a supposedly complete result. Artifact identity and teardown completion must be
known before admitting a successful record.

After explicit authorization to resume, verify the rebuilt protocol and
same-build repeatability, then name the clean baseline commit, artifacts, host
and controls. Measure both baseline and candidate with that same rebuilt harness
contract; never compare old five-second/shared-state scores with new scores or
apply a conversion factor. An older product revision needs a reproducible way to
build the same harness and observation contract without including candidate
performance changes. Choose and record that baseline preparation before running.

L2's existing mixed-rate formula remains `0.70 × 1 / sum(fi / ri) × cores`, with
spans/logs/native shares unchanged. Replace its permissive history lookup with a
complete compatible set of the needed new independent c1/c64 benchmarks, same
server build and host. Compute each path's peak from the declared complete sweep
using run aggregates (the mean across independent executions when a repeated set
is supplied), retain all contributing IDs, and never substitute a legacy or mixed
build record. Wide spans remain outside the existing L2 mix. A subset missing a
calibration input is insufficient for **that consumer**, not intrinsically an
invalid benchmark. [I6]

### 9. Interaction with bifrost-write-throughput-and-retention

The active packet is approved revision 3. Its
[REQ-013](../../bifrost-write-throughput-and-retention/spec.md#required-behavior)
at `spec.md:191` requires 80 on unchanged logs-c64 input, whole-server CPU,
improvement beyond normal variation, separate full-matrix reporting, no material
regression elsewhere, preserved hot/Parquet reads, bounded memory, sustainable
publication, and drain/recovery under competing load calibrated to improved
capacity. It explicitly rejects short ACK-only evidence as sufficient.

This proposal preserves the 80 units and logs input population. It replaces the
measurement protocol and baseline, so the old 58.589 value in that spec and its
handwritten ledger cannot qualify the new result. This packet does not amend
that approved spec. Before affected qualification resumes, the maintainer must
approve the corresponding evidence/baseline revision in its owning packet;
no numerical target or durability/read protection is silently weakened.

The fixed output separates three questions:

1. Were the selected L1 measurements valid and repeatable?
2. Did both declared throughput and input-CPU-efficiency comparisons improve,
   and did logs-c64 clear 80? These components form the **L1 mandate result**.
3. Is REQ-013 as a whole established? L1 alone prints **NOT ESTABLISHED**.
   Matching full write-matrix, hot/Parquet and competing-load evidence is still
   required; L2 and the active packet retain ownership of that proof.

The user has not specified whether both-improve applies to logs-c64 alone or
every write step. Recommendation: make logs-c64 throughput and input efficiency
co-primary, preserve all eight write results separately and require the active
packet's no-material-regression protection elsewhere. Do not average different
payloads into a pass. If both-improve is intended for all OTLP populations,
declare each of the six comparisons explicitly before execution. Native currently
supports rows/s and rows/CPU-s, not input MiB; see the open decision below.

### 10. Smallest implementation outline and observable proof

This is sequencing for design comprehension, not a task packet.

| Existing owner | Proposed work and why reuse fits | Observable proof after approval |
|---|---|---|
| `experiment.rs`, `bin/capacity/{main,levels}.rs`, `mise.toml` | Keep the declarations/readers; make measured benchmarks the execution unit, give every ID discoverable registration, normalize selection, own six-execution comparison schedules. Reuse spawning rather than add a campaign service. | Enumerate all 65 IDs and paced variants. Normal, reversed, subset and standalone selections yield identical declarations/setup for a given key. Missing derived inputs stay missing. |
| `levels.rs`, `release_server.rs`, `scripts/postgres/with-test-postgres.sh` | Move existing process/DB/storage ownership under the benchmark deadline and parent completion record. A fresh server alone cannot reset PostgreSQL or SDK globals, hence a fresh child and database lifecycle are necessary. | Real-dependency tests run two different selected cases; distinct state/resources, no foreign rows, all children/volumes gone. Inject reused storage, server replacement, timeout and cleanup failure; expect INVALID and no following launch after failed cleanup. |
| `query/isolated.rs`, `query/ui/workload.rs`, `verification/{run,driver}.rs`, `gateway/run.rs` | Make existing preparation consume one chosen benchmark. Rebuild full fixtures and own providers/SDK children per execution. Preserve workload generators, exact-answer checks and load semantics. | All residence/layout and language paths work alone and in reversed order. Canonical Verification is unchanged by selecting model cases elsewhere. Gateway policy and durable capture evidence belong only to its own benchmark. |
| `capacity.rs`, `experiment.rs`, existing `wyrd-benchmark` recorder and benchmark snapshot owner | Retain accounting/populations; persist boundary timestamps, raw totals, late work, phase CPU, setup facts and durable reconciliation. Extend existing inspection only if necessary facts cannot be obtained from current owned rows/files. | Controlled-clock cases cover late completions, counter reset, missing CPU, invalid populations, deferred publication and unequal window work. Totals/ratios agree with independent arithmetic fixtures; no post-drain counter is labelled as stop-time evidence. |
| `Record::render/compare`, `levels.rs`, `results.html`, query calibration | One eligibility/calculation contract, paired comparison and fixed output, versioned historical display; complete-set calibration. | Golden reports reproduce after raw artifact removal; refuse mismatched host/fixture/protocol, diagnostic qualification, incomplete pairs and legacy calibration. Cases include throughput gain with efficiency loss, target interval crossing 80 and missing native input. |
| `benchmarks/README.md`, `benchmarks/AGENTS.md`, owning active spec where approved | Update terminology, timing, selected-benchmark eligibility, command descriptions, report semantics and baseline policy once approved. | Documentation and registrations agree; selection has no hidden diagnostic flag; benchmark execution remains opt-in. |

Keep tests at the existing pure arithmetic/declaration owners, real PostgreSQL
lifecycle seam and `capacity` CLI journey. No benchmark execution is part of a
unit/CI test. Later tasks select exact focused commands under
[TESTING.md:91](../../../../TESTING.md#named-tests); this design names proof
observations, not invented test selectors. Full runtime/order validation is
separate, explicitly authorized performance work after the hold is lifted.

## Evidence and rationale

### Repository evidence (inspected, not benchmarked)

Links use repository-relative paths; labels include source line numbers at the
inspected commit. Table references above refer to these entries.

- **D1 — Current benchmark contract:** [benchmarks/README.md:25](../../../../benchmarks/README.md#measurement)
  (smoke/build/runtime rules), `:103` (byte meanings), `:135` (old timing),
  `:142` (writes), `:158` (hot/Parquet), `:193` (Verification), `:299`
  (Gateway), `:333` (validity), `:362` (records), `:431` (range comparisons).
- **D2 — Contributor rules:** [benchmarks/AGENTS.md:10](../../../../benchmarks/AGENTS.md)
  (execution), `:28` (readers), `:50` (six sections), `:71` (spread), `:72`
  (selection exclusion), `:81` (one recorder), `:100` (history).
- **I1 — Bifrost registration and exact matrix:**
  [query/mod.rs:179](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/query/mod.rs#L179)
  (`ALL` and `EXTENDED`), `:306` (nine hot measurements), `:350` (eight Parquet),
  `:982` (declaration), `:1022` (scaling), `:1047` (eight writes/four peaks),
  `:1156` (old 10-second exceptions).
- **I2 — Setup facts and byte limitations:**
  [query/mod.rs:926](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/query/mod.rs#L926)
  and `:972–975` define settled facts. [query/run.rs:319](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/query/run.rs#L319)
  returns zero native wire bytes but measures OTLP encoded length;
  [isolated.rs:432](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/query/isolated.rs#L432)
  excludes late rows/bytes and labels native wire bytes missing. This is an
  instrumentation gap, not evidence of zero native traffic.
- **I3 — All UI arms and setup:**
  [query/ui/mod.rs:27](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/query/ui/mod.rs#L27)
  (residences/pages/counts), `:56` (three arms), `:131` (diagnostic meanings),
  `:161` (six steps per arm);
  [query/ui/workload.rs:124](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/query/ui/workload.rs#L124)
  currently forbids reversing residence order; `:262` learns its sample;
  `:571` loads/seeds/reconstructs reference answers.
- **I4 — All Verification cases:**
  [verification/mod.rs:56](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/verification/mod.rs#L56)
  (client counts/sweep/model IDs), `:294` (canonical fixture), `:418` (optional
  model/sweep declaration), `:453` (languages/r3000), `:534` (model fixture).
  [verification/run.rs:732](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/verification/run.rs#L732)
  and `:780` own server/provider and SDK children.
- **I5 — All Gateway cases:**
  [gateway/mod.rs:94](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/gateway/mod.rs#L94)
  (four exact case IDs and shapes), `:257` (pace and eight measured steps);
  [gateway/run.rs:641](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/gateway/run.rs#L641)
  changes capture policy in the existing server and settles after windows.
- **I6 — L2 consumer:**
  [query/mod.rs:1188](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/query/mod.rs#L1188)
  defines the mixed rate; `:1219` checks host/build recipe/fixture/limits but
  selects each path's highest median from whatever measured sweep steps exist.
  It needs the new protocol/identity/completeness check.
- **L1 — Shared lifecycle:**
  [experiment.rs:169](../../../../crates/wyrd/wyrd-testing/src/experiment.rs#L169)
  defines old timing; `:2012` prepares once, `:2051` loops steps, `:2121`
  finishes with a separate grace, `:2954` compares identity and VALID before
  testing overlapping repetition ranges. [levels.rs:385](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/levels.rs#L385)
  wraps whole experiments in fresh PostgreSQL today.
- **L2 — Isolation defect:**
  [isolated.rs:229](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/query/isolated.rs#L229)
  deliberately retains c1 backlog into c64; `:257` gates publication settlement
  on c1; `:752` prepares one server/all senders. This verifies the structural
  selection defect, not a quantitative explanation of earlier score swings.
- **L3 — Timing and ownership:**
  [capacity.rs:165](../../../../crates/wyrd/wyrd-testing/src/capacity.rs#L165)
  freezes CPU before request drain; [isolated.rs:369](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/query/isolated.rs#L369)
  collects cohort timing afterward; `:489` wrongly equates zero published files
  with zero publication cost. [release_server.rs:182](../../../../crates/wyrd/wyrd-testing/src/release_server.rs#L182)
  creates temporary storage and migrates; `:97` reserves 45-second shutdown.
  [with-test-postgres.sh:21](../../../../scripts/postgres/with-test-postgres.sh#L21)
  disposes volumes, `:54` creates/waits/retries, currently outside the experiment
  clock. [capacity.rs:1105](../../../../crates/wyrd/wyrd-testing/src/capacity.rs#L1105)
  identifies binaries only by metadata.
- **M1 — Command inventory:** [mise.toml:540](../../../../mise.toml#L540)
  builds server/driver; `:549` builds SDKs; `:574–622` declares every L1 task
  above. [main.rs:102](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/main.rs#L102)
  dispatches default versus explicitly named families. README's old three-family
  wording and shorthand Gateway example do not match those current declarations.
- **A1 — Durable boundary and telemetry authority:**
  [architecture/bifrost-design.md:707](../../../../architecture/bifrost-design.md#durability-and-visibility)
  distinguishes asynchronous recording from append durability;
  `:729` excludes publication/compaction from ACK;
  [:2037](../../../../architecture/bifrost-design.md#telemetry) reserves durable
  facts to durable records, not metrics.

### Established practice and its application

- **R1 — Explicit warmup, measurement and per-case setup.** The
  [Google Benchmark user guide](https://google.github.io/benchmark/user_guide.html#runtime-and-reporting-considerations)
  separates discarded warmup from measured repetitions and defines setup per
  thread-count case. This supports distinct phases; it does not establish that
  30 seconds proves steady state. Thirty seconds is the maintainer's decision.
- **R2 — Fresh executions.** The
  [OpenJDK JMH forking example](https://github.com/openjdk/jmh/blob/master/jmh-samples/src/main/java/org/openjdk/jmh/samples/JMHSample_12_Forking.java)
  demonstrates profile pollution when cases share a JVM and motivates separate
  forks. Its JVM-specific mechanism is not evidence about Rust; applying fresh
  processes here is an inference supported by Wyrd's observed shared lifecycle.
- **R3 — Repeat at the level where variation arises.**
  [Kalibera and Jones, Rigorous Benchmarking in Reasonable Time (2013)](https://kar.kent.ac.uk/33611/)
  distinguishes iteration, execution and build variation, recommending repetition
  where uncertainty occurs. That supports independent executions rather than
  treating three correlated windows as independent runs. It does not prescribe
  three executions or guarantee power for this workload.
- **R4 — Difference intervals.**
  [NIST, Confidence intervals for differences between means](https://www.itl.nist.gov/div898/handbook/prc/section3/prc312.htm)
  gives the paired t interval used above. The proposal uses adjacent paired
  executions to control slow host drift. An interval crossing zero is inconclusive,
  not proof of equality. The target uses the corresponding one-sample mean
  interval; [NIST's confidence-limit explanation](https://www.itl.nist.gov/div898/handbook/eda/section3/eda352.htm)
  describes its normality assumption and interpretation.
- **R5 — Order control.**
  [Google Benchmark random interleaving](https://github.com/google/benchmark/blob/main/docs/random_interleaving.md)
  provides an established mechanism to reduce effects of changing system state.
  Alternating baseline/candidate execution is the simpler proposal here; its
  effectiveness for Wyrd still needs same-build validation. Randomization does
  not repair inherited application state.

## Decisions and alternatives

Use one isolated execution per measured step and extend existing owners. Merely
fixing the c1 publication barrier would preserve shared PostgreSQL/client/cache
state and would not cover Verification, UI or Gateway. Resetting only counters
cannot establish isolation. Resetting before every 30-second window would change
the agreed benchmark into three cold starts; retain one environment across its
warmup and windows instead.

Use fixed small independent comparisons and admit uncertainty. One execution
per build is faster but supplies no execution-level variance estimate. Twenty
runs, adaptive stopping and long “until stable” warmups conflict with the desired
simple bounded procedure. Three pairs may be inconclusive; that is a result,
not permission to tune the method afterward.

Retain short ACK semantics and explicit L2 proof. Requiring publication to keep
pace inside every L1 window would invent a sustained-throughput promise that
the fixed 30-second design cannot establish for every case. Capturing deferred
work makes the limitation visible without changing the production ACK boundary.

## Standards conflicts

| Existing owner/rule | Proposed disposition |
|---|---|
| README:135 and `Level::timing`: 5-second warmup/windows and collection budgets | Replace with the binding 30 + 3×30 per-benchmark protocol and under-ten-minute lifecycle. User decision settled; detailed allowances remain proposed. |
| AGENTS:72/100: selected steps diagnostic, only full plan in history; shared-memory hot-path review preference also requires a full run | Selection exclusion is superseded by the explicit current instruction. Update owners in implementation; preserve complete coverage only where a consumer actually needs all inputs. |
| README:146 and `isolated.rs:229`: intentional cross-step state | Replace with fresh per-benchmark state. No c1 or path-order dependency survives. |
| AGENTS:71 / README:431: repetition-range overlap means “no measured change” | Propose independent-run paired uncertainty, with INCONCLUSIVE instead of equality claims. Histogram descriptive bounds remain explicitly distinguished. |
| Existing short L1 smoke timing | Cannot satisfy the uniform timing decision. Proposed reduced fixtures keep the same L1 clock; faster lifecycle checks belong to tests. Maintainer disposition still needed. |
| `architecture/bifrost-design.md:2071`: retained Arrow bytes described as ingest-throughput units, versus README:103 and REQ-013 input-byte mandate | Preserve the retained-Arrow counter meaning, but clarify that it is a storage diagnostic; input-throughput qualification uses actual OTLP protobuf bytes. This is an owner-wording correction to approve, not a server metric rename. |
| Active approved REQ-013's old baseline/repetition protocol | Requires an approved evidence/baseline revision in that packet before resumed qualification. The 80 target and all read/durability/competing-load protections remain. |

No authority file was changed by these notes. Standards updates follow approved
design/spec work, not an exception silently implemented here.

## Open questions

The lifecycle unit, fresh state, subset equivalence, 30 + 3×30 timing, per-benchmark
ten-minute limit and tool-owned reporting are settled. Remaining material decisions:

1. **Comparison cost and inference:** approve three fresh paired executions per
   build, approximate 95% paired t intervals for exact metrics, and conservative
   descriptive latency comparisons. Six executions can take nearly 59 minutes
   for one before/after benchmark comparison; each execution remains below ten
   minutes. A full suite is correspondingly longer. Choose the tolerance for
   same-build order equivalence before qualification; failing to detect a
   difference is insufficient proof.
2. **Mandate coverage and native units:** confirm which write steps must improve
   both throughput and input efficiency. Recommend logs-c64 co-primary plus full
   matrix/read protection as REQ-013 says. Native has no input-byte counter;
   recommend rows/s and rows/CPU-s for its unchanged population. If native must
   meet an input-MiB improvement mandate too, agree the serialized IPC byte
   definition and add a counter at its existing encoder (without second
   serialization), as additional design scope. Also define “material regression”
   for the active packet's other populations; INCONCLUSIVE is not non-regression.
3. **Runtime feasibility and diagnostics:** approve the proposed 590-second phase
   envelope, build prerequisites outside it, and uniform L1 timing for smoke.
   Successful full-size setup/drain remains to be demonstrated after the hold.
   A case that cannot fit returns for a decision; there is no implicit exception.
4. **Baseline/authority handoff:** choose the clean baseline build and matching
   rebuilt harness, approve the active REQ-013 evidence revision and the narrow
   byte-unit wording correction. Benchmark execution needs separate authorization
   to resume; approving these notes alone does not lift the hold.

## Handoff

This proposal covers every registered L1 family, optional measured case, derived
row and existing mode. It is ready for maintainer design discussion, **not** for
specification or implementation. After the material decisions are settled and
the maintainer agrees the design, finalize these notes; only then use `wyrd-spec`
and later `wyrd-plan`. Private struct names, file splits and helper choices remain
implementation-owned within the existing owners.

Research and static inventory are complete for this draft. Budget feasibility,
repeatability, order equivalence and the 80 mandate have **not** been measured.

## Maintainer review — 2026-11-22

Recorded verbatim against the open questions above:

1. "Too many things at once. Focus on standardizing benchmarks first before we
   talk about comparison" — comparison methodology is deferred until the
   standardized benchmark protocol is settled.
2. "I told you i want a proposal for all benchmarks/cases. Why are you talking
   about writes?" — the proposal must address every L1 benchmark and case
   uniformly, not centre write mandates.
3. "sure" — the 590-second per-benchmark phase envelope is accepted.
4. "there is no fucking baseline yet" — no baseline exists; none is to be
   chosen in this design.
