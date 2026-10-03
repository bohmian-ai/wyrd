# TASK-010 R3 behavior review

## Immutable subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `1f4466a9ae482eb1311f6e5206484758bc272ad8`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- Current remediation authority:
  `changes/active/oidc-production-readiness/review/TASK-010-r2/lead-direction-FIND-TASK-010-10.md`

The candidate resolved to the stated commit before and after inspection.
`.codegraph/` is absent, so this review used the immutable Git range and
repository source directly. I reviewed the complete cumulative range, the R1
and R2 findings and verdicts, the R1 remediation, the current lead direction,
the applicable repository and deployment authorities, the changed owners,
their reachable callers, and the recorded focused verification. The
superseded `TASK-010-R2-public-edge-device-admission.md` was not treated as
authority. `FIND-TASK-010-1` remains routed to TASK-011 by standing lead
direction and was not reopened.

The principal caller-to-result traces were:

- authorize query -> unique registered client/redirect binding -> provider
  login state -> common callback -> hashed authorization code -> atomic token
  redemption;
- device authorization -> user-code decision -> provider callback records
  approval only -> locked device-code poll -> issue-and-delete transaction;
- authenticated refresh client -> tenant routing -> family lock ->
  confidential access-only renewal or public rotation/reuse containment;
- form extractor and OAuth client authentication -> grant dispatch -> shared
  RFC success/error response;
- revocation form -> token/client binding -> login-local chain revocation;
- typed route annotations -> served OpenAPI and RFC 8414 metadata; and
- public ingress -> bundled image NGINX -> device verification route, including
  the R3 removal of the incorrect image-local admission mechanism.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Authorization code grant uses `wyrd-ui`, exact redirect binding, S256 PKCE, a hashed code with at most a 60-second lifetime, and one-use redemption | `auth/authorize.rs::authorize`; `wyrd-auth/src/callback.rs::AuthorizationCodeExchange`; `wyrd-sql/src/queries/auth/login_state.rs::issue_authorization_code` and `redeem_authorization_code` | Recorded authorize/callback PostgreSQL tests and filtered `tenant_human_login_journey` / `tenant_callback_refusal_journey` | PASS |
| Wrong, expired, or replayed code, PKCE mismatch, wrong redirect, and wrong secret issue no token; `invalid_client` is 401 with a Basic challenge | `AuthorizationCodeExchange::redeem_code`; `OAuthClients::identify`; `OAuthError::into_response` | Recorded callback-refusal journey and OAuth route/unit tests | PASS |
| The named production `openid-client` proof is not reopened in TASK-010 | `review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md` assigns that BFF proof to TASK-011; no compatibility channel is retained here | Integrated BFF proof remains TASK-011/change-review work by explicit human direction | PASS / ROUTED |
| Device authorization stores no issued credential; callback records only the approving principal and connection; live redemption mints once | `CliLogins::authorize`, `AuthorizationCodeExchange::finish_id_token_exchange`, `CliLogins::redeem_in`; device rows contain approval state, not tokens | Recorded `an_approved_device_code_issues_exactly_once` and device refusal journey | PASS |
| Pending, fast polling, denial, expiry, deletion, and replay return the RFC 8628 results and leave no issued token/session/refresh row | `poll_device_authorization` locks the row; `redeem_in` classifies and deletes terminal rows before issuance; guarded callback approval updates only a still-live undecided row | Recorded `device_codes_poll_approve_deny_and_expire`, `a_denial_during_approval_wins`, `an_expiry_deleted_during_approval_wins`, and `device_grant_refusal_journey` | PASS |
| RFC 8628 user-code attempt admission follows the current fixed correction: no in-image limiter; operator ingress limit documented; no Wyrd limiter, option, state, or edge manifest | Candidate deletes every `map`, `limit_req_zone`, `limit_req_status`, and `limit_req` line from the official image template; removes only their startup assertions; `sso-and-oidc.svx` tells operators to rate-limit `POST /auth/device` per client address at public ingress; repository search finds no replacement application limiter or dependency | Recorded `mise run test:server:startup`, `mise run docs:check`, `mise run fmt`, and `mise run lints` in the lead-direction evidence | PASS |
| Public-client refresh rotates; replay revokes only the affected rotation chain; confidential `wyrd-ui` refresh does not rotate and obeys the database-clock absolute lifetime | `RefreshTokens::execute`; `active_refresh`; `consume_active_refresh`; `revoke_refresh_chain` | Recorded refresh concurrency/containment tests and human login/renewal journey | PASS |
| Refresh and callback lifecycle changes do not revive an old connection | `RefreshTokens::execute` reissues through the stored binding; callback final transaction takes the connection-slot fence and rechecks the exact active revision | Recorded multi-replica connection cutoff journey and callback tests | PASS |
| `/auth/revoke` uses form input, is client-bound and login-local, and returns 200 for unknown tokens | `auth/cli_login.rs::revoke`; `CliLogins::revoke` | Recorded revocation and human-login journeys | PASS |
| All listed OAuth endpoints use form bodies, RFC 6749 success/error JSON, no-store, and the registered errors | `OAuthForm`, `OAuthError`, `no_store`, and the token/device/revoke/platform adapters; unsupported token-exchange audience is classified as `invalid_target` | Recorded principals integration, exact OAuth tests, JSON-body negatives, and code generation | PASS |
| `client_secret_basic` is interoperable and the public contract describes both public `client_id` and confidential Basic alternatives | `OAuthClients::identify` matches the scheme case-insensitively; `ClientForm<T>` and `oauthClientBasic` appear on token/device/revoke operations | Recorded `basic_scheme_matches_case_insensitively` and served OpenAPI contract tests | PASS |
| RFC 8414 metadata advertises the implemented endpoints, grants, client authentication, and S256 | `auth/authorize.rs::metadata` derives one static document from the configured public origin | Recorded metadata device journey and principals/OpenAPI integration | PASS |
| Existing RFC 8693 delegation/API-key exchange, RFC 7523 JWT bearer, and workload semantics remain intact | `TokenGrants` dispatch retains the standard token-exchange subject types and JWT bearer path; the private `wyrd_api_key` grant has no alias | Recorded workload, platform, CLI, identity, and Bifrost/MCP focused lanes | PASS |
| Tenant, principal, role, audit, and issuance ownership are unchanged | Human grants route through `TenantConn`, shared issuance, connection binding, and canonical auth audit; `active_refresh` relies on RLS rather than a second tenant predicate | Recorded tenant-isolation, SQL, callback audit-failure, refresh, and principals lanes | PASS |
| Private BFF channel, browser-session storage, sealed completion/bootstrap/session rewrap, JSON alternatives, and compatibility routes remain deleted | Cumulative diff deletes the named modules, queries, migrations, columns, and routes; no server-side replacement or alias is present | Recorded migration/SQL, compile, codegen, and targeted integration lanes | PASS |
| No unrequested grant, mechanism, setting, store, retry mask, edge artifact, or dependency is introduced | The cumulative correction removed `tower_governor`; R3 removes the image NGINX limiter and adds only the operator note required by lead direction | Manifest/source inspection and recorded owner lanes | PASS |

