# Spec-Driven Development

Wyrd plans decisions that are expensive to reverse and proves everything else
after implementation. This reference governs specifications, tasks,
implementation, review, remediation, and final completion without replacing
`AGENTS.md` or architecture authority.

> Spec = decision complete. Task = outcome complete. Implementation =
> intentionally not complete.

## Authority

Apply authority in this order:

1. current user instructions and explicitly approved outcomes;
2. `AGENTS.md`, `architecture/agent-rules.md`, and applicable architecture;
3. the approved specification revision;
4. the original implementation or remediation task;
5. tests expressing its acceptance criteria; and
6. implementation and review evidence.

Code and tests never silently redefine approved behavior. A required semantic
change returns to `$wyrd-spec` and explicit human approval.

## Active change packet

Keep the tracked active packet with the implementation:

```text
changes/active/<slug>/
|-- spec.md
|-- tasks/
|   `-- TASK-001-<name>.md
`-- review/
    `-- <review-name>/
        |-- verdict.md
        `-- TASK-001-R1-<name>.md  # only when remediation is required
```

Branches, worktrees, scheduling, commits, and merging remain caller-owned
mechanics. No workflow step force-stages files. Keep the active packet through
final review; `$wyrd-complete` condenses it to
`changes/completed/<year>/<slug>.md` after final `PASS`.

## Specification contract

The specification fixes intent, observable behavior, constraints, non-goals,
invariants, and decisions expensive to reverse. These include public and
persisted contracts, architectural ownership, cross-service behavior,
concurrency guarantees, security assumptions, compatibility, and persistent
data formats.

It does not prescribe helpers, private methods, local module structure,
variables, loops, local control flow, already-approved dependency APIs, test
fixture structure, or other reversible decisions. Include a mechanism only when
the user or architecture makes it part of the contract.

Use proportionate sections for objective, requirements, constraints and
non-goals, expensive-to-reverse decisions, acceptance criteria, open material
decisions, and revision history. Stable `REQ-*`, `INV-*`, and `AC-*` IDs provide
traceability when useful. Only explicit human approval changes `draft` to
`approved`; a material revision returns it to `draft`.

## Task contract

`$wyrd-plan` derives the minimum cohesive task set from an approved spec. Each
task defines one small observable outcome and contains only:

```markdown
## Objective
## Constraints
## Relevant Surface
## Approach
## Acceptance Criteria
## Verification
```

The approach has three to seven high-level steps. Metadata records the task ID,
approved spec revision, mapped obligations, and real dependencies. Relevant
paths are guidance, not an implementation allowlist.

Do not specify helper functions, private signatures, exact code structure,
local control flow, test fixture structure, or mechanical subtasks. Stop
planning when implementation can begin without deciding product behavior,
public APIs, architecture, security, compatibility, cross-service or
concurrency semantics, or persistent data.

No separate readiness review is required. The planner checks that acceptance
criteria have credible proof, non-goals are explicit, dependencies are real,
and no expensive-to-reverse decision remains open.

## Implementation and verification

`$wyrd-implement` owns reversible technical decisions and makes the smallest
change satisfying the task. It does not create another implementation plan or
add speculative requirements. For non-trivial behavior, use scenario-sized
Red-Green-Refactor cycles without requiring the planner to predict test names,
fixtures, helpers, or control flow.

Verification proves that the implementation works. Run focused tests and the
narrowest complete type, lint, build, integration, journey, codegen, and
boundary checks required by the changed surface. Record a compact matrix:

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `<criterion>` | `<source>` | `<test/check>` | `PASS | FAIL` |

Also prove that non-goals remain excluded and unrelated files did not enter the
diff. See [implementation execution](implementation-execution.md) and
[testing workflows](testing-workflows.md).

Implementation returns `IMPLEMENTED`, not task completion. A task is complete
only when its independent acceptance review returns `PASS` after confirming:

- every acceptance criterion has implementation and verification evidence;
- every non-goal remains excluded;
- no unrelated code changed;
- required checks pass; and
- the complete diff satisfies the original task.

## Independent task acceptance review

Review answers whether the repository contains exactly the requested change;
it is not an open-ended quality or redesign exercise. The reviewer must be
fresh relative to implementation: use the current context when it is already
independent, otherwise delegate to one fresh reviewer. Give the reviewer the
original spec and task, actual base-to-candidate diff, relevant repository
rules, and verification results. Implementation summaries are not evidence.

The review is adversarial: begin unconvinced and try to falsify every claimed
acceptance result through reachable behavior. Apply the Ponytail ladder to each
changed abstraction, dependency, configuration surface, compatibility path, and
speculative extension: delete first, then repository reuse, standard library or
native platform, installed dependency, and only then minimum new code. When a
smaller existing solution satisfies the complete contract, unnecessary
complexity is `DRIFT`. This never permits weakening validation, error handling,
security, accessibility, durability, or another explicit requirement.

The reviewer maps every requirement, acceptance criterion, constraint, and
non-goal to implementation and verification evidence, then classifies material
findings only as:

- `MISSING` — required behavior is absent;
- `INCORRECT` — behavior does not satisfy the requirement;
- `DRIFT` — implementation exceeds the requested scope;
- `VIOLATION` — an explicit constraint, non-goal, or repository rule is broken;
- `REGRESSION` — existing behavior changed unintentionally.

Optional improvements, preferences, speculative hardening, and unrelated debt
are not findings. A separate specialist is justified only for a changed
high-risk boundary needing that expertise.

Write the acceptance matrix and verdict to
`changes/active/<slug>/review/<review-name>/verdict.md`. Return `PASS`,
`FIX_REQUIRED`, `SPEC_REVISION_REQUIRED`, or `BLOCKED`.

## Remediation

Review does not trigger another planning cycle. For `FIX_REQUIRED`, the reviewer
writes one self-contained remediation task beside `verdict.md` in the same
review directory. It carries the original task and candidate identities,
validated findings, correction outcome, preserved behavior and non-goals,
acceptance criteria, and verification. It does not prescribe reversible private
mechanics.

A fresh agent receives the approved spec, original task, verdict, and
remediation task and executes it with `$wyrd-implement`. Re-review covers the
complete cumulative base-to-candidate range. A semantic conflict instead
returns to `$wyrd-spec`.

## Final change review and completion

After all tasks pass and are integrated, `$wyrd-change-review` independently
maps the complete specification to the integrated diff and verification,
checking cross-task seams, resolution drift, required journeys, public and
durable contracts, security and reliability boundaries, and regressions. It
uses the same five finding categories and the same remediation-task flow.

Final `PASS` automatically invokes `$wyrd-complete`. Completion writes one
compact historical record and deletes the exact active packet after validating
both paths. It does not merge, push, deploy, or grant product authorization.

## Primary grounding

- [Kent Beck, Canon TDD](https://newsletter.kentbeck.com/p/canon-tdd)
- [Martin Fowler, Test-Driven Development](https://martinfowler.com/bliki/TestDrivenDevelopment.html)
- [W3C, A Method for Writing Testable Conformance Requirements](https://www.w3.org/TR/test-methodology/)
- [GitHub Spec Kit](https://github.github.com/spec-kit/)
- [Michael Nygard, Documenting Architecture Decisions](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions)
