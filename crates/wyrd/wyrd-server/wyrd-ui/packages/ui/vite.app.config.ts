/**
 * Builds `dist/wyrd-view.html`: the MCP App as one HTML file with its script,
 * styles and fonts inline, so a host needs no CSP `connectDomains` or assets.
 */
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig, type Plugin } from 'vite';

/** Fold the IIFE chunk and its stylesheet into a single HTML asset. */
const singleFile: Plugin = {
  name: 'wyrd-single-file',
  enforce: 'post',
  generateBundle(_, bundle) {
    let js = '';
    let css = '';
    for (const [name, output] of Object.entries(bundle)) {
      if (output.type === 'chunk') js += output.code;
      else if (name.endsWith('.css')) css += String(output.source);
      delete bundle[name];
    }
    this.emitFile({
      type: 'asset',
      fileName: 'wyrd-view.html',
      source: `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Wyrd view</title><style>${css.replaceAll('</style', '<\\/style')}</style></head><body><script>${js.replaceAll('</script', '<\\/script')}</script></body></html>`
    });
  }
};

export default defineConfig({
  plugins: [svelte(), singleFile],
  build: {
    outDir: 'dist',
    emptyOutDir: false,
    assetsInlineLimit: Number.MAX_SAFE_INTEGER,
    lib: { entry: 'src/app/main.ts', formats: ['iife'], name: 'WyrdView', fileName: 'wyrd-view' }
  }
});
