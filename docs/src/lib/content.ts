import type { Component } from 'svelte';

export type DocMetadata = {
  title?: string;
  description?: string;
  [key: string]: unknown;
};

export type DocModule = {
  default: Component;
  metadata?: DocMetadata;
};

// Eager glob: content modules are resolved at build time so the matched page
// component is available synchronously during SSR/prerender (the rendered HTML
// must contain the content, not an async-pending shell). The catch-all root is
// docs/src/content/docs — Phase D converts the remaining `.mdx` to `.svx`.
const modules = import.meta.glob<DocModule>('/src/content/docs/**/*.{svx,md}', {
  eager: true
});

function toSlug(path: string): string {
  return path
    .replace('/src/content/docs/', '')
    .replace(/\.(svx|md)$/, '')
    .replace(/(^|\/)index$/, '$1')
    .replace(/\/$/, '');
}

const bySlug = new Map<string, DocModule>(
  Object.entries(modules).map(([path, mod]) => [toSlug(path), mod])
);

export function getEntry(slug: string): DocModule | undefined {
  return bySlug.get(slug);
}

export function allSlugs(): string[] {
  return [...bySlug.keys()];
}
