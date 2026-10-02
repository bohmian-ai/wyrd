# TASK-003 R5 system-resilience review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `989d0734b0a9b04f314ef4b52aa7d8510f26fe11`
- Approved authority: `SPEC-oidc-production-readiness`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediations: `TASK-003-R2-production-ui-remediation.md`,
  `TASK-003-R3-browser-renewal-and-rustdoc-remediation.md`, and
  `TASK-003-R4-renewal-contract-boundaries.md`; R3 and R4 were explicitly
  authorized by the human owner.
- Human directions: `human-direction-FIND-TASK-003-1.md` and
  `human-direction-connection-test.md`
- Cumulative range reviewed:
  `63c5bffc93cd2f7b5ed558e610a213efcc34fd49..989d0734b0a9b04f314ef4b52aa7d8510f26fe11`
- Latest remediation locator inspected:
  `6aedcda5166509001db0cc851a5bc74502b4b043..989d0734b0a9b04f314ef4b52aa7d8510f26fe11`

The repository has no `.codegraph/` directory. The latest locator changes
only classifier visibility, local rustdoc, and a direct unit check; it changes
no runtime branch, durable state, wire contract, or deployment topology.

## Deployed-path coverage

| Runtime path | Deployment and lifecycle ownership | Failure and recovery evidence | Assessment |
|---|---|---|---|
| Production login and callback | Browser -> either SvelteKit BFF replica -> public Wyrd login route -> Postgres login state -> screened OIDC provider -> common callback -> fixed BFF completion route | `wyrd-ui/src/lib/server/auth/server-sessions.ts`; `wyrd-auth/src/callback.rs`; `wyrd-auth/src/connections.rs`; `identity_ui_e2e.rs` and the production BFF journeys | Provider outage, timeout, issuer mismatch, invalid response, cancellation, or process loss creates no fallback authority. Callback state is bounded and one-use; a spent or interrupted attempt starts over. Existing browser sessions renew through Wyrd and do not contact the IdP. |
| Interactive connection test | Authorized settings action -> `HumanConnections::begin_test` -> durable candidate-bound login state -> the ordinary callback and ID-token verification -> candidate revision stamp | `wyrd-auth/src/connections.rs`; `wyrd-auth/src/callback.rs`; `settings/+page.server.ts`; revision-7 identity journeys | A restart after the state commit loses no durable test state. Provider failure, callback replay, caller deauthorization, audit failure, or a changed candidate refuses only that test attempt, stamps no other revision, and issues no User, credential, or browser session. |
| Session completion and use across replicas | Either BFF -> TLS private `/internal/bff/v1/*` channel -> `BrowserSessions` -> tenant RLS transaction -> Postgres; the BFF receives authority only for its server-side `/v1` call | `components/auth/bff.rs`; `browser_sessions.rs`; browser-session SQL; `production-auth.integration.test.ts` | Session and credential state is durable and process-independent. A BFF or Wyrd restart loses no committed session. A crash before completion commit leaves the completion redeemable; an uncertain response after commit can orphan one bounded session but cannot replay or expose it. Non-`401` channel failures retain the cookie for retry. |
| Concurrent renewal, logout, and connection cutoff | `BrowserSessions::current` locks the session row; refresh/API-key and exact-connection owners run in that transaction; logout locks the same row | `browser_sessions.rs` `current`, `renew`, and `logout`; `queries/auth/browser_sessions.rs` `FOR UPDATE`; `refresh.rs`; `issuance.rs` | One replica serializes renewal while competitors reread committed state. Cancellation or process loss before commit rolls back tentative refresh consumption, API-key use, browser rotation, and required audit together. Logout cannot race a committed rotation, and replacement/deactivation prevents the old connection from issuing a successor. |
| Renewal refusal and replay containment | The shared credential owners classify lifecycle refusal; `RefreshTokens` alone owns family replay containment and its canonical audit; `BrowserSessions` owns only browser-row lifecycle | `Renewal::{Refused, Contained, Failed}` and their branches in `browser_sessions.rs`; focused Postgres tests for ordinary refusal, replay, and internal failure | Ordinary refusal rolls back and preserves the already-issued access token until its stored expiry, then ends the browser session. Replay containment commits family revocation and audit before serving only that same bounded token. A restarted or competing replica sees the durable containment. |
| Internal renewal failure and repair | Store, audit, signing, corrupt-role, verification-task, and absent/unopenable envelope outcomes become `Renewal::Failed` | `IssuanceError::is_refusal`, `RefreshError::is_refusal`, `ExchangeError::is_refusal`, and `open_credential`; R3 Postgres tests plus the R4 direct unit check | The affected request fails without serving stale authority or committing tentative credential/session writes. The browser row stays renewable, so repairing the dependency, role, keyring, or envelope permits retry. The R4 visibility narrowing cannot change this runtime result. |
| Sealing-key and BFF service-key rollout | Boot's canonical rewrap inventories live browser envelopes; exact-byte CAS tolerates concurrent renewal/logout. The server accepts one or two BFF key hashes during bounded overlap | `boot/mod.rs::rewrap_sealed_secrets`; `wyrd-auth/src/sealing.rs`; config parsing; `browser_session_sealing_rotation_journey` | Interrupted rewrap leaves completed changes durable and a later pass converges. Retained keys keep old envelopes readable; the required post-writer pass closes late writes before retirement. A replica missing a needed retained key returns a retryable request failure rather than destroying the session. Independent SDK, CLI, MCP, and machine-auth surfaces remain outside this failure boundary. |

