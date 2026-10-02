# TASK-003 R6 system-resilience review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `ad3b92ad0f326917c731383fd3b57cb7ac6a8c82`
- Approved authority: `SPEC-oidc-production-readiness`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediations: R2, R3, R4, and R5; R3, R4, and R5 were explicitly
  authorized by the human owner.
- Human directions: conditional callback issuer binding, real interactive
  connection testing, and R5 logout scoped to the browser session's own
  refresh chain.
- Cumulative range reviewed:
  `63c5bffc93cd2f7b5ed558e610a213efcc34fd49..ad3b92ad0f326917c731383fd3b57cb7ac6a8c82`
- Latest remediation locator inspected:
  `989d0734b0a9b04f314ef4b52aa7d8510f26fe11..ad3b92ad0f326917c731383fd3b57cb7ac6a8c82`

The repository has no `.codegraph/` directory. The candidate remained at the
stated object before this report was written.

## Deployed-path coverage

| Runtime path | Deployment and lifecycle ownership | Failure and recovery evidence | Assessment |
|---|---|---|---|
| Login, callback, and interactive connection test | Browser -> either SvelteKit BFF replica -> public Wyrd login route -> Postgres login state -> screened OIDC provider -> common callback -> fixed BFF completion route | `wyrd-auth/src/callback.rs`, `connections.rs`, BFF routes, and cumulative identity journeys | Provider/JWKS outage, timeout, invalid response, cancellation, or process loss creates no fallback authority. Durable bounded state allows retry only where the one-use transition did not commit. Existing browser sessions do not contact the IdP for routine renewal. |
| Session completion and cross-replica use | Either BFF -> private TLS `/internal/bff/v1/*` channel -> `BrowserSessions` -> tenant RLS transaction -> Postgres | `components/auth/bff.rs`, `browser_sessions.rs`, browser-session SQL, and production UI integration coverage | Committed sessions survive BFF and Wyrd replacement. Open transactions roll back on cancellation or crash. A non-`401` private-channel failure retains the opaque cookie for retry and exposes no token to the browser. |
| OIDC renewal and logout | `BrowserSessions` locks the browser row; `RefreshTokens` and logout use the same User refresh-family advisory lock; logout then wipes the browser row in the caller-owned tenant transaction | `browser_sessions.rs:419-447`; `refresh_tokens.rs:96-120,173-209`; migration `20261001000003`; focused Postgres test at `browser_sessions.rs:1478-1567` | A rotation committed before logout is included by the recursive chain update. A concurrent rotation waits on the family lock and then observes revoked ancestry. Lock, SQL, wipe, commit, cancellation, or process failure before commit rolls back both effects. A retry after an uncertain successful commit is an idempotent missing-row no-op. |
| Other login chains and API-key sessions | The browser row stores the first refresh-row id only for OIDC mode; chain traversal follows `rotated_from`. API-key mode stores no chain id | Migration check/FK; `BrowserSessionWrite`; logout test's separate same-User login; existing API-key logout journey | Logout cannot revoke another browser/CLI/SDK chain for the same User. API-key logout wipes browser state without revoking the operator key. |
| Connection cutoff, replay containment, and renewal failure | Exact connection binding governs successor issuance; `RefreshTokens` owns principal-wide theft containment; internal failures remain rollback-and-retry outcomes | `BrowserSessions::current`, `RefreshTokens::execute`, `Renewal` branches, and prior focused Postgres evidence | Connection replacement/deactivation blocks successors. Detected replay still retires the whole User family as the approved theft response, distinct from logout's grant-scoped chain retirement. Store, audit, signing, or envelope failures do not become destructive logout. |
| In-process test-server teardown | `WyrdTestServer::shutdown` cancels the process token and directly calls `Bifrost::shutdown` before dropping the Postgres fixture when `mode` is `InProcess` | `wyrd-testing/src/server.rs:748-787`; Bifrost abort-on-drain-failure contract at `wyrd-server/src/state.rs:1967-2001` | Ordinary router-only in-process harnesses no longer leave Oracle role tasks alive while the fixture force-drops its database. However, `Mode::InProcess` is also assigned to a bound dedicated Forge worker with a live serve task, creating the regression below. |

## Failure and recovery assessment

- **BFF or Wyrd replica replacement:** browser authority and refresh-chain
  identity are durable Postgres state. No in-process owner is required to
  continue a committed session, and another replica takes the same row and
  family locks.
- **Postgres outage or request cancellation:** login, completion, session use,
  renewal, settings, and logout stop at the request boundary. Logout returns
  before commit, so neither chain retirement nor browser wipe can commit
  alone. Recovery permits retry.
- **OIDC provider/JWKS outage:** new login and connection testing fail closed;
  existing Wyrd browser renewal, OIDC-off API-key sessions, and independent
  workload authentication remain outside that dependency boundary.
