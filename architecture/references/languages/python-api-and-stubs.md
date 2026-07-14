# Python API And Stubs

The Python package is a first-class typed client over Wyrd server contracts
and Rust-owned local boundary behavior. It may provide ergonomic authoring
helpers, OTEL hooks, agent workflow integrations, generated stubs, and
Python-native tests, but it must not duplicate durable server logic or
become the source of truth for API behavior.

## Export Checklist

For every Python-visible change, verify all six layers:

1. Rust type/function exists in the owning crate.
2. PyO3 wrapper or registration exists in that owning crate behind its
   optional `python` feature.
3. `python/py-wyrd/src` registers the owning crate's submodule or exported
   class/function.
4. Python package exports exist under `python/py-wyrd/python/wyrd`.
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
python/py-wyrd/python/wyrd/
├── __init__.py
├── __init__.pyi
├── _wyrd.pyi              # extension-module stub
├── agent/                 # skald-agent, skald-workflow surface
├── bifrost/               # vala-sdk bifrost sink + query surface
├── cards/                 # wyrd-cards Card wrappers
│   ├── data/
│   ├── model/
│   └── prompt/
├── config/                # wyrd-config
├── observer.pyi
├── observe/               # vala-sdk observe + wyrd-observe surface
├── prompt/                # skald-prompt
├── providers/             # skald-runtime
├── testing/               # feature-gated, dev-only wheel
└── tool/                  # skald-tool
```

`_wyrd` (extension module) is the private surface; users import from
`wyrd.*` public modules.

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
`references/testing-workflows.md` §Tiers). Unit tests do not substitute
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
