# PyO3 Boundaries

PyO3 is a boundary layer. Keep Wyrd core crates Rust-native and translate Python
inputs into domain types before calling core behavior.

## Placement

PyO3 code belongs in `python/py-wyrd*` unless an explicit allowlist says
otherwise. Do not add PyO3 to foundation, client, spec, storage, telemetry, or
provider-core crates without an approved boundary.

## Modern API

Use `Bound<'py, T>` for new PyO3 code:

```rust
fn register(py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<WyrdThing>()?;
    Ok(())
}
```

Use `Py<T>` for objects that must outlive the current GIL-bound scope.

Never hold `Bound<'py, T>` across `.await`.

## Constructor Naming

Name the `#[new]` method `fn __new__`, not `fn new`. PyO3 generates the Python
`__new__` slot from the `#[new]` attribute regardless of the Rust fn name, so the
name is cosmetic — but Wyrd standardizes on `__new__` so the Rust source mirrors
the Python surface it exposes. prior implementation uses `fn new` (its PyO3 predated separate
`__init__` support); Wyrd diverges deliberately. Every Wyrd `#[new]` carries an
explicit `#[pyo3(signature = (...))]`. Builder-only pyclasses (e.g. `Split`,
constructed solely via `#[staticmethod]`) take no `#[new]`.

```rust
#[new]
#[pyo3(signature = (*, data=None, compression="snappy"))]
fn __new__(data: Option<Py<PyAny>>, compression: &str) -> (Self, DataInterface) { ... }
```

A subclassable base uses the `__new__` + `__init__` initializer pair (PyO3 0.28),
not prior implementation's abstract-`#[new]`-that-raises workaround.

## Conversion Pattern

Good boundary shape:

1. Extract Python input into primitive Rust values or domain newtypes.
2. Validate at construction.
3. Call Rust-native core APIs.
4. Convert Rust errors into typed Python exceptions.
5. Return Python-friendly values.

Do not pass `PyAny`, `PyDict`, `PyList`, or `PyErr` into reusable Rust core code.

## GIL Discipline

- Acquire the GIL only when working with Python objects.
- Release the GIL for blocking disk or network work if it is not already routed
  through async infrastructure.
- Reacquire the GIL in spawned work only when converting back to Python objects.
- Batch Python object work into one GIL acquisition when practical.

## Python Object Fields

Avoid automatic getters for owned heap data in hot paths:

- Prefer manual accessors returning `&str` for strings when PyO3 supports the
  shape.
- Prefer explicit conversion for vectors and maps.
- Prefer immutable construction over mutation-heavy Python setters.
- Be careful with nested Python-visible objects; implement manual getters and
  setters when automatic extraction hides clone or lifetime behavior.

## Async Bridge

Use the Wyrd runtime boundary for sync/async bridging. Do not create a new Tokio
runtime inside `#[pymethods]`, helper functions, or module registration.

Python-visible async behavior should be designed as a Python API first, but the
work should still execute in Rust-owned async code.
