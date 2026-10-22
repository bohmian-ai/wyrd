# wyrd-sdk-python

Rules for the Python package and its PyO3 boundary. The root `AGENTS.md` still
applies. The package is a typed client over server contracts and Rust-owned
behavior: it may add authoring helpers, OTEL hooks, agent integrations, stubs,
and tests, but never duplicates validation, lifecycle, registry, storage,
transport, or runtime logic, and never becomes the source of truth for API
behavior.

## Placement

- `wyrd-spec` is PyO3-free: no `python` feature, no PyO3 imports, no
  `Python<'py>`, `Bound<'py, T>`, `Py<T>`, or `PyErr`.
- New and materially relocated PyO3 code lives under `src/`, behind the
  boundary feature with `pyo3 = { workspace = true, optional = true }`.
- Existing owner-crate `python` features (for example
  `crates/wyrd/wyrd-cards/src/card_ref.rs`) are migration state. Only this
  package enables them; the Rust and TypeScript SDKs never do. Do not move an
  untouched wrapper only for directory purity.
- `crates/wyrd/wyrd-interfaces` (Python-only framework adapters) is the same
  kind of optional-feature migration boundary: only this package enables it,
  and its long-term home is here.
- Keep Python-lifetime types out of crates that did not opt into `python`.
- The `testing` extra may expose `WyrdTestServer`; production wheels never
  enable it (`mise run check:py-wheel-no-testing`).

## PyO3 code

- Use `Bound<'py, T>` for new code. Convert to `Py<T>` before storing an object
  across awaits, threads, or long-lived state. Never hold `Bound<'py, T>`
  across `.await`.
- Name the `#[new]` method `fn __new__` and give it an explicit
  `#[pyo3(signature = (...))]`, so the Rust source mirrors the Python slot.
  Builder-only pyclasses built through `#[staticmethod]` take no `#[new]`. A
  subclassable base uses the `__new__` + `__init__` pair, not a `#[new]` that
  raises.
- Boundary shape: extract Python input into primitives or domain newtypes,
  validate at construction, call the Rust-native API, convert errors at the
  edge, return Python-friendly values. Never pass `PyAny`, `PyDict`, `PyList`,
  or `PyErr` into reusable Rust code.

```rust
#[new]
#[pyo3(signature = (kind, name, version, *, uid=None))]
fn __new__(
    kind: &Bound<'_, PyAny>,
    name: &str,
    version: &str,
    uid: Option<&str>,
) -> WyrdPyResult<Self> {
    let parsed_kind = parse_kind_input(kind)?;
    let parsed_name = CardName::new(name)
        .map_err(|error| WyrdPyError::from(invalid_identity("name", name, error)))?;
    // ... parse the rest, then build the Rust-native CardRef ...
}
```

- Do not put `#[pyo3(get, set)]` on a field whose type is another `#[pyclass]`;
  write manual `#[getter]`/`#[setter]` with `into_bound_py_any` and `extract`.
  Avoid `#[pyo3(get)]` on owned `String`, `Vec<T>`, or `HashMap<K, V>` in hot
  paths; return borrow-equivalent values from a manual accessor.
- Hold the GIL only while touching Python objects. Detach (`py.detach(|| ...)`)
  for blocking disk or network work not already routed through async
  infrastructure; batch Python object work into one acquisition.
- Bridge sync/async only through the shared `wyrd-runtime` runtime. Never
  create a Tokio runtime in `#[pymethods]`, helpers, or module registration.

## Errors

- Never store `PyErr` in reusable Rust errors. Keep typed Rust errors and
  convert at the edge through `wyrd_utils::py` (`WyrdPyError`,
  `WyrdPyResult`, `wyrd_error_to_py_err`), which preserves the RFC 9457
  projection.
- `WyrdError` is the one structured Python exception root. It exposes `code`,
  `message`, `detail`, `details`, `remediation`, `status`, `title`, and `type`;
  there is no aggregate `problem` attribute. Every expected Wyrd failure,
  including Bifrost ingest, flush, shutdown, query, and stream failures, raises
  it; callers branch on `code`, never on message text.
- Do not publish aliases that only rename `WyrdError`. Keep a safe,
  string-backed `__cause__` at the edge.

## Public API: six layers

Every Python-visible change touches all of these:

1. The Rust type or function in its Rust-native owning crate.
2. The PyO3 wrapper under `src/` (or an approved owner-crate wrapper).
3. Native registration under `src/`.
4. Package exports under `python/wyrd`.
5. The authored `.pyi` stubs, passing `mise run py:lints` and
   `mise run py:typecheck`.
6. Python tests that import public `wyrd` modules.

Do not leave a type reachable only through a private extension path unless
that is the intended contract.

Native registration (`wyrd._wyrd.*`) may nest differently from the public
package. Users import `wyrd.cards`, `wyrd.data`, `wyrd.model`, `wyrd.prompt`,
`wyrd.state`, `wyrd.bifrost`, `wyrd.observe`; public imports and stubs must
agree. `wyrd.bifrost` projects `wyrd_client::Bifrost` and never constructs
separate write and query clients. `wyrd.state.WyrdState` is the local,
non-network hydrated graph.

## Stubs

- The package-local `.pyi` files, including `_wyrd.pyi`, are the one authored
  declaration of the native API. Edit them in the same change as the native
  API; there is no generator or second stub tree.
- Declare a pure-Python definition once, inline, and re-export it from the
  stub. Declare a shared native type once and import it elsewhere.
- `__all__` in each `__init__.pyi` matches its `__init__.py`.
- Document every public class, constructor (as `__init__`), method, and
  function with an `Args:` entry per parameter. `scripts/check_stubs.py`
  (run by `py:lints`) enforces docs, export parity, and layout.
- Use precise types; reserve `Any` for genuinely open input.
- Typechecking does not prove native signature parity; keep runtime and export
  tests.

## Python code

- Preserve server field names for durable data; no Python-only vocabulary for
  stored facts.
- No hidden IO in constructors, `__repr__`, or properties. A server-facing
  handle's constructor retains its arguments and connects on first use; its
  `connect` staticmethod (classmethod on the pure-Python Bifrost facades)
  connects eagerly through the same Rust path.
- No global mutable state. Typed dataclasses or Pydantic models only where
  Python owns the user-facing shape.
- Format with `mise run py:format`; lint with `mise run py:lints`.

## Tests

- Top-level `def test_*` only; never `class TestFoo:`.
- Import public `wyrd.*`; construct values the way users do; assert typed
  exceptions and Wyrd error codes.
- No credentials or live services in unit tests (`mise run py:test:unit`).
- User-facing capabilities ship an `integration`-marked journey against
  `WyrdTestServer` from `wyrd.testing` (dev wheel only).
- Python-lifetime behavior belongs here; pure Rust logic belongs in Rust tests.
  A Python test that exists only to exercise Rust-only behavior means the
  boundary is misplaced.
