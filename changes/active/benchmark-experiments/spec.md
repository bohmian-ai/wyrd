---
id: SPEC-benchmark-experiments
revision: 19
status: approved
---

# Benchmarks as experiments

Revision 19 makes a run VALID or INVALID and nothing else: a VALID run
measured what it claims, and its targets are reported, never failed. Each
record stands on its own: its results, host and controls, and the relative
metrics derived from them, such as MiB/s and rows/s per server core and CPU per
row or query. Comparing one run with another is a separate convenience and no
part of a run, its record or its validity; the reference file, qualification
mode and regression criteria are removed. It also fixes the writes budget and
history eligibility, adds realistic spans and write-phase timing, orders the
measured optimizations, and approves designing and implementing memtable
Variant shredding; cross-query plan reuse remains parked. Record history moves
off the code branches to the `benchmark-records` branch, checked out at
`benchmarks/history/`. The maintainer approved this on 2026-10-27.

Revision 18 measures the production binary and judges lost evidence. The
measured `wyrd-server` builds in its own cargo invocation, so the driver's
`test-support` features never unify into it. Each `direct-verify` repetition
records `lost_writes`, the Scribe outbox writes lost over its window and drain
for any reason, shed audit decisions included, and makes a full run INVALID above zero. The
maintainer approved this on 2026-10-27.

Revision 17 makes the benchmark simple enough to read without this document.
It is defined by the nine lines below. A requirement that says more says how,
never something different. Revision 19 replaces its verdict rule: VALID or
INVALID, with targets reported separately. Driver
CPU is reported, never judged. Each
measurement is stored once and every answer is derived from it. Only complete,
clean, unprofiled runs enter history. The maintainer approved this on
2026-10-26.

Revision 16 puts the slow tail of Verification in the record. Each
`direct-verify` repetition records its maximum overhead, how many executions
exceeded 0.5, 1, 5 and 10 ms of overhead, and the p99 and maximum of each
execution phase, all from the same server HDR timing. One HTML page renders
every record and charts each experiment's history in chronological order. The
maintainer approved this on 2026-10-26.

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

L1 is the manual benchmark suite engineers run whenever they change Bifrost,
Verification or Gateway. It measures server latency, CPU cost and throughput per
available core. L2 retains sustained contention, overload and recovery coverage.
Tracing remains available to explain individual requests; CPU profiling explains
expensive functions. Neither is required to produce benchmark latency samples.

Before: complete operator trace export was required to judge every request, so
export loss could invalidate a long benchmark and tracing cost could change its
CPU baseline. After: bounded server-side high-resolution histograms measure all
operations, while sampled operator traces and tenant-visible traces retain their
separate production purposes.

## The benchmark in nine lines

1. Each benchmark asks one question. It also checks that every answer was
   correct and nothing errored.
2. Each step warms up and is then measured three times, back to back.
3. The record stores each measurement once. The answer is worked out from it
   when the record is read.
4. The report shows VALID or INVALID with reasons, each target as met, missed
   or unsettled, and the run's own relative metrics.
5. VALID means trustworthy, complete measurement and integrity in every
   repetition; INVALID names the broken condition. Exits are 0 and 2 only.
6. Driver CPU is shown, never judged.
7. `benchmarks/` keeps only complete, clean, unprofiled full runs, including
   target misses and invalid results. Everything else goes to `target/benchmarks/`.
8. One HTML page shows each benchmark's answers over time, with each run's
   report.
9. Old records are deleted.

The three repetitions are back-to-back windows on one server process. Their
spread shows stability within a run, not between server starts, builds or
days; only the history across runs shows that.

## Problem, proposed change, and value

Wyrd's Bifrost, Verification, and Gateway benchmarks ask related capacity and
latency questions through different measurement and report implementations.
Some steps only report numbers; some thresholds exist only as invocation flags;
results mostly disappear into `target/`. An engineer or agent cannot reliably
answer what a result measured, whether it is trustworthy, or whether it is noise.

Treat every benchmark as an experiment: declare the question, sweep, controls,
metrics, and criteria before measurement, then produce one attributable record
and one report derived from it. One shared `experiment` module in `wyrd-testing`
owns instrumentation, measurement, judgment, and recording for all three
engines. Engine workloads retain their production workflows and fixtures.

For example, a clean, complete run with correct results can miss a latency
target and still be VALID, exiting 0. A busy host, incomplete measurement or
integrity failure makes it INVALID, exiting 2 with reasons. Nothing exits 1.
Complete, clean, unprofiled full runs retain history under either verdict.
Performance targets are observations, never CI gates.

Level 1 gives opt-in feedback within 5 minutes per experiment command, except
`writes-parquet`, whose budget is 360 seconds.
Bifrost has separately runnable hot reads, Parquet reads and
Parquet-configured writes; each has its own budget. Level 2 measures
mixed traffic and background work together over 20–30 minutes per engine.
The purpose is to measure server throughput and server overhead. Report both
throughput per provisioned vCPU and work per server CPU-second used, with
explicit denominators. Absolute latency objectives measure server handling time.

The costs are three measured repetitions per step, explicit validity checking,
portable resource accounting, and growing summary history on the
`benchmark-records` branch. High-resolution
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
named in REQ-021 and the Revision 19 supplement are part of this change; other engine optimization and
ClickBench reproduction are not. Repository architecture
remains authoritative.

“Experiment” here means a repository performance experiment, not a new Card
kind or a requirement to register an Experiment Card. There is no new server,
SDK, MCP, or UI product workflow. A benchmark-only, read-only histogram
snapshot on the existing metrics listener is the sole added measurement surface;
its exposure is constrained below. Existing client trace APIs are preserved. The public contributor surfaces are
`mise` commands, documentation and JSON records.

Non-goals: optimize an engine beyond REQ-021 and the Revision 19 supplement; relax a declared limit to make
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
  Missing declarations refuse measurement as untrusted; observed performance
  cannot choose or weaken the criteria afterward.
