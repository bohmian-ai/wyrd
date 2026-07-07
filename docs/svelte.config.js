import adapter from '@sveltejs/adapter-static';
import { mdsvex, escapeSvelte } from 'mdsvex';
import { createHighlighter } from 'shiki';
import { wyrdLight, wyrdDark, WYRD_THEMES } from './src/lib/shiki-theme.js';
import rehypeSlug from 'rehype-slug';
import rehypeAutolinkHeadings from 'rehype-autolink-headings';
import { fileURLToPath } from 'url';
import path from 'path';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

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
// ink frame + 5px radius + hard shadow live on `.mk .main table` (arcade.css);
// the `.table-scroll` wrapper is the horizontal-scroll container.
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

// One shared Shiki highlighter on the custom Wyrd themes (brand-token syntax
// colors). Dual-theme output emits inline `--shiki-dark` custom props; arcade.css
// activates them under `:root[data-theme="dark"]`.
const highlighter = await createHighlighter({
  themes: [wyrdLight, wyrdDark],
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

// Every component an `.svx`/`.md` page may reference without a per-page import.
// Includes the highlighter's auto-injected <CodeBlock>.
const AUTHORING_COMPONENTS = [
  'CodeBlock',
  'Aside',
  'LangTabs',
  'LangTab',
  'CodeFromFile',
  'CardTileGrid',
  'CardTile',
  'DataTable',
  'CardSummary',
  'Pagination',
  'Toc',
  'Tabs',
  'Tab',
  'Steps',
  'WyrdFlowDiagram',
  'WyrdSequenceDiagram',
  'WyrdServerGraph',
  'WyrdShutdownGraph'
];

// Svelte preprocessor that runs AFTER mdsvex. mdsvex 0.12.7's layout-module
// `Components.*` rewrite only fires on parsed hast `element` nodes — never on
// raw markdown-authored tags or the highlighter's injected `<CodeBlock>` string
// — so those names are otherwise undefined at prerender and the page 404s. This
// injects bare-name imports for exactly the components a page references into
// its instance script, so content authors just drop the tag in.
function injectMdsvexComponents() {
  // Instance (non-module) <script>; mdsvex always emits one for the layout import.
  const instanceScript = /<script(?![^>]*\bmodule\b)[^>]*>/;
  return {
    name: 'inject-mdsvex-components',
    /** @param {{ content: string, filename?: string }} input */
    markup({ content, filename }) {
      if (!filename || !/\.(svx|md)$/.test(filename)) return;
      const used = AUTHORING_COMPONENTS.filter((name) =>
        new RegExp(`<${name}[\\s/>]`).test(content)
      );
      if (used.length === 0) return;
      const importLine = `\timport { ${used.join(', ')} } from '$lib/mdsvex/components.js';`;
      const match = content.match(instanceScript);
      if (match && match.index !== undefined) {
        const at = match.index + match[0].length;
        return { code: `${content.slice(0, at)}\n${importLine}\n${content.slice(at)}` };
      }
      return { code: `<script>\n${importLine}\n</script>\n${content}` };
    }
  };
}

/** @type {import('mdsvex').MdsvexOptions} */
const mdsvexOptions = {
  extensions: ['.svx', '.md'],
  layout: path.join(__dirname, 'src/lib/mdsvex/Layout.svelte'),
  highlight: {
    highlighter: async (code, lang = 'text') => {
      const themes = WYRD_THEMES;
      let html;
      try {
        html = highlighter.codeToHtml(code, { lang: lang || 'text', themes });
      } catch {
        html = highlighter.codeToHtml(code, { lang: 'text', themes });
      }
      // Wrap with CodeBlock (bare name supplied by injectMdsvexComponents) to
      // add copy-to-clipboard and the Direction A `.ch` header. escapeSvelte'd
      // html means backticks/braces inside Shiki output don't break the template
      // literal; CodeBlock renders {@html html} verbatim so dual-theme inline
      // --shiki-dark props survive. `lang` drives the header's language label.
      return `<CodeBlock lang="${lang || 'text'}" html={\`${escapeSvelte(html)}\`} />`;
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
  preprocess: [mdsvex(mdsvexOptions), injectMdsvexComponents()],
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
