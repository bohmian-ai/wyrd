// @vitest-environment jsdom
import { spawn, type ChildProcess } from 'node:child_process';
import { once } from 'node:events';
import { beforeAll, afterAll, expect, test } from 'vitest';
import { WyrdClient } from '$lib/server/wyrd';
import { CARD_KINDS } from './types';
import type { BrowserSession } from '$lib/server/auth/browser-sessions';
import type { components } from '$lib/server/generated/wyrd-http';

// The shared Card detail journey against the real dev server: exact
// identity/version/metadata/relationships/Spec, read-only prior versions,
// the typed Spec fallback, and the earned Workflow and Verifier
// presentations inside the unchanged shell (TASK-006 scenarios 2 and 4).
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

/** The markers every Card kind's page must render from the shared shell. */
const SHELL = [
  'read-only projection of server truth',
  'Metadata',
  'server-managed',
  'Relationships',
  'server-derived',
  'Versions'
];

test('generic detail renders exact identity, version, metadata, relationships and Spec', async () => {
  const html = await (await get('/t/acme/cards/card_generic_01')).text();
  for (const marker of SHELL) expect(html).toContain(marker);
  expect(html).toContain('settlement-guardrails');
  expect(html).toContain('Policy');
  expect(html).toContain('v7 (current)');
  expect(html).toContain('card_generic_01 · apiVersion wyrd/v1');
  expect(html).toContain('block on violation');
  expect(html).toContain('raise retention to 90d');
  expect(html).toContain('?version=v6');
  expect(html).toContain('applies_to');
  expect(html).toContain('/t/acme/cards/card_service_06');
}, 30_000);

test('selecting a prior version renders that version’s declared Spec', async () => {
  const html = await (await get('/t/acme/cards/card_generic_01?version=v6')).text();
  expect(html).toContain('Viewing v6');
  expect(html).toContain('v7 is current');
  expect(html).toContain('2 declared · pci-isolation · retention-90d');
  expect(html).not.toContain('egress-allowlist');
  const missing = await get('/t/acme/cards/card_generic_01?version=v99');
  expect(missing.status).toBe(404);
});

test('a prior version without a projected declaration says so instead of lying', async () => {
  const html = await (await get('/t/acme/cards/card_workflow_01?version=v6')).text();
  expect(html).toContain('Viewing v6');
  expect(html).not.toContain('retrieve context');
  expect(html).toContain('The v6 declaration is not projected in this environment.');
});

test('Workflow renders its earned stage presentation inside the same shell', async () => {
  const html = await (await get('/t/acme/cards/card_workflow_01')).text();
  for (const marker of SHELL) expect(html).toContain(marker);
  for (const stage of ['retrieve context', 'score tasks', 'judge thresholds', 'publish record'])
    expect(html).toContain(stage);
  expect(html).toContain('no run results render here');
  expect(html).toContain('/t/acme/cards/card_eval_01');
});

test('Verifier renders its earned presentation inside the same shell', async () => {
  const html = await (await get('/t/acme/cards/card_verifier_01')).text();
  for (const marker of SHELL) expect(html).toContain(marker);
  expect(html).toContain('Accepted evidence');
  expect(html).toContain('TestRunEvidence');
  expect(html).toContain('base → candidate commit pair');
  expect(html).toContain('No secrets render on a Verifier Card');
  expect(html).toContain('/t/acme/changes/change_01/verification');
});

test('a kind without a workspace uses the generated typed Spec fallback', async () => {
  const html = await (await get('/t/acme/cards/card_mcp_01')).text();
  for (const marker of SHELL) expect(html).toContain(marker);
  expect(html).toContain('payments-mcp');
  expect(html).toContain('kind-owned sections');
});

test('an unknown Card is a structured 404, never a blank shell', async () => {
  const html = await (await get('/t/acme/cards/card_nope')).text();
  expect(html).toContain('WYRD_SPEC_404_NOT_FOUND');
});

type Upstream = { path?: Record<string, string>; query?: Record<string, unknown> };

