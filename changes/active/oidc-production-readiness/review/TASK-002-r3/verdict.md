# TASK-002 Review Verdict — R3

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `d861845f3f5d89aca413857dcfb8c1bbfaee349d`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Prior remediation tasks: `TASK-002-R1-tenant-login-corrections.md` and
  `TASK-002-R2-repository-rule-corrections.md`
- Reviewed range: complete cumulative base-to-candidate diff, 68 files,
  6,274 insertions and 1,713 deletions

The candidate remained `d861845f3f5d89aca413857dcfb8c1bbfaee349d`
through both review waves. Review artifacts are outside the immutable subject.
The lead-directed reuse cleanups recorded in the R2 evidence table and the two
unrelated `test:wyrd` flaky-test repairs were treated as authorized work, not
scope drift, and were still audited for regressions and repository compliance.

## Verdict

**FIX_REQUIRED**

The tenant-login implementation, security boundary, tenancy and persistence
model, contracts, generated artifacts, journeys, and all nine prior findings
pass. One bounded repository-rule violation remains after an authorized reuse
cleanup: `verify_id_token_algorithm` no longer performs the redundant HMAC
filter that its rustdoc still claims it performs. The composed runtime path
remains fail-closed because its sole caller immediately invokes the shared
`ExternalVerifier`, which rejects symmetric algorithms before key lookup. The
minimum correction is rustdoc-only and requires no specification revision.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-006/INV-001: route context cannot become authority; callback tenant and connection come only from one-use state | `HumanConnections::begin_login`, `AuthorizationCodeExchange`, narrow `WyrdPostgres::login_state_tenant`; header-hostility, refusal, and OpenAPI proofs | PASS |
| REQ-007/INV-004: PKCE/state/nonce, exact redirect, issuer/audience/`azp`/algorithm/signature/time checks, screened provider IO, and replay refusal | Callback and shared-verifier owners; focused `azp`, advertised-algorithm, PKCE-redaction, and refusal evidence | PASS |
| REQ-008/REQ-014/INV-002: exact `(issuer, sub)` tenant User identity, no email linking, and mapped tenant roles only | Exact-`sub` validation and stored-row decode; distinct-subject and provider-switch proofs | PASS |
| REQ-013/INV-003: machine and human authority remain separate | Existing API-key/workload paths and `tenant_machine_independence_journey` | PASS |
| REQ-015: same-issuer and callback routing remain tenant isolated | Forced-RLS transitions, least-disclosure lookup, same-issuer and Postgres isolation proofs | PASS |
| REQ-016: replacement stops renewal, refresh provenance is exact, and issued access remains bounded | Active-connection locking/rechecks, refresh provenance, switch/cutoff/migration proofs | PASS |
| REQ-017: role changes and issuance use canonical transactional audit | Shared `principal_event` construction preserves canonical role-sync evidence; changed/unchanged and rollback proofs | PASS |
| Packet-local login/completion contract and authorization-code retirement | Consume-before-provider-IO, sealed one-use completion, fixed callback output, retired legacy grant/client/CLI route, schemas/OpenAPI/docs | PASS |
| Prior findings `FIND-TASK-002-1` through `FIND-TASK-002-9` | Independently validated closure in task, standards, security, tenancy, and Wave 2 reports | PASS |
| Lead-authorized reuse cleanups | Audit helper reuse, shared-verifier HMAC ownership, callback test reuse, and sealing-key runbook clarification preserve behavior | PASS |
| Lead-authorized flaky-test repairs | Gateway JSON comparison and Forge boot-pass ordering retain their assertions and touch no product behavior | PASS |
| Accurate rustdoc for materially modified Rust items | `verify_id_token_algorithm` documents an asymmetric filter its body no longer owns; `FIND-TASK-002-10` | FAIL |
| Non-goals and scope | No compatibility route, alternate audit path, new dependency, public API expansion, TASK-003 BFF flow, or TASK-004 CLI handoff entered the implementation | PASS |

The complete requirement-level matrix is preserved in `task-review.md`; the
authority-level matrix is preserved in `standards-review.md`.

## Wave 1 results

| Review | Result | Findings |
|---|---|---|
| Task implementation | PASS | None; prior findings 1–9 closed |
| Repository standards | FAIL | `STD-R3-001` |
| Security/RBAC domain | PASS | None |
| Tenancy/persistence domain | PASS | None |

## Validated finding ledger

| Finding | Status | Classification | Violated obligation | Correction and closure proof |
|---|---|---|---|---|
| `FIND-TASK-002-10` | CONFIRMED from `STD-R3-001` | VIOLATION | Accurate substantive rustdoc and accurate `# Errors` for materially modified fallible Rust items | Update only `verify_id_token_algorithm`'s rustdoc to describe advertised-set membership and name the shared verifier as owner of symmetric-algorithm rejection; inspect both unchanged bodies, then run `mise run fmt`, `mise run lints`, and `git diff --check` |

Full evidence, caller tracing, the Ponytail ladder, and the exact correction
boundary are in `findings-validation.md`. The task, security, and tenancy empty
finding ledgers were independently validated.

## Prior-finding closure

| Prior finding | Result |
|---|---|
| `FIND-TASK-002-1` — exact human `sub` identity | CLOSED |
| `FIND-TASK-002-2` — OIDC authorized-party validation | CLOSED |
| `FIND-TASK-002-3` — canonical role-change audit | CLOSED |
| `FIND-TASK-002-4` — advertised algorithm plus shared asymmetric verification | CLOSED |
| `FIND-TASK-002-5` — RLS-only tenant state selection | CLOSED |
| `FIND-TASK-002-6` — owner-bound state lookup | CLOSED |
| `FIND-TASK-002-7` — redacted PKCE verifier | CLOSED |
| `FIND-TASK-002-8` — required Rust-item documentation | CLOSED |
| `FIND-TASK-002-9` — module-top imports | CLOSED |

`FIND-TASK-002-10` is a new source-contract violation introduced by the later
authorized cleanup. It does not reopen the runtime closure of
`FIND-TASK-002-4`.

## Verification limits

- Reviewers inspected the complete cumulative diff, applicable authorities,
  prior findings, relevant complete bodies and callers, migrations, contracts,
  journeys, and every post-R2 executable change. The repository has no
  `.codegraph/` index, so Git and source inspection were used.
- Fresh review checks passed for exact-`sub` validation, retired authorization
  code exchange, PKCE Debug redaction, `azp` enforcement, pool construction
  boundaries, tenant isolation, code generation, and cumulative diff hygiene.
- The candidate records green results for all four required identity journeys,
  the full 27-test identity lane, principals unit/integration, SQL, client-tier,
  docs, codegen, format, lints, and final `test:wyrd` with 2,213 passing tests.
  Reviewers did not rerun every expensive Docker, Postgres, IdP, or broad lane
  during this bounded review.
- No reviewer exceeded the 20-minute limit, and no required report, authority,
  source, or caller trace was unavailable.
- TASK-003's BFF completion route and TASK-004's CLI handoff persistence remain
  intentional non-goals.

## Remediation

Implement `TASK-002-R3-algorithm-helper-rustdoc-correction.md` through
`$wyrd-implement`, then reassess the complete original base-to-new-candidate
range.
