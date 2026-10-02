# TASK-009 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `04597909203463820b2033c12956f5fe6fcfe1f4`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Prior review records: `changes/active/oidc-production-readiness/review/TASK-009-r1/` and `TASK-009-r2/`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-009-r2/TASK-009-R2-relying-party-corrections.md`
- Latest remediation delta inspected: `1ddc10e21054ddc158f461e8c6d8aa862c32a067..04597909203463820b2033c12956f5fe6fcfe1f4`

The candidate resolved to the requested commit before and after review. I
reviewed the complete base-to-candidate range and used the latest remediation
delta only to locate changed owners and test closure.

`FIND-TASK-009-5` and `FIND-TASK-009-14` are withdrawn by explicit lead
direction. I did not reopen, reassess, renumber, or indirectly require either
one. I also applied the standing standard-practice direction: the selected
`openidconnect` behavior, Moka cache, screened transport, and existing Wyrd
owners are sufficient; this review requires no new option, check, cache,
coordination mechanism, compatibility path, or test harness.

## Producer-to-sink invariant trace

- `HumanConnections` owns one process-local `RelyingParty` shared by tenant
  begin, candidate testing, and callback completion
  (`connections.rs:84-177`). Library-produced state, nonce, and PKCE verifier
  are persisted with the exact connection revision, issuer, client ID,
  redirect, and initiation binding (`login.rs:95-149`). Callback consumption
  selects the tenant through the hashed state, consumes the state before
  provider IO, re-resolves the bound connection, checks RFC 9207, verifies the
  ID token, and rechecks the connection before authority is issued
  (`callback.rs:99-207,243-346`). No request-controlled tenant or connection
  selector reaches issuance.
- Server boot constructs one `PlatformLogin` and stores it in
  `ServerAuth::platform_login` (`boot/mod.rs:1585-1631`; `components/auth/state.rs:14-48`).
  Platform configuration, begin, and callback all borrow that owner
  (`components/platform/identity.rs:235-244,651-746`), so fresh setup discovery
  and both login phases use the same process cache.
- Platform state remains single-use and server-owned. Completion loads the
  current durable connection, consumes the state, refuses an issuer replacement,
  checks the optional RFC 9207 response issuer against the state-bound issuer
  before redemption, and passes only verified issuer/subject/claims into the
  existing platform-session transaction (`platform_login.rs:238-346`). Tenant
  and platform authority remain separate.
- `CodeRedemption` has one audience authority: `client_id`
  (`relying_party.rs:316-332`). Both production producers populate it from the
  bound connection (`callback.rs:187-198`; `platform_login.rs:297-318`). The
  `openidconnect` verifier uses that value, retains the library's default
  refusal of additional audiences, and checks a present `azp`
  (`relying_party.rs:570-665`). The platform request/view/SQL row expose no
  second human audience (`wyrd-spec/src/auth/platform_identity.rs:37-79`;
  `wyrd-sql/src/queries/platform/identity.rs:21-124`). Workload
  `expected_audience` remains a distinct RFC 7523 contract.
- `RelyingParty::cached` uses Moka `try_get_with`; clones share the cache and a
  failed fetch is not cached (`relying_party.rs:343-415`). Unknown-key handling
  invalidates that issuer and re-enters the same single-flight path once, then
  terminates (`relying_party.rs:479-550`). This preserves the requested
  process-local boundary without a nonstandard generation protocol or
  cross-replica guarantee.
- Human discovery and token/JWKS requests all traverse `ScreenedHttp`, whose
  client screens and pins every destination, disables redirects and proxies,
  bounds time and decoded response size, and refuses unsafe schemes
  (`screening.rs:20-245`; `relying_party.rs:155-259`). Platform setup now uses
  full `RelyingParty::discover`, while the two workload setup producers retain
  metadata-only `ScreenedHttp::provider_metadata`; workload key retrieval still
  belongs to `ExternalVerifier`/`JwksCache` (`components/admin/routes.rs:317-349,735-763`;
  `boot/issuer.rs:220-264`; `boot/auth.rs:37-105`).
