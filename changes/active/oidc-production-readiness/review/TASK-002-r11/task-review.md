# TASK-002 R11 Task Implementation Review

## Immutable subject and scope

- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `bae424cc647dad4be80e7debec976d0b7b3e4cf8`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 5; original `tasks/TASK-002-tenant-login.md` (revision 4 historical); R10 rustdoc correction and human-directed drift packets.
- Reviewed the cumulative base-to-candidate diff. The `454bdb90ef8eedca4bbd4754e083d41f15d0c4ae..bae424cc647dad4be80e7debec976d0b7b3e4cf8` fix diff located the new keyless activation guard, shared JWT proof, lock ordering, completion rotation proof, and journey retry helper. `HEAD` remained the candidate during this review. No `.codegraph/` index exists.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-006, INV-001: route key is pre-login routing context; callback tenant comes only from one-use state | `HumanConnections::begin_login` resolves a typed route slug, stores hashed state; `AuthorizationCodeExchange::complete` consumes that state; `WyrdPostgres::login_state_tenant` returns only its tenant | `tenant_callback_refusal_journey`, login header tests, `pg_login_state` | PASS |
| REQ-007, INV-004: code/PKCE/nonce, exact callback, verified issuer/audience/signature/algorithm/time/claims, screened provider IO, replay refusal | `wyrd-auth/src/{login,callback}.rs`, `ExternalVerifier::{verify_external_against,verify_id_token_against}`, screened discovery and callback exchange | refusal journey, focused OIDC claim/algorithm tests | PASS |
| REQ-008, INV-002/003: active provider creates tenant User keyed by `(issuer, sub)` with only valid mapped tenant roles | `ensure_user_identity`, `verify_authorized_party`, callback role replacement and `issue_human_session`; stored human connection refuses a non-`sub` subject mapping | `tenant_human_login_journey`, `tenant_provider_switch_journey`, unmapped-user and same-email assertions | PASS |
| REQ-013, AC-005: machine credentials remain independent of human SSO; workload assertion has exact issuer/subject/audience/tenant binding | API-key and JWT-bearer grants remain on `/auth/token`; shared verifier requires `exp`, `iss`, `aud` and validates present `nbf`; workload resolution uses `WyrdPostgres::resolve_tenant_slug` | `tenant_machine_independence_journey`, `workload_jwt_bearer_journey_keycloak`, focused missing-claim/future-`nbf` test | PASS |
| REQ-014/015/016, AC-003/006/007: tested tenant-specific replacement, no email linking, old-connection renewal cutoff, bounded access snapshots, current mappings on issuance | Connection revision in login/refresh rows; callback and refresh recheck active connection; refresh family lock serializes refresh, issue, callback role sync, and revocation | provider-switch, same-issuer isolation, session-cutoff and concurrency proofs | PASS |
| REQ-017: redacted canonical audit of connection mutations, login outcomes, role changes, and required decisions; failure cannot establish authority | Callback issuance and role-sync audit share transaction; `HumanConnections::activate` now returns on missing key before `begin_locked` appends its evaluated allowed decision | Existing audit-failure proofs do not assert the new keyless activation decision | **FAIL** (`TASKREV-R11-1`) |
| REQ-005 rev5 and R10 human direction: sealed, one-use, expiring completion; keyless secretless-provider login/activation refused; retained rotation key opens in-flight completion | `seal_completion`, `complete_login_state`, `redeem_login_completion`, `require_keyring`; keyless activation guard | one-use binding test, new expiry/rotation/unusable-key test, new keyless activation test | PASS for credential safety; audit defect above remains |
| Packet-local begin/callback contract: one binding; no public authorization-code grant or token-bearing callback | Typed `BeginLogin` and `LoginState`; retired `TokenRequest::AuthorizationCode`; fixed callback response and served route/OpenAPI ownership | token contract test, callback refusal journey, `pg_openapi_contract` | PASS |
| Required real-server task journeys and negative paths | Four named ignored `identity_e2e` journeys remain selected by the identity lane; the single-peer retry helper only retries pre-handler `429` admission | Recorded `mise run test:identity:journey` 27/27 and focused task tests | PASS |
| R10 directed SQL boundary: one slug resolver; scoped auth/boot/query capabilities | `WyrdPostgres::resolve_tenant_slug` owns operator lookup, auth/boot call it; tenant work uses `TenantConn`; no app-pool fallback | Recorded slug refusal tests and `check:tenant-isolation` / `check:from-pools-allowlist` | PASS |
| R10 documentation findings 23/24 | `token.rs::new_grant_variants_reject_unknown_fields` and auth-routes module have the required corrected rustdoc | Direct source inspection; recorded `fmt` and `lints` | PASS |
| Scope exclusions: no email membership/linking, platform fallback, provider-token API authority, instantaneous revocation promise, new machine model, TASK-003 BFF or TASK-004 CLI implementation | Callback identity and `/auth/token` contract; downstream completion/claim callers are not implemented here | Cumulative diff and task journeys | PASS |

