/**
 * Real-browser Observe journeys. The production BFF build (`node build`) runs
 * in front of a real Wyrd server started by `@wyrd/testing` over repository
 * Postgres; Chromium signs in through the API-key page and investigates
 * evidence exported over stock OTLP. Nothing here mocks the server or the BFF.
 */
import { execFileSync, spawn } from 'node:child_process';
import { closeSync, mkdtempSync, openSync, readFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { chromium, type Browser, type Page } from 'playwright';
import { test as base, expect, vi } from 'vitest';
import { serverTest } from '../../../../../../sdks/wyrd-sdk-ts/wyrd/tests/support/server';
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

/** The built BFF, a browser, and a tenant holding one trace; the default tenant holds another. */
const test = serverTest().extend<{ ui: { base: string; browser: Browser; adminKey: string; readerKey: string; serverUrl: string } }>({
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
        await use({ base: bff.url, browser, adminKey, readerKey, serverUrl: server.baseUrl });
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

test('authenticated logs investigation', async ({ ui }) => {
  const page = await signIn(ui.browser, ui.base, TENANT, ui.adminKey);
  const logs = `${ui.base}/t/${TENANT}/observe?signal=logs&from=${RANGE.from}&to=${RANGE.to}`;
  await page.goto(logs);
  const list = page.locator('[data-panel="list"]');

  // The recorded record of this tenant only, with its recorded severity, and a timeline under the same filters.
  await expect.poll(() => list.textContent(), { timeout: 30_000 }).toContain('search timed out');
  expect(await list.textContent()).toContain('ERROR');
  expect(await list.textContent()).not.toContain('foreign-agent');
  await expect.poll(() => page.locator('[data-panel="overview"]').textContent()).toMatch(/Log records\s*1.*Error logs\s*1/s);

  // Severity facets filter the timeline and the records alike.
  await page.locator('[data-panel="facets"]').getByRole('button', { name: /ERROR/ }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('where')).toBe(JSON.stringify([{ column: 'severity_text', value: 'ERROR' }]));
  await expect.poll(() => list.locator('tbody tr').count()).toBe(1);

  // Wrap, columns and paging are reader controls over the rows already read.
  await list.getByRole('button', { name: 'Wrap lines' }).click();
  expect(await list.getByRole('button', { name: 'Wrap lines' }).getAttribute('aria-pressed')).toBe('true');
  await list.locator('summary', { hasText: 'Columns' }).click();
  await list.locator('label[title="body_text"] input').uncheck();
  expect(await list.locator('th[title="body_text"]').count()).toBe(0);
  await list.locator('label[title="body_text"] input').check();
  expect(await list.getByRole('navigation', { name: 'Pages' }).getByRole('combobox').inputValue()).toBe('50');

  // The record opens by its exact identity, with its body and recorded fields.
  await list.locator('tbody tr').first().click();
  const drawer = page.getByRole('dialog', { name: 'Log record' });
  await expect.poll(() => drawer.textContent(), { timeout: 30_000 }).toContain('search timed out');
  expect(JSON.parse(new URL(page.url()).searchParams.get('log') ?? '{}')).toMatchObject({ wyrd_request_id: expect.any(String) });

  // Open trace leaves Logs in history; Back returns to the same record.
  const opened = page.url();
  await drawer.getByRole('button', { name: /Open trace/ }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('trace')).toBe(TRACE);
  expect(new URL(page.url()).searchParams.get('signal')).toBeNull();
  await expect.poll(() => page.getByRole('dialog', { name: 'Trace' }).getByRole('treeitem').count(), { timeout: 30_000 }).toBe(3);
  await page.goBack();
  await page.waitForURL(opened);
  await expect.poll(() => page.getByRole('dialog', { name: 'Log record' }).textContent()).toContain('search timed out');

  // A detail field filters or excludes into WHERE; excluding the only service leaves a truthful empty list.
  await page.getByRole('dialog', { name: 'Log record' }).getByRole('button', { name: 'Exclude' }).click();
  await expect.poll(() => JSON.parse(new URL(page.url()).searchParams.get('where') ?? '[]')).toContainEqual({ column: 'service_name', value: 'ui-agent', not: true });
  await expect.poll(() => list.textContent()).toContain('No rows in bifrost.logs.records');
  expect(new URL(page.url()).searchParams.has('log')).toBe(false);

  // A principal holding no role sees the Logs panels denied with their stable code.
  const viewer = await signIn(ui.browser, ui.base, TENANT, ui.readerKey);
  await viewer.goto(logs);
  await expect.poll(() => viewer.locator('[data-panel="overview"]').textContent()).toContain('WYRD_PERMISSION_403_DENIED_RBAC');
});

/** Integers past 2^53, which a JSON number cannot carry exactly. */
const UNSAFE = ['9007199254740993', '9007199254740995'] as const;

test('authenticated metrics investigation', async ({ server, ui }) => {
  // One metric name recorded with two meanings, unsafe integers, and a histogram.
  const point = (at: string, value: Record<string, unknown>) => ({ timeUnixNano: nanos(at), ...value });
  const scope = { name: 'wyrd-ui-metrics' };
  const resource = (service: string) => ({ attributes: [{ key: 'service.name', value: { stringValue: service } }] });
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
                  dataPoints: [point('2026-10-19T04:00:00Z', { asInt: UNSAFE[0] }), point('2026-10-19T04:01:00Z', { asInt: UNSAFE[1] })]
                }
              },
              {
                name: 'ui.latency',
                unit: 's',
                histogram: {
                  aggregationTemporality: 1,
                  dataPoints: [point('2026-10-19T04:00:00Z', { count: '3', sum: 1.5, bucketCounts: ['1', '2'], explicitBounds: [1] })]
                }
              }
            ]
          }
        ]
      },
      { resource: resource('ui-worker'), scopeMetrics: [{ scope, metrics: [{ name: 'ui.queue', unit: 'By', gauge: { dataPoints: [point('2026-10-19T04:00:00Z', { asDouble: 2.5 })] } }] }] }
    ]
  });
  server.flushBifrost();

  const page = await signIn(ui.browser, ui.base, TENANT, ui.adminKey);
  await page.goto(`${ui.base}/t/${TENANT}/observe?signal=metrics&from=${RANGE.from}&to=${RANGE.to}`);
  const list = page.locator('[data-panel="list"]');

  // Points list exact integers and recorded structures; no chart until a metric is selected.
  await expect.poll(() => list.textContent(), { timeout: 30_000 }).toContain(UNSAFE[1]);
  expect(await list.textContent()).toContain(UNSAFE[0]);
  expect(await list.textContent()).toContain('histogram');
  expect(await page.locator('[data-panel="series"]').count()).toBe(0);

  // Selecting the metric charts one line per recorded meaning and summarizes each exactly.
  await page.locator('[data-panel="facets"]').getByRole('button', { name: /ui\.queue/ }).click();
  const chart = page.locator('[data-panel="series"]');
  await expect.poll(() => chart.locator('table.legend tbody tr').count(), { timeout: 30_000 }).toBe(2);
  expect(await chart.textContent()).toContain('plotted as double precision');
  const summary = page.locator('[data-panel="summary"]');
  await expect.poll(() => summary.locator('tbody tr').count()).toBe(2);
  const rows = await summary.locator('tbody tr').allTextContents();
  const sum = rows.find((row) => row.includes('sum'));
  expect(sum).toContain('cumulative');
  expect(sum).toContain('monotonic');
  expect(sum).toContain(UNSAFE[1]);
  expect(sum).toContain('9007199254740994');
  expect(rows.find((row) => row.includes('gauge'))).toContain('2.5');

  // Unticking a series hides its line, and grouping by a recorded dimension keeps the meanings apart.
  await chart.getByRole('checkbox').first().uncheck();
  await expect.poll(() => chart.textContent()).toContain('1 shown');
  await page.getByLabel('Group by').selectOption('service_name');
  await expect.poll(() => summary.textContent()).toContain('ui-worker');
  expect(new URL(page.url()).searchParams.get('group')).toBe('service_name');

  // A histogram is listed with its recorded structure and explicitly not plotted.
  await page.getByRole('link', { name: 'Remove metric_name' }).click();
  await page.locator('[data-panel="facets"]').getByRole('button', { name: /ui\.latency/ }).click();
  await expect.poll(() => page.locator('[data-panel="series"]').textContent(), { timeout: 30_000 }).toContain('gauge and sum points only');
  await expect.poll(() => list.textContent()).toContain('[1,2]');
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
  const text = (await page.locator('[data-panel="overview"]').textContent()) ?? '';
  return Number(/Traces\s*(\d[\d,]*)/.exec(text)?.[1].replaceAll(',', '') ?? NaN);
}

