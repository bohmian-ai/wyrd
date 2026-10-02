# System-resilience review: TASK-009

Immutable subject: base `7d96c30066425e0cde2290842d5801307843283d`, candidate `e4f30547906b8046bdccbfd38dff0eb0ee475d06`.

## Review Findings

### Critical

None.

### Important

- **SYS-001 — [`sdks/wyrd-sdk-python/python/wyrd/otel.py:294`](../../../../../sdks/wyrd-sdk-python/python/wyrd/otel.py) — a detach failure leaves stale Run correlation active after the scope exits.** REQ-151 requires exit to restore the prior execution-local context and requires optional detach failures to become no enrichment. `_exit_run` removes its bookkeeping entry before calling `context.detach`, then swallows any detach failure without clearing or superseding the OpenTelemetry context that `_enter_run` attached. The candidate's own failure test confirms the resulting leak at [`test_observe_surface.py:419`](../../../../../sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py): it runs the failure inside a copied context solely to prevent the attached scope from contaminating later tests. In an application, later framework spans on the same execution context can therefore be exported with a stale `wyrd.card_ref` and `wyrd.run_id`. Under the same signed Card scope they become durably misattributed Bifrost evidence; under a different or narrower authenticated publisher they can cause OTLP admission to reject the affected request rather than merely disabling enrichment. Restore or neutralize the scoped correlation even when detach fails, while still allowing the user exception and unrelated application work to continue. Add a focused test that injects detach failure, then creates a span after exit in the **same** context and proves it has no Wyrd correlation (and a nested variant proves the outer correlation is restored).

### Suggestions

None.

## Open Questions

None.

## Deployed-path evidence

| Changed runtime path | Process/service topology | Failure propagation and recovery |
|---|---|---|
| `WyrdState::run_for_card` and `Run::for_card` | Pure local shared-Rust state projected into Rust, Python, and TypeScript SDK processes; no server Run resource or network call | Unknown aliases fail locally before a Run is opened. Run views retain one UUIDv7 invocation identity and immutable Card selection. A client-process restart loses only this local invocation value; a new process creates a new Run ID as intended. |
| Python `Run.__enter__` / `__exit__` | PyO3 delegates to `wyrd.otel` in the caller's Python process. A process-global tracer provider owns the registered processor; `ContextVar` and OpenTelemetry context carry execution-local scope | Missing OpenTelemetry, unsupported providers, registration failure, lookup failure, and span-processor failure are contained to enrichment. User exceptions and asyncio cancellation still pass through because `__exit__` returns `False`. Normal and nested detach restore correctly, but injected detach failure leaks correlation as SYS-001 describes. |
| `_RunCorrelationProcessor.on_start` | Runs synchronously in every instrumented span-start path of each registered provider in that Python process | Its complete path catches provider/context/span errors, so telemetry failure does not crash the process or block explicit Wyrd observations. Registration is locked and weakly tracked per provider, preventing duplicate processors in the tested provider lifecycle. Provider shutdown calls the processor's no-op lifecycle hooks and does not own or close Wyrd/Bifrost. |
| Stock OTLP/HTTP exporter to `POST /v1/traces` | Python `BatchSpanProcessor` buffers in the SDK process and sends authenticated trace batches to `wyrd-server`; Gate authorizes record CardRefs and Scribe/Bifrost persists accepted rows | Export failure or abrupt client-process loss can lose buffered spans, which is consistent with the approved optional telemetry and explicit export-loss model. It does not stop the Wyrd server or explicit observation writer. Graceful proof calls `provider.force_flush()` and later `provider.shutdown()` explicitly; Run exit does neither. |
| Explicit `run.observe.*` writer | The state-owned `wyrd_client::Bifrost` facade uses its existing bounded queue independently of Python OpenTelemetry; `WyrdState.shutdown()` drains it | Optional telemetry failures do not enter this writer path. Queue/admission errors remain visible. Abrupt process loss may lose queued rows; graceful shutdown remains the explicit durability barrier and is exercised before server publication/query. |
| Persisted correlation journey | One real Python SDK process drives a real `WyrdTestServer`, authenticated OTLP HTTP ingest, the existing observation writer, server publication, and public Bifrost query | The journey proves healthy-path trace/custom/Eval joins after explicit tracer flush, state shutdown, and server publication. It also verifies the authenticated publisher and server-resolved Card UID rather than trusting client identity. |

## Failure-path and affected-capability assessment

- **Missing or API-only OpenTelemetry:** Run construction, Card lookup, application execution, and explicit Drift/Eval/custom observations remain available; only ambient span correlation stops.
- **Provider registration or processor/enrichment failure:** failures are synchronous and caught at the optional Python boundary, so they do not crash the shared application process or take unrelated SDK capabilities offline.
- **Exceptions and cancellation:** the synchronous context manager always returns `False`; ordinary Python exceptions, including cancellation delivered through context-manager unwinding, are not suppressed. No server or writer lifecycle is coupled to exit.
- **Dependency or OTLP outage:** the stock exporter owns its buffering/retry behavior. Trace availability degrades independently; explicit Wyrd observation admission and shutdown remain separate. The candidate does not add a second Wyrd queue or a health/readiness dependency on OTLP success.
- **Process restart or abrupt termination:** in-process OpenTelemetry batches and Bifrost queue contents may be lost before their explicit flush/shutdown barriers. This is accurately represented and not converted into synthetic evidence or a durability acknowledgement.
- **Detach failure:** the application stays up, but correlation does not safely recover; SYS-001 can contaminate later spans and persisted evidence in the same process.
- **Server-side scope enforcement:** OTLP Card identity remains a client assertion authorized against the signed scope, while tenant, publisher, and Card UID remain server-derived. No new client-trusted identity or tenancy path was introduced.

## Recovery and proof assessment

The healthy runtime path has production-shaped proof: a private provider is explicitly registered, exports through the authenticated HTTP endpoint, is force-flushed, the independent Bifrost writer is shut down, server publication completes, and durable query assertions join all three signals. Unit coverage also exercises normal nesting, concurrent asyncio tasks, task-context inheritance, missing packages, unsupported providers, registration failure, lookup failure, user exceptions, and idempotent registration.

The detach-failure proof is not credible closure of the required recovery behavior: it acknowledges the leak and quarantines it in `contextvars.copy_context()` rather than demonstrating same-context restoration. No additional commands were run for this review; the task's recorded green commands were treated as supplied evidence, while source and the cumulative diff were reviewed directly. No overlapping Cargo command was started.

## Overall result

**FAIL** — SYS-001 leaves a reachable recovery path that can misattribute later telemetry after a best-effort integration failure. The rest of the changed deployment and lifecycle paths preserve unrelated application, SDK, server, and explicit-observation availability.
