---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-software-factory
spec_revision: 2
spec_status: approved
requirements: [REQ-004, REQ-005, REQ-006, REQ-011, INV-002, INV-003, AC-003, AC-004, AC-005]
depends_on: [TASK-001]
---

# Run implementation, independent review, and remediation automatically

## Outcome and Value

The maintainer starts a packet once. The factory implements its next eligible
task, checkpoints the candidate, launches independent review, and forwards the
actual remediation file until review passes or needs a decision. The UI shows
the current diff, evidence, verdict, and commit-specific CI results. No copied
prompts or replacement review policy are needed.

## What Changes and What Must Stay True

Extend TASK-001's service/session owner. Reuse the exact existing skill results,
packet paths, native integrations, and decisions. Preserve implementation
verification and the review skill's own independent subagent topology.
One packet worktree has one active owner. Checkpoint permission is explicit;
no pushes, merges, external checkout resets, or silent adoption of user edits.
Do not add another acceptance matrix or run a second review engine.

## Approach

1. Create the packet branch/worktree from the chosen clean committed base.
2. Dispatch original tasks in dependency order using existing skill sessions.
3. Stop implementation, checkpoint, and supply immutable review identities.
4. Validate result/artifact identity and route PASS, remediation, or decisions.
5. Expose cumulative diff, evidence, audit history, usage, and read-only CI.
6. Preserve configured final-review/completion results and pending-final state.

## Ordered Implementation Scenarios

### Scenario 1 — Complete a task through a real remediation handoff

**Behavior.** A packet's first eligible task returns IMPLEMENTED, its independent
review returns FIX_REQUIRED with a remediation file, implementation consumes
that file, and review PASS unlocks the dependent task. Review receives the
original task plus prior/cumulative candidate context, never the implementor's
session as its own review context. No intermediate completion claim advances it.

**RED.** Add the planned automatic-handoff browser journey against the real
service, SQLite, temporary Git, and scripted native processes. Assert invocation
inputs, distinct native review session IDs, actual remediation path, ordering,
and persisted verdict transitions.

**GREEN.** Implement the minimum skill-result routing and candidate handoff.
Keep the original task as acceptance context throughout remediation.

**REFACTOR.** Reuse session/decision persistence from TASK-001; a remediation is
an existing task file, not another planner or workflow type.

### Scenario 2 — Refuse an ambiguous or changed review subject

**Behavior.** A dirty source checkout is not reset. The maintainer explicitly
authorizes factory-branch checkpoints. Existing branches/worktrees are not
overwritten. Review starts only after implementation stops and records exact
base/candidate commits. A changed source, stale candidate, missing result file,
escaped path, or contradictory envelope cannot advance work. Review can write
only its designated report artifacts without altering the source subject.

**RED.** Add the planned candidate-integrity test using real Git. Exercise each
refusal and assert the original checkout, partial work, and last valid verdict
are preserved. Include a source write during review.

**GREEN.** Validate ownership, candidate identity, and result/artifact paths
at transition boundaries. Surface the specific blocker without guessing a fix.

**REFACTOR.** Use Git's existing commit/worktree semantics and the skill's
identity fields; do not archive or reconstruct a second source snapshot.

### Scenario 3 — Preserve blocker and final-review semantics

**Behavior.** BLOCKED exposes the question and stops dispatch. A required spec
revision remains unapproved until the maintainer approves the changed content;
the affected task then reruns rather than inheriting a stale PASS. All task
PASS results show final-review-pending unless the configured existing final
workflow passes. If final completion removes the active packet, retain its
completion reference and avoid treating that expected removal as corruption.

**RED.** Add the planned verdict-boundary test covering all review statuses,
spec revision, absent final workflow, final failure, and successful completion
that removes active files. Assert no merge/push occurs.

**GREEN.** Route existing statuses through service decisions and final-session
invocation. Keep task success and integrated approval separate.

**REFACTOR.** Use existing skills for semantic revision and final review;
do not encode their internal approval or completion logic in the factory.

### Scenario 4 — Inspect evidence and CI without inventing acceptance

**Behavior.** The browser shows the candidate diff, actual implementation and
review reports, handoffs, usage, and explicit read-only GitHub check refresh for
a selected commit. Success, failure, pending, no result, missing gh/auth, and
results for another commit are distinct. No CI state or cost display changes
the skill verdict. Unknown cost is labeled unknown/estimated, never measured.

