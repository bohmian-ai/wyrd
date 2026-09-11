---
id: SPEC-py-error-refactor
revision: 3
status: approved
---

# One structured Python error boundary

## Objective and user value

Every Wyrd-owned failure exposed by a public Python operation shall raise the
same structured Wyrd error contract. Python callers shall be able to catch the
shared `WyrdError`, inspect one coherent machine-readable error shape, and
act on a stable catalog code regardless of which Wyrd, Skald, or Vala owner
produced the failure.

This change consolidates existing Python error adapters and projections. It
does not add product behavior or require test-driven implementation ordering.

## Scope

- Establish one shared Rust result/error adapter for Wyrd-owned Python
  operations in the existing `wyrd-utils` Python boundary.
- Complete the shared Python `WyrdError` projection from the derive-backed
  catalog.
- Add catalog coverage for stable codes currently maintained by public SDK or
  Python-boundary implementations outside the derive-backed catalog.
- Migrate every approved Python owner crate, the public `py-wyrd` package, and
  the test-tier `WyrdTestServer` surface to the shared adapter.
- Remove owner-local public error projection, duplicated metadata, and raw
  generic Python exceptions for Wyrd-owned failures.
- Regenerate public Python stubs from their owning sources.

## Required behavior

### REQ-001 — One Wyrd-owned Python result path

Every user-callable Python operation implemented by Wyrd that can fail for a
Wyrd-owned reason shall return through one shared adapter owned by
`wyrd-utils`. The adapter shall accept a derive-backed
`wyrd_spec::error::WyrdError` and perform the final conversion into a Python
exception.

Owner-local errors may remain internal, but they shall convert explicitly to a
catalog-backed `WyrdError` before reaching the shared Python adapter. They
shall not become independent public error contracts.

The shared cross-crate boundary is:

```rust
pub type WyrdPyResult<T> = Result<T, WyrdPyError>;

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct WyrdPyError(#[from] wyrd_spec::error::WyrdError);

impl From<WyrdPyError> for pyo3::PyErr {
    fn from(error: WyrdPyError) -> Self {
        wyrd_error_to_py_err(error.0)
    }
}
```

This expands the existing `wyrd-utils` setup; it does not replace
`wyrd_error_to_py_err`, the registered shared Python `WyrdError`, or the
derive-backed catalog. Every Wyrd-owned public Python operation returns
`WyrdPyResult<T>`. Owner-local failures convert to `WyrdError` first, and the
shared adapter performs the sole final Python projection.

### REQ-002 — Complete structured Python exception

Every Wyrd-owned Python failure shall raise an exception that is an instance
of the shared Python `WyrdError` and exposes values derived from
`WyrdError::as_problem_json()`:

- `code`
- `message`
- `detail`
- `details`
- `remediation`
- `status`
- `title`
- `type`

The direct attributes shall agree with their corresponding members in
`WyrdError::as_problem_json()`. The Python exception shall not expose a separate
`problem` attribute containing the aggregate payload. Public callers shall
distinguish durable failures by stable `code`, not error text.

### REQ-003 — Catalog ownership is complete

Every stable code reachable from a Wyrd-owned public Python operation shall be
owned by the derive-backed Wyrd error catalog. Conversion shall preserve the
original code, status, title, detail, safe details, and actionable remediation.
No known public client, queue, Vala, Skald, configuration, serialization, or
validation failure may degrade to `WYRD_INTERNAL_500` merely because its code
was absent from the catalog.

### REQ-004 — No parallel public projection

Approved Python owner crates and pure-Python package code shall not hand-write
public `code`, `status`, `title`, `remediation`, `type`, aggregate problem
payload, or exception selection logic. They shall not directly construct
`PyValueError`, `PyRuntimeError`, or another generic Python exception for a
Wyrd-owned failure.

Existing owner-local converters, result wrappers, and partial pure-Python
constructors shall be removed once their callers use the shared adapter.

### REQ-005 — Intrinsic Python failures remain Python failures

Raw `PyResult`, `PyErr`, and native Python exception behavior remain permitted
only where the failure is intrinsic to the interpreter or PyO3 rather than a
Wyrd-owned operation. This includes:

- module, submodule, class, and function registration;
- Python object construction inside the shared converter;
- automatic argument binding or extraction before a method body executes;
- Python garbage-collection traversal;
- import and interpreter failures; and
- user callback exceptions whose established behavior is to preserve the
  originating Python exception.

These cases are not alternate Wyrd public error contracts. Internal helpers
may use `PyResult` while manipulating Python objects, but a Wyrd-owned failure
that reaches a public operation shall still use the shared adapter.

### REQ-006 — Public imports and typing agree

The public `wyrd` package shall expose the shared structured `WyrdError`
consistently at runtime and in generated stubs. Partial pure-Python exception
construction shall route through catalog-backed native behavior or be removed.

