---
name: wyrd-pipeline
description: End-to-end driver for the Wyrd build pipeline. One entry point that runs a feature through spec → plan → tasks → implement → review → finalize, enforcing the Gate 1 and Gate 2 dynamic review loops, and driving autonomously — stopping only for spec-interview clarification, a product decision only the user can make, or a hard blocker. Driven from the main loop end to end: the front half (spec → Gate 1 → plan → Gate 2) is interactive; the back half (tasks → implement → review → finalize) is autonomous, with parallelism delegated to per-wave wave.js workflows inside wyrd-implement. Use when the user says /wyrd-pipeline, /pipeline, "build this feature end to end", or hands over a feature idea and wants it carried through the whole pipeline without babysitting each stage. Does not re-implement any stage; it invokes wyrd-spec, wyrd-plan, wyrd-architecture-reviewer, wyrd-tasks, wyrd-implement, review-and-plan, and wyrd-finalize in order.
---

# Wyrd Pipeline

The **driver** for the whole pipeline:
**idea → spec → plan → tasks → implementation → test → review → finalize**.

This skill owns *sequencing and hand-off*, not stage logic. It writes no
artifacts of its own — every artifact is produced by the stage it delegates to.

The pipeline is driven from the **main loop end to end** — that is what lets it
interview you *and* create git worktrees (a background Workflow can do neither).
It has two halves, split at the Gate 2 approval seam:

- **Front half — interactive:** `spec → Gate 1 → plan → Gate 2`. Needs you (spec
  interview, gate decisions).
- **Back half — autonomous:** `tasks → implement → review → finalize`. Does not
  need you, but still runs from the main loop, because `wyrd-implement` owns all
  git (worktree creation, integration, cleanup) which a Workflow script cannot do.
  The **only** background workflow is `wave.js` — `wyrd-implement` fires one per
  dependency wave to run that wave's commits in parallel. A blocked task surfaces
  back through `wyrd-implement` to you; nothing is faked green.

## When To Use

- `/wyrd-pipeline <feature description>`, `/pipeline`, "take this feature all the
  way", or any hand-off of an idea meant to be carried end to end.
- Not for a one-line fix with no new contract or decision — go straight to the
  relevant implementation skill.

## Interaction Contract — Hands-Off

Drive autonomously. Do **not** stop between stages for a go/no-go. Surface to the
user **only** when one of these is true:

1. **Clarification** — the `wyrd-spec` interview needs genuine product intent
   (goal, users, success criteria, a tradeoff only the user can settle). Ask one
   high-impact question at a time, per the `wyrd-spec` / `wyrd-plan-interviewer`
   discipline.
2. **Product decision** — a gate returns `reopen locked decision`, or a stage
   surfaces a choice the repo cannot answer. Get the decision, then resume the
   loop.
3. **Hard blocker** — a gate loop cannot converge (same blocking finding repeats
   and needs user input), a mandatory gate fails and cannot be fixed
   autonomously, or the user explicitly stops or changes scope.

Everything else — running each stage, running the Gate 1/Gate 2 review loops,
revising artifacts, re-reviewing, running `mise` gates, iterating to green — you
drive without asking. The reviewer's `Decision: approve` is the signal to advance
a gate; you do **not** add a separate human sign-off.

## Process

