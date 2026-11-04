/**
 * Real-browser Observe journeys. The production BFF build (`node build`) runs
 * in front of a real Wyrd server started by `@wyrd/testing` over repository
 * Postgres; Chromium signs in through the API-key page and investigates
 * evidence exported over stock OTLP. Nothing here mocks the server or the BFF.
 */
import { execFileSync, spawn } from 'node:child_process';
import { closeSync, mkdtempSync, openSync, readFileSync, statSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { chromium, type Browser, type Page } from 'playwright';
import { test as base, expect, vi } from 'vitest';
import { serverTest } from '../../../../../../sdks/wyrd-sdk-ts/wyrd/tests/support/server';
import { Principals, WyrdClient } from 'wyrd-ai';
import { exportOtlp, nanos, seedSignals } from '$lib/ui-signals.integration';

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

/** An invitation code for the built-in `role` of `tenant`, issued through the server by its administrator `adminKey`. */
async function invitation(serverUrl: string, adminKey: string, role: string): Promise<string> {
  const issued = await fetch(`${serverUrl}/v1/principals/users/invitations`, {
    method: 'POST',
    headers: {
      'x-wyrd-access-token': `Bearer ${await WyrdClient.connect({ serverUrl, credential: adminKey }).accessToken()}`,
      'content-type': 'application/json'
    },
    body: JSON.stringify({ roles: [role] })
  });
  expect(issued.status).toBe(201);
  return ((await issued.json()) as { invitation_code: string }).invitation_code;
}

/** A new browser signed in to `tenantKey` the normal human way: invitation signup, then the username and password form. */
async function humanSignIn(browser: Browser, base: string, tenantKey: string, code: string): Promise<Page> {
  const username = `person-${crypto.randomUUID().slice(0, 8)}`;
  const password = `correct horse ${crypto.randomUUID()}`;
  const enrolled = await browser.newContext();
  const signup = await enrolled.newPage();
  await signup.goto(`${base}/t/${tenantKey}/signup`);
  await signup.getByLabel('Invitation code').fill(code);
  await signup.getByLabel('Username').fill(username);
  await signup.getByLabel('Password').fill(password);
  await signup.getByRole('button', { name: 'Create account' }).click();
  await signup.waitForURL(`${base}/t/${tenantKey}`);
  await enrolled.close();
  const page = await (await browser.newContext()).newPage();
  await page.goto(`${base}/t/${tenantKey}/login`);
  await page.getByLabel('Username').fill(username);
  await page.getByLabel('Password').fill(password);
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await page.waitForURL(`${base}/t/${tenantKey}`);
  return page;
}

/** The built BFF, a browser, and a tenant holding one trace; the default tenant holds another. */
const test = serverTest().extend<{ ui: { base: string; browser: Browser; tenant: string; adminKey: string; readerKey: string; serverUrl: string } }>({
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
        await use({ base: bff.url, browser, tenant, adminKey, readerKey, serverUrl: server.baseUrl });
      } finally {
        await browser.close();
        bff.stop();
      }
    },
    { scope: 'file' }
  ]
});

/** The server-rendered HTML of `url` in `page`'s session, before any script runs. */
async function serverHtml(page: Page, url: string): Promise<string> {
  const response = await page.request.get(url);
  expect(response.status()).toBe(200);
  return response.text();
}

