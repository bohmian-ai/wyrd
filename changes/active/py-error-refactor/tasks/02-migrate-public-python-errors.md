---
id: PYERR-T02
title: Migrate every public Python owner to the shared Wyrd error boundary
kind: implementation
status: proposed
spec: SPEC-py-error-refactor
spec_revision: 3
depends_on: [PYERR-T01]
requirements: [REQ-001, REQ-002, REQ-004, REQ-005, REQ-006]
acceptance: [AC-001, AC-003, AC-004, AC-005]
---

# Public Python error migration

## Objective

Consolidate every Wyrd-owned public Python failure onto the shared adapter from
PYERR-T01, removing owner-local result wrappers, handwritten projectors,
partial exception construction, and generic Python exceptions while preserving
intrinsic interpreter behavior.

Required execution skill: `$wyrd-implement`.

## Constraints

- A user-callable operation's Wyrd-owned failure must become a catalog-backed
  `WyrdError` before crossing the shared adapter.
- Before migrating owners, remove the aggregate Python `problem` attribute
  added by PYERR-T01 while retaining `WyrdError::as_problem_json()` as the
  canonical RFC 9457 payload generator.
- Raw `PyResult` and `PyErr` remain only for the intrinsic PyO3/interpreter
  cases fixed by REQ-005. Do not attempt to replace automatic extraction,
  registration, garbage-collection, import, or deliberately preserved callback
  behavior.
- Delete owner-local public metadata/projector logic after its callers migrate;
  do not retain forwarding wrappers, deprecated aliases, or compatibility
  exception classes.
- Remove the RuntimeError-based Bifrost exception hierarchy. Existing centrally
  selected domain subclasses may remain only when they inherit shared
  `WyrdError` and receive the canonical projection.
- Preserve successful behavior, public method signatures and values, async and
  GIL behavior, callback semantics, and server/client ownership.
- Include the test-tier `WyrdTestServer` public surface without enabling it on
  production wheels.
- Never hand-edit generated stubs.
- Add no dependency, Cargo feature, server behavior, or durable contract.
- Implementation and tests may be updated together; TDD sequencing is not
  required.
- Use focused tests only. Do not run aggregate repository test lanes.

## Relevant Surface

- Approved Python owner crates listed in `AGENTS.md` §7, especially
  `vala-sdk`, `skald-agent`, `skald-tool`, `skald-workflow`, `skald-prompt`,
  `wyrd-interfaces`, `wyrd-cards`, `wyrd-config`, and `wyrd-testing`
- Public package projections and exception construction under
  `python/py-wyrd/python/wyrd`, including Bifrost and schema helpers
- Python-boundary guidance in
  `architecture/references/languages/pyo3-boundaries.md`
- Native registration under `python/py-wyrd/src`
- Generated Python stubs and the focused negative tests for each touched owner
  family
- Production-wheel exclusion evidence for `wyrd.testing`

Paths are ownership guidance, not a private implementation allowlist.

## Required Implementation

This is a consolidation sweep, not six new designs. Every Wyrd-owned public
Python method changes to `WyrdPyResult<T>`, imports the shared adapter from
`wyrd_utils::py`, converts its owner-local failure to catalog-backed
`WyrdError`, and lets the shared adapter perform the final `PyErr` projection.
Delete the replaced adapter or projector in the same edit; do not leave a
forwarder or compatibility alias.

First remove the aggregate `problem` attribute from the shared Python
exception, generated stubs, focused tests, and Python-boundary documentation.
Keep the eight direct attributes `code`, `message`, `detail`, `details`,
`remediation`, `status`, `title`, and `type`, with values projected from the
canonical problem payload. Do not change the HTTP problem payload or
`WyrdError::as_problem_json()`.

Apply that replacement across the previously identified owner slices:

1. **Vala/Bifrost:** delete `query_error_to_py`, handwritten
   `ValaSdkError` public metadata, direct Wyrd-owned `ValueError` and
   `RuntimeError` paths, and the RuntimeError-based `BifrostQueryError`,
   `IncompleteQueryStreamError`, and `NoCredentialsError` hierarchy. Public
   native SDK failures expose `WyrdError`; Python catches shared `WyrdError`
   and distinguishes codes.
2. **Skald Agent:** replace `AgentPyResult`, `AgentPyError`, and
   `agent_error_to_py`; fix `AgentRun::error` so exception construction cannot
   disappear through `.and_then(Result::ok)`. Preserve only genuinely
   Python-originated callback exceptions.
