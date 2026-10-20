import { statSync } from 'node:fs';
import { createRawSnippet, type Component } from 'svelte';
import { render } from 'svelte/server';
import { describe, expect, it, vi } from 'vitest';

// Server-render the landing, the shell (home and doc modes), and the error page
// with SvelteKit's runtime modules stubbed, then require every internal link to
// reach a page the build actually emits or a file it serves from public/.
const app = vi.hoisted(() => ({
  page: {
    url: new URL('https://docs.test/wyrd/'),
    route: { id: '/' as string | null },
    error: null as { message: string } | null,
    status: 200
  }
}));
vi.mock('$app/paths', () => ({ base: '/wyrd' }));
vi.mock('$app/state', () => app);
vi.mock('$app/navigation', () => ({ afterNavigate: () => {} }));

const { default: Landing } = await import('./+page.svelte');
const { load: landingData } = await import('./+page.server');
const { default: Layout } = await import('./+layout.svelte');
const { default: ErrorPage } = await import('./+error.svelte');
const { allSlugs } = await import('$lib/content');

const routes = new Set(['/', ...allSlugs().map((slug) => `/${slug}/`)]);
const publicFile = (path: string) =>
  statSync(new URL(`../../public${path}`, import.meta.url), { throwIfNoEntry: false })?.isFile() ?? false;
const children = createRawSnippet(() => ({ render: () => '<p></p>' }));

function internalLinks(component: Component<any>, props = {}): string[] {
  const { body } = render(component, { props });
  return [...body.matchAll(/href="\/wyrd(\/[^"#?]*)/g)].map((m) => m[1]);
}

function deadLinks(links: string[]): string[] {
  return links.filter((path) => !routes.has(path) && !publicFile(path));
}

describe('landing and shell actions reach real destinations', () => {
  it('landing links only to published pages', () => {
    expect(deadLinks(internalLinks(Landing, { data: landingData() }))).toEqual([]);
  });

  it('home shell header and footer link only to published pages', () => {
    app.page.route.id = '/';
    expect(deadLinks(internalLinks(Layout, { children }))).toEqual([]);
  });

  it('doc shell header, sidebar, and footer link only to published pages', () => {
    app.page.route.id = '/[...slug]';
    app.page.url = new URL('https://docs.test/wyrd/missing/');
    expect(deadLinks(internalLinks(Layout, { children }))).toEqual([]);
  });

  it('error page recovery links reach published pages', () => {
    app.page.status = 404;
    app.page.error = { message: 'No content for "missing".' };
    expect(deadLinks(internalLinks(ErrorPage))).toEqual([]);
  });

  it('always offers a way home from the error page', () => {
    expect(internalLinks(ErrorPage)).toContain('/');
  });
});
