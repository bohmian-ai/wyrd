# TASK-009 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `0bd3686e8bb763b07376f84661946aadc6200bfb`
- Candidate tree: `86fb8e11681b4d2e60bba2aa7ee3bf2c4f134153`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-009-r3/TASK-009-R3-platform-login-boundary-and-contract.md`
- Complete range: `35a53faa216b10651d85c96ce12e34f382cac637..0bd3686e8bb763b07376f84661946aadc6200bfb`
- Latest remediation delta: `04597909203463820b2033c12956f5fe6fcfe1f4..0bd3686e8bb763b07376f84661946aadc6200bfb`

The requested commit and tree were unchanged before and after this review.
`.codegraph/` is absent, so navigation used commit-qualified Git source and
direct caller and writer tracing.

`FIND-TASK-009-5`, `FIND-TASK-009-14`, and `FIND-TASK-009-15` are withdrawn by
explicit lead direction. I did not reopen, rename, reassess, or indirectly
require any of them. In particular, the public `PlatformLogin::relying_party`
accessor is accepted as directed: its placement has no behavioral, security,
tenancy, durability, or public-contract consequence. No nonstandard mechanism,
check, file, setting, or option is required by this review.

## Producer-to-sink invariant trace

- `HumanConnections` owns the tenant human `RelyingParty`. Library-produced
  state, nonce, and PKCE verifier are stored with the exact tenant connection
  revision, issuer, client ID, redirect, and initiation binding. The callback
  resolves the tenant only from the hashed server state, consumes that state
  before provider I/O, re-resolves the bound connection, checks RFC 9207 before
  redemption, verifies the ID token, and rechecks the connection before issuing
  authority (`wyrd-auth/src/{login,callback,connections}.rs`). No route, header,
  email, or unverified provider value selects a tenant or connection.
- Boot constructs one process-owned `PlatformLogin` in `ServerAuth`. Platform
  configuration, begin, and callback borrow it, so full setup discovery and
  both login phases share one platform provider cache without joining the
  tenant cache (`components/auth/state.rs`, `boot/mod.rs`,
  `components/platform/identity.rs:209-288`).
- Platform callback state remains single-use and server-bound. Completion
  consumes it, requires the current connection to retain the recorded issuer,
  checks the optional RFC 9207 response issuer before token redemption, and
  hands only verified issuer, subject, and claims to `PlatformSessions`
  (`wyrd-auth/src/platform_login.rs:83-346`). Platform and tenant authority
  remain separate.
- `CodeRedemption` has one human audience authority, `client_id`; the verifier
  uses it and applies the standard authorized-party rules. No independent
  platform human audience survives in the request, view, SQL row, or verifier.
  Workload `expected_audience` remains a separate RFC 7523 contract
  (`wyrd-auth-oidc/src/relying_party.rs:316-332,553-665`).
- `RelyingParty::cached` uses Moka `try_get_with`; clones share the cache and a
  failed fetch is not cached. An unknown key invalidates the issuer and
  re-enters that same coalesced path once, after which verification terminates
  (`relying_party.rs:377-415,479-550`). No second cache, custom lock, or
  cross-replica promise was added.
- All human discovery, JWKS, and token requests traverse the single
  `ScreenedHttp` adapter. It preserves DNS resolution and pinning, address and
  scheme screening, no redirects, no proxy, timeouts, and the response-size
  cap. Workload setup remains metadata-only through
  `ScreenedHttp::provider_metadata`; workload key retrieval remains with
  `ExternalVerifier` and `JwksCache`.
- Platform configuration performs metadata-plus-JWKS discovery before the SQL
  write and maps unavailable, undecodable, or issuer-mismatched discovery to
  the existing `503/WYRD_AUTH_503_DISCOVERY_UNAVAILABLE`; a failed discovery
  reaches no durable connection write (`components/platform/identity.rs:173-288`,
  `components/admin/routes.rs:772-786`). The route declaration and served
  OpenAPI now publish that same contract, and the route test pins both status
  and stable code (`pg_openapi_contract.rs:106-179`,
  `platform_admin_e2e.rs:1607-1650`).

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003 / AC-006: connection testing performs the real sign-in against the exact candidate and creates no User, credential, or session | Candidate-bound `LoginState`; `HumanConnections::tested_candidate` and `stamp_test_sign_in`; the common callback exits through the test branch before tenant issuance | Recorded identity journey and exact callback/connection tests from the cumulative remediation evidence | PASS |
| REQ-004: human setup uses typed OIDC discovery and JWKS, validates issuer, derives audience from client ID, and refuses unsupported client authentication | `RelyingParty::{fetch,authorize,redeem,verify}`; client-ID-only `CodeRedemption`; platform configure performs full discovery | Exact relying-party refusal tests, served platform test, and codegen evidence recorded green | PASS |
| REQ-006 / INV-001: unverified request/provider values cannot select effective tenant or connection after login starts | Tenant and platform flows consume opaque server state and require the current durable connection to match its binding before authority issuance | Wrong-state, replay, replaced/inactive connection, cross-tenant, and issuer-binding evidence recorded green | PASS |
| REQ-007: authorization code flow uses S256 PKCE, library state/nonce, exact redirect, and complete ID-token validation | `relying_party.rs:438-665`; all human producers use that owner | Exact success and negative relying-party selectors and tenant callback refusal journey recorded green | PASS |
| RFC 9207 binds tenant, connection-test, and platform responses before token redemption | Shared `verify_response_issuer`; platform callback carries optional `iss`; both completion owners check it before `redeem` | Tenant issuer-binding journey and served platform journey prove match, advertised-missing, and mismatch, including zero token requests on refusal | PASS |
| INV-004 / AC-007: unsafe destinations, redirects, outages, and unknown signing keys fail closed | One screened adapter; no alternate client; one terminal unknown-key refresh | Exact unsafe issuer/JWKS, redirect, outage, unknown-key, and cache-concurrency evidence recorded green | PASS |
| Per-issuer process cache coalesces cold/expired misses and rotation refreshes | Moka `try_get_with`; unknown-key path invalidates and re-enters the same cache once; tenant and platform owners remain separate | `overlapping_cache_misses_share_one_discovery` and `concurrent_rotated_key_redemptions_share_one_refresh` recorded green | PASS |
| Platform setup refuses unavailable/undecodable JWKS without replacing durable configuration and refreshes its process cache on success | Full discovery precedes `upsert_platform_oidc_connection`; provider insertion occurs only after successful fetch | Independently rerun `federated_platform_sign_in_runs_through_the_served_callback`: 1 selected, 1 passed | PASS |
| FIND-TASK-009-16: the public platform setup contract declares its reachable discovery/JWKS `503` | Handler rustdoc and `utoipa` response name `503/WYRD_AUTH_503_DISCOVERY_UNAVAILABLE`; served OpenAPI exposes `WyrdProblem` for it | Independently rerun `the_served_document_describes_the_composed_surface`: 1 selected, 1 passed | PASS |
| Workload setup remains metadata-only and workload verification retains JWKS ownership | Admin and boot call `ScreenedHttp::provider_metadata`; workload `ExternalVerifier`/`JwksCache` wiring remains | Exact workload admin/boot zero-JWKS tests and Wyrd server lane recorded green | PASS |
| Platform and tenant principal/authorization planes remain distinct | Platform identity reaches `PlatformSessions`; tenant identity reaches tenant issuance and role owners | Served platform journey proves platform authority succeeds on a platform operation and is refused on `/v1/cards` | PASS |
| No handwritten human discovery, PKCE/state/nonce generation, token POST, or human `ExternalVerifier` remains | Human paths use `ProviderMetadata::discover_async`, library random constructors, S256, library code exchange, and library claims verification | Source/caller trace and recorded shared/auth/server lanes | PASS |
| `oauth2`'s `reqwest` feature stays disabled and no unscreened human client exists | `openidconnect = 4.0.1` remains default-features-off; `ScreenedHttp` is the injected `AsyncHttpClient` | Workspace-hack, boundary, lint, and source feature evidence recorded green | PASS |
| Prohibited provider branches, duplicate trust/configuration, BFF/device/session redesign, and distributed cache coordination are absent | Complete cumulative diff and caller trace show one provider-agnostic human RP and unchanged adjacent owners | Source inspection and narrow owner lanes | PASS |
| Task-level verification is narrowest-lane and zero-selection-safe; full journeys remain for change review | R3 evidence records the affected server/OpenAPI tests and narrow Rust/server lanes; this review ran the two exact selectors through the repository Postgres wrapper | Both exact selectors passed; no every-language or full journey rerun is required here | PASS |

## Prior-finding closure

| Prior finding | Current source evidence | Result |
|---|---|---|
| `FIND-TASK-009-1` | One boot-owned `PlatformLogin` is reused by served setup, begin, and callback; outage-after-begin is proved | CLOSED |
| `FIND-TASK-009-2` | Platform callback carries optional `iss` and verifies it before redemption through the real route | CLOSED |
| `FIND-TASK-009-3` | Workload admin and boot remain metadata-only; workload JWKS stays with `ExternalVerifier` | CLOSED |
| `FIND-TASK-009-4` | Native Moka coalescing owns misses and the single unknown-key refresh | CLOSED |
| `FIND-TASK-009-5` | Explicit lead direction withdraws the finding | WITHDRAWN — NOT REOPENED |
| `FIND-TASK-009-6` | Human audience is derived solely from `client_id`; no second platform setting remains | CLOSED |
| `FIND-TASK-009-7` | The unused lower-level `RelyingParty::http` accessor remains absent | CLOSED |
| `FIND-TASK-009-8` | State, nonce, and S256 PKCE use the library constructors | CLOSED |
| `FIND-TASK-009-9` | The changed connection helpers retain accurate adjacent rustdoc | CLOSED |
| `FIND-TASK-009-10` | Durable evidence records exact, non-zero-selection commands for the named TASK-009 tests | CLOSED |
| `FIND-TASK-009-11` | The additional-audience override is absent and standard library refusal is covered | CLOSED |
| `FIND-TASK-009-12` | Human `ExternalVerifier` wiring is absent; workload wiring remains | CLOSED |
| `FIND-TASK-009-13` | Platform setup performs full discovery through the process-owned RP and refreshes the same cache | CLOSED |
| `FIND-TASK-009-14` | Explicit lead direction withdraws the migration finding | WITHDRAWN — NOT REOPENED |
| `FIND-TASK-009-15` | Explicit lead direction withdraws the placement-only accessor finding | WITHDRAWN — NOT REOPENED |
| `FIND-TASK-009-16` | Route docs/OpenAPI declare the reachable discovery `503`; served failures assert its exact status and stable code | CLOSED |

## Proposed findings

None. The cumulative candidate preserves the task's state, trust, cache,
tenancy, workload/human separation, and issuance invariants. The R3 behavioral
contract gap is closed, and the three human-withdrawn findings remain closed to
review.

## Verification assessment

The R3 remediation evidence records `fmt` and `lints` green on the final
FIND-15-reverted candidate, and records the affected platform and OpenAPI proof
green. This review independently ran the exact, zero-selection-safe commands
for `federated_platform_sign_in_runs_through_the_served_callback` and
`the_served_document_describes_the_composed_surface` inside the repository
Postgres setup; each selected one test and passed. Prior focused and aggregate
evidence remains applicable to the unchanged cumulative owners. Under the
standing narrowest-lane direction, full identity and every-language journeys
belong to final change review and are not a task-review verification limit.

## Overall result

**PASS**
