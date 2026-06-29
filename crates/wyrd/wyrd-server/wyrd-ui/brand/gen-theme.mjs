#!/usr/bin/env node
// Generates brand/theme.css, .claude/skills/wyrd-ui/references/wyrd-theme.css, and
// .codex/skills/wyrd-ui/references/wyrd-theme.css from brand/palette.json (the
// canonical token source).
// Usage:
//   node brand/gen-theme.mjs           regenerate every output target
//   node brand/gen-theme.mjs --check   exit non-zero if any target is stale (CI / drift lock)
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const palettePath = join(here, 'palette.json');
const themePath = join(here, 'theme.css');
const claudePath = join(here, '../../../../../.claude/skills/wyrd-ui/references/wyrd-theme.css');
const codexPath = join(here, '../../../../../.codex/skills/wyrd-ui/references/wyrd-theme.css');
const docsPath = join(here, '../../../../../docs/src/styles/wyrd-tokens.css');

const palette = JSON.parse(readFileSync(palettePath, 'utf8'));
const { scale, tokens, modes } = palette;

// Skip --r in the @theme color map; it is geometry, not a color.
const colorEntries = Object.entries(tokens).filter(([name]) => name !== '--r');

// The five font custom properties, emitted identically into every target. Single
// source so adding a font is one edit, not N hand-copied lists across the blocks.
const FONT_KEYS = ['sans', 'display', 'mono', 'arcade', 'pixel'];
function fontLines() {
  return FONT_KEYS.map((k) => `  --font-${k}: ${scale.fonts[k]};`);
}

// Fail loudly on an incomplete palette instead of emitting `--token: undefined;`
// into every target (which `--check`, svelte-check, and astro build all pass).
function validatePalette() {
  const errors = [];
  if (!Array.isArray(modes) || modes.length === 0) {
    errors.push('palette.modes must be a non-empty array');
  }
  for (const k of FONT_KEYS) {
    if (!scale?.fonts?.[k]) errors.push(`scale.fonts.${k} is missing or empty`);
  }
  if (!scale?.radius) errors.push('scale.radius is missing');
  for (const s of ['sm', 'md', 'lg']) {
    if (!scale?.shadow?.[s]) errors.push(`scale.shadow.${s} is missing`);
  }
  for (const [name, def] of Object.entries(tokens)) {
    for (const mode of Array.isArray(modes) ? modes : []) {
      const v = def[mode];
      if (v === undefined || v === null || v === '') {
        errors.push(`token ${name} has no value for mode "${mode}"`);
      }
    }
  }
  if (errors.length) {
    console.error('palette.json is invalid:');
    for (const e of errors) console.error(`  - ${e}`);
    process.exit(1);
  }
}
validatePalette();

function themeBlock() {
  const lines = [];
  lines.push('@theme {');
  lines.push(...fontLines());
  lines.push(`  --radius-wy: ${scale.radius};`);
  lines.push(`  --shadow-wy-sm: ${scale.shadow.sm} ${scale.shadow.sm} 0 0 var(--shadow);`);
  lines.push(`  --shadow-wy-md: ${scale.shadow.md} ${scale.shadow.md} 0 0 var(--shadow);`);
  lines.push(`  --shadow-wy-lg: ${scale.shadow.lg} ${scale.shadow.lg} 0 0 var(--shadow);`);
  lines.push('  /* mode-aware color utilities: bg-*, text-*, border-* resolve per [data-mode] */');
  for (const [name] of colorEntries) {
    lines.push(`  --color-${name.slice(2)}: var(${name});`);
  }
  lines.push('}');
  return lines.join('\n');
}

function modeBlock(mode) {
  const lines = [];
  let current = null;
  for (const [name, def] of Object.entries(tokens)) {
    if (def.group !== current) {
      current = def.group;
      lines.push(`${lines.length ? '\n' : ''}  /* ${current} */`);
    }
    lines.push(`  ${name}: ${def[mode]};`);
  }
  return `[data-mode="${mode}"] {\n${lines.join('\n')}\n}`;
}

