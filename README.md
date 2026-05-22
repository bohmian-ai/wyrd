# Wyrd

The AI layer: the managed control plane for every AI capability a company runs.

> Pre-1.0.

## Quickstart

```console
mise install
mise run dev:full
# UI: http://localhost:3000
# wyrd-server: http://localhost:8080/healthz
# wyrd-observability-server: http://localhost:8090/healthz
```

## Layout

- `crates/wyrd-*` - Wyrd contracts, shared infrastructure, CLI, MCP, UI, and server crates.
- `crates/vala/` - observability, evaluation, traces, and analytical storage.
- `crates/skald/` - LLM providers, prompts, and agents.
- `python/py-wyrd/` - Python wheel source.

## License

Apache-2.0. See `LICENSE.md`.
