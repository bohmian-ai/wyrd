---
id: TASK-003a-R3
kind: remediation
status: proposed
spec: SPEC-long-horizon-verification
spec_revision: 20
parent_task: TASK-003a-R2
remediates: [FIND-TASK-003a-9]
---

# Align grouped verification documentation with the shipped incomplete-snapshot outcome

## Outcome and authority

Implement this documentation-only correction through `$wyrd-implement`. Describe
revision 20's existing behavior accurately and regenerate the existing public
error reference. No runtime changes, new safeguards or product decisions.

- Repository/worktree: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
- Branch: `change/long-horizon-verification-task-001`.
- Starting candidate: `8fa32190b26b11588721e8db44b3eb94139e3739`.
- Original cumulative base: `22335c2c40a5ad24c27bfa0d4686f4da49f922c5`.
- Approved spec: `changes/active/long-horizon-verification/spec.md`, revision 20, REQ-004 and functional/documentation closure.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-003a-simplify-grouped-verification.md`, A9 and scenario 8.
- R1: `changes/active/long-horizon-verification/review/task-003a-20261018-7cda4e931/TASK-003a-R1-retain-pending-work-and-remove-stale-code.md`.
- R2: `changes/active/long-horizon-verification/review/task-003a-r1-20261018-3d0b301c5/TASK-003a-R2-close-grouped-verification-gaps.md` (local untracked artifact with implementation evidence).
- Validated finding: `findings-validation.md` and `verdict.md` beside this task.

Read AGENTS, agent rules and applicable documentation/source-generation rules.
Keep prior task/review evidence intact. FIND-1/2/3/4/5/6/7 and human cleanup remain
closed; FIND-8 is superseded by approved revision 19. Do not implement them again.

## FIND-TASK-003a-9 diagnosis

The current runtime detects a selected execution with a visible result but no
visible input before pinning a snapshot. It retains pending work, then records a
terminal error if that condition still exists more than five minutes past due.
The existing guide and grouped architecture describe the earlier visible-row
behavior without this exception. The generated error reference also omits the
new stable code, despite advertising the complete source-derived catalog.

Affected consumers:

- `docs/src/content/docs/understand/how-verification-works.md:176–180`.
- `architecture/wyrd-design.md:1324–1345`.
- `docs/src/content/docs/api/errors.md`, verification error section.

Source producer and current proof:

- `verification/groups.rs::Visible::missing_inputs` identifies result execution
  IDs absent from the input map. `GroupPoller::poll` returns Incomplete before pinning.
- `verification/scheduler.rs::hand_off` retries while not overdue, otherwise
  records VerificationSnapshotIncomplete through existing `refuse_group`.
  The due query uses PostgreSQL `statement_timestamp()` and a strict >5-minute
  comparison. `hand_off_groups` consumes pending and commits the error atomically.
- Error readback stores code/message only; missing IDs are in the message.
  No accepted authority, inputs, snapshot, evaluation, verdict/result or Operator
  is created for this pre-execution error.
- `wyrd-spec/src/error.rs` owns code `WYRD_VERIFICATION_409_SNAPSHOT_INCOMPLETE`;
  existing `docs/scripts/generate_api_docs.py::error_catalog/render_errors` reads
  its metadata and renders the missing row. TS error codes already contain it.

The review's read-only generator comparison returned False with exactly this
missing row. P1/P2 runtime checks pass. Thus the defect is misleading consumed
documentation and generated output drift, not a runtime failure needing machinery.

## Selected minimum correction and consumer closure

| Capability | Existing owner | Required correction / proof |
|---|---|---|
| Group behavior explanation | Existing guide and grouped architecture paragraphs | Qualify their visible-row/null statements with the approved exception; inspect against current runtime and revision 20 |
| Public error reference | Existing docs source generator and generated API page | Regenerate consumed outputs and verify exact source-derived catalog parity |

1. Update the two existing grouping explanations to say: a visible result without
   its execution's input pins nothing and retries; if still incomplete more than
   five minutes past the current due time on PostgreSQL's clock, Wyrd records one
   terminal errored execution with the named code and missing execution IDs,
   consumes pending atomically and performs no evaluation or Operator dispatch.
   New matching source activity can schedule the group again normally.
2. Qualify rather than remove normal null handling: an input without a result
   remains valid, as do absent selected values. Distinguish a missing whole input
   from these cases. State IDs are available in the stored error message; do not
   promise a new persisted details object.
3. Use `mise run docs:generate` to refresh the existing public error reference
   and any consumed generated outputs it actually changes. Keep generation
   source-driven; do not hand-add a parallel error list or change the generator.

Preserve internal snapshot-read authority versus owner execution authority,
ordinary query-error retries, the write-time 365-day duration bound, fixed elapsed
seconds, immutable snapshots and accepted visibility/crash/backlog/overlap/
postponement limits. This task adds no timing option or stronger completeness
promise. The five-minute rule already exists; documentation must describe it.

Do not change runtime, SQL, migrations, auth, SDKs, errors/storage contracts,
fixtures, test harnesses or the specification. The security-posture-only note
was rejected as a blocker; it is not an additional required edit. Do not replace
the accepted real-query DST test with a literal-date test or add migration build
machinery. Benchmarking REQ-015/AC-009 stays pending as subsequent work.

## Acceptance and proof

TDD does not apply: this task changes explanatory prose and generated output,
with no new executable behavior. Do not manufacture RED or add runtime tests.

- Both existing grouping descriptions match the approved retry/deadline/terminal
  outcome, distinguish legitimate nulls, and explain normal fresh-activity scheduling.
- The generated error catalog includes the existing code with status 409, its
  source-derived title and remediation, and exactly matches `render_errors()`.
- No runtime or public/persisted contract changes enter the diff; existing R2
  fixes and accepted tradeoffs remain intact.

Use the existing documentation workflow:

```bash
mise run docs:generate
mise run docs:check
git diff --check
```

The docs check enforces generated-output parity in addition to site checks;
handle finalized generated files through the normal implementation checkpoint.
Do not weaken its drift check to accommodate intentional edits. The existing
read-only catalog comparison recorded in this review's verification.md can
confirm the exact missing-row closure without writing files.

No repeat P1/P2, full SDK/MCP journeys, broad repository gate, format/lint rebuild
or benchmark is required for this documentation-only write set. If the write set
actually expands, reassess the owning proof rather than silently adding behavior.

## Completion evidence and handoff

Append a compact FIND-9 acceptance/evidence table, exact commands/results,
source-to-prose comparison and final diff/scope audit. Return the cumulative
candidate to `$wyrd-task-review` with original TASK-003a, R1/R2/R3 and approved
revision 20. Do not claim integrated change approval or completed benchmarks.

## Implementation evidence (2026-10-19, commit 756e48fc9 from 8fa32190b)

Status: `IMPLEMENTED`. FIND-1–7 and human cleanup remain closed; FIND-8 remains
superseded by revision 19. None were re-implemented.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-9: guide matches revision 20 retry/deadline/terminal outcome, distinguishes legitimate nulls, explains fresh-activity rescheduling | New paragraph after the owner-refusal paragraph in `docs/src/content/docs/understand/how-verification-works.md` | Compared with `Visible::missing_inputs`, `GroupPoller::poll` (Incomplete before pin), `hand_off` (`overdue` arm → `refuse_group`), `DUE_GROUP_SQL` (`statement_timestamp() - due_at > 5 minutes`), message-only stored error | PASS |
| FIND-9: grouped architecture matches the same outcome | `architecture/wyrd-design.md` source-list paragraph qualified (null for missing outcome/absent value; result-without-input retry; >5 min terminal error via refusal path, atomic consume, IDs in message, no details object, normal rescheduling) | Same source comparison | PASS |
| FIND-9: error reference includes the code with source-derived 409/title/remediation and matches `render_errors()` | `docs/src/content/docs/api/errors.md` regenerated by `mise run docs:generate` (one added row; generator unchanged) | `mise run docs:check` exit 0 (generate + drift check + tests + commands + links + build + a11y) after commit | PASS |
| No runtime/public/persisted contract change | Diff limited to the three docs files | `git diff --check` clean; `git show --stat 756e48fc9` | PASS |

Commands: `mise run docs:generate`; `mise run docs:check` (first run before
commit reported drift because its check compares against the committed tree;
exit 0 after committing the regenerated output); `git diff --check`.
The user then requested `mise run gate`; its result is reported separately.
