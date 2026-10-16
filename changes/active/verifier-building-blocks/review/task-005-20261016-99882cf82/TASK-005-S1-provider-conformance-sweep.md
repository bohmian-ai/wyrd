---
id: TASK-005-S1
kind: supplemental-implementation
status: ready
parent_task: TASK-005
---

# TASK-005-S1: Live provider sweep

Route to `$wyrd-implement`.

## Problem

We have not validated, against live providers, that:

1. the Wyrd gateway relays native provider calls correctly; and
2. Wyrd Prompts, portable and native, lower to requests the current provider
   models accept.

## Outcome

The existing opt-in live smoke lane grows into a per-provider sweep that a
maintainer runs locally with their own keys:

```bash
mise run test:gateway:smoke:live
```

Keys come from the maintainer's untracked `mise.local.toml` (already in
`.gitignore`), for example:

```toml
[env]
OPENAI_API_KEY    = "{{exec(command='pass show dev/openai/api-key')}}"
GEMINI_API_KEY    = "{{exec(command='pass show dev/google/api-key')}}"
ANTHROPIC_API_KEY = "{{exec(command='pass show dev/anthropic/api-key')}}"
TYPESAFE_API_KEY  = "{{exec(command='pass show dev/jev/api-key')}}"
```

A provider without a key is skipped with a message naming the variables.
pytest's output is the report: each case passes or fails with the provider's
own status and message.

## Scope

Home: `sdks/wyrd-sdk-python/tests/smoke/`.

| Part | Question | How it calls |
|---|---|---|
| 1 | Does the gateway relay native calls? | Each provider's own client (`openai`, `anthropic`, `google.genai`, Jev) pointed at a live `WyrdTestServer(live_providers=True)` gateway |
| 2 | Do Wyrd Prompts lower to requests the current models accept? | Public `wyrd.agent.Agent` with a portable or native `wyrd.prompt.Prompt`, on its `native` route, straight to the provider (no gateway). This isolates a Prompt-format defect from a gateway defect. |
| 3 | Do Wyrd Agents work through the gateway, with tools? | **Blocked.** The SDK `Agent` cannot hold a `WyrdClient` or use Wyrd MCP tools yet. See `changes/active/agent-runtime-composition/spec.md`. This task writes no part-3 cases. |

### Models

The current models, each overridable with `WYRD_SMOKE_<PROVIDER>_MODEL`:

| Provider | Default model |
|---|---|
| OpenAI | `gpt-6-luna` |
| Anthropic | `claude-haiku-5-5` |
| Gemini | `gemini-3.8-flash` |
| Jev | `jev-latest` |

The existing OpenAI operation-family models (embeddings, speech,
transcription, images) stay as they are.

### Part 1: native calls through the gateway

For OpenAI (Chat Completions), Anthropic (Messages) and Gemini
(GenerateContent), each through its own client:

- plain text;
- system instruction;
- streaming;
- one forced function tool call that returns a parseable call;
- native structured output that returns parseable JSON.

The existing cases also stay:

- the OpenAI-compatible client against each provider;
- OpenAI Responses, embeddings, audio, images and batch;
- the Jev typed questions.

### Part 2: Prompt lowering per provider

Portable Prompts cover the common use cases **for that provider's current
model**, not every setting on every model. A setting the model itself
refuses, such as `temperature` other than 1 on `gpt-6-luna`, is not a
portable case for that provider.

Portable cases, for OpenAI, Anthropic and Gemini:

- text;
- system plus a multi-turn `messages` history;
- `max_output_tokens`;
- `response_format`, a JSON Schema that each provider accepts in its
  structured-output field, decoded into `AgentRun.structured_output`;
- one local `@tool` call completing the Agent loop;
- `reasoning_effort` on OpenAI, where the model needs it to accept tools.

Native cases, one Prompt per format:

- `chat_completion` and `responses` (OpenAI);
- `messages` (Anthropic);
- `generate_content` (Gemini);
- `system_one` (Jev).

Pass means the `AgentRun` finished without error and returned the expected
shape: text, structured output or a completed tool call. A failing case
stays failing. It is the finding. Do not use xfail, skip-on-error or
allowlists to make it green.

## Constraints

- **Touch only:**
  - `sdks/wyrd-sdk-python/tests/smoke/`;
  - the `test:gateway:smoke:live` description in `mise.toml`;
  - this packet;
  - `findings.md` beside it.
- No product code changes. Defects found are recorded, not fixed.
- Reuse the lane's existing `credential`, `submit`, `caller` and
  `live_server` helpers. Move them to `conftest.py` only if more than one
  module needs them. Boot one server per provider per module, not one per
  test.
- Credentials are never printed, logged or written, as the lane already
  guarantees.
- Python tests use top-level `def test_*` only.
- The lane stays outside every gate and CI workflow.

## Done when

1. `mise run test:gateway:smoke:live` runs every part 1 and part 2 case for
   each provider with a key, and skips each provider without one.
2. `mise run py:format`, `mise run py:lints` and `mise run py:typecheck`
   pass.
3. The run's results are recorded in `findings.md` beside this packet,
   replacing the 2026-10-16 seed rows: per provider and per case, pass or
   fail with the provider's message. Each failure is attributed to a provider
   limit, Prompt lowering or the gateway.
