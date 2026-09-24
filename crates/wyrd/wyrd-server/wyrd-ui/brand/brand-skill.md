# Wyrd Brand — Evidence Thread

Scientific provenance, flat and ruled, in light and dark: a neutral mineral canvas, 1px
rules, 2px corners, no elevation, no glow, no arcade.

**Indigo `--declare` is Declaration and the primary action. Green `--observe` is observed
fact. Lime `--evidence` is retained Evidence only** — never an accent, hover, or secondary
action. Familjen Grotesk carries the interface and reading; Fragment Mono carries
machine-shaped values. Both ship locally.

**Never colour alone.** Status carries a glyph and a word, thresholds carry a tick, and
chart series carry a dash. `palette.json`'s `pairs` block declares every
foreground/background pairing and `gen-theme.mjs` fails the token gate on a contrast
violation in either mode.

The mark is the README Wyrd mark, recolored from the palette and never redrawn. The product
name is always **Wyrd**.

The full doctrine, tokens, and component contracts live in this directory:

- `DESIGN.md` — rules doctrine (read this first)
- `palette.json` — canonical tokens → `node brand/gen-theme.mjs` regenerates every
  projection; `mise run check:tokens` fails on drift
- `components.json` — per-component contracts
- `renders/styleguide.html` — the rendered visual reference (open from `file://` after
  `pnpm install`, which provides the local fonts)
