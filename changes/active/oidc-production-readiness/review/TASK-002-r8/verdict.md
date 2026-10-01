# TASK-002 R8 Review Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `ced8acaabce7dbe1682bef46d65d073b07e0dbd9`
- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation inputs: TASK-002 R1 through R7, their prior verdicts and
  validated ledgers

`HEAD` remained the candidate throughout both review waves. The untracked R8
review directory was outside the immutable subject. Lead-directed reuse and
test commits recorded separately in the evidence tables were treated as
authorized rather than scope drift and were inspected for regression.

## Verdict

**FIX_REQUIRED**

The cumulative implementation closes `FIND-TASK-002-1` through
`FIND-TASK-002-15`, including the R7 issuance/revocation race, and otherwise
satisfies the original task. Two bounded findings remain: one materially
modified CLI helper has false rustdoc, and the shared OIDC ID-token verifier
accepts a Subject Identifier outside the OpenID Connect syntax contract. Both
corrections fit existing owners and require no specification revision.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-006–008 and INV-001–002: tenant routing is pre-login only; callback trust checks fail closed; verified `(issuer, sub)` is the stable human identity | One-use state, screened provider IO, issuer/audience/signature/time/nonce/`azp` checks, exact connection recheck, tenant User/role owners, and real-server journeys pass; invalid empty, non-ASCII, or over-255-byte OIDC `sub` remains admitted (`FIND-TASK-002-17`) | **FAIL** |
| REQ-013–015 and INV-003: machine, platform, and tenant-human authority remain separate and tenant-isolated | Existing machine/platform owners remain distinct; forced RLS and narrow state lookup bind tenant login; machine and same-issuer two-tenant journeys provide proof | PASS |
| REQ-016 and INV-004: renewal, replay containment, connection cutoff, and administrative revocation serialize and fail closed | Family-before-connection locking, provenance checks, replay containment, R6 rotation/revocation proof, and fresh R7 two-ordering first-issuance/revocation proof | PASS |
| REQ-017: issuance, role change, refusal, replay containment, and revocation use canonical audit | Transactional audit and rollback evidence plus refusal/containment journeys | PASS |
| Packet begin/callback/completion contract and authorization-code retirement | Typed state-bound flow, fixed safe callback result, sealed completion, removed legacy grant/client/CLI path, schema/OpenAPI evidence | PASS |
| Rust documentation and repository rules | All other mapped rules pass; `print_tokens` carries a false argument-parsing sentence (`FIND-TASK-002-16`) | **FAIL** |
| R1–R7 and prior findings `FIND-TASK-002-1` through `-15` | Independently revalidated against current source, callers, prior ledgers, and focused R7 proof | PASS |
| Explicit non-goals and authorized scope | No compatibility route, email linking, provider-token bearer authority, platform fallback, new machine model, TASK-003 BFF route, or TASK-004 CLI persistence entered the candidate | PASS |

The complete obligation matrix is preserved in `task-review.md`; authority and
domain coverage are preserved in the other Wave 1 reports.

## Wave 1 results

| Review | Result | Proposed findings |
|---|---|---|
| Task implementation | PASS | None |
| Repository standards | FAIL | `REPO-R8-1` |
| Security/OIDC/RBAC domain | FAIL | `SEC-R8-001` |
| Tenancy/persistence/concurrency domain | PASS | None |

Wave 2 independently traced the complete correction owners and their callers.
It confirmed `REPO-R8-1` as `FIND-TASK-002-16`, revised `SEC-R8-001` as
`FIND-TASK-002-17`, overturned the task review's empty ledger only for that
claim-validation gap, and validated the tenancy/data empty ledger.

## Validated finding ledger

| Finding | Status | Classification | Violated obligation | Decision-complete correction and closure proof |
|---|---|---|---|---|
| `FIND-TASK-002-16` | CONFIRMED from `REPO-R8-1` | VIOLATION | Materially modified Rust items require accurate rustdoc describing their actual role and operation | Delete the stale argument-parsing sentence from `print_tokens`; preserve its accurate output documentation and body. Inspect the result and run format, lints, and cumulative diff hygiene. |
| `FIND-TASK-002-17` | REVISED from `SEC-R8-001` | INCORRECT | REQ-007 full ID-token claim verification, REQ-008/INV-002 verified external identity, and fail-closed invalid-claim handling | In the existing OIDC-specific verifier owner, reject a raw `sub` that is empty, non-ASCII, or longer than 255 bytes; preserve generic workload semantics and downstream behavior. Extend the existing focused verifier test with invalid and valid subject cases, then run the focused selector and required broader auth checks. |

No finding requires a new public contract, dependency, persistence object,
security decision, or architecture revision.

## Prior-finding closure

`FIND-TASK-002-1` through `FIND-TASK-002-15` remain **CLOSED**. Wave 2
revalidated exact subject-path selection, `azp`, role-change audit, advertised
algorithms, RLS state transitions, owner-bound lookup, secret redaction,
documentation/import repairs, mandatory binding/time claims, replay and
rotation serialization, refresh lookup documentation, administrative
revocation versus rotation, and administrative revocation versus initial
issuance. `FIND-TASK-002-16` is a distinct later stale CLI rustdoc sentence;
`FIND-TASK-002-17` is a distinct OIDC Subject Identifier syntax gap.

## Verification limits

- No `.codegraph/` index exists, so reviewers used Git, `rg`, and direct
  full-body/caller inspection.
- Fresh review evidence passed the exact R7 two-ordering Postgres proof,
  repository-managed migration setup, and cumulative `git diff --check`.
- Recorded candidate evidence covers principals unit/integration, SQL, all 27
  identity journeys, tenant isolation, codegen, docs, format, and lints. The
  bounded review did not rerun all broad lanes.
- No current test exercises OIDC Subject Identifier syntax. Existing tests
  cover claim presence/path selection and other binding/time claims.
- Every required reviewer completed within the 20-minute limit; no required
  report is missing.
- TASK-003 BFF redemption, TASK-004 CLI persistence, and live Okta/Entra
  qualification remain intentional downstream or change-level work.

## Remediation

Implement `TASK-002-R8-oidc-subject-and-rustdoc-corrections.md` through
`$wyrd-implement`, then review the complete original base-to-new-candidate
range.
