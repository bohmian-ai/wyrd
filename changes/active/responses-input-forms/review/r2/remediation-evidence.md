---
id: responses-input-forms-R1-contract-and-journey
status: ready
parent: SPEC-responses-input-forms
findings:
  - FIND-responses-input-forms-1
  - FIND-responses-input-forms-2
  - FIND-responses-input-forms-3
---

# Complete accepted-input contracts and Responses journey proof

Approved spec and original task: `changes/active/responses-input-forms/spec.md`, revision 1 (the spec is the task by owner instruction). Reviewed base `807c4fe68ddcf43f325e970ed15f8a7c9918e0d4`; cumulative candidate `6294bf6bbc0acb9abc37e4e4d617e2638f9dc3f7`. Parent verdict and independently validated evidence: `verdict.md` and `findings-validation.md` beside this task. Route directly to `$wyrd-implement`.

## Diagnosis and selected correction

### FIND-responses-input-forms-1: the real agent gateway seam lacks proof

The captured-body unit at `crates/skald/skald-spec/src/wire/openai_responses.rs:692` proves typed decoding only. Existing Skald tool-loop tests bypass gateway ingress; existing `sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-inference.test.ts:79` uses Chat Completions. No actual Agents SDK Responses tool loop drives authenticated `/v1/responses` through WyrdTestServer. The spec records that absence, but does not waive AGENTS §11's journey obligation.

Source supports correct forwarding: gateway `typed_body` validates then returns original JSON; native Responses adapter changes only model. A normalized decoder assertion cannot prove the caller's easy input and second-turn content-parts output survive that separate path. This is a proof gap, not a diagnosis of broken forwarding.

Extend the existing TypeScript gateway inference journey owner using its `serverTest`, recording upstream, deployment/credential setup and scoped tokens. Run the real Agents SDK `OpenAIResponsesModel` with one tool through WyrdTestServer. Prove easy input and parts-valued output arrive upstream unchanged apart from existing model routing, the agent returns its final answer, and a caller lacking gateway invocation is refused without upstream dispatch. The SDK is currently absent from test dependencies; add only the test dependency required for this named integration. Reuse current transport and test infrastructure. No alternate harness, production dependency or live provider credentials.

### FIND-responses-input-forms-3: schema generation cannot infer the Serde rewrite

`OpenAiResponsesInput::Items` at `crates/skald/skald-spec/src/wire/openai_responses.rs:291` adds `easy_items` deserialization. Schemars still derives an array of canonical `OpenAiResponseItem`, whose message requires `type` and array `content`. Consequently the captured `{role:"user",content:"go"}` is accepted at runtime but rejected by generated request schemas and cannot be expressed through generated Python/TypeScript input declarations. The same source backs transparent GatewayResponsesRequest's served OpenAPI. Passing codegen verifies regeneration, not completeness of accepted input.

Correct the source schema for Responses **input items** on the existing Skald wire owner using installed Schemars customization. Describe easy message input alongside canonical items: required string role and string-or-existing-content-parts content, with type omitted or `message`. Cover missing-type parts messages and explicitly typed string-content messages already accepted by the decoder. Preserve canonical normalized storage, tagged item/response-output types, existing non-message validation, and the current decoder. Do not broaden the entire item type or publish arbitrary JSON. Regenerate all affected schemas, eval fixtures and generated Python/TypeScript declarations through current generators. Extend existing served-contract test ownership to prove the runtime OpenAPI input schema admits the supported forms.

This selects input-only contract projection, rather than widening canonical output or preserving easy shorthand inside Skald. It closes the producer mismatch once, without another gateway decoder or consumer guard.

### FIND-responses-input-forms-2: the Python text view is undocumented

`crates/skald/skald-prompt/src/python/openai_responses.rs:571` now calls `OpenAiFunctionCallOutput::text().into_owned()` instead of cloning a string. For parts, it concatenates input/output text with no separators and skips nontext parts. The materially changed fallible accessor has no rustdoc or `# Errors`, violating AGENTS §16. Its public stub describes returning output without this text-view qualification.

