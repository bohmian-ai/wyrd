# TASK-003 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `2e6f13f505daf19050e588ea0d1d7fc936697e27`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12, including the human-approved 2026-10-03 amendment adding `pub model: ModelRef` to `WyrdGatewayCall`
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior review and ledger: `changes/active/skald-workflow-runtime/review/TASK-003-r1/{verdict.md,findings-validation.md}`
- Remediation task: `changes/active/skald-workflow-runtime/review/TASK-003-r1/TASK-003-R1-preserve-client-context-and-native-call-safety.md`, including the human-approved same-spec native-401 wording correction

The cumulative base-to-candidate source and the remediation delta were reviewed. No build, compile, test, Cargo, mise, pnpm, pytest, package-manager, or other verification command was run. Recorded implementation evidence was treated as a claim and checked against source.

## Producer-to-sink invariant trace

- Remote lifecycle values flow from `Workflows::{create,get,cancel,wait}` through the existing shared `WyrdClient` transport. Create uses the existing idempotent submission owner; wait only performs GET plus the fixed one-second delay and has no cancellation or resubmission side effect.
- Resolved Prompt provider/model flows through `WorkflowExecutionDependencies::resolve_route` into `StepRoute::WyrdGateway.model`, then into the per-attempt adapter and `WyrdGatewayCall.model`. `PublicWyrdGatewayCaller` projects that typed model into the existing OpenAI, Anthropic, or Gemini ingress; Vertex is refused before IO.
- Fallback, remaining deadline, cancellation, and correlation remain per-call values. The caller builds one request-local fallback header, races the native POST with the supplied cancellation token and timeout, and emits correlation only on the tracing span.
- The public fallback header reaches authenticated OpenAI Chat/Responses, Anthropic, and Gemini handlers, is decoded against the requested model, and enters only `GatewayCallRequest.fallback`. Provider request construction consumes the typed call body rather than caller headers.
- Selected local routes are collected before dependency preparation. Only selected external bindings resolve `SecretRef` values, while a `wyrd_gateway` route uses the client retained by the loaded `wyrd_client::Workflow` or lazily constructs the ambient client for a client-less local workflow.
- Python now retains that complete shared Workflow through registered loading, authored external-ref loading, authoring mutations, and `run`; sibling TypeScript and Rust already retain the same owner.
- Native refusal bodies are normalized at `workflow/gateway.rs`: recognized codes take catalog title/remediation, uncoded or unknown codes take the existing provider category, and only the optional OpenAI field survives into `RemoteProblem` and then `WorkflowRunError`.
- A native model POST is emitted once. After a complete 401 response body, `post_native` invokes `AuthMiddleware::force_refresh`, propagates refresh failure, and otherwise returns the original status/body. If reading that 401 body fails, however, the early `?` returns before renewal.

## Prior-finding closure

