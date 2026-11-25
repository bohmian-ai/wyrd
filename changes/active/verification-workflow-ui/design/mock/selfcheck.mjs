// Real-browser self-check for the verification design preview. Serves the repository root on an ephemeral localhost
// port, drives dist/ with the Playwright already installed for wyrd-ui, checks behaviour and graph geometry, and writes
// screenshots to a temp directory outside the source tree.
// Run after building: mise exec -C changes/active/verification-workflow-ui/design/mock -- node selfcheck.mjs
// `--shots` only captures screenshots and prints errors and escaped requests.
import { createRequire } from 'node:module';
import { createServer } from 'node:http';
import { readFile, mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve, extname, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';

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
const scope = `${origin}/changes/active/verification-workflow-ui/design/mock/`;
const base = `${scope}dist/index.html`;
const shots = await mkdtemp(join(tmpdir(), 'wyrd-verification-mock-'));
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
  // Every asset and every navigation must stay inside design/mock/.
  page.on('request', (r) => { const u = r.url(); if (!u.startsWith(scope) && !u.startsWith('data:')) escaped.push(u); });
}
async function open(page, hash, mode) {
  await page.goto(`${base}${mode === 'light' ? '?mode=light' : ''}${hash}`);
  await page.waitForSelector('main.view');
  await page.waitForTimeout(150);
}
async function shot(page, name) { await page.screenshot({ path: join(shots, `${name}.png`), fullPage: true }); }

const PAGES = [
  ['verify', '#/verify'],
  ['exec-ordinary', '#/x/exe-7c1e02a4'],
  ['exec-session', '#/x/exe-6c19e5f0'],
  ['exec-approval', '#/x/exe-4d1e9a07'],
  ['exec-pr', '#/x/exe-7f3a91c2'],
  ['exec-drift', '#/x/exe-d1f7a00e'],
  ['pull-482', '#/repositories/payments-api/pulls/482'],
  ['repos', '#/repositories'],
  ['repo-payments', '#/repositories/payments-api'],
  ['repo-ledger-old', '#/repositories/ledger?v=1.0.0'],
  ['repo-undeclared', '#/repositories/github/741660215'],
  ['verifier-policy', '#/cards/verifiers/policy-review@1.0.0'],
  ['settings', '#/settings/github'],
];
const GRAPHS = ['exe-7c1e02a4', 'exe-6c19e5f0', 'exe-4d1e9a07', 'exe-7f3a91c2', 'exe-d1f7a00e'];

/** Graph geometry in screen space: every arrow tip on its target's boundary, no edge through another node. */
const geometry = (page) => page.evaluate(() => {
  const rects = {};
  for (const n of document.querySelectorAll('.svelte-flow__node')) if (!n.classList.contains('svelte-flow__node-stage')) rects[n.dataset.id] = n.getBoundingClientRect();
  const inside = (r, [x, y], m) => x > r.left + m && x < r.right - m && y > r.top + m && y < r.bottom - m;
  const res = { edges: 0, stages: document.querySelectorAll('.svelte-flow__node-stage').length, misses: [], crossings: [], bus: 0, badBus: [] };
  for (const g of document.querySelectorAll('g.dep-edge')) {
    res.edges += 1;
    const [s, t] = g.dataset.edge.split('->');
    const path = g.querySelector('path.dep');
    const m = path.getScreenCTM();
    const pt = (x, y) => { const p = new DOMPoint(x, y).matrixTransform(m); return [p.x, p.y]; };
    const tip = pt(...g.querySelector('path.arrow').getAttribute('d').slice(1).split(' L')[0].split(',').map(Number));
    const r = rects[t];
    const edgeDist = Math.min(Math.abs(tip[0] - r.left), Math.abs(tip[0] - r.right), Math.abs(tip[1] - r.top), Math.abs(tip[1] - r.bottom));
    if (!inside(r, tip, -1.5) || edgeDist > 1.5) res.misses.push(g.dataset.edge);
    const len = path.getTotalLength();
    for (let l = 0; l <= len; l += 3) {
      const q = path.getPointAtLength(l);
      const hit = Object.entries(rects).find(([id, rr]) => id !== s && id !== t && inside(rr, pt(q.x, q.y), 2));
      if (hit) { res.crossings.push(`${g.dataset.edge} through ${hit[0]}`); break; }
    }
    if (g.dataset.bus) { res.bus += 1; if (g.classList.contains('bad')) res.badBus.push(g.dataset.edge); }
  }
  return res;
});
const transforms = (page) => page.$$eval('.svelte-flow__node', (ns) => ns.map((n) => `${n.dataset.id}:${n.style.transform}`).join('|'));
// Hash changes and clicks render on the next frame; let them settle before reading.
const text = async (page, sel) => { await page.waitForTimeout(60); return page.locator(sel).first().innerText(); };
const num = (s) => Number(String(s).replace(/[^\d]/g, ''));

