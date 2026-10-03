# Maintainer Review — TASK-010 R3

## Immutable subject

- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `1f4466a9ae482eb1311f6e5206484758bc272ad8`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- Prior remediation: `changes/active/oidc-production-readiness/review/TASK-010-r1/TASK-010-R1-authorization-server-corrections.md`
- Current remediation direction: `changes/active/oidc-production-readiness/review/TASK-010-r2/lead-direction-FIND-TASK-010-10.md`, which supersedes the R2 remediation task

`FIND-TASK-010-1` remains routed to TASK-011 by
`review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md` and is not reopened.
The lead-directed closure of `FIND-TASK-010-10` is evaluated as written: no
image-local or application limiter, no edge manifest or new setting, and one
operator-facing public-ingress note.

## Changed-surface coverage

| Changed surface | Owners and symbols inspected | Callers, declarations, and proof inspected | Result |
|---|---|---|---|
| OAuth and OIDC contracts | `TokenRequest`, `TokenResponse`, `ExchangeTokenType`, `OAuthClientId`, `OAuthErrorCode`, `AuthorizationServerMetadata`, `DeviceAuthorizationRequest`, `TokenRevocationRequest`, `LoginInitiation`, `ClientAuthorization`, `CallbackQuery::response`, `ProviderResponse` | Schema generator, checked-in schemas and goldens, server form decoding, shared-client serialization, OpenAPI request projections, callback consumers | PASS |
| Authorization server HTTP surface | `authorize`, `metadata`, `OAuthForm`, `OAuthClients`, `OAuthError`, `ClientForm<T>`, `token`, `TokenGrants`, device authorization/decision/revoke handlers, callback adapter, auth router | Router composition, `ServerAuth`, boot/config registration, Basic and public-client identification, served OpenAPI security and request declarations, endpoint tests | PASS |
| Human-login domain owners | `AuthorizationCodeExchange::{execute,complete,verify_and_finish,finish_id_token_exchange,redeem_code}`, connection fencing and role synchronization, callback refusal mapping | Human-connection owner, login-state queries, tenant issuance, audit owner, provider callback and code-redemption callers, focused callback and identity tests | PASS |
| Device grant owner | `CliLogins::{authorize,approve,deny,redeem,revoke}`, user/device-code normalization and refusal mapping | Device/login-state query owners, provider callback completion, token route, client/CLI polling, denial/expiry/delete/concurrency and exactly-once tests | PASS |
| Refresh, revocation, and issuance | `RefreshTokens::execute`, human access/session issuance, `active_refresh`, `refresh_by_hash`, chain revocation, session binding | Token and revoke routes, RLS transaction owner, connection lifecycle checks, refresh/reuse/client-mismatch tests and journey assertions | PASS |
| Persistent auth state | Login-state, device-authorization, human-connection, and refresh-token queries and row types; auth migrations; removed browser-session queries and migrations | Domain writers/readers, RLS connection types, clean-migration and PG tests, sealing rewrap owner | PASS |
| Shared client and CLI projection | `TokenExchange::{exchange,platform_session,device_authorization,revoke_refresh_token,send,decode}`, middleware exchange paths, saved-login refresh, CLI output | Platform, API-key, workload, delegation, device, saved-login, CLI tests, and Python/TypeScript integration call sites | PASS |
| Generated/public declaration parity | JSON schemas, served OpenAPI components and operations, auth docs | Source contract types, `SecurityAddon`, schema generator, codegen and served-document tests | PASS — prior `MAINT-010-1` remains closed |
| Deleted private protocols | Removed BFF/session route owner, browser-session owner and SQL owner, sealed completion and narrowed sealing tables | Router/boot/lib exports, migrations, tests, docs, config, and repository references | PASS |
| Deployment correction for `FIND-TASK-010-10` | Official-image NGINX template, startup lane, self-hosting SSO/OIDC page | Deployment gateway authority, Docker composition, current repository search for device-login limiting mechanisms, lead direction and its focused evidence | PASS — the image-local `limit_req` mechanism and its assertions are deleted; the operator ingress obligation is documented; no replacement mechanism or option was added |
| Tests, fixtures, scripts, and documentation | Materially changed Rust tests/helpers, identity/OpenAPI/SQL/CLI tests, startup shell, language integration support, auth documentation | Test ownership and names, declaration parity, narrow task evidence, rustdoc on materially changed Rust items | PASS |

