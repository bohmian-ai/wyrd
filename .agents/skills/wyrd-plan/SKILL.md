---
name: wyrd-plan
description: Translate an approved Wyrd specification into the smallest cohesive task set without adding obligations or predicting private implementation details.
---

# Wyrd Plan

Translate the approved contract into executable work. Planning may decompose
obligations and identify necessary implementation work; it may not enlarge the
agreement. Leave reversible technical decisions to `$wyrd-implement`.

Do not use this skill for review remediation. Validated implementation findings
go directly to `$wyrd-implement`; findings that change approved behavior go to
`$wyrd-spec`.

## Establish the contract

Require an explicitly approved `changes/active/<slug>/spec.md`. Read `AGENTS.md`,
applicable directory guidance, [spec-driven development](../../../changes/spec-driven-development.md),
[implementation execution](../../../changes/implementation-execution.md), and
[testing](../../../TESTING.md). Load only architecture references applicable to
the change. Follow CodeGraph instructions and inspect the owners, consumers,
tests, manifests and `mise.toml` tasks needed to locate the work and its proof.

Read relevant design notes when present. User decisions and approved semantics
govern; proposed private mechanics remain guidance. Revalidate research only
where it matters to the task.

Do not add product guarantees, supported cases, interfaces, operational features
or acceptance obligations. A newly discovered material need returns to
`$wyrd-spec`; optional improvements stay outside the plan. Return
`SPEC_REVISION_REQUIRED` when an unresolved decision materially changes the
outcome, public or persisted contract, required ownership, or safety boundary.
Ordinary implementation uncertainty is not a spec blocker.

## Decompose only as needed

Default to one task. Split for a real dependency, distinct behavioral owner, or
independently deliverable outcome. Each task includes the necessary consumer,
contract, generated projection and test updates for its outcome. Do not split
by file, agent, phase, mechanical update or arbitrary size target.

Identify the existing owner and missing behavior. Keep routine reuse references
brief. When proposing new machinery or a changed ownership boundary, compare
the existing owner's complete workflow and explain why extending it cannot
appropriately satisfy the outcome. Reusing leaf calls inside duplicate
orchestration is insufficient. Paths and symbols guide implementation; they
are not an allowlist.

## Write bounded tasks

Write tasks under `changes/active/<slug>/tasks/` with compact metadata for task
ID, status, approved spec revision, mapped obligations and real dependencies.
Use the smallest structure that conveys:

- outcome and scope, including relevant exclusions;
- the existing owner and missing behavior;
- acceptance observations mapped once to the approved obligations;
- credible verification at the required test tier; and
- any real dependency or material blocker.

Add approach notes only where they resolve a real handoff ambiguity. Do not
require a fixed step count or empty sections. Do not repeat the spec or invent
extra cases to fill a template. Preserve the journeys, language surfaces and
safeguards required by applicable repository authority.

Describe verification as a concrete trigger and expected observation. Group
equivalent cases where useful. TDD remains an execution discipline: do not
script `RED`, `GREEN` or `REFACTOR` blocks or predict a refactoring. For static
or already-correct behavior, name the appropriate proof without manufacturing
tests or production changes.

Identify the test owner, tier and existing runner. Include exact focused
commands for existing tests named in the task. Do not invent names or selectors
for tests that do not exist; the implementer records their actual focused
commands after adding them, before RED. List only the owner lanes, boundary
checks and format/lint checks needed for the task. Integrated change review
owns full journey lanes for shipped surfaces and any required broad aggregate.

Private helpers, signatures, module layout, algorithms, control flow, test
fixtures and use of already-approved dependencies remain implementation-owned.

## Check and hand off

Confirm coverage of approved obligations without adding new ones, respect
exclusions, and check that dependencies are real. Stop elaborating when the
outcome, affected owner, material constraints and credible verification are
clear. A conceivable failure mode alone does not authorize another requirement.

Complete the shared independent comprehension pass over the spec and tasks.
Repair explanations without expanding approved semantics. Planning does not
implement, approve, merge or issue a separate readiness verdict.

Return `TASKS_PROPOSED`, `PLAN_BLOCKED`, or `SPEC_REVISION_REQUIRED`, followed by
task paths and a short summary of the work, verification, and any material
blocker or remaining comprehension-review limitation.
