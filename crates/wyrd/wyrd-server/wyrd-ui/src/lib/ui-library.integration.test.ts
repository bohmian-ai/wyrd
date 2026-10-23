/**
 * TASK-003 live proof. The built `ui://wyrd/view` HTML runs in the ext-apps
 * AppBridge test host; every other read goes to a real Wyrd server started by
 * `@wyrd/testing` over repository Postgres, through the SDK, the MCP endpoint,
 * and the BFF's real browser session. Nothing here mocks the server.
 */
import { readFileSync } from 'node:fs';
import { Client, StreamableHTTPClientTransport } from '@modelcontextprotocol/client';
import { AppBridge, PostMessageTransport } from '@modelcontextprotocol/ext-apps/app-bridge';
import type { Cookies } from '@sveltejs/kit';
import { expect, vi } from 'vitest';
import { z } from 'zod';
import { Bifrost, Cards, TableConfig, WyrdClient, type CardRef } from 'wyrd-ai';
import { loadCardHeader, loadChart, type ChartInput, type Executor, type WireRequest } from 'wyrd-ai-ui/loaders';
import { mcpExecutor } from 'wyrd-ai-ui/mcp';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { env } from '$env/dynamic/private';
import { browserSessions } from '$lib/server/auth/browser-sessions';
import { card, registered, series, summary, timeseries } from '$lib/ui-fixtures';
import { POST as loadPanel } from '../routes/t/[tenantKey]/ui/load/+server';
import { declared, fixture, serverTest } from '../../../../../../sdks/wyrd-sdk-ts/wyrd/tests/support/server';
import { load as loadExamplePage } from '../../../../../../examples/ui-app/src/routes/+page.server';
import { POST as loadExamplePanel } from '../../../../../../examples/ui-app/src/routes/view/load/+server';
import { dashboard } from '../../../../../../examples/ui-app/src/panels';

vi.setConfig({ testTimeout: 120_000 });

/** The single-file MCP App the package build emits. */
const ARTIFACT = 'packages/ui/dist/wyrd-view.html';

/**
 * Load the built artifact into an iframe behind a real AppBridge, answer its
 * `tools/call` with `answer`, deliver `doc` as the tool result, and return
 * the iframe's document and the tool calls it made.
 */
async function appBridgeView(doc: unknown, answer: (name: string, args: Record<string, unknown>) => object) {
  const iframe = document.createElement('iframe');
  document.body.append(iframe);
  const view = iframe.contentWindow as Window & typeof globalThis;
  // jsdom's postMessage leaves `event.source` unset, which ext-apps requires;
  // deliver each message between the two windows with its real source. The
  // host window is the iframe's parent: Vitest's `window` global is not it.
  const host = view.parent as typeof view;
  const deliver = (target: typeof view, source: Window) => (data: unknown) =>
    void setTimeout(() => target.dispatchEvent(new target.MessageEvent('message', { data, source })));
  view.postMessage = deliver(view, host);
  // jsdom has no ResizeObserver; ext-apps observes the body to report its size.
  // Its windows also lack structuredClone, which every real host provides.
  Object.assign(view, { ResizeObserver: class { observe() {} disconnect() {} }, structuredClone });
  // ext-apps traces every message at debug level on both sides.
  for (const each of [view.console, console]) vi.spyOn(each, 'debug').mockImplementation(() => {});
  host.postMessage = deliver(host, view);
  const calls: { name: string; arguments: Record<string, unknown> }[] = [];
  const bridge = new AppBridge(null, { name: 'wyrd-test-host', version: '1.0.0' }, { serverTools: {} }, { hostContext: { theme: 'dark' } });
  bridge.oncalltool = async (params) => {
    const args = (params.arguments ?? {}) as Record<string, unknown>;
    calls.push({ name: params.name, arguments: args });
    return { content: [], structuredContent: answer(params.name, args) as Record<string, unknown> };
  };
  const initialized = new Promise<void>((resolve) => (bridge.oninitialized = () => resolve()));
  await bridge.connect(new PostMessageTransport(view, view));
  view.document.open();
  view.document.write(readFileSync(ARTIFACT, 'utf8'));
  view.document.close();
  await initialized;
  await bridge.sendToolResult({ content: [], structuredContent: doc as Record<string, unknown> });
  return { document: view.document, calls, bridge };
}

