---
name: wyrd-plan
description: Turn an approved Wyrd specification into a small set of maintainer-readable, outcome-complete implementation tasks without prescribing reversible implementation details.
---

# Wyrd Plan

Produce the minimum task set the maintainer can understand and an implementation
agent can execute without making a product, public API, architecture, security,
compatibility, cross-service, concurrency-semantics, or persistent-data decision.
The maintainer must understand the proposed work, invariants and proof without
reading code or the author conversation. Stop planning when both needs are met.

Do not use this skill for review remediation. Validated implementation findings
go directly to `$wyrd-implement`; a finding that changes approved behavior goes
to `$wyrd-spec`.

## Establish authority

Require an explicitly approved `changes/active/<slug>/spec.md`. Read `AGENTS.md`, the nested `AGENTS.md` files for every directory the diff touches,
[spec-driven development](../../../changes/spec-driven-development.md),
[implementation execution](../../../changes/implementation-execution.md),
and [testing](../../../TESTING.md).
Follow the shared maintainer-approval and independent-comprehension guidance in
spec-driven development.
Load only applicable architecture references. Follow CodeGraph instructions and
inspect the owners, consumers, tests, manifests, and `mise.toml` tasks needed to
identify real boundaries and credible verification.

If required behavior or an expensive-to-reverse decision remains ambiguous or
conflicts with repository authority, return `SPEC_REVISION_REQUIRED` instead of
inventing it.

## Decompose by outcome

Before writing tasks, trace each capability through its existing owners,
callers, and tests. Record source-backed reuse evidence in the task's
implementation reference: existing owner/symbol, inspected callers/tests,
missing behavior, selected extension, and justification for new machinery when
needed. Use short entries or a compact table; do not force long symbols and
several independent explanations into a wide six-column table.

Compare whole workflows, not only leaf calls. Extending an existing owner is
the default. A new graph store, traversal, loader, parser, transport, validator,
lifecycle owner, cache, or orchestration layer requires evidence that the
existing owner cannot appropriately provide the behavior. Paths without
inspected symbols and a precise gap are not reuse evidence. Ordinary private
helpers remain implementation-owned; do not create a type or file allowlist.

Each task owns one small observable outcome and its necessary consumer and test
closure. Split only for a real dependency, a distinct behavioral owner, or an
independently deliverable outcome. Do not create tasks for files, agents,
phases, mechanical updates, or arbitrary size targets.

Collectively map every required `REQ-*`, `INV-*`, and `AC-*` obligation. Preserve
the user journeys and language surfaces required by `AGENTS.md`, but do not
pre-design test fixtures or private production structure. Keep the mapping in
metadata and one acceptance/proof table; explain outcomes in ordinary prose.
Use one scenario per distinct behavior, with a small case table for equivalent
variations. Do not meet a scenario-count target by bundling unrelated promises.

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

What changes, why it matters, and one concrete before/after example where
useful. Explain the outcome without requiring the reader to follow spec IDs.

## What Changes and What Must Stay True

Scope, important invariants and the failures they prevent, material costs,
required ownership boundaries, and explicit non-goals. Define specialist terms
when first needed. Leave source inventories to the implementation reference.

## Approach

Three to seven high-level implementation steps.

## Ordered Implementation Scenarios

### Scenario 1 — <observable behavior>

**Behavior.** <Concrete trigger, success/failure/edge behavior, and the invariant
it protects. Explain the expected result for each distinct case.>

**RED.** <The smallest focused test and the expected failure that proves the
behavior is missing. Say what the test exercises and asserts. For a named test,
link to its exact focused command in Verification and Evidence.>

**GREEN.** <The minimum cohesive behavior needed to pass this scenario and the
earlier scenarios that must be rerun.>

**REFACTOR.** <The repository-native simplification permitted while all scenario
tests remain green.>

## Acceptance Criteria

One mapping from spec obligations to concrete success, failure, edge and
regression observations. Explain which proof establishes each important rule
and what result would violate it; IDs and command references are supplementary.

## Implementation Reference

Source-backed reuse evidence and likely production, consumer, contract,
generated and test changes. Combine owner and file inventories here rather
than repeat them in several sections. Paths are guidance, not an implementation
allowlist.

## Verification and Evidence

The narrowest repository-native checks that prove this task: exact
named-test commands (including the task's own journey tests by exact
selector) plus only the owner lanes, boundary checks, and format/lint checks
covering the task's write set. Do not list full user-journey suites, every
language surface, or broad aggregates such as `mise run gate` in a task; those
run once on the integrated candidate at change review. Record each command once
with a descriptive name, purpose and expected evidence; link from scenarios.

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
target, runner and setup from the current test owner; confirm existing selectors
from source. Mark a new test name as planned, and require the implementer to
confirm its actual selector after adding it, before RED. Do not invent a target
or setup from an assumed path. The implementer still owns the smallest suitable
fixture structure. Commands may live once in Verification and Evidence, with
descriptive links from the relevant scenarios; do not duplicate long shell blocks.

Do not specify helper functions, private methods, variable names, exact loops,
local control flow, private module structure, test fixture structure, or exact
code shape. Do not create subtasks for obvious mechanical work. A reversible
choice that tests can validate belongs to `$wyrd-implement`.

Before handoff, confirm that every acceptance criterion has an owner and a
credible proof, every non-goal remains excluded, dependencies are real, and no
expensive-to-reverse decision is unresolved. Recheck the reuse map against the
task: it must not require replacement machinery without the stated evidence.
Complete the shared independent comprehension pass over the spec and proposed
tasks together, repair missing explanations, and preserve approved semantics.
A task that adds or changes
executable behavior, scenarios, or logic is invalid when any scenario lacks an
explicit `Behavior`, `RED`, `GREEN`, or `REFACTOR` block; do not return
`TASKS_PROPOSED` until all four are present for every scenario. Planning does
not implement, approve, merge, or issue a separate readiness verdict.

Return `TASKS_PROPOSED`, `PLAN_BLOCKED`, or `SPEC_REVISION_REQUIRED`, followed
by task paths and a short maintainer summary: what changes, the important
invariants and costs, how the work will be verified, and any material blocker
or remaining comprehension-review limitation. Keep the status vocabulary and
approval boundaries unchanged.
