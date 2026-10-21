---
id: SPEC-gateway-capacity
revision: 3
status: approved
---

# AI gateway overhead and sustained-capacity benchmarks

The user approved the two-mode design on 2026-10-20 and requested this revision
and its companion task. Revision 3 (user-approved 2026-10-20) shortens the
capacity windows for quick turnaround: 2 s warmup, 15 s windows, 3 s backlog
edges, 10 s overload, 20 s recovery judged on its last 15 s, and a 30-call
per-tenant/mode minimum. One shared harness serves two opt-in commands with
separate measurements and verdicts. The user additionally required overhead to
follow verification's client → server → client roundtrip measurement pattern.
Implementation and benchmark execution are not part of this planning revision.

## Intent and scope

Measure the real gateway's Wyrd-induced overhead and capacity, prevent capture
loss from masquerading as speed, and rank subsequent improvements from measured
phases. Ship production telemetry and two opt-in release-server benchmarks,
including shared measurement extraction and verification client-filter parity.
Overhead establishes added latency under controlled conditions; sustained capacity
establishes the highest tested arrival rate meeting a declared SLO for the stated
host, deployment and workload. Neither result is a general production-capacity
claim. Share provisioning, fixtures, telemetry, resources, profiling and artifacts;
do not build a second harness.

Input: `/home/thorrester/.claude/jobs/a6d5e555/tmp/gateway-capacity-plan.md`, read
fully, including section (g) and the quick-turnaround addendum. Source verification
on 2026-10-19 used the current working tree, including existing uncommitted work.
Cited line numbers are navigation aids, not immutable authority.

## Requirements

### REQ-001 — Shared real-server journey and overhead upstream

Use `wyrd_testing::release_server::LocalServer` to run the release `wyrd-server`
on the host through its migrate/serve/setup/readiness journey, with managed
Postgres. Overhead provisions one tenant; capacity provisions the tenant set in
REQ-009. Provision an `openai_compatible` deployment per tenant through
`PUT /v1/admin/gateway/provider-deployments/{name}`, using `<mock>/v1`,
`auth: "none"`, and capability `chat_completions`. No sealing key or built-in
endpoint override is needed. Retain Development endpoint screening; do not
bypass SSRF policy.

Drive `/v1/chat/completions` with raw reqwest HTTP/1.1 keep-alive. Overhead
uses one distinct `wyrd_sk_` key per concurrent client; capacity uses a bounded,
fixed warmed-key pool per tenant, recorded in the run identity. Warm each key
before the measured window; do not bypass normal cache expiry or re-exchange.
For overhead, wiremock injects no delay and disables request recording to bound
memory. Share the completion and native SSE fixtures through the existing `gateway_capture`
library owner, updating journey consumers. The OpenAI stream has 32 data chunks,
valid usage/termination and `[DONE]`, served as one mock body in overhead only.
Capacity uses the paced upstream in REQ-009; a preassembled SSE body cannot stand in for a paced stream.

### REQ-002 — Overhead matrix and iteration controls

| Case | Payload and response | Clients | Capture |
|---|---|---|---|
| 1 `chat` | Buffered, approximately 1 KiB request and 1 KiB JSON answer with usage | 1/4/16/64 | Explicit metadata |
| 2 `chat-stream` | Same approximately 1 KiB chat request with `stream: true`, 32 data chunks then `[DONE]` | 1/4/16/64 | Explicit metadata |
| 3 `chat-capture` | Approximately 64 KiB messages and 64 KiB buffered answer | 16/64 | Request and response payload |

Every overhead step uses 2 s warmup and a 10 s closed-loop traffic window. The
overhead default is all ten steps, profiled at 999 Hz. Support `--case 1,2,3`, `--clients 1,4,16,64`, and
`--no-profile`. Select only the intersection with each case's client matrix;
invalid/missing values or an empty effective selection must fail clearly.
Add the same `--clients` filter to verification-capacity, preserving its defaults,
workloads and verdicts. No duration environment knobs or skip-build option.

`--no-profile` skips only profiler start/finish: runtime Pareto, percentiles,
resources, counters and verdicts remain. It requires no perf configuration.
Default profiling reuses `profile::Capture`; report incomplete CPU/off-CPU
evidence honestly. Optional profiling must not become a production prerequisite
or a reason to introduce platform-gated application behavior.

### REQ-003 — Monotonic server overhead