/** The seeded latency table every live chart reads. */
const LATENCY = { namespace: 'vala.datasets', name: 'ui_latency' } as const;
/** The window the seeded rows fall in, and an earlier one holding none. */
const RANGE = { from: '2026-10-19T00:00:00Z', to: '2026-10-19T12:00:00Z' };
const BEFORE = { from: '2026-10-18T00:00:00Z', to: '2026-10-18T12:00:00Z' };
const LATENCY_CHART: ChartInput = {
  type: 'timeseries',
  title: 'p95 latency',
  sql: `SELECT at, model, p95 FROM ${LATENCY.namespace}.${LATENCY.name} WHERE at >= $1 AND at < $2 ORDER BY at, model`,
  x: 'at',
  y: ['p95'],
  series: 'model'
};
const DENIED = 'WYRD_PERMISSION_403_DENIED_RBAC';

/** Register the latency table as `client` and publish three rows inside {@link RANGE}. */
async function seedLatency(server: { flushBifrost(): void }, client?: WyrdClient) {
  const writer = await Bifrost.connect({
    client,
    table: TableConfig.fromJsonSchema(
      `${LATENCY.namespace}.${LATENCY.name}`,
      z.object({ at: z.iso.datetime(), model: z.string(), p95: z.number() })
    )
  });
  await writer.register();
  for (const [hour, model, p95] of [[1, 'ranker', 120.5], [1, 'reranker', 80.25], [2, 'ranker', 99]] as const)
    writer.insert({ at: `2026-10-19T0${hour}:00:00Z`, model, p95 });
  await writer.flush();
  await writer.shutdown();
  server.flushBifrost();
}

/** An executor running each read as `client` through the server's MCP endpoint. */
async function mcpOver(baseUrl: string, client: WyrdClient): Promise<{ executor: Executor; close(): Promise<void> }> {
  const mcp = new Client({ name: 'wyrd-ui-journey', version: '1.0.0' }, { versionNegotiation: { mode: 'auto' } });
  await mcp.connect(new StreamableHTTPClientTransport(new URL('/mcp', baseUrl), { fetch: client.httpAuth }));
  return { executor: mcpExecutor({ callServerTool: (params, options) => mcp.callTool(params, options) }), close: () => mcp.close() };
}

/** A SvelteKit cookie jar holding what the server sets, as a browser would. */
function jar(): Cookies {
  const cookies = new Map<string, string>();
  return {
    get: (name) => cookies.get(name),
    getAll: () => [...cookies].map(([name, value]) => ({ name, value })),
    set: (name, value) => void cookies.set(name, value),
    delete: (name) => void cookies.delete(name),
    serialize: () => ''
  } as Cookies;
}

/** The fixture tenant's Cards and latency table, seeded once per file. */
const test = serverTest().extend<{ seeded: { model: CardRef; foreign: CardRef } }>({
  seeded: [
    async ({ server }, use) => {
      await seedLatency(server);
      const cards = await Cards.connect();
      const model = declared(await cards.declareFromPath(fixture('cards/declare_and_pull/support-model.yaml')));
      const foreign = declared(await cards.declareFromPath(fixture('cards/latency_baseline/latency-baseline.yaml')));
      // The custom app example shows the support desk Service.
      await cards.declareFromPath(fixture('cards/declare_and_pull/support-desk.yaml'));
      await use({ model, foreign });
    },
    { scope: 'file' }
  ]
});

