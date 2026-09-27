---
name: wyrd-task-review
description: Orchestrate a required two-wave, multi-agent audit of one immutable cumulative Wyrd task implementation, then write its verdict and any remediation task from citation-checked, reproduced, root-cause findings.
---

# Wyrd Task Review

Answer one question: does the resulting repository satisfy the original task
exactly? This is an acceptance audit, not an opportunity to improve, redesign,
or refactor the implementation beyond what a blocking finding requires.

**Candidate-owned surface.** Public API, types, traits, and seams that the
candidate introduced and the approved spec does not name are implementation
choices, not approved decisions. When a blocking finding's root cause is the
shape of such a surface, redesigning that surface is an in-scope correction.
Only surface the spec names requires `SPEC_REVISION_REQUIRED` to change.

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

Run the review in two waves with one script between them:

1. **Wave 1, in parallel:** always spawn two independent task implementation
   reviewer samples (`task-rev` A and B) and one repository-standards reviewer
   (`repo-rev`). Also spawn one domain reviewer (`domain-rev`) for each
   materially changed sensitive domain, including security/RBAC, tenancy,
   concurrency, durability, persistent data, or another boundary whose
   correctness needs domain expertise. The two `task-rev` samples receive
   identical inputs and never see each other; running both raises recall
   because each explores the code differently.
2. **Citation check:** after every Wave 1 report is complete, run
   `mise run review:verify-citations <review-dir> <candidate>`. Drop every
   finding it prints as `REJECT`; it cites code that does not exist at the
   candidate. A non-zero exit means a report broke the finding contract and
   the review is `BLOCKED`.
3. **Wave 2:** always spawn one structured Ponytail reviewer (`ponytail-rev`).
   It reproduces the surviving claims, applies the blocking gate, merges
   findings by root cause, and produces the final ledger and decision-complete
   recommendations.

Wave 1 reviewers receive the immutable subject and inputs needed for their own
scope, but not another reviewer's conclusions or an intended verdict. The
`ponytail-rev` receives the complete diff, applicable authorities, and the
claim ledger described in Wave 2, not the Wave 1 reasoning. The orchestrator
only establishes the subject, routes inputs, runs the citation check, builds
the claim ledger, and writes final artifacts from the validated ledger. If any
required agent cannot be spawned, any required report is missing, or the candidate changes during either wave, return `BLOCKED`.

## Finding contract (every Wave 1 reviewer)

A finding is a root cause, not a symptom. Before reporting, each reviewer:

1. traces the symptom back to the first place the wrong value, decision, or
   rule violation is produced, and stops at that owner, not a caller;
2. names the defect class in one line, such as "posting keys are not
   normalized before lookup", and states whether the owner's shape (a public
   field, an enum variant payload, a raw `String` where a validated type
   belongs) is what permits the defect; and
3. sweeps for siblings: every other site in the candidate with the same root
   cause or pattern, found through callers, references, and the same rule.
   Siblings are locations of one finding, never separate findings. When the
   defect is a missing check (cancellation, validation, authorization,
   cleanup), find siblings from the sinks, not a code pattern: list every
   externally visible effect and success result the facade can reach, and
   show whether the check precedes each one.

Each finding records its ID, classification, root cause, defect class, every
sibling location, violated obligation or written rule, evidence, observable
consequence, and testable correction.

Every report also ends with exactly one fenced `citations` block: a JSON array
with one entry per cited location, empty when there are no findings. Each
entry has the finding `id`, repository-relative `path`, 1-based `line`, a
verbatim `snippet` of the code starting at that line, and the `symbols` the
claim depends on:

````markdown
```citations
[{"id": "TASK-A-1", "path": "crates/x/src/lib.rs", "line": 42,
  "snippet": "let used = 0;", "symbols": ["pack"]}]
```
````

Quote code exactly; the citation check rejects paraphrase.

**Verify, don't bounce.** When a gap is missing evidence that a read-only
command can produce, such as an exact focused test run, the reviewer runs the
command and records the observed result instead of reporting a finding. Code,
test-placement, and behavior gaps still go back to implementation.

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

Each sample returns `task-review-a.md` or `task-review-b.md` with the
acceptance matrix, proposed findings, and one overall `PASS`, `FAIL`, or `BLOCKED` result. Each finding needs a source-local
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

