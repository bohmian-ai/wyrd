# Durability and delivery lifecycle review — R1

**FAIL** — one material proposal, DUR-R1-01, at the existing FIND-TASK-001-5 correction boundary. Effective waiting is bounded now, but a joined flush can report success without flushing spans completed before that caller invoked it.

## Subject, authority, and coverage

Reviewed base `dac4c340f835676adc29f3dcc01f98e1d3d51465` through candidate `295446005b18604030bf376de6209098a28886f7`, using the prior candidate `692d390dc40e43c6e1d7e25c7850d5c826d26a9e` only to navigate R1 changes. HEAD remained the candidate. Prior verdict, findings-validation, and TASK-001-R1 were read as historical claims, without assuming closure.

Authority: AGENTS.md and agent rules; approved spec r2 REQ-010/012/013/016, INV-002/004 and AC-009; original TASK-001 and R1; spec-driven development, maintainer style, reference router, telemetry-observations, analytical-operations-reliability, testing-workflows, PyO3 boundaries and errors references. Governing design anchors include `architecture/wyrd-design.md:681-685` and Bifrost's distinct admission/acknowledgement/publication boundaries. This review does not require durable SDK telemetry queues, cancellation of an in-flight export, guaranteed delivery on process death, or end-to-end exactly-once delivery.

| Boundary / obligation | Source and proof inspected | Result |
|---|---|---|
| Python managed flush bounded independently of stock processor; drain after false/timeout/exception | `python/wyrd/otel.py:213-254`; `src/state/mod.rs:278-281,300-303,510-530`; actual installed OTEL 1.42.1 BatchProcessor `_export`, `force_flush`; held-export and parametrized unit coverage | Deadline and settlement ordering fixed; joined-success coverage fails below |
| Repeated/overlapping attempts use bounded retained work; recovery and ordinary callbacks | `_ManagedTelemetry` lock/Future/daemon worker; deadline test; stock synchronous callback/correlation owners | At most one in-flight worker; fresh calls after settlement start a new flush; callbacks stay on caller context. A still-running attempt is incorrectly reusable as a successful later barrier |
| Native flush/shutdown retains existing ownership and retry semantics | shared `state.rs:689-705`, `observe/lifecycle.rs:113-150`, `bifrost/handle.rs:315-383`; shared `TelemetryFlushFailure:2088-2163`; both runtime callers | Native drain is attempted even after OTEL failure. Ambiguous native shutdown keeps the existing writer; successful shutdown closes permanently. No replacement native queue or retry owner added |
| TS managed lifecycle delegates to stock bounded provider and preserves global provider | `wyrd/src/index.ts:2727-2745,2971-2995`; `src/otel.ts:96-111`; installed sdk-trace 2.11.0 provider and batch implementations; lifecycle unit owner | PASS for changed ordering, failure precedence, active/global provider preservation and conventional stock delegation |
| Real flush readback occurs before shutdown and further framework/native work succeeds | Python app `live:312-333`, `observed:336-341`; Python test `:138-179`; TS managed journey `:232-255` | FIND-TASK-001-3 closed in source: live flush, first readback, second real turn plus native observation, held span completion, shutdown, further real framework turn and explicit global-provider flush |
| Server publication cannot substitute for SDK export/drain | Python timeline/drift `:73-125`; TS timeline/drift `:147-214`; server `flush_bifrost:1229-1251` and binding `testing.rs:545-560` | PASS: server flush settles server outbox/Scribe, not application OTEL buffers or the client's native writer. Python waits on stdin until first readback; TS awaits readback before second turn/shutdown |
| Application-owned providers and bridge lifecycle remain separate | Python `_start_telemetry`, bridge force_flush/shutdown; TS provider field/bridge; existing owned-provider journeys | PASS: bridge shutdown ends its unfinished spans, not the provider; state touches only its managed owner. No new persistent queue, server contract, or stronger delivery promise |

## Prior-finding closure

- FIND-TASK-001-3: **closed within this domain**, supported by the phased real-framework journey sources above and the committed C6/C7 claims (three Python cases and one TS case). No shutdown or explicit provider flush precedes the first readback.
- FIND-TASK-001-5: **partially corrected, not closed**. Real bounded waiting, one daemon worker and post-timeout instrumentation usability address the original blocked-stock-flush defect. The newly introduced join path does not retain the completed-span barrier for every lifecycle invocation. The R1 evidence's stated limit that a joined call reports the earlier attempt's result and later spans need another flush is not an approved relaxation of REQ-013.
- Findings 1/2/4/6 are outside this coherent delivery scope; no independent disposition is asserted here.

