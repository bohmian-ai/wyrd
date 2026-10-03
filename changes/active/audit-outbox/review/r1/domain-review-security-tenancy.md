# Security and tenancy domain review

## Result

**FAIL**

The candidate preserves blocking permission enforcement, verified-principal
tenant binding, RLS-backed tenant writes, and the system-sentinel boundary for
unverified peer traffic. It does not, however, satisfy two audit-integrity
obligations: transient audit-store failures are treated as final loss instead
of retryable dependency slowness, and six tenant-administration handlers can
evaluate an allowance but return or perform outbound discovery before staging
that decision.

## Reviewed boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`
- Candidate: `f451d52be01acba66bae003b8509c95086fc8558`
- Range: base through candidate only; candidate identity was rechecked after
  source inspection and remained unchanged.
- Approved change authority: `changes/active/audit-outbox/spec.md`, revision 1,
  plus tasks `01-publication-progress.md`, `02-one-outbox.md`, and
  `03-remove-audit-unavailable.md`.
- Additional user-approved authority: high-throughput acknowledgements mean
  receipt rather than guaranteed durability; audit is eventually consistent
  within seconds; the server-owned outbox retries dependency slowness, drains
  on graceful shutdown, and may lose counted unflushed work on hard process
  termination. Those accepted windows were not treated as defects.

## Authority and source coverage

| Boundary | Authority inspected | Candidate source and consumers inspected | Result |
|---|---|---|---|
| Permission ordering and allow/deny audit | `AGENTS.md` §2, §9; `architecture/agent-rules.md`; spec REQ-001, REQ-003, REQ-005, INV-001 | `wyrd-server/src/audit/mod.rs`; Cards, principals, admin, platform, operators, storage, verification, query, gateway, and auth routes/services | FAIL: `SEC-TEN-002` |
| Audit outbox failure handling | User-approved retry principle; spec objective, REQ-002, REQ-003, REQ-007 | `vala-sql/src/audit_outbox.rs`; boot/state sharing; shutdown/settle; `pg_audit_outbox.rs` | FAIL: `SEC-TEN-001` |
| Tenant binding and RLS | `architecture/wyrd-security-posture.md` security principles and tenant isolation; `architecture/agent-rules.md` `TenantConn` rules; spec INV-003 | `AuditOutbox::stage` callers; `commit_tenant`; `append_audit_events`; audit-publication migration; platform sentinel staging | PASS |
| Identity, credential, request, and delegation attribution | `architecture/wyrd-design.md` runtime identity/request correlator/audit; `architecture/wyrd-security-posture.md` principal lifecycle and audit integrity | server `audit_event`; auth audit builders; Gate/Oracle audit projection; peer audit; platform authorization/session audit | PASS within the task's no-event-content-change boundary |
| Platform/admin trust boundary | `architecture/wyrd-security-posture.md` two administration planes and authorization; spec REQ-005 | `wyrd-auth/platform_authz.rs`, `platform_sessions.rs`; server platform identity/provisioning/recovery/routes; tenant admin routes | FAIL: `SEC-TEN-002` is limited to tenant admin routes; platform-plane authorization stages immediately |
| Gateway, Gate, Oracle, and peer boundaries | `architecture/wyrd-security-posture.md` peer identity; `architecture/bifrost-design.md` Gate/Oracle read audit and system sentinel | Gate authorization/audit, Oracle query audit, peer authority/audit/service, gateway invocation/administration | PASS |
| Removal of audit-unavailable responses | spec REQ-003, REQ-004, AC-006; security posture rule that audit failure does not deny the operation | Wyrd/Bifrost error catalogs, Gate mappings, HTTP/platform mappings, proto reservation, SDK mappings | PASS: permission-denied/authentication failures remain typed and blocking |

## Verification limits

- This was a read-only source audit of the immutable range. No test or
  production source was modified and no verification lane was rerun.
- The task packet records green broad and focused lanes, including
  `test:wyrd`, `test:principals:integration`, Bifrost SQL/redux/server journeys,
  codegen, docs, format, and lints. Those results demonstrate compilation and
  the tested success/failure cases, but the current outbox tests exercise
  successful commits, lock waiting, concurrency, and shutdown; they do not
  prove retry after a transient failed acquire/append/commit.
- Admin audit-failure tests prove that a staged event may fail without refusing
  the operation. They do not force an error between the permission decision
  and the later `stage` call, so they do not cover `SEC-TEN-002`.
- No secrets, token material, SQL interpolation, path traversal, unsafe
  deserialization, CORS, redirect, or new third-party supply-chain issue was
  found in the reviewed range. The lockfile changes only add existing workspace
  dependencies to `vala-sql`.

## Material findings

### SEC-TEN-001 — INCORRECT — transient dependency failures permanently discard audit batches

- Violated obligation: the user-approved system principle requires the
  server-owned audit outbox to retry rather than drop work on dependency
  slowness. Graceful shutdown must drain queued work; only the explicitly
  accepted unflushed hard-kill window may lose it.