The RuntimeError-based Bifrost exception hierarchy shall be removed rather
than retained as a compatibility surface. Existing centrally selected domain
subclasses may remain only when they inherit the shared `WyrdError` and receive
their attributes through the shared projection.

## Invariants and constraints

- `wyrd-spec` remains PyO3-free.
- Generic Python boundary behavior remains owned by `wyrd-utils` behind its
  existing optional `python` feature.
- `python/py-wyrd` remains a thin aggregator and package projection.
- Public error metadata comes from the derive-backed catalog; no handwritten
  parallel metadata tables or accessors are permitted.
- Conversion failures never panic, swallow the original Wyrd failure, or
  silently return `None`.
- Public error details remain JSON-safe and scrubbed; internal transport,
  filesystem, provider, or database strings do not leak without an explicit
  safe catalog projection.
- No new dependency, Cargo feature, public compatibility alias, or durable
  server behavior is introduced.
- Implementation may consolidate mechanically without Red-Green-Refactor or
  other TDD sequencing.
- Tests are focused on changed error seams and representative public methods.
  Aggregate repository test lanes are outside this change.

## Non-goals

- Banning every textual use of `PyResult` or `PyErr`.
- Replacing idiomatic Python `TypeError`, import failures, garbage-collection
  failures, or deliberately preserved user callback exceptions.
- Changing HTTP, TypeScript, CLI, MCP, database, storage, authentication,
  authorization, tenancy, audit, or durable server behavior.
- Adding a new exception framework, compatibility hierarchy, registration
  layer, or owner-specific adapter.
- Rewriting successful Python API behavior, signatures, values, or lifecycle.
- Running whole-repository test aggregates as implementation evidence.

## Expensive-to-reverse decisions and ownership

1. `wyrd-spec` is the sole owner of stable Wyrd error metadata and problem
   semantics while remaining PyO3-free. `WyrdError::as_problem_json()` remains
   the canonical RFC 9457 payload generator, but the Python exception does not
   expose that aggregate payload as a `problem` attribute.
2. `wyrd-utils` owns the sole generic Rust-to-Python Wyrd error adapter and
   exception construction. Its shared cross-crate names are `WyrdPyError` and
   `WyrdPyResult<T>`, and its existing `wyrd_error_to_py_err` remains the sole
   final projector.
3. Python owner crates own only explicit conversion from their internal errors
   into catalog-backed `WyrdError` values.
4. The shared Python `WyrdError` is the catch boundary for every Wyrd-owned
   public failure. Domain subclasses are optional ergonomics, never separate
   metadata or conversion authorities.
5. RuntimeError-based Bifrost exception classes are removed without
   compatibility aliases.

## Acceptance criteria and evidence

### AC-001 — Shared adapter and complete shape

Focused Rust and Python tests prove the shared adapter raises `WyrdError` with
the eight required direct attributes, without a `problem` attribute, and that
their values agree with the canonical RFC 9457 payload projection.

### AC-002 — Catalog preservation

Focused catalog tests prove every stable code migrated from client, queue,
Vala, Skald, configuration, validation, and serialization paths resolves to a
derive-backed entry and retains actionable metadata rather than becoming
`WYRD_INTERNAL_500`.

### AC-003 — All public owners converge

Focused Python tests over representative negative paths in every affected
owner family prove that Wyrd-owned failures are catchable as `WyrdError` and
carry the shared complete shape. Source inspection proves no reachable
owner-local public projector or generic Python exception constructor remains.

### AC-004 — Intrinsic PyO3 behavior is preserved

Focused tests or existing evidence prove automatic Python signature/type
failures and deliberately preserved callback exceptions retain their idiomatic
Python behavior, while registration and converter internals remain functional.

### AC-005 — Runtime, exports, and stubs agree

Focused import and typing checks prove the runtime exception surface and
generated stubs agree, removed Bifrost exception classes are absent, and the
test-only `wyrd.testing` surface uses the same Wyrd-owned failure contract.

## Open material decisions

None.

## Revision history

- **Revision 3 — 2026-09-11 — approved.** The user removed the Python
  exception's aggregate `problem` convenience attribute. Task 01 remains the
  completed revision-2 foundation; Task 02 first removes that attribute before
  continuing migration. `WyrdError::as_problem_json()` remains the canonical
  RFC 9457 payload generator.
- **Revision 2 — 2026-09-10 — approved.** The user fixed the exact shared
  cross-crate adapter shape, required reuse and expansion of the existing
  `wyrd-utils` converter, and required deletion or replacement of stale
  owner-local error paths rather than parallel infrastructure.
- **Revision 1 — 2026-09-10 — approved.** The user approved one shared
  Wyrd-owned Python error path, complete migration of deviations, two
  implementation tasks, focused tests only, and no TDD requirement.

## Authorities

- `AGENTS.md` §§4, 7, 8, 11, and 14
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/references/languages/errors.md`
- `architecture/references/languages/pyo3-boundaries.md`
- `architecture/references/languages/python-api-and-stubs.md`
- `architecture/references/languages/testing-workflows.md`
