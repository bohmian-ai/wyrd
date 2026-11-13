// Builds the preview into dist/. Svelte, Vite, Svelte Flow, micromark, and the fonts resolve from wyrd-ui's installed
// node_modules so no second copy is installed; only Dagre is local (see package.json).
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const ui = join(here, '../../../../../crates/wyrd/wyrd-server/wyrd-ui');
const { build } = await import(join(ui, 'node_modules/vite/dist/node/index.js'));
const { svelte } = await import(join(ui, 'node_modules/@sveltejs/vite-plugin-svelte/src/index.js'));

const BORROWED = /^(svelte|@xyflow\/|micromark|@fontsource\/)/;
const fromWyrdUi = {
  name: 'resolve-from-wyrd-ui',
  enforce: 'pre',
  async resolveId(id, importer) {
    if (!BORROWED.test(id) || !importer?.startsWith(here)) return null;
    // Svelte Flow is a dependency of the component package, not of the workspace root.
    return (
      (await this.resolve(id, join(ui, 'package.json'), { skipSelf: true })) ??
      this.resolve(id, join(ui, 'packages/ui/package.json'), { skipSelf: true })
    );
  },
};

await build({
  root: here,
  base: './',
  configFile: false,
  logLevel: 'warn',
  plugins: [fromWyrdUi, svelte({ configFile: false })],
  build: { outDir: join(here, 'dist'), emptyOutDir: true, assetsInlineLimit: 0 },
});
console.log('built', join(here, 'dist'));
