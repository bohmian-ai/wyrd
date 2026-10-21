# Changelog

## Unreleased

- wyrd-cli: `wyrd init` replaces `wyrd server dev`, with no alias; scripts
  must use the new name. Startup, saved settings, and failures are unchanged.
  `wyrd init --print-prompt` prints the release's agent setup prompt without
  a server, `--format json` readiness carries it as `setup_prompt`, and a
  ready run prints the command that starts each connected coding agent on it.
  The server serves the same prompt as the MCP prompt `setup`.
- wyrd-ai (Python): `wyrd.integrations.asgi.WyrdMiddleware` runs a loaded
  `WyrdState` with an ASGI app's lifespan and opens one Run per HTTP request
  or WebSocket connection.
- wyrd-spec: the `Spec` JSON Schema now lists Card spec variants under
  `anyOf`, so strict MCP clients accept `cards.get` results.
- wyrd-server: the HTTP listener now defaults to `127.0.0.1:8080` instead of
  `0.0.0.0:8080`, matching the gRPC default. A server reached directly rather
  than through a same-host proxy must set `WYRD_SERVER_BIND`, for example
  `0.0.0.0:8080`. Listener collision checks now also reject a wildcard bind
  that overlaps another listener's port.
- skald-agentic: `skald-agent` (live agent, bounded tool loop, and `SKALD_AGENT_*` catalog) and `skald-workflow` (explicit-binding
  Agent DAG with native, Wyrd gateway, and bound external gateway routes,
  bounded execution with retries, deadlines, and cancellation, and the
  portable `WorkflowRun` result with stable Wyrd workflow codes) land as
  independent skald sub-package crates. Observability is plain payload-free
  `tracing` spans, and skald does not depend on Wyrd server or `vala-*`
  crates.
- skald-workflow-runtime: removed the Skald Observer system (`skald-observer`,
  Agent/Workflow observer hooks, and Python `Observer`/`OtelObserver`/
  `observers=`). Workflow and Agent runs emit payload-free `tracing` spans.

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
