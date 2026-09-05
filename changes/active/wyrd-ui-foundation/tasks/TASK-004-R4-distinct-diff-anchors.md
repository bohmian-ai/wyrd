---
id: TASK-004-R4
title: Keep old and new source-row discussions distinct
kind: remediation
status: approved
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-064, REQ-065, REQ-099, INV-013, INV-014]
depends_on: [TASK-004]
parent_task: TASK-004
remediates: [FIND-TASK-004-4]
---

# Keep old and new source-row discussions distinct

Closes validated MODERATE finding `FIND-TASK-004-4` under the approved spec.

## Authority and boundaries

- [Approved spec revision 6](../spec.md) and [original TASK-004](TASK-004-change-request-workspace.md).
- User-supplied validated review of `027fa942a16626fef2c3a0788bca7c1cf05ee7e8..4f32eefe22e7d55d71bf1465ffaa6b9eec234317`;
  [preserved review handoff](../evidence/TASK-004-remediation-handoff.md).
- [AGENTS.md](../../../../AGENTS.md), [Wyrd design](../../../../architecture/wyrd-design.md),
  [doctrine](../../../../architecture/wyrd-doctrine.mdx), and
  [spec-driven development](../../../../architecture/references/languages/spec-driven-development.md).
- UI paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui`.
  Temporary projections stay behind the existing server-only WyrdClient.
  Preserve tenant binding, CSRF/origin validation, authorization, immutable
  revisions and retry semantics. No durable protocol or new dependency.


## Outcome and ownership

Deletion line 1 and addition line 1 in `subject_model` have distinct stable
coordinates, unique DOM IDs and independent discussion targets. Own the shared
mutable source-anchor seam: `src/lib/features/changes/types.ts`, source-row and
thread fixtures, `src/lib/server/changes/store.ts` validation, action parsing as
needed, the subject page, Composer and Review anchor links. Extend the existing
HTTP journey and `review-actions.test.ts` for this behavior. No durable Card or
Wyrd protocol changes, source editing or compatibility layer.

## Implementation constraints

Use a coordinate `(side, line)` within the existing revision/subject/file anchor,
with explicit `old` / `new` side. Deletions belong to old, additions belong to
new; context rows use one documented canonical side (new). Keep the row
coordinate server-projected and stable, not an array index, displayed text,
client-generated UUID or line number alone. Model the required source coordinate
so a source action cannot omit its side. Update every seeded source anchor.

Validate the complete coordinate against the exact revision's subject/file rows,
including side membership. Reject missing/invalid sides and nonexistent rows;
do not silently map ambiguous line-only requests. There is no persisted data
migration for this process-local mock. Give DOM targets side-qualified IDs and
update fragment links, selection state, Composer input, thread matching and
Review deep links together. Retain no side parameter on unrelated anchor kinds.

## Ordered Red-Green-Refactor scenarios

1. Load the real model subject diff with equal-number deletion/addition rows.
   Require unique side-qualified DOM IDs and distinct source-action coordinates;
   current output fails RED. Update projection/rendering and all seeded anchors
   to make this green without removing either row.
2. Post one comment on each row through the real HTTP action. Reload source and
   Review; prove distinct stored coordinates, each thread only on its own row,
   and each Review deep link returning to the intended DOM target. Switching
   files/revisions must not retain the previous selected coordinate or draft.
3. Submit a missing side, invalid side, nonexistent coordinate and a coordinate
   from another subject/revision; assert rejection with no new thread or partial
   mutation. Retain existing author/tenant/CSRF and retry checks. Use a focused
   store test for immutable identity/retry assertions where clearer, while
   keeping cross-boundary acceptance/rejection in the HTTP journey.

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/changes/ChangesJourney.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/changes/review-actions.test.ts
```

Browser-check both row buttons, keyboard activation, thread navigation and file
switching. Any change to source presentation requires refreshing affected R2
comparisons on the final cumulative candidate.

## Verification and handoff

Run the focused commands above, then:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
git diff --check
```

Use the existing formatter/toolchain and inspect tracked and untracked diffs.
No Rust/schema source is expected to change; if scope legitimately enters that
boundary, add its canonical format/lint/codegen checks under AGENTS.md. The
previous unrelated workspace Clippy failure is evidence limitation, not authority
to weaken a gate. Do not require the broad `gate` lane for these bounded UI edits.

Record each scenario's observed RED, GREEN, any refactor, exact command results,
and evidence links in this task. Do not mark the parent approved. After all
remediations integrate, review the original base through the cumulative candidate,
including R1 and all new tasks. Refresh visual evidence if another task changes
what it depicts; overlapping files do not create a behavioral dependency.

Use `wyrd-task-readiness` before implementation; execute with `wyrd-implement`
and `wyrd-ui`. Stop with `SPEC_REVISION_REQUIRED` only if fixing the finding
would change approved behavior, durable contracts, tenancy, or architecture.


## Execution evidence — 2026-09-05

Readiness: READY — no blocking readiness findings. Approved revision 6,
validated finding coverage, cohesive owners, real dependencies, ordered TDD,
exact existing test targets and browser evidence are sufficient; no material
decision is hidden. Executed with wyrd-implement and wyrd-ui.

RED: equal-number deletion/addition rows had no distinct DOM coordinates.
Source anchors now require side, file and line within revision/subject; server
fixture rows carry old/new explicitly (context uses new). Validation checks the
complete coordinate before a thread is created. Source selection, Composer,
thread matching, DOM IDs and Review links all use the side-qualified coordinate.

GREEN: the real HTTP journey posts separate old/new line-1 discussions, retries
each without duplicates, and proves each row has exactly its own thread and
Review destination. Missing/invalid side, nonexistent line/file, another subject
and stale revision are rejected without a partial thread. Existing source tests
now submit the required side. Chrome exercised both line buttons and submissions;
client-side file-link navigation clears the old coordinate's unsaved composer.
No durable protocol, migration or compatibility alias was added.

Verification: exact ChangesJourney command passes 10 tests; exact review-actions
passes 5; exact verification-state passes 1. Full UI suite: 111 tests / 23 files
passed. Svelte check: zero errors / warnings. Production build, check:tokens and
whitespace checks pass. Original Rust codegen/format evidence remains applicable;
no Rust or generated source changed. The previously documented unrelated workspace
Clippy warning remains a limitation, not a remediated TASK-004 defect.


## Cumulative review recorded — 2026-09-05

[APPROVE](../evidence/TASK-004-remediation-review.md) for the original base through
`7a4d76ecd1be75f030ee0ac416b60cd94058d4c4`. All five finding IDs are closed.
This task approval does not complete the broader change or authorize deployment.