- **REQ-002 — Validity.** The shared runner checks quiet-host eligibility before
  workload execution, generalizing Forge's free-CPU discard to all engines.
  It also records driver CPU as context, scheduled/sent/completed/missed
  arrivals, sample sufficiency, source cleanliness, measurement availability,
  and elapsed budget. Busy host, missed open-loop arrivals,
  too few samples, dirty tree even with an explicit override, missing required
  resource readings, or over-budget execution is untrusted. A dirty override is
  declared before execution and stored; it permits diagnostic execution, not
  VALID status. REQ-008 defines smoke's sole sample-floor
  applicability exception; other validity checks are not bypassed.
  Sample host CPU for 10 seconds before the run and again after it. Either
  sample showing non-benchmark processes using more than 10% of logical CPUs
  invalidates the run. Driver CPU is never judged. Open-loop traffic is untrusted on any missed arrival or send-lateness
  p99 above 5 ms (Gateway's existing `LATENESS_LIMIT_US`). Each repetition needs
  ≥1,000 samples for each p99 criterion and ≥200 for each p95 criterion;
  fewer is untrusted. A judged slow-step percentile, including L2 heavy scans,
  uses p95 with ≥200 samples per repetition; there is no heavy-scan p99 exception.
  Send lateness is solely a driver-validity check, never a server latency SLO.
- **REQ-003 — Repetitions.** Every full-run step has a discarded warmup followed by three
  measured repetitions with the same controls. L1 discards 5 seconds of warmup
  then measures 3 × 5 seconds, or 3 × 10 seconds for a case whose 5-second
  window cannot meet its sample floor (REQ-008); L2 discards 2 minutes of warmup then measures
  3 × 6 minutes. Store each repetition's
  metrics and sample counts once; the record reader derives the median, minimum
  and maximum. Never merge the samples into a percentile and call it the median
  repetition. Warmup contributes no
  measured work, CPU, latency, or sample count. A missing/invalid repetition
  invalidates the step; do not silently select the fastest repetition.
- **REQ-004 — Judgment.** A full run is VALID when it measured what it claims:
  quiet host, clean source, completion within budget, sufficient samples, every
  required measurement and counter present, and every correctness and integrity
  criterion (`Criterion::every`) held in every repetition. Otherwise it is
  INVALID with all reasons. Unmeasured declared steps say `not run` with their
  reasons and make the run INVALID. VALID exits 0; INVALID exits 2; nothing
  exits 1. Aggregates `bench:l1` and `bench:l1:bifrost` preserve individual
  records and exit with the worst of 0/2. Performance targets
  (`Criterion::limit`) use median repetitions and report `met` or `missed`;
  censored or straddling readings that cannot settle a target report
  `unsettled`. A target never changes validity or exits. REQ-008 retains smoke's
  plumbing-only applicability exception.
- **REQ-005 — Complete steps.** Every step declares criteria, including stress,
  overload and recovery. Correctness and integrity require no errors, wrong
  answers or verdicts, lost, dropped, unstaged or misattributed evidence, or
  undeclared refusals or shedding in any full-run repetition. Overload must
  actually be exercised, expected rejections and shedding declared and exactly
  accounted, and queues drained. Expected server rejection differs from a
  driver that failed to issue arrivals. Any violation makes the run INVALID.
  `backlog-bounded` and `bytes-bounded` are performance targets, not integrity;
  their misses do not invalidate. Missing counters, unreconciled loss or a
  failure to drain do invalidate.

Performance targets use median repetitions; correctness and integrity hold in
every full-run repetition. Smoke enforces measurement and evidence validity
under REQ-008 without judging measured production outcomes. Spread never hides
a target miss. A runner failure retains partial INVALID evidence and marks
unmeasured steps explicitly; it never fabricates zero metrics.

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
unavailable and the required measurement untrusted.

Use `sysinfo`, already a workspace dependency, for process CPU time and sampled
peak resident memory. The quiet-host check and driver CPU context use the same
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
unreconciled population makes the affected measurement untrusted; never clamp.
Zero-duration observations must remain represented and counted explicitly.

Verification `direct-verify` judges every repetition by `lost_writes`, the
growth of `outbox_events_lost_total{outbox="scribe"}` from before its window
until after its drain; any loss makes the full run INVALID.

Verification `direct-verify` also records, per repetition and from the same
HDR timing, the maximum overhead, the count of executions whose overhead
exceeded 0.5, 1, 5 and 10 ms (bounded by the histogram bucket that straddles
each threshold), and the p99 and maximum of each execution phase (`admit`,
`load`, `prepare`, `engine`, `stage`, and `outside` every phase). Each phase is
recorded under the same lock as its request. Phase readings are diagnostic:
phases overlap, are never summed, and never judge or invalidate a run.

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
CPU windows, drain work and drain duration; incomplete cohorts are untrusted.
The snapshot mechanism must prove concurrent recording does not lose or count
an observation twice. Stage timings explain latency; they are not summed when
overlapping. An unavailable optional phase is labelled, never fabricated.

Absolute performance targets use histogram quantization bounds: `met` when the
entire percentile interval satisfies the comparison, `missed` when it wholly
violates it, and `unsettled` when it straddles the target. Censored readings
that cannot settle a target also report `unsettled`. These outcomes never
invalidate measurements or change exits; missing or corrupt required timing
evidence still does.
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
never substitute for server-latency metrics or determine validity. Client
p50/p95/p99 are reported beside server latency, separately labelled. Visibility diagnostics
retain write acknowledgment, first correct query result and probe cadence.
Retain scheduled/actual send times only to check driver send lateness and missed
arrivals. Remove scheduled-time-to-completion latency judgments and supporting
collection used only for those judgments. Server backlog and timed drain/recovery bounds below are performance targets
against server-side state and phase boundaries. Actual queue drain and recovery
phase completion remain integrity obligations; a timed target miss alone does
not invalidate. These are not client-observed request latency SLOs.

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
components are untrusted, not zero. Existing timer calculations stay with their
engine owners. Async span busy time and ordinary span timestamps are not a
substitute. Preserve counters for expected timing components and outcomes so
histogram counts can be reconciled without retaining per-request records.
Mixed-workload efficiency retains total server CPU and work by class; it does
not pretend to attribute isolated CPU to each class. Retire `wyrd.timing`
span emission/collection once no consumer needs it, preserving ordinary behavior
and diagnostic spans and their required client-visible catalog.

### Levels and commands

**REQ-008 — Level 1, fast.** Verification `direct-verify`, Gateway `overhead`,
and Bifrost `hot` and `parquet` each complete within 300 seconds. Bifrost
`writes-parquet` completes within 360 seconds. `bench:l1:bifrost` has a
960-second (16-minute) upper bound across its three experiments.
L1 measures 1 and 64 clients only on every engine. Measured windows are
3 × 5 seconds after a 5-second warmup; a case keeps 3 × 10 seconds only when
5 seconds cannot meet its declared sample floor, and the record names which
cases did. The 20 Bifrost cases need about 7 minutes of warmup/repetition
windows in total, split across their three commands. Individual cases
and experiments remain available for fast optimization feedback. Build binaries and prepare external prerequisites before
starting the experiment clock; the clock includes quiet-host checking, server
startup, fixture loading, warmups, repetitions, and drain. An overrun is stopped
and recorded untrusted. Artifact finalization after stopping must still retain
the overrun and actual finish time. No compilation during execution.

Use closed-loop clients (a client sends its next operation after completion),
one fixed fixture configuration per experiment, and isolated operations:

| Engine | Required L1 coverage |
|---|---|
| Bifrost `writes-parquet` | Native Arrow, small OTLP spans, approximately 1 KiB OTLP spans and OTLP logs at 1 and 64 clients: 8 measured cases. Reuse the 2M-row generators and fresh identities. The server uses production default rotation sizes and a 30 s retention; no write experiment overrides rotation. Measure durable acknowledgement, not publication. Include actual staging/publication CPU in Parquet-configured windows, record its activity and backlog, and do not claim storage-maintenance cost if none occurred. |
| Bifrost `hot` | Reads while every row stays in the memtable: the server runs with memtable and on-disk rotation sizes of 16,384 MB and a 24-hour retention, the earlier hot tier's declared settings, so nothing stages or publishes; a staged or published row makes the experiment untrusted. Fixtures: the standard 10M-row events fixture and 1M telemetry spans. Events point lookup at 1 and 64 clients; events small aggregate at 1; span filter on a shredded attribute key at 1. |
| Bifrost `parquet` | Reads after every row is published as Parquet: default rotation sizes and a declared 30 s retention, then one settlement until nothing remains in a memtable, staging or claim. Fixtures: the standard 10M-row events fixture, 1M telemetry spans, and one 200,000-row sample per write path, all settled together. This record holds each write path's storage ratio; both write records report it as inapplicable with that reason. Events point lookup at 1 and 64 clients; events small aggregate and full scan at 1; span lookup by `trace_id`, a declared Bloom column, at 1 and 64; span filters on a shredded attribute key and on a long-tail key below the shredding threshold at 1. Two correctness checks run once after settlement, outside every measured window: the published span files shred exactly the expected attribute keys and none below the threshold, and a span-time window scans fewer files than the whole table. |
| Verification `direct-verify` | Two direct-verify cases at 1 and 64 clients: existing four assertion checks over 2 KiB JSON, alternating expected pass/fail judgments. Expected failing judgments are correct completed verifications, not transport errors. Record elapsed, Verifier wait and server overhead, CPU/verification and scaling. |
| Gateway `overhead` | Six cases: buffered chat with metadata capture, streamed chat with metadata capture, and buffered chat with payload capture, each at 1 and 64 clients. Reuse the local controlled provider, 1 KiB metadata shapes/32 stream events and 64 KiB request/response payload-capture shape. Record header/final overhead where applicable, calls/s, CPU/call, capture bytes and memory. |

L1 measures 1 and 64 clients only: 1 gives unloaded latency and the
cost floor, and 64 is where the SLO applies. Scaling is reported for each query
class measured at both; a class measured at 1 only records scaling as
inapplicable with that reason. The absolute Bifrost L1 latency criterion remains point-lookup p99 at 64
clients. Other cases report their distributions and relative metrics and
judge correctness. The L1 full scan keeps
latency percentiles descriptive and reports throughput/core and CPU/query and judges
correctness; it does not pretend a short single-client scan window supplies
200 tail samples. Non-scan L1 latency reports p99 with the existing
1,000-sample floor. L2 scans retain their judged p95/200-sample requirement. The same case at different
storage states is never averaged into one result. A complete Bifrost L1 suite
is all three records at full default coverage: 12 reads and 8 writes. The
16,384 MB rotation/24-hour retention override exists only for the `hot` read
experiment, whose bounded fixture is loaded once and never grows; an override
that lets an unbounded write workload accumulate in memory is not allowed. The telemetry span fixture is deterministic, with attribute keys
straddling the 10% shredding threshold. The former request-only release preflight
is replaced by the following real-path smoke contract.
A settled Parquet read without required scan facts remains untrusted.

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
capture shedding and missed performance targets. Those observations do not fail smoke. For example, accurately accounting for
10 dropped calls is successful plumbing; losing those 10 calls from the outcome
totals is a reconciliation failure. Smoke is VALID/exit 0 when its plumbing
works and INVALID/exit 2 otherwise; smoke VALID claims plumbing only. Every
required measured population stays nonempty. Full runs retain normal criteria,
sample floors, durations and repetitions, with VALID/0 or INVALID/2.

Separate deterministic cases feed controlled target misses and
integrity or measurement failures through full-mode judgment, record/readback
and report regeneration. Target misses remain visible with
VALID/exit 0; integrity or measurement failures produce INVALID/exit 2. No case
exits 1.
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
meaning when that field is absent. Smoke records never enter history, including when
manually moved. Reports identify smoke
as functional pipeline evidence rather than performance qualification.

Reset or otherwise establish equivalent declared data state between repetitions;
repeated writes must not turn later repetitions into duplicate acknowledgments.
The Parquet query fixture and its fixed size/seed are declared before measuring.
Measured windows follow REQ-003; the 2M-row configuration does not end
measurement early, and writes use fresh identities as needed. Throughput is
reported with no new absolute floors. Growing staging/publication backlog is visible;
a short write-ack rate is not labelled sustained storage throughput. Retain only native
write ≥200k total rows/s and OTLP wire ≥30 MiB/CPU-s as sanity
floors on each respective sweep's peak capacity, the better of its 1- and
64-client steps; record the peak client count.
Only full default coverage counts as an engine's completed L1 suite; a selected
subset records its selection and cannot claim the omitted coverage passed.

**REQ-009 — Level 2, workload.** Each engine's complete workload targets 20–30
minutes with a hard 30-minute budget under the same clock rules; exceeding it
is untrusted. A successful complete workload includes at least 20 minutes of
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
whose search exceeds its declared share of the budget, is untrusted; no
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
1,000-sample p99 requirement; insufficient evidence remains untrusted. Declare
an offered query rate capable of at least 200 scans per repetition (at least
4,000 total queries at the unchanged mix), and check actual completions.
Write-to-query visibility, including the historical p99 ≤5 s target, is
diagnostic-only because client polling cannot establish a server request boundary. Table aggregates retain correctness
and efficiency metrics without a new absolute latency floor.
Backlog may grow by at most one second of offered work over the measured
window; drain completes ≤60 seconds after load stops. After overload ends,
The recovery target is SLOs met again within 30 seconds. Apply these
backlog/drain/recovery targets to corresponding Verification and Gateway L2
phases too. Target misses are reported without invalidation; queues must still
actually drain and the declared recovery phase must complete. The capture `lost` count must be zero always. Explicitly accounted shedding
is allowed only in the deliberate overload phase and is recorded separately
from `lost`; it cannot excuse any lost capture. Outside that phase, shedding
makes the full run INVALID.

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
precedence across VALID/0 and INVALID/2 even if an earlier experiment is INVALID. `bench:l1` runs every L1 experiment sequentially so they do not
contaminate one another.
All old `bench:*` entry points are replaced without aliases.  No L2 aggregate
is required. Every experiment and both levels use identical commands and record
schema on all three platforms. Each engine command accepts optional `--profile`,
and `--smoke`, forwarded through mise (for example,
`mise run bench:l1:bifrost -- --smoke` or `--profile`);
`bench:l1` forwards it to each sequential engine run. There is no platform-only
experiment or alternative command. `benchmarks/README.md` explains levels, budgets, prerequisites,
validity, metrics/units, history eligibility, exits, examples, and how to read
a record, with a pointer from `TESTING.md`. Commands and documentation above
are implementation obligations.

### Self-contained records

**REQ-011 — A record stands on its own.** A record holds every repetition's
measurements, the host, the build, the commit and the controls, and its report
derives every answer from that record alone: absolute values and the relative
metrics normalized to the run's own resources (per server core, per provisioned
vCPU, per second, per row or query). Reading or reporting a record never needs
another record, a reference file or an earlier run. `baselines.toml`,
`--qualify-baseline`, `capacity reference` and every regression criterion are
removed. Comparing two records is a separate convenience outside this
specification and never affects a run's validity, exit or report.

