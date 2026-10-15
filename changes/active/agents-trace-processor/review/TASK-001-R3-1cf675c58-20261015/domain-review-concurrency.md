# Concurrency domain review — TASK-001 R3

**PASS. Proposed finding ledger: empty.**

Immutable subject: original base `dac4c340f835676adc29f3dcc01f98e1d3d51465`,
candidate `1cf675c58d01cba0e714102c75cb7d89e7a678b5`. The original cumulative
range governs this review. The R3 delta from `d088519ea` locates remediation;
the contrast with imported main `cb29781ab` distinguishes authorized imported
behavior without replacing the original base. HEAD matched the candidate before
and after source inspection. No current peer discovery report was read.

## Boundary and authority

Reviewed provider installation and rejection cleanup, Python callback/map and
Run-context concurrency, Python bounded managed flush coordination, PyO3
GIL/setup/publication/GC seams, TypeScript suspension boundaries and native
lifecycle sequencing, and the task-facing OTLP replay authority seam.

Applied AGENTS.md and agent rules; approved spec r2 and original TASK-001;
R1/R2/R3 remediation packets and historical concurrency/verdict/evidence inputs;
reference router, spec-driven development, maintainer style, PyO3 boundaries,
TypeScript guide, Rust core and testing-workflow references; telemetry
observations; active design's managed telemetry/Observation identity contracts;
doctrine's runtime/client/server boundary; and Bifrost's durability/visibility
and ingest authority. Scope is acceptance under those contracts, not a new
linearizability or shutdown-cancellation contract.

CodeGraph was consulted before code search. Relevant complete owner bodies and
callers were expanded from its source/navigation output. Cargo.lock selects
PyO3 0.28.3; its installed `sync.rs:412–437` confirms `lock_py_attached` detaches
while contended and reattaches before using Python. Python's OTEL lock remains
1.42.1. Source/test inspection and committed execution evidence are distinguished
from fresh execution below.

## Coverage and sibling trace

| Obligation / boundary | Current source and consumer evidence | Result |
|---|---|---|
| REQ-010 / AC-007: thread-safe bridge map, duplicate callbacks, start-time identity | Python `otel.py:377–450` serializes membership/start/publication with `_lock`; `_end` claims one entry under that lock, completes it outside; shutdown atomically closes and takes remaining entries. TS `src/index.ts:2552–2630` updates its map without an intervening await. Both derive parent context at start and never recalculate Card/Run at end. | PASS |
| REQ-010 / INV-004: bridge shutdown and flush do not own provider | Python `shutdown/force_flush:401–412`, TS `shutdown/forceFlush:2580–2590`: only mapped spans are ended; subsequent callbacks cannot create new entries. Neither calls provider shutdown or flush. | PASS |
| REQ-012 / AC-008: same-state setup, cross-owner refusal and resource release | Python `PyWyrdState::start_telemetry:191–219` double-checks the published OnceLock around its GIL-aware setup gate; `_start_telemetry:271–303` checks the actual global winner and releases a refused provider. TS `startTelemetry:2924–2940` checks again after dynamic import, then installs/publishes synchronously through `otel.ts::startTelemetry`. Different states still reach OTEL global refusal, releasing only their losing pipeline. | PASS |
| REQ-012: retryable correlation registration and preserved owned-provider configuration | Python `install_run_correlation:115–145` serializes marker/check/add and removes its marker on add failure. TS `runCorrelationProcessor` returns the existing stateless processor for construction-time composition. Existing providers remain application owned. | PASS |
| REQ-013 / AC-009: effective bounded wait, one in-flight managed attempt | Python `_ManagedTelemetry:215–268` owns one Future/worker, starts actual flushing outside ordinary correlation callbacks, and uses one monotonic per-call deadline for predecessor and covering-attempt waits. A still-running operation does not spawn another worker. Timeout does not join or close the provider. | PASS |
| REQ-013 / AC-009: later calls cannot consume obsolete success | `_ManagedTelemetry.force_flush` snapshots `_started` under its lock; only an attempt started after entry covers the caller. Waiting for an older attempt permits a new flush or a covering concurrent caller's flush; `_flush` settles its own passed Future. A predecessor's old success or failure cannot substitute for the current operation. | PASS |
| PyO3 publication/read/GC seam | `src/state/mod.rs:93–99,191–219,484–540`: setup owns only its mutex; readers and traversal use the OnceLock directly, and clear uses exclusive mutable access. No traversal takes the setup gate while the installer needs Python. Retained values are `Py<PyAny>`; no Python borrow crosses an await. | PASS |
| REQ-013 / INV-003–004: both lifecycle consumers drain despite OTEL failure | Python `flush/shutdown:295–320` and TS `#flushTelemetry/#settle:2727–2744`, `flush/shutdown:2971–2995` first capture provider outcome, then await existing native drain, then settle. Shared `state.rs::TelemetryFlushFailure:2097–2163` preserves native-only code and attaches native failure secondarily when OTEL is primary. Native TS `cards.rs:548–586` delegates to shared state. | PASS |
| Native lifecycle preserved | Shared `state.rs:689–705` uses `observe/lifecycle.rs::BifrostLifecycle`; its `claim`, `started`, `shutdown`, and `StartClaim::complete/drop` retain the one-writer lifecycle, cancellation release, closed-state refusal, and retry after ambiguous drain failure. SDK telemetry glue does not replace or duplicate it. | PASS |
| REQ-014 / AC-010: nested/concurrent scope | Python immutable context-local scope tuples and `_RunCorrelationProcessor` preserve selected parent context and restore enclosing scope; TS `Run.scope` and `otel.ts` use OTEL context/AsyncLocalStorage and the same context key. Scope exit neither owns spans nor flushes. Explicit captured context is supported; implicit arbitrary-thread propagation is not promised. | PASS |
| INV-001, REQ-011, R3 FIND-9: implicit replay publisher identity | Gate `otlp_batch_id:99–157` binds verified tenant, subject/current actor, table, logical rows and accepted Card/Run. `dispatch_canonical:946–1003` supplies `auth.attribution()` to both identity and Scribe frame. The same-pair replay inputs exclude request/time/token refresh. | PASS |
| All signal/transport and replay siblings | Trace/log/metric projection calls converge at `dispatch_canonical`; HTTP/gRPC authentication constructs the same AuthContext. Scribe `ingress.rs::prepare_and_dispatch` stamps supplied attribution, routes stable tenant/table/batch identity to existing shards and awaits durable completion. `shards.rs::commit_batch_control_fences` retains SQL duplicate/conflict settlement; WAL replay retains logical identity. Native `dispatch_native_frame` retains caller-supplied IDs. Internal `scribe_outbox.rs:788–807` already hashes both attribution members and retains its own request/content ownership. No second replay owner or lock was added. | PASS |