// The docs site (Starlight) has no Tailwind @theme layer and toggles modes via
// [data-theme] (not [data-mode]). docsModeBlock emits the SAME canonical tokens
// as modeBlock, but remaps the selector to Starlight's [data-theme="<mode>"];
// light also covers bare :root as the no-JS default. Values are def[mode] HEX
// verbatim — no @theme block, no --color-* aliases, no oklch.
function docsModeBlock(mode) {
  const selector =
    mode === 'light'
      ? ':root,\n:root[data-theme="light"]'
      : `:root[data-theme="${mode}"]`;
  const lines = [];
  let current = null;
  for (const [name, def] of Object.entries(tokens)) {
    if (def.group !== current) {
      current = def.group;
      lines.push(`${lines.length ? '\n' : ''}  /* ${current} */`);
    }
    lines.push(`  ${name}: ${def[mode]};`);
  }
  return `${selector} {\n${lines.join('\n')}\n}`;
}

// Fonts are mode-independent and live in a plain :root block because docs has
// no @theme to carry them; commit 05 aliases the arcade-hero font vars onto these.
function docsFontsBlock() {
  const lines = [];
  lines.push('  /* fonts */');
  lines.push(...fontLines());
  return `:root {\n${lines.join('\n')}\n}`;
}

// The .codex skill ships a full Skeleton theme keyed on [data-theme='wyrd'].
// codexBlock emits the SAME canonical tokens as modeBlock, but under the
// Codex harness's theme-light/theme-dark selectors. Fonts are emitted here too
// so the ported utilities below can reference var(--font-*) without redefining
// any value by hand. INVARIANT: every value comes from palette.json — no oklch,
// no hand-written color literal.
function codexBlock(mode) {
  const selector =
    mode === 'light'
      ? "[data-theme='wyrd'].theme-light,\n[data-theme='wyrd']:not(.theme-dark)"
      : "[data-theme='wyrd'].theme-dark";
  const lines = [];
  lines.push('  /* fonts */');
  lines.push(...fontLines());
  let current = null;
  for (const [name, def] of Object.entries(tokens)) {
    if (def.group !== current) {
      current = def.group;
      lines.push(`\n  /* ${current} */`);
    }
    lines.push(`  ${name}: ${def[mode]};`);
  }
  return `${selector} {\n${lines.join('\n')}\n}`;
}

