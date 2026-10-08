# TASK-009 system-resilience review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `017a54d5390eb488e820f890bbb953a5f1ca3a53`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 45, especially REQ-123, REQ-151, INV-007, INV-012, and AC-032
- Task chain: original TASK-009 plus TASK-009-R1, TASK-009-R2, and TASK-009-R3

The candidate was still `017a54d5390eb488e820f890bbb953a5f1ca3a53` immediately before this report was written.

## Deployed-path evidence

| Runtime boundary | Source and behavior | System effect |
|---|---|---|
| Run selection | `crates/shared/wyrd-client/src/state.rs:489-510` resolves the hydrated alias locally before constructing `Run`; `crates/shared/wyrd-client/src/observe/mod.rs:47-113` keeps the UUIDv7 `RunId`, exact `CardRef`, and shared state-held writer in an immutable view. | No network request, server Run, process-global subject, or new service dependency is introduced. Rust, Python, and TypeScript project the same local owner. |
| Python scope boundary | `sdks/wyrd-sdk-python/src/observe/mod.rs:195-239` delegates entry and exit to `wyrd.otel`, returns the same Run, ignores only optional telemetry failures, and always returns `false` from `__exit__`. | Application execution and explicit observations remain available when correlation cannot be installed; user exceptions continue out of the block. |
| Execution-local correlation | `sdks/wyrd-sdk-python/python/wyrd/otel.py:189-212,279-326` owns a `ContextVar` stack of exact attach tokens and prior values. Key creation and provider registration share one lock; entry pushes one paired stack entry, and exit restores the preceding pair even when public detach raises or silently does nothing. | Nested scopes, copied asyncio contexts, and simultaneous users of the same immutable Run do not share a mutable token slot. Correlation state disappears with the Python process and is not a durable resource. |
| Span-provider integration | `sdks/wyrd-sdk-python/python/wyrd/otel.py:215-276` installs one stateless processor per weakly held provider and contains import, registration, lookup, and span mutation failures. Private providers use the same explicit hook as the journey. | The caller continues to own provider/exporter startup, flushing, and shutdown. Wyrd adds no provider singleton, exporter, retry loop, worker, or health dependency. |
| Explicit observation path | `sdks/wyrd-sdk-python/src/observe/mod.rs:125-153,288-330` reads an active Python span only as a best-effort Eval default and otherwise calls the existing Rust observation path; `crates/shared/wyrd-client/src/observe/mod.rs:117-178` retains the state-owned bounded writer and explicit shutdown barrier. | Broken ambient telemetry cannot suppress validation, authorization, queue-pressure, or writer errors. Custom, Drift, and Eval writes remain independent of OTLP exporter health. |
| Authenticated trace persistence | `sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py:365-402` uses the stock SDK and OTLP/HTTP exporter against authenticated `POST /v1/traces`; `:413-474` queries persisted Bifrost rows and proves run, Card, principal, server-resolved UID, and trace/span joins. | The candidate reuses the existing server listener, Gate authorization/projection, Scribe durability, and Oracle query paths; no server or Bifrost production source changed in this task. |
| Graceful completion | The journey at `sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py:550-563` explicitly orders provider `force_flush`, state `shutdown`, server publication, query, and provider shutdown. | Context exit is correctly not represented as trace-export, client-queue, Scribe-publication, or query-visibility acknowledgement. |

Affected capabilities are limited to local first-Card Run selection in all first-class SDKs and optional Python span correlation. Registry availability, unrelated server routes, and non-Python SDK observation behavior do not acquire a dependency on OpenTelemetry.

## Failure and recovery assessment

