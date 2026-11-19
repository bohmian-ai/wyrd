# TASK-003 expanded remediation: lifecycle invariants

Result: **FAIL** — proposed finding INV-001 requires independent validation.

Subject: prior `634e19cbc517293ed2458f30ce73401714f3447d` → candidate `480251a70f799b8581ca5a4be77983d6f345d5e3`, with the approved positioning revision 6 and service-identity-and-telemetry revision 17 pinned by `review-scope.md`. HEAD and both authority hashes matched those pins during this review. Source was read from the candidate commit; dirty tests were not counted as candidate proof.

## Scope and authority

Reviewed the changed `BifrostLifecycle::flush` and state forwarding, Python and TypeScript configure composition, unchanged telemetry installer/provider owners, ASGI startup/drain, telemetry flush settlement, and affected unit/integration callers. Read root instructions, both SDK directory rules, skill instructions, workflow authority order, maintainer guide, the owning configure/telemetry architecture sections, and approved REQ-017/AC-016 plus service identity REQ-050 and its revision-17 telemetry contract. Prior tutorial scheduling/runtime evidence is carried forward as specified in `review-scope.md`; it is not independently rediscovered here.

The expansion into the unchanged Python installer is necessary: default `configure()` now calls that process-global owner, and its existing per-state serialization does not establish the newly composed setup's required isolation between states. No storage, authorization, or verification hot-path behavior was changed by these lifecycle operations.

## Scoped acceptance matrix

| Obligation | Implementation evidence at candidate | Verification evidence inspected | Result |
|---|---|---|---|
| Flush before a writer exists succeeds, including an in-flight start; closed still refuses | `observe/lifecycle.rs:130–137` snapshots the phase without holding its mutex across await; `state.rs:1081–1082` forwards to it | New `flush_drains_only_a_started_writer` covers NotStarted, Starting and Closed | PASS |
| Started flush retains all producer/sink settlement and does not close or replace its writer | Lifecycle clones the existing `Arc<StartedBifrost>` and calls the existing `Bifrost::flush`; `bifrost/facade.rs:824–825,853–862` preserves pooled writer drain | Existing observe writer tests and facade drain implementation; supplied client suite result is reported evidence only | PASS |
| Failed/abandoned startup releases its claim; shutdown during startup remains terminal; ambiguous started shutdown retains the same writer | Complete `StartClaim::{complete,drop}`, `BifrostLifecycle::{claim,started,shutdown}` bodies unchanged | Existing `failed_startup_may_be_retried`, `second_startup_is_refused`, `shutdown_during_start_stays_closed`, `ambiguous_shutdown_retries_the_same_batch_on_the_same_state` source | PASS |
| Python configure fixes context before installation, failed install remains frozen, and opt-out performs no telemetry installation | `src/state/mod.rs:342–351` calls the Rust owner before the conditional; native `state.rs:907–966` owns validation and OnceLock freeze | Candidate Python missing-extra/frozen test; integration configure calls explicitly opt out when application owns providers. Dirty opt-out unit test excluded | PASS |
| TypeScript configure is asynchronous with or without telemetry; freezes before dynamic import; repeated configure refuses | `wyrd/src/index.ts:3413–3425` native configure precedes conditional await; `#startTelemetry` uses frozen native client/environment | Candidate telemetry setup test checks resource and repeated frozen refusal; changed HTTP-auth caller awaits both opt-out configurations | PASS |
| Default configure uses existing managed telemetry owners rather than another provider pipeline | Python delegates through the existing native installer and `wyrd.otel._start_telemetry`; TS delegates to unchanged `otel.ts::startTelemetry` and stock exporter | Candidate default-setup tests assert installed resource values, refusal for existing providers and ownership | PASS |
| Concurrent states must refuse incompatible startup without corrupting another state's providers | Python native `telemetry_setup` protects one state only; `otel.py:348–380` has a multi-provider check/install race across states | Candidate `test_telemetry_setup` tests overlap on the same state and a single-signal simulated lost race, but does not assert all three globals remain usable under cross-state overlap | FAIL (INV-001) |
| Normal existing application-provider refusal leaves it intact, and state drains never flush application-owned providers | Python checks all three providers before setup; TS uses atomic `setGlobalTracerProvider`; wrappers retain only handles they installed | Candidate Python application/app-meter cases, TS application-provider test and owned-provider correlation tests | PASS, subject to concurrent Python finding |
| ASGI telemetry option retains its separate approved lifecycle and configured states can reuse their own installation | `_middleware.py::_start` starts the writer, conditionally calls private `_start_telemetry`, and on install failure drains before rethrowing; native installer remains same-state idempotent | ASGI integration failure case now targets private method; Python repeat and overlap tests exercise internal installer | PASS |
| Telemetry failures do not bypass native drain; primary/secondary errors retain stable catalog behavior and bounded flush | Complete Python `flush`, `shutdown`, `flush_telemetry`, `_ManagedTelemetry` and `TelemetryFlushFailure::settle`; complete TS `#flushTelemetry`, `#settle`, `flush`, `shutdown` | Updated managed lifecycle cases, Python real deadline/stale-flush tests; unchanged settle combines provider error and native error code | PASS |
| Public Python/TS explicit startup is removed while Rust setup remains separate | Python exposed name is `_start_telemetry`; TypeScript uses private `#startTelemetry`; no new public provider pipeline | Changed callers use configure, with opt-out where they need independent native state contexts; Rust source setup unchanged | PASS |

