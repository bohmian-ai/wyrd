# Concurrency domain review — TASK-001 R1

Result: **PASS**. No material concurrency proposal remains.

Immutable subject: original base `dac4c340f835676adc29f3dcc01f98e1d3d51465`,
prior candidate `692d390dc40e43c6e1d7e25c7850d5c826d26a9e`, candidate
`295446005b18604030bf376de6209098a28886f7`; approved specification revision 2,
original TASK-001, historical verdict/findings-validation and TASK-001-R1.
Both cumulative changes and the latest correction were inspected within this
domain. No closure is inferred from the implementation evidence table.

## Boundary and authority

The reviewed boundary is Python managed-provider flush coordination and its
state callers, Python callback/map/context ownership, TypeScript asynchronous
same-state startup, and preserved callback/context/lifecycle behavior. This
review does not promise arbitrary-thread propagation or new linearization
guarantees for application lifecycle operations racing one another.

Applied authority: `AGENTS.md` runtime/ownership/testing rules;
`architecture/agent-rules.md`; reference router; complete selected references
`languages/spec-driven-development.md`, `languages/maintainer-style.md`,
`languages/pyo3-boundaries.md`, `languages/python-api-and-stubs.md`,
`languages/typescript-guide.md`, `languages/testing-workflows.md`, and
`domain/telemetry-observations.md`; `architecture/wyrd-doctrine.mdx` and
`wyrd-design.md` managed-export and Observation identity sections (664–685,
818–846). Acceptance derives from REQ-010, REQ-012–014, REQ-016, INV-002–004,
AC-007–010 and the original task's scenarios 1–4/6. The remediation's
FIND-TASK-001-5/6 recommendations remain subordinate to those obligations.

CodeGraph was consulted first for owners/callers. Full relevant method bodies,
sibling entry points and actual installed dependency implementations were then
inspected. No other current discovery report was read.

## Source, caller and obligation coverage

| Obligation / boundary | Source and consumer evidence | Assessment |
|---|---|---|
| One managed flush in flight, bounded lifecycle wait | Python `python/wyrd/otel.py:213–254` (`_ManagedTelemetry`); construction at `:279–289`; Python state `src/state/mod.rs:272–302,510–533` | The lock covers checking/replacing `_attempt` and starting its one daemon worker. Every caller captures that attempt before unlocking. Waiting happens outside the lock through `Future.result(timeout)`. Running attempts are shared; completed attempts allow a fresh flush. There is no pending-attempt collection, retry queue or executor. PASS. |
| Correct result ownership across overlapping callers | `_ManagedTelemetry.force_flush/_flush` | Worker receives its own Future explicitly rather than settling whichever Future is currently on the handle. False, true and exceptions stay attached to that operation; wait timeout does not cancel the Future. A later call cannot overwrite the result awaited by an earlier caller. PASS. |
| Native drain follows failure; no timeout cleanup re-block | `src/state/mod.rs::flush_telemetry`, `flush`, `shutdown`; shared `wyrd-client/src/state.rs::TelemetryFlushFailure` | Builtin `TimeoutError` becomes the existing timeout value, other exceptions become type-only failure values; both methods attempt native drain afterward and settle failures through the existing owner. No worker join or provider shutdown occurs in these paths. PASS. |
| Ordinary callbacks retain caller-local correlation and remain available during blocked export | `_RunCorrelationProcessor.on_start`, `_scope_stack`, `install_run_correlation`, `_enter_run/_exit_run`, `_ManagedTelemetry`; installed OTEL synchronous multi-processor and BatchProcessor | Only explicit provider flushing moves to a worker. `on_start`, ending, and `on_end` stay on the originating thread; correlation reads the supplied/current OTEL context there. Batch `emit` enqueues without acquiring the export lock. A held exporter cannot occupy a shared callback executor because none was added. PASS. |
| Managed and application-owned lifecycle remain distinct | Python `_start_telemetry`; Python state `start_telemetry`, GC traverse/clear; TS `WyrdState.#flushTelemetry`, `flush/shutdown`; both bridges' shutdown/flush | States retain only their own installed managed handle; application providers are not adopted. State shutdown does not close the global provider. Adapter shutdown closes callback admission and ends its own unfinished spans only. PASS. |
| Same-state TS setup is idempotent across ordinary async overlap | TS `wyrd/src/index.ts:2924–2943`; synchronous installer `src/otel.ts:96–107` | Post-import recheck sees any installation performed by the other continuation. No await separates that check, install, and field assignment. Exactly one continuation installs; others return. Other states still reach the global refusal and loser cleanup. PASS; prior FIND-TASK-001-6 closed. |
| Thread-safe bridge map; duplicate/missing/late callbacks; start-time attribution | Python `otel.py:360–435`; TS `index.ts:2552–2629`; Python Run entry/exit binding; TS `Run.scope:2481` | Python admission, parent selection and span construction occur under one map lock; completion atomically removes its retained span, then updates/ends it outside the lock. Shutdown closes admission and drains the remaining map under that lock. No completion-time Run lookup occurs. TS map transitions contain no awaits, so promise-returning callback signatures do not introduce mid-transition interleaving. PASS. |
| No new propagation contract or callback scheduler | Context-local tuple stack, `_RunCorrelationProcessor`; TS active OTEL context and AsyncLocalStorage; installed Agents producer callbacks | Python Agents invokes processors synchronously from `SpanImpl.start/finish` and `TraceImpl.start/finish`. Explicit OTEL context propagation remains available; the flush thread is not used to dispatch callbacks. No new application runtime or detached-task propagation is assumed. PASS. |

