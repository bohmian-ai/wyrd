# Concurrency, async, cancellation, and lifecycle domain review

**Immutable subject:** base `58d07d7260df1f022a721e720a28ea48e5096e35`, candidate `b954976648b49429f1c0950c4fa3509573885be8`.

**Overall result: PASS.** The cumulative candidate satisfies the concurrency,
async, cancellation, and lifecycle obligations assigned to TASK-003 and its
R1-R5 remediations. Per-call model, fallback, deadline, cancellation, and
correlation remain immutable; the native request body is sent once; a known
`401` starts credential renewal before body collection can stall it without
replaying the model call; run-start filesystem work stays on the blocking pool;
and one configuration snapshot supplies both selected local route families.
Remote polling owns no background task, Python uses the repository runtime
bridge, and caller cancellation composes with the gateway's existing tracked
settlement owner. I found no material defect and no qualifying DRIFT.

## Domain boundary and authority

I reviewed the complete base-to-candidate diff and candidate source against:

- `AGENTS.md`, especially the client ownership, struct-centered Rust, narrow
  async boundary, PyO3 runtime, test taxonomy, and completion rules;
- `architecture/agent-rules.md`, the spec-driven-development and maintainer
  references, and `architecture/references/architecture/patterns.md`;
- the Workflow and coordination-clock sections of
  `architecture/wyrd-design.md` and the Workflow doctrine in
  `architecture/wyrd-doctrine.mdx`;
- approved `changes/active/skald-workflow-runtime/spec.md` Revision 12,
  particularly REQ-031, REQ-036A, REQ-038, REQ-043, REQ-046-048, REQ-058,
  INV-016, INV-020-021, and AC-011A, AC-019-020; and
- original TASK-003 plus remediation tasks R1-R5 and their recorded evidence.

The human-approved `WyrdGatewayCall.model: ModelRef` amendment, send-once
native-`401` correction, and run-start `spawn_blocking` boundary were treated
as authority. The review remained strictly source-only: I ran no build, test,
Cargo, mise, package-manager, formatter, linter, codegen, or verification
command.

The source trace covered `Workflow::run_with` and `SelectedRoutes` through
secret resolution and `WorkflowExecutionDependencies`; Skald route selection,
attempt deadlines, cancellation tokens, task abort/drain, and provider
adapters; `PublicWyrdGatewayCaller`, `HttpTransport::post_native`, and
`AuthMiddleware`; authenticated public ingress and the gateway's existing
tracked call task; `Workflows::{create,get,cancel,wait}`; and the Python
load/run wrappers over the shared runtime.

## Boundary and source assessment

