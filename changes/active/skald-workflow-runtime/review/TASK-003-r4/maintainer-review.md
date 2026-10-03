# Maintainer Review — TASK-003 r4

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `cfd60f6442e4dbdfcbc57cfec0bde494a225ae89`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12,
  including the human-approved 2026-10-03 minimal amendment adding
  `pub model: ModelRef` to `WyrdGatewayCall`
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Remediations: `TASK-003-R1-preserve-client-context-and-native-call-safety`,
  `TASK-003-R2-close-renewal-proof-and-source-contracts`, and
  `TASK-003-R3-close-pending-renewal-and-async-context-contracts`
- Human approvals retained: the R1 same-spec correction for native `401`
  handling and the approved `spawn_blocking` boundary for run-start
  config/client/secret reads

The candidate remained at the stated commit throughout this review. This was a
strictly read-only source review. I did not build, compile, run tests or lanes,
or invoke Cargo, mise, pnpm, pytest, a formatter, a linter, or another
verification command. The implementer's recorded evidence was inspected but
not rerun.

## Changed-surface coverage

| Surface | Owning symbols and source inspected | Callers, consumers, and recorded proof inspected | Maintainer assessment |
|---|---|---|---|
| Shared remote run client | `workflow/remote.rs::Workflows::{new,create,get,cancel,wait}`, `WyrdClient::{submit_idempotent,request_json}`, root exports | `shared_workflow_client_contract`, Rust SDK export projection | One dependency-owning handle exposes the lifecycle directly. Method names, direct DTO results, idempotency, polling, and cancellation/partial-progress docs are findable at their owners. |
| Public governed gateway caller | `workflow/gateway.rs::PublicWyrdGatewayCaller`, `NativeCall`, `Ingress`, error projection helpers, `HttpTransport::post_native`, `AuthMiddleware::force_refresh` | `WyrdGatewayCaller`, Skald attempt adapter, all call constructors, `public_gateway_call_context_and_errors` including the recorded pending-body case | Protocol projection, raw authenticated send, and refusal normalization are focused stages. The native send-once rule and renewal-before-body ordering are explicit at the transport owner, and the auth-owner docs distinguish retry-safe callers from this no-replay caller. |
| Shared local Workflow facade | `workflow/mod.rs::Workflow`, `WorkflowCards`, `load_bundle`, `blocking_task_failed`, `load_local_setup`; `local.rs::SelectedRoutes`, `resolve_binding`; `GlobalConfig` and `ClientConfig` constructors | Rust, Python, and TypeScript load/run consumers; CLI-facing shared surface; selected-route and retained-client recorded evidence | Loading, Cards hydration, and Skald execution remain with existing owners. The R3 blocking boundary is placed at run-start as approved. `load_local_setup`, however, does not use the one ambient snapshot required by R3; see `MAINT-TASK-003-R4-1`. |
| Shared secret boundary | `wyrd_utils::secret::{read_secret_ref,read_secret_file,restrictive}`, gateway `read_binding`, server managed-key loading | Gateway credential resolution, server boot configuration, local Workflow binding resolution, existing source tests/evidence | The shared reader consolidates the established open-handle, bounded, redacted rule. Async callers retain the blocking boundary; no setting, backend, feature, or compatibility path was added. |
| Skald gateway seam | `WyrdGatewayCall`, `WorkflowExecutionDependencies::resolve_route`, `StepRoute`, `gateway_model`, `WyrdGatewayProvider::send`, execution-plan call site | Route/executor tests and shared-client caller | The approved `model` field is derived from the Prompt owner and carried through immutable attempt state. Skald remains independent of Wyrd client/server IO. |
| Fallback contract and public ingress | `GatewayFallbackOverride::{to_header_value,from_header_value}`, constants, `requested_fallback`, OpenAI/Anthropic/Gemini handlers, `openai_call`, invocation lowering | Fallback contract test, `public_ingress_workflow_fallback`, served OpenAPI contract evidence | Pure encoding and validation stay in `wyrd-spec`; authenticated ingress owns extraction; invocation receives only the typed value. Shared route documentation avoids divergent protocol descriptions without adding a general header API. |
| Python and Rust SDK projections | `PyWorkflow`, authoring mutations, `from_path`, `run`, Cards load handoff; role aliases; Rust root exports | Python retained-client integration evidence, TypeScript sibling source, Rust export test | Python holds the complete shared Workflow and mutates only its Skald value, preserving endpoint, credential, pool, and token-cache context. Public parameter/return shapes are unchanged, so no generated declaration edit is expected. Rust projects the shared exports directly. |
| Tests, manifests, and documentation | Changed Rust test helpers/selectors, Python integration journey, manifests, public rustdoc, prior remediation records | Recorded focused selectors and format/lint/codegen/boundary results | Tests use existing unit, raw HTTP, Postgres, OpenAPI, and language-runtime patterns. The pending-response raw fixture is a standard protocol test technique for behavior Wiremock cannot express, not unsupported DRIFT. Panic, cancellation, and ownership-transition contracts are present on the cited remediated items. The recorded evidence does not establish the one-snapshot R3 requirement and is contradicted by source. |

## Prior-finding closure

