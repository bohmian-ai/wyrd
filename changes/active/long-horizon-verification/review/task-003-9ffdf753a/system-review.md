# TASK-003 system-resilience review

Overall: **BLOCKED** on the subject's unresolved approved-revision identity. Runtime assessment under the unchanged TASK-003 obligations: **PASS**, with no material source-local findings.

## Immutable subject and authority

Reviewed complete cumulative range `ef3a57b0776dd7bc451273b9176498fcca700550...9ffdf753ab11ce9b638c906388d3690a6aede249`, original `tasks/TASK-003-bounded-outbox-admission.md`, and the supplied subject. HEAD remained `9ffdf753ab11ce9b638c906388d3690a6aede249`.

The task names approved revision 13. The available spec is revision 14/draft and records revision 13 approval in prose. I used its unchanged REQ-006, REQ-005 loss boundary and TASK-003 scenarios for useful source review; this report does not approve draft revision 14. The orchestrator must resolve that authority identity before a final acceptance PASS.

Applied AGENTS.md, agent rules, spec-driven-development and maintainer-style references, architecture constraints and analytical operations/reliability guidance, and the Wyrd design/doctrine and Bifrost durability/visibility, ingest, audit and internal capture boundaries relevant here. Used CodeGraph before code navigation; examined the cumulative diff itself and expanded into actual producers, writer, claim, runtime-supervisor and server-shutdown callers. No other discovery reports were consulted.

## Deployed paths and availability boundaries

| Runtime path | Source evidence | Capability and ownership |
|---|---|---|
| HTTP/MCP direct verify → shared control → inline engine → Scribe outbox | `components/verification/service.rs:471–584`, `stage_direct:695–744`; routes and MCP share `VerificationControl` | Authorization finishes first. Two reserved Scribe items are acquired before engine work. Failure or cancellation drops unused reservation; successful encoding stages both source and result without an await between them. Pressure refuses this request; it does not terminate an HTTP/MCP server or worker. |
| HTTP/MCP submit → Scribe input ACK → submission outbox → tenant Postgres queue | `service.rs:611–681`; `scribe_outbox.rs:878–954`; `verification/submissions.rs:23–119` | Both outboxes are reserved before source staging. The downstream reservation travels inside the inputs write and survives Scribe retry. Only a completed Scribe slice whose input was not terminally lost stages the submission. Postgres writer uses one tenant transaction, idempotent submission acceptance, and four concurrent tenant writes. |
| Durable queued work → claims → engine → fenced completion/result | `verification/claims.rs:146–321`; `runner.rs:231`, `349–390`, `496–526`, `attempt:725 onward` | Default process cap is 64 claimed execution tasks. A result slot is reserved before engine execution. Full result staging maps to deferred work with refunded attempt, rather than executing and dropping a result to make space. Lease loss/drain cancellation drops unused reservation. |
| Shared audit, gateway capture, server spans → same Scribe outbox | `wyrd-runtime/src/audit.rs:23–32`; `outbox.rs:227–239`; `scribe_outbox.rs:105–149`, `587–637` | These use unreserved staging. Its capacity refusal logs/counts loss and returns synchronously; authorization and unrelated server capabilities do not await Scribe persistence. Server-span permits remain held through writing/retry and are dropped with the write. |
| Boot and replicas | `boot/mod.rs:877–889`; `config.rs:329–341`, `1563–1569`; `verification/mod.rs:96–123` | Each process composes its own bounded outbox and its own runner limit. Local or peer Scribe remains the durable ingest owner; no cluster quota or new durable staging ledger is introduced. Client/harness changes project the same server behavior and do not introduce a new runtime service. |

## Credible failure and recovery paths

