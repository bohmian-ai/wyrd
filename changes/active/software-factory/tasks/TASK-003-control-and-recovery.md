---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-software-factory
spec_revision: 2
spec_status: approved
requirements: [REQ-007, REQ-008, REQ-011, REQ-012, REQ-013, INV-003, INV-004, INV-005, AC-006, AC-007, AC-011, AC-012]
depends_on: [TASK-002]
---

# Control and recover packet execution without losing work

## Outcome and Value

The maintainer can pause, cancel, resume, retry, or replace a session without
losing the packet or starting competing writers. A service crash leaves an
honest interrupted state and recoverable files. Feedback remains visible even
when its delivery is uncertain. A stopped factory can be backed up and restored.

## What Changes and What Must Stay True

Extend the existing service/session/workspace owners from TASK-001/002.
Persisted files and native sessions are different things: the former survive
even when the latter cannot resume. Task approval never follows from process
exit, timeout, a recovered transcript, or reaching a remediation limit.

One active attempt globally is the version-1 capacity policy. Skill-delegated
subagents belong to that attempt and must not outlive a confirmed stop in a way
that can continue writing. Unknown ownership blocks dispatch rather than
causing a guessed retry. Backup/restore is quiescent and excludes configured
credentials and browser/Slack authentication secrets by default.

## Approach

1. Make attempt creation, transition, and control requests durable and audited.
2. Add stop confirmation and startup reconciliation before any dispatch.
3. Provide browser pause/cancel/retry/replacement and bounded execution limits.
4. Persist feedback delivery and generate resumable/fresh context from files.
5. Integrate native lifecycle events, post-compaction context refresh, and only
   the missing hook behavior. TASK-004 exposes this same context through MCP.
6. Supply quiescent backup/restore and document host availability limits.

## Ordered Implementation Scenarios

### Scenario 1 — Pause or cancel an attempt and its writing children

**Behavior.** A browser pause interrupts the attempt and displays pausing until
the writer and its children are stopped. No next task starts. Cancel preserves
partial files/history without redispatch. Resume/retry starts only after stop
confirmation and creates a new attempt when execution is restarted. Duplicate
button submissions do not launch another writer. An external edit becomes an
explicit decision rather than a silently accepted candidate.

**RED.** Add the planned control journey with native-process fixtures that
include a writing child. Assert no later writes after confirmed stop, preserved
partial files, and at most one active attempt across two registered packets.

**GREEN.** Implement durable controls, stop confirmation, ownership checks, and
the existing service's global dispatch limit.

**REFACTOR.** Keep lifecycle ownership with the session runner; avoid a second
queue, distributed lease service, or per-agent policy engine.

### Scenario 2 — Recover crashes without duplicate or falsely approved work

**Behavior.** Crash before launch, after launch but before recording a native
session ID, during implementation/review, and after result receipt but before
transition. Restart preserves the last committed state and partial files.
Surviving processes are reconciled before dispatch. Unknown process ownership
stays interrupted. A second factory instance cannot dispatch from the same
state. Recovered results still need the current task/candidate validation.

**RED.** Add the planned fault-injection integration test with explicit
barriers at those boundaries, real SQLite/Git, and surviving fixture processes.
Assert no duplicate writes, no fabricated PASS, and no lost approved transition.
Do not use sleeps as synchronization or PID existence alone as ownership proof.

**GREEN.** Persist attempts before spawning, commit transitions/events together,
and reconcile native process ownership before replacement execution.

**REFACTOR.** Use SQLite transactions and portable process/session capabilities;
do not introduce event sourcing or platform-specific /proc inspection.

### Scenario 3 — Preserve feedback and rebuild a fresh session

**Behavior.** Persist a message before native delivery. Show pending, delivered,
or uncertain status after a crash. Do not automatically resend uncertain
delivery. A replacement implementor receives the original task, actual source
state, current findings/remediation, decisions, and available handoff note even
without the prior transcript. Review always starts independently. Missing
pre-compaction handoff or native resume support does not erase execution state.
Before compaction request a checkpoint; afterward refresh critical instructions
and current task/decision/checkpoint context directly through the native
lifecycle integration before work continues. Preserve the implementation
skill's task/ledger reread requirement. Record the supplied refresh even when
the agent does not request more context. Missing supported post-compaction
refresh is a capability blocker. TASK-004 adds on-demand access to the same
context; this task must not depend on that later MCP implementation.

**RED.** Add the planned feedback/context test with crashes before and after
delivery acknowledgment, absent transcript/handoff, and native resume failure.
Add the focused compaction test below: change a packet decision before
compaction, then assert the current decision, preferences, task references,
checkpoint, and direct recovery instruction are supplied afterward. Repeat
with no checkpoint and unsupported refresh capability. Assert exact input
context and no duplicate automatic feedback delivery.

**GREEN.** Extend message persistence and context generation from the existing
packet/session owners. Use native lifecycle events, adding repository-approved
hooks only for missing lifecycle signals, checkpoint requests, and context
refresh. Startup and refresh use the same context owner. Do not count tool
availability or a pre-compaction note as proof of post-compaction delivery.

**REFACTOR.** Keep raw conversations optional for continuity; do not summarize
them into a competing task contract or treat a hook stop as task acceptance.

### Scenario 4 — Stop unattended work at explicit limits

