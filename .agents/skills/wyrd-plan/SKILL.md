---
name: wyrd-plan
description: Use an approved Wyrd spec and finalized design to write cohesive tasks that carry the agreed approach through real inputs, outputs, dependencies, and observable proof without adding scope.
---

# Wyrd Plan

Translate the approved contract and agreed design into executable work. Carry
the selected approach into tasks; decompose the flow without enlarging the
agreement or designing a replacement solution. Leave unsettled reversible
coding choices to `$wyrd-implement`.

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

Read the finalized design notes and relevant supplements alongside the approved
spec. Use the agreed implementation approach, flow and rationale to write the
tasks. Explicitly settled constraints and approved semantics govern;
illustrative private mechanics remain guidance. Revalidate research only where
it matters to the task. Existing approved packets without design notes remain
usable; do not invent a design or force retroactive artifacts.

Do not add product guarantees, supported cases, interfaces, operational features
or acceptance obligations. A newly discovered material need returns to
the design conversation, with `$wyrd-spec` revision and renewed approval when
the approved contract must change; optional improvements stay outside the plan.
Return `SPEC_REVISION_REQUIRED` when an unresolved decision materially changes
the outcome, public or persisted contract, required ownership, or safety
boundary. Ordinary implementation uncertainty is not a spec blocker.

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
- the existing owner, missing behavior and implementation approach carried
  forward from the design;
- inputs and their actual sources, outputs and their consumers;
- acceptance observations mapped once to the approved obligations;
- credible verification at the required test tier; and
- any real dependency or material blocker.

Name the design sections or supplements that explain the selected approach.
Keep the task self-contained enough to execute without rediscovering the
solution. Do not require a fixed step count or empty sections. Do not repeat
the spec or invent extra cases to fill a template. Preserve the journeys,
language surfaces and safeguards required by applicable repository authority.

Each task implements an agreed part of the flow:
input and source → agreed change → output and consumer → observable proof.
An input comes from existing repository behavior or an identified predecessor
task's output. State which output a dependency supplies; do not add placeholder
tasks for capabilities that already exist or fabricate inputs to hide a gap.

Describe verification as a concrete trigger and expected observation. Group
equivalent cases where useful. TDD remains an execution discipline: do not
script `RED`, `GREEN` or `REFACTOR` blocks or predict a refactoring. For static
or already-correct behavior, name the appropriate proof without manufacturing
tests or production changes.

Proof must exercise the task's actual inputs and demonstrate its promised
outputs. At connected boundaries, establish that the downstream consumer uses
the predecessor's output through the agreed path. Preserve the end-to-end
acceptance journey; disconnected fixtures alone do not prove the delivered flow.

Identify the test owner, tier and existing runner. Include exact focused
commands for existing tests named in the task. Do not invent names or selectors
for tests that do not exist; the implementer records their actual focused
commands after adding them, before RED. List only the owner lanes, boundary
checks and format/lint checks needed for the task. Integrated change review
owns full journey lanes for shipped surfaces and any required broad aggregate.

Private helpers, signatures, module layout, algorithms, control flow, test
fixtures and use of already-approved dependencies remain implementation-owned.

## Check and hand off

Confirm coverage of approved obligations and the agreed flow without adding
new ones. Respect exclusions, check each input's source and dependency output,
and ensure the combined proof connects the delivered flow. Stop elaborating
when the outcome, affected owner, material constraints and credible verification
are clear. A conceivable failure mode alone does not authorize another requirement.

Complete the shared independent comprehension pass over the spec and tasks.
Repair explanations without expanding approved semantics. Planning does not
implement, approve, merge or issue a separate readiness verdict.

Return `TASKS_PROPOSED`, `PLAN_BLOCKED`, or `SPEC_REVISION_REQUIRED`, followed by
task paths and a short summary of the work, verification, and any material
blocker or remaining comprehension-review limitation.
