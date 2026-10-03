---
id: TASK-008-CLOSEOUT-R7
kind: remediation
status: review
spec: changes/active/verified-change-contract/spec.md
spec_revision: 57
original_task: changes/active/verified-change-contract/tasks/task-008-closeout.md
base: ea0ed46fad8aa9ffa38c29c1b2d093d9d06177e8
reviewed_candidate: c4bc77a5c877c508191dc606b3cd3bb78047dc29
parent_task: TASK-008-CLOSEOUT
requirements: [REQ-171]
remediates: [FIND-TASK-008-CLOSEOUT-19]
route_to: wyrd-implement
---

# Normalize the completed remediation's review status

## Outcome

Close `FIND-TASK-008-CLOSEOUT-19` by representing the implemented R6
remediation at the lifecycle state that owns independent task review. This is
a metadata-only correction to the active change packet.

## Issue diagnosis

The reviewed range
`ea0ed46fad8aa9ffa38c29c1b2d093d9d06177e8..c4bc77a5c877c508191dc606b3cd3bb78047dc29`
changes the R6 remediation header from `status: ready` to
`status: implemented`, appends completed implementation and verification
evidence, and submits that immutable candidate to `$wyrd-task-review`.

`architecture/references/languages/spec-driven-development.md` defines the
task lifecycle as `proposed`, `ready`, `in_progress`, `review`, and `approved`,
with `superseded` for invalidated work. The implementation workflow's
`IMPLEMENTED` result routes a candidate to review; it does not add another
front-matter state. The R6 artifact is therefore in a state that task-selection,
review, and completion consumers cannot classify under the repository
contract, even though its executable correction validly closes
`FIND-TASK-008-CLOSEOUT-17`.

## Intended correction outcome

The R6 remediation record says `status: review`, accurately representing that
its implementation is complete and awaiting independent approval. Its complete
diagnosis, implementation evidence, commands, and verification results remain
unchanged.

## Decision-complete recommendation

Change only line 4 of
`changes/active/verified-change-contract/review/TASK-008-r7/TASK-008-CLOSEOUT-R6-close-final-audit-handoff.md`
from `status: implemented` to `status: review`.

Reuse the repository's existing closed task lifecycle. Do not add
`implemented` as a new state, introduce a parser, checker, alias, or
compatibility convention, or alter any executable source. The invalid state is
produced in the R6 front matter, so correcting it at that owner closes the
finding without downstream guards or workflow expansion.

## Constraints and preserved behavior

- Preserve every other byte of the R6 remediation record, including its
  diagnosis, implementation evidence, exact commands, and FIND-13 deferral.
- Preserve `Queue::poll`, the public Oracle held-commit proof, and all earlier
  accepted TASK-008 behavior.
- Preserve the conclusion that `FIND-TASK-008-CLOSEOUT-17` is closed.
- Keep `FIND-TASK-008-CLOSEOUT-13`, the full default `bench:capacity` run, and
  AC-040/AC-041 qualification deferred to integration.
- Do not modify production code, tests, schemas, configuration, manifests,
  generated artifacts, or runtime behavior.

## Non-goals

- No new lifecycle state or workflow compatibility behavior.
- No cleanup of earlier out-of-scope task records that may contain drift.
- No rerun or reinterpretation of the capacity benchmark or its runtime proof.
- No production or test change.

## Acceptance criteria

### AC-R7-1 — the R6 remediation is in review

The R6 remediation front matter contains exactly `status: review`. It contains
no `status: implemented`, and every other R6 field and body section remains
unchanged.

### AC-R7-2 — the correction remains metadata-only

The remediation diff changes only the R6 status value. No production code,
test, contract, configuration, generated artifact, or other task record changes.

### AC-R7-3 — preserved closure and deferral

The R6 record still documents the `S1 -> Q1 -> S2 -> Q2` correction and proof,
keeps `FIND-TASK-008-CLOSEOUT-17` closed, and retains the integration deferral
for `FIND-TASK-008-CLOSEOUT-13` without claiming AC-040 or AC-041 qualification.

## Focused proof and broader verification

Inspect the R6 front matter and the remediation diff to prove the exact
metadata replacement and absence of any other change. Run:

```bash
git diff --check
```

No executable test, database fixture, capacity run, format lane, lint lane, or
broader verification is warranted for this one-word Markdown metadata change.
