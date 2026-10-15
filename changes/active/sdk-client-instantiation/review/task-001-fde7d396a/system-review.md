# System-resilience review — TASK-001

Result: **FAIL**. One material proposed finding: `SYS-001`.

## Subject and authority

Reviewed cumulative base `2f1db2194f5630df850b10987fe0f360946a525c` to candidate `fde7d396a3917964dc344ede2ed09046e179aa66`, original task and approved spec revision 9. HEAD remains the candidate. This review was independent of other reports and implementation conclusions. Read task-review skill, shared subject/map, repository rules, spec-driven-development and maintainer references, relevant client model/doctrine and Bifrost system-boundary authority. Used CodeGraph before locating runtime code and expanded into auth, transport, runtime bridges, callers and lifecycle owners.

## Deployed paths

- HTTP-only Rust handles call `WyrdClient::connect_ambient` or `connected`, which call the existing `access_token` path before returning a client clone (`client.rs:84-105,469-470`). Cards retains it through RegistryEngine; Gateway, Principals and OperatorConnections retain their existing client fields. No server-process supervision or request-serving topology changes.
- Platform preserves the existing platform exchange and session-bound client (`platform/handle.rs:81-144`). The server production diff only migrates the setup-command call (`wyrd-server/src/main.rs:290`); it does not add a new boot dependency to a running shared server.
- Bifrost validates queue options, completes required exchange, then dials existing gRPC and creates its query client/writer pool (`bifrost/facade.rs:145-155,221-248`). Its list operation is one authenticated GET using the retained client (`bifrost/query.rs:316-322`), without describes or client-side aggregation.
- Python retains optional native client and OnceLock handle behind a first-use gate (`src/client.rs:84-138`). The bridge releases GIL while synchronously driving the shared runtime. Async Bifrost server operations use existing `asyncio.to_thread`; inherited synchronous local operations are the exception identified below.
- TypeScript async factories retain owned client clones in existing N-API promises (`native/src/cards.rs:47-73`, sibling gateway/principals/operators methods, `native/src/client.rs:180-202`, `native/src/trace.rs:80-88`). No JS borrow is retained across the added awaits.
- Local WyrdState/Workflow loaders preserve their existing deferred IO boundaries (`wyrd-client/src/state.rs:490-494,557-565`; `workflow/mod.rs:159-170`). CLI callers await changed factories. Their dependency failures remain command/request errors.

The actual topology remains SDK process -> wyrd-server HTTP/gRPC public edge -> server-owned services. Bifrost roles may share a pod/process; Postgres is the control-plane/catalog dependency and object storage is the analytical system of record (`architecture/bifrost-design.md:15-48`). No persistent catalog, WAL, Scribe/Oracle/Forge engine, audit or pod-membership behavior changes in this diff.

## Failure, containment and recovery

| Failure | Source-backed behavior |
| --- | --- |
| Required credential exchange refused/unavailable | Eager connect returns the existing structured error before publishing a handle. Supplied bearer/fresh cache performs no health probe. AuthMiddleware remains the token-cache/retry authority (`auth.rs:248-263,840-888`); unrelated initialized handles and server capabilities are not deliberately stopped. |
| Exchange timeout / cancelled Rust connect | Existing exchange HTTP client has a configured request timeout. Dropping the Rust future releases the async cache lock. Failure publishes no handle and no writer; caller may reconnect. No process crash is introduced. |
| gRPC unavailable after exchange | Existing bounded dial/backoff returns TransportDown (`transport/grpc.rs:75-105`). WriterPool is created only after successful dial (`facade.rs:221-248`). Explicit client's fresh token can be reused on retry; Python failed initialization stores no handle. HTTP-only handles acquire no gRPC dependency. |
| Concurrent Python first use | OnceLock is checked before/after the gate; successful initialization is shared. Gate wait/connection release GIL. Failure leaves initialization retryable. Waiting callers can attempt sequentially after failure; there is no autonomous unbounded retry loop. |
| asyncio cancellation | Existing `to_thread` work may outlive its awaiting task; native arguments remain owned. Factory work has not admitted data before returning. Source does not promise interruption of worker IO. Synchronous inherited local methods instead directly block the event-loop thread (SYS-001). |
| Abandoned TypeScript promise | Futures own client clones rather than interpreter borrows. Connect itself admits no ingest data. Node shutdown internals are unchanged; no stronger cancellation guarantee is inferred. |
| Expiry / restart | Retained AuthMiddleware renews renewable credentials; bearer remains presented as-is; Platform remains session-bound. Successful initialization prevents ambient retargeting. Process restart recreates ordinary in-memory handles; no new persistent recovery state. |
| Ingest outage / shutdown | Existing transport owns attempt budgeting and retained batches. Existing writer drain runs on the blocking pool (`facade.rs:694-731`). Candidate adds eager auth before assembly, not another ingest retry/shutdown owner. Abrupt SDK process death still cannot promise durability for unacknowledged rows. |
| Listing failure | One existing authenticated HTTP request returns existing stable auth/availability errors. No describe fan-out, mutation or extra retry owner. |

