# Maintainer Review — TASK-003-r2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `2e6f13f505daf19050e588ea0d1d7fc936697e27`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12, including the human-approved 2026-10-03 amendment adding `pub model: ModelRef` to `WyrdGatewayCall`
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Remediation task: `changes/active/skald-workflow-runtime/review/TASK-003-r1/TASK-003-R1-preserve-client-context-and-native-call-safety.md`, including the approved same-spec native-401 wording correction

The candidate remained at the stated commit during this review. This was a strictly read-only source review: no build, compile, test, cargo, mise, pnpm, pytest, package-manager, or other verification command was run. The implementer's recorded evidence was inspected as evidence of prior execution, not rerun.

## Changed-surface coverage

| Surface | Owning symbols and source inspected | Callers and proof inspected | Maintainer assessment |
|---|---|---|---|
| Shared remote run client | `workflow/remote.rs::Workflows::{new,create,get,cancel,wait}`, `HttpTransport` request/idempotency owners, root exports | `workflow_transport::shared_workflow_client_contract`, Rust SDK export-shape test | Cohesive dependency-owning handle with direct method names and documented polling/idempotency semantics. No duplicate transport. |
| Public governed gateway caller | `workflow/gateway.rs::PublicWyrdGatewayCaller`, `NativeCall`, `Ingress`, `HttpTransport::post_native` | `WyrdGatewayCaller` call path, `workflow_transport::public_gateway_call_context_and_errors`, Skald attempt adapter | Caller owns the shared client; protocol projection and refusal normalization remain focused private stages. R1's send-once and catalog-message corrections are easy to trace. |
| Shared local execution composition | `workflow/mod.rs::Workflow::{run,run_with,as_skald,as_skald_mut}`, `local.rs::SelectedRoutes`, `GlobalConfig.workflow`, shared secret reader | Rust/Python/TypeScript run callers, selected-route tests, gateway/server secret consumers | One shared owner composes selected dependencies. The new mutable Skald accessor keeps the retained client in the wrapper and is used only for boundary authoring edits. Secret resolution reuses one redacted shared boundary. |
| Skald gateway seam | `WyrdGatewayCall`, `WorkflowExecutionDependencies::resolve_route`, `StepRoute`, `WyrdGatewayProvider::send`, plan call site | Skald route/executor tests and shared-client caller | The approved model amendment is carried explicitly from the Prompt-owned provider/model to each immutable call without moving gateway IO into Skald. |
| Fallback contract and server ingress | `GatewayFallbackOverride::{to_header_value,from_header_value}`, `requested_fallback`, OpenAI/Anthropic/Gemini handlers and route docs | `public_ingress_workflow_fallback`, served OpenAPI assertion | Codec lives with the contract; ingress owns header consumption; handlers keep their existing invocation owner. Naming and documentation expose the boundary clearly. |
| Python ownership remediation | `PyWorkflow`, all authoring mutations, `from_path`, `run`, `WorkflowCardsClient::load` | New public Python registered/authored load-to-run journey; TypeScript and Rust sibling projections | Prior context-loss split is closed: Python now stores the complete shared Workflow, mutates only its Skald value, and runs through the retained client owner. |
| Public declarations and manifests | `wyrd-client`/`wyrd-utils` manifests and exports, Rust SDK root projection, Python source annotations | Recorded codegen/typecheck/boundary evidence | Dependency additions are existing-workspace owner edges. No new feature, setting, compatibility alias, or generated-file edit entered the implementation. |

## Prior-finding closure

The prior maintainer finding `MAINT-TASK-003-1` is closed in source. `PyWorkflow.inner` now retains `wyrd_client::Workflow` (`sdks/wyrd-sdk-python/src/workflow.rs:203-214`), registered and authored loaders pass that owner through unchanged (`sdks/wyrd-sdk-python/src/state/mod.rs:2635-2638`, `sdks/wyrd-sdk-python/src/workflow.rs:511-515`), mutations update only `as_skald_mut()` (`workflow.rs:304-435`), and `run` delegates to the retained owner's `run` (`workflow.rs:536-561`). The new journey exercises registered loading, authored external refs, successful and refused edits, changed and absent ambient configuration, and the loading server's actual model-call boundary.

