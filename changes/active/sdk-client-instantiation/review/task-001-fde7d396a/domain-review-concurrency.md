# Concurrency and foreign-runtime domain review

## Subject and result

**FAIL** — one material initialization-boundary regression proposed below.

Reviewed cumulative range `2f1db2194f5630df850b10987fe0f360946a525c..fde7d396a3917964dc344ede2ed09046e179aa66` against approved specification revision 9 and original TASK-001. HEAD was checked and remains the candidate. This report is independent discovery, not a completion verdict.

## Authority and source coverage

| Boundary | Governing authority reviewed | Source and consumers inspected |
|---|---|---|
| One successful Python lazy connection; eager parity; retry after failure | Spec REQ-003, REQ-006–009, REQ-014–015, INV-002–005; original task outcomes/scenarios 2–6; AGENTS §§5–8,11; agent rules; spec-driven development | `src/client.rs::LazyConnection::{new,get}`; Python `Bifrost`, `PyCards`, `PyGateway`, `Principals`, `OperatorConnections` constructors/factories and first-use callers; shared `WyrdClient::{connect_ambient,connected}` and native Bifrost connect/assemble |
| GIL and runtime boundary | Complete `languages/pyo3-boundaries.md`, `rust-core.md`, relevant `python-api-and-stubs.md`; AGENTS §§6–8 | Python `src/trace.rs::{active_trace_context,block_on}`; shared `wyrd-runtime::runtime`; all new LazyConnection call sites; dependency declarations pin PyO3 0.28 and Tokio 1.49 |
| Async Bifrost local and IO paths | Spec REQ-005,007–009 and INV-001; Wyrd design Client model; Bifrost design Public surface; Python API rule against hidden IO in properties; original nonblocking AsyncBifrost contract | Complete changed Python facade diff and relevant complete base/facade methods; native Bifrost local getters/use_table/insert and IO methods; shared facade `insert`, `use_table`, `table`, `flush`, `shutdown`; WriterPool/ClientScope construction and enqueue ownership |
| Resource retention, shutdown, cancellation | Spec REQ-014, INV-001,005; Wyrd design/client doctrine; Bifrost public composition and buffered-admission contract | Bifrost lazy storage, shared connect/assemble and writer pool; Python asyncio.to_thread methods and eager async connect; shared drain uses an owned Arc and spawn_blocking |
| N-API asynchronous factory lifetime | Complete `languages/typescript-guide.md` and relevant maintainer guide; AGENTS §§6,11 | Full changed factory bodies in native cards/gateway/principals/operators, native client Bifrost connection path and new table listing; TypeScript factory consumers; `native/src/trace.rs::spawn` and its Send + static future bound |
| Proof coverage | Complete testing-workflows reference, AGENTS §11 | Python unit/integration SDK-instantiation tests; shared-client eager/cache test; task appendix lane claims (treated as claims only) |

CodeGraph was used first for source location and call-path discovery, then focused source/diff inspection expanded its results. No other reviewer report was read.

## Proposed finding

### CONCURRENCY-001 — REGRESSION / INCORRECT — Important

**Location:** `sdks/wyrd-sdk-python/src/bifrost/mod.rs:662` (`use_table`), `:682` (`table`), `:707` (`insert`), `:893` (`dropped`), `:899` (`producer_count`); common cause is unconditional initialization through `Bifrost::handle` at `:554–565` and `LazyConnection::get` in `src/client.rs:129–138`.

**Violated obligation:** REQ-007 places lazy connection on the first operation that needs the server; INV-001 preserves the existing capability/lifecycle boundary; Python API and stubs reference forbids hidden IO in property accessors. The existing AsyncBifrost contract explicitly says blocking native calls run in a worker and `insert`/`use_table` remain synchronous because they perform no IO (`python/wyrd/bifrost/_client.py:1027–1032`, unchanged from the base). Shared Bifrost `insert` is explicitly synchronous in every language because it is a bounded nonblocking channel send (`crates/shared/wyrd-client/src/bifrost/facade.rs:569–587`); Bifrost design Public surface requires immediate bounded admission without waiting on network sends.

**Producer → state → consumer evidence:**

1. `AsyncBifrost(...)` now constructs an uninitialized native handle (`_client.py:670–674`; native constructor `mod.rs:592–595`). This is the approved constructor change.
2. AsyncBifrost inherits `_BifrostBase.use_table`, `insert`, `table`, `dropped`, and `producer_count` unchanged. They directly call the native method/property on the event-loop thread (`_client.py:690,697,721,731,737`), unlike its async IO methods, which use `asyncio.to_thread`.
3. Every corresponding changed native operation now calls `self.handle(py)?`. On first use that calls the shared Rust connect, requiring credential exchange when necessary and always dialing Bifrost's persistent gRPC transport (`shared bifrost/facade.rs:147–154,220–228`).
4. `LazyConnection::get` releases the GIL, but then calls `runtime().block_on(...)` synchronously on that same calling OS thread (`client.rs:130–136`). GIL release allows other Python threads to execute; it cannot let that event loop execute another task.

