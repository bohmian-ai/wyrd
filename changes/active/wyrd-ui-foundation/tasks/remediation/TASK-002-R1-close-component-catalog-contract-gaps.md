---
id: TASK-002-R1
title: Close component catalog contract and accessibility gaps
kind: remediation
status: review
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-128, REQ-130, REQ-131, INV-008, INV-021, AC-010]
depends_on: [TASK-002]
parent_task: TASK-002
remediates: [FIND-TASK-002-1, FIND-TASK-002-2, FIND-TASK-002-3, FIND-TASK-002-4, FIND-TASK-002-5]
---

# Outcome and value

Make the shipped component catalog truthful and safe: catalog components accept
only their declared semantic inputs, chart time and state contracts agree with
their Svelte implementations, status and series meaning never depend on color,
and repeated native controls retain correct accessible labels.

# Owner, scope, and consumers

Own only the affected Wyrd UI foundation files:

- `wyrd-ui/brand/components.json`;
- `wyrd-ui/src/lib/components/Button.svelte`, `Select.svelte`, and their focused
  tests;
- `wyrd-ui/src/lib/components/charts/ChartPanel.svelte`, `Line.svelte`,
  `Spark.svelte`, and their focused tests;
- `wyrd-ui/src/lib/components/component-contracts.test.ts`;
- the homepage's temporary mode-toggle control and directly affected style-guide
  examples; and
- `changes/active/wyrd-ui-foundation/evidence/TASK-002-component-foundation.md`
  for corrected final evidence.

Affected consumers are the homepage, the paired light/dark style guide, and the
existing Card, Observe, Service, Home, and Change Request mock consumers named
for `Button`, `Select`, `ChartPanel`, `Line`, and `Spark` in
`brand/components.json`.

# Decisions and invariants

- `Button` MUST expose only the catalog-declared semantic inputs. Do not spread
  the full native anchor or button attribute sets. The temporary homepage mode
  toggle uses a local native button; callbacks, arbitrary classes, and styles do
  not enter the catalog component API.
- Keep the current registry and do not build a renderer, schema DSL, generic prop
  parser, or plugin boundary. `resolve()` has no authored-view consumer in this
  change; remediation closes the concrete component contract instead of adding
  speculative infrastructure.
- Align `ChartPanel` contract requiredness with its implementation:
  `latestValue` is optional so unavailable states need no placeholder value, and
  every supplied freshness/range stamp carries a required machine-readable
  `at` value. Remove the plain-text timestamp fallback.
- Replace `Line`'s string-only time ticks with JSON-safe display label plus
  timestamp values and render `<time datetime>` semantics for each exposed time
  tick. Preserve the current visible SVG treatment and keyboard tooltip flow.
- `Spark` is neutral geometry only. Remove the unused `sentiment` input and its
  status colors; add status meaning later only when a real consumer requires it
  and supplies a non-color signal.
- Use Svelte's installed per-instance ID facility for `Select`; `name` remains
  the form field name and MUST NOT determine DOM identity.
- `Line` supports at most four series, matching its four fixed non-color styles.
  Reject a fifth series explicitly rather than repeating a style or silently
  dropping data. Do not build a style generator without a real fifth-series
  consumer.
- Preserve current light/dark structure, native Select behavior, URL-backed form
  submission, chart tooltip keyboard behavior, trusted chrome exclusion, and
  all unaffected component contracts.

Authority: `AGENTS.md` §§11–12; `architecture/agent-rules.md` gate-integrity
rules; `architecture/references/languages/spec-driven-development.md` TDD and
remediation rules; approved spec revision 6 REQ-128, REQ-130, REQ-131,
INV-008, INV-021, and AC-010; original TASK-002 locked prop and catalog
decisions.

# Ordered test scenarios

Execute each scenario through RED, GREEN, and REFACTOR before starting the next:

1. Add a catalog-contract regression that detects `Button` event handlers,
   arbitrary style/class inputs, native rest-attribute widening, and undeclared
   prop drift. Confirm it fails on the current `HTML*Attributes` intersection,
   then narrow the component and replace the homepage callback use outside the
   catalog component.
