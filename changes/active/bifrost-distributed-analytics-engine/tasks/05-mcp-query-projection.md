---
id: BIFROST-R5-T05-MCP
title: Project bounded Bifrost discovery and query journeys through MCP
kind: implementation
mode: RECONCILE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 5
depends_on: [BIFROST-R5-T05-PRE-MCP-BUILDOUT]
requirements: [REQ-001, REQ-008, REQ-010, REQ-012]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-008, INV-009]
acceptance: [AC-003, AC-004, AC-006, AC-007, AC-008, AC-009]
parent_task: BIFROST-R4-T05-MCP
frozen_candidate:
  commit: 5d3cc09f75d0d3583164baeb481179ed92358808
  task_blob: 52ede2f8a23ad5187778021dc1274231ac02b093
  uncommitted: none
---

# Bifrost MCP query projection

## Outcome and value

An authenticated agent discovers exactly three read tools through the real
`/mcp` endpoint, uses compact catalog and physical-layout descriptions to form
bounded SQL, and receives one complete positional result only after Oracle's
successful terminal and ownership settlement. Oracle—not the caller or MCP
adapter—selects Interactive or Analytical.

Required execution skill: `$wyrd-implement`.

## Reconciliation amendment

### Retained

- The closed query fields `sql`, `visibility`, `freshness`, `deadline_ms`,
  `max_rows`, and `max_bytes`, including the frozen MCP limits.
- Canonical structured Wyrd errors, no successful truncation or partial
  result, server-selected path evidence, and zero retained Oracle ownership.
- Task 04's production query behavior, now completed through Task 04A, remains
  unchanged and is satisfied transitively through Task 05-pre.

### Deleted

- `bifrost.list_permissions`, `bifrost.list_errors`, and every separate schema,
  partition, filter, cluster, join-hint, topology, validation, preflight, or
  public `EXPLAIN` tool.
- Skald `AgentTool`/`ToolRegistry` ownership, `ToolError` as a public contract,
  the standalone MCP binary, and server dependencies on Skald.
- `WyrdClient`, `vala_sdk::QueryClient`, or any other client call from the
  server back into itself.
- Object-shaped rows, in-process tool invocation as journey evidence, result
  rows in progress notifications, and caller-selected execution paths.

### Invalidated and unfinished

- The frozen Task 05's Skald and SDK-loopback architecture is invalidated by
  approved Revision 5.
- This task now owns only the three Bifrost tool adapters, direct authenticated
  application calls, bounded final serialization, and the two real agent
  journeys. Task 05-pre owns all reusable MCP transport/client/auth plumbing.

## Owners and exact write set

- `crates/wyrd/wyrd-server/src/mcp/{mod.rs,bifrost.rs}`: register exactly
  `bifrost.list_tables`, `bifrost.describe_table`, and `bifrost.query` on the
  Task 05-pre handler. Keep MCP request/result types local to this adapter.
- `crates/wyrd/wyrd-server/src/bifrost/service.rs`: existing authenticated
  `list_tables(&AppState, Caller)` and `describe_table(&AppState, Caller, ...)`
  remain the discovery owners; no behavior change is expected.
- `crates/wyrd/wyrd-server/src/query/service.rs`: existing authenticated
  `stream_query(AppState, Caller, BifrostQueryRequest)` and its returned
  `OracleQueryStream` remain the authorization and lifecycle owners; no query
  service behavior change is expected.
- `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/{main.rs,discovery.rs,query.rs}`:
  replace the obsolete `layout` and `rbac` module registrations with ordinary
  `discovery` and `query` modules for local client-to-`/mcp`-to-server
  journeys. Reuse the narrowest existing `wyrd-testing` catalog, OTEL,
  cancellation, and inspection fixtures; add no generic harness framework.
- `crates/wyrd/wyrd-testing/{Cargo.toml,tests/bifrost/oracle/main.rs,tests/bifrost/oracle/mcp.rs}`:
  add `rmcp` only as test plumbing, register `mod mcp`, and own the one
  cross-process MCP analytical journey that requires the existing
  `bifrost_peer_test_node` capability binary and `BifrostProcessCluster`.

Existing `wyrd-spec` catalog/query/terminal types are source values. This task
adds no shared public contract or generated schema unless implementation proves
the existing language-neutral source type cannot represent a required value.

