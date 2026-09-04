---
id: TASK-001-R1
title: Remediate product mock composition and drill-downs
kind: remediation
status: complete
spec: SPEC-wyrd-ui-foundation
spec_revision: 1
requirements: [REQ-004, REQ-005, REQ-060, REQ-061, REQ-062, REQ-063, REQ-064, REQ-066, REQ-068, REQ-071, REQ-072, REQ-076, REQ-081, REQ-082, REQ-084, REQ-087, REQ-090, REQ-096, REQ-097, REQ-098, INV-008, INV-012, AC-003, AC-004, AC-005, AC-007]
depends_on: [TASK-001]
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4]
---

# Outcome

Replace TASK-001's rejected generic mock composition with an explicitly
approved Wyrd product language. First prove that language in three golden
desktop screens. Stop for human approval. Only then propagate the approved
composition and interaction patterns through the complete desktop/mobile,
light/dark mock inventory.

The result must recover the visual energy, hierarchy, and operational clarity
of the canonical Wyrd Workbench while preserving Wyrd routes, vocabulary,
redacted
styling.

# Human value

- Product, Data Science, and Engineering users can understand a Change Request
  and its current decision without decoding implementation tables.
- Evaluation users can move naturally from an Eval event to a workflow and its
  tasks, then inspect one task with enough space for meaningful evidence.
- Operational users receive the familiar search-to-inspection flow expected of
  an observability workbench without every Wyrd page looking interchangeable.
- The approved SVGs become a credible visual contract for later Svelte work.

# Authority

Read these before changing an SVG:

1. `changes/active/wyrd-ui-foundation/spec.md`
2. `changes/active/wyrd-ui-foundation/tasks/TASK-001-route-mapped-svg-mocks.md`
3. `changes/active/wyrd-ui-foundation/reviews/TASK-001-visual-acceptance.md`
4. `crates/wyrd/wyrd-server/wyrd-ui/brand/DESIGN.md`
5. `crates/wyrd/wyrd-server/wyrd-ui/brand/palette.json`
6. `crates/wyrd/wyrd-server/wyrd-ui/brand/theme.css`
7. `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/styleguide.html`
8. `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/workbench.html`

Focused interaction references:

- Change Request composition:
  `/Users/stevenforrester/Documents/GitHub/agent-workflows/wyrd/active/verified-changes-trust-layer-v3-20260825/partner-mockup.html`
- OpsML workflow selection and drawer:
  `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/lib/components/card/agent/evaluation/AgentEvalWorkflowTable.svelte`
  and
  `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/lib/components/scouter/agent/workflow/AgentEvalWorkflowSideBar.svelte`
- OpsML workflow/task split view:
  `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/lib/components/scouter/agent/workflow/AgentEvalWorkflowContent.svelte`
- OpsML Eval record drawer:
  `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/lib/components/scouter/agent/record/EvalRecordSideBar.svelte`
redacted
  TASK-001.

The references demonstrate hierarchy and interaction. Wyrd's brand directory
remains the sole visual authority.

# Ownership and write scope

Own only:

- `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/*.svg`
- `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/README.md`
- the existing product-mock links in
  `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/index.html`, if filenames or
  descriptions require correction

Do not modify TASK-001, its review, the approved specification, brand tokens,
Svelte code, BFF routes, authentication, Nginx, or server code.

# Non-goals

- No new design system, SVG framework, dependency, token, font, or route.
- No Svelte implementation.
- No durable Change Request, Evidence, Verifier, Eval, or trace contract work.
- No Workflow or Task resource route.
- No GitHub merge action or duplicate provider review system.
- No attempt to make every page visually unique. Shared shell and controls
  should repeat; page composition should follow the work being performed.

# Required visual grammar

Using the existing Wyrd tokens, every remediated screen must demonstrate:

- deliberate differences in type and panel scale rather than uniformly tiny
  labels and cards;
- one obvious primary work region;
- filled bands or surfaces that organize controls, state, and selected
  content;
- clear selected, hover-intent, and current-state treatments;
- wide data regions where comparison or inspection needs width;
- contextual drawers only when a selected item needs substantial inspection;
- compact summary information without squeezing detailed content into a
  universal right rail;
