---
id: TASK-004
title: Change Request coordination workspace
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-005, REQ-006, REQ-007, REQ-008, REQ-060, REQ-061, REQ-062, REQ-063, REQ-064, REQ-065, REQ-081, REQ-082, REQ-092, REQ-095, REQ-099, REQ-100, REQ-101, REQ-127, REQ-128, REQ-132, INV-001, INV-002, INV-008, INV-009, INV-012, INV-013, INV-014, INV-021, INV-022, AC-001, AC-003, AC-004, AC-005, AC-010, AC-011]
depends_on: [TASK-003]
parent_task:
remediates: []
---

# Outcome and value

Deliver the first product workspace: a GitHub-familiar Change Request flow in
which Product, Data Science, and Engineering can understand intent, exact source
subjects, Claims, verification, review discussion, and immutable history.

# Owner and write set

- Own `/t/[tenantKey]/changes/**`, `src/lib/features/changes/**`, and its typed
  mock projections and server actions.
- Implement the searchable pull-request-style list; one saveable progressive
  draft form; Overview, Verification, Review, Timeline, and subject drilldown.
- Support multiple subjects/repositories, required and advisory Verifiers,
  Evidence, results, provenance, manual/automatic modes, and authorized override.
- Review supports stable threads/comments, immutable edits, anchors, mentions,
  replies, stale-edit rejection, resolve, and reopen.

# Locked decisions and non-goals

- Plain intent, impact, ownership, Claims, and decision state lead; digests,
  runs, cases, commits, and diffs progressively disclose.
- Lifecycle, execution, verdict, Claim resolution, provenance, and authorization
  stay visibly separate. Override never looks Verified.
- No durable protocol/persistence, Git provider, source editing, merge/deploy
  action, global Inbox, verifier marketplace, or parallel provider review UI.

# Ordered test scenarios

1. List search and Open/Needs attention/Verified/Closed filters restore from URL.
2. An incomplete draft saves and resumes, then accepts multiple subjects,
   Claims, Verifiers, and a visible automatic-run cost warning.
3. Overview makes what/why/impact/owners and exact revision subjects scannable.
4. Verification distinguishes all lifecycle, execution, verdict, Claim,
   provenance, missing-Evidence, and override states in representative fixtures.
5. Review exercises anchored reply/mention/edit/conflict/resolve/reopen behavior.
6. Timeline and subject drilldown expose exact commits, files, read-only diff,
   provider link, Evidence, decisions, overrides, and coordination events.
7. Narrow and dark/light presentations retain primary actions and meaning.

# Red-Green-Refactor

Implement one journey slice at a time through the real load/action boundary.
Reuse TASK-002 components but keep Change-specific semantics under this feature.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/changes/ChangesJourney.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/changes/verification-state.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/changes/review-actions.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

# Evidence and stop conditions

Record the route/state matrix, fixture-state ledger, action results, and paired
responsive comparisons to the accepted Changes mocks. Stop before temporary
view models become durable Wyrd contracts or UI-derived verification truth.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.
