# TASK-009 focused follow-up review

## Immutable subject and scope

- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `6578921d8ced6316c850e4d8f16bd101630a7056`
- `HEAD` resolved to the candidate before and after this investigation.
- Scope was limited to the six conflicts routed by the orchestrator. No
  intended verdict was supplied or inferred.
- The standing direction was applied: use the established standard,
  conventional project behavior, or an existing dependency mechanism; do not
  prescribe a Wyrd-specific setting, check, cache, or coordination mechanism.

Paths inspected included the complete bodies and callers in
`wyrd-auth-oidc/src/{relying_party,jwks,provider}.rs` (candidate and base where
applicable), `wyrd-auth/src/{callback,connections,error,platform_login}.rs`,
`wyrd-server/src/{boot/issuer,components/admin/routes,components/platform/identity}.rs`,
`wyrd-spec/src/auth/{oidc,platform_identity}.rs`, the platform and identity
journeys, `wyrd-testing/src/server.rs`, `mise.toml`, commit `9ef532660`, the
task/spec/research authorities, and all discovery reports in this review
directory. The installed `openidconnect 4.0.1` and `oauth2 5.0.0` source was
read only where a library behavior was disputed.

## Resolution 1: platform lifetime and cache behavior

**Resolved: the system/standards claim is correct; the cache-domain statement
is true only inside one `PlatformLogin` value and does not describe the shipped
routes.**

`begin_login` and `complete_login` independently call `login_service`
(`components/platform/identity.rs:652-669,696-704,732-748`). Each call builds a
new `PlatformLogin`, and `PlatformLogin::new` builds a new `RelyingParty` and
therefore a new empty Moka cache (`platform_login.rs:107-123`). No
`PlatformLogin` or `RelyingParty` is stored in `AppState`. The provider cached
during begin is dropped with that request; callback necessarily executes a
new discovery-document and JWKS fetch at `platform_login.rs:257-262`.

The failure path is reachable: begin can persist valid one-use state after a
successful discovery, the provider can become unavailable, and callback then
consumes state and fails its avoidable second discovery even though the
five-minute metadata/JWKS entry would still be valid in a process-owned cache.
Concurrent platform requests also all start cold. The global platform
credential remains usable, so the failure boundary is federated platform
login, not the whole service.

Retain `SYS-001` / `REPO-TASK-009-1`, narrowed to platform composition. Reuse
one process-owned `PlatformLogin`/`RelyingParty` through existing server state;
do not add another cache type, setting, distributed cache, or cross-replica
coordination. The standards report's inclusion of administrative issuer create
in the same lifecycle correction is not supported: that path intentionally
performs one fresh discovery for one create operation and has no cache reuse
workflow. Its separate eager-JWKS behavior is addressed below.

## Resolution 2: platform RFC 9207 and completion coverage

**Resolved: the platform path omits a task-required standard check, and the
recorded journey bypasses the changed callback.**

The task applies the vetted relying party and RFC 9207 to tenant login,
connection test, and platform login (`TASK-009:15-20,41-50,69-73`). Scenario 2
requires a missing or wrong response `iss` to yield no User, credential, or
session (`TASK-009:96-106`), and the acceptance criteria require platform login
plus the RFC 9207 cases (`TASK-009:144-153`). Thus this is not an optional
hardening extension or a rule inferred solely from the RFC.

The tenant callback carries optional `iss` and calls the existing
`verify_response_issuer` before token redemption
(`callback.rs:160-199`; `wyrd-spec/src/auth/oidc.rs:630-650`). In contrast,
`PlatformCallbackRequest` contains only `code` and `state`
(`platform_identity.rs:113-122`), the handler forwards only those values
(`components/platform/identity.rs:732-748`), and `PlatformLogin::complete`
proceeds from state/connection binding directly to `redeem`
(`platform_login.rs:239-285`). It therefore cannot enforce the same conditional
response-issuer comparison when discovery advertises RFC 9207 support.

The alleged platform journey at `platform_admin_e2e.rs:1491-1503` calls
`WyrdTestServer::federated_platform_session`; that helper explicitly stands in
for the provider round trip and directly exercises session issuance
(`wyrd-testing/src/server.rs:2685-2704`). Repository callers contain no test of
`/auth/platform/callback` or `PlatformLogin::complete`. Retain
`INV-REV-001` and `OIDC-SEC-003` as one correction boundary: carry optional
`iss` through the existing platform callback contract, invoke the existing
RFC 9207 verifier before redemption, and extend the existing platform/identity
journey through the public begin and callback routes. No new check, option, or
test harness is needed.

## Resolution 3: workload discovery regression

**Resolved: `BEH-001` is reachable and outside the requested human-RP
replacement.**

