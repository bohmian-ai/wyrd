# TASK-009 structured Ponytail validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `6578921d8ced6316c850e4d8f16bd101630a7056`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Routed authority: `changes/active/oidc-production-readiness/review/TASK-004-r2/lead-direction-routing.md`, including `FIND-TASK-004-13`

The candidate remained unchanged during validation. I read the complete cumulative diff, every discovery and follow-up report, the applicable repository and task authorities, the base implementations replaced by the candidate, the installed `openidconnect 4.0.1` and `oauth2 5.0.0` behavior needed to resolve disputed claims, and the complete producer/caller/consumer paths cited below. No reviewed source was modified.

The standing direction is applied as an acceptance rule: Wyrd uses the standard or conventional mechanism and keeps no Wyrd-only mechanism, check, file, setting, or option without such authority. None of the retained corrections adds one.

## Proposal validation

| Discovery proposal | Result | Source validation and disposition |
|---|---|---|
| `BEH-001` | **CONFIRMED** | Both workload setup consumers changed from metadata-only discovery at the base to `RelyingParty::discover`, whose library call necessarily fetches JWKS. Retained as `FIND-TASK-009-3`. |
| `INV-REV-001` | **REVISED** | The missing platform RFC 9207 input/check is reachable and task-required. Its missing completion proof is the same correction boundary as `OIDC-SEC-003`, not a second finding. Retained as `FIND-TASK-009-2`. |
| `REPO-TASK-009-1` | **REVISED** | Per-request construction defeats caching on the platform begin/callback flow. Administrative issuer creation is intentionally one-shot and does not need a long-lived cache. The platform-only defect is retained with `SYS-001` as `FIND-TASK-009-1`. |
| `REPO-TASK-009-2` | **CONFIRMED** | The inserted JWKS helper captured `not_tested_reason`'s rustdoc and left that helper undocumented. Retained as `FIND-TASK-009-9`. |
| `REPO-TASK-009-3` | **CONFIRMED** | The task explicitly requires exact selectors for named tests; the durable evidence records only aggregate lanes. Retained as `FIND-TASK-009-10`. |
| `MNT-001` | **REVISED** | The shared `CodeRedemption` producer permits `client_id` and ID-token audience to disagree, and the platform public/store contract supplies the duplicate value. Deduplicated with `OIDC-SEC-001` as `FIND-TASK-009-6`. |
| `MNT-002` | **CONFIRMED** | `RelyingParty::http` has no repository caller and exposes no required workflow. Retained as `FIND-TASK-009-7`. |
| `MNT-003` | **CONFIRMED** | The candidate overrides the library's conventional state/nonce constructors with a Wyrd-owned length absent from the task or standards. Retained as `FIND-TASK-009-8`. |
| `SYS-001` | **REVISED** | Confirmed and deduplicated with the platform portion of `REPO-TASK-009-1` as `FIND-TASK-009-1`. |
| `SYS-002` | **REJECTED** | `9ef532660` changes only Hakari's generated feature-union manifest and matching lock edges, not versions or runtime source. `mise run lints` compiles the final all-feature/all-target graph and shipped server feature graph; `check:workspace-hack` proves the generated union. Re-running runtime lanes solely for this mechanical commit is not required. |
| `OIDC-SEC-001` | **REVISED** | Confirmed and deduplicated with `MNT-001` as `FIND-TASK-009-6`. |
| `OIDC-SEC-002` | **CONFIRMED** | `oauth2::StandardErrorResponse::Display` includes provider-controlled description and URI text; the candidate retains that text in `TokenRejected` and logs it. Retained as `FIND-TASK-009-5`. |
| `OIDC-SEC-003` | **REVISED** | The helper used by the claimed platform journey bypasses the changed callback. Consolidated with the missing platform RFC 9207 boundary as `FIND-TASK-009-2`; the real callback journey is its closure proof. |
| `CACHE-1` | **REVISED** | The base Moka cache coalesced overlapping misses with `try_get_with`; the candidate explicitly does not. Retained only for process-local overlapping miss/refresh coalescing. No cache-generation protocol, all-interleaving guarantee, or cross-replica coordination is required. Retained as `FIND-TASK-009-4`. |
| Cache-domain post-Hakari runtime-rerun limit | **REJECTED** | Same source-based resolution as `SYS-002`. It is not a verification limit and does not weaken the separate focused proof required to close `FIND-TASK-009-4`. |

