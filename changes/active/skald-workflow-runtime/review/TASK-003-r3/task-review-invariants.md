# TASK-003 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `0732ed92fc2907cb806ac918b09abbf6d44ff04f`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12, including the human-approved 2026-10-03 amendment adding `pub model: ModelRef` to `WyrdGatewayCall`
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior reviews and ledgers: `changes/active/skald-workflow-runtime/review/TASK-003-r1/` and `changes/active/skald-workflow-runtime/review/TASK-003-r2/`
- Remediation tasks: `TASK-003-R1-preserve-client-context-and-native-call-safety.md` and `TASK-003-R2-close-renewal-proof-and-source-contracts.md`, including the human-approved same-spec native-`401` wording correction

The complete base-to-candidate range, both prior finding ledgers, both remediation tasks, current source, callers, sibling consumers, and recorded implementation evidence were reviewed. No build, compile, test, Cargo, mise, pnpm, pytest, formatter, linter, package-manager, or other verification command was run. The candidate remained unchanged while this report was prepared.

## Producer-to-sink invariant trace

- Remote run values flow from `Workflows::{create,get,cancel,wait}` through the existing shared `WyrdClient` transport. Create delegates to the existing idempotent submission owner, get/cancel use the direct run routes, and wait performs only GET plus the fixed one-second delay. The new cancellation documentation accurately states that dropping a client future does not roll back a server-side acceptance or cancellation.
- The Prompt's resolved provider/model flows through `WorkflowExecutionDependencies::resolve_route` into `StepRoute::WyrdGateway`, then into the per-attempt `WyrdGatewayProvider` and the human-approved `WyrdGatewayCall.model`. The public caller projects that typed model into the existing OpenAI Chat, OpenAI Responses, Anthropic, or Gemini ingress; Vertex is refused before IO.
- Fallback, remaining deadline, cancellation, and correlation remain fresh per-call values. `PublicWyrdGatewayCaller::call` constructs a request-local fallback header, races the complete authenticated POST against the supplied cancellation token and timeout, and emits correlation only as trace fields. No call context enters shared registry or transport state.
- The fallback header reaches only authenticated OpenAI Chat/Responses, Anthropic, and Gemini handlers. The ingress decodes, bounds, deserializes, and validates it against the requested model after authentication, then places only the typed override in `GatewayCallRequest.fallback`; provider request construction has no caller-header forwarding path.
- Local dependency preparation first computes the resolved route set. Only selected `ExtGateway` bindings resolve `SecretRef` values, while a selected public Wyrd gateway route uses the client retained by the loaded shared `Workflow` or lazily creates the ambient client for a client-less local workflow. Loading and authoring mutations do not resolve execution secrets or dispatch calls.
- Python retains the complete `wyrd_client::Workflow` through registered loading, authored external-ref loading, mutations, and `run`; TypeScript and Rust likewise hold the shared facade. The loading client's endpoint, credential, token cache, and connection pool therefore remain the authority for later public gateway calls.
- Native refusal bodies are normalized at the shared-client boundary. Recognized Wyrd codes take catalog title/remediation; uncoded and unknown refusals take the existing provider status category; only the approved optional OpenAI `param` survives as `field`. Provider body messages and arbitrary details do not enter portable Workflow errors.
- A native model request is sent once. `HttpTransport::post_native` captures status, retains the body-read result, and runs `AuthMiddleware::force_refresh` for every completed `401` status before returning either the original body or its body-read error. Renewal failure remains authoritative. JSON, framed, gRPC, and ordinary gateway relay siblings are unchanged.
- The R2 proof reaches the two formerly unproved states: the cut-off-body case observes `/auth/token`, one `/v1/chat/completions`, and one renewal `/auth/token`; the cancellation case waits until the model POST reaches the loopback boundary before cancelling a call whose own deadline is 300 seconds. Both extend the existing focused selector with ordinary repository-local loopback HTTP patterns and add no production mechanism, option, setting, check, or dependency.

