---
name: wyrd-task-review
description: Independently verify an immutable Wyrd task's assigned requirements, scenario coverage, implementation, and evidence, with validated findings and focused remediation review.
---

# Wyrd Task Review

Answer: does this candidate implement every approved requirement assigned to
this task, preserve applicable repository invariants, and provide sufficient
evidence? The maintainer initiates review. Return the verdict and any
remediation task to the caller; do not launch implementation or another review.

The approved spec defines behavior. Tasks define scenarios, implementation
records evidence, and review independently verifies all three against source.
A task can omit required behavior or prescribe invalid mechanics. Neither its
wording, passing tests, nor a finding creates new product authority. This is an
acceptance audit, not an opportunity to improve, redesign, or refactor.

## Establish the subject

Require the repository root, unambiguous base and candidate commits, approved
spec revision, original task, actual diff, governing authority, and available
verification results. Keep the reviewed source immutable; if the candidate
changes during review, return `BLOCKED`.

Read `AGENTS.md`, nested `AGENTS.md` files for the changed directories,
[spec-driven development](../../../changes/spec-driven-development.md),
[maintainer style](../../../agent_docs/maintainer-style.md), and the applicable
architecture and [testing](../../../TESTING.md) references. Follow CodeGraph
instructions when tracing source. Build a compact navigation map of owners,
changed symbols, callers, consumers, and tests; it is a starting point, not a
coverage claim or a conclusion.

Create a new `changes/active/<slug>/review/<review-name>/` directory without
overwriting prior attempts. It holds reports and remediation tasks only; never
store diffs, patches, or source archives there. Record the commit identities;
reviewers read `git diff <base>...<candidate>` directly. For remediation use
`git diff <prior-candidate> <candidate>`, with the cumulative range available.
If rebasing changed ancestry, establish patch correspondence before carrying
forward evidence and inspect differences that affect behavior.

## Delegate independent review

The calling agent orchestrates; it does not act as a reviewer or validate its
own findings. Use fresh, separate subagents for discovery and validation;
simulating roles in one context is not independent review.

- **First review:** one primary `behavior-rev` reviews assigned requirements,
  coverage, implementation, assertions, repository rules, maintainability, and
  relevant failure/recovery paths. It writes `task-review-behavior.md`.
- **Specialists:** add a reviewer for each materially affected boundary that
  needs separate expertise. Use `invariant-rev` for complex shared state or
  lifecycle behavior, `repo-rev` for broad cross-tier rule coverage,
  `maintainer-rev` for substantial owner or abstraction changes, `system-rev`
  for material failure propagation or recovery, and `domain-rev` for sensitive
  security, tenancy, concurrency, or persistent-data boundaries. Scope each to
  a concrete boundary and give it an exact report path.
- **Validation:** one fresh `ponytail-rev` independently checks requirement
  coverage, proof strength, specialist applicability, and every proposed
  finding, even when discovery found nothing. It writes
  `findings-validation.md`. No reviewer validates its own new finding.
- **Remediation:** default to one fresh `ponytail-rev` checking prior finding
  closure, changed code, and affected paths using prior validated evidence.
  Add discovery or specialists only for material changes or unresolved gaps.
  Any new finding needs separate focused source validation before retention.

Record boundary assignments and specialist selection or omission reasons once
in the verdict. Assess affected contracts, producers, consumers, and unchanged
dependencies, not just diff paths. The primary reviewer retains coverage of
rules and boundaries not assigned to specialists. Missing required expertise
or coverage blocks acceptance; do not add a reviewer merely for another vote.

Give discovery reviewers the immutable subject, approved spec, original task,
assigned scope, relevant authority, navigation map, and evidence, without other
reviewers' conclusions or an intended verdict. The validator also receives
selected reports and prior closure evidence. All can inspect cumulative source.
Use one focused follow-up only for a material conflict, unresolved path, or
untraced common cause; a unique finding alone needs no extra discovery pass.
A missing selected reviewer or report returns `BLOCKED`.

## Shared review procedure

Every reviewer applies this procedure within its assigned scope; specialists
report coverage and findings for their boundary without repeating the complete
acceptance matrix. The validator reconciles the primary report and specialist
evidence into the final verdict record.