## Non-goals

No MCP transport, listener, service, session store, authentication client,
Skald integration, progress-row stream, public plan/validation operation,
agent-selected path, Bifrost write, DML, CTAS, or ingest tool is in scope.
Bifrost writes require a separately approved change covering scopes,
idempotency, audit, payload bounds, and write journeys.

## Ordered implementation scenarios

### Scenario 1 — Compact authorized discovery through exactly three tools

**Trace.** REQ-010, REQ-012; INV-002, INV-005, INV-008, INV-009; AC-006,
AC-008, AC-009. Revision 5 discovery contract and both journey discovery
steps.

**RED.** Add ignored journey
`discovery::pg_tests::agent_discovers_only_authorized_tables_and_layout`. Connect a real
authenticated rmcp client and assert `tools/list` exposes exactly the three
`bifrost.*` names. Seed two tenants. Assert `bifrost.list_tables` returns only
the caller's authorized compact `namespace`, `name`, and `status` values.
Assert `bifrost.describe_table` returns the selected table's schema, every
field's metadata, event-time partition granularity, ordered sort keys, and
Bloom columns. An absent/foreign table returns the existing canonical Wyrd
error; an under-scoped principal is denied.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey -E 'test(=discovery::pg_tests::agent_discovers_only_authorized_tables_and_layout)' --run-ignored=all"
```

**GREEN.** Register three concrete tool methods. Derive `Caller` only from the
Task 05-pre verified request extension and call the existing catalog service
methods with `AppState`. Project only the compact list fields; project the
existing full field and physical-layout description without a second lookup or
new registry.

**REFACTOR.** Remove the old Skald registry and its extra discovery tools. Do
not add a generic catalog adapter: the two existing server operations are the
reusable seam.

### Scenario 2 — Closed bounded query and earliest-owner refusal

**Trace.** REQ-001, REQ-008, REQ-010; INV-001, INV-002, INV-003, INV-005,
INV-008; AC-003, AC-004, AC-006. Revision 5 query-input and early-refusal
obligations.

**RED.** Add unit test
`mcp::bifrost::tests::query_schema_is_closed_bounded_and_has_no_path_selector`.
Assert the request schema has exactly six properties and rejects unknown keys,
including tenant, principal, delegation, roles, execution path, query class,
topology, and plan. Assert these exact MCP-local constraints:

- `sql`: required non-whitespace UTF-8, `1..=65_536` bytes;
- `visibility`: `published_only | fused`, default `published_only`;
- `freshness`: `strict | allow_degraded`, default `strict`;
- `deadline_ms`: optional integer `1..=4_294_967_295`;
- `max_rows`: default `1_000`, hard maximum `10_000`; and
- `max_bytes`: default `4_194_304`, hard maximum `16_777_216`.

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=mcp::bifrost::tests::query_schema_is_closed_bounded_and_has_no_path_selector)'
```

Add ignored journey
`query::pg_tests::bifrost_query_errors_are_actionable_before_row_execution`. Through
the real endpoint, submit malformed SQL, multiple statements, non-SELECT SQL,
unknown tables and fields, an unsupported shape, and policy-floor violations.
Assert each fails at its existing owning boundary before rows execute and
returns repairable canonical Wyrd fields `code`, `status`, `title`, `detail`,
`remediation`, and `details` with no result payload.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey -E 'test(=query::pg_tests::bifrost_query_errors_are_actionable_before_row_execution)' --run-ignored=all"
```

**GREEN.** Parse the closed local request at the MCP boundary, map its four
query-policy values into the existing `BifrostQueryRequest`, and call the
existing `query::service::stream_query` operation directly with `AppState` and
verified `Caller`. MCP invalid parameters use rmcp's protocol-correct
invalid-params error. Authenticated Wyrd failures return `CallToolResult` with
`is_error: true` and the canonical structured public problem. Reuse the
service's existing authorization, policy-floor validation, and public error
mapping; do not reproduce them in the MCP adapter.

**REFACTOR.** Keep pure parsing helpers local and synchronous. Do not add
public preflight or `EXPLAIN`; the first actual query call owns validation.

### Scenario 3 — Journey A: interactive OTEL debugging

**Trace.** REQ-001, REQ-008, REQ-010, REQ-012; INV-001, INV-002, INV-003,
INV-004, INV-005, INV-008, INV-009; AC-003, AC-004, AC-006, AC-008, AC-009.
Revision 5 Journey A and its cancellation/ceiling/settlement obligations.

**RED.** Add ignored journey
`query::pg_tests::agent_debugs_otel_error_trace_through_mcp`. A real authenticated
rmcp client initializes, discovers the three tools, lists authorized tables,
describes the OTEL span/trace table, constructs a bounded read-only error-trace
query, and invokes it as the verified principal. Assert Oracle selects
`Interactive` and MCP returns one complete result with columns once,
positional rows, and terminal evidence. Retain the rmcp `RequestHandle` for the
cancellation case, invoke `RequestHandle::cancel(None).await`, and verify
Oracle settlement before closing the MCP client or shutting down the server. Separately prove row
ceiling refusal, serialized-byte ceiling refusal, no successful partial or
truncated response, no result rows in progress notifications, and zero active
queries, memory, peer work, scratch/spill, and graph leases before MCP returns.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey -E 'test(=query::pg_tests::agent_debugs_otel_error_trace_through_mcp)' --run-ignored=all"
```

