# Concurrency and resource-lifecycle review

Overall result: **PASS**. No material proposed findings in this domain.

## Immutable subject and scope

Candidate `ca99db0af5a0d898ef67834699405c1c73719f56`, correction parent `2f188cb6185061a43db36122aad68b5e253308d1`, cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`, TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. HEAD was rechecked and remains the candidate. Review includes the cumulative changed live-source/scan lifecycle and the R4 correction; unrelated TASK-006 benchmark completion is excluded.

Authority: current static-only instructions and fixed maintainer decisions; approved spec revision 20 REQ-003/004/005/014/015, INV-004/005/007, AC-003/005/016/017; original TASK-007/TASK-008 and TASK-008-R4; AGENTS.md ownership/async/tenancy/testing rules; architecture/agent-rules.md; spec-driven-development and maintainer-style references; Bifrost design sections on live reads, shutdown, query-owned execution and exact release. No cap, Postgres tenant-column correction, or change to FIND-007-3 is proposed.

## Boundary and source coverage

| Boundary | Reviewed source and relevant evidence | Result |
|---|---|---|
| Socket shutdown and cancellation wakeup | `wyrd-tonic/src/server/mod.rs:267–379`: each accepted private socket owns an allocated, pinned cancellation future; read/write/vectored-write/flush poll that future before forwarding TCP IO. `app/server.rs:662,678` selects graceful public serving and cancellable private serving separately. | PASS |
| TLS identity through wrapped socket | `StoppingIo::Connected` retains `TcpConnectInfo`; `wyrd-tonic` server feature enables `tonic/tls-connect-info`; `grpc/mod.rs:155–180` still checks `TlsConnectInfo<TcpConnectInfo>` and cluster certificate DNS identity before admitting a private body. | PASS |
| Open stream/body ownership | `oracle/peer_service.rs` `ScribeFragmentExecutor::execute` retains batches and scan evidence in its result stream; `OraclePeerGrpc::execute_fragment` owns that stream and selects shutdown while polling it. IO cancellation additionally covers the flow-control state where this body is not being polled. | PASS |
| Memtable/staged source construction | `scribe/tail_rpc.rs:326–372,572–699`: shallow memtable snapshot and staged lease remain one opened read. `oracle/follower.rs:779–868,1023–1130` transfers projected Arrow references into the memory source and staged lease into the shared Parquet leaf, including construction-error drops. | PASS |
| Staged protection and query memory release | `oracle/exec.rs:2668–2673,2735–2742,2880–2965`: plan and partition streams retain the staged lease; stream-local drop guard cancels outstanding metadata work; range/batch memory reservations use the existing governed owners. `scribe/hot_source.rs:239–264` releases leases exactly through Drop. `resources.rs:1250–1279` issues a growing shared-pool view rather than an independently reserved Scribe concurrency slot. | PASS |
| Leader cancellation/deadline/LIMIT | `oracle/live.rs:356–401,495–637`: leaf opens lazily; cancelled/deadline select exits drop the child stream; intentional plan stop owes no footer; still-needed unexpected EOF fails; pre-row source loss alone degrades. Shared query-stream lifecycle retains terminal/cancellation ownership. | PASS |
| Concurrent publication | `tail_rpc.rs:579–598` excludes staged generations already represented by its memtable snapshot; unserved staged runs remain leased while read. Publication cleanup waits on existing staged leases rather than deleting underneath a reader. Best-effort cut transition semantics remain the approved behavior. | PASS |

## Socket-to-body failure trace

The prior defect occurs below response-body polling. Hyper's HTTP/2 sender can park on capacity and stop polling `execute_fragment`'s shutdown-select. R4 makes the accepted socket itself cancellation-aware, which is the shared cause boundary for stalled peer connections. The owned cancellation future registers the connection driver's waker on every underlying read/write/flush. It stays pending without a busy loop. Cancelling the server token wakes that driver; its next socket poll returns `ConnectionAborted` and fails the HTTP/2 connection. Shutdown permission is limited to this listener's server token, not a per-request token, so cancellation of one query does not close unrelated peer requests.

Dependency source was inspected locally, without running tooling: tonic 0.14.6 `transport/server/mod.rs` wraps custom incoming IO in its ordinary TLS path, drives the connection, and waits for connection watchers to drop; `server/conn.rs` creates `TlsConnectInfo<T::ConnectInfo>` for TLS streams. Hyper 1.9.0 `proto/h2/server.rs` drives the h2 connection and per-stream tasks; `proto/h2/mod.rs`'s `PipeToSendStream` awaits h2 capacity/reset. The resulting connection error reaches blocked stream tasks and drops their response bodies, including the captured Scribe source stream. tokio-util's owned cancellation future checks the cancelled flag around notification polling, preventing a missed-cancellation race. The custom socket does not replace TLS or request-context authentication.

`poll_shutdown` intentionally still delegates to TCP so transport cleanup can close the write half. A TCP stream's shutdown does not require the peer to supply HTTP/2 credit; this exception does not reintroduce the blocked body wait. No new retry, timeout, detached producer, lease service, or alternative admission owner is added.

The private listener serves more than Scribe fragments, so stopping a pod ends every connection on that listener. This is a shutdown-only boundary: peer work already has query/role cancellation and ordinary transport-failure handling; durable ACK/WAL/publication ownership is not relocated. The public listener uses the unchanged graceful function. R4 neither closes another pod's listener nor introduces a runtime integrity error that terminates a healthy shared process.

## Closure and proof assessment

| Prior obligation | Static closure and supplied proof | Result |
|---|---|---|
| FIND-007-10: window-blocked peer read cannot retain a stopping Scribe | New journey `peer_network::analytical::remote_live_window_blocked_scribe_stops_cleanly` opens an undrained public stream over 2M remote rows, waits until the sole producer's batch counter stops, then awaits ordinary Scribe stop before reading the client. It asserts clean stop, zero remaining live producers, failed read, and released Oracle admissions. It does not resume reads to permit shutdown. Supplied RED drain-deadline failure and GREEN clean stop exercise the diagnosed boundary. Existing client-drop journey and server-peer lane remain reported green. | CLOSED |
| FIND-007-9: streamed tenant failure must not become source loss | `dispatcher.rs:1813–1823` keeps Aborted as TenantInvariant; `live.rs` refuses degradation for that class. Error exits drop the same owned producer/lease path; no new ownership branch. Distributed staged-only foreign-footer journey asserts failed tenant terminal with zero rows and exactly one leader refusal observation. | CLOSED |
| FIND-007-8: live snapshot authority must match cap-free lifetime | Bifrost design now describes shallow references, no batch/byte cap, query-pool governance, source ownership through stream completion/drop, and no independent tail timeout. | CLOSED |
| FIND-007-7 | Content-only whitespace correction has no concurrency effect; supplied checks cover both bases. | No domain objection |

The stall counter is compiled only with test-support and records production progress without controlling production scheduling or release. Because it is process-wide, the journey also requires exactly one producer; its serialized lane and isolated case make its observation credible. Source-level staged-release proof additionally exists in `a_dropped_staged_scan_releases_its_lease_immediately`: the lease persists during an open read and disappears when its stream and plan drop. These are complementary checks rather than a new resource mechanism.

## Verification limits and findings

Static review only. No cargo, nextest, mise, builds, tests, benchmarks, commits, or reviewed-source edits were performed. Reported fmt/lints/docs and Oracle 42/42 plus server-peer 11/11 outcomes are supplied implementer evidence, not independently reproduced execution. Runtime timing and operating-system scheduling cannot be newly established by this review; the focused RED/GREEN evidence matches the inspected producer-to-transport failure path.

Proposed finding ledger: **empty**. No concurrency/lifecycle regression requiring correction was established. FIND-007-10 is closed at the socket owner without weakening tenancy, source protection, query cancellation, or public-listener semantics.
