# Verification Rubric

Use this reference to score whether an implementation executed its task's
intent. It defines the shared traceability-matrix contract for every Wyrd
review surface: standalone `$wyrd-review`, plan-execution binding under
`$wyrd-implement-plan`, and terminal integration-binding under
`$review-and-plan`.

Verification answers one question per acceptance criterion: *did the delta
execute this intent, and is there source-and-test evidence that proves it?*
The rubric turns that judgment into a fixed set of axes so a reviewer, an
orchestrator, or a human can reach the same pass/fail from the same matrix.

## The five axes

Score every in-scope requirement and acceptance criterion on all five axes.
Each axis is an independent `PASS`/`FAIL`; a row passes an axis only when it
clears that axis's floor from inspected source, not from narrative.

| Axis | Floor to pass |
|---|---|
| **Intent** | The delta actually addresses this acceptance criterion — the changed code covers the required behavior, not merely an adjacent area. |
| **Enforcement** | A concrete `path:line` in the reviewed delta enforces the invariant, with a source-derived explanation of *how* it is enforced. |
| **Regression** | The exact test assertion that fails if this behavior regresses is named, at its own `path:line`. |
| **Evidence** | The proof is real: a recorded command actually ran and selected that assertion, or the behavior was observed. Asserted-only, test-name-only, or passing-count-only rows fail this axis. |
| **Negative-case** | The acceptance criterion's negative or edge flow is covered by an assertion, **or** the row records a concrete reason the criterion has no distinct negative flow (per AGENTS.md §11, a missing negative flow is a coverage gap, not a nit). |

The axes are orthogonal on purpose. Intent proves the code is in the right
place; Enforcement proves it enforces the invariant; Regression proves a test
pins it; Evidence proves the test actually ran; Negative-case proves the
failure path is not silently uncovered. A row can clear one and fail another,
and each failure is a distinct kind of gap.

## The conjunctive rule

Verification is conjunctive, never additive. A criterion is verified only when
it passes **all five** axes. A task or plan is verified only when **every**
in-scope criterion is verified.

Do not average axes, weight them, or emit a single 0–100 score. Seven of eight
criteria proven is not "88% done" — it is not done. A blended number lets a
strong row mask a fatal gap, which is exactly the shallow `APPROVE` this rubric
exists to prevent. The only meaningful numbers are per-axis coverage counts,
e.g. `8/8 enforced, 8/8 regression-asserted, 0 asserted-only`.

`APPROVE` is permitted only when every axis passes for every criterion. Any
axis `FAIL` on any row routes the row to `RESUME_IMPLEMENTATION` (bounded
source/test/evidence work) or `ROOT_DECISION_REQUIRED` / `REPLAN_REQUIRED` (a
material gap), per the verdict rules in the review skill.

## Filling the matrix

Each traceability-matrix row carries, in addition to the requirement/AC
identifier and the overall `PASS`/`FAIL`/`UNVERIFIED` result:

```text
Intent: PASS/FAIL — <what in the delta covers this criterion>
Enforcement: PASS/FAIL — <path:line> enforces <invariant> because <how>
Regression: PASS/FAIL — <path:line> asserts <behavior>; fails if <regression>
Evidence: PASS/FAIL — <command that ran and selected the assertion, or observation>
Negative-case: PASS/FAIL — <negative assertion path:line, or the reason none applies>
```

These fields are what a rigorous review already produces; the rubric only fixes
their shape so the assessment is reproducible and, where an orchestrator runs a
gate, mechanically countable.
