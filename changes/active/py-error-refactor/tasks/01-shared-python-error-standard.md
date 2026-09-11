---
id: PYERR-T01
title: Establish the shared Python error adapter and complete catalog
kind: implementation
status: proposed
spec: SPEC-py-error-refactor
spec_revision: 2
depends_on: []
requirements: [REQ-001, REQ-002, REQ-003]
acceptance: [AC-001, AC-002]
---

# Shared Python error standard

## Objective

Provide one reusable Python boundary result/error adapter backed exclusively by
the derive-generated Wyrd error catalog, with a complete and consistent Python
problem projection. This gives every Python owner one standard to adopt in
PYERR-T02 without losing any currently public stable code.

Required execution skill: `$wyrd-implement`.

## Constraints

- Keep `wyrd-spec` PyO3-free and place generic Python conversion behavior in
  `wyrd-utils` behind its existing `python` feature.
- Reuse `WyrdError::as_problem_json()` and the existing shared exception
  registration; do not add another exception framework or metadata table.
- Add derive-backed catalog ownership for every stable code currently emitted
  by a public Python owner but absent from the catalog before callers migrate.
- Preserve each code's current safe meaning while making its title, status,
  detail, details, and remediation informative and actionable.
- Converter construction failure must not panic or silently erase the original
  Wyrd failure.
- Add no dependency, Cargo feature, compatibility alias, or server behavior.
- Implementation and tests may be written together; TDD sequencing is not
  required.
- Use focused tests only. Do not run aggregate repository test lanes.

## Relevant Surface

- `crates/wyrd-spec/src/error.rs` and subordinate derive-backed error owners
- `crates/shared/wyrd-utils/src/py.rs`
- Existing client, queue, Vala, Skald, configuration, validation, and
  serialization error owners whose public codes are missing from the catalog
- Python exception annotations and stub-generation sources under
  `python/py-wyrd`
- Focused Wyrd error derive/catalog tests and Python error contract tests

Paths are ownership guidance, not a private implementation allowlist.

## Required Implementation

Extend `crates/shared/wyrd-utils/src/py.rs`, which already owns
`wyrd_error_to_py_err` and the registered Python `WyrdError`, with this shared
cross-crate boundary:

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

Reuse the existing exception registration, `WyrdError::as_problem_json()`,
`build_wyrd_py_exception`, and `wyrd_error_to_py_err`. Expand those owners only
where needed: add the missing direct `detail` attribute, align generated stub
source with all nine public fields, and preserve the existing safe fallback
when Python exception construction fails. Do not add a second converter,
exception class, metadata builder, or generic local-error wrapper.

Add the missing derive-backed variants to the existing catalog owners before
Task 02 deletes their handwritten projections. The starting set includes
currently reachable client configuration, missing-credential, payload-size,
row-deserialization, queue-full, transport-down, flush-timeout, Vala schema and
query projection, and Skald agent/runtime/session/tool/workflow codes. Confirm
the complete set from current production sources rather than treating this
list as exhaustive or copying handwritten metadata unchanged.

## Approach

1. Inventory the stable codes reachable from current public Python operations
   and add only the missing derive-backed catalog variants in their established
   Wyrd domains.
2. Add the approved `WyrdPyError` and `WyrdPyResult<T>` boundary to the existing
   `wyrd-utils` Python module and delegate its final conversion to the existing
   `wyrd_error_to_py_err` implementation.
3. Complete the shared exception projection so every required direct attribute
   agrees with the generated problem payload.
4. Align the exception annotation/stub source with the runtime shape; regenerate
   generated stubs rather than editing them manually.
5. Add the minimum focused Rust and Python tests that prove catalog coverage,
   adapter conversion, complete attributes, and non-panicking fallback.

## Acceptance Criteria

- `wyrd_utils::py::WyrdPyResult<T>` and `WyrdPyError` exist with the approved
  shape, and `WyrdPyError` delegates to the existing
  `wyrd_error_to_py_err` projector.
- Its Python exception is an instance of the shared `WyrdError` and exposes
  `code`, `message`, `detail`, `details`, `remediation`, `status`, `title`,
  `type`, and `problem` with mutually consistent values.
- Every known stable public Python code that will migrate in PYERR-T02 resolves
  through the derive-backed catalog and retains actionable metadata.
- Unknown or internal sources fail safely without panicking, swallowing an
  exception-construction error, or exposing unsanitized implementation detail.
- `wyrd-spec` contains no PyO3 dependency or feature, and no new dependency or
  Cargo feature is introduced.