Every finding cites a rule written in `AGENTS.md` or `architecture/` by file
and line; an inferred convention or preference is not a finding. Only
violations introduced or materially touched by the diff count. A rule that a
mise lane already enforces mechanically is proved by that lane, not re-argued.
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

Always spawn a fresh `ponytail-rev` after the citation check, even when the
surviving finding union is empty. Give it the immutable subject, applicable
authorities, complete diff, prior `FIND-*` ledger during remediation, and a
claim ledger the orchestrator builds from the citation-checked findings: for
each finding, its source ID, reporting reviewer, citations, one-sentence claim,
root cause, defect class, and sibling locations. Do not pass Wave 1 evidence
prose, matrices, overall results, or an intended verdict; the `ponytail-rev`
must reach its own conclusion from source. Keep the full reports in the review
directory for audit.

The `ponytail-rev` first merges claims that share a root cause, including
different symptoms reported by the two `task-rev` samples, and then applies
the blocking gate. A finding blocks only when at least one holds:

1. **Production path:** it is reachable from the public facade on realistic
   input and makes the common case observably wrong: wrong result, rejected
   valid request, lost or duplicated evidence, panic, or silent data loss.
2. **Task contract:** it violates a named acceptance criterion, constraint, or
   non-goal of the approved task.
3. **Repository standard:** the diff violates a rule written in `AGENTS.md` or
   `architecture/`, cited by file and line.
4. **Absolute boundary:** it breaks security, tenant isolation, audit, or
   durability, even on a rare path.

Everything else is a **note**: a preference without a written rule, an edge
case off the common production path, speculative hardening, or pre-existing
debt outside the diff. Notes are recorded in the verdict, need no
reproduction, and never block `PASS` or produce remediation.

Reproduce every blocking claim independently: show a failing focused test, a
command result, or a traced source path from the facade or the cited rule to
the defect. Agreement between the `task-rev` samples raises confidence but
never replaces reproduction. An unreproduced claim is `REJECTED`, not
softened into a note.

During remediation, a blocking claim whose root cause matches a prior
`FIND-*` reopens that ID with status `CLASS_INCOMPLETE`: the earlier fix did
not close the class. Never assign it a new ID. A `CLASS_INCOMPLETE` reopens the
earlier correction's design, not only its coverage: state why its shape let
the defect back in, and prefer replacing it over extending it.

During remediation, also audit code added by prior remediation rounds. Code a
later fix or obligation has made redundant (for example, validation subsumed by
a stronger check at the same boundary) is `DRIFT`, and its deletion belongs in
the correction. Removing a check that a stronger check covers is not weakening
validation.

Apply this ladder to every blocking finding and proposed remediation:

1. Can it be deleted while preserving the complete task?
2. Can a different shape of the candidate-owned owner make the defect
   unrepresentable, so no guard is needed?
3. Does existing repository behavior already solve it?
4. Does the standard library or native platform solve it?
5. Does an already-installed dependency solve it?
6. Only then, what is the minimum necessary guard?

Prefer one root-cause fix in the shared owner over repeated symptom guards. Do
not mistake fewer lines for a valid simplification when it weakens validation,
error handling, security, accessibility, durability, or another explicit
requirement. For each proposed finding the `ponytail-rev` must:

1. trace every caller and read the full body of each function the correction
   would change or move, and every producer of each value the correction
   validates or transforms, confirming those producers' real outputs still
   pass;
2. prove the reported path is reachable and required by the approved task,
   rejecting dormant, test-only, speculative, or zero-caller surfaces unless
   the task explicitly requires them;
3. distinguish the requested behavior from bundled adjacent behavior and reject
   a correction that moves, duplicates, or weakens unrelated lifecycle,
   admission, durability, security, or resource ownership;
4. ask whether the finding or remediation can be deleted, whether existing
   behavior already satisfies the task, and whether the proposed proof is the
   smallest credible check without a new dependency or test harness; and
5. return `CONFIRMED`, `REVISED`, `CLASS_INCOMPLETE`, `NOTE`, or `REJECTED`
   with source evidence, the gate that applies, and the smallest safe
   correction boundary.

