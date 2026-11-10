# Maintainer review — TASK-001

Overall: **FAIL**. Two proposals require independent validation. No source changes or Cargo commands were run.

Subject: base `0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb`, candidate `cd905f4cd929785bda25696326d58b4a4ae88dee`; HEAD still equals the candidate. This is a first review of the complete range, including Revision 1, with no acceptance carried forward. Scope is substantial ownership changes, shared writer extension, replacement history/publication/inspection workflows, and their projections and test maintainability. Other reviewers' reports were not read.

## Coverage and existing alternatives

Read the task including Revision 1, subject map, root rules, SQL rules, maintainer-style and Rust guides, spec-driven workflow, and the execution-state owner sections of Wyrd/Bifrost design. Navigation started with CodeGraph, then expanded through the diff and callers. Verification claims in the task were treated as claims; reviewed tests provide static proof assessment, not newly executed results.

| Changed surface / full workflow | Source and caller coverage | Maintenance assessment |
| --- | --- | --- |
| RunState, StateScribe, Held, Pending, Acceptance | Entire `verification/state.rs`, including `claim/fresh/accepted/read`, live transitions, ticket coverage, publication receipts, terminal selection, Checkpoints implementation and all four unit tests; direct service and runner composition | Clear dependency ownership and one pending operation. `StateScribe` is justified by shared future ownership: it retains write dependencies while RunState retains the memory authority. Held/Pending are bookkeeping values, not parallel writers. Preparation remains outside the established bounded owner: MNT-001. |
| Existing engine/ledger versus new adapter | `VerificationEngine::run`, `attempt`, `with_checkpoints` callers; Checkpoints interface; runner recovery mapping and removed ExecutionCheckpoints | The existing engine still owns scheduling, dependencies and attempt policy. The replacement implements its existing checkpoint interface; no second walker or execution scheduler was added. Existing ledger alone does not provide durable receipts, completed File custody or selected terminal identity. Keeping that interface and replacing the SQL adapter is warranted by the approved durable-state change. |
| Shared writer extension | `ScribeWrite`, `ScribeSink::frames/chunks/submit/deliver/tally/lane/weight/write`, frame/chunk state propagation and peer projection; unchanged Outbox reservation owner | Per-execution lanes extend the existing mechanism. Per-frame state receipts follow the nearby Object acknowledgement pattern and preserve the Result acknowledgement after group scheduling. Stable write keys extend existing chunk identities. No independent queue/actor/flush task. MNT-001 concerns the preparation preceding these mechanisms. |
| Acceptance and replay | Complete new `scribe_state_heads` module; changed batch record path and surrounding `ShardOwner::append_and_sync_batch_commits`, `commit_batch_control_fences`, `refuse_state`, `resolve_replay_fence` | State-head advance extends the existing batch-fence transaction and existing replay suppression. It does not introduce an application recovery mirror or alternative WAL. New state-head SQL is necessary for writer/revision ordering that existing batch-id dedup does not supply. Concurrency and durable-data correctness are independent specialist boundaries, not waived by this ownership assessment. |
| Deleted SQL/history workflow | Base EventProjector and old ExecutionCheckpoints, deleted SQL event/inspection/publication owners, payload-removal migration, current runner and inspection readers | SQL projection/backlog and SQL invocation/publication custody are removed rather than retained as competing authorities. Reusing the old projector would retain the forbidden SQL payload authority and background projection stage; it cannot meet accepted-state recovery semantics. Queue claims/renewal/result completion remain with the existing queue owner. |
| Hosted publication and File workflow | `Publications::json/path/complete/receipt/accept/publication`, `published_object`; RunState receipt/budget logic; TenantEvidence changed binding/publications/published; complete Mount::publication and HostedBlocks::engine composition | Existing workspace freeze, File upload/reconcile, ObjectInventory acknowledgement and exact-descriptor reads are reused as complete workflows. New acceptance/accounting belongs with the shared state rather than another storage service. Evidence and inspection share `published_object`; no second digest-location resolver. |
| Inspection and public projections | Complete changed cursor/page/invocation, authorization/accepted, invocation/publication page, publication/select/file/download/content/cancel flows; InputResolver::state and ResultPayloadBuilder checkpoint/build_as consumers | Public reads project accepted state while the queue retains status and authorization identity. Pure paging/projection helpers are caller-owned transformations, not misplaced services. No SDK workflow is moved into the server UI or duplicated into a new API. Generated StateRefused error-code projection is mechanical and introduces no alternate behavior. |
| Test/fixture maintenance | Entire state unit-test module; shared TableGate changes; outbox frame/lane tests; Leased fixture; direct/queued, accepted-recovery, exact descriptor, terminal-inspection and batched-checkpoint tests; VerificationFixture changed claim/read/publication helpers and their callers | Tests use existing server/SQL/outbox seams; no new general harness or dependency. Leased needs a real HeldLease while the general fixture explicitly supplies no lease confirmation, so their differing setup is purposeful. Batched integration proof directly drives Checkpoints, so it does not independently demonstrate engine-level A/B→C dependency scheduling. That proof limit belongs in task acceptance assessment; it is not a reason to invent another harness. One mandatory rustdoc gap is MNT-002. |

