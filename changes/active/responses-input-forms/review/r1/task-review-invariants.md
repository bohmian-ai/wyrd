# Independent invariant acceptance review

## Subject and result

Result: **FAIL**.

Base `807c4fe68` → candidate `6294bf6bbc0acb9abc37e4e4d617e2638f9dc3f7`; repository root and complete diff are recorded in `subject.md`. The approved `changes/active/responses-input-forms/spec.md` revision 1 is also the original task, as explicitly instructed by the user. HEAD remained the candidate at the end of this review. No source, fixtures, generated outputs, or manifests were changed by this reviewer.

The implementation satisfies the concrete decoding and forwarding behavior described in REQ-001. The remaining proposed findings concern its missing required gateway journey and missing documentation on the materially changed Python-facing accessor. They do not claim that the raw gateway request is normalized before forwarding.

## Authority and scope

Applied `AGENTS.md` §§2–12, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; the reference router; and the spec-driven development, maintainer style, Rust, PyO3, Python, TypeScript, and testing references. The spec is the task, so absence of a separate task packet is not a finding. The approved spec explicitly records the TypeScript journey gap; that records missing proof rather than an explicit current-user waiver of the stronger journey rule.

Inspected the complete cumulative diff, including all generated schema/fixture/Python/TypeScript changes. CodeGraph was the first navigation mechanism for changed symbols and gateway consumers. The independent sensitive-domain review owns the deeper security audit; this review nevertheless traced tenant provenance on the affected reachable path.

## Producer-to-sink audit

1. `POST /v1/responses` enters `routes.rs::responses` (915–928), which calls `typed_body::<GatewayResponsesRequest>`.
2. `GatewayResponsesRequest` is the transparent wrapper around Skald's request (`crates/wyrd-spec/src/gateway/openai.rs:46–49`). The request's `input` decoder is the changed `OpenAiResponsesInput::Items` field (`openai_responses.rs:291`). `easy_items` (307–333) supplies a missing message discriminator and transforms string content into the corresponding input/output text part. Other items retain their typed decoder and malformed items still return a serde error. No new parser owner, service, dependency, or transport was introduced.
3. `typed_body` (`routes.rs:1723–1738`) deserializes solely to validate and returns the **original** `Value`. The normalized typed request is discarded. Consequently, the new unit test's normalized message output and dropped function-item `id`/`status` do not establish an upstream mutation: they are not the body forwarded by this route.
4. `invoke_openai` (1779–1819) obtains the model projection, optional fallback/attribution, streaming choice, and usage bound from that original body, then transfers it to `GatewayInvocation`.
5. `GatewayInvocation::admit_call` (`invocation.rs:424–483`) binds `tenant` to `caller.data_tenant_id`, performs existing permission/attribution decisions, loads that tenant's snapshot, and retains the original body. Ledger admission and accounting acquire tenant connections from that retained authenticated tenant (`746–760`, `774–789`). Neither easy-message role nor function-output content can replace tenant provenance.
6. OpenAI/compatible Responses calls use the existing native adapter (`crates/wyrd/wyrd-gateway/src/adapter/mod.rs:178–186`). `native` (531–541) copies the object and changes only its model projection. Both requested input forms therefore reach the provider with their original input JSON.
7. The new untagged `OpenAiFunctionCallOutput` (426–433) stores text and typed content-part arrays separately. For the captured `input_text` shape, serialization retains the original output array. Text remains a string. Existing prompt authoring (`skald-prompt/src/prompt.rs:128–167`) and agent tool-result construction (`skald-agent/src/request_builder.rs:375–413`, called from `append_turn_message`) continue constructing strings via the standard `From<String>` implementation; they do not acquire a second workflow or choose arrays unexpectedly.
8. Sibling typed consumers include request rendering/editing via `items`/`items_mut`, Responses answer output, `MessageNum::OpenAiResponses`, the Python request view, and generated Card contracts. The materially changed Python accessor (`skald-prompt/src/python/openai_responses.rs:571–575`) keeps its existing string signature and obtains text via the Rust owner; parts are concatenated and non-text parts omitted. This is a text view, not the provider-bound value. It needs its semantics documented.
9. Schema and generated declaration changes project the new function-output union through the existing generators. They add no persistent owner, durable state, retry path, compatibility route, configuration, or cross-tenant cache. Easy inputs deliberately become typed messages in Skald; the spec does not require the typed message to serialize as its original shorthand.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: easy message decodes as its typed equivalent | `openai_responses.rs:291,307–333` | Captured-body exact unit test passed, supplied by orchestrator | PASS |
| REQ-001: function output accepts text or content parts and preserves the selected form | `openai_responses.rs:392–395,426–433,458–463` | New unit assertion preserves `input_text` array; existing text tool-loop test remains in passing Skald lane | PASS |
| REQ-001: both specified forms reach provider unchanged | `routes.rs:1723–1738,1779–1819`; native adapter `mod.rs:178–186,531–541` | Source establishes raw preservation; required real gateway journey absent | PASS (implementation); FAIL (required journey proof) |
| INV-001: tenant isolation on changed paths | Authenticated Caller → `admit_call` tenant → tenant snapshot/ledger/connection | Source trace; decoder is pure and cannot select tenant | PASS |
| AC-001: unit decodes captured Agents SDK shape | `openai_responses.rs:692–716` | Exact captured-body unit test passed | PASS |
| AC-002: format, lints, codegen and owning lanes pass | Minimal existing crate/contract ownership retained | `fmt:check` passed; exact unit passed; `test:skald` passed 345/345. Lints and immutable-snapshot codegen check pending | FAIL (not yet fully established; no additional defect inferred) |
| Stronger repository journey obligation | Only new proof is a unit test; modified agent integration fixture remains an in-process Skald tool loop | No SDK → real server Responses tool journey in candidate | FAIL: INV-REV-001 |
| Stronger rustdoc obligation on materially changed methods | Python function-output accessor changes from string clone to text projection without rustdoc | Static source inspection | FAIL: INV-REV-002 |
| Scope: smallest sufficient change, no unrelated capability | Complete 19-file diff; shared wire owner and existing sibling consumers reused | Static cumulative inspection | PASS |

