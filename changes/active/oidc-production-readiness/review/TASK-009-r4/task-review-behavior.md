# TASK-009 round-4 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `0bd3686e8bb763b07376f84661946aadc6200bfb`
- Candidate tree: `86fb8e11681b4d2e60bba2aa7ee3bf2c4f134153`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Prior verdict and ledger: `changes/active/oidc-production-readiness/review/TASK-009-r3/{verdict.md,findings-validation.md}`
- Current remediation: `changes/active/oidc-production-readiness/review/TASK-009-r3/TASK-009-R3-platform-login-boundary-and-contract.md`
- Latest correction delta: `04597909203463820b2033c12956f5fe6fcfe1f4..0bd3686e8bb763b07376f84661946aadc6200bfb`

All source evidence below is commit-qualified at the candidate. `.codegraph/`
is absent, so navigation used repository search, the cumulative and latest
diffs, and direct caller tracing. By binding lead direction,
`FIND-TASK-009-5`, `FIND-TASK-009-14`, and `FIND-TASK-009-15` are withdrawn
and were not reopened, renamed, or implemented indirectly. Placement, naming,
structure, and wording alone were not treated as blocking behavior defects.

## Behavior navigation and caller trace

- `wyrd_auth_oidc::RelyingParty::{cached,discover,authorize,redeem,verify}`
  owns human discovery, the process-local provider cache, library-generated
  PKCE/state/nonce, token exchange, and human ID-token verification.
- Tenant login and candidate testing converge through
  `HumanConnections::{begin_login,begin_test}`, the common
  `AuthorizationCodeExchange`, and the same relying party.
- Platform configuration, begin, and callback use the boot-composed
  `ServerAuth::platform_login`; configuration performs fresh full discovery
  through its accepted `relying_party` accessor, and begin/callback share that
  relying party's cache.
- Workload issuer setup remains on metadata-only
  `ScreenedHttp::provider_metadata`; workload RFC 7523 verification remains on
  `ExternalVerifier` and its existing `JwksCache`.
- The latest runtime-contract correction is confined to the platform
  configuration route declaration and its existing served platform/OpenAPI
  tests. It does not change login, trust, cache, tenancy, issuance, or session
  behavior.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Tenant login, candidate test sign-in, and platform login use `openidconnect 4.0.1` for discovery, PKCE/state/nonce, token redemption, and human ID-token validation | `crates/shared/wyrd-auth-oidc/src/relying_party.rs:364-665`; tenant callers in `login.rs`, `callback.rs`, and `connections.rs`; platform caller in `platform_login.rs:113-346` | Recorded exact relying-party/callback tests and identity journeys | PASS |
