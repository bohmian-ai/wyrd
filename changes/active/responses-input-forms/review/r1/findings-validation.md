# Independent structured Ponytail validation

## Subject and outcome

Validated base `807c4fe68` → candidate `6294bf6bbc0acb9abc37e4e4d617e2638f9dc3f7`. The approved specification and original task are both `changes/active/responses-input-forms/spec.md`, revision 1, as directed by the user. Read the complete `candidate.diff`, every discovery report, `claim-comparison.md`, and final `verification.md`. HEAD still matches the candidate. Implementation was not modified; no generator or test was run by this validator.

Recommended verdict: **FIX_REQUIRED**. Three independently retained correction boundaries remain. None requires a new product or expensive architecture decision. No prior finding IDs exist.

## Authority and reconciliation

Applied repository AGENTS §§2, 8–12 and 16, agent-rules, active design's client/contracts and journey doctrine, doctrine's Skald ownership, reference router, spec-driven-development and maintainer-style. CodeGraph preceded source searches. Existing source, not report agreement, establishes the claims below.

The spec's final paragraph records that the TypeScript Agents SDK journey is “not written yet” and is “recorded as a gap in the pull request.” It does not direct omission, exempt this capability from AGENTS §11, or redefine the repository completion rule. The current user asks for review against that spec and does not waive the journey. Spec-driven-development explicitly says task compliance cannot waive stronger repository authority. Therefore the gap remains required proof. This conclusion does not require additional language-wide journeys or unrelated gateway cases.

| Acceptance obligation | Reconciled assessment |
|---|---|
| REQ-001: easy message becomes its typed equivalent | PASS by decoder source and passing captured-body unit. Canonical typed serialization may normalize easy input; original shorthand preservation inside Skald is not required. |
| REQ-001: text or parts function output keeps the selected form | PASS by the untagged enum and passing parts assertion; existing string writers choose Text through `From<String>`. |
| REQ-001: both forms reach provider unchanged | PASS by runtime source; required real-server proof is missing (FIND-1). Gateway validates a borrowed raw body, then forwards the original body; it never forwards the normalized typed value. |
| INV-001 tenant isolation | PASS by authenticated Caller → invocation admission → existing tenant owner. No new tenant authority, SQL, credentials or permission path. |
| AC-001 captured Agents SDK unit | PASS: exact selector ran one test successfully. |
| AC-002 formatting, lints, codegen and owner lane | PASS: final supplement records all requested checks and 345 Skald tests passing. Earlier reports' pending-check failures are superseded and are not retained findings. |
| Applicable public input contract parity | FAIL (FIND-3). Decoder accepts easy messages but generated/schema input alternatives reject them. |
| Applicable materially changed method rustdoc | FAIL (FIND-2), the explicit hard documentation exception. |
| Scope and non-goals | PASS: owner extension and mechanical consumer adaptations; no duplicate runtime owner, transport, lifecycle or durable behavior. |

## Every proposed claim adjudicated

| Discovery IDs | Status | Retained ID | Validation |
|---|---|---|---|
| BEH-001, INV-REV-001, STD-001 | REVISED | FIND-responses-input-forms-1 | Missing Responses journey confirmed. Narrow correction to this real Agents SDK tool continuation and same-boundary refused caller; discard STD-001's added malformed-body demand as unnecessary to close this specific claim. |
| INV-REV-002, STD-002, MNT-001 | CONFIRMED | FIND-responses-input-forms-2 | Changed accessor has no rustdoc or Errors section and now returns a lossy text projection. Batch documentation correction; no new accessor or API. |
| MNT-002 | REVISED | FIND-responses-input-forms-3 | Easy wire input is absent from request schema and generated authoring types. Correct input-only accepted alternatives on existing owner; do not broaden canonical item/output types or require typed shorthand preservation. |

System and security/tenancy reports proposed no findings. Their empty findings remain compatible with the runtime source trace; they do not close missing proof or schema parity. No follow-up uncertainty remains after tracing these boundaries.

## FIND-responses-input-forms-1 — missing gateway Responses journey

Status: **REVISED**. Classification: **MISSING**. Sources: BEH-001, INV-REV-001, STD-001.

Obligation: AGENTS §11, agent-rules journey requirement, active design doctrine 20, and REQ-001 provider preservation. Location: `crates/skald/skald-spec/src/wire/openai_responses.rs:692`; absent corresponding Responses coverage in `sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-inference.test.ts:79`.

