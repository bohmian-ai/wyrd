# TASK-002 R9 Review Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `fa2bda92a7e79471b79b607870c1e86a9f35639c`
- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation inputs: TASK-002 R1 through R8 and their prior verdicts and
  validated finding ledgers

The candidate remained the named commit throughout both review waves. The R9
artifacts are review output outside the immutable committed subject.

## Verdict

**FIX_REQUIRED**

The cumulative candidate closes `FIND-TASK-002-1` through
`FIND-TASK-002-17`, but five bounded defects remain. Concurrent callbacks for
one User can union disjoint provider-derived roles before issuance; the token
schema falsely promises refresh tokens for API-key exchange; the changed token
write handler lacks scrubbed instrumentation; the changed auth router retains
placeholder rustdoc without its panic contract; and begin-login propagates a
raw application pool instead of using a narrow `WyrdPostgres` capability.
Each correction fits an existing owner and requires no specification revision.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-006–007 and INV-001/004: header-free tenant routing, one-use state, PKCE/nonce/redirect binding, screened provider IO, and fail-closed ID-token verification | Tenant is recovered only from stored state; provider IO is screened and pinned; signature, issuer, audience, algorithm, time, nonce, `azp`, and Subject Identifier checks precede persistence; focused and journey evidence is green | PASS |
| REQ-008 and INV-002/003: `(issuer, sub)` identity and only roles mapped from the verified callback groups | Identity and sequential role replacement are correct, but two callbacks can write disjoint rows before the family lock and the later issuer can mint their union (`FIND-TASK-002-18`) | **FAIL** |
| REQ-013 and machine/human authority separation | Runtime API-key and workload issuance remains machine-only and returns no refresh token, but the source-generated public token schema says API-key exchange does return one (`FIND-TASK-002-19`) | **FAIL** |
| REQ-014–016: provider replacement, tenant separation, connection cutoff, refresh containment, and revocation serialization | Connection revision rechecks, forced RLS, family-before-connection locking, replay containment, and deterministic R4/R6/R7 overlap proofs remain present | PASS |
| REQ-017: role, issuance, refresh, containment, and revocation audit remains canonical and transactionally coupled | Successful and failed flows use the canonical audit path; failure rolls back the establishing transaction | PASS |
| Server write-handler tracing and secret scrubbing | The materially changed `POST /auth/token` handler lacks the required scrubbed handler span (`FIND-TASK-002-20`) | **FAIL** |
| Materially changed Rust items have substantive rustdoc and panic contracts | `auth_router` still says only “Build auth routes.” and omits the static governor `expect` panic contract (`FIND-TASK-002-21`) | **FAIL** |
| SQL capability and tenant-boundary rules | Tenant work otherwise uses `TenantConn`/RLS, but begin-login passes `app_pool()` to a public raw-`PgPool` resolver instead of keeping role selection behind `WyrdPostgres` (`FIND-TASK-002-22`) | **FAIL** |
| Public callback/token contracts, retired authorization-code grant, generated artifacts, docs, and explicit non-goals | Token-bearing callback and legacy grant/CLI path remain retired; no email linking, provider bearer authority, platform fallback, TASK-003 BFF route, TASK-004 handoff, or new machine identity model entered the candidate | PASS except the field-description mismatch in `FIND-TASK-002-19` |
| Prior remediations and stable findings `FIND-TASK-002-1` through `-17` | Wave 2 independently revalidated every prior correction against current source and callers | PASS |

## Wave 1 results

| Review | Result | Proposed findings |
|---|---|---|
| Task implementation | FAIL | `TASK-REV-001`, `TASK-REV-002` |
| Repository standards | FAIL | `REPO-001`, `REPO-002` |
| Security/OIDC domain | PASS | Empty ledger |
| Tenancy/persistence/concurrency domain | FAIL | `TD-001`, `TD-002` |

Wave 2 traced the complete correction owners and callers, deduplicated
`TASK-REV-001` with `TD-001`, confirmed the other four proposals, revised the
minimum boundary for `TD-002`, and rejected the security report's overall
empty conclusion only for the already-retained role-expansion race. It found
no additional security defect.

## Validated finding ledger

| Finding | Status | Classification | Violated obligation | Minimum correction and closure proof |
|---|---|---|---|---|
| `FIND-TASK-002-18` | CONFIRMED from `TASK-REV-001`, `TD-001` | INCORRECT | REQ-008, INV-003, and the task's atomic role/issuance boundary | Take the existing User refresh-family lock after canonical identity resolution and before role replacement; preserve the existing transaction, audit, issuance, completion, and lock order. Add deterministic two-callback Postgres proof for both serialized orderings and exact roles. |
| `FIND-TASK-002-19` | CONFIRMED from `TASK-REV-002` | INCORRECT | REQ-013 and public contract/runtime agreement | Correct the one source field description to human OIDC login/refresh only, regenerate both schema trees, and run codegen plus the existing machine proof. |
| `FIND-TASK-002-20` | CONFIRMED from `REPO-001` | VIOLATION | Required scrubbed tracing on write handlers | Add the local `#[tracing::instrument(level = "debug", skip_all)]` pattern to `token`; verify format/lints and existing route lanes. |
| `FIND-TASK-002-21` | CONFIRMED from `REPO-002` | VIOLATION | Required substantive rustdoc and `# Panics` contract | Document the four auth surfaces, shared governor, and invalid-static-config panic; preserve the implementation. |
| `FIND-TASK-002-22` | REVISED from `TD-002` | VIOLATION | Raw pools may not cross library workflow boundaries; app-role selection stays behind `WyrdPostgres` | Add a narrow inherent slug-resolution capability that delegates to the existing resolver with the private app pool; call it from begin-login and leave unrelated callers alone. Run pool/tenant boundary checks and login proofs. |

No retained finding requires a new public contract, dependency, persistence
model, isolation mode, security decision, or architecture revision.

## Prior-finding closure

`FIND-TASK-002-1` through `FIND-TASK-002-17` remain **CLOSED**. Wave 2
revalidated exact `sub` selection and syntax, authorized-party and advertised
algorithm enforcement, role-change audit, RLS login state, owner-bound state
lookup, PKCE redaction, Rust documentation/import corrections, ID-token
binding/time claims, replay/rotation serialization, refresh lookup rustdoc,
administrative revocation versus rotation and initial issuance, and the CLI
printer documentation. `FIND-TASK-002-18` is a distinct missing serialization
boundary before provider-role replacement; findings 19–22 concern separately
modified public documentation, server tracing/rustdoc, and route-slug lookup.

## Verification limits

- No `.codegraph/` index exists, so reviewers used Git, `rg`, and complete
  direct source/caller inspection.
- `git diff --check` passed for the immutable range and `HEAD` remained the
  candidate after both waves.
- The task reviewer reran the focused R8 verifier test successfully. Other
  reviewers kept the shared checkout free of overlapping Cargo/Postgres work.
- Recorded candidate evidence covers the R1–R8 focused proofs,
  `test:principals:unit`, `test:principals:integration`, `test:sql`, all 27
  identity journeys, codegen, docs, tenant/pool/client boundary checks,
  formatting, lints, and diff hygiene.
- No existing proof overlaps two callbacks for one established User with
  disjoint mapped roles. That deterministic Postgres proof is required to
  close `FIND-TASK-002-18`.
- Live provider qualification, TASK-003 BFF completion, and TASK-004 CLI
  persistence remain outside this bounded task review.

## Remediation

Implement `TASK-002-R9-tenant-login-final-corrections.md` through
`$wyrd-implement`, then review the complete original base-to-new-candidate
range.