**REQ-012 — Spread.** Each metric reports its median, minimum, maximum and
spread over the three repetitions, so a reader can see a run's own noise.
Repeat a suspect experiment under matching controls before attributing a code
change; three repetitions are mandatory.

### Record and report contract

**REQ-013 — One persisted record.** The shared recorder writes each complete,
clean, unprofiled full run of the whole plan, VALID or INVALID, to the
following history. A run with `--steps`, `--profile`, `--allow-dirty` or
`--smoke` writes the same record under `target/benchmarks/` and never enters
history:

```text
benchmarks/history/<engine>/<experiment>/<UTC timestamp>-<short sha>.json
```

Engines are exactly `bifrost`, `verification`, `gateway`. Experiment identifiers
are stable lowercase path-safe names. Filenames are
`YYYYMMDDTHHMMSSZ-<9-char sha>.json`, using UTC and the first nine commit hex
digits; the full SHA is authoritative in JSON. Never overwrite a run: append
`-2`, then `-3`, etc. before `.json` on collision, retaining the actual start.
History is the `benchmark-records` branch. It shares no commits with the
code and holds only `<engine>/<experiment>/` records and a README. Each
checkout mounts it as a worktree at `benchmarks/history/`, which the code
branches ignore (`git worktree add benchmarks/history benchmark-records`).
Records are committed and pushed on that branch through normal contributor
review, never in a code change; running a benchmark never stages or commits
automatically. Without the worktree, the recorder creates the directory and
writes the record there all the same; it is simply not yet kept. Full histogram distributions, diagnostic traces, logs and profiles remain
under `target/` with optional record references, not embedded in the summary.

