# TASK-003 behavior review

## Review findings

### Critical

None.

### Important

None. The cumulative candidate satisfies the original task's behavioral
obligations, and the R1/R2 corrections close the prior behavior findings.

### Suggestions

None. This acceptance review does not prescribe optional work.

## Subject and review boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `0732ed92fc2907cb806ac918b09abbf6d44ff04f`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 12
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior remediation:
  `changes/active/skald-workflow-runtime/review/TASK-003-r1/TASK-003-R1-preserve-client-context-and-native-call-safety.md`
  and
  `changes/active/skald-workflow-runtime/review/TASK-003-r2/TASK-003-R2-close-renewal-proof-and-source-contracts.md`

The human-approved 2026-10-03 addition of `pub model: ModelRef` to
`WyrdGatewayCall` and the same-spec R1 correction that a native `401` renews
the refused credential without replaying the model POST are authoritative.
Neither is drift.

I reviewed the complete base-to-candidate range and independently traced the
remote Workflow handle, Skald route planning, public gateway caller,
authenticated native transport, fallback codec and ingresses, shared local
dependency composition, language wrappers, and focused proof. The candidate
remained at the stated commit throughout this review.

This was a strict source-only review. I did not build, compile, run tests, run
Cargo or Mise, or execute another verification command. Verification entries
below are the implementer's recorded results, checked against the source they
purport to exercise.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Recorded verification evidence | Result |
|---|---|---|---|
| REQ-024/031/045/046 and AC-019: one shared Rust remote handle exposes create/get/cancel/wait, direct snapshots, a stable create idempotency key, one-second polling, terminal failures as values, and drop-without-cancel behavior | `crates/shared/wyrd-client/src/workflow/remote.rs:23-121`; shared and Rust SDK exports in `crates/shared/wyrd-client/src/lib.rs:32-40` and `sdks/wyrd-sdk-rust/src/lib.rs:27-48` | `shared_workflow_client_contract` records first acceptance/replay, stable retry key, exact routes, terminal statuses, error propagation, and dropped polling; the task records the shared and Rust SDK lanes passing | PASS |
| REQ-036A/038/039/043 and the approved model amendment: every local WyrdGateway attempt carries the Prompt-derived model and immutable fallback, remaining deadline, cancellation, and trace-only correlation; supported public dialects dispatch and Vertex refuses before IO | `crates/skald/skald-workflow/src/route.rs:281-309,543-590`; `crates/shared/wyrd-client/src/workflow/gateway.rs:50-124,217-286` | Recorded Skald route proof and `public_gateway_call_context_and_errors` cover model carriage, OpenAI Chat/Responses, Anthropic, Gemini, Vertex refusal, timeout, cancellation, and concurrent fallback isolation | PASS |
| Corrected native-401 contract: one model POST; every observed `401` renews through the existing auth owner without replay; renewal failure is authoritative; successful renewal preserves the original refusal or existing body-read failure | `crates/shared/wyrd-client/src/transport/http.rs:351-410` performs one `send`, retains the body-read result, renews on the already-known status, then returns that result | Recorded focused cases cover complete-body renewal success, renewal failure, the renewed bearer on the next call, and a cut-off `401` body with exactly one model POST and one renewal | PASS — `FIND-TASK-003-2` closed |
| REQ-043 and INV-012: native refusal normalization keeps only safe status, code, optional approved OpenAI field, message, and remediation; recognized-code text comes from the Wyrd catalog and uncoded refusals use existing provider categories | `crates/shared/wyrd-client/src/workflow/gateway.rs:157-214,313-339` discards envelope messages and arbitrary details | Recorded three-dialect canary cases, uncoded 400/408/429/5xx cases, and recognized-code catalog comparisons | PASS — `FIND-TASK-003-3` remains closed |
| AC-011A: fallback production is unpadded base64url over JCS; authenticated OpenAI Chat/Responses, Anthropic, and Gemini ingresses decode, bound, deserialize, and semantically validate it; duplicate/malformed/oversized/empty/repeated/self-referential values dispatch nothing; absence preserves tenant policy; the header is not forwarded | `crates/wyrd-spec/src/gateway/policy.rs:88-184`; `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs:101-140,189-224,261-303,343-430`; `routes.rs:834-919,1329-1346,1767-1805` | Recorded policy and PG ingress proof covers producer bytes, limits and invalid values, authentication precedence, four public routes, typed assignment, omission, and non-forwarding | PASS. Receiver-side byte recanonicalization is not approved or ordinary JSON receiver behavior and is correctly absent rather than required as drift |
| AC-012: the served OpenAPI describes the optional header, encoding, limits, refusal code, and non-forwarding on every affected public operation | `FALLBACK_HEADER_DOC` at `ingress.rs:189-196` and the four route annotations | Recorded `gateway_inference_ingress_publishes_the_fallback_header` under the served OpenAPI integration lane | PASS |
| REQ-058 and INV-004/011/012: local dependency composition is shared; explicit native injection remains; route selection precedes secret resolution; only selected ExtGateway secrets resolve during a run; loading/apply dispatch nothing; public WyrdGateway uses the Workflow's Wyrd client | `crates/shared/wyrd-client/src/workflow/mod.rs:34-175`; `workflow/local.rs:30-145`; `global_config.rs:12-35`; `wyrd-utils/src/secret.rs:20-76` | Recorded selected-dependency proof covers Native, selected/unselected and unusable ExtGateway bindings, load-time behavior, and public WyrdGateway bearer/model projection; shared gateway/server evidence covers the reused secret reader | PASS |
| REQ-058/INV-007: Rust, Python, and TypeScript retain the shared Workflow owner and originating Cards/external-ref client through loading and local execution; Python mutations preserve that context | Shared owner retention at `wyrd-client/src/workflow/mod.rs:34-42,75-95,127-154,248-275`; Python wrapper at `sdks/wyrd-sdk-python/src/workflow.rs:204-215,305-435,537-580` and Cards handoff at `state/mod.rs:2635-2638`; TypeScript stores `wyrd_client::Workflow` in `sdks/wyrd-sdk-ts/native/src/workflow.rs:21-26,134-159` | Recorded Python public journey uses explicit server A with ambient server B/absence, registered and authored-ref loading, successful/refused mutation, and client-less local refusal; existing TypeScript/Rust journeys exercise the same shared owner | PASS — `FIND-TASK-003-1` remains closed |
| Shared env/file secret reading preserves prior gateway/server behavior while exposing only a redacted cross-module value | `crates/shared/wyrd-utils/src/secret.rs:23-76`; plaintext file stage is private; gateway, server config, and local Workflow consume `read_secret_ref` | Recorded gateway, server config, and shared-client evidence; source confirms the public result is `SecretString` and prior checks remain in the single owner | PASS — `FIND-TASK-003-4` remains closed |
| R2 source-contract corrections do not alter behavior: changed declarations use imported types; identified panic contracts and durable cancellation/partial-progress contracts are present | Imports and rustdoc in `transport/http.rs`, `workflow/{gateway,local,mod,remote}.rs`, `workflow_transport.rs`, `skald-workflow/src/route.rs`, `gateway/policy.rs`, server PG tests, and Python workflow wrapper | Recorded ordinary lint/static evidence; no bespoke check, setting, option, or harness was introduced | PASS — `FIND-TASK-003-6`, `-7`, and `-8` closed |
| Scenario 2 directly proves post-dispatch cancellation rather than only pre-cancel or timeout | Production race at `workflow/gateway.rs:96-116`; deterministic pending-socket case at `workflow_transport.rs:756-784` waits until `/v1/chat/completions` arrives, then cancels under a 300-second call deadline and observes no resend | Recorded exact `public_gateway_call_context_and_errors` selector passes after the added case | PASS — `FIND-TASK-003-9` closed |
| Non-goals and drift boundary: no new ingress, Vertex endpoint, credential mutation, provider-body Workflow fields, polling/retry option, public arbitrary-header API, duplicate transport/parser/graph/executor, Python/TypeScript remote lifecycle, or unsupported check/mechanism/file/setting | Complete cumulative diff and current callers; the native transport method is crate-private and composes existing auth/pool owners; the raw HTTP fixture is an ordinary focused test facility and not production machinery | Source review plus the recorded client-tier, SDK-tier, PyO3, codegen, format, and lint evidence | PASS |
| AC-013 and the original/R1/R2 tasks have credible focused closure proof | Original task evidence plus R1 Python context journey and R2 native-401/post-dispatch cancellation cases directly exercise every prior behavioral gap | Implementer recorded the exact focused selectors and the narrow shared, gateway, server, language, contract, and boundary lanes; source is consistent with those claims | PASS |

