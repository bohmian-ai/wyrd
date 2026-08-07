# Controller State Machine

Use this reference for worktree isolation, durable execution state, task
transitions, commits, and resume behavior.

## Contents

- [Execution boundary](#execution-boundary)
- [Canonical and controller state](#canonical-and-controller-state)
- [Allowed transitions](#allowed-transitions)
- [Commit boundaries](#commit-boundaries)
- [Resume and recovery](#resume-and-recovery)

## Execution boundary

Create one dedicated local branch and clean worktree from the caller's resolved
`HEAD`. Use a unique branch such as
`wyrd/implement-<plan-slug>-<UTC timestamp>` after verifying it does not
already exist. Put the worktree under a task-specific temporary parent rather
than a broad or user-owned target.

Never mutate the caller's worktree. In particular, do not stash, reset, clean,
checkout paths, commit caller changes, or delete either worktree. Preserve the
dedicated worktree and branch after completion or blockage.

Record in the canonical plan's execution discoveries:

- invocation baseline branch and full SHA;
- execution branch and absolute worktree path;
- whether plan artifacts were imported;
- execution-baseline commit when one was required;
- each task's risk tier and the ladder rung reached, when it escalated past the
  first remediation.

When the supplied plan artifacts are absent or modified relative to the
baseline, import only the caller-selected plan directory into the execution
worktree and validate it before product edits. Check every imported path with
Git before staging. If any artifact is ignored, keep it local and untracked;
create a plan-baseline commit only for non-ignored artifacts intended to be
repository-tracked. Never override ignore rules. Do not import unrelated caller
changes.

## Canonical and controller state

The Wyrd task packet remains the durable source of truth:

| Controller state | Canonical task status |
|---|---|
| `READY` | `Ready` |
| `IMPLEMENTING` | `Ready` |
| `REVIEWING` | `Ready` |
| `REMEDIATING` | `Ready` |
| `ACCEPTED` | `Complete` |
| `EXTERNAL_BLOCKED` | `Blocked` |

Append controller attempts, findings, decisions, commands, results, and commit
references under the task's controlled completion evidence. Update the parent
plan's execution discoveries and requirement-to-proof mapping. Do not create a
second status file, run ledger, sentinel, or duplicate specification.

## Allowed transitions

```text
READY -> IMPLEMENTING
IMPLEMENTING -> REVIEWING
IMPLEMENTING -> REMEDIATING
REVIEWING -> ACCEPTED
REVIEWING -> REMEDIATING
REMEDIATING -> IMPLEMENTING
REMEDIATING -> REVIEWING
any nonterminal state -> EXTERNAL_BLOCKED
```

`EXTERNAL_BLOCKED` is allowed only after the orchestrator proves that required
external credentials, permissions, infrastructure, or user-owned authority is
unavailable and no local or equivalent proof can complete the outcome.

Before dispatch, validate that every named dependency is `ACCEPTED`. A task
cannot be accepted until:

- implementation result is `COMPLETE`;
- all required focused verification is `PASS`;
- task review verdict is `APPROVE`;
- no unresolved review finding remains;
- the task diff contains no unrelated change.

## Commit boundaries

Use local, append-only, fix-forward history:

1. Optional execution-baseline commit containing only non-ignored imported plan
   artifacts intended to be repository-tracked.
2. One checkpoint commit after each task becomes `ACCEPTED`.
3. One fix-forward commit for each reopened accepted task.
4. One final closeout commit for integrated evidence and plan completion.

The accepted-task commit contains its implementation, tests, generated outputs,
canonical task/plan updates, verification evidence, and review disposition.
Ignored canonical plan/task updates remain local execution state and are not
part of the commit. Stage exact non-ignored paths and inspect the staged
name-status diff before committing. Verify the staged paths with
`git check-ignore --no-index` and unstage any ignored path.

Never push, merge, amend, rebase, squash, reset, or rewrite accepted commits.
Never use `git add -f`, `git add --force`, broad staging (`git add .` or
`git add -A`), `git commit -a`, or any force-push variant. A skill or plan that
asks for an artifact under an ignored path authorizes writing it locally, not
tracking it. Plan execution has no override for that tracking prohibition.
Never add AI authorship trailers or change Git identity.

## Resume and recovery

Resume from the dedicated worktree, canonical plan/task files, local commit
history, current diff, and recorded evidence. Conversation history is not
authority.

On resume:

1. verify branch, worktree, baseline, and Git identity;
2. validate plan artifacts;
3. inspect commits since the execution baseline;
4. identify the first task not `Complete`;
5. reconstruct its controller state from completion evidence and current diff;
6. validate dependency acceptance;
7. continue from the first incomplete transition.

Run the reconstruction rather than deriving it by hand:

```bash
python3 <skill-dir>/scripts/resume_state.py \
  --plan-dir <plan-dir> --worktree <worktree> --baseline <baseline-sha>
```

It returns branch, head, dirty state, commits since the baseline, every task's
status, and the first task that is not `Complete` with its reconstructed
controller state.

**Every agent from the prior session is dead.** An `agentId` does not survive a
session boundary, so do not attempt to resume one — `SendMessage` to a stale ID
is not a recoverable path. Spawn replacements with the canonical task, relevant
plan decisions, current diff, findings, and evidence. Do not ask the user to
restate the plan.

The same distinction applies mid-session: an agent reporting partial progress
is idle and should be resumed with `SendMessage`; an agent that is genuinely
unreachable is replaced.
