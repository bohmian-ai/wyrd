# Security domain review — TASK-003-r5

## Subject and review boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `5084b0e5b30fe79fdb2468ad7d0fa3a9d2106069`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Remediation authority: TASK-003 R1 through R4 under
  `changes/active/skald-workflow-runtime/review/`
- Human approvals applied: the 2026-10-03 `WyrdGatewayCall.model` amendment,
  the R1 same-spec native-`401` wording correction, and the approved run-start
  `spawn_blocking` boundary for configuration, client, and selected secret reads

This review is scoped to security, authentication, authorization, tenancy, and
credential boundaries changed by the cumulative candidate: authentication
before fallback interpretation; fallback bounds, validation, and consumption;
model and tenant authority; retained client context; native `401` renewal with
no resend; selected-secret-only resolution; secret-file handling; portable
error redaction; and public ingress dispatch. The candidate remained at the
stated commit throughout this review.

## Authority and source coverage

| Boundary | Governing authority | Source and caller coverage | Assessment |
|---|---|---|---|
| Authentication before fallback interpretation | `architecture/wyrd-security-posture.md` trust-boundary and identity rules; Revision 12 public-header contract; TASK-003 Scenario 3 | `wyrd-server/src/components/gateway/ingress.rs::{GatewayIngress::authenticate,gateway_ingress_router,requested_fallback,anthropic_messages,gemini_generate_content}`; `gateway/routes.rs::{chat_completions,responses,openai_call,invoke_openai}`; recorded PG ingress evidence | Protocol-specific middleware verifies the Wyrd token and inserts the principal before the route handler runs. Handlers do not enter fallback parsing or invocation for an authentication error. Only the typed override reaches `GatewayCallRequest`. PASS. |
| Fallback bounds, duplicates, malformed values, and self-reference | Revision 12 REQ-036A/AC-011A; `wyrd-spec` contract rules | `wyrd-spec/src/gateway/policy.rs::GatewayFallbackOverride::{to_header_value,from_header_value,validate_for}`; server `requested_fallback`; PG test source | The receiver caps the encoded header at 8 KiB and decoded JSON at 4 KiB, requires unpadded base64url and the deny-unknown typed shape, rejects empty/duplicate/self-referential candidates, and rejects repeated header values. Every refusal occurs before gateway dispatch. PASS. |
| Header consumption and provider isolation | Security posture client-to-gateway boundary; Revision 12 REQ-034/038 and INV-012 | `wyrd-client/src/workflow/gateway.rs`; server ingress/routes; gateway invocation and recorded provider-request assertions | The public caller adds only the request-scoped fallback header to its authenticated Wyrd request. The server consumes it into `GatewayCallRequest.fallback`; provider dispatch receives neither this header nor the caller's Wyrd bearer. Gateway provider credentials remain server-owned. PASS. |
| Tenant, principal, model authorization, and audit | Security posture identity/authorization/audit rules; approved `WyrdGatewayCall.model` amendment | `skald-workflow/src/route.rs::{gateway_model,WyrdGatewayProvider}`; `wyrd-client/src/workflow/gateway.rs::NativeCall::project`; public ingress; existing `GatewayInvocation` admission/decision path | The Prompt-derived typed `ModelRef`, not a provider-body override, selects the public route target. Tenant and principal come only from the verified credential. Requested and fallback models still traverse the existing tenant snapshot, typed authorization, provider-credential, and tracked canonical audit owners. The fallback header carries no tenant or credential authority. PASS. |
| Native bearer renewal and send-once safety | Security posture token lifecycle; approved R1-R3 correction | `wyrd-client/src/auth.rs::{bearer,force_refresh}`; `transport/http.rs::post_native`; `workflow/gateway.rs::call`; recorded complete, truncated, and pending-body `401` cases | One model POST is sent. Once response headers establish `401`, the existing auth owner renews before body collection; renewal failure is authoritative and successful renewal prepares later calls without replaying the current request. Cancellation and the remaining deadline continue to bound local IO. PASS. |
| Retained client and tenant context | Revision 12 client reuse contract; R1 FIND-TASK-003-1 | `wyrd-client/src/workflow/mod.rs::{Workflow,WorkflowCards::load}`; Python `workflow.rs` and `state/mod.rs`; recorded Python retained-client journey | Registered and external-ref loading retain the complete `WyrdClient`, including endpoint, credential, token cache, and connection context. Python keeps the shared Workflow owner instead of reconstructing ambient authority or copying a bearer into Python-visible state. Server tenant authority remains token-derived. PASS. |
| One run-start configuration context | R3/R4 FIND-TASK-003-10; human-approved run-start blocking boundary | `wyrd-client/src/workflow/mod.rs::{run_with,load_local_setup,local_setup_from}`; `ClientConfig::from_global_with_env`; R4 focused source/evidence | A client-less mixed-route run calls `GlobalConfig::load` at most once and derives both `LocalWorkflowConfig` and the public Wyrd client from that same value. A retained client suppresses ambient client assembly, and a native-only run reads no ambient configuration. The prior split-snapshot credential/context defect is closed. PASS. |
| Selected external secrets and file handling | Security posture cryptography/secret rules; Revision 12 REQ-042/058 | `wyrd-client/src/workflow/local.rs::{SelectedRoutes,resolve_binding}`; `wyrd-utils/src/secret.rs`; `skald-workflow/src/route.rs::{ExternalGatewayBinding,merged_headers}`; gateway/server secret consumers | Route selection precedes resolution. Only named selected bindings are read at run start; loading/apply resolve no execution secret. Values remain `SecretString`, secret HTTP values are marked sensitive, collision/reserved-header/origin/protocol checks remain intact, and errors disclose no locator or value. File reads validate metadata on the opened handle, require a regular owner-only Unix file, and cap content at 64 KiB. PASS. |
| Portable error and telemetry redaction | Revision 12 `RemoteProblem` contract; repository error authority | `wyrd-client/src/workflow/gateway.rs::{Ingress::problem,wyrd_problem,decode}`; Skald workflow error projection; tracing annotations | Recognized codes use catalog title/remediation rather than provider text; uncoded refusals use fixed category text; transport and decode failures quote no response body; only the approved optional OpenAI `param` projects as `field`. Tracing records scrubbed run/step/attempt correlation and skips request bodies and credentials. PASS. |
| Dependency and supply-chain delta | AGENTS dependency-cost rule; task non-goals | Changed manifests and `Cargo.lock` | The change adds only existing workspace edges and dependencies already resolved in the workspace. No new external package or version is introduced. PASS. |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None. No optional hardening is required for acceptance.

