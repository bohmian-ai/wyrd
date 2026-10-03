---
id: TASK-003-R1
kind: remediation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 12
requirements: [REQ-038, REQ-043, REQ-058, INV-007, INV-012, INV-020, AC-011A, AC-013, AC-019, AC-031]
parent_task: TASK-003
remediates: [FIND-TASK-003-1, FIND-TASK-003-2, FIND-TASK-003-3, FIND-TASK-003-4, FIND-TASK-003-5]
---

# Preserve client context and native call safety

Implementation skill: `$wyrd-implement`. Approved spec: `changes/active/skald-workflow-runtime/spec.md`, Revision 12 including the human-approved 2026-10-03 `WyrdGatewayCall.model` amendment. Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`. Review: `changes/active/skald-workflow-runtime/review/TASK-003-r1/verdict.md`; independent ledger: `findings-validation.md` in that directory.

Original base: `58d07d7260df1f022a721e720a28ea48e5096e35`. Reviewed candidate: `a1792c45323489157e818eb29722913114014927`. This task extends that candidate; it does not replace the original task's obligations.

## Diagnosis and selected corrections

### FIND-TASK-003-1: Python loses loaded authority context

Shared authored loading retains the client used for external refs (`wyrd-client/src/workflow/mod.rs:82-94`); registered loading retains the Cards client (`:257-263`). Python `state/mod.rs:2635-2638` and `workflow.rs:503-507` discard the facade with `into_skald()`. The changed run path (`workflow.rs:548-551`) wraps only Skald state; `From<SkaldWorkflow>` at shared `workflow/mod.rs:190-197` sets `client: None`. A public gateway step therefore uses ambient configuration instead of the explicit server/credential that loaded its graph. Rust and TypeScript keep the shared owner. The recorded shared Rust test and Python unit/type checks do not prove this handoff.

Retain the complete `wyrd_client::Workflow` through Python load/run lifetime and authoring mutations. Run through that owner's existing composition and shared runtime bridge. Builder/from-YAML values may enter it through the existing Skald conversion when no retained client exists. Preserve mutation success/failure behavior and connection context; do not duplicate client/config/bearer state at the Python boundary. This repairs the source of context loss for both registered and authored external-ref consumers.

### FIND-TASK-003-2: ambiguous 401 duplicates model work

`HttpTransport::post_native` at `transport/http.rs:365-401` retries every first 401, ignores refresh failure and sends the full POST again. Native provider status refusals were already dispatched and survive `wyrd-gateway/src/adapter/http.rs:400-422`, `adapter/mod.rs:258-308`, server `invocation.rs:553-559` and `routes.rs:1339-1360`. Thus one call can produce two governed invocations and provider requests/accounting/capture/audit effects. A fixed bearer can be resent unchanged. Existing refusal coverage omits 401.

Remove resend from this native model transport operation. Keep initial `AuthMiddleware::bearer`; when the response is 401, preserve reactive renewal through existing `force_refresh`, propagate its existing authentication error on renewal failure, and otherwise return the original status/body. Existing outer timeout/cancellation continues to bound this work. Architecture requires re-exchange on refusal, not same-call replay. Correct TASK-003's private refresh/retry wording through `$wyrd-plan` under Revision 12 if it implies resending this POST. The selected outcome is fixed; an implementer must not substitute code-gated replay, which remains spoofable, or invent provenance. Preserve sibling shared JSON/framed/gRPC retries and gateway-owned fallback/settlement.

### FIND-TASK-003-3: known-code body text crosses redaction

`workflow/gateway.rs:155-209`, especially `:190-198`, copies a native envelope's message whenever its code is recognized. Provider native relay preserves provider JSON after credential scrubbing; providers control those codes/messages. A real Wyrd code can therefore admit prompt/diagnostic text into RemoteProblem and `skald-workflow/src/attempt.rs:170-184` WorkflowRunError. Existing recognized-code tests preserve text and canary tests cover unknown codes only.

Use existing trusted derive-backed catalog metadata (catalog title) as the recognized-code portable message at the public-client normalization owner. Keep required HTTP status, code, permitted safe field, and catalog remediation; preserve the existing unknown/uncoded fixed-message category path. This owner establishes the portable error safety invariant. Changing the producer's native relay would affect ordinary sibling clients and is unnecessary; preserve it. No second catalog or origin marker is needed.

### FIND-TASK-003-4: obsolete public plaintext helper

`wyrd-utils/src/secret.rs:45-59` exposes `read_secret_file -> String`, but its only remaining caller is adjacent `read_secret_ref`, which wraps the value in SecretString. The former server external consumer now uses that redacted API, as do gateway and client. The broad shared public surface is unnecessary; no current production leak is claimed.

Make the file-reading stage private. Keep `read_secret_ref -> SecretString` as the shared public boundary, preserving open-handle metadata checks, regular-file and permission refusal, UTF-8/size bounds, static errors and blocking-pool caller behavior. Do not replace the IO algorithm or add an abstraction/check.

### FIND-TASK-003-5: module imports violate mandatory rule

New function-local imports at `wyrd-utils/src/secret.rs:66`, `wyrd-client/src/workflow/mod.rs:472`, and `wyrd-spec/src/gateway/policy.rs:622-623` are in nongeneric functions and do not meet the agent-rules exception. The declared module dependency blocks omit these dependencies.

Move the imports to their existing owning production/test module blocks with the same Unix cfg restrictions. Preserve bodies and runtime behavior. No new lint/check/test is required.

## Preserved behavior and non-goals

Preserve exact remote Workflows APIs/idempotency/polling, the approved model amendment, supported dialect projection, immutable fallback/deadline/cancellation/correlation, authenticated fallback-header semantics, selected-only secret timing, native injection, existing authoring/builders, language result contracts and ordinary gateway clients. Keep server authorization/audit/credential/fallback/accounting ownership intact.

No new public provenance/header/credential API, model ingress, compatibility path, configuration option, polling/retry framework, transport, parser, graph, executor, test harness, dependency, feature, allowlist, checker or gate weakening is authorized. The human standing direction requires established standard practice and reuse; removal of unnecessary machinery comes first. A material need outside the selected corrections returns to authority rather than being improvised.

## Ordered behavioral closure and acceptance

The implementer performs Red-Green-Refactor for changed executable behavior and records exact focused commands, expected RED diagnosis, GREEN results and final relevant checks. The review remains source-only.

| Finding | Required acceptance and direct proof |
|---|---|
| FIND-TASK-003-1 | Python registered load with explicit Cards context A runs through A when ambient config is absent or points to B; only A receives the gateway call with A's credential. Authored external-ref loading also retains its client; local native authoring/builders and loaded mutation preserve semantics/context. Use existing Python runtime fixtures and public exports. |
| FIND-TASK-003-2 | Uncoded and known-auth-code-spoofing provider 401 each send one model POST. Renewable credentials renew without resend; renewal failure returns the existing authentication error; successful renewal preserves the original refusal. Uncoded error is SKALD_PROVIDERS_401_AUTH. |
| FIND-TASK-003-3 | OpenAI, Anthropic and Google known-code canary messages never reach RemoteProblem/WorkflowRunError. Required status/code/field/remediation remain; genuine gateway-coded cases use trusted catalog message. Ordinary native relay remains unchanged. |
| FIND-TASK-003-4 | Source/API proof shows only redacted SecretRef cross-module output; all existing reader safety and consumer behavior persists. Visibility alone needs no new runtime test. |
| FIND-TASK-003-5 | Source proof shows imports at owning module tops, cfg preserved and bodies unchanged. Import relocation alone needs no new runtime test. |

Extend the existing `public_gateway_call_context_and_errors` selector rather than creating a new transport harness. Its exact focused command for the implementer is `mise exec -- cargo nextest run --locked -p wyrd-client --test workflow_transport -E 'test(=public_gateway_call_context_and_errors)'`. Preserve remote regression proof through `mise exec -- cargo nextest run --locked -p wyrd-client --test workflow_transport -E 'test(=shared_workflow_client_contract)'` and selected dependency proof through `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=workflow::tests::selected_local_dependencies_use_shared_config)'`. Newly named Python tests must have exact public-runtime selectors recorded by the implementer.

For broader implementation verification, inspect current canonical tasks and run affected shared/Python/gateway/SDK families, Python typing, codegen and relevant boundary checks plus required format/lint lanes from the original task. Reuse gateway/server file-secret coverage for the private-reader edit. No full gateway relay change is selected; add a broader server lane only if implementation actually affects that boundary. Record proof in task evidence; missing/unclear/contradicted proof is assessed by the next source-only review. Do not expand this into the repository-wide aggregate without the repository's scope trigger.

## Completion and next review

Route this task to `$wyrd-implement`; the private retry wording correction goes through `$wyrd-plan` under the same spec. Do not implement optional alternatives. Record closure by stable finding ID, exact evidence and cumulative candidate. The subsequent `$wyrd-task-review` reassesses the complete original base-to-new-candidate range with the original task, this verdict/ledger and remediation task.
