import { execFileSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { allSlugs, getEntry } from './content.js';
import { buildNav } from './derived-nav.js';

// llms.txt and llms-full.txt are generated from the content tree and committed;
// `mise run docs:check` regenerates them and fails on drift. These checks pin
// what the generator must produce: only published destinations, in sidebar order.
const read = (name: string) => readFileSync(new URL(`../../public/${name}`, import.meta.url), 'utf8');
// Links are deploy-base-relative, so a locally served build resolves them too.
const sitePaths = (text: string) => [...text.matchAll(/\]\(\/wyrd(\/[^)#]*)(?:#[^)]*)?\)/g)].map((m) => m[1]);
const indexPaths = (text: string) => sitePaths(text.slice(text.indexOf('## Home')));

const routes = new Set(['/', ...allSlugs().map((slug) => `/${slug}/`)]);
const publicFile = (path: string) =>
  statSync(new URL(`../../public${path}`, import.meta.url), { throwIfNoEntry: false })?.isFile() ?? false;

describe('machine-readable indexes follow published pages', () => {
  for (const name of ['llms.txt', 'llms-full.txt']) {
    it(`${name} links only to published pages and served files`, () => {
      expect(sitePaths(read(name)).filter((path) => !routes.has(path) && !publicFile(path))).toEqual([]);
    });
  }

  it('llms.txt lists the home page, then exactly the sidebar pages in sidebar order', () => {
    const pages = indexPaths(read('llms.txt')).filter((path) => !publicFile(path));
    // The production sidebar: the test runner is a dev build, which would add drafts.
    const sidebar = buildNav(allSlugs().map((slug) => ({ slug, meta: getEntry(slug)!.metadata })));
    expect(pages).toEqual(['/', ...sidebar.flatMap((group) => group.items.map((item) => item.path))]);
  });

  it('both indexes open with the canonical start route', () => {
    for (const name of ['llms.txt', 'llms-full.txt']) {
      const text = read(name);
      const start = text.slice(text.indexOf('## Start here'));
      expect(sitePaths(start).slice(0, 2)).toEqual(['/get-started/setup/', '/get-started/quickstart/']);
      expect(text).toContain('wyrd.guide {"topic":"start"}');
      expect(text).not.toContain('](https://docs.bohmian.ai');
    }
  });

  it('llms-full.txt carries every sidebar page body with its embedded source', () => {
    const full = read('llms-full.txt');
    const sidebar = buildNav(allSlugs().map((slug) => ({ slug, meta: getEntry(slug)!.metadata })));
    const pages = [...full.matchAll(/^Page: \/wyrd(\/[^\s]*)$/gm)].map((m) => m[1]);
    expect(pages).toEqual(sidebar.flatMap((group) => group.items.map((item) => item.path)));
    expect(full).toContain('state = WyrdState.declare("support-reply.yaml")');
    expect(full).toContain('```yaml title="support-reply.yaml"\n# The subject: a Service verified by refund-limit.\napiVersion: wyrd/v1\nkind: Service');
    expect(full).not.toMatch(/<\/?(CodeFromFile|LangTabs?|Tabs?|Aside)\b|ANCHOR(_END)?:/);
  });
});

const DOCS = new URL('../../', import.meta.url).pathname;
const CONTENT = path.join(DOCS, 'src/content/docs');

// Run a docs script's module against `content` in place of the real content
// tree, so omission fixtures never touch the working corpus.
function withCorpus(content: string, script: string, call: string): string {
  const code = `import sys; sys.path.insert(0, "scripts"); from pathlib import Path; import ${script} as m; m.${
    script === 'check_links' ? 'CONTENT_ROOT' : 'CONTENT_DIR'
  } = Path(sys.argv[1]); ${call}`;
  try {
    return execFileSync('uv', ['run', 'python', '-c', code, content], { cwd: DOCS, encoding: 'utf8' });
  } catch (err) {
    return (err as { stdout?: string }).stdout ?? '';
  }
}

// A throwaway copy of the content tree with `omit` removed and `drafts` marked draft.
function fixtureCorpus(omit: string[], drafts: string[] = []): string {
  const dir = mkdtempSync(path.join(tmpdir(), 'wyrd-docs-'));
  cpSync(CONTENT, dir, { recursive: true });
  for (const page of omit) rmSync(path.join(dir, page));
  for (const page of drafts) {
    const file = path.join(dir, page);
    writeFileSync(file, readFileSync(file, 'utf8').replace(/^---\n/, '---\ndraft: true\n'));
  }
  return dir;
}

// The routes a fixture corpus serves in production: every non-draft page.
function publishedRoutes(dir: string): Set<string> {
  const pages = execFileSync('find', [dir, '-name', '*.md'], { encoding: 'utf8' }).trim().split('\n').filter(Boolean);
  return new Set([
    '/',
    ...pages
      .filter((file) => !/^draft: true$/m.test(readFileSync(file, 'utf8').split('\n---')[0]))
      .map((file) => `/${path.relative(dir, file).replace(/\.md$/, '')}/`)
  ]);
}

describe('machine-readable indexes follow published pages', () => {
  it('tolerates omitted onboarding and reference pages', () => {
    const cases = [
      fixtureCorpus(['get-started/setup.md', 'get-started/quickstart.md', 'api/errors.md']),
      fixtureCorpus([], ['get-started/setup.md', 'get-started/quickstart.md', 'api/errors.md'])
    ];
    const empty = mkdtempSync(path.join(tmpdir(), 'wyrd-docs-'));
    for (const dir of [...cases, empty]) {
      const out = withCorpus(dir, 'generate_llms_txt', 'import json; print(json.dumps([m.render_llms_txt(), m.render_llms_full()]))');
      const [txt, full] = JSON.parse(out) as [string, string];
      const routes = publishedRoutes(dir);
      // Page bodies keep their authored links, which the link check owns; the
      // generator owns llms.txt, the llms-full.txt preamble, and its Page lines.
      const firstPage = full.search(/\n# (?!Wyrd — full)/);
      const preamble = firstPage < 0 ? full : full.slice(0, firstPage);
      const pages = [...full.matchAll(/^Page: \/wyrd(\/\S*)$/gm)].map((m) => m[1]);
      const dead = [...sitePaths(txt), ...sitePaths(preamble), ...pages].filter((p) => !routes.has(p) && !publicFile(p));
      expect(dead, dir).toEqual([]);
      expect(txt).toContain('## Home');
    }
  });
});

describe('publication closure', () => {
  it('rejects published links to withdrawn content', () => {
    const dir = mkdtempSync(path.join(tmpdir(), 'wyrd-docs-'));
    const page = (name: string, body: string, draft = false) => {
      mkdirSync(path.dirname(path.join(dir, name)), { recursive: true });
      writeFileSync(path.join(dir, name), `---\ntitle: ${name}\n${draft ? 'draft: true\n' : ''}---\n\n${body}\n`);
    };
    page('guide/kept.md', '[same page](#top) [asset](/llms.txt) [peer](./other) [root](/guide/other/) [based](/wyrd/guide/other/)');
    page('guide/other.md', '[back](./kept)');
    page('guide/held.md', '[kept](./kept) [held peer](./held-too)', true);
    page('guide/held-too.md', 'Deferred.', true);
    page('guide/leaks.md', '[relative](./held) [absolute](/guide/held/) [based](/wyrd/guide/held-too/)');
    const out = withCorpus(dir, 'check_links', 'sys.exit(m.main())');
    expect(out.split('\n').filter((line) => line.includes(': ')).map((line) => line.slice(line.indexOf('guide/')))).toEqual([
      'guide/leaks.md:5: link to unpublished page `./held`',
      'guide/leaks.md:5: link to unpublished page `/guide/held/`',
      'guide/leaks.md:5: link to unpublished page `/wyrd/guide/held-too/`'
    ]);
  });
});
