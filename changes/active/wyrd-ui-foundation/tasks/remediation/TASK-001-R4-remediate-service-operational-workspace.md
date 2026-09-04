---
id: TASK-001-R4
title: Remediate Service into an operational workspace
kind: remediation
status: complete
spec: SPEC-wyrd-ui-foundation
spec_revision: 4
requirements: [REQ-015, REQ-016, REQ-017, REQ-018, REQ-080, REQ-084, REQ-112, REQ-113, REQ-123, REQ-124, REQ-125, INV-008, INV-009, INV-015, INV-016, INV-019, AC-003, AC-007, AC-008]
depends_on: [TASK-001-R2]
parent_task: TASK-001
remediates: [FIND-TASK-001-R2-1, FIND-TASK-001-R2-2]
---

# Outcome and user value

Replace the graph-dominant Service mock with a Service-local operational
workspace. A user arriving at a Service must be able to answer:

> What is happening to this Service now, why does it need attention, which
> component or signal explains that state, and where should I investigate?

The default Overview must serve Product, Data Science, and Engineering without
flattening their different needs:

- a product manager sees a plain-language assessment and customer-relevant
  attention before implementation detail;
- a data scientist sees the measured subject, metric or feature, threshold,
  baseline context, sample/freshness context, and direct Eval/Drift analysis;
  and
- an engineer sees exact Service version, time scope, component attribution,
  traffic/error/latency evidence, freshness, and direct Logs/Metrics/Traces
  investigation.

Preserve the implementor's deterministic relationship graph under
`Composition`. It remains useful system context; it is no longer the default
or dominant Service experience.

# Validated finding ledger

- **FIND-TASK-001-R2-1 — The relationship graph displaces operational
  understanding.** C-09 gives most of the viewport to topology while current
  behavior and attention occupy one thin summary strip. It also presents an
  active Card and a healthy-looking Service beside an active Drift alert
  without separating Card, deployment, operational, and data-freshness state.
  Consequence: no persona can reliably determine current Service behavior or
  whether displayed health is authoritative.
- **FIND-TASK-001-R2-2 — Service investigation is too coarse.** C-09 exposes
  one generic `Open in Observe` action and a selected Drift link rather than
  discoverable, scope-preserving Logs, Metrics, Traces, Evaluations, and Drift
  investigations. Alert origin, component attribution, time range, selected
  Service version, and the return path are not fixed. Consequence: an incident
  responder must reconstruct context after leaving the Service, and an alert
  risks becoming an invented Service-owned concept.

The user validated both findings and explicitly approved the corrected
Service behavior on 2026-09-04 after an adversarial Product Manager, Data
Scientist, and Software Engineer usability review. Specification revision 4
records that authority.

# Authority and required research

Read before editing:

1. `changes/active/wyrd-ui-foundation/spec.md` revision 4.
2. `changes/active/wyrd-ui-foundation/tasks/TASK-001-route-mapped-svg-mocks.md`.
3. `changes/active/wyrd-ui-foundation/tasks/remediation/TASK-001-R1-remediate-product-mock-composition.md`.
4. `changes/active/wyrd-ui-foundation/tasks/remediation/TASK-001-R2-remediate-card-workflows-and-service-composition.md`.
5. `changes/active/wyrd-ui-foundation/tasks/remediation/TASK-001-R3-remediate-experiment-workspace-mocks.md` for the existing earned-workspace render-package pattern only.
6. `changes/active/wyrd-ui-foundation/reviews/TASK-001-visual-acceptance.md`.
7. `architecture/wyrd-design.md`, especially Service, observation identity,
   publication bindings, relationships, Eval, Drift, Trigger, and Operator.
8. `architecture/wyrd-doctrine.mdx`.
9. `architecture/bifrost-design.md`, especially typed observation queries and
   tenant-scoped analytical projection.
10. `crates/wyrd/wyrd-server/wyrd-ui/brand/DESIGN.md`.
11. `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/workbench.html`.
12. `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/styleguide.html`.
13. Current C-09 and M-02 in the general Card sheets and the canonical Observe
    route/state ledger in `brand/renders/product/README.md`.

