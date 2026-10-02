---
id: TASK-009-R1
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 11
parent_task: TASK-009
remediates:
  - FIND-TASK-009-1
  - FIND-TASK-009-2
  - FIND-TASK-009-3
  - FIND-TASK-009-4
  - FIND-TASK-009-5
  - FIND-TASK-009-6
  - FIND-TASK-009-7
  - FIND-TASK-009-8
  - FIND-TASK-009-9
  - FIND-TASK-009-10
---

# Close TASK-009 relying-party findings

## Authority and immutable inputs

- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Review base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Reviewed candidate: `6578921d8ced6316c850e4d8f16bd101630a7056`
- Validated ledger: `changes/active/oidc-production-readiness/review/TASK-009-r1/findings-validation.md`
- Routed prior direction: `changes/active/oidc-production-readiness/review/TASK-004-r2/lead-direction-routing.md`

Implement with `$wyrd-implement`. Reassess the complete original base-to-remediated-candidate range afterward; this task does not replace TASK-009.

## Intended outcome

Tenant login, connection testing, and platform login use one conventional `openidconnect` relying-party path through the screened transport, with long-lived process-local caches, standard RFC 9207 handling, client-ID-derived human audience, bounded and non-sensitive errors, and real route-level proof. Workload RFC 7523 setup retains its prior metadata-only behavior and leaves key retrieval with its existing verifier. The implementation contains no unneeded Wyrd-specific option or escape hatch, and the final evidence records every named test's exact selection.

## Issue diagnoses and required corrections

### Platform owner lifetime — `FIND-TASK-009-1`

The served platform begin and callback handlers independently construct `PlatformLogin`, which creates a fresh `RelyingParty` and empty cache. A provider outage after begin can therefore break callback despite metadata and keys having just been fetched, and steady traffic repeatedly discovers the same issuer.

Compose one `PlatformLogin` through the existing process-owned server authentication state and reuse it for both served handlers. Preserve separate tenant/platform caches, the existing five-minute policy, screened transport, single-redemption unknown-key retry, and global administrative credential fallback. Do not add another cache, setting, retry, health check, distributed lock, or cross-replica coordination.

### Platform RFC 9207 and real callback proof — `FIND-TASK-009-2`

The platform callback request carries only code and state, so `PlatformLogin::complete` cannot apply the retained response-issuer binding. The claimed journey creates a federated session through a test helper and never exercises the shipped callback.

Add standard optional response `iss` to the existing platform callback contract, pass it to the existing platform owner, and reuse the existing RFC 9207 verification before any token request. Drive the existing real-server platform journey through begin and `/auth/platform/callback`; prove matching advertised `iss` succeeds with platform-only authority and advertised-missing or mismatched `iss` is refused before the token endpoint receives a request. Do not introduce a new check type, setting, protocol, or test harness.

### Workload setup eagerly fetches keys — `FIND-TASK-009-3`

Workload trusted-issuer administration and boot seeding previously fetched and typed only the discovery document, persisted its `jwks_uri`, and let `ExternalVerifier` retrieve keys when an assertion required them. Reusing full human `RelyingParty::discover` now makes setup depend on immediate JWKS availability and changes behavior outside TASK-009's human scope.

For those two workload setup consumers, perform one screened, redirect-free, bounded read of the standard discovery document into the installed library's typed metadata representation and persist its advertised `jwks_uri` without fetching keys. Keep full `openidconnect` discovery and caching for human login, and leave workload key retrieval/refresh solely with the existing workload `ExternalVerifier`/`JwksCache`. Do not restore the deleted hand-written metadata model, add a second cache or readiness probe, expose an option, or branch by provider.

### Cache miss coalescing — `FIND-TASK-009-4`

The prior Moka owner coalesced overlapping same-key misses. The replacement performs a `get` followed by unconditional discovery and also discovers unconditionally on an unknown key, so bursts multiply provider requests.

Reuse Moka's installed single-flight miss mechanism for cold and expired issuer entries. The one allowed unknown-key retry must invalidate and re-enter that same cached path; each redemption still retries at most once. Keep this process-local and tenant/platform-separated. Do not add generation state, custom locking, distributed coordination, a public option, or a stronger all-interleaving guarantee.

Prove overlapping cold/expired requests for one issuer share one discovery/JWKS fetch. Prove concurrent rotated-key redemptions through clones of one owner coalesce overlapping refresh work, each redemption retries no more than once, and a still-unknown key fails closed.

### Provider-controlled error text — `FIND-TASK-009-5`

The OAuth library's server-response display includes provider-controlled description and URI text. The candidate stores the full chain in a loggable token rejection, allowing a malicious provider to reflect its known client secret or PKCE verifier into Wyrd logs.

At the relying-party boundary, retain only the standard closed OAuth error kind for a server response and fixed local classifications for parsing or other refusal classes. Discard provider descriptions, URIs, bodies, response bytes, and source strings before constructing any loggable error. Preserve the existing stable public refusal and do not add a redaction framework, allowlist, setting, or second error taxonomy.

Prove a provider response containing a canary/secret in description and URI reaches neither the existing trace sink nor the public response while retaining a useful standard refusal classification.

### Duplicate human audience — `FIND-TASK-009-6`

The shared redemption input accepts both client ID and ID-token audience, and the platform configuration exposes both. They can disagree even though REQ-004 makes the human audience the relying party's client ID.