Start at `RequestArrival`, before authentication and body decode. Use monotonic
server time, never client/mock clocks or Utc attempt duration. Measure actual
HTTP exchange boundaries; credential lookup, preparation, decode and translation
remain Wyrd work. Record the total server duration at each boundary as well as
its upstream interval and overhead. Timing carried between owners is transient, not persisted
accounting data or a public wire field.

| Mode and point | Overhead |
|---|---|
| Buffered `answer` | Arrival to answer handoff to response body, minus the sum of HTTP-send-to-fully-read-body intervals across attempts |
| Streaming `answer` | Arrival to stream-head handoff, minus upstream send-to-headers time, including completed prior attempts when present |
| Streaming `end` | Arrival to final frame handoff to the response-body consumer, minus upstream-open-to-last-byte elapsed time, including completed prior attempts when present |

A relay-channel enqueue is not final body handoff; neither server handoff proves
peer receipt. Upstream lifetime and relay work overlap. The streaming end metric
is the specified elapsed-time residual, not an additive measure of every chunk's
processing cost; the relay phase supplies that evidence. Shared-host mock CPU
can affect scheduling and throughput even when upstream time is subtracted.
Do not claim that a zero-delay mock makes the verdict independent of host load.

### REQ-004 — Production metrics and diagnostics

| New family | Bounded labels |
|---|---|
| `wyrd_gateway_duration_seconds` | `operation`, `stream=false|true`, `point=answer|end`, `outcome` |
| `wyrd_gateway_overhead_seconds` | `operation`, `stream=false|true`, `point=answer|end`, `outcome` |
| `wyrd_gateway_upstream_seconds` | `operation`, `stream`, `point=headers|end` |
| `wyrd_gateway_phase_duration_seconds` | `operation`, `phase` |

Phases: `authenticate`, `parse`, `snapshot`, `authorize`, `route`, `admit`,
`credential`, `translate`, `account`, `relay`, `capture`. Parse includes body
decode and typed validation. Capture remains after the answer. Cover terminal
errors without altering outcomes, leaking secrets or double-counting samples.

Register these and the existing gateway request, attempt, time-to-first-byte and
queue-delay duration families in `app/metrics.rs` with the verification duration
buckets, in seconds:

`0.0001, 0.00025, 0.0005, 0.00075, 0.0009, 0.001, 0.0025, 0.005, 0.0075,
0.009, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10, 30, 60, 300`.

Read window deltas from `/metrics`. Report p50/p95/p99 bucket upper bounds for
buffered overhead, streaming answer and end overhead separately, and buffered
upstream duration. The total-duration family uses the same call populations and
handoff points as overhead. Upstream diagnostic samples used for attribution
must identify whether they represent an attempt or the aggregate for one call;
do not subtract incompatible populations. Never combine stream points to judge
p99. Runtime breakdown uses these family/label pairs:

- `wyrd_gateway_duration_seconds` / `stream`
- `wyrd_gateway_overhead_seconds` / `stream`
- `wyrd_gateway_phase_duration_seconds` / `phase`
- `wyrd_gateway_upstream_seconds` / `stream`
- `wyrd_gateway_attempt_duration_seconds` / `outcome`
- `wyrd_gateway_time_to_first_byte_seconds` / `operation`
- `wyrd_postgres_pool_acquire_seconds` / `pool`
- `outbox_write_seconds` / `outbox`
- `wyrd_http_request_duration_seconds` / `path`

Report deltas for `outbox_events_lost_total`, `wyrd_gateway_refusals_total`,
`wyrd_gateway_capture_total` by outcome, and `wyrd_gateway_requests_total`.
Rank phases by total time, with observation counts and time per arrival. Nested
or overlapping families cannot be added into total overhead. Profiling also
reports CPU Pareto by crate, hotspots and off-CPU folded stacks.

### REQ-005 — Overhead verdict and common integrity gates

Every overhead workload row passes only when:

1. Each applicable overhead point has at least 1,000 samples and a p99 bucket
   upper bound strictly below 1 ms. Exactly 1 ms fails. Missing/non-finite
   quantiles or required telemetry fail. Mark `TARGET` at bounds <=500 us,
   without overriding other failures.
2. There are zero refusals, non-2xx responses, transport/body/SSE errors and
   wrong/truncated answers. HTTP 200 alone does not prove stream success.
