# TASK-002 Review Verdict — R7

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `e1ce3c847c14d306c69a704cd15ce6686db9e62e`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation tasks: TASK-002 R1 through R6
- Reviewed range: complete cumulative base-to-candidate diff, 101 files,
  10,841 insertions and 1,850 deletions

The candidate remained `e1ce3c847c14d306c69a704cd15ce6686db9e62e`
through both review waves. Review artifacts are outside the immutable subject.
Lead-directed reuse and test commits recorded separately in the evidence
tables were treated as authorized rather than scope drift and were still
inspected for regression.

## Verdict

**FIX_REQUIRED**

The cumulative implementation satisfies the tenant-login, OIDC, identity,
tenancy, public-contract, journey, and prior-remediation obligations except for
one reachable concurrency gap. Initial human-session issuance does not
participate in the refresh-family lock used by refresh rotation and
administrative User revocation. An in-flight callback can therefore read an
Active User, allow revocation to commit, and then commit new access and refresh
authority after revocation returned. The existing owner and lock are
sufficient; no specification revision is required.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-006–008 and INV-001–002: tenant routing is pre-login context only; callback trust checks fail closed; human identity is exact `(issuer, sub)` | Opaque one-use state, screened provider IO, complete ID-token checks, Active-connection recheck, tenant User/role owners, and login/refusal journeys | PASS |
| REQ-013–015 and INV-003: machine, platform, and tenant-human authority remain separate and tenant-isolated | Existing machine and platform owners remain distinct; forced RLS and the narrow state lookup bind tenant login; machine and same-issuer two-tenant journeys provide recorded proof | PASS |
| REQ-016, INV-004, and the renewal/revocation contract: suspension refuses next issuance and successful revocation leaves no renewable authority | Rotation, replay, connection cutoff, and administrative revocation versus rotation serialize correctly, but first issuance omits the family lock and can commit new authority after revocation (`FIND-TASK-002-15`) | **FAIL** |
| REQ-017: issuance, role changes, refusal, replay containment, and administrative revocation use canonical transactional audit | Callback, refresh, and revocation transaction boundaries plus audit rollback/containment evidence | PASS |
| AC-002/003/005/006/007 task slices and four required real-server journeys | Recorded focused journeys and full identity lane; focused verifier and prior concurrency proofs | PASS except the uncovered initial-issuance/revocation overlap |
| Packet begin/callback/completion contract and authorization-code retirement | Typed state-bound flow, fixed safe callback result, sealed completion, removed legacy grant/client/CLI path, and OpenAPI/schema proof | PASS |
| R1–R6 and prior findings `FIND-TASK-002-1` through `-14` | Independently revalidated against current source and callers | PASS |
| Repository rules, generated artifacts, documentation, and authorized lead-directed reuse/test commits | Standards review and cumulative diff inspection | PASS |
| Explicit non-goals and absence of unrelated scope drift | No compatibility route, alternate trust/audit model, email linking, provider-token bearer authority, TASK-003 BFF route, or TASK-004 CLI persistence entered the candidate | PASS |

The complete obligation matrix is preserved in `task-review.md`; detailed
authority and domain coverage is preserved in the other Wave 1 reports.

## Wave 1 results

| Review | Result | Proposed findings |
|---|---|---|
| Task implementation | PASS | None |
| Repository standards | PASS | None |
| Security/OIDC/RBAC domain | PASS | None |
| Tenancy/persistence/concurrency domain | FAIL | `TD-R7-001` |

Wave 2 independently traced every caller and full body of the human-session
issuer, refresh-family lock, refresh-row insertion, callback, rotation, User
lookup, and administrative revocation paths. It confirmed `TD-R7-001` as the
stable finding below and validated the other Wave 1 ledgers as empty.

## Validated finding ledger

| Finding | Status | Classification | Violated obligation | Decision-complete correction and closure proof |
|---|---|---|---|---|
| `FIND-TASK-002-15` | CONFIRMED from `TD-R7-001` | INCORRECT | Principal suspension must refuse next issuance; callback issuance and administrative revocation must leave no renewable authority committed after successful revocation; human refresh insertion must preserve family-before-connection ordering | At the start of `TenantTokenIssuer::issue_human_session`, reuse `lock_refresh_family(conn, "user", principal_id)` before the connection-slot lock and principal-status read. Preserve caller-owned transactions, audit, provenance, persistence, and public contracts. Add one deterministic Postgres proof covering issuance-first and revocation-first orderings. |

## Prior-finding closure

`FIND-TASK-002-1` through `FIND-TASK-002-14` remain **CLOSED**. Wave 2
specifically revalidated exact OIDC subject identity, `azp`, role-change audit,
advertised algorithms, RLS-only state transitions, owner-bound state lookup,
secret redaction, Rust documentation/import rules, mandatory ID-token claims,
replay-versus-rotation serialization, corrected refresh lookup documentation,
and administrative revocation versus refresh rotation. `FIND-TASK-002-15` is a
distinct omitted initial-issuance participant in the same family serialization
protocol.

## Verification limits

- Reviewers inspected the complete cumulative diff, all R1–R6 tasks and prior
  ledgers, applicable authorities, relevant complete bodies and callers,
  migrations, contracts, generated artifacts, and focused tests. No CodeGraph
  index exists, so inspection used Git, `rg`, and direct source reads.
- Fresh review evidence passed the focused administrative-revocation versus
  rotation proof, the connection-deactivation versus rotation proof, their
  repository-managed migration setup, and cumulative `git diff --check`.
- Recorded candidate evidence covers the four identity journeys, full 27-test
  identity lane, principals unit/integration, SQL, tenant isolation,
  codegen/docs, format, and lints. The bounded review did not rerun all broad
  Docker, Keycloak/Dex, Postgres, codegen, docs, or workspace lanes.
- No current test overlaps administrative User revocation with initial human
  session issuance. Existing overlap tests cannot prove this seam.
- Every required reviewer completed within the 20-minute limit; no required
  report is missing.
- TASK-003 BFF completion, TASK-004 CLI persistence, and live Okta/Entra
  qualification remain intentional downstream or change-level work.

## Remediation

Implement `TASK-002-R7-initial-session-refresh-family-serialization.md`
through `$wyrd-implement`, then reassess the complete original
base-to-new-candidate range.
