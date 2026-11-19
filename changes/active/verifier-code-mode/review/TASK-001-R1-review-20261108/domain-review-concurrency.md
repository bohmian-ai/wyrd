# R1 concurrency and cleanup boundary review

**Overall: FAIL.** FIND-TASK-001-2 remains partially open. The other reviewed
concurrency closures pass static review. No new finding ID is proposed.

## Subject and authority

Reviewed prior candidate `0e51af2a2332f362ab44b7c5b88b51c3cb4f8011` to
candidate `f8b45a7208dbc3e8ec6c3cdac3605f6f84b6cc18` (runtime source
`0393740405bb47ab95ff1a404a20be44597fbdeb`). Read the approved revision-2
specification through `git show f8b45a720:changes/active/verifier-code-mode/spec.md`,
original TASK-001, first-review concurrency report, FIND diagnoses and selected
corrections in TASK-001-R1, and its appended proof evidence. Local revision 3
and TASK-003 are excluded.

Applied root AGENTS.md, task-review skill, spec-driven-development authority,
agent_docs/index.md, rust.md and maintainer-style.md, and the Agents/tools/
workspaces section of architecture/wyrd-design.md. The pertinent approved
obligations are REQ-009–013/015 and AC-006–010/018. No `.codegraph/` directory
or additional AGENTS.md exists in the reviewed Vala/server owner paths.

## Source and caller coverage

- Read the remediation changes to Vala ToolSession, Dispatch, Halt, Delivery,
  task-local ACTIVE/hold_permit, and AgentExecutor invocation cleanup.
- Traced direct RecordedTool and nested ToolBridge into their shared admission,
  semaphore, invocation and recording owner. Inspected the deterministic
  post-stop/waiter proof and permit-retention proof in blocks_agent.rs.
- Traced HostedTools::open, Invocation::call/code/blocking and TaskTracker,
  Workspace::writer/extract, Sandbox::exec/drive/request/answer, and real
  hosted Blocks forwarding into AgentExecutor.
- Traced the caller that owns attempt timeouts and retries in
  VerificationEngine::attempt, whole-execution deadline in engine::run,
  Agent check forwarding, and queued runner::attempt's abandon/lease-loss
  select. These callers can drop AgentExecutor, so its lexical close call
  alone is not sufficient lifecycle evidence.
- Inspected hosted owner P2/P3 tests and the appended selector corrections.
  Carried unchanged budget arithmetic, normal workspace/cap-1 behavior and
  ordinary query cancellation evidence from the first concurrency review;
  no remediation dependency invalidates that arithmetic or normal-path proof.

The existing owners are reused: ToolCalls owns request accounting and bounded
records, Dispatch owns non-code admission, Invocation tracks blocking work,
Workspace owns scratch/write serialization, and Sandbox owns worker turns.
The new owned guard and installed TaskTracker are appropriate extensions.
The defect is the missing connection to caller-owned termination, rather
than a need for a replacement extraction or scheduling service.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence inspected | Result |
| --- | --- | --- | --- |
| Atomic ceiling, retries do not refund requests; final allowance alone succeeds | Unchanged ToolCalls::spend and Dispatch::spend; engine keeps ToolCalls outside retry loop | First-review accepted budget tests; R1 budget regression evidence | PASS, carried forward |
| FIND-4: admitted permit waiters retain flat cancellation records | Dispatch::call starts slot after bounded argument validation and before permit wait, tools.rs:536–573 | admitted_waiters_survive_halt_without_dispatch checks outer/held/waiting slots, cancelled waiter, zero bytes/digest | PASS |
| FIND-6: waiting work never starts after terminal refusal | Dispatch::permit selects Halt and checks it again after acquisition, tools.rs:639–650 | HaltScript spends outer/A/B, refuses fourth, releases A within same poll; test asserts only A invoked and no later model request | PASS |
| Shared cap retained by already-started blocking operation | ACTIVE.scope owns permit until hosted blocking takes it; blocking closure owns permit until completion | interrupted_blocking_work_keeps_its_permit proves dropped extraction still blocks following read at cap 1 | PASS within one session; retry escape is FIND-2 |
| Extraction keeps writer/workspace/admission through real work | Invocation::blocking captures writer/permit in tracked blocking closure; extraction closure captures workspace; Workspace::writer returns owned guard | extraction_settles_before_workspace_release checks locked writer, unsettled tracker, directory/admission retention and fresh extraction | PASS for guard retention |
| FIND-2: cleanup waits for started extraction before ending/retrying | AgentExecutor::invoke only awaits ToolSession::close on ordinary inner completion; engine timeout/drop bypasses it | P2 manually closes/waits tracker; neither P2 half drives engine timeout or cancellation across close | FAIL, FIND-TASK-001-2 |
| FIND-3: unstarted workspace wait obeys operation deadline and charges wall time | Invocation::code applies timeout_at around writer/mount setup; started mounts use tracked blocking owner | code_workspace_wait_obeys_operation_deadline holds actual writer, observes typed timeout before worker, diminished second allowance and spent third call | PASS for changed wait; started-work termination shares FIND-2 |
| Failure/capacity/budget stops prevent later model work | Halt::observe retains typed host error and exact QueryQueueFull; AgentExecutor races run and Halt; before_model guard preserved | R1 hosted time-stop and Vala oracle_queue_refusal_is_busy_at_every_origin evidence, plus deterministic HaltScript | PASS for stop propagation; cleanup remains FIND-2 |
| No late returned tool value after cancellation; successful delivery records remain honest | Dropped blocking waiter cannot return Delivery; unfinished record becomes cancellation; Sandbox owns and drops pending delivery/futures | P2 cancellation record in permit proof; P4 waiter proof; source inspected at Sandbox answer/resume boundary | PASS for tool-value publication; detached scratch mutation remains FIND-2 |
| Normal shared scratch/no-overwrite and cap-1 dependent calls remain | Same Workspace::extract/work_room/persist_noclobber owner, stronger owned writer; code.run takes no child permit | First-review workspace/gather proof and R1 code_calls_share_workspace_mutations regression | PASS, carried forward plus affected owner inspection |

