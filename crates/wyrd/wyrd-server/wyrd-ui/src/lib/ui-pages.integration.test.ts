/**
 * Real-browser Observe journeys. The production BFF build (`node build`) runs
 * in front of a real Wyrd server started by `@wyrd/testing` over repository
 * Postgres; Chromium signs in through the API-key page and investigates
 * evidence exported over stock OTLP. Nothing here mocks the server or the BFF.
 */
import { execFileSync, spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { chromium, type Browser, type Page } from 'playwright';
import { expect, vi } from 'vitest';
import { serverTest } from '../../../../../../sdks/wyrd-sdk-ts/wyrd/tests/support/server';
import { seedSignals } from '$lib/ui-signals.integration';

vi.setConfig({ testTimeout: 300_000 });

/** The window the seeded evidence falls in. */
const RANGE = { from: '2026-10-19T00:00:00.000Z', to: '2026-10-19T12:00:00.000Z' };
const TRACE = '4bf92f3577b34da6a3ce929d0e0e4736';
const FOREIGN = 'a3ce929d0e0e47364bf92f3577b34da6';
const TENANT = 'ui-browser';

/** A free loopback port, so the BFF's ORIGIN is known before it starts. */
async function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const probe = createServer().listen(0, '127.0.0.1', () => {
      const address = probe.address();
      probe.close(() => (typeof address === 'object' && address ? resolve(address.port) : reject(new Error('no port'))));
    });
  });
}

/** The production BFF build in front of `serverUrl`; resolves once it listens. */
async function startBff(serverUrl: string): Promise<{ url: string; stop: () => void }> {
  const port = await freePort();
  const url = `http://127.0.0.1:${port}`;
  const child = spawn('node', ['build'], {
    env: { ...process.env, HOST: '127.0.0.1', PORT: String(port), ORIGIN: url, WYRD_SERVER_URL: serverUrl },
    stdio: ['ignore', 'pipe', 'inherit']
  });
  await new Promise<void>((resolve, reject) => {
    child.stdout.on('data', (chunk: Buffer) => /Listening on/.test(chunk.toString()) && resolve());
    child.once('exit', (code) => reject(new Error(`the BFF exited with ${code} before listening`)));
  });
  return { url, stop: () => child.kill() };
}

/** One browser signed in to `tenant` with `apiKey` through the API-key page. */
async function signIn(browser: Browser, base: string, tenant: string, apiKey: string): Promise<Page> {
  const page = await (await browser.newContext()).newPage();
  await page.goto(`${base}/t/${tenant}/login/api-key`);
  await page.getByLabel('API key').fill(apiKey);
  await page.getByRole('button', { name: 'Sign in with API key' }).click();
  await page.waitForURL(`${base}/t/${tenant}`);
  return page;
}

/** The built BFF, a browser, and a tenant holding one trace; the default tenant holds another. */
const test = serverTest().extend<{ ui: { base: string; browser: Browser; adminKey: string; readerKey: string } }>({
  ui: [
    async ({ server }, use) => {
      execFileSync('pnpm', ['exec', 'vite', 'build'], { stdio: 'inherit', env: { ...process.env, VITEST: '' } });
      const tenant = server.seedTenant(TENANT);
      const adminKey = server.bootstrapServiceInTenant(tenant, ['admin'], 'ui-browser-admin');
      const readerKey = server.bootstrapServiceInTenant(tenant, [], 'ui-browser-unprivileged');
      await seedSignals(server, adminKey, { trace: TRACE, service: 'ui-agent' });
      await seedSignals(server, server.apiKey, { trace: FOREIGN, service: 'foreign-agent' });
      const bff = await startBff(server.baseUrl);
      const browser = await chromium.launch();
      try {
        await use({ base: bff.url, browser, adminKey, readerKey });
      } finally {
        await browser.close();
        bff.stop();
      }
    },
    { scope: 'file' }
  ]
});