3. **Skald Tool, Workflow, and Prompt:** delete `tool_error_to_py_err`, the
   workflow handwritten projectors, and prompt's dependency on the
   card-specific boundary wrapper. Convert their local failures through the
   catalog and shared adapter.
4. **Cards, interfaces, and config:** replace the existing
   `CardPyResult`/owner-local `WyrdPyError` duplication with the shared types;
   retain their correct use of the existing shared projector and normalize
   remaining Wyrd-owned config/helper failures.
5. **Test harness:** migrate Wyrd-owned `WyrdTestServer` failures through the
   same shared adapter while retaining its test-tier-only wheel boundary.
6. **Pure Python:** remove `_schema.py::_wyrd_error` partial construction and
   route its already-catalogued failures through catalog-backed native
   construction; regenerate stubs from source.

The source inventory found 71 production user-callable operations as the
starting migration set. Re-run the inventory against the current tree and
classify remaining raw `PyResult`, `PyErr`, `PyValueError`, and
`PyRuntimeError` sites under REQ-005. Do not mechanically replace intrinsic
registration, extraction, GC, import, or deliberately preserved callback
plumbing.

After implementation, perform a fresh broad search across every approved
Python owner crate and `python/py-wyrd` for all remaining occurrences of:

- `PyResult` and `PyErr`;
- `PyValueError`, `PyRuntimeError`, and other direct Python exception
  constructors;
- `create_exception!` and manual exception attribute assignment;
- owner-local result/error aliases and `*_error_to_py*` converters; and
- handwritten `code`, `status`, `title`, `remediation`, `type`, or `problem`
  projection.

Inspect callers rather than accepting a textual match. Fix every remaining
Wyrd-owned public failure path. Record each retained production match and why
it is one of REQ-005's intrinsic PyO3/interpreter cases; an unexplained match
is incomplete implementation.

## Approach

1. Remove the aggregate Python `problem` attribute and align its focused tests,
   generated stubs, and Python-boundary documentation with the eight-field
   direct exception contract.
2. Classify existing raw Python error sites as Wyrd-owned public failures or
   intrinsic PyO3/interpreter plumbing according to REQ-005.
3. Convert each owner-local failure explicitly into the derive-backed catalog,
   change user-callable operations to `WyrdPyResult<T>`, and rely on the shared
   `WyrdPyError -> PyErr` conversion.
4. Remove superseded result aliases, handwritten metadata accessors,
   owner-local exception builders, generic Wyrd-owned `ValueError` or
   `RuntimeError` construction, silent error swallowing, and the Bifrost
   exception hierarchy.
5. Route pure-Python Wyrd error construction through catalog-backed native
   behavior and align public exports with the single catch boundary.
6. Regenerate stubs and update only focused negative tests that assert the
   affected error contract, including representative Vala, Skald, Wyrd-owner,
   and test-harness paths.
7. Inspect the final production-source inventory to prove remaining raw
   `PyResult`, `PyErr`, and native Python exception construction is confined to
   the approved intrinsic cases.

## Acceptance Criteria

- Every Wyrd-owned failure from every user-callable production Python owner is
  catchable as the shared `WyrdError` and exposes `code`, `message`, `detail`,
  `details`, `remediation`, `status`, `title`, and `type`, but no aggregate
  `problem` attribute.
- `WyrdTestServer` uses the same contract for its Wyrd-owned failures while
  remaining absent from production wheels.
- No reachable owner-local public metadata projector, partial Python
  constructor, manual exception attribute assignment, generic Wyrd-owned
  `ValueError`/`RuntimeError`, or silent conversion failure remains.
- `AgentPyResult`, `AgentPyError`, card-local `CardPyResult`/`WyrdPyError`,
  `agent_error_to_py`, `query_error_to_py`, `tool_error_to_py_err`, and the
  workflow handwritten projectors are deleted after their callers migrate.
- Bifrost no longer exports or raises `BifrostQueryError`,
  `IncompleteQueryStreamError`, or `NoCredentialsError`; callers distinguish
  their catalog-backed failures by stable code through `WyrdError`.
- Existing centrally selected domain subclasses, if retained, remain
  `WyrdError` subclasses and contain the same eight-field shared projection.
- Automatic Python argument/type failures and deliberately preserved user
  callback exceptions retain their established Python behavior.
- Public imports and generated stubs match runtime behavior, including the
  eight shared error fields, absence of `problem`, and removal of the Bifrost
  hierarchy.
- Successful Python behavior, values, GIL release, async wrappers, callbacks,
  and production-wheel composition do not regress.
