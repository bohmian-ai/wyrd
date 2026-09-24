# Wyrd Design System — Evidence Thread

The rules every Wyrd documentation and workbench surface obeys. This is the prose source of
truth; it pairs with:

- **`palette.json`** — the one canonical token source (machine-readable).
- **`gen-theme.mjs`** — projects `palette.json` into every consumer: `theme.css` (workbench),
  `docs/src/styles/wyrd-tokens.css` (docs), the `wyrd-ui` skill theme, and every Wyrd mark
  rendering (`logo.svg`, `logo-light.svg`, `app-icon.svg`, `docs/src/assets/wyrd-mark.svg`,
  `docs/public/favicon.svg`).
- **`components.json`** — per-component contracts (machine-readable; consumed by A2UI/agents).
- **`./renders/styleguide.html`** — the rendered visual reference.

Evidence Thread expresses Wyrd's product truth: an exact Declaration is connected to
observed behavior, a versioned Verifier, its Judgment, and the retained Evidence that makes
the result attributable. It is scientific provenance — archival finding aids and laboratory
accession records — not crime-scene, policing, or compliance theater.

There are two consumers and **one** design contract. The docs site is the *read* surface,
optimized for comprehension and task completion. The workbench is the *operate* surface,
optimized for dense inspection, comparison, and action. They share identity, token roles,
geometry, and state language; only density and layout composition differ.

In the workbench there is **one** set of standardized pieces: a piece is "standardized" only
when it exists as a contract entry, a real component in `src/lib/components`, and (for
catalog pieces) a registry entry in `src/lib/registry.ts`, and they agree (enforced by
`component-contracts.test.ts` and `registry.test.ts`). Contracts marked `status: "spec"` are
designed but not yet built.

---

## Tokens & modes

- Every color and geometry value is a token in `palette.json`. Never hardcode a hex in a
  consumer — use `var(--token)` (or the workbench's Tailwind `bg-*`/`text-*`/`border-*`
  utilities, which resolve per mode). No consumer owns a second palette.
- Regenerate after any token edit: `node brand/gen-theme.mjs` (from
  `crates/wyrd/wyrd-server/wyrd-ui`). `mise run check:tokens` runs the generator tests and
  fails on any projection drift. Never hand-edit a generated file.
- Every token carries a documented `role` and a literal value per mode; the generator
  rejects aliases (`var(--x)` values) and undocumented tokens.
- Light and dark are **two complete renderings of the same system**: same information,
  hierarchy, actions, geometry, and accessibility semantics. Neither is a fallback or an
  inversion.
- The workbench applies mode via `data-mode="light|dark"`; the docs site via
  `data-theme="light|dark"` on `:root`. Both are projections of the same values.

### Semantic roles

| Role | Token | Light | Dark | Meaning |
|---|---|---:|---:|---|
| Canvas | `--bg` | `#F3F6F5` | `#08100F` | application/page field |
| Surface | `--surface` | `#FFFFFF` | `#101B19` | primary panel and chrome |
| Surface secondary | `--surface-2` | `#E7EEEC` | `#182725` | subordinate regions, neutral controls |
| Surface hover | `--surface-hover` | `#EDF2F0` | `#1D2F2B` | neutral interaction feedback |
| Ink | `--text` | `#10201D` | `#EAF2F0` | primary text, strongest rule |
| Muted ink | `--muted` | `#53645F` | `#9AB0AB` | supporting text, metadata |
| Rule | `--border` | `#9AACAA` | `#39504B` | structural dividers, control borders |
| Rule soft | `--border-soft` | `#D7E1DE` | `#263B37` | row and subordinate dividers |
| Declare / primary | `--declare` | `#5036D5` | `#A894FF` | Declaration, links, selection, focus, primary action |
| Declare soft | `--declare-soft` | `#EAE6FF` | `#241F46` | selected/contextual declaration field |
| On declare | `--declare-ink` | `#FFFFFF` | `#08100F` | label on a `--declare` fill |
| Observe | `--observe` | `#007563` | `#67B891` | observed facts, Observation identity |
| Observe soft | `--observe-soft` | `#BDF5E7` | `#15352F` | selected/contextual observation field |
| Retained Evidence | `--evidence` | `#D7F33F` | `#D7F33F` | retained-evidence signal, mark spine |
| On evidence | `--evidence-ink` | `#10201D` | `#10201D` | label/outline on an `--evidence` fill |
| Success | `--ok` | `#187B45` | `#50DA83` | success, with a non-color cue |
| Warning | `--warn` | `#A65F00` | `#FFB455` | caution, with a non-color cue |
| Failure | `--danger` | `#B52828` | `#FF715E` | failed judgment or error, with a non-color cue |
| Failure soft | `--danger-soft` | `#F9E8E7` | `#3B1D1B` | failure context field |
| Code surface | `--code-bg` | `#0E1716` | `#050908` | code and terminal blocks |
| Code ink | `--code-text` | `#DCF5EE` | `#DCF5EE` | default code text |

