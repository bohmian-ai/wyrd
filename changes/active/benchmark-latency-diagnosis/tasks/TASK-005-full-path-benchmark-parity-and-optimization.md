---
id: TASK-005
kind: implementation
status: proposed
spec: SPEC-benchmark-latency-diagnosis
spec_revision: 5
requirements: [REQ-003, REQ-005, REQ-006, REQ-007, REQ-011, REQ-017, REQ-018, REQ-019, REQ-020, REQ-021, REQ-022, INV-001, INV-002, INV-003, INV-004, AC-013, AC-014, AC-015, AC-016, AC-017, AC-018]
depends_on: [TASK-004]
---

# Measure and optimize the complete Gateway and Verification path

## Outcome and Value

Give client and server the same benchmark collection and profiling, account for
the complete controlled path, and implement the supported small latency/throughput
improvements within this task. A server residual that excludes the provider HTTP
exchange is not a measurement of all Wyrd work. SDK preparation, local transport,
fake-provider work and response decoding must be visible too.

Today only the server has recorder snapshots and heap hooks; existing CPU profiles
omit the driver/SDK/local provider. Verification's current benchmark uses assertions
and never calls a model. After this task, a canonical report distinguishes both
processes and both Verification paths, retained profiles identify their code and
wait intervals, and implemented improvements have comparable before/after evidence.

Instrumentation, a ranked report, or proposed future optimizations alone do not
complete TASK-005. The two source-proven reductions below are required. Additional
supported small reductions from the measured candidates must be implemented here;
a rejected candidate needs explicit source/profile counterevidence rather than an
unexplained deferral. No promise of 100 us or submillisecond latency is made.

## What Changes and What Must Stay True

### One recorder and explicit activation in each process

Reuse `wyrd-benchmark`, not another recorder, exporter or timing registry. Benchmark
startup selects its backend before creating client metric handles. Use the existing
`WYRD_METRICS_BENCHMARK_SNAPSHOT=1` semantics; `capacity l1/l2` explicitly starts
benchmark mode in driver and child server. Reporting/help commands do not start it.
Install the benchmark tracing registry even if normal logs are disabled, since
case/phase labels live on spans. A client constructor/clone never installs a global
recorder or subscriber; conflicting backend installation is an explicit error.

Ordinary SDK calls perform only the disabled check: no benchmark timers, allocations,
spans, profile hooks or retained observations. Add the plain narrow `wyrd-benchmark`
dependency to `wyrd-client`; no new metrics Cargo feature or direct `metrics`
dependency is required. Shared SDK instrumentation reaches its Rust core, without
new language-specific public APIs or ordinary-mode backend changes.

### Bounded observations and population contract

| Observation | Meaning and population |
|---|---|
| Client invocation | One terminal observation for each timed Gateway invocation or SDK Verify flow, including preparation through full reception/validation or error/cancellation. Keep their nested HTTP totals distinct. |
| Client logical HTTP call | One terminal observation across retries and credential refresh. Parts accumulate actual reached work across attempts; retry backoff remains explicit elapsed work, not an invented component. |
| HTTP attempt | One observation per actual send attempt. Reconcile 401/refreshed replay and failures separately from the one logical call. |
| SDK/client components | Preparation, auth, serialization, send-to-headers, body receipt and decode; SDK invocation preparation is separately identified. A phase that was never reached is inapplicable, not a measured zero. |
| Fixture upstream | Request parse and answer preparation under a separate fixture boundary. Keep it separate from SDK work even though both run in the driver process. |
| Server controlled path | Existing total/residual and phases plus actual dispatch wait, answer-send-to-handler-resume, provider send-to-headers/body completion and Verification hosted setup. Provider send-to-headers includes local pool/connect/runtime/provider work; do not call it remote compute. |
| Background work | Capture projection, reservation pressure, outbox submission and drain remain separately identified. They can compete with response work without being added to its latency. |

For streaming, retain distinct headers/first-event and full-end boundaries. Body
receipt/decode spans consumption, not merely returning a stream object; dropping
the stream records cancellation of its terminal cohort. Keep the existing first-
event/end meanings and validate chunks before completion. Include this bounded
extra head boundary in the exact census rather than double-counting completed calls.

Use the same HDR precision, grouped snapshot consistency and invalid-evidence rules
as the server. Logical roots/attempts retain closed terminal outcomes `success`,
`error`, `cancelled`; an expected failed Verifier verdict is a successfully completed
call, with its business outcome retained separately. Cancellation cannot disappear.

Keep root request/attempt distributions in existing bounded timing series. Detailed
components use the same recorder's existing standard histogram section, as server
phase histograms do, with closed boundary/case/phase labels and reached-call counts.
Do not multiply every detailed phase by every terminal outcome. Record the logical
root and its completed phase observations coherently at the existing owner; this
is one collection path, not duplicate observations in two registries.

There are at most four root families: invocation, logical HTTP, HTTP attempt and
fixture upstream. Gateway's four case labels and four window labels give at most
192 labelled root/outcome keys, at most twelve extra streamed-head keys, plus
bounded unlabelled HTTP/setup keys. c1/c64 reuse
case/phase labels. Bifrost SDK calls do not acquire the Gateway/Verify invocation
or fixture roots; the existing all-engine census must include their actual reach
as well. Extend the exact outcome-aware lifetime proof for both process roles and
all component metric keys, including setup, failures and cancellation. Preserve
the existing 256 timing/1024 standard-metric caps; do not blindly widen them or
omit required failure observations. An uncovered/dropped series invalidates evidence.

Propagate existing closed benchmark classification into local-provider calls only
in benchmark mode, so late calls retain their originating cohort rather than the
stub's current global window. Classification conveys no authority. Take separate
opening/drained snapshots for driver and server and reconcile each producer's own
logical/attempt counts. Never difference snapshots or absolute clocks across
processes. Existing `queue_delay` starts before admission; add actual dispatch wait
under a distinct name instead of silently redefining it.

### Symmetric profiling and historical compatibility

The fixed command contract is:

| Selection | Driver and server behavior |
|---|---|
| No `--profile` | Benchmark metrics enabled; CPU and heap profilers disabled. |
| `--profile cpu` | CPU sampling for both process roles; heap observers disabled. |
| `--profile heap` | Heap/allocation sampling for both roles; no CPU sampler starts. |
| `--profile` or `--profile both` | Both collectors for both roles; diagnostic results remain separate from unprofiled qualification. |

Preserve the existing bare option while adding these optional values. Forward them
through aggregate commands, declare mode/roles in evidence, and keep profiled runs
out of historical calibration. Do not add the driver to `Workload::processes()`:
that list defines server resource accounting. Derive profiling targets separately.

Move the existing server heap sampler to `wyrd-benchmark` behind the earned,
default-off `heap` feature, reusing its unwinding, 512 KiB sampling, allocation/live
semantics and pprof encoding. Optional dependencies are the already-used
`backtrace`, `prost` and `flate2`. Ordinary client dependencies do not enable it;
server's existing `bench` feature and benchmark tooling do. Do not move this into
`wyrd-bench`, which dependency policy forbids in production cones. This one feature
keeps unwinding/pprof out of ordinary SDKs; no other feature/framework is selected.

Allocator hooks remain binary-owned. Preserve server MiMalloc and worker memory
charge/refund/null behavior; preserve the driver's current allocator rather than
switch it for comparability. Reuse the existing hook safety contract and sampler,
not another allocator implementation or broad lint suppression. Driver captures
call the shared sampler locally; no new driver HTTP listener is needed.

CPU captures must include both processes, named Wyrd frames and matching retained
executable hashes/build IDs. Retain case-specific files: start/finish capture per
step at the common experiment owner, not one whole-run profile mislabelled as one
case. Mark setup/warmup/measurement/drain scope and clock-alignment uncertainty;
do not invent exact per-repetition attribution without an anchor. Retain thread
context-switch information where supported; it does not identify a Tokio task's
await while another task runs on its worker. New request diagnostics close the
specific waits instead of treating CPU percentages as request latency.

Reuse existing schema-3 evidence/readings and `Record::render`. Add explicit
producer/process roles with historical server defaults; old records remain readable
and their missing client evidence is unavailable, not zero or retrospectively
client-profiled. Persist required client/server summaries before raw artifacts
disappear. Keep both raw snapshot streams and role-qualified profile artifacts.

### Verification coverage and optimization scope

Preserve assertion steps `direct-verify-c1`/`direct-verify-c64`, their alternating
expected outcomes and judgments. Add diagnostic-only `direct-verify-model-c1` and
`direct-verify-model-c64`, selected explicitly through `--steps`; do not silently
change the default assertion experiment or apply its latency limits to model calls.
Use the existing Service/Model/Verifier graph, Card-bound account and WyrdState SDK
flow. One Agent check calls a local Gateway deployment with one bounded iteration,
returns a deterministic valid cited Verdict and fixed usage, and retains evidence.
One upstream attempt per successful model verification must reconcile with results.
No external OpenAI call, credential, privileged bypass or new driver is required.
Full-path coverage means every component of these declared cases, not an assertion
that every provider/auth/TLS variant was exercised. Record the local fixture's
transport and unexercised branches explicitly; do not extrapolate its timings to
remote OpenAI or treat an unexercised branch as zero cost.

Implement the supported changes below after the measurement-complete milestone:

| Candidate | Required disposition and protected behavior |
|---|---|
| Canonical borrowed strings | Required: compare eligible borrowed/unescaped text without reserializing it solely for canonical comparison. Preserve serializer fallback, Unicode/control/escape handling, duplicate/depth/numeric checks, compact token bounds and final model-rewrite bytes. |
| Vertex access-token wrapper | Required: avoid constructing a network client that the supplied-token source does not use. Preserve sensitive bearer handling and every network-backed OAuth source's refresh, deadline/error and caching behavior. No new credential cache. |
| Tracing bookkeeping | Use CPU-only named stacks to remove supported redundant span/filter/registry/conversion work. Preserve span IDs, parent/links, events/errors, attributes, tenant catalog/export/drain and loss accounting. Inactivity/thread/location tracking is already disabled; suppressing required tenant tracing is prohibited. |
| Verification hosted setup | Profile construction outside Engine. Avoid supported unused Agent/tool/Gateway/trace setup at existing owners without changing eager validation/refusal, evidence admission/pinning, caller attribution, route overrides, deadlines, executor clock, or Agent/Code/extraction/drift behavior. No cross-caller/tenant cache. |
| Capture draining | Use CPU/phase/allocation evidence for small redundant concat/encoding/digest work at existing owners. Preserve byte charges, row/order/schema, stable retry frames and release/acknowledgement boundaries. No buffering/concurrency increase or retry/capacity redesign. |

For each profile-informed candidate, document the named cost, reachable path,
smallest safe reduction and focused proof. Implement supported reductions here.
If the work is required representation/validation or no small safe reduction exists,
show the concrete source/profile counterevidence; do not invent a speedup, delete
required work or defer a supported fix because more general research is possible.
No new auth cache, permission bypass, scheduler, public product workflow or durable
contract is selected. Preserve all existing spec invariants.

## Approach

1. Retain current code/binaries and reference readings; inventory old-profile limits.
2. Add guarded client metrics, startup registry and process/cohort evidence using
   the existing owner; close the exact bounded-series proof and canonical reports.