Schema version 3 is the persisted contract. Readers reject every other version;
earlier records are deleted, not read. The following field names
and meanings are proposed; nested metric/criterion entries carry
stable identifiers and explicit units rather than formatted table strings.

| Required JSON field | Meaning |
|---|---|
| `schema_version` | Integer 3. Breaking semantic/shape changes require a new version; readers reject unsupported versions explicitly. |
| `engine`, `experiment`, `level`, `question` | Declared identity, level integer 1/2, and question. |
| `mode` | `full` or `smoke` for new records; absent on older records retains their original full-run meaning. Smoke never enters history, independently of filename/location. |
| `started_at`, `finished_at` | UTC RFC 3339 instants, including partial/untrusted runs. Elapsed budgeting uses monotonic time, also stored. |
| `commit`, `dirty`, `dirty_override` | Full measured-source SHA, source dirtiness at declaration, explicit override state. Dirty runs store `dirty: true` and `diff_sha256`, SHA-256 of the bytes from `git diff HEAD`. Output created by this run does not retroactively dirty its source. |
| `host` | OS/version, architecture, CPU model/logical count, physical memory, and declared storage context; record available provisioned vCPUs P per server host and combined scope separately from mean used cores C/T. P supplies throughput per provisioned vCPU; C supplies work/CPU-s. |
| `build` | Profile, feature/compiler flags, compiler and binary identities; detect a stale binary rather than attribute it to the current tree. Existing build stale-binary detection applies; no additional source-to-binary correspondence validity rule is required. The diff digest does not make a dirty run history-eligible. |
| `command` | Exact invocation as argument values plus the non-secret configuration needed to reproduce it. |
| `variables`, `controls`, `criteria` | Sweep and fixed controls, validity/budget declarations, metric definitions (including latency clock/resolution, histogram method/range/precision, quantization interval, population and expected/recorded counts) and criterion IDs/operators/limits/units/applicability. |
| `steps` | Ordered declared steps: IDs/params, timings, status (`measured`, or `not run` with its reason), and each repetition's measurements, sample counts and measurement problems (three in full mode, one in smoke mode). Each value is stored once; no summary, observed value, verdict or reason is stored. Missing observations are null with reasons, never NaN or infinity. |
| `validity` | Host samples and other check observations, stored once. The record reader derives every verdict and reason for both human and agent readers. |
| `artifacts` | Optional local `target/` references and capture completeness; historical summaries remain readable without them. |

Recording failure cannot produce a successful result: exit 2 with an explicit
persistence error and any salvageable local evidence. Complete records are
published by writing a temporary file then renaming to the collision-safe destination. A caught interruption retains partial untrusted evidence;
an abrupt kill/power loss cannot guarantee a final record and must not leave
a partial JSON document masquerading as complete evidence.

**REQ-014 — One report.** One record reader renders every report from the
persisted record; the console, the regenerated report and the HTML page show
the same thing. A report fits on one screen, in this order:

1. The experiment, VALID or INVALID, and why.
2. The question, observed values beside targets with `met`/`missed`/`unsettled`,
   correctness and integrity, and the run's relative metrics.
3. Source commit, build, host, fixture and method, with a plain definition of
   each judged metric and of a client.
4. One row per step: the judged metric, throughput and server CPU per
   operation, each the median of the repetitions with [min–max], 3 significant
   digits and units. Driver CPU is one line of context. A step that did not run
   says `not run` and why.

Repetitions, CPU normalizations, histogram bounds, per-phase timings and tail
counts sit under details. Display rounding never changes stored values or
judgment. Client timing diagnostics never stand in for judged latency.

`capacity results` renders every record under `benchmarks/` into one
self-contained HTML page under `target/`, read from the records alone. For
each experiment it charts the judged metric of each step across runs in
chronological order against its limit, and opens each run's report.

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
profiling with a recorded summary/report note; it never makes a run
INVALID and cannot waive required measurements. The runner never changes OS
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
−10% throughput/+15% p95/p99 are not reactivated. The new declared criteria own judgment. Point-lookup
<5 ms at 64 clients replaces selective targets at 1 client, including the later
≤1 ms target. Gateway's historical MIN_CALLS 100→30 is not silently adopted as
p99 sufficiency. Capture shedding is counted separately and allowed only in deliberate overload; any nonzero `lost` count makes a full run INVALID.
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
then measure the improvement under the normal configuration. Diagnostic
runs record their configuration, export loss and profile availability and never
enter history. Document temporary detailed capture on a selected server
instance, restoring normal settings afterward; no new dynamic capture control
plane or client-forced operator sampling is introduced. Client spans appear in
Wyrd's timeline only when the client exporter also sends them to Wyrd. Preserve
Rust/Python/TypeScript timeline behavior and tenant-negative coverage.

**REQ-021 — Measured engine optimization.** Declared performance targets stay
as written. TASK-005 missed four on the reference host. This change investigates
and measures improvements to these costs; meeting targets is not a condition of
validity or completion:

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
under the normal configuration.  Each fix is measured before and after under the normal configuration. Revision 19 orders remaining work and parks cross-query plan
reuse unless profiling shows planning cost.

