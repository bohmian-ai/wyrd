# PyO3 Boundaries

PyO3 is a boundary layer. Keep Wyrd core crates Rust-native and translate
Python inputs into domain types before calling core behavior.

## Placement

`wyrd-spec` is PyO3-free: no `python` feature, no PyO3 imports, no
Python-lifetime types.

PyO3 code belongs in the crate that owns the Python-visible behavior, behind
an optional `python` feature:

```toml
[features]
python = ["dep:pyo3"]

[dependencies]
pyo3 = { workspace = true, optional = true }
```

`python/py-wyrd` is the extension-module aggregator. It enables approved
owner-crate `python` features and registers their submodules; it must not
duplicate validation, lifecycle, registry, storage, or runtime logic.

Approved owner crates are `wyrd-interfaces`, `wyrd-cards`, `wyrd-config`,
`wyrd-utils`, `vala-sdk`, `skald-observer`, `skald-prompt`, `skald-runtime`,
`skald-agent`, `skald-tool`, `skald-workflow`, and `wyrd-testing`.
`check:pyo3-scope` enforces the foundational, shared, and Skald exclusions.
Approved Wyrd and Vala owner features are additionally verified through crate
manifests, `python/py-wyrd` feature wiring and registration, code generation,
and public import tests. Do not add PyO3 to another crate without changing the
architecture and its applicable verification.

`wyrd-testing`'s `python` feature exposes the `WyrdTestServer` harness only;
it is a test-tier crate and must never be enabled on production Python
wheels. Enforced by `mise run check:py-wheel-no-testing`.

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

Canonical example — `crates/wyrd/wyrd-cards/src/card_ref.rs`:

```rust
#[cfg(feature = "python")]
#[pymethods]
impl CardRef {
    #[new]
    #[pyo3(signature = (kind, name, version, *, space, uid=None))]
    fn __new__(
        kind: &Bound<'_, PyAny>,
        name: &str,
        version: &str,
        space: &str,
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

The Python exception exposes `code`, `message`, `detail`, `details`,
`remediation`, `status`, `title`, and `type` as attributes. It does not expose
the aggregate RFC 9457 payload as a separate `problem` attribute. Domain
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
`python/py-wyrd/src/lib.rs` and the owning crate's submodule registration
function. Keep native extension topology distinct from the public Python
package:

- `agent` (from `skald-agent` + `skald-workflow`)
- `cards` (from `wyrd-cards`, including native `data`, `model`, and `prompt`
  children used to assemble public projections)
- `tool` (from `skald-tool`)
- `prompt` (from `skald-prompt`)
- `providers` (from `skald-runtime`)
- `bifrost` (from `vala-sdk::bifrost`)
- `observe` (from `vala-sdk::observe` + `skald-observer`)
- `testing` (from `wyrd-testing`, feature-gated, dev-only wheel)

Users import the package projections such as `wyrd.cards`, `wyrd.data`,
`wyrd.model`, and `wyrd.prompt`; they do not depend on the private
`wyrd._wyrd.cards.*` registration tree.

Do not stop after adding `#[pyclass]`; registration, Python package
exports, generated stubs, and Python tests are all part of the public API
surface (see [Python API and stubs](python-api-and-stubs.md)).
