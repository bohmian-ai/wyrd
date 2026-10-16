# Provider probe findings (2026-10-16, seed evidence)

Recorded by a throwaway probe before TASK-005-S1 was rewritten. Not re-verified;
some rows are stale (portable settings now carry `reasoning_effort`, so OAI-1
no longer holds as written). The sweep in TASK-005-S1 supersedes these rows
once it runs.

##  Known findings (expectations for `refused-by-defect`)

| Id | Attribution | Finding | Evidence | Fix site (not this task) |
|---|---|---|---|---|
| GEM-1 | wyrd-lowering | Wyrd sends Gemini schemas in `generationConfig.responseSchema` and `functionDeclarations[].parameters`. Those fields reject `additionalProperties`, `$defs`/`$ref` and `type: [x, "null"]`. Every built-in tool schema has `additionalProperties: false`, so every Gemini Agent with tools fails, and so does every portable `response_format` written for OpenAI strict mode. | §7.1 `gn-response-schema-*`, `gn-parameters-ap`, `gc-output`, `gc-tools`; §7.2 `hg-tool-*` | `crates/skald/skald-spec/src/wire/google_generate.rs` (`response_schema` ~224, `GoogleFunctionDeclaration.parameters` ~289); `request.rs::google_tool`; `authoring.rs` ~795; the second site is `crates/wyrd/wyrd-gateway/src/adapter/google.rs` (`output`, `translate_tools`). |
| GEM-2 | wyrd-lowering, **unconfirmed** | Gemini puts a `thoughtSignature` on its answer parts. Skald's `GooglePart` (`google_generate.rs` ~89) has no field for it, so the signature is dropped when the turn is decoded and re-sent. If Gemini requires it on replayed function-call turns, every multi-turn Gemini tool loop fails even after GEM-1 is fixed. | Seen on the live `gn-plain` answer. The replay trial decides it. | `GooglePart` and `GoogleAnswerContent`; the compat adapter's tool-call round trip |
| OAI-1 | wyrd-dx | Portable Prompt settings (`authoring.rs::PortableSettings`) have no `reasoning_effort`. `gpt-6-luna` on Chat Completions refuses function tools unless `reasoning_effort: none`, so a portable OpenAI Prompt with tools always fails. | §7.1 `on-tools-*`; §7.2 `ho-tool-*` fail while `ho-native-*` pass | portable settings or lowering for reasoning models |
| OAI-2 | provider-limit, with wyrd-dx | `gpt-6-luna` rejects `max_tokens`, `temperature` other than 1, `top_p`, `stop` and the penalties. Portable settings forward them. `max_output_tokens` is correctly lowered to `max_completion_tokens`. | §7.1 sampling trials | per-model setting validation, if wanted |
| OAI-3 | provider-limit | Strict OpenAI tools require every property in `required`. Built-in `fs.read` has an optional `start`, so it cannot be a strict tool. This is harmless today, because Skald sends tools with `strict: None`. | §7.1 `on-tools-strict-optional` | none |
| OBS-1 | wyrd-gateway | On the Agent path, a provider 400 surfaces only as `SKALD_PROVIDERS_400_BAD_REQUEST: Wyrd gateway answered HTTP 400`, and the provider's message is lost. The native ingress relays it intact. | §7.2 `ho-tool-*`, `hg-tool-*` | gateway caller error normalization (`RemoteProblem`) |
| AGT-1 | wyrd-dx | A Wyrd Agent cannot use its own Wyrd gateway from the SDK: `Agent.run` on a `wyrd_gateway` route refuses with "no gateway caller is configured" (`crates/skald/skald-agent/src/agent.rs:389`). The SDK `Agent` cannot accept a `WyrdClient`. `PublicWyrdGatewayCaller` exists, but only the tool-free `Run.observe.invoke` attaches it. A local Skald Google client also refuses a `provider_base_url` override. | probe; source | SDK and `skald-agent` Agent construction |

