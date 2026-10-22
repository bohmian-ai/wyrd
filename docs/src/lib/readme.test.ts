import { readFileSync } from 'node:fs';
import { render } from 'svelte/server';
import { expect, it } from 'vitest';
import WyrdDiagram from '$lib/components/WyrdDiagram.svelte';

// The repository README shows the Introduction's stack diagram as a static SVG
// per GitHub theme. Each file is the server-rendered drawing with the docs'
// SVG rules and that theme's tokens inlined, so the README never drifts from
// the docs. Regenerate with `pnpm test -u` after changing the diagram.
const css = readFileSync(new URL('../styles/docs.css', import.meta.url), 'utf8');
const tokens = readFileSync(new URL('../styles/wyrd-tokens.css', import.meta.url), 'utf8');
const rules = css
  .slice(css.indexOf('/* SVG grammar */'), css.indexOf('@media (prefers-reduced-motion: reduce)', css.indexOf('/* SVG grammar */')))
  .replaceAll('.wd svg ', '');

function vars(theme: 'light' | 'dark'): Map<string, string> {
  const block = (sel: string) => tokens.slice(tokens.indexOf(sel), tokens.indexOf('}', tokens.indexOf(sel)));
  const out = new Map<string, string>();
  for (const b of [block(':root {'), block(`:root[data-theme="${theme}"]`)])
    for (const [, k, v] of b.matchAll(/(--[\w-]+):\s*([^;]+);/g)) out.set(k, v.trim());
  return out;
}

function standalone(theme: 'light' | 'dark'): string {
  const v = vars(theme);
  const style = rules.replace(/var\((--[\w-]+)\)/g, (_, k: string) => v.get(k) ?? '');
  const { body } = render(WyrdDiagram, { props: { name: 'wyrd-in-your-stack' } });
  const svg = body.match(/<svg[\s\S]*?<\/svg>/)![0].replace(/<!--[\s\S]*?-->/g, '');
  const [, , w, h] = svg.match(/viewBox="([\d.]+ ){2}([\d.]+) ([\d.]+)"/)!;
  return (
    svg
      .replace(/^<svg/, `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}"`)
      .replace(/(<svg[^>]*>)/, `$1<style>${style}</style><rect width="100%" height="100%" fill="${v.get('--surface')}"/>`) + '\n'
  );
}

for (const theme of ['light', 'dark'] as const) {
  it(`README stack diagram (${theme}) matches the docs diagram`, async () => {
    await expect(standalone(theme)).toMatchFileSnapshot(`../../../.github/assets/wyrd-stack-${theme}.svg`);
  });
}

// Each README code block marked `<!-- example: <path> -->` is a verbatim copy of
// that tested example file, since GitHub cannot embed files.
it('README examples match their tested files', () => {
  const readme = readFileSync(new URL('../../../README.md', import.meta.url), 'utf8');
  const blocks = [...readme.matchAll(/<!-- example: (\S+) -->\s*```\w*\n([\s\S]*?)```/g)];
  expect(blocks.length).toBeGreaterThan(0);
  for (const [, path, code] of blocks) {
    expect(code, path).toBe(readFileSync(new URL(`../../../${path}`, import.meta.url), 'utf8'));
  }
});
