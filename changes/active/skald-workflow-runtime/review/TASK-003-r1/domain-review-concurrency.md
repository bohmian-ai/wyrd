# Concurrency, cancellation, and deadline domain review

**Immutable subject:** base `58d07d7260df1f022a721e720a28ea48e5096e35`, candidate `a1792c45323489157e818eb29722913114014927`.

**Overall result: FAIL.** One material request-isolation/replay defect remains. The candidate otherwise keeps fallback, model, deadline, cancellation, correlation, selected secret resolution, and ingress headers request-scoped without shared mutable per-call state.

## Reviewed boundary and authority

I reviewed the approved Revision 12 specification and original TASK-003, treating the human-approved `WyrdGatewayCall.model` amendment as authoritative. Applicable obligations were the shared `Workflows` lifecycle contract, REQ-036A/038/039/043/046/058, INV-004/011/012/020, AC-011A/013/019, the task's prohibition on mutable adapter context and competing transport, and the repository async/cancellation and struct-ownership rules in `AGENTS.md`, `architecture/agent-rules.md`, and the applicable Rust/maintainer guidance.

The source trace covered:

- `PublicWyrdGatewayCaller::call` and `NativeCall::project`, through `HttpTransport::post_native`, `AuthMiddleware`, and the public gateway ingresses;
- `StepRoute::attempt_registry` and `WyrdGatewayProvider::send`, including the earliest deadline calculation and run cancellation token;
- `Workflows::{create,get,cancel,wait}`, the one-second polling loop, and future-drop behavior;
- `SelectedRoutes::{of,dependencies}` and blocking secret-file resolution;
- authenticated OpenAI, Anthropic, and Gemini request-local fallback parsing and non-forwarding;
- gateway provider-refusal relay, cancellation/drop ownership, and the focused transport and ingress tests.

## Boundary assessment

| Boundary | Evidence | Result |
|---|---|---|
| Immutable per-attempt route state | `skald-workflow/src/route.rs:360-426,543-590` constructs a new provider adapter per attempt with cloned model/fallback/correlation and its own deadline/token. `wyrd-client/src/workflow/gateway.rs:75-121` derives local headers/body and races the whole authenticated request future against cancellation and remaining time. | PASS |
| Deadline and cancellation ownership | `skald-workflow/src/workflow.rs:430-518` gives cancellation and total deadline biased precedence over attempt completion and aborts/drains run-owned tasks. The public caller drops its request future on cancellation/timeout. | PASS |
| Remote wait/drop behavior | `wyrd-client/src/workflow/remote.rs:89-103` performs one GET at a time, sleeps one second only after a nonterminal snapshot, returns all terminal statuses, and has no cancellation or submission side effect when dropped. | PASS |
| Selected route preparation | `wyrd-client/src/workflow/local.rs:38-102` selects bindings before resolution and reads only selected secret refs. Each blocking file read is awaited without shared mutation; preparation completes before execution dispatch. | PASS |
| Ingress header isolation | `wyrd-server/src/components/gateway/ingress.rs:101-139,198-224` authenticates first and parses fallback from the current `HeaderMap`; handlers put only the typed override into `GatewayCallRequest`. Provider dispatch receives no caller header map. | PASS |
| Native-call authentication replay | `wyrd-client/src/transport/http.rs:350-401` replays every HTTP 401 before reading its body, even though the gateway intentionally relays provider 401 refusals. | **FAIL — CONC-001** |

## Proposed finding

### CONC-001 — provider-originated 401 responses replay a non-replay-safe model call

- **Classification:** INCORRECT / REGRESSION
- **Violated obligation:** TASK-003 requires native model calls not to retry except for the safe edge-authentication refresh path, while preserving provider refusal semantics and request-local behavior. The transport's own contract says a model call is not replay-safe. INV-020 requires each call's lifecycle to remain its own immutable operation.
- **Exact location:** `crates/shared/wyrd-client/src/transport/http.rs:350-401`, especially the status-only branch at lines 393-397. The refresh error is also discarded at line 396.
- **Evidence and reachability:** `post_native` treats every `401 Unauthorized` as proof that the Wyrd bearer was refused, refreshes, and sends the cloned body again before it reads or classifies the first response. That premise is false for this ingress: `crates/wyrd/wyrd-server/src/components/gateway/invocation.rs:205-222` explicitly returns the last provider refusal's status and body, and the existing PG proof at `pg_invocation_tests.rs:4152-4159,4350-4358` demonstrates an upstream provider 401 is returned to the caller as 401. Thus an ordinary provider authentication refusal reaches the new client branch and produces a second governed invocation/provider dispatch. If token refresh itself fails, the ignored error still allows another loop iteration rather than returning the authentication failure.
- **Observable consequence:** one Workflow model-call attempt can create two server authorization/admission/audit/accounting attempts and two provider requests while Skald records only one call. This is not retry isolation: it adds an unapproved transport retry below Workflow retry accounting and can consume limits twice. The repeated request retains the same client request ID, which correlates the duplicate but does not make the provider operation idempotent.
- **Required testable correction:** make the single reactive replay conditional on an authenticated Wyrd-edge rejection, not status `401` alone, and propagate refresh failure without dispatching again. Reuse the existing native error envelopes/stable Wyrd auth codes and shared `AuthMiddleware` refresh behavior; do not add a header, setting, retry option, compatibility path, or second transport. Add focused proof that (1) a known Wyrd authentication 401 refreshes once and replays once with the same request identity, (2) an uncoded/provider-native 401 returns as one redacted `RemoteProblem` after exactly one server request, and (3) a failed refresh returns without a second model dispatch.

## Recovery and cleanup assessment

Cancellation and timeout drop the in-flight `reqwest` future. The server gateway remains the owner of accepted invocation settlement and documents that a dropped invocation cancels the attempt while tracked accounting settles (`invocation.rs:205-227`); Workflow terminalization does not retain that work. No lock, global registry, mutable header map, or per-call adapter field is shared across concurrent calls. `SelectedRoutes` preparation may leave an already-started blocking secret file read running after outer future cancellation, which is normal Tokio `spawn_blocking` behavior and performs no durable or network side effect; this is not a material finding.

## Verification evidence and limits

- Independently ran `mise exec -- cargo nextest run --locked -p wyrd-client --test workflow_transport -E 'test(=public_gateway_call_context_and_errors)'`: **PASS**, 1 test run, 1 skipped by the exact selector.
- The passing test proves concurrent fallback/model isolation, supported-dialect projection, pre-cancel refusal, local timeout, native error normalization, and uncoded 400/408/429/503 handling.
- It does not exercise an uncoded/provider-originated 401, a refresh failure, or mid-flight cancellation. The first two omissions leave CONC-001 reachable; the source-level cancellation composition is otherwise direct and covered by the runtime's biased cancellation ownership.
- Candidate remained `a1792c45323489157e818eb29722913114014927` at report completion.

## Standing DRIFT direction

No bespoke concurrency check, setting, retry knob, allowlist, lifecycle file, or compatibility option is recommended. The correction should use the already established typed native error envelopes, stable Wyrd error codes, shared auth middleware, and ordinary focused transport tests used by comparable Rust HTTP clients.
