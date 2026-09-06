# Wyrd Design System — Doctrine (bohmian)

The rules every Wyrd UI piece obeys. This is the prose source of truth; it pairs with:

- **`palette.json`** — canonical tokens (machine-readable; generates `theme.css`).
- **`components.json`** — per-component contracts (machine-readable; consumed by A2UI/agents).
- **`./renders/styleguide.html`** — the rendered visual reference.

Derived from `.dev/assets/bohmian/bohmian-brand-mockups-v4.html`, the approved brand
direction.

There are two consumers and **one** set of standardized pieces: the SvelteKit workbench
(imports components from `src/lib/components`) and an eventual dynamic-layout / A2UI renderer
(resolves the same components by name from `src/lib/registry.ts` using `components.json`).
A piece is "standardized" only when it exists as all three — a contract entry, a real
component, and a registry entry — and they agree (enforced by `registry.test.ts`).
Contracts marked `status: "spec"` are designed but not yet built, and are excluded from
that parity check until they are.

---

## The two voices

The palette has two accent voices, and every accent decision starts by picking one.

- **Blue** is primary — Wyrd's identity, the one strong action, links, selection, ramps.
- **Lime** is secondary — the second voice: secondary actions, the client/runtime plane,
  second-voice emphasis. Never decoration.

---

## Tokens & modes

- Every color/geometry value is a token in `palette.json`. Never hardcode hex in a component
  — use `var(--token)` (or the Tailwind `bg-*`/`text-*`/`border-*` utilities the generator
  emits, which resolve per mode).
- Light and dark are **the same geometry**, different palette/atmosphere. Light is warm
  paper (`#f2f0ea`), dark is a flat blue-cast near-black (`#0b0c12`) — no CRT, scanlines,
  vignette, glow, or phosphor.
- One geometry, **two intensities**. Light renders the geometry in ink: near-black borders
  and hard shadows carry the neobrutalist signature. Dark renders the same geometry
  quietly: `--border` sits near 2:1 against `--surface` — enough for panel structure to
  read at a glance during long sessions, never light-mode loudness. Do not "fix" dark mode
  by pushing its borders toward ink; the workbench is stared at for hours.
- Mode is applied via a `data-mode="light|dark"` attribute (see `ModeProvider.svelte`). It
  cascades, so nested subtrees can pin a mode (the styleguide shows both at once).
- Regenerate `theme.css` after any token edit: `node brand/gen-theme.mjs`. Drift is
  checked with `--check`.
- **No render may restate a hex.** `renders/*.html` link `../theme.css` and read
  `var(--token)`. The previous system's reference HTML duplicated the palette in a JS array
  and silently drifted from `palette.json`; that failure mode is closed by construction.

### Frozen foreground (readability)

The workbench is an analytics surface used for hours. Foreground is locked for calm
readability, not loudness: `--text` = `#101014` light (~17:1) / `#e6e4da` dark (~14:1).
Do not "brighten" or re-tune these.

---

## Type

Four faces. Each has one job.

| Token | Face | Job |
|---|---|---|
| `--font-sans` | Archivo | body copy, prose, form labels |
| `--font-display` | Space Grotesk 700 | headings, KPI numerals |
| `--font-mono` | JetBrains Mono | **all data** — tables, badges, IDs, code, nav, metrics |
| `--font-serif` | Fraunces | bohmian wordmark; loud altitude and marketing |

Fraunces appears in workbench chrome only for the bohmian wordmark. It is never used for
workbench headings, prose, navigation, or data. Fraunces is the company voice — hero
display, landing pages, and the wordmark — and mixing it into quiet product content
collapses the distinction between "this is bohmian talking" and "this is your system
reporting".

There is no italic serif in the token set. The workbench never needs one, and the marketing
italic voice (STIX Two Text) is loaded per-page by the marketing surface, not shipped as a
product token.

