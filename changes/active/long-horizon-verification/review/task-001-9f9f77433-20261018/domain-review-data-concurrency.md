# Data, concurrency and durability review

Result: **FAIL** — one source-authority race remains. This is static independent discovery; root owns fresh execution and independent validation.

Subject: cumulative `ef3a57b0776dd7bc451273b9176498fcca700550` → `9f9f7743311aec0f2191f775e7bf054aca00ff91`, assigned worktree. HEAD was checked during review and remained the candidate. Latest remediation diff was navigation only; cumulative data-boundary changes and surviving earlier machinery were inspected. Only this report was written. No tests, Cargo/build/check lanes, source edits or sibling current-round conclusions were used.

## Authority and coverage

Applied: AGENTS.md SQL ownership, server-owned durable behavior, ownership, testing and completion rules; agent-rules; reference router; spec-driven-development and maintainer-style; Wyrd design Verifier/Trigger/Operator/client boundaries and doctrine; Bifrost table identity/managed time, Scribe acknowledgement, query cuts and terminal contracts; evaluation, OLAP-serving and analytical-operations-reliability references. Approved spec revision 15, original TASK-001 and prior R1/R2/R3 verdicts, findings, remediation/evidence govern acceptance. Evidence appendices are claims, not source or fresh execution proof.

The owner's uncapped identities, ceiling on payloads, PostgreSQL-anchored ingest time, single-statement membership/value read and newest-64/no-cursor decisions are accepted. No performance finding challenges them. TASK-002/003 and REQ-015 remain excluded; no retained-table upgrade, receipt, terminal-source barrier or lifecycle table is requested.

CodeGraph was used first for group/queue and producer/registration navigation. Scope expanded through these owners and siblings:

| Boundary | Source coverage and assessment |
|---|---|
| Caller to attributed source | Shared Observe/Target/StagedRun; Python Observe and Node NativeRun changes; typed VerificationRequest/execution/GroupCoverage; principal owner extraction; VerificationControl enqueue/execute/stage_direct; ResultPayloadBuilder inputs/summary and managed schemas. Group metadata remains separate from slots; existing queue_execution distinguishes submitted from direct inputs. |
| Registration to frozen source | EffectiveSpecs/GroupSite/source_binding; BindingProjector group_sources; Trigger/source slots, source-reference validation and shared artifact-manifest validation. Exact UID pins/owning occurrence/participating ambiguity checks feed GroupEndSettings. No alternate durable identity owner is needed. |
| Scribe to queue authority | ScribeSink frames/write acknowledgement and subsequent SubmissionOutbox staging; SubmissionSink transaction; Submission/RequestKey; queue accept/insert/conflict; group_submissions refusal/selected-result classification. This supplies the finding below. |
| Discovery to snapshot | Entire production groups.rs and units: candidates, Sources one-statement window count/CASE payload gate, decode, Ended, ordering, Payloads and null/malformed handling; group_states/covers/enqueue_group. Complete eligible activity now drives first/newest/member coverage, and values and membership come from one query cut. |
| Lifecycle to result/reaction | DUE_GROUP_SQL/claim_due_group/advance_cursor; scheduler poll_groups commit/error branches; INSERT_GROUP_SQL and outstanding partial unique index; grouped runner/ceiling outcome; COMPLETE/PREVIOUS_GROUP/dispatch SQL; selected result pointer and result mapper. Ordinary manual/scheduled/direct writers were traced. TenantConn retains tenant ownership and caller-controlled transaction lifecycle. |
| Managed time/storage | execution_inputs/results definitions/layouts; managed columns; Scribe ingress receipt clock, preprocess/admitted timestamp and fence; PostgreSQL anchor plus monotonic advance. recorded_at/ended_at no longer decide group timing/order. In-place changes honor pre-release reset. |
| Query recovery | ScheduledQueryCaller accepted owner path/run_with and terminal consumption; collection and test fault application; pg_verification_groups tick/no-enqueue/retry, SQL group lifecycle tests. Production SQL errors roll back; read errors permit cursor plus previously complete groups to commit and retry later. |
| Consumers/proof | Rust support/refund/coding/frames/recovery/late-work and HeldReview ordering; Python/TS grouped bodies/forwarding; MCP grouped support/media/query proofs; OpenAPI/source schemas and generated TS projection; changed design/evaluation docs and recorded mise/manifests/lock scope. Assessed for data-boundary consistency, not independent certification of every declaration/runtime test. |