| Prior finding | Source closure | Recorded proof | Result |
|---|---|---|---|
| FIND-TASK-003-1 | `PyWorkflow.inner` is now `wyrd_client::Workflow`; registered and authored-ref loads preserve it; authoring methods replace only `as_skald_mut`; run calls the retained owner | Recorded Python integration journey covers explicit Cards context A against ambient B/absence, authored external refs, successful and refused mutation, and a client-less local workflow | PASS |
| FIND-TASK-003-2 | The resend loop is gone and the model body is sent once. Complete 401 bodies trigger existing `force_refresh`; refresh failure propagates and successful renewal leaves the original refusal intact. The body-read failure path bypasses renewal at `transport/http.rs:393-399` | Recorded focused proof covers uncoded and known-code 401, one model POST, successful renewal, next-call bearer, and renewal failure, but only with complete response bodies | **FAIL — INV-R2-001** |
| FIND-TASK-003-3 | Native envelope message text is no longer read; recognized codes use derive-backed catalog title/remediation and unknown/uncoded bodies use fixed category text | Recorded focused proof covers OpenAI, Anthropic, and Google recognized-code canaries; source shows the resulting `RemoteProblem` is projected unchanged except for the permitted field | PASS |
| FIND-TASK-003-4 | `read_secret_file` is private; all cross-module consumers use `read_secret_ref -> SecretString`; the open-handle, regular-file, permissions, UTF-8, and size logic is unchanged | Recorded shared/gateway coverage plus source/API inspection | PASS |
| FIND-TASK-003-5 | All three cited imports now live in their owning production or test module import blocks with Unix cfg preserved | Recorded focused/lint evidence plus source inspection | PASS |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-031/045/046: one shared create/get/cancel/wait handle, stable create idempotency, fixed one-second polling, terminal snapshots returned, dropped wait has no cancel/resubmit | `crates/shared/wyrd-client/src/workflow/remote.rs:23-109`; existing shared transport | Recorded `shared_workflow_client_contract` and `test:shared` | PASS |
| Rust SDK projects the same handles without Python activation | `crates/shared/wyrd-client/src/lib.rs`; `sdks/wyrd-sdk-rust/src/lib.rs` | Recorded SDK, client-tier, SDK-tier, and PyO3-scope evidence | PASS |
| Human-approved `WyrdGatewayCall.model` is derived from the resolved Prompt and remains immutable per attempt | `crates/skald/skald-workflow/src/route.rs:62-94,281-309,395-427,464-481`; call construction at `:543-590` | Recorded Skald/shared verification from the original implementation evidence | PASS |
| REQ-036A/038/039 and INV-020: model/fallback/deadline/cancellation/correlation remain per-call; supported public dialects use existing ingress; Vertex refuses before IO | `crates/shared/wyrd-client/src/workflow/gateway.rs:50-123,212-281`; Skald per-attempt adapter | Recorded `public_gateway_call_context_and_errors` | PASS |
| Native POST is never replayed, and a server refusal renews the existing durable credential for later calls without changing sibling transports | One request is built and sent at `transport/http.rs:373-392`; sibling JSON/framed/gRPC retry owners are unchanged. Renewal is ordered after fallible body collection at `:393-399` | Recorded 401 cases prove complete-body behavior only; source contradicts unconditional refusal renewal when the 401 body is truncated or otherwise fails to read | **FAIL — INV-R2-001** |
| REQ-043/INV-012: only safe common refusal fields survive, recognized messages use trusted catalog metadata, and ordinary native relay stays unchanged | `crates/shared/wyrd-client/src/workflow/gateway.rs:155-212`; `skald-workflow/src/attempt.rs:164-184`; no gateway relay edit in remediation | Recorded three-dialect recognized-code canaries and uncoded category cases | PASS |
| AC-011A: authenticated OpenAI Chat/Responses, Anthropic, and Gemini ingress decodes and validates the exact fallback header, assigns only typed fallback, refuses invalid values before dispatch, and never forwards the header | `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs:101-169,189-224,261-303,343-435`; `routes.rs:834-913,1332-1346,1769-1805`; codec in `wyrd-spec/src/gateway/policy.rs:88-184` | Recorded authenticated PG ingress and codec evidence | PASS |
| AC-012: served OpenAPI documents the header encoding, limits, and refusal on all four affected operations | `FALLBACK_HEADER_DOC` and four `utoipa::path` parameter declarations; served contract test at `pg_openapi_contract.rs:809-855` | Recorded `test:principals:integration` result | PASS |
| REQ-058/AC-029/031: shared local configuration selects routes first, resolves only selected secrets at run time, preserves explicit native injection, and performs no load/apply dispatch | `workflow/local.rs:29-144`; `workflow/mod.rs:74-168`; `global_config.rs:16-35`; `wyrd-utils/src/secret.rs:21-76` | Recorded selected-dependency, shared, Python, and boundary evidence | PASS |
| Registered and authored external-ref Workflows retain their loading client through Python mutations and run; wholly local workflows remain ambient/client-less as appropriate | `sdks/wyrd-sdk-python/src/workflow.rs:203-218,298-579`; `state/mod.rs:2607-2638`; shared owner at `workflow/mod.rs:29-168,226-274` | Recorded Python integration journey directly distinguishes client A, ambient B, and no ambient client | PASS |
| Shared secret reading exposes no plaintext cross-module result and keeps existing consumer behavior | `crates/shared/wyrd-utils/src/secret.rs:21-76`; gateway/server/client callers use `read_secret_ref` | Recorded focused gateway and shared suites; source inspection | PASS |
| Mandatory import placement is restored without a new checker, feature, allowlist, or setting | `wyrd-utils/src/secret.rs:12-17`; `wyrd-client/src/workflow/mod.rs` test imports; `wyrd-spec/src/gateway/policy.rs` test imports | Recorded focused and lint evidence; source inspection | PASS |
| Non-goals: no new ingress, Vertex endpoint, credential mutation API, provider-body Workflow context, polling/retry setting, public arbitrary-header API, language remote lifecycle, alternate graph/executor/transport, or bespoke check | Complete cumulative diff and caller/source inspection | Static evidence | PASS |
| AC-013: recorded evidence directly covers the changed failure behavior and is not contradicted by source | Evidence covers normal and refusal bodies, context retention, code normalization, visibility, imports, family lanes, codegen, typing, and boundaries; it does not cover the source-visible truncated-401 renewal gap | Review was source-only by standing direction | **FAIL — INV-R2-001** |

