# TASK-003 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `a1792c45323489157e818eb29722913114014927`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12, including the human-approved 2026-10-03 amendment adding `pub model: ModelRef` to `WyrdGatewayCall`
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Candidate identity was rechecked before writing this report and remained unchanged.

## Invariant trace

The cumulative candidate was traced from each changed producer to its runtime sink and relevant sibling consumers:

- `CreateWorkflowRunRequest` -> `Workflows::create` -> the existing idempotent transport -> direct `WorkflowRun`; run IDs similarly flow through get/cancel/wait without introducing local execution or cancellation side effects.
- resolved Prompt provider/model -> `WorkflowExecutionDependencies::resolve_route` -> `StepRoute::WyrdGateway.model` -> per-attempt `WyrdGatewayProvider` -> `WyrdGatewayCall.model` -> protocol path/body projection in `PublicWyrdGatewayCaller`.
- step fallback/deadline/cancellation/correlation -> the private per-attempt adapter -> one fresh public call -> request-local header/timeout/cancellation race and trace fields; none enters provider body or shared caller state.
- `wyrd-gateway-fallback` -> protocol authentication middleware -> ingress-owned decoding/validation -> typed `GatewayCallRequest.fallback` -> gateway invocation; ordinary callers omit it, and provider request construction does not consume caller headers.
- selected `ext_gateway` route names -> `SelectedRoutes` -> matching `LocalWorkflowConfig` entries -> selected `SecretRef` reads -> Skald `ExternalGatewayBindings`; unselected configuration is not read.
- loaded Workflow connection context -> Rust/TypeScript shared `wyrd_client::Workflow` owner, but Python converts that owner to bare `skald_workflow::Workflow` and later constructs a new context-free client Workflow at run time.
- native refusal body -> protocol error envelope -> `Ingress::problem` -> `ProviderError::RemoteProblem` -> Workflow error projection. Native provider refusals can supply the same envelope members as gateway-originated errors, so code recognition alone does not establish message provenance.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-031/REQ-045/REQ-046: one shared create/get/cancel/wait facade, stable create idempotency key, one-second polling, terminal values, dropped wait has no cancellation/resubmission | `crates/shared/wyrd-client/src/workflow/remote.rs:23-109`; existing `submit_idempotent` owner | `shared_workflow_client_contract` passed in this review; task evidence records `test:shared` | PASS |
| Rust SDK projects the shared facade without Python activation | `crates/shared/wyrd-client/src/lib.rs:40`; `sdks/wyrd-sdk-rust/src/lib.rs:46-47` | task evidence records `test:wyrd-sdk`, client-tier and PyO3-scope checks | PASS |
| Human-approved `WyrdGatewayCall.model` is produced from the resolved Prompt and remains immutable through each attempt | `crates/skald/skald-workflow/src/route.rs:62-75,281-309,395-427,464-481,543-590`; `plan.rs:264-299` | `workflow::tests::isolated_route_calls` passed in this review | PASS |
| REQ-036A/REQ-038/REQ-039/INV-020: fallback, deadline, cancellation and correlation remain per-call; supported dialects use existing ingress and Vertex refuses before IO | `crates/shared/wyrd-client/src/workflow/gateway.rs:50-123,212-281`; per-attempt route adapter above | `public_gateway_call_context_and_errors` passed in this review | PASS |
| REQ-043: remote errors retain only safe common fields and no arbitrary upstream text | `crates/shared/wyrd-client/src/workflow/gateway.rs:155-209` trusts a recognized envelope code as sufficient provenance for the envelope message; native upstream refusal bodies are retained by `crates/wyrd/wyrd-gateway/src/adapter/mod.rs:258-308` | existing test covers an uncoded canary only (`workflow_transport.rs:664-690`), not a provider body that supplies a known Wyrd code | **FAIL (INV-REV-002)** |
| AC-011A: all affected public ingresses authenticate first, decode/validate the exact header, attach typed fallback, refuse malformed/duplicate/self values before dispatch, and never forward the header | authentication wraps routes at `ingress.rs:101-169`; decoder at `ingress.rs:198-224`; Anthropic/Gemini assembly at `ingress.rs:261-303,343-435`; OpenAI assembly at `routes.rs:1769-1805` | task evidence records the authenticated PG ingress module; fallback codec focused test passed in this review | PASS |
| AC-012: served OpenAPI documents the header and error behavior on Chat, Responses, Anthropic and Gemini | `FALLBACK_HEADER_DOC` at `ingress.rs:189-196`; four affected `utoipa::path` parameter declarations | task evidence records `pg_openapi_contract` and `test:principals:integration` | PASS |
| REQ-058: one shared local configuration/dependency path selects routes first, resolves only selected secrets at run time, and performs no secret read/dispatch while loading | `workflow/local.rs:29-144`; `workflow/mod.rs:74-150`; `global_config.rs:16-35`; shared reader at `wyrd-utils/src/secret.rs:21-75` | task evidence records `selected_local_dependencies_use_shared_config` and shared/gateway/server suites | PASS |
| Cards-loaded Workflows retain their existing Cards connection context for public gateway calls; authored mixed-ref Workflows retain the client that performed hydration | Rust owner preserves `Workflow.client` at `workflow/mod.rs:33-41,82-94,237-263`; Python discards it at `sdks/wyrd-sdk-python/src/workflow.rs:503-507` and `sdks/wyrd-sdk-python/src/state/mod.rs:2635-2638` | no Python test exercises a WyrdGateway run through explicit Cards connection overrides; unit suite counts do not cover this state path | **FAIL (INV-REV-001)** |
| Python and TypeScript local execution call the shared Workflow execution path rather than duplicating configuration/runtime behavior | TypeScript retains `wyrd_client::Workflow` and calls `run` at `sdks/wyrd-sdk-ts/native/src/workflow.rs:21-26,134-159`; Python calls shared `run` but first rewraps a context-free Skald value at `sdks/wyrd-sdk-python/src/workflow.rs:548-551` | task evidence records Python/TypeScript unit and type checks; the failing context invariant is not exercised | **FAIL (INV-REV-001)** |
| Shared secret reading preserves the established env/file behavior for gateway, server wrapping keys and local Workflow bindings without adding another credential system | `wyrd-utils/src/secret.rs:21-75`; callers in `wyrd-gateway/src/credential.rs`, `wyrd-server/src/config.rs`, and `wyrd-client/src/workflow/local.rs` | task evidence records gateway/server/shared suites | PASS |
| Non-goals: no new ingress, Vertex public endpoint, credential mutation API, provider-body Workflow context, polling option, public arbitrary-header transport, or Python/TypeScript remote lifecycle surface | cumulative diff and exports; `post_native` remains crate-private | source inspection | PASS |