| Boundary | Source and recorded evidence | Result |
|---|---|---|
| Immutable per-attempt call context | `crates/skald/skald-workflow/src/workflow.rs:430-568` fixes the attempt, Agent, and run deadlines and constructs one attempt context containing the run cancellation token and correlation. `route.rs:395-426,543-590` creates an attempt-private adapter and a fresh `WyrdGatewayCall`; model, fallback, deadline, cancellation, and correlation never enter shared registry or header state. `wyrd-client/src/workflow/gateway.rs:77-123` builds the body and fallback header vector as call locals. The recorded focused fixture drives concurrent calls through one caller and distinguishes their models and fallback values. | PASS |
| Deadline and cancellation ownership | Skald gives cancellation and the total deadline biased precedence and aborts/drains its owned tasks (`workflow.rs:430-518`). The public caller races the complete authenticated request future against the call's remaining timeout and cancellation (`workflow/gateway.rs:96-116`). Dropping that future closes caller-side IO; the already-established server owner keeps admitted gateway work in its tracked `CallTask` for bounded settlement (`wyrd-server/src/components/gateway/invocation.rs:205-227,331-378,918-970`). The recorded raw transport case observes dispatch, then cancellation, a prompt timeout-category return, and no resend (`workflow_transport.rs:775-803`). | PASS |
| Native body ownership, send-once, and renewal | `HttpTransport::post_native` consumes one `Bytes` body into one request and calls `send` exactly once (`transport/http.rs:376-410`). Once response headers establish `401`, it awaits the existing `AuthMiddleware::force_refresh` before body collection; renewal failure wins, successful renewal affects later calls only, and no request reconstruction exists. `AuthMiddleware` retains the established cache/single-flight owner (`auth.rs:436-619`). Recorded complete, renewal-failure, truncated-body, and held-open-body cases each observe one model POST, including renewal while the original body remains pending (`workflow_transport.rs:922-1106`). | PASS |
| Run-start configuration and selected secrets | `Workflow::run_with` discovers routes synchronously, then places ambient configuration/client assembly on Tokio's blocking pool (`workflow/mod.rs:142-160`). `load_local_setup` reads at most one `GlobalConfig` and derives both external bindings and any client-less Wyrd client from it (`:190-238`). Retained loading clients bypass ambient client assembly. `SelectedRoutes::dependencies` resolves only selected binding names, and every synchronous secret read uses `spawn_blocking` (`workflow/local.rs:39-110,113-149`). A dropped future may leave a bounded read finishing, but it performs no dispatch or durable mutation and its result is discarded. R5's recorded exact selector exercises a real client-less mixed-route `Workflow::run_with` path rather than a helper-only path. | PASS |
| Remote facade and future drop | `Workflows::create` reuses shared idempotent submission; `get` and idempotent `cancel` use the existing authenticated transport; `wait` owns no spawned poller and performs one foreground GET followed by one one-second sleep (`workflow/remote.rs:41-116`). Dropping `wait` therefore drops only the current GET/sleep and cannot issue cancellation or resubmission. Create and cancel document the possible server-side progress after a caller drops their response future. Recorded source fixtures cover first/replay responses, stable retry key, every terminal value, errors, and dropped-wait traffic (`workflow_transport.rs:124-304`). | PASS |
| Python runtime and retained context | Python stores the complete shared `ClientWorkflow`, retains it through registered and authored-external loading, and calls `self.inner.run` while detached from the GIL through `wyrd_runtime::runtime().block_on` (`sdks/wyrd-sdk-python/src/workflow.rs:204-215,480-517,537-579`; `state/mod.rs:2600-2638`). This is the existing multi-thread runtime bridge, not an ad hoc runtime or second engine. The recorded Python journey distinguishes the loading client from hostile/absent ambient configuration. | PASS |
| Public ingress and settlement isolation | Authentication middleware runs before handlers inspect fallback (`gateway/ingress.rs:101-169`). `requested_fallback` reads only the current request's header map and returns a typed value (`:198-224`); handlers lower that value into the current `GatewayCallRequest`. No caller header map reaches provider dispatch. Existing `GatewayInvocation` owns admitted call tracking, caller-drop cancellation, accounting, capture, and shutdown drain; TASK-003 adds no competing tracker or settlement lifecycle. | PASS |
| Process-local Workflow-run state | No server Workflow-run registry, transition owner, eviction policy, restart recovery, or cross-replica lookup is added in this task's diff. TASK-003 explicitly ships the shared client before those routes exist and assigns real Workflow lifecycle implementation/proof to TASK-004/005. The client facade neither simulates nor persists that future server state. | PASS (excluded by this task boundary) |

## Prior concurrency finding closure

| Stable finding | Closure in the cumulative candidate | Result |
|---|---|---|
| `FIND-TASK-003-1` | Python retains and runs the complete shared Workflow owner, preserving its loading client. | CLOSED |
| `FIND-TASK-003-2` | The native body is sent once; renewal begins immediately after `401` headers, precedes body collection, propagates renewal failure, and never replays the call. | CLOSED |
| `FIND-TASK-003-8` | Durable create/cancel and native POST operations document caller-drop and possible post-dispatch progress. | CLOSED |
| `FIND-TASK-003-9` | The existing focused transport selector directly cancels an already-dispatched held request and observes no resend. | CLOSED |
| `FIND-TASK-003-10` | One blocking run-start owner reads one ambient `GlobalConfig`; R5's focused proof now traverses `Workflow::run_with`, route discovery, blocking setup, selected secret resolution, and both mixed-route consumers. | CLOSED |
| `FIND-TASK-003-11` | `force_refresh` documentation separates replay-safe transport retry from native send-once renewal. | CLOSED |
| `FIND-TASK-003-12` | `Workflow::into_skald` documents loss of retained client and automatic dependency composition. | CLOSED |
| `FIND-TASK-003-14` | Run-start owners now document ambient IO triggers, caller drop, already-started blocking reads, and partial progress accurately. | CLOSED |

## Findings

No findings.

The implementation reuses established Tokio cancellation, `spawn_blocking`,
shared auth/HTTP transport, shared Workflow facade, and gateway task tracking.
It introduces no cache, watcher, generation token, retry knob, polling option,
custom cancellation worker, lifecycle file, checker, allowlist, or parallel
runtime. Under the standing human direction, there is no unsupported mechanism
to classify as DRIFT and no novel mechanism should be required in remediation.

## Evidence limits

I did not execute the recorded commands. The candidate contains the named
fixtures, and the remediation packets record exact focused results for mixed
route setup, pending/truncated `401` renewal, post-dispatch cancellation,
remote polling, shared-client coverage, and the public Python retained-client
journey. Source matches those claims and does not expose an untested contrary
path in this domain. Server Workflow-run transitions, retained-run eviction,
shutdown/restart loss, and deployment affinity remain later-task obligations,
not missing TASK-003 proof.

Candidate identity remained `b954976648b49429f1c0950c4fa3509573885be8` at
report completion.