test('authenticated traces and spans', async ({ ui }) => {
  const page = await signIn(ui.browser, ui.base, TENANT, ui.adminKey);
  const observe = `${ui.base}/t/${TENANT}/observe`;
  const signals = page.getByRole('navigation', { name: 'Signal' });

  // The rail links Observe; Traces is the default signal; undelivered destinations stay visible and disabled.
  const rail = page.getByRole('navigation', { name: 'Destinations' });
  await rail.getByRole('link', { name: 'Observe' }).click();
  await page.waitForURL(observe);
  await expect.poll(() => signals.getByRole('link', { name: 'Traces' }).getAttribute('aria-current')).toBe('page');
  expect(await rail.locator('[aria-disabled="true"]', { hasText: 'Verify' }).count()).toBe(1);
  expect(await page.getByRole('button', { name: /Find or ask/ }).isDisabled()).toBe(true);

  // The first HTML response already carries this tenant's evidence: no script runs the initial reads.
  const ranged = `${observe}?from=${RANGE.from}&to=${RANGE.to}`;
  const html = await serverHtml(page, ranged);
  expect(html).toContain('invoke_agent support');
  expect(html).toContain('Drag across any chart to zoom all three');
  expect(html).not.toContain('foreign-agent');

  // Trace roots of this tenant only, with the three overview charts and facets under the same range.
  await page.goto(ranged);
  const list = page.locator('[data-panel="list"]');
  const overview = page.locator('[data-panel="overview"]');
  await expect.poll(() => list.textContent(), { timeout: 30_000 }).toContain('invoke_agent support');
  expect(await list.textContent()).not.toContain('foreign-agent');
  expect(await overview.getByRole('region').count()).toBe(3);
  await expect.poll(() => overview.textContent()).toMatch(/1 trace(?!s)/);
  expect(await list.locator('tbody tr').count()).toBe(1);
  // Wrap, Columns and CSV sit in the list bar, once.
  const listbar = page.locator('.listbar');
  await expect.poll(() => listbar.locator('summary', { hasText: 'Columns' }).count()).toBe(1);
  expect(await listbar.getByRole('link', { name: 'CSV' }).count()).toBe(1);
  expect(await listbar.getByRole('button', { name: 'Wrap lines' }).count()).toBe(1);
  expect(await list.locator('summary', { hasText: 'Columns' }).count()).toBe(0);

  // Spans lists every span of the trace.
  await page.getByRole('group', { name: 'Rows' }).getByRole('link', { name: 'Spans' }).click();
  await expect.poll(() => list.locator('tbody tr').count()).toBe(3);
  expect(new URL(page.url()).searchParams.get('mode')).toBe('spans');

  // Filters fold and unfold; a facet value becomes a shareable WHERE clause.
  await page.getByRole('button', { name: /^Filters/ }).click();
  expect(await page.locator('[data-panel="facets"]').count()).toBe(0);
  await page.getByRole('button', { name: /^Filters/ }).click();
  await page.locator('[data-panel="facets"]').getByRole('button', { name: /chat ranker/ }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('where')).toBe(JSON.stringify([{ column: 'name', value: 'chat ranker' }]));
  await expect.poll(() => list.locator('tbody tr').count()).toBe(1);
  await page.getByRole('link', { name: 'Remove name' }).click();
  await expect.poll(() => list.locator('tbody tr').count()).toBe(3);

  // An invalid draft is refused with its stable code and never replaces the applied view.
  const before = page.url();
  await page.getByLabel('WHERE').fill('no_such_column = 1');
  await page.getByLabel('WHERE').press('Enter');
  await expect.poll(() => page.getByRole('alert').textContent()).toContain('WYRD_VALA_400_QUERY_INVALID_SQL');
  expect(page.url()).toBe(before);
  expect(await list.locator('tbody tr').count()).toBe(3);
  // A valid draft is applied in its server-normalized form.
  await page.getByLabel('WHERE').fill('status_code=2');
  await page.getByLabel('WHERE').press('Enter');
  await expect.poll(() => new URL(page.url()).searchParams.get('where_sql')).toBe('status_code = 2');
  await expect.poll(() => list.locator('tbody tr').count()).toBe(1);
  expect(await list.textContent()).toContain('execute_tool search');

  // Opening the span shows the exact trace and its detail, server-rendered too; the state survives reload.
  await list.locator('tbody tr').first().click();
  const drawer = page.getByRole('dialog', { name: 'Trace' });
  await expect.poll(() => drawer.getByRole('tree').getByRole('treeitem').count(), { timeout: 30_000 }).toBe(3);
  expect(new URL(page.url()).searchParams.get('trace')).toBe(TRACE);
  expect(await serverHtml(page, page.url())).toContain('role="treeitem"');
  // The drawer is the wide trace surface: root and totals head it, beside Link, Expand and Close.
  expect(await drawer.locator('header').first().textContent()).toMatch(/trace [0-9a-f]{4}…[0-9a-f]{4}.*3 spans/s);
  expect(await drawer.locator('header').first().getByTitle(TRACE).count()).toBe(1);
  expect((await drawer.boundingBox())?.width).toBe(884);
  await drawer.getByRole('button', { name: /^Expand ↗$/ }).click();
  expect((await drawer.boundingBox())?.width).toBe(page.viewportSize()?.width);
  await drawer.getByRole('button', { name: /^Collapse ↗$/ }).click();
  expect(await drawer.getByRole('button', { name: 'Link' }).count()).toBe(1);
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

  // Zoom halves the applied range around its centre for every panel; Back and reload restore the committed range.
  await page.getByRole('button', { name: 'Zoom in' }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('from')).toBe('2026-10-19T03:00:00.000Z');
  expect(new URL(page.url()).searchParams.get('to')).toBe('2026-10-19T09:00:00.000Z');
  await expect.poll(() => overview.getByRole('button', { name: 'Reset zoom' }).textContent()).toContain('03:00:00 – 09:00:00');
  await page.reload();
  expect(new URL(page.url()).searchParams.get('from')).toBe('2026-10-19T03:00:00.000Z');
  await page.goBack();
  await page.waitForURL(observed);

  // Hovering a chart floats its tooltip without moving the page; a drag across the middle half
  // zooms every panel to that half of the applied range, and the Zoomed badge resets it.
  await page.goto(observed);
  const plot = overview.locator('svg.plot').first();
  const box = (await plot.boundingBox())!;
  const tall = (await overview.boundingBox())!.height;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await expect.poll(() => overview.locator('.tooltip').count()).toBeGreaterThan(0);
  expect((await overview.boundingBox())!.height).toBe(tall);
  await page.mouse.move(box.x + box.width * 0.25, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.75, box.y + box.height / 2, { steps: 5 });
  await page.mouse.up();
  await expect.poll(() => new URL(page.url()).searchParams.get('from')).not.toBe(RANGE.from);
  const near = (name: string, at: string) => Math.abs(Date.parse(new URL(page.url()).searchParams.get(name)!) - Date.parse(at)) < 10 * 60_000;
  expect(near('from', '2026-10-19T03:00:00Z') && near('to', '2026-10-19T09:00:00Z')).toBe(true);
  await overview.getByRole('button', { name: 'Reset zoom' }).click();
  await expect.poll(() => new URL(page.url()).searchParams.has('from')).toBe(false);
  await page.goto(observed);

  // The time picker commits a preset or a validated custom range, in the chosen zone.
  const picker = page.getByRole('button', { name: /^Time range/ });
  await picker.click();
  const dialog = page.getByRole('dialog', { name: 'Time range' });
  await dialog.getByLabel('From').fill('yesterday-ish');
  await dialog.getByRole('button', { name: 'Apply range' }).click();
  await expect.poll(() => dialog.getByRole('alert').textContent()).toContain('From is not');
  expect(page.url()).toBe(observed);
  await dialog.getByLabel('From').fill('2026-10-19 01:00');
  await dialog.getByLabel('To').fill('2026-10-19T11:00:00Z');
  await dialog.getByRole('button', { name: 'Apply range' }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('from')).toBe('2026-10-19T01:00:00.000Z');
  expect(new URL(page.url()).searchParams.get('to')).toBe('2026-10-19T11:00:00.000Z');
  await expect.poll(() => list.locator('tbody tr').count()).toBe(1);
  await picker.click();
  await dialog.getByRole('button', { name: 'Last 6 hours' }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('last')).toBe('6h');
  expect(new URL(page.url()).searchParams.has('from')).toBe(false);
  await picker.click();
  await expect.poll(() => dialog.textContent()).toContain('01:00:00 → 11:00:00');
  await dialog.press('Escape');
  // At 390px the open picker keeps every control inside the viewport.
  const size = page.viewportSize();
  await page.setViewportSize({ width: 390, height: 844 });
  await picker.click();
  const panel = await dialog.boundingBox();
  expect(panel && panel.x >= 0 && panel.x + panel.width <= 390).toBe(true);
  for (const name of ['From', 'To']) {
    const field = await dialog.getByLabel(name).boundingBox();
    expect(field && field.x >= 0 && field.x + field.width <= 390).toBe(true);
  }
  for (const name of ['UTC', 'Local', 'Apply range', 'Last 6 hours']) {
    const control = await dialog.getByRole('button', { name }).first().boundingBox();
    expect(control && control.x >= 0 && control.x + control.width <= 390).toBe(true);
  }
  await dialog.press('Escape');
  if (size) await page.setViewportSize(size);
  await page.goBack();
  await expect.poll(() => list.locator('tbody tr').count()).toBe(1);

  // Live holds what is on screen while it is read.
  await page.getByRole('button', { name: 'Live' }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('live')).toBe('1');
  await expect.poll(() => list.getByRole('status').textContent()).toMatch(/Live/);

  // An empty range is truthful, not blank.
  await page.goto(`${observe}?from=2026-10-18T00:00:00.000Z&to=2026-10-18T12:00:00.000Z`);
  await expect.poll(() => list.textContent()).toContain('No root spans in bifrost.traces.spans');
  // A malformed range is refused with its field, before any read.
  expect(await serverHtml(page, `${observe}?from=2026-10-19T00:00:00Z&to=yesterday`)).toContain('WYRD_SPEC_400_VALIDATION');

  // A principal holding no role sees each panel denied with its stable code, in the first response.
  const viewer = await signIn(ui.browser, ui.base, TENANT, ui.readerKey);
  expect(await serverHtml(viewer, ranged)).toContain('WYRD_PERMISSION_403_DENIED_RBAC');
  await viewer.goto(ranged);
  await expect.poll(() => viewer.locator('[data-panel="list"]').textContent()).toContain('WYRD_PERMISSION_403_DENIED_RBAC');
});