## DUR-R1-01 — INCORRECT — Joined Python flush can return success for an obsolete drain

**Stable finding boundary:** retain FIND-TASK-001-5; this is the same newly introduced managed-wait coordination owner and its completion guarantee, not another callback/exporter defect.

**Exact producer:** `sdks/wyrd-sdk-python/python/wyrd/otel.py:236-247`, especially unconditional reuse of `_attempt` at `:245` and returning its result at `:247`. `:252` calls `provider.force_flush(...)` before settling that attempt, leaving an independently schedulable window between the stock drain and Future completion.

**Consumers:** public Python `WyrdState.flush` and `shutdown`, `sdks/wyrd-sdk-python/src/state/mod.rs:278-281,300-303`; both interpret `True` as successful OTEL delivery and proceed to native drain through `flush_telemetry:510-530` and shared settlement. Native drain cannot export a span left in the separate OTEL batch buffer.

**Violated obligation:** REQ-013 requires state flush/shutdown to flush completed managed spans before native drain; AC-009 requires the flush barrier's visibility while alive. Reusing a wait may bound concurrency, but success must still cover spans completed before the current call. This is neither a request to flush active spans nor to promise lossless export under arbitrary process failure.

**Reachable sequence:** an initial flush drains the stock batch queue; before its Future settles, another thread ends a new span and calls state flush/shutdown. `_attempt.done()` is still false, so the later call joins the older operation. Its Future becomes true without a stock flush after that span ended. Both calls report success; the newer completed span remains queued until an unrelated timer or another flush. A job exiting after successful shutdown can therefore lose that span. This also applies when the first caller previously timed out and a recovery call reaches the same completion window.

**Fresh read-only probe:** ran through `mise exec -- sdks/wyrd-sdk-python/.venv/bin/python -B -`, with the actual installed `TracerProvider`, `BatchSpanProcessor`, `InMemorySpanExporter`, and production `_ManagedTelemetry`. A standard `threading.setprofile` hook paused only the worker's return from the real provider `force_flush` (after export, before Future settlement). While paused, a new span ended and a second caller reached `Future.result` on the shared attempt. Releasing the worker produced:

```text
{'flush_results': [True, True], 'exported_at_success': ['before']}
```

The missing span was named `completed-before-second-flush`. Batch schedule delay was 60000 ms to keep the independent timer out of this deterministic interleaving. No provider/processor force_flush implementation was replaced; profiling merely exposes a legal thread scheduling window. Both callers joined, profiling was disabled, and the provider shut down in `finally`. No files, network, dependencies, source, or package state were changed.

**Smallest safe correction:** keep this managed lifecycle owner, stock provider/exporter, synchronous callbacks, one in-flight worker, and one absolute SDK-default wait bound per public invocation. Waiting on an older attempt must not by itself establish success for the later caller: after it settles, establish a stock flush that covers that caller's pre-invocation completed spans within the remaining bound, or report timeout/failure. Serialize that lifecycle work without a pending retry backlog, extra export queue, or unbounded workers. Preserve OTEL-primary/native-secondary error settlement, native drain on every failure, active spans, and global-provider usability. No public API or persistent-data decision is needed.

**Focused closure proof:** extend the existing managed deadline/lifecycle owner with the deterministic completion-window interleaving using a real stock batch processor. End a new span after the earlier stock drain but before its attempt settles, invoke the later lifecycle call, and require the span exported before that call succeeds (or a bounded timeout with native drain attempted). Cover later flush and shutdown/recovery callers, retaining one in-flight worker and the held-export callback/correlation checks. The current held-export test queues new spans while the exporter is blocked; stock `_export(EXPORT_ALL)` subsequently drains those spans, so it cannot expose the empty-queue-to-Future-settlement window. Existing C6/C7 live journeys remain the normal-path proof; no new IO harness or broad suite is required.

## Verification limits

No build, test lane, installation, source edit, or service operation was run in this domain review. Committed R1 commands are inspected historical claims, not fresh execution. The only fresh execution was the isolated dependency/scheduling probe above. The prior real-stock timeout proof and R1 held-export source establish the intended bound mechanism; this report does not claim a second fresh full timeout/journey run. The held-export test exercises actual blocked `flush` twice and later successful shutdown, whereas shutdown-failure branches otherwise use immediate injected provider outcomes.

Skipped broad re-verification and source changes. Remaining risk: successful joined Python flush/shutdown can leave a completed span buffered.
