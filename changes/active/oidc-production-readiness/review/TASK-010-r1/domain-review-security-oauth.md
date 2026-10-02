# Domain Review — Security and OAuth

## Overall Result

**FAIL** — the authorization-server wire, grant bindings, and credential handling are coherent, but refresh-token replay containment is broader than RFC 9700 and is triggered by states that are not replay. One material security/availability finding remains.

## Reviewed Boundary

Immutable subject:

- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `04366e7fc28c466fcdcbc7279885cfee82a988a2`
- Candidate was still `HEAD` at review time.

The review traced the complete candidate diff and the live caller-to-authority paths for:

- `GET /auth/authorize`, the shared OIDC callback, authorization-code persistence and redemption, exact redirect binding, PKCE S256, and one-time/expiry behavior;
- `POST /auth/token` dispatch for authorization code, refresh, device code, RFC 8693 token exchange, API-key exchange, delegation, and RFC 7523 JWT bearer assertions;
- `POST /auth/platform/token`, `POST /auth/device_authorization`, the device verification/approval flow, `POST /auth/revoke`, and RFC 8414 metadata;
- OAuth form parsing, confidential/public client identification, `invalid_client` status/challenge behavior, RFC error mapping, and no-store responses;
- refresh issuance, rotation, replay containment, revocation, client/connection binding, SQL row authority, locking, RLS, and audit commits;
- shared-client form encoding, redirect refusal, transport validation, API-key/JWT/delegation calls, and error decoding;
- configuration and debug/log boundaries for signing, sealing, provider, client, API-key, assertion, code, and bearer material;
- the changed unit, PostgreSQL integration, OpenAPI, off-the-shelf-client, and journey tests that claim these paths.

The remaining BFF/UI consumer rewrite is outside this task and belongs to TASK-011. In particular, failures caused by the deleted `/internal/bff/v1/*` surface, and the stale UI test that still constructs the deleted `wyrd_api_key` request, are not treated as TASK-010 implementation gaps. The served token endpoints and shared client contain no `grant_type=wyrd_api_key` compatibility alias.

## Authority and Source Coverage

