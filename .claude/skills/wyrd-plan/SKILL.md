---
name: wyrd-plan
description: Turn an approved Wyrd specification into a small set of outcome-complete implementation tasks without prescribing reversible implementation details.
---

# Wyrd Plan

Produce the minimum task set an implementation agent needs to begin without
making a product, public API, architecture, security, compatibility,
cross-service, concurrency-semantics, or persistent-data decision. Stop planning
at that point.

Do not use this skill for review remediation. Validated implementation findings
go directly to `$wyrd-implement`; a finding that changes approved behavior goes
to `$wyrd-spec`.

## Establish authority

Require an explicitly approved `changes/active/<slug>/spec.md`. Read `AGENTS.md`,
[agent rules](../../../architecture/agent-rules.md),
[spec-driven development](../../../architecture/references/languages/spec-driven-development.md),
[implementation execution](../../../architecture/references/languages/implementation-execution.md),
and [testing workflows](../../../architecture/references/languages/testing-workflows.md).
Load only applicable architecture references. Follow CodeGraph instructions and
inspect the owners, consumers, tests, manifests, and `mise.toml` tasks needed to
identify real boundaries and credible verification.

If required behavior or an expensive-to-reverse decision remains ambiguous or
conflicts with repository authority, return `SPEC_REVISION_REQUIRED` instead of
inventing it.

## Decompose by outcome

Each task owns one small observable outcome and its necessary consumer and test
closure. Split only for a real dependency, a distinct behavioral owner, or an
independently deliverable outcome. Do not create tasks for files, agents,
phases, mechanical updates, or arbitrary size targets.

Collectively map every required `REQ-*`, `INV-*`, and `AC-*` obligation. Preserve
the user journeys and language surfaces required by `AGENTS.md`, but do not
pre-design test fixtures or private production structure.

Classify each task's proof before writing it:

- A task that adds or changes executable behavior, scenarios, or logic requires
  TDD. Write each behavioral scenario in execution order using the exact
  `Behavior` / `RED` / `GREEN` / `REFACTOR` structure below.
- A task limited to documentation, generated artifacts, configuration, static
  obligations, or verification of already-correct behavior does not require a
  manufactured RED. State why TDD is not applicable and name the static,
  verification-only, regression, or no-op proof instead.
- A mixed task applies TDD only to its new or changed executable behavior.

## Write tasks

Write tasks under `changes/active/<slug>/tasks/`. Use this structure, omitting
only sections that are demonstrably inapplicable:

```markdown
## Outcome and Value

The observable outcome and mapped spec obligations.

## Owners, Scope, Consumers, and Prohibited Changes

Required boundaries, likely owners and consumers, invariants, and explicit
non-goals. Paths are guidance, not a private implementation allowlist.

## Approach

Three to seven high-level implementation steps.

## Ordered Implementation Scenarios

### Scenario 1 — <observable behavior>

**Behavior.** <Success, failure, or edge behavior and mapped obligations.>

**RED.** <The smallest focused test and the expected failure that proves the
behavior is missing. If the test is named, include its exact focused command.>

**GREEN.** <The minimum cohesive behavior needed to pass this scenario and the
earlier scenarios that must be rerun.>

**REFACTOR.** <The repository-native simplification permitted while all scenario
tests remain green.>

## Acceptance Criteria

Concrete success, failure, edge, and regression behavior.

## Expected Write Set and Consumer Closure

Likely production, consumer, contract, generated, and test surfaces. Paths are
guidance, not an implementation allowlist.

## Verification and Evidence

The focused and broader repository-native checks that can prove completion.

## Material Stop Conditions

The discoveries that require planning or specification authority rather than an
implementation decision.

## Authority Links

The approved spec and applicable repository authorities.
```

Repeat the complete scenario block for each behavioral scenario. If the task
has no new or changed executable behavior, replace `Ordered Implementation
Scenarios` with `Proof Strategy` and explain the applicable static,
verification-only, regression, or no-op proof. For a mixed task, retain the
scenario blocks and add the non-TDD proof for the remaining obligations under
`Verification and Evidence`.

Add compact metadata for task ID, approved spec revision, mapped obligations,
and real dependencies. Every specifically named test, existing or planned,
includes its exact focused repository-native command. Confirm its package,
target, and selector from the current test owner; do not invent a command from
an assumed path. The implementer still owns the smallest suitable fixture
structure.

Do not specify helper functions, private methods, variable names, exact loops,
local control flow, private module structure, test fixture structure, or exact
code shape. Do not create subtasks for obvious mechanical work. A reversible
choice that tests can validate belongs to `$wyrd-implement`.

Before handoff, confirm that every acceptance criterion has an owner and a
credible proof, every non-goal remains excluded, dependencies are real, and no
expensive-to-reverse decision is unresolved. A task that adds or changes
executable behavior, scenarios, or logic is invalid when any scenario lacks an
explicit `Behavior`, `RED`, `GREEN`, or `REFACTOR` block; do not return
`TASKS_PROPOSED` until all four are present for every scenario. Planning does
not implement, approve, merge, or issue a separate readiness verdict.

Return `TASKS_PROPOSED`, `PLAN_BLOCKED`, or `SPEC_REVISION_REQUIRED`, followed
only by task paths and any material blocker.
