# TASK-010 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `04366e7fc28c466fcdcbc7279885cfee82a988a2`
- Specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- Routed prior direction: `changes/active/oidc-production-readiness/review/TASK-004-r2/lead-direction-routing.md`, including `FIND-TASK-004-12`

The candidate remained at the stated commit throughout this review. I reviewed
the complete base-to-candidate diff and traced authorization requests, provider
callbacks, authorization codes, device approvals, refresh families, client
identity, connection revision, tenant/principal binding, one-time consumption,
canonical audit, rollback, and the sibling token-exchange and revocation
consumers. I treated the task's implementation evidence as claims to check, not
as proof, and did not modify reviewed source.

## State and invariant trace

- `OAuthForm` is the common form/query parser. `/auth/authorize` validates the
  one registered `wyrd-ui` redirect before `HumanConnections::authorize`
  persists `ClientAuthorization` beside the provider state, nonce, verifier,
  exact connection revision, tenant, and provider redirect.
- The provider callback commits consumption of that state before provider IO.
  A successful authorize initiation attaches a fresh 256-bit Wyrd code hash
  and principal to the same row for 60 seconds; a device initiation records
  only the approving principal and connection revision on the live device row.
- Authorization-code redemption routes only by the untrusted tenant prefix,
  deletes the RLS-visible hash once, then checks lifetime, client, exact
  redirect, and `S256(code_verifier)`. Issuance and canonical audit share the
  transaction; failure rolls back the delete, while a binding mismatch commits
  the spent code.
- Device redemption locks the RLS-visible device row, rechecks expiry, denial,
  polling cadence, approval, connection revision, and principal, deletes the
  grant, issues the session, appends canonical audit, and commits once. The
  callback cannot leave a token or refresh row behind. This closes routed
  `FIND-TASK-004-12`, including deny/delete/expiry races.
- Refresh rows now carry the OAuth client. `wyrd-cli` consumes and rotates
  under the tenant-qualified principal-family lock; replay commits family
  revocation and its audit. `wyrd-ui` reads the still-active row under the same
  lock and issues only access authority until the fixed absolute expiry. Both
  paths re-read the exact Active connection and current principal/role state.
- Revocation authenticates the client, resolves the opaque token by digest
  under tenant RLS, locks the family, revokes only that login's rotation chain,
  and commits the canonical audit. Unknown and malformed tokens return `200`.
- API-key exchange is only RFC 8693 token exchange with
  `subject_token_type=urn:wyrd:oauth:token-type:api_key`. The production
  request enum and grant list have no `grant_type=wyrd_api_key` alias.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| RFC 6749 authorization endpoint: exact registered redirect, code response, and §4.1.2.1 errors returned to a validated client | `wyrd-server/src/auth/authorize.rs:75-127,191-211`; callback routing at `components/auth/routes.rs:468-496` | Authorize Postgres tests cover success, unknown tenant, missing/unsupported request values, and unsafe redirect; no provider-error callback case exists, while the duplicate-state test pins the contrary local-page behavior | **FAIL — INV-REV-001** |
