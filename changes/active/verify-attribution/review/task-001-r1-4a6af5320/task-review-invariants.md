# TASK-001 remediation invariant review

## Immutable subject and scope

- Base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Prior candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`
- Candidate: `4a6af5320ab8c236fbe640ff0dc5ddf544bdcde2`
- Candidate tree: `036d647413d0acfd1afdebff2b7057b45ac00609`
- Approved authority: `changes/active/verify-attribution/spec.md`, revision 3
- Original task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`
- Prior verdict and ledger: `review/task-001-827ec9f15/{verdict.md,findings-validation.md}`
- Remediation task: `review/task-001-827ec9f15/TASK-001-R1-close-trace-continuity-gaps.md`

I used the supplied navigation map only to locate owners, then independently
traced the current candidate through admission, sealing, retry settlement,
Scribe outage and recovery, timeline decoding, and all three SDK projections.
The candidate commit and tree stayed unchanged during this pass. Review
artifacts are outside the candidate tree.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| REQ-004 / AC-004: execution actor and optional Run/Card attribution; judge calls remain joined | The cumulative candidate retains the verifier-run context, result attribution, and judge-call joins reviewed in the prior candidate; neither remediation nor either merge changes these owners. | Recorded server integration and drift journeys passed before the final merges; the final merges do not touch this attribution path. | PASS |
| REQ-005: valid HTTP, MCP, and gRPC context is the SERVER parent; malformed input is harmless; deferred work links | `serve_request`, the shared HTTP/gRPC context owner, verifier fan-out instrumentation, and deferred-run links remain unchanged from the prior reviewed candidate. | Recorded server, gateway, MCP, and SDK journeys passed before the final merges. | PASS |
| REQ-006 / D4: all SDK transports propagate context; queued writes keep a bounded, stable producer-context set and Gate links it | The propagation and Gate paths are unchanged. `Producer::enqueue_prepared` admits context bytes, and `SealedBatch` keeps the first 32 contexts, but `RecordQueue::seal` attempts to reserve those already-admitted bytes again. See `INV-R1-001`. | The focused queue retry test passed before the merges but does not exhaust admission before sealing; current Python and TypeScript journeys were not rerun after their public projections changed. | FAIL |
| REQ-007 / INV-001 through INV-005: operator sampling is independent; authenticated tenant routing is isolated, allowlisted, bounded, non-blocking, and recovers | Sampler, route registry, tenant copy filter, one process outbox, and the 2,048-permit `ServerSpanSink` are unchanged. The new role-separated cluster test keeps the Oracle server and its `Arc<ScribeOutbox>` alive while the delayed Scribe node joins, proves exactly 2,048 copies, all result summaries, sibling HTTP/gRPC/MCP availability, zero pending writes, and a later accepted copy. | The named outage test passed three times before the merges. The relevant production outbox/telemetry owners were not changed by the merges, but the test was not rerun on the final candidate. | PASS |
| REQ-008: authenticated one-hop timeline is ordered and unique by `(trace_id, span_id)` in Rust, Python, and TypeScript | `Bifrost::trace_timeline` sorts by `(start_time, trace_id, span_id)` and retains the first identity with a local `HashSet`. The strengthened fixture separates duplicate copies with another span. Python now returns the same serialized record as an authored `TypedDict`; TypeScript uses the generated canonical `SpanRecord`; both still call the shared Rust facade. | The focused Rust test passed before the merges. The Python/TypeScript action-timeline journeys have not run after their projection changes. | PASS |
| REQ-009: request identity and trace causality remain complementary and dual export is documented | Both architecture documents retain the approved correlation and dual-export text; no remediation changes it. | Document inspection; post-merge `docs:check` passed. | PASS |
| AC-001 / AC-002: current Rust, Python, TypeScript, MCP/HTTP journey proof, including the connected action timeline | All required journey sources remain present, and the Python test was deliberately updated from generated value objects to the current `TypedDict` keys; the TypeScript facade still returns generated `SpanRecord[]`. | Pre-merge journeys passed, but the public Python and TypeScript timeline projections changed in the two merges and neither timeline journey was rerun on `4a6af5320`. Static format, lint, typecheck, codegen, and docs checks do not execute that runtime boundary. | FAIL |
| AC-003: malformed/absent/unsampled context, foreign tenant, safe attributes, and deferred-link edges | The prior candidate's negative paths and tenant-scoped Oracle query remain unchanged. Merge-era SDK shape changes do not alter server isolation or query authorization. | Recorded focused and journey coverage passed before the final merges. | PASS |
| AC-005 and repository gates | Final-candidate format, lints, typecheck, codegen, and docs checks are recorded green. | Focused remediation tests, owner lanes, Python action timeline, and TypeScript action timeline were not rerun after the merges. | FAIL |
| Prohibited changes: no second Scribe writer/table, trace-read route, presigned propagation, client-controlled operator decision, duplicate attempt row, compatibility alias, or auth/scope change | The cumulative task implementation continues to use the single `ScribeOutbox`, authenticated Bifrost SQL, existing transport guards, filtered operator exporter, and canonical gateway attempt rows. | Source trace; prior boundary checks were green and final codegen/type checks are green. | PASS |
| Non-goals: no sub/act work, Workflow correlation expansion, or general mesh propagation | None appears in the task implementation or remediation commits. | Diff inspection. | PASS |