**REQ-020 — Export overhead and discoverability.** Measure the same clean build with operator endpoint absent versus ordinary
1% export. Use identical workload/fixtures and record the configurations; report the observed CPU,
latency and throughput difference without subtracting it. Run each engine's
full L1 separately and report after each. Prove injected/controlled target misses in latency, throughput/core and CPU/work
are reported without invalidating runs or weakening targets. Run full L2 once on the integrated candidate;
L2 evidence may be collected after the L1 work, in its own task.
README explains matrix, quantities, startup/prerequisites, runtime budgets,
partial selections, comparison with earlier records, diagnosis and recovery. L1 remains
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
| INV-005 | Declared controls/criteria cannot change after observing performance. Invalid measurements never enter history. Targets never change validity or exits; integrity holds in every repetition. |
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
2. **Persistent evidence:** versioned JSON schema v3, exact three-engine directory
   layout, timestamp/SHA identity, committed summaries and external raw artifacts.
3. **Levels:** L1 experiments ≤300 seconds except writes-parquet ≤360 seconds,
   at 1 and 64 clients; Bifrost aggregate ≤960 seconds;
   Bifrost L1 as three independently runnable experiments; L2 20–30 minutes
   per engine with contended in-run ingest calibration;
   three repetitions per step, defined clock boundaries, opt-in commands
   replacing old tasks without aliases.
4. **Metric semantics:** both provisioned-vCPU and consumed-CPU denominators, explicit byte units,
   server-only judged latency,
   bounded high-resolution histograms, independent diagnostic tracing,
   process scope, spread, fixed-client SLOs and portable readings.
5. **Self-contained records:** each record and report stands alone with its
   relative metrics; comparing records is separate and outside this specification.
6. **Portability:** identical experiments, commands and schema on all three
   platforms; plain-child servers, sysinfo readings and optional samply
   diagnostics, without OS resource envelopes. Required real evidence is Linux
   x86_64 only; the other two platforms require unchanged runnable behavior.
7. **Validity:** VALID exits 0; INVALID exits 2; nothing exits 1. Integrity
   holds in every full-run repetition, including overload and recovery. Targets
   are separate. Smoke checks plumbing with production criteria
   explicitly inapplicable.

8. **Smoke:** the actual experiment workflow with reduced declared controls,
   run only on request, explicit criterion applicability and local-only,
   permanently history-ineligible records. A separate probe runner is rejected
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
| AC-001 — Engineer runs one engine's L1 | Each engine command is exercised separately, and each Bifrost experiment (`writes-parquet`, `hot`, `parquet`) within its declared 300- or 360-second budget, without running a smoke first; `hot` reads show no staged or published rows and `parquet` reads show nothing left in a memtable, staging or claim. Within its budget the full matrix runs with discarded warmup and three repetitions. Engineer reads question, controls, units, per-step criteria and verdict; opens the printed JSON path and regenerates an identical report. Real Linux x86_64 release-process journey, record and report comparison. Recompute server latency bounds from retained interval histograms; changing client network delay, polling cadence or scheduled-time latency cannot change server latency judgment (driver send lateness still checks validity); verify exact values just below, equal to and above strict SLO limits, and reject missing/overflowed timing evidence and report ambiguous target bounds as unsettled without invalidation; coarse monitoring buckets cannot judge latency. Operator export disabled or lossy cannot erase complete benchmark timing. | REQ-001, REQ-003, REQ-007, REQ-008, REQ-010, REQ-013, REQ-014 |
| AC-002 — Engineer reads a run on its own | One VALID full record, read with no other record, file or run present: its report shows every metric's median, minimum, maximum and spread, absolute and per server core, per provisioned vCPU, per second and per row or query. No report, exit or validity reads another record. | REQ-011, REQ-012 |
| AC-003 — Engineer encounters noisy results | Repetitions 70/85/100: the report shows median 85 and spread 30; an absolute target miss reports missed and retains VALID/exit 0. Censored or straddling target readings report unsettled without invalidation. Integrity failure in any repetition gives INVALID/exit 2 regardless of median or spread. | REQ-004, REQ-012 |
| AC-004 — Engineer cannot obtain valid evidence | Independently exercise busy preflight/post-run host (10-second samples, >10% non-benchmark CPU), any missed arrival or p99 send lateness >5 ms, insufficient samples (<1,000 p99 / <200 p95), dirty source with or without override, missing/reset CPU and budget overrun: untrusted, exit 2, reasons and partial record, no fabricated successful step. Declared dirty override is visible. Combine an invalid step and target miss: overall INVALID retains both. Dirty overrides remain INVALID. Injected resource/clock observations and runner integration; no synthetic host load. | REQ-002, REQ-004, REQ-006, INV-005, INV-007 |
| AC-005 — Engineer runs and compares supported platforms | Prove real CPU accounting and every L1 and L2 experiment for all three engines on Linux x86_64. Inspect portability of the same commands, schema v3 and measurement paths on macOS arm64 and Linux arm64; real runs there are optional and missing runs do not block completion. Recompute (W/T)/P, W/C and C/W from records, with distinct labels and recorded P; results agree and never substitute removed quotas. Verify write rows and uncompressed MiB in both normalizations and query scan rows/bytes per query alongside server time. Each record carries its OS/architecture fingerprint. Exercise `--profile` on Linux x86_64 and missing/unusable profiler cases: capture or an explicit note is recorded without affecting judgment; required accounting stays mandatory. Real release-process journeys/records plus deterministic arithmetic, provenance and diagnostic-failure checks. Static command/source review confirms both levels and all experiments lack OS-specific paths and resource envelopes. | REQ-006, REQ-010, REQ-011, REQ-013, REQ-015, REQ-016, INV-004, INV-009 |
| AC-006 — Agent reads history without prose parsing | Agent discovers engine/experiment records, reads schema v3 fields, reads numeric values/units, repetitions and the reader-derived typed verdict/reasons, and identifies omitted/invalid steps. Verdicts and reasons are not duplicated in persisted fields. Unsupported version refuses interpretation. Report regeneration works without target artifacts; same-second/SHA collisions use -2/-3 and cannot overwrite history; temp-write/rename failure produces INVALID/exit 2. A dirty override records the SHA-256 of `git diff HEAD`. Record-reader/writer integration and malformed/unsupported input cases. | REQ-013, REQ-014, INV-008 |
| AC-007 — Engineer runs L2 with competing work | For each engine, observe real overlapping foreground/background work during each repetition, offered versus achieved arrivals, concurrently evaluated SLOs, bounded backlog and drain. Bifrost records its contended calibration steps, selected sustainable rate, 70% load and 32 query clients; Verification includes evidence writes; Gateway overload/recovery judges capture accounting. Concurrent integrity failure invalidates despite isolated success; backlog and byte bounds are performance targets reported separately. Real workload journeys, timelines and records. | REQ-005, REQ-009 |
| AC-008 — Contributor adds an experiment | Contributor declares question/variables/controls/metrics/criteria and supplies workload behavior to the existing shared experiment owner. The same validity, repetitions, report, record and verdict behavior applies without another measurement/report implementation. One small representative integration experiment and source review demonstrate reuse. | REQ-015 |
| AC-009 — Maintainer checks migration and discoverability | README and TESTING explain invocation and records; coverage mapping has no lost workload; old tasks/aliases, dead thresholds/SloGate/load exports and duplicate code are gone. Every client-queue step judges criteria; Gateway has declared capacity SLO constants. Static consumer/task/docs inventory plus relevant existing correctness lanes; no benchmark added to CI/gates. | REQ-010, REQ-016 |
| AC-010 — Operator checks preserved production guarantees | Wrong results and unexpected loss make full runs INVALID; tenant-negative journeys cannot expose data; acknowledged writes survive existing durability checks; audit stays non-blocking and accounted; fixture cleanup remains scoped and credentials never enter summaries. Existing security/storage journeys and focused record privacy/cancellation checks are credible evidence. | INV-001, INV-002, INV-003, INV-006 |
| AC-011 — Operator changes logging/sampling or loses its backend | Production-initialized tests prove independent log/operator policies and SDK overflow/shutdown loss reporting and accurate failed-export-batch counters; real authenticated client → gateway → tenant timeline journeys retain catalog IDs/links at ratio zero/no endpoint, with tenant-negative readback. All shipped language surfaces retain their existing journeys at integrated review. | REQ-017, REQ-018, INV-010 |
| AC-012 — Engineer smoke-tests the actual benchmark without trace export | Real release commands exercise every default L1/L2 plan for all engines, including all three Bifrost L1 experiments and both span sizes, through actual loading/publication, selected steps/phases, measured stop/drain, exact outcome reconciliation, record/full timing evidence and report. Read the record and regenerate its report; verify retained histograms reproduce recorded readings. Accurately recorded production errors, dropped/refused calls, capture shedding, wrong answers and target misses do not fail smoke. Missing measurements/counters, mismatched populations, zero CPU, missing applicable scan facts and failed evidence/report handling are plumbing failures. Production criteria remain explicitly inapplicable with observed values retained; smoke VALID claims plumbing only. Separate deterministic full-mode cases prove target misses survive judgment, record/readback and report with VALID/exit 0; integrity or measurement failures give INVALID/exit 2 and nothing exits 1. Selected-step proof remains; a full run runs no smoke. Mode round-trips, output stays outside history and smoke cannot enter history after relocation. Deterministic interval/quantization/overflow/reset/drain proof and bounded known-delay, known-CPU and instrumentation-cost calibration use the actual measurement path with independent references and justified tolerances, without synthetic host load or a side runner. Normal readout absence, non-loopback refusal and snapshot privacy remain. | REQ-007, REQ-008, REQ-013, REQ-015, INV-006, INV-010 |
| AC-014 — Maintainer checks the engine limits | Clean full L1 reports met, missed or unsettled for Verification 64-client latency, Bifrost hot and Parquet point-lookup p99 <5 ms at 64 clients, the OTLP spans ≥30 MiB/CPU-s target and Gateway <500 µs overhead at 64 clients. Targets stay unchanged, results exact and tenancy/durability journeys green; target misses do not block completion. Before/after records and the diagnostic run that located each cost. | REQ-021, INV-001, INV-002 |
| AC-013 — Engineer reads relative metrics | Every write record reports wire MiB/s and uncompressed MiB/s per server core, rows/s per server core and CPU per row; every read record reports queries/s per server core and CPU per query; each from that record alone. The same-build export comparison quantifies overhead; diagnostic traces and profiles never enter history. | REQ-011, REQ-012, REQ-019, REQ-020 |