##  Results already gathered (2026-10-16, `gemini-3.8-flash` and `gpt-6-luna`)

These came from a throwaway Python probe, which was not a deliverable. Use
them as the seed expectations for set A.

### Native and OpenAI-compatible calls through the gateway: valid, 48 trials

All 48 trials relayed correctly, with the provider's status and verbatim
refusal message, so **Q1 holds for native OpenAI and Gemini.** Anthropic has
not been run because no key exists.

| Trial | Provider | Route | Variable | Status | Provider message (verbatim, truncated) |
|---|---|---|---|---|---|
| `gn-plain` | gemini | native | plain text | 200 |  |
| `gn-system` | gemini | native | systemInstruction | 200 |  |
| `gn-sampling` | gemini | native | temperature, topP, topK, seed, stopSequences, maxOutputTokens | 200 |  |
| `gn-thinking-budget` | gemini | native | thinkingConfig.thinkingBudget | 200 |  |
| `gn-thinking-level` | gemini | native | thinkingConfig.thinkingLevel | 200 |  |
| `gn-include-thoughts` | gemini | native | thinkingConfig.includeThoughts | 200 |  |
| `gn-response-schema-ap` | gemini | native | responseSchema with additionalProperties (what Skald sends) | 400 | Invalid JSON payload received. Unknown name "additionalProperties" at 'generation_config.response_schema.properties[0].value.items': Cannot find field. |
| `gn-response-schema-plain` | gemini | native | responseSchema without additionalProperties | 200 |  |
| `gn-response-json-schema-ap` | gemini | native | responseJsonSchema with additionalProperties | 200 |  |
| `gn-response-json-schema-ref` | gemini | native | responseJsonSchema with $defs/$ref | 200 |  |
| `gn-response-schema-ref` | gemini | native | responseSchema with $defs/$ref | 400 | Invalid JSON payload received. Unknown name "$defs" at 'generation_config.response_schema': Cannot find field. |
| `gn-response-json-schema-nullable` | gemini | native | responseJsonSchema with type [string, null] | 200 |  |
| `gn-response-schema-nullable` | gemini | native | responseSchema with type [string, null] | 400 | Invalid JSON payload received. Unknown name "type" at 'generation_config.response_schema.properties[0].value': Proto field is not repeating, cannot start list. |
| `gn-parameters-ap` | gemini | native | function parameters with additionalProperties (what Skald sends) | 400 | Invalid JSON payload received. Unknown name "additionalProperties" at 'tools[0].function_declarations[0].parameters': Cannot find field. |
| `gn-parameters-plain` | gemini | native | function parameters without additionalProperties | 200 |  |
| `gn-parameters-json-schema-ap` | gemini | native | function parametersJsonSchema with additionalProperties | 200 |  |
| `gn-tools-and-json-schema` | gemini | native | parametersJsonSchema tools plus responseJsonSchema together | 200 |  |
| `on-plain` | openai | native | plain text | 200 |  |
| `on-max-tokens` | openai | native | max_tokens | 400 | Unsupported parameter: 'max_tokens' is not supported with this model. Use 'max_completion_tokens' instead. |
| `on-max-completion-tokens` | openai | native | max_completion_tokens | 200 |  |
| `on-temperature` | openai | native | temperature 0.2 | 400 | Unsupported value: 'temperature' does not support 0.2 with this model. Only the default (1) value is supported. |
| `on-temperature-default` | openai | native | temperature 1 | 200 |  |
| `on-top-p` | openai | native | top_p 0.5 | 400 | Unsupported parameter: 'top_p' is not supported with this model. |
| `on-seed` | openai | native | seed | 200 |  |
| `on-stop` | openai | native | stop | 400 | Unsupported parameter: 'stop' is not supported with this model. |
| `on-penalties` | openai | native | presence and frequency penalty | 400 | Unsupported parameter: 'presence_penalty' is not supported with this model. |
| `on-effort-none` | openai | native | reasoning_effort none | 200 |  |
| `on-effort-low` | openai | native | reasoning_effort low | 200 |  |
| `on-effort-medium` | openai | native | reasoning_effort medium | 200 |  |
| `on-effort-high` | openai | native | reasoning_effort high | 200 |  |
| `on-tools-default-effort` | openai | native | function tools, default reasoning_effort | 400 | Function tools with reasoning_effort are not supported for gpt-6-luna in /v1/chat/completions. To use function tools, use /v1/responses or set reasoning_effort to 'none'. |
| `on-tools-effort-none` | openai | native | function tools, reasoning_effort none | 200 |  |
| `on-tools-effort-low` | openai | native | function tools, reasoning_effort low | 400 | (same as on-tools-default-effort) |
| `on-tools-strict-optional` | openai | native | strict tool whose schema has an optional member (fs.read `start`) | 400 | Invalid schema for function 'read_file': In context=(), 'required' is required to be supplied and to be an array including every key in properties. Missing 'start'. |
| `on-strict-output` | openai | native | strict json_schema output | 200 |  |
| `on-strict-output-no-ap` | openai | native | strict json_schema output without additionalProperties | 400 | Invalid schema for response_format 'answer': In context=(), 'additionalProperties' is required to be supplied and to be false. |
| `on-strict-output-ref` | openai | native | strict json_schema output with $defs/$ref | 200 |  |
| `on-strict-output-nullable` | openai | native | strict json_schema output with type [string, null] | 200 |  |
| `on-tools-and-output` | openai | native | tools plus strict output, reasoning_effort none | 200 |  |
| `on-stream-usage` | openai | native | stream with include_usage | 200 |  |
| `on-responses-plain` | openai | native | Responses plain text | 200 |  |
| `on-responses-tools` | openai | native | Responses function tools, default effort | 200 |  |
| `on-responses-output` | openai | native | Responses strict text.format | 200 |  |
| `gc-plain` | gemini | compat | plain text | 200 |  |
| `gc-output` | gemini | compat | json_schema output with additionalProperties | 400 | Unknown name "additionalProperties" at 'generation_config.response_schema...' |
| `gc-output-plain` | gemini | compat | json_schema output without additionalProperties | 200 |  |
| `gc-tools` | gemini | compat | function tools with additionalProperties | 400 | Unknown name "additionalProperties" at 'tools[0].function_declarations[0].parameters' |
| `gc-tools-plain` | gemini | compat | function tools without additionalProperties | 200 |  |

