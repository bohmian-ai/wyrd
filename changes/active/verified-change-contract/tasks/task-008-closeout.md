---
id: TASK-008-CLOSEOUT
kind: implementation
status: proposed
caller_approval: approved
planning_result: SPEC_REVISION_REQUIRED
spec: SPEC-verified-change-contract
spec_revision: 49
requirements: [REQ-089, REQ-101, REQ-114, REQ-115, REQ-135, REQ-136, REQ-137, REQ-145, REQ-146, REQ-151, REQ-152, INV-015, AC-017, AC-020, AC-021, AC-022, AC-023, AC-024, AC-030, AC-032, AC-033]
depends_on: [TASK-005, TASK-006, TASK-009, TASK-010, TASK-012]
continues: TASK-008
intended_to_replace: task-008-recovery.md
---

## Outcome and Value

Close all remaining TASK-008 implementation and verification obligations and
replace the observation-traffic benchmark with a maintainer-readable
verification-capacity benchmark. One command must demonstrate PSI, SPC, Custom
Drift, assertion Eval and LLM-judge Eval executing together, report sustainable
throughput, and locate preparation, computation and waiting costs. Include an
initial supplied-input synchronous API so the same benchmark also measures
client → server → verifier → client latency without a Bifrost evidence path.

The caller explicitly approved this revised closeout plan after its telemetry,
metric catalog, trace and CPU-profiling details were added. Grafana deliverables
remain excluded. This records plan approval; it does not start production-code
implementation or constitute an independent task-review verdict. Do not ask
again for approval of the decisions already fixed here.

Revision 49 remains the historical approved spec baseline. Before execution,
carry these approved plan decisions into the spec and finish the explicitly
listed direct API wire/error semantics and reference workload details. Approval
of the plan does not supply an unspecified wire shape or numeric workload.
`SPEC_REVISION_REQUIRED` identifies that remaining contract closure, not a need
to reapprove the telemetry/concurrency/profiling recommendation.

## Current State and Resume Point

Checked against HEAD `796166959` and the existing uncommitted working tree.
TASK-008 is substantially implemented; it is not a task to restart from scratch.
The original task records historical PASS evidence but is superseded by the
recovery task. Neither that evidence nor the recovery task's `proposed` status
establishes completion of this closeout.

| Work | Current evidence | Resume action |
|---|---|---|
| Integrated correctness journeys | Original TASK-008 Implementation Evidence; current Rust `sdks/wyrd-sdk-rust/tests/drift_verification.rs`, Python `sdks/wyrd-sdk-python/tests/integration/test_drift_journey.py`, TypeScript `sdks/wyrd-sdk-ts/wyrd/tests/integration/drift-verification.test.ts`; existing runtime, routes, Operator and Bifrost tests | Preserve implementations and journey registration. Reconcile the inherited matrix below; add only missing proof/seam fixes and reverify the final candidate. |
| Production runtime telemetry | Commit `7ddcb7f0d`; `wyrd-server/src/app/metrics.rs` and `verification/runner.rs` expose queue wait, trigger-to-terminal and load/engine/publication/settlement phases, using pooled `implementation=drift|eval` dimensions | Extend the existing owners with per-kind/input-read/preparation/provider/scheduling measurements. Do not rebuild telemetry. |
| Release-process benchmark | Commits `0530fa638`, `2bd73d434`, `e9542b441`, `0080886ca`; existing `verification_journey` binary and `bench:verification:journey` mise task | Refactor its release harness, traffic and report. Current setup is Custom Drift plus one assertion Eval, with scheduled/manual Drift, custom rows and OTLP; it does not prove five-kind coexistence. Preserve the correctness journeys for removed traffic. |
| Workload-role and replay corrections | Current commits through `796166959` include workload-role provisioning, OTLP instance identity/client latency and same-batch Scribe group deduplication | Carry forward; reverify affected inherited contracts, do not reimplement them. |
| Verifier/baseline permit removal | Existing uncommitted runtime, metric documentation and regression-test edits remove verifier/baseline count permits; Operator delivery retains a separate pool | Preserve these edits as requested. Inspect and verify them after approval; do not repeat removal or treat uncommitted changes as reviewed/approved. |
| Direct supplied-input API | Current `components/verification/routes.rs` has only binding GET, asynchronous run POST and run GET | New work: typed execute-and-return operation plus shared-client/SDK/agent closure after contract approval. Existing manual/direct Drift enqueue is not synchronous execution. |
| New capacity/fairness evidence | No capacity command or five-kind queued/direct/noisy-tenant report exists in the current source | New work: scenarios below, current-candidate gate and opt-in benchmark. |

The earlier verifier-cap implementation session produced focused green checks;
its already-existing `/tmp/wyrd-remove-verifier-caps-test-wyrd.log` ends with
2,314 passing tests and 164 skipped for `mise run test:wyrd`. This is supporting
local evidence, not a final gate, lint result, performance result or task-review
approval. No tests or benchmarks were run during this planning update. Do not
claim the copied prior conversation's benchmark numbers as current-tree evidence;
its referenced `scratchpad/bench/` artifacts are absent here.

## Owners, Scope, Consumers, and Prohibited Changes

- `wyrd-testing` owns the release-process benchmark, resource capture and report;
  reuse the Bifrost query-capacity and release-server facilities.
- `wyrd-server` owns authenticated serving, audit, request orchestration,
  verification runtime telemetry and publication. Vala owns scoring; Skald owns
  provider invocation. Queued and direct execution reuse those engines.
- `wyrd-spec` owns typed public contracts and stable errors. `wyrd-client` owns
  the shared client operation; Rust, Python and TypeScript project it through
  their existing SDK boundaries. HTTP and any approved MCP projection require
  their real runtime/catalog proof.
- Existing language journey owners retain the full Service/Agent-to-Operator,
  Run, custom-data and OTLP correctness coverage. Removing unrelated custom-table
  and OTLP load from the capacity benchmark must not remove that proof.

Do not add another engine, scheduler, telemetry service, exporter, process-local
work ledger, provider protocol or Bifrost benchmark. No arbitrary user-code
execution, offline dataset executor, new Verifier kind, compatibility route or
new storage format. Production auth, audit, tenancy, deadlines, replay fences,
durability and trust-boundary input validation cannot be relaxed to improve
benchmark numbers.
Do not directly insert jobs or call in-process engines from the load driver.
Local provider/Operator fixtures replace external destinations, not the real
server, engines or storage dependencies. Do not weaken or disable failing gates.

Wyrd is deployed across scalable replicas. CPU/memory allocations and replica
counts describe the measured deployment, not fixed Wyrd capacity. This task adds
no admission controller, stage-specific permits, resource-sizing policy or new
global/tenant cap. Record existing controls when they affect results; do not
invent protective bottlenecks or tune limits against hypothetical exhaustion.
No Grafana dashboard, JSON, provisioning, dashboard tests, tenant dashboard API,
new monitoring service or infrastructure-metrics exporter is in scope. Metrics
and documentation support future dashboards; this task builds none.