## Prior-finding closure

| Finding | Closure assessment |
|---|---|
| `FIND-TASK-003-1` | Closed. Python retains `wyrd_client::Workflow`, both loading paths preserve the originating client, authoring edits mutate only the inner Skald value, and the recorded journey distinguishes that client from ambient configuration. |
| `FIND-TASK-003-2` | Closed. The transport sends one model POST, renews after every known `401` even when body collection fails, never replays the POST, and records direct complete-body, failed-renewal, and cut-off-body proof. |
| `FIND-TASK-003-3` | Closed. Recognized codes take catalog title/remediation and discard provider-controlled message text. |
| `FIND-TASK-003-4` | Closed. Only the redacted `SecretString` reader is public; the plaintext file stage is private. |
| `FIND-TASK-003-5` | Closed. The prior function-local imports remain in their owning module blocks. |
| `FIND-TASK-003-6` | Closed. R2's cited changed declarations now use module-imported bare or role-specific aliased types. |
| `FIND-TASK-003-7` | Closed. The cited new panic-capable tests and helpers document their actual panic invariants. |
| `FIND-TASK-003-8` | Closed. Remote create/cancel, native POST, the public caller, and shared run path document cancellation and partial progress at their real ownership boundaries. |
| `FIND-TASK-003-9` | Closed. The existing focused selector now cancels only after the model POST reaches the boundary, returns through cancellation far before its deadline, and observes no resend. |

## Open questions

None.

## Verification notes

- No command that builds, compiles, tests, formats, lints, lists tests, or runs
  a verification lane was executed by this reviewer.
- The implementer records passing focused selectors for the remote client,
  public gateway context/errors, selected local dependencies, fallback policy,
  public ingress, and Python client-context journey, plus the relevant shared,
  SDK, gateway, served-OpenAPI, codegen, typing, boundary, format, and lint
  lanes.
- Source inspection found those records consistent with the candidate. No
  required behavioral evidence is missing, unclear, or contradicted by source.

## Overall result

**PASS**

The independently proposed finding ledger is empty.
