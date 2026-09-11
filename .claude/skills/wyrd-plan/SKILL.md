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

## Write tasks

Write tasks under `changes/active/<slug>/tasks/`. Use proportionate Markdown:

```markdown
## Objective

The observable outcome and mapped spec obligations.

## Constraints

Required boundaries, invariants, and explicit non-goals.

## Relevant Surface

Likely owners, consumers, contracts, and systems involved. Paths are guidance,
not a private implementation allowlist.

## Approach

Three to seven high-level implementation steps.

## Acceptance Criteria

Concrete success, failure, edge, and regression behavior.

## Verification

The focused and broader repository-native checks that can prove completion.
```

Add compact metadata for task ID, approved spec revision, mapped obligations,
and real dependencies. Include exact commands only for an existing specifically
named test or a repository-owned verification lane; the implementer may choose
the smallest suitable new test and fixture structure.

Do not specify helper functions, private methods, variable names, exact loops,
local control flow, private module structure, test fixture structure, or exact
code shape. Do not create subtasks for obvious mechanical work. A reversible
choice that tests can validate belongs to `$wyrd-implement`.

Before handoff, confirm that every acceptance criterion has an owner and a
credible proof, every non-goal remains excluded, dependencies are real, and no
expensive-to-reverse decision is unresolved. Planning does not implement,
approve, merge, or issue a separate readiness verdict.

Return `TASKS_PROPOSED`, `PLAN_BLOCKED`, or `SPEC_REVISION_REQUIRED`, followed
only by task paths and any material blocker.