test('authenticated logs investigation', async ({ ui }) => {
  const page = await signIn(ui.browser, ui.base, TENANT, ui.adminKey);
  const logs = `${ui.base}/t/${TENANT}/observe?signal=logs&from=${RANGE.from}&to=${RANGE.to}`;
  expect(await serverHtml(page, logs)).toContain('search timed out');
  await page.goto(logs);
  const list = page.locator('[data-panel="list"]');

  // The recorded record of this tenant only, with its recorded severity, and a severity timeline under the same filters.
  await expect.poll(() => list.textContent(), { timeout: 30_000 }).toContain('search timed out');
  expect(await list.textContent()).toContain('ERROR');
  expect(await list.textContent()).not.toContain('foreign-agent');
  const timeline = page.locator('[data-panel="timeline"]');
  await expect.poll(() => timeline.textContent()).toMatch(/Records per/);
  expect(await timeline.getByLabel('Key').textContent()).toMatch(/info.*warn.*error/);
  expect(await timeline.getByRole('application').getAttribute('aria-label')).toMatch(/: error$/);
  expect(await timeline.getByRole('button', { name: /Open in Query/ }).count()).toBe(0);

  // Severity facets filter the timeline and the records alike.
  await page.locator('[data-panel="facets"]').getByRole('button', { name: /ERROR/ }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('where')).toBe(JSON.stringify([{ column: 'severity_text', value: 'ERROR' }]));
  await expect.poll(() => list.locator('tbody tr').count()).toBe(1);

  // Wrap and paging are reader controls over the rows already read; Columns is kept in the URL.
  await list.getByRole('button', { name: 'Wrap lines' }).click();
  expect(await list.getByRole('button', { name: 'Wrap lines' }).getAttribute('aria-pressed')).toBe('true');
  await list.locator('summary', { hasText: 'Columns' }).click();
  await list.locator('label[title="body_text"] input').uncheck();
  await expect.poll(() => list.locator('th[title="body_text"]').count()).toBe(0);
  expect(JSON.parse(new URL(page.url()).searchParams.get('columns') ?? '[]')).not.toContain('body_text');
  await page.goBack();
  await expect.poll(() => list.locator('th[title="body_text"]').count()).toBe(1);
  expect(await list.getByRole('navigation', { name: 'Pages' }).getByRole('combobox').inputValue()).toBe('50');

  // The record opens by its exact identity beside the list, with its body and recorded fields.
  await list.locator('tbody tr').first().click();
  const aside = page.getByRole('complementary', { name: 'Log record' });
  await expect.poll(() => aside.textContent(), { timeout: 30_000 }).toContain('search timed out');
  expect(JSON.parse(new URL(page.url()).searchParams.get('log') ?? '{}')).toMatchObject({ wyrd_request_id: expect.any(String) });
  expect(await serverHtml(page, page.url())).toContain('aria-label="Log record"');

  // Open trace leaves Logs in history; Back returns to the same record.
  const opened = page.url();
  await aside.getByRole('button', { name: /Open trace/ }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('trace')).toBe(TRACE);
  expect(new URL(page.url()).searchParams.get('signal')).toBeNull();
  await expect.poll(() => page.getByRole('dialog', { name: 'Trace' }).getByRole('treeitem').count(), { timeout: 30_000 }).toBe(3);
  await page.goBack();
  await page.waitForURL(opened);
  await expect.poll(() => page.getByRole('complementary', { name: 'Log record' }).textContent()).toContain('search timed out');

  // A detail field filters or excludes into WHERE; excluding the only service leaves a truthful empty list.
  await page.getByRole('complementary', { name: 'Log record' }).getByRole('button', { name: 'Exclude' }).click();
  await expect.poll(() => JSON.parse(new URL(page.url()).searchParams.get('where') ?? '[]')).toContainEqual({ column: 'service_name', value: 'ui-agent', not: true });
  await expect.poll(() => list.textContent()).toContain('No rows in bifrost.logs.records');
  expect(new URL(page.url()).searchParams.has('log')).toBe(false);

  // A principal holding no role sees the Logs panels denied with their stable code.
  const viewer = await signIn(ui.browser, ui.base, TENANT, ui.readerKey);
  await viewer.goto(logs);
  await expect.poll(() => viewer.locator('[data-panel="timeline"]').textContent()).toContain('WYRD_PERMISSION_403_DENIED_RBAC');
});

/** Integers past 2^53, which a JSON number cannot carry exactly. */
const UNSAFE = ['9007199254740993', '9007199254740995'] as const;