| Failure or interruption | What stops | What remains available | Recovery and proof |
|---|---|---|---|
| OpenTelemetry package missing, API-only provider, or unsupported provider | Automatic span enrichment only. | Run construction/selection, application code, and explicit observation validation and writes. | `test_missing_opentelemetry_is_a_no_op`, `test_unsupported_providers_are_refused_without_raising`, and `test_registration_and_attach_failures_never_block_observations` cover these paths. Provider registration returns `False`; no retry worker or service failure is created. |
| Provider registration or span enrichment raises | That registration attempt or the affected span's two optional attributes. | The request/process, later Run use, and explicit Wyrd observations. | `install_run_correlation` removes a failed registration mark so a later entry may retry; `_RunCorrelationProcessor.on_start` contains its complete path. Focused tests exercise a genuinely raising registration and a failing lookup while an explicit observation still reaches its ordinary writer boundary. |
| Attach fails | Ambient correlation for that entry. | Application work, explicit observations, and balanced exit. | `_enter_run` records a `None` stack entry and `_exit_run` consumes it. The focused failure test proves observation behavior remains fail-closed only at the ordinary Bifrost boundary. |
| Detach raises or is swallowed | The normal OTel reset operation. | The process and explicit writes. | `_exit_run` compares the live value with the recorded prior pair and reattaches that prior pair when necessary. `test_detach_failure_restores_the_prior_correlation` proves both nested restoration and final clearing, including observations after failed exits. |
| Nested scopes, concurrent tasks, same Run object, or task copied inside a scope | No capability stops. | Each execution context retains its own selected Card and shared invocation ID. | `test_nested_card_scopes_share_the_run_and_restore_the_outer_card`, `test_scope_survives_await_and_isolates_concurrent_tasks`, and `test_concurrent_tasks_entering_the_same_run_exit_independently` cover restoration, `await`, copied-task lifetime, distinct views, and interleaved exits of the identical Run object. The same-Run case uses events rather than timing. |
| Async cancellation or an application exception | The cancelled/user operation. | Cleanup and sibling tasks/process capabilities. | Python's synchronous context protocol invokes `__exit__` during block unwinding; the implementation ignores the exception triple after cleanup and returns `false`, so it cannot suppress cancellation or a user exception. The user-exception path is directly tested. Hard process termination needs no cross-process detach because the context is process-local. |
| OTLP endpoint/exporter outage or timeout | Trace delivery may fail or remain buffered in the caller-owned exporter. | Explicit custom/Eval/Drift emission and the rest of the Wyrd client remain separate. | The task intentionally does not make scope exit a barrier or take exporter lifecycle ownership. The caller observes `force_flush`/shutdown results and may apply its provider's policy. The candidate adds no retry or health loop that could amplify a server outage. |
| Client process exits before exporter flush or `WyrdState.shutdown` | Unflushed spans and queue-admitted observation rows may be lost. | Rows already accepted by the server retain the existing Scribe durability and publication behavior. | This is the explicit lifecycle ceiling, not a new recovery claim: provider flush and state shutdown are separate caller-owned barriers, and the journey exercises both before server publication. Restart creates a fresh local Run; there is deliberately no durable server Run to recover. |
| Server restart after accepted ingest | Local correlation production is unaffected; reads may wait for normal server recovery/publication. | Existing Bifrost WAL/publication recovery and tenant-authorized query behavior. | No Gate, Scribe, Oracle, or server lifecycle code changes in the cumulative diff. The journey's authenticated persisted read proves the healthy full path without claiming that Python scope state survives a server or client restart. |

No changed local failure path crashes the shared server or disables an unrelated capability. Optional telemetry fails at the span-enrichment boundary; validation, authorization, queue admission, and durable server behavior keep their existing fail-closed boundaries.

## Proof assessment

- Independent focused run: `mise exec -- uv run python -m pytest -q tests/unit/state/test_observe_surface.py` from `sdks/wyrd-sdk-python` completed with `35 passed` on the candidate.
- Candidate evidence records the authenticated Python journey passing after explicit tracer flush, state shutdown, and server publication, plus the broader Python unit/integration, typing, formatting, lint, codegen, PyO3-scope, shared-Rust, and diff checks.
- The focused suite directly covers the changed recovery semantics rather than inferring them from a healthy span. The persisted journey crosses the real SDK, authenticated HTTP endpoint, server identity resolution, storage publication, and public query client.
- No separate exporter-outage or process-kill journey is required by TASK-009. Those cases do not have changed recovery machinery: ownership and the acknowledged loss ceiling remain explicit, and adding retries, persistence, or provider lifecycle would violate the task's non-goals.

## Material proposed findings

None.

## Overall result

**PASS**

The cumulative candidate preserves failure isolation and recovery ownership across Python scope state, caller-owned telemetry, the state-owned Bifrost writer, and the existing server data plane. Its proofs cover every changed runtime failure boundary required by TASK-009 and the three remediation tasks without claiming scope exit or process-local context as a durability mechanism.
