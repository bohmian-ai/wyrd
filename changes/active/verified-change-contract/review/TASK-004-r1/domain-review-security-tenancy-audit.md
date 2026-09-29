# TASK-004 Security, Tenancy, Identity, and Audit Domain Review

## Subject

- Base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Candidate: `49ad24707de47378b9df51764034c49a129c6b8b`
- Candidate was still `HEAD` at the end of review.
- Reviewed boundary: SYSTEM principal provisioning, issuance and verification; exclusion from public principal/credential lifecycle; Gate's closed result-table matrix; signed Card scope enforcement; tenant derivation and Postgres RLS; Verification route permissions; and authorization-audit cardinality.

## Authority and Source Coverage

| Boundary | Authority | Source and proof inspected |
|---|---|---|
| SYSTEM identity lifecycle | TASK-004 Scenario 1 and acceptance criteria; spec REQ-086, AC-023; `architecture/wyrd-security-posture.md` principal/token rules; `architecture/wyrd-design.md` runtime identity | `20260601000028_system_principal.sql`; `service_accounts.rs`; platform tenant provisioning; `TenantTokenIssuer::issue_system_token`; shared issuer/verifier and runtime principal projections; principal route/revocation callers; principal integration and real-router tests |
| Result admission and signed scope | spec REQ-086, REQ-145, INV-007, AC-023, AC-030; security posture authorization and audit rules | Gate `authorize_record_write` and native-frame path; Scribe `validate_card_scope` and stamping; result payload construction; gRPC SYSTEM-writer integration proof; Gate matrix and scope unit tests |
| Tenancy and durable control state | spec REQ-078, REQ-079, INV-007; security posture tenant/data isolation; `AGENTS.md` and `architecture/agent-rules.md` TenantConn/RLS rules | `verifier_runs` and idempotency migrations; verifier-run SQL owner; scheduler, runner, publisher and Verification control service; cross-tenant route tests and RLS definitions |
| Public Verification authorization/audit | spec REQ-136, REQ-145, AC-030; security posture authorization/audit integrity | Verification HTTP routes and shared `VerificationControl`; canonical audit calls; manual-run allow/deny and cross-tenant route tests |
| Audit projection | spec REQ-086 and AC-023; canonical audit authority | audit-staging migration, row type and Bifrost audit projection for `principal_kind=system`; Gate audit sink and integration assertions |

CodeGraph was unavailable because this worktree has no `.codegraph/` index, so source and callers were traced with repository search and direct inspection.

## Security Audit

### Critical

- None.

### High

- **SEC-001 — INCORRECT / VIOLATION** — [`crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:730`] SYSTEM result writes can bypass their exact Verifier scope by omitting `card_ref` or supplying null. REQ-086 requires every reserved result-table write to require the token's exact signed Verifier scope, requires result/detail rows to carry that managed Verifier Card UID, and makes Gate's combined table/scope check the single canonical audited decision. `validate_card_scope` explicitly accepts a missing column at lines 731-733 and null rows at lines 738-741. Gate's decision at [`crates/vala/vala-bifrost-redux/src/gate/mod.rs:446`] checks only `bifrost_record:write`, principal kind, and table, records `allowed`, and then passes the frame to Scribe. The canonical source contract treats `card_ref` as optional, while the normal publisher merely supplies non-null values by convention at [`crates/wyrd/wyrd-server/src/verification/results.rs:373`]. **Exploit path:** bearer tokens are explicitly replayable until expiry; anyone who obtains a still-live five-minute SYSTEM bearer, or a compromised result worker using it, can send a schema-valid Arrow batch to public gRPC for any reserved result table with no `card_ref` column (or all-null values). Gate records an allowed SYSTEM write, Scribe accepts it, and stamping persists null `card_uid`, defeating the token's one-Verifier confinement and introducing canonical verification evidence that is not attributable to the exact Verifier version. **Impact:** tenant-local verification-result integrity and audit accuracy are broken; result/detail rows can enter the assurance store outside the only Card scope the token was intended to authorize. **Required correction:** in the existing Gate/Scribe admission path, make SYSTEM admission to the three reserved result tables require a present, non-null `card_ref` on every row and authorize every value against the token's sole signed UID-bearing Verifier before durable Scribe admission. Preserve one canonical Gate decision: missing, null, malformed, or out-of-scope correlation must produce exactly one denied `bifrost_record:write` audit row, while the exact signed Verifier produces one allowed row. Add a real authenticated gRPC regression covering absent, null, and foreign-Verifier `card_ref`, asserting denial, no durable result row, and one denied audit event for each request; retain the valid exact-scope success case.

### Medium

- None.

### Low / Defense In Depth

- None. No speculative hardening is proposed.

### Positive Controls

- SYSTEM provisioning is tenant-scoped, idempotent, UUIDv7-backed, unique per tenant, and coupled to tenant provisioning; the migration also backfills existing non-sentinel tenants.
- Public principal and credential paths converge on `service_account_by_id`, which excludes `principal_kind = 'system'`; suspension/deletion queries also exclude SYSTEM.
- Internal issuance loads the persisted tenant SYSTEM row through `TenantConn`, caps normal issuance at five minutes, grants exactly `bifrost_record:write`, and supplies no roles, credential, root Card, refresh token, or delegation.
- Token verification rejects malformed SYSTEM claims, including non-UUIDv7 identity, mismatched subject, root Card, roles, credentials, delegation, non-exact permissions, and empty/multi/non-Verifier/UID-less scope.
- Gate correctly denies every non-SYSTEM principal, including wildcard administrators, on the three result tables and denies SYSTEM on other tables; audit staging and retained projection accept the stored `system` kind.
- Verification control tables added by TASK-004 enable and force RLS and use `wyrd.current_tenant()`; runtime mutation paths use `TenantConn`, while cross-tenant discovery/metrics use read-only `OperatorPool` queries.
- Manual Verification operations reuse `cards:read` and `evals:run`, derive tenancy from the verified caller, enforce Card-bound subject scope before enqueue, freeze the requester separately, and exercise allow/deny and cross-tenant cases.

## Verification Limits

- This was a static acceptance/security review. I did not rerun the large verification matrix; the task records successful targeted and aggregate lanes, which I treated as supplied evidence rather than independent execution.
- Existing positive tests cover conforming SYSTEM mint/verify, malformed claim shapes, public-route exclusion, Gate's kind/table matrix, foreign Verifier scope refusal inside Scribe, tenant forgery, and canonical audit cardinality. They do not exercise missing or null `card_ref` under a valid SYSTEM token through real gRPC, which is the reachable gap in SEC-001.
- Runtime scheduling, lease durability, publication retry, and analytical schema semantics were inspected only where they crossed identity, tenancy, or audit boundaries.

## Overall Result

**FAIL** — SEC-001 leaves the reserved result-write trust boundary weaker than REQ-086 and AC-023 require. The issue is bounded within the existing Gate/Scribe admission owner and does not require a specification revision.
