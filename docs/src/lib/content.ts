import type { Component } from 'svelte';

export type Pillar = 'wyrd' | 'fathom' | 'shared';

export type DocMetadata = {
  title: string;
  description?: string;
  pillar?: Pillar;
  group?: string;
  order?: number;
  draft?: boolean;
};

export type DocModule = {
  default: Component;
  metadata: DocMetadata;
};

// Eager glob: content modules are resolved at build time so the matched page
// component is available synchronously during SSR/prerender (the rendered HTML
// must contain the content, not an async-pending shell). The catch-all root is
// docs/src/content/docs — Phase D converts the remaining `.mdx` to `.svx`.
const modules = import.meta.glob<DocModule>('/src/content/docs/**/*.{svx,md}', {
  eager: true
});

// Deterministic content-path → slug map. This is the single slug authority:
// 04 (nav), 07 (generators), and 08 (archetypes) all derive slugs from here.
// index → directory root; .svx/.md stripped; trailing slash removed.
export function toSlug(path: string): string {
  return path
    .replace('/src/content/docs/', '')
    .replace(/\.(svx|md)$/, '')
    .replace(/(^|\/)index$/, '$1')
    .replace(/\/$/, '');
}

// Validate frontmatter for a single content file. Throws at build/module-eval
// time so a missing or empty title is a hard build failure that names the file.
// Commit 07's generators must emit frontmatter that passes this check.
export function validateFrontmatter(contentPath: string, raw: unknown): DocMetadata {
  const meta = raw as Record<string, unknown> | null | undefined;
  const title = meta?.title;
  if (typeof title !== 'string' || title.trim() === '') {
    throw new Error(
      `Content file "${contentPath}" is missing a non-empty "title" in its frontmatter.`
    );
  }
  const result: DocMetadata = { title: title.trim() };
  if (typeof meta?.description === 'string') result.description = meta.description;
  if (meta?.pillar === 'wyrd' || meta?.pillar === 'fathom' || meta?.pillar === 'shared') {
    result.pillar = meta.pillar;
  }
  if (typeof meta?.group === 'string') result.group = meta.group;
  if (typeof meta?.order === 'number') result.order = meta.order;
  if (typeof meta?.draft === 'boolean') result.draft = meta.draft;
  return result;
}

const bySlug = new Map<string, DocModule>(
  Object.entries(modules).map(([path, mod]) => {
    const metadata = validateFrontmatter(path, mod.metadata);
    return [toSlug(path), { ...mod, metadata }];
  })
);

// Drafts are excluded from allSlugs() (and thus prerender/sitemap) in
// production builds; surfaced in dev. Single branch — no env sprawl.
const isDev = import.meta.env?.DEV ?? false;

export function getEntry(slug: string): DocModule | undefined {
  return bySlug.get(slug);
}

export function allSlugs(): string[] {
  return [...bySlug.keys()].filter(
    (slug) => isDev || !bySlug.get(slug)?.metadata.draft
  );
}
