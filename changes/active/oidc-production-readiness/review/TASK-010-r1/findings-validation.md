# TASK-010 R1 independent findings validation

## Validation method and completeness

Reviewed immutable cumulative range
`fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa..04366e7fc28c466fcdcbc7279885cfee82a988a2`
against approved spec revision 11, the original TASK-010, the routed TASK-004 R2
directions and FIND-TASK-004-12 diagnosis, `AGENTS.md`,
`architecture/agent-rules.md`, the security posture, deployment, architecture,
Rust, testing, and error authorities, the cumulative diff, and every required R1
report: `task-review-behavior.md`, `task-review-invariants.md`,
`standards-review.md`, `maintainer-review.md`, `system-review.md`, all three
domain reports, and `followup-review.md`. `.codegraph/` is absent. `HEAD` was
`04366e7fc28c466fcdcbc7279885cfee82a988a2` before and after source inspection;
the candidate was not changed. All required reports and source were present, so
no structured-validation `BLOCKED` condition applies.

For every proposal I independently read the complete correction owner and
traced its producers, consumers, sibling paths, durable writers, and tests. The
main traces were authorize query -> downstream redirect, provider callback ->
login-state consumption -> final tenant transaction, device lookup -> callback
approval -> token redemption, OAuth form/client extraction -> grant dispatch,
refresh-row lookup -> family lock -> lifecycle classification -> containment,
`TenantConn` -> RLS query, utoipa annotation -> served OpenAPI -> generated
client consumer, and TCP peer -> router governor -> official gateway -> replica.
I also checked the binding API-key decision and the deleted BFF surface: the
accepted API-key path is RFC 8693 with
`subject_token_type=urn:wyrd:oauth:token-type:api_key`; the old
`grant_type=wyrd_api_key` is rejected and has no alias. Remaining UI callers of
the deleted `/internal/bff/v1` surface are TASK-011 consumers and are not
retained here.

