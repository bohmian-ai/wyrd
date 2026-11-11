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

Before editing, record an `## Obligations` ledger in the task or remediation
file covering its distinct required outcomes and material invariants. Reuse
approved requirement and acceptance IDs where possible; add a local ID only
for a distinct promise without a suitable reference. Describe each promise once,
citing its contract source and planned proof. Group related scenarios and
surface variants beneath it, keeping every required negative, concurrency,
tenancy, and lifecycle case visible. Split a row only for an independently
meaningful outcome, invariant, or failure condition; do not target a row count.

The ledger records the existing contract: it cannot add requirements or prescribe
private implementation structure. An obligation does not imply a separate test,
helper, implementation step, or production mechanism. One test may prove several
obligations, and one obligation may need several forms of evidence. Test names
are mechanics; a matching selector proves only the assertions the test contains.

A conversation summary is not the contract. After any context compaction, and
again before recording evidence, re-read the task file and reconcile the ledger
against it.

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

1. add or select the smallest test that fails for the missing behavior;
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

## Delegate by obligation

A subagent receives the verbatim task text and ledger IDs of the obligations it
owns, plus the repository rules for its write set, never a paraphrase. It
returns per-obligation evidence; verify the assertions or inspect the cited
non-runtime proof, as appropriate, before accepting its work.

## Verify and record evidence

Run the narrowest verification set that covers your write set: exact
named tests (journey tests by exact selector), the owner lanes and boundary
checks your diff touches, format and lint checks, `git diff --check`, and a
final tracked and untracked diff audit. Do not run full user-journey suites or
broad aggregates for a task or remediation; they run once at change review. Derive corrected commands when task text is stale, without
substituting weaker proof.

Record RED for each new or changed behavioral scenario: the selected test and
its missing-behavior failure. Production code written before its failing test
is a process exception to record, not a pass. Already-correct behavior may use
existing proof; documentation and structural constraints do not need artificial
RED failures.

Append compact evidence to the task or remediation task, one row per ledger
obligation:

| Obligation and required cases | Implementation evidence | Proof location | Verification evidence | Result |
|---|---|---|---|---|
| `<ID: observation>` | `<location>` | `<file:line>` | `<test or check>` | `PASS | FAIL` |

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
task file, the ledger, and the diff, never your summary. It answers: which
rows duplicate promises, lack a contract source, or prescribe private mechanics;
which required outcomes or cases are missing; and which lack appropriate proof?
Correct the ledger without changing the contract, then resolve or mark `FAIL`
every unproven required outcome or case.

Return `IMPLEMENTED` only when every obligation is `PASS`. Otherwise return
`BLOCKED` with the open obligation IDs, or `SPEC_REVISION_REQUIRED` when closing
one needs an expensive-to-reverse decision. A goal, stop hook, or completion
pressure never justifies a `PASS`. Return the status with the task path,
verification status, material risk, and diff or commit reference. Create a
commit only when requested. `IMPLEMENTED` routes the immutable cumulative
candidate to `$wyrd-task-review`; only that independent review can complete the
task. Implementation does not approve, merge, push, or deploy it.
