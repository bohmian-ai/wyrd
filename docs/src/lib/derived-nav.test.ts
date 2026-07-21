import { describe, it, expect } from 'vitest';
import { GROUP_ORDER, navGroups, siblings, stripBase } from './derived-nav.js';

const nav = navGroups();
// The flat spine mirrors derived-nav's internal navFlat (groups.flatMap(items)),
// so boundary assertions track the real content tree without hardcoding slugs.
const flat = nav.flatMap((g) => g.items);

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

describe('nav ordering', () => {
  it('orders named groups by GROUP_ORDER', () => {
    const ranked = nav
      .map((g) => GROUP_ORDER.findIndex((k) => k.toLowerCase() === g.label.toLowerCase()))
      .filter((i) => i !== -1);
    const sorted = [...ranked].sort((a, b) => a - b);
    expect(ranked).toEqual(sorted);
  });

  it('places every configured group ahead of any unconfigured one', () => {
    const isConfigured = (label: string) =>
      GROUP_ORDER.some((k) => k.toLowerCase() === label.toLowerCase());
    const firstUnconfigured = nav.findIndex((g) => !isConfigured(g.label));
    if (firstUnconfigured === -1) return;
    const lastConfigured = nav.map((g) => isConfigured(g.label)).lastIndexOf(true);
    expect(lastConfigured).toBeLessThan(firstUnconfigured);
  });

  it('exposes a non-empty spine of pages', () => {
    expect(flat.length).toBeGreaterThan(0);
  });

  it('organizes the sidebar by reader intent', () => {
    expect(nav.map((group) => group.label)).toEqual([
      'Start here',
      'Learn Wyrd',
      'Build with Wyrd',
      'Products and components',
      'Reference',
      'Operate Wyrd',
      'For agents'
    ]);
  });

  it('keeps product topics nested inside the products section', () => {
    const products = nav.find((group) => group.label === 'Products and components');
    expect(products?.sections.map((section) => section.label)).toEqual([
      'Overview',
      'Cards',
      'Bifrost',
      'Skald',
      'Fathom'
    ]);
    expect(products?.items.some((item) => item.path === '/bifrost/')).toBe(true);
    expect(products?.items.some((item) => item.path === '/cards/')).toBe(true);
  });

  it('marks Fathom as coming soon without hiding it from navigation', () => {
    const fathom = flat.find((item) => item.path === '/fathom/');
    expect(fathom?.soon).toBe(true);
  });
});

describe('siblings', () => {
  it('the first page has no prev and a next', () => {
    const first = flat[0];
    const pair = siblings(`/wyrd${first.path}`, '/wyrd');
    expect(pair.prev).toBeUndefined();
    expect(pair.next).toEqual(flat[1]);
  });

  it('the last page has a prev and no next', () => {
    const last = flat[flat.length - 1];
    const pair = siblings(`/wyrd${last.path}`, '/wyrd');
    expect(pair.next).toBeUndefined();
    expect(pair.prev).toEqual(flat[flat.length - 2]);
  });

  it('a middle page links to both neighbours', () => {
    const mid = Math.floor(flat.length / 2);
    const pair = siblings(`/wyrd${flat[mid].path}`, '/wyrd');
    expect(pair.prev).toEqual(flat[mid - 1]);
    expect(pair.next).toEqual(flat[mid + 1]);
  });

  it('returns empty for a pathname not in the spine', () => {
    expect(siblings('/wyrd/not-a-real-page/', '/wyrd')).toEqual({});
  });

  it('matches regardless of trailing-slash normalization', () => {
    const target = flat[1];
    const withSlash = siblings(`/wyrd${target.path}`, '/wyrd');
    const withoutSlash = siblings(`/wyrd${target.path.replace(/\/$/, '')}`, '/wyrd');
    expect(withoutSlash).toEqual(withSlash);
  });
});
