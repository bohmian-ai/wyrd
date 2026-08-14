# Executable Task Packet

Task packets are standardized assignments. The validator checks only the
metadata and heading order below; it does not prescribe skills, models,
repositories, or an execution process.

## Metadata

```markdown
Status: Planned | Ready | Complete | Blocked
Repository origin: <host>/<owner>/<repository>
Repository revision: <7-40 lowercase hex commit>
REPO_ROOT: $REPO_ROOT
PLAN_PATH: $PLAN_PATH
TASK_PATH: $TASK_PATH
Plan: ../plan.md
Milestone: M1 | None
Requirements: R1
Decisions: D1 | None
Depends on: T1 | None
```

`Repository origin` and `Repository revision` identify the source checkout.
`REPO_ROOT`, `PLAN_PATH`, and `TASK_PATH` are runtime inputs supplied by the
caller.

## Required headings

```text
Objective
Context
Required changes
Non-goals
Allowed scope
Prohibited changes
Target paths and symbols
Required types and interfaces
Implementation guidance
Control flow and pseudocode
Failure and edge cases
Acceptance criteria
Required tests
Required features
Focused verification
Commands explicitly excluded
Stop and escalate if
Completion evidence
```

## Complete example

```markdown
# T1: Example outcome

Status: Ready
Repository origin: github.com/example/project
Repository revision: 0123456789abcdef0123456789abcdef01234567
REPO_ROOT: $REPO_ROOT
PLAN_PATH: $PLAN_PATH
TASK_PATH: $TASK_PATH
Plan: ../plan.md
Milestone: M1
Requirements: R1
Decisions: D1
Depends on: None

## Objective

Deliver the bounded result.

## Context

State the relevant current behavior.

## Required changes

Implement the approved change.

## Non-goals

Do not expand scope.

## Allowed scope

Use the listed source paths.

## Prohibited changes

Do not change unrelated behavior.

## Target paths and symbols

Name the affected owner and test.

## Required types and interfaces

No interface change.

## Implementation guidance

Follow the established local pattern.

## Control flow and pseudocode

Keep the existing ordering.

## Failure and edge cases

Preserve error behavior.

## Acceptance criteria

- AC1. The result is observable.

## Required tests

Add focused coverage.

## Required features

No feature change.

## Focused verification

Run the focused test.

## Commands explicitly excluded

No broad gate is required.

## Stop and escalate if

A material requirement changes.

## Completion evidence

Record implementation and verification evidence.
```
