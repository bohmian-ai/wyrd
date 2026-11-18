/**
 * Real-browser Observe journeys. The production BFF build (`node build`) runs
 * in front of a real Wyrd server started by `@wyrd/testing` over repository
 * Postgres; Chromium signs in through the API-key page and investigates
 * evidence exported over stock OTLP. Nothing here mocks the server or the BFF.
 */
import { execFileSync, spawn } from 'node:child_process';
import { closeSync, mkdtempSync, openSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { chromium, type Browser, type Page } from 'playwright';
import { test as base, expect, vi } from 'vitest';
import { AGREES, assistantTest, judgeReply } from '../../../../../../sdks/wyrd-sdk-ts/wyrd/tests/support/assistant';
import { declared, fixture, groupExecution, runtimeAccount, scheduledResult, serverTest } from '../../../../../../sdks/wyrd-sdk-ts/wyrd/tests/support/server';
import { Cards, Principals, WyrdClient, WyrdState } from 'wyrd-ai';
import { loadExecution } from 'wyrd-ai-ui/loaders';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { declarePolicy, exportOtlp, nanos, seedSignals } from '$lib/ui-signals.integration';

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

/** Builds the production BFF that `startBff` runs. */
const buildUi = () => execFileSync('pnpm', ['exec', 'vite', 'build'], { stdio: 'inherit', env: { ...process.env, VITEST: '' } });

/** The built BFF, a browser, and a tenant holding one trace; the default tenant holds another. */
const test = serverTest().extend<{ ui: { base: string; browser: Browser; tenant: string; adminKey: string; readerKey: string; serverUrl: string } }>({
  ui: [
    async ({ server }, use) => {
      buildUi();
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

  // The rail links Observe and Verify; Traces is the default signal; the undelivered Ask stays visible and disabled.
  const rail = page.getByRole('navigation', { name: 'Destinations' });
  await rail.getByRole('link', { name: 'Observe' }).click();
  await page.waitForURL(observe);
  await expect.poll(() => signals.getByRole('link', { name: 'Traces' }).getAttribute('aria-current')).toBe('page');
  expect(await rail.getByRole('link', { name: 'Verify' }).getAttribute('href')).toBe(`/t/${TENANT}/verify`);
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

  // Wrap is a reader control over the rows already read; Columns is kept in the URL.
  await list.getByRole('button', { name: 'Wrap lines' }).click();
  expect(await list.getByRole('button', { name: 'Wrap lines' }).getAttribute('aria-pressed')).toBe('true');
  await list.locator('summary', { hasText: 'Columns' }).click();
  await list.locator('label[title="body_text"] input').uncheck();
  await expect.poll(() => list.locator('th[title="body_text"]').count()).toBe(0);
  expect(JSON.parse(new URL(page.url()).searchParams.get('columns') ?? '[]')).not.toContain('body_text');
  await page.goBack();
  await expect.poll(() => list.locator('th[title="body_text"]').count()).toBe(1);
  // One record fills one keyset page: neither Newer nor Older has a cursor.
  const pager = list.getByRole('navigation', { name: 'Pages' });
  expect([await pager.getByRole('button', { name: /Newer/ }).isDisabled(), await pager.getByRole('button', { name: /Older/ }).isDisabled()]).toEqual([true, true]);

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
              // Mixed int64 and double samples in two windows that round to one double: Max compares them exactly.
              { name: 'ui.low', unit: '1', gauge: { dataPoints: [point('2026-10-19T04:00:00Z', { asInt: '-9007199254740993' }), point('2026-10-19T05:00:00Z', { asDouble: -9007199254740992 })] } },
              { name: 'ui.high', unit: '1', gauge: { dataPoints: [point('2026-10-19T04:00:00Z', { asInt: UNSAFE[0] }), point('2026-10-19T05:00:00Z', { asDouble: 9007199254740992 })] } },
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
  // The rail searches on the server, ignoring case, and says so when nothing holds the text.
  await rail.getByRole('searchbox', { name: 'Find a metric' }).fill('SPR');
  await expect.poll(() => names.allTextContents(), { timeout: 30_000 }).toEqual([expect.stringContaining('ui.spread')]);
  await rail.getByRole('searchbox', { name: 'Find a metric' }).fill('no-such-metric');
  await expect.poll(() => rail.textContent(), { timeout: 30_000 }).toContain('No metric names hold “no-such-metric”.');
  await rail.getByRole('searchbox', { name: 'Find a metric' }).fill('');

  // One series exported under two Service releases is indexed under each accepted context, so the rail lists it under either.
  const client = WyrdClient.connect({ serverUrl: ui.serverUrl, credential: ui.adminKey });
  const cards = await Cards.connect({ client });
  await cards.declareFromPath(fixture('cards/observe_a_run/observed-model.yaml'));
  const releases = [
    declared(await cards.declareFromPath(fixture('cards/observe_a_run/observed-service.yaml'))),
    declared(await cards.declareFromPath(fixture('cards/observe_a_run/observed-service-2.yaml')))
  ];
  const { key } = await runtimeAccount({ kind: 'Service', name: 'observed-service' }, client);
  for (const release of releases)
    await exportOtlp(
      server,
      key,
      '/v1/metrics',
      { resourceMetrics: [{ resource: resource('ui-rooted'), scopeMetrics: [{ scope, metrics: [{ name: 'ui.rooted', unit: '1', gauge: { dataPoints: [point('2026-10-19T04:00:00Z', { asInt: '3' })] } }] }] }] },
      { 'x-wyrd-workload-uid': release.uid }
    );
  server.flushBifrost();
  for (const release of releases) {
    await page.goto(`${metrics}&where=${encodeURIComponent(JSON.stringify([{ column: 'workload_card_uid', value: release.uid }]))}`);
    await expect.poll(() => names.allTextContents(), { timeout: 30_000 }).toEqual([expect.stringContaining('ui.rooted')]);
  }
  await page.goto(metrics);
  await expect.poll(() => names.count(), { timeout: 30_000 }).toBeGreaterThan(1);

  // Selecting the metric aggregates across series by default: one line per recorded meaning, counters as rates.
  await names.filter({ hasText: /ui\.queue/ }).first().click();
  const chart = page.locator('[data-panel="series"]');
  const legend = chart.locator('table.legend tbody tr');
  await expect.poll(() => legend.count(), { timeout: 30_000 }).toBe(2);
  expect(await chart.textContent()).toContain('counters per second · plotted as double precision');
  const calculation = chart.getByRole('group', { name: 'Calculation' });
  expect(await calculation.getByText('p90').getAttribute('aria-disabled')).toBe('true');

  // Every series on request: each apart, named by its recorded resource and attributes.
  await calculation.getByRole('link', { name: 'every series' }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('apart')).toBe('1');
  await expect.poll(() => legend.count(), { timeout: 30_000 }).toBe(5);
  const rows = await legend.allTextContents();
  // A gauge window plots its last point: the worker's 2.5 then 100 in one window plot 100.
  expect(rows.find((row) => row.includes('ui-worker'))).toMatch(/100(\.0)?\s*100(\.0)?\s*100(\.0)?/);
  // Another route at the same instant stays its own series.
  expect(rows.some((row) => row.includes('{"route":"not set"}'))).toBe(true);

  // Unticking a series hides its line; the table and the chart name the same series.
  const lines = () => chart.locator('svg.plot polyline').count();
  expect(await lines()).toBe(5);
  await chart.getByRole('checkbox').first().uncheck();
  await expect.poll(() => chart.textContent()).toContain('4 shown');
  expect(await lines()).toBe(4);

  // Max of samples that round to one double is the exactly larger one, whichever representation it has, in the first response too.
  for (const [metric, max] of [['ui.low', '-9007199254740992'], ['ui.high', UNSAFE[0]]] as const) {
    await page.goto(`${metrics}&where=${encodeURIComponent(JSON.stringify([{ column: 'metric_name', value: metric }]))}`);
    await expect.poll(() => legend.count(), { timeout: 30_000 }).toBe(1);
    const [, , latest, largest] = await legend.first().locator('td, th').allTextContents();
    expect([latest.trim(), largest.trim().replace(/\.0+$/, '')]).toEqual([expect.stringMatching(/^-?9007199254740992(\.0+)?$/), max]);
    expect(await serverHtml(page, page.url())).toContain(max);
  }

  // An explicit aggregation applies across series; group keys keep a recorded dimension apart, one key read at a time.
  await page.goto(`${metrics}&where=${encodeURIComponent(JSON.stringify([{ column: 'metric_name', value: 'ui.queue' }]))}`);
  await expect.poll(() => legend.count(), { timeout: 30_000 }).toBe(2);
  await calculation.getByRole('link', { name: 'max' }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('agg')).toBe('max');
  await expect.poll(() => legend.count()).toBe(2);
  const keys = chart.getByRole('group', { name: 'Group series by' });
  await keys.getByLabel('Add a group key').fill('service_name');
  await keys.getByLabel('Add a group key').press('Enter');
  await expect.poll(() => new URL(page.url()).searchParams.get('by')).toBe(JSON.stringify(['service_name']));
  // An unset service and the text "not set" stay two groups.
  await expect.poll(() => legend.count()).toBe(4);
  // Captions name only the cells that differ; the service is the last of them.
  const services = (await legend.locator('th[scope="row"]').allTextContents()).map((caption) => caption.trim().split(' · ').at(-1));
  expect(services.toSorted()).toEqual(['not set', 'ui-agent', 'ui-worker', '∅']);
  // The key list offers what the metric's series record, read only once the reader opens the picker; an attribute key splits the sum by route.
  expect(await chart.locator('datalist#group-keys option[value="attributes.route"]').count()).toBe(0);
  await keys.getByLabel('Add a group key').fill('attr');
  await expect.poll(() => chart.locator('datalist#group-keys option[value="attributes.route"]').count()).toBe(1);
  await keys.getByRole('button', { name: 'Stop grouping by service_name' }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('by')).toBeNull();
  await keys.getByLabel('Add a group key').fill('attributes.route');
  await keys.getByLabel('Add a group key').press('Enter');
  await expect.poll(() => legend.count()).toBe(3);
  expect(await serverHtml(page, page.url())).not.toContain('to_json');

  // An explicit-bucket histogram offers its default merge or every series apart, each line keyed by its recorded identity.
  await page.goto(`${metrics}&where=${encodeURIComponent(JSON.stringify([{ column: 'metric_name', value: 'ui.latency' }]))}`);
  await expect.poll(() => chart.textContent(), { timeout: 30_000 }).toContain('mean of recorded sums and percentiles estimated from buckets');
  expect(await calculation.getByRole('link').allTextContents()).toEqual(['default', 'every series']);
  // A hidden line belongs to one plot of one view: rate keeps its lines, and regrouping or every series draws the new view's lines.
  const rate = page.locator('[data-panel="rate"]');
  const plotted = (panel: typeof chart) => panel.locator('svg.plot polyline').count();
  await expect.poll(() => plotted(chart), { timeout: 30_000 }).toBeGreaterThan(1);
  const [latencyLines, rateLines] = [await plotted(chart), await plotted(rate)];
  expect(rateLines).toBeGreaterThan(0);
  await chart.getByRole('checkbox').first().uncheck();
  await expect.poll(() => plotted(chart)).toBe(latencyLines - 1);
  expect(await plotted(rate)).toBe(rateLines);
  await keys.getByLabel('Add a group key').fill('service_name');
  await keys.getByLabel('Add a group key').press('Enter');
  await expect.poll(() => new URL(page.url()).searchParams.get('by')).toBe(JSON.stringify(['service_name']));
  // The one recorded series is one group, so the new view draws every line of both plots.
  await expect.poll(() => plotted(chart), { timeout: 30_000 }).toBe(latencyLines);
  expect(await plotted(rate)).toBe(rateLines);
  await calculation.getByRole('link', { name: 'every series' }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('apart')).toBe('1');
  await expect.poll(() => chart.textContent(), { timeout: 30_000 }).toContain('each series apart');
  await expect.poll(() => plotted(chart)).toBe(latencyLines);
  expect(await plotted(rate)).toBe(rateLines);
  // Returning to the first view keeps the reader's choice there.
  await page.goBack();
  await page.goBack();
  await expect.poll(() => new URL(page.url()).searchParams.get('by')).toBeNull();
  await expect.poll(() => plotted(chart), { timeout: 30_000 }).toBe(latencyLines - 1);
  expect(await plotted(rate)).toBe(rateLines);
  await page.goForward();
  await page.goForward();
  await expect.poll(() => chart.textContent(), { timeout: 30_000 }).toContain('each series apart');
  await legend.first().getByRole('button').click();
  expect(await legend.first().locator('dl').textContent()).toContain('ui-agent');

  // An explicit-bucket histogram plots estimated percentiles; every structure's points list exact nested counts.
  const structures = ['metric_name', 'histogram_count', 'bucket_counts', 'explicit_bounds', 'positive_buckets', 'negative_buckets', 'summary_count', 'summary_sum', 'quantile_values', 'exemplars'];
  await page.goto(`${metrics}&where=${encodeURIComponent(JSON.stringify([{ column: 'metric_name', value: 'ui.latency' }]))}&columns=${encodeURIComponent(JSON.stringify(structures))}&points=1`);
  await expect.poll(() => chart.textContent(), { timeout: 30_000 }).toContain('mean of recorded sums and percentiles estimated from buckets');
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
      expect(await list.locator('th').allTextContents()).toEqual(['Start ↓', 'Name', 'Service', 'Trace', 'Parent', 'Duration', 'Status']);
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
      // Every series apart, so both services' lines are drawn; the default merges them into one.
      await page.goto(`${observe}&signal=metrics&apart=1&where=${encodeURIComponent(JSON.stringify([{ column: 'metric_name', value: 'ui.canvas' }]))}`);
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

test('authenticated card browsing and prompt comparison', async ({ ui }) => {
  const cards = await Cards.connect({ client: WyrdClient.connect({ serverUrl: ui.serverUrl, credential: ui.adminKey }) });
  const ui_cards = (name: string) => fixture(`cards/ui_cards/${name}`);
  const model = declared(await cards.declareFromPath(fixture('cards/declare_and_pull/support-model.yaml')));
  await cards.declareFromPath(ui_cards('support-reply-1.0.0.yaml'));
  const prompt = declared(await cards.declareFromPath(ui_cards('support-reply-1.1.0.yaml')));
  const agent = declared(await cards.declareFromPath(ui_cards('reply-agent.yaml')));
  // The default tenant's Prompt is never listed here.
  const theirs = declared(await (await Cards.connect()).declareFromPath(fixture('cards/gateway_inference/ask-prompt.yaml')));

  const page = await humanSignIn(ui.browser, ui.base, TENANT, await invitation(ui.serverUrl, ui.adminKey, 'admin'));
  const inventory = `${ui.base}/t/${TENANT}/cards`;
  await page.getByRole('navigation', { name: 'Destinations' }).getByRole('link', { name: 'Cards' }).click();
  await page.waitForURL(inventory);
  const rows = page.getByRole('table', { name: 'Card versions' }).locator('.row');
  const mine = rows.filter({ hasText: prompt.name });
  await expect.poll(() => mine.count()).toBe(2);
  expect(await rows.filter({ hasText: agent.name }).count()).toBe(1);
  expect(await page.getByRole('region', { name: 'Cards', exact: true }).textContent()).not.toContain(theirs.name);

  // Kind and name filters live in the URL; Back restores them.
  await page.getByRole('navigation', { name: 'Kind' }).getByRole('button', { name: 'Prompt', exact: true }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('kind')).toBe('Prompt');
  await expect.poll(() => rows.filter({ hasText: agent.name }).count()).toBe(0);
  expect(new Set(await rows.locator('.kind').allTextContents())).toEqual(new Set(['Prompt']));
  await page.getByRole('textbox', { name: /^Filter Cards/ }).fill('reply-agent');
  await page.getByRole('textbox', { name: /^Filter Cards/ }).press('Enter');
  await expect.poll(() => page.getByRole('region', { name: 'Cards', exact: true }).textContent()).toContain('No Cards match');
  await page.goBack();
  await expect.poll(() => mine.count()).toBe(2);
  expect(new URL(page.url()).searchParams.get('q')).toBeNull();
  expect(new URL(page.url()).searchParams.get('kind')).toBe('Prompt');
  // An invalid filter is the server's stable refusal.
  await page.getByRole('textbox', { name: /^Filter Cards/ }).fill('name = = "x"');
  await page.getByRole('textbox', { name: /^Filter Cards/ }).press('Enter');
  await expect.poll(() => page.getByRole('region', { name: 'Cards', exact: true }).textContent()).toContain('WYRD_SPEC_400_VALIDATION');
  await page.goBack();

  // A row opens its exact version; Back returns to the filtered rows.
  // The whole row is the target; it opens that exact version.
  await page.getByRole('button', { name: `Open Prompt/${prompt.name}@1.1.0` }).click();
  await page.waitForURL((url) => url.pathname.endsWith('/cards/detail') && url.searchParams.get('version') === '1.1.0');
  expect(new URL(page.url()).searchParams.get('uid')).toBe(prompt.uid);
  const definition = page.getByRole('region', { name: 'Prompt', exact: true });
  await expect.poll(() => definition.textContent()).toContain('Cite the policy section you rely on.');
  expect(await page.getByRole('region', { name: 'Variables', exact: true }).textContent()).toMatch(/ticket.*tone/);
  expect(await page.getByRole('region', { name: 'Used by', exact: true }).textContent()).toContain(`${agent.name}@1.0.0`);
  // The overview names what changed in one line; the comparison itself is the Versions tab, a unified diff.
  expect(await page.getByRole('note', { name: 'Changes', exact: true }).textContent()).toMatch(/Changed from v1\.0\.0: .*response_format/);
  expect(await page.getByRole('region', { name: /^Changes from/ }).count()).toBe(0);
  await page.getByRole('link', { name: 'Compare versions' }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('tab')).toBe('versions');
  const changes = page.getByRole('region', { name: /^Changes from/ });
  await expect.poll(() => changes.textContent()).toContain('v1.0.0 → v1.1.0');
  expect(await changes.getByRole('group', { name: 'response_format', exact: true }).locator('[data-line="added"]').textContent()).toContain('+ verdict');
  await page.getByRole('navigation', { name: `${prompt.name} sections` }).getByRole('button', { name: 'Overview' }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('tab')).toBeNull();
  // A Prompt records no telemetry: its primary action opens the Agent using it.
  expect(await page.getByRole('button', { name: /Open in Observe/ }).count()).toBe(0);
  expect(await page.getByRole('button', { name: `Open ${agent.name}`, exact: true }).isEnabled()).toBe(true);

  // Choosing another version replaces the whole reference; that version has no Agent and, being the first, nothing to compare from.
  await page.getByLabel('Version').selectOption('1.0.0');
  await expect.poll(() => new URL(page.url()).searchParams.get('version')).toBe('1.0.0');
  expect(new URL(page.url()).searchParams.get('uid')).toBeNull();
  await expect.poll(() => page.getByRole('region', { name: 'Used by', exact: true }).textContent()).toContain('No registered Agent uses this Prompt version.');
  expect(await page.getByRole('note', { name: 'Changes', exact: true }).count()).toBe(0);
  expect(await definition.textContent()).not.toContain('Cite the policy section');

  // Tabs keep the exact reference: Versions, YAML, then Files. The first version has nothing to compare from.
  await page.getByRole('navigation', { name: `${prompt.name} sections` }).getByRole('button', { name: /^Versions/ }).click();
  await expect.poll(() => page.getByRole('region', { name: 'Changes between versions', exact: true }).textContent()).toContain('Version 1.0.0 is the first registered version.');
  await page.getByRole('navigation', { name: `${prompt.name} sections` }).getByRole('button', { name: 'YAML' }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('tab')).toBe('yaml');
  await expect.poll(() => page.getByRole('region', { name: 'YAML', exact: true }).textContent()).toContain('version: "1.0.0"');
  await page.getByRole('navigation', { name: `${prompt.name} sections` }).getByRole('button', { name: 'Files' }).click();
  await expect.poll(() => page.getByRole('region', { name: 'Files', exact: true }).textContent()).toContain('Registration recorded no files for this version.');
  expect(new URL(page.url()).searchParams.get('version')).toBe('1.0.0');

  // The Agent's relationships open the exact Prompt it uses.
  await page.goto(`${ui.base}/t/${TENANT}/cards/detail?kind=Agent&name=${agent.name}&version=1.0.0`);
  // The Agent's own page shows the exact Prompt it runs, so reading it needs no second page.
  await expect.poll(() => page.getByRole('region', { name: `Prompt ${prompt.name}@1.1.0`, exact: true }).textContent()).toContain('Cite the policy section you rely on.');
  const related = page.getByRole('region', { name: 'Relationships', exact: true });
  await related.getByRole('button', { name: `references Prompt/${prompt.name}@1.1.0#${prompt.uid}` }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('uid')).toBe(prompt.uid);
  // A file-carrying Card lists what registration recorded.
  await page.goto(`${ui.base}/t/${TENANT}/cards/detail?kind=Model&name=${model.name}&version=${model.version}&tab=files`);
  await expect.poll(() => page.getByRole('region', { name: 'Files', exact: true }).textContent()).toContain('support-model.bin');

  // An invalid identity is a validation error, an absent version not found, and another tenant's Card absent; none falls back to latest.
  const detail = `${ui.base}/t/${TENANT}/cards/detail`;
  expect(await serverHtml(page, `${detail}?kind=Prompt&name=${prompt.name}`)).toContain('WYRD_SPEC_400_VALIDATION');
  expect(await serverHtml(page, `${detail}?kind=Prompt&name=${prompt.name}&version=latest`)).toContain('WYRD_SPEC_400_VALIDATION');
  expect(await serverHtml(page, `${detail}?kind=Prompt&name=${prompt.name}&version=9.9.9`)).toContain('WYRD_REGISTRY_404_CARD_NOT_FOUND');
  expect(await serverHtml(page, `${detail}?kind=Prompt&name=${theirs.name}&version=${theirs.version}`)).toContain('WYRD_REGISTRY_404_CARD_NOT_FOUND');

  // A principal holding no role sees the inventory and detail denied with the stable code.
  const viewer = await signIn(ui.browser, ui.base, TENANT, ui.readerKey);
  await viewer.goto(inventory);
  await expect.poll(() => viewer.getByRole('region', { name: 'Cards', exact: true }).textContent()).toContain('WYRD_PERMISSION_403_DENIED_RBAC');
  expect(await serverHtml(viewer, `${detail}?kind=Prompt&name=${prompt.name}&version=1.1.0`)).toContain('WYRD_PERMISSION_403_DENIED_RBAC');
});

test('an exact Verifier Card page draws its declared graph and where it runs', async ({ ui }) => {
  const cards = await Cards.connect({ client: WyrdClient.connect({ serverUrl: ui.serverUrl, credential: ui.adminKey }) });
  // A Service bound to a three-step Verifier, and a second version of a small Verifier.
  const receipt = await cards.declareFromPath(fixture('cards/verifier_building_blocks/policy-desk.yaml'));
  const review = declared(receipt, 'policy-review');
  const promise = fixture('cards/declare_and_pull/no-refund-promise.yaml');
  const older = declared(await cards.declareFromPath(promise));
  const next = join(mkdtempSync(join(tmpdir(), 'wyrd-ui-verifier-')), 'no-refund-promise.yaml');
  writeFileSync(next, readFileSync(promise, 'utf8').replace('version: 1.0.0', 'version: 1.1.0'));
  await cards.declareFromPath(next);

  const page = await humanSignIn(ui.browser, ui.base, TENANT, await invitation(ui.serverUrl, ui.adminKey, 'admin'));
  const detail = `${ui.base}/t/${TENANT}/cards/detail`;
  await page.goto(`${detail}?kind=Verifier&name=${review.name}&version=${review.version}`);
  const graph = page.getByRole('region', { name: 'Graph', exact: true });
  const node = (key: string) => graph.locator(`[data-node="${key}"]`);
  const selected = () => graph.locator('[data-node][aria-pressed="true"]').evaluateAll((nodes) => nodes.map((each) => each.getAttribute('data-node')));

  // The declaration as registered: description, stage headings, every node and dependency.
  await expect.poll(() => graph.locator('[data-node]').count()).toBe(14);
  expect(await page.getByText('A support answer follows its refund policy and was looked up, not guessed.').count()).toBe(1);
  expect(await graph.locator('[data-stage]').allTextContents()).toEqual(['Inputs', 'Steps', 'After all steps']);
  expect(await graph.locator('[data-edge]').count()).toBe(13);
  expect(await node('step:refund-rules').textContent()).toContain('needs policy-text');
  expect(await graph.textContent()).not.toMatch(/passed|failed|running/i);
  // The page itself shows no result; its one link to them opens Verify filtered to this exact Verifier version.
  expect(await page.getByRole('link', { name: 'Results in Verify' }).getAttribute('href')).toMatch(/\/verify\?where=.*verifier_card_uid/);

  // Selecting a step shows its declaration; arrow keys move the selection and focus with it.
  await node('step:refund-rules').click();
  expect(await selected()).toEqual(['step:refund-rules']);
  await expect.poll(() => page.getByRole('region', { name: 'Step refund-rules', exact: true }).textContent()).toContain('timeout_seconds: 5');
  await page.keyboard.press('ArrowRight');
  expect(await selected()).toEqual(['req:follows-policy']);
  expect(await page.evaluate(() => document.activeElement?.getAttribute('data-node'))).toBe('req:follows-policy');
  await expect.poll(() => page.getByRole('region', { name: 'Requirement follows-policy', exact: true }).textContent()).toContain('judged after every step');
  await page.keyboard.press('ArrowLeft');
  expect((await selected())[0]).toMatch(/^(step|req):(refund-rules|limit-found|answer-facts)$/);

  // Nothing is authored: a drag across a node moves no node relative to another, and Delete removes none.
  const gap = async () => ((await node('step:refund-rules').boundingBox())?.x ?? NaN) - ((await node('input:answer').boundingBox())?.x ?? NaN);
  const before = await gap();
  const box = await node('step:refund-rules').boundingBox();
  if (!box) throw new Error('the step has no box');
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 90, box.y + box.height / 2 + 30, { steps: 6 });
  await page.mouse.up();
  await page.keyboard.press('Delete');
  expect(await gap()).toBeCloseTo(before, 0);
  expect(await graph.locator('[data-node]').count()).toBe(14);
  expect(await graph.locator('[data-edge]').count()).toBe(13);

  // Zoom is a view control only.
  const zoom = graph.locator('[data-zoom]');
  await graph.getByRole('button', { name: 'Fit' }).click();
  const fitted = await zoom.textContent();
  await graph.getByRole('button', { name: 'Zoom in' }).click();
  expect(await zoom.textContent()).not.toBe(fitted);
  await graph.getByRole('button', { name: '100%' }).click();
  expect(await zoom.textContent()).toBe('100%');
  await graph.getByRole('button', { name: 'Fit' }).click();
  expect(await zoom.textContent()).toBe(fitted);

  // A phone-width screen keeps every node; the graph pans instead of shrinking below its minimum.
  await page.setViewportSize({ width: 390, height: 800 });
  await page.reload();
  await expect.poll(() => graph.locator('[data-node]').count()).toBe(14);
  expect(await zoom.textContent()).toBe('60%');
  await node('req:follows-policy').focus();
  await page.keyboard.press('Enter');
  expect(await selected()).toEqual(['req:follows-policy']);
  await page.setViewportSize({ width: 1280, height: 720 });

  // Where it runs: the Service's own binding, which opens that exact Service.
  const runs = page.getByRole('region', { name: 'Runs on', exact: true });
  await expect.poll(() => runs.textContent()).toContain('the whole Card · explicit invocation only');
  await page.getByRole('navigation', { name: `${review.name} sections` }).getByRole('button', { name: 'YAML' }).click();
  await expect.poll(() => page.getByRole('region', { name: 'YAML', exact: true }).textContent()).toContain('kind: Verifier');
  await page.getByRole('navigation', { name: `${review.name} sections` }).getByRole('button', { name: 'Graph' }).click();
  await runs.getByRole('button', { name: 'policy-desk@1.0.0' }).click();
  await page.waitForURL((url) => url.searchParams.get('kind') === 'Service' && url.searchParams.get('name') === 'policy-desk');

  // An older version says so and changes nothing; choosing the newest is exact and drops the note.
  await page.goto(`${detail}?kind=Verifier&name=${older.name}&version=1.0.0`);
  await expect.poll(() => page.getByText(/You are viewing/).textContent()).toContain('the newest registered version is 1.1.0');
  await expect.poll(() => graph.locator('[data-node]').count()).toBe(2);
  expect(await runs.textContent()).toContain('No binding runs this exact version.');
  await page.getByLabel('Version').selectOption('1.1.0');
  await expect.poll(() => new URL(page.url()).searchParams.get('version')).toBe('1.1.0');
  await expect.poll(() => page.getByText(/You are viewing/).count()).toBe(0);

  // A missing version is not found and never another version; a principal holding no role is denied.
  expect(await serverHtml(page, `${detail}?kind=Verifier&name=${review.name}&version=9.9.9`)).toContain('WYRD_REGISTRY_404_CARD_NOT_FOUND');
  const viewer = await signIn(ui.browser, ui.base, TENANT, ui.readerKey);
  expect(await serverHtml(viewer, `${detail}?kind=Verifier&name=${review.name}&version=${review.version}`)).toContain('WYRD_PERMISSION_403_DENIED_RBAC');
});

test('an exact execution page keeps its selection through refresh, downloads a retained file and cancels once', async ({ server, ui }) => {
  // The fixture tenant's unfinished execution: four retained outputs, no result.
  const tenant = 'test-tenant-1';
  const { executionId } = server.holdPublications('ui-page-held');
  server.flushBifrost();
  const at = (base: string) => `${ui.base}/t/${base}/verify/executions/${executionId}`;
  const page = await signIn(ui.browser, ui.base, tenant, server.apiKey);
  const status = () => page.locator('[data-execution-status]').getAttribute('data-execution-status');
  const query = () => Object.fromEntries(new URL(page.url()).searchParams);
  const capture = (name: string) => captureExecution(page, name);

  await page.goto(at(tenant));
  await expect.poll(status).toBe('running');
  const graph = page.getByRole('region', { name: 'Graph', exact: true });
  await expect.poll(() => graph.locator('[data-node]').count()).toBe(2);
  // Unfinished: recorded state only, and no verdict anywhere on the page.
  expect(await page.locator('[data-verdict]').getAttribute('data-verdict')).toBe('');
  expect(await page.locator('main').textContent()).not.toMatch(/\b(passed|failed)\b/);
  await capture('running');

  // Every retained output, then one exact intermediate publication of one producer attempt; the URL holds the selection.
  await page.getByRole('button', { name: 'All retained outputs', exact: true }).click();
  await expect.poll(query).toEqual({ node: 'outputs' });
  const outputs = page.getByRole('region', { name: 'Retained outputs', exact: true });
  await expect.poll(() => outputs.locator('tbody tr').count()).toBe(4);
  // The page lists the same recorded identities the shared loader reads: status, and each output with its producer.
  const loaded = await loadExecution({ executionId, node: 'outputs' }, { executor: nodeExecutor(WyrdClient.connect()) });
  if (loaded.result.state !== 'populated' || loaded.result.data.publications?.state !== 'populated') throw new Error('the execution and its outputs load');
  expect(await status()).toBe(loaded.result.data.status.status);
  expect(await outputs.locator('tbody tr').evaluateAll((rows) => rows.map((row) => [...row.querySelectorAll('td')].slice(0, 2).map((cell) => cell.textContent?.trim())))).toEqual(
    loaded.result.data.publications.data.items.map((each) => [each.name, each.producer.step_id])
  );
  await outputs.getByRole('button', { name: 'tree', exact: true }).click();
  const selection = { node: 'step:draft', attempt: '1', output: 'tree' };
  await expect.poll(query).toEqual(selection);
  const preview = page.locator('[data-preview="tree"]');
  await expect.poll(() => preview.textContent()).toContain('nested/b.md');
  await capture('publication');

  // Refresh rereads recorded state and moves nothing: same zoom, same node positions, same selection.
  await graph.getByRole('button', { name: 'Zoom in' }).click();
  const zoom = await graph.locator('[data-zoom]').textContent();
  const placed = () => graph.locator('[data-node]').evaluateAll((nodes) => nodes.map((node) => JSON.stringify(node.getBoundingClientRect())));
  const before = await placed();
  await page.getByRole('button', { name: 'Refresh' }).click();
  await expect.poll(() => page.getByRole('button', { name: 'Refresh' }).isEnabled()).toBe(true);
  expect([await graph.locator('[data-zoom]').textContent(), await placed(), query()]).toEqual([zoom, before, selection]);
  // A reload and a copied link restore the same producer attempt and output from the URL alone.
  await page.reload();
  await expect.poll(() => preview.textContent()).toContain('nested/b.md');
  expect(query()).toEqual(selection);
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.getByRole('button', { name: /Copy link/ }).click();
  await expect.poll(() => page.locator('main').textContent()).toContain('Link copied');
  const copied = await page.evaluate(() => navigator.clipboard.readText());
  expect(copied).toBe(page.url());
  const pasted = await page.context().newPage();
  await pasted.goto(copied);
  await expect.poll(() => pasted.locator('[data-preview="tree"]').textContent()).toContain('nested/b.md');
  expect(await pasted.locator('[data-producer]').getAttribute('data-producer')).toBe('attempt 1');
  await pasted.close();

  // A slow answer for an older selection never replaces a newer one: the older read is held until the newer is shown.
  let release = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  let late = 0;
  let delivered = () => {};
  const answered = new Promise<void>((resolve) => (delivered = resolve));
  const older = (url: URL) => url.pathname.endsWith('__data.json') && url.searchParams.get('node') === 'outputs';
  await page.route(older, async (route) => {
    late += 1;
    const response = await route.fetch();
    await held;
    // A navigation the page already replaced may have dropped this request; either way the older answer is finished.
    await route.fulfill({ response }).catch(() => {});
    delivered();
  });
  await page.getByRole('button', { name: 'All retained outputs', exact: true }).click();
  await expect.poll(() => late).toBe(1);
  const newer = graph.locator('[data-node="req:drift"]');
  await newer.click();
  await expect.poll(query).toEqual({ node: 'req:drift' });
  await expect.poll(() => newer.getAttribute('aria-pressed')).toBe('true');
  release();
  await answered;
  await page.evaluate(() => new Promise(requestAnimationFrame));
  expect([query(), await newer.getAttribute('aria-pressed')]).toEqual([{ node: 'req:drift' }, 'true']);
  expect(await page.getByRole('region', { name: 'Retained outputs', exact: true }).count()).toBe(0);
  await page.unroute(older);
  await page.goto(`${at(tenant)}?${new URLSearchParams(selection)}`);
  await expect.poll(() => preview.textContent()).toContain('nested/b.md');

  // A retained file leaves through the host as a download; the page stays where it is.
  const [download] = await Promise.all([page.waitForEvent('download'), preview.locator('tr', { hasText: 'nested/b.md' }).getByRole('button', { name: 'Download' }).click()]);
  expect([download.suggestedFilename(), readFileSync(await download.path(), 'utf8')]).toEqual(['b.md', 'beta']);
  expect(query()).toEqual(selection);

  // A retained text file previews as inert text beside its manifest.
  await page.getByRole('button', { name: 'Preview' }).first().click();
  await expect.poll(query).toEqual({ ...selection, output: 'report' });
  await expect.poll(() => page.locator('[data-preview="report"] [data-text] pre').textContent()).toBe('# Report\n');
  expect(await page.locator('[data-preview="report"] h1').count()).toBe(0);
  await capture('text');
  await page.getByRole('button', { name: 'Preview' }).nth(1).click();
  await expect.poll(query).toEqual(selection);

  // Completed details are not available before a result, and say so.
  await page.getByRole('button', { name: 'Load completed details' }).click();
  await expect.poll(query).toEqual({ ...selection, details: '1' });
  await expect.poll(() => page.locator('[data-part="Completed details"]').textContent()).toContain('Details are recorded when the execution completes');

  // Back returns to the earlier selection with its preview, and Forward to the later one.
  await page.goBack();
  await expect.poll(query).toEqual(selection);
  await expect.poll(() => page.locator('[data-preview="tree"]').count()).toBe(1);
  await page.goForward();
  await expect.poll(query).toEqual({ ...selection, details: '1' });

  // Another tenant's session finds no such execution; a viewer's cancel is refused and nothing changes.
  const other = await signIn(ui.browser, ui.base, TENANT, ui.adminKey);
  expect(await serverHtml(other, at(TENANT))).toContain('WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND');
  const viewer = await signIn(ui.browser, ui.base, tenant, server.bootstrapService(['viewer'], 'ui-page-viewer'));
  await viewer.goto(at(tenant));
  await viewer.getByRole('button', { name: 'Cancel execution' }).click();
  await expect.poll(() => viewer.locator('main').textContent()).toContain('WYRD_PERMISSION_403_DENIED_RBAC');
  expect(await viewer.locator('[data-execution-status]').getAttribute('data-execution-status')).toBe('running');

  // Cancel: one request, then the recorded status; the selection and the retained outputs stay.
  const cancels: string[] = [];
  page.on('request', (request) => request.url().endsWith('/cancel') && cancels.push(request.method()));
  await page.getByRole('button', { name: 'Cancel execution' }).dblclick();
  await expect.poll(status).toBe('cancelled');
  await expect.poll(() => page.locator('main').textContent()).toContain('Recorded status: cancelled');
  expect(cancels).toEqual(['POST']);
  expect(query()).toEqual({ ...selection, details: '1' });
  await expect.poll(() => preview.textContent()).toContain('nested/b.md');
  await capture('cancelled');

  // Reopened after settlement, on a phone-width screen: cancelled, no verdict, the same outputs.
  await page.setViewportSize({ width: 390, height: 800 });
  await page.goto(`${at(tenant)}?node=outputs`);
  await expect.poll(status).toBe('cancelled');
  expect(await outputs.locator('tbody tr').count()).toBe(4);
  expect(await page.locator('[data-verdict]').textContent()).toContain('no verdict recorded');
  expect(await page.getByRole('button', { name: 'Cancel execution' }).count()).toBe(0);
  await capture('settled');
  await page.setViewportSize({ width: 1280, height: 720 });

  // An execution that does not exist is a stable problem, never an empty page.
  expect(await serverHtml(page, `${ui.base}/t/${tenant}/verify/executions/0192f0a0-0000-7000-8000-000000000000`)).toContain('WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND');
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

/** Draw an execution page in both themes and, when a capture directory is set, keep a full-page screenshot of each. */
async function captureExecution(page: Page, name: string) {
  for (const mode of ['light', 'dark'] as const) {
    await page.evaluate((next) => ((document.querySelector('.app-root') as HTMLElement).dataset.mode = next), mode);
    if (process.env.WYRD_UI_CAPTURE_DIR) await page.screenshot({ path: join(process.env.WYRD_UI_CAPTURE_DIR, `execution-${name}-${mode}-${page.viewportSize()?.width}.png`), fullPage: true });
  }
}

/** How many requests the judging provider has received in this file. */
let judgeRequests = 0;
/** A fresh server that runs Verifiers, with the assistant Service registered. */
const judged = assistantTest({ perTest: true, judge: () => ((judgeRequests += 1), judgeReply(AGREES)) });

judged('a completed execution page shows the recorded judgment, loads details only when asked, and stays completed', async ({ server, state }) => {
  // A real Run verification that fails its assertion.
  const submitted = await state.run('agent').observe.submit('answer-check', { answer: 'no' });
  if (submitted.kind !== 'queued') throw new Error('answer-check queues an execution');
  await submitted.execution.wait({ timeoutMs: 60_000 });
  server.flushBifrost();
  const executionId = submitted.execution.id;
  const tenant = 'test-tenant-1';
  const judgedBefore = judgeRequests;
  buildUi();
  const bff = await startBff(server.baseUrl);
  const browser = await chromium.launch();
  try {
    const page = await signIn(browser, bff.url, tenant, server.apiKey);
    const url = `${bff.url}/t/${tenant}/verify/executions/${executionId}`;
    const query = () => Object.fromEntries(new URL(page.url()).searchParams);
    await page.goto(url);

    // Lifecycle and verdict are separate facts, and a settled execution offers no cancel.
    await expect.poll(() => page.locator('[data-execution-status]').getAttribute('data-execution-status')).toBe('completed');
    expect(await page.locator('[data-verdict]').getAttribute('data-verdict')).toBe('failed');
    expect(await page.getByRole('button', { name: 'Cancel execution' }).count()).toBe(0);

    // The page shows the identities and judgment the shared loader reads for the same execution.
    const loaded = await loadExecution({ executionId }, { executor: nodeExecutor(WyrdClient.connect()) });
    if (loaded.result.state !== 'populated' || loaded.result.data.definition.state !== 'populated') throw new Error('the execution and its Verifier load');
    const { status, definition } = loaded.result.data;
    const judgment = status.result?.outcome.requirements.find((each) => each.id === 'says-yes');
    expect(loaded.input.node).toBe('req:says-yes');
    const main = page.locator('main');
    expect(await main.textContent()).toContain(executionId);
    expect(await main.textContent()).toContain(`${definition.data.ref.name}@${definition.data.ref.version}`);
    const requirement = page.getByRole('region', { name: 'Requirement says-yes', exact: true });
    expect(await requirement.locator('[data-outcome]').getAttribute('data-outcome')).toBe(judgment?.outcome);
    expect(await requirement.textContent()).toContain(judgment?.reason);

    // Completed details are read only when asked for; the compact judgment was readable before them.
    expect(await page.locator('[data-details]').count()).toBe(0);
    // While the details read is slow, the compact judgment and the graph stay readable and no details are drawn.
    let release = () => {};
    const slow = new Promise<void>((resolve) => (release = resolve));
    let waiting = 0;
    const details = (target: URL) => target.pathname.endsWith('__data.json') && target.searchParams.get('details') === '1';
    await page.route(details, async (route) => {
      waiting += 1;
      await slow;
      await route.continue();
    });
    await page.getByRole('button', { name: 'Load completed details' }).click();
    await expect.poll(() => waiting).toBe(1);
    expect(await page.locator('[data-details]').count()).toBe(0);
    expect(await requirement.locator('[data-outcome]').getAttribute('data-outcome')).toBe(judgment?.outcome);
    expect(await requirement.textContent()).toContain(judgment?.reason);
    expect(await page.getByRole('region', { name: 'Graph', exact: true }).locator('[data-node]').count()).toBeGreaterThan(0);
    release();
    await expect.poll(query).toMatchObject({ details: '1' });
    await expect.poll(() => page.locator('[data-details]').count()).toBeGreaterThan(0);
    await page.unroute(details);
    expect(await requirement.locator('[data-outcome]').getAttribute('data-outcome')).toBe('failed');

    // A cancel that arrives after completion changes nothing: the terminal state and its judgment stand on reopen.
    const late = await page.request.post(`${url}/cancel`, { headers: { origin: bff.url } });
    expect(await late.json()).toEqual({ execution_id: executionId, status: 'completed' });
    await page.goto(url);
    await expect.poll(() => page.locator('[data-execution-status]').getAttribute('data-execution-status')).toBe('completed');
    expect(await page.locator('[data-verdict]').getAttribute('data-verdict')).toBe('failed');

    // Inspecting created no work: the tenant still holds exactly the one execution it submitted.
    server.flushBifrost();
    const [executions] = await nodeExecutor(WyrdClient.connect()).run([{ op: 'bifrost.query', sql: 'SELECT DISTINCT execution_id FROM bifrost.verification.execution_inputs', params: [], max_rows: 10 }]);
    expect(executions).toMatchObject({ ok: true, value: { rows: [[executionId]] } });
    // Nor did it ask a provider anything: the judging provider received no request while the page was inspected.
    expect(judgeRequests).toBe(judgedBefore);
  } finally {
    await browser.close();
    bff.stop();
  }
});

judged('session, drift and cited judgments open as pages: members in recorded order, selection kept on return, declared baseline, and refused context said in place', async ({ server, assistant, state }) => {
  const cards = await Cards.connect();
  const token = await WyrdClient.connect().accessToken();
  /** The one binding of `owner` whose status carries `field`: a schedule or a session end. */
  const bindingWith = async (owner: Exclude<Parameters<Cards['get']>[0], string>, field: string) => {
    for (const id of (await cards.get(owner)).status?.verification?.binding_ids ?? []) {
      const response = await fetch(`${server.baseUrl}/v1/verification/bindings/${id}`, { headers: { 'x-wyrd-access-token': `Bearer ${token}` } });
      if (((await response.json()) as Record<string, unknown>)[field]) return id;
    }
    throw new Error(`${owner.name} has no binding with ${field}`);
  };
  // A judged answer that cites its input.
  const submitted = await state.run('agent').observe.submit('answer-judged', { answer: 'yes' });
  if (submitted.kind !== 'queued') throw new Error('answer-judged queues an execution');
  await submitted.execution.wait({ timeoutMs: 60_000 });
  // A scheduled drift window far from its fitted baseline.
  const latency = (await cards.get({ kind: 'Verifier', name: 'latency-drift', version: '1.0.0' })).metadata.uid as string;
  server.waitForBaseline(latency, 90_000);
  await state.startBifrost();
  for (let sample = 0; sample < 100; sample += 1) await state.run('model').observe.submit('latency-drift', { latency: 99 });
  await state.shutdown();
  server.flushBifrost();
  const schedule = await bindingWith(assistant.service, 'next_run_at');
  server.makeBindingDue(schedule);
  const drifted = (await scheduledResult(server, schedule, 90_000)).execution_id;
  // A session of three turns, judged together when it ends.
  const support = await cards.declareFromPath(fixture('cards/verify_in_real_time/support.yaml'));
  const bundle = mkdtempSync(join(tmpdir(), 'wyrd-ui-page-session-'));
  await cards.pull(support.root, bundle);
  const desk = WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential: (await runtimeAccount({ kind: 'Service', name: support.root.name })).key }) });
  await desk.startBifrost();
  for (const turn of [1, 2, 3]) await desk.run('agent').observe.verify('turn-check', { turn, answer: `answer ${turn}` }, { positional: true, group_id: 'ui-page-conversation' });
  await desk.shutdown();
  const session = await groupExecution(server, await bindingWith(declared(support), 'group_end'), 90_000);
  server.flushBifrost();
  const members = session.group?.sources.turns.execution_ids ?? [];
  expect(members).toHaveLength(3);

  const tenant = 'test-tenant-1';
  buildUi();
  const bff = await startBff(server.baseUrl);
  const browser = await chromium.launch();
  try {
    const page = await signIn(browser, bff.url, tenant, server.apiKey);
    const at = (id: string, search = '') => `${bff.url}/t/${tenant}/verify/executions/${id}${search}`;
    const path = () => new URL(page.url()).pathname + new URL(page.url()).search;

    // Session: the source lists its pinned members in recorded order.
    await page.goto(at(session.execution_id, '?node=input%3Aturns'));
    const listed = page.getByRole('list', { name: 'Members of turns' }).getByRole('button');
    await expect.poll(() => listed.allTextContents()).toEqual(members);
    expect(await page.locator('[data-verdict]').getAttribute('data-verdict')).toBe('passed');
    await captureExecution(page, 'session');
    // Opening a member is its own exact page; Back returns to the session with the same source selected and the same members.
    await listed.nth(1).click();
    await expect.poll(path).toBe(new URL(at(members[1])).pathname);
    await expect.poll(() => page.locator('main').textContent()).toContain(members[1]);
    await page.goBack();
    await expect.poll(path).toBe(new URL(at(session.execution_id)).pathname + '?node=input%3Aturns');
    await expect.poll(() => listed.allTextContents()).toEqual(members);
    expect(await page.locator('[data-node="input:turns"]').getAttribute('aria-pressed')).toBe('true');

    // Drift: the recorded verdict, window and features, with the baseline the exact Verifier declares.
    await page.goto(at(drifted));
    const drift = page.locator('[data-drift]');
    await expect.poll(() => drift.count()).toBe(1);
    expect(await drift.textContent()).toContain('Window');
    expect(await drift.locator('[data-declared]').textContent()).toContain('Declared baseline and condition');
    expect(await page.getByRole('region', { name: 'Drift features' }).locator('tbody tr').first().textContent()).toContain('latency');
    expect(await page.locator('[data-verdict]').getAttribute('data-verdict')).toBe('failed');
    await captureExecution(page, 'drift');
    await page.setViewportSize({ width: 390, height: 800 });
    await captureExecution(page, 'drift');
    await page.setViewportSize({ width: 1280, height: 720 });

    // A cited judgment names the slot and position it cites.
    await page.goto(at(submitted.execution.id, '?node=req%3Aagrees'));
    const agrees = page.getByRole('region', { name: 'Requirement agrees', exact: true });
    await expect.poll(() => agrees.locator('[data-outcome]').getAttribute('data-outcome')).toBe('passed');
    expect(await agrees.locator('li .mono').allTextContents()).toEqual(['answer']);
    await captureExecution(page, 'judged');

    // A principal that may read executions but not query evidence sees the drift judgment, and its window and history as refused.
    const narrow = await signIn(browser, bff.url, tenant, server.scopedApiKey('ui_page_narrow', ['verifier:read']));
    await narrow.goto(at(drifted));
    await expect.poll(() => narrow.locator('[data-drift] [data-part="Window and history"]').textContent()).toContain('WYRD_PERMISSION_403_DENIED_RBAC');
    expect(await narrow.getByRole('region', { name: 'Drift features' }).locator('tbody tr').count()).toBeGreaterThan(0);
    await captureExecution(narrow, 'drift-refused');
  } finally {
    await browser.close();
    bff.stop();
  }
});

judged('the Verify dashboard filters, groups, brushes, pages, opens an execution and returns to the same list', async ({ server, assistant, state }) => {
  const cards = await Cards.connect();
  const submit = async (verifier: string, answer: string) => {
    const submitted = await state.run('agent').observe.submit(verifier, { answer });
    if (submitted.kind !== 'queued') throw new Error(`${verifier} queues an execution`);
    await submitted.execution.wait({ timeoutMs: 60_000 });
    return submitted.execution.id;
  };
  // One failed and one passed Run execution, one still running, and 53 direct turns: three of one conversation, fifty of none.
  const refused = await submit('answer-check', 'no');
  await submit('answer-judged', 'yes');
  const support = await cards.declareFromPath(fixture('cards/verify_in_real_time/support.yaml'));
  const bundle = mkdtempSync(join(tmpdir(), 'wyrd-ui-verify-page-'));
  await cards.pull(support.root, bundle);
  const { key } = await runtimeAccount({ kind: 'Service', name: support.root.name });
  const desk = WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential: key }) });
  await desk.startBifrost();
  for (let turn = 1; turn <= 53; turn += 1)
    await desk.run('agent').observe.verify('turn-check', { turn, answer: `answer ${turn}` }, { positional: true, ...(turn <= 3 ? { group_id: 'ui-verify-conversation' } : {}) });
  await desk.shutdown();
  // A hosted Verifier, whose step can be held waiting for memory.
  const capacity = await cards.declareFromPath(fixture('cards/verification_capacity/capacity-desk.yaml'));
  const capacityBundle = mkdtempSync(join(tmpdir(), 'wyrd-ui-verify-capacity-'));
  await cards.pull(capacity.root, capacityBundle);
  const capacityRoot = declared(capacity);
  const counter = WyrdState.fromPath(capacityBundle, { client: WyrdClient.connect({ credential: (await runtimeAccount({ kind: 'Service', name: capacityRoot.name })).key, workloadUid: capacityRoot.uid }) });
  await counter.startBifrost();
  // The ended conversation is judged once as its own group execution: 56 accepted executions so far.
  await vi.waitFor(
    async () => {
      server.flushBifrost();
      const [read] = await nodeExecutor(WyrdClient.connect()).run([{ op: 'bifrost.query', sql: 'SELECT count(DISTINCT execution_id) FROM bifrost.verification.execution_inputs', params: [], max_rows: 1 }]);
      expect(read.ok && Number(read.value.rows[0][0])).toBe(56);
    },
    { timeout: 90_000, interval: 1_000 }
  );

  const tenant = 'test-tenant-1';
  buildUi();
  const bff = await startBff(server.baseUrl);
  const browser = await chromium.launch();
  let memory: { release(): void } | undefined;
  try {
    const page = await signIn(browser, bff.url, tenant, server.apiKey);
    const list = `${bff.url}/t/${tenant}/verify`;
    const query = () => Object.fromEntries(new URL(page.url()).searchParams);
    const rows = page.locator('[aria-label="executions rows"] tbody tr:not(.group)');
    const rail = page.getByRole('navigation', { name: 'Filter executions' });
    const where = page.getByLabel('WHERE: read-only projection of the active filters');

    // The 57th execution: under contended memory each claim returns it to the queue, so it stays queued or pending with no result.
    memory = server.occupyHostedMemory(64 << 20);
    const claimed = await counter.run('agent').observe.submit('adds-one', { answer: 2 });
    if (claimed.kind !== 'queued') throw new Error('adds-one queues an execution');
    const pending = claimed.execution.id;
    server.flushBifrost();

    // Verify is a destination of the tenant shell; it opens on the newest page of every accepted execution.
    await page.getByRole('navigation', { name: 'Destinations' }).getByRole('link', { name: 'Verify' }).click();
    await page.waitForURL(list);
    await expect.poll(() => rows.count()).toBe(50);
    expect(await where.textContent()).toContain('select facet values to filter');
    // The charts stream in after the list.
    for (const title of ['Executions', 'By Verifier']) await expect.poll(() => page.getByRole('region', { name: title }).locator('svg.plot').count()).toBe(1);
    // The Verifier is named from its Card; the pending execution is counted under its recorded status.
    await expect.poll(() => rail.getByRole('button', { name: /turn-check@1\.0\.0/ }).count()).toBe(1);
    const unfinished = rail.getByRole('button', { name: /^(queued|pending) 1$/ });
    await expect.poll(() => unfinished.count()).toBe(1);
    await captureExecution(page, 'verify-dashboard');

    // The unfinished execution is found by its recorded status, and its own page shows it unfinished.
    await unfinished.click();
    await expect.poll(() => rows.count()).toBe(1);
    await expect.poll(() => rows.first().textContent()).toContain('none');
    await rows.first().click();
    await page.waitForURL(`${list}/executions/${pending}`);
    expect(['queued', 'pending']).toContain(await page.locator('[data-execution-status]').getAttribute('data-execution-status'));
    await page.goBack();
    await expect.poll(() => rows.count()).toBe(1);
    await page.getByRole('link', { name: 'Clear' }).click();
    await page.waitForURL(list);
    await expect.poll(() => rows.count()).toBe(50);

    // Show more keeps the population: the list grows to hold the rest, and the address holds how many it shows.
    await expect.poll(() => page.getByText('Showing 50 of 57 · newest first').count()).toBe(1);
    await page.getByRole('button', { name: 'Show 7 more' }).click();
    await expect.poll(query).toMatchObject({ shown: '100' });
    await expect.poll(() => rows.count()).toBe(57);
    await expect.poll(() => page.getByRole('button', { name: /Show \d+ more/ }).count()).toBe(0);

    // Open a row and come back: the same rows of the same list, at the same scroll position.
    await page.setViewportSize({ width: 1280, height: 420 });
    await rows.last().scrollIntoViewIfNeeded();
    const before = { url: page.url(), first: await rows.first().textContent(), scroll: await page.evaluate(() => window.scrollY) };
    expect(before.scroll).toBeGreaterThan(0);
    await rows.last().click();
    await page.waitForURL(/\/verify\/executions\//);
    await expect.poll(() => page.locator('[data-execution-status]').getAttribute('data-execution-status')).toBe('completed');
    await page.goBack();
    await page.waitForURL(before.url);
    await expect.poll(() => rows.count()).toBe(57);
    // Card names stream in after the rows.
    await expect.poll(() => rows.first().textContent()).toBe(before.first);
    await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(before.scroll);
    await page.setViewportSize({ width: 1280, height: 720 });

    // Status and verdict filters combine; the WHERE box projects them and the list starts from its first rows.
    await rail.getByRole('button', { name: /failed/ }).click();
    await expect.poll(() => rows.count()).toBe(1);
    expect(query()).not.toHaveProperty('shown');
    await rail.getByRole('button', { name: /completed/ }).click();
    await expect.poll(async () => (await where.textContent())?.trim()).toBe("verdict = 'failed' AND status = 'completed'");
    await expect.poll(() => rows.count()).toBe(1);
    await expect.poll(() => rows.first().textContent()).toContain('answer-check');
    // The failed execution opens as its exact page; Back restores both filters.
    await rows.first().click();
    await page.waitForURL(`${list}/executions/${refused}`);
    await expect.poll(() => page.locator('[data-verdict]').getAttribute('data-verdict')).toBe('failed');
    await page.goBack();
    await expect.poll(async () => (await where.textContent())?.trim()).toBe("verdict = 'failed' AND status = 'completed'");
    await page.getByRole('link', { name: 'Clear' }).click();
    await page.waitForURL(list);
    await expect.poll(() => rows.count()).toBe(50);

    // The same investigation by keyboard: a facet value filters on Enter, the focused row opens the execution on Enter, and Back returns.
    await rail.getByRole('button', { name: /failed/ }).focus();
    await page.keyboard.press('Enter');
    await expect.poll(async () => (await where.textContent())?.trim()).toBe("verdict = 'failed'");
    await expect.poll(() => rows.count()).toBe(1);
    await rows.first().focus();
    await page.keyboard.press('Enter');
    await page.waitForURL(`${list}/executions/${refused}`);
    await page.goBack();
    await expect.poll(async () => (await where.textContent())?.trim()).toBe("verdict = 'failed'");

    // On a phone-width screen the page does not scroll sideways; the table scrolls inside its own region, and a facet still filters.
    await page.setViewportSize({ width: 390, height: 844 });
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    await rail.getByRole('button', { name: /completed/ }).click();
    await expect.poll(async () => (await where.textContent())?.trim()).toBe("verdict = 'failed' AND status = 'completed'");
    await page.setViewportSize({ width: 1280, height: 720 });
    await page.getByRole('link', { name: 'Clear' }).click();
    await page.waitForURL(list);
    await expect.poll(() => rows.count()).toBe(50);

    // Grouping arranges the same rows under their session; it adds no row and no judgment.
    await page.getByRole('group', { name: 'Group by' }).getByRole('button', { name: 'session' }).click();
    await expect.poll(query).toMatchObject({ group: 'group_id' });
    await expect.poll(() => page.locator('[aria-label="executions rows"] tr.group').count()).toBeGreaterThan(0);
    await expect.poll(() => rows.count()).toBe(50);

    // Either chart brushes the range every panel reads; Reset zoom returns to the preset.
    for (const title of ['Executions', 'By Verifier']) {
      const box = await page.getByRole('region', { name: title }).locator('svg.plot').boundingBox();
      if (!box) throw new Error(`${title} draws no plot to brush`);
      await page.mouse.move(box.x + box.width * 0.5, box.y + box.height / 2);
      await page.mouse.down();
      await page.mouse.move(box.x + box.width * 0.99, box.y + box.height / 2, { steps: 5 });
      await page.mouse.up();
      await expect.poll(() => 'from' in query() && 'to' in query()).toBe(true);
      // The brushed window is the applied range of the whole dashboard, and the grouping is kept.
      const { from, to, group } = query();
      expect([Date.parse(to) > Date.parse(from), group]).toEqual([true, 'group_id']);
      await expect.poll(() => page.locator('[data-panel="charts"] .zoomed').textContent()).toBe(`${from.slice(0, 19)}Z → ${to.slice(0, 19)}Z`);
      await page.getByRole('button', { name: 'Reset zoom' }).click();
      await expect.poll(() => 'from' in query()).toBe(false);
      await expect.poll(() => rows.count()).toBe(50);
    }

    // A brush over the early part of the day, before any execution was accepted, empties the list; Reset zoom brings the rows back.
    const early = await page.getByRole('region', { name: 'Executions' }).locator('svg.plot').boundingBox();
    if (!early) throw new Error('Executions draws no plot to brush');
    await page.mouse.move(early.x + early.width * 0.05, early.y + early.height / 2);
    await page.mouse.down();
    await page.mouse.move(early.x + early.width * 0.3, early.y + early.height / 2, { steps: 5 });
    await page.mouse.up();
    await expect.poll(() => 'from' in query()).toBe(true);
    await expect.poll(() => rows.count()).toBe(0);
    await page.getByRole('button', { name: 'Reset zoom' }).click();
    await expect.poll(() => 'from' in query()).toBe(false);
    await expect.poll(() => rows.count()).toBe(50);

    // A slow load that a newer choice supersedes never replaces it: the newer selection is what stays shown.
    let release = () => {};
    const slow = new Promise<void>((resolve) => (release = resolve));
    let waiting = 0;
    const filtered = (target: URL) => target.pathname.endsWith('__data.json') && (target.searchParams.get('where') ?? '').includes('failed');
    await page.route(filtered, async (route) => {
      waiting += 1;
      await slow;
      // The browser has already abandoned the superseded request by the time it is released.
      await route.continue().catch(() => {});
    });
    await rail.getByRole('button', { name: /failed/ }).click();
    await expect.poll(() => waiting).toBe(1);
    await page.getByRole('group', { name: 'Group by' }).getByRole('button', { name: 'Service' }).click();
    await expect.poll(query).toEqual({ group: 'service_name' });
    release();
    await page.unroute(filtered);
    await expect.poll(() => page.locator('[aria-label="executions rows"] tr.group').first().textContent()).toMatch(/executions? · /);
    expect(query()).toEqual({ group: 'service_name' });
    await expect.poll(() => rows.count()).toBe(50);
    expect(await where.textContent()).toContain('select facet values to filter');

    // The narrow layout keeps every control and the table reachable.
    await page.setViewportSize({ width: 390, height: 844 });
    await expect.poll(() => rows.count()).toBe(50);
    expect(await page.getByRole('button', { name: 'Paused' }).isVisible()).toBe(true);
    await captureExecution(page, 'verify-dashboard');
    await page.setViewportSize({ width: 1280, height: 720 });

    // Results in Verify on the exact Verifier Card page opens the list for that Verifier version only.
    await page.goto(`${bff.url}/t/${tenant}/cards/detail?kind=Verifier&name=answer-check&version=1.0.0`);
    await page.getByRole('link', { name: 'Results in Verify' }).click();
    await expect.poll(async () => (await where.textContent())?.trim()).toBe(`verifier_card_uid = '${assistant.answerCheck.uid}'`);
    await expect.poll(() => rows.count()).toBe(1);
    await expect.poll(() => rows.first().textContent()).toContain('answer-check');
  } finally {
    await browser.close();
    bff.stop();
    memory?.release();
    await counter.shutdown();
  }
});

