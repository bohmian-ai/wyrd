---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-benchmark-experiments
spec_revision: 5
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010]
depends_on: []
parent_task:
remediates: []
---

Superseded by the proposed revision-8 tasks once that revision is approved.
Preserve this task and its evidence as history; do not execute its conflicting
raw-span or older matrix instructions alongside the replacement packet.


Approval: the maintainer approved spec revision 5 and this task. All prior
implementation and evidence were rejected and confer no acceptance credit.

## Outcome and Value

An engineer or agent can run any of Wyrd's three benchmark engines as a declared
experiment and distinguish a performance failure from an invalid measurement.
All engines use one workflow for declaration, validity, measurement, judgment,
history, and reporting. Existing workload owners still exercise real production
paths. For example, a queue stress step currently printed as report-only becomes
a judged step with explicit overload expectations; a saturated driver instead
produces an INVALID record and exit 2.

This single task delivers the whole proposed revision-5 specification: both levels for
Bifrost, Verification, and Gateway; their command and documentation surfaces;
historical baseline configuration; portable accounting and optional profiling;
and retirement of the superseded workflow. Partial engine migrations do not
complete this task.

## What Changes and What Must Stay True

The shared owner is `wyrd-testing::experiment`. Reuse the existing release-server
launcher, resource readings, measurement primitives, engine fixtures and client
workflows. Consolidation must cover the complete workflow, rather than placing
a common renderer over three independent measurement/judgment pipelines.
An experiment here is contributor performance evidence, not a Card or a new
server, SDK, HTTP, MCP, or UI capability.

L1 is closed-loop isolated work: each client waits for completion before sending
again. Its entire default suite must finish within 15 minutes per engine.
L2 is open-loop competing work: arrivals follow the declared schedule regardless
of response completion. Its complete workload lasts 20–30 minutes per engine,
with a hard 30-minute budget. Builds and external prerequisite preparation occur
before the clock; quiet-host checks, server startup, fixture loading, warmup,
measurements and drain occur inside it. Artifact finalization preserves actual
finish time after a stopped overrun.

Each independent step discards warmup and retains three equivalent-state
repetitions: 5 seconds then 3 × 10 seconds for L1, 2 minutes then 3 × 6 minutes
for L2. Stateful capacity/overload/recovery repetitions each contain the whole
sequence; compare matching phases and warm the sequence rather than clearing
the recovery backlog. Numeric judgment uses median repetition metrics unless
declared otherwise; correctness and integrity hold in every repetition.

Report both throughput per provisioned vCPU, (W/T)/P, and efficiency per
server CPU-second used, W/C. P is the recorded logical CPUs available to the
server processes, counting shared CPUs once across same-host replicas/workers
and summing distinct server hosts. T is measured wall-seconds, C measured
server user+system CPU-seconds, and W successful work. Never label C/T as P.
Server processes, replicas and workers are declared and summed over the same
measurement window; driver, upstream, Postgres and object store are separate.
Background work stays in the L2 CPU-used denominator. P supplies provisioned
throughput; declared load budgets supply neither measured denominator.
Baselines and noise-aware regression use W/C and C/W only; report (a) separately. Missing/reset counters, process
loss and zero denominators cannot become favorable numbers. Sampled resident
memory is a sampled peak with a recorded interval, not an OS high-water mark.

Every applicable criterion and measured step has PASS, FAIL or INVALID.
INVALID takes precedence over FAIL, which takes precedence over PASS, with
exit codes 2, 1 and 0 respectively. Preserve valid failures beside invalidity
reasons. Wrong results, unexpected errors and undeclared loss fail valid runs.
Declared overload rejection is distinct from arrivals the driver never sent.
Capture saturation is an observation; capture lost is always zero, and shedding
is permitted only when explicitly accounted in deliberate overload.

Use sysinfo on macOS arm64, Linux x86_64 and Linux arm64, with the same commands,
schema and experiment behavior. Every server is a plain child without OS CPU or
memory limits. Remove the prohibited scoped/cgroup paths and direct perf
processing; optional external samply diagnostics never determine validity,
efficiency or verdict. No permission or OS-limit changes to obtain a profile.

Keep production authentication, tenant binding, permission decisions and their
non-blocking audit costs enabled. Keep acknowledged durability, publication,
exact-answer checks, admission and bounded queues unchanged. Disposable fixtures
and cancellation affect only benchmark-owned state. Summary history contains
synthetic-fixture evidence and symbolic secret inputs, never credentials,
private endpoints, payloads or sensitive host identifiers.

Costs include three measurements per step, host checks, sampling, large L2
fixtures, external services and Linux x86_64 qualification and portability inspection. Only compact
JSON history belongs in Git; raw samples/logs/profiles stay under `target/`.
Cross-host normalization remains qualified advisory evidence. Do not optimize
engines, reproduce ClickBench, import old reports as runs, auto-promote
baselines, add dependencies, invent platform modes, stage/commit results, or
wire any benchmark into CI, gates or verification lanes.

## Approach

1. Revalidate the reuse map and the revision-5 declarations, metric boundaries,
   matrices, baseline mapping and persisted v1 contract against current owners.
2. Establish the shared declaration, validity, repetition, budget and judgment
   workflow with deterministic failure-path proof.
3. Extend portable process/host accounting and optional profiling at their
   existing owners; make missing evidence explicit and remove OS envelopes.
4. Add collision-safe record publication, version-aware reading and one report
   derived from the persisted record, then import only specified baseline data.
5. Migrate all engine workloads to the shared workflow and specified L1/L2
   declarations while preserving their production and integrity checks.
6. Replace public commands and docs, retire dead and duplicate owners and
   references, then collect focused regression and real experiment evidence.

## Ordered Implementation Scenarios

V1–V12 names are proposed proof selectors; the rejected candidate already
contains tests with these names, whose assertions must be revised where
needed. Existing names do not establish revision-5 compliance. Confirm each actual selector after
adding the test and before RED. Verification entries V1–V12 give the exact
commands; plain references replace document links throughout this packet.

### Scenario 1 — A declaration fixes what the run will measure

