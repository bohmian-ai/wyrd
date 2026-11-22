// Real-browser self-check for the workspace signup design preview. Serves the repository root on an ephemeral
// localhost port, drives dist/ with the Playwright already installed for wyrd-ui, and writes screenshots to a temp
// directory outside the source tree. Fails on any page error, script dialog, or request outside design/mock/.
// Run after building: mise exec -C changes/active/authorization-model/design/mock -- node selfcheck.mjs
// `--shots` only captures screenshots (light and dark, 1280 and 390 wide) and prints errors and escaped requests.
import { createRequire } from 'node:module';
import { createServer } from 'node:http';
import { readFile, mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve, extname, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import { STATES, slugify, slugError } from './src/data.js';

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
const scope = `${origin}/changes/active/authorization-model/design/mock/`;
const base = `${scope}dist/index.html`;
const shots = await mkdtemp(join(tmpdir(), 'wyrd-signup-mock-'));
const only = process.argv.includes('--shots');

const browser = await chromium.launch();
const errors = [];
const escaped = [];
const passed = [];
const check = async (name, fn) => { await fn(); passed.push(name); };

function watch(page) {
  page.on('pageerror', (e) => errors.push(`${page.url().split('index.html')[1]}: ${e.message}`));
  page.on('console', (m) => m.type() === 'error' && errors.push(`${page.url().split('index.html')[1]}: ${m.text()}`));
  page.on('dialog', () => errors.push('a script dialog opened'));
  page.on('request', (r) => { const u = r.url(); if (!u.startsWith(scope) && !u.startsWith('data:')) escaped.push(u); });
}
async function open(page, hash, mode) {
  await page.goto('about:blank');
  await page.goto(`${base}${mode === 'light' ? '?mode=light' : ''}${hash}`);
  await page.waitForSelector('main.entry');
  await page.evaluate(() => document.fonts.ready);
}
const slugOf = (hash) => hash.replace(/^#\/?/, '').replace(/[^a-z0-9]+/gi, '-').replace(/-$/, '') || 'sign-in';
const hashOf = (page) => page.evaluate(() => location.hash);

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
  if (only) { console.log(`errors: ${JSON.stringify(errors)}\nescaped: ${JSON.stringify(escaped)}\nscreenshots: ${shots}`); process.exit(errors.length || escaped.length ? 1 : 0); }

  const fresh = async (hash, mode) => { const p = await desk.newPage(); watch(p); await open(p, hash, mode); return p; };
  const problem = (p) => p.locator('[data-testid=problem]').innerText();

  await check('slug rules: derived from the name; 3-63 lowercase letters, digits, hyphens; no edge hyphen', async () => {
    assert.equal(slugify('  Acme Research & Co. '), 'acme-research-co');
    assert.equal(slugify('Café Müller'), 'cafe-muller');
    assert.equal(slugError(slugify('x'.repeat(80))), '');
    for (const bad of ['', 'ab', '-abc', 'abc-', 'Abc', 'a_b', 'a'.repeat(64)]) assert.notEqual(slugError(bad), '', bad);
    for (const good of ['abc', 'a-b', '123', 'a'.repeat(63)]) assert.equal(slugError(good), '', good);
  });

  await check('dark and light differ, the toggle switches them, and every preview state renders', async () => {
    const p = await fresh('#/signup');
    const bg = () => p.evaluate(() => getComputedStyle(document.body).backgroundColor);
    const dark = await bg();
    await p.click('[data-testid=theme]');
    assert.notEqual(await bg(), dark);
    for (const [label, hash] of STATES) {
      await p.selectOption('[data-testid=states]', hash);
      await p.waitForSelector('main.entry');
      assert.equal(await hashOf(p), hash);
      assert.equal(await p.locator('[data-testid=missing]').count(), 0, label);
    }
    await p.evaluate(() => { location.hash = '#/nope'; });
    await p.locator('[data-testid=missing]').waitFor();
    await p.close();
  });

  await check('signup: only enabled providers, disabled, refused proof, and 429 with retry minutes', async () => {
    const p = await fresh('#/signup');
    assert.deepEqual(await p.locator('button[data-provider]').allInnerTexts(), ['Continue with Google', 'Continue with GitHub']);
    assert.ok(await p.getByRole('link', { name: 'Already have a workspace? Sign in' }).isVisible());
    await open(p, '#/signup?providers=github');
    assert.deepEqual(await p.locator('button[data-provider]').allInnerTexts(), ['Continue with GitHub']);
    await open(p, '#/signup?preview=disabled');
    assert.equal(await p.locator('button[data-provider]').count(), 0);
    assert.ok(await p.getByRole('link', { name: 'Sign in to an existing workspace' }).isVisible());
    await open(p, '#/signup?signup=failed');
    assert.match(await problem(p), /not confirmed[\s\S]*Try again/);
    assert.equal(await p.locator('button[data-provider]').count(), 2);
    await open(p, '#/signup?preview=throttled');
    assert.match(await problem(p), /Too many workspace signups[\s\S]*Try again in 23 minutes\.[\s\S]*WYRD_AUTH_429_SIGNUP_ATTEMPTS/);
    await p.close();
  });

  await check('journey: provider, name, derived then edited slug, create, land on /t/<slug>', async () => {
    const p = await fresh('#/signup');
    await p.click('button[data-provider=google]');
    assert.equal(await p.locator('[data-testid=identity]').innerText(), 'Signed in with Google as sam@example.com.');
    assert.equal(await p.locator('[data-testid=turnstile]').count(), 0);
    await p.fill('#display-name', 'Acme Research');
    assert.equal(await p.inputValue('#slug'), 'acme-research');
    assert.equal(await p.locator('[data-testid=url]').innerText(), '/t/acme-research');
    await p.fill('#slug', 'acme-lab');
    await p.fill('#display-name', 'Acme Research Group');
    assert.equal(await p.inputValue('#slug'), 'acme-lab', 'an edited slug no longer follows the name');
    await p.click('[data-testid=create]');
    assert.ok(await p.locator('[data-testid=create]').isDisabled());
    assert.equal(await p.locator('[data-testid=create]').innerText(), 'Creating workspace…');
    await p.locator('[data-testid=created]').waitFor();
    await p.locator('[data-testid=landing]').waitFor();
    assert.equal(await hashOf(p), '#/t/acme-lab');
    await p.close();
  });

  await check('inline validation is tied to its field and blocks creation', async () => {
    const p = await fresh('#/signup/workspace?identity=github');
    assert.equal(await p.locator('[data-testid=identity]').innerText(), 'Signed in with GitHub.');
    await p.click('[data-testid=create]');
    assert.equal(await p.getAttribute('#display-name', 'aria-invalid'), 'true');
    assert.equal(await p.locator(`#${await p.getAttribute('#display-name', 'aria-describedby')}`).innerText(), '⊘ Enter a workspace name.');
    assert.equal(await p.evaluate(() => document.activeElement.id), 'display-name');
    await p.fill('#display-name', 'Acme');
    for (const [value, reason] of [['-acme', /not a hyphen/], ['ac', /at least 3/], ['Acme_1', /lowercase letters, digits and hyphens/]]) {
      await p.fill('#slug', value);
      assert.equal(await p.getAttribute('#slug', 'aria-invalid'), 'true');
      assert.match(await p.getAttribute('#slug', 'aria-describedby'), /slug-error/);
      assert.match(await p.locator('#slug-error').innerText(), reason);
      await p.click('[data-testid=create]');
      assert.equal(await p.locator('[data-testid=create]').innerText(), 'Create workspace');
    }
    await p.fill('#slug', 'acme-1');
    assert.equal(await p.getAttribute('#slug', 'aria-invalid'), null);
    for (const id of (await p.getAttribute('#slug', 'aria-describedby')).split(' ')) assert.equal(await p.locator(`#${id}`).count(), 1, id);
    await p.close();
  });

  await check('a taken slug answers 409 on the slug field, and editing the slug clears it', async () => {
    const p = await fresh('#/signup/workspace');
    await p.fill('#display-name', 'Acme');
    await p.click('[data-testid=create]');
    assert.match(await problem(p), /already taken[\s\S]*WYRD_SPEC_409_CONFLICT/);
    assert.equal(await p.getAttribute('#slug', 'aria-invalid'), 'true');
    assert.match(await p.getAttribute('#slug', 'aria-describedby'), /\bproblem\b/);
    assert.equal(await p.inputValue('#display-name'), 'Acme');
    await p.fill('#slug', 'acme-2');
    assert.equal(await p.locator('[data-testid=problem]').count(), 0);
    await p.close();
  });

  await check('Turnstile placeholder appears only when configured; an unpassed challenge creates nothing', async () => {
    const p = await fresh('#/signup/workspace?challenge=on');
    assert.match(await p.locator('[data-testid=turnstile]').innerText(), /Cloudflare Turnstile widget/);
    await p.fill('#display-name', 'Acme Research');
    await p.click('[data-testid=create]');
    assert.match(await problem(p), /Challenge not passed/);
    await p.getByLabel('Simulate a passed challenge').check();
    await p.click('[data-testid=create]');
    await p.locator('[data-testid=landing]').waitFor();
    await p.close();
  });

  await check('refusals are stable problems; ended attempts offer the way out, not the form', async () => {
    const p = await fresh('#/signup/workspace?preview=throttled');
    assert.match(await problem(p), /Try again in 23 minutes\./);
    assert.ok(await p.locator('[data-testid=create]').isEnabled());
    await open(p, '#/signup/workspace?preview=limit');
    assert.match(await problem(p), /maximum number of workspaces \(3\)[\s\S]*WYRD_AUTH_403_SIGNUP_WORKSPACE_LIMIT/);
    await open(p, '#/signup/workspace?preview=disabled');
    assert.equal(await p.locator('form').count(), 0);
    assert.ok(await p.getByRole('link', { name: 'Sign in to an existing workspace' }).isVisible());
    await open(p, '#/signup/workspace?preview=expired');
    assert.equal(await p.locator('form').count(), 0);
    await p.getByRole('link', { name: 'Start again' }).click();
    assert.equal(await hashOf(p), '#/signup');
    await open(p, '#/signup/workspace?preview=creating');
    assert.ok(await p.locator('[data-testid=create]').isDisabled());
    assert.equal(await p.getAttribute('[data-testid=create]', 'aria-busy'), 'true');
    await p.close();
  });

  for (const mode of ['dark', 'light']) {
    await check(`keyboard reaches every control with a visible focus ring in ${mode} mode`, async () => {
      const p = await fresh('#/signup/workspace?challenge=on', mode);
      await p.focus('#display-name');
      const seen = [];
      for (let i = 0; i < 5; i++) {
        const [what, ring, ringColor, behind] = await p.evaluate(() => {
          const e = document.activeElement; const s = getComputedStyle(e);
          return [e.id || e.type || e.tagName, s.outlineStyle !== 'none' && parseFloat(s.outlineWidth) >= 2, s.outlineColor, getComputedStyle(document.querySelector('.card')).backgroundColor];
        });
        assert.ok(ring, `${what} has no focus ring`);
        assert.notEqual(ringColor, behind, `${what} ring matches the card`);
        seen.push(what);
        await p.keyboard.press('Tab');
      }
      assert.deepEqual(seen, ['display-name', 'slug', 'checkbox', 'submit', 'A']);
      await p.close();
    });
  }
} finally {
  await browser.close();
  server.close();
}

assert.deepEqual(errors, [], `page errors: ${errors.join(' | ')}`);
assert.deepEqual(escaped, [], `requests outside design/mock/: ${escaped.join(' | ')}`);
console.log(`ok · ${passed.length} checks\n${passed.map((p) => `  ✓ ${p}`).join('\n')}\nscreenshots: ${shots}`);
