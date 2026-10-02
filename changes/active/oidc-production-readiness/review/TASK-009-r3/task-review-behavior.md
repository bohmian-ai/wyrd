# TASK-009 round-3 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `04597909203463820b2033c12956f5fe6fcfe1f4`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Prior verdict and ledger: `changes/active/oidc-production-readiness/review/TASK-009-r2/{verdict.md,findings-validation.md}`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-009-r2/TASK-009-R2-relying-party-corrections.md`
- Withdrawals: `review/TASK-009-r1/lead-direction-FIND-TASK-009-5.md` and `review/TASK-009-r2/lead-direction-FIND-TASK-009-14.md`

All source evidence below is commit-qualified at the candidate. `.codegraph/`
is absent, so navigation used repository search, the cumulative and remediation
diffs, and direct caller tracing. `FIND-TASK-009-5` and
`FIND-TASK-009-14` are withdrawn by lead direction and were not reopened.

## Behavior navigation and caller trace

- `wyrd_auth_oidc::RelyingParty::{cached,discover,authorize,redeem,verify}`
  owns human discovery, PKCE/state/nonce generation, token exchange, ID-token
  verification, and the process-local provider cache.
- Tenant login and connection testing converge through
  `HumanConnections::relying_party`, `LoginService::begin`, and
  `AuthorizationCodeExchange::{execute,complete}`.
- Platform configuration, begin, and callback converge on the boot-composed
  `ServerAuth::platform_login`; configuration calls that owner's fresh
  discovery, while begin and callback share its cache.
- Workload issuer setup remains on `ScreenedHttp::provider_metadata`, and
  workload RFC 7523 assertions remain on `ExternalVerifier`/`JwksCache`.
- Served negative proof is in `identity_e2e.rs` and the real
  `/platform/oidc/connection` plus `/auth/platform/{login,callback}` paths in
  `platform_admin_e2e.rs`.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Tenant login, candidate test sign-in, and platform login use `openidconnect` for discovery, PKCE/state/nonce, token redemption, and human ID-token validation | `relying_party.rs:364-665`; tenant callers in `login.rs`, `callback.rs`, and `connections.rs`; platform caller in `platform_login.rs:131-334` | Recorded exact relying-party/callback tests and identity journeys | PASS |
| Every relying-party provider request uses the screened, DNS-pinned, proxy-free, redirect-disabled, bounded transport; `oauth2`'s `reqwest` feature remains off | `relying_party.rs:155-260,430-435`; `screening.rs`; `openidconnect = 4.0.1` with `default-features = false` | Exact unsafe issuer/JWKS and redirect-target-zero-request tests; shared/lint evidence | PASS |
| ID tokens reject bad nonce, issuer, audience, algorithm, signature, expiry, future/missing `iat`, invalid subject, and an untrusted additional audience | `relying_party.rs:570-665`; the round-two trust-all override is deleted, retaining the library's default additional-audience refusal | `id_token_refusals_fail_closed`; served `tenant_callback_refusal_journey` now refuses `[client_id, other]` even with `azp = client_id` | PASS |
| A valid ordinary token, including a single client-ID audience with a matching present `azp`, still completes | `verify_authorized_party` permits matching `azp` after library audience verification | Unit success assertion and served refusal journey's final success case | PASS |
| RFC 9207 response issuer is checked before redemption for tenant, candidate-test, and platform callbacks | Tenant `callback.rs`; platform `platform_login.rs`; shared `verify_response_issuer` | Tenant issuer-binding journey and served platform callback prove missing/wrong `iss` reaches no token request | PASS |
| An unknown `kid` forces at most one fresh discovery; overlapping misses and refreshes coalesce locally; a still-unknown key fails closed | `RelyingParty::cached` uses Moka `try_get_with`; `redeem` invalidates and re-enters it once | Exact unknown-key, still-unknown-key, overlapping-miss, and concurrent-refresh tests recorded green | PASS |
| Unsafe discovery/JWKS destinations, token redirects, and provider outages fail closed without tenant/platform fallback | `ScreenedHttp`; relying-party error projection; no alternative-provider branch in the callers | Exact unsafe URL, redirect target, and outage proofs plus served refusal journeys | PASS |
| Workload setup reads metadata without requiring JWKS availability; workload verification remains on `ExternalVerifier` | Workload admin/boot call `discover_jwks_uri`/`provider_metadata`; production `jwt-bearer` wiring remains the only `ExternalVerifier` consumer | Exact workload admin and boot zero-JWKS tests; `test:wyrd` evidence | PASS |
| The removed human `ExternalVerifier` path is absent from behavior, maintained ownership documentation, and callback fixture wiring | Candidate `wyrd-auth-verify` and `ServerAuth` docs name workload-only use; callback fixture constructs only issuing and human-connection owners | Callback target 18/18 and source inspection; prior `FIND-TASK-009-12` closed | PASS |
| Platform human configuration performs standard full discovery through the same process owner used for login, stores the advertised JWKS URI, and refuses broken key sets without replacing durable configuration | `components/platform/identity.rs:196-282` calls `PlatformLogin::relying_party().discover`; `RelyingParty::discover` inserts only after successful library discovery | Served platform journey covers unavailable/undecodable JWKS, unchanged stored connection, and same-issuer key refresh used through an outage | PASS |
| Human audience is derived only from client ID; there is no independent public audience setting | `CodeRedemption`, platform request/view, resolver, verifier, and SQL use the client ID; the old request field is refused | Platform served journey and codegen/SQL evidence | PASS |
| Platform audience column removal remains as directed by the lead | Candidate migration drops the obsolete independent audience column | N/A | EXCLUDED — withdrawn `FIND-TASK-009-14`, not reopened |
| Conventional OAuth provider diagnostics remain as directed by the lead without exposing Wyrd-owned secrets | Existing token error projection is unchanged by remediation | Existing negative paths and source inspection | EXCLUDED — withdrawn `FIND-TASK-009-5`, not reopened |
| No hand-written discovery, PKCE/state/nonce generation, token POST, or human JWT verifier remains | Replaced helpers/types are absent; `openidconnect` owns those protocol operations | Cumulative source and dependency inspection | PASS |
| No provider-specific branch, second HTTP client, changed tenant selection, role mapping, issuance, audit, session/device-grant design, or speculative option/check entered the behavior | Cumulative base-to-candidate diff and caller trace | Recorded scoped lanes and source inspection | PASS |
| Prior active findings remain closed | `FIND-TASK-009-1` through `-4`, `-6` through `-13` have the shared-owner corrections described above and in the prior ledgers | Recorded exact focused proof and narrow owner lanes | PASS |

