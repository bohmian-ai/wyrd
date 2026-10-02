# TASK-010 behavior review

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `04366e7fc28c466fcdcbc7279885cfee82a988a2`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- Routed prior diagnosis: `FIND-TASK-004-12` in
  `changes/active/oidc-production-readiness/review/TASK-004-r2/`

The candidate remained `HEAD` while this report was prepared. `.codegraph/`
is absent, so navigation used the cumulative Git diff, `rg`, and direct
source/caller inspection. I treated the lead-approved RFC 8693 API-key shape as
binding: `grant_type=urn:ietf:params:oauth:grant-type:token-exchange` with
`subject_token_type=urn:wyrd:oauth:token-type:api_key`; the deleted
`grant_type=wyrd_api_key` has no alias. I excluded BFF/UI consumer failures
caused only by deletion of `/internal/bff/v1`, which belong to TASK-011.

## Navigation and caller-to-result paths reviewed

| Path | Owners and consumers inspected | Result path |
|---|---|---|
| Authorization code + PKCE | `authorize.rs` -> provider redirect -> `callback.rs` -> hashed `auth_login_state` code -> `AuthorizationCodeExchange::redeem_code` -> `TenantTokenIssuer` | The callback returns only a one-minute code; authenticated `wyrd-ui` redemption deletes it, validates the exact client/redirect/S256 challenge, and issues in the same tenant transaction. |
| Device grant | device authorization route -> `CliLogins::{approve,deny,redeem}` -> common callback -> device row approval -> locked token-endpoint redemption | The callback records principal/connection only. The token endpoint locks and revalidates the live row, deletes it, issues, audits, and commits once. |
| Human refresh and revoke | `/auth/token` / `/auth/revoke` -> OAuth client identification -> `RefreshTokens` / `CliLogins::revoke` -> connection revision and refresh-family rows | `wyrd-cli` rotates and contains replay; `wyrd-ui` reuses its original bounded token; connection retirement cuts both off. |
| Machine grants | `/auth/token` form decode -> API-key RFC 8693 exchange, access-token delegation, or RFC 7523 assertion -> existing issuance owners | API-key exchange uses the approved subject-token type and the old private grant is not dispatched. Delegation preserves the existing Wyrd/Bifrost audiences. |
| OAuth wire | `OAuthForm` + `OAuthClients` -> typed request dispatch -> `OAuthError` / `no_store`; RFC 8414 metadata | Form-only requests, Basic client authentication, RFC-shaped bodies, no-store responses, and static endpoint metadata are centralized. |
| Deleted protocol | router/config/sealing/SQL migration and query consumers | Private BFF routes, server browser sessions, sealed completion, and their rewrap columns are removed without an alias. |