Establish the feature slug up front (from the user's description) so every stage
writes under `.dev/plan/<feature>/`. Create a TodoWrite list with one item per
stage and keep exactly one `in_progress`.

### Tier 1 — interactive (this skill)

1. **Spec.** Invoke `wyrd-spec`. Interview the user for genuine product intent;
   ground everything else from the repo. Produce `spec.md`.
2. **Gate 1.** Run the dynamic review loop
   (`.claude/references/review/dynamic-review-loop.md`): hand `spec.md` to
   `wyrd-architecture-reviewer` at **full sweep**, revise, re-review, repeat until
   `Decision: approve`. Do not advance on `approve with changes`,
   `needs redesign`, or `reopen locked decision`. Preserve the loop ledger.
3. **Plan.** Invoke `wyrd-plan`. Produce `plan.md` (commit DAG) + the
   `tasks.yaml` skeleton.
4. **Gate 2.** Run the dynamic review loop on `plan.md` until `Decision: approve`,
   same terms as Gate 1.

### Seam — Gate 2 approved, continue into the back half (still main loop)

5. **Tasks.** Invoke `wyrd-tasks`. Expand the approved DAG into thin per-commit
   contracts and fill `tasks.yaml` — each node's `verify` a `mise run <task>` gate.
6. **Implement.** Invoke `wyrd-implement`. It computes dependency waves, creates a
   correctly-based worktree per node, fires `wave.js` per wave to run that wave's
   commits in parallel, integrates each wave, and verifies once per wave. Let it
   own all git and the wave loop; do not run commits inline. A task it cannot get
   to green (escalated, then `blocked`) is a **hard blocker** — surface it to the
   user with the evidence; never mark it `done`.
7. **Review.** Invoke `review-and-plan` on the whole feature diff vs. its parent;
   drive it to its terminal artifact (`implementation-plan.md`, preceded by
   `summary.md` and `validation.md`). Surface **only** confirmed findings from
   `validation.md`. Fix confirmed findings, then re-run the relevant review on the
   new diff (no stale evidence) before proceeding.
8. **Finalize.** Invoke `wyrd-finalize`: pipeline-evidence, spec/plan/task
   alignment, human + agent journey coverage, and mandatory gates including
   `mise run pre-pr`. Report `PASS` or `BLOCKED`; a `BLOCKED` is a hard blocker,
   not a soft pass.

Advance each stage's todo to `completed` only when its exit condition is met
(gate stages: `Decision: approve`; implement: all `tasks.yaml` nodes `done` and
gates green; review: `implementation-plan.md` exists and confirmed findings
closed; finalize: `PASS`). Watch `wyrd-implement`'s per-wave workflows in
`/workflows`.

## Stop And Resume

- **Tier 1 pause** (spec interview, gate reopen): record the current stage, the
  last `Decision` or gate state, and what input is needed — the loop ledger holds
  the iteration evidence. Resume by re-entering the paused stage with the user's
  input; do not restart a stage whose exit condition already held.
- **Back-half blocker** (a `blocked` task, a confirmed finding needing a product
  call, a failing finalize gate): surface it with evidence, get the decision, then
  resume. Resume is **status-driven, not a replay**: `wyrd-implement` reads
  `tasks.yaml` status, so a re-run skips `done` nodes and picks up at the first
  incomplete wave. On-disk state (`tasks.yaml`, worktrees, the impl branch, review
  ledgers) is the source of truth — nothing is lost across the pause.

## Non-Negotiables

- **Never skip a gate.** Gate 1 and Gate 2 each run the dynamic review loop to
  `Decision: approve` before the next stage starts. One review plus one correction
  is never a pass.
- **Never fake approval.** If a gate cannot reach `approve` without user input,
  surface the blocker — do not report the gate as passed.
- **Never fake a `pass`.** The pipeline is done only when `wyrd-finalize` reports
  `PASS`. A `blocked` task or `BLOCKED` finalize is an escalation to relay, never a
  quiet stop or a softened success.
- **Never surface unvalidated review findings.** Only confirmed findings from
  `review-and-plan`'s `validation.md` reach the user; do not investigate or fix a
  raw reviewer finding before it is validated.
- **Own sequencing, not stage logic.** Delegate every stage to its skill;
  `wyrd-implement` owns all git and the wave loop (with `wave.js` for
  parallelism). Do not re-implement a stage here and do not run commits inline
  instead of through `wyrd-implement`.
- Honor `AGENTS.md`, `architecture/wyrd-design.md`, and core doctrine throughout;
  the stage skills enforce their own doctrine — do not override it.