Use existing Wyrd mocks as the interaction reference. Do not introduce another
observability product, copy a vendor service page, or import predecessor
routes, vocabulary, visual identity, or status semantics.

# Ownership and write scope

Own only:

- a new directory:
  `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards/service/`;
- `C-09-light` and `C-09-dark` inside
  `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards.svg`;
- `M-02-light` and `M-02-dark` inside
  `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/mobile.svg`;
- `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/README.md`; and
- the product-mock links/count description in
  `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/index.html` when present.

Preserve every unrelated artboard, the complete Experiment package, and
`golden-{CR-04,O-09,O-04}.svg` exactly. Do not modify Svelte, BFF, Rust,
server, Vala, Bifrost, Card contracts, architecture, brand tokens, generated
theme output, or the original task files.

TASK-001-R3 touches `cards.svg` and the README for the independent Experiment
workspace. Integrate against its current cumulative artifacts when present;
do not overwrite or revert them. This shared file surface is an execution
coordination concern, not a behavioral dependency on Experiment.

# Non-goals

- No Svelte, BFF, server, query, storage, SDK, or production contract work.
- No browser-derived health, aggregation, alert, verdict, threshold, sort, or
  freshness decision.
- No Alert Card, alert configuration, alert mutation, `/observe/alerts`, or
  Service-owned Logs/Metrics/Traces/Evaluations/Drift route tree.
- No configurable dashboard, saved view, panel editor, investigation builder,
  correlation inference, or cross-version comparison.
- No deployment control, restart, rollback, scaling, invoke, rerun, or
  acknowledgement action.
- No new primary navigation area or change to canonical Observe ownership.
- No graph framework, force layout, minimap, graph controls, or replacement of
  the accepted deterministic relationship grammar.
- No application tests, linters, repository gate, or broad verification
  command for this static visual-contract task.

# Locked Service workspace

## Shared identity and scope

The shared Card header remains visible in every Service view and owns:

- human-readable Service name and purpose;
- Service Card UID and exact selected version;
- Card lifecycle state, labeled explicitly as `Card state`;
- owner and tenant context;
- version selection; and
- copy/direct Card affordances.

The selected Service version and visible time range scope every operational
projection. The default fixture uses `checkout-api`, `card_service_01`, version
`v12`, tenant `acme`, and `range=1h`.

Selecting historical `v11` changes both the declaration and operational scope
to `v11`. It must show version-scoped observations or an explicit no-data
state; it must not silently retain `v12` health. Cross-version comparison is
deferred.

Every operational value identifies freshness through a timestamp, age, or
window. Missing, stale, unauthorized, partial, and failed data are never zero
and never healthy.

## Exact local navigation

The Service-local primary subnavigation is exactly:

1. `Overview`
2. `Composition`
3. `Definition`

`Overview` is the default. Do not add Deployment, Configuration,
Observability, Alerts, Activity, or Versions tabs. Deployment facts belong in
Definition or the Overview assessment when authoritative. Version selection
remains in the shared Card header. Signal investigation remains in Observe.

Use visible, restorable query state in the mocks:

```text
/t/acme/cards/card_service_01?view=overview&version=v12&range=1h
/t/acme/cards/card_service_01?view=composition&version=v12
/t/acme/cards/card_service_01?view=definition&version=v12
```

The route encoding is the UI interaction contract for this change, not a new
durable server API.

## State separation

Never collapse these channels into one badge:

| Channel | Meaning | Required treatment |
|---|---|---|
| Card state | Registry lifecycle for the selected immutable Card version | Labeled `Card state`; never called runtime health |
| Deployment state | Runtime/deployment projection when authoritative | Show source/freshness; otherwise `Unknown` or absent |
| Operational state | Authorized server-projected assessment for the selected version/window | Plain-language state plus glyph and the reasons that support it |
| Data freshness | Whether the operational evidence is current, stale, partial, unauthorized, absent, or failed | Always visible beside the assessment and affected summaries |

