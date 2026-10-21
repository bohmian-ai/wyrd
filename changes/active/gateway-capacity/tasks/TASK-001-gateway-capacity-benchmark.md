---
id: TASK-001
kind: implementation
status: ready
spec: SPEC-gateway-capacity
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, INV-001, INV-002, INV-003, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009]
depends_on: []
---

# AI gateway overhead and sustained-capacity benchmarks

The user explicitly approved the complete revision 2 specification and this task
on 2026-10-20. This task is ready for implementation;
execute with [$wyrd-implement](../../../../.agents/skills/wyrd-implement/SKILL.md).
The caller owns scheduling and Git actions. There are no other task dependencies.

## Outcome and Value

An operator can run two opt-in commands over one real release gateway harness:
`bench:gateway:overhead` measures the client → server → client roundtrip and
separates upstream/server overhead, while `bench:gateway:capacity` finds the
highest tested fixed arrival rate meeting a declared SLO under delayed/paced
upstream responses and tenant contention. Separate verdicts prevent a sub-ms
server overhead result from being mistaken for sustainable deployment capacity.
Both commands follow existing verification/Bifrost instrumentation, profiling,
profile-generation and report/artifact standards; see REQ-008/AC-006.

One task closes the shared production instrumentation, common measurements,
fixtures, workload modes, verdicts, commands and regression/evidence consumers.
Overhead is implemented and measured before adding the sustained mode. The
second mode extends the same workflow rather than creating a second harness.
This task maps every requirement, invariant and acceptance criterion in revision 2.

The original measurement and integrity obligations remain mandatory:

1. Expect an initial overhead FAIL, preserve the threshold and use phase evidence
   to rank follow-up fixes; do not implement speculative caching/accounting changes.
2. Gate every row on zero outbox loss and complete successful capture accounting,
   including work after answer delivery. Do not change gateway reservation policy.
3. Move shared Window/Histogram/breakdown into `wyrd_testing::capacity`, close the
   query and verification consumers, and keep Report/Row workload-specific.

## Owners, Scope, Consumers, and Prohibited Changes

The existing server gateway owns ingress, admission, settlement and capture;
`wyrd-gateway` owns attempts/adapters/relay; Skald owns actual provider HTTP;
`wyrd-testing` owns capacity measurements, fixtures and the benchmark. Mise owns
invocation. These boundaries are retained. Raw HTTP is the actual OpenAI gateway
surface: adding inference to the administration-only Wyrd SDK would be scope
expansion. This change adds no language-specific SDK contract; existing language
journeys remain integrated regression coverage, not three new implementations.

