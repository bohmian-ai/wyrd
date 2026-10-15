# OTEL / framework interoperability review

Result: **FAIL**. Base `dac4c340f835676adc29f3dcc01f98e1d3d51465`; candidate `295446005b18604030bf376de6209098a28886f7`.

## Boundary, authority, and method

Fresh cumulative audit of native framework interoperability, explicit Agents bridges, ordinary OTEL context/sampling/provider semantics, managed flush, runnable recipes, and actual framework proof. The latest diff was navigation, not the acceptance boundary. Prior findings were hypotheses. CodeGraph was used before source navigation; full bridge/lifecycle owners, callers, surrounding tests, and pinned framework producers were expanded independently. No other current discovery report was read.

Authority: approved spec r2 REQ-010–016, AC-001/007–012 and INV-002–004; original TASK-001; prior verdict/validation and R1 remediation; AGENTS.md, agent rules, reference router, spec-driven development, maintainer style, complete telemetry-observations and testing-workflows references; active design OTLP/client/provider, W3C context and Observation identity contracts; doctrine; docs CONTRIBUTING and PRODUCT. Primary standard checks: [OTEL Trace SDK ForceFlush](https://opentelemetry.io/docs/specs/otel/trace/sdk/#forceflush) and [GenAI span conventions](https://raw.githubusercontent.com/open-telemetry/semantic-conventions-genai/main/docs/gen-ai/gen-ai-spans.md). These corroborate standard provider/span operations; no new compatibility schema or stronger capture guarantee is imposed.

| Obligation / boundary | Source and proof coverage | Assessment |
| --- | --- | --- |
| Actual native instrumentation, no universal Wyrd runtime/adapter | Python `frameworks/app.py` builders execute actual Pydantic AI, ADK runners and OpenAI Agents on local model endpoints; TS journey uses actual Agents + Chat Completions model. Explicit bridge registration only. | PASS |
| Bridge hierarchy/timing/start identity and harmless lifecycle callbacks | Python `otel.py:340–495`; TS `index.ts:2493–2707`; callback tests and real framework journeys. Standard `start_span` / `startSpan` mint IDs; parent-map/root/enclosing fallback retains context; correlation is copied at start; final data is read at end; late/duplicate callbacks and interrupted shutdown are handled. | PASS |
| GenAI metadata without payload invention | Actual pinned `GenerationSpanData` / `ResponseSpanData`, TS `SpanData` and model producers compared with both mappers. Python now reads safe direct Responses usage, including zero, with response fallback. Unknown types keep ordinary span information. No model/provider identity is invented when absent. | PASS |
| Sampling/context and application provider ownership | Python correlation installation plus synchronous processor callbacks; TS `runCorrelationProcessor`, standard API context, construction-time processors; stock samplers/processors/exporters retained. Non-recording spans are never reconstructed. Provider conflict refuses and disposes of the losing candidate. | PASS |
| Managed lifecycle bound, ordered native drain, active spans/global provider retained | Python `_ManagedTelemetry`, binding `flush_telemetry` and public flush/shutdown; TS stock provider `forceFlush` and state settlement; deterministic lifecycle tests and new live journeys. Finite waiting is fixed, but joining an earlier flush can falsely acknowledge later completed spans: OTEL-R1-001. | FAIL |
| Runnable docs and version/content honesty | Observe page install table, concrete four framework procedures, ADK session/message construction, independent provider credentials, managed and owned setup, ambient authenticated readback, lifecycle errors; unchanged draft status. Pinned content controls inspected. | PASS |
| Real framework lifecycle/readback evidence | Python parent reads first model/tool spans plus native drift while child waits after public `state.flush`; second work proves writer continuation; held span ends afterward; post-shutdown work uses the same global provider. TS mirrors these phases. Server publication flush cannot flush the child SDK queue and does not replace the state barrier. | PASS for ordinary successful lifecycle; incomplete for joined-flush edge |
| Real edges and negative exports | Existing Python/TS journeys assert model/tool error status, streaming completion, nesting/concurrency, ancestry, exact Card/Run identity, no duplicates, sampling, content sentinels and denied/cross-tenant export. Gateway journeys use ambient credentials and actual shared stock-client transport. | PASS structurally; execution claims not independently rerun |

Pinned sources inspected in installed dependency trees and compared with lockfiles: Python OpenAI Agents 0.23.1 (`tracing/span_data.py`, `models/openai_responses.py`, usage producer), OTEL SDK 1.42.1 (`sdk/trace`, `sdk/_shared_internal`), Pydantic AI 2.56.0 (`models/instrumented.py`) and Google ADK 2.12.0 (`telemetry/tracing.py`, `telemetry/context.py`); TS Agents Core/OpenAI 0.20.0 (actual span data declarations and Chat Completions/Responses producers), OTEL 2.11.0 (`sdk-trace-base` shim and underlying `sdk-trace/TracerProvider`). TS Chat Completions success genuinely does not put returned usage on its generation span; no missing-success-usage finding is asserted. TS Responses only supplies `_response` when upstream tracing includes data; unlike Python, it has no separately supplied safe usage field to recover.

## Prior-finding closure

| Prior stable finding | Source-backed assessment |
| --- | --- |
| FIND-TASK-001-1 optional TS declaration dependency | Root bridge no longer imports Agents types. Narrow local structural callback fields match actual framework inputs; actual framework-enabled tests use `satisfies TracingProcessor` and `setTraceProcessors`. Clean package checker now compiles ordinary consumers without optional peers. Closed in this domain; package/check execution remains a recorded claim. |
| FIND-TASK-001-2 incomplete procedures | All four recipes now construct the agent/tool and all required ADK runner/session/message values; prerequisites install framework packages, and `answer` returns the run ID consumed by readback. Closed. |
| FIND-TASK-001-3 live flush/global continuation proof | New Python managed live scenario blocks its child after flush until parent readback; TS reads back immediately after state flush. Both produce second native/framework work, then post-state-shutdown framework export. Closed for the original ordinary lifecycle gap. |
| FIND-TASK-001-4 safe Responses counters | `otel.py:477–483` consumes actual `ResponseSpanData.usage` with per-counter response fallback; callback test uses actual `ResponseSpanData(response=None)` and late usage including zero. Closed. No new Responses HTTP harness is required for this pure mapper correction. |
| FIND-TASK-001-5 effective bounded wait | `_ManagedTelemetry` moves only stock provider flushing to a daemon and bounds the Future wait, leaving synchronous correlation/callbacks intact. Held real stock-exporter test proves timeout plus native-stage result and resumed export. Original unbounded wait fixed, but its new shared-attempt coordination has OTEL-R1-001 below. |
| FIND-TASK-001-6 overlapping same-state TS setup | Post-import state check precedes synchronous provider installation; a later same-state continuation returns successfully. Closed. |

## Material proposed finding

### OTEL-R1-001 — INCORRECT / REGRESSION: joining an older flush can acknowledge spans it never flushed

**Obligation:** REQ-013 and AC-009 require state flush to flush completed managed spans before native drain and report failure honestly. The R1 bounded-wait correction must preserve that existing barrier, including subsequent calls. This is a consequence of the new attempt sharing, not a request for a queue, retries, permanent provider shutdown, or capture beyond OTEL sampling.

**Exact location:** `sdks/wyrd-sdk-python/python/wyrd/otel.py:236–247`, especially `:237` and `:245`; settlement window at `:252`. Consumers are both `PyWyrdState::flush` and `shutdown` (`src/state/mod.rs:277–302`) through `flush_telemetry:506–524`.

**Producer-to-consumer evidence:** A new call creates no stock provider flush while `_attempt` is unfinished; it returns that older Future's outcome. In pinned OTEL 1.42.1, `BatchProcessor.force_flush` drains its queue synchronously and returns (`sdk/_shared_internal/__init__.py:239–246`); the default provider subsequently returns, and only afterward Wyrd marks the Future done. A scheduler switch in that interval permits a new completed span and a second lifecycle call. The second call joins the unfinished Future even though its new span was never seen by the prior provider flush. Future success is then treated as current-call OTEL success and the native stage proceeds. Native Bifrost drain does not flush the SDK span processor. The R1 evidence even describes the semantic deviation: later calls report the prior attempt's result, and spans ended after its beginning wait for another flush. That is not an approved weaker barrier.

**Independent isolated proof:** Ran the installed Python interpreter with actual `TracerProvider`, actual `BatchSpanProcessor` (60-second schedule delay), `InMemorySpanExporter`, and current `_ManagedTelemetry`. A `threading.settrace` return hook paused only the worker at the real provider `force_flush` return; no provider method or exporter behavior was replaced. After the first provider flush returned, the main thread completed a second span and invoked managed flush. A 100 ms timer resumed the first worker. This is a deterministic scheduler boundary, not synthetic load. Observed:

```text
{'first': [True], 'second': True,
 'exported_after_second_flush': ['first']}
```

`completed-before-second-flush` was absent despite the second successful result. The provider was then shut down and all probe threads joined; no probe files or repository changes were created. Public state methods use this same handle and do not add another OTEL flush. The source and probe establish a reachable race even without exporter failure. It also affects a retry joining a still-finishing timed-out attempt.

**Observable consequence:** A successful `state.flush()` can leave spans completed before that call buffered; a successful `state.shutdown()` can do the same. Readback may miss the work until a periodic export or another flush, and immediate process exit can lose it. Ordinary single-call live journeys and the held-export test do not cover this: the held test releases the earlier attempt before shutdown starts its fresh flush.

**Smallest testable correction boundary:** Keep the bounded wait around this managed provider, its stock export path, synchronous callbacks, existing default bound, and at most one running flush. Joining an earlier attempt may serialize access, but must not count as the current call's successful barrier unless it demonstrably covers spans completed before the current call. After waiting for an earlier attempt, perform the necessary fresh stock flush within the current call's remaining deadline, or return timeout if that cannot complete. Preserve failure mapping, native drain on all failures, active spans and global provider continuation. Do not add a public timeout, export queue, retry engine or application-provider ownership.

**Focused closure proof:** Extend managed lifecycle coverage with a deterministic pause after a real stock batch flush drained and before its Future settles. End another span, invoke the second flush, release the prior attempt, and require either fresh export containing that span before success or the existing timeout result while native drain is attempted. Preserve the existing held-export timeout and live successful-flush checks. This runtime-local scheduler branch fits the spec's unit-only lifecycle-injection allowance; no new framework/server harness is needed. Stable ID assignment belongs to independent validation; prior FIND-TASK-001-5's original timeout diagnosis is fixed, while this is a new regression in its correction.

## Verification limits and immutability

Inspected original/R1 commands, recorded counts and closure evidence as claims, including three Python managed framework cases, one TS managed case, callback/unit/type/doc checks and the no-op-flush mutation. Did not run builds, test lanes, installs, live providers or server journeys. Only the isolated stock-OTEL scheduling probe above ran during this review. Unchanged server OTLP fidelity and tenancy owners were not re-executed; no server/storage redesign is proposed.

HEAD was checked as `295446005b18604030bf376de6209098a28886f7`; tracked sources remained unchanged. Only this assigned report was written by this reviewer.