## Proposed findings

### INV-REV-001 — INCORRECT: Python drops the Cards connection context before local execution

- **Violated obligation:** REQ-058 and the task's shared-local-execution contract require a Cards-loaded Workflow to retain the Cards connection context for public WyrdGateway calls, and require all first-class SDKs to use that shared execution composition.
- **Exact location:** `sdks/wyrd-sdk-python/src/workflow.rs:503-507,548-551`; `sdks/wyrd-sdk-python/src/state/mod.rs:2635-2638`.
- **Evidence:** both Python load paths receive a `wyrd_client::Workflow`, immediately call `into_skald()`, and store only `skald_workflow::Workflow`. `PyWorkflow::run` then creates a new `wyrd_client::Workflow::from(self.inner.clone())`; that constructor sets `client: None` (`crates/shared/wyrd-client/src/workflow/mod.rs:190-197`). The run consequently uses ambient configuration at `workflow/mod.rs:134-142` instead of the explicit Cards client that loaded the registered graph. The TypeScript sibling retains the shared Workflow value and does not have this break.
- **Observable consequence:** a Python Workflow loaded through `Cards(server_url=..., credential=...)` can send a local `wyrd_gateway` step to a different ambient server, fail for missing ambient credentials, or use the wrong principal after ambient configuration changes. A mixed authored Workflow that read external Cards has the same loss between load and run.
- **Required testable correction:** preserve the shared `wyrd_client::Workflow` execution owner, including its private client context, across the Python handle's load-to-run lifetime. Keep pure authoring behavior on the existing Skald owner without duplicating transport or configuration. Add focused Python-runtime proof that a Workflow loaded through explicit Cards overrides still sends a WyrdGateway call through those overrides after ambient configuration is absent or points elsewhere, and that authored local-only behavior remains unchanged.

### INV-REV-002 — INCORRECT: a native upstream can spoof a known Wyrd code and make arbitrary provider text portable

- **Violated obligation:** REQ-043 and the task's packet-local native-error contract prohibit arbitrary upstream error text, prompts, and diagnostic details from surviving normalization; only a safe message may enter `RemoteProblem`/`WorkflowRunError`.
- **Exact location:** `crates/shared/wyrd-client/src/workflow/gateway.rs:155-209`, especially `190-198`; producer path `crates/wyrd/wyrd-gateway/src/adapter/mod.rs:258-308`.
- **Evidence:** native gateway refusals intentionally retain the provider's JSON body after credential scrubbing. `Ingress::problem` treats any envelope code present in `WyrdError::codes()` as gateway-authored and copies that same envelope's message unchanged. OpenAI, Anthropic, and Google providers control those native envelope members and can return a known Wyrd-looking code with a message containing echoed prompt or arbitrary diagnostic text. The uncoded branch redacts such text, but the recognized-code branch re-admits it. The current test proves only an uncoded `sk-canary` body is removed.
- **Observable consequence:** a reachable provider refusal can persist arbitrary upstream text in the normalized Workflow failure despite the explicit redaction invariant.
- **Required testable correction:** at the public-caller normalization boundary, do not use presence of a recognized code as proof that the native envelope message is gateway-authored. Preserve the required status/code/field/remediation, but derive the portable message from trusted catalog/category metadata rather than the untrusted native body unless provenance is already established by an existing trusted mechanism. Do not change ordinary public gateway clients' native relay behavior or add a new wire marker. Extend `public_gateway_call_context_and_errors` with known-code OpenAI, Anthropic, and Google envelopes whose messages contain a canary and prove none survives while the required common fields remain.

## Verification notes

Commands run against the immutable candidate during this review:

- `mise exec -- cargo nextest run --locked -p wyrd-client --test workflow_transport -E 'test(=shared_workflow_client_contract) | test(=public_gateway_call_context_and_errors)'` — 2 passed.
- `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=gateway::policy::tests::fallback_header_round_trips_and_refuses)'` — 1 passed.
- `mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::isolated_route_calls)'` — 1 passed.
- `git diff --check 58d07d7260df1f022a721e720a28ea48e5096e35..a1792c45323489157e818eb29722913114014927` — passed.

The task's recorded broader verification was reviewed as evidence, not rerun wholesale. The green focused transport tests do not cover the two failing states above.

## Overall result

**FAIL**

The remote facade, route/model propagation, per-call isolation, authenticated fallback ingestion, selected-secret preparation, OpenAPI surface, and non-goals satisfy the reviewed obligations. The candidate does not preserve registered connection context through the Python SDK, and its public error normalization can trust provider-controlled text when the provider spoofs a known Wyrd code.