- Location: `crates/vala/vala-sql/src/audit_outbox.rs:271-308`.
- Evidence: `commit_tenant` converts any connection-acquire, append, or commit
  error into one logged/counted loss per event and returns `()`. `settle` then
  removes the in-flight batch and decrements `pending` exactly as it does for a
  successful commit. The batch is never returned to `waiting`, and the module
  contains no retry or backoff path.
- Reachable scenario: a short Postgres connection-pool timeout, transaction
  abort, failover, or other transient SQL fault occurs while a tenant batch is
  committing. The audited request has already proceeded, and the writer
  immediately settles every decision in that batch as lost even though the
  process remains healthy and queue capacity is available. Load-induced
  dependency slowness can therefore erase the authorization evidence for a
  burst of otherwise successful sensitive operations.
- Observable consequence: retained audit history permanently omits those
  authorization decisions; `settle` and graceful shutdown can report zero
  pending even though the batch never reached `vala.audit_staging`. This is
  outside the accepted eventual-consistency and hard-kill windows.
- Required testable correction: keep a failed transient batch owned by its
  tenant queue and retry it with bounded backoff while preserving one in-flight
  commit per tenant and bounded cross-tenant concurrency. Definitive
  non-retryable event/data failures may still be counted and dropped, and queue
  capacity, non-blocking request acknowledgement, tenant isolation, graceful
  shutdown deadline, and hard-kill semantics must remain unchanged. Add a
  Postgres integration test that makes the first commit attempt fail
  transiently, restores the dependency without restaging the event, and proves
  the original batch commits once, in order, before `settle`/shutdown reaches
  zero.

### SEC-TEN-002 — INCORRECT — tenant admin allowances are not staged when first known

- Violated obligation: spec REQ-005 and `AGENTS.md`/agent rules require every
  permission evaluation, allowed or denied, to be staged after the decision is
  known and before the operation proceeds or refuses.
- Locations: `crates/wyrd/wyrd-server/src/components/admin/routes.rs:296-334`,
  `:375-385`, `:443-453`, `:523-546`, `:592-605`, and `:658-668`.
- Evidence: each route obtains an allowed `AuditEvent` from
  `authorize_service_accounts_write`, but defers `audit_outbox.stage` until
  after fallible work. All six defer it until after tenant-connection
  acquisition. `create_trusted_issuer` additionally performs screened outbound
  OIDC discovery and secret-envelope preparation first;
  `create_workload_binding` performs fallible projection first. Denials are
  correctly staged inside `authorize_recording_denial`, so the defect affects
  the returned allowance path.
- Reachable scenario: a caller with `service_accounts:write` requests trusted
  issuer creation whose screened discovery endpoint is unavailable, or any of
  these routes encounters a tenant-pool acquisition failure. The permission
  decision has already been evaluated, and trusted-issuer creation may already
  have performed outbound network activity, but the early `?` returns before
  the audit event is enqueued.
- Observable consequence: authorization history silently omits an allowed
  administrative decision and, for issuer creation, an attempted external
  interaction. Repeated failing attempts are invisible to the canonical audit
  trail even though permission evaluation occurred.
- Required testable correction: on each affected route, finish only the pure
  construction needed to make the event accurate, then stage the returned
  allowance immediately, before discovery, encryption/projection, connection
  acquisition, or other fallible work. Preserve the existing denial staging,
  typed permission failures, RLS tenant binding, SSRF screening, and operation
  semantics. Add focused coverage that forces at least discovery failure and
  tenant-connection acquisition failure after an allowed check, drains the
  outbox, and proves exactly one allowed decision with the caller's verified
  tenant, principal, credential attribution, and request ID.

## Security Audit

### Critical

- None.

### High

- None.

### Medium

- `[crates/vala/vala-sql/src/audit_outbox.rs:271]` Transient audit-store
  failures are finalized as permanent loss rather than retried. A short pool or
  database fault during a burst of sensitive operations can erase the whole
  batch's accountability evidence while the operations continue. Preserve the
  batch and retry transient failures with bounded backoff and tenant ordering.
- `[crates/wyrd/wyrd-server/src/components/admin/routes.rs:296]` Allowed tenant
  administration decisions can return before audit staging; trusted-issuer
  creation can also perform outbound discovery first. An authorized caller can
  make repeated failing administrative attempts that do not appear in the
  canonical trail. Stage the completed allowance immediately after evaluation,
  before any fallible work.

### Low / Defense In Depth

- None within the approved task boundary.

### Positive Controls

- Permission denials remain blocking and keep their typed `403`/authentication
  behavior after audit-unavailable variants are removed.
- Tenant-scoped commits acquire a `TenantConn` for the selected
  `DataTenantId`; the new publication table has forced RLS and the expected
  tenant policy.
- Platform decisions are staged under the canonical system sentinel, while
  peer violations use the system chain until receiver-owned state has verified
  the tenant binding.
- Audit failure logs include operation and typed request ID, not event detail or
  bearer/secret material.
- Gate and Oracle retain authorization before data effects, delegation
  attribution, and receiver-side peer fence/body/tenant checks.
- No compatibility fallback reintroduces an audit-unavailable response or
  converts a real permission refusal into success.
