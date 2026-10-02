# TASK-003 r4 system-resilience review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `6aedcda5166509001db0cc851a5bc74502b4b043`
- Approved authority: `SPEC-oidc-production-readiness`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediations: `TASK-003-r2/TASK-003-R2-production-ui-remediation.md` and `TASK-003-r3/TASK-003-R3-browser-renewal-and-rustdoc-remediation.md`
- Human directions: `TASK-003-r1/human-direction-FIND-TASK-003-1.md` and `TASK-003-r2/human-direction-connection-test.md`
- Cumulative range reviewed: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49..6aedcda5166509001db0cc851a5bc74502b4b043`
- Latest remediation locator inspected: `8289fa298ed33d21f2568558bc0a02905fd0b218..6aedcda5166509001db0cc851a5bc74502b4b043`

The repository has no `.codegraph/` directory. The candidate remained at the
stated object id throughout this review.

## Deployed-path coverage

| Runtime path | Deployment and lifecycle ownership | Failure and recovery evidence | Assessment |
|---|---|---|---|
| Production login, callback, and interactive connection test | Browser -> either SvelteKit BFF replica -> public Wyrd login API -> tenant login state in Postgres -> screened OIDC provider -> common callback -> fixed BFF completion; candidate testing reuses that callback without issuing a User, credential, or session | `server-sessions.ts`; `wyrd-auth/src/callback.rs`; `wyrd-auth/src/connections.rs`; prior R2 real-provider journeys | Provider outage, timeout, invalid response, or process loss consumes at most one bounded state and creates no authority. Existing browser sessions renew through Wyrd and do not depend on the IdP. No fallback tenant or provider exists. |
| Session completion and use across replicas | Either BFF -> TLS-protected private `/internal/bff/v1/*` route -> `BrowserSessions` -> tenant RLS transaction -> Postgres; authority returns only to the BFF for one ordinary Wyrd API request | `bff.rs`; `browser_sessions.rs:186-435`; `server-sessions.ts`; production UI journey with two BFF processes and one trusted TLS hop | Session authority is durable and process-independent. A BFF or Wyrd restart loses no committed session. Postgres or private-channel outage refuses the request without selecting mock identity; non-`401` BFF failures retain the cookie for retry. |
| Concurrent renewal, logout, and connection cutoff | `BrowserSessions::current` locks the browser row; OIDC renewal also uses the refresh-family and connection-slot owners; API-key renewal uses ordinary exchange; logout locks the same browser row | `browser_sessions.rs:470-612`; browser-session SQL `FOR UPDATE`; `refresh.rs:115-226`; connection slot-lock paths | One replica rotates while competitors block and reread the winner. Cancellation before the caller commit rolls back the credential and browser-row changes together. Logout cannot race a committed renewal, and a retired connection cannot mint a successor after its cutoff commits. |
| Refresh replay containment | `RefreshTokens` remains the sole family-revocation and canonical-audit owner; `BrowserSessions` preserves its distinct `Contained` result | `browser_sessions.rs:503-528,585-594,690-702`; `refresh.rs:115-173`; focused test at `browser_sessions.rs:1257-1334` | Before access expiry, containment commits, the browser row stays live, and only the already-issued snapshot is served. At or after expiry, family containment and browser revocation commit together. A competing or restarted replica sees the durable family revocation. |
| Internal renewal failure and recovery | Shared error owners classify only credential/lifecycle outcomes as refusal; store, audit, signing, corrupt-state, verification-task, and envelope-open outcomes become `Renewal::Failed` | `issuance.rs:285-299`; `refresh.rs:55-70`; `exchange_api_key.rs:88-107`; `browser_sessions.rs:528,590-609,765-778`; focused tests at `browser_sessions.rs:1336-1420` | The request fails before and after access expiry, the open transaction rolls back, and the session remains available for a later retry. No stale access token is served and no tentative refresh consumption, key-use stamp, or browser revocation commits. |
| Sealing-key and service-key rolling replacement | Canonical boot rewrap inventories all live browser envelopes; exact-byte CAS tolerates concurrent renewal/logout. Wyrd accepts one or two BFF key hashes during bounded overlap | `sealing.rs`; browser-session sealed inventory queries; `boot/mod.rs`; BFF configuration and middleware | A process interruption leaves completed rewraps durable and a later pass converges. A replica lacking a key needed for one stored renewal credential now returns a retryable internal failure instead of causing a destructive session decision. Independent SDK, CLI, MCP, and machine-auth surfaces remain outside this BFF failure boundary. |

## Failure and recovery assessment

- **Dependency outage:** Postgres unavailability stops login, completion, read,
  renewal, logout, and settings at their request boundaries. Audit, signing,
  role/store, API-key verification-task, or envelope-open failures during
  renewal return an error and leave the browser row and backing credential
  unchanged. Recovery permits the next request to retry.
- **Cancellation or Wyrd restart:** a dropped tenant transaction releases the
  browser-row, refresh-family, and connection locks and rolls back tentative
  writes. A successful replay-containment commit may survive a cancelled
  request, which is the required security result; the browser can retry while
  its already-issued token remains within its stored lifetime.
- **Concurrent renewal:** the browser-row lock is the common serialization
  point. The winner rotates and commits through `read` or `authority`; the
  loser rereads the new row as fresh. There is no in-process renewal state to
  lose during replica replacement.
- **Exact expiry:** ordinary credential/lifecycle refusal rolls back and serves
  only a token that PostgreSQL classified unexpired, then ends the session on
  the first lock at or after expiry. Replay containment follows the same
  snapshot boundary while preserving its required commit. Internal failure
  never becomes either outcome.
- **Replay:** an already-rotated refresh token commits complete-family
  revocation and the canonical denied audit. The attacker's successor cannot
  rotate after that commit. Repeated containment attempts before access expiry
  remain bounded by the existing token lifetime; the approved R3 remediation
  intentionally adds no persistent marker or second audit owner.
- **Corrupt or unopenable stored renewal credential:** the flagged behavior
  change is correct. Treating this as `Renewal::Failed` avoids converting one
  replica's missing retained key, transient key-provider error, or repairable
  stored-state problem into a permanent browser logout. The BFF receives a
  non-`401` failure and retains the cookie; no token is served and no durable
  mutation commits. A permanently corrupt envelope remains unavailable until
  operator repair or absolute session expiry, but it does not broaden
  authority or amplify the fault into unrelated authentication surfaces.

## Affected capabilities and blast radius

The production UI depends on the BFF channel, browser-session owner, shared
sealing material, and Postgres. Their outage can make UI requests unavailable,
but it cannot authorize a fallback identity, select another tenant, or stop
independently authenticated SDK, CLI, MCP, and machine traffic. IdP outage
affects new login and connection testing only. The R3 correction narrows an
audit/signing/store/key failure from durable fleet-wide browser logout to
retryable request failure for sessions that reach renewal during the outage.

## Material proposed findings

None.

The prior `SYSTEM-R3-001` path is closed: replay containment, ordinary
credential refusal, and internal failure retain distinct transaction meanings
through `Renewal::{Contained, Refused, Failed}`. No new reachable runtime or
recovery regression was found in the cumulative candidate.

## Proof assessment

The R3 implementation record reports the four focused Postgres selectors, the
renewal-classification unit selector, `mise run test:wyrd`,
`mise run test:identity:journey`, `mise run check:tenant-isolation`, formatting,
lints, and `git diff --check` green. The focused source assertions establish
durable replay containment, exact-expiry session termination, rollback before
and after expiry for both renewal modes, unchanged backing state, and successful
renewal after repair.

This review did not rerun builds, Postgres, provider, browser, Cargo, mise, or
pnpm commands. Direct audit-storage outage, Postgres outage/recovery, process
cancellation between containment commit and relock, and mixed old/new BFF
service-key rollout remain unexecuted recovery cases. Source inspection shows
bounded fail-closed behavior, transaction rollback/commit ownership, and the
two-key overlap mechanism for those paths; none exposes a material source
defect.

## Overall result

**PASS**

The cumulative candidate preserves cross-replica browser sessions and confines
dependency failures to the affected request or UI capability. The latest
remediation closes the prior outage-amplification defect without introducing a
second state owner, retry loop, persistent marker, or broader service failure.