Document these existing semantics and the wrong-item-kind error on the accessor. Align the hand-authored stub source `sdks/wyrd-sdk-python/python/wyrd/stubs/prompt.pyi:899` and regenerate its public projection using the existing assembly owner. Preserve `str`, existing error mapping and implementation. This bounded documentation correction is batched with substantive contract/proof work; no new accessor, wrapper relocation or runtime semantics.

## Constraints and preserved behavior

Honor the approved spec and repository authority. Tenant identity, authz, admission, accounting, audit, credentials, model routing, cancellation and lifecycle remain under their existing owners. Gateway forwards original input JSON; Skald normalization remains permitted. Existing text output writers retain text output. Existing canonical messages, function calls, output arrays and response item types remain supported. Generated artifacts are regenerated, never hand-edited; authored stub source is the correction boundary.

Non-goals: broader multimodal expansion, unrelated gateway negative cases, all-language journey additions, new runtime/transport/parsing owners, arbitrary-JSON schema escape hatches, full-suite remediation gates and unrelated documentation cleanup.

## Acceptance and focused proof

| Finding | Acceptance |
|---|---|
| FIND-1 | Actual Agents SDK Responses tool loop through real WyrdTestServer completes; upstream assertions prove original easy/parts forms; refused caller triggers no upstream request. |
| FIND-3 | Accepted easy variants validate against source/generated/served input schema and appear in generated input declarations. Canonical items stay valid; malformed new alternatives remain invalid. Canonical output declarations are not broadened. |
| FIND-2 | Meaningful accessor rustdoc and Errors plus authored/regenerated stub agree on text projection and wrong-kind failure without behavior/signature changes. |

Use red-green proof for the missing journey and input-schema contract; current captured-body decoder stays green. Documentation uses static/source-generation proof. Name every added test in the implementation report and include its actual exact focused command; do not claim future selector placeholders are executable tests.

Existing focused decoder:

```sh
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=wire::openai_responses::tests::easy_input_messages_decode_as_typed_messages)'
```

Input-schema regression: reuse existing schema tooling/feature-gated owning unit tests, no new validator dependency or harness. Run the actual new exact selector through `mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib --features schemars -E 'test(=<actual complete test name>)'`.

Build existing TypeScript addons if necessary with `mise run ts:build` and `mise run ts:build:testing`. Run only the added journey's actual exact test-name selector in the existing file using:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/gateway-inference.test.ts -t "<actual complete added test name>"'
```

Use separate exact selectors if acceptance is split across tests. Extend the existing served contract test and run its already-established exact command:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_openapi_contract::gateway_ingress_publishes_typed_contracts)"'
```

Final narrow lanes for this write set: format/lints required by AGENTS, `mise run codegen:check`, `mise run ts:typecheck`, `mise run py:typecheck`, and owning `mise run test:skald`. Run owning TypeScript/Python formatting/lint checks where their source files change. Full gateway/user-journey aggregates run at change review, not this remediation. Record all results and actual selectors. Stop for specification revision only if correction requires a material decision beyond accepted input projection and proof already specified here.

## Implementation evidence

Status: `IMPLEMENTED` (uncommitted working tree on `6294bf6bb`).

