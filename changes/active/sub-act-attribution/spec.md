---
id: SPEC-sub-act-attribution
revision: 1
status: approved
---

# Record who work was for and who did it, on every table, the RFC 8693 way

## Objective and user value

Today Wyrd records attribution four different ways. Bifrost rows carry one
managed `principal_id` (the row writer). Gateway calls add
`caller_principal_id`. Verification results and runs add
`requested_by_principal_id`. Audit uses `audit_principal_id`, and
registration tables use `created_by` or `actor`. Gateway calls and verification
results each store a principal twice with different meanings. Internal work
(scheduled and observation-triggered verification, and the judge calls inside
it) either records SYSTEM and loses who it was for, or records a real principal
as if it had acted.

After this change every table that records work carries the same two columns,
with the meaning RFC 8693 gives `sub` and `act`. Wyrd's delegated token already
has that shape: top-level `principal` plus an `act` chain. Every decision, cost
and audit row traces back to the service the work was for. The row also says
plainly whether a principal or Wyrd itself did the work.

## Approval

Approved by the human owner on 2026-10-14: "I approve the design. there is no
need for migrations. nothing has shipped".

## Definitions

- **`sub_principal_id`**: the principal the work was for. Permissions are
  checked against it, and cost is billed to it.
- **`act_principal_id`**: the principal that did the work.

When a principal does its own work, the two are equal. When Wyrd does work in
the background, `act` is the tenant SYSTEM principal and `sub` is the principal
the work was for.

## Requirements

### REQ-001 One attribution pair

Every table that records work carries `sub_principal_id` and
`act_principal_id`. Both are non-null on every row. The server stamps them, and
a client can never supply either one.

### REQ-002 Who is sub and who is act

| Work | `sub_principal_id` | `act_principal_id` |
|---|---|---|
| A principal calls the gateway | caller | caller |
| A principal calls verify directly | caller | caller |
| A delegated token (B acting for A) | A (top-level `principal`) | B (outermost `act`) |
| Manual binding run | caller | caller |
| Scheduled binding run | binding owner's principal | SYSTEM |
| Observation-triggered binding run | binding owner's principal | SYSTEM |
| Judge model call inside any Verifier run | the run's `sub` | SYSTEM |
| A client writes observations, spans, logs, metrics, or table rows | writer, or A under delegation | writer, or B under delegation |
| Card, service account, API key, operator connection, or Bifrost table registration | caller | caller, or B under delegation |

- The binding owner's principal is the principal projected for the binding's
  `owner_card_uid` Service or Agent Card.
- The observation writer is never the `sub` of an observation-triggered run.
  It stays reachable through `source_record_id`.

### REQ-003 Authorization and cost follow sub

- Every gateway call and every judge call checks the `gateway:invoke` permission
  of the call's `sub`. This holds whether the call came in through the gateway
  or was started inside Wyrd.
- A delegated external call keeps today's rule: its permissions are the
  intersection of the subject's and the actor's.
- SYSTEM as `act` adds no restriction and grants nothing. An internal call is
  authorized exactly as its `sub` would be.
- Gateway spend and admission accounting are charged to `sub`.
- SYSTEM is never a `sub`.

### REQ-004 Card columns

- `card_uid` is the Card a row is about. On verification tables that is the
  Card being verified.
- A row that references a second Card names it by its role:
  - `owner_card_uid`;
  - `verifier_card_uid`, which replaces `verifier_uid` and also replaces
    today's use of the managed `card_uid` for the Verifier on result tables.
- `subject_card_uid` no longer exists.

### REQ-005 Schemas

A plain `principal_id` remains only where it is the row's own identity or owner
key: `auth_refresh_tokens`, `platform.credentials`,
`platform.principal_grants`, `auth_login_state`, `auth_device_authorizations`
and `oracle_admission_blocks`. A table never carries both `principal_id` and
the pair. `auth_connection_test_state.tester_principal_id` is unchanged: it
names the principal holding the test, not the principal the work was for.

Bifrost managed columns, appended to every Observation-policy table:

```
run_id, card_uid, sub_principal_id, act_principal_id, wyrd_request_id, wyrd_ingested_at
```

Bifrost system tables:

| Table | Change |
|---|---|
| gateway calls | drop `caller_principal_id`; attribution is the managed pair |
| verification results | drop `requested_by_principal_id` and `subject_card_uid`; managed `card_uid` = Card verified; add `verifier_card_uid` |
| eval result items, drift result features | drop `subject_card_uid`; managed `card_uid` = Card verified; add `verifier_card_uid` |
| audit log | `audit_principal_id` → `sub_principal_id`; `principal_kind` → `sub_principal_kind`; add `act_principal_id`; `delegation_chain` stays in detail |

PostgreSQL tables:

| Table | Change |
|---|---|
| `wyrd.verifier_runs` | `requested_by_principal_id` → pair, both `NOT NULL`; `subject_card_uid` → `card_uid`; `verifier_uid` → `verifier_card_uid`; idempotency unique on `(data_tenant_id, act_principal_id, idempotency_key)` |
| `wyrd.verification_bindings` | `subject_card_uid` → `card_uid`; `verifier_uid` → `verifier_card_uid` |
| `wyrd.card_registration_operations` | `principal_id` → pair; unique on `(data_tenant_id, act_principal_id, idempotency_key)` |
| `wyrd.cards`, `wyrd.auth_service_accounts`, `wyrd.auth_api_keys`, `wyrd.operator_connections` | `created_by` → pair |
| `vala.bifrost_tables` | `actor` → pair |

