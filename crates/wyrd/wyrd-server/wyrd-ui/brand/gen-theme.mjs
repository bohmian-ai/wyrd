#!/usr/bin/env node
// Generates every Evidence Thread projection from brand/palette.json (the one
// canonical token source):
//   brand/theme.css                                   workbench Tailwind theme
//   docs/src/styles/wyrd-tokens.css                   docs site tokens
//   .agents/skills/wyrd-ui/references/wyrd-theme.css  wyrd-ui skill reference
//   brand/logo.svg, brand/logo-light.svg, brand/app-icon.svg,
//   docs/src/assets/wyrd-mark.svg, docs/public/favicon.svg   Wyrd mark renderings
// Usage:
//   node brand/gen-theme.mjs           regenerate every target
//   node brand/gen-theme.mjs --check   exit non-zero if any target is stale
//
// The .claude/skills copy is a byte mirror of .agents/skills owned by
// `mise run skills:sync`; run it after regenerating.
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, '../../../../..');

/** Read and parse the canonical palette. */
export function loadPalette() {
  return JSON.parse(readFileSync(join(here, 'palette.json'), 'utf8'));
}

// Evidence Thread has exactly two faces: Familjen Grotesk for interface and
// reading, Fragment Mono for machine-shaped values.
const FONT_KEYS = ['sans', 'mono'];
const HEX = /^#[0-9A-Fa-f]{6}$/;

// The README Wyrd mark geometry (docs/src/assets/wyrd-mark.svg as approved).
// Renderings recolor its wings, spine, and outline; they never redraw it.
const MARK_WINGS = ['M42 92 L30 92 L6 8 L22 8 Z', 'M58 92 L70 92 L94 8 L78 8 Z'];
const MARK_SPINE = 'M44 8 h12 v84 h-12 z';

/**
 * Every structural problem with a palette, as readable messages. An empty list
 * means the palette may be projected. Each token must carry a documented role and
 * a literal value in every mode, so a projection can never emit an undocumented
 * alias or `--token: undefined;`.
 */
export function paletteErrors({ modes, scale, tokens }) {
  const errors = [];
  if (!Array.isArray(modes) || modes.length === 0) {
    errors.push('palette.modes must be a non-empty array');
  }
  for (const k of FONT_KEYS) {
    if (!scale?.fonts?.[k]) errors.push(`scale.fonts.${k} is missing or empty`);
  }
  if (!scale?.radius) errors.push('scale.radius is missing');
  for (const [name, def] of Object.entries(tokens)) {
    if (!def.role) errors.push(`token ${name} has no documented role`);
    for (const mode of Array.isArray(modes) ? modes : []) {
      const v = def[mode];
      if (v === undefined || v === null || v === '') {
        errors.push(`token ${name} has no value for mode "${mode}"`);
      } else if (def.group !== 'geometry' && !HEX.test(v)) {
        errors.push(`token ${name} [${mode}] must be a literal #rrggbb color, not "${v}"`);
      }
    }
  }
  return errors;
}

// ── Contrast contract ──────────────────────────────────────────────────────
// An ink/fill pair is safe only if the two tokens diverge in luminance in every
// mode, so each pair in palette.pairs is asserted in every mode by the build
// rather than by a reviewer's eyes.

/** Parse `#rrggbb` into an [r,g,b] byte triple. */
function parseHex(hex) {
  const h = hex.trim().replace('#', '');
  return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16));
}

