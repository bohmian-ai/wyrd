import { describe, expect, it } from 'vitest';
import { embedCodeFromFile, fenceBlock } from './highlight.js';

// Embeds resolve at build time, so a page carries only the highlighted sample,
// never the repository source tree.
describe('embedCodeFromFile', () => {
  it('replaces the tag with a highlighted CodeBlock titled by file name', () => {
    const { code, dependencies } = embedCodeFromFile(
      '<CodeFromFile file="sdks/wyrd-sdk-ts/wyrd/src/index.ts" lang="typescript" />'
    );
    expect(code).toMatch(/^<CodeBlock lang="typescript" title="index\.ts" html=\{`<pre class="shiki/);
    expect(dependencies).toHaveLength(1);
    expect(dependencies[0]).toMatch(/sdks\/wyrd-sdk-ts\/wyrd\/src\/index\.ts$/);
  });

  it('embeds only the requested line range', () => {
    const one = embedCodeFromFile('<CodeFromFile file="../../sdks/wyrd-sdk-ts/wyrd/src/index.ts" lang="text" range="1-1" title="x" />');
    expect(one.code).toContain('title="x"');
    expect(one.code.match(/class="line"/g)).toHaveLength(1);
  });

  it('fails the build for a missing file, a bad range, or a non-literal attribute', () => {
    expect(() => embedCodeFromFile('<CodeFromFile file="sdks/missing.ts" lang="typescript" />')).toThrow(/file not found/);
    expect(() =>
      embedCodeFromFile('<CodeFromFile file="sdks/wyrd-sdk-ts/wyrd/src/index.ts" lang="text" range="9-2" />')
    ).toThrow(/invalid range/);
    expect(() => embedCodeFromFile('<CodeFromFile file={path} lang="text" />')).toThrow(/literal/);
  });
});

// Fences and embeds mark the lines a page wants the reader to look at.
describe('line marking', () => {
  it('marks fence lines from the info string and keeps its title', () => {
    const out = fenceBlock('a\nb\nc\nd', 'text', 'title="app.py" {2,4}');
    expect(out).toContain('title="app.py"');
    expect(out.match(/class="line hl"/g)).toHaveLength(2);
  });

  it('marks embed lines counted within the embedded range', () => {
    const { code } = embedCodeFromFile(
      '<CodeFromFile file="sdks/wyrd-sdk-ts/wyrd/src/index.ts" lang="text" range="1-3" highlight="2-3" />'
    );
    expect(code.match(/class="line hl"/g)).toHaveLength(2);
  });
});
