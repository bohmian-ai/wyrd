import type { Component } from 'svelte';

export type Pillar = 'wyrd' | 'fathom' | 'shared';

// Availability of a task journey: `runnable` steps are backed by shipped
// artifacts, `partial` pages lead with the supported portion and name the gap,
// and `planned` pages stay out of primary navigation.

// The approved reader-facing sections, in sidebar order. Only Build and verify
// has topics; every other section lists its pages directly, so navigation is
// never deeper than section → topic → page.
export const SECTIONS = [
  'Get started',
  'Build and verify',
  'Connect',
  'Operate',
  'Understand',
  'Reference'
] as const;
export type Section = (typeof SECTIONS)[number];
export const TOPICS: Partial<Record<Section, readonly string[]>> = {
  'Build and verify': ['Declare', 'Observe', 'Verify']
};

export type DocMetadata = {
  title: string;
  description?: string;
  pillar?: Pillar;
  section: Section;
  topic?: string;
  order?: number;
  draft?: boolean;
  // Date (YYYY-MM-DD) of the last commit that changed the page source. Injected
  // at build time by the `remarkUpdated` plugin; absent for uncommitted pages.
  updated?: string;
};

export type DocModule = {
  default: Component;
  metadata: DocMetadata;
};

// Eager glob: content modules are resolved at build time so the matched page
// component is available synchronously during SSR/prerender (the rendered HTML
// must contain the content, not an async-pending shell). The catch-all root is
// docs/src/content/docs; an empty tree is a valid, landing-only site. Local
// mock pages in docs/mocks/pages (gitignored) join only under
// `vite dev --mode mocks`, for reviewing the shell without publishing anything.
const modules = {
  ...import.meta.glob<DocModule>('/src/content/docs/**/*.{svx,md}', { eager: true }),
  ...(import.meta.env?.MODE === 'mocks'
    ? import.meta.glob<DocModule>('/mocks/pages/**/*.{svx,md}', { eager: true })
    : {})
};

// Deterministic content-path → slug map. This is the single slug authority for
// navigation and archetypes; scripts/generate_llms_txt.py mirrors it.
// index → directory root; .svx/.md stripped; trailing slash removed.
export function toSlug(path: string): string {
  return path
    .replace(/^\/(src\/content\/docs|mocks\/pages)\//, '')
    .replace(/\.(svx|md)$/, '')
    .replace(/(^|\/)index$/, '$1')
    .replace(/\/$/, '');
}

// Validate frontmatter for a single content file. Throws at build/module-evaluation
// time so a missing or empty title is a hard build failure that names the file.
// Generated reference pages must emit frontmatter that passes this check.
export function validateFrontmatter(contentPath: string, raw: unknown): DocMetadata {
  const meta = raw as Record<string, unknown> | null | undefined;
  const title = meta?.title;
  if (typeof title !== 'string' || title.trim() === '') {
    throw new Error(
      `Content file "${contentPath}" is missing a non-empty "title" in its frontmatter.`
    );
  }
  // `pillar` is required because it identifies the product or shared surface
  // that owns the page. A missing/invalid pillar makes product navigation and
  // machine-readable indexes ambiguous, so fail closed at build time.
  if (meta?.pillar !== 'wyrd' && meta?.pillar !== 'fathom' && meta?.pillar !== 'shared') {
    throw new Error(
      `Content file "${contentPath}" has a missing or invalid "pillar" in its ` +
        `frontmatter. Expected one of: wyrd, fathom, shared.`
    );
  }
  // `section` places the page in the approved map; a typo must not silently
  // move a page, so an unknown section or topic fails the build.
  const section = meta.section as Section;
  if (!SECTIONS.includes(section)) {
    throw new Error(
      `Content file "${contentPath}" has a missing or invalid "section" in its frontmatter. ` +
        `Expected one of: ${SECTIONS.join(', ')}.`
    );
  }
  const topics = TOPICS[section] ?? [];
  if (topics.length > 0 ? !topics.includes(meta.topic as string) : meta.topic !== undefined) {
    throw new Error(
      `Content file "${contentPath}" has an invalid "topic" for section "${section}". ` +
        (topics.length > 0 ? `Expected one of: ${topics.join(', ')}.` : 'This section has no topics.')
    );
  }
  const result: DocMetadata = { title: title.trim(), pillar: meta.pillar, section };
  if (topics.length > 0) result.topic = meta.topic as string;
  if (typeof meta?.description === 'string') result.description = meta.description;
  if (typeof meta?.order === 'number') result.order = meta.order;
  if (typeof meta?.draft === 'boolean') result.draft = meta.draft;
  if (typeof meta?.updated === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(meta.updated)) {
    result.updated = meta.updated;
  }
  return result;
}

const bySlug = new Map<string, DocModule>(
  Object.entries(modules).map(([path, mod]) => {
    const metadata = validateFrontmatter(path, mod.metadata);
    return [toSlug(path), { ...mod, metadata }];
  })
);

// Drafts exist only in dev. A production build cannot look one up, so a
// published page that links to a draft fails the prerender instead of
// shipping the draft. Single branch — no env sprawl.
const isDev = import.meta.env?.DEV ?? false;

export function getEntry(slug: string): DocModule | undefined {
  const entry = bySlug.get(slug);
  return isDev || !entry?.metadata.draft ? entry : undefined;
}

export function allSlugs(): string[] {
  return [...bySlug.keys()].filter((slug) => getEntry(slug) !== undefined);
}
