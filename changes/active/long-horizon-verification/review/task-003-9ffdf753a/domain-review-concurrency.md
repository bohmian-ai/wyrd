# TASK-003 concurrency and resource-admission review

**Overall: PASS for the unchanged TASK-003 concurrency boundary.** No material proposed findings.

Subject: base `ef3a57b0776dd7bc451273b9176498fcca700550`, candidate `9ffdf753ab11ce9b638c906388d3690a6aede249`; complete cumulative three-dot diff. HEAD was checked at completion and remains the candidate. Production source, tests, and configuration were not edited.

## Authority and limits

Reviewed the task, immutable subject, repository rules, reference router, spec-driven development and maintainer guidance, Rust ownership/async guidance, analytical reliability guidance, and applicable Wyrd verification, submission, Scribe-outbox, and audit architecture sections. REQ-006, INV-002/005, and the task's staging/progress/retry/drain obligations govern this boundary.

The subject records an approval discrepancy: TASK-003 targets revision 13, while the available spec identifies revision 14/draft. This report assesses the unchanged TASK-003 obligations and does not approve revision 14. The orchestrator must resolve the authority question for the overall task verdict. Grouped J-005 is expressly deferred until TASK-001 integration; this review does not claim its completion.

## Expanded source and consumer coverage

CodeGraph was used before code navigation. The complete cumulative diff was read, including SDK fixtures and generated/documentation changes; the concurrency investigation expanded beyond its starting map as follows.

| Boundary | Source and consumers inspected | Assessment |
|---|---|---|
| Whole pending ownership | `wyrd-runtime/src/outbox.rs`: `Outbox::{new,stage,reserve,send,settle,shutdown}`, `Reservation::{stage,drop}`, `Admission::release`, `admit`, `OutboxWriter::{run,dispatch,finish,release}` | Admission includes unspent reservations and staged queued/waiting/retrying/in-flight items. Taking an item off the channel does not free capacity. Writer completion/loss frees the relevant share once; retry retains the exact slice and its reserved-item count. |
| Source atomic refusal | `components/verification/service.rs`: `execute`, `enqueue`, `stage_direct`, `staging_full` | Direct execution reserves both writes before the asynchronous engine path. Enqueue obtains the submission reservation and Scribe reservation before staging its input; failure of the second releases the first through RAII. Pressure therefore stages neither part of the refused source call. Authorization's independent audit staging remains permitted. |
| Acknowledgement handoff | `scribe_outbox.rs`: `ScribeWrite::ExecutionInputs`, `ScribeSink::write`; `verification/submissions.rs`: `SubmissionSink::{outbox,write}` | The submission slot remains owned inside the Scribe input item until successful input acknowledgement. Retryable Scribe failure returns before handoff. Terminally lost inputs do not queue submissions. The synchronous post-ACK handoff spends the reserved slot, so a successful source input cannot be discarded because the second queue is full. Queue-writer retries retain the submission slice. |
| Runner resource ownership | `verification/runner.rs`: constructor, `execute`, `settle`, `attempt`, `LeasedWork` implementation; `claims.rs`: `run`, `claim_round`, `claim` | The `JoinSet` cap is checked before a round and before each claim. It bounds tasks through settlement, including tasks awaiting a database connection. Results reserve before engine execution; full capacity produces existing deferred/released semantics without charging an attempt. Cancellation/error/stale-lease paths drop unused reservations. Fenced settlement stages a result once through its slot; settlement retry does not create a second write. |
| Shared Operator sibling | `verification/operators.rs` constructor and shared `ClaimLoop`; `verification/permits.rs` | The new active cap uses the same existing global Operator ceiling, preserving tenant/global permits and their lifecycle. Verifier execution uses a separate loop and ceiling, so Operators do not consume its execution slots. No shared permit service or alternate queue was introduced. |
| Tenant progress and retry | Outbox dispatch/readiness/backoff; `ClaimLoop` round iteration; Rust/Python/TypeScript pressure journeys; runtime cap integration test | Outbox writes remain per tenant with a finite concurrent writer count, and failed slices back off independently. Claim rounds still take at most one runnable execution per tenant before repeating and stop at finite available task capacity. Journeys intentionally admit B while A remains active; they do not claim strict fairness or progress when all resources are occupied. |
| Lifecycle consumers | `app/server.rs` shutdown sequence; `boot/mod.rs` rollback; `wyrd-testing/src/server.rs` teardown | Production drain fences the Scribe outbox before Bifrost and drains the submission outbox afterward, so no live Scribe handoff races closing the submission destination. The test harness follows the real server lifecycle or its sequential in-process equivalent. |
| Configuration and projections | `config.rs`, boot composition, test-server builder, Python/TS native-testing options, error catalog/HTTP projection, SDK journeys/support, changed docs | Capacity reaches the production owner. Client/runtime harness changes project that setting; durable admission remains in Rust server/runtime owners. Error projections carry retry timing. No new per-language resource policy was added. |

## Invariant and failure-path assessment

- `admit` uses an atomic checked update, so competing producers cannot both acquire the same remaining share. A reserved item stays charged when `Reservation::stage` transfers it to the writer; unused slots are released by `Drop`.
- `send` increments pending and reserved-staged accounting before publishing the channel item. The queue's read/write fence prevents a late sender from entering after shutdown closes admission. Failed sends release the acquired share and count loss.
- Failed writes keep items in the writer's failed backlog with their original identity and reservation count; later arrivals remain separate. Waiting, failed, and in-flight ownership cannot each independently admit another unbounded population.
- Direct source work and result construction keep the two-slot reservation across awaits. Any early return/cancellation drops it. Successful direct calls stage both pre-reserved rows without an admission check between them.
- A queued execution under result pressure is deferred before its engine starts. Its durable row remains the existing recovery authority. This does not add an execution receipt or alter accepted abrupt-process-loss semantics.
- Unreserved audit/capture/span writes have a separate finite share and use non-waiting staging. Their overflow logs/counts a loss without entering the invocation's error path. The existing span permits still limit retained span copies.
- Runner claims retain the existing stop race: uncommitted claims roll back; a late committed claim is released unexecuted. Active task ownership ends only when the task completes/reaps or shutdown drains/cancels it.
- The bounded quantities are item/work counts with producer payload limits, rather than measured RSS. TASK-004 owns memory/throughput qualification; this review makes no hardware-capacity or deployment-wide-quota claim.

## Verification actually run

```bash
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::bounded_staging_covers_retrying_and_inflight_work) | test(=outbox::tests::bounded_audit_staging_remains_nonblocking) | test(=outbox::tests::a_retry_writes_the_identical_failed_slice_without_later_items) | test(=outbox::tests::a_failing_tenant_does_not_delay_another_tenant) | test(=outbox::tests::shutdown_flushes_items_that_recover_before_the_deadline) | test(=outbox::tests::shutdown_counts_items_unwritten_at_the_deadline_as_lost)'
```

Result: **6 selected, 6 passed, 62 skipped**; nextest run `c92a0572-968d-476d-ab5f-bebb9604ea67`. This independently verifies the focused accounting/retry/progress/drain cases. The task's other command results are recorded claims, not rerun evidence from this reviewer.

## Findings and remaining proof limits

Material proposed findings: **none**. No local finding IDs assigned.

No live-provider, SDK/server/Postgres journey, broad aggregate, synthetic-load, benchmark, or grouped J-005 run was performed by this reviewer. The relevant journey and runner-cap test source was inspected. These limits do not turn ordinary item-count admission into a throughput or strict-fairness guarantee. Overall task approval remains subject to the independent review reconciliation and the spec-revision authority clarification.
