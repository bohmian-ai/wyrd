# TASK-002 Review Verdict — R2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `8b201627c0a957dccf46649d00c8c205689bc5de`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Prior remediation: `changes/active/oidc-production-readiness/review/TASK-002-r1/TASK-002-R1-tenant-login-corrections.md`
- Reviewed range: complete cumulative base-to-candidate diff, 59 files, 5,458 insertions and 1,688 deletions

The candidate remained `8b201627c0a957dccf46649d00c8c205689bc5de` through both review waves. Review artifacts are outside that immutable subject.

## Verdict

**FIX_REQUIRED**

The tenant-login behavior, security boundary, tenancy and persistence model, contracts, generated artifacts, and required journeys pass. All seven prior findings are closed. The candidate still violates two mandatory repository source rules: required Rust-item documentation is incomplete, and three new imports are inside non-generic function bodies. Both corrections are bounded, behavior-preserving, and require no specification revision.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-006/INV-001: routing context cannot become authority; callback derives tenant and connection from one-use state | `HumanConnections::begin_login`, `AuthorizationCodeExchange`, narrow `WyrdPostgres::login_state_tenant`; header-hostility, refusal, and OpenAPI proofs | PASS |
| REQ-007/INV-004: PKCE/state/nonce, exact redirect, issuer/audience/`azp`/algorithm/signature/time checks, screened provider IO, replay refusal | Callback/verifier owners; focused `azp` and algorithm tests; refusal journey; screened-IO tests | PASS |
| REQ-008/014/INV-002: tenant User identity is exact `(issuer, sub)`, never email-linked; mapped tenant roles only | Input and stored-row `sub` enforcement; distinct-subject and provider-switch proofs | PASS |
| REQ-013/INV-003: machine and human authority remain separate | Existing API-key/workload paths; `tenant_machine_independence_journey` | PASS |
| REQ-015: same-issuer and state routing remain tenant isolated | Forced RLS transitions, least-disclosure lookup, same-issuer and Postgres isolation proofs | PASS |
| REQ-016: replacement stops renewal, successors retain provenance, access remains bounded | Active connection/revision locking and refresh provenance; switch/cutoff/migration proofs | PASS |
| REQ-017: role changes and issuance use canonical transactional audit | `auth.user.roles.sync`, shared issuance transaction, changed/unchanged and audit-failure rollback tests | PASS |
| Packet-local state/completion contract and authorization-code retirement | Consume-before-IO, sealed one-use completion, fixed callback output; legacy grant/client/CLI route removal; schema/OpenAPI/docs proof | PASS |
| Prior remediation findings 1–7 | Independently validated closure in task, security, tenancy, and Wave 2 reports | PASS |
| Repository Rust documentation rule | Missing required rustdoc and required `# Errors`/`# Panics` at the exact locations in `FIND-TASK-002-8` | FAIL |
| Repository module-top import rule | Three new function-scoped imports fall outside both permitted exceptions | FAIL |
| Non-goals and scope | No compatibility route, new dependency, alternate audit path, public API expansion, or TASK-003/TASK-004 behavior entered the implementation | PASS |

The complete requirement-level matrix is preserved in `task-review.md`; the authority-level matrix is preserved in `standards-review.md`.

## Wave 1 results

| Review | Result | Findings |
|---|---|---|
| Task implementation | PASS | None; prior findings 1–7 closed |
| Repository standards | FAIL | `STD-001`, `STD-002` |
| Security/RBAC domain | PASS | None; prior security findings closed |
| Tenancy/persistence domain | PASS | None; prior data findings closed |

## Validated finding ledger

| Finding | Status | Classification | Violated obligation | Correction and closure proof |
|---|---|---|---|---|
| `FIND-TASK-002-8` | REVISED from `STD-001` | VIOLATION | Mandatory rustdoc for new/materially changed Rust items, including `# Errors` and `# Panics` where applicable | Document only the exact trait methods, SQL constants, signing helper, and local alias named in `findings-validation.md`; run format and lints and inspect the cited source |
| `FIND-TASK-002-9` | CONFIRMED from `STD-002` | VIOLATION | Imports must live in the module import block except the two documented exceptions | Move the SHA-256, Utoipa, and Wiremock imports to their module import blocks without changing bodies; run format, lints, and codegen check |

Full evidence, caller tracing, exact locations, and correction boundaries are in `findings-validation.md`. No Wave 1 finding was omitted or softened: the empty task and domain ledgers were independently validated, and both standards findings were retained.

## Prior-finding closure

| Prior finding | Result |
|---|---|
| `FIND-TASK-002-1` — exact human `sub` identity | CLOSED |
| `FIND-TASK-002-2` — OIDC authorized-party validation | CLOSED |
| `FIND-TASK-002-3` — canonical role-change audit | CLOSED |
| `FIND-TASK-002-4` — provider-advertised ID-token algorithm | CLOSED |
| `FIND-TASK-002-5` — RLS-only tenant state selection | CLOSED |
| `FIND-TASK-002-6` — owner-bound state lookup | CLOSED |
| `FIND-TASK-002-7` — redacted PKCE verifier | CLOSED |

## Verification limits

- Reviewers inspected the complete cumulative diff and the relevant complete bodies, callers, authorities, migrations, contracts, and tests. The repository has no `.codegraph/` index, so Git and source inspection were used.
- Fresh review checks passed for the focused contract/SQL remediation tests, eight security-remediation tests, `tenant_machine_independence_journey`, `check:from-pools-allowlist`, `check:tenant-isolation`, `codegen:check`, and cumulative `git diff --check`.
- The candidate records green results for all four identity journeys, the full 27-test identity lane, principals unit/integration, SQL, client-tier, format, lints, docs, codegen, and focused Postgres tests. Not every expensive lane was rerun during this bounded review.
- No required reviewer timed out or was unavailable. TASK-003's BFF completion route and TASK-004's CLI handoff persistence remain intentional non-goals.

## Remediation

Implement `TASK-002-R2-repository-rule-corrections.md` through `$wyrd-implement`, then reassess the complete original base-to-new-candidate range.