At the base, `OidcProvider::discover` fetched and validated only the discovery
document and returned its `jwks_uri`
(`35a53faa2:wyrd-auth-oidc/src/provider.rs:120-190`). Workload admin create and
boot seeding persisted that URI; `ExternalVerifier`/`JwksCache` fetched keys
when a workload assertion actually needed verification. At the candidate,
admin `discover_jwks_uri` calls `RelyingParty::discover`
(`admin/routes.rs:735-771`) and boot retry does the same
(`boot/issuer.rs:131-149,240-253`). `RelyingParty::discover` delegates to
`ProviderMetadata::discover_async`, which fetches both metadata and its JWKS
(`relying_party.rs:348-375`; installed `openidconnect` discovery implementation).

Consequently, a workload issuer with available discovery and temporarily
unavailable JWKS can no longer be registered or initially seeded, although
that operation succeeded at the base and workload verification already owns
the key-fetch failure. The changed fixtures' new `/jwks` responses mask rather
than disprove the new dependency. TASK-009 explicitly keeps workload RFC 7523
on `ExternalVerifier`, limits the new cache to human issuers, and promises no
operator-visible behavior change (`TASK-009:15-20,46-57,69-74`).

Retain `BEH-001`. Restore a one-shot, screened, typed discovery-document read
for the two workload setup consumers and persist the advertised URI, while
leaving key retrieval with the existing workload `JwksCache`. This does not
justify a second cache, readiness probe, option, or provider branch. The
`openidconnect` metadata type is deserializable with an empty skipped key set,
so correction can reuse its typed document representation plus the existing
screened transport; it need not restore the deleted hand-written metadata
struct or human discovery owner.

## Resolution 4: cache concurrency

**Resolved with revision: cold/expired-miss coalescing regressed; a stronger
generation-wide unknown-key guarantee was not present at the base.**

The base `JwksCache` expressly used Moka `try_get_with` for same-URI misses
(`35a53faa2:jwks.rs:232-317`), so overlapping cold misses shared a fetch. Its
unknown-key path invalidated then re-entered that same coalesced fetch. That
coalesced overlapping misses, but it did not prove that every possible
invalidate/fetch interleaving across concurrent stale callers resulted in one
network request; a late invalidation could remove a just-installed value.

The candidate's `cached` performs `get` followed by unconditional `discover`
and even documents that each concurrent miss discovers
(`relying_party.rs:331-345`). `redeem` likewise calls unconditional `discover`
for every callback that observes `UnknownKey` (`:477-485`). Thus the cold or
expired same-issuer regression is certain, and the common stale-key burst can
duplicate refreshes, but discovery did not establish the cache-domain report's
stronger claim that the base guaranteed exactly one refresh under every race.

Retain `CACHE-1` in revised form: preserve the existing process-local Moka
single-flight behavior for issuer misses and route the one permitted
unknown-key refresh back through that native coalesced cache mechanism after
invalidation. Do not require a cache-generation protocol, custom lock,
distributed coordination, public option, or deployment-wide "one request"
promise. TASK-009's "exactly one forced re-discovery" remains a per-redemption
retry bound; each redemption still gets at most one retry. Focused concurrent
proof should establish Moka-level coalescing for overlapping process-local
misses without claiming cross-replica or all-interleaving serialization.

## Resolution 5: final verification after `9ef532660`

**Resolved: `mise run lints` plus `mise run check:workspace-hack` is sufficient
for the isolated Hakari regeneration; a runtime rerun is not mandated for that
commit.**

Commit `9ef532660` changes only generated
`crates/shared/workspace-hack/Cargo.toml` entries and the corresponding
`Cargo.lock` dependency edges. It changes no package version, Rust source,
test, fixture, or route. The added crypto features are additive features
already selected by the new `openidconnect` dependency graph; the regeneration
makes Hakari's workspace union match that graph. `mise run lints` compiles the
entire workspace with `--all-features --all-targets` and separately compiles
the shipped server binary at release features (`mise.toml:56-61`).
`check:workspace-hack` runs Hakari generate diff, manage-deps dry-run, and
verify (`mise.toml:1484-1492`). Together they prove the final union compiles and
the generated file is exact. Re-running every identity or crypto behavior lane
would repeat behavior already exercised before this source-free generation
step and is not required by the repository's narrowest-complete-verification
rule.

This conclusion does not cure unrelated proof omissions: the platform
completion path is not exercised, concurrent cache behavior is not tested, and
the task's mandatory exact named-test commands were not recorded.

## Resolution 6: remaining unique claims

### Provider OAuth error text reaches logs — confirmed (`OIDC-SEC-002`)