- Generated stubs describe the runtime shared exception shape.
- No duplicate converter, exception registration, problem builder, or parallel
  metadata accessor was added.

## Verification

Run only focused tests for the changed catalog variants and shared Python error
contract. Newly added Rust tests must be invoked with exact `mise exec -- cargo
nextest run` package, target, and test expressions after their final names are
known. Run only the exact Python error-contract test file through `mise exec --`
after `mise run py:setup`; do not run `py:test:unit`.

Required non-test checks for the changed contract and Python boundary remain:

```bash
mise run fmt
mise run lints
mise run py:format
mise run py:lints
mise run py:typecheck
mise run check:pyo3-scope
mise run check:error-coverage
mise run codegen:check
git diff --check
```

Do not run `mise run gate`, `mise run test:rust`, `mise run test:shared`,
`mise run test:wyrd`, `mise run test:skald`, `mise run test:vala`, or any full
Python test lane.

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `WyrdPyResult<T>` / `WyrdPyError` exist with the approved shape and delegate to `wyrd_error_to_py_err` | `crates/shared/wyrd-utils/src/py.rs` (`WyrdPyResult`, `WyrdPyError`, `From<WyrdPyError> for PyErr`) | `mise exec -- cargo check --locked -p wyrd-utils --features python`; `mise run lints` | PASS |
| Python exception is a shared `WyrdError` exposing all nine mutually consistent fields | `crates/shared/wyrd-utils/src/py.rs::build_wyrd_py_exception` (added the missing `detail` attribute) | `mise exec -- uv run pytest tests/test_error_contract.py` (3 passed) | PASS |
| Every stable public Python code migrating in PYERR-T02 resolves through the derive-backed catalog with actionable metadata | `crates/wyrd-spec/src/error.rs` (+40 `WYRD_AGENT_*`, `WYRD_SESSION_*`, `WYRD_TOOL_*`, `WYRD_RUNTIME_*`, `WYRD_WORKFLOW_*`, `WYRD_CLIENT_*` variants); `crates/wyrd-spec/src/vala/error.rs::BifrostError::SchemaParse` | `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=error::tests::python_boundary_codes_resolve_through_the_catalog) + test(=error::tests::bifrost_schema_parse_is_catalog_owned)'` (2 passed) | PASS |
| Unknown or internal sources fail safely without panicking or swallowing a construction error | Unchanged `wyrd_error_to_py_err` `PyRuntimeError` fallback and `WyrdError::from_code` `None` contract | `crates/shared/wyrd-client/src/error.rs::tests::unknown_code_preserved_in_catch_all` (existing) | PASS |
| `wyrd-spec` stays PyO3-free; no new dependency or Cargo feature | No manifest changes in the diff | `mise run check:pyo3-scope`; `git diff --stat` shows no `Cargo.toml` change | PASS |
| Generated stubs describe the runtime shared exception shape | `python/py-wyrd/python/wyrd/stubs/error.pyi` (source), `_wyrd.pyi` (regenerated) | `mise run codegen:check`; `mise run py:typecheck` | PASS |
| No duplicate converter, exception registration, problem builder, or parallel metadata accessor added | Only `wyrd-utils/src/py.rs` gained types; no new `create_exception!` or metadata table | `mise run lints`; `git diff --stat` | PASS |

### Commands

```
mise run fmt
mise run lints
mise run py:format
mise run py:lints
mise run py:typecheck
mise run check:pyo3-scope
mise run check:error-coverage
mise run codegen:check
git diff --check
mise exec -- cargo nextest run --locked -p wyrd-spec --lib \
  -E 'test(=error::tests::python_boundary_codes_resolve_through_the_catalog) + test(=error::tests::bifrost_schema_parse_is_catalog_owned)'
mise run py:setup
cd python/py-wyrd && mise exec -- uv run pytest tests/test_error_contract.py
```

### Material limits

- The derive rejects any code that does not start with `WYRD_`
  (`crates/shared/wyrd-error-derive/src/lib.rs::validate_code`). Catalog
  ownership of the Skald-emitted codes therefore lands under the canonical
  `WYRD_*` domain names the existing `skald-observer` projection already maps
  `SKALD_*` onto. PYERR-T02 performs the caller-side switch.
- `BifrostError::SchemaParse` is not `{ message, details }` shaped, so it does
  not reconstruct through `WyrdError::from_code`. PYERR-T02 converts the queue's
  schema-parse failure explicitly rather than through code reconstruction.
- Non-goals stayed excluded: no owner crate, server, CLI, MCP, or TypeScript
  behavior changed; no aggregate test lane was run.
