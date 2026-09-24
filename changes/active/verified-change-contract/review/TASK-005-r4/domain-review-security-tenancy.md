# TASK-005 security and tenancy domain review

**Result: PASS.** Immutable cumulative subject: `f8811ac5035c3aa165d34c38992f9889b3c9081f..6d93b75825926ac27c9fd7de90879b19d67e39b3`. Reviewed approved spec revision 36, original TASK-005, prior r1/r2/r3 findings and remediation, and the cumulative security boundary. No material security or tenancy finding remains.

## Boundary, authority, and source coverage

| Boundary | Governing authority | Source and result |
|---|---|---|
| Tenant SYSTEM read identity | `spec.md` REQ-084; `AGENTS.md` §§2, 9; `architecture/wyrd-design.md` SYSTEM identity; `architecture/wyrd-security-posture.md` access tokens and tenant authority | `wyrd-auth/src/issuance.rs:541-598` obtains the observation table UID from a `TenantConn` catalog lookup, validates the UID-bearing Verifier, tenant credential admission, and persisted UUIDv7 SYSTEM principal, and signs one short-lived, table-scoped read permission. Missing table mints no token. PASS. |
| Closed token verification | Security posture token shape; `architecture/agent-rules.md` tenant and audit rules | `wyrd-auth-verify/src/lib.rs:801-831` rejects SYSTEM claims with roles, credentials, delegation, root Card, malformed identity or scope, or multiple purposes; `wyrd-runtime/src/permission.rs:613-637` fixes the read permission shape. PASS. |
| Scoped SQL and query admission | Spec revision 36; `architecture/bifrost-design.md` query and read audit contract; `AGENTS.md` §9 | `wyrd-server/src/verification/drift.rs:93-221,533-625` fixes table and SQL forms, escapes variable string literals via SQL AST rendering, checks numeric edges, verifies the minted token for the run tenant, and calls `ScheduledQueryCaller::authenticated`. `query/scheduled.rs:160-186` reaches `query/service.rs:259-283`; Oracle's `authorize_resolved_tables` in `vala-bifrost-redux/src/oracle/mod.rs:4698-4754` checks catalog-resolved table UIDs before source IO or peer dispatch. PASS. |
| Read decisions and negative paths | `architecture/bifrost-design.md` read audit; security posture audit; `AGENTS.md` §11 | `query/service.rs:72-92,259-283` handles capability admission and audited object denial; Oracle owns allowed read decision. `pg_grpc_ingest_smoke.rs:1608-1750` proves no-table behavior, wrong tenant refusal, result-write refusal, other-table refusal, and allowed/denied audit rows. The peer-forwarded read test in `wyrd-testing/tests/bifrost/server/verification_runtime.rs` covers a runner without local Oracle. PASS. |
| Shutdown claim tenancy | Security posture TenantConn/RLS and fail-closed settlement; `architecture/agent-rules.md` transaction ownership | The r3 fix in `verification/fitter.rs:232-273` claims and settles through `postgres.tenant_conn(tenant)`, rolls back a claim still pending on shutdown, releases a committed late claim without fitting, and uses the existing fenced lease for settlement. No new identity source, cross-tenant pool, or authorization bypass is introduced. PASS. |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None required by this task.

### Positive Controls

- The internal reader has one tenant, one registered table UID, one operation, one Verifier attribution, and a lifetime capped at five minutes; read and result-write tokens cannot combine permissions.
- Fixed SQL limits the subject, series, and window using escaped literals; Oracle makes the authoritative table decision using catalog identities, including distributed execution.
- Query admission and Oracle record read decisions through the canonical audit path; denials are covered by a real server integration test.
- The shutdown change retains TenantConn/RLS and fenced lease settlement rather than adding a privileged claim path.

## Verification limits

This was a source and diff audit; I did not rerun integration tests. I inspected the recorded successful focused Postgres test, `test:bifrost:integration:server`, `test:wyrd`, Drift journey, format/lints, and `git diff --check` evidence. The token intentionally permits its tenant's full observation table; subject, series, and window restriction relies on the server-built SQL, as approved by spec revision 36. The shutdown claim test checks lifecycle behavior; its security consequence is bounded by the existing `TenantConn` and lease implementation.

**Material proposed findings:** none.
