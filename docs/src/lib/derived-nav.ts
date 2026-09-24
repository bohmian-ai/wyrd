// Frontmatter-driven nav derivation. Reads validated entries from content.ts,
// filters to published pages, maps content groups into reader-intent sections,
// and exposes the sidebar tree plus a flat spine for prev/next pagination.
//
// `section` is an escape hatch for pages that cross the normal group mapping.
// The common case only needs a product/topic `group`, so adding a page does not
// require editing a central navigation list.

import type { DocMetadata } from './content.js';
import { allSlugs, getEntry } from './content.js';

export type NavItem = { label: string; path: string };
export type NavSection = { label: string; items: NavItem[] };
export type NavGroup = {
  label: string;
  sections: NavSection[];
  items: NavItem[];
};

// The first level is a short task path: start, build and verify, run, automate,
// then background and lookup. Product/topic groups nest inside those sections as
// subheads, so a page is found by what the reader is trying to do.
export const GROUP_ORDER: readonly string[] = [
  'Get started',
  'Build and verify',
  'Run Wyrd',
  'Automate',
  'Understand',
  'Reference'
];

const GROUP_SECTION: Record<string, string> = {
  Overview: 'Get started',
  'Get Started': 'Get started',
  Tutorials: 'Get started',
  'How-to': 'Build and verify',
  Cards: 'Build and verify',
  Bifrost: 'Build and verify',
  Skald: 'Build and verify',
  'Self-hosting': 'Run Wyrd',
  'API and CLI': 'Automate',
  'Agents and MCP': 'Automate',
  Concepts: 'Understand',
  Architecture: 'Understand',
  Reference: 'Reference'
};

const GROUP_ORDER_WITHIN_SECTION: readonly string[] = [
  'Overview',
  'Get Started',
  'Tutorials',
  'How-to',
  'Cards',
  'Bifrost',
  'Skald',
  'Self-hosting',
  'API and CLI',
  'Agents and MCP',
  'Concepts',
  'Architecture',
  'Reference'
];

function sectionFor(meta: DocMetadata): string {
  if (meta.section) return meta.section;
  if (meta.group && GROUP_SECTION[meta.group]) return GROUP_SECTION[meta.group];
  return 'Understand';
}

function groupOrderIndex(label: string): number {
  return GROUP_ORDER_WITHIN_SECTION.findIndex(
    (group) => group.toLowerCase() === label.toLowerCase()
  );
}

function sectionOrderIndex(label: string): number {
  return GROUP_ORDER.findIndex((group) => group.toLowerCase() === label.toLowerCase());
}

function sortLabels(labels: string[], indexOf: (label: string) => number): void {
  labels.sort((a, b) => {
    const ia = indexOf(a);
    const ib = indexOf(b);
    if (ia !== -1 && ib !== -1) return ia - ib;
    if (ia !== -1) return -1;
    if (ib !== -1) return 1;
    return a.localeCompare(b);
  });
}

function navItem(slug: string, meta: DocMetadata): NavItem {
  return { label: meta.title, path: slugToPath(slug) };
}

// Convert a content slug to a base-free nav path. Empty slug → root '/';
// all others get a leading and trailing slash for canonical form.
function slugToPath(slug: string): string {
  return slug === '' ? '/' : `/${slug}/`;
}

// Strip a leading base segment from a browser pathname. nav paths are
// base-free; the SvelteKit router gives base-prefixed pathnames.
// Mirrors `paths.base = '/wyrd'` in svelte.config.js.
export function stripBase(pathname: string, base = '/wyrd'): string {
  if (base && pathname.startsWith(base)) {
    const rest = pathname.slice(base.length);
    return rest === '' ? '/' : rest;
  }
  return pathname;
}

function normalize(path: string): string {
  return path === '/' ? '/' : path.replace(/\/$/, '');
}

function buildNav(): { nav: NavGroup[]; flat: NavItem[] } {
  type Entry = { slug: string; meta: DocMetadata };

  const entries: Entry[] = allSlugs()
    .map((slug) => ({ slug, meta: getEntry(slug)!.metadata }))
    // Planned products are reachable by URL and search with their status, but
    // never sit in primary navigation beside shipped tasks.
    .filter(({ meta }) => !meta.draft && meta.pillar !== 'fathom');

  const sectionBuckets = new Map<string, Map<string, Entry[]>>();
  for (const entry of entries) {
    const section = sectionFor(entry.meta);
    const group = entry.meta.group ?? entry.meta.title;
    const groups = sectionBuckets.get(section) ?? new Map<string, Entry[]>();
    const items = groups.get(group) ?? [];
    items.push(entry);
    groups.set(group, items);
    sectionBuckets.set(section, groups);
  }

  for (const groups of sectionBuckets.values()) {
    for (const items of groups.values()) {
      items.sort((a, b) => {
        const oa = a.meta.order ?? Infinity;
        const ob = b.meta.order ?? Infinity;
        if (oa !== ob) return oa - ob;
        return a.slug.localeCompare(b.slug);
      });
    }
  }

  const sectionLabels = [...sectionBuckets.keys()];
  sortLabels(sectionLabels, sectionOrderIndex);

  const groups: NavGroup[] = [];
  for (const sectionLabel of sectionLabels) {
    const buckets = sectionBuckets.get(sectionLabel)!;
    const groupLabels = [...buckets.keys()];
    sortLabels(groupLabels, groupOrderIndex);
    const sections = groupLabels.map((label) => ({
      label,
      items: buckets.get(label)!.map(({ slug, meta }) => navItem(slug, meta))
    }));
    groups.push({
      label: sectionLabel,
      sections,
      items: sections.flatMap((section) => section.items)
    });
  }

  return { nav: groups, flat: groups.flatMap((group) => group.items) };
}

// Lazily computed and memoized. buildNav() reads content.ts's module-level
// `bySlug`, and content.ts's eager content glob transitively imports the
// components that import this module — so computing at top level would re-enter
// content.ts mid-initialization and read `bySlug` in its TDZ. Deferring the
// first build to first access (sidebar render / siblings call) breaks that cycle.
let cache: { nav: NavGroup[]; flat: NavItem[] } | undefined;
function derived(): { nav: NavGroup[]; flat: NavItem[] } {
  return (cache ??= buildNav());
}

// Sidebar tree: ordered groups with their items.
export function navGroups(): NavGroup[] {
  return derived().nav;
}

// Section and page for a base-prefixed browser pathname: the current-location
// cue shown by the sidebar and the narrow-screen navigation control.
export function locate(
  pathname: string,
  base = '/wyrd'
): { section: string; item: NavItem } | undefined {
  const target = normalize(stripBase(pathname, base));
  for (const group of derived().nav) {
    const item = group.items.find((it) => normalize(it.path) === target);
    if (item) return { section: group.label, item };
  }
  return undefined;
}

// Return the prev and next pages relative to a given browser pathname.
// pathname is base-prefixed (e.g. `/wyrd/cards/`); comparison is base-normalized.
export function siblings(
  pathname: string,
  base = '/wyrd'
): { prev?: NavItem; next?: NavItem } {
  const navFlat = derived().flat;
  const target = normalize(stripBase(pathname, base));
  const i = navFlat.findIndex((it) => normalize(it.path) === target);
  if (i === -1) return {};
  return { prev: navFlat[i - 1], next: navFlat[i + 1] };
}