The existing alternatives are the stock OTEL processor/exporter, OTEL global
provider registration, shared native Bifrost lifecycle and Scribe replay fence.
They remain the owners. The small Python bounded-wait owner is justified by the
recorded stock processor timeout gap; it does not replace exporting, hold a
retry queue, or move ordinary correlation callbacks to an executor. No new
concurrency abstraction can be deleted without reopening the bounded wait or
same-state setup obligations.

## Prior finding closure and proof assessment

- FIND-5 remains closed by the bounded daemon operation and native error/drain
  sequencing. `test_managed_telemetry_flush_deadline` uses a real batch processor
  with a held exporter, verifies timeout while the export remains held, permits
  new correlated spans, and observes one worker.
- FIND-6 remains closed by TS's post-import check and synchronous publication.
  The existing `telemetry setup` test uses overlapping same-state calls, then
  verifies other-state/application conflict and losing-provider cleanup.
- FIND-7 remains closed by current-call covering operations. The same Python
  deadline test invokes `_stale_flush` for both public consumers: stock flush
  has drained but not settled before the caller ends its new span; the later
  call must export that span before returning success/native-only refusal.
- FIND-8 remains closed by GIL-aware setup serialization with a lock-free
  publication/read/GC seam. Python `test_telemetry_setup[overlap]` enters from
  two threads and checks one installer entry and no refusal; the missing-SDK
  case retries successfully after its ImportError.
- FIND-3's concurrency/lifecycle proof remains intact: Python `app.py::live`
  waits for parent readback after public flush, continues with the same state,
  retains an active span across flush, then exports another actual framework
  turn after state shutdown. The parent test reads the first result before
  releasing the child. TS's managed journey follows the same sequence.
- FIND-9 is closed at its common producer, without changing Scribe exclusion or
  native explicit IDs. The derived-ID regression distinguishes `(S,S)`, `(S,A)`
  and `(S,B)` while repeated pairs converge. Existing actual attribution journey
  now submits direct/delegated identical exports across HTTP/gRPC and groups by
  subject/actor/Run, requiring five complete distinct logical row sets and no
  replay duplication. FIND-12's publisher readback now uses the current pair.
- Supporting Python callback thread, duplicate-end/shutdown, concurrent async,
  shared-Run and explicitly captured thread-context tests, plus both language
  owned-provider tests and actual nested/concurrent framework journeys, match
  the source contracts. Earlier non-concurrency findings and R3 rustdoc/error-code
  proof remain other reviewers' acceptance scope; no concurrency regression in
  those touched seams was found.

Historical R2 verification records 16 focused Python runtime passes, five TS
telemetry passes, and real framework files with 13 Python/five TS passes. R3's
committed evidence records the actor-only identity RED/GREEN and actual
attribution journey GREEN. Those are historical recorded results, not execution
performed by this reviewer. The orchestrator owns fresh verification and its
current verification artifact; this report does not predeclare those results.

## Proposals, limits and outcome

Material proposals: **none**. Minimal corrective scope: **empty**. No new FIND ID.

No arbitrary snapshot/cancellation guarantee, implicit thread propagation,
joining of callbacks already claimed by application threads, or joint
setup/flush/shutdown linearizability is required. A bounded provider wait is
not a total deadline for the native drain. A timed-out stock export can finish
in the background, as explicitly authorized by prior remediation. R3 changes
implicit OTLP IDs; compatibility of retries spanning versions is not established
by the concurrency proof and is disclosed in the R3 evidence rather than
silently claimed.

Only this assigned report was written. No tests, builds, installs, source edits,
index edits or Git mutations were performed; no live-service or host-load probe
was run. HEAD remained the candidate. **PASS** within the reviewed concurrency
boundary, with fresh execution owned by the orchestrator.