## Material finding

### MAINT-TASK-003-R2-1 — Changed signatures and fields hide type ownership behind qualified paths

- **Classification:** repository-maintainability violation
- **Changed locations:**
  - `crates/shared/wyrd-client/src/transport/http.rs:367-372` uses `bytes::Bytes` in the new `post_native` signature.
  - `crates/skald/skald-workflow/src/route.rs:281-287` adds `prompt: &skald_spec::Prompt` to `resolve_route` despite the existing `skald_spec` import block.
  - `sdks/wyrd-sdk-python/src/workflow.rs:203-214,575-578` uses `wyrd_client::Workflow` in a struct field, an `impl From` header, and its method parameter.
  - `crates/shared/wyrd-client/tests/workflow_transport.rs:100,387` returns `Vec<wiremock::Request>` from new helpers.
  - `crates/wyrd/wyrd-server/src/components/gateway/pg_invocation_tests.rs:5323-5329` returns `axum::response::Response` from the new ingress helper.
- **Governing rule:** `architecture/agent-rules.md` requires types to be imported at module top and used by bare name in struct fields, function parameters, return types, trait bounds, and `where` clauses. The import block is the module's dependency manifest.
- **Concrete maintenance cost:** these newly changed declarations make a reader scan implementation-qualified paths instead of one dependency manifest and create inconsistent names for the same type within a module. The Python boundary is the clearest case: unaliased Skald `Workflow` and qualified shared-client `Workflow` obscure which owner is stored exactly where the R1 correction depends on that distinction. This also leaves the new code in the same conformance state R1 had to repair for function-local imports.
- **Smallest testable correction:** import and use bare names at the owning module tops. Where two `Workflow` types coexist, alias both by role (for example, the repository already uses `NativeBifrost` and `NativePrompt` aliases) and use those aliases consistently in fields, impl headers, parameters, and docs. Import `Bytes`, `Prompt`, `Request`, and `Response` for the other signatures. Do not add a lint, allowlist, checker, helper, feature, or behavior change. Static source inspection plus the ordinary formatting/lint evidence is sufficient; no new runtime test is warranted.

## Calibrated nonblocking observations

- `PublicWyrdGatewayCaller`, `Workflows`, `SelectedRoutes`, `NativeCall`, and the shared secret reader each have a concrete owner and live callers. I found no unsupported abstraction, compatibility path, configuration option, or bespoke review mechanism to classify as DRIFT.
- `Workflow::as_skald_mut` is a broad-looking accessor, but it is an earned cross-crate boundary for the existing Python authoring surface, preserves the dependency-owning client wrapper, and exposes no additional field visibility inside Skald. I therefore do not treat it as drift.
- The large `public_gateway_call_context_and_errors` test is cohesive around one public caller contract and was explicitly selected by the task/remediation. Splitting it is a preference, not a blocking maintainer issue.

## Verification assessment

The implementation packet records focused RED/GREEN proof for Python client retention, single-send 401 handling, and catalog-derived refusal messages, plus shared, Python, codegen, boundary, format, and lint lanes. Source matches the stated R1 corrections. This reviewer did not execute any command that builds or verifies the repository, per standing direction. The qualified-type source violation above is visible directly and is not closed by recorded green lint evidence because it is a repository rule rather than a compiler/type-check outcome.

## Overall result

**FAIL**

The runtime ownership and prior maintainer defect are repaired, but the candidate still violates the repository's mandatory declaration/import shape at several newly changed locations. The correction is mechanical, bounded, behavior-preserving, and needs no new mechanism or verification surface.
