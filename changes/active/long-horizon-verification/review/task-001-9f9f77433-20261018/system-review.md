# TASK-001 independent system resilience review

Overall result: **FAIL** — one proposed source-authority interleaving for independent validation.

## Subject, authority and limits

Repository `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`; immutable base `ef3a57b0776dd7bc451273b9176498fcca700550`; candidate `9f9f7743311aec0f2191f775e7bf054aca00ff91` (HEAD checked). The subject is the complete cumulative range; the latest remediation diff was navigation only. No current-round sibling report or intended verdict was supplied or read.

Read task-review skill, AGENTS/agent rules, reference router, spec-driven development and maintainer style; applicable Wyrd design/doctrine, Bifrost durability/visibility/query/failure contracts, deployment/recovery authority, architecture constraints, evaluation and analytical reliability. Applied approved spec revision 15, original TASK-001, R1/R2/R3 verdicts/validated findings/remediation and appended evidence. CodeGraph preceded source navigation. Current owner decisions supersede old private mechanics: uncapped identities, payload-only ceiling, one statement for membership and payloads, PostgreSQL-anchored wyrd_ingested_at for row time. Newest-64/no-cursor is accepted. TASK-002, TASK-003 and REQ-015 are excluded.

No Cargo/build/check/test lane ran; no source/test was changed. Root owns sequential execution. Packet results are claims inspected against the bodies, not this reviewer's executions. Root separately reported build worker, five units, four PG seams and seven Rust journeys passing while this report was being finalized; Python was running. The only authored artifact is this file.

## Deployed paths and credible failures

One logical wyrd-server can have replicas and role-targeted pods. RuntimeBuilder composes GroupPoller into the existing scheduler where hosted blocks supply the ceiling (`verification/mod.rs:380–390`). Producers use the shared SDK transport; the scheduler/runner use PostgreSQL; existing ScribeOutbox reaches local or peer Scribe and authorized Oracle supplies reads. No new service, queue, receipt or durable group inventory appears.

| Expanded path coverage | Interruption, durable authority and recovery | Assessment |
|---|---|---|
| Rust Observe/Target/StagedRun, Python Observe/stubs, Node/TS → VerificationRequest → Principal owner copy → VerificationControl/ResultPayloadBuilder → managed execution_inputs/results | Producer cancellation/death before Scribe ACK may lose process staging; retained rows survive producer/Run lifetime. Existing submission identities handle replay. | Metadata adds no producer PostgreSQL dependency or binding lookup; accepted REQ-005 crash-loss boundary remains. |
| SourceText/InputSlot/reference, Trigger/Operator and artifact validation → EffectiveSpecs/GroupSite/source_binding → BindingProjector → bindings | Invalid/ambiguous/missing/cyclic source refuses registration. Registry transaction owns rollback. Exact versions/settings persist across replacement processes. | Existing owners are extended; invalid registration fails at request boundary. |
| Scheduler pass/poll_groups → GroupPoller → ScheduledQueryCaller | SQL failure rolls back tick. Oracle denial/outage/timeout/protocol failure logs GroupPollError::Read and commits cadence plus any preceding whole snapshots; next tick retries. SKIP LOCKED coordinates replicas. Cancellation before commit selection rolls back; selected commit is awaited. | Failure stays at binding/read boundary; no recommendation to crash shared server or stop unrelated APIs. |
| candidates → group_submissions → Sources::read/decode → Ended/Payloads → group_states/covers | Accepted discovery page bound; one group statement supplies complete identities, activity, counts and gated payloads. The queue classification is taken before that separate analytical cut. | Old cross-payload-cut invalid state is removed. Earlier queue classification still misidentifies newly queued results: SYS-R4-001. |
| enqueue_group → stored inputs/coverage → claim/leases → runner grouped branch | Before tick commit, crash rolls back; after it, immutable snapshot survives. Outstanding partial unique index serializes pending/running/retrying reviews. Lease reclaim/stale-settlement fences remain existing owners. | Retries judge pinned inputs; source overflow completes explicit inconclusive before checks. |
| runner result staging → complete → previous verdict → ordinary dispatch/outbox | Staging precedes completion commit. Failed/delayed commit can leave visible unselected attempt result; later lease owner may supersede it. Completion/dispatch insert is atomic; delivery has existing retries. | Preserve this accepted boundary; it exposes the authority interleaving, not a need for receipts or moving staging. |
| Vala check-Agent → HostedTools → BoundedQuery → Oracle | Only declared bifrost.query, with ordinary tool/query/payload permission, deadline and admission. Model/query/parse failures stay in check/execution path; spans remain live. | No new mutating/workspace capability or span durability promise; empty each is inconclusive. |
| ScheduledQueryCaller/BoundedQuery and test-only fault apply | Shared consumer requires valid terminal and clean EOF; failures cancel/settle query resources. Test-only truncation now reaches internal scheduled caller. | Prefix delivery does not become a successful snapshot. Existing query lifecycle owns cancellation. |
| SQL group rows/index/settlement; binding/status/result projection; SDK/HTTP/MCP; source schema/served OpenAPI/types; design/evaluation/docs/fixtures | Existing execution records own snapshots and history. Approved pre-release schema replacement means no retained-upgrade machinery is required. | No deployment or production dependency expansion; manifest delta is installed workspace sha2 for MCP tests, no mise topology change. |

