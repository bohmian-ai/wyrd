---
id: TASK-008-CLOSEOUT
kind: implementation
status: proposed
caller_approval: approved
planning_result: SPEC_REVISION_REQUIRED
spec: SPEC-verified-change-contract
spec_revision: 49
requirements: [REQ-089, REQ-101, REQ-114, REQ-115, REQ-135, REQ-136, REQ-137, REQ-145, REQ-146, REQ-151, REQ-152, INV-015, AC-017, AC-020, AC-021, AC-022, AC-023, AC-024, AC-030, AC-032, AC-033, REQ-172, REQ-173, REQ-174, REQ-175, REQ-176, REQ-177, INV-019, AC-041, AC-042, REQ-178, REQ-179, REQ-180, INV-020, AC-043]
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
mise exec -- cargo nextest run --locked -p wyrd-testing --bin verification_capacity -E 'test(=report::tests::reconciliation_refuses_every_mismatch)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin verification_capacity -E 'test(=evidence::tests::quantile_reads_bucket_deltas)'
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

## Amendment A — Client ingestion throughput and memory (spec revision 53)

### Outcome and value

The capacity benchmark's Drift seed exposed the client queue as the
bottleneck. This amendment makes the shared client queue meet REQ-172 to
REQ-177 and INV-019, and proves AC-041 and AC-042, inside this task.

Already committed in `9e6464d2d`:
- intake no longer awaits sends;
- all-or-none record admission, used by Drift;
- sealing headroom;
- ambiguous gRPC outcomes retained.

The amendment finishes the memory model, batching, concurrency, SDK override,
journeys, and throughput proof, then resumes the closeout benchmarks.

### Owners, scope, and prohibited changes

**Owners:**
- `crates/shared/wyrd-queue`: budget, producer, staging, sealing, sends;
- `crates/shared/wyrd-client/src/bifrost`: `WriterPool` and facade;
- the SDK budget option in `sdks/wyrd-sdk-python` and `sdks/wyrd-sdk-ts`;
- a throughput benchmark in `crates/wyrd/wyrd-testing`.

**Prohibited:**
- blocking or awaitable admission;
- inter-batch ordering machinery;
- any server or Scribe change, unless AC-041 proves the server is the limit,
  in which case stop and report;
- new Python or TypeScript queue settings other than the byte budget;
- a compatibility alias for removed `QueueConfig` fields.

### Approach

1. Replace the per-producer partition, fixed-slot charge, `ArrayQueue`
   staging, producer-count limit, and row clamps with one shared byte budget
   charged per admitted byte. Default 256 MiB; configured values honoured.
2. Seal on `max_message_bytes` or a 5 ms linger since the first staged row;
   flush and shutdown seal immediately.
3. Allow up to `max_in_flight` concurrent sends per producer with stable batch
   UUIDs; retained ambiguity retries without blocking other sends.
4. Expose the budget override on Python and TypeScript `start_bifrost` and
   Bifrost connect; refuse too-small budgets at connect.
5. Add the three-SDK 1,000×9 Drift burst journeys and the AC-041 benchmark.
   Set the `max_in_flight` default from it.
6. Fix the capacity benchmark seed to resubmit on `QUEUE_FULL` after flush and
   to count `SAMPLES × 9` tenant-scoped rows. Resume the closeout benchmarks.

### Scenario A1 — One shared budget, no caps, no preallocation

**Behavior.** One handle writes through at least 1,000 table producers. An
idle producer reserves no budget bytes. A producer may stage up to the whole
admission budget. Configured capacities are not clamped. (REQ-172, REQ-173,
INV-019)

**RED.** These wyrd-queue and wyrd-client tests fail today on the 64-producer
limit and the fixed-slot charge:
- one handle registering 1,000 tables;
- an idle producer's `owned_bytes == 0`;
- a single producer admitting more than 465 rows.

**GREEN.** Shared byte budget only; staging grows with admitted rows. Rerun
the all-or-none and sealing-headroom tests.

**REFACTOR.** Delete `ProducerCapacities`, `MAX_LIVE_ENTRIES`, and the row
clamps, along with their accounting and tests that pinned them.

### Scenario A2 — Byte and linger sealing

**Behavior.** A producer seals when staged bytes reach `max_message_bytes` or
5 ms after its first staged row. A record larger than the admission budget is
`PAYLOAD_TOO_LARGE`. (REQ-174, REQ-176)

**RED.** Tests that fail on the current row-count and 1 s triggers:
- a batch seals at the byte target without a flush;
- a single row seals within the linger;
- a record larger than the budget is refused as too large.

**GREEN.** Byte and linger triggers replace `flush_max_rows` and
`flush_interval_ms`.

**REFACTOR.** Remove the row-count trigger code and configuration.

### Scenario A3 — Concurrent sends with stable identity

**Behavior.** With a held sink, up to `max_in_flight` batches are in flight at
once. Intake continues while the budget has room. A retained ambiguous batch
retries under the same UUID while other batches proceed. (REQ-175, REQ-177)

**RED.** These fail with one send in flight:
- a held sink observes `max_in_flight` concurrent batches;
- a retained batch does not block a later batch's ACK.

**GREEN.** Bounded concurrent sends in the producer task. Rerun the gRPC
retention tests and `public_sdk_owned_batch_timeout_retry_retains_then_deduplicates`.

**REFACTOR.** Keep one owner task per producer; no new trait.

### Scenario A4 — SDK budget override and three-language bursts

**Behavior.** Rust, Python, and TypeScript accept a byte-budget override,
refuse one too small to seal a message, and survive an uninterrupted
1,000×9 Drift burst with resubmit-after-flush on `QUEUE_FULL`. Each reads back
exactly 9,000 rows, 1,000 `record_id`s, and 9 rows per id. (REQ-172, REQ-176,
AC-042)

**RED.** New journeys in `sdks/wyrd-sdk-rust/tests/observe_run.rs`, Python
`tests/integration/state/test_observe_journey.py`, and TypeScript
`wyrd/tests/integration/observe-run.test.ts` fail on the missing option.

**GREEN.** Add the option through `wyrd-client` and the SDK wrappers, then
regenerate stubs.

**REFACTOR.** None beyond the wrappers.

### Scenario A5 — 50,000 rows/s sustained

**Behavior.** AC-041. A release benchmark runs 500 obs/s × 100 features for
60 s with defaults against a real server, Postgres, and RustFS.
- It must show zero refusals, flat client bytes, drain within 1 s, and
  exactly 3,000,000 rows with 100 per id.
- It reports a 1,000 obs/s step, a 50 ms ack-delay step, batch sizes, send
  latency, CPU per row, and `max_in_flight`.

**RED.** The run on the current queue misses the bar; record the numbers.

**GREEN.** Choose the smallest `max_in_flight` that passes and make it the
default.

