# Changelog

## Unreleased

- skald-agentic: `skald-agent` (live agent, bounded tool loop, Observer hook,
  and `SKALD_AGENT_*` catalog) and `skald-workflow` (DAG executor,
  `execute_task`, retries, and cross-provider handoff via shared
  `MessageConversion` with stable Wyrd workflow validation codes) land as independent
  skald sub-package crates. The engine remains PyO3-free, observability is
  injected through the `Observer` trait and tracing spans, and skald does not
  depend on `wyrd-*` or `vala-*`.
- workflow-card-docs: replaced global Python observer installation with
  `Workflow(observers=[...])`, refreshed workflow/agent docs, stubs, and
  examples, and removed public `WyrdInstrumentor` / `set_observer`.

## PR3 - PromptCard + Skald Native-Canonical Surface

- Added the native-canonical `skald-spec` prompt/provider contract with native
  `ProviderRequest`, `ProviderResponse`, `MessageNum`, `Prompt`,
  `ResponseAdapter`, media references, and `SKALD_*` errors.
- Added the `wyrd-spec` PromptCard envelope that wraps native
  `skald_spec::Prompt`, validates PromptCard invariants, computes content
  hashes, provides `PromptRef`, and keeps `wyrd-spec` PyO3-free, IO-free,
  async-free, and wasm-safe.
- Added the `skald-prompt` Python authoring builder for native prompt creation
  through `wyrd.prompt.Prompt`.
- Added the `wyrd-cards::PromptCard` local holder with local save/load and JSON
  string round-trip behavior.
- Added provider/cache/tool/runtime crates for native provider clients, cache
  keys, tool declarations, runtime dispatch, and public Rust `MockProvider`.
- Kept PR3 free of neutral prompt/message/provider models,
  `PromptWireSerializer`, projection helpers, `RunOutput`,
  `Prompt.register()`, and `PromptCard.register()`.
