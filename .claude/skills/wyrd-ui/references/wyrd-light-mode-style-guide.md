# Wyrd Light Mode Style Guide — Brutalist Parchment

Reference for all light-mode styling decisions. Light mode is the canonical brutalist surface — the inscribed record on parchment.

> *Sibling document to* `DARK_MODE_STYLE_GUIDE.md`*. Both modes share the same geometry (zero corners, 2px borders, hard-offset shadows). Only the palette differs.*

---

## Core Principle

**Carved, deliberate, confident.** Thick black borders, hard-offset solid shadows, vivid color blocks, bold typography. Zero rounded corners. Every UI element should look stamped onto the page, not floating above it.

Light mode is where the brutalism reads loudest. Dark mode applies the same rules through a phosphor-terminal filter.

---

## Color Palette

### Backgrounds (lightest to darkest)

| Token | Value | Usage |
|---|---|---|
| `surface-50` | `oklch(100% 0 none)` | Card surface, dialog body — pure white |
| `surface-100` | `oklch(99.43% 0 287.34deg)` | Slightly off-white inner panels |
| `surface-200` | `oklch(98.84% 0 287.33deg)` | Nested sections |
| `surface-500` | `oklch(97.18% 0 301.83deg)` | Page background — warm parchment |
| `surface-700` | `oklch(82.2% 0 301.74deg)` | Recessed / inset surfaces |
| `surface-950` | `oklch(58.44% 0 308.74deg)` | Heaviest neutral, used sparingly |

### Borders

| Context | Value | Notes |
|---|---|---|
| Card / panel border | `#0a0a0a` (ink black) | 2px solid — the brutalist edge |
| Hero / feature border | black | 3-5px — for the most important containers |
| Divider lines (inside cards) | black, 2px dashed | Hard-edged grouping |
| Input border | black | 2px solid |
| Focus ring | `--color-primary-700` | 3px purple |

### Text

| Role | Value | Usage |
|---|---|---|
| Primary text | `#0a0a0a` | All body copy, standard UI text |
| Heading text | `#0a0a0a` | Same black, heavier weight |
| Muted/label text | `rgba(10,10,10,0.65)` | Secondary labels, timestamps |
| Disabled text | `rgba(10,10,10,0.4)` | Inactive elements |
| Brand accent text | `#8a6df0` (`--color-primary-500`) | Rune-purple — used sparingly |

### Card-type semantic colors (reserved)

These do not swap based on context — they are the visual identity of each card type:

| Card type | Fill | Token |
|---|---|---|
| DataCard | moss green `#5fd68d` | `--color-secondary-500` |
| ModelCard | rune purple `#8a6df0` | `--color-primary-500` |
| ExperimentCard | warning gold `#fddc5a` | `--color-warning-500` |
| PromptCard | ink blue `#87aaf0` | `--color-tertiary-500` |
| AgentCard | ink black `#0a0a0a` | structural |
| ServiceCard | white `#ffffff` | `--color-surface-50` |

Each card-tag pill uses the type's fill, 2px black border, 3x3 black hard-offset shadow.

---

## Component Rules

### Shadows

**Hard-offset solid shadows. The blur is always zero. The offset is always one of three canonical values.**

```css
/* Small UI: badges, inline pills, status dots */
box-shadow: 3px 3px 0 0 #0a0a0a;

/* Default: cards, panels, buttons, inputs */
box-shadow: 6px 6px 0 0 #0a0a0a;

/* Hero / feature: banners, splash cards, primary CTAs */
box-shadow: 10px 10px 0 0 #0a0a0a;
```

Any shadow with `blur-radius > 0` breaks the aesthetic. Tailwind's default `shadow-md`, `shadow-lg`, etc. are *blurred* shadows — the theme overrides them but inline `box-shadow: 0 4px 6px rgba(0,0,0,0.1)` will sneak through. Don't write those.

### Borders

- **Width**: 2px default, 3px for feature/hero containers, 5px for the absolute strongest framings
- **Color**: `#0a0a0a` (the ink-black brand color). Never `#666`, never gray.
- **Style**: solid for most borders. Dashed (2px) reserved for internal section dividers inside cards.
- **Radius**: `0`. Always. The `--radius-base` token resolves to zero in this theme. Don't override with inline radius values.

### Cards

```
┌────────────────────────────────────────────────────┐
│                                                    │
│  bg: surface-50 (white)                            │
│  border: 2px solid #0a0a0a                         │
│  shadow: 6px 6px 0 0 #0a0a0a                       │
│  radius: 0                                         │
│                                                    │
│  ── 2px dashed #0a0a0a divider ─────────────────── │
│                                                    │
│  Nested section: surface-500 (parchment)           │
│                                                    │
└────────────────────────────────────────────────────┘
```

