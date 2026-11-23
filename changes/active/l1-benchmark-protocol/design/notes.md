# Independent L1 benchmark protocol

Status: finalized

Repository evidence was originally inspected at
`d2f37d07b4c6e595ecdc9f59bb27cfa467be16b0`. The current proposal incorporates
the maintainer review at the end of these notes. Benchmark execution is on hold.

## Problem

The current L1 runner prepares one workload environment and reuses it across
measured steps. Earlier steps can leave data, caches, clients and background
work behind. Selecting a different set of steps can therefore change the
workload a named case actually measures. The reports also depend on
implementors assembling the same readings consistently.

## Desired outcome

Every named L1 case runs the same declared workload when selected alone, in a
subset, or in a suite. Each execution produces a self-contained result that
`capacity` renders from its saved record. A collection groups benchmarks for
selection. A benchmark is one measured operation, fixture, client count and load
combination. The time limit applies to each benchmark.

## Constraints and non-goals

### Agreed

- Every measured L1 step is its own benchmark. Start it with a fresh server,
  PostgreSQL data and processes, WAL, object storage, and client state.
- Use 30 seconds of warmup followed by three 30-second measurement windows.
  Setup, drains, evidence capture, cleanup and reporting follow the accepted
  590-second total phase envelope below.
- Selection and order change which benchmarks run. Each selected benchmark
  follows the same lifecycle and evidence rules.
- Each category command runs its agreed routine sweep of 5 Verification,
  5 Gateway, or 18 Bifrost cases. Targeted selections use the same protocol.
- `capacity` renders a deterministic category summary table for direct review
  and preserves full per-case evidence. Agents use the emitted table verbatim.

### Scope

The design covers single-execution L1 measurement, reporting and lifecycle
validation. Comparison methodology, baseline selection, performance-change
qualification and L2 calibration policy are deferred. Production
workload semantics and resource controls stay governed by their existing owners.

Fresh application state is within the harness's control. Host device caches,
temperature, Docker VM state and unrelated host activity can still vary. The
protocol establishes structural independence and records host observations.
Numerical order-equivalence criteria belong to the deferred comparison design.

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
efficiency.

Keep server latency and per-request overhead boundaries. Overhead subtracts the
union of that request's provider/Verifier waits, never differences of aggregate
percentiles. Combine histogram counts to obtain a benchmark percentile; never
average window percentiles. Keep the three window percentiles, sample counts and
bucket bounds. Existing p99/p95 floors (1,000/200 per window when judged) remain;
insufficient samples make the affected benchmark INVALID, without extending its
window. A purely diagnostic percentile below its floor says unavailable.

Judge existing case-level performance thresholds using the same aggregates the
summary shows. Use ratios of totals for rates and pool histogram counts across
the three windows for latency percentiles. Apply thresholds to the underlying
reading with its recorded precision and bucket bounds. Preserve the existing
threshold values and operators. Record the aggregation as part of the new
protocol because pooling can change judgments compared with the old median of
window results. Retain each window's readings, correctness checks and sample
floors in the full case record.

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
benchmark, as today. Print build preparation time separately and refuse to run
with missing or stale binaries. The ten-minute clock starts before PostgreSQL
creation and ends after teardown, durable result capture and rendering.

Accepted uniform maximum allowances:

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

The agreed [case audit](case-audit.md) defines the 28 routine cases. The
inventory below records current registrations, including targeted workloads.