## Prior-finding closure

`FIND-TASK-002-1` through `FIND-TASK-002-22` remain closed at their cumulative owners: exact `sub` and `azp` checks, role-sync audit, advertised algorithm and ID-token claims, RLS login state, shared slug owner, SecretString verifier, mandatory rustdoc/imports, refresh-family serialization across rotation/revocation/issuance/callback, retired CLI/token contract, and bounded refresh response. `FIND-TASK-002-23` and `FIND-TASK-002-24` are closed by commit `1c6b86c65` and current source. The new finding concerns the later keyless activation guard and does not reopen those corrections.

## Proposed finding

### TASKREV-R11-1 — Allowed activation decision disappears on missing sealing key

- **Classification:** INCORRECT; REQ-017 and the repository's audited-authorization rule.
- **Location:** `crates/wyrd/wyrd-auth/src/connections.rs:469` and caller `crates/wyrd/wyrd-server/src/components/admin/identity.rs:308-311`.
- **Evidence:** `activate_candidate` calls `decide`, which evaluates `identity_connections:write` and returns an allowed `AuditEvent` without committing it. The owner normally appends that event inside `begin_locked` (`connections.rs:659-670`). The new `self.require_keyring()?` returns before `begin_locked`, so an authorized activation attempt on a keyless deployment yields `sealing_key_missing` with no canonical allowed decision. The new test at `connections.rs:1322` checks only the error and does not inspect audit staging. A denied caller is already audited by `authorize_recording_denial`; this gap affects the allowed caller refused for missing key.
- **Observable consequence:** Security-significant connection activation attempts disappear from the durable audit trail exactly when the deployment is misconfigured, contrary to REQ-017. The refusal itself is safe and must remain fail closed.
- **Required testable correction:** Keep the missing-key refusal, but commit the already-evaluated allowed decision through the existing connection activation transaction/refusal path before returning it. Reuse `begin_locked` and the owner's committed-refusal pattern; do not add another audit sink or an independent transaction. Extend the keyless activation test to assert one redacted allowed activation decision and no promoted connection, and force audit append failure to confirm activation still refuses. The handler's `decide` call and denied-decision path remain unchanged.

## Verification notes and result

Fresh read-only checks: `git diff --check` passed for the full cumulative diff; the candidate `HEAD` was stable. I inspected the cited production call path, SQL state/refresh owners, corrected R10 rustdoc, and latest fix diff. The packet records green format, lint, boundary, focused Postgres/JWT, and 27/27 identity journey runs; I did not rerun Cargo, Postgres, Docker, or live IdP lanes. TASK-003 BFF redemption and TASK-004 CLI handoff remain downstream and cannot serve as TASK-002 end-to-end proof.

**Overall: FAIL** — one bounded audit correction is required.
