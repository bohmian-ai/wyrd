/**
 * TASK-003, TASK-004 and TASK-005 live proof. The built `ui://wyrd/view` HTML runs in the ext-apps
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
import { Bifrost, Cards, TableConfig, WyrdClient } from 'wyrd-ai';
import {
  CardRefSchema,
  loadArtifactTable,
  loadCardFacts,
  loadCardHeader,
  loadCardInventory,
  loadPromptMessages,
  loadPromptVariables,
  loadRelationshipList,
  loadUsedBy,
  loadVersionDiff,
  loadChart,
  loadFacetRail,
  loadMetric,
  loadRecordDetail,
  loadRecordTable,
  loadStatStrip,
  loadTraceWaterfall,
  spanTree,
  type ChartInput,
  type Executor,
  type WireRequest
} from 'wyrd-ai-ui/loaders';
import { mcpExecutor } from 'wyrd-ai-ui/mcp';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { env } from '$env/dynamic/private';
import { browserSessions } from '$lib/server/auth/browser-sessions';
import { AGENT, PROMPT, agent as agentCard, card, inventory, notFound, observeAnswer, prompts, registered, series, summary, timeseries } from '$lib/ui-fixtures';
import { exportOtlp, nanos, seedSignals } from '$lib/ui-signals.integration';
import { POST as loadPanel } from '../routes/t/[tenantKey]/ui/load/+server';
import { declared, fixture, serverTest, type DeclaredRef } from '../../../../../../sdks/wyrd-sdk-ts/wyrd/tests/support/server';
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
const LATENCY = { namespace: 'bifrost.datasets', name: 'ui_latency' } as const;
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

/** A declared Card as the exact reference a component input takes. */
const exact = (ref: DeclaredRef) => ({ ...CardRefSchema.parse(ref), uid: ref.uid });
type Exact = ReturnType<typeof exact>;

