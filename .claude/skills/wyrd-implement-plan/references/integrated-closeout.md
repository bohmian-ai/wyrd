# Integrated Closeout

Use this reference for milestone gates, final verification, reopened tasks,
plan-wide review, and completion evidence.

## Contents

- [Verification ownership](#verification-ownership)
- [Milestone verification](#milestone-verification)
- [Reopen on regression](#reopen-on-regression)
- [Final verification](#final-verification)
- [Final plan-wide review](#final-plan-wide-review)
- [Completion](#completion)

## Verification ownership

| Stage | Owner | Responsibility |
|---|---|---|
| Task implementation | `wyrd-implementor` agent | Run, diagnose, repair, and record focused task verification |
| Task review | `wyrd-reviewer` agent | Statically audit implementation, test strength, and recorded evidence |
| Milestone/final verification | Root orchestrator | Run integrated plan and repository gates sequentially |
| Material plan revision | `wyrd-plan-reviewer` agent | Statically validate architecture and executability |
| Final plan review | Fresh `wyrd-reviewer` agent | Statically review the complete integrated implementation and evidence |

Do not duplicate trustworthy focused commands during task review. The
orchestrator reruns a command only when later integration changed its proof,
evidence is stale or incomplete, the wrong target/features ran, or risk
requires new integrated proof.

## Milestone verification

At each plan-defined milestone:

1. confirm all milestone tasks are `Complete` and committed;
2. inspect every planned command and its repository-managed setup;
3. run milestone integration commands sequentially;
4. map results to requirements and accepted task commits;
5. record exact commands, features, environment source, and results in the
   canonical plan.

Do not run a whole-workspace all-feature gate after every task. Follow the
plan and `AGENTS.md` verification scope.

## Reopen on regression

Reopen an accepted task when milestone/final proof fails or when a later commit
changes one of its invariant-bearing owners, contracts, consumers, cleanup
paths, or public projections. A later passing test does not preserve an earlier
approval automatically.

When reopening:

1. localize the earliest accepted task responsible for the regression;
2. change its controller state from `ACCEPTED` to `REMEDIATING` and canonical
   task status from `Complete` back to `Ready`;
3. record the regression and affected downstream tasks;
4. dispatch remediation through its surface-appropriate implementation skill;
5. run focused proof and repeat the `wyrd-reviewer` pass;
6. update the task/plan and create a new fix-forward commit;
7. rerun affected downstream, milestone, and final gates.

Never amend or rewrite the original accepted-task commit. If remediation
changes architecture, public contracts, security, persistence, acceptance
outcomes, or multiple tasks, revise the plan first and obtain advisory
`wyrd-plan-reviewer` approval.

## Final verification

After all tasks are accepted:

1. run every plan-required integrated test and user journey;
2. run format, lint, typecheck, codegen, schema, docs, migration, and boundary
   gates required by the affected surfaces;
3. run `mise run pre-pr` only when the plan, `AGENTS.md`, shared CI/build/test
   infrastructure, release scope, or user requires it;
4. otherwise run the exact whole-plan closeout gate named by the plan and
   current repository task definitions;
5. inspect the full branch diff and generated-artifact provenance;
6. confirm no unrelated file, debug artifact, credential, weakened test, or
   hidden failure remains.

Run Cargo-backed commands sequentially. Inspect `mise` definitions before use
and recover repository-managed local environments rather than treating unset
variables or services as terminal.

## Final plan-wide review

Spawn one fresh `wyrd-reviewer` agent in plan-execution binding mode.
Give it:

- original and revised plan/task artifacts;
- execution baseline and complete branch diff;
- all task commits and fix-forward commits;
- the canonical requirements and acceptance criteria, with no
  implementation-owned traceability ledger treated as authority;
- exact focused, milestone, and final command evidence;
- current Wyrd authorities and routed references.

Require static review of architecture, ownership, contracts, security,
tenancy, audit, persistence, concurrency, migrations, public workflows,
language projections, generated artifacts, test strength, and complete plan
conformance.

Require the reviewer to derive conformance from the cumulative current source
and exact test assertions. Earlier task approvals, completion summaries, test
names, and passing counts are context only and cannot establish correctness.

Confirmed findings reopen the responsible task. Repeat a full plan-wide review
after remediation that affects architecture, public/persisted contracts,
security, or multiple tasks; otherwise request focused re-review.

Do not invoke `review-and-plan`. It is a separate terminal branch-against-
branch review that the user invokes after this skill.

## Completion

Validate closeout state with `scripts/validate_execution_state.py`. Mark the
plan complete only when:

- every task is `ACCEPTED` and canonically `Complete`;
- every acceptance criterion has passing implementation and proof;
- all focused, milestone, and final commands required by the plan pass;
- generated outputs are source-derived and current;
- final plan-wide review returns `APPROVE`;
- no unresolved finding, prohibited change, or unrelated diff remains;
- local commits and execution worktree are intact.

Create one final closeout commit containing final plan/task evidence and
completion state. Report:

- execution branch, worktree, baseline, and final commit;
- implemented outcome;
- each task and accepted/fix-forward commit;
- requirement and acceptance evidence;
- exact verification commands and results;
- material decisions and advisory reviews;
- final plan-wide review outcome;
- remaining external blocker or risk, normally none.
