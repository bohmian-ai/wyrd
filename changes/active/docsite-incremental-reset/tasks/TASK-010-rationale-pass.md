---
id: TASK-010
kind: implementation
status: proposed
spec: SPEC-docsite-incremental-reset
spec_revision: 9
requirements: [REQ-011, INV-001, AC-012]
depends_on: [TASK-008]
parent_task:
remediates: []
review: manual review by the user
---

## Outcome and Value

The published guides stop feeling heavy. Setup, Quickstart, Integrate with
FastAPI, and Configure the verification runtime keep their steps, code, and
output; the reasons behind them move to the Understand pages that own them.

Example: Quickstart's paragraph on why supplied evidence is not a measured
model run becomes one sentence linking to How verification works, where the
full explanation lives.

## What Changes and What Must Stay True

- Each guide passes the docs/AGENTS.md tests: code on the first screen, every
  step with its file, command, and output, no more than about three sentences
  between code blocks.
- Moved rationale lands on an existing Understand page; nothing is deleted that
  a reader needs, and no Understand page gains a procedure.
- Embedded examples, commands, and their tests do not change. A change to an
  example is out of scope for this task.
- Published behavior claims stay true; nothing new is claimed.

## Approach

1. For each guide, mark every sentence as step, output, rationale, or detail.
2. Move rationale to the owning Understand page's "Why it works this way"
   section; move detail to reference or delete it.
3. Run the deletion test and the humanizer lint on each page.
4. Hand each before/after to the user for review.

## Proof Strategy

Prose only; no executable behavior, so TDD does not apply. Proof is the docs
gate (links, commands, build, a11y), the lint, and the user's read-through of
each before/after diff.

## Acceptance Criteria

| Obligation | Observation | Proof |
| --- | --- | --- |
| REQ-011, AC-012 | Each guide carries steps, code, and output; its rationale is on an Understand page linked once | Manual review |
| INV-001 | Site behavior and accessibility unchanged | `mise run docs:check` |

## Implementation Reference

Pages: `get-started/setup.md`, `get-started/quickstart.md`,
`connect/integrate-fastapi.md`, `operate/configure-verification-runtime.md`.
Receivers: `understand/how-verification-works.md`,
`understand/runs-observations-and-evidence.md`,
`understand/cards-versions-and-relationships.md`, and the TASK-008
Architecture page.

## Verification and Evidence

- **Prose lint:** `~/.claude/skills/humanizer/scripts/lint.sh <page>`.
- **Site gate, once before commit:** `mise run docs:check`.

## Material Stop Conditions

A guide cannot shed its rationale without losing a step the reader needs
(the step is missing from its example); a claim cannot be traced to source.

## Authority Links

- [Approved spec revision 9](../spec.md)
- [Docs writing rules](../../../../docs/AGENTS.md)
