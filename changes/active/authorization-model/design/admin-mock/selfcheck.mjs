// Real-browser self-check for the admin console design preview. Serves the repository root on an ephemeral localhost
// port, opens every preview state with the Playwright wyrd-ui installs, in light and dark at 1280 and 390 wide, and
// writes screenshots to a temp directory outside the source tree. Fails on any page error, script dialog, sideways
// page scroll, or request outside design/mock/.
// Run after building: mise exec -C changes/active/authorization-model/design/admin-mock -- node selfcheck.mjs
import { createRequire } from 'node:module';
import { createServer } from 'node:http';
import { readFile, mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve, extname, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { STATES } from './src/data.js';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../../../../..');
const require = createRequire(join(root, 'crates/wyrd/wyrd-server/wyrd-ui/package.json'));
const { chromium } = require('playwright');

const types = { '.html': 'text/html', '.css': 'text/css', '.js': 'text/javascript', '.svg': 'image/svg+xml', '.woff2': 'font/woff2', '.woff': 'font/woff' };
const server = createServer(async (req, res) => {
  const path = resolve(root, `.${decodeURIComponent(new URL(req.url, 'http://x').pathname)}`);
  if (!path.startsWith(root)) { res.writeHead(403).end(); return; }
  try { res.writeHead(200, { 'content-type': types[extname(path)] || 'application/octet-stream' }).end(await readFile(path.endsWith('/') ? join(path, 'index.html') : path)); }
  catch { res.writeHead(404).end(); }
}).listen(0, '127.0.0.1');
await new Promise((r) => server.once('listening', r));
const origin = `http://127.0.0.1:${server.address().port}`;
const scope = `${origin}/changes/active/authorization-model/design/admin-mock/`;
const base = `${scope}dist/index.html`;
const shots = await mkdtemp(join(tmpdir(), 'wyrd-admin-mock-'));

const browser = await chromium.launch();
const errors = [];
const escaped = [];

function watch(page) {
  page.on('pageerror', (e) => errors.push(`${page.url().split('index.html')[1]}: ${e.message}`));
  page.on('console', (m) => m.type() === 'error' && errors.push(`${page.url().split('index.html')[1]}: ${m.text()}`));
  page.on('dialog', () => errors.push('a script dialog opened'));
  page.on('request', (r) => { const u = r.url(); if (!u.startsWith(scope) && !u.startsWith('data:')) escaped.push(u); });
}
async function open(page, hash, mode) {
  await page.goto('about:blank');
  await page.goto(`${base}${mode === 'light' ? '?mode=light' : ''}${hash}`);
  await page.waitForSelector('main');
  await page.evaluate(() => document.fonts.ready);
}
const slugOf = (hash) => hash.replace(/^#\/?/, '').replace(/[^a-z0-9]+/gi, '-').replace(/-$/, '') || 'sign-in';

try {
  const desk = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const phone = await browser.newContext({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });
  for (const [ctx, width] of [[desk, 1280], [phone, 390]]) {
    const page = await ctx.newPage();
    watch(page);
    for (const mode of ['dark', 'light']) for (const [, hash] of STATES) {
      await open(page, hash, mode);
      await page.screenshot({ path: join(shots, `${slugOf(hash)}-${mode}-${width}.png`), fullPage: true });
      const over = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
      if (over > 1) errors.push(`${hash} overflows sideways by ${over}px at ${width}`);
    }
    await page.close();
  }
  console.log(`states: ${STATES.length} × 2 modes × 2 widths\nerrors: ${JSON.stringify(errors)}\nescaped: ${JSON.stringify(escaped)}\nscreenshots: ${shots}`);
  process.exitCode = errors.length || escaped.length ? 1 : 0;
} finally {
  await browser.close();
  server.close();
}
