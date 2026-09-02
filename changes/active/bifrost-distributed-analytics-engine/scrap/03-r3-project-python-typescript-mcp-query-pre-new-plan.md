---
task_id: BIFROST-R3-T3-FIRST-CLASS-PROJECTIONS
title: Project the terminal-safe query lifecycle through Python, TypeScript, and MCP
kind: reconcile
status: proposed
approved_spec: SPEC-bifrost-distributed-analytics-engine
approved_revision: 3
parent_task: pre-r3 T3 projection plan
frozen_candidate: f1ac4cb01
dependencies: [BIFROST-R3-T2-PRODUCTION-ACTIVATION]
mapped_requirements: [REQ-001, REQ-008, REQ-010, REQ-011]
mapped_invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-008]
mapped_acceptance: [AC-006, AC-007, AC-008]
---

# Superseded task — Revision 3 first-class projections

## Disposition and outcome

Replace and simplify the pre-revision-3 Task 3. Retain idiomatic incremental
Python and TypeScript streams, MCP bounds, selected-path terminals, structured
errors, runtime cancellation/close, generated typing, and real runtime
journeys. Scrap all EXPLAIN work and any requirement that each runtime repeat
Task 2's physical operator or replica matrix.

After this task, Python, TypeScript, and MCP project Task 2's one raw-SQL
contract. Each runtime proves a real client-to-server-to-client lifecycle for
both selected paths, structured failure, cancellation/close, and cleanup while
leaving routing, auth, admission, audit, and durable lifecycle on the server.

Required execution skill: `$wyrd-implement`.

## Owners and current-state amendment

- `vala-sdk` remains the shared Rust query client and terminal/error decoder.
- `vala-sdk`'s existing optional `python` feature owns PyO3 behavior;
  `python/py-wyrd` remains a thin registration/package layer.
- `crates/bindings/wyrd-node` owns N-API values and native lifetime;
  `typescript/wyrd` owns the idiomatic async iterator and `AbortSignal` state.
- `wyrd-mcp` owns closed agent-facing query schemas and result ceilings.
- `wyrd-testing` may extend existing dev-only runtime fixtures; it must not
  enter production wheels/packages.

Current code already has Python and TypeScript incremental query streams and an
MCP query tool. It does not yet project the revision-3 selected execution path.
Python `aclose()` describes abandonment without server cancellation, TypeScript
has no `AbortSignal` state machine, and runtime close/error paths are not yet
proved against the activated distributed graph. Retain these implementations
as starting seams, not as proof of completion.

## Design closure

### 1. Shared terminal and errors

All three projections consume the same generated `BifrostQueryRequest`, Arrow
frames, `QueryExecutionPath`, terminal outcome, and structured Wyrd problem
details from `vala-sdk`/wire conversion. No client parses error strings or
reimplements route/fallback logic. Missing, malformed, duplicate, or
row-count-inconsistent terminal data is a structured protocol failure and can
never become successful exhaustion.

### 2. Python lifetime

`BifrostQueryClient.query` retains its current signature and returns an async
iterator of `pyarrow.RecordBatch`. The native stream owner stores Rust-native
state only across blocking work. It never holds `Bound<'py, T>` across await or
thread boundaries.

`BifrostQueryStream.aclose`, task cancellation during `__anext__`, malformed
IPC/terminal, transport error, and finalization all converge on one idempotent
native close operation: signal server cancellation when the stream has not
already received a valid terminal, wait for server settlement under the
existing deadline, release the native stream, then release Python state. A
valid terminal that already won is preserved. The `terminal` property becomes
the typed public terminal projection including selected path, available only
after validation.

### 3. TypeScript/N-API lifetime

Add `signal?: AbortSignal` to `BifrostQueryRequest` as runtime-only client
ergonomics; it is not serialized to the server. `BifrostQueryStream` owns one
closed state machine:

```text
idle -> starting -> streaming -> terminal -> closing -> closed
```

An already-aborted signal starts no native query. During startup/streaming,
abort starts server cancellation before native close. A validated terminal
that wins the race remains authoritative; otherwise abort wins. `abort`,
`return()`, decode/protocol error, transport error, and finalization share one
idempotent settlement path that removes the listener exactly once and releases
all N-API/native handles. Rust owns native async state; no V8 handle crosses a
thread/await. Preserve integers outside JavaScript's safe range with the
existing approved string/nullable projection, never truncation.

### 4. MCP bounds and no truncation

Retain one closed `bifrost.query` tool. Its input is SQL, visibility, freshness,
optional deadline, and existing `max_rows`/`max_bytes`; unknown fields and any
path/class/topology input are rejected. Validate UTF-8 SQL bytes, positive
deadline, and caller ceilings before transport, all beneath server hard limits.
Authentication and tenant identity come only from the server-owned MCP caller
context.

The tool drains a complete terminal-safe stream. If row/byte output would cross
the caller ceiling, it cancels and drains the server query and returns a
structured ceiling failure; it never returns a successfully truncated result.
The successful result includes the selected path and validated terminal.

### 5. Runtime journeys and telemetry