White card on parchment page background. Crisp black border. Hard-offset black shadow. Internal sections separated by 2px dashed lines (dashed signals "internal grouping" — solid lines are reserved for the outer border).

### Navbar

- Background: `--color-primary-700` (deep purple) or solid black, full-width
- Border-bottom: 2px solid black
- Logo: heavy display font, wordmark on left
- Nav links: white text on the colored bar, hover = underline or block highlight
- Active nav link: contrast color block with 2px black border (looks stamped into the nav)
- Icons: filled, solid color — never thin-line icons

### Pills, Tags, Badges

Flat color blocks with thick borders and small hard-offset shadows:

```css
.card-tag--data {
  background: var(--card-data);     /* moss green */
  color: #0a0a0a;
  border: 2px solid #0a0a0a;
  box-shadow: 3px 3px 0 0 #0a0a0a;
  padding: 3px 8px;
  font-family: var(--mono-font-family);
  font-weight: 700;
  font-size: 0.625rem;
  text-transform: uppercase;
  letter-spacing: 0.2em;
}
```

Use the `.card-tag--{data|model|experiment|prompt|agent|service}` classes from `wyrd-theme.css` rather than reinventing. They already encode the right colors.

### Buttons

The press-in animation is the signature interaction. On hover, the button lifts toward the user (-1px, -1px). On click, it presses flat against the page (shadow → 0).

```css
.neo-btn {
  background: var(--color-primary-500);   /* rune purple */
  color: white;
  border: 2px solid #0a0a0a;
  box-shadow: 3px 3px 0 0 #0a0a0a;
  padding: 0.5rem 1.1rem;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  transition: transform 0.08s ease, box-shadow 0.08s ease;
}
.neo-btn:hover {
  transform: translate(-1px, -1px);
  box-shadow: 4px 4px 0 0 #0a0a0a;
}
.neo-btn:active {
  transform: translate(3px, 3px);
  box-shadow: 0 0 0 0 #0a0a0a;
}
```

| Variant | Background | Text | Use case |
|---|---|---|---|
| `.neo-btn` (default) | `--color-primary-500` (purple) | white | Primary CTAs, register actions |
| `.neo-btn--secondary` | `--color-surface-50` (white) | black | Secondary actions, "Cancel", "Filter" |
| `.neo-btn--dark` | `#0a0a0a` (black) | white | Destructive or commitment actions ("Lock", "Delete") |

### Tables

- Header row: solid color band (often `--color-primary-100` or `--color-secondary-100`) with bold black text
- Row borders: 2px solid black between rows
- Hover row: `--color-primary-100` tint
- Cell text: standard black
- Headers in uppercase, JetBrains Mono, wide letter-spacing

### Inputs & Form Controls

