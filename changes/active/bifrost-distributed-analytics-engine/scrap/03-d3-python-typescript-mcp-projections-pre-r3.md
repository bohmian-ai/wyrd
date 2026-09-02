# SCRAPPED — pre-revision-3 T3 projection plan

Non-authoritative history. Approved specification revision 3 removed public
EXPLAIN and repeated physical-operator proof from each runtime. Its active
successor is `../tasks/03-r3-project-python-typescript-mcp-query.md`.

# T3 — Prove Python, TypeScript, and MCP projections

Status: Planned
Repository origin: github.com/bohmian-ai/wyrd
Repository revision: 3978aa7a2bbe49299fc8cd50e36c2e3c8e6e9d09
REPO_ROOT: $REPO_ROOT
PLAN_PATH: $PLAN_PATH
TASK_PATH: $TASK_PATH
Plan: ../plan.md
Milestone: M2
Requirements: R6, R7, R8
Decisions: D2, D3, D6
Depends on: T2

## Objective

Project T2's final raw-SQL query, EXPLAIN, execution-path/evidence, deadline,
and structured-error contract through Python, TypeScript, and MCP, and prove
each runtime's real client → server → client lifecycle without moving durable
routing, admission, storage, audit, or cancellation ownership into a client.

Required skill: `wyrd-implement`.

## Context

`vala-sdk` owns Rust-native client behavior. `python/py-wyrd` is a thin PyO3
aggregator plus public Python package. `crates/bindings/wyrd-node` and
`typescript/wyrd` own the N-API and TypeScript projection. `wyrd-mcp` owns
agent-facing tools. T2 has already integrated the final server contract and
real Rust/HTTP/gRPC journeys; T3 does not repeat its internal route matrix.

T2 supplies one production `OracleTelemetry` owner and the existing dev-only
server/cluster production-capture seam. Python, TypeScript, and MCP journeys
must inspect that server-side production telemetry while keeping all capture
support out of production client packages.

Python API/stub, TypeScript, PyO3, agent-harness, errors, and testing references
fix runtime ownership, generated declarations, structured errors, bounded MCP
inputs/outputs, audit, and required runtime-native journeys.

## Required changes

1. Extend `vala-sdk` query/EXPLAIN and terminal evidence projections, including
   exact deadline and structured-error conversions.
2. Extend the owner crate's optional Python feature, native registration,
   public `wyrd.bifrost` exports, source annotations, and generated stubs.
3. Extend N-API native types and TypeScript wrappers/declarations with an
   explicit `AbortSignal`-aware stream state machine and JS-safe numerics.
4. Expose closed deny-unknown `bifrost.query` and `bifrost.query_explain` MCP
   tools with caller ceilings beneath fixed server hard ceilings.
5. Add named runtime unit tests and real Python, Node, and MCP journeys for
   both paths, EXPLAIN parity, structured failure, cancellation/abort/ceiling,
   and cleanup.
6. Make every runtime journey assert the expected server-side production
   `OracleTelemetry` metric deltas and correlated trace/log lifecycle through
   the existing dev-only test-server capture.

## Non-goals

- Reimplementing routing, admission, stage validation, audit, or durable
  cancellation policy in Python, TypeScript, N-API, or MCP.
- Repeating T2's exhaustive operator/fallback/fault matrix.
- A new Python runtime, TypeScript server, private MCP route, or testing harness
  in production wheels/packages.
- Client-owned Oracle instrumentation or a language-specific surrogate for the
  production server telemetry capture.

## Allowed scope

- `crates/vala/vala-sdk/**` behind its existing optional `python` feature.
- `python/py-wyrd/src/**`, `python/py-wyrd/python/wyrd/bifrost/**`, source
  annotations/generator inputs, and Python tests.
- `crates/bindings/wyrd-node/**`, `typescript/wyrd/**`,
  `typescript/testing/**`, and TypeScript tests.
- `crates/wyrd/wyrd-mcp/**` and its `mcp` journey target.
- `crates/wyrd/wyrd-testing` only for existing dev-only server/cluster fixture
  extensions needed by real runtime journeys.
- Generated stubs/declarations only via generation.

## Prohibited changes

- PyO3 in `wyrd-spec`, durable logic in `python/py-wyrd`, DataFusion/SQL/cloud
  dependencies in a client tier, or test harness in a production wheel.
- Holding `Bound<'py, T>` or V8/N-API handles across await/thread boundaries.
- String-parsed errors, integer truncation, partial-stream success, leaked
  abort listeners, or a client-selected execution path.
- MCP trusted tenant/principal input, unbounded SQL/result, unknown fields, or
  successful truncated result/stage evidence.
- Hand-edited `.pyi` or N-API-generated declarations.
- Tests that emit the telemetry they assert or expose production debugging
  capture through a public Python, TypeScript, or MCP contract.

## Target paths and symbols

