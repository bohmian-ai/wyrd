# PyO3 Boundaries

PyO3 is a boundary layer. Keep Wyrd core crates Rust-native and translate
Python inputs into domain types before calling core behavior.

## Placement

`wyrd-spec` is PyO3-free: no `python` feature, no PyO3 imports, no
Python-lifetime types.

New and materially relocated PyO3 code belongs in `sdks/wyrd-sdk-python`,
which wraps `wyrd-client`. Existing owner-crate `python` features may remain
during the integration as migration state. Only `wyrd-sdk-python` enables and
aggregates them; `wyrd-sdk-rust` and `wyrd-sdk-ts` do not. Core crates remain
usable without Python. The long-term target is to consolidate all Python logic
in the Python SDK, but do not move otherwise untouched wrappers solely to
achieve directory purity.

```toml
[features]
python = ["dep:pyo3"]

[dependencies]
pyo3 = { workspace = true, optional = true }
```

`sdks/wyrd-sdk-python` is the extension-module aggregator and public package. It
must not duplicate validation, lifecycle, registry, storage, transport, or
runtime logic. `check:pyo3-scope` enforces the foundational exclusions and the
approved optional-feature boundary.

The Python testing extra may expose the `WyrdTestServer` harness, but test-tier
behavior must never be enabled on production Python wheels. Enforced by
`mise run check:py-wheel-no-testing`.

## Modern API

Use `Bound<'py, T>` for new PyO3 code:

```rust
#[cfg(feature = "python")]
pub fn register(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<CardRef>()?;
    Ok(())
}
```

Use `Py<T>` for objects that must outlive the current GIL-bound scope.
**Never hold `Bound<'py, T>` across `.await`.**

## Constructor Naming

The `#[new]` method **must** be named `fn __new__` (not `fn new`) and carry
an explicit `#[pyo3(signature = (...))]`. PyO3 generates the Python
`__new__` slot from the `#[new]` attribute regardless of the Rust fn name —
Wyrd standardizes on `__new__` so the Rust source mirrors the Python surface
it exposes.

Canonical target example — `sdks/wyrd-sdk-python/src/cards/card_ref.rs`:

```rust
#[cfg(feature = "python")]
#[pymethods]
impl CardRef {
    #[new]
    #[pyo3(signature = (kind, name, version, *, space=None, uid=None))]
    fn __new__(
        kind: &Bound<'_, PyAny>,
        name: &str,
        version: &str,
        space: Option<&str>,
        uid: Option<&str>,
    ) -> PyResult<Self> {
        let parsed_kind = parse_kind_input(kind).map_err(wyrd_error_to_py_err)?;
        let parsed_name = CardName::new(name)
            .map_err(|error| wyrd_error_to_py_err(invalid_identity("name", name, error)))?;
        let parsed_version = VersionBlock::parse(version)
            .map_err(|error| wyrd_error_to_py_err(invalid_identity("version", version, error)))?;
        // ... call Rust-native builder ...
        Ok(Self { /* fields */ })
    }
}
```

A subclassable base uses the `__new__` + `__init__` initializer pair (PyO3
0.28), not an abstract-`#[new]`-that-raises workaround. Builder-only
pyclasses constructed via `#[staticmethod]` take no `#[new]`.

## Conversion Pattern

Good boundary shape:

1. Extract Python input into primitive Rust values or domain newtypes.
2. Validate at construction.
3. Call Rust-native core APIs.
4. Convert Rust errors into typed Python exceptions.
5. Return Python-friendly values.

Do not pass `PyAny`, `PyDict`, `PyList`, or `PyErr` into reusable Rust core
code outside an approved `python` feature boundary.

## Error Conversion At The Edge

Do not store `PyErr` in reusable Rust errors. Convert `WyrdError` (or a
crate-local `thiserror` enum) to the structured Python exception hierarchy at
the boundary. The shared helper must preserve the RFC 9457 projection rather
than collapsing errors into Python's generic value/runtime classes:

```rust
#[cfg(feature = "python")]
pub fn wyrd_error_to_py_err(err: WyrdError) -> pyo3::PyErr {
    wyrd_utils::py::wyrd_error_to_py_err(err)
}
```

The Python exception exposes `code`, `message`, `details`, `remediation`,
`status`, `title`, `type`, and the full `problem` payload as attributes. Domain
subclasses may improve `except` ergonomics, but callers distinguish durable
failure contracts by `code`, not by parsing text.

## Nested `#[pyclass]` Fields

If a `#[pyclass]` struct has a field whose type is itself a `#[pyclass]`,
do **not** put `#[pyo3(get, set)]` on that field. PyO3-generated accessors
can leak Python lifetimes into pure Rust call sites and tests.

Prefer manual `#[getter]` / `#[setter]` using `IntoPyObjectExt` and
`extract`:

```rust
#[getter]
pub fn inner<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    Ok(self.inner.clone().into_bound_py_any(py)?)
}

#[setter]
pub fn set_inner(&mut self, inner: &Bound<'_, PyAny>) -> PyResult<()> {
    self.inner = inner.extract::<InnerKind>()?;
    Ok(())
}
```

Also avoid `#[pyo3(get)]` on owned `String`, `Vec<T>`, or `HashMap<K, V>`
fields in hot paths — prefer manual accessors returning borrow-equivalent
values.

## GIL Discipline

- Acquire the GIL only when working with Python objects.
- Detach from the interpreter for blocking disk or network work that is not
  already routed through async infrastructure (`py.detach(|| ...)`).
- Reacquire the GIL in spawned work only when converting back to Python
  objects.
- Batch Python object work into one GIL acquisition when practical.

## Async Bridge

Use the shared `wyrd-runtime` bridge for sync/async bridging. Do not create
a new Tokio runtime inside `#[pymethods]`, helper functions, or module
registration.

Python-visible async behavior should be designed as a Python API first, but
the work must still execute in Rust-owned async code.

## Module Registration

New Python-visible Rust functions/classes must be wired through
`sdks/wyrd-sdk-python/src` and its focused wrapper/registration modules. Keep native
extension topology distinct from the public Python
package:

- `agent` (from `skald-agent` + `skald-workflow`)
- `cards` (from `wyrd-cards`, including native `data`, `model`, and `prompt`
  children used to assemble public projections)
- `tool` (from `skald-tool`)
- `prompt` (from `skald-prompt`)
- `providers` (from `skald-runtime`)
- `bifrost` (one projection of `wyrd_client::Bifrost`)
- `observe` (from `vala-sdk::observe` + `skald-observer`)
- `testing` (from `wyrd-testing`, feature-gated, dev-only wheel)

Users import the package projections such as `wyrd.cards`, `wyrd.data`,
`wyrd.model`, `wyrd.prompt`, `wyrd.state`, and `wyrd.bifrost`; they do not
depend on the private
`wyrd._wyrd.cards.*` registration tree.

Do not stop after adding `#[pyclass]`; registration, Python package
exports, generated stubs, and Python tests are all part of the public API
surface (see [Python API and stubs](python-api-and-stubs.md)).