Implementation planning must select exact focused commands and evidence under
these decisions; a proposed task is not a claim that its tests already pass.

## Revision 19 supplement

The maintainer approved this execution scope on 2026-10-27: a benchmark that
runs and collects correct metrics is successful; benchmarks are outside CI and
do not gate performance. The numbered order below is the execution order.
Items 1–5 make every run's own record trustworthy and complete before
optimization. The maintainer approved items 1, 3 and 10 explicitly; item 10's
design is shown to the maintainer before its code. Existing
requirements own changed behavior; new IDs below cover additional outcomes.

### 1. Valid or invalid only

**Intent and evidence.** A slower result is useful evidence, not a broken
benchmark. The maintainer explicitly approved successful runs that collect
correct metrics and rejected performance gating because benchmarks are not CI.

**Requirement.** Apply REQ-004, REQ-005, REQ-008, REQ-011 and REQ-012 in every
reader, report and command. VALID/0 requires trustworthy complete measurement
and every full-run correctness and integrity criterion in every repetition;
otherwise report INVALID/2 with reasons. Nothing exits 1. Median performance
targets remain per-metric observations, including `unsettled`, never exit
conditions. `backlog-bounded` and
`bytes-bounded` are targets. Update `mise.toml` descriptions and
`benchmarks/README.md`; remove the claim “fails (exit 1) on a declared criterion”.

**Acceptance.** AC-002, AC-003 and AC-012 show that a missed target and an
unsettled target each leave the run VALID with exit 0, and that each integrity
or measurement failure gives INVALID/2. Both L1 aggregates
return the worst of 0/2 and retain the individual results. Reports and task
and README wording agree. No benchmark becomes a CI or verification gate.

**Order/dependency.** First; items 2–4 follow.

### 2. Writes budget

**Intent and evidence.** Two clean writes-parquet runs took 311.3 and 311.2
seconds on commits f96c23fa2 and d86cf238e against 300 seconds. Both are always
INVALID under that budget, and native-write-c64 loses a repetition.

**Requirement.** REQ-008 gives writes-parquet 420 seconds, enough for the
current 311 seconds plus item 4's two added steps, retains 300 seconds for hot
and parquet, and updates the Bifrost L1 aggregate maximum to 1,020 seconds. Keep the same clock boundaries and full coverage; do not shorten or
omit repetitions to fit.

**Acceptance.** A complete writes-parquet record contains every repetition,
including native-write-c64, and reports its actual elapsed time against 420
seconds. The aggregate's stated maximum agrees with its component budgets.
An actual overrun still invalidates rather than truncating evidence silently.

**Order/dependency.** After item 1; before item 4.

### 3. History location, and profiled runs never enter it

**Intent and evidence.** Profiled writes-parquet records
`20261027T134553Z-f96c23fa2.json` and `20261027T140216Z-f96c23fa2.json`, and
matching profiled hot and parquet records from those runs, entered history
contrary to its complete, clean, unprofiled rule; they are already deleted.
Records committed on the code branch also made code review carry about 264,000
lines of run data, so history now lives on the `benchmark-records` branch. That
branch already holds every earlier eligible record, and the code branch no
longer tracks any.

**Requirement.** Enforce REQ-013. The recorder writes history only under
`benchmarks/history/<engine>/<experiment>/`, and every reader of history uses
that same directory: the `capacity results` page, the L2 calibration that reads
the newest compatible `writes-parquet` record, and their tests. No reader
falls back to the old `benchmarks/<engine>/<experiment>/` location. The
recorder writes profiled runs only under `target/benchmarks/`, regardless of
validity or completeness. `benchmarks/README.md` explains the
`benchmark-records` worktree and how to keep a record. This is correction of
ineligible history, not permission to delete slow results.

**Acceptance.** An eligible full run writes its record under
`benchmarks/history/`, and `capacity results` and L2 calibration read it from
there. A profiled full execution creates no history record. Ordinary eligible
full runs still enter history regardless of target misses. History on
`benchmark-records` contains none of the identified profiled records.