The standards report's statement that provider causes are safe to log is contradicted by the actual `oauth2` display implementation and the candidate's log sink; `OIDC-SEC-002` therefore controls. The cache-domain statement that platform begin and callback share one `PlatformLogin` is true only for calls on one value, not for the shipped handlers, which construct separate values; `SYS-001` therefore controls.

## Final finding ledger

### FIND-TASK-009-1 — REVISED — INCORRECT: platform requests discard the relying-party cache

- **Discovery sources:** `SYS-001`, platform portion of `REPO-TASK-009-1`.
- **Violated obligation:** TASK-009 requires platform login to use the per-issuer metadata/JWKS cache, and Wyrd's composition rules require dependency-backed state to be process-owned rather than rebuilt by handlers.
- **Exact location:** `crates/wyrd/wyrd-server/src/components/platform/identity.rs:652-669,698-704,732-748`; `crates/wyrd/wyrd-auth/src/platform_login.rs:107-123,153-164,239-275`.
- **Evidence and reachability:** Both shipped platform handlers call `login_service`. Each call constructs a new `PlatformLogin`; its constructor constructs a new `RelyingParty` and empty Moka cache. The begin request's discovered metadata is therefore dropped before callback, and every concurrent request starts cold. `ServerAuth`/`AppState` already provide the conventional process-owned handle registry, and tenant `HumanConnections` demonstrates the required lifecycle. The admin create path is not part of this finding because it intentionally performs one fresh discovery for one write.
- **Observable consequence:** A platform begin that succeeds can fail after its state is persisted if discovery/JWKS becomes unavailable before callback, even though the process fetched still-valid metadata moments earlier. Normal callbacks also repeat discovery/JWKS IO and amplify provider degradation. The global platform credential and tenant login remain available.
- **Decision-complete correction:** Compose one existing `PlatformLogin` (and therefore its one `RelyingParty`) in the process-owned server auth state and reuse clones/references from both platform handlers. Preserve the separate tenant/platform caches, existing screened transport, five-minute Moka policy, one-redemption unknown-key retry, and global-credential fallback. Do not add another cache, setting, retry, health check, distributed lock, or cross-replica coordination.
- **Focused closure proof:** Through the served platform routes, begin login, make discovery unavailable, then complete callback with the already-known key and prove session issuance succeeds without another discovery/JWKS fetch. Separately prove an unknown key still makes at most one forced rediscovery and fails closed when that refresh is unavailable.

### FIND-TASK-009-2 — REVISED — MISSING: platform callback omits RFC 9207 binding and bypasses real completion proof

- **Discovery sources:** `INV-REV-001`, `OIDC-SEC-003`.
- **Violated obligation:** TASK-009 applies the vetted relying party and retained RFC 9207 `iss` check to tenant login, connection testing, and platform-administrator login; Scenario 2 and the acceptance criteria require missing/wrong advertised response issuers to fail closed and the platform flow to work through the changed callback.
- **Exact location:** `crates/wyrd-spec/src/auth/platform_identity.rs:113-122`; `crates/wyrd/wyrd-server/src/components/platform/identity.rs:732-748`; `crates/wyrd/wyrd-auth/src/platform_login.rs:239-275`; proof bypass at `crates/wyrd/wyrd-testing/src/server.rs:2685-2704` and `crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:1491-1503`.
- **Evidence and reachability:** `PlatformCallbackRequest` accepts only code/state and denies unknown fields, the handler cannot forward `iss`, and `PlatformLogin::complete` redeems without the existing `verify_response_issuer` call. The sibling tenant/candidate path checks optional `iss` against the state-bound issuer and advertised provider flag before token IO. The platform E2E uses `federated_platform_session`, whose rustdoc says it stands in for the provider round trip; no test calls the shipped callback or `PlatformLogin::complete`.
- **Observable consequence:** When metadata advertises RFC 9207 support, platform login accepts an authorization response with no `iss` and cannot accept/check a normal response carrying it; a mismatched issuer is not refused before the secret-bearing token request. Regressions in code exchange, nonce/audience verification, verified-email pinning, error mapping, or session issuance can pass the claimed journey.
- **Decision-complete correction:** Add the standard optional response `iss` to the existing platform callback wire contract, forward it to the existing `PlatformLogin` owner, and invoke the existing `verify_response_issuer` against the consumed state's issuer and cached metadata before `RelyingParty::redeem`. Use the existing platform/identity journey infrastructure; add no new check type, setting, protocol, or harness.
- **Focused closure proof:** Drive begin, provider authorization/token exchange, and `/auth/platform/callback` through a real server. Prove a matching advertised `iss` yields a session that has platform authority and no tenant authority; prove advertised-missing and mismatched `iss` are refused before the mock token endpoint receives a request.

