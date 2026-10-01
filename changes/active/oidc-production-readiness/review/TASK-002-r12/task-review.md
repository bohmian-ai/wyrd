# TASK-002 R12 task implementation review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Original base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `2d57a6605da9bc1cee61140d7c09742cc4636efa`
- Latest remediation range: `bae424cc647dad4be80e7debec976d0b7b3e4cf8..2d57a6605da9bc1cee61140d7c09742cc4636efa`
- Authority: approved `changes/active/oidc-production-readiness/spec.md` revision 5; historical original `tasks/TASK-002-tenant-login.md`; R1–R11 verdicts and remediation packets; `AGENTS.md` and `architecture/agent-rules.md`.

The candidate stayed at the stated commit during this review. `.codegraph/` is absent. I inspected the cumulative diff and used the latest diff to locate R11 changes. The implementation record reports the focused and broad tests below; this reviewer inspected source and ran `git diff --check`, but did not rerun Cargo or provider-backed lanes.

## Acceptance matrix

| Requirement, criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-006/007; INV-001/004; AC-003: server-bound tenant state, authorization code with PKCE and nonce, checked ID token, one-use callback and fail-closed provider boundary | `wyrd-auth/src/login.rs`, `callback.rs`, shared `wyrd-auth-verify/src/lib.rs`, SQL login-state queries; common callback remains the only code exchange | Recorded `tenant_callback_refusal_journey`, verifier tests and 27/27 identity journey | PASS |
| REQ-008/015; INV-002/003; AC-002: tenant User derives from verified `(issuer, sub)` and current tenant mappings, with no email linking or privileged default | `callback.rs` resolves tenant User and role grants; migration removes tenant-email uniqueness | Recorded `tenant_human_login_journey`, same-issuer cross-tenant and provider-switch journeys | PASS |
| REQ-013; AC-005: API-key and exactly bound workload assertion paths remain independent | JWT bearer and API-key grants remain; shared JWT verifier requires RFC 7523 issuer/audience/expiry and rejects future `nbf`, while ID-token-only rules remain on the ID-token entry point | Recorded workload missing-claim, future-`nbf`, valid-assertion tests and `tenant_machine_independence_journey` | PASS |
| REQ-014/016; AC-006/007: tested replacement, old-connection renewal cutoff, current role authority and bounded access snapshot | Connection revision checked on login/refresh; existing refresh-family lock covers issuance, refresh, login role sync and revocation | Recorded provider-switch, refresh/revocation concurrency and 27/27 identity journey | PASS |
| REQ-005 rev5: sealed recoverable handoff, keyless refusal, key rotation, and no independent machine-auth dependency | `HumanConnections::require_keyring`, sealed callback completion and one-time tenant redemption | Recorded keyless test, rotation journey and machine journey | PASS |
| REQ-017; R11 finding 25: authorized keyless activation must record its allowed decision, refuse, and promote nothing | `HumanConnections::activate` calls `begin_locked` before `require_keyring`; `begin_locked` appends the decision under the slot lock; `commit_refusal` commits the refusal without promotion | Recorded exact keyless Postgres test asserts one redacted staged row and no Active connection; existing rotation journey injects audit-append failure | PASS |
| R11 finding 26: resolver module documentation matches its owner and boundary | `wyrd-auth/src/pg_resolvers.rs` header names `wyrd-auth`, `WyrdPostgres`, and `TenantConn` RLS, matching both resolver fields and read paths | Source and latest diff inspection; recorded format/lints | PASS |
| R10 SQL boundary and R11 finding 27: single operator-backed tenant-slug resolver and no raw pool through the live audit entry points | `WyrdPostgres::resolve_tenant_slug` remains the only slug resolver; both server standalone audit forms now accept `ValaPostgres`, acquire `TenantConn`, and reuse canonical `append_audit`; all live callers pass the handle | Recorded tenant-isolation check, injected raw-signature failure proof, identity, gateway, Bifrost and principals lanes | PASS |
| Audit operation semantics: transactional decisions except the named tracked nonblocking paths | `audit::record_audit` commits the same canonical staging row; `record_audit_owned` keeps the owned `Send` task; gateway still spawns its tracked audit task | Recorded gateway/native, Bifrost/server and principals integration lanes | PASS |
| Exclusions: no email linking, platform fallback, provider-token API bearer, new machine identity, or early TASK-003/004 browser/CLI claim implementation | Cumulative code and contract diff preserve those exclusions | Journey and contract evidence recorded in original and remediation packets | PASS |

## Prior findings and R11 closure

`FIND-TASK-002-1` through `FIND-TASK-002-24` remain closed at their previously reviewed owners: shared claim validation, state/tenant resolution, family serialization, token-contract and route rustdoc, and the R10 single slug resolver are still present. R11's only source changes to those owners are the bounded activation audit and resolver-header corrections. `FIND-TASK-002-25` closes because the missing-key branch now runs after the existing canonical audit append and commits via `commit_refusal`. `FIND-TASK-002-26` closes by the corrected resolver header. `FIND-TASK-002-27` closes for the live server audit entry points because both signatures and all callers now use the existing `ValaPostgres` handle, which acquires `TenantConn`.

The pre-existing, currently uncalled eval resolver still has a raw-pool signature; the R11 verdict explicitly excluded that dormant surface from TASK-002 remediation and did not approve it as an exception. `ServerPostgres::vala_pool()` also remains for non-audit callers. Those facts are not findings against this bounded remediation. The repository-standards reviewer owns the independent rule audit.

## Findings and result

No new task-scope finding. **PASS.** The three R11 findings close without changing the original tenant-login contract or adding a second audit, SQL, or identity-resolution mechanism. Verification is based on committed implementation evidence plus source inspection; TASK-003 browser redemption and TASK-004 CLI redemption remain downstream.