Reachable producer/consumer evidence: stock Agents input enters `routes.rs::responses:915`, `GatewayResponsesRequest` transparently wraps `OpenAiResponsesRequest` (`wyrd-spec/src/gateway/openai.rs:46`), and `typed_body:1723` deserializes only to validate then returns its original `Value`. `invoke_openai:1779` transfers that body to `GatewayInvocation`. Its admission retains authenticated tenant and requires permission before provider work (`invocation.rs:424`). `adapter::prepare:150` selects the native Responses branch; `native:531` copies the object and changes only model. Full decoder, typed-body, invoke-openai, prepare and native bodies were inspected. Thus no runtime corruption is alleged; the missing executable proof is at the same ingress seam that caused the reported failure.

The candidate's new unit stops at serde and asserts canonical typed items. The existing `loop_responses.rs` uses the Skald mock-provider owner and bypasses public ingress. The TypeScript `gateway-inference` deployment has only Chat Completions capability and its client uses `chat.completions.create`; no Responses Agents SDK tool continuation is present. Sibling Python/Rust proof does not run the named stock Agents SDK path. The existing TypeScript `serverTest` already starts a real Wyrd server and recording local `Receiver`, exposes scoped keys, and allows request-dependent provider replies. It supplies the required existing owner; a new harness is unnecessary.

Consequence: accepting the captured shape in isolation cannot establish that the real agent's initial easy request and next tool-parts request survive authenticated gateway forwarding and return a final answer.

Smallest correction: extend that existing TypeScript gateway inference journey to run the real Agents SDK `OpenAIResponsesModel` with one tool through `/v1` on `WyrdTestServer`. Assert upstream received the easy input and content-parts tool output in their original forms, aside from existing model routing; assert final answer and refusal of a caller lacking gateway invocation without upstream dispatch on this Responses path. Reuse `serverTest`, recording receiver, access-token acquisition, deployment/credential setup, and Postgres wrapper. The Agents SDK is absent from the workspace devDependencies: add it only as the test dependency needed to exercise the expressly named integration; no production dependency, substitute agent framework, transport wrapper or live-provider credential. Do not change authz, forwarding or lifecycle to manufacture proof. No additional all-language or full gateway suite is required in remediation.

Focused closure: build existing TypeScript addons as needed (`mise run ts:build`, `mise run ts:build:testing`), then the repository runner command for this file with its new exact Vitest test-name expression:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/gateway-inference.test.ts -t "<exact added Responses journey test name>"'
```

This is an explicit future-selector template, not a claim that an unimplemented test exists. Record the actual full test name in remediation; use a separate exact selector for the refusal test if separate. The path and runner match `ts:test:gateway:journey:inner`; do not invoke its other gateway files merely to close this finding.

## FIND-responses-input-forms-2 — changed Python text projection lacks rustdoc

Status: **CONFIRMED**. Classification: **VIOLATION**. Sources: INV-REV-002, STD-002, MNT-001.

Obligation: AGENTS §16 and agent-rules explicitly require substantive rustdoc and `# Errors` on materially modified fallible methods; the task-review skill preserves this as blocking. Exact location: `crates/skald/skald-prompt/src/python/openai_responses.rs:571`.

Producer/consumer evidence: the decoder constructs `OpenAiFunctionCallOutput::Parts`; Python request `input:59` copies items into registered `PyOpenAiResponseItem`. Its entire `as_function_call_output` body returns `output.text().into_owned()` or existing `wrong_variant`. The full Rust `text` body (`openai_responses.rs:437`) borrows Text or concatenates input/output-text strings and skips image/file parts. This accessor is its only production caller. Other changed writers (`tool_result_message` and `Prompt::with_tool_result`) produce Text, not projections, so no shared downstream guard is warranted. Public `prompt/__init__.pyi:900` still says it returns output rather than explaining extracted text.

Consequence: changed Python projection's media/boundary loss and wrong-variant failure are undocumented and the explicit documentation gate is unmet. No assertion is made that projecting text violates REQ-001 raw gateway preservation.

Smallest correction: document the existing accessor's text return, no-separator concatenation, nontext omission, and wrong-kind error under `# Errors`. Align its existing hand-authored stub source `sdks/wyrd-sdk-python/python/wyrd/stubs/prompt.pyi:899`, then regenerate the public stub. `scripts/assemble_stubs.py` owns that copy, so editing only generated `prompt/__init__.pyi` would not fix the source. Preserve `str`, error mapping and text implementation. Batch this with substantive corrections; no style-only round or new runtime test is needed for prose.