## Retained finding

### FIND-TASK-001-2 — INCORRECT: engine termination bypasses awaited blocking settlement

**Source-local ID:** CON-R1-001 (continuation of CON-001 / FIND-TASK-001-2).

**Obligation.** Approved REQ-012 requires cancellation/timeout/return cleanup
to settle started bounded extraction, with no detached work beyond that
cleanup. R1 explicitly requires cleanup to wait, and says moving guards or
retaining the workspace alone is insufficient. REQ-011 also bounds active
non-code work for the logical invocation across direct and nested origins.

**Locations.** Vala `blocks/agent.rs:410–420`, `blocks/tools.rs:236–250`;
hosted `tools.rs:175–178,304–316`. Actual terminating consumers are Vala
`blocks/engine.rs:359–376,675–688` and server verification
`runner.rs:891–895`.

**Reachable trace.** A supported Agent step has an authored per-attempt
timeout shorter than the execution deadline and one retry. It starts a
direct or nested fs.extract; Invocation::blocking moves the permit and
writer into its TaskTracker blocking closure. If the attempt deadline
expires while extraction is running, engine::attempt's timeout_at drops
actions.execute. This drops AgentExecutor before it reaches close, or
drops its pending close future if an inner stop already reached it.
ToolSession has no drop-time join and the settlement future does not run
independently. The blocking closure continues extracting and can still
persist scratch. The engine classifies the timed-out attempt and immediately
enters its retry loop without waiting for that closure.

The retry opens a fresh ToolSession with a fresh Dispatch semaphore
(`ToolSession::resolve`, tools.rs:317–328). At concurrency cap 1 a fresh
non-code operation can therefore run while the first attempt's extraction
still owns its old semaphore permit. The retry may instead be refused Busy
if detached work still occupies the last workspace. With no retry, the
engine can publish the failed-step result before the old extraction finishes.
The outer engine deadline and queued abandon/lease-loss select also drop
the execution without awaiting this settlement. This is the same missing
termination ownership as the original finding, not a new adjacent feature.

**Proof assessment.** The new owner P2 test correctly proves retention but
explicitly obtains interrupted.work, closes it and awaits work.wait itself.
Production timeout/cancellation does none of those steps. The Vala P2 test
drops one bridge call while its enclosing program remains running, then
releases the blocking task before finishing the Agent; it also cannot prove
invocation cleanup. The appended R1 evidence explicitly acknowledges that
an engine timeout drops the session without awaiting settlement. These
tests are useful partial proofs, but their selector relocation is not
equivalent to the required engine termination observation.

**Material consequence.** A normal configured attempt timeout/retry can
overlap supposedly settled attempt work, exceed the invocation's non-code
cap via fresh session permits, and delay workspace capacity recovery after
the engine has returned or moved on. Owned guards prevent the former writer
race and premature directory deletion; they do not fulfill awaited cleanup.
No synthetic load or unusual input is needed: any bounded extraction still
running at its configured attempt deadline reaches this path.

**Smallest correction boundary.** Finish the existing lifecycle integration
at the owner of attempt/execution termination. Cancellation must stop new
dispatch, and the existing tracked blocking work must settle before the
attempt is classified complete, a retry starts, or invocation cleanup is
declared finished. Keep TaskTracker/owned writer/permit retention and ordinary
partial scratch during cleanup; preserve existing timeout classification,
request-budget charging, query cancellation, Busy behavior and sibling
service availability. Another guard in Workspace::extract, a detached
cleanup task by itself, or an extra post-stop check cannot connect these
caller-owned termination paths. No rollback, process crash, second extraction
service or new public concurrency decision is required by this finding.

**Focused closure proof.** Extend the deterministic start/release seam so
the real AgentExecutor/engine attempt times out while blocking extraction is
held. Assert it has not finished cleanup or started its retry/new-session
operation before release; after release, check cancellation records,
recovered workspace admission and the next authorized operation. Exercise
the actual external cancellation/engine-deadline cleanup owner as well,
or show source-backed equivalent ownership for that drop path. Keep the
current guard-retention and held-writer tests.

## Verification limits

No tests, builds or source mutations were performed, per assignment. Proof
assessment is source inspection and recorded candidate evidence, not an
independent green execution. Scope stayed within changed concurrency and
prior cleanup obligations; unrelated security, SDK and historical contract
evidence carries forward through the orchestrator's separate coverage.

Skipped broad gates and unrelated discovery. Risk: FIND-TASK-001-2 still
permits timed-out attempt work to survive termination and overlap a retry.
