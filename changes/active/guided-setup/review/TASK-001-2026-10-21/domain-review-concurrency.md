# Concurrency and lifecycle domain review

Result: **FAIL**.

Subject: base `ebd75dc224854413d17cb06b70da8181b78cde6b`, candidate `32fbc264adccaf1160894769d5501fbe441259a5`. HEAD still equals the candidate; the only working-tree additions are review reports. No candidate source was changed. No other discovery report was read.

## Boundary and authority coverage

| Boundary | Authority | Source and consumers inspected |
| --- | --- | --- |
| Startup, app lifespan, final drain, cancellation | Approved revision 5 REQ-021–023, AC-014; TASK-001 scenarios 4/7; Python SDK AGENTS; architecture/wyrd-design.md Client model and managed-telemetry ownership | Complete new ASGI middleware, exports/stub; PyWyrdState start_bifrost/start_telemetry/shutdown; shared WyrdState and observe/lifecycle.rs; ASGI lifespan tests, FastAPI example and integration guide |
| Request/WebSocket Run identity and context restoration | REQ-024–025, AC-012; architecture/wyrd-design.md Observation identity | Middleware __call__; native PyRun __enter__/__exit__, immutable Run views; changed otel.py stack/processor; isolation and instrumentation journeys; existing nested/concurrent/captured-context unit cases |
| Workers and process resource ownership | REQ-026, INV-009, AC-013 | Gunicorn command and asgi_app.py; loaded-state and ambient-client construction; worker journey, guide, lifecycle start and shutdown owners |
| Retained evidence and lifecycle reuse | Architecture Client model; Bifrost System boundary; root/SDK AGENTS; TESTING.md; maintainer style and spec-driven development | Shared state delegates to BifrostLifecycle, Python shutdown flushes only its managed provider before native drain; no alternate writer or queue was added |

The complete cumulative change range was inspected for materially changed concurrency/lifecycle surfaces, including the CLI move/handoff, MCP projections, installed CLI wrappers, documentation, test registration and worker dependencies. Detailed caller/source expansion followed the ASGI and Run boundaries above. CLI foreground orchestration remains the existing moved owner; new prompt reads do not create shared mutable runtime state. CodeGraph was used before locating code.

## Findings

### CONC-001 — Application lifespan errors after startup skip the final drain

- Classification: **INCORRECT**; blocking, retained-evidence/resource lifecycle consequence.
- Violated obligation: REQ-023 requires state shutdown after application shutdown, preserving application failures; TASK-001 scenario 4 requires the wrapped-app failure path and existing bounded lifecycle/cancellation semantics. This is an ordinary application teardown error, independent of the excluded hard-kill or shutdown-deadline guarantee.
- Exact location: `sdks/wyrd-sdk-python/python/wyrd/integrations/asgi/_middleware.py:143–147`, with the drain triggered only from `app_send` at lines 137–138.
- Producer-to-consumer evidence: `_start` successfully starts the state's writer. Sending `lifespan.startup.complete` sets `replied=True`. A wrapped ASGI app can then receive `lifespan.shutdown`, run its teardown hook and raise before sending a shutdown response. The exception handler immediately reraises because `replied` records any earlier response. No code calls `_drain` on that path. The existing native `WyrdState.shutdown`/`BifrostLifecycle.shutdown` owner is never reached, and the Python managed telemetry flush is skipped too. `CancelledError` also bypasses this `except Exception` without any terminal cleanup.
- Observable consequence: pending rows and final completed spans can be abandoned on an otherwise graceful server shutdown when application cleanup raises, and state remains started until process exit. Propagating the original application error correctly does not close the state. A generic ASGI wrapper is explicitly supported; the adapter cannot rely on Starlette translating every teardown exception into a failure message first.
- Read-only reproduction: importing the public middleware with a recording state double, use an app that reads startup, sends startup.complete, reads shutdown, then raises `RuntimeError("shutdown hook failed before sending response")`. Result: the original RuntimeError propagates and the state event list is `['start']`, with no `drain`. A second probe substituting `asyncio.CancelledError()` likewise records `['start']`. These probes exercise the candidate implementation, with no source edits or external services.
- Smallest correction boundary: retain `WyrdState.shutdown` and `_drain` as the existing owners; make successful middleware startup have terminal cleanup even when the app exits by exception/cancellation before its terminal reply. Preserve the primary application failure/cancellation, app-before-drain ordering, current explicit failure signaling and application-owned-provider behavior. Avoid a second drain after an already handled terminal reply. No new queue, supervisor, retry policy or durable semantics is needed.
- Focused closure proof: extend the existing lifespan journey with a wrapped ASGI app that sends startup.complete and, after receiving shutdown and submitting final evidence, raises before a reply. Assert the application failure remains observable, the state closes, and that evidence is queryable. Cover cancellation's context/cleanup branch with a small controlled in-process case; no hard-kill delivery claim is required. Existing successful/failure-message paths must still drain once and preserve their failures.