3. The `outbox_events_lost_total` delta is zero across all outboxes, including
   post-response capture and drain attributable to the step.
4. `wyrd_gateway_capture_total{outcome="staged"}` delta equals measured arrivals
   and every dropped-capture outcome delta is zero. Summing all outcomes is
   insufficient. All three overhead cases explicitly enable capture.
5. Peak server memory plus 1 GiB is strictly below the server memory limit,
   matching verification's headroom rule.

The sample/p99 and zero-refusal gates above belong to overhead. Capacity uses
REQ-010 for latency, capture eligibility and expected overload refusals. Both
modes retain the common evidence validity, zero-loss, memory, drain and shutdown
requirements below.

Settle warmup work before the baseline. Stop new arrivals at the traffic-window
boundary, complete in-flight responses, then allow captures and Scribe to drain
before the final correctness scrape and next step. Bound drain at 60 s, reusing
verification's convention; timeout fails. Separate traffic-window and drain
intervals in evidence so throughput/CPU figures cannot conceal drain cost.
Counter reset, missing or invalid evidence cannot become a zero-loss PASS.

Give drain and shutdown their own PASS/FAIL rows, also conditioned on loss-free
capture completion. Shutdown must exit cleanly within LocalServer's 45 s grace.
Retain the terminal server log and fail on shutdown loss or inability to prove
loss-free completion; do not claim a post-exit scrape. Any integrity, drain,
shutdown or harness/evidence failure causes nonzero exit. Every overhead FAIL also causes nonzero exit;
capacity rate exploration follows REQ-010. Overhead client roundtrip percentiles
are mandatory measurements under REQ-011, reported without a sub-ms client gate.

### REQ-006 — Expected initial FAIL and ranked follow-up

Ship metrics and both benchmark modes first, retaining the fixed overhead
threshold even if the initial benchmark fails. Preserve actual failed case/client/point evidence and
rank subsequent fixes from the measured phase breakdown. Tenant snapshot caching
with admin-write invalidation and answering buffered calls before accounting are
hypotheses, not authorized production changes. Reservation/refusal semantics are
a separate product decision. Do not relax thresholds, suppress losses or change
accounting to manufacture green benchmark results. Functional checks must pass;
a correctly reported unmet performance target is a legitimate benchmark result.

### REQ-007 — Shared measurement owner

Move shared `Window`, `Histogram` and breakdown behavior into
`wyrd_testing::capacity` in this change. Gateway and verification consume it;
query consumes common window/resource behavior while retaining its specialized
scan/write/profile measurements. Preserve consumer measurement and output
semantics. Keep workload-specific `Report`, `Row`, thresholds and workload logic
local. Do not introduce a third window implementation, replacement launcher,
profiler, transport, metrics parser or resource sampler.

### REQ-008 — Mise invocation and artifacts

Add `gateway-overhead` and `gateway-capacity` to the existing capacity binary,
with `bench:gateway:overhead` and `bench:gateway:capacity` mise commands. These
select two modes of the same gateway harness, not independent implementations.
Mirror the verification task's locked incremental release build of both binaries into `target/profiling`, frame pointers, line-table
debug information, argument forwarding and managed Postgres wrapper, changing the
workload subcommand. Document mode-specific flags (REQ-002/009) and membership
in no gate/verify lane.

Both commands must follow the existing verification/Bifrost benchmark standards:
production duration/phase/counter instrumentation exposed by the real server;
release builds with frame pointers and line-table debug information; per-step
`profile::Capture` at 999 Hz; generated profile files in the step directory
(`perf.data`, `report.txt`, `metadata.json`, `offcpu.data`, `offcpu.folded`, and
recorder diagnostics under the existing replica directory convention);
runtime component Pareto, CPU Pareto by crate, hotspots and off-CPU folded stacks;
raw client samples, histogram/window deltas, host/build identity, resources,
report verdict reasons and retained server logs. Profile every sustained rate
and the overload/recovery phases separately. Reuse the existing profiler,
resource/metrics owners and reporting conventions. Do not introduce another
benchmark framework or copy their infrastructure.

Default runs generate profiles as part of benchmarking. `--no-profile` is only
an iteration option: all numeric measurements and verdict rules remain present,
but it does not satisfy the required profiled-run evidence. Profiling may change
the measured values; record whether it was enabled. Report unavailable/incomplete
profile classes honestly; do not call an incomplete profile set complete or add
an OS-specific production mechanism to obtain it. Always rebuild; do not split
the testing library dependency.

