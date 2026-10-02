# TASK-009 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `1ddc10e21054ddc158f461e8c6d8aa862c32a067`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Prior verdict and validated ledger: `changes/active/oidc-production-readiness/review/TASK-009-r1/`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-009-r1/TASK-009-R1-relying-party-corrections.md`
- Latest implementation delta inspected: `0b516e235^..1ddc10e21054ddc158f461e8c6d8aa862c32a067`

The candidate remained at the requested commit before and after review. I
reviewed the complete base-to-candidate range, using the remediation delta to
locate changed owners and the prior findings only as closure hypotheses.
`FIND-TASK-009-5` is withdrawn by
`review/TASK-009-r1/lead-direction-FIND-TASK-009-5.md`; I did not reopen or
reassess it.

The standing direction was applied throughout: standard `openidconnect`,
Moka, and existing Wyrd owners are sufficient. This review requires no new
mechanism, check, file, setting, option, cache, lock, retry policy, or
compatibility path.

## State and invariant trace

- `HumanConnections` owns one `RelyingParty` for tenant login, connection-test
  sign-in, and callback completion. Library-generated state, nonce, and PKCE
  verifier remain bound in durable login state to the connection and
  initiation; callbacks consume state once, re-resolve the bound connection,
  check RFC 9207 before redemption, and re-check connection state before
  issuing authority.
- `PlatformLogin` is now built once by server boot and stored in
  `ServerAuth::platform_login` (`boot/mod.rs:1585-1631`,
  `components/auth/state.rs:33-44`). Both served platform handlers borrow this
  owner (`components/platform/identity.rs:644-739`), so one process-local
  provider cache spans begin and callback without joining the tenant cache or
  creating cross-replica state.
- Platform callback state remains single-use and server-owned. Completion
  reads the current platform connection, consumes the state, refuses a changed
  issuer, obtains the cached provider, and applies the shared RFC 9207 check
  before token redemption (`platform_login.rs:249-306`). Only a verified
  `(issuer, subject)` can reach the existing platform session owner; the
  successful real-route proof also demonstrates that platform authority does
  not confer tenant authority.
- Human token audience is now the configured client ID by construction:
  `CodeRedemption` carries only `client_id`, `id_token_verifier` verifies it,
  and the platform request/view/row no longer carries a second audience
  (`relying_party.rs:316-332,620-647`, `platform_identity.rs:37-79`,
  `wyrd-sql/src/queries/platform/identity.rs:24-121`). Workload
  `expected_audience` remains separate and unchanged for RFC 7523.
- `RelyingParty::cached` uses Moka's native `try_get_with` for one-process,
  same-issuer miss coalescing. Unknown-key handling invalidates the issuer and
  re-enters that same cached path exactly once per redemption
  (`relying_party.rs:377-415,479-550`). Clones share the Moka cache; no custom
  synchronization or deployment-wide promise was added.
- Workload issuer administration and boot seeding use
  `ScreenedHttp::provider_metadata` to read typed discovery metadata without
  fetching JWKS (`relying_party.rs:197-247`, `admin/routes.rs:735-775`,
  `boot/issuer.rs:220-264`). Workload key retrieval and refresh remain solely
  on the existing `ExternalVerifier`/`JwksCache` path. Human relying-party
  discovery still uses `ProviderMetadata::discover_async` and fetches the
  discovered key set through the screened adapter.
- Every provider request still routes through `ScreenedHttp`; DNS screening,
  address pinning, HTTPS policy, response bounds, no proxy, and no redirects
  remain unchanged. The human `ExternalVerifier` path is absent; its production
  consumer remains workload JWT-bearer verification.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003 / AC-006: connection testing completes a real sign-in against the exact candidate without issuing a User, credential, or session | `connections.rs` retains fresh candidate discovery, usable-JWKS validation, revision binding, and candidate-only completion; tenant callback issuance remains separate | Recorded unfiltered identity journey and exact TASK-009 selectors on the remediation candidate | PASS |
