import { statSync, readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { allSlugs, getEntry } from './content.js';
import { buildNav } from './derived-nav.js';

// llms.txt and llms-full.txt are generated from the content tree and committed;
// `mise run docs:check` regenerates them and fails on drift. These checks pin
// what the generator must produce: only published destinations, in sidebar order.
const read = (name: string) => readFileSync(new URL(`../../public/${name}`, import.meta.url), 'utf8');
const sitePaths = (text: string) => [...text.matchAll(/\]\(https?:\/\/[^/)]+\/wyrd(\/[^)]*)\)/g)].map((m) => m[1]);

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
    const pages = sitePaths(read('llms.txt')).filter((path) => !publicFile(path));
    // The production sidebar: the test runner is a dev build, which would add drafts.
    const sidebar = buildNav(allSlugs().map((slug) => ({ slug, meta: getEntry(slug)!.metadata })));
    expect(pages).toEqual(['/', ...sidebar.flatMap((group) => group.items.map((item) => item.path))]);
  });
});