test('authenticated metrics investigation', async ({ server, ui }) => {
  // One metric name recorded with two meanings and unsafe integers; every recorded structure with unsafe nested counts.
  const point = (at: string, value: Record<string, unknown>) => ({ timeUnixNano: nanos(at), ...value });
  const scope = { name: 'wyrd-ui-metrics' };
  const resource = (service: string) => ({ attributes: [{ key: 'service.name', value: { stringValue: service } }] });
  const route = (value: string) => [{ key: 'route', value: { stringValue: value } }];
  await exportOtlp(server, ui.adminKey, '/v1/metrics', {
    resourceMetrics: [
      {
        resource: resource('ui-agent'),
        scopeMetrics: [
          {
            scope,
            metrics: [
              {
                name: 'ui.queue',
                unit: '1',
                sum: {
                  aggregationTemporality: 2,
                  isMonotonic: true,
                  dataPoints: [
                    point('2026-10-19T04:00:00Z', { asInt: UNSAFE[0] }),
                    point('2026-10-19T04:01:00Z', { asInt: UNSAFE[1], exemplars: [{ timeUnixNano: nanos('2026-10-19T04:01:00Z'), asDouble: 7.5, filteredAttributes: route('/exemplar') }] }),
                    // The same instant with another point dimension: a separate series, never summed.
                    point('2026-10-19T04:00:00Z', { asInt: '5', attributes: route('not set') })
                  ]
                }
              },
              {
                name: 'ui.latency',
                unit: 's',
                histogram: {
                  aggregationTemporality: 1,
                  dataPoints: [point('2026-10-19T04:00:00Z', { count: '9007199254740995', sum: 1.5, bucketCounts: [UNSAFE[0], '2'], explicitBounds: [1] })]
                }
              },
              {
                name: 'ui.spread',
                unit: 'ms',
                exponentialHistogram: {
                  aggregationTemporality: 2,
                  dataPoints: [
                    point('2026-10-19T04:00:00Z', { count: '9007199254740996', sum: 4, scale: 1, zeroCount: '0', positive: { offset: 2, bucketCounts: [UNSAFE[0], '1'] }, negative: { offset: -1, bucketCounts: ['2'] } })
                  ]
                }
              },
              // Mixed int64 and double samples that round to one double: Max compares them exactly.
              { name: 'ui.low', unit: '1', gauge: { dataPoints: [point('2026-10-19T04:00:00Z', { asInt: '-9007199254740993' }), point('2026-10-19T04:01:00Z', { asDouble: -9007199254740992 })] } },
              { name: 'ui.high', unit: '1', gauge: { dataPoints: [point('2026-10-19T04:00:00Z', { asInt: UNSAFE[0] }), point('2026-10-19T04:01:00Z', { asDouble: 9007199254740992 })] } },
              {
                name: 'ui.window',
                unit: 'ms',
                summary: { dataPoints: [point('2026-10-19T04:00:00Z', { count: UNSAFE[1], sum: 12.25, quantileValues: [{ quantile: 0.5, value: 3 }, { quantile: 0.99, value: 9 }] })] }
              }
            ]
          }
        ]
      },
      { resource: resource('ui-worker'), scopeMetrics: [{ scope, metrics: [{ name: 'ui.queue', unit: 'By', gauge: { dataPoints: [point('2026-10-19T04:00:00Z', { asDouble: 2.5 }), point('2026-10-19T04:01:00Z', { asInt: '100' })] } }] }] },
      // A service recorded as the text "not set", and a resource with no service at all.
      { resource: resource('not set'), scopeMetrics: [{ scope, metrics: [{ name: 'ui.queue', unit: 'By', gauge: { dataPoints: [point('2026-10-19T04:00:00Z', { asDouble: 1 })] } }] }] },
      { resource: { attributes: [] }, scopeMetrics: [{ scope, metrics: [{ name: 'ui.queue', unit: 'By', gauge: { dataPoints: [point('2026-10-19T04:00:00Z', { asDouble: 2 })] } }] }] }
    ]
  });
  server.flushBifrost();

  const page = await signIn(ui.browser, ui.base, TENANT, ui.adminKey);
  const metrics = `${ui.base}/t/${TENANT}/observe?signal=metrics&from=${RANGE.from}&to=${RANGE.to}`;
  await page.goto(metrics);
  const list = page.locator('[data-panel="list"]');
  const rail = page.locator('[data-panel="metrics"]');

  // No raw points by default; Recorded points opens the bounded list on request, and closing it leaves the board.
  const names = rail.getByRole('list', { name: 'Metric names' }).getByRole('link');
  await expect.poll(() => names.count(), { timeout: 30_000 }).toBeGreaterThan(0);
  expect(await list.count()).toBe(0);
  await rail.getByRole('link', { name: 'Recorded points' }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('points')).toBe('1');
  const points = page.getByRole('dialog', { name: 'Recorded points' });
  // Points list exact integers; the rail lists each recorded metric with its type and unit; no chart until one is chosen.
  await expect.poll(() => points.locator('[data-panel="list"]').textContent(), { timeout: 30_000 }).toContain(UNSAFE[1]);
  expect(await list.textContent()).toContain(UNSAFE[0]);
  await page.getByRole('button', { name: 'Close Recorded points' }).click();
  await expect.poll(() => new URL(page.url()).searchParams.has('points')).toBe(false);
  expect(await list.count()).toBe(0);
  expect(await names.allTextContents()).toEqual(
    expect.arrayContaining([expect.stringMatching(/ui\.latency\s*histogram · s/), expect.stringMatching(/ui\.queue\s*gauge · By/), expect.stringMatching(/ui\.queue\s*sum · 1/)])
  );
  expect(await page.locator('[data-panel="series"]').count()).toBe(0);
  await rail.getByRole('searchbox', { name: 'Find a metric' }).fill('spr');
  expect(await names.allTextContents()).toEqual([expect.stringContaining('ui.spread')]);
  await rail.getByRole('searchbox', { name: 'Find a metric' }).fill('');

  // Selecting the metric charts every recorded series apart and summarizes each exactly, in the first response too.
  await names.filter({ hasText: /ui\.queue/ }).first().click();
  const chart = page.locator('[data-panel="series"]');
  const legend = chart.locator('table.legend tbody tr');
  await expect.poll(() => legend.count(), { timeout: 30_000 }).toBe(5);
  expect(await chart.textContent()).toContain('plotted as double precision');
  expect(await serverHtml(page, page.url())).toContain(UNSAFE[1]);
  const rows = await legend.allTextContents();
  // The unsafe series: latest and max exact, mean exact decimal.
  expect(rows.find((row) => row.includes(UNSAFE[1]))).toContain('9007199254740994');
  // Mixed integer and double samples: max is the integer 100, mean over both.
  expect(rows.find((row) => row.includes('ui-worker'))).toMatch(/100\s*100\s*51\.25/);
  // Another route at the same instant stays its own series.
  expect(rows.find((row) => row.includes('{"route":"not set"}'))).toMatch(/5\s*5\s*5/);

  // Max of samples that round to one double is the exactly larger one, whichever representation it has.
  for (const [metric, max] of [['ui.low', '-9007199254740992'], ['ui.high', UNSAFE[0]]] as const) {
    await page.goto(`${metrics}&where=${encodeURIComponent(JSON.stringify([{ column: 'metric_name', value: metric }]))}`);
    await expect.poll(() => legend.count(), { timeout: 30_000 }).toBe(1);
    const [, , latest, largest] = await legend.first().locator('td, th').allTextContents();
    expect([latest.trim(), largest.trim().replace(/\.0$/, '')]).toEqual([expect.stringMatching(/^-?9007199254740992(\.0)?$/), max]);
  }
  await page.goto(`${metrics}&where=${encodeURIComponent(JSON.stringify([{ column: 'metric_name', value: 'ui.queue' }]))}`);
  await expect.poll(() => legend.count(), { timeout: 30_000 }).toBe(5);

  // Unticking a series hides its line; the table and the chart name the same series.
  const lines = () => chart.locator('svg.plot polyline').count();
  expect(await lines()).toBe(5);
  await chart.getByRole('checkbox').first().uncheck();
  await expect.poll(() => chart.textContent()).toContain('4 shown');
  expect(await lines()).toBe(4);

  // An explicit aggregation applies across series; grouping keeps a recorded dimension apart; percentiles are unavailable.
  const calculation = chart.getByRole('group', { name: 'Calculation' });
  expect(await calculation.getByText('p90').getAttribute('aria-disabled')).toBe('true');
  expect(await chart.getByLabel('Group series by').isDisabled()).toBe(true);
  await calculation.getByRole('link', { name: 'max' }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('agg')).toBe('max');
  await expect.poll(() => legend.count()).toBe(2);
  expect((await legend.allTextContents()).find((row) => row.includes('sum'))).toContain(UNSAFE[1]);
  await chart.getByLabel('Group series by').selectOption('service_name');
  await expect.poll(() => new URL(page.url()).searchParams.get('by')).toBe('service_name');
  // An unset service and the text "not set" stay two groups.
  await expect.poll(() => legend.count()).toBe(4);
  // Captions name only the cells that differ; the service is the last of them.
  const services = (await legend.locator('th[scope="row"]').allTextContents()).map((caption) => caption.trim().split(' · ').at(-1));
  expect(services.toSorted()).toEqual(['not set', 'ui-agent', 'ui-worker', '∅']);

  // Histogram, exponential histogram and summary points are listed with exact nested counts and explicitly not plotted.
  const structures = ['metric_name', 'histogram_count', 'bucket_counts', 'explicit_bounds', 'positive_buckets', 'negative_buckets', 'summary_count', 'summary_sum', 'quantile_values', 'exemplars'];
  await page.goto(`${metrics}&where=${encodeURIComponent(JSON.stringify([{ column: 'metric_name', value: 'ui.latency' }]))}&columns=${encodeURIComponent(JSON.stringify(structures))}&points=1`);
  await expect.poll(() => chart.textContent(), { timeout: 30_000 }).toContain('No gauge or sum point of ui.latency');
  await expect.poll(() => list.textContent()).toContain(`["${UNSAFE[0]}","2"]`);
  expect(await list.textContent()).toContain('9007199254740995');
  // Without the metric filter the inspector lists every recorded structure.
  await page.goto(`${metrics}&columns=${encodeURIComponent(JSON.stringify(structures))}&points=1`);
  await expect.poll(() => list.textContent()).toContain(`"bucket_counts":["${UNSAFE[0]}","1"]`);
  const all = await list.textContent();
  expect(all).toContain('"offset":-1');
  expect(all).toContain(UNSAFE[1]);
  expect(all).toContain('12.25');
  expect(all).toContain('0.99');
  expect(all).toContain('/exemplar');
  // Revealing one more structure goes through Columns into the URL.
  await list.locator('summary', { hasText: 'Columns' }).click();
  await list.locator('label[title="exponential_zero_count"] input').check();
  await expect.poll(() => JSON.parse(new URL(page.url()).searchParams.get('columns') ?? '[]')).toContain('exponential_zero_count');
});

