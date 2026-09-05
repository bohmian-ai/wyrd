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

## Run independent integration review

Dispatch one independent read-only reviewer over the exact immutable
base-to-target range. It runs in a separate agent context from both the
integrator and verdict orchestrator, receives no intended verdict, and does not
edit, implement, or plan fixes. Require it to inspect cross-task seams,
resolution drift, aggregate control flow, public and durable contracts,
generated surfaces, security and reliability boundaries, and integrated
regressions. It returns a coverage ledger, candidate findings with supporting
and counterevidence, and the strongest realistic counterexample attempted for
every clean area.

The change-review orchestrator independently source-validates and root-cause
deduplicates those candidates. Bind the independent reviewer identity,
coverage ledger, validation ledger, and adversarial clean evidence to the
completion payload. Missing independent integration review blocks `APPROVE`.

## Verify the change

Build a requirement-to-evidence matrix for every required `REQ-*`, `INV-*`, and
`AC-*`. Preserve evidence-class distinctions; passing checks, human approval,
and model review do not silently replace one another.

Always assess aggregate human intent and spec closure, every task's cumulative
result, cross-task seams, public/durable contracts, generated and first-class
language surfaces, applicable security and reliability invariants, required
journeys, evidence identity and adequacy, completed-task closure, permanent
documentation closure, and candidate-introduced regressions.

Require credible task-review evidence that every task received independent
baseline code review, every triggered specialist lens, orchestrator-side source
validation, adversarial clean evidence, and blocking-versus-follow-up
classification. Missing or materially incomplete review evidence is a
`TASK_REPAIR`; a task verdict alone is not evidence that code review occurred.

Reuse task-review evidence only when its reviewed task candidate is an ancestor
of the integrated target with no resolution change to that task's content, or
when exact patch equivalence is established. A rebase, squash, cherry-pick, or
conflict resolution that prevents either proof requires a new `$wyrd-task-review`
over the affected cumulative task range.

Review the integrated diff for defects created by task interaction, resolution
drift, and aggregate control flow. Do not repeat each complete task-level code
review when its immutable evidence remains credible, but do not let task-local
approval substitute for inspection of cross-task seams and regressions.

Reuse credible task-local evidence. Run only narrowly necessary sequential
checks to adjudicate missing or contradictory proof unless the caller requests
broader runtime verification. Broad green gates do not replace inspection of
whether their tests prove the specification.

## Findings and routing

Classify every reviewed concern as blocking, follow-up, or rejected using the
same materiality threshold as task review. A concern blocks only when it is
source-validated, reachable, introduced or left unresolved by the integrated
candidate, and materially violates approved behavior, repository authority,
correctness, security, durability, compatibility, or required verification.
Optional hardening, preferences, speculative cleanup, and unrelated
pre-existing debt are follow-up at most; unsupported, unreachable, and
duplicate concerns are rejected.

Report only confirmed blocking integrated defects as findings. Each contains a
stable ID, severity, exact location, affected spec/task obligation, reachable
consequence, required outcome, non-goals, focused closure evidence, and the
smallest decision-complete Ponytail recommendation. Walk delete, repository
reuse, stdlib or native platform, installed dependency, then minimum new code;
reject unnecessary abstraction, dependency, configuration, compatibility, and
speculative flexibility.

Preserve validated follow-up separately with its location, evidence, and reason
it does not block. Keep rejected candidates in the validation ledger so a clean
verdict remains auditable.

Classify each finding as `REMEDIATION`, `TASK_REPAIR`, or `SPEC_REVISION`.
`REMEDIATION` routes to `$wyrd-plan`. `TASK_REPAIR` routes directly to
`$wyrd-task-review` over the affected immutable cumulative task subject; it
never creates an implementation task merely to restore review evidence.
`SPEC_REVISION` routes to `$wyrd-spec` and renewed human approval.

## Verdict and output

Return `APPROVE`, `REMEDIATE`, `SPEC_REVISION_REQUIRED`, or `BLOCKED`. Lead with
the verdict, then list only validated findings and verification limits. Build
the complete requirement-to-evidence matrix and completion payload for the
workflow, but do not dump them into the user-facing response or repeat the
active packet. Pass them directly to `$wyrd-complete` on approval.

For `REMEDIATE`, route each `REMEDIATION` finding to `$wyrd-plan` and each
`TASK_REPAIR` to `$wyrd-task-review`. `SPEC_REVISION_REQUIRED` routes to
`$wyrd-spec`; `BLOCKED` stops. `APPROVE` is not a stopping point: immediately
load and execute `$wyrd-complete` in the same turn without waiting for another
developer prompt. Pass it the reviewed base and target identities, active
packet, task-review closure, requirement-to-evidence matrix, and completion
payload. If automatic completion cannot run, report the approved review and the
exact completion blocker; never claim the change is complete.

Review approval is evidence. Automatic completion does not merge, push,
deploy, or grant product authorization.
