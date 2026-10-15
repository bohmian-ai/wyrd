# Focused overlap follow-up

**RESOLVED.** Both Python paths are reachable under the approved contract: (1) a later managed lifecycle call can receive success from an earlier flush that did not cover its already-completed spans; (2) overlapping setup on one Python state can reject that same state's provider. The TypeScript setup correction is effective. These conclusions do not require stronger arbitrary concurrent-write durability, provider cancellation, or lifecycle linearizability guarantees.

Subject: original base `dac4c340f835676adc29f3dcc01f98e1d3d51465`, immutable candidate `295446005b18604030bf376de6209098a28886f7`. HEAD was checked against that candidate. Only this report was written. No stable finding IDs are assigned here.

## Scope and authority

This follow-up investigated the conflict between behavior/OTEL/durability reports' joined-success concern and system/concurrency reports' lifecycle closure, and invariant review's Python setup proposal. Read subject, original approved revision-2 spec and TASK-001, prior validation and R1 correction/evidence, current conflicting sections, review skill, repository/agent rules, reference router, spec-driven-development, maintainer style, telemetry-observations, PyO3-boundaries and Python API references. Consulted CodeGraph before code navigation, then expanded relevant owners and sibling callers. This is an uncertainty-resolution pass, not a new full task audit.

The relevant approved requirements are explicit:

- REQ-012/AC-008: repeated managed setup on the same state is idempotent; another state/application's provider is refused and preserved.
- REQ-013/AC-009: each Python/TS state flush attempts a bounded managed-provider flush of completed spans before native drain; shutdown has the same ordering; timeout/failure must not report success; active spans and global provider survive.
- `architecture/wyrd-design.md:681-685` retains force-flush-before-drain. No authority there permits replacing a later invocation with an obsolete successful provider operation.
- The original R1 correction requires bounded waiting and bounded in-flight work; it does not amend REQ-013. The appended implementation evidence's “later calls ... reports that attempt's outcome” / “Spans ended after that attempt began ... next flush” is a limitation claim at a lower authority level, not approved new public semantics.