## Concurrency Decision

- Remove whole-attempt global/per-tenant execution-count ceilings for Verifiers
  and baseline fitting. No experimental variant, replacement limiter, adaptive
  controller, stage-specific permits or new concurrency configuration.
- Preserve fair, work-conserving queued selection: eligible tenants receive
  turns, and an otherwise idle deployment can execute one tenant's work.
  Reuse the existing one-item-per-due-tenant rounds. Verify progress beyond one
  due-tenant batch and across replicas; local selection is not an equal-CPU or
  deployment-wide latency guarantee. A waiting judge must not monopolize a
  shared verifier permit and prevent another tenant's work from starting.
- CPU computation and suspended provider/storage work are different resource
  uses. Add no new CPU pool or verifier-type partition in this task. Measure
  preparation/scoring CPU cost, scheduling delay and retained memory. If an
  execution-pool change is justified by observed contention, bring that measured
  boundary back for a separate decision rather than inventing a blanket cap.
- Direct supplied-input requests execute through the shared scoring owners
  without a durable verification queue or synchronous-path concurrency cap.
  Exercise their contention with queued work; queued tenant rotation does not
  imply fairness for direct HTTP traffic.
- Retain authorization, tenant isolation, durable claim exclusivity, leases,
  replay fencing, execution deadlines, cancellation and tracked shutdown.
  Operator delivery remains separately scoped; this proposal does not change it.
- Capacity is measured for the deployed replicas and their actual allocations.
  Identify the first limiting resource as offered load increases. Any later
  restriction must name that resource and show measured improvement; this task
  prescribes no production concurrency number or automatic scaling policy.

## Telemetry Contract and Implementation Closure

### Ownership and classification

One concrete verification telemetry owner alongside the server runtime owns the
closed catalog, classification, attempt lifetime and elapsed-time accounting.
The exporter remains the existing Prometheus recorder; `app/metrics.rs` installs
buckets/registration. Reuse the Bifrost lifetime-accounting pattern, including
cancellation/drop cleanup, without creating a shared telemetry framework.

Vala owns preparation/scoring instrumentation and Skald owns provider spans.
Queued and direct orchestration pass the same correlation/classification into
those owners. Do not put all measurements in `VerifierRunner`: direct execution
must receive the same engine observations. Baseline fitting and Operator delivery
keep their own lifecycle; they are not fabricated verifier attempts.

Closed dimensions:

- `mode`: `queued`, `direct`.
- `kind`: `drift_psi`, `drift_spc`, `drift_custom`, `eval_assertion`,
  `eval_llm_judge`, `eval_other`, `unknown`.
- Judge presence takes precedence. An all-assertion Eval is `eval_assertion`;
  supported non-judge trace/agent/mixed graphs are `eval_other`. `unknown` is
  only for attempts whose exact spec cannot be resolved/classified. Do not
  claim those graphs meet the assertion latency objective.
- Queued `origin`: existing `schedule`, `manual`, `observation`.
- Queued `outcome`: existing `completed`, `retrying`, `exhausted`,
  `awaiting_trace`, `cancelled`, `timed_out`, `errored`, `released`, `deferred`,
  `stale_lease`, `settlement_failed`. Direct outcomes: `completed`, `cancelled`,
  `timed_out`, `errored`; no fabricated durable settlement state.
- `phase`: `load`, `input_read`, `prepare`, `engine`, `publication`, `settlement`.

Emit one attempt observation and at most one observation per applicable phase
per attempt, never a metric sample per task/record. Aggregate repeated phase
scopes by their elapsed interval union within the attempt; trace child operations
retain detailed timing. Overlapping parent/child phases remain non-additive.

Replace `implementation` with `kind` in the affected families; do not emit old
and new schemas simultaneously. Use `outcome`, not the existing inconsistent
failure-family rustdoc's `status`. No tenant, Card, Verifier, Run, request,
result, provider URL, prompt, input field or arbitrary error text as a metric
label. Exporter deployment/instance labels remain scrape configuration.

### Catalog grouped by operator question

Names below are exact. Preserve unspecified existing families/labels.

| Group | Family | Labels and observation boundary |
|---|---|---|
| Demand/progress | `wyrd_verification_run_attempts_total` | `kind,mode`; once per claimed queued attempt or admitted direct-runtime invocation, including retries as new attempts. HTTP validation/auth refusals before runtime entry remain HTTP metrics/audit. |
| Demand/progress | `wyrd_verification_run_failures_total` | `kind,mode,outcome`; unsuccessful execution outcomes. A failed judgment is `completed`, not a failure. Retain retry/exhaustion visibility. |
| Demand/progress | `wyrd_verification_active_runs` | `kind,mode`; tracked owned runtime executions; increment/decrement exactly once, including error, cancellation, stale lease and shutdown. Classification changes must not leak or double-count activity. |
| Queue | `wyrd_verification_queue_depth` | Existing `queue,status` only; no added per-kind joins or per-Verifier queue aggregation. |
| Queue | `wyrd_verification_queue_wait_seconds` | `kind,origin`; queued-only, existing PostgreSQL-measured claimable-to-claim wait. |
| Queue | `wyrd_verification_trigger_to_terminal_seconds` | `kind,origin,outcome`; queued-only, existing creation-to-durable-terminal meaning. `_count` is terminal settlement observations, not total attempts. |
| Runtime latency | `wyrd_verification_run_duration_seconds` | `kind,mode,outcome`; queued claim-to-settlement attempt duration; direct runtime-entry-to-result duration. Do not combine modes into a supposedly equivalent percentile. |
| Runtime latency | `wyrd_verification_phase_duration_seconds` | `kind,mode,phase`; phase definitions below; absent phases emit no samples, not zeros. |
| Engine overhead | **New** `wyrd_verification_engine_overhead_seconds` | `kind,mode,outcome`; per-execution engine elapsed time less the union of measured evidence/dependency wait intervals inside that engine. Includes preparation/scoring; wall time, not CPU. |
| Health | Existing schedule ticks, capability up/restarts, observation enqueue-failure metrics | Existing closed labels; document their meaning and keep them separate from completed-execution counts. |
| Delivery | Existing Operator active/attempt/duration families | Existing labels and delivery semantics; distinct from verifier execution. |
| API latency | Existing `wyrd_http_requests_total`, `wyrd_http_request_duration_seconds` | Existing `method,path,status`; matched route template, not raw URI. Direct server-edge latency includes authorization/audit and response construction, not client networking or body delivery after handler return. |

Use this bucket set, seconds, for run duration, queue wait, trigger-to-terminal,
phase duration and engine overhead. Do not change Bifrost, generic HTTP or
Operator buckets as collateral work:

```text
0.0005, 0.001, 0.0025, 0.005, 0.0075, 0.009, 0.01,
0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10, 30, 60, 300
```