- consistent gutters, aligned panel edges, and no accidental empty lower half;
- no text collision, clipping, or bleeding at declared scale;
- status conveyed by text plus shape/glyph/pattern, never color alone; and
- identical information and geometry across each light/dark pair.

Do not reuse one `main table + 260px rail` composition across unrelated work
areas. Reuse components only where the user is performing the same kind of
work.

# Gate A — three golden screens

Change only the following desktop artboards first, in both light and dark
modes. Other artboards remain the rejected TASK-001 draft during this gate.

## Golden 1: CR-04 Change Request verification

Route: `/t/acme/changes/change_01/verification`

The screen must include a persistent Change Request header shared conceptually
with its Overview, Review, Timeline, and Subject pages:

- Change title and identifier;
- lifecycle state and current immutable revision;
- plain-language intent summary;
- Claims satisfied/required summary;
- Verifiers running/passed/failed/not-run summary;
- tabs or navigation to Overview, Verification, Review, Timeline, and Subjects;
- permission-aware `Approve`, `Request changes`, and `Close Change Request`
  controls; and
- provider navigation where relevant, but no source-code merge control.

The primary work region is a checks-like Claim/Verifier hierarchy:

- each Claim leads with its human-readable assertion and current resolution;
- required and advisory Verifiers are visibly subordinate to the Claim;
- Not ready, Not run, queued, running, passed, failed, and inconclusive states
  are immediately scannable;
- missing/received Evidence and manual/On-new-evidence mode are visible without
  collapsing Evidence, execution, verdict, and Claim state into one badge;
- expanded technical Evidence, binding, provenance, retry, and billable-rerun
  information is available through deliberate disclosure or detail treatment,
  not a permanently cramped rail; and
- approval, override authorization, verification, and lifecycle remain
  separate concepts.

Use the partner mock's narrative order—Change, exact revision, Claims,
Evidence, current decision—as interaction/composition inspiration. Use Wyrd's
own typography, colors, geometry, shell, and components.

## Golden 2: O-09 Eval event and workflow/task inspection

Route:
`/t/acme/observe/evaluations/eval_record_01?task=groundedness&service=checkout-agent`

Show three related states within the artboard without inventing routes:

1. Eval event summary with resolved Eval Card, subject/correlation, lifecycle,
   pass summary, and workflow results.
2. Selecting a workflow opens a large right-side workflow drawer over the
   event view. The drawer provides workflow identity, status, duration,
   pass/fail summary, stages, and task list.
3. Within that drawer, selecting a task shows its type, state, stage, operator,
   expected value, authorized actual value or redaction, score, timing,
   explanation, and trace link in a substantial detail region.

The task list and task detail use a desktop split view. The eventual mobile
adaptation uses an explicit `Task list` / `Task detail` switch. Per REQ-086,
selecting an event in O-08 navigates to the shareable O-09 event page; do not
replace that navigation with an Eval-record drawer. The workflow drawer is
contextual to O-09, and its selected-task detail remains inside that workflow
inspection experience. Workflow and Task remain views of the Eval event and
do not receive standalone resource routes.

## Golden 3: O-04 Trace search workbench

Route:
`/t/acme/observe/traces?service=checkout-api&status=error&range=1h`

Use this as the representative operational composition:

- visible URL-backed service/status/time filters and removable chips;
- a useful trend or distribution region that supports the search;
- a wide trace-results region with legible identity, service, operation,
  duration, span/error counts, and time;
- an unmistakable selected row;
- selected trace summary/detail that earns its space and links to the trace
  detail route while preserving filter context; and
- partial-results, no-match, and safe-error states without shrinking the main
  successful workflow.

Model the familiar search → trend → records → selected-detail flow. Do not add
an observability landing product, service hierarchy, or browser-created
correlation.

# Gate A acceptance and stop condition

Render the six golden light/dark artboards at their declared `1440 × 1024`
scale and present direct images or links to the human reviewer.

Stop after Gate A. Do not revise the remaining artboards until the user
explicitly approves all three golden screens. Feedback at this gate updates
this remediation candidate; it does not silently redefine the approved spec.

