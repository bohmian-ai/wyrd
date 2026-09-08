---
name: wyrd-change-review
description: Independently audit an immutable integrated Wyrd change against its approved specification, task verdicts, cross-task seams, actual diff, and verification evidence, then complete it on approval.
---

# Wyrd Change Review

Determine whether the immutable integrated repository satisfies the approved
specification exactly. Review remains read-only; completion is a separate
automatic handoff.

## Establish the subject

Require unambiguous base and target commits, the complete active change packet,
task-review verdicts, execution evidence, and delivery context when supplied.
Read these directly from the target. Read `AGENTS.md`,
[agent rules](../../../architecture/agent-rules.md),
[spec-driven development](../../../architecture/references/languages/spec-driven-development.md),
and applicable architecture and testing references. Follow CodeGraph
instructions and inspect the complete diff plus enough surrounding code to
evaluate integrated impact.

The reviewer must be fresh relative to implementation and integration. If the
current context participated in either, delegate this audit to one fresh
reviewer; otherwise review directly. Give the reviewer the approved spec,
original tasks, actual integrated diff, relevant repository rules, and
verification results. Its scope is cross-task seams, resolution drift,
aggregate behavior, public and durable contracts, security and reliability
boundaries, required journeys, and integrated regressions. Do not provide an
intended verdict or rely on implementation summaries.

## Audit acceptance

Build a matrix for every `REQ-*`, `INV-*`, `AC-*`, constraint, and non-goal:

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `<obligation>` | `<source/diff>` | `<test/check or N/A>` | `PASS | FAIL` |

Confirm every task has a credible `PASS` review bound to code present in the
integrated target. Re-review an affected task when conflict resolution or other
integration changes prevent ancestry or exact-patch equivalence from proving
that its approved candidate is unchanged.

Classify material concerns only as `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`,
or `REGRESSION`. Do not report optional improvements, preferences, unrelated
debt, or speculative hardening. Each finding names the violated obligation,
exact location, evidence, consequence, required outcome, and closure proof.

## Verdict and routing

Return `PASS`, `FIX_REQUIRED`, `SPEC_REVISION_REQUIRED`, or `BLOCKED`.

- `FIX_REQUIRED`: write a self-contained remediation task under a new
  `changes/active/<slug>/review/<review-name>/` directory using the same compact
  contract as `$wyrd-task-review`, then route it directly to `$wyrd-implement`.
- `SPEC_REVISION_REQUIRED`: route to `$wyrd-spec` for explicit human approval.
- `BLOCKED`: report the exact missing subject, authority, review, or evidence.
- `PASS`: immediately load and execute `$wyrd-complete` with the reviewed
  identities, acceptance matrix, task-review closure, and completion evidence.

Lead with the verdict and list only material findings and verification limits.
Review does not implement, merge, push, or deploy.
