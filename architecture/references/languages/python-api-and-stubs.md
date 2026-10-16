# Python API And Stubs

The Python package is a first-class typed client over Wyrd server contracts
and Rust-owned local boundary behavior. It may provide ergonomic authoring
helpers, OTEL hooks, agent workflow integrations, authored stubs, and
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
5. The authored public stubs declare the change and pass `mise run py:lints`
   and `mise run py:typecheck`.
6. Python tests import from public `wyrd` modules, not private extension
   paths, unless the private path is the intended contract.

Do not leave a type reachable only through a private extension path unless
that is intentionally the public shape.

## Stub Rules

- The package-local `.pyi` files, including `_wyrd.pyi`, are the one authored
  declaration of the native API. Edit them directly; there is no stub
  generator or second stub tree.
- Declare a pure-Python definition once, with inline annotations, in a
  stub-less module, and re-export it from the public stub. Do not restate it.
- Declare a shared native type once and import it where other stubs use it.
- Stubs must match runtime imports; `__all__` in each `__init__.pyi` matches its
  `__init__.py`.
- Document every public class, constructor (as `__init__`), method, and
  function with an `Args:` entry per parameter. `mise run py:lints` runs
  `scripts/check_stubs.py` to enforce this, export parity, and package layout.
- Public signatures should use precise types, not broad `Any`, unless the
  API truly accepts arbitrary input.
- Type-check the stub surface with `mise run py:typecheck`. Typechecking does
  not prove native signature parity; keep runtime and export tests.

## Package Layout

```text
sdks/wyrd-sdk-python/python/wyrd/
├── __init__.py
├── __init__.pyi
├── _wyrd.pyi              # extension-module stub
├── agent/                 # skald-agent surface
├── bifrost/               # wyrd-client Bifrost surface
├── cards/                 # CardKind, CardRef, and shared card surface
├── config/                # public Wyrd configuration projection
├── data/                  # public DataCard projection
├── errors.py              # structured Wyrd exception exports
├── model/                 # public ModelCard projection
├── observe/               # wyrd-client observation surface
├── prompt/                # public Prompt and PromptCard projection
├── state/                 # WyrdState offline hydrated graph
└── testing/               # feature-gated, dev-only wheel
```

`_wyrd` is the private extension module. Its native registration tree may use
children such as `wyrd._wyrd.cards.data`; users import the public projections
from top-level `wyrd.data`, `wyrd.model`, `wyrd.prompt`, and `wyrd.cards`.
Public imports and authored stubs must agree even when native registration is
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
- A server-facing handle's constructor retains its arguments and connects on
  its first server operation; its `connect` staticmethod (classmethod on the
  pure-Python Bifrost facades) connects eagerly through the same shared Rust
  path.
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