- Background: white (`--color-surface-50`)
- Border: 2px solid black
- Shadow: 3x3 hard-offset black (yes, inputs get shadows too — they're physical objects in this system)
- Radius: 0
- Placeholder: `rgba(10,10,10,0.45)`
- Focus: 3px purple ring (`box-shadow: 0 0 0 3px var(--color-primary-300), 3px 3px 0 0 #0a0a0a`)

### Tabs

- Inactive: black text on transparent background, no border
- Active: filled color block (often primary purple) with 2px black border + 3x3 shadow — visually stamps forward from inactive tabs
- Hover: subtle background tint
- The active tab uses the reverse of the press-in dynamic — it's *the one that came forward*

---

## Hover & Interaction States

Apply the brutalist press-in pattern to anything tappable:

| Interaction | Effect |
|---|---|
| Hover (button/card) | `translate(-1px, -1px)` + shadow offset +1px |
| Hover (text link) | Underline appears, color unchanged |
| Hover (row in table) | Background tints to `surface-200` or `primary-100` |
| Focus | 3px purple ring around the element |
| Active/pressed | `translate(3px, 3px)` + shadow collapses to `0 0 0 0` |
| Disabled | 60% opacity, no shadow, no hover effects |

Transitions: 80-120ms with `ease-out`. Anything slower feels sluggish; anything faster looks broken.

---

## Typography

| Role | Font | Weight | Tracking |
|---|---|---|---|
| Display (wordmark, hero) | Archivo Black | 900 | -0.04em |
| H1 / H2 | Archivo Black | 900 | -0.02em |
| H3 / H4 | Archivo | 700-800 | -0.01em |
| Body | Archivo | 500 | 0 |
| UI labels (badges, eyebrow) | JetBrains Mono | 700 | 0.18em-0.25em, ALL CAPS |
| Code blocks | JetBrains Mono | 400 | 0 |
| Pixel accents (IDs, versions) | VT323 | 400 | 0.05em |

Headings are heavy. Body is medium (500). Light/thin weights aren't used anywhere in light mode.

VT323 is the secret weapon — reserved for inscribed data like `v3.0.0-rc.16`, `card_id: 7f2a91e4`, `2026-05-15T14:22:08Z`. Used sparingly so it stays special.

---

## Semantic Colors in Light Mode

Unlike dark mode (which is monochromatic phosphor + muted card types), **light mode uses the full semantic palette aggressively** for signaling:

| Semantic | Token | Where it appears |
|---|---|---|
| Success | `--color-secondary-500` (green) | Healthy state, completed actions |
| Warning | `--color-warning-500` (gold) | Drift detected, attention needed |
| Error | `--color-error-500` (red) | Failed actions, critical alerts |
| Info | `--color-tertiary-500` (blue) | Neutral notifications |
| Brand | `--color-primary-500` (purple) | Active states, primary CTAs |

Card-type colors share the palette but are **not interchangeable** with status colors. A red badge on a ModelCard means *alert*, not *AgentCard*. Context disambiguates — but to be safe, prefer the `.card-tag--*` classes for card-type pills and the semantic tokens for status indicators.

---

## Anti-Patterns (Never Do These in Light Mode)

1. **No rounded corners.** Not even 4px. The theme enforces `border-radius: 0` globally; don't override.
2. **No soft drop shadows.** Hard-offset solid shadows only. Third value of `box-shadow` stays at zero.
3. **No `rounded-full` for pills.** Use the standard zero-radius rectangle. Exception: actual circular things (avatars, status dots) use `.rounded-full` which is whitelisted as a true circle.
4. **No thin (1px) borders** for major UI. 2px minimum.
5. **No gray borders** (`border-gray-300`, etc.). Black or palette colors only.
6. **No light/thin font weights for headings.** 700-900 only.
7. **No multi-stop gradients on UI surfaces** (buttons, cards, panels, inputs). Solid fills only. Decorative hero areas may use the `.gradient-*` utility classes.
8. **No `backdrop-filter` / glassmorphism.** Brutalism is opaque.
9. **No emoji or decorative illustrations as primary visual content.** The type and color blocks carry the work.
10. **No system fonts as a brand statement.** Load Archivo. Helvetica/Arial breaks the system.

---

## CSS Variable Mapping Cheat Sheet

Common Tailwind utilities and their light-mode resolution:

```
Tailwind class       → Light Mode Value
─────────────────────────────────────────────────────
text-black           → #0a0a0a
text-gray-600        → rgba(10,10,10,0.65)
text-gray-900        → #0a0a0a
bg-white             → #ffffff (card body)
bg-surface-50        → #ffffff
bg-surface-500       → oklch(97.18% 0 301.83deg) (page bg parchment)
bg-primary-500       → #8a6df0 (rune purple)
bg-secondary-500     → #5fd68d (moss green)
border-black         → #0a0a0a (solid, 2px default)
shadow / shadow-md   → 6px 6px 0 0 #0a0a0a (hard offset)
shadow-sm            → 3px 3px 0 0 #0a0a0a
shadow-lg            → 10px 10px 0 0 #0a0a0a
ring-primary-500     → 3px solid rgba(138,109,240,0.5)
rounded / rounded-md → 0 (overridden globally)
rounded-full         → 9999px (whitelisted for circles)
```

---

## Implementation Strategy

Same as dark mode: CSS variable swapping via `[data-theme='wyrd'].theme-light` — not Tailwind `light:` prefixes.

1. **Prefer theme-aware CSS variables** over hardcoded hex values
2. Tailwind utility classes work — `bg-primary-500`, `border-black`, `shadow-md` resolve to the brutalist palette
3. For shadows specifically, Tailwind's defaults (blurred) are overridden to hard-offset versions in `wyrd-theme.css`. Don't write inline shadows that re-introduce blur.
4. Icons should be filled (not stroked thin-line icons) and use `currentColor` or palette tokens

---

## Quick reference — when you're editing a component

Ask yourself, in order:

1. **Does it have a hard-offset solid shadow?** It should, unless it's nested inside another shadowed element.
2. **Are the borders 2px and black?** They should be.
3. **Is the radius zero?** It must be. (Circles excepted.)
4. **Am I using a soft shadow, gray border, or rounded corner?** Don't.
5. **Am I using a semantic color appropriately?** Green = good, gold = warning, red = error, purple = brand/active. Card-type colors are reserved; don't swap.
6. **Does it feel like an inscribed record on parchment?** That's the bar.
