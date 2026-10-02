# TASK-009 round-4 system-resilience review

## Subject and result

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `0bd3686e8bb763b07376f84661946aadc6200bfb`
- Candidate tree: `86fb8e11681b4d2e60bba2aa7ee3bf2c4f134153`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Current remediation: `changes/active/oidc-production-readiness/review/TASK-009-r3/TASK-009-R3-platform-login-boundary-and-contract.md`
- Overall result: **PASS**

The cumulative candidate keeps provider failure inside the affected login or
configuration request, preserves durable login and connection authority in
Postgres, and recovers from process replacement through ordinary cache misses.
No provider failure crashes the shared server or removes the independent
platform credential path. The round-three runtime change only makes the
existing platform configuration `503` contract accurate and pins it with the
existing served tests.

`FIND-TASK-009-5`, `FIND-TASK-009-14`, and `FIND-TASK-009-15` are withdrawn by
lead direction. This review does not reopen them directly or indirectly.

## Deployed-path assessment

| Runtime path | Deployment and process ownership | Failure, recovery, and affected capabilities | Assessment |
|---|---|---|---|
| Tenant begin, callback, and connection test | `install_auth` builds one `HumanConnections` per `wyrd-server` process; its clones share one `RelyingParty` and Moka provider cache (`crates/wyrd/wyrd-server/src/boot/mod.rs:1585-1590`; `crates/wyrd/wyrd-auth/src/connections.rs:84-105,148-178`). Login state, connection revision, and completion state are durable in Postgres. | A new or restarted replica begins cold and discovers through the screened provider transport. Other replicas and unrelated server capabilities remain available. Callback consumes state before provider IO; cancellation or outage leaves no issued session and requires a new login, while replay remains refused (`crates/wyrd/wyrd-auth/src/callback.rs:57-127,134-207`). | PASS |
| Platform connection configuration | The authenticated platform route uses the boot-owned `PlatformLogin` and its process-local `RelyingParty`; full metadata and JWKS discovery completes before the audited Postgres upsert (`crates/wyrd/wyrd-server/src/components/platform/identity.rs:173-297,665-677`). | Unavailable, undecodable, or mismatched discovery/JWKS returns the existing `503/WYRD_AUTH_503_DISCOVERY_UNAVAILABLE`; the durable row is unchanged. A successful same-issuer configuration refreshes this process's cache before commit, as the remediation explicitly preserves. A later store or audit failure persists no connection change. | PASS |
| Platform begin and callback | One `PlatformLogin` is built per server process and shared through `ServerAuth`; begin and callback load the current durable connection and use the same provider cache (`crates/wyrd/wyrd-server/src/boot/mod.rs:1591-1603,1620-1628`; `crates/wyrd/wyrd-server/src/components/auth/state.rs:34-42`; `crates/wyrd/wyrd-auth/src/platform_login.rs:83-141,156-181,238-346`). | Restart or rolling replacement loses only cache contents. A cold replica rediscovers; a warm replica reuses metadata/JWKS. Provider outage or token refusal fails the request closed without terminating the process. State is one-use, so an interrupted remote redemption restarts rather than risking duplicate authority. Identity pinning, session minting, and audit commit atomically in the platform session owner (`crates/wyrd/wyrd-auth/src/platform_sessions.rs:187-260`). | PASS |
| Provider transport and key rotation | All human relying-party IO goes through `ScreenedHttp`; redirects and proxies are disabled, destinations are screened and pinned, request/DNS work is bounded, and response bodies are capped. `RelyingParty` caches at most 256 issuers for five minutes (`crates/shared/wyrd-auth-oidc/src/relying_party.rs:141-259,343-436`). | Discovery, JWKS, and token outages return typed errors. Concurrent cold misses for one issuer coalesce through Moka. An unknown `kid` invalidates and re-enters the same cache exactly once; a failed refresh issues no authority and does not retry without bound (`relying_party.rs:479-550`). No second client or provider fallback can redirect failure into another tenant or the platform plane. | PASS |
| Workload federation | Workload setup reads metadata only through the screened transport; assertion verification remains on `ExternalVerifier`, separate from human relying-party caches (`crates/shared/wyrd-auth-oidc/src/relying_party.rs:197-247`; `crates/wyrd/wyrd-server/src/components/auth/state.rs:22-29`). | Human provider cache churn, human callback failure, and platform OIDC outage do not alter workload trust or token verification. Boot-time workload discovery retains its bounded retry and existing durable-row fallback. | PASS |
| Platform recovery credential | `/auth/platform/token` constructs the existing `PlatformSessions` over the operator boundary and exchanges the deployment credential independently of platform OIDC (`crates/wyrd/wyrd-server/src/components/platform/routes.rs:51-124`; `crates/wyrd/wyrd-auth/src/platform_sessions.rs:145-185`). | An absent, removed, or unavailable platform IdP stops only federated platform login. The global credential continues to mint a short-lived platform session unless the shared Postgres or signing-key dependency itself is unavailable; no OIDC health state gates it. | PASS |