// Static brutalist utilities ported forward from the old hand-authored .codex
// fork, re-pointed at canonical tokens (var(--*)). The Never-rule layers from
// the old fork — .neo-glow, --neo-glow-color, .gradient-*, --retro-*, .grain,
// CRT scanlines/vignette, phosphor text-shadow — are intentionally dropped.
// CONTRACT: no raw hex/oklch/rgb literal may appear below; only var(--token)
// aliases and geometry (px/rem/em).
const codexUtilities = `/* ═══════════════════════════════════════════════════════════════════════════
   BRUTALIST UTILITY CLASSES — ported forward, re-pointed at canonical tokens.
   Token VALUES come from the generated blocks above (brand/palette.json).
   The Never-rule layers (glow, gradients, CRT scanlines/vignette, phosphor
   text-shadow) are intentionally absent.
   ═══════════════════════════════════════════════════════════════════════════ */

/* Zero-radius enforcement — overrides every Tailwind .rounded-* in both modes. */
[data-theme='wyrd'] .rounded,
[data-theme='wyrd'] .rounded-none,
[data-theme='wyrd'] .rounded-sm,
[data-theme='wyrd'] .rounded-md,
[data-theme='wyrd'] .rounded-lg,
[data-theme='wyrd'] .rounded-xl,
[data-theme='wyrd'] .rounded-2xl,
[data-theme='wyrd'] .rounded-3xl,
[data-theme='wyrd'] .rounded-t,
[data-theme='wyrd'] .rounded-b,
[data-theme='wyrd'] .rounded-l,
[data-theme='wyrd'] .rounded-r {
  border-radius: 0 !important;
}
/* Exception: true circles (avatars, status dots that should stay round). */
[data-theme='wyrd'] .rounded-full {
  border-radius: 9999px !important;
}

/* neo-card: canonical card surface */
[data-theme='wyrd'] .neo-card {
  background: var(--surface);
  border: 2px solid var(--border);
  box-shadow: 6px 6px 0 0 var(--shadow);
  border-radius: 0;
  transition: transform 0.12s ease, box-shadow 0.12s ease;
}
[data-theme='wyrd'] .neo-card:hover {
  transform: translate(-2px, -2px);
  box-shadow: 8px 8px 0 0 var(--shadow);
}

/* neo-btn: brutalist button with press-in animation */
[data-theme='wyrd'] .neo-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0.5rem 1.1rem;
  background: var(--rune-btn);
  color: var(--rune-btn-ink);
  border: 2px solid var(--border);
  box-shadow: 3px 3px 0 0 var(--shadow);
  border-radius: 0;
  font-family: var(--font-sans);
  font-weight: 700;
  font-size: 0.875rem;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  cursor: pointer;
  user-select: none;
  transition: transform 0.08s ease, box-shadow 0.08s ease;
}
[data-theme='wyrd'] .neo-btn:hover {
  transform: translate(-1px, -1px);
  box-shadow: 4px 4px 0 0 var(--shadow);
}
[data-theme='wyrd'] .neo-btn:active {
  transform: translate(3px, 3px);
  box-shadow: 0 0 0 0 var(--shadow);
}
[data-theme='wyrd'] .neo-btn--secondary {
  background: var(--surface);
  color: var(--text);
}
[data-theme='wyrd'] .neo-btn--dark {
  background: var(--border);
  color: var(--bg);
}

/* pixel-text: VT323 accents for IDs, version strings, timestamps */
[data-theme='wyrd'] .pixel-text {
  font-family: var(--font-pixel);
  font-size: 1.4em;
  line-height: 1;
  letter-spacing: 0.05em;
}

/* mono-tag: uppercase JetBrains Mono pill/label (canonical pill surface) */
[data-theme='wyrd'] .mono-tag {
  display: inline-block;
  padding: 3px 8px;
  font-family: var(--font-mono);
  font-size: 0.625rem;
  font-weight: 700;
  letter-spacing: 0.2em;
  text-transform: uppercase;
  border: 2px solid var(--border);
  background: var(--surface-2);
  color: var(--text);
  border-radius: 0;
}

/* card-type pills — mapped onto The Line + canonical accents (per token role) */
[data-theme='wyrd'] .card-tag--data       { background: var(--control);     color: var(--ink-on-fill);  border: 2px solid var(--border); }
[data-theme='wyrd'] .card-tag--model      { background: var(--rune);        color: var(--rune-btn-ink); border: 2px solid var(--border); }
[data-theme='wyrd'] .card-tag--experiment { background: var(--warn);        color: var(--ink-on-fill);  border: 2px solid var(--border); }
[data-theme='wyrd'] .card-tag--prompt     { background: var(--rune-strong); color: var(--rune-btn-ink); border: 2px solid var(--border); }
[data-theme='wyrd'] .card-tag--agent      { background: var(--client);      color: var(--ink-on-fill);  border: 2px solid var(--border); }
[data-theme='wyrd'] .card-tag--service    { background: var(--server);      color: var(--ink-on-fill);  border: 2px solid var(--border); }

/* status dots (drift/health indicators) */
[data-theme='wyrd'] .status-dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  background: var(--ok);
  border: 1.5px solid var(--border);
  margin-right: 6px;
  vertical-align: middle;
  border-radius: 0;
}
[data-theme='wyrd'] .status-dot--warn  { background: var(--warn); }
[data-theme='wyrd'] .status-dot--alert { background: var(--danger); }
`;

const banner = [
  '/* GENERATED FILE — do not edit by hand.',
  ' * Source of truth: brand/palette.json. Regenerate with `pnpm tokens`.',
  ' * Token names are identical to ./wyrd-ui-source-of-truth-v2.html. */',
].join('\n');

const themeOut = [
  banner,
  '',
  themeBlock(),
  '',
  ...modes.map(modeBlock),
  ''
].join('\n');

const claudeOut = [
  banner,
  '',
  ...modes.map(modeBlock),
  ''
].join('\n');

const codexOut = [
  banner,
  '',
  ...modes.map(codexBlock),
  '',
  codexUtilities,
].join('\n');

const docsOut = [
  banner,
  '',
  docsFontsBlock(),
  '',
  ...modes.map(docsModeBlock),
  ''
].join('\n');

const targets = [
  { path: themePath, content: themeOut },
  { path: claudePath, content: claudeOut },
  { path: codexPath, content: codexOut },
  { path: docsPath, content: docsOut },
];

if (process.argv.includes('--check')) {
  let allOk = true;
  for (const { path, content } of targets) {
    let existing = '';
    try {
      existing = readFileSync(path, 'utf8');
    } catch {
      /* missing file → treated as stale */
    }
    if (existing !== content) {
      console.error(`${path} is out of date with palette.json. Run \`pnpm tokens\`.`);
      allOk = false;
    }
  }
  if (!allOk) {
    process.exit(1);
  }
  console.log('All targets are in sync with palette.json.');
} else {
  for (const { path, content } of targets) {
    writeFileSync(path, content);
    console.log(`Wrote ${path}`);
  }
}