**Behavior.** Before workload execution, fix the question, identity/level,
sweep, controls, metric units/boundaries, criteria/operators/applicability,
baseline snapshots, validity thresholds and budget. Controls include source,
build/binary identity, host, fixture size/seed/projection, limits, workload mix
and durations. Missing declarations refuse measurement as INVALID. Dirty source
without a declared override refuses measurement; an override stores dirtiness
and the digest of `git diff HEAD` without bypassing other checks. Output from
the run does not retroactively change declared source cleanliness. Criteria
cannot change after observing performance.

**RED.** V1 exercises a small representative experiment through the shared
runner with valid, missing, dirty and attempted post-declaration changes. Assert
no workload admission on refusal, immutable criteria, and attributable controls.
The expected failure is the absent common declaration/refusal workflow, not a
compiler error or service setup failure.

**GREEN.** Supply that declaration workflow and reuse existing binary identity
and flag owners. Demonstrate that another workload supplies declarations and
workload behavior without a second recorder, launcher or judgment pipeline.

**REFACTOR.** Remove duplicated declaration/flag validation only where this
common workflow owns it; keep V1 green.

### Scenario 2 — Invalid evidence is refused without hiding valid failures

**Behavior.** Independently apply these cases to the shared runner; boundary
equalities remain eligible, subject to all other checks.

| Observation | Expected result |
|---|---|
| Either 10-second pre/post host sample exceeds 10% logical CPUs in non-benchmark use | INVALID; preflight refusal admits no workload |
| Driver exceeds 80% of its declared CPU budget | INVALID |
| Any missed open-loop arrival, or send-lateness p99 >5 ms | INVALID; preserve scheduled/sent/completed/missed counts |
| Each applicable p99 has <1,000 samples or p95 has <200 in any repetition | INVALID; L2 heavy-scan p95 needs 200 |
| Required readings missing/reset, declared process lost, or required denominator zero | INVALID; unavailable metrics have reasons |
| Invalid step beside a valid criterion miss | Run INVALID/exit 2; both reasons retained |

**RED.** V2 injects resource, arrival and sample observations, including exact
thresholds and an invalid-plus-failed combination. Assert refusal/partial
evidence and precedence. Expected RED is accepting an invalid case or collapsing
it into performance FAIL. Never saturate the host to test these cases.

**GREEN.** Make validity common to all engines and preserve structured reasons
without granting a general bypass. Rerun V1–V2.

**REFACTOR.** Fold Forge's host eligibility and engine-local sufficiency rules
into this owner without keeping its eight-free-CPU policy.

### Scenario 3 — Repetitions and lifecycle boundaries produce honest evidence

**Behavior.** Warmup contributes no measured work, CPU, latency or samples.
Three repetitions preserve independent metrics/counts and median/minimum/maximum;
they never merge samples to invent a median percentile. Fresh write identities
and equivalent declared state prevent retry acknowledgments replacing work.
Missing/invalid repetitions invalidate the step. Injected elapsed time proves
L1 and L2 budget limits and the L2 minimum duration. On timeout, caught
interruption or runner failure after declaration, stop admission, bound owned
drain/cancellation, retain partial INVALID evidence and mark unmeasured steps;
never fill them with zeros. Cleanup leaves unrelated state intact.

**RED.** V3 uses controlled time/work observations to distinguish warmup from
three windows, deliberately differing percentiles, equivalent-state preparation,
missing repetitions, overrun and interruption. Assert actual finish time,
window scope, bounded cancellation and unmeasured reasons. Expected RED is
single-window behavior, contaminated measurement, or lost partial evidence.

**GREEN.** Apply the lifecycle and equivalent-state contract once, preserving
existing release-server startup/provision/drain and cleanup. Rerun V1–V3.

**REFACTOR.** Remove independent timing/repetition orchestration where replaced;
do not invent a replacement server launcher or fixture framework.

### Scenario 4 — Resource and metric units describe actual server work

**Behavior.** Sum actual user+system CPU across declared server processes in
the same window. Exclude separately reported driver/upstream/service usage.
For wall time T, CPU C, available provisioned vCPUs P, work W and bytes B,
record used cores C/T, throughput (W/T)/P, efficiency W/C and CPU/operation C/W.
Writes report rows/s/vCPU, rows/CPU-s, uncompressed MiB/s/vCPU and MiB/CPU-s;
queries report queries/s/vCPU and queries/CPU-s with executed-plan scanned
rows/bytes per query alongside each request's server query time.
Missing/zero/changed P invalidates provisioned throughput; process replicas
sharing available host CPUs cannot multiply P. Scaling is Q64/(64×Q1) for
matching controls/query class, preserving both inputs. Storage ratio is
published stored/ingested uncompressed bytes after declared settlement;
memory intensity is sampled peak resident bytes/achieved uncompressed MB/s.
Keep MB=10^6, MiB=2^20 and GB=10^9. Retained Arrow bytes count newly inserted
rows, encoded protobuf bytes count OTLP payload before framing, scan bytes come
from executed plans, and retry duplicates are not successful new work.

Judged latency starts at server handler entry after request receipt, and ends
at complete result ready, write acknowledgment or applicable response handoff.
Point-query time includes server admission/queueing after entry through the
complete result, not just stream opening or Oracle execution. Client round
trip, first-token receipt, full-stream receipt, polling visibility and inter-chunk
lag are diagnostic-only. Keep send scheduling/lateness solely for driver validity;
remove scheduled-time latency criteria and collection supporting only them.
Verification overhead subtracts that request's Verifier-block interval union
from handler-entry-to-result-staging elapsed. Gateway overhead subtracts its
own provider wait intervals through each buffered/head/final handoff, including
retries. Retain components and compute residuals per request before percentiles.
Use raw integer-nanosecond samples and nearest-rank percentiles for server
latency/overhead and driver send-lateness validity. Server backlog/drain/recovery
remain lifecycle checks using server state and phase boundaries. Never derive
judged latency from Prometheus/OTel metrics buckets, counts or sums. Do not change
bucket bounds. Inapplicability is explicit; mixed work reports classes against
total server CPU.

Internal timings use standard OpenTelemetry spans and the existing operator
OTLP export, configured for 100% recording, to a local collector. Preserve the
server's monotonic per-request duration observations before metrics aggregation.
Complete instrumentation at the existing server timing owners where needed;
do not create an endpoint or timing protocol. Pair components by request/trace,
process, repetition and phase, subtract within each request, then calculate
percentiles. Count overlapping Verifier blocks once and provider waits through
the same Gateway handoff, including retries. Preserve component and residual
samples, clock/resolution, population/count and collection settings; reconcile
requests/handoffs, flush collection, and make missing/dropped/duplicate required
observations, invalid components or negative residuals INVALID. Do not clamp or
substitute a client round trip or span entered/busy time. Server tracing cost
stays in server CPU; collector cost is separate. Declare its PID as a benchmark
helper for quiet-host checks and preserve its resource/collection-health
readings; it cannot hide unrelated host load or dropped timing evidence.

