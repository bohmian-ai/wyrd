# TASK-009 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `1ddc10e21054ddc158f461e8c6d8aa862c32a067`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-009-r1/TASK-009-R1-relying-party-corrections.md`
- Prior verdict and ledger: `changes/active/oidc-production-readiness/review/TASK-009-r1/{verdict.md,findings-validation.md}`
- Human direction: `changes/active/oidc-production-readiness/review/TASK-009-r1/lead-direction-FIND-TASK-009-5.md`; `FIND-TASK-009-5` is withdrawn and was not reviewed or reopened.

The candidate remained at the stated commit throughout this review. `.codegraph/`
is absent, so source navigation used repository search and direct caller tracing.

## Behavior navigation and caller trace

- `wyrd_auth_oidc::RelyingParty::{cached,discover,authorize,redeem,verify}` owns
  human discovery, authorization construction, token exchange, ID-token
  verification, and the process-local Moka cache.
- Tenant login and connection-test callers converge through
  `HumanConnections::relying_party`, `LoginService::begin`, and
  `AuthorizationCodeExchange::{execute,complete}`.
- Platform begin and callback converge on the one boot-composed
  `ServerAuth::platform_login`, whose `PlatformLogin::{begin,complete}` share
  one `RelyingParty`.
- Workload issuer administration and boot seeding use only
  `ScreenedHttp::provider_metadata`; workload assertion verification remains on
  `ExternalVerifier`/`JwksCache`.
- Served proof is concentrated in `identity_e2e.rs` and the real
  `/auth/platform/{login,callback}` journey in `platform_admin_e2e.rs`.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Tenant login, candidate test sign-in, and platform login use `openidconnect` for discovery, PKCE/state/nonce, token redemption, and ID-token validation | `relying_party.rs:364-596`; tenant callers in `login.rs` and `callback.rs:151-198`; candidate caller in `connections.rs`; platform caller in `platform_login.rs:159-334` | Recorded exact relying-party tests and unfiltered `test:identity:journey`; platform served callback journey | PASS |
| All relying-party HTTP uses the screened, pinned, proxy-free, redirect-disabled, bounded transport; `oauth2`'s reqwest feature stays disabled | `relying_party.rs:155-260`; `screening.rs:139-199`; `wyrd-auth-oidc/Cargo.toml` | Exact unsafe issuer/JWKS and token redirect tests; recorded `test:shared`, lints, and dependency-feature evidence | PASS |
| Human ID tokens reject bad nonce, issuer, audience, algorithm, expiry, signature, future `iat`, and invalid subject before issuing identity | `relying_party.rs:570-665` | `id_token_refusals_fail_closed`, `an_unadvertised_signing_algorithm_is_refused`, and tenant refusal journey | **FAIL** — the verifier deliberately trusts every additional audience; see `BEH-R2-001` |
| RFC 9207 response issuer is checked before token redemption for tenant, candidate-test, and platform callbacks | Tenant `callback.rs:180-198`; platform `platform_login.rs:268-297`; shared decision `callback.rs:404-430` | Exact tenant issuer-binding journey; platform served callback step 2 proves missing/wrong `iss` produces no `/token` request | PASS |
| Unknown `kid` triggers at most one re-discovery and still-unknown keys fail closed | `relying_party.rs:539-550` | Exact `an_unknown_key_rediscovers_exactly_once`, `a_still_unknown_key_fails_after_one_rediscovery`, and concurrent refresh tests | PASS |
| Unsafe destinations, redirects, and IdP outages fail closed without another tenant/platform fallback | `ScreenedHttp::send` and `client_for`; error projection in `wyrd-auth/src/error.rs:34-75` | Exact unsafe URL, redirect-target-zero-request, and outage tests; tenant refusal journey | PASS |
| Workload RFC 7523 verification remains on `ExternalVerifier`; setup reads metadata without requiring immediate JWKS availability | `components/admin/routes.rs:735-775`; `boot/issuer.rs:220-264`; workload verifier callers remain in `jwt_bearer.rs` | Recorded admin and boot tests serve discovery while JWKS is unavailable; broader identity and Wyrd lanes pass | PASS |
| No provider-specific branch, bypass HTTP client, changed tenant selection, role mapping, issuance, audit, BFF/session/device-grant redesign, or new dependency entered the cumulative change | Cumulative base-to-candidate diff and traced callers | Recorded format/lint/boundary/codegen/docs/language and journey lanes | PASS |
| `FIND-TASK-009-1`: platform begin and callback share one process-owned cache and callback survives a post-begin discovery outage | `boot/mod.rs:1562-1627`; `components/auth/state.rs:14-47`; `components/platform/identity.rs:644-748` | Platform served callback journey step 5 | PASS |
| `FIND-TASK-009-2`: platform callback carries and validates RFC 9207 `iss` through the real route | `wyrd-spec/src/auth/platform_identity.rs:110-125`; `platform_login.rs:249-297`; served handler `components/platform/identity.rs:693-748` | Platform journey steps 2 and 4 | PASS |
| `FIND-TASK-009-3`: workload setup does not fetch JWKS | `ScreenedHttp::provider_metadata` at `relying_party.rs:197-247`; its two production callers above | Exact boot/admin tests record zero JWKS requests | PASS |
| `FIND-TASK-009-4`: overlapping cache misses and refreshes reuse Moka single-flight work | `relying_party.rs:377-415,539-548` | Exact `overlapping_cache_misses_share_one_discovery` and `concurrent_rotated_key_redemptions_share_one_refresh` | PASS |
| `FIND-TASK-009-6`: human audience is the configured client ID and no independent platform human audience remains | `CodeRedemption` at `relying_party.rs:316-332`; verifier construction at `:580`; platform request/view and migration remove `expected_audience` | Platform journey refuses the removed input and another-client token; codegen and SQL tests recorded green | PASS, subject to `BEH-R2-001` for multi-audience tokens |
| `FIND-TASK-009-7` through `-10`: unused HTTP accessor removed; library state/nonce defaults retained; helper rustdoc corrected; exact test evidence recorded | `relying_party.rs:450-476`; `connections.rs:936-975`; remediation evidence at `TASK-009-R1-relying-party-corrections.md:166-207` | Exact selectors each recorded one selected passing test; focused rerun below also selected one test | PASS |
| Withdrawn `FIND-TASK-009-5` | Human lead direction requires no correction and preserves conventional OAuth server-response diagnostics | N/A | EXCLUDED — not reopened |

