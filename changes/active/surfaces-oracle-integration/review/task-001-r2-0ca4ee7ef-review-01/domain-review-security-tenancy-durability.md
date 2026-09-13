# TASK-001-R2 security, tenancy, and durability review

## Reviewed boundary

- Base: `8e61c03493f5c0ebcc12cdb4a9998df284f7fc8f`
- Candidate: `0ca4ee7ef7416f870ce3244add406901b57337de` (verified `HEAD`)
- Domain: system-owner admission for retained audit publication, publication
  freeze/settle, service-account verdict auditing, Card and Bifrost
  registration verdict coupling, tenant isolation
- Result: **FAIL** (one material finding, DR-1)

Traced end to end: peer/tail rejection → `SYSTEM_OWNER` staging →
`AuditPublisher::publish_tenant` (freeze, read, project, Scribe, settle) →
`validate_logical_transport_frame` → `TenantTableBinding::facts` → retained
`vala.system.audit_log`. Also traced every Gate path that builds a
`ScribeIngressFrame`, the admin/revoke/issuance handlers through
`authorize_service_accounts_write`, Card register/delete through
`write_registration`/`delete_card_*`, and Bifrost `register_table` through
`create_table_locked`.

## Authority and source coverage

| Boundary | Authority | Source | Result |
|---|---|---|---|
| Nil-tenant admission is internal-only | AGENTS.md §2, REQ-026B, REQ-027 | `scribe/ingress.rs:54-63`, `tenant_table.rs:118-123`, `audit_log.rs` `admits_system_owner`, `gate/mod.rs:879-881` (native path refuses Audit namespace), `gate/mod.rs:668/714/756` (OTLP paths use fixed traces/metrics/logs tables), `publication.rs` `publisher_principal` | PASS |
| Cross-tenant write to system audit log | INV-008D, tenant isolation | Binding keys on the authenticated tenant; `register_dataset` only accepts `Datasets`, so nil is refused for caller tables; Gate cannot address `audit.*` | PASS |
| Freeze/settle/replay | REQ-027C, REQ-028, REQ-029, INV-008C/D | `publication.rs` freeze commits before Scribe IO; settle is a separate transaction; batch id derives from frozen range; unchanged by R2 except test deletion | PASS |
| Configured RBAC owner | R1-9, REQ-026 | `audit/mod.rs` `authorize_service_accounts_write` → `authorize_recording_denial` → `state.authz.permission_check`; only `PermissionDeniedRbac` text is rewritten | PASS |
| Issuer verdict before discovery | R1-3, REQ-026 | `admin/routes.rs` create: local `request_client_auth` → verdict → standalone `record_audit` → discovery → insert without a second append | PASS |
| Card write coupling | R1-4, REQ-026 | `append_on` first in `write_registration` and both delete transactions; `register_card` records standalone on replay, error, or lost idempotency race; `record_unless_committed` for delete errors; route records on missing idempotency key | PASS |
| Bifrost verdict exactly once | R1-2, REQ-026 | `bifrost_catalog.rs:995,1018` append before commit on both committing exits; `service.rs` records standalone on every non-`AuditUnavailable` catalog error; every pre-catalog branch records | PASS |
| Other `service_accounts:write` handlers | REQ-026 "exactly one ... audit event" | `admin/routes.rs` binding create/delete, issuer delete; `auth/revoke.rs`; `components/auth/routes.rs` issuance | **FAIL (DR-1)** |

## Material findings

### DR-1 — Service-account writes lose their Allowed verdict on reachable post-append failures

- Classification: `MISSING` against REQ-026. Also `DRIFT`: R2 added a module
  claim the code does not meet.
- Violated obligation: REQ-026 says every permission decision appends exactly
  one canonical audit event. R2 applies this same rule to Card no-write outcomes
  (R1-4) and Bifrost pre-commit failures (R1-2). In the same file, R2 adds the
  claim "Every handler audits its `service_accounts:write` verdict exactly once"
  (`crates/wyrd/wyrd-server/src/components/admin/routes.rs:15`).