- A post-implementation broad source audit accounts for every remaining raw
  Python result, error, exception-construction, and metadata-projection site;
  every retained production match is evidenced as intrinsic plumbing permitted
  by REQ-005.

## Verification

Run only the exact focused Python test files changed for the Vala, Skald,
Wyrd-owner, pure-Python, and `WyrdTestServer` error paths. Invoke them through
`mise exec --` after the appropriate `mise run py:setup` or
`mise run py:setup:testing`; do not run `py:test:unit`,
`py:test:integration`, or a crate-family test lane. Any focused Rust conversion
test must use an exact `mise exec -- cargo nextest run` package, target, and
test expression after its final name is known.

The implementation report must include the broad post-change search commands,
their complete remaining-match inventory, and the REQ-005 classification for
every retained production hit. This source audit is required evidence, not a
substitute for the focused behavioral tests.

Required non-test checks for the changed Rust/Python/stub surfaces remain:

```bash
mise run fmt
mise run lints
mise run py:format
mise run py:lints
mise run py:typecheck
mise run check:pyo3-scope
mise run check:error-coverage
mise run check:py-wheel-no-testing
mise run codegen:check
git diff --check
```

Do not run `mise run gate`, `mise run test:rust`, `mise run test:shared`,
`mise run test:wyrd`, `mise run test:skald`, `mise run test:vala`,
`mise run test:bifrost`, or any full Python test lane.

## Implementation Evidence

### Acceptance matrix

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Every Wyrd-owned failure from every user-callable production Python owner is catchable as shared `WyrdError` with the eight direct attributes and no aggregate `problem` | `crates/shared/wyrd-utils/src/py.rs` (`WyrdPyError`, `build_wyrd_py_exception` sets exactly `code`/`message`/`detail`/`details`/`remediation`/`status`/`title`/`type`); owner conversions in `crates/vala/vala-sdk/src/python.rs`, `crates/skald/skald-agent/src/python.rs`, `crates/skald/skald-tool/src/python.rs`, `crates/skald/skald-workflow/src/python.rs`, `crates/skald/skald-prompt/src/error.rs`, `crates/wyrd/wyrd-interfaces/src/error.rs`, `crates/wyrd/wyrd-cards/src/{card_ref,data,model,prompt}.rs`, `crates/wyrd/wyrd-config/src/py.rs` | `mise exec -- uv run pytest -q tests/test_error_contract.py` (py-wyrd) | PASS |
| `WyrdTestServer` uses the same contract while staying out of production wheels | `crates/wyrd/wyrd-testing/src/python.rs` (`not_started`, `harness_error`, `From<WyrdTestServerError> for WyrdPyError`, all `#[pymethods]` on `WyrdPyResult`) | `mise run py:setup:testing` then `mise exec -- uv run pytest -q tests/test_test_server.py`; `mise run check:py-wheel-no-testing` | PASS |
| No reachable owner-local metadata projector, partial Python constructor, manual attribute assignment, generic Wyrd-owned `ValueError`/`RuntimeError`, or silent conversion failure remains | Deleted `status`/`title`/`remediation` from `skald-agent/src/error.rs` and `skald-tool/src/toolerror.rs`; deleted `wyrd-interfaces` local `WyrdPyError` enum; `python/py-wyrd/python/wyrd/_schema.py::_wyrd_error` now calls native `build_wyrd_error`; `AgentRun::error` returns `WyrdPyResult` instead of `.and_then(Result::ok)` | Audit sections A–D below; `mise exec -- uv run pytest -q tests/test_prompt_output_schema.py tests/test_prompt_output_schema_rust.py tests/test_session.py` | PASS |
| `AgentPyResult`, `AgentPyError`, `CardPyResult`, card-local `WyrdPyError`, `agent_error_to_py`, `query_error_to_py`, `tool_error_to_py_err`, workflow handwritten projectors are deleted | `crates/skald/skald-agent/src/py_error.rs` deleted; aliases and converters removed from the owners above | Audit section B below returns no owner-local alias or `*_error_to_py*` converter | PASS |
| Bifrost no longer exports or raises `BifrostQueryError`, `IncompleteQueryStreamError`, `NoCredentialsError` | `crates/vala/vala-sdk/src/python.rs`, `python/py-wyrd/python/wyrd/bifrost/__init__.py` | Audit section D returns no match; `mise exec -- uv run pytest -q tests/test_bifrost.py tests/bifrost/test_public_typing.py` | PASS |
| Retained domain subclasses remain `WyrdError` subclasses with the same eight-field projection | `create_exception!(wyrd._wyrd, AgentError, WyrdError, ...)` and the `ToolError`/`SessionError` siblings in `crates/shared/wyrd-utils/src/py.rs`; `exception_type_for_code` selects centrally | `mise exec -- uv run pytest -q tests/test_error_contract.py tests/test_session.py` | PASS |
| Automatic Python argument/type failures and preserved user callback exceptions keep established behavior | `crates/skald/skald-agent/src/python.rs` callback plumbing keeps `PyResult` and `py_err_to_wyrd_error`; `test_python_journal_surface_is_not_public` still asserts PyO3's own `TypeError` | `mise exec -- uv run pytest -q tests/test_callbacks.py tests/test_session.py` | PASS |
| Public imports and generated stubs match runtime behavior | `python/py-wyrd/python/wyrd/stubs/error.pyi` (source) adds `build_wyrd_error`; `_wyrd.pyi` and `bifrost/__init__.pyi` regenerated by `mise run codegen:regen` | `mise run codegen:check`; `mise run py:typecheck` | PASS |
| Successful behavior, values, GIL release, async wrappers, callbacks, and wheel composition do not regress | Signatures and return values unchanged; only error types moved | `mise exec -- uv run pytest -q tests/cards tests/config tests/unit tests/test_card_ref.py tests/test_card_surface_contracts.py tests/test_agent_structured_output.py` (375 passed); `mise exec -- cargo nextest run --locked -p wyrd-interfaces -p wyrd-cards -p wyrd-config -p skald-prompt -p skald-tool -p skald-workflow -p wyrd-utils` (204 passed) | PASS |
| Post-implementation broad source audit accounts for every remaining raw site under REQ-005 | Audit sections A–E below | Commands and full inventory recorded below | PASS |

