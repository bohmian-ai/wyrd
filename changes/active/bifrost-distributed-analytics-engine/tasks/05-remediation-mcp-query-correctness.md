---
id: BIFROST-R6-T05-R01-MCP-QUERY-CORRECTNESS
title: Make MCP query errors, terminals, and cancellation trustworthy
kind: remediation
mode: REMEDIATE
status: ready
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 6
depends_on: [BIFROST-R5-T05-MCP]
requirements: [REQ-001, REQ-008, REQ-010, REQ-012]
invariants: [INV-001, INV-003, INV-004, INV-005, INV-008, INV-009]
acceptance: [AC-003, AC-004, AC-006, AC-007, AC-008, AC-009]
parent_task: BIFROST-R5-T05-MCP
reviewed_candidate: 4d3a2839281ff259dda7f58089f4c63f0f2fa7ce
planning_base: 4d3a2839281ff259dda7f58089f4c63f0f2fa7ce
remediates:
  - FIND-BIFROST-R5-T05-MCP-1
  - FIND-BIFROST-R5-T05-MCP-2
  - FIND-BIFROST-R5-T05-MCP-3
  - FIND-BIFROST-R5-T05-MCP-4
  - FIND-BIFROST-R5-T05-MCP-5
---

# MCP query correctness remediation

## Outcome and value

An MCP agent receives repairable planning refusals, protocol-correct argument
errors, and a final query result only after one internally consistent terminal
and clean stream end. A real distributed cancellation is proven while the MCP
connection remains open. The existing `WyrdMcpHandler` owns the adapter
workflows, and no Bifrost adapter API escapes the server module.

Required execution skill: `$wyrd-implement`.

This one task closes the complete validated Task 05 review ledger. The findings
converge on the same agent-facing query lifecycle and its two existing journeys;
no new endpoint, contract, dependency, fixture framework, or abstraction is
needed.

## Validated finding ledger

The Task 05 review and independent Ponytail audit validated:

- `FIND-BIFROST-R5-T05-MCP-1`: unknown fields and unsupported functions are
  collapsed into an opaque execution failure whose remediation requires server
  diagnostics, so the agent cannot repair its SQL.
- `FIND-BIFROST-R5-T05-MCP-2`: the MCP collector does not validate the terminal
  matrix or emitted row count and stops before proving clean EOF, allowing a
  malformed, duplicate, or trailing terminal stream to become success.
- `FIND-BIFROST-R5-T05-MCP-3`: the Analytical cancellation journey neither
  proves the request is active before cancellation nor proves settlement before
  disconnect, so ordinary completion or connection teardown can satisfy it.
- `FIND-BIFROST-R5-T05-MCP-4`: adapter-local argument failures use a structured
  tool error instead of rmcp `INVALID_PARAMS`, while `list_tables` silently
  ignores arguments its descriptor forbids.
- `FIND-BIFROST-R5-T05-MCP-5`: three dependency-backed free functions thread
  `&AppState` despite the existing `WyrdMcpHandler` owner, and `pub mod bifrost`
  exposes an adapter with no external consumer.

## Owners, scope, and non-goals

Primary owners and expected write set:

- `crates/vala/vala-bifrost-redux/src/oracle/mod.rs`: at
  `Oracle::plan_physical` only, classify typed DataFusion
  SQL/planning/schema/unsupported-feature refusals as the existing
  `BifrostError::QueryInvalidSql` with a fixed scrubbed repair hint. Preserve
  the shared `map_datafusion_error` and all of its non-planning callers.
- `crates/wyrd/wyrd-server/src/mcp/{mod.rs,bifrost.rs}`: put dependency-backed
  tool workflows on `WyrdMcpHandler`, enforce local arguments through rmcp's
  invalid-params channel, and validate/settle the complete Oracle stream.
- `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs`: distinguish protocol input
  errors from application query errors and prove returned planning guidance.
- `crates/wyrd/wyrd-testing/tests/bifrost/oracle/mcp.rs`: make the existing
  distributed cancellation phase non-vacuous with existing process controls and
  production telemetry.