Check unified features: `wyrd-testing` enables `wyrd-server/test-support`. Keep
the existing combined build only with evidence that test behavior does not affect
this gateway workload; otherwise build the measured server separately without
that feature. No new dependency or Cargo feature is authorized.

Each command replaces only its own output at run start:
`target/gateway-overhead/` or `target/gateway-capacity/`. Running one must preserve
the other's evidence. Write `report.md`,
`samples.jsonl`, `server.log`, and `profiles/NN-<step>/` for profiled steps. Preserve
partial evidence on errors. Record host fingerprint, binary/build identity,
selections, traffic/drain intervals, throughput, raw client samples, telemetry
deltas, phase ranking, CPU/memory, profiler completeness and verdict reasons.
Time an actual quick iteration before citing host-specific runtime; the input's
30–45 s / 1–1.5 min / 4–6 min estimates were not measured.

### REQ-009 — Sustained open-loop capacity workloads

Use the same release-server, HTTP/1.1 keep-alive, warmed-key and capture journey.
A bounded local upstream built with installed HTTP/body facilities must actually
delay response headers and pace body writes. Extend the existing fixture owner;
wiremock's whole-response delay cannot emulate incremental SSE delivery. This is
fixture support, not a new production transport. Disable unbounded request/body
recording. Retain the shared 32-data-chunk SSE content and termination validation.

The deterministic default upstream profiles are benchmark inputs, not provider
performance promises: buffered answers wait 100 ms before headers/body; streams
wait 250 ms before headers and the first data chunk, emit 32 data chunks at 20 ms intervals,
then usage/termination and `[DONE]`. Record actual upstream send times and driver
receive times using monotonic timestamps in the benchmark process. TCP writes
are not receipt; parse complete SSE events across arbitrary network chunks.

| Capacity case | Tenant/workload profile | Capture |
|---|---|---|
| 1 `buffered` | One tenant, approximately 1 KiB request/answer, delayed buffered upstream | Metadata |
| 2 `paced-stream` | One tenant, approximately 1 KiB request, paced 32-data-chunk stream | Metadata |
| 3 `tenant-contention` | Four tenants at 70/10/10/10 percent of aggregate arrivals, each alternating buffered and paced streaming requests | Metadata |

Run fixed offered rates independently of response completion. Default aggregate
rates are 50/100/250/500/1000 requests/s, in ascending order for each selected
case. Each rate has a 2 s warmup, settled before the baseline, then a 15 s
arrival window. Retain per-tenant/mode evidence; do not hide the quieter tenants
in aggregate percentiles or promise a new production fairness policy.

Support `--case 1,2,3`, `--rates <positive comma-separated requests/s>` and
`--no-profile`; validate missing/duplicate/invalid values and empty selections.
Capacity does not accept `--clients`: concurrency is observed under open-loop
arrivals, not substituted for rate. Require positive finite caller-declared
`--max-answer-ms`, `--max-first-token-ms`, `--max-chunk-lag-ms`, and
`--max-stream-ms` for every selected applicable response mode. These are workload
SLO inputs, printed in every report, not a new universal gateway latency target.
No duration environment knobs or skip-build option.

Measure client latency from scheduled arrival as well as actual send. Record
scheduled, sent, server-arrived, refused, completed, timed-out and driver-missed
counts, send lateness and peak in-flight calls. Keep driver work bounded and
report its explicit bound. Never delay arrivals to await a free slot, retry a
refusal, replay overdue arrivals in a burst or grow an unbounded task queue.
Any driver-missed arrival, cap hit or p99 send lateness above 5 ms invalidates
that rate as capacity evidence; distinguish driver limitation from server
saturation. Report upstream/driver CPU and scheduling evidence alongside server
resources; the local upstream shares host resources.

### REQ-010 — Capacity envelope, overload and recovery

Report each tested rate as sustainable, outside-SLO, or invalid evidence, with
separate integrity and recovery verdicts. Do not apply overhead's sub-ms residual
gate to paced streams. Server overhead remains diagnostic in this mode.

A sustainable rate requires:

- Every scheduled arrival sent in its window, no refusals, response errors,
  timeouts or malformed/truncated responses, and valid driver evidence.
