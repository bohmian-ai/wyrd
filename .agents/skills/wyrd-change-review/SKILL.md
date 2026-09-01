---
name: wyrd-change-review
description: Review an immutable integrated Wyrd change against its active approved spec, tasks, architecture authority, seams, journeys, and evidence, then automatically invoke wyrd-complete on approval. Use after every task is approved and integrated.
---

# Wyrd Change Review

Perform final specification verification for one immutable integrated
base-to-target range. This is not per-task review and never reviews a mutable
working tree. The review phase remains read-only; successful completion is a
separate automatic handoff.

## Required authority and subject

Require repository root, unambiguous base and target commits, the complete
`changes/active/<slug>` packet in the target, available task-review verdicts and
execution evidence, and delivery context when supplied. Read the approved spec,
tasks, and their execution evidence directly from that immutable candidate.
Remain read-only during review and do not merge, push, or implement fixes.

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
journeys, evidence identity and adequacy, completed-task closure, permanent
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

Return `APPROVE`, `REMEDIATE`, `SPEC_REVISION_REQUIRED`, or `BLOCKED`. Lead with
the verdict and findings. Then provide reviewed commit and authority identities,
impact coverage, the complete requirement-to-evidence matrix, task-review
closure, verification limits, merge readiness, and a completion payload with
the durable intent, shipped behavior, lasting invariants and constraints,
material decisions and rationale, revisions or deviations, evidence summary,
delivery references, and current authority links.

`REMEDIATE` routes to `$wyrd-plan`; `SPEC_REVISION_REQUIRED` routes to
`$wyrd-spec`; `BLOCKED` stops. `APPROVE` is not a stopping point: immediately
load and execute `$wyrd-complete` in the same turn without waiting for another
developer prompt. Pass it the reviewed base and target identities, active
packet, task-review closure, requirement-to-evidence matrix, and completion
payload. If automatic completion cannot run, report the approved review and the
exact completion blocker; never claim the change is complete.

Review approval is evidence. Automatic completion does not merge, push,
deploy, or grant product authorization.