| Existing command/family | Exact measured IDs and count | Fresh preparation for **each** ID | Result and byte source |
|---|---|---|---|
| `bench:l1:bifrost:writes` / `capacity l1 bifrost writes-parquet` | `otlp-spans-c{1,64}`, `otlp-spans-1k-c{1,64}`, `otlp-logs-c{1,64}`, `native-write-c{1,64}` — 8 | Only the chosen sender, its existing generator, batch size, production rotation and 30 s retention. Recreate 2M-row span identity population; retain existing logs/native definitions. No previous path or c1 backlog. | OTLP input MiB/s, input MiB/server CPU-s; all paths rows/s, rows/CPU-s, CPU/row, ACK latency, memory, errors, publication/backlog. OTLP actual uncompressed protobuf length. Native input bytes unavailable today; retained Arrow and Parquet never substitutes. [I1] |
| `bench:l1:bifrost:hot` / `l1 bifrost hot` | `point-memtable-c{1,64}`; `point-staged-c1`; `small-aggregate-{memtable,staged}-c1`; `span-shredded-{memtable,staged}-c1`; `span-long-tail-{memtable,staged}-c1` — 9 | Rebuild the **whole declared** 10M-event/1M-span fixture, half staged/half memtable, after the separate published 500k-span learning sample. Existing large rotation/24 h retention pins residence; check it through the run. Do not load just the half queried. | Queries/s, queries/CPU-s, CPU/query, server/client latency, exact answers; memtable point c64 p99 <5 ms. Physical scan bytes only when there is a physical scan; live decoded Arrow bytes labelled separately. No ingest input MiB rate. [I1, D1] |
| `bench:l1:bifrost:parquet` / `l1 bifrost parquet` | `point-c{1,64}`, `small-aggregate-c1`, `full-scan-c1`, `span-trace-c{1,64}`, `span-shredded-c1`, `span-long-tail-c1` — 8 | Rebuild 10M events, 1M spans, 200k-row native/log storage samples and settle; perform existing layout/pruning checks before warmup. Recreate the same fixture even for one selected query. | Queries/s, queries/CPU-s, CPU/query, latency (full scan p95), exact answers; point c64 p99 <5 ms. Executed-plan scanned rows/bytes/query are physical facts. Compression and storage ratios are setup facts, described below. [I1, I2] |
| `bench:l1:bifrost:ui` / `l1 bifrost ui-sql` | `{memtable,staged,published}-ui-c{1,8}` — 6 | Rebuild default 500-request journey and seeded gateway/verifications. Establish its own memtable reference answers, then place directly into the requested residence before warmup. No dependence on a previous residence benchmark. | Query rate/CPU cost and server latency; page/statement client latency and counts for traces, verification, logs and metrics. Frozen SQL corpus digest and exact typed answers. Physical Parquet scan bytes distinct from live examined rows/decoded-column Arrow bytes; fixture payload volume is not input throughput. [I3] |
| `bench:l1:bifrost:ui:10x` / `l1 bifrost ui-sql-10x` | Same six IDs under the distinct family — 6 | Same procedure with 5,000-request journey and declared seeding. Fresh preparation for every residence/count. | Same metrics/sources as default UI; workload size is part of identity. Record the arm and workload size with its result. [I3] |
| `bench:l1:bifrost:ui:learned` / `l1 bifrost ui-sql-learned` | Same six IDs under the distinct family — 6 | Publish its own 10,000-span learning sample and verify a learned layout, then load the default journey and place it. The learning sample is declared fixture data, not contamination. | Same UI metrics/sources, with layout fingerprint and learning evidence. Record the layout treatment with its result. [I3] |
| `bench:l1:verification` / `l1 verification`, `direct-verify` | `direct-verify-c{1,64}`, `direct-verify-python-c{1,64}`, `direct-verify-typescript-c{1,64}` — 6 | Fresh canonical Verifier (one Assert/two Agent checks), applications, local provider and only the selected SDK driver. Each Agent makes two exchanges and one `code.run`. Fresh credentials, provider ledger and SDK process per benchmark. | Verifications/s and /CPU-s, CPU/verification, server overhead and total latency, client call-return/result-ready/materialization latency, server concurrency and phases. c64 overhead p99 <500 µs. The 2 KiB context is a fixture size, not measured wire/input MiB. [I4] |
| Same Verification command, declared paced cases | `direct-verify-r3000`, `direct-verify-python-r3000`, `direct-verify-typescript-r3000` — 3 | Same fresh fixture; 64 clients offer 3,000/s with the existing scheduled-arrival rule in every window. | Same readings plus offered/sent/missed/late arrivals; misses or start-lateness p99 >5 ms invalidate. Preserve scheduled-arrival evidence; no c64 overhead target on these rows. These **fit** the lifecycle: standardized timing does not require closed-loop load. [I4, D1] |
| Same Verification command, explicit `--steps` concurrency choices | `direct-verify-c4`, `direct-verify-c16`, `direct-verify-c32` — 3 | Same canonical Rust fixture, recreated for chosen count. Register these as explicit available benchmarks, without selector-dependent workload construction. | Same Verification metrics, no invented c64 target. [I4] |
| Same Verification command, explicit model choices | `direct-verify-model-c{1,64}` — 2 | Fresh Rust Agent-only model fixture: one iteration through local Gateway to immediate OpenAI-compatible provider with fixed usage and cited verdict. Do not use the canonical two-Agent fixture. | Verification/overhead/client/CPU readings, model exchange accounting; no remote-provider/TLS/retry/fallback/stream/tool performance claim. Context/token constants are fixture facts, not byte throughput. Selecting these must not alter another benchmark's fixture. [I4] |
| `bench:l1:gateway` / `l1 gateway`, `overhead` | `buffered-1kib-capture-metadata.c{1,64}`, `streamed-1kib-capture-metadata.c{1,64}`, `buffered-64kib-capture-metadata.c{1,64}`, `buffered-64kib-capture-payload.c{1,64}` — 8 | Fresh tenant, per-client key/connections, local upstream and chosen capture policy. Preserve 1 KiB/64 KiB buffered bodies and 32-event stream. | Requests/s and /CPU-s, CPU/request, overhead p99 (<500 µs at c64, including end handoff for streaming), completion/first-token/end timings, capture reconciliation and memory. Payload sizes are generator body sizes, not full serialized input traffic; captured/stored bytes remain storage diagnostics. [I5] |
| Gateway `--pace <requests/s>` | Any of the eight IDs, with offered rate in identity | Same isolated setup and fixed timing; use declared offered rate and existing missed/capped-arrival checks. | Same readings plus pacing evidence. The report identifies the actual offered rate and labels the run as paced. [I5] |

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
| `--profile` and `--allow-dirty` variants | Apply per-case isolation and the fixed L1 clock. Label the execution mode in the record and summary. [D1, D2] |

