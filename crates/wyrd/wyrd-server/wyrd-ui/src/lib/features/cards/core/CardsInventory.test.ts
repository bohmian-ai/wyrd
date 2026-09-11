// @vitest-environment jsdom
import { spawn, type ChildProcess } from 'node:child_process';
import { once } from 'node:events';
import { beforeAll, afterAll, expect, test } from 'vitest';
import { WyrdClient } from '$lib/server/wyrd';
import { CARD_KINDS } from './types';

// The Card inventory journey against the real dev server: URL-restored
// filters across every registrable kind, lookup, and truthful alternate
// states (TASK-006 scenario 1).
let server: ChildProcess;
let origin: string;
beforeAll(async () => {
  server = spawn(
    process.execPath,
    ['node_modules/vite/bin/vite.js', '--host', '127.0.0.1', '--port', '0'],
    {
      env: {
        ...process.env,
        WYRD_UI_LOCAL_AUTH: 'true',
        WYRD_UI_MOCK_DATA: 'true',
        NO_COLOR: '1'
      },
      stdio: ['ignore', 'pipe', 'pipe']
    }
  );
  origin = await new Promise<string>((resolve, reject) => {
    let output = '';
    const timer = setTimeout(() => reject(new Error(`UI did not start: ${output}`)), 20_000);
    server.stdout!.on('data', (chunk) => {
      output += chunk.toString();
      const url = output.match(/http:\/\/127\.0\.0\.1:\d+/)?.[0];
      if (url) {
        clearTimeout(timer);
        resolve(url);
      }
    });
    server.stderr!.on('data', (chunk) => {
      output += chunk.toString();
    });
    server.once('error', (error) => {
      clearTimeout(timer);
      reject(error);
    });
    server.once('exit', () => {
      clearTimeout(timer);
      reject(new Error(output));
    });
  });
}, 25_000);
afterAll(async () => {
  if (server && server.exitCode === null) {
    const closed = once(server, 'exit');
    server.kill();
    await closed;
  }
});

let cookie: string;
async function get(path: string) {
  return fetch(origin + path, { headers: { cookie } });
}
beforeAll(async () => {
  const login = await fetch(origin + '/?/login', {
    method: 'POST',
    headers: { origin, accept: 'text/html' },
    body: new URLSearchParams(),
    redirect: 'manual'
  });
  cookie = login.headers.get('set-cookie')!.split(';')[0];
});

test('inventory lists every registrable kind with counts and row links', async () => {
  const response = await get('/t/acme/cards');
  expect(response.status).toBe(200);
  const html = await response.text();
  for (const kind of CARD_KINDS) expect(html).toContain(`?kind=${kind}`);
  expect(html).toContain('All kinds');
  expect(html).toContain('checkout-api');
  expect(html).toContain('/t/acme/cards/card_service_01');
  expect(html).toContain('pii-review');
  // Narrow-width row labels (M-01): every cell carries its stacked-row label.
  for (const label of ['name', 'kind', 'version', 'space', 'status', 'owner', 'updated'])
    expect(html).toContain(`data-l="${label}"`);
}, 30_000);

test('filters restore from a pasted URL as removable chips', async () => {
  const html = await (await get('/t/acme/cards?kind=Service&space=prod')).text();
  expect(html).toContain('checkout-api');
  expect(html).not.toContain('checkout-quality-dataset');
  // Removing one chip keeps the other filter.
  expect(html).toContain('?space=prod');
  expect(html).toContain('?kind=Service');
  // The search form carries the active filters so searching keeps them.
  expect(html).toContain('type="hidden" name="kind" value="Service"');
  expect(html).toContain('type="hidden" name="space" value="prod"');
});

test('rows sort newest-updated first and counts respect the other filters', () => {
  const acme = new WyrdClient(
    {
      tenant: { key: 'acme', name: 'Acme', tenantId: 't1', permissions: ['cards:read'] },
      subject: { id: 's', name: 'S' },
      permissions: ['cards:read']
    },
    true
  );
  const view = acme.cards({});
  const stamps = view.rows.map((row) => row.updatedAt);
  expect(stamps).toEqual([...stamps].sort().reverse());
  // Kind counts narrow with a non-kind filter instead of showing tenant totals.
  const scoped = acme.cards({ space: 'staging' });
  expect(scoped.kinds.find((entry) => entry.kind === 'Data')?.count).toBe(1);
  expect(scoped.kinds.find((entry) => entry.kind === 'Service')?.count).toBe(0);
  // The owner filter works, including the unowned sentinel.
  expect(acme.cards({ owner: 'unowned' }).rows.map((row) => row.uid)).toEqual(['card_service_05']);
  expect(acme.cards({ owner: 'r.okafor' }).rows.length).toBeGreaterThan(0);
});

test('lookup matches name, kind and uid', async () => {
  const byName = await (await get('/t/acme/cards?q=pii-review')).text();
  expect(byName).toContain('/t/acme/cards/card_verifier_01');
  const byUid = await (await get('/t/acme/cards?q=card_drift_01')).text();
  expect(byUid).toContain('model-drift');
});

test('no matching cards renders a truthful empty state preserving filters', async () => {
  const html = await (await get('/t/acme/cards?q=zzz-no-such-card')).text();
  expect(html).toContain('No matching cards');
});

test('another tenant sees a truthful empty registry, never acme cards', () => {
  const research = new WyrdClient(
    {
      tenant: { key: 'research', name: 'Research', tenantId: 't2', permissions: ['cards:read'] },
      subject: { id: 's', name: 'S' },
      permissions: ['cards:read']
    },
    true
  );
  const view = research.cards({});
  expect(view.rows).toEqual([]);
  expect(view.total).toBe(0);
  expect(view.recent).toEqual([]);
  expect(() => research.card('card_service_01')).toThrowError(
    expect.objectContaining({ status: 404 })
  );
});

test('cards:read is required before any inventory or detail projection', () => {
  const denied = new WyrdClient(
    {
      tenant: { key: 'acme', name: 'Acme', tenantId: 't', permissions: [] },
      subject: { id: 's', name: 'S' },
      permissions: []
    },
    true
  );
  for (const read of [() => denied.cards({}), () => denied.card('card_service_01')])
    expect(read).toThrowError(expect.objectContaining({ status: 403 }));
});
