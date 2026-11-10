---
id: SUPPLEMENT-verifier-waits-and-approvals
revision: 4
status: draft
parent: SPEC-verifier-review-pipelines
parent_revision: 21
---

# Durable waits and human approval inside a Verifier

## Agreed intent and existing constraints

The user requested new wait and human-approval steps as part of code review,
so a Verifier can suspend, resume at a time or on an action, and release
execution resources while suspended. The user explicitly requires recent
interactive human login for approval; a User API key is not sufficient.
The user has also approved SubmissionSink as the queue-admission owner for
initial and resumed work: approval/timer eligibility creates a new queue work
item referencing the same suspended execution, rather than a new execution.
Approval requires a separate Verifier approval grant, selected explicitly by
the user; the existing Verifier run grant alone is insufficient. Scope it to
the exact Verifier UID or the existing tenant-wide Verifier scope. Existing
grants and built-in roles are not automatically broadened.

Code review is the first use case. Existing Wyrd doctrine keeps these as
Verifier capabilities in the current engine, not new Card kinds, a general
workflow service, Trigger activation kinds or Operator reactions. Existing
Bifrost execution-state ownership and PostgreSQL queue coordination remain
constraints. These are inherited boundaries, not additional user decisions.

Example: independent reviewers produce their reports. A human-approval step
pauses its dependents while unrelated ready steps can continue. An eligible
human approves the exact execution, then its dependents become eligible under
normal admission. If no work remains active or runnable, the suspended
execution releases its worker permit and renewable lease. Restart restores the
pending step and accepted work. What rejection does is not yet decided.

```text
Initial admission: input acceptance -> SubmissionSink -> initial queue work
Execution: claim -> restore/run -> accept suspended checkpoint -> release
Resumption: decision/time eligibility -> SubmissionSink -> new resume queue item
            -> claim -> restore the same execution -> continue eligible steps
```

Bifrost never inserts a PostgreSQL row or schedules a worker. The Rust server
receives Bifrost acceptance and asks SubmissionSink to commit admission. The
verification engine consumes admitted work; queue admission is outside its
execution loop. LeaseRenewal is a separate active-worker lifecycle owner.

This document records capability intent and the decisions still needed before
the proposed implementation task can be marked ready. It is not a decision-complete public or
persisted contract. The existing TASK-001-R4 remediation proceeds independently
under revision 18 and current user corrections; new waits are not its scope.

## Required outcomes

- Add a wait step supporting timed and action-driven continuation, and a
  human-approval step, through the existing Verifier execution owner.
- Retain the pending logical step, its timing and any accepted action/decision
  canonically in Bifrost so supported restart recovery can resume the same
  execution without repeating acknowledged predecessors.
- A pending step blocks its dependents, not independent ready work. Release
  that step's active execution capacity while waiting. When the whole execution
  is suspended, retain no worker permit, renewing worker lease, provider
  connection or sleeping future solely to keep the wait alive. Durable storage
  and bounded coordination necessarily remain costs.
- Restore execution capacity through normal bounded admission when eligible.
  Suspension and resumption are not failed computation attempts and must not
  silently replenish retries, model budgets or the original execution deadline.
  Existing cancellation, revocation and repository-head supersession still apply.
- Require current authorization and recent interactive human login for an
  approval. User identity, a service/delegation chain or an API key alone cannot
  manufacture that proof. The approver's identity must never replace or expand
  the execution's accepted authority.
- Bind any continuation to the exact tenant, execution and logical step
  occurrence, including fanout/iteration identity. Approval of one PR head must
  not approve a later head. Recover accepted decisions and handle delivery
  retries without silently turning them into additional human decisions.
- Use existing HTTP/client/SDK/CLI/MCP and inspection owners under repository
  surface-parity rules. The optional UI cannot own durable resumption or
  provide an authorization bypass. Exact operations and interactive login flow
  are undecided below.

These outcomes extend the existing durability contract; they do not strengthen
Bifrost's acknowledged-write failure domain. In particular, retained-volume WAL
restart is different from permanent loss of unpublished WAL. The accepted rare
old-worker overlap limitation remains; this draft does not introduce a new
SQL ownership fence or claim universal exactly-once decision effects.

## Approved queue identity and ownership

The workflow keeps its original ExecutionId for status, results, evidence,
budgets, authority and history. Every initial/resume queue delivery has its own
work-item identity. Retry of the same continuation preserves its delivery
identity, so ambiguous queue COMMIT or a lost response cannot insert another
logical activation. A later genuine continuation gets another delivery identity.
Queue claim/lease/settlement fences apply to that delivery. Normal queue
coordination prevents multiple deliveries from intentionally running the same
execution concurrently; this does not add the rejected distributed SQL checkpoint
head or strengthen the accepted rare delayed-old-worker interval.