/** A live client whose upstream answers each `/v1` call from `answer`, recording what it was asked. */
function live(answer: (path: string, request: Upstream) => [number, unknown]) {
  const calls: ({ route: string } & Upstream)[] = [];
  const session = {
    api: async (_method: string, path: string, request: Upstream = {}) => {
      calls.push({ route: path, ...request });
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

const UID = '0190a000-0000-7000-8000-000000000001';
const PROMPT_UID = '0190a000-0000-7000-8000-000000000002';
/** One exact Card read as the registry returns it. */
function read(
  kind: components['schemas']['CardKind'],
  version: string,
  spec: unknown,
  relationships: components['schemas']['Relationships'] = {}
): components['schemas']['GetCardResponse'] {
  return {
    card: {
      apiVersion: 'wyrd/v1',
      kind,
      metadata: { name: 'ranker', space: 'search', version, uid: UID, labels: { team: 'search' }, spec_hash: `h-${version}` },
      spec: spec as components['schemas']['Card']['spec'],
      relationships,
      status: { phase: 'active' }
    },
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-02T00:00:00Z'
  };
}
const notFound: [number, unknown] = [404, { code: 'WYRD_REGISTRY_404_CARD_NOT_FOUND' }];

test('live UI Model detail projection', async () => {
  const v1 = read('Model', '1.0.0', { framework: 'sklearn', task: { type: 'classification' }, tags: [] }, {
    outbound_refs: [{ ref: { kind: 'Data', name: 'clicks', space: 'search', version: '1.0.0', uid: PROMPT_UID } }],
    inbound_refs: [{ alias: 'scorer', ref: { kind: 'Service', name: 'search', space: 'search', version: '3.0.0' } }]
  });
  const v2 = read('Model', '2.0.0', { framework: 'torch' });
  const registry = (path: string, request: Upstream): [number, unknown] =>
    path === '/cards/by-uid/{kind}/{card_uid}'
      ? request.path!.kind === 'Model' && request.path!.card_uid === UID ? [200, v1] : notFound
      : path === '/cards/{kind}/{space}/{name}/versions'
        ? [200, { versions: ['2.0.0', '1.0.0'] }]
        : request.query!.version === '2.0.0' ? [200, v2] : notFound;

  // The bare UID renders its own pinned version; the kind resolves through bounded per-kind reads.
  const direct = live(registry);
  const detail = await direct.client.card(UID.toUpperCase());
  expect(direct.calls.filter((call) => call.route.startsWith('/cards/by-uid'))).toHaveLength(CARD_KINDS.length);
  expect(direct.calls.find((call) => call.route.endsWith('/versions'))).toMatchObject({
    path: { kind: 'Model', space: 'search', name: 'ranker' },
    query: { include_prerelease: true }
  });
  expect(direct.calls.some((call) => call.route === '/cards/by-ref')).toBe(false);
  expect(detail).toMatchObject({
    uid: UID,
    name: 'ranker',
    kind: 'Model',
    version: '1.0.0',
    currentVersion: '2.0.0',
    status: { label: 'active', tone: 'ok' },
    versions: [{ version: '2.0.0' }, { version: '1.0.0' }],
    spec: [
      { title: 'Spec', entries: [{ k: 'framework', v: 'sklearn' }] },
      { title: 'task', entries: [{ k: 'type', v: 'classification' }] },
      { title: 'tags', entries: [], absent: 'Declared empty.' }
    ],
    relationships: [
      { relation: 'outbound', label: 'Data search/clicks@1.0.0', href: `/cards/${PROMPT_UID}` },
      { relation: 'inbound · scorer', label: 'Service search/search@3.0.0', href: undefined }
    ]
  });
  expect(detail.metadata).toContainEqual({ k: 'labels', v: 'team=search' });
  expect(detail.metadata).toContainEqual({ k: 'spec hash', v: 'h-1.0.0' });

  // Selecting another version reads that exact identity by reference.
  const selected = live(registry);
  const current = await selected.client.card(UID, '2.0.0');
  expect(selected.calls.find((call) => call.route === '/cards/by-ref')!.query).toEqual({
    kind: 'Model',
    space: 'search',
    name: 'ranker',
    version: '2.0.0'
  });
  expect(current).toMatchObject({ version: '2.0.0', currentVersion: '2.0.0', spec: [{ entries: [{ k: 'framework', v: 'torch' }] }] });

  // Unknown versions, unknown or malformed UIDs and denials are the server's answers; nothing falls back to fixtures.
  await expect(live(registry).client.card(UID, '9.9.9')).rejects.toMatchObject({ status: 404 });
  await expect(live(() => notFound).client.card(UID)).rejects.toMatchObject({ status: 404 });
  const malformed = live(registry);
  await expect(malformed.client.card('card_mcp_01')).rejects.toMatchObject({ status: 404 });
  expect(malformed.calls).toEqual([]);
  await expect(live(() => [403, { code: 'WYRD_PERMISSION_403_DENIED_RBAC' }]).client.card(UID)).rejects.toMatchObject({ status: 403 });
});

/** A registry that holds one Card line at `pinned`, listing `versions`. */
const line = (kind: components['schemas']['CardKind'], pinned: components['schemas']['GetCardResponse'], versions: string[]) =>
  live((path, request) =>
    path === '/cards/by-uid/{kind}/{card_uid}'
      ? request.path!.kind === kind ? [200, pinned] : notFound
      : [200, { versions }]
  );

test('live UI Data detail projection', async () => {
  const data = read(
    'Data',
    '1.0.0',
    {
      interface: { kind: 'Parquet', meta: { compression: 'Snappy' } },
      schema: { columns: [{ name: 'feature', dtype: 'float64' }, { name: 'label', dtype: 'int64' }] },
      stats: { row_count: 100 }
    },
    { inbound_refs: [{ ref: { kind: 'Verifier', name: 'drift', space: 'search', version: '1.0.0', uid: PROMPT_UID } }] }
  );
  const detail = await line('Data', data, ['1.0.0']).client.card(UID);
  // Every declared schema column renders under its indexed path; nothing is dropped.
  expect(detail.spec).toEqual([
    { title: 'interface', entries: [{ k: 'kind', v: 'Parquet' }, { k: 'meta.compression', v: 'Snappy' }] },
    {
      title: 'schema',
      entries: [
        { k: 'columns[0].name', v: 'feature' },
        { k: 'columns[0].dtype', v: 'float64' },
        { k: 'columns[1].name', v: 'label' },
        { k: 'columns[1].dtype', v: 'int64' }
      ]
    },
    { title: 'stats', entries: [{ k: 'row_count', v: '100' }] }
  ]);
  expect(detail).toMatchObject({ kind: 'Data', version: '1.0.0', currentVersion: '1.0.0' });
  expect(detail.relationships).toEqual([
    { relation: 'inbound', label: 'Verifier search/drift@1.0.0', href: `/cards/${PROMPT_UID}` }
  ]);
});

test('live UI Agent detail projection', async () => {
  const agent = read(
    'Agent',
    '1.0.0',
    {
      prompt: { kind: 'Prompt', name: 'triage', space: 'search', version: '1.0.0' },
      run_config: { max_iterations: 2 }
    },
    { outbound_refs: [{ ref: { kind: 'Prompt', name: 'triage', space: 'search', version: '1.0.0', uid: PROMPT_UID } }] }
  );
  agent.card.status = { phase: 'pending' };
  const detail = await line('Agent', agent, ['1.0.0']).client.card(UID);
  // The declared Prompt reference and the server-derived exact edge both render; the edge links its UID.
  expect(detail.spec).toContainEqual({
    title: 'prompt',
    entries: [
      { k: 'kind', v: 'Prompt' },
      { k: 'name', v: 'triage' },
      { k: 'space', v: 'search' },
      { k: 'version', v: '1.0.0' }
    ]
  });
  expect(detail.relationships).toEqual([
    { relation: 'outbound', label: 'Prompt search/triage@1.0.0', href: `/cards/${PROMPT_UID}` }
  ]);
  expect(detail.status).toEqual({ label: 'pending', tone: 'running' });
  // No execution evidence or actions are invented for a declaration.
  expect(detail.actions).toEqual([]);
  expect(detail.presentation).toBeUndefined();
});

test('live UI Prompt detail projection', async () => {
  const text = '<script>alert(1)</script> Summarize {{ticket}}';
  const prompt = read('Prompt', '1.0.0', { provider: 'openai', model: 'gpt-4o', messages: [text] });
  delete prompt.card.status;
  const detail = await line('Prompt', prompt, ['1.0.0']).client.card(UID);
  // Declared text passes through verbatim as data; the page renders it as text.
  expect(detail.spec).toEqual([
    { title: 'Spec', entries: [{ k: 'provider', v: 'openai' }, { k: 'model', v: 'gpt-4o' }] },
    { title: 'messages', entries: [{ k: '[0]', v: text }] }
  ]);
  // A Card without a server status says so rather than claiming one.
  expect(detail.status).toEqual({ label: 'unknown', tone: 'neutral' });
  expect(detail.actions).toEqual([]);
  expect(detail.presentation).toBeUndefined();
});
