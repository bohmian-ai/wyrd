#!/usr/bin/env node
// Generates brand/theme.css from brand/palette.json (the canonical token source).
// Usage:
//   node brand/gen-theme.mjs           regenerate brand/theme.css
//   node brand/gen-theme.mjs --check   exit non-zero if theme.css is stale (CI / drift lock)
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const palettePath = join(here, 'palette.json');
const themePath = join(here, 'theme.css');

const palette = JSON.parse(readFileSync(palettePath, 'utf8'));
const { scale, tokens, modes } = palette;

// Skip --r in the @theme color map; it is geometry, not a color.
const colorEntries = Object.entries(tokens).filter(([name]) => name !== '--r');

function themeBlock() {
  const lines = [];
  lines.push('@theme {');
  lines.push(`  --font-sans: ${scale.fonts.sans};`);
  lines.push(`  --font-display: ${scale.fonts.display};`);
  lines.push(`  --font-mono: ${scale.fonts.mono};`);
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

const out = [
  '/* GENERATED FILE — do not edit by hand.',
  ' * Source of truth: brand/palette.json. Regenerate with `pnpm tokens`.',
  ' * Token names are identical to .dev/assets/wyrd-ui-source-of-truth.html. */',
  '',
  themeBlock(),
  '',
  ...modes.map(modeBlock),
  ''
].join('\n');

if (process.argv.includes('--check')) {
  let existing = '';
  try {
    existing = readFileSync(themePath, 'utf8');
  } catch {
    /* missing file → treated as stale */
  }
  if (existing !== out) {
    console.error('theme.css is out of date with palette.json. Run `pnpm tokens`.');
    process.exit(1);
  }
  console.log('theme.css is in sync with palette.json.');
} else {
  writeFileSync(themePath, out);
  console.log(`Wrote ${themePath}`);
}
