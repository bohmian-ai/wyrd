# Maintainer Review — TASK-003

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `a1792c45323489157e818eb29722913114014927`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12, including the human-approved 2026-10-03 amendment adding `pub model: ModelRef` to `WyrdGatewayCall`
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Candidate remained at the supplied commit throughout this review.

## Changed-surface coverage

| Changed surface | Owners, callers, tests, and declarations inspected | Maintainer assessment |
|---|---|---|
| Remote Workflow lifecycle | `wyrd_client::workflow::remote::Workflows`; shared `WyrdClient` request/idempotent transport; Rust SDK root projection; `workflow_transport::shared_workflow_client_contract` | Cohesive dependency-owning handle with discoverable methods. Names, signatures, direct DTO returns, polling semantics, and rustdoc align with the approved public contract. |
| Public governed gateway client | `PublicWyrdGatewayCaller`, `NativeCall`, `Ingress`, native-envelope normalization helpers, `HttpTransport::post_native`, Skald `WyrdGatewayCaller` consumers, and `workflow_transport::public_gateway_call_context_and_errors` | The public handle owns one shared client and keeps per-call data local. Protocol projection and error normalization are colocated with that owner rather than exposed as a general header API. No material maintainer issue found. |
| Local route composition | `Workflow::run`/`run_with`, `SelectedRoutes`, `resolve_binding`, `WorkflowExecutionDependencies`, registered and authored loaders, and `selected_local_dependencies_use_shared_config` | `Workflow` remains the public shared-client owner and `SelectedRoutes` is a meaningful private planning value, not a utility shell. Selected binding resolution stays beside the run composition. Python fails to retain this owner; see `MAINT-TASK-003-1`. |
| Model propagation | `ExecutionPlan::build`, `WorkflowExecutionDependencies::resolve_route`, `StepRoute`, `WyrdGatewayProvider`, `gateway_model`, `WyrdGatewayCall`, and `isolated_route_calls` | The approved `ModelRef` is derived once during route resolution, stored with immutable route state, and copied into each call. The shape is findable and avoids reading protocol bodies as model authority. |
| Fallback header contract | `GatewayFallbackOverride::{to_header_value,from_header_value}`, exported header/size constants, `requested_fallback`, OpenAI/Anthropic/Gemini route assembly, served OpenAPI annotations, policy tests, PG ingress tests, and OpenAPI contract test | Encoding/validation belongs to the typed contract; ingress owns authenticated extraction; route handlers pass a typed value. Names and docs agree. The mechanism is explicitly approved and uses established base64/JCS/OpenAPI facilities; no extra check, option, or competing mechanism was added. |
| Secret reference reading | `wyrd_utils::secret`, gateway credential resolution, server managed-key loading, client external-binding resolution, manifests, and existing file-secret tests | This is consolidation of three established env/file secret readers under one shared rule, not a new configuration mechanism. Callers retain their domain-specific error mapping and async blocking boundary. No material maintainer issue found. |
| SDK projections | Rust root re-export proof; TypeScript `NativeWorkflow` and registered loader; Python `PyWorkflow::{from_path,run}` and `PyWorkflowCards::load` | Rust and TypeScript retain the shared `wyrd_client::Workflow`. Python unwraps it to `skald_workflow::Workflow`, which loses owner state and contradicts the shared-facade boundary. See `MAINT-TASK-003-1`. No generated declaration mismatch was introduced by the changed public signatures. |
| Tests and evidence | New shared-client HTTP contract test, local selected-route test, fallback policy test, server PG ingress matrix, served OpenAPI test, Skald route assertion, task-recorded focused and aggregate results | Tests are outcome-oriented and use existing Wiremock/Postgres/OpenAPI patterns. The task records passing format, lint, shared, SDK, gateway, server, codegen, boundary, Python, and TypeScript lanes. The missing Python context-preservation case explains why those green lanes do not close the finding below. |

## Material finding

### MAINT-TASK-003-1 — Python discards the shared Workflow owner and its Cards connection context