| Failure/interruption | What stops; what remains available | Surviving state and recovery evidence |
|---|---|---|
| Scribe unavailable or slow | Up to four tenant writes can be active; waiting, failed and active items continue holding admission. At reserved-share exhaustion new source calls get typed retryable pressure; audit/capture/span overflow is consumed and counted. HTTP/MCP, reads and other server capabilities remain running. | `OutboxWriter` preserves exact failed slices and separates subsequent arrivals (`outbox.rs:490–698`). Exponential 50ms–5s tenant retry is unchanged; Scribe peer calls have a 30s timeout and attempt routing (`scribe_outbox.rs:86–90`, route submit). Recovery drains retained work and releases its capacity only at consumption, not dequeue. |
| One tenant's sink fails | Backoff applies to that tenant, not the writer process. Other admitted tenant slices may use freed writer slots; a permanently hanging sink can occupy one slot until shutdown, consistent with existing behavior. | `outbox.rs:dispatch`, `is_ready`, `next_retry`, `finish`; independently executed tenant-progress and identical-retry tests pass. No strict fairness or latency guarantee is inferred. |
| Postgres unavailable after Scribe ACK | Submission writer retries while holding already-reserved work; subsequent submissions refuse once downstream admission fills. The ACK-to-submission path cannot evade the bound. | `submissions.rs:90–119` repeats `accept_submissions` and commit with the same submission identities. The Scribe input can survive while an unwritten submission is lost on process death; existing staging-only 202 semantics are preserved, not upgraded into a durable queue receipt. |
| Reserved Scribe share fills while durable executions await work | Runner defers before executing, refunds an attempt through its existing defer transition, then claims again through existing queue timing. It does not crash the shared process or discard successful work to regain capacity. | `runner.rs:361–371`, `attempt`, `settle`; slots are released by reservation Drop or writer consumption. ClaimLoop counts its JoinSet, including tasks awaiting settlement, so a database outage cannot create unlimited execution tasks. |
| Request cancellation or worker lease loss | An unspent reservation releases through RAII. Before direct staging there are no source writes; once staging starts the two synchronous stage operations have no async cancellation point between them. Worker abandon/lease-loss selection cancels execution and releases unused result capacity. | `Reservation::Drop` (`outbox.rs:402–410`); `runner.rs:740–754`. Already-staged result delivery retains the previous uncertain-settlement identity semantics rather than requiring Postgres/Scribe atomicity. |
| Graceful shutdown/deadline | Server drains supervised requests and verification runtime, then fences/drains Scribe outbox while Scribe still accepts. After that handoff source stops, it shuts down the submission outbox. Deadline abandonment cancels in-flight sink writes and logs/counts remaining items lost. | `app/server.rs:850–935`; `claims.rs:179–200`; `outbox.rs:328–360`. Admission is released for the counted lost staged items; unused carried reservations drop with their owning values. A write whose commit outcome is unknown may already be durable, explicitly documented in SubmissionSink. |
| Capability crash/process restart/rolling replacement | Existing verification supervisor restarts the individual capability after backoff and degrades its health slot; sibling capabilities stay alive. Abrupt process loss discards process-only outbox work. | `verification/mod.rs:243–293`; durable claims keep leases and become reclaimable after expiry. Surviving Scribe WAL/batch fences and Postgres queue rows remain the durable authorities. No new deployment-wide availability or loss-free crash claim is made by this change. |

## Accounting and preserved adjacent behavior

`Outbox::reserve`, `stage`, `send`, `Reservation::stage`/Drop, writer finish/release, and shutdown all use the same Admission owner (`outbox.rs:227–454`, `639–698`). Reserved and unreserved counts partition the configured total. Reserved slots not yet staged count against admission even though they do not count in the pending-items gauge. Retrying does not acquire a new slot. The channel, waiting/failed maps and inflight tasks therefore cannot collectively admit beyond capacity. A pending Scribe write also carries its downstream submission reservation, so that handoff is included before ACK.

The existing writer remains the batching/retry/shutdown owner; the submission outbox remains the existing durable-queue writer. ClaimLoop is reused by Operator delivery, with its existing global-permit limit passed as the active cap (`operators.rs:185–190`), preserving that sibling capability's bound. No new failure escalation from staging pressure to process shutdown is present.

## Proof assessment

Independently ran this exact focused command, selecting six tests (nonzero selection):

```sh
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::bounded_staging_covers_retrying_and_inflight_work) | test(=outbox::tests::bounded_audit_staging_remains_nonblocking) | test(=outbox::tests::a_retry_writes_the_identical_failed_slice_without_later_items) | test(=outbox::tests::a_failing_tenant_does_not_delay_another_tenant) | test(=outbox::tests::shutdown_flushes_items_that_recover_before_the_deadline) | test(=outbox::tests::shutdown_counts_items_unwritten_at_the_deadline_as_lost)'
```

Result: **6 passed**, 62 skipped by selector; nextest run `f13e232c-0849-4a2c-9828-aeea4c703763`. These directly exercise hanging/inflight/retrying admission, release/recovery, audit overflow, stable retry slices, tenant progress and shutdown success/loss.

Inspected the Rust pressure journey and the new runner-cap integration test. The journey fills reserved capacity with slow direct judgments, completes a second tenant's admitted queued request during that work, checks verify/submit refusal, retries after drain and checks source-row counts. The runner test holds execution and proves claim count equals slots. The recorded Python/TS/MCP/server test passes are implementation claims, not independently rerun evidence. The pressure journey does not itself exercise a process kill or Scribe outage; those limits are addressed here through lifecycle tracing plus the executed owner-level failure tests, without claiming a new crash qualification result. Combined group_id J-005 remains explicitly open until TASK-001 integration, as allowed by this task's dependency boundary.

## Material source-local findings

None. No bounded implementation correction is proposed.

Skipped full SDK/Postgres suites and process-kill experiments; they were not needed to establish a new source-local defect. Acceptance remains blocked solely on the supplied revision-13/revision-14 authority discrepancy; outbox-only crash loss and finite-capacity audit loss remain intentional risks.