# Gate B — propagation after explicit approval

After Gate A approval:

1. Propagate the approved Change Request header, hierarchy, actions, and
   disclosures through CR-01–CR-07 and M-03/M-04 where applicable.
2. Propagate the approved Eval event/workflow/task inspection through O-08,
   O-09, and M-06 where applicable.
3. Propagate the approved operational Workbench composition through O-01–O-07,
   O-10, Q-01, M-05, M-07, and M-08 according to each page's actual workflow.
4. Audit Home and Cards against the same visual grammar. Revise only screens
   that reproduce the rejected uniform-card composition, weak hierarchy,
   spacing defects, or unexplained empty space. Preserve the shared Card shell
   and every TASK-001 route/state obligation.
5. Update `product/README.md` so its interaction and responsive ledger matches
   the corrected drawers, disclosures, action placement, overflow, and mobile
   behavior.
6. Present the complete six-sheet set for final explicit human acceptance.

# Ordered visual scenarios

Execute one scenario at a time:

1. **CR-04 orientation:** A cross-functional reviewer can identify the Change,
   revision, intent, Claim progress, Verifier progress, lifecycle, and next
   available action without opening technical detail.
2. **CR-04 active verification:** Evidence arrival produces visible queued and
   running Verifier states beneath the correct Claim; completed results update
   without conflating execution, verdict, Claim resolution, or authorization.
3. **CR-04 decision/action:** Approve, Request changes, Close, manual Run/Rerun,
   and override are placed and labeled according to their distinct effects and
   permissions.
4. **O-09 workflow inspection:** Selecting a workflow opens a large drawer and
   preserves the underlying Eval event context.
5. **O-09 task inspection:** Selecting a passed, failed, errored, or skipped
   task exposes meaningful task detail, including authorized/redacted values
   and trace navigation, without clipping.
6. **O-04 investigation:** URL-backed filters lead through trend and trace rows
   to selected detail while maintaining service/status/time context.
7. **Theme parity:** Each golden light/dark pair has identical hierarchy,
   geometry, information, and state.
8. **Propagation:** Every affected desktop and mobile artboard adopts the
   approved pattern without losing its original route, fixture, state, link,
   or responsive obligation.
9. **Final artifact review:** All 80 artboards remain directly viewable and
   legible, with no collision, uncontrolled overflow, or false completion
   claim.

# Visual RED, GREEN, REFACTOR discipline

For each scenario:

- **RED:** Capture the corresponding TASK-001 artboard and name the review
  finding it demonstrates.
- **GREEN:** Make the smallest SVG change that visibly satisfies the scenario
  using existing brand definitions and tokens.
- **REFACTOR:** Remove duplicated SVG definitions only when direct standalone
  viewing remains intact; align spacing and repeated anatomy without turning
  six static artifacts into a rendering framework.

# Verification and evidence

This remediation is static visual-contract work. Do not run application tests,
linters, or the repository gate.

Required evidence:

- direct rendered review of each Gate A artboard at declared scale;
- explicit human approval before Gate B;
- direct rendered review of all final sheets;
- light/dark information and geometry parity;
- a final artboard census of 64 desktop and 16 mobile artboards;
- a `README.md` walk proving every original route, state, link, and responsive
  obligation remains represented; and
- explicit human acceptance of the complete remediated mock set.

# Completion evidence

Record:

- before/after images or direct links for CR-04, O-09, and O-04;
- the user's Gate A approval;
- the list of propagated artboards and why each changed;
- the final six-sheet links;
- the route/state/responsive ledger result; and
- the user's final mock acceptance.

# Stop conditions

Stop and return to `$wyrd-spec` if remediation requires a new route, Card kind,
durable state, Change Request semantic, Eval resource, or materially different
workflow.

Stop and ask the user before changing Wyrd brand tokens, the five primary
navigation areas, tenant routing, or approved route vocabulary.

Do not begin TASK-002 or any Svelte product-view task until this remediation is
approved.

# Required implementation skills

- `$wyrd-implement`
- `wyrd-ui`

---

# Execution evidence — Gate A (2026-09-03)

