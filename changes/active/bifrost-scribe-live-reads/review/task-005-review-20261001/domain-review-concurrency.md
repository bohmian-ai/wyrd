# TASK-005 concurrency and cancellation domain review

**Subject:** `05d7d7413..885d16c11` (excluding `1f1cbcf5f`). **Result: FAIL.** Source remained immutable.

## Boundary and authority coverage

| Boundary | Source and authority inspected | Assessment |
|---|---|---|
| Oracle queue, admission, and stream | `oracle/admission.rs`, `oracle/mod.rs` (`start_query`, `admitted`, `Drop`, `admit_built_attempt`), `oracle/query_stream.rs` (`QueryStreamLifecycle`, `polled_in_span`, terminal finish), `gate/mod.rs` dispatch, `oracle/dispatcher.rs` peer stream, published/capacity/peer journey changes; task query dashboard contract, `architecture/bifrost-design.md` telemetry and admission sections | Waiting and admitted gauges have separate owners. Stream polling retains the query span, and Gate lifecycle retains the public stream span. No material concurrency or cancellation regression established. |
| Scribe lanes | `scribe/execution_lanes.rs` ingress, persistence, and WAL submit/worker paths and `record_lane_*`; task `lane_queued` contract; `architecture/bifrost-design.md` measurement meanings | Queue gauge now decrements at worker start, and active gauge decrements on caught panic as well as normal exit. Detached work retains its resource owner. |
| Scribe staged ownership | `scribe/staging_runtime.rs` registration, claim take/residue, restore, settlement; `scribe/assembly.rs::backlog`; staging telemetry and restart test; task staged backlog contract; `architecture/bifrost-design.md` telemetry section | The assembler is the correct source, but its snapshot can be published out of order under concurrent transitions. Finding CONC-1. |
| Shared storage | `storage/mod.rs::StorageRequestGuard`, `storage/cache.rs` registration, retention, close, inspection, `storage/telemetry.rs` gauge functions; cache concurrency tests; task logical-request contract | Request gauge follows the settlement guard; cache state gauges are published while the cache state lock is held. No material regression established. |

Applicable rules: `AGENTS.md` §§6, 10–12 (bounded async ownership, accurate observability and required journeys), `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md` (cancellation, admission, telemetry), and the focused testing, Rust core, OLAP, and analytical reliability references. The approved spec's REQ-012/INV-009/AC-014 and the original task require live waiting/active/backlog series to reflect production owners without a telemetry-only ledger.

## Finding

### CONC-1 — staging backlog publication can regress behind its owner

**Classification:** INCORRECT. **Violated obligation:** the four staging gauges must show current assembler-owned ready and claimed members after durable registration, claim take, settlement, and restore; a stalled publication must expose its actual backlog and eventual zero.

**Location:** `crates/vala/vala-bifrost-redux/src/scribe/staging_runtime.rs:165-181`, called after releasing the `assembly` mutex at lines 241–247, 310–320, 402–411, and 784–791. `StagingBacklog::publish` sets the four gauges from that detached snapshot.

**Reachable sequence:** Concurrent shard staging or claim/settlement calls share the runtime. Transition A mutates the assembler, then `publish_backlog` snapshots state A and releases the mutex. Before A sets gauges, transition B mutates the assembler, snapshots and publishes state B. A resumes and sets older state A last. Because no further transition is required, the series may indefinitely show a missing member, a settled claim still outstanding, or nonzero backlog after settlement, while the assembler and durable stage have the correct state. This is the same read-then-set race that `execution_lanes.rs` explicitly avoids with additive updates and that `storage/cache.rs` avoids by setting state gauges under its state lock.

**Observable consequence:** an operator can miss a live publication backlog or see a phantom one. The tests inspect serial transition results and restored snapshots; they do not establish ordering under concurrent transitions.

**Testable correction:** Keep each assembler mutation and its owner-derived gauge publication in one serialized critical section, using the existing `assembly` mutex and `StagingBacklog`; do not add a second ledger. Ensure every mutating path, including claim take and settlement, goes through that boundary. A focused concurrent transition check should end with gauge values equal to `runtime.backlog()` after both operations settle.

## Verification limits

The candidate task records passing focused Scenarios 1–4, touched module tests, the Scribe write/read journey, formatting, lints, docs, and diff checks. I inspected the tests and changed paths rather than rerunning lanes in this independent static review. The requested benchmark, broad gate, and whole journey lanes were not run; their absence is recorded in task evidence and cannot prove this interleaving.