## Failure and recovery analysis

### Provider outage, timeout, and cancellation

Discovery/JWKS failures during configuration stop that request before its
durable write. Begin and callback failures are projected as request-level
authentication or availability errors. The screened transport bounds resolver,
request, and response work; there is no retry loop, process exit, or readiness
dependency added by TASK-009. Cancellation after a provider may have redeemed a
code leaves the local one-time state spent, so recovery is a fresh login. That
is the conventional authorization-code boundary and avoids replay or duplicate
session issuance.

### Process crash, restart, and rolling replacement

Provider caches are intentionally process-local. A crash discards only cached
metadata/JWKS; Postgres retains connection rows, one-time state, platform
identity pins, tenant users, roles, refresh families, and audit state. A new
replica repopulates its cache on demand. Existing replicas continue serving
independently, and no distributed invalidation, cache setting, health probe, or
retry mechanism is required by the approved task or conventional OIDC practice.

### Dependency isolation

Postgres failure blocks the operations whose durable state or audit record it
owns, without turning an OIDC component error into a shared-process crash.
Provider failure affects only the connection selected before login and cannot
fall through to another tenant or the platform connection. Platform OIDC is an
additive entry path; its outage does not disable credential-based platform
administration. Workload assertion verification remains on its separate
authority and verifier.

### Key rotation

Cached-key verification performs one standard forced rediscovery when a token
names an unknown key and then either verifies against the refreshed set or
fails closed. Overlapping refreshes share the existing Moka miss. There is no
unbounded retry, alternate issuer, stale-allow path, or new operational
mechanism.

## Recovery and proof assessment

The candidate's round-three executable delta is limited to the existing
platform configuration failure contract and its existing tests. Candidate
verification reported by the orchestrator is:

- exact `platform_admin_e2e` selector for
  `federated_platform_sign_in_runs_through_the_served_callback`: **1/1 PASS**;
- exact `pg_openapi_contract` selector for
  `the_served_document_describes_the_composed_surface`: **1/1 PASS**.

Those tests directly prove that unavailable and undecodable JWKS responses are
contained to the configuration request, return the stable discovery `503`,
leave the durable connection unchanged, and publish that response in the
served contract. The remediation record also reports `mise run fmt` and
`mise run lints` green on the final placement-revert candidate; the earlier
behavioral owner lanes were green before that no-behavior revert. Under the
standing narrowest-lane rule, full identity and every-language journeys belong
to final change review, not this task review.

## Material proposed findings

None.

No reachable behavioral, recovery, durability, security, tenancy, or public-
contract defect remains in this review's system-resilience scope. No placement,
naming, structure, wording, custom mechanism, setting, option, or additional
verification lane is proposed.

## Overall result

**PASS**