| Prior finding | Maintainer closure assessment |
|---|---|
| `FIND-TASK-003-1` | Closed. Python retains `ClientWorkflow` through loading, authoring mutation, and execution. |
| `FIND-TASK-003-2` | Closed in source for the R3 diagnosis. `post_native` renews immediately after reading a `401` status and before body collection, without replaying the model POST. |
| `FIND-TASK-003-3` | Closed. Recognized refusal codes take safe title and remediation from the error catalog. |
| `FIND-TASK-003-4` | Closed. The shared file reader is private below the public `SecretString`-returning boundary. |
| `FIND-TASK-003-5` | Closed. Imports reside in module import blocks. |
| `FIND-TASK-003-6` | Closed. Changed declarations use imported bare names or role-specific aliases. |
| `FIND-TASK-003-7` | Closed. The cited panic-capable helpers and tests have substantive panic contracts. |
| `FIND-TASK-003-8` | Closed. Remote mutations, native POST, public gateway calls, and shared local execution document cancellation and possible post-dispatch progress. |
| `FIND-TASK-003-9` | Closed by source and recorded evidence. Cancellation is triggered only after the existing loopback boundary observes dispatch, with one POST recorded. |
| `FIND-TASK-003-10` | Partially closed. Ambient setup runs on the approved blocking pool, but the implementation does not load and reuse one ambient `GlobalConfig` snapshot when both route families need it. See `MAINT-TASK-003-R4-1`. |
| `FIND-TASK-003-11` | Closed. `force_refresh` rustdoc distinguishes replay-safe transports from the native no-replay path. |
| `FIND-TASK-003-12` | Closed. `Workflow::into_skald` states exactly which retained client and automatic-composition context is discarded. |

## Material finding

### MAINT-TASK-003-R4-1 — Run setup has two ambient configuration owners instead of one snapshot

- **Classification:** `INCORRECT`
- **Changed locations:**
  - `crates/shared/wyrd-client/src/workflow/mod.rs:190-210`
  - `crates/shared/wyrd-client/src/client.rs:47-54`
  - `crates/shared/wyrd-client/src/config.rs:91-103`
- **Governing obligation:** TASK-003-R3's selected correction requires the
  shared Workflow preparation owner to "load one ambient `GlobalConfig`
  snapshot when selected routes require it" and reuse existing
  `GlobalConfig`, `ClientConfig`, and `WyrdClient` construction behavior for
  both Workflow bindings and a client-less public-gateway client. The
  maintainer guide requires one owner for shared state and a main path whose
  stages are explicit.
- **Evidence:** `load_local_setup` first calls `GlobalConfig::load()` at lines
  195-196 when an `ext_gateway` route selects configuration. When the same
  Workflow also selects `wyrd_gateway` and has no retained Cards client, lines
  200-208 call `WyrdClient::from_global()`. That constructor calls
  `ClientConfig::from_global()`, which calls `GlobalConfig::load()` again.
  Therefore one run-start setup can parse the same ambient file twice through
  two constructors. The implementation evidence reports that both operations
  moved onto the blocking pool, but neither the listed source evidence nor the
  recorded tests establish the required single snapshot.
- **Concrete maintenance cost and observable consequence:** the shared setup
  owner returns values that appear to form one run configuration while they
  can come from different reads. A config replacement between reads can pair
  external bindings from one file version with endpoint, tenant, or cache
  settings from another. A maintainer must also trace through
  `WyrdClient::from_global -> ClientConfig::from_global -> GlobalConfig::load`
  to discover the second filesystem boundary, defeating the helper's stated
  role as the complete synchronous setup stage.
- **Smallest testable correction:** in `load_local_setup`, load at most one
  `GlobalConfig` when either selected external bindings need it or a
  client-less selected Wyrd gateway needs it. Derive the returned
  `LocalWorkflowConfig` from that value and build the client with the existing
  `ClientConfig::from_global_with_env(&global)` plus
  `WyrdClient::with_config`; continue to reuse a retained Cards client without
  loading ambient client configuration. Preserve the approved `spawn_blocking`
  boundary, lazy route selection, error mapping, and selected-secret-only
  resolution. Closure needs static source proof that this path contains one
  `GlobalConfig::load`, plus the existing selected-local-dependencies and
  retained-client/language proofs. Do not add a timing fixture, watcher, cache,
  option, helper layer, check, or test harness.

## DRIFT assessment

No unsupported mechanism, setting, option, compatibility path, feature,
dependency, checker, allowlist, or public abstraction was found. `Workflows`,
`PublicWyrdGatewayCaller`, `SelectedRoutes`, the fallback codec, and the shared
secret reader have concrete owners and live consumers. The existing raw HTTP
fixture is comparable to ordinary Rust HTTP-client protocol tests and is
limited to response states the installed mock server cannot represent. The
finding above removes duplicate ambient reads by composing existing
constructors; it requires no novel mechanism.

## Verification assessment

The original task and R1-R3 records contain focused evidence for remote
lifecycle, public gateway context/errors and native-`401` renewal, selected
local dependencies, retained client context, fallback codec/ingress/OpenAPI,
and relevant formatting, lint, codegen, language, and boundary lanes. I did
not rerun any evidence under the standing read-only direction. The R3 record
for `FIND-TASK-003-10` is insufficient for the one-snapshot portion of its
selected correction and is contradicted by the call chain above, so it cannot
close that obligation.

## Overall result

**FAIL**

The cumulative implementation is otherwise owner-centered and documented, but
the latest remediation leaves two ambient config reads inside the one shared
run-setup workflow. Reusing the already-existing config-to-client constructors
is a bounded correction that restores one obvious owner and the approved
single-snapshot contract without adding machinery.
