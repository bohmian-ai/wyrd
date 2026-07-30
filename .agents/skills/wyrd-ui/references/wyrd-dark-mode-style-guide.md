# Wyrd Dark Mode Style Guide — Brutalist Phosphor Terminal

Reference for all dark-mode styling decisions. Dark mode is the brutalist aesthetic transposed onto a phosphor CRT — same geometry, different palette.

> *Sibling document to* `LIGHT_MODE_STYLE_GUIDE.md`*. Both modes share the same brutalist geometry (zero corners, 2px borders, hard-offset shadows). Only the palette and the CRT atmosphere differ.*

---

## Core Principle

**Brutalism, on a terminal.** The visual system is the same as light mode — thick borders, hard-offset solid shadows, zero rounded corners, press-in interactions. The atmosphere is what changes: phosphor green on near-black, with CRT scanlines, vignette, and text bloom layered over the whole interface.

This is **not** "the same surface in different colors." It's "the same surface, on a vintage hardware monitor." Same physicality, different room.

The body font swaps to **JetBrains Mono** in dark mode — that, plus the green and the CRT effects, is what creates the terminal feel. Light mode = parchment + Archivo. Dark mode = phosphor + JetBrains Mono.

---

## Color Palette

### Backgrounds (darkest to lightest)

| Token | Value | Usage |
|---|---|---|
| `surface-950` | `oklch(5.5% 0.004 150)` | Page background — deepest CRT glass black |
| `surface-900` | `oklch(6% 0.003 150)` | Recessed areas, footers |
| `surface-800` | `oklch(6.5% 0.003 150)` | Secondary panels |
| `surface-500` | `oklch(8% 0.003 150)` | Card backgrounds — barely elevated |
| `surface-200` | `oklch(10% 0.003 150)` | Nested sections within cards |
| `surface-100` | `oklch(13% 0.003 150)` | Hovered card areas |
| `surface-50` | `oklch(15% 0.004 150)` | Lightest surface — active/selected states |

### Borders

Where dark mode previously used 1px thin atmospheric borders, this version keeps the brutalist 2px geometry — but with phosphor-green instead of black:

| Context | Value | Notes |
|---|---|---|
| Card / panel border | `oklch(50% 0.10 150 / 0.7)` | 2px solid phosphor green at 70% alpha |
| Hero / feature border | `oklch(55% 0.11 150 / 0.8)` | 3px, slightly brighter |
| Divider lines (inside cards) | `oklch(45% 0.08 150 / 0.5)` | 2px dashed, dimmer |
| Input border | `oklch(50% 0.10 150 / 0.7)` | 2px solid |
| Focus ring | `oklch(65% 0.13 150)` | 3px bright phosphor with optional bloom |

### Text

| Role | Value | Usage |
|---|---|---|
| Primary text | `oklch(82% 0.14 152)` / `#8ddb9f` | All body copy |
| Heading text | `oklch(85-87% 0.15 150)` | Slightly brighter for h1-h3 |
| Muted/label text | `oklch(60-65% 0.10 150)` | Secondary labels, timestamps |
| Disabled text | `oklch(40-45% 0.08 150)` | Inactive elements |
| Bright accent | `oklch(90% 0.14 150)` | Hover state text, emphasis |

### Card-type semantic colors (muted dark variants)

Dark mode does **not** collapse card types to monochromatic green. Each retains its identity but at reduced lightness and chroma so it reads as "dimmed CRT phosphor of that color" rather than "saturated daylight color":

| Card type | Dark variant | Token |
|---|---|---|
| DataCard | `oklch(45% 0.11 155)` (dim moss) | `--card-data` |
| ModelCard | `oklch(45% 0.11 150)` (dim phosphor green) | `--card-model` |
| ExperimentCard | `oklch(50% 0.10 68)` (CRT amber) | `--card-experiment` |
| PromptCard | `oklch(50% 0.06 265)` (CRT blue) | `--card-prompt` |
| AgentCard | `oklch(5.5% 0.003 150)` (deepest black) | `--card-agent` |
| ServiceCard | `oklch(76% 0.10 150)` (phosphor light) | `--card-service` |

Cards stay visually distinct from each other but all live inside the CRT's color gamut.

### Status colors (dimmed but visible)

| Semantic | Dark Mode | Usage |
|---|---|---|
| Success | `oklch(65% 0.14 150)` | Same green as primary — no differentiation needed |
| Warning | `oklch(68% 0.14 60)` | CRT amber, visible but desaturated |
| Error | `oklch(65% 0.14 14)` | Muted CRT red |
| Info | use primary green or `--card-prompt` blue |

---

## Component Rules

### Shadows

**Yes — dark mode keeps the hard-offset shadows.** Same geometry as light mode (3px / 6px / 10px, blur=0). The difference is the **color**: pure black on near-black would be invisible, so shadows in dark mode use **dim phosphor green at ~75% alpha**:

```css
--neo-shadow-color: oklch(50% 0.10 150 / 0.75);

box-shadow: 6px 6px 0 0 var(--neo-shadow-color);
```

This is the most important departure from a typical "dark mode" approach. The shadows still exist; they're just rendered in a color that survives the dark background. The brutalist physicality of "this element is a stamped object" carries through both modes.

If you absolutely need a true "elevation glow" (e.g., a floating modal that should suggest distance from the page), add a faint outer green glow *in addition to* the hard-offset shadow:

```css
.modal {
  box-shadow:
    6px 6px 0 0 var(--neo-shadow-color),
    0 0 24px oklch(65% 0.14 150 / 0.15);
}
```

### Borders

- **Width**: 2px default, 3px for feature/hero containers (same as light)
- **Color**: phosphor-green at reduced alpha (`oklch(50% 0.10 150 / 0.7)`) — never pure black, never gray
- **Style**: solid for outer borders, 2px dashed for internal section dividers
- **Radius**: 0 (same as light)

The `border-black` Tailwind utility resolves to the phosphor-green color via the theme overrides — you can keep writing `border-2 border-black` in your markup and it'll Just Work.

### Cards

```
┌─ 2px phosphor green @ 70% alpha ─────────────────┐
│                                                    │
│  bg: oklch(8% 0.003 150) (surface-500)            │
│  shadow: 6px 6px 0 0 phosphor-green @ 75% alpha   │
│  radius: 0                                        │
│                                                    │
│  ── 2px dashed phosphor @ 50% alpha ────────────── │
│                                                    │
│  Nested: oklch(10% 0.003 150) (surface-200)       │
│                                                    │
└────────────────────────────────────────────────────┘
```

Same structural pattern as light mode. The card is a defined, shadowed object — not a "floating in darkness" element.

### Navbar

- Background: `oklch(15% 0.01 150)` — slightly elevated from page bg, distinguishably "the nav strip"
- Border-bottom: 2px solid phosphor green
- Logo: bold heading-brightness green, optional faint text bloom
- Nav links: phosphor green (`oklch(75% 0.15 150)`)
- Active nav link: brightest green + underline at `oklch(75% 0.15 150)`
- Icons: `currentColor` (inherits phosphor green)

### Pills, Tags, Badges

Same shape as light mode (2px border, 3x3 hard-offset shadow), but the fills become the muted card-type variants:

```css
/* DataCard pill in dark mode */
.card-tag--data {
  background: var(--card-data);  /* dim moss oklch(45% 0.11 155) */
  color: oklch(85% 0.10 150);    /* light phosphor text */
  border: 2px solid var(--neo-shadow-color);
  box-shadow: 3px 3px 0 0 var(--neo-shadow-color);
}
```

Filled, not outlined. Brutalism demands visible color blocks even at low saturation.

### Buttons

Same press-in pattern as light mode. The button is filled (not outlined like in a typical dark mode — that approach loses the brutalist physicality):

```css
.neo-btn {
  background: oklch(36% 0.09 148);  /* dim phosphor fill */
  color: oklch(83% 0.15 150);       /* bright phosphor text */
  border: 2px solid oklch(55% 0.11 150 / 0.8);
  box-shadow: 3px 3px 0 0 oklch(50% 0.10 150 / 0.75);
  /* press-in animation same as light mode */
}
```

The text gets a soft phosphor bloom (`text-shadow: 0 0 6px oklch(75% 0.10 150 / 0.5)`) so it glows slightly — reinforces the CRT feel without sacrificing legibility.

### Inputs & Form Controls

- Background: `oklch(8% 0.01 150)` — slightly darker than the card it sits in
- Border: 2px solid phosphor green
- Shadow: 3x3 phosphor-green hard-offset
- Text: standard phosphor green
- Placeholder: dimmer green (`oklch(50% 0.08 150)`)
- Focus: border brightens to `oklch(65% 0.13 150)` + faint outer glow

### Tabs

- Inactive: muted green text, no background, no border
- Active: filled phosphor-green block (`oklch(36% 0.09 148)`) with 2px border + 3x3 shadow — same stamp-forward behavior as light mode

---

## Hover & Interaction States

The press-in pattern works identically in dark mode. The only addition: a subtle bloom enhancement on hover for emphasis.

| Interaction | Effect |
|---|---|
| Hover (text) | Brighten to `oklch(90% 0.14 150)` + slight text-shadow bloom |
| Hover (card/row) | `translate(-2px, -2px)` + shadow offset +2px (same as light) |
| Hover (button) | `translate(-1px, -1px)` + shadow offset +1px (same as light) |
| Focus | 3px bright phosphor border + optional outer glow |
| Active/pressed | `translate(3px, 3px)` + shadow collapses to 0 (same as light) |
| Disabled | 50% opacity, no shadow, no hover effects |

Transitions: 80-120ms `ease-out` (same as light).

---

## Typography in Dark Mode

The dark mode **font swap is intentional**: body and headings become JetBrains Mono. This is what makes dark mode read as "a terminal session" rather than "a recolored web page."