## Proposed finding

### INV-R2-001 — INCORRECT: a 401 with an unreadable body bypasses required credential renewal

- **Violated obligation:** The approved R1 correction requires the native call to be sent once and, when its response is 401, to renew through `AuthMiddleware::force_refresh`, propagate renewal failure, and otherwise return the original refusal. The stronger client architecture likewise requires an API-key-backed SDK to re-exchange its durable key when the server refuses its access token (`architecture/wyrd-design.md:194-196,554-557`).
- **Exact location:** `crates/shared/wyrd-client/src/transport/http.rs:393-399`.
- **Producer-to-sink evidence:** `request.send()` has already completed and exposes `StatusCode::UNAUTHORIZED` at line 393. Line 394 then consumes the body with `response.bytes().await?`. A truncated response, connection reset after headers, or other body-read failure returns `WyrdError::Internal` immediately, so lines 395-399 never call the existing auth owner. `PublicWyrdGatewayCaller::call` maps that internal error to `ProviderError::Connect`; the cached refused access token remains installed, and the next model call can present it again. This path sends no duplicate model POST, but it fails the required reactive renewal invariant.
- **Recorded-evidence conflict:** `public_gateway_call_context_and_errors` uses complete JSON 401 bodies. Its green result proves send-once, successful renewal, next-call bearer, and renewal-failure behavior only after body collection succeeds; it cannot establish the task's unconditional 401 renewal claim.
- **Observable consequence:** after an edge-authentication 401 whose body is interrupted, an API-key, workload, renewable, or delegated client reports a transport failure but does not refresh the refused credential. A subsequent local Workflow gateway call can reuse the same stale token instead of the renewed cache required by the client architecture.
- **Required testable correction:** Preserve the single model POST and the original owners. Once the status is known to be 401, ensure the existing `AuthMiddleware::force_refresh` path is attempted even if collecting the native envelope body fails. A renewal error remains authoritative; after successful renewal, return the original status/body when body collection succeeded or the existing body-read transport error when it did not. Do not replay the model POST, change sibling JSON/framed/gRPC retry behavior, add provenance, add configuration, or introduce a new mechanism.
- **Focused closure proof:** Reuse the repository's established raw HTTP fixture pattern to return 401 headers with a truncated body. Prove one model POST, one reactive token renewal, no resend, and the existing body-read/connection failure result. Keep the already recorded complete-body success and renewal-failure cases. No new dependency, generalized harness, setting, option, or check is warranted.

## Verification notes

No commands that build, compile, execute tests, or run verification lanes were used in this review. The implementer's recorded evidence reports green focused Rust tests, the Python context journey, shared and Python families, formatting/lints, codegen, typing, and boundary checks. That evidence is credible for the states its source tests exercise. It does not cover the reachable body-read failure between receipt of a 401 status and `force_refresh`, and source contradicts the recorded blanket claim that every 401 renews.

## Overall result

**FAIL**

FIND-TASK-003-1, -3, -4, and -5 are closed. FIND-TASK-003-2 is only partially closed: duplicate model dispatch is removed and complete 401 responses use the approved renewal owner, but a 401 whose body cannot be collected still bypasses the required re-exchange. The remaining correction is bounded to ordering/error preservation in the existing native transport and requires no new public or architectural decision.