## Proposed findings

### MNT-001 — checkpoint preparation bypasses the bounded writer preparation owner

Classification: **VIOLATION**.

Changed location: `crates/wyrd/wyrd-server/src/verification/state.rs:248–279`, `StateScribe::write`; related existing owner `scribe_outbox.rs:1590–1700`, `ScribeSink::submit/deliver`.

Governing requirement: Revision 1 selected correction 6 explicitly requires preparing frozen checkpoint JSON/Arrow through the existing bounded writer preparation path. `agent_docs/rust.md`, Async, also says never to block inside an async request path without an explicit blocking strategy. The task permits snapshot copying under the memory lock; this finding does not challenge that approved cost.

Source-backed path: every direct and queued Checkpoints mutation and every accepted publication/terminal selection reaches `RunState::durable`, whose shared future polls `StateScribe::write`. Before its first capacity await, this function serializes the whole ExecutionState with `serde_json::to_value`, builds the checkpoint, and encodes its Variant (`ResultPayloadBuilder::checkpoint` → `variant` → `EncodedVariant::from_serialize`). Only after that work does it call `reserve_until`. Later, ScribeSink::deliver performs Arrow preparation on the blocking pool under submit's `FRAMES_IN_FLIGHT` limit. Consequently the existing bounded preparation stage protects the later Arrow work but not the complete-state serialization/Variant preparation.

Concrete maintenance/runtime cost: there are now two preparation boundaries for the same checkpoint: request/engine polling performs the growing full-state conversion, while the writer bounds only the remaining Arrow stage. A maintainer changing the writer preparation limit or reasoning from it cannot bound this added preparation work. A supported run accumulating settlements/publications repeatedly performs complete-state CPU work on a Tokio request/runner task before writer admission; releasing the memory lock does not make that task yield. The current static path violates the explicit selected constraint regardless of whether the small held-ack tests expose a latency difference. No throughput/starvation magnitude is claimed without measurement.

Smallest correction: extend the existing Scribe adapter's bounded preparation path to own frozen checkpoint JSON/Variant preparation as well as Arrow encoding. Keep the same RunState snapshot/ticket/shared receipt, one pending checkpoint, stable identity, expected-next fence, existing reservation/byte accounting and deadlines. Do not add a new writer, per-run task or independent unbounded spawn per caller. Merely moving serialization outside the mutex is insufficient because it is already outside it; a standalone spawn would still bypass the specified bounded preparation owner. This is a local preparation-boundary correction, not a new acceptance protocol or state format.

Focused proof: through the existing controlled writer seam, show checkpoint preparation uses its bounded preparation admission, another execution can progress while that preparation is held, and the existing exact-coverage/cancellation/frame-receipt tests still hold. A machine-specific latency benchmark is unnecessary to establish placement and boundedness.

### MNT-002 — new format-compatibility test omits mandatory rustdoc

Classification: **VIOLATION** (mandatory repository gate).

Changed location: `crates/wyrd-spec/src/verification/state.rs:139–160`, `tests::only_the_current_format_decodes`.

Governing rule: root AGENTS Code requires rustdoc on every new/changed Rust item and states missing rustdoc blocks merge. `agent_docs/rust.md`, Rustdoc, explicitly includes tests and requires `# Panics` when applicable.

Evidence: the newly added test starts immediately with `#[test]` and has no rustdoc. It contains `expect("state serializes")` and assertions. Its surrounding domain state types are documented, and the nearby newly added `verification::state::tests` use outcome-oriented test rustdoc with `# Panics`, showing the established correction shape.

Concrete cost: the persisted-format compatibility proof is a required recovery-safety test; it lacks the mandatory statement of what incompatibility/malformed-state outcomes it pins, and its panic contract. This is an explicit gate, not a request for more stylistic prose elsewhere.

Smallest correction: add a concise outcome-oriented rustdoc and `# Panics` to this test. No production code or additional tests are needed. Batch with other validated documentation corrections rather than creating a separate style round. Static inspection establishes closure.

## Limits

This was a read-only maintenance/ownership review, not a complete security, concurrency or recovery verdict. No optional redesigns are proposed. Cargo tests, format, lints and codegen were not rerun; recorded task results remain the available execution evidence.