**Behavior.** The maintainer sees and can change the 45-minute attempt limit
and four-remediation default before starting. Exhaustion requests a stop and
prevents redispatch until extension/resolution. Keep the review skill's
two-failure convergence assessment visible. Display reported usage accurately;
missing usage/cost is unknown, and estimates are labeled estimates.

**RED.** Add the planned limits test with a controlled clock and review results.
Assert dispatch/stop behavior at each boundary and that none grants PASS.

**GREEN.** Enforce the visible limits in the existing runner and expose one
pending extension/stop decision. Stop still requires confirmation.

**REFACTOR.** Reuse control/decision actions; do not add a dollar-budget engine
that cannot stop on delayed provider billing.

### Scenario 5 — Back up and restore a stopped factory

**Behavior.** Refuse backup/restore while any writer is active or uncertain.
A quiescent backup preserves SQLite, referenced logs/context, packet files,
uncommitted work, and the Git history needed by recorded review subjects.
Restore into a separate state directory, reconcile paths and artifact/commit
references, and require fresh credentials before dispatch. Missing/corrupt
objects refuse dispatch. Do not restore by overwriting the original checkout.

**RED.** Add the planned backup test with real Git/SQLite, partial work, and a
completed review. Move the restored data to a different path and assert all
referenced history/evidence remains readable and configured secrets are absent.

**GREEN.** Use supported SQLite and Git backup facilities plus referenced
artifacts; validate the restored set before making it executable.

**REFACTOR.** Keep one documented quiescent recovery procedure; no background
replication, cloud backup, or general repository migration framework.

## Acceptance Criteria

| Obligations | Observation and proof |
|---|---|
| REQ-007, INV-004, AC-006 | Control and crash tests preserve partial work and prevent redispatch while any writer ownership is unresolved, including children and second instances. |
| REQ-008, INV-005, AC-007 | Feedback/context and compaction tests recover without transcript/handoff, directly refresh current instructions after compaction, preserve honest delivery, and retain fresh review identity. |
| REQ-011 (controls/messages audit) | Fault cases commit an ordered control/transition history; persisted actions remain inspectable after restart. |
| REQ-012, AC-011 | Clock/round cases pause rather than approve; usage labels distinguish measured, estimated, and unknown. |
| REQ-013, INV-003, AC-012 (backup/recovery) | A separate restored state retains actual task/candidate/evidence references and refuses dispatch for incomplete objects or unresolved writers. |

## Implementation Reference

TASK-001 owns sessions, messages, decisions, state directory, native permission
handling, and service lifecycle. TASK-002 owns candidate validation and routing.
Inspect those actual symbols, callers, and tests before extension; this task
must not create another scheduler or workspace owner.

The inspected agent_readiness/live.py::settle helper manages smoke-process
termination but has no durable restart ownership or UI controls; it is a
failure-case reference, not a reusable service. The implementation skill's
compaction reread rule and review skill's immutable-subject rule remain the
authority for resumed work. Existing Git history and SQLite transactions/backups
provide the native persistence boundary; no Wyrd SQL/server owner applies.
Likely changes remain in the factory, its focused mise tasks, and runbook.

## Verification and Evidence

All selectors are **planned**; TASK-001 establishes the test runners. Confirm
actual selectors before RED. Fault tests use deterministic barriers and real
subprocesses, temporary repositories, and SQLite rather than mock durability.

| Proof | Exact focused command |
|---|---|
| Controls | `mise exec -- pnpm --dir tools/wyrd-factory exec playwright test tests/journeys/control.spec.ts --grep '^pauses cancels and retries without competing writers$'` |
| Crash recovery | `mise exec -- pnpm --dir tools/wyrd-factory exec vitest run tests/integration/recovery.test.ts -t '^recovers every attempt boundary without duplicate writers or false approval$'` |
| Feedback/context | `mise exec -- pnpm --dir tools/wyrd-factory exec vitest run tests/integration/context.test.ts -t '^recovers feedback and fresh context without a transcript$'` |
| Compaction refresh | `mise exec -- pnpm --dir tools/wyrd-factory exec vitest run tests/integration/context.test.ts -t '^refreshes current instructions after compaction with or without a checkpoint$'` |
| Limits | `mise exec -- pnpm --dir tools/wyrd-factory exec vitest run tests/integration/limits.test.ts -t '^pauses at time and remediation limits without granting approval$'` |
| Backup | `mise exec -- pnpm --dir tools/wyrd-factory exec vitest run tests/integration/backup.test.ts -t '^restores quiescent state history and partial work without credentials$'` |

Add explicit mise tasks factory:backup and factory:restore with required target
arguments; document their actual invocation after implementation. Run focused
proofs, factory:format:check, factory:lints, factory:typecheck, factory:build,
and git diff --check. Exercise real native interruption and session replacement
through factory:smoke:live on both harnesses and record platform-specific
installation/execution observations without introducing different semantics.
Record an actual compaction and the resulting supplied context for each native
harness. A synthetic lifecycle event proves controller behavior only; it does
not establish that the installed harness emits the required signal.

## Material Stop Conditions

Stop if safe recovery requires guessing process ownership, dropping partial
work, weakening native permissions, bypassing immutable review, or implementing
different execution guarantees per platform. Do not conceal unavailable native
resume behind an automatic rerun of uncertain work.

## Authority Links

- [Specification](../spec.md)
- [Dependency](TASK-002-automatic-task-handoffs.md)
- [Implementation execution](../../../implementation-execution.md)
- [Testing](../../../../TESTING.md)
