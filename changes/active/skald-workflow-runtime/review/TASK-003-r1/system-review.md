# System Resilience Review

## Review Findings

### Critical

None.

### Important

- **SYS-001 — INCORRECT** — [`crates/shared/wyrd-client/src/transport/http.rs:394`](../../../../../crates/shared/wyrd-client/src/transport/http.rs) retries every native-protocol HTTP `401` after refreshing the Wyrd bearer, before reading or classifying the response body. That status is not proof that Wyrd authentication rejected the request: the authenticated gateway deliberately returns a completed provider refusal with the provider's status (`crates/wyrd/wyrd-server/src/components/gateway/invocation.rs:553-559`), and the public ingress relays that status unchanged (`crates/wyrd/wyrd-server/src/components/gateway/routes.rs:1339-1345`). An upstream provider `401` therefore means the first non-replay-safe model call was already dispatched, yet `post_native` sends the whole call a second time. This violates the task's single public gateway-call boundary and its requirement to normalize uncoded `401`/`403` provider refusals through the existing provider status category; it can duplicate provider work, accounting, capture, and audit, and the second response masks the fact that a replay occurred. Restrict bearer refresh/replay to a response that is demonstrably a Wyrd edge-authentication refusal; an uncoded/provider `401` must be returned after one request and normalized as `SKALD_PROVIDERS_401_AUTH`. Add focused proof for both paths: a relayed uncoded provider `401` produces exactly one request, while a Wyrd-coded authentication refusal may perform the one safe credential refresh/replay expected by the shared authentication policy. This correction needs no new retry framework, setting, marker header, or other mechanism not already present in the native error-envelope and auth owners.

### Suggestions

None.

## Open Questions

