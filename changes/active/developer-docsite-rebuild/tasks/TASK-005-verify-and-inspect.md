---
id: TASK-005
kind: implementation
status: proposed
spec: SPEC-developer-docsite-rebuild
spec_revision: 2
requirements: [REQ-003, REQ-005, REQ-006, REQ-007, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-016, INV-001, INV-002, INV-003, INV-004, INV-005, AC-001, AC-003, AC-004, AC-005, AC-006]
depends_on: [TASK-003, TASK-004]
parent_task:
remediates: []
---

## Outcome and Value

A reader can run or trigger supported verification, inspect a verdict and its
evidence, and understand what happened when verification failed. Eval, drift,
and reaction procedures appear only where there is a demonstrable end-to-end
path.

## Owners, Scope, Consumers, and Prohibited Changes

Own Verify task pages and the verification-model explanation/reference needed
for them. Use the declaration and observations from earlier tasks. Distinguish
Verifier judgment, Trigger activation, Operator reaction, Policy decision,
and retained evidence. Do not present Change Request, Code Review, workflows,
or live verification as runnable on the strength of a design alone.
Use `$human-tech-docs` for every reader-facing guide, explanation, and
reference page, including its Diátaxis, humanizer, and voice passes.

## Approach

1. Confirm the supported verification initiation and result-inspection path.
2. Author the task to obtain a pass or fail verdict and trace it to exact
   declaration, subject, Verifier, and observation identities.
3. Add evaluation, drift, and failure-reaction guides where their full paths
   can be demonstrated; put remaining concepts in Understand if useful.
4. Add reference links for verifier bindings, results, errors, and relevant
   limits; verify navigation and search.

## Proof Strategy

The task documents existing verification behavior rather than adding a
capability. Execute the supported path from declaration and observation
through a retrieved verdict; reproduce one meaningful failure and show how
the page directs recovery. Check every claimed Eval, Drift, Trigger, or
Operator path against current contracts and runnable tests. Run
`mise run docs:check` and built-site link/search checks. No manufactured RED
applies.

## Acceptance Criteria

- The primary guide reaches a real result with attributable evidence and
  shows how to inspect both a successful and failed outcome.
- Concepts explain bindings, Triggers, Verifiers, Operators, and their
  distinct roles without replacing the task procedure.
- Eval, drift, and failure-response guides are published only with runnable
  procedures; specialized candidate subjects remain outside primary task
  navigation until supported.
- Reference and error links resolve to current Wyrd contracts.

## Expected Write Set and Consumer Closure

New Verify and Understand content and associated reference links under
`docs/src/content/docs/`; generated indexes for published pages. No runtime
verification or Card contract change is in scope.

## Verification and Evidence

Observed verdict/evidence and failure transcripts, source comparison for
each published path, `mise run docs:check`, and rendered site inspection.

## Material Stop Conditions

Stop for a spec revision if the claimed end-to-end verification path cannot
produce the approved attributable result without new Wyrd behavior.

## Authority Links

- [Approved specification](../spec.md)
- [Repository instructions](../../../../AGENTS.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Wyrd doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Testing workflow](../../../../architecture/references/languages/testing-workflows.md)