/**
 * The environment of a development command with its config home at `config`.
 * The development environment starts its own server, so this file's test
 * server address and key, stubbed on the process by earlier tests, are left out.
 * So is the test runner's own marker: with it set, the development UI would
 * resolve Svelte as a test does and share this run's dependency cache.
 */
function devEnv(config: string) {
  const { WYRD_SERVER_URL: _url, WYRD_GRPC_URL: _grpc, WYRD_API_KEY: _key, VITEST: _vitest, ...ambient } = process.env;
  return { ...ambient, WYRD_CONFIG_HOME: config };
}

/** Starts `mise run dev:full` in its own process group, writing to a log file in `config`. */
function devFull(config: string, env: Record<string, string> = {}) {
  // Output goes to a file, never a pipe this process must drain: a blocked test would stall every service writing to it.
  const log = join(config, `dev-full-${Date.now()}.log`);
  const out = openSync(log, 'w');
  const child = spawn('mise', ['run', 'dev:full', '--requests', '20', '--concurrency', '2'], {
    cwd: REPO,
    env: { ...devEnv(config), ...env },
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
    // Pull request review registration runs beside the app and batch without holding either up.
    expect(output()).toContain('review       registers bohmian-ai/wyrd-example-py once GitHub is connected');
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
    execFileSync('mise', ['run', 'dev:batch', '--requests', '4', '--concurrency', '2'], { cwd: REPO, env: devEnv(config), stdio: 'inherit' });
    // Each reload is read only once its overview has loaded.
    await vi.waitFor(
      async () => {
        await page.reload();
        await expect.poll(() => traceTotal(page), { timeout: 30_000 }).not.toBeNaN();
        expect(await traceTotal(page)).toBeGreaterThan(before);
      },
      { timeout: 180_000, interval: 2_000 }
    );

    // Setup seeded the Cards board through public registration: the rail opens it, a Prompt compares its versions and lists its Agent.
    await page.getByRole('navigation', { name: 'Destinations' }).getByRole('link', { name: 'Cards' }).click();
    await page.waitForURL('http://127.0.0.1:3000/t/dev/cards');
    const cards = page.getByRole('region', { name: 'Cards', exact: true });
    await expect.poll(() => cards.locator('.row', { hasText: 'support-reply' }).count()).toBe(2);
    expect(await cards.locator('.row', { hasText: 'reply-agent' }).count()).toBe(1);
    await cards.getByRole('button', { name: 'Open Prompt/support-reply@1.1.0' }).click();
    await page.waitForURL((url) => url.pathname === '/t/dev/cards/detail' && url.searchParams.get('version') === '1.1.0');
    await expect.poll(() => page.getByRole('region', { name: 'Used by', exact: true }).textContent()).toContain('reply-agent@1.0.0');
    expect(await page.getByRole('note', { name: 'Changes', exact: true }).textContent()).toContain('Changed from v1.0.0');
    await page.goto('http://127.0.0.1:3000/t/dev/cards/detail?kind=Model&name=support-model&version=1.0.0&tab=files');
    await expect.poll(() => page.getByRole('region', { name: 'Files', exact: true }).textContent()).toContain('support-model.bin');

    // The running app answers one question and queues a real `follows-policy` check; that execution opens by its exact URL with recorded state, its Verifier and its graph.
    const asked = await fetch('http://127.0.0.1:8000/ask', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ question: 'When will my order ship?', order_id: 'A-1001' }) });
    const reply = (await asked.json()) as { submit?: { execution_id: string } };
    expect(reply.submit?.execution_id, `the app answered ${asked.status}`).toEqual(expect.any(String));
    const reviewed = reply.submit?.execution_id as string;
    await page.goto(`http://127.0.0.1:3000/t/dev/verify/executions/${reviewed}`);
    await expect.poll(() => page.locator('[data-execution-status]').getAttribute('data-execution-status'), { timeout: 60_000 }).toMatch(/^(queued|running|completed)$/);
    expect(await page.locator('main').textContent()).toContain(reviewed);
    // Its Verifier is named once retained metadata is visible, so the page is reread until then.
    const reread = async () => {
      await page.reload();
      return page.locator('main').textContent();
    };
    await expect.poll(reread, { timeout: 120_000, interval: 3_000 }).toContain('follows-policy@');
    await expect.poll(() => page.getByRole('region', { name: 'Graph', exact: true }).locator('[data-node]').count()).toBeGreaterThan(0);

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