The following reuse map was checked against current working source. Paths are
guidance, not an implementation allowlist.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Real release process, tenant and resources | `release_server.rs::LocalServer::{start,stop,metrics}`, `Metrics`, `MemoryPeak`; `capacity.rs::ResourceWindow`, `release_binary` | `verification/mod.rs::benchmark`, `verification/workload.rs::Tenant::provision`; query runner | No gateway workload | Reuse launcher/setup/resources and public admin provisioning | None; no new lifecycle, resource sampler or server fixture |
| Buffered/streamed upstream and payload fixtures | `gateway_capture.rs::GatewayCapture::completion`; harness native SSE; `compatible.rs::CHAT_EVENTS` | `resilience.rs::deploy_compatible`, operations mock mounting, compatible stream/outage journey | Benchmark-sized bodies, no-recording mock and common SSE library fixture | Consolidate fixtures in existing `gateway_capture`; preserve all journey imports | Only workload-specific request generation; installed wiremock/reqwest suffice |
| Arrival, parse and gateway phases | `RequestArrival`, `GatewayIngress::authenticate`, `GatewayApiKeys::authenticate`, `typed_body`, `GatewayInvocation::{govern,admit_call,decide,attribute,admit,account}`, `CallTask` | `/v1/chat/completions` handlers; native/compatible/resilience real-server journeys; verification telemetry owner | No overhead/phase families; `received` starts too late | Extend existing request/execution owners with bounded production telemetry | No replacement invocation owner; timing must span decode as well as typed validation |
| Upstream intervals and stream completion | `HttpProviderDispatch`, `GatewayEngine`, `CredentialResolver::resolve`, `adapter/stream.rs::Relay`, `EventStream`; Skald `OpenAiClient::{send_raw,stream_raw}`, `raw::send_raw`, HTTP send/read helpers in `clients/mod.rs` | Buffered `settle_buffered`, `settle_stream`; compatible truncation/outage journey; resilience cancellation | Utc attempt time includes Wyrd work; no precise transient exchange/end clocks | Instrument actual send/read boundary and relay/body handoff; carry transient timing through existing execution | Narrow Skald timing seam only if needed; no second HTTP transport or persisted timing contract |
| Prometheus exposition | `app/metrics.rs::install_recorder`, `VERIFICATION_DURATION_BUCKETS`; verification telemetry | Existing recorder bucket tests, `Metrics::buckets`, verification runner | Gateway families export summaries; four new families absent | Register gateway duration families with existing fine buckets | No new recorder/parser/dependency |
| Capture and zero-loss correctness | `GatewayInvocation::capture`, `components/gateway/capture.rs::record`, `service.rs::capture_from`, `Outbox::stage`, ScribeOutbox | Compatible capture-outage journey; verification drain/report; `GatewayCapture::capture_call` | No capacity loss gate; capture counter includes drops, source default disabled | Explicit metadata/payload policies; count staged only, reject drops/loss, settle step boundaries and drain | No new staging path, reservation/refusal or durability mechanism |
| Shared measurement | `capacity.rs::{Percentiles,deltas,quantile,ResourceWindow}`; verification `Window`, `Histogram`, `breakdown`; query specialized `Window`/`ProfileDelta` | Verification `Bench::run`/report; query `Bench` and query/write/profile consumers; existing capacity delta unit tests | Workload-local windows; verification-local histogram/breakdown | Move common measurement behavior to `wyrd_testing::capacity`, update all three workloads | User explicitly requires extraction now; query has no Histogram/breakdown copy to move |
| Verdict, artifacts, profiling and iteration | Verification `Report::{direct,write_to}`, `direct_miss`, `correctness_miss`, `parse_cases`; `profile::Capture`; capacity main; mise verification/query tasks | Verification `Bench::run`, query report/window/profile path; registered capacity bin and gateway integration target | No gateway verdict/report/dispatch/tasks, verification has no client filter | One gateway workflow with two verdict modes and opt-in commands; use existing per-step profiler and report conventions | No replacement profiling, launcher or report framework |
| Client roundtrip and attribution | Verification `Measured::{percentiles,mean_latency}`, `Bench::drive`, `report::client_extra` | `Report::direct` prints client p50/p95/p99 alongside server total/overhead and mean client extra | Gateway has no raw client completion/head/first-event samples | Extend gateway workload measurement to full validated roundtrip and matched server-total attribution | No new SDK inference client; installed reqwest drives the real public wire |
| Fixed-rate sustained arrivals | `client_queue/step.rs::Bench::run`, `Plan`, `Record`; shared resource windows | `client_queue/mod.rs::benchmark` sustained/headroom steps; `slope_reads_the_trailing_window` regression | Observation submission pacing is not a bounded concurrent HTTP/stream load driver | Reuse fixed-rate evidence conventions, resources and delta windows in the gateway workload; extend only its arrival driving | A gateway-specific bounded HTTP driver is earned by simultaneous in-flight paced responses; do not copy the Drift SDK producer |
| Paced upstream and tenant contention | `gateway_capture.rs::GatewayCapture::completion`; installed axum/HTTP-body/Tokio facilities; `LocalServer::start` accepts tenant slugs | `resilience::deploy_compatible`, existing SSE journeys; client-queue `Tenant::provision` and release-server setup loop | wiremock's single mock body cannot pace SSE writes; no gateway multi-tenant rate case | Extend existing gateway fixture owner with controlled delayed/paced responses; provision four tenant-scoped deployments through existing setup/admin | Only streaming fixture support is new; no production transport, generic load framework or alternate server lifecycle |

Prohibited: INV-001/002 changes, new dependency/feature, migrations, SDK APIs,
provider-wire expansion, CI performance gating, synthetic host load, unrelated
cleanup, new OS-specific mechanisms or broad resource-launcher rewrites. Preserve
all existing working-tree changes. Functional checks must pass even when measured
capacity fails. Do not treat an expected performance FAIL as permission to ignore
a functional failure.

## Approach

1. Revalidate source facts, feature unification and approved revision; establish
   verification/Bifrost measurement, profiling and consumer characterization.
2. Extract shared capacity measurement behavior and close query/verification
   consumers without changing their workload-specific report semantics.
3. Add precise production telemetry through existing arrival, gateway execution,
   provider HTTP and response-body owners, plus production histogram buckets.
4. Consolidate fixtures, provision the real-server deployment/policies/client
   keys and implement the closed-loop overhead matrix, full client roundtrip
   measurements and default per-step profile generation.
5. Close overhead verdicts and evidence; extend that harness with bounded
   fixed-rate arrivals, paced upstream fixtures and multi-tenant capacity cases,
   preserving separate traffic, drain and correctness windows.
6. Add capacity SLO/envelope and overload/recovery verdicts, both mise commands,
   verification client-filter parity, and collect numeric plus profiled evidence
   for both modes with phase-ranked follow-up recommendations.
7. Run focused functional proof, affected owner checks and final diff audit.

## Ordered Implementation Scenarios

Test identifiers below marked **planned** are new tests to create, not claims
that they exist today. Packages/targets and existing module owners were checked
in Cargo manifests/source. Commands in the proof table are exact focused commands
for those names. Confirm selection with nextest list after creation; zero selected
tests is never evidence. Private fixtures and implementation structure remain
implementer-owned.

### Scenario 1 — Shared measurements preserve consumer meaning

**Behavior.** Gateway/verification use common histogram and breakdown deltas;
query uses common window resources without losing specialized scan/write/profile
counters. New series, empty intervals and independent label selections retain
correct quantiles/counts. REQ-007, AC-005, INV-003.

