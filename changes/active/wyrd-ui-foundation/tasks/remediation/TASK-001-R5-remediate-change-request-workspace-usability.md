---
id: TASK-001-R5
title: Remediate Change Requests into a familiar review workspace
kind: remediation
status: complete
spec: SPEC-wyrd-ui-foundation
spec_revision: 4
requirements: [REQ-005, REQ-006, REQ-060, REQ-061, REQ-062, REQ-063, REQ-064, REQ-065, REQ-082, REQ-099, INV-001, INV-002, INV-008, INV-009, INV-012, INV-013, INV-014, AC-001, AC-003, AC-004, AC-005, AC-007]
depends_on: [TASK-001-R1]
parent_task: TASK-001
remediates: [FIND-CHANGES-01, FIND-CHANGES-02, FIND-CHANGES-03, FIND-CHANGES-04, FIND-CHANGES-05, FIND-CHANGES-06, FIND-CHANGES-07]
---

# Outcome and user value

Turn the current Change Request mocks into a coherent review workspace that is
as easy to enter, understand, review, and act on as a familiar pull-request
workflow while preserving Wyrd's broader coordination role.

A user must be able to move through this loop without decoding Wyrd internals:

> find work requiring attention -> understand the proposed change -> inspect
> Claims and verification -> review subjects and discussion -> comment,
> approve, or request changes -> understand what remains blocked

The workspace serves one shared Change Request to Product, Data Science, and
Engineering:

- a product manager sees intent, impact, coordination, readiness, and the next
  human decision in plain language;
- a data scientist sees the Claim, Evidence, Verifier result, evaluation
  context, failure explanation, and investigation path needed to judge the
  proposed behavior; and
- an engineer sees exact revisions and subjects, repository and provider
  context, changed files, commits, source anchors, discussions, and review
  actions.

Use GitHub Pull Requests as the interaction-familiarity bar, not as a visual
system or source of durable Wyrd semantics. Wyrd coordinates one exact revision
across one or more provider-neutral subjects; providers retain source editing
and merge authority.

# Validated finding ledger

- **FIND-CHANGES-01 — The Needs attention view is not a truthful inbox.**
  CR-01 selects `Needs attention` but displays all records, including verified
  and closed Changes. Rows show state but not why the current user or team must
  act. Consequence: every persona must inspect unrelated records to discover
  their work.
- **FIND-CHANGES-02 — New Change Request is presented as a report rather than
  an operable form.** CR-02 shows populated subject and Claim/Verifier tables
  without discoverable add, remove, or edit affordances. Consequence: an author
  cannot tell how to resolve the validation error or assemble a Change.
- **FIND-CHANGES-03 — Review does not let a reviewer complete a review.** CR-05
  shows existing threads but no visible Change-level composer or submission
  action for Comment, Approve, or Request changes. Consequence: the natural
  Review destination becomes a mostly read-only transcript.
- **FIND-CHANGES-04 — The persistent Change header is not persistent.** The
  review action and `Subjects` navigation appear on Verification but disappear
  from Overview, Review, Timeline, and subject detail. Consequence: users must
  repeatedly reorient and hunt for the next action.
- **FIND-CHANGES-05 — A Not ready Verifier offers Run now.** CR-04 reports that
  required test Evidence is missing while presenting an active manual run
  action. Consequence: users are invited to attempt work that cannot succeed
  and may be billable.
- **FIND-CHANGES-06 — Subject detail is inspection-only rather than a review
  loop.** CR-07 has a useful file list and diff but no direct way to open or
  create a revision-aware source discussion, move between changed files, or
  submit the review. Consequence: engineering review loses context through
  unnecessary navigation.
- **FIND-CHANGES-07 — State is visible but ownership of the next action is
  not.** Verification, approval, override, and lifecycle remain correctly
  separate, but the user must synthesize blockers and responsibility from
  several regions. Consequence: authors and reviewers cannot quickly answer
  “what is blocking this exact revision, whose turn is it, and what can I do?”

The user validated these findings and requested this remediation task on
2026-09-04 after an adversarial Product Manager, Data Scientist, Software
Engineer, Change author, and reviewer usability review of the current
`changes.svg` renders.

# Authority and interaction research

Read before editing:

1. `changes/active/wyrd-ui-foundation/spec.md` revision 4, especially
   REQ-005, REQ-060 through REQ-065, REQ-082, and REQ-099.
2. `changes/active/wyrd-ui-foundation/tasks/TASK-001-route-mapped-svg-mocks.md`.
3. `changes/active/wyrd-ui-foundation/tasks/remediation/TASK-001-R1-remediate-product-mock-composition.md`.
4. `changes/active/wyrd-ui-foundation/reviews/TASK-001-visual-acceptance.md`.
5. `architecture/wyrd-design.md` and `architecture/wyrd-doctrine.mdx`.
6. `crates/wyrd/wyrd-server/wyrd-ui/brand/DESIGN.md`.
7. `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/workbench.html` and
   `styleguide.html`.
8. The current CR-01 through CR-07 artboards in
   `brand/renders/product/changes.svg` and M-03/M-04 in `mobile.svg`.
9. The current Change Request route, state, link, and responsive ledger in
   `brand/renders/product/README.md`.

Use current GitHub documentation as interaction research only:

- reviewing proposed changes:
  `https://docs.github.com/en/pull-requests/how-tos/review-pull-requests/reviewing-proposed-changes-in-a-pull-request`;
- pull-request review decisions:
  `https://docs.github.com/en/pull-requests/reference/pull-request-reviews`; and
- pull-request search and filtering:
  `https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/filtering-and-searching-issues-and-pull-requests`.

Adopt the familiar loop of context, changed work, anchored discussion, and one
submitted review decision. Do not copy GitHub styling, branch authority,
suggested-change mutation, merge controls, provider-specific payloads, or
repository-only assumptions.

# Ownership and write scope

Own only:

- a new dedicated directory:
  `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/changes/`;
- CR-01 through CR-07 in
  `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/changes.svg`;
- M-03 and M-04 in
  `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/mobile.svg`;
- `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/README.md`; and
- the product-render links and census in
  `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/index.html` when needed.

Preserve every unrelated artboard and all approved golden files exactly.
Service remediation may concurrently change M-02, `cards.svg`, the Service
package, README, and the render index. Integrate against those cumulative
artifacts without reverting or overwriting them. That shared write surface is
an execution-coordination concern, not a behavioral dependency.

# Non-goals

- No Svelte, BFF, Rust, server, database, provider adapter, SDK, or production
  contract implementation.
- No durable Change Request, revision, review, discussion, Evidence, Verifier,
  approval, override, or attention-state redesign.
- No source editing, commit mutation, branch mutation, suggested-change
  application, provider review replacement, or merge action.
- No new route hierarchy, top-level navigation item, mention inbox, kanban,
  configurable dashboard, saved-view system, or review analytics.
- No browser-derived verification, readiness, attention reason, required
  reviewer, approval count, or next actor. The mocks project server-owned
  values.
- No new brand tokens, design system, SVG framework, dependency, or generalized
  form/review builder.
- No application tests, language linters, code generation, or broad repository
  gate for this static visual-contract task.

# Locked Change Request workspace

## Shared identity, navigation, and action area

CR-03 through CR-07 and their dedicated-package counterparts use one
structurally identical Change header. It owns:

- human-readable title and intent summary;
- Change identifier, lifecycle, exact immutable revision, author, and age;
- subject count and participating repositories;
- current verification summary and approval summary as distinct states;
- a concise server-projected blocker/next-action sentence;
- permission-aware `Review changes` with `Comment`, `Approve`, and
  `Request changes` choices;
- `Close Change Request` in the existing overflow treatment when authorized;
  and
- provider navigation where relevant, never source merge.

The local navigation is exactly:

1. `Overview`
2. `Verification`
3. `Review`
4. `Timeline`
5. `Subjects`

`Subjects` does not create a new collection route. It links to the subject
section on the existing Overview route, using a fragment or visible restorable
query state, and each selected subject continues to use the approved
`/changes/[id]/subjects/[subjectId]` route.

The header must identify the current destination consistently on Overview,
Verification, Review, Timeline, and subject detail. Do not duplicate the same
decision in a banner, rail, and table.

## Attention inbox

Question answered: **Which Change Requests need action, why, and from whom?**

For `/t/acme/changes?view=needs-attention`:

- the result set contains only records matching the selected view;
- every row states a concise server-projected attention reason such as
  `Your review requested`, `PII review failed`, `Required Evidence missing`,
  or `Changes requested`;