- Existing `crates/vala/vala-sdk/src/query.rs`: Rust query stream/client,
  EXPLAIN method, wire conversions, stable errors.
- Existing owner-crate Python module and
  `python/py-wyrd/src` registration: native query/EXPLAIN/evidence projections.
- Existing `python/py-wyrd/python/wyrd/bifrost/__init__.py`:
  `BifrostQueryClient`, `BifrostQueryStream`, new typed EXPLAIN/evidence exports.
- Existing `crates/bindings/wyrd-node` native query owner and generated
  `index.d.ts` source annotations.
- Existing `typescript/wyrd/src/index.ts`: `BifrostClient`,
  `BifrostQueryStream`, query request/terminal types, and new EXPLAIN types.
- Existing `crates/wyrd/wyrd-mcp` Bifrost tool owner and `tests/mcp` target.
- Proposed journey files: Python
  `python/py-wyrd/tests/integration/test_bifrost_analytical.py`; TypeScript
  `typescript/wyrd/tests/integration/bifrost-analytical.test.ts`; MCP current
  Bifrost journey module.

## Required types and interfaces

Python public API remains:

```python
await BifrostQueryClient.query(sql, *, visibility="published_only",
    freshness="strict", deadline_ms=None) -> BifrostQueryStream
await BifrostQueryClient.explain(sql, *, visibility="published_only",
    freshness="strict", deadline_ms=None) -> BifrostQueryExplain
```

The stream yields `pyarrow.RecordBatch` incrementally and exposes terminal
evidence only after a validated terminal. Python cancellation/drop owns native
close and server cancellation settlement without holding Python-bound objects
across await.

TypeScript adds `signal?: AbortSignal` to the query request and keeps
`AsyncIterableIterator<RecordBatch>`. The state machine is exactly
`idle | starting | streaming | terminal | closing | closed`; one settlement
path removes the listener and releases the native owner. A validated terminal
already won by the native stream wins a simultaneous abort; otherwise abort
starts server cancellation before native close. Repeated abort, `return()`, and
drop are idempotent. Wire integers beyond JS safe range use the approved
nullable/string-safe projection and are never truncated.

MCP tool input is raw `sql`, visibility, freshness, optional deadline, and
existing `max_rows`/`max_bytes`; schemas deny unknown fields. SQL byte length,
row/byte ceilings, and deadline are checked before transport, but auth/authz and
tenant binding remain server-owned. No path/class/stage/plan selector exists.

## Implementation guidance

**Test-driven design is required for this task.**

Each Python, Node, and MCP journey starts a scoped baseline on the dev-only
server/cluster production capture, drives the real client → server → client
path, and asserts production `OracleTelemetry` deltas plus correlated
trace/log terminal behavior. The clients do not gain telemetry ownership or a
new public diagnostics API; only test fixtures read the server capture.

Convert runtime inputs at the boundary, then call `vala-sdk`. Use the shared
Wyrd Python runtime boundary; release the GIL around blocking native work not
already async. Generate stubs from source annotations and N-API declarations
from Rust.

Keep the TypeScript cancellation state in the existing `BifrostQueryStream` or
one cohesive helper owned by it; do not distribute listener cleanup across
callbacks. MCP query and EXPLAIN must share the server DTO conversions and
stable error catalog rather than a hand-maintained shadow schema.

Every integration test uses public package imports, the actual runtime, and a
real `WyrdTestServer`/repository Postgres boundary. Test-only analytical fixture
controls remain dev-only.

## Control flow and pseudocode

```text
runtime caller
  -> runtime-local type/range/unknown-field validation
  -> shared Rust client request
  -> real server auth/authz/Oracle query or EXPLAIN
  -> incremental Arrow/evidence/error projection
  -> success: validated terminal wins, release runtime owner
  -> cancel/abort/return/drop/error: server cancellation first when applicable,
     close native stream, remove listener, release all runtime handles
```

MCP additionally enforces caller and server response ceilings; crossing either
ends in a structured refusal and drains the query, never truncated success.

## Failure and edge cases

- Python rejects zero, negative, fractional, wrong-type, and overflow deadline
  before transport; exact bounds pass.
- Python cancellation during `next_ipc`, malformed IPC, invalid/missing terminal,
  and native error all close and release state.
- TypeScript already-aborted signal starts no native request; abort during
  startup/streaming cancels; terminal-versus-abort follows the locked precedence;
  listener removal occurs on every exit.
- MCP rejects empty/oversized UTF-8 SQL, invalid deadline, unknown fields,
  underprivileged/cross-tenant calls, and response ceiling without disclosure or
  partial success. EXPLAIN has denial audit but no successful read acceptance.
- Structured resource/engine errors preserve code/status/title/detail/
  remediation/details without parsing text.

## Acceptance criteria

- AC1: public Python imports, runtime behavior, generated stubs, and typed
  structured exceptions agree with T2.
