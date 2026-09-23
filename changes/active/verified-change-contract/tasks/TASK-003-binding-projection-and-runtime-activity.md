---
id: TASK-003
kind: implementation
status: approved
spec: SPEC-verified-change-contract
spec_revision: 33
requirements: [REQ-078, REQ-095, REQ-102, REQ-104, REQ-105, REQ-106, REQ-107, REQ-108, REQ-112, REQ-134, REQ-145, INV-001, INV-007, INV-010, AC-018, AC-019, AC-020, AC-028, AC-030]
depends_on: [TASK-001]
---

## Outcome and Value

Every registered owner Card has an atomic, tenant-isolated Postgres projection
of its exact verification bindings. A binding receives one stable UUIDv7 ID,
becomes eligible only through the exact existing Card-bound principal's real
authentication lifecycle, and exposes enough derived status for schedulers,
Eval routing, and users without a heartbeat or binding resource.

## Owners, Scope, Consumers, and Prohibited Changes

`wyrd-sql` owns all five `wyrd`-schema verification control tables and their
typed `TenantConn` operations. The registry transaction owns binding projection;
existing tenant machine-principal state owns `last_authenticated_at`;
`wyrd-auth` and the shared tenant issuer own qualifying exchange-time updates;
`wyrd-server` orchestrates registration/auth/status. Vala consumers use the
`vala-sql` re-export. Schedulers and post-commit Eval enqueue are downstream
consumers.

Do not create a Vala control schema, activity table, heartbeat, activation
endpoint, idle refresh timer, per-request touch, manual tenant predicates, or a
second binding identity. A callee taking `TenantConn` never commits.

## Approach

1. Add forced-RLS `wyrd` control migrations and typed SQL operations needed for
   binding projection and activity reads, preserving transaction composition.
2. Project each effective binding during composite registration under its
   UUIDv7 natural-key contract and freeze effective identities/digests.
3. Update successful API-key and workload-`jwt-bearer` issuance for an exact
   Card-bound Service or Agent to record server time and initialize only a null
   schedule cursor.
4. Derive active/readiness/status views from principal state and binding rows.
5. Prove version, component, replica, inactivity, and reauthentication cases
   through Postgres and real-client seams.

## Ordered Implementation Scenarios

### Scenario 1 — Registration and binding projection are atomic

**Behavior.** Card, Card-bound principal, binding rows, and required dependent
control rows commit together. A failure at any validation/write point leaves
none. Re-applying the same exact owner/occurrence/Verifier natural key preserves
the UUIDv7 `BindingId`; reordering does not change it, while a changed owner
version, alias, or Verifier version produces a new ID.

**RED.** Add Postgres registration cases for success, injected rollback,
reapply, reorder, and changed natural-key members. Current registration has no
binding projection.

**GREEN.** Add forced-RLS migrations/constraints and compose `wyrd-sql`
operations in the registry's caller-owned transaction.

**REFACTOR.** Keep natural-key/UUID logic in the SQL owner and delete any
parallel derivation in registry or status code.

### Scenario 2 — Only exact machine authentication activates an owner

**Behavior.** Successful API-key exchange or workload `jwt-bearer` issuance
records server time for the exact enabled Card-bound Service/Agent principal
and sets a null schedule cursor to the next future boundary. A later qualifying
exchange renews activity without resetting an existing cursor. Delegation,
OIDC login, human refresh, Card-free automation, SYSTEM issuance, cached-token
requests, component scope, observation writes, and another Card version do not
touch activity.

**RED.** Add auth/SQL integration cases for both qualifying grants and every
excluded grant/use path. They fail because `last_authenticated_at` and cursor
initialization are not part of qualifying issuance.

**GREEN.** Extend the existing shared tenant-issuance transaction with the
minimal principal-state and null-cursor updates, gated by the verified grant
and a bound owner Card.

**REFACTOR.** Reuse the current grant enum, Card-bound principal lookup, and
clock injection; do not add authentication observers or a machine refresh path.

### Scenario 3 — Activity gates new work without changing lifecycle

**Behavior.** Eligibility requires enabled principal plus recent successful
exchange under the default/configured timeout. Inactivity, suspension, or
deletion prevents new work immediately; reauthentication permits only future
occurrences. Already admitted work is not cancelled. A/B versions are
independent and replicas of one principal share state. An already-issued
five-minute permission snapshot may remain valid for ordinary authorization,
but it does not bypass the binding admission query's current principal status.