base('development environment lifecycle', { timeout: 1_800_000 }, async () => {
  // The launcher's ports are fixed; another service on one would be mistaken for this environment.
  for (const port of [8080, 3000, 8000, 1080]) expect(await answers(port), `port ${port} is already serving; stop that service first`).toBe(false);
  // A config home of its own: startup replaces the dev key there, never in the developer's.
  const config = mkdtempSync(join(tmpdir(), 'wyrd-dev-'));
  // Output goes to a file, never a pipe this process must drain: a blocked test would stall every service writing to it.
  const log = join(config, 'dev-full.log');
  const out = openSync(log, 'w');
  const child = spawn('mise', ['run', 'dev:full', '--requests', '20', '--concurrency', '2'], {
    cwd: REPO,
    env: { ...process.env, WYRD_CONFIG_HOME: config },
    detached: true,
    stdio: ['ignore', out, out]
  });
  closeSync(out);
  const output = () => readFileSync(log, 'utf8');
  const exited = new Promise<number | null>((done) => child.once('exit', (code) => done(code)));
  const browser = await chromium.launch();
  try {
    // One command reaches readiness, runs the batch to terminal evidence and reports usable URLs.
    await vi.waitFor(() => expect(output()).toMatch(/batch {8}(passed|FAILED)/), { timeout: 1_500_000, interval: 1_000 });
    expect(output(), log).toContain('batch        passed');
    expect(output()).toContain('UI           http://127.0.0.1:3000/t/dev/login/api-key');
    for (const port of [8080, 3000, 8000, 1080]) expect(await answers(port), `port ${port}`).toBe(true);
    const key = /api_key = "([^"]+)"/.exec(readFileSync(join(config, 'credentials.toml'), 'utf8'))?.[1];
    expect(key).toBeDefined();

    // Normal browser login reaches the batch's retained traces, logs and metrics.
    const page = await signIn(browser, 'http://127.0.0.1:3000', 'dev', key!);
    const observe = 'http://127.0.0.1:3000/t/dev/observe';
    for (const signal of ['traces', 'logs', 'metrics']) {
      await page.goto(`${observe}?signal=${signal}&last=1h`);
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
  } finally {
    await browser.close();
    // Ctrl-C stops every owned child: the app first, so its telemetry drains.
    if (child.exitCode === null) process.kill(-child.pid!, 'SIGINT');
    await exited;
  }
  for (const port of [8080, 3000, 8000]) expect(await answers(port), `port ${port} after Ctrl-C`).toBe(false);
});