The [OTEL Trace SDK specification](https://opentelemetry.io/docs/specs/otel/trace/sdk/#forceflush) confirms the ordinary interpretation: provider ForceFlush invokes its processors; processor ForceFlush covers previously received span events, prioritizes the timeout when necessary, and supports concurrent calls. This does not promise successful delivery after exporter rejection, nor export of spans that end only after the current call. Those are different claims and are not required here.

## Complete sibling-path map for the disputed authority

| Surface | Owner and inspected behavior | Effect on the conflict |
|---|---|---|
| Python public `WyrdState.start_telemetry` | `sdks/wyrd-sdk-python/src/state/mod.rs:183-203`, field `:92-94`; `otel.py:257-289` | One shared `&self` method checks OnceLock, calls Python construction/registration, and publishes afterward. This is not one atomic initialization. |
| Python public `WyrdState.flush` / `shutdown` | Same binding `:278-281`, `:300-303`, `flush_telemetry:508-525` | Both use the same retained handle's force_flush result; both always attempt native drain. Truthy old success bypasses telemetry error projection in both paths. |
| Python managed operation | `python/wyrd/otel.py:216-254` | `_ManagedTelemetry.force_flush` creates a Future/daemon worker only when the previous Future is absent/done; otherwise it waits for and returns the older operation's result. There is no separate `join` method in candidate source. |
| Python retained-reference siblings | `state/mod.rs:467-498` GC traversal/clear; construction initializes OnceLock | No alternate initializer or lifecycle exporter repairs either race. GC clear requires mutable access and is not a normal competing startup writer. Any initialization-owner correction must preserve traversal and clear. |
| TS public `WyrdState.startTelemetry` | `wyrd/src/index.ts:2924-2943`; internal installer `src/otel.ts:96-107` | Post-import recheck is followed by synchronous install and assignment with no await. Same-state continuations cannot both install. Other owners still refuse/clean up. |
| TS public `WyrdState.flush` / `shutdown` | `index.ts:2727-2745,2958-2995` | Each invocation calls the retained provider's forceFlush anew, then native lifecycle and shared error settlement; no Wyrd old-attempt success cache. |
| Stock TS batching | Installed `@opentelemetry/sdk-trace` 2.11.0 `TracerProvider.js:forceFlush`, `export/BatchSpanProcessorBase.js:48-51,104-147` | Provider invokes processors per call; batch forceFlush starts `_flushAll`, synchronously removes the current finished-span batches before asynchronous export completion. A later completed span is not substituted with an old Wyrd Future. |
| Application-owned Python/TS provider | `install_run_correlation`, `runCorrelationProcessor`, `span_exporter`/`spanExporter` | States retain no handle to these providers. Applications directly flush them; neither Python state issue warrants adopting or changing their lifecycle. |
| Python/TS Agents bridge flush/shutdown | `otel.py:387-398`; `index.ts:2582-2595` | Bridge flush deliberately does nothing; shutdown ends retained unfinished spans only. Both defer provider export to state/application, so they neither repair nor independently cause these state-owner defects. |
| Shared native lifecycle/error settlement | `crates/shared/wyrd-client/src/state.rs:689-705,2090-2164`; `observe/lifecycle.rs:113-150`; Python `src/trace.rs:91-99` | Native flush drains native writer; shutdown closes it under existing semantics. Native cannot drain separate Python OTEL queue. Shared settlement preserves telemetry failure precedence/native secondary code, but cannot detect an invalid True. |
| Rust telemetry sibling | `sdks/wyrd-sdk-rust/src/otel.rs::start_telemetry`, `Telemetry::shutdown`; shared Rust state lifecycle | Separate pre-existing Rust process subscriber/flush owner, unchanged by this correction. It does not use Python `_ManagedTelemetry`, PyWyrdState publication, or the TS async installer. No new Rust setup/lifecycle contract is proposed. |
| Public declarations and tests | Python `state/__init__.pyi:196-270`; Python `test_observe_surface.py:650-681,846-894`; TS setup/lifecycle tests and live Python/TS framework journeys | Stubs expose these same synchronous Python state operations. Python repeat setup test is sequential. Held-export deadline test flushes repeatedly while the first export is blocked, releases it, then fresh-flushes; it does not select the final-drain-to-Future-settlement gap. Live journeys prove ordinary per-call export and continuation, not obsolete joined success. |

Pinned Python dependency sources inspected: OTEL SDK 1.42.1 in the existing Python environment, `sdk/trace/__init__.py:150-219,1458-1477`, `sdk/trace/export/__init__.py::BatchSpanProcessor.force_flush`, and `sdk/_shared_internal/__init__.py:128-140,167-216,240-246`; PyO3 0.28.3 cached `src/pycell.rs` shared borrow semantics and `src/sync/once_lock.rs`. PyO3 shared borrowing permits overlapping `&self` calls; the GIL is not a method-duration lock when invoked Python code releases it. Thread startup/imports and ordinary Python scheduler transitions provide legitimate scheduling windows.

## FUP-OVERLAP-01 — joined-success regression

**Source-local proposal: retain a material correction at the Python managed flush owner.**

Exact source: `sdks/wyrd-sdk-python/python/wyrd/otel.py:236-252`; consumers are both public Python lifecycle methods above. The lock correctly bounds worker creation and Future identity; it does not associate a successful operation with every joining caller's completed-span set.

Pinned stock batch `_export(EXPORT_ALL)` takes its export lock and loops until its queue no longer needs draining. Thus, spans ending while an exporter is held will ordinarily be drained by that same operation, and the existing blocked-export proof is meaningful. The decisive reachable interval is later: after the final empty-queue check/provider force_flush return but before `_flush` calls `attempt.set_result`. During this interval:

1. Another thread ends a span, synchronously enqueueing its OnEnd event.
2. That thread or another thread calls the same state's flush/shutdown. The Future is still pending, so the call reuses it.
3. The first worker publishes True, waking both calls. No provider flush has covered the newer span. Native drain cannot cover it either.

This can happen during ordinary overlapping calls without an outage, or during a recovery call joining an earlier timed-out operation. Success may leave pre-invocation completed spans buffered until a timer/third flush. A finite job exiting after successful state shutdown can lose that evidence. This is not a demand to export spans ending after the later call began.

**Fresh probe:** `mise exec -- sdks/wyrd-sdk-python/.venv/bin/python -B -` ran a tiny read-only script. It AST-extracted the exact candidate class without modification; supplied existing threading/Future imports and a 2000 ms probe bound; used real TracerProvider, BatchSpanProcessor (60-second schedule delay), and InMemorySpanExporter. `threading.setprofile` paused only the real provider force_flush return on the managed worker. It ended a second span before launching a later force_flush caller, observed that caller entering Future.result, then released settlement. Result, exit 0:

```text
{'results': [('first', True), ('later', True)], 'exported': ['first'], 'queued': 1}
```

The missing span was named `completed-before-later-call`. Cleanup released the event, joined both caller threads, disabled profiling, explicitly flushed the provider and shut it down. No source mutation, fake provider outcome, install, build or test lane was used.

**Conflict resolution:** concurrency/system evidence establishes bounded wait, one worker, unchanged correlation, and no cancellation guarantee. Those properties remain true. It does not establish that an old success covers a later call's already-completed span; rejecting this proposal as “new per-caller snapshot semantics” overlooks the existing ordinary ForceFlush contract and REQ-013. The standard allows honest timeout rather than completion, not false successful completion of an unrelated earlier barrier. The original unbounded-wait cause is fixed; this regression is introduced by R1 operation coalescing at the same owner. Independent validation decides stable-ID consolidation.

**Narrow correction constraints:** keep one managed owner and bounded in-flight work, stock batching/export, synchronous instrumentation/correlation, existing bound and scrubbed errors, native drain regardless of failure, and usable global provider. A caller joining an operation begun before its invocation must not treat that operation's success alone as its own barrier: after that work completes, obtain a provider flush covering this call or return the existing timeout within the same total deadline. Do not restart a full timeout after waiting, spawn an unbounded worker set, add export/retry queues, create public timeout controls, or impose serialization on native Bifrost/other services. A runtime-local controlled-return-barrier regression assertion is sufficient focused proof; broad new journey infrastructure is unnecessary.

## FUP-OVERLAP-02 — same-state Python setup rejects itself

**Source-local proposal: retain invariant review's Python initialization finding.**

Exact source: `sdks/wyrd-sdk-python/src/state/mod.rs:183-203` plus `_start_telemetry:277-289`. Two public calls can both observe an empty state cell before either publishes. Each enters provider construction. OTEL's once-only global install admits one and correctly refuses the other; the losing installer shuts down its own candidate and returns None. The binding converts None into 409 before the winning state's retained value can make this call idempotent. Even if the winner publishes first, the losing caller does not recheck the cell before raising. OnceLock protects final storage, not initialization side effects.

**Fresh probe:** a separate `mise exec -- sdks/wyrd-sdk-python/.venv/bin/python -B -` subprocess loaded the actual public WyrdState over the checked-in complete bundle, existing Tiny interfaces and an explicit offline client. A `threading.setprofile` call hook used a Barrier at real TracerProvider.__init__ entry to select the legitimate interleaving after each helper's global-provider check. Two ThreadPoolExecutor workers called public `state.start_telemetry()`; no installer/provider result or binding was replaced. Result, exit 0:

```text
Overriding of current TracerProvider is not allowed
['WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS', 'ok']
```

Cleanup disabled profiling, joined the executor, and shut down the installed provider. This confirms actual PyO3 shared-state reachability, not just a Python imitation of the Rust cell.

**Root cause and correction constraints:** same approved idempotence obligation as the prior TS race, separate runtime owner and mechanism. TS's post-await recheck closes its race because installation/publication thereafter cannot suspend; copying only that check into Python does not close Python construction's further release windows. Serialize fallible initialization/publication at the existing per-state retained owner using the already-installed PyO3 `PyOnceLock::get_or_try_init` mechanism, which detaches blocked interpreter waiters and leaves failures retryable (`pyo3-0.28.3/src/sync/once_lock.rs:6-15,76-85`). Preserve traversal/clear and lifecycle readers, other-state/application refusal, loser-resource cleanup and unchanged optional-import/credential errors. Avoid holding an ordinary blocking Rust mutex while Python code may need the GIL, and avoid a global identity registry/provider adoption. Focused public-runtime proof should make two same-state starts overlap, require both success and one provider, then retain other-owner refusal and failed-attempt retry cases.

The two Python proposals share a general need to distinguish operation identity, but not a single lock or durable authority: startup owns one-time state initialization, while flushing owns each invocation's completed-span barrier. Consolidating them into one global lifecycle coordinator would widen scope. The validator should group prior evidence by those two actual correction boundaries and preserve all sibling paths listed above.

## Limits

No source edit, build, installation, test lane, broad verification, server/service probe or credentials were used. Both isolated scheduling probes used existing dependency/runtime installations, cleaned controlled work, and created no probe files or bytecode. Tracked candidate remained immutable; no additional general start-vs-flush, shutdown-vs-write or detached-thread contract is proposed.

Skipped broad re-verification. Risk: both demonstrated Python races remain in the immutable candidate pending independent validation and correction.