Inspected supporting PG group/execution/binding tests, all three SDK grouped story/support owners, Rust HeldReview, MCP support/media/query cases and served/schema/type assertions. They cover replay/conflict, source isolation/order/nulls, grants/denials, media overflow/empty, serial reactions, restart and late results. Those bodies and recorded passing runs do not exercise every interleaving below.

## Proposed material finding

### SYS-R4-001 — INCORRECT: an earlier queue read's absence is treated as direct-result authority

**Obligation:** REQ-002/003/005 selected authoritative queued result, correct immutable group snapshots and serial reactions; R3 FIND-TASK-001-1 correction. Best-effort visibility permits missing evidence, not judging an abandoned queued attempt as authoritative.

**Locations:** `crates/wyrd/wyrd-server/src/verification/groups.rs:142–143` reads group_submissions before starting the independent group query. Sources::read at `:465–478` emits `(NOT execution_id IN (<earlier queued IDs>) OR result_id IN (<earlier selected IDs>))`. Every execution absent from that earlier PostgreSQL result is therefore treated as direct. Queue `GROUP_SUBMISSIONS_SQL` and `group_submissions` (`wyrd-sql/src/queries/verifier_executions.rs:252–257,1706–1727`) read rows committed by their statement; they do not fence later inserts or positively establish direct origin. `groups.rs:519–545` decodes a permitted result into snapshot payloads.

**Producer/siblings:** VerificationControl::enqueue stages input before independent SubmissionOutbox acceptance (`components/verification/service.rs:554–647`, `verification/submissions.rs`). SubmissionSink and queue accept_submissions/accept commit independently; their key advisory lock does not lock the group binding. Runner settle (`verification/runner.rs:474–565`) stages before completion commit, explicitly allowing retained unselected results after commit failure/lease loss. All manual queued SDK/HTTP/MCP callers share those owners. Direct verify legitimately publishes one result with no queue row through stage_direct. Group/scheduled runner results use the same queue selection authority. The binding lock does not serialize source acceptance/settlement.

**Reachable path:** an existing group is max_age-due within finality. Poll P reads group_submissions. New queued source E is accepted and claimed while P's analytical query opens. Its result R reaches Scribe, but SQL completion is delayed or fails, so E has no selected result. Oracle's later cut includes R. Because E was absent from P's map, NOT queued is true: P includes its result activity/count and outcome as direct. Max_age permits review even though the new row is too recent for idle. E can lose its lease and a retry select a different result. Separate background writers, query startup and uncertain completion are ordinary deployed paths; no corruption or synthetic load is needed.