/** sRGB channel → linear light, per WCAG 2.x relative-luminance. */
function channelToLinear(byte) {
  const c = byte / 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

/** WCAG relative luminance of an [r,g,b] triple. */
function relativeLuminance([r, g, b]) {
  return 0.2126 * channelToLinear(r) + 0.7152 * channelToLinear(g) + 0.0722 * channelToLinear(b);
}

/** WCAG contrast ratio between two [r,g,b] triples, always >= 1. */
function contrastRatio(a, b) {
  const la = relativeLuminance(a);
  const lb = relativeLuminance(b);
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

/** Every declared pair that falls below its floor, per mode, as readable messages. */
export function contrastFailures({ modes, tokens, pairs = [] }) {
  const failures = [];
  const value = (name, mode) => {
    if (!tokens[name]) throw new Error(`pairs references unknown token ${name}`);
    return parseHex(tokens[name][mode]);
  };
  for (const pair of pairs) {
    for (const mode of modes) {
      let ratio;
      try {
        ratio = contrastRatio(value(pair.ink, mode), value(pair.fill, mode));
      } catch (err) {
        failures.push(err.message);
        continue;
      }
      if (ratio < pair.min) {
        failures.push(
          `${pair.ink} on ${pair.fill} [${mode}] = ${ratio.toFixed(2)}:1, ` +
            `needs ${pair.min}:1 — ${pair.role ?? ''}`
        );
      }
    }
  }
  return failures;
}

// ── CSS projections ────────────────────────────────────────────────────────

/** The per-mode custom property lines, grouped by role family. */
function tokenLines(tokens, mode) {
  const lines = [];
  let current = null;
  for (const [name, def] of Object.entries(tokens)) {
    if (def.group !== current) {
      current = def.group;
      lines.push(`${lines.length ? '\n' : ''}  /* ${current} */`);
    }
    lines.push(`  ${name}: ${def[mode]};`);
  }
  return lines;
}

function fontLines(scale) {
  return FONT_KEYS.map((k) => `  --font-${k}: ${scale.fonts[k]};`);
}

/** Workbench: a Tailwind @theme mapping plus one block per [data-mode]. */
function workbenchCss({ modes, scale, tokens }) {
  const theme = ['@theme {', ...fontLines(scale), `  --radius-wy: ${scale.radius};`];
  theme.push('  /* mode-aware color utilities: bg-*, text-*, border-* resolve per [data-mode] */');
  for (const [name, def] of Object.entries(tokens)) {
    if (def.group !== 'geometry') theme.push(`  --color-${name.slice(2)}: var(${name});`);
  }
  theme.push('}');
  const modeBlocks = modes.map(
    (mode) => `[data-mode="${mode}"] {\n${tokenLines(tokens, mode).join('\n')}\n}`
  );
  return [BANNER, '', theme.join('\n'), '', ...modeBlocks, ''].join('\n');
}

/**
 * Docs: the same tokens under the docs site's [data-theme] selector. Light also
 * covers bare :root as the no-JS default. Fonts are mode-independent.
 */
function docsCss({ modes, scale, tokens }) {
  const fonts = `:root {\n  /* fonts */\n${fontLines(scale).join('\n')}\n}`;
  const modeBlocks = modes.map((mode) => {
    const selector = mode === 'light' ? ':root,\n:root[data-theme="light"]' : `:root[data-theme="${mode}"]`;
    return `${selector} {\n${tokenLines(tokens, mode).join('\n')}\n}`;
  });
  return [BANNER, '', fonts, '', ...modeBlocks, ''].join('\n');
}

/** wyrd-ui skill reference: the same tokens under the skill's theme selectors. */
function skillCss({ modes, scale, tokens }) {
  const modeBlocks = modes.map((mode) => {
    const selector =
      mode === 'light'
        ? "[data-theme='wyrd'].theme-light,\n[data-theme='wyrd']:not(.theme-dark)"
        : `[data-theme='wyrd'].theme-${mode}`;
    const lines = ['  /* fonts */', ...fontLines(scale), '', ...tokenLines(tokens, mode)];
    return `${selector} {\n${lines.join('\n')}\n}`;
  });
  return [BANNER, '', ...modeBlocks, ''].join('\n');
}

const BANNER = [
  '/* GENERATED FILE — do not edit by hand.',
  ' * Source of truth: crates/wyrd/wyrd-server/wyrd-ui/brand/palette.json.',
  ' * Regenerate with `node brand/gen-theme.mjs` from crates/wyrd/wyrd-server/wyrd-ui. */'
].join('\n');

// ── Wyrd mark renderings ───────────────────────────────────────────────────

/**
 * One Wyrd mark SVG. `mode` is `light`, `dark`, or `adaptive`; adaptive follows
 * the viewer's color scheme and is used where the embedding page cannot pass a
 * mode (favicons, the README). Wings are Declare, the spine is Retained Evidence,
 * and the outline is Ink on light and Canvas on dark.
 */
function markSvg({ tokens }, mode, size) {
  const colors = (m) => ({ wing: tokens['--declare'][m], line: m === 'light' ? tokens['--text'][m] : tokens['--bg'][m] });
  const spine = tokens['--evidence'].light;
  const dims = size ? ` width="${size}" height="${size}"` : '';
  const lines = [`<svg xmlns="http://www.w3.org/2000/svg"${dims} viewBox="0 0 100 100" role="img" aria-label="Wyrd">`, '  <title>Wyrd</title>'];
  if (mode === 'adaptive') {
    const lc = colors('light');
    const dc = colors('dark');
    lines.push(
      `  <style>.w{fill:${lc.wing}}.o{stroke:${lc.line}}@media (prefers-color-scheme: dark){.w{fill:${dc.wing}}.o{stroke:${dc.line}}}</style>`
    );
    for (const d of MARK_WINGS) lines.push(`  <path class="w o" d="${d}" stroke-width="2"/>`);
    lines.push(`  <path class="o" d="${MARK_SPINE}" fill="${spine}" stroke-width="2"/>`);
  } else {
    const c = colors(mode);
    for (const d of MARK_WINGS) lines.push(`  <path d="${d}" fill="${c.wing}" stroke="${c.line}" stroke-width="2"/>`);
    lines.push(`  <path d="${MARK_SPINE}" fill="${spine}" stroke="${c.line}" stroke-width="2"/>`);
  }
  lines.push('</svg>', '');
  return lines.join('\n');
}

/** Every projection, as `{ path, content }`, derived from one palette. */
export function renderTargets(palette) {
  return [
    { path: join(here, 'theme.css'), content: workbenchCss(palette) },
    { path: join(repo, 'docs/src/styles/wyrd-tokens.css'), content: docsCss(palette) },
    { path: join(repo, '.agents/skills/wyrd-ui/references/wyrd-theme.css'), content: skillCss(palette) },
    { path: join(here, 'logo.svg'), content: markSvg(palette, 'dark') },
    { path: join(here, 'logo-light.svg'), content: markSvg(palette, 'light') },
    { path: join(here, 'app-icon.svg'), content: markSvg(palette, 'adaptive') },
    { path: join(repo, 'docs/src/assets/wyrd-mark.svg'), content: markSvg(palette, 'adaptive', 96) },
    { path: join(repo, 'docs/public/favicon.svg'), content: markSvg(palette, 'adaptive') }
  ];
}

/** CLI: validate, assert contrast, then write or --check every projection. */
function main() {
  const palette = loadPalette();
  const errors = paletteErrors(palette);
  if (errors.length) {
    console.error('palette.json is invalid:');
    for (const e of errors) console.error(`  - ${e}`);
    process.exit(1);
  }
  const failures = contrastFailures(palette);
  if (failures.length) {
    console.error('palette.json fails its contrast contract:');
    for (const f of failures) console.error(`  - ${f}`);
    console.error('\nFix the token values, or correct the pair in palette.pairs if the');
    console.error('role changed. Do not lower `min` to make a real failure pass.');
    process.exit(1);
  }
  const targets = renderTargets(palette);
  if (process.argv.includes('--check')) {
    const stale = targets.filter(({ path, content }) => {
      try {
        return readFileSync(path, 'utf8') !== content;
      } catch {
        return true;
      }
    });
    for (const { path } of stale) {
      console.error(`${path} is out of date with palette.json. Run \`node brand/gen-theme.mjs\`.`);
    }
    if (stale.length) process.exit(1);
    console.log('All targets are in sync with palette.json.');
  } else {
    for (const { path, content } of targets) {
      writeFileSync(path, content);
      console.log(`Wrote ${path}`);
    }
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main();