Use fewer files if those owners suffice. Do not change `wyrd-spec`, add an error
variant or generated schema, expose raw DataFusion/schema text, reproduce query
planning in MCP, add a validator service, add another handler/trait, add a
listener or tool, or alter Oracle routing, admission, execution, audit, or
terminal production.

Preserve exact byte accounting, positional Arrow JSON rows, the three-tool
catalog, test-probe opt-in, tenant/RBAC behavior, and successful Interactive and
Analytical results from the parent task. The unrelated removal of the tracked
TypeScript native build artifact is outside this remediation and remains
untouched.

## Ordered implementation scenarios

### Scenario 1 — Planning refusals are safe and repairable

**Behavior.** A DataFusion failure from `Oracle::plan_physical` that is typed as
SQL parse, plan, schema, or unsupported-feature input is a pre-stream
invalid-query refusal, not an opaque execution failure. Oracle returns the existing
`WYRD_VALA_400_QUERY_INVALID_SQL` with one fixed detail directing the caller to
inspect `bifrost.describe_table` and submit a supported `SELECT`. Context and
diagnostic wrappers are traversed to their typed source. Resource, tenant,
audit, reconciliation, IO, internal, and execution failures retain their
existing mappings. No raw dependency message crosses the public boundary.
Maps REQ-001, REQ-010, REQ-012; INV-001, INV-008; AC-006, AC-009.

**RED.** Add unit test
`oracle::tests::datafusion_query_rejections_are_safe_and_actionable`. Exercise
the new planning-only mapper with direct and wrapped `Plan`, `SchemaError`, and
`NotImplemented` values, assert the existing invalid-SQL code and fixed
scrubbed detail, and prove both an `Internal` value and a `Plan` value passed to
the unchanged general mapper remain `QueryExecutionFailed`. Extend
`query::pg_tests::bifrost_query_errors_are_actionable_before_row_execution` so
the real unknown-field and unsupported-function calls return invalid SQL with a
repair hint usable without server diagnostics. The old generic-500 assertions
must fail first.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::tests::datafusion_query_rejections_are_safe_and_actionable)'
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey -E 'test(=query::pg_tests::bifrost_query_errors_are_actionable_before_row_execution)' --run-ignored=all"
```

**GREEN.** Add `map_query_planning_error` and use it only for the
`SessionContext::sql` and `DataFrame::create_physical_plan` failures inside
`Oracle::plan_physical`. It keeps the existing resource-chain classification
first, recursively unwraps only DataFusion `Context` and `Diagnostic`, maps
typed `SQL`, `Plan`, `SchemaError`, and `NotImplemented` variants to
`BifrostError::QueryInvalidSql` with a constant scrubbed action, and delegates
every other value to the unchanged `map_datafusion_error`. Do not change any
other caller of the general mapper or inspect new error strings.

**REFACTOR.** The classifier is a private deterministic helper, not an error
framework. Oracle remains the one planning owner, and every public projection
receives the same mapped behavior.

### Scenario 2 — Closed inputs use the MCP protocol channel and one handler owner

**Behavior.** Unknown, missing, wrongly typed, or locally out-of-range tool
arguments return rmcp `INVALID_PARAMS`. `bifrost.list_tables` accepts only an
absent or empty argument object. Errors returned after an authenticated server
operation begins remain `CallToolResult { is_error: true }` canonical Wyrd
problems. The existing `WyrdMcpHandler` owns all three dependency-backed
workflows. Maps REQ-010, REQ-012; INV-001, INV-008, INV-009; AC-006, AC-008,
AC-009.

**RED.** Extend
`query::pg_tests::bifrost_query_errors_are_actionable_before_row_execution` to
assert rmcp `ServiceError::McpError` with `ErrorCode::INVALID_PARAMS` for an
unknown query key, missing `sql`, wrong field type, blank/oversized SQL, invalid
deadline, both invalid ceilings, a nonempty `list_tables` argument object, and
one invalid `describe_table` object missing `name`. Retain structured-problem
assertions for SQL-floor, catalog, authorization, and Oracle failures. The
current test must fail because those local cases arrive as tool results and
list arguments succeed.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey -E 'test(=query::pg_tests::bifrost_query_errors_are_actionable_before_row_execution)' --run-ignored=all"
```

