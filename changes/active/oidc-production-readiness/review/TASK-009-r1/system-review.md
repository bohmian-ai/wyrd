# TASK-009 system-resilience review

## Subject and result

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `6578921d8ced6316c850e4d8f16bd101630a7056`
- Task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Approved authority: `SPEC-oidc-production-readiness`, revision 11
- Routed prior finding: `FIND-TASK-004-13` (no redirects on secret-bearing calls)
- Overall result: **FAIL**

The candidate fails closed without crashing the shared server, bounds provider
IO, refuses redirects, and keeps tenant login's provider cache process-local and
shared. Platform login does not actually share that cache: its HTTP adapter
constructs a new `PlatformLogin` and therefore a new empty `RelyingParty` on
every begin and callback request. The recorded verification is also not final-
candidate proof because the crypto feature union changed after every runtime
lane, and only compile-time linting plus the workspace-hack consistency check
were rerun.

## Reviewed deployment and runtime paths

| Path | Deployment/process ownership | Failure and recovery evidence | Assessment |
|---|---|---|---|
| Tenant login, connection test, and tenant callback | `install_auth` constructs one `HumanConnections` per `wyrd-server` process (`crates/wyrd/wyrd-server/src/boot/mod.rs:1583-1612`). Its clones share the `RelyingParty`/Moka cache (`crates/wyrd/wyrd-auth/src/connections.rs:84-100,148-177`). Replicas intentionally have independent caches. | A cold process discovers metadata and JWKS. Cached metadata serves later begin/callback work for five minutes. An unknown `kid` forces one discovery for that redemption (`crates/shared/wyrd-auth-oidc/src/relying_party.rs:419-488`). Discovery, token exchange, and body reads are bounded and return typed errors; a failed callback leaves its one-time state spent and creates no session (`crates/wyrd/wyrd-auth/src/callback.rs:57-205`). | PASS. Restart/rolling replacement produces ordinary cold caches, while another replica and unrelated server capabilities remain available. Key rotation fails closed if the one refresh fails. |
| Platform login and callback | `begin_login` and `complete_login` each call `login_service` (`crates/wyrd/wyrd-server/src/components/platform/identity.rs:647-670,695-705,731-748`). `login_service` constructs a fresh `PlatformLogin`, whose constructor creates a fresh `RelyingParty` and cache (`crates/wyrd/wyrd-auth/src/platform_login.rs:82-123`). | Begin discovers successfully into a request-local cache which is dropped. Callback consumes durable state, then necessarily discovers again (`platform_login.rs:220-285`). A discovery outage between those requests therefore refuses the callback even when the still-valid metadata/JWKS had just been fetched. Every concurrent platform request also starts cold, amplifying provider discovery during degradation. The global platform credential remains available, so the failure is confined to federated platform login rather than the whole server. | FAIL: SYS-001. |
| Screened provider transport | Every library request calls `ScreenedHttp::client_for`, which resolves/screens/pins the requested origin, disables redirects and proxies, sets a ten-second request timeout, and caps decoded responses at 1 MiB (`crates/shared/wyrd-auth-oidc/src/screening.rs:20-32,75-97,139-220`; `relying_party.rs:158-210`). | Resolver stalls and request/body stalls terminate with a typed refusal. `3xx` responses are returned to `openidconnect` and never followed; `5xx` is classified unavailable. Dropping a canceled request releases local response/body state. Unit proof covers redirect target receiving zero requests and discovery/token outage (`relying_party.rs:1131-1197`). | PASS. This closes routed `FIND-TASK-004-13` without a second transport or redirect option. |
| Session/grant commit after callback | Tenant state is consumed before provider IO; user, roles, refresh family, audit, session, and sealed completion commit together afterward (`crates/wyrd/wyrd-auth/src/callback.rs:121-205,207-319`). Platform state is consumed before provider IO, and verified identity/session issuance is delegated to the transactional platform session owner (`platform_login.rs:239-312`). | Cancellation before the final transaction creates no local authority. Cancellation or transport loss after remote code redemption may make that code unusable on retry; the state is already spent, so the user restarts login. This is the conventional one-time authorization-code failure boundary and avoids replay or duplicate issuance. | PASS. No shared process or unrelated service is taken down. |
| Boot-time workload issuer discovery | Boot uses one local relying party for the configured issuer loop and keeps an already-seeded row when discovery remains unavailable (`crates/wyrd/wyrd-server/src/boot/issuer.rs:106-183,220-259`). | A first-time unreachable issuer blocks boot; a restart with durable prior trust logs the outage and continues. Retry count and backoff are bounded. | PASS for the existing boot contract. This path is workload trust, not the human relying-party cache. |

## Credible failure scenarios

### Dependency outage and timeout

Discovery/JWKS/token failures are contained to the affected login request and
are projected as fail-closed `401`/`503` responses (`crates/wyrd/wyrd-auth/src/error.rs:34-75` and
`crates/wyrd/wyrd-server/src/components/platform/identity.rs:783-809`). No panic,
process exit, readiness dependency, or retry loop was introduced. The ten-second
DNS and request bounds prevent an indefinitely held request. The platform cache
lifecycle defect nevertheless adds a discovery dependency to every platform
callback and turns a short discovery-only outage into loss of all in-flight
platform federated logins.

### Restart and rolling replacement

Tenant cache loss on a new replica is expected and safe: the new process
rediscovers while old replicas continue serving, and all durable login state is
in Postgres. Platform requests are effectively in permanent cold-start mode,
not merely cold after replacement, because no cache survives an individual
request. That contradicts the task's process-local cache topology and makes
rolling behavior no better or worse than steady state for this path.

