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
  `crates/wyrd/wyrd-server/wyrd-ui`). `mise run codegen:check` fails on projection
  drift. Run generator regressions with `mise exec -- node --test
  brand/gen-theme.test.mjs` from that directory. Never hand-edit a generated file.
- Every token carries a documented `role` and a literal value per mode; the generator
  rejects aliases (`var(--x)` values) and undocumented tokens.
- Light and dark are **two complete renderings of the same system**: same information,
  hierarchy, actions, geometry, and accessibility semantics. Neither is a fallback or an
  inversion.
- The color values are the palette approved on 2026-09-26: cool paper and white in light
  mode, blue-black canvas and lighter panels in dark mode, cobalt (light) / periwinkle (dark)
  as the primary accent, and lime as the observation and evidence signal. Lime, green, amber,
  and red are the same hue in both modes; only the primary accent and the neutrals shift.
  Code blocks sit on the dark panel field in both modes, lifted above the dark canvas.
  Layout, type, geometry, and interaction behavior remain shared.
- The workbench applies mode via `data-mode="light|dark"`; the docs site via
  `data-theme="light|dark"` on `:root`. Both are projections of the same values.

### Semantic roles

| Role | Token | Light | Dark | Meaning |
|---|---|---:|---:|---|
| Canvas | `--bg` | `#f5f6f8` | `#0b0c12` | application/page field |
| Surface | `--surface` | `#ffffff` | `#12141c` | primary panel and chrome |
| Surface secondary | `--surface-2` | `#eef0f4` | `#1a1d28` | subordinate regions, neutral controls |
| Surface hover | `--surface-hover` | `#e9ecf2` | `#1f2230` | neutral interaction feedback |
| Ink | `--text` | `#101014` | `#e6e4da` | primary text, strongest rule |
| Muted ink | `--muted` | `#62626d` | `#8f92a5` | supporting text, metadata |
| Rule | `--border` | `#d9dde5` | `#252838` | structural dividers, control borders |
| Rule soft | `--border-soft` | `#e8ebf0` | `#1c1f2c` | row and subordinate dividers |
| Declare / primary | `--declare` | `#4d5ef0` | `#7a8cff` | Declaration, links, selection, focus, primary action |
| Declare soft | `--declare-soft` | `#eceefe` | `#1a2040` | selected/contextual declaration field |
| On declare | `--declare-ink` | `#ffffff` | `#0b0c12` | label on a `--declare` fill |
| Observe | `--observe` | `#c5f23b` | `#c5f23b` | observed facts, Observation identity |
| Observe soft | `--observe-soft` | `#f3fce0` | `#394523` | selected/contextual observation field |
| Retained Evidence | `--evidence` | `#c5f23b` | `#c5f23b` | retained-evidence signal, mark spine |
| On evidence | `--evidence-ink` | `#0b0c12` | `#0b0c12` | label/outline on an `--evidence` fill |
| Success | `--ok` | `#3dbe5a` | `#3dbe5a` | success, with a non-color cue |
| Warning | `--warn` | `#f0a13c` | `#f0a13c` | caution, with a non-color cue |
| Failure | `--danger` | `#e2484d` | `#e2484d` | failed judgment or error, with a non-color cue |
| Failure soft | `--danger-soft` | `#fbe9ea` | `#2b1a22` | failure context field |
| Code surface | `--code-bg` | `#12141c` | `#12141c` | code and terminal blocks |
| Code ink | `--code-text` | `#e6e4da` | `#e6e4da` | default code text |
| Code comment | `--code-muted` | `#8f92a5` | `#8f92a5` | code comments and punctuation |
| Code keyword | `--code-keyword` | `#7a8cff` | `#7a8cff` | keywords and storage |
| Code string | `--code-string` | `#c5f23b` | `#c5f23b` | strings and literal values |
| Code number | `--code-number` | `#f0a13c` | `#f0a13c` | numbers, constants, function names |
| Strong weight | `--weight-strong` | `700` | `600` | headings, labels, emphasis; lighter in dark, where light ink renders heavier |

### Color semantics

- **Cobalt (`--declare`)** is Declaration and the primary action: links, selection, focus,
  the one strong action on a surface.
- **Lime (`--observe`)** is observed fact and Observation identity. It is the same lime in
  both modes and is never text on a light field: render it as a fill carrying
  `--evidence-ink`, a chart line, or a two-pixel underline beneath ordinary ink.
- **Lime (`--evidence`)** marks retained Evidence, the Wyrd mark's center spine,
  and direct interaction feedback that signifies evidence capture or retention (for example
  a "copied" or "retained" confirmation). `--observe` shares this hue in both modes, so
  observation and evidence must always be distinguished by labels and context. Lime is never
  a general accent, secondary action, hover, or wash. It is a fill carrying `--evidence-ink`;
  it is never text on a light surface (1.3:1 on white). As a line it is 1px.
- **Red, amber, and success green** keep failure, warning, and success meaning and are the
  same hue in both modes. Status labels always carry a glyph or word and a two-pixel colored
  border; the hue alone is not the contrast contract. Amber and red are not palette accents;
  amber may serve as a third chart series after cobalt and lime.
- No accent becomes a decorative page wash. The canvas stays cool paper or blue-black:
  no full-screen cobalt, lime, gradient, or neon background.

---

## Type

Two faces, both shipped with each application (`@fontsource/familjen-grotesk`,
`@fontsource/fragment-mono`). Production never requests a third-party font service.

| Token | Face | Job |
|---|---|---|
| `--font-sans` | Familjen Grotesk | interface, navigation, headings, documentation reading |
| `--font-mono` | Fragment Mono | code, commands, identifiers, timestamps, measurements, compact metadata |

Fragment Mono is limited to genuinely machine-shaped values. Prose, labels, and headings are
never set in mono for flavor. Strong text uses `--weight-strong` (700 light, 600 dark), never a
literal bold weight. Workbench components reference the `--fm` (mono) / `--fh`
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
chrome; lime secondary actions; the client/server/control plane color roles; the five-pixel radius, 2–3px borders, and
hard-offset shadow dial with its lift/sink motion; Archivo, Space Grotesk, JetBrains Mono,
Fraunces, and the arcade/pixel faces; and runtime Google Fonts imports.