**RED.** Add planned test S1. It must fail behaviorally if the shared owner cannot
represent the required workload selections/deltas; do not count an unresolved
import as RED. Existing extraction-only behavior gets characterization/no-op proof
instead of a manufactured failure.

**GREEN.** Extend the current capacity owner minimally, move shared measurement
behavior and update all consumers. Reuse Percentiles/deltas/ResourceWindow rather
than creating another sampler. Rerun S1 and existing capacity tests.

**REFACTOR.** Remove superseded local common definitions; keep query-specific
profile/scan logic and workload Report/Row local. Preserve output/interval semantics.

### Scenario 2 — Real requests expose precise bucketed overhead

**Behavior.** Buffered and streaming requests expose the new metric labels,
separate stream points and all applicable phases starting at RequestArrival.
Total server duration is exposed alongside residual overhead. Actual exchange
time is subtracted across attempts; auth/parse/credential/decode
work is retained. Truncation, cancellation and refusal retain existing outcomes.
REQ-003/004/011, AC-002/009, INV-001.

**RED.** Add planned S2 real-server HTTP/served-metrics journey and S3 recorder
exposition test. Expected failures are absent metric series/buckets, late arrival
coverage or incorrect timing attribution. Add deterministic timing assertions
through the existing timing/provider owners to distinguish delayed upstream from
Wyrd preparation/decode and prior-attempt intervals; avoid wall-clock p99 assertions
in ordinary tests. Stream validation must cover head and final body handoff.

**GREEN.** Instrument existing owners and carry only transient monotonic durations.
Reach Skald's actual send/read boundary when needed; timing an entire adapter
future is insufficient. Register all eight duration families using the existing
fine buckets. Rerun S1–S3 and exact stream/cancellation regressions R1/R2 below.

**REFACTOR.** Consolidate timing with the existing execution lifecycle. Keep
bounded labels, cancellation cleanup and ownership clear; no utility owner or
parallel provider transport. Do not change persisted attempt/accounting shapes.

### Scenario 3 — Overhead roundtrips follow verification measurement and profiling

**Behavior.** Cases/client selections produce the specified ten default steps;
unsupported/missing/empty selections fail, case 3 remains limited to 16/64, and
verification gains the same client filter. Correct 32-chunk SSE is fully consumed.
Metadata capture is explicitly enabled for cases 1/2 and request/response payload
capture for case 3. The full client request-to-validated-answer roundtrip is
timed, including body consumption; streaming retains headers, first complete data and validated EOF
separately. Raw samples, p50/p95/p99/mean, server total/overhead/upstream, comparable
client-extra mean and per-step generated profiles follow verification/Bifrost.
No-profile needs no perf yet retains all numeric evidence.
REQ-001/002/008/011, AC-001/006/009.

**RED.** Add planned S4 command/matrix test and exercise the selected release
journey after dispatch is available. Expected failures: wrong selections, absent
client filter, missing fixture/policy, a client timer stopping at response
headers, absent raw roundtrip samples, mismatched mean attribution, missing
profile artifacts, or profiler invoked in no-profile mode. Extend S2 with delayed
headers and delayed body events proving client receipt timing is independent of
server handoff; use deterministic synchronization rather than unit-test p99
thresholds. Use S9 for profiled artifact/no-profile parity. Actual measured
latency may change with profiler overhead; only evidence fields and verdict rules match.
Characterize moved SSE fixtures before changing journey imports.

**GREEN.** Reuse LocalServer/setup, public gateway administration, per-client keys,
shared fixtures and no-recording wiremock. Keep fixed 2/10 s windows and 999 Hz.
Add overhead dispatch/mise invocation and filter parity. Emit roundtrip and
server timing side by side without subtracting mock delay or percentiles.
Rerun S1–S4 and fixture consumer regressions; inspect actual selected-step output.

**REFACTOR.** Reuse existing hand-parsed flag conventions and workload ownership;
no generic CLI framework, alternate server bootstrap or skip-build mechanism.

### Scenario 4 — Overhead loss or weak evidence can never earn PASS

**Behavior.** Strict p99/sample/memory gates, separate stream-point gates, response
correctness, staged capture equality and all-outbox zero loss apply to every
overhead workload row. Exactly 1 ms, 999 samples, missing/non-finite metrics,
counter reset,
a 200 stream with terminal error, any capture drop or any outbox loss fails.
TARGET at <=500 us never overrides a correctness failure. Client latency is
reported only. REQ-005, AC-003.

**RED.** Add planned S5 table-driven verdict test. Include p99 below/at/above
1 ms, 999/1000 samples, 500 us TARGET, each stream point failing independently,
headroom boundary, non-2xx/transport/SSE errors, zero/one lost event and a capture
counter whose total equals arrivals but whose staged count does not. Expected
failure is a false PASS or incorrect boundary verdict.