## Prior-finding closure

| Prior finding | Current source result |
| --- | --- |
| `FIND-TASK-001-1` | **Not closed.** Admission now counts context strings and retained retries remain charged, but sealing reserves the same context bytes a second time. This breaks the pre-existing sealing-headroom guarantee and is `INV-R1-001` below. |
| `FIND-TASK-001-2` | **Closed.** The local identity set removes non-adjacent duplicates after the required total-order sort, and the fixture exercises separated start times. |
| `FIND-TASK-001-3` | **Closed in source.** The existing multi-pod harness is the smallest real reversible Scribe outage seam; it keeps the same Oracle outbox, checks the cap and siblings, joins the Scribe node, drains that outbox, and accepts a later copy. No production fault switch or second outbox was added. Final-candidate execution is still missing. |
| `FIND-TASK-001-4` | **Closed.** `fixed_width` and `stored` each have the correct adjacent rustdoc and `# Errors` section at `verifier_runs.rs:2143-2162`. |

## Proposed findings

### INV-R1-001 — INCORRECT — the context charge is duplicated during sealing, so admitted traced rows can become impossible to flush

- **Violated obligation:** the remediation requires `ClientByteBudget` to be
  the single owner of context bytes from admission through settlement and to
  transfer that charge at sealing. The existing queue invariant also promises
  that admitted rows cannot consume the sealing headroom needed to publish
  them (`producer.rs:83-95`, exercised by
  `admitted_rows_cannot_take_the_sealing_headroom`).
- **Exact locations:**
  - `crates/shared/wyrd-queue/src/producer.rs:545-568`
  - `crates/shared/wyrd-queue/src/queue.rs:531-603`
  - `crates/shared/wyrd-queue/src/queue.rs:672-695`
  - incomplete proof at
    `crates/shared/wyrd-queue/src/producer.rs:1336-1417`
- **Producer-to-consumer evidence:** `enqueue_prepared` includes
  `context_bytes` in the record reservation at lines 552-555, and every staged
  row shares that guard. `seal` then clones the selected contexts and calls
  `reserve_sealing(max_message_bytes + trace_bytes)` while the record guard
  still owns the original context bytes. The configured sealing headroom is
  only `max_message_bytes`; therefore, when admitted work fills the admission
  ceiling, an untraced row can seal but any traced row needs an additional
  `trace_bytes` beyond the total client limit and is restored with
  `Backpressure`. No owner can release the original charge without first
  sealing those rows, so multiple traced producers near the shared ceiling can
  all remain unable to make progress. The focused remediation test deliberately
  leaves roughly half of the admission ceiling unused and therefore cannot
  expose this path.
- **Observable consequence:** a legal burst across one or more traced table
  producers can be accepted and then fail every flush even though the reserved
  frame headroom is available. `shutdown` cannot durably drain that work until
  unrelated ownership disappears, violating the queue's accepted-work and
  bounded-drain contract.
- **Required testable correction:** preserve one charge for one retained
  context allocation and transfer/share that already-reserved owner from the
  staged record into every sealed batch that retains it; do not clone the
  context and reserve its bytes a second time. Only the IPC frame should consume
  the existing sealing headroom. Reuse `ClientByteBudget`, the existing staged
  `Arc` context ownership, and `SealedBatch`; add no limiter, option, or changed
  wire bound. Extend the existing sealing-headroom proof with a sampled context
  while admission is exhausted, and require the flush to publish successfully,
  keep the context charged through an ambiguous retry, and return the budget to
  zero at terminal settlement. This is the smallest proof because it directly
  combines the pre-existing headroom invariant with the remediation's context
  lifecycle; no new harness is needed.

## Verification limits

No Cargo, mise, database, Python, or TypeScript command was run in this
parallel review. The remediation record supplies pre-merge focused and owner
lane results. On the final candidate only format, lints, typecheck,
`codegen:check`, and `docs:check` are recorded. Because the two merges changed
the Python `SpanRecord` runtime shape and TypeScript generated declaration,
the Python and TypeScript action-timeline journeys remain necessary current
runtime evidence even after `INV-R1-001` is corrected.

## Overall result

**FAIL**

The timeline, outage, and rustdoc remediations are correct in current source,
and the merge-era SDK projections are internally coherent. The queue-memory
remediation does not transfer its charge: it double-reserves context bytes at
the only point that must always be able to turn admitted work into a sendable
batch. That reachable liveness regression blocks acceptance independently of
the missing post-merge SDK journey runs.