/** The canonical palette, resolved per mode to the `rgb(r, g, b)` a computed style reports. */
const PALETTE = JSON.parse(readFileSync(resolve(import.meta.dirname, '../../brand/palette.json'), 'utf8')) as { tokens: Record<string, { light: string; dark: string }> };
const rgb = (mode: 'light' | 'dark', token: string) => {
  const hex = PALETTE.tokens[token][mode];
  return `rgb(${[1, 3, 5].map((at) => parseInt(hex.slice(at, at + 2), 16)).join(', ')})`;
};

/** Every box of `locator`, in document order. */
const boxes = async (locator: ReturnType<Page['locator']>) =>
  Promise.all((await locator.all()).map(async (each) => (await each.boundingBox()) ?? { x: NaN, y: NaN, width: NaN, height: NaN }));
/** One computed style property of every element `locator` matches. */
const computed = (locator: ReturnType<Page['locator']>, property: string) =>
  locator.evaluateAll((elements, name) => elements.map((element) => getComputedStyle(element).getPropertyValue(name)), property);

test('observe boards match approved canvas', async ({ server, ui }) => {
  // Two recorded services on one gauge, so the Metrics board plots the first two series roles.
  await exportOtlp(server, ui.adminKey, '/v1/metrics', {
    resourceMetrics: ['ui-agent', 'ui-worker'].map((service, index) => ({
      resource: { attributes: [{ key: 'service.name', value: { stringValue: service } }] },
      scopeMetrics: [{ scope: { name: 'wyrd-ui-canvas' }, metrics: [{ name: 'ui.canvas', unit: '1', gauge: { dataPoints: [0, 1].map((minute) => ({ timeUnixNano: nanos(`2026-10-19T05:0${minute}:00Z`), asDouble: index + minute })) } }] }]
    }))
  });
  server.flushBifrost();
  const page = await humanSignIn(ui.browser, ui.base, TENANT, await invitation(ui.serverUrl, ui.adminKey, 'admin'));
  const observe = `${ui.base}/t/${TENANT}/observe?from=${RANGE.from}&to=${RANGE.to}`;
  const capture = async (board: string, mode: string, width: number) =>
    process.env.WYRD_UI_CAPTURE_DIR && page.screenshot({ path: join(process.env.WYRD_UI_CAPTURE_DIR, `${board}-${mode}-${width}.png`), fullPage: true });

  for (const mode of ['dark', 'light'] as const) {
    await page.context().addCookies([{ name: 'wyrd-mode', value: mode, url: ui.base }]);
    const token = (name: string) => rgb(mode, name);
    for (const width of [1440, 390]) {
      await page.setViewportSize({ width, height: 900 });
      const desktop = width === 1440;

      // Traces. The selected theme is the rendered one, and light is really light, before any colour is compared.
      await page.goto(observe);
      const list = page.locator('[data-panel="list"]');
      await expect.poll(() => list.textContent(), { timeout: 30_000 }).toContain('invoke_agent support');
      expect(await page.locator('.app-root').getAttribute('data-mode')).toBe(mode);
      const canvas = await computed(page.locator('.app-root'), 'background-color');
      expect(canvas).toEqual([token('--bg')]);
      const lightness = canvas[0].match(/\d+/g)?.map(Number) ?? [];
      expect(lightness.every((channel) => (mode === 'light' ? channel > 200 : channel < 40))).toBe(true);
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);

      // The navigation rail and the applied WHERE carry their own canvas roles.
      const nav = page.getByRole('complementary', { name: 'Primary' });
      expect(await computed(nav, 'background-color')).toEqual([token('--nav-surface')]);
      expect((await nav.boundingBox())?.width).toBe(desktop ? 212 : width);
      expect(await computed(page.locator('form.where'), 'border-top-color')).toEqual([token('--selected-border')]);

      // Overview, list bar, then list; three 56px plots on one baseline on desktop, stacked inside the screen on mobile.
      const overview = page.locator('[data-panel="overview"]');
      const [top, bar, rows] = await boxes(page.locator('[data-panel="overview"], .listbar, [data-panel="list"]'));
      expect(top.y < bar.y && bar.y < rows.y).toBe(true);
      const plots = await boxes(overview.locator('.box'));
      expect(plots.map((plot) => plot.height)).toEqual([56, 56, 56]);
      const bottoms = plots.map((plot) => plot.y + plot.height);
      if (desktop) expect(new Set(bottoms).size).toBe(1);
      else expect(bottoms[0] < bottoms[1] && bottoms[1] < bottoms[2]).toBe(true);
      expect(plots.every((plot) => plot.x >= 0 && plot.x + plot.width <= width)).toBe(true);
      const [spans, errors, duration] = await overview.locator('.chart').all();
      expect(new Set(await computed(spans.locator('rect.segment'), 'fill'))).toEqual(new Set([token('--series-3')]));
      expect(new Set(await computed(errors.locator('rect.segment'), 'fill'))).toEqual(new Set([token('--danger')]));
      expect(new Set(await computed(overview.locator('rect.segment'), 'opacity'))).toEqual(new Set(['1']));
      expect(await computed(duration.locator('svg.plot polyline'), 'stroke')).toEqual([token('--observe'), token('--declare')]);
      expect(await duration.getByLabel('Key').textContent()).toMatch(/p95.*p50/);
      expect(await computed(errors.locator('.headline'), 'color')).toEqual([token('--danger')]);
      expect(await page.locator('.listbar .counts').textContent()).toMatch(/^1 trace · /);
      await capture('traces', mode, width);

      // The time picker opens on the panel surface, inside the screen.
      await page.getByRole('button', { name: /^Time range/ }).click();
      const picker = page.getByRole('dialog', { name: 'Time range' });
      expect(await computed(picker, 'background-color')).toEqual([token('--surface')]);
      const panel = await picker.boundingBox();
      expect(panel && panel.x >= 0 && panel.x + panel.width <= width).toBe(true);
      await capture('timepicker', mode, width);
      await picker.press('Escape');

      // Spans: every matching span of the range, filters open.
      await page.goto(`${observe}&mode=spans`);
      await expect.poll(() => list.locator('tbody tr').count(), { timeout: 30_000 }).toBe(3);
      expect(await page.locator('[data-panel="facets"]').count()).toBe(1);
      expect(await page.locator('.listbar .counts').textContent()).toMatch(/^3 spans match · p95 /);
      expect(await list.locator('th').allTextContents()).toEqual(['Start', 'Name', 'Service', 'Trace', 'Parent', 'Duration', 'Status']);
      expect(await list.getByTitle(TRACE).first().textContent()).toBe(`${TRACE.slice(0, 4)}…${TRACE.slice(-4)}`);
      expect(await list.locator('.foot').textContent()).toMatch(/Showing 3 spans · a trace id opens that trace/);
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
      await capture('spans', mode, width);

      // The waterfall: 884px drawer (the screen on mobile), 28px rows on the 300/110/flex/64 grid, bars by recorded service and error.
      await page.goto(`${observe}&trace=${TRACE}`);
      const drawer = page.getByRole('dialog', { name: 'Trace' });
      const spanRows = drawer.getByRole('tree').getByRole('treeitem');
      await drawer.getByRole('button', { name: 'Expand all' }).click({ timeout: 30_000 });
      await expect.poll(() => spanRows.count(), { timeout: 30_000 }).toBe(3);
      expect((await drawer.boundingBox())?.width).toBe(Math.min(884, width));
      expect(new Set((await boxes(spanRows)).map((row) => row.height))).toEqual(new Set([28]));
      if (desktop) {
        const grid = (await computed(spanRows.first(), 'grid-template-columns'))[0].split(' ').map((column) => Math.round(parseFloat(column)));
        expect([grid[0], grid[1], grid[3]]).toEqual([300, 110, 64]);
      }
      const bars = Object.fromEntries(
        await spanRows.evaluateAll((items) => items.map((item) => [item.querySelector('.select')?.getAttribute('title'), getComputedStyle(item.querySelector('.bar') as Element).backgroundColor]))
      );
      expect(bars).toEqual({ 'invoke_agent support': token('--declare'), 'chat ranker': token('--declare'), 'execute_tool search': token('--danger') });
      const key = drawer.getByRole('list', { name: 'Services' });
      expect((await key.locator('li').allTextContents()).map((item) => item.trim())).toEqual(['ui-agent', '✕ error']);
      expect(await computed(key.locator('i'), 'background-color')).toEqual([token('--declare'), token('--danger')]);
      // The header shortens the trace id and keeps it whole as its title; the selected error span opens its detail.
      expect(await drawer.locator('.facts').getByTitle(TRACE).textContent()).toBe(`${TRACE.slice(0, 4)}…${TRACE.slice(-4)}`);
      await drawer.locator('button.select[title="execute_tool search"]').click();
      await expect.poll(() => drawer.locator('.span').textContent(), { timeout: 30_000 }).toContain('execute_tool search');
      await capture('waterfall', mode, width);

      // Logs: one 64px severity band in the named fills, then facets, records and the record side by side (stacked on mobile).
      await page.goto(`${observe}&signal=logs`);
      const timeline = page.locator('[data-panel="timeline"]');
      await expect.poll(() => list.textContent(), { timeout: 30_000 }).toContain('search timed out');
      expect((await timeline.locator('.box').boundingBox())?.height).toBe(64);
      expect(await computed(timeline.locator('rect.segment'), 'fill')).toEqual([token('--danger')]);
      expect(new Set(await computed(timeline.locator('rect.segment'), 'opacity'))).toEqual(new Set(['1']));
      expect(await computed(timeline.getByLabel('Key').locator('i'), 'background-color')).toEqual([token('--border'), token('--level-info'), token('--warn'), token('--danger'), token('--muted')]);
      expect(await timeline.locator('h3').textContent()).toMatch(/· drag to zoom$/);
      expect(await timeline.getByRole('button', { name: /Open in Query/ }).count()).toBe(0);
      // Group by sits in the records header with Wrap lines, Columns and CSV; severities read in their level's colour.
      expect(await list.locator('.bar').getByLabel('Group by').count()).toBe(1);
      expect(await computed(list.locator('tbody td .level-error'), 'color')).toEqual([token('--danger')]);
      expect(await computed(page.locator('[data-panel="facets"] .value.error'), 'color')).toEqual([token('--danger')]);
      await list.locator('tbody tr').first().click();
      const aside = page.getByRole('complementary', { name: 'Log record' });
      await expect.poll(() => aside.textContent(), { timeout: 30_000 }).toContain('search timed out');
      const [facets, records, record] = await boxes(page.locator('.work > [data-panel="facets"], .work > [data-panel="list"], .work > aside'));
      if (desktop) {
        expect(facets.x + facets.width <= records.x && records.x + records.width <= record.x).toBe(true);
        expect(Math.abs(records.y - record.y)).toBeLessThan(1);
        expect(record.width).toBeLessThanOrEqual(400);
        // Beside the open record, the count and its tools still share one header row.
        const [count, tools] = await boxes(list.locator('.bar > span'));
        expect(Math.abs(count.y + count.height / 2 - (tools.y + tools.height / 2))).toBeLessThan(2);
      } else expect(facets.y < records.y && records.y + records.height <= record.y).toBe(true);
      expect([facets, records, record].every((region) => region.x >= 0 && region.x + region.width <= width)).toBe(true);
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
      await capture('logs', mode, width);

      // Metrics: a 240px rail, then the chart card above the series card; no raw points until asked for.
      await page.goto(`${observe}&signal=metrics&where=${encodeURIComponent(JSON.stringify([{ column: 'metric_name', value: 'ui.canvas' }]))}`);
      const series = page.locator('[data-panel="series"]');
      await expect.poll(() => series.locator('table.legend tbody tr').count(), { timeout: 30_000 }).toBe(2);
      if (desktop) expect((await page.locator('[data-panel="metrics"]').boundingBox())?.width).toBe(240);
      const [plotCard, seriesCard] = await series.locator('.chart > .frame').all();
      const [plotBox, seriesBox] = [await plotCard.boundingBox(), await seriesCard.boundingBox()];
      expect(plotBox && seriesBox && plotBox.y + plotBox.height <= seriesBox.y).toBe(true);
      expect(await plotCard.locator('table.legend').count()).toBe(0);
      expect(new Set((await boxes(seriesCard.locator('table.legend tbody tr'))).map((row) => row.height))).toEqual(new Set([28]));
      expect(await computed(plotCard.locator('svg.plot polyline'), 'stroke')).toEqual([token('--declare'), token('--observe')]);
      expect(await computed(series.getByRole('group', { name: 'Calculation' }).locator('[aria-current="true"]'), 'border-top-color')).toEqual([token('--selected-border')]);
      // The calculation choices share the metric name's header row on desktop; the zoom hint reads under the plot.
      if (desktop) {
        const [name, calculation] = [await series.locator('.metric').boundingBox(), await series.getByRole('group', { name: 'Calculation' }).boundingBox()];
        expect(name && calculation && calculation.y < name.y + name.height && calculation.x > name.x + name.width - 1).toBe(true);
      }
      expect(await plotCard.locator('.note').textContent()).toBe('Drag across the chart to zoom every Observe view to that window.');
      expect(await page.locator('[data-panel="list"]').count()).toBe(0);
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
      await capture('metrics', mode, width);
      await page.getByRole('link', { name: 'Recorded points' }).click();
      const inspector = page.getByRole('dialog', { name: 'Recorded points' });
      await expect.poll(() => inspector.locator('[data-panel="list"] tbody tr').count(), { timeout: 30_000 }).toBe(4);
      expect((await inspector.boundingBox())?.width).toBe(Math.min(884, width));
      await capture('metrics-points', mode, width);
      await inspector.press('Escape');
      await expect.poll(() => new URL(page.url()).searchParams.has('points')).toBe(false);
    }
  }
});