No measured workload is inherently excluded by its shape. Paced Verification,
streaming Gateway, learned fixtures and multi-statement UI work fit the same
lifecycle. Full-size setup/drain feasibility is unverified, not an authorized
exception. Peak/scaling rows and setup facts cannot honestly fit a timed-window
protocol because they are calculations or checks, not independently driven work.

### 5. Category sweeps and deterministic summaries

| Mise command | Default selection |
|---|---|
| `mise run benchmark:l1:verification` | All 5 routine Verification cases |
| `mise run benchmark:l1:gateway` | All 5 routine Gateway cases |
| `mise run benchmark:l1:bifrost` | All 18 routine Bifrost cases |

Use the `benchmark:l1:<category>` public spelling for the redesign. The
current tasks use `bench:l1:<category>`; Bifrost also has separate family
tasks. Migrate the L1 task definitions and their live callers and documentation
together so each category has one public execution command. Use the
`verify:bifrost` convention of a discoverable scoped task with one default
selection. L1 runs are opt-in.

Use the same invocation rules for all three tasks:

- No arguments runs the agreed routine sweep.
- `-- --cases <id>[,<id>...]` runs exactly the requested cases. Use **case**
  for the independently executed workload, **sweep** for its category
  invocation, **phase** for lifecycle work, and **window** for a timed sample.
  The redesigned L1 selector replaces the current `--steps` flag.
- `-- --profile cpu|heap|both` uses the existing profiler options. It applies
  to the routine sweep or combines with `--cases` for targeted profiling.
- `-- --list-cases` shows the exact selectors and their routine or
  targeted status without building or executing. Generate the list, the
  default suite and argument validation from the same registry. List
  family-qualified selectors when IDs recur across Bifrost variants.
  Refuse unknown or ambiguous selectors before starting a case.

