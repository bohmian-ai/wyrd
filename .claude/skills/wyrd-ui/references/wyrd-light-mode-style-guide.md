# Wyrd Light Mode Style Guide — Brutalist Parchment

**Canonical source:** the app's `brand/` directory (`DESIGN.md` + `palette.json`) and
`.dev/assets/wyrd-ui-source-of-truth.html`. This is a portable summary; `brand/` wins on any
disagreement.

Light and dark share identical geometry (5px radius, 2px ink borders, hard-offset zero-blur
shadows). Only the palette differs.

## Surfaces & ink

- `--bg` `#f4f4f5` (page) · `--surface` `#ffffff` (cards/tables) · `--surface-2` `#ececef`
  (insets, hover, pills)
- `--border` `#0b0b0e` · `--shadow` `#0b0b0e` (hard offset, zero blur)

## Text (locked for readability)

- `--text` `#14141a` (~15:1) · `--muted` `#6b6b76`. Do not brighten or re-tune.

## Brand violet (rune) & acid

- `--rune` `#8b5cf6` · `--rune-strong` `#7c3aed` (links, big stats, llm kind, heat ramps) ·
  `--rune-soft` `#f1ecfd` (selection/hover wash)
- `--lime` `#c5f23c` (primary action fill; signal only) · `--lime-text` `#4a7a06` (lime as text)

## The Line

- `--client` `#c5f23c` / `--client-bar` `#c5f23c` (runtime)
- `--server` `#f2a23c` / `--server-bar` `#ec8b14` (enterprise; bar darkened for contrast)
- `--control` `#7cc4e8` / `--control-bar` `#2f6f9e` (deploy; bar darkened for contrast)

## Status

- `--ok` `#2f9e44` · `--warn` `#ec8b14` · `--danger` `#d2402a`

## Geometry

5px radius (`--r`); 2px borders (3px hero, 2px dashed in-card dividers); shadow dial 3/6/10px.
See `brand/DESIGN.md` for the altitude system and signal rules.