1. **Establish requirements.** Read the assigned approved requirements and
   relevant authority independently of the task scenarios. Check mapping and
   exclusions. A broad outcome such as "complete support" does not authorize
   additional promises. A discovered obligation needs an existing spec or
   authority reference, or evidence of a supported-use regression.
2. **Check scenario coverage.** For each assigned promise, determine whether
   scenarios exercise the relevant starting state, real input, trigger,
   expected result, failure behavior, and prohibited side effects. Follow the
   actual consumer path across task seams. Passing the listed scenarios cannot
   excuse a requirement they omit. Group equivalent cases without concealing
   required variants, negative paths, tenancy, concurrency, or lifecycle cases.
3. **Check implementation.** Trace caller-to-result behavior and state from
   producer to consumer, including sibling callers, writers, and failure paths
   where relevant. Try to falsify the promise through a realistic supported
   scenario. Check affected repository rules, generated projections, rustdoc,
   and shipped language surfaces; one surface cannot stand in for another.
4. **Check evidence.** Inspect actual assertions and execution results bound
   to the candidate, not test names or completion summaries. Establish that
   fixtures use the required dependency boundary and downstream consumers use
   predecessor outputs. Runtime behavior needs executable proof at the required
   tier; builds, plausible code, and disconnected mocks are insufficient.
   Source inspection can establish structural constraints, content review can
   establish documentation claims, and generated-output checks can establish
   projection parity. Record missing or stale execution evidence honestly.
5. **Check scope and correction.** Compare new machinery with existing owners
   and complete workflows. Prefer deletion, reuse, standard-library or native
   platform behavior, installed dependencies, then minimal new code. A coherent
   struct or reuse of leaf calls does not justify duplicate orchestration.
   Convention or a shorter alternative alone is not a defect and cannot
   override approved Wyrd behavior. Check each proposed correction against
   adjacent lifecycle, ownership, security, durability, and availability rules.

For SQL boundaries inspect actual production capability types and tenant
handling; a green boundary scanner proves only what it scans. For recovery,
trace what fails, what remains available, what survives interruption, and how
service resumes in the real process/deployment topology. Do not turn a local
component failure into a shared-service outage unless authority requires it.
Check mandatory performance evidence when applicable, not just functional tests.

Treat recorded failure diagnoses as claims: the cause must explain the trace
and the fix must sit at the shared owner. Check diagnostic requirements from
`wyrd-implement` for unexpected failures; an expected TDD RED is not such a
failure. A required diagnostician diagnoses causes, not acceptance.

Record behavioral verification and TDD compliance separately. RED evidence
must show the expected missing behavior before production changes; compilation
or setup errors do not establish RED. Later mutation checks may strengthen
assertions but cannot establish test-first history. Preserve TDD violations in
the verdict; never require historical RED or production churn to repair them.

## Coverage and findings

Use one review matrix as a verdict record with existing requirement and
scenario references, implementation locations, assertions/execution evidence,
and `PASS | FAIL | UNRESOLVED`. It does not define or rewrite requirements.
Reuse prior source-backed rows where valid; do not duplicate the task's
scenario text or create another obligations ledger. Older task records remain
usable evidence, not new authority.

Distinguish:

- **Implementation defect:** a concrete path violates a requirement. Classify
  as `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or `REGRESSION`.
- **Evidence gap:** a required claim lacks adequate assertions, test boundary,
  or valid execution evidence. Classify as `EVIDENCE_GAP`; do not claim the code
  is wrong merely because proof is missing. Required behavior without credible
  proof cannot pass.
- **Contract ambiguity:** governing text permits materially different behavior.
  Follow the shared ambiguous-wording rule. Correct unclear task mechanics
  through `wyrd-plan` when the spec settles behavior; otherwise return
  `SPEC_REVISION_REQUIRED` for clarification. Do not prescribe a preferred
  interpretation as an implementation finding.

Every proposed blocking finding identifies its source-local ID, classification,
requirement or authority, exact location, starting conditions and trigger,
expected behavior, actual behavior or missing proof, evidence, consequence,
and smallest testable correction. Show materiality for supported use or an
explicit boundary; mere reachability or a conceivable input is insufficient.
Preserve explicit mandatory gates and required safety boundaries regardless of
rarity or correction cost. Omit optional improvements, speculative hardening,
subjective style, and unrelated debt. Stop when credible coverage and material
findings are resolved; no finding quota or exhaustive input search is required.

The validator traces each proposed finding against source and affected callers,
checks the requirement reference and proof, and returns `CONFIRMED`, `REVISED`,
or `REJECTED` with reasons. Consolidate shared causes and reject speculative
corrections. For `DRIFT`, identify unnecessary machinery and the deletion or
existing owner that preserves required behavior. Challenge correction burden
without using cost to waive a required promise. Agreement is corroboration,
not proof; a unique finding can still be valid.

The validator checks coverage even when the finding set is empty, including
whether omitted specialists or carried-forward evidence leave a gap. Obtain
missing specialist evidence before a verdict. A blocker newly discovered by
the validator needs another independent focused source check. Missing source,
unresolved authority, incomplete coverage, or unavailable independent
validation blocks acceptance.

Retained findings receive stable `FIND-<task>-<n>` IDs. Preserve them across
rounds, assigning new numbers only to new findings; never relabel human scope
additions as independently discovered defects. Record each retained finding
once in `findings-validation.md`; the verdict references it. Rejected proposals
create no remediation work and are not retained as optional advice.

## Remediation review

Retain the original task, approved spec, prior verdict and candidates,
remediation task, and cumulative candidate as context. Check prior finding
closure and the latest diff's affected callers, consumers, invariants, and
tests. Carry forward unchanged acceptance evidence only when dependencies,
authority, and proof remain valid. Reopen an unchanged area for a specific
new material defect or affected behavior, recording why; inspect missing
prior evidence narrowly rather than restarting discovery across the task.

After two unsuccessful remediation rounds, distinguish incomplete fixes,
fix-induced regressions, missed original defects, and expanding expectations.
Consolidate common causes and identify the next bounded scope. This is a
convergence assessment, not an automatic PASS, round limit, or approval gate.

## Verdict and remediation task

Write `verdict.md` with immutable identities, review scope, authority references,
reviewer/boundary assignments, reconciled coverage matrix, validated finding
references, verification limits, prior closure, and any convergence assessment.
Report behavioral verification and TDD compliance separately. Return:

- `PASS`: every assigned requirement and applicable invariant has credible
  implementation and proof, non-goals remain excluded, required independent
  coverage is complete, and no validated blocking finding remains. Preserve any
  recorded TDD violation; behavioral acceptance does not erase process history.
- `FIX_REQUIRED`: validated implementation defects, evidence gaps, or mandatory
  gate violations remain within settled approved behavior.
- `SPEC_REVISION_REQUIRED`: approved behavior needs clarification or correction
  requires changing a material contract or boundary.
- `BLOCKED`: the subject, governing authority, required review, or necessary
  external access cannot be obtained, or contradictory task mechanics prevent
  acceptance under an otherwise settled spec. Name the precise missing input
  or task correction. Available but inadequate proof is an evidence gap.

For `FIX_REQUIRED`, write one cohesive `<task-id>-R<n>-<name>.md` in the review
directory containing:

- spec revision, original task, reviewed identities, and stable finding IDs;
- each diagnosed gap, its requirement, concrete behavioral scenario and evidence;
- the smallest correction boundary, existing owner/mechanism to reuse, affected
  consumers, preserved behavior, and non-goals; and
- observable closure scenarios mapped to findings and existing requirements,
  with the narrowest credible test tier, runner, and affected checks.

An implementation defect needs a correction at its owner. An evidence gap may
need only a test or valid execution evidence; do not demand production changes
without a code defect. Keep recommendations decision-complete at the behavior
and owner boundary, leaving helpers, signatures, control flow, and fixtures to
the implementer. An ambiguous contract needs clarification, not remediation
that chooses new semantics. Never add product requirements through findings.

Use focused journey tests where required, not full journey suites or broad
aggregates; those run on the integrated candidate at change review. Include
bounded mandatory documentation corrections without creating style-only rounds.
Return the verdict, verdict path, remediation path if present, finding IDs,
and material clarification or verification limits. `PASS` is the task's
completion gate. Review does not implement, merge, push, or deploy.