## Other reviewed paths

| Obligation | Evidence assessment | Result |
| --- | --- | --- |
| Startup reuses writer and telemetry owners; provider conflict cleans started writer | `_start` invokes native lifecycle and shuts down on telemetry setup exception. Native StartClaim rolls failed startup back, and shutdown prevents an in-flight start publishing after closure. Existing provider is not replaced. | PASS for ordinary exceptions; terminal cleanup gap above remains |
| Normal final drain stays off the event loop and follows app teardown | `_drain` uses asyncio.to_thread; terminal message interception awaits it before forwarding shutdown.complete. Python state flushes managed provider and attempts native drain even on flush failure. | PASS on terminal-message paths |
| HTTP/WebSocket Run isolation, streaming, exceptions and request cancellation | One state.run per call; existing synchronous context manager encloses the entire awaited app. Alias resolves before app execution; no per-request flush or registry. | PASS |
| OTel correlation survives instrumentation extraction | Mirrored ContextVar survives replacement of the OTel context; processor still prefers a captured nonempty parent scope. Both stacks pop only their matching top. Nested views and concurrent task inheritance keep existing behavior. | PASS |
| Worker-owned resources start after fork | Documented command imports application in each worker; test disables telemetry and starts each writer in lifespan. Example's offline from_path creates no running writer/provider before worker startup. No new cross-process sharing was introduced. | PASS by source; runtime coverage limit below |

The Run remains an immutable correlation view, not a server resource or drain boundary. Child tasks inherit their creation context under existing semantics, including child work that outlives the parent's lexical scope; this change does not introduce that behavior. A plain thread with an explicitly captured OTel parent still correlates because the original OTel stack remains supported. `asyncio.to_thread` copies execution context, and the lifecycle methods do not open request Runs. Cancellation of its await does not stop the worker thread; no hard-timeout delivery guarantee is inferred.

## Verification and limits

- Ran `mise exec -- uv run --no-sync --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py`: **68 passed in 59.42s**. This includes nested views, concurrent scopes, captured OTel context in plain threads, optional telemetry, and existing provider ownership behavior.
- Ran the two small public-middleware lifecycle probes described above; both reproduced missing shutdown invocation.
- Inspected all new ASGI journey bodies and appended C5–C8 evidence. Did not rerun Postgres/server journeys or Cargo commands in this independent review. The existing lifespan journey uses FastAPI's explicit failure-message path and therefore does not cover CONC-001.
- Recorded worker runtime proof is Linux x86_64 with two Gunicorn Uvicorn workers and `telemetry=False`; default managed telemetry per worker, macOS arm64 and Linux arm64 were not exercised by this review. No additional source-backed worker defect was found.
- No changes, optional refactors, new dependencies or speculative hardening are proposed. Risk remaining in the candidate: final evidence can be lost when wrapped application lifespan exits without its terminal response.
