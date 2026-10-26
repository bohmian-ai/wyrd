---
id: SPEC-benchmark-experiments
revision: 15
status: approved
---

# Benchmarks as experiments

Revision 15 decouples smoke from full runs. A full command runs only its full
experiment; smoke runs only when asked, through `--smoke` on the same command
or `mise run bench:smoke` for every default plan. A smoke does not judge driver
saturation, which affects measured latency, not whether the workflow runs. The
maintainer directed this on 2026-10-26.

Revision 14 fixes how Verification stores its records. Every execution has
one input record in `bifrost.verification.execution_inputs`, every result one
detailed output record in `bifrost.verification.execution_outputs`, and one
payload-free summary in `bifrost.verification.results`. PostgreSQL coordinates
work and stores no input values. Every API that returns a result assembles the
same public result from those records. Stored values are encoded to Variant
once, from the typed value. The maintainer approved this on 2026-10-26.

Revision 12 removes the sampled-client `traceparent` driver option
(`--client-trace-ratio`, formerly REQ-022) and its tenant-recording comparison.
The maintainer directed and approved the removal on 2026-10-26.

Revision 11 applies five maintainer decisions from TASK-005's real runs: drop
`writes-hot`, make each L1 experiment command finish within 5 minutes, bring
the engine optimizations needed to meet the unchanged limits into this change,
calibrate Bifrost L2 under contention, and add a sampled-client `traceparent`
driver option. The maintainer approved it on 2026-10-26.

Revision 10 records the maintainer's explicit correction and approval: smoke
checks that the actual benchmark collects, reconciles, saves and reports its
measurements. Dropped calls, server errors, capture shedding, wrong returned
results and missed performance targets are recorded production benchmark
outcomes; they do not fail smoke. Broken measurement or evidence plumbing does.
Judging is verified separately with controlled results. Revision 9's reduced
real-path workflow and revision 8's approved measurement decisions remain.

L1 is the manual regression suite engineers run whenever they change Bifrost,
Verification or Gateway. It measures server latency, CPU cost and throughput per
available core. L2 retains sustained contention, overload and recovery coverage.
Tracing remains available to explain individual requests; CPU profiling explains
expensive functions. Neither is required to produce benchmark latency samples.

Before: complete operator trace export was required to judge every request, so
export loss could invalidate a long benchmark and tracing cost could change its
CPU baseline. After: bounded server-side high-resolution histograms measure all
operations, while sampled operator traces and tenant-visible traces retain their
separate production purposes.

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

Level 1 gives opt-in feedback within 5 minutes per experiment command.
Bifrost has separately runnable hot reads, Parquet reads and
Parquet-configured writes; each has its own budget. Level 2 measures
mixed traffic and background work together over 20–30 minutes per engine.
The purpose is to measure server throughput and server overhead. Report both
throughput per provisioned vCPU and work per server CPU-second used, with
explicit denominators. Absolute latency objectives measure server handling time.

The costs are three measured repetitions per step, explicit validity checking,
portable resource accounting, and growing committed summary history. High-resolution
histograms have bounded memory and recording cost; distributions and optional
trace/profile artifacts remain outside Git in `target/`. Collection stays bounded
and overflow invalidates the affected measurement. Normalized metrics do not remove differences in CPU architecture,
storage, caches, or fixture shape. This is performance evidence, not a promise
of equal capacity on every machine.

Every experiment at both levels must run unchanged on macOS arm64, Linux
x86_64, and Linux arm64. Linux x86_64 is the only required evidence platform;
real runs on macOS arm64 and Linux arm64 are optional.
Servers run as plain child processes without OS resource limits; only Wyrd
configuration supplies server-side limits. Optional profiling uses samply
through the same `--profile` flag on every platform.

Keeping separate harnesses would preserve inconsistent judgments. Importing old
reports as new records would invent missing declarations and repetitions.
Reporting only one CPU normalization would hide either provisioned capacity or
actual CPU cost. These alternatives are rejected.

## Scope and ownership

This packet owns benchmark structure, recording, measurement definitions,
criteria, and command levels across `bifrost`, `verification`, and `gateway`,
and the shared production tracing configuration used by these engines.
Capacity coverage is kept and rebuilt, not deleted. The engine optimizations
named in REQ-021 are part of this change; other engine optimization and
ClickBench reproduction are not. Repository architecture
remains authoritative.

“Experiment” here means a repository performance experiment, not a new Card
kind or a requirement to register an Experiment Card. There is no new server,
SDK, MCP, or UI product workflow. A benchmark-only, read-only histogram
snapshot on the existing metrics listener is the sole added measurement surface;
its exposure is constrained below. Existing client trace APIs are preserved. The public contributor surfaces are
`mise` commands, documentation, JSON records, and baseline configuration.

