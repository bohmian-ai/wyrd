# Maintainer Review — TASK-003 r3

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `0732ed92fc2907cb806ac918b09abbf6d44ff04f`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12, including the human-approved 2026-10-03 minimal amendment adding `pub model: ModelRef` to `WyrdGatewayCall`
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior remediation: `changes/active/skald-workflow-runtime/review/TASK-003-r1/TASK-003-R1-preserve-client-context-and-native-call-safety.md`, including the approved same-spec correction of the native-`401` wording
- Current remediation: `changes/active/skald-workflow-runtime/review/TASK-003-r2/TASK-003-R2-close-renewal-proof-and-source-contracts.md`

The candidate remained at the stated commit throughout this review. This was a
strictly read-only source review: no build, compile, test, Cargo, mise, pnpm,
pytest, formatter, linter, test-listing, package-manager, or other verification
command was run. The implementer's recorded evidence was inspected but not
rerun.

## Changed-surface coverage

| Surface | Owning symbols and source inspected | Callers, consumers, and proof inspected | Maintainer assessment |
|---|---|---|---|
| Shared remote run client | `workflow/remote.rs::Workflows::{new,create,get,cancel,wait}`, `HttpTransport::{submit_idempotent,request_json}`, client and Rust SDK exports | `workflow_transport::shared_workflow_client_contract`, root SDK projection | One concrete dependency-owning handle exposes the lifecycle with direct names. Idempotency, polling, cancellation, and partial-progress contracts are discoverable at the methods that own them; no route-specific transport was added. |
| Public governed gateway caller | `workflow/gateway.rs::PublicWyrdGatewayCaller`, `NativeCall`, `Ingress`, error projection helpers, `HttpTransport::post_native` | `WyrdGatewayCaller`, Skald attempt adapter, all `WyrdGatewayCall` constructors, `public_gateway_call_context_and_errors` | The caller owns one shared client and keeps per-call state local. Protocol projection, raw transport, and refusal normalization are separated into focused private stages. The send-once and reactive-renewal ordering is visible at the transport owner. |
| Shared local execution composition | `workflow/mod.rs::Workflow`, `WorkflowCards`, `local.rs::SelectedRoutes`, `LocalWorkflowConfig`, `resolve_binding` | Rust load/run paths, Python and TypeScript facade consumers, selected-route unit evidence, CLI-facing shared facade | The shared Workflow wrapper adds the client boundary without duplicating the loader, Card traversal, validation, or executor. Route selection and selected-secret preparation remain with focused owners. The mutable Skald accessor preserves the wrapper's client context during language-boundary authoring. |
| Shared secret boundary | `wyrd_utils::secret::{read_secret_ref,read_secret_file,restrictive}`, gateway `read_binding`, server managed-key loading | Gateway credential resolution, server boot configuration, local Workflow binding resolution, their existing tests/evidence | The extracted reader replaces duplicated file checks with the same open-handle, bounded, redacted contract. It introduces no feature, setting, backend, or compatibility path and keeps callers responsible for async blocking boundaries. |
| Skald gateway seam | `WyrdGatewayCall`, `WorkflowExecutionDependencies::resolve_route`, `StepRoute`, `gateway_model`, `WyrdGatewayProvider::send`, execution-plan call site | Route/executor tests, public shared-client caller, all call constructors | The approved `model` field is derived once from the Prompt-owned provider/model and carried through immutable attempt state. Skald remains independent of Wyrd transport and server tenancy. Names and types make that ownership traceable. |
| Fallback contract and public ingress | `GatewayFallbackOverride::{to_header_value,from_header_value}`, fallback constants, `requested_fallback`, OpenAI/Anthropic/Gemini handlers, `openai_call`, `invoke_openai` | `public_ingress_workflow_fallback`, served OpenAPI contract test, `GatewayCallRequest` lowering | Encoding/validation stays with the pure contract; authentication remains before interpretation; ingress owns header consumption; invocation remains with the existing gateway owner. The shared OpenAPI description avoids divergent route prose without creating a new configuration or runtime mechanism. |
| Python retained-client boundary | `PyWorkflow`, all authoring mutations, `from_path`, `run`, `PyWorkflowCards::load`, role-specific `SkaldWorkflow`/`ClientWorkflow` aliases | Public integration journey covering registered and authored external-ref loading, successful/refused edits, changed or absent ambient configuration | Python now stores the complete shared Workflow and edits only its Skald value, so the loading client survives mutation and execution. The aliases expose which owner is held at every declaration; no validation or runtime behavior moved into Python. |
| Tests, declarations, and manifests | New/changed Rust tests and helpers, Python integration test, crate manifests, public exports, generated-declaration evidence | R1/R2 verdicts and remediation evidence, focused selector records, codegen/typecheck/boundary records | Test names describe caller outcomes and the R2 loopback fixture reuses ordinary native HTTP primitives for body-cutoff and post-dispatch cancellation cases that Wiremock cannot express. Panic contracts and imported type aliases are now present. Dependencies are existing workspace owner edges; no generated artifact was hand-edited. |

