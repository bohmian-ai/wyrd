# Maintainer review

## Subject

- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `6578921d8ced6316c850e4d8f16bd101630a7056`
- Task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Routed direction: `FIND-TASK-004-13` in `review/TASK-004-r2/lead-direction-routing.md`

The candidate remained checked out at the requested commit throughout this review.

## Changed-surface coverage

| Surface | Symbols and paths inspected | Caller, test, and declaration coverage | Assessment |
|---|---|---|---|
| Relying-party owner and screened transport | `wyrd-auth-oidc/src/relying_party.rs`: `IssuerParameterMetadata`, `OtherClaims`, `ProviderHttpError`, `RelyingPartyError`, `Authorization`, `CodeRedemption`, `VerifiedIdToken`, `RelyingParty::{new,cached,discover,authorize,redeem,verify}`, `ScreenedHttp`'s `AsyncHttpClient` implementation, private verifier/error helpers | Traced every repository construction and method call into tenant login, candidate testing, platform login, boot discovery, admin discovery, and test fixtures; read the complete inline test module | Findings MNT-001 through MNT-003. The concrete `RelyingParty` is otherwise the right owner: it owns the screened transport and cache, and its inherent methods replace the former cross-module protocol workflow. |
| Deleted hand-written provider and residual workload verification | Deleted `wyrd-auth-oidc/src/provider.rs`; reduced `jwks.rs` and `error.rs`; `wyrd-auth-verify/src/lib.rs` deletion of the human `verify_id_token_against` path | Confirmed `ExternalVerifier` retains the workload assertion path and its renamed focused test; inspected crate exports in `wyrd-auth-oidc/src/lib.rs` | Clear owner separation. Human ID-token behavior moved to the relying party while workload JWT verification stays with `ExternalVerifier`. |
| Tenant connection/login/callback consumers | `wyrd-auth/src/{connections,login,callback,error}.rs`: `HumanConnections`, login initiation, `AuthorizationCodeExchange::{execute,finish_id_token_exchange}`, relying-party error projection and candidate checks | Traced provider metadata, state, nonce, PKCE verifier, connection revision, mapped claims, and errors from login initiation through callback completion; inspected unit/Postgres tests and the server callback adapter | Call flow is discoverable and the protocol work is no longer duplicated in these consumers. MNT-001 originates in the shared input they construct. |
| Platform login | `wyrd-auth/src/platform_login.rs`: `PlatformLogin::{new,begin,authorize,complete}`, `PlatformConnection`, `verified_email`, `provider_unavailable` | Traced server construction and the platform login tests, including the decoded stored connection | The owner/method layout is cohesive. MNT-001 leaves its token audience representable independently from its OAuth client identity. |
| Adjacent auth compilation change | `wyrd-auth/src/cli_logins.rs`: `new_device_secret` replacing the deleted login helper | Traced the sole call and its existing device-grant tests | Small, local replacement using already-installed primitives; no new abstraction or option. |
| Server wiring and discovery consumers | `wyrd-server/src/auth/{callback,login}.rs`, `boot/issuer.rs`, `components/admin/{identity,routes}.rs`, `components/platform/identity.rs` | Traced `AppState` construction, tenant/platform service construction, workload issuer boot/admin discovery, and their focused tests | Wiring consistently consumes the shared owner. One-shot boot/admin discovery does not create a second protocol implementation. |
| User journeys and fixtures | `wyrd-server/tests/identity_e2e.rs`, `wyrd-testing/src/oidc_fixture.rs`, `wyrd-cli/tests/operator_journey.rs` | Read changed issuer-binding, refusal, discovery, JWKS, Keycloak/Dex fixture, and CLI journey setup | Tests assert caller-visible outcomes and use the real library path. The second issuer in the RFC 9207 cache scenarios makes each discovery document stable for its issuer and is clearer than mutating one issuer's metadata behind the cache. |
| Manifests and generated dependency union | workspace/crate `Cargo.toml`, `Cargo.lock`, `workspace-hack/Cargo.toml` | Inspected the complete base-to-candidate dependency diff and the isolated `9ef532660` regeneration diff | `openidconnect = 4.0.1` is narrowly enabled without its `reqwest` feature. Workspace-hack changes are generated feature-union updates, not a new handwritten configuration surface. |
| Generated/public declaration parity | Rust exports and public wire consumers | No Python, TypeScript, OpenAPI request/response, schema, or generated-stub shape changed | No generated declaration update is required. |

## Material findings

### MNT-001 — `CodeRedemption` represents the OIDC client audience twice