/** A password the server's policy accepts, unique per call. */
const strongPassword = () => `correct horse ${crypto.randomUUID()}`;

/** The profile `key` resolves to, read through the SDK straight from the server. */
const meWith = async (serverUrl: string, key: string) =>
  (await Principals.connect({ client: WyrdClient.connect({ serverUrl, credential: key }) })).me();

test('local signup and personal settings', async ({ server, ui }) => {
  const tenantKey = 'ui-local';
  const tenant = server.seedTenant(tenantKey);
  const adminKey = server.bootstrapServiceInTenant(tenant, ['admin'], 'ui-local-admin');
  const code = await invitation(ui.serverUrl, adminKey, 'editor');
  const login = `${ui.base}/t/${tenantKey}/login`;
  // Longer than 128 UTF-16 units, decomposed: the server counts NFC characters, so the form must not cut it.
  const password = `${'𝒜'.repeat(64)} cafe\u0301 ${crypto.randomUUID().slice(0, 8)}`;

  const context = await ui.browser.newContext();
  const page = await context.newPage();
  const logged: string[] = [];
  page.on('console', (message) => logged.push(message.text()));
  try {
    // A wrong invitation code is refused without creating the account.
    await page.goto(login);
    await page.getByRole('link', { name: 'Sign up with an invitation' }).click();
    await page.getByLabel('Invitation code').fill('A'.repeat(43));
    await page.getByLabel('Username').fill('ui-person');
    await page.getByLabel('Password').fill(password);
    await page.getByRole('button', { name: 'Create account' }).click();
    await expect.poll(() => page.getByRole('alert').count()).toBe(1);
    expect(page.url()).toContain('/signup');

    await page.getByLabel('Invitation code').fill(code);
    await page.getByLabel('Password').fill(password);
    // The keyboard reveals and conceals the password without changing it.
    const field = page.getByLabel('Password');
    await page.getByRole('button', { name: 'Show password' }).focus();
    await page.keyboard.press('Enter');
    expect(await field.getAttribute('type')).toBe('text');
    expect(await field.inputValue()).toBe(password);
    await page.keyboard.press('Space');
    expect(await field.getAttribute('type')).toBe('password');
    expect(await field.inputValue()).toBe(password);
    await page.getByRole('button', { name: 'Create account' }).click();
    await page.waitForURL(`${ui.base}/t/${tenantKey}`);
    expect(page.url()).not.toContain(code);

    // The sidebar names the person by username, and the identity menu leads to personal Settings, which an editor without SSO administration may open.
    expect(await page.locator('details.identity .who > span').first().textContent()).toBe('ui-person');
    await page.locator('details.identity summary').click();
    await page.getByRole('link', { name: 'Account' }).click();
    const account = `${ui.base}/t/${tenantKey}/settings/account`;
    await page.waitForURL(account);
    await expect.poll(() => page.locator('dl').textContent()).toContain('ui-person');
    expect(await page.locator('dl').textContent()).toContain('Username and password');
    await expect.poll(() => page.getByText('No keys yet').count()).toBe(1);
    expect(await page.getByLabel('Expires after (days)').inputValue()).toBe('30');
    expect(await page.locator('input[name=permission]:checked').count()).toBe(0);

    // A key with no permissions chosen is refused; one with a ceiling is shown once.
    await page.getByLabel('Name').fill('laptop');
    await page.getByRole('button', { name: 'Create key' }).click();
    await expect.poll(() => page.getByRole('alert').count()).toBe(1);
    await page.getByLabel('Name').fill('laptop');
    await page.locator('input[name=permission][value="cards:read"]').check();
    await page.getByRole('button', { name: 'Create key' }).click();
    const shown = page.locator('[data-secret]');
    await expect.poll(() => shown.count()).toBe(1);
    const secret = (await shown.textContent()) ?? '';
    expect(secret.length).toBeGreaterThan(40);
    expect((await meWith(ui.serverUrl, secret)).permissions).toEqual(['cards:read']);

    await page.getByRole('button', { name: 'Dismiss' }).click();
    expect(await shown.count()).toBe(0);
    await page.goto(account);
    expect(await page.content()).not.toContain(secret);
    expect(await serverHtml(page, account)).not.toContain(secret);
    const row = page.locator('tbody tr', { hasText: 'laptop' });
    expect(await row.textContent()).toContain('✓ active');
    expect(await row.textContent()).toContain('cards:read');

    // Revoke reports success and the key stops working.
    await row.getByRole('button', { name: 'Revoke laptop' }).click();
    await expect.poll(() => page.getByText('Key revoked.').count()).toBe(1);
    expect(await page.locator('tbody tr', { hasText: 'laptop' }).textContent()).toContain('⊘ revoked');
    await expect(meWith(ui.serverUrl, secret)).rejects.toMatchObject({ code: 'WYRD_AUTH_401_API_KEY_INVALID' });

    // A password change ends the session; only the new password signs in.
    const next = strongPassword();
    await page.getByLabel('Current password').fill(password);
    await page.getByLabel('New password').fill(next);
    await page.getByRole('button', { name: 'Change password' }).click();
    await page.waitForURL(login);
    expect((await page.request.get(account, { maxRedirects: 0 })).status()).toBe(303);
    await page.getByLabel('Username').fill('ui-person');
    await page.getByLabel('Password').fill(password);
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await expect.poll(() => page.getByRole('alert').count()).toBe(1);
    expect(await page.getByLabel('Username').inputValue()).toBe('ui-person');
    expect(await page.getByLabel('Password').inputValue()).toBe('');
    await page.getByLabel('Password').fill(next);
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await page.waitForURL(`${ui.base}/t/${tenantKey}`);

    // Another tenant's sign-in does not admit this User.
    const foreign = await (await ui.browser.newContext()).newPage();
    await foreign.goto(`${ui.base}/t/${TENANT}/login`);
    await foreign.getByLabel('Username').fill('ui-person');
    await foreign.getByLabel('Password').fill(next);
    await foreign.getByRole('button', { name: 'Sign in', exact: true }).click();
    await expect.poll(() => foreign.getByRole('alert').count()).toBe(1);
    expect(foreign.url()).toContain('/login');

    // Repeated wrong guesses are throttled: sign-in and password change both say when to try again.
    const guess = strongPassword();
    const guesser = await (await ui.browser.newContext()).newPage();
    await guesser.goto(login);
    const throttled = async (target: Page, submit: () => Promise<void>) => {
      const [response] = await Promise.all([target.waitForResponse((r) => r.request().method() === 'POST'), submit()]);
      return response;
    };
    let refused = await throttled(guesser, async () => {
      await guesser.getByLabel('Username').fill('ui-person');
      await guesser.getByLabel('Password').fill(guess);
      await guesser.getByRole('button', { name: 'Sign in', exact: true }).click();
    });
    for (let attempt = 0; refused.status() !== 429 && attempt < 10; attempt++) {
      refused = await throttled(guesser, async () => {
        await guesser.getByLabel('Password').fill(guess);
        await guesser.getByRole('button', { name: 'Sign in', exact: true }).click();
      });
    }
    expect(refused.status()).toBe(429);
    expect(Number(refused.headers()['retry-after'])).toBeGreaterThan(0);
    await expect.poll(() => guesser.getByRole('alert').textContent()).toMatch(/Try again in \d+ (seconds|minutes?)\./);
    expect(await guesser.content()).not.toContain(guess);
    await page.goto(account);
    const change = await throttled(page, async () => {
      await page.getByLabel('Current password').fill(next);
      await page.getByLabel('New password').fill(strongPassword());
      await page.getByRole('button', { name: 'Change password' }).click();
    });
    expect(change.status()).toBe(429);
    expect(Number(change.headers()['retry-after'])).toBeGreaterThan(0);
    await expect.poll(() => page.getByRole('alert').textContent()).toMatch(/Try again in \d+ (seconds|minutes?)\./);
    expect(await page.content()).not.toContain(next);

    for (const value of [code, password, next, secret, guess]) expect(logged.join('\n')).not.toContain(value);
  } finally {
    await context.close();
  }
});