| Authority or source | Coverage and conclusion |
|---|---|
| Approved OIDC production-readiness specification revision 11 | Reviewed REQ-005, REQ-006, REQ-007, REQ-009, REQ-011, REQ-012, REQ-013, REQ-016, REQ-017, REQ-021; INV-001, INV-005, INV-007; and AC-004, AC-005, AC-007, AC-009. |
| TASK-010 and TASK-004 r2 routing | Reviewed every authorization-server scenario and the routed device-redemption finding. Device approval stores no credential and redemption performs the single mint in the deletion transaction. |
| `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, and `architecture/wyrd-security-posture.md` | Applied tenant derivation, audit, secret handling, public-contract, client/server, no-compatibility, and standards-first requirements. |
| [RFC 6749](https://www.rfc-editor.org/rfc/rfc6749), [RFC 7636](https://www.rfc-editor.org/rfc/rfc7636), [RFC 7009](https://www.rfc-editor.org/rfc/rfc7009), [RFC 8628](https://www.rfc-editor.org/rfc/rfc8628), [RFC 8693](https://www.rfc-editor.org/rfc/rfc8693), [RFC 7523](https://www.rfc-editor.org/rfc/rfc7523), [RFC 8414](https://www.rfc-editor.org/rfc/rfc8414), and [RFC 9700](https://www.rfc-editor.org/rfc/rfc9700) | Checked the task-listed sections against the handlers, wire types, durable state, and shared clients. RFC 9700 §4.14.2 retains the relationship between rotated tokens and revokes the active token belonging to the compromised grant; it does not turn any expired or independently revoked token into a principal-wide revocation handle. |
| Server and contract sources | Reviewed `auth/authorize.rs`, `auth/oauth.rs`, `auth/cli_login.rs`, `auth/callback.rs`, `components/auth/routes.rs`, `components/platform/routes.rs`, `components/auth/state.rs`, `wyrd-spec/src/auth/{token,device,oidc}.rs`, route mounting, and served OpenAPI tests. |
| Durable auth owners | Reviewed `wyrd-auth/src/{callback,cli_logins,refresh,issuance,exchange_api_key,jwt_bearer,revoke,sealing}.rs`, auth SQL queries/row types/migrations, RLS predicates, locks, transaction boundaries, and audit writes. |
| Consumers and tests | Reviewed `wyrd-client/src/auth.rs`, saved-login/credential callers, CLI refresh/login callers, server identity journeys, OAuth refusal tests, refresh concurrency/replay tests, device tests, SQL migration tests, and TASK-010's recorded verification table. |

## Verification Limits

- This was a review-only static and source audit of the immutable range. No source was changed and no broad or full-journey lane was rerun; full journeys remain the change-review gate by direction.
- `git diff --check <base> <candidate>` passed during this review.
- TASK-010 records green focused owner lanes, SQL/codegen/docs/boundary checks, and filtered identity journeys. Those results establish substantial coverage but do not clear the finding below: the focused refresh test explicitly asserts the over-broad behavior, and the expired-token test explicitly classifies expiry as replay.
- TASK-011's BFF/UI journey and TASK-012's final client-library migration remain deferred as recorded by the task. They do not limit the source conclusion below.

## Material Proposed Findings / Security Audit

### Critical

None.

### High

None.

### Medium

- **[SEC-OAUTH-001] DRIFT — [`crates/wyrd/wyrd-auth/src/refresh.rs:166`](../../../../../crates/wyrd/wyrd-auth/src/refresh.rs) and [`crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs:252`](../../../../../crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs): any inactive CLI refresh row becomes a principal-wide revocation handle.**
  - **Violated obligation:** REQ-012 and TASK-010 require RFC 9700 §4.14.2 rotation and family containment when a *rotated* public-client token is replayed. The standing standards direction requires the RFC/conventional grant relationship, not a broader Wyrd-only mechanism. Expired tokens and tokens revoked by logout or administration are ordinary `invalid_grant` refusals; they are not evidence of rotation replay. A token family is the rotation chain/grant, not every refresh credential held by the principal across independent CLI and `wyrd-ui` logins.
  - **Evidence:** `refresh_by_hash` returns active, rotated, revoked, and expired rows. After the principal-wide lock, `consume_active_refresh` returns `None` for all inactive states, and `RefreshTokens::execute` treats that single result as theft, calls `revoke_refresh_family`, and commits the containment despite returning an error. `revoke_refresh_family` updates every active row matching only `(principal_kind, principal_id)`, so a replay from one CLI chain also revokes independent CLI chains and non-rotating confidential-client UI sessions. The repository already has the RFC-shaped `revoke_refresh_chain` helper at `refresh_tokens.rs:217`, which follows `rotated_from` descendants and explicitly leaves other logins untouched. The tests make the defect observable: `refresh.rs:638-688` requires an unrelated sibling to be revoked, and `refresh.rs:780-817` requires an expired token to return `Reused`.
  - **Exploit path and impact:** an attacker who once obtained a CLI refresh token can wait until it is rotated, expires, or is logged out. After the user signs in again—through another CLI login or the UI—the attacker presents the old token to `/auth/token`. Wyrd commits `reuse_detected` against every active refresh row for that user, terminating all current renewable sessions. Because the old inactive row remains addressable by hash, replay can be repeated against later independent logins. This turns standard replay containment into a persistent cross-client account-denial primitive; it grants no new data authority but has a concrete availability and session-containment impact.
  - **Testable correction:** while retaining the existing transaction/advisory-lock ordering, distinguish a predecessor consumed by rotation from expiry and other revocation reasons. Only a `wyrd-cli` row invalidated as `rotated` should enter reuse containment, and containment should call the existing `revoke_refresh_chain(stored.id, "reuse_detected")` so only its active descendants are retired. Expired, logged-out, administratively revoked, wrong-client, and unknown rows must return the ordinary invalid-grant path without a containment write. Replace the sibling-revocation expectation with a PostgreSQL test containing (1) a rotated ancestor and successor, (2) an independent CLI chain, and (3) a `wyrd-ui` row; replay must revoke only the successor. Add focused cases proving expired and logout-revoked tokens cannot revoke a later login.

### Low / Defense In Depth

None. No optional hardening or placement/naming/wording issue is proposed as a task finding.

### Positive Controls

- Exact registered redirect validation occurs before any client redirect; hostile `Host` and forwarding headers do not select the tenant, issuer, or redirect target.
- Authorization codes and device codes are random, stored only by digest, expiry-bound, consumed transactionally, and bound to the server-owned tenant/principal/connection context. Authorization codes additionally bind client, exact redirect URI, and PKCE S256 challenge.
- Confidential `wyrd-ui` authentication uses form-decoded HTTP Basic credentials and returns `401 invalid_client` with a Basic challenge; public `wyrd-cli` credentials stay client-bound in refresh/revocation rows.
- OAuth token, platform-token, device-authorization, and revocation endpoints reject JSON, refuse repeated parameters, return RFC-shaped errors, and attach `Cache-Control: no-store` plus `Pragma: no-cache` to token-bearing and error responses.
- The approved API-key exchange is exactly RFC 8693 with `subject_token_type=urn:wyrd:oauth:token-type:api_key`; `grant_type=wyrd_api_key` has no served alias. Delegation verifies both signed subject and actor tokens and derives tenant authority from verified material.
- Device approval stores only principal/connection approval; token and refresh rows are created only by the single redemption transaction. Denial, expiry, deletion, or a second redemption cannot mint a credential.
- Token, code, API-key, assertion, provider-secret, and client-secret values are skipped or redacted at traced handler and domain boundaries. Shared clients refuse remote cleartext targets and redirects that could replay secret form bodies.