Derive human ID-token verification audience directly from client ID. Remove the independent platform human audience request, view, and persistence setting and update generated/public projections from their source. Do not add equality validation, a toggle, fallback, or compatibility alias. Preserve workload `expected_audience`, which belongs to the separate RFC 7523 contract.

Prove human tokens addressed to the configured client ID succeed, every other audience is refused, and no independent platform human audience input remains. Run the required contract/codegen proof and the real platform journey.

### Unneeded relying-party surface — `FIND-TASK-009-7`, `FIND-TASK-009-8`

`RelyingParty::http` has no caller and exposes a lower-level escape hatch. State and nonce generation use a Wyrd-owned length constant instead of the selected library's conventional constructors.

Delete the unused accessor. Delete the length constant and use the library's standard state and nonce constructors; retain the library's S256 PKCE generator. Add no replacement surface or setting. The authorization-request test must continue proving nonempty state/nonce and S256 PKCE.

### Changed-helper documentation — `FIND-TASK-009-9`

The existing stable-reason documentation was left attached to the new JWKS validator, while the stable-reason constructor lost its rustdoc.

Give each changed helper accurate adjacent rustdoc describing its actual role, without changing behavior or adding a documentation check.

### Exact named-test evidence — `FIND-TASK-009-10`

TASK-009 names individual relying-party and identity tests but records only aggregate lanes. That does not prove the exact zero-selection-safe commands were run.

Run every specifically named test in TASK-009's implementation evidence through its exact repository-native selector. For identity journeys, first list the ignored target and then use the prescribed `WYRD_IDENTITY_TARGET=server WYRD_IDENTITY_FILTER=<exact-name>` wrapper. Record the command, selected test identity, remediated candidate commit, and exit status in the existing task evidence. Do not add a harness or permanent check.

## Constraints and preserved behavior

- Keep `openidconnect 4.0.1` as the human relying party and keep all its traffic on the one existing `ScreenedHttp` adapter.
- Keep SSRF screening, DNS pinning, HTTPS policy, no proxy, no redirects, the 1 MiB response cap, timeouts, and routed `FIND-TASK-004-13` closure unchanged.
- Keep tenant selection, single-use login state, role mapping, user/principal issuance, audit, tenant/platform plane separation, connection-test semantics, and connection revision checks unchanged.
- Keep one forced rediscovery at most per redemption for an unknown `kid`; do not promise one network request across replicas or every invalidation interleaving.
- Keep workload assertion verification on `ExternalVerifier` and do not duplicate signature/JWKS verification.
- Keep `oauth2`'s `reqwest` feature disabled.
- Do not add a dependency, provider-specific branch, compatibility alias, cache setting, retry policy, health probe, distributed coordination mechanism, redaction subsystem, or new test harness.
- No BFF, device-grant, refresh/session, SAML, SCIM, or authorization-server redesign belongs in this remediation.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-009-1` | Served platform begin and callback reuse one process-owned relying-party cache; a callback can use still-valid cached provider state during a discovery outage. |
| `FIND-TASK-009-2` | The real platform callback validates RFC 9207 before token redemption, succeeds for matching `iss`, and refuses advertised-missing/mismatched `iss` without token endpoint traffic. |
| `FIND-TASK-009-3` | Workload admin creation and boot seeding succeed from valid discovery when JWKS is unavailable; assertion verification still fails closed until keys are available. |
| `FIND-TASK-009-4` | Overlapping process-local cache misses and rotation refreshes use Moka coalescing; each redemption has at most one retry and still-unknown keys fail closed. |
| `FIND-TASK-009-5` | Provider-controlled OAuth descriptions, URIs, bodies, and source text cannot appear in public errors, logs, or traces. |
| `FIND-TASK-009-6` | Human ID-token audience equals the configured client ID by construction; no independent platform human audience setting remains. |
| `FIND-TASK-009-7` | The unused transport accessor is absent and the workspace compiles. |
| `FIND-TASK-009-8` | State and nonce use library defaults and PKCE remains S256. |
| `FIND-TASK-009-9` | Both changed private helpers have accurate adjacent rustdoc. |
| `FIND-TASK-009-10` | Durable evidence contains successful exact, non-zero-selection commands for every named test. |

## Verification

Use Red-Green-Refactor for each executable correction and record each focused RED and GREEN. Every specifically named Rust test must use an exact `mise exec -- cargo nextest run --locked ... -E 'test(=...)'` command; identity journeys must use the repository's listing and exact `WYRD_IDENTITY_FILTER` workflow.

At minimum, add or extend focused proof for:

- process-owned platform cache reuse across served begin/callback requests and outage;
- platform callback success plus RFC 9207 missing/wrong refusal before token traffic;
- workload setup with discovery available and JWKS unavailable, for both admin and boot consumers;
- concurrent Moka cold/expired miss and unknown-key refresh coalescing;
- OAuth provider-error canary exclusion from logs and public errors;
- client-ID-derived audience and removal of the public duplicate setting;
- conventional library state/nonce plus S256 PKCE;
- all previously named TASK-009 tests with exact selectors.

Then run the original TASK-009 broader verification set applicable to the touched final surfaces, including the unfiltered identity journey, shared and Wyrd server suites, principal integration, codegen, docs, formatting, lints, language format/lint/unit/typecheck/integration lanes, SDK/CLI journeys, N-API check, client/PyO3/unwrap boundaries, and `check:workspace-hack`. If the remediation changes only Rust and generated contracts, omit unrelated language runtime lanes only when TASK-009's existing evidence remains valid and record the source-based reason; do not rerun a lane solely because of the already-resolved generated-only `9ef532660` delta.
