# TASK-002 Review Verdict — R6

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `0ca117a744ddcb7414b104c4382027970531b608`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation tasks: TASK-002 R1 through R5
- Reviewed range: complete cumulative base-to-candidate diff, 93 files,
  9,537 insertions and 1,842 deletions

The candidate remained `0ca117a744ddcb7414b104c4382027970531b608`
through both review waves. Review artifacts are outside the immutable subject.
Lead-directed reuse and test commits recorded separately in the evidence tables
were treated as authorized rather than scope drift and were still inspected for
regression.

## Verdict

**FIX_REQUIRED**

The cumulative implementation satisfies the original tenant-login, OIDC,
identity, tenancy, public-contract, journey, and prior-remediation obligations.
One bounded concurrency defect remains: administrative User revocation performs
a family-wide refresh-token update without participating in the existing
refresh-family lock, so it can miss a successor inserted by a concurrent
rotation. The existing lock and transaction owners are sufficient; no
specification revision is required.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-006–008 and INV-001–002: tenant routing is pre-login context only; callback trust checks fail closed; human identity is exact `(issuer, sub)` | Opaque one-use state, screened provider IO, complete ID-token checks, Active-connection recheck, tenant User/role owners, and login/refusal journeys | PASS |
| REQ-013–015 and INV-003: machine, platform, and tenant-human authority remain separate and tenant-isolated | Existing machine and platform owners remain distinct; forced RLS and narrow state lookup bound tenant login; machine and same-issuer two-tenant journeys | PASS |
| REQ-016 and packet renewal contract: connection replacement cuts renewal off and all family-wide refresh mutations serialize with rotation | Connection provenance and replay/rotation serialization pass, but administrative User revocation omits the family lock and can miss a concurrent successor (`FIND-TASK-002-14`) | **FAIL** |
| REQ-017: issuance, role changes, refusal, and replay containment use canonical transactional audit | Callback and refresh route transaction boundaries plus audit rollback/containment evidence | PASS |
| AC-002/003/005/006/007 task slices and four required real-server journeys | Recorded focused journeys and full identity lane; focused verifier and concurrency proofs | PASS except the uncovered administrative-revocation overlap above |
| Packet begin/callback/completion contract and authorization-code retirement | Typed state-bound flow, fixed safe callback result, sealed completion, removed legacy grant/client/CLI path, OpenAPI/schema proof | PASS |
| Remediation R1–R5 and prior findings `FIND-TASK-002-1` through `-13` | Independently revalidated against current source and callers | PASS |
| Repository rules, contracts, generated artifacts, documentation, and authorized lead-directed reuse/test commits | Standards review and cumulative diff inspection | PASS |
| Explicit non-goals and absence of scope drift | No compatibility route, alternate trust/audit model, email linking, provider-token bearer authority, TASK-003 BFF route, or TASK-004 CLI persistence entered the candidate | PASS |

The complete obligation matrix is preserved in `task-review.md`; detailed
authority and domain coverage is preserved in the other Wave 1 reports.

## Wave 1 results

| Review | Result | Proposed findings |
|---|---|---|
| Task implementation | PASS | None |
| Repository standards | PASS | None |
| Security/OIDC/RBAC domain | PASS | None |
| Tenancy/persistence/concurrency domain | FAIL | `TD-R6-001` |

Wave 2 independently traced all family-lock, family-revocation, rotation,
principal-revocation, route-transaction, and test callers. It confirmed
`TD-R6-001` as the new stable finding below and validated the other Wave 1
ledgers as empty.

## Validated finding ledger

| Finding | Status | Classification | Violated obligation | Decision-complete correction and closure proof |
|---|---|---|---|---|
| `FIND-TASK-002-14` | CONFIRMED from `TD-R6-001` | INCORRECT | The tenant renewal contract, R4 family serialization, and administrative User-revocation contract require family retirement not to miss a concurrent successor | Reuse the existing tenant-qualified `lock_refresh_family` in the User branch of the existing revocation owner after existence is established and before suspension/family revocation. Preserve `TenantConn`, route-owned commit, and family-before-connection order. Add one deterministic Postgres overlap proof that rotation and administrative revocation leave the successor revoked with no active family row. |

## Prior-finding closure

`FIND-TASK-002-1` through `FIND-TASK-002-13` remain **CLOSED**. Wave 2
specifically revalidated exact OIDC subject identity, `azp`, role-change audit,
advertised algorithms, RLS-only state transitions, owner-bound state lookup,
secret redaction, Rust documentation/import rules, mandatory ID-token claims,
replay-versus-rotation serialization, and the corrected refresh lookup rustdoc.
`FIND-TASK-002-14` is a distinct omitted production caller of the shared family
mutation invariant, not a reopening of the accepted replay path.

## Verification limits

- Reviewers inspected the complete cumulative diff, all R1–R5 tasks and prior
  ledgers, applicable authorities, complete relevant bodies and callers,
  migrations, contracts, generated artifacts, and focused tests. No usable
  CodeGraph index exists, so inspection used Git, `rg`, and direct source reads.
- Fresh evidence includes the focused shared-verifier test and cumulative
  `git diff --check`. Recorded candidate evidence covers the four identity
  journeys, full identity lane, principals unit/integration, SQL, tenant
  isolation, codegen/docs, format, and lints.
- The bounded review did not rerun Docker, Keycloak/Dex, Postgres, migration,
  or broad workspace lanes. No existing test overlaps administrative User
  revocation with current-token rotation; sequential revocation and the
  replay/rotation overlap test cannot prove this seam.
- Every required reviewer completed within the 20-minute limit. No independent
  review report is missing.
- TASK-003 BFF completion, TASK-004 CLI persistence, and live Okta/Entra
  qualification remain intentional downstream or change-level work.

## Remediation

Implement `TASK-002-R6-admin-revocation-refresh-family-serialization.md`
through `$wyrd-implement`, then reassess the complete original
base-to-new-candidate range.
