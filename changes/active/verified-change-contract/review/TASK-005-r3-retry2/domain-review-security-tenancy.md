# TASK-005 security and tenancy domain review

**Result: PASS.** Reviewed immutable range `f8811ac5035c3aa165d34c38992f9889b3c9081f..0ef7208565c69d96c44dfe5316394764678bd4e6` against approved spec revision 36 and the original TASK-005, including the prior r1/r2 findings and r2 remediation. No material security or tenancy finding remains.

## Boundary and authority coverage

| Boundary | Authority | Source evidence | Result |
|---|---|---|---|
| Tenant-scoped SYSTEM mint | Spec REQ-084 SYSTEM read purpose; `AGENTS.md` §§2, 9; `architecture/agent-rules.md` tenant connection and RLS rules | `wyrd-auth/src/issuance.rs:535-598` looks up only `vala.drift.observations` using `TenantConn`, mints at most five-minute token for the persisted tenant SYSTEM UUIDv7 principal, one UID-bearing Verifier, and exactly one table-scoped read permission. `vala-sql/src/queries/olap_catalog.rs:80-99` binds the FQN and relies on tenant RLS. No table is created when absent. | PASS |
| Closed token verification | Spec REQ-084; `architecture/wyrd-design.md` tenant and auth model | `wyrd-auth-verify/src/lib.rs:807-832` accepts one read permission for `vala.drift` or the existing result-write permission, never both, and rejects roles, credential, delegation, root Card, malformed SYSTEM ID, or missing Verifier UID. `wyrd-runtime/src/permission.rs:613-637` defines the exact read shape. | PASS |
| Query admission, object authorization and audit | Spec REQ-084; `architecture/bifrost-design.md:788-805`; `architecture/agent-rules.md:12-14` | `wyrd-server/src/verification/drift.rs:539-628` verifies the token and builds a `Caller`; `query/scheduled.rs` dispatches authenticated callers to `query/service.rs:259-283`; `query/service.rs:72-92,229-283` admits the capability and records object denials; `vala-bifrost-redux/src/oracle/mod.rs:4698-4754` authorizes every resolved table by registered UID before source IO. Oracle records allowed read decisions. | PASS |
| SQL trust boundary | Spec REQ-084 fixed SQL and literal requirement; `AGENTS.md` §9 | `wyrd-server/src/verification/drift.rs:93-221` fixes the table and SQL forms; subject, series, category labels, and timestamps use SQL parser quoted-string rendering; numeric fitted edges must be finite, and `chunk_size` is positive. No Verifier-supplied SQL text is used. | PASS |
| Tenant, table and write refusal proofs | `AGENTS.md` §11 user journey and negative flows | `wyrd-server/tests/pg_grpc_ingest_smoke.rs:1608-1748` exercises absent table, wrong-tenant token verification, refused result write, allowed observation read, refused result-table query, and staged audit outcomes. `wyrd-testing/tests/bifrost/server/verification_runtime.rs:185-244` exercises a runner without local Oracle using a peer with one audited read. | PASS |

## Security audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None required for this task.

### Positive controls

- The minted credential is scoped to a tenant, one table UID, one operation, and one Verifier; the read and write purposes are mutually exclusive.
- Oracle makes the authoritative object decision on catalog-resolved table identities, including peer-forwarded reads, and the query service audits denials.
- Fixed SQL renders variable values as quoted literals; the table name and query shapes are server-owned.

## Verification limits

This was a static review; I did not rerun the named integration tests. The task's recorded post-change verification includes formatting, lints, Vala, Wyrd, all nine Bifrost lanes, focused tests, and `git diff --check`; the directly relevant source tests above exercise the security boundary. The token intentionally authorizes the tenant's entire observation table; subject, series, and window restriction is supplied by server-built SQL, exactly as the approved spec states.

**Material proposed findings:** none.
