# TASK-009 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa2`
- Candidate: `6578921d8ced6316c850e4d8f16bd101630a7056`
- Specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Routed prior direction: `changes/active/oidc-production-readiness/review/TASK-004-r2/lead-direction-routing.md`, including `FIND-TASK-004-13`

I reviewed the complete base-to-candidate range and traced configuration and
discovery through authorization state, code redemption, ID-token verification,
identity issuance, and the tenant, connection-test, platform, and workload
consumers. The reviewed source was not modified.

## State and consumer trace

- `HumanConnections` owns one shared `RelyingParty`; tenant login and candidate
  testing obtain provider metadata through it, store library-generated state,
  nonce, and PKCE verifier in `LoginState`, and bind that state to the exact
  connection id, revision, issuer, client id, redirect URI, and initiation.
- `AuthorizationCodeExchange` consumes state before provider IO, re-resolves the
  bound Active connection (or exact Candidate), checks the RFC 9207 response
  issuer, redeems through `RelyingParty`, and re-checks the connection before
  candidate stamping or tenant session issuance.
- `PlatformLogin` owns a separate shared `RelyingParty`, stores its generated
  state, nonce, verifier, issuer, and redirect URI, consumes state once, checks
  the current configured issuer, and redeems through the same library. Unlike
  the tenant consumer, its callback contract and completion method carry no
  RFC 9207 response issuer.
- `RelyingParty::redeem` uses `openidconnect` for the token request and core
  validation, then applies the standard caller-owned `azp`, issued-at, and
  Subject Identifier rules. A `NoMatchingKey` result causes exactly one fresh
  discovery and one final verification attempt.
- `ScreenedHttp` is the only `AsyncHttpClient` supplied to discovery, JWKS, and
  token requests. It resolves and pins each destination, applies the deployment
  address policy, disables redirects and proxies, and bounds time and decoded
  response size. The routed `FIND-TASK-004-13` behavior is directly exercised
  for 307 and 308 responses with an untouched second origin.
- `ExternalVerifier` no longer has a human ID-token entry point. Its production
  request consumer is the workload JWT-bearer exchange; references in callback
  tests are fixture construction, not the human redemption path.
- Boot-time workload-issuer discovery now reuses `RelyingParty::discover`, but
  the durable workload assertion verifier and issuer/binding authority remain
  unchanged. The cumulative source and stated lanes provide no reachable
  workload regression.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003 / AC-006: a candidate is tested by a real sign-in through that exact revision and creates no User or session | `connections.rs:342-389`, `callback.rs:152-171,244-265`; state and candidate revision are re-read before stamping | Unfiltered identity journey and `test:wyrd` reported green; callback persistence tests cover the no-issuance boundary | PASS |
| REQ-004: endpoints and JWKS come from discovery, audience follows tenant client id, and unsupported `private_key_jwt` is refused | `RelyingParty::discover`; `pg_resolvers.rs:563` derives tenant `expected_audience` from `client_id`; `RelyingParty::redeem` refuses `PrivateKeyJwt` | Discovery/JWKS tests, connection journeys, and codegen reported green | PASS |
| REQ-006 / INV-001: request-controlled values cannot select tenant or connection after login starts | `login.rs:95-149`; `callback.rs:99-208,322-348`; only the SHA-256 state lookup selects tenant and the durable row supplies connection, issuer, client, redirect, verifier, and nonce | Identity wrong-tenant, state replay, and issuer-binding journeys reported green | PASS |
| REQ-007: authorization code, S256 PKCE, random state and nonce, exact redirect, and verified ID-token signature/issuer/audience/algorithm/key/time/claims | `relying_party.rs:390-417,441-531,557-600`; all three login owners call `authorize`/`redeem` | Relying-party validation unit tests and tenant identity journeys reported green | PASS, subject to INV-REV-001 for the platform RFC 9207 boundary |
| INV-004 / AC-007: unsafe discovery or JWKS and provider outage fail closed | `ScreenedHttp::send`; `screening.rs:139-220`; relying-party error mapping exposes no destination detail | `an_unsafe_issuer_is_refused_before_any_request`, `an_unsafe_key_set_url_is_refused_before_any_request`, `a_provider_outage_fails_closed`, and tenant refusal journey reported green | PASS |
| INV-004 / AC-007: unknown signing key causes one re-discovery and then succeeds or fails closed | `relying_party.rs:477-487`; fresh discovery replaces the issuer cache and the second `verify` has no retry branch | `an_unknown_key_rediscovers_exactly_once` and `a_still_unknown_key_fails_after_one_rediscovery` reported green with exact discovery/JWKS counts | PASS |
| RFC 9207 response issuer is checked before secret-bearing token redemption for every relying-party login consumer | Tenant and candidate path calls `verify_response_issuer` at `callback.rs:180-186` before `redeem`; platform path at `platform_login.rs:239-285` does not receive or check response `iss`, and `PlatformCallbackRequest` has only `code` and `state` | Tenant unit and journey cases exist; no platform completion or RFC 9207 case exists | FAIL — INV-REV-001 |
| FIND-TASK-004-13: token endpoint redirects are never followed | `ScreenedHttp::client_for` uses `Policy::none`; every library request uses the adapter | `a_redirecting_token_endpoint_is_refused_without_following` covers 307 and 308 and asserts zero target hits | PASS |
| AC-002 / AC-003 / AC-008: Keycloak and Dex, tenant and connection-test flows remain provider agnostic through the library | `RelyingParty` is shared by the tenant login/test owners with no provider branches | Full identity journey reported 31 server cases plus UI, CLI, Rust, client, Python, and TypeScript targets | PASS |
| Platform-administrator login uses the library and remains operational | `platform_login.rs:153-217,239-313` uses `RelyingParty::{cached,authorize,redeem}` | `test:wyrd` was reported green, but repository search finds no test that calls `/auth/platform/callback` or `PlatformLogin::complete`; existing platform tests stop at begin/screening or mint a federated session through a test helper | FAIL — closure proof is part of INV-REV-001 |
| No hand-written discovery, PKCE/state/nonce, token POST, or human `ExternalVerifier` path remains | `provider.rs` deleted; prior callback/login helpers deleted; production `ExternalVerifier::verify_external` consumer is `jwt_bearer.rs` | Source search plus reported shared/auth/server lanes | PASS |
| Workload RFC 7523 verification remains on `jsonwebtoken` through `ExternalVerifier` | `wyrd-auth/src/jwt_bearer.rs:38,100`; `ExternalVerifier::verify_external_against` remains | Shared and server test lanes reported green | PASS |
| `oauth2`'s reqwest feature remains disabled | workspace `openidconnect = { version = "=4.0.1", default-features = false }`; no crate enables an openidconnect/oauth2 reqwest feature | Recorded `cargo tree -e features -i oauth2`; `check:workspace-hack` green | PASS |
| Prohibited provider branches, alternate HTTP client, redirects, tenant/role/issuance changes, and unrelated BFF/grant work are absent | One adapter and one provider-agnostic RP; tenant issuance and role owners remain; `cli_logins.rs` change only relocates the deleted random device-secret helper without changing its 32-byte base64url result | Complete diff inspection and reported broad lanes | PASS |

