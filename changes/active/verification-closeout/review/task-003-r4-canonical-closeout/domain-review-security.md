# TASK-003 r4 security and RBAC domain review

**Subject:** `7f79fb341..f6c841d57` in `wyrd-verification-closeout-task3` (candidate `f6c841d57`). **Result: PASS.** No material security finding in the reviewed boundary.

## Boundary and authority

Reviewed the approved [spec](../../spec.md), [task](../../tasks/TASK-003-r4-canonical-support-desk-closeout.md), cumulative diff, `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`, and the applicable architecture-constraints reference. The governing rules are verified-principal tenancy, signed Card scope or tenant registry resolution for attribution, exact `verifier:run` authorization, and synchronous permission decisions with nonblocking audit staging.

## Source coverage and result

| Boundary | Source path and security result |
|---|---|
| Public gateway authentication and input | `components/gateway/ingress.rs:106-145,245-291`: Wyrd token is verified before handlers; paired headers reject duplicates, malformed UID, missing mate, and empty or oversized Run ID. Headers are consumed rather than forwarded. PASS. |
| Gateway authorization and capture | `components/gateway/invocation.rs:424-482,651-738`: model permission is audited before Card attribution; a bound principal's requested UID must be in its signed UID scope; an unbound principal's UID is resolved through a `TenantConn` and restricted to observation-target kinds before routing, admission, or dispatch. `components/gateway/capture.rs:232-305` projects the authorized UID and Run. `pg_invocation_tests.rs:3100-3229` exercises allowed capture and both denied branches before dispatch. PASS. |
| SDK call and judge authority | `wyrd-client/src/observe/invoke.rs:40-67` binds `Run::invoke` to its Agent UID and the authenticated shared client; `wyrd-client/src/workflow/gateway.rs:115-144` sends both headers through that client's auth transport. `verification/authority.rs:175-217` recovers the queued observation writer in a tenant transaction; `wyrd-auth/src/issuance.rs:520-619` reloads active principal, current roles, grants, and scope; `verification/eval.rs:201-219,324-354` hands that caller to the gateway. Direct judges use the request caller. PASS. |
| Verifier authorization and tenant isolation | `components/verification/service.rs:414-447,598-629` checks exact Verifier UID permission and audits the decision before tenant-scoped resolution and execution; direct subject resolution is in the same tenant. `wyrd-sql/src/queries/auth/service_accounts.rs:212-239` excludes inactive and SYSTEM identities from recovery. PASS. |
| Unbound ingest attribution | `vala-bifrost-redux/src/gate/attribution.rs:59-120,130-235` resolves only parseable UIDs through a tenant connection, bounds distinct lookups, and excludes non-observation kinds; Scribe checks the resulting scope. PASS. |

## Security audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense in depth

None required for this task. The application Run ID is intentionally opaque and caller supplied; its value is correlation metadata, while authorization rests on the verified principal, Card UID, and model or Verifier grant.

### Positive controls

- Tenant identity comes from the verified caller. Card and Verifier lookups use `TenantConn`, not a client-selected tenant or privileged pool.
- The gateway rejects out-of-scope attribution before any provider attempt, and the captured Card UID is the checked value.
- Queued LLM judging recovers current writer authority; it does not use SYSTEM authority or a provider credential fallback.
- Direct Verifier execution checks the exact UID grant and stages both allowed and denied decisions.

## Verification limits

I inspected source and the complete security-relevant cumulative diff, including the changed auth, Gate attribution, gateway, verification, client, and lockfile surfaces. I did not rerun tests. The task records a passing `mise run -c gate` on 2026-10-09, Scribe/Forge/Oracle journeys of 28/28, 22/22, and 50/50, and passing codegen, format, and lint checks; those are supplied evidence, not independently reproduced here. The lockfile changes inspected add the support-desk example's existing OpenTelemetry context dependency and do not show a new credential or network-execution package in this boundary.