## Proposed findings

### INV-REV-001 — MISSING: the required caller-to-provider journey is absent

- Violated obligation: `AGENTS.md` §11 and `architecture/agent-rules.md`: every user/agent capability ships a real SDK → real server user journey; unit tests never substitute. This proof directly concerns REQ-001's forwarding outcome and the Agents SDK objective.
- Location: `crates/skald/skald-spec/src/wire/openai_responses.rs:692–716`; missing corresponding Responses coverage in `sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-inference.test.ts`.
- Evidence: the new test constructs JSON by hand, only deserializes a Rust request, and asserts its normalized typed input. The modified `loop_responses.rs` merely adapts an existing Skald agent fixture to the new type. The existing TypeScript gateway inference journey at lines 79–104 exercises Chat Completions and its permission rejection, not Responses or a tool-result continuation. The spec's last paragraph expressly acknowledges the missing journey.
- Observable consequence: no executable product-level proof establishes that a stock Responses agent's first easy-message call and subsequent array-output call survive the gateway, preserve the caller's input at the provider, and return an answer. The exact defect described by the task lives at that cross-boundary seam.
- Smallest correction: extend the existing TypeScript `serverTest`/local-provider gateway journey infrastructure with the specified Responses tool continuation, using a real client and `WyrdTestServer`. Reuse provider receipt inspection to assert the easy-message and output-array forms are unchanged (apart from model projection), and retain relevant unauthorized-caller refusal coverage. Do not create a new harness or change transport/authorization. Prefer the stock Agents SDK driver named by the approved spec when available; a merely fabricated Rust fixture does not close the SDK integration proof.
- Focused closure: run the exact added TypeScript test through the existing repository-managed Postgres wrapper and owning TypeScript runner; final change review owns the full journey aggregate.

### INV-REV-002 — VIOLATION: the changed Python output accessor has no rustdoc

- Violated obligation: `AGENTS.md` §16 and `architecture/agent-rules.md` explicitly require rustdoc and `# Errors` on every materially modified fallible Rust method, regardless of visibility; missing rustdoc remains a blocking exception in this review skill.
- Location: `crates/skald/skald-prompt/src/python/openai_responses.rs:571–575`, `PyOpenAiResponseItem::as_function_call_output`.
- Evidence: the changed method now calls `OpenAiFunctionCallOutput::text`, which concatenates text parts and skips non-text parts (`openai_responses.rs:441–453`), while still returning a string. The method itself has no documentation or `# Errors` section.
- Consequence: the public Python-facing view's newly meaningful projection and wrong-variant error are absent from the required documentation; the explicit repository correctness gate is unmet.
- Smallest correction: document the existing projection and its wrong-variant error on the accessor itself. Do not change its signature, move the wrapper, or invent a new parts-view API. Batch this bounded documentation correction with the functional proof remediation.
- Focused closure: source inspection of meaningful rustdoc including `# Errors`, plus the existing owning lint check. No new documentation-only harness or journey is needed.

## Verification limits and exclusions

Supplied verification: format check, the exact captured-body unit test, and 345/345 Skald tests passed. Lints/codegen remain pending and must be reconciled from the final orchestrator evidence; this report does not assert a failure in either lane. I ran no Cargo commands or generators, preserving shared target serialization and candidate immutability.

I did not infer a provider forwarding bug from canonical typed-message normalization, or demand a new gateway validator. The existing `typed_body` raw retention already solves fidelity on this route. Broader multimodal protocol expansion, optional hardening, new runtime ownership, and unrelated documentation were excluded: the concrete task defect is easy messages and the captured `input_text` tool-output array.