### Key rotation

For a request that reaches `RelyingParty::redeem`, an unknown key causes one
forced discovery and one verification against the fresh set; a still-unknown
key is refused. The refresh publishes fresh metadata only after successful
discovery, so a failed refresh does not poison the prior cache. The focused
unit tests count exactly two discovery/JWKS calls for success and failure.
Tenant login exercises the intended cached-old-key path. The platform server
wiring cannot exercise it across begin/callback because callback always starts
with an empty cache (SYS-001).

### Shared-server propagation

Provider failures return from the request path and do not affect other tenants,
workload authentication, the global platform credential, health checks, or the
server process. The screened adapter creates request-scoped clients and retains
no failed response state. No recommendation here adds retries, health probes,
distributed cache state, or another failure detector; those would exceed the
task and are not needed by the standard OIDC flow.

## Recovery and proof assessment

The candidate records successful pre-`9ef532660` runs for the full identity
journey and the other named task lanes. Commit `9ef532660` then regenerated
`workspace-hack`, changing the effective third-party feature union for the
cryptographic dependencies used by this task (`ed25519-dalek` `pem` and
`rand_core`; `elliptic-curve` `ecdh`, `hazmat`, `pem`, and `std`; `rsa` `pkcs5`
and `sha2`; plus `crypto-bigint`, while removing the direct `signature` entry).
Only `mise run lints` and `mise run check:workspace-hack` were rerun.

That is not sufficient final-candidate proof. `mise run lints` invokes Clippy
for all features/targets and the shipped server binary, so it credibly proves
the regenerated graph compiles. `check:workspace-hack` proves only Hakari
consistency. Neither executes discovery, PEM/key decoding, signature
verification, token redemption, unknown-key refresh, or the real Keycloak/Dex
journeys under the final feature union. At minimum the affected relying-party
tests and `test:identity:journey` must run after `9ef532660`; the task's own
verification contract requires its listed runtime lanes against the immutable
candidate. This is SYS-002.

The existing tests also do not close SYS-001. The platform administration
journey bypasses the provider callback by directly calling
`WyrdTestServer::federated_platform_session` (`crates/wyrd/wyrd-testing/src/server.rs:2685-2725`),
and platform-login unit tests exercise authorization endpoint screening only
(`crates/wyrd/wyrd-auth/src/platform_login.rs:370-543`). No server-path test
performs platform begin then callback through the two handlers or proves cache
survival between them.

## Proposed findings

### SYS-001 — INCORRECT: platform login's provider cache is discarded per request

- Violated obligation: TASK-009 requires tenant login, connection testing, and
  platform login to use the per-issuer metadata/JWKS cache, with one forced
  rediscovery for an unknown key. The deployment topology calls for a
  per-process cache shared by the relying-party consumers.
- Exact location: `crates/wyrd/wyrd-server/src/components/platform/identity.rs:647-670,695-705,731-748` and
  `crates/wyrd/wyrd-auth/src/platform_login.rs:107-123,153-164,239-285`.
- Evidence: both HTTP handlers call `login_service`; every call constructs
  `PlatformLogin::new`, which constructs `RelyingParty::new` and an empty Moka
  cache. The begin request's cache is dropped before the callback request.
- Observable consequence: a platform login whose begin succeeded fails at
  callback if discovery/JWKS becomes unavailable in between, even though the
  process fetched valid metadata moments earlier. Platform callbacks always
  incur discovery/JWKS IO, and concurrent requests amplify an IdP degradation.
  No platform session is issued, but the one-time state is consumed and the
  user must restart. Tenant login is unaffected; the global platform credential
  remains usable.
- Required correction: make the existing server auth/application owner retain
  one platform `RelyingParty` or `PlatformLogin` per process and reuse it from
  both handlers, matching the existing `HumanConnections` ownership pattern.
  Do not add a distributed cache, cache setting, retry policy, health check, or
  second transport. Prove through the served platform routes that begin warms
  the cache and callback with the same known key can complete while discovery
  is then unavailable; separately prove an unknown key performs only the one
  standard forced rediscovery and fails closed if refresh is unavailable.

### SYS-002 — VIOLATION: runtime proof predates the final crypto feature union

- Violated obligation: TASK-009's verification section and `AGENTS.md` require
  the targeted runtime tests/checks to pass for the changed candidate; compile
  checks do not substitute for user journeys.
- Exact location: candidate commit `9ef532660612a323f6e4e517dd8fa1e4b98a02db`
  (`crates/shared/workspace-hack/Cargo.toml` and `Cargo.lock`) and the
  implementation evidence recorded in
  `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`.
- Evidence: every runtime lane is recorded at `b97fa1c46`; after the feature-
  union regeneration, only `lints` and `check:workspace-hack` were rerun.
  Those commands compile/check but do not execute the relying-party or identity
  behavior.
- Observable consequence: acceptance currently lacks evidence that the final
  dependency feature graph still performs discovery, signature verification,
  key rotation, provider refusal, and Keycloak/Dex flows correctly.
- Required correction: after SYS-001 is corrected and the candidate is fixed,
  run the task's affected focused relying-party/platform tests and the complete
  required runtime verification set, including the unfiltered identity journey,
  against that final immutable candidate. Record exact final-candidate results;
  do not add a new check, setting, feature, or verification mechanism.

## Finding summary

| ID | Classification | Affected capability | Result |
|---|---|---|---|
| SYS-001 | INCORRECT | Platform-administrator federated login during steady state, outage, key rotation, and rolling operation | Open |
| SYS-002 | VIOLATION | Credibility of final-candidate runtime and recovery proof | Open |

**Overall: FAIL.**