| REQ-004: human discovery and JWKS use `openidconnect`; audience is derived from client ID; unsupported `private_key_jwt` remains refused | `RelyingParty::{fetch,authorize,redeem,verify}` and client-ID-only `CodeRedemption`; duplicate platform audience removed from wire and persistence | `id_token_refusals_fail_closed`, platform route journey, `pg_platform_identity`, and `codegen:check` recorded green | PASS |
| REQ-006 / INV-001: request-controlled routing values cannot select effective tenant, connection, or platform issuer after login starts | Tenant callback uses consumed durable state and connection revision; platform callback uses consumed durable state plus current connection and refuses issuer replacement | Tenant wrong-state/issuer journeys and served platform callback journey recorded green | PASS |
| REQ-007: authorization code, S256 PKCE, standard state/nonce, exact redirect, and verified ID-token signature, issuer, audience, algorithm, key, time, nonce, and claims | `relying_party.rs:438-595`; all human consumers call the same relying-party owner | Exact relying-party negative and success selectors plus Keycloak/Dex identity journeys recorded green | PASS |
| RFC 9207 applies before token redemption to tenant, candidate-test, and platform login | Tenant callback retains `verify_response_issuer`; platform callback now carries optional `iss` and checks it at `platform_login.rs:268-296` before `redeem` | `tenant_callback_issuer_binding_journey`; served platform journey proves advertised-missing and wrong `iss` yield 401 with zero token requests, while exact `iss` succeeds | PASS |
| INV-004 / AC-007: unsafe discovery/JWKS destinations, redirects, outages, and unknown keys fail closed | One screened adapter; unknown key has one terminal refresh branch; state is consumed before provider IO | Exact unsafe-issuer, unsafe-JWKS, redirect, outage, and unknown-key selectors recorded green | PASS |
| Existing per-issuer Moka cache coalesces overlapping misses and rotated-key refreshes in one process | `cached` uses `try_get_with`; unknown-key retry invalidates and re-enters `cached`; tenant and platform owners remain separate | `overlapping_cache_misses_share_one_discovery` and `concurrent_rotated_key_redemptions_share_one_refresh` recorded green and independently rerun: 2 tests passed | PASS |
| Platform begin and callback reuse one long-lived process owner and cached provider state | Boot/test-server composition stores one `PlatformLogin`; both route handlers borrow it | Served platform journey completes during post-begin discovery/JWKS outage with zero discovery/JWKS requests | PASS |
| Workload RFC 7523 setup does not acquire a human-JWKS availability dependency; verification stays on `ExternalVerifier` | Admin and boot consume metadata-only discovery; workload `expected_audience`, resolver, verifier, and `JwksCache` remain the durable verification path | Exact admin and boot selectors prove JWKS is unavailable and receives zero requests; server and identity aggregate lanes recorded green | PASS |
| FIND-TASK-004-13: token endpoint redirects are refused and their targets receive no request | `ScreenedHttp` remains redirect-free and is the only human RP HTTP capability | `a_redirecting_token_endpoint_is_refused_without_following` exact selector recorded green | PASS |
| Platform and tenant authority remain separate | Verified platform identity reaches `PlatformSessions`; no tenant role mapper or tenant issuer is used | Served platform journey uses the returned token on a platform tenant-create route successfully and receives 401 from `/v1/cards` | PASS |
| No hand-written human discovery, PKCE/state/nonce generation, token POST, or human `ExternalVerifier` path remains | Human path uses `ProviderMetadata::discover_async`, `CsrfToken::new_random`, `Nonce::new_random`, `PkceCodeChallenge::new_random_sha256`, and library code exchange/claims verification | Source trace plus shared/auth/server/identity recorded lanes | PASS |
| `oauth2`'s `reqwest` feature remains disabled; no alternate unscreened human HTTP client exists | Workspace dependency remains `openidconnect` with default features disabled; human RP accepts only `ScreenedHttp` | Workspace-hack, lints, and boundary lanes recorded green | PASS |
| Prohibited provider branches, second trust model, tenant/role/issuance redesign, BFF/device-grant changes, and distributed cache/coordination are absent | Complete cumulative diff and caller trace show one provider-agnostic RP and unchanged adjacent owners | Source inspection and recorded aggregate lanes | PASS |
| Every named TASK-009 test has zero-selection-safe evidence | Remediation evidence records exact nextest expressions and the prescribed identity listing/filter workflow | Every recorded selector reports one selected passing test; broad final lanes are also recorded green | PASS |

## Prior-finding closure

| Prior finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-009-1` | Process-owned `PlatformLogin` is composed once and reused by both served handlers; outage-after-begin is exercised | CLOSED |
| `FIND-TASK-009-2` | Platform wire carries optional RFC 9207 `iss`; completion checks it before redemption; real callback route is exercised | CLOSED |
| `FIND-TASK-009-3` | Workload admin and boot read discovery metadata without fetching JWKS; verifier retains JWKS ownership | CLOSED |
| `FIND-TASK-009-4` | Native Moka coalescing restored for cache misses and the one unknown-key refresh path | CLOSED |
| `FIND-TASK-009-5` | Withdrawn by explicit lead direction; not reopened | WITHDRAWN |
| `FIND-TASK-009-6` | Human audience is client ID by construction; independent platform setting and column removed | CLOSED |
| `FIND-TASK-009-7` | Unused `RelyingParty::http` accessor is absent | CLOSED |
| `FIND-TASK-009-8` | Library-default `CsrfToken` and `Nonce` constructors replace the Wyrd length constant | CLOSED |
| `FIND-TASK-009-9` | `require_usable_jwks` and `not_tested_reason` each have accurate adjacent rustdoc | CLOSED |
| `FIND-TASK-009-10` | Durable remediation evidence contains exact selectors/listing for every named test | CLOSED |

## Proposed findings

None. The cumulative candidate satisfies the original TASK-009 obligations,
and the remediation closes every non-withdrawn prior finding without adding a
nonstandard mechanism or widening the task.

## Verification assessment

The remediation record reports all required focused selectors and the broader
format, lint, codegen, docs, boundary, shared, SDK, CLI, principal, language,
identity, and Wyrd server lanes green on code commit `0b516e235`. Candidate
`1ddc10e210` changes only that remediation evidence document after the code
commit, so the runtime evidence applies to the reviewed source. I independently
reran the two changed cache-concurrency tests together through the pinned Mise
toolchain; both passed. `git diff --check` is clean.

No verification limit prevents an invariant verdict.

## Overall result

**PASS**
