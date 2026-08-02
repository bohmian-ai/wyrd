---
name: wyrd-implement-plan
description: Autonomously execute or resume a complete Wyrd implementation plan through every dependency-ordered task, orchestrating implementation and review subagents while retaining plan truth, material decisions, commits, and closeout. Use when asked to implement, continue, finish, or close an entire Wyrd plan supplied in conversation or by plan-file path; use wyrd-implement for a single non-UI task and wyrd-ui for a single UI task.
---

# Wyrd Implement Plan

Run the complete plan as a manager-controlled agentic loop. You are the root
orchestrator: you own plan truth, task order, material decisions, integration,
commits, and the user-facing outcome. Implementation and review agents are
bounded tools; they never take over plan control.

Continue until the plan is `COMPLETE` or genuinely unavailable external
authority makes correct completion impossible. A worker report, test failure,
plan defect, material decision, context reset, or elapsed time is not a
terminal result.

`$name` denotes a Wyrd skill; load it with the `Skill` tool.

## Load the controller contract

Read the complete supplied plan and every task before dispatching work. Read
repository `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and every
authority named by the plan.

Read these skill resources completely when their stage begins:

| Resource | Read when |
|---|---|
| `references/controller-state-machine.md` | Establishing the worktree, task states, commits, resume behavior, or transitions |
| `references/orchestrator-authority.md` | Repairing a plan, resolving a blocker, or changing product, security, architecture, or contracts |
| `references/model-routing.md` | Selecting, escalating, or replacing an implementation or review agent |
| `references/agent-contracts.md` | Dispatching an implementor, reviewer, or plan reviewer |
| `references/integrated-closeout.md` | Running milestone/final gates, reopening a task, final review, or completion reporting |

Read `architecture/references/README.md`, then preserve its progressive
architecture routing. Every implementation and review agent must load its own
skill and the conditional Wyrd references required by its affected surface. Do
not replace those references with a summarized task prompt.

When `.codegraph/` exists, use CodeGraph before grep, find, or manual source
discovery.

## Agent roles and models

Model assignment is fixed by role, not by task risk:

| Role | `subagent_type` | Model |
|---|---|---|
| Root orchestrator | — (this context) | Opus |
| Implementation | `wyrd-implementor` | Sonnet |
| Task and final review | `wyrd-reviewer` | Opus |
| Material plan revision | `wyrd-plan-reviewer` | Opus |

`Luna`, `Terra`, and `Sol` in a task packet are **risk tiers**, not model
names. They set task scope, reviewer rigor, and how quickly you escalate on
repeated failure. They never select a model. See `references/model-routing.md`.

Only one write-capable implementation agent runs at a time. Read-only
investigation may be delegated concurrently only when it cannot race with
source or plan mutation.

## Establish isolated execution

Invocation authorizes local implementation, focused and integrated
verification, a dedicated local branch/worktree, plan/task updates, and local
checkpoint commits. It does not authorize pushing, opening a PR, merging,
rewriting history, changing Git identity, or modifying the caller's worktree.

Run preflight through the workflow script:

```javascript
Workflow({
  scriptPath: '.claude/skills/wyrd-implement-plan/workflow.js',
  args: { planPath: '<plan dir or file>', repoPath: '.', stamp: '<UTC timestamp>' },
})
```

Supply `stamp` yourself — workflow scripts cannot call `Date.now()`. It returns
`{ok, slug, planPath, closeoutCommands, branch, worktree, baselineSha,
callerDirty, importedArtifacts, tasks[]}`. If `ok` is false, resolve the
reported errors before any product edit; do not dispatch into an unvalidated
plan or a missing worktree.

Never stash, reset, clean, commit, copy back, or otherwise absorb the caller's
tracked or untracked changes. Retain the execution worktree and local branch at
handoff so the result is inspectable.

## Preserve Git ignore and history boundaries

Treat Git ignore rules as repository authority, including for `.dev/` plans,
task packets, execution evidence, PR summaries, and generated local state.

- Never run `git add -f`, `git add --force`, or an equivalent index operation
  that stages an ignored path.
- Never force-add an ignored file or directory, even when this skill, another
  skill, a plan, or a task requires that artifact to be written or updated.
  Keep it local and untracked. Plan execution has no override for this rule.
- Stage exact non-ignored paths. Do not use broad staging commands such as
  `git add .`, `git add -A`, or `git commit -a` during plan execution.
- Before every commit, inspect the staged name-status diff and verify staged
  paths with `git check-ignore --no-index`. Unstage any ignored path before
  committing.
- Never force-push (`--force`, `-f`, or `--force-with-lease`). Plan execution
  does not authorize any push; a later explicit push request authorizes only a
  normal fast-forward push. This skill never rewrites remote history.
- Never add AI authorship trailers or change Git identity.

## Repair plan truth autonomously

Preflight already ran the canonical plan validator. Inspect live repository
reality, task dependencies, execution skills, risk tiers, commands, features,
setup, acceptance criteria, and closeout gates.

When the plan or a task is incomplete, contradictory, stale, structurally
invalid, incorrectly decomposed, or not executable:

1. reconstruct the intended outcome from the complete artifact and current
   repository authority;
2. revise the canonical plan and affected task packets;
3. update affected requirements, decisions, consumers, tests, downstream
   tasks, plan version, and design authority cohesively;
4. re-run `python3 .claude/skills/wyrd-plan/scripts/validate_plan_artifacts.py <plan-dir>`;
5. dispatch a `wyrd-plan-reviewer` agent in advisory mode for every material
   revision;
6. resolve its findings autonomously and repeat until `ADVISORY_APPROVE`;
7. continue execution.

Do not return a defective plan to the user for routine planning. Stop only
when the supplied artifact establishes no discernible outcome or genuinely
unavailable external authority prevents any correct implementation.

Do not dispatch product implementation until the canonical plan is `Approved`,
the active task is `Ready`, structural validation passes, and any material
repair has advisory approval.

## Execute one serial task cycle

Keep exactly one active write task. Ignore plan parallelism hints during this
skill; task implementation, review, remediation, acceptance, and commits are
serial.

```text
READY -> IMPLEMENTING -> REVIEWING -> ACCEPTED -> next READY task
             ^               |
             |               +-> REMEDIATING
             +-----------------------+
