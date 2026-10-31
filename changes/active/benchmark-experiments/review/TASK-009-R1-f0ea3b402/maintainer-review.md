# Maintainer review — TASK-009

Overall result: **PASS**. Proposed material finding ledger: **empty**.

## Subject and authority

- Base: `6f38db16c524417301ffc4a5dc0b478daf997ed1`.
- Candidate: `f0ea3b4023e544d0f95e9b5e38d398105dbe2e33`; HEAD remained at that candidate during this review.
- Approved subject: `changes/active/benchmark-experiments/spec.md`, revision 24, especially REQ-035–038 and AC-015–018; TASK-009 revision 2.
- Scope: the shared `scope.md` navigation map, expanded to actual producers, ACK filtering, queue transactions, SQL convenience callers, runner consumers, and focused tests. No prior acceptance was carried forward.
- Rules read: supplied root AGENTS, `agent_docs/index.md`, `rust.md`, `maintainer-style.md`, SQL AGENTS, architecture authority map, owning verification/input-table sections of Wyrd/Bifrost design, relevant accepted-authority section of security posture, and the spec-driven workflow. CodeGraph exploration preceded source search.

This is the ownership/maintainability specialist's result, not the complete task verdict or an attestation of benchmark/publication delivery.

## Changed-surface coverage

| Surface | Source/caller/test coverage | Assessment |
|---|---|---|
| Common work and ACK boundary | `verification/submissions.rs`: `Queued`, `SubmissionSink`, lanes, `write`; `scribe_outbox.rs`: `ScribeWrite::ExecutionInputs`, `Acknowledged`, Scribe lane selection and complete write success path; submitted producer in `components/verification/service.rs::enqueue` | PASS. One existing publication owner and admission writer, with private construction of the ACK value. The wrapper carries metadata and a lifetime claim; it does not own another queue or retry loop. |
| Scheduled preparation | `VerificationScheduler::schedule_tenant`, `selector`, `Step`, `stage`; `DriftEngine::reserve/select`; SQL `claim_due`, `settle_due`, `prepare_scheduled`, `accept_scheduled`, cursor SQL | PASS. Planning/selection remains with the existing scheduler/drift owners. The producer releases its connection before staging; the writer owns accepted execution plus cursor transition. |
| Group preparation | `GroupPoller::reserve/poll/record`; scheduler `hand_off_groups/hand_off`; SQL `claim_due_group`, `prepare_group`, `accept_grouped`, `consume_group/refuse_group`, activity SQL and generation migration | PASS. Existing source query, selection, digest, refusal and incomplete-source behavior remain in their existing owners. New generation metadata supports the approved delayed-admission boundary. |
| In-flight lifetime | `Source`, `InFlightSet::{claim,claimed}`, `InFlight::drop`, and `Queued.claim` through Scribe and admission | PASS. The guard follows staged work and releases through ordinary ownership. Capacity is inherited from the reserved submission slot; it creates no persistent recovery service. |
| SQL API and sibling callers | Complete prepare/accept functions; `enqueue_scheduled`, `schedule_due`, `enqueue_group`, existing occurrence/snapshot lookup; production call search and SQL integration callers | PASS. Lower SQL operations preserve caller-owned transactions. Existing immediate helpers delegate to the same prepare/accept implementation rather than retaining separate inserts. Production background admission goes through the common writer. |
| Execution/input/result consumers | `VerificationRuntime` composition and shared submissions handle; runner `execute/dispatch` and recorded Bifrost input handling; `ResultPayloadBuilder::inputs`; execution-input table marker documentation | PASS. No second runner, input loader, output encoder, or alternate input store was introduced. |
| Shared outbox clock | `OutboxWriter::run`, complete `dispatch`, readiness/linger and `next_wake`, completion/retry interaction, both production sink implementations, added deterministic clock-boundary test | PASS. The fix resides in the existing batching/retry owner and passes one readiness instant through the two decisions. It adds no independent timer/retry mechanism. |
| SDK Verifier borrowing | `WyrdState::bound_verifier`, `BoundVerifier`, all three Observe callers and `Observe::target` | PASS. Borrowing the already typed spec deletes a serialization round trip; existing state resolution and typed error boundaries remain. No binding/export/stub contract changes. |
| Paced upstream response reuse | `Paced`, `PacedUpstream::start`, complete `answer`, existing `sized_completion`/stream frame ownership | PASS. Existing fixture startup prepares immutable buffered bytes once; Axum state cloning shares `Bytes`. Per-call timing/marker recording remains in the handler. |
| Test seams and outcome clarity | Scribe table-specific barrier installation/consumption and server/cluster forwarding; new SQL, server and SDK journey tests named below; adjusted cancellation tests | PASS. The barrier extends the existing deterministic seam. Assertions inspect queue absence, source-state retention, admitted identities and durable transaction outcomes rather than merely response existence. |