**REFACTOR.** Reuse the capacity benchmark's server harness; no
benchmark-only fast path.

### Acceptance criteria

- REQ-172 to REQ-177, INV-019, AC-041, and AC-042 each have passing evidence in
  the table below.
- Existing `wyrd-queue`, `wyrd-client`, and SDK journeys stay green.

### Verification

**Format and lints:** `mise run fmt`, `mise run lints`, `mise run py:format`,
`mise run py:lints`.

**Tests and checks:**
- `mise exec -- cargo nextest run --locked -p wyrd-queue`
- `mise exec -- cargo nextest run --locked -p wyrd-client --lib`
- `mise run test:bifrost:journey:sdk`
- `mise run test:bifrost:journey:observe`
- `mise run py:test:integration`
- `mise run ts:test:integration`
- `mise run codegen:check`

**Benchmark:** the AC-041 benchmark command, recorded with its report.

### Stop conditions

- AC-041 fails because the server cannot ingest 50,000 rows/s.
- Meeting AC-041 would require ordering, blocking admission, or a
  persisted-format change.

## Amendment B — Gateway capture writer (spec revision 54)

### Outcome and value

Gateway capture stops minting tenant tokens and writing through one embedded
client per tenant over the server's own public gRPC listener. Each server
process owns one capture writer: in-process to a local Scribe, otherwise a
capture-only peer ingest RPC over the mTLS peer plane (REQ-178). Capture is a
server-internal write with no token, permission, or audit decision (REQ-179),
delivered before the call's deadline or dropped with a counted reason
(REQ-180). Proves INV-020 and AC-043. The peer-mode journey runs on the same
topology as this task's 1- and 2-replica smoke.

### Owners, scope, and prohibited changes

**Owners:**
- `crates/wyrd/wyrd-server/src/components/gateway/capture.rs`: the writer;
  `invocation.rs` settle path and `app/server.rs` shutdown consume it;
- `crates/wyrd/wyrd-tonic/proto/wyrd.v1.proto` and `wyrd-server` `grpc`: the
  capture peer service, mounted only when Scribe runs in the pod;
- `crates/vala/vala-bifrost-redux` Gate: the reserved-table refusal. Scribe
  keeps readiness, schema, dedup, and backpressure;
- auth issue, verify, and builtin-role crates: removal of the capture token
  and role.

**Reuse:** `AuditPublisher`'s in-process `Scribe::ingest_frame` with a
constructed `Principal`; `BifrostPeerTls`, `ClusterRegistry::live_scribes`,
and the `RegistryTailStreamDiscovery` channel pattern for the peer path.

**Remove:**
- `CaptureTokenSource`, the per-tenant producer map and construction lock,
  `MAX_CAPTURE_TENANTS`, `CAPTURE_CLIENT_BYTE_LIMIT`, and the capture
  producer, backlog, and retry gauges;
- the capture shutdown drain;
- `issue_gateway_capture_access_token`, capture token verification,
  `GATEWAY_CAPTURE_TOKEN_MAX_TTL_SECONDS`, `GATEWAY_CAPTURE_ROLE`, and
  `gateway_capture_permissions`;
- the renewable-credential client plumbing if capture is its only production
  consumer.

`GATEWAY_CAPTURE_PRINCIPAL` stays as the identity stamped on captured rows.

**Prohibited:**
- a general-purpose internal ingest RPC;
- any capture token, permission check, or audit decision;
- per-tenant capture state or an in-memory capture backlog;
- moving verification `ResultPublisher` onto this path.

### Approach

1. Replace the per-tenant registry with one writer selected at boot from pod
   topology; retain peer TLS for the peer variant.
2. Add the capture-only peer service, its allowlist checks, and its mount.
3. Retry retryable refusals with the queue's backoff inside the existing call
   deadline; map every other outcome to a counted `CaptureDrop`.
4. Make Gate refuse every public write to `vala.gateway.calls`; delete the
   capture token, role, and verification surfaces.
5. Rewrite `architecture/wyrd-design.md` (token planes, principal model),
   `architecture/bifrost-design.md` (reserved table, peer plane), and
   `architecture/wyrd-security-posture.md` (capture identity).

### Scenario B1 — In-process capture with a local Scribe

**Behavior.** With Scribe in the pod, a gateway call's capture lands in its
tenant's `vala.gateway.calls` and `vala.traces.spans` without a token, stamped
with the capture principal. A resubmitted batch is absorbed by dedup.
(REQ-178, REQ-179, REQ-180, AC-043)

**RED.** Rework `gateway_capture_follows_policy_and_never_affects_the_call` to
read capture rows back through Bifrost instead of a mock sink. It fails while
capture still dials the public listener with a minted token:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib --run-ignored=all -E "test(=components::gateway::pg_invocation_tests::gateway_capture_follows_policy_and_never_affects_the_call)"'
```

**GREEN.** One writer submitting canonical batches to the local Scribe with a
deterministic tenant/call/table batch id.

**REFACTOR.** Delete the per-tenant map, token source, gauges, and drain.

### Scenario B2 — Deadline-bounded delivery

**Behavior.** Backpressure retries until the call deadline, then drops as
`Saturated`. Other failures drop with their reason. The call result never
changes. (REQ-180, AC-043)

**RED.** A unit test where Scribe refuses with backpressure past the deadline
expects a counted `Saturated` drop and fails without the retry mapping. These
stay green:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib --run-ignored=all -E "test(=components::gateway::pg_invocation_tests::gateway_capture_work_ends_at_the_call_deadline)"'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=compatible::openai_compatible_streams_terminate_and_survive_a_capture_outage)"'
```

**GREEN.** Bounded backoff inside the existing `timeout_at(call.deadline)`.

**REFACTOR.** Delete `CaptureDrop::from_client` and the queue-error mapping.

### Scenario B3 — Gate refuses every public write to the calls table

**Behavior.** No principal, including one carrying the capture identity, can
write `vala.gateway.calls` through Gate. (REQ-179, INV-020, AC-043)

