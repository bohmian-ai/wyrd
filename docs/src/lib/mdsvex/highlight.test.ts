import { readFileSync } from 'node:fs';
import { describe, expect, it, vi } from 'vitest';
import { embedCodeFromFile, fenceBlock } from './highlight.js';

// One example file stands in for an anchored source; every other path reads
// the real repository.
const ANCHORED = [
  'def serve():',
  '    # ANCHOR: ask',
  '    answer = run.invoke(question)',
  '    if answer:',
  '        return answer',
  '    # ANCHOR_END: ask',
  ''
].join('\n');
vi.mock('node:fs', async (real) => {
  const fs = await real<typeof import('node:fs')>();
  const read = ((file: string, ...rest: unknown[]) =>
    String(file).endsWith('examples/anchored.py') ? ANCHORED : (fs.readFileSync as Function)(file, ...rest)) as typeof readFileSync;
  return { ...fs, readFileSync: read };
});

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

  it('embeds a named region dedented, without its anchor comments', () => {
    const { code } = embedCodeFromFile('<CodeFromFile file="examples/anchored.py" lang="text" region="ask" />');
    expect(code.match(/class="line"/g)).toHaveLength(3);
    expect(code).not.toContain('ANCHOR');
    expect(code).toContain('<span class="line"><span>answer = run.invoke(question)');
  });

  it('fails the build for a missing region or a range past the end of the file', () => {
    expect(() => embedCodeFromFile('<CodeFromFile file="examples/anchored.py" lang="text" region="gone" />')).toThrow(
      /region "gone" not found/
    );
    expect(() => embedCodeFromFile('<CodeFromFile file="examples/anchored.py" lang="text" range="1-99" />')).toThrow(
      /ends past line 7/
    );
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