Gate A only. The six golden artboards (CR-04, O-09, O-04 × light/dark) were
rebuilt in place inside `changes.svg` and `observe.svg`. All other artboards
remain the rejected TASK-001 draft, per the gate. No brand token, route, README,
or index.html change was needed.

## Scenario cycles (visual RED → GREEN)

1. **CR-04 orientation** — RED: FIND-TASK-001-3 — small cards/passive rails, no
   persistent header or actions. GREEN: persistent Change header (title,
   `change_01`, OPEN lifecycle, `rev_07 · immutable`, provider link, intent),
   filled progress band with Claim/Verifier KPI numerals, tabs to
   Overview/Verification/Review/Timeline/Subjects.
2. **CR-04 active verification** — RED: execution/verdict/Evidence collapsed
   into one cramped table + universal right rail. GREEN: checks-like Claim
   blocks with subordinate required/advisory Verifier rows across not-ready,
   queued, verifying, completed, passed, failed, stale; expanded in-flow
   evidence disclosure for `eval-gate-verifier v4` (binding, provenance, retry,
   billable manual run) instead of the cramped rail.
3. **CR-04 decision/action** — GREEN: permission-aware `✓ Approve` (primary),
   `! Request changes`, `✕ Close` with role note; separate CURRENT DECISION
   raised panel (verification vs approval vs override vs lifecycle); manual
   Run/Rerun marked billable; no source-merge control anywhere.
4. **O-09 workflow inspection** — RED: FIND-TASK-001-1 — permanently narrow
   selected-task rail, no drawer. GREEN: selecting `card_workflow_01 v7` opens
   a large (~920px) raised drawer with brand top-bar over the event page;
   scrimmed event summary and workflow results stay visible behind; drawer head
   carries identity, failed status, duration and 4/6 pass numerals.
5. **O-09 task inspection** — GREEN: drawer split view — task list (6 states:
   failed/passed/passed/failed/skipped/error, selected `groundedness`) beside a
   substantial detail pane: type, stage, operator, weight, timing, score 0.61
   with positional threshold tick at 0.80, expected vs authorized-actual value
   panels, redaction note, judge explanation, attempts/cost, trace link. Mobile
   list/detail switch noted in-frame and in the artboard desc.
6. **O-04 investigation** — RED: FIND-TASK-001-2/-4 — chip collision, big
   empty bottom, error states crowding the main flow. GREEN: URL-backed filter
   band + removable chips, trend band with matching/error-rate/p95 numerals and
   partial-results notice, wide 8-row results table with duration bars and
   unmistakable selected row, raised selected-trace panel (danger top-bar, span
   waterfall, filter-preserving `Open trace →`), no-match and
   WYRD-TRACE-SEARCH-FAILED as compact alternate-state insets.
7. **Theme parity** — light and dark are emitted from one geometry function per
   page; only token values differ (verified by construction and side-by-side
   render).

## Verification

- Static visual work: no app tests/lints run, per the task.
- Both sheets re-parse as well-formed XML after splicing.
- Each of the six artboards re-extracted from the committed sheets and rendered
  at declared 1440 × 1024 via headless Chrome; inspected for collision,
  clipping, bleed and empty-void defects.
- `git diff --check` clean. Write scope: `brand/renders/product/observe.svg`,
  `brand/renders/product/changes.svg` only.

## Stop condition

Stopped after Gate A as required. Gate B (propagation to the remaining 74
artboards + README ledger) awaits explicit human approval of these three golden
screens.

## Post-presentation revision 2 — workbench metric alignment (2026-09-03)

Feedback: `brand/renders/workbench.html` reads sleeker — better spacing, border,
padding, and text spacing. Extracted the exact `renders.css` metrics and retuned
the generator primitives:

- Row separators: dashed ink lines → `2px solid surface-2` (workbench `td` rule).
- Badges: 9.5px uppercase 700 ls0.5, 12% status tint, status-colored 2px border, r4.
- Card heads: 9.5px uppercase muted, borderless band, 30px tall; claim heads lose
  the surface-2 fill.