### Color semantics

- **Indigo (`--declare`)** is Declaration and the primary action: links, selection, focus,
  the one strong action on a surface.
- **Green (`--observe`)** is observed fact and Observation identity.
- **Lime (`--evidence`)** is reserved for retained Evidence, the Wyrd mark's center spine,
  and direct interaction feedback that signifies evidence capture or retention (for example
  a "copied" or "retained" confirmation). It is never a general accent, secondary action,
  hover, or wash. It is a fill carrying `--evidence-ink`; it is never text on a light
  surface (1.3:1 on white).
- **Red, amber, and success green** keep failure, warning, and success meaning. They are
  never palette accents or chart series.
- No accent becomes a decorative page wash. The canvas stays a neutral mineral field in both
  modes: no full-screen indigo, lime, gradient, or dark-neon background.

---

## Type

Two faces, both shipped with each application (`@fontsource/familjen-grotesk`,
`@fontsource/fragment-mono`). Production never requests a third-party font service.

| Token | Face | Job |
|---|---|---|
| `--font-sans` | Familjen Grotesk | interface, navigation, headings, documentation reading |
| `--font-mono` | Fragment Mono | code, commands, identifiers, timestamps, measurements, compact metadata |

Fragment Mono is limited to genuinely machine-shaped values. Prose, labels, and headings are
never set in mono for flavor. Workbench components reference the `--fm` (mono) / `--fh`
(heading) shorthands set by `ModeProvider`.

---

## The Wyrd mark

The mark is the exact geometry of the README mark — two wings and a center spine on a
100×100 grid. `gen-theme.mjs` owns that geometry and renders it in palette colors:

- wings `--declare`, spine `--evidence`, outline `--text` (light) / `--bg` (dark);
- `logo-light.svg` for light surfaces, `logo.svg` for dark surfaces;
- `app-icon.svg`, `docs/src/assets/wyrd-mark.svg`, and `docs/public/favicon.svg` follow the
  viewer's color scheme, for places the page cannot pass a mode (favicons, the README).

Recolor, never redraw. A parent-company mark or an evidence-thread diagram never stands in
for the Wyrd mark in product chrome or favicons. Prose always uses the proper name **Wyrd**;
chrome reads **mark → `Wyrd`**.

---

## Geometry

Flat and ruled. The grammar is the same in both modes and at every width.

- **Radius:** 2px (`--r`). True circles only for dots and avatars.
- **Rules:** 1px solid. `--border` for structure and control borders, `--border-soft` for
  rows and subordinate dividers, `--text` for the page's single attention surface.
- **No decorative elevation.** No hard-offset shadows, glassmorphism, glow, scanlines, CRT
  effects, pixel treatments, faux terminal chrome, or AI gradients. State is carried by
  fill and rule, never by lifting or sinking a control.
- Functional overlays (a drawer, a dialog) may use a restrained scrim —
  `color-mix(in srgb, var(--bg) 72%, transparent)` — to preserve context.

### Rules are structure

`--border` is 2.0–2.4:1 against `--surface`/`--bg`: calm enough for long sessions, too quiet
to be the only thing identifying a control. So a control is never identified by its rule
alone: it also carries a visible label, a fill (`--surface-2`), or text, and focus draws a
2px `--declare` ring (≥4.5:1). The `pairs` contract records rules at `min: 2.0` with this
reason rather than hiding the exception.

---

## Never colour alone

Status hues can collapse under colour-vision deficiency, so **every meaning carries a second,
non-chromatic channel**:

- **Status carries a glyph and a word.** `Badge` renders `✓` / `!` / `✕` / `●` before its
  label; state blocks write the state out.