**Reachable consequence:** `b = AsyncBifrost(table=cfg); b.use_table(other_cfg)`, `b.table`, `b.producer_count`, `b.dropped`, or `b.insert(row)` as first use now performs network setup on the event-loop thread. A slow exchange/dial suspends every unrelated coroutine on that loop. A local property or table swap also raises missing-credential/transport errors and freezes ambient identity even though no server operation was requested. This affects the sanctioned lazy path; requiring users to call eager connect first would remove the promised addition rather than fix it. Successfully initialized handles do not have this first-use stall.

**Existing owners / minimum correction boundary:** The local active-table state already belongs to native Bifrost (`table`/`use_table`), and bounded enqueue/resource accounting belong to its existing WriterPool; the shared Rust connect/transport remains the only connection owner. Correct the lazy/local boundary so pure binding/counter access does not initialize a transport and inherited synchronous operations cannot perform a blocking connect on an asyncio thread. Preserve the existing synchronous bounded insert contract, its backpressure and flush/shutdown ownership, and shared connect/error/cache semantics. Reuse or extend those existing owners as needed; do not add an SDK-side unbounded row buffer, new authentication path, per-operation transport, hidden background fire-and-forget writes, or a requirement that every lazy caller explicitly connect. Simply wrapping only `AsyncBifrost.connect` in `to_thread` is insufficient (it already is), as is detaching the GIL. If reconciling nonblocking lazy enqueue requires a new approved public/concurrency decision, route that conflict to specification authority rather than silently changing `insert` into an awaitable or weakening bounded admission.

**Smallest focused closure proof:** Add Python-runtime coverage of the lazy inherited local operations before any awaited server operation. Without credentials, table/binding/counters must remain local and must not select a client. Through a controlled delayed connection path, an asyncio heartbeat must progress during every operation that can initiate server connection; no inherited synchronous method may block on that delay. For any revised enqueue initialization, prove the same bounded queue-full behavior and that rows accepted before connection are flushed/shut down through the one shared owner without loss or duplicate staging. Existing tests only assert that the constructor itself returns and that an awaited IO method works; they never exercise these first-use inherited paths. Unit `FIRST_USE` currently treats `producer_count` itself as a connection trigger (`tests/unit/test_sdk_client_instantiation.py:24–26`), so it would preserve the regression.

## Other boundary assessments

| Obligation/path | Assessment |
|---|---|
| Simultaneous first use of one native Python handle | Source PASS: OnceLock fast path, then a mutex acquired inside `py.detach`, then a second OnceLock check. Successful connection is published while serialized; the mutex is dropped before the detach closure returns and Python reattaches. No mutex is held while waiting to reacquire the GIL. Shared/read PyO3 borrows and native Send/Sync state do not expose an evident race. No new synchronization machinery is required. |
| Connection error and retry | Source PASS: the fallible block_on returns before publication on failure, so failure does not poison the logical connection state. A later attempt uses the retained explicit client or retries the ambient connect. A successful handle is reused for later operation failures; those failures do not clear it or re-resolve ambient state. |
| Eager/lazy successful native path | Source PASS except the local/async boundary above: factories build the same lazy state and explicitly initialize it. Constructor state contains native arguments, not Python Bound objects held across awaits. |
| GIL release | Source PASS for synchronous network calls and initialization: shared trace block_on and LazyConnection detach before driving IO; trace context is captured while attached. This does not imply asyncio nonblocking behavior. |
| Awaited AsyncBifrost IO methods | Source PASS: existing to_thread pattern contains the now-added lazy initialization as part of the worker call. The new async eager connect uses to_thread too. |
| Async cancellation | No separate changed defect established. Cancelling a to_thread await cannot stop an already-running worker, an existing bridge property; the worker retains its bound native object. Eager connect has no accepted writes yet. No cancellation/timeout API redesign is demanded by this review. |
| Retained Bifrost resources | Source PASS after initialization: one NativeBifrost remains in OnceLock; reads/writes/drains use that same owner. Shared flush/shutdown clone the existing WriterPool Arc and run blocking drains in spawn_blocking. Table swaps preserve the existing pooled producers. No candidate change resets or replaces that initialized owner. |
| N-API futures | Source PASS for the changed factories/listing: a native WyrdClient (or Bifrost Arc) is cloned before async move; trace::spawn requires a Send + static future. No Env, V8 handle, or borrowed JS value crosses those new awaits. Result handles own their connected native state. |

## Verification limits

Static review only, as required by the immutable subject. No builds, tests, formatters, generators, or runtime child processes were run. The candidate task appendix reports green lane results but provides no raw transcripts; these were not independently reproduced. No simultaneous-first-use, retry-after-initialization-failure, cancellation, or event-loop-heartbeat test exists in the inspected instantiation tests; absence alone is not reported as another defect or as a requirement for new concurrency semantics.

Skipped unrelated server/storage/security behavior and whole-repository correctness. The material remaining risk is first-use IO from synchronous AsyncBifrost/local methods; ordinary awaited happy-path journeys do not cover it.
