# Skald Architecture

Skald is Wyrd's LLM runtime crate family. It stays provider-native and depends
on neutral infrastructure plus Skald crates only. Wyrd and Vala depend on Skald;
Skald does not depend on `wyrd-*` or `vala-*`.

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
- `skald-agent`: live agent runtime. It owns `Agent`, `AgentDef`, the bounded
  tool loop, `Observer`, `AgentTool`, `ToolRegistry`, and the
  `SKALD_AGENT_*` error catalog.
- `skald-workflow`: workflow runtime. It owns `Workflow`, `WorkflowDef`,
  `Task`/`TaskDef`, `Context`, the DAG executor, `execute_task`,
  `MessageConversion`-routed handoff, and stable Wyrd workflow error codes.
- `skald-prompt`: Python authoring boundary. It builds native prompt/request
  values and exposes the `wyrd.prompt.Prompt` Python class.

## Dependency Direction

`skald-spec` is the bottom crate. It has zero Wyrd dependencies. Wyrd contracts
that need prompt/provider shapes depend upward on `skald-spec`; Skald engine
crates do not depend on `wyrd-spec`.

The dependency layers are:

```text
skald-workflow -> skald-agent -> skald-runtime
                                  |
                                  v
                {skald-providers, skald-cache, skald-tool} -> skald-spec
                                                           ^
                                                           |
                                                skald-prompt
```

`skald-prompt` is the Python authoring boundary and uses Wyrd Python
error/stub utilities. The runtime, providers, cache, tool, spec, agent, and
workflow crates remain Rust-native engine crates.

## Runtime Boundary

`skald-runtime` is internal Rust in PR3. It has no Python module and no
`SkaldRuntime` pyclass. Runtime dispatch returns `ProviderResponse`; callers
read provider output through `ResponseAdapter`.

`MockProvider` is a public `skald-runtime` type so runtime behavior can be
tested without credentials or live provider calls.

`skald-agent` and `skald-workflow` observe runtime activity through the injected
`Observer` trait and `tracing` spans. They never link `vala-client`; Wyrd/Vala
consumers provide observer implementations from their own layer.

## PyO3 Scope

Only the authoring and Wyrd-card boundary crates opt into Python:

- `skald-prompt`, for the ergonomic Python `Prompt` builder and helper classes;
- `wyrd-cards`, for the local `PromptCard` holder.

`skald-spec`, `skald-cache`, `skald-tool`, `skald-providers`, `skald-runtime`,
`skald-agent`, `skald-workflow`, and `wyrd-spec` remain PyO3-free at the engine.
Their manifests may carry optional Python feature scaffolding, but source-level
PyO3 belongs outside the engine path.