**RED.** Change `gate_reserves_the_gateway_call_table_to_the_capture_principal`
to expect refusal for the capture principal; it fails on the exemption:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::tests::gate_reserves_the_gateway_call_table_to_the_capture_principal)'
```

**GREEN.** Unconditional refusal; remove the capture confinement rule.

**REFACTOR.** Delete capture issuance, verification, role, and their tests.
Move `gateway_capture_decisions_publish_ahead_of_later_history` onto a
non-capture principal.

### Scenario B4 — Peer RPC capture in peer mode

**Behavior.** A gateway served by a pod without Scribe lands capture through
the peer RPC on a live Scribe. The RPC refuses non-capture tables, reserved
tenants, and callers without a peer certificate. (REQ-178, INV-020, AC-043)

**RED.** A new Rust gateway journey in `wyrd-testing --test gateway` running an
`oracle`-target gateway pod beside a `scribe`-target pod in peer mode over
RustFS, plus peer-handler negative tests. Both fail without the service.
Record their exact `mise exec -- cargo nextest run` commands once named.

**GREEN.** The peer service, its mount, and the writer's peer variant dialing
live Scribes.

**REFACTOR.** Share channel handling with the existing peer dialers where it
removes code.

### Acceptance criteria

- REQ-178 to REQ-180, INV-020, and AC-043 each have passing evidence in the
  table below.
- Existing gateway journeys, including the Python capture-evidence journeys,
  stay green.

### Verification

- The focused commands above, plus the new journey and peer-handler commands.
- `mise run test:gateway:journey`, `mise run test:bifrost`,
  `mise run test:principals:integration`, `mise run codegen:check`,
  `mise run check:client-tier`, `mise run docs:check`, `mise run fmt`,
  `mise run lints`.

### Stop conditions

- Any need to widen the peer RPC beyond the two capture tables.
- Scribe or Gate behavior that requires a token or audit row for capture.
- A gateway-serving pod with neither a local Scribe nor peer TLS configured.

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

Diagnosis, wyrd-client `blocking_mint_spends_one_transport_budget_then_retains`
(Amendment A):
- **Symptom:** the test hung until its timeout once retained ambiguous batches
  entered `Task::drain`.
- **Evidence:** `RecordQueue::start` restarted every retained entry whose
  backoff was due *now*; `drain` called it in a loop, so each retry that came
  due during the pass was restarted inside the same pass and the pass never
  ended.
- **Cause:** drain re-armed batches that became due after the pass began.
- **Fix site:** `RecordQueue::start(cutoff)` (`wyrd-queue/src/queue.rs`) starts
  only entries due at or before `cutoff`; `Task::drain` holds the pass-start
  instant, while `pump` and `send_outbox` pass `Instant::now()`. Those are the
  only callers. Pinned by `queue::tests::a_held_cutoff_starts_each_retained_batch_once`.
  An independent read-only diagnostician reached the same cause and fix site.
  The hung test itself exercised `ResolvedCredential::Renewable`, whose only
  production consumer was gateway capture; Amendment B removed that plumbing and
  the test with it.

### Amendment A evidence (revisions 53 and 55)

| Criterion | Implementation | Verification | Result |
|---|---|---|---|
| REQ-172, REQ-173, INV-019: one shared budget, no per-producer caps, no preallocation | `9e6464d2d`, `de3dfaca6`; `wyrd-queue` `ClientByteBudget`, `QueueConfig` | `mise exec -- cargo nextest run --locked -p wyrd-queue` (50/50), including `producer::tests::{a_thousand_tables_share_one_budget_and_idle_producers_hold_nothing, bounded_client_bytes_refuse_before_allocation}`; `wyrd-client` lib 219/219 including `bifrost::tests::a_thousand_tables_share_one_handle_budget` | PASS |
| REQ-174 (rev 55): byte or linger-with-free-slot sealing | `cac83f17d`; `Task::pump` and its linger arm in `wyrd-queue/src/producer.rs` | RED then GREEN: `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=producer::tests::stalled_sends_bound_concurrency_and_grow_the_next_batch)'` (RED: 1,000 rows behind two stalled sends never stayed staged; GREEN: they leave as one third batch); `producer::tests::byte_and_linger_triggers_seal_without_a_flush` | PASS |
| REQ-175: bounded concurrent sends, default is the smallest passing value | `cac83f17d`; `QueueConfig::default().max_in_flight = 1` | AC-041 benchmark below: 1, 2, 4 and 8 all pass; smallest passing is 1 | PASS |
| REQ-176: immediate all-or-none admission | `wyrd-queue` producer | `producer::tests::multi_row_admission_is_all_or_none`, `admitted_rows_cannot_take_the_sealing_headroom` | PASS |
| REQ-177: ambiguous sends retained under one identity | `9e6464d2d`; `RecordQueue::start(cutoff)` | `producer::tests::ambiguous_sends_retry_one_identity_until_acked`, `queue::tests::a_held_cutoff_starts_each_retained_batch_once`; `wyrd-client` `bifrost::grpc::tests::held_calls_exhaust_one_transport_budget_then_retain` | PASS |
| AC-042: SDK byte-budget override and 1,000×9 bursts | `de3dfaca6`; Rust/Python/TypeScript wrappers and stubs | `mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test observe_run -P journey --run-ignored=all -E 'test(=drift_burst_survives_a_byte_budget_override)'`; Py `tests/integration/state/test_observe_journey.py::test_drift_burst_survives_a_byte_budget_override`; TS `observe-run.test.ts` "lands every row of a 1,000 x 9 burst exactly once"; `mise run codegen:check` | PASS |
| AC-041: 50,000 rows/s sustained | `cac83f17d`; `wyrd-testing` bin `bifrost_ingest_capacity`, `mise run bench:bifrost:ingest-capacity` | Table below; report at `target/bifrost-ingest-capacity/report.{json,txt}` | PASS |
| Capacity seed resubmits on `QUEUE_FULL` and counts `SAMPLES × 9` | `de3dfaca6`; `verification_capacity/fixture.rs` | clippy clean; exercised by `mise run bench:verification:capacity` | PASS |

AC-041 run (release `wyrd-server --features cloud`, one process, Postgres,
RustFS; one `WyrdState` emitting Drift observations of 100 features; 60 s per
step):

| Step | obs/s | max_in_flight | Refused | Peak client bytes | Byte slope (B/s) | Drain (s) | Durable / expected rows | Uneven ids | Gate frames | Rows per frame | Gate write p50 / p99 (ms bucket) | Client / server µs per row |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| sustained | 500 | 4 | 0 | 5,592,976 | -34,191 | 0.047 | 3,000,000 / 3,000,000 | 0 | 6,069 | 494 | 25 / 100 | 4.5 / 4.5 |
| headroom | 1,000 | 4 | 0 | 6,845,464 | -16,420 | 0.053 | 6,000,000 / 6,000,000 | 0 | 4,656 | 1,289 | 25 / 50 | 3.8 / 2.5 |
| ack delay +50 ms | 500 | 4 | 0 | 7,030,936 | 78,892 | 0.117 | 3,000,000 / 3,000,000 | 0 | 1,835 | 1,635 | 25 / 50 | 4.1 / 2.5 |
| max_in_flight 1 | 500 | 1 | 0 | 5,231,104 | -47,591 | 0.045 | 3,000,000 / 3,000,000 | 0 | 1,387 | 2,163 | 25 / 25 | 3.7 / 2.2 |
| max_in_flight 2 | 500 | 2 | 0 | 5,335,048 | -20,887 | 0.045 | 3,000,000 / 3,000,000 | 0 | 3,619 | 829 | 25 / 50 | 4.2 / 3.6 |
| max_in_flight 4 | 500 | 4 | 0 | 5,462,872 | -92,523 | 0.048 | 3,000,000 / 3,000,000 | 0 | 6,316 | 475 | 25 / 50 | 4.4 / 4.5 |
| max_in_flight 8 | 500 | 8 | 0 | 4,536,856 | 4,118 | 0.043 | 3,000,000 / 3,000,000 | 0 | 8,676 | 346 | 25 / 50 | 4.6 / 5.5 |

