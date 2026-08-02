# Agent Contracts

Use this reference to dispatch implementation, task review, and material plan
review agents. The root orchestrator remains the only controller and
user-facing agent.

## Contents

- [Implementation dispatch](#implementation-dispatch)
- [Task-review binding](#task-review-binding)
- [Material plan-review binding](#material-plan-review-binding)
- [Controller ownership](#controller-ownership)

## Implementation dispatch

```javascript
Agent({
  subagent_type: 'wyrd-implementor',
  name: 'impl-<task-id>',
  run_in_background: true,
  description: 'Implement <task-id>',
  prompt: <everything listed below>,
})
```

Record the `agentId` from the spawn result. It is how you resume this exact
agent later; a fresh `Agent` call does not preserve its context.

Give one fresh implementor:

- absolute dedicated-worktree path and execution branch;
- complete active task and relevant parent-plan decisions;
- applicable `AGENTS.md` paths and authority order;
- assigned execution skill for the agent to load with the `Skill` tool:
  - `$wyrd-implement` for non-UI work;
  - `$wyrd-ui` for UI work;
  - both for one cohesive cross-boundary task;
- owned outcome and expected/conditionally allowed/prohibited scope;
- contracts, invariants, acceptance criteria, required tests, features, setup,
  commands, and completion evidence;
- current diff and prior findings when remediating;
- the instruction that it owns focused verification and must continue through
  local failures. Focused means named changed tests plus directly affected
  adjacent tests; it excludes whole-crate, unfiltered integration, fuzz,
  journey, and aggregate lanes during task implementation unless the user
  expressly requests one;
- the instruction that material questions return to the orchestrator rather
  than the user.

Require this structured result:

```text
Status: COMPLETE | BLOCKED
Task: <ID and path>
Acceptance criteria: <AC -> PASS/FAIL/UNVERIFIED with source/test evidence>
Files changed: <path -> requirement>
Tests: <test -> behavior proved>
Verification: <ordered exact commands, features, and results>
Execution updates: <local/bounded discoveries>
Material decision request: <none or evidence and required decision>
Remaining work: <none or exact items>
Diff base: <last accepted commit>
```

`BLOCKED` does not end the task. It transfers a decision request to the
orchestrator.

`INCOMPLETE`, progress-only, or self-reported turn/context-limit results are
not terminal implementation outcomes.

### Resuming the same implementor

```javascript
SendMessage({
  to: '<agentId from the spawn result>',
  summary: 'remediate <task-id>: <5-10 words>',
  message: <the binding correction, findings, and failing diff>,
})
```

This wakes the idle agent with its context intact. Do not restate the task, the
plan, or work it already completed — it still has all of that. Send only what
is new.

Everything the agent needs must be inside `message`; your plain-text output is
not visible to it.

Do not replace an agent merely because it reported a turn limit, a context
limit, or partial progress. Those are agent assertions, not platform facts.
Replacement is ladder step 3 in `model-routing.md`, not a first response.

## Task-review binding

After an implementation result reports `COMPLETE`:

```javascript
Agent({
  subagent_type: 'wyrd-reviewer',
  name: 'review-<task-id>',
  description: 'Review <task-id>',
  prompt: <everything listed below, including the computed delta>,
})
```

Provide:

- mode: `plan-execution binding`;
- active task and parent plan;
- last accepted commit as the task diff base;
- complete tracked and untracked task delta;
- implementation report and exact verification evidence;
- routed architecture authorities;
- instruction to perform static analysis only.

The reviewer must not run tests, builds, lints, formatters, generators,
migrations, services, repository gates, or new verification. It audits source,
tests, contracts, consumers, and recorded evidence.

The reviewer derives scope from the canonical task before reading the
implementor report. The report is an untrusted locator, not proof. Require the
reviewer to inspect cumulative current owners and consumers and explain both
how each invariant is enforced and which exact assertion fails on regression.
Test names, passing counts, task status, and prior approvals are insufficient.

Require:

```text
Verdict: APPROVE | RESUME_IMPLEMENTATION |
         ORCHESTRATOR_DECISION_REQUIRED | REVIEW_BLOCKED
Task: <ID and path>
Reviewed target: <base commit and working-tree delta>
Conformance: <requirement/AC -> path:line, source-derived enforcement, regression assertion, evidence, result>
Findings: <ID, classification, source evidence, impact, required outcome>
Evidence audit: <sufficient, stale, missing, or weaker proof>
Inspected surfaces: <owners, consumers, contracts, tests, references>
Static limits: <none or material uncertainty>
```

The reviewer never writes an artifact, modifies plan/task/source, assigns task
status, invokes another skill, or communicates with the user in this mode.

### Auditing the verdict

Count the conformance rows and count how many carry a concrete `path:line` into
the delta plus a source-derived enforcement and regression-assertion
explanation. A row backed only by a test name, passing count, task status,
requirement restatement, or the implementor's report is uncited.

Feed both counts to the acceptance gate:

```bash
python3 <skill-dir>/scripts/validate_execution_state.py accept \
  ... --matrix-rows <total> --cited-rows <cited>
```

It fails the acceptance when the counts differ, which makes the audit
mechanical rather than a judgment call. Re-review rather than accepting a
partially cited `APPROVE`.

## Material plan-review binding

When the orchestrator revises product behavior, security, public/persisted
contracts, migration, dependency/feature policy, ownership, acceptance
outcomes, or task structure:

```javascript
Agent({
  subagent_type: 'wyrd-plan-reviewer',
  name: 'plan-review-<revision>',
  description: 'Advisory review of <revision>',
  prompt: <everything listed below>,
})
```

Provide:

- mode: `plan-execution advisory`;
- original intended outcome;
- revised canonical plan and affected tasks;
- revision decision record and repository evidence;
- affected architecture authorities and downstream consumers;
- instruction to perform static analysis and cold rehearsal only.

Require:

```text
Verdict: ADVISORY_APPROVE | ADVISORY_REVISE
Architecture axis: PASS | FAIL
Executability axis: PASS | FAIL
Findings: <Critical/Major root causes and required edits>
Validated effects: <contracts, owners, tasks, tests, closeout>
Static limits: <none or material uncertainty>
```

The advisory reviewer never edits, writes a review artifact, runs project
verification, invokes planning, or stops the user workflow. The orchestrator
resolves findings, revises, and repeats until `ADVISORY_APPROVE`.

## Controller ownership

Only the root orchestrator may:

- escalate a task up the ladder or replace an agent;
- update requirements, decisions, tasks, status, or architecture authority;
- accept a task;
- create checkpoint or fix-forward commits;
- run milestone or integrated verification;
- reopen an accepted task;
- declare plan completion or external blockage;
- communicate the final outcome to the user.