test('authenticated traces and spans', async ({ ui }) => {
  const page = await signIn(ui.browser, ui.base, TENANT, ui.adminKey);
  const observe = `${ui.base}/t/${TENANT}/observe`;

  // The shell links Observe; Traces is the default signal.
  await page.getByRole('navigation', { name: 'Primary' }).getByRole('link', { name: 'Observe' }).click();
  await page.waitForURL(observe);
  await expect.poll(() => page.getByRole('link', { name: 'Traces' }).getAttribute('aria-current')).toBe('page');

  // Trace roots of this tenant only, with the overview and facets under the same range.
  await page.goto(`${observe}?from=${RANGE.from}&to=${RANGE.to}`);
  const list = page.locator('[data-panel="list"]');
  await expect.poll(() => list.textContent(), { timeout: 30_000 }).toContain('invoke_agent support');
  expect(await list.textContent()).not.toContain('foreign-agent');
  await expect.poll(() => page.locator('[data-panel="overview"]').textContent()).toMatch(/Traces\s*1/);
  expect(await list.locator('tbody tr').count()).toBe(1);

  // All spans lists every span of the trace.
  await page.getByRole('link', { name: 'All spans' }).click();
  await expect.poll(() => list.locator('tbody tr').count()).toBe(3);
  expect(new URL(page.url()).searchParams.get('mode')).toBe('spans');

  // A facet value becomes a shareable WHERE clause.
  await page.locator('[data-panel="facets"]').getByRole('button', { name: /chat ranker/ }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('where')).toBe(JSON.stringify([{ column: 'name', value: 'chat ranker' }]));
  await expect.poll(() => list.locator('tbody tr').count()).toBe(1);
  await page.getByRole('link', { name: 'Remove name' }).click();
  await expect.poll(() => list.locator('tbody tr').count()).toBe(3);

  // An invalid draft is refused with its stable code and never replaces the applied view.
  const before = page.url();
  await page.getByLabel('WHERE').fill('no_such_column = 1');
  await page.getByRole('button', { name: 'Apply' }).last().click();
  await expect.poll(() => page.getByRole('alert').textContent()).toContain('WYRD_VALA_400_QUERY_INVALID_SQL');
  expect(page.url()).toBe(before);
  expect(await list.locator('tbody tr').count()).toBe(3);
  // A valid draft is applied in its server-normalized form.
  await page.getByLabel('WHERE').fill('status_code=2');
  await page.getByRole('button', { name: 'Apply' }).last().click();
  await expect.poll(() => new URL(page.url()).searchParams.get('where_sql')).toBe('status_code = 2');
  await expect.poll(() => list.locator('tbody tr').count()).toBe(1);
  expect(await list.textContent()).toContain('execute_tool search');

  // Opening the span shows the exact trace and its detail; the state survives reload.
  await list.locator('tbody tr').first().click();
  const drawer = page.getByRole('dialog', { name: 'Trace' });
  await expect.poll(() => drawer.getByRole('tree').getByRole('treeitem').count(), { timeout: 30_000 }).toBe(3);
  expect(new URL(page.url()).searchParams.get('trace')).toBe(TRACE);
  await page.reload();
  await expect.poll(() => page.getByRole('dialog', { name: 'Trace' }).getByRole('tab').count()).toBeGreaterThan(0);
  await page.getByRole('button', { name: 'Close Trace' }).click();
  await expect.poll(() => new URL(page.url()).searchParams.has('trace')).toBe(false);
  expect(new URL(page.url()).searchParams.get('where_sql')).toBe('status_code = 2');

  // Open in Query shows the exact SQL and binds; a rerun is authorized and bounded; Back restores Observe.
  const observed = page.url();
  await list.getByRole('button', { name: 'Open in Query' }).click();
  await page.waitForURL(/\/query\?/);
  expect(await page.locator('pre').textContent()).toContain('(status_code = 2)');
  expect(await page.locator('.binds').textContent()).toContain(JSON.stringify(RANGE.from));
  await page.getByRole('button', { name: 'Run again' }).click();
  await expect.poll(() => page.getByRole('status').textContent()).toMatch(/success · 1 rows/);
  await page.getByRole('link', { name: '← Back to Observe' }).click();
  await page.waitForURL(observed);

  // Live holds what is on screen while it is read.
  await page.getByLabel('Live').check();
  await expect.poll(() => new URL(page.url()).searchParams.get('live')).toBe('1');
  await expect.poll(() => list.getByRole('status').textContent()).toMatch(/Live/);

  // An empty range is truthful, not blank.
  await page.goto(`${observe}?from=2026-10-18T00:00:00.000Z&to=2026-10-18T12:00:00.000Z`);
  await expect.poll(() => list.textContent()).toContain('No root spans in bifrost.traces.spans');

  // A principal holding no role sees each panel denied with its stable code.
  const viewer = await signIn(ui.browser, ui.base, TENANT, ui.readerKey);
  await viewer.goto(`${observe}?from=${RANGE.from}&to=${RANGE.to}`);
  await expect.poll(() => viewer.locator('[data-panel="list"]').textContent()).toContain('WYRD_PERMISSION_403_DENIED_RBAC');
});