`token_error` converts every token error other than a screened/request failure
to `TokenRejected(error_chain(...))` (`relying_party.rs:614-626`). In
`oauth2 5.0.0`, `StandardErrorResponse::Display` includes provider-controlled
`error_description` and `error_uri`; `RequestTokenError::ServerResponse`
displays that value. `relying_party_error` then logs the full
`TokenRejected` value (`wyrd-auth/src/error.rs:69-73`). A token endpoint can
therefore reflect its known client secret or other canary into durable logs,
contrary to REQ-005 and the security posture's prohibition on secrets in logs,
traces, or errors. Retain the finding. Map a server response to its closed
standard error kind and discard description/URI/body before forming a loggable
error; no redaction framework or configurable allowlist is warranted.

### Duplicate human audience — confirmed (`MNT-001` / `OIDC-SEC-001`)

REQ-004 explicitly says the human ID-token audience is derived from client ID,
not entered twice (`spec.md:80-86`). Candidate `CodeRedemption` nevertheless
accepts both values (`relying_party.rs:263-279`) and constructs the token client
from `client_id` but its verifier from `audience` (`:510-520,563-570`). Platform
configuration additionally exposes and persists `expected_audience` separately
from `client_id` (`wyrd-spec/src/auth/platform_identity.rs:41-77`;
`platform_login.rs:264-272`). This permits a standard configuration to fail on
two disagreeing values and permits verification against a different client.
Retain one DRIFT/correctness finding at the shared source: derive verification
audience from `client_id` and remove the independent platform human setting;
do not add an equality setting or compatibility alias.

### Unused transport accessor — confirmed (`MNT-002`)

`RelyingParty::http` (`relying_party.rs:325-329`) has no repository caller.
Discovery, exchange, and verification are already owner methods, while the
workload verifier owns its separate transport. Delete the accessor; no
replacement is needed. This is precisely the standing direction's DRIFT case.

### Custom state/nonce length — confirmed (`MNT-003`)

The candidate adds `RANDOM_VALUE_BYTES = 32` and uses
`CsrfToken::new_random_len`/`Nonce::new_random_len`
(`relying_party.rs:64-65,399-406`). The selected libraries' conventional
constructors use 16 random bytes (`CsrfToken::new_random` and
`Nonce::new_random`) and are the constructors used by their RP examples. The
task asks for library generation and no Wyrd-specific length. The 32-byte
values are secure, but the setting is unearned DRIFT. Delete the constant and
use the library defaults; keep the library's PKCE S256 generator.

### Rustdoc placement — confirmed (`REPO-TASK-009-2`)

At `connections.rs:936-954`, the sentence describing a stable failed-test
reason is attached to `require_usable_jwks`; `not_tested_reason` has no rustdoc.
This directly violates the repository's mandatory documentation rule for
materially changed private Rust items. Move/restore the two accurate comments;
no helper or documentation check is required.

### Exact named-test evidence — confirmed (`REPO-TASK-009-3`)

TASK-009:164-168 requires every named test to be run with its exact selector
and requires identity selection to be listed. The implementation evidence at
`:208-245` names individual tests but records aggregate `mise run` lanes only.
Those broad lanes are useful behavior evidence, but they do not satisfy the
explicit zero-selection-safe evidence contract. Run and record existing exact
selectors/setup commands; do not add a harness or another permanent check.

## Proposed finding union after follow-up

The independently validated union should consider:

1. platform request-local construction defeats cache reuse (`SYS-001` /
   `REPO-TASK-009-1`, narrowed as above);
2. missing platform RFC 9207 enforcement and real callback journey
   (`INV-REV-001` / `OIDC-SEC-003`, one correction boundary);
3. out-of-scope eager JWKS fetch for workload setup (`BEH-001`);
4. lost process-local Moka miss coalescing (`CACHE-1`, revised as above);
5. provider-controlled OAuth error text logged (`OIDC-SEC-002`);
6. duplicate human audience (`MNT-001` / `OIDC-SEC-001`, deduplicated);
7. unused `RelyingParty::http` (`MNT-002`, DRIFT);
8. non-default random-length setting (`MNT-003`, DRIFT);
9. misplaced/missing private rustdoc (`REPO-TASK-009-2`); and
10. absent exact-selector evidence (`REPO-TASK-009-3`).

No finding should require a post-`9ef532660` runtime rerun solely for the
workspace-hack regeneration. No correction above requires a new mechanism,
setting, dependency, cache, harness, or permanent check.

## Result

**RESOLVED** — every routed uncertainty is decidable from the approved task,
repository source, base/candidate history, installed library source, and
recorded verification. The platform lifetime and RFC 9207 claims are confirmed;
the cache concurrency claim is narrowed to behavior the base actually proved;
the workload regression and unique security/DRIFT/standards claims are
reachable; and the Hakari-only final verification is sufficient for that
isolated generated delta.