| REQ-009 / AC-007: Wyrd code is hashed, expires within 60 seconds, is single-use, and is bound to tenant, principal, `wyrd-ui`, exact redirect URI, connection revision, and PKCE S256 | Code issuance and redemption at `wyrd-auth/src/callback.rs:396-442,445-554`; durable transition at `wyrd-sql/src/queries/auth/login_state.rs:71-101,360-408` | `tenant_callback_refusal_journey`; callback Postgres cases for wrong, expired, replayed, redirect-mismatched, verifier-mismatched, and audit-failed redemption were reported green | PASS |
| RFC 6749 §2.3.1 confidential `wyrd-ui` authentication and public `wyrd-cli` identification interoperate with conforming HTTP clients | `wyrd-server/src/auth/oauth.rs:231-298`; accepted secret hashes come from `WYRD_UI_CLIENT_SECRET_SHA256` | `clients_authenticate_by_registration` covers only the preferred `Basic` spelling; wrong-secret journey covers `401` and `WWW-Authenticate` | **FAIL — INV-REV-002** |
| REQ-011 / RFC 8628: standard form request, user-code entropy/rate limiting, pending/slow-down/denied/expired results, and mint only at live redemption | `wyrd-server/src/auth/cli_login.rs`; `wyrd-auth/src/cli_logins.rs:109-384`; `device_authorizations.rs:24-101,180-263`; shared auth governor | `human_oidc_login_journey`, `device_grant_refusal_journey`, and focused `cli_logins` Postgres tests were reported green | PASS |
| Routed FIND-TASK-004-12: terminal or already-redeemed device grants leave no credential/session/refresh row; one live approval issues exactly once | Callback records only approval at `callback.rs:418-432`; redemption revalidates, deletes, issues, audits, and commits at `cli_logins.rs:310-384` | `an_approved_device_code_issues_exactly_once`, terminal device tests, and the identity refusal journey were reported green | PASS |
| REQ-012 / RFC 9700 §4.14.2: CLI rotates and replay revokes the family; confidential UI refresh does not rotate and has bounded absolute lifetime | `wyrd-auth/src/refresh.rs:93-259`; `issuance.rs:635-699`; refresh-row client binding in migration/query/row types | `human_oidc_login_journey` and refresh Postgres tests, including reuse containment and nonrotating UI refresh, were reported green | PASS |
| REQ-016: replacement/deactivation/removal of the bound connection prevents later refresh or redemption | `issue_human_access` locks and checks the exact connection revision; code/device/refresh paths carry `HumanConnectionBinding` | Connection cutoff journeys and issuance refusal tests were reported green | PASS |
| RFC 7009 §2: form request, authenticated client binding, login-chain revocation, and `200` for malformed/unknown/already-ended tokens | `wyrd-server/src/auth/cli_login.rs:265-307`; `wyrd-auth/src/cli_logins.rs:387-453` | Unknown-token journey and revocation/chain tests were reported green | PASS |
| REQ-021: form wire and RFC token/error bodies; JSON is refused; responses are non-cacheable | `OAuthForm`, `OAuthError`, and `no_store` in `wyrd-server/src/auth/oauth.rs`; token/platform/revoke/device handlers | `tenant_callback_refusal_journey`, OAuth units, served OpenAPI contract, and principals integration were reported green | PASS, apart from the authorization-response failures above |
| Lead-approved API-key contract: RFC 8693 with `urn:wyrd:oauth:token-type:api_key`; no legacy grant alias | `wyrd-spec/src/auth/token.rs:44-85,266-319`; grant dispatch at `components/auth/routes.rs:168-190`; platform dispatch at `components/platform/routes.rs:93-141` | Contract unit explicitly rejects `grant_type=wyrd_api_key`; platform, CLI, and workload journeys were reported green | PASS |
| RFC 8693 delegation and RFC 7523 JWT bearer semantics remain at their established owners | Token request variants and token dispatch reuse `DelegateToken` and `exchange_jwt_bearer`; no alternate issuer, tenant selector, or verifier was introduced | Workload JWT-bearer, service-account, federated-cloud, platform, CLI, and MCP journeys were reported green | PASS |
| RFC 8414 metadata publishes the actual authorization, token, device authorization, and revocation endpoints and supported methods | `wyrd-server/src/auth/authorize.rs:130-179` | Metadata assertions in `device_grant_refusal_journey` and served OpenAPI checks were reported green | PASS |
| Decision 7 / REQ-005: private BFF protocol, browser-session store, sealed completion, and narrowed rewrap columns are deleted; no compatibility alias remains | Deleted `components/auth/bff.rs`, `browser_sessions.rs`, browser-session queries/migrations; new code/approval transitions replace sealed completion | `test:sql`, sealing boot tests, codegen, boundary, format, and lint lanes were reported green | PASS |
| TASK-011 boundary: stale UI/BFF callers failing only because `/internal/bff/v1` was deleted do not keep a compatibility route and are not a TASK-010 defect | Server route and owner are absent; the remaining TypeScript BFF consumer is unchanged in this candidate | Task evidence identifies only the TASK-011 UI/BFF lane as deferred | PASS for TASK-010 boundary |
| Prohibited drift: no extra grant, parameter, compatibility document, stored issued credential, tenant/header authority, role mapper, or alternate audit owner | Complete diff and producer-to-sink trace | Static inspection plus recorded targeted lanes | PASS |

## Proposed findings

### INV-REV-001 — INCORRECT: two standard authorization-error paths bypass the registered client redirect

- **Violated obligation:** TASK-010 requires `/auth/authorize` and its code
  response to implement RFC 6749 §4.1.1-§4.1.2.1. Once the client and exact
  redirect URI are valid, malformed authorization parameters and resource-owner
  denial must be returned to that redirect as an authorization error; only a
  missing/invalid client or redirect stays on a local page. REQ-021 requires a
  standard OAuth client to work unchanged.
- **Exact location:** `crates/wyrd/wyrd-server/src/auth/authorize.rs:84-95`
  discards the whole parsed form on any duplicate before it can retain a valid
  client/redirect binding; the contrary test is
  `authorize.rs:432-451`. Independently,
  `crates/wyrd-spec/src/auth/oidc.rs:378-398` models only a successful provider
  callback with required `code`, and
  `crates/wyrd/wyrd-server/src/components/auth/routes.rs:468-481` lets Axum
  reject a provider `error=access_denied&state=...` before the durable state can
  recover the downstream client redirect.
- **Evidence:** RFC 6749 §4.1.2.1 defines a repeated parameter as
  `invalid_request` and directs failures other than a missing/invalid redirect
  URI to the client redirect. A request containing exactly one registered
  `client_id` and `redirect_uri` but two `state` parameters currently returns a
  local `400` with no `Location`; the candidate test explicitly asserts that
  result. In the provider-denial path, `CallbackQuery` cannot deserialize the
  standard error response because `code` is absent, so no login-state lookup or
  downstream `client_redirect` occurs.
