# TASK-009 R3 maintainer review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `04597909203463820b2033c12956f5fe6fcfe1f4`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Current remediation: `changes/active/oidc-production-readiness/review/TASK-009-r2/TASK-009-R2-relying-party-corrections.md`

The review used commit-qualified source for the immutable candidate. CodeGraph
is not indexed in this repository, so caller and consumer tracing used Git and
repository search. I read the complete cumulative base-to-candidate diff and
used `1ddc10e2..045979092` only to locate the R2 correction sites.
`FIND-TASK-009-5` and `FIND-TASK-009-14` remain withdrawn by their binding lead
directions and were not reopened.

## Changed-surface coverage

| Surface | Symbols and paths inspected | Caller, test, and declaration coverage | Assessment |
|---|---|---|---|
| Shared relying party and screened transport | `wyrd-auth-oidc/src/relying_party.rs`: provider metadata, `ScreenedHttp::{send,provider_metadata}`, `RelyingPartyError`, `Authorization`, `CodeRedemption`, `VerifiedIdToken`, `RelyingParty::{new,cached,discover,fetch,authorize,redeem,verify}`, verifier/error helpers; `screening.rs`; crate exports and manifests | Traced tenant begin/callback, candidate testing, platform login/setup, workload metadata discovery, test fixtures, cache refresh, and inline tests | The concrete `RelyingParty` remains the cohesive protocol owner. R2 correctly deletes the nonstandard audience override and adds no audience option, verifier, or provider branch. |
| Tenant login, connection testing, and callback | `wyrd-auth/src/{login,callback,connections,error,pg_resolvers}.rs`; server `auth/{login,callback}.rs` | Traced state creation and consumption, RFC 9207, provider redemption, candidate stamping, identity/session issuance, and callback tests | Workflows remain discoverable through `HumanConnections` and `AuthorizationCodeExchange`; no human `ExternalVerifier` dependency remains. |
| Workload assertion verification | `wyrd-auth-verify/src/lib.rs`; server `boot/auth.rs`, `components/auth/state.rs`, and `auth/jwt_bearer.rs` | Traced `ExternalVerifier` construction through the sole production JWT-bearer consumer and the corrected callback fixture | R2 removed stale human/platform ownership prose and dead callback setup while preserving workload verification. Documentation now names the real production consumer. |
| Platform login and setup | `wyrd-auth/src/platform_login.rs`; server `boot/mod.rs`, `components/auth/state.rs`, and `components/platform/identity.rs`; test-server construction | Traced the one boot-owned `PlatformLogin` through configure, begin, callback, cache refresh, and session issuance | Full discovery now uses the correct process cache and preserves behavior. The setup handler reaches through that owner via a public dependency accessor; see `MNT-R3-001`. Its public failure declaration is incomplete; see `MNT-R3-002`. |
| Workload issuer administration and boot | server `components/admin/routes.rs` and `boot/issuer.rs` | Traced both metadata-only callers, shared `discovery_error`, and zero-JWKS focused tests | Workload setup remains metadata-only and uses the existing screened transport. Extracting the stateless error projection avoids duplicate mappings without adding a service or option. |
| Platform wire and persistence contract | `wyrd-spec/src/auth/platform_identity.rs`; `wyrd-sql/src/queries/platform/identity.rs`; migration `20261002000001_platform_oidc_client_audience.sql`; route projections and SQL tests | Traced configure/read request and view shapes, row decoding, read/write callers, migration, and generated-contract evidence | `client_id` is the only public human audience value. The separately stored platform audience is removed as directed. `FIND-TASK-009-14` remains withdrawn; no overlap mechanism is required. |
| Tests, fixtures, and runner configuration | relying-party inline tests; server callback tests; `identity_e2e.rs`; `platform_admin_e2e.rs`; `wyrd-testing` OIDC/server fixtures; `.config/nextest.toml` | Read the focused audience refusal, failed/fresh platform JWKS setup, same-issuer refresh, cache, workload, and callback proofs plus the recorded exact selectors | Tests assert caller-visible behavior through existing surfaces. The nextest override reuses the established `postgres-fixtures` group after a recorded connection-exhaustion diagnosis; it is not a new harness or setting exposed to users. The platform setup test does not assert its documented status/code; see `MNT-R3-002`. |
| Dependency and declaration parity | workspace/crate manifests, lockfile, workspace-hack, Rust wire types, and recorded codegen checks | Confirmed `openidconnect 4.0.1` remains narrowly enabled without the `oauth2` reqwest feature; no Python, TypeScript, SDK, or generated source changed in R2 | No new dependency, feature, compatibility path, public option, or handwritten generated artifact was introduced by R2. The task-level verification correctly retained prior language evidence under standing direction `518026d54`. |

## Prior-finding closure

| Prior finding | Candidate evidence | Result |
|---|---|---|
| `FIND-TASK-009-1` through `-4`, and `-6` through `-10` | The owners and proofs recorded in the prior validation remain present in the cumulative candidate. | **CLOSED** |
| `FIND-TASK-009-5` | Binding lead direction requires no change to standard provider error logging. | **WITHDRAWN — NOT REOPENED** |
| `FIND-TASK-009-11` | `id_token_verifier` uses `openidconnect`'s default additional-audience refusal; focused and served refusal tests cover `[client_id, other]` with matching `azp`. | **CLOSED** |
| `FIND-TASK-009-12` | Workload-only rustdoc replaces the stale human claims, and the callback fixture no longer constructs `PgIssuerResolver`, `JwksCache`, or `ExternalVerifier`. | **CLOSED** |
| `FIND-TASK-009-13` | Platform setup performs fresh full discovery through the process-owned cache; the served journey proves failed JWKS leaves the row unchanged and same-issuer reconfiguration supplies the next callback's key set. | **FUNCTIONALLY CLOSED**; the correction leaves the independent owner/API and declaration findings below. |
| `FIND-TASK-009-14` | Binding lead direction requires the shipped conventional column removal and no preflight, derived overlap column, or dual write. | **WITHDRAWN — NOT REOPENED** |