- Buttons: 13px→12px/700 with 3px hard shadow on every variant (workbench `wy-btn`).
- Sidebar: items 12px/600 muted; active gets brand-soft + border + 2px shadow.
- Topbar pills and crumbs to 10/11.5px uppercase-styled workbench sizes.
- Right-column heads harmonized to the same 9.5px uppercase treatment.

Verified at full screen size: rendered all six artboards at 1448×1032 and
inspected 1:1 crops (CR-04 claims column, O-09 drawer, O-04 top/bottom) plus
full dark renders. Zero generator overflow warnings. Regenerated the three
golden SVGs in `brand/renders/product/`. Awaiting Gate A approval.

## Post-presentation revision 3 — reviewer criticisms (2026-09-03)

Refinement of the three goldens only; no spec, token, fixture, or scope change.

- Shell recomposed to the Workbench contract: full-height 212px sidebar with
  the bohmian brand (Fraunces 600, lowercase) at its top; topbar begins right
  of the sidebar and carries WYRD pill, breadcrumbs (current segment bold),
  tenant identity, principal menu, and theme control; the former full-width
  toolbar is gone; five primary nav entries preserved; 3px outer shell border.
- Typography roles enforced: Fraunces wordmark (webfont @import embedded in
  each sheet so the SVG renders canonically); Space Grotesk titles/values/state
  headings; Archivo for descriptive prose at 1.55 line height; JetBrains Mono
  reserved for IDs, crumbs, tables, badges, compact labels.
- Edge density reduced: badges are borderless tint pills (status border only
  via explicit `strong` emphasis, used once on the drawer FAILED badge);
  verifier/task/result rows use surface-2 separators with no per-row boxes;
  nested blocks (score, explanation, expected value) are borderless surface-2
  fills; only the failed actual-value block keeps a danger border; shadows only
  on raised panels, quiet top-level panels, and buttons; selection is the
  brand-soft wash + 4px marker without extra chrome.
- CR-04: single Claims panel in checks-list form — CLAIM-1 compact satisfied,
  CLAIM-2 the one expanded Claim (not ready / verifying / queued + evidence
  note), CLAIM-3 compact failed summary with finding count and "view findings"
  disclosure; Current Decision stays raised; Bound Revision and Latest
  Activity are flat (no shadow) subordinate panels.
- O-09: drawer widened to 1088px (75.6% of viewport) overlaying the fully
  drawn event page behind a 28% black scrim; drawer canvas is the page
  background with contained surface panels; strong header (identity, state,
  duration, tasks passed, close); stage chips are passive surface-2 fills;
  task list (quiet) / selected-task detail (raised, dominant) split; all
  required task fields preserved; mobile note moved off the product surface.
- O-04: corrected shell/typography; filter facets are text controls and the
  search input a borderless surface-2 field; no-match and safe-error
  demonstrations moved to the sheet annotation band; selected-trace panel
  raised at full main width with span waterfall and filter-preserving action.
- Mock-author commentary removed from all product viewports; each sheet now
  has a dashed "MOCK ANNOTATIONS — NOT PRODUCT UI" band below the artboards
  carrying state-coverage, mobile, scrim, and alternate-state notes.
- Verification: all three sheets rendered via headless Chrome at 1:1
  (3016×1300), light/dark artboards structurally identical, Fraunces wordmark
  confirmed in crops, zero text-overflow warnings from the generator; a
  CLAIM-3 overflow past the Claims panel edge was caught in render review and
  fixed (panel height 420 → 446).

Gate A remains open: awaiting explicit human approval before any propagation.

## Post-presentation revision 4 — Gate A REVISE feedback (2026-09-03)

Scope held to the verdict: CR-04 materially simplified; O-09/O-04 labeling
cleanups only; no broad visual redesign.

