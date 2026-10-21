---
id: SPEC-gateway-capacity
revision: 1
status: draft
---

# AI gateway capacity benchmark

Awaiting human approval. The single companion task is proposed, not ready for
implementation. The user explicitly requested both draft artifacts together;
this precedes the skill's normal approved-spec handoff.

## Intent and scope

Measure the real gateway's Wyrd-induced overhead and capacity, prevent capture
loss from masquerading as speed, and rank subsequent improvements from measured
phases. Ship production telemetry and an opt-in release-server benchmark,
including shared measurement extraction and verification client-filter parity.

Input: `/home/thorrester/.claude/jobs/a6d5e555/tmp/gateway-capacity-plan.md`, read
fully, including section (g) and the quick-turnaround addendum. Source verification
on 2026-10-19 used the current working tree, including existing uncommitted work.
Cited line numbers are navigation aids, not immutable authority.

## Requirements

### REQ-001 — Real-server workload

Use `wyrd_testing::release_server::LocalServer` to run the release `wyrd-server`
on the host through its migrate/serve/setup/readiness journey, with managed
Postgres. Provision one tenant and an `openai_compatible` deployment through
`PUT /v1/admin/gateway/provider-deployments/{name}`, using `<mock>/v1`,
`auth: "none"`, and capability `chat_completions`. No sealing key or built-in
endpoint override is needed. Retain Development endpoint screening; do not
bypass SSRF policy.

Drive `/v1/chat/completions` with raw reqwest HTTP/1.1 keep-alive and one distinct
`wyrd_sk_` key per concurrent client. Warm each key before the measured window.
wiremock injects no delay and disables request recording to bound memory. Share
the completion and native SSE fixtures through the existing `gateway_capture`
library owner, updating journey consumers. The OpenAI stream has 32 data chunks,
valid usage/termination and `[DONE]`, served as one mock body.

### REQ-002 — Matrix and iteration controls

| Case | Payload and response | Clients | Capture |
|---|---|---|---|
| 1 `chat` | Buffered, approximately 1 KiB request and 1 KiB JSON answer with usage | 1/4/16/64 | Explicit metadata |
| 2 `chat-stream` | Same approximately 1 KiB chat request with `stream: true`, 32 data chunks then `[DONE]` | 1/4/16/64 | Explicit metadata |
| 3 `chat-capture` | Approximately 64 KiB messages and 64 KiB buffered answer | 16/64 | Request and response payload |

Every step uses 2 s warmup and a 10 s closed-loop traffic window. Default is all
ten steps, profiled at 999 Hz. Support `--case 1,2,3`, `--clients 1,4,16,64`, and
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
remain Wyrd work. Timing carried between owners is transient, not persisted
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
upstream duration. Never combine stream points to judge p99. Runtime breakdown
uses these family/label pairs:

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

### REQ-005 — Honest PASS/FAIL and zero loss on every row

Every workload row passes only when:

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
   insufficient. All three cases explicitly enable capture.
5. Peak server memory plus 1 GiB is strictly below the server memory limit,
   matching verification's headroom rule.

Settle warmup work before the baseline. Stop new arrivals at the traffic-window
boundary, complete in-flight responses, then allow captures and Scribe to drain
before the final correctness scrape and next step. Bound drain at 60 s, reusing
verification's convention; timeout fails. Separate traffic-window and drain
intervals in evidence so throughput/CPU figures cannot conceal drain cost.
Counter reset, missing or invalid evidence cannot become a zero-loss PASS.

Give drain and shutdown their own PASS/FAIL rows, also conditioned on loss-free
capture completion. Shutdown must exit cleanly within LocalServer's 45 s grace.
Retain the terminal server log and fail on shutdown loss or inability to prove
loss-free completion; do not claim a post-exit scrape. Any FAIL or harness/evidence
error causes nonzero exit. Client p50/p95/p99 is reported, not latency-gated.

### REQ-006 — Expected initial FAIL and ranked follow-up

Ship metrics and the benchmark first, retaining the fixed threshold even if the
initial benchmark fails. Preserve actual failed case/client/point evidence and
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

Add `gateway-capacity` to the existing capacity binary and
`bench:gateway:capacity` to mise. Mirror the verification task's locked incremental
release build of both binaries into `target/profiling`, frame pointers, line-table
debug information, argument forwarding and managed Postgres wrapper, changing the
workload subcommand. Document all three flags and membership in no gate/verify
lane. Always rebuild; do not split the testing library dependency.