### Phase meanings and overlap

| Phase | Meaning |
|---|---|
| `load` | Resolve the exact registered Verifier and required dependency identities/metadata. Include registry work; do not hide it inside scoring. |
| `input_read` | Authorized queued evidence retrieval through the existing reader. Direct supplied-input execution emits none. Normalize Eval/Drift evidence span meaning; identify streaming fold/scoring overlap explicitly. |
| `prepare` | Validate/normalize engine input, decode the spec/baseline and construct or retrieve prepared plans/selectors. Includes cold and warm preparation; do not remove preparation from the latency objective. |
| `engine` | Inclusive engine dispatch through judgment: evidence retrieval where applicable, preparation, scoring, dependency resolution and provider waits. This is the parent duration, not pure scoring or CPU time. |
| `publication` | Queued-only report encoding and durable publication through required acknowledgements. |
| `settlement` | Queued-only fenced durable state transition. |

Engine contains child phases/spans; Drift can compute during a streaming read.
Do not force input materialization or change scoring merely to create a linear
waterfall. Do not sum phase percentiles or subtract aggregate percentiles.

Measure each execution's wait intervals using one process-monotonic clock,
clip them to the engine interval and subtract their union once. Nested or
parallel waits cannot be counted twice. Wait scopes cover actual evidence IO,
baseline/dependency IO and provider invocation, not an entire mixed CPU/IO reader
scope. Synchronous folding/scoring stays measured as local work. Preparation
and incidental scheduling delay remain in residual elapsed overhead. CPU work
concurrent with an external wait may be hidden by this residual; profiles supply
CPU attribution. Never label the residual as all Wyrd CPU or infer it from a
configured mock delay. Missing wait accounting is incomplete evidence, not zero.

### Trace structure and live debugging

Keep one scrubbed queued `verification.attempt` or direct request execution
root. Required children are INFO-level spans (sampling controls export), not
DEBUG-only instrumentation. Propagate parent context into spawned and blocking
work; avoid routine success log lines duplicating the spans.

```text
verification.attempt / direct execute request
  verification.load
  verification.engine
    verification.evidence_read       # queued only
    baseline retrieval / decode
    plan/cache lookup / selector compilation
    assertion / PSI / SPC / Custom scoring
    judge preparation
      provider invocation(s)
  verification.publish               # queued only
    encoding / capability acquisition / transport acquisition / batch ACK(s)
  verification.settle                # queued only
```

Record kind/mode, exact version identities, attempt/outcome and stable error
code on the correlated trace. Retain authorized scrubbed correlation IDs, never
observation bodies, prompts, secrets, credentials or tokens. Cache hit/miss and
cold/warm preparation are trace fields initially, not additional metric families.

Measure queued spawn-to-first-execution delay from process-monotonic timestamps
and expose it in correlated traces/benchmark evidence. Label it task-start delay;
it is not every future wake-to-poll delay. Direct inline execution has no
fabricated task-start sample. Existing exporter/test-process Tokio busy time
must not be presented as release-server runtime CPU or scheduler delay.
Initial authorization/audit stays on the direct HTTP request trace; use that
request's correlation for runtime children. Failed HTTP admission creates no
phantom runtime attempt.

### Catalog documentation and future dashboard queries

Update the owning verification telemetry/operator documentation and architecture
with grouped family descriptions, exact labels, units, lifecycle and overlap
semantics, bucket bounds and bounded outcome definitions. Document these query
shapes against the real exporter; no Grafana deliverable:

```promql
sum by (kind, mode) (rate(wyrd_verification_run_attempts_total[5m]))

sum by (kind, mode, outcome) (rate(wyrd_verification_run_duration_seconds_count[5m]))

histogram_quantile(0.95,
  sum by (le, kind, mode) (rate(wyrd_verification_phase_duration_seconds_bucket{phase="prepare"}[5m])))

histogram_quantile(0.95,
  sum by (le, kind, mode) (rate(wyrd_verification_engine_overhead_seconds_bucket[5m])))

sum by (queue, status) (wyrd_verification_queue_depth)

histogram_quantile(0.99,
  sum by (le, kind, origin) (rate(wyrd_verification_queue_wait_seconds_bucket[5m])))
```

Counter queries count attempt observations; durable reconciliation remains SQL/
public result evidence. Aggregate selected replicas' histogram buckets before
quantiles; do not average replica p95s. Queue snapshots may repeat shared durable
state across replicas: select one queue observer, do not sum replicas. Explain
empty/no-sample series and counter resets. Examples use all selected instances;
document deployment/instance filtering as scrape configuration, not application
labels. Infrastructure CPU/memory metrics are not added by this task.

## CPU Profiling Contract

Bifrost's current release capacity harness supplies cgroup CPU/memory capture,
not a maintained perf workflow. This task adds the following explicit optional
benchmark workflow. It creates no production profiler service or new Rust
profiler dependency, and requires no flamegraph tool for completion.

- `mise run bench:verification:capacity`: ordinary unprofiled release run;
  authoritative throughput/latency and process resource measurements.
- `mise run bench:verification:capacity -- --profile`: diagnostic run; build
  the serving binary with line tables/frame pointers and capture each solo and
  mixed step for every serving replica. Never attach to the load driver, the
  `systemd-run` wrapper or an in-process test runtime.
- Add line tables/frame pointers for this diagnostic build only. Preserve other
  compiler flags and record the actual binary/build identity. Do not change
  workspace release/dist profiles or silently profile the authoritative run.

Document the equivalent diagnostic build:

```bash
CARGO_PROFILE_RELEASE_DEBUG=line-tables-only \
RUSTFLAGS="-C force-frame-pointers=yes" \
mise exec -- cargo build --locked --release -p wyrd-server
```

The benchmark obtains actual serving PIDs from the existing release-process
owner and controls the capture lifecycle. After warmup, start perf for each
replica; coordinate measured traffic with captures and record actual timestamps.
Stop capture at the end of that step, before drain/next-step traffic, await the
profiler and flush the artifact. Do not use manual PID discovery or require an
operator to time attachment. Capture startup/termination gaps are recorded;
a capture missing the measured step is incomplete evidence.

Equivalent per-replica commands, with the PID, artifact and duration supplied
by the harness rather than guessed by the caller:

```bash
perf record -F 99 -g --call-graph fp -p "$SERVER_PID" \
  -o "$STEP_PERF_DATA" -- sleep "$STEP_SECONDS"
perf report --stdio -i "$STEP_PERF_DATA" > "$STEP_REPORT"
```

If capture duration ends before traffic, the harness must end/mark that capture
as incomplete, not pretend the nominal sleep defines the actual measurement
window. Profiler processes are tracked and stopped on cancellation/shutdown.

