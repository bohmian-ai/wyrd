---
id: BIFROST-R3-T05-PYTHON
title: Project the terminal-safe query lifecycle through Python
kind: implementation
mode: RECONCILE
status: superseded
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 3
depends_on: [BIFROST-R3-T04-PRODUCTION-ACTIVATION]
requirements: [REQ-001, REQ-008, REQ-010]
invariants: [INV-001, INV-003, INV-004, INV-005, INV-008]
acceptance: [AC-006, AC-008]
parent_task: BIFROST-R3-T3-FIRST-CLASS-PROJECTIONS
frozen_candidate: f1ac4cb01fe9ddda0a133cb58c955bab1e1cf7df
---

# Python query projection

> Deferred from this change by approved specification revision 4. Preserved as
> non-authoritative planning input for a later Python integration change.

## Outcome and value

Python users consume incremental `pyarrow.RecordBatch` values and one typed
terminal containing the server-selected path. Normal completion, `aclose`, task
cancellation, malformed frames/terminal, transport failure, and finalization
settle the server query and release native/Python state exactly once. Python
does not select routes or own durable lifecycle behavior.

Required execution skills: `$wyrd-implement` and the repository PyO3 guidance.

## Current-state amendment and owners

Retain Task 4's shared `vala-sdk` Rust stream settlement, optional `python` feature,
`PyBifrostQueryStream`, and the public facade in
`python/py-wyrd/python/wyrd/bifrost`. Replace the current `aclose` abandonment
semantics and untyped terminal dictionary. `vala-sdk` owns native cancellation,
deadline-bounded settlement, request/response conversion, incremental decoding,
terminal validation, and canonical error extraction; `py-wyrd` remains a thin
registration/facade/stub/test layer. Any reusable behavior found missing is
implemented and Rust-tested in `vala-sdk` before Python exposes it. Consume Task
4's test-tier multi-node `WyrdTestServer` option unchanged; testing support must
not enter the production wheel.

Do not add a Python path selector, EXPLAIN, client routing, Python-owned graph
state, ad hoc runtime, or Rust tests that initialize CPython.

## Ordered implementation scenarios

### Scenario 1 — Typed terminal and structured errors

**Behavior.** The public iterator exposes the generated terminal path/outcome
only after complete validation and preserves Wyrd error fields without parsing
strings. Maps REQ-001, REQ-008, REQ-010, INV-001, INV-003, AC-006.

**RED.** Add
`tests/bifrost/test_query.py::test_query_terminal_path_and_errors_are_typed`.
Feed valid Interactive/Analytical terminals plus missing, duplicate,
row-count-inconsistent, and malformed terminals; assert typed path and protocol
failure. Exact:

```bash
mise run py:setup
mise exec -- bash -lc "cd python/py-wyrd && uv run pytest -q tests/bifrost/test_query.py::test_query_terminal_path_and_errors_are_typed"
```

**GREEN.** Project the generated Rust terminal into a Python-visible typed
terminal value; let the facade retain it after validation. Convert derive-backed
errors at the PyO3 edge with code/status/title/detail/remediation/details. Never
hold `Bound<'py, T>` across blocking work or store `PyErr` in Rust owners.

**REFACTOR.** One Rust terminal decoder and error model serve every client;
Python adds only idiomatic iteration, exception, and property shapes.

### Scenario 2 — Idempotent close and cancellation races

**Behavior.** `aclose`, cancellation during `__anext__`, decode/transport error,
finalization, and normal terminal converge on one exactly-once settlement;
terminal wins only after validation. Maps REQ-010, INV-003, INV-004, AC-006.

**RED.** Add
`tests/bifrost/test_query.py::test_query_close_and_cancel_settle_exactly_once`.
Use deterministic native probes for startup/poll/terminal races and assert one
cancel/drain/close plus no blocking of the event loop. Exact:

```bash
mise run py:setup
mise exec -- bash -lc "cd python/py-wyrd && uv run pytest -q tests/bifrost/test_query.py::test_query_close_and_cancel_settle_exactly_once"
```

**GREEN.** Bridge Python `aclose`, `CancelledError`, decode/transport exceptions,
and normal terminal to Task 4's one Rust-native async close/settlement API
through the approved shared runtime boundary. Preserve the server request's
original absolute deadline; do not start a Python-local timeout budget.
Finalization may signal cancellation/leak-report when it cannot await but must
not claim successful cleanup.

**REFACTOR.** Python wrapper state mirrors native state but never becomes the
authority. It may translate Python runtime events into Rust calls, but may not
reimplement request validation, stream decoding, terminal/error semantics,
deadlines, cancellation, or settlement. No `Bound` survives a thread/await
boundary.

### Scenario 3 — Real Python client journey

**Behavior.** Public Python imports drive Interactive success, Analytical
success, structured post-selection failure, task cancellation/close, terminal
paths, deadline parity with the Rust request, and zero retained Python/native
state and server query ownership. Maps REQ-001, REQ-008, REQ-010, AC-006,
AC-008.

**RED.** Add
`tests/integration/test_bifrost_query.py::test_python_query_paths_failure_cancel_and_cleanup`.
It uses `wyrd.testing.WyrdTestServer` and public `wyrd.bifrost` imports against
repository Postgres; it does not repeat Task 3's operator matrix. Exact:

```bash
mise run py:setup:testing
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd python/py-wyrd && uv run pytest -q -m integration tests/integration/test_bifrost_query.py::test_python_query_paths_failure_cancel_and_cleanup"
```

**GREEN.** Select Task 4's existing test-tier multi-node option through the
current `WyrdTestServer` public shape. Exercise production routes and native
client; add no cluster pyclass or Python-specific server lifecycle.

**REFACTOR.** Server-side physical proof remains Task 3; this journey proves the
Python runtime seam and lifecycle only.

## Broader verification

```bash
mise run py:format
mise run py:lints
mise run py:test:unit
mise run py:test:integration
mise run py:typecheck
mise run codegen:check
mise run check:pyo3-scope
mise run check:client-tier
mise run check:py-wheel-no-testing
mise run fmt
mise run lints
git diff --check
```

## Completion evidence and stop conditions

Provide generated-stub provenance, terminal/error parity, close-race traces,
the real Python journey, deadline parity, and zero Python/native/server query
ownership snapshots. Return
`SPEC_REVISION_REQUIRED` if Python needs a divergent wire contract, path
selector, successful partial output, client-owned lifecycle, new dependency/
feature, or weakened server auth/tenant/audit behavior.

## Authority

`architecture/references/languages/pyo3-boundaries.md`,
`architecture/references/languages/python-api-and-stubs.md`,
`architecture/references/languages/errors.md`, and `AGENTS.md` §§7–8, 11.