## Proposed finding

### INV-001 — Concurrent Python states can publish a split set of global providers, then shut all installed providers down

Classification: **INCORRECT**. Violated obligation: approved service-identity-and-telemetry revision 17, “Automatic startup must not silently reuse a provider bound to another state or replace an application-owned provider. Refuse incompatible startup”; the managed setup must keep the installed providers owned by the successful state so that its flush drains them. Positioning REQ-017 makes this installer the default configure operation. The consequence is loss of process telemetry, not merely a duplicate setup attempt.

Locations: new entry at `sdks/wyrd-sdk-python/src/state/mod.rs:342–350`; existing shared producer at `sdks/wyrd-sdk-python/python/wyrd/otel.py:348–380`; per-state gate at native `src/state/mod.rs:234–237`.

Source proof and reachable path:

1. Two separately loaded Python states can call `configure()` from different threads (ASGI also runs its setup with `asyncio.to_thread`). They hold different `telemetry_setup` mutexes. The default configure now enters the existing installer for both.
2. Both installers can observe proxy providers at lines 348–353 before either publishes. Provider/exporter construction happens after that check and includes SDK worker setup; there is no process-wide setup lock.
3. A valid interleaving is A installs tracer; B's tracer install is refused; B installs meter; A's meter install is refused; A installs logger; B's logger install is refused. The setters are separate calls at lines 367–369, with no common ownership claim.
4. Each caller's `installed` conjunction is false. Both execute lines 378–379 and shut down their own tracer, meter and logger. The global tracer and logger point to A's now-closed objects; the global meter points to B's now-closed object. Neither state receives a `_ManagedTelemetry` handle because both return `None` and raise provider-exists. Their later flush/shutdown has no managed provider to drain. Standard application instrumentation subsequently resolves these closed globals and cannot export normally.

This is a targeted gap in the newly composed default setup, rather than a reopening of unrelated old telemetry debt: two supported state startups can damage the entire process's signal providers, including sibling consumers. The existing same-state overlap test cannot exercise it because the two calls share one mutex. The existing lost-race case checks the tracer identity/release count only; it does not prove atomic ownership of all three signals.

Smallest safe correction boundary: keep the existing installer and stock providers/exporters, but serialize its shared process-global check/create/install/publication transaction across Wyrd states. Preserve the native per-state idempotence, configuration freeze, ASGI opt-out and application-provider refusal. Do not add another provider owner, retry protocol, or application-global reset/rollback mechanism. The existing `_INSTALL_LOCK` protects correlation-processor registration and is acquired inside this transaction, so blindly reusing that non-reentrant lock around the whole installer would deadlock; preserve that distinct invariant when choosing the minimal setup synchronization.

Focused proof: force two different states' configure calls to overlap before publication; assert one succeeds, one receives provider-exists, all three global providers belong to the successful installation and none of those global providers was shut down by the refused attempt; the successful state's flush must reach all three. Use the existing Python telemetry unit fixtures and synchronization primitives; no server or new harness is necessary. Existing normal application-provider refusal and same-state installer tests must keep their behavior.

## Existing-owner and burden assessment

The new Rust flush method belongs to the existing phase owner and avoids creating a second lifecycle. It delegates the started case unchanged to the existing Bifrost pooled writer. Python and TS configure similarly compose their established context and telemetry owners rather than duplicating them. Removing the telemetry composition would violate the approved outcome. INV-001 requires only synchronization at the existing global producer; downstream flush guards cannot repair split ownership, and native per-state locking cannot serialize different states. Broader changes to global OpenTelemetry behavior are not prescribed.

No other material lifecycle finding is proposed. Shutdown/setup overlap involving callers that do not wait for configuration completion was considered; no new guarantee beyond the approved setup contract was inferred. Existing provider lifetimes intentionally remain installed and usable after a state's shutdown.

## Verification limits

Static source review only; no Cargo, package checks, benchmarks or tests run by this reviewer, as assigned. Supplied results remain reported evidence; orchestrator owns fresh checks. Dirty Python/TS opt-out tests and unrelated working files were excluded. This report does not decide other specialists' boundaries or the final task verdict.