Output: `target/verification-capacity/profiles/<step>/<replica>/` contains
`perf.data`, `report.txt` and metadata: PID/replica, step/kind/mode/load, actual
capture start/end, sampling frequency/unwinder, command/exit status, binary
identity and effective build flags. Retain the matching binary and symbols with
the run artifacts. Check that samples exist and Wyrd hot frames resolve; report
unknown/native frames instead of manufacturing attribution. Publish hotspot
summaries per solo step and mixed step; mixed profiles are process-wide, not
per-kind or per-span CPU counters.

Capture cgroup `usage_usec` deltas, CPU-seconds, mean cores and memory per step
and replica; capture driver CPU separately. CPU-ms/completed-run for a solo step
includes background work and is labelled accordingly. `perf` locates executing
code, spans locate elapsed waits, and cgroup deltas measure total CPU consumed.
Tokio busy duration and thread CPU measured across arbitrary async awaits are
not CPU attribution. External provider computation is outside server profiles.

Missing perf, permission, symbols, empty samples, an exited server or failed
profiler produces a diagnostic failure and nonzero `--profile` exit after
artifacts/cleanup; ordinary unprofiled runs remain available. Never change host
perf security settings automatically. Profiling failure blocks required profile
evidence, not correctness proof. Save separate profiled/unprofiled reports and
compare performance only among equivalent unprofiled deployments.

## Approach

1. Reconcile TASK-008's historical implementation evidence and recovery task
   against current source, registered tests and the final candidate. Carry
   forward completed work; close missing seams rather than rebuilding journeys.
2. Resolve the listed specification amendments, then instrument actual runtime
   owners so both execution paths expose preparation, scoring and external waits.
3. Add the initial direct API and shared-client/SDK closure using existing Vala
   supplied-record and supplied-batch execution, preserving ordinary observe and
   asynchronous run behavior.
4. Refactor the existing benchmark into one capacity command with queued,
   direct and concurrent queued/direct cases, solo/mixed and noisy/quiet-tenant
   comparisons, increasing offered load and drain. Reuse the release harness
   and provider fixture protocol; prove one- and multi-replica behavior.
5. Record baseline measurements, apply the smallest justified latency fixes,
   and rerun the same workloads. Use profiles for CPU hotspots and spans for
   waits; report the actual deployment and replica count with every measurement.
6. Finish the inherited proof matrix, generated/public documentation closure,
   final gate and opt-in benchmark evidence at the same integrated candidate.

## Ordered Implementation Scenarios

All executable changes use the following TDD scenarios after authority is
resolved. New tests are described by behavior rather than invented symbols;
the implementer must select their owning targets and record exact focused
commands before RED. Existing named regression commands appear below.

### Scenario 1 — Runtime telemetry identifies the actual execution costs

**Behavior.** Implement the exact Telemetry Contract above: grouped catalog,
closed labels, six defined phases, paired wait-union overhead, INFO-level
correlated spans and task-start delay. Emit from real queued and direct owners;
absence of queued-only phases is not a zero measurement. Preserve exact attempt,
terminal and active-lifetime accounting on every exit path.

**RED.** Extend production-path telemetry coverage to reject pooled verifier
measurements, missing preparation spans, incorrect outcome classification,
leaked active gauges and sensitive fields. Cover unknown/other/judge
classification, nested/concurrent waits, streaming CPU during reads and failed
judgments counted as completed execution. First prove the current paths cannot
produce the required per-type breakdown. Use the existing attempt telemetry
regression below as an anchor, not synthetic metric emission as proof.

**GREEN.** Emit the catalog and scrubbed spans from concrete owners, preserving
retries, cancellation, clock ownership and durable accounting. Scrape the real
release exporter to confirm family names, labels and bucket bounds; reconcile
samples with actual transitions. Document all family/query semantics.

**REFACTOR.** Reuse the current metrics/tracing setup and remove duplicate
successful logs or measurements while the production-path assertions stay green.

### Scenario 2 — Direct execution returns a judgment on supplied input

**Behavior.** Proposed `POST /v1/verification/execute` accepts an exact registered
Verifier, an authorized exact subject and typed bounded input. Eval uses a
supplied record; Drift uses supplied samples, with registered fitted baselines
for PSI/SPC. Return a judgment and typed details in the same response. A failed
judgment is not an execution error. The exact wire shape and transient response
identity are specification decisions, not decisions for the implementer.

The requested initial scope is PSI, SPC, Custom, assertion-only Eval and Eval
with LLM judges using supplied context. Reject unsupported archived-context
tasks before partial execution. Do not silently retrieve Bifrost evidence.
No durable verification enqueue, result publication or Operator dispatch occurs.
Add no direct-path execution-count ceiling. Registry/baseline/provider resolution and canonical authorization audit remain;
"no Bifrost evidence path" does not mean "no SQL or audit."

**RED.** Add real public-client/server journeys for each supplied-input variant,
exact version attribution, pass/fail judgments and missing/unready baselines.
The current router has no execute-and-return route. Assert that no durable
verification job/result or dispatch is created, and that no Bifrost evidence
operation is needed to complete the request.

**GREEN.** Serve the approved operation over existing Vala scoring entry points
and registered server dependencies. Project it through the shared client and
all three first-class SDKs; do not duplicate transport or scoring.

**REFACTOR.** Share meaningful preparation/scoring ownership between execution
paths while preserving their different input, durability and response semantics.

### Scenario 3 — Direct execution preserves trust and cancellation

**Behavior.** Exact-target `evals:run` authorization and tenant isolation apply.
Allowed/denied decisions use canonical transactional audit; append failure
fails closed. Invalid, oversized, wrong-tenant, incompatible or insufficient
inputs and unsupported task dependencies produce approved structured outcomes.
Reuse existing request/provider safeguards and timeouts; do not add an admission
layer or CPU/memory-derived cap. Timeout/disconnect releases owned work and
creates no detached durable job; already-issued provider
calls may have incurred cost. Retries cannot silently replay a judge request.

**RED.** Add public journeys for permission and cross-tenant refusals,
malformed/oversized data, baseline/schema mismatch, unsupported tasks,
existing refusal behavior, timeout and cancellation. Use an isolated supporting test for injected
audit-append failure where end-to-end forcing would obscure the boundary.
Confirm earlier successful execution scenarios remain green.

**GREEN.** Enforce approved input bounds, errors, permission scope and cancellation
through existing owners without bypassing SQL/audit or provider protections.

**REFACTOR.** Reuse existing validation and error catalog mechanisms. Do not
create a second permission cache or a new cancellation framework.

### Scenario 4 — Five verifier workloads genuinely coexist under load

**Behavior.** Proposed `mise run bench:verification:capacity` replaces the current
journey benchmark command and binary, with no duplicate long-term driver.
Setup publicly registers Services, all five Verifier cases and representative
fitted baselines. Measure queued production and direct API cases separately,
then together against the same runtime to expose interference.
Queued cases retain actual scheduled Drift and observation-triggered Eval;
manual activations, if used for a measured capacity slice, are explicitly
labelled and never presented as scheduler throughput.

