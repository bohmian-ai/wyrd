---
name: wyrd-implement
description: Implement and verify one bounded Wyrd task or review-produced remediation task, owning reversible local decisions and making the smallest sufficient change.
---

# Wyrd Implement

Implement one task, prove it, and inspect the final diff. Own reversible local
technical decisions. Do not expand the task into another implementation plan.

## Establish the contract

Read the approved spec, the original task, and applicable repository authority.
For remediation, also read the review verdict and the remediation task under
`changes/active/<slug>/review/<review-name>/`. Treat the original task plus the
validated findings as the contract; preserve portions already satisfying it.

Read `AGENTS.md`, the nested `AGENTS.md` files for every directory the diff touches,
[spec-driven development](../../../changes/spec-driven-development.md),
[implementation execution](../../../changes/implementation-execution.md),
[maintainer style](../../../agent_docs/maintainer-style.md),
and [testing](../../../TESTING.md).
Load only references applicable to the changed surface. Follow CodeGraph
instructions and inspect the nearest owner, callers, consumers, tests,
manifests, generated projections, and `mise.toml`.

Use the task's existing acceptance list as the working checklist and evidence
record. Do not append a separate obligations ledger, invent another ID family,
or restate the contract. For an older task, annotate its existing proof rows or
bullets in place; no packet rewrite is required. Keep required negative,
concurrency, tenancy and lifecycle cases visible. Add a missing required case
only with its contract reference, without expanding approved scope.

One test may prove several acceptance observations, and one observation may
need several forms of evidence. Cite shared proof once and refer to it from
the relevant entries. A test selector proves only the assertions it contains.

A conversation summary is not the contract. After any context compaction, and
again before recording evidence, re-read the task file and reconcile the
checklist against it.

A bounded direct instruction may be implemented without a change packet when
the caller explicitly requests it and no expensive-to-reverse decision remains.
Never use that exception to bypass an active approved spec.

## Implement

Implement the smallest cohesive change satisfying the acceptance criteria. Make
local decisions yourself, including helper decomposition, private signatures,
module placement inside the established owner, local control flow, use of
already-approved dependencies, and test or fixture structure. Reuse the nearest
repository behavior before adding code, abstractions, configuration, features,
or dependencies.

Revalidate relevant reuse evidence against current source before adding
graph stores, traversals, loaders, parsers, transports, validators, lifecycle
owners, caches, or orchestration layers. Record the existing owner, the precise
gap, and why the selected extension is sufficient in task evidence. Reusing
calls inside a duplicate workflow is not reuse. A cohesive struct is not proof
that another struct needs to exist.

Do not obey invalid task mechanics literally. Mechanics are commands,
selectors, paths, and file placement; required observations and outcomes are
never mechanics. Record each mechanical deviation in task evidence. Resolve
reversible choices inside the established owner yourself. If contradictory
task instructions prevent that, return `BLOCKED` with the task conflict and route task correction
to `$wyrd-plan` without manufacturing a product approval. A necessary change to
approved behavior or a material boundary routes to `$wyrd-spec` instead.

For non-trivial behavior, work one observable scenario at a time:

1. add or select the next scenario's smallest test, run its exact focused
   command, and record the expected missing-behavior failure before writing
   production code for that scenario; setup or compilation failures are not RED;
2. implement enough production behavior to pass it;
3. refactor only when needed for the task or repository rules; and
4. continue until the acceptance criteria are covered.

Do not implement speculative requirements, optional improvements, or non-goals.
Simplicity rules, including Ponytail and minimal-diff modes, govern how an
obligation is met, never whether it is met.
Do not weaken tests, hide gate failures, add sleeps for synchronization, mock
away required behavior, or hand-edit generated artifacts.

Stop with `SPEC_REVISION_REQUIRED` only when correctness requires changing an
approved outcome, public or persisted contract, architectural boundary,
cross-service or concurrency semantics, security or tenancy rule,
compatibility requirement, or another expensive-to-reverse decision. Otherwise
resolve repository-local details and continue.

## Diagnose unexpected failures

An unexpected test, lane, or check failure is a defect to diagnose, not a gate
to satisfy. Before editing to fix it, re-run it with tracing on (`WYRD_LOG`,
else `RUST_LOG`), read the trace, and append a diagnosis to the task evidence:

- **Symptom:** what failed and where;
- **Evidence:** the trace lines or source locations that show the failure;
- **Cause:** the code-level reason it fails; "flaky" or "timing" is not a
  cause; and
- **Fix site:** the shared owner where the fix belongs and the other callers
  checked there.

Without a stated cause there is no fix. Spawn a fresh read-only diagnostician
before fixing when the fix would touch a test or assertion, a timeout, sleep,
retry, `#[ignore]`, `#[allow]`, or skip; when a previous fix for the same
failure did not hold; when the fix site is outside the task's write set; or
when the cause involves timing, ordering, or concurrency. Give it only the
failing command, the trace, and the diff, never your hypothesis. It returns the
cause, fix site, and affected callers. Record its report beside the diagnosis,
then apply the Ponytail ladder to the smallest fix at that root cause.

## Never invent complexity

If there is already a standard and conventional way to do something (a
published standard such as an RFC, or the established practice of comparable
widely used projects), do it that way. Do not invent a mechanism, state,
check, file, setting, option, or error that the standard way does not have.
If the task text, a review finding, or a remediation asks for one, report it
to the caller instead of building it. Report it when found, naming the
obligation ID, and route it as `SPEC_REVISION_REQUIRED` or `BLOCKED`; do not
continue as though the obligation were settled or met. An obligation the
standard way cannot satisfy is open, not a known limit.

## Delegate by acceptance scenario

A subagent receives the verbatim task text and the acceptance entries it
owns, plus the repository rules for its write set, never a paraphrase. It
returns evidence for those entries; verify the assertions or inspect the cited
non-runtime proof, as appropriate, before accepting its work.

## Verify and record evidence

Run the narrowest verification set that covers your write set: exact
named tests (journey tests by exact selector), the owner lanes and boundary
checks your diff touches, format and lint checks, `git diff --check`, and a
final tracked and untracked diff audit. Do not run full user-journey suites or
broad aggregates for a task or remediation; they run once at change review. Derive corrected commands when task text is stale, without
substituting weaker proof.

Update the existing acceptance entries in place with concise proof locations,
RED and GREEN results, and behavioral verification status. Record each exact
command once; entries may refer to it. Do not append a second evidence table or
an implementation narrative. Already-correct behavior may use existing proof;
documentation and structural constraints need appropriate proof, not artificial
RED failures.

Report behavioral verification and TDD compliance separately. If production code
preceded its failing test, record the affected scenarios as a TDD process
violation and resume test-first work for remaining scenarios. Later mutation
checks may establish assertion strength but cannot establish test-first
execution. Do not undo correct code or manufacture historical RED evidence.

Use evidence appropriate to the claim: runtime assertions at the required test
tier for behavior, cited source/diff inspection for structural constraints,
content review for documentation, and generated-output checks for projections.
A successful build or docs check alone does not prove required behavior or
document content. Shared evidence may cover multiple obligations; identify the
cases it proves without duplicating the evidence. A grouped row passes only
when every required case has proof. An obligation without adequate cited proof
is `FAIL`; "known limit", "deviation", "covered by design", or "covered elsewhere"
alone is not `PASS`.

Also confirm that every non-goal remained excluded and no unrelated file was
changed. Record commands and material limits without narrating implementation.

Before returning `IMPLEMENTED`, spawn a fresh read-only checker with only the
task file and the diff, never your summary. It answers: which acceptance entries
lack a contract source; which required outcomes or cases are missing; which
lack appropriate proof; and whether the recorded TDD compliance is supported.
Correct the evidence without changing the contract, then resolve or mark `FAIL`
every unproven required outcome or case.

Return `IMPLEMENTED` only when every obligation is `PASS`. Otherwise return
`BLOCKED` with the open obligation IDs, or `SPEC_REVISION_REQUIRED` when closing
one needs an expensive-to-reverse decision. A goal, stop hook, or completion
pressure never justifies a `PASS`. Return the status with the task path,
behavioral verification status, TDD compliance (`PASS`, `VIOLATED`, or
`NOT_APPLICABLE`), material risk, and diff or commit reference. `IMPLEMENTED`
reports verified behavior; it does not erase a TDD violation, which must remain
visible to independent task review. Create a commit only when requested.
`IMPLEMENTED` routes the immutable cumulative
candidate to `$wyrd-task-review`; only that independent review can complete the
task. Implementation does not approve, merge, push, or deploy it.