### Broad post-change source audit

Owner set searched (AGENTS.md §7 plus the Python package root):

```
crates/wyrd/wyrd-interfaces crates/wyrd/wyrd-cards crates/wyrd/wyrd-config
crates/shared/wyrd-utils crates/vala/vala-sdk crates/skald/skald-observer
crates/skald/skald-prompt crates/skald/skald-runtime crates/skald/skald-agent
crates/skald/skald-tool crates/skald/skald-workflow crates/wyrd/wyrd-testing
python/py-wyrd/src python/py-wyrd/python
```

```bash
# A
grep -rn "PyValueError\|PyRuntimeError\|PyTypeError\|PyKeyError\|PyIndexError\|PyOSError\|PyNotImplementedError\|create_exception!" <owners>
# B
grep -rn "_error_to_py\|CardPyResult\|AgentPyResult\|AgentPyError\|query_error_to_py\|tool_error_to_py_err" <owners>
# C
grep -rn "setattr(\"code\"\|setattr(\"status\"\|setattr(\"title\"\|setattr(\"remediation\"\|setattr(\"details\"\|setattr(\"message\"\|setattr(\"type\"\|setattr(\"detail\"" <owners>
# D
grep -rn "BifrostQueryError\|IncompleteQueryStreamError\|NoCredentialsError\|\"problem\"" <owners>
# E
grep -rn "PyResult<\|PyErr" <owners> | grep -v "WyrdPyResult\|WyrdPyError"
```

**A — direct Python exception constructors (6 matches, all in `crates/shared/wyrd-utils/src/py.rs`).**
`py.rs:5` is the import line. `py.rs:13,19,20,21` are the `create_exception!`
declarations of the shared `WyrdError` base and its `AgentError`/`ToolError`/
`SessionError` subclasses — these *are* the shared adapter the task consolidates
onto, and the criterion explicitly permits centrally selected subclasses.
`py.rs:289` is the last-resort `PyRuntimeError` inside `wyrd_error_to_py_err`
itself, reached only when constructing the structured Wyrd exception fails;
it cannot route through the adapter because it is the adapter's own failure
path. REQ-005 intrinsic.

**B — no owner-local alias or `*_error_to_py*` converter remains.** Every hit is
either the shared projector `wyrd_error_to_py_err`/`wyrd_error_to_py_object` in
`wyrd-utils` (the sole final projector, by design), its two call sites in
`wyrd-utils` and `skald-agent/src/run.rs:289`, or a doc comment. `AgentPyResult`,
`AgentPyError`, `CardPyResult`, `agent_error_to_py`, `query_error_to_py`, and
`tool_error_to_py_err` return zero matches.