Each runtime uses public package imports and its real interpreter/runtime
against the production server route. Each proves Interactive success,
Analytical success, one structured post-selection failure, runtime-native
cancel/abort/ceiling handling, selected-path terminal, and zero retained server
ownership. Reuse Task 2's server-side production telemetry capture only to
assert path/terminal/cancellation/cleanup deltas; do not re-prove join,
aggregation, pushdown, exchange, spill, or replica-count matrices.

Extend the existing dev-only `WyrdTestServer` harness configuration so its
single Python/TypeScript testing projection may internally compose the existing
same-process multi-node Oracle fixture needed to select Analytical. Keep the
public handle shape as `WyrdTestServer`; do not export a second cluster pyclass
or N-API class. Task 2's separate-process journey remains the sole physical
topology proof. MCP's Rust journey may use the same existing test fixture
directly.

## Rejected alternatives

- Public EXPLAIN methods/tools: revision 3 defers them.
- Client-side path selection or routing: violates server authority.
- Python facade or TypeScript wrapper implementing durable cancellation policy:
  native/server owners settle the query; wrappers only trigger and await it.
- Returning partial MCP rows with a success flag: violates terminal safety.
- Rust tests that initialize CPython or Node: runtime-dependent behavior belongs
  in its owning runtime suite.

## Ordered TDD scenarios

1. **Shared terminal typing.** Python, N-API/TypeScript, and MCP receive the
   generated selected path and stable structured errors without string parsing.
2. **Python close/cancel.** Normal completion, `aclose`, task cancellation,
   decode error, transport error, and malformed terminal release exactly once
   and leave the server clean.
3. **TypeScript abort races.** Already-aborted, startup abort, streaming abort,
   terminal-versus-abort, repeated abort, `return()`, decode error, and drop
   remove listeners and native ownership exactly once.
4. **MCP closed bounds.** Unknown fields, invalid SQL/deadline, auth denial,
   row/byte ceilings, and server hard-limit failures are structured and never
   return successful truncation.
5. **Real runtime journeys.** Python, Node, and MCP each prove both selected
   paths, one failure, cancellation/ceiling cleanup, terminal evidence, and
   server telemetry baselines without repeating the operator matrix.

Run each scenario RED, GREEN, REFACTOR before the next. Runtime journeys must
fail first because the public runtime behavior is absent, not because a private
extension path or fake server was used.

## Named tests and exact focused commands

Python unit/runtime contract:

```bash
mise run py:setup:testing
mise exec -- bash -lc "cd python/py-wyrd && uv run pytest -q tests/bifrost/test_query.py::test_query_terminal_path_and_close_are_typed_and_settled"
```

Python journey:

```bash
mise run py:setup:testing
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd python/py-wyrd && uv run pytest -q -m integration tests/integration/test_bifrost_analytical.py::test_python_query_paths_failure_cancel_and_cleanup"
```

TypeScript unit/runtime contract:

```bash
mise run ts:build
mise exec -- bash -lc "cd typescript/wyrd && pnpm exec vitest run tests/unit/bifrost-query.test.ts -t 'abort signal settles every terminal race'"
```

TypeScript journey:

```bash
mise run ts:build
mise run ts:build:testing
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd typescript/wyrd && pnpm exec vitest run tests/integration/bifrost-analytical.test.ts -t 'query paths failure abort and cleanup'"
```

MCP journey:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey -E 'test(=rbac::pg_tests::bifrost_query_paths_bounds_failure_cancel_and_cleanup)'"
```

## Broader verification

```bash
mise run py:setup
mise run py:test:unit
mise run py:test:integration
mise run py:typecheck
mise run py:format
mise run py:lints
mise run ts:build
mise run ts:typecheck
mise run ts:test:unit
mise run ts:test:integration
mise run ts:napi:check
mise run test:bifrost:journey:mcp
mise run codegen:check
mise run check:client-tier
mise run check:pyo3-scope
mise run check:py-wheel-no-testing
mise run fmt
mise run lints
git diff --check
```

Do not run `mise run gate` for this task slice.

## Completion evidence

- Generated-source provenance for Python stubs and N-API declarations.
- Runtime state-transition/mutation evidence for close/abort races.
- Structured error parity and large-integer projection proof.
- MCP schema closure, ceiling, authorization, and no-truncation evidence.
- Python, Node, and MCP real journeys with terminal paths, cancellation, zero
  ownership, and server telemetry deltas.
- Dependency/boundary checks proving no server logic or testing harness entered
  production client packages.
- All focused/broader command results and final diff audit.

## Stop conditions

Return `SPEC_REVISION_REQUIRED` if a runtime requires a divergent wire shape,
public EXPLAIN, client-selected path, successful truncation, client-owned
durable lifecycle, a new feature/dependency, or weakened auth/tenant/audit
semantics.

## Authority

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/references/languages/pyo3-boundaries.md`
- `architecture/references/languages/python-api-and-stubs.md`
- `architecture/references/languages/typescript-guide.md`
- `architecture/references/languages/agent-harness.md`
- `architecture/references/languages/errors.md`
- `architecture/references/languages/testing-workflows.md`