- CR-04: right rail removed entirely (Bound Revision, Latest Activity gone,
  nothing added in their place). Summary banner and Current Decision merged
  into one raised decision panel: "Not verified" · "1 of 3 Claims satisfied ·
  5 passed · 1 failed · 3 pending" (complete aggregate) · plain-language cause
  "Waiting for ranking test results; PII review failed." Verification /
  approval / override distinction is one quiet Archivo line under the panel.
  Claims region spans the full 1180px content width with the same
  collapsed / expanded / collapsed structure and disclosures. Primary layer
  uses human names — Eval gate, Regression suite, Baseline comparison, PII
  review, test results, "revision 7" — with technical identifiers
  (eval-gate-verifier v4, …) as secondary metadata inside the expanded Claim
  only. Header metadata reduced to Open · change_01 · revision 7 · opened by
  s.okafor + intent + acme/checkout #482; base/candidate hashes moved to
  Subjects (noted in the annotation band). Actions collapsed to one primary
  "Review changes ▾" (Comment / Approve / Request changes) plus a ⋯ overflow
  holding Close.
- O-09: drawer heading is now the human-readable "Checkout quality workflow"
  with card_workflow_01 v7 beneath as technical identity; event page leads
  with the resolved "Checkout agent eval" name before eval_record_01;
  operator kv replaced by "method: threshold judge" with the exact judge (>=)
  operator preserved in the score caption; "6 · stage order" removed. No new
  panels or metrics.
- O-04: "Saved views" removed; Copy link retained.
- Verification: regenerated and rendered at 1:1 via headless Chrome, both
  themes structurally identical, zero overflow warnings (one 4px-margin
  warning caught and resolved during iteration), decision aggregate complete.

Gate A remains open: awaiting explicit human approval before any propagation.

## Gate A verdict — APPROVED (2026-09-03)

Human approval received for the revision-4 golden screens (CR-04, O-09, O-04)
in both themes. The approved grammar is now the visual authority for every
product mock. Goldens preserved unchanged as
`brand/renders/product/golden-{CR-04,O-09,O-04}.svg`.

## Gate B execution — grammar propagated to all 40 pages (2026-09-03)

All 35 remaining artboard pairs rebuilt in the approved Gate A grammar with
the accepted fixture content preserved (page ids, routes, params, fixtures,
states, links unchanged): H-01/H-02, C-01–C-12, O-01–O-03/O-05–O-08/O-10,
CR-01–CR-03/CR-05–CR-07, Q-01, M-01–M-08. The three approved goldens carried
into their sheets as-is.

- Shell, badges, altitude, selection, typography and one-question-per-page
  composition applied exactly as approved; alternate-state demonstrations and
  mock-author commentary live only in each sheet's dashed annotation strip.
- Six contact sheets regenerated and published to
  `brand/renders/product/{home,cards,observe,changes,query,mobile}.svg`
  (one page per row, light left / dark right, annotation strip per pair).
- Verification: every sheet rendered at 1:1 via headless Chrome; 25 of 40
  pages crop-inspected across both themes covering every family and every
  layout archetype; zero generator overflow warnings. Defects caught and
  fixed in render review: systemic badge/table-row misalignment; caption/row
  collisions in C-02, C-06, C-07, C-11, C-12, O-06, O-08, CR-01, CR-03;
  panel overlaps in C-04, O-01, O-02, O-03, O-05, CR-05, CR-07, Q-01, M-06;
  C-10 task-table clipping; O-05 identity-strip badge collision; mobile
  tenant-pill truncation.
- README updated: sheet anatomy (annotation strips carry alternate states;
  Google Fonts @import with documented system-face fallback), Gate A
  approved-grammar section, goldens note, and ledger lines resynced to the
  approved designs (CR-04 combined decision panel, O-09 drawer, O-04, and the
  six list-page state lines).

Gate B open: awaiting final human visual approval of the six sheets. Task not
complete; no Svelte implementation may begin until the sheets and ledger are
explicitly accepted.

## Gate B remediation pass — four mechanical findings closed (2026-09-04)

Reviewer verdict REMEDIATE (mechanical only, no redesign); also CR-07 subject
drilldown no longer highlights the Overview tab — it renders the tab strip
inactive with a "← Back to Overview — Subjects · subject 1 of 3" line.

- FIND-TASK-001-R1-1 (fragment IDs): every outer artboard group now carries
  id="<PAGE-ID>-<theme>"; verified 80/80 ids across the six sheets.