**RED.** V4 supplies multiple process windows, known bytes/work, changed provisioned
CPU counts at fixed consumed CPU, unavailable/reset counters, missing/zero/changed
P and shared-host replica counts, per-request timing components and
out-of-order trace components, overlapping blocks, retry waits, mismatched
handoffs and missing/dropped/duplicate required observations. Feed raw samples
whose p99 is 499,999 ns, 500,000 ns and 500,001 ns; strict <500 µs gives PASS,
FAIL and FAIL. V4 must exercise collection/decoding through the existing OTLP
path and shared measurement consumer, not only hand-constructed residuals.
Change diagnostic metrics bucket layouts while holding samples fixed and assert
identical latency and verdict; a bucket-only input cannot supply judged latency.
Vary client network delay and polling cadence with identical server samples;
server latency verdicts stay identical. Send lateness still affects driver
validity, and diagnostic-only timings cannot satisfy a server criterion.
Assert both throughput normalizations, historical MB-to-MiB conversion, per-query
scan rows/bytes, nearest-rank p50/p95/p99, timer metadata, sample counts, arithmetic,
units, scope and invalidity. Expected RED is an aggregate-derived percentile,
a missing observation accepted as complete, a clamped residual or a wrong
strict comparison.

**GREEN.** Extend existing capacity/resource/percentile and sysinfo owners;
update all consumers of changed readings and plain-child launch, including
Forge workers. Rerun V1–V4. Real accounting still requires V14 qualification.

**REFACTOR.** Delete replaced query Usage/percentile, client-queue quantile,
Forge Host and repeated memory-headroom logic; preserve still-used primitives.

### Scenario 5 — Criteria distinguish regression, noise and correctness

**Behavior.** For eligible baseline b and three valid values, preserve median
m, low lo, high hi and spread s=hi−lo. For W/C only, higher-is-better fails only when
hi<0.90b and b−m>s; lower-is-better fails only when lo>1.15b and m−b>s.
Equality at either boundary does not fail. Baseline 100 with 75/80/85 fails;
70/85/100 shows no demonstrated regression. Noise never excuses absolute SLO
or correctness failure. Strict 64-client objectives are point p99 <5 ms and
Verification/Gateway overhead p99 <500 µs, with equality failing. Wrong answers,
unexpected errors and undeclared loss fail even if numeric medians pass.

Select eligible comparisons before measurement and store provenance/value and
both comparisons. Import the research mapping's selected values, units, source
identities and statuses to per-engine baselines.toml, preserving original
operands and approximations. Historical “per-core” means W/C, never (W/T)/P;
convert decimal MB/CPU-s to MiB/CPU-s by ×10^6/2^20 and retain source units.
Provisioned-vCPU throughput is reported without a baseline regression criterion. Primary candidates lacking clean provenance stay
excluded; use eligible cfe21dc90 fallback entries where mapped. Verification
and Gateway have no invented eligible fallback. A declared criterion without
a usable baseline is INVALID. Cross-host/OS evidence remains advisory with both
fingerprints; dirty override runs cannot become references. No auto-promotion.

**RED.** V5 table-drives both regression directions, percentage/spread equality,
the specified examples, strict SLO equality and correctness in only one
repetition. V6 exercises selected/excluded/missing/incompatible baseline entries,
unit conversion and advisory comparisons through runner-to-record integration.
Expected RED is old inclusive SLOs, single-run comparison, or invented eligibility.

**GREEN.** Use one criterion/judgment owner and specified baseline data; retain
valid performance failures independently of invalidity. Rerun V1–V6.

**REFACTOR.** Remove old verdict labels as judgments and retire dead SloGate;
retain informative saturation observations without a fourth verdict.

### Scenario 6 — Historical summaries survive failure and remain private

**Behavior.** Persist schema v1 with every field/meaning in the proposed record
table, explicit units, three repetitions, criterion and validity reasons,
baseline snapshots, controls, build identity, monotonic elapsed time and RFC
3339 instants. Publish under benchmarks/<engine>/<experiment>/ using actual
UTC start YYYYMMDDTHHMMSSZ and nine SHA digits. Same-second/SHA collisions append
-2, -3 and cannot overwrite history. Full SHA remains authoritative. Use
temporary-file/rename publication; partial JSON must never appear complete.
Persistence failure exits 2 with a persistence error and salvageable local
evidence. Unsupported versions and malformed input refuse interpretation.
Missing observations are null with reasons, never NaN/infinity or fabricated
zero. Caught interruptions preserve partial evidence; abrupt kill/power loss
cannot guarantee final publication. Synthetic summaries exclude secret values,
private endpoints, tenant payloads and sensitive host identifiers; optional
target references do not make history depend on raw artifacts.

**RED.** V7 round-trips representative complete/failed/partial records, malformed
or unsupported data, collisions and write/rename failures. Include sensitive
configuration sentinels and assert they never enter committed summaries. Assert
dirty digest bytes and unsupported-reader refusal. Expected RED is missing
history, overwrite, false success, secret leakage or permissive interpretation.

**GREEN.** Implement the shared persisted reader/writer contract with explicit
failure handling. No old report becomes a fabricated v1 run. Rerun V1–V7.

**REFACTOR.** Keep raw samples/logs/profiles external; remove superseded summary
serialization rather than maintaining parallel record forms.

### Scenario 7 — A report can be regenerated from history alone

**Behavior.** Render console and regenerated reports from the persisted record,
including INVALID/FAIL reasons, spread and verdict. Common columns are exactly
step, params, rate, p50/p95/p99 ms, errors, server CPU, peak mem, driver CPU,
criterion, observed, verdict. Units accompany values; CPU shows consumed seconds
and mean used cores plus provisioned vCPUs. Label both throughput metrics in
rate/supporting detail, server time in latency columns, and scan rows/bytes per
query alongside query time. Client diagnostics never fill judged latency cells.
Non-applicable cells use an em dash with a reason. Extra
engine detail also comes from JSON. Removing target artifacts cannot change
the summary or require rerunning measurement.

