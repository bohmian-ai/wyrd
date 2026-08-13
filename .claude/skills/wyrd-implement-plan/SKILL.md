---
name: wyrd-implement-plan
description: Autonomously execute or resume a complete Wyrd implementation plan in one persistent root session. Use when asked to implement, continue, finish, or close an entire Wyrd plan supplied in conversation or by plan-file path. The root implements every dependency-ordered task itself through wyrd-implement and, when needed, wyrd-ui; fresh Opus/low subagents invoke wyrd-review after every task and full review-and-plan at terminal closeout.
---

# Wyrd Implement Plan

Execute the complete plan in this root session. The root is the only writer: it
owns plan truth, task order, implementation, verification, material decisions,
task status, commits, remediation, and the user-facing outcome.

Use subagents only as independent read-only reviewers. Never delegate product,
test, plan, evidence, worktree, or Git mutations.

Continue until the plan is `COMPLETE` or genuinely unavailable external
authority makes correct completion impossible. A failure, finding, plan defect,
context reset, or elapsed time is not a terminal result.

`$name` denotes a skill; load it with the `Skill` tool.

## Load the execution contract

Read the complete plan and every task before editing. Read repository
`AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`,
`architecture/wyrd-doctrine.mdx`, and every authority named by the plan.

Read these resources completely when their stage begins:

| Resource | Read when |
|---|---|
| `references/execution-boundary.md` | Establishing or resuming the worktree, commits, plan state, or blockers |
| `references/task-cycle.md` | Implementing, verifying, reviewing, remediating, or accepting any task |
| `references/closeout.md` | Running milestone/final gates, terminal review, or completion reporting |

Read `architecture/references/README.md` and load only the architecture slices
required by the affected surface. Use CodeGraph before grep, find, or manual
source discovery when `.codegraph/` exists.

## Establish isolated execution

Follow `references/execution-boundary.md`. The root validates the supplied plan,
inspects Git status and identity, and creates one dedicated local branch and
clean worktree from the caller's resolved `HEAD`. Do not call a workflow or
subagent to perform these mutations. Perform every subsequent read, edit,
command, review dispatch, and commit from that worktree.

Invocation authorizes local implementation, focused and integrated
verification, canonical plan/task updates, a dedicated local branch/worktree,
and local checkpoint commits. It does not authorize pushing, opening a PR,
merging, rewriting history, changing Git identity, or modifying the caller's
worktree.

Validate imported or supplied plan artifacts before product edits. Repair stale
private mechanics autonomously. When repository evidence invalidates a material
requirement, contract, security rule, ownership boundary, migration,
dependency, or acceptance outcome, revise the canonical plan and affected
tasks cohesively and invoke the read-only `wyrd-plan-reviewer` agent until it
returns `ADVISORY_APPROVE`.

## Execute every task in the root session

Keep exactly one active task and follow dependency order. Ignore plan
parallelism hints. For each task, load and apply `$wyrd-implement` directly;
also load and apply `$wyrd-ui` when its write set enters the Wyrd UI tree.

```text
READY -> ROOT_IMPLEMENTING -> ROOT_VERIFYING -> WYRD_REVIEW
             ^                                      |
             +--------------- ROOT_REMEDIATING <----+
                                                    |
                                               ACCEPTED
```

The canonical task remains `Ready` through implementation, verification,
review, and remediation. Set it to `Complete` only after focused proof passes
and an explicit `$wyrd-review` returns a substantiated `APPROVE`.

For every task:

1. Confirm all dependencies are accepted.
2. Establish the task contract, current owners and mechanics, required tests,
   verification, and material stop conditions.
3. Implement the complete task in the root session. Continue through bounded
   diagnosis and repair while an in-scope recovery path remains.
4. Run focused verification and audit the complete tracked and untracked diff.
5. Spawn a fresh `wyrd-reviewer` agent. It is pinned to Opus/low and must load
   `$wyrd-review` in plan-execution binding mode. Give it the approved plan,
   active task, last accepted commit, complete current task delta, canonical
   completion evidence, exact verification evidence, and applicable authority.
6. Reject a shallow `APPROVE`. Require the complete binding-mode matrix,
   path-and-line citations, source-derived enforcement, exact regression
   assertions, evidence audit, inspected surfaces, and adversarial probes.
7. Validate findings against source. Record dispositions and implement every
   confirmed bounded correction directly. Rerun affected proof and resume the
   same reviewer for focused re-review until `APPROVE`.
8. Resolve `ROOT_DECISION_REQUIRED` at the root. Revise the plan/task and use
   `wyrd-plan-reviewer` when material, then implement and resubmit. Resolve
   `REVIEW_BLOCKED` locally when evidence or authority is available.
9. Mark the task `Complete`, update plan evidence, and create its acceptance
   checkpoint commit only after review approval.

Do not create implementation-agent reports, subordinate remediation packets,
implementation-agent handoffs, or a separate controller ledger. Canonical
artifacts, Git history, source, and recorded verification are durable state.

## Resume

Conversation history is not authority. Resume from the dedicated worktree,
canonical artifacts, Git history, current diff, and evidence. Use the read-only
helper when available:

```bash
python3 .claude/skills/wyrd-implement-plan/scripts/resume_state.py \
  --plan-dir <plan-dir> --worktree <worktree> --baseline <baseline-sha>
```

Reconstruct the active task and continue in the root session. Prior reviewer
agent IDs are not durable; spawn a fresh reviewer when the active review cannot
be resumed.

## Integrate and close

Follow `references/closeout.md`. Run plan-defined milestone gates, reopen the
earliest responsible accepted task when later work changes its invariant, and
repeat focused `$wyrd-review` before accepting the repair.

After every task is accepted, run integrated verification, commit the verified
implementation, then spawn a fresh Opus/low subagent and explicitly instruct it
to invoke full `$review-and-plan` against the committed execution baseline with
the approved Wyrd plan as the intent reference. The root validates and
implements every confirmed finding, reruns affected proof, and commits fixes
forward. Repeat full review after material or cross-task remediation.

Report `COMPLETE` only when every task is accepted, all focused and integrated
proof passes, terminal review is clean, no unrelated change remains, and the
dedicated branch contains inspectable append-only history.
