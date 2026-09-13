---
name: wyrd-task-review
description: Orchestrate a required two-wave, multi-agent audit of one immutable cumulative Wyrd task implementation, then write its verdict and any remediation task from independently validated findings.
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
CodeGraph instructions. Map the changed surfaces only far enough to select the
required `domain-rev` agents; the reviewers own inspection and judgment.

Before spawning Wave 1, create a new
`changes/active/<slug>/review/<review-name>/` directory without overwriting a
prior attempt. Assign every reviewer its exact report path in that directory.

## Required agent topology

The calling agent is the review orchestrator, not one of the reviewers. It MUST
use the harness's agent-delegation capability to spawn separate fresh subagents;
simulating multiple reviewer roles in one context does not satisfy this skill.
No agent may fill more than one role.

Run the review in two waves:

1. **Wave 1, in parallel:** always spawn a task implementation reviewer
   (`task-rev`) and repository-standards reviewer (`repo-rev`). Also spawn one
   domain reviewer (`domain-rev`) for each materially changed sensitive domain,
   including security/RBAC, tenancy, concurrency, durability, persistent data,
   or another boundary whose correctness needs domain expertise.
2. **Wave 2:** after every Wave 1 report is complete, always spawn one
   structured Ponytail reviewer (`ponytail-rev`). It independently validates
   the union of all findings and produces the final finding ledger and
   decision-complete recommendations.

Wave 1 reviewers receive the immutable subject and inputs needed for their own
scope, but not another reviewer's conclusions or an intended verdict. The
`ponytail-rev` receives the complete diff, applicable authorities, and every
Wave 1 report. The orchestrator only establishes the subject, routes inputs,
checks report completeness, and writes final artifacts from the validated
ledger. If any required agent cannot be spawned, any required report is
missing, or the candidate changes during either wave, return `BLOCKED`.

## Wave 1: task implementation review (`task-rev`)

The `task-rev` must be fresh relative to implementation. Give it the original
specification and task, actual diff, relevant repository rules, and available
verification results—not the implementation agent's completion summary or an
intended verdict.

Start unconvinced. The candidate earns acceptance through repository, diff, and
verification evidence; intent, summaries, plausible code, and green checks
alone do not establish completion. Try to falsify every acceptance criterion,
constraint, non-goal, and claimed regression boundary through a realistic
reachable path.

Apply the Ponytail ladder to every changed abstraction, dependency,
configuration surface, compatibility path, generic layer, and speculative
extension: delete it if the task does not need it; otherwise reuse repository,
standard-library, native-platform, or installed-dependency behavior before
accepting new code. Require the smallest safe root-cause correction without
weakening validation, error handling, security, accessibility, or durability.

Build an explicit matrix:

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `<obligation>` | `<diff or source location>` | `<test/check or N/A>` | `PASS | FAIL` |

Inspect specifically for:

- **MISSING** — required behavior was not implemented;
- **INCORRECT** — behavior exists but does not satisfy the requirement;
- **DRIFT** — implementation extends beyond the requested scope;
- **VIOLATION** — an explicit constraint or non-goal was violated; and
- **REGRESSION** — existing behavior was unintentionally changed.

Do not report optional improvements, speculative hardening, preferences,
unrelated pre-existing debt, or refactors not required by the task. Tests prove
behavior; they do not prove that the requested behavior was built. Rely on
repository source and the diff, not agent summaries.

Return `task-review.md` with the acceptance matrix, proposed findings, and one
overall `PASS`, `FAIL`, or `BLOCKED` result. Each finding needs a source-local
ID, classification, violated obligation, exact location, evidence, observable
consequence, and required testable correction. For `DRIFT`, identify what can
be deleted or which existing or native mechanism already covers the outcome.

## Wave 1: repository standards review (`repo-rev`)

Give the repository-standards specialist the immutable subject, complete diff,
repository root, and available verification results. Do not provide `task-rev`
conclusions or an intended verdict.

The specialist reads `AGENTS.md`, [agent rules](../../../architecture/agent-rules.md),
and the complete applicable authority selected through the
[reference router](../../../architecture/references/README.md). It maps every
changed surface to its governing rules and inspects enough surrounding source,
tests, manifests, generated artifacts, and consumers to determine compliance.
It audits all touched languages and layers; one surface cannot stand in for
Rust, Python, TypeScript, server, contract, test, documentation, or tooling
rules that independently apply.

The `repo-rev` returns:

1. an authority-coverage table mapping each changed surface to every applicable
   repository authority;
2. a pass or fail result for each applicable rule, with exact rule and source
   evidence; and
