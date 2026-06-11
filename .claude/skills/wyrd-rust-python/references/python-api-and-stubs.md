# Python API And Stubs

The Python package is a typed, ergonomic surface over Rust-owned behavior. Do
not duplicate core logic in Python.

## Export Checklist

For every Python-visible change:

1. Implement or update the Rust behavior in the owning crate.
2. Add or update the PyO3 wrapper in that owning crate behind its optional
   `python` feature.
3. Register the class, function, or submodule from `python/py-wyrd/src`.
4. Export the public symbol from `python/py-wyrd/python/wyrd`.
5. Regenerate stubs through the repo task.
6. Add or update Python tests that import from public `wyrd` modules.

Do not leave a type reachable only through a private extension path unless that
is intentionally the public shape for the feature.

## Stub Rules

- Do not hand-edit generated stubs.
- Fix source annotations or generator behavior, then regenerate.
- Stubs must match runtime imports.
- Public signatures should use precise types, not broad `Any`, unless the API
  truly accepts arbitrary input.

## Python Code Style

- Keep helpers small and ergonomic.
- Use typed dataclasses or Pydantic models only when Python owns the user-facing
  shape.
- Avoid hidden IO in constructors.
- Avoid global mutable state.
- Use `uv run` through repo tasks when available.

## Test Shape

Python tests should model real user workflows:

- import public `wyrd` symbols
- construct values the way users do
- assert typed exceptions and error codes
- verify stubs/imports when public APIs change
- avoid credentials and live external services in unit tests

If Python tests are required just to test Rust-only behavior, the boundary is
probably misplaced. Python-lifetime behavior belongs in Python tests; pure Rust
logic belongs in Rust tests.