## Proposed findings

### BEH-R2-001 — DRIFT: the human verifier marks every additional audience trusted

- **Violated obligation:** TASK-009 lines 30-44 and 96-106 require OIDC Core
  1.0 §3.1.3.7 validation through the selected library, including bad-audience
  refusal. The approved security posture accepts an external token only for an
  explicitly configured audience (`architecture/wyrd-security-posture.md:198-200`).
  OIDC Core §3.1.3.7 step 3 requires rejection when an ID token contains an
  additional audience the client has not trusted.
- **Exact location:** `crates/shared/wyrd-auth-oidc/src/relying_party.rs:620-665`,
  especially `id_token_verifier` line 638; contradictory acceptance proof at
  `crates/shared/wyrd-auth-oidc/src/relying_party.rs:1038-1043`.
- **Evidence and reachability:** `openidconnect 4.0.1` defaults its
  `other_aud_verifier_fn` to `false` and documents the OIDC step-3 rule. The
  candidate overrides that standard behavior with
  `set_other_audience_verifier_fn(|_| true)`. It then accepts
  `aud = [client_id, arbitrary-other]` whenever `azp = client_id`; the changed
  test explicitly proves that acceptance. `RelyingParty::verify` is the shared
  verifier reached by tenant login, connection-test completion, and platform
  login, so this is not dormant or test-only.
- **Observable consequence:** A signed token that is also addressed to an
  arbitrary, unconfigured audience can create a tenant user/session, mark a
  candidate tested, or mint a platform session. Wyrd has no additional-audience
  trust configuration, so the assertion that every other audience is trusted
  is neither derived from the approved contract nor the selected library's
  conventional behavior.
- **Required testable correction:** Delete the trust-all
  `set_other_audience_verifier_fn` override and keep the library's default
  rejection of additional audiences. Retain the existing local `azp` check for
  the cases it owns; add no audience option, allowlist, or second verifier.
  Change the focused ID-token test so a multi-audience token is refused even
  when `azp` names the client, while a single `client_id` audience with a valid
  or absent `azp` continues to pass. Exercise the shared path once and retain
  the tenant/platform journey coverage.

## Prior-finding closure

`FIND-TASK-009-1`, `-2`, `-3`, `-4`, `-6`, `-7`, `-8`, `-9`, and `-10`
are closed by current source and focused/recorded evidence as shown above.
`FIND-TASK-009-5` is withdrawn by human direction and was not reopened.

## Verification notes

- Reviewed the complete cumulative diff
  `35a53faa216b10651d85c96ce12e34f382cac637..1ddc10e21054ddc158f461e8c6d8aa862c32a067`
  and the remediation delta `0b516e235^..1ddc10e21054ddc158f461e8c6d8aa862c32a067`.
- Reviewed the exact-test and aggregate-lane evidence recorded on code candidate
  `0b516e235` in the remediation task; final candidate `1ddc10e21` changes only
  that durable evidence.
- Independently reran:
  `mise exec -- cargo nextest run --locked -p wyrd-auth-oidc --lib -E 'test(=relying_party::tests::id_token_refusals_fail_closed)'` — exit 0,
  one selected test passed. Its green result does not close `BEH-R2-001`
  because the same test asserts acceptance of the non-standard
  multi-audience case.
- Normative comparison: OpenID Connect Core 1.0 §3.1.3.7, OpenID Connect
  Discovery 1.0 §4, RFC 7636, and RFC 9207. No missing source or verification
  artifact blocked this behavior review.

## Overall result

**FAIL**

The remediation closes the prior active findings, but the cumulative candidate
still weakens the selected library's standard audience validation on every
human relying-party path. The smallest correction is deletion of the trust-all
override plus one focused refusal assertion.