- **Observable consequence:** a conforming BFF receives neither
  `error=invalid_request` for a malformed request whose redirect was safe nor
  `error=access_denied` when the person denies or cancels at the upstream IdP.
  Its authorization attempt remains unresolved even though Wyrd has the exact
  registered redirect and client state needed to finish it safely.
- **Required testable correction:** reuse the existing
  `ClientAuthorization`/`client_redirect` authority. For the authorization
  endpoint, establish that `client_id` and `redirect_uri` each occur once and
  match the registration before routing duplicate or otherwise malformed
  non-binding parameters to `invalid_request`; retain the local page for an
  invalid or ambiguous client/redirect. Model the provider callback's standard
  success-or-error response, consume the state through the existing one-time
  owner, apply the existing issuer binding when present/required, and map a
  provider denial to the downstream authorization error without minting a code,
  token, session, or refresh row. Add focused real-handler cases for duplicate
  non-binding parameters, duplicate binding parameters, and provider denial,
  asserting the registered redirect, exact echoed client state, one-time state
  consumption, and no authority. Add no compatibility route or new protocol
  state.

### INV-REV-002 — INCORRECT: `client_secret_basic` rejects a conforming case variant of the HTTP authentication scheme

- **Violated obligation:** TASK-010's client-authentication row requires RFC
  6749 §2.3.1 `client_secret_basic`, and REQ-021 requires standard OAuth clients
  to interoperate unchanged. HTTP authentication schemes are case-insensitive
  tokens (RFC 9110 §11.1).
- **Exact location:**
  `crates/wyrd/wyrd-server/src/auth/oauth.rs:255-269`, especially the
  case-sensitive `strip_prefix("Basic ")`; the only positive unit at
  `oauth.rs:345-380` constructs the preferred capitalization.
- **Evidence:** `Authorization: basic <credentials>` and other case variants
  are valid HTTP Basic credentials, but the parser returns `invalid_client`
  before decoding them. Header field-name normalization does not normalize the
  authentication scheme value.
- **Observable consequence:** a standards-conforming confidential client using
  a non-preferred case cannot redeem an authorization code, refresh, or revoke
  its login and receives `401 invalid_client` despite presenting the configured
  secret.
- **Required testable correction:** split the existing Authorization value into
  its scheme and credentials, compare the scheme with
  `eq_ignore_ascii_case("Basic")`, then retain the existing Base64,
  form-component decoding, client-id binding, secret-hash comparison, and error
  behavior. Extend the existing client-authentication unit to prove at least
  `Basic` and `basic` succeed with the same registered secret and an unrelated
  scheme remains `invalid_client`. No new authentication mode or setting is
  required.

## Non-blocking notes

- RFC 8628 §3.5 makes the client increase its polling interval by five seconds
  after `slow_down`. The server already detects an early poll and returns that
  code; requiring it to persist a growing server-side interval would add state
  the cited RFC does not require, so I did not report it.
- RFC 7636 §4.6 requires the server to calculate and compare the S256 challenge,
  which this candidate does. The 43-128-character verifier construction rule
  is a client-side rule in §4.1; I did not require a second server-side syntax
  mechanism when the binding calculation remains exact.
- `crates/wyrd-spec/src/error.rs:4591-4594` retains `wyrd_api_key` only as stale
  detail text in a test fixture, and `wyrd-server/src/boot/mod.rs:1522-1538`
  still says “browser-session” in rewrap documentation/error wording. Neither
  is a production grant alias or reachable browser-session mechanism. They are
  wording-only and therefore non-blocking under the binding review direction.
- The unchanged TypeScript BFF and UI journey still mention the deleted private
  route. Per the task and lead direction, failures caused only by that deleted
  consumer are TASK-011 work and are not a TASK-010 finding.

## Verification assessment

The task records successful focused identity journeys, 54 server auth tests,
34 `wyrd-auth` tests, `test:principals:integration`, `test:sql`, platform and CLI
journeys, code generation, boundary checks, format, lint, and language checks.
Those results are proportionate to TASK-010's narrow owner lanes and corroborate
the source trace; full multi-language journeys correctly remain for change
review. I did not rerun the recorded lanes in this discovery pass.

Neither finding is a generic verification limit: current source and the
existing contrary test establish INV-REV-001, while the literal parser and HTTP
case rule establish INV-REV-002. No required reviewer was unavailable to this
independent report, and no subject change blocked review.

## Overall result

**FAIL**

The candidate closes the device terminal-state finding, preserves tenant,
principal, connection, refresh-family, audit, and API-key exchange invariants,
and deletes the obsolete protocols without an alias. It does not yet complete
the standard authorization-error response lifecycle, and its confidential
client parser rejects a conforming HTTP Basic scheme spelling.
