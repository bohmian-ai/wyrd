# Domain Review: Stream Lifecycle, Durability, and Resource Settlement

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `f66a337698940920dca20b126c1c6c28a6390191`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
- Candidate identity was rechecked before this report was written and remained `f66a337698940920dca20b126c1c6c28a6390191`.

## Reviewed boundary

This review traced the client-side Bifrost write and query lifecycles from the public Rust facade through `WriterPool`, `wyrd-queue`, HTTP framing, Arrow IPC decoding, terminal validation, and the Python and TypeScript stream owners. It also inspected the public HTTP and gRPC server adapters far enough to verify drop propagation and frame boundaries, plus the real Python and TypeScript query journeys and the Rust stream/queue unit coverage.

The boundary includes bounded Arrow ownership, buffering and backpressure, flush/shutdown durability, query running/status/cancel controls, schema-once/batch/EOS/row-count/terminal enforcement, broken or abandoned stream cleanup, and foreign-runtime ownership.

## Authority and source coverage

| Authority | Applied rule | Source and consumer coverage |
|---|---|---|
| `AGENTS.md` §§2, 3, 5, 6, 9, 11, 12 | `wyrd-client` is the sole client implementation; `Bifrost` is the SDK-facing facade; async is limited to IO; writes and stream failures preserve durability and explicit errors; user journeys are primary | `crates/shared/wyrd-client/src/bifrost/{mod.rs,facade.rs,handle.rs,query.rs,sink.rs,grpc.rs,blocking.rs}`, `crates/shared/wyrd-queue/src/*`, Python and TypeScript bindings/journeys |
| `architecture/agent-rules.md` | Every user-facing capability has a real journey; no gate may be weakened; struct-centered owner boundaries apply | Facade, queue owner, stream state machine, and recorded task verification |
| `architecture/wyrd-design.md` client model and doctrine 20 | Durable behavior stays server-owned and first-class SDKs project one shared client; complete client→server→client journeys prove the boundary | Rust/Python/TypeScript SDK roots and query journeys |
| `architecture/bifrost-design.md` Query, Resource and failure invariants, Public surface | Bounded transport without full-result spooling; exactly one explicit terminal; cancellation and cleanup release owners exactly once; `wyrd_client::Bifrost` owns query streaming and lifecycle | Query decoder/settlement, server HTTP/gRPC adapters, facade lifecycle methods, foreign-runtime close/error paths |
| `architecture/references/domain/arrow-analytical-interop.md` | `RecordBatch` is the bounded ownership unit; EOF, loss, cancellation, and success remain distinct; cancellation releases foreign and Rust owners exactly once | `QueryIpcDecoder`, `RawQueryStream`, `QueryResultStream`, Python IPC/PyArrow and TypeScript IPC/Arrow projections |
| `architecture/references/domain/analytical-operations-reliability.md` | Governed queues and ownership are bounded; shutdown drains admitted work; cancellation releases descendants | `WriterPool`, `Producer`, byte/cardinality guards, query settlement |
| `architecture/references/domain/olap-serving.md` | Stream bounded Arrow batches with one terminal; never expose failed/truncated output as success | HTTP/gRPC query adapters and client terminal state machine |
| TASK-002 constraints and acceptance criteria | One public Bifrost facade; preserve Oracle bounded ownership and settlement; schema-once/batch/EOS/row-count/identity/one-terminal checks; dropped/broken stream settlement | Complete sources and callers named above |

## Material findings

### STREAM-001 — VIOLATION: query lifecycle remains a second public client surface

- **Violated obligation:** TASK-002 requires all public Bifrost behavior to enter through `wyrd_client::Bifrost`, explicitly forbids exposing `QueryClient` as a sibling client owner, and requires running/status/cancel on the one facade. `architecture/bifrost-design.md` likewise says internal `QueryClient` mechanics are not a sibling public client.
- **Location:** `crates/shared/wyrd-client/src/bifrost/mod.rs:47-50`; `crates/shared/wyrd-client/src/bifrost/facade.rs:477-483`; representative callers at `crates/wyrd/wyrd-cli/src/query/mod.rs:105`, `sdks/wyrd-sdk-python/src/bifrost/mod.rs:438-502`, and `sdks/wyrd-sdk-ts/native/src/lib.rs:607-702`.
- **Evidence:** `QueryClient` is publicly re-exported and `Bifrost::query_client()` publicly returns it as an expressly documented "escape hatch." The facade has `sql`, `sql_as`, and `stream`, but has no inherent `running`, `status`, or `cancel` operation. Production CLI and both foreign SDK bindings therefore call the sibling `QueryClient` directly. `RawQueryStream` is also publicly re-exported despite having no external production caller.
- **Observable consequence:** Rust users cannot obtain the required lifecycle capability set through the promised one facade, while the Python and TypeScript projections depend on a second public owner. The public API therefore preserves the parallel client authority the task requires removing.
- **Required testable correction:** Put raw-request streaming and running/status/cancel/describe operations on the existing `Bifrost` owner, route CLI and foreign bindings through those inherent methods, and remove the public `QueryClient`/`RawQueryStream` escape hatch unless a task-required external caller remains. A Rust public-surface check must compile every required lifecycle operation through `Bifrost` and prove the sibling types are not publicly nameable; existing Python, TypeScript, and CLI journeys must remain green.

### STREAM-002 — INCORRECT: shutdown can miss and accept a concurrently created producer