**GREEN.** Make `mcp::bifrost` private and narrow its constants and descriptor
builder to `pub(super)`. In `bifrost.rs`, add `pub(super)` inherent methods on
`WyrdMcpHandler` for list, describe, and query; use `self.state` instead of
threading `&AppState`. Make the local argument parser and bound validation
return `ErrorData::invalid_params` directly. Each handler method returns
`Result<CallToolResult, ErrorData>`: local input errors use `Err`, while service
and stream `WyrdError`s are converted to `Ok(CallToolResult::structured_error)`.
Have `call_tool` dispatch directly to these methods and map their successful
result to `CallToolResponse::Complete`. Keep its tracker token alive across the
entire method await. Update the predecessor-era module rustdoc to describe the
three production tools.

**REFACTOR.** Keep pure descriptor, parsing, projection, and byte-count helpers
as free functions. Add no second owner or error enum.

### Scenario 3 — One validated terminal followed by clean EOF

**Behavior.** Every terminal is first validated for the requested visibility
and decoded row count. A failed terminal carries no Arrow EOS; it is settled and
mapped from its canonical `frame.error` immediately. A successful or degraded
terminal is accepted only when its Arrow EOS is valid and the frame stream then
ends cleanly. A valid failed terminal maps its canonical `frame.error`
immediately without EOS processing or cancellation because Oracle has already
settled it. Any malformed terminal, duplicate terminal, trailing frame,
post-terminal stream error, cancellation, or missing EOF awaits
`OracleQueryStream::cancel()` before returning a canonical error. Maps REQ-001,
REQ-008, REQ-010, REQ-012; INV-003, INV-004, INV-005; AC-004, AC-006, AC-009.

**RED.** Add unit test
`mcp::bifrost::tests::query_rejects_untrustworthy_terminal_and_settles_stream`.
Use the existing synthetic stream builder to table-test an invalid terminal
matrix, mismatched row count, malformed EOS, failed terminal, duplicate
terminal, trailing batch, and post-terminal stream error. Every malformed or
post-terminal case asserts the canonical error and cancelled synthetic token.
The valid failed-terminal case instead asserts the exact Wyrd code mapped from
`frame.error` and that the token was not cancelled, proving the already-settled
failure returned directly and empty EOS did not replace it with an Arrow error.
Make the existing exact-byte fixture carry the valid
`PublishedOnly` source-completion matrix so it remains valid under the new
checks. The current collector must fail the mismatch, matrix, duplicate, and
trailing-frame cases by returning success.

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(=mcp::bifrost::tests::query_rejects_untrustworthy_terminal_and_settles_stream)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(=mcp::bifrost::tests::query_result_is_positional_and_counts_exact_structured_json_bytes)'
```

**GREEN.** Retain the request `VisibilityMode` in `ResultCollector`. Track
decoded rows with checked `u64` arithmetic. Before projecting a terminal, call
`QueryTerminalFrame::validate(visibility)`, then
`validate_emitted_rows(decoded_rows)`. For a valid `Failed` terminal, do not
call `accept_eos` or cancel: map its canonical error immediately. For `Success`
or `Degraded`, accept EOS, store the projected terminal, and keep polling. Clean
EOF is the only success exit; the existing post-terminal match guard rejects
every later frame. Route each malformed validation, EOS, and post-terminal
failure through awaited stream cancellation before returning. Charge the
stored terminal only after clean EOF.

**REFACTOR.** Reuse the two contract validators and the existing collector.
Do not create a second stream decoder, terminal type, or generic collector.

### Scenario 4 — Distributed cancellation is observed before disconnect

**Behavior.** The three-Oracle/one-Scribe MCP journey cancels a query only after
a remote follower has activated its graph and paused before source IO. The
coordinator's production
`oracle_query_duration_seconds{class="analytical",outcome="cancelled"}`
observation advances, every Oracle returns to its pre-query ownership baseline,
and only then does the journey close the MCP client. Maps REQ-008, REQ-010, REQ-012;
INV-003, INV-004, INV-005; AC-004, AC-007, AC-009.

**RED.** Strengthen
`mcp::pg_tests::agent_runs_three_table_analytical_query_through_mcp`. Arm the
existing `AnalyticalExecutePause` on one follower, start the cancellable MCP
query, wait for `await_execute_paused`, record the coordinator's existing
Analytical cancelled-outcome duration observation total, send
`RequestHandle::cancel`, release the pause, await every baseline, and then
assert the observation increased before `client.cancel`. The current ordering
must fail because it neither reaches the pause nor checks settlement before
disconnect.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=mcp::pg_tests::agent_runs_three_table_analytical_query_through_mcp)' --run-ignored=all"
```