### FIND-TASK-009-3 — CONFIRMED — REGRESSION: workload issuer setup now requires JWKS availability

- **Discovery source:** `BEH-001`.
- **Violated obligation:** TASK-009 limits the relying-party replacement/cache to human login, keeps workload RFC 7523 verification on `ExternalVerifier`, and promises unchanged tenant/operator-visible behavior.
- **Exact location:** `crates/wyrd/wyrd-server/src/components/admin/routes.rs:735-771`; `crates/wyrd/wyrd-server/src/boot/issuer.rs:123-149,227-253`; shared cause at `crates/shared/wyrd-auth-oidc/src/relying_party.rs:348-375`.
- **Evidence and reachability:** At the base, both workload consumers fetched and typed the discovery document only, persisted its advertised `jwks_uri`, and left keys to the existing workload `JwksCache`. The candidate routes both through `ProviderMetadata::discover_async`, whose installed-library implementation always fetches the discovery document and then JWKS. Changed fixtures had to add `/jwks`. These are production admin-create and boot-seeding callers, not dormant/test-only paths.
- **Observable consequence:** A conforming workload issuer with available discovery but temporarily unavailable JWKS can no longer be registered or first seeded, although the base operation succeeded and the first workload assertion already owned the key-fetch failure.
- **Decision-complete correction:** Keep full `openidconnect` discovery plus metadata/JWKS caching on human relying parties. For the two workload setup consumers, perform one screened, redirect-free, bounded read of the standard discovery document into the installed library's typed provider-metadata representation and persist its `jwks_uri` without fetching keys. Leave workload key retrieval and refresh solely with the existing `ExternalVerifier`/`JwksCache`. Do not restore the deleted hand-written metadata type, add a second cache/readiness probe/option, or use a provider branch.
- **Focused closure proof:** In the existing workload admin and boot tests, serve valid discovery while `/jwks` is unavailable and prove setup reaches its prior result; retain the workload assertion test proving unavailable keys fail closed when verification actually needs them.

### FIND-TASK-009-4 — REVISED — REGRESSION: the replacement cache drops Moka miss coalescing

- **Discovery source:** `CACHE-1`.
- **Violated obligation:** TASK-009 retains the existing Moka-backed per-issuer metadata/JWKS cache, unchanged behavior, and one bounded unknown-key rediscovery.
- **Exact location:** `crates/shared/wyrd-auth-oidc/src/relying_party.rs:331-375,477-485`; base owner at `35a53faa2:crates/shared/wyrd-auth-oidc/src/jwks.rs:232-317`.
- **Evidence and reachability:** The base used `Cache::try_get_with` so overlapping same-key misses shared a fetch; unknown-key handling invalidated and re-entered that native coalesced path. Candidate `cached` does `get` then unconditional `discover`, and its rustdoc explicitly says concurrent misses each discover. Candidate unknown-key handling also calls unconditional `discover`. The production tenant owner is cloned across requests, and the platform owner will be process-shared under `FIND-TASK-009-1`, so overlapping cold/expired and rotation misses are reachable. The base did not guarantee one network request for every possible late-invalidation interleaving, so no stronger generation protocol is justified.
- **Observable consequence:** A burst after cold start/expiry makes discovery and JWKS traffic proportional to callbacks rather than one overlapping process-local fetch; a common stale-key burst can do the same during rotation and amplify an IdP slowdown/outage. Verification still fails closed.
- **Decision-complete correction:** Route cache misses through Moka's existing `try_get_with` single-flight mechanism. On the one permitted unknown-key retry, invalidate and re-enter that same cached discovery path; every redemption still receives at most one retry. Keep caches process-local and tenant/platform-separated. Do not add generation state, a custom lock, distributed coordination, a public option, or an all-interleaving/deployment-wide one-request promise.
- **Focused closure proof:** Concurrently call the cache for one cold/expired issuer and assert overlapping callers produce one discovery/JWKS fetch. Prime an old key, concurrently redeem rotated-key tokens through clones of one owner, and prove overlapping refreshes coalesce through Moka while each redemption retries at most once and a still-unknown key fails closed.

### FIND-TASK-009-5 — CONFIRMED — VIOLATION: provider-controlled OAuth error text reaches logs