- Client p99 buffered completion, first complete stream data event, pooled
  nonnegative data-event lag relative to each matched upstream send, and complete
  stream receipt within the declared applicable bounds (equality passes).
  Completion/first-token clocks start at scheduled arrival. Each applicable
  tenant/mode distribution requires at least 30 completed calls; chunk lag
  additionally reports all matched data-event samples. Insufficient evidence
  cannot become sustainable. Apply these checks per tenant/mode as well as
  aggregate, so low-volume tenant failures remain visible.
- REQ-005 memory headroom and zero losses/dropped captures, including post-answer
  work. Reconcile captures against calls eligible under the unchanged production
  lifecycle, not against every offered request: a pre-admission refusal need not
  produce a successful-call capture. Show offered/refused/admitted/completed/
  capture-eligible/staged counts separately and validate expected tenant capture
  ownership through tenant-authorized reads. A staged count alone is not proof
  of durable capture completion.
- Bounded outstanding accepted work and capture/Scribe backlog: compare the
  first and last 3 s of the measured window. A positive growth above one second
  of the offered workload (or its measured capture bytes for a byte queue)
  fails sustainability. Report samples and slopes rather than using drain to
  hide sustained accumulation. Missing queue/completion evidence is invalid.
  Finish in-flight work, drain within 60 s and shut down cleanly within 45 s.

Report the highest passing tested rate per profile and the first failing rate;
do not interpolate an untested maximum. If the highest rate passes, report a
lower bound with the ceiling unlocated. With no passing rate, report no measured
sustainable rate. A valid outside-SLO exploration row does not itself fail the
command; invalid evidence or integrity failure always does, as does finding no
sustainable rate for a selected profile.

For each selected case with a passing rate, add a bounded overload/recovery
sequence without changing production admission semantics: 10 s at twice its
highest passing rate, then 20 s back at that passing rate. Explicit configured
admission refusals may pass overload safety; timeouts, unexpected errors,
malformed/truncated successful responses, loss, dropped captures, exceeded memory
headroom or invalid driver evidence fail. Honor existing request deadlines.
Record whether refusal/saturation was actually observed. If none was observed,
say overload safety was not demonstrated; do not manufacture a failure by
changing the production policy or claim the capacity ceiling was found.

After the high-rate interval, keep lower-rate arrivals running. Recovery requires
the final 15 s to satisfy the declared per-tenant/mode SLOs with zero refusals,
restored bounded backlog, and subsequent common drain/shutdown integrity. Final
recovery distributions need 100 calls each; insufficient samples fail that proof.
Keep overload, recovery, drain and shutdown as separate verdict rows. End the run
nonzero on a failed safety/recovery row, preserving all evidence.

### REQ-011 — Client → server → client overhead measurement

Follow verification's `Bench::run` and `Report::direct` pattern: raw monotonic
client samples, client p50/p95/p99 and mean, server-total/upstream/overhead
measurements, throughput, resources and client/transport-added mean. The
benchmark must drive the real HTTP roundtrip; server histograms alone do not
satisfy this requirement.

For buffered calls, start before sending the request and stop after the complete
answer is received and validated. For streaming, retain separate client samples
for response headers, first complete data event, and fully validated stream
termination/body EOF. A stream-head handoff on the server is not client first
token or full roundtrip completion. Include body consumption and validation in
client completion; keep errors and partial streams visible rather than counting
HTTP 200 as success.

Report unadjusted roundtrip latency; do not subtract mock delay from it. Keep
server-derived Wyrd overhead alongside it, using REQ-003 boundaries. As in
verification, the sub-ms gate judges server overhead, while client latency is
reported. Client-added mean may subtract only matched mean server-total durations
at comparable completion/header points and equal sample populations; never
subtract percentiles or infer first-token time from header time. Report an
unavailable attribution explicitly. Persist raw client samples and both sets of
percentiles in no-profile and profiled modes.

## Invariants and non-goals

- **INV-001:** Preserve authn/authz, tenant isolation, non-blocking audit staging,
  secrecy, SSRF, admission locking, accounting order/error semantics, capture
  durability/loss semantics, deadlines, cancellation and SSE termination. No new
  durable state or metric labels carrying caller-controlled identifiers.
