---
id: TASK-004-R3
title: Preserve the seeded ledger draft on resume and save
kind: remediation
status: review
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-062, REQ-099, INV-009]
depends_on: [TASK-004]
parent_task: TASK-004
remediates: [FIND-TASK-004-3]
---

# Preserve the seeded ledger draft on resume and save

Closes validated MODERATE finding `FIND-TASK-004-3` under the approved spec.

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

Resuming and saving `change_03` without edits preserves its ledger intent,
impact, owner/teams, subjects and Claims. Own `src/lib/server/changes/fixtures.ts`
and the existing `src/lib/features/changes/ChangesJourney.test.ts`; inspect the
store and DraftForm consumer but change them only if the new journey proves an
additional defect within this round-trip. Leave the separately authored `/new`
checkout example unchanged.

## Implementation constraints

Construct the seeded draft from the same record-owned inputs used for its
Overview; do not spread the unrelated checkout `newDraft` and repair only its
title. Preserve exact subject identities/commits and Claim/Verifier requirements.
Do not maintain two independent ledger fixtures or introduce a draft registry.
Keep intended incomplete fields incomplete. Save may advance revision, timestamps
and draft history; it must not silently replace the user's content.

## Ordered Red-Green-Refactor scenarios

1. Extend the real HTTP journey: sign in, load `change_03`, follow Resume draft,
   and compare the rendered editable intent, impact, owner/teams, exact subjects
   and Claims to its detail projection. Existing fixtures should fail RED because
   the editor supplies checkout/ranking content. Change the fixture construction
   minimally until that boundary is green.
2. Submit those unchanged form values through the real save action, including
   actual CSRF, expected revision and request key. Reload detail, resume again,
   and verify all content fields and identities remain ledger-owned. Verify the
   previous revision remains unchanged and retry does not create a second save.
   Exempt only expected save-generated revision/activity/history fields.
3. Rerun the existing newly-created-draft journey and primary fixture checks so
   the fix does not alter the `/new` example or primary Change. Refactor only
   duplication revealed by those green scenarios.

Use actual form/projection values in the journey, not an independently supplied
correct ledger payload which would bypass the defective resume step. Supporting
pure fixture checks may clarify equality but cannot replace this HTTP journey.

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/changes/ChangesJourney.test.ts
```

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

RED: the real rendered ledger draft form supplied checkout intent instead of
"Split ledger write path". Build the seeded Draft from its own complete Change
projection; the unrelated /new checkout example stays separate. The HTTP journey
reads actual form controls with DOMParser/FormData, compares intent, impact,
ownership, subjects and Claims with the displayed record, posts those unchanged
values, retries, and resumes again. GREEN: values remain identical, revision is
8 (not 9 after retry), and historical revision 7 remains ledger-owned. JSON field
comparison uses semantic equality, not irrelevant object-key serialization order.
The existing new-draft journey still passes. No persistence/API change.

Verification: exact ChangesJourney command passes 10 tests; exact review-actions
passes 5; exact verification-state passes 1. Full UI suite: 111 tests / 23 files
passed. Svelte check: zero errors / warnings. Production build, check:tokens and
whitespace checks pass. Original Rust codegen/format evidence remains applicable;
no Rust or generated source changed. The previously documented unrelated workspace
Clippy warning remains a limitation, not a remediated TASK-004 defect.
