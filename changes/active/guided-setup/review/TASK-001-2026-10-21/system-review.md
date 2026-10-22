# TASK-001 system resilience review

Result: **FAIL**

Subject: base `ebd75dc224854413d17cb06b70da8181b78cde6b` to candidate `32fbc264adccaf1160894769d5501fbe441259a5`. HEAD remained the candidate during inspection. Only this report was written; candidate source was not modified. No other discovery report was read.

## Authority and review scope

Read the approved revision-5 specification, original task and appended evidence, root and touched Python/TypeScript/docs AGENTS, task-review skill, change workflow, maintainer guidance, architecture authority map, and applicable client/framework, telemetry, Bifrost system-boundary, and local-release lifecycle sections. Used CodeGraph first; its two queries did not locate the new Python module, so source inspection followed. Reviewed the cumulative changed runtime paths and their tests, documentation/example projections, manifests, and schema changes, rather than just the final commit.

The resilience obligations are REQ-001/INV-008 (bootstrap lifecycle preservation), REQ-005–006 (read-only prompt), REQ-021–023/AC-014 (startup failure and final drain), REQ-024–025/AC-012 (per-call context), and REQ-026/INV-009/AC-013 (worker-local lifecycle and portability). TASK-002 credential semantics remain outside this candidate.

## Deployed paths and failure coverage

| Deployed path | Owner and dependencies inspected | Failure and recovery assessment |
|---|---|---|
| Native/Python/Node installed CLI → `init` → installed server + UI | `cli.rs::Cli::dispatch`; `init.rs::InitArgs::run`, `DevServer::{run,command,ready,exited,stop}`, `settle`, `interruption`; Node `src/cli.ts` signal forwarding; global configuration owner | Actual rename diff retains foreground ownership. Signal listener registers before input resolution; cancellation after constructing the owner settles both children; child exit cancels gateway/host offers; `stop` attempts both child drains. `settle` forwards TERM, waits 45 seconds, then kills/reaps. Durable setup and saved identity survive retry; no new provisioning or child ownership path was introduced. Readiness/host prompt printing does not execute a coding agent. |
| Offline prompt and post-readiness handoff | `InitArgs::run` early return; shared `wyrd_spec::mcp::setup_prompt`; `McpInstallReport::print_handoff`; host launch quoting | Offline path returns before installer/config/network access. Host command construction is output only and preserves literal shell arguments. Partial failed host outcomes still pass through the existing report refusal boundary. No new supervisor or automatic restart exists. |
| MCP client → stdio proxy → authenticated shared server | CLI `McpProxy::{list_prompts,get_prompt}`, `upstream_error`; server `WyrdMcpHandler` and `guide::{prompt,prompt_descriptor}` | New server operations return static release text; invalid name/arguments produce request errors. Proxy forwards upstream protocol refusal and maps transport outage into request error. No change to process supervision, durable state, tool failure propagation, or the shared server's admission/drain graph. A failed prompt request does not take unrelated capabilities offline. |
| ASGI worker lifespan → state writer → telemetry → wrapped app | `_middleware.py::{_start,_lifespan,_drain}`; Python native `state/mod.rs::{start_bifrost,start_telemetry,shutdown}`; Rust `WyrdState` and `observe/lifecycle.rs::{claim,complete,shutdown}` | Startup invokes the existing state owners off the event loop. A writer refusal sends startup.failed without entering the app. Telemetry failure attempts existing shutdown and preserves the primary exception. Native startup claim releases on failure; successful shutdown is terminal and failed drain retains the same writer for retry. Application protocol startup.failed and shutdown messages attempt final drain. **An application exception after startup.complete skips drain: SYSTEM-001.** |
| HTTP/WebSocket ASGI exchange → Run → evidence/correlation | `WyrdMiddleware.__call__`; Python native `Run::{__enter__,__exit__}`; `otel.py::{_enter_run,_exit_run,_RunCorrelationProcessor}` | Alias resolves before the app is invoked. `with run` covers awaiting the whole exchange, including streaming and synchronous framework work; exception/cancellation executes Run exit and propagates. Run exit itself is not a durability barrier. Mirrored ContextVar follows Python task/thread context propagation and is popped alongside OTel scope; no per-request registry, writer, or new persistence is introduced. |
| Gunicorn parent → Uvicorn workers → per-worker app/module/lifespan | `tests/integration/asgi_app.py`, worker subprocess journey, published FastAPI example/guide | Tested command omits preload: module/state loads in each forked worker; middleware constructs no running resources, starts them in worker lifespan, and drains on normal shutdown messages. Worker failure is contained to its process; other workers and Wyrd server retain their own resources. A new worker reloads the local bundle and starts a new state; successful old-state shutdown is terminal. Guide expressly excludes hard kill/deadline/failed-drain delivery. |
| Published docs landing and setup projections | `+page.server.ts`, mdsvex highlighting, generated llms text, example imports | Raw Quickstart source now comes from Vite build input rather than locating repository files in a deployed server chunk. This removes the reported ENOENT runtime dependency. No background activity or persistent-data migration is introduced. |
| Shared `Spec` schema projection | `envelope.rs::JsonSchema for Spec` and MCP output consumer journey | `anyOf` permits overlapping typed spec variants; no execution, queue, state, transport, or recovery owner changes. It does not alter native schema-specific validation or the server lifecycle. |