## Material proposed finding

### SYS-001 — INCORRECT / REGRESSION — local Python Bifrost access forces connection and can stall unrelated asyncio work

**Changed locations:** `sdks/wyrd-sdk-python/src/bifrost/mod.rs:660-664` (`use_table`), `:680-684` (`table`), `:892-899` (`dropped`, `producer_count`). Common IO path: `:565-580` -> `sdks/wyrd-sdk-python/src/client.rs:129-137`.

**Obligation:** REQ-007 defers initialization to the first operation needing the server; REQ-008/INV-001 preserve other capabilities. The public async facade preserves synchronous `use_table` because it performs no IO (`python/wyrd/bifrost/_client.py:1017-1022`); the base describes binding as a lock swap (`:640-643`).

**Reachable path:** In an asyncio application, construct `bifrost = AsyncBifrost(table=config)` and inspect `bifrost.table` or call `bifrost.use_table(config)`. These inherited synchronous methods (`_client.py:690-698`) invoke native `.handle(py)?`, which runs shared connect with synchronous `runtime().block_on`. Releasing GIL lets other threads run but does not advance the caller thread's event loop. Required exchange and gRPC dial can therefore occupy that thread through existing timeouts/backoff. Without credentials, even reading an already-supplied local table raises NoCredentials. Both counters also initialize despite having an initial local value of zero.

**Consequence:** An auth/gRPC outage stops unrelated coroutines and timers during local binding/introspection in the SDK process. It also selects ambient identity earlier than the first server operation. This is not a claimed server-process failure.

**Minimal correction:** Before connection, return/update the retained local table binding and initial local counters without invoking the initializer. `use_table` must return the previous binding and retain the replacement for eventual connection. After connection, delegate to existing native binding/counter methods so pooled producers and retained-client semantics remain unchanged. Keep actual server operations/eager connect on the shared connection path; add no probe, alternative transport, cache, background retry owner or mandatory explicit client.

**Closure proof:** Public Python tests construct both lazy facades with a local table and unusable ambient credentials, read/change/read the binding, and read zero counters without error or connection. The first server operation must still produce the existing NoCredentials error. Exercise AsyncBifrost local operations alongside an event-loop heartbeat and a stalled exchange/dial seam, proving no connection attempt occurs. A real-server case should register the binding selected before first use. Current `tests/unit/test_sdk_client_instantiation.py:24-29,53-60` instead treats `producer_count` as a server operation expected to raise NoCredentials, entrenching this wrong boundary.

## Proof assessment

Reviewed new shared-client exchange-count/bearer tests and all three instantiation journey sources. They cover required exchange refusal, invalid bearer first-operation refusal, retained-client renewal and discovery. Python constructor tests and async listing prove useful paths but do not execute inherited local operations while keeping the event loop responsive. No supplied runtime test directly exercises failed-lazy-attempt recovery, simultaneous first use, cancellation during connect or process replacement; those paths have source support only.

Rust's ambient journey resolves an explicit Environment and then calls connect_with rather than process-ambient connect. This is a proof limitation for acceptance review, not evidence of an outage defect here. Recorded lane PASS labels have no raw transcripts supplied to this reviewer.

## Limits

Static review only: no tests, builds, formatting, generators, baseline runs or runtime child processes executed. An initial attempt to write this report with the `python` shim failed because the shim is invalid; the report was then written with a shell heredoc. Only this report was written; reviewed source was not edited. No new resilience requirement beyond approved behavior and reachable regressions is proposed.