The run used the then-default `max_in_flight = 4` for the sustained step; the
`max_in_flight 1` step is the same configuration as the new default and passes
every AC-041 check. Batch figures are the Gate's accepted-frame counter, so
they are means, not a distribution; latency is the Gate write histogram's
bucket bound.

Before the revision 55 fix, a 5 s smoke at `max_in_flight = 4` sealed ~350-row
frames at ~142/s against a sink capacity of ~95 frames/s, so client bytes grew
~3 MB/s and drain took 2.4 s (16 s with the 50 ms ack delay).

Diagnosis, AC-041 sustained step (Amendment A):
- **Symptom:** the sustained step failed "bytes grew" and "drain 2.4 s" with
  exact rows and no refusals.
- **Evidence:** `Task::pump` sealed whenever the linger had elapsed regardless
  of free slots; the linger `select!` arm ignored `can_send`; the outbox is a
  FIFO of sealed frames that never merge (`queue.rs`); `staging_full` was never
  reached because the linger emptied staging every ~7 ms; server cost is per
  frame (~42 ms ack, mostly WAL sync and the dedup fence), not per row.
- **Cause:** the linger, not load or sink capacity, set the frame rate, so
  capacity was fixed at `max_in_flight / ack latency` frames of ~350 rows.
- **Fix site:** `Task::pump` and its linger arm, the single producer owner
  reached by every SDK through `wyrd-client` `bifrost/handle.rs`; `Task::drain`,
  `RecordQueue::seal`, and the gRPC sink were checked and need no change. An
  independent read-only diagnostician reached the same cause and fix. The fix
  needed spec revision 55, which the user approved.
- A second defect in the benchmark itself counted rows by overlapping
  ±500 ms time windows, so each later step also counted its predecessor's
  tail; counts are now scoped by the step's `run_id`.

### Amendment B evidence (revision 54)

| Criterion | Implementation | Verification | Result |
|---|---|---|---|
| REQ-178, B1: one writer per process, in-process to a local Scribe | `de3dfaca6`; `components/gateway/capture.rs` (`GatewayCapture`, `CaptureRoute`) | `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^components::gateway::capture::tests::/)'` including `local_writer_submits_under_the_capture_principal`, `batches_carry_deterministic_ids_under_the_admitting_request`; `pg_invocation_tests::gateway_capture_follows_policy_and_never_affects_the_call` | PASS |
| REQ-180, B2: deadline-bounded delivery, counted drops | `capture.rs` `until_deadline`, `CaptureDrop` | `capture::tests::{saturation_retries_until_acknowledged_or_the_deadline, terminal_parked_and_absent_scribes_drop_without_retry, scribe_refusals_and_peer_codes_classify_alike}`; `pg_invocation_tests::gateway_capture_work_ends_at_the_call_deadline` (21/21 `pg_invocation_tests` with `scripts/postgres/with-test-postgres.sh`) | PASS |
| REQ-179, INV-020, B3: no token, permission, or audit; Gate refuses every public write to `vala.gateway.calls` | Gate, auth issue/verify, builtin roles, `wyrd-client` renewable credential removed | `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::tests::gate_reserves_the_gateway_call_table_to_the_capture_principal)'`; `audit_publication::service_decisions_publish_ahead_of_later_history` (`-p wyrd-testing --test server -P journey`) | PASS |
| REQ-178, B4: peer RPC capture from a pod without Scribe | `grpc/capture_peer.rs` (`ScribeCapturePeerGrpc`), mounted on the `wyrd-peer` mTLS listener; `WyrdTestCluster` gateway provider root | `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=peer::oracle_only_gateway_captures_through_the_peer_scribe)"'`; peer handler `grpc::capture_peer::tests::{requests_are_confined_to_tenant_capture_destinations, scribe_saturation_answers_resource_exhausted}`; misnamed-leaf refusal is covered for every peer service by `peer_network::listener` | PASS |
| Architecture docs | `de3dfaca6`; `wyrd-design.md`, `bifrost-design.md`, `wyrd-security-posture.md` | `mise run docs:check` | PASS |

The B4 journey runs the Oracle and Scribe pods over local shared storage, not
RustFS: the cluster harness has no RustFS option. Peer mode over RustFS is
exercised by the capacity benchmark topology.

### Direct execution documentation

`8ae5dfdd8` adds "Judging input directly" to
`docs/src/content/docs/how-to/evaluate/index.svx`: the HTTP route, request and
response, bounds, stable errors, `evals:run`, no idempotency or retries, the
three SDK calls, and the MCP tool. `mise run docs:check` passes. The MCP tool's
wire name is `verification.execute`, matching every other MCP tool's dotted
name; REQ-169's text spells it `verification_execute`.

### CLOSE-01 inherited obligation matrix

"gate" means the row's tests run inside `mise run gate` (traced through
`test:rust`, `test:bifrost:gate`, `py:test:integration`,
`ts:test:integration`, `codegen:check`, `docs:check` and the static checks).
Rust SDK journeys and the principals lane are outside gate and were run
separately; their results are recorded here.