**RED.** V8 persists a representative multi-criterion record, renders it, removes
optional local artifacts and regenerates. Assert exact header and agreement of
values, units, reasons, spread and verdict for all three outcomes. Expected RED
is an engine-specific layout or dependence on ephemeral samples.

**GREEN.** Adapt Gateway Report/Row into the common record-derived rendering
workflow and migrate all writers. Rerun V1–V8.

**REFACTOR.** Delete old report writers and duplicate formatting/calculation;
keep engine detail as projections of the same record.

### Scenario 8 — Profiling is portable optional diagnostic evidence

**Behavior.** Every engine and level accepts optional --profile and uses samply
with identical invocation on supported platforms. Missing/unusable profiling
records a note and capture completeness without changing a measured verdict.
It cannot excuse missing required accounting, supply the CPU denominator, or
alter OS permissions/limits. Captures stay under target; direct perf capture
and its report/script processing are removed.

**RED.** V9 exercises absent, failing, incomplete and successful profiler
observations with otherwise identical measurements. Assert identical judgment,
diagnostic notes and no required-reading bypass. Expected RED is a mandatory
perf dependency or a profiler failure changing the verdict.

**GREEN.** Extend the existing profile lifecycle for optional samply, updating
all workload consumers. Rerun V1–V9; V14 supplies actual platform capture proof.

**REFACTOR.** Delete superseded perf processing and avoid backend-specific
harness branches or diagnostic permission setup.

### Scenario 9 — Every engine completes its full declared L1 matrix

**Behavior.** Default L1 declarations cover these isolated operations with one
fixed deterministic fixture configuration per engine and the shared windows.

| Engine | Full default coverage and judgment |
|---|---|
| Bifrost | 2M-row native, OTLP spans and OTLP logs write sweeps at 1/4/16/64; Parquet point lookup at 1/4/16/64; small aggregate and ordinary scan at 1/64; exact results and required metrics |
| Verification | Direct verify at 1/4/16/64, server overhead components and verifications/CPU-second |
| Gateway | Added overhead at 1/4/16/64 across declared buffered/streaming/capture shapes and applicable handoffs, with capture accounting |

Fixed 64-client SLOs apply only at their declared client count. Ordinary slow
scans use p95. Measured writes run the full 10 seconds with fresh identities,
not until a finite fixture is exhausted. Only native peak ≥200k total rows/s
and OTLP peak ≥30 wire MiB/CPU-s are retained sanity floors;
record peak client counts. Other throughput judgments use declared baseline
regression without new absolute floors. A selected subset records omissions
and cannot claim full coverage passed.

**RED.** V10 checks engine declarations, selected subsets, timing/metric
applicability, fixture controls and sanity-floor peak selection against the
matrix. Expected RED is missing coverage or inherited floors/short windows.
V14 then exercises each actual full release-process suite and record/report
agreement; structural tests do not establish measured capacity or completion.

**GREEN.** Migrate existing query/write, direct-verification and Gateway-overhead
workflows to the shared owner and preserve production checks. Rerun V1–V10;
collect V14 evidence within declared budgets without compiling during execution.

**REFACTOR.** Remove engine-owned measurement/judgment duplication, retaining
engine workload behavior and useful source primitives.

### Scenario 10 — L2 measures competing work through overload and recovery

**Behavior.** Each engine uses declared open-loop load with foreground and
background work observed concurrently in every repetition, not separately
measured best cases. Bifrost uses the 100M-row heavy fixture, Forge compaction,
reads while writing, and full-queue behavior. Ingest fractions are 50% OTLP
spans, 30% logs and 20% native. Its 32 query clients use 60% point lookups,
25% dashboard aggregates (declared small/1m split), 10% table aggregates and
5% scans. Matching L1 peak valid per-signal rows/CPU-second capacities r_i give
mixed capacity 1/Σ(f_i/r_i); initial rate is 70% of that capacity times declared
server CPU budget. Missing/incompatible host/build/projection/limits calibration
refuses execution as INVALID, never substitutes historical total throughput.

Bifrost concurrent bounds are point p99 <5 ms, small aggregate p95 <100 ms,
1m aggregate p95 <300 ms, heavy scan p95 <2 s with ≥200 samples per repetition,
and server write acknowledgment p99 ≤100 ms. Preserve the 5% scan mix and
six-minute windows: declare a query rate capable of ≥4,000 queries (≥200 scans)
per repetition and check actual completions; under-sampling is INVALID.
Visibility polling and its historical ≤5 s reference are diagnostic-only. Table aggregates retain correctness and efficiency
regression without a new absolute floor. Do not label 32-client results as the
64-client L1 SLO.

Verification overlaps direct and background verification, Bifrost evidence
writes and backlog progression. Gateway overlaps capture with server completion,
first-token response handoff, final-frame handoff and applicable overhead.
Measured from handler entry, defaults are p99 ≤200/350/1,100 ms respectively;
explicit overrides are fixed before execution. Client timings and inter-chunk
lag (historical p99 ≤50 ms reference) are diagnostic-only. Every stateful repetition runs capacity → overload →
recovery from equivalent initial state and retains matching phase metrics and
criteria. Backlog grows by at most one second of offered work, drain is ≤60 s
and recovery restores SLOs within 30 s for corresponding phases in all engines.
Expected admission refusals are declared; driver misses invalidate. Capture
lost always fails; accounted shedding is allowed only during deliberate
overload, and outside it fails. Preserve client-queue headroom/ACK-delay/in-flight
and Forge maintenance workload purposes without inherited report-only steps or
resource quotas.

**RED.** V11 tests incompatible calibration, harmonic capacity arithmetic,
concurrent criterion failure despite isolated success, full-sequence repetition
state, and backlog/drain/recovery/loss/shedding boundaries with controlled
observations. Also assert heavy-scan p95 sufficiency at 199/200 samples and
p99 insufficiency below 1,000; visibility/client timing and chunk-lag cannot
create SLO failures. Expected RED is separate windows, omitted stress judgment or
incorrect calibration/phase accounting. V15 supplies actual overlapping
production workload timelines and records for each engine.

**GREEN.** Extend existing engine workload owners, including the current
open-loop Gateway scheduling and existing Forge/client-queue production setup,
to use the shared contract. Rerun V1–V11, then collect V15 without relaxing sample,
duration, correctness, queue, admission or concurrency obligations.