The final deduplicated ledger records each retained blocking finding's stable
`FIND-<task>-<n>` ID, Wave 1 source IDs, status, gate, classification, root
cause, defect class, every sibling location, violated obligation, evidence,
reproduction, observable consequence, decision-complete correction at the
root-cause owner, and focused closure proof covering every sibling. Notes are
listed separately with their source IDs and one line each. Preserve
prior `FIND-*` IDs during remediation and assign the next unused number only to
new findings. Each correction selects the smallest safe approach, names the
existing owner or mechanism to reuse, and preserves adjacent behavior. When
Wave 1 proposed no findings, return an explicitly validated empty ledger.

The orchestrator may block only on reproduced `CONFIRMED`, `REVISED`, or
`CLASS_INCOMPLETE` findings.
A rejected finding is omitted, not softened into optional advice. If validation
shows that the correction needs a new product, spec-named public API,
architecture, security, compatibility, cross-service, concurrency, resource-
ownership, or persistent-data decision, do not prescribe it as remediation;
return `SPEC_REVISION_REQUIRED` when the approved task truly requires that
decision, otherwise reject the finding as out of scope.

Missing source, incomplete caller tracing, a failed citation check, an
unavailable independent `ponytail-rev`, or disagreement that cannot be resolved
from approved authority blocks the review. Preserve its final ledger and recommendations as
`findings-validation.md` in the review directory.

## Verdict and remediation task

In the established review directory, preserve `task-review-a.md`,
`task-review-b.md`, `standards-review.md`, every `domain-review-<domain>.md`,
the citation-check output as `citations.txt`, and `findings-validation.md`,
then write `verdict.md` containing the immutable subject, acceptance matrix,
Wave 1 results, rejected citations, validated blocking ledger, notes,
verification limits, prior-finding closure, and one verdict:

- `PASS` — `ponytail-rev` completes with no blocking finding, every obligation
  passes, non-goals remain excluded, verification is credible, and no unrelated
  change entered the diff; notes may remain;
- `FIX_REQUIRED` — one or more bounded implementation findings remain;
- `SPEC_REVISION_REQUIRED` — correction requires changing approved behavior or
  an expensive-to-reverse decision; or
- `BLOCKED` — the immutable subject, authority, diff, or required independent
  review cannot be obtained.

For `FIX_REQUIRED`, also write one self-contained remediation task named
`<task-id>-R<n>-<name>.md` in the same review directory. It must contain:

1. the approved spec path, original task path, and candidate identities;
2. an issue diagnosis for each blocking finding: the root cause, defect class,
   every sibling location, violated obligation, current behavior, exact
   evidence, reproduction, observable consequence, and why the candidate or its
   existing proof falls short;
3. the intended correction outcome;
4. a decision-complete recommendation within the approved behavior: select the
   minimal correction approach, name the existing owner or mechanism to reuse,
   resolve alternatives that would change scope or proof, and explain why that
   approach closes the diagnosed gap;
5. constraints, preserved behavior, and explicit non-goals;
6. acceptance criteria mapped to every finding and every sibling location; and
7. focused proof that directly exercises the gap plus broader verification.

The correction is made once at the root-cause owner and closes the whole
defect class, not only the reported symptom.

Do not write an outcome checklist or merely restate the acceptance matrix. The
diagnosis and recommendation are the substance of the remediation task;
acceptance criteria only prove that correction. An implementer must not need to
rediscover the defect or choose the correction boundary. If that recommendation
requires a new product, spec-named public API, architecture, security,
compatibility, cross-service, concurrency-semantics, or persistent-data
decision, return `SPEC_REVISION_REQUIRED` instead. Redesigning candidate-owned
surface is not such a decision. The remediation's write set includes every file
the root-cause correction touches; never narrow it below that.

The remediation task packages validated findings for a fresh implementation
agent; it is not another design plan. Do not specify helpers, private methods,
local control flow, check placement, fixture structure, or optional
improvements. For a missing check, state the rule and its complete sink list;
the closure proof covers every sink. Route it
directly to `$wyrd-implement`. A later review reassesses the complete cumulative
candidate against the original task.

Return only the verdict, verdict path, remediation task path when present, and
finding IDs. `PASS` is the task's completion gate. Review does not implement,
merge, push, or deploy.
