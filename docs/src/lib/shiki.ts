import { createHighlighter } from 'shiki';
import { wyrdCode, WYRD_THEME } from './shiki-theme.js';

// Shared highlighter for CodeFromFile (the same token-driven Wyrd
// theme the mdsvex fence highlighter uses in svelte.config.js, so embedded-file
// code blocks and Markdown fences render identically). The top-level await
// resolves before any importing module's body runs, so the `highlight()`
// wrapper below is synchronous at call sites and produces the highlighted HTML
// inline into the prerendered output (no client `{#await}`).
const highlighter = await createHighlighter({
  themes: [wyrdCode],
  langs: ['rust', 'python', 'yaml', 'toml', 'bash', 'json', 'text']
});

export function highlight(code: string, lang: string): string {
  try {
    return highlighter.codeToHtml(code, { lang, theme: WYRD_THEME });
  } catch {
    return highlighter.codeToHtml(code, { lang: 'text', theme: WYRD_THEME });
  }
}