Sibling paths inspected included platform assertion exchange, API-key and
delegation issuance/audit, authorization-request refusal redirects, connection
replacement/deactivation/removal, refresh replay containment, device
denial/expiry/poll cadence, served OpenAPI, `wyrd-client::TokenExchange`, and
the changed identity journeys.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-009 / REQ-021: an off-the-shelf `openid-client` 6.8.8 client completes authorization code + S256 PKCE against the real server | Public authorize/callback/token path exists in `crates/wyrd/wyrd-server/src/auth/authorize.rs`, `wyrd-auth/src/callback.rs`, and `components/auth/routes.rs` | `identity_e2e.rs:1268-1299,1581-1675` constructs the authorization and token requests itself; no `openid-client` dependency, driver, or test exists outside the task packet | **FAIL — BEHAVIOR-TASK-010-1** |
| Authorization codes are hashed, expire within 60 seconds, are one-use, and are bound to tenant, principal, `wyrd-ui`, exact redirect URI, and S256 verifier | `wyrd-auth/src/callback.rs:396-441,445-530`; `wyrd-sql/src/queries/auth/login_state.rs` | `tenant_human_login_journey`; `tenant_callback_refusal_journey` covers wrong redirect, PKCE mismatch, expiry, replay, wrong secret, and JSON refusal | PASS |
| Confidential `wyrd-ui` uses `client_secret_basic`; public `wyrd-cli` identifies without a secret; wrong UI secret is `401 invalid_client` with a Basic challenge | `wyrd-server/src/auth/oauth.rs:183-302`; `components/auth/routes.rs:127-167` | `auth::oauth::tests::clients_authenticate_by_registration`; `tenant_callback_refusal_journey` | PASS |
| REQ-011: the off-the-shelf Rust `oauth2` client completes device authorization and refresh through form requests | `wyrd-server/src/auth/cli_login.rs`; `wyrd-auth/src/cli_logins.rs`; `wyrd-client/src/auth.rs` | `identity_e2e.rs:1745-1810,1893-1907`; `human_oidc_login_journey` uses `oauth2` for device login and rotating refresh | PASS |
| Device callback stores approval only; no token/session/refresh row exists until token-endpoint redemption | `wyrd-auth/src/callback.rs:301-341,396-442`; `cli_logins.rs:310-384` | `cli_logins::pg_tests::an_approved_device_code_issues_exactly_once` checks zero refresh rows before redemption and one after | PASS |
| FIND-TASK-004-12 / AC-007: denial, expiry, deletion, or redemption during an in-flight browser approval cannot mint authority; live approval issues exactly once | Callback approval conditionally updates the still-live device row (`callback.rs:418-431`; `device_authorizations.rs:58-69,208-227`), and redemption locks/deletes/issues in one transaction (`cli_logins.rs:316-384`) | Existing tests cover sequential denial/expiry and direct approved redemption, but none pauses after `CliLogins::approve`'s initial lookup (`cli_logins.rs:202-215`), ends/deletes the device row in another actor, resumes provider completion, and asserts no refresh row. The routed finding explicitly requires this interleaving proof. | **FAIL — BEHAVIOR-TASK-010-2** |
| RFC 8628 negatives return `authorization_pending`, `slow_down`, `access_denied`, `expired_token`, then no token | `poll_device_authorization` and `CliLogins::redeem_in` | `device_grant_refusal_journey`; `device_codes_poll_approve_deny_and_expire` | PASS |
| REQ-012: public-client refresh rotates and replay revokes the family; confidential-client refresh does not rotate and has an absolute bound | `wyrd-auth/src/refresh.rs`; issuance stores the authenticated OAuth client | `human_oidc_login_journey`; `revoking_a_human_kills_the_session_refresh_authority`; refresh PostgreSQL tests | PASS |
| REQ-016: an old-connection session cannot refresh after replacement, deactivation, or removal | Human refresh rows retain the connection revision and reissue through the active-connection check | `tenant_connection_session_cutoff_journey` | PASS |
| RFC 7009 revocation is form encoded, client-bound, idempotent for unknown/malformed tokens, and returns 200 | `wyrd-server/src/auth/cli_login.rs`; `wyrd-auth/src/cli_logins.rs:387-470` | `device_grant_refusal_journey` unknown-token case; logout/revocation PostgreSQL tests | PASS |
| REQ-021: token, platform token, revoke, and device authorization reject JSON, answer RFC bodies, and apply `Cache-Control: no-store` | `OAuthForm`, `OAuthError`, and `no_store` in `wyrd-server/src/auth/oauth.rs`; route request bodies use form content types | `tenant_callback_refusal_journey`; OAuth unit tests; served OpenAPI test | PASS |
| RFC 8414 metadata names the authorize, token, device, and revoke endpoints, accepted grants/auth methods, and S256 | `wyrd-server/src/auth/authorize.rs::metadata`; `AuthorizationServerMetadata` | `device_grant_refusal_journey` metadata assertions; served OpenAPI test | PASS |
| RFC 8693 §2 error contract uses its registered errors, including `invalid_target` for a syntactically valid but unsupported target audience | `TokenAudience` is a closed deserialization enum (`wyrd-spec/src/auth/token.rs:87-100`); decode maps every unknown audience to `invalid_request` (`wyrd-server/src/auth/oauth.rs:175-180`); `OAuthErrorCode` has no `InvalidTarget` (`token.rs:179-209`) | No unsupported-target endpoint test; the focused OAuth mapping test confirms only the current generic mapping | **FAIL — BEHAVIOR-TASK-010-3** |
| Approved API-key exchange shape is RFC 8693 and `grant_type=wyrd_api_key` is deleted without an alias | `TokenRequest::TokenExchange` and `ExchangeTokenType::ApiKey`; token dispatch accepts only the RFC grant | `auth::token::tests::grants_decode_from_rfc_parameters` explicitly accepts the approved pair and rejects `wyrd_api_key`; repository search found no production alias | PASS |
| Existing RFC 8693 delegation, RFC 7523 workload, API-key, and workload semantics remain under their existing owners | `TokenGrants::{api_key,delegate}` and `exchange_jwt_bearer`; shared client posts form requests | Existing principal/exchange and identity journeys named by the task; direct caller inspection | PASS |
| Deleted private BFF/session/completion protocols remain deleted, with no compatibility route or JSON alternative | Deleted BFF/router/session/query/migration sources; rewrap table set is narrowed | Cumulative diff and production symbol search; SQL migration and boundary lanes are recorded by the task | PASS |
| BFF/UI consumers that fail only because `/internal/bff/v1` is absent are deferred to TASK-011 | Remaining UI `server-sessions.ts` use of the deleted route is a consumer, not a TASK-010 server alias | Binding lead direction | PASS (out of scope) |
| No change to tenant authority, role mapping, token contents, or audit ownership; no second grant/store/issuer | Existing `TenantConn`, `TenantTokenIssuer`, role-sync, and canonical audit owners are reused | Caller and transaction tracing across callback, issuance, refresh, device redemption, and machine exchanges | PASS |
| No nonstandard mechanism, check, setting, file, or option beyond the approved RFC surfaces | The implementation uses ordinary form extraction, Basic auth, one-use database rows/locks, and existing issuance/audit owners | Cumulative diff inspection found no additional mechanism that is absent from both the RFCs and comparable authorization-server practice | PASS |

