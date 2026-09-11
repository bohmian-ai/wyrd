---
id: SPEC-object-scoped-rbac
revision: 4
status: approved
approved_at: 2026-09-10
---

# Object-scoped RBAC

## Objective

Bring Wyrd RBAC in line with the standard operation/object model and use it to
grant Bifrost query access to specific schemas or tables. Wyrd retains one
role-derived permission vocabulary and one synchronous checker; static grants
do not enter the Policy plane.

## Requirements

### REQ-001 — Wyrd RBAC includes typed object scope

`Permission` shall carry `resource`, `action`, and `PermissionScope`.
`PermissionScope` is a closed domain-tagged union whose initial variants are
`All` and `Bifrost(BifrostPermissionScope)`. The Bifrost scope is a closed
union of schema and table scope.

- `All` covers all objects for the permission's covered resource and action;
  it does not grant another resource or action.
- Schema scope identifies one canonical tenant-local Bifrost catalog and
  schema pair and covers current and future tables resolved beneath it.
- Table scope carries the stable server-managed table UID. Its resolved target
  also carries canonical catalog and schema identity so the synchronous
  checker can evaluate schema containment. It covers only that UID and does
  not transfer to a dropped-and-recreated table.

The persisted JSON remains the direct typed projection:

```json
{"resource":"bifrost_query","action":"read","scope":"all"}
{"resource":"bifrost_query","action":"read","scope":{"bifrost":{"schema":{"catalog":"vala","schema":"logs"}}}}
{"resource":"bifrost_query","action":"read","scope":{"bifrost":{"table":{"catalog":"vala","schema":"traces","table_uid":"<canonical table uid>"}}}}
```

`scope` is required for every permission. All repository-owned grants that do
not target an object explicitly use `All`; scope-less input is rejected. No
compatibility decoder or migration shim is required because this contract has
not shipped. Invalid resource/scope combinations and malformed identities are
rejected. Bifrost object scope is valid only for Bifrost query reads.

Role permissions remain the sole static grant model, persist through the
existing role permission JSON, and resolve into the principal's existing
effective `PermissionSet`. Coverage requires resource, action, and scope
coverage. Grants remain additive and absence remains deny; no explicit-deny
precedence is added.

The active design, security posture, foundation permission docs, Rust structs,
persisted contract, generated contracts, and public documentation shall all
describe this operation/object RBAC model. They shall no longer equate RBAC
with object-free route permissions or reject permission scope.

### REQ-002 — Bifrost authorizes every resolved table

The public query boundary may reject a principal with no applicable Bifrost
query-read grant, but final object authorization occurs after Oracle resolves
the complete logical scan set and before physical planning, admission, audit
acceptance, peer dispatch, or source IO. Every underlying table in a direct
query, join, view expansion, or other multi-table plan must be covered; one
unauthorized table denies the complete query without returning rows.

Authorization uses the verified tenant and catalog-resolved table identity,
never SQL text, aliases, caller-supplied tenant data, or unverified names.
Interactive, analytical, HTTP, gRPC, SDK, and MCP paths converge on the same
decision. Distributed authority binds the coordinator-approved scoped decision
so workers cannot widen the table set.

Table query permission governs every column. Sensitive-column declarations
remain descriptive metadata and do not create a second authorization check.

### REQ-003 — Focused role journeys prove distinct access

One focused real-server Bifrost journey shall seed tenant-local custom roles
with this matrix:

| Role | Grant | Accepted | Rejected |
|---|---|---|---|
| `analyst` | schema-scoped `bifrost_query:read` for catalog `vala`, schema `logs` | non-sensitive projection from `vala.logs.records` | `vala.traces.spans` and a mixed logs/traces query |
| `data_scientist` | table-scoped `bifrost_query:read` for `vala.traces.spans` | non-sensitive projection from `vala.traces.spans` | `vala.logs.records` and a mixed traces/logs query |

The journey asserts the stable authorization rejection and that denials return
no rows. These roles are test fixtures, not new builtin production roles.
Focused unit and Oracle tests separately prove scope validation, schema
containment, exact UID matching, and distributed authority binding.