## Material proposed finding

### DATA-R4-01 — INCORRECT — a submitted input can be judged before the queue refuses it

Related invariant: existing **FIND-TASK-001-2** eligible-refusal/source-membership obligation. This is a remaining producer-to-source-authority interleaving, not a new readiness feature.

**Obligation:** REQ-001 preserves idempotency/conflict semantics; REQ-002 returns executions of the resolved source binding; REQ-004 judges eligible source activity; REQ-009 counts eligible distinct executions. J-001/AC-001 exclude conflicting enqueue attempts; AC-006 preserves immutable snapshots and existing processing.

**Changed location:** `crates/wyrd/wyrd-server/src/verification/groups.rs:430–445,458–480`, reached from poll `:141–155`. Sources::surfaces selects inputs without their existing queue_execution discriminator. Sources::read removes only identities already in GroupSubmissions.refused. A submitted identity absent from SQL is admitted just like a direct input.

**Producer and sibling evidence:** `components/verification/service.rs:610–647` stages the input with its Submission. `verification/results.rs:306–345` records `queue_execution = submission.is_some()`, so this distinction already exists. `scribe_outbox.rs:881–933` first successfully submits frames, then stages acknowledged submissions on the separate outbox. `verification/submissions.rs:84–95` later opens a tenant transaction, calls accept_submissions and commits. `wyrd-sql/src/queries/verifier_executions.rs:1280–1332` decides keyed conflicts inside that transaction and records the distinct refused attempt. group_submissions `:1706–1727` reads only committed queue rows; the binding claim locks neither submission acceptance nor its key. Direct inputs intentionally have no queue record; queued results require a prior queue claim. Direct, manual, scheduled and grouped writers were inspected to establish those distinctions.

**Reachable trace:**

1. An earlier request owns key K. A conflicting request under K names grouped source tuple G. Its input is acknowledged/query-visible while acceptance has not committed. This is the existing asynchronous two-outbox pipeline; writer retry or a key-lock wait can exceed idle while an independent replica polls.
2. group_submissions sees the conflicting attempt in neither refused nor selected. The Bifrost statement still admits its input because it ignores queue_execution.
3. A due review pins conflicting content/identity/activity, or marks false overflow at the ceiling. The ordinary runner can settle a judgment/reaction from that invalid snapshot.
4. Acceptance later commits conflict refusal. It cannot repair immutable inputs. Removing the row reduces or leaves newest/count, and GroupState::covers (`:996–998`) can consider the contaminated coverage sufficient, so correction is not ensured.

**Consequence:** content from a submission that never runs and is ultimately refused can influence the group judgment, or N accepted executions plus one unaccepted conflict can produce source_execution_limit_exceeded at ceiling N. Existing settled-conflict proof does not exercise this pre-acceptance state. This is independent of payload/identity limits and clock choices.

**Smallest correction:** extend existing Sources eligibility using the retained direct/submitted discriminator and existing group_submissions accepted identity keys. A submitted input whose acceptance is not established must not contribute activity, capacity or payload. A direct input remains immediately eligible; accepted pending/running work remains eligible without waiting for its result. Reuse the existing queue classification and one-statement owner, preserving selected-result authority, genuine nulls, complete activity and immutable retries. No tracker, timer, receipt, producer IO, staging reorder, extra queue/store or downstream SDK/evaluator/Operator guard is justified.

