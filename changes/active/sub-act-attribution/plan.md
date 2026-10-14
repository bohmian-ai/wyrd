# Sub/act attribution implementation plan

Status: `TASKS_PROPOSED`  
Approved specification: `spec.md`, revision 1  
Task count: exactly one

## Plan

| Task | Outcome | Obligations | Dependency |
|---|---|---|---|
| [TASK-001](tasks/TASK-001-sub-act-attribution.md) | Replace every in-scope work attribution with the server-derived `sub`/`act` pair, carry it through Postgres, Bifrost, Scribe, gateway, verification, audit, registration, fixtures, contracts, and all first-class SDKs, and prove the complete user journeys. | REQ-001..REQ-007, INV-001..INV-005, AC-001..AC-009 | Approved spec r1 and the existing owners named in the task; no preceding implementation task. |

The work is intentionally indivisible. The managed-column fingerprint, defining
Postgres history, runtime authorization/accounting, verification authority,
audit projection, generated contracts, and SDK journeys form one contract cut;
an intermediate task boundary would leave persistent and public surfaces with
different attribution meanings.

## Integrated proof

TASK-001 owns all executable, schema, documentation, generation, and journey
proof. No migration, compatibility alias, backfill, or transitional state is
permitted. Change review should evaluate the single cumulative task against the
approved specification after its focused scenarios and scoped lanes pass.

## Decision check

The approved specification resolves the durable names, attribution semantics,
authorization and accounting subject, idempotency actor, SYSTEM behavior,
schema-history policy, SDK closure, and excluded work. No product, architecture,
security, compatibility, concurrency, or persistent-data decision remains for
planning.
