# Live provider sweep findings (2026-10-16)

`mise run test:gateway:smoke:live`, one server and all four keys, models
`gpt-6-luna`, `claude-haiku-5-5`, `gemini-3.8-flash` and `jev-latest`:
**42 passed, 1 failed, 0 skipped.** No credential appears in the log.

## Coverage

- **Part 1** sends each provider's own client through the gateway. All cases
  passed: plain text, system, streaming, forced tool call and native
  structured output for OpenAI, Anthropic and Gemini, plus OpenAI-compat,
  Responses, embeddings, audio, images, batch and the Jev typed questions.
- **Part 2** runs Prompts straight to the provider through a native-route
  Agent. Every case passed except `test_native_system_one` (JEV-1).
  - Portable cases: text, system plus history, `max_output_tokens`, a
    closed, nested `$ref` `response_format`, and a local tool loop.
  - Native cases: `chat_completion`, `responses`, `messages` and
    `generate_content`.
- **Not covered yet:**
  - Wyrd Prompts through the gateway (part 3);
  - native tool, structured-output and streaming Prompt cases;
  - a tool-result turn through the gateway, which would prove Gemini
    thought-signature replay;
  - expected-rejection cases.

## Findings

| Id | Finding | Attribution | Status |
|---|---|---|---|
| JEV-1 | `Agent.run` validates every request as a conversation, so a Jev `system_one` Prompt fails with "request shape does not support tool-loop dispatch". | Skald Agent dispatch | Open. Spec `agent-runtime-composition` REQ-001 to REQ-003. |
| PRM-1 | Invalid authored Prompt JSON raised `WYRD_SPEC_500_INTERNAL` "JSON conversion failed". | Python Prompt boundary | Fixed: raises `WYRD_PROMPT_400_INVALID` naming the field (`skald-prompt` `invalid_authored_json`). |
| PRM-2 | A native `generate_content` body needed a `role` on `system_instruction`, which Gemini does not require. | Gemini wire type | Fixed: `GoogleContent.role` is optional and round-trips as absent. The live case runs without a role. |
| PRM-3 | Gemini schemas went to the OpenAPI-subset `response_schema` and `parameters`, which refuse `additionalProperties`, so one portable schema could not reach every provider. Same as seed GEM-1. | Skald lowering and gateway adapter | Fixed: Skald and the gateway's OpenAI-compat adapter emit `response_json_schema` and `parameters_json_schema`. The live case sends one closed, nested `$ref` schema to all three providers. |
| GW-1 | A bare Gemini model id (`/v1beta/models/<model>:generateContent`) fell through to Wyrd API authentication: 401 "missing X-Wyrd-Access-Token". | Gateway ingress routing | Fixed: refused under the Gemini ingress with a Google-envelope 400, `WYRD_GATEWAY_400_INVALID_REQUEST`, naming `<deployment>/<model>`. |
| LUNA | `gpt-6-luna` refuses temperature other than 1, `top_p`, `stop`, penalties, and tools unless `reasoning_effort: "none"`. | Provider limit, relayed accurately | No product fix. Add expected-rejection cases. |
| GEM-2 | Gemini `thoughtSignature` has no field on `GooglePart`, so replayed tool turns may lose it. | Skald wire type, unconfirmed | Open until a tool-result case runs through the gateway. |
| OBS-1 | On the Agent gateway path, a provider 400 surfaces as `SKALD_PROVIDERS_400_BAD_REQUEST` without the provider's message. | Gateway caller error mapping, from seed | Open, not re-verified. It cannot run until part 3. |

Seed rows OAI-1 (portable `reasoning_effort`, now present), OAI-2 (luna
limits, now LUNA), OAI-3 (harmless while tools are non-strict) and AGT-1 (now
spec `agent-runtime-composition`) are retired.
