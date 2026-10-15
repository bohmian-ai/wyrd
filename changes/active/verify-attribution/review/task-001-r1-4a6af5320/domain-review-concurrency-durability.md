# Concurrency, resource-ownership, and durability review

## Subject and boundary

- Original base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Prior candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`
- Cumulative candidate: `4a6af5320ab8c236fbe640ff0dc5ddf544bdcde2`
  (tree `036d647413d0acfd1afdebff2b7057b45ac00609`)
- Authority: approved `changes/active/verify-attribution/spec.md` revision 3,
  original TASK-001, the prior validated ledger, TASK-001-R1, `AGENTS.md`,
  `architecture/bifrost-design.md`, and the telemetry/testing references routed
  by the repository.
- Sensitive domain reviewed: client queue byte ownership and settlement;
  server-span permit ownership; Scribe-outbox outage, retry, recovery, and
  sibling availability. The later merges were checked where they touched the
  queue, server integration test, and cluster/test-server harness.

CodeGraph was used first to map `Producer::enqueue_prepared`,
`ClientByteBudget`, `RecordQueue::seal`, `SealedBatch`, `ServerSpanSink`,
`ScribeWrite::ServerSpans`, the generic `Outbox`, and the role-separated
cluster lifecycle. Their owning implementations, callers, and focused tests
were then inspected at the candidate.

## Boundary coverage

| Boundary | Source and ownership evidence | Result |
| --- | --- | --- |
| Context admission | `Producer::enqueue_prepared` includes the context strings in the record's one `ClientByteGuard`, shared by all rows of that logical record (`producer.rs:545-568`). Failed admission drops the newly built owner and does not enter the unbounded channel. | PASS |
| Split/seal and first-32 selection | `producer_traces` selects the first 32 identities per chunk in admission order. A selected context is cloned into `SealedBatch`; unselected contexts leave with their rows. The sealed guard includes frame plus selected context bytes (`queue.rs:561-603, 672-696`). | FAIL: the overlap reservation can permanently prevent a full admitted traced staging set from sealing; see `CD-001`. |
| Retry, ACK, refusal, and shutdown | `SealedBatch` is non-cloneable and its guard moves through fresh, in-flight, and `RetryEntry` states. Retry borrows the same batch and keeps its `RetryPermit`; ACK consumes it, terminal refusal drops it before loss observation, and task shutdown/drop retains or finally drops the same owner. | PASS apart from the pre-seal liveness defect in `CD-001`. |
| Span permit admission | `ServerSpanSink::stage` uses non-waiting `try_acquire_owned`; overflow increments the existing Scribe loss counter and returns without reaching the request (`scribe_outbox.rs:640-687`). | PASS |
| Span permit lifetime | The `OwnedSemaphorePermit` is stored only in `ScribeWrite::ServerSpans`. Generic outbox waiting, in-flight, and failed retry slices own the same item; successful write, terminal consumption, post-shutdown staging refusal, and deadline abandonment all drop the item once. Audit, capture, and result variants carry no permit. | PASS |
| Same-outbox outage recovery | The integration test boots the existing role-separated topology with the final Scribe slot absent, retains the Oracle process's actual `scribe_outbox`, drives traced HTTP verification until overflow, probes gRPC and MCP, starts that Scribe slot, settles the retained Oracle outbox, verifies exactly 2,048 copies and every result, then proves a later copy is admitted (`pg_verification_runtime.rs:3547-3662`). This is the production peer route and not a second outbox or production fault switch. | PASS by source; current-tip execution remains a verification limit below. |
| Merge effects | The first merge changed the shared test-server credential setup and several Bifrost fixtures; the second changed SDK auth surfaces. Neither changed queue accounting, span permit ownership, the role-separated node order, delayed-final-node boot, or the outage test after its integer metric-parser correction. | PASS by source. |

## Material finding

### CD-001 — INCORRECT — traced work admitted at the byte ceiling can never seal

- **Violated obligation:** `FIND-TASK-001-1` requires the existing
  `ClientByteBudget` charge to transfer from admitted rows to the selected
  sealed contexts through settlement. `QueueConfig` also promises that the one
  `max_message_bytes` admission headroom lets every admitted row set seal
  (`config.rs:9-17, 50-76`).
- **Exact locations:**
  `crates/shared/wyrd-queue/src/producer.rs:545-568` and
  `crates/shared/wyrd-queue/src/queue.rs:561-603`.
- **Evidence:** admission already charges each record's context in `used`.
  Sealing clones the selected contexts while the row owners are still live,
  then asks `reserve_sealing` for `max_message_bytes + trace_bytes`. The budget
  reserves only `max_message_bytes` as sealing headroom. Therefore a set of
  traced rows that validly fills `admission_limit()` leaves exactly
  `max_message_bytes` free but sealing asks for strictly more. On refusal,
  `seal` restores every row with its original guards, so no ownership is
  released; every background or explicit retry encounters the same arithmetic.
  The existing headroom regression test fills the admission ceiling only with
  an untraced row (`producer.rs:2033-2068`), while the new retry test
  intentionally uses roughly half of the admission ceiling
  (`producer.rs:1336-1418`), so neither reaches this state.
- **Reachable consequence:** a normal traced producer can accept work up to its
  documented budget and then return `Backpressure` forever from `flush` and
  `shutdown`; dropping the handle also loops trying to drain the unchanged
  staging set. The accepted rows, their contexts, and their byte guards remain
  resident without ever reaching the sink. This is a liveness and bounded
  resource-lifecycle regression introduced while closing FIND-TASK-001-1.
- **Smallest testable correction:** keep `ClientByteBudget`, the existing
  first-32 rule, queue limits, and wire representation, but transfer the
  selected contexts' existing charged ownership from staging to the sealed
  batch instead of requiring the same context bytes a second time from the
  fixed frame headroom. Do not raise the configured budget, enlarge permanent
  headroom, add a limiter, or weaken context accounting. Extend the existing
  sealing-headroom proof with sampled contexts that fill the admission ceiling
  and assert one flush publishes, followed by zero owned bytes after terminal
  settlement. Retain the ambiguous-retry assertions to prove the transferred
  context charge stays with the exact sealed batch.

## Prior-finding assessment

| Prior finding | Assessment |
| --- | --- |
| `FIND-TASK-001-1` | **Not closed.** Contexts are now counted at admission and retained retry, but the implementation duplicates their reservation during seal instead of transferring it, creating the permanent full-budget state in `CD-001`. |
| `FIND-TASK-001-3` | **Closed by source.** The real process outbox is capped during an absent peer Scribe, sibling result/request paths continue, the same outbox drains after that Scribe joins, and item ownership releases permits. The multi-pod harness is the smallest existing reversible production-route seam and does not alter production behavior. |

## Verification limits

- I did not run Cargo, mise, Postgres, or journey lanes, per the review
  orchestration constraint against parallel shared lanes.
- The remediation record reports the queue accounting test and outage test
  green before the two merges, including removal-based RED evidence.
- On candidate `4a6af5320`, format, lints, typecheck, codegen, and docs checks
  are recorded green, but the named queue test, outage integration test, and
  SDK journeys were not rerun after the merges. The merge diff does not change
  the production owners relevant to this report, but current-tip runtime proof
  is still absent.

## Overall result

**FAIL** — `CD-001` is a reachable liveness/resource-ownership defect in the
remediation for `FIND-TASK-001-1`. The span permit and same-outbox recovery
remediation for `FIND-TASK-001-3` is otherwise sound and appropriately simple.