## Proposed findings

None. The cumulative implementation satisfies the task's observable behavior,
and the round-three remediation closes the active round-two findings without a
new mechanism or behavior extension.

## Prior-finding closure

- `FIND-TASK-009-1` through `-4` and `-6` through `-10` remain closed by the
  process-owned platform service, RFC 9207 route proof, workload-only metadata
  path, Moka single-flight use, client-ID-derived audience, deletion of drift,
  corrected rustdoc, and exact selector evidence.
- `FIND-TASK-009-11` is closed: the trust-all additional-audience callback is
  deleted, and both focused and served proofs refuse the formerly accepted
  token while preserving the ordinary single-audience case.
- `FIND-TASK-009-12` is closed: maintained docs and callback fixture wiring now
  reflect workload-only `ExternalVerifier` ownership.
- `FIND-TASK-009-13` is closed: platform setup uses the existing boot-owned
  relying party's standard full discovery and cache.
- `FIND-TASK-009-5` and `FIND-TASK-009-14` remain withdrawn and excluded.

## Verification notes

- Reviewed the complete cumulative diff
  `35a53faa216b10651d85c96ce12e34f382cac637..04597909203463820b2033c12956f5fe6fcfe1f4`
  and remediation delta
  `1ddc10e21054ddc158f461e8c6d8aa862c32a067..04597909203463820b2033c12956f5fe6fcfe1f4`.
- The remediation record contains zero-selection-safe exact commands for the
  relying-party refusal/success tests, callback target, workload boot/admin
  tests, platform configuration/login journeys, and the tenant refusal
  journey; all recorded exit 0. It also records green `fmt`, `lints`,
  `codegen:check`, documentation and boundary checks, `test:shared`,
  `test:principals:integration`, and `test:wyrd`.
- The `platform_admin_e2e` fixture concurrency failure was diagnosed as
  Postgres connection exhaustion and corrected with the existing
  `postgres-fixtures` nextest group; the affected exact journey then passed.
- Per the standing verification rule from commit `518026d54`, task review
  requires only the narrowest lanes covering the write set. Full journey and
  every-language sweeps run once at change review, so their absence on the
  final remediation is not a task-review gap.
- No missing source, required reviewer input, or behavior proof blocks this
  review.

## Overall result

**PASS**

The candidate satisfies TASK-009's behavior and closes every non-withdrawn
prior finding examined through this behavior lens.