- **Discovery source:** `OIDC-SEC-002`.
- **Violated obligation:** Specification REQ-005 and the security posture require provider secrets to be absent from logs, traces, and errors.
- **Exact location:** `crates/shared/wyrd-auth-oidc/src/relying_party.rs:614-626,640-649`; log sink at `crates/wyrd/wyrd-auth/src/error.rs:69-73`.
- **Evidence and reachability:** For `RequestTokenError::ServerResponse`, `oauth2 5.0.0` formats the provider-controlled `error_description` and `error_uri`. Candidate `token_error` stores the whole error chain in `TokenRejected`; both tenant and platform mappings reach `relying_party_error`, which logs the full value. A token endpoint sees client authentication and the PKCE verifier and can reflect either into that standard response text.
- **Observable consequence:** A malicious or compromised provider can copy a client secret or verifier into durable Wyrd logs and downstream log systems. The body cap limits size, not disclosure.
- **Decision-complete correction:** Match the library token-error variants at the relying-party boundary. For a standards server response, retain at most the closed `CoreErrorResponseType`; for parse/other refusals, use fixed local classifications. Discard provider descriptions, URIs, response bytes, and source strings before constructing any loggable error. Keep the existing public stable refusal. Do not add a redaction framework, allowlist, setting, or second error taxonomy.
- **Focused closure proof:** Return an OAuth error response whose description and URI contain a canary/client-secret value; capture the existing tracing sink and public response and assert the canary appears in neither while the standard refusal kind remains diagnosable.

### FIND-TASK-009-6 — REVISED — DRIFT: human ID-token audience remains independently configurable

- **Discovery sources:** `MNT-001`, `OIDC-SEC-001`.
- **Violated obligation:** Specification REQ-004 says the human ID-token audience is derived from client ID rather than entered twice; OIDC Core validates this relying party's client identifier. The standing direction rejects an extra Wyrd-only option.
- **Exact location:** `crates/shared/wyrd-auth-oidc/src/relying_party.rs:263-279,507-520,563-570`; platform option at `crates/wyrd-spec/src/auth/platform_identity.rs:41-57,66-81`; platform consumer at `crates/wyrd/wyrd-auth/src/platform_login.rs:264-272`. The tenant resolver already derives its human `expected_audience` from `client_id` at `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:563`.
- **Evidence and reachability:** `CodeRedemption` accepts both values, builds the token client with `client_id`, and builds the verifier with `audience`. Platform configure/view/store exposes both values and sends them independently. Tenant and platform are the only production `CodeRedemption` callers.
- **Observable consequence:** Ordinary platform configuration can fail solely because duplicate values differ, and the shared verifier can accept a token addressed to another client at the same issuer rather than the client that redeemed the code.
- **Decision-complete correction:** Delete `CodeRedemption::audience` and construct `IdTokenVerifier` from `client_id`. Remove the independent platform human `expected_audience` request/view/persistence setting rather than adding equality validation, a toggle, fallback, or compatibility alias. Preserve workload `expected_audience`, which is a separate RFC 7523 contract.
- **Focused closure proof:** Prove the human relying party accepts `aud = client_id`, rejects every other audience, and exposes no independent platform human audience input. Run contract/codegen checks required by the public request/view change and the real platform journey from `FIND-TASK-009-2`.

### FIND-TASK-009-7 — CONFIRMED — DRIFT: unused `RelyingParty::http` widens the owner

- **Discovery source:** `MNT-002`.
- **Violated obligation:** The standing direction and maintainer rules allow owner methods for required workflows, not unused lower-level escape hatches.
- **Exact location:** `crates/shared/wyrd-auth-oidc/src/relying_party.rs:325-329`.
- **Evidence and reachability:** Repository-wide caller search finds no call. `discover`, `authorize`, and `redeem` already encapsulate all human provider traffic; workload verification owns its separate transport.
- **Observable consequence:** The public shared-crate API advertises a second, lower-level path around the owner without a current consumer or invariant.
- **Decision-complete correction:** Delete the accessor and its rustdoc. Add nothing.
- **Focused closure proof:** Compile the owning crate/workspace and confirm no accessor or call remains.

### FIND-TASK-009-8 — CONFIRMED — DRIFT: state and nonce override the library default