- AC2: public TypeScript types/runtime/N-API declarations agree, preserve large
  integers, and settle every AbortSignal race without a listener/native leak.
- AC3: MCP schemas and runtime validation are closed, bounded, authorized,
  tenant-safe, audit-correct, and preserve stable errors/evidence.
- AC4: each runtime's real journey proves Interactive, actual-follower
  Analytical, EXPLAIN/terminal agreement, structured failure, and runtime-owned
  cancellation/ceiling cleanup.
- AC5: no client tier owns durable server behavior or forbidden dependencies.
- AC6: Python, TypeScript, and MCP journeys reuse the server's production
  `OracleTelemetry` capture to prove intended path, failure, cancellation,
  ceiling, terminal, and cleanup behavior without shipping test harness code.

## Required tests

| Slice | Exact test and file | Tier / production seam | First RED | Smallest GREEN owner | Required mutation RED | Exact focused command |
|---|---|---|---|---|---|---|
| Python contract | `test_query_deadline_and_execution_evidence_are_typed` in `python/py-wyrd/tests/test_bifrost.py` | unit/runtime; public import → native boundary | new types/method absent | vala-sdk PyO3 projection + package export | accept fraction/overflow or omit path evidence; assertion fails | `mise run py:test:unit` |
| Python journey | `test_python_bifrost_interactive_analytical_explain_error_and_cancel` in `tests/integration/test_bifrost_analytical.py` | journey; CPython → real server | final API/journey absent | Python facade over shared Rust client | suppress cancel close or follower evidence; cleanup assertion fails | `mise run py:test:integration` |
| TS abort unit | `abort_signal_state_machine_settles_every_race` in `typescript/wyrd/tests/bifrost.test.ts` | unit/Node; wrapper → fake recording native boundary | `signal` state machine absent | cohesive `BifrostQueryStream` state transition | remove listener cleanup or let abort beat prior terminal; test fails | `mise run ts:test:unit` |
| TS journey | `typescript_bifrost_interactive_analytical_explain_error_and_abort` in `tests/integration/bifrost-analytical.test.ts` | journey; Node/N-API → real server | final projection absent | N-API types + TS wrapper | truncate large integer or close before server cancel; assertion fails | `mise run ts:test:integration` |
| MCP contract | `bifrost_tools_reject_unknown_fields_and_enforce_byte_deadline_and_result_bounds` in the current MCP Bifrost test module | integration; MCP invocation → tool handler | final schemas/tools absent | shared DTO extraction + bounds | permit unknown field or truncated success; test fails | `mise run test:bifrost:journey:mcp` |
| Runtime production telemetry | `runtime_journeys_observe_server_production_oracle_telemetry` in the existing dev-only Bifrost test-server owner | integration/journey support; Python + Node + MCP requests → installed server recorder/capture | runtime journeys do not share a scoped production telemetry assertion | extend existing dev-only capture fixture only | replace server capture with client/test-only events or omit terminal gauge baselines; runtime journey fails | owning Python, TypeScript, and MCP integration commands below |
| MCP journey | `bifrost_mcp_proves_both_paths_explain_denial_audit_and_cleanup` in the same module | journey; MCP client → real server | final Analytical/EXPLAIN behavior absent | tool handlers over server owners | bypass auth or omit drain on ceiling; audit/cleanup fails | `mise run test:bifrost:journey:mcp` |

Python tests are top-level functions. Rust-only native binding compile/static
checks do not substitute for Python or Node lifetime tests.

## Required features

- Existing `vala-sdk/python`, `py-wyrd`, `wyrd-node`, and dev-only
  `wyrd-testing/python` feature arrangements.
- `wyrd-testing` remains absent from production Python and TypeScript packages.
- No new Cargo feature; no DataFusion/SQL/cloud dependency in a client tier.

## Focused verification

Run sequentially:

```bash
mise run py:setup
mise run py:test:unit
mise run py:test:integration
mise run py:typecheck
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
mise run py:format
mise run py:lints
mise run fmt
mise run lints
```

## Commands explicitly excluded

- `mise run gate`; the integrated plan closeout owns release aggregation.
- Rust tests that initialize Python or Node runtimes.
- Private extension imports in public Python journeys.
- Hand editing `.pyi`, `index.d.ts`, OpenAPI, or schemas.

## Stop and escalate if

Stop if a client must own durable routing/admission/lifecycle behavior, wire
shapes must diverge, a new feature/dependency is required, PyO3/N-API lifetime
rules cannot be met, a partial stream would be reported as success, MCP bounds
or auth require a compatibility alias, or a named journey can pass without the
real runtime and server boundaries.

## Completion evidence

Return AC1-AC6 mapping; generated-source provenance; named RED/GREEN/mutation
results; Python, Node, and MCP cleanup evidence; scoped production telemetry
deltas and correlated trace/log terminals; package dependency/boundary checks;
every sequential command result; final diff audit; and confirmation that
production packages contain no test harness or duplicated server logic.