## Existing alternatives compared end to end

1. **Extend the submitted-work pipeline or add a background lifecycle.** Before the change, submitted work already reserved both outboxes, archived input through Scribe, and handed success to `SubmissionSink`, which owned batched Postgres acceptance. Background schedulers inserted first and published later. The candidate extends the existing Scribe payload/success loop and the existing writer to all three origins. Selection remains origin-specific, but reservation, publication, successful-row filtering, retry/backoff, tenant commit and runner composition are shared. This is workflow reuse, not merely reuse of an insert helper. Adding scheduler-side admission would duplicate the transaction/retry owner and is absent here.
2. **Use existing immediate SQL enqueue/schedule methods unchanged.** Those methods previously combined preparation and insertion before input publication. They cannot supply the required own-input ACK boundary unchanged. Splitting their existing logic into typed prepare/accept stages is sufficient; the immediate methods now compose those stages, so no parallel SQL insertion implementation remains. Frozen target/authority fields prevent a second mutable resolution after ACK, while submitted target checks continue using the existing cache route.
3. **Use existing cursor/pending state or create a prequeue store.** Existing source state remains the retry point. Conditional cursor movement and a non-reused generation on the existing pending row provide the required atomic admission transition. The local set suppresses repeated preparation only while the existing bounded handoff holds it. No journal, preparation table, scanner, public receipt, or stronger cross-store guarantee is introduced.
4. **Reuse input/engine/result owners or create a new executor.** Schedule/group selectors still use `DriftEngine`, `GroupPoller`, `InputResolver` and `ResultPayloadBuilder`; the unchanged distributed runner resolves archived evidence and uses the existing engine. The enum changes dispatch metadata rather than creating another executor or generic producer framework.
5. **Repair outbox wake selection locally or add new coordination.** Both dispatch and wake selection already belong to `OutboxWriter`; passing one instant corrects their shared decision at that owner. The regression test controls the exact deadline boundary without new harness machinery.
6. **Convert the hydrated spec or borrow it.** `Spec::Verifier` already stores the required concrete type, and Observe immediately borrows it for projection. The removed JSON conversion provided no needed validation or compatibility contract. A direct lifetime-bound borrow is the smaller complete mechanism.

## Focused proof inspected

Source assertions were inspected in:

- `pg_verification_runtime::queued_origins_require_their_own_input_ack`: queue absence and unchanged cursor while the selected input is held, unrelated admission, direct return, and final staged identity.
- `pg_verification_groups::queued_origins_share_batched_admission`: preparation continues while the writer is blocked; repeated passes skip outstanding work; execution and cursor transaction IDs demonstrate a shared mixed-origin commit.
- `pg_verification_groups::failed_handoffs_retry_without_consuming_source_work`: rejected inputs leave sources pending, claims release for new preparation, and a failed writer retries frozen identities/metadata.
- `pg_verifier_executions::acknowledged_admission_preserves_scheduling_state`: rollback, competing occurrence admission, edited cursor, newer/recreated group generations and content replay.
- `pg_verifier_executions::queued_admission_preserves_tenant_and_input_identity`: foreign-tenant admission refusal and staged execution identity.
- `verification_runtime::queued_origins_share_ack_gated_admission`: real client submission, held-input not-found, direct execution, all-origin completion and group readback.
- `outbox::tests::a_lane_short_of_its_linger_keeps_a_wake_at_its_deadline`: deterministic readiness/wake boundary.

The task's recorded verification evidence was available. I did not independently execute Cargo, Postgres, SDK lanes, gates, or benchmarks. This static review found no material maintenance defect or unearned machinery requiring a correction; it does not replace the other selected reviewers' behavioral, persistence, standards, resilience or delivery coverage.