test('TASK-003: Identical authorized reads', async ({ server, seeded }) => {
  const client = WyrdClient.connect();
  const sdk = nodeExecutor(client);
  const mcp = await mcpOver(server.baseUrl, client);
  /** Run `request` on both surfaces; they must answer identically. */
  const both = async (request: WireRequest, signal?: AbortSignal) => {
    const [[direct], [tool]] = await Promise.all([sdk.run([request], signal), mcp.executor.run([request], signal)]);
    expect(tool, request.op).toEqual(direct);
    return direct;
  };
  try {
    const reads: WireRequest[] = [
      { op: 'cards.get_by_ref', card_ref: seeded.model },
      { op: 'cards.list', kind: 'Model', name: 'support-model', include_prerelease: true, limit: 10, cursor: null },
      { op: 'bifrost.query', sql: LATENCY_CHART.sql, params: [RANGE.from, RANGE.to] },
      { op: 'bifrost.list_tables' },
      { op: 'bifrost.describe_table', ...LATENCY }
    ];
    for (const request of reads) {
      const result = await both(request);
      expect(result, JSON.stringify(result)).toMatchObject({ ok: true });
    }
    const query = await both(reads[2]);
    expect(query.ok && query.value).toMatchObject({
      columns: [{ name: 'at' }, { name: 'model' }, { name: 'p95' }],
      rows: [
        [expect.stringMatching(/^2026-10-19T01:00:00/), 'ranker', 120.5],
        [expect.stringMatching(/^2026-10-19T01:00:00/), 'reranker', 80.25],
        [expect.stringMatching(/^2026-10-19T02:00:00/), 'ranker', 99]
      ],
      terminal: { outcome: 'success', row_count: 3 }
    });
    expect(await both({ op: 'verification.get_execution', execution_id: '0192f0a0-0000-7000-8000-000000000000' })).toMatchObject({ ok: false, problem: { status: 404 } });

    // Duplicate names and nulls stay positional.
    const table = `${LATENCY.namespace}.${LATENCY.name}`;
    const duplicated = await both({ op: 'bifrost.query', sql: `SELECT a.model, b.model, NULL AS n FROM ${table} a JOIN ${table} b ON a.p95 = b.p95 ORDER BY a.p95 LIMIT 1` });
    expect(duplicated, JSON.stringify(duplicated)).toMatchObject({
      ok: true,
      value: { columns: [{ name: 'model' }, { name: 'model' }, { name: 'n' }], rows: [['reranker', 'reranker', null]] }
    });
    // A placeholder with no bind, a row ceiling and a byte ceiling are refused, never truncated.
    expect(await both({ op: 'bifrost.query', sql: LATENCY_CHART.sql, params: [RANGE.from] })).toMatchObject({ ok: false });
    for (const limit of [{ max_rows: 2 }, { max_bytes: 64 }])
      expect(await both({ op: 'bifrost.query', sql: LATENCY_CHART.sql, params: [RANGE.from, RANGE.to], ...limit })).toMatchObject({
        ok: false,
        problem: { code: 'WYRD_VALA_413_QUERY_RESULT_TOO_LARGE' }
      });
    // A ceiling outside its admitted range is refused before any row, each surface in its own envelope.
    for (const limit of [{ max_rows: 0 }, { max_rows: 10_001 }, { max_bytes: 0 }, { max_bytes: 16_777_217 }]) {
      const request: WireRequest = { ...reads[2], ...limit } as WireRequest;
      const reason = `${Object.keys(limit)[0]} is outside the allowed range`;
      const [direct] = await sdk.run([request]);
      expect(direct, JSON.stringify(direct)).toEqual({ ok: false, problem: expect.objectContaining({ code: 'WYRD_SPEC_400_VALIDATION', detail: expect.stringContaining(reason) }) });
      const [tool] = await mcp.executor.run([request]);
      expect(tool, JSON.stringify(tool)).toEqual({ ok: false, problem: expect.objectContaining({ detail: expect.stringContaining(reason) }) });
    }
    // A wrapping or fractional original is refused, never narrowed into an admitted ceiling.
    for (const limit of [{ max_rows: 4_294_977_296 }, { max_rows: 10_000.5 }, { max_bytes: 4_299_161_600 }, { max_bytes: 4_194_304.5 }]) {
      const request: WireRequest = { ...reads[2], ...limit } as WireRequest;
      const [direct] = await sdk.run([request]);
      expect(direct, JSON.stringify(direct)).toEqual({ ok: false, problem: expect.objectContaining({ code: 'WYRD_SPEC_400_VALIDATION' }) });
      const [tool] = await mcp.executor.run([request]);
      expect(tool, JSON.stringify(tool)).toMatchObject({ ok: false });
    }
    // The exact maxima answer, as the omitted defaults did above.
    expect(await both({ ...reads[2], max_rows: 10_000, max_bytes: 16_777_216 } as WireRequest)).toMatchObject({ ok: true });
    // A refused SDK collection releases its stream; later queries are unaffected.
    const bifrost = await Bifrost.connect({ client });
    for (const maxRows of [10_001, 4_294_977_296]) {
      const refused = await bifrost.stream(LATENCY_CHART.sql, [RANGE.from, RANGE.to]);
      await expect(refused.collect({ maxRows })).rejects.toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
      await expect(refused.collect()).rejects.toThrow('query stream is closed');
    }
    expect((await (await bifrost.stream(LATENCY_CHART.sql, [RANGE.from, RANGE.to])).collect()).terminal).toMatchObject({ row_count: 3 });
    // A cancelled read rejects on both surfaces rather than answering.
    const cancelled = AbortSignal.abort();
    for (const executor of [sdk, mcp.executor])
      await expect(executor.run([reads[2]], cancelled)).rejects.toMatchObject({ name: 'AbortError' });
  } finally {
    await mcp.close();
  }

  // A principal without query permission is denied identically.
  const reader = WyrdClient.connect({ credential: server.scopedApiKey('ui_card_reader', ['cards:read']) });
  const readerMcp = await mcpOver(server.baseUrl, reader);
  try {
    const request: WireRequest = { op: 'bifrost.query', sql: LATENCY_CHART.sql, params: [RANGE.from, RANGE.to] };
    const [[direct], [tool]] = await Promise.all([nodeExecutor(reader).run([request]), readerMcp.executor.run([request])]);
    expect(tool).toEqual(direct);
    expect(direct).toMatchObject({ ok: false, problem: { code: DENIED } });
  } finally {
    await readerMcp.close();
  }
});

