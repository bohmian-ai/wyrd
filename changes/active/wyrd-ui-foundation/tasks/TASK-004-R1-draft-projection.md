---
id: TASK-004-R1
title: Preserve draft-owned metadata
kind: remediation
status: ready
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-060, REQ-062, REQ-065, REQ-099, INV-009]
depends_on: [TASK-004]
parent_task: TASK-004
remediates: [FIND-TASK-004-1]
---

Restore truthful metadata after draft saves. Own only the mock Change store and
its existing journey test. Do not add durable persistence or change contracts.

Readiness: bounded by the approved draft workflow, existing server action and
fixture owner; no spec decision is needed. Use wyrd-implement and wyrd-ui.

Ordered scenario: save a draft with its own team and owner; verify its detail and
filtered list project that team, an actual save event, and no inherited ledger
service or stale activity. A later save retains earlier revision history.

RED: extend the existing draft HTTP journey to require a draft revision event
and absence of inherited ledger metadata. GREEN: initialize draft-owned summary
metadata and append revision events in the existing save operation. Preserve
immutable snapshots and retries. No unrelated refactor.

Verification:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/changes/ChangesJourney.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
git diff --check
```

Stop before changing approved behavior. Record results in the review evidence.

Execution: the HTTP journey failed on the missing saved-revision event (RED),
then passed with the revision event and submitted-team filter assertions
(GREEN). All 108 UI tests, Svelte check, build, and whitespace checks pass.
No unrelated refactor. Closure evidence is in TASK-004-review.md.
