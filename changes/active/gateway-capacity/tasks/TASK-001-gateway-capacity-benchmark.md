---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-gateway-capacity
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, INV-001, INV-002, INV-003, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006]
depends_on: []
---

# AI gateway capacity benchmark

Spec revision 1 is **draft, awaiting human approval**. No approved revision
exists yet. This task is proposed at the user's explicit request and must not
begin implementation until that revision is approved. Execute with
[$wyrd-implement](../../../../.agents/skills/wyrd-implement/SKILL.md); the caller
owns scheduling and Git actions. There are no other task dependencies.

## Outcome and Value

An operator can run the real release gateway against a zero-delay local OpenAI
upstream, see trustworthy overhead and phase/resource evidence, and receive a
nonzero exit for any failed latency, correctness, capture-loss, drain or shutdown
row. One task closes the complete workflow: production metrics, common measurement
extraction, fixtures, workload, verdicts, command, evidence and regressions.
It maps every requirement, invariant and acceptance criterion in revision 1.

All three input section (g) recommendations are mandatory in this task:

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
| Prometheus exposition | `app/metrics.rs::install_recorder`, `VERIFICATION_DURATION_BUCKETS`; verification telemetry | Existing recorder bucket tests, `Metrics::buckets`, verification runner | Gateway families export summaries; three new families absent | Register gateway duration families with existing fine buckets | No new recorder/parser/dependency |
| Capture and zero-loss correctness | `GatewayInvocation::capture`, `components/gateway/capture.rs::record`, `service.rs::capture_from`, `Outbox::stage`, ScribeOutbox | Compatible capture-outage journey; verification drain/report; `GatewayCapture::capture_call` | No capacity loss gate; capture counter includes drops, source default disabled | Explicit metadata/payload policies; count staged only, reject drops/loss, settle step boundaries and drain | No new staging path, reservation/refusal or durability mechanism |
| Shared measurement | `capacity.rs::{Percentiles,deltas,quantile,ResourceWindow}`; verification `Window`, `Histogram`, `breakdown`; query specialized `Window`/`ProfileDelta` | Verification `Bench::run`/report; query `Bench` and query/write/profile consumers; existing capacity delta unit tests | Workload-local windows; verification-local histogram/breakdown | Move common measurement behavior to `wyrd_testing::capacity`, update all three workloads | User explicitly requires extraction now; query has no Histogram/breakdown copy to move |
| Verdict, artifacts, profiling and iteration | Verification `Report`, `direct_miss`, `correctness_miss`, `parse_cases`; `profile::Capture`; capacity main; mise verification task | Query report/window/profile path; registered capacity bin and gateway integration target | No gateway verdict/report/dispatch/task, verification has no client filter | Workload-local gateway report with fixed gates, flags and opt-in mise entry; retain shared profiler | Report/Row remain separate because workloads judge different outcomes |

Prohibited: INV-001/002 changes, new dependency/feature, migrations, SDK APIs,
provider-wire expansion, CI performance gating, synthetic host load, unrelated
cleanup, new OS-specific mechanisms or broad resource-launcher rewrites. Preserve
all existing working-tree changes. Functional checks must pass even when measured
capacity fails. Do not treat an expected performance FAIL as permission to ignore
a functional failure.

## Approach

1. Revalidate source facts, feature unification and approved revision; establish
   consumer characterization before moving common measurement code.
2. Extract shared capacity measurement behavior and close query/verification
   consumers without changing their workload-specific report semantics.
3. Add precise production telemetry through existing arrival, gateway execution,
   provider HTTP and response-body owners, plus production histogram buckets.
4. Consolidate fixtures, provision the real-server deployment/policies/client
   keys and implement the selected closed-loop matrix with optional profiling.
5. Implement strict workload/drain/shutdown verdicts and isolated capture/loss
   accounting; preserve artifacts on successful and failed runs.
6. Add dispatch and mise entry, verification client-filter parity, then collect
   actual benchmark evidence and phase-ranked follow-up recommendations.
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
Actual exchange time is subtracted across attempts; auth/parse/credential/decode
work is retained. Truncation, cancellation and refusal retain existing outcomes.
REQ-003/004, AC-002, INV-001.

**RED.** Add planned S2 real-server HTTP/served-metrics journey and S3 recorder
exposition test. Expected failures are absent metric series/buckets, late arrival
coverage or incorrect timing attribution. Add deterministic timing assertions
through the existing timing/provider owners to distinguish delayed upstream from
Wyrd preparation/decode and prior-attempt intervals; avoid wall-clock p99 assertions
in ordinary tests. Stream validation must cover head and final body handoff.

**GREEN.** Instrument existing owners and carry only transient monotonic durations.
Reach Skald's actual send/read boundary when needed; timing an entire adapter
future is insufficient. Register all seven duration families using the existing
fine buckets. Rerun S1–S3 and exact stream/cancellation regressions R1/R2 below.

**REFACTOR.** Consolidate timing with the existing execution lifecycle. Keep
bounded labels, cancellation cleanup and ownership clear; no utility owner or
parallel provider transport. Do not change persisted attempt/accounting shapes.