- Classification: `DRIFT`
- Changed locations: `crates/shared/wyrd-auth-oidc/src/relying_party.rs:265-279`, `crates/shared/wyrd-auth-oidc/src/relying_party.rs:517`, and `crates/shared/wyrd-auth-oidc/src/relying_party.rs:563-570`
- Governing authority: `spec.md` REQ-004 says the human ID-token audience is derived from the configured client ID rather than entered twice; the maintainer guide requires APIs that make invalid states difficult to represent and named structs only for meaningful, non-conflicting records.
- Evidence: `CodeRedemption` accepts both `client_id` and `audience`; `redeem` constructs the OAuth client with the former, while `verify` constructs the ID-token verifier with the latter. Both production callers repeat separate fields: tenant callback passes `trusted.client_id` and `trusted.expected_audience`, while platform login passes `connection.client_id` and `connection.verification.expected_audience`.
- Concrete maintenance cost: every human-login caller must know that two fields describe the same OIDC client, and the shared API permits them to disagree. A maintainer can therefore change connection decoding or add a caller that exchanges a code as one client but verifies `aud` as another. That is unnecessary state beyond the conventional relying-party contract.
- Smallest testable correction: delete `CodeRedemption::audience` and construct `IdTokenVerifier` from `CodeRedemption::client_id`. Update the two production initializers and the focused relying-party tests. Do not add a replacement setting or compatibility path.
- Closure proof: a focused relying-party test must verify that an ID token whose `aud` differs from the code-exchange client ID is refused, with no independent audience argument available to the caller; rerun the identity journey.

### MNT-002 — the new owner exposes an unused transport accessor

- Classification: `DRIFT`
- Changed location: `crates/shared/wyrd-auth-oidc/src/relying_party.rs:325-329`
- Governing authority: the maintainer guide says an owner's public methods should expose its workflows; the standing direction rejects mechanisms and options with no established or conventional consumer.
- Evidence: `RelyingParty::http` has no repository caller. All relying-party HTTP is already encapsulated by `discover` and `redeem`, and the remaining workload JWKS path owns its `ScreenedHttp` independently.
- Concrete maintenance cost: this widens the shared crate API and offers future callers a second, lower-level route around the owner's discover/redeem workflow without satisfying any current task consumer.
- Smallest testable correction: delete `RelyingParty::http` and its rustdoc. No replacement is required.
- Closure proof: repository compilation and `rg` show no accessor or call remains.

### MNT-003 — state and nonce generation overrides the library's conventional default without an approved need

- Classification: `DRIFT`
- Changed locations: `crates/shared/wyrd-auth-oidc/src/relying_party.rs:64-65` and `crates/shared/wyrd-auth-oidc/src/relying_party.rs:402-406`
- Governing authority: the task requires library-generated `CsrfToken` and `Nonce`; the standing direction requires the established standard or comparable widely used project mechanism and rejects an extra setting that neither requires.
- Evidence: the candidate adds `RANDOM_VALUE_BYTES = 32` and calls `CsrfToken::new_random_len` / `Nonce::new_random_len`. `openidconnect` 4.0.1 provides `CsrfToken::new_random` and `Nonce::new_random` as its 128-bit defaults and uses those exact constructors in its own relying-party documentation and examples. No task, Wyrd authority, caller, or provider interoperability requirement asks Wyrd to tune these lengths.
- Concrete maintenance cost: Wyrd now owns a security parameter and must explain and preserve a non-default choice even though the selected library already supplies the conventional safe generation path.
- Smallest testable correction: delete `RANDOM_VALUE_BYTES` and pass `CsrfToken::new_random` and `Nonce::new_random` directly to `authorize_url`. Keep `PkceCodeChallenge::new_random_sha256` unchanged.
- Closure proof: the authorization-request test still proves nonempty state/nonce and the `S256` challenge; no Wyrd-owned random-length setting remains.

## Verification assessment

The recorded verification is credible for the maintained surfaces. In particular, rerunning only `mise run lints` and `mise run check:workspace-hack` after `9ef532660` is sufficient for that mechanical regeneration:

- the commit changed only `workspace-hack/Cargo.toml` and the generated `workspace-hack` dependency list in `Cargo.lock`;
- the added crypto features were already selected transitively by the `openidconnect` code exercised by the earlier identity and crate test lanes, so the regeneration did not introduce a new OIDC runtime branch;
- workspace lints compile the all-features workspace against the regenerated feature union; and
- `check:workspace-hack` proves the generated manifest and lock entry match Hakari's current calculation.

The earlier behavioral results therefore remain applicable. A second full identity journey after the Hakari-only commit would repeat already-exercised runtime behavior rather than close a distinct proof gap.

## Calibration preferences

- `relying_party.rs` is large, but its production half is one cohesive capability and its tests belong inline under repository rules. Splitting it solely by line count would make the workflow harder to follow; this is not a finding.
- `ProviderHttpError` must remain public because it appears as the associated error type of the public `AsyncHttpClient` implementation. Whether it is also root-reexported is not a material maintenance issue.
- The fixed five-minute, 256-entry provider cache follows the repository's existing `JwksCache` defaults and the approved task's small per-issuer cache. It is not an unearned public option.

## Overall result

**FAIL**

The implementation has a coherent owner and substantially simpler consumers, but the independently configurable human-token audience and two smaller unneeded public/configuration surfaces violate the approved one-source client identity and the standing conventional-mechanism direction.
