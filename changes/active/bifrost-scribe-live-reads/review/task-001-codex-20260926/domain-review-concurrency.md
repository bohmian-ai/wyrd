# Domain review: live stream concurrency and resource ownership

Subject: `d1ec13200d332745af2fed8069a21d5b5c39cb47..f9115fbbf6b6f116cf5ec5fe5582a9543107955a` (HEAD verified at candidate). Reviewed the complete diff inventory, approved spec revision 3, TASK-001, `AGENTS.md`, `architecture/agent-rules.md`, `architecture/bifrost-design.md`, and the DataFusion and analytical-reliability references. Boundary traced from Oracle `LiveScribeExec` and `LiveFragmentRead` through dispatcher, private peer RPC, Scribe follower, `LiveTailBatches`, staged Parquet reader, and terminal stream; inspected Oracle journey and unit coverage.

| Boundary obligation | Source and verification | Result |
|---|---|---|
| Selected live fragments are pull driven and bounded | `oracle/live.rs` partitions and `LiveFragmentRead`; `scribe/tail_rpc.rs` `LiveTailBatches`; `distributed::live_stream_backpressure_and_query_owned_lifetime` | PASS for the exercised in-memory path |
| Drop, cancel, and completion release snapshot and admission | `LiveTailPartition` owns `LiveTailBatches`; Scribe peer output owns follower lease; Oracle stream owns query admission; above journey checks producer and lease release | PASS for the exercised in-memory path |
| Query deadline is the only live-read lifetime bound | Oracle `LiveFragmentRead` selects cancellation and deadline while polling frames; no 30-second tail lease remains | PASS for the exercised path; staged blocking read caveat below |
| Capacity, schema, and local execution faults fail rather than degrade | `ScribeTailResolver::resolve` and `execute_scribe_fragment` collapse source errors into availability; Oracle `LiveFragmentRead` degrades pre-row availability | FAIL |

## Proposed findings

### CONC-001 — Scribe capacity and staged-read faults become degraded completion

- **Classification / obligation:** INCORRECT; spec REQ-004 and AC-005 require resource and schema faults to fail, with degradation limited to a genuinely unavailable live Scribe before rows.
- **Location:** `crates/vala/vala-bifrost-redux/src/oracle/follower.rs:1042-1054`, `crates/wyrd/wyrd-server/src/oracle/peer_service.rs:327-335,356-371`, `crates/vala/vala-bifrost-redux/src/oracle/live.rs:493-512,531-547`.
- **Evidence:** The bounded memtable collector returns `ScribeError::IngestBusy` when the signed max batch or byte ceiling is exceeded (`scribe/memtable.rs:1825-1843`). `open_live_batches` propagates it to `ScribeTailResolver::resolve` as a string; the Scribe peer maps every `PhysicalPlanFollowerError::Resolution` to `EligibleSourceLoss`, whose gRPC `FailedPrecondition` is classified as availability and yields `Degraded` before rows. The staged reader also returns `ScribeError::Internal` for a missing required column (`scribe/staged_tail.rs:110-127`); the Scribe peer maps any non-tenant, non-stale stream error to `Unavailable`, which Oracle likewise degrades before rows. These are reachable production paths, not fault-hook artifacts.
- **Consequence:** A query can return its complete published portion as `Degraded` with `LiveTailUnavailable` despite a local capacity refusal or invalid staged schema. The terminal misstates the fault and allows verification to score a resource/schema failure as best-effort evidence.
- **Testable correction:** Preserve closed resource and schema/error identity from Scribe producer through follower resolution and stream status, and classify only real Scribe availability as degradable at Oracle. Reuse the existing `DispatchError::Capacity` / terminal classification and existing peer status mapping; avoid a new query mode or retry path. A focused production-shaped journey should force a live snapshot bound refusal and assert `Failed` with client rejection; a staged-schema fault can use a narrow integration check if a real-server fixture is impractical. Check both pre-row resolution and pre-row stream failure paths.

### CONC-002 — Staged Parquet IO blocks the async fragment stream

- **Classification / obligation:** VIOLATION; AGENTS.md §6 forbids blocking inside async request paths without an explicit blocking strategy, and spec REQ-005 requires cancellation and deadline to release live ownership.
- **Location:** `crates/vala/vala-bifrost-redux/src/scribe/tail_rpc.rs:634-644`, `crates/vala/vala-bifrost-redux/src/scribe/staged_tail.rs:59-78`, `crates/wyrd/wyrd-server/src/oracle/peer_service.rs:343-356`.
- **Evidence:** Polling `LiveTailBatches::into_stream` directly calls `std::fs::File::open`, synchronously parses Parquet metadata, and advances a synchronous Parquet iterator to decode the next window. This happens under the async Scribe peer output's `batches.next().await`. No `spawn_blocking` or async file boundary appears on this path. Oracle's cancellation and deadline `select!` can win only after that poll yields; a slow staged-file read can occupy a Tokio request worker and retain the Scribe lease and staged references past cancellation/deadline.
- **Consequence:** Slow local storage can stall the server runtime and delay resource release and deadline failure for a live query; the current 30-second, cancel, and drop journey exercises only memtable rows.
- **Testable correction:** Keep the same query-owned bounded producer and leases, but perform blocking Parquet open/decode in an explicit bounded blocking strategy that lets the async stream observe cancellation and deadline and drop its ownership promptly. Add one focused staged-source cancellation/deadline check without a new general scheduler or unbounded prefetch.

## Verification limits and result

The task reports green `verify:bifrost`, Oracle journeys 33/33, and full gate. I did not rerun those lanes because this is an immutable acceptance review. Existing lifecycle journey uses unflushed memtable rows; the staged reader tests exercise decode and leases but not cancellation or resource-terminal classification. **FAIL** due to CONC-001 and CONC-002.
