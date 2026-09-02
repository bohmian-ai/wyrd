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

The existing agent-facing `bifrost.query` tool exposes the same server-owned raw
SQL lifecycle with a closed bounded input, typed selected-path terminal,
structured errors, and no successful truncation. Ceiling, cancellation,
transport, protocol, or server failure drains and settles the query before the
tool returns failure. Tenant and principal remain server-derived.

Required execution skill: `$wyrd-implement`.

## Current-state amendment and owners

Retain `wyrd-mcp`'s existing query tool, Task 4's shared `vala-sdk` settlement,
RBAC journey target,
and generated tool catalog owner. Extend the current schema/result with terminal
path and exact lifecycle cleanup. `max_rows`/`max_bytes` remain caller ceilings
beneath server hard limits. No path/class/topology/plan field is accepted.
All client-neutral request conversion, stream decoding, terminal/error
validation, deadlines, cancellation, and settlement remain in `vala-sdk`; MCP
owns only tool schema, permission binding, caller ceilings, and JSON projection.

Do not add EXPLAIN, a second MCP query tool, client routing, trusted tenant
input, successful truncation, or a separate MCP cancellation/lifecycle owner.

## Ordered implementation scenarios

### Scenario 1 — Closed bounded input and server identity

**Behavior.** Input accepts SQL, visibility, freshness, optional deadline,
`max_rows`, and `max_bytes` only; validates UTF-8 SQL bytes and positive bounded
ceilings before transport; unknown/path/tenant fields fail. Authentication,
tenant, and sensitive-column authority come from the server caller context.
Maps REQ-001, REQ-010, INV-001, INV-002, INV-005, INV-008, AC-006.

**RED.** Add
`rbac::bifrost_query_schema_is_closed_bounded_and_server_tenant_bound` to the
MCP journey target. Assert catalog schema closure and exercise unknown fields,
path selector, tenant override, invalid SQL/deadline/ceilings, and
under-privileged token with stable errors. Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey -E 'test(=rbac::bifrost_query_schema_is_closed_bounded_and_server_tenant_bound)' --run-ignored=all"
```

**GREEN.** Keep one serde-deny-unknown-fields input owned by `wyrd-mcp`; validate
caller ceilings locally and build the unchanged shared `BifrostQueryRequest`.
Never copy tenant/principal into the request. Map authorization and validation
through canonical Wyrd errors.

**REFACTOR.** Shared client request validation remains `vala-sdk`-owned; MCP
adds only tool-schema validation, permission binding, and agent-output ceilings.

### Scenario 2 — Complete terminal or structured ceiling failure

**Behavior.** Success returns all emitted rows within ceilings plus the validated
selected path/terminal. Crossing a row/byte ceiling requests cancellation,
drains settlement, and returns failure with no partial success. Malformed,
missing, duplicate, or inconsistent terminal is protocol failure. Every failure
uses the canonical MCP wire fields `code`, `status`, `title`, `detail`,
`remediation`, and `details`; the existing scrubbed SDK accessor supplies
`details` but `safe_details` is not a second public field. Maps REQ-008,
REQ-010, INV-003, INV-004, INV-005, AC-006.

**RED.** Add
`rbac::bifrost_query_never_reports_successful_truncation`. Exercise exact-bound,
one-row-over, one-byte-over, malformed terminal, and cancellation failure;
assert no successful partial payload, exact canonical error keys with no
`safe_details` key, and zero server ownership before return.
Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey -E 'test(=rbac::bifrost_query_never_reports_successful_truncation)' --run-ignored=all"
```

**GREEN.** Drain the shared terminal-safe stream incrementally while checking
rows and encoded response bytes before adding each batch. On prospective
overflow, call Task 4's shared stream cancellation/settlement owner and await it
under the original query deadline,
then return a structured ceiling error without rows. On success, require and
project the validated terminal/path. Serialize the scrubbed value returned by
`ValaSdkError::safe_details()` under the canonical public `details` key and
update the generated tool catalog/schema and RBAC assertions accordingly. Do
not collect beyond the configured response ceiling.

**REFACTOR.** `vala-sdk`'s one settlement owner covers protocol, transport, and
caller cancellation. MCP detects its response ceiling, invokes that owner, and
does not duplicate server lifecycle or stream policy.

### Scenario 3 — Real agent-facing path journey

**Behavior.** MCP drives real Interactive and Analytical queries, one
post-selection failure, ceiling/cancellation cleanup, canonical structured
errors, selected paths, original-deadline parity, and zero retained MCP/client
state and server query ownership. Maps REQ-001, REQ-008, REQ-010, AC-006,
AC-008.

**RED.** Add
`rbac::bifrost_query_paths_bounds_failure_cancel_and_cleanup` to the existing
MCP target using the real server and client. It must not repeat Task 3's
operator matrix. Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey -E 'test(=rbac::bifrost_query_paths_bounds_failure_cancel_and_cleanup)' --run-ignored=all"
```

**GREEN.** Select Task 4's existing test-tier multi-node test-server composition
and drive the production MCP handler. Assert telemetry from Oracle's production
owner; add no MCP-specific server lifecycle or test-only engine events.

**REFACTOR.** MCP remains a thin permissioned projection; routing, audit,
admission, and graph cleanup remain server/Oracle-owned.

## Broader verification

```bash
mise run fmt
mise run lints
mise run test:bifrost:journey:mcp
mise run test:bifrost:journey:server
mise run codegen:check
mise run check:client-tier
mise run check:error-coverage
git diff --check
```

## Completion evidence and stop conditions

Provide runtime catalog/schema proof, closed-input negative cases, exact ceiling
and no-truncation evidence, real MCP journey terminals/errors, deadline parity,
and zero MCP/client/server-query ownership snapshots. Return
`SPEC_REVISION_REQUIRED` if MCP requires
a divergent wire contract, path/tenant selector, successful truncation,
client-owned durable lifecycle, new dependency/feature, or weakened permission,
tenant, audit, or terminal semantics.

## Authority

`architecture/references/languages/agent-harness.md`,
`architecture/references/languages/errors.md`,
`architecture/wyrd-security-posture.md`, and `AGENTS.md` §§9, 11.