**GREEN.** Implement the minimum workload-local judgment over shared measurements.
Require explicit valid evidence for every gate, not default-zero missing values.
Rerun S1–S5; exercise capture outage through R1 without changing caller semantics.

**REFACTOR.** Keep thresholds and Row/Report with gateway; share measurement only.
Remove duplicate gate paths while preserving independent failure reasons.

### Scenario 5 — Post-answer work and shutdown cannot hide losses

**Behavior.** Warmup captures cannot leak into measured arrivals; delayed captures
are included through bounded drain. Drain and shutdown rows require zero loss and
successful capture completion. Failure/timeout exits nonzero and leaves partial
report/samples/logs. REQ-005/008, AC-003/006, INV-001.

**RED.** Add planned S6 real-server journey exercising delayed post-response
capture, terminal capture failure and drain observation through current seams.
Expected failure: premature zero-loss PASS, false capture mismatch from warmup,
missing final loss evidence or lost artifacts. Add shutdown/drain judgment cases
to S5; force timeout via deterministic existing seams, not arbitrary sleeps.

**GREEN.** Separate traffic and correctness intervals, finish in-flight calls,
settle capture/Scribe within 60 s, and retain shutdown evidence under the existing
45 s process grace. Do not pretend a stopped server can be scraped. If final
loss-free evidence cannot be established, fail clearly rather than assume it.
Rerun S1–S6 and R1/R2; check the real binary's exit status and artifacts.

**REFACTOR.** Reuse the process/task/outbox lifecycle; no new relay, WAL, capture
reservation, staging table, persistence path or public metrics-control endpoint.

### Scenario 6 — Initial performance failure guides the next change

**Behavior.** The initial benchmark can report overhead FAIL without relaxing
requirements; evidence identifies case/client/point and ranks fixes from phase
totals. No-profile and profiled reports retain identical verdict semantics.
REQ-006/008, AC-004/006.

**RED.** Extend planned S5 to assert that an over-limit measurement remains FAIL
when profiling is absent and that loss remains FAIL even with excellent latency.
A synthetic failing report pins reporting behavior; do not manufacture a missing
production feature if reporting already satisfies it.

**GREEN.** Run the quick command, full no-profile matrix and profiled matrix below.
Record actual exit codes, all failed gates, sample sufficiency and phase rankings.
Keep expected performance FAIL as FAIL. Rank candidate snapshot-cache/admin-write
invalidation and buffered-accounting changes only where measurements support them;
propose alternatives if other phases dominate. No such optimization is implemented.

**REFACTOR.** Simplify report/evidence rendering while preserving raw measurements,
profile completeness and the distinction between functional PASS and overhead FAIL.

### Scenario 7 — Fixed arrivals and real upstream pacing remain observable

**Behavior.** Capacity runs the REQ-009 delayed buffered, paced-stream and
four-tenant uneven traffic profiles against the same real-server lifecycle.
Arrival scheduling does not wait for responses, retry refusals, burst overdue
arrivals or grow an unbounded queue. Driver misses/cap hits/excessive send lateness
invalidate a rate. Per-tenant/mode samples preserve scheduled/send/first-event/
completion and matched upstream-send/chunk-receipt times. REQ-009, AC-007, INV-001/003.

**RED.** Add planned S7 for rate/case/SLO validation and open-loop evidence. It
must fail when delayed responses reduce offered rate, a missed arrival vanishes,
invalid limits are accepted, or a single-body mock is mistaken for paced delivery.
Extend planned S8 real-server journey to prove delayed headers and distinct body
writes, parse SSE independent of transport boundaries, validate responses and
read captures only under the expected tenant. Characterize existing fixture
consumers before extending their owner.

**GREEN.** Extend the existing gateway workload and fixtures with bounded HTTP
arrivals and actual paced writes using installed facilities. Reuse LocalServer's
multi-tenant setup and public gateway administration. Keep identifiers in local
benchmark evidence, not new unbounded production metric labels. Rerun S1–S8 and
R1/R2; confirm no-profile still emits all numeric evidence.

**REFACTOR.** Share provisioning/fixtures/timing/resources/profiling with overhead.
Keep rate driving and workload-specific judgments in the existing gateway owner;
no generic load platform, second launcher, metrics parser or provider transport.

### Scenario 8 — Capacity verdicts separate SLO limits from invalid evidence

**Behavior.** REQ-010 classifies each tested rate, judges declared SLOs per
applicable tenant/mode, detects sustained backlog growth, and reports the highest
passing tested rate plus the first failing rate or an unlocated ceiling. No
sub-ms residual gate is applied to paced streams. Capture eligibility follows
unchanged production refusal/admission semantics. AC-008, INV-001/002.

**RED.** Extend S5 with declared-bound equality, insufficient per-tenant samples,
quiet-tenant SLO failure hidden by an aggregate, driver limitation, one-second
backlog growth boundaries, lost/dropped/misattributed capture, and offered versus
capture-eligible counts. Assert an outside-SLO exploration row can coexist with a
valid lower passing rate, but invalid evidence, no passing rate or integrity
failure cannot return success. Reject an interpolated maximum or false ceiling.