Each path reports warmup separately, each case alone, all five together at
matched per-case rates, increasing offered load and drain. Default measurement
is approximately 30 seconds per step;
report actual duration/sample count and total run time. Register and seed once,
outside timing. State bytes, task count, sample/feature counts and baseline
identity. Run the same workload on one and multiple replicas through the existing
cluster/deployment harness and report aggregate and per-replica results. Record
tenant count and replica count; Verifier/baseline execution has no
16-process/4-tenant permit gates. A mixed step must
demonstrate overlapping execution and continuing
progress for every kind, not merely that all five were enqueued.

Include a noisy tenant with sustained queued work and a quiet tenant with
low-volume requests while judges wait on the mock. Quiet-tenant runs must start
and complete during the noisy traffic. Report each tenant's observed latency
and progress in the benchmark artifacts (not production metric labels). Exercise
more eligible queued tenants than one discovery batch to detect selection
starvation. Repeat on multiple replicas; preserve exclusive claims and reconcile
results across replicas. Also send low-volume direct requests during queued load
and queued work during direct load. Report deterioration without claiming a
strict fairness or latency bound the design has not established.

Use a delayed local OpenAI-shaped mock over trusted TLS for the release server.
Do not disable certificate validation, SSRF defenses or release restrictions.
The mock measures Wyrd overhead and concurrency, not real-provider capacity.

**RED.** Add a production-process smoke journey that requires all five kinds,
actual overlap, both execution paths and successful query/response reconciliation.
Require quiet-tenant progress during noisy traffic, progress across discovery
batches, cross-replica claim exclusivity and concurrent queued/direct execution.
It must expose the current Custom-plus-assertion-only benchmark as insufficient.
Prove the real release process can reach the judge fixture before a long run.

**GREEN.** Reuse release-server, Postgres, telemetry and fixture protocol support;
drive public SDK/HTTP paths. Maintain scheduled activation correctness evidence
when adding higher-rate manual measurements authorized by the amended scope.

**REFACTOR.** Retire unrelated custom-table/OTLP capacity traffic and duplicated
report plumbing, retaining their complete correctness journeys.

### Scenario 5 — Reports expose overload and calculate latency honestly

**Behavior.** Arrival scheduling is independent of responses with bounded driver
concurrency. Report offered, started, accepted, activated, terminal, rejected,
failed, missed and outstanding work at their actual boundaries. Drain each solo
case before the next; stop or account for scheduled arrivals during drain.
An observation ACK/202 is never a completed verification. Expected negative
judgments are distinct from execution failures. Loss, duplication, wrong-tenant
results or failed reconciliation fail correctness.

Emit Markdown and JSON at `target/verification-capacity/`, with one row per
path/step/kind: offered and achieved rate, p50/p95/p99 client latency, server
phase latency, exclusive Wyrd overhead, scheduling delay, failures, backlog and verdict. Record raw
client samples at microsecond precision; label histogram-derived server
percentiles as bucket estimates. Empty/low-sample measurements cannot pass a
latency target. Compute exclusive overhead from paired per-request provider
intervals before taking quantiles; concurrent waits use their interval union.
Never subtract aggregate p95s, summed concurrent task times, or a configured
mock sleep to infer overhead. Do not equate wall time or Tokio busy time to CPU.

Capture actual deployment replica count and resource allocations, per-replica
and aggregate CPU/memory, driver CPU, existing controls and instrumentation
settings. A local fixture's resource envelope is a reproducibility setting, not
a production requirement. Solo CPU includes background server work. Mixed-step process CPU is not
per-kind CPU; report it only at process/step scope. Identify where throughput
flattens or latency/outstanding work grows and the measured resource responsible;
if the data cannot identify the resource, mark the attribution unresolved.
Use symbolized CPU profiles for diagnostic solo/mixed windows; authoritative
capacity numbers come from unprofiled, info-level runs. A profile failure is
explicit missing evidence, not a fabricated CPU attribution.

**RED.** Add bounded driver/report tests that expose generator saturation,
cross-step contamination, missing/duplicated completions, expected failed
judgments, empty samples and incorrect percentile subtraction. Retain the
existing reconciliation and histogram-delta regressions below.

**GREEN.** Produce distinct correctness, coexistence and performance verdicts
from actual driver/server evidence. Check catalog/labels/buckets using production
scrapes, retain raw client samples and paired overhead measurements, and show
inclusive phase timings without adding them. Report capacity below target honestly; a
generated report by itself does not close performance acceptance.

**REFACTOR.** Keep one concise report with machine-readable evidence. Reuse
existing scrape/quantile/resource parsing and avoid a second telemetry ledger.

### Scenario 6 — Diagnostic CPU profiles identify actual server hotspots

**Behavior.** Implement the CPU Profiling Contract above: optional `--profile`,
diagnostic build flags, serving-PID attachment, warmup/step separation,
per-replica captures, symbolized text reports, metadata and cleanup. Keep ordinary
capacity runs unprofiled and maintain separate diagnostic artifacts.

**RED.** Add bounded benchmark-owner checks proving absent/wrong PIDs, failed or
empty captures, unresolved Wyrd symbols and cancellation cannot be reported as a
successful profile. The current harness has no profiling mode or coordinated
per-step capture. Select and record exact focused commands before RED; do not
require host perf privileges for ordinary unit coverage. A real opt-in release
profile is required separately to prove the actual attachment/capture path.

**GREEN.** Use existing release-process lifecycle plus external `perf`, with no
new profiler dependency or production instrumentation service. Run the opt-in
mode and verify samples, symbols, recorded windows and per-replica artifacts.
Also rerun unprofiled smoke/report coverage to prove normal mode stays ordinary.

**REFACTOR.** Keep profiling a narrow benchmark capability. Reuse process/resource
ownership and cleanup; no duplicate server launcher or custom flamegraph pipeline.

### Scenario 7 — Warm engine and publication paths avoid repeated preparation

**Behavior.** Establish a baseline first, then reduce observed overhead without
changing judgments, tenant isolation or durable semantics. Immediate candidates
are immutable prepared Eval plans/selectors (regex only where used), decoded
PSI/SPC baselines, provider/publisher transport reuse and enqueue notification
with recovery polling. Cache identity includes the exact Verifier/baseline
version and relevant schema/profile; memory is bounded and concurrent cold use
converges. Publish capability reuse must preserve tenant/resource scope and
expiry. A blanket per-tenant authorization cache is outside scope without an
approved revocation contract.

Enqueue notification follows committed durable work, retains multi-process
discovery/recovery and changes no PostgreSQL deadline predicates. The current
loop also wakes on attempt completion; do not claim an unconditional one-second
tax on every attempt. Publication still respects ACK ordering, sealed replay
identity and settlement fences. Custom scoring receives no speculative engine
or cache when shared preparation already addresses its cost.

**RED.** Add focused behavior checks for actual reused preparation/transport,
concurrent cold access, different tenant/version/schema isolation, bounded
eviction, expired capability handling and committed-enqueue wakeup/recovery.
Run the existing sealed-batch replay regression before and after publication
changes. For each chosen optimization record the baseline span/profile cost.