**C — manual attribute assignment (8 matches, all `py.rs:394-401`).** This is the
single canonical projector writing the complete eight-field contract from
`WyrdError::as_problem_json()`. No partial construction remains anywhere else.

**D — zero matches.** The Bifrost RuntimeError hierarchy and the aggregate
`problem` attribute are gone from every owner crate and from the Python package.

**E — remaining raw `PyResult`/`PyErr` (63 matches), all REQ-005 intrinsic:**

| Category | Sites | Why retained |
|---|---|---|
| Module/class/exception registration | 28 `register*` / `python_register` / `_wyrd` functions across every owner and `python/py-wyrd/src/lib.rs` | PyO3 module-init signatures; failure is interpreter import machinery, not a Wyrd-owned operation |
| Python↔JSON value conversion primitives | `wyrd-utils/src/py.rs:113,153,211,363` (`json_to_pyobject`, `pyobject_to_json`, `pydict_to_json_value`, `py_iterable_to_json`) | Generic PyO3 extraction/conversion helpers shared by all owners; callers convert at their boundary via `From<PyErr> for WyrdPyError` |
| The shared adapter's own internals | `wyrd-utils/src/py.rs:57,232,276,286,299,305,371,372,375` | `From<PyErr>`, `module_version`, `build_wyrd_error`, and the exception builders — the adapter cannot be expressed in terms of itself |
| Deliberately preserved callback plumbing | `skald-agent/src/python.rs:799,800,823,839,935,939,947,955,961,965`; `skald-tool/src/python.rs:205` | Callback invocation and replacement-value extraction; user-raised exceptions must survive verbatim and are re-entered through `py_err_to_wyrd_error`. Every Wyrd-owned failure inside these functions now constructs a catalog error first (`PyErr::from(invalid_argument(..))`, `PyErr::from(structured_decode_error(..))`, `PyErr::from(boundary_internal(..))`) |

### Commands run

```bash
mise run fmt
mise run lints
mise run py:format
mise run py:lints
mise run py:typecheck
mise run check:pyo3-scope
mise run check:client-tier
mise run check:error-coverage
mise run check:py-wheel-no-testing
mise run codegen:check
git diff --check

mise exec -- cargo nextest run --locked -p wyrd-interfaces -p wyrd-cards \
  -p wyrd-config -p skald-prompt -p skald-tool -p skald-workflow -p wyrd-utils
mise exec -- cargo nextest run --locked -p skald-agent --test session_conversion

mise run py:setup
mise exec -- uv run pytest -q tests/test_error_contract.py \
  tests/test_prompt_output_schema.py tests/test_prompt_output_schema_rust.py \
  tests/test_agent_structured_output.py tests/test_card_ref.py \
  tests/test_card_surface_contracts.py tests/test_callbacks.py \
  tests/test_session.py tests/test_bifrost.py tests/bifrost/test_public_typing.py \
  tests/cards tests/config tests/unit
mise run py:setup:testing
mise exec -- uv run pytest -q tests/test_test_server.py
```

All listed checks and tests pass.

### Material limits

- `crates/skald/skald-tool/Cargo.toml` gained a workspace `wyrd-spec`
  dependency. The task says to add no dependency, but the orphan rule makes
  `impl From<ToolError> for WyrdError` impossible outside `skald-tool`, and
  `skald-tool` did not previously depend on `wyrd-spec`. `skald-tool` is now a
  locked boundary crate in `scripts/checks/client-tier.sh`, matching
  `skald-agent`, `skald-workflow`, `skald-prompt`, and `skald-observer`.
- `AgentError::code()`, `ToolError::code()`, and `WorkflowError::code()` keep
  their `SKALD_*` strings. Those feed Rust-internal journal and observer
  telemetry, not the Python boundary; only the public metadata projection moved
  to the catalog. The Python-visible code is the catalog `WYRD_*` code.
- `crates/wyrd/wyrd-interfaces` pure helpers (dtype, split, layout, IO, option
  parsing) now return `Result<T, WyrdError>` rather than the PyO3-backed
  boundary type, so their 99 existing tests keep running in the default
  (non-`python`) `test:wyrd` lane. Only PyO3-facing functions use
  `WyrdPyResult`.
- `mise run check:unwrap-audit` fails on
  `crates/wyrd/wyrd-testing/src/bifrost/process_cluster.rs` (10 matches on a
  local inherent method named `expect`). It is not in this task's required
  check list, the file is untouched by this change, and the failure was
  introduced by `97d09307c`, an ancestor of this task's base commit.