/** A fresh deployment whose GitHub App is the backend test owner's loopback GitHub. */
const connected = serverTest({ githubApp: true, perTest: true });

/** Draw a page in both themes at desktop and phone width and, when a capture directory is set, keep a full-page screenshot of each. */
async function captureRepositories(page: Page, name: string) {
  for (const width of [1440, 390]) {
    await page.setViewportSize({ width, height: 800 });
    // The page never scrolls sideways; wide tables scroll inside their own region.
    expect(await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth), `${name} at ${width}`).toBeLessThanOrEqual(1);
    for (const mode of ['light', 'dark'] as const) {
      await page.evaluate((next) => ((document.querySelector('.app-root') as HTMLElement).dataset.mode = next), mode);
      if (process.env.WYRD_UI_CAPTURE_DIR) await page.screenshot({ path: join(process.env.WYRD_UI_CAPTURE_DIR, `repositories-${name}-${mode}-${width}.png`), fullPage: true });
    }
  }
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.evaluate(() => ((document.querySelector('.app-root') as HTMLElement).dataset.mode = 'light'));
}

connected('a tenant connects GitHub from Settings, reads its Repository Cards apart from GitHub access, and disconnects without losing them', async ({ server }) => {
  // GitHub: installation 101 selects three repositories and the signed-in GitHub user may install it.
  const github = server.githubApp;
  github.addInstallation(101, [
    { id: 1, fullName: 'acme/api' },
    { id: 2, fullName: 'acme/web' },
    { id: 3, fullName: 'acme/docs' }
  ]);
  github.addUser('octo', [101]);
  github.signIn('octo');
  github.installTarget(101);
  const tenant = server.tenantSlug;
  const adminKey = server.bootstrapService(['admin'], 'repositories_ui_admin');
  const admin = WyrdClient.connect({ credential: adminKey });
  const cards = await Cards.connect({ client: admin });
  const declare = (fullName: string, policy: string) => declarePolicy(cards, github.host, fullName, policy);
  buildUi();
  const bff = await startBff(server.baseUrl);
  const browser = await chromium.launch();
  try {
    const base = `${bff.url}/t/${tenant}`;
    const page = await signIn(browser, bff.url, tenant, adminKey);
    const main = page.locator('main');
    /** The index rows as their four named cells: the first word of each. */
    const rows = () =>
      page
        .getByRole('region', { name: 'Repositories', exact: true })
        .locator('tbody tr')
        .evaluateAll((all) =>
          all.map((row) => {
            const [repository, version, access, verification] = [...row.querySelectorAll('td')].map((cell) => cell.querySelector('.link, .st, .mono')?.textContent?.replace(/^[^\w]+/, '').trim());
            return { repository, version, access, verification };
          })
        );
    const noSecrets = async (url: string) => {
      const html = (await serverHtml(page, url)) + (await page.content());
      for (const secret of github.secrets()) expect(html).not.toContain(secret);
    };

    // From Verify, Repositories is its own workspace beside Cards. Nothing is connected or registered yet.
    await page.goto(`${base}/verify`);
    await page.getByRole('link', { name: 'Repositories', exact: true }).click();
    await page.waitForURL(`${base}/repositories`);
    await expect.poll(() => main.textContent()).toContain('GitHub is not connected.');
    expect(await main.textContent()).toContain('No Repository Card is registered and the GitHub connection reaches no repository.');
    await page.getByRole('button', { name: 'GitHub connection' }).click();
    await page.waitForURL(`${base}/settings/integrations/github`);
    await expect.poll(() => main.textContent()).toContain('Not connected');

    // Wyrd-first: Connect leaves for GitHub, which returns to the server's own setup and callback; Settings then reads the connection back.
    await page.getByRole('button', { name: 'Connect GitHub' }).click();
    await page.waitForURL(/\/v1\/integrations\/github\/callback/);
    expect(JSON.parse((await page.locator('body').textContent()) ?? '')).toMatchObject({ state: 'active', installation_id: 101 });
    await page.goto(`${base}/settings/integrations/github`);
    await expect.poll(() => main.textContent()).toContain('Connected');
    const access = page.getByRole('region', { name: 'Repository access', exact: true });
    // Connecting registered nothing: each reachable repository has no Card.
    expect(await access.locator('tbody tr').count()).toBe(3);
    expect(await access.locator('.tag').allTextContents()).toEqual(['no Card', 'no Card', 'no Card']);
    expect(await page.getByRole('link', { name: /Manage access on GitHub/ }).getAttribute('href')).toBe(`https://${github.host}/apps/wyrd-test-app/installations/new`);
    await noSecrets(`${base}/settings/integrations/github`);
    await captureRepositories(page, 'settings-connected');

    // The tenant declares two versions of `api` and an empty policy for `web` through the normal declaration; `docs` stays undeclared.
    await cards.declareFromPath(fixture('cards/verify_in_real_time/answer-check.yaml'));
    const first = await declare('acme/api', 'minimal');
    const second = await declare('acme/api', 'changed');
    await declare('acme/web', 'empty');
    // A pull request of `acme/api` admits one execution per selected binding; this deployment runs no Verifier, so both stay pending.
    github.setPullRequest(1, 7, 'b1', 'a1');
    expect(github.deliverPullRequest(server.baseUrl, 101, 1, 7, 'opened')).toBe(202);

    // The index states registration, access and readiness per repository.
    await page.goto(`${base}/repositories`);
    await expect.poll(rows).toEqual([
      { repository: 'api', version: second.version, access: 'Granted', verification: 'Ready' },
      { repository: 'web', version: expect.any(String), access: 'Granted', verification: 'Not configured' },
      { repository: 'acme/docs', version: '', access: 'Granted', verification: 'Not declared' }
    ]);
    await noSecrets(`${base}/repositories`);
    await captureRepositories(page, 'index');

    // Search and Show live in the URL and travel into the detail page and back.
    await page.getByRole('searchbox', { name: 'Filter repositories' }).fill('acme');
    await page.getByRole('searchbox', { name: 'Filter repositories' }).press('Enter');
    await page.waitForURL(`${base}/repositories?q=acme`);
    await page.getByRole('group', { name: 'Show' }).getByRole('button', { name: 'Ready' }).click();
    await page.waitForURL(`${base}/repositories?q=acme&show=ready`);
    await expect.poll(rows).toEqual([{ repository: 'api', version: second.version, access: 'Granted', verification: 'Ready' }]);
    await page.getByRole('button', { name: 'api', exact: true }).click();
    await page.waitForURL(`${base}/repositories/api?version=${second.version}&back=%3Fq%3Dacme%26show%3Dready`);
    await expect.poll(() => main.textContent()).toContain('✓ selected');
    await captureRepositories(page, 'detail');

    // Viewing the older version changes the URL only; the selected policy is still the server's.
    await page.getByRole('combobox', { name: 'Card version' }).selectOption(first.version);
    await page.waitForURL(new RegExp(`/repositories/api\\?version=${first.version.replaceAll('.', '\\.')}&`));
    await expect.poll(() => main.textContent()).toContain('not selected · viewing only');
    expect((await cards.get({ kind: 'Repository', name: 'api', version: second.version })).status?.repository?.selected).toBe(true);
    await page.getByRole('tab', { name: 'wyrd.toml' }).click();
    await expect.poll(() => new URL(page.url()).searchParams.get('declaration')).toBe('toml');
    expect(await page.getByRole('tabpanel').textContent()).toContain(`# Projection of Repository/api@${first.version}, not the declared file.`);

    // Activity is this Repository's executions: the row opens the exact execution, and Verify opens filtered to the same Card.
    const activity = page.getByRole('region', { name: 'Verification activity', exact: true });
    await expect
      .poll(
        async () => {
          server.flushBifrost();
          await page.reload();
          return activity.locator('tbody tr').count();
        },
        { timeout: 30_000, interval: 1_000 }
      )
      .toBe(2);
    for (const text of await activity.locator('tbody tr').allTextContents()) expect(text).toContain('queued');
    const detailUrl = page.url();
    await activity.locator('tbody tr').first().click();
    await page.waitForURL(/\/verify\/executions\/[^/?]+$/);
    const executionId = decodeURIComponent(new URL(page.url()).pathname.split('/').pop() as string);
    await expect.poll(() => page.locator('[data-execution-status]').getAttribute('data-execution-status')).toBe('pending');
    await page.goto(detailUrl);
    await activity.getByRole('link', { name: 'View all in Verify' }).click();
    await page.waitForURL(/\/verify\?/);
    expect(JSON.parse(new URL(page.url()).searchParams.get('where') ?? '[]')).toEqual([{ column: 'subject_kind', value: 'Repository' }, { column: 'subject_name', value: 'api' }]);
    await expect.poll(() => page.getByRole('region', { name: 'executions rows' }).locator('tbody tr').count()).toBe(2);
    await page.goto(detailUrl);
    await page.getByRole('button', { name: 'Verifier definition' }).click();
    await page.waitForURL(/\/cards\/detail\?kind=Verifier&name=code-review&/);
    await page.goto(detailUrl);

    // The way back restores the index state the reader left.
    await page.getByRole('navigation', { name: 'Breadcrumb' }).getByRole('link', { name: 'Repositories' }).click();
    await page.waitForURL(`${base}/repositories?q=acme&show=ready`);
    await expect.poll(rows).toEqual([{ repository: 'api', version: second.version, access: 'Granted', verification: 'Ready' }]);

    // A reachable repository without a Card explains how to declare one; a declared, unknown or malformed id names nothing.
    await page.getByRole('group', { name: 'Show' }).getByRole('button', { name: 'No Card' }).click();
    await page.getByRole('button', { name: 'acme/docs' }).click();
    await page.waitForURL(new RegExp('/repositories/github/3\\?back='));
    await expect.poll(() => main.textContent()).toContain('wyrd declare wyrd.toml');
    await captureRepositories(page, 'undeclared');
    for (const id of ['1', '999']) {
      const html = await serverHtml(page, `${base}/repositories/github/${id}`);
      expect(html).toContain('WYRD_SPEC_404_NOT_FOUND');
      expect(html).not.toContain('acme/');
    }
    expect(await serverHtml(page, `${base}/repositories/nowhere?version=1.0.0`)).toMatch(/WYRD_[A-Z]+_404_/);

    // A viewer reads the Cards but not the connection: the refusal is shown as such, with no control and no guessed access.
    const viewer = await signIn(browser, bff.url, tenant, server.bootstrapService(['viewer'], 'repositories_ui_viewer'));
    await viewer.goto(`${base}/settings/integrations/github`);
    await expect.poll(() => viewer.locator('main').textContent()).toContain('WYRD_PERMISSION_403_DENIED_RBAC');
    expect(await viewer.locator('main').getByRole('button').count()).toBe(0);
    expect(await viewer.locator('main').textContent()).not.toContain('acme/');
    await viewer.goto(`${base}/repositories`);
    await expect.poll(() => viewer.locator('main').textContent()).toContain('GitHub connection unavailable');
    expect(await viewer.getByRole('button', { name: 'api', exact: true }).count()).toBe(1);
    expect(await viewer.locator('main').textContent()).not.toContain('acme/docs');
    // Its direct requests to the host actions are refused by the server and change nothing.
    for (const method of ['POST', 'DELETE'] as const) {
      const refused = await viewer.request.fetch(`${base}/settings/integrations/github/connection`, { method, headers: { origin: bff.url } });
      expect([refused.status(), ((await refused.json()) as { code: string }).code]).toEqual([403, 'WYRD_PERMISSION_403_DENIED_RBAC']);
    }
    await viewer.context().close();

    // Manage connection leads from the Repository to Settings. Disconnect asks first in a modal dialog; cancelling sends nothing.
    await page.goto(detailUrl);
    await page.getByRole('button', { name: 'Manage connection' }).click();
    await page.waitForURL(`${base}/settings/integrations/github`);
    const sent: string[] = [];
    let release = () => {};
    await page.route(`${base}/settings/integrations/github/connection`, async (route) => {
      if (route.request().method() !== 'DELETE') return route.continue();
      sent.push(route.request().method());
      await new Promise<void>((resolve) => (release = resolve));
      await route.continue();
    });
    const disconnect = page.getByRole('button', { name: 'Disconnect', exact: true });
    await disconnect.click();
    const dialog = page.getByRole('dialog', { name: 'Disconnect GitHub?' });
    expect(await dialog.evaluate((element) => element.matches(':modal'))).toBe(true);
    // The dialog sits in the middle of the viewport.
    const box = (await dialog.boundingBox())!;
    expect(Math.abs(box.x + box.width / 2 - 640)).toBeLessThan(2);
    expect(await dialog.textContent()).toContain('Unfinished executions for those repositories are cancelled.');
    expect(await dialog.textContent()).toContain('Repository Cards and versions are unchanged.');
    // Keyboard: focus starts on Cancel, and Escape closes without disconnecting.
    expect(await page.evaluate(() => document.activeElement?.textContent)).toBe('Cancel');
    await page.keyboard.press('Escape');
    expect(await dialog.count()).toBe(0);
    await disconnect.click();
    await captureRepositories(page, 'disconnect-dialog');
    await dialog.getByRole('button', { name: 'Cancel' }).click();
    expect(sent).toEqual([]);

    // Confirmed: one request while pending, then the page reads the connection back as not connected.
    await disconnect.click();
    await dialog.getByRole('button', { name: 'Disconnect GitHub' }).click();
    await expect.poll(() => disconnect.isDisabled()).toBe(true);
    expect(sent).toEqual(['DELETE']);
    release();
    await expect.poll(() => main.textContent()).toContain('Not connected');
    expect(sent).toEqual(['DELETE']);
    await page.unroute(`${base}/settings/integrations/github/connection`);

    // The Cards, their selected policy and the recorded execution stay; access is gone, nothing new starts, and the unfinished executions were cancelled.
    await page.goto(`${base}/repositories`);
    await expect.poll(rows).toEqual([
      { repository: 'api', version: second.version, access: 'No connection', verification: 'Inactive' },
      { repository: 'web', version: expect.any(String), access: 'No connection', verification: 'Inactive' }
    ]);
    await captureRepositories(page, 'index-disconnected');
    await page.goto(`${base}/verify/executions/${encodeURIComponent(executionId)}`);
    await expect.poll(() => page.locator('[data-execution-status]').getAttribute('data-execution-status'), { timeout: 30_000 }).toBe('cancelled');
    await page.goto(detailUrl);
    await expect.poll(() => main.textContent()).toContain('No connection');
    await expect
      .poll(
        async () => {
          server.flushBifrost();
          await page.reload();
          return (await activity.locator('tbody tr').allTextContents()).map((text) => /cancelled/.test(text));
        },
        { timeout: 30_000, interval: 1_000 }
      )
      .toEqual([true, true]);

    // GitHub-first: starting from GitHub's own install page returns through the same server callback, and the Cards regain access.
    await page.goto(`http://${github.host}/apps/wyrd-test-app/installations/new`);
    await page.waitForURL(/\/v1\/integrations\/github\/callback/);
    expect(JSON.parse((await page.locator('body').textContent()) ?? '')).toMatchObject({ state: 'active', installation_id: 101 });
    await page.goto(`${base}/repositories`);
    await expect.poll(rows).toEqual([
      { repository: 'api', version: second.version, access: 'Granted', verification: 'Ready' },
      { repository: 'web', version: expect.any(String), access: 'Granted', verification: 'Not configured' },
      { repository: 'acme/docs', version: '', access: 'Granted', verification: 'Not declared' }
    ]);
  } finally {
    await browser.close();
    bff.stop();
  }
});
