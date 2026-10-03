# TASK-003 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `a1792c45323489157e818eb29722913114014927`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12, including the human-approved 2026-10-03 amendment adding `pub model: ModelRef` to `WyrdGatewayCall`
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`

The candidate remained at the stated commit throughout this review.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-031/REQ-045/REQ-046: one shared create/get/cancel/wait handle, exact routes and DTOs, stable create idempotency key, one-second polling, terminal failures returned as snapshots, and dropping `wait` only stops polling | `crates/shared/wyrd-client/src/workflow/remote.rs:17-109` delegates create to `submit_idempotent`, uses the required run routes, and polls terminal state with the fixed interval | Independently ran both focused `workflow_transport` tests; `shared_workflow_client_contract` passed. The task also records `test:shared` passing | PASS |
| Rust SDK projects the same shared handles without Python activation | `crates/shared/wyrd-client/src/lib.rs` and `sdks/wyrd-sdk-rust/src/lib.rs` re-export/check `Workflows` and `PublicWyrdGatewayCaller` | Task records `test:wyrd-sdk`, client-tier, SDK-client-tier, and PyO3-scope checks passing | PASS |
| REQ-036A/REQ-038/REQ-039/REQ-043 and the approved amendment: every public gateway call carries the Prompt-derived `ModelRef`, step fallback, remaining duration, cancellation, and trace-only correlation; supported request dialects reach their existing ingress and Vertex is refused locally | `crates/skald/skald-workflow/src/plan.rs:264-281`, `route.rs:283-313,404-428,546-591`, and `crates/shared/wyrd-client/src/workflow/gateway.rs:50-121,222-281` propagate the immutable values and project OpenAI Chat/Responses, Anthropic, and Gemini | Independently ran `public_gateway_call_context_and_errors`; it passed. Skald's `isolated_route_calls` is recorded passing | PASS |
| Public model calls are not replayed after a response that may have come from the governed provider; only an edge authentication refusal may buy the request-local bearer refresh/replay | `HttpTransport::post_native` claims this distinction at `crates/shared/wyrd-client/src/transport/http.rs:350-360`, but its implementation retries every HTTP 401 at lines 393-397. The gateway explicitly relays provider 401 responses after dispatch (`crates/wyrd/wyrd-gateway/src/adapter/http.rs:400-420`) | Existing caller coverage tests uncoded 400/408/429/503, but no upstream 401 and no request-count assertion for this path | FAIL (B-002) |
| Native protocol errors retain only safe status/code/message/field/remediation; uncoded statuses use existing provider categories; timeout remains the timeout category | `crates/shared/wyrd-client/src/workflow/gateway.rs:138-209,294-334` uses typed native envelopes, known catalog codes, fixed uncoded messages, and only OpenAI `param` as `field` | `public_gateway_call_context_and_errors` passed its known-code, uncoded-category, timeout, and no-canary assertions | PASS |
| AC-011A: after ordinary authentication, OpenAI Chat/Responses, Anthropic Messages, and Gemini consume one optional fallback header, reject duplicate/empty/oversized/invalid/self-referential values before dispatch, set only `GatewayCallRequest.fallback`, and do not forward the header | `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs:186-294,307-428` and `routes.rs:833-927,1325-1345,1763-1798` interpret the header only after `Caller` succeeds and pass the typed value into invocation | Task records the focused PG ingress module (23/23) passing | PASS |
| AC-011A exact wire form: the public caller emits unpadded base64url over the JCS UTF-8 serialization of `GatewayFallbackOverride`, and ingress performs the specified decode/deserialization/semantic validation | `crates/wyrd-spec/src/gateway/policy.rs:126-172` emits JCS and accepts the specified protocol encoding while refusing malformed and invalid values. The authority does not require a second canonical-byte equality check after parsing | `fallback_header_round_trips_and_refuses` covers canonical output, padding, malformed JSON, limits, unknown fields, empty/duplicate/self candidates | PASS |
| AC-012: served OpenAPI documents the optional header, exact encoding/limits, and stable invalid-request outcome on all affected operations | `FALLBACK_HEADER_DOC` is attached to OpenAI Chat, OpenAI Responses, Anthropic Messages, and Gemini route declarations | Task records `gateway_inference_ingress_publishes_the_fallback_header` passing under `test:principals:integration` | PASS |
| REQ-058/AC-031: one shared local dependency path selects routes before resolving only selected external-gateway secrets; loading/apply resolve no execution secret; Native runs need no Wyrd client | `crates/shared/wyrd-client/src/workflow/local.rs:29-143` collects selected routes and resolves only named bindings; `workflow/mod.rs:123-149` loads config only for ExtGateway and creates a client only for WyrdGateway | `workflow::tests::selected_local_dependencies_use_shared_config` is recorded passing; source test covers unselected secrets, absent/wrong/unreadable bindings, no load dispatch, and a WyrdGateway bearer call | PASS |
| REQ-058 and task-specific connection rule: Rust, Python, TypeScript, and CLI consume the shared Workflow execution composition, and a Cards-loaded Workflow retains the exact Cards connection context for public WyrdGateway calls | Rust `WorkflowCards::load` retains `self.cards.engine.client` at `crates/shared/wyrd-client/src/workflow/mod.rs:237-263`; TypeScript stores that full facade. Python immediately calls `into_skald()` after file and Cards loads (`sdks/wyrd-sdk-python/src/workflow.rs:503-507`; `state/mod.rs:2635-2638`) and reconstructs a new context-free facade at run (`workflow.rs:548-551`) | Existing Python journey uses Native routes and keeps ambient configuration pointed at the same test server, so it cannot detect loss of an explicit `Cards(server_url=..., credential=...)` context. No Python WyrdGateway context-retention proof exists | FAIL (B-001) |
| INV-004/INV-011/INV-012: local Native/ExtGateway remain local; public WyrdGateway crosses the authenticated server boundary without provider credentials, provider-body workflow fields, or gateway credential administration | Route composition uses native registry, concrete external bindings, or the authenticated Wyrd client. The new request projection changes only model/body for the native dialect and adds the fallback header | Focused client test verifies bearer usage and request bodies; selected-secret test verifies secret absence from the run snapshot | PASS |
| Preserve explicit native injection and avoid a second parser, graph, traversal, validator, executor, transport, mutable adapter registry, public arbitrary-header API, polling option, new ingress, or Python/TypeScript server-run lifecycle | `Workflow::run_with` retains explicit `ProviderRegistry`; the client facade delegates hydration/execution; `post_native` is crate-private; candidate adds no listed public surface | Complete cumulative diff reviewed | PASS |
| Share existing env/file secret-reading behavior without weakening gateway/server consumers | `crates/shared/wyrd-utils/src/secret.rs` owns the moved open-handle, regular-file, owner-only, size-bounded reader; gateway, server, and local Workflow reuse it | Task records shared, gateway, and server config test sets passing | PASS |
| AC-013 and task scenarios provide credible proof of negative and edge behavior | The task includes focused transport, ingress, OpenAPI, config, SDK, language, codegen, and boundary evidence | Recorded lanes are broad, but the two failed rows have no direct closure proof: Python connection retention and provider-401 no-replay | FAIL (B-001, B-002) |

## Proposed findings

### B-001 — INCORRECT — Python discards the Cards connection context before local gateway execution

- **Violated obligation:** REQ-058, INV-007, AC-031, and TASK-003's explicit rule that a Cards-loaded Workflow retain its existing Cards connection context for public WyrdGateway calls while every SDK uses the shared facade.
- **Location:** `sdks/wyrd-sdk-python/src/state/mod.rs:2635-2638`; `sdks/wyrd-sdk-python/src/workflow.rs:503-507,548-551`.
- **Evidence:** `WorkflowCards::load` returns a `wyrd_client::Workflow` that holds the originating Cards client. Python immediately consumes it with `into_skald()`. `PyWorkflow::run` later wraps only the cloned Skald value through `wyrd_client::Workflow::from`, whose client context is `None`, so it falls back to ambient configuration.
- **Observable consequence:** `Cards(server_url=A, credential=A).workflow.load(...)` can load successfully from A, then a WyrdGateway step can fail with `WYRD_WORKFLOW_503_BINDING_UNAVAILABLE` when no ambient credential exists, or call a different ambient server/tenant B. The TypeScript projection retains the full facade and does not have this behavior.
- **Required testable correction:** Preserve the full shared `wyrd_client::Workflow` execution context in Python for both `Workflow.from_path` and `Cards.workflow.load`, and run through that retained facade. Add a Python runtime test that loads through explicit Cards overrides while ambient configuration is absent or points elsewhere, then proves its WyrdGateway call reaches only the originating server with the originating credential. Builder/from-YAML behavior and the shared runtime bridge must remain unchanged.

### B-002 — INCORRECT — every native 401 replays a non-idempotent model call, including provider refusals after dispatch

- **Violated obligation:** TASK-003's public-caller requirement to reuse authenticated transport without replaying model work, and its narrower allowance that a refresh/replay is safe only when the Wyrd edge rejects an unusable caller credential before any service acts.
- **Location:** `crates/shared/wyrd-client/src/transport/http.rs:350-400`, especially lines 393-397; reachable provider-refusal producer at `crates/wyrd/wyrd-gateway/src/adapter/http.rs:400-420`.
- **Evidence:** `post_native` checks only `StatusCode::UNAUTHORIZED`, discards the first body, refreshes, and sends the complete POST again. The gateway's provider adapter deliberately relays any non-redirect `ProviderError::Status`, including 401, as the public response after the upstream provider received the request. Existing gateway PG coverage independently demonstrates a provider 401 is relayed as status 401.
- **Observable consequence:** one Workflow model call can create two governed gateway invocations and two upstream provider attempts when the first provider answers 401. This can duplicate provider-side work, audit/accounting/capture effects, and violates the transport's own non-replay-safe model-call contract. A refresh failure is also ignored before the duplicate send.
- **Required testable correction:** Distinguish an edge authentication refusal from a relayed provider 401 before replay. Only the former may refresh and replay; a provider-originated 401 must be returned after one request, and refresh failure must surface through the existing authentication error path. Add focused native-envelope transport tests proving a stable Wyrd authentication 401 refreshes once while an uncoded/provider 401 produces exactly one HTTP request and normalizes to `SKALD_PROVIDERS_401_AUTH`.

## Non-goals and drift assessment

No unrelated product surface, compatibility alias, model ingress, credential mutation API, public arbitrary-header API, polling setting, new crate, third-party dependency, duplicate transport, parser, graph, or executor was found in the cumulative diff. The approved `WyrdGatewayCall.model` amendment is authoritative and is not drift. The shared secret reader is a direct reuse boundary for behavior already present in gateway/server consumers and needed by the shared client; it is not a new policy mechanism.

## Verification notes

- Independently ran:
  - `mise exec -- cargo nextest run --locked -p wyrd-client --test workflow_transport -E 'test(=shared_workflow_client_contract) | test(=public_gateway_call_context_and_errors)'`
  - Result: 2 passed, 0 skipped.
- Reviewed the task's recorded passing evidence for shared, SDK, gateway-native, PG ingress, OpenAPI, language, codegen, formatting, lint, and boundary lanes. Those results do not exercise the two failed paths above.
- `git diff --check` for the immutable range passed.

## Overall result

**FAIL**

The shared remote contract and most public-gateway projection are implemented, but Python loses required connection authority and provider 401 responses can duplicate non-idempotent model calls.