**GREEN.** Apply the minimum verified reuse/wakeup changes in owning structs and
rerun the identical solo/mixed profiles. Record each measured improvement and
any trade-off; do not present an unmeasured optimization as achieved latency.

**REFACTOR.** Prefer existing concrete caches and transports; no single-use
traits, speculative configuration, duplicate scoring or benchmark-only fast path.

## Inherited TASK-008 Proof Closure

Audit the original task's Implementation Evidence and the recovery task first.
Historical PASS rows identify reusable coverage, not fresh proof of this final
candidate. Do not repeat already-complete implementation. TDD is not applicable
to static closure or rerunning already-correct behavior; new/changed executable
tests and seam fixes still require an expected RED and the scenario discipline.

Every row below needs a current owner, source/test or static evidence, exact
command where a test is named, final result and any remaining blocker:

| Inherited obligation | Owner / strongest proof |
|---|---|
| PSI/SPC/Custom/assertion/judge Service/Agent → result → failed-verdict Operator, exact principal, SDK parity | Rust/Python/TypeScript real SDK/server journeys; REQ-101, AC-017, AC-020 |
| Locked Run identity, typed/mapping input parity, native projection, bounded IPC, custom-table reuse and shutdown | Shared client plus all three SDK journeys; AC-017 and inherited observation obligations |
| Python framework-span enrichment, joins, nested scopes, same-run asyncio isolation, private provider and optional-OTEL fail-open behavior | Python runtime journeys; REQ-151, AC-032; no inference of log/metric enrichment |
| Existing HTTP/MCP binding/direct Drift runs, requester identity, nullable ownership, idempotency, authorization and result queries | Existing manual/status journeys and runtime MCP catalog; REQ-135–REQ-137, AC-030 |
| Worker without local Scribe, SYSTEM tenant/table capability matrix, public SYSTEM credential/principal refusals and shared observation fanout | Real multi-server SDK/Bifrost journeys; AC-023 |
| Daily partitions, real Bloom/row-group pruning, exact Eval record-day lookup and event time across midnight ACKs | Physical storage/query journeys; AC-024 |
| Record/batch/cron duplicates, identical sealed replay, expired leases, retries, fail-open Eval enqueue, partial ACKs and summary-only writes | Runtime/SQL plus real public replay/restart journeys; REQ-089, AC-020 |
| PostgreSQL-owned activity/schedule/claim/lease/retry/dispatch times and restart fencing | DB-backed coordination tests and runtime journeys; REQ-152, INV-015, AC-033 |
| Verifier execution beyond former ceilings, retained Operator limits/timeout, shutdown/reclaim and supervised health | Runtime/public concurrency and delivery journeys; REQ-115, REQ-146 |
| Allowed/denied authorization audit at public/Gate boundaries, no audit for internal mechanics | Auth/Gate/SQL journeys; REQ-145, AC-030 |
| Operator CRUD/rotation/redaction, real Slack/PagerDuty/HTTP protocol fixtures, SDK/CLI/MCP parity and multi-replica secret rotation | Existing connection and delivery journey owners; inherited Operator obligations |
| Qualifying API-key/workload exchanges activate owners; delegation, human refresh, card-free automation, SYSTEM, cached requests and idle expiry do not; suspension and permission snapshots remain correct | Auth/activity Postgres and SDK journeys |
| Current Verifier vocabulary, no retired routes/tables/crates/checks, generated contracts, source registration and architecture/public docs | Static/source/codegen/catalog/served OpenAPI closure; REQ-114, AC-021, AC-022 |

Credentialed cloud/provider smokes and official-image qualification remain
separately identified release evidence; do not silently delete their inherited
obligations or claim local mock evidence proves them.

## Acceptance Criteria

- **CLOSE-01:** Every inherited TASK-008 obligation has current strongest-tier
  proof or an explicit unresolved blocker; historical prose alone is not closure.
- **CLOSE-02:** Approved direct execution returns results for the five initial
  cases, preserves security/audit/cancellation, and has Rust/Python/TypeScript
  real-client journeys, served-contract proof and the approved agent projection.
- **CLOSE-03:** The single capacity command proves all five cases overlap and
  progress, for queued production and direct paths separately and together,
  with solo/mixed load and one-/multi-replica evidence. Quiet tenants start and
  complete work during noisy traffic; eligible tenants beyond one discovery
  batch progress; cross-replica claims remain exclusive. Report quiet-tenant
  latency without inventing an unapproved slowdown threshold. Existing
  correctness journeys remain registered.
- **CLOSE-04:** Reports reconcile work, show actual demand/throughput/backlog and
  per-type p50/p95/p99, include the resource envelope, and correctly separate
  paired engine overhead from evidence/dependency/provider waits. Phase overlap
  is explicit; actual task-start delay is not mislabelled full scheduler delay.
  Missing data never yields PASS.
- **CLOSE-05:** Agreed objective: warm preparation plus scoring p95 <10 ms
  for assertion Eval, Custom, PSI and SPC at declared sustainable load and
  declared sample/feature sizes, solo and mixed. This is not a universal bound
  for arbitrary input sizes. Report full direct-request latency and cold
  preparation separately; measure engine overhead using the defined per-execution
  wait-interval union. For non-judge cases this includes preparation and scoring;
  show provider waits separately for judges. Finalize the reference workloads
  and strict-threshold proof in the spec amendment. Report mixed slowdown and saturation without inventing a
  contractual threshold or claiming a pass merely because a report exists.
- **CLOSE-06:** Chosen latency fixes have equivalent correctness and measured
  before/after evidence on stated deployments. No speculative admission or
  resource-cap work, CPU pool or verifier-type partition is introduced. Existing
  correctness/security protections and separately scoped Operator behavior remain
  intact; verifier/baseline execution has no count ceiling.
- **CLOSE-07:** Final integrated gate and opt-in benchmark pass required
  correctness/approved performance criteria at the reported candidate. Diagnostic
  profiles explain CPU hotspots; scrubbed spans explain latency/waits.
- **CLOSE-08:** The grouped production metric catalog exactly matches the stated
  names/labels/buckets and lifecycle/phase semantics, covers queued/direct paths,
  and has real-transition/exporter proof plus operator docs/PromQL. Gauge cleanup
  and wait-union accounting survive failure/cancellation. No Grafana assets,
  provisioning or dashboard tests are delivered.
- **CLOSE-09:** Optional `--profile` produces nonempty, symbolized per-step,
  per-replica server captures and hotspot reports with build/PID/window metadata.
  Missing evidence fails diagnostic mode explicitly; no automated security
  bypass or interpretation of profiled numbers as authoritative capacity.

## Expected Write Set and Consumer Closure