```

For every task:

1. Confirm all dependencies are `ACCEPTED`.
2. Validate the dispatch boundary:
   ```bash
   python3 .claude/skills/wyrd-implement-plan/scripts/validate_execution_state.py \
     dispatch --task <ID> --state READY --dependency <DEP>=ACCEPTED
   ```
3. Dispatch a fresh implementor and **record its `agentId`**:
   ```javascript
   Agent({
     subagent_type: 'wyrd-implementor',
     name: 'impl-<task-id>',
     run_in_background: true,
     description: 'Implement <task-id>',
     prompt: <the complete contract from references/agent-contracts.md>,
   })
   ```
4. Keep that implementor through ordinary diagnosis and remediation. If it
   returns a nonterminal progress or incomplete report, **resume the same
   agent** rather than replacing it:
   ```javascript
   SendMessage({ to: '<agentId from the spawn result>', summary: '...', message: '...' })
   ```
   This preserves its context. A self-reported turn or context limit is not a
   platform fact and is not grounds for replacement. Put every instruction
   inside `message` — your plain text output is not visible to the agent.
5. Require focused verification and a structured implementation result. During
   task implementation, only named tests added or changed by the task and their
   directly affected adjacent tests may run. Whole-crate suites, unfiltered
   package integration lanes, fuzz lanes, journeys, and aggregate gates are
   milestone or final integration proof, not task proof, even when a task
   packet names them.
6. Dispatch a fresh `wyrd-reviewer` agent in plan-execution binding mode. Pass
   the computed task delta in the prompt so the reviewer does not need to fetch
   it. Reviewers are always fresh for a first review and retained for focused
   re-review of their own findings.
7. **Audit the review before accepting its verdict.** Reject a shallow
   `APPROVE` that relies on the implementation report, task status, test names,
   passing counts, or requirement restatement. Every traceability-matrix row
   must carry a concrete `path:line` into the delta plus a source-derived
   explanation of how the invariant is enforced and which assertion fails on
   regression. A verdict token alone is never acceptance evidence. This audit
   is yours — it cannot be delegated to the reviewer being audited.
8. Route `RESUME_IMPLEMENTATION` findings back to the implementor via
   `SendMessage`.
9. Resolve `ORCHESTRATOR_DECISION_REQUIRED` and `REVIEW_BLOCKED` at the root;
   revise the task or plan when needed, then resume.
10. Repeat review until it returns a substantiated `APPROVE`.
11. Validate acceptance, which enforces the citation rule mechanically:
    ```bash
    python3 .claude/skills/wyrd-implement-plan/scripts/validate_execution_state.py \
      accept --task <ID> --state REVIEWING --implementation COMPLETE \
      --focused-verification PASS --review APPROVE --unresolved-findings 0 \
      --diff-base <sha> --matrix-rows <N> --cited-rows <N>
    ```
12. Update the canonical task to `Complete`, update plan progress/evidence, and
    create one local task checkpoint commit.

Canonical task status remains `Ready` throughout implementation, review, and
remediation. Set it to `Complete` only after focused proof passes and the
independent review approves. Use `Blocked` only for genuinely unavailable
external authority.

Do not create or require a separate implementation-owned traceability ledger.
Agents can forget to update it or mark work complete without proof. The
canonical task defines scope; reviewers must reconstruct conformance from the
current code and tests on every pass.

## Resolve blockers at the root

An implementor's `BLOCKED` result is a request for orchestrator judgment, not a
plan outcome. Inspect its evidence independently.

- Return reversible or bounded work to the same implementor with a binding
  correction.
- Escalate per the ladder in `references/model-routing.md` when risk or
  repeated failure exceeds the current attempt. Escalation raises scope and
  authority, not model tier.
- Repair material plan decisions autonomously and obtain advisory
  `wyrd-plan-reviewer` approval before resuming.
- Never use a fixed retry limit. Repetition triggers a step up the ladder, not
  another identical retry and not abandonment.

You may change requirements, acceptance criteria, product behavior, security
semantics, public or persisted contracts, migrations, dependencies, features,
ownership, and task structure when the result is repository-grounded,
cohesive, reasonable, and no weaker in correctness, security, ergonomics, or
user experience. Record the decision and update every affected authority, task,
test, and verification path.

## Resume

Conversation history is not authority. On resume, reconstruct from disk:

```bash
python3 .claude/skills/wyrd-implement-plan/scripts/resume_state.py \
  --plan-dir <plan-dir> --worktree <worktree> --baseline <baseline-sha>