Existing repository/native writer lifecycle ownership was reused rather than
reimplemented. Neither stock Python batch flushing nor whole-provider concurrent
processing supplies the required combination of bounded waiting and unchanged
ordinary callback context. The new narrow standard-library handle earns its
state by owning a retained in-flight operation. It does not own a second export
buffer or perform durable delivery.

## Dependency evidence and prior finding closure

The lockfile pins Python OTEL SDK 1.42.1 and Agents 0.23.1. Installed
`opentelemetry/sdk/trace/__init__.py:150–219` dispatches ordinary callbacks
synchronously and checks flush deadlines between processors. Installed
`sdk/_shared_internal/__init__.py:167–200,240–246` performs flush export under
`_export_lock` and ignores the passed timeout. Its `emit` at `:202–216` uses
the existing deque/event path, not that export lock. The wrapper bounds the
caller wait around this exact operation; it does not claim to cancel it.

| Prior finding | Independent closure assessment |
|---|---|
| FIND-TASK-001-5 — effective Python lifecycle bound | Closed in this domain. The worker, Future timeout and unchanged native settlement address the diagnosed synchronous wait. A blocked attempt stays in flight across later timeouts instead of accumulating workers. Ordinary callbacks retain their thread/context and can enqueue while export is held. |
| FIND-TASK-001-6 — overlapping TS same-state startup | Closed by the post-import check and no-suspension installation sequence. The existing setup test now starts both calls before awaiting and retains other-state/application refusal and loser cleanup. |
| FIND-TASK-001-3 — provider continuity / live flush | Concurrency/lifetime portion supported: Python live app keeps a span open over flush, then performs second work/native write, ends it, shuts down the state and runs further framework work through the same provider. TS managed journey does likewise. Full persistence/readback acceptance is outside this domain's verdict. |
| Other prior findings | Safe usage/type/docs closure is not independently adjudicated by this concurrency review. None requires changing the reviewed concurrency semantics. |

The committed deadline test (`tests/unit/state/test_observe_surface.py:847–892`)
uses the actual installed managed stock batch path with an Event-held exporter,
two sequential timeout calls, native-not-started secondary code, one worker,
correlated span creation during the hold, later successful shutdown and active
span/global-provider survival. Its fallback releases the controlled export if
the bound regresses. It directly supports repeated-attempt behavior; concurrent
waiters were additionally examined by the isolated probe below. The other seven
lifecycle cases prove returned outcomes/error settlement; they do not substitute
for the held stock-export test.

Existing callback/thread, explicit captured-context, async/nested scope and
real concurrent framework tests were inspected as regression coverage. The
remediation command record claims nine focused Python lifecycle/correlation
passes, the TS setup pass and the three Python/one TS managed journey passes.
These are recorded evidence, not newly executed suite results in this review.

## Fresh review-time probe and limits

One tiny read-only probe ran through `mise exec --
sdks/wyrd-sdk-python/.venv/bin/python -B` and exited 0. It AST-extracted the
exact candidate `_ManagedTelemetry` class into an isolated process, supplied
the existing `Future`/threading modules and a 40 ms test-only bound, and used a
real `TracerProvider`/`BatchSpanProcessor` with an Event-held in-memory exporter.
Five callers entered together through a Barrier. All five timed out within
500 ms; exactly one `wyrd-telemetry-flush` worker remained; a new ordinary span
started/ended during the hold; releasing the export and making a fresh flush
succeeded, retaining both completed span names. Controlled threads were joined,
the Event was released in cleanup and the provider shut down. No probe file,
package install, source edit or bytecode artifact was created.

This probe establishes only the owner's overlapping wait coordination with
the installed stock processor. It does not rerun public PyO3/catalog/native
drain or real-server journeys. Those links were traced in source and covered
by the inspected committed tests. No build, test lane, live-service experiment,
or host-load generation was performed by this reviewer.

No new material proposal, source-local correction, or stable FIND ID is needed.
In particular, this review does not strengthen an overlapping flush into a new
per-caller snapshot/cancellation guarantee, require implicit arbitrary-thread
Run propagation, or require shutdown to join callbacks already being completed
by the application. Timeout remains a bounded wait with best-effort background
export, as the approved remediation explicitly specifies.

HEAD remained the pinned candidate and tracked source diff was empty at the
final source check. Only this assigned report was written by this reviewer.

**PASS** within the concurrency boundary.

Skipped broad re-verification and source changes. Risk: a timed-out stock export
continues best-effort in the background; this is the approved lifecycle limit.
