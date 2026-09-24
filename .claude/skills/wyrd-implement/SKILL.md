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

Read `AGENTS.md`, [agent rules](../../../architecture/agent-rules.md),
[spec-driven development](../../../architecture/references/languages/spec-driven-development.md),
[implementation execution](../../../architecture/references/languages/implementation-execution.md),
and [testing workflows](../../../architecture/references/languages/testing-workflows.md).
Load only references applicable to the changed surface. Follow CodeGraph
instructions and inspect the nearest owner, callers, consumers, tests,
manifests, generated projections, and `mise.toml`.

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

For non-trivial behavior, work one observable scenario at a time:

1. add or select the smallest test that fails for the missing behavior;
2. implement enough production behavior to pass it;
3. refactor only when needed for the task or repository rules; and
4. continue until the acceptance criteria are covered.

Do not implement speculative requirements, optional improvements, or non-goals.
Do not weaken tests, hide gate failures, add sleeps for synchronization, mock
away required behavior, or hand-edit generated artifacts.

Stop with `SPEC_REVISION_REQUIRED` only when correctness requires changing an
approved outcome, public or persisted contract, architectural boundary,
cross-service or concurrency semantics, security or tenancy rule,
compatibility requirement, or another expensive-to-reverse decision. Otherwise
resolve repository-local details and continue.

## Verify and record evidence

Run the smallest complete verification set required by `AGENTS.md`: focused
tests, applicable owner or capability lanes, required journey or boundary
checks, format and lint checks, `git diff --check`, and a final tracked and
untracked diff audit. Derive corrected commands when task text is stale, without
substituting weaker proof.

Append compact evidence to the task or remediation task:

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `<criterion>` | `<location>` | `<test or check>` | `PASS | FAIL` |

Also confirm that every non-goal remained excluded and no unrelated file was
changed. Record commands and material limits without narrating implementation.

Return `IMPLEMENTED`, `SPEC_REVISION_REQUIRED`, or `BLOCKED` with the task path,
verification status, material risk, and diff or commit reference. Create a
commit only when requested. `IMPLEMENTED` routes the immutable cumulative
candidate to `$wyrd-task-review`; only that independent review can complete the
task. Implementation does not approve, merge, push, or deploy it.