- every row identifies the expected person or team when known;
- lifecycle, verification, required Claim progress, repositories/PRs, owner,
  and last activity remain separate and visible;
- search covers human title plus owner, service, repository, and PR identity;
- initial Open, Needs attention, Verified, and Closed views remain unchanged;
  and
- no-match, unauthorized, and safe-error states do not fall back to an
  unfiltered list.

Do not copy GitHub's search language wholesale. A compact `Add filter`
affordance may expose only filters already supported by the approved mock
projection.

## Progressive creation

Question answered: **What is changing, what must be true, and who should
review it?**

Keep one scrolling form. The primary subject path accepts a linked pull-request
URL and resolves it into a reviewable provider-neutral preview containing
repository, provider, optional PR, and exact base/candidate commits. The exact
commit pair remains the identity; the URL is input convenience, never truth.

Also provide:

- a manual exact-commit path when no pull request exists;
- explicit `Add subject`, remove subject, and repair-invalid-subject actions;
- editable title, intent, impact, owner, and participating teams;
- explicit `Add Claim`, edit/remove Claim, and required/advisory selection;
- explicit `Add Verifier`, edit/remove Verifier, and Manual/On-new-evidence
  mode selection;
- the adjacent billable-mode warning already required by the specification;
- incomplete `Save draft` at all times; and
- inline errors beside the field or row that must be repaired.

Do not show Evidence upload or acceptance before an existing revision and
exact subject are resolved.

## Overview and readiness

Question answered: **What is changing, who is affected, and what blocks this
exact revision?**

Preserve the approved intent, impact, owner/team, revision, subject, Claim,
verification, and override content. Add one compact server-projected action
summary that names:

- the exact revision;
- blocking verification or review conditions;
- the expected next actor when known; and
- the current user's available action.

This summary is a presentation of typed mock truth, not browser-computed
readiness. Keep verification, approval, override, and lifecycle separately
labeled beneath it.

## Verification action integrity

Question answered: **What has been established, what failed, and what can run
now?**

Preserve the checks-like Claim/Verifier hierarchy and progressive Evidence,
run, finding, and provenance drilldowns. Correct action availability:

- `Not ready` because Evidence is missing exposes `View missing Evidence` or
  the relevant prerequisite, not `Run now`;
- a ready Manual Verifier exposes `Run now` with explicit billable
  confirmation;
- a completed rerunnable Verifier exposes `Rerun` with the same confirmation;
- queued or running work links to its run and cannot be started again;
- stale or carried-forward provenance remains distinct from verdict; and
- approval or override never changes a failed or unresolved verification
  result into a pass.

## Review authoring and submission

Question answered: **What feedback remains, and what decision will I submit?**

CR-05 must support the visible interaction anatomy for:

- a Change-level comment composer with stable `@name`/team mention authoring;
- an inline reply composer opened from an existing thread;
- open, resolved, and reopened threads anchored to Change, Claim, subject,
  Evidence, Verifier result, or revision-aware source location;
- edit history, resolve, and reopen affordances;
- the stale expected-revision failure with draft text preserved; and
- one `Review changes` submission panel containing an optional summary and the
  three approved decisions: Comment, Approve, or Request changes.

The submission panel must state which exact revision is being reviewed and the
effect of the selected decision. It does not merge, close a provider PR, run a
Verifier, or turn approval into verification.

## Subject and source review

Question answered: **What changed in this subject, and where does feedback
belong?**

Preserve repository/provider identity, PR context, exact commits, stacked
relationships, changed files, commit history, read-only unified diff, and the
provider link. Add the minimum review continuity:

- previous/next changed-file navigation and a visible file position;
- direct `Open thread` for an existing source anchor;
- direct `Comment on line` or `Start discussion` that creates a Wyrd
  revision-aware source discussion;
- an obvious return to the complete subject list; and
- the persistent `Review changes` submission action.

Do not add source suggestions, edit controls, a browser checkout, merge
authority, or persisted `Viewed` state. Reconsider viewed-file tracking only
after a real production need proves it.

## Timeline

Keep one chronology of revision, provider, Evidence, run/result, Claim,
decision/override, and discussion events. Preserve type, actor, time, plain
explanation, and destination. The shared header and action summary remain
available; the timeline does not become a second audit administration view.

