# TASK-001 invariant review

**Subject:** `d1ec13200d332745af2fed8069a21d5b5c39cb47..f1f1d5ebd264e8f9ac861ec79c6340da44a7a1a8` in `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`, approved spec revision 3, original TASK-001, and R1. The candidate HEAD was unchanged during this read-only pass. `e15c610af` is the separately approved skill edit.

**Result: FAIL.** Two live-source invariants remain unproven or violated by reachable paths. The R1 failure classes and staged async isolation otherwise close the prior issues traced below. Green gate evidence is recorded in R1; I did not rerun it.

## Obligation matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001, AC-001: one request and terminal | `wyrd-spec/src/vala/api.rs`, shared client and server query adapters, Python/TS projections | contract, SDK, CLI/MCP journeys and codegen in TASK-001 evidence | PASS |
| REQ-002, AC-002: discover and select bound live owners | `oracle/mod.rs::discover_live_routes`, `tail_discovery.rs::discover`, planner/live routes | relevant-owner journey | FAIL: INV-R2-01 for mismatched returned node identity |
| REQ-003, AC-003, INV-003: one physical plan with distributed published work and Scribe streaming | `oracle/planner.rs`, `oracle/live.rs`, `oracle/dispatcher.rs`, `scribe/tail_rpc.rs` | published-worker/live-Scribe and filtering journeys | PASS |
| REQ-004, AC-005: availability degrades only before rows; required faults fail | `oracle/live.rs::LiveFragmentRead`, R1 `FollowerResolutionError` and `scribe_start_error`, `tonic_error` | failure-matrix journey, class unit tests; staged decode/schema covered at producer and mapper only | PASS except INV-R2-01's identity degradation |
| REQ-005, AC-004, INV-005: bounded query-owned live resources and cancellation | `peer_service.rs::execute_scribe_fragment`, `LiveTailBatches::into_stream`, `StagedRunWindows` | backpressure/>30-second journey; cancellation unit test | FAIL: INV-R2-02 for decoded window after Scribe admission drops |
| REQ-006, INV-001: publication overlap, ACK and durability unchanged | staged lease, publication architecture, no new write path in diff | source inspection and Bifrost gate | PASS |
| REQ-007, AC-006, INV-004: verification shares query and accepts only terminal | `query/scheduled.rs::consume_to_terminal`, Drift caller | live-row Drift journey and failed-terminal unit test | PASS with preexisting end-to-end failed-Drift verification limit |
| AC-007: tail fence removed, listing retained | deleted `oracle/tail_fence.rs`; retained authenticated listing | source inspection and Bifrost gate | PASS |
| AC-008 and non-goals | R1 reports `verify:bifrost` 9/9 and full gate 48/48; diff adds no second planner, public mode, persistent index, or ACK change | recorded sequential lane results | PASS |
| INV-002: auth, tenant, signed binding and source identity | `grpc/scribe_tail.rs`, `tail_discovery.rs`, peer preflight | ticket-refusal journey and unit status mapping | FAIL: INV-R2-01 |

## Proposed findings

### INV-R2-01 — A different Scribe node identity is treated as transient source loss

**Class:** INCORRECT / VIOLATION. **Obligation:** REQ-002's node-incarnation binding, REQ-004's fatal security/binding/protocol faults, INV-002, AC-005, and R1 FIND-TASK-001-1.

`wyrd-server/src/oracle/tail_discovery.rs:201-205` receives each stream identity from the Scribe RPC and compares it to the selected roster node and writer epoch. It collapses **either** a different node ID or a changed epoch to `TailReadError::StaleIdentity`. `vala-bifrost-redux/src/oracle/mod.rs:3520-3537` retries that class once, then marks `listing_lost` and degrades the query. Thus a ready authenticated endpoint returning a route for a different node is accepted as best-effort loss, even though it violates the signed owner binding. The route is produced by `ScribeTailGrpc::list_active_streams` from `self.source.stream()` (`grpc/scribe_tail.rs:115-145`); a mismatched source identity is reachable if service wiring and the ready roster disagree, and the explicit check exists to detect exactly that state. The R1 journey injects a missing ticket and an outage, not a mismatched returned identity.

**Consequence:** published rows can receive a valid Degraded terminal after a peer identity/binding fault. **Correction:** keep the transient writer-epoch retry, but classify a returned node mismatch as a fatal identity/binding fault at discovery before it reaches Oracle's availability branch. A focused discovery test should return a wrong node ID and prove query failure; retain the stale-epoch retry and outage Degraded proofs.

### INV-R2-02 — Detached staged decode runs after its Scribe resource grant is released

**Class:** INCORRECT / VIOLATION. **Obligation:** REQ-005, INV-005, AC-004 and the TASK-001/R1 query-owned resource contract.

`wyrd-server/src/oracle/peer_service.rs:313-351` acquires `ScribeFollowerLease` (one concurrency permit and a root-accounted memory quantum per `resources.rs:2152-2182`) and holds it only inside the returned async output stream. `scribe/staged_tail.rs:178-213` moves a one-window Parquet decode into `spawn_blocking`; after the async reader is cancelled, that blocking task continues with its staged lease until the decode exits. When the stream drops during that step, `peer_service`'s follower grant drops immediately while the detached decode can still create a `RecordBatch`. The new unit test `tail_rpc.rs:1463-1524` explicitly proves the staged lease persists after cancellation, but does not couple or observe the admitted Scribe grant. Repeated cancelled reads can therefore leave decoding work and batches outside the Scribe concurrency and memory accounting while new reads are admitted; the one-window limit is per cancelled query, not a process bound after its permit returns.

**Consequence:** cancellation can oversubscribe Scribe's bounded follower execution even though each live window is individually bounded. **Correction:** retain the existing Scribe follower admission until the in-flight blocking window completes or is otherwise safely stopped, while still cancelling further windows immediately; bind that grant to the in-flight step at the existing stream/admission owner, without adding an independent deadline or durable lease. A controlled cancellation check should hold a staged window, cancel its client stream, show that another follower cannot take the released slot/bytes yet, release the window, then prove admission returns and no next window starts. Preserve the current runtime-responsiveness and staged-file-lease checks.

## Prior finding closure and limits

R1 FIND-TASK-001-1 now preserves gRPC listing classes for credential/ticket/tenant/binding/state; the unresolved node-identity case above shares that failure boundary. FIND-TASK-001-2 now carries source-loss, capacity, and fault classes through Scribe follower start and stream errors, with public capacity and ticket refusal journeys. FIND-TASK-001-3 moves synchronous staged open/decode to the blocking pool and retains the staged file lease through one in-flight window; its admission coupling is the remaining resource issue above. FIND-TASK-001-4 through -6 are direct documentation/import/signature edits and appear closed. Schema and corrupt-staged-file failures remain unit-only, but the same Scribe error mapping is exercised by the public journey's fatal path; failed Drift query to no verdict remains unit-only with a traced rejecting caller. No new public or architectural decision is needed for either proposed correction.