Dense data is mono at 8.5–12px. Column headers and badges are 8.5px uppercase with ~0.5px
letter-spacing. Components reference the `--fm`/`--fh` shorthands set by `ModeProvider`.

### Wordmark

**"bohmian" is lowercase always** — including at the start of a sentence, including in a
heading, including as the first word of a title. Set in `--font-serif` (Fraunces) at weight
600.

The wordmark is **typeset, not an asset**. There is no `wordmark.svg`: outlining Fraunces
is not hand-authorable, and a `<text>` element would smuggle a webfont dependency into a
file that is supposed to be self-contained. Render it as text.

App chrome reads, left to right: **mark → `bohmian` → product pill**.

```
[◈] bohmian [ WYRD ]   registry / cards / payments-svc
```

The mark is a serif `b` monogram with the periwinkle pilot wave weaving behind the stem and
in front of the bowl, carrying a lime particle.

**`logo.svg` is the default at every size, on both light and dark surfaces.** It carries its
own dark field, so it reads as a tile rather than as ink — the same way an app icon does —
and mockups-v4 uses exactly this file on its dark company surface *and* its light product
chrome. Do not swap it per mode; a mark that changes with the theme stops being a constant.

`logo-light.svg` is the exception, not the light-mode counterpart: use it only where the
mark must sit **directly on a light field with no tile** — print, a light-only embed, a
favicon against light browser chrome. `app-icon.svg` is the same mark in a 228px-radius
tile for launchers and favicons.

All three are self-contained — no filters, no font dependency, no raster.

---

## Geometry (non-negotiable)

Unchanged from the previous system. The brand changed; the build did not.

- **Radius: 5px** (`--r`). Locked — not zero, not larger. True circles (`50%`) only for dots
  and avatars.
- **Borders:** 2px solid normal, 3px solid for hero/feature containers, 2px **dashed** for
  dividers inside cards. Always present, always `--border` — never gray, hairline, or
  low-contrast.
- **Shadows:** hard-offset, **zero blur**, color `--shadow`. The dial:
  - `3px 3px 0 0` — quiet (workbench default)
  - `6px 6px 0 0` — raised (panels, stat blocks)
  - `10px 10px 0 0` — loud (hero only)
  - No blur, glassmorphism, soft elevation, `filter: drop-shadow()`, or glow.

---

## Altitude system

Three discrete registers. They never blend on one screen.

- **Quiet** — the workbench default. 3px shadows, restrained accent, calm. Cards, tables,
  filters, toolbars, nav. This is where users live.
- **Raised** — drilldown drawers/panels. 6px shadow + a 4px colored **top-bar** signalling
  the panel's kind (or status, see below).
- **Loud** — hero / landing only. 10px shadow, tricolor rail, serif display type, blue CTA.
  Never appears beside the workbench.

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
- Shared shell and controls repeat. Page composition follows the work being performed; do
  not impose one universal table-and-rail template across unrelated product areas.
- Add only controls backed by the current workflow. Do not add speculative saved views,
  configuration, actions, or navigation to make a surface look complete.
- Change Requests must remain understandable to Product, Data Science, and Engineering.
  Expert Observe, Eval, Drift, and Query screens prioritize their actual operators; they
  need not flatten earned technical detail for every audience.

The authenticated desktop shell has one stable composition: a full-height sidebar owns the
mark and lowercase bohmian wordmark; the main topbar begins beside it and owns the Wyrd
product pill, breadcrumbs/context, tenant identity, principal menu, and theme control. The
sidebar exposes exactly Home, Cards, Observe, Changes, and Query as primary navigation.
Contextual navigation stays inside the active workspace.

Use contextual rails for compact supporting facts. Use a raised drawer when the selected
record needs substantial inspection. A drawer preserves recognizable page context under a
restrained scrim and contains one primary detail region rather than a new dashboard of
equally weighted panels.

---

## Layout & responsiveness