- Evidence: each handler below appends the Allowed event with `append_on` on
  its tenant transaction. It then returns through `?` on a SQL or domain error,
  so the transaction drops and the verdict rolls back with it. Nothing records
  that verdict standalone. The not-found delete branches already commit before
  refusing, but the conflict branches do not.
  - `admin/routes.rs:403-408` `create_workload_binding`: a duplicate binding
    returns `409 AdminConflict`, and an unregistered issuer returns
    `404 AdminNotFound` through `map_binding_write_error`.
  - `admin/routes.rs:336-351` `delete_trusted_issuer_route` without `cascade`:
    a live binding trips the FK `RESTRICT` and returns `409`.
  - `admin/routes.rs:473-478` `delete_workload_binding_route`: any
    `map_write_error` failure.
  - `auth/revoke.rs:47-52`: `revoke_principal_in_conn` returns
    `PrincipalNotFound` for an unknown target (`wyrd-auth/src/revoke.rs:45`).
  - `components/auth/routes.rs:294-301` API-key issuance: any error from
    `IssueApiKey::execute`.
  - The existing tests `delete_issuer_with_live_binding_conflicts_then_cascades`
    and `create_binding_for_unknown_issuer_is_not_found` reach these exits.
    They assert only the error variant, never the audit rows.
- Consequence: a principal holding `service_accounts:write` can probe
  credential-administration state without any durable security record. Repeated
  duplicate or unknown-issuer binding creates, blocked issuer deletes, and
  revocations of nonexistent principal ids all return a permitted decision that
  never reaches staging or retained history. This is the privileged admin
  surface, and it is the exact gap class R2 closed for Cards and Bifrost.
  Origin: this predates R2; neither the R1 review nor the R1-3 validation raised
  it. It is still reachable in the cumulative candidate, and the new R2 doc
  claim hides it.
- Testable correction: reuse the R2 Card pattern (`record_unless_committed`
  semantics). When the handler transaction does not commit, record the same
  Allowed event once through `audit::record_audit` before returning the mapped
  error. Keep `AuditUnavailable` fail-closed and add no second writer. Extend the
  two existing admin tests plus one revoke-not-found case to assert exactly one
  `allowed` staging row for the operation and no effect.

## Passed high-risk properties (no finding)

- **Privilege escalation to the system audit log:** not reachable. Nil
  admission requires both `principal.id == PLATFORM_AUDIT_PRINCIPAL` and the
  exact `audit.audit_log` table. The only production frame with that principal
  is built in `publication.rs`. Every Gate native frame refuses the Audit
  namespace, and the OTLP frames carry fixed non-audit tables. So even a verified
  system-owner token (`gate/auth.rs:85` maps nil to `SYSTEM_OWNER`) cannot write
  `vala.system.audit_log`.
- **Nil relaxation:** stays narrow. `project_audit_rows` no longer rejects nil,
  but its only caller is the publisher, which projects rows read under the same
  tenant's RLS connection.
- **Frozen bound and dedup fence:** unchanged. The system tenant uses the same
  per-tenant bound, deterministic batch id, and settle transaction as any other
  tenant.
- **Transactions and external IO:** issuer discovery runs with no open
  transaction. The Card registration transaction now holds the tenant's
  `audit_chain_head` row lock for its full SQL duration. There is no external IO
  inside it, lock order is chain → card rows on every audited Card path, and a
  Postgres deadlock abort would still record once through the error branch.
  That is contention, not a durability or security defect.
- **Double-record edges:** reviewed and rejected as speculative. An ambiguous
  commit acknowledgement, or `TableUid::from_row` failing on a corrupt row after
  commit (`bifrost_catalog.rs:997`), could double-record. Neither is a reachable
  normal path.
- **R1-5 "ceiling of 8" deviation:** no security or resource consequence found.
  Each tenant cycle holds at most one Vala connection at a time, and never across
  Scribe IO (freeze, read, and settle are separate short transactions). Eight
  concurrent cycles fit within the default Vala pool of 16
  (`vala-sql/src/postgres.rs:154`). Request-path audit appends keep headroom, so
  fail-closed `AuditUnavailable` starvation is not produced by the bound. The
  deviation is a proof gap, not a behavior risk: the constant is not exercised
  above eight tenants end to end.

## Verification limits

- The review was static. I ran no Postgres-backed tests: another reviewer was
  running focused set #3 (Bifrost and admin) at the time, and those tests must
  run serially. The implementation evidence for focused sets 1-5 is taken as
  claimed, not reproduced.
- DR-1 was confirmed by reading the code and the existing tests. I did not
  execute a failing assertion for it, because this review may not add tests.
- `bifrost_tables_concurrent_same_fqn_register_records_each_verdict` uses
  `tokio::join!` and does not force both requests past `describe_table`. A pass
  does not prove the row-exists branch ran, although static reading shows that
  branch is correct.
