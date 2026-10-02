# TASK-009 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa2`
- Candidate: `6578921d8ced6316c850e4d8f16bd101630a7056`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Routed prior finding: `FIND-TASK-004-13`, from `review/TASK-004-r2/lead-direction-routing.md`
- Review scope: complete cumulative base-to-candidate diff. The candidate remained unchanged while this report was prepared.

## Behavior trace

The tenant paths converge on one `HumanConnections`-owned `RelyingParty`: login and candidate-test initiation use library-generated state, nonce, and S256 PKCE; callback state alone recovers the tenant and exact connection; RFC 9207 `iss` is checked before token redemption; and `RelyingParty::redeem` performs the library token request and ID-token verification. Platform login owns a separate `RelyingParty` for its single platform connection. Both use the existing `ScreenedHttp`, whose pinned, proxy-free reqwest client refuses redirects. Unknown `kid` reaches one explicit fresh discovery and one final verification attempt. The workload JWT-bearer request path still calls `ExternalVerifier`.

The cumulative diff also routes two workload trusted-issuer setup consumers through the new human-login `RelyingParty::discover`: the tenant-admin create route and self-hosted boot seeding. Unlike their base implementation, that method fetches the advertised JWKS before returning provider metadata. This changes when workload issuer setup depends on key-set availability and is the finding below.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003 / AC-003: a candidate connection test completes a real sign-in against that exact candidate and marks only that revision tested | `HumanConnections::begin_test`; `AuthorizationCodeExchange::complete` fresh-discovers a test candidate; `finish_id_token_exchange` delegates only the test stamp | Recorded unfiltered `test:identity:journey`; candidate-test journeys and unchanged store assertions | PASS |
| REQ-004: discovery supplies endpoints/JWKS, audience derives from the human client id, and only implemented client authentication methods are accepted | `RelyingParty::discover`; `human_connection_trusted_issuer` sets `expected_audience` from `client_id`; `redeem` maps basic/post/public and refuses `PrivateKeyJwt` | `test:shared`, `test:principals:integration`, identity journeys | PASS |
| REQ-006 / INV-001: route input selects only the pre-login tenant context; callback authority comes from one-use server state | `HumanConnections::begin_login`; `AuthorizationCodeExchange::{execute,complete,bound_connection}` | `login_ignores_request_headers`, callback and identity journeys | PASS |
| REQ-007 / AC-007: authorization code, state, nonce, S256 PKCE, exact redirect, signature, issuer, audience, asymmetric advertised algorithm, key, expiry, issued-at, and claims fail closed | `RelyingParty::{authorize,redeem,verify}` and `id_token_verifier`; callback consumes state before provider IO | `the_authorization_request_carries_state_nonce_and_pkce`, `id_token_refusals_fail_closed`, `an_unadvertised_signing_algorithm_is_refused`, `tenant_callback_refusal_journey` | PASS |
| RFC 9207 response issuer is enforced before any token-endpoint request | `verify_response_issuer` uses cached discovery metadata and runs before `redeem` | `response_issuer_is_bound_exactly_and_required_only_when_advertised`, `tenant_callback_issuer_binding_journey` | PASS |
| INV-004 / AC-007: unsafe issuer/JWKS destinations, redirects, and provider outages fail closed | `ScreenedHttp::client_for` pins screened addresses, disables proxies and redirects; adapter bounds bodies; token 5xx maps to retryable refusal | `an_unsafe_issuer_is_refused_before_any_request`, `an_unsafe_key_set_url_is_refused_before_any_request`, `a_redirecting_token_endpoint_is_refused_without_following`, `a_provider_outage_fails_closed` | PASS |
| INV-004 / AC-007: unknown `kid` forces exactly one rediscovery, then succeeds or refuses | `RelyingParty::redeem` has one `UnknownKey` arm, one `discover`, and one terminal `verify` | `an_unknown_key_rediscovers_exactly_once`, `a_still_unknown_key_fails_after_one_rediscovery` | PASS |
| AC-002 / AC-006 / AC-008: tenant login, connection test, and platform login work against Keycloak and Dex through the library | `RelyingParty` is the human owner used by tenant and platform flows; fixture now reads library metadata | Recorded unfiltered `test:identity:journey` covering server, UI, CLI, Rust client, Python, and TypeScript, plus the recorded focused journeys | PASS |
| REQ-016: replacement/deactivation blocks an in-flight old tenant connection before issuance | `AuthorizationCodeExchange::bound_connection` is checked before redemption and again before durable completion | Existing changed-connection and identity journeys in the recorded `test:identity:journey`/`test:wyrd` lanes | PASS |
| FIND-TASK-004-13: secret-bearing token requests never follow redirects | One `ScreenedHttp` adapter supplies a reqwest client with `Policy::none`; no oauth2 reqwest feature is enabled | Redirect test covers 307 and 308 and asserts zero requests at the second origin; `cargo tree` evidence records no oauth2 reqwest feature | PASS |
| Delete handwritten human discovery, PKCE/state/nonce, token POST, and human `ExternalVerifier`; leave workload JWT bearer on `ExternalVerifier` | `provider.rs` and old helpers are removed; tenant/platform callbacks use `RelyingParty`; `jwt_bearer.rs` still owns `ExternalVerifier` | Source search plus recorded shared/server/journey lanes | PASS |
| Preserve existing operator-visible behavior and keep unrelated workload federation outside this human relying-party replacement | Workload admin create and boot seeding now call `RelyingParty::discover`, which always fetches JWKS | Existing fixtures were changed to add `/jwks`; no proof covers discovery-success/JWKS-outage preservation | FAIL (`BEH-001`) |
| Non-goals: no provider-specific branch, oauth2 reqwest client, SAML/SCIM, BFF/device/sealed-completion redesign, or second human token verifier | Cumulative diff contains none; the device secret helper only replaces a deleted random helper with existing rand/base64 dependencies | Source inspection; boundary, codegen, SDK, CLI, Python, and TypeScript lanes recorded green | PASS |