```

It returns the branch, head, dirty state, commits since baseline, every task's
status, and the first task that is not `Complete` with its reconstructed
controller state.

Every agent from the prior session is gone. Do not try to reattach a stale
`agentId`. Spawn replacements seeded from the canonical task, relevant plan
decisions, current diff, findings, and evidence — never from a conversational
summary, and never by asking the user to restate the plan.

## Integrate and close

Run plan-defined milestone and final verification as described in
`references/integrated-closeout.md`. If integrated proof fails, reopen the
earliest responsible accepted task, remediate it, repeat focused review, and
create a fix-forward commit. Never amend or rewrite an accepted task commit.

After all tasks and final gates pass, dispatch one fresh plan-wide
`wyrd-reviewer` in plan-execution binding mode. Resolve confirmed findings
through the same task cycle. Do not invoke `review-and-plan`; it remains a
separate user-invoked terminal branch review.

Before that final review, compare every accepted task's owned symbols,
contracts, and consumers with subsequent commits. Reopen any accepted task
whose invariant-bearing surface changed, even if integrated verification is
green. The final reviewer must inspect the cumulative branch and may not treat
earlier approvals as proof of the current implementation.

Validate closeout:

```bash
python3 .claude/skills/wyrd-implement-plan/scripts/validate_execution_state.py \
  closeout --task <ID>=ACCEPTED ... --integrated-verification PASS \
  --final-review APPROVE --unresolved-findings 0
```

Create a final closeout commit containing canonical plan completion state and
integrated evidence. Report `COMPLETE` only when every task is accepted, all
required focused and integrated proof passes, the final review approves, no
confirmed finding remains, and the dedicated branch contains no unrelated
change.
