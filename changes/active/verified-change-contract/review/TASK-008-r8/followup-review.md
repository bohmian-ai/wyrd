# TASK-008 round-eight focused follow-up

## Immutable subject and conflict

- Candidate: `c4bc77a5c877c508191dc606b3cd3bb78047dc29`.
- Remediation range: `ea0ed46fa..c4bc77a5c877c508191dc606b3cd3bb78047dc29`.
- Approved authority: `changes/active/verified-change-contract/spec.md`,
  revision 57.
- Conflict investigated: whether changing the R6 remediation task from
  `status: ready` to the undefined `status: implemented` is a material,
  in-scope regression, or a harmless execution-record convention.

The candidate remained at the stated commit during this follow-up. The
checkout has no `.codegraph/` directory, so navigation used Git, `rg`, and
direct source inspection. This pass inspected only the disputed task-state
claim and did not reassess earlier accepted implementation or deferred
`FIND-TASK-008-CLOSEOUT-13`.

## Authority and repository evidence

1. `architecture/references/languages/spec-driven-development.md:132-179`
   defines the task contract and exhaustively moves task state through
   `proposed`, `ready`, `in_progress`, `review`, and `approved`, with
   `superseded` for invalidated work. It states that review phases are
   read-only. `implemented` is not a task state.
2. `.agents/skills/wyrd-implement/SKILL.md:96-100` uses `IMPLEMENTED` as the
   implementation agent's return result and says that this result routes the
   immutable candidate to `$wyrd-task-review`; only that independent review can
   complete the task. The result token therefore does not extend the tracked
   task-status vocabulary. At handoff, the tracked task is in `review`.
3. The changed R6 artifact now says `status: implemented` at
   `changes/active/verified-change-contract/review/TASK-008-r7/TASK-008-CLOSEOUT-R6-close-final-audit-handoff.md:4`,
   while its appended implementation evidence at lines 181-213 records finished
   work and submits the immutable candidate to this review. The range itself
   introduced the invalid state by replacing `ready` with `implemented`.
4. The nearest relevant packet pattern confirms the authority. The original
   `changes/active/verified-change-contract/tasks/task-008-closeout.md:4` uses
   `status: review`. More decisively, the independently validated
   `FIND-TASK-011-13` in
   `changes/active/verified-change-contract/review/TASK-011-r3/findings-validation.md:24-29`
   treated completed remediation tasks not marked `review` as a material
   lifecycle violation and required metadata-only correction. The subsequent
   r4 verdict at
   `changes/active/verified-change-contract/review/TASK-011-r4/verdict.md:23,38`
   accepted closure specifically because all implemented task headers read
   `review`.
5. A few other active artifacts use `status: implemented`, including an older
   file in this change packet. They are implementation drift, not authority:
   they conflict with the explicit closed vocabulary and the packet's later
   independently validated precedent. This follow-up does not reopen those
   earlier files because the user-directed scope permits findings only for the
   current range.

## Resolution

**RESOLVED.** `STD-R8-001` is a material, in-scope, range-introduced
repository-rule violation. It does not undermine the benchmark result or the
code closure of `FIND-TASK-008-CLOSEOUT-17`, but task review requires both the
implementation and its active-packet handoff to conform to governing
repository authority. The invalid state makes the current remediation record
ambiguous to task selection, review, and completion consumers: it is neither a
defined review state nor an approved state. Because the range introduced the
metadata change, retaining the finding respects rather than broadens the
closure-review boundary.

## Proposed finding

### `FOLLOWUP-R8-001` — VIOLATION — remediation handoff uses an undefined task state

- **Violated obligation:**
  `architecture/references/languages/spec-driven-development.md:175-179`
  requires completed implementation submitted to independent review to use
  `status: review`; `.agents/skills/wyrd-implement/SKILL.md:96-100` keeps
  `IMPLEMENTED` as an execution result rather than task metadata.
- **Location:**
  `changes/active/verified-change-contract/review/TASK-008-r7/TASK-008-CLOSEOUT-R6-close-final-audit-handoff.md:4`.
- **Evidence:** `ea0ed46fa..c4bc77a5` changes `status: ready` to
  `status: implemented` and appends completed implementation evidence. The
  repository's validated TASK-011 precedent required the same lifecycle
  boundary to be represented by `review`.
- **Observable consequence:** the active packet represents a submitted
  remediation with a state outside the authoritative lifecycle, so later
  workflow consumers cannot distinguish its handoff state using the defined
  contract.
- **Smallest testable correction:** change only line 4 of the R6 remediation
  artifact to `status: review`. Preserve all implementation evidence, source,
  tests, benchmark behavior, and the deferral of `FIND-TASK-008-CLOSEOUT-13`.
  Prove closure by inspecting the front matter and running
  `git diff --check`; no runtime test is warranted.

