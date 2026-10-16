# TASK-005 R3 concurrency and resource-lifecycle review

Result: **PASS**. No material proposed findings in this domain.

## Subject and authority

Reviewed source only in detached `/tmp/wyrd-task005-review-02848d737`, candidate `02848d737a621938ec0b5f817e1aa84d5f3df1fd`, cumulative range `f2a726c90edbf8956c7b91c843379175ee20eac0...02848d737a621938ec0b5f817e1aa84d5f3df1fd`. Recent R2 changes were a locator, not the acceptance boundary. Upstream integration is baseline context, not unrequested TASK-005 machinery. HEAD stayed pinned and source worktree clean at final inspection.

Inputs: frozen task-review skill and subject; approved r9 spec; original TASK-005; R1 concurrency report/remediation; R2 remediation and committed evidence; frozen prior R2 verdict/validation hypotheses. No current-round peer report was read. This report covers the assigned concurrency domain, not the complete task verdict.

Authorities applied: AGENTS.md ownership/async/runtime/testing rules; agent-rules.md transaction/audit/runtime rules; wyrd-design.md Verifier execution, admission and coordination clocks (particularly current lines 1337–1354); bifrost-design.md stream/drop and descendant ownership; wyrd-doctrine.mdx server-owned verification; approved spec REQ-002/003/007/008/012/013, INV-004/007, deployment limits, AC-002/003/004/006. Reference routing selected spec-driven-development, maintainer-style, rust-core, evaluation and analytical-operations-reliability. No external protocol claim is needed for this domain.

## Boundary and source coverage

| Boundary | Current source inspected | Result |
| --- | --- | --- |
| Capacity producers | hosted `sandbox.rs::exec,failed`; `workspace.rs::Workspaces::open`; `tools.rs::Invocation::run` | PASS: actual shared resource admission is bounded and Busy retains its typed cause. |
| Refusal latching and wakeup | `vala-eval/blocks/tools.rs::ToolSession,Refusal,RecordedTool`; `blocks/agent.rs::invoke` | PASS: persisted watch state, first-wins reason, race against owned Agent loop. |
| Parallel descendants | Skald `loop_runtime.rs` buffered tool futures; hosted invocation owners; `query/collect.rs::BoundedQuery::run`, query service and Oracle query-stream owners | PASS: inline tool futures drop with the loop; existing query/admission guards own downstream teardown. |
| Graph execution | `blocks/engine.rs::run,drive,attempt,judge`; stage walker and related tests | PASS: async host admission and graph work share the execution deadline; Busy escapes before retries or judgment. |
| Whole execution File budget | hosted `blocks.rs::engine`; `evidence.rs::admit,artifact_file,artifact,read,stored` | PASS: checked admission total precedes graph consumers, independent of concurrent/repeated reads. |
| Workspace and authored Python budget | hosted `workspace.rs::open,Workspace,writer,mounts`; `tools.rs::HostedTools,Invocation,Reservation` | PASS: cohesive owners retain permits/scratch and action budget across authored retries. |
| Direct/queued projection | `components/verification/service.rs::execute`; runner `dispatch,engine_failure,attempt,settle` | PASS: common engine; direct Busy is structured 503 and queued Busy defers without a verdict. |
| Claim/lease/refund/shutdown | `claims.rs::ClaimLoop`; `leases.rs::LeaseRenewal,HeldLease`; queue claim/release/renew/settlement SQL | PASS: existing SQL clocks/fences, refund and drain cancellation remain. |
| ACK and result seam | submission sink, Scribe outbox integration, runner result/settle, queue completion/release | PASS in this domain: background acceptance/replay remains; Busy stages no result or failed-verdict dispatch. |

## FIND-TASK-005-2 closure

The prior residual was Busy waiting behind an unfinished parallel tool until an outer timeout hid the capacity refusal. Current producer-to-consumer trace closes that path:

1. Sandbox admission and exhausted/unserved checkout produce Busy. Workspace admission refuses immediately with Busy. Actual compile/runtime/crash errors keep their distinct categories. Checkout still uses the earlier operation/caller deadline; expiration of the enclosing execution remains an execution timeout.
2. Hosted `Invocation::run` preserves Busy as a typed tool error cause. `RecordedTool::invoke` observes that cause, not strings. Ordinary tool errors and spent authored Python budgets do not raise the refusal.
3. `Refusal::observe` calls `watch::Sender::send_if_modified`, changing only None to Some. Concurrent writers cannot replace the first reason. The value persists without a receiver. `refusal_raised` subscribes then uses `wait_for(Option::is_some)`, covering refusal before subscription, during registration and after awaiting. The borrowed session retains the sender, so closure cannot spuriously resolve the branch.
4. AgentExecutor races ordinary `run_prompt` against that watch. Even if both branches are ready, its post-selection `refused` check makes the latched Busy authoritative. The before-model hook prevents a continuation after refusal. There is no clearing writer, so the early-stop `expect` invariant holds.
5. The Skald loop owns inline async tool futures through `buffer_unordered(concurrency_cap).collect().await`; it introduces no detached tool tasks. Dropping run_prompt drops pending siblings. Session, Agent and tool references then drop on return. Caller cancellation or step/execution timeout follows the same ownership tree without inventing Busy.
6. Both action and check engine paths propagate Busy before authored retry or judgment. Runner maps it to Deferred and existing fenced release. `RELEASE_SQL` requires the current running lease token, decrements attempts, clears expiry and sets the database-clock due time. Busy selects no result branch, so it cannot publish a subject verdict or failure Operator.

