---
name: wyrd-change-review
description: Run the terminal read-only review of an immutable integrated Wyrd change against its approved spec, all tasks, architecture authority, cross-task seams, journeys, and evidence. Use after every task is approved and temporary change artifacts are removed from the merge candidate.
---

# Wyrd Change Review

Perform final specification verification for one immutable integrated
base-to-target range. This is not per-task review and never reviews a mutable
working tree.

## Required authority and subject

Require repository root, unambiguous base and target commits, the pinned
authority commit containing the approved spec and task artifacts, available
task-review verdicts and execution evidence, and delivery context when
supplied.

The merge candidate should no longer contain `.dev/changes/<slug>`; read the
approved spec and tasks from the pinned authority commit. Confirm the cleanup
removed only temporary change artifacts and did not alter implementation
behavior. Remain read-only and do not merge, push, or implement fixes.

Read `AGENTS.md`, [agent rules](../../../architecture/agent-rules.md),
[spec-driven development](../../../architecture/references/languages/spec-driven-development.md),
[implementation execution](../../../architecture/references/languages/implementation-execution.md),
and [testing workflows](../../../architecture/references/languages/testing-workflows.md).
Start at [the reference router](../../../architecture/references/README.md) and
load applicable [Wyrd design](../../../architecture/wyrd-design.md),
[Wyrd doctrine](../../../architecture/wyrd-doctrine.mdx),
[Bifrost design](../../../architecture/bifrost-design.md), security, operations,
language, and domain references.

Follow CodeGraph instructions. Inspect the complete diff and enough unchanged
owners, consumers, contracts, generated projections, tests, manifests, and
deployment surfaces to evaluate integrated impact.

## Verify the change

Build a requirement-to-evidence matrix for every required `REQ-*`, `INV-*`, and
`AC-*`. Preserve evidence-class distinctions; passing checks, human approval,
and model review do not silently replace one another.

Always assess aggregate human intent and spec closure, every task's cumulative
result, cross-task seams, public/durable contracts, generated and first-class
language surfaces, applicable security and reliability invariants, required
journeys, evidence identity and adequacy, temporary artifact cleanup, permanent
documentation closure, and candidate-introduced regressions.

Reuse credible task-local evidence. Run only narrowly necessary sequential
checks to adjudicate missing or contradictory proof unless the caller requests
broader runtime verification. Broad green gates do not replace inspection of
whether their tests prove the specification.

## Findings and routing

Report only confirmed integrated defects. Each finding contains a stable ID,
severity, exact location, affected spec/task obligation, reachable consequence,
required outcome, non-goals, and focused closure evidence.

Classify each finding as `REMEDIATION`, `TASK_REPAIR`, or `SPEC_REVISION`.
Remediation and task repair route to `$wyrd-plan`. Spec revisions route to
`$wyrd-spec` and renewed human approval.

## Verdict and output

Return `CLEAN`, `REMEDIATION_REQUIRED`, `SPEC_REVISION_REQUIRED`, or `BLOCKED`.
Lead with the verdict and findings. Then provide reviewed commit and authority
identities, impact coverage, the complete requirement-to-evidence matrix,
task-review closure, verification limits, and merge readiness. Produce a
compact summary suitable for the pull request so the main branch does not need
a permanent change-artifact folder. `CLEAN` is review evidence, not merge or
deployment authorization.