For example, a targeted CPU profile uses the category task:

```bash
mise run benchmark:l1:verification -- --cases direct-verify-c64 --profile cpu
```

Execution prepares the required build artifacts once, then the supervisor
runs the resolved cases sequentially, completing cleanup before starting the
next fresh environment. Each attempted case gets a full record, with profile
references and capture notes when requested. Update the benchmark README and
contributor instructions with these exact commands. Agents and developers use
the documented mise tasks for execution and the existing `capacity report`
reader for saved records.

Today the harness writes a full schema-3 JSON record per experiment, with
per-step and per-window evidence. With isolation the record boundary moves to
one benchmark execution. Keep the existing full report reader and profiling
mechanisms. A sweep record lists each selected case in declared order, its
execution outcome, and its individual record reference. It also carries the
invocation and sweep mode. Preserve the referenced records with the sweep so
the summary can be regenerated after temporary profiling artifacts are removed.

**Default sweep output.** End the command with one compact Markdown table. The
same renderer regenerates that summary from a saved sweep through the existing
`capacity report` command. Individual records still open the full report.
Print coverage, mode, and the saved sweep path with the table. Agents paste
the emitted table verbatim and put interpretation in prose below it.

**Table fields.** The fixed columns are Case, Status, Throughput, CPU
per unit, Headline latency, Target, and Record. Bifrost OTLP rows also show
input MiB/s and input MiB/server CPU-second in a fixed Input column. Use the
registered case's unit and latency label explicitly:

| Workload | Throughput and CPU unit | Headline latency |
|---|---|---|
| Verification | Verifications/s and server CPU per verification | Server overhead p99 |
| Gateway | Requests/s and server CPU per request | Added Gateway overhead p99 |
| Bifrost writes | Acknowledged rows/s and server CPU per row | Durable ACK p99 |
| Bifrost point and trace reads | Queries/s and server CPU per query | Server query p99 |
| Bifrost full scan | Queries/s and server CPU per query | Server query p95 |
| Bifrost UI SQL | Pages/s and server CPU per page where accounted | Overall client page p99, with per-page-family readings in the full report |

For UI SQL, expose only a CPU/page reading that uses matching server CPU and
completed-page boundaries. Report unavailable where the record lacks a
supported reading. Preserve percentile bucket bounds and use fixed precision
and explicit units. Derive targets, statuses and displayed aggregates from
recorded evidence through the existing report calculations. Show unavailable
and inapplicable separately from zero. Render invalid attempts with the reason
and record reference, and render selected cases that could not start as
`NOT RUN` with the reason. Withhold performance cells for both. Continue
after an invalid case if cleanup succeeded. Stop and mark remaining cases
unstarted if cleanup cannot be confirmed. A threshold miss remains distinct
from an invalid measurement. Profiled sweeps are visibly marked as diagnostic.

### 6. Full per-case report

For each execution, render:

- The benchmark key, command, protocol version, fixture and load declaration,
  build and host identity, and mode.
- VALID or INVALID with reasons, phase timings, isolation and teardown evidence.
  A missed performance threshold is a separate judgment from measurement validity.
- The three windows and their aggregate, with successful work and elapsed time,
  whole-server CPU, rates and latency population, and registered criteria.
  Show units and distinguish zero, unavailable and inapplicable readings.
- Setup facts once, plus drain and reconciliation evidence, errors and late
  work, and all retained diagnostics with their boundary and population.
- Derived rows with their source record IDs. If a required input was omitted or
  invalid, show the missing input and keep each measured result's own verdict.

Persist raw inputs for the report, so it can be regenerated without temporary
logs, profiles or live services. Retain invalid-attempt evidence for diagnosis.
Version the new L1 records and preserve earlier records with their original
protocol identity. Existing comparison and L2 readers must keep their current
interpretation of existing records. New records must not silently enter readers
that assume the old protocol.

### 7. Implementation flow and proof