/** The fixture tenant's Cards and latency table, seeded once per file. */
const test = serverTest().extend<{ seeded: { model: Exact; foreign: Exact } }>({
  seeded: [
    async ({ server }, use) => {
      await seedLatency(server);
      const cards = await Cards.connect();
      const model = exact(declared(await cards.declareFromPath(fixture('cards/declare_and_pull/support-model.yaml'))));
      const foreign = exact(declared(await cards.declareFromPath(fixture('cards/latency_baseline/latency-baseline.yaml'))));
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

test('TASK-004: Authorized Observe across hosts', async ({ server, seeded }) => {
  const TRACE = '4bf92f3577b34da6a3ce929d0e0e4736';
  const FOREIGN = 'a3ce929d0e0e47364bf92f3577b34da6';
  await seedSignals(server, server.apiKey, { trace: TRACE, service: 'ui-agent', subject: seeded.model.uid });
  const acme = server.bootstrapServiceInTenant(server.seedTenant('ui-observe-foreign'), ['admin'], 'ui-observe-admin');
  await seedSignals(server, acme, { trace: FOREIGN, service: 'foreign-agent' });

  const client = WyrdClient.connect();
  const mcp = await mcpOver(server.baseUrl, client);
  try {
    for (const executor of [nodeExecutor(client), mcp.executor]) {
      const context = { executor, range: RANGE };
      // Traces: one row per root span with counts over the whole trace; the other tenant's trace never appears.
      const traces = await loadRecordTable({ signal: 'traces' }, context);
      expect(traces.result, JSON.stringify(traces.result)).toMatchObject({
        state: 'populated',
        data: {
          rows: { rows: [[expect.stringMatching(/^2026-10-19T03:00:00\.000000007/), 'invoke_agent support', 'ui-agent', 900_000_000, 0, TRACE, '00f067aa0ba902b7']] },
          counts: { state: 'populated', data: { rows: [[TRACE, 3, 1]] } }
        }
      });
      // A traces WHERE matching a child span lists that span's root; facets count each trace once.
      const byChild = await loadRecordTable({ signal: 'traces', where: [{ column: 'name', value: 'execute_tool search' }] }, context);
      expect(byChild.result).toMatchObject({ state: 'populated', data: { rows: { rows: [[expect.any(String), 'invoke_agent support', 'ui-agent', 900_000_000, 0, TRACE, '00f067aa0ba902b7']] } } });
      expect((await loadFacetRail({ signal: 'traces', facets: ['service_name'] }, context)).result).toMatchObject({ state: 'populated', data: { facets: [{ values: { rows: [['ui-agent', 1]] } }] } });
      // Spans filter by a bound WHERE; the waterfall reads the exact trace with nanosecond starts.
      const errors = await loadRecordTable({ signal: 'spans', where: [{ column: 'status_code', value: 2 }] }, context);
      expect(errors.result).toMatchObject({ state: 'populated', data: { rows: { rows: [[expect.any(String), 'execute_tool search', 'ui-agent', 100_000_000, 2, TRACE, 'b9c7c989f97918e1', '53995c3f42cd8ad8']] } } });
      const waterfall = await loadTraceWaterfall({ traceId: TRACE }, context);
      expect(waterfall.result.state === 'populated' && spanTree(waterfall.result.data).map((span) => [span.name, span.depth, span.startMs])).toEqual([
        ['invoke_agent support', 0, 0],
        ['chat ranker', 1, 100],
        ['execute_tool search', 2, 200]
      ]);
      expect((await loadTraceWaterfall({ traceId: FOREIGN }, context)).result.state).toBe('empty');
      // Detail returns the whole span, GenAI messages included, and the log record by its full key.
      const span = await loadRecordDetail({ traceId: TRACE, spanId: '53995c3f42cd8ad8' }, context);
      expect(span.result.state === 'populated' && JSON.stringify(span.result.data.rows[0])).toContain('refund?');
      const logs = await loadRecordTable({ signal: 'logs' }, context);
      expect(logs.result.state, JSON.stringify(logs.result)).toBe('populated');
      const [time, , , , , , , observed, request] = logs.result.state === 'populated' ? logs.result.data.rows.rows[0] : [];
      expect(logs.result.state === 'populated' && logs.result.data.rows.rows[0].slice(1, 7)).toEqual(['ERROR', 'ui-agent', 'search timed out', null, TRACE, 'b9c7c989f97918e1']);
      const log = await loadRecordDetail({ log: { time_unix_nano: String(time), observed_time_unix_nano: String(observed), wyrd_request_id: String(request) } }, context);
      expect(log.result).toMatchObject({ state: 'populated', data: { rows: [expect.anything()] } });
      // Metrics keep their recorded type: the sum is integer-valued as exact decimal text, the gauge double-valued, neither zero-filled.
      const metrics = await loadRecordTable({ signal: 'metrics', columns: ['metric_name', 'metric_type', 'int_value', 'double_value', 'histogram_count'] }, context);
      expect(metrics.result.state === 'populated' && [...metrics.result.data.rows.rows].sort((a, b) => String(a[0]).localeCompare(String(b[0])))).toEqual([
        ['ui.load', 'gauge', null, 0.5, null],
        ['ui.requests', 'sum', '7', null, null]
      ]);
      // Facets and stats count this tenant's evidence only.
      expect((await loadFacetRail({ signal: 'spans', facets: ['service_name'] }, context)).result).toMatchObject({ state: 'populated', data: { facets: [{ values: { rows: [['ui-agent', 3]] } }] } });
      // No gateway call was recorded: tokens total null ("—"), cost has no currency.
      const stats = await loadStatStrip({ stats: ['traces', 'spans', 'errors', 'invocations', 'tokens'] }, context);
      expect(stats.result.state !== 'loading' && stats.result.state !== 'denied' && stats.result.state !== 'error' && stats.result.data.stats.map(({ total }) => total.rows)).toEqual([[[1]], [[3]], [[1]], [[1]], [[null]]]);
      const cost = await loadStatStrip({ stats: ['cost'] }, context);
      expect(cost.result.state !== 'loading' && cost.result.state !== 'denied' && cost.result.state !== 'error' && cost.result.data.stats[0].total.rows).toEqual([]);
      // A subject scope counts only the span attributed to that Card; unattributed evidence never counts.
      const scoped = await loadStatStrip({ stats: ['spans', 'errors'], scope: { column: 'subject_card_uid', uid: seeded.model.uid } }, context);
      expect(scoped.result.state === 'populated' && scoped.result.data.stats.map(({ total }) => total.rows)).toEqual([[[1]], [[1]]]);
      expect((await loadStatStrip({ stats: ['spans'], scope: { column: 'subject_card_uid', uid: 'card_unrecorded' } }, context)).result.state).toBe('empty');
      // An earlier range holds nothing.
      expect((await loadRecordTable({ signal: 'traces' }, { executor, range: BEFORE })).result).toMatchObject({ state: 'empty', reason: 'No root spans in bifrost.traces.spans with start_time_unix_nano in this range.' });
      expect((await loadStatStrip({ stats: ['spans'] }, { executor, range: BEFORE })).result.state).toBe('empty');
    }
  } finally {
    await mcp.close();
  }

  // A principal without query permission sees each component's denied state and no dependent read.
  const cardReader = { executor: nodeExecutor(WyrdClient.connect({ credential: server.scopedApiKey('ui_observe_denied', ['cards:read']) })), range: RANGE };
  for (const loaded of await Promise.all([
    loadRecordTable({ signal: 'traces' }, cardReader),
    loadTraceWaterfall({ traceId: TRACE }, cardReader),
    loadRecordDetail({ traceId: TRACE, spanId: '53995c3f42cd8ad8' }, cardReader),
    loadFacetRail({ signal: 'traces', facets: ['service_name'] }, cardReader),
    loadStatStrip({ stats: ['traces'] }, cardReader)
  ]))
    expect(loaded.result).toMatchObject({ state: 'denied', problem: { code: DENIED } });

  // T4: the built HTML renders Observe panels through host tools/call alone.
  const doc = {
    version: 'wyrd.view/v1',
    catalogId: 'wyrd.ui/v1',
    range: RANGE,
    panels: [
      { id: 'list', component: 'RecordTable', input: { signal: 'traces' } },
      { id: 'trace', component: 'TraceWaterfall', input: { traceId: TRACE } }
    ]
  };
  const app = await appBridgeView(doc, (_name, args) => observeAnswer(String(args.sql)) ?? { code: 'WYRD_SPEC_500_INTERNAL', title: 'Unexpected read' });
  await vi.waitFor(() => expect(app.document.querySelector('[data-panel="trace"] [role="tree"]')).not.toBeNull());
  expect(app.document.querySelector('[data-panel="list"]')?.textContent).toContain('invoke_agent support');
  expect(app.calls.map((call) => call.name)).toEqual(['bifrost.query', 'bifrost.query', 'bifrost.query']);
  expect(app.calls.find((call) => String(call.arguments.sql).includes("trace_id = decode($1, 'hex')"))?.arguments).toMatchObject({ params: [TRACE], max_rows: 10_000 });
});

test('TASK-004: Metric windows count every recorded value', async ({ server }) => {
  // One hour plots one-minute windows. Every series started before it, so its first point counts nothing.
  const range = { from: '2026-10-19T04:00:00Z', to: '2026-10-19T05:00:00Z' };
  const started = nanos('2026-10-19T03:00:00Z');
  const at = (time: string, value: Record<string, unknown>, start = started) => ({ startTimeUnixNano: start, timeUnixNano: nanos(`2026-10-19T04:${time}Z`), ...value });
  const route = (value: string) => [{ key: 'route', value: { stringValue: value } }];
  const histogram = (name: string, aggregationTemporality: number, dataPoints: unknown[]) => ({ name, unit: 's', histogram: { aggregationTemporality, dataPoints } });
  const resource = (service: string, metrics: unknown[]) => ({ resource: { attributes: [{ key: 'service.name', value: { stringValue: service } }] }, scopeMetrics: [{ scope: { name: 'ui-math' }, metrics }] });
  const tenant = 'ui-metric-math';
  const key = server.bootstrapServiceInTenant(server.seedTenant(tenant), ['admin'], 'ui-metric-math-admin');
  await exportOtlp(server, key, '/v1/metrics', {
    resourceMetrics: [
      resource('api', [
        // 100 in one window, then 110 and a restart counting 5 in the next: that window counts 10 + 5.
        {
          name: 'm.requests',
          unit: '1',
          sum: {
            aggregationTemporality: 2,
            isMonotonic: true,
            dataPoints: [at('10:00', { asInt: '100' }), at('11:10', { asInt: '110' }), at('11:40', { asInt: '5' }, nanos('2026-10-19T04:11:30Z'))]
          }
        },
        // A smaller value under the same start is a reset too: 50, then 60 and 4 count 10 + 4.
        { name: 'm.dropped', unit: '1', sum: { aggregationTemporality: 2, isMonotonic: true, dataPoints: [at('10:00', { asInt: '50' }), at('11:10', { asInt: '60' }), at('11:40', { asInt: '4' })] } },
        // Delta points add up; a gauge plots its last value exactly.
        { name: 'm.delta', unit: '1', sum: { aggregationTemporality: 1, isMonotonic: true, dataPoints: [at('11:10', { asInt: '3' }), at('11:40', { asInt: '4' })] } },
        { name: 'm.gauge', unit: '1', gauge: { dataPoints: [at('11:10', { asInt: '1' }), at('11:40', { asInt: '9007199254740993' })] } },
        histogram('h.reset', 2, [
          at('10:00', { count: '100', sum: 10, bucketCounts: ['60', '40'], explicitBounds: [1] }),
          at('11:10', { count: '110', sum: 11, bucketCounts: ['65', '45'], explicitBounds: [1] }),
          at('11:40', { count: '5', sum: 0.5, bucketCounts: ['3', '2'], explicitBounds: [1] }, nanos('2026-10-19T04:11:30Z'))
        ]),
        // Twenty-one routes; the lexically last counted most.
        histogram('h.routes', 1, Array.from({ length: 21 }, (_, index) => at('11:10', { count: index === 20 ? '50' : '1', sum: 1, bucketCounts: [index === 20 ? '50' : '1', '0'], explicitBounds: [1], attributes: route(`/r${String(index).padStart(2, '0')}`) }))),
        // No sum at all, a sum missing beside a recorded one, a recorded zero, and a cumulative baseline without its sum.
        histogram('h.nosum', 1, [at('11:10', { count: '4', bucketCounts: ['4', '0'], explicitBounds: [1] })]),
        histogram('h.mixed', 1, [at('11:10', { count: '4', sum: 2, bucketCounts: ['4', '0'], explicitBounds: [1], attributes: route('/a') }), at('11:10', { count: '4', bucketCounts: ['4', '0'], explicitBounds: [1], attributes: route('/b') })]),
        histogram('h.zero', 1, [at('11:10', { count: '4', sum: 0, bucketCounts: ['4', '0'], explicitBounds: [1] })]),
        histogram('h.baseline', 2, [at('10:00', { count: '10', bucketCounts: ['10', '0'], explicitBounds: [1] }), at('11:10', { count: '12', sum: 5, bucketCounts: ['12', '0'], explicitBounds: [1] })]),
        histogram('h.apart', 1, [at('11:10', { count: '2', sum: 1, bucketCounts: ['2', '0'], explicitBounds: [1] })])
      ]),
      resource('worker', [histogram('h.apart', 1, [at('11:10', { count: '2', sum: 3, bucketCounts: ['2', '0'], explicitBounds: [1] })])])
    ]
  });
  server.flushBifrost();
  const context = { executor: nodeExecutor(WyrdClient.connect({ credential: key })), range };
  const cells = (result: { state: string; data?: { columns: readonly { name: string }[]; rows: readonly (readonly unknown[])[] } }, ...names: string[]) => {
    expect(result.state, JSON.stringify(result)).toBe('populated');
    const index = names.map((name) => result.data!.columns.findIndex((column) => column.name === name));
    return result.data!.rows.map((row) => index.map((at) => row[at]));
  };
  const window = '2026-10-19T04:11:00';
  const inWindow = (rows: unknown[][]) => rows.filter(([instant]) => String(instant).startsWith(window)).map(([, ...rest]) => rest);

  // A counter's window adds each point's increase, so a reset inside it keeps the increase before it.
  await vi.waitFor(async () => expect(inWindow(cells((await loadMetric({ metric: 'm.requests' }, context)).chart.result, 'at', 'value'))).toEqual([[15 / 60]]), { timeout: 30_000 });
  expect(cells((await loadMetric({ metric: 'm.requests' }, context)).chart.result, 'at', 'value').map(([, value]) => value)).toEqual([0, 15 / 60]);
  expect(inWindow(cells((await loadMetric({ metric: 'm.dropped' }, context)).chart.result, 'at', 'value'))).toEqual([[14 / 60]]);
  expect(inWindow(cells((await loadMetric({ metric: 'm.delta' }, context)).chart.result, 'at', 'value'))).toEqual([[7 / 60]]);
  expect(cells((await loadMetric({ metric: 'm.gauge' }, context)).chart.result, 'latest')).toEqual([[expect.stringMatching(/^9007199254740993(\.0+)?$/)]]);

  // A histogram window counts the same way, its buckets and sum included.
  const reset = (await loadMetric({ metric: 'h.reset' }, context)).histogram!;
  expect(inWindow(cells(reset.rate.result, 'at', 'value'))).toEqual([[15 / 60]]);
  expect(inWindow(cells(reset.latency.result, 'at', 'measure', 'value')).filter(([measure]) => measure === 'mean')).toEqual([['mean', 1.5 / 15]]);
  expect(cells(reset.distribution.result, 'bucket', 'count')).toEqual([['≤ 1', 8], ['> 1', 7]]);

  // The twenty routes that counted most are read, the busiest first; the title says how many there were.
  const routes = (await loadMetric({ metric: 'h.routes', by: ['attributes.route'] }, context)).histogram!;
  const plotted = new Set(cells(routes.rate.result, 'attributes.route').map(([value]) => value));
  expect(plotted.size).toBe(20);
  expect(plotted.has('/r20')).toBe(true);
  expect(plotted.has('/r19')).toBe(false);
  expect(routes.rate.input.title).toContain('the 20 that counted most of 21 groups plotted');

  // A mean needs every merged sum: none, a missing one or a missing baseline leave it unavailable, and a recorded zero is zero.
  const mean = async (metric: string) =>
    cells((await loadMetric({ metric }, context)).histogram!.latency.result, 'at', 'measure', 'value').filter(([instant, measure]) => String(instant).startsWith(window) && measure === 'mean').map(([, , value]) => value);
  expect(await mean('h.nosum')).toEqual([null]);
  expect(await mean('h.mixed')).toEqual([null]);
  expect(await mean('h.baseline')).toEqual([null]);
  expect(await mean('h.zero')).toEqual([0]);
  // Counts and estimates stay whatever the sum.
  const nosum = (await loadMetric({ metric: 'h.nosum' }, context)).histogram!;
  expect(inWindow(cells(nosum.rate.result, 'at', 'value'))).toEqual([[4 / 60]]);
  expect(cells(nosum.latency.result, 'measure').map(([measure]) => measure)).toContain('p50 estimated');

  // Every series apart: each compatible series its own line, labelled from the series index.
  const apart = (await loadMetric({ metric: 'h.apart', apart: true }, context)).histogram!;
  expect(
    cells(apart.latency.result, 'resource', 'measure', 'value')
      .filter(([, measure]) => measure === 'mean')
      .map(([resource, , value]) => [JSON.parse(String(resource))['service.name'], value])
      .sort()
  ).toEqual([['api', 0.5], ['worker', 1.5]]);
});

test('card and prompt components use authorized host reads and actions', async ({ server, seeded }) => {
  const cards = await Cards.connect();
  const ui = (name: string) => fixture(`cards/ui_cards/${name}`);
  /** A declared Prompt as the exact Prompt reference its components take. */
  const asPrompt = (ref: Exact) => ({ ...ref, kind: 'Prompt' as const });
  const older = asPrompt(exact(declared(await cards.declareFromPath(ui('support-reply-1.0.0.yaml')))));
  const prompt = asPrompt(exact(declared(await cards.declareFromPath(ui('support-reply-1.1.0.yaml')))));
  const agent = exact(declared(await cards.declareFromPath(ui('reply-agent.yaml'))));
  const native = asPrompt(exact(declared(await cards.declareFromPath(fixture('cards/declare_and_pull/native-prompt.yaml')))));
  // Another tenant registers a Prompt of its own; this tenant never sees it.
  const tenant = 'ui-cards-foreign';
  const foreignKey = server.bootstrapServiceInTenant(server.seedTenant(tenant), ['admin'], 'ui-cards-admin');
  const foreignCards = await Cards.connect({ client: WyrdClient.connect({ credential: foreignKey }) });
  const theirs = exact(declared(await foreignCards.declareFromPath(fixture('cards/gateway_inference/ask-prompt.yaml'))));

  const client = WyrdClient.connect();
  const mcp = await mcpOver(server.baseUrl, client);
  try {
    for (const [surface, executor] of [['node', nodeExecutor(client)], ['mcp', mcp.executor]] as const) {
      const context = { executor };
      // Inventory: one row per version, filtered by kind, name and label; the other tenant's Card never lists.
      const prompts = await loadCardInventory({ kind: 'Prompt' }, context);
      expect(prompts.result.state, surface).toBe('populated');
      const uids = prompts.result.state === 'populated' ? prompts.result.data.items.map((item) => item.card_uid) : [];
      expect(uids).toEqual(expect.arrayContaining([older.uid, prompt.uid, native.uid]));
      expect(uids).not.toContain(theirs.uid);
      const named = await loadCardInventory({ query: 'SUPPORT-reply' }, context);
      expect(named.result.state === 'populated' && named.result.data.items.map((item) => item.version).sort()).toEqual(['1.0.0', '1.1.0']);
      const labelled = await loadCardInventory({ query: 'labels.team = "support"', kind: 'Prompt' }, context);
      expect(labelled.result.state === 'populated' && labelled.result.data.items.map((item) => item.card_uid).sort()).toEqual([older.uid, prompt.uid].sort());
      expect((await loadCardInventory({ query: 'ask-prompt' }, context)).result).toMatchObject({ state: 'empty', reason: 'No Card version matches these filters.' });
      expect((await loadCardInventory({ query: 'name = = "x"' }, context)).result).toMatchObject({ state: 'error', problem: { code: 'WYRD_SPEC_400_VALIDATION' } });

      // Exact detail: facts, relationships in both directions and the registered files.
      expect((await loadCardFacts({ cardRef: agent }, context)).result).toMatchObject({ state: 'populated', data: { metadata: { uid: agent.uid } } });
      expect((await loadRelationshipList({ cardRef: agent }, context)).result).toMatchObject({
        state: 'populated',
        data: { outbound: [{ ref: { kind: 'Prompt', name: 'support-reply', version: '1.1.0', uid: prompt.uid } }] }
      });
      expect((await loadRelationshipList({ cardRef: prompt }, context)).result).toMatchObject({ state: 'populated', data: { inbound: [{ ref: { uid: agent.uid } }] } });
      expect((await loadCardFacts({ cardRef: { ...prompt, version: '9.9.9', uid: undefined } }, context)).result).toMatchObject({
        state: 'error',
        problem: { code: 'WYRD_REGISTRY_404_CARD_NOT_FOUND' }
      });
      expect((await loadCardFacts({ cardRef: theirs }, context)).result).toMatchObject({ state: 'error', problem: { code: 'WYRD_REGISTRY_404_CARD_NOT_FOUND' } });
      const files = await loadArtifactTable({ cardRef: seeded.model }, context);
      if (surface === 'node') {
        expect(files.result).toMatchObject({ state: 'populated', data: [{ relative_path: 'support-model.bin' }] });
        expect((await loadArtifactTable({ cardRef: agent }, context)).result).toMatchObject({ state: 'empty' });
      } else {
        // The MCP server has no file-listing tool yet; the panel shows its refusal.
        expect(files.result.state, JSON.stringify(files.result)).toBe('error');
      }

      // Prompt: authored forms kept, server-derived variables, its Agent and a comparison.
      expect((await loadPromptMessages({ cardRef: prompt }, context)).result).toMatchObject({
        state: 'populated',
        data: { system: 'You answer support tickets.\nCite the policy section you rely on.', response_format: 'verdict' }
      });
      expect((await loadPromptMessages({ cardRef: native }, context)).result).toMatchObject({
        state: 'populated',
        data: { format: 'chat_completion', body: { seed: 123, future_knob: { enabled: true } } }
      });
      expect((await loadPromptMessages({ cardRef: agent }, context)).result).toMatchObject({ state: 'error', problem: { code: 'WYRD_SPEC_400_VALIDATION' } });
      expect((await loadPromptVariables({ cardRef: prompt }, context)).result).toEqual({
        state: 'populated',
        data: [
          { name: 'ticket', kind: 'text' },
          { name: 'tone', kind: 'text' }
        ]
      });
      expect((await loadUsedBy({ cardRef: prompt }, context)).result).toMatchObject({
        state: 'populated',
        data: [{ ref: { uid: agent.uid }, agent: { state: 'populated', data: { route: { kind: 'native' }, services: [] } } }]
      });
      expect((await loadUsedBy({ cardRef: older }, context)).result).toMatchObject({ state: 'empty' });
      const diff = await loadVersionDiff({ cardRef: prompt }, context);
      expect(diff.result.state === 'populated' && [diff.result.data.other, diff.result.data.changes.map((change) => change.field)]).toEqual([
        '1.0.0',
        ['system', 'messages[0]', 'response_format', 'variables']
      ]);
      expect((await loadVersionDiff({ cardRef: older, otherVersion: '1.1.0' }, context)).result).toMatchObject({ state: 'populated', data: { other: '1.1.0' } });
      expect((await loadVersionDiff({ cardRef: native }, context)).result).toMatchObject({ state: 'empty', reason: 'Only version 1.0.0 is registered.' });
    }
  } finally {
    await mcp.close();
  }

  // A principal without card permission sees each component denied and no dependent read.
  const runs: WireRequest[][] = [];
  const queryOnly = nodeExecutor(WyrdClient.connect({ credential: server.scopedApiKey('ui_cards_denied', ['bifrost_query:read']) }));
  const denied: { executor: Executor } = { executor: { run: ((requests, signal) => (runs.push([...requests]), queryOnly.run(requests, signal))) as Executor['run'] } };
  for (const loaded of await Promise.all([
    loadCardInventory({}, denied),
    loadCardFacts({ cardRef: agent }, denied),
    loadRelationshipList({ cardRef: agent }, denied),
    loadArtifactTable({ cardRef: seeded.model }, denied),
    loadPromptMessages({ cardRef: prompt }, denied),
    loadPromptVariables({ cardRef: prompt }, denied),
    loadUsedBy({ cardRef: prompt }, denied),
    loadVersionDiff({ cardRef: prompt }, denied)
  ]))
    expect(loaded.result).toMatchObject({ state: 'denied', problem: { code: DENIED } });
  expect(runs.every((requests) => requests.length === 1)).toBe(true);
  expect(runs).toHaveLength(8);

  // BFF: the other tenant's browser session lists its own Prompt and never this tenant's.
  env.WYRD_SERVER_URL = server.baseUrl;
  const url = new URL(`http://ui.test/t/${tenant}/ui/load`);
  const cookies = jar();
  await browserSessions.signInWithApiKey(tenant, foreignKey, url, cookies);
  const browserSession = await browserSessions.read(tenant, url, cookies);
  const bff = async (body: unknown) => {
    const response = await loadPanel({
      locals: { browserSession },
      request: new Request(url, { method: 'POST', body: JSON.stringify(body), headers: { 'content-type': 'application/json' } })
    } as never);
    const text = await response.text();
    expect(text).not.toContain(foreignKey);
    return JSON.parse(text) as { result: { state: string; data?: { items: { card_uid: string }[] } } };
  };
  const listed = await bff({ component: 'CardInventory', input: {}, range: RANGE });
  const visible = listed.result.data?.items.map((item) => item.card_uid) ?? [];
  expect(visible).toContain(theirs.uid);
  for (const ours of [older, prompt, agent, native, seeded.model]) expect(visible).not.toContain(ours.uid);
  expect(await bff({ component: 'PromptMessages', input: { cardRef: prompt }, range: RANGE })).toMatchObject({
    result: { state: 'error', problem: { code: 'WYRD_REGISTRY_404_CARD_NOT_FOUND' } }
  });

  // T4: the built HTML renders Card and Prompt panels through host tools/call alone.
  const doc = {
    version: 'wyrd.view/v1',
    catalogId: 'wyrd.ui/v1',
    range: RANGE,
    panels: [
      { id: 'cards', component: 'CardInventory', input: { kind: 'Prompt' } },
      { id: 'messages', component: 'PromptMessages', input: { cardRef: PROMPT } },
      { id: 'used', component: 'UsedBy', input: { cardRef: PROMPT } }
    ]
  };
  const app = await appBridgeView(doc, (name, args) => {
    if (name === 'cards.list') return { items: inventory.filter((item) => (item as { kind: string }).kind === 'Prompt'), next_cursor: null };
    if (name === 'cards.get') return { card: (args.card_ref as { name: string }).name === AGENT.name ? agentCard : prompts['1.1.0'] };
    return notFound;
  });
  await vi.waitFor(() => expect(app.document.querySelector('[data-panel="used"]')?.textContent).toContain(AGENT.name));
  expect(app.document.querySelector('[data-panel="cards"]')?.textContent).toContain(PROMPT.name);
  expect(app.document.querySelector('[data-panel="messages"]')?.textContent).toContain('{{ticket}}');
  expect(app.calls.map((call) => call.name).sort()).toEqual(['cards.get', 'cards.get', 'cards.get', 'cards.list']);
  expect(app.calls.find((call) => call.name === 'cards.list')?.arguments).toMatchObject({ kind: 'Prompt', limit: 50 });
});
