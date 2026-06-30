// Frontmatter-driven nav derivation. Reads validated entries from content.ts,
// filters to wyrd-pillar non-draft pages, buckets by group, sorts by the
// declarative GROUP_ORDER config + per-entry `order` field, and exposes the
// sidebar tree plus a flat spine for prev/next pagination.
//
// Dropping a file with valid wyrd-pillar frontmatter is sufficient to place it
// in the sidebar and prev/next chain — no edit to any central list.

import type { DocMetadata } from './content.js';
import { allSlugs, getEntry } from './content.js';

export type NavItem = { label: string; path: string };
export type NavGroup = { label: string; kind?: string; items: NavItem[] };

// Declarative group-order config — the single ordering knob for group sequence.
// Groups not listed appear after all configured groups, sorted alphabetically.
// Entries within each group are sorted by `order` ASC, then by slug for a
// deterministic tiebreak so builds are reproducible.
// Diátaxis 7-section IA + How-to capability sub-groups. Groups not listed
// here appear after all configured groups, sorted alphabetically.
export const GROUP_ORDER: readonly string[] = [
  'Overview',
  'Setup',
  'Tutorials',
  'How-to',
  'Declare',
  'Evaluate',
  'Observe',
  'Operate',
  'Concepts',
  'cards',
  'api',
  'For Agents',
];

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

// Sentinel string used as a Map key for entries that carry no group.
const UNGROUPED = '\x00ungrouped';

function buildNav(): { nav: NavGroup[]; flat: NavItem[] } {
  type Entry = { slug: string; meta: DocMetadata };

  // allSlugs() excludes drafts in prod; we re-filter draft here so drafts are
  // always absent from the nav regardless of build environment.
  const entries: Entry[] = allSlugs()
    .map((slug) => ({ slug, meta: getEntry(slug)!.metadata }))
    .filter(({ meta }) => meta.pillar === 'wyrd' && !meta.draft);

  // Bucket by group label.
  const buckets = new Map<string, Entry[]>();
  for (const entry of entries) {
    const key = entry.meta.group ?? UNGROUPED;
    if (!buckets.has(key)) buckets.set(key, []);
    buckets.get(key)!.push(entry);
  }

  // Sort entries within each bucket: order ASC, slug for deterministic tiebreak.
  for (const items of buckets.values()) {
    items.sort((a, b) => {
      const oa = a.meta.order ?? Infinity;
      const ob = b.meta.order ?? Infinity;
      if (oa !== ob) return oa - ob;
      return a.slug.localeCompare(b.slug);
    });
  }

  // Sort named group keys by GROUP_ORDER; unlisted groups sort alphabetically after.
  const namedKeys = [...buckets.keys()].filter((k) => k !== UNGROUPED);
  namedKeys.sort((a, b) => {
    const ia = GROUP_ORDER.indexOf(a);
    const ib = GROUP_ORDER.indexOf(b);
    if (ia !== -1 && ib !== -1) return ia - ib;
    if (ia !== -1) return -1;
    if (ib !== -1) return 1;
    return a.localeCompare(b);
  });

  const groups: NavGroup[] = [];

  for (const key of namedKeys) {
    groups.push({
      label: key,
      items: buckets.get(key)!.map(({ slug, meta }) => ({
        label: meta.title,
        path: slugToPath(slug)
      }))
    });
  }

  // Ungrouped entries become standalone groups (single item, label = title)
  // so Sidebar.svelte's isStandalone check renders them as bare links.
  const ungrouped = buckets.get(UNGROUPED) ?? [];
  for (const { slug, meta } of ungrouped) {
    groups.push({
      label: meta.title,
      items: [{ label: meta.title, path: slugToPath(slug) }]
    });
  }

  return { nav: groups, flat: groups.flatMap((g) => g.items) };
}

const derived = buildNav();

// Sidebar tree: ordered groups with their items.
export const nav: NavGroup[] = derived.nav;

// Flat ordered list — the spine for prev/next pagination.
const navFlat: NavItem[] = derived.flat;

// Return the prev and next pages relative to a given browser pathname.
// pathname is base-prefixed (e.g. `/wyrd/cards/`); comparison is base-normalized.
export function siblings(pathname: string): { prev?: NavItem; next?: NavItem } {
  const target = normalize(stripBase(pathname));
  const i = navFlat.findIndex((it) => normalize(it.path) === target);
  if (i === -1) return {};
  return { prev: navFlat[i - 1], next: navFlat[i + 1] };
}
