---
name: wyrd-task-review
description: Review one immutable cumulative Wyrd task implementation against its approved spec, original task, remediation tasks, repository authority, and evidence. Use after implementation or remediation; return approval, bounded findings, or a required spec revision.
---

# Wyrd Task Review

Remain read-only. Review exactly one task's complete immutable
base-to-candidate range. After remediation, include the original task, every
remediation task, prior findings, and the cumulative candidate; never review
only the latest fix diff.

## Establish the review subject

Require repository root, unambiguous base and candidate commits, the approved
`changes/active/<slug>/spec.md` revision, the original active task, available
execution evidence, and any remediation tasks or prior findings. If the subject
changes during review, return `BLOCKED` rather than implying the new candidate
was reviewed.

Read `AGENTS.md`, [agent rules](../../../architecture/agent-rules.md),
[spec-driven development](../../../architecture/references/languages/spec-driven-development.md),
[implementation execution](../../../architecture/references/languages/implementation-execution.md),
and [testing workflows](../../../architecture/references/languages/testing-workflows.md).
Start at [the reference router](../../../architecture/references/README.md) and
load applicable [Wyrd design](../../../architecture/wyrd-design.md),
[Wyrd doctrine](../../../architecture/wyrd-doctrine.mdx),
[Bifrost design](../../../architecture/bifrost-design.md), security, operations,
language, and domain references.

Follow CodeGraph instructions. Inspect changed owners plus only the callers,
consumers, negative and cleanup paths, generated surfaces, tests, manifests,
and `mise.toml` commands needed to evaluate the task.

## Review adversarially and validate conservatively

Try to falsify each mapped requirement, invariant, acceptance obligation, TDD
scenario, changed state transition, and claimed evidence. Check exact authority
alignment, complete behavior and consumer closure, regressions, applicable hard
rules, test effectiveness and RED-to-GREEN evidence, exact named-test commands,
broader proof, absence of hidden spec changes, and resolution of every prior
finding across the cumulative candidate.

Reuse credible current evidence. Run only the narrowest sequential checks
needed to resolve missing, contradictory, suspicious, or stale proof. A passing
command is not evidence when its assertions cannot detect the claimed defect.

## Findings

Report only source-validated defects with a reachable consequence. Every
actionable finding includes a stable `FIND-<task>-<n>` ID and severity, exact
source or authority location, mapped spec and task obligations, reachable
scenario and consequence, required testable outcome, supporting and
counterevidence, and focused closure verification.

Use `CRITICAL`, `MAJOR`, or `MODERATE` only for concrete in-scope defects.
Preference, speculative cleanup, and unrelated baseline concerns do not block.
A finding is not product authority and does not prescribe private mechanics
when several solutions satisfy the required outcome.

## Verdict

Return one verdict:

- `APPROVE` — the cumulative candidate satisfies the task and mapped spec;
- `REMEDIATE` — bounded corrections remain under the approved spec;
- `SPEC_REVISION_REQUIRED` — correction requires changed behavior or another
  material decision; or
- `BLOCKED` — the immutable subject or essential authority cannot be inspected.

Lead with the verdict, reviewed commit identity, and verification limits. List
only validated findings; on `APPROVE`, add one compact obligation-coverage
statement instead of repeating the task or spec. `REMEDIATE` routes validated
findings to `$wyrd-plan`, not directly to ad hoc implementation. `APPROVE`
approves only this task candidate; it does not merge, push, deploy, or replace
final `$wyrd-change-review`. The review phase does not edit the active packet;
the caller or execution harness may record the returned verdict there.