**Order/dependency.** After item 2.

### 4. Realistic span fixture

**Intent and evidence.** Current OTLP spans are about 339 wire bytes each,
against 1,253 for logs. Per-row server costs are similar: about 48k spans/s per
server core against 36k log records/s per core. At 64 clients this reads as
15.5 wire MiB/s per core for spans against 43.3 for logs. Payload size must be
visible before interpreting that throughput difference.

**REQ-023 — Representative span coverage.** Keep the small-span steps and add
a second deterministic OTLP span fixture of about 1 KiB of wire per span,
including resource attributes, a dozen span attributes and events. Measure it
beside small spans in writes-parquet at 1 and 64 clients. Both sizes report
actual wire bytes per span, rows/s per server core and wire MiB/s per server
core with explicit denominators. Keep fixture identities distinct so a comparison
never pairs different payloads.

**Acceptance.** The full record and report distinguish small and approximately
1 KiB spans at both client counts, preserve exact results and report both row
and byte throughput from measured wire sizes. Smoke exercises both fixtures.
The expanded matrix completes within the item 2 budget.

**Order/dependency.** After item 3; before item 5.

### 5. Self-contained records

**Intent and evidence.** Every record already stores its measurements, host,
build, commit and controls, and its report already prints relative metrics
such as 43.3 wire MiB/s per server core for logs. Yet every run reports "no
reviewed reference" because REQ-011 gated reports on a hand-reviewed
`baselines.toml` that was never filled.

**Requirement.** REQ-011 and REQ-012 as amended. Remove `baselines.toml`,
`--qualify-baseline`, `capacity reference` and every regression criterion and
the code that reads them. The headline relative metrics are wire MiB/s per
server core, uncompressed MiB/s per server core, rows/s per server core, CPU
per row or query, and server and client p50/p95/p99.

**Acceptance.** AC-002 and AC-013. The next VALID run of each experiment reports
no reference-related reason and is complete on its own.

**Order/dependency.** After items 1–4; before items 6–12.

### 6. Per-phase write timing

**Intent and evidence.** At one client a write costs 11–14 ms server-side:
native 14.2 ms p50, OTLP spans 10.8 ms, logs 12.1 ms. Current timing exposes
one `bifrost.gate.write` or `bifrost.otlp.*` span and cannot locate that cost.

**REQ-024 — Write-phase evidence.** Record decode, validation, WAL append,
WAL sync and acknowledgement timing, plus shard wait under concurrency, from
the same server HDR timing used by Verification. Include phase populations and
boundaries so shared work, waiting and overlapping phases are explicit; do not
sum overlapping timings as an end-to-end latency. Retain the phase timing in
the record and make it readable without external profile artifacts.

**Acceptance.** Native, span and log records at 1 and 64 clients expose the
applicable phase distributions with correct populations, labelled unavailable
phases and no fabricated samples. Controlled timing and record/readback proof
exercise the real collector and report. End-to-end timing remains unchanged.

**Order/dependency.** After item 5; before the tail diagnosis in
item 9. Use it to explain subsequent write changes.

### 7. Span decode CPU

**Intent and evidence.** Candidate costs are repeated parsing and validation.
Prior estimates, not measurements, assign 10–13% of span CPU to message passes,
and about 8% to repeated Variant validation. The Gate's logical identity is
already reused by Scribe preprocessing when known, yet SHA-256 still takes about
1.8% of write CPU in the 2026-10-27 profile from an unidentified caller.

**REQ-025 — Measured write CPU reduction.** Measure these changes individually
before and after each change, in the following order: (a) a single-byte fast path
in `WireFields::read_varint` in
`crates/wyrd/wyrd-server/src/otlp_decode.rs`; (b) avoid fully re-validating
server-built Variants in `finish_variant` in
`crates/vala/vala-bifrost-redux/src/tables/signal.rs`, which currently passes
them through `EncodedVariant::from_bytes`, while preserving size and depth
limits exactly; (c) use two passes per OTLP message instead of 7–10, preserving
exact capacity reservation, last-wins semantics and every malformed-input
check; (d) attribute the remaining SHA-256 cost to its caller from the profile
and remove it only if it repeats a digest already computed for the same bytes.
Keep external validation and identity semantics unchanged. Item 8 extends this
same measured CPU-reduction requirement; all memory accounting remains exact.

**Acceptance.** Each change has its own comparable before/after result for
CPU per row, row and wire throughput and latency, with correctness proof for
its affected malformed, boundary and duplicate-field behavior. No estimate is
presented as measured improvement; no combined result hides an individual cost.

**Order/dependency.** After item 6; implement and measure (a)–(d) one at a time.

### 8. Write CPU from the 2026-10-27 profile

**Intent and evidence.** The writes-parquet profile on f96c23fa2 attributes
4.7% inclusive to `scribe::memory::unshared_arrow_bytes`, which calls
`Array::to_data` per array per batch; 1.1% to the per-enqueue gauge lookup in
`scribe::execution_lanes::record_lane_enqueued`; 0.8% to `mkdir` under
`encode_runs`; and 3.9% of server CPU to rayon worker idle spin (`__sched_yield` under
`wait_until_cold`).
Idle CPU directly lowers every per-core result.

**Requirement.** Under REQ-025, reduce these four costs individually without
changing memory-accounting values, ownership, lifetime or admission behavior.
Keep WAL, staging, filesystem error handling and worker progress guarantees.

**Acceptance.** Each change has a separate comparable measurement and profile
showing its cost, plus focused proof that retained/shared/released memory is
still charged exactly and queues still progress and drain. Report observed
changes, including no improvement; percentages above are diagnostic evidence,
not promised savings.

**Order/dependency.** After item 7, one change and measurement at a time.

### 9. 64-client write tail

**Intent and evidence.** On d86cf238e, client p99 at 64 clients is 170 ms for
spans, 289 ms for logs and 615 ms for native writes.

**REQ-026 — Diagnose write tails before changing code.** Use item 6 phase
timing to locate the cause of each tail, separating client duration, shard
wait and server work. Choose a change only after the timing supports its cause;
preserve durability, ordering and exact accounting.

**Acceptance.** Retain phase evidence explaining each affected path and a
causal diagnosis before a fix. Measure any resulting change separately, before and after, showing client/server tails and CPU rather than hiding queued work.

**Order/dependency.** After item 8; depends on item 6 timing.

### 10. Fix 4: memtable Variant shredding

**Intent and evidence.** The shredded-attribute span filter takes 50.4 ms client
p99 from the memtable (hot), against 17.7 ms from Parquet.

**REQ-027 — Design approval before memtable shredding.** Produce a separate
design section here and obtain maintainer approval before implementation. The
design must state what is shredded, when, how memory is charged, how reads use
it, and equivalence with Parquet shredding. The maintainer approved this
item on 2026-10-27; the design is written here and shown to the maintainer
before code changes.