- **Violated obligation:** TASK-002 requires flush/shutdown durability and preservation of queue drain and settlement. `WriterPool::shutdown` promises to close before draining so new rows are rejected and buffered rows are sent before it returns.
- **Location:** `crates/shared/wyrd-client/src/bifrost/handle.rs:170-183`, `267-289`, and `307-336`.
- **Evidence:** `insert` checks `closed` before acquiring the producer registry. `shutdown` independently stores `closed = true`, snapshots the registry under its mutex, releases the mutex, and drains only that snapshot. An insert that reads `closed == false`, pauses, then resumes in `producer_for` after shutdown has taken an empty or incomplete snapshot can create a new producer, enqueue successfully, and leave that producer outside the terminal drain. No second closed check is made while the registry transition is serialized, and the final `retain` keeps the newly created, non-drained producer.
- **Observable consequence:** A caller can receive successful return from both `insert` and `shutdown` while the accepted row remains buffered and non-durable after shutdown. The supposedly terminal client can retain producer storage and work, contradicting the durability boundary exposed in Rust, Python, and TypeScript.
- **Required testable correction:** Make close admission and producer lookup/creation one serialized owner transition so no producer can be created after the shutdown snapshot and no insert accepted by that transition can escape the drain. Add one deterministic concurrent test that pauses first-producer creation across shutdown and proves either the insert is refused or its row is durably acknowledged before shutdown succeeds, with zero retained producers and dynamic ownership afterward.

### STREAM-003 — INCORRECT: a failed terminal is accepted without proving it is the only terminal

- **Violated obligation:** TASK-002 requires query streams to enforce the one-terminal invariant. `architecture/bifrost-design.md` requires length-delimited terminal-safe streams with one explicit success or failure terminal.
- **Location:** `crates/shared/wyrd-client/src/bifrost/query.rs:761-775`; contrast the clean-EOF validation used only for successful/degraded terminals at `800-834`; current failed-terminal test at `2622-2643`.
- **Evidence:** On a failed terminal, `QueryResultStream::next_batch` immediately retains the terminal, marks settlement complete, and returns `FailedTerminal`. It does not poll the raw stream to clean EOF. A second terminal, batch, or schema after that failed terminal is therefore never passed through `QueryStreamConverter` and is not rejected. Successful/degraded terminals do perform this clean-EOF proof.
- **Observable consequence:** A malformed stream containing a failed terminal followed by more frames is reported as a validated server failure rather than a protocol violation. The client cannot substantiate its advertised exactly-one-terminal contract on the failure path.
- **Required testable correction:** Require clean EOF after a failed terminal before promoting it to retained terminal metadata and returning the failed-terminal result, using the same server-pinned deadline and broken-stream settlement rules as the existing successful-terminal proof. Add focused cases for failed-terminal-plus-batch and duplicate failed terminals; both must return the protocol/incomplete-stream projection and settle once.

### STREAM-004 — VIOLATION: one HTTP body item can materialize an unbounded queue of decoded Arrow batches

- **Violated obligation:** TASK-002 forbids blending away bounded Arrow ownership. `architecture/bifrost-design.md` requires every Wyrd-owned stream and buffer to be bounded and forbids spooling a complete result; `arrow-analytical-interop.md` names one bounded `RecordBatch` as the ownership/backpressure unit.
- **Location:** `crates/shared/wyrd-client/src/bifrost/query.rs:408-425` and `485-541`; misleading bound assertion at `2690-2766`; shared decoder behavior at `crates/wyrd/wyrd-tonic/src/frame_codec.rs:69-110`.
- **Evidence:** `FrameDecoder::push` returns every complete protobuf frame present in an arbitrary response-body chunk. `RawQueryStream` then decodes every returned Arrow batch and pushes all `(frame, RecordBatch)` pairs into an unconstrained `VecDeque` before yielding one item. HTTP does not preserve application frame boundaries, so an intermediary or transport read may coalesce multiple server yields into one body item. The recorded `peak_pending_frame_bytes` measures only the largest Arrow fragment copied through `QueryIpcDecoder`; it excludes all simultaneously retained `RecordBatch` values in `pending`. Existing tests feed one logical frame per body item and therefore do not exercise this path.
- **Observable consequence:** Backpressure is applied per transport chunk rather than per Arrow batch. A coalesced chunk can cause the client to decode and retain many batches at once, with memory proportional to the coalesced result portion rather than the declared single-batch ownership unit.
- **Required testable correction:** Decode and expose at most one complete logical frame/Arrow batch per poll while retaining only bounded undecoded framing state, or enforce an explicit bounded pending-frame/byte admission before decoding additional frames. Add a focused stream test that supplies many valid length-delimited frames in one body item and proves retained decoded ownership never exceeds the accepted bound while preserving schema, row order, terminal, and backpressure behavior.

## Verification limits

- Reviewed the task-recorded passing evidence for `fmt`, `lints`, Python unit/integration/typecheck, TypeScript build/typecheck/unit/integration/N-API, contract checks, and focused CLI/MCP/Rust SDK scenarios.
- The task intentionally did not run the Bifrost aggregate lane. No recorded Rust SDK user journey directly exercises `Bifrost` query lifecycle methods because those methods do not exist on the facade; existing Rust and foreign-runtime tests instead use `query_client()` or wrapper-native calls.
- Existing stream tests cover schema-once, EOS, row counts, clean EOF for successful terminals, incomplete HTTP bodies, cancellation-driven server resource release, and explicit bounded collection. They do not cover a failed terminal followed by another frame, multiple decoded batches coalesced into one HTTP body item, or insert/first-producer creation racing shutdown.
- This review was static and did not rerun the already-recorded lanes. The missing focused proofs above are directly tied to the findings and cannot be inferred from the broader green suites.

## Overall result

**FAIL**

The candidate does not yet satisfy the stream lifecycle/durability boundary: public lifecycle authority is still split, shutdown is not atomic with producer creation, failed terminals do not prove the one-terminal invariant, and HTTP chunk coalescing bypasses the stated per-batch ownership bound.