**GREEN.** Add capacity-specific verdicts over shared measurements; require all
applicable SLO inputs and explicit sample sufficiency. Preserve actual failed
rate evidence, finite bounds and common drain/shutdown requirements. Rerun S5/S7/S8
and the earlier overhead scenarios; overload refusal allowances never leak into
overhead or sustainable-rate verdicts.

**REFACTOR.** Reuse measurement/report rendering while keeping overhead and
capacity judgments explicit. Do not create a production quota/fairness change or
weaken existing outbox/capture semantics to make higher rates pass.

### Scenario 9 — Overload recovers and both modes retain generated profiles

**Behavior.** Twice the highest passing rate is driven for 30 s, then reduced to
that rate for the specified recovery interval. Only explicit configured admission
refusals are allowed during overload. Accepted-response integrity, bounded memory,
zero loss, per-tenant recovery SLOs, backlog restoration and common drain/shutdown
remain mandatory. A run with no observed saturation says so. Default runs of both
commands generate usable per-step CPU/off-CPU profile files, metadata and
recorder diagnostics under the existing replica directory convention, plus
verification/Bifrost-style breakdowns;
no-profile is an iteration mode, not a substitute for profiled release evidence.
REQ-008/010, AC-006/008.

**RED.** Extend S5 with expected refusal versus unexpected error/timeout, bad 200
stream, loss after answer, failed recovery and unobserved saturation. Extend S8
to prove configured admission refusals under higher offered load followed by
successful lower-rate requests and correct tenant-attributed capture completion.
Add planned S9 for default profiler invocation, retained profile files/completeness,
isolated mode output directories and the same numeric evidence fields and verdict
rules with no-profile.
Use existing test seams for deterministic correctness; host speed does not decide
ordinary tests.

**GREEN.** Drive the specified high/low sequence through existing production
admission. Render separate overload/recovery/drain/shutdown rows and retain partial
artifacts on error. Reuse `profile::Capture` and existing release build flags for
both modes, collect actual profile artifacts with resolved Wyrd frames and valid
metadata, and run both complete release benchmarks. Rerun all new focused scenarios and R1/R2. Profile collection gaps are
explicit and cannot be presented as complete evidence.

**REFACTOR.** Keep one gateway harness and reuse its lifecycle/artifact handling;
no second profiler, test-only admission bypass or production reservation change.

## Acceptance Criteria

- AC-001: The real release-server journey and exact matrix/flags/fixtures are
  demonstrated, including client roundtrip timing, both server stream points and
  no-profile operation (S2/S4, release commands). Verification filter parity
  preserves its default steps.
- AC-002: S2/S3 and deterministic timing proof establish arrival, transport,
  terminal coverage and bucket exposition; R1/R2 preserve failure behavior.
- AC-003: S5/S6 and release evidence enforce all gates, post-answer drain and
  loss-free shutdown, including terminal errors that arrive with HTTP 200.
- AC-004: Actual benchmark results and phase-ranked recommendations are retained;
  expected overhead FAIL is neither hidden nor mistaken for a broken harness.
- AC-005: S1, existing capacity tests and query/verification consumer proof show
  the common owner is used without changing their measurement/report semantics.
- AC-006: Build/feature parity, opt-in task placement, artifact preservation and
  functional checks pass; INV-001/002/003 remain intact. Both default modes emit
  per-step profiles and all benchmark-standard breakdowns (S9/release evidence).
- AC-007: S7/S8 and release evidence prove fixed offered rates, real pacing,
  declared-SLO validation, bounded driver evidence and tenant ownership.
- AC-008: S5/S8 prove independent capacity/SLO and integrity judgments; release
  evidence records the tested envelope, high/low recovery and separate safety,
  drain and shutdown verdicts without silently forgiving evidence loss.
- AC-009: S2/S4/S9 prove complete client roundtrip raw samples and comparable
  attribution in both profile modes, separately from server residual overhead.

## Expected Write Set and Consumer Closure

Likely owners: `crates/wyrd/wyrd-testing/src/capacity.rs`, existing
`src/bin/capacity/{main.rs,verification/,query/}` and a gateway workload;
`src/gateway_capture.rs` and its gateway journey consumers; server
`src/app/metrics.rs`, `src/http/middleware/metrics.rs` and
`src/components/gateway/{ingress.rs,routes.rs,invocation.rs}`; gateway
`src/{engine.rs,credential.rs,adapter/http.rs,adapter/stream.rs}` and response
execution/body types; narrow Skald `src/{raw.rs,clients/mod.rs,clients/openai.rs}`
timing seams if required; `mise.toml`; owner-local unit tests and existing
`wyrd-testing/tests/gateway/` integration target.

No contract/codegen/migration or language binding change is expected. Do not
hard-code a file/type allowlist; adjacent private support can be earned by the
owning behavior. Reuse `Metrics`, resources, profiler and client admin transport.
Inspect nearest AGENTS/manifests before edits and preserve unrelated work.

## Verification and Evidence

