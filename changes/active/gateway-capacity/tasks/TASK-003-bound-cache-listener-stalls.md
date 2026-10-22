---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-gateway-capacity
spec_revision: 4
requirements: [REQ-013, INV-001, INV-003, AC-011]
depends_on: [TASK-002]
parent_task: TASK-002
---

# Bound cache staleness after a silent listener failure

## Outcome and Value

Gateway snapshots, Verifier targets, and Oracle cuts stop serving cached data
when their Postgres notification connection silently stops responding. Today
that connection can look healthy forever, leaving a hot tenant on an obsolete
deployment or provider credential. A heartbeat detects the stall, clears the
cache, and makes requests read Postgres until a fresh listener is subscribed.

This is the explicitly requested follow-up to
[TASK-002](TASK-002-complete-startup-and-gateway-closeout.md), closing revision
4's existing listener-loss and bounded-lifecycle obligations. Implement with
`$wyrd-implement`; do not repeat startup refactoring or benchmark closeout.
Start after TASK-002 is complete and integrated, including its closeout; this
task does not hold TASK-002 open or move its remaining obligations here.

## What Changes and What Must Stay True

- Put the duplicated notification-watch behavior in `wyrd-sql`; Vala consumes
  it through `vala-sql` reexports. Keep the existing reset/notification callback
  responsibilities. Each engine still owns its cache and supervised worker,
  with a separate listener connection per cache and only on applicable roles.
- Probe the listener's own connection with `SELECT 1` every 30 seconds and a
  5-second timeout. Activity must not postpone the heartbeat indefinitely.
  These are internal defaults, not new settings or environment variables.
- Disable SQLx eager reconnection. On a reported disconnect, receive/query
  error, or heartbeat timeout, clear entries and disable cache reads/fills
  **before** reconnecting or waiting for cleanup. Discard the failed listener;
  never issue another query that could silently reconnect it while live.
- Retry failed startup/reconnection after the existing 1-second delay. Enable
  caching from an empty generation only after the replacement's `LISTEN`
  succeeds. Pending acquisition, subscription, ping, receive, and retry must
  remain cancellable within the existing engine shutdown deadline.
- Preserve tenant/table/Card payload handling, malformed-payload clearing,
  eviction limits, and generation fences that reject fills started before an
  invalidation. Drain notifications buffered during heartbeat queries through
  the normal receive path. Preserve Oracle's publication-revision checks.
- Failed listeners must not accumulate retained pool slots or detached cleanup
  tasks. SQLx 0.9's destructor performs asynchronous `UNLISTEN`; ordinary drop
  alone is not evidence of bounded cleanup on a blackholed socket.
- Log the channel and failure reason using existing tracing. A silent failure
  is normally detected within 35 seconds, plus scheduling delay; this is not
  immediate revocation or a stronger consistency contract for in-flight work.

Reuse installed SQLx/Tokio facilities. Relocate only the existing runtime
dependency requirements needed by the extracted watcher; no new third-party
package, version, Cargo feature, custom connector, PID tracking, shared
connection service, cache TTL, migration, or public API/SDK change. Keep raw
pools inside SQL-owner plumbing, behind the existing Postgres handles; do not
spread the current raw-pool signature into domain callers. No new audit events
or changes to authentication, authorization, accounting, or capture behavior.

## Approach

1. Extend the existing refresh journey with a silent-stall failure and capture
   its expected RED before changing production behavior.
2. Consolidate the two watchers at the SQL owner and adopt it in all three
   caches, preserving their independent engine lifetimes and invalidation rules.
3. Add same-connection liveness checks and explicit cache-off recovery; retain
   healthy notification delivery and empty-cache resubscription.
4. Prove detected-loss ordering and bounded cancellation/resource cleanup,
   then update touched rustdoc to explain the actual freshness limit.

## Ordered Implementation Scenarios

### Scenario 1 — A silent stall cannot preserve an obsolete route indefinitely