## Prior-finding closure

| Prior finding | Source closure | Recorded proof | Result |
|---|---|---|---|
| `FIND-TASK-003-1` | `PyWorkflow.inner` is `ClientWorkflow`; both load paths preserve it; mutations use `as_skald_mut`; `run` delegates to the retained owner | Recorded Python integration journey distinguishes loading server A from ambient server B/no ambient client and covers successful and refused mutations | PASS |
| `FIND-TASK-003-2` | `post_native` emits one POST, retains the body-read result, calls `force_refresh` after every observed `401`, gives renewal failure precedence, and never replays the model call (`transport/http.rs:376-410`) | Recorded focused selector covers complete-body success, renewal failure, next-call bearer replacement, and cut-off-body renewal with exactly one model POST | PASS |
| `FIND-TASK-003-3` | `Ingress::problem` discards native message text; recognized codes use derive-backed catalog metadata and uncoded/unknown refusals use fixed provider-category metadata (`workflow/gateway.rs:157-213`) | Recorded three-dialect recognized-code canaries and uncoded status cases | PASS |
| `FIND-TASK-003-4` | Plaintext file reading is private; cross-module consumers receive `SecretString` only through `read_secret_ref`; existing file checks remain in one owner (`wyrd-utils/src/secret.rs:23-76`) | Recorded shared/gateway/server evidence plus source inspection | PASS |
| `FIND-TASK-003-5` | The three cited function-local imports remain removed and imports are at their owning module/test-module boundary | Recorded lint evidence plus source inspection | PASS |
| `FIND-TASK-003-6` | R2-cited declarations now use module-imported bare or role-specific aliased types, including `Bytes`, `LoadedTree`, `Prompt`, request/response test types, and `ClientWorkflow`/`SkaldWorkflow` | Recorded lint evidence and the implementation's cumulative source sweep | PASS |
| `FIND-TASK-003-7` | The R2-identified panic-capable tests/helpers now document their fixture, parsing, recording, or assertion invariant with `# Panics`; no production mechanism was introduced | Recorded static/lint evidence plus source inspection | PASS |
| `FIND-TASK-003-8` | Remote create/cancel, native POST, public gateway call, and shared local run document cancellation and partial progress at the actual owner (`workflow/remote.rs:41-57,78-90`; `transport/http.rs:351-375`; `workflow/gateway.rs:52-67`; `workflow/mod.rs:106-126`) | Recorded static/lint evidence plus source inspection | PASS |
| `FIND-TASK-003-9` | The focused test waits for `/v1/chat/completions`, then cancels the supplied token while the response is pending and distinguishes cancellation from the 300-second call deadline (`workflow_transport.rs:756-784`) | Recorded exact focused selector passed and reported no second model request | PASS |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-031/045/046 and AC-019: one shared create/get/cancel/wait handle, direct snapshots, one create idempotency key across retry, one-second polling, terminal values, and drop without server cancel/resubmit | `crates/shared/wyrd-client/src/workflow/remote.rs:23-121`; existing `submit_idempotent` and JSON transport owners | Recorded `shared_workflow_client_contract`, shared family, and Rust SDK evidence | PASS |
| Rust SDK projects the shared handles without Python activation; INV-007 | Shared exports in `wyrd-client/src/lib.rs` and thin Rust SDK re-export | Recorded SDK, client-tier, SDK-tier, and PyO3-scope checks | PASS |
| Approved `WyrdGatewayCall.model` comes from the resolved Prompt and remains immutable per attempt | `skald-workflow/src/route.rs:62-75,281-309,543-590` | Recorded Skald route/attempt evidence | PASS |
| REQ-036A/038/039/043 and INV-020: model, fallback, remaining deadline, cancellation, and correlation remain per-call; supported dialects use existing ingress; Vertex refuses before IO | `wyrd-client/src/workflow/gateway.rs:50-124,217-286`; Skald per-attempt adapter | Recorded `public_gateway_call_context_and_errors`, including direct post-dispatch cancellation | PASS |
| Corrected native-`401` contract: one model POST; every completed `401` status renews through the existing auth owner even when its body is unreadable; renewal failure is authoritative; no sibling retry change | `wyrd-client/src/transport/http.rs:376-410`; sole caller at `workflow/gateway.rs:96-116`; existing `AuthMiddleware::force_refresh` owner | Recorded complete-body, failed-renewal, next-call-bearer, and truncated-body cases; source agrees with all claims | PASS |
| REQ-043/INV-012: only safe common refusal fields survive and recognized text comes from trusted catalog metadata | `wyrd-client/src/workflow/gateway.rs:157-213,313-339`; Workflow projection retains the bounded `RemoteProblem` | Recorded OpenAI/Anthropic/Google canaries and uncoded category cases | PASS |
| AC-011A: authentication precedes fallback interpretation; exact limits/encoding/semantic validation; typed assignment; no provider forwarding; absent header keeps tenant policy | Authentication middleware and handlers at `wyrd-server/src/components/gateway/ingress.rs:101-169,189-224,261-303,343-435`; OpenAI routes consume the same helper; codec at `wyrd-spec/src/gateway/policy.rs:88-184` | Recorded authenticated PG ingress and policy-codec evidence | PASS |
| AC-012: served OpenAPI documents the optional header, encoding, limits, refusal code, and non-forwarding on all affected operations | `FALLBACK_HEADER_DOC` and four route parameter declarations; served contract test | Recorded `test:principals:integration` and codegen evidence | PASS |
| REQ-058, INV-004/011/012, AC-029/031: shared local dependency composition, selected-only secret resolution at run, no load/apply dispatch, explicit native injection, and retained Cards context | `wyrd-client/src/workflow/mod.rs:34-175,229-275`; `workflow/local.rs:30-146`; `global_config.rs:12-35`; shared secret owner | Recorded selected-dependency/shared evidence and public Python client-context journey; TypeScript/Rust source delegates to the same facade | PASS |
| Secret material stays behind the shared redacted reader and existing gateway/server/client consumers | `wyrd-utils/src/secret.rs:23-76`; all three cross-module consumers call `read_secret_ref` | Recorded shared/gateway/server evidence; source inspection | PASS |
| AC-013 proof directly exercises the task-owned transport, cancellation, ingress, context, and contract states and is not contradicted by source | Existing focused transport selector now covers cut-off `401` renewal and post-dispatch cancellation; prior focused/PG/OpenAPI/Python evidence remains source-aligned | Implementer's recorded exact focused, family, integration, codegen, typing, formatting, lint, and boundary results | PASS |
| Non-goals: no new ingress, Vertex public endpoint, credential mutation API, provider-body Workflow context, polling/retry option, arbitrary-header public API, remote Python/TypeScript lifecycle, duplicate graph/executor/transport, receiver-side JCS equality, or bespoke enforcement mechanism | Complete cumulative diff and producer/caller inspection | Static source evidence | PASS |

## Proposed findings

None. The cumulative candidate satisfies the original task and both remediation tasks under the approved amendments. No missing, incorrect, drifting, violating, or regressing invariant was established.

## Verification notes

This review was strictly source-only as directed. It ran no builds, tests, verification lanes, package managers, formatters, or linters. The implementer records successful exact focused tests for the shared lifecycle and public gateway caller, including the R2 RED/GREEN cut-off-`401` case and post-dispatch cancellation case; relevant shared, Skald, gateway, server PG/OpenAPI, Python context, codegen, typing, formatting/lint, and boundary checks are also recorded. Source inspection agrees with the recorded exercised paths. No missing, unclear, or source-contradicted evidence remains for an invariant assigned to TASK-003.

## Overall result

**PASS**

The validated invariant ledger is empty. Prior `FIND-TASK-003-1` through `FIND-TASK-003-9` are closed, including the reopened native-`401` renewal invariant, and no new finding is proposed.