- Classification: `INCORRECT`
- Changed location: `sdks/wyrd-sdk-python/src/workflow.rs:505-507` and `sdks/wyrd-sdk-python/src/workflow.rs:548-551`
- Reachable caller: `sdks/wyrd-sdk-python/src/state/mod.rs:2635-2638`
- Shared owner: `crates/shared/wyrd-client/src/workflow/mod.rs:33-40`, `123-149`, and `257-263`
- Governing authority:
  - `AGENTS.md` §2 and §3: `wyrd-client` is the sole SDK-facing Rust client implementation; language wrappers project it rather than duplicating transport or lifecycle behavior.
  - `AGENTS.md` §5 and maintainer style “Structs and methods: own shared state once”: dependency-backed behavior keeps one concrete owner and its dependencies together.
  - Revision 12 “Resolution, authority, and ownership”: language SDKs project the thin shared Workflow facade, and a Cards-loaded Workflow retains the Cards connection context for public `WyrdGateway` calls.
  - TASK-003 “Shared local execution configuration”: Cards connection overrides remain in effect and Python uses the shared run path.
- Evidence: `WorkflowCards::load` returns `wyrd_client::Workflow { inner, client: Some(self.cards.engine.client.clone()) }`. `PyWorkflowCards::load` immediately calls `workflow.into_skald()`, and `PyWorkflow::from_path` does the same. `PyWorkflow` stores only `skald_workflow::Workflow`. Its newly changed `run` then calls `wyrd_client::Workflow::from(self.inner.clone())`; that constructor necessarily sets `client: None`. A `wyrd_gateway` route therefore falls back to `WyrdClient::from_global()` instead of using the client that loaded the registered Workflow.
- Concrete maintenance cost and observable consequence: the Python type claims to project the shared Workflow but splits its state across an irreversible conversion. A maintainer following `cards.workflow.load -> Workflow.run` cannot see that the explicit Cards endpoint, credential, token cache, and connection pool vanish at the language boundary. A configured Cards client can load from one server and then run against ambient configuration for another server, or fail for missing ambient credentials. Rust and TypeScript retain the shared owner, so the same public workflow has language-specific connection semantics.
- Smallest testable correction: keep the `wyrd_client::Workflow` returned by shared file and Cards loaders as the Python wrapper's owner through `run`; do not duplicate its private client state in a Python-only transport/config field. Builder-created Python Workflows may still enter that owner through the existing `From<skald_workflow::Workflow>` conversion, and any builder mutation must update the wrapped Skald value without silently dropping retained client context. Reuse the existing Python SDK server/fixture pattern to prove that a registered Workflow loaded through a Cards instance whose server/credential differs from ambient configuration sends its local public-gateway call through that Cards context. This proof is boundary-focused and does not require TASK-005's full registered Workflow journey.

## Calibrated nonblocking observations

- The two new HTTP contract tests each cover several task-prescribed cases in one selector. They are long, but the task explicitly defined those selectors as the focused contract matrices and the cases share one wire fixture. Splitting them is an equally valid preference, not a blocking maintainer finding.
- `SelectedRoutes::of` makes two short passes over Workflow steps. Collapsing them would not materially improve ownership or change safety.
- The shared secret reader adds no novel policy: it moves the existing open-handle, owner-only, bounded-file convention into one reusable owner. Under the standing direction, it is not DRIFT.

## Verification assessment

The task records successful focused tests and the relevant shared, SDK, gateway, server/OpenAPI, codegen, boundary, format, lint, Python, and TypeScript lanes. I did not rerun those lanes for this read-only review. Their coverage is credible for the exercised surfaces, but none tests the Python `PyWorkflowCards::load -> PyWorkflow::run` context handoff identified above.

## Overall result

**FAIL**

`MAINT-TASK-003-1` is material because the Python projection destroys dependency state owned by the shared facade and changes which authenticated server receives a reachable public-gateway call. The correction is bounded to preserving the existing owner across the Python boundary; it requires no new mechanism, check, file, setting, option, or product decision.
