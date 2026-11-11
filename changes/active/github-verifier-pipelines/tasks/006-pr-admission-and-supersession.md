---
id: TASK-006
status: proposed
approved_spec_revision: 5
requirements: [REQ-004, REQ-005, REQ-015, REQ-016]
acceptance: [AC-003, AC-011]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007]
depends_on: [TASK-001, TASK-002, TASK-003]
---

# Admit PR generations through the verification queue

Implement **approved-runtime-and-github.md → Webhook ingress** and
**approved-code-review-support.md → Successive commits and pipeline ownership**.
Use **bursts-and-successive-commits.md** for the fair-queue/burst rationale,
not as authority for a new SLA or cache subsystem.

## Actual sources and complete owner workflow

Input is a signature-verified provider PR/source delivery for task 002's connected,
selected repository. Resolve its stable host/repository identity to task 003's
selected registered Repository and normal PR bindings. Provider delivery ID
identifies replay; PR base/head/source identity and current provider state identify
the source generation. Do not read TOML/policy from the PR head. Current tenant
execution authority is derived and constrained through existing authority owners,
not saved personal OAuth credentials or provider metadata.

Existing `verification/submissions.rs::SubmissionSink` reserves bounded staging,
accepts Scribe-acknowledged inputs, batches per tenant into
`VerifierExecutionQueue::accept_submissions`, and answers AdmissionReply only
after commit. `runner.rs` then claims fairly and loads the same engine, with
Bifrost RunState recovery. Extend this complete pipeline for frozen PR fanout;
do not build a webhook engine, inbox table, second queue or detached finalizer.

Freeze Repository, bound Verifier/Trigger, authority, source repository/PR/base/head
and reviewed source identity before durable handoff. Stage a stable canonical
Scribe batch with stable internal delivery/execution identities for its frozen
binding fanout. Redelivery/recovery reuses the first accepted batch/policy and
settles each binding exactly once, even when only part reached queue commit.
An in-process reservation is not durable acceptance. Return positive webhook
receipt only after required admissions commit, or durable obsolete/refusal
settlement establishes no work owed; before that, failure/no answer must remain
retryable. Do not silently resample a newer policy on retry.

GitHub does not automatically redeliver failed deliveries: extend the same App
maintenance/connection owner for bounded failed-delivery recovery and progress
using existing control state. Recovery must rediscover the original canonical
handoff and queue identities, without a new webhook inbox or generic broker.

## Lifecycle, cancellation and handoff

Open/reopen/ready and actual head/base changes admit non-draft PRs; metadata-only
edits do not. Draft conversion, close, repository removal, suspension/disconnect
cancel affected unfinished queued/running/parked work, preserving history. Policy
selection changes only future admissions, with no sweep of all open PRs.

Confirm current provider PR source outside webhook DAG computation before old
out-of-order source can become current. New head supersedes obsolete unfinished
work: coalesce replaced queued heads and fence running/parked generations before
any new continuation or current delivery. Keep original accepted pins/history;
P completed, A running, B pushed cancels A and leaves P as usable baseline for
B. Task 005 performs that baseline selection, rather than admitting from A's
partial output. Already-sent remote IO may finish; retain pins and reconcile,
never promise cross-provider atomic cancellation.

Output is an ordinary durable ExecutionId per frozen binding with accepted PR
activation/generation, visible execution/binding status and cancellation fences.
Task 005 reads the pins, tasks 007/008 test them before continuation, and task 009
rechecks them before each provider write. Centralize these checks at the existing
queue/authority owners so disconnect and supersession cover later wait/delivery
extensions too. Public inspection/CLI/SDK/MCP projection of PR admission and
supersession belongs here, including guide to event eligibility/diagnosis.

Amend **wyrd-design.md → Trigger / Verifier queued activation** to authorize closed
pull_request (inline and referenced Trigger) and exact source/policy ownership;
remove the contradictory blanket external-push exclusion only for this agreed
activation. Document complete consequential ACK and failed-delivery recovery at
existing execution/operations owners. Keep ordinary scheduled/group submissions.

## Proof

**AC-003:** Through the real connect→declare outputs from 002/003, send 41 eligible
PRs under bounded ingress and observe committed recoverable executions, fair
queue progression and typed status. Duplicate a delivery, interrupt partial
binding fanout at canonical ACK/queue commit and redeliver after restart; assert
same frozen policy/IDs and complete fanout, not duplicate runs. Send delayed old
source events after B, replace queued B by C, supersede running A, and apply
draft/close/access loss; assert stale work cannot become current and accepted
history stays inspectable. Before task 005 lands use an ordinary declared queued
Verifier for admission proof, not a fabricated binding; task 005 supplies the
actual extraction consumer seam. Tasks 007/008/009 extend this same source journey
to parked continuation and remote write races.

**AC-011 (admission):** Public inspection and docs explain eligibility, accepted
pins and cancellation without a hidden orchestrator or UI.