**GREEN.** Change no production cancellation code unless the new journey
exposes a real defect. Reuse `ProcessNode::{arm_execute_pause,
await_execute_paused,release_execute_pause,metric_totals_labeled}` and
`await_baseline`; keep the same topology and SQL. Sample the existing
Analytical cancelled-outcome duration observation before cancellation and
assert its delta after baseline restoration while the connection is still open
so server shutdown or transport teardown cannot satisfy the claim.

**REFACTOR.** Add no new stall, observer, process command, timeout, reason API,
or telemetry family. The pause and existing production duration metric already
prove the required edge.

## Cross-scenario decisions and authority

- Actionable feedback uses the existing invalid-SQL public error. It exposes a
  fixed repair action, never DataFusion's message, identifiers, candidate
  fields, or schema shape.
- rmcp invalid params are reserved for adapter-local request-shape and local
  ceiling validation. Authenticated application failures remain structured
  Wyrd tool results.
- A terminal is provisional until clean EOF. MCP validates the same terminal
  contract as the Rust client without calling that external client or moving
  Oracle lifecycle ownership into the adapter.
- `WyrdMcpHandler` is the only dependency-owning MCP adapter struct.
- The default-off connectivity probe and exact three-tool ordinary catalog do
  not change.

Authority: `AGENTS.md` §§2, 5, 9, 11;
`architecture/agent-rules.md`;
`architecture/bifrost-design.md` §§Query: Oracle, Read audit and terminal
contract, Public surface;
`architecture/wyrd-design.md` §§Doctrine 20, Client model, MCP;
`architecture/wyrd-doctrine.mdx`;
`architecture/wyrd-security-posture.md`;
`architecture/references/languages/{errors,implementation-execution,testing-workflows}.md`;
`architecture/references/domain/{datafusion,olap-serving,analytical-operations-reliability}.md`;
and `changes/active/bifrost-distributed-analytics-engine/spec.md` revision 6.

## Broader verification

After all four focused RED-GREEN-REFACTOR cycles pass, run once:

```bash
mise run fmt
mise run lints
mise run test:wyrd
mise run test:bifrost:integration:redux
mise run test:bifrost:journey:mcp
mise run test:bifrost:journey:oracle
mise run check:client-tier
mise run check:error-coverage
mise run check:tenant-isolation
mise run check:unwrap-audit
git diff --check
```

`mise run codegen:check` is not required because this remediation reuses the
existing `QueryInvalidSql` contract and keeps all MCP types local. Run it only
if implementation unexpectedly changes a generated source contract, which is a
stop condition below rather than planned work.

## Completion evidence

Record each focused RED failure and GREEN pass, then the broader commands.
Provide source and test evidence that:

- unknown-field and unsupported-function failures are safe and agent-repairable
  across the shared Oracle boundary;
- local invalid inputs use rmcp invalid params while application errors remain
  structured Wyrd problems;
- every successful MCP result follows one fully validated terminal and clean
  EOF, while every rejected stream is settled;
- the distributed cancellation query reached an active follower, emitted the
  production Analytical cancelled-outcome duration observation, and returned
  every Oracle owner to baseline before MCP disconnect; and
