---
name: wyrd-task-review
description: Independently audit one immutable cumulative Wyrd task implementation for exact acceptance, repository-standard compliance, and Ponytail minimalism, then write a verdict and any remediation task.
---

# Wyrd Task Review

Answer one question: does the resulting repository satisfy the original task
exactly? This is an acceptance audit, not an opportunity to improve, redesign,
or refactor the implementation.

Keep the reviewed source immutable. Review the complete base-to-candidate range,
not the implementation summary or only the latest fix diff. After remediation,
include the original task, prior verdict and findings, remediation task, and
cumulative candidate.

## Establish the subject

Require the repository root, unambiguous base and candidate commits, approved
spec, original task, actual diff, repository rules, and available verification
results. If the candidate changes during review, return `BLOCKED`.

Read `AGENTS.md`, [agent rules](../../../architecture/agent-rules.md),
[spec-driven development](../../../architecture/references/languages/spec-driven-development.md),
and only the architecture and testing references applicable to the task. Follow
CodeGraph instructions. Inspect the diff and enough surrounding owners,
callers, consumers, negative paths, tests, manifests, and generated surfaces to
judge the task.

The reviewer must be fresh relative to implementation. If the current context
implemented the change, delegate this audit to one fresh reviewer; otherwise
review directly. The reviewer receives the original specification and task,
actual diff, relevant repository rules, and verification results—not the
implementation agent's completion summary or an intended verdict.

Always delegate repository-standard compliance to a separate fresh specialist.
This specialist is independent of both implementation and the primary acceptance
reviewer. Add another specialist when a changed high-risk security, tenancy,
concurrency, durability, or persistent-data boundary needs expert review.

## Audit repository standards independently

Give the repository-standards specialist the immutable subject, complete diff,
repository root, and available verification results. Do not provide the primary
reviewer's conclusions or an intended verdict.

The specialist reads `AGENTS.md`, [agent rules](../../../architecture/agent-rules.md),
and the complete applicable authority selected through the
[reference router](../../../architecture/references/README.md). It maps every
changed surface to its governing rules and inspects enough surrounding source,
tests, manifests, generated artifacts, and consumers to determine compliance.
It audits all touched languages and layers; one surface cannot stand in for
Rust, Python, TypeScript, server, contract, test, documentation, or tooling
rules that independently apply.

The specialist returns:

1. an authority-coverage table mapping each changed surface to every applicable
   repository authority;
2. a pass or fail result for each applicable rule, with exact rule and source
   evidence; and
3. material repository-rule findings with the violated rule, location,
   consequence, and testable correction.

The specialist does not review task acceptance, propose optional improvements,
or repeat the Ponytail audit. Missing authority, incomplete coverage, or an
unavailable independent specialist blocks the review. Preserve its report as
`standards-review.md` in the review directory and include every material
standards finding in the primary verdict and remediation task.

## Apply adversarial Ponytail review

Start unconvinced. The candidate earns `PASS` through repository, diff, and
verification evidence; intent, summaries, plausible code, and green checks alone
do not establish completion. Try to falsify every acceptance criterion,
constraint, non-goal, and claimed regression boundary through a realistic
reachable path.

Apply the Ponytail ladder to every changed abstraction, dependency,
configuration surface, compatibility path, generic layer, and speculative
extension:

1. Can it be deleted while preserving the complete task?
2. Does existing repository behavior already solve it?
3. Does the standard library or native platform solve it?
4. Does an already-installed dependency solve it?
5. Only then, is the new code the minimum necessary behavior?

Require source evidence for the complexity. When a smaller existing solution
satisfies every requirement and constraint, classify the unnecessary addition
as `DRIFT` and require deletion or simplification. Prefer one root-cause fix in
the shared owner over repeated symptom guards. Do not mistake fewer lines for a
valid simplification when it weakens validation, error handling, security,
accessibility, durability, or another explicit requirement.

Best-practice review means enforcing applicable repository rules and the
simplest maintainable solution required by the task. It does not authorize
subjective cleanup, a preferred style, or broader redesign.

## Audit acceptance

Build an explicit matrix:

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `<obligation>` | `<diff or source location>` | `<test/check or N/A>` | `PASS | FAIL` |

Inspect specifically for:

- **MISSING** — required behavior was not implemented;
- **INCORRECT** — behavior exists but does not satisfy the requirement;
- **DRIFT** — implementation extends beyond the requested scope;
- **VIOLATION** — an explicit constraint, non-goal, or repository rule was
  violated; and
- **REGRESSION** — existing behavior was unintentionally changed.

Do not report optional improvements, speculative hardening, preferences,
unrelated pre-existing debt, or refactors not required by the task. Tests prove
behavior; they do not prove that the requested behavior was the behavior built.
Rely on repository source and the diff, not agent summaries.

Give every material finding a stable `FIND-<task>-<n>` ID plus its
classification, violated obligation, exact location, evidence, observable
consequence, and required testable correction. For `DRIFT`, identify what can be
deleted or which existing or native mechanism already covers the outcome.
Prescribe the outcome and boundary, not private implementation mechanics when
several equally minimal corrections remain.

## Verdict and remediation task

Create a new `changes/active/<slug>/review/<review-name>/` directory without
overwriting a prior attempt. Write the specialist's `standards-review.md`, then
write `verdict.md` containing the immutable subject, acceptance matrix,
repository-standards result, verification limits, material findings,
prior-finding closure, and one verdict:

- `PASS` — every obligation and the independent repository-standards audit
  passes, non-goals remain excluded, verification is credible, and no unrelated
  change entered the diff;
- `FIX_REQUIRED` — one or more bounded implementation findings remain;
- `SPEC_REVISION_REQUIRED` — correction requires changing approved behavior or
  an expensive-to-reverse decision; or
- `BLOCKED` — the immutable subject, authority, diff, or required independent
  review cannot be obtained.

For `FIX_REQUIRED`, also write one self-contained remediation task named
`<task-id>-R<n>-<name>.md` in the same review directory. It must contain:

1. the approved spec path, original task path, and candidate identities;
2. an issue diagnosis for each material finding: the violated obligation,
   current behavior, exact evidence, observable consequence, and why the
   candidate or its existing proof falls short;
3. the intended correction outcome;
4. a decision-complete recommendation within the approved behavior: select the
   minimal correction approach, name the existing owner or mechanism to reuse,
   resolve alternatives that would change scope or proof, and explain why that
   approach closes the diagnosed gap;
5. constraints, preserved behavior, and explicit non-goals;
6. acceptance criteria mapped to every finding; and
7. focused proof that directly exercises the gap plus broader verification.

Do not write an outcome checklist or merely restate the acceptance matrix. The
diagnosis and recommendation are the substance of the remediation task;
acceptance criteria only prove that correction. An implementer must not need to
rediscover the defect or choose the correction boundary. If that recommendation
requires a new product, public API, architecture, security, compatibility,
cross-service, concurrency-semantics, or persistent-data decision, return
`SPEC_REVISION_REQUIRED` instead.

The remediation task packages validated findings for a fresh implementation
agent; it is not another design plan. Do not specify helpers, private methods,
local control flow, fixture structure, or optional improvements. Route it
directly to `$wyrd-implement`. A later review reassesses the complete cumulative
candidate against the original task.

Return only the verdict, verdict path, remediation task path when present, and
finding IDs. `PASS` is the task's completion gate. Review does not implement,
merge, push, or deploy.
