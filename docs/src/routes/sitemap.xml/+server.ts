import { allSlugs, getEntry } from '$lib/content';
import type { RequestHandler } from './$types';

// The sitemap advertises exactly the pages a production build serves: the
// landing, then every published content page (allSlugs drops drafts), with its
// last commit date when it has one. URLs are absolute on the production site.
export const prerender = true;
export const trailingSlash = 'never';

export const GET: RequestHandler = () => {
  const url = (path: string, updated?: string) =>
    `<url><loc>${__WYRD_SITE__}${path}</loc>${updated ? `<lastmod>${updated}</lastmod>` : ''}</url>`;
  const urls = [
    url('/'),
    ...allSlugs()
      .filter(Boolean)
      .map((slug) => url(`/${slug}/`, getEntry(slug)?.metadata.updated))
  ];
  const xml = `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls.join('\n')}\n</urlset>\n`;
  return new Response(xml, { headers: { 'content-type': 'application/xml; charset=utf-8' } });
};