### Scenario 3 — Selected workloads run without unbounded mock recording

**Behavior.** Cases/client selections produce the specified ten default steps;
unsupported/missing/empty selections fail, case 3 remains limited to 16/64, and
verification gains the same client filter. Correct 32-chunk SSE is fully consumed.
Metadata capture is explicitly enabled for cases 1/2 and request/response payload
capture for case 3. No-profile needs no perf yet retains all numeric evidence.
REQ-001/002/008, AC-001/006.

**RED.** Add planned S4 command/matrix test and exercise the selected release
journey after dispatch is available. Expected failures: wrong selections, absent
client filter, missing fixture/policy, or profiler invoked in no-profile mode.
Characterize moved SSE fixtures before changing journey imports.

**GREEN.** Reuse LocalServer/setup, public gateway administration, per-client keys,
shared fixtures and no-recording wiremock. Keep fixed 2/10 s windows and 999 Hz.
Add gateway dispatch/mise invocation and filter parity. Rerun S1–S4 and fixture
consumer regressions; inspect actual selected-step output.

**REFACTOR.** Reuse existing hand-parsed flag conventions and workload ownership;
no generic CLI framework, alternate server bootstrap or skip-build mechanism.

### Scenario 4 — Loss or weak evidence can never earn PASS

**Behavior.** Strict p99/sample/memory gates, separate stream-point gates, response
correctness, staged capture equality and all-outbox zero loss apply to every
workload row. Exactly 1 ms, 999 samples, missing/non-finite metrics, counter reset,
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
profile completeness and the distinction between functional PASS and capacity FAIL.

## Acceptance Criteria

- AC-001: The real release-server journey and exact matrix/flags/fixtures are
  demonstrated, including both stream points and no-profile operation (S2/S4,
  release commands). Verification filter parity preserves its default steps.
- AC-002: S2/S3 and deterministic timing proof establish arrival, transport,
  terminal coverage and bucket exposition; R1/R2 preserve failure behavior.
- AC-003: S5/S6 and release evidence enforce all gates, post-answer drain and
  loss-free shutdown, including terminal errors that arrive with HTTP 200.
- AC-004: Actual benchmark results and phase-ranked recommendations are retained;
  expected overhead FAIL is neither hidden nor mistaken for a broken harness.
- AC-005: S1, existing capacity tests and query/verification consumer proof show
  the common owner is used without changing their measurement/report semantics.
- AC-006: Build/feature parity, opt-in task placement, artifact preservation and
  functional checks pass; INV-001/002/003 remain intact.

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
| R1 (existing) | `mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E 'test(=compatible::openai_compatible_streams_terminate_and_survive_a_capture_outage)'` |
| R2 (existing) | `mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E 'test(=resilience::a_cancelled_call_is_still_settled_and_the_server_keeps_serving)'` |
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

Release evidence (new gateway task becomes available during implementation):

```sh
mise run bench:gateway:capacity -- --case 1 --clients 16 --no-profile
mise run bench:gateway:capacity -- --no-profile
mise run bench:gateway:capacity
mise run bench:verification:capacity -- --case 1 --clients 16
```

The first command is the timed edit/rebuild/measure loop, the next two exercise
all ten gateway steps and profiling parity, and the last proves verification
filter/measurement closure. Preserve each run's artifacts before the next run
replaces its output directory. Functional tests exercise query measurement/output
regressions; the existing `bench:bifrost:query-capacity` heavy data sweep is not a
required task-level iteration lane. Add focused query regression assertions for
any extraction seam not covered by the shared measurement tests.

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
those gates. A benchmark latency FAIL is expected evidence; a failed functional
check, missing data or invalid measurement is not acceptable completion proof.

## Material Stop Conditions

- Revision 1 is not approved, or approval excludes the proposed metadata policy
  or streaming residual semantics: do not implement dependent behavior.
- Meeting the timing definition requires a public/persisted contract, new
  dependency/feature, security/tenancy change or ownership redesign beyond the
  transient telemetry boundary. Return for specification authority.
- Accurate final stream handoff or terminal loss cannot be established without
  changing runtime semantics: do not silently substitute enqueue time or assume
  zero loss; report the evidence gap and required decision.
- Someone requires a green capacity result by relaxing p99/sample/capture gates,
  reserving gateway capture slots, caching snapshots or reordering accounting:
  these require separate product/spec authority, not an implementation shortcut.
- Common extraction cannot preserve query/verification semantics inside their
  existing owners: surface the concrete conflict instead of creating another
  measurement framework or silently dropping consumer evidence.

Normal compiler/test failures, local fixture setup, reversible helper choices,
and corrected focused commands are implementation work, not material decisions.

## Authority Links

- [Specification revision 1 (draft)](../spec.md)
- [AGENTS.md](../../../../AGENTS.md)
- [Agent rules](../../../../architecture/agent-rules.md)
- [Design](../../../../architecture/wyrd-design.md)
- [Doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)