**RED.** Add deterministic-clock binding eligibility tests including component
inheritance, standalone Agent, A/B versions, and shared replicas.

**GREEN.** Derive eligibility from existing principal state and timestamp at
each admission query.

**REFACTOR.** Keep activity a query predicate/value object, not mutable runtime
state.

### Scenario 4 — Owner Card status exposes stable binding IDs

**Behavior.** Existing Card GET returns stable `card.status.verification.binding_ids`
for Service/Agent owners without mutating authored spec or creating a listing
resource. Cross-tenant and under-privileged reads fail and audit normally.

**RED.** Add Card GET journeys for owner, component, reapply, authorization,
and tenant isolation. Current status has no binding projection.

**GREEN.** Derive status from the tenant binding store through the existing
Card read path.

**REFACTOR.** Reuse current server-managed status composition rather than
persisting duplicate status JSON.

### Scenario 5 — SQL boundaries remain composable and tenant-isolated

**Behavior.** All verification control rows use `data_tenant_id`, forced RLS,
`wyrd.current_tenant()`, and `TenantConn`; Vala imports the re-export. Tenant A
cannot observe or mutate tenant B, and no runtime query accepts a raw pool.

**RED.** Extend migration, tenant-isolation, and transaction-coupling tests to
the new tables and queries.

**GREEN.** Use the approved `wyrd-sql` ownership and existing connection types.

**REFACTOR.** Remove redundant tenant predicates and any Vala-side repository.

## Acceptance Criteria

- Composite registration and principal creation remain one transaction.
- Stable typed UUIDv7 binding IDs obey the exact approved natural key.
- Activity changes only on successful API-key or workload-`jwt-bearer`
  issuance for the exact Card-bound owner.
- Delegation, human refresh, Card-free automation, SYSTEM issuance, cached
  bearer use, and idle expiry never activate or renew a binding owner.
- No heartbeat, activity table, backfill, or Card lifecycle mutation appears.
- `AC-018`, `AC-019`, binding portions of `AC-028`, and relevant auth/audit
  portions of `AC-030` pass.

## Expected Write Set and Consumer Closure

Likely owners are `wyrd-sql` migrations/queries/re-exports, registry service
transaction code, shared tenant issuance and tenant machine-principal rows,
server Card-status composition, `wyrd-spec` status/ID contracts, and real
registration/auth tests. Scheduler/Eval tasks consume these operations but do
not own a second store.

## Verification and Evidence

```bash
mise run test:cards:integration
mise run test:principals:unit
mise run test:principals:integration
mise run test:sql
mise run test:wyrd
mise run test:e2e
mise run check:tenant-isolation
mise run check:registry-tx-coupling
mise run check:from-pools-allowlist
mise run codegen:check
mise run fmt
mise run lints
git diff --check
```

Every new specifically named test must be recorded and run with its exact
repository-native selector after its final name exists.

## Material Stop Conditions

Stop if implementation requires a different BindingId contract, control tables
outside `wyrd-sql`, non-transactional registration projection, a new activity
protocol, a machine refresh-token path, activation from a non-qualifying grant,
or weakened RLS/audit behavior.

## Authority Links