/** The repository root, where `mise run dev:full` runs. */
const REPO = resolve(import.meta.dirname, '../../../../../..');

/** Whether anything still answers HTTP on this loopback port. */
async function answers(port: number): Promise<boolean> {
  return fetch(`http://127.0.0.1:${port}/`, { signal: AbortSignal.timeout(2_000) }).then(
    () => true,
    () => false
  );
}

/** The `Traces` total the overview shows, or NaN while it loads. */
async function traceTotal(page: Page): Promise<number> {
  // The count is a headline's own detail; the panel's whole text runs the headline value into it.
  const details = await page.locator('[data-panel="overview"] .detail').allTextContents();
  const count = details.map((text) => /^(\d[\d,]*) traces?$/.exec(text)?.[1]).find(Boolean);
  return Number(count?.replaceAll(',', '') ?? NaN);
}

/** Starts `mise run dev:full` in its own process group, writing to a log file in `config`. */
function devFull(config: string, env: Record<string, string> = {}) {
  // Output goes to a file, never a pipe this process must drain: a blocked test would stall every service writing to it.
  const log = join(config, `dev-full-${Date.now()}.log`);
  const out = openSync(log, 'w');
  const child = spawn('mise', ['run', 'dev:full', '--requests', '20', '--concurrency', '2'], {
    cwd: REPO,
    env: { ...process.env, WYRD_CONFIG_HOME: config, ...env },
    detached: true,
    stdio: ['ignore', out, out]
  });
  closeSync(out);
  const exited = new Promise<number | null>((done) => child.once('exit', (code) => done(code)));
  return {
    log,
    exited,
    output: () => readFileSync(log, 'utf8'),
    /** Ctrl-C, as a developer stops it. */
    async interrupt() {
      if (child.exitCode === null) process.kill(-child.pid!, 'SIGINT');
      await exited;
    }
  };
}