None.

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `a1792c45323489157e818eb29722913114014927`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12, including the human-approved 2026-10-03 amendment adding `WyrdGatewayCall.model`
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`

The candidate remained `HEAD` throughout this review.

## Deployed-Path Evidence

| Path | Source evidence | System effect |
|---|---|---|
| Local Workflow route selection and dependency preparation | `crates/shared/wyrd-client/src/workflow/mod.rs:123-149`; `crates/shared/wyrd-client/src/workflow/local.rs:38-102` | The loaded Workflow keeps its Cards client when present, otherwise lazily constructs the ambient client only for a selected `wyrd_gateway` route. Only selected external bindings are prepared. Missing bindings reach Skald's pre-dispatch route validation. |
| Selected secret resolution | `crates/shared/wyrd-client/src/workflow/local.rs:105-135`; `crates/shared/wyrd-utils/src/secret.rs:21-59` | Each selected env/file reference is resolved on Tokio's blocking pool. Unreadable, permissive, non-regular, oversized, or unsupported references fail before an external gateway dependency is installed. Unselected secrets are not read. |
| Local public Wyrd gateway call | `crates/shared/wyrd-client/src/workflow/gateway.rs:50-122`; `crates/shared/wyrd-client/src/transport/http.rs:350-401` | Per-call model, fallback, timeout, cancellation, and correlation remain immutable and local. The outer `select!` bounds the full auth/send/body-read exchange and drops caller-side IO on cancellation or timeout. The transport does not generally retry model calls, but its unconditional `401` replay is the defect in SYS-001. |
| Authenticated public ingress and fallback validation | `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs:101-139`; `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs:198-224`; `crates/wyrd/wyrd-server/src/components/gateway/routes.rs:1769-1805` | Authentication completes before handler interpretation. The fallback header is decoded and validated against the request model before `GatewayInvocation`; malformed, duplicate, oversized, empty, or self-referential overrides dispatch nothing. Only the decoded fallback enters the governed request, so the transport header does not reach an upstream provider. |
| Governed gateway and upstream refusal | `crates/wyrd/wyrd-server/src/components/gateway/invocation.rs:520-565`; `crates/wyrd/wyrd-server/src/components/gateway/routes.rs:1332-1359` | Gateway admission, provider selection, credentials, fallback, accounting, capture, and audit remain in their existing owner. A provider refusal is a successful gateway pipeline result carrying the provider status and is relayed at the public edge, which is why status-only `401` replay is unsafe. |
| Remote server Workflow handle | `crates/shared/wyrd-client/src/workflow/remote.rs:34-104`; `crates/shared/wyrd-client/src/transport/http.rs:692-790` | Create reuses one idempotency key across transport retries; get and cancel use the shared authenticated transport; wait polls once per second, returns all terminal snapshots, and dropping it stops polling without signalling the server run. No second lifecycle owner or client-side run state is introduced. |

## Failure, Recovery, and Sibling-Capability Assessment

| Failure or recovery path | Observed behavior | Assessment |
|---|---|---|
| Wyrd server unreachable during a local public gateway call | `post_native` returns a transport `WyrdError::Internal`; `PublicWyrdGatewayCaller` converts it to `ProviderError::Connect`. The Workflow executor retains ownership of any configured step retry. | Pass. No unbounded transport retry or shared-process failure is introduced. |
| Public gateway call cancellation or deadline | `PublicWyrdGatewayCaller::call` races the entire exchange against the supplied cancellation token and remaining timeout. Dropping the HTTP future stops the client wait; any already accepted server call settles under the existing gateway owner and cannot mutate client-side Workflow state after the future returns. | Pass for the task boundary. Focused tests cover pre-cancel and slow-response timeout. |
| Upstream provider `401` after governed dispatch | Gateway returns the provider refusal status; client transport treats it as Wyrd credential failure and replays the model call once. | **Fail — SYS-001.** This amplifies an upstream refusal into duplicate non-idempotent work. |
| Other upstream refusal/outage (`408`, `429`, `5xx`) | The native transport does not retry them. The public caller normalizes the native envelope, and the Workflow executor decides whether the step policy warrants another attempt. | Pass. Retry ownership remains with the established Workflow/gateway layers. |
| Malformed fallback header | Auth succeeds, header validation fails before `GatewayInvocation`, and no upstream dispatch occurs. Omitted header preserves tenant fallback policy. | Pass. Failure is request-scoped and does not affect sibling ingress capabilities. |
| Secret source unavailable or blocking read fails | Blocking work is isolated from async polling threads; the selected binding fails closed before dispatch. Other bindings and unrelated client capabilities retain availability. | Pass. The file-size bound makes the blocking read finite; no new secret watcher, cache, or background task was added. |
| Remote Workflow polling interrupted | Dropping `wait` drops only the client polling future. A transient GET error is returned to the caller as specified rather than cancelling or resubmitting the run. | Pass. The accepted server job remains owned by the server lifecycle. |
| Server process restart / owning replica loss | This task adds only the remote client projection. It neither persists nor reconstructs runs; the approved Revision 12 lifecycle intentionally makes process-lost run identities resolve through the server's common not-found behavior. | Pass for TASK-003. No durability or affinity mechanism is required here. |
| Concurrent public gateway calls | Fallback and timeout are stack-local, correlation is trace-only, and no mutable headers or registry state are shared between calls. | Pass. Focused proof exercises distinct simultaneous fallback values and models. |

No candidate mechanism, check, file, setting, or option was found that lacks both a Wyrd precedent and a comparable ordinary platform pattern. The finding above does not require inventing one.

## Affected Capabilities

SYS-001 is limited to locally executed Workflow steps using `LlmRoute::WyrdGateway` through `PublicWyrdGatewayCaller`. It does not affect direct in-process server Workflow gateway calls, external-gateway routes, native-provider routes, remote Workflow create/get/cancel/wait, or ordinary gateway clients that do not use this new native transport helper. Its duplicated request does, however, enter all existing gateway-owned downstream effects for the second attempt: provider dispatch, limits/accounting, capture, invocation audit, and any fallback evaluation.

## Verification Notes

- Reviewed the complete base-to-candidate diff and traced the changed client facade, route composition, native transport, ingress authentication and fallback decoder, governed gateway response path, shared secret reader, and focused tests.
- Re-ran the exact focused command:
  `mise exec -- cargo nextest run --locked -p wyrd-client --test workflow_transport -E 'test(=public_gateway_call_context_and_errors)'` — PASS, 1 test run (1 unrelated test skipped by the selector).
- The passing test covers concurrent fallback isolation, model projection, supported dialects, pre-cancellation, timeout, known native errors, and uncoded `400`/`408`/`429`/`503` mapping. Its uncoded-status table at `crates/shared/wyrd-client/tests/workflow_transport.rs:664-691` omits `401`, so it does not exercise the ambiguous status that triggers SYS-001.
- Task evidence reports passing shared, SDK, gateway, authenticated ingress/OpenAPI, codegen, boundary, Python, TypeScript, formatting, and lint lanes. Those results are credible for healthy paths and the fallback boundary, but none proves that a relayed provider `401` is not replayed.

## Overall Result

**FAIL**

The candidate's healthy paths, cancellation bounds, fallback validation, selected-secret behavior, and remote polling contract are resilient within TASK-003's scope. SYS-001 leaves one reachable dependency-refusal path that duplicates a non-replay-safe governed model call, so the task cannot pass until that bounded transport distinction and its focused proof are corrected.