- **Discovery source:** `MNT-003`.
- **Violated obligation:** TASK-009 selects library-generated `CsrfToken` and `Nonce`; the standing direction requires the conventional library mechanism and rejects an unrequired Wyrd security setting.
- **Exact location:** `crates/shared/wyrd-auth-oidc/src/relying_party.rs:64-65,399-406`.
- **Evidence and reachability:** Candidate owns `RANDOM_VALUE_BYTES = 32` and calls `new_random_len`. `openidconnect/oauth2` provide and document `CsrfToken::new_random` and `Nonce::new_random` with their conventional 128-bit defaults; no approved requirement or provider needs a Wyrd-specific length. Every human authorization request uses this path.
- **Observable consequence:** Wyrd unnecessarily owns a protocol-adjacent tuning value and its future justification while gaining no required behavior.
- **Decision-complete correction:** Delete `RANDOM_VALUE_BYTES` and pass the library's `CsrfToken::new_random` and `Nonce::new_random` constructors to `authorize_url`. Keep the library's S256 PKCE generator unchanged.
- **Focused closure proof:** The existing authorization-request test continues to prove nonempty state/nonce and S256 PKCE, and source contains no Wyrd-owned state/nonce length.

### FIND-TASK-009-9 — CONFIRMED — VIOLATION: changed private helpers have inaccurate/missing rustdoc

- **Discovery source:** `REPO-TASK-009-2`.
- **Violated obligation:** `AGENTS.md` §16 and `architecture/agent-rules.md` require accurate rustdoc on every new or materially modified Rust item, including private helpers.
- **Exact location:** `crates/wyrd/wyrd-auth/src/connections.rs:936-954`.
- **Evidence and reachability:** The pre-existing sentence describing a stable failed-test reason now precedes the newly inserted `require_usable_jwks`, where it is inaccurate; `not_tested_reason` begins without rustdoc.
- **Observable consequence:** The JWKS validator is documented as an error constructor, while the actual stable-reason constructor's workflow contract is undocumented.
- **Decision-complete correction:** Leave the JWKS validation and error construction unchanged; attach only the JWKS-specific documentation to `require_usable_jwks` and restore the stable machine-reason documentation to `not_tested_reason`. Add no helper or documentation check.
- **Focused closure proof:** Source inspection plus the existing format/lint/docs lane shows both private helpers have accurate, adjacent rustdoc.

### FIND-TASK-009-10 — CONFIRMED — VIOLATION: named tests lack mandatory exact-selector evidence

- **Discovery source:** `REPO-TASK-009-3`.
- **Violated obligation:** TASK-009 lines 162-168 and repository testing rules require every specifically named test to be selected and run with the exact repository-native command; aggregate lanes do not replace this zero-selection-safe evidence.
- **Exact location:** `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md:208-245`.
- **Evidence and reachability:** The evidence names the relying-party tests and `tenant_callback_refusal_journey` / `tenant_callback_issuer_binding_journey`, but records only aggregate `mise run` lanes. It records neither exact nextest expressions nor the required identity test listing and `WYRD_IDENTITY_FILTER` commands.
- **Observable consequence:** The durable task record does not establish that each named negative/edge test was actually selected; an aggregate command can be green without proving the named selector contract.
- **Decision-complete correction:** Run every specifically named test at task lines 211-216 with its exact `mise exec -- cargo nextest ... -E 'test(=...)'` form, or for identity journeys first list the ignored target and then use the prescribed `WYRD_IDENTITY_TARGET=server WYRD_IDENTITY_FILTER=<exact-name>` wrapper. Record commands, selected test identity, candidate commit, and exit status in the existing implementation evidence. Retain the aggregate lane evidence; add no harness or permanent check.
- **Focused closure proof:** The durable task evidence contains a successful exact selector/listing record for every named test and cannot pass through zero selection.

## Verification reconciliation

The broad recorded lanes remain useful and credible for the behavior they actually execute. The isolated `9ef532660` regeneration changed only generated workspace-hack feature-union entries and matching lockfile edges. No dependency version, Rust source, test, route, schema, or fixture changed. The post-regeneration `mise run lints` compiled the final all-feature/all-target workspace and shipped server graph, and `mise run check:workspace-hack` proved the generated union. That is sufficient for this mechanical delta; no post-regeneration runtime rerun is required solely because of it.

That conclusion does not cure the missing exact-selector record (`FIND-TASK-009-10`) or the absent focused behavior proofs attached to the implementation findings. Those proofs belong on the remediated final candidate.

## Validation result

- **Validated ledger:** 10 findings (`FIND-TASK-009-1` through `FIND-TASK-009-10`).
- **`SPEC_REVISION_REQUIRED`:** No. Every correction follows the approved specification, named standards, an existing repository owner, or the selected library's conventional mechanism.
- **`BLOCKED`:** No. The immutable source, complete caller traces, authorities, reports, and verification evidence were available, and every material conflict was resolved from them.

The candidate requires bounded remediation. No retained finding asks for a novel mechanism, setting, dependency, cache, coordination service, compatibility path, test harness, or permanent check.
