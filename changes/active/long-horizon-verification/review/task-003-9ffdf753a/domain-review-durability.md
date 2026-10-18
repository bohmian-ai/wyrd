# TASK-003 durability review

Overall: **PASS for the common TASK-003 durability obligations**, subject to the unresolved specification-authority limit below. No material source-local durability findings.

Subject: complete cumulative range `ef3a57b0776dd7bc451273b9176498fcca700550...9ffdf753ab11ce9b638c906388d3690a6aede249`. HEAD remained the candidate at completion. Production source was not changed.

## Authority and boundary

Reviewed source admission through in-memory Scribe ownership, Scribe acknowledgement, submission ownership, PostgreSQL acceptance/replay, queued-result settlement, and orderly/expired shutdown. Applicable authority was AGENTS.md, agent-rules.md, TASK-003, the shared REQ-005/006 and INV-001–005 obligations, Wyrd design's verification/submission and audit boundaries, Bifrost design's server-internal write/ACK boundary, and the reference router's architecture patterns and analytical operations/reliability guidance. Spec-driven development and maintainer-style references were also read.

The packet requests approved revision 13, while the checked-in specification identifies revision 14/draft. This report does not approve draft revision 14 or resolve that discrepancy. Its implementation assessment uses the unchanged TASK-003/common durability requirements; the orchestrator must preserve the authority limit in its final verdict.

## Coverage and source assessment

| Obligation | Producer, owner, sibling and sink evidence | Assessment |
|---|---|---|
| Refused direct calls stage no source rows; accepted direct work retains staging capacity | `components/verification/service.rs:486` reserves two Scribe items before execution. `stage_direct` constructs both payloads before sending either, then stages both through the reservation (`:715–739`). A failed reservation returns the stable typed pressure error and does not stage source input/result rows. Authorization audits may already exist, as required. | PASS |
| Refused enqueue has no partial first/second-outbox handoff | `service.rs:670–681` reserves the submission slot and then Scribe slot before staging inputs. If the latter reservation fails, the former RAII reservation drops unused. `ScribeWrite::ExecutionInputs` retains the submission reservation with the same submission identity (`scribe_outbox.rs:125–135`). | PASS |
| Every pending write remains admitted through waiting, in-flight and retry states | Generic `Outbox::reserve`, `send`, `Admission`, and `OutboxWriter::{dispatch,finish,release}` preserve admitted ownership until successful/terminal consumption or abandonment. Waiting/failed backlogs retain their reserved counts; retry does not readmit or merge later items into a failed slice. The existing writer, rather than a second queue owner, retains all these states. | PASS |
| Scribe retry identity and ACK precede submission | `ScribeSink::{frames,encode,split,identify}` (`scribe_outbox.rs:730–878`) retain deterministic frame order and tenant/destination/attribution/content-derived batch IDs. `write` (`:905–951`) returns retryable failures before any submission handoff; after every frame settles, only non-terminally-lost input items consume their reserved submission slot. Thus partial ACK followed by retry repeats the original Scribe identities before handing off. Terminally rejected/unprojectable inputs do not create runnable executions. | PASS |
| Slow/unavailable PostgreSQL does not discard acknowledged submissions | `verification/submissions.rs:90–119` still accepts a tenant slice in one tenant transaction and returns connection/write/commit errors to the existing generic retry owner. `VerifierExecutionQueue::{accept_submissions,accept,insert_submission}` (`wyrd-sql/src/queries/verifier_executions.rs:1078–1207`) remains the existing durable authority: execution replay short-circuits, keyed submissions serialize under the existing lock, matching requests replay, conflicts settle errored, and the caller owns commit. No SQL or persistence protocol was replaced. | PASS |
| Completion cannot lose a result merely because admission filled during execution | `VerifierRunner::execute` (`runner.rs:361–388`) reserves the result slot before running the engine; refusal defers with no result. `settle` (`:494–567`) keeps the existing lease-fenced completion, stages the owned result only when settlement applies, and commits afterwards. The existing settlement retry retains the selected result identity and consumes its write once; stale leases stage nothing. Deferred work uses the existing release/refund transition. | PASS |
| Siblings preserve their distinct durability policies | All `ScribeWrite` variants still use the same sink. Audit and tenant spans use non-blocking unreserved staging, with immediate refusal logged/counted by `Outbox::stage`; server spans retain their existing permit lifetime. Gateway capture (`components/gateway/invocation.rs:950–986`) remains post-response best-effort capture and uses that same unreserved staging; the bound can now shed it, counted as Scribe outbox loss. Verification inputs/results use reservations instead, so ordinary source verification cannot silently fall into the audit/span/capture shedding policy. Existing Scribe terminal-refusal handling remains unchanged. | PASS |
| Shutdown preserves Scribe → submission ordering and truthful observed loss | `app/server.rs:850–935` drains source/runtime work, shuts down the Scribe outbox before Bifrost, and shuts down the submission outbox only afterwards. `Outbox::shutdown` (`outbox.rs:334–361`) fences new sends, waits to the deadline, abandons remaining writer ownership, releases staged admission, and counts remaining pending writes lost. Carried unspent submission reservations drop with abandoned inputs. A later stage after the fence remains counted loss. No durable receipt or crash-proof in-memory outbox was added. | PASS |

Tenant batching/routing remains keyed by the same typed `DataTenantId`; Scribe identities retain that tenant and `SubmissionSink` still opens `tenant_conn(tenant)`. No changed SQL query, authorization policy, RLS rule or tenant derivation was found in this boundary. This durability scope supplies no additional reason to select a separate tenancy reviewer.

## Verification actually run

```sh
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::bounded_staging_covers_retrying_and_inflight_work) | test(=outbox::tests::a_retry_writes_the_identical_failed_slice_without_later_items) | test(=outbox::tests::shutdown_flushes_items_that_recover_before_the_deadline) | test(=outbox::tests::shutdown_counts_items_unwritten_at_the_deadline_as_lost)'
```

Result: **4 selected, 4 passed, 64 skipped**, nextest run `f0c841d8-d11e-4426-9465-5c7f2ae90770`.

Source-inspected supporting proof includes the new three-language staging-pressure journeys and MCP refusal journey, the runtime admission/retry/drain tests, existing Scribe identical-frame/terminal-refusal tests, and the changed runner-cap integration test. The implementation evidence records additional successful checks; those records were treated as claims, not independent execution here.

## Proof limits and findings

No production durability defect was established. No proposed findings or remediation are requested by this domain review.

The new SDK journeys exercise pressure using held direct judgments, then observe accepted/refused input counts and recovery. They do not independently inject a PostgreSQL outage at the Scribe-ACK/submission seam or crash a process at a settlement commit boundary; preservation of those existing semantics is supported here by source tracing and the focused generic retry/shutdown tests. Live-server SDK/MCP suites and fault-injection suites were not rerun in this review. The grouped J-005 seam remains explicitly deferred until TASK-001 integration, as TASK-003 permits. In-memory data can still be lost on abrupt process death; only losses the process observes can be counted.

Implementation result under the common durability obligations: **PASS**. Full task approval remains subject to resolving the recorded specification-authority discrepancy and the other independent review reports.