**Design questions.** Current authority in
`architecture/bifrost-design.md`, Storage format and Variant, requires the
unshredded metadata/value Arrow representation at the wire. Parquet uses
metadata, residual value and typed leaves, with adaptive per-file layout;
recovery-stage runs stay unshredded. Oracle uses the existing DataFusion Variant
semantics, not a Wyrd-specific Variant function or planner. Current Scribe
`MemtableBucket` retains Arrow batches and a shared `RetainedArrowCharge`;
Oracle `MemtableSource` reads those captured batches and keeps their accounting
owners alive. These facts do not settle a live shredded representation or its
construction policy. The design must answer:

- Which Variant columns and literal paths get typed leaves, how keys and types
  are selected, and how residual, missing, null, nested, list and mixed-type
  values remain exact. Does the layout reuse Parquet's selection policy or
  require an architecture amendment?
- When construction occurs for active appends and immutable generations, how
  concurrent captures see a coherent layout, and whether work adds to write
  acknowledgement or query latency.
- Which owner charges original Arrow buffers, typed leaves, residuals,
  metadata and construction scratch, how shared captures avoid double charges,
  and when retirement releases memory under existing admission limits.
- How local and remote live reads use the same DataFusion semantics while
  preserving the canonical wire schema, projection by field id, fallback to
  residual values and whole-column results. Can existing owners provide this
  without a new planner, adapter or persisted format?
- How hot, staged and published reads prove equal types and values, including
  mixed layouts, malformed boundaries and tenant-negative cases, and how
  ingest CPU/memory costs and filter-tail benefits will be measured.

**Acceptance.** The design resolves these questions with source and authority
evidence, states any required owning architecture amendments, and is shown to
the maintainer before code changes. Subsequent implementation proves exact
hot/Parquet equivalence and accounting, then measures the filter and write cost.

**Order/dependency.** After item 9. The written design is a dependency for
this implementation only.

### 11. Parquet tails at 64 clients

**Intent and evidence.** Parquet point lookups have 16.1 ms client p99 and span
trace reads 26.1 ms at 64 clients against the 5 ms goal. Keep client diagnostics
and the server-latency goal distinctly labelled.

**REQ-028 — Profile-led Parquet tail work.** Analyse the captured profile of
the 2026-10-27 parquet run before choosing a fix. Identify the work or waiting
that explains the tail; preserve read correctness, tenant scope, source cuts
and memory limits. A missing or inconclusive profile is a stated evidence gap,
not permission to infer a cause from the p99 alone.

**Acceptance.** Record the profile's supported causal explanation before a
chosen change, then measure that change before and after with separate
client/server p99 and CPU/query. Targets stay unchanged and remain advisory.

**Order/dependency.** Independent of items 1–10; the profile analysis may run
at any time. This analysis decides whether item 12 can reopen.

### 12. Cross-query plan reuse is parked

**Intent and evidence.** Planning is no longer a hotspot after per-query
planning; cross-query reuse would add complexity without measured need.

**Requirement.** Under REQ-021 and REQ-028, park the remainder of fix 5.
Reopen cross-query plan reuse only if item 11's profile shows planning as a
material cost. A performance target miss alone does not reopen it.

**Acceptance.** No cross-query reuse is implemented without that profile
evidence and a stated resulting scope. If planning is absent, record that the
work remains parked.

**Order/dependency.** Last; conditional on item 11's profile analysis.

### Already landed and measured context

These are context, not new work: gRPC listeners disable Nagle in d86cf238e;
one-client p50 fell from 42.0 to 16.0 ms for native and 44.0 to 13.4 ms for
logs, and one-client write throughput rose 1.6–3.1×. The Parquet byte-range
cache landed in f96c23fa2, path-only Variant validation in 432d186bb, and
Kafka-style Scribe lane linger with a byte-bounded outbox in edc1f2eac. Preserve
these changes and measure subsequent work separately.

## Delivery and supersession

The maintainer requested this revision and task packet after aligning on the
proposal. The proposed dependency order is:

1. TASK-003: production tracing (tasks/TASK-003-production-tracing.md).
2. TASK-004: independent server timing (tasks/TASK-004-server-benchmark-timing.md).
3. TASK-005: complete regression suite (tasks/TASK-005-regression-matrix.md).

Revision 11 adds one revision task after TASK-005. It drops `writes-hot`,
applies the L1 turnaround and smoke reuse changes,
makes the REQ-021 optimizations, and runs only L1: the limit checks and the
telemetry comparison. The contended L2
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
TASK-004-R2 remediation (review/TASK-004-R2/TASK-004-R2-finish-stop-counters-and-otlp-cohorts.md).
That direction is separate from the four independently validated residual
findings. The current smoke contract supersedes TASK-004/005
instructions for request-only smoke, skipping fixture/publication work, accepting
probe-only evidence. Revision 15 removes the automatic prepass revision 9
introduced. Their
historical evidence remains intact. TASK-005 full-run
qualification obligations remain; a small smoke VALID cannot close them.

The maintainer subsequently clarified smoke's verdict and withdrew
FIND-TASK-004-7 as written. Revision 10 supersedes any task/review instruction
to fail smoke for accurately recorded production errors, loss, wrong answers or
criterion misses. The revised TASK-004-R3 retains F8 calibration and separately
proves normal full-mode judging with controlled results. Historical review
reports remain evidence under their recorded authority, not current instructions.

## Open material decisions

Revision 19 execution is approved, including items 1, 3 and 10 explicitly.
Item 10's design is written in its section and shown to the maintainer before
its code. Cross-query plan reuse remains parked under item 12.

No material decision remained open in revision 11; the maintainer decided all
five items on 2026-10-26. Revision 8's approval and the directed
real-path smoke workflow persist; the maintainer explicitly approved revision
10's plumbing-only verdict and separate controlled judgment proof. Private
scaling choices remain implementation-owned.
Historical clean-source gaps remain recorded as exclusions/advisory evidence,
not a reason to weaken coverage.

## Revision history and authority

- Revision 19 — 2026-10-27: Approved validity-only judgment, self-contained
  records with their own relative metrics (comparison between records is a
  separate convenience; references and regression criteria removed), and the
  twelve ordered supplement items, including memtable shredding (design shown
  to the maintainer before code); cross-query reuse remains parked unless
  profiling justifies it. Record history moves to the orphan
  `benchmark-records` branch, mounted at `benchmarks/history/`.

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
- Revision 16 — 2026-10-26: Verification's slow tail was visible only in
  side files outside the record, and history could be compared only by reading
  JSON. The maintainer decided: the record carries maximum overhead,
  over-threshold counts and per-phase p99/max from server HDR timing, and one
  HTML page renders and compares history from records only. Approved by the
  maintainer on 2026-10-26.
- Revision 17 — 2026-10-26: The maintainer could not read the benchmarks.
  INVALID hid measured failures, an unenforced driver-core budget refused real
  runs, each measurement was copied into four places, and profiled, partial and
  dirty runs sat in history. The maintainer decided the nine-line definition:
  PASS or FAIL with why, driver CPU as context only, measurements stored once in
  schema v3 with every answer derived by one reader, history of complete clean
  unprofiled runs only, a one-screen report, and old records deleted. Approved by
  the maintainer on 2026-10-26.

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