- `changes/active/verified-change-contract/spec.md`
- `changes/active/verified-change-contract/architecture/verification-control-flow.html`
- `architecture/wyrd-security-posture.md`
- `architecture/agent-rules.md`
- `AGENTS.md`

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Composite registration and principal creation remain one transaction | `persist_node` projects bindings after `upsert_service_account_from_card` on the caller's `TenantConn` (`wyrd-server/src/components/cards/service.rs`); `project_bindings` never commits (`wyrd-sql/src/queries/verification.rs`); schedule parse and alias uniqueness are refused before writes (`cards/resolve.rs`, `wyrd-spec/src/graph/composition.rs`) | `pg_verification_bindings::projection_is_transactional_and_tenant_isolated`; `pg_card_registration_route::owner_status_serves_stable_binding_ids_and_exchange_activates` (bad cron and duplicate alias leave no writes); `graph::composition::tests::rejects_duplicate_component_alias`; `mise run test:cards:integration` | PASS |
| Stable typed UUIDv7 binding IDs obey the exact approved natural key | `BindingId` (`wyrd-spec/src/ids.rs`); migration `20260601000027_verification_bindings.sql` `UNIQUE NULLS NOT DISTINCT (data_tenant_id, owner_card_uid, subject_occurrence_key, verifier_uid)`, forced RLS; referenced Trigger/Operator frozen by UID, inline by canonical digest | `foundation_contracts_tests::binding_id_is_uuid7_backed`; `projection_identity_is_stable_under_reapply_and_reorder`; server journey (re-apply keeps IDs, Trigger UID and Operator digest frozen) | PASS |
| Activity changes only on successful API-key or workload `jwt-bearer` issuance for the exact Card-bound owner | `TenantGrant::records_owner_activity` + `record_machine_authentication` inside `TenantTokenIssuer::issue` (`wyrd-auth/src/issuance.rs`); arms only null schedule cursors | `issuance::pg_tests::qualifying_machine_grants_activate_the_bound_owner`; `first_exchange_arms_schedule_and_renewal_keeps_cursor`; server journey (real `/auth/token` API-key exchange stamps activity, arms schedule only) | PASS |
| Delegation, human refresh, Card-free automation, SYSTEM issuance, cached bearer use, and idle expiry never activate or renew | Grant gate excludes Delegation/OidcLogin/Refresh; SQL update requires an active Card-bound service/agent row; no request-path or expiry writer exists | `issuance::pg_tests::non_qualifying_grants_never_touch_activity`; `excluded_principals_record_nothing`; `activity_is_per_version_and_revoked_immediately` (window expiry) | PASS (SYSTEM: see limits) |
| No heartbeat, activity table, backfill, or Card lifecycle mutation | Activity is `auth_service_accounts.last_authenticated_at`; `binding_activity` derives eligibility per query; renewal never moves an armed cursor | `first_exchange_arms_schedule_and_renewal_keeps_cursor`; diff audit | PASS |
| AC-018 binding portion, AC-019, AC-028 binding-ID portion, AC-030 auth/audit portion | `card.status.verification.binding_ids` via `hydrate_card`/`owner_binding_ids`; A/B versions are distinct principals (dropped tenant-wide name uniqueness for Card-bound rows); component bindings inherit Service activity | server journey (403 under-privileged, 404 cross-tenant, registration alone does not activate); `activity_is_per_version_and_revoked_immediately`; `mise run test:principals:integration`, `test:wyrd`, `test:cli:journey`, `test:wyrdstate:journey`, `test:platform:journey` | PASS |

Commands run (all pass): `mise run test:cards:integration`, `test:principals:unit`,
`test:principals:integration`, `test:sql`, `test:wyrd`, `test:cli:journey`,
`test:wyrdstate:journey`, `test:platform:journey`, `check:tenant-isolation`,
`check:registry-tx-coupling`, `check:from-pools-allowlist`, `codegen:check`
(schemas regenerated through `codegen:regen`), `fmt`, `lints`, `cargo hakari
verify`, `git diff --check`, and every named test above via its exact
`mise exec -- cargo nextest run --locked -p <crate> ... -E 'test(=...)'` selector.

Limits and deviations:

- `mise run test:e2e` does not exist; the three real-server journey lanes above
  replace it.
- SYSTEM issuance does not exist yet (TASK-004). This task adds no path by
  which it could record activity.
- Readiness (baseline fitting) belongs to TASK-005. The inactivity timeout is a
  value object with the 86 400 s default. Wiring it to the environment waits
  for its first consumer (the TASK-004 scheduler).
- There is no `vala-sql` re-export because no Vala consumer exists yet, and
  the existing `vala_sql_does_not_call_wyrd_query_modules` guard forbids an
  unused one. The first Vala consumer adds it.
- Only `wyrd.verification_bindings` ships here. The remaining control tables
  belong to the tasks that consume them.
- A stale assertion was fixed: `pg_openapi_contract` expected 16 `Spec`
  alternatives, while the doctrine and the enum have 15.

Non-goals stayed excluded. There is no Vala control schema, activity table,
heartbeat, activation endpoint, idle refresh, per-request touch, manual tenant
predicate, or second binding identity, and no unrelated files changed.

## Human Closeout

TASK-003 and remediation tasks `TASK-003-R1` through `TASK-003-R3` are closed
as approved at candidate `133f3f47fd57dc41d126e749f387b0078477d87d`.
The human owner accepted the implemented `FIND-TASK-003-14` correction on
2026-09-22 and explicitly directed that no further task-review cycle be run.
The immutable R3 verdict remains preserved as the review of its older
`a5a5b60f446981760ac831f64aba871582ec45e4` candidate.