## Invariants

- **INV-001:** Tenant identity comes only from the verified principal; scope
  never selects or widens tenancy.
- **INV-002:** One `PermissionSet` and synchronous `PermissionCheck` decide
  static grants without a query-path database or network-policy lookup.
- **INV-003:** Missing, malformed, mismatched, or unresolved object authority
  fails closed before data IO and cannot produce partial success.
- **INV-004:** Wildcard resource/action behavior grants object-wide access only
  when paired with `All` scope.
- **INV-005:** Scoped decisions participate in existing revocation epoch,
  audit, and distributed permission-digest boundaries.

## Scope and non-goals

In scope: the typed and persisted permission contract; role decoding,
resolution, validation, and subsumption; Bifrost query enforcement; conflicting
architecture/docs and generated contracts; focused unit, resolver/Oracle, and
one real-server role journey.

Not in scope:

- ABAC, CEL, row filters, column masks, or query-path Policy calls;
- another grant store, checker, cache, or SQL-compatible `GRANT` language;
- raw globs, explicit deny, ownership, or grant-option semantics;
- scoped Bifrost ingest, table management, or private peer permissions;
- column-level authorization or sensitive-column classification redesign;
- non-Bifrost object scopes or new builtin analyst/data-scientist roles; and
- compatibility aliases or alternate authorization paths.

## Expensive-to-reverse decisions and boundaries

- Object scope is part of the single public and persisted `Permission`
  contract.
- Scope is domain-tagged; future Wyrd domains add typed variants only when
  needed instead of sharing a string object language.
- Scope is required; every repository-owned permission is updated and no
  compatibility form is retained for the unshipped contract.
- Bifrost uses its existing table UID plus explicit canonical catalog and
  schema identity. Its current flattened internal namespace is not exposed as
  the scoped RBAC contract. No new catalog/schema registry or second permission
  persistence model is introduced.
- Schema grants include future tables; exact grants follow table UID.
- `PermissionCheck` keeps its principal-and-permission signature because the
  required object lives in `Permission`.

## Acceptance criteria

- **AC-001:** Round-trip serialization, required-scope rejection, validation,
  and three-axis subsumption prove the approved contract without widening
  resource or action.
- **AC-002:** Tenant-scoped role resolution produces scoped effective
  permissions through the existing path and preserves cache/epoch behavior.
- **AC-003:** The `analyst` journey accepts the logs query and denies the traces
  and mixed-table queries before returning rows.
- **AC-004:** The `data_scientist` journey accepts the traces query and denies
  the logs and mixed-table queries before returning rows.
- **AC-005:** Focused tests prove schema containment, exact table UID coverage,
  no distributed widening, and stable remote query denials.
- **AC-006:** Architecture, generated contracts, formatting, lint, and affected
  boundary checks pass without the repository gate or complete
  Bifrost/platform test suites.

## Open material decisions

None.

## Authority

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/wyrd-security-posture.md`
- `architecture/bifrost-design.md`
- `architecture/references/domain/olap-serving.md`
- `architecture/references/languages/spec-driven-development.md`
- [NIST Role Based Access Control](https://csrc.nist.gov/projects/role-based-access-control)

This approved specification intentionally replaces the previous object-free
RBAC rule. Static operation/object grants remain RBAC.

## Revision history

- **Revision 4 — 2026-09-10 — approved.** Removes unused column-level payload
  permissions and enforcement. Sensitive-column metadata remains descriptive;
  scoped table query permission governs the complete row.
- **Revision 3 — 2026-09-10 — approved.** Represents Bifrost identities as
  explicit catalog, schema, and table components and removes compatibility
  behavior because no permission contract has shipped.
- **Revision 2 — 2026-09-10 — approved.** Adds the explicitly required
  industry-aligned documentation/contract revision, Bifrost enforcement, and
  focused `analyst`/`data_scientist` access matrix.
- **Revision 1 — 2026-09-10 — approved.** Established the nested
  `PermissionScope::Bifrost` decision and one-task boundary.
