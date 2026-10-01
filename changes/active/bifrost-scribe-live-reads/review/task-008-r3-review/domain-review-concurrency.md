# Concurrency and resource lifecycle review

Result: FAIL (one proposed finding for independent validation).

Immutable subject: candidate `2f188cb6185061a43db36122aad68b5e253308d1`; cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`, TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`; final corrections relative to `9c3d7ecb982435919924dfa8e6930352b27a9b7e`. Static review only. No build, test, mise, benchmark, source edit, or commit performed.

## Boundary and authority coverage

Reviewed the cumulative live-read lifecycle and final cap deletion/shutdown corrections through `scribe/memtable.rs`, `scribe/shards.rs`, `scribe/tail_rpc.rs`, `scribe/hot_source.rs`, `oracle/follower.rs`, `oracle/exec.rs`, `oracle/live.rs`, server `oracle/peer_service.rs`, `grpc/mod.rs`, `app/server.rs`, `app/supervise.rs`, and tonic `server/mod.rs`. Relevant obligations: spec REQ-003/004/005/006, INV-002/005/006, AC-004/005; AGENTS ownership/async/cancellation rules; agent rules; spec-driven-development and maintainer-style; Bifrost architecture live scans and shutdown; datafusion and analytical-operations-reliability references. Current maintainer authority removes snapshot batch/byte ceilings; this review accepts that decision without proposing a replacement cap.

| Boundary | Source evidence | Assessment |
|---|---|---|
| Shallow snapshot selection | `Memtable::collect_readable_batches` holds writable and immutable locks together, selects exact tenant/table/range, projects Arrow references, tags immutable generations and skips durable ones | PASS; cap removal does not change selection or seal interlock |
| Shard snapshot cancellation | `ScribeShardRuntime::snapshot` retains partial batches in its future and returns remaining shard responses through oneshot; dropping the future drops partial snapshots and the receiver; a subsequently generated response is dropped on failed send | PASS; bounded mailbox admission remains independently effective |
| Publication and staged lifetime | `FetchLiveTailService::open_live_batches` deduplicates staged generations already captured by the memtable cut; `ScribeHotSourceRegistry::staged_sources` leases under one registry lock; `StagedSourceLease::drop` returns every lease | PASS; no live-file deletion under an existing read; accepted publication overlap remains best effort |
| Executor ownership | `ScribeTailResolver::live_leaf` puts shallow rows in MemorySourceConfig; staged lease moves to HotParquetExec; `hot_stream` captures its lease and cancellation drop guard | PASS for ordinary stream drop; pool-backed staged decode retains existing behavior |
| Trust and schema before snapshot | `ScribeFragmentExecutor::execute` verifies target, ticket, assignment tenant and v6 assignment digest before follower resolution; resolver validates schema/closure before opening tail | PASS; no tenancy widening from cap deletion |
| Deleted-cap references | Rust/proto search for maximum fields, limits type, constants and old refusal string finds only reserved proto names | PASS for executable contract; stale bounded-snapshot wording is not a new concurrency mechanism |
| Shutdown while execution itself is pending | `execute_fragment` selects shutdown versus `stream.next()` | PASS for a polled response body, including the three-small-batch production-pause journey |
| Shutdown while transport itself is backpressured | Response body is not independently driven; pinned Hyper waits for HTTP/2 send capacity before polling it again | FAIL, proposal C-LIFECYCLE-01 below |

## C-LIFECYCLE-01 — shutdown cancellation cannot release a fragment stalled behind HTTP/2 flow control

Classification: INCORRECT. Changed location: `crates/wyrd/wyrd-server/src/oracle/peer_service.rs:572-583` (lazy `async_stream` shutdown select), with transport owner composed at `app/server.rs:665-695` and `wyrd-tonic/src/server/mod.rs:234-244`.

Violated obligation: the approved follow-on correction says server shutdown ends open fragment streams; REQ-005 and the server's new field documentation explicitly require releasing snapshot/fragment ownership when cancellation occurs, including a paused or slow reader. The select only cancels a fragment while the response body is being polled.

Reachable producer-to-consumer trace:

1. An authenticated remote fragment creates the snapshot and executor. The response generator captures its `WorkerAttemptStream`; that generator owns the follower batches and therefore the shallow memtable cohort or staged scan leases.
2. Arrow batches are encoded and yielded at peer_service.rs:586-595. An Oracle receiver can stop draining under ordinary downstream backpressure. After sufficiently large output exhausts the HTTP/2 window, Hyper has a buffered outgoing DATA chunk.
3. Cargo.lock pins Hyper 1.10.1. Its locally available `src/proto/h2/mod.rs:149-165` checks `buffered_data` and waits on `body_tx.poll_capacity(cx)`; it polls the actual response body only later at line 185. Thus even a previously registered token wake cannot advance the cancellation branch while this capacity gate remains pending.
4. The same server token passed to OraclePeerGrpc is used by `serve_grpc_with_listener`. Tonic's shutdown path requests connection graceful shutdown rather than resetting active response streams. Server supervision eventually reaches its outer drain deadline and aborts owned tasks (`app/supervise.rs:183-202`), but the new response-body select cannot produce Unavailable or drop the captured producer promptly on the shutdown signal.

Observable consequence: a stopped/slow remote reader can still retain the stopping Scribe's fragment, source references and staged leases and delay graceful peer termination until read resumption/reset, the leader deadline, or the server drain bound. The supplied 12.5-second journey does not cover this path: `peer_network/analytical.rs:1160-1163` ingests three one-row batches, pauses the producer, and continues reading during the Scribe-loss case at 1203-1218. That is a pending source poll, not a flow-control-stalled response body.

Smallest correction boundary: retain the peer server/connection owner as the cancellation boundary. Make server shutdown end active peer transport responses independently of polling the yielded body, so dropping a backpressured response drops its existing fragment owner. Reuse the existing server shutdown token and supervised peer listener/connection lifecycle; preserve ordinary live-stream backpressure, authentication, per-request resource ownership, and Unavailable-before-first-row versus terminal-after-rows policy. Do not add polling, a second snapshot lifetime, an unbounded producer task, or change ingestion shutdown. A select solely around `stream.next()` cannot satisfy this boundary. The precise existing connection API should be chosen by source validation rather than prescribing a new transport abstraction here.

Focused closure proof: hold an authenticated remote response open after enough output fills its HTTP/2 receive window; do not drain or reset it; signal the Scribe server shutdown. Verify the Scribe fragment/lease owners release and peer shutdown progresses within its normal cancellation budget without waiting for read resumption or the leader query deadline. Retain the existing small producer-pause journey to show ordinary cancellation and failure classification remain effective. No such check was executed during this static review.

## Verification limits

The TASK-006 benchmark diagnosis provides existing passing evidence for cap deletion, absolute root resolution and a producer-paused shutdown journey. Those results were inspected as supplied evidence, not rerun. The cap's removal was accepted as current authority. No new memory cap or speculative scheduler was proposed. The current HEAD remained the declared candidate.