- Production human consumers no longer call `ExternalVerifier`. Its remaining
  server handle and resolver documentation identify the workload
  `jwt-bearer` path, and the callback fixture no longer constructs an unused
  human verifier. The production workload caller remains intact.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003 / AC-006: candidate testing performs the real standard sign-in and creates no User, credential, or session | `HumanConnections::begin_test`, candidate-bound `LoginState`, `tested_candidate`, and `stamp_test_sign_in`; common callback exits on the connection-test branch before user/session issuance | Recorded identity journeys and focused callback/connection tests from the original and R1 evidence remain applicable; R2 changes do not alter the candidate issuance branch | PASS |
| REQ-004: human setup uses OIDC discovery/JWKS, validates the issuer, derives audience from client ID, and refuses unsupported client authentication | `RelyingParty::{fetch,authorize,redeem,verify}`; client-ID-only `CodeRedemption`; platform configure uses the process owner's `discover`; `PrivateKeyJwt` has a terminal configuration refusal | `id_token_refusals_fail_closed`; served platform configure/callback journey; `codegen:check` | PASS |
| REQ-006 / INV-001: unverified route, header, state, or provider values cannot select effective tenant or connection | Tenant state hash resolves one tenant and one stored connection revision; platform state records one issuer and completion requires the current connection to retain it | Wrong-state, wrong-tenant, issuer-binding, inactive/replaced-connection, and replay evidence recorded in the identity/server lanes | PASS |
| REQ-007: authorization code flow uses S256 PKCE, library state/nonce, exact redirect, signature, issuer, audience, algorithm, key, time, nonce, and claim validation | `relying_party.rs:438-665`; every human producer calls that owner | Exact relying-party success/refusal selectors and tenant callback refusal journey recorded green | PASS |
| OIDC audience and authorized-party invariants use the standard library behavior without another Wyrd trust option | Library default rejects any audience beside `client_id`; present `azp` must equal it; no human `expected_audience` request or runtime input remains | R2 RED/GREEN `id_token_refusals_fail_closed`; `tenant_callback_refusal_journey` refuses `[client_id, other]` with `azp=client_id` and accepts the single-audience case | PASS |
| RFC 9207 binds tenant, candidate-test, and platform authorization responses before token redemption | Shared `verify_response_issuer`; platform callback contract carries optional `iss`; both completion owners call the check before `redeem` | Tenant issuer-binding journey and served platform journey prove match, missing-advertised, and mismatch; refusal cases make zero token requests | PASS |
| INV-004 / AC-007: unsafe destinations, redirects, outages, and unknown signing keys fail closed | One screened adapter; no redirect-capable alternate client; one terminal unknown-key refresh | Exact unsafe issuer/JWKS, redirect, outage, unknown-key, and cache-concurrency tests recorded green | PASS |
| Existing per-issuer cache coalesces process-local cold/expired misses and a rotated-key refresh | `cached` uses Moka `try_get_with`; unknown-key handling invalidates and re-enters `cached`; tenant/platform owners are long-lived and separate | `overlapping_cache_misses_share_one_discovery`; `concurrent_rotated_key_redemptions_share_one_refresh` | PASS |
| Platform setup refuses unavailable/undecodable advertised JWKS without durable replacement and refreshes the current process's same-issuer provider | Configure performs full discovery before the upsert and inserts the returned provider into the same `PlatformLogin` cache | `federated_platform_sign_in_runs_through_the_served_callback` covers both setup refusals, unchanged stored client ID, and same-issuer `mock-2` use during outage | PASS |
| Workload setup retains metadata-only behavior and workload verification ownership | Admin create and boot seeding call `ScreenedHttp::provider_metadata`; boot still constructs `ExternalVerifier` over `JwksCache`; `jwt-bearer` caller remains | Exact workload admin/boot zero-JWKS tests and `test:wyrd` recorded green | PASS |
| Platform and tenant principal/authorization planes remain distinct | Platform verified identity reaches `PlatformSessions`; tenant login reaches tenant issuance/role owners; neither substitutes for the other | Served platform journey proves the returned token can perform a platform operation and is unauthorized on `/v1/cards` | PASS |
| No hand-written human discovery, PKCE/state/nonce generation, token POST, or human `ExternalVerifier` path remains | Human paths use `ProviderMetadata::discover_async`, `CsrfToken::new_random`, `Nonce::new_random`, `PkceCodeChallenge::new_random_sha256`, library exchange, and library claims verification | Source/caller trace; callback target, shared, Wyrd, docs, and lint lanes recorded green | PASS |
| `oauth2`'s `reqwest` feature remains disabled and no unscreened human HTTP client exists | Workspace pins `openidconnect = 4.0.1` with default features off; the crate injects only `ScreenedHttp` as `AsyncHttpClient` | Workspace-hack, client-tier, lint, and source feature inspection evidence | PASS |
| Prohibited provider branches, second trust model, BFF/device/session redesign, and distributed cache coordination are absent | Complete cumulative diff and caller trace show one provider-agnostic human RP and unchanged adjacent authority owners | Source inspection and narrow owner lanes | PASS |
| Task-level verification follows the human standing rule: exact changed tests and the narrowest lanes cover this write set; change review owns full journeys/every-language sweeps | R2 evidence records exact selectors for changed RP, callback, boot, admin, served platform, and tenant refusal tests plus the relevant scoped lanes | Exact tests all selected and passed; `fmt`, `lints`, `codegen:check`, `docs:check`, boundary checks, `test:shared`, `test:principals:integration`, `test:identity:journey`, and `test:wyrd` all exited 0 | PASS |

