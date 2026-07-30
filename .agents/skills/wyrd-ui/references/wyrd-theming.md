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

- Use `--color-wyrd-*` and Wyrd theme tokens for brand surfaces.
- Use semantic tokens for status: success, warning, error, info.
- Reserve status colors for actual state, risk, or feedback.
- Use card-type colors only for card identity, not arbitrary accents.
- Avoid hardcoded hex in components unless editing the theme itself.

## Brutalist geometry

- Radius: `0`.
- Border: `2px` default.
- Shadow: hard-offset, zero blur.
- Button interaction: press-in movement that aligns with the shadow offset.
- Dividers inside cards: `2px dashed`.

## Dark mode

Dark mode preserves the same brutalist geometry. Swap palette and atmosphere, not component structure. Borders and shadows must remain visible against near-black surfaces.

## Icons and charts

- Use the existing icon library when available.
- Prefer recognizable icons for common actions.
- Keep icon strokes visually strong enough for the brutalist UI.
- Charts should use theme-aware colors and remain legible in both modes.

## Anti-patterns

- Blurred shadows.
- Rounded cards, pills, or buttons.
- Gray hairline borders.
- Inline styles that bypass tokens.
- Component-library defaults that override Wyrd geometry.
- One-off color palettes per feature.