3. material repository-rule findings with source-local IDs, the violated rule,
   location, consequence, and testable correction; and
4. one overall `PASS`, `FAIL`, or `BLOCKED` result.

The specialist does not review task acceptance, propose optional improvements,
or perform the Ponytail audit. Missing authority or incomplete coverage blocks
the review. Preserve its report as `standards-review.md`.

## Wave 1: sensitive domain review (`domain-rev`)

Spawn a separate `domain-rev` for each sensitive domain materially changed by
the candidate. Scope each agent to one coherent domain and give it the approved
specification, original task, complete diff, governing authority, surrounding
code, available verification results, and exact report path needed to trace that
boundary end to end. Examples include an RBAC/security reviewer for permission
and trust-boundary changes or a data-layer reviewer for transaction, RLS,
durability, concurrency, and persistent-state changes.

Each `domain-rev` returns `domain-review-<domain>.md` with its reviewed boundary,
authority and source coverage, verification limits, material proposed findings,
and one overall `PASS`, `FAIL`, or `BLOCKED` result. Every finding needs a
source-local ID, violated obligation, exact location, evidence, observable
consequence, and testable correction. It does not broaden the approved task or
report speculative hardening.

## Wave 2: structured Ponytail validation (`ponytail-rev`)

Always spawn a fresh `ponytail-rev` after all Wave 1 reports are complete, even
when their proposed finding union is empty. Give it the immutable subject,
applicable authorities, complete diff, `task-review.md`, `standards-review.md`,
and every `domain-review-<domain>.md`, but no intended verdict.

The `ponytail-rev` independently inspects the actual source, validates every
Wave 1 finding and correction, removes duplicates, and resolves contradictions
from approved authority. Apply this ladder to every finding and proposed
remediation:

1. Can it be deleted while preserving the complete task?
2. Does existing repository behavior already solve it?
3. Does the standard library or native platform solve it?
4. Does an already-installed dependency solve it?
5. Only then, what is the minimum necessary correction?

Prefer one root-cause fix in the shared owner over repeated symptom guards. Do
not mistake fewer lines for a valid simplification when it weakens validation,
error handling, security, accessibility, durability, or another explicit
requirement. For each proposed finding the `ponytail-rev` must:

1. trace every caller and read the full body of each function the correction
   would change or move;
2. prove the reported path is reachable and required by the approved task,
   rejecting dormant, test-only, speculative, or zero-caller surfaces unless
   the task explicitly requires them;
3. distinguish the requested behavior from bundled adjacent behavior and reject
   a correction that moves, duplicates, or weakens unrelated lifecycle,
   admission, durability, security, or resource ownership;
4. ask whether the finding or remediation can be deleted, whether existing
   behavior already satisfies the task, and whether the proposed proof is the
   smallest credible check without a new dependency or test harness; and
5. return `CONFIRMED`, `REVISED`, or `REJECTED` with source evidence and the
   smallest safe correction boundary.

The final deduplicated ledger records each retained finding's stable
`FIND-<task>-<n>` ID, Wave 1 source IDs, `CONFIRMED` or `REVISED` status,
classification, violated obligation, exact location, evidence, observable
consequence, decision-complete correction, and focused closure proof. Preserve
prior `FIND-*` IDs during remediation and assign the next unused number only to
new findings. Each correction selects the smallest safe approach, names the
existing owner or mechanism to reuse, and preserves adjacent behavior. When
Wave 1 proposed no findings, return an explicitly validated empty ledger.

The orchestrator may include only independently confirmed or revised findings.
A rejected finding is omitted, not softened into optional advice. If validation
shows that the correction needs a new product, public API,
architecture, security, compatibility, cross-service, concurrency, resource-
ownership, or persistent-data decision, do not prescribe it as remediation;
return `SPEC_REVISION_REQUIRED` when the approved task truly requires that
decision, otherwise reject the finding as out of scope.

Missing source, incomplete caller tracing, an unavailable independent
`ponytail-rev`, or disagreement that cannot be resolved from approved authority
blocks the review. Preserve its final ledger and recommendations as
`findings-validation.md` in the review directory.

## Verdict and remediation task

In the established review directory, preserve `task-review.md`,
`standards-review.md`, every `domain-review-<domain>.md`, and
`findings-validation.md`, then write `verdict.md` containing the immutable
subject, acceptance matrix, Wave 1 results, validated finding ledger,
verification limits, prior-finding closure, and one verdict:

- `PASS` — `task-rev`, `repo-rev`, and every selected `domain-rev` pass,
  `ponytail-rev` completes with an empty validated ledger, every obligation
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