## Prior-finding closure

| Prior finding | Current source evidence | Result |
|---|---|---|
| `FIND-TASK-009-1` | One boot-owned `PlatformLogin` is stored in `ServerAuth`; setup, begin, and callback borrow it, and the route journey proves cached completion during provider outage | CLOSED |
| `FIND-TASK-009-2` | Platform callback carries optional `iss`; `PlatformLogin::complete` checks it against consumed state and provider metadata before redemption | CLOSED |
| `FIND-TASK-009-3` | Workload admin and boot read metadata only; workload JWKS remains on `ExternalVerifier` | CLOSED |
| `FIND-TASK-009-4` | Moka `try_get_with` coalesces shared misses; unknown-key handling re-enters it once | CLOSED |
| `FIND-TASK-009-5` | Explicit lead direction withdraws the finding and forbids reopening it | WITHDRAWN — NOT REOPENED |
| `FIND-TASK-009-6` | Human verifier audience is derived solely from `client_id`; the platform public/runtime contract has no second audience input | CLOSED |
| `FIND-TASK-009-7` | No unused `RelyingParty::http` accessor or equivalent escape hatch exists | CLOSED |
| `FIND-TASK-009-8` | Authorization uses the library-default state, nonce, and S256 PKCE constructors | CLOSED |
| `FIND-TASK-009-9` | The corrected private helpers retain accurate adjacent rustdoc | CLOSED |
| `FIND-TASK-009-10` | Durable remediation evidence records exact zero-selection-safe selectors for every named changed test | CLOSED |
| `FIND-TASK-009-11` | The additional-audience override is absent; library-default refusal is covered at RP and served tenant boundaries | CLOSED |
| `FIND-TASK-009-12` | Human `ExternalVerifier` claims and callback fixture wiring are removed; workload docs and production wiring remain | CLOSED |
| `FIND-TASK-009-13` | Platform configure uses the process-owned RP's full discovery and replaces that cache entry before durable commit | CLOSED |
| `FIND-TASK-009-14` | Explicit lead direction withdraws the finding and voids its remediation section | WITHDRAWN — NOT REOPENED |

## Proposed findings

None. The cumulative candidate satisfies TASK-009's state, trust, cache,
tenancy, and authority invariants. All non-withdrawn prior findings are closed,
and the R2 remediation adds no nonstandard mechanism that the approved task
requires removed.

## Verification assessment

The durable R2 evidence records the exact changed-test selectors and all
narrow repository lanes covering the final write set as passing on the final
source. The latest source-only journey correction was followed by `fmt`,
`lints`, and the unfiltered identity journey, all green. The absence of another
full every-language sweep is not a task-review gap under standing direction
`518026d54`; those suites run once at change review. No missing source, caller,
authority, or required task-level proof blocks this invariant review.

## Overall result

**PASS**