## Proposed findings

### BEHAVIOR-TASK-010-1 — MISSING: the required `openid-client` 6.8.8 real-server journey does not exist

- **Violated obligation:** TASK-010 Scenario 1 and its first acceptance
  criterion require `openid-client` 6.8.8, unchanged, to complete code + PKCE
  against a real Wyrd server. REQ-021 requires that a standard OAuth client
  work unchanged.
- **Location:** `crates/wyrd/wyrd-server/tests/identity_e2e.rs:1220-1299,
  1581-1675`; repository test manifests and lockfiles contain no
  `openid-client` test dependency or driver.
- **Evidence:** the existing journey manually generates PKCE, serializes the
  authorize query and token form, creates the Basic header, parses redirects,
  and interprets the token response. `rg -n "openid-client" --glob
  '!changes/**'` returns no source, manifest, or test hit. The implementation
  record itself labels the required client driver deferred, but TASK-011 owns
  the BFF consumer, not this task's independently specified public
  authorization-server interoperability proof.
- **Observable consequence:** the candidate can pass its handwritten request
  assertions while remaining incompatible with the exact off-the-shelf client
  the acceptance criterion names. Discovery processing, client authentication,
  callback validation, or response parsing incompatibility would not be
  detected before integration.
- **Required testable correction:** add the task's focused real-server
  `openid-client` 6.8.8 journey over public RFC 8414/authorize/token endpoints,
  including successful S256 code redemption and the listed code/client
  negatives. It must not restore or call `/internal/bff/v1`; BFF cookie and UI
  behavior remain TASK-011. Run that exact focused journey command and the
  narrow owner lane only.

### BEHAVIOR-TASK-010-2 — MISSING: routed device-terminal race closure has no interleaving regression proof

- **Violated obligation:** `FIND-TASK-004-12`, TASK-010 Scenario 2, and the
  acceptance criteria require a focused proof that a device authorization
  denied, expired/poll-deleted, deleted, or redeemed while browser approval is
  in flight leaves no access token, refresh token, session, or refresh row.
- **Location:** the race window remains structurally present between
  `CliLogins::approve`'s lookup/commit and `begin_bound`
  (`crates/wyrd/wyrd-auth/src/cli_logins.rs:202-215`). The new protective
  recheck is `callback.rs:418-431` through
  `device_authorizations.rs:208-227`. Existing coverage is
  `cli_logins.rs:785-889` and
  `wyrd-server/tests/identity_e2e.rs:2034-2138`.