test('TASK-003: Actual four-host foundation proof', async ({ server, seeded }) => {
  // T4: the built HTML renders a view document through host tools/call alone.
  const doc = {
    version: 'wyrd.view/v1',
    catalogId: 'wyrd.ui/v1',
    title: 'Ranker',
    range: { from: '2026-10-19T00:00:00Z', to: '2026-10-19T12:00:00Z' },
    panels: [
      { id: 'header', component: 'CardHeader', input: { cardRef: card } },
      { id: 'latency', component: 'Chart', input: timeseries },
      { id: 'invalid', component: 'Chart', input: { type: 'stat' } }
    ]
  };
  const app = await appBridgeView(doc, (name) => {
    if (name === 'cards.get') return { card: registered, created_at: '2026-10-19T00:00:00Z', updated_at: '2026-10-19T00:00:00Z' };
    if (name === 'cards.list') return { items: [summary('1.4.2')], next_cursor: null };
    return series;
  });
  await vi.waitFor(() => expect(app.document.querySelector('[data-panel="latency"] svg')).not.toBeNull());
  expect(app.document.documentElement.dataset.mode).toBe('dark');
  expect(app.document.querySelector('h2')?.textContent).toBe('Ranker');
  expect(app.document.querySelector('[data-panel="header"]')?.textContent).toContain(registered.metadata.name);
  expect(app.document.querySelector('[data-panel="invalid"]')?.textContent).toContain('WYRD_SPEC_400_VALIDATION');
  expect(app.calls.map((call) => call.name).sort()).toEqual(['bifrost.query', 'cards.get', 'cards.list']);
  expect(app.calls.find((call) => call.name === 'bifrost.query')?.arguments).toEqual({
    sql: timeseries.sql,
    params: ['2026-10-19T00:00:00Z', '2026-10-19T12:00:00Z']
  });
  app.bridge.setHostContext({ theme: 'light' });
  await vi.waitFor(() => expect(app.document.documentElement.dataset.mode).toBe('light'));

  // T5 Node: real server data, an empty range, and a denied principal.
  const node = { executor: nodeExecutor(WyrdClient.connect()) };
  const populated = await loadChart(LATENCY_CHART, { ...node, range: RANGE });
  expect(populated.result).toMatchObject({ state: 'populated', data: { terminal: { row_count: 3 } } });
  expect(populated.executed).toEqual({ sql: LATENCY_CHART.sql, params: [RANGE.from, RANGE.to] });
  expect((await loadChart(LATENCY_CHART, { ...node, range: BEFORE })).result).toMatchObject({ state: 'empty' });
  const cardReader = { executor: nodeExecutor(WyrdClient.connect({ credential: server.scopedApiKey('ui_chart_denied', ['cards:read']) })) };
  expect((await loadChart(LATENCY_CHART, { ...cardReader, range: RANGE })).result).toMatchObject({ state: 'denied', problem: { code: DENIED } });
  const header = await loadCardHeader({ cardRef: seeded.model }, node);
  expect(header.result).toMatchObject({
    state: 'populated',
    data: { card: { metadata: { name: 'support-model', uid: seeded.model.uid } }, versions: { state: 'populated', data: { items: [{ version: '1.0.0' }] } } }
  });
  const queryReader = { executor: nodeExecutor(WyrdClient.connect({ credential: server.scopedApiKey('ui_header_denied', ['bifrost_query:read']) })) };
  expect((await loadCardHeader({ cardRef: seeded.model }, queryReader)).result).toMatchObject({ state: 'denied', problem: { code: DENIED } });

  // BFF: a real Rust browser session for another tenant loads only that tenant's panels.
  const tenant = 'ui-acme';
  const adminKey = server.bootstrapServiceInTenant(server.seedTenant(tenant), ['admin'], 'ui-acme-admin');
  const admin = WyrdClient.connect({ credential: adminKey });
  await seedLatency(server, admin);
  const own = declared(await (await Cards.connect({ client: admin })).declareFromPath(fixture('cards/declare_and_pull/support-model.yaml')));
  // The BFF reaches Wyrd at its configured internal origin; Vitest snapshots that env once.
  env.WYRD_SERVER_URL = server.baseUrl;
  const url = new URL(`http://ui.test/t/${tenant}/ui/load`);
  const cookies = jar();
  await browserSessions.signInWithApiKey(tenant, adminKey, url, cookies);
  const browserSession = await browserSessions.read(tenant, url, cookies);
  expect(browserSession).not.toBeNull();
  const secrets = [adminKey, server.apiKey, ...cookies.getAll().map(({ value }) => value)];
  /** POST one panel to the BFF as this browser; the answer never carries a credential. */
  const bff = async (body: unknown, session = browserSession) => {
    const response = await loadPanel({
      locals: { browserSession: session },
      request: new Request(url, { method: 'POST', body: JSON.stringify(body), headers: { 'content-type': 'application/json' } })
    } as never);
    const text = await response.text();
    for (const secret of secrets) expect(text).not.toContain(secret);
    return { status: response.status, body: JSON.parse(text) as Record<string, unknown> };
  };
  const chart = await bff({ component: 'Chart', input: LATENCY_CHART, range: RANGE });
  expect(chart, JSON.stringify(chart.body)).toMatchObject({
    status: 200,
    body: { result: { state: 'populated', data: { terminal: { row_count: 3 } } } }
  });
  expect(await bff({ component: 'CardHeader', input: { cardRef: { kind: own.kind, name: own.name, version: own.version } }, range: RANGE })).toMatchObject({
    status: 200,
    body: { result: { state: 'populated', data: { card: { metadata: { uid: own.uid } } } } }
  });
  expect(own.uid).not.toBe(seeded.model.uid);
  // The fixture tenant's Card is not reachable from this tenant's session.
  const foreign = await bff({ component: 'CardHeader', input: { cardRef: seeded.foreign }, range: RANGE });
  expect(foreign.body).toMatchObject({ result: { state: 'error', problem: { code: 'WYRD_REGISTRY_404_CARD_NOT_FOUND' } } });
  // Malformed panels are refused before any read.
  for (const [body, detail] of [
    [{ component: 'Nope', input: {}, range: RANGE }, 'Nope input: is not in the catalog'],
    [{ component: 'Chart', input: { type: 'timeseries' }, range: RANGE }, 'Chart input.'],
    [{ component: 'Chart', input: LATENCY_CHART, range: { from: 'yesterday' } }, 'Panel input.range'],
    [{ input: LATENCY_CHART, range: RANGE }, 'Panel input.component']
  ] as const)
    expect(await bff(body)).toMatchObject({ status: 400, body: { code: 'WYRD_SPEC_400_VALIDATION', detail: expect.stringContaining(detail) } });
  await expect(bff({ component: 'Chart', input: LATENCY_CHART, range: RANGE }, null)).rejects.toMatchObject({ status: 401 });

  // Custom app: its server loads props with WYRD_API_KEY; neither page receives the key.
  const page = await loadExamplePage();
  expect(page.header.result).toMatchObject({ state: 'populated', data: { card: { metadata: { name: 'support-desk' } } } });
  expect(page.chart.result.state).toBe('empty');
  expect(JSON.stringify(page)).not.toContain(server.apiKey);
  for (const panel of dashboard.panels) {
    const response = await loadExamplePanel({
      request: new Request('http://app.test/view/load', { method: 'POST', body: JSON.stringify({ component: panel.component, input: panel.input, range: RANGE }) })
    } as never);
    const text = await response.text();
    expect(response.status, panel.id).toBe(200);
    expect(text).not.toContain(server.apiKey);
  }
});
