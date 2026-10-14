# TASK-011 Gateway accepts every Responses input form the Agents SDK sends

Spec: `changes/active/example-journey-fixes/spec.md` r6, AC-004 (the example
runs clean with no workarounds).

## Defect

Found in the live `wyrd-example-ts` run. The gateway checks a `/v1/responses`
body by decoding it as Skald's `OpenAiResponsesRequest`, which accepted only
the typed message form and string tool results. The official Agents SDK
sends two other forms `OpenAI` accepts, so every Agents SDK Responses agent got
`WYRD_GATEWAY_400_INVALID_REQUEST` before any provider call:

- the easy input message `{"role": "user", "content": "go"}` (no `type`,
  text `content`);
- a tool result whose `output` is content parts
  (`[{"type": "input_text", "text": ...}]`).

## Fix

`crates/skald/skald-spec/src/wire/openai_responses.rs`: input items decode
easy messages as the typed message they stand for, and
`OpenAiFunctionCallOutput` accepts text or parts, keeping the sent form.
Builders wrap text results with `.into()`; the Python accessor returns the
result's text.

## Evidence

- Unit: `wire::openai_responses::tests::easy_input_messages_decode_as_typed_messages`
  decodes the captured Agents SDK body shape.
- Live: `wyrd-example-ts` `npm run investigate` (Agents SDK
  `OpenAIResponsesModel`, MCP tools, reasoning `medium`) completes multi-turn
  through the gateway, with and without the tool allow-list.
- Lanes: `cargo nextest` for wyrd-skald-spec, wyrd-skald-agent,
  wyrd-skald-prompt (227 passed); clippy `-D warnings`; `codegen:check` after
  `codegen:regen`; `ts:typecheck`; `py:typecheck`.

## Open

A TypeScript gateway journey driving the Agents SDK `OpenAIResponsesModel`
with a tool through `WyrdTestServer` against a fake upstream is not yet
written.
