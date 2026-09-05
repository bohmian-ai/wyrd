---
id: TASK-004-R2
title: Restore narrow Review and Subject composition
kind: remediation
status: approved
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-082, REQ-132, REQ-064, REQ-065, INV-008, INV-012, INV-022]
depends_on: [TASK-004]
parent_task: TASK-004
remediates: [FIND-TASK-004-2]
---

# Restore narrow Review and Subject composition

Closes validated MAJOR finding `FIND-TASK-004-2` under the approved spec.

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

Restore the accepted CRWM-02/03 regions at 390 × 844 in both themes: compact
identity/status header, two compact review threads and Submit review; source
identity and file navigation followed by the diff around the accepted 220px
region, discussion and anchored threads, with the sticky Review action.

Own `src/lib/features/changes/{ChangeHeader,Review,Composer}.svelte`,
`changes.css`, and `src/routes/t/[tenantKey]/changes/[id]/subjects/[subjectId]/+page.svelte`.
Touch `src/lib/components/app/Shell.svelte` only if the inherited chrome is the
remaining source of excess height, preserving other routes and its existing
accessible menu. Extend existing shell tests if that shared component changes.
No mock-state changes, new shell framework, arbitrary font shrinking, or hiding
required information to make a screenshot fit.

## Implementation constraints

Read [brand doctrine](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/DESIGN.md)
and inspect `brand/palette.json`, `components.json`, and the detailed
`brand/renders/product/changes/mobile.svg` CRWM-02/03 groups. The original task's
approved mobile navigation exception wins over the generic no-hamburger rule.
Keep desktop CRW-05/07 and the shared header's Gate A hierarchy intact.
Use existing controls, compact spacing, and progressive disclosure for supporting
history; retain identity, permissions, all state channels and actions. Keep diff
scrolling inside its region. Reorder workflow regions rather than merely
squeezing the entire desktop stack. Preserve DOM/keyboard reading order; avoid
visually moving focusable regions into a conflicting order.

## Ordered Red-Green-Refactor scenarios

1. Review: capture the current fresh primary fixture at 390 × 844; use the
   supplied comparison as credible RED. Restore the compact header and review
   composition so two thread summaries and Submit review occupy the accepted
   regions. Exercise reply, edit-history, resolve/reopen and review submission;
   expanding detail must preserve access to the full discussion.
2. Subjects: reproduce the diff starting near 600px. Restore the CRWM-03 order
   and approximate accepted positions, including file chips, exact commits,
   in-place horizontal diff scroll, line discussion and sticky Review action.
   Verify provider and thread navigation plus keyboard reachability.
3. Capture both routes in light and dark at 390 × 844 (four captures and four
   paired comparisons). Measure region top/bottom coordinates, not just overflow.
   Compare against the named artboards; record justified native-control differences
   without redefining their required regions. Check both desktop routes at
   1440 × 1024 and creation at 390 to detect shared-header/shell regressions.

Do not fabricate a test that merely checks CSS declarations. Browser geometry,
visual comparisons and real interaction are the proof for placement; retain
functional HTTP coverage. Use a browser with fonts loaded and hydration complete.
Start the existing local-auth/mock dev server through:

```bash
WYRD_UI_LOCAL_AUTH=true WYRD_UI_MOCK_DATA=true mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui dev --host 127.0.0.1 --port 3014
```

Use an available port if occupied, record it, sign in through the normal mock
SSO form, and use `/t/acme/changes/change_01/review` and
`/t/acme/changes/change_01/subjects/subject_api?file=src/capture/rank.rs`.
Capture a fresh fixture state, without hiding application/DEV controls for evidence.
Store evidence with `TASK-004-R2-` filenames; preserve the failed original captures.

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

The supplied failing comparisons were the RED evidence. Review now shows two
compact threads before Submit review, with further threads and full revision /
authorization information behind operable disclosures. Subject navigation and
commit history disclose in place; file chips precede a read-only diff, its composer,
and anchored threads in DOM reading order. Desktop keeps the existing workbench.

Browser evidence also exposed shared Vite optimization-cache pollution: Vitest's
server-resolved Svelte MediaQuery was served to the dev browser. Separated the
test cache from the browser cache in the existing Vite config. Rechecking after
tests confirmed browser MediaQuery behavior. Bound native disclosure state
explicitly so SSR fallback does not leave narrow disclosures expanded; creation
was included in regression captures. No additional dependency or app endpoint.

At 390 × 844, automated browser assertions require two review threads, the full
submission panel below 800px and its button above the DEV overlay, source panel
top below 240px, discussion below 720px and anchored threads below 740px. Both
themes pass. The actual source panel begins at about 210px, versus the prior
~600px placement; the submission panel ends at 798px. These measurements and
all comparisons use fresh fixtures, loaded fonts and completed hydration.

[Measured regions](../evidence/TASK-004-R2-browser-metrics.json).
Paired mobile comparisons:
[Review light](../evidence/TASK-004-R2-compare-review-mobile-light.png),
[Review dark](../evidence/TASK-004-R2-compare-review-mobile-dark.png),
[Subjects light](../evidence/TASK-004-R2-compare-subjects-mobile-light.png),
[Subjects dark](../evidence/TASK-004-R2-compare-subjects-mobile-dark.png).
Desktop comparisons and 390px creation regression captures have the same R2 prefix.
Visually inspected both desktop routes and all narrow captures in both themes.
Keyboard menu/disclosure controls, focus restoration, review submission and source
line discussion passed in Chrome. Native form submission and enhanced actions work.

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
