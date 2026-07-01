import { createHighlighter } from 'shiki';
import { wyrdLight, wyrdDark, WYRD_THEMES } from './shiki-theme.js';

// Shared dual-theme highlighter for CodeFromFile (the same custom Wyrd theme
// pair the mdsvex fence highlighter uses in svelte.config.js, so embedded-file
// code blocks and Markdown fences render identically). The top-level await
// resolves before any importing module's body runs, so the `highlight()`
// wrapper below is synchronous at call sites and produces the highlighted HTML
// inline into the prerendered output (no client `{#await}`).
const highlighter = await createHighlighter({
  themes: [wyrdLight, wyrdDark],
  langs: ['rust', 'python', 'yaml', 'toml', 'bash', 'json', 'text']
});

const THEMES = WYRD_THEMES;

export function highlight(code: string, lang: string): string {
  try {
    return highlighter.codeToHtml(code, { lang, themes: THEMES });
  } catch {
    return highlighter.codeToHtml(code, { lang: 'text', themes: THEMES });
  }
}
