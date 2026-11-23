// Where the docs are published: GitHub Pages serves the build on the
// docs.bohmian.ai custom domain under the /wyrd project subpath.
// svelte.config.js uses BASE for paths.base; vite.config.ts joins both into
// the canonical and sitemap URL on every build, so a dev server or local
// preview never advertises its own host. scripts/generate_llms_txt.py mirrors both.
export const BASE = '/wyrd';
export const ORIGIN = 'https://docs.bohmian.ai';