The pending-query concern was traced beyond the test double. Hosted Bifrost queries call existing BoundedQuery/Oracle stream owners. Stream/admission/Analytical graph guards retain teardown responsibility; `LeaderStreamOwners::drop` drops admission before profiling, revoking followers and local drivers. No new cancellation service is needed.

**FIND-TASK-005-2: CLOSED in this domain.** Existing runtime, loop and queue owners suffice; no second queue/scheduler/transport is introduced.

## FIND-TASK-005-10/11 resource closure

Engine `run` checks initial expiry, synchronously validates declaration conformity, plans, then wraps host admission plus graph work in the execution deadline. Final expiry prevents ready work publishing a late verdict. Artifact registry/authorization IO occurs inside that timeout. No action, workspace or model call starts before admission succeeds. Direct and submitted dispatch construct the same hosted engine/evidence reader.

`TenantEvidence::admit` counts each original slot declaration once. Files contribute declared size without body reads; Artifacts use existing authorized inventory metadata after slot format/size checks. `checked_add` plus the combined-limit filter refuses overflow/excess. The total is local to admission: parallel extraction, media, workspace and citation consumers cannot race, double-charge, refund or reset it. Repeated reads are not extra original inputs; two original slots binding one Artifact still count twice, as the regression asserts. Per-file length/digest/sniffing and per-workspace/scratch restrictions remain in their existing owners.

This enforces the approved original-file bound without asserting aggregate resident-memory control over derived buffers or introducing persistent reservations. Missing required evidence remains missing/inconclusive; optional omission remains null.

**FIND-TASK-005-10/11: CLOSED for this domain.** Detailed evidence and security acceptance remain the corresponding domain reviewers' responsibility.

## Preserved lifecycle and previous closure

Ordinary tool errors remain model-visible through the existing loop; provider retry classification is unchanged. True step timeout retains authored retry/failure semantics and an execution timeout stays an execution error. Non-Python tools, model waits and tool-less checks acquire no Python permit.

Workspaces retain their permit through asynchronous reads, blocking materialization and the owning TempDir. Cancelling reads releases it. An already-running blocking materialization retains ownership until its bounded operation finishes and discarded output drops, rather than prematurely releasing capacity. Code tools serialize `/work` writers while calculating mounts. Mutex-protected reservations prevent concurrent budget overbooking, and Drop returns only unused time. Action budgets remain in per-execution HostedTools across attempts; fresh hosted engines prevent cross-execution authority/scratch reuse. Existing one-checkout worker lifecycle remains unchanged.

ClaimLoop retains refusal/defer handling rather than a new Python wait queue. Held leases cover execution and settlement; renewal loss cancels execution; shutdown abandonment releases/refunds; stale tokens cannot settle a selected result. FIND-5 remains closed at the concurrency seam: payload identity is distinct from execution identity, completion selects it under the lease fence, and pre-commit outbox staging can leave an abandoned analytical row without making it selected. FIND-9 remains closed: obsolete trace-wait machinery is absent while ordinary release/retry/lease handling remains. Other prior IDs are not reclassified by this specialized review.

## Proof assessment and limits

Recorded R2 exact selection `mise exec -- cargo nextest run --locked -p vala-eval --test integration -E 'test(=blocks_agent::busy_tool_ends_the_agent_step_before_a_pending_sibling)'` traverses the timed engine and actual Agent loop with Busy code and a pending parallel query double. Source assertions require Busy before the 30-second step deadline, no retry/second model call, and dropped pending sibling. Recorded blocks unit (23), Agent integration (10) and route/hosted (18) slices supply supporting seams.

Existing real-hosted `capacity_refusal_is_busy_at_every_hosted_consumer` exercises code action/check, Agent code.run and workspace admission. `busy_queued_execution_is_deferred_until_capacity_frees` composes hosted code with queue defer logging, same-ID eventual completion and attempts == 1. These complement the pending-sibling regression. Engine tests preserve independent overlap, timeout/cancellation and Busy-versus-retry behavior. Workspace and runtime tests cover permit cancellation, drain, lease loss, post-stop committed claims and replica fencing.

The hosted Artifact admission regression rejects format/slot-size/combined-budget cases, including an unread File declaration; its control reads one Artifact twice. Raw direct/queued nonconformance proof uses the same engine. No separate MCP input regression or single combined parallel-Agent-to-queue-refund test was executed; source tracing establishes common owners without requiring a redundant suite. These are proof limits, not discovered defects.

No fresh tests/builds, broad gates, live providers/cloud, credentials or external example rebuilds were run. Supplied results are recorded evidence, not fresh execution on this checkout. The old full gate predates remediation and is not a candidate-wide R3 gate. Final change review owns integrated broad verification. No source, task or authority edits, archives, migrations, or draft runtime features were produced.

## Material finding ledger

Empty. No new stable FIND ID or residual of FIND-2/10/11 is proposed in this domain.

Skipped fresh execution and broad/live checks. Remaining limits are the stated proof composition limits and approved asynchronous outbox/result visibility and loss semantics.