Primary standards checked were RFC 6749 §§2.3.1 and 4.1.2.1, RFC 8628 §5.1,
RFC 8693 §2.2.2, RFC 9110 §11.1, and RFC 9700 §4.14.2. For DRIFT calibration I
also checked the conventional existing boundaries in [NGINX's native request
limiter](https://nginx.org/en/docs/http/ngx_http_limit_req_module.html),
[Keycloak's reverse-proxy guidance](https://www.keycloak.org/server/reverseproxy)
and distributed login-failure storage, and [Ory Hydra's request-id token-chain
containment](https://github.com/ory/hydra/blob/master/persistence/sql/persister_oauth2.go).
This is a static source-and-evidence validation; I did not run product lanes.
Closure proofs below use the task's narrow named lanes. Unfiltered journeys and
cross-language sweeps remain change-review work.

## Disposition of every proposal

| Proposal source | Disposition | Independent validation |
| --- | --- | --- |
| `BEHAVIOR-TASK-010-1` | **CONFIRMED** -> `FIND-TASK-010-1` | The only real-server code-grant journey hand-builds authorize and token requests. No `openid-client` dependency, driver, or test exists outside the task packet. TASK-011 may replace the BFF, but TASK-010 itself explicitly requires `openid-client` 6.8.8 proof. |
| `BEHAVIOR-TASK-010-2` | **CONFIRMED** -> `FIND-TASK-010-2` | The guarded callback update and atomic redemption close the old issuance defect in source, but the exact routed pause/terminal/resume interleaving has no deterministic proof. Sequential denial/expiry and one-use tests do not exercise that race. |
| `BEHAVIOR-TASK-010-3` | **CONFIRMED** -> `FIND-TASK-010-3` | A syntactically valid unsupported token-exchange audience fails closed-enum deserialization and is flattened to `invalid_request`; RFC 8693 provides `invalid_target` for precisely this target refusal. |
| `INV-REV-001` | **REVISED, split** -> `FIND-TASK-010-4` and `FIND-TASK-010-5` | The two observations are valid but do not share a producer or complete correction boundary. Duplicate request parameters fail in authorize parsing; upstream provider denial is rejected by callback query extraction before login-state consumption. |
| `INV-REV-002` | **CONFIRMED** -> `FIND-TASK-010-6` | `strip_prefix("Basic ")` rejects conforming case variants although RFC 9110 makes the authentication-scheme token case-insensitive. |
| `REPO-TASK-010-1` | **CONFIRMED** -> `FIND-TASK-010-7` | All four cited items are new/materially modified and violate the explicit hard rustdoc rule. This is the standing exception to the non-blocking wording rule. |
| `REPO-TASK-010-2` | **REVISED** -> `FIND-TASK-010-8` | The new `active_refresh` query duplicates tenant selection under `TenantConn`. This is DRIFT, not a request for another check: delete the predicate and binding and rely on the existing RLS owner. |
| `MAINT-010-1` | **CONFIRMED** -> `FIND-TASK-010-9` | Runtime accepts `client_id` form identification and RFC 6749 Basic authentication, while the three served operation contracts expose neither complete alternative. The current OpenAPI test checks only body media type/grant text and forbids any second scheme. |
| `FIND-SYS-1` | **REVISED** -> `FIND-TASK-010-10` | The defect is reachable, but a fleet-aware application limiter would itself be nonstandard DRIFT. The existing deployment authority already assigns rate-limit integration to the gateway, and RFC 8628 calls for limiting user-code attempts rather than sharing one budget across every auth route. |
| `SEC-OAUTH-001`, `PERSIST-CONC-001` | **REVISED, merged** -> `FIND-TASK-010-11` | Both reports identify one classification/containment defect. The implementation treats every inactive CLI row as replay and revokes every active row for the principal, while the RFC and conventional projects contain the compromised grant/token chain. |
| `TA-001` | **CONFIRMED** -> `FIND-TASK-010-12` | Callback completion rechecks the connection in a separate committed transaction, then changes the User, roles, code/device approval without the lifecycle slot lock. The existing multi-replica test demonstrates the stale callback can commit a code after deactivation. |
| `TA-002` | **CONFIRMED** -> `FIND-TASK-010-13` | A successful callback with unchanged roles commits a code or device approval with no canonical login-outcome event. Redemption audit cannot cover an abandoned callback outcome. |
| `followup-review.md` | **Validated; no additional proposal** | Its TA, device-proof, rustdoc, and system-governor resolutions agree with the independent traces above. |

## Deduplicated retained finding ledger

### FIND-TASK-010-1 — CONFIRMED / MISSING — required `openid-client` 6.8.8 proof is absent

- **Discovery sources:** `BEHAVIOR-TASK-010-1`.
- **Violated obligation:** TASK-010 Scenario 1 and acceptance lines 84-95 and
  168-170 require the off-the-shelf `openid-client` 6.8.8 client to complete
  authorization code + S256 PKCE against a real server, including the named
  negatives. REQ-021 requires a standard OAuth client to work unchanged.
- **Exact location and evidence:**
  `crates/wyrd/wyrd-server/tests/identity_e2e.rs:1268-1299,1581-1675`
  constructs the authorize URL, Basic header, form body, and replay request by
  hand. The repository has no `openid-client` dependency or driver. The task's
  implementation-evidence line 247 expressly defers this proof to TASK-011.
- **Observable consequence:** Server-owned helpers can agree with server-owned
  parsing while an actual standards client still cannot complete discovery,
  PKCE, callback processing, or token redemption. The task's named acceptance
  remains unproved; this is independent of deleted `/internal/bff/v1` callers.
- **Decision-complete minimal correction:** Add one focused real-server driver
  using exactly `openid-client` 6.8.8 to the existing identity-test lifecycle.
  It must use RFC 8414 discovery and the real `/auth/authorize` and
  `/auth/token` surfaces, complete code + S256 as `wyrd-ui`, and exercise the
  task-named wrong/expired/replayed code, PKCE, redirect, and client-secret
  refusals. Reuse the current real server, provider fixture, and registered
  client; do not add a BFF compatibility route or a second OAuth implementation.
- **Focused closure proof:** Run the new exact test command and the filtered
  identity journey through
  `mise exec -- env WYRD_IDENTITY_TARGET=server WYRD_IDENTITY_FILTER=<exact-test> mise run test:identity:journey`.
  Full identity and language sweeps wait for change review.

### FIND-TASK-010-2 — CONFIRMED / MISSING — routed terminal-device interleaving has no regression proof

- **Discovery sources:** `BEHAVIOR-TASK-010-2`; corroborated by
  `followup-review.md`; carries routed `FIND-TASK-004-12`.
- **Violated obligation:** TASK-010 Scenario 2 lines 98-118 and acceptance lines
  175-177 require the exact pause-after-lookup terminal-state proof. AC-007
  requires denied, expired, deleted, and already-redeemed grants to yield no
  credential or session.
- **Exact location and evidence:** `CliLogins::approve` reads the pending row and
  commits before `begin_bound`
  (`crates/wyrd/wyrd-auth/src/cli_logins.rs:192-215`). Callback completion uses
  the correct live-state predicate
  (`callback.rs:418-431`;
  `wyrd-sql/src/queries/auth/device_authorizations.rs:58-69,214-227`), and
  redemption locks/deletes/issues in one transaction (`cli_logins.rs:310-384`).
  `device_codes_poll_approve_deny_and_expire` and
  `an_approved_device_code_issues_exactly_once` cover only sequential states;
  no test pauses that producer/consumer boundary.
- **Observable consequence:** The source correction is plausible and the old
  callback-mint path is gone, but the routed concurrency regression can return
  without detection. Sequential green tests do not close the prior finding's
  acceptance requirement.
- **Decision-complete minimal correction:** At the existing real-server/device
  test owner, deterministically park approval after its live-row lookup and
  before the bound login-state insert using the existing Postgres lock/waiter
  pattern. From a second actor deny the grant, and separately expire then
  poll-delete it; resume provider completion and prove the guarded approval
  commits no User authority, code, device approval, access token, refresh token,
  or refresh row. Retain the existing normal exactly-once redemption case. Do
  not add a timing sleep, production hook, or concurrency framework.
- **Focused closure proof:** Run each new named test with an exact
  `mise exec -- cargo nextest run` selector under the repository Postgres
  wrapper, plus only its filtered identity journey if the proof lives there.

### FIND-TASK-010-3 — CONFIRMED / INCORRECT — unsupported exchange audience returns the wrong registered error

- **Discovery sources:** `BEHAVIOR-TASK-010-3`.
- **Violated obligation:** REQ-021 and TASK-010 Scenario 4 require RFC 8693 §2
  errors. RFC 8693 §2.2.2 says an unsupported requested target service should
  use `invalid_target`.
- **Exact location and evidence:** `TokenRequest::TokenExchange.audience` is a
  closed `Option<TokenAudience>`
  (`crates/wyrd-spec/src/auth/token.rs:52-64,87-100`). `OAuthForm::decode`
  maps every serde failure to `InvalidRequest`
  (`crates/wyrd/wyrd-server/src/auth/oauth.rs:175-183`), and
  `OAuthErrorCode` has no `InvalidTarget` (`token.rs:179-209`). Token dispatch
  only sees successfully decoded values
  (`wyrd-server/src/components/auth/routes.rs:147-190`).
- **Observable consequence:** A conforming token-exchange client cannot
  distinguish a malformed request from a valid request for an audience Wyrd
  cannot serve and cannot apply RFC 8693 target remediation.
- **Decision-complete minimal correction:** Add the registered
  `invalid_target` OAuth error and classify a present token-exchange audience
  against the existing `TokenAudience` set before generic request decoding.
  Preserve `invalid_request` for malformed requests and unusable subject/actor
  tokens, the approved API-key subject-token type, and the no-alias deletion of
  `grant_type=wyrd_api_key`.
- **Focused closure proof:** Exact `wyrd-spec` decode/serialization test and
  exact token-route test proving unsupported audience -> `400 invalid_target`,
  malformed exchange -> `invalid_request`, and both supported audiences still
  dispatch; then `mise run codegen:check` and
  `mise run test:principals:integration`.

### FIND-TASK-010-4 — REVISED / INCORRECT — duplicate non-binding authorize parameters bypass the registered redirect

- **Discovery sources:** first half of `INV-REV-001`.
- **Violated obligation:** RFC 6749 §4.1.2.1 and TASK-010's authorization-error
  contract require errors other than missing/invalid client or redirect URI to
  return to the already validated registered redirect with the original state.
- **Exact location and evidence:** `OAuthForm::parse` rejects every duplicate
  before returning a form (`wyrd-server/src/auth/oauth.rs:148-167`).
  `authorize` discards that error while attempting to recover client and
  redirect and answers a local 400 page
  (`wyrd-server/src/auth/authorize.rs:75-95`). The contrary test includes a
  duplicate `state` in the unsafe-local-page group
  (`authorize.rs:432-451`). Exact redirect comparison remains correct.
- **Observable consequence:** A standards client receives a local HTML error
  instead of its registered callback and loses the state-correlated OAuth error,
  even though neither client identity nor redirect safety failed.
- **Decision-complete minimal correction:** Keep rejecting repeated parameters,
  but separate safe extraction/validation of the sole `client_id` and
  `redirect_uri` from validation of the remaining authorization request. Once
  those two binding values are uniquely present and exact, route duplicate
  `state`, `response_type`, PKCE, tenant, and other non-binding failures through
  the existing `ClientAuthorization`/`client_redirect` path as
  `invalid_request`, echoing state only when it has one unambiguous value. Keep
  missing, duplicated, invalid, or mismatched client/redirect local and never
  follow caller-controlled URIs.
- **Focused closure proof:** Exact authorize tests covering duplicate state and
  other non-binding fields -> registered redirect/`invalid_request`, alongside
  duplicated or mismatched client/redirect -> local 400/no Location. Run the
  exact test selectors and `mise run test:principals:integration`.

### FIND-TASK-010-5 — REVISED / MISSING — upstream provider denial never reaches downstream OAuth error handling

- **Discovery sources:** second half of `INV-REV-001`.
- **Violated obligation:** RFC 6749 §4.1.2.1, REQ-021, and the task require a
  provider/resource-owner refusal after a validated downstream authorization
  request to return an OAuth error to that registered client.
- **Exact location and evidence:** `CallbackQuery` requires `code` and `state`
  and models no upstream `error`
  (`crates/wyrd-spec/src/auth/oidc.rs:378-398`). Axum extracts that type before
  the callback handler (`wyrd-server/src/components/auth/routes.rs:468-481`), so
  a normal provider `error=access_denied&state=...` response never reaches
  login-state consumption. `AuthorizationCodeExchange::complete` already owns
  state consumption and converts post-consume authorize failures into
  `LoginCompletion::Refused` (`wyrd-auth/src/callback.rs:192-226`), but has no
  upstream-denial entry path.
- **Observable consequence:** The browser gets a Wyrd extraction error instead
  of returning to the registered client; the bound state is not consumed by the
  normal callback owner and downstream state/error correlation is lost.
- **Decision-complete minimal correction:** Extend the existing callback wire
  and `AuthorizationCodeExchange` owner to accept exactly one standard upstream
  success (`code`) or error result with required state. For an error, consume
  the same server-bound login state, validate optional response issuer under the
  existing rule, and return `LoginCompletion::Refused` through
  `client_redirect`, mapping provider denial to downstream `access_denied` and
  server/unavailable cases with the existing OAuth mapping. Do not reflect
  provider descriptions, trust query tenant data, or create a second callback
  route/state store.
- **Focused closure proof:** Exact callback-route tests for provider
  `access_denied`, malformed success+error/missing-state inputs, single state
  consumption/replay refusal, and registered redirect/state echo; then the
  filtered callback refusal identity journey.

### FIND-TASK-010-6 — CONFIRMED / INCORRECT — HTTP Basic scheme matching is case-sensitive

- **Discovery sources:** `INV-REV-002`.
- **Violated obligation:** RFC 9110 §11.1 makes the HTTP authentication-scheme
  token case-insensitive; RFC 6749 §2.3.1 and TASK-010 require interoperable
  `client_secret_basic`.
- **Exact location and evidence:** `OAuthClients::identify` uses literal
  `strip_prefix("Basic ")`
  (`crates/wyrd/wyrd-server/src/auth/oauth.rs:242-283`). Existing tests cover
  only preferred capitalization. Secret comparison and conflicting form
  `client_id` handling are otherwise shared and correct.
- **Observable consequence:** Conforming clients sending `basic`, `BASIC`, or
  another case variant receive `401 invalid_client` with no credential error.
- **Decision-complete minimal correction:** Split the Authorization value once
  at required whitespace, compare only the scheme with
  `eq_ignore_ascii_case("basic")`, and feed the unchanged token68 credentials
  into the existing Base64/form-decoding and constant digest comparison. Do not
  add a second authentication method or permissive whitespace grammar.
- **Focused closure proof:** Extend the exact `OAuthClients` unit test with
  preferred, lower, upper, malformed, and wrong-secret cases; run that exact
  selector and `mise run test:principals:integration`.

### FIND-TASK-010-7 — CONFIRMED / VIOLATION — four touched Rust items lack mandatory rustdoc

- **Discovery sources:** `REPO-TASK-010-1`; corroborated by
  `followup-review.md`.
- **Violated obligation:** `AGENTS.md:716-730` and
  `architecture/agent-rules.md:35` require substantive rustdoc on every new or
  materially modified item, including tuple fields, associated types, test
  helpers, and `# Errors` for fallible functions. Missing rustdoc is explicitly
  `BLOCK_BEFORE_MERGE`; this preserved hard rule is not downgraded as wording.
- **Exact location and evidence:** The public tuple field in
  `OAuthError(pub OAuthErrorCode)` at
  `crates/wyrd/wyrd-server/src/auth/oauth.rs:30-32`; `FromRequest::Rejection` at
  `oauth.rs:186-187`; fallible async `from_request` without `# Errors` at
  `oauth.rs:189-208`; and fallible test helper `parse` without `# Errors` at
  `crates/wyrd-spec/src/auth/token.rs:257-264` are all new in the candidate.
- **Observable consequence:** The changed contract does not meet the
  repository's explicit implementation-completeness gate even though it
  compiles.
- **Decision-complete minimal correction:** Document the tuple field's carried
  registered code, the associated rejection type, the extractor's exact error
  conditions in `# Errors`, and the test helper's serde failure in `# Errors`.
  Use the existing items; add no lint, allowlist, check, or wrapper.
- **Focused closure proof:** Source inspection of the four items, then
  `mise run fmt` and `mise run lints`.

### FIND-TASK-010-8 — REVISED / DRIFT — new `TenantConn` query repeats tenant selection manually

- **Discovery sources:** `REPO-TASK-010-2`.
- **Violated obligation:** `architecture/agent-rules.md:11` and
  `architecture/references/architecture/patterns.md:172-174` make forced RLS
  through `TenantConn` the sole tenant selector and forbid parallel hand-written
  predicates.
- **Exact location and evidence:** New `ACTIVE_REFRESH_SQL` adds
  `data_tenant_id = $2`, and `active_refresh` retrieves and binds the same tenant
  (`crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs:46-56,160-179`).
  Its only caller is the `wyrd-ui` non-rotating branch after `refresh_by_hash`
  and the principal-family lock (`wyrd-auth/src/refresh.rs:136-163`); the
  supplied connection is already RLS-bound. The module's index-benefit claim
  cannot override the load-bearing architecture rule.
- **Observable consequence:** Tenant isolation has a second, independently
  editable expression on the new path, obscuring which mechanism is
  authoritative and allowing predicate/RLS drift without adding isolation.
- **Decision-complete minimal correction:** Delete only the manual
  `data_tenant_id` predicate, local tenant extraction, second bind, and stale
  explanatory claim for this newly added query. Keep the token-hash,
  active-state, and database-clock predicates and continue using the existing
  `TenantConn` RLS transaction. Do not add a check or alternate query wrapper.
- **Focused closure proof:** Exact SQL-backed `active_refresh` tests for active,
  revoked, expired, and cross-tenant same-hash visibility through separate
  `TenantConn`s; run their exact selectors and `mise run test:sql`.

### FIND-TASK-010-9 — CONFIRMED / INCORRECT — served OpenAPI omits OAuth client identification

- **Discovery sources:** `MAINT-010-1`.
- **Violated obligation:** REQ-021 and AC-009 require public contracts to match
  the accepted OAuth wire so independent generated clients work unchanged.
- **Exact location and evidence:** Runtime `OAuthClients` accepts public-client
  `client_id` in the form or confidential `client_secret_basic`
  (`wyrd-server/src/auth/oauth.rs:231-298`) before decoding partial payload
  structs. Those structs explicitly exclude client identification
  (`wyrd-spec/src/auth/token.rs:1-8`;
  `wyrd-spec/src/auth/device.rs:17-27,49-58`). The utoipa annotations publish
  only those partial types for `/auth/token`, `/auth/device_authorization`, and
  `/auth/revoke` (`components/auth/routes.rs:107-125`;
  `auth/cli_login.rs:54-74,279-293`) and clear security. The served-contract test
  asserts only form content/grant text
  (`pg_openapi_contract.rs:284-339`), while its blanket security test rejects any
  operation scheme besides the unrelated Wyrd access-token header
  (`pg_openapi_contract.rs:514-559`).
- **Observable consequence:** Generated clients cannot discover how to identify
  `wyrd-cli` or authenticate `wyrd-ui`, so the published contract cannot produce
  calls the server accepts.
- **Decision-complete minimal correction:** At the existing utoipa/OpenAPI
  owner, publish the complete form request shape including optional
  `client_id`, register ordinary RFC 7617 HTTP Basic for the confidential OAuth
  client, and describe each route's accepted public-form or confidential-Basic
  alternatives. Keep runtime extraction centralized in `OAuthClients`, retain
  the document-wide Wyrd session scheme for protected APIs, and do not invent a
  Wyrd authentication scheme or JSON body.
- **Focused closure proof:** Extend
  `tenant_login_operations_publish_their_contract` to assert form `client_id`
  and Basic alternatives on all three operations and generated-client-usable
  schemas; narrowly revise the blanket scheme assertion for these standard
  OAuth operations. Run the exact test and
  `mise run test:principals:integration` plus `mise run codegen:check`.

### FIND-TASK-010-10 — REVISED / DRIFT — the all-auth TCP-peer governor is neither client nor fleet admission

- **Discovery sources:** `FIND-SYS-1`; revised by `followup-review.md`.
- **Violated obligation:** TASK-010 requires RFC 8628 §5.1 user-code attempt
  limiting. `AGENTS.md:690` and
  `architecture/operations/deployment-and-release.md:8-24` require multi-replica
  operation behind one gateway and assign rate-limit integration to that
  gateway. Standing direction rejects a bespoke mechanism absent from the RFCs
  and comparable projects.
- **Exact location and evidence:** `auth_router` puts authorize, metadata,
  device, callback, token, revoke, and key issuance behind one default
  `tower_governor` instance
  (`crates/wyrd/wyrd-server/src/components/auth/routes.rs:46-84`). Version 0.8's
  default is the TCP `PeerIpKeyExtractor`; `serve` supplies only peer
  `SocketAddr` (`wyrd-server/src/app/serve.rs:15-24`). The official NGINX gateway
  forwards real-address headers (`docker/official/extras/nginx/nginx.conf.template:40-60`),
  but the extractor does not use them. Thus all clients behind a gateway share
  one per-replica bucket, and requests distributed to other replicas get fresh
  buckets. `identity_e2e::auth_call` retries every 429 until admitted
  (`identity_e2e.rs:1229-1258`), masking both effects. RFC 8628 §5.1 recommends
  limiting user-code attempts; it does not define an all-auth shared governor.
  NGINX conventionally provides a gateway-observed client-keyed shared zone,
  while Keycloak either trusts explicitly configured proxy identity or stores
  brute-force state in shared durable infrastructure.
- **Observable consequence:** One external caller can consume the replica's
  budget for all other callers behind that gateway; adding replicas bypasses
  the intended protection; unrelated metadata, refresh, callback, and
  revocation calls compete with device verification.
- **Decision-complete minimal correction:** Remove the server-local shared
  governor and its retry-masking helper behavior. Use the existing gateway's
  native rate-limit integration at the RFC-required user-code verification
  attempt boundary, keyed by the client address observed and overwritten by the
  trusted gateway. Preserve high-entropy/finite-lifetime device codes and the
  token endpoint's existing RFC 8628 polling interval/`slow_down` behavior. Do
  not add a database limiter, trusted-header parser in Wyrd, distributed cache,
  new rate-limit setting, or fleet coordination layer.
- **Focused closure proof:** Validate the rendered existing gateway
  configuration, then make focused device-verification attempts from two real
  client addresses across multiple Wyrd replicas: one client's excess attempts
  are limited without throttling the other, and replica selection does not
  reset admission. Prove unrelated auth routes do not share the bucket. Run the
  narrow official-container/startup check and filtered device journey; fleet and
  full journeys remain change-review gates.

### FIND-TASK-010-11 — REVISED / DRIFT — inactive refresh rows trigger principal-wide theft containment

- **Discovery sources:** merged `SEC-OAUTH-001` and `PERSIST-CONC-001`.
- **Violated obligation:** REQ-012, the security posture
  (`architecture/wyrd-security-posture.md:149-151`), and RFC 9700 §4.14.2 require
  replay of a **rotated** refresh token to revoke the active refresh token(s) of
  that authorization grant/token family. Expiry, logout, or administrative
  revocation are ordinary inactive states. Conventional Hydra containment keys
  the token chain by request/grant identity, not by the human principal.
- **Exact location and evidence:** After `refresh_by_hash` and the existing
  principal-family serialization lock, every failure of
  `consume_active_refresh`—rotated, revoked, or expired—calls
  `revoke_refresh_family(principal_kind, principal_id)` and records `Reused`
  (`crates/wyrd/wyrd-auth/src/refresh.rs:129-211`). That SQL revokes all active
  rows for the principal (`wyrd-sql/src/queries/auth/refresh_tokens.rs:245-273`).
  The same owner already has `revoke_refresh_chain(root_id)`, whose recursive
  `rotated_from` traversal explicitly preserves other sessions
  (`refresh_tokens.rs:207-243`) and is used by logout. Current tests pin the
  defect: `reuse_detection_revokes_family` revokes an unrelated sibling and
  `expired_token_is_rejected` expects `Reused`
  (`refresh.rs:638-688,772-817`). Concurrent ancestor replay is otherwise
  correctly serialized.
- **Observable consequence:** Presenting an expired or logged-out CLI token can
  kill every other live CLI login for that User and emit a false theft event;
  genuine replay of one chain also terminates unrelated grants/sessions.
- **Decision-complete minimal correction:** Under the existing family lock,
  classify the stored row explicitly. Only `revoked_reason == "rotated"`
  triggers replay containment and canonical replay audit; call the existing
  `revoke_refresh_chain(stored.id, "reuse_detected")` so the replayed row's
  active descendants are retired. Expired rows and rows revoked for logout,
  administration, or prior containment return the ordinary inactive-token
  refusal without a theft event or collateral revocation. Keep unknown/client
  mismatch indistinguishable, UI non-rotation, and the present lock order. Do
  not add a grace-period knob, principal-wide option, second table, or new
  containment mechanism.
- **Focused closure proof:** Exact SQL/auth tests proving rotated-ancestor replay
  revokes its current descendant under the existing concurrent race; unrelated
  login chains remain active; expired/logout/admin rows return inactive
  `invalid_grant` and stage no replay event. Run exact selectors plus
  `mise run test:principals:integration` and `mise run test:sql`.

### FIND-TASK-010-12 — CONFIRMED / INCORRECT — callback completion is not fenced by connection lifecycle

- **Discovery sources:** `TA-001`; corroborated by `followup-review.md`.
- **Violated obligation:** REQ-016 requires connection replacement,
  deactivation, or deletion to block new login immediately; INV-007 requires the
  same tenant connection authority on every login path. The established lock
  order is refresh family, then human-connection slot.
- **Exact location and evidence:** `verify_and_finish` reads the active binding
  via `active_connection` in a separate transaction
  (`crates/wyrd/wyrd-auth/src/callback.rs:243-299`;
  `connections.rs:631-666`). `finish_id_token_exchange` repeats that detached
  read, opens a new `TenantConn`, ensures the User, takes only the family lock,
  changes roles, and commits a code or device approval
  (`callback.rs:342-442`). Lifecycle writers take
  `lock_human_connection_slot` (`connections.rs:703-719`), and token issuance
  already reuses family -> slot -> exact active-binding check
  (`issuance.rs:727-755`). The existing
  `tenant_connection_session_cutoff_journey` deterministically deactivates while
  callback role work is parked, then observes that the stale callback still
  returns and commits a code; only later redemption refuses it
  (`identity_e2e.rs:3422-3565`).
- **Observable consequence:** After deactivation commits, an in-flight callback
  can still create/update the User, replace durable roles, commit a code or
  device approval, and report success. Redemption prevents a token but does not
  undo those committed login effects.
- **Decision-complete minimal correction:** In the existing final callback
  transaction, after acquiring the User family lock, acquire the existing
  human-connection slot lock and call the existing exact
  `human_connection_is_active(connection_id, revision)` predicate before role,
  code, or approval commit. On mismatch, return the existing inactive/invalid
  login refusal so all earlier writes in that transaction roll back. Preserve
  pre-provider state consumption, provider verification, family-before-slot
  order, connection-test behavior, and redemption's defense-in-depth check.
- **Focused closure proof:** Modify the existing deterministic multi-replica
  cutoff case to require callback refusal and assert no new User/role/code/device
  approval/success audit, plus an unchanged-connection success. Run its filtered
  identity command and the exact callback-owner tests; the unfiltered journey
  remains change-review scope.

### FIND-TASK-010-13 — CONFIRMED / REGRESSION — unchanged-role callback success has no canonical login audit

- **Discovery sources:** `TA-002`; corroborated by `followup-review.md`.
- **Violated obligation:** REQ-017 requires login outcomes and role changes to
  produce redacted canonical audit evidence, with required audit failure unable
  to establish a session/login outcome. INV-007 requires one audit authority.
- **Exact location and evidence:** The callback transaction appends only
  `auth.user.roles.sync`, and only when `replace_user_roles` changes rows, before
  committing an authorization code or device approval
  (`crates/wyrd/wyrd-auth/src/callback.rs:385-441`). The contrary tests assert
  that a successful callback has no audit until code redemption and that a
  repeated unchanged-role callback has no sync event
  (`wyrd-server/src/auth/callback.rs:223-276,408-449`). Redemption's
  `auth.token.exchange` and `auth.device_code.grant` events occur only when a
  code/device code is later presented, so an abandoned successful callback has
  no login-outcome evidence. Connection-test outcomes already use the canonical
  append through their owning path.
- **Observable consequence:** Successful provider authentication can establish
  a User and durable pending grant without any canonical outcome record when
  roles were unchanged; an unredeemed code or device approval is permanently
  absent from audit history.
- **Decision-complete minimal correction:** For every successful non-test
  callback, append one redacted allowed login-outcome event through the existing
  `append_auth_audit` path in the same final tenant transaction as the role
  update and code/device approval. Keep `auth.user.roles.sync` as a separate
  conditional role-change event and keep redemption's token/device-grant events
  as issuance evidence. If the login-outcome append fails, roll back User/roles
  and code/approval. Add no audit sink, relay, table, or best-effort path.
- **Focused closure proof:** Exact callback tests for unchanged-role authorize
  and device success each producing one login-outcome event, changed roles
  producing login + role-sync separately, and injected login-audit failure
  rolling back User/roles/code/approval. Run their exact selectors and
  `mise run test:principals:integration`.

## Rejected proposal portions and non-findings

- No complete Wave 1 proposal was rejected. `INV-REV-001` was rejected as a
  single finding because its two valid observations have different producers,
  owners, consequences, and closure tests; they are retained separately.
- The server-side fleet-aware limiter implied by the original `FIND-SYS-1`
  remediation is rejected as DRIFT. RFC 8628 requires user-code attempt
  limiting, the repository assigns rate-limit integration to the gateway, and
  NGINX/Keycloak use gateway client identity or shared durable defenses rather
  than a replica-local all-route bucket.
- Principal-wide refresh revocation and “any inactive row is theft” are rejected
  as the correction model. RFC 9700 and Hydra's conventional request/grant chain
  identify the narrower existing `rotated_from` chain owner.
- The remaining `wyrd_api_key` string in an error-catalog test fixture is stale
  wording, not an accepted alias; the parser explicitly rejects that grant.
  Placement/naming/wording-only cleanup is non-blocking under standing direction.
- UI/BFF files still calling `/internal/bff/v1` are TASK-011 consumers. Their
  failures are not TASK-010 gaps where deletion of that route is the only cause.
- No finding requires a full unfiltered journey, a new repository check, a
  compatibility alias, or a novel configuration option.

## Prior FIND-TASK-004-12 closure

**NOT CLOSED as an acceptance item.** The implementation closes its two source
causes: callback completion stores only a guarded approval, and token redemption
locks, revalidates, deletes, issues, audits, and commits atomically. Denied,
expired, or deleted rows therefore cannot be approved by the callback predicate,
and a live approved row issues once. However, the routed prior finding required
the deterministic pause-after-lookup terminal-state regression. That proof is
absent, so closure remains represented solely by `FIND-TASK-010-2`; there is no
second retained device-state finding.

## Final recommendations and validation result

Apply the retained corrections at their existing owners. The dependency order
with the least rework is: callback lifecycle fence and login audit; device-race
proof; refresh classification/chain containment; OAuth redirect, provider-error,
Basic, and `invalid_target` wire fixes; complete served OpenAPI; remove the RLS
and governor drift; add the required `openid-client` proof and rustdoc. Verify
each with its listed exact/focused selector and the task's narrow owner lanes.
Do not run or require the unfiltered cross-language/full journey suite until
change review.

**Validation result: COMPLETE.** Thirteen deduplicated retained findings remain:
eight confirmed proposal IDs and four revised proposal groups yielding thirteen
correction boundaries. No source/reviewer/authority conflict requires
`BLOCKED`, and no retained correction requires a spec revision, compatibility
surface, bespoke distributed mechanism, or TASK-011 BFF work.