- **INV-002:** No SDK inference surface, public HTTP/MCP operation, Card kind,
  schema/migration, new protocol case, dependency/feature, performance CI gate,
  snapshot cache, accounting reordering or gateway reservation/refusal change.
  Other provider wires, deliberate cold-cache churn, live-provider benchmarking,
  distributed gateway scaling and production fairness/admission changes are
  excluded. Multi-tenant contention measurements are included in REQ-009/010.
- **INV-003:** Reuse existing owners; follow Rust struct-centered ownership and
  rustdoc rules; preserve macOS/Linux and x86_64/arm64 support. No new OS-specific
  production mechanisms, no expansion of legacy host-specific resource plumbing,
  and no synthetic host load. Numbers-only mode requires no perf setup.

## Acceptance criteria

| ID | Outcome and proof | Owner |
|---|---|---|
| AC-001 | Exact overhead matrix, controls, shared fixtures and real release-server OpenAI journey; argument/fixture regression and release evidence | Capacity/testing (REQ-001/002/008) |
| AC-002 | Arrival-based timing, precise upstream subtraction, phases and bucket exposition; deterministic timing tests and served-metrics journey including terminal errors | Server/gateway/Skald (REQ-003/004, INV-001) |
| AC-003 | Strict overhead latency/sample and common memory/correctness/capture/loss gates; boundary tests, lost/dropped capture regression, bounded drain/shutdown evidence | Gateway report (REQ-005) |
| AC-004 | Initial performance FAIL honestly retained, follow-up fixes ranked by measured phases, quick-loop runtime measured | Benchmark report/evidence (REQ-006/008) |
| AC-005 | Shared extraction reaches gateway, verification and query with no measurement/output regression | `wyrd_testing::capacity` (REQ-007, INV-003) |
| AC-006 | Both opt-in commands follow verification/Bifrost instrumentation, per-step profiling and generated-profile/report standards; artifact retention, feature parity and functional checks; task inspection, release run and targeted verification | Mise/capacity (REQ-008, INV-001/002/003) |
| AC-007 | Fixed-rate delayed/paced and multi-tenant capacity profiles; scheduler/matrix tests and real-server paced-body/tenant-ownership journey plus release runs | Shared gateway harness (REQ-009, INV-001/003) |
| AC-008 | Highest tested sustainable rate, honest outside-SLO/invalid rows, per-tenant SLOs, bounded backlog, overload and recovery; verdict boundary tests and real-server load/recovery evidence | Gateway capacity report (REQ-010) |
| AC-009 | Full client roundtrip raw samples/percentiles and comparable client-added mean, separate from server overhead; delayed-header/body journey and report regression | Gateway client measurement/report (REQ-011) |

## Source verification and corrections

- `http/middleware/metrics.rs::RequestArrival`/`track_metrics` supplies arrival.
  `router.rs` also attaches trace context, omitted from the input list.
  `config.rs` confirms 1 MiB / 30 s / 1,024 defaults; `AppState` is Arc-backed.
- `GatewayIngress::authenticate` and `GatewayApiKeys::authenticate` confirm the
  4,096-entry/60 s token cache and per-request verification. `routes.rs::typed_body`,
  `GatewayInvocation::govern`, `admit_call`, `decide`, `attribute`, `admit`,
  `account`, and `CallTask::settle_buffered`/`settle_stream` confirm the wait path.
  `GatewayAdministration::snapshot` commits around `gateway_snapshot`; gateway SQL's
  `LOCK_ADMISSION_SQL` takes the per-tenant advisory lock. Three transactions is
  source evidence, not measured proof of the expected p99 bottleneck.
- `CallPlan`/`retain_representable` and adapter preparation confirm body copies.
  `CredentialResolver::resolve` may perform blocking env/file or external-secret
  work, but this benchmark's `auth: none` returns before those operations.
- Correct relay path is `crates/wyrd/wyrd-gateway/src/adapter/stream.rs`:
  `relay`/`Relay` uses `FRAME_BUFFER = 16`. `HttpProviderDispatch` delegates
  HTTP to Skald `clients/openai.rs`, `raw.rs`, and `clients/mod.rs`. Timing the
  whole adapter call would subtract serialization/decode work too; a narrow
  transient transport-timing seam may therefore reach those Skald owners.
- `components/gateway/capture.rs::record` counts staged and dropped outcomes.
  `GatewayInvocation::capture` records staged even though `Outbox::stage` can
  drop. Both staged-count and loss-counter checks are essential. Scribe capacity
  is 65,536; `RESERVED_SHARE = (3,4)` leaves a quarter for unreserved staging.