3. Add server wait/setup observations, shared heap hooks and symmetric profile modes.
4. Add local model-backed Verify diagnostics and complete real SDK/server regressions.
5. Qualify observation cost, then collect CPU-only and separate heap evidence for
   the measurement-complete milestone across both processes and Verification paths.
6. Implement the two source-proven reductions and the supported small profile-ranked
   reductions at existing owners with focused TDD/regression evidence.
7. Repeat matched qualification/profiles and publish canonical per-repetition
   comparisons, ranked findings, implemented changes and evidence-based rejections.

## Ordered Implementation Scenarios

### Scenario 1 — Explicit startup enables one recorder in each process

**Behavior.** Benchmark startup installs one backend and a real classification
registry before clients, even with no log environment. Ordinary client creation
and calls install neither and accumulate nothing. Conflicting backend selection
fails explicitly; repeated client construction does not repeat global installation.

**RED.** Add the [planned startup/disabled tests](#startup-and-recorder-proof).
Current driver startup fails labelled collection without logs and ordinary SDK
instrumentation is absent. Use process-isolated setup for global state, not shared
test-order assumptions or runtime environment mutation.

**GREEN.** Reuse the existing activation/recorder at the process owner and guard
before clocks, spans or allocations. Keep ordinary production monitoring unchanged.

**REFACTOR.** Remove duplicate startup plumbing; no hidden constructor subscriber,
second backend, exporter or new client configuration knob.

### Scenario 2 — Client, attempt and fixture populations reconcile

**Behavior.** One logical terminal observation survives success, error, cancellation
and credential replay, while attempts and reached phases keep their own counts.
SDK, direct caller and stub remain distinguishable; late calls keep original
classification. Driver/server snapshots cannot be confused or cross-differenced.

**RED.** Add the [planned cohort/population tests](#client-population-proof) for
success, failed auth/send/decode, 401 replay, cancellation and late drain. Extend
the all-engine lifetime census and assert no dropped/rejected observations. Current
records lack driver evidence and fail the process/population assertions.

**GREEN.** Instrument shared SDK paths, raw Gateway caller and stub with the fixed
root/component contract; persist role-qualified readings and raw snapshots using
the current experiment owners. Rerun scenario 1 and auth/replay regressions.

**REFACTOR.** Share existing grouping/interval machinery and retained handles.
Preserve independent producer progress; no new process-wide hot lock or registry.

### Scenario 3 — Controlled server waits are explicit

**Behavior.** Provider headers/body, actual dispatch, answer resumption and hosted
Verification setup have measured boundaries without changing total/residual/queue
meaning. Retries/overlapping exchanges retain proper unions; handoff diagnostics
never change cancellation, answer, settlement or capture ownership.

**RED.** Add the [planned exchange/handoff tests](#server-boundary-proof) with
controlled local delays and overlapping/retried failures. Keep total/residual
conservation and prove dispatch excludes admission. Existing observations fail to
distinguish these intervals. Do not assert wall-clock microsecond thresholds.

**GREEN.** Extend existing exchange, Answer and Verification telemetry owners and
their consumers. Rerun earlier scenarios and real boundary/ownership regressions.

**REFACTOR.** Reuse local instants and existing timing groups; no new scheduler or
cross-process timestamp subtraction. Label overlaps and unassigned gaps honestly.

### Scenario 4 — CPU and heap modes cover both processes symmetrically

**Behavior.** All four mode selections follow the table above. Separate profile
target roles keep driver out of server accounting. Shared sampler hooks preserve
both allocators; off mode has no sampling, CPU-only no heap observer and heap-only
no CPU subprocess. Artifacts retain case, role, unit and executable identity.

**RED.** Extend [profile-mode and sampler tests](#symmetric-profile-proof), including
startup failure/partial artifact notes, combined/bare parsing, forwarding and
readback. Current server-only captures and missing driver hooks fail parity.

**GREEN.** Extract the existing sampler under the narrow heap feature, wire binary
hooks and central collectors, and retain per-step CPU and per-role heap files.
Rerun startup, dependency, allocator-accounting and collector lifecycle regressions.

**REFACTOR.** Remove the old server-only sampler copy, preserving its existing safety
and stack-trimming tests at the new owner. Do not switch allocators or duplicate samplers.

### Scenario 5 — Verification profiles include a real local model call

**Behavior.** The new explicit model steps execute through real WyrdState/SDK,
accepted runtime authority, Verify, Gateway and local provider; results carry the
expected valid citation/usage and one reconciled upstream attempt. Assertion defaults
and expected negative verdicts remain unchanged; all evidence drains correctly.

**RED.** Add the [planned model declaration/result and journey tests](#model-verification-proof).
Current assertions-only fixtures cannot produce the required upstream/citation
evidence and unknown new steps must not accidentally select an empty run.

**GREEN.** Extend existing provision/declare/call owners with the diagnostic Agent
check and local response fixture. Preserve grants, tenant/caller identity and deadlines.
Rerun existing assertion and real SDK authority/negative journeys.

**REFACTOR.** Reuse the current graph pull/SDK/local Gateway fixtures; no bespoke
model driver, external credentials or new default benchmark judgment.

### Scenario 6 — Canonical strings avoid redundant formatting

**Behavior.** Borrowed unescaped strings retain identical canonical eligibility,
compact bounds and forwarded bytes without a JSON-formatting pass solely for
comparison. Escaped/transient strings and other scalar types keep compatible fallback.

**RED.** Add the [planned canonical equivalence proof](#supported-optimization-proof)
over escapes, Unicode, controls, numbers, duplicates, depth and fallback. Preserve
existing behavior first and demonstrate the targeted redundant formatting through
actual owner-path/ownership proof, not an artificial semantic failure.

**GREEN.** Implement the narrow comparison at the existing request owner and
rerun native request/response and token-bound regressions.

**REFACTOR.** Preserve validation and final upstream serialization ownership;
do not introduce another parser or infer that all body copies are redundant.

### Scenario 7 — Supplied Vertex tokens do not construct an unused client

**Behavior.** A caller-supplied access token is presented as the same sensitive
bearer without creating an unused network client. Metadata/service-account/network
sources still exchange, refresh and fail under their existing rules.

**RED.** Add the [planned access-token construction proof](#supported-optimization-proof)
at the existing source owner, retaining network-source and Vertex routing tests.
The current unnecessary client construction fails its owner-path assertion;
preserve token secrecy and output before changing construction.

**GREEN.** Remove the unused setup without widening the token cache or changing
network-backed sources. Rerun symmetric measurement and provider tests.

**REFACTOR.** Keep one OAuth owner and reused transport capabilities; no new cache,
provider-specific bypass or public auth option.

### Scenario 8 — Profile-supported small reductions ship with trace/evidence fidelity

**Behavior.** Tracing, hosted setup and capture-drain candidates are ranked from
full-path evidence. Each supported small reduction is implemented with equivalent
trace/validation/caller/capture behavior; necessary work is rejected with concrete
counterevidence. Profiling plus a future-work list is insufficient.

**RED.** Before each selected change, choose/extend the relevant
[candidate regressions](#profile-informed-candidate-proof) and record the precise
source/profile cost. Changed branches need a focused failing assertion; already
correct semantics use regression plus structural and measured cost proof. A new
test name must be recorded with its exact package/selector before RED.

**GREEN.** Remove only supported redundant work at current owners, retain all
required telemetry/validation/ownership, and rerun preceding scenarios and affected
journeys. Do not skip a supported correction by calling it a later task.

**REFACTOR.** Reuse existing execution and trace/capture owners; no new authority,
cache, scheduler, retry protocol or memory-accounting framework.

### Scenario 9 — Canonical reports explain the complete measured result

**Behavior.** Fresh and saved reports identify both process populations, logical
versus attempt counts, full server versus residual time, phases, allocations,
profile mode/coverage and all repetitions. Old server-only records remain readable
with unavailable client fields. Regeneration needs no raw artifact directory.

**RED.** Extend [saved-report proof](#canonical-report-proof) with distinct driver/
server values, retries/cancellations and three different repetitions. Current
reports lack these distinctions; zero-filled gaps or role substitution must fail.

**GREEN.** Persist readings and optional/defaulted role/mode evidence in existing
schema-3 owners and render through the one canonical workflow. Include partial
profile and clock-alignment limits without weakening validity.

**REFACTOR.** Remove ad hoc extraction from the required reporting path. Keep
profiling analysis reproducible and source-backed without another report format.

## Acceptance Criteria

| Obligations | Proof, owner and failure observation |
|---|---|
| REQ-017; AC-013 | Scenario 1 and startup/default-off tests prove one backend/registry per benchmark process and no ordinary-mode initialization. Missing labels, conflicting backend fallback or ordinary timers/allocations fail. |
| REQ-003,017–018; AC-013–014 | Scenario 2/census proves exact roots/attempts/reached phases across success/error/cancel/replay/late cohorts. Dropped series, exceeded caps, duplicated durations or cross-process differencing fails. |
| REQ-005–006,018; AC-014 | Scenario 3/journeys preserve original boundaries and explicitly measure local exchange, dispatch, handoff and hosted setup. Remote-compute claims from exchange duration or added overlapping quantiles fail. |
| REQ-019; AC-015 | Scenario 4, sampler/mode/dependency tests and actual artifacts prove symmetric CPU/heap collection, allocators, process roles and default-off SDK dependency closure. Server resource accounting including driver or CPU-only heap sampling fails. |
| REQ-020; AC-016 | Scenario 5 and real SDK model journey reconcile local upstream/verdict/citation/usage/evidence under existing authority. Assertion-only coverage presented as model coverage or changed default judgments fails. |
| REQ-021; AC-017 | Scenarios 6–8 deliver the two required reductions and all supported small selected reductions, with fidelity tests and evidence-based rejections. Required trace suppression, weaker auth/capture, unsupported savings or deferred supported fixes fail. |
| REQ-003,007,011,022; AC-018 | Scenario 9 and protocol below preserve canonical readback, per-process profile scope/units/binaries, observer-cost qualification and measured before/after results. Missing profiles/counts, unmeasured final edits or a report-only handoff fails. |

## Implementation Reference

- **Shared recording:** `wyrd-benchmark::{install,enabled,label_request,record_timing_parts,snapshot}`,
  `BenchmarkSnapshot::since`, standard histogram storage and grouped snapshot fences.
  Current timing and metric limits are 256/1024. Extend existing grouping/census,
  not a second pipeline. Ordinary SDK adds this plain dependency only.
- **Startup/client:** capacity `main.rs` and `capacity::install_tracing` currently
  omit a registry when log env is absent. `wyrd-client::{observe,verify}` and
  `transport/http.rs` own SDK preparation and shared buffered/request attempt paths;
  `transport/trace.rs` owns scoped headers. Instrument current `Buffered` and
  `request_json_with_min_timeout`, not removed wrappers. Raw Gateway workload
  bypasses SDK; `gateway_capture.rs` owns the local provider.
- **Intervals/records:** Gateway/Verification run owners use `Population`,
  `TimingEvidence`, `Repetition::diagnose`, `keep_timings` and `Record::render`.
  They currently assume server snapshots. Extend roles and default historical
  semantics; persist new diagnostics before publication. `Lifetime`/`Reach` and
  `tests::declared_workloads_fit_recording_bounds` own exact process-lifetime census.
- **Server waits:** `skald-providers::exchange::Exchange` already holds sent,
  headers and ended instants; Gateway `ExecutionTiming` needs the diagnostic split.
  `GatewayInvocation::govern`/`CallTask::run` and `Answer` own dispatch and handoff;
  `verification::service` times `blocks.engine` between existing Load and Engine.
  Prepare is inclusive within Engine; retain those relationships.
- **Profiling:** `profile::{Profiler,HeapProfiler}`, central `experiment::execute`,
  options and CLI forwarding own collectors. `Profiler::start` already accepts
  multiple PIDs. Preserve `Workload::processes` as server accounting. Existing
  `wyrd-server/src/heap_profile.rs` and `CountingMiMalloc` supply sampler/hook
  contracts; move sampler to narrow optional shared ownership and add driver hooks
  without importing server or `wyrd-bench` into the client. Preserve stack trimming.
- **Model Verify:** existing assertion workload and `WyrdState` call owner need a
  separately declared model variant. Existing `pg_verification_routes.rs::judged`
  demonstrates an Agent check with local Gateway route and Verdict response format.
  `PacedUpstream` currently returns prose; extend its existing response fixture for
  a valid cited Verdict rather than pretending sized prose is a model judgment.
- **Small reductions:** `wyrd-gateway/src/request.rs::{Canonical::written,Part}`
  serializes borrowed text to compare canonical bytes; `GoogleOAuth::from_access_token`
  constructs an unused network client. Existing request/OAuth/Vertex tests cover
  semantics. Verification `Blocks` constructs caller-bound facilities before Engine;
  tracing preserves operator-filter OR tenant catalog; capture concat/IPC/digest/
  write owners preserve accounted retryable slices. Revalidate actual named costs.
- **Existing profile evidence:** TASK-004 B profile
  `target/benchmarks/gateway/overhead/20261110T162345Z/profiles/profile-4041372.json.gz`
  matches retained `target/benchmarks/task004/b-old-driver/wyrd-server`, build ID
  `d9e8d5f1874b7354b28db779193850e15f417aa7`. It predates `3ae65fe4a`, covers only
  server and includes heap observer overhead. Its approximately 9–13% metadata
  tracing leaf samples and canonical formatting samples are candidates, not
  current unprofiled wall-time attribution. `/tmp` symbol dictionaries are not
  durable proof; retain reproduction/analysis with the new artifacts/report.

## Verification and Evidence

New selectors below are **planned**. Package/target/runner have been inspected;
confirm each actual selector after adding it, before RED. Run Cargo sequentially.
Default features suffice unless shown. Existing tests remain regression proof;
record any equivalent selector/setup correction rather than weakening assertions.

### Startup and recorder proof

`mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::client_cohorts_record_without_log_environment) | test(=tests::ordinary_clients_do_not_install_or_record_benchmarks)'`

Both planned; exercise startup and SDK calls in isolated processes.

`mise exec -- cargo nextest run --locked -p wyrd-benchmark --lib -E 'test(=tests::standard_metrics_and_request_timings_share_one_snapshot) | test(=tests::snapshots_preserve_complete_timing_groups_during_recording)'`

Existing single-owner, units, independent-progress/group-consistency regressions.

### Client population proof

`mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::driver_and_server_snapshots_keep_distinct_process_populations) | test(=tests::driver_phase_counts_reconcile_completed_and_failed_calls) | test(=tests::declared_workloads_fit_recording_bounds) | test(=tests::all_engine_consumers_read_benchmark_snapshots_without_prometheus)'`

First two planned; existing census/consumer tests must cover both process roles,
all default L1/L2 classes, explicit model diagnostics and invalid evidence.

`mise exec -- cargo nextest run --locked -p wyrd-client --test integration -E 'test(=transport::http::transport_behavior::json_body_and_request_identity_survive_auth_refresh) | test(=transport::http::transport_behavior::non_idempotent_json_is_not_replayed_after_ambiguous_failure) | test(=transport::http::transport_behavior::four_oh_one_re_exchanges_api_key_and_retries_with_fresh_token)'`

Existing local HTTP replay/auth proofs; extend observation assertions through the
actual client owner for failure/decode/cancelled/late cases. No credentials/Postgres.

`mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=transport::http::benchmark_tests::stream_heads_ends_and_cancellation_keep_populations)'`

Planned stream-consumption/head/end/cancellation proof through existing SDK stream
ownership. Retain direct Gateway stream parsing/completion checks as well.

### Server boundary proof

`mise exec -- cargo nextest run --locked -p wyrd-skald-providers --lib -E 'test(=exchange::tests::header_and_body_intervals_preserve_failed_and_retried_exchanges) | test(=exchange::tests::exchanges_are_logged_only_inside_a_scope)'`

First test planned; second existing scope regression.

`mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=components::gateway::telemetry::tests::handoff_diagnostics_preserve_request_boundaries) | test(=verification::hosted::blocks::tests::executor_time_is_verifier_time_not_overhead) | test(=verification::hosted::blocks::tests::histogram_overhead_excludes_real_overlapping_blocks)'`

First test planned. Extend controlled dispatch/setup cases to preserve negative
and overlapping timing rules without sleeps as correctness synchronization.

### Symmetric profile proof

`mise exec -- cargo nextest run --locked -p wyrd-benchmark --features heap --lib -E 'test(=heap::tests::stacks_start_at_the_requesting_code) | test(=heap::tests::samples_live_allocations_from_the_start)'`

Existing sampler tests move with the owner; selectors are planned at the new
package. Preserve allocated/live/refund/null behavior and off-mode hooks.

`mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=profile::tests::cpu_targets_keep_driver_and_server_roles_separate) | test(=profile::tests::driver_and_server_heap_captures_use_the_same_sample_contract) | test(=experiment::tests::profile_modes_start_only_the_requested_collectors) | test(=experiment::tests::profile_manifests_preserve_executable_and_window_identity) | test(=profile::tests::heap_captures_and_failures_become_profile_evidence) | test(=experiment::tests::profiling_is_optional_diagnostic_evidence) | test(=experiment::tests::profile_saving_never_delays_drain_or_budget)'`

First four tests planned; remaining lifecycle/evidence tests exist. Use local
collector fixtures, not a live profiler/OS permission change, for unit failures.

`mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::level_commands_preserve_records_and_exit_precedence)'`

Existing CLI test must cover bare/cpu/heap/both parsing, aggregate forwarding,
invalid values and preserved server-only historical interpretation.

### Model Verification proof

`mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=verification::tests::model_steps_declare_the_existing_sdk_flow_and_local_fixture) | test(=verification::workload::tests::model_fixture_declares_one_gateway_agent_check) | test(=verification::run::tests::model_results_keep_provider_exclusion_and_expected_outcomes)'`

All planned; prove step selection/defaults, the real SDK route and distinct outcome/
provider/citation populations. Existing assertions retain their judgments.

`mise exec -- env WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=resilience::model_verification_benchmark_records_the_complete_local_path) | test(=resilience::benchmark_mode_keeps_capture_and_snapshot_counts_without_prometheus) | test(=resilience::capacity_capture_accounting_waits_for_post_response_work)"'`

First journey planned in the existing capability owner; others exist. Assert real
SDK→Verify→Gateway→local provider, one call, citation/usage, driver/server counts,
retained result and drain. Extend its negative authority/timeout cases as needed.

### Supported optimization proof

`mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=request::tests::canonical_borrowed_strings_match_compact_serialization) | test(=adapter::tests::openai_chat_translates_to_vertex_and_back) | test(=adapter::tests::native_dialects_pass_through_with_adapter_routes_and_auth) | test(=adapter::tests::native_response_validation_preserves_usage_capture_and_answer_bytes)'`

First test planned; others preserve native/Vertex and TASK-004 behavior.

`mise exec -- cargo nextest run --locked -p wyrd-skald-providers --lib -E 'test(=auth::google_oauth::google_auth_oauth::access_token_source_needs_no_network_client) | test(/^auth::google_oauth::google_auth_oauth::/)'`

First test planned; existing owner suite preserves network and token sources.

### Profile-informed candidate proof

`mise exec -- cargo nextest run --locked -p wyrd-telemetry --lib -E 'test(=tests::production_filters_preserve_tenant_traces)'`

Existing mandatory trace floor. Selected trace changes also require exact local
fanout/attribute/error/drain regressions identified and recorded before RED.

`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E "test(=verification::hosted::blocks::pg_tests::direct_execution_reuses_evidence_without_disposable_payload_copies)"'`

Existing evidence/ownership proof. If unused setup is changed, add planned
`verification::hosted::blocks::tests::assertions_skip_unused_agent_setup` and run:

`mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=verification::hosted::blocks::tests::assertions_skip_unused_agent_setup)'`

`mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(/^scribe_outbox::tests::/) | test(/^components::gateway::capture::tests::/)'`

Existing narrow capture/replay/accounting owner suites, required if that candidate
changes. Record exact added selectors before RED for any new branch; this is
implementation proof selection, not permission to defer supported corrections.

### Real SDK and native regression closure

Run existing `mise run build:server` before real-server journeys. No external
provider credentials or synthetic host load are needed.

`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::agent_answer_passes_its_verifier) | test(=verify_in_real_time::agent_answer_fails_its_verifier) | test(=verify_in_real_time::caller_without_the_verifier_grant_is_refused) | test(=verify_in_real_time::another_tenant_cannot_verify_the_assistant) | test(=verify_in_real_time::submitted_answer_is_judged_in_the_background_and_resumes_by_id)"'`

`mise exec -- env WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=compatible::configured_gateway_headers_are_authorized_and_captured) | test(=compatible::openai_compatible_streams_terminate_and_survive_a_capture_outage) | test(=native::anthropic_messages_ingress_is_native_governed_and_fail_closed) | test(=native::gemini_generate_content_ingress_is_native_governed_and_fail_closed)"'`

Existing public success/failure, authority, native/stream and capture regressions.

### Canonical report proof

`mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::reports_keep_repetition_diagnostics_after_artifacts_are_removed) | test(=experiment::tests::reports_regenerate_from_persisted_records)'`

Extend existing tests with role-separated metrics/profile modes/partial failures,
old-record defaults, missing client evidence and three distinct repetitions.

### Owner/static closure

- `mise exec -- cargo nextest run --locked -p wyrd-benchmark --features heap --lib` — recorder/shared sampler owner.
- `mise exec -- cargo nextest run --locked -p wyrd-client --lib` — SDK instrumentation default mode.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(/^gateway::/) | test(/^verification::/)'` — driver/model consumers.
- `mise run fmt:check` — Rust format.
- `mise exec -- cargo clippy --locked -p wyrd-benchmark -p wyrd-client -p wyrd-testing -p wyrd-server -p wyrd-gateway -p wyrd-skald-providers -p wyrd-telemetry --all-features --all-targets -- -D warnings` — affected-owner lint policy.
- `mise run check:deps` — no tooling leak or tier violation.
- `mise exec -- cargo tree --locked -p wyrd-client --edges normal,features` — ordinary client graph must not enable the new `wyrd-benchmark/heap` feature or compile its sampler. Unrelated pre-existing uses of the same packages are not forbidden. Inspect this normal graph separately from tooling feature unification.
- `mise exec -- git diff --check` — patch hygiene plus final source/link inspection.

Do not use full journey suites or `gate` as a bounded-task checklist. Include
focused accounting regressions if actual allocator-hook closure reaches another
owner. No generated schema or public SDK stub changes are selected.

### Bounded execution and performance evidence

Planning runs no builds or experiments. Present this protocol for the spec's
explicit experiment authorization before execution. It is current-code evidence,
not a historical baseline rerun. Build through `mise run bench:build` before each
retained milestone, never during measurement; keep actual sibling copies of
`capacity` and `wyrd-server` with source/diff, hashes/build IDs and feature/settings
manifests under `target/benchmarks/task005/<milestone>/bin/`.

| Milestone | Required evidence |
|---|---|
| U — current | Before new collection/optimization, unprofiled Gateway and assertion Verification subsets. Includes latest TASK-004 final fix, previously unmeasured. |
| M — measurement complete | New symmetric collection, waits and model diagnostics; no selected optimizations yet. Unprofiled, CPU-only and heap-only passes for each engine. Establish observer cost, full path and ranked candidates. |
| O — optimized | Same instrumentation/fixtures/profiling settings as M, with required/supported reductions. Repeat those six passes. No unmeasured last fix is included in claimed results. |

The protocol is fourteen invocations total: U has two; M and O each have six.
Preserve default warmup and three measured repetitions. CPU mode creates a
case-scoped capture at each selected step, so eight selected case captures cover
Gateway buffered/streamed metadata and payload, assertion Verify and model Verify.
No full-suite, L2 load sweep, remote provider or profiler/OS installation is implied.
The new profile values/model steps below are planned command behavior, not
currently supported syntax. Each command is run once at its declared milestone.

U, existing syntax:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task005/current/bin/capacity" l1 gateway --allow-dirty --steps buffered-1kib-capture-metadata.c64,streamed-1kib-capture-metadata.c64,buffered-64kib-capture-metadata.c64,buffered-64kib-capture-payload.c64
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task005/current/bin/capacity" l1 verification --allow-dirty --steps direct-verify-c1,direct-verify-c64
```

M, after scenarios 1–5/9 and their correctness proofs:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task005/measured/bin/capacity" l1 gateway --allow-dirty --steps buffered-1kib-capture-metadata.c64,streamed-1kib-capture-metadata.c64,buffered-64kib-capture-metadata.c64,buffered-64kib-capture-payload.c64
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task005/measured/bin/capacity" l1 verification --allow-dirty --steps direct-verify-c1,direct-verify-c64,direct-verify-model-c1,direct-verify-model-c64
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task005/measured/bin/capacity" l1 gateway --allow-dirty --profile cpu --steps buffered-1kib-capture-metadata.c64,streamed-1kib-capture-metadata.c64,buffered-64kib-capture-metadata.c64,buffered-64kib-capture-payload.c64
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task005/measured/bin/capacity" l1 verification --allow-dirty --profile cpu --steps direct-verify-c1,direct-verify-c64,direct-verify-model-c1,direct-verify-model-c64
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task005/measured/bin/capacity" l1 gateway --allow-dirty --profile heap --steps buffered-1kib-capture-metadata.c64,streamed-1kib-capture-metadata.c64,buffered-64kib-capture-metadata.c64,buffered-64kib-capture-payload.c64
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task005/measured/bin/capacity" l1 verification --allow-dirty --profile heap --steps direct-verify-c1,direct-verify-c64,direct-verify-model-c1,direct-verify-model-c64
```

O, after all supported changes and regressions:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task005/optimized/bin/capacity" l1 gateway --allow-dirty --steps buffered-1kib-capture-metadata.c64,streamed-1kib-capture-metadata.c64,buffered-64kib-capture-metadata.c64,buffered-64kib-capture-payload.c64
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task005/optimized/bin/capacity" l1 verification --allow-dirty --steps direct-verify-c1,direct-verify-c64,direct-verify-model-c1,direct-verify-model-c64
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task005/optimized/bin/capacity" l1 gateway --allow-dirty --profile cpu --steps buffered-1kib-capture-metadata.c64,streamed-1kib-capture-metadata.c64,buffered-64kib-capture-metadata.c64,buffered-64kib-capture-payload.c64
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task005/optimized/bin/capacity" l1 verification --allow-dirty --profile cpu --steps direct-verify-c1,direct-verify-c64,direct-verify-model-c1,direct-verify-model-c64
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task005/optimized/bin/capacity" l1 gateway --allow-dirty --profile heap --steps buffered-1kib-capture-metadata.c64,streamed-1kib-capture-metadata.c64,buffered-64kib-capture-metadata.c64,buffered-64kib-capture-payload.c64
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task005/optimized/bin/capacity" l1 verification --allow-dirty --profile heap --steps direct-verify-c1,direct-verify-c64,direct-verify-model-c1,direct-verify-model-c64
```

Retain exact commands, collection modes, executable/source lineage, both snapshot
streams, role-qualified CPU/heap files, logs, sampling settings, clock/window scope,
all repetitions and canonical reports. Retained source detection uses current cwd;
record each executable's actual build lineage separately. Profile completeness
requires both roles and usable named Wyrd frames/pprof units, not merely a file.
Missing/partial captures are explicit failed proof, never zero work. Do not change
OS permissions/install tools; diagnose available-tool failures and present a bounded
rerun if needed rather than silently repeating until favorable.

Qualify U→M on the unchanged Gateway/assertion cases before attributing optimized
gains. For client p50, server total/residual p50/p99, both CPU/request readings and
throughput, M's median must be no worse than U's worst repetition in the relevant
direction. A miss requires correcting observer overhead or resolving the evidence;
do not hide it with phase omission or production optimization. New model cases
have no U baseline and are not substituted for the old controls. Apply the same
task-local nonregression envelope to M→O, including the new cases. This is not a
new benchmark judgment, statistical-significance assertion or product SLO.

Use CPU-only named stacks for ranking and thread waiting as supporting evidence;
do not translate CPU samples directly into latency percentages. Heap reports use
declared `alloc_space`/`alloc_objects` cumulative step deltas and positive reconciled
whole-step call counts, with sampling/background/setup caveats. Keep live heap/RSS
separate. Model provider work, fixtures and background capture retain their own
counts rather than being divided by an unrelated request population.

The final canonical result must state plainly for each case: did client latency,
total server latency, residual, CPU/request, throughput and allocations improve,
regress or remain unresolved? Include actual before/after values and all repetitions,
not target attainment alone. Distinguish original authority/representation work
from removable cost, source-proven reductions from measured benefit, and any
candidate rejected by evidence. Report both existing p50 objectives independently;
do not infer a median hardware limit from a mean-throughput equation.

## Material Stop Conditions

- Correctness requires changed auth/tenant/trace/capture/durable semantics, a new
  scheduler/cache/owner or another dependency/feature beyond the selected plain
  recorder dependency, extracted heap feature and already-owned sampler packages.
- A proposed optimization suppresses required traces, skips validation, changes
  accepted-work/cancellation or widens model authority. Reject it rather than
  weakening proof. Private constructor/borrow/timing details remain implementor-owned.
- Mandatory observations cannot fit the exact existing bounds under the fixed
  root/component contract. Reconcile the census; no cap widening or missing errors.
- Required experiment authorization or valid role/binary/cohort evidence is absent.
  Finish independent work and present the concrete gap/protocol; do not claim
  completion, move supported implementation elsewhere or disguise missing profiles.

## Authority Links

- [Spec revision 5](../spec.md), [implemented prerequisite](TASK-004-client-and-json-hot-path.md), [existing research](../research/gateway-latency.md).
- [Workflow](../../../spec-driven-development.md), [implementation execution](../../../implementation-execution.md), [testing](../../../../TESTING.md), [repository rules](../../../../AGENTS.md).
- [Benchmark authority](../../../../benchmarks/README.md), [Rust](../../../../agent_docs/rust.md), [maintainer style](../../../../agent_docs/maintainer-style.md), [agent surfaces](../../../../agent_docs/agent-surfaces.md).
- [Security](../../../../architecture/wyrd-security-posture.md), [capture/admission](../../../../architecture/wyrd-design.md#principal-model), [Bifrost/trace and durable ownership](../../../../architecture/bifrost-design.md).

## Execution evidence

Commits: `461bda5a9`, `a3035d1ef` (scenario 1 and shared heap sampler),
`11c9df558` (2), `0b6226369` (3), `3dad3eb7d` (4), `52cb6a0de` (5),
`0aa48a6d3` (9), `2b6b56d31` (6), `06ba3ecbd` (7), `955058166`, `a84284355` (8),
`f13dc2b1f` (assertion steps mark provider diagnostics inapplicable, found in M).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-017; INV-001 — explicit startup enables one recorder per process (scenario 1) | capacity `main.rs` installs the recorder and the System-wrapping `SampledSystem` allocator; `wyrd-benchmark::heap` owns the sampler both processes use | `wyrd-benchmark --features heap --lib` 12/12; capacity bin suite; `cargo tree -p wyrd-client --edges normal,features` shows `wyrd-benchmark` default only, no `heap` | PASS |
| REQ-018 — client, attempt and fixture populations reconcile (scenario 2) | `wyrd-client` `observe`/`transport/http.rs` roots `client.verify`, `client.http`, `client.http.attempt`; raw Gateway `client.gateway`/`.head`; fixture `fixture.upstream` with `parse`/`answer` | `wyrd-client --lib`; capacity `gateway::`/`verification::` suites; journey `resilience::model_verification_benchmark_records_the_complete_local_path` (2 each of client.verify, client.http, verification.direct, fixture.upstream) | PASS |
| REQ-019 — controlled server waits explicit (scenario 3) | Gateway dispatch/resume/provider header and body waits; hosted Verification setup | journeys `benchmark_mode_keeps_capture_and_snapshot_counts_without_prometheus`, `capacity_capture_accounting_waits_for_post_response_work` | PASS |
| REQ-020; AC-016 — CPU/heap/both cover both roles (scenario 4) | `levels::profile`, `ProfileMode`, per-step CPU captures `cpu-<role>-<pid>.json.gz`, heap `heap-<role>-<step>.pb.gz` | `experiment::tests::profile_modes_start_only_the_requested_collectors`, `profile_manifests_preserve_executable_and_window_identity`; every M/O profiled report "complete (8 captures: server 4, driver 4)" | PASS |
| REQ-021; AC-017 — model Verify through a real local Gateway call (scenario 5) | `verification/workload.rs` `bench-judge` Agent check, `gateway_capture::verdict_completion`, `PacedUpstream::answering`, `Op::Judge` | `model_fixture_declares_one_gateway_agent_check`, `model_results_keep_provider_exclusion_and_expected_outcomes`, `model_steps_declare_the_existing_sdk_flow_and_local_fixture`; model journey above; M/O model steps 0 errors, 0 wrong verdicts every repetition | PASS |
| REQ-022 — canonical borrowed strings (scenario 6) | `wyrd-gateway/src/request.rs` `Canonical::borrowed`, `Part::visit_borrowed_str`, borrowed `Name`; escaped/owned strings and other scalars keep `written` | `request::tests::canonical_borrowed_strings_match_compact_serialization` green before the change (equivalence) and after; Vertex/native adapter regressions 4/4; `wyrd-gateway --lib` 70/70; M server profile `format_escaped_str::<&mut Canonical>` 0.55% (64 KiB payload) → absent in O | PASS |
| REQ-022 — supplied Vertex token builds no client (scenario 7) | `skald-providers/src/auth/google_oauth.rs` `http: Option<reqwest::Client>`, `None` only for `AccessToken`; `http()` names the invariant | `google_auth_oauth::access_token_source_needs_no_network_client` (structural: the field was a required client before); `google_auth_oauth::` 7/7, Google/Vertex selectors 12/12 | PASS |
| REQ-022; AC-018 — profile-supported reductions or evidence-backed rejections (scenario 8) | `955058166` plain Verdict schema generated once (`PLAIN_VERDICT`); `a84284355` inline Agent borrowed in `AgentExecutor::resolve` | `wyrd-spec --lib` 759/759 (resolved-schema tests at `execution.rs` plain and data forms); `vala-verify --lib` 57/57; candidate table below | PASS |
| REQ-003,007,011,022; AC-018 — reports explain the complete result (scenario 9) | `experiment.rs` `render_profile` (mode, roles, per-role capture counts, limits), role-labelled diagnostic rows, historical driver-unavailable line | `experiment::tests::reports_keep_repetition_diagnostics_after_artifacts_are_removed`, `reports_regenerate_from_persisted_records` (three distinct repetitions, logical vs attempt counts, partial profile, pre-mode record default); experiment suite 18/18 | PASS |
| REQ-005–007,011; AC-013–015 — U/M/O protocol | fourteen authorized invocations, lineage below | tables below | PASS with unresolved items named |
| U→M qualification on unchanged cases | — | envelope table below | UNRESOLVED (see limits) |

### Experiment lineage

Built with `mise run bench:build` (release, `-C force-frame-pointers=yes`, line
tables, rustc 1.99.0), copies retained under `target/benchmarks/task005/<milestone>/bin/`
with `SHA256SUMS`, `SOURCE` (commit plus porcelain status), `build.log`, every
invocation log and `exits`.

| Milestone | Source | `capacity` SHA-256 | `wyrd-server` SHA-256 | Records (Gateway / Verification) |
|---|---|---|---|---|
| U `current` | `c9a2e27e6` | `7b56e161ca3cc9ff…` | `4888a30a1a0cb8da…` | `20261110T175926Z` / `20261110T175803Z` |
| M `measured` | `0aa48a6d3` | `7d752600fda92cd9…` | `af7910f5ff45f480…` | unprofiled `195615Z` / `195812Z`; CPU `200037Z` / `200235Z`; heap `200434Z` / `200634Z` |
| O `optimized` | `a84284355` | `bc9859a875d3fa24…` | `e0ffadf4776de426…` | unprofiled `202336Z` / `202532Z`; CPU `202730Z` / `202929Z`; heap `203128Z` / `203328Z` |

Exits: U Gateway refused once as host-busy with no workload admitted and was retried
once (VALID); U Verification VALID. M: Gateway VALID; Verification **INVALID**
(`driver.saturated direct-verify-c64` repetition 1: 6.46 driver cores over 6.40);
all four profiled passes VALID with complete profiles. The first launch of the four
M profiled commands passed `--profile <mode>` as one argv word and was refused as a
usage error before any workload was admitted (`*-refused-argv.log`); each was then
run once with the authorized argv. O: all six VALID, profiles complete. The host
carried the owner's concurrent unrelated load during M and O; the owner chose to
keep those results.

### U → M (unchanged Gateway/assertion cases; U median → M median, result against the U worst repetition)

| Step | client p50 ms U→M | server p50 ms | residual p50 µs | residual p99 µs | server CPU µs/call | driver CPU µs/call | throughput /s |
|---|---|---|---|---|---|---|---|
| buffered-1kib c64 | 1.011 → 1.068 MISS | 0.583 → 0.612 MISS | 180.9 → 182.1 MISS | 464.9 → 514.8 MISS | 346.5 → 343.4 within | 50.0 → 58.4 MISS | 59.5k → 55.8k MISS |
| streamed-1kib c64 | 41.07 → 41.23 MISS | 0.267 → 0.289 MISS | 114.3 → 127.6 MISS | 222.6 → 271.9 MISS | 954 → 1030 MISS | 385 → 417 MISS | 1539 → 1535 MISS |
| buffered-64kib meta c64 | 2.113 → 2.592 MISS | 1.222 → 1.465 MISS | 354.6 → 363.8 MISS | 1282 → 1602 MISS | 707 → 715 MISS | 164 → 185 MISS | 28.8k → 23.4k MISS |
| buffered-64kib payload c64 | 4.52 → 4.67 MISS | 3.48 → 3.60 within | 2722 → 2742 within | 53.9k → 71.2k MISS | 1561 → 1605 within | 240 → 267 MISS | 9472 → 7985 MISS |
| direct-verify c1 | 0.080 → 0.084 MISS | 0.0395 → 0.0406 within | — | 151.7 → 156.4 MISS | 117 → 123 within | 26.8 → 29.7 MISS | 9837 → 9306 MISS |
| direct-verify c64 | 0.714 → 0.762 MISS | 0.111 → 0.113 MISS | — | 288 → 258 within | 223 → 230 MISS | 72.4 → 80.7 MISS | 82.8k → 78.7k MISS |

Values are medians; every repetition is listed under Per-repetition comparisons. Driver CPU per
call rises 8–17% in every case. Measured driver observer cost from the M CPU
captures: `wyrd_benchmark::Observation` drop/finish 5.5% of driver CPU (1 KiB),
3.3% (64 KiB metadata), 2.7% (payload), with `BenchmarkRecorder::handle`
(standard-macro component histograms) 1.8%; no lock contention (std mutex
`lock_contended` 0.01%). That accounts for at most about half the driver increase;
the rest and the server-side movement coincide with the concurrent host load, which
also explains movement on paths M did not change. The qualification is therefore
**unresolved**, not passed, and the M Verification invocation is INVALID.

### M → O (same instrumentation, fixtures and profile settings)

| Step | client p50 ms M→O | server p50 ms | residual p50 µs | residual p99 µs | server CPU µs/call | driver CPU µs/call | throughput /s | result |
|---|---|---|---|---|---|---|---|---|
| buffered-1kib c64 | 1.068 → 1.048 | 0.612 → 0.601 | 182.1 → 180.7 | 514.8 → 514.8 | 343 → 349 | 58.4 → 57.7 | 55.8k → 57.1k | within envelope; remain unresolved within noise |
| streamed-1kib c64 | 41.23 → 41.09 | 0.289 → 0.276 | 127.6 → 120.4 | 271.9 → 222.6 | 1030 → 950 | 417 → 392 | 1535 → 1536 | improved, within envelope |
| buffered-64kib meta c64 | 2.592 → 2.249 | 1.465 → 1.285 | 363.8 → 350.0 | 1602 → 1383 | 715 → 723 | 185 → 184 | 23.4k → 27.1k | improved latency/throughput; CPU within |
| buffered-64kib payload c64 | 4.667 → 4.646 | 3.596 → 3.803 | 2742 → 3049 **MISS** | 71.2k → 64.7k | 1605 → 1645 | 267 → 264 | 7985 → 7842 | residual p50 regressed; others within |
| direct-verify c1 | 0.084 → 0.092 **MISS** | 0.0406 → 0.0442 **MISS** | — | 156.4 → 180.1 **MISS** | 123 → 147 **MISS** | 29.7 → 34.2 **MISS** | 9306 → 8071 **MISS** | regressed |
| direct-verify c64 | 0.762 → 0.789 **MISS** | 0.113 → 0.109 | — | 258 → 336 **MISS** | 230 → 222 | 80.7 → 80.3 | 78.7k → 74.4k **MISS** | client/residual p99/throughput regressed |
| direct-verify-model c1 | 1.251 → 1.112 | 0.889 → 0.813 | — | 380.7 → 380.4 | 1395 → 1287 | 307 → 243 | 761 → 840 | improved |
| direct-verify-model c64 | 6.666 → 5.837 | 4.026 → 3.588 | — | 2875 → 2236 | 2448 → 2260 | 401 → 318 | 9141 → 10.5k | improved |

Model cases have no U baseline. The assertion Verify path and the payload capture
path are not changed by any M→O commit (`git diff 0aa48a6d3 a84284355` touches
Gateway request canonicalization, Google OAuth construction, Verdict schema
resolution, inline Agent resolution and assertion-step diagnostic labelling only);
their regressions are measured, not attributed to a mechanism, and remain
**unresolved** under the concurrent host load.

Existing objectives at O, reported independently: client p50 < 1 ms — 1 KiB
buffered 1.048 ms missed, 64 KiB metadata 2.249 ms missed, payload 4.646 ms missed,
streamed is a 41 ms paced stream; Gateway residual p50 < 500 µs — 1 KiB 180.7 µs
met, streamed 120.4 µs met, 64 KiB metadata 350.0 µs met, payload 3049 µs missed.
Verification overhead p99 < 0.5 ms at c64: assertion 336 µs met.

### Profile ranking and candidate decisions (M CPU captures, named Wyrd frames)

| Candidate | Evidence | Decision |
|---|---|---|
| SDK result decode recompiles the inline Agent Prompt and regenerates the Verdict schema | model c64 driver: `VerificationResult` decode 52.6% of driver CPU; `compile_prompt` 46.8%; `Verdict::response_format` 20.5% (schemars `json_schema` 10.9%); heap: `compile_with` 19.9% of driver allocation objects | **implemented** `955058166`: plain schema generated once. O: `response_format` 4.4%, driver CPU/call −21%, client p50 −11/−12%, driver allocation objects 356.7M → 276.9M per model c64 step |
| Meta-validation of the compiled output schema on decode | `check_output_schema` 12.6% (M) / 15.6% (O) of model-case driver CPU | **rejected here**: generic Prompt validation in `Prompt::new` owned by Skald; skipping it for a known schema needs a validated-schema identity or cache (material stop: new cache). Next candidate |
| Server copies the inline Agent per Agent check | `AgentSpec::clone` 2.19% of model c64 server CPU, from `AgentExecutor::resolve` 1.25% and `Judged::report` 0.94% | **implemented** `a84284355` for `resolve` (absent in O); `report` copy kept: the result owns its exact check configuration (wire contract) |
| Canonical comparison formats borrowed strings | `format_escaped_str::<&mut Canonical>` 0.55% of payload-case server CPU | **implemented** `2b6b56d31`; absent in O |
| Unused hosted Agent setup on assertion checks | `HostedBlocks::engine` 1.64% inclusive (assertion c64), mostly inlined `Arc` construction; `CallerTraces::new` 0.33% | **rejected**: ≤1.3% and laziness would reshape the executor owner; `assertions_skip_unused_agent_setup` not added |
| Tracing span creation | `Span::new` 2.7–4.1% of server CPU; `sharded_slab` get 1.8%; `EnvFilter::cares_about_span` 0.8% (1 KiB) | **rejected**: required tenant trace floor (`tests::production_filters_preserve_tenant_traces`); no supported redundant span found |
| Capture drain / payload write | `GatewayInvocation::capture` 6.1% of payload-case server CPU; ZSTD, Arrow `to_data`, sha256 leaves | **rejected**: required accounted payload capture work |
| Benchmark observer (server) | `BenchmarkRecorder::handle` via standard `metrics::histogram!` 1.5% (1 KiB), 0.6% (payload) | measured observer cost; the same per-call registration a production recorder performs, not removed |
| Benchmark observer (driver) | `Observation` drop 5.5% / 3.3% / 2.7% of driver CPU (Gateway 1 KiB / 64 KiB / payload) | measured; components share the standard family the census reads, so a separate fast path would be a second pipeline. Remains the leading driver-side observer cost |
| Heap sampler stack capture | `capture_stack` 16.5% of driver allocation objects in heap mode | heap-mode observer cost only; heap passes are diagnostic, never qualification |

### Closure verification (at `a84284355`)

| Check | Result |
|---|---|
| `mise run fmt:check`; clippy `--all-features --all-targets -D warnings` (benchmark, client, testing, server, gateway, skald-providers, telemetry, spec, vala-verify); `mise run check:deps`; `git diff --check` | PASS |
| `cargo tree -p wyrd-client --edges normal,features` | PASS: `wyrd-benchmark` default only |
| `wyrd-benchmark --features heap --lib` 12/12; `wyrd-client --lib` 271/271; `wyrd-testing --bin capacity` 65/65; `wyrd-testing --lib` (Postgres) 67/67 | PASS |
| `wyrd-telemetry --lib --features test-support -E 'test(=tests::production_filters_preserve_tenant_traces)'` | PASS (without `test-support` the lib test target does not compile: `log_writer.rs` test reaches `crate::tests`, gated on that feature on `main`; the family lane unifies the feature) |
| `wyrd-server --lib --features test-support` hosted blocks, Scribe outbox, Gateway capture (Postgres) 39/39; `mise run build:server` | PASS |
| `wyrd-ai` verify journeys 5/5; Gateway journeys 7/7 including `resilience::model_verification_benchmark_records_the_complete_local_path` | PASS |
| capacity release-server `tests::pg_tests::engine_measurements_keep_boundaries_processes_and_windows`, `verification::run::tests::calibration::the_measurement_path_agrees_with_independent_references` | **NOT VERIFIED**: the first launch ran both concurrently against one Postgres catalog (Bifrost refused another run's warehouse metadata; calibration counted no successful `client.verify` outcomes); the serial rerun with `--test-threads=1` was refused `host.busy` (unrelated `rustc` builds, 11.8%/18.7% of logical CPUs) with no workload admitted. Rerun serially on an idle host |

### Limits

- U→M and the assertion/payload M→O comparisons are confounded by concurrent host
  load the owner kept; an idle-host rerun of the unprofiled M and O pair is the
  bounded follow-up that would resolve them.
- Heap step deltas are sampled `alloc_space`/`alloc_objects` estimates including
  each step's warmup and settle; they are not divided by call counts.
- CPU percentages rank named stacks; they are not latency attributions.
- Profiling scripts used for ranking: a samply-capture summarizer resolving frames
  against the retained binaries' symbol tables and a pprof reader, kept outside the
  repository; every number above is reproducible from the retained captures and
  binaries.

### Non-goals

No L2 sweep, remote provider, profiler or OS install, generated schema or public
SDK stub change. Untracked review folders were left unchanged.

### Per-repetition comparisons

Envelope: the later median must not be worse than the earlier worst repetition. `p50_ms`/`p99_ms` are server latency; `overhead_*` is the residual.

#### U → M

##### buffered-1kib-capture-metadata.c64
| metric | U reps | M reps | M median | envelope | result |
|---|---|---|---|---|---|
| client_p50_ms | 0.986 / 1.011 / 1.014 | 1.01 / 1.069 / 1.068 | 1.068 (+5.6% vs 1.011) | ≤ 1.014 | MISS, regressed |
| client_p99_ms | 2 / 2.059 / 2.063 | 2.143 / 2.36 / 2.29 | 2.29 (+11.2% vs 2.059) | ≤ 2.063 | MISS, regressed |
| p50_ms | 0.5688 / 0.5832 / 0.5847 | 0.5811 / 0.6134 / 0.6118 | 0.6118 (+4.9% vs 0.5832) | ≤ 0.5847 | MISS, regressed |
| p99_ms | 1.304 / 1.343 / 1.333 | 1.375 / 1.526 / 1.486 | 1.486 (+11.4% vs 1.333) | ≤ 1.343 | MISS, regressed |
| overhead_p50_ms | 0.1782 / 0.1809 / 0.181 | 0.178 / 0.1823 / 0.1821 | 0.1821 (+0.7% vs 0.1809) | ≤ 0.181 | MISS, regressed |
| overhead_p99_ms | 0.4649 / 0.4639 / 0.4721 | 0.4982 / 0.5258 / 0.5148 | 0.5148 (+10.7% vs 0.4649) | ≤ 0.4721 | MISS, regressed |
| cpu_us_per_request | 333.5 / 347.5 / 346.5 | 336.2 / 356.7 / 343.4 | 343.4 (-0.9% vs 346.5) | ≤ 347.5 | within, improved |
| driver_cpu_us_per_call | 49.63 / 49.99 / 50.16 | 57.21 / 58.37 / 59.11 | 58.37 (+16.7% vs 49.99) | ≤ 50.16 | MISS, regressed |
| rate | 6.101e+04 / 5.954e+04 / 5.942e+04 | 5.918e+04 / 5.562e+04 / 5.578e+04 | 5.578e+04 (-6.3% vs 5.954e+04) | ≥ 5.942e+04 | MISS, regressed |

##### streamed-1kib-capture-metadata.c64
| metric | U reps | M reps | M median | envelope | result |
|---|---|---|---|---|---|
| client_p50_ms | 41.09 / 41.07 / 41.06 | 41.08 / 41.23 / 41.39 | 41.23 (+0.4% vs 41.07) | ≤ 41.09 | MISS, regressed |
| client_p99_ms | 42.62 / 42.42 / 42.45 | 42.63 / 42.78 / 42.61 | 42.63 (+0.4% vs 42.45) | ≤ 42.62 | MISS, regressed |
| p50_ms | 0.2711 / 0.2668 / 0.2647 | 0.2811 / 0.2893 / 0.3021 | 0.2893 (+8.4% vs 0.2668) | ≤ 0.2711 | MISS, regressed |
| p99_ms | 0.5468 / 0.4982 / 0.4836 | 0.5873 / 0.7491 / 0.577 | 0.5873 (+17.9% vs 0.4982) | ≤ 0.5468 | MISS, regressed |
| overhead_p50_ms | 0.1151 / 0.1143 / 0.1137 | 0.1229 / 0.1276 / 0.135 | 0.1276 (+11.6% vs 0.1143) | ≤ 0.1151 | MISS, regressed |
| overhead_p99_ms | 0.229 / 0.2226 / 0.2184 | 0.2719 / 0.3502 / 0.2637 | 0.2719 (+22.1% vs 0.2226) | ≤ 0.229 | MISS, regressed |
| cpu_us_per_request | 978.7 / 953.7 / 943.2 | 991.2 / 1030 / 1069 | 1030 (+8.0% vs 953.7) | ≤ 978.7 | MISS, regressed |
| driver_cpu_us_per_call | 387.8 / 384.6 / 379.4 | 400.6 / 416.8 / 440.7 | 416.8 (+8.4% vs 384.6) | ≤ 387.8 | MISS, regressed |
| rate | 1536 / 1539 / 1539 | 1537 / 1535 / 1533 | 1535 (-0.2% vs 1539) | ≥ 1536 | MISS, regressed |

##### buffered-64kib-capture-metadata.c64
| metric | U reps | M reps | M median | envelope | result |
|---|---|---|---|---|---|
| client_p50_ms | 2.119 / 2.113 / 2.062 | 2.592 / 2.639 / 2.539 | 2.592 (+22.7% vs 2.113) | ≤ 2.119 | MISS, regressed |
| client_p99_ms | 4.098 / 4.126 / 4.029 | 5.042 / 5.218 / 5.122 | 5.122 (+25.0% vs 4.098) | ≤ 4.126 | MISS, regressed |
| p50_ms | 1.222 / 1.222 / 1.191 | 1.465 / 1.492 / 1.435 | 1.465 (+19.9% vs 1.222) | ≤ 1.222 | MISS, regressed |
| p99_ms | 2.748 / 2.716 / 2.652 | 3.398 / 3.488 / 3.439 | 3.439 (+26.6% vs 2.716) | ≤ 2.748 | MISS, regressed |
| overhead_p50_ms | 0.3553 / 0.3546 / 0.353 | 0.3651 / 0.3638 / 0.3571 | 0.3638 (+2.6% vs 0.3546) | ≤ 0.3553 | MISS, regressed |
| overhead_p99_ms | 1.342 / 1.282 / 1.271 | 1.571 / 1.661 / 1.602 | 1.602 (+24.9% vs 1.282) | ≤ 1.342 | MISS, regressed |
| cpu_us_per_request | 710.1 / 707.3 / 686.1 | 715.2 / 751 / 702.6 | 715.2 (+1.1% vs 707.3) | ≤ 710.1 | MISS, regressed |
| driver_cpu_us_per_call | 164.5 / 163.6 / 159.6 | 185.1 / 187.3 / 180.7 | 185.1 (+13.2% vs 163.6) | ≤ 164.5 | MISS, regressed |
| rate | 2.867e+04 / 2.879e+04 / 2.927e+04 | 2.339e+04 / 2.298e+04 / 2.367e+04 | 2.339e+04 (-18.7% vs 2.879e+04) | ≥ 2.867e+04 | MISS, regressed |

##### buffered-64kib-capture-payload.c64
| metric | U reps | M reps | M median | envelope | result |
|---|---|---|---|---|---|
| client_p50_ms | 4.538 / 4.52 / 4.367 | 4.914 / 4.667 / 4.54 | 4.667 (+3.3% vs 4.52) | ≤ 4.538 | MISS, regressed |
| client_p99_ms | 67.48 / 50.18 / 55.32 | 94.01 / 73.71 / 63.29 | 73.71 (+33.3% vs 55.32) | ≤ 67.48 | MISS, regressed |
| p50_ms | 3.146 / 3.682 / 3.482 | 3.803 / 3.574 / 3.596 | 3.596 (+3.3% vs 3.482) | ≤ 3.682 | within, regressed |
| p99_ms | 66.49 / 49.61 / 54.69 | 93.06 / 72.29 / 62.39 | 72.29 (+32.2% vs 54.69) | ≤ 66.49 | MISS, regressed |
| overhead_p50_ms | 0.9242 / 2.98 / 2.722 | 2.798 / 2.595 / 2.742 | 2.742 (+0.8% vs 2.722) | ≤ 2.98 | within, regressed |
| overhead_p99_ms | 65.93 / 48.89 / 53.9 | 92.01 / 71.24 / 61.77 | 71.24 (+32.2% vs 53.9) | ≤ 65.93 | MISS, regressed |
| cpu_us_per_request | 1709 / 1561 / 1556 | 1756 / 1588 / 1605 | 1605 (+2.8% vs 1561) | ≤ 1709 | within, regressed |
| driver_cpu_us_per_call | 261.2 / 239.3 / 239.5 | 266.5 / 268.1 / 262.8 | 266.5 (+11.2% vs 239.5) | ≤ 261.2 | MISS, regressed |
| rate | 8775 / 9563 / 9472 | 6940 / 7985 / 8086 | 7985 (-15.7% vs 9472) | ≥ 8775 | MISS, regressed |

##### direct-verify-c1
| metric | U reps | M reps | M median | envelope | result |
|---|---|---|---|---|---|
| client_p50_ms | 0.078 / 0.08 / 0.083 | 0.089 / 0.084 / 0.083 | 0.084 (+5.0% vs 0.08) | ≤ 0.083 | MISS, regressed |
| client_p99_ms | 0.277 / 0.301 / 0.309 | 0.316 / 0.31 / 0.295 | 0.31 (+3.0% vs 0.301) | ≤ 0.309 | MISS, regressed |
| p50_ms | 0.0384 / 0.03949 / 0.04083 | 0.04246 / 0.04057 / 0.04006 | 0.04057 (+2.8% vs 0.03949) | ≤ 0.04083 | within, regressed |
| p99_ms | 0.1395 / 0.1533 / 0.1582 | 0.1601 / 0.1587 / 0.1449 | 0.1587 (+3.5% vs 0.1533) | ≤ 0.1582 | MISS, regressed |
| overhead_p99_ms | 0.1377 / 0.1517 / 0.1563 | 0.1578 / 0.1564 / 0.1434 | 0.1564 (+3.1% vs 0.1517) | ≤ 0.1563 | MISS, regressed |
| overhead_mean_ms | 0.0463 / 0.04877 / 0.05094 | 0.05352 / 0.05036 / 0.04854 | 0.05036 (+3.3% vs 0.04877) | ≤ 0.05094 | within, regressed |
| cpu_us_per_verification | 105 / 126.2 / 117.1 | 123.1 / 130.6 / 111.8 | 123.1 (+5.1% vs 117.1) | ≤ 126.2 | within, regressed |
| driver_cpu_us_per_call | 25.28 / 26.83 / 27.74 | 32.22 / 29.65 / 28.15 | 29.65 (+10.5% vs 26.83) | ≤ 27.74 | MISS, regressed |
| rate | 1.036e+04 / 9837 / 9444 | 8690 / 9306 / 9661 | 9306 (-5.4% vs 9837) | ≥ 9444 | MISS, regressed |

##### direct-verify-c64
| metric | U reps | M reps | M median | envelope | result |
|---|---|---|---|---|---|
| client_p50_ms | 0.714 / 0.716 / 0.699 | 0.754 / 0.762 / 0.779 | 0.762 (+6.7% vs 0.714) | ≤ 0.716 | MISS, regressed |
| client_p99_ms | 1.972 / 1.911 / 1.937 | 1.845 / 1.925 / 2.137 | 1.925 (-0.6% vs 1.937) | ≤ 1.972 | within, improved |
| p50_ms | 0.11 / 0.1115 / 0.1114 | 0.1114 / 0.1126 / 0.1134 | 0.1126 (+1.0% vs 0.1114) | ≤ 0.1115 | MISS, regressed |
| p99_ms | 0.3044 / 0.2908 / 0.286 | 0.2541 / 0.2601 / 0.2867 | 0.2601 (-10.6% vs 0.2908) | ≤ 0.3044 | within, improved |
| overhead_p99_ms | 0.3005 / 0.288 / 0.2834 | 0.2514 / 0.2577 / 0.2839 | 0.2577 (-10.5% vs 0.288) | ≤ 0.3005 | within, improved |
| overhead_mean_ms | 0.1142 / 0.1157 / 0.1154 | 0.1135 / 0.1154 / 0.1182 | 0.1154 (+0.0% vs 0.1154) | ≤ 0.1157 | within, regressed |
| cpu_us_per_verification | 223.3 / 228.8 / 222.2 | 222.5 / 230.2 / 230.5 | 230.2 (+3.1% vs 223.3) | ≤ 228.8 | MISS, regressed |
| driver_cpu_us_per_call | 72.96 / 72.36 / 71.84 | 80.69 / 80.72 / 81.32 | 80.72 (+11.5% vs 72.36) | ≤ 72.96 | MISS, regressed |
| rate | 8.256e+04 / 8.276e+04 / 8.446e+04 | 8.008e+04 / 7.866e+04 / 7.599e+04 | 7.866e+04 (-5.0% vs 8.276e+04) | ≥ 8.256e+04 | MISS, regressed |

##### direct-verify-model-c1
| metric | U reps | M reps | M median | envelope | result |
|---|---|---|---|---|---|
| client_p50_ms | — no U baseline | 1.254 / 1.246 / 1.251 | 1.251 | — | new case |
| client_p99_ms | — no U baseline | 2.21 / 2.236 / 2.973 | 2.236 | — | new case |
| p50_ms | — no U baseline | 0.894 / 0.8883 / 0.8888 | 0.8888 | — | new case |
| p99_ms | — no U baseline | 1.661 / 1.673 / 2.142 | 1.673 | — | new case |
| overhead_p99_ms | — no U baseline | 0.3756 / 0.3807 / 0.4918 | 0.3807 | — | new case |
| overhead_mean_ms | — no U baseline | 0.2088 / 0.2084 / 0.2182 | 0.2088 | — | new case |
| cpu_us_per_verification | — no U baseline | 1392 / 1395 / 1447 | 1395 | — | new case |
| driver_cpu_us_per_call | — no U baseline | 304.6 / 306.8 / 327.7 | 306.8 | — | new case |
| rate | — no U baseline | 761.4 / 762.4 / 732.2 | 761.4 | — | new case |

##### direct-verify-model-c64
| metric | U reps | M reps | M median | envelope | result |
|---|---|---|---|---|---|
| client_p50_ms | — no U baseline | 7.63 / 6.666 / 6.003 | 6.666 | — | new case |
| client_p99_ms | — no U baseline | 14.72 / 13.24 / 10.41 | 13.24 | — | new case |
| p50_ms | — no U baseline | 4.538 / 4.026 / 3.658 | 4.026 | — | new case |
| p99_ms | — no U baseline | 10.09 / 8.962 / 7.057 | 8.962 | — | new case |
| overhead_p99_ms | — no U baseline | 3.209 / 2.875 / 1.83 | 2.875 | — | new case |
| overhead_mean_ms | — no U baseline | 0.4828 / 0.468 / 0.4338 | 0.468 | — | new case |
| cpu_us_per_verification | — no U baseline | 2448 / 2469 / 2297 | 2448 | — | new case |
| driver_cpu_us_per_call | — no U baseline | 409.5 / 401.1 / 388.9 | 401.1 | — | new case |
| rate | — no U baseline | 8030 / 9141 / 1.033e+04 | 9141 | — | new case |


#### M → O

##### buffered-1kib-capture-metadata.c64
| metric | M reps | O reps | O median | envelope | result |
|---|---|---|---|---|---|
| client_p50_ms | 1.01 / 1.069 / 1.068 | 1.004 / 1.048 / 1.092 | 1.048 (-1.9% vs 1.068) | ≤ 1.069 | within, improved |
| client_p99_ms | 2.143 / 2.36 / 2.29 | 2.042 / 2.217 / 2.609 | 2.217 (-3.2% vs 2.29) | ≤ 2.36 | within, improved |
| p50_ms | 0.5811 / 0.6134 / 0.6118 | 0.5786 / 0.6011 / 0.6257 | 0.6011 (-1.8% vs 0.6118) | ≤ 0.6134 | within, improved |
| p99_ms | 1.375 / 1.526 / 1.486 | 1.323 / 1.444 / 1.67 | 1.444 (-2.8% vs 1.486) | ≤ 1.526 | within, improved |
| overhead_p50_ms | 0.178 / 0.1823 / 0.1821 | 0.178 / 0.1807 / 0.1823 | 0.1807 (-0.8% vs 0.1821) | ≤ 0.1823 | within, improved |
| overhead_p99_ms | 0.4982 / 0.5258 / 0.5148 | 0.4454 / 0.5148 / 0.556 | 0.5148 (+0.0% vs 0.5148) | ≤ 0.5258 | within, same |
| cpu_us_per_request | 336.2 / 356.7 / 343.4 | 336.5 / 352 / 348.9 | 348.9 (+1.6% vs 343.4) | ≤ 356.7 | within, regressed |
| driver_cpu_us_per_call | 57.21 / 58.37 / 59.11 | 57.04 / 57.74 / 58.28 | 57.74 (-1.1% vs 58.37) | ≤ 59.11 | within, improved |
| rate | 5.918e+04 / 5.562e+04 / 5.578e+04 | 5.976e+04 / 5.707e+04 / 5.37e+04 | 5.707e+04 (+2.3% vs 5.578e+04) | ≥ 5.562e+04 | within, improved |

##### streamed-1kib-capture-metadata.c64
| metric | M reps | O reps | O median | envelope | result |
|---|---|---|---|---|---|
| client_p50_ms | 41.08 / 41.23 / 41.39 | 41.09 / 41.1 / 41.09 | 41.09 (-0.3% vs 41.23) | ≤ 41.39 | within, improved |
| client_p99_ms | 42.63 / 42.78 / 42.61 | 42.66 / 42.84 / 42.74 | 42.74 (+0.3% vs 42.63) | ≤ 42.78 | within, regressed |
| p50_ms | 0.2811 / 0.2893 / 0.3021 | 0.2808 / 0.2762 / 0.2726 | 0.2762 (-4.5% vs 0.2893) | ≤ 0.3021 | within, improved |
| p99_ms | 0.5873 / 0.7491 / 0.577 | 0.5898 / 0.5955 / 0.6344 | 0.5955 (+1.4% vs 0.5873) | ≤ 0.7491 | within, regressed |
| overhead_p50_ms | 0.1229 / 0.1276 / 0.135 | 0.1219 / 0.1204 / 0.1191 | 0.1204 (-5.7% vs 0.1276) | ≤ 0.135 | within, improved |
| overhead_p99_ms | 0.2719 / 0.3502 / 0.2637 | 0.2267 / 0.2203 / 0.2226 | 0.2226 (-18.1% vs 0.2719) | ≤ 0.3502 | within, improved |
| cpu_us_per_request | 991.2 / 1030 / 1069 | 965.6 / 949.9 / 935.7 | 949.9 (-7.8% vs 1030) | ≤ 1069 | within, improved |
| driver_cpu_us_per_call | 400.6 / 416.8 / 440.7 | 391.7 / 391.7 / 382.6 | 391.7 (-6.0% vs 416.8) | ≤ 440.7 | within, improved |
| rate | 1537 / 1535 / 1533 | 1536 / 1536 / 1536 | 1536 (+0.1% vs 1535) | ≥ 1533 | within, improved |

##### buffered-64kib-capture-metadata.c64
| metric | M reps | O reps | O median | envelope | result |
|---|---|---|---|---|---|
| client_p50_ms | 2.592 / 2.639 / 2.539 | 2.249 / 2.249 / 2.271 | 2.249 (-13.2% vs 2.592) | ≤ 2.639 | within, improved |
| client_p99_ms | 5.042 / 5.218 / 5.122 | 4.309 / 4.31 / 4.488 | 4.31 (-15.9% vs 5.122) | ≤ 5.218 | within, improved |
| p50_ms | 1.465 / 1.492 / 1.435 | 1.285 / 1.285 / 1.298 | 1.285 (-12.3% vs 1.465) | ≤ 1.492 | within, improved |
| p99_ms | 3.398 / 3.488 / 3.439 | 2.863 / 2.906 / 2.998 | 2.906 (-15.5% vs 3.439) | ≤ 3.488 | within, improved |
| overhead_p50_ms | 0.3651 / 0.3638 / 0.3571 | 0.3405 / 0.351 / 0.35 | 0.35 (-3.8% vs 0.3638) | ≤ 0.3651 | within, improved |
| overhead_p99_ms | 1.571 / 1.661 / 1.602 | 1.319 / 1.383 / 1.407 | 1.383 (-13.6% vs 1.602) | ≤ 1.661 | within, improved |
| cpu_us_per_request | 715.2 / 751 / 702.6 | 695.4 / 723 / 732.8 | 723 (+1.1% vs 715.2) | ≤ 751 | within, regressed |
| driver_cpu_us_per_call | 185.1 / 187.3 / 180.7 | 178 / 184.1 / 184.3 | 184.1 (-0.5% vs 185.1) | ≤ 187.3 | within, improved |
| rate | 2.339e+04 / 2.298e+04 / 2.367e+04 | 2.706e+04 / 2.705e+04 / 2.653e+04 | 2.705e+04 (+15.6% vs 2.339e+04) | ≥ 2.298e+04 | within, improved |

##### buffered-64kib-capture-payload.c64
| metric | M reps | O reps | O median | envelope | result |
|---|---|---|---|---|---|
| client_p50_ms | 4.914 / 4.667 / 4.54 | 5.065 / 4.62 / 4.646 | 4.646 (-0.4% vs 4.667) | ≤ 4.914 | within, improved |
| client_p99_ms | 94.01 / 73.71 / 63.29 | 76.26 / 66.38 / 58.67 | 66.38 (-9.9% vs 73.71) | ≤ 94.01 | within, improved |
| p50_ms | 3.803 / 3.574 / 3.596 | 3.942 / 3.803 / 3.774 | 3.803 (+5.8% vs 3.596) | ≤ 3.803 | within, regressed |
| p99_ms | 93.06 / 72.29 / 62.39 | 75.04 / 65.44 / 57.54 | 65.44 (-9.5% vs 72.29) | ≤ 93.06 | within, improved |
| overhead_p50_ms | 2.798 / 2.595 / 2.742 | 2.925 / 3.127 / 3.049 | 3.049 (+11.2% vs 2.742) | ≤ 2.798 | MISS, regressed |
| overhead_p99_ms | 92.01 / 71.24 / 61.77 | 73.2 / 64.72 / 56.13 | 64.72 (-9.2% vs 71.24) | ≤ 92.01 | within, improved |
| cpu_us_per_request | 1756 / 1588 / 1605 | 1929 / 1645 / 1614 | 1645 (+2.5% vs 1605) | ≤ 1756 | within, regressed |
| driver_cpu_us_per_call | 266.5 / 268.1 / 262.8 | 294.6 / 263.6 / 261.9 | 263.6 (-1.1% vs 266.5) | ≤ 268.1 | within, improved |
| rate | 6940 / 7985 / 8086 | 7088 / 7842 / 8772 | 7842 (-1.8% vs 7985) | ≥ 6940 | within, regressed |

##### direct-verify-c1
| metric | M reps | O reps | O median | envelope | result |
|---|---|---|---|---|---|
| client_p50_ms | 0.089 / 0.084 / 0.083 | 0.09 / 0.092 / 0.103 | 0.092 (+9.5% vs 0.084) | ≤ 0.089 | MISS, regressed |
| client_p99_ms | 0.316 / 0.31 / 0.295 | 0.398 / 0.418 / 0.439 | 0.418 (+34.8% vs 0.31) | ≤ 0.316 | MISS, regressed |
| p50_ms | 0.04246 / 0.04057 / 0.04006 | 0.04281 / 0.04422 / 0.04931 | 0.04422 (+9.0% vs 0.04057) | ≤ 0.04246 | MISS, regressed |
| p99_ms | 0.1601 / 0.1587 / 0.1449 | 0.1752 / 0.1824 / 0.1939 | 0.1824 (+14.9% vs 0.1587) | ≤ 0.1601 | MISS, regressed |
| overhead_p99_ms | 0.1578 / 0.1564 / 0.1434 | 0.1727 / 0.1801 / 0.1917 | 0.1801 (+15.1% vs 0.1564) | ≤ 0.1578 | MISS, regressed |
| overhead_mean_ms | 0.05352 / 0.05036 / 0.04854 | 0.05481 / 0.05684 / 0.0628 | 0.05684 (+12.9% vs 0.05036) | ≤ 0.05352 | MISS, regressed |
| cpu_us_per_verification | 123.1 / 130.6 / 111.8 | 127.3 / 150.6 / 147.1 | 147.1 (+19.4% vs 123.1) | ≤ 130.6 | MISS, regressed |
| driver_cpu_us_per_call | 32.22 / 29.65 / 28.15 | 33.82 / 34.19 / 37.1 | 34.19 (+15.3% vs 29.65) | ≤ 32.22 | MISS, regressed |
| rate | 8690 / 9306 / 9661 | 8277 / 8071 / 7383 | 8071 (-13.3% vs 9306) | ≥ 8690 | MISS, regressed |

##### direct-verify-c64
| metric | M reps | O reps | O median | envelope | result |
|---|---|---|---|---|---|
| client_p50_ms | 0.754 / 0.762 / 0.779 | 0.789 / 0.832 / 0.788 | 0.789 (+3.5% vs 0.762) | ≤ 0.779 | MISS, regressed |
| client_p99_ms | 1.845 / 1.925 / 2.137 | 2.194 / 2.47 / 2.204 | 2.204 (+14.5% vs 1.925) | ≤ 2.137 | MISS, regressed |
| p50_ms | 0.1114 / 0.1126 / 0.1134 | 0.1064 / 0.1111 / 0.1092 | 0.1092 (-3.0% vs 0.1126) | ≤ 0.1134 | within, improved |
| p99_ms | 0.2541 / 0.2601 / 0.2867 | 0.3384 / 0.3802 / 0.3341 | 0.3384 (+30.1% vs 0.2601) | ≤ 0.2867 | MISS, regressed |
| overhead_p99_ms | 0.2514 / 0.2577 / 0.2839 | 0.3356 / 0.3768 / 0.3308 | 0.3356 (+30.3% vs 0.2577) | ≤ 0.2839 | MISS, regressed |
| overhead_mean_ms | 0.1135 / 0.1154 / 0.1182 | 0.1124 / 0.1199 / 0.1154 | 0.1154 (-0.1% vs 0.1154) | ≤ 0.1182 | within, improved |
| cpu_us_per_verification | 222.5 / 230.2 / 230.5 | 217.9 / 235.5 / 222.3 | 222.3 (-3.4% vs 230.2) | ≤ 230.5 | within, improved |
| driver_cpu_us_per_call | 80.69 / 80.72 / 81.32 | 80.27 / 83.56 / 80.09 | 80.27 (-0.6% vs 80.72) | ≤ 81.32 | within, improved |
| rate | 8.008e+04 / 7.866e+04 / 7.599e+04 | 7.456e+04 / 6.967e+04 / 7.443e+04 | 7.443e+04 (-5.4% vs 7.866e+04) | ≥ 7.599e+04 | MISS, regressed |

##### direct-verify-model-c1
| metric | M reps | O reps | O median | envelope | result |
|---|---|---|---|---|---|
| client_p50_ms | 1.254 / 1.246 / 1.251 | 1.216 / 1.101 / 1.112 | 1.112 (-11.1% vs 1.251) | ≤ 1.254 | within, improved |
| client_p99_ms | 2.21 / 2.236 / 2.973 | 2.338 / 2.026 / 2.101 | 2.101 (-6.0% vs 2.236) | ≤ 2.973 | within, improved |
| p50_ms | 0.894 / 0.8883 / 0.8888 | 0.8888 / 0.8054 / 0.8125 | 0.8125 (-8.6% vs 0.8888) | ≤ 0.894 | within, improved |
| p99_ms | 1.661 / 1.673 / 2.142 | 1.761 / 1.545 / 1.63 | 1.63 (-2.6% vs 1.673) | ≤ 2.142 | within, improved |
| overhead_p99_ms | 0.3756 / 0.3807 / 0.4918 | 0.4155 / 0.3697 / 0.3804 | 0.3804 (-0.1% vs 0.3807) | ≤ 0.4918 | within, improved |
| overhead_mean_ms | 0.2088 / 0.2084 / 0.2182 | 0.2188 / 0.1985 / 0.202 | 0.202 (-3.2% vs 0.2088) | ≤ 0.2182 | within, improved |
| cpu_us_per_verification | 1392 / 1395 / 1447 | 1429 / 1269 / 1287 | 1287 (-7.7% vs 1395) | ≤ 1447 | within, improved |
| driver_cpu_us_per_call | 304.6 / 306.8 / 327.7 | 273.3 / 239.7 / 242.7 | 242.7 (-20.9% vs 306.8) | ≤ 327.7 | within, improved |
| rate | 761.4 / 762.4 / 732.2 | 768.3 / 851.1 / 840.3 | 840.3 (+10.4% vs 761.4) | ≥ 732.2 | within, improved |

##### direct-verify-model-c64
| metric | M reps | O reps | O median | envelope | result |
|---|---|---|---|---|---|
| client_p50_ms | 7.63 / 6.666 / 6.003 | 5.708 / 5.936 / 5.837 | 5.837 (-12.4% vs 6.666) | ≤ 7.63 | within, improved |
| client_p99_ms | 14.72 / 13.24 / 10.41 | 10.16 / 11.98 / 10.91 | 10.91 (-17.6% vs 13.24) | ≤ 14.72 | within, improved |
| p50_ms | 4.538 / 4.026 / 3.658 | 3.496 / 3.629 / 3.588 | 3.588 (-10.9% vs 4.026) | ≤ 4.538 | within, improved |
| p99_ms | 10.09 / 8.962 / 7.057 | 7.135 / 8.049 / 7.561 | 7.561 (-15.6% vs 8.962) | ≤ 10.09 | within, improved |
| overhead_p99_ms | 3.209 / 2.875 / 1.83 | 1.897 / 2.302 / 2.236 | 2.236 (-22.2% vs 2.875) | ≤ 3.209 | within, improved |
| overhead_mean_ms | 0.4828 / 0.468 / 0.4338 | 0.4405 / 0.459 / 0.4573 | 0.4573 (-2.3% vs 0.468) | ≤ 0.4828 | within, improved |
| cpu_us_per_verification | 2448 / 2469 / 2297 | 2165 / 2300 / 2260 | 2260 (-7.7% vs 2448) | ≤ 2469 | within, improved |
| driver_cpu_us_per_call | 409.5 / 401.1 / 388.9 | 314.1 / 319.3 / 318.4 | 318.4 (-20.6% vs 401.1) | ≤ 409.5 | within, improved |
| rate | 8030 / 9141 / 1.033e+04 | 1.083e+04 / 1.023e+04 / 1.053e+04 | 1.053e+04 (+15.2% vs 9141) | ≥ 8030 | within, improved |

