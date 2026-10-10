// Frontmatter-driven nav derivation. Reads validated entries from content.ts,
// keeps the published pages, and arranges them by the approved section and
// topic order. Sections appear only once they hold a published page, so the
// sidebar, pager, and shell links never point at an empty destination.

import type { DocMetadata, Section } from './content.js';
import { SECTIONS, TOPICS, allSlugs, getEntry } from './content.js';

export type NavItem = { label: string; path: string };
// A topic subhead inside a section; `label` is undefined for sections without
// topics, whose pages form one unlabeled list.
export type NavTopic = { label?: string; items: NavItem[] };
export type NavGroup = { label: Section; topics: NavTopic[]; items: NavItem[] };
export type NavEntry = { slug: string; meta: DocMetadata };

// Planned products and planned journeys are reachable by URL and search with
// their status, but never sit in primary navigation beside shipped tasks.
// Drafts join navigation only when `drafts` is set, which the dev server does.
export function inPrimaryNav(meta: DocMetadata, drafts = false): boolean {
  return (drafts || !meta.draft) && meta.pillar !== 'fathom' && meta.status !== 'planned';
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

// Arrange entries into sections (approved order), topics (approved order), and
// pages (frontmatter `order`, then slug by code point, as generate_llms_txt.py
// sorts). Pure, so the shape is testable
// against a fixture corpus independent of the pages published today.
export function buildNav(entries: NavEntry[], drafts = false): NavGroup[] {
  const rank = (a: NavEntry, b: NavEntry) =>
    (a.meta.order ?? Infinity) - (b.meta.order ?? Infinity) || (a.slug < b.slug ? -1 : a.slug > b.slug ? 1 : 0);
  const published = entries.filter(({ meta }) => inPrimaryNav(meta, drafts)).sort(rank);

  const groups: NavGroup[] = [];
  for (const section of SECTIONS) {
    const inSection = published.filter(({ meta }) => meta.section === section);
    if (inSection.length === 0) continue;
    const topics = (TOPICS[section] ?? [undefined])
      .map((label) => ({
        label,
        items: inSection
          .filter(({ meta }) => meta.topic === label)
          .map(({ slug, meta }) => ({ label: meta.title, path: slugToPath(slug) }))
      }))
      .filter((topic) => topic.items.length > 0);
    groups.push({ label: section, topics, items: topics.flatMap((topic) => topic.items) });
  }
  return groups;
}

// Lazily computed and memoized. buildNav() reads content.ts's module-level
// `bySlug`, and content.ts's eager content glob transitively imports the
// components that import this module — so computing at top level would re-enter
// content.ts mid-initialization and read `bySlug` in its TDZ. Deferring the
// first build to first access (sidebar render / siblings call) breaks that cycle.
let cache: NavGroup[] | undefined;

// Sidebar tree for the published content: ordered sections with their pages.
// The dev server also shows drafts so they can be reviewed in place.
export function navGroups(): NavGroup[] {
  return (cache ??= buildNav(
    allSlugs().map((slug) => ({ slug, meta: getEntry(slug)!.metadata })),
    import.meta.env?.DEV ?? false
  ));
}

// First published page of a section, or undefined while the section is empty.
// Shell and landing links resolve through this so they never outrun content.
export function firstPage(section: Section, nav = navGroups()): NavItem | undefined {
  return nav.find((group) => group.label === section)?.items[0];
}

// Section and page for a base-prefixed browser pathname: the current-location
// cue shown by the sidebar and the narrow-screen navigation control.
export function locate(
  pathname: string,
  base = '/wyrd',
  nav = navGroups()
): { section: Section; item: NavItem } | undefined {
  const target = normalize(stripBase(pathname, base));
  for (const group of nav) {
    const item = group.items.find((it) => normalize(it.path) === target);
    if (item) return { section: group.label, item };
  }
  return undefined;
}

// Return the prev and next pages relative to a given browser pathname.
// pathname is base-prefixed (e.g. `/wyrd/cards/`); comparison is base-normalized.
export function siblings(
  pathname: string,
  base = '/wyrd',
  nav = navGroups()
): { prev?: NavItem; next?: NavItem } {
  const flat = nav.flatMap((group) => group.items);
  const target = normalize(stripBase(pathname, base));
  const i = flat.findIndex((it) => normalize(it.path) === target);
  if (i === -1) return {};
  return { prev: flat[i - 1], next: flat[i + 1] };
}