- `service.rs::capture_from` and `GatewayCaptureMode` default to **disabled**.
  This specification requires metadata capture for overhead cases 1/2 and all
  capacity cases. Overhead case 3 administers
  `/v1/admin/gateway/capture-policy` with
  `{"mode":"payload","payload_fields":["request","response"]}`.
- `app/metrics.rs::install_recorder` omits gateway bucket registration. Existing
  gateway families, spans, pool/outbox/HTTP diagnostics and Utc attempt timing
  match the input. The four proposed metric families remain implementation work.
- `resilience.rs::deploy_compatible`, `GatewayCapture::completion`, harness SSE
  constants and `compatible.rs::CHAT_EVENTS` are real reuse sites. OpenAI SSE
  already exists; extend shared fixtures instead of adding a competing owner.
  `wyrd-client::Gateway` administers the gateway and has no inference operation.
- `capacity.rs` owns `Percentiles`, `deltas`, `quantile`, `ResourceWindow`,
  `driver_cpu_seconds`, `release_binary` and `install_tracing`. Verification has
  `Window`, `Histogram`, `breakdown`. Query has specialized `Window` and profile
  deltas, **no Histogram/breakdown copy**. Extract actual common behavior;
  do not force unused histogram machinery into query.
- `LocalServer`, `Metrics`, `MemoryPeak`, `Placement::Host`, `SERVER_URL`, and
  `profile::Capture` exist. Query/verification constants are 2 s / 10 s / 999 Hz.
  Verification has `--case`, no `--clients`. Its report confirms 1,000 samples,
  strict <1 ms and 1 GiB headroom. Capacity has no gateway dispatch arm yet.
- `mise.toml` has `bench:verification:capacity` and
  `bench:bifrost:query-capacity`, not `bench:query:capacity`; both gateway commands are
  absent. `with-test-postgres.sh` owns isolated Compose. Release uses four
  codegen units; fat LTO belongs to `dist`. Workspace wiremock is 0.6, currently
  locked to 0.6.5. Server test-support is enabled by the testing dependency;
  no direct gateway hot-path conditional was found, but shared boot/capture
  effects still require implementation-time parity evidence.

- Revision 2 source check: verification `Bench::run`/`Measured::percentiles` and
  `Report::direct`/`client_extra` measure raw client roundtrip and server-total
  attribution while `direct_miss` gates server overhead. Client-queue
  `step.rs::Bench::run`/`Record` supplies fixed-rate evidence conventions, but
  does not provide concurrent gateway stream driving. `LocalServer::start`
  already provisions a tenant slice. `profile::Capture::{start,finish}` writes
  per-step CPU/off-CPU files and records incomplete evidence; gateway must reuse
  it. Installed axum, HTTP-body and Tokio facilities support paced test responses
  without a dependency/feature addition. These are source facts, not run results.

## Approval and history

**Approved scope:** on 2026-10-20 the user agreed to the proposed overhead and
sustained-capacity split sharing one harness and requested spec/task updates.
The follow-ups explicitly require verification-style client-server roundtrip
measurement in overhead and existing verification/Bifrost instrumentation,
profiling and generated-profile standards in both benchmark modes. Revision 2
records that authority. Capacity latency bounds are caller-declared run inputs; this packet fixes no universal capacity
number or production latency promise. No material decisions remain open.
Performance optimization and reservation/refusal policy remain outside scope.

- 2026-10-19 — Revision 1 drafted from the input and source inspection; no
  implementation or benchmark execution.
- 2026-10-20 — Revision 2 records the approved two-mode design, explicit client
  roundtrip measurement, independent verdicts, paced multi-tenant open-loop
  capacity, overload/recovery evidence and shared benchmark/profile standards.
  Spec/task documentation only.
- 2026-10-20 — User explicitly approved the complete revision 2 specification
  and companion task after reviewing the updated packet. TASK-001 is ready;
  implementation has not started.

## Authorities

[AGENTS.md](../../../AGENTS.md),
[agent rules](../../../architecture/agent-rules.md),
[Wyrd design](../../../architecture/wyrd-design.md),
[doctrine](../../../architecture/wyrd-doctrine.mdx),
[spec-driven development](../../../architecture/references/languages/spec-driven-development.md).
No Bifrost ingest/storage contract change is authorized.