**REFACTOR.** Consolidate shared lifecycle and accounting; do not replace
production queues, storage, clients or the release-server launcher.

### Scenario 11 — Contributor commands give one consistent result

**Behavior.** Replace all old bench entry points without aliases with the seven
specified L1/L2 commands. L1 aggregate runs the engines sequentially, forwards
--profile, retains individual records and combines exit precedence even when
an engine fails or discards. Engine commands use the same schema/options on all
three platforms. No build occurs during measurement. Documentation teaches
levels, budgets, prerequisites, criteria, INVALID versus FAIL, clean/dirty source,
baselines, platform limits, optional profiling and history/report reproduction.

**RED.** V12 exercises dispatch/aggregation with controlled engine outcomes and
argument forwarding; assert sequential execution and 2>1>0 precedence. Expected
RED is old entry points, dropped --profile or stopping before later engine
records. V16 independently inspects the mise graph and documentation.

**GREEN.** Replace task wiring and descriptions, update benchmarks/README.md
and TESTING.md, and remove stale task/threshold/load references without widening
test filters. Rerun V1–V12; V14 exercises the real aggregate and engine commands.

**REFACTOR.** Remove obsolete flags, exports, dead configuration and duplicate
parsers after checking consumers; retain still-used benchmark primitives and
production correctness coverage.

## Acceptance Criteria

TASK-001 owns every row. Proof labels refer to Verification and Evidence.

| Obligations | Required observation and what would violate it | Proof |
|---|---|---|
| REQ-001, INV-005, AC-008 | Workload admission follows a complete immutable declaration; another experiment reuses the entire common workflow. Missing controls or post-measurement criteria changes cannot be accepted. | V1; reuse audit in V16 |
| REQ-002, REQ-004, AC-004 | Independent validity failures and exact boundaries produce typed reasons, exit 2 and partial evidence; valid failures remain visible under INVALID precedence. | V2, V3, V12 |
| REQ-003, INV-007 | Warmup is excluded, all three repetitions retain independent statistics/equivalent state, budgets stop work and no compilation or synthetic host load contaminates execution. | V3, V14, V15, V16 |
| REQ-005, AC-007 | All stress/overload phases judge declared correctness, rejection, loss and recovery; isolated passing work cannot excuse concurrent misses. | V5, V11, V15 |
| REQ-006, INV-004 | (W/T)/P, W/C and C/W recompute from disjoint real process/window data; missing/reset readings cannot masquerade as zero use. | V2, V4, V14 |
| REQ-007 | All applicable engine metrics use specified units, timing boundaries and raw samples and complete matched trace components; absent/inapplicable values explain why. | V4, V10, V11, V14, V15 |
| REQ-008, AC-001 | All three full default L1 matrices finish within budget with fixed fixtures, repetitions, strict fixed-client SLOs, limited sanity floors and accurate subset claims. | V3, V10, V14 |
| REQ-009 | L2 meets minimum/hard durations, real overlap, exact mixes and compatible L1 calibration; phase-equivalent repetitions preserve concurrent SLO/backlog/drain/recovery evidence. | V3, V11, V15 |
| REQ-010, AC-009 | Seven discoverable commands replace old entries without aliases; aggregate is sequential; docs explain reproducibility; no benchmark enters CI/gates. | V12, V14, V16 |
| REQ-011 | Baseline data matches the normative research mapping with explicit absence/exclusion, declared snapshots and advisory cross-host/OS treatment. Dirty runs cannot become references. | V6; baseline inventory in V16 |
| REQ-012, AC-002, AC-003 | Both conservative comparisons retain operands, percentage/spread equality passes, noisy values do not excuse absolute/correctness failures. | V5, V6 |
| REQ-013, INV-008, AC-006 | v1 is attributable, append-only, atomically published and independently readable; collisions, unsupported versions, partial records and persistence errors behave honestly. | V7, V14, V15 |
| REQ-014 | Exact common report columns, values, units, spread and reasons regenerate from the persisted summary without target artifacts. | V8, V14 |
| REQ-015 | The whole workflow is shared, existing launcher/workloads are extended, and optional profiling cannot affect judgment or replace required readings. | V1, V9, V16 |
| REQ-016 | Dead thresholds/SloGate/load, direct perf, scoped/cgroup accounting and duplicate measurement/report/parsing paths disappear; workload purposes and used primitives remain. | V13, V16 |
| INV-001, INV-002, INV-003, AC-010 | Real paths retain result checks, tenant denials, acknowledged durable recovery and non-blocking permission audit; no performance shortcut bypasses them. | V13; real-workflow review in V14/V15 |
| INV-006 | Summary serialization excludes sensitive sentinels; cancellation/cleanup affect only owned fixtures, and incomplete work is not claimed successful. | V3, V7, V13 |
| INV-009, AC-005 | Every L1/L2 experiment and real CPU accounting run on Linux x86_64; all experiments remain runnable unchanged on macOS arm64 and Linux arm64 with identical commands/schema; profile capture or explicit diagnostic note is present. Both levels lack prohibited harness paths. | V4, V9, V14, V15, V16 |

## Implementation Reference

Paths describe owners and likely changes, not a file or type allowlist.
No nested AGENTS.md was found in changes, wyrd-testing or wyrd-bench. Production
semantics remain authoritative; only measurement instrumentation may extend
the existing server timing/telemetry owners for exact per-request collection.

The inventory below describes the original owners, including files removed by
the rejected uncommitted candidate. It is reuse guidance, not approval of that
candidate. Current experiment.rs Reading::histogram/bucket and interval-bound
judgment, capacity.rs bucket quantiles, Verification run.rs histogram p99s and
Gateway run.rs/capacity.rs histogram overhead conflict with revision 5. Reuse
lifecycle/resource primitives and workload setup where correct; retain raw
client timing only as optional diagnostics. Revise declaration, record, report,
throughput arithmetic and tests for P and the two labelled metrics. Replace
bucket-dependent judged latency with raw server tracing; delete client-latency
criteria, scheduled-time judgment-only collection and heavy-scan p99. Retain
send-lateness validity and CPU-used regression. Do not delete the candidate
wholesale; none of its existing evidence establishes revision-5 acceptance.
Do not restore retired workflows merely to use an old filename.