## Material findings

### MNT-R3-001 — platform setup exposes the owner's relying-party dependency instead of its operation

- **Classification:** `DRIFT`
- **Changed locations:** `crates/wyrd/wyrd-auth/src/platform_login.rs:128-141`; `crates/wyrd/wyrd-server/src/components/platform/identity.rs:235-244`
- **Governing rule:** `AGENTS.md` §5 and the maintainer guide require stateful workflows to be inherent methods on their concrete owner; callers should discover the operation through that owner rather than reach through it to a dependency. Standing direction rejects extra public mechanisms that the standard flow does not need.
- **Evidence:** `PlatformLogin` owns the one `RelyingParty` and its process cache, but R2 adds public `PlatformLogin::relying_party() -> &RelyingParty` solely so one server handler can call `.discover(&issuer)` and then project `jwks_uri`. Repository search finds no second caller. The accessor exposes every lower-level public relying-party operation, although platform setup needs only one fresh-discovery result.
- **Concrete maintenance cost:** the public `wyrd-auth` surface now offers a route around `PlatformLogin`'s cohesive workflow boundary. A future platform caller can independently invoke cached discovery, authorization, or redemption through the exposed dependency, while a maintainer changing platform setup must understand that the actual operation lives across two owners and a server handler.
- **Smallest testable correction:** delete the accessor. Put one narrowly named fresh-discovery method on `PlatformLogin` that calls its owned `RelyingParty::discover` and returns the advertised JWKS URL needed by setup; have the configure handler call that method. Reuse the existing `RelyingPartyError`/error projection and cache. Add no trait, second cache, invalidation service, option, or wrapper type.
- **Closure proof:** repository search shows no public `PlatformLogin::relying_party` accessor; the existing served platform journey still proves failed JWKS discovery preserves the row and same-issuer reconfiguration refreshes the provider used by the next begin/callback.

### MNT-R3-002 — platform configuration's declared failure contract omits discovery/JWKS unavailability

- **Classification:** `INCORRECT`
- **Changed locations:** `crates/wyrd/wyrd-server/src/components/platform/identity.rs:173-191,221-243`; `crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:1607-1633`
- **Governing rule:** the maintainer guide requires public documentation and declarations to match behavior; `AGENTS.md` requires typed stable public errors and generated documentation parity. R2 specifically adds full provider/JWKS discovery to this public handler.
- **Evidence:** fresh discovery maps an unreachable or undecodable discovery document or key set through `WyrdError::DiscoveryUnavailable`, whose catalog status is `503` and code is `WYRD_AUTH_503_DISCOVERY_UNAVAILABLE`. The route rustdoc omits discovery failure, and its `utoipa` responses declare 200/400/401/403/500 but no 503; the 400 description incorrectly includes an unreachable issuer. The adjacent comment still says discovery is the only network call even though `openidconnect` performs the discovery-document request and the advertised JWKS request. The new journey checks only `!status.is_success()`, so it would not catch declaration drift or an accidental 400/500 mapping.
- **Concrete maintenance cost:** generated OpenAPI clients and operators are told that provider/JWKS unavailability is a caller validation failure or is impossible, while the server returns a retryable 503. Maintainers also get contradictory side-effect documentation immediately above the two-request full-discovery call.
- **Smallest testable correction:** update the existing handler rustdoc/comment and `utoipa` response list to state that full discovery fetches metadata and JWKS and returns `503 WYRD_AUTH_503_DISCOVERY_UNAVAILABLE` for unavailable or undecodable provider data; keep blocked-address and invalid-input cases at 400. Tighten the existing unavailable/undecodable-key-set loop to assert that status and stable code. Add no new error, document, schema mechanism, or test harness.
- **Closure proof:** the focused served platform configure test asserts 503 plus `WYRD_AUTH_503_DISCOVERY_UNAVAILABLE`, and the repository's existing served-OpenAPI check confirms the route advertises the same response.

## Calibration preferences

- `relying_party.rs` is large, but its production portion is one cohesive capability and repository rules keep unit tests inline. Splitting it by line count would worsen navigation; this is not a finding.
- The long platform journey remains one stateful configured-provider workflow. Its added setup/refusal/cache steps reuse the same server and provider state, so splitting it would duplicate substantial setup without clarifying ownership.
- The `platform_admin_e2e` nextest override uses the repository's existing resource group and records a concrete Postgres connection diagnosis. A different scheduling mechanism would add complexity without improving the test contract.
- The two adjacent `impl ScreenedHttp` blocks could be merged, but both methods remain easy to find and the split creates no material maintenance cost.

## Verification assessment

The R2 record supplies exact, non-zero-selection commands for the changed
relying-party, callback, workload-discovery, platform configuration, and
identity refusal tests. It records the narrow repository lanes for the Rust,
server, contract, format, lint, boundary, and workspace-hack write set as
green. Under standing human direction `518026d54`, not rerunning the full
every-language sweep at this task review is not a gap: R2 changed no Python,
TypeScript, SDK, or generated source, and `codegen:check` was green. The two
findings above use existing focused tests and the existing served-OpenAPI lane;
they require no broad aggregate or new verification mechanism.

## Overall result

**FAIL**

The R2 behavioral corrections use the selected library and existing caches
without introducing a new protocol mechanism. Acceptance is still blocked by
one avoidable public dependency escape hatch and one public error/declaration
mismatch at the newly changed platform full-discovery seam.
