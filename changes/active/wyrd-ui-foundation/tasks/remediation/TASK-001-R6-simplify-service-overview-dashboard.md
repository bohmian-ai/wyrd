---
id: TASK-001-R6
title: Simplify Service Overview into an intuitive mini-dashboard
kind: remediation
status: complete
spec: SPEC-wyrd-ui-foundation
spec_revision: 5
requirements: [REQ-112, REQ-123, REQ-124, REQ-125, REQ-126, INV-008, INV-009, INV-015, INV-016, INV-019, INV-020, AC-003, AC-007, AC-008]
depends_on: [TASK-001-R4]
parent_task: TASK-001
remediates: [FIND-TASK-001-R4-1, FIND-TASK-001-R4-2, FIND-TASK-001-R4-3, FIND-TASK-001-R4-4, FIND-TASK-001-R4-5]
---

# Outcome

Make the dedicated Service package pass the five-second test: a user sees how
the Service is behaving, which published signal needs attention, and where to
investigate without reading an information wall.

# Scope

Own only:

- `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards/service/overview.svg`
- `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards/service/mobile.svg`
- `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards/service/definition.svg`

Preserve `composition.svg` byte-for-byte. Do not update `cards.svg`, the general
`mobile.svg`, the product README, render index, application code, contracts,
tokens, or generated theme. Those contact-sheet surfaces may be synchronized
only after this dedicated package is accepted.

# Validated findings

- **FIND-TASK-001-R4-1:** Narrow Overview removed Component State, Recent
  Operational Activity, and latest-change context instead of stacking or
  progressively disclosing them.
- **FIND-TASK-001-R4-2:** Attention identified `model-drift` and `feature` but
  omitted the affected `ranker` component.
- **FIND-TASK-001-R4-3:** Historical v11 scoping was described in annotation
  prose rather than visibly rendered with v11 selected.
- **FIND-TASK-001-R4-4:** Definition claimed progressive disclosure while raw
  YAML was permanently open without a disclosure control.
- **FIND-TASK-001-R4-5:** Overview became a dense status report with one minor
  graph instead of an intuitive operational mini-dashboard.

# Locked Overview

Use one clear vertical reading order:

1. **Compact assessment.** One sentence names the state, reason, affected
   component or signal, detection time, freshness, and latest relevant change.
   Keep Card, deployment, operational, and freshness channels visible in the
   same compact header without repeating the conclusion.
2. **Service activity.** Make three time-series charts the dominant region:
   requests/sec, error rate, and latency. Latency distinguishes p50/p95/p99 by
   dash or marker as well as color. Show latest value, unit, freshness, and any
   threshold or budget. Availability is a small indicator unless a useful
   history is present.
3. **Published signals.** Render one concise Drift trend for `model-drift`
   published by `ranker` and one Eval trend for `checkout-agent-eval` published
   by `checkout-agent`. Each keeps subject/version, current result, threshold
   or pass context, sample/baseline context, freshness, and its canonical
   Observe link. Put the breached Drift first.
4. **Next action.** Keep one short Attention row and direct Logs, Metrics,
   Traces, Evaluations, and Drift links. The Attention row includes `ranker
   (Model v12)` rather than forcing the user to reconstruct the subject.
5. **Details.** Component State and Recent Operational Activity remain
   available behind one clear disclosure. They do not compete with the charts.

Do not add a dashboard grid, drag-and-drop layout, chart editor, saved-view
system, arbitrary SQL, or duplicate status panels.

# Optional custom chart

Show at most one opt-in custom metric slot. Demonstrate either:

- a configured `checkout decline rate` trend labeled `CUSTOM`; or
- an empty `Add chart` state.

Treat it as a mock server projection. Do not define persistence, ownership,
query, authorization, or audit semantics in this task.

# Required states

`overview.svg` must contain the accepted needs-attention desktop Overview in
light and dark plus compact demonstrations of:

- healthy current data;
- stale/no recent data with chart gaps, never zeros;
- partial authorization localized to the unavailable chart;
- safe backend failure localized to the failed chart with retry; and
- historical `v11` visibly selected with only v11-scoped data or explicit no
  data.

The alternate states must reuse the same dashboard anatomy; do not present four
simultaneous product dashboards.

# Narrow width

Update SM-01 in `mobile.svg` to stack the same information:

1. assessment;
2. requests, errors, and latency charts;
3. breached Drift then Eval;
4. Attention and canonical investigations; and
5. the Details disclosure for component state and recent activity.

No information, scope, action, subject, or freshness state disappears. Preserve
SM-02 and the Composition behavior unless a purely mechanical offset is needed.

# Definition correction

In `definition.svg`, render Raw Spec as a real closed disclosure with a visible
expand affordance. If an expanded example is shown, it must visibly expose a
collapse affordance and remain subordinate to the human-readable declaration.

# Visual scenarios

Execute one at a time:

1. Replace the status-panel wall with the compact assessment and three dominant
   operational trends.
2. Add publication-driven Drift and Eval trends with exact subject and scope.
3. Reduce Attention and investigation controls to one obvious next-action
   region.
4. Add the single optional custom-chart state without creating a builder.
5. Demonstrate honest healthy, stale, partial, failure, and selected-v11 states
   through the same chart anatomy.
6. Preserve the complete Overview at 390 × 844 by stacking and disclosure.
7. Correct Definition raw-spec disclosure.
8. Inspect every changed artboard in both themes at declared scale.

# RED, GREEN, REFACTOR

- **RED:** Capture the current `overview.svg` and SM-01. Record that they are
  text-dominant, provide only one minor trend, and omit narrow-width context.
- **GREEN:** Make the smallest SVG edits that produce the locked hierarchy and
  states using the existing Wyrd shell, chart grammar, tokens, and typography.
- **REFACTOR:** Remove duplicated status prose and panels. Reuse existing render
  anatomy; add no generator, component system, or dashboard framework.

# Verification

This is static visual-contract work. Do not run application tests, linters,
codegen, or the repository gate.

Required evidence:

- before/after captures of desktop Overview and SM-01;
- direct inspection of every changed light/dark artboard at 1440 × 1024 or
  390 × 844;
- visible chart units, latest values, freshness, thresholds, and non-color
  series distinction;
- exact Service version/range and subject identity on every chart;
- localized stale, partial, unauthorized, and failed states;
- no clipping, collision, illegible text, or uncontrolled overflow;
- `composition.svg` unchanged;
- `git diff --check`; and
- explicit human acceptance.

# Stop conditions

Stop if the mock requires a durable custom-chart contract, browser-derived
health or verdicts, a new Observe route, a new Card kind, changed publication
meaning, or edits outside the dedicated Service package.

# Required implementation skills

- `$wyrd-implement`
- `wyrd-ui`