try {
  const desk = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  await desk.grantPermissions(['clipboard-read', 'clipboard-write'], { origin });
  const page = await desk.newPage();
  watch(page);
  for (const mode of ['dark', 'light']) for (const [name, hash] of PAGES) { await open(page, hash, mode); await shot(page, `${name}-${mode}`); }
  const phone = await browser.newContext({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });
  const mobile = await phone.newPage();
  watch(mobile);
  for (const [name, hash] of PAGES.filter(([n]) => ['verify', 'exec-session', 'exec-approval', 'pull-482', 'repo-payments', 'settings'].includes(n))) {
    await open(mobile, hash, 'dark');
    await shot(mobile, `${name}-mobile`);
    if (!only && name === 'verify') {
      await check('Repositories stays a visible primary destination on mobile', async () => {
        assert.ok(await mobile.locator('aside.nav a[data-nav=repositories]').isVisible());
      });
    }
    if (!only) {
      await check(`no sideways page scroll on mobile ${name}`, async () => {
        const over = await mobile.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
        assert.ok(over <= 1, `${name} overflows by ${over}px`);
      });
    }
  }
  if (only) { console.log(`errors: ${JSON.stringify(errors)}\nescaped: ${JSON.stringify(escaped)}\nscreenshots: ${shots}`); process.exit(0); }

  // A fresh page per scenario: sample state lives in the page, so each starts from the same fixtures.
  const fresh = async (hash, mode) => { const p = await desk.newPage(); watch(p); await open(p, hash, mode); return p; };

  await check('dark and light themes differ and the toggle switches them', async () => {
    const p = await fresh('#/verify');
    const bg = () => p.evaluate(() => getComputedStyle(document.body).backgroundColor);
    const dark = await bg();
    await p.click('[data-testid=theme]');
    assert.notEqual(await bg(), dark);
    await p.close();
  });

  await check('every route renders inside the one shell; unknown routes say so', async () => {
    const p = await fresh('#/verify');
    for (const [name, hash] of PAGES) {
      await p.evaluate((h) => { location.hash = h; }, hash);
      await p.waitForTimeout(60);
      assert.equal(await p.locator('[data-testid=missing]').count(), 0, `${name} rendered as missing`);
      assert.equal(await p.locator('aside.nav').count(), 1);
    }
    // Repositories are not under Cards, and there is no Verifier index: the old paths are not aliases.
    for (const h of ['#/nope', '#/cards/repositories', '#/cards/repositories/payments-api', '#/cards/verifiers']) {
      await p.evaluate((x) => { location.hash = x; }, h);
      await p.locator('[data-testid=missing]').waitFor();
    }
    const options = await p.$$eval('[data-testid=examples] option', (os) => os.map((o) => o.value).filter(Boolean));
    assert.ok(options.length >= 13, `only ${options.length} examples`);
    for (const v of options) {
      await p.selectOption('[data-testid=examples]', v);
      await p.waitForTimeout(60);
      assert.ok((await p.evaluate(() => location.hash)).startsWith(v.split('?')[0]), `example ${v}`);
      assert.equal(await p.locator('[data-testid=missing]').count(), 0, `example ${v} is missing`);
    }
    await p.close();
  });

  await check('Repositories is its own visible navigation destination, reached from Verify without Cards', async () => {
    const p = await fresh('#/verify');
    const nav = p.locator('aside.nav');
    const repos = nav.getByRole('link', { name: 'Repositories', exact: true });
    assert.ok(await repos.isVisible());
    // Cards is the shell placeholder: not a link, and nothing nested under it.
    assert.equal(await nav.getByRole('link', { name: 'Cards' }).count(), 0);
    assert.equal(await nav.locator('span.nav-item[aria-disabled=true]', { hasText: 'Cards' }).count(), 1);
    assert.equal(await nav.locator('.nav-links > a.nav-item[data-nav=repositories]').count(), 1);
    await repos.click();
    await p.locator('[data-testid=repo-table]').waitFor();
    assert.equal(await p.evaluate(() => location.hash), '#/repositories');
    assert.equal(await repos.getAttribute('aria-current'), 'page');
    const crumbs = await text(p, '[data-testid=crumbs]');
    assert.match(crumbs, /ui-live\s*\/\s*Repositories$/);
    assert.doesNotMatch(crumbs, /Cards/);

    await p.click('[data-repo="acme/payments-api"] a.mono');
    await p.locator('[data-testid=card-name]').waitFor();
    assert.equal(await p.evaluate(() => location.hash), '#/repositories/payments-api');
    assert.match(await text(p, '[data-testid=crumbs]'), /^ui-live\s*\/\s*Repositories\s*\/\s*payments-api$/);
    assert.match(await text(p, '[data-testid=sel-marker]'), /selected/);
    assert.match(await text(p, '[data-testid=facts]'), /GitHub access[\s\S]*Granted[\s\S]*Automatic verification[\s\S]*Ready/i);
    assert.equal(await p.locator('#b-h').textContent(), 'verified_by · Repository/payments-api@1.4.0');
    assert.ok((await p.locator('[aria-labelledby=b-h] [href^="#/cards/verifiers/"]').count()) > 0, 'bindings link their exact Verifier');
    assert.ok(await p.locator('[data-testid=decl]').isVisible());
    assert.ok((await p.locator('[data-testid=activity] tbody tr').count()) > 0);
    assert.ok(await p.locator('[data-testid=view-all]').isVisible());

    await p.click('[data-testid=facts] >> text=Manage connection');
    await p.locator('[data-testid=conn-state]').waitFor();
    assert.equal(await p.evaluate(() => location.hash), '#/settings/github');
    assert.equal(await nav.locator('a[data-nav=settings]').getAttribute('aria-current'), 'page');

    await repos.click();
    await p.locator('[data-testid=repo-table]').waitFor();
    await p.click('[data-repo="acme/web-checkout"] a.mono');
    await p.locator('[data-testid=decl]').waitFor();
    assert.equal(await p.evaluate(() => location.hash), '#/repositories/github/741660215');
    assert.match(await text(p, 'main.view'), /wyrd declare wyrd\.toml/);
    assert.equal(await repos.getAttribute('aria-current'), 'page');
    assert.doesNotMatch(await text(p, '[data-testid=crumbs]'), /Cards/);
    await p.close();
  });

  await check('Verify charts, count and list agree, before and after filtering and range changes', async () => {
    const p = await fresh('#/verify');
    const agree = async () => {
      const count = num(await text(p, '[data-testid=count]'));
      const foot = await text(p, '[data-testid=foot]');
      assert.equal(num(await p.getAttribute('[data-testid=chart-verdict]', 'data-total')), count);
      assert.equal(num(await p.getAttribute('[data-testid=chart-verifier]', 'data-total')), count);
      assert.equal(num(foot.match(/of ([\d,]+)/)[1]), count);
      const rows = await p.locator('a.vt').count();
      assert.equal(rows, Math.min(50, count));
      return count;
    };
    const all = await agree();
    await p.click('[data-filter="verdict=failed"]');
    const failed = await agree();
    assert.ok(failed > 0 && failed < all);
    assert.match(await text(p, '[data-testid=where]'), /verdict = 'failed'/);
    for (const v of await p.$$eval('a.vt .vd', (els) => els.map((e) => e.textContent))) assert.match(v, /failed/);
    await p.selectOption('[data-testid=range]', '7d');
    assert.ok((await agree()) >= failed);
    await p.click('[data-testid=clear]');
    await p.click('[data-testid=show-more]');
    assert.equal(await p.locator('a.vt').count(), 100);
    await p.close();
  });

  await check('Verify list keeps Service and clips no heading or name at 1440px; extra width scrolls inside the table', async () => {
    const p = await fresh('#/verify');
    assert.equal(await text(p, 'a.vt[data-row=exe-7c1e02a4] [data-service]'), 'policy-desk');
    assert.equal(await text(p, 'a.vt[data-row=exe-4d1e9a07] [data-service]'), '—');
    const cells = '.vt-head > span:not(.out), .vt-head .out > span, a.vt > span:not(.out), a.vt .out > span';
    const clipped = await p.$$eval(cells, (els) => els.filter((e) => e.scrollWidth > e.clientWidth + 1).map((e) => e.textContent.trim()));
    assert.deepEqual(clipped, [], 'clipped cells');
    // The pinned outcome group must not cover its own Status cell.
    const covered = await p.$$eval('a.vt .out > .st', (els) => els.filter((e) => { const r = e.getBoundingClientRect(); if (r.top < 0 || r.bottom > innerHeight) return false; const top = document.elementFromPoint(r.left + 2, r.top + r.height / 2); return !e.contains(top) && top !== e; }).length);
    assert.equal(covered, 0, 'status cells hidden under another cell');
    const [over, scroller] = await p.evaluate(() => [document.documentElement.scrollWidth - document.documentElement.clientWidth, (() => { const s = document.querySelector('.vt-scroll'); return s.scrollWidth - s.clientWidth; })()]);
    assert.ok(over <= 1, `page overflows by ${over}px`);
    if (scroller > 0) assert.match(await text(p, '[data-testid=foot]'), /scroll the table sideways/);
    await p.close();
  });

  await check('a Verify row opens the execution page and Back restores the exact Verify state', async () => {
    const p = await fresh('#/verify?verdict=failed');
    await p.click('a.vt[data-row=exe-7c1e02a4]');
    await p.locator('[data-testid=exec]').waitFor();
    assert.match(await p.evaluate(() => location.hash), /^#\/x\/exe-7c1e02a4\?.*back=%23%2Fverify%3Fverdict%3Dfailed$/);
    assert.equal(await p.locator('.drawer, [role=dialog][open]').count(), 0, 'a row must open a page, not a drawer');
    assert.equal(await p.getAttribute('[data-testid=verifier-link]', 'href'), '#/cards/verifiers/policy-review@1.0.0');
    await p.click('[data-testid=back]');
    await p.locator('[data-testid=dashboard]').waitFor();
    assert.equal(await p.evaluate(() => location.hash), '#/verify?verdict=failed');
    assert.equal(await p.getAttribute('[data-filter="verdict=failed"]', 'aria-pressed'), 'true');
    assert.equal(await p.getAttribute('a.vt[data-row=exe-7c1e02a4]', 'aria-current'), 'true');
    await p.close();
  });

  for (const id of GRAPHS) {
    await check(`graph invariants for ${id}: real Svelte Flow, stage headings, arrows on boundaries, no edge through a node`, async () => {
      const p = await fresh(`#/x/${id}`);
      await p.locator('.svelte-flow .svelte-flow__node').first().waitFor();
      const g = await geometry(p);
      assert.ok(g.edges > 0, 'no edges');
      assert.ok(g.stages >= 2, `only ${g.stages} stage headings`);
      assert.deepEqual(g.misses, [], 'arrow tips off their target boundary');
      assert.deepEqual(g.crossings, [], 'edges through nodes');
      if (id === 'exe-6c19e5f0') {
        assert.equal(g.bus, 8, 'every member edge rides the session bus');
        assert.deepEqual(g.badBus, [], 'a member edge is red');
      }
      if (id === 'exe-7c1e02a4') assert.ok(await p.locator('g.dep-edge.bad[data-edge="answer-facts->refund-within-limit"]').count());
      await p.close();
    });
  }

  await check('selection, live updates, zoom and pan never relayout the graph; fit and keyboard work', async () => {
    const p = await fresh('#/x/exe-7f3a91c2');
    await p.locator('.svelte-flow__node').first().waitFor();
    const before = await transforms(p);
    await p.click('[data-node=extract]');
    await p.click('[data-testid=simulate]');
    await p.click('[data-testid=simulate]');
    await p.waitForTimeout(100);
    assert.equal(await transforms(p), before);
    assert.match(await text(p, '[data-node=summarize]'), /running/);
    const zoom = () => text(p, '[data-testid=zoom]');
    const fit = await zoom();
    await p.click('[data-testid=graph] button[aria-label="Zoom in"]');
    assert.notEqual(await zoom(), fit);
    const vp = () => p.$eval('.svelte-flow__viewport', (v) => v.style.transform);
    const moved = await vp();
    const box = await p.locator('.svelte-flow__pane').boundingBox();
    await p.mouse.move(box.x + 30, box.y + box.height - 10);
    await p.mouse.down();
    await p.mouse.move(box.x + 130, box.y + box.height - 30, { steps: 5 });
    await p.mouse.up();
    assert.notEqual(await vp(), moved, 'pan did not move the viewport');
    await p.click('[data-testid=fit]');
    assert.equal(await zoom(), fit);
    assert.equal(await transforms(p), before);
    await p.focus('[data-node=extract]');
    await p.keyboard.press('ArrowRight');
    assert.notEqual(await p.evaluate(() => document.activeElement.dataset.node), 'extract');
    assert.match(await p.evaluate(() => location.hash), /sel=step(%3A|:)(security|correctness)-review/);
    await p.close();
  });

  await check('drift windows step with Previous and Next, keeping the selection', async () => {
    const p = await fresh('#/x/exe-d1f7a00e?sel=req:latency-stable');
    await p.locator('[data-testid=drift]').waitFor();
    await p.click('[data-testid=prev-window]');
    await p.locator('[data-testid=drift]').waitFor();
    const h = await p.evaluate(() => location.hash);
    assert.ok(!h.startsWith('#/x/exe-d1f7a00e') && /sel=req/.test(h), h);
    await p.close();
  });

  await check('outputs never render stored markup; Source shows the exact text', async () => {
    const p = await fresh('#/x/exe-7f3a91c2?sel=step:validate&tab=outputs&out=notes.md');
    await p.locator('[data-testid=preview] .md').waitFor();
    assert.equal(await p.locator('[data-testid=preview] .md img, [data-testid=preview] .md script').count(), 0);
    assert.match(await text(p, '[data-testid=preview] .md'), /<img src=x onerror="alert\(1\)">/);
    await p.click('[data-testid=source]');
    assert.match(await text(p, '[data-testid=source-text]'), /<script>alert\(2\)<\/script>/);
    await p.close();
  });

  await check('approval: admin approves the exact occurrence; a viewer cannot', async () => {
    const p = await fresh('#/x/exe-4d1e9a07?sel=step:release-approval');
    await p.click('[data-testid=approve]');
    await p.locator('[data-testid=approval-state]', { hasText: 'approved' }).waitFor();
    await p.close();
    const v = await fresh('#/x/exe-4d1e9a07?sel=step:release-approval');
    await v.click('[data-testid=role-viewer]');
    await v.locator('[data-testid=approval] [data-testid=viewer-note]').waitFor();
    assert.equal(await v.locator('[data-testid=approve], [data-testid=cancel]').count(), 0);
    await v.close();
  });

  await check('cancel: cancels unfinished work, and reports when the execution finished first', async () => {
    const p = await fresh('#/x/exe-7f3a91c2');
    await p.click('[data-testid=cancel]');
    await p.locator('[data-testid=status]', { hasText: 'cancelled' }).waitFor();
    await p.close();
    const r = await fresh('#/x/exe-7f3a91c2');
    await r.selectOption('[data-testid=race]', 'finished');
    await r.click('[data-testid=cancel]');
    await r.locator('[data-testid=notice]', { hasText: 'completed before' }).waitFor();
    assert.match(await text(r, '[data-testid=status]'), /completed/);
    await r.close();
  });

  await check('review history: the plain PR link follows the latest set, a pinned link stays, a new set takes over', async () => {
    const p = await fresh('#/repositories/payments-api/pulls/482');
    const current = async () => { await p.waitForTimeout(60); return p.getAttribute('[data-set][aria-current]', 'data-set'); };
    assert.equal(await current(), 'rs-482-3');
    assert.ok(await p.locator('[data-member=exe-7f3a91c2]').count());
    assert.equal(await p.locator('[data-testid=pinned]').count(), 0);
    await p.click('[data-set=rs-482-1]');
    assert.equal(await current(), 'rs-482-1');
    assert.match(await p.evaluate(() => location.hash), /\?set=rs-482-1$/);
    assert.match(await text(p, '[data-testid=pinned]'), /earlier review set rs-482-1/);
    await p.click('[data-testid=to-latest]');
    for (let k = 0; k < 4; k++) await p.click('[data-testid=simulate]');
    assert.equal(await current(), 'rs-482-4');
    await p.evaluate(() => { location.hash = '#/repositories/payments-api/pulls/482?set=rs-482-3'; });
    await p.waitForTimeout(60);
    assert.equal(await current(), 'rs-482-3');
    await p.evaluate(() => { location.hash = '#/x/exe-7f3a91c2'; });
    await p.locator('[data-testid=superseded]').waitFor();
    assert.equal(await text(p, '[data-testid=set-link]'), 'rs-482-3');
    await p.close();
  });

  await check('Repository detail: versions, declaration tabs and latest-or-all review sets', async () => {
    const p = await fresh('#/repositories/payments-api');
    assert.match(await text(p, '[data-testid=identity]'), /Repository\/payments-api@1\.4\.0/);
    assert.equal(await p.locator('[data-testid=activity] [data-set=rs-482-1]').count(), 0);
    await p.click('[data-reviews=all]');
    await p.locator('[data-testid=activity] [data-set=rs-482-1]').first().waitFor();
    await p.click('[data-tab=toml]');
    assert.match(await text(p, '[data-testid=decl]'), /verifier = \[/);
    await p.evaluate(() => { location.hash = '#/repositories/ledger?v=1.0.0'; });
    await p.locator('[data-testid=old-version]').waitFor();
    assert.match(await text(p, '[data-testid=sel-marker]'), /not selected/);
    await p.evaluate(() => { location.hash = '#/repositories/github/741660215'; });
    assert.match(await text(p, '[data-testid=decl]'), /\[repository\.verified_by\]/);
    await p.close();
  });

  await check('Repositories list filters and reflects the connection', async () => {
    const p = await fresh('#/repositories');
    assert.match(await text(p, '[data-testid=repo-count]'), /4 of 4/);
    await p.fill('input.search', 'ledger');
    assert.match(await text(p, '[data-testid=repo-count]'), /1 of 4/);
    await p.click('[data-repo="acme/ledger"] a.mono');
    await p.locator('[data-testid=card-name]').waitFor();
    await p.click('[data-testid=crumbs] >> text=Repositories');
    await p.locator('[data-testid=repo-table]').waitFor();
    assert.equal(await p.evaluate(() => location.hash), '#/repositories?q=ledger');
    assert.equal(await p.inputValue('input.search'), 'ledger');
    await p.click('[data-conn=disconnected]');
    await p.locator('[data-testid=conn-banner]').waitFor();
    await p.close();
  });

  await check('Settings: disconnect confirms its consequences, connect hands off, viewers and lost access', async () => {
    const p = await fresh('#/settings/github');
    await p.click('[data-testid=disconnect]');
    assert.match(await text(p, '[data-testid=disconnect-list]'), /unfinished execution/);
    await p.click('[data-testid=disconnect-confirm]');
    assert.match(await text(p, '[data-testid=conn-state]'), /Not connected/);
    await p.evaluate(() => { location.hash = '#/x/exe-7f3a91c2'; });
    await p.locator('[data-testid=status]', { hasText: 'cancelled' }).waitFor();
    await p.evaluate(() => { location.hash = '#/settings/github'; });
    await p.click('[data-testid=connect]');
    await p.locator('[data-testid=handoff]').waitFor();
    await p.locator('[data-testid=conn-state]', { hasText: 'Connected' }).waitFor();
    await p.click('[data-conn=lost]');
    assert.match(await text(p, '[data-testid=conn-state]'), /Access lost/);
    await p.click('[data-testid=role-viewer]');
    await p.locator('[data-testid=viewer-note]').waitFor();
    assert.equal(await p.locator('[data-testid=disconnect]').count(), 0);
    await p.close();
  });

  await check('Verifier pages: exact version, YAML tab and Results in Verify', async () => {
    const p = await fresh('#/cards/verifiers/policy-review@1.0.0');
    await p.locator('[data-testid=graph] .svelte-flow__node').first().waitFor();
    assert.match(await p.getAttribute('[data-testid=results-in-verify]', 'href'), /verifier=policy-review%401\.0\.0/);
    await p.click('a[href$="?tab=yaml"]');
    assert.match(await text(p, '[data-testid=yaml]'), /apiVersion: wyrd\/v1/);
    await p.close();
  });
} finally {
  await browser.close();
  server.close();
}

assert.deepEqual(errors, [], `page errors: ${errors.join(' | ')}`);
assert.deepEqual(escaped, [], `requests outside design/mock/: ${escaped.join(' | ')}`);
console.log(`ok · ${passed.length} checks\n${passed.map((p) => `  ✓ ${p}`).join('\n')}\nscreenshots: ${shots}`);