Closure: source inspection of method rustdoc/Errors and source/public stub agreement, plus `mise run codegen:check`. No new API, relocation, projection parser or Python lifetime test is prescribed.

## FIND-responses-input-forms-3 — accepted easy input omitted from machine contract

Status: **REVISED**. Classification: **INCORRECT** public-contract parity. Source: MNT-002.

Obligation: REQ-001 accepted easy form, AGENTS §§2 and 8–9 wire/schema/language contract alignment, maintainer-style typed contracts. Source location: `crates/skald/skald-spec/src/wire/openai_responses.rs:291`; consumers: `crates/wyrd-spec/schemas/prompt_spec.json:3385`, TypeScript `card-types.ts:2557,2640`, and Python `_card_types.py` corresponding input/message aliases. Same cause affects all nine changed schemas and two eval fixtures.

Producer/consumer evidence: `easy_items` inserts message type for missing discriminator with role, and converts string message content to a typed text-part array before existing tagged decoding. Schemars still derives Items from `Vec<OpenAiResponseItem>`; it cannot infer those rewritten input alternatives from the Serde hook. Published `OpenAiResponseItem` message requires content/role/type and content must be an array (`prompt_spec.json:3030`). Input's array branch references that item unchanged. The captured `{role:"user",content:"go"}` fails both requirements. Generated TypeScript and Python mirror the same required discriminator and list content. The public gateway request derives that same schema and its `ToSchema` implementation converts it into OpenAPI (`gateway/openai.rs::openapi::component`), so this is a reachable request contract rather than dormant metadata. Read the full input enum/helper/access/edit methods and the existing schema conversion/registration owner. Canonical item consumers in prompts, agent loops, Python views and Responses answers must remain canonical.

Consequence: machine-readable request validation rejects a form the route now accepts; typed authoring cannot express the required easy input without bypassing generated declarations. Green codegen proves agreement with an incomplete source schema, not agreement with accepted input.

Smallest correction: describe accepted easy-message alternatives in the schema for **Responses input items**, alongside existing canonical items. Reuse the current Skald input owner and installed Schemars source-schema customization; its runtime decoder, normalized storage and tagged output/item types remain unchanged. The accepted input alternative has required string role and string-or-existing-content-parts content, with message type omitted or equal to `message`; it must allow missing-type parts messages and typed-message string content already accepted by the hook. Do not weaken non-message tagged variants or publish arbitrary JSON. Regenerate schemas, fixtures and existing Python/TypeScript projections from that source. Use input-level union composition without widening response output/canonical item types. This projects behavior already approved and implemented; it introduces no new product contract choice. Do not require preservation of easy shorthand in the normalized typed object.

Focused closure: existing captured-body decoder exact test remains passing; an input-schema regression must prove these easy variants represented, canonical and function items retained, and malformed content/discriminators not admitted by the new alternatives. Use existing schema tooling or feature-gated owning unit coverage, not a new validator dependency/harness. Name any added test and run its actual exact nextest selector through `mise exec --`, `-p wyrd-skald-spec --lib --features schemars`. Run `mise run codegen:check` and owning `mise run ts:typecheck` / `mise run py:typecheck` for generated type changes. Because public OpenAPI is affected, extend existing served-contract proof and run its exact named `pg_openapi_contract` test through the Postgres wrapper; full principal suites are not demanded here. Extend existing `pg_openapi_contract::gateway_ingress_publishes_typed_contracts` in `crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:834`, then run its source-confirmed exact command:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_openapi_contract::gateway_ingress_publishes_typed_contracts)"'
```

This runner/package/target matches the existing `test:principals:integration:inner` served-contract lane. Checked-in schemas do not prove the served document.

## Limits and ladder outcome

All supplied format/lint/codegen and Skald executions passed. This validator performed source-based independent validation only. Missing journey and schema mismatch remain material despite those results. No source-supported failure warrants replacing gateway admission, native forwarding, tenant selection, lifecycle ownership or text projection. Each retained correction uses an existing owner; deletion would leave its explicit requirement unmet. No optional hardening, new check script, full-suite requirement, or unrelated refactor is retained.