Execute scenarios sequentially using Red-Green-Refactor. Planned names in this
table are the requested test identities, not existing tests verified to pass.
The existing `gateway` integration target is registered in wyrd-testing's manifest;
its journeys are ignored, use profile `journey`, and require managed Postgres.

| Ref | Test / exact focused command |
|---|---|
| S1 (planned) | `mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=capacity::tests::shared_measurements_preserve_workload_deltas)'` |
| S2 (planned) | `mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E 'test(=resilience::gateway_capacity_metrics_cover_buffered_and_streamed_calls)'` |
| S3 (planned) | `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=app::metrics::tests::gateway_duration_families_render_buckets)'` |
| S4 (planned) | `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=gateway::tests::case_and_client_selection_preserves_matrix)'` |
| S5 (planned) | `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=gateway::report::tests::every_row_requires_latency_correctness_and_zero_loss)'` |
| S6 (planned) | `mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E 'test(=resilience::capacity_capture_accounting_waits_for_post_response_work)'` |
| S7 (planned) | `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=gateway::tests::capacity_arrivals_preserve_offered_rate_and_slo_inputs)'` |
| S8 (planned) | `mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E 'test(=resilience::gateway_capacity_paces_streams_isolates_tenants_and_recovers)'` |
| S9 (planned) | `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=gateway::tests::benchmark_modes_preserve_profiles_and_roundtrip_evidence)'` |
| R1 (existing) | `mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E 'test(=compatible::openai_compatible_streams_terminate_and_survive_a_capture_outage)'` |
| R2 (existing) | `mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E 'test(=resilience::a_cancelled_call_is_still_settled_and_the_server_keeps_serving)'` |
| R4 (existing) | `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=client_queue::step::tests::slope_reads_the_trailing_window)'` |
| R3 (existing) | `mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=capacity::tests::quantile_reads_bucket_deltas)'` |

For any additional named test, confirm and record its exact package/target/selector
and run it through mise, including managed environment setup. Discover new names
with `mise exec -- cargo nextest list --locked -p wyrd-testing --bin capacity`
and corresponding `--lib`/`--test gateway` listings. Diagnose failures only after
rerunning with WYRD_LOG/RUST_LOG and reading traces, per AGENTS.md.

Affected-owner proof after the scenarios:

- `WYRD_TEST_PACKAGES='wyrd-testing wyrd-gateway wyrd-server' mise run test:wyrd`
  uses the existing family task's package selection and managed lifecycle.
- If Skald transport timing changes: `WYRD_TEST_PACKAGES=skald-providers mise run test:skald`.
- `mise run fmt`, `mise run lints`, `mise run check:deps`, and
  `mise run check:tenant-isolation` for the Rust/dependency/tenant boundaries.
- `git diff --check`, inspect the complete diff and new files, and map acceptance
  criteria to evidence. No Python/TS/codegen lane is required without changes to
  those contracts. A discovery requiring such a change triggers the stop below.

Release evidence (new commands become available during implementation):

```sh
mise run bench:gateway:overhead -- --case 1 --clients 16 --no-profile
mise run bench:gateway:overhead -- --no-profile
mise run bench:gateway:overhead
mise run bench:gateway:capacity -- --max-answer-ms 200 --max-first-token-ms 350 --max-chunk-lag-ms 50 --max-stream-ms 1100 --no-profile
mise run bench:gateway:capacity -- --max-answer-ms 200 --max-first-token-ms 350 --max-chunk-lag-ms 50 --max-stream-ms 1100
mise run bench:verification:capacity -- --case 1 --clients 16
```

The first command times the edit/rebuild/measure loop; the next two exercise all
ten overhead steps and profiling parity. The capacity commands exercise all
three profiles, the default rate sweep and any eligible high/low recovery
sequences, with and without profile generation. The numerical SLO flags above
are explicit example run inputs for the deterministic upstream, not production
promises or a universal gateway target. Retain actual failed rates and driver
limitations; do not adjust SLOs silently to manufacture a passing envelope.
The final command proves verification filter/measurement closure.

Preserve each run's artifacts before the next run replaces that mode's directory.
Collect generated profile files and runtime/CPU/off-CPU evidence for both default
modes; a no-profile run alone does not complete AC-006. If the profiler cannot
collect a required evidence class, record the exact gap and continue numeric and
functional proof without calling the profiling obligation complete.
Functional tests exercise query measurement/output regressions; the existing
`bench:bifrost:query-capacity` heavy data sweep is not a required task-level
iteration lane. Add focused query regression assertions for extraction seams
not covered by the shared measurement tests.

Record the measured server feature union and inspect shared boot/capture paths;
no direct gateway test-support branch was found during planning, but that alone
does not prove feature parity. Split build invocations only if needed to obtain
an unaffected measured server. Record the binary/build/host identity with results.

Extraction-only moves, TOML task wiring, no-gate membership, build recipe and
feature-parity review use characterization/static/regression proof rather than
manufactured RED. The executable additions above require TDD. The input's runtime
estimates are hypotheses until the quick command is timed.