The UI may format a server-projected state; it must not calculate one from KPI
values, an active Card, or the presence/absence of alerts.

## Overview composition

Question answered: **What is happening now, why does it need attention, and
where should I investigate?**

Use this order:

1. **Operational assessment — dominant region.** One wide, plain-language
   assessment states the current operational condition, reason, affected
   component or signal, detection/start time when available, freshness, and
   latest relevant change when available. State the conclusion once.
2. **Compact operating evidence.** Traffic, error rate, latency, and
   availability or another earned service indicator support the assessment.
   They may use one compact trend region; do not create a grid of equally
   weighted decorative KPI cards.
3. **Quality signals.** Eval and Drift summaries identify the measured
   component/subject, Eval or Drift Card, metric/feature, current result,
   threshold or pass context, sample/data-quality context when available, and
   last calculation. They link to canonical analysis.
4. **Attention.** A short prioritized list explains each active item in plain
   language, preserves originating signal and component identity, records age
   and severity, and links to its source investigation. Do not call this a
   standalone Alerts product.
5. **Component state.** A compact list identifies each runtime component,
   version, observed state, last observation, and relevant signal shortcut.
   Service aggregation must not erase component identity.
6. **Recent operational activity.** Show only events useful to interpreting
   current state, such as a version change, new Drift breach, Eval transition,
   or trace-error burst. This is not a general audit timeline.
7. **Investigate.** Present direct Logs, Metrics, Traces, Evaluations, and Drift
   destinations with preserved scope.

Use a coherent needs-attention fixture rather than contradictory badges:

- Card state: `ACTIVE`;
- deployment state: `DEPLOYED · 6/6`, explicitly a mock server projection;
- operational state: `NEEDS ATTENTION`;
- data freshness: current, last observation under one minute ago;
- traffic, errors, and latency remain within their displayed context;
- Model `ranker` publishes to Drift `model-drift` through Service `v12`;
- `model-drift` reports PSI `0.27` over threshold `0.20` for feature
  `feature`, with baseline `txns-2026q3`, visible sample/freshness context, and
  an active attention item;
- Agent `checkout-agent` publishes to Eval `checkout-agent-eval`, whose current
  result is shown with subject and last-calculation context; and
- the latest relevant change identifies deployment or selection of `v12`
  without claiming that correlation proves causation.

## Canonical investigation links

Every destination carries the selected Service, exact version where supported,
and visible time range. Use these mock routes:

```text
/t/acme/observe/logs?service=checkout-api&serviceVersion=v12&range=1h
/t/acme/observe/metrics?service=checkout-api&serviceVersion=v12&range=1h
/t/acme/observe/traces?service=checkout-api&serviceVersion=v12&range=1h
/t/acme/observe/evaluations?service=checkout-api&serviceVersion=v12&range=1h
/t/acme/observe/drift?service=checkout-api&serviceVersion=v12&range=1h
```

Drift attention may add `driftCard=card_drift_01&feature=feature`. Eval and
trace attention may link to an independently identified record when the mock
already has one. The destination must expose the inherited scope as visible,
removable filters and preserve ordinary browser Back behavior. `View all`
removes inherited Service scope. Do not add an Alerts route; each attention
item links to its originating canonical signal.

## Composition

Question answered: **How is this Service assembled, measured, and connected to
reaction wiring?**

Move the accepted R2 graph into this selected destination without reducing its
information:

- deterministic inputs/definitions, runtime composition, measurement, and
  reaction lanes;
- direct and transitive linked Cards;
- Card kind, human name, version, status, and direct route on every node;
- semantic edge meaning and direction;
- distinct authored aliases or reference occurrences resolving to one Card;
- Service-version-specific publication language;
- mock-only Drift → Trigger → Operator → Workflow continuation labeled as
  mock-only; and
- selected-node contextual inspection in a raised drawer while graph context
  remains recognizable.

