# Immutable review subject

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/responses-input-forms`.
- Base: `807c4fe68` (parent of the implementation commit).
- Candidate: `6294bf6bbc0acb9abc37e4e4d617e2638f9dc3f7`.
- Spec and original task: `changes/active/responses-input-forms/spec.md`, approved revision 1. The user explicitly identifies the spec as the task; no separate packet is required.
- Complete diff: `candidate.diff` beside this file. Initial tracked worktree is clean. Review artifacts alone may be added; reviewed implementation remains immutable.
- Prior reviews and supplied verification results: none. Verification performed during review is recorded separately.

## Navigation starting map

The implementation extends `crates/skald/skald-spec/src/wire/openai_responses.rs`: `OpenAiResponsesInput::Items`, `easy_items`, `OpenAiResponseItem::FunctionCallOutput`, `OpenAiFunctionCallOutput::{text,from}`, and `tests::easy_input_messages_decode_as_typed_messages`.

Consumers changed to accommodate the output type: `crates/skald/skald-agent/src/request_builder.rs::tool_result_message`, `crates/skald/skald-prompt/src/prompt.rs::Prompt`, `crates/skald/skald-prompt/src/python/openai_responses.rs::PyOpenAiResponseItem::as_function_call_output`, and `crates/skald/skald-agent/tests/integration/loop_responses.rs`.

Generated contracts changed: nine JSON schemas under `crates/wyrd-spec/schemas`, two eval schema fixtures, Python `_card_types.py`, TypeScript `card-types.ts`. No manifest or transport implementation is changed.

Reachable surrounding owners to trace: gateway `/v1/responses` route in `crates/wyrd/wyrd-server/src/components/gateway/routes.rs`, gateway request validation and forwarding, `crates/wyrd-spec/src/gateway/openai.rs`, provider request authoring, prompt editing, Skald agent Responses tool loop, and Python projection. Use CodeGraph first; expand this map independently.

Authorities: AGENTS.md, architecture/agent-rules.md, architecture/wyrd-design.md, architecture/wyrd-doctrine.mdx, reference router, spec-driven-development and maintainer-style references; select Rust, PyO3, generated Python/TypeScript, testing and security references as applicable.

Sensitive domain: gateway request trust boundary and tenant isolation. Independent domain review traces changed decoding through existing authorization and forwarding, without requesting unrelated hardening.

Verification candidates: exact `mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=wire::openai_responses::tests::easy_input_messages_decode_as_typed_messages)'`; `mise run test:skald`; format, lints, codegen check required by AC-002. Confirm selectors and task contents before running. The spec records the missing TypeScript Agents SDK journey as a gap; assess against stronger repository authority explicitly.
