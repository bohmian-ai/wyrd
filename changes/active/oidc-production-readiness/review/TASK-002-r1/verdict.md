# TASK-002 Review Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `87de451ed87ad059cefd579eb15ef4b028a92547`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Reviewed range: complete base-to-candidate diff, 46 files, 3,728 insertions and 1,663 deletions

## Verdict

**FIX_REQUIRED**

The candidate establishes the intended state-bound tenant callback, screened provider exchange, one-use completion, connection-bound renewal, replacement cutoff, machine-path separation, generated contract updates, and real-server journeys. Seven independently validated, bounded gaps remain. None requires a specification revision.

## Acceptance matrix

| Obligation | Result | Evidence and remaining gap |
|---|---|---|
| REQ-006; tenant route is untrusted routing context and callback derives tenant/connection only from state | PASS | Begin login and callback use the route resolver and one-use state; callback accepts no tenant or host selector. |
| REQ-007; PKCE/state/nonce, exact redirect, bounded state, and complete token trust validation | FAIL | State, PKCE, nonce, issuer, signature, time, scalar audience, screened IO, and replay controls exist. Multi-audience `azp` semantics and discovered ID-token algorithm pinning are absent (`FIND-TASK-002-2`, `FIND-TASK-002-4`). |
| REQ-008; tenant User identity is verified `(issuer, subject)` with groups mapped only to tenant roles | FAIL | Role mapping and zero-default grants exist, but human connection configuration can substitute email or another mutable claim for OIDC `sub` (`FIND-TASK-002-1`). |
| REQ-013; machine authentication remains independent | PASS | API-key and workload assertion arms remain separate and have a real-server journey. |
| REQ-014; provider replacement does not link or transfer authority | FAIL | Replacement cutoff works, but a configurable non-`sub` human subject can link same-issuer identities (`FIND-TASK-002-1`). |
| REQ-015; tenant membership and authority remain isolated | PASS | State resolves one tenant, subsequent work uses tenant RLS, and same-issuer cross-tenant refusal is covered. |
| REQ-016; old connection stops login/renewal and access snapshots remain bounded | PASS | Callback and refresh recheck exact connection id/revision; successors preserve provenance and access TTL remains five minutes. |
| REQ-017; login outcomes and role changes have canonical redacted audit, with required audit failure closed | FAIL | Token exchange and refusal evidence exist, but provider-driven durable role replacement has no distinct canonical role-change evidence (`FIND-TASK-002-3`). |
| INV-001; untrusted paths, headers, browser state, email, and provider bytes cannot select tenant/connection | PASS | Tenant and connection selection are server-bound and header-free. |
| INV-002; external identity is `(issuer, subject)` and email is never linking authority | FAIL | Human subject mapping is not constrained to OIDC `sub` (`FIND-TASK-002-1`). |
| INV-003; platform, tenant-user, and workload planes remain distinct | PASS | Tenant callback always issues User authority and machine/platform paths remain separate. |
| INV-004; trust, SSRF, replay, audit, and RLS fail closed | FAIL | Core controls exist, but `azp` and advertised-algorithm checks are incomplete (`FIND-TASK-002-2`, `FIND-TASK-002-4`). |
| AC-002 task slice; controlled provider login maps authority and proves allow/deny | PASS | `tenant_human_login_journey`. |
| AC-003 task slice; tenant-separated provider state and wrong-tenant refusal | PASS | Callback refusal and same-issuer two-tenant journeys. |
| AC-005 task slice; machine paths continue with exact binding | PASS | `tenant_machine_independence_journey`. |
| AC-006; replacement creates a separate User without email linking or inherited authority | FAIL | Existing journey uses `subject: sub` and does not close configurable same-issuer claim aliasing (`FIND-TASK-002-1`). |
| AC-007 task slice; trust failures, mapping changes, audit failure, and renewal cutoff | FAIL | Existing evidence omits multi-audience `azp`, provider-advertised asymmetric algorithms, and role-change audit (`FIND-TASK-002-2` through `-4`). |
| Packet login/renewal contract and retirement of public authorization-code exchange | PASS | One binding, 256-bit state, consume-before-IO, sealed completion, fixed response, refresh provenance, legacy-row revocation, and legacy grant/route/client removal are present. |
| Repository SQL capability and RLS rules | FAIL | Tenant queries duplicate RLS predicates and the state resolver exposes raw `PgPool` (`FIND-TASK-002-5`, `FIND-TASK-002-6`). |
| Repository secret-handling rule | FAIL | Public durable state values derive `Debug` over a plain PKCE verifier (`FIND-TASK-002-7`). |
| Non-goals and scope | FAIL only for email-linking prohibition | No provider token bearer authority, platform fallback, instant-revocation promise, compatibility route, or new machine identity model was introduced; configurable human subject mapping violates the email-linking non-goal. |

## Wave 1 results

| Review | Result | Proposed findings |
|---|---|---|
| Task implementation | FAIL | `TR-001`, `TR-002` |
| Repository standards | FAIL | `STD-001`, `STD-002`, `STD-003` |
| Security/auth domain | FAIL | `SEC-001`, `SEC-002` |
| Tenancy/data domain | FAIL | `TD-001`, `TD-002` |

## Validated finding ledger

| Finding | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-002-1` | REVISED | INCORRECT | Human connections use exact OIDC `sub`, including fail-closed handling of stored non-`sub` mappings. |
| `FIND-TASK-002-2` | REVISED | INCORRECT | Human ID tokens enforce OIDC authorized-party semantics without changing workload JWT rules. |
| `FIND-TASK-002-3` | REVISED | MISSING | Changed provider-driven role assignments append canonical role-sync evidence in the issuance transaction. |
| `FIND-TASK-002-4` | CONFIRMED | INCORRECT | Tenant callback accepts only provider-advertised supported asymmetric ID-token algorithms. |
| `FIND-TASK-002-5` | CONFIRMED | VIOLATION | TenantConn login-state transitions rely on forced RLS instead of duplicate tenant predicates. |
| `FIND-TASK-002-6` | REVISED | VIOLATION | State-to-tenant lookup is a narrow inherent `WyrdPostgres` operation; no raw pool crosses the library API. |
| `FIND-TASK-002-7` | CONFIRMED | VIOLATION | PKCE verifier remains secret/redacted across public state values. |

The complete independently validated evidence, caller traces, preserved behavior, and closure proofs are in `findings-validation.md`.

## Prior-finding closure

This is the first review attempt for TASK-002. There are no prior stable findings to close or preserve.

## Verification limits

- Reviewers inspected the complete immutable diff and the relevant complete source bodies and callers. They did not rerun Cargo, Postgres, or IdP lanes during the review.
- The candidate records successful focused identity journeys, the 27-test identity lane, focused auth/server/spec/migration tests, principals integration/unit, SQL, codegen, docs, tenant-isolation, client-tier, format, lint, and `git diff --check`.
- Those recorded lanes do not directly exercise the seven retained gaps, and the recorded evidence omits `mise run check:from-pools-allowlist`.
- TASK-003's BFF completion route and TASK-004's CLI handoff persistence remain intentionally outside this task.
- No live Okta or Entra qualification was expected from TASK-002.

## Remediation

Implement `TASK-002-R1-tenant-login-corrections.md` through `$wyrd-implement`, then review the complete original base-to-new-candidate range against this task and the approved specification.