- Exact internal timing: Gateway server components/gateway/telemetry.rs
  Profile::point already computes each monotonic elapsed/provider-wait/residual
  tuple before metrics recording. Verification components/verification/service.rs
  VerificationControl::execute documents arrival-to-staging minus block union; extend
  verification/telemetry.rs ExecutionTelemetry::emit already computes block
  union and residuals. Extend these timing owners to expose observations on
  request spans. Bifrost Gate/Oracle/Scribe owners also need exact
  handler-entry-to-complete-result/write-ack observations and executed-plan scan
  rows/bytes per query; stream-open and Oracle-only timers omit required work.
  The existing metrics export loses individual values; client
  collectors cannot recover them. crates/shared/wyrd-telemetry/src/lib.rs
  telemetry_sampler, OTLP exporter and export.rs tracer_provider already own
  server tracing, sampling and operator routing. Reuse this standard stack with
  local OTLP collection and 100% operator recording. Existing request tracing
  is not evidence of complete boundary coverage: add only missing timing
  observations at the existing owners and verify collection against request
  counts. No new public product surface or dependency is proposed.

- Shared workflow and resources: `crates/wyrd/wyrd-testing/src/capacity.rs`
  owns Percentiles::raw and existing diagnostic bucket helpers, Window::open/close,
  ResourceWindow::open/freeze/finish, driver_cpu_seconds, release_binary and
  binary_identity. Inspected Gateway/Verification run consumers and Forge
  deployment resource windows. Existing inline tests cover primitive telemetry
  and percentiles. Current freeze/finish saturates counter subtraction, and
  missing readings may become zero; there is no declared experiment, repeated
  judgment or durable summary workflow. Extend these primitives into the
  shared experiment owner rather than duplicate them or move behavior to
  wyrd-spec. Preserve existing binary/build identification behavior without
  inventing stronger source-to-binary validity requirements.
- Server lifecycle/accounting: `src/release_server.rs` LocalServer::start,
  serve, start_forge_worker, cpu/memory readings and stop already own complete
  migrate/serve/setup and cleanup. `src/release_server/os.rs`
  process_cpu_usec and PeakSampler read sysinfo CPU/RSS, with a 10 ms sampling
  interval and zero on missing CPU. The inline OS test checks actual readings.
  `src/bin/bifrost_forge_capacity/host.rs` HostLoad::sample/qualifies samples
  host/process use but enforces an obsolete eight-free-CPU envelope. Reuse these
  owners for portable process-aware 10-second pre/post checks and required
  availability. Remove scoped placement, cgroup reads and quota accounting;
  update every LocalServer/resource consumer, not just engine entry points.
- Bifrost: `src/bin/capacity/query/mod.rs` routes fixtures and sweeps;
  query/run.rs Bench and Measured preserve public-client timings, exact results,
  retained Arrow/OTLP/scan observations. Measured::percentile_ms and its private
  Usage duplicate shared owners. Inspected query setup and report ownership;
  extend the existing query/workload, telemetry and logs paths for specified
  fixture sizes, L1 matrices, calibration and concurrent L2. Do not retain hot
  and Parquet as separate default L1 suites or lose the heavy workload purpose.
- Verification: `src/bin/capacity/verification/workload.rs` Tenant::provision
  builds authenticated WyrdState workflows. run.rs Bench::run/drive/drain and
  Op preserve direct/submit behavior and server overhead components. report.rs
  inline tests currently accept inclusive overhead and report-only queued
  latency. Replace those expectations with shared strict/applicable criteria
  and true direct/background/evidence overlap; retain correctness checks.
- Gateway: `src/bin/capacity/gateway/capacity.rs` Driver, Bench::rate/overload,
  Measured and LATENESS_LIMIT_US own scheduled arrivals and backlog evidence;
  current MIN_CALLS=30 and short windows do not satisfy the revision-5 contract.
  envelope.rs classifies capacity/capture/recovery; report.rs Report/Row is the
  original common-report starting point but currently includes OUTSIDE-SLO and
  CAPTURE-SATURATED judgments. Inspected report inline tests and mod.rs
  overhead/capacity callers. Extend those workload/timing/capture owners and
  migrate report tests, not another Gateway transport or scheduling layer.
- Queue and Forge closure: `src/bin/capacity/client_queue/mod.rs` constructs
  sustained/headroom/ACK-delay/in-flight plans with a gating flag, and step.rs
  Bench/Record/ServerSnapshot preserve durable-row and queue accounting with a
  duplicate quantile. `src/bin/bifrost_forge_capacity/main.rs` and deployment.rs
  Deployment::drain start leader/workers, populate backlog and measure rewrite
  work; they consume Envelope and ResourceWindow. Preserve these workload
  purposes in Bifrost L2, remove report-only gating and resource scopes, and
  retire their separate workflow/report/host owners when superseded.
- Profiling: `src/profile.rs` Capture::start/finish and Profile already track
  PID/replica artifacts and incompleteness, but invoke perf record/report/script
  and feed engine Pareto formatting. Replace only diagnostic capture/processing
  and affected consumers with optional samply; this is not resource accounting.
- Parsing and dispatch: `src/bin/capacity/flags.rs` value/list/clients/known
  already rejects malformed/unknown/duplicate selections for Gateway and
  Verification. Its inline test covers those refusals. main.rs dispatches the
  existing workloads. Reuse the shared parser owner instead of adding a new
  parser stack; migrate query/queue flag handling as appropriate.
- Retirement: `crates/shared/wyrd-bench/src/slo.rs` SloGate and its loader/tests
  consume benches/thresholds.toml, exported from src/lib.rs. Repository consumer
  inspection found no live capacity workload using it. Keep other used
  wyrd-bench primitives; remove dependent imports/dependencies only when unused.
  `crates/wyrd/wyrd-testing/src/load/` is exported from src/lib.rs with inline
  harness tests but no external workflow consumer found. Remove the stale
  load:: selector in test:bifrost:unit:rust:inner without broadening its filter.
- Contributor/persistence closure: mise.toml owns seven old bench tasks and
  their Postgres/build/storage setup; retain prerequisite ownership while
  replacing the public task names. `wyrd-testing/Cargo.toml` declares capacity,
  bifrost_forge_capacity, lib/default features and capability test targets;
  sysinfo is already consumed. Likely documentation/data changes are
  benchmarks/README.md, per-engine baselines.toml and TESTING.md. No SDK export,
  wire schema, migration or generated product artifact change is required.