Tier 1: `wyrd-testing` existing server verification runtime owner and all shipped
SDK/MCP journey surfaces (README runners), local GitHub protocol transport and
real server/Postgres. Tier 2: SQL `integration` queue and server route owners for
conditional fanout/generation/crash handoffs via `mise run test:sql` and affected
`mise run test:wyrd`; tier 3: signature/eligibility decision branches. Record
focused commands for new tests after creation. Use existing deterministic crash
and execution controls where applicable; no synthetic host-load benchmark or
arbitrary completion-time target. Run README boundary/contract checks.

## Obligations

| ID | Source | Observation | Planned assertion |
|---|---|---|---|
| A1 | REQ-004; INV-001; ¶Actual sources 1 | A signature-verified `pull_request` delivery for a connected, selected repository resolves to the selected Repository Card and its `pull_request` bindings; unsigned/mis-signed, unconnected and unselected deliveries admit nothing; policy never comes from the PR head | unit signature/eligibility branches; journey admits one execution per binding; unselected repository admits none |
| A2 | REQ-004; INV-005; ¶Actual sources 3 | Repository, Verifier, Trigger, authority and PR base/head/head-repository identity are frozen on each execution before handoff; a later policy selection affects only later admissions and sweeps no open PR | inspection shows pins and frozen Verifier/Trigger; re-declare then redeliver keeps the first policy; new event after re-declare uses the new one |
| A3 | REQ-004; ¶Lifecycle 1 | opened/reopened/ready_for_review/synchronize and base change admit non-draft PRs; draft PRs and metadata-only `edited` do not | eligibility unit branches; journey draft-open admits none, metadata edit admits none |
| A4 | REQ-004; INV-001 | Execution authority is the selecting declaration's tenant principal, constrained by current permission; missing `verifier:run` settles durably as a refusal, never through saved OAuth or provider metadata | SQL/server test: revoked principal → durable refusal, 2xx, no runnable work |
| A5 | INV-003; ¶Actual sources 3 | 2xx follows only committed admissions or durable obsolete/refusal settlement; failure before commit answers non-2xx and stays retryable | crash before queue commit → non-2xx; 2xx body lists committed execution IDs |
| A6 | INV-003; INV-004; AC-003 | Duplicate delivery and redelivery after a restart that interrupted fanout at canonical ACK or queue commit reuse the first batch's frozen policy and execution IDs and settle each binding exactly once | journey with `CapabilityCrash` at both boundaries + restart + redeliver: same IDs, one execution per binding |
| A7 | INV-003; ¶Actual sources 4 | Failed deliveries are recovered through the GitHub App owner: bounded listing of App hook deliveries and redelivery of those without a successful attempt, rediscovering the original handoff identities, with no inbox table or broker | mock records redelivery request; recovered delivery admits same IDs as original |
| A8 | REQ-005; INV-006; AC-003 | A new head/base coalesces queued older generations, fences running and parked older work, keeps their pins and history; P completed, A running, B pushed cancels A and leaves P completed | journey: queued B replaced by C (B cancelled superseded), running A cancelled with fence, P untouched |
| A9 | REQ-005; INV-006; ¶Lifecycle 2 | Delayed old events cannot restore an obsolete generation: current provider PR state is confirmed before a source becomes current | journey: delayed A event after B admits nothing and B stays current |
| A10 | REQ-004; INV-006; ¶Lifecycle 1 | Draft conversion, close, repository removal, suspension and disconnect cancel affected unfinished work with a stable reason while preserving history | journeys for draft, close, repositories_removed, suspend, disconnect |
| A11 | REQ-015; AC-003 | 41 eligible PRs under bounded ingress commit recoverable executions that progress through the existing fair queue with typed status | 41-PR journey: 41 executions committed, all reach terminal completed |
| A12 | ¶Lifecycle 3; INV-006 | Stale-generation checks live at the existing queue owner so later wait/delivery extensions inherit them | source inspection: one queue-owned supersession/fence path |
| A13 | REQ-016; AC-011 | Execution inspection projects PR activation (repository, number, base, head, head repository, delivery) and cancellation reasons on HTTP/MCP, Rust, Python and TypeScript SDKs and CLI | journeys on each surface read pins and cancellation reason |
| A14 | AC-011 | Docs explain eligibility, accepted pins, cancellation and diagnosis without a UI | docs content review; `mise run docs:check` |
| A15 | ¶Lifecycle 4 | `wyrd-design.md → Trigger / Verifier queued activation` authorizes closed `pull_request` activation and removes the external-push exclusion only for it; ACK and failed-delivery recovery documented at execution and operations owners | content review |
| A16 | INV-002; INV-007 | No new tables, queue, engine, inbox or broker; Bifrost owns evidence; no secrets in Cards/results | migration review; `check:deps`, `check:tenant-isolation` |