| Every human relying-party request uses the screened, DNS-pinned, proxy-free, redirect-disabled, bounded transport; the `oauth2` `reqwest` feature remains disabled | `relying_party.rs:155-260,430-435`; `screening.rs`; workspace dependency uses `openidconnect = 4.0.1` with default features off | Exact unsafe issuer/JWKS, redirect-target-zero-request, proxy, and body-bound tests; shared/lint evidence | PASS |
| ID-token negatives refuse bad nonce, issuer, audience, additional audience, authorized party, algorithm, signature, expiry, future or missing `iat`, invalid subject, and unmappable claims | `RelyingParty::verify`, `id_token_verifier`, and `verify_authorized_party` at `relying_party.rs:553-665`; no trust-all additional-audience override | `id_token_refusals_fail_closed`; served tenant refusal journey; recorded callback proof | PASS |
| A valid ordinary ID token still completes and maps its identity | Library claim verification followed by `map_claims`; matching `azp` and one client-ID audience remain accepted | Unit success cases and served tenant/platform successful callbacks | PASS |
| RFC 9207 response issuer is checked before redemption for tenant, candidate-test, and platform callbacks | Tenant `callback.rs:180-198`; platform `platform_login.rs:280-318`; shared `verify_response_issuer` | Tenant issuer-binding journey and served platform callback prove missing/wrong advertised `iss` reaches no token request | PASS |
| Unknown `kid` causes one bounded rediscovery, overlapping local misses share the Moka fetch, and a still-unknown key fails closed | `RelyingParty::cached` uses `try_get_with`; `redeem` invalidates and re-enters it once at `relying_party.rs:539-548` | Exact unknown-key, still-unknown-key, overlapping-miss, and concurrent-refresh tests recorded green | PASS |
| Unsafe discovery/JWKS destinations, redirects, and IdP outages fail closed without another tenant or platform/tenant fallback | `ScreenedHttp`; typed relying-party error projection; no alternative-provider branch | Exact unsafe destination, redirect, outage, tenant refusal, and served platform refusal proofs | PASS |
| Workload issuer setup does not require JWKS availability, and workload assertions remain on `ExternalVerifier` | Workload admin/boot call `ScreenedHttp::provider_metadata`; production `jwt-bearer` remains the `ExternalVerifier` consumer | Exact admin and boot zero-JWKS tests; recorded `test:wyrd` evidence | PASS |
| Human verification no longer uses `ExternalVerifier` | Tenant and platform callback paths consume `VerifiedIdToken` from `RelyingParty`; `wyrd-auth-verify` documents and retains workload-only ownership | Callback target, source/caller inspection, and prior focused lanes | PASS |
| Platform configuration performs standard full discovery through the process-owned relying party, stores the advertised JWKS URI, and leaves the durable row unchanged when discovery/JWKS fails | `components/platform/identity.rs:209-288` calls `PlatformLogin::relying_party().discover`; `RelyingParty::discover` inserts only after successful full discovery | `federated_platform_sign_in_runs_through_the_served_callback`, including unavailable/undecodable key sets, unchanged row, same-issuer refresh, and subsequent callback | PASS |
| The platform configuration public contract accurately distinguishes malformed/blocked input (`400`) from unavailable, undecodable, or mismatched discovery/JWKS (`503/WYRD_AUTH_503_DISCOVERY_UNAVAILABLE`) | Handler rustdoc and `utoipa` responses at `components/platform/identity.rs:173-207`; existing error projection in `components/admin/routes.rs` | Final-candidate exact platform test asserts status/code; exact served OpenAPI test asserts the `503` problem schema and stable code | PASS — closes `FIND-TASK-009-16` |
| Human ID-token audience is derived only from the configured client ID | `CodeRedemption`, platform request/view/resolver, SQL row, and verifier have no independent human audience input | Platform served journey, SQL tests, and codegen evidence | PASS |
| Tenant and platform identity remain separate, and provider failure cannot remove global platform administration | Tenant paths use `TenantConn`; platform connection/login/session paths use `OperatorPool`; platform login remains additive to the global credential | Served platform journey proves platform authority and tenant refusal; recorded principals and boundary lanes | PASS |
| No hand-written discovery, PKCE/state/nonce generation, token POST, or human JWT verifier remains | Removed helpers/types have no candidate definition or human caller; `openidconnect` owns those operations | Complete cumulative source and dependency inspection | PASS |
| No provider-specific branch, second unscreened human client, changed tenant selection, role mapping, issuance, audit, BFF/device/session redesign, or speculative mechanism entered the task | Complete base-to-candidate diff and callers | Recorded focused and narrow owner lanes | PASS |
| Conventional provider error diagnostics remain as directed by the lead | Candidate retains the standard library error display; no equivalent redaction mechanism was added | Source inspection | EXCLUDED — withdrawn `FIND-TASK-009-5`, not reopened |
| The unreleased platform audience column removal remains as directed by the lead | Candidate migration directly removes the obsolete duplicate audience column; no preflight/overlap/dual-write mechanism was added | Source inspection | EXCLUDED — withdrawn `FIND-TASK-009-14`, not reopened |
| The accepted public `PlatformLogin::relying_party` accessor remains as directed by the lead | `platform_login.rs:131-141`; the configuration caller at `components/platform/identity.rs:253-257` | Source inspection | EXCLUDED — withdrawn `FIND-TASK-009-15`, not reopened |
| Every other prior finding remains closed | Process-owned platform login, RFC 9207 callback, metadata-only workload setup, coalesced cache, client-ID audience, deleted drift, corrected docs, exact selectors, standard verifier defaults, workload-only `ExternalVerifier`, full platform discovery, and the corrected `503` contract remain present | Prior exact proof plus final-candidate exact R3 tests | PASS |

## Proposed findings

None. The cumulative candidate satisfies TASK-009's required observable
behavior and negative flows. The latest remediation closes
`FIND-TASK-009-16`; the three withdrawn findings remain excluded exactly as
directed. No behavior, security, tenancy, durability, public-contract, or
deletion consequence supports another finding.

## Prior-finding closure

- `FIND-TASK-009-1` through `-4` and `-6` through `-13` remain closed by the
  process-owned platform login, both RFC 9207 callback paths, metadata-only
  workload setup, Moka single-flight use, client-ID-derived audience, removal
  of unearned mechanisms, corrected documentation, exact test selection,
  standard verifier behavior, workload-only `ExternalVerifier`, and full
  platform discovery through the shared process cache.
- `FIND-TASK-009-16` is closed: runtime already returned the catalogued
  discovery `503`; the handler/OpenAPI declaration now publishes it, and both
  unavailable/undecodable key-set cases and the served document pin the exact
  status, code, and problem schema.
- `FIND-TASK-009-5`, `FIND-TASK-009-14`, and `FIND-TASK-009-15` are
  **WITHDRAWN — NOT REOPENED**.

## Verification notes

- Reviewed the complete cumulative diff
  `35a53faa216b10651d85c96ce12e34f382cac637..0bd3686e8bb763b07376f84661946aadc6200bfb`
  and latest correction delta
  `04597909203463820b2033c12956f5fe6fcfe1f4..0bd3686e8bb763b07376f84661946aadc6200bfb`.
- On the immutable final candidate, the exact
  `federated_platform_sign_in_runs_through_the_served_callback` and
  `the_served_document_describes_the_composed_surface` tests both passed when
  invoked through the repository Postgres setup and migration wrapper. Initial
  direct invocations failed only because `WYRD_TEST_DATABASE_ADMIN_URL` was
  absent; that was an invocation/setup error, not a product failure.
- The R3 remediation record also reports final-candidate `fmt` and `lints`
  green. The broader `test:wyrd` and `test:principals:integration` lanes were
  green on the immediately preceding candidate whose only later source change
  was the lead-directed revert of withdrawn `FIND-TASK-009-15`; the two exact
  affected final-candidate tests above are green.
- Under the human standing direction, task review uses the narrowest lanes for
  the task write set. Full identity journeys and every-language sweeps run once
  at change review, so their absence from this final small remediation is not a
  task-review verification limit.

## Overall result

**PASS**

The candidate satisfies TASK-009's behavior, closes every non-withdrawn prior
finding in this behavior scope, and adds no nonstandard mechanism.