| Inherited obligation | Strongest current proof | Lane | Result |
|---|---|---|---|
| PSI/SPC/Custom/assertion/judge → failed-verdict Operator, principal, SDK parity | Py `test_drift_journey.py::test_service_bindings_verify_drift_and_eval_through_an_http_operator`; TS `drift-verification.test.ts` "verifies one Service through Drift, Eval, and an Operator end to end"; RS `drift_verification.rs::service_verifies_drift_and_eval_through_the_sdk`; `pg_operator_delivery.rs::failed_verdict_fans_out_to_every_provider_independently` | gate; `mise run test:bifrost:journey:drift` (4/4) | GATE_PENDING |
| Locked Run identity, typed/mapping input, native projection, bounded IPC, custom-table reuse, shutdown | `wyrd-client` `observe::tests::{each_run_is_its_own_invocation, drift_projects_one_tall_row_per_feature, ambiguous_shutdown_retries_the_same_batch_on_the_same_state}`; Py/TS/RS observe journeys including the 1,000×9 bursts | gate; `mise run test:bifrost:journey:observe` (3/3) | GATE_PENDING |
| Python span enrichment, joins, nested scopes, asyncio isolation, private provider, OTEL fail-open | Py `test_observe_journey.py::test_scoped_run_emits_drift_eval_and_generic_rows`; `unit/state/test_observe_surface.py::{test_scope_survives_await_and_isolates_concurrent_tasks, test_nested_card_scopes_share_the_run_and_restore_the_outer_card, test_missing_opentelemetry_is_a_no_op, test_enrichment_failure_never_blocks_observations}` | gate | GATE_PENDING; the "no log/metric enrichment inferred" clause is a non-goal with no executable assertion |
| HTTP/MCP binding and direct Drift runs, requester, ownership, idempotency, authz, result queries, MCP catalog | `pg_verification_routes.rs::{manual_runs_enqueue_replay_and_read_back, manual_run_refusals_fail_before_enqueue, verification_state_is_tenant_isolated}`; `pg_verifier_runs.rs::manual_idempotency_keys_replay_conflict_and_scope_to_requester`; MCP `pg_tests::an_agent_runs_a_verifier_directly_and_reads_its_result`; Py/TS run journeys; RS `verification_run.rs::starts_a_keyed_manual_run_and_reads_its_status` | gate; `mise run test:cards:integration` | GATE_PENDING |
| Worker without local Scribe, SYSTEM table matrix, SYSTEM refusals, shared fanout | `verification_runtime.rs::{runner_without_local_scribe_publishes_through_the_ingest_endpoint, drift_runner_without_local_oracle_reads_through_a_peer, two_bindings_share_one_client_observation}`; `pg_grpc_ingest_smoke.rs::system_writer_matrix_spans_every_builtin_table`; `pg_verification_routes.rs::system_writer_token_is_refused_by_every_public_token_grant` | gate | GATE_PENDING |
| Daily partitions, Bloom/row-group pruning, Eval record-day lookup, midnight ACKs | `verification_runtime.rs::result_layout_partitions_blooms_and_prunes_by_result`; `eval_verification.rs::sealed_replay_on_a_later_day_activates_once`; `pg_verifier_runs.rs::observation_runs_are_unique_per_input_record` | gate | GATE_PENDING |
| Duplicates, sealed replay, expired leases, retries, fail-open Eval enqueue, partial ACKs, summary-only | `pg_verification_runtime.rs::{lost_result_ack_replays_the_identical_sealed_batch_and_scribe_deduplicates, unacknowledged_summary_retries_with_a_fresh_result, crashed_runner_restarts_and_reclaims_without_duplicates}`; `pg_verifier_runs.rs::concurrent_schedulers_create_one_run_per_occurrence`; `eval_verification.rs::integrated_enqueue_failure_preserves_ack` | gate | GATE_PENDING |
| PostgreSQL-owned times and restart fencing | `pg_verifier_runs.rs::{database_clock_owns_verifier_queue_deadlines, reclaimed_lease_fences_the_stale_token, dispatch_delivery_obeys_budget_deadline_and_fencing}`; `pg_verification_bindings.rs::database_clock_owns_machine_activity_and_schedule_arming` | gate | GATE_PENDING |
| Execution beyond former ceilings, Operator limits/timeout, shutdown/reclaim, health | `pg_verification_runtime.rs::{verifier_runs_execute_beyond_the_former_permit_ceilings, shutdown_stops_claims_drains_bounded_and_restart_recovers_identity}`; `pg_operator_delivery.rs::{verifiers_progress_while_operator_deliveries_are_capped, slow_endpoint_exhausts_the_budget_and_shutdown_releases}` | gate | GATE_PENDING |
| Allowed/denied authorization audit at public/Gate boundaries only | `pg_verification_runtime.rs::completed_run_publishes_details_then_summary_and_records_metrics`; `pg_verification_routes.rs::status_reads_audit_cards_read_decisions`; `pg_bifrost_e2e.rs::denied_describe_is_audited_before_admission` | gate | GATE_PENDING |
| Operator CRUD, rotation, redaction, protocol fixtures, SDK/CLI/MCP parity | `pg_operator_connection_routes.rs::admin_manages_redacted_encrypted_connections`; `pg_operator_delivery.rs::{next_attempt_on_another_replica_uses_the_rotated_credential, ambiguous_pagerduty_retry_reuses_the_dedup_key}`; RS/Py/TS/CLI/MCP connection journeys | gate | GATE_PENDING; live Slack/PagerDuty (`test:operators:smoke:live`) is credentialed release evidence, not run |
| Qualifying exchanges activate owners; others do not; suspension and snapshots | `wyrd-auth` `issuance::pg_tests::{qualifying_machine_grants_activate_the_bound_owner, non_qualifying_grants_never_touch_activity, a_suspended_principal_is_refused_by_every_grant}`; `pg_verification_bindings.rs::{excluded_principals_record_nothing, out_of_order_exchanges_never_move_activity_backward}` | gate; `mise run test:principals:integration` (15/15, 19/19) | GATE_PENDING; human refresh, SYSTEM and cached-bearer non-activation rest on the single `records_owner_activity` match without a dedicated test (open gap) |
| Verifier vocabulary, no retired surfaces, codegen, registration, docs, served OpenAPI | `pg_openapi_contract.rs::verification_contract_publishes_exactly_four_typed_operations`; `wyrd-loader` `parse_rejects_retired_drift_and_eval_card_kinds`; `codegen:check`, `docs:check` | gate; `mise run test:principals:integration` | GATE_PENDING; no gated scan names retired Verifier routes/tables; absence is proven by the four-operation OpenAPI and loader refusal |

Credentialed cloud/provider smokes and official-image qualification
(`test:server:startup`, `test:operators:smoke:live`, `test:gateway:smoke:live`,
`test:storage:*:cloud`) remain separately identified release evidence and were
not run.

### Capacity benchmark startup diagnosis