**RED.** Add the planned inspection journey using fixture gh responses and
real local artifact files. Assert commit association, all states, and unchanged
task acceptance after refresh. A credentialed real GitHub read supplies the
external-service smoke evidence without posting or pushing anything.

**GREEN.** Add views and the small read-only gh boundary. Local commits without
remote checks remain inspectable without requiring publication.

**REFACTOR.** Read existing report files directly and share the service's event
stream; do not introduce a CI scheduler, webhook server, or test evaluator.

## Acceptance Criteria

| Obligations | Observation and proof |
|---|---|
| REQ-004, INV-002, AC-003 | The handoff journey routes the real remediation path and advances only on independent task PASS; verdict-boundary cases retain blockers/final-review distinction. |
| REQ-005, INV-003, AC-004 | Real Git integrity cases preserve work and reject mutated/stale candidates, external edits, and invalid artifacts before transition. |
| REQ-006, AC-005 | Browser inspection shows real diff/reports and commit-specific CI states, including unavailable/stale checks, without granting approval. |
| REQ-011 (handoffs/verdict audit) | Every tested transition records its attempt, candidate, verdict, and event together; the browser can inspect that history. |

## Implementation Reference

Existing owners: wyrd-implement's obligations/verification/result contract and
wyrd-task-review's Establish the subject, remediation scope, and Verdict and
remediation task sections. Their IMPLEMENTED/PASS distinction and
review-produced remediation are reused verbatim. Spec-driven development owns
task dependencies, approval, and final completion; the factory owns only their
execution mechanics.

TASK-001 introduces the native-session service, packet loader, SQLite, browser,
and decisions needed here. Inspect its actual implementation, callers, and
tests before extending them; those symbols do not exist at planning time.
No existing production Wyrd agent runtime should be reused for contributor
orchestration: its product boundary and durable contracts differ. Git already
owns immutable commits; gh already owns GitHub authentication and read access.
Likely changes remain inside tools/wyrd-factory and its focused commands/tests.

## Verification and Evidence

Names below are **planned**. TASK-001 supplies Vitest/Playwright setup and mise
entrypoints; confirm selectors before RED. Each journey uses the real local
service, SQLite, Git, and browser; scripted native peers cover routing faults.

| Proof | Exact focused command |
|---|---|
| Automatic handoff | `mise exec -- pnpm --dir tools/wyrd-factory exec playwright test tests/journeys/execution.spec.ts --grep '^implements reviews remediates and advances a packet$'` |
| Candidate integrity | `mise exec -- pnpm --dir tools/wyrd-factory exec vitest run tests/integration/candidate.test.ts -t '^rejects unsafe or changed candidate subjects$'` |
| Verdict boundaries | `mise exec -- pnpm --dir tools/wyrd-factory exec vitest run tests/integration/routing.test.ts -t '^preserves blockers spec approval and final review boundaries$'` |
| Inspection and CI | `mise exec -- pnpm --dir tools/wyrd-factory exec playwright test tests/journeys/inspection.spec.ts --grep '^shows candidate evidence and commit specific CI without approving work$'` |

Extend factory:smoke:live with one bounded real implementation/review/remediation
packet using both installed harnesses and the repository skills. Invoke with
`mise run factory:smoke:live -- --packet` only under explicit paid-smoke
authorization. Add a read-only GitHub smoke invocation
`mise run factory:smoke:github -- --repo <owner/repo> --commit <sha>` using a real
accessible commit; concrete evidence records the supplied repository/commit.
These two invocation forms are planned contracts implemented by this task.

Run focused proofs, factory:format:check, factory:lints, factory:typecheck,
factory:build, and git diff --check. Final full packet verification is performed
once by change review. Missing live evidence remains explicitly unverified.

## Material Stop Conditions

Stop if routing needs new skill statuses, if native review cannot preserve
required fresh delegation, if Git checkpointing would require discarded user
changes, or if the solution would need automatic pushes/merges or altered
verification semantics. Ordinary path/layout and SDK choices remain local.

## Authority Links

- [Specification](../spec.md)
- [Dependency](TASK-001-local-skill-sessions.md)
- [Implementation skill](../../../../.agents/skills/wyrd-implement/SKILL.md)
- [Review skill](../../../../.agents/skills/wyrd-task-review/SKILL.md)
- [Spec-driven development](../../../spec-driven-development.md)
