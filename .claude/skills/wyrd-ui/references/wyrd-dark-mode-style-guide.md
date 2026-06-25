# Wyrd Dark Mode Style Guide — Flat Near-Black

**Canonical source:** the app's `brand/` directory (`DESIGN.md` + `palette.json`) and
`.dev/assets/wyrd-ui-source-of-truth.html`. This is a portable summary; `brand/` wins on any
disagreement.

Dark mode is the **same brutalist system on darker surfaces** — identical geometry (5px
radius, 2px borders, hard-offset zero-blur shadows). **No CRT, scanlines, vignette, glow, or
phosphor.** It is not a terminal skin; it is flat near-black.

## Surfaces & ink

- `--bg` `#0e0e0e` (page) · `--surface` `#17171b` (cards/tables) · `--surface-2` `#1f1f24`
  (insets, hover, pills)
- `--border` `#313139` · `--shadow` `#2a2a31` (hard offset, zero blur — visible against
  near-black)

## Text (locked for readability)

- `--text` `#d6d6dc` (~12:1, calm anti-halation off-white) · `--muted` `#8a8a95`. Do not
  brighten or re-tune.

## Brand violet (rune) & acid

- `--rune` `#241b40` (deep fill) · `--rune-strong` `#a78bfa` (links, big stats, llm kind, heat
  ramps) · `--rune-soft` `#241b40` (selection/hover wash)
- `--lime` `#c5f23c` (primary action fill; signal only) · `--lime-text` `#c5f23c`

## The Line

- `--client` `#c5f23c` / `--client-bar` `#c5f23c` (runtime)
- `--server` `#f2a23c` / `--server-bar` `#f2a23c` (enterprise)
- `--control` `#6fb3d6` / `--control-bar` `#6fb3d6` (deploy)

## Status

- `--ok` `#7ee081` · `--warn` `#f2a23c` · `--danger` `#e8615a`

## Geometry

Identical to light. See `brand/DESIGN.md` for the altitude system and signal rules.
