# Tenancy, Persistent Data, Durability, and Concurrency Review

## Review Findings

### Critical

None.

### Important

- **`DATA-R7-1` — INCORRECT: same-second credential rotation can mint an unusable successor token.** [`crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs:198`](../../../../../../crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs#L198) stores a service-account epoch with sub-second `now()`, while [`crates/shared/wyrd-auth-issue/src/lib.rs:581`](../../../../../../crates/shared/wyrd-auth-issue/src/lib.rs#L581) truncates every access-token `iat` to whole seconds and [`crates/shared/wyrd-auth-verify/src/lib.rs:551`](../../../../../../crates/shared/wyrd-auth-verify/src/lib.rs#L551) rejects that token when its whole-second `iat` is earlier than the sub-second epoch. Therefore a surviving credential exchanged during the same wall-clock second as another credential's revocation successfully returns a token that the next protected request rejects, contradicting `REQ-007` and `AC-005`'s uninterrupted overlapping rotation, the route's promise that a surviving credential “re-exchanges immediately” at [`crates/wyrd/wyrd-server/src/components/principals/routes.rs:472`](../../../../../../crates/wyrd/wyrd-server/src/components/principals/routes.rs#L472), and the R6 acceptance requirement that ordered successors remain valid. The new journey stops after asserting that exchange succeeds at [`crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:1169`](../../../../../../crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs#L1169), so it does not exercise verification of the returned token and cannot detect the outage. **Required correction:** reuse the existing ordered-epoch mechanism already used for human role withdrawal: advance the service-account epoch to the next whole second, return/read that stored instant during exchange, and mint a surviving credential's successor at or after that exact epoch; then make the real-server rotation proof use the successor token on a protected route immediately while the predecessor remains refused on its next request.

### Suggestions

None.

## Open Questions

None. The approved specification already fixes both immediate predecessor
revocation and uninterrupted overlapping rotation, so this does not require a
new product or persistence decision.

## Reviewed Boundary

Fresh Wave-1 review of immutable candidate
`ddd80c7a7b723a2f39a729e5aebe6b6072d2b983` against cumulative base
`c5c20754a167e8f4d74a555a720bd51df6179a6f`, including remediation range
`4d185da9..ddd80c7a`.

The review traced the complete cumulative persistent-data boundary far enough
to reassess tenant RLS, `TenantConn` and `OperatorPool` ownership, deployment
initialization, tenant provisioning/recovery, platform and tenant principal and
credential storage, authorization-decision/effect atomicity, revocation epochs,
tenant admission, audit staging and publication, migrations, and concurrency
constraints. It also traced the R6 changes through token issuance and
verification rather than accepting the implementation summary.

## Authority and Source Coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Tenant isolation and transaction ownership | `AGENTS.md`; `architecture/agent-rules.md`; SQL-foundation and security authorities; `TenantConn`; `OperatorPool`; auth/platform query owners | **PASS.** Tenant work remains inside caller-owned RLS transactions and cross-tenant work remains behind `OperatorPool`; no new connection abstraction, callee commit in a query module, or manual tenant-filter substitute was found. |
| Initialization, provisioning, recovery, and lifecycle | `REQ-020`–`REQ-033`, `INV-005`, `INV-006`, `AC-001`, `AC-002`, `AC-006`, `AC-007`; boot, provisioning, recovery, tenant directory, role, principal, credential, and admission paths | **PASS.** Durable creation and promotion boundaries, retry/concurrency behavior, and tenant admission remain transactionally coupled as approved. |
| R6 combined admission and epoch read | `REQ-005`, `INV-013`, `AC-008`, `AC-010`; `SqlRevocationCheck`; `user_admission`; `service_account_admission`; `platform.tenant_admits_credentials`; token-verifier cache-hit and cache-miss paths | **PASS.** Every tenant token verification acquires one tenant transaction and executes one statement whose snapshot contains both lifecycle admission and the RLS-visible principal epoch. SQL or connection failure maps to unavailable and fails closed. The epoch cache, listener, notification fan-out, and their dependencies are gone while the separate verified-token cache remains. |
| Credential revocation ordering and concurrency | `REQ-006`, `REQ-007`, `INV-013`, `AC-005`, `AC-010`; credential ownership/revoke SQL; epoch mutation; API-key exchange; access-token issuance; verification; rotation journey | **FAIL (`DATA-R7-1`).** Revocation is atomic and concurrent replay cannot advance the epoch twice, but the epoch/token precision mismatch creates a same-second outage for the surviving credential's successor. |
| Authorization decision/effect atomicity | `REQ-037`, `AC-009`; canonical audit rules; platform tenant-status and admin-registration paths; tenant principal create/issue/list paths; `append_audit`; R6 no-effect proof | **PASS.** Stable logical no-effect results commit the already-appended decision only; lookup, mutation, hashing, append, and commit failures still roll back both decision and attempted effect. Role resolution now precedes principal insertion, so an absent role leaves no tentative principal. |
| Audit durability and publication | canonical audit and Bifrost authorities; audit staging schema and append; hash chain; publisher freeze/read/settle and watermark behavior; current projection | **PASS.** The candidate retains one append path, one publisher, transaction-coupled decisions, deterministic frozen publication ranges, monotonic settlement, and the current strict audit projection. |
| Migration durability and durable constraints | admin-principal, tenant-lifecycle, platform-identity, tenant-admission, and Vala audit migrations; principal/credential constraints and migration tests | **PASS.** Ordered forward migrations and durable tenancy/kind/credential constraints remain intact; no applied migration rewrite was introduced by R6. |

## Verification Notes

- Review-only: no candidate source was edited and no broad suite was rerun.
- The remediation packet records passing format/lint/boundary checks, SQL,
  principal integration, platform journey twice, identity/CLI/MCP journeys,
  storage, Bifrost integration, codegen, examples, and docs. Those results were
  considered only for their stated selectors.
- The recorded warm-token tenant-admin test directly proves next-request
  refusal of the predecessor, but lines 1169–1172 prove only that the surviving
  credential exchanges; they do not submit its returned bearer token to any
  verifier. The implementation packet itself records the same-second limitation.
- `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f..ddd80c7a7b723a2f39a729e5aebe6b6072d2b983`
  was clean.
- Candidate HEAD was rechecked immediately before writing and remained
  `ddd80c7a7b723a2f39a729e5aebe6b6072d2b983`.

## Overall Result

**FAIL**

The R6 direct admission-and-epoch read and no-effect audit corrections preserve
the reviewed SQL, RLS, and atomicity boundaries, but `DATA-R7-1` leaves the
credential-rotation acceptance behavior observably incomplete.