Add unit test
`mcp::bifrost::tests::query_result_is_positional_and_counts_exact_structured_json_bytes`.
Use one synthetic Arrow batch containing duplicate column names, a null, binary
data, and a nested value. Assert the ordered column triples and positional
values follow Arrow JSON encoding, recompute the returned compact structured
content with `serde_json::to_vec`, and prove the exact byte limit succeeds
while one byte less returns `WYRD_VALA_413_QUERY_RESULT_TOO_LARGE`, retains no
candidate value past the ceiling, and settles the synthetic stream.

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=mcp::bifrost::tests::query_result_is_positional_and_counts_exact_structured_json_bytes)'
```

**GREEN.** Consume the existing `OracleQueryStream` in the MCP adapter with
the existing query decoder and terminal mapper. The tool method receives
`RequestContext<RoleServer>` and races stream consumption against
`context.ct.cancelled()` with `tokio::select!`; cancellation calls
`OracleQueryStream::cancel().await` before the tool returns its cancellation
result. Accept the schema once and project ordered columns as exactly
`{name, data_type, nullable}`, with `data_type` from Arrow's display spelling.
Project every row as an array in schema order: null slots are JSON `null`, and
non-null binary, nested, temporal, numeric, and string values use Arrow 59.2's
existing `arrow::json::writer::make_encoder` with default `EncoderOptions`,
then parse that encoded value as `serde_json::Value`. Positional rows preserve
duplicate column names without inventing another row model.

Define `max_bytes` as the exact compact UTF-8 JSON byte length produced by
`serde_json::to_vec` for the MCP structured content
`{"columns":...,"rows":...,"terminal":...}`. Exclude rmcp-owned JSON-RPC and
`CallToolResult` framing. Charge the fixed object/array delimiters, serialized
columns, every row plus its comma, and the terminal with checked arithmetic;
serialize each candidate value into a temporary buffer and cancel before
retaining it when the resulting exact count would exceed the ceiling. Accept
exactly one successful terminal and return the structured content only after
settlement. On overflow, protocol failure, or a failed/missing/duplicate
terminal, call `OracleQueryStream::cancel().await` before returning the
canonical error. Never send rows in progress notifications.

**REFACTOR.** This collector owns only MCP's lower final-result budget and
serialization. Oracle retains parsing, planning, routing, admission,
execution, cancellation, terminal settlement, and cleanup.

### Scenario 4 — Journey B: analytical three-table query

**Trace.** REQ-001, REQ-008, REQ-010, REQ-012; INV-001, INV-002, INV-003,
INV-004, INV-005, INV-008, INV-009; AC-003, AC-004, AC-006, AC-007, AC-008,
AC-009. Revision 5 Journey B.

**RED.** Add ignored journey
`mcp::pg_tests::agent_runs_three_table_analytical_query_through_mcp` in the
existing `wyrd-testing --test oracle` target. Pass
`env!("CARGO_BIN_EXE_bifrost_peer_test_node")` to `BifrostProcessCluster::start`
and use the smallest existing real three-Oracle/one-Scribe topology. An authenticated
agent lists and describes three tenant-authorized tables, uses only those
descriptions to submit a bounded supported multi-table equi-join with a
fixed-width aggregation, and supplies no path hint. Assert Oracle selects
`Analytical`, the final MCP result is complete and trustworthy, and existing
exchange/remote-execution telemetry proves the distributed path. Drive
malformed and unsupported repairs, both ceiling refusals, and caller
cancellation by retaining the rmcp `RequestHandle` and invoking
`RequestHandle::cancel(None).await`. Verify Oracle settlement before closing the MCP client
or shutting down the process cluster, and assert no successful partial result
and zero retained Oracle ownership before every return.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=mcp::pg_tests::agent_runs_three_table_analytical_query_through_mcp)' --run-ignored=all"
```

