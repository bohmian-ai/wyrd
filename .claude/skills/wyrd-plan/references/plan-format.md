# Canonical Implementation Plan

Each plan directory contains `plan.md` and `tasks/<NN>-<slug>.md` packets.
The validator enforces the metadata and heading order below. It does not impose
skills, repositories, models, execution workflow, or implementation policy.

## Metadata

```markdown
Status: Draft | Review Required | Approved
Repository origin: <host>/<owner>/<repository>
Repository revision: <7-40 lowercase hex commit>
REPO_ROOT: $REPO_ROOT
PLAN_PATH: $PLAN_PATH
Created: YYYY-MM-DD
Last updated: YYYY-MM-DD
Plan version: 1
Evidence snapshot: <evidence>
Review: not required | required | reviews/<review>.md
```

`Repository origin` and `Repository revision` identify the source checkout.
`REPO_ROOT` and `PLAN_PATH` are runtime inputs supplied by the caller.

## Required headings

```text
Objective
Current state and evidence
Requirements
Non-goals
Constraints
Architecture and design decisions
Domain and data contracts
Interfaces and function contracts
Control flow and pseudocode
Failure and edge-case matrix
Milestones
Task inventory
Global acceptance criteria
Verification strategy
Closeout verification
Risks, migration, and rollout
Execution handoff
```

## Compact example

````markdown
# Example outcome

Status: Approved
Repository origin: github.com/example/project
Repository revision: 0123456789abcdef0123456789abcdef01234567
REPO_ROOT: $REPO_ROOT
PLAN_PATH: $PLAN_PATH
Created: 2026-01-01
Last updated: 2026-01-01
Plan version: 1
Evidence snapshot: inspected current behavior
Review: not required

## Objective

Deliver the stated outcome.

## Current state and evidence

Current behavior is recorded here.

## Requirements

- R1. The observable result.

## Non-goals

No adjacent work.

## Constraints

Keep scope bounded.

## Architecture and design decisions

- D1. Keep the established owner.

## Domain and data contracts

No contract change.

## Interfaces and function contracts

No interface change.

## Control flow and pseudocode

Use the existing flow.

## Failure and edge-case matrix

Reject invalid input.

## Milestones

M1 completes the outcome.

## Task inventory

| Task | Packet |
|---|---|
| T1 | `tasks/01-example.md` |

tasks/01-example.md

## Global acceptance criteria

- AC1. The outcome is observable.

## Verification strategy

Run focused verification.

## Closeout verification

Inspect the final diff.

## Risks, migration, and rollout

No rollout change.

## Execution handoff

Use the task packet.
````