## Proposed findings

### INV-REV-001 — MISSING: platform login omits the standard RFC 9207 response-issuer binding and has no completion-path proof

- **Violated obligation:** TASK-009 requires tenant login, connection testing,
  and platform-administrator login to use the same vetted relying party while
  retaining the RFC 9207 `iss` check; REQ-007 requires mismatched providers to
  fail closed; the task's platform-login outcome and acceptance criteria
  require the changed platform flow to work through `openidconnect`.
- **Exact location:**
  `crates/wyrd-spec/src/auth/platform_identity.rs:113-122` defines
  `PlatformCallbackRequest` with only `code` and `state` and rejects unknown
  fields; `crates/wyrd/wyrd-server/src/components/platform/identity.rs:732-748`
  therefore cannot forward response `iss`; and
  `crates/wyrd/wyrd-auth/src/platform_login.rs:239-285` proceeds from consumed
  state to token redemption without the shared pre-redemption issuer check.
- **Evidence:** the sibling tenant/candidate consumer passes optional `iss` to
  `verify_response_issuer` before `redeem` (`callback.rs:180-197`), using the
  same provider metadata already available to the platform consumer. Repository
  search finds no caller of `/auth/platform/callback` in tests and no test call
  to `PlatformLogin::complete`; the platform E2E test uses
  `federated_platform_session` instead of the provider callback.
- **Observable consequence:** when discovery advertises
  `authorization_response_iss_parameter_supported`, the platform path accepts
  a callback with no issuer and cannot accept and validate the issuer parameter
  emitted by the provider. A mismatch is not rejected before the authorization
  code is sent to the token endpoint. Independently, a regression in the newly
  changed platform token request, nonce/ID-token verification, or session
  issuance path can pass every recorded test because that path is not run.
- **Required testable correction:** use the existing RFC 9207 mechanism, not a
  new check: retain optional `iss` in `PlatformCallbackRequest`, forward it to
  `PlatformLogin::complete`, and call `verify_response_issuer` against the
  state-bound issuer and the cached provider's advertised flag before
  `RelyingParty::redeem`. Extend the existing real-server platform identity
  journey to complete a provider authorization-code flow through
  `/auth/platform/callback`, prove the resulting platform session can call a
  protected platform route, and prove advertised-missing and mismatched `iss`
  are refused before the mock token endpoint receives a request. No new
  protocol mechanism, setting, option, or harness is required.

## Verification assessment

The evidence before `9ef532660` remains credible after workspace-hack
regeneration. That commit changes no source, package version, or runtime
configuration; it regenerates the feature-unification manifest and lock entry.
The added crypto features are additive capability features, and the removed
direct `signature` entry is the generator's new union. The post-regeneration
`mise run lints` compiles the whole workspace with all features and all targets,
then the shipped server binary at release features, while
`mise run check:workspace-hack` proves the generated union. Re-running all
runtime lanes would not close INV-REV-001 because none drives the platform
callback completion path.

Apart from INV-REV-001, the supplied verification is proportionate and covers
the changed owners and sibling language journeys. There is no verification
limit that blocks a verdict.

## Overall result

**FAIL**

The candidate satisfies the tenant, candidate-test, transport, cache,
workload-separation, and redirect obligations, but it does not apply the
standard RFC 9207 boundary to the required platform relying-party consumer and
does not exercise that changed completion path.