**Consequence:** an immutable group judgment can consume an unselected/abandoned attempt and, after a prior baseline, dispatch a wrong verdict-change Operator. A later correcting review cannot undo the old result or external action. This is the same selected-result invariant as FIND-1, not a request for complete membership, terminal-source readiness, a cursor or new crash guarantees.

**Minimal correction boundary:** keep the correction at GroupPoller/Sources plus existing queue classification. Do not infer direct origin from absence in a PostgreSQL read preceding a later Bifrost cut. Before enqueue, reconcile captured analytical execution identities with queue classification at a point that cannot miss a queued execution which already produced a captured result. If the classification used by the one statement was stale, defer that affected snapshot and retry using refreshed existing authority. Preserve the single membership/payload statement, payload-only ceiling, exact filters, genuine queued-null snapshots, immutable history, direct authority and existing staging/settlement order. Existing input `queue_execution` is available if pending handoff must be distinguished. No new durable column/inventory, producer IO, service-wide fail-closed action or extra lifecycle owner is justified. A global isolation setting alone cannot synchronize the two stores.

**Focused proof:** using existing group/query/queue seam, capture classification, then accept a queued source and expose its unselected result before the analytical read. With an old max_age-due group, assert it is not pinned as direct and causes no reaction; later proper selection/retry yields the correct next immutable snapshot once. Current `a_visible_result_counts_once_its_settlement_selects_it` creates the source queue row before polling, so it proves only the known-row case. No real network crash, new public fault API, load generator or all-language harness is needed. Counterexample is source-established, not executed here.

## Prior closure and proof assessment

| Prior item | Current conclusion |
|---|---|
| FIND-1 discovery amendment | False watermark/hour cutoff remains removed. Newest-64/no-cursor stays accepted. |
| FIND-1 selected-result transition | New PG test excludes an already-known queued row's unselected result, selects the identical visible result without republication and gets a new snapshot. Predicate filters known rows before counting. Partial closure: SYS-R4-001 concerns newly queued identities across the two reads. |
| FIND-2 capped activity/refused-first finality | Uncapped rows supply full first/newest/counts; discovery MAX plus eligible Ended avoids old refused-first suppression. New finality PG test and window/decode units prove stable cases. No request to restore identity caps. |
| FIND-2 incomplete-payload proof | Superseded by explicit one-statement decision. Identity and payload now come from the same row; separate payload statements/completeness guard and impossible-state proof are deleted. No replacement defensive framework requested. |
| FIND-3 duration safety | Checked timestamp addition and elapsed comparisons plus extreme-duration unit remain; no scheduler panic regression found. |
| FIND-4 schemas | String and typed slot alternatives remain source-owned with served/schema/type coverage. No new recovery consequence. |
| FIND-5 authority | Accepted owner grants, ordinary query admission/denial audit and regrant story remain. Query failure defers rather than widening grants. |
| FIND-6 docs | Original corrections and Support::tick Errors/partial-progress contract are present. No system finding from style. |
| FIND-7 live query | Design/evaluation/validator/runtime agree on declared bifrost.query only for checks. |
| FIND-8 internal truncation/live ordering | Partial internal stream test proves no enqueue then complete retry. Rust coding now holds first check request after pinned inputs, publishes/flushes later span, then releases tool call and asserts live span plus unchanged trace IDs. Earlier ordering gap is closed in source. |
| Rev15 timing | Group rows use wyrd_ingested_at and Ended uses PostgreSQL due.clock. PG tick no longer shifts clock. Scribe admission clock is PostgreSQL-anchored and monotonically advanced; producer recorded_at/ended_at remain facts. |

Existing queue, scheduler, registration, query, Vala engine and Operator owners remain the proper workflows. No proposal demands capacity qualification, process pressure work, retained upgrade or speculative hardening. Healthy paths and restart checks do not disprove the unfenced queue/result ordering above.

Overall **FAIL**, proposed **SYS-R4-001** for independent validation and consolidation under existing selected-result finding if confirmed.

Skipped execution/deployment experiments as instructed; residual risk is accepting an unselected queued result as direct across the two store reads.
