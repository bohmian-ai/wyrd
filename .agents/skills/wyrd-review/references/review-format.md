# Durable Implementation Review Format

Write one review artifact:

```text
.dev/review/<short-head>-<UTC-YYYYMMDD-HHMMSS>-wyrd-review/review.md
```

Use this ordered structure:

```markdown
# Wyrd Implementation Review

Status: APPROVE | RESUME_IMPLEMENTATION | REPLAN_REQUIRED | REVIEW_BLOCKED
Repository: wyrd
Review ID: <directory basename>
Created: <UTC YYYY-MM-DD>
Review target: <diff, range, or working tree>
Plan: <path, Not applicable, or unresolved>
Tasks: <task IDs and statuses, Not applicable, or unresolved>
Next artifact: <none, existing task path, revised plan/task, or missing authority>

## Summary

<Verdict and observable implementation state.>

## Plan Conformance

| Requirement / AC | Implementation | Test / artifact | Verification | Result |
|---|---|---|---|---|

## Adaptations and Evidence

- <local mechanic or bounded correction, repository evidence, and disposition>

## Adversarial Analysis

| Counter-hypothesis | Source evidence | Disposition |
|---|---|---|
| <wrong owner, duplicate path, unnecessary complexity, failed negative path, weak assertion, or missed consumer> | <authority and source> | Disproved / Finding <ID> |

Record only meaningful probes for the changed surface. When material new
structure was introduced, compare the smallest repository-native alternative
only where observable behavior, ownership, safety, proof, or material
reliability/performance/maintenance consequences differ. Do not prescribe
private helper, type, adapter, fixture, or local-module design when multiple
in-owner implementations satisfy the contract.

## Findings

### WRD-001: <plain-English issue>

Type: <finding classification>
Plan or rule: <exact authority>
Source: <path and line>
Impact: <concrete behavior, proof, or maintenance consequence>
Required outcome: <bounded correction or material decision>
Verification: <proof required>

## Verification

- <commands evidenced or independently checked and exact result>
- <required proof still missing>

## Inspected Surfaces

- <owners, consumers, contracts, generated artifacts, tests, and references>

## Handoff

<For APPROVE: no work remains.
For RESUME_IMPLEMENTATION: finding IDs, existing task path, and the
surface-appropriate execution skill ($wyrd-implement or $wyrd-ui).
For REPLAN_REQUIRED: material conflict and canonical plan/task revision.
For REVIEW_BLOCKED: missing authority or mandatory proof.>
```

Use `Not applicable: <one-sentence reason>` instead of an empty section.
Consolidate symptoms sharing one root cause. A clean review still includes the
complete conformance matrix, verification state, adaptations, and inspected
surfaces, plus the meaningful adversarial probes supporting approval.

Do not turn the review into a replacement task packet. The applicable
implementation skill receives the existing task for bounded work; `$wyrd-plan`
owns material plan revision.
