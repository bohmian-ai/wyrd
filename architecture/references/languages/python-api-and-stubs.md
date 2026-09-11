# Python API And Stubs

The Python package is a first-class typed client over Wyrd server contracts
and Rust-owned local boundary behavior. It may provide ergonomic authoring
helpers, OTEL hooks, agent workflow integrations, generated stubs, and
Python-native tests, but it must not duplicate durable server logic or
become the source of truth for API behavior.

## Export Checklist

For every Python-visible change, verify all six layers:

1. Rust type/function exists in its Rust-native owning crate.
2. New or materially relocated PyO3 wrappers live under
   `sdks/wyrd-sdk-python/src`; existing owner-crate wrappers may remain as
   approved migration state.
3. `sdks/wyrd-sdk-python/src` registers the native wrapper or exported
   class/function.
4. Python package exports exist under `sdks/wyrd-sdk-python/python/wyrd`.
5. Generated stubs regenerate cleanly (`mise run codegen:check`).
6. Python tests import from public `wyrd` modules, not private extension
   paths, unless the private path is the intended contract.

Do not leave a type reachable only through a private extension path unless
that is intentionally the public shape.

## Stub Rules

- Do not hand-edit generated stubs.
- Fix source annotations or generator behavior, then regenerate via
  `mise run codegen:stubs`.
- Stubs must match runtime imports.
- Public signatures should use precise types, not broad `Any`, unless the
  API truly accepts arbitrary input.
- Type-check the stub surface with `mise run py:typecheck` after stub
  regeneration.

## Package Layout

```text
sdks/wyrd-sdk-python/python/wyrd/
├── __init__.py
├── __init__.pyi
├── _wyrd.pyi              # extension-module stub
├── agent/                 # skald-agent, skald-workflow surface
├── bifrost/               # wyrd-client Bifrost surface
├── cards/                 # CardKind, CardRef, and shared card surface
├── config/                # public Wyrd configuration projection
├── data/                  # public DataCard projection
├── errors.py              # structured Wyrd exception exports
├── model/                 # public ModelCard projection
├── observe/               # wyrd-client observation surface
├── observer.py            # public observer projection
├── otel.py                # OTEL integration helpers
├── prompt/                # public Prompt and PromptCard projection
├── state/                 # WyrdState offline hydrated graph
└── testing/               # feature-gated, dev-only wheel
```

`_wyrd` is the private extension module. Its native registration tree may use
children such as `wyrd._wyrd.cards.data`; users import the public projections
from top-level `wyrd.data`, `wyrd.model`, `wyrd.prompt`, and `wyrd.cards`.
Public imports and generated stubs must agree even when native registration is
nested differently.

`wyrd.bifrost` projects `wyrd_client::Bifrost` for table management,
buffered ingestion, query streaming, and query lifecycle. It does not expose or
construct separate write and query clients. `wyrd.state.WyrdState` remains the
local, non-network hydrated graph loaded from a downloaded Service bundle.

## Python Code Style

- Keep helpers small and ergonomic.
- Use typed dataclasses or Pydantic models only when Python owns the
  user-facing shape.
- Preserve server/API field names for durable contract data; do not invent
  Python-only vocabulary for stored facts.
- Avoid hidden IO in constructors, `__repr__`, or property accessors.
- Avoid global mutable state.
- Use `uv run` through repo tasks when available.
- Format with `ruff` (`mise run py:format`). Lint with `mise run py:lints`.

## Test Shape

Python tests should model real user workflows:

- Import public `wyrd` symbols.
- Construct values the way users do.
- Assert typed exceptions and Wyrd error codes.
- Verify stubs/imports when public APIs change.
- Avoid credentials and live external services in unit tests.

For user-facing capabilities, add a **user-journey** test (see
[Testing workflows](testing-workflows.md)). Unit tests do not substitute
for a missing journey.

## Test Conventions

- Top-level `def test_*` only. Never `class TestFoo:`.
- Import from `wyrd.*` (public) unless the private extension path is the
  intended contract.
- Use fixtures from `wyrd.testing` for `WyrdTestServer`-backed journeys
  (this requires the `wyrd-testing` dev wheel — production wheels must not
  expose `wyrd.testing`; enforced by `check:py-wheel-no-testing`).

If Python tests are required just to test Rust-only behavior, the boundary
is misplaced. Python-lifetime behavior belongs in Python tests; pure Rust
logic belongs in Rust tests.