### Positive Controls

- Public gateway authentication completes before fallback decoding and before
  any governed dispatch.
- The fallback header is request-scoped, size-bounded, duplicate-checked,
  typed, model-validated, consumed at ingress, and never forwarded upstream.
- Tenant and principal identity remain exclusively credential-derived; neither
  provider bodies nor fallback candidates can select a tenant or credential.
- Prompt-derived `WyrdGatewayCall.model` is the target authority for protocol
  projection, preventing provider-body model confusion.
- Native model requests are never replayed after ambiguous or
  provider-originated `401` responses; renewal begins before an untrusted body
  can hold the auth path open.
- Gateway provider credentials remain behind the server boundary and do not
  enter Workflow state, Skald requests, client configuration, portable errors,
  observations, audit payloads, or tracing.
- Registered and authored-external Workflows retain their loading client, while
  client-less mixed-route runs derive one coherent setup from one configuration
  snapshot.
- External binding secrets resolve only for selected routes at run start,
  remain in redacted secret types, and are subject to established origin,
  protocol, header, TLS/profile, and screened/pinned endpoint controls.
- No mechanism, check, file, setting, option, allowlist, dependency, or
  compatibility path absent from repository and common project practice was
  added or required; no DRIFT finding is proposed.

## Prior-finding closure

| Prior finding | Security assessment |
|---|---|
| `FIND-TASK-003-1` | CLOSED. Rust and Python retain the loading client and its authenticated connection context through local execution. |
| `FIND-TASK-003-2` | CLOSED. An observed native `401` renews before body collection, sends one model POST, and never replays it. |
| `FIND-TASK-003-3` | CLOSED. Provider-controlled message text does not survive a recognized-code projection; catalog metadata is authoritative. |
| `FIND-TASK-003-4` | CLOSED. Cross-module secret consumers receive `SecretString`; the plaintext file reader is private. |
| `FIND-TASK-003-5` through `FIND-TASK-003-9` | No security regression is present in the cumulative candidate. |
| `FIND-TASK-003-10` | CLOSED. Filesystem-bearing setup remains on the approved blocking boundary, and one run-start `GlobalConfig` snapshot now feeds both mixed-route consumers. |
| `FIND-TASK-003-11` | CLOSED. Auth-owner documentation distinguishes replay-safe transports from the native send-once path. |
| `FIND-TASK-003-12` | CLOSED. Consuming the shared facade documents loss of retained client and automatic dependency composition. |

## Verification evidence and limits

Per the human standing direction, this review was strictly source-only. It did
not build, compile, or run tests, Cargo, mise, pnpm, pytest, or any other
verification command. I inspected the cumulative base-to-candidate diff,
current source and callers, the approved specification, original task, R1-R4
remediations, prior security findings, and the recorded implementation
evidence.

The recorded evidence is specific and consistent with source: it covers all
affected public ingresses and non-forwarding, malformed/oversized/repeated and
self-referential fallback refusal, per-call isolation, all native error
envelopes, send-once complete/truncated/pending-body `401` renewal, selected
local dependencies, the mixed-route single-snapshot setup, and the Python
retained-client journey. No required security evidence is missing, unclear, or
contradicted by source.

## Overall result

**PASS**

The cumulative candidate satisfies the approved security, authentication,
tenant-authority, credential-containment, fallback-validation, native-renewal,
and error-redaction obligations for TASK-003. No material security finding is
proposed.