- **Thresholds carry a tick.** A threshold is a labeled dashed rule plus a positional tick.
- **Chart series carry a dash and marker.** Series use `--declare`, `--observe`, `--text`,
  `--muted` with distinct dash patterns and markers, mirrored in the legend.
- **Evidence connectors are real.** A thread between Declaration, Observation, Verifier,
  Judgment, and Evidence is drawn only when the interface shows a real causal path from
  server data — never as decoration, serial numbers, or custody marks.

### The contrast contract

`palette.json` carries a `pairs` block declaring every foreground/background pairing the
applications render, with the WCAG floor for its role (4.5 text, 3.0 non-text, 7.0 where we
hold AAA). `gen-theme.mjs` asserts all of them **in both modes** and fails the token gate on
a violation. Do not lower a `min` to make a failure pass. Fix the value, or correct the pair
if the role genuinely changed.

---

## Workbench composition

The workbench is effective through hierarchy, not decoration. Every page answers one
current question through one dominant work region. Supporting context stays subordinate;
selected detail appears only after selection. Empty space is preferable to filler panels.

- Use progressive disclosure instead of showing every state, explanation, and technical
  detail simultaneously. Complexity is earned by the user's action: opening a record,
  expanding a Claim, selecting a task, or requesting provenance.
- State a decision once at the highest useful level. Do not repeat the same conclusion in a
  banner, rail, summary card, and row badge.
- Lead with human-readable names and explanations. IDs, hashes, implementation nouns, and
  provider payload details are secondary metadata or drilldowns.
- Shared shell and controls repeat. Page composition follows the work being performed.
- Add only controls backed by the current workflow.

The authenticated desktop shell: a full-height sidebar owns the mark and `Wyrd` wordmark;
the topbar owns the product pill, breadcrumbs/context, tenant identity, principal menu, and
theme control. The sidebar exposes Home, Cards, Observe, Changes, and Query. Contextual
navigation stays inside the active workspace. Use a drawer when the selected record needs
substantial inspection; it preserves page context under a restrained scrim.

---

## Layout & responsiveness

Both surfaces are laptop/monitor-first and degrade gracefully to a narrow viewport without
clipping, hidden content, or a separate mobile product.

- **Components are container-robust, not viewport-coupled.** Prefer `flex-wrap`,
  `min-width: 0`, `overflow-x: auto` on dense tabular content, and container queries
  (`container-type: inline-size`). Reserve viewport `@media` for page-level layout.
- **Dense data scrolls, it doesn't crush.** Tables, trace waterfalls, and code keep their
  widths inside a focusable, labeled horizontal scroll region.
- **Navigation stays reachable** and primary actions keep touch-sized targets on narrow
  screens.
- **Geometry is invariant across sizes.** Only layout flow changes.

---

## Data vs. presentation

The server owns **meaning**; the UI owns **presentation**. Wyrd is agent-first and
headless, so the API serves the CLI, MCP, other-language clients, and the A2UI renderer,
not just these surfaces.

- **The server owns, and the UI must not re-derive:** canonical values, units, rounding
  that changes truth, derived status (`pass`, `drifted`), judgments, lineage, thresholds and
  policy, sorting, pagination, aggregation.
- **The UI owns:** display formatting (`1400` → `"1.4k"`), bar widths, relative time,
  truncation, and verdict→token mapping.

The server sends `value: 0.79, threshold: 0.8, pass: false`; the UI decides the value
renders as a failure. Typed-domain components take raw semantic values and humanize them
via `src/lib/format.ts`. `KpiTile` is the deliberate exception: its caller formats the
heterogeneous `value`.

---

## Building a new component

1. Add its contract to `components.json` with `status: "spec"` (anatomy, tokens, variants,
   altitude, rules).
2. Build it in `src/lib/components` against the tokens (no hardcoded hex; 1px rules; 2px
   radius; no elevation).
3. Register it in `src/lib/registry.ts` when it is a catalog piece and flip its contract
   `status` to `built`.
4. Add it to `/styleguide` and write a test.

---

## What this system retired

Recorded so nobody re-adds it by accident: the parent-company mark and wordmark in Wyrd
chrome; the warm-paper and blue-cast palettes; the blue/lime "two voices" and lime secondary
actions; the client/server/control plane colors; the five-pixel radius, 2–3px borders, and
hard-offset shadow dial with its lift/sink motion; Archivo, Space Grotesk, JetBrains Mono,
Fraunces, and the arcade/pixel faces; and runtime Google Fonts imports.
