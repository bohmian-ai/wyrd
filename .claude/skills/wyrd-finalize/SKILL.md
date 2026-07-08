---
name: wyrd-finalize
description: Final post-review gate for the Wyrd build pipeline. Use after wyrd-spec, wyrd-plan, wyrd-tasks, wyrd-implement, and review-and-plan have run for a feature, or when the user says /finalize, final check, close out, verify the workflow, prove spec alignment, or ensure final gates and user/agent journey tests are complete. Audits pipeline artifacts, validates implementation alignment with spec/plan/tasks/review findings, requires all final gates to pass, and blocks finalization when user or agent experience journey coverage is missing.
---

# Wyrd Finalize

Final stage of the pipeline: **idea -> spec -> plan -> tasks -> implementation -> test -> review -> finalize**.

This skill is a blocking closeout gate. It does not implement new feature scope.
It verifies that the whole workflow actually ran, confirmed review findings were
addressed, the code still matches the approved specification, and the feature is
covered by practical human and agent journey tests.

## Inputs

Accept these inputs from the user when provided; otherwise discover them from the
repo:

- Feature directory: `.dev/plan/<feature>/`
- Review directory: `.dev/review/<review-id>/`
- Target branch and base branch, when the review was not `HEAD` vs `main`

If multiple plausible feature or review directories exist, use git history,
`tasks.yaml`, and review `setup.log` to pick the matching pair. Ask only if two
matches are equally plausible and finalization would otherwise target the wrong
feature.

## Source Of Truth

Load these before judging alignment:

1. `AGENTS.md`
2. `architecture/wyrd-design.md`
3. `.claude/references/doctrine/positioning-and-vocabulary.md`
4. `.claude/references/doctrine/architecture-constraints.md`
5. `.claude/references/review/agent-first-review.md`
6. `.claude/references/rust-python/testing-workflows.md`
7. Feature artifacts: `spec.md`, `plan.md`, `tasks.yaml`, and `tasks/`
8. Review artifacts: `summary.md`, `validation.md`, and `implementation-plan.md`

If `docs/src/content/docs/concepts/doctrine.svxx` exists, read it too. If it
does not exist, treat the `.claude/references/doctrine/*` files as the doctrine
slice for this gate.

Use CodeGraph for source alignment checks. Name the symbols and seams from
`tasks.yaml` and the touched crates; do not inspect source through a grep/read
loop unless CodeGraph cannot answer a specific non-code artifact question.

## Process

1. **Confirm pipeline evidence.**
   - `spec.md` exists and records objective, success criteria, scope, contracts,
     cross-cutting decisions, boundaries, and user/agent journey expectations.
   - `plan.md` exists and is a dependency-ordered commit DAG.
   - `tasks.yaml` exists, has every node from `plan.md`, and every node is
     `done`. No `pending`, `blocked`, `needs_escalation`, or missing status.
   - `tasks/NN-*.md` exists for every task node and names seams with invariants.
   - `review-and-plan` reached terminal artifacts:
     `.dev/review/<id>/summary.md`, `validation.md`, and
     `implementation-plan.md`.

2. **Check review closure.**
   - Read `validation.md` and `implementation-plan.md`.
   - If confirmed findings, blocking investigations, or verification-required
     checks remain unresolved, block finalization.
   - If fixes were made after the review, require the relevant review or
     targeted reviewer pass to be rerun on the final diff. Do not finalize from
     stale review evidence.

3. **Audit spec/plan/task alignment.**
   - Compare implemented diff against `spec.md` success criteria and contracts.
   - Verify each `plan.md` commit outcome exists in the implementation or was
     explicitly removed by an accepted follow-up decision.
   - Verify each `tasks.yaml` seam/invariant still holds in source with
     CodeGraph and tests.
   - Reject drift from Wyrd doctrine: stale card kinds, legacy route aliases,
     PyO3 in `wyrd-spec`, client-tier persistence dependencies, missing tenant
     isolation, missing audit context, or generated contract drift.

4. **Audit journey coverage.**
   - Require at least one test or documented executable check for the primary
     human workflow: CLI, UI, Python SDK, HTTP, or docs flow, depending on the
     feature.
   - Require at least one test or documented executable check for the primary
     agent workflow: MCP, CLI/headless, generated schema/error contract, task
     harness, or machine-readable docs path, depending on the feature.
   - Journey tests must verify usability, not only isolated units. Prefer
     repo-local integration tests, contract tests, CLI command tests, UI
     interaction tests, Python import/use tests, or MCP tool tests.
   - If a journey cannot be automated yet, require a concrete recorded manual
     transcript with commands, inputs, outputs, and reason automation is not
     practical. Treat this as a temporary exception, not a pass pattern.

5. **Run mandatory gates.**
   - Run the feature-level `final_gate` from `tasks.yaml` exactly as written.
   - Run `mise run pre-pr`.
   - Run `mise run docs:check` when `docs/` changed.
   - Run `mise run codegen:check` when contracts, schemas, stubs, OpenAPI, API
     payloads, public errors, Python-visible surfaces, MCP, CLI, or generated
     docs changed.
   - Run additional targeted commands named by `tasks.yaml`, `implementation-
     plan.md`, or the touched surface.

All mandatory gates must pass. Do not accept "probably fine", skipped gates,
partial output, stale logs, or "will run in CI" as finalization evidence.

## Blocking Rules

Block finalization when any of these are true:

- A required pipeline artifact is missing or stale.
- `tasks.yaml` is incomplete or does not match the plan DAG.
- `review-and-plan` did not produce `summary.md`, `validation.md`, and
  `implementation-plan.md`.
- Confirmed review findings or required verification checks remain unresolved.
- Implementation behavior contradicts `spec.md`, `plan.md`, Wyrd doctrine, or
  task seam invariants.
- Human or agent journey coverage is missing or only unit-level.
- Any mandatory gate fails, is skipped, times out without diagnosis, or cannot
  be run in the current environment.

If blocked, do not soften the result. Report the blocker list and the exact work
needed before finalization can be attempted again.

## Output

Write `.dev/plan/<feature>/finalize.md` with:

```markdown
# Finalize: <Feature>

## Result
PASS | BLOCKED

## Pipeline Evidence
- Spec:
- Plan:
- Tasks:
- Implementation:
- Review:

## Alignment
- Spec success criteria:
- Contract and doctrine alignment:
- Task seam invariants:

## Journey Coverage
- Human journey:
- Agent journey:

## Gates
| Command | Result | Notes |
|---|---|---|

## Remaining Blockers
- None, or the exact blockers preventing finalization.
```

Final response to the user should state `PASS` or `BLOCKED`, link the
`finalize.md` file, list gate results, and name any remaining blockers.

## Non-Negotiables

- No final pass without `mise run pre-pr` passing on the final assembled branch.
- No final pass with unaddressed confirmed review findings.
- No final pass without human and agent journey coverage.
- No final pass from stale review or test evidence after code changes.
- No new feature scope during finalization; open a follow-up task or return to
  the relevant pipeline stage when scope is missing.
