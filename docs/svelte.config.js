import adapter from '@sveltejs/adapter-static';
import { mdsvex, escapeSvelte } from 'mdsvex';
import { createHighlighter } from 'shiki';
import rehypeSlug from 'rehype-slug';
import rehypeAutolinkHeadings from 'rehype-autolink-headings';

// Single source of truth for the GitHub Pages project subpath. Used both for
// SvelteKit's `paths.base` and the rehype link rewriter below, so markdown
// content can author clean root-absolute links (`/cards/data/`) and have them
// resolve correctly under `/wyrd/` at build time.
const BASE = '/wyrd';

// Rewrite markdown-authored absolute links/images so they survive the Pages
// subpath. SvelteKit only resolves `<a>` in component markup, not links emitted
// from `.svx`/`.md` content, so without this every `[x](/foo)` and `<img src=/foo>`
// 404s on `/wyrd/`. Internal = starts with a single `/`, not `//`, not a full
// URL, not a bare `#anchor`, and not already base-prefixed. Idempotent.
function rehypeBaseLinks() {
  const isInternal = (v) =>
    typeof v === 'string' &&
    v.startsWith('/') &&
    !v.startsWith('//') &&
    v !== BASE &&
    !v.startsWith(`${BASE}/`);

  const fixSrcset = (value) =>
    value
      .split(',')
      .map((part) => {
        const seg = part.trim();
        if (!seg) return seg;
        const [url, ...descriptors] = seg.split(/\s+/);
        return isInternal(url) ? [`${BASE}${url}`, ...descriptors].join(' ') : seg;
      })
      .join(', ');

  const visit = (node) => {
    if (node.type === 'element' && node.properties) {
      const p = node.properties;
      if (isInternal(p.href)) p.href = `${BASE}${p.href}`;
      if (isInternal(p.src)) p.src = `${BASE}${p.src}`;
      if (typeof p.srcSet === 'string') p.srcSet = fixSrcset(p.srcSet);
    }
    if (Array.isArray(node.children)) node.children.forEach(visit);
  };

  return (tree) => visit(tree);
}

// Wrap every markdown table in a horizontal-scroll container so wide tables
// (error catalog, schema reference, card fields) scroll within the content
// column on narrow viewports instead of forcing page-level overflow. The 2px
// ink frame + 5px radius + hard shadow live on `.table-scroll` (see wyrd.css).
function rehypeWrapTables() {
  const wrap = (node) => {
    if (!Array.isArray(node.children)) return;
    node.children = node.children.map((child) => {
      wrap(child);
      if (child.type === 'element' && child.tagName === 'table') {
        return {
          type: 'element',
          tagName: 'div',
          properties: { className: ['table-scroll'] },
          children: [child]
        };
      }
      return child;
    });
  };
  return (tree) => wrap(tree);
}

// One shared Shiki highlighter on the same dual themes the product UI uses.
// Dual-theme output emits inline `--shiki-dark` custom props; the [data-theme]
// switch CSS that activates them lands in Phase C.
const highlighter = await createHighlighter({
  themes: ['github-light', 'github-dark'],
  langs: [
    'bash',
    'shell',
    'json',
    'toml',
    'yaml',
    'rust',
    'python',
    'typescript',
    'javascript',
    'svelte',
    'html',
    'css',
    'markdown',
    'text'
  ]
});

/** @type {import('mdsvex').MdsvexOptions} */
const mdsvexOptions = {
  extensions: ['.svx', '.md'],
  highlight: {
    highlighter: async (code, lang = 'text') => {
      const themes = { light: 'github-light', dark: 'github-dark' };
      let html;
      try {
        html = highlighter.codeToHtml(code, { lang: lang || 'text', themes });
      } catch {
        html = highlighter.codeToHtml(code, { lang: 'text', themes });
      }
      return `{@html \`${escapeSvelte(html)}\`}`;
    }
  },
  rehypePlugins: [
    rehypeBaseLinks,
    rehypeWrapTables,
    rehypeSlug,
    [rehypeAutolinkHeadings, { behavior: 'wrap' }]
  ]
};

/** @type {import('@sveltejs/kit').Config} */
const config = {
  extensions: ['.svelte', '.svx', '.md'],
  preprocess: [mdsvex(mdsvexOptions)],
  kit: {
    // Static SSG with a client-side fallback shell for any non-prerendered URL.
    adapter: adapter({ fallback: '404.html' }),
    // Project subpath on GitHub Pages: https://<org>.github.io/wyrd/
    paths: { base: BASE },
    // Static assets live in public/ (favicon, llms.txt, llms-full.txt) — the
    // generator scripts write there. SvelteKit's default is static/.
    files: { assets: 'public' },
    prerender: { handleHttpError: 'warn' }
  }
};

export default config;