- **Symptom:** `mise run bench:verification:capacity` failed while
  provisioning. The client error alternated between
  `WYRD_CLIENT_503_TRANSPORT_DOWN` "error decoding response body for url
  (http://127.0.0.1:8080/auth/token)" and an empty
  `WYRD_SPEC_502_UPSTREAM_FAILURE`. The server log had no errors.
- **Evidence:** `wyrd-server/src/components/auth/routes.rs` puts a per-peer-IP
  `GovernorLayer` on the `/auth/*` routes: a burst of 20, then one request per
  100 ms. The benchmark provisioned 72 tenants (m0, m1 and 70 background
  tenants) from 127.0.0.1 back to back. Each tenant makes a token exchange and
  an `issue-key` call, then one more exchange per replica for each client.
  With `--background 0` (2 tenants), provisioning and the run completed.
- **Cause:** the benchmark exceeded the server's intentional auth rate limit.
  The governor's 429 response is plain text. `AuthClient::decode`
  (`wyrd-client/src/auth.rs`) parses every error body as JSON and turns this
  one into a transport error. `transport/http.rs` maps a non-JSON error body to
  an empty code, which drops the HTTP status.
- **Fix site:** the benchmark owner (`verification_capacity/main.rs`,
  `load.rs`). Tenant provisioning and client connects are now spaced by
  `AUTH_SPACING` (400 ms). Each administrator client exchanges its token at
  connect, so no load step opens with every tenant authenticating at once.
  The production limit is unchanged.
- **Open product defect (recorded, not fixed):** the auth governor's 429 is not
  problem+json, and the client loses the status for non-JSON error bodies.
  AGENTS §9 requires structured Wyrd errors. A fix needs a stable
  auth-rate-limit code; the catalog has none today. That is a contract decision
  outside this task.

### Capacity benchmark replica join diagnosis

- **Symptom:** the first revision 56 run failed after the one-replica
  sustained step. The joining replica 1 exited with status 70.
- **Evidence:** its kept log shows `Bifrost peer listener failed to bind
  127.0.0.1:50062: Address already in use`. `ip_local_port_range` is
  `32768 60999`. Replica 0's log shows it bound 50051 and 50052 at startup,
  before any load.
- **Cause:** `release_server.rs` gave joined replicas fixed gRPC and peer
  ports of 50061 and 50062, which lie inside the kernel's ephemeral port
  range. After 11 minutes of load from 72 tenants, an outbound connection
  already held local port 50062 when replica 1 tried to bind it. The previous
  run's logs were lost because `LocalServer`'s temporary directory was
  deleted on the error path.
- **Fix site:** `wyrd-testing/src/release_server.rs`, whose only
  `start_replica` caller is this benchmark.
  - Joined replicas now bind gRPC and peer ports below the ephemeral range,
    from bases 30051 and 30052. Replica 0 keeps the server defaults.
  - A `LocalServer` dropped without `stop` keeps its working directory and
    prints the path of its `server.log`.

### Capacity benchmark second-replica readiness diagnosis

- **Symptom:** after the port fix, replica 1 (`all`) never reported ready. A
  10-second smoke run (`-- --steps 50 --step-seconds 10 --sustained-seconds
  10 --background 0`) reproduced it in about 3 minutes.
- **Evidence:** `await_ready` now carries the last `/readyz` body. It showed
  `"forge_coordinator":{"reason":"forge_coordinator_unavailable"}`, with every
  other check ok. `forge/scheduler.rs` published coordinator readiness as
  `!outcome.standby && !outcome.incomplete`.
- **Cause:** Forge planning is a singleton held by the
  `vala.forge_scheduler_state` lease. A second replica finds the live peer's
  lease, skips planning (standby), and keeps running its Forge worker, Scribe,
  Oracle, and API. Readiness treated that healthy standby as unready, so
  `/readyz` failed and Kubernetes would drop every `all` replica after the
  first from its Service. `pg_router_smoke::coordinator_standby_pass_is_not_ready`
  pinned the defect, and both Kubernetes guides, `deploy/kubernetes/kind/wyrd.yaml`,
  and `peer_network/join.rs` documented it as the reason to scale with
  `oracle` pods instead of more `all` pods.
- **Fix site:** `forge/scheduler.rs` publishes
  `outcome.standby || !outcome.incomplete`, so only a lease holder whose pass
  left demand unplanned is unready. The only consumers are `/readyz`
  (`components/health/mod.rs::probe_forge_coordinator`) and the
  `bifrost_role_ready{role="forge_coordinator"}` gauge. The test is now
  `coordinator_standby_pass_is_ready`. The guides recommend scaling with `all`
  replicas first and role-specific pods second. The benchmark's second replica
  is `all`, every replica shares one `WYRD_SIGNING_KEY_FILE`, and observations
  rotate across both replicas.
- **Verification:** `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_router_smoke -E "test(=coordinator_standby_pass_is_ready) | test(/coordinator/)"'`
  passed 6/6 (standby, partial pass, completed pass, insert failure, object
  store failure, roster discovery). `mise run docs:check` passed.
- **Finding, not fixed:** neither `deploy/kubernetes/kind/wyrd.yaml` nor the
  production guide sets `WYRD_VERIFICATION_INGEST_ENDPOINT` on `wyrd-oracle`.
  Per `configuration.svx`, those Oracle pods therefore run no Verifier runner.

### Capacity benchmark run 1 on two `all` replicas

`mise run bench:verification:capacity`: exit 1, wall 1351 s, setup 43 s. That
exceeds REQ-171's 30-minute budget only because the two-replica drain ran its
full 300 s. Another session's clippy build shared the host during the run.

| Step | Replicas | Offered/s | Achieved/s | Backlog at deadline/final | Drain s | Sustainable |
|---|---|---|---|---|---|---|
| warmup-50 | 1 | 87.0 | 86.8 | 0/0 | 0.3 | yes |
| ramp-50 | 1 | 87.0 | 86.9 | 0/0 | 0.5 | yes |
| ramp-100 | 1 | 137.0 | 136.8 | 0/0 | 0.3 | yes |
| ramp-200 | 1 | 237.0 | 231.9 | 0/0 | 1.8 | yes |
| ramp-400 | 1 | 437.0 | 292.8 | 4768/0 | 100.0 | no |
| sustained-200 | 1 | 237.0 | 232.6 | 0/0 | 6.0 | yes |
| sustained-200 | 2 | 237.0 | 200.6 | 6166/6166 | 300.0 | no |

AC-040 passed in the one-replica sustained step: 3,600 samples per objective
kind, 100% at or below 9 ms. The only failing check was the two-replica
overlap: the noisy tenant's queued `eval_assertion` and `eval_llm_judge` runs
stopped completing, with 517 of 3,600 made. Its Drift runs, the quiet tenant,
and all 70 background tenants completed everything.

- **Symptom:** in the two-replica step, replica 0 logged 908 occurrences of
  `acknowledged Eval observation did not enqueue verification runs ...
  error="error returned from database: deadlock detected at line 1130"`, all
  for the noisy tenant. The first came at 03:27:50Z, 42 s after replica 1
  started; none came in the one-replica steps. Each was preceded by a 1 s wait
  on `SELECT 1 FROM wyrd.verification_bindings WHERE binding_id = $1 FOR NO KEY
  UPDATE`.
- **Evidence:** both sides of each cycle are in replica 0's log. The victim
  waited 1.0 s (`deadlock_timeout`) on the binding of subject `…68f4…`; the
  survivor, on `…68f1…`, got its lock the moment the victim aborted. "Line
  1130" is the Postgres C source line that sqlx 0.9 appends, not a SQL line.
  Replica 1 logged no binding-lock waits.
- **Cause (independent diagnostician, confirmed in source):**
  `ObservationEnqueue::enqueue` (`verification/observations.rs`) enqueues one
  acknowledged frame in one tenant transaction, row by row in emission order.
  `VerifierRunQueue::insert` (`wyrd-sql/src/queries/verifier_runs.rs`) row-locks
  each row's binding with `FOR NO KEY UPDATE` until commit. The noisy tenant's
  frames mix rows for two subjects (assertion and judge, one binding each) in
  arbitrary order, so two concurrent frames locked the two bindings in opposite
  orders. Eval enqueue is fail-open with no retry, and Gate calls the hook only
  on a batch's first commit, so every row of the aborted frame permanently lost
  its runs. Background tenants have one binding and cannot form a cycle. The
  second replica only raised concurrency; it is not required for the cycle.
  None of the uncommitted closeout diff touches this path.
- **Fix site:** `VerifierRunQueue::enqueue_observations`, the lock owner. It
  locks every `observations_ready` binding of the frame's distinct subjects in
  one statement ordered by `binding_id`, then enqueues rows in frame order
  through `enqueue_observation`, whose per-binding lock is then already held.
  `ObservationEnqueue::enqueue` is the only production caller and now calls it.
  Manual and scheduled enqueue never take the binding lock and are unchanged.
- **Verification:** RED, then GREEN:
  `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_verifier_runs -E "test(=frames_naming_subjects_in_opposite_orders_serialize)"'`.
  The whole `pg_verifier_runs` target passed 23/23. `-p wyrd-testing --test
  server -P journey --run-ignored=all -E "test(/eval_verification::/)"` passed
  7/7. Clippy for `wyrd-sql`, `wyrd-server` and `vala-bifrost-redux` with
  `--all-targets --all-features` is clean.
- **Separate finding, undiagnosed:** replica 0, the planning-lease holder,
  logged `gRPC health marking NotServing because cached readiness snapshot
  failed` every 2 minutes in both one- and two-replica steps. Replica 1 never
  did. No Postgres or storage probe warning accompanied it, so a role readiness
  bit flipped. The info-level logs do not name which one.

### Capacity benchmark run 3: two tenants (draft revision 57)

Run: `WYRD_LOG=info,vala_bifrost_redux::scribe=debug,vala_bifrost_redux::gate=debug,wyrd_server::verification=debug,sqlx=warn mise run bench:verification:capacity`. The run exited 0 in 777 s, with 10 s of setup.

| step | replicas | offered/s | achieved/s | client p95 ms | drain s | sustainable |
|---|---|---|---|---|---|---|
| ramp-200 | 1 | 202.0 | 201.3 | 207.3 | 0.3 | yes |
| ramp-400 | 1 | 402.0 | 266.1 | 233.6 | 77.0 | no |
| sustained-200 | 1 | 202.0 | 201.7 | 207.5 | 0.3 | yes |
| sustained-200 | 2 | 202.0 | 179.7 | 530.6 | 42.4 | no |

What passed:

- Every step reconciled, with no wrong verdicts. Every kind overlapped in every 5 s slice.
- On two replicas, queued work was claimed across replicas exactly once.
- The quiet tenant progressed in both sustained steps.
- AC-040 passed for every non-judge kind: 3,600 samples each, 100% at or below 9 ms.
- Trace scrubbing passed, and both replicas shut down cleanly.
- There were zero deadlocks and zero NotServing transitions. The 15 s Scribe acknowledgement stall did not recur.

Open finding: two replicas sustain less throughput than one.

#### Diagnosis: two-replica throughput loss

A fresh read-only diagnostician worked from the run 3 logs and source.

- **Symptom:** At 202/s, two replicas achieved 179.7/s. The noisy tenant's queued kinds completed 15.6/s against 20/s offered. Client p95 rose from 207 ms to 530 ms on both the queued and the direct paths. Total replica CPU was about 5.0 cores on one replica and about 5.5 cores on two.
- **Evidence:**
  - The only statements slower than 1 s in the run were `SELECT last_seq, head_hash FROM vala.audit_chain_head ... FOR UPDATE`.
    - There were zero in the one-replica window.
    - In the two-replica window there were about 200 per minute per replica, with a mean of 1.3 s and a maximum of 3.92 s, nearly all for tenant m0.
  - The callers were Gate `dispatch_native_frame` writes to `vala.verification.results`, `vala.drift.result_features` and `vala.eval.result_items`, plus `start_run`.
  - Publication phase p95 rose from 0.5 s to 30 s.
  - The quiet tenant m1 has its own chain-head row. On two replicas its terminal p95 was 486 ms, against about 52 s for m0.
- **Cause:** Gate write audit (`wyrd-server/src/bifrost/gate_audit.rs::PostgresGateAudit::append_write_decision`) and `start_run` audit (`components/verification/service.rs`) append synchronously, one transaction per decision. Each append locks the tenant's single `audit_chain_head` row (`vala-sql/src/queries/audit_staging.rs::append_audit_batch`). A second replica doubles the number of concurrent appenders queued on that row. Waiting connections also take pool slots away from the direct path. This contradicts AGENTS.md: permissions block, audits do not.
- **Secondary cause:** `freeze_publication_range` locks the same row `FOR UPDATE NOWAIT` and failed nearly every sweep, even on one replica. As a result, staged audit was not published during load.
- **Ruled out:** binding enqueue locks, run-claim SQL, peer tail RPC, and pool exhaustion as a root cause. The 48 HTTP 503s were `/readyz` polls during replica 1 startup.
- **Fix site:** one shared batched audit outbox for every surface, and publication state separated from the chain-head lock. This is drafted as `changes/active/audit-outbox/spec.md` revision 1, which awaits approval.

#### Forge planning review

An independent reviewer compared Forge planning with the local RisingWave copy and found:

- Planning is a fleet singleton behind a 15-minute lease that is never released.
- Planning is paced by a timer at 256 demands per 60 s tick.
- Every cycle re-seeds the full roster.
- Hot tables starve.
- Worker claims serialize on one cursor row.

The target design, concurrent per-table demand claims, is drafted as `changes/active/forge-concurrent-planning/spec.md` revision 1, which awaits approval.

### bench:capacity implementation (revision 57)

Commits `dec4213fa` (benchmark), `4c99f8448` (tests moved out of the
benchmark), `4bb5d3aa2` (report readability). Owner:
`crates/wyrd/wyrd-testing/src/bin/capacity/` (`main.rs`, `load.rs`,
`step.rs`, `evidence.rs`, `report.rs`, `fixture.rs`, `judge.rs`,
`profile.rs`); task `bench:capacity` in `mise.toml`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-171: one benchmark `bench:capacity`, the two old tasks and their binaries deleted | `mise.toml` `[tasks."bench:capacity"]`; one `[[bin]] capacity` in `wyrd-testing/Cargo.toml`; `bin/verification_capacity/` renamed, `bin/bifrost_ingest_capacity/` and the OTLP collector deleted | `git grep` finds the old names only in historical task/spec text | PASS |
| REQ-171 workload: four identical tenants; direct and queued at L/2 over five kinds; ingest 2.5·L Drift observations × 100 features via `WyrdState` default queue; Oracle L/2 split lookup/aggregate over the last 5 min | `load.rs::{mix, Lane::drive, Request, query}`, `main.rs::TENANTS` | `cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=load::tests::mix_offers_the_required_rates)'`, `-E 'test(=load::tests::queries_read_the_last_five_minutes)'` | PASS |
| REQ-171 steps: warmup, ramp to first SLO miss (knee K), sustained K on 1 and 2 replicas, scale-out 2K on 2 | `main.rs` flow, `step.rs::Deployment::run` | smoke run below executed every step | PASS |
| REQ-171 SLOs per cell: traffic ≥ 95%, zero errors, overhead p95 < 10 ms with ≥ 1,000 samples in 1-replica sustained, ingest drain ≤ 1 s and no QUEUE_FULL, every backlog drained in 60 s; latency/CPU/memory reported | `report.rs::{step_row, op_row, overhead}`, `step.rs::{ops, drain}`, `evidence.rs::{Scrapes::overhead, scribe_backlog, Queue::backlog}` (one SQL probe over `wyrd.verifier_runs`, `vala.audit_staging`/`vala.audit_chain_head`, `vala.forge_planning_demands`) | `-E 'test(=report::tests::every_slo_failure_fails_the_step)'`, `-E 'test(=evidence::tests::quantile_reads_bucket_deltas)'`, `-E 'test(=evidence::tests::raw_percentiles_use_nearest_rank)'` | PASS |
| REQ-171 verdict and report: passes when 1-replica sustained and both 2-replica steps pass; one row per step then per operation; `--profile` kept | `report.rs::{Report::passed, verdict_steps, render}`; `mise.toml` profiling branch | `-E 'test(=report::tests::verdict_needs_every_verdict_step)'` | PASS |
| AC-040 overhead SLO from the sustained 1-replica step | `report.rs::overhead` marks a short kind `< 1,000` | smoke report | PASS |
| AC-041 ingest SLOs at or below the knee | `load.rs` ingest lane times `flush()`; QUEUE_FULL counted as an error | smoke report | PASS |
| (a) exactly-once queued claims across two replicas | `crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs::two_replicas_claim_each_queued_run_exactly_once` | `WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E 'test(=two_replicas_claim_each_queued_run_exactly_once)'` | PASS |
| (b) tenant fairness for queued runs | `pg_verification_runtime.rs::a_flooding_tenant_does_not_delay_another_tenants_run` | same command with `-E 'test(=a_flooding_tenant_does_not_delay_another_tenants_run)'` | PASS |
| (c) failed LLM-judge verdict on the Rust direct path | `sdks/wyrd-sdk-rust/tests/drift_verification.rs` `REJECTED_ANSWER` judged row in `DirectJourney::assert_judgments` | `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test drift_verification -P journey --run-ignored=all -E 'test(=direct_execution_judges_supplied_input_through_the_sdk)'` | PASS |
| (c) SPC passed and failed on the Rust direct path | existing rows in `DirectJourney::assert_judgments` | same command | PASS |
| (c) queued Custom failed verdict | existing `assert_direct_scores` Custom `failed` in `drift_methods_fit_score_persist_and_dispatch` | same wrapper, `-E 'test(=drift_methods_fit_score_persist_and_dispatch)'` | PASS |
| (d) AC-041: 100 features per `record_id`, exactly once | `sdks/wyrd-sdk-rust/tests/observe_run.rs::sustained_hundred_feature_drift_lands_exactly_once_with_flat_client_bytes` (1,500 observations, 1,500 groups of 100) | `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test observe_run -P journey --run-ignored=all -E 'test(=sustained_hundred_feature_drift_lands_exactly_once_with_flat_client_bytes)'` | PASS |
| (e) AC-041: flat client-owned bytes under sustained emission | same test: late-third max ≤ early-third max + one `max_message_bytes`, 0 after flush | same command | PASS |

Why (a) and (b) are integration tests, not process journeys: a release
replica cannot script or hold its engine, so a journey cannot observe a
second execution of one run or the order of claims. Both tests compose the
`VerificationRuntime` every replica runs on the shared Postgres queue. (a)
holds both runtimes until all 100 runs are claimed, requires both to claim,
and checks `attempts == 1` and exactly 100 durable summaries. (b) reads claim
order from PostgreSQL lease deadlines: the quiet tenant's run, enqueued after
a 60-run flood, is claimed behind at most one flood run.

Diagnosis while writing (b):

- **Symptom:** with a 200-run flood, `wait_run_in` panicked with
  `PoolTimedOut` after release.
- **Evidence:** Gate writes failed with `audit outbox unavailable: ... pool
  timed out`, and publications waited more than 4 s for a connection.
- **Cause:** releasing 201 held executions at once makes 201 publications
  compete for the test server's Postgres pool. The ordering assertion had
  already passed.
- **Fix site:** the test's flood size. 60 runs still separate round-robin
  claiming (≤ 1 ahead) from FIFO (60 ahead).

Also fixed: `observe_run.rs` existing burst-test casts failed
`-D warnings` clippy, so the constants became `u32` with `From`
conversions.

Other commands:

- `mise run fmt`: clean.
- `mise exec -- cargo clippy --locked -p wyrd-testing --all-targets --all-features -- -D warnings`: clean.
- `mise exec -- cargo clippy --locked -p wyrd-sdk-rust --all-targets --all-features -- -D warnings`: clean.
- `mise run lints`: clean.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`: 6 passed.
- `git diff --check`: clean.

Smoke run, reduced durations, not the full benchmark:

`WYRD_LOG=info mise run bench:capacity -- --levels 20 --warmup-seconds 5 --ramp-seconds 10 --sustained-seconds 15`

Every step ran, the report rendered, and both replicas shut down cleanly.

- Zero errors in every step.
- Every backlog drained in ≤ 4.9 s.
- Ingest drain ≤ 0.07 s.
- Overhead ≤ 0.5 ms.
- Exit 1, as expected at these durations. The 1-replica sustained step fails the sample floor (n 32 < 1,000), and short windows bias queued traffic low (91–93%). That second point is explained below.

Material risks:

1. **Queued traffic counts runs settled inside the arrival window.** Claim
   polling (1 s) shifts roughly 1 s of completions past the window. That is
   about 9% of a 15 s smoke window but under 1% of a 180 s sustained step.
2. **If the knee is 50, the 1-replica sustained step fails AC-040 by
   construction.** It would yield 900 direct samples per kind
   (1.25/s × 4 tenants × 180 s), below 1,000.

Non-goals stayed excluded:

- No production code changed.
- No benchmark asserts fairness, claims, or judgment correctness.
- `bench:bifrost:query-capacity` was not touched.

IMPLEMENTED

## Specification Revision 51

Approved and folded into the spec under "Direct execution, telemetry, and
verification capacity (revision 51)", with revision 52 making its audit
non-blocking. The spec is the authority for the direct-execution contract,
bounds, stable errors, CLOSE-05 reference workloads, and capacity defaults.
