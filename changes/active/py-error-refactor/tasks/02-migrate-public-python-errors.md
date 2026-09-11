---
id: PYERR-T02
title: Migrate every public Python owner to the shared Wyrd error boundary
kind: implementation
status: proposed
spec: SPEC-py-error-refactor
spec_revision: 2
depends_on: [PYERR-T01]
requirements: [REQ-001, REQ-004, REQ-005, REQ-006]
acceptance: [AC-003, AC-004, AC-005]
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

1. Classify existing raw Python error sites as Wyrd-owned public failures or
   intrinsic PyO3/interpreter plumbing according to REQ-005.
2. Convert each owner-local failure explicitly into the derive-backed catalog,
   change user-callable operations to `WyrdPyResult<T>`, and rely on the shared
   `WyrdPyError -> PyErr` conversion.
3. Remove superseded result aliases, handwritten metadata accessors,
   owner-local exception builders, generic Wyrd-owned `ValueError` or
   `RuntimeError` construction, silent error swallowing, and the Bifrost
   exception hierarchy.
4. Route pure-Python Wyrd error construction through catalog-backed native
   behavior and align public exports with the single catch boundary.
5. Regenerate stubs and update only focused negative tests that assert the
   affected error contract, including representative Vala, Skald, Wyrd-owner,
   and test-harness paths.
6. Inspect the final production-source inventory to prove remaining raw
   `PyResult`, `PyErr`, and native Python exception construction is confined to
   the approved intrinsic cases.

## Acceptance Criteria

- Every Wyrd-owned failure from every user-callable production Python owner is
  catchable as the shared `WyrdError` and exposes the complete canonical
  problem shape.
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
  `WyrdError` subclasses and contain the same complete shared projection.
- Automatic Python argument/type failures and deliberately preserved user
  callback exceptions retain their established Python behavior.
- Public imports and generated stubs match runtime behavior, including the
  complete shared error fields and removal of the Bifrost hierarchy.
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