**GREEN.** Reuse the Scenario 1 descriptions and Scenario 3 query adapter
unchanged. There is no Analytical MCP branch: Oracle's existing support
predicate, admission, physical-plan proof, and execution own path selection.

**REFACTOR.** Delete any plan hint, partition inventory, topology, or
join-registry code introduced while making the journey pass. `describe_table`
already exposes the required schema and physical layout.

## Cross-scenario decisions and authority

- Tool names are exactly `bifrost.list_tables`, `bifrost.describe_table`, and
  `bifrost.query`; all are read-only.
- `bifrost.query` returns one final `{columns, rows, terminal}` value only.
  Columns occur once as ordered `{name, data_type, nullable}` entries and rows
  are positional arrays using Arrow's existing JSON value semantics.
- `max_bytes` counts only the exact compact UTF-8 JSON bytes of
  `{columns,rows,terminal}`, including its fixed delimiters and every retained
  value, and excludes rmcp-owned JSON-RPC and result framing.
- MCP defaults/hard ceilings remain below server hard limits of 1,000,000 rows
  and 256 MiB. Exceeding either MCP ceiling returns
  `WYRD_VALA_413_QUERY_RESULT_TOO_LARGE`; truncation is never success.
- The server MCP adapter consumes `AuthenticatedPrincipal` plus the separate
  `RequestId`, derives the existing `Caller`, and calls application operations
  directly with `AppState`. `wyrd-server::mcp` and its Bifrost adapters do not
  invoke or depend on `wyrd-client`, `vala-sdk`, or Skald; the existing
  server-level `wyrd-client` dependency for `ServerBifrostPeerCredentials`
  remains unchanged.
- Every cancellable MCP query method receives `RequestContext<RoleServer>`,
  races work against `context.ct.cancelled()`, and awaits
  `OracleQueryStream::cancel()` before returning cancellation.
- Oracle remains the sole query lifecycle owner. MCP owns only request
  adaptation, its lower response budget, final serialization, and protocol
  error translation.

Authority: `architecture/bifrost-design.md` §§Query: Oracle, Read audit and
terminal contract, Public surface; `architecture/wyrd-design.md` §§Doctrine 20,
Client model, MCP; `architecture/wyrd-doctrine.mdx`;
`architecture/wyrd-security-posture.md`; `architecture/agent-rules.md`;
`architecture/references/languages/{agent-harness,errors,testing-workflows}.md`;
and `AGENTS.md` §§2, 3, 9, 11.

## Broader verification

```bash
mise run fmt
mise run lints
mise run test:wyrd
mise run test:bifrost
mise run test:bifrost:journey:mcp
mise run test:bifrost:journey:oracle
mise run check:client-tier
mise run check:error-coverage
mise run check:tenant-isolation
mise run check:unwrap-audit
git diff --check
```

Run `mise run codegen:check` only if implementation changes a generated shared
contract; the planned MCP-local types do not justify duplicate generated API
surface.

## Completion evidence and stop conditions

Provide the exact three-tool catalog, compact discovery and complete layout
payloads, closed schema proof, canonical early failures, both real rmcp agent
journeys, Interactive/Analytical terminal evidence, ceiling/cancellation/no-
partial assertions, and zero retained Oracle ownership before every return.
Return `SPEC_REVISION_REQUIRED` if implementation requires another tool or
transport, caller-selected routing, a server self-call, a successful partial
result, Bifrost writes, or weaker auth, tenant, audit, floor, cancellation, or
terminal semantics. Stop for an approved-plan revision if Task 04A changes the
application/query seam or the exact terminal/cleanup evidence assumed here.
