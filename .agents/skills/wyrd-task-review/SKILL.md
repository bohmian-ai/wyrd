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
counterevidence, a concrete recommendation, and focused closure verification.

Recommend the smallest repository-native correction supported by the evidence.
Name the production owner, the control-flow, state, or API change, the existing
mechanism to reuse, and the focused test change. Fix the shared root cause, not
one reported symptom; do not add a dependency, abstraction, or configuration
surface unless the existing repository cannot satisfy the required outcome. A
recommendation is planning input, not new authority. When multiple materially
different corrections remain valid, state the unresolved choice and its fixed
constraints instead of inventing a preferred private design.

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

Use this exact user-facing structure for every verdict. Keep every heading and
field in this order; write `None` when a field has no entries.

```markdown
# <VERDICT>

## Subject
- Base: <commit>
- Candidate: <commit>
- Planning snapshot: <commit or None>
- Evidence snapshot: <commit or None>

## Verification
- Reused: <current recorded evidence or None>
- Rerun: <commands or None>
- Not run: <commands and limits or None>

## Findings

### <FINDING-ID> — <SEVERITY>: <title>
- Obligations: <spec and task IDs>
- Locations: <source and authority locations>
- Scenario: <reachable path>
- Consequence: <observable failure>
- Supporting evidence: <evidence>
- Counterevidence: <evidence or None>
- Recommendation: Reuse <existing mechanism> in <production owner>; change <control flow, state, or API>; update <focused test>.
- Required outcome: <testable result>
- Closure verification: <focused tests and commands>

## Obligation Coverage
- <compact coverage statement>

## Prior Finding Closure
- <finding ID and status, or None>

## Routing
- Next skill: <$wyrd-plan, $wyrd-spec, $wyrd-change-review, or None>
- Finding IDs: <IDs or None>
```

When there are no findings, replace the complete example finding block under
`Findings` with `- None`. Under `APPROVE`, keep obligation coverage compact
instead of repeating the task or spec. List only validated findings.
`REMEDIATE` routes them to `$wyrd-plan`, not directly to ad hoc implementation.
`APPROVE` approves only this task candidate; it does not merge, push, deploy, or
replace final `$wyrd-change-review`. The review phase does not edit the active
packet; the caller or execution harness may record the returned verdict there.