Full language/journey suites and repository aggregate verification belong to the
integrated change review, not this bounded task. The benchmark remains outside
those gates. An overhead latency FAIL or a valid outside-SLO capacity row is
expected evidence; a failed functional check, missing data or invalid measurement is not acceptable completion proof.

## Material Stop Conditions

- Revision 2 approval is withdrawn or the requested scope changes: do not
  implement dependent behavior under an outdated revision.
- Meeting the timing definition requires a public/persisted contract, new
  dependency/feature, security/tenancy change or ownership redesign beyond the
  transient telemetry boundary. Return for specification authority.
- Accurate final stream handoff or terminal loss cannot be established without
  changing runtime semantics: do not silently substitute enqueue time or assume
  zero loss; report the evidence gap and required decision.
- Someone requires a green result by relaxing the applicable overhead or capacity
  SLO/sample/capture gates,
  reserving gateway capture slots, caching snapshots or reordering accounting:
  these require separate product/spec authority, not an implementation shortcut.
- Common extraction cannot preserve query/verification semantics inside their
  existing owners: surface the concrete conflict instead of creating another
  measurement framework or silently dropping consumer evidence.

Normal compiler/test failures, local fixture setup, reversible helper choices,
and corrected focused commands are implementation work, not material decisions.

## Planning Verification

This revision changes specification/task Markdown only. TDD is not applicable
to the packet edit; its executable implementation scenarios above still require
TDD. Static planning proof passed: all 23 requirement/invariant/acceptance IDs
map to the task, all nine scenarios contain Behavior/RED/GREEN/REFACTOR blocks,
revision/command/profile-artifact checks and local authority links are consistent,
and `git diff --check` passes. No implementation tests, release benchmarks or
profiles have been executed/generated by this planning update.

## Authority Links

- [Specification revision 2 (approved)](../spec.md)
- [AGENTS.md](../../../../AGENTS.md)
- [Agent rules](../../../../AGENTS.md)
- [Design](../../../../architecture/wyrd-design.md)
- [Doctrine](../../../../architecture/wyrd-design.md)
- [Spec-driven development](../../../../changes/spec-driven-development.md)
- [Implementation execution](../../../../changes/implementation-execution.md)
- [Testing workflows](../../../../TESTING.md)

## Implementation Evidence

Status: **BLOCKED** on capacity release evidence. Overhead is complete. The
capacity code, unit tests, and journeys pass. The capacity release runs were
stopped on 2026-10-20 at the owner's direction, because the approved REQ-009/010
windows make one capacity run about 25–30 min and the task asks for two. A spec
decision on shorter capacity windows is pending (see Findings).

Commits: `c37b6784d` (overhead workload, verification `--clients`), `c0fc29cd2`
(capacity mode, `bench:gateway:capacity`), `059d4846e` (S6, shared capture read
helper), `b1f83f26b` (S8), `3000600fa` and `429eb234e` (review-record
whitespace, bounded-label allow-list).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-001 matrix, flags, release journey, no-profile | `bin/capacity/gateway/{mod,workload,run,report}.rs`; `bench:gateway:overhead` | S2, S4; both overhead release runs below (10 steps each) | PASS |
| AC-002 arrival timing, stream points, buckets; R1/R2 | server telemetry (`c52b4aa9f`) | S2, S3, R1, R2 | PASS |
| AC-003 gates, post-answer drain, loss-free shutdown | `report.rs` verdicts; `run.rs` `Integrity`/`settle` | S5; S6 (warmup excluded, drain-gated staged = calls, lost 0, pending 0, rows readable); release drain and shutdown rows PASS | PASS (see limit 1) |
| AC-004 honest FAIL, phase-ranked recommendations, quick-loop time | report Pareto and notes | release runs: every overhead row FAILs on p99 only, all integrity PASS; ranking under Findings | PASS |
| AC-005 shared owner, consumers unchanged | `wyrd_testing::capacity` | S1, R3, R4; `test:wyrd` 954/954 | PASS |
| AC-006 build/feature parity, opt-in tasks, artifacts, profiles in both modes | `mise.toml` tasks (in no gate); `Profiler` | S9; profiled overhead wrote `profiles/00..09` with CPU and off-CPU Paretos; capacity profiled run not executed | PARTIAL |
| AC-007 fixed offered rate, real pacing, SLO validation, tenant ownership | `capacity.rs` `open_loop`, `PacedUpstream` | S7, S8 | PASS (unit/journey); release not run |
| AC-008 capacity/SLO vs integrity, envelope, overload/recovery rows | `envelope.rs` | S5 capacity cases, S8 limit refusal then recovery | PASS (unit/journey); release not run |
| AC-009 client roundtrip samples and attribution in both modes | `samples.jsonl`, client columns | S9; both overhead runs report client percentiles and client-added means | PASS |

Owner lanes (all PASS): `mise run fmt`, `mise run lints`, `mise run check:deps`,
`mise run check:tenant-isolation`, `git diff --check`,
`WYRD_TEST_PACKAGES='wyrd-testing wyrd-gateway wyrd-server' mise run test:wyrd`
(954 passed), `WYRD_TEST_PACKAGES=wyrd-skald-providers mise run test:skald`
(48 passed), the exact S1–S9 and R1–R4 selectors above, and every
`resilience::` and `native::` journey after the helper move.

