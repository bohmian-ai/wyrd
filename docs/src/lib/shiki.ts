import { createHighlighter } from 'shiki';

// Shared dual-theme highlighter for CodeFromFile (the same github-light /
// github-dark pair the mdsvex fence highlighter uses in svelte.config.js, so
// embedded-file code blocks and Markdown fences render identically). The
// top-level await resolves before any importing module's body runs, so the
// `highlight()` wrapper below is synchronous at call sites and produces the
// highlighted HTML inline into the prerendered output (no client `{#await}`).
const highlighter = await createHighlighter({
  themes: ['github-light', 'github-dark'],
  langs: ['rust', 'python', 'yaml', 'toml', 'bash', 'json', 'text']
});

const THEMES = { light: 'github-light', dark: 'github-dark' } as const;

export function highlight(code: string, lang: string): string {
  try {
    return highlighter.codeToHtml(code, { lang, themes: THEMES });
  } catch {
    return highlighter.codeToHtml(code, { lang: 'text', themes: THEMES });
  }
}