## Failure and recovery assessment

- **BFF process restart or rolling replacement:** the BFF stores no production
  authority in process. Another replica resolves the same opaque cookie through
  Wyrd and Postgres. Two accepted service-key hashes provide the declared
  bounded key-rotation overlap.
- **Wyrd process restart or request cancellation:** open tenant transactions
  roll back and release browser, refresh-family, and connection locks. Durable
  login state, completions, sessions, credentials, and replay containment
  survive restart. Cancellation after a completed containment commit preserves
  the security result; the next request can relock the browser row.
- **Postgres outage:** login, connection testing, completion, read, renewal,
  settings, and logout stop at their request boundaries. No mock identity,
  alternate tenant, or alternate provider is selected. Recovery permits retry
  of operations whose transaction did not commit.
- **Provider/JWKS outage or timeout:** new login and interactive connection
  testing fail closed. Existing browser authority continues until its Wyrd
  access-token boundary and renews without an IdP call. OIDC-off API-key
  sessions and independent workload authentication are unaffected by provider
  availability.
- **Audit, signing, issuance-store, or envelope failure:** creation decisions
  still fail closed. During renewal, the failure is request-scoped and rolls
  back; it does not become a durable logout or commit a half-consumed refresh
  token/API-key use. Once repaired, the same browser session can renew.
- **Connection replacement, deactivation, principal/tenant suspension, or
  API-key revocation:** no successor authority is minted. The current access
  token retains only its approved bounded snapshot lifetime; first use at or
  after expiry ends the browser session. There is no alternate-credential
  retry.
- **Refresh replay:** family revocation and its canonical audit commit even
  when the browser's current access token is still valid. The attacker's
  successor cannot rotate; browser authority ends at that already-issued
  token's expiry.
- **Sealing-envelope failure:** the server returns no authority. The BFF maps
  the non-`401` error to an upstream failure and keeps the opaque cookie, so a
  corrected keyring or repaired envelope restores service without widening
  authority. An unrepaired session remains unavailable and naturally becomes
  unreachable at absolute expiry.

## Affected capabilities and blast radius

The production UI depends on the BFF private channel, the shared deployment
sealing material, and Postgres-backed browser sessions. Failure of one of
those dependencies can make the affected UI request or the UI surface
unavailable, but cannot authorize a fallback identity, select another tenant,
or stop independently authenticated SDK, CLI, MCP, or machine traffic. IdP
failure is narrower: it blocks new login and connection testing, not existing
Wyrd-issued browser authority.

The candidate preserves the intended recovery boundary introduced by R3:
security/lifecycle refusals may end renewable authority, replay commits its
containment, and infrastructure or corrupt-state failures remain retryable
request failures. R4 only makes that policy crate-private and documents and
tests the already-implemented envelope case.

## Material proposed findings

None.

The previously identified renewal-outage amplification path remains closed.
No new reachable runtime, cross-replica, rollout, cancellation, dependency, or
recovery regression was found in the cumulative candidate.

## Proof assessment

Recorded evidence reports both exact R4 unit selectors, the four focused
Postgres renewal selectors, `mise run test:wyrd`,
`mise run test:identity:journey`, `mise run check:tenant-isolation`,
`mise run fmt`, `mise run lints`, and `git diff --check` green. Together the
focused checks cover missing/unopenable renewal envelopes, classifier parity,
ordinary refusal before and after expiry, committed replay containment, and
rollback plus successful retry for refresh and API-key internal failures. The
identity journeys cover real providers, two BFF replicas, connection
replacement, sealing-key rotation, and independent tenant/provider sessions.

This review did not rerun builds, Postgres, browser, provider, Cargo, mise, or
pnpm commands. Direct process kill during an open renewal, a live Postgres
outage/recovery, cancellation between containment commit and relock, and a
mixed old/new BFF service-key rollout remain unexecuted recovery cases in this
round. Source inspection establishes bounded request failure, transaction
rollback/commit ownership, durable state, and the two-key overlap for those
paths; none exposes a material defect.

## Overall result

**PASS**

The cumulative candidate keeps browser authority durable across replicas,
contains failures to the affected request or UI capability, preserves replay
containment, and recovers from retryable renewal failures without destructive
session mutation. The latest remediation adds no runtime or deployment risk.
