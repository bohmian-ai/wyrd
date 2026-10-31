import { describe, it, expect } from 'vitest';
import { SECTIONS, type DocMetadata } from './content.js';
import {
  buildNav,
  firstPage,
  inPrimaryNav,
  locate,
  navGroups,
  siblings,
  stripBase,
  type NavEntry
} from './derived-nav.js';

// Fixture corpus: the published content tree starts empty and grows one page at
// a time, so navigation behavior is pinned against explicit entries rather than
// whatever pages happen to exist today.
function page(slug: string, meta: Partial<DocMetadata> & Pick<DocMetadata, 'section'>): NavEntry {
  return { slug, meta: { title: slug, pillar: 'wyrd', ...meta } };
}

const corpus: NavEntry[] = [
  page('reference/errors', { title: 'Error codes', section: 'Reference' }),
  page('verify/run', { title: 'Run a verification', section: 'Build and verify', topic: 'Verify' }),
  page('get-started/quickstart', { title: 'Quickstart', section: 'Get started', order: 2 }),
  page('get-started/introduction', { title: 'Introduction', section: 'Get started', order: 1 }),
  page('declare/service', { title: 'Define a service', section: 'Build and verify', topic: 'Declare' }),
  page('get-started/draft', { title: 'Draft', section: 'Get started', draft: true })
];
const nav = buildNav(corpus);
const flat = nav.flatMap((group) => group.items);

describe('stripBase', () => {
  it('removes the base prefix and keeps a leading slash', () => {
    expect(stripBase('/wyrd/cards/', '/wyrd')).toBe('/cards/');
  });

  it('maps the bare base to root', () => {
    expect(stripBase('/wyrd', '/wyrd')).toBe('/');
  });

  it('leaves a pathname without the base untouched', () => {
    expect(stripBase('/cards/', '/wyrd')).toBe('/cards/');
  });

  it('does not strip when base is empty', () => {
    expect(stripBase('/wyrd/cards/', '')).toBe('/wyrd/cards/');
  });
});

describe('published pages define navigation', () => {
  it('builds no sections from an empty corpus', () => {
    expect(buildNav([])).toEqual([]);
  });

  it('never exposes an empty section from the real content tree', () => {
    for (const group of navGroups()) {
      expect(SECTIONS).toContain(group.label);
      expect(group.items.length).toBeGreaterThan(0);
    }
  });

  it('shows only sections with a published page, in the approved order', () => {
    expect(nav.map((group) => group.label)).toEqual(['Get started', 'Build and verify', 'Reference']);
  });

  it('orders pages by frontmatter order, then slug', () => {
    expect(nav[0].items.map((item) => item.label)).toEqual(['Introduction', 'Quickstart']);
  });

  it('nests Build and verify pages under Declare, Observe, Verify topics', () => {
    const build = nav.find((group) => group.label === 'Build and verify')!;
    expect(build.topics.map((topic) => topic.label)).toEqual(['Declare', 'Verify']);
    expect(build.topics[0].items).toEqual([{ label: 'Define a service', path: '/declare/service/' }]);
  });

  it('gives sections without topics one unlabeled page list', () => {
    expect(nav[0].topics.map((topic) => topic.label)).toEqual([undefined]);
  });

  it('keeps draft and Fathom pages out of navigation', () => {
    expect(flat.map((item) => item.label)).not.toContain('Draft');
    expect(inPrimaryNav({ title: 'X', pillar: 'wyrd', section: 'Operate' })).toBe(true);
    expect(inPrimaryNav({ title: 'X', pillar: 'fathom', section: 'Operate' })).toBe(false);
  });

  it('shows draft pages when drafts are enabled for dev', () => {
    const labels = buildNav(corpus, true).flatMap((group) => group.items.map((item) => item.label));
    expect(labels).toContain('Draft');
  });
});

describe('firstPage', () => {
  it('returns the first published page of a section', () => {
    expect(firstPage('Get started', nav)).toEqual({ label: 'Introduction', path: '/get-started/introduction/' });
  });

  it('returns undefined for a section with no published page', () => {
    expect(firstPage('Operate', nav)).toBeUndefined();
  });
});

describe('locate', () => {
  it('names the section and page for the current location', () => {
    expect(locate('/wyrd/declare/service/', '/wyrd', nav)).toEqual({
      section: 'Build and verify',
      item: { label: 'Define a service', path: '/declare/service/' }
    });
  });

  it('matches without a trailing slash', () => {
    expect(locate('/wyrd/reference/errors', '/wyrd', nav)?.section).toBe('Reference');
  });

  it('returns undefined off the spine', () => {
    expect(locate('/wyrd/', '/wyrd', nav)).toBeUndefined();
  });
});

describe('siblings', () => {
  it('the first page has no prev and a next', () => {
    expect(siblings(`/wyrd${flat[0].path}`, '/wyrd', nav)).toEqual({ prev: undefined, next: flat[1] });
  });

  it('the last page has a prev and no next', () => {
    const last = flat.length - 1;
    expect(siblings(`/wyrd${flat[last].path}`, '/wyrd', nav)).toEqual({ prev: flat[last - 1], next: undefined });
  });

  it('crosses section boundaries in sidebar order', () => {
    expect(siblings('/wyrd/get-started/quickstart/', '/wyrd', nav).next).toEqual({
      label: 'Define a service',
      path: '/declare/service/'
    });
  });

  it('returns empty for a pathname not in the spine', () => {
    expect(siblings('/wyrd/not-a-real-page/', '/wyrd', nav)).toEqual({});
  });

  it('matches regardless of trailing-slash normalization', () => {
    const withSlash = siblings(`/wyrd${flat[1].path}`, '/wyrd', nav);
    const withoutSlash = siblings(`/wyrd${flat[1].path.replace(/\/$/, '')}`, '/wyrd', nav);
    expect(withoutSlash).toEqual(withSlash);
  });
});