Likely owners: `crates/wyrd/wyrd-testing/src/bin/verification_journey/` refactored
into the capacity driver; existing release-server and telemetry support;
`crates/wyrd/wyrd-testing/Cargo.toml`; opt-in registration in `mise.toml`;
`crates/wyrd/wyrd-server/src/verification/`, metrics and verification routes;
Vala Eval/Drift preparation owners; `crates/wyrd-spec` wire/error/schema sources;
shared-client verification surface; all three SDK projections and journey homes;
existing server/MCP integration owners; generated artifacts from their sources;
owning architecture, grouped metric/operator documentation and public docs;
benchmark-only external-perf capture support. No Grafana files or provisioning.
Paths are guidance,
not a private implementation allowlist. No unrelated Bifrost redesign is implied.

## Verification and Evidence

For each newly selected test, record and run the exact focused command before
RED and after GREEN. Confirm package/target/selector from source and nextest
listing; never use a filter that can silently select no tests. Language-runtime
behavior stays in its owning Python/Node tests. Diagnose failures with tracing
enabled and actual logs before changing code.

Existing focused anchors, confirmed in the current source/manifest:

```bash
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E 'test(=attempts_record_queue_wait_phases_terminal_latency_and_one_trace)'
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E 'test(=verifier_runs_execute_beyond_the_former_permit_ceilings)'
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E 'test(=baseline_fits_do_not_wait_for_verifier_executions)'
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E 'test(=expired_lease_is_reclaimed_and_the_stale_holder_is_fenced)'
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_operator_delivery -E 'test(=verifiers_progress_while_operator_deliveries_are_capped)'
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E 'test(=lost_result_ack_replays_the_identical_sealed_batch_and_scribe_deduplicates)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin verification_journey -E 'test(=report::tests::reconciliation_refuses_every_mismatch)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin verification_journey -E 'test(=report::tests::quantile_reads_bucket_deltas)'
```

The last two commands are current-target iteration anchors; after renaming the
binary, update them to the actual confirmed target/selectors and retain equivalent
report proof. Do not keep the obsolete binary solely to preserve commands.

This is whole-change closeout with runtime, SDK, contract and test-infrastructure
scope. Final verification uses `mise run gate` once, not a redundant list of its
component lanes. Current gate includes Rust families, Bifrost/gateway and language
journeys, codegen, docs and boundary checks. Verify newly added journeys and
served OpenAPI/runtime MCP coverage are actually registered there. The benchmark
is explicitly outside gate; its proposed command must be implemented before use:

```bash
mise run gate
mise run bench:verification:capacity
mise run bench:verification:capacity -- --profile
git diff --check
```

Record candidate commit/diff, commands and features, setup versus measurement
time, workload/input sizes, replica counts, server/driver resources, existing controls, raw sample/report
paths, metric snapshots, scrubbed correlated traces, baseline/optimized results,
profile commands/artifact paths and every acceptance row's result. Profiling
uses the same release configuration with symbols/frame pointers where required;
record diagnostic overhead and do not use profiled runs for authoritative
capacity claims. No compiler/profiler/test was run during this planning update.

Metric catalog/query documentation is static proof: inspect it against the real
scraped exporter and production-transition tests, without a manufactured RED for
prose. Reuse existing catalog tests; add no permanent checker that only verifies
another check. Existing green regressions above are verification-only resume
anchors: revalidate them, do not manufacture a new RED or reimplement working behavior. Apply TDD only to missing
behavior or actual corrections.

## Material Stop Conditions

Before implementation, route these known conflicts through `$wyrd-spec`:

1. REQ-135 permits only three Verification HTTP operations and REQ-136 locks
   asynchronous enqueue semantics; approve the separate direct route's exact
   input/output, supported tasks, transient identity/durability, permissions,
   retries, bounds/deadlines and SDK/agent projections. Ordinary observations and
   existing start-run remain asynchronous.
2. Reconcile the working revision-50 spec edits with the caller's explicit
   earlier statement that code/spec editing was unapproved and subsequent
   alignment with this planning recommendation. The decision removes
   Verifier/baseline count ceilings, preserves fair selection and adds no
   replacement admission or CPU-pool policy. Existing code edits remain in
   place but do not establish authority. Retain durable claims, fencing,
   deadlines, shutdown and telemetry; Operator delivery remains separate.
3. Carry the caller-aligned telemetry/profiling contract above into the spec
   amendment; do not redesign it in implementation. Finalize reference workload
   rates/input sizes and the strict latency acceptance proof. Observed mixed
   slowdown is reported; no strict slowdown/fairness SLO or Grafana deliverable
   is authorized.
4. The recovery task requires the old fixed traffic profile, prohibits synchronous
   executors and substituting manual Drift for scheduled Drift. Align its scope
   with this closeout before formally superseding it; preserve scheduled
   production correctness while clearly labeling capacity activations.

Telemetry ownership, grouping, dimensions, interval semantics and perf workflow
are resolved by the caller-aligned sections above. Do not reopen them as private
implementation choices. Direct API wire/error/identity semantics and reference
performance workloads remain material spec closure, not guesses for an agent.

During implementation, stop affected work for unresolved public/persisted
contracts, changed audit/revocation/tenant semantics, new dependencies/features,
altered durability/replay/coordination ownership or required weaker proof.
Trusted local TLS fixture support is reversible harness work when it preserves
security; needing a verification/network-policy bypass is a material blocker.

## Authority Links

- [Working specification; historical approved baseline is revision 49](../spec.md)
- [Original TASK-008 and historical evidence](TASK-008-integrated-verification-proof.md)
- [TASK-008 recovery](task-008-recovery.md)
- [TASK-012 integration closure](../integration/TASK-012-replay-operator-connections-and-delivery.md)
- [Repository standards](../../../../AGENTS.md)
- [Agent rules](../../../../architecture/agent-rules.md)
- [Wyrd protocol](../../../../architecture/wyrd-design.md)
- [Wyrd doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Bifrost architecture and telemetry](../../../../architecture/bifrost-design.md)
- [Security posture](../../../../architecture/wyrd-security-posture.md)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)
- [Telemetry observations](../../../../architecture/references/domain/telemetry-observations.md)

Primary profiling references for the documented commands:

