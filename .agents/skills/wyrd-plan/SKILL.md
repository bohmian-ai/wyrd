---
name: wyrd-plan
description: Turn an approved Wyrd specification into decision-complete test-driven tasks, reconcile it with partial implementation, or plan bounded remediation from validated review findings. Do not redesign approved behavior.
---

# Wyrd Plan

Own implementation architecture below an approved behavioral specification.
Use `DECOMPOSE` for initial tasks, `REMEDIATE` for validated review findings,
and `RECONCILE` when an approved revision must replace or close a frozen,
partially implemented task candidate. Produce as many tasks as cohesive
ownership and real dependencies require.

## Require the right authority

For `DECOMPOSE`, require an explicitly approved spec revision. For `REMEDIATE`
from `$wyrd-task-review`, require that spec, the original task, the cumulative
reviewed task candidate, and validated implementation finding IDs. For
`REMEDIATE` from `$wyrd-change-review`, require that spec, the integrated base
and target, the complete affected task set, and validated implementation
finding IDs. Missing or incomplete review evidence is not an implementation
finding and returns directly to `$wyrd-task-review`.
For `RECONCILE`, require the approved revision, original task, frozen committed
and uncommitted candidate, and a current-state amendment that classifies what is
retained, deleted, invalidated, and unfinished. Partial code is evidence, not
authority. `RECONCILE` produces successor tasks without pretending the mutable
candidate has completed task review.

If the spec is draft, materially ambiguous, or contradicted by repository
authority, stop with `SPEC_REVISION_REQUIRED`. A finding is evidence of a
possible defect, not new product authority. Remediation that changes required
behavior, an invariant, acceptance, or a material constraint also returns to
`$wyrd-spec`.

Read `AGENTS.md`, [agent rules](../../../architecture/agent-rules.md),
[spec-driven development](../../../architecture/references/languages/spec-driven-development.md),
[implementation execution](../../../architecture/references/languages/implementation-execution.md),
and [testing workflows](../../../architecture/references/languages/testing-workflows.md).
Start at [the reference router](../../../architecture/references/README.md).
Read [Wyrd design](../../../architecture/wyrd-design.md),
[Wyrd doctrine](../../../architecture/wyrd-doctrine.mdx),
[Bifrost design](../../../architecture/bifrost-design.md), security, operations,
and focused references whenever the task surface makes them applicable.

Follow CodeGraph instructions. Inspect the exact owners, consumers, tests,
manifests, features, and `mise.toml` commands that constrain decomposition.
Record materially relevant authority links in each task; never substitute the
task for the underlying authority.

## Decompose by behavior and ownership

Each task owns one cohesive observable outcome and its complete test and
consumer closure. Split when work has a real dependency, different behavioral
owner, independently reviewable contract, or materially distinct evidence.
Keep a shared mutable seam together. Do not create tasks to match agent count,
files, phases, or arbitrary size targets.

Resolve plan-level implementation architecture from repository patterns and
keep it in the task, not the spec. This includes concrete owning
structs/modules, durable identities and schema, state transitions,
synchronization and cross-process ordering, transaction/atomicity boundaries,
failure/shutdown/crash recovery, dependency/API use, consumer wiring, and test
topology. The implementer retains only local coding choices that cannot alter
those decisions.

Use as many tasks as real ownership and dependency boundaries require. A human
request for one named change, phase, or closeout does not require one task file.
Do not collapse independently reviewable Oracle, Forge, Scribe, contract,
telemetry, or qualification outcomes merely because they share a parent task.
Identify direct dependencies only when a successor needs an integrated
predecessor contract or behavior.

Collectively map every required `REQ-*`, `INV-*`, and `AC-*` obligation to at
least one task and credible evidence. User- or agent-facing behavior includes
the real client-to-server journey coverage required by `AGENTS.md`.

## Co-design tests and production behavior

For every required behavior, design one ordered implementation scenario that
binds its proof to the production change:

1. **Behavior** — name the observable outcome and mapped `REQ-*`, `INV-*`, and
   `AC-*` obligations.
2. **RED** — name the test, correct test tier and target, real dependencies or
   deterministic gates, decisive assertions, expected failure, and exact
   repository-native command.
3. **GREEN** — name the concrete production owner and repository location, then
   fix the state, API, control flow, synchronization, ordering, atomicity,
   lifecycle, recovery, dependency, migration, and consumer changes required
   to make that test pass wherever those dimensions are material.
4. **REFACTOR** — state only the ownership, invariant, and consumer constraints
   that must remain true while improving the green implementation.

