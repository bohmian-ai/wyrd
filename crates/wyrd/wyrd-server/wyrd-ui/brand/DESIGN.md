# Wyrd Design System — Doctrine

The rules every Wyrd UI piece obeys. This is the prose source of truth; it pairs with:

- **`palette.json`** — canonical tokens (machine-readable; generates `theme.css`).
- **`components.json`** — per-component contracts (machine-readable; consumed by A2UI/agents).
- **`../../../../.dev/assets/wyrd-ui-source-of-truth.html`** — the rendered visual reference.
- **`../../../../.dev/assets/wyrd-workbench-mock.html`** — the assembled product view.

There are two consumers and **one** set of standardized pieces: the SvelteKit workbench
(imports components from `src/lib/components`) and an eventual dynamic-layout / A2UI renderer
(resolves the same components by name from `src/lib/registry.ts` using `components.json`).
A piece is "standardized" only when it exists as all three — a contract entry, a real
component, and a registry entry — and they agree (enforced by `registry.test.ts`).

---

## Tokens & modes

- Every color/geometry value is a token in `palette.json`. Never hardcode hex in a component
  — use `var(--token)` (or the Tailwind `bg-*`/`text-*`/`border-*` utilities the generator
  emits, which resolve per mode).
- Light and dark are **the same geometry**, different palette/atmosphere. Dark mode is **flat
  near-black** — no CRT, scanlines, vignette, glow, or phosphor.
- Mode is applied via a `data-mode="light|dark"` attribute (see `ModeProvider.svelte`). It
  cascades, so nested subtrees can pin a mode (the styleguide shows both at once).
- Regenerate `theme.css` after any token edit: `pnpm tokens`. CI guards drift with
  `pnpm tokens --check`.

### Frozen foreground (readability)

The workbench is an analytics surface used for hours. Foreground is locked for calm
readability, not loudness: `--text` = `#14141a` light (~15:1) / `#d6d6dc` dark (~12:1).
Do not "brighten" or re-tune these.

### Fonts

`--font-sans` Archivo (body), `--font-display` Archivo Black (headings/display),
`--font-mono` JetBrains Mono (data, labels, code). Components reference `--fm`/`--fh`
shorthands set by `ModeProvider`.

---

## Geometry (non-negotiable)

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
- **Loud** — hero / landing only. 10px shadow, tricolor rail, display type, lime CTA. Never
  appears beside the workbench.

---

## Color semantics — "the Line"

Three plane colors encode the Wyrd architecture. Use them for identity, not decoration.

| Color | Token(s) | Plane |
|-------|----------|-------|
| Lime  | `--client` / `--client-bar` | client / runtime (agent, data, live events) |
| Amber | `--server` / `--server-bar` | server / enterprise (eval, services, compliance) |
| Blue  | `--control` / `--control-bar` | control / deploy (policy, audit, governance) |

`*-bar` variants are darkened on light mode for contrast (lime/amber/blue can't sit as text
or thin bars on white). Applied as a **3px left-bar** on rows/leaves to mark kind.

### Span kinds (traces)

Fixed mapping in waterfalls and trace tables: `agent` → `--client-bar`, `llm` →
`--rune-strong`, `tool` → `--server-bar`, `retrieval` → `--control-bar`, `error` →
`--danger`.

### Brand violet (rune)

`--rune` / `--rune-strong` / `--rune-soft` are the brand identity: links, big stats, selected
refs, the `llm` span kind, hero, and **all sequential/heat ramps** (lime cannot darken on
white — never ramp with it). Selection/hover wash is `--rune-soft`, never lime.

---

## Signal rules

- **Accent is signal, never decoration.** Lime and rune mark only: the one primary action,
  selected rows, active filters, status, and kind left-bars. They never tint body text or
  fill backgrounds at large.
- **Status overrides kind.** Normally a panel's top-bar shows its span/record kind. When the
  thing is in an alert state (eval failed, feature drifted), the top-bar turns `--danger`
  regardless of kind.
- **Status is state, not flavor.** `--ok` / `--warn` / `--danger` mean healthy / caution /
  failure — don't use them as palette accents.

---

## Building a new component

1. Add its contract to `components.json` (anatomy, tokens, variants, altitude, rules).
2. Build it in `src/lib/components` against the locked tokens (no hardcoded hex; correct
   altitude shadow; 5px radius; 2px/3px/dashed borders per role).
3. Register it in `src/lib/registry.ts` and flip its contract `status` to `built`.
4. Add it to `/styleguide` and write a test. `registry.test.ts` enforces contract ↔ registry
   parity.
