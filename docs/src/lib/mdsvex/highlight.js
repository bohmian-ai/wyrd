import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { escapeSvelte } from 'mdsvex';
import { createHighlighter } from 'shiki';
import { wyrdCode, WYRD_THEME } from '../shiki-theme.js';

// Build-time code rendering for `.svx`/`.md` pages. Markdown fences and
// `<CodeFromFile>` embeds both become a `<CodeBlock>` carrying pre-highlighted
// HTML, so neither the highlighter nor any embedded source reaches the browser.

const REPO_ROOT = fileURLToPath(new URL('../../../../', import.meta.url));

// One shared Shiki highlighter on the token-driven Wyrd code theme.
const highlighter = await createHighlighter({
  themes: [wyrdCode],
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

/**
 * Highlight `code` and wrap it in the `<CodeBlock>` markup a page renders.
 * `escapeSvelte` keeps backticks and braces in Shiki output from breaking the
 * template literal; an unknown language falls back to plain text. `marked` is
 * a 1-based line list such as `3,5-7`; those lines get the `hl` class.
 * @param {string} code
 * @param {string} [lang]
 * @param {string} [title]
 * @param {string} [marked]
 */
export function codeBlock(code, lang = 'text', title, marked) {
  const lines = parseLines(marked);
  /** @type {import('shiki').ShikiTransformer[]} */
  const transformers = [
    {
      line(node, line) {
        if (lines.has(line)) this.addClassToHast(node, 'hl');
      }
    }
  ];
  const html = highlight(code, lang, transformers);
  const titleAttr = title ? ` title="${title}"` : '';
  return `<CodeBlock lang="${lang}"${titleAttr} html={\`${escapeSvelte(html)}\`} />`;
}

/**
 * Shiki HTML for `code` on the Wyrd theme; an unknown language falls back to
 * plain text. Pages outside mdsvex, such as the landing page, render the
 * result through `<CodeBlock html>` themselves.
 * @param {string} code
 * @param {string} lang
 * @param {import('shiki').ShikiTransformer[]} [transformers]
 */
export function highlight(code, lang, transformers = []) {
  try {
    return highlighter.codeToHtml(code, { lang, theme: WYRD_THEME, transformers });
  } catch {
    return highlighter.codeToHtml(code, { lang: 'text', theme: WYRD_THEME, transformers });
  }
}

/**
 * Render a markdown fence. The info string after the language may carry
 * `title="file.py"` and a `{3,5-7}` line list to mark.
 * @param {string} code
 * @param {string} [lang]
 * @param {string} [meta]
 */
export function fenceBlock(code, lang, meta = '') {
  const title = meta.match(/title="([^"]*)"/)?.[1];
  const marked = meta.match(/\{([\d,\s-]+)\}/)?.[1];
  return codeBlock(code, lang || 'text', title, marked);
}

/**
 * @param {string} [spec] 1-based lines such as `3,5-7`
 * @returns {Set<number>}
 */
function parseLines(spec) {
  const lines = new Set();
  for (const part of (spec ?? '').split(',')) {
    const [a, b] = part.trim().split('-').map((n) => Number.parseInt(n, 10));
    if (!Number.isFinite(a)) continue;
    for (let n = a; n <= (Number.isFinite(b) ? b : a); n++) lines.add(n);
  }
  return lines;
}

/**
 * Replace every `<CodeFromFile file lang [title] [region|range] [highlight] />` tag
 * with the highlighted `<CodeBlock>` for that repository file. `file` is a repo
 * path under crates/, examples/, python/, or sdks/ (leading `../` or `/` is
 * tolerated). `region` names the lines between mdBook-style `ANCHOR: name` and
 * `ANCHOR_END: name` comments, so an embed follows its code as the file
 * changes; `range` is an inclusive 1-based line span such as `5-10`.
 * `highlight` marks lines, counted within the embedded span, such as `2,4-6`.
 * Anchor comment lines never appear in the output. Throws, failing the build,
 * for a missing file, a missing region, a range past the end of the file, or a
 * non-literal attribute. Returns the embedded files so Vite rebuilds the page
 * when one changes. `render` turns each resolved body into its replacement:
 * the highlighted `<CodeBlock>` for the site, or a Markdown fence for the
 * plain-text agent reference.
 * @param {string} content
 * @param {(code: string, lang: string, title: string, marked?: string) => string} [render]
 * @returns {{ code: string, dependencies: string[] }}
 */
export function embedCodeFromFile(content, render = codeBlock) {
  /** @type {string[]} */
  const dependencies = [];
  const code = content.replace(/<CodeFromFile\b([^>]*?)\/>/g, (tag, attrs) => {
    if (attrs.includes('{')) throw new Error(`CodeFromFile: attributes must be literal strings: ${tag}`);
    const props = Object.fromEntries([...attrs.matchAll(/(\w+)="([^"]*)"/g)].map((m) => [m[1], m[2]]));
    const tail = props.file?.match(/(?:crates|examples|fixtures|python|sdks)\/.*$/)?.[0];
    if (!tail || !props.lang) throw new Error(`CodeFromFile: needs a repo "file" and a "lang": ${tag}`);
    const abs = path.join(REPO_ROOT, tail);
    if (path.relative(REPO_ROOT, abs).startsWith('..')) throw new Error(`CodeFromFile: file not found: ${props.file}`);
    let src;
    try {
      src = readFileSync(abs, 'utf8');
    } catch {
      throw new Error(`CodeFromFile: file not found: ${props.file}`);
    }
    dependencies.push(abs);
    if (props.region && props.range) throw new Error(`CodeFromFile: use "region" or "range", not both: ${tag}`);
    const body = props.region ? sliceRegion(src, props.region, tail) : props.range ? sliceRange(src, props.range) : src;
    return render(stripAnchors(body), props.lang, props.title ?? path.basename(tail), props.highlight);
  });
  return { code, dependencies };
}

/**
 * @param {string} src
 * @param {string} range
 */
function sliceRange(src, range) {
  const [a, b] = range.split('-');
  const start = Number.parseInt(a, 10);
  const end = Number.parseInt(b ?? a, 10);
  if (!Number.isFinite(start) || !Number.isFinite(end) || start < 1 || end < start) {
    throw new Error(`CodeFromFile: invalid range "${range}"`);
  }
  const lines = src.split('\n');
  if (end > lines.length) throw new Error(`CodeFromFile: range "${range}" ends past line ${lines.length}`);
  return lines.slice(start - 1, end).join('\n');
}

const ANCHOR = /\bANCHOR(_END)?:\s*([\w-]+)/;

/**
 * The lines strictly between `ANCHOR: name` and `ANCHOR_END: name`.
 * @param {string} src
 * @param {string} name
 * @param {string} file
 */
function sliceRegion(src, name, file) {
  const lines = src.split('\n');
  const at = (/** @type {boolean} */ end) => lines.findIndex((l) => {
    const m = l.match(ANCHOR);
    return m !== null && m[2] === name && Boolean(m[1]) === end;
  });
  const start = at(false);
  const end = at(true);
  if (start < 0 || end <= start) throw new Error(`CodeFromFile: region "${name}" not found in ${file}`);
  return dedent(lines.slice(start + 1, end));
}

/** @param {string} body */
function stripAnchors(body) {
  return body.split('\n').filter((l) => !ANCHOR.test(l)).join('\n');
}

/**
 * Remove the indentation every non-blank line shares, so a region cut from a
 * function body reads as top-level code.
 * @param {string[]} lines
 */
function dedent(lines) {
  const indents = lines.filter((l) => l.trim()).map((l) => l.length - l.trimStart().length);
  const cut = indents.length ? Math.min(...indents) : 0;
  return lines.map((l) => l.slice(cut)).join('\n');
}