Feature parity: the combined build unifies `wyrd-server/test-support`. Its cfg
sites are Bifrost test controls (None in the binary), shutdown probes, the MCP
probe, and query/peer counters. None of them is on a gateway or Skald path, so
the combined build is kept.

### Release runs (AMD Ryzen 9 9950X, 32 CPU / 91 GiB, Linux; same binary for both)

| Run | Exit | Wall time | Result |
|---|---|---|---|
| `bench:gateway:overhead -- --case 1 --clients 16 --no-profile` (earlier) | 1 | 416 s, mostly the release rebuild | 1 FAIL (p99), integrity PASS |
| `bench:gateway:overhead -- --no-profile` | 1 | about 4.5 min after a 3 m 09 s rebuild | 10 FAIL (p99 only); 10 drain PASS; shutdown PASS |
| `bench:gateway:overhead` (profiled) | 1 | 322 s, no rebuild | 10 FAIL (p99 only); 10 drain PASS; shutdown PASS; 10 profiles |
| `bench:gateway:capacity` (both modes) | — | not run | stopped at owner direction |
| `bench:verification:capacity -- --case 1 --clients 16` | — | not run | stopped with the capacity runs |

Overhead, profiled (no-profile values within about 5%). Every step: 0 errors,
captures staged = arrivals, dropped 0, lost 0.

| Step | Throughput | Mean overhead | Overhead p99 bucket |
|---|---|---|---|
| chat 1 / 4 / 16 / 64 | 236 / 700 / 696 / 644 per s | 4.0 / 5.2 / 22.7 / 98.3 ms | 7.5 / 7.5 / 50 / 250 ms |
| chat-stream 1 / 4 / 16 / 64 | about 300 / 675 / 655 / 658 per s | 3.0 / 5.6 / 24.1 / 97.1 ms | 5 / 9 / 50 / 250 ms |
| chat-capture 16 / 64 | 633 / 613 per s | 25.0 / 104.0 ms | 50 / 250 ms |

### Findings

1. **Admission is serialized per tenant; this is the overhead.** Throughput
   is flat at about 700/s from 4 clients up, while server CPU stays at about
   1.1 cores on a 32-CPU host. Overhead grows linearly with clients (about
   1.4 ms per client). At 16 clients the `admit` phase is 19.6 ms of the
   22.7 ms overhead, and it is almost all `queue_delay` (20.7 ms). Cause:
   admission takes the tenant's admission advisory lock (`ledger.rs:3-6`)
   and holds it for about 1.4 ms per call, so one tenant tops out near 700
   admissions/s. Next is `account` (1.9 ms), then `snapshot` (1.1 ms).
   Recommendation, ranked: (a) do not take the lock when the snapshot has no
   limits or budgets; otherwise shorten the critical section. (b) Batch or
   defer buffered accounting. (c) Cache the snapshot. All three are out of
   scope (reservation/admission/accounting changes) and need their own spec.
2. **Each drain takes 12–15 s.** Scribe outbox writes average about 2.2 s
   each across about 11 large slices per step, and WAL fsync accounts for
   about 1.9 ms per call. About 2 of the overhead run's minutes are drains. A
   faster outbox drain shortens every benchmark.
3. **Capacity run time was set by the spec; revision 3 shortens it.** Each rate has a 10 s warmup and
   a 60 s window, across 5 rates and 3 cases, plus 30 s of overload and 90 s
   of recovery per case. One run is about 25–30 min. Proposed revision: 2 s
   warmup, 15 s window, 10 s overload, and 20 s of recovery judged on its
   last 15 s, for about 6–8 min per full run. The 100-sample minimum would
   need to scale with the rate for the 10% tenants at 50/s. Needs owner
   approval.
4. **Process feedback:** nine scenarios for two benchmark modes is heavier
   than the behavior needs. Two scenarios (overhead, capacity), each with a
   list of must-fail cases, would cover the same ground.

### Limits

1. S6 does not drive a terminal capture loss end to end. Server-internal
   Scribe writes send `expected_schema_fingerprint: None`, so the
   fingerprint seam does not reach gateway captures, and no other existing
   seam forces a terminal refusal. The S5 verdict unit cases cover loss
   failing a row, from the same exported counter.
2. Diagnosis, `gateway_observations_are_bounded_correlated_and_secret_free`.
   Symptom: the test reported "unbounded label phase". Cause: the timing
   labels from `c52b4aa9f` (`phase`, `stream`, `point`) are closed sets
   (`telemetry.rs` `Phase::name`, bool, three literals) but were missing
   from the test's allow-list. Fix site: the allow-list only, confirmed by an
   independent read-only diagnostician; no other gateway label allow-list
   exists.

Non-goals held: no new dependency or feature, no migration, no SDK API, no
CI gate, no admission/accounting/snapshot change, no OS-specific mechanism.