Wyrd is a **dense developer workbench**, designed **laptop/monitor-first** (the target user
is on a cluster, reading traces and tables). We do **not** redesign for phones — but every
piece must **degrade gracefully** down to a narrow viewport, never clip or overflow its frame.

- **Floor: ~768px** renders *well*. Below that, surfaces **stack and scroll** — they stay
  usable, not pretty. No separate mobile layout, no hamburger nav, no hidden content.
- **Components are container-robust, not viewport-coupled.** A component must not assume its
  container is wide — it's composed into cards, panels, and (later) A2UI layouts of unknown
  width. Prefer intrinsic robustness — `flex-wrap`, `min-width: 0` so flex children can
  shrink, `overflow-x: auto` on dense tabular content — and **container queries**
  (`container-type: inline-size`) over viewport `@media` for component-level adaptation.
  Reserve viewport `@media` for true page-level layout (route pages, the styleguide harness).
- **Dense data scrolls, it doesn't crush.** Multi-column tables, the trace waterfall, and the
  lineage strip keep their widths and gain a horizontal scroll region rather than compressing
  to illegibility.
- **The app shell collapses, it doesn't disappear.** Below the floor the `Shell` stacks the
  sidebar above the content; nothing is removed.
- **Geometry is invariant across sizes.** 5px radius, 2px/3px borders, and the hard-offset
  shadow dial never change with screen width — only layout flow does.

---

## Color: blue primary, lime secondary

**Blue `#2745e8` / periwinkle `#7a8cff` is primary** — Wyrd's identity, links, big stats,
selected refs, the `llm` span kind, hero, the one strong action, and **all
sequential/heat ramps**.

**Lime `#c5f23c` is the secondary accent** — the second voice after blue. It carries
secondary action fills, the client/runtime plane, and secondary emphasis. It marks a
genuine second action beside a blue primary; it is never a wash, a hover, or decoration.

### Bright lime is a fill, never a mark

`--lime` is **1.30:1 on white**. It cannot be text, and it cannot be a thin bar, on any
light surface. Use `--lime-text` (`#4a7a06` light / `#c5f23c` dark) for anything that
needs lime as a *mark* rather than a *field*. The client plane is defined this way.

### Two ink traps, both verified

`--ink-on-fill` and `--brand-btn-ink` are near-black. They are only safe on a **bright**
fill, and two plausible-looking pairings fail badly in dark mode. Both are AA-passing in
light, which is exactly why they are easy to ship broken.

| Pairing | Light | Dark | Use instead |
|---|---|---|---|
| `--brand-btn-ink` on `--brand` | 6.77:1 | **1.23:1** | `--brand-btn` as the fill (6.53:1 dark) |
| `--ink-on-fill` on `color-mix(--lime 26%, --surface)` | 17.6:1 | **2.12:1** | `--text` as the label (7.04:1 dark) |

The rule behind both: `--brand` is a *saturated fill* in light but a *dim navy panel* in
dark, and a lime tint mixed against `--surface` lands on dark olive in dark. Reach for
`--brand-btn` when you need a fill that carries near-black ink, and let `--text` label any
tinted band — the band already carries the identity.

### Brand blue

Selection/hover wash is `--brand-soft`, never lime.

Two tokens collapse per mode, by design: `--brand` == `--brand-strong` in light, and
`--brand` == `--brand-soft` in dark. They stay separate tokens because their *roles* differ —
`--brand` is a saturated fill, `--brand-soft` is a wash — and those roles diverge again the
moment either mode is re-tuned.

---

## Never colour alone