Existing `wyrd.verifier_executions` coordination must be extended coherently;
there is no approved second queue or per-execution SQL recovery payload. Initial
admission idempotency, schedule/group occurrence selection and cursor movement
remain initial-admission effects. Resumption cannot repeat them, clear prior
terminal state or create additional Operator/Check side effects. Queue migrations
are ordered and preserve installed data. Ordinary suspension/resumption neither
consumes a failed-computation allowance nor resets the execution-wide budget.

## Material decisions not yet agreed

The earlier draft selected policies and wire shapes without user agreement.
They have been removed from mandatory requirements and acceptance criteria.
None of the following choices is a default authorization to implement it:

| Decision | What must be settled and why |
| --- | --- |
| Entry modes | Whether waits initially require queued submission, and how direct verification behaves when its Verifier reaches a wait. Do not silently reject direct invocations or convert them into background execution. Existing direct queue-free/fast-return behavior is preserved until explicitly revised. |
| Rejection | Whether rejection produces a recorded step decision for existing conditions/requirements or automatically cancels the execution. The explicit question is pending; do not select it silently. Existing deadline expiry retains its current lifecycle. |
| Action-driven resumption | The permitted action/continuation input, whether it carries typed data, and behavior when it arrives before the step is pending or after resolution. Do not silently ban payloads or pre-signals. Existing no-free-form-event-vocabulary doctrine still applies. |
| Authoring and composition | Exact action variants, fields, timing/output representation and interaction with repeat/fanout/conditions/speculation. No speculative-combination ban or particular field spelling has been approved. |
| Public and login surfaces | A separate Verifier approval grant and recent interactive login are selected. Exact permission wire spelling, HTTP/client operations, and how clients reach interactive proof remain to be fixed. A new approval-session API or link protocol has not been selected. Reuse existing auth owners. |
| Transition and race semantics | SubmissionSink admission and stable delivery identity are agreed. Select the durable recovery of decision acceptance before queue commit, the command acknowledgement promise, and conflict/expiry/cancellation behavior under existing controls and the accepted old-worker limit. Do not imply an atomic Bifrost/PostgreSQL transaction or new distributed fencing. |

Before whole-contract approval and task readiness, each decision needs one
selected behavior and corresponding observable proof. Merely calling choices implementation details would delegate
product, security and persistent-contract decisions to the implementer.

Approval eligibility itself is settled by the user's selected separate grant
and recent interactive login. No additional self-approval ban or quorum
mechanism is introduced by this scope.

## Ownership and feasibility to establish before task handoff

Bifrost remains the canonical recovery source for pending waits, accepted
continuations and approval provenance. PostgreSQL remains coordination, not a
second payload store or per-execution checkpoint-head authority. No new
recovery database, writer or lease system is authorized.

SubmissionSink is the selected owner for initial and resumed queue admission;
the existing scheduler/claim loop supplies due-time discovery and execution.
Extend these owners, not a second queue writer, workflow scheduler or lease
system. Their current implementation has no resume admission variant and
deduplicates submissions by ExecutionId, so new delivery identity is necessary.
They have not been shown to supply a replayable parked-execution wake-up mechanism.
The later design must explain recovery from both of these interruptions:

1. Pending wait accepted, process dies before coordination releases/parks work.
2. Continuation accepted, process dies before runnable coordination is recorded.

An acknowledged continuation cannot depend solely on a volatile callback or
require another human approval after restart. The exact recovery/acknowledgement contract remains a material design decision
that blocks task readiness; the new task records the current source-backed
reuse and its missing behavior. Do not prescribe
an unbounded history scan, new sidecar or SQL payload mirror to fill that gap.

Existing coordination uses PostgreSQL's authoritative clock. Fixed chosen
times must survive restart; waiting must not restart its duration. New timeout
defaults, a longer recovery horizon or expanded retention remain separate
policy decisions, not implied changes. Existing accepted input/output and
completed File custody must remain sufficient for the supported wait interval.

## Acceptance evidence for agreed outcomes