- **Evidence:** the implementation now appears to close the original issuance
  bug: callback approval updates only an unexpired, undenied, undecided live
  row, and failure rolls back the User/role transaction; token redemption then
  locks/deletes/issues atomically. But the cited tests exercise direct or
  sequential states. None deterministically pauses after the initial
  `pending_device_authorization` lookup, ends the grant from a second actor,
  resumes the provider callback, and checks durable refresh/session state—the
  exact interleaving the routed finding required.
- **Observable consequence:** closure of the prior production race depends on
  unexercised transaction ordering. A later edit could remove the callback
  live-row predicate or split issuance from locked redemption while all current
  device tests remain green, recreating orphan renewable authority.
- **Required testable correction:** add the focused Postgres/server
  interleaving proof prescribed by `FIND-TASK-004-12`: pause approval after its
  initial device lookup; in separate cases deny and expire/poll-delete (or
  otherwise delete) the row; resume and complete provider authentication; then
  prove the callback records no approval/credential and no refresh row exists.
  Retain the existing normal exactly-once redemption proof. Reuse repository
  transaction/lock test facilities; add no lease, marker, cleanup process, or
  alternate device state.

### BEHAVIOR-TASK-010-3 — INCORRECT: unsupported RFC 8693 target audiences are returned as `invalid_request`

- **Violated obligation:** REQ-021 requires RFC 8693 §2 and its registered
  error codes. RFC 8693 §2.2.2 defines `invalid_target` for a requested target
  service the authorization server cannot or will not satisfy.
- **Location:** `crates/wyrd-spec/src/auth/token.rs:87-100,179-209` and
  `crates/wyrd/wyrd-server/src/auth/oauth.rs:175-180`; dispatch is
  `crates/wyrd/wyrd-server/src/components/auth/routes.rs:168-190`.
- **Evidence:** a request with the supported token-exchange grant and token
  types but a syntactically valid unsupported audience, for example
  `audience=storage`, fails closed-enum deserialization. `OAuthForm::decode`
  maps every such failure to `invalid_request`; the public error enum cannot
  serialize `invalid_target` at all. This is not an optional new error or Wyrd
  extension; it is the standard error for the approved RFC surface.
- **Observable consequence:** conforming token-exchange clients cannot
  distinguish a malformed request from an unsupported target and therefore do
  not receive the RFC 8693 error contract REQ-021 promises.
- **Required testable correction:** preserve an unsupported audience through
  request decoding far enough for the existing token-exchange dispatch/error
  owner to return RFC 8693 `invalid_target`; add that registered value to the
  existing OAuth error contract. Keep the supported Wyrd/Bifrost audience
  semantics and all token contents unchanged. Prove a form-encoded public
  token-endpoint request with an unsupported target returns `400
  {"error":"invalid_target"}` and no token. Add no audience registry,
  setting, compatibility alias, or custom error mechanism.

## Non-blocking notes

None. I did not report placement, naming, structure, wording, or optional
hardening observations. I also did not treat the TASK-011-owned UI source that
still calls deleted `/internal/bff/v1` routes as a TASK-010 finding.

## Verification assessment

Independently executed:

```text
git diff --check fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa..04366e7fc28c466fcdcbc7279885cfee82a988a2
mise exec -- cargo nextest run --locked -p wyrd-spec --lib \
  -E 'test(=auth::token::tests::grants_decode_from_rfc_parameters)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(=auth::oauth::tests::catalog_refusals_map_to_rfc_codes)'
```

The diff check was clean and both focused tests passed. I inspected the source
of the task-named identity and PostgreSQL tests rather than treating the
implementation evidence table as proof. Per the binding review direction I did
not run the unfiltered multi-language/full-journey sweep; that belongs to
change review. The missing `openid-client` journey and the absent routed-race
interleaving proof are task-level narrow-lane omissions, not full-journey
verification limits.

## Overall result

**FAIL**

The core authorization-code, device, refresh, revoke, metadata, deletion, and
approved API-key exchange implementation paths are present, and the source
appears to close the previously diagnosed device issuance race. TASK-010 is
not yet acceptable because two expressly required focused proofs are absent
and the public RFC 8693 error contract cannot return `invalid_target` for an
unsupported target.
