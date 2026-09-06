---
id: TASK-004
title: Change Request coordination workspace
kind: implementation
status: approved
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

# Locked visual implementation authority

- The mandatory desktop surfaces are `CRW-01` through `CRW-07` in the detailed
  [`changes/`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/changes/)
  package: inbox, new, overview, verification, review, timeline, and subject
  review. `CR-01` through `CR-07` in
  [`changes.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/changes.svg)
  lock their canonical routes, states, links, and responsive patterns.
- `CRWM-01`, `CRWM-02`, and `CRWM-03` in the detailed
  [`changes/mobile.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/changes/mobile.svg)
  lock narrow creation, review, and subject-review behavior; `M-03` and `M-04`
  in the general [`mobile.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/mobile.svg)
  are the matching cross-product shell references.
- [`golden-CR-04.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/golden-CR-04.svg)
  is the immutable Gate A reference for the persistent Change header, dominant
  checks-like Claim/Verifier workflow, raised decision surface, spacing, and
  hierarchy. The detailed package and current [ledger](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/README.md#changessvg)
  supply the later accepted interaction and fixture detail.
- Implement the information hierarchy, major regions, action placement,
  state separation, fixtures, links, and alternate states recorded by those
  artboards. Do not substitute a generic CRUD list, admin form, tab shell, or
  metadata dashboard.
- Completion evidence must compare every implemented route to its named desktop
  artboard in both themes and compare creation, review, and subject review at
  390 × 844. Functional correctness does not excuse a visual-contract mismatch.

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


## Implementation evidence — 2026-09-04

Implemented against approved specification revision 6 and the existing TASK-003
session, tenant, shared server client, and component foundations. The original implementation candidate
was uncommitted and not task-review approved. Existing unrelated workspace edits
were preserved. The required workspace lint lane remains blocked as noted below.

### Ownership and boundaries

The seven routes compose Change-specific components and temporary typed views.
`src/lib/server/changes` owns bounded process-local fixture state, revision
snapshots, action validation, retry identity, and decisions. The existing
`WyrdClient` is the sole BFF composition point. Explicit development permissions
cover read, write, review, run, and override. Mock-disabled calls fail through the
existing upstream error path; they do not silently receive fixtures. No durable
Change protocol, provider fetch, source editor, merge, or deployment was added.

The existing Rust error-example generator now includes the existing validation,
not-found, and conflict catalog variants. Both JSON outputs were regenerated;
no public error definitions or hand-written browser error catalog were added.
Micromark 4.0.2 and its GFM extension 3.0.0 provide the required Markdown parser.
Raw HTML, unsafe URL schemes, and remote image embedding are disabled, with a
rendered-component regression check. Native forms, details, selects, URL state,
and the shared UI components handle the remaining interactions.

### Ordered scenario evidence

| Scenario | Expected RED and resulting GREEN |
| --- | --- |
| 1 — list | Missing list/load contract became URL-restored Open, Needs attention, Verified, and Closed views; search and owner/team/repository/lifecycle filters retain their URL in empty/error states. PR identities support `#4412`. |
| 2 — draft | Save initially returned 404; an incomplete multi-subject draft now saves and resumes through real HTTP actions. Repair resolves the fixture PR to exact commits. Claims project the server Verifier catalog and paid automatic mode remains explicit. |
| 3 — Overview | Missing Action summary became a full-width action summary, intent/impact/owners, exact subjects, required Claims, and separate revision/verification/decision regions. |
| 4 — Verification | The fixture initially contained no check-state coverage; seven checks now separate execution, verdict, Claim resolution, provenance, evidence, eligibility, and override. HTTP tests prove confirmed runs and retry deduplication, and reject missing prerequisites. |
| 5 — Review | Missing Submit review became anchored comments/replies, mentions, immutable edit revisions, expected-revision conflict, author restrictions, resolve/reopen, and independent review decisions. Markdown testing caught an image construct still enabled; disabling `labelStartImage` fixed the rendered regression. |
| 6 — history/source | Missing Timeline returned 404; exact revision links, audit/review event kinds, decisions, source files, commits, provider links, read-only diffs, and revision-aware source discussion now traverse the load/action boundary. |
| 7 — responsive | Browser comparison exposed excessive mobile header spacing and supporting panels preceding the diff. Change-scoped compact spacing, native narrow draft disclosures, file chips, diff-first ordering, and compact composers corrected those layouts. Both themes retain actions and have no horizontal page overflow. |

### Route and visual matrix

Desktop captures use 1440 × 1024. Comparisons place the named approved artboard
on the left and the implementation on the right. Narrow comparisons use the
390 × 844 artboards. These are inspection evidence, not a claim of pixel identity
or independent review approval. Native editable controls, the inherited identity
bar and DEV overlay, and full decision labels use more vertical space than the
static mocks; supporting content remains reachable by scrolling. Alternate-state
annotations in the SVG are actual route/action states, not permanent panels.

| Route under `/t/acme/changes` | Authority | Paired comparison |
| --- | --- | --- |
| `?view=needs-attention` | CRW-01 | [light](../evidence/TASK-004-compare-inbox-light.png), [dark](../evidence/TASK-004-compare-inbox-dark.png) |
| `/new` | CRW-02 | [light](../evidence/TASK-004-compare-new-light.png), [dark](../evidence/TASK-004-compare-new-dark.png) |
| `/change_01` | CRW-03 | [light](../evidence/TASK-004-compare-overview-light.png), [dark](../evidence/TASK-004-compare-overview-dark.png) |
| `/change_01/verification` | CRW-04; Gate A hierarchy | [light](../evidence/TASK-004-compare-verification-light.png), [dark](../evidence/TASK-004-compare-verification-dark.png) |
| `/change_01/review` | CRW-05 | [light](../evidence/TASK-004-compare-review-light.png), [dark](../evidence/TASK-004-compare-review-dark.png) |
| `/change_01/timeline` | CRW-06 | [light](../evidence/TASK-004-compare-timeline-light.png), [dark](../evidence/TASK-004-compare-timeline-dark.png) |
| `/change_01/subjects/subject_api` | CRW-07 | [light](../evidence/TASK-004-compare-subjects-light.png), [dark](../evidence/TASK-004-compare-subjects-dark.png) |
| `/new` at 390 | CRWM-01 | [light](../evidence/TASK-004-compare-new-mobile-light.png), [dark](../evidence/TASK-004-compare-new-mobile-dark.png) |
| `/change_01/review` at 390 | CRWM-02 | [light](../evidence/TASK-004-compare-review-mobile-light.png), [dark](../evidence/TASK-004-compare-review-mobile-dark.png) |
| `/change_01/subjects/subject_api` at 390 | CRWM-03 | [light](../evidence/TASK-004-compare-subjects-mobile-light.png), [dark](../evidence/TASK-004-compare-subjects-mobile-dark.png) |

[Browser measurements](../evidence/TASK-004-browser-metrics.json) record all 20
route/theme/width combinations. Full-size implementation captures accompany the
comparisons. Chrome also exercised PR repair, save/resume, stale-edit draft
recovery, billable-run cancellation, source discussion, and keyboard Menu
Enter/Escape with focus restoration. Page overflow was checked at narrow width.

### Fixture-state ledger and action outcomes

- `change_01`: revision 7, three subjects across two repositories; one of three
  required Claims satisfied, missing ranking Evidence, failed PII review, and a
  recorded override that does not verify. Prior revision views disable actions.
- Checks cover not-run, queued, running, completed, passed, failed, current,
  stale, and carried-forward. Detail histories also cover cancelled, timed-out,
  errored, and inconclusive attempts. Current required-Claim progress is server
  projected; illustrative mock aggregate counts are not fabricated from the
  seven visible checks.
- `change_02`: verified with author changes requested; `change_03`: incomplete
  draft; `change_04`: open/verified; `change_05`: closed/cancelled. Verification,
  approval, lifecycle, and override remain distinct during mutation.
- Save returns a redirect to the saved draft; PR repair preserves form data;
  invalid input uses the existing 400 catalog error. Historical/stale writes use
  409; tenant/permission violations use 403; unavailable records use 404.
- Running requires current revision, authorization, prerequisites, and explicit
  confirmation. Accepted work becomes queued and cannot be started twice by a
  retry. Completed attempts remain available in history.
- Review retries retain stable comment identity; edits append immutable versions.
  A stale edit retains its text and offers recovery against the current revision.
  Resolution changes append history; overriding/closing never modifies verifier
  verdicts or source content. Timeline distinguishes Audit from Review activity.

### Verification results

The exact commands listed in the task were run: ChangesJourney **7 passed**,
verification-state **1 passed**, review-actions **5 passed**, full UI suite
**108 passed in 23 files**, Svelte check **0 errors / 0 warnings**, production
build **passed**, and `mise run check:tokens` **passed**.

Additional required checks:

```bash
mise run codegen:check
mise run fmt
mise run lints
mise exec -- cargo clippy --locked -p wyrd-spec --example gen_schemas --all-features -- -D warnings
git diff --check
```

Code generation, formatting, focused generator Clippy, and whitespace checks
passed. **Workspace `mise run lints` failed** on the unchanged
`crates/shared/wyrd-client/src/transport/http.rs:600` (`clippy::question_mark`).
That code is identical in HEAD; it was not modified or suppressed. Consequently
the mandatory workspace lint result is not green. Final tracked/untracked diff
inspection found only the intended UI, generator outputs, and task evidence in
this task's changes; no commit was created.


## Self-review and commit — 2026-09-05

[Review verdict: REMEDIATE](../evidence/TASK-004-review.md). Fixed draft metadata
and revision history through TASK-004-R1. Narrow visual fidelity and seeded
draft consistency remain open findings; this task is not approved. The user
authorized committing the reviewed implementation.


## R2–R5 implemented — 2026-09-05

All four user-validated findings have implementation and closure evidence in their
remediation packets. New HTTP regressions cover seeded-draft preservation,
side-qualified diff anchors and removal of the fabricated Verifier link. New
paired browser captures prove the compact narrow Review/Subject regions in both
themes. Full UI verification passes 111 tests, typing, build and token checks.
The original review and failed screenshots are preserved for comparison.


## Cumulative review recorded — 2026-09-05

[APPROVE](../evidence/TASK-004-remediation-review.md) for the original base through
`7a4d76ecd1be75f030ee0ac416b60cd94058d4c4`. All five finding IDs are closed.
This task approval does not complete the broader change or authorize deployment.


## User-directed visual revisions after approval — 2026-09-06

Three commits after the approved candidate revise the workspace's visual
treatment at the user's direction, following two live-browser design audits:

- `c7c67548` — GitHub-style panel anatomy applied globally; Change workspace
  revisions consolidated.
- `ad85e99b` — audit revisions (dark hard shadows, `accent` panel variant,
  Verification status washes, lime Observe numeral, interaction states, type
  hierarchy) and retirement of the Fathom ownership machinery: lime is now the
  plain secondary accent, applied to verifier run and revision actions.
- `e7bc19f1` — theme mode persisted in a `wyrd-mode` cookie so SSR paints the
  chosen theme without a hydration repaint.

These revisions intentionally supersede the Fathom-era treatment in the locked
artboards (`CRW-01`–`CRW-07`, `golden-CR-04.svg`): the artboards remain the
authority for information hierarchy, regions, routes, states, and action
placement, but panel accents, secondary-action colour, and Fathom ownership
markers now follow the updated `brand/` authority (`DESIGN.md`, `palette.json`,
`components.json`) committed in `ad85e99b`. Change review should evaluate
visual conformance against that current brand authority, not against the
superseded artboard styling. Verification after each commit: Svelte check
0 errors / 0 warnings, 112 UI tests passed, production build passed.
