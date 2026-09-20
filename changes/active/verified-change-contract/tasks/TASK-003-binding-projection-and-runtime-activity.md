---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-verified-change-contract
spec_revision: 32
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
existing auth service-account state owns `last_authenticated_at`; `wyrd-server`
orchestrates registration/auth/status. Vala consumers use the `vala-sql`
re-export. Schedulers and post-commit Eval enqueue are downstream consumers.

Do not create a Vala control schema, activity table, heartbeat, activation
endpoint, idle refresh timer, per-request touch, manual tenant predicates, or a
second binding identity. A callee taking `TenantConn` never commits.

## Approach

1. Add forced-RLS `wyrd` control migrations and typed SQL operations needed for
   binding projection and activity reads, preserving transaction composition.
2. Project each effective binding during composite registration under its
   UUIDv7 natural-key contract and freeze effective identities/digests.
3. Update exact-principal token exchange/refresh to record server time and
   initialize only a null schedule cursor.
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

**Behavior.** Successful initial token exchange records server time for the
exact enabled Service/Agent principal and sets a null schedule cursor to the
next future boundary; refresh renews activity without resetting an existing
cursor. User auth, cached-token requests, component scope, observation writes,
and another Card version do not touch activity.

**RED.** Add auth/SQL integration cases for each accepted and rejected touch.
They fail because `last_authenticated_at` and cursor initialization are not
part of issuance.

**GREEN.** Extend the existing issuance/refresh transaction with the minimal
principal-state and null-cursor updates.

**REFACTOR.** Reuse current Card-bound principal lookup and clock injection;
do not add authentication observers.

### Scenario 3 — Activity gates new work without changing lifecycle

**Behavior.** Eligibility requires enabled principal plus recent successful
exchange under the default/configured timeout. Inactivity, suspension, or
deletion prevents new work immediately; reauthentication permits only future
occurrences. Already admitted work is not cancelled. A/B versions are
independent and replicas of one principal share state.

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
- Activity changes only on successful exact-principal exchange/refresh.
- No heartbeat, activity table, backfill, or Card lifecycle mutation appears.
- `AC-018`, `AC-019`, binding portions of `AC-028`, and relevant auth/audit
  portions of `AC-030` pass.

## Expected Write Set and Consumer Closure

Likely owners are `wyrd-sql` migrations/queries/re-exports, registry service
transaction code, auth token exchange/refresh and service-account rows,
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
protocol, changed token refresh semantics, or weakened RLS/audit behavior.

## Authority Links

- `changes/active/verified-change-contract/spec.md`
- `changes/active/verified-change-contract/architecture/verification-control-flow.html`
- `architecture/wyrd-security-posture.md`
- `architecture/agent-rules.md`
- `AGENTS.md`