Idempotency is keyed on `act` because the actor is the party that retries.

### REQ-006 Wire contracts and SDKs

- Public request, response and row types use the new names everywhere they
  expose these columns. This covers the gateway record, verification result
  and run views, audit detail and Bifrost managed-column constants.
- Rust, Python and TypeScript SDK typed rows, read paths, stubs and generated
  schemas match.
- `sub_principal_id` and `act_principal_id` are reserved Bifrost column names.
  A client frame that declares either one is rejected, as reserved managed
  columns are today.
- No old name survives as an alias.

### REQ-007 SYSTEM always exists

- Every tenant has an active SYSTEM principal, so `act_principal_id` is never
  NULL for internal work.
- `wyrd-dev-fixtures` tenant seeding creates it, like production provisioning
  does.

## Invariants

- **INV-001** `sub_principal_id` and `act_principal_id` are server-derived from
  verified identity or from the binding owner, never from client input,
  headers, or row values.
- **INV-002** SYSTEM never appears as `sub`. SYSTEM appears as `act` only for
  work Wyrd started itself.
- **INV-003** Tenant isolation is unchanged. The binding owner's principal and
  SYSTEM resolve within the run's tenant.
- **INV-004** Origin references (`run_id`, `card_uid`, `owner_card_uid`,
  `binding_id`, `source_record_id`, trigger) keep their meaning and are never
  copied into principal columns.
- **INV-005** Trace context stays observability only. It never sets `sub` or
  `act`.

## Expensive-to-reverse decisions

- **No migrations.** Nothing has shipped, so each change edits the migration or
  Iceberg schema declaration that defines the column, in place:
  - `20261014000001_verifier_run_requester_every_origin.sql` is deleted;
  - any `ALTER` migration that exists only to adjust a renamed column is
    folded into the defining `CREATE TABLE`;
  - no backfill, compatibility view, or fingerprint migration path is written.
- **Managed-column set.** The managed-column set and every system-table schema
  change together. Bifrost table fingerprints change accordingly.

## Scope and non-goals

- **In scope:**
  - every table named above, and their writers, readers and wire types;
  - verification runtime authority, where judge calls run as the run's `sub`;
  - gateway attribution and spend;
  - Scribe stamping;
  - SDK projections;
  - generated contracts;
  - dev-fixture SYSTEM seeding;
  - `architecture/wyrd-design.md` and `architecture/bifrost-design.md`
    wherever they name the old columns or the old attribution rule.
- **Not in scope:**
  - changing token issuance or delegated exchange;
  - new permissions or roles;
  - trace propagation;
  - identity tables listed in REQ-005.

## Relationship to example-journey-fixes

This change branches from the example-journey-fixes integration candidate
(`c4ba2b050`). It supersedes that change's attribution rules:
- OD-1's "SYSTEM is the requester" clauses;
- the open judge-authority item;
- REQ-004's requester for every origin.

OD-1's span link and OD-3 trace behavior are kept.

## Acceptance criteria

| ID | Criterion | Evidence |
|---|---|---|
| AC-001 | A scheduled binding run, its judge calls (where Eval), and its result rows record `sub` = owner principal, `act` = SYSTEM. | Rust journey against `WyrdTestServer` and Postgres |
| AC-002 | An observation-triggered run records `sub` = binding owner principal, `act` = SYSTEM, even when another principal wrote the observation. Its judge gateway call is refused when the owner lacks `gateway:invoke` on the judge model, and its spend lands on the owner. | Rust journey |
| AC-003 | A direct gateway call and a direct verify record `sub` = `act` = caller in Bifrost, and the audit row matches. | journeys in Rust, Python and TypeScript |
| AC-004 | A delegated call (B on behalf of A) records `sub` = A and `act` = B on the gateway row and the audit row. | Rust journey |
| AC-005 | A client frame declaring `sub_principal_id` or `act_principal_id` is rejected with the existing reserved-column error. | journey in each SDK |
| AC-006 | No schema, wire type, SDK, generated artifact, or architecture doc names `caller_principal_id`, `requested_by_principal_id`, `subject_card_uid`, `audit_principal_id`, `verifier_uid`, `created_by`, or the managed `principal_id` for attribution. | repository search plus `mise run codegen:check` |
| AC-007 | No new migration exists. `20261014000001` is gone. Postgres applies from empty. | `mise run test:sql` |
| AC-008 | A tenant seeded by `wyrd-dev-fixtures` has an active SYSTEM principal. | integration test |
| AC-009 | Scoped lanes pass for every touched surface (`test:wyrd`, `test:vala`, `test:bifrost`, `test:gateway:gate`, Python and TypeScript unit and integration lanes, `codegen:check`, `check:tenant-isolation`). | lane output |

## Open decisions

None.

## Revision history

- r1 (2026-10-14): initial, approved.

## Authority

- `architecture/wyrd-design.md`:
  - SYSTEM principal (around line 173);
  - delegated token shape (lines 197-203 and 712-756).
- `architecture/bifrost-design.md`: managed columns and system tables.
- RFC 8693 §4.1 (`act` claim).
- `changes/active/example-journey-fixes/owner-decisions.md` OD-1 to OD-3.