**Behavior.** Warm a tenant's route through a real server, stop forwarding its
listener traffic without closing either socket, and commit a deployment move
over an unaffected database connection. Within the heartbeat detection budget,
new calls read the current route from Postgres while the listener is still
blocked. Restore listener connectivity, wait for subscription, then move the
deployment again: notification invalidation works after recovery. Another
tenant retains its own route throughout. Exercise the shared watcher's channel
callbacks for Verifier targets and Oracle cuts as supporting integration proof.

**RED.** Add the planned [silent-stall journey](#silent-stall-journey). It must
fail because the old route remains cached past the detection budget, not because
ordinary request/database connections were also blocked. Use portable test-only
traffic control with synchronization proving the stall; an in-process TCP proxy
is suitable. A backend termination or closed socket does not reproduce this bug.

**GREEN.** Add the shared heartbeat and fallback behavior, reconnect from an
empty cache, and pass this journey plus the existing refresh regression. Prove
healthy notification delivery still works, including a notification received
during a heartbeat; heartbeat progress must survive notification traffic.

**REFACTOR.** Remove both duplicated watcher implementations and their stale
reconnect documentation; leave payload parsing and cache mutation with each
cache owner. Keep the journey and healthy-path checks green.

### Scenario 2 — Detected loss disables caching before reconnection

**Behavior.** Terminate a listening session and hold replacement acquisition or
subscription pending. The cache becomes empty and unavailable before recovery
can complete; a read begun under the old generation cannot refill it. Caching
resumes empty only after a successful subscription. Startup/acquisition or
subscription failure likewise leaves caching off and retries without spinning.

**RED.** Add the planned [loss-ordering test](#loss-ordering). Assert the cache-off
transition while reconnection is deliberately blocked; the existing eager
reconnect behavior fails that ordering. Cover the failure cases above with the
smallest shared-owner Postgres fixture, rather than additional SDK journeys.

**GREEN.** Make disconnect handling explicit with eager reconnect disabled and
ensure every recovery path preserves that ordering. Run this test, Scenario 1,
and the existing refresh regression.

**REFACTOR.** Keep one recovery policy for all callers and remove redundant
reconnect checks. No backend-PID comparison is needed when a disconnected
listener is discarded before further use.

### Scenario 3 — Stalled listeners release resources and respect shutdown

**Behavior.** Repeated stalls and recoveries do not consume additional retained
pool slots or leave cleanup tasks waiting on abandoned connections. Cancelling
during receive, heartbeat, acquisition, subscription, or retry clears/disables
the cache and finishes within the existing shutdown deadline. Stalling one
cache's listener does not stop the other listeners processing notifications.

**RED.** Add the planned [listener-lifecycle test](#listener-lifecycle). Keep the
failed socket blocked while asserting resource release and cancellation; do
not unblock it merely to make teardown pass. Cover cancellation stages as cases
of this one lifecycle test. Record any already-correct cases as regression
proof rather than manufacturing failures.

**GREEN.** Bound failed-listener disposal and all pending worker stages using
the pinned dependency's supported facilities. Pass this test and earlier
scenarios without weakening shutdown assertions or adding OS-specific controls.

**REFACTOR.** Remove redundant cleanup paths and temporary test instrumentation;
retain only the smallest deterministic fault support required by these proofs.

## Acceptance Criteria

| Spec obligation | Required observation and proof |
|---|---|
| REQ-013, AC-011 | Scenario 1: a silently stalled listener stops cache service within 30 s + 5 s, allowing scheduling tolerance recorded by the test; uncached calls see committed changes before recovery, and subsequent notifications work. Stale routes past the bound fail. |
| REQ-013, AC-011 | Scenario 2: cache-off/clear precedes blocked recovery; startup failures stay off; old-generation fills are rejected; successful subscription starts empty. Any live cache during recovery fails. |
| REQ-013, AC-011 | Scenario 3: cancellation respects the existing deadline, repeated stalls release resources while the fault remains, and other listeners remain functional. Retained slots/tasks or coupled failures fail. |
| INV-001, INV-003 | Existing refresh regression plus owner tests preserve tenant/payload/generation isolation and Oracle revision checks. Source inspection confirms all three caches use one SQL-owned implementation while retaining independent workers. No public contract, privilege, or durability change. |

## Implementation Reference

- `crates/wyrd/wyrd-server/src/postgres.rs::watch_channel` and
  `crates/vala/vala-bifrost-redux/src/catalog/cut_cache.rs::CutCache::watch`
  duplicate connect/LISTEN/reset/receive/retry behavior, with no liveness probe
  and cache reset after SQLx's default eager reconnect. Extract that workflow
  once; a new notification framework or trait has no demonstrated need.
- `components/gateway/snapshots.rs::GatewaySnapshots::watch` and
  `verification/cache.rs::VerifierCardCache::watch` call the server watcher.
  Their `reset`, `fill_generation`, and `insert` operations already fence fills;
  retain them. `catalog/bifrost_catalog.rs::BifrostCatalog::watch_cuts` calls the
  cut watcher and must retain the Vala Postgres boundary.
- `crates/wyrd/wyrd-sql/src/{postgres,operator_pool}.rs` own Wyrd connections;
  `crates/vala/vala-sql/src/{postgres,lib}.rs` own Vala pool access and reexports.
  Use these boundaries for listener construction, not a raw pool passed from
  domain code. The manifests already share SQLx 0.9.0 and workspace Tokio;
  `wyrd-sql` currently declares Tokio only for tests, so account for the
  extracted runtime requirements without new package versions/features.
- `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs`
  contains `RefreshJourney` and the existing refresh regression below. It
  covers real gateway writes, tenant separation, backend termination and
  recovery, but not silently blocked traffic or cache-off ordering. Extend
  that journey and reuse `WyrdTestServer`/`PgFixture`; portable fault support
  belongs in the existing testing owner, with fixture structure left to the
  implementer. No production fault switch or runtime binding change is needed.
- SQLx 0.9.0 `PgListener::try_recv` has no await between removing a dead
  connection and returning `None` when eager reconnect is disabled. Its query
  executor buffers notifications, and its destructor awaits `UNLISTEN` in a
  spawned task. Verify these pinned behaviors when implementing cleanup.

## Verification and Evidence

The three new names below are **planned** in the existing `wyrd-testing`
`server` target and `verification_runtime` module. Confirm actual selectors
after adding them and before RED; record source-validated corrections. Use
real Postgres and tracing (`WYRD_LOG=info,wyrd_server=debug`) when diagnosing
failures. Test fault controls may shorten timing privately; retain an explicit
assertion of production defaults. Do not add sleeps as synchronization.

Run `mise run build:server` once before the focused server tests: the existing
journey lane requires the same-profile worker binary. Each command below owns
its Postgres environment through the repository wrapper.

### Silent-stall journey

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::silent_listener_stall_falls_back_and_recovers)'
```

### Loss ordering

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::listener_loss_disables_cache_before_reconnect)'
```

### Listener lifecycle

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::listener_shutdown_releases_stalled_connections)'
```

### Existing refresh regression

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::engine_owned_refresh_preserves_tenant_changes)'
```

### Affected owners and boundaries

```sh
WYRD_TEST_PACKAGES='wyrd-sql wyrd-server' mise run test:wyrd
WYRD_TEST_PACKAGES='vala-sql vala-bifrost-redux' mise run test:vala
mise run fmt
mise run lints
mise run check:deps
mise run check:tenant-isolation
git diff --check
```

Record RED/GREEN results, actual detection timings, recovery ordering, retained
connection/task counts, and final commands. Static proof checks reexports,
three caller paths, unchanged cache bounds/payload handling, and rustdoc. No
generated contracts or language bindings change, so no duplicate language
journeys or codegen work is required for this internal fix.

## Material Stop Conditions

Stop if bounded failed-connection cleanup requires a SQLx patch/upgrade, new
package/feature, custom connector, broader pool redesign, or a changed security,
shutdown, or freshness contract. Report the precise pinned-API limitation;
do not silently accept leaked cleanup or weaken the fault test. Private fixture
support and equivalent focused selectors remain implementation decisions.

## Authority Links

- [Approved revision 4](../spec.md) and [amendment](../spec-revision-4.md)
- [Parent task](TASK-002-complete-startup-and-gateway-closeout.md)
- [AGENTS.md](../../../../AGENTS.md) and [agent rules](../../../../architecture/agent-rules.md)
- [Wyrd design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-doctrine.mdx), and [Bifrost design](../../../../architecture/bifrost-design.md)
- [Spec-driven development](../../../../changes/spec-driven-development.md), [implementation execution](../../../../changes/implementation-execution.md), and [testing workflows](../../../../TESTING.md)

## Implementation Evidence

### Reuse and owner

- **Existing owners:** `wyrd-server/src/postgres.rs::watch_channel` (gateway
  snapshots, Verifier targets) and `vala-bifrost-redux/.../cut_cache.rs::CutCache::watch`
  (Oracle cuts) duplicated connect/`LISTEN`/receive/retry with SQLx eager
  reconnect and no liveness probe.
- **Gap:** neither could detect a silent stall, and both reset the cache only
  after SQLx had already reconnected.
- **Extension:** one `wyrd_sql::NotificationListener`
  (`crates/wyrd/wyrd-sql/src/listener.rs`), built by
  `WyrdPostgres::listener` / `ValaPostgres::listener` and reexported by
  `vala-sql`, so no domain caller holds a raw pool. Both old watchers are
  deleted. Each cache keeps its own `reset`/`invalidate`, generation fence,
  eviction bounds and worker.
- **Dependencies:** `wyrd-sql` now declares the workspace `tokio` and
  `tokio-util` it uses at runtime. Tokio's `time` feature already comes from
  SQLx's `runtime-tokio`, so no new package, version or feature was added.
- **Test configuration:** `wyrd-testing` enables the existing
  `wyrd-sql/test-support` feature, which shortens the heartbeat. `check:deps`
  confirms the shipped server enables no test seam.

### Pinned SQLx behaviour and bounded cleanup

- **Disconnects:** with `eager_reconnect(false)`, `PgListener::try_recv`
  returns `Ok(None)` after closing a dead connection, without reconnecting.
- **Probe buffering:** `SELECT 1` runs on `&mut PgListener`, and its stream
  buffers any `NotificationResponse` that arrives during the probe for the
  next `try_recv`.
- **Cancellation:** `recv_unchecked` is cancel-safe, so racing receive
  against the heartbeat tick and shutdown loses nothing.
- **The dropped listener's cleanup task:** `PgListener::drop` spawns an
  `UNLISTEN *` that waits forever on a stalled path.
  `NotificationListener::discard` avoids it using only the public API. It
  detaches a fresh pool connection and `mem::replace`s it into the listener
  through `Acquire for &mut PgListener`, then `close_hard`s the stalled
  connection, bounded by the probe timeout. Dropping the listener then sends
  `UNLISTEN` on the healthy connection and returns it to the pool.
- **When no replacement connection arrives:** the failed listener is kept,
  never more than one per watcher, and released before any new subscription.
  If it still can't be released at shutdown, it is dropped and its stalled
  connection lives until its socket fails. This happens only when Postgres
  itself is unreachable.
- **Defaults:** heartbeat 30 s, probe/stage timeout 5 s, reconnect delay
  1 s. These are internal constants, not settings.

### Portable fault support

- `crates/wyrd/wyrd-testing/src/listener_stall.rs::ListenerStallProxy` is an
  in-process loopback TCP proxy. It routes DSNs with `sslmode=disable` and
  tags each connection by the `LISTEN "<channel>"` it sends.
- Stalling a channel drops data in both directions on the tagged connections
  while their sockets stay open. A server close is still forwarded, and the
  proxy reports client-side release.
- Wiring: `WyrdTestServerBuilder::with_postgres_proxy_for_test` and
  `PgFixture::runtime_dsns`. No production switch.

### RED / GREEN

- **Scenario 1 RED** (before `e04a6d8cd`): `silent_listener_stall_falls_back_and_recovers`
  failed after 61.5 s: `A's route during a silent stall ... stored "/one/..."
  expected "/two/..."`. The cached route never expired, and request
  connections were unaffected.
- **Scenario 1 GREEN:** a stall was detected in 34.4 s and 34.8 s on two runs,
  logged as `cache listener heartbeat timed out ... channel=wyrd_gateway_snapshot`.
  During the stall, calls read Postgres and `listening == 0`. Tenant B kept
  its route. After restore and resubscription, the next move was seen.
- **Scenarios 2–3:** these exercise `NotificationListener` directly, which
  doesn't exist before the extraction, so they have no runnable pre-change
  RED. The old eager-reconnect path is gone. They are recorded as regression
  proof.
  - Loss ordering: stalled starts stayed off and retried at the 1 s delay
    after each 5 s subscription timeout, without spinning. After
    `pg_terminate_backend`, the cache went off while the replacement
    subscription was held pending. Recovery switched it on exactly once.
  - Lifecycle: across three stall/restore rounds, each stalled connection was
    released while the stall was still in place, the other channel's listener
    kept delivering, and the pool grew by no connections. Cancellation took
    25 ms in receive, 1.7 ms during a pending probe, 0.3 ms during a pending
    subscription and 0.07 ms in retry, all well within the 35 s drain. No
    stalled connection was left open.

#### Diagnosis: loss-ordering test terminated its own session

- **Symptom:** `listener_loss_disables_cache_before_reconnect` failed with
  `57P01 terminating connection due to administrator command` from its own
  superuser statement.
- **Evidence:** the trace showed the listener loss being handled normally;
  the error came back on the terminating query itself.
- **Cause:** `SELECT ... WHERE query = $1 AND pg_terminate_backend(pid)`
  leaves the qual order to the planner, which may evaluate the termination
  before the filter.
- **Fix site:** the new test only. It now selects the pids first and
  terminates them in a second statement. The existing
  `RefreshJourney::listeners` was left unchanged; it is outside this task and
  passes.

### Acceptance

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-013, AC-011: silent stall stops cache service within 30 s + 5 s; uncached calls see changes; notifications work after recovery | `listener.rs::receive`/`probe`; three cache `watch` callers | `silent_listener_stall_falls_back_and_recovers` (34.4 s / 34.8 s detection, 10 s tolerance; production defaults asserted) | PASS |
| REQ-013, AC-011: cache-off before blocked recovery; failed starts stay off; empty restart; old-generation fills rejected | `subscription` resets off before `discard`/retry; owner generation fences unchanged | `listener_loss_disables_cache_before_reconnect`; existing owner unit tests in `test:wyrd`/`test:vala` (e.g. `cut_cache_serves_only_fresh_listened_cuts`) | PASS |
| REQ-013, AC-011: bounded cancellation, released stalled connections, independent listeners | `discard`, shutdown branches at every stage | `listener_shutdown_releases_stalled_connections` | PASS |
| INV-001, INV-003: tenant/payload/generation isolation, Oracle revision checks, one SQL-owned implementation | Owners keep `invalidate`/`reset`/`evict`; `rg` finds no `PgListener` outside `wyrd-sql` and tests | `engine_owned_refresh_preserves_tenant_changes`, `test:wyrd`, `test:vala`, `check:tenant-isolation` | PASS |

### Verification

| Command | Result |
|---|---|
| `mise run build:server` | PASS |
| silent-stall, loss-ordering, listener-lifecycle, and refresh exact selectors (task commands) | PASS (4/4) |
| `WYRD_TEST_PACKAGES='wyrd-sql wyrd-server' mise run test:wyrd` | PASS (1018) |
| `WYRD_TEST_PACKAGES='vala-sql vala-bifrost-redux' mise run test:vala` | PASS (970) |
| `mise run fmt`, `mise run lints` | PASS |
| `mise run check:deps`, `mise run check:tenant-isolation` | PASS |
| `git diff --check` | PASS |

- **Non-goals:** none were pursued. There is no cache TTL, setting,
  environment variable, PID tracking, custom connector, shared connection
  service, migration, public API/SDK change, audit/authorization/capture
  change, or OS-specific mechanism.
- **Other files:** none outside the task's write set were changed.
- **Not run (deliberate):** the broad journey suites and `gate` were not run,
  per the task scope.

### Full-gate failure diagnoses

The user asked for the full gate to pass. Each failure was rerun with
`WYRD_LOG` and diagnosed before it was fixed. Timing and connection causes
went to a fresh read-only diagnostician, which was given only the command,
trace, and diff.

| Failure | Cause | Fix site | Proof |
|---|---|---|---|
| Seven TypeScript integration journeys reported `WYRD_TOOL_500_CALL: failed to spawn monty worker: target/x86_64-unknown-linux-gnu/debug/wyrd-server` | napi always builds with `--target <host>`. The harness build script resolved the profile under the triple directory, but `build:server` places the worker in `target/debug`. | `wyrd-testing/build.rs` resolves the host-native profile when `TARGET == HOST`. | Both TS files pass 46/46 (`1e310dd82`). |
| Listener shutdown journey, gate only | `RefreshJourney::listeners` terminated pids in the same statement that filtered `pg_stat_activity`, so the planner evaluated the terminate before the database filter and killed concurrent tests' listeners. | The test helper now selects pids, then terminates them. | All four listener and refresh tests pass (`1e310dd82`). |
| `typed_builtin_payloads_are_queryable` | Revision 4 made the envelope `wyrd_request_id` the Scribe batch id, but the test still filtered on it. | The test filters on the payload `request_id`. | Passes (`1e310dd82`). |
| `client_declaration_saga_returns_active_receipt`: `IdempotentNoop` instead of `Declared` | A pending heavy Card was due for reconciliation at insert (`declare.rs:295`). Under load, the 1 s reconciler verified the landed upload and activated the Card before the client's own completion. The diagnostician agreed. | `insert_card_row` binds `reconcile_delay`. `persist_node` passes the presign TTL for Cards with artifacts and zero otherwise. | Card route tests pass 32/32 and SQL card tests 51/51. A new assertion shows a heavy Card is deferred (`1ab039a84`). |
| `owner_status_serves_stable_binding_ids_and_exchange_activates`: pool timed out at startup | Concurrent tests overcommitted Postgres `max_connections=400`. Each server holds two pool sets, and the Vala pool was uncapped (16). Ungrouped `wyrd-sql`/`vala-sql` fixtures ran outside the budget. SQLx reports 53300 as `PoolTimedOut`. The diagnostician agreed. | The wrapper caps `WYRD_DB_MAX_CONNECTIONS_VALA=8`. Nextest's `pg-servers` group now includes the `wyrd-sql`/`vala-sql` `pg_*` modules, and its budget comment is corrected. The release server removes the cap. | `e05251c5e`; proved by the full gate. |
| Scribe `ambiguous_retry`: retry never acknowledged within 10 s | An already-durable resend re-entered the serial shard and was rewritten and synced behind every fresh write's 750 ms injected sync before the fence suppressed it. | User decision: answer honest retries of fenced, visible batches at ingress from a bounded pod-local index. A miss or mismatch takes the shard path and the fence. `bifrost-design.md` records the exception. | `ambiguous_retry` passes. Scribe lib tests pass 363/363 and Scribe journeys 30/30 (`dfeb1f163`). |
| Identity journey, gate only: Keycloak `IncompleteMessage` | Not reproduced alone: `mise run test:identity:journey` passed. | None. | Judged by the full-gate rerun. |