- Production regression seams: existing `wyrd-testing` server and scribe
  targets cover tenant-scoped query permissions, lifecycle audit, durable
  staging recovery, and shared shutdown/capture acknowledgment. V13 names the
  inspected tests precisely. They supplement actual release-process evidence;
  they do not establish benchmark throughput, portability or budget claims.

## Verification and Evidence

These are implementation obligations; none was executed during planning.
Run Cargo-backed checks sequentially. Use current default features for the
focused library/binary tests. The package's manifest confirms the lib, capacity
binary and server/scribe targets. Planned names below are selectors, not claims
of existing tests or prescribed private production structure. After adding
each test, confirm the actual selector and record its exact command before RED;
an empty selection, compilation error or setup failure is not behavioral RED.
If private test organization changes, correct selectors under bounded execution
rules while retaining the same observations.

### V1–V12 — Focused deterministic and runner integration proof

Each command below runs one proposed proof test (reusing a candidate selector
where present). V1 also supplies the representative
shared-workflow integration experiment. V6 integrates baseline selection with
run recording; V7 exercises the real filesystem; V8 exercises the real persisted
reader and renderer. V4 uses local OTLP collection with known timing observations; other
resource/clock/process/profiler failure observations are controlled without synthetic host load. Use local in-process OTLP capture for
V4, not a deployed collector or credentials. These tests never replace
V14/V15 release-process qualification and the real server timing/export path.

```text
V1 — declaration and shared-workflow reuse
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::declaration_precedes_workload)'
V2 — validity thresholds and precedence
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::invalid_observations_preserve_reasons)'
V3 — windows, equivalent state, budgets and cancellation
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::repetitions_exclude_warmup_and_bound_execution)'
V4 — raw latency, complete OTLP collection, component and CPU/byte arithmetic
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::metrics_preserve_process_scope_and_units)'
V5 — numeric, strict SLO and all-repetition integrity judgment
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::criteria_distinguish_regression_noise_and_invalidity)'
V6 — specified baseline data, selection, provenance and record snapshot
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::baseline_selection_preserves_eligibility)'
V7 — v1 history, collisions, publication failure, privacy and malformed input
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::history_is_append_only_private_and_versioned)'
V8 — exact record-derived reporting without raw artifacts
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::reports_regenerate_from_persisted_records)'
V9 — optional profiling cannot affect judgment
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::profiling_is_optional_diagnostic_evidence)'
V10 — full engine declarations, L1 matrices and subset applicability
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::level_one_declares_complete_engine_coverage)'
V11 — calibration and concurrent L2 phase judgments
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::level_two_preserves_concurrent_work_and_recovery)'
V12 — sequential aggregation, forwarding and exit precedence
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::level_commands_preserve_records_and_exit_precedence)'
```

V10/V11 live in the current workload-bearing capacity target unless execution
discovers a simpler existing owner; they must exercise the real engine
declarations/judgment inputs, not copies of the required matrix in test-only
code. Keep newly introduced benchmark execution opt-in even when deterministic
tests run in normal test lanes.

### V13 — Existing production guarantees and owner regressions

The following selectors exist in the current sources. The Postgres wrapper and
db:migrate:inner setup match the owning capability lanes; journey profile and
run-ignored=all include these real-server tests. Their storage fixtures use the
repository harness. Run separately so each observation has exact evidence.
First satisfy the existing server journey/family lane's build prerequisite,
outside any experiment clock:

```text
mise run build:server
```

```text
Tenant permission denial and table grants
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=query::tenant_scoped_roles_reach_only_their_granted_bifrost_tables)"'
Tenant-scoped lifecycle control and permission audit
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=query_lifecycle::grpc_lifecycle_controls_are_tenant_scoped_and_audited)"'
Acknowledged staging recovers and publishes once
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=recovery::variant_staging_restores_and_publishes_once)"'
Bounded shutdown preserves accepted work and captures
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::shared_shutdown_drains_producers_workers_and_acknowledgements)"'
```

Expected evidence is preserved exact results, tenant-negative behavior, audit
decisions and recoverable acknowledged work, including expired drain refusal.
Inspect changed benchmark fixtures for scoped cleanup and measured production
audit cost as well. These tests do not alone prove non-blocking audit under a
writer outage; any touched audit seam needs its existing owner regression,
without changing production semantics.

Owner checks after focused scenarios, for changed reusable primitives and
remaining binary tests, use these narrow package/target commands:

```text
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --lib'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity
mise exec -- cargo nextest run --locked -p wyrd-bench --lib
```

If the Forge binary remains as a workload owner, also run its current unit
target; if retired, record source/manifest removal as its proof instead:

```text
mise exec -- cargo nextest run --locked -p wyrd-testing --bin bifrost_forge_capacity
```

### V14 — Actual L1 release-process and platform journeys

These are the proposed commands, already wired in the rejected candidate;
their presence is not proof of a compliant implementation or an executed run:

```text
mise run bench:l1:bifrost
mise run bench:l1:verification
mise run bench:l1:gateway
mise run bench:l1
mise run bench:l1:bifrost -- --profile
mise run bench:l1:verification -- --profile
mise run bench:l1:gateway -- --profile
mise run bench:l1 -- --profile
```

Exercise every L1 experiment for every engine on Linux x86_64. Real macOS arm64
and Linux arm64 runs are optional; inspect unchanged runnable behavior there. Use the engine commands separately for budget/coverage evidence
and the aggregate for sequential invocation/forwarding evidence. Commands own
existing build, repository Postgres, storage and local upstream prerequisites;
finish setup/build before the experiment clock. Use release servers and real
public SDK/OTLP/Gateway paths with synthetic data. Keep each experiment free of
other benchmark/build activity. Missing/unusable profiling must demonstrate a
diagnostic note without changing judgment, in addition to successful captures
where available. Do not claim a capture exists when unavailable.

