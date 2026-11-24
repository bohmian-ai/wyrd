// Builds the preview into dist/. Svelte, Vite and the fonts resolve from wyrd-ui's installed node_modules, so this
// mock installs nothing of its own.
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const ui = join(here, '../../../../../crates/wyrd/wyrd-server/wyrd-ui');
const { build } = await import(join(ui, 'node_modules/vite/dist/node/index.js'));
const { svelte } = await import(join(ui, 'node_modules/@sveltejs/vite-plugin-svelte/src/index.js'));

const BORROWED = /^(svelte|@fontsource\/)/;
const fromWyrdUi = {
  name: 'resolve-from-wyrd-ui',
  enforce: 'pre',
  resolveId(id, importer) {
    if (!BORROWED.test(id) || !importer?.startsWith(here)) return null;
    return this.resolve(id, join(ui, 'package.json'), { skipSelf: true });
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