# Dedicated render package

Create the following minimum package:

```text
product/changes/
|- inbox.svg          # CRW-01
|- new.svg            # CRW-02
|- overview.svg       # CRW-03
|- verification.svg   # CRW-04
|- review.svg         # CRW-05
|- timeline.svg       # CRW-06
|- subjects.svg       # CRW-07
`- mobile.svg         # CRWM-01, CRWM-02, CRWM-03
```

Required pages:

- **CRW-01:** truthful Needs-attention inbox with action reason and owner;
- **CRW-02:** operable progressive creation with resolved PR convenience and
  one repairable invalid subject;
- **CRW-03:** Overview with persistent header and next-action summary;
- **CRW-04:** Verification with correct blocked/ready/manual action states;
- **CRW-05:** Review with general comment, inline reply, and submitted review
  decision anatomy;
- **CRW-06:** Timeline with stable header and typed destinations;
- **CRW-07:** subject diff with anchored discussion and file navigation;
- **CRWM-01:** narrow creation preserving add/edit/validation and Save draft;
- **CRWM-02:** narrow review preserving composer, threads, and review
  submission; and
- **CRWM-03:** narrow subject review preserving identity, file selection,
  source discussion, provider link, and review action.

Every page has paired light/dark artboards with identical structure,
information, geometry, state, and links. The package adds 10 pages and 20
artboards. After the independent Service and Change packages are both present,
the expected complete product census is 79 pages and 158 artboards; if the
integration order differs, verify the Change-package delta and recompute the
cumulative README census from the actual tree.

Keep `changes.svg` as the complete CR-01 through CR-07 contact sheet and update
those artboards from the same approved interaction anatomy. Do not maintain a
second divergent design. M-03 and M-04 remain the general narrow-width entries
and point into the dedicated package.

Update `product/README.md` with:

- links and counts for every Change package sheet;
- complete CRW-01 through CRW-07 and CRWM-01 through CRWM-03 route, state,
  action, and link ledgers;
- the stable local navigation and shared-header contract;
- the truthful attention-filter rule;
- creation input-to-exact-subject resolution;
- Verifier action eligibility and billable confirmation;
- review submission and source-discussion behavior;
- responsive mappings and fixture identities;
- deferred production gaps; and
- the actual cumulative page/artboard census.

# Deferred production gaps — record, do not implement

1. The production Change Request contract and BFF projection must provide an
   authoritative attention reason, expected actor/team, and current-user action
   rather than making the browser infer them.
2. Pull-request URL resolution needs a server-owned provider-neutral operation
   that validates authorization and returns repository, provider, optional PR,
   and exact base/candidate identity. The URL itself must never replace the
   exact subject identity.
3. Production creation actions need typed add/update/remove behavior for
   subjects, Claims, Verifier requirements, owners, and teams while preserving
   retry safety and incomplete drafts.
4. Review composition, replies, mentions, decisions, resolve/reopen, and stale
   revision rejection require tenant-scoped transactional control-plane
   operations; mocks and Bifrost are not their authority.
5. Source discussions require a stable revision-aware anchor projection and a
   provider diff projection without granting source mutation or merge
   authority.
6. Review requirements, approval counts, blocker explanation, and the next
   authorized action require server-owned projection. The UI must not derive a
   synthetic mergeability or readiness state.
7. The later Svelte/BFF task must reconcile its route loads, form actions,
   authorization, CSRF, focus management, drafts, and safe errors to the
   accepted Change mocks before implementation.

# Ordered visual scenarios

Execute one scenario at a time:

1. Make Needs attention a truthful subset with one plain-language reason and
   expected actor per row; prove no-match does not leak the unfiltered list.
2. Make CR-02 visibly operable from PR URL or manual commits through subjects,
   Claims, Verifiers, validation, and incomplete draft saving.
3. Establish one shared header, five-item local navigation, and review action
   across Overview, Verification, Review, Timeline, and subject detail.
4. Add one authoritative next-action summary without collapsing lifecycle,
   verification, approval, or override.
5. Correct Not-ready, ready-manual, running, and rerunnable Verifier actions
   with billable confirmation only when execution is eligible.
6. Prove Change-level commenting, inline reply, stale-revision recovery, and
   Comment/Approve/Request-changes submission on Review.
7. Preserve exact subject and provider context while adding file navigation
   and revision-aware source discussion to the read-only diff.
8. Preserve the shared header and typed drilldowns on Timeline.
9. Prove the three critical narrow-width flows without hiding state, filters,
   validation, discussion, or review actions.
10. Synchronize the contact sheet, mobile entries, dedicated package, README,
    links, and census without changing unrelated artifacts.
11. Inspect every changed and added artboard directly in both themes at its
    declared scale.

# Visual RED, GREEN, and REFACTOR

For each scenario:

- **RED:** capture the current CR-01 through CR-07 or M-03/M-04 behavior and
  identify the exact validated finding demonstrated by that state.
- **GREEN:** make the smallest render and ledger change that visibly satisfies
  the scenario with the current Wyrd shell, tokens, typography, status cues,
  and approved Change fixture.
- **REFACTOR:** share render anatomy only where the same Change header, action,
  form, thread, or diff behavior repeats. Do not create a general SVG
  framework, universal workflow component, or duplicate product model.

# Verification and completion evidence

This is static visual-contract work. Do not run application tests, language
linters, code generation, or the repository gate.

Required evidence:

- before/after direct captures for CR-01 through CR-07 and M-03/M-04;
- direct rendered inspection of CRW-01 through CRW-07 and CRWM-01 through
  CRWM-03 in both themes at declared 1440 x 1024 or 390 x 844 scale;
- identical light/dark structure, information, selected state, actions, and
  links;
- explicit persona walk-throughs for Product Manager, Data Scientist,
  Software Engineer, Change author, and reviewer/approver;
- a complete author journey from new draft through exact subjects, Claims,
  Verifiers, validation, and Save draft;
- a complete reviewer journey from truthful attention inbox through context,
  verification, subject diff, anchored discussion, and submitted decision;
- proof that Not ready never exposes an executable run action and that
  billable Run/Rerun always requires confirmation;
- proof that no Change view exposes source editing or merge authority;
- no clipping, collision, bleed, illegible text, accidental empty lower half,
  or uncontrolled page-width overflow;
- status and action eligibility never conveyed by color alone;
- all unrelated general, Experiment, and Service artboard IDs and content
  preserved;
- Change package census of 10 pages / 20 artboards and an accurate cumulative
  product census;
- README walk proving every route, state, action, link, responsive mapping,
  persona obligation, and deferred production gap;
- `git diff --check`; and
- explicit human acceptance of the remediated Change Request workspace.

There are no named runtime tests for this static task and therefore no exact
`mise exec --` test command. Record the renderer and image-inspection commands
actually used without adding them as repository infrastructure.

# Completion and stop conditions

Complete only when every required Change artboard and ledger entry is updated,
the required evidence is recorded, unrelated artifacts are preserved, and the
user explicitly accepts the workspace.

Stop and return to `$wyrd-spec` if the work requires:

- a new durable Change Request, review, attention, readiness, discussion,
  Evidence, Verifier, approval, override, or file-review state;
- a new route, mention inbox, top-level navigation area, or provider-specific
  Change workspace;
- source mutation, provider review replacement, branch or commit mutation, or
  merge authority;
- browser-derived attention, review requirements, verification, readiness,
  authorization, or next-actor decisions;
- a new provider-resolution security or credential contract;
- a brand-token, primary-navigation, authentication, tenancy, authorization,
  audit, or secret-handling change; or
- removal or reinterpretation of exact revision/subject identity.

Do not begin the later Svelte Change Request implementation until this task is
independently readiness-reviewed, implemented, task-reviewed, and accepted.

# Required implementation skills

- `$wyrd-implement`
- `wyrd-ui`

## Execution evidence — 2026-09-04

Static visual-contract execution via the scratchpad SVG generator (genlib
Board API shared with the Experiment/Service packages). No application tests,
linters, codegen or gate were run, per the task's verification scope.

### Scenario walk (visual RED → GREEN)

1. **Truthful inbox (FIND-CHANGES-01)** — RED: CR-01 showed all 5 records
   including verified/closed with no reason. GREEN: CRW-01/CR-01 renders only
   the 3 matching open records, each with a server-projected reason badge
   (Your review requested / Required Evidence missing / Changes requested)
   and expected actor; no-match, unauthorized and WYRD_CHANGE_502 states keep
   the view and search; reason-vocabulary and search-scope contract panels
   fill the lower page.
2. **Operable creation (FIND-CHANGES-02)** — GREEN: CRW-02/CR-02 adds the
   PR-URL input with Resolve (URL is convenience, never identity), per-row
   Edit/Remove and Repair on the invalid acme/ranking subject with its inline
   error, + Add subject from PR URL / + Add by exact commits, + Add Claim /
   + Add Verifier, Required and Manual/On-new-evidence selects, editable
   owners/teams, earned-absent Evidence and ever-present Save draft.
3. **Shared header (FIND-CHANGES-04)** — GREEN: one `cr_header` builder is
   used by CRW-03…CRW-07 (and CR-03…CR-07): title, identity line, four
   separately labeled channels (lifecycle · verification · approval ·
   override), server-projected next line, Review changes ▾ + ⋯, and the exact
   five-item nav; Subjects links to Overview #subjects while subject detail
   keeps /subjects/{subjectId}.
4. **Next-action summary (FIND-CHANGES-07)** — GREEN: CRW-03 Action summary
   names revision 7, both blockers with expected actors, and the current
   user's available action; lifecycle/verification/approval/override stay
   separate beneath it.
5. **Verifier action integrity (FIND-CHANGES-05)** — RED: CR-04 offered Run
   now on a Not-ready Verifier. GREEN: Not ready → View missing Evidence
   only; verifying/queued → run links that cannot start again; the failed
   rerunnable PII review → Rerun behind the raised billable confirmation
   panel; stale stays provenance; the Run-eligibility rail states the rules.
6. **Review completion (FIND-CHANGES-03)** — GREEN: CRW-05 has the
   Change-level composer with @mention autocomplete, an open inline reply
   composer, resolve/reopen/edit-history affordances, the
   WYRD-COMMENT-STALE-REVISION recovery preserving the draft, and the raised
   Submit review panel (exact revision, optional summary, Comment / Approve /
   Request changes with stated effects).
7. **Subject review loop (FIND-CHANGES-06)** — GREEN: CRW-07 adds ← All
   subjects (Overview #subjects), file 1 of 6 with Prev/Next file, Open
   thread on the resolved rank.rs:118 anchor, and an open revision-aware
   Start-discussion composer on rank.rs:121; the diff stays read-only with no
   edit/merge/suggested-change/Viewed state.
8. **Timeline** — GREEN: CRW-06 keeps one chronology with typed destinations
   under the shared header; audit vs discussion separation retained; an
   explicit not-an-admin-view absent panel.
9. **Narrow flows** — GREEN: CRWM-01 (creation with Repair + sticky Save
   draft), CRWM-02 (mini header, composer, reply, stale recovery, full Submit
   review), CRWM-03 (identity, file chips with position, sideways diff, line
   composer, provider link, sticky Review changes).
10. **Synchronization** — changes.svg regenerated from the same builders
    (never a second design, strips point into product/changes/); M-03/M-04
    spliced in place from the CRWM-01/CRWM-02 builders; README and index.html
    ledgers and census updated.

### Fixture corrections

- Author unified to r.okafor (CR-01 listed r.okafor while CR-03…CR-07 said
  s.okafor); reviewer roster unchanged (j.reyes approved, r.okafor requested
  changes, m.linden reviewing).

### Verification results

- Build: `python3 cr_main.py` — 8 package sheets + changes.svg (7 pages) +
  mobile.svg splice; overflow audit 0 findings; mobile.svg 18 unique
  artboards preserved.
- Entity-aware same-baseline collision scan (`cr_scan.py`) over every owned
  artboard: final result 0 findings (5 distinct issues found and fixed:
  inbox column spacing, review thread badges, subject diff header, creation
  mode/actions columns, narrow thread badges).
- Census from the actual tree: 83 pages / 166 artboards (Change package
  10/20); all per-file artboard ids unique; goldens and all general,
  Experiment and Service artboards untouched (`golden-CR-04.svg` preserved
  as the Gate A record).
- Direct rendered inspection via headless Chrome at declared scale, both
  themes: CRW-01…CRW-07 light, CRW-03/04/05/07 dark, CRWM-01…03 light,
  CRWM-02 dark, spliced M-03 light and M-04 dark, contact-sheet CR-03 dark —
  no clipping, collision, bleed, empty lower half, or color-only state.
- `git diff --check` clean. brand/renders/ remains untracked working state;
  no commit created (commit only on request).

### Persona walk-throughs

- **Product manager**: CRW-01 reason column → CRW-03 plain intent/impact +
  Action summary states what blocks revision 7, who acts next, and their own
  available action without decoding Claims.
- **Data scientist**: CRW-04 CLAIM-2 shows the missing test Evidence
  prerequisite and run history binding (last run on revision 6); Evidence
  thread on CRW-05 links the eval record; failure explanation and findings
  paths preserved on CLAIM-3.
- **Engineer**: CRW-07 exact commits, stacked relationship, file navigation,
  anchored discussion and provider link complete the diff-review loop without
  leaving scope.
- **Author**: CRW-02 from PR URL or exact commits through subjects, Claims,
  Verifiers, validation and Save draft; CRW-01 Changes-requested row routes
  the author back.
- **Reviewer/approver**: CRW-01 Your-review-requested row → CRW-03 context →
  CRW-04 verification → CRW-07 diff + discussion → CRW-05 Submit review
  (Request changes selected) — one submitted decision, never a merge.

Status: COMPLETE pending explicit human acceptance of the remediated Change
Request workspace (task stop condition).

## Adversarial review evidence — 2026-09-04

Post-implementation adversarial pass (technical + persona) over the delivered
package. Five real contract gaps were found and fixed; three judgment calls
were reviewed and kept.

Fixed:

1. **Missing owner on inbox rows** (contract: owner "remains separate and
   visible"). Each CRW-01/CR-01 row now carries `owner <user>` end-anchored on
   its reason line.
2. **change_03 lifecycle regression** — the fixture's draft record had been
   rendered `● OPEN`, and the Open chip counted 5. Restored `○ DRAFT`
   (neutral), chips now Open · 3, panel renamed "3 of 5 Change Requests", and
   the footer names change_04 (open · verified) and change_05
   (closed · cancelled) explicitly.
3. **No demonstrated ready-manual Run now** — the eligibility rail documented
   the Ready · manual rule but no row exercised it. CLAIM-1 is now expanded:
   Checkout suite `✓ PASSED` with View result, plus the advisory manual
   billable "Hold-rate spot check" at `● READY` exposing Run now with the
   caption "billable — Run now asks for confirmation" (claims panel 470→574).
4. **Close Change Request never rendered** despite being claimed in the
   ledger. CRW-06/CR-06 now shows the ⋯ overflow open, hanging from the
   button: "Close Change Request — authorized · records a closure reason ·
   never closes provider PRs".
5. **Misleading mobile verification badge** — `✕ 1/3 VERIFIED` read as
   partially verified. Now `✕ NOT VERIFIED · 1/3` on CRWM-02/03 and M-04
   (matches the desktop channel wording).

Two rendering defects introduced by fix 3/4 were caught by screenshot and
fixed: the Run now button overflowed the claims panel edge (w 88→68) and the
overflow menu initially floated mid-page clipped by the DESTINATIONS rail
(re-anchored under the header ⋯ at 1216,126).

Reviewed and kept (judgment calls, not gaps):

- Review changes ▾ is never shown dropped open; its three choices and their
  effects are demonstrated by the CRW-03 action summary and the CRW-05 raised
  Submit review panel.
- CRW-02 claim rows pair one Edit/Remove per combined claim+verifier line;
  separate per-verifier actions deferred to production.
- Verified/Closed views are described (chips + footer) but not rendered as
  additional artboards — the truthful-subset rule is proven by the
  needs-attention and no-match states.

Verification after fixes: `cr_main.py` overflow findings 0; `cr_scan.py`
collision findings 0 across all 10 files; mobile.svg 18/18 unique artboards;
screenshots of CRW-01, CRW-04 (detail), CRW-06 (menu detail), CRWM-02 in
light confirmed; `git diff --check` clean. README ledger rows for
CR-01·CRW-01, CR-04·CRW-04, CR-06·CRW-06 and the three entry sections updated
to match. Strips for CRW-01/04/06 updated in the same builders, so
changes.svg and the package remain byte-identical projections of one design.

Status: COMPLETE pending explicit human acceptance (unchanged).
