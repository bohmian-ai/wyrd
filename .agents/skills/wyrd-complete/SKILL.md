---
name: wyrd-complete
description: Finalize an approved Wyrd change by writing one compact completed record and removing its active specification and task packet. Automatically invoked by wyrd-change-review after APPROVE; do not use before final review approval.
---

# Wyrd Complete

Own the bounded write transition from an approved active change packet to one
durable completed record. This skill is normally invoked automatically by
`$wyrd-change-review`; developers should not need to remember a separate step.

## Require approved completion authority

Require the `APPROVE` output from the immediately preceding final change review,
including reviewed base and target commits, the complete
`changes/active/<slug>` packet, task-review closure, requirement-to-evidence
matrix, completion payload, and delivery context when available.

Read `AGENTS.md`, [agent rules](../../../architecture/agent-rules.md), and
[spec-driven development](../../../architecture/references/languages/spec-driven-development.md).
Read the active spec's materially relevant architecture links and confirm the
completion worktree's `HEAD` equals the reviewed target. Require the exact
active packet to match that target before writing. Do not reinterpret the spec
or repeat final review.

Stop with `BLOCKED` if the review did not return `APPROVE`, the reviewed target
changed, the active packet is incomplete, the completion payload lacks credible
evidence, or lasting behavior is missing from or contradicts its current
architecture authority. Missing product or architecture documentation is
remediation through `$wyrd-plan`, not cleanup work.

Derive `<slug>` from the single reviewed directory entry under
`changes/active/`, not from unchecked caller text. Accept only a lowercase
single path segment composed of letters, digits, and internal hyphens; reject
separators, traversal, empty segments, and ambiguity among multiple active
changes. Record one ISO completion date and derive `<year>` from that date.

## Write the completed record

Create `changes/completed/<year>/<slug>.md` from only the approved spec, reviewed
tasks and evidence, and the change-review completion payload. Use proportionate
Markdown containing:

1. change ID, completed date, reviewed target, and delivery reference, using
   `not supplied` rather than inventing a missing delivery reference;
2. intent and user or operator value;
3. shipped externally observable behavior;
4. lasting invariants and constraints;
5. material decisions and rationale;
6. approved spec revisions and material deviations;
7. compact requirement-to-evidence closure; and
8. links to current architecture, contracts, code owners, and tests.

The record is historical context, not a replacement for current architecture.
Do not retain task checklists, RED/GREEN mechanics, review conversation, command
transcripts, agent metadata, branch scheduling, or worktree state. Do not create
an ADR for every change; link a focused existing or newly approved decision
record only when the change contains an architecturally significant decision.

Never overwrite an existing completed record blindly. When both the completed
record and active packet exist, resume only after proving that the record
represents the same reviewed target and completion payload; otherwise return
`BLOCKED`. When the active packet is already absent and the record matches the
approved change, return an idempotent `COMPLETE` without writing.

## Retire the active packet

Validate the completed record before removing `changes/active/<slug>/`. Delete
only that exact active packet. Do not modify production code, tests, generated
artifacts, or architecture documents during completion. Preserve unrelated
working-tree changes and stop if they make the reviewed target ambiguous. The
active path must be unchanged from the reviewed target. The destination must be
absent or be the exact matching record from an interrupted retry; any other
staged, unstaged, or untracked change at either path returns `BLOCKED`.

Run `git diff --check`, inspect the complete completion diff, verify that the
completed record exists and the exact active packet does not, and confirm that
all repository-relative links resolve. Create a commit only when requested.

Return `COMPLETE` or `BLOCKED` with the completed-record path, removed active
path, reviewed target identity, verification performed, and any delivery action
still requiring human authorization. Completion never merges, pushes, deploys,
or grants product authorization.
