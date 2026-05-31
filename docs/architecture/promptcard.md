# PromptCard Architecture

PR3 makes PromptCard native-canonical. There is one prompt/provider type set:
the `skald-spec` native `Prompt`, `ProviderRequest`, `ProviderResponse`, and
provider wire structs. Wyrd does not store a second neutral prompt model and
does not project from a Wyrd-owned message/content shape.

## Native Prompt Flow

`PromptCard` stores a `wyrd_spec::card::prompt::PromptSpec`, and that spec wraps
one native `skald_spec::Prompt`. The prompt's `request` is already the provider
request shape that runtime dispatch and proxy paths use.

`Prompt::render` performs declared `{{name}}` text substitution inside the
native provider request. The request variant is the same before and after
rendering: OpenAI Chat stays OpenAI Chat, Anthropic stays Anthropic, Gemini
stays Gemini, Vertex stays Vertex, and `RawV1` stays `RawV1`. Rendering is not a
compile step.

Provider responses are read through `ResponseAdapter`. It is the read surface
for text, tool calls, structured output, usage, and finish reason over native
provider responses. Runtime returns `ProviderResponse`; it does not wrap results
in a neutral run output.

## Envelope Invariants

The PromptCard envelope owns Wyrd card identity, declared variables, validation,
content hashing, local save/load materialization, and references. It does not
own provider message/content models.

PromptCard validation enforces:

- variable names use the Wyrd prompt parameter grammar;
- declared text variables are unique;
- every `{{name}}` text placeholder is declared;
- every declared text variable is referenced;
- model is non-empty;
- JSON schema response formats carry an object schema;
- every `${media:name}` media placeholder is declared;
- every declared media variable is referenced.

S10A media binding keeps media placeholders on the explicit `${media:name}`
lane. Media placeholders are isolated before native media replacement, system
message media placement is rejected where providers cannot support it, and media
binding writes provider-native blocks directly.

## Non-Goals

PR3 intentionally does not add:

- a neutral prompt/message/content/provider model;
- projection helpers or provider wire projection;
- `PromptWireSerializer` or `DefaultWireSerializer`;
- `RunOutput`;
- `Prompt.register()` or `PromptCard.register()`.

Registration remains a registry/client responsibility. Card instances only own
local holder behavior such as `save`, `load`, `model_dump_json`, and
`model_validate_json`.