Reuse map: the existing Skald wire owner (`OpenAiResponsesInput`) gains a Schemars `schema_with` hook, using the installed schemars 0.8. No new type, decoder, or validator. The existing `pg_openapi_contract` test validates the served document with the `jsonschema` dev dependency wyrd-server already has. The existing `gateway-inference.test.ts` `serverTest`/Receiver/deployment fixtures carry the journey. The only new dependencies are the dev-only `@openai/agents-core` and `@openai/agents-openai` at `^0.20.0`. The umbrella `@openai/agents` was rejected because its realtime typings need DOM libs and break `ts:typecheck`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-1: a real Agents SDK Responses tool loop through WyrdTestServer completes, and upstream receives the easy input and parts output unchanged apart from routing | `gateway-inference.test.ts`: Responses-capable `gpt-4o-mini` deployment, path-branching upstream (`responsesTurn`), test `agents sdk responses tool loop runs through the gateway` | Exact vitest `-t` selector passes. Upstream gets two `/v1/responses` calls with `model: "gpt-4o-mini"`, first input `[{role:"user",content:"go"}]`, and second input carrying that message plus a `function_call_output` with `output: [{type:"input_text",text:"hi"}]`. `finalOutput === "echoed"` | PASS |
| FIND-1: a caller without gateway invoke is refused with no upstream dispatch | test `caller without gateway invoke cannot run an agents sdk agent` | Exact vitest selector passes: rejects with 403 and receives no upstream requests | PASS |
| FIND-3: the source schema admits easy messages beside canonical items | `openai_responses.rs` `input_items_schema` registers `OpenAiEasyInputMessage` (required `content`, `role`; `type` enum `["message"]`; `content` is a string or content parts) | `wire::openai_responses::tests::input_items_schema_admits_easy_messages`: red with the hook removed (`left: Null`), green with it | PASS |
| FIND-3: the served OpenAPI admits the supported forms and refuses malformed alternatives | `pg_openapi_contract.rs` `gateway_ingress_publishes_typed_contracts` compiles the served `GatewayResponsesRequest` | Admits an easy message without `type`, a typed message with string content, an untyped parts message, a canonical message, and a parts `function_call_output`. Refuses a message with no `role`, numeric `content`, and an unknown `type`. Passes | PASS |
| FIND-3: generated declarations carry the easy input. Canonical item and output types are not broadened | `codegen:regen`. TS: `OpenAiResponsesInput = string \| readonly (OpenAiResponseItem \| OpenAiEasyInputMessage)[]`. Python: `OpenAiEasyInputMessage` dataclass. JSON schemas and eval fixtures regenerated. `OpenAiResponseItem` and output types are unchanged | `codegen:check` passes | PASS |
| FIND-2: the accessor's rustdoc and `# Errors`, plus the authored and regenerated stub, agree with no behavior change | `skald-prompt/src/python/openai_responses.rs` rustdoc. `stubs/prompt.pyi` docstring with `Raises: WYRD_PROMPT_400_PROVIDER_MISMATCH`, regenerated into `prompt/__init__.pyi` | `codegen:check`, `py:typecheck`, `lints` | PASS |
| The existing captured-body decoder stays green | unchanged | exact selector passes | PASS |

Commands run (all exit 0 in their final run):

```sh
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib --features schemars -E 'test(=wire::openai_responses::tests::input_items_schema_admits_easy_messages) | test(=wire::openai_responses::tests::easy_input_messages_decode_as_typed_messages)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_openapi_contract::gateway_ingress_publishes_typed_contracts)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/gateway-inference.test.ts -t "agents sdk responses tool loop runs through the gateway"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/gateway-inference.test.ts -t "caller without gateway invoke cannot run an agents sdk agent"'
mise run fmt; mise run lints; mise run codegen:check; mise run test:skald
mise run ts:typecheck; mise run ts:format:check; mise run ts:lints
mise run py:typecheck; mise run py:format; mise run py:lints
git diff --check
```

Limits:
- Journey red: not run against a rebuilt pre-fix server. The base decoder rejects `{role, content:"go"}` with `WYRD_GATEWAY_400_INVALID_REQUEST`, per the spec's defect and the red captured-body unit, so the first assertion cannot pass on the base.
- Served OpenAPI red: not run separately. The red skald schema unit covers the same source.
- Non-goals: no multimodal expansion, no other gateway negative cases or languages, and no new runtime, transport, or parser owners. No unrelated files changed.
- The untracked `review/` packet predates this work; only this task file was appended to.
