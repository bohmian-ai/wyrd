---
id: BIFROST-R4-T05-MCP
title: Project the bounded terminal-safe query lifecycle through MCP
kind: implementation
mode: RECONCILE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 4
depends_on: [BIFROST-R4-T04-PRODUCTION-ACTIVATION]
requirements: [REQ-001, REQ-008, REQ-010]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-008]
acceptance: [AC-006, AC-008]
parent_task: BIFROST-R3-T3-FIRST-CLASS-PROJECTIONS
frozen_candidate: f1ac4cb01fe9ddda0a133cb58c955bab1e1cf7df
---

# MCP query projection

## Outcome and value

The existing agent-facing `bifrost.query` tool exposes Task 4's shared Rust
query behavior with closed bounded input, the selected-path terminal, canonical
structured errors, and no successful truncation. MCP validates its own caller
ceilings and serializes the completed SDK result or error; it does not own query
collection, cancellation, draining, or settlement.

Required execution skill: `$wyrd-implement`.

## Current-state amendment and owners

- `vala-sdk::QueryClient::collect_bounded` is the sole reusable collection,
  cancellation, drain, terminal validation, and settlement owner. Task 4
  upgrades that method; `BifrostQueryTool::invoke` calls the upgraded method
  unchanged exactly once.
- `wyrd-mcp::bifrost::BifrostQueryTool` owns the runtime tool schema, an
  explicit allowed-key check, parsing of MCP-only ceilings, request projection,
  and JSON serialization after `collect_bounded` returns.
- Keep the existing `serde_json::Value` parser. `wyrd-mcp/Cargo.toml` gains no
  `serde` dependency and no serde-backed input wrapper.
- `wyrd-testing::WyrdTestCluster` owns the MCP journey topology. Its selected
  `WyrdTestServer` already owns the connected one-shot `QueryStreamFaultController`
  used for post-selection transport failure; Task 5 adds no child-process
  registration, cluster abstraction, malformed-terminal producer, or new fault
  infrastructure.

The accepted input keys are exactly `sql`, `visibility`, `freshness`,
`deadline_ms`, `max_rows`, and `max_bytes`. No tenant, principal, execution
path, query class, topology, graph, plan, or cancellation field is accepted.
Authentication, tenant, permissions, routing, audit, and durable lifecycle stay
server-owned.

## Ordered implementation scenarios

### Scenario 1 — Closed input with exact caller ceilings

**Behavior.** The runtime schema and invocation accept only:

- required non-whitespace `sql`, measured as UTF-8 bytes, `1..=65_536`;
- optional `visibility`, `published_only | fused`, default `published_only`;
- optional `freshness`, `strict | allow_degraded`, default `strict`;
- optional integer `deadline_ms`, `1..=4_294_967_295` (`u32::MAX`);
- optional integer `max_rows`, `1..=10_000`, default `1_000`; and
- optional integer `max_bytes`, `1..=16_777_216`, default `4_194_304`.

The SQL ceiling matches Oracle's existing 64 KiB default and is enforced by
`sql.len()` before transport. An unknown key, wrong JSON type, empty/whitespace
SQL, oversized SQL, or out-of-range deadline/row/byte value returns the existing
`ToolError::InvalidInput` contract (`SKALD_TOOL_422_INPUT`, status 422) with the
field and accepted range in its detail. Server rejection of syntactically
invalid or unsupported SQL remains `BifrostError::QueryInvalidSql`
(`WYRD_VALA_400_QUERY_INVALID_SQL`); MCP does not parse SQL. Maps REQ-001,
REQ-010, INV-001, INV-002, INV-005, INV-008, AC-006.

**RED.** Add the in-module test
`bifrost::tests::bifrost_query_input_contract_is_closed_and_bounded`. Assert the
schema has exactly the six keys and `additionalProperties: false`, its numeric
maximums match the limits above, defaults are unchanged, the explicit
allowed-key check rejects `tenant`, `path`, and an arbitrary key, and every
invalid local value returns `SKALD_TOOL_422_INPUT` without issuing HTTP. Exact:

```bash
mise exec -- cargo nextest run --locked -p wyrd-mcp --lib -E 'test(=bifrost::tests::bifrost_query_input_contract_is_closed_and_bounded)'
```