The bodies used were:
- the strict schema `{findings: [{rule: string, page: integer}]}`, with
  `additionalProperties: false` at both levels;
- the same schema expressed with `$defs`/`$ref`;
- `{note: ["string", "null"]}`;
- the verbatim `fs.read` descriptor schema.

### Agents run on the server through a Verifier step: valid rows only

These rows ran through the rejected Verifier detour, so they are evidence and
not a design to copy. Only the trials with tools are valid. The tool-less
trials failed on a setup error and are discarded.

| Trial | Prompt | Tools | Result |
|---|---|---|---|
| `ho-native-tools` | OpenAI `chat_completion`, `reasoning_effort: none` | fs.read | succeeded, tool called |
| `ho-native-tools-output` | the same plus strict output | fs.read | succeeded, structured `findings` decoded |
| `ho-native-cards-list` | the same | cards.list | succeeded |
| `ho-native-bifrost-query` | the same | bifrost.query | succeeded |
| `ho-native-code-run` | the same | code.run | succeeded |
| `ho-responses-tools` | OpenAI `responses` | fs.read | succeeded |
| `ho-tool-*` (7 trials) | OpenAI portable | each built-in | `SKALD_PROVIDERS_400_BAD_REQUEST: Wyrd gateway answered HTTP 400` (OAI-1, OBS-1) |
| `hg-tool-*` (7 trials) | Gemini portable | each built-in | `SKALD_PROVIDERS_400_BAD_REQUEST: Wyrd gateway answered HTTP 400` (GEM-1, OBS-1) |