**Focused proof:** at the existing group/queue/Scribe seam, make a queued input visible while acceptance is held/uncommitted. With N accepted IDs plus that pending conflict at ceiling N, a due poll must pin only accepted IDs with no exceeded marker. Commit the refusal and prove no contaminated review/extra dispatch. Also normally accept an initially absent source (pending/running suffices), then show one later whole snapshot includes it without waiting for completion; preserve direct inputs and aligned null results. Existing tenant/key-lock/staging fixtures suffice; no new public fault API or generic harness is needed.

## Prior closure checked against current source

| ID | Disposition within this domain |
|---|---|
| FIND-TASK-001-1 | Old watermark/hour omission removed. Unselected results are excluded before Ended/member coverage. PG a_visible_result_counts_once_its_settlement_selects_it pins null, selects the same already-visible result without republishing, then pins the real outcome. Accepted newest-64/no cursor stays closed. |
| FIND-TASK-001-2 | Separate count/list/payload cuts, physical-prefix suppression, broad over-ceiling payload collection, capped-newest activity and refused-first finality are removed. One window-count statement supplies all identities/activity/values; CASE gates over-ceiling values, Ended sees uncapped activity. PG settled-refusal ceiling and first-eligible finality cases address committed refusals. DATA-R4-01 remains. Approved revision 15 supersedes the old successful selected-payload-omission guard/proof: one statement cannot independently omit the selected value from its listed row; genuine absent slots/results still project null. |
| FIND-TASK-001-3 | Checked addition and elapsed spans remain; maximum accepted-duration unit covers arithmetic. Group lifecycle uses no producer wall clock. |
| FIND-TASK-001-4 | String SourceText InputSlot alongside typed declarations and group/source projections agree for this data path; comprehensive schema parity belongs to contract review. |
| FIND-TASK-001-5 | Every grouped statement uses accepted/authenticated owner ScheduledQueryCaller with normal grants/admission; no SYSTEM privilege replacement. Drift's distinct exception is unchanged. |
| FIND-TASK-001-6 | Support::tick now documents Errors and commit/partial progress. Comprehensive rustdoc belongs to standards review. |
| FIND-TASK-001-7 | Current design/evaluation text recognizes the bifrost.query-only check exception and live spans outside immutable inputs. |
| FIND-TASK-001-8 | Rust HeldReview observes pinned trace IDs before publishing/flushing later spans and releasing the first provider response; final assertions retain pinned IDs and live tool results. Temporal gap is closed in source. Explicit partial group transport/no-enqueue/next complete retry PG proof remains. No fresh runtime pass is claimed. |

## Assessment and limits

| Obligation | Result |
|---|---|
| Exact tuple/tenant/owner pins and no-IO producer owner recording | PASS by inspected source |
| Serial whole immutable snapshots, coalescing, stale fencing, retry, verdict-change baseline/replay | PASS by queue/index/scheduler/runner and existing reported proof |
| PostgreSQL-anchored ingest time, complete first/newest activity and maximum durations | PASS by source and targeted test bodies |
| One-cut capacity/membership/values, dedup, aligned nulls and no over-ceiling payload | PASS conditional on correct eligibility |
| Refused submissions excluded before activity/capacity/snapshot | FAIL: pre-acceptance interleaving above |
| Failed/truncated reads, cursor recovery and sibling availability | PASS by reused terminal owner and explicit fault/retry body |
| Existing outbox-loss boundaries; no receipt/upgrade/group lifecycle/checkpoint | PASS |

Ponytail: existing Drift window machinery cannot replace aligned cross-execution projection, so GroupPoller is earned. The one-statement simplification removes previous reconciliation machinery. The minimum remaining fix reuses its source eligibility owner and existing queue authority; no new abstraction/dependency is needed.

Skipped execution/build/check lanes as instructed; root owns sequential fresh verification. Static counterexample only, not an executed reproduction. Accepted live-source omission/overlap, outbox loss and newest-page discovery limits remain and are not findings.
