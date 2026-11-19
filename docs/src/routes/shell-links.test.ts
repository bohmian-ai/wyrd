import { execFileSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
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

const DOCS = new URL('../../', import.meta.url).pathname;

// A copy of the docs project with `pages` as its whole content tree and no
// application sources beside it, built and indexed exactly as production is.
function buildFixtureSite(pages: Record<string, string>): string {
  const root = mkdtempSync(path.join(tmpdir(), 'wyrd-site-'));
  const docs = path.join(root, 'docs');
  const skip = new Set(['node_modules', 'build', '.svelte-kit', 'src/content/docs'].map((dir) => path.join(DOCS, dir)));
  cpSync(DOCS, docs, { recursive: true, filter: (src) => !skip.has(src) });
  cpSync(path.join(DOCS, '../Cargo.toml'), path.join(root, 'Cargo.toml'));
  symlinkSync(path.join(DOCS, '../examples'), path.join(root, 'examples'));
  symlinkSync(path.join(DOCS, 'node_modules'), path.join(docs, 'node_modules'));
  for (const [name, body] of Object.entries(pages)) {
    const file = path.join(docs, 'src/content/docs', name);
    mkdirSync(path.dirname(file), { recursive: true });
    writeFileSync(file, body);
  }
  const env = { ...process.env };
  delete env.NODE_ENV;
  execFileSync('uv', ['run', 'python', 'scripts/generate_llms_txt.py'], { cwd: docs, env });
  execFileSync('pnpm', ['run', 'build'], { cwd: docs, env, stdio: 'pipe' });
  return path.join(docs, 'build');
}

// Query the built Pagefind index the way the site's search does, returning
// the result URLs under the deploy base.
function pagefindSearch(build: string, term: string): string[] {
  const script = `
    import { readFileSync } from 'node:fs';
    import { fileURLToPath, pathToFileURL } from 'node:url';
    const dir = ${JSON.stringify(path.join(build, 'pagefind'))};
    globalThis.fetch = async (url) => new Response(readFileSync(fileURLToPath(new URL(url))));
    const pagefind = await import(pathToFileURL(dir + '/pagefind.js').href);
    await pagefind.options({ basePath: pathToFileURL(dir + '/').href, baseUrl: '/wyrd/' });
    const search = await pagefind.search(${JSON.stringify(term)});
    const data = await Promise.all(search.results.map((result) => result.data()));
    process.stdout.write(JSON.stringify(data.map((page) => page.url)));`;
  return JSON.parse(execFileSync('node', ['--input-type=module', '-e', script], { encoding: 'utf8' }));
}

function builtFiles(dir: string): string[] {
  return readdirSync(dir, { recursive: true, encoding: 'utf8' })
    .map((name) => path.join(dir, name))
    .filter((file) => statSync(file).isFile());
}

const page = (section: string, title: string, body: string, draft = false) =>
  `---\ntitle: ${title}\npillar: wyrd\nsection: ${section}\n${draft ? 'draft: true\n' : ''}---\n\n${body}\n`;

describe('publication closure', () => {
  it('withdrawn pages leave every production discovery surface', () => {
    const build = buildFixtureSite({
      'get-started/introduction.md': page('Get started', 'Fixture start', 'Published quokkafern text. [Kept](/understand/kept/)'),
      'understand/kept.md': page('Understand', 'Kept concept', 'Retained concept.'),
      // A withdrawn page whose embed names an application file absent from the fixture.
      'get-started/withdrawn.md': page(
        'Get started',
        'Withdrawn page',
        'Deferred zebracorn prose.\n\n<CodeFromFile file="examples/support-desk-app/service/ticket-complete.yaml" lang="yaml" />',
        true
      ),
      'operate/held.md': page('Operate', 'Held operation', 'Deferred zebracorn procedure.', true)
    });

    expect(existsSync(path.join(build, 'get-started/introduction/index.html'))).toBe(true);
    expect(existsSync(path.join(build, 'get-started/withdrawn'))).toBe(false);
    expect(existsSync(path.join(build, 'operate'))).toBe(false);

    // Withdrawn prose is absent from every emitted page, script, and index;
    // only a draft's frontmatter compiles, so the content map can exclude it.
    const text = builtFiles(build).filter((file) => /\.(html|js|json|txt|css)$/.test(file));
    expect(text.filter((file) => readFileSync(file, 'utf8').includes('zebracorn'))).toEqual([]);

    // Every built page links only to built destinations; the empty Operate
    // section and the withdrawn page appear nowhere.
    const served = (href: string) => {
      const target = path.join(build, href.replace(/^\/wyrd\/?/, ''));
      return existsSync(path.join(target, 'index.html')) || (existsSync(target) && statSync(target).isFile());
    };
    const hrefs = text
      .filter((file) => file.endsWith('.html'))
      .flatMap((file) => [...readFileSync(file, 'utf8').matchAll(/href="(\/wyrd\/[^"#?]*)/g)].map((m) => m[1]));
    expect(hrefs).toContain('/wyrd/understand/kept/');
    expect(hrefs.filter((href) => !served(href))).toEqual([]);
    expect(hrefs.filter((href) => /\/(operate|get-started\/withdrawn)\//.test(href))).toEqual([]);

    for (const index of ['llms.txt', 'llms-full.txt']) {
      const body = readFileSync(path.join(build, index), 'utf8');
      expect(body).toContain('/wyrd/understand/kept/');
      expect(body).not.toMatch(/Withdrawn page|Held operation|\/operate\/|get-started\/withdrawn/);
    }

    expect(pagefindSearch(build, 'quokkafern')).toEqual(['/wyrd/get-started/introduction/']);
    expect(pagefindSearch(build, 'zebracorn')).toEqual([]);
  }, 180_000);
});
