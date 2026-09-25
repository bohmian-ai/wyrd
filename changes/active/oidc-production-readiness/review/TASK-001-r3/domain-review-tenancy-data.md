# Wave 1 Domain Review — Tenancy, SQL, and Persistent Data

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `eb4142cc95fa24b0525c71f66bfc978577cd4b7c`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Prior remediation: `TASK-001-R1-production-readiness-gaps.md` and `TASK-001-R2-remaining-production-readiness-gaps.md`

The candidate remained at the named commit throughout this review.

## Boundary and authority coverage

| Boundary | Governing obligation | Source and proof inspected | Result |
|---|---|---|---|
| Tenant connection state machine | REQ-002/003/014/016; task packet-local lifecycle contract | `wyrd-auth/src/connections.rs`; `wyrd-sql/src/queries/auth/human_connections.rs`; partial unique indexes and advisory transaction lock in `20260925000000_auth_human_connections.sql`; admin and rotation journeys | PASS |
| Tenant isolation and SQL ownership | AGENTS.md sections 2/3/9; `architecture/agent-rules.md`; tenancy and SQL foundations | `HumanConnections` owns `WyrdPostgres`; tenant paths acquire `TenantConn`; RLS is enabled and forced; cross-tenant rewrap alone uses `OperatorPool` with exact-row CAS | PASS |
| Migration/preflight | Original task migration contract; SQL foundation migration rules | Complete migration and `pg_tests::human_connection_upgrade_preflight`, including multiple-Human/default-role refusal, workload binding preservation, and atomic migration behavior | PASS |
| Refresh provenance and replacement cutoff | REQ-016; R1/R2 `FIND-TASK-001-5` | Login state and refresh rows carry connection id/revision; `issue_human_session` takes the shared slot lock and rechecks exact Active binding; R2 leaves legacy families unbound; migration and refresh tests cover refusal and fresh binding | PASS |
| Callback tenant derivation | REQ-006, INV-001, AC-003; packet requirement for wrong-tenant callback refusal | Login state creation/consume, callback adapter, state table key, and identity journey callback helper | FAIL (`TD-R3-001`) |
| Mutation/audit transaction coupling | REQ-017; AGENTS.md current decisions; `architecture/agent-rules.md` authorization-audit rules | Admin handler decision, `HumanConnections::begin_locked`, activation recovery-key verification, canonical audit append, audit-failure journey | FAIL (`TD-R3-002`) |

## Material findings

### TD-R3-001 — INCORRECT: the callback still selects the effective tenant from `Host`

- **Violated obligation:** REQ-006 requires the common callback to derive tenant and connection solely from server-owned, single-use login state; INV-001 expressly prohibits a host from selecting effective tenant identity after login starts. AC-003 and TASK-001 require wrong-tenant callback refusal in the multi-tenant journey.
- **Exact location:** `crates/wyrd/wyrd-server/src/auth/callback.rs:18-35,48-70,75-91`; `crates/wyrd/wyrd-server/src/auth/mod.rs:23-35`; state creation/consume at `crates/wyrd/wyrd-auth/src/login.rs:141-169` and `crates/wyrd/wyrd-auth/src/callback.rs:85-111`; durable key shape at `crates/wyrd/wyrd-sql/migrations/20260601000010_auth_federated_identity.sql:28-37`.
- **Evidence and reachability:** `callback` passes request headers into `exchange_authorization_code`; `resolve_callback_tenant` parses the leftmost `Host` label and resolves that slug before the opaque state is read. `AuthorizationCodeExchange::execute` then accepts that externally selected tenant and consumes `(tenant_id, state)` under RLS. The login state itself is a random string and carries no independently authenticated tenant routing value. Every production `/auth/callback` request reaches this path.
- **Observable consequence:** The callback is not a common state-routed callback: changing `Host` changes the tenant transaction searched after login began. A legitimate flow succeeds only when the callback host happens to encode the initiating tenant, and the implementation cannot support two tenants through one configured public callback origin as specified. The committed journey hides this by hard-coding `test-tenant-1.wyrd.test` in `finish_callback` (`crates/wyrd/wyrd-server/tests/identity_e2e.rs:1072-1089`) and authenticating only tenant A in its two-tenant connection journey (`:1602-1628`); it does not exercise tenant B or a host mismatch.
- **Required testable correction:** Make the callback recover and authenticate the tenant from the server-created one-time state before acquiring `TenantConn`; request host/path/header values must not participate in effective tenant or connection selection. Keep state single-use, bounded, opaque to the browser, and keep the exact connection id/revision check. Add a real-server two-tenant proof using the same configured callback origin: both tenants complete their own state-bound callback, changing the callback `Host` cannot redirect either state to another tenant, and tenant A state/code cannot establish tenant B authority.

