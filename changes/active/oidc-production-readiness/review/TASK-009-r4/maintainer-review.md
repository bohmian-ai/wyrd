# TASK-009 R4 maintainer review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `0bd3686e8bb763b07376f84661946aadc6200bfb`
- Candidate tree: `86fb8e11681b4d2e60bba2aa7ee3bf2c4f134153`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Current remediation: `changes/active/oidc-production-readiness/review/TASK-009-r3/TASK-009-R3-platform-login-boundary-and-contract.md`

The review used the complete cumulative base-to-candidate diff and the latest
remediation delta. `.codegraph/` is absent, so navigation used Git and direct
repository caller searches. The candidate object and tree remained unchanged.

`FIND-TASK-009-5`, `FIND-TASK-009-14`, and `FIND-TASK-009-15` are withdrawn by
binding lead direction. They were not reopened, renamed, or required through an
equivalent correction. In particular, the accepted
`PlatformLogin::relying_party` accessor is not a finding in this review.

## Changed-surface coverage

| Surface | Symbols and paths inspected | Caller, test, and declaration coverage | Assessment |
|---|---|---|---|
| Shared relying party and screened transport | `wyrd-auth-oidc/src/relying_party.rs`: provider metadata types, `ScreenedHttp::{send,provider_metadata}`, `RelyingPartyError`, `Authorization`, `CodeRedemption`, `VerifiedIdToken`, `RelyingParty::{new,cached,discover,fetch,authorize,redeem,verify}`, verifier/error helpers; `screening.rs`; crate exports and manifests | Traced tenant begin/callback, candidate testing, platform setup/login, workload metadata-only discovery, cache refresh, and inline tests | Cohesive concrete owner over the screened transport and one Moka cache. Library discovery, authorization, code redemption, and ID-token verification replace the deleted handwritten path without a second client, cache, provider branch, or configuration option. Public and private changed items have substantive rustdoc. |
| Tenant human login and connection lifecycle | `wyrd-auth/src/{connections,login,callback,error,pg_resolvers}.rs`; server `auth/{login,callback}.rs` | Traced state/nonce/PKCE production and persistence, RFC 9207 issuer binding, code redemption, candidate-test completion, identity/session issuance, and callback tests | Workflows remain discoverable through `HumanConnections` and `AuthorizationCodeExchange`. The callback no longer depends on the workload `ExternalVerifier`, and error conversion stays centralized. |
| Platform login, setup, and boot ownership | `wyrd-auth/src/platform_login.rs`; server `boot/{auth,mod}.rs`; `components/auth/state.rs`; `components/platform/identity.rs`; test-server construction | Traced the boot-built `PlatformLogin` through connection configuration, cache replacement, begin, callback, principal resolution, and session issuance | The process-owned service, method names, typed arguments, and return values make the flow followable. The dependency accessor and handler placement are expressly accepted by lead direction and have no independently reviewable behavioral, security, tenancy, durability, or public-contract consequence. |
| Workload assertion verification and issuer setup | `wyrd-auth-verify/src/lib.rs`; server `boot/issuer.rs`; `components/admin/routes.rs`; production JWT-bearer consumer | Traced metadata-only setup to the retained assertion-time key fetch and workload verifier | Human and workload verification ownership is clear. Metadata-only workload setup reuses the screened transport without restoring the deleted handwritten provider model. |
| Platform wire and persistence contract | `wyrd-spec/src/auth/platform_identity.rs`; `wyrd-sql/src/queries/platform/identity.rs`; `20261002000001_platform_oidc_client_audience.sql`; configure/read projections and SQL tests | Traced request/view/row parity, upsert/read callers, client-ID-derived audience, callback `iss`, migration, and served OpenAPI coverage | Rust source, SQL columns, route projections, and public declarations agree. The direct unreleased-column removal remains accepted by the binding withdrawal of `FIND-TASK-009-14`; no nonstandard migration mechanism is required. |
| R3 discovery failure contract | `components/platform/identity.rs::configure_connection`; `components/admin/routes.rs::discovery_error`; `pg_openapi_contract.rs::the_served_document_describes_the_composed_surface`; `platform_admin_e2e.rs::federated_platform_sign_in_runs_through_the_served_callback` | Traced unavailable and undecodable metadata/JWKS errors from `RelyingParty::discover` through the stable catalog and served response | `FIND-TASK-009-16` is closed. Rustdoc, the discovery comment, `utoipa`, runtime response, served failure assertions, and OpenAPI proof now agree on `503/WYRD_AUTH_503_DISCOVERY_UNAVAILABLE`; malformed or blocked input remains `400`. |
| Test fixtures, journeys, and dependency surface | relying-party inline tests; auth/server tests; `identity_e2e.rs`; `platform_admin_e2e.rs`; `wyrd-testing/src/{oidc_fixture,server}.rs`; `.config/nextest.toml`; Cargo manifests, lockfile, workspace-hack | Read changed helpers and tests, traced fixture construction through real owners, and inspected dependency/feature changes | Test names and setup expose the behavior they prove. `openidconnect 4.0.1` remains narrowly enabled without the `oauth2` `reqwest` feature. The nextest entry reuses the existing Postgres resource group rather than adding a harness. No Python, TypeScript, SDK, or handwritten generated artifact was introduced. |

## Prior-finding closure

| Finding | Candidate evidence | Result |
|---|---|---|
| `FIND-TASK-009-1` through `-4`, `-6` through `-13` | Their corrected owners and focused proofs remain present in the cumulative candidate: one boot-owned platform service, RFC 9207 binding, workload-only `ExternalVerifier`, coalesced cache refresh, client-ID-derived human audience, library generators, accurate helper rustdoc, exact selectors, strict additional-audience refusal, and full platform discovery before persistence. | **CLOSED** |
| `FIND-TASK-009-5` | Lead direction requires standard provider error logging and no added redaction mechanism. | **WITHDRAWN — NOT REOPENED** |
| `FIND-TASK-009-14` | Lead direction accepts the direct unreleased-column removal and prohibits the proposed preflight/overlap mechanism. | **WITHDRAWN — NOT REOPENED** |
| `FIND-TASK-009-15` | Lead direction expressly accepts the existing `PlatformLogin::relying_party` placement because moving the call has no behavioral effect and deletes no code. | **WITHDRAWN — NOT REOPENED** |
| `FIND-TASK-009-16` | Platform configuration now documents and declares the existing discovery/JWKS `503`; the served failure and OpenAPI tests assert its status, stable code, and problem schema. | **CLOSED** |

## Material findings

None.

## Non-blocking notes and uncertainties

None. No placement, naming, structure, wording, optional refactor, or
nonstandard mechanism is promoted to a finding.

## Verification assessment

The remediation's two exact, repository-wrapped selectors pass on the
candidate: `federated_platform_sign_in_runs_through_the_served_callback` runs
one test and passes, and `the_served_document_describes_the_composed_surface`
runs one test and passes. The recorded `fmt` and `lints` lanes also pass.
These are the narrowest lanes for the R3 write set; full journeys belong to
change review under the standing human direction. No new check, file, setting,
option, or broad aggregate is needed for maintainer acceptance.

## Overall result

**PASS**

Every materially changed symbol, its owner, relevant caller, focused test, and
public declaration was covered. The code is maintainable under the approved
task and binding lead directions, and no material maintainer finding remains.