Composition never claims a runtime execution or observation occurred. It does
not repeat operational KPIs or become a second Overview.

## Definition

Question answered: **What exact Service declaration and runtime wiring does
this version define?**

Show:

- purpose and exact Card version;
- entry point and runtime/deployment facts that are part of the declaration;
- component aliases and versioned Card references;
- component-level and Service-level publication bindings with their different
  subject semantics;
- principal context and composed Policy references when available;
- owner, labels, annotations, timestamps, and server-derived relationships;
- raw typed Spec disclosure with copy affordance; and
- direct routes to referenced Cards.

Keep declaration separate from deployment and observation projections. Never
show credentials or secret values.

# Required artboards

## C-09 — Service Overview entry

Replace the current graph-dominant C-09 body in both themes with the complete
needs-attention Overview above. Show the three-item local navigation, selected
Overview state, exact version and range, direct investigation destinations,
and a clear path to the detailed Service render package. C-09 remains the
general Card contact-sheet entry, not a second design.

## S-01 — Overview alternate and failure states

Route:
`/t/acme/cards/card_service_01?view=overview&version=v12&range=1h`.

Demonstrate the same Overview region under these honest alternatives without
turning them into simultaneous product panels:

- `HEALTHY` with current supporting evidence and no active attention;
- `STALE / NO RECENT DATA`, with last-seen context and no inferred health;
- `PARTIAL`, where authorization prevents one or more signal summaries and the
  available values remain usable; and
- safe backend failure, localized to the failed operational content with retry
  nearby while Card identity and navigation remain available.

## S-02 — Composition

Route: `/t/acme/cards/card_service_01?view=composition&version=v12`.

Render the accepted deterministic R2 graph as the dominant work region for
this selected destination, including selected-node inspection and every
identity, alias, edge, publication, mock-only, and navigation obligation above.

## S-03 — Definition

Route: `/t/acme/cards/card_service_01?view=definition&version=v12`.

Render the declaration-focused view above. Lead with human-readable meaning;
keep raw wire detail one disclosure away.

## M-02 — Service Overview entry at narrow width

Replace the current graph-first M-02 body with the needs-attention Overview.
Preserve Card identity, exact version, visible range, state separation,
assessment, attention, subject identity, freshness, and all five investigation
destinations. The three local destinations remain reachable without hiding
content or creating a mobile-only route.

## SM-01 — Overview at narrow width

Route:
`/t/acme/cards/card_service_01?view=overview&version=v12&range=1h`.

Make assessment and attention precede compact evidence. Investigation links
wrap or stack. Tables become labeled rows; no required state, filter, action,
or subject identity disappears.

## SM-02 — Composition at narrow width

Route: `/t/acme/cards/card_service_01?view=composition&version=v12`.

Present the complete graph as ordered relationship lanes or a controlled
horizontal graph region. Preserve every node and edge label. Selected-node
detail becomes a full-width raised sheet with an explicit return to
Composition.

# Render package and census

Create the minimum dedicated package:

```text
product/cards/service/
|- overview.svg      # S-01
|- composition.svg   # S-02
|- definition.svg    # S-03
`- mobile.svg        # SM-01, SM-02
```

Every page has paired light/dark artboards with identical structure,
information, geometry, state, and links:

- existing complete set after TASK-001-R3: 64 pages / 128 artboards;
- detailed Service package: 5 pages / 10 artboards;
- complete product set after this task: 69 pages / 138 artboards.

Update `product/README.md` with:

- links and counts for every Service sheet;
- the revised C-09 and M-02 entries;
- complete S-01 through S-03 and SM-01 through SM-02 route/state/link ledgers;
- exact Service-local navigation and URL-state contract;
- state-separation and canonical-investigation mappings;
- the distinction between Service Overview, Composition, Definition, and
  canonical Observe ownership;
- responsive mappings and fixture identities; and
- deferred production gaps below.

Update `renders/index.html` only when its product-set description or census
requires it.

# Deferred production gaps — record, do not implement

The static mocks intentionally project approved future UI behavior. Record
these gaps in the README and implementation handoff:

1. The current production Service/Card APIs do not provide one typed Service
   operational-workspace projection joining deployment, traffic, error,
   latency, Eval, Drift, attention, component, freshness, and recent-change
   context.
2. The authoritative owner and vocabulary for an aggregate operational state
   remain production integration work; the browser must not derive them.
3. Deployment state and latest-change correlation require server-owned data
   sources and may be unavailable independently from observation data.
4. Typed Observe query contracts must consistently accept and return exact
   Service Card-version correlation where the stored observation supports it.
5. A server-owned attention projection is needed to combine references to
   relevant Eval, Drift, trace, and log findings without inventing an Alert
   resource or browser-side severity model.
6. Production relationship projections must expose semantic edge kind and
   authored occurrence/path for the accepted Composition view.
7. The later Svelte/BFF task must reconcile its Service view model, URL state,
   partial authorization, stale/no-data, and safe-error behavior to revision 4
   before implementation.

# Ordered visual scenarios

Execute one scenario at a time:

1. Replace graph-first C-09 with one needs-attention operational Overview that
   separates all four state channels and preserves version/range scope.
2. Make the assessment understandable to Product while retaining subject,
   threshold, sample, freshness, and component evidence for Data Science and
   Engineering.
3. Link Logs, Metrics, Traces, Evaluations, and Drift to canonical Observe
   routes with compatible filters; link attention to its originating signal
   without creating Alerts navigation.
4. Demonstrate healthy, stale/no-data, partial-authorization, and safe backend
   failure behavior at the affected content position.
5. Move the accepted deterministic graph intact to selected Composition.
6. Add selected Definition with exact declaration/publication semantics and
   progressive raw detail.
7. Replace graph-first M-02 and prove narrow Overview and Composition behavior.
8. Update package links, routes, state ledger, responsive mapping, deferred
   gaps, and final census without changing unrelated artboards.
9. Inspect every changed and added artboard directly in light and dark modes at
   declared scale.

# Visual RED, GREEN, REFACTOR

For each scenario:

- **RED:** capture the current C-09 or M-02 state, or identify the missing
  Service destination/state, and name which validated finding it demonstrates.
- **GREEN:** make the smallest SVG and ledger change that visibly satisfies
  the scenario using existing Wyrd tokens, shells, graph grammar, and render
  conventions.
- **REFACTOR:** reuse existing static render anatomy only where the same user
  action repeats. Do not create a generator framework, universal dashboard,
  new component system, or copied Observe implementation.

# Verification and evidence

This is static visual-contract work. Do not run application tests, language
linters, code generation, or the repository gate.

Required evidence:

- before/after direct captures for C-09 and M-02;
- direct rendered inspection of C-09, S-01, S-02, S-03, M-02, SM-01, and SM-02
  in both themes at their declared 1440 x 1024 or 390 x 844 scale;
- identical light/dark structure, information, geometry, selected state, and
  links;
- direct review of healthy, needs-attention, stale/no-data,
  partial-authorization, and safe backend-failure treatments;
- readable state-channel labels and non-color status cues;
- no clipping, collision, bleed, illegible text, accidental empty lower half,
  or uncontrolled page-width overflow;
- all pre-existing general and Experiment artboard IDs preserved;
- Service package census of 5 pages / 10 artboards and complete-set census of
  69 pages / 138 artboards;
- README walk proving every route, subnavigation destination, state, link,
  version/range rule, persona obligation, and responsive behavior;
- `git diff --check`; and
- explicit human acceptance of the remediated Service workspace.

There are no named runtime tests for this static task and therefore no exact
`mise exec --` test command. Record the renderer and image-inspection commands
actually used without adding them as repository infrastructure.

# Completion and stop conditions

Complete only when every required Service artboard and ledger entry is updated,
the required evidence is recorded, unrelated artifacts are preserved, and the
user explicitly accepts the Service workspace.

Stop and return to `$wyrd-spec` if the work requires:

- a new Card kind, durable health/alert/deployment contract, or status meaning;
- an Alerts route, Service-owned observation route tree, or changed canonical
  Observe ownership;
- browser-derived health, severity, freshness, or correlation;
- a new production relationship meaning;
- cross-version operational comparison; or
- a brand-token, primary-navigation, authentication, tenancy, authorization,
  or secret-handling change.

Record production gaps rather than shrinking or falsely claiming the static
mock behavior. Do not begin the later Svelte Service implementation until this
task is independently readiness-reviewed, implemented, task-reviewed, and
accepted.

# Required implementation skills

- `$wyrd-implement`
- `wyrd-ui`

## Execution evidence — 2026-09-04 ($wyrd-implement)

### Visual RED baseline

- Captured C-09 light/dark and M-02 light/dark before any change (headless
  Chrome over `cards.svg`/`mobile.svg`). RED confirmed both validated
  findings: C-09 gives the viewport to the composition graph with one thin
  summary strip and shows `✓ HEALTHY` beside an active model-drift alert with
  no channel separation (FIND-TASK-001-R2-1); investigation is one generic
  `Open in Observe` action plus a drawer drift link with no scoped
  Logs/Metrics/Traces/Evaluations/Drift destinations (FIND-TASK-001-R2-2).
  M-02 was entirely composition lanes with no operational content.

### Scenario GREEN results (ordered per task)

1. **C-09 replaced** in `cards.svg` (both themes, same offsets/labels/strip
   geometry): dominant plain-language assessment (model-drift PSI 0.27 > 0.20
   on feature `feature`, subject ranker (Model v12), detected 15:00, v12
   deploy shown as correlation only) over four visibly separated channels —
   `card state ✓ ACTIVE` (labeled in the shared header and channel row),
   deployment `● DEPLOYED 6/6` (explicit mock server projection), operational
   `▲ NEEDS ATTENTION` (server-projected), data freshness `✓ CURRENT` (obs
   38s) — with local nav exactly Overview · Composition · Definition and
   URL-carried `?view=overview&version=v12&range=1h`.
2. **Persona layering**: headline sentence for Product; drift line carries
   metric/threshold/feature/baseline/sample/last-calculation for Data
   Science; evidence carries exact version, window bounds, budget/target/slo
   context, per-component versions and last observations for Engineering.
3. **Canonical links**: five Investigate rows →
   `/t/acme/observe/{logs,metrics,traces,evaluations,drift}?service=checkout-api&serviceVersion=v12&range=1h`;
   attention row links its originating signal
   (`/observe/drift?driftCard=card_drift_01&feature=feature&…`); no Alerts
   route anywhere.
4. **S-01** (`cards/service/overview.svg`): healthy, stale/no-recent-data
   (with the v11 rescope rule), partial-authorization (drift summary named
   unauthorized, never zero/healthy), and safe backend failure
   (WYRD_OBS_504_QUERY_TIMEOUT, localized Retry, identity/nav intact) as four
   dash-annotated alternatives of the one assessment region.
5. **S-02** (`composition.svg`): the accepted R2 graph fragment moved
   verbatim (byte-identical inner SVG extracted per theme from the prior
   C-09) under the Composition-selected header with a declaration-only scope
   line; drawer, aliases, publication language and mock-only chain intact.
6. **S-03** (`definition.svg`): purpose/entry_point/identity, 7 aliased
   component refs (two aliases → one capture-review Card), component-level vs
   service-level publication subject semantics (service-level stated absent),
   principal + Policy, metadata, relationships, versions, raw spec disclosure
   with Copy YAML; no secrets.
7. **M-02 replaced** in `mobile.svg` and **SM-01/SM-02** added
   (`cards/service/mobile.svg`): assessment and attention precede labeled-row
   evidence; 2×2 state-channel grid; five scoped investigations preserved;
   SM-02 reuses the accepted M-02 lanes fragment verbatim under a
   Composition-selected chip header with the full-width `‹ Back to
   Composition` sheet.
8. **Ledgers**: `product/README.md` — census 69 pages / 138 artboards,
   service package table rows, rewritten C-09/M-02 entries, full
   `cards/service/` section (nav + URL-state contract, state-separation and
   canonical-investigation mappings, Overview/Composition/Definition vs
   Observe ownership, fixtures), R-SVC responsive pattern + SM-01/SM-02
   mobile mappings, material-states rows, and the 7-item Service deferred
   production-gap section. `renders/index.html` — Service workspace card and
   69/138 census.
9. **Direct inspection**: every changed/added artboard reviewed at declared
   scale in both themes (C-09, S-01, S-02, S-03, M-02, SM-01, SM-02 light +
   dark) via headless-Chrome captures.

### Refactor / defects fixed during review

- C-09 component-state rows restructured to two lines (badge right-aligned)
  after a badge/last-observation collision; eval link moved to its own line
  after overflowing the quality panel; Investigate params reduced to fit the
  rail.
- S-03 components footnote collided with row 7 (row height 24→22, footnote
  moved); Copy YAML button was drawn under the raw-spec block (reordered).
- S-01 healthy quality line crossed its dashed frame by ~4px (fs 9→8.5).

### Verification

- Generator: `python3 svc_main.py` (scratchpad; renderer commands recorded,
  not added as repository infrastructure) — 5 pages / 10 artboards, overflow
  findings 0; `cards.svg` 28 unique artboards; `mobile.svg` 18 unique
  artboards (all pre-existing IDs preserved; goldens and the Experiment
  package untouched — the generator writes only `cards.svg`, `mobile.svg`,
  and `cards/service/*`).
- Screenshots: headless Chrome
  (`python3 shot.py <svg>:<name>:<x>:<y>:<w>:<h>:<scale>`), before/after for
  C-09 and M-02 plus all service pages, both themes.
- Collision scans (entity-aware panel-edge + same-baseline pair, over
  `cards/service/*.svg`, `cards.svg`, `mobile.svg`): findings 0
  (pre-existing untouched C-07 excluded).
- Census: complete set 69 pages / 138 artboards (43/86 general + 21/42
  Experiment + 5/10 Service).
- `git diff --check`: clean (tree under `renders/` is untracked on this
  branch; no whitespace errors in tracked diffs).

No named runtime tests exist for this static task; no application tests,
linters, codegen, or gate were run, per the task's verification scope.
Status: implementation COMPLETE — awaiting explicit human acceptance; nothing
committed.

### Self-review round — 2026-09-04 (post-implementation audit)

Re-audited every locked obligation against the rendered artboards; three
defects found and fixed, all regenerated and re-verified:

1. **Owner missing from the shared header** (locked "owner and tenant
   context"): `owner j.reyes` added to the header identity line on every
   desktop Service view and both mobile headers; re-captured C-09 and M-02.
2. **Pattern-label drift**: the README matrix assigns R-SVC but the SVG
   annotation strips said `pattern: R-CARD` — S-01/S-02/S-03/M-02/SM-01/SM-02
   strips now all read R-SVC (C-09 correctly remains R-CARD in both places);
   the README M-02 entry line was aligned to R-SVC.
3. **INV-019 wording**: the component-state footer claimed "observed state"
   while ranker's badge is a drift signal flag — footer now reads "observed
   state + active signal flag · last observation each".

Post-fix: regeneration stable (5 pages / 10 artboards, overflow 0; cards.svg
28 unique, mobile.svg 18 unique), entity-aware edge + baseline-pair scans 0
findings, C-09/M-02 re-inspected, `git diff --check` clean.

### Directed redraw — 2026-09-04 (S-01 state grid → four full Overview pages)

Direct user instruction (authority rank 1) superseded the task's single-page
S-01: the 2×2 grid of annotation-framed assessment fragments read as four
simultaneous panels and was rejected — "you need to separate into 4 distinct
overview visualizations." Redrawn accordingly:

- `overview.svg` now carries four complete 1440 × 1024 Overview pages, one per
  state, each rendering every region (assessment, channels, evidence, quality,
  attention, components, activity, investigate) honestly for that state:
  S-01 healthy, S-04 stale/no-recent-data (KPIs `—` with last-recorded 15:17,
  labeled trend gap, aged calculations, UNKNOWN deployment/attention), S-05
  partial-authorization (named unauthorized drift region, scoped-out Drift
  investigation, possibly-incomplete attention), S-06 safe backend failure
  (structured WYRD_OBS_504_QUERY_TIMEOUT with localized Retry; independently
  sourced panels remain usable). S-02/S-03 ids unchanged. Shared builder
  `overview_page(T, state)` also emits C-09 (needs-attention), keeping the five
  renderings structurally identical.
- Census supersedes the packet's locked 5 pages / 10 artboards: service package
  8 / 16, complete set 72 pages / 144 artboards. README census/ledger/matrix,
  index.html, C-09 + SM-01 strips updated to match.
- Verification: rebuild clean (unique artboard ids preserved: cards.svg 28,
  mobile.svg 18, service sheets 16); overflow audit 0; entity-aware
  panel-edge + baseline-pair scans 0 findings after one fix (S-04 aged drift
  baseline line shortened); all four states screenshot-reviewed in light,
  S-04/S-06 additionally in dark; `git diff --check` clean.

### Reviewer round 2 — 2026-09-04 (FIND-TASK-001-R4-1…5, mini-dashboard redraw)

Executed per direct user instruction carrying the reviewer's
SPEC_REVISION_REQUIRED findings; the custom-chart configuration contract is
recorded as deferred gap 8 (README), not invented in the mock.

- FIND-R4-5 (MAJOR, sniff test): every Overview rendering (C-09, S-01,
  S-04–S-07) redrawn as a four-layer signal-driven mini-dashboard — compact
  assessment strip (four labeled channels + latest change as correlation),
  dominant six-slot chart grid sharing the exact range (req/s, error vs 0.50%
  budget line, dash+marker p50/p95/p99 vs p95 target, availability vs 99.9%
  SLO, saved custom checkout-decline chart, restrained + Add chart slot; each
  chart carries latest value/unit/freshness/source and a canonical
  Metrics/Traces link), published Drift/Eval intelligence with 30d PSI and
  10-run pass-rate trends against visible thresholds and identified subjects,
  and one supporting context band. State pages affect charts locally: stale
  truncates every series into a labeled gap, failure renders per-chart NOT
  LOADED with a strip-local Retry, partial renders the drift card NOT
  AUTHORIZED.
- FIND-R4-1 (MAJOR): M-02 rebuilt as the complete stacked dashboard (six
  trend rows + intelligence + attention + component chips + activity + latest
  change + investigations — nothing dropped); SM-01 rebuilt as the detailed
  narrow dashboard (full-width charts with axes and positional markers).
- FIND-R4-2 (MODERATE): attention items name the affected component —
  "model-drift breach on ranker (model_primary · Model v12)" — on desktop and
  mobile.
- FIND-R4-3 (MODERATE): new S-07 (?view=overview&version=v11&range=1h)
  visibly selects historical v11 — header chip v11 (historical), per-chart
  explicit NO v11 OBSERVATIONS, no v11 intelligence — replacing annotation
  prose. Census: service package 9 pages / 18 artboards; complete set 73/146.
- FIND-R4-4 (MODERATE): S-03 raw spec is now closed by default behind a real
  ▸ disclosure row (line count, validation, Copy YAML, inline expansion).
- Verification: rebuild clean, unique ids preserved (cards.svg 28, mobile.svg
  18, service 18); overflow audit 0; entity-aware scans 0; screenshots
  reviewed — C-09 light+dark, S-04/S-05/S-06/S-07, S-03, M-02, SM-01
  light+dark; `git diff --check` clean. Composition untouched per reviewer.
