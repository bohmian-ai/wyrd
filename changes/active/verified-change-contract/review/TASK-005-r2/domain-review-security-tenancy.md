# Security and tenancy domain review

## Subject and boundary

- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `81d2221b4d3b1a9fb6bd36c591d421385cc8c67d`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 36; original task `tasks/TASK-005-production-drift-verifier.md` and r1 remediation evidence.
- Boundary: SYSTEM Drift read issuance and verification, tenant and table scope, fixed SQL, query admission, local or peer Oracle dispatch, and read decision auditing. This is a static, review-only audit of the cumulative candidate.

## Authority and source coverage

| Obligation | Authority | Source and proof inspected | Result |
|---|---|---|---|
| Tenant-local SYSTEM identity; one short-lived purpose and one Verifier scope | `AGENTS.md` §§2, 9; `architecture/wyrd-design.md` SYSTEM identity; `architecture/wyrd-security-posture.md` access tokens; spec revision 36 and AC-030 | `wyrd-auth/src/issuance.rs:519-613`; `wyrd-auth-verify/src/lib.rs:794-832`; `wyrd-runtime/src/permission.rs:605-637`; verifier claim tests | PASS |
| Observation-table-only read; no result write or cross-tenant use | `architecture/bifrost-design.md` object-scoped query permission; `architecture/wyrd-security-posture.md` tenant isolation; spec AC-030 | Issuer resolves the tenant's registered table UID under `TenantConn`; `oracle/mod.rs:4694-4752` checks every resolved table; `pg_grpc_ingest_smoke.rs:1608-1740` proves other-tenant verification refusal, write refusal, table denial, and allowed/denied audit rows | PASS |
| Subject, series and half-open window from server-built SQL | Spec revision 36; `architecture/wyrd-security-posture.md` explicit SQL limit; `architecture/references/domain/olap-serving.md` | `verification/drift.rs:93-267,395-616`: typed subject UID, timestamp and escaped SQL string literals; SQL execution tests cover quote escaping, subject/series and window exclusion | PASS |
| Ordinary query path reaches peer Oracle and audits reads | `architecture/agent-rules.md` canonical audit path; `architecture/bifrost-design.md` query route and read decision; spec AC-030 | `query/scheduled.rs:48-181` authenticated dispatch; `query/service.rs:259-280` coarse admission, Gate dispatch and audited object denial; `gate/mod.rs:629-683`; `oracle/planner.rs:186-214` pre-read authorization; `oracle/mod.rs:3270-3340` accepted read audit; `wyrd-testing/tests/bifrost/server/verification_runtime.rs:185-246` proves peer read with one audit row | PASS |

Applicable reference routing also checked: `architecture/references/README.md`, `languages/spec-driven-development.md`, `languages/testing-workflows.md`, and `doctrine/architecture-constraints.md`.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None within the approved task.

### Positive Controls

- The issuer gets the table UID from tenant-scoped storage, mints no token before the table exists, and caps token life at five minutes. The verifier rejects added permissions, missing Verifier scope, roles, credentials, and delegation.
- Oracle authorizes the complete resolved scan set before source IO. The permission's table UID binds the registered observation table, and `AuthorizedQueryContext::try_new` refuses caller/principal tenant mismatch.
- Drift's SQL interpolates only typed numbers and escaped literals. The read token deliberately covers the tenant's whole observation table; subject, series and window restrictions are carried by the fixed server SQL as revision 36 specifies.
- The authenticated scheduled caller enters the normal query service. An allowed read yields one Oracle read decision, and an out-of-scope table returns `QueryForbidden` with an audited denial. The peer journey exercises this path without a local Oracle.

## Verification limits

- I inspected source and the named tests but did not rerun the reported `mise` lanes. The task reports format, lints, Vala, SQL, Wyrd, Bifrost, SDK journeys, codegen, tenant-isolation and boundary checks passing after the final code change.
- The peer test proves one allowed audited read; the separate SYSTEM reader test proves allowed and denied decisions and token scope. Their combination supports the reviewed boundary. The broader scheduler, fitter resource controls, and SDK method semantics belong to other review scopes.

## Overall result

**PASS** — no reachable security or tenancy finding within this boundary.