Preserve records with command, platform, controls, window/process scope, all
repetitions, timing method/resolution/population, sample counts, metrics/units
and verdict/exit agreement. Independently recompute judged latencies from raw
samples, including matched internal components and per-request residuals.
Reconcile recorded requests/handoffs with collected timing observations; prove
missing/dropped timings yield INVALID rather than a successful subset. Recompute
(W/T)/P, W/C and C/W, including write rows and uncompressed MiB in both
normalizations and query scan rows/bytes alongside server time; validate full coverage and timing, and compare console with a report
regenerated from persisted JSON after target artifacts are unavailable. Use the
implemented record reader/renderer at its actual invocation and record that
command in execution evidence; the spec fixes report behavior but does not
name a separate report command. Do not invent another public interface in this
plan. Performance FAIL/INVALID is honest evidence, not permission to weaken
criteria or claim a passing complete suite. Missing required Linux x86_64 evidence blocks an implementation-complete claim;
missing optional macOS arm64 or Linux arm64 runs do not.

### V15 — Actual concurrent L2 release-process journeys

```text
mise run bench:l2:bifrost
mise run bench:l2:verification
mise run bench:l2:gateway
mise run bench:l2:bifrost -- --profile
mise run bench:l2:verification -- --profile
mise run bench:l2:gateway -- --profile
```

Collect one complete qualified workload per engine with real overlapping work,
the declared shared windows/budget, three repetitions and timelines establishing
concurrent SLO and phase applicability. Preserve compatible Bifrost L1 run IDs,
per-signal capacities and mixed-load arithmetic, 32-client mix, Forge work,
queue behavior, Verification evidence writes and Gateway capture accounting.
Show offered/sent/completed/missed arrivals, per-phase backlog/drain/recovery,
strict versus inclusive boundaries and integrity in every repetition. Recompute
server-only latencies and both throughput metrics from raw observations; check
≥200 heavy-scan samples for p95 and the declared query-rate feasibility in each
repetition, with client visibility/chunk timing excluded from SLO judgment. Exercise
optional profiling at both levels without making it required evidence.
Run every required L2 experiment on Linux x86_64. Both levels and every
experiment must work unchanged on macOS arm64 and Linux arm64; inspect commands,
dependencies and measurement paths for portability. Real runs on those two
platforms are optional and their absence is not a blocker.

### V16 — Static migration, authority and documentation proof

TDD is not applicable to documentation, baseline transcription, dead-code
removal and unchanged production contracts. Inspect the final consumer and task
inventory: coverage purposes map to the new levels; no old bench aliases,
threshold loader/export, load export/selector, duplicate measurement/report
workflow, perf processing, OS scope/envelope/cgroup discovery, prohibited
platform path, new dependency or benchmark CI/gate edge remains. Preserve
other test selection and used benchmark primitives. Compare every initial
baseline entry with the normative research mapping, including excluded/absent
sources and original units/operands. Confirm README/TESTING examples and
explanations match actual invocation and record/report behavior.

Run the repository's task listing to inspect names and dependency graph:

```text
mise tasks ls --hidden
```

Run format/lints and the dependency boundary check for the Rust/manifest write
set, once after focused checks pass:

```text
mise run fmt
mise run lints
mise run check:deps
```

No generated SDK/wire artifact or SQL query changes are expected; do not add
irrelevant codegen, language or tenancy lanes as boilerplate. If execution
discovers an actual affected boundary, add its narrow owning proof without
adding benchmark execution to it. Broad integrated checks belong to change
review, not this task. Record RED/GREEN results, actual selectors, final focused
checks, static inventories, real platform/workload evidence, remaining limits
and bounded corrections using changes/implementation-execution.md.

## Material Stop Conditions

- Required behavior cannot fit the specified budgets, fixture sizes, windows,
  repetitions, sample sufficiency or workload mixes without weakening one.
  Demonstrate the conflict; do not shorten steps or invent a passing subset.
- Exact internal timing cannot be collected completely through standard
  tracing at the specified server boundaries. Do not substitute bucket metrics,
  sampled traces or client round trips; request a spec revision to a directly
  measurable boundary. No universal standard overhead metric fills this gap.
- A required metric/byte boundary cannot be measured honestly through current
  owners, or would require a changed public telemetry contract,
  dependency, feature, tenancy, durability, audit or admission change.
- An in-scope portable sysinfo/plain-child implementation cannot supply required
  accounting on a supported platform. Do not restore prohibited OS mechanisms
  or turn missing readings into zero. An unavailable profiler alone is not a
  stop condition.
- Completing baseline selection requires an unmapped value, provenance waiver,
  changed eligibility or automatic promotion. Missing historical references
  must stay explicit; a declared unusable comparison is INVALID.
- Completing persistence/report behavior requires changing revision-5 v1 field
  meanings, history identity/collision semantics or a new durable product owner.
- A discovered conflict with repository authority needs a meaning-changing spec
  revision. Reversible private structure, fixtures, helpers and equivalent
  focused commands remain implementation-owned bounded decisions.
- Required real Linux x86_64 evidence is unavailable: finish unaffected work and
  report the precise unverified obligation rather than claiming completion or
  manufacturing results.

## Authority Links

References are plain repository paths, following the packet's no-links rule.

- changes/active/benchmark-experiments/spec.md — draft revision 5 (approval pending), all
  behavior, v1 record table, metrics, levels, constants and acceptance journeys.
- changes/active/benchmark-experiments/research.md — eligible/excluded baseline
  mapping incorporated by the spec; other research is supporting evidence.
- AGENTS.md; changes/spec-driven-development.md;
  changes/implementation-execution.md; TESTING.md — authority, ownership,
  execution, proof and completion rules.
- architecture/wyrd-design.md — Doctrine and Bifrost engine/client boundary.
- architecture/bifrost-design.md — System boundary, Durability and visibility,
  Resource and failure invariants, and Telemetry / Measurement meanings.
- architecture/wyrd-security-posture.md — Security principles, Tenant and data
  isolation, Audit integrity and privacy.
- agent_docs/index.md; agent_docs/rust.md; agent_docs/errors.md;
  agent_docs/maintainer-style.md; agent_docs/agent-surfaces.md — applicable
  Rust, error, maintainability and contributor/agent surface conventions.

## Revision history

- Revision 5 — 2026-10-24: Aligned the proposed task with server-only latency,
  per-request traced wait subtraction, both throughput normalizations with
  CPU-used regression, query scan evidence, diagnostic client timing and feasible
  heavy-scan p95. Spec remains draft; task remains proposed and blocked on
  maintainer approval. Existing implementation must be revised, not discarded
  wholesale; obsolete judgment paths must be deleted.
