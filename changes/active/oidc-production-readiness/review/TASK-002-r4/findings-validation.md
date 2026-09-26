# TASK-002-r4 findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `63e6a545db156a095b670a5bb8bc36f6f36fba32`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Prior remediation: TASK-002 R1, R2, and R3

The checked-out `HEAD` equaled the candidate before and after validation. The
repository has no `.codegraph/` index, so source and caller tracing used Git and
`rg`. The complete cumulative base-to-candidate diff contains 75 files, 7,185
insertions, and 1,713 deletions. Lead-directed reuse and test commits recorded
in the task evidence were treated as authorized scope and checked for
regression.

## Wave 1 finding dispositions

| Wave 1 report | Proposed finding | Disposition | Final finding | Validation |
|---|---|---|---|---|
| `task-review.md` | None | **EMPTY LEDGER REJECTED** | — | Its acceptance matrix accurately closes the ten prior findings and most task behavior, but it misses the reachable ID-token claim gap and refresh-family overlap retained below. |
| `standards-review.md` | None | **EMPTY LEDGER VALIDATED** | None | Independent inspection found no additional repository-structure, ownership, documentation, import, generated-artifact, or test-placement violation. Its empty standards ledger remains empty; the retained defects are behavioral security/concurrency findings. |
| `domain-review-security.md` | `SEC-R4-001` | **REVISED** | `FIND-TASK-002-11` | Missing `iss`, `aud`, and `iat`, future `iat`, and present future `nbf` are accepted. The correction must distinguish OIDC ID tokens from workload assertions because the shared verifier serves both. |
| `domain-review-tenancy-data.md` | `TD-R4-001` | **REVISED** | `FIND-TASK-002-12` | The family race is reachable, but not when two callers present the same current row: that row lock serializes them. It occurs when replay targets an older row while legitimate rotation holds the current row and inserts a successor. |

The standards empty ledger is independently validated. The task empty ledger
is not retained because the two domain findings directly violate obligations
mapped by the task. No additional finding was produced from the complete diff.

## Independent validation

### SEC-R4-001 caller and dependency trace

`ExternalVerifier::verify_external_against` has three production entry paths:

1. `ExternalVerifier::verify_external` resolves a tenant issuer and is called
   by `JwtBearer::verify_workload_assertion` for workload federation.
2. `AuthorizationCodeExchange::finish_id_token_exchange` calls it for tenant
   human OIDC, then checks nonce and `azp`, revalidates the connection, creates
   or resolves the User, replaces roles, audits, issues, seals, and commits.
3. `PlatformLogin::complete` calls it for the deployment platform OIDC login,
   then checks nonce and resolves a pre-registered platform principal.

The complete shared body constructs `jsonwebtoken::Validation::new`, sets the
configured issuer and audience, and sets leeway. Locked dependency
`jsonwebtoken 9.3.1` defaults `required_spec_claims` to only `exp`, defaults
`validate_nbf` to false, and checks configured issuer/audience only when those
claims are present. Its supported required-claim set does not include `iat`.
`map_claims`, tenant nonce/`azp`, and platform nonce checks do not restore any
of these missing time or binding checks. Thus a correctly signed token without
`iss` or `aud` reaches identity mapping, and tenant/platform ID tokens without
`iat`, with future `iat`, or with a present future `nbf` can reach issuance.

REQ-007 explicitly requires issuer, audience, time, and claims verification,
and INV-004 requires fail-closed federation. The gap is reachable before any
tenant identity or session mutation and needs no speculative attacker
capability beyond a signing key already trusted for another token shape.

The Wave 1 correction is too broad if it simply makes `iat` mandatory inside
the existing generic method: the same method verifies workload assertions,
where the current contract is not the OIDC ID-token contract. The minimum safe
shape is:

- keep one shared signature/JWKS implementation;
- in its existing generic validation, require `exp`, `iss`, and `aud` and set
  `validate_nbf = true` so a present future `nbf` is refused;
- add one explicit OIDC-ID-token entry method on `ExternalVerifier` that reuses
  `verify_external_against` and then requires numeric `iat` not later than the
  current time plus the existing configured skew;
- route the two real ID-token callers—tenant callback and platform login—through
  that method, while leaving workload `verify_external` on the generic path.

This is one earned two-caller semantic boundary, not a second verifier. It uses
the installed JWT validator plus `SystemTime`; no dependency, policy knob,
claim DTO, or caller-local duplicate validation is justified.

### TD-R4-001 caller and transaction trace