| Parent obligation | Required observation; unresolved details stay outside acceptance |
| --- | --- |
| REQ-060; INV-020; AC-052 | A declared timed wait survives restart with its original chosen time and execution bounds; selected entry-mode behavior is tested after it is decided. |
| REQ-061; INV-020; AC-053 | Independent work continues while a step waits; a fully suspended execution releases worker permits and renewable leases. An authorized action resumes the same logical execution under bounded admission without rerunning accepted predecessors. |
| REQ-062; INV-021; AC-054 | Recent interactive human approval can continue the workflow; a User API key, automation/delegation without human presence, stale login, foreign tenant or insufficient authority cannot satisfy approval. Eligibility follows the separate grant plus recent login, with no additional initiator ban. Rejection outcomes await the pending decision. |
| REQ-063; INV-022; AC-055 | Retried delivery does not silently create another decision or target another iteration/head. Conflicts/expiry/cancellation are tested against the selected transition contract and its declared old-worker limitation, not an invented universal fence. |
| REQ-064; INV-023; AC-056 | Controlled crashes at pending/continuation and coordination boundaries restore accepted state and progress without another approval; unavailable canonical state cannot imply fresh state. |
| REQ-065; INV-024; AC-057 | Original authority, evidence, budgets and deadlines survive suspension; cancellation/revocation/supersession prevent unauthorized continuation. No permanent-volume-loss promise is added. |
| REQ-066; AC-058 | Shipped HTTP/client/SDK/CLI/MCP operations use the same server-owned control and human-login checks; optional UI is not the authority. Exact interaction journeys follow the chosen auth/public contract. |
| REQ-068; INV-025–026; AC-060 | Initial and resume admissions go through SubmissionSink; a fresh queue work item references the same ExecutionId. Retry/lost-queue-COMMIT response deduplicates delivery, and resumption repeats neither initial admission effects nor accepted computation. |
| REQ-067; AC-059 | Inspection distinguishes suspended progress from terminal success; code-review examples show timed/action/human continuation, restart and recorded evidence. Existing Verifiers retain their behavior. |

Use real public-client/server journeys, existing queue/Bifrost/auth integration
seams and controlled time/crash injection. Do not sleep for hours, generate
synthetic host load or claim runtime proof during drafting. Compatibility and
rolling-version tests must cover whatever persistent/public variants are
actually selected. Exact selectors belong in the later implementation task.

## Inspected owners and authority amendments

- `wyrd-spec/src/card/verifier/mod.rs::Action` and existing Step/validation own
  authoring; the current actions are extract, agent and code. New wait/approval
  variants require explicit contract selection, not a second workflow model.
- Existing walker/Scope/RunState own logical identity, readiness and restore.
  Durable suspension is missing; ordinary async I/O is not that capability.
- `wyrd-server/src/verification/claims.rs` and `runner.rs` own permits, leases,
  admission and retries. Their parked/resumable extension is not yet proven.
- Existing verification routes and `wyrd-client::verify::Execution` own
  control and inspection; no new route names are selected by this draft.
- `wyrd-server/src/auth/browser.rs::RecentHumanLogin` is existing password/SSO
  human-presence proof, currently limited to a five-minute browser session and
  excluding API-key/bearer requests. Reusing/extending that owner across the
  selected client flows requires explicit auth-contract work; User kind alone
  does not prove interactive presence.
- Before new task handoff, reconcile chosen Action/control/permission/status
  semantics in `architecture/wyrd-design.md`, human proof and authority in
  `architecture/wyrd-security-posture.md`, and wait custody/recovery in Bifrost
  and operations owners. Update `public-contracts.md` and examples consistently.
  The baseline durability supplement's no-public-resume statement would be
  superseded only by the explicitly approved new scope, not retroactively.

## Approval, provenance and task home

The user explicitly agreed to the capability, recent interactive human login,
a separate Verifier approval grant, SubmissionSink ownership and a new queue
work item resuming the same execution,
then requested this specification update and TASK-007. Those decisions are
approved; outstanding policies above are not silently included in that approval.

[TASK-007](tasks/TASK-007-durable-waits-and-approval.md) is the sole proposed
implementation home for delay/wait, action/timer triggers of resumption, human
approval, suspension/resource release, new delivery identity and necessary
consumer closure. Its contract-dependent portions are not ready until the
remaining decisions are resolved. Existing TASK-001 stays implemented/history;
all of its existing follow-ups remain in R4. TASK-007 does not absorb foundation remediation findings.

Revision 3 records the newly approved queue ownership/identity and creates the
requested task home. Revision 2 removed unagreed policy/wire choices from the
first draft. No implementation, tests or measurements are claimed by planning.

## Approved settlement ordering carried into TASK007

The user-approved terminal amendment in durability supplement3 also governs the
end of a suspended queue delivery: write the canonical waiting checkpoint, await
the existing Bifrost WAL/batch-commit ACK, then retire that delivery and release
all active resources. Suspension is nonterminal for the workflow. A later trigger
uses SubmissionSink to admit new delivery of the same execution. No process-only
callback may replace durable delivery responsibility. This clarifies ACK ordering;
remaining authoring/approval/continuation contracts in this draft stay open.