- FIND-TASK-001-R1-2 (sheet geometry): contact-sheet framing restored to the
  task's declaration — desktop 96px outer margins / 64px column gap / 96px
  label band (viewBox width 3136); mobile 64px / 48px / 80px (width 956);
  rows ≥64px apart. Product artboards untouched (1440×1024 / 390×844).
- FIND-TASK-001-R1-3 (H-01 centering): the subtitle and footer note passed
  "middle" positionally as font-weight; both calls now use weight 400 with
  anchor="middle". Verified centered in both themes.
- FIND-TASK-001-R1-4 (type floor): the three mobile status-badge call sites
  (M-03 subject rows, M-06 task rows, M-07 feature rows) raised 8px → 8.5px;
  badge widths derive from text measure so no adjacent compression. Verified
  in render.
- Google Fonts @import retained per reviewer deferral (matches the approved
  workbench reference; system-face fallback documented in the README) —
  awaiting the human decision if strict standalone is required.
- Verification: all six sheets regenerated and rendered 1:1 via headless
  Chrome (3136- and 956-wide); H-01, M-06 and sheet framing crop-inspected;
  zero generator overflow warnings; published to
  brand/renders/product/{home,cards,observe,changes,query,mobile}.svg.

Gate B open: awaiting final human acceptance of the six sheets.

## Persona-review remediation pass (content correction)

Verdict remediated: REMEDIATE (persona review). One focused content-correction
pass; no visual redesign. Golden SVGs unchanged as the Gate A record.

### Canonical change_01 fixture (now consistent everywhere)

- Title "Raise checkout ranking cutoff for high-risk carts"; revision 7; open;
  opened 2d ago by s.okafor; header shows "3 subjects" (no PR number).
- Subjects: subject_api acme/checkout PR #4412, subject_worker acme/checkout
  PR #4413 (stacked), subject_model acme/ranking PR #221.
- Claims: all three REQUIRED. CLAIM-1 satisfied, CLAIM-2 pending, CLAIM-3
  failed. Aggregate 1/3 everywhere (Home, CR-01, CR-03 rail, CR-04, M-03).
- Approval 1 of 3 (j.reyes approved; r.okafor requested changes; m.linden
  reviewing). m.chen and "0 of 1" removed. CLAIM-3 override authorized by
  j.reyes, recorded separately, does not verify.
- Applied across H-02, CR-01…CR-07, M-03; in-sheet CR-04 (golden-derived
  builder) updated: aggregate line, approval line, CLAIM-3 badge "failed"
  (canonical vocabulary, was "not satisfied").

### Other required changes

- O-01: Attention and Recent activity feeds labeled tenant-wide; caption states
  chips scope only signal tiles and signal pages. No out-of-scope rows imply a
  filtered feed.
- CR-01: search field added beside view filters; head note "search + views".
- CR-06: repetitive KIND/AUDIT badge column removed; TYPE column names each
  event source; caption states audit and discussion are never merged.
- O-10: restrained "+ Filter ▾" affordance with the full vocabulary
  (subject · principal · Run · method · signal · verdict); the
  no-report-in-range panel removed from the viewport and kept as a described
  alternate state in the annotation strip.
- H-02: attention leads with "Raise checkout ranking cutoff — PII review
  failed"; change_01 · CLAIM-3 demoted to secondary metadata.
- C-01: Verifier added to the kind rail; 17 registrable kinds; rail counts sum
  to 318 total; Service 42 matches "6 of 42 match".
- C-12: header action "Open in Observe" replaced with "View verification".
- README.md: O-01/O-10/CR-01/CR-06 material-state lines synced (page blocks and
  state ledger).

### Verification

- Regenerated all six sheets via gen_product.py; zero width-overflow warnings.
- Headless-Chrome renders + sips crops verified: H-02 attention row, C-01 kind
  rail, C-12 action button, O-01 feed labels, O-10 filter affordance and
  removed panel, CR-01 search, CR-03 claims/rail, CR-04 summary + approval +
  CLAIM-3 FAILED badge, CR-06 columns, M-03 claims line.
- golden-CR-04.svg, golden-O-09.svg, golden-O-04.svg untouched.