- no new public module, error contract, dependency, tool, owner, or generated
  artifact was added.

## Stop conditions

Return `SPEC_REVISION_REQUIRED` rather than implementing if safe actionable SQL
feedback requires exposing raw DataFusion/schema details, adding a new public
error contract, changing supported SQL, or weakening the approved repairability
requirement. Stop for plan revision if rmcp cannot distinguish local
invalid-params from tool results through its existing `ErrorData` and
`ServiceError` APIs, if the existing follower pause cannot prove an in-flight
MCP query, or if terminal validation requires changing Oracle's public stream
contract rather than consuming its existing validators.

## Execution evidence

- Independent `$wyrd-task-readiness` review: **READY**, no blocking readiness findings (2026-09-04).
- Starting candidate: `4d3a2839281ff259dda7f58089f4c63f0f2fa7ce`; working tree contained only this remediation packet.

### Scenario 1 — safe actionable planning refusals

- RED: the named unit test returned `QueryExecutionFailed` instead of `QueryInvalidSql`; the named MCP journey returned `WYRD_VALA_500_QUERY_EXECUTION_FAILED` for the unknown field.
- GREEN: both focused tests pass. Direct and Context/Diagnostic-wrapped planning/schema/unsupported errors receive the fixed describe-table/SELECT repair action; the general mapper and Internal mapping stay unchanged.
- Exact unit command correction: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support,bench-support --lib -E 'test(=oracle::tests::datafusion_query_rejections_are_safe_and_actionable)'`. The featureless command cannot compile existing analytical tests using feature-gated `runtime_inspection`; use the canonical repository test union.
- The task's exact Postgres-wrapped MCP journey command ran unchanged for RED and GREEN. `mise run fmt` passed.

### Scenario 2 — protocol input errors and handler ownership

- RED: the exact named MCP journey received a structured tool result for an unknown query key instead of `ServiceError::McpError(INVALID_PARAMS)`.
- GREEN: the same exact Postgres-wrapped command passes all local invalid-input cases, including nonempty list arguments and missing describe name, and preserves application-level SQL/catalog errors and Scenario 1 repair hints.
- List, describe, query, and the test-only schema-stall claim are inherent methods on `WyrdMcpHandler`; `mcp::bifrost` is private. The existing tracker remains held across each complete await.
- `mise run fmt` passed. No public contract or dependency changed.

### Scenario 3 — validated terminal and clean EOF

- RED: the named terminal regression returned successful rows for an empty source-completion matrix.
- GREEN: the exact named terminal test passes nine cases: matrix, row count, malformed EOS, valid failed terminal, duplicate terminal, trailing batch/error, missing terminal, and cancellation while awaiting missing EOF. All untrusted streams cancel before return; a valid failed terminal preserves `QueryTimeout` and does not cancel again.
- The exact named positional/exact-byte test passes with a valid PublishedOnly source-completion fixture. Existing decoder and contract validators are reused; no Oracle terminal production changed.
- `mise run fmt` and `git diff --check` passed.

### Scenario 4 — cancellation before disconnect

- Strengthened the existing four-process MCP journey with the existing activated-follower pause, Analytical/cancelled duration observation, and all three pre-query ownership baselines before MCP disconnect.
- RED mutation: deliberately omitted `RequestHandle::cancel` while retaining the request. The exact named journey failed: `the active MCP query did not record Analytical cancellation before disconnect`. Ordinary completion cannot satisfy the new assertion.
- GREEN: restored the cancel call; the exact named Postgres-wrapped Oracle journey passed (18.6 seconds). No production cancellation behavior changed and no new fixture control or telemetry family was added.
- `mise run fmt` passed.

### Final inspection correction — preserve non-input wrapper mappings

- Added cases to the same named planning test for contextual tenant, reconciliation, audit, and resource failures. RED showed recursive delegation discarded a tenant-bearing outer context and returned `QueryExecutionFailed`.
- GREEN: unwrap only to classify input errors, but delegate every other failure with its original complete chain. The exact corrected unit command passes all cases; no general mapper changed.