**GREEN.** In `crates/wyrd/wyrd-mcp/src/bifrost/mod.rs`, keep parsing the
`serde_json::Value` object. Before extracting fields, compare every object key
against the six-key constant set and return `ToolError::InvalidInput` for the
first unknown key. Validate SQL byte length, parse `deadline_ms` through
`Value::as_u64` plus `u32::try_from`, place it in the existing
`BifrostQueryRequest.deadline_ms` as `u64::from(value)`, and keep
`requested_limits` as the row/byte parser. Reflect the same exact bounds in
`BifrostQueryTool::input_schema`; do not add a request type or dependency.

**REFACTOR.** Local helpers remain synchronous deterministic parsers owned by
the MCP module. Shared request validation and every server decision remain in
their existing owners.

### Scenario 2 — One SDK collection owner and canonical error projection

**Behavior.** `BifrostQueryTool::invoke` constructs one
`BifrostQueryRequest`, calls the Task-4-upgraded
`QueryClient::collect_bounded(&request, limits)` unchanged, and serializes only
its completed `CollectedQueryResult` or `ValaSdkError`. MCP never iterates the
query stream or independently cancels, drains, polls running-query state, or
settles an overflow. A row/byte overflow remains
`ValaSdkError::ResultTooLarge` (`WYRD_VALA_413_QUERY_RESULT_TOO_LARGE`); a
deadline terminal remains `BifrostError::QueryTimeout`
(`WYRD_VALA_504_QUERY_TIMEOUT`). Every structured MCP failure exposes exactly
`code`, `status`, `title`, `detail`, `remediation`, and `details`.
`ValaSdkError::safe_details()` supplies the value serialized under `details`;
`safe_details` remains an internal accessor and is not a second public field.
Maps REQ-008, REQ-010, INV-003, INV-004, INV-005, AC-006.

**RED.** Add only the focused in-module projection test
`bifrost::tests::bifrost_query_error_projection_uses_canonical_details`.
Project representative transport, failed-terminal, timeout, protocol, and
result-too-large `ValaSdkError` values. Assert their existing canonical
code/status/title/detail/remediation values, the scrubbed accessor value under
the public `details` key, and absence of a public `safe_details` key. Exact:

```bash
mise exec -- cargo nextest run --locked -p wyrd-mcp --lib -E 'test(=bifrost::tests::bifrost_query_error_projection_uses_canonical_details)'
```

Task 4's `vala-sdk` tests remain the sole proof for overflow cancellation,
drain, malformed/missing/duplicate/inconsistent terminal handling, and
settlement. Do not reproduce those streams or assertions in `wyrd-mcp`.

**GREEN.** Keep the current direct `QueryClient::new(...).collect_bounded(...)`
control flow. Reduce `query_tool_error` to projection through the existing
`ValaSdkError::{code,status,title,detail,remediation,safe_details}` accessors;
do not pattern-match to recreate lifecycle behavior or recover metadata from
formatted prose. Update the runtime error schema/serialization so the scrubbed
value is named `details` publicly and `safe_details` is not emitted.

**REFACTOR.** `vala-sdk` remains the only client lifecycle owner. MCP owns only
input parsing and completed value/error serialization.

### Scenario 3 — Executable agent-facing journey

**Behavior.** A discovered MCP tool drives real Interactive and Analytical
queries, a post-selection transport failure, a caller row ceiling, and a caller
deadline value through the production server and Task 4 client. Success
contains the complete rows and selected path. Failure contains no partial
payload, preserves the canonical structured error, and returns only after
Oracle ownership reaches the Task 4 settlement state. Maps REQ-001, REQ-008,
REQ-010, INV-001, INV-003, INV-004, INV-005, AC-006, AC-008.

**RED.** Add
`rbac::pg_tests::bifrost_query_paths_bounds_failure_and_cleanup` to the existing
`wyrd-mcp --test mcp` target. Start exactly
`WyrdTestCluster::start_spec(BifrostClusterSpec::three_oracles_one_scribe())`,
seed through the Scribe server, and invoke the registered `bifrost.query` tool
against an Oracle server with one shared authenticated caller. Drive:

1. one Interactive success and one exchange-requiring Analytical success;
2. `max_rows` one below the real result, expecting
   `WYRD_VALA_413_QUERY_RESULT_TOO_LARGE` and no result payload;
3. one successful query with an explicit in-range `deadline_ms`, proving MCP
   forwards the value into Task 4's unchanged client request; and
4. a supported Analytical query after
   `WyrdTestServer::fail_next_query_after_batch`, expecting Task 4's settled
   `WYRD_VALA_502_QUERY_STREAM_INCOMPLETE` projection with no successful partial
   payload or Interactive rerun.

For every case compare the resource fields from
`WyrdTestCluster::oracle_inspection()` with their baseline and assert active
queries, peer work, memory, and scratch/spill ownership are zero before MCP
returns. For every Oracle server, also assert the existing
`server.state().bifrost_query().engine().graph_lease_counts()` live count is
zero. Prove the one-shot fault was consumed by running the next query
successfully. Do not synthesize malformed terminals; Task 4 owns that
settlement proof. Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey -E 'test(=rbac::pg_tests::bifrost_query_paths_bounds_failure_and_cleanup)' --run-ignored=all"
```

**GREEN.** Wire no new lifecycle or harness path. Use the cluster's existing
servers, connected `QueryStreamFaultController`, production HTTP client, tool
registry, and inspection APIs. The production change remains limited to
Scenario 1 parsing and Scenario 2 completed-result/error projection; the
journey must pass through the unchanged `QueryClient::collect_bounded` call.

**REFACTOR.** MCP stays a thin permissioned projection. Routing, audit,
admission, failure, cancellation, and graph cleanup remain server/Oracle- and
`vala-sdk`-owned.

## Cross-scenario decisions and authority

- There is one lifecycle owner: Task 4's `vala-sdk` query stream and
  `QueryClient::collect_bounded`. MCP never receives an unsettled collection
  result.
- The six-key MCP input is closed in both the runtime schema and invocation;
  JSON Schema alone is not trusted to reject unknown keys.
- The 65,536-byte SQL ceiling is MCP-owned request budgeting, not SQL parsing or
  authorization. Oracle remains authoritative and may reject within its own
  configured server limits.
- The MCP journey uses `WyrdTestCluster`, because the `wyrd-mcp` test target
  cannot launch `wyrd-testing`'s separately registered child binary. Task 4's
  `BifrostProcessCluster` journeys retain cross-process acceptance ownership.
- Post-selection failure uses only the selected server's already connected
  `QueryStreamFaultController`; the cluster's currently data-only
  `OracleFaultController` is not a reachable production transport seam and is
  not expanded by this task.
- Malformed terminal production and settlement remain Task 4 evidence. Task 5
  proves only canonical MCP projection of the resulting SDK error.

Authority: `architecture/bifrost-design.md` §§Query: Oracle, Read audit and
terminal contract, Public surface; `architecture/wyrd-design.md` §§Doctrine 20,
Client model; `architecture/wyrd-security-posture.md` §§Trust boundaries,
Authorization and policy, Peer identity and distributed Oracle;
`architecture/references/languages/agent-harness.md` §§Tool contracts,
Validation; `architecture/references/languages/errors.md` §§Error codes,
Boundary conversion; `architecture/references/languages/testing-workflows.md`;
and `AGENTS.md` §§9, 11.

## Broader verification

```bash
mise run fmt
mise run lints
mise run test:wyrd
mise run test:bifrost:journey:mcp
mise run check:client-tier
mise run check:error-coverage
git diff --check
```

The MCP runtime catalog is owned by the MCP tests; Task 5 does not change a
generated contract and therefore does not add `codegen:check` as duplicate
proof.

## Completion evidence and stop conditions

Provide the closed six-key runtime schema and negative validation evidence,
the unchanged `collect_bounded` call site, canonical public `details` mapping,
real MCP Interactive/Analytical terminals and errors, and zero retained Oracle
ownership before return. Return `SPEC_REVISION_REQUIRED` if implementation
requires a divergent request/error contract, caller-selected path or tenant,
successful truncation, a second lifecycle owner, new dependency/feature, new
fault infrastructure, or weakened permission, tenant, audit, deadline, or
terminal semantics.
