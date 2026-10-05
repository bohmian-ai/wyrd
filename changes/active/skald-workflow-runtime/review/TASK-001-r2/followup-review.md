# Focused follow-up review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `28473e049705595306f2934cf4bc664168254086`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 10
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Candidate hash before inspection: `28473e049705595306f2934cf4bc664168254086`
- Candidate hash after inspection: `28473e049705595306f2934cf4bc664168254086`

## Uncertainty reviewed

The standards review reports that the committed base-to-candidate range fails
`git diff --check` on the round-1 validation report. A contrary pass was
attributed to an invocation without tree arguments. This follow-up determines
what those commands inspect and whether the failure belongs in the cumulative
task review.

## Source and command paths inspected

- `changes/active/skald-workflow-runtime/review/TASK-001-r1/findings-validation.md`
- `changes/active/skald-workflow-runtime/review/TASK-001-r2/standards-review.md`
- `changes/active/skald-workflow-runtime/review/TASK-001-r2/task-review-invariants.md`
- `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md:420-466`
- `architecture/references/languages/implementation-execution.md:242-258`
- `architecture/references/languages/spec-driven-development.md`
- `AGENTS.md` §§11-12

The following checks were reproduced at candidate `28473e049`:

| Command | Exit | What it proves |
|---|---:|---|
| `git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721..28473e049705595306f2934cf4bc664168254086` | 2 | The immutable cumulative committed range contains `findings-validation.md:269: new blank line at EOF`. |
| `git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721 28473e049705595306f2934cf4bc664168254086` | 2 | The equivalent two-tree comparison reports the same committed defect. |
| `git diff --check` | 0 | Only tracked working-tree changes relative to the index are clean; it says nothing about already committed changes and ignores the untracked round-2 review directory. |
| `git diff --cached --check` | 0 | The index relative to `HEAD` is clean; it says nothing about the base-to-candidate range. |
| `git diff --check 28473e049705595306f2934cf4bc664168254086` | 0 | The tracked working tree matches the candidate; it does not inspect changes already present between base and candidate. |

The candidate blob ends with bytes `2e 0a 0a`, so the final content line is
followed by two line feeds. The path does not exist at the base. Commit
`e84a9fd9c` introduced it, and
`git diff --check eb22b03f2..e84a9fd9c -- <path>` reproduces the failure.
The later Revision-10 commit is clean in isolation, so it neither introduced
nor corrected the defect.

## Resolution

The passing no-argument invocation and the failing cumulative invocation are
both accurate, but they inspect different states. Only the explicit
base-to-candidate command proves the immutable subject required by task review.
The working-tree-only pass cannot support a claim that candidate `28473e049`
passes the selected check.

The defect is physically in a prior review artifact and has no runtime effect.
It is nevertheless a reachable, bounded acceptance finding rather than a mere
verification limit: the spec-driven workflow keeps the complete active packet
inside the reviewed candidate; the original task explicitly selects
`git diff --check`; its completion evidence claims that check passed; and the
implementation-execution authority lists it as required focused verification.
The immutable cumulative range directly falsifies that recorded claim.

## Proposed finding

### FOLLOWUP-R2-001 — VIOLATION — cumulative diff-check evidence is false

- Violated obligation: the original task's `Verification and Evidence` section
  requires `git diff --check`, and its scoped-lane evidence records that check
  as exit zero; task review must assess the complete immutable
  base-to-candidate range.
- Exact location:
  `changes/active/skald-workflow-runtime/review/TASK-001-r1/findings-validation.md:269`
  and the stale evidence claim at
  `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md:466`.
- Evidence: the explicit base-to-candidate command exits 2 with `new blank line
  at EOF`; the candidate blob ends in two line feeds. No-argument and cached
  checks exit zero because they exclude already committed range changes.
- Observable consequence: the immutable candidate cannot substantiate its
  required clean-diff evidence. There is no production or runtime consequence.
- Minimum testable correction: remove only the extra terminal blank line from
  the existing round-1 report, leaving one terminating line feed. In the new
  immutable candidate, run and record
  `git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721..<new-candidate>`
  with exit zero. No code change, test, new check, or broader formatting pass is
  needed.

## Follow-up result

**RESOLVED**

The standards report used the command that covers the immutable cumulative
subject. The working-tree-only pass does not contradict it. Retain the bounded
verification finding for independent Ponytail validation; this follow-up does
not choose the overall verdict or assign a stable final `FIND-*` ID.
