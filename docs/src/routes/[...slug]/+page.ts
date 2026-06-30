import { error } from '@sveltejs/kit';
import { allSlugs, getEntry } from '$lib/content';
import type { EntryGenerator, PageLoad } from './$types';

// Enumerate every content slug so adapter-static prerenders each page even
// though no internal links point at them yet.
export const entries: EntryGenerator = () => allSlugs().map((slug) => ({ slug }));

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
    metadata: entry.metadata
  };
};