2. Add contract and render regressions proving unavailable `ChartPanel` states
   need no `latestValue`, every supplied stamp has a machine-readable instant,
   and time-axis labels expose `<time datetime>` semantics. Align the manifest,
   `ChartPanel`, `Line`, examples, and tests.
3. Add a regression proving `Spark` has no status-semantic color input, then
   remove `sentiment` from the manifest, component, examples, and tests while
   preserving its neutral geometry.
4. Render two `Select` instances with the same form name. Confirm the current
   duplicate IDs make the second label resolve to the first control, then give
   each instance a unique ID and assert both label/control relationships.
5. Prove four `Line` series have four distinct dash-and-marker identities and a
   fifth is rejected. Add the smallest explicit ceiling without changing the
   existing three-series presentation.

# Red-Green-Refactor expectations

- **RED:** Run the one focused file for the active scenario and record the
  expected failure against candidate `2d2aff2e61fcc752a8df7e72dace9a22416ac150`.
- **GREEN:** Make only the component, manifest, consumer, and test changes needed
  for that scenario; rerun it and all previously green remediation scenarios.
- **REFACTOR:** Consolidate only duplicated local test setup. Do not introduce a
  catalog parser, renderer, validation framework, dependency, or generalized
  chart system.

# Exact focused verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/components/Button.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/components/component-contracts.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/components/charts/ChartPanel.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/components/charts/Spark.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/components/charts/Line.test.ts
```

# Broader verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
git diff --check
```

No server journey is required: this remediation changes local static component
contracts and browser accessibility behavior without a server or durable-state
boundary. Reinspect `/styleguide` in both modes and its 340px container after
the broader checks; update captures only if the visible output changes.

# Completion evidence

Record:

- one RED and GREEN result per ordered scenario;
- the final exact and broader command results with the current test count;
- manifest/Svelte/registry agreement for every changed catalog component;
- the two-instance Select label/control assertion;
- machine-readable chart time evidence;
- neutral Spark and four-series Line boundary evidence; and
- whether the existing paired-mode and narrow-container captures remained
  accurate or were replaced.

# Non-goals and stop conditions

- Do not add dependencies, a renderer, a schema DSL, a universal chart, more
  series styles, or a reusable callback/action abstraction.
- Do not change trusted shell, tenant, session, route, or data-fetch ownership.
- Do not weaken the existing catalog boundary tests or accessibility checks.
- Stop for specification revision if closure requires executable catalog props,
  browser-owned data loading, changed status meaning, or another material
  behavior not authorized by revision 6.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.

# Execution evidence — 2026-09-04

Readiness: READY, no blocking findings. Approved revision 6, parent task,
consumer ownership, dependencies, ordered scenarios, command precision, and
local browser evidence are sufficient without a material decision.

Implementation: COMPLETE against `2d2aff2e61fcc752a8df7e72dace9a22416ac150`.
All five ordered RED/GREEN cycles completed: Button native widening (3 RED
failures → 6 passing tests); chart state/time contract (2 RED failures → 49
cumulative passing tests); Spark sentiment (1 → 51); duplicate Select IDs
(1 → 52); fifth Line series (1 → 54). Refactoring was limited to a local
Line timestamp fixture helper and fixing the existing catalog prop inspection.

The five exact focused commands above pass individually (6, 19, 15, 2, and 12
tests respectively). The full UI suite passes 78 tests in 15 files;
Svelte checking reports 0 errors and 0 warnings; build, token drift check,
and `git diff --check` pass. Production-browser inspection covers both modes,
340px containers, 768px stacking, hydrated Select labels, timestamped keyboard
tooltips, and the homepage mode toggle. Existing captures remain accurate.

Full obligation mapping, command results, bounded test corrections, and visual
inspection details are in
[component foundation evidence](../../evidence/TASK-002-component-foundation.md#remediation-verification-task-002-r1-2026-09-04).
No dependencies, registry membership, server boundary, or material behavior
outside the remediation changed. The user subsequently authorized a commit.
Ready for cumulative
`$wyrd-task-review`; implementation completion does not approve the task.
