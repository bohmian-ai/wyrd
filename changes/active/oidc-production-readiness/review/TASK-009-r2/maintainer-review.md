# TASK-009 R2 maintainer review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `1ddc10e21054ddc158f461e8c6d8aa862c32a067`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-009-r1/TASK-009-R1-relying-party-corrections.md`
- Human direction: `changes/active/oidc-production-readiness/review/TASK-009-r1/lead-direction-FIND-TASK-009-5.md`; `FIND-TASK-009-5` is withdrawn and was not reopened.

The candidate remained at the requested commit throughout this review. I read
the complete cumulative base-to-candidate diff and used the remediation diff to
locate the corrected owners without limiting the audit to that diff.

## Changed-surface coverage

| Surface | Symbols and paths inspected | Caller, test, and declaration coverage | Assessment |
|---|---|---|---|
| Shared relying party and screened provider transport | `wyrd-auth-oidc/src/relying_party.rs`: provider metadata aliases, `ScreenedHttp::{send,provider_metadata}`, `RelyingPartyError`, `Authorization`, `CodeRedemption`, `VerifiedIdToken`, `RelyingParty::{new,cached,discover,fetch,authorize,redeem,verify}`, verifier and error helpers; `screening.rs`; crate exports and manifests | Traced tenant login, candidate testing, platform login, workload admin/boot discovery, cache refresh, and all inline tests | Cohesive concrete owner. Human protocol mechanics stay on `RelyingParty`; the metadata-only workload read reuses the screened transport and installed typed metadata without restoring the deleted handwritten model. Cache coalescing, library-default state/nonce generation, and client-ID-derived audience are clear at their owner. |
| Tenant login, connection testing, and callback | `wyrd-auth/src/{login,callback,connections,error,pg_resolvers}.rs` | Traced begin through state persistence, callback consumption, RFC 9207 check, code redemption, candidate-test completion, user/session issuance, and server callback adapter/tests | Production flow is discoverable and no longer depends on the workload verifier. The callback test fixture still carries the removed dependency; see `MNT-R2-001`. |
| Platform login and process ownership | `wyrd-auth/src/platform_login.rs`; `wyrd-server/src/boot/mod.rs`; `components/auth/state.rs`; `components/platform/identity.rs`; `wyrd-testing/src/server.rs` | Traced production and test-server construction, `ServerAuth` storage, both served handlers, connection reload, cached provider use, RFC 9207 validation, session issuance, and route-level journey | `PlatformLogin` is the correct stateful owner and is built once per process. Handler lookup returns a reference instead of rebuilding dependencies. Public names and method order make the workflow findable. One adjacent owner description is stale; see `MNT-R2-001`. |
| Workload issuer setup | `wyrd-server/src/boot/issuer.rs`; `components/admin/routes.rs`; `wyrd-auth-verify` workload verifier/JWKS path | Traced metadata-only discovery to persisted `jwks_uri`, then the separate assertion-time key fetch; read boot/admin focused tests | Clear separation: setup reads metadata, while the existing workload verifier owns key retrieval. No second cache, option, readiness probe, or provider branch was added. |
| Platform wire and persistence contract | `wyrd-spec/src/auth/platform_identity.rs`; `wyrd-sql/src/queries/platform/identity.rs`; migration `20261002000001_platform_oidc_client_audience.sql`; platform route projections and SQL tests | Traced configure/read/callback request shapes, row decoding, upsert/read call sites, migration, OpenAPI/codegen evidence, and public route journey | The duplicate human audience is removed through every layer; `client_id` is now the single named source. The optional `iss` field is documented consistently with RFC 9207. Workload `expected_audience` remains separate. |
| Tests and fixtures | `wyrd-auth-oidc` inline tests; `wyrd-auth` tests; `wyrd-server` identity and platform journeys; `wyrd-testing` OIDC/server fixtures; SQL tests | Read cache coalescing/rotation tests, workload setup with unavailable JWKS, real platform begin/callback journey, negative issuer/audience paths, generated-contract assertions, and exact-selector evidence | Tests generally assert caller-visible outcomes through the owning surface. The platform journey legitimately groups one continuous configured-provider flow. The callback unit fixture retains dead setup from the deleted human verifier path; see `MNT-R2-001`. |
| Dependency and generated parity | workspace/crate manifests, lockfile, workspace-hack, Rust wire types and recorded codegen checks | Confirmed `openidconnect 4.0.1` remains narrowly enabled without the `oauth2` `reqwest` feature; inspected contract source and final verification record | No new dependency, feature, compatibility surface, or handwritten generated artifact was introduced in remediation. Recorded `codegen:check`, N-API, Python, and TypeScript checks are green. |