A Brettel/Viénot simulation across this palette found five pairs that collapse under
deuteranomaly (~6% of men; this workbench's audience skews heavily male):

| pair | normal | deuteranopic | ratio |
|---|---|---|---|
| **`--ok` vs `--danger`** | `#2f9e44` / `#d2402a` | `#898948` / `#828219` | **1.11:1** |
| `--server-bar` vs `--warn` | same hex | same hex | 1.00:1 |
| `--client-bar` vs `--server-bar` | `#4a7a06` / `#ec8b14` | — | 1.18:1 |
| `--control-bar` vs `--brand-strong` | `#2f6f9e` / `#2745e8` | `#61619f` / `#3e3ee8` | 1.22:1 |

Pass and fail are the same colour. **Every meaning in this system therefore carries a
second, non-chromatic channel.** Three rules follow, and they are not optional polish:

- **Status carries a glyph.** `Badge` renders `✓` / `!` / `✕` before its label, matching
  what `DriftPanel`'s check list (`.chk .ic`) already did.
- **Thresholds carry a tick.** A metric bar scored against a threshold draws a 2px rule at
  the threshold position, so "did this pass" is a positional read. Colour alone made this
  judgement impossible for a deuteranopic reader.
- **Chart series carry a dash.** Multi-series lines vary `stroke-dasharray` (p50 solid,
  p95 `6 3`, p99 `2 3`) and the legend mirrors the pattern — `--control-bar` and
  `--brand-strong` are otherwise indistinguishable.

The plane colours (the Line) are exempt from needing a second channel only because every
one of their use sites — trace ops, sidebar items, tree leaves, waterfall rows, Dist
legends — already renders a text label beside the bar. If a new surface ever shows a plane
colour with no adjacent label, it needs a second channel too.

### The contrast contract

`palette.json` carries a `pairs` block declaring every foreground/background pairing the
system actually renders, with the WCAG floor for its role (4.5 text, 3.0 non-text, 7.0
where we hold AAA). `gen-theme.mjs` asserts all of them **in both modes** and fails the
build on a violation. `fill` may be a token name or `{mix:[a,b],pct}` for a `color-mix()`
background, because both shipped bugs involved a mixed background.

Do not lower a `min` to make a failure pass. Fix the value, or correct the pair if the
role genuinely changed.

---

## Color semantics — "the Line"

Three plane colors encode the Wyrd architecture. Use them for identity, not decoration.

| Color | Token(s) | Plane |
|-------|----------|-------|
| Lime  | `--client` / `--client-bar` | client / runtime (agent, data, live events) |
| Amber | `--server` / `--server-bar` | server / enterprise (eval, services, compliance) |
| Steel | `--control` / `--control-bar` | control / deploy (policy, audit, governance) |

The client plane is **lime, mode-split**: `#4a7a06` light / `#c5f23c` dark. Raw lime is
1.30:1 on white and fails as a 3px bar there, so light mode takes the darkened value —
the same one `--lime-text` uses. This is deliberately the *same colour* as the secondary
accent, not a near-miss of it.

> A previous revision used olive `#8aa829` here, to keep the client plane distinct from
> the secondary accent. It was
> withdrawn on measurement: 2.72:1 on white (below the 3:1 non-text floor), only 1.07:1
> luminance separation from amber, and — decisively — **0.7° of hue from `--lime`**. It was
> not a distinct colour, it was lime pretending to be one.

`*-bar` variants are darkened on light mode for contrast (lime/amber/steel can't sit as
text or thin bars on white). Applied as a **3px left-bar** on rows/leaves to mark kind,
always beside a text label.

**Known weakness, accepted:** `--server-bar` is 2.54:1 on white, under the 3:1 non-text
floor. Its hex is shared with `--warn`, so darkening it shifts the warning colour across
every table and chart. The `pairs` contract records this at `"min": 2.5` with a pointer
here rather than silently passing it — the exception is visible, not hidden.

**Watch the steel/blue adjacency.** Control-plane steel `#2f6f9e` and brand blue `#2745e8`
are distinguishable side by side, but they are the closest pair in the palette. The control
plane marks *what kind of thing this is*; brand blue marks *identity, selection, and the one
primary action*. Never reach for `--control-bar` because you wanted "a blue".

### Span kinds (traces)

Fixed mapping in waterfalls and trace tables: `agent` → `--client-bar`, `llm` →
`--brand-strong`, `tool` → `--server-bar`, `retrieval` → `--control-bar`, `error` →
`--danger`.

---

## Signal rules

- **Accent is signal, never decoration.** Blue and lime mark only: the one primary action,
  selected rows, active filters, status, kind left-bars, and product identity. They never
  tint body text or fill backgrounds at large.
- **Status overrides kind.** Normally a panel's top-bar shows its span/record kind. When the
  thing is in an alert state (eval failed, feature drifted), the top-bar turns `--danger`
  regardless of kind.
- **Status is state, not flavor.** `--ok` / `--warn` / `--danger` mean healthy / caution /
  failure — don't use them as palette accents.

---

## Data vs. presentation

The server owns **meaning**; the UI owns **presentation**. These are different
responsibilities and the boundary is load-bearing — Wyrd is agent-first and headless, so
the API serves the CLI, MCP, other-language clients, and (later) the A2UI renderer, not
just this workbench.

- **The server owns, and the UI must not re-derive:** canonical values, units, rounding
  that changes truth, derived status (`pass`, `drifted`), thresholds and policy, sorting,
  pagination, aggregation. Same number, same verdict, everywhere.
- **The UI owns, and the server must not bake into the wire:** display formatting
  (`1400` → `"1.4k"`, `4210` → `"4.21s"`, `0.18` → `"$0.18"`), bar widths, relative time,
  truncation, and verdict→color/altitude mapping. These depend on the rendering medium and
  viewport — context the server doesn't have. If the server pre-formatted `"1.4k"`, every
  non-UI client would have to parse a lossy string back into a number.

The seam: the server sends `value: 0.79, threshold: 0.8, pass: false`; the UI decides the
value renders red. The **threshold** is policy (server); the **redness** is presentation (UI).

**Prop convention.** Typed-domain components take **raw semantic values** — numbers,
milliseconds, `0–1` scores, enums — and humanize them internally via `src/lib/format.ts`
(`fmtCount`, `fmtDuration`, `fmtCost`). Strings are only for genuinely opaque values: IDs,
names, free-form attribute pairs, ISO timestamps shown as-is, and arbitrary key/value rows
(`d-kv` lists). A pre-formatted display string in a typed prop (`tokens: "1.4k"`) is a smell
— it pushes a presentation decision up the call chain and breaks consistency. The generic
`KpiTile` is the one deliberate exception: its `value` is heterogeneous across uses
(latency, spend, rate, count), so the caller formats it with the same `format.ts` helpers
at the call site.

---

## Building a new component

1. Add its contract to `components.json` with `status: "spec"` (anatomy, tokens, variants,
   altitude, rules).
2. Build it in `src/lib/components` against the locked tokens (no hardcoded hex; correct
   altitude shadow; 5px radius; 2px/3px/dashed borders per role).
3. Register it in `src/lib/registry.ts` and flip its contract `status` to `built`.
4. Add it to `/styleguide` and write a test. `registry.test.ts` enforces contract ↔ registry
   parity for `built` entries.

---

## What this system dropped

Recorded so nobody re-adds it by accident.

- **The arcade register.** `Press Start 2P` and `VT323`, the `--sky-*` / `--grid` / `--star`
  / `--spr-d` scene tokens, the 2×2 ordered dither, the 8-bit `notch` clip-path, and the
  pixel agent portrait. The bohmian brand is editorial, not retro; the two registers cannot
  share a product.
- **Brand violet.** `--rune*` is gone entirely, renamed to `--brand*` and revalued to blue.
- **`Archivo Black`.** Replaced by Space Grotesk 700, which holds up better at KPI sizes.
- **`wordmark.svg`.** The wordmark is typeset (see above); the old asset was also off-palette.
- **The `rune` button variant.** It was the violet twin of `primary`; `primary` is now blue
  and the second variant slot is the lime `secondary`.