Check unified features: `wyrd-testing` enables `wyrd-server/test-support`. Keep
the existing combined build only with evidence that test behavior does not affect
this gateway workload; otherwise build the measured server separately without
that feature. No new dependency or Cargo feature is authorized.

Replace only `target/gateway-capacity/` at run start. Write `report.md`,
`samples.jsonl`, `server.log`, and `profiles/NN-<step>/` for profiled steps. Preserve
partial evidence on errors. Record host fingerprint, binary/build identity,
selections, traffic/drain intervals, throughput, raw client samples, telemetry
deltas, phase ranking, CPU/memory, profiler completeness and verdict reasons.
Time an actual quick iteration before citing host-specific runtime; the input's
30–45 s / 1–1.5 min / 4–6 min estimates were not measured.

## Invariants and non-goals

- **INV-001:** Preserve authn/authz, tenant isolation, non-blocking audit staging,
  secrecy, SSRF, admission locking, accounting order/error semantics, capture
  durability/loss semantics, deadlines, cancellation and SSE termination. No new
  durable state or metric labels carrying caller-controlled identifiers.
- **INV-002:** No SDK inference surface, public HTTP/MCP operation, Card kind,
  schema/migration, new protocol case, dependency/feature, performance CI gate,
  snapshot cache, accounting reordering or gateway reservation/refusal change.
  Other wires, cache misses and multi-tenant capacity are excluded.
- **INV-003:** Reuse existing owners; follow Rust struct-centered ownership and
  rustdoc rules; preserve macOS/Linux and x86_64/arm64 support. No new OS-specific
  production mechanisms, no expansion of legacy host-specific resource plumbing,
  and no synthetic host load. Numbers-only mode requires no perf setup.

## Acceptance criteria

| ID | Outcome and proof | Owner |
|---|---|---|
| AC-001 | Exact matrix, controls, shared fixtures and real release-server OpenAI journey; argument/fixture regression and release evidence | Capacity/testing (REQ-001/002/008) |
| AC-002 | Arrival-based timing, precise upstream subtraction, phases and bucket exposition; deterministic timing tests and served-metrics journey including terminal errors | Server/gateway/Skald (REQ-003/004, INV-001) |
| AC-003 | Strict latency/sample/memory and every-row correctness/capture/loss gates; boundary tests, lost/dropped capture regression, bounded drain/shutdown evidence | Gateway report (REQ-005) |
| AC-004 | Initial performance FAIL honestly retained, follow-up fixes ranked by measured phases, quick-loop runtime measured | Benchmark report/evidence (REQ-006/008) |
| AC-005 | Shared extraction reaches gateway, verification and query with no measurement/output regression | `wyrd_testing::capacity` (REQ-007, INV-003) |
| AC-006 | Correct opt-in build, artifact retention, feature parity and passing functional checks; task inspection, release run and targeted verification | Mise/capacity (REQ-008, INV-001/002/003) |

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
  This draft proposes explicit metadata capture for cases 1/2 to satisfy the
  input's section (g) all-row capture accounting. Case 3 administers
  `/v1/admin/gateway/capture-policy` with
  `{"mode":"payload","payload_fields":["request","response"]}`.
- `app/metrics.rs::install_recorder` omits gateway bucket registration. Existing
  gateway families, spans, pool/outbox/HTTP diagnostics and Utc attempt timing
  match the input. The three proposed metric families are absent today.
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
  `bench:bifrost:query-capacity`, not `bench:query:capacity`; gateway capacity is
  absent. `with-test-postgres.sh` owns isolated Compose. Release uses four
  codegen units; fat LTO belongs to `dist`. Workspace wiremock is 0.6, currently
  locked to 0.6.5. Server test-support is enabled by the testing dependency;
  no direct gateway hot-path conditional was found, but shared boot/capture
  effects still require implementation-time parity evidence.

## Approval and history

**Open approval:** approve revision 1, including explicit metadata capture for
cases 1/2, the qualified streaming residual definition, and acceptance of an
honest initial performance FAIL with passing functional checks. Until approval,
the spec remains draft and TASK-001 remains proposed. Performance optimization
and reservation/refusal policy require separate approval outside this packet.

- 2026-10-19 — Revision 1 drafted from the full input and current source. No
  implementation, benchmark execution, staging or commit performed.

## Authorities

[AGENTS.md](../../../AGENTS.md),
[agent rules](../../../architecture/agent-rules.md),
[Wyrd design](../../../architecture/wyrd-design.md),
[doctrine](../../../architecture/wyrd-doctrine.mdx),
[spec-driven development](../../../architecture/references/languages/spec-driven-development.md).
No Bifrost ingest/storage contract change is authorized.
