# Wyrd

The AI Operational Layer: the managed control plane for every AI capability a company runs.

> Pre-1.0.

## Quickstart

```console
mise install
mise run dev:full
# UI: http://localhost:3000
# wyrd-server: http://localhost:8080/healthz
```

## Layout

- `crates/wyrd/` - registry, cards, services, CLI, MCP, UI, server.
- `crates/vala/` - observability, evaluation, traces, and analytical storage.
- `crates/skald/` - LLM providers, prompts, agents, and workflows.
- `crates/shared/` - shared infrastructure.
- `python/py-wyrd/` - Python wheel source.

## License

Apache-2.0. See `LICENSE.md`.
