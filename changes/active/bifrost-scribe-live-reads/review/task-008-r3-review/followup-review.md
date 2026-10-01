# Focused remote-stream follow-up

Result: **RESOLVED**. Both investigated defects have reachable source-backed paths. This report proposes findings for independent validation; it does not select the task verdict.

## Subject, scope, and limits

Candidate `2f188cb6185061a43db36122aad68b5e253308d1`, final parent `9c3d7ecb982435919924dfa8e6930352b27a9b7e`, cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`, TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. HEAD remained the candidate. Read shared subject, relevant cumulative/final changes, TASK-007/008 requirements and TASK-006 diagnosis, and the four specified discovery reports. Applied current user authority, AGENTS, agent rules, spec-driven-development, maintainer-style, spec revision 20 REQ-003/004/005/015 and Bifrost live-source, cancellation and resource ownership authority. No CodeGraph index. Static source inspection only; no tests, builds, mise, benchmark, commits or source changes. This report is the only artifact written by this reviewer.

The approved cap deletion, FIND-007-3 disposition, Postgres exclusion and existing query-tenant-invariant error code stand. The investigation addresses only the conflicting streamed tenant-error classification and the unpolled backpressured shutdown path.

## FUP-001 — streamed tenant refusal is converted into availability loss

**Proposed classification: INCORRECT.** Corroborates BEH-R3-2; contradicts the tenancy report's blanket classification-preservation statement.

### Producer-to-consumer proof

1. `oracle/follower.rs:778-869` builds a staged `HotParquetExec` from authenticated assignment tenant and local leased paths. It reads path size, not footer tenant, during construction. `PhysicalPlanFollower::execute` at `follower.rs:1431-1460` returns the execution stream; this does not await each file's footer.
2. `oracle/exec.rs:2845-2872` returns the lazy hot stream. `hot_stream` at `exec.rs:2883-2963` obtains retained metadata asynchronously and calls `tenant_proven_reader_metadata` before building the Arrow reader. A staged-only remote live route with foreign, missing or duplicate footer can therefore fail during the first stream poll, after the RPC response exists and before any batch.
3. `ScribeFragmentExecutor::execute`, `wyrd-server/src/oracle/peer_service.rs:379-397`, maps the failing native batch through `scribe_stream_error`. Its complete classifier at `690-695` preserves the typed tenant failure as `DispatchError::TenantInvariant`.
4. `OraclePeerGrpc::execute_fragment`, `peer_service.rs:572-601`, emits schema first and maps the failing native frame through `dispatch_status`. The complete mapping at `648-658` emits `Status::aborted` for tenant invariant. Tonic's encoder preserves an application error, emitting buffered schema/data first if necessary (`tonic-0.14.6/src/codec/encode.rs:87-140`); it does not convert Aborted to another status.
5. The authenticated remote consumer `TonicOraclePeerTransport::execute_candidate`, `oracle/dispatcher.rs:1470-1514`, applies `stream_status_error` to the delivered tonic stream. Its complete classifier at `1809-1815` maps Aborted through the catch-all to Unavailable.
6. `oracle/live.rs:563-576` permits that Unavailable outcome to omit the route when decoder row count is zero. Schema arrival does not increment rows. `is_availability_loss` at `624-634` includes Unavailable; `live_error` at `639-647` would preserve TenantInvariant if it reached that consumer. Thus the before-row refusal can end with a successful degraded query rather than the required fail-closed tenant-invariant terminal. No foreign row need escape for this contract violation.

### Siblings and correction boundary

Initial remote open refusal uses `live_execution_status_error`/`execution_status_error` (`dispatcher.rs:1778-1806`), which already reserves Aborted for TenantInvariant. Local native Scribe routing preserves the native enum. The shared streamed transport conversion manufactures the incorrect state; neither the authenticated footer producer nor the live omission policy needs another guard. The conversion has one runtime caller, shared by remote fragment roles; preserving tenant identity there agrees with the already-established private peer convention and preserves sibling completeness behavior.

**Smallest correction:** add the existing Aborted-to-TenantInvariant classification to `stream_status_error`; retain existing availability handling and security-terminal cases. Do not replace the whole streamed classifier with the open classifier: that would also change unrelated streamed stale/source-loss and retry semantics. No new public error, wire field, protocol or authorization decision is required.

**Focused proof:** extend existing classifier assertions for a streamed Aborted and add the existing real-peer staged-footer negative journey with a remote Scribe, staged-only data and zero earlier rows. Assert the public existing tenant-invariant code and refusal audit, rather than degraded success; retain genuine pre-row Unavailable degradation and after-row failure cases. Current open-classifier test `live_scribe_open_status_separates_availability_from_faults` does not exercise this conversion.

## FUP-002 — shutdown does not drive a fragment blocked by HTTP/2 send capacity

**Proposed classification: INCORRECT.** Consolidates SYSTEM-001 and C-LIFECYCLE-01. The behavior report's polled-body qualification is correct; its blanket shutdown acceptance does not cover the claimed slow-reader boundary.

### Dependency and lifecycle proof

Cargo.lock pins tonic 0.14.6 (`9748-9749`) and hyper 1.10.1 (`4525-4526`). Inspected their local registry source under `/home/thorrester/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`.

- `peer_service.rs:572-601` captures the native fragment stream in a lazy generator, yields encoded frames and races shutdown only when the generator is polled again. Source leases and shallow references remain owned by the captured native stream until it drops; `hot_stream` explicitly captures the staged lease and cancellation-on-drop guard (`exec.rs:2898-2905`).
- Tonic's `codec/encode.rs:87-140` polls that generator from the encoded response-body poll. It does not independently drive shutdown.
- Hyper's `proto/h2/mod.rs:134-185`, `PipeToSendStream::poll`, checks remote reset, then drains `buffered_data`. At `150-165`, zero send capacity returns Pending before `stream.poll_frame` at `185`. Once an ordinary reader stops granting credit after sufficient DATA, no body select runs even if its old cancellation waker wakes the task. A remote RST_STREAM would resolve this, but server token cancellation does not produce one here.
- Wyrd `wyrd-tonic/src/server/mod.rs:234-244` calls `serve_with_incoming_shutdown`. Tonic `transport/server/mod.rs:925-963` spawns detached per-connection tasks and handles its shutdown watcher by calling `conn.graceful_shutdown`; it keeps polling the connection. Its outer server waits for all watcher receivers to close at `868-876`. Graceful shutdown does not drop active responses that cannot progress.
- `BoundServer` at `app/server.rs:665-695` owns the peer serving task but not tonic's detached connection tasks. `drain_with_shutdown_hooks` (`app/supervise.rs:168-202`) cancels the token then waits until the common drain deadline; outer task abort at that deadline does not itself abort those detached connection tasks. `app/server.rs:860-888` then takes the Bifrost abort fallback if transport drain consumed the budget. Production process exit eventually kills sockets, but it cannot retroactively establish a clean drain; retained in-process server runs can retain the connection after outer task abort.

### Reachability, scope and correction choice

A valid remote fragment with enough rows to exceed the receiver window, followed by ordinary downstream backpressure, supplies this path without malformed traffic, synthetic load or a novel failure model. No live-read cap exists by approved decision, and this report requires none. The new field documentation specifically claims paused/slow readers cease holding the fragment during shutdown. The observed consequence is retained stopping-node fragment ownership and transport drain consuming the role-drain budget, not a tenant leak or acknowledged-data deletion.

The supplied existing journey is narrower: `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/analytical.rs:1160-1163` ingests three one-row batches, pauses the producer, and `1203-1218` continues reading during Scribe loss. It proves the pending-source select, not exhausted downstream HTTP/2 credit. Some discovery citations incorrectly placed this test under `wyrd-server/tests`; the actual owning test is under `wyrd-testing/tests/bifrost/oracle/peer_network`.

**Smallest safe correction boundary:** retain normal pull execution and add shutdown-driven termination to accepted connections of the existing private peer listener. Reuse its existing server token and tonic's generic incoming-IO API (`transport/server/mod.rs:1094-1113`, `AsyncRead + AsyncWrite + Connected`). Cancellation-aware accepted IO can wake and fail the connection's read/write driver on that token without relying on response-body polling; tonic then drops its connection and owned response streams. Use a connection IO error, not a graceful EOF that could retain outstanding sends. Forward the existing TCP connect info so tonic TLS still produces the same `TlsConnectInfo<TcpConnectInfo>` used by peer authentication (`transport/server/conn.rs:106-120`). This is a local serving-boundary implementation option, not a new transport or protocol.

Apply it only to the private peer listener in `app/server.rs:673-695`, not by silently changing the globally shared `serve_grpc_with_listener` behavior for public gRPC (`660-671`). `build_peer_grpc` (`grpc/mod.rs:400-468`) mounts private fragment/query control, active-stream discovery, Analytical stage and lifecycle services; these peers share the process shutdown tree already. Ending connections on an already-stopping peer does not authorize crashing a live shared server or changing public ingestion settlement. Preserve admission stop/readiness-before-cancellation ordering, mTLS/typed authorization, existing query failure mapping, durable Scribe recovery and role settlement. No new timeout, source pool, task pump or unbounded queue is needed. A producer pump alone could drop source ownership but would still leave the blocked tonic response/connection preventing transport drain, so it does not close the complete diagnosed path.

An Unavailable trailer cannot be guaranteed to reach a receiver refusing flow-control credit. A connection IO failure/reset on shutdown yields the existing unavailable transport outcome, which already obeys before-row degradation versus after-row fatality. Requiring that bounded termination does not need a new product/lifecycle decision beyond the explicitly authorized stopping-server correction. Do not generalize immediate force-close to ordinary request cancellation or unrelated public listeners.

**Focused proof:** use the existing real peer network lifecycle test owner and an authenticated response whose receiver holds it open without draining after enough output exhausts HTTP/2 credit. Signal server shutdown, keep receiver open, and prove source/lease ownership releases and the peer serving task settles before the normal shared drain budget or query deadline, without resumed receiver reads. Retain the small producer-pause, client-disconnect and post-row-failure cases. No runtime proof was executed here.

## Resolution

Both uncertainties are resolved from actual callers and pinned dependency source. FUP-001 is a narrow lost-error-class conversion; FUP-002 is a distinct connection-lifecycle cancellation gap. They do not share one correction source. Independent validation should inspect and deduplicate these proposals against BEH-R3-2 and SYSTEM-001/C-LIFECYCLE-01. No finding restores the cap or revisits excluded decisions.
