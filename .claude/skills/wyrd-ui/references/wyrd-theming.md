# Wyrd Theming

**Canonical source:** the app's `brand/` directory (`DESIGN.md`, `palette.json`,
`components.json`). If this summary ever disagrees with `brand/`, `brand/` wins.

## Tailwind v4 (no Skeleton)

Wyrd uses Tailwind v4's CSS-first setup with bespoke tokens — there is no Skeleton or other
component library. `src/app.css` imports Tailwind then the generated theme:

```css
@import 'tailwindcss';
@import '../brand/theme.css';
```

Do not introduce a parallel CSS system or a UI component library for one component.

## Tokens

- All color/geometry values are tokens in `brand/palette.json`. Edit there, then run
  `pnpm tokens` to regenerate `brand/theme.css` (GENERATED — never hand-edit it).
- Reference tokens via `var(--surface)`, `var(--text)`, etc., or the generated Tailwind
  utilities (`bg-surface`, `text-text`, `border-border`) which resolve per mode.
- Never hardcode hex in a component.

## Brutalist geometry

- Radius: `var(--r)` = **5px** (true circles only for dots/avatars).
- Border: `2px` default, `3px` hero/feature, `2px dashed` for in-card dividers — always
  `var(--border)`.
- Shadow: hard-offset, zero blur, `var(--shadow)`; altitude dial 3px (quiet) / 6px (raised) /
  10px (loud).
- Button interaction: press-in movement aligned with the shadow offset.

## Modes

- Light and dark share identical geometry; only the palette changes. Dark mode is flat
  near-black — **no CRT, scanlines, vignette, glow, or phosphor.**
- Mode is set by a `data-mode="light|dark"` attribute (`ModeProvider.svelte`); it cascades,
  so a subtree can pin a mode.
- Foreground locked for readability: `--text` `#14141a` light / `#d6d6dc` dark.

## Semantic colors

- Brand violet (rune): identity, links, big stats, selection wash, `llm` span kind, and all
  heat/sequential ramps (lime can't darken on white — never ramp with it).
- The Line: `--client` (lime, runtime), `--server` (amber, enterprise), `--control` (blue,
  deploy); `*-bar` variants darkened on light.
- Status (`--ok`/`--warn`/`--danger`) is state, not decoration; status overrides kind for
  panel top-bars in alert states.
- Accent is signal: lime = the one primary action, never default text.

## Anti-patterns

- Blurred shadows, glassmorphism, soft elevation, glow, CRT/phosphor.
- Zero or large radius (it's 5px), or rounding that fights the system.
- Gray hairline borders; inline styles that bypass tokens.
- A UI component library or one-off per-feature palettes.