## Prior-finding closure

| Prior finding | Maintainer closure assessment |
|---|---|
| `FIND-TASK-003-1` | Closed. `PyWorkflow` retains `ClientWorkflow`; registered and authored external-ref loaders preserve the client, authoring mutations operate through `as_skald_mut`, and `run` delegates to the retained owner. |
| `FIND-TASK-003-2` | Closed in source. `post_native` records status, retains the body-read result, performs `force_refresh` for every observed `401`, and only then propagates a body-read failure, without replaying the model POST. |
| `FIND-TASK-003-3` | Closed. Recognized refusal codes take title and remediation from the derive-backed catalog, while provider-controlled envelope messages are discarded. |
| `FIND-TASK-003-4` | Closed. The file reader is private; public resolution returns `SecretString` and preserves the existing open-handle, size, and Unix permission checks. |
| `FIND-TASK-003-5` | Closed. Imports live in the owning module or test-module import blocks. |
| `FIND-TASK-003-6` | Closed. Changed fields and signatures use module-imported bare names or explicit role aliases (`SkaldWorkflow` and `ClientWorkflow`). |
| `FIND-TASK-003-7` | Closed. The remediation's panic-capable helpers and tests carry substantive `# Panics` contracts naming their fixture or assertion invariants. |
| `FIND-TASK-003-8` | Closed. Remote create/cancel, native POST, public gateway call, and shared local execution document cancellation and post-dispatch partial progress at their owning APIs. |
| `FIND-TASK-003-9` | Closed by source and recorded evidence. The existing focused selector now waits until the loopback boundary observes the model POST, cancels the pending call, and checks that no resend appears. |

## Material findings

None.

I found no changed layout, owner or method shape, name, argument/return type,
test structure, documentation contract, generated declaration boundary, or
manifest edge that creates a material maintenance defect under `AGENTS.md`,
`architecture/agent-rules.md`, or the maintainer guide.

## DRIFT assessment

No unsupported mechanism, check, file, setting, option, compatibility path,
feature, dependency, abstraction, or test harness survives this candidate.
`Workflows`, `PublicWyrdGatewayCaller`, `SelectedRoutes`, the fallback codec,
and the shared secret reader each have a concrete owner and multiple live
callers or consumers. The R2 raw loopback fixture is a normal protocol-level
test technique already used by comparable Rust HTTP clients and is limited to
the response-cutoff and pending-connection behavior that the installed mock
server does not provide. No remediation or additional enforcement mechanism is
required.

## Verification assessment

The task and R2 remediation record successful focused evidence for the remote
Workflow client, public gateway context/error behavior, unreadable-`401`
renewal, post-dispatch cancellation, selected local dependencies, fallback
codec and ingress, served OpenAPI, and the Python retained-client journey, plus
format, lint, codegen, typing, and client/PyO3 boundary lanes. The current
source is consistent with those recorded claims. Per the standing direction,
this review did not execute any of them; a missing, unclear, or source-
contradicted record would have been a finding, but none was found for the
maintainer scope.

## Overall result

**PASS**

The cumulative candidate is findable, owner-centered, documented, and safe for
a maintainer to follow and change. The prior maintainer and source-contract
defects are closed without adding unestablished machinery.