## Prior-finding closure

- `FIND-TASK-010-1` is routed to TASK-011 by human direction and is not a
  TASK-010 R3 finding.
- `FIND-TASK-010-2` is closed by deterministic denial-during-approval and
  expiry/delete-during-approval proofs over the guarded approval producer.
- `FIND-TASK-010-3` through `FIND-TASK-010-9` are closed in source by the
  `invalid_target` classification, safe duplicate-parameter redirect path,
  provider-error callback path, case-insensitive Basic scheme, required
  rustdoc, RLS-only refresh query, and complete OAuth OpenAPI alternatives.
- `FIND-TASK-010-10` is closed by the superseding lead direction: the
  incorrect image-local NGINX mechanism and its assertions are deleted, the
  operator-owned public-ingress responsibility is documented, and no
  application limiter, forwarded-header parser, Wyrd option/state, or edge
  manifest was added. The old R2 task's demand for Wyrd-owned edge artifacts
  has no remaining authority.
- `FIND-TASK-010-11` through `FIND-TASK-010-13` are closed by rotation-chain
  containment, the final connection lifecycle fence, and the canonical login
  outcome audit in the callback transaction.

## Proposed findings

None. The cumulative candidate satisfies the task under the current approved
and human-directed behavior. I found no reachable behavioral, security,
tenancy, durability, public-contract, regression, or deletable-drift defect.

## Non-blocking notes

None. Placement, naming, structure, wording, and review-artifact formatting
were not promoted to findings.

## Verification assessment

The task packet records green focused identity journeys and owner lanes for the
authorization code, device, refresh, revocation, workload, migration, SQL,
served OpenAPI, generated contracts, client-tier boundary, tenant isolation,
unwrap audit, formatting, and lints. The R1/R2 reviewers independently reran
the key OAuth, callback, persistence, and concurrency tests. The current lead
direction records the narrow correction lanes: official-image startup,
documentation, formatting, and lints. Source inspection confirms that the R3
runtime change is deletion-only and that the remaining documentation exactly
assigns the RFC 8628 user-code ingress responsibility to the operator.

Unfiltered identity, every-language, and integrated BFF/public-deployment
journeys are intentionally change-review work under the standing
narrowest-lane direction; their absence here is not a task-review verification
limit. No behavior finding depends on broad aggregate proof.

## Overall result

**PASS**