- **Refresh rotation overlapping logout:** both paths serialize on the User
  family lock. Logout's recursive update sees every successor committed before
  its statement; a later rotation sees the revoked row and cannot mint another
  successor. The browser-row lock separately serializes BFF renewal and logout.
- **Process crash after an uncertain logout response:** PostgreSQL exposes
  either the pre-logout state or the committed chain-and-browser retirement.
  The browser can retry; already committed logout is idempotent.
- **Rolling schema deployment:** the chain column is server-owned, tenant-bound
  by a composite foreign key, and constrained to OIDC mode. These migrations
  are part of the same previously unshipped cumulative delivery, so no released
  browser row requires an online backfill in this subject.
- **Test fixture teardown:** direct in-process teardown now cancels and settles
  Bifrost before database release. The mode predicate is broader than that
  topology and can run the abort path while a dedicated Forge worker is still
  supervised.

## Affected capabilities and blast radius

The production login, browser-session, renewal, and logout paths remain
request-scoped under dependency failure and durable across replicas. Logout
affects only the named browser session's refresh chain; other same-User logins,
API-key sessions, SDK/CLI credentials, MCP, and machine authentication remain
available.

The proposed finding is confined to the repository test harness. It can make a
dedicated Forge-worker teardown exercise an abort-under-live-worker path while
returning success, weakening the reliability of Forge shutdown/recovery
journeys. It does not change a deployed production server path.

## Material proposed findings

### SYSTEM-R6-001 — The in-process drain predicate also matches a live dedicated Forge worker

- **Classification:** `REGRESSION`
- **Violated obligation:** The bundled shutdown correction must settle a
  router-only in-process server before its fixture is dropped without changing
  sibling harness lifecycle ordering. Bifrost authority requires Forge
  supervision to quiesce before role/storage shutdown; the harness itself
  documents that cancellation only signals a worker and joining ends worker
  liveness.
- **Exact location:** `crates/wyrd/wyrd-testing/src/server.rs:767-783`, with the
  reachable producer at `crates/wyrd/wyrd-testing/src/server.rs:3557-3572`.
- **Evidence:** `shutdown` treats every `Mode::InProcess` value as a server with
  no serve task and calls `self.inner.state.bifrost.shutdown(...)` before it
  inspects or joins `serve_handle`. The dedicated `BifrostTarget::ForgeWorker`
  branch in `bind`, however, spawns a live Forge worker into `serve_handle` and
  then explicitly assigns `self.mode = Mode::InProcess`. `Bifrost::shutdown`
  refuses an undrained Forge supervisor and runs `abort_selected_owners`; only
  afterward does `WyrdTestServer::shutdown` join the worker, and it logs rather
  than propagates a terminal worker exit.
- **Observable consequence:** `start_bound()` for a dedicated Forge worker
  followed by the ordinary `shutdown()` path can abort Bifrost roles/storage
  while the worker is still live and still return `Ok(())`. A Forge shutdown or
  recovery journey using that seam can therefore observe a harness-created
  failure or pass without proving the production worker-first lifecycle.
- **Required testable correction:** Select the direct Bifrost drain from the
  absence of a serve task, not from `Mode::InProcess`. Preserve the existing
  serve-task-first lifecycle for the dedicated Forge-worker branch; do not
  introduce another lifecycle abstraction. Add one focused harness check that
  starts that existing bound Forge-worker topology, shuts it down through the
  ordinary seam, and proves the worker has joined before any direct role/storage
  settlement. Retain a router-only in-process shutdown check proving Oracle is
  settled before fixture release.

## Recovery and proof assessment

The R5 implementation record reports the focused logout selector, all
`browser_sessions` tests, `fmt`, `lints`, `codegen:check`,
`check:tenant-isolation`, `test:sql`, `test:wyrd`,
`test:identity:journey`, and `git diff --check` green. This review reran:

```text
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=browser_sessions::pg_tests::oidc_logout_retires_only_its_refresh_chain_without_opening_it)'"
```

The migration checks and focused logout test passed. An earlier direct
`mise exec` invocation failed only because it omitted the repository-managed
Postgres wrapper; it did not execute the behavior under review.

The reported full `test:wyrd` rerun is credible evidence that the original
database-drop abort no longer occurs in its affected router tests. No focused
test exercises `WyrdTestServer::shutdown` with the separate bound
`ForgeWorker` shape that shares `Mode::InProcess`, so the source-proven sibling
lifecycle regression remains open.

This review did not rerun the full browser/provider journey, the broad Wyrd
family lane, or Bifrost's distributed journey suite. Direct process kill during
logout and live Postgres outage/recovery remain unexecuted in this round;
transaction ownership and durable lock/state inspection show no additional
material defect on those paths.

## Overall result

**FAIL**

The R5 logout correction closes `FIND-TASK-003-18` at the approved
session-chain boundary and preserves production failure containment. The
bundled harness shutdown fix introduces one bounded sibling-topology lifecycle
regression, `SYSTEM-R6-001`.