### TD-R3-002 — VIOLATION: activation evaluates the recovery principal's permission without auditing that decision

- **Violated obligation:** `architecture/agent-rules.md` requires one audit row for every principal permission evaluation, allowed and denied, naming the principal, permission, resource, and outcome; non-exempt authorization decisions must be recorded in the transaction that acts or refuses. REQ-017 requires canonical evidence for security-significant connection mutations and fail-closed required audit.
- **Exact location:** caller decision at `crates/wyrd/wyrd-server/src/components/admin/identity.rs:303-310`; activation transaction at `crates/wyrd/wyrd-auth/src/connections.rs:451-497`; recovery decision at `crates/wyrd/wyrd-auth/src/connections.rs:918-951`; existing journey assertion at `crates/wyrd/wyrd-server/tests/identity_e2e.rs:1761-1796`.
- **Evidence and reachability:** The handler audits the bearer caller's `identity_connections:write` decision. Activation then verifies a second credential, resolves that recovery principal's current roles and permissions, and calls `permissions.contains(identity_connections_write())`. No audit event is built or appended for this second principal/credential and outcome; the sole activation event remains attributed to the bearer caller. Every activation reaches this recovery permission decision, including the reachable valid-key-but-insufficient-permission refusal.
- **Observable consequence:** Retained audit cannot establish which recovery principal and credential authorized an activation, or record that a valid recovery principal was denied. A connection can therefore be activated after two permission evaluations while durable evidence contains only one, contrary to the repository's exact-decision audit invariant.
- **Required testable correction:** Record the recovery principal's allowed or denied `identity_connections:write` decision, with its credential attribution, in the same locked tenant transaction as the activation or refusal. Preserve the existing bearer-caller decision and secret redaction; invalid credentials that resolve no principal must remain indistinguishable and must not leak lookup facts. Extend the activation journey to use a recovery credential distinct from the bearer and assert both attributed decisions; add the valid-key-without-permission case and an injected failure on the recovery-decision append, proving no retirement or promotion commits.

## Prior-finding closure in this domain

| Prior finding | Result | Evidence |
|---|---|---|
| `FIND-TASK-001-5` | CLOSED | The migration no longer assigns a current provider to provenance-free legacy families (`20260925000000_auth_human_connections.sql:188-205`). Runtime refresh refuses an unbound family and exact id/revision remains rechecked under the shared lifecycle lock. `pg_tests::human_connection_upgrade_preflight` covers replaced A→B legacy state and a fresh B binding. |
| `FIND-TASK-001-6` | CLOSED | `HumanConnections` and `PgLoginStateStore` acquire tenant transactions through owned `WyrdPostgres`; no raw pool remains in these owners. |
| `FIND-TASK-001-7` | CLOSED | Candidate provider IO is outside a transaction; a second permission decision is evaluated after the probe and its audit commits with the exact revision stamp under the slot lock. |
| `FIND-TASK-001-14` | CLOSED | `PgIssuerResolver` now owns `WyrdPostgres` and uses its tenant acquisition boundary. |

## Verification limits

- Static review only, as assigned. No Cargo, `mise`, Postgres, provider, or journey lane was run.
- Committed test source and recorded prior evidence were inspected, but execution claims were not independently reproduced.
- The complete cumulative base-to-candidate diff is large; this report is intentionally limited to tenancy, RLS/SQL, durable state, transaction/concurrency, refresh provenance, and their audit coupling.
- No CodeGraph index exists at the repository root, so source and caller tracing used repository search and direct file inspection.

## Overall result

**FAIL** — the SQL/RLS lifecycle, slot-lock concurrency, migration preflight, and R2 refresh-provenance correction are sound, but the production callback still takes tenant authority from `Host`, and activation leaves its recovery-principal permission decision unaudited. Both are reachable violations of approved tenancy/audit obligations.