base('development environment lifecycle', { timeout: 2_400_000 }, async () => {
  // The launcher's ports are fixed; another service on one would be mistaken for this environment.
  for (const port of [8080, 3000, 8000, 1080]) expect(await answers(port), `port ${port} is already serving; stop that service first`).toBe(false);
  // A config home of its own: startup replaces the dev key there, never in the developer's.
  const config = mkdtempSync(join(tmpdir(), 'wyrd-dev-'));

  // A UI that cannot start ends startup with an error and stops everything it started.
  const broken = devFull(config, { WYRD_UI_PORT: '70000' });
  try {
    expect(await broken.exited, broken.log).not.toBe(0);
    expect(broken.output(), broken.log).toContain('the UI exited before it was ready');
    expect(broken.output()).not.toContain('UI ready');
  } finally {
    await broken.interrupt();
  }
  for (const port of [8080, 1080]) expect(await answers(port), `port ${port} after a failed UI start`).toBe(false);

  // A setup refused after the services are ready leaves them running for diagnosis and starts nothing that depends on it.
  const refused = devFull(config, { WYRD_API_KEY: 'wyrd_not_a_key' });
  try {
    await vi.waitFor(() => expect(refused.output()).toMatch(/batch {8}(not sent|passed|FAILED)/), { timeout: 1_500_000, interval: 1_000 });
    expect(refused.output(), refused.log).toContain('example app  not started: setup FAILED');
    expect(refused.output()).toContain('batch        not sent: setup FAILED');
    expect(refused.output()).not.toContain('scripts.batch');
    expect(await answers(8000), 'example app after a failed setup').toBe(false);
    expect((await fetch('http://127.0.0.1:8080/readyz')).ok).toBe(true);
    expect((await fetch('http://127.0.0.1:3000/t/dev/login')).ok).toBe(true);
    expect((await fetch('http://127.0.0.1:1080/mockserver/ready')).ok).toBe(true);
  } finally {
    await refused.interrupt();
  }
  for (const port of [8080, 3000, 8000]) expect(await answers(port), `port ${port} after Ctrl-C`).toBe(false);

  const child = devFull(config);
  const output = child.output;
  const log = child.log;
  const browser = await chromium.launch();
  try {
    // The UI's sign-in is reported ready before the example app is set up or its batch sent.
    await vi.waitFor(() => expect(output()).toContain('UI ready     http://127.0.0.1:3000/t/dev/login (sign in as development / development)'), { timeout: 1_500_000, interval: 1_000 });
    expect(output()).not.toMatch(/batch {8}/);
    expect(statSync(join(config, 'credentials.toml')).mode & 0o777).toBe(0o600);

    // The seeded development User signs in normally and reaches personal Settings.
    const page = await (await browser.newContext()).newPage();
    await page.goto('http://127.0.0.1:3000/t/dev/login');
    await page.getByLabel('Username').fill('development');
    await page.getByLabel('Password').fill('development');
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await page.waitForURL('http://127.0.0.1:3000/t/dev');
    await page.goto('http://127.0.0.1:3000/t/dev/settings/account');
    await expect.poll(() => page.locator('dl').textContent()).toContain('Username and password');

    // One command runs the batch to terminal evidence and reports usable URLs.
    await vi.waitFor(() => expect(output()).toMatch(/batch {8}(passed|FAILED)/), { timeout: 1_500_000, interval: 1_000 });
    expect(output(), log).toContain('batch        passed');
    expect(output()).toContain('UI           http://127.0.0.1:3000/t/dev/login (sign in as development / development)');
    expect(output()).not.toContain('/login/api-key');
    for (const port of [8080, 3000, 8000, 1080]) expect(await answers(port), `port ${port}`).toBe(true);

    // The signed-in browser reaches the batch's retained traces, logs and metrics.
    const observe = 'http://127.0.0.1:3000/t/dev/observe';
    // Metrics lists raw points only in its Recorded points inspector.
    for (const view of ['signal=traces', 'signal=logs', 'signal=metrics&points=1']) {
      await page.goto(`${observe}?${view}&last=1h`);
      await expect.poll(() => page.locator('[data-panel="list"] tbody tr').count(), { timeout: 120_000 }).toBeGreaterThan(0);
    }

    // An appended batch reaches the running environment without a restart.
    await page.goto(`${observe}?last=1h`);
    await expect.poll(() => traceTotal(page), { timeout: 60_000 }).toBeGreaterThan(0);
    const before = await traceTotal(page);
    execFileSync('mise', ['run', 'dev:batch', '--requests', '4', '--concurrency', '2'], { cwd: REPO, env: { ...process.env, WYRD_CONFIG_HOME: config }, stdio: 'inherit' });
    // Each reload is read only once its overview has loaded.
    await vi.waitFor(
      async () => {
        await page.reload();
        await expect.poll(() => traceTotal(page), { timeout: 30_000 }).not.toBeNaN();
        expect(await traceTotal(page)).toBeGreaterThan(before);
      },
      { timeout: 180_000, interval: 2_000 }
    );

    // Sign-out ends the session.
    await page.locator('details.identity summary').click();
    await page.getByRole('button', { name: 'Sign out' }).click();
    await page.waitForURL(/\/login$/);
  } finally {
    await browser.close();
    // Ctrl-C stops every owned child: the app first, so its telemetry drains.
    await child.interrupt();
  }
  for (const port of [8080, 3000, 8000]) expect(await answers(port), `port ${port} after Ctrl-C`).toBe(false);
});