| Role | Font |
|---|---|
| All body text | JetBrains Mono, weight 500 |
| Headings | JetBrains Mono, weight 700 |
| UI labels | JetBrains Mono (already mono in light too) |
| Code blocks | JetBrains Mono (already mono in light too) |
| Pixel accents | VT323 (with phosphor bloom for extra CRT vibe) |

Letter spacing increases slightly in dark mode (`0.01em` on headings, `0.02em` on nav active states) for better readability of the monospace text at smaller sizes.

---

## CRT Atmosphere

Three layered effects compose the CRT illusion. All three respect `prefers-reduced-motion: reduce`.

### Scanlines

A fixed-position 2px-repeating-linear-gradient overlay covers the whole viewport at z-index 100. Subtle — `rgba(0,0,0,0.06)` per scanline.

### Vignette

Radial-gradient at z-index 99 darkens the edges, simulating a curved CRT screen. Center stays clear; corners fade to `rgba(0,0,0,0.25)`.

### Phosphor bloom

Every text node gets a very faint `text-shadow: 0 0 1px oklch(75% 0.08 150 / 0.12)`. Almost imperceptible per-character but the cumulative effect makes text look like it's emitting light rather than absorbing it.

Code blocks, inputs, and editor surfaces get `text-shadow: none` so the bloom doesn't interfere with legibility of monospace data.

---

## Anti-Patterns (Never Do These in Dark Mode)

1. **No rounded corners.** Same rule as light mode. The theme enforces `border-radius: 0`.
2. **No black box-shadows.** Pure black shadows on near-black backgrounds are invisible. Use the phosphor-green shadow color (`--neo-shadow-color`).
3. **No `border-black` resolving to literal black.** The theme remaps it to phosphor green; don't fight that with inline overrides.
4. **No white or near-white text** (`oklch(95%+)`). Max brightness is `oklch(90%)` phosphor green.
5. **No surfaces brighter than `oklch(15%)`.** Everything is near-black. The CRT illusion breaks if a panel is too light.
6. **No completely monochromatic card-type collapse.** Each card type retains a distinct hue, just dimmed.
7. **No `text-gray-*` resolving to literal grays.** The theme remaps; don't break it with inline color overrides.
8. **No `backdrop-filter` / glassmorphism.** Brutalism is opaque in both modes.
9. **No saturated daylight colors.** Card-type fills in dark mode top out around `oklch(50% 0.11 *)`. Anything brighter looks anachronistic.
10. **No opacity-based darkening** (`bg-black/50`). Use the explicit oklch surface tokens.

---

## CSS Variable Mapping Cheat Sheet

When converting light-mode Tailwind classes to dark-mode-aware styles:

```
Tailwind class           → Dark Mode Resolved Value
─────────────────────────────────────────────────────
text-black               → oklch(82% 0.14 152)  [phosphor green]
text-gray-600            → oklch(65% 0.10 150)  [muted green]
text-gray-900            → oklch(80% 0.13 150)  [bright green]
bg-surface-50            → oklch(15% 0.004 150) [card bg, lightest surface]
bg-surface-500           → oklch(8% 0.003 150)  [recessed]
bg-white                 → oklch(8% 0.003 150)  [dark surface]
border-black             → oklch(50% 0.10 150 / 0.7)  [phosphor border]
border-2.border-black    → oklch(55% 0.11 150 / 0.8)  [stronger phosphor]
shadow / shadow-md       → 6px 6px 0 0 phosphor-green@0.75
shadow-sm                → 3px 3px 0 0 phosphor-green@0.75
ring-primary-500         → 3px solid oklch(65% 0.13 150)
rounded / rounded-md     → 0 (overridden globally)
```

---

## Implementation Strategy

Same as light mode: CSS variable swapping via `[data-theme='wyrd'].theme-dark` — not Tailwind `dark:` prefixes.

1. **Prefer theme-aware CSS variables.** `var(--neo-shadow-color)` resolves to black in light and phosphor-green in dark automatically.
2. Tailwind utility classes work via the theme's dark-mode overrides — `border-black`, `text-black`, `bg-white`, `shadow-md` all remap.
3. When introducing a new component, write it for light mode first; the dark-mode behavior should fall out of the theme overrides. If you find yourself needing a `[data-theme='wyrd'].theme-dark` override for your new component, the component is probably hardcoding a value it should be reading from a token.

---

## Quick reference — when you're editing a component

Ask yourself, in order:

1. **Does the shadow use `var(--neo-shadow-color)`?** It should, not hardcoded `#000`.
2. **Are the borders 2px and using `var(--neo-shadow-color)` or `border-black` (which remaps)?** They should be.
3. **Is the radius zero?** Yes, same as light.
4. **Am I using a saturated daylight color anywhere?** Don't. Stick to the dark variants.
5. **Does it feel like a brutalist UI rendered on a CRT?** That's the bar — same physicality as light mode, different atmosphere.