## Proposed findings

### BEH-001 — REGRESSION: human relying-party discovery now gates workload issuer setup on JWKS availability

- **Classification:** REGRESSION
- **Violated obligation:** TASK-009 promises no tenant/operator-visible behavior change, limits the replacement to the human relying-party paths, and explicitly leaves workload RFC 7523 verification on `ExternalVerifier`.
- **Exact locations:** `crates/wyrd/wyrd-server/src/components/admin/routes.rs:750-771`; `crates/wyrd/wyrd-server/src/boot/issuer.rs:131-179` and `220-252`; shared cause at `crates/shared/wyrd-auth-oidc/src/relying_party.rs:350-370`.
- **Evidence:** At the base, workload trusted-issuer creation and boot seeding fetched and decoded only the standard discovery document, saved its `jwks_uri`, and left key retrieval to the existing workload verifier. At the candidate, both consumers call `RelyingParty::discover`; its documented and actual library call fetches the discovery document and then the JWKS. The changed workload fixtures had to add `/jwks` responses, confirming the new dependency rather than preserving the former boundary.
- **Observable consequence:** A conforming workload issuer whose discovery endpoint is available while its JWKS endpoint is temporarily unavailable can no longer be created through the tenant-admin API or seeded on first boot. The same setup succeeded at the base and would defer the transient key failure to the first workload assertion, where `ExternalVerifier` owns the existing refresh/retry behavior. This is unrelated to tenant, connection-test, or platform human sign-in.
- **Required testable correction:** Keep the `openidconnect` relying-party owner and its metadata-plus-JWKS cache on human login only. Restore the workload trusted-issuer setup boundary so it obtains and validates the standard discovery metadata and persists the advertised `jwks_uri` without fetching keys; continue to use the existing screened transport, and leave the existing `ExternalVerifier`/`JwksCache` as the sole workload key-fetch owner. Reuse an installed typed standards mechanism; do not add a second cache, readiness probe, setting, option, or provider-specific branch. Add one focused regression proof in the existing workload admin/boot tests: discovery succeeds, `/jwks` is unavailable, and trusted-issuer setup still reaches its prior result; retain the existing workload-verification refusal for unavailable keys.

This is not classified as `DRIFT`: fetching JWKS during full OIDC relying-party discovery is conventional and is the library's native behavior. The defect is applying that human-login boundary to unchanged workload setup consumers.

## Verification assessment

The recorded behavior evidence is broad and credible for the human relying-party change: the unfiltered identity journey, shared/server/SDK/CLI/integration lanes, language-runtime lanes, codegen/docs, and boundary checks all passed. The named relying-party negative tests directly exercise the new trust boundary; the Keycloak/Dex journeys exercise the real server path.

After `9ef532660`, rerunning `mise run lints` and `mise run check:workspace-hack` was sufficient for that commit's mechanical Hakari regeneration. The diff only updates the generated workspace feature union and corresponding lock edges; `check:workspace-hack` proves the generated manifest is exact, while all-feature lints compile the resulting union. The behavior lanes had already exercised the actual `openidconnect` dependency and crypto implementations introduced by the functional commits. No runtime source, test, route, schema, or fixture changed in `9ef532660`, so repeating every journey would not add a distinct behavior proof.

That sufficiency does not close `BEH-001`: the existing changed fixtures make JWKS available and therefore do not test the workload setup behavior that regressed.

## Overall result

**FAIL**

The human tenant, candidate-test, and platform paths satisfy the task, including the routed redirect finding and the required OIDC negative cases. The cumulative candidate nevertheless changes an out-of-scope workload setup behavior through the shared discovery replacement, so acceptance requires the bounded correction in `BEH-001`.
