# Security and OIDC domain review — TASK-002 R12

## Subject and boundary

- Immutable base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`.
- Candidate: `2d57a6605da9bc1cee61140d7c09742cc4636efa` (HEAD at inspection).
- Latest fix: `bae424cc647dad4be80e7debec976d0b7b3e4cf8..2d57a6605da9bc1cee61140d7c09742cc4636efa`.
- Reviewed the cumulative security-sensitive implementation of TASK-002 against approved spec revision 5, the historical original task, and R11 remediation. This is a source review; no implementation was changed.

## Authority and source coverage

| Security boundary | Authority | Source and evidence inspected | Result |
|---|---|---|---|
| Tenant login, callback, and external trust | Spec REQ-006–008, INV-001/004; TASK-002; `architecture/wyrd-security-posture.md` | `wyrd-auth/{login,callback,connections}.rs`, `wyrd-sql/queries/auth/login_state.rs`, server auth handlers, screened provider transport; cumulative diff and recorded identity journeys | PASS |
| OIDC ID token and workload assertion profiles | Spec REQ-007/013, INV-003/004; TASK-002; R10 standards decision | `wyrd-auth-verify/src/lib.rs`, callback algorithm/nonce/authorized-party checks, `wyrd-server/src/auth/jwt_bearer.rs`; valid and invalid assertion evidence | PASS |
| User identity, role and session authority | Spec REQ-008/014–016, INV-002/003; security posture | Callback `(issuer, sub)` resolution, role replacement, refresh-family locking, connection-bound renewal and revocation, provider-switch journey | PASS |
| Sealed completion and keyless activation | Spec rev 5 REQ-005, REQ-017; R11 FIND-25 | `HumanConnections::activate`, `begin_locked`, `commit_refusal`, keyless Postgres test, rotation journey evidence | PASS |
| Canonical authorization audit | Spec REQ-017; `AGENTS.md` audit rule; `architecture/agent-rules.md` | Server `decide`/`authorize_recording_denial`, `wyrd-auth` activation append and refusal; server `record_audit`/`record_audit_owned` and live callers | PASS |

## R11 security closure and prior findings

`FIND-TASK-002-25` is closed. The handler evaluates `identity_connections:write` and hands an uncommitted allowed event to `HumanConnections::activate`. Activation now calls `begin_locked` first; that takes the tenant connection and slot lock and appends the event. If the sealing keyring is absent, `commit_refusal` commits that decision and returns the same `sealing_key_missing` error before reading the candidate or recovery key. An append or commit failure returns an error and cannot promote a connection. The focused Postgres test checks one allowed staging row, absence of the supplied recovery secret, and no Active connection. The recorded rotation journey covers an injected activation audit failure.

The R11 standalone audit change retains the canonical staging append through `ValaPostgres::tenant_conn`; the owned adapter delegates to that same function in a spawned task. Denials remain recorded before refusal, and gateway invocation keeps its tracked background audit task. This closes the security aspect of `FIND-TASK-002-27`; repository-wide raw-pool compliance is for the standards review. `FIND-TASK-002-26` is documentation-only and outside this domain.

Earlier security findings remain closed in current source: server-owned one-time state selects the tenant; exact active connection and revision are rechecked before issuance; ID tokens take signature/issuer/audience/algorithm/time/`sub`/`iat` plus nonce and authorized-party checks; generic signed assertions require `exp`, `iss`, `aud` and enforce a present `nbf`; verified groups map only to tenant roles; concurrent User role/session writes use the refresh-family lock; and refresh remains bound to the originating human connection. No new security finding arose from the cumulative or latest fix diff.

## Verification limits

This review inspected committed source and the cumulative and latest diffs. Fresh `git diff --check` passed. I did not rerun Postgres, Keycloak, Cargo, or live IdP tests. The R11 implementation record reports the focused keyless and rotation tests, identity journey 27/27, principals integration, gateway native, Bifrost server journey, tenant-isolation, pool-construction, format, and lints as passing. Browser redemption (TASK-003) and CLI handoff claim (TASK-004) remain downstream and are not credited as TASK-002 proof.

## Findings and result

No material security or OIDC findings. **PASS**.