The RED proof must be sensitive to the selected production design: it must fail
when a material ownership, state, ordering, lifecycle, recovery, or consumer
decision is implemented incorrectly. Do not describe tests separately from the
production mechanism they prove, design the complete implementation before its
tests, or batch all RED tests before implementation.

When an obligation cannot meaningfully execute, bind the equivalent static or
generated proof to its production source change instead of manufacturing a RED
test.

Include only dimensions material to the scenario. Do not require a standalone
design ledger, matrix, pseudocode, or rejected-alternative inventory. Record a
rejected alternative only when it explains a material selected decision.

Inspect dependency capabilities during planning. Do not send an implementer to
discover whether the selected API can support the required durability or
recovery protocol. If repository evidence cannot resolve a private design,
return `PLAN_BLOCKED` with the exact missing capability. If resolving it would
change approved behavior or a material constraint, return
`SPEC_REVISION_REQUIRED`.

Apply this decision-completeness test: if two implementors could follow the task
and choose materially different durability, synchronization, ownership,
recovery, schema, dependency, migration, or test-topology designs, the task is
not ready to write.

Do not delegate a material choice through phrases such as “use an ordering
boundary,” “use an authoritative registry,” “or equivalent,” “choose the
appropriate owner,” “implement the smallest exact protocol,” or “if the API
cannot support this, stop.” Such language may summarize a concrete decision
already fixed in the scenario; it cannot replace one.

After designing the scenarios, verify cross-scenario closure: every production
owner, direct consumer, lifecycle route, generated surface, and required
evidence tier has one cohesive task owner. Add a scenario or split the task when
that closure is missing.

For every specifically named test, include its exact canonical
repository-native command. Use `mise run` when the repository task owns the
required environment, profile, ignored-test policy, or concurrency settings.
Use direct `mise exec -- cargo nextest run` only for an appropriate narrow Rust
test after confirming the package, target, features, and full name from source
and, when needed, `mise exec -- cargo nextest list`. For example:

```bash
mise exec -- cargo nextest run --locked -p <crate> --lib \
  -E 'test(=module::tests::test_name)'
```

Use `mise run <task>` for crate-, module-, family-, environment-, integration-,
journey-, codegen-, or aggregate-level coverage. Inspect the task body before
using it. A named Postgres, emulator, or server test includes the owning setup
wrapper or narrow environment-owning `mise` task. Never append flags that
override a canonical lane. Do not use positional test filters or invent
selectors.
Named Python and TypeScript tests likewise include the exact pytest or package
runner, path, selector, and required setup.

## Write tasks

Write tasks under `changes/active/<slug>/tasks/` on the active change branch.
Task branches inherit the complete packet from the approved change branch and
merge results back. Do not encode branch scheduling, worktrees, leases, or
controller state in task artifacts.

Use proportionate Markdown containing:

1. task ID, title, kind, status, approved spec ID/revision, dependencies, and
   mapped spec obligations;
2. outcome and user/operator value;
3. owner, scope, non-goals, and affected consumers;
4. ordered implementation scenarios that co-design each behavior's RED proof,
   GREEN production decisions, and REFACTOR constraints;
5. material cross-scenario decisions, invariants, rationale, and authority
   links that are not local to one scenario;
6. exact canonical commands for named tests;
7. broader `mise run` verification and required journeys/boundary checks;
8. completion evidence; and
9. material stop conditions.

For remediation record `remediates` validated finding IDs. Record `parent_task`
when one existing task owns the finding. When an integrated finding spans tasks,
leave `parent_task` empty and name the affected tasks in scope and consumer
closure rather than inventing a parent. For reconciliation, record the parent
task, frozen candidate identity, current-state amendment, and exact
retained/deleted/invalidated obligations.
Create the minimum cohesive task set. Do not overwrite the original task or
hide its failed review or partial-implementation history. A successor may cite
the original task as authority, but may not delegate work through an opaque
instruction to “complete the remaining original task.”

## Readiness handoff

Before handoff, trace every scenario from its observable behavior and decisive
RED proof to the concrete GREEN production change. Verify complete spec
coverage, real dependencies, exact named-test commands, broader verification,
consumer closure, and cross-scenario closure. Explicitly ask what decisions an
implementer would still need to make; any plan-level answer requires further
planning.

Return `TASKS_PROPOSED`, `PLAN_BLOCKED`, or `SPEC_REVISION_REQUIRED`, followed
only by task paths and any decision or blocker requiring attention. Do not
repeat task contents in chat. Recommend `$wyrd-task-readiness` for an
independent pre-implementation review. Planning does not implement, approve a
task candidate, merge, revise the spec, or issue its own readiness verdict.