The cumulative base-to-candidate trace found each workflow on a cohesive owner:
HTTP extraction and RFC response shaping remain at the server edge; login,
device, refresh, issuance, and revocation behavior remain on their auth-domain
owners; tenant state stays behind `TenantConn`; and client projection stays in
`wyrd-client`. The candidate does not introduce another protocol owner,
durable authority, compatibility path, or review-only abstraction.

## Material findings

None.

## Prior-finding and remediation closure

- `FIND-TASK-010-1` remains routed to TASK-011's production
  `openid-client` journey and is outside this task-review round.
- Prior maintainer finding `MAINT-010-1` remains closed. `ClientForm<T>` and
  `SecurityAddon` publish the public `client_id` and ordinary HTTP Basic
  alternatives, and the served-document test asserts them.
- The R2 proposal `MAINT-010-R2-1` stays rejected by the independent R2
  validation. The startup lane explicitly provisions its one-tenant subject
  with the supported owner-only file KEK; requiring a new live Vault topology
  here would broaden this task and its proof surface.
- `FIND-TASK-010-10` is closed under the superseding lead direction. The
  candidate removes the per-image NGINX `map`, `limit_req_zone`,
  `limit_req_status`, and `limit_req`, removes the matching startup
  assertions, and documents that operators rate-limit `POST /auth/device` per
  client address at their public ingress. It adds no edge manifest,
  application limiter, forwarded-header trust, rate-limit state, option, or
  dependency.
- Previously corrected Rust documentation remains present on the materially
  changed owners and state transitions. No new blocking rustdoc omission was
  found.

## Non-blocking notes

- `auth_router`'s rustdoc still says device user-code attempts "are limited"
  at a gateway that sees one budget across replicas. The shipped repository
  now deliberately owns no such gateway configuration; the operator note is
  the durable deployment instruction. This is stale wording only. Under the
  standing direction, wording does not block and does not justify a new
  correction round.
- `HumanSessionBinding::client` and `ServerAuth::oauth_clients` use
  fully-qualified Rust type paths in fields instead of top-of-module imports.
  This is a local structural/readability issue without a behavioral or public
  contract consequence; placement, naming, and structure observations are
  non-blocking in this review.
- The R2 `verdict.md` ends with an extra blank line, so cumulative
  `git diff --check` reports that review-artifact formatting. It has no executable,
  contract, security, tenancy, durability, or documentation consequence and
  is not promoted to a finding.

## Uncertain preferences

- `ClientForm<T>` is an OpenAPI-only projection rather than the runtime
  extractor. It is small, local, and keeps the standard client-auth
  alternatives visible without introducing a second runtime parser; no
  replacement is warranted.
- The longer callback and refresh methods keep one transaction and its lock,
  audit, and state-transition order visible on the owning service. Splitting
  them for line count would risk obscuring those invariants, so no maintainer
  finding is proposed.
- The operator ingress note is deliberately brief. More deployment examples
  or edge-specific manifests would exceed the lead direction and comparable
  project practice, so they are not requested.

## Verification evidence and limits

The task and lead-direction records report green focused auth, SQL, OpenAPI,
codegen, boundary, startup, docs, formatting, lint, CLI, and filtered identity
lanes. This maintainer review independently inspected the cumulative diff,
current owners, callers, tests, generated declarations, and the final
lead-directed three-file correction. It did not rerun the recorded lanes.
That is the required narrowest-lane posture for task review; unfiltered
identity and every-language journeys run at change review. No missing journey
is converted into a task-review blocker, and the routed TASK-011 proof is not
reopened.

`HEAD` was `1f4466a9ae482eb1311f6e5206484758bc272ad8` before the report was written.

## Overall result

**PASS** — all materially changed owners, callers, tests, documentation, and
generated declarations were covered; no material maintainer finding remains.
