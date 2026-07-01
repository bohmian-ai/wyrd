import { error } from '@sveltejs/kit';
import { allSlugs, getEntry } from '$lib/content';
import type { DocMetadata } from '$lib/content';
import type { EntryGenerator, PageLoad } from './$types';

// Enumerate every content slug so adapter-static prerenders each page even
// though no internal links point at them yet.
export const entries: EntryGenerator = () => allSlugs().map((slug) => ({ slug }));

// Archetype is resolved at build time from frontmatter + slug shape.
// No server runtime; the four content archetypes cover all non-home routes.
export type Archetype = 'article' | 'hub' | 'reference' | 'fathom';

function resolveArchetype(slug: string, meta: DocMetadata): Archetype {
  if (meta.pillar === 'fathom') return 'fathom';
  // Directory-index: another slug starts with this slug as a path prefix,
  // meaning this entry is a section parent.
  if (allSlugs().some((s) => s.startsWith(`${slug}/`))) return 'hub';
  if (meta.group === 'api' || meta.group === 'reference') return 'reference';
  return 'article';
}

export const load: PageLoad = ({ params }) => {
  // `trailingSlash: 'always'` leaves a trailing slash on the rest param; the
  // content map keys are slash-free, so normalize before lookup.
  const slug = (params.slug ?? '').replace(/\/$/, '');
  const entry = getEntry(slug);
  if (!entry) {
    throw error(404, `No content for "${slug || '/'}"`);
  }
  return {
    slug,
    metadata: entry.metadata,
    archetype: resolveArchetype(slug, entry.metadata)
  };
};