## Material finding

### SYSTEM-001 — Final drain is skipped when the wrapped ASGI app raises during shutdown

Classification: **INCORRECT**. Violated obligations: REQ-021 (generic ASGI support), REQ-023 and AC-014 (application shutdown precedes final Wyrd drain; application failures remain failures).

Location: `sdks/wyrd-sdk-python/python/wyrd/integrations/asgi/_middleware.py:144–146` (`except Exception` → `if replied: raise`), with drain limited to the protocol-message callback at lines 130–139.

Reachable path:

1. The middleware starts Bifrost; a wrapped ASGI app reads startup and sends `lifespan.startup.complete`, setting `replied=True`.
2. On graceful shutdown the app receives `lifespan.shutdown`, records final evidence, and raises an application/dependency exception before sending its own shutdown response.
3. The `replied` branch immediately re-raises. `_drain` and `state.shutdown` are never called.
4. Uvicorn's installed `LifespanOn.main` catches that exception, marks a lifecycle error, and releases `shutdown_event` in its `finally`. The worker shutdown therefore completes its failure path and can exit with buffered Wyrd rows/spans never offered to the existing bounded drain.

This is a missing drain attempt during ordinary application cleanup failure, not a guarantee of delivery on hard kill, deadline, or sink refusal. The failure boundary should remain this ASGI worker's lifecycle; no change to the shared Wyrd server's availability is needed.

Proof: executed a read-only probe through the candidate's public `WyrdMiddleware` import (confirmed `__file__` resolves to the candidate source). A minimal state recorded its lifecycle calls; a generic ASGI app sent startup.complete, received shutdown, then raised. The direct adapter probe returned `state_events ['start']`, sent only startup.complete, and propagated the original RuntimeError. A second probe used installed Uvicorn's real `Config`/`LifespanOn` (no listener/database) and observed:

```text
{'state_events': ['start', 'final-evidence'], 'error_occurred': True,
 'shutdown_finished': True, 'should_exit': True}
```

There is no `shutdown` event in either result. The unchanged native owners establish the consequence: `Run.__exit__` only restores correlation, whereas `WyrdState.shutdown` is the explicit buffer drain/durability boundary.

Smallest testable correction: ensure the adapter invokes the existing off-loop state shutdown after the wrapped application's failing shutdown path as well as its protocol-message path, while preserving the primary application failure, existing failure diagnostics, and application-before-drain ordering. Keep state/writer/provider lifecycle in the existing owners, and avoid repeating shutdown after a protocol path already drained. Do not replace the writer, add retry/supervision machinery, or turn the app failure into shutdown.complete.

Focused closure proof: extend the existing lifespan journey with a generic wrapped ASGI application that sends startup.complete, receives shutdown, submits final evidence through the real state, then raises before a shutdown response. Assert the original application failure remains observable, the state reaches its existing closed status after successful drain, and the final evidence is queryable. Add the corresponding failing-drain case only as needed to show secondary drain failure does not hide the primary app error. Existing FastAPI protocol-failure and healthy-shutdown cases must continue to pass.

## Evidence assessment and limits

- Available task evidence claims C1–C8 and targeted format/lint/type/schema/docs checks pass on Linux x86_64. Source inspection confirms meaningful real-server assertions for persisted results, shutdown evidence, alias refusal, concurrent Run identity, instrumentation correlation, and two real Gunicorn worker PIDs. This review did not rerun Cargo or Postgres journeys.
- `test_asgi_lifespan` covers writer-already-started refusal, provider conflict/partial cleanup, FastAPI startup failure (which sends startup.failed), successful application-shutdown evidence, and sink refusal mapped into shutdown.failed. It does not cover a generic app raising after startup.complete. Its lifecycle test helper waits unboundedly for a shutdown reply, so that branch needs a driver that also observes task termination, as the startup helper already does.
- C8 exercises two workers and final row delivery with `telemetry=False`; C7 proves default provider startup/export/drain in a single process. Worker-local default telemetry is supported by startup placement in source but not directly exercised together in the supplied multiprocess test.
- Cancellation of request execution is covered and uses synchronous context-manager exit. Cancellation of lifespan itself, especially while `asyncio.to_thread(_start)` continues, is not exercised by the supplied tests; no delivery guarantee for deadline cancellation or hard kill is inferred. No separate speculative cancellation finding is proposed.
- Existing bounded native shutdown and retry-on-ambiguous-failure behavior remains owned below the adapter. The adapter should reuse it; a failed drain is a worker shutdown failure, never permission to crash a shared Wyrd service or report success.
- Full installed bootstrap regressions, combined SDK journeys, cross-task identity refusal integration, and full docs aggregate are expressly pending change review. macOS arm64 and Linux arm64 were not exercised; no new platform-specific runtime branch appears in this candidate.

Overall: **FAIL**, pending correction and focused proof for SYSTEM-001.
