import { embedCodeFromFile } from './highlight.js';

// Plain-Markdown projection of one content page for llms-full.txt. It keeps
// what a reader of the rendered page sees: embeds become fenced source through
// the same `<CodeFromFile>` resolution the site build uses, authoring
// components become their visible labels, and root-absolute links resolve
// under the deploy base exactly as rehypeBaseLinks rewrites them.

// Mirrors `paths.base` in svelte.config.js.
const BASE = '/wyrd';

/** @type {Record<string, string>} */
const LANGS = { python: 'Python', rust: 'Rust', typescript: 'TypeScript' };

// Components a page renders as structure or pictures, not text.
const COMPONENT = /<\/?(Tabs|Tab|LangTabs|LangTab|Aside|Steps|WyrdDiagram|WyrdSequence|WyrdTimeline)\b([^>]*?)\/?>/g;

/**
 * Render `source` (a page with optional frontmatter) as plain Markdown.
 * Throws for an embed the site build would also refuse.
 * @param {string} source
 * @returns {string}
 */
export function pageText(source) {
  const body = source.replace(/^---\n[\s\S]*?\n---\n/, '').replace(/<script\b[\s\S]*?<\/script>\n?/g, '');
  const { code } = embedCodeFromFile(body, fence);
  return outsideFences(code, (prose) => prose.replace(COMPONENT, component).replace(/\]\(\/(?!\/)/g, `](${BASE}/`))
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

/**
 * A Markdown fence long enough to hold any backtick run in `code`.
 * @param {string} code
 * @param {string} lang
 * @param {string} title
 */
function fence(code, lang, title) {
  const ticks = '`'.repeat(Math.max(3, ...[...code.matchAll(/`+/g)].map((m) => m[0].length + 1)));
  return `${ticks}${lang} title="${title}"\n${code}\n${ticks}`;
}

/**
 * The visible text of one authoring component tag.
 * @param {string} tag
 * @param {string} name
 * @param {string} attrs
 */
function component(tag, name, attrs) {
  if (tag.startsWith('</')) return '';
  const attr = (/** @type {string} */ key) => attrs.match(new RegExp(`\\b${key}="([^"]*)"`))?.[1];
  if (name === 'Tab') return `**${attr('label')}**`;
  if (name === 'LangTab') {
    const lang = attr('lang') ?? '';
    return `**${LANGS[lang] ?? lang}**`;
  }
  if (name === 'Aside') return `**${attr('title') ?? 'Note'}:** `;
  if (name.startsWith('Wyrd')) return `_Diagram \`${attr('name')}\`: see the web page._`;
  return '';
}

/**
 * Apply `edit` to the prose between fenced code blocks only.
 * @param {string} text
 * @param {(prose: string) => string} edit
 */
function outsideFences(text, edit) {
  return text
    .split(/(^[ \t]*(`{3,})[^\n]*\n[\s\S]*?^[ \t]*\2[ \t]*$)/m)
    .map((part, index, parts) => (index % 3 === 0 ? edit(part) : index % 3 === 1 ? part : ''))
    .join('');
}
