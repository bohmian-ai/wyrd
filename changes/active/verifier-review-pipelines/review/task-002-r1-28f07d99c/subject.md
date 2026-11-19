# TASK-002 R1 immutable remediation review

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow
Original cumulative base: 990a85dc7155ff0d4de1acc17e4aeaa7a42d582f
Prior candidate / remediation base: 75c9bfa9e07163099eac40d5a556c4447c6a519d
New candidate: 28f07d99cee44669cd4a8209d1f096ee19c5dfd4
Branch: task/verifier-review-pipelines/TASK-002
Review diff: `git diff 75c9bfa9e07163099eac40d5a556c4447c6a519d 28f07d99cee44669cd4a8209d1f096ee19c5dfd4`.
Cumulative context: original base...candidate. cc4165193 is included because its parent is the prior candidate; no rebase/patch-correspondence problem.

## Acceptance inputs

Approved baseline revision 18 plus explicitly approved corrections in changes/active/verifier-review-pipelines/spec.md; unrelated revision-21 draft waits remain excluded. Original task tasks/TASK-002-agent-tooling.md, publication/durability supplements, prior review task-002-75c9bfa9e/{verdict,findings-validation}.md and every selected prior report, remediation task task-002-75c9bfa9e/TASK-002-R1-publication-integrity-and-proof.md. Its 30 O1.1–O8.1 obligations and source assertions seed the closure matrix; inspect any required omitted observations, not just labels. Supplied verification execution is recorded in Remediation Evidence, not in a reviewer-facing completion summary. Prior performance addendum is measurement context, not optimization requirements.

The original task received a narrow wyrd-plan documentation correction before this review freezes: accepted authority snapshot, later issuance, principal-revocation/cancellation and actual Proof-5 limits are distinguished. Historical evidence/admissions and prior verdict are preserved. This does not change the approved spec/security behavior or implement live grant refresh. The separate comprehension pass is recorded in planning-correction.md. Hashes identify the resulting reviewed authority; don't mutate any source/authority/report owned by others. Two .agents/skills edits and the untracked changes packet preexist remediation and are excluded from candidate code diff.

## Scope and carried-forward source evidence

First remediation review: close FIND-TASK-002-1…7 and all mapped obligations against the fix diff and affected callers/consumers. Carry forward prior accepted write/path safety, snapshots/JSON/null, host producer construction, selection, ordinary direct/queued composition, thin projections and no duplicated loop/store/replay owner, with references in prior validated matrix/reports. Reopen only a fix-affected invariant or a specific proof gap with a recorded reason. No comprehensive whole-task/performance rediscovery. Historical production-before-RED remains a process deviation; do not fabricate repair.

## Shared navigation map

- RunState receipt: state.rs::receipt now matches producer plus name OR call ID, then exact name/call/digest; accept_publication and publication_receipt callers share it. Owner test a_call_keeps_its_original_publication; SDK agent_publish_retries_use_host_identity. FIND-1 / O1.*.
- RunState::forget now preserves attempts with accepted publications; Checkpoints trait/engine comments updated; inspect begin/change/durable, all forget callers, queue deferral and restored attempt reconstruction. Server retained_publications_keep_their_busy_attempt with positive and no-retry/before-effect controls. FIND-2 / O2.*.
- Publications::path reuses VerificationControl::authorize_evidence before work/replay; JSON remains unaffected. Inspect existing receiving authorization/audit and Caller attribution. Server tools pg test file_publications_decide_their_file_permission asserts exact scope, both allowed decisions, denied no-IO, replay, JSON and delegated/SYSTEM attribution. Existing success fixture authority adjusted in pg_durable_verifier_work, pg_hosted_artifacts and wyrd-testing VerificationFixture::publish. FIND-3 / O3.*.
- Python/TS invalid selection, existing building-block domain refusals; MCP hosted journey adds actual invoke/domain negative records; CLI in SDK hosted Agent journey; cards route and served ToolCallRecord enum checks. FIND-4 / O4.*. MCP has no Card declare tool; selection uses existing shared Rust client, assess actual shipped surfaces.
- Server pg_durable_verifier_work real Agent/recovery, inventory-blind exact-object read, accepted-reply loss/replay, HeldRevisions deterministic dropped direct work; SDK lifecycle rewritten to public outcomes and fixed fixture digests. FIND-5 / O5.*.
- SDK tool_limits_release_capacity_and_preserve_provenance adds cumulative write refusal/replacement/readback/record assertions, HostedLimits projected through existing wyrd-testing server facade. FIND-6 / O6.*.
- Changed helper panic docs/module imports and new test helper docs, no forbidden sleeps/private SDK lifecycle or digest algorithms. FIND-7 / O7.*. Task wording/historical evidence O8.1.

## Specialist applicability and selection

| Contract/invariant | Changed owner/producer/consumer | Current selection and carry-forward |
|---|---|---|
| Receipt identity, durable invocation/attempt retention and recovery | RunState receipt/forget -> shared Publications and runner/engine; small owner corrections plus new focused state/recovery proof | domain-rev-data: targeted sensitive persisted state/identity lifecycle discovery; prior invariant/data/system reports carry other coverage |
| Receiving File authority, tenant confinement and permission audit attribution | Publications::path -> existing authorize_evidence -> Scribe audit; new accepted-authority test fixtures/callers | domain-rev-security: targeted discovery; prior security authority resolution and unchanged tenant/path mechanics carry forward |
| Cross-surface proof placement, stable errors, test docs and 30 assertion obligations | Server, Vala, MCP, Rust/Python/TS tests and test-server projection; no schema/generator input changes | repo-rev: focused changed rules and cross-surface proof assertions, no full original audit |
| Integrated closure of 7 findings and all obligations, reuse and correction burden | Complete remediation diff and affected paths | fresh ponytail-rev after selected reports; independently validates closure and applicability |

No separate behavior/invariant discovery: remediation defaults to ponytail closure, supplemented only by above material state/security boundary changes. No new maintainer abstraction specialist: production extends same owners with no new orchestration/store/cache/trait; prior whole-workflow comparison carries forward and validator checks changed small bodies for drift. No separate system/concurrency specialist: no new locks/tasks/queue/IO ownership, failure mapping or resource mechanism; state retry invariant is specifically covered by data reviewer, existing system/concurrency evidence carries other coverage. If actual changed source invalidates that omission, flag targeted missing coverage. No UI/provider-protocol/domain integration changes. Performance costs remain recorded, unmeasured and unchanged by this task.

## Process and verification

Read root AGENTS, applicable nested Python/TS rules, skill, changes/spec-driven-development.md, maintainer guide, applicable Rust/errors/agent-surfaces/testing and narrow architecture sections. CodeGraph exists: use mise exec -- codegraph explore before code navigation; no new navigation scripts. Source and reviewed authority immutable. Only assigned report writes; do not revert others' edits. No subagents. No Cargo/benchmark/generator commands without root coordination; shared target commands must be sequential. Static source/actual assertion checks can establish closure alongside credible supplied execution, without broad reruns. Root full remediation diff --check is clean. Independent executed results must be distinguished from supplied claims. Candidate/authority change during frozen review blocks it.