Non-goals: optimize an engine beyond REQ-021; relax a declared limit to make
it pass; change durability, authorization, or admission;
add benchmark CI wiring or any gate dependency;
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
  declared before execution and stored. REQ-008 defines smoke's sole sample-floor
  applicability exception; other validity checks are not bypassed.
  Sample host CPU for 10 seconds before the run and again after it. Either
  sample showing non-benchmark processes using more than 10% of logical CPUs
  invalidates the run. Driver use above 80% of its declared CPU budget is
  INVALID. Open-loop traffic is INVALID on any missed arrival or send-lateness
  p99 above 5 ms (Gateway's existing `LATENESS_LIMIT_US`). Each repetition needs
  ≥1,000 samples for each p99 criterion and ≥200 for each p95 criterion;
  fewer is INVALID. A judged slow-step percentile, including L2 heavy scans,
  uses p95 with ≥200 samples per repetition; there is no heavy-scan p99 exception.
  Send lateness is solely a driver-validity check, never a server latency SLO.
- **REQ-003 — Repetitions.** Every full-run step has a discarded warmup followed by three
  measured repetitions with the same controls. L1 discards 5 seconds of warmup
  then measures 3 × 5 seconds, or 3 × 10 seconds for a case whose 5-second
  window cannot meet its sample floor (REQ-008); L2 discards 2 minutes of warmup then measures
  3 × 6 minutes. Preserve each repetition's
  metrics, median, minimum, maximum, and sample counts; never merge the samples
  into a percentile and call it the median repetition. Warmup contributes no
  measured work, CPU, latency, or sample count. A missing/invalid repetition
  invalidates the step; do not silently select the fastest repetition.
- **REQ-004 — Judgment.** In full mode, every measured step and every applicable criterion
  yields PASS, FAIL, or INVALID. PASS means valid evidence meets its criteria;
  FAIL means valid evidence misses at least one criterion; INVALID means the
  experiment did not measure what it claims. An invalid step invalidates the
  run, even if another valid step fails; otherwise any failed step fails the
  run. Exit codes are PASS=0, FAIL=1, INVALID/discarded=2. An aggregate L1
  invocation uses the same precedence across engines and preserves their
  individual records. Invalidity reasons and any valid failures remain visible.
  REQ-008 defines smoke's plumbing-only verdict and inapplicable production
  criteria; separate controlled tests prove normal judgment and propagation.
- **REQ-005 — Complete steps.** No sustained-only gate or report-only stress
  step remains. Overload steps declare expected rejections, queue/capture loss
  accounting, recovery, and bounds in advance; a successfully measured overload
  is not inherently INVALID or FAIL. Accidental request errors, wrong results,
  or undeclared loss in a valid full run fail correctness criteria. Expected server
  admission rejection differs from a driver that failed to issue arrivals.

A full step's numeric criteria use the median repetition metric unless explicitly
declared otherwise; correctness and integrity criteria must hold in every
full-run repetition. Smoke enforces measurement/evidence validity under REQ-008,
without judging the measured production outcomes against these criteria.
Repetition spread never excuses an absolute SLO miss. Conditions
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
which processes are included and the window boundaries. Any local diagnostic trace
collector is a declared helper, separate from server and driver CPU accounting;
quiet-host checks exclude its known PID. Diagnostic export loss is reported but
cannot invalidate independent, complete histogram measurements. Counter reset, process
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

Record `P`, the provisioned vCPU count: logical CPUs available to the declared
server processes, independent of utilization. On a shared host count the same
available logical CPUs once, not once per worker/replica; across distinct server
hosts sum their available counts. Record the per-host counts and process scope.
Do not substitute CPU quotas, driver cores or a load-setting budget for P.
Missing/zero P or a changed CPU availability during a repetition invalidates
its required provisioned-throughput measurement.

Let `T` be measured wall-seconds, `C` server user+system CPU-seconds used,
`W` successful work, `B` measured bytes, and `Q_N` completed queries/second at N clients:

| Metric kind | Definition and units |
|---|---|
| Throughput per provisioned vCPU | `(W/T)/P`, labelled work/s/vCPU. P is recorded logical CPUs available to the server, not mean used cores. |
| Efficiency per server CPU-second used | `W/C`, labelled work/CPU-s; mean used cores = `C/T`. This is distinct from provisioned-throughput. |
| CPU per operation | `C/W`, converted to microseconds or milliseconds as named. |
| Write byte throughput / efficiency | Uncompressed `(B/2^20/T)/P` MiB/s/vCPU and `(B/2^20)/C` MiB/CPU-s; also rows/s/vCPU and rows/CPU-s. MB=10^6 bytes, MiB=2^20 bytes, GB=10^9 bytes. |
| SLO | Absolute percentile for the declared timing boundary and client count; the fixed L1 objectives use p99 at 64 clients. No CPU normalization. |
| Scaling | `Q_N / (N × Q_1)` for the same fixture, controls, and query class; report N=64 and preserve both inputs. |
| Storage ratio | Published stored bytes / ingested uncompressed bytes for the same data after declared publication/compaction settlement; lower is better. State excluded WAL/staging/catalog bytes separately. |
| Memory intensity | Peak server resident bytes / achieved uncompressed MB/s. Store sampling interval and scope; a sampled peak is not an exact OS high-water mark. |

**REQ-007 — Engine metrics.** Each engine records the following applicable
metrics; an inapplicable metric carries an explicit reason rather than zero.
Every judged request latency is measured by an owning server monotonic timer,
then recorded into a bounded HDR histogram before aggregation. Start at server
handler entry and end at complete result readiness, durable acknowledgement or
the applicable response handoff. Include queueing and work after handler entry;
exclude client scheduling, inbound transport, SDK work and polling. Query timing
ends at complete result/terminal handoff, not stream opening or Oracle dispatch.

Use integer nanoseconds and three significant digits, with declared range and
quantization bounds. Record count, p50/p95/p99, maximum, clock source/effective
resolution, population, outcome, histogram parameters and collection method.
Keep the full interval distribution under target/ for percentile regeneration.
Nanosecond representation is not a claim of nanosecond clock accuracy. A value
outside the configured range, a failed recording, overflow, counter reset or an
unreconciled population makes the affected measurement INVALID; never clamp.
Zero-duration observations must remain represented and counted explicitly.

The server timing owner extends the already installed HDR capability, not the
coarse production Prometheus buckets. Do not install a second global metrics
recorder or introduce a production dependency on the test-tooling crate. The
existing Prometheus recorder and operational metric meanings remain intact.

The existing dedicated metrics listener gains an opt-in read-only snapshot for
benchmark-owned server processes. It exposes only bounded distributions and
aggregate counters with closed operation/query-class/repetition/phase/outcome
dimensions, process
identity and monotonic snapshot times. It accepts no reset, arbitrary query,
request identity, SQL, payload or tenant selection. Enabling it requires a
loopback-only metrics bind; a non-loopback bind is rejected at startup. It is
disabled in ordinary deployments. This is an internal benchmark readout, not a
new SDK, public data-query API, or authentication bypass on application routes.
Reuse the existing metrics server/client lifecycle; do not build a collector
service or a second server launcher for histogram retrieval.

For mixed-load attribution, benchmark-mode clients attach the closed case and
phase using `x-wyrd-benchmark-case` and `x-wyrd-benchmark-phase` HTTP/gRPC metadata.
The first is an enumerated case/query class from this matrix; the second is a
warmup or repetition/steady-capacity-overload-recovery classification, never an
arbitrary caller string. The server reads these labels where each request
arrives, onto the request context the timing owner reads. They confer no
authority and cannot select tenant, storage or sampling. Ordinary deployments
ignore them; enabled benchmark mode rejects unknown classifications with a
counted invalid-measurement reason. Internal incidental/probe operations use a
separate closed classification and cannot satisfy offered workload counts.
Series are prebounded by the declared matrix, not by request identities. L2
retains each query class and phase independently even while operations overlap.

Read cumulative bucket counts before and after a declared interval and derive
the interval distribution, never subtract percentile summaries. Snapshot
consistency, process identity, bounds and expected/completed counts must be
checked. Drain warmup before the starting snapshot. Retain completions during
measurement separately from completions after admission stops: same-window
throughput/CPU excludes drain completions, while the measured request cohort's
latency distribution includes their final outcomes. Record both populations,
CPU windows, drain work and drain duration; incomplete cohorts are INVALID.
The snapshot mechanism must prove concurrent recording does not lose or count
an observation twice. Stage timings explain latency; they are not summed when
overlapping. An unavailable optional phase is labelled, never fabricated.

Strict absolute limits use histogram quantization bounds: PASS only when the
entire percentile value interval satisfies the comparison, FAIL when it wholly
violates it, and INVALID with a precision reason when it straddles the limit.
Percentile rank and value bounds are recorded. Do not silently round a value
onto the passing side. The bounds apply to approximation error, not statistical
confidence. Each outcome is counted; failures and timeouts retain their duration
distributions and cannot disappear to improve successful-request percentiles.

| Engine / operation | Required metrics and fixed SLO |
|---|---|
| Bifrost write | rows/s/vCPU and rows/CPU-s; uncompressed MiB/s/vCPU and MiB/CPU-s; OTLP wire MiB/s/vCPU and MiB/CPU-s for OTLP; CPU µs/row; stored/ingested bytes; peak memory per ingested MB/s; server write-ack latency. Client write-to-query visibility is diagnostic-only. |
| Bifrost query | Server point-lookup p99 **<5 ms at 64 clients**; queries/s/vCPU and queries/CPU-s; CPU ms/query; executed-plan scanned rows/bytes totals and per-query distributions alongside the matching case’s server query distribution; 1→64 scaling by query class. |
| Verification | Server overhead p99 **<500 µs at 64 clients**; verifications/s/vCPU and verifications/CPU-s; CPU µs/verification. |
| Gateway | Added overhead p99 **<500 µs at 64 clients**; requests/s/vCPU and requests/CPU-s; CPU µs/request; capture bytes/s/vCPU and bytes/CPU-s. |

Uncompressed Bifrost ingest counts retained Arrow bytes inserted, consistent
with the Scribe telemetry authority; successful work excludes retry duplicates.
OTLP wire counts encoded protobuf payload bytes sent, before transport framing,
with compression settings declared. Scanned bytes are executed-plan scan bytes,
not logical dataset size. Stored bytes count published table data, not input
wire bytes; do not label its inverse as the same ratio.

Client round trip, first token received, stream completion, visibility polling
and inter-chunk lag may be recorded only as clearly labelled diagnostics; they
never select a latency verdict or regression baseline. Visibility diagnostics
retain write acknowledgment, first correct query result and probe cadence.
Retain scheduled/actual send times only to check driver send lateness and missed
arrivals. Remove scheduled-time-to-completion latency judgments and supporting
collection used only for those judgments. Server backlog/drain/recovery bounds
below remain lifecycle checks against server-side state and phase boundaries;
they are not client-observed request latency SLOs.

Benchmark latency does not depend on operator tracing, its endpoint, sampler,
export queue or a client trace flag. Default runs use the production operator
sampling policy when an operator endpoint is declared; they do not force 100%.
The manifest records endpoint presence (not private addresses), sampling and
filter policies, tenant capture state and profiler state. All actual server
instrumentation, histogram recording and export cost remains in measured CPU.
No estimated telemetry cost is subtracted.

Verification overhead is each request's handler-entry-to-result-staging elapsed
time minus the union of that request's Verifier-block intervals. Gateway
records buffered-result, streaming-head and streaming-final-frame durations,
minus provider exchange waits through the corresponding boundary. Retries and
overlap are included in the wait union once. Record elapsed, excluded and
residual histograms separately. The streaming consumer continuously drains;
server response handoff is not client receipt. These residuals are elapsed
server overhead, not CPU time.

Compute each residual before recording it: never subtract aggregated percentiles
or cross-process clocks. Negative/inconsistent residuals or missing required
components are INVALID, not zero. Existing timer calculations stay with their
engine owners. Async span busy time and ordinary span timestamps are not a
substitute. Preserve counters for expected timing components and outcomes so
histogram counts can be reconciled without retaining per-request records.
Mixed-workload efficiency retains total server CPU and work by class; it does
not pretend to attribute isolated CPU to each class. Retire `wyrd.timing`
span emission/collection once no consumer needs it, preserving ordinary behavior
and diagnostic spans and their required client-visible catalog.

### Levels and commands

**REQ-008 — Level 1, fast.** Every L1 experiment command completes in at most
5 minutes: Verification `direct-verify`, Gateway `overhead`, and each of the
three Bifrost experiments `writes-parquet`, `hot` and `parquet`. The Bifrost
aggregate runs three such experiments and therefore has a 15-minute upper
bound. L1 measures 1 and 64 clients only on every engine. Measured windows are
3 × 5 seconds after a 5-second warmup; a case keeps 3 × 10 seconds only when
5 seconds cannot meet its declared sample floor, and the record names which
cases did. The 18 Bifrost cases need about 6 minutes of warmup/repetition
windows in total, split across their three commands. Individual cases
and experiments remain available for fast optimization feedback. Build binaries and prepare external prerequisites before
starting the experiment clock; the clock includes quiet-host checking, server
startup, fixture loading, warmups, repetitions, and drain. An overrun is stopped
and recorded INVALID. Artifact finalization after stopping must still retain
the overrun and actual finish time. No compilation during execution.

Use closed-loop clients (a client sends its next operation after completion),
one fixed fixture configuration per experiment, and isolated operations:

| Engine | Required L1 coverage |
|---|---|
| Bifrost `writes-parquet` | Native Arrow, OTLP spans and OTLP logs at 1 and 64 clients: 6 measured cases. Reuse the 2M-row generators and fresh identities. The server uses production default rotation sizes and a 30 s retention; no write experiment overrides rotation. Measure durable acknowledgement, not publication. Include actual staging/publication CPU in Parquet-configured windows, record its activity and backlog, and do not claim storage-maintenance cost if none occurred. |
| Bifrost `hot` | Reads while every row stays in the memtable: the server runs with memtable and on-disk rotation sizes of 16,384 MB and a 24-hour retention, the earlier hot tier's declared settings, so nothing stages or publishes; a staged or published row makes the experiment INVALID. Fixtures: the standard 10M-row events fixture and 1M telemetry spans. Events point lookup at 1 and 64 clients; events small aggregate at 1; span filter on a shredded attribute key at 1. |
| Bifrost `parquet` | Reads after every row is published as Parquet: default rotation sizes and a declared 30 s retention, then one settlement until nothing remains in a memtable, staging or claim. Fixtures: the standard 10M-row events fixture, 1M telemetry spans, and one 200,000-row sample per write path, all settled together. This record holds each write path's storage ratio; both write records report it as inapplicable with that reason. Events point lookup at 1 and 64 clients; events small aggregate and full scan at 1; span lookup by `trace_id`, a declared Bloom column, at 1 and 64; span filters on a shredded attribute key and on a long-tail key below the shredding threshold at 1. Two correctness checks run once after settlement, outside every measured window: the published span files shred exactly the expected attribute keys and none below the threshold, and a span-time window scans fewer files than the whole table. |
| Verification `direct-verify` | Two direct-verify cases at 1 and 64 clients: existing four assertion checks over 2 KiB JSON, alternating expected pass/fail judgments. Expected failing judgments are correct completed verifications, not transport errors. Record elapsed, Verifier wait and server overhead, CPU/verification and scaling. |
| Gateway `overhead` | Six cases: buffered chat with metadata capture, streamed chat with metadata capture, and buffered chat with payload capture, each at 1 and 64 clients. Reuse the local controlled provider, 1 KiB metadata shapes/32 stream events and 64 KiB request/response payload-capture shape. Record header/final overhead where applicable, calls/s, CPU/call, capture bytes and memory. |

L1 measures 1 and 64 clients only: 1 gives unloaded latency and the
cost floor, and 64 is where the SLO applies. Scaling is reported for each query
class measured at both; a class measured at 1 only records scaling as
inapplicable with that reason. The absolute Bifrost L1 latency criterion remains point-lookup p99 at 64
clients. Other cases retain their distributions, correctness and relative
regression criteria when a compatible baseline exists. The L1 full scan keeps
latency percentiles descriptive and judges throughput/core, CPU/query and
correctness; it does not pretend a short single-client scan window supplies
200 tail samples. Non-scan L1 latency regressions judge p99 with the existing
1,000-sample floor. L2 scans retain their judged p95/200-sample requirement. The same case at different
storage states is never averaged into one result. A complete Bifrost L1 suite
is all three records at full default coverage: 12 reads and 6 writes. The
16,384 MB rotation/24-hour retention override exists only for the `hot` read
experiment, whose bounded fixture is loaded once and never grows; an override
that lets an unbounded write workload accumulate in memory is not allowed. The telemetry span fixture is deterministic, with attribute keys
straddling the 10% shredding threshold. The former request-only release preflight
is replaced by the following real-path smoke contract.
A settled Parquet read without required scan facts remains INVALID.

**Smoke mode.** `--smoke` on the normal L1/L2 commands runs the same shared
experiment workflow, engine plans and selected steps as a full run. Without
`--steps`, every declared step runs. Only declared fixture scale, warmup duration,
repetition count and measured/phase durations shrink: one short discarded warmup
and one short measured repetition, with windows measured in seconds. Publish
the actual reduced controls and a bounded smoke budget before execution.
Do not silently omit a case, transport, query class or capacity/overload/recovery
phase to make smoke pass. Preserve concurrency and physical fixture properties
needed to exercise each declared path, including hot versus published Parquet,
shredded versus residual attributes, scans, capture and background work.

The normal declaration validation, real fixture loader and publication waits,
step loop, warmup, measurement, stop/freeze, bounded drain, cohort reconciliation,
judgment, record and timing-evidence writes, and report generation all execute.
Use the same production authentication, tenant, durability and telemetry paths.
Delete the handwritten per-engine request-only smoke workflows and separate
capacity smoke subcommand; retain `mise run bench:smoke` as an aggregate of the
normal commands in smoke mode. Smoke enforces measurement/evidence validity:
required CPU/work readings, exact labelled outcome populations, required counters,
Scribe/capture accounting and applicable scan facts. Diagnostic trace export may
be disabled. Missing/reset counters, absent required measurements, zero CPU,
incomplete or mismatched populations, unreadable or missing artifacts and failed
record/evidence writes remain plumbing failures. Failures name the engine, step,
boundary and missing or inconsistent evidence.

Smoke uses the shared runner, judgment entry and report machinery with production
criteria explicitly inapplicable. Record all observed values and reasons,
including dropped/refused calls, server errors, incorrect returned results,
capture shedding and missed performance targets. Those observations, and a
saturated driver, do not fail smoke. For example, accurately accounting for
10 dropped calls is successful plumbing; losing those 10 calls from the outcome
totals is a reconciliation failure. Smoke is PASS/exit 0 when its plumbing works
and INVALID/exit 2 when it does not; PASS never claims production criteria passed.
Every required measured population remains nonempty. Full runs retain all normal
criteria, sample floors, durations, repetitions, references and FAIL/exit 1 rules.

Separate deterministic tests feed controlled passing, failing and invalid
readings through the normal full-mode judge, then persist/read the record and
regenerate the report. They prove that a known criterion miss becomes FAIL with
its observed value and exit 1, while invalidity retains precedence and exit 2.
This proves judging without requiring live smoke workloads to meet production
targets or changing the real benchmark's limits.

A full invocation never runs a smoke first and never consults smoke records.
Smoke runs only when invoked: `--smoke` on any L1/L2 command smokes that plan,
and `mise run bench:smoke` smokes every default plan. Smoke mode does not
recursively run itself. Build and external prerequisites remain outside the
experiment clock, which includes normal host checks, startup, loading,
measurement and drain. Smoke fixture/publication work must not pollute full-run
counters or state.

Smoke records, timing evidence and reports live only under
`target/benchmarks/smoke/`, never committed benchmark history. New records declare
`mode: smoke` or `mode: full`; readers retain older records' original full-run
meaning when that field is absent. Smoke records are always baseline-ineligible,
including when manually moved or supplied as a reference. Reports identify smoke
as functional pipeline evidence rather than performance qualification.

Reset or otherwise establish equivalent declared data state between repetitions;
repeated writes must not turn later repetitions into duplicate acknowledgments.
The Parquet query fixture and its fixed size/seed are declared before measuring.
Measured windows follow REQ-003; the 2M-row configuration does not end
measurement early, and writes use fresh identities as needed. Throughput uses
the baseline regression rule with no new absolute floors. Growing staging/publication backlog is visible;
a short write-ack rate is not labelled sustained storage throughput. Retain only native
write ≥200k total rows/s and OTLP wire ≥30 MiB/CPU-s as sanity
floors on each respective sweep's peak capacity, the better of its 1- and
64-client steps; record the peak client count. All cases declare compatible replacement-baseline criteria for their applicable
metrics under REQ-011;
previous `standard-hot`, `standard-parquet` and write entries remain provenance
context until their methodology compatibility is established. Entries for other
client counts remain in `baselines.toml` but no step declares them. A step
with no eligible entry declares baseline-unavailable explicitly and cannot claim qualified regression
coverage until a compatible reference is established.
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
| Bifrost | 100M-row heavy fixture (today's `--heavy`); ingest at 70% of the contended sustainable rate defined below, split by rows 50% OTLP spans / 30% logs / 20% native. 32 query clients: 60% point lookups, 25% dashboard aggregates (small and 1m), 10% table aggregates, 5% scans. Forge compaction on; reads while writing and full-queue behavior. Record the L1 calibration and declared server CPU budget; the budget sets load, never the measured CPU denominator. |
| Verification | Direct and background verification together, Bifrost evidence writes, backlog progression and bounded drain. |
| Gateway | Open-loop capacity with captures, declared overload, then recovery; judge server completion, first-token handoff, full-stream handoff, overhead, capture accounting, and recovery concurrently where applicable; client timings and inter-chunk lag are diagnostic-only. |

For stateful overload/recovery workloads, each of the three repetitions runs
the complete capacity → overload → recovery sequence from equivalent initial
state. Its phases have separate criteria and metrics; compare the matching phase
across repetitions. Warm up the complete sequence, not recovery in isolation,
which would erase the backlog recovery is intended to measure. Other L2 steps
also declare and preserve their initial state across repetitions.

Bifrost L2 calibrates its ingest rate under contention, inside the same run
and clock, before its measured repetitions. With the declared 32 query clients
and Forge compaction running on the loaded heavy fixture, it offers the
declared row mix at increasing rates and finds the highest rate the server
sustains: achieved arrivals match offered arrivals and backlog grows by at
most one second of offered work over a declared calibration window. Measured
repetitions then run at 70% of that rate. The record keeps each calibration
step's offered and achieved rate and backlog, the selected sustainable rate
and the resulting load. A calibration that cannot find a sustainable rate, or
whose search exceeds its declared share of the budget, is INVALID; no
historical or isolated L1 rate substitutes for it. The search bounds, window
length and step size, and the small/1m split within the 25% dashboard share,
are run controls fixed before execution. Isolated L1 write capacity overstates
what the server sustains while queries and compaction compete, so it no longer
sets L2 load. The fixed-client L1 SLO and L2's 32 query clients have different
applicability: L2 uses the concurrent criteria below; it cannot claim a
64-client SLO from a 32-client observation.

Concurrent Bifrost L2 criteria: point lookup p99 <5 ms; small aggregate
p95 <100 ms; 1m aggregate p95 <300 ms; heavy scan p95 <2 s (≥200 samples per repetition); server write
acknowledgement p99 ≤100 ms. The 5% heavy-scan population needs at least 200
completed samples in each six-minute repetition, rather than the infeasible
1,000-sample p99 requirement; insufficient evidence remains INVALID. Declare
an offered query rate capable of at least 200 scans per repetition (at least
4,000 total queries at the unchanged mix), and check actual completions.
Write-to-query visibility, including the historical p99 ≤5 s target, is
diagnostic-only because client polling cannot establish a server request boundary. Table aggregates retain correctness
and efficiency/regression criteria without a new absolute latency floor.
Backlog may grow by at most one second of offered work over the measured
window; drain completes ≤60 seconds after load stops. After overload ends,
SLOs must be met again within 30 seconds. Apply these backlog/drain/recovery
bounds to corresponding Verification and Gateway L2 phases too. The capture `lost` count must be zero always. Explicitly accounted shedding
is allowed only in the deliberate overload phase and is recorded separately
from `lost`; it cannot excuse any lost capture. Outside that phase, shedding
fails the run.

Gateway capacity constants, all measured from server handler entry: buffered
completion/result ready p99 ≤200 ms, first-token response handoff p99 ≤350 ms,
and final-frame handoff p99 ≤1,100 ms. Inter-chunk lag and its historical
p99 ≤50 ms reference are diagnostic-only; it does not start at handler entry. They are declared defaults,
not merely flags; any explicit override is fixed and recorded before measuring.
The strict <500 µs overhead SLO at 64 clients remains unchanged.

**REQ-010 — Invocation and documentation.** The new public commands are:

```text
mise run bench:l1:bifrost
mise run bench:l1:bifrost:writes
mise run bench:l1:bifrost:hot
mise run bench:l1:bifrost:parquet
mise run bench:l1:verification
mise run bench:l1:gateway
mise run bench:l1
mise run bench:l2:bifrost
mise run bench:l2:verification
mise run bench:l2:gateway
```

`bench:l1:bifrost:writes` runs the `writes-parquet` experiment.
`bench:l1:bifrost` runs `writes-parquet`, `hot` and `parquet`
sequentially, each under fresh Postgres, preserving records and worst-exit
precedence even if an earlier experiment fails or is invalid. `bench:l1` runs every L1 experiment sequentially so they do not
contaminate one another.
All old `bench:*` entry points are replaced without aliases. L1 commands
accept the planned `--qualify-baseline` mode defined in REQ-011 and forward it
through aggregates; it is rejected with partial selections, --profile, --smoke or L2. No L2 aggregate
is required. Every experiment and both levels use identical commands and record
schema on all three platforms. Each engine command accepts optional `--profile`,
and `--smoke`, forwarded through mise (for example,
`mise run bench:l1:bifrost -- --smoke` or `--profile`);
`bench:l1` forwards it to each sequential engine run. There is no platform-only
experiment or alternative command. `benchmarks/README.md` explains levels, budgets, prerequisites,
validity, metrics/units, baseline eligibility, exits, examples, and how to read
a record, with a pointer from `TESTING.md`. Commands and documentation above
are implementation obligations.

### Historical baselines and regression

**REQ-011 — Comparable baselines.** Keep existing baseline TOMLs and provenance;
do not delete or rewrite historical values to pass a run. Existing single-run
references and runs with different collection/tracing policies are advisory
until methodology compatibility is demonstrated. Mark the reason explicitly.
Establish replacement references from two clean, complete same-host L1 suite
runs with the final histogram/tracing policy, keeping all repetitions and
spread. Use the median of the six repetition values (mean of the two central
values when sorted) as the reference; latency references retain value bounds. Require
both suites to meet absolute/correctness/validity criteria. Reference promotion
is an explicit reviewed TOML change; it never happens automatically during a
benchmark. No reference is created from FAIL, INVALID, partial, dirty, profiled
or diagnostic-capture runs. Source records and exact method identifiers remain
linked. `--qualify-baseline` is an explicit L1-only invocation mode declared
before measurement. It requires full coverage, clean non-profiled source and
ordinary telemetry configuration. It omits relative criteria, records
baseline-unavailable and regression-coverage=false, and may PASS only the
absolute/correctness/validity criteria. Such a PASS is eligible qualification
evidence, not proof against regression. Two such suites establish initial
references. Normal L1 requires compatible references for every applicable
metric/case; absence is INVALID, not an implicit qualification run. L1 full-scan
latency remains descriptive as above; its CPU/throughput references are required.

Match engine/case, concurrency, fixture size/seed/schema/payload, storage state,
build/features, server limits, CPU model/available cores, OS/architecture,
measurement method/range/precision/boundary and telemetry policy. A deliberately
incompatible comparison is advisory only. Never relabel historical work/CPU-s
as throughput per available core. Preserve decimal MB to MiB conversion where
historical values are displayed. Store baseline identity/operands before work;
a declared comparison with missing or incompatible operands is INVALID.

**REQ-012 — Noise-aware regression.** A baseline criterion fails only when
W/C efficiency or throughput per provisioned core is more than 10% below
its reference, or CPU/operation or the declared latency percentile is more
than 15% above, **and** the difference is beyond the run's own repetition
spread. The comparison rule proposed here: for three valid values, let `m`
be median, `lo` minimum, `hi` maximum, `s = hi-lo`, and `b` the baseline. For
higher-is-better efficiency, fail only if `hi < 0.90b` and `b-m > s`. For
lower-is-better CPU/operation and latency, fail only if `lo > 1.15b` and `m-b > s`.
Exact equality at either percentage boundary does not fail. This conservative
rule requires all three repetitions beyond the percentage limit and a shift
larger than their spread. It does not establish statistical confidence or hide
an absolute SLO/correctness failure. Record the operands and both comparisons. Apply latency comparison
conservatively using quantization intervals; an approximation-boundary ambiguity
is INVALID, not a favorable rounding. Preserve absolute objectives separately.

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
failed, or discarded full experiment to the following history; smoke uses the
separate local destination in REQ-008 through the same recorder:

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
never stages or commits automatically. Full histogram distributions, diagnostic traces, logs and profiles remain
under `target/` with optional record references, not embedded in the summary.

Schema version 2 is the revised persisted contract. Retain read/report support
for existing version-1 records with their original raw-sample meaning; do not
rewrite them or compare their method silently against version 2. Unknown
versions are rejected. The following field names
and meanings are proposed; nested metric/criterion entries carry
stable identifiers and explicit units rather than formatted table strings.

| Required JSON field | Meaning |
|---|---|
| `schema_version` | Integer 2 for new records; legacy 1 remains readable. Breaking semantic/shape changes require a new version; readers reject unsupported versions explicitly. |
| `engine`, `experiment`, `level`, `question` | Declared identity, level integer 1/2, and question. |
| `mode` | `full` or `smoke` for new records; absent on older records retains their original full-run meaning. Smoke is never an eligible baseline, independently of filename/location. |
| `started_at`, `finished_at` | UTC RFC 3339 instants, including partial/INVALID runs. Elapsed budgeting uses monotonic time, also stored. |
| `commit`, `dirty`, `dirty_override` | Full measured-source SHA, source dirtiness at declaration, explicit override state. Dirty runs store `dirty: true` and `diff_sha256`, SHA-256 of the bytes from `git diff HEAD`. Output created by this run does not retroactively dirty its source. |
| `host` | OS/version, architecture, CPU model/logical count, physical memory, and declared storage context; record available provisioned vCPUs P per server host and combined scope separately from mean used cores C/T. P supplies throughput per provisioned vCPU; C supplies work/CPU-s. |
| `build` | Profile, feature/compiler flags, compiler and binary identities; detect a stale binary rather than attribute it to the current tree. Existing build stale-binary detection applies; no additional source-to-binary correspondence validity rule is required. The diff digest does not make a dirty run baseline-eligible. |
| `command` | Exact invocation as argument values plus the non-secret configuration needed to reproduce it. |
| `variables`, `controls`, `criteria` | Sweep and fixed controls, validity/budget declarations, metric definitions (including latency clock/resolution, histogram method/range/precision, quantization interval, population and expected/recorded counts) and criterion IDs/operators/limits/units/applicability, chosen baseline snapshots. |
| `steps` | Ordered declared steps: IDs/params, timings, metrics with units, three repetition summaries in full mode or one in smoke mode, median/min/max spread, per-criterion observed values/verdicts/reasons and explicit applicability, step verdict. Missing observations are null with reasons, never NaN or infinity. |
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
shows consumed seconds, mean used cores and provisioned vCPUs, and driver CPU is separate. Rate cells/supporting detail label both work/s/vCPU and work/CPU-s; queries also show scanned rows/bytes per query alongside server time. Client timing diagnostics cannot populate judged latency columns. A
non-applicable value is `—` with a reason. Additional engine metrics and spread
are rendered from the same record as supporting detail. Reports can be
regenerated without rerunning the experiment or loading full distributions. Console
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
permissions or limits to obtain a profile. See portability research
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
correctness coverage. Evidence for current consumers is in research.

| Current coverage | New owner / level |
|---|---|
| query-capacity standard, hot and Parquet tiers, telemetry fixture | Bifrost L1 `hot` / `parquet` |
| write sweeps | Bifrost L1 `writes-parquet` |
| heavy fixture, reads-while-writing, full queue, forge-capacity, client-queue | Bifrost L2 |
| verification case 1 / case 2 | Verification L1 / L2 |
| gateway-overhead / gateway-capacity | Gateway L1 / L2 |

This migration retains the workload purposes, not every historical flag,
verdict label, absolute floor, scoped launcher, or report layout. Historical
criteria are reconciled explicitly: L2 keeps server write-ack p99 ≤100 ms; client-observed visibility
p99 ≤5 s becomes diagnostic-only. Historical first-frame p99 ≤500 ms and raw latency compare gates
−10% throughput/+15% p95/p99 are not reactivated. The new declared criteria own judgment; relative
regression in revision 8 also covers provisioned-core throughput and compatible
server latency, without reactivating incompatible historical references. Point-lookup
<5 ms at 64 clients replaces selective targets at 1 client, including the later
≤1 ms target. Gateway's historical MIN_CALLS 100→30 is not silently adopted as
p99 sufficiency. Capture shedding is counted separately and allowed only in deliberate overload; any nonzero `lost` count always fails.
Gateway capacity SLOs become declared constants with recorded explicit overrides,
rather than required ad hoc CLI flags alone. The constants above resolve D-1/D-2.

### Production tracing and optimization

**REQ-017 — Independent tracing policies.** Keep one server tracer provider and
the existing W3C continuation/link semantics. Logs use `WYRD_LOG`/`RUST_LOG`;
operator trace verbosity uses a separate `WYRD_TRACE_FILTER`, default `info`.
Neither log level nor operator filter/sampling may disable required tenant
catalog copies. With an operator endpoint, the absent-ratio default becomes
0.01; the existing `WYRD_OTLP_SAMPLE_RATIO` explicitly overrides it, validated
in [0,1]. With no endpoint, no operator export occurs. Client sampled flags,
trace IDs and tracestate cannot increase operator export. Tenant copying works
without an operator endpoint and under operator ratio zero.

Review the actual operator span tree for each engine. Keep meaningful request,
queue/admission, engine-stage, dependency, retry, streaming lifecycle and
background-operation boundaries at normal trace verbosity; function-level
implementation detail is debug-only. Required catalog spans and handler
instrumentation are preserved. Document a compact retained/moved span inventory
and inspect representative traces; an arbitrary spans/request limit is not a
criterion. A global filter before both destinations cannot implement this
policy. Keep production and test subscriber policy consistent.

**REQ-018 — Bounded export and debuggable failures.** Keep the installed
OpenTelemetry BatchSpanProcessor as owner of its bounded queue, worker, batching,
flush and shutdown. Retain its queue-overflow warning and exact lifetime drop
total logged at orderly shutdown. Exact live queue-drop counts are not required:
the installed SDK does not expose them. Do not parse warning messages into span
counts, count warning events as dropped spans, fork the SDK or replace its
processor to obtain a counter. Document that filtering SDK warnings or abrupt
termination can remove these log signals; keep warnings enabled in the normal
operator configuration.

Expose failed operator export batches as a low-cardinality counter through a
small wrapper around the existing exporter. Increment once for each export call
returning an error and preserve its result; successful calls do not increment.
A failed batch is not a count of queue-dropped spans or proof of permanent loss.
Delegate exporter flush/shutdown behavior; no new queue, retry policy, worker or
export framework is introduced. Tenant loss counters remain independent.
Exporter failure, stall or saturation cannot change the originating request
result or tenant-copy policy. Errors retain scrubbed trace/request-correlated
logs regardless of trace sampling. No claim promises every error trace from
head sampling. Tail-sampling infrastructure and a second provider remain out
of scope. Prove bounded saturation, SDK loss reporting, accurate failed-batch
counts, bounded flush/shutdown/interruption and preserved request/tenant results.

**REQ-019 — Optimization workflow.** Reports identify changed case and stage
metrics. Engineers rerun that case with existing operator ratio/filter overrides
and optional samply to investigate individual waits versus CPU-heavy functions,
then judge improvement under the original baseline configuration. Diagnostic
runs record their configuration, export loss and profile availability and are
ineligible references. Document temporary detailed capture on a selected server
instance, restoring normal settings afterward; no new dynamic capture control
plane or client-forced operator sampling is introduced. Client spans appear in
Wyrd's timeline only when the client exporter also sends them to Wyrd. Preserve
Rust/Python/TypeScript timeline behavior and tenant-negative coverage.

**REQ-021 — Engine limits met in this change.** The declared limits stay as
written. TASK-005's runs on the reference host missed four of them, and this
change optimizes the engines until each passes on that host:

- Verification `direct-verify` at 64 clients: the per-request persistence
  work that dominates server CPU. Each execution's inputs are stored once, in
  `execution_inputs`; its Verifier's requirements and steps once, in
  `execution_outputs`; and `results` keeps only the payload-free summary.
  Result-returning APIs assemble `evidence` and the breakdown from those
  records; PostgreSQL stores no input values. Each stored Variant is encoded
  directly from its typed value, not through JSON text. Staged rows are
  admitted by the memory they own, and different executions' writes drain
  through the producer outbox lanes concurrently, each execution's in order.
  Staged verification and audit rows are built once into dense batches when
  their lane drains; the local Scribe ingests them as server-built canonical
  batches while peer IPC keeps full validation and the same batch identity;
  stored Variants are validated without copying; and persistence sorting
  gathers payload from its source batches instead of concatenating it first.
- Bifrost point lookup, from both the memtable (`hot`) and Parquet (`parquet`):
  the projection cost that holds p99 above 5 ms at 64 clients.
- Bifrost `writes-parquet` OTLP spans: the ≥30 MiB/CPU-s wire floor.
- Gateway `overhead`: the <500 µs overhead SLO at 64 clients, including the
  payload capture that serializes canonical JSON twice.

Each optimization keeps INV-001 and INV-002: exact results, authentication,
tenancy, audit and durability are unchanged, and no work moves outside the
measured server. Use REQ-019's workflow to locate the cost and judge the fix
under the normal configuration. References are qualified only after every L1
limit passes; a limit that still misses blocks qualification instead of being
relaxed or recorded as accepted.

**REQ-020 — Qualification and discoverability.** Before baseline promotion,
measure the same clean build with operator endpoint absent versus ordinary
1% export. Use identical workload/fixtures and record the configurations; report the observed CPU,
latency and throughput difference without subtracting it. Run each engine's
full L1 separately and report after each. Establish compatible references and
prove injected/controlled regressions in latency, throughput/core and CPU/work
fail without weakening limits. Run full L2 once on the integrated candidate;
L2 evidence may be collected after the L1 qualification work, in its own task.
README explains matrix, quantities, startup/prerequisites, runtime budgets,
partial selections, reference qualification, diagnosis and recovery. L1 remains
manual and outside CI/gates; contributors run the owning engine after its code
changes and all engines after shared measurement/scheduling changes. Confirm
the documented commands on Linux x86_64; no new setup script is required.

## Invariants and constraints

| ID | Promise and failure prevented |
|---|---|
| INV-001 | Production server/client paths, exact result checks, authentication, tenant binding and permission checks stay enabled. A benchmark cannot obtain speed by bypassing real work or reading another tenant's data. |
| INV-002 | Acknowledged durability and publication semantics remain unchanged. Report accepted work, durable publication, and visibility distinctly; benchmark convenience cannot weaken storage guarantees. |
| INV-003 | Authorization still stages non-blocking audit at each permission decision; its production cost stays measured. Benchmark records are repository evidence, not canonical `bifrost.system.audit_log` rows. |
| INV-004 | Server/driver accounting is disjoint and portable on all supported platforms. Provisioned vCPU count P and consumed CPU C have separate labelled metrics; missing readings or an unavailable profiler cannot fabricate favorable efficiency. |
| INV-005 | Declared controls/criteria cannot change after observing performance. Invalid measurements never become baseline evidence or a performance FAIL. |
| INV-006 | Committed records contain synthetic-fixture summaries only: no keys, tokens, private endpoints, tenant payloads, or sensitive host identifiers. Reproducibility records symbolic secret inputs, never secret values. |
| INV-007 | No compilation or synthetic host load runs during an experiment. A busy real host is discarded; validity tests inject observations rather than deliberately saturate the host. |
| INV-008 | Summary history is append-only and attributable; no fabricated historical run, overwritten record, or silent unknown schema acceptance. |
| INV-009 | Every experiment at L1 and L2 runs unchanged on macOS arm64, Linux x86_64, and Linux arm64 with identical commands and record schema. The harness uses no cgroups, namespaces, `/proc`, `prlimit`, systemd scopes, credential switching, seccomp, Landlock, sandbox-exec, or `cfg(target_os)` gating; a platform-specific request is invalid. This prevents platform-only workloads and accounting. External samply owns its native sampling backend; optional diagnostics do not relax the harness rule. |
| INV-010 | Changing operator/log configuration cannot change tenant trace authorization, catalog, scrubbing, linkage or nonblocking delivery. New local aggregate readout cannot expose request identities or become a network data route. |

Fixtures use disposable benchmark-owned state. Cleanup cannot touch an operator's
unrelated data. Cancellation stops admission, drains/cancels owned work under
bounds, and reports incomplete evidence. Existing production queue, resource,
and tenancy limits remain effective; a full queue is measured, not unbounded.
The shared benchmark owner does not migrate durable server behavior into an SDK
or into `wyrd-spec`. No new third-party package or Cargo feature is selected: sysinfo and HDR are
already in the workspace and samply is an optional external tool. An existing
workspace dependency may be wired into its narrow runtime owner; test-tooling
crates must not become production dependencies. The selected tools are
constraints; private implementation choices remain open.

## Proposed expensive-to-reverse decisions

1. **Ownership:** one `wyrd-testing::experiment` workflow for all three engines;
   engine-specific workload behavior remains with existing owners.
2. **Persistent evidence:** versioned JSON schema v2, exact three-engine directory
   layout, timestamp/SHA identity, committed summaries and external raw artifacts.
3. **Levels:** every L1 experiment command ≤5 minutes at 1 and 64 clients;
   Bifrost L1 as three independently runnable experiments; L2 20–30 minutes
   per engine with contended in-run ingest calibration;
   three repetitions per step, defined clock boundaries, opt-in commands
   replacing old tasks without aliases.
4. **Metric semantics:** both provisioned-vCPU and consumed-CPU denominators, explicit byte units,
   server-only judged latency,
   bounded high-resolution histograms, independent diagnostic tracing,
   process scope, spread, fixed-client SLOs and portable readings.
5. **Baseline contract:** TOML references with provenance; no historical record
   import; eligibility visible; conservative percentage-plus-spread regression.
6. **Portability:** identical experiments, commands and schema on all three
   platforms; plain-child servers, sysinfo readings and optional samply
   diagnostics, without OS resource envelopes. Required real evidence is Linux
   x86_64 only; the other two platforms require unchanged runnable behavior.
7. **Validity versus failure:** in full mode, INVALID has precedence and exit 2;
   valid criterion misses exit 1; every step is judged, including overload/recovery.
   Smoke checks plumbing with production criteria explicitly inapplicable.

8. **Smoke:** the actual experiment workflow with reduced declared controls,
   run only on request, explicit criterion applicability and local-only,
   permanently baseline-ineligible records. A separate probe runner is rejected
   because it cannot validate the measurement and evidence workflow and can drift.

Revision 10 preserves the approved measurement, comparison, coverage, record
and tracing decisions. The maintainer explicitly approved the plumbing-only
smoke correction and directed revision of its remediation packet; ordinary
private structure remains implementation-owned.

## Acceptance: user and agent journeys

These are evidence obligations, not claims that verification has run. Stable IDs map promises once in the final column. Real
workload journeys use the actual release server through public SDK/OTLP/Gateway
surfaces and repository Postgres/storage dependencies. Where suitable, existing
`WyrdTestServer` journeys verify production seams; they do not substitute for
release-process benchmark evidence. This change adds no Python, TypeScript, or
MCP API that would require a new product-surface projection.

| ID / situation and actor | Expected observable result / credible evidence | Obligations |
|---|---|---|
| AC-001 — Engineer runs one engine's L1 | Each engine command is exercised separately, and each Bifrost experiment (`writes-parquet`, `hot`, `parquet`) within its 5-minute budget, without running a smoke first; `hot` reads show no staged or published rows and `parquet` reads show nothing left in a memtable, staging or claim. Within its budget the full matrix runs with discarded warmup and three repetitions. Engineer reads question, controls, units, per-step criteria and verdict; opens the printed JSON path and regenerates an identical report. Real Linux x86_64 release-process journey, record and report comparison. Recompute server latency bounds from retained interval histograms; changing client network delay, polling cadence or scheduled-time latency cannot change server latency judgment (driver send lateness still checks validity); verify exact values just below, equal to and above strict SLO limits, and reject missing/overflowed timing evidence and ambiguous threshold bounds; coarse monitoring buckets cannot judge latency. Operator export disabled or lossy cannot erase complete benchmark timing. | REQ-001, REQ-003, REQ-007, REQ-008, REQ-010, REQ-013, REQ-014 |
| AC-002 — Engineer compares a valid regression with its baseline | W/C baseline 100, repetitions 75/80/85: criterion FAIL and exit 1; a CPU/operation case beyond +15% and spread also fails. Exact percentage boundaries pass. Deterministic judgment cases plus an eligible compatible replacement-baseline-to-run integration journey preserve operands and provenance. | REQ-011, REQ-012 |
| AC-003 — Engineer encounters noisy results | Baseline 100, repetitions 70/85/100: regression criterion PASS with spread visible; another absolute SLO miss still makes the run FAIL. Shared-judgment cases prove noise cannot excuse latency/correctness. | REQ-004, REQ-012 |
| AC-004 — Engineer cannot obtain valid evidence | Independently exercise busy preflight/post-run host (10-second samples, >10% non-benchmark CPU), driver above 80% budget, any missed arrival or p99 send lateness >5 ms, insufficient samples (<1,000 p99 / <200 p95), dirty source without override, missing/reset CPU and budget overrun: INVALID, exit 2, reasons and partial record, no fabricated successful step. Declared dirty override is visible. Combine an invalid step and valid failure: overall INVALID retains both. Injected resource/clock observations and runner integration; no synthetic host load. | REQ-002, REQ-004, REQ-006, INV-005, INV-007 |
| AC-005 — Engineer runs and compares supported platforms | Prove real CPU accounting and every L1 and L2 experiment for all three engines on Linux x86_64. Inspect portability of the same commands, schema v2 and measurement paths on macOS arm64 and Linux arm64; real runs there are optional and missing runs do not block completion. Recompute (W/T)/P, W/C and C/W from records, with distinct labels and recorded P; results agree and never substitute removed quotas. Verify write rows and uncompressed MiB in both normalizations and query scan rows/bytes per query alongside server time. Cross-host and cross-OS comparisons show both OS/architecture fingerprints and remain advisory, never same-host regression proof. Exercise `--profile` on Linux x86_64 and missing/unusable profiler cases: capture or an explicit note is recorded without affecting judgment; required accounting stays mandatory. Real release-process journeys/records plus deterministic arithmetic, provenance and diagnostic-failure checks. Static command/source review confirms both levels and all experiments lack OS-specific paths and resource envelopes. | REQ-006, REQ-010, REQ-011, REQ-013, REQ-015, REQ-016, INV-004, INV-009 |
| AC-006 — Agent reads history without prose parsing | Agent discovers engine/experiment records, validates new v2 and legacy v1 fields, reads numeric values/units, repetitions, baseline snapshot and typed verdict/reasons, and identifies omitted/invalid steps. Unsupported version refuses interpretation. Report regeneration works without target artifacts; same-second/SHA collisions use -2/-3 and cannot overwrite history; temp-write/rename failure cannot return PASS. A dirty override records the SHA-256 of `git diff HEAD`. Record-reader/writer integration and malformed/unsupported input cases. | REQ-013, REQ-014, INV-008 |
| AC-007 — Engineer runs L2 with competing work | For each engine, observe real overlapping foreground/background work during each repetition, offered versus achieved arrivals, concurrently evaluated SLOs, bounded backlog and drain. Bifrost records its contended calibration steps, selected sustainable rate, 70% load and 32 query clients; Verification includes evidence writes; Gateway overload/recovery judges capture accounting. A missed concurrent criterion fails despite an isolated case passing. Real workload journeys, timelines and records. | REQ-005, REQ-009 |
| AC-008 — Contributor adds an experiment | Contributor declares question/variables/controls/metrics/criteria and supplies workload behavior to the existing shared experiment owner. The same validity, repetitions, report, record and verdict behavior applies without another measurement/report implementation. One small representative integration experiment and source review demonstrate reuse. | REQ-015 |
| AC-009 — Maintainer checks migration and discoverability | README and TESTING explain invocation and records; coverage mapping has no lost workload; old tasks/aliases, dead thresholds/SloGate/load exports and duplicate code are gone. Every client-queue step judges criteria; Gateway has declared capacity SLO constants. Static consumer/task/docs inventory plus relevant existing correctness lanes; no benchmark added to CI/gates. | REQ-010, REQ-016 |
| AC-010 — Operator checks preserved production guarantees | Wrong results and unexpected loss fail valid runs; tenant-negative journeys cannot expose data; acknowledged writes survive existing durability checks; audit stays non-blocking and accounted; fixture cleanup remains scoped and credentials never enter summaries. Existing security/storage journeys and focused record privacy/cancellation checks are credible evidence. | INV-001, INV-002, INV-003, INV-006 |
| AC-011 — Operator changes logging/sampling or loses its backend | Production-initialized tests prove independent log/operator policies and SDK overflow/shutdown loss reporting and accurate failed-export-batch counters; real authenticated client → gateway → tenant timeline journeys retain catalog IDs/links at ratio zero/no endpoint, with tenant-negative readback. All shipped language surfaces retain their existing journeys at integrated review. | REQ-017, REQ-018, INV-010 |
| AC-012 — Engineer smoke-tests the actual benchmark without trace export | Real release commands exercise every default L1/L2 plan for all engines, including all three Bifrost L1 experiments, through actual loading/publication, selected steps/phases, measured stop/drain, exact outcome reconciliation, record/full timing evidence and report. Read the record and regenerate its report; verify retained histograms reproduce recorded readings. Accurately recorded production errors, dropped/refused calls, capture shedding, wrong answers and target misses do not fail smoke. Missing measurements/counters, mismatched populations, zero CPU, missing applicable scan facts and failed evidence/report handling are plumbing failures. Production criteria remain explicitly inapplicable with observed values retained; smoke PASS claims plumbing only. A separate deterministic full-mode test proves a controlled criterion miss survives judgment, record/readback, report and exit 1, and invalidity retains precedence/exit 2. Selected-step proof remains; a saturated driver does not invalidate a smoke, and a full run runs no smoke. Mode round-trips, output stays outside history and smoke cannot become a baseline after relocation. Deterministic interval/quantization/overflow/reset/drain proof and bounded known-delay, known-CPU and instrumentation-cost calibration use the actual measurement path with independent references and justified tolerances, without synthetic host load or a side runner. Normal readout absence, non-loopback refusal and snapshot privacy remain. | REQ-007, REQ-008, REQ-013, REQ-015, INV-006, INV-010 |
| AC-014 — Maintainer checks the engine limits | Clean full L1 on the reference host passes Verification 64-client latency, Bifrost hot and Parquet point-lookup p99 <5 ms at 64 clients, the OTLP spans ≥30 MiB/CPU-s floor and Gateway <500 µs overhead at 64 clients, with unchanged limits, exact results and existing tenancy/durability journeys green. Before/after records and the diagnostic run that located each cost. | REQ-021, INV-001, INV-002 |
| AC-013 — Maintainer establishes regression evidence | Only after AC-014 passes, two clean full suites establish reviewed same-method references; latency/throughput-core/CPU-work regression cases fail correctly. The same-build export comparison quantifies overhead; diagnostic traces and profile reproduce the documented investigation workflow without becoming references. | REQ-011, REQ-012, REQ-019, REQ-020 |

Implementation planning must select exact focused commands and evidence under
these decisions; a proposed task is not a claim that its tests already pass.

## Delivery and supersession

The maintainer requested this revision and task packet after aligning on the
proposal. The proposed dependency order is:

1. [TASK-003: production tracing](tasks/TASK-003-production-tracing.md).
2. [TASK-004: independent server timing](tasks/TASK-004-server-benchmark-timing.md).
3. [TASK-005: complete regression suite](tasks/TASK-005-regression-matrix.md).

Revision 11 adds one revision task after TASK-005. It drops `writes-hot`,
applies the L1 turnaround and smoke reuse changes,
makes the REQ-021 optimizations, and runs only L1: the limit checks, the
telemetry comparison and reference qualification. The contended L2
calibration and the full L2 runs follow in a later task; the revision task
does not run L2. TASK-005's recorded evidence remains history.

Revision 8 replaced remaining execution instructions in TASK-001, TASK-002 and the
TASK-001-R1 refinement. Existing implemented fixes,
review findings, verdicts and run artifacts remain evidence, not erased work.
Carry forward relevant resource/window/lifecycle/cancellation/doc fixes; the
raw timing collector, its mandatory helper and exact-span matching obligations
retire only when their replacement histogram proof passes. Do not undertake
unrelated helper cleanup, baseline format migration or a harness rewrite.

Prior findings map as follows: timing export/type and span population findings
1/4/5/6 move to bounded histogram/population proof in TASK-004; required metrics,
window accounting and common controls findings 2/7/8 map to TASK-004/005;
open-arrival, cancellation and profile lifecycle findings 3/9/10 remain
TASK-005 obligations; rustdoc finding 11 applies to each changed owner; real
qualification finding 12 remains once at integrated change review. No review
finding is declared fixed merely by revising this spec.

The maintainer directed the real-path smoke correction into the existing
[TASK-004-R2 remediation](review/TASK-004-R2/TASK-004-R2-finish-stop-counters-and-otlp-cohorts.md).
That direction is separate from the four independently validated residual
findings. The current smoke contract supersedes TASK-004/005
instructions for request-only smoke, skipping fixture/publication work, accepting
probe-only evidence. Revision 15 removes the automatic prepass revision 9
introduced. Their
historical evidence remains intact. TASK-005 full-run
qualification obligations remain; a small smoke PASS cannot close them.

The maintainer subsequently clarified smoke's verdict and withdrew
FIND-TASK-004-7 as written. Revision 10 supersedes any task/review instruction
to fail smoke for accurately recorded production errors, loss, wrong answers or
criterion misses. The revised TASK-004-R3 retains F8 calibration and separately
proves normal full-mode judging with controlled results. Historical review
reports remain evidence under their recorded authority, not current instructions.

## Open material decisions

No material decision remains open in revision 11; the maintainer decided all
five items on 2026-10-26. Revision 8's approval and the directed
real-path smoke workflow persist; the maintainer explicitly approved revision
10's plumbing-only verdict and separate controlled judgment proof. Private
scaling choices remain implementation-owned.
Historical clean-source gaps remain recorded as exclusions/advisory evidence,
not a reason to invent a reference or weaken coverage.

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
  comparison and three-platform L1 proof. This revision and its task and
  implementation were subsequently rejected for bucket-based latency judgment.
- Revision 4 — 2026-10-24: Replaced bucket-derived latency and resolution rules
  with raw per-request samples, nearest-rank percentiles and standard
  OpenTelemetry tracing of exact internal timer observations. Defined component
  matching, completeness and invalidity; retained strict SLOs. Required evidence
  is Linux x86_64 only while all experiments remain portable unchanged to macOS
  arm64 and Linux arm64. Removed rejected implementation evidence from the
  single proposed task. Status is draft, awaiting maintainer approval; no code
  or benchmarks changed/run for this revision.

- Revision 5 — 2026-10-24: Clarified server-throughput/overhead purpose and
  handler-entry-to-result-ready/ack/handoff latency; made client timing,
  visibility polling and inter-chunk lag diagnostic-only. Retained per-request
  wait-subtracted tracing overhead. Added throughput per provisioned vCPU
  alongside work per consumed server CPU-second; baselines/regression use the
  latter, with explicit historical MB-to-MiB conversion. Changed L2 heavy scans
  to p95 with ≥200 samples per repetition and declared query-rate feasibility.
  Preserved all experimental lifecycle, levels, evidence-platform and retirement
  obligations. Approved by the maintainer on 2026-10-24.

- Revision 6 — 2026-10-24: Split Bifrost L1 into `writes`, `hot` and
  `parquet` experiments at 1 and 64 clients with per-experiment budgets;
  restored hot-tier reads and telemetry span reads (Bloom `trace_id` lookup,
  shredded and long-tail attribute filters, shredding and pruning checks);
  storage ratio from the single Parquet settlement; smoke without publication
  wait; L2 calibration from the `writes` record. Approved by the maintainer on 2026-10-24.

- Revision 7 — 2026-10-24: Maintainer-aligned proposed plan: replace mandatory
  raw timing spans/100% OTLP with bounded HDR interval distributions; retain
  server-only wait-subtracted boundaries; preserve client/tenant dual tracing
  while separating log/operator policy and selecting a configurable 1% operator
  default; expose export loss; expand writes to hot/Parquet (24 Bifrost L1 cases);
  make latency and provisioned-core throughput regression dimensions; require
  same-method references and schema v2 while reading legacy v1; retain L1 fast
  selections, mandatory smoke, all L2 and portability obligations. Draft pending
  approval of the concrete revision and internal readout boundary.

- Revision 8 — 2026-10-24: Address plan-readiness finding PR-001 using the
  maintainer-selected simpler policy: retain the SDK batch processor and its
  overflow warning/orderly-shutdown drop total, count failed export batches with
  a narrow exporter wrapper, and require saturation/flush/shutdown/interruption
  proof without owning a new queue or parsing loss logs. Other benchmark,
  tracing, matrix and client-timeline decisions are unchanged. Draft; earlier
  readiness report remains historical evidence, not a verdict on this revision.

- Revision 9 — 2026-10-25: At the maintainer's direction, replace the separate
  request-only smoke with reduced executions of the actual benchmark workflow,
  automatic isolated prepasses, explicit applicability and baseline-ineligible
  local records. Inspection during TASK-004 remediation found the old smoke
  bypassed fixture loading, measurement, judging and persisted report/evidence;
  its green result therefore could not establish that those paths worked.
  Add this correction to TASK-004-R2 without relabelling it as an old review
  finding. Draft; no source implementation or benchmark run is claimed.

- Revision 10 — 2026-10-25: The maintainer explicitly agreed that smoke verifies
  benchmark plumbing and directed the remediation packet revision. Production
  outcomes, including errors and dropped calls, are recorded without failing
  smoke; only broken measurement/evidence handling makes it INVALID. Withdraw
  F7 as written and prove ordinary full-mode judging separately with controlled
  results. F8 calibration and full benchmark criteria remain unchanged. Approved
  by that explicit direction; no additional approval is requested.

- Revision 11 — 2026-10-26: TASK-005's real runs (recorded in its V6–V9
  evidence) showed that the `writes-hot` override let the write workload
  exhaust host memory, that L1 took far longer than fast feedback allows, that
  four declared limits miss on the reference host, that isolated L1 capacity
  overstates contended L2 ingest, and that the driver had no way to exercise
  sampled-client tenant recording. The maintainer decided: drop `writes-hot`;
  ≤5 minutes per L1 experiment command at 1 and 64 clients with 3 × 5 s
  windows where sample floors hold and same-commit smoke reuse; keep the limits
  and optimize the engines in this change (REQ-021); calibrate L2 under
  contention; add `--client-trace-ratio` (REQ-022). Approved by the
  maintainer on 2026-10-26.
- Revision 12 — 2026-10-26: the maintainer removed `--client-trace-ratio`
  (REQ-022) and the sampled-client tenant-recording comparison from REQ-020
  and AC-013; that cost is not needed now. Approved by the maintainer's
  direction on 2026-10-26.
- Revision 13 — 2026-10-26: TASK-006 profiling at 64 clients showed most
  direct-verify CPU in persistence: the 2 KiB inputs were stored twice (inputs
  row and the result `outcome.evidence`) and every Variant went typed value →
  JSON text → re-parse → Variant. The maintainer decided: store inputs only in
  `execution_inputs`, keep `evidence` in API results by reading it from there,
  and encode Variants once. Approved by the maintainer on 2026-10-26.
- Revision 14 — 2026-10-26: TASK-006-R1 found inputs still copied into
  PostgreSQL and grouped snapshots, drift detail duplicated in
  `bifrost.drift.result_features`, and readback unable to assemble executions
  without an inputs row. The maintainer decided: three verification tables
  (`execution_inputs` per execution, `execution_outputs` per result,
  payload-free `results`), grouped and scheduled selectors recorded as input
  references, cumulative grouped history through each settled cutoff,
  PostgreSQL coordination only, and one result reader. Approved by the
  maintainer on 2026-10-26.
- Revision 15 — 2026-10-26: Smoke was coupled to every full run as an
  automatic prepass and judged driver saturation, so a server speedup that
  raised closed-loop driver CPU refused every full verification run before it
  measured anything. The maintainer decided: remove the automatic prepass and
  smoke-record reuse; smoke runs only through `--smoke` or `bench:smoke`; a
  smoke does not judge driver saturation, which full runs still enforce.
  Directed by the maintainer on 2026-10-26.

Authority (plain repository references):

- AGENTS.md; changes/spec-driven-development.md.
- architecture/wyrd-design.md — Doctrine, Bifrost, Coordination clock.
- architecture/bifrost-design.md — System boundary, Durability and visibility,
  Resource and failure invariants, Telemetry / Measurement meanings.
- architecture/wyrd-security-posture.md — Security principles, Tenant and data
  isolation, Audit integrity and privacy.
- agent_docs/agent-surfaces.md.
- changes/active/benchmark-experiments/research.md — historical criteria and
  measurement research. Historical values and provenance remain preserved; revision 8 REQ-011 governs
  comparison eligibility. Research creates no parallel requirements.