`RefreshTokens::execute` has one production caller, the refresh-token arm of
`POST /auth/token`. The route opens one `TenantConn`, calls `execute`, commits
success, and also commits `RefreshError::Reused` so family revocation and its
audit survive the refused request. The complete rotation body calls
`consume_active_refresh`, then `TenantTokenIssuer::issue_human_session`, which
takes the human-connection slot lock, rechecks the connection, mints authority,
and inserts the successor. The stale branch calls `refresh_by_hash`,
`revoke_refresh_family`, and the canonical audit append. No operation shared by
both branches locks `(tenant, principal_kind, principal_id)`.

Two callers presenting the same current token do not demonstrate the defect:
both update the same row, so PostgreSQL makes the loser wait for the winner's
transaction. After the winner commits, the loser's later statements see the
successor and revoke it. Existing tests prove only that serialization.

The reachable failing interleaving uses different rows in one family:

1. Transaction A consumes current token `B`, holding `B`'s row lock, and begins
   minting successor `C` in the same transaction.
2. Transaction R replays already-rotated ancestor `A`; its consume and stale
   lookup do not conflict with `B`.
3. R's family-wide `UPDATE` finds active `B` in its statement snapshot and
   waits on A's row lock.
4. A commits revoked `B` and newly inserted active `C`. PostgreSQL rechecks `B`,
   which no longer matches `revoked_at IS NULL`; `C` was not in R's statement
   snapshot and is not added to its target set.
5. R commits `RefreshError::Reused` and the containment audit while `C` remains
   active.

That contradicts the task's explicit requirement to lock the token family and
preserve replay-family revocation, plus the security posture's statement that
reuse revokes the family.

The minimum correction uses PostgreSQL's existing transaction-lock pattern,
not a new table, family identifier, lease, mutex, or isolation-level change:

- look up the presented row under the existing `TenantConn`/forced-RLS boundary
  solely to derive its stored principal family; unknown hashes still return
  `NotFound`;
- acquire one transaction-scoped advisory lock keyed by tenant,
  `principal_kind`, and `principal_id`;
- after the lock, re-read/consume the token and keep the lock through successor
  insertion or family revocation, audit, and the caller-owned commit;
- retain the existing human-connection slot lock after the family lock, giving
  every refresh rotation the fixed order family then connection.

One SQL lock helper in the existing refresh-token query module is sufficient.
Do not add a repository, trait, lock service, family table, retry loop, or
serializable transaction.

## Final deduplicated finding ledger

### FIND-TASK-002-11 — Tenant ID-token verification accepts missing binding/time claims

- **Wave 1 source:** `SEC-R4-001`
- **Status:** REVISED
- **Classification:** INCORRECT
- **Violated obligation:** SPEC REQ-007, INV-004, the task's full ID-token
  verification requirement, and the security posture's explicit configured
  issuer/audience and fail-closed federation rules.
- **Exact location:**
  `crates/shared/wyrd-auth-verify/src/lib.rs:506-550`, tenant caller
  `crates/wyrd/wyrd-auth/src/callback.rs:201-220`, and platform ID-token caller
  `crates/wyrd/wyrd-auth/src/platform_login.rs:234-314`.
- **Evidence:** locked `jsonwebtoken 9.3.1` requires only `exp` by default,
  ignores missing configured `iss`/`aud`, does not validate `nbf` unless enabled,
  and has no `iat` validation. The later claim mapping, nonce, and `azp` checks
  do not close those omissions.
- **Observable consequence:** a correctly signed tenant ID token can establish
  tenant authority without the required issuer/audience claim binding or OIDC
  issued-at/time contract.
- **Decision-complete correction:** require `exp`/`iss`/`aud` and enable
  optional-`nbf` validation in the existing generic verifier; add one
  `ExternalVerifier` ID-token method that reuses that verifier and requires a
  numeric, non-future `iat` under the existing skew; use it from both tenant and
  platform OIDC callers, leaving workload assertions on the generic method.
  Update rustdoc/error mapping only as required by those exact behaviors.
- **Focused closure proof:** add focused verifier cases for absent `iss`, absent
  `aud`, future `nbf`, absent/non-numeric `iat`, and future `iat`; extend the
  existing callback-refusal journey with at least the missing-`iat` signed-token
  case and assert no User, roles, refresh row, or completion is committed. Run
  the exact focused tests through `mise exec --`, the filtered
  `tenant_callback_refusal_journey` wrapper, `mise run fmt`, and `mise run
  lints`.
- **Specification decision:** no revision required; this narrows behavior to
  the already approved contract.

### FIND-TASK-002-12 — Replay containment is not serialized with current-family rotation

