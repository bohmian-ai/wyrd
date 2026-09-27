// Regression tests for the Evidence Thread projection generator.
// Run: node --test brand/gen-theme.test.mjs (from crates/wyrd/wyrd-server/wyrd-ui).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { contrastFailures, loadPalette, paletteErrors, renderTargets } from './gen-theme.mjs';

// The semantic roles retain the current design, with the color values approved 2026-09-26.
const ROLES = {
  '--bg': ['#f5f6f8', '#0b0c12'],
  '--surface': ['#ffffff', '#12141c'],
  '--surface-2': ['#eef0f4', '#1a1d28'],
  '--surface-hover': ['#e9ecf2', '#1f2230'],
  '--text': ['#101014', '#e6e4da'],
  '--muted': ['#62626d', '#8f92a5'],
  '--border': ['#d9dde5', '#252838'],
  '--border-soft': ['#e8ebf0', '#1c1f2c'],
  '--declare': ['#4d5ef0', '#7a8cff'],
  '--declare-soft': ['#eceefe', '#1a2040'],
  '--declare-ink': ['#ffffff', '#0b0c12'],
  '--observe': ['#c5f23b', '#c5f23b'],
  '--observe-soft': ['#f3fce0', '#394523'],
  '--evidence': ['#c5f23b', '#c5f23b'],
  '--evidence-ink': ['#0b0c12', '#0b0c12'],
  '--ok': ['#3dbe5a', '#3dbe5a'],
  '--warn': ['#f0a13c', '#f0a13c'],
  '--danger': ['#e2484d', '#e2484d'],
  '--danger-soft': ['#fbe9ea', '#2b1a22'],
  '--code-bg': ['#12141c', '#12141c'],
  '--code-text': ['#e6e4da', '#e6e4da'],
  '--code-muted': ['#8f92a5', '#8f92a5'],
  '--code-keyword': ['#7a8cff', '#7a8cff'],
  '--code-string': ['#c5f23b', '#c5f23b'],
  '--code-number': ['#f0a13c', '#f0a13c']
};

// The README Wyrd mark geometry (REQ-002). Renderings may recolor, never redraw.
const MARK_PATHS = ['M42 92 L30 92 L6 8 L22 8 Z', 'M58 92 L70 92 L94 8 L78 8 Z', 'M44 8 h12 v84 h-12 z'];

const palette = loadPalette();
const targets = renderTargets(palette);
const byName = (suffix) => targets.find((t) => t.path.endsWith(suffix)).content;

/** The `--token: value;` declarations inside the first block opened by `selector`. */
function block(css, selector) {
  const start = css.indexOf(`${selector} {`);
  assert.notEqual(start, -1, `missing ${selector} block`);
  const body = css.slice(start, css.indexOf('}', start));
  return Object.fromEntries([...body.matchAll(/(--[a-z0-9-]+): ([^;]+);/g)].map((m) => [m[1], m[2]]));
}

test('both application projections carry every approved role value in both modes', () => {
  const workbench = byName('brand/theme.css');
  const docs = byName('docs/src/styles/wyrd-tokens.css');
  const blocks = {
    light: [block(workbench, '[data-mode="light"]'), block(docs, ':root[data-theme="light"]')],
    dark: [block(workbench, '[data-mode="dark"]'), block(docs, ':root[data-theme="dark"]')]
  };
  for (const [token, [light, dark]] of Object.entries(ROLES)) {
    for (const b of blocks.light) assert.equal(b[token], light, `${token} light`);
    for (const b of blocks.dark) assert.equal(b[token], dark, `${token} dark`);
  }
});

test('projections emit no retired hard-shadow, arcade, or superseded font contract', () => {
  for (const { path, content } of targets) {
    assert.doesNotMatch(content, /box-shadow|--shadow/, `${path} emits a shadow`);
    assert.doesNotMatch(
      content,
      /Archivo|Space Grotesk|JetBrains|Fraunces|VT323|Press Start|--font-(display|serif|pixel)/,
      `${path} emits a superseded font`
    );
  }
  assert.match(byName('brand/theme.css'), /--font-sans: "Familjen Grotesk"/);
  assert.match(byName('brand/theme.css'), /--font-mono: "Fragment Mono"/);
});

test('every Wyrd mark rendering keeps the exact README geometry', () => {
  const marks = targets.filter((t) => t.path.endsWith('.svg'));
  assert.ok(marks.length >= 4, 'mark renderings are projected');
  for (const { path, content } of marks) {
    const drawn = [...content.matchAll(/ d="([^"]+)"/g)].map((m) => m[1]);
    assert.deepEqual(drawn, MARK_PATHS, `${path} geometry`);
    assert.match(content, /viewBox="0 0 100 100"/, `${path} viewBox`);
    assert.match(content, new RegExp(ROLES["--evidence"][0]), `${path} spine is retained-evidence lime`);
  }
});

test('an undocumented or aliased token is rejected', () => {
  const aliased = structuredClone(palette);
  aliased.tokens['--brand'] = { group: 'declare', light: 'var(--declare)', dark: 'var(--declare)' };
  const errors = paletteErrors(aliased);
  assert.ok(errors.some((e) => e.includes('--brand') && e.includes('role')), errors.join('\n'));
  assert.ok(errors.some((e) => e.includes('--brand') && e.includes('literal')), errors.join('\n'));
  assert.deepEqual(paletteErrors(palette), []);
});

test('a used pair below its contrast floor fails in the mode that breaks it', () => {
  assert.deepEqual(contrastFailures(palette), []);
  const faded = structuredClone(palette);
  faded.tokens['--muted'].dark = '#3A4A46';
  const failures = contrastFailures(faded);
  assert.ok(failures.length > 0);
  assert.ok(failures.every((f) => f.includes('--muted') && f.includes('[dark]')), failures.join('\n'));
});
