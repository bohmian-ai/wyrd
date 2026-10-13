// @vitest-environment jsdom
import { spawn, type ChildProcess } from 'node:child_process';
import { once } from 'node:events';
import { beforeAll, afterAll, expect, test } from 'vitest';
import { WyrdClient } from '$lib/server/wyrd';
import { CARD_KINDS } from './types';
import type { BrowserSession } from '$lib/server/auth/browser-sessions';
import type { components } from '$lib/server/generated/wyrd-http';

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
  for (const label of ['name', 'kind', 'version', 'space', 'status', 'updated'])
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

test('rows sort newest-updated first and counts respect the other filters', async () => {
  const acme = new WyrdClient(
    {
      tenant: { key: 'acme', name: 'Acme', tenantId: 't1', permissions: ['cards:read'] },
      subject: { id: 's', name: 'S' },
      permissions: ['cards:read']
    },
    true
  );
  const view = await acme.cards({});
  const stamps = view.rows.map((row) => row.updatedAt);
  expect(stamps).toEqual([...stamps].sort().reverse());
  // Kind counts narrow with a non-kind filter instead of showing tenant totals.
  const scoped = await acme.cards({ space: 'staging' });
  expect(scoped.kinds.find((entry) => entry.kind === 'Data')?.count).toBe(1);
  expect(scoped.kinds.find((entry) => entry.kind === 'Service')?.count).toBe(0);
  // The owner filter works, including the unowned sentinel.
  expect((await acme.cards({ owner: 'unowned' })).rows.map((row) => row.uid)).toEqual(['card_service_05']);
  expect((await acme.cards({ owner: 'r.okafor' })).rows.length).toBeGreaterThan(0);
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

test('another tenant sees a truthful empty registry, never acme cards', async () => {
  const research = new WyrdClient(
    {
      tenant: { key: 'research', name: 'Research', tenantId: 't2', permissions: ['cards:read'] },
      subject: { id: 's', name: 'S' },
      permissions: ['cards:read']
    },
    true
  );
  const view = await research.cards({});
  expect(view.rows).toEqual([]);
  expect(view.total).toBe(0);
  expect(view.recent).toEqual([]);
  await expect(research.card('card_service_01')).rejects.toMatchObject({ status: 404 });
});

test('cards:read is required before any inventory or detail projection', async () => {
  const denied = new WyrdClient(
    {
      tenant: { key: 'acme', name: 'Acme', tenantId: 't', permissions: [] },
      subject: { id: 's', name: 'S' },
      permissions: []
    },
    true
  );
  await expect(denied.cards({})).rejects.toMatchObject({ status: 403 });
  await expect(denied.card('card_service_01')).rejects.toMatchObject({ status: 403 });
});

test('server wildcard grants satisfy cards:read', async () => {
  const client = (permissions: string[]) =>
    new WyrdClient(
      {
        tenant: { key: 'acme', name: 'Acme', tenantId: 't', permissions },
        subject: { id: 's', name: 'S' },
        permissions
      },
      true
    );
  for (const grants of [['wildcard:wildcard'], ['cards:wildcard'], ['wildcard:read']])
    await expect(client(grants).cards({})).resolves.toBeDefined();
  await expect(client(['artifacts:wildcard']).cards({})).rejects.toMatchObject({ status: 403 });
});

type Summary = components['schemas']['CardSummary'];
const summary = (name: string, kind: Summary['kind'] = 'Prompt'): Summary => ({
  card_uid: `0190a000-0000-7000-8000-${String(name.length).padStart(12, '0')}`,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-02-03T04:05:06Z',
  kind,
  name,
  space: 'default',
  spec_hash: 'h',
  status: 'active',
  version: '1.0.0'
});

/** A live client whose upstream answers each `/v1` call from `answer`, recording what it was asked. */
function live(answer: (path: string, request: { path?: Record<string, string>; query?: Record<string, unknown> }) => [number, unknown]) {
  const calls: { path: string; query?: Record<string, unknown>; params?: Record<string, string> }[] = [];
  const session = {
    api: async (_method: string, path: string, request: { path?: Record<string, string>; query?: Record<string, unknown> } = {}) => {
      calls.push({ path, query: request.query, params: request.path });
      const [status, body] = answer(path, request);
      return new Response(JSON.stringify(body), { status });
    }
  } as unknown as BrowserSession;
  const client = new WyrdClient(
    {
      tenant: { key: 'acme', name: 'acme', tenantId: 't', permissions: ['cards:read'] },
      subject: { id: 'p', name: 'p' },
      permissions: ['cards:read']
    },
    false,
    session
  );
  return { client, calls };
}

test('live UI Card inventory projection', async () => {
  // URL controls reach the registry's typed filters and keyset cursor.
  const items = [summary('triage'), summary('ranker', 'Model')];
  const listed = live(() => [200, { items, next_cursor: 'next-1' }]);
  const view = await listed.client.cards(
    { q: 'tri.age"', kind: 'Prompt', space: 'default', status: 'active', label: 'team=search', owner: 'r.okafor' },
    'cur-0'
  );
  expect(listed.calls).toEqual([
    {
      path: '/cards',
      params: undefined,
      query: {
        kind: 'Prompt',
        space: 'default',
        status: 'active',
        filter: String.raw`name =~ "(?i)tri\\.age\"" and labels."team" = "search" and labels."owner" = "r.okafor"`,
        limit: 20,
        cursor: 'cur-0'
      }
    }
  ]);
  expect(view.rows.map((row) => [row.name, row.kind, row.status, row.updated])).toEqual([
    ['triage', 'Prompt', { label: 'active', tone: 'ok' }, '2026-02-03 04:05Z'],
    ['ranker', 'Model', { label: 'active', tone: 'ok' }, '2026-02-03 04:05Z']
  ]);
  // Server order and cursor pass through; no page-local sorting or invented counts.
  expect(view).toMatchObject({ total: null, next: 'next-1', order: 'oldest registered first', recent: [] });
  expect(view.kinds.every((entry) => entry.count === null)).toBe(true);
  expect(view.rows[0]).not.toHaveProperty('owner');

  // A bare label key selects Cards declaring it; the last page has no cursor.
  const last = live(() => [200, { items: [], next_cursor: null }]);
  const empty = await last.client.cards({ label: 'tier' });
  expect(last.calls[0].query).toEqual({ filter: 'labels."tier" exists', limit: 20 });
  expect(empty).toMatchObject({ rows: [], next: null, total: null });
  // Only an unscoped first page with no Cards is an empty tenant.
  expect((await live(() => [200, { items: [] }]).client.cards({})).total).toBe(0);

  // An exact UID resolves through bounded kind-qualified reads, then its identity line under the same filters.
  const target = summary('ranker', 'Model');
  const other = { ...summary('ranker', 'Model'), card_uid: '0190a000-0000-7000-8000-999999999999', version: '2.0.0' };
  const byUid = live((path, request) =>
    path === '/cards'
      ? [200, { items: [other, target], next_cursor: null }]
      : request.path!.kind === 'Model'
        ? [200, { card: { kind: 'Model', metadata: { name: 'ranker' } } }]
        : [404, { code: 'WYRD_REGISTRY_404_CARD_NOT_FOUND' }]
  );
  const found = await byUid.client.cards({ q: target.card_uid.toUpperCase(), status: 'active' });
  expect(found.rows.map((row) => row.uid)).toEqual([target.card_uid]);
  expect(byUid.calls.filter((call) => call.path !== '/cards')).toHaveLength(CARD_KINDS.length);
  expect(byUid.calls.at(-1)).toMatchObject({
    path: '/cards',
    query: { kind: 'Model', name: 'ranker', status: 'active', include_prerelease: true, limit: 200 }
  });
  expect(byUid.calls.at(-1)!.query).not.toHaveProperty('filter', expect.stringContaining('name =~'));
  // With a kind filter the UID costs one read; an unknown UID is an empty match.
  const missing = live(() => [404, { code: 'WYRD_REGISTRY_404_CARD_NOT_FOUND' }]);
  expect((await missing.client.cards({ q: target.card_uid, kind: 'Data' })).rows).toEqual([]);
  expect(missing.calls).toHaveLength(1);

  // Refusals are the page's refusals; nothing falls back to fixtures.
  await expect(live(() => [403, { code: 'WYRD_PERMISSION_403_DENIED_RBAC' }]).client.cards({})).rejects.toMatchObject({ status: 403 });
  await expect(live(() => [400, { code: 'WYRD_REGISTRY_400_CURSOR_MISMATCH' }]).client.cards({}, 'bad')).rejects.toMatchObject({ status: 400 });
  await expect(
    new WyrdClient({ tenant: { key: 'acme', name: 'acme', tenantId: 't', permissions: [] }, subject: { id: 'p', name: 'p' }, permissions: [] }, false).cards({})
  ).rejects.toMatchObject({ status: 502 });
});
