---
name: wyrd-plan
description: Decompose an approved Wyrd change specification into cohesive test-driven implementation tasks, or turn validated task-review findings into bounded remediation tasks. Use after spec approval or after wyrd-task-review; do not use to redesign the approved behavior.
---

# Wyrd Plan

Own implementation reasoning below an approved behavioral specification. Use
`DECOMPOSE` for initial tasks and `REMEDIATE` for validated review findings.
Produce as many tasks as cohesive ownership and real dependencies require.

## Require the right authority

For `DECOMPOSE`, require an explicitly approved spec revision. For `REMEDIATE`,
require that spec, the original task, the cumulative reviewed candidate, and
validated finding IDs from `$wyrd-task-review` or `$wyrd-change-review`.

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

Tasks may decide private implementation mechanics that the spec leaves open.
Resolve those choices from repository patterns and keep them in the task, not
the spec. Identify direct dependencies only when a successor needs an
integrated predecessor contract or behavior.

Collectively map every required `REQ-*`, `INV-*`, and `AC-*` obligation to at
least one task and credible evidence. User- or agent-facing behavior includes
the real client-to-server journey coverage required by `AGENTS.md`.

## Plan test-driven execution

Create an ordered behavioral scenario list for each task. The implementer will
execute one scenario at a time through RED, GREEN, REFACTOR, then repeat. Do not
turn the complete scenario list into a batch of failing concrete tests.

For every specifically named test, include its exact repository-native focused
command. For Rust, confirm the package, target, features, and full name from
source and, when needed, `mise exec -- cargo nextest list`. Use the pinned
toolchain, normally:

```bash
mise exec -- cargo nextest run --locked -p <crate> --lib \
  -E 'test(=module::tests::test_name)'
```

Use `mise run <task>` for crate-, module-, family-, environment-, integration-,
journey-, codegen-, or aggregate-level coverage. Inspect the task body before
using it. A named Postgres, emulator, or server test includes the owning setup
wrapper or narrow environment-owning `mise` task. Do not use positional test
filters or invent selectors.
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
4. material implementation decisions, invariants, and authority links;
5. ordered test scenarios;
6. RED, GREEN, and REFACTOR expectations;
7. exact focused commands for named tests;
8. broader `mise run` verification and required journeys/boundary checks;
9. completion evidence; and
10. material stop conditions.

For remediation also record `parent_task` and `remediates` finding IDs. Create
the minimum cohesive remediation set. Do not overwrite the original task or
hide its failed review history.

## Readiness handoff

Before handoff, verify complete spec coverage, real dependencies, credible RED
scenarios, exact named-test commands, broader verification, consumer closure,
and no hidden material decision. Recommend `$wyrd-task-readiness` for an
independent pre-implementation review. Planning does not implement, approve a
task candidate, merge, or revise the spec.
