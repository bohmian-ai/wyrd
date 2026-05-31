# Skald Architecture

Skald is Wyrd's LLM runtime crate family. PR3 keeps the core Rust crates
provider-native and keeps Python at the authoring/card boundary only.

## Crate Map

- `skald-spec`: bottom contract crate. It owns native
  `ProviderRequest`, `ProviderResponse`, `MessageNum`, provider wire structs,
  `Prompt`, `ResponseAdapter`, `MessageConversion`, media references, and the
  `SKALD_*` error catalog. It is PyO3-free, Wyrd-free, IO-free, async-free, and
  wasm-safe.
- `skald-cache`: cache key derivation and in-memory cache storage over native
  request/cache fields. It depends on `skald-spec` and does not emit provider
  wire directives.
- `skald-tool`: tool declarations and native tool-use/tool-result helpers. It
  does not execute tools.
- `skald-providers`: provider clients, auth, retry, transport, fixtures, and
  streaming decoders. It sends native `ProviderRequest` values and parses native
  `ProviderResponse` values.
- `skald-runtime`: internal Rust dispatch and orchestration. It routes native
  requests to registered providers and returns native `ProviderResponse`.
  `MockProvider` is public here for offline runtime tests.
- `skald-prompt`: Python authoring boundary. It builds native prompt/request
  values and exposes the `wyrd.prompt.Prompt` Python class.

## Dependency Direction

`skald-spec` is the bottom crate. It has zero Wyrd dependencies. Wyrd contracts
that need prompt/provider shapes depend upward on `skald-spec`; Skald engine
crates do not depend on `wyrd-spec`.

The only PR3 Skald-to-Wyrd boundary crate is `skald-prompt`, because it is the
Python authoring layer and uses Wyrd Python error/stub utilities. The runtime,
providers, cache, tool, and spec crates remain Rust-native engine crates.

## Runtime Boundary

`skald-runtime` is internal Rust in PR3. It has no Python module and no
`SkaldRuntime` pyclass. Runtime dispatch returns `ProviderResponse`; callers
read provider output through `ResponseAdapter`.

`MockProvider` is a public `skald-runtime` type so runtime behavior can be
tested without credentials or live provider calls.

## PyO3 Scope

Only two PR3 crates opt into Python:

- `skald-prompt`, for the ergonomic Python `Prompt` builder and helper classes;
- `wyrd-cards`, for the local `PromptCard` holder.

`skald-spec`, `skald-cache`, `skald-tool`, `skald-providers`, `skald-runtime`,
and `wyrd-spec` remain PyO3-free.