## Prior-finding closure

| Prior maintainer finding | Source evidence | Result |
|---|---|---|
| `MNT-001` / `FIND-TASK-009-6`: duplicate human audience | `CodeRedemption` has only `client_id`; `id_token_verifier` uses it; platform request/view/row/migration no longer carry `expected_audience` | **CLOSED** |
| `MNT-002` / `FIND-TASK-009-7`: unused `RelyingParty::http` | Accessor is absent and no caller remains | **CLOSED** |
| `MNT-003` / `FIND-TASK-009-8`: Wyrd-owned state/nonce length | `authorize` uses `CsrfToken::new_random` and `Nonce::new_random`; the length constant is absent | **CLOSED** |
| `FIND-TASK-009-9`: helper rustdoc | `require_usable_jwks` and `not_tested_reason` each have accurate adjacent documentation | **CLOSED** |

## Material findings

### MNT-R2-001 — removed human-verifier ownership remains in docs and callback test setup

- **Classification:** `DRIFT`
- **Changed/affected locations:** `crates/wyrd/wyrd-server/src/components/auth/state.rs:22-24`; `crates/wyrd/wyrd-server/src/boot/auth.rs:43-56`; `crates/wyrd/wyrd-server/src/auth/callback.rs:1231-1282`
- **Governing rule:** the maintainer guide requires documentation to describe the actual typed contract and tests to expose the dependencies the behavior really needs. TASK-009 requires deletion of the human `ExternalVerifier` path, while standing direction rejects a mechanism that the standard flow and comparable relying parties do not need.
- **Evidence:** production callback construction now supplies only `TenantTokenIssuer` and `HumanConnections`, and repository-wide production access to `ServerAuth::external_verifier` is the RFC 7523 `jwt_bearer` path. Nevertheless, both `ServerAuth::external_verifier` and `AuthHandles::external_verifier` still say they verify human ID tokens for login callback/platform login. The materially changed callback test module still constructs `PgIssuerResolver`, `JwksCache`, and `ExternalVerifier`, stores them in `ServerAuth`, and calls the fixture `test_state_with_external`, although neither `exchange_authorization_code` nor `AuthorizationCodeExchange` reads those fields anymore.
- **Concrete maintenance cost:** the owner documentation sends a maintainer to the workload verifier when changing human login, contradicting the new `RelyingParty` boundary. The callback tests also advertise and initialize a dependency they do not exercise, obscuring the minimum test setup and allowing future edits to preserve or repair dead wiring under the mistaken belief that human callback correctness depends on it.
- **Smallest testable correction:** describe `external_verifier` as the workload/JWT-bearer verifier only. In the callback test fixture, remove only the unused `PgIssuerResolver`/`JwksCache`/`ExternalVerifier` construction and `ServerAuth` fields, rename the helper to describe the issuing-key plus human-connections state it actually builds, and delete imports/constants that then have no caller. Do not change the production workload verifier or add a replacement abstraction, option, or check.
- **Closure proof:** compile/run the existing callback tests and verify repository search shows no human callback or platform-login documentation/setup claiming an `ExternalVerifier` dependency; the production `jwt_bearer` consumer remains unchanged.

## Calibration preferences

- `relying_party.rs` is large, but its production portion is one cohesive capability and repository rules prefer its unit tests inline. Splitting it solely by line count would make the flow less discoverable; this is not a finding.
- The adjacent `impl ScreenedHttp` blocks could be combined, but both methods share the same transport owner and the split imposes no material navigation or safety cost.
- The 500 ms delay in the cache-coalescing mock is bounded and directly creates overlap without adding a custom synchronization harness. No replacement mechanism is warranted without observed flakiness.
- The long platform journey is one ordered configured-provider workflow and directly proves several route seams that share setup. Splitting it would duplicate setup without making failures materially easier to locate.

## Verification assessment

The remediation record contains exact non-zero-selection commands for every
named TASK-009 test and successful focused proofs for cache coalescing,
metadata-only workload discovery, client-ID audience binding, RFC 9207, and
the real served platform callback. It also records successful final format,
lint, codegen, docs, boundary, crate, language, and user-journey lanes on the
code commit `0b516e235`; the candidate adds only the evidence commit afterward.
No additional verification mechanism is required for this maintainer finding;
the existing callback test target is the focused closure proof.

## Overall result

**FAIL**

The implementation and remediation owners are otherwise clear and conventional,
but the removed human `ExternalVerifier` path still appears as an active owner
in production documentation and test construction. That bounded residual drift
should be deleted rather than carried forward.
