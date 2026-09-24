# Wyrd Theming

## Tailwind v4 and Skeleton

Wyrd uses Tailwind v4's CSS-first setup and Skeleton. Global styling belongs in `src/app.css` and Wyrd theme files. Keep component styling token-based so light and dark modes can resolve cleanly.

Typical import shape:

```css
@import 'tailwindcss';
@import '@skeletonlabs/skeleton';
```

Add Wyrd tokens through `@theme` or the dedicated Wyrd theme CSS. Do not introduce a parallel CSS system for one component.

## Token usage

- `brand/DESIGN.md` and `brand/palette.json` are the authority; `brand/theme.css` is their
  generated projection. Use its tokens (`--declare`, `--observe`, `--evidence`, `--ok`, …)
  or the Tailwind utilities they map to.
- Reserve status colors for actual state, and lime (`--evidence`) for retained Evidence.
- Avoid hardcoded hex in components; never hand-edit a generated theme file.

## Evidence Thread geometry

- Radius: `2px` (`--r`).
- Rules: `1px` — `--border` for structure, `--border-soft` for rows and inner dividers.
- No decorative elevation and no lift/sink press motion; state is carried by fill and rule.

## Dark mode

Dark mode is a complete rendering of the same structure. Swap token values, not component
structure.

## Icons and charts

- Use the existing icon library when available.
- Prefer recognizable icons for common actions.
- Charts use theme tokens, and every series carries a dash and marker as well as a color.

## Anti-patterns

- Hard-offset, blurred, or glowing shadows; gradients; scanlines.
- Lime as an accent, hover, or secondary action.
- Inline styles that bypass tokens.
- Component-library defaults that override Wyrd geometry.
- One-off color palettes per feature.