- **Wave 1 source:** `TD-R4-001`
- **Status:** REVISED
- **Classification:** INCORRECT
- **Violated obligation:** the task's explicit family-lock requirement,
  INV-004 replay protection, and the security posture's promise that reuse of a
  rotated refresh token revokes its family.
- **Exact location:** `crates/wyrd/wyrd-auth/src/refresh.rs:82-226`,
  `crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs:79-162`, successor
  insertion at `crates/wyrd/wyrd-auth/src/issuance.rs:475-527`, and durable
  refusal commit at
  `crates/wyrd/wyrd-server/src/components/auth/routes.rs:219-257`.
- **Evidence:** active rotation and stale replay share no family lock. Row-level
  serialization covers duplicate presentation of one row, but not replay of an
  ancestor while a different current row is being consumed and its successor
  is invisible to the replay transaction's family-update snapshot.
- **Observable consequence:** the server can return and audit family
  containment while the concurrently inserted successor remains usable.
- **Decision-complete correction:** before mutation, derive the stored family
  owner under RLS, take one transaction advisory lock keyed by tenant/kind/id,
  then re-read and perform consume/rotate or revoke/audit while holding it
  through route commit. Preserve the connection-slot lock and use fixed order
  family then connection. Add only the lock helper and the minimum workflow
  reorder needed to use it.
- **Focused closure proof:** add one deterministic Postgres concurrency test
  that pauses legitimate rotation after consuming current `B`, overlaps replay
  of ancestor `A`, commits route-equivalent outcomes, and from a fresh
  transaction proves successor `C` is revoked, cannot rotate, and one canonical
  containment audit is durable. Run that exact test with the repository
  Postgres wrapper, then the narrow auth/SQL task, `mise run fmt`, and `mise run
  lints`.
- **Specification decision:** no revision required; the approved task already
  mandates this serialization and outcome.

## Prior-finding closure

| Prior finding | Independent result |
|---|---|
| `FIND-TASK-002-1` — exact human `sub` | CLOSED: request validation and stored-row decode require exact `sub`; email does not select identity. |
| `FIND-TASK-002-2` — OIDC `azp` | CLOSED: tenant callback enforces multi-audience and present-`azp` semantics. |
| `FIND-TASK-002-3` — provider role-change audit | CLOSED: changed roles append canonical evidence transactionally and audit failure rolls back. |
| `FIND-TASK-002-4` — advertised asymmetric algorithm | CLOSED: discovery membership plus the immediately following shared verifier rejects HMAC. |
| `FIND-TASK-002-5` — duplicate tenant predicates | CLOSED: login-state transitions rely on forced RLS. |
| `FIND-TASK-002-6` — raw pool lookup | CLOSED: state-owner lookup remains a narrow inherent `WyrdPostgres` capability. |
| `FIND-TASK-002-7` — printable PKCE verifier | CLOSED: the verifier remains `SecretString` with redaction proof. |
| `FIND-TASK-002-8` — incomplete rustdoc | CLOSED: the R2 inventory remains substantively documented. |
| `FIND-TASK-002-9` — function-scoped imports | CLOSED: the cited imports remain at module scope. |
| `FIND-TASK-002-10` — false algorithm-helper rustdoc | CLOSED: current rustdoc accurately describes advertised-set membership and shared HMAC ownership. |

Neither retained finding reopens a prior stable finding. `FIND-TASK-002-11`
concerns required binding/time claims after signature verification;
`FIND-TASK-002-12` concerns cross-row family serialization.

## Verification limits

- I inspected the approved spec/task, applicable authorities, all four R4 Wave
  1 reports, prior validation/verdict records, the complete cumulative changed
  file inventory, and the full bodies and production callers named above.
- I inspected locked `jsonwebtoken 9.3.1` source for required-claim, audience,
  issuer, expiry, and `nbf` semantics. No external source was needed.
- I did not run expensive Cargo, Postgres, Docker, Keycloak/Dex, or broad lanes.
  `git diff --check` passed for the immutable range. Existing recorded tests do
  not cover missing ID-token claims/future times or the ancestor-replay/current-
  rotation overlap.
- PostgreSQL concurrency was validated from the concrete statement and
  transaction ordering. The required deterministic Postgres test remains the
  closure proof, not a prerequisite for retaining the source-demonstrated gap.
- TASK-003 BFF completion and TASK-004 CLI handoff remain intentional non-goals.
  No required source, caller, dependency semantics, or authority was missing.

## Overall validation outcome

**VALIDATED WITH FINDINGS.** Retain `FIND-TASK-002-11` and
`FIND-TASK-002-12`; both are bounded corrections inside existing owners and
require no specification revision. The appropriate task verdict is
`FIX_REQUIRED`.
