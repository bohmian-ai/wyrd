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
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md), [implementation execution](../../../../architecture/references/languages/implementation-execution.md), and [testing workflows](../../../../architecture/references/languages/testing-workflows.md)
