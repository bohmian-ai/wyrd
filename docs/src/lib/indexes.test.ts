import { statSync, readFileSync } from 'node:fs';
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
      expect(text).not.toContain('](https://bohmian-ai.github.io');
    }
  });

  it('llms-full.txt carries every sidebar page body with its embedded source', () => {
    const full = read('llms-full.txt');
    const sidebar = buildNav(allSlugs().map((slug) => ({ slug, meta: getEntry(slug)!.metadata })));
    const pages = [...full.matchAll(/^Page: \/wyrd(\/[^\s]*)$/gm)].map((m) => m[1]);
    expect(pages).toEqual(sidebar.flatMap((group) => group.items.map((item) => item.path)));
    expect(full).toContain('cards.declare_from_path(SERVICE / "support-desk.yaml")');
    expect(full).toContain('```yaml title="service/support-desk.yaml"\napiVersion: wyrd/v1\nkind: Service');
    expect(full).not.toMatch(/<\/?(CodeFromFile|LangTabs?|Tabs?|Aside)\b|ANCHOR(_END)?:/);
  });
});
