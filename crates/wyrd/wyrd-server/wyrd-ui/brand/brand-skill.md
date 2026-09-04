# bohmian · Wyrd Brand

Neobrutalism in light and dark: 5px radius, 2px ink borders, hard-offset zero-blur shadows,
calm foregrounds on warm paper / blue-cast near-black. No retro, no arcade, no glow.

**Blue `#2745e8` is primary, lime `#c5f23c` is secondary.** Bright lime is a fill only
(1.30:1 on white) — use `--lime-text` for text and thin bars. Fathom is named by the
product pill, the tinted card head, and the label, not by owning a colour.

**Never colour alone.** `--ok` and `--danger` collapse to 1.11:1 under deuteranopia, so
status carries a glyph, thresholds carry a tick, and chart series carry a dash.
`palette.json`'s `pairs` block declares every foreground/background pairing and
`gen-theme.mjs` fails the build on a contrast violation in either mode.

The wordmark "bohmian" is lowercase always.

The full doctrine, tokens, and component contracts live in this directory:

- `DESIGN.md` — rules doctrine (read this first)
- `palette.json` — canonical tokens → generates `theme.css` (`node brand/gen-theme.mjs`)
- `components.json` — per-component contracts
- `renders/` — the rendered visual reference (`styleguide.html`, `workbench.html`, `landing.html`)

`styleguide.html`, `workbench.html`, and `landing.html` open straight from `file://`.
`renders/index.html` reads `palette.json` over `fetch`, which `file://` blocks — serve the
folder instead:

```sh
cd crates/wyrd/wyrd-server/wyrd-ui/brand && python3 -m http.server 8777
# → http://localhost:8777/renders/index.html
```