| Existing owner | Proposed work | Observable proof |
|---|---|---|
| `experiment.rs`, `bin/capacity/{main,levels}.rs`, `mise.toml` | Resolve the selection into complete benchmark declarations before starting children. The supervisor owns phase deadlines and result capture. Use the same declaration and lifecycle for a given key regardless of how it was selected. | Category commands resolve exactly 5, 5 and 18 agreed routine cases. Targeted workloads remain discoverable. Standalone, subset and reversed selection produce the same per-key declarations. Missing derived inputs stay missing. |
| `levels.rs`, `release_server.rs`, `scripts/postgres/with-test-postgres.sh` | Use the existing process and storage owners for each child. Record resource ownership and cleanup before issuing the final verdict. Reuse portable quiet-host checks and serialize cooperating runs across worktrees. | Real lifecycle tests verify distinct state and removal of owned children, volumes and storage. Server replacement, timeout, missing evidence and incomplete teardown yield INVALID attempts. Failed cleanup prevents the next launch. |
| Bifrost, Verification and Gateway workload owners | Prepare the full declared fixture and only the selected case's clients, workload and dependencies. Keep existing correctness, custody, load and sample checks. | Setup and exact-answer checks cover UI placement and learning, SDK/provider isolation, Gateway capture, and write and read fixtures. Selecting another case cannot mutate a declaration. |
| `capacity.rs`, `experiment.rs`, `wyrd-benchmark` and `Record::render` | Reuse accounting, metric populations and typed records. Persist stop-time inputs and separate post-stop evidence. Calculate the report at the existing reader. | Controlled-clock and arithmetic tests cover late completions, counter resets, missing CPU, unequal window totals and unavailable metrics. CLI tests regenerate the same table from a saved sweep and check row order, profiled mode, invalid attempts and unstarted cases. Individual records still produce the full report. |
| `benchmarks/README.md`, `benchmarks/AGENTS.md` | Document the new case, selection, timing and report contract after design approval. Encode executable requirements in the harness. | Controls and labels agree across registrations, commands and reports. The tool reports coverage without treating a selection as a diagnostic mode. |

**Harness smoke.** Move the short L1 workflow check into the harness test
lane. Use small fixtures and short windows, exercising the production sweep
path through selection, service startup, workload execution, drain,
reconciliation, cleanup, record capture and summary regeneration. Run at
least two cases in sequence and verify isolation across the boundary.
Pass or fail on workflow and evidence integrity. The L1 performance commands
use the fixed warmup and measurement clock.

Full-size wall-clock feasibility requires opt-in runs on supported benchmark
hosts after the execution hold is lifted. Structural tests and lifecycle
integration tests can establish the harness contract before those runs.

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
- **L1 — Shared lifecycle:**
  [experiment.rs:169](../../../../crates/wyrd/wyrd-testing/src/experiment.rs#L169)
  defines old timing; `:2012` prepares once, `:2051` loops steps, `:2121`
  finishes with a separate grace. [levels.rs:385](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/levels.rs#L385)
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
## Decisions and alternatives

Reuse the existing benchmark runner, recorder and reader. Resetting counters or
fixing an individual publication barrier leaves shared state in place. A fresh
server alone leaves PostgreSQL and other clients in place. Give each case its
own full environment and retain it through that case's warmup and three windows.

## Standards conflicts

| Existing rule | Design disposition |
|---|---|
| Shared environment, 5-second defaults and collection budgets in the benchmark README and runner | Replace with the agreed per-case lifecycle, fixed windows and accepted phase envelope. |
| Selected steps are diagnostic in `benchmarks/AGENTS.md` | Replace for L1. Selection identifies coverage. Mode and workload declaration carry their own labels. |
| Existing short L1 smoke timing | Move the short workflow check to the harness test lane. |
| Existing report format includes comparison | Use a self-contained single-execution report for the standardized protocol. Existing comparison methodology is deferred. |

## Handoff

The maintainer agreed to short smoke checks in the harness test lane and to
judging existing thresholds on the same aggregates displayed in the summary.
Full-size timing feasibility remains unverified and requires opt-in runs after
the execution hold is lifted. The finalized design feeds the [draft specification](../spec.md).

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