- [Cargo debug profiles](https://doc.rust-lang.org/cargo/reference/profiles.html#debug)
- [Cargo environment variables](https://doc.rust-lang.org/cargo/reference/environment-variables.html)
- [rustc frame pointers](https://doc.rust-lang.org/rustc/codegen-options/index.html#force-frame-pointers)
- [perf record](https://raw.githubusercontent.com/torvalds/linux/master/tools/perf/Documentation/perf-record.txt)
- [perf report](https://raw.githubusercontent.com/torvalds/linux/master/tools/perf/Documentation/perf-report.txt)

## Implementation Evidence (in progress)

| Item | Implementation | Verification | Result |
|---|---|---|---|
| Verifier/baseline permit removal (rev 50), Operator permits retained | `89214267d` | six focused anchors listed under Verification and Evidence | PASS |
| Scenario 1, queued path: closed `kind`/`mode` labels, `input_read`/`prepare` phases, engine overhead from per-execution wait union, per-kind active gauge with drop cleanup, task-start delay on the attempt span, verification bucket set | `35d35c8ec`; `verification/telemetry.rs` (`ExecutionTelemetry`, `WaitSink`, `StreamWaits`), runner, Drift/Eval engines, `app/metrics.rs` | `cargo nextest run -p wyrd-server --features test-support --lib -E 'test(/verification::telemetry::tests::/)'` (classification precedence, nested/concurrent waits, streaming fold time); anchor `attempts_record_queue_wait_phases_terminal_latency_and_one_trace` RED on the pooled label, then GREEN | PASS |
| Scenario 1 catalog/PromQL docs | `1e8da536a`; `running-the-server.svx`, `telemetry-observations.md` | `mise run docs:check` | PASS |
| Scenario 1, direct path and real release-exporter scrape | blocked on revision 51 (direct API) and the capacity benchmark | — | OPEN |

Combined lane `--lib --test pg_verification_runtime --test pg_operator_delivery`:
523/523 on two consecutive runs with `WYRD_LOG=info`.

Diagnosis, `baseline_fits_do_not_wait_for_verifier_executions`:
- **Symptom:** under the full lane it intermittently asserted `Building == Failed`.
- **Evidence:** `fitter.rs:224` commits the claim (state `Building`) before a separate transaction fails it (`fitter.rs:256`). The test polled only while the state was `Pending`.
- **Cause:** the test read the intermediate `Building` state.
- **Fix site:** the test's wait loop now waits for a terminal state. An independent read-only diagnostician reached the same cause and fix; no other test has this pattern.

Unreproduced: one combined-lane run failed the telemetry anchor
`attempts_record_queue_wait_phases_terminal_latency_and_one_trace` at its first
assertion. Its output was not captured, and four later runs with tracing passed.
If it recurs, capture the trace before changing the test. The real Drift run's
evidence read retrying under load is a candidate cause, not a diagnosis.

## Proposed Specification Revision 51 (awaiting human approval)

Not authoritative until approved. It closes Material Stop Conditions 1, 3 and
4. Condition 2 is closed by revision 50.

**REQ-135 amendment.** Add a fourth Verification operation:
`POST /v1/verification/execute`. Ordinary observations and `POST
/v1/verification/runs` stay asynchronous (REQ-136 unchanged).

**Request.** The target is exact and untagged: `{ verifier_uid,
subject_card_uid, input }`. `input` is tagged by `kind`:
- `eval_record { context: object, media?: [MediaRef] }` for assertion-only and
  LLM-judge Evals. The judge receives the supplied `context`.
- `drift_samples { columns: { <feature>: [number | string | null] } }`. PSI and
  SPC score against the Verifier's `ready` fitted baseline. Custom scores the
  mean of `profile.metric_name`.

**Response `200`.** `{ execution_id, verifier: CardRef, subject: CardRef, kind,
verdict: passed | failed | inconclusive, summary, counts, detail: { drift:
DriftReport } | { eval: EvalReport } }`.
- `execution_id` is a UUIDv7 that appears only in the response, the audit row
  and the trace. It is never persisted or queryable.
- A `failed` verdict is a `200`, not an error.

**Not performed.** No durable run, published result, Operator dispatch,
Bifrost read or write, or Eval sampling policy. Registry, baseline and judge
Agent/Prompt resolution and canonical audit still use PostgreSQL.

**Authorization.**
- Requires `evals:run` with exact Verifier and subject scope, as for the
  direct-target `POST /runs`.
- One allowed or denied decision is audited transactionally per request. An
  audit-append failure refuses the request.
- Cross-tenant or unknown targets return `404`.

**Bounds.**
- Request body at most 1 MiB.
- `drift_samples` at most 64 columns × 100,000 values.
- `eval_record.context` at most 256 KiB.
- One 60 s execution deadline per request. No concurrency cap or admission
  layer.

**Stable errors.**

| Status | Code | Condition |
|---|---|---|
| 400 | `verification_input_invalid` | Malformed input |
| 413 | `verification_input_too_large` | A bound is exceeded |
| 403 | existing RBAC code | Permission denied |
| 404 | `verification_target_not_found` | Unknown or cross-tenant target |
| 409 | `verification_baseline_not_ready` | No `ready` fitted baseline |
| 409 | `verification_baseline_legacy` | Baseline fitted under an earlier format |
| 422 | `verification_input_incompatible` | Missing feature or type mismatch |
| 422 | `verification_input_unsupported` | Eval with trace or agent assertions, refused before any task runs |
| 502 | `verification_dependency_failed` | Judge provider failure after the task's own `max_retries` |
| 504 | `verification_execution_timed_out` | The 60 s deadline elapsed |

**Retries and cancellation.**
- The operation is not idempotent and takes no `Idempotency-Key`.
- SDKs never retry it automatically.
- A client disconnect drops the handler and cancels in-flight work. Provider
  calls already issued may have incurred cost.

**Projections.**
- `client.verification.execute(...)` in `wyrd-client` and in the Rust, Python
  and TypeScript SDKs.
- An MCP write tool `verification_execute` gated on `evals:run`.
- The served OpenAPI document.
- Telemetry uses `mode="direct"` with only `load`, `prepare` and `engine` phases.

**CLOSE-05 reference workloads.** One release `wyrd-server` replica plus a
two-replica repeat.

| Case | Workload |
|---|---|
| Assertion Eval | 4 assertion tasks over a 2 KiB context |
| Custom | 1 metric, 1,000 samples |
| PSI | 8 numeric features × 1,000 samples, 10 quantile bins, baseline fitted from 10,000 rows |
| SPC | 4 features × 1,000 samples, subgroup size 5, baseline from 10,000 rows |
| LLM judge | 1 judge task plus 1 assertion against the local TLS mock with a 200 ms delay |

**Sustainable load** is the highest offered step whose achieved rate is at
least 95% of offered and whose outstanding work drains within one step.

**Strict proof** is direct-mode paired per-request engine overhead below 10 ms
at p95. It requires at least 1,000 samples per case per step, solo and mixed,
at sustainable load, for the four non-judge cases. Judge cases report overhead
and provider waits separately with no threshold.

**Capacity benchmark defaults.**
- 30 s steps.
- Offered totals of 10, 25, 50, 100 and 200 executions/s, split evenly across
  the five kinds.
- 1 noisy tenant, 1 quiet tenant and 70 background tenants. This exceeds one
  64-tenant discovery round.

**Recovery-task alignment.** The capacity benchmark replaces the recovery
task's fixed observation-traffic profile and its prohibition on synchronous
execution. Scheduled Drift and observation-triggered Eval stay in queued
capacity traffic. Manual activations are labelled and never counted as
scheduler throughput. Existing correctness journeys keep the removed
custom-table and OTLP coverage.
