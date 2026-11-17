/**
 * TASK-003, TASK-004 and TASK-005 live proof. The built `ui://wyrd/view` HTML runs in the ext-apps
 * AppBridge test host; every other read goes to a real Wyrd server started by
 * `@wyrd/testing` over repository Postgres, through the SDK, the MCP endpoint,
 * and the BFF's real browser session. Nothing here mocks the server.
 */
import { copyFileSync, cpSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Client, StreamableHTTPClientTransport } from '@modelcontextprotocol/client';
import { AppBridge, PostMessageTransport } from '@modelcontextprotocol/ext-apps/app-bridge';
import type { Cookies } from '@sveltejs/kit';
import { render } from '@testing-library/svelte';
import { expect, vi } from 'vitest';
import { TraceWaterfall } from 'wyrd-ai-ui';
import { z } from 'zod';
import { Bifrost, Cards, GithubConnection, Principals, TableConfig, WyrdClient, WyrdState } from 'wyrd-ai';
import { signInGithubBrowser } from '@wyrd/testing';
import {
  CardRefSchema,
  loadArtifactTable,
  loadCardFacts,
  loadCardHeader,
  loadCardInventory,
  loadExecution,
  loadExecutionCharts,
  loadPromptMessages,
  loadPromptVariables,
  loadRelationshipList,
  loadRunsOn,
  loadUsedBy,
  loadVerifierDefinition,
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
  type QueryResult,
  type WireRequest
} from 'wyrd-ai-ui/loaders';
import { mcpExecutor } from 'wyrd-ai-ui/mcp';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { env } from '$env/dynamic/private';
import { browserSessions } from '$lib/server/auth/browser-sessions';
import { AGENT, PROMPT, agent as agentCard, card, inventory, notFound, observeAnswer, prompts, registered, series, summary, timeseries } from '$lib/ui-fixtures';
import { exportOtlp, nanos, seedSignals } from '$lib/ui-signals.integration';
import { POST as loadPanel } from '../routes/t/[tenantKey]/ui/load/+server';
import { POST as cancelExecution } from '../routes/t/[tenantKey]/verify/executions/[executionId]/cancel/+server';
import { GET as downloadPublication } from '../routes/t/[tenantKey]/verify/executions/[executionId]/download/+server';
import { AGREES, assistantTest, judgeReply, toolCall, toolTurn } from '../../../../../../sdks/wyrd-sdk-ts/wyrd/tests/support/assistant';
import { declared, fixture, groupExecution, runtimeAccount, scheduledResult, serverTest, type DeclaredRef } from '../../../../../../sdks/wyrd-sdk-ts/wyrd/tests/support/server';
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
async function appBridgeView(doc: unknown, answer: (name: string, args: Record<string, unknown>) => object | Promise<object>) {
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
  // Its windows also lack structuredClone and matchMedia, which every real host provides.
  Object.assign(view, { ResizeObserver: class { observe() {} disconnect() {} }, structuredClone, matchMedia: window.matchMedia });
  // ext-apps traces every message at debug level on both sides.
  for (const each of [view.console, console]) vi.spyOn(each, 'debug').mockImplementation(() => {});
  host.postMessage = deliver(host, view);
  const calls: { name: string; arguments: Record<string, unknown> }[] = [];
  const bridge = new AppBridge(null, { name: 'wyrd-test-host', version: '1.0.0' }, { serverTools: {} }, { hostContext: { theme: 'dark' } });
  bridge.oncalltool = async (params) => {
    const args = (params.arguments ?? {}) as Record<string, unknown>;
    calls.push({ name: params.name, arguments: args });
    // An answer that is already a whole tool result (it has `content`) is relayed as is, so a tool's refusal reaches the page as one.
    const answered = (await answer(params.name, args)) as Record<string, unknown>;
    return Array.isArray(answered.content) ? (answered as never) : { content: [], structuredContent: answered };
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

test('an exact Verifier definition and where it runs load through every authorized host', async ({ server, seeded: _seeded }) => {
  const cards = await Cards.connect();
  // A Service bound to a three-step Verifier, a component-bound Verifier and a drift Verifier nothing binds.
  const receipt = await cards.declareFromPath(fixture('cards/verifier_building_blocks/policy-desk.yaml'));
  const desk = exact(declared(receipt));
  const review = exact(declared(receipt, 'policy-review'));
  const promise = exact(declared(await cards.declareFromPath(fixture('cards/declare_and_pull/support-desk.yaml')), 'no-refund-promise'));
  const drift = exact(declared(await cards.declareFromPath(fixture('cards/mcp_investigation/drift-verifier.yaml'))));
  const tenant = 'ui-verifier-foreign';
  const foreignKey = server.bootstrapServiceInTenant(server.seedTenant(tenant), ['admin'], 'ui-verifier-admin');

  const client = WyrdClient.connect();
  const mcp = await mcpOver(server.baseUrl, client);
  try {
    for (const [surface, executor] of [['node', nodeExecutor(client)], ['mcp', mcp.executor]] as const) {
      const context = { executor };
      // The declared graph: every input, step and requirement, and the dependencies its references name.
      const definition = await loadVerifierDefinition({ cardRef: review }, context);
      if (definition.result.state !== 'populated') throw new Error(`${surface}: ${JSON.stringify(definition.result)}`);
      const { graph, spec } = definition.result.data;
      expect(spec.description).toBe('A support answer follows its refund policy and was looked up, not guessed.');
      expect(graph.inputs.map((input) => [input.id, input.kind])).toEqual([['answer', 'json'], ['policy', 'media'], ['trace', 'trace']]);
      expect(graph.steps.map((step) => [step.id, step.kind, step.needs, step.requirements.map((each) => each.id)])).toEqual([
        ['policy-text', 'extract', [], ['policy-has-text']],
        ['refund-rules', 'agent', ['policy-text'], ['limit-found']],
        ['answer-facts', 'code', ['refund-rules'], []]
      ]);
      expect(graph.requirements.map((each) => each.id)).toEqual(['refund-within-limit', 'policy-looked-up', 'answer-drafted-after-lookup', 'lookup-read-refunds', 'follows-policy', 'names-the-policy']);
      expect(graph.edges).toHaveLength(13);
      expect(graph.edges).toEqual(
        expect.arrayContaining([
          { source: 'input:policy', target: 'step:policy-text' },
          { source: 'step:answer-facts', target: 'req:refund-within-limit', label: 'within_limit' },
          { source: 'input:trace', target: 'req:policy-looked-up' },
          { source: 'step:policy-text', target: 'step:refund-rules', label: 'outline' },
          { source: 'step:refund-rules', target: 'req:follows-policy', label: 'rules' }
        ])
      );
      // Drift-shaped: one samples input judged by one whole-Verifier requirement.
      expect((await loadVerifierDefinition({ cardRef: drift }, context)).result).toMatchObject({
        state: 'populated',
        data: { graph: { inputs: [{ id: 'latency', kind: 'samples' }], steps: [], requirements: [{ kind: 'drift' }], edges: [{ source: 'input:latency', target: 'req:latency-under-threshold' }] } }
      });

      // Where it runs: the Service itself, one component of another Service, and nothing.
      expect((await loadRunsOn({ cardRef: review }, context)).result).toMatchObject({
        state: 'populated',
        data: [{ ref: { kind: 'Service', name: 'policy-desk', uid: desk.uid }, bindings: { state: 'populated', data: [{ when: 'explicit invocation only' }] } }]
      });
      expect((await loadRunsOn({ cardRef: promise }, context)).result).toMatchObject({
        state: 'populated',
        data: [{ ref: { kind: 'Service', name: 'support-desk' }, bindings: { state: 'populated', data: [{ subject: 'agent', when: 'explicit invocation only' }] } }]
      });
      expect((await loadRunsOn({ cardRef: drift }, context)).result).toMatchObject({ state: 'empty', reason: 'No binding runs this exact version.' });

      // A missing version and a Card of another kind are refused, never replaced by another version.
      for (const load of [loadVerifierDefinition, loadRunsOn]) {
        expect((await load({ cardRef: { ...review, version: '9.9.9', uid: undefined } }, context)).result).toMatchObject({
          state: 'error',
          problem: { code: 'WYRD_REGISTRY_404_CARD_NOT_FOUND' }
        });
        expect((await load({ cardRef: desk }, context)).result).toMatchObject({
          state: 'error',
          problem: { code: 'WYRD_SPEC_400_VALIDATION', detail: 'policy-desk@1.0.0 is a Service Card, not a Verifier' }
        });
      }
    }
  } finally {
    await mcp.close();
  }

  // A principal without card permission sees both denied after one read each.
  const runs: WireRequest[][] = [];
  const queryOnly = nodeExecutor(WyrdClient.connect({ credential: server.scopedApiKey('ui_verifier_denied', ['bifrost_query:read']) }));
  const denied: { executor: Executor } = { executor: { run: ((requests, signal) => (runs.push([...requests]), queryOnly.run(requests, signal))) as Executor['run'] } };
  for (const loaded of [await loadVerifierDefinition({ cardRef: review }, denied), await loadRunsOn({ cardRef: review }, denied)])
    expect(loaded.result).toMatchObject({ state: 'denied', problem: { code: DENIED } });
  expect(runs.map((requests) => requests.length)).toEqual([1, 1]);

  // BFF: another tenant's browser session cannot read this tenant's Verifier.
  env.WYRD_SERVER_URL = server.baseUrl;
  const url = new URL(`http://ui.test/t/${tenant}/ui/load`);
  const cookies = jar();
  await browserSessions.signInWithApiKey(tenant, foreignKey, url, cookies);
  const browserSession = await browserSessions.read(tenant, url, cookies);
  for (const component of ['VerifierDefinition', 'RunsOn']) {
    const response = await loadPanel({
      locals: { browserSession },
      request: new Request(url, { method: 'POST', body: JSON.stringify({ component, input: { cardRef: review }, range: RANGE }), headers: { 'content-type': 'application/json' } })
    } as never);
    expect(await response.json(), component).toMatchObject({ result: { state: 'error', problem: { code: 'WYRD_REGISTRY_404_CARD_NOT_FOUND' } } });
  }

  // Custom app: its server loads the same props with WYRD_API_KEY and never returns the key.
  for (const component of ['VerifierDefinition', 'RunsOn']) {
    const response = await loadExamplePanel({
      request: new Request('http://app.test/view/load', { method: 'POST', body: JSON.stringify({ component, input: { cardRef: review }, range: RANGE }) })
    } as never);
    const text = await response.text();
    expect(text).not.toContain(server.apiKey);
    expect(JSON.parse(text), component).toMatchObject({ result: { state: 'populated' } });
  }

  // The built HTML draws the registered declaration and its binding through host tools/call alone.
  const [verifier, service] = await nodeExecutor(client).run([
    { op: 'cards.get_by_ref', card_ref: review },
    { op: 'cards.get_by_ref', card_ref: desk }
  ]);
  if (!verifier.ok || !service.ok) throw new Error('the registered Cards did not read back');
  const app = await appBridgeView(
    {
      version: 'wyrd.view/v1',
      catalogId: 'wyrd.ui/v1',
      range: RANGE,
      panels: [
        { id: 'definition', component: 'VerifierDefinition', input: { cardRef: review } },
        { id: 'runs', component: 'RunsOn', input: { cardRef: review } }
      ]
    },
    (name, args) => (name === 'cards.get' ? { card: (args.card_ref as { kind: string }).kind === 'Service' ? service.value : verifier.value } : notFound)
  );
  await vi.waitFor(() => expect(app.document.querySelector('[data-panel="runs"]')?.textContent).toContain('policy-desk'));
  const drawn = app.document.querySelector('[data-panel="definition"]') as Element;
  await vi.waitFor(() => expect(drawn.querySelector('[data-node]')).not.toBeNull());
  expect([...drawn.querySelectorAll('[data-node]')].map((node) => node.getAttribute('data-node'))).toEqual(
    expect.arrayContaining(['input:policy', 'step:refund-rules', 'req:limit-found', 'req:follows-policy'])
  );
  expect(drawn.querySelectorAll('[data-edge]')).toHaveLength(13);
  // Both panels read the Verifier; the view issues that identical read once, then the owner's.
  expect(app.calls.map((call) => call.name)).toEqual(['cards.get', 'cards.get', 'cards.get']);
});

/** The populated data of one execution load, or its refusal as a failure. */
function data(loaded: Awaited<ReturnType<typeof loadExecution>>, surface: string) {
  if (loaded.result.state !== 'populated') throw new Error(`${surface}: ${JSON.stringify(loaded.result)}`);
  return loaded.result.data;
}

test('an unfinished execution shows recorded progress and retained outputs through every authorized host, and cancels once', async ({ server, seeded: _seeded }) => {
  const { executionId, verifier } = server.holdPublications('ui-held');
  server.flushBifrost();
  const attempt = { item_index: null, iteration_index: null, attempt: 1 };
  const tree = { executionId, node: 'step:draft', producer: attempt, output: 'tree' };

  const client = WyrdClient.connect();
  const mcp = await mcpOver(server.baseUrl, client);
  try {
    for (const [surface, executor] of [['node', nodeExecutor(client)], ['mcp', mcp.executor]] as const) {
      const context = { executor };
      // Running, with no result: the Verifier it ran is found by its recorded UID, never by name.
      const open = data(await loadExecution({ executionId, details: true }, context), surface);
      expect([open.status.status, open.status.result], surface).toEqual(['running', undefined]);
      expect(open.definition, surface).toMatchObject({ state: 'populated', data: { ref: { kind: 'Verifier', uid: verifier }, graph: { requirements: [{ kind: 'drift' }] } } });
      expect(open.publications.state === 'populated' && open.publications.data.items.map((item) => [item.producer.step_id, item.name, item.kind]), surface).toEqual([
        ['review', 'verdict', 'json'],
        ['review', 'nothing', 'json'],
        ['draft', 'report', 'file'],
        ['draft', 'tree', 'directory']
      ]);
      expect(open.invocations.state, surface).toBe('populated');
      // No result yet: completed details are not read and say why.
      expect(open.details, surface).toEqual({ state: 'empty', reason: 'Details are recorded when the execution completes with a visible result.' });
      expect(open.publication, surface).toBeUndefined();

      // One exact intermediate publication of one producer attempt, and a recorded null kept apart from no output.
      const selected = data(await loadExecution(tree, context), surface);
      expect(selected.publication, surface).toMatchObject({
        state: 'populated',
        data: { name: 'tree', kind: 'directory', producer: { step_id: 'draft', attempt: 1 }, manifest: { files: [{ path: 'a.md' }, { path: 'nested/b.md' }], directories: ['nested'] } }
      });
      expect(data(await loadExecution({ ...tree, node: 'step:review', output: 'nothing' }, context), surface).publication, surface).toMatchObject({ state: 'populated', data: { kind: 'json', value: null } });
      // A retained text file previews as text where the host can read file content; an MCP host has no such tool and says so, keeping the manifest.
      const report = data(await loadExecution({ ...tree, output: 'report' }, context), surface);
      expect(report.publication, surface).toMatchObject({ state: 'populated', data: { kind: 'file', manifest: { files: [{ path: 'report', size_bytes: 9 }] } } });
      if (surface === 'node') expect(report.text, surface).toEqual({ state: 'populated', data: { text: '# Report\n', truncated: false } });
      else expect(report.text?.state, surface).toBe('error');
      // Another attempt and an unknown output are refusals of that part only.
      for (const missing of [{ ...tree, producer: { ...attempt, attempt: 2 } }, { ...tree, output: 'absent' }]) {
        const loaded = data(await loadExecution(missing, context), surface);
        expect(loaded.publication, surface).toMatchObject({ state: 'error', problem: { status: 404 } });
        expect(loaded.publications.state, surface).toBe('populated');
      }
      // An execution that does not exist ends the load at its first read.
      expect((await loadExecution({ executionId: '0192f0a0-0000-7000-8000-000000000000' }, context)).result, surface).toMatchObject({
        state: 'error',
        problem: { code: 'WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND' }
      });
    }
  } finally {
    await mcp.close();
  }

  // A principal that may query but not read executions is denied after one read; nothing dependent is asked.
  const runs: WireRequest[][] = [];
  const restrictedKey = server.scopedApiKey('ui_execution_denied', ['bifrost_query:read']);
  const queryOnly = nodeExecutor(WyrdClient.connect({ credential: restrictedKey }));
  const denied: { executor: Executor } = { executor: { run: ((requests, signal) => (runs.push([...requests]), queryOnly.run(requests, signal))) as Executor['run'] } };
  expect((await loadExecution(tree, denied)).result).toMatchObject({ state: 'denied', problem: { code: DENIED } });
  expect(runs.map((requests) => requests.map((request) => request.op))).toEqual([['verification.get_execution']]);

  // BFF: this tenant's browser session loads the same props; another tenant's finds no such execution.
  env.WYRD_SERVER_URL = server.baseUrl;
  const session = async (tenant: string, key: string) => {
    const url = new URL(`http://ui.test/t/${tenant}/ui/load`);
    const cookies = jar();
    await browserSessions.signInWithApiKey(tenant, key, url, cookies);
    return { url, browserSession: await browserSessions.read(tenant, url, cookies) };
  };
  const panel = async ({ url, browserSession }: Awaited<ReturnType<typeof session>>) =>
    (
      await loadPanel({
        locals: { browserSession },
        request: new Request(url, { method: 'POST', body: JSON.stringify({ component: 'VerificationExecution', input: tree, range: RANGE }), headers: { 'content-type': 'application/json' } })
      } as never)
    ).json();
  const own = await session('test-tenant-1', server.apiKey);
  const viewer = await session('test-tenant-1', server.bootstrapService(['viewer'], 'ui-execution-viewer'));
  const foreignKey = server.bootstrapServiceInTenant(server.seedTenant('ui-execution-foreign'), ['admin'], 'ui-execution-admin');
  const foreign = await session('ui-execution-foreign', foreignKey);
  expect(await panel(own)).toMatchObject({
    result: { state: 'populated', data: { status: { status: 'running' }, definition: { state: 'populated', data: { ref: { uid: verifier } } }, publication: { state: 'populated', data: { kind: 'directory' } } } }
  });
  expect(await panel(foreign)).toMatchObject({ result: { state: 'error', problem: { code: 'WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND' } } });

  // The host download leaves as an attachment the browser may not sniff; another tenant's session gets the server's refusal.
  const event = (from: Awaited<ReturnType<typeof session>>, path: string) =>
    ({ locals: { browserSession: from.browserSession }, params: { tenantKey: 'test-tenant-1', executionId }, url: new URL(`http://ui.test/x/${path}`) }) as never;
  const file = await downloadPublication(event(own, 'download?step=draft&name=tree&attempt=1&path=nested/b.md'));
  expect([file.status, file.headers.get('content-type'), file.headers.get('content-disposition'), file.headers.get('x-content-type-options'), await file.text()]).toEqual([
    200,
    'application/octet-stream',
    "attachment; filename*=UTF-8''b.md",
    'nosniff',
    'beta'
  ]);
  expect((await downloadPublication(event(foreign, 'download?step=draft&name=report&attempt=1'))).status).toBe(404);
  // A JSON value is not a file: the existing operation refuses it and the host relays that.
  expect((await downloadPublication(event(own, 'download?step=review&name=verdict&attempt=1'))).status).toBe(400);

  // Cancel: a viewer and another tenant are refused and the execution keeps running; the owner's cancel is recorded once and a repeat keeps it.
  expect(await (await cancelExecution(event(viewer, 'cancel'))).json()).toMatchObject({ code: DENIED, status: 403 });
  expect(await (await cancelExecution(event(foreign, 'cancel'))).json()).toMatchObject({ code: 'WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND', status: 404 });
  expect(await panel(own)).toMatchObject({ result: { data: { status: { status: 'running' } } } });
  for (const _ of [1, 2]) expect(await (await cancelExecution(event(own, 'cancel'))).json()).toEqual({ execution_id: executionId, status: 'cancelled' });

  // Custom app, after settlement: the same execution, cancelled with no result, still lists what it retained.
  const response = await loadExamplePanel({
    request: new Request('http://app.test/view/load', { method: 'POST', body: JSON.stringify({ component: 'VerificationExecution', input: tree, range: RANGE }) })
  } as never);
  const text = await response.text();
  expect(text).not.toContain(server.apiKey);
  const settled = JSON.parse(text) as Awaited<ReturnType<typeof loadExecution>>;
  expect(settled).toMatchObject({ result: { state: 'populated', data: { status: { status: 'cancelled' }, publication: { state: 'populated', data: { kind: 'directory' } } } } });
  expect(data(settled, 'app').publications).toMatchObject({ state: 'populated', data: { items: [{}, {}, {}, {}] } });
  // Recorded progress stays readable after the cancel, through Node and the custom app alike.
  const afterCancel = data(await loadExecution({ executionId }, { executor: nodeExecutor(client) }), 'node after cancel');
  expect(afterCancel.invocations.state).toBe('populated');
  expect(data(settled, 'app').invocations).toEqual(afterCancel.invocations);

  // An execution the app's credential cannot find is the server's refusal, not an empty panel.
  const missing = await loadExamplePanel({
    request: new Request('http://app.test/view/load', { method: 'POST', body: JSON.stringify({ component: 'VerificationExecution', input: { executionId: '0192f0a0-0000-7000-8000-000000000000' }, range: RANGE }) })
  } as never);
  expect(await missing.json()).toMatchObject({ result: { state: 'error', problem: { code: 'WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND' } } });

  // Run as another tenant's key, the app finds no such execution.
  vi.stubEnv('WYRD_API_KEY', foreignKey);
  const other = await loadExamplePanel({
    request: new Request('http://app.test/view/load', { method: 'POST', body: JSON.stringify({ component: 'VerificationExecution', input: tree, range: RANGE }) })
  } as never);
  vi.stubEnv('WYRD_API_KEY', server.apiKey);
  expect(await other.json()).toMatchObject({ result: { state: 'error', problem: { code: 'WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND' } } });

  // The built HTML draws that execution through host tools/call alone, each answered by the real MCP server.
  const host = new Client({ name: 'wyrd-ui-artifact-host', version: '1.0.0' }, { versionNegotiation: { mode: 'auto' } });
  await host.connect(new StreamableHTTPClientTransport(new URL('/mcp', server.baseUrl), { fetch: client.httpAuth }));
  try {
    const app = await appBridgeView(
      { version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: RANGE, panels: [{ id: 'execution', component: 'VerificationExecution', input: tree }] },
      async (name, args) => ((await host.callTool({ name, arguments: args })).structuredContent ?? {}) as object
    );
    const drawn = () => app.document.querySelector('[data-panel="execution"]') as Element;
    await vi.waitFor(() => expect(drawn().querySelector('[data-execution-status]')?.getAttribute('data-execution-status')).toBe('cancelled'));
    await vi.waitFor(() => expect(drawn().querySelector('[data-node]')).not.toBeNull());
    expect(drawn().querySelector('[data-preview]')?.textContent).toContain('nested/b.md');
    expect(drawn().textContent).not.toMatch(/passed|failed/);
    expect(new Set(app.calls.map((call) => call.name))).toEqual(
      new Set(['verification.get_execution', 'bifrost.query', 'cards.get', 'verification.list_invocations', 'verification.list_publications', 'verification.get_publication'])
    );
    // The same artifact shows the MCP server's refusal of an unknown execution and asks for nothing else.
    const refusing = await appBridgeView(
      { version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: RANGE, panels: [{ id: 'execution', component: 'VerificationExecution', input: { executionId: '0192f0a0-0000-7000-8000-000000000000' } }] },
      async (name, args) => (await host.callTool({ name, arguments: args })) as object
    );
    await vi.waitFor(() => expect(refusing.document.querySelector('[data-panel="execution"]')?.textContent).toContain('WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND'));
    expect(refusing.calls.map((call) => call.name)).toEqual(['verification.get_execution']);
  } finally {
    await host.close();
  }

  // Answered by the MCP server as another tenant, the artifact shows that refusal for the real execution and reads nothing further.
  const outsider = new Client({ name: 'wyrd-ui-artifact-outsider', version: '1.0.0' }, { versionNegotiation: { mode: 'auto' } });
  await outsider.connect(new StreamableHTTPClientTransport(new URL('/mcp', server.baseUrl), { fetch: WyrdClient.connect({ credential: foreignKey }).httpAuth }));
  try {
    const refused = await appBridgeView(
      { version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: RANGE, panels: [{ id: 'execution', component: 'VerificationExecution', input: tree }] },
      async (name, args) => (await outsider.callTool({ name, arguments: args })) as object
    );
    await vi.waitFor(() => expect(refused.document.querySelector('[data-panel="execution"]')?.textContent).toContain('WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND'));
    expect(refused.calls.map((call) => call.name)).toEqual(['verification.get_execution']);
  } finally {
    await outsider.close();
  }

  // The restricted principal is refused the same way through the remaining hosts: denied, never empty, and nothing asked after the one read.
  const refusal = { result: { state: 'denied', problem: { code: DENIED } } };
  expect(await panel(await session('test-tenant-1', restrictedKey))).toMatchObject(refusal);
  vi.stubEnv('WYRD_API_KEY', restrictedKey);
  const appDenied = await loadExamplePanel({
    request: new Request('http://app.test/view/load', { method: 'POST', body: JSON.stringify({ component: 'VerificationExecution', input: tree, range: RANGE }) })
  } as never);
  vi.stubEnv('WYRD_API_KEY', server.apiKey);
  expect(await appDenied.json()).toMatchObject(refusal);
  const restricted = new Client({ name: 'wyrd-ui-artifact-restricted', version: '1.0.0' }, { versionNegotiation: { mode: 'auto' } });
  await restricted.connect(new StreamableHTTPClientTransport(new URL('/mcp', server.baseUrl), { fetch: WyrdClient.connect({ credential: restrictedKey }).httpAuth }));
  try {
    const asked: string[][] = [];
    const tools = mcpExecutor({ callServerTool: (params, options) => restricted.callTool(params, options) });
    const counted: Executor = { run: ((requests, signal) => (asked.push(requests.map((request) => request.op)), tools.run(requests, signal))) as Executor['run'] };
    expect(await loadExecution(tree, { executor: counted })).toMatchObject(refusal);
    expect(asked).toEqual([['verification.get_execution']]);
    const drawn = await appBridgeView(
      { version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: RANGE, panels: [{ id: 'execution', component: 'VerificationExecution', input: tree }] },
      async (name, args) => (await restricted.callTool({ name, arguments: args })) as object
    );
    await vi.waitFor(() => expect(drawn.document.querySelector('[data-panel="execution"]')?.textContent).toContain(DENIED));
    expect(drawn.calls.map((call) => call.name)).toEqual(['verification.get_execution']);
  } finally {
    await restricted.close();
  }

  // A server that cannot be reached is a problem through every host: never an empty panel, never recorded state.
  const DOWN = 'http://127.0.0.1:9';
  const unreachable = (surface: string, loaded: unknown) => {
    expect(loaded, surface).toMatchObject({ result: { state: 'error', problem: { code: expect.stringMatching(/^WYRD_/) } } });
    // Unreachable is not "no such execution".
    expect(JSON.stringify(loaded), surface).not.toContain('WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND');
  };
  unreachable('node', await loadExecution(tree, { executor: nodeExecutor(WyrdClient.connect({ serverUrl: DOWN, credential: server.apiKey })) }));
  const gone = await mcpOver(server.baseUrl, client);
  await gone.close();
  unreachable('mcp', await loadExecution(tree, { executor: gone.executor }));
  env.WYRD_SERVER_URL = DOWN;
  unreachable('bff', await panel(own));
  env.WYRD_SERVER_URL = server.baseUrl;
  vi.stubEnv('WYRD_SERVER_URL', DOWN);
  const appDown = await loadExamplePanel({
    request: new Request('http://app.test/view/load', { method: 'POST', body: JSON.stringify({ component: 'VerificationExecution', input: tree, range: RANGE }) })
  } as never);
  vi.stubEnv('WYRD_SERVER_URL', server.baseUrl);
  unreachable('app', await appDown.json());
  const dark = await appBridgeView({ version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: RANGE, panels: [{ id: 'execution', component: 'VerificationExecution', input: tree }] }, async () => {
    throw new Error('the host cannot reach its server');
  });
  await vi.waitFor(() => expect(dark.document.querySelector('[data-panel="execution"]')?.textContent).toMatch(/WYRD_[A-Z0-9_]+/));
  expect(dark.document.querySelector('[data-panel="execution"]')?.textContent).not.toContain('WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND');
  expect(dark.document.querySelector('[data-execution-status]')).toBeNull();
});

/** A fresh server that runs Verifiers, with the assistant Service registered. */
const judged = assistantTest({ perTest: true });

judged('a judged execution shows its judgments, accepted inputs, completed details, drift and session through the Node and MCP hosts', async ({ server, assistant, state }) => {
  const cards = await Cards.connect();
  const token = await WyrdClient.connect().accessToken();
  /** The one binding of `owner` whose status carries `field`: a schedule or a session end. */
  const bindingWith = async (owner: Exact, field: string) => {
    for (const id of (await cards.get(owner)).status?.verification?.binding_ids ?? []) {
      const response = await fetch(`${server.baseUrl}/v1/verification/bindings/${id}`, { headers: { 'x-wyrd-access-token': `Bearer ${token}` } });
      if (((await response.json()) as Record<string, unknown>)[field]) return id;
    }
    throw new Error(`${owner.name} has no binding with ${field}`);
  };
  const submit = async (verifier: string, answer: string) => {
    const submitted = await state.run('agent').observe.submit(verifier, { answer });
    if (submitted.kind !== 'queued') throw new Error(`${verifier} queues an execution`);
    await submitted.execution.wait({ timeoutMs: 60_000 });
    return submitted.execution.id;
  };
  // Two Run executions: a failed assertion and a judged answer that cites its input.
  const refused = await submit('answer-check', 'no');
  const agreed = await submit('answer-judged', 'yes');

  // A scheduled drift window far from its fitted baseline.
  const latency = (await cards.get({ kind: 'Verifier', name: 'latency-drift', version: '1.0.0' })).metadata.uid as string;
  server.waitForBaseline(latency, 90_000);
  await state.startBifrost();
  const model = state.run('model');
  for (let sample = 0; sample < 100; sample += 1) await model.observe.submit('latency-drift', { latency: 99 });
  await state.shutdown();
  server.flushBifrost();
  const schedule = await bindingWith(exact(assistant.service), 'next_run_at');
  server.makeBindingDue(schedule);
  const drifted = (await scheduledResult(server, schedule, 90_000)).execution_id;

  // A session: three turns of one conversation, judged together when it ends.
  const support = await cards.declareFromPath(fixture('cards/verify_in_real_time/support.yaml'));
  const bundle = mkdtempSync(join(tmpdir(), 'wyrd-ui-session-'));
  await cards.pull(support.root, bundle);
  const { key } = await runtimeAccount({ kind: 'Service', name: support.root.name });
  const desk = WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential: key }) });
  await desk.startBifrost();
  const conversation = desk.run('agent');
  for (const turn of [1, 2, 3]) await conversation.observe.verify('turn-check', { turn, answer: `answer ${turn}` }, { positional: true, group_id: 'ui-conversation' });
  await desk.shutdown();
  const session = await groupExecution(server, await bindingWith(exact(declared(support)), 'group_end'), 90_000);
  server.flushBifrost();

  const client = WyrdClient.connect();
  const mcp = await mcpOver(server.baseUrl, client);
  try {
    for (const [surface, executor] of [['node', nodeExecutor(client)], ['mcp', mcp.executor]] as const) {
      const context = { executor };
      // A failed execution opens on its first failed requirement, with the judgment the server recorded.
      const failed = await loadExecution({ executionId: refused, details: true }, context);
      expect(failed.input.node, surface).toBe('req:says-yes');
      const open = data(failed, surface);
      expect(open.status, surface).toMatchObject({
        status: 'completed',
        result: { verifier: { name: 'answer-check', uid: assistant.answerCheck.uid }, outcome: { verdict: 'failed', requirements: [{ id: 'says-yes', outcome: 'failed', reason: 'operator Equals returned false', actual: 'no' }] } }
      });
      expect(open.definition, surface).toMatchObject({ state: 'populated', data: { ref: { uid: assistant.answerCheck.uid }, graph: { edges: [{ source: 'input:answer', target: 'req:says-yes' }] } } });
      expect(open.invocations, surface).toMatchObject({
        state: 'populated',
        data: { items: [{ producer: { step_id: 'says-yes', attempt: 1 }, state: 'requirement', outcome: 'failed' }], history: { pending: 0, failed: 0 } }
      });
      expect(open.publications, surface).toMatchObject({ state: 'populated', data: { items: [] } });
      expect(open.details, surface).toMatchObject({ state: 'populated', data: { execution_id: refused, outcome: { requirements: [{ id: 'says-yes' }] } } });
      // Details are read only when asked for; the accepted input is read for its selected node only.
      const input = data(await loadExecution({ executionId: refused, node: 'input:answer' }, context), surface);
      expect([input.details, input.inputs], surface).toEqual([undefined, { state: 'populated', data: { kind: 'json', value: 'no' } }]);
      expect(open.inputs, surface).toBeUndefined();

      // A judged requirement keeps its reason and the citation into the input it judged.
      const cited = data(await loadExecution({ executionId: agreed, node: 'req:agrees' }, context), surface);
      expect(cited.status.result?.outcome, surface).toMatchObject({
        verdict: 'passed',
        requirements: [{ id: 'agrees', outcome: 'passed', reason: 'it agrees', citations: [{ slot: 'answer', locator: { json_pointer: '' } }] }]
      });

      // Drift: the report of this execution, with its binding's retained history ending at this window.
      const drift = data(await loadExecution({ executionId: drifted }, context), surface);
      expect(drift.status.result, surface).toMatchObject({ verifier: { name: 'latency-drift' }, outcome: { verdict: 'failed', requirements: [{ id: 'latency-stable', outcome: 'failed' }] } });
      expect(drift.status.result?.outcome.requirements[0]?.detail?.drift.features.latency?.score ?? 0, surface).toBeGreaterThan(0.25);
      const history = drift.drift?.state === 'populated' ? drift.drift.data.history : [];
      expect(history.map((point) => point.execution_id), surface).toEqual([drifted]);
      expect(history[0]?.scores.latency ?? 0, surface).toBeGreaterThan(0.25);
      expect(drift.drift?.state === 'populated' && drift.drift.data.window_end, surface).toEqual(expect.any(String));
      // A requirement that is not drift reads no drift history.
      expect(open.drift, surface).toBeUndefined();

      // Session: the group execution names its owner and its members, each read as its own exact execution.
      const group = data(await loadExecution({ executionId: session.execution_id, node: 'input:turns' }, context), surface);
      expect(group.status, surface).toMatchObject({ group_id: 'ui-conversation', owner_card_uid: declared(support).uid, result: { outcome: { verdict: 'passed' } } });
      expect(group.members?.map((member) => [member.execution_id, member.status?.state === 'populated' && member.status.data.status]), surface).toEqual(
        (session.group?.sources.turns.execution_ids ?? []).map((id) => [id, 'completed'])
      );
      expect(group.members, surface).toHaveLength(3);
      expect(group.inputs, surface).toBeUndefined();
    }
  } finally {
    await mcp.close();
  }

  // Cancel after completion: the terminal state wins, and the judged result stays readable.
  const late = await client.httpAuth(`${server.baseUrl}/v1/verification/executions/${refused}/cancel`, { method: 'POST' });
  expect([late.status, await late.json()]).toEqual([200, { execution_id: refused, status: 'completed' }]);
  expect(data(await loadExecution({ executionId: refused }, { executor: nodeExecutor(client) }), 'late').status.result?.outcome.verdict).toBe('failed');

  // A turn observed after the session was judged joins no recorded membership: the judged execution keeps its three pinned members and verdict.
  const later = WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential: key }) });
  await later.startBifrost();
  await later.run('agent').observe.verify('turn-check', { turn: 4, answer: 'answer 4' }, { positional: true, group_id: 'ui-conversation' });
  await later.shutdown();
  server.flushBifrost();
  const pinned = data(await loadExecution({ executionId: session.execution_id, node: 'input:turns' }, { executor: nodeExecutor(client) }), 'later turn');
  expect(pinned.members?.map((member) => member.execution_id)).toEqual(session.group?.sources.turns.execution_ids);
  expect(pinned.status.result?.outcome.verdict).toBe('passed');

  // A second drift window of the same binding: its history holds both windows, oldest first, ending at the viewed one; the first window's own history is unchanged.
  const sampler = WyrdState.fromPath(assistant.bundle, { client: WyrdClient.connect({ credential: assistant.key }) });
  await sampler.startBifrost();
  for (let sample = 0; sample < 100; sample += 1) await sampler.run('model').observe.submit('latency-drift', { latency: 99 });
  await sampler.shutdown();
  server.flushBifrost();
  server.makeBindingDue(schedule);
  const windows = async () => {
    server.flushBifrost();
    const [read] = await nodeExecutor(client).run([{ op: 'bifrost.query', sql: 'SELECT execution_id FROM bifrost.verification.results WHERE binding_id = $1 ORDER BY ended_at', params: [schedule], max_rows: 10 }]);
    return read.ok ? read.value.rows.map(([id]) => String(id)) : [];
  };
  await vi.waitFor(async () => expect(await windows()).toHaveLength(2), { timeout: 90_000, interval: 1_000 });
  const [, second] = await windows();
  for (const [surface, executor] of [['node', nodeExecutor(client)], ['mcp', (await mcpOver(server.baseUrl, client)).executor]] as const) {
    const points = async (executionId: string) => {
      const loaded = data(await loadExecution({ executionId }, { executor }), surface).drift;
      return loaded?.state === 'populated' ? loaded.data.history.map((point) => point.execution_id) : [];
    };
    expect(await points(second), surface).toEqual([drifted, second]);
    expect(await points(drifted), surface).toEqual([drifted]);
  }

  // Another tenant reads none of them, by ID alone, through either host.
  const outsider = WyrdClient.connect({ credential: server.bootstrapServiceInTenant(server.seedTenant('ui-judged-foreign'), ['admin'], 'ui-judged-admin') });
  // A principal that may read executions only: the recorded state loads, and the Card and query reads are each denied apart.
  const limited = WyrdClient.connect({ credential: server.scopedApiKey('ui_judged_narrow', ['verifier:read']) });
  const [foreignMcp, narrowMcp] = [await mcpOver(server.baseUrl, outsider), await mcpOver(server.baseUrl, limited)];
  try {
    for (const [surface, foreign, narrow] of [['node', nodeExecutor(outsider), nodeExecutor(limited)], ['mcp', foreignMcp.executor, narrowMcp.executor]] as const) {
      for (const executionId of [refused, drifted, session.execution_id])
        expect((await loadExecution({ executionId }, { executor: foreign })).result, surface).toMatchObject({ state: 'error', problem: { code: 'WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND' } });
      const partial = data(await loadExecution({ executionId: refused, node: 'input:answer' }, { executor: narrow }), surface);
      expect(partial.status.result?.outcome.verdict, surface).toBe('failed');
      expect(partial.invocations.state, surface).toBe('populated');
      expect([partial.definition, partial.inputs], surface).toMatchObject([
        { state: 'denied', problem: { code: DENIED } },
        { state: 'denied', problem: { code: DENIED } }
      ]);
    }
  } finally {
    await foreignMcp.close();
    await narrowMcp.close();
  }
});

judged('Verify lists executions with and without a recorded result as one authorized population through every host', async ({ server, assistant, state }) => {
  const cards = await Cards.connect();
  const client = WyrdClient.connect();
  const token = await client.accessToken();
  const bindingWith = async (owner: Exact, field: string) => {
    for (const id of (await cards.get(owner)).status?.verification?.binding_ids ?? []) {
      const response = await fetch(`${server.baseUrl}/v1/verification/bindings/${id}`, { headers: { 'x-wyrd-access-token': `Bearer ${token}` } });
      if (((await response.json()) as Record<string, unknown>)[field]) return id;
    }
    throw new Error(`${owner.name} has no binding with ${field}`);
  };
  const submit = async (verifier: string, answer: string) => {
    const submitted = await state.run('agent').observe.submit(verifier, { answer });
    if (submitted.kind !== 'queued') throw new Error(`${verifier} queues an execution`);
    await submitted.execution.wait({ timeoutMs: 60_000 });
    return submitted.execution.id;
  };
  // Ordinary Run executions: one failed, one passed.
  const refused = await submit('answer-check', 'no');
  const agreed = await submit('answer-judged', 'yes');
  // Drift: one scheduled window far from its fitted baseline.
  const latency = (await cards.get({ kind: 'Verifier', name: 'latency-drift', version: '1.0.0' })).metadata.uid as string;
  server.waitForBaseline(latency, 90_000);
  await state.startBifrost();
  for (let sample = 0; sample < 100; sample += 1) await state.run('model').observe.submit('latency-drift', { latency: 99 });
  await state.shutdown();
  server.flushBifrost();
  const schedule = await bindingWith(exact(assistant.service), 'next_run_at');
  server.makeBindingDue(schedule);
  const drifted = (await scheduledResult(server, schedule, 90_000)).execution_id;
  // A session: three direct turns of one conversation, then the group execution that judges them together.
  const support = await cards.declareFromPath(fixture('cards/verify_in_real_time/support.yaml'));
  const bundle = mkdtempSync(join(tmpdir(), 'wyrd-ui-verify-'));
  await cards.pull(support.root, bundle);
  const { key } = await runtimeAccount({ kind: 'Service', name: support.root.name });
  const desk = WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential: key }) });
  await desk.startBifrost();
  for (const turn of [1, 2, 3]) await desk.run('agent').observe.verify('turn-check', { turn, answer: `answer ${turn}` }, { positional: true, group_id: 'ui-verify-conversation' });
  await desk.shutdown();
  const session = await groupExecution(server, await bindingWith(exact(declared(support)), 'group_end'), 90_000);
  const turns = session.group?.sources.turns.execution_ids ?? [];
  // A hosted Verifier, whose step can be held waiting for memory, and one execution cancelled while held.
  const capacity = await cards.declareFromPath(fixture('cards/verification_capacity/capacity-desk.yaml'));
  const capacityBundle = mkdtempSync(join(tmpdir(), 'wyrd-ui-verify-capacity-'));
  await cards.pull(capacity.root, capacityBundle);
  const capacityRoot = declared(capacity);
  const counter = WyrdState.fromPath(capacityBundle, { client: WyrdClient.connect({ credential: (await runtimeAccount({ kind: 'Service', name: capacityRoot.name })).key, workloadUid: capacityRoot.uid }) });
  await counter.startBifrost();
  const stopped = server.holdPublications('ui-verify-stopped').executionId;
  expect((await client.httpAuth(`${server.baseUrl}/v1/verification/executions/${stopped}/cancel`, { method: 'POST' })).status).toBe(200);
  server.flushBifrost();

  const range = { from: new Date(Date.now() - 3_600_000).toISOString(), to: new Date(Date.now() + 60_000).toISOString() };
  const named = (read: QueryResult) => read.rows.map((row) => Object.fromEntries(read.columns.map((column, at) => [column.name, row[at]])));
  const total = (read: QueryResult) => read.rows.reduce((sum, row) => sum + Number(row[row.length - 1]), 0);
  /** One page of the list as named rows, or the reason it is empty. */
  const page = async (input: Record<string, unknown>, executor: Executor) => {
    const loaded = await loadRecordTable({ signal: 'executions', ...input } as never, { executor, range });
    if (loaded.result.state === 'empty') return { rows: [], older: undefined, reason: loaded.result.reason };
    if (loaded.result.state !== 'populated') throw new Error(JSON.stringify(loaded.result));
    return { rows: named(loaded.result.data.rows), older: loaded.result.data.page?.older, reason: undefined };
  };
  /** Every row of the list, read three at a time. */
  const all = async (input: Record<string, unknown>, executor: Executor) => {
    const rows: Record<string, unknown>[] = [];
    for (let read = await page({ ...input, limit: 3 }, executor); ; read = await page({ ...input, limit: 3, after: read.older }, executor)) {
      rows.push(...read.rows);
      if (!read.older) return rows;
    }
  };
  const facets = async (where: readonly object[], executor: Executor) => {
    const loaded = await loadFacetRail({ signal: 'executions', facets: ['status', 'verdict', 'verifier_card_uid'], where } as never, { executor, range });
    if (loaded.result.state !== 'populated') throw new Error(JSON.stringify(loaded.result));
    return loaded.result.data.facets.map((facet) => facet.values);
  };

  // Unfinished work: a runner claims one execution and holds it while its hosted step waits for memory. The list finds it by its
  // recorded status, with no verdict, as its own exact read says.
  const memory = server.occupyHostedMemory(64 << 20);
  const claimed = await counter.run('agent').observe.submit('adds-one', { answer: 2 });
  if (claimed.kind !== 'queued') throw new Error('adds-one queues an execution');
  server.waitForHostedWaiters(1, 60_000);
  const running = claimed.execution.id;
  server.flushBifrost();
  const held = await nodeExecutor(client).run([{ op: 'verification.get_execution', execution_id: running }]);
  expect((await page({ where: [{ column: 'status', value: 'running' }] }, nodeExecutor(client))).rows.map((each) => [each.execution_id, each.status, each.verdict])).toEqual([[running, held[0].ok && held[0].value.status, null]]);
  // Given memory it finishes, and the list follows its newest recorded status.
  memory.release();
  await claimed.execution.wait({ timeoutMs: 60_000 });
  await counter.shutdown();
  server.flushBifrost();

  const mcp = await mcpOver(server.baseUrl, client);
  try {
    for (const [surface, direct] of [['node', nodeExecutor(client)], ['mcp', mcp.executor]] as const) {
      const ops: string[] = [];
      const executor: Executor = { run: (requests, signal) => (ops.push(...requests.map((request) => request.op)), direct.run(requests, signal)) };
      // Every accepted execution is one row, whatever its origin, exactly once across pages.
      const rows = await all({}, executor);
      const ids = rows.map((row) => row.execution_id);
      expect(new Set(ids).size, surface).toBe(ids.length);
      expect(ids, surface).toEqual(expect.arrayContaining([refused, agreed, drifted, session.execution_id, ...turns, running, stopped]));
      const row = (id: string) => rows.find((each) => each.execution_id === id) as Record<string, unknown>;
      // Status and verdict are separate recorded facts, with the exact Verifier and the recorded correlations.
      expect(row(refused), surface).toMatchObject({ status: 'completed', verdict: 'failed', verifier_card_uid: assistant.answerCheck.uid, group_id: null });
      expect(row(agreed), surface).toMatchObject({ status: 'completed', verdict: 'passed' });
      expect(row(drifted), surface).toMatchObject({ status: 'completed', verdict: 'failed', verifier_card_uid: latency, run_id: null });
      expect(row(session.execution_id), surface).toMatchObject({ status: 'completed', verdict: 'passed', group_id: 'ui-verify-conversation' });
      expect(turns.map((id) => [row(id).status, row(id).verdict, row(id).group_id]), surface).toEqual(turns.map(() => ['completed', 'passed', 'ui-verify-conversation']));
      expect(row(refused).run_id, surface).toEqual(expect.any(String));
      // The execution that was running is now completed with its verdict; the one cancelled while held has its recorded status and no verdict.
      expect([row(running).status, row(running).verdict], surface).toEqual(['completed', 'passed']);
      expect([row(stopped).status, row(stopped).verdict], surface).toEqual(['cancelled', null]);
      // Every queued row agrees with the execution's own exact read.
      for (const id of [refused, agreed, drifted, session.execution_id, running]) {
        const [live] = await direct.run([{ op: 'verification.get_execution', execution_id: id }]);
        expect(live.ok && [live.value.status, live.value.result?.outcome.verdict, live.value.result?.verifier.uid], surface).toEqual([row(id).status, row(id).verdict, row(id).verifier_card_uid]);
      }
      const [cancelled] = await direct.run([{ op: 'verification.get_execution', execution_id: stopped }]);
      expect(cancelled.ok && cancelled.value.status, surface).toBe('cancelled');

      // Facets and both charts count the same population the pages hold.
      const [statuses, verdicts, verifiers] = await facets([], executor);
      const charts = await loadExecutionCharts({}, { executor, range });
      if (charts.verdicts.result.state !== 'populated' || charts.verifiers.result.state !== 'populated') throw new Error(`${surface}: ${JSON.stringify(charts)}`);
      expect([total(statuses), total(verdicts), total(verifiers), total(charts.verdicts.result.data), total(charts.verifiers.result.data)], surface).toEqual(Array(5).fill(rows.length));
      const count = (read: QueryResult, value: unknown) => Number(read.rows.find((each) => each[0] === value)?.[1] ?? 0);
      for (const status of ['completed', 'running', 'cancelled', 'pending', 'not recorded']) expect(count(statuses, status), `${surface} ${status}`).toBe(rows.filter((each) => each.status === status).length);
      for (const verdict of ['passed', 'failed', null]) expect(count(verdicts, verdict), `${surface} ${verdict}`).toBe(rows.filter((each) => each.verdict === verdict).length);
      // No recorded verdict is its own neutral chart series, never a verdict.
      expect(named(charts.verdicts.result.data).filter((each) => each.verdict === 'no verdict').reduce((sum, each) => sum + Number(each.executions), 0), surface).toBe(count(verdicts, null));

      // Status and verdict combine across pages, and the facets under the same clauses count the same rows.
      const failed = [{ column: 'status', value: 'completed' }, { column: 'verdict', value: 'failed' }];
      const failures = await all({ where: failed }, executor);
      expect(failures.map((each) => each.execution_id).sort(), surface).toEqual(rows.filter((each) => each.verdict === 'failed').map((each) => each.execution_id).sort());
      expect((await facets(failed, executor)).map(total), surface).toEqual(Array(3).fill(failures.length));
      // One exact Verifier, by its recorded UID; a session; no recorded verdict.
      expect((await all({ where: [{ column: 'verifier_card_uid', value: assistant.answerCheck.uid }] }, executor)).map((each) => each.execution_id), surface).toEqual([refused]);
      expect((await all({ where: [{ column: 'group_id', value: 'ui-verify-conversation' }] }, executor)).map((each) => each.execution_id).sort(), surface).toEqual([session.execution_id, ...turns].sort());
      expect((await all({ where: [{ column: 'verdict', value: null }] }, executor)).map((each) => each.execution_id).sort(), surface).toEqual([stopped]);
      // An incompatible combination is empty, and says so.
      expect(await page({ where: [{ column: 'status', value: 'cancelled' }, { column: 'verdict', value: 'passed' }] }, executor), surface).toMatchObject({ rows: [], reason: 'No execution matches these filters in this window.' });
      // An earlier range holds nothing.
      expect((await loadRecordTable({ signal: 'executions' }, { executor, range: BEFORE })).result.state, surface).toBe('empty');
      // Bounded shared queries only: no execution's status or details was read to build the list, counts or charts.
      expect(new Set(ops), surface).toEqual(new Set(['bifrost.query']));
    }
  } finally {
    await mcp.close();
  }

  // Another tenant sees no row, count or chart bucket; a principal without the query permission is refused, never shown an empty list.
  const foreignKey = server.bootstrapServiceInTenant(server.seedTenant('ui-verify-foreign'), ['admin'], 'ui-verify-admin');
  const outsider = { executor: nodeExecutor(WyrdClient.connect({ credential: foreignKey })), range };
  expect((await loadRecordTable({ signal: 'executions' }, outsider)).result.state).toBe('empty');
  expect((await loadFacetRail({ signal: 'executions', facets: ['status', 'verdict'] }, outsider)).result.state).toBe('empty');
  const foreignCharts = await loadExecutionCharts({}, outsider);
  expect([foreignCharts.verdicts.result.state, foreignCharts.verifiers.result.state]).toEqual(['empty', 'empty']);
  // `verifier:read` alone opens exact executions but grants no discovery read: every panel is denied whole.
  const narrowKey = server.scopedApiKey('ui_verify_narrow', ['verifier:read']);
  const narrowNode = nodeExecutor(WyrdClient.connect({ credential: narrowKey }));
  let narrowBatches = 0;
  const countingExecutor: Executor = {
    run(requests, signal) {
      narrowBatches += 1;
      return narrowNode.run(requests, signal);
    }
  };
  const narrow = { executor: countingExecutor, range };
  const refusedCharts = await loadExecutionCharts({}, narrow);
  for (const loaded of [await loadRecordTable({ signal: 'executions' }, narrow), await loadFacetRail({ signal: 'executions', facets: ['status'] }, narrow), refusedCharts.verdicts, refusedCharts.verifiers])
    expect(loaded.result).toMatchObject({ state: 'denied', problem: { code: DENIED } });
  // The list, the facets and the two charts together sent one batch each, and nothing was read after a refusal.
  expect(narrowBatches).toBe(3);

  // BFF and custom app: the same rows for this tenant's session, nothing for another tenant's.
  env.WYRD_SERVER_URL = server.baseUrl;
  const failedBody = JSON.stringify({ component: 'RecordTable', input: { signal: 'executions', where: [{ column: 'verdict', value: 'failed' }] }, range });
  const app = async (body = failedBody) => JSON.parse(await (await loadExamplePanel({ request: new Request('http://app.test/view/load', { method: 'POST', body }) } as never)).text());
  const signedIn = async (tenant: string, credential: string) => {
    const url = new URL(`http://ui.test/t/${tenant}/ui/load`);
    const cookies = jar();
    await browserSessions.signInWithApiKey(tenant, credential, url, cookies);
    return { url, browserSession: await browserSessions.read(tenant, url, cookies) };
  };
  const bff = async ({ url, browserSession }: Awaited<ReturnType<typeof signedIn>>, body = failedBody) =>
    (await loadPanel({ locals: { browserSession }, request: new Request(url, { method: 'POST', body, headers: { 'content-type': 'application/json' } }) } as never)).json();
  const own = await signedIn('test-tenant-1', server.apiKey);
  const failedIds = [refused, drifted].sort();
  const idsOf = (loaded: { result: { state: string; data?: { rows: QueryResult } } }) => (loaded.result.state === 'populated' && loaded.result.data ? named(loaded.result.data.rows).map((each) => each.execution_id).sort() : loaded.result.state);
  expect(idsOf(await bff(own))).toEqual(failedIds);
  expect(idsOf(await bff(await signedIn('ui-verify-foreign', foreignKey)))).toBe('empty');
  const answered = await app();
  expect(JSON.stringify(answered)).not.toContain(server.apiKey);
  expect(idsOf(answered)).toEqual(failedIds);

  // Each chart is a plain Chart panel over the same population: the BFF and the custom app count every execution the pages hold.
  const charts = await loadExecutionCharts({}, { executor: nodeExecutor(client), range });
  const everything = (await all({}, nodeExecutor(client))).length;
  for (const chart of [charts.verdicts, charts.verifiers]) {
    const chartBody = JSON.stringify({ component: 'Chart', input: chart.input, range });
    for (const loaded of [await bff(own, chartBody), await app(chartBody)]) {
      expect(loaded.result.state, JSON.stringify(loaded.result)).toBe('populated');
      expect(total(loaded.result.data)).toBe(everything);
    }
  }

  // The refusal and the other tenant's empty list read the same through MCP, the BFF and the custom app.
  const refusal = { result: { state: 'denied', problem: { code: DENIED } } };
  const [narrowMcp, foreignMcp] = [await mcpOver(server.baseUrl, WyrdClient.connect({ credential: narrowKey })), await mcpOver(server.baseUrl, WyrdClient.connect({ credential: foreignKey }))];
  try {
    expect(await loadRecordTable({ signal: 'executions' }, { executor: narrowMcp.executor, range })).toMatchObject(refusal);
    expect((await loadRecordTable({ signal: 'executions' }, { executor: foreignMcp.executor, range })).result.state).toBe('empty');
    expect((await loadFacetRail({ signal: 'executions', facets: ['status', 'verdict'] }, { executor: foreignMcp.executor, range })).result.state).toBe('empty');
    const foreignMcpCharts = await loadExecutionCharts({}, { executor: foreignMcp.executor, range });
    expect([foreignMcpCharts.verdicts.result.state, foreignMcpCharts.verifiers.result.state]).toEqual(['empty', 'empty']);
  } finally {
    await narrowMcp.close();
    await foreignMcp.close();
  }
  expect(await bff(await signedIn('test-tenant-1', narrowKey))).toMatchObject(refusal);
  vi.stubEnv('WYRD_API_KEY', narrowKey);
  const appDenied = await app();
  vi.stubEnv('WYRD_API_KEY', foreignKey);
  const appForeign = await app();
  vi.stubEnv('WYRD_API_KEY', server.apiKey);
  expect(appDenied).toMatchObject(refusal);
  expect(idsOf(appForeign)).toBe('empty');

  // The built HTML draws the list and its facets through host tools/call alone, each answered by the real MCP server.
  const host = new Client({ name: 'wyrd-ui-artifact-host', version: '1.0.0' }, { versionNegotiation: { mode: 'auto' } });
  await host.connect(new StreamableHTTPClientTransport(new URL('/mcp', server.baseUrl), { fetch: client.httpAuth }));
  const doc = {
    version: 'wyrd.view/v1',
    catalogId: 'wyrd.ui/v1',
    range,
    panels: [
      { id: 'list', component: 'RecordTable', input: { signal: 'executions', where: [{ column: 'verdict', value: null }] } },
      { id: 'facets', component: 'FacetRail', input: { signal: 'executions', facets: ['status', 'verdict'] } },
      { id: 'verdicts', component: 'Chart', input: charts.verdicts.input }
    ]
  };
  try {
    const built = await appBridgeView(doc, async (name, args) => ((await host.callTool({ name, arguments: args })).structuredContent ?? {}) as object);
    const list = () => built.document.querySelector('[data-panel="list"]') as Element;
    await vi.waitFor(() => expect(list().querySelectorAll('tbody tr')).toHaveLength(1));
    expect(list().textContent).toContain('cancelled');
    expect(list().textContent).toContain('no verdict');
    expect(list().textContent).not.toMatch(/passed|failed/);
    await vi.waitFor(() => expect(built.document.querySelector('[data-panel="facets"]')?.textContent).toContain('completed'));
    await vi.waitFor(() => expect(built.document.querySelector('[data-panel="verdicts"] .legend')?.textContent).toContain('no verdict'));
    expect(new Set(built.calls.map((call) => call.name))).toEqual(new Set(['bifrost.query']));
  } finally {
    await host.close();
  }
  // The same built HTML behind another principal's host: a refusal is shown as a refusal, another tenant's list as empty.
  const artifactList = async (credential: string) => {
    const as = new Client({ name: 'wyrd-ui-artifact-host', version: '1.0.0' }, { versionNegotiation: { mode: 'auto' } });
    await as.connect(new StreamableHTTPClientTransport(new URL('/mcp', server.baseUrl), { fetch: WyrdClient.connect({ credential }).httpAuth }));
    try {
      // The whole tool result is relayed, so a refusal reaches the page as one.
      const view = await appBridgeView(doc, async (name, args) => (await as.callTool({ name, arguments: args })) as object);
      const text = () => view.document.querySelector('[data-panel="list"]')?.textContent ?? '';
      await vi.waitFor(() => expect(text()).toMatch(/WYRD_|No execution matches/));
      return text();
    } finally {
      await as.close();
    }
  };
  const artifactDenied = await artifactList(narrowKey);
  expect(artifactDenied).toContain(DENIED);
  expect(artifactDenied).not.toContain('No execution matches');
  expect(await artifactList(foreignKey)).toContain('No execution matches these filters in this window.');
  // A server that cannot be reached is a problem, never an empty list.
  const DOWN = 'http://127.0.0.1:9';
  const unreachable = { result: { state: 'error', problem: { code: expect.stringMatching(/^WYRD_/) } } };
  expect(await loadRecordTable({ signal: 'executions' }, { executor: nodeExecutor(WyrdClient.connect({ serverUrl: DOWN, credential: server.apiKey })), range })).toMatchObject(unreachable);
  const gone = await mcpOver(server.baseUrl, client);
  await gone.close();
  expect(await loadRecordTable({ signal: 'executions' }, { executor: gone.executor, range })).toMatchObject(unreachable);
  env.WYRD_SERVER_URL = DOWN;
  expect(await bff(own)).toMatchObject(unreachable);
  env.WYRD_SERVER_URL = server.baseUrl;
  vi.stubEnv('WYRD_SERVER_URL', DOWN);
  const appDown = await app();
  vi.stubEnv('WYRD_SERVER_URL', server.baseUrl);
  expect(appDown).toMatchObject(unreachable);
  const dark = await appBridgeView(doc, async () => {
    throw new Error('the host cannot reach its server');
  });
  await vi.waitFor(() => expect(dark.document.querySelector('[data-panel="list"]')?.textContent).toMatch(/WYRD_[A-Z0-9_]+/));
  expect(dark.document.querySelector('[data-panel="list"]')?.textContent).not.toContain('No execution matches');
});

/** A fresh deployment with the GitHub App mock, for a tenant that connects a repository. */
const reviewed = serverTest({ githubApp: true, perTest: true });

reviewed('Verify lists pull request executions by their recorded repository and number', async ({ server }) => {
  // The tenant connects installation 101 and declares the review policy of its repository `acme/api` (provider id 1).
  const github = server.githubApp;
  github.addInstallation(101, [{ id: 1, fullName: 'acme/api' }]);
  github.addUser('octo', [101]);
  github.signIn('octo');
  github.installTarget(101);
  const key = server.bootstrapService(['admin'], 'verify_repository_admin');
  const admin = WyrdClient.connect({ credential: key });
  const connection = await (await GithubConnection.connect({ client: admin })).begin();
  expect(signInGithubBrowser(server.baseUrl, server.tenantSlug, key).open(connection.authorization_url).status).toBe(200);
  const checkout = mkdtempSync(join(tmpdir(), 'wyrd-ui-verify-checkout-'));
  cpSync(fixture('cards/repository_declaration'), checkout, { recursive: true });
  mkdirSync(join(checkout, '.git'));
  writeFileSync(join(checkout, '.git/config'), `[remote "origin"]\n\turl = http://${github.host}/acme/api.git\n`);
  copyFileSync(join(checkout, 'policies/minimal.toml'), join(checkout, 'wyrd.toml'));
  await (await Cards.connect({ client: admin })).declareFromPath(join(checkout, 'wyrd.toml'));

  // Pull request 7 opens and is then pushed to, which supersedes its first execution; pull request 8 opens.
  const deliver = (number: number, action: string) => expect(github.deliverPullRequest(server.baseUrl, 101, 1, number, action)).toBe(202);
  github.setPullRequest(1, 7, 'b1', 'a1');
  deliver(7, 'opened');
  github.setPullRequest(1, 7, 'b1', 'b2');
  deliver(7, 'synchronize');
  github.setPullRequest(1, 8, 'b1', 'c1');
  deliver(8, 'opened');

  const context = { executor: nodeExecutor(admin), range: { from: new Date(Date.now() - 3_600_000).toISOString(), to: new Date(Date.now() + 60_000).toISOString() } };
  const named = (read: QueryResult) => read.rows.map((row) => Object.fromEntries(read.columns.map((column, at) => [column.name, row[at]])));
  const list = async (where: { column: string; value: string }[]) => {
    const loaded = await loadRecordTable({ signal: 'executions', columns: ['recorded_at', 'origin', 'repository', 'pull_request', 'status', 'verdict'], where } as never, context);
    return loaded.result.state === 'populated' ? named(loaded.result.data.rows) : [];
  };

  // Both executions of pull request 7 are listed with their recorded repository and number: the superseded one cancelled, its successor pending.
  let seventh: Record<string, unknown>[] = [];
  await vi.waitFor(
    async () => {
      server.flushBifrost();
      seventh = await list([{ column: 'pull_request', value: '7' }]);
      expect(seventh.map((row) => row.status).sort()).toEqual(['cancelled', 'pending']);
    },
    { timeout: 30_000, interval: 500 }
  );
  for (const row of seventh) expect(row).toMatchObject({ origin: 'pull_request', repository: '1', pull_request: '7', verdict: null });

  // The filters combine, and the pending row is the execution its own exact read describes.
  const [pending] = await list([{ column: 'pull_request', value: '7' }, { column: 'status', value: 'pending' }]);
  const [live] = await context.executor.run([{ op: 'verification.get_execution', execution_id: pending.execution_id as string }]);
  expect(live.ok && [live.value.status, live.value.pull_request?.number, live.value.pull_request?.repository_id, live.value.pull_request?.head.sha]).toEqual(['pending', 7, '1', 'b2']);
  expect(await list([{ column: 'repository', value: '1' }, { column: 'pull_request', value: '9' }])).toEqual([]);

  // The facets count the same three executions by repository and by pull request.
  const rail = await loadFacetRail({ signal: 'executions', facets: ['repository', 'pull_request', 'origin'] } as never, context);
  if (rail.result.state !== 'populated') throw new Error(JSON.stringify(rail.result));
  expect(rail.result.data.facets.map((facet) => facet.values.rows)).toEqual([[['1', 3]], [['7', 2], ['8', 1]], [['pull_request', 3]]]);
});

/** The `coding-verifier` statement every request of its `spans-reviewed` check carries. */
const LIVE_REVIEW = 'spans were reviewed live';
/** What a Chat Completions request carrying tool results contains. */
const TOOL_TURN = /"role"\s*:\s*"tool"/;
/** The trace the coding session's first turn names. */
const SESSION_TRACE = '7d3efb1b173fecfa5cf1d7a7e9c2a441';

/**
 * A server whose judge reviews a session's spans: it reads the span names of
 * the trace its answer names through `bifrost.query`, then passes citing the
 * first trace of the session; every other judge agrees.
 */
const traced = assistantTest({
  perTest: true,
  judge: (request) => {
    if (!request.body.includes(LIVE_REVIEW)) return judgeReply(AGREES);
    if (TOOL_TURN.test(request.body)) return judgeReply('{"outcome":"passed","reason":"the spans were read","citations":[{"slot":"traces","locator":{"json_pointer":"/0"}}]}');
    const [traceId] = request.body.match(/[0-9a-f]{32}/) ?? [];
    return toolTurn([toolCall('call-spans', 'bifrost.query', { sql: `SELECT name FROM bifrost.traces.spans WHERE trace_id = decode('${traceId}', 'hex') ORDER BY name`, max_rows: 10 })]);
  }
});

traced('a session judgment cites an exact trace: the citation leads to its pinned member and trace, a later span revises nothing, and trace reads keep their own permission', async ({ server, assistant: _ }) => {
  // A coding session of three turns; the first names a retained trace.
  await seedSignals(server, server.apiKey, { trace: SESSION_TRACE, service: 'coding-agent' });
  const cards = await Cards.connect();
  const coding = await cards.declareFromPath(fixture('cards/verify_in_real_time/coding.yaml'));
  const bundle = mkdtempSync(join(tmpdir(), 'wyrd-ui-coding-'));
  await cards.pull(coding.root, bundle);
  const { principalId, key } = await runtimeAccount({ kind: 'Service', name: coding.root.name });
  // The judge reads spans as the Service, which needs the tool and query grants.
  server.scopedApiKey('ui_live_reader', ['tool:invoke', 'bifrost_query:read']);
  await (await Principals.connect()).grantRole(principalId, 'ui_live_reader');
  const token = await WyrdClient.connect().accessToken();
  /** The binding of `owner` that judges a session when it ends. */
  const sessionBinding = async (owner: Exact) => {
    for (const id of (await cards.get(owner)).status?.verification?.binding_ids ?? []) {
      const response = await fetch(`${server.baseUrl}/v1/verification/bindings/${id}`, { headers: { 'x-wyrd-access-token': `Bearer ${token}` } });
      if (((await response.json()) as { group_end?: unknown }).group_end) return id;
    }
    throw new Error(`${owner.name} has no session binding`);
  };
  const desk = WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential: key }) });
  await desk.startBifrost();
  const run = desk.run('agent');
  const calls = [{ call: 'prompt', tests: 'passed' }, { call: 'edit', tests: 'failed' }, { call: 'test', tests: 'passed' }];
  for (const [index, call] of calls.entries()) await run.observe.verify('coding-turn', index === 0 ? { call, trace: SESSION_TRACE } : { call }, { group_id: 'ui-coding' });
  await desk.shutdown();
  const session = await groupExecution(server, await sessionBinding(exact(declared(coding))), 90_000);

  // Another Service ends a session under the same group id: the two stay apart by owner.
  const support = await cards.declareFromPath(fixture('cards/verify_in_real_time/support.yaml'));
  const supportBundle = mkdtempSync(join(tmpdir(), 'wyrd-ui-coding-support-'));
  await cards.pull(support.root, supportBundle);
  const helpdesk = WyrdState.fromPath(supportBundle, { client: WyrdClient.connect({ credential: (await runtimeAccount({ kind: 'Service', name: support.root.name })).key }) });
  await helpdesk.startBifrost();
  for (const turn of [1, 2, 3]) await helpdesk.run('agent').observe.verify('turn-check', { turn, answer: `answer ${turn}` }, { positional: true, group_id: 'ui-coding' });
  await helpdesk.shutdown();
  const other = await groupExecution(server, await sessionBinding(exact(declared(support))), 90_000);
  server.flushBifrost();
  const members = session.group?.sources.traces.execution_ids ?? [];
  expect(members).toHaveLength(3);
  // Same group id, two owners: each judged execution names its own Service and pins only its own members.
  expect([session.group_id, other.group_id]).toEqual(['ui-coding', 'ui-coding']);
  expect([session.owner_card_uid, other.owner_card_uid]).toEqual([declared(coding).uid, declared(support).uid]);
  const otherMembers = other.group?.sources.turns.execution_ids ?? [];
  expect(otherMembers).toHaveLength(3);
  expect(members.filter((id) => otherMembers.includes(id))).toEqual([]);

  const client = WyrdClient.connect();
  const mcp = await mcpOver(server.baseUrl, client);
  /** The session's judgments and pinned members, as one host reads them. */
  const recorded = async (executor: Executor, surface: string) => {
    const judgment = data(await loadExecution({ executionId: session.execution_id, node: 'req:spans-reviewed' }, { executor }), surface);
    const sources = data(await loadExecution({ executionId: session.execution_id, node: 'input:traces' }, { executor }), surface);
    return { verdict: judgment.status.result?.outcome.verdict, requirements: judgment.status.result?.outcome.requirements, members: sources.members?.map((member) => member.execution_id) };
  };
  try {
    const before = await recorded(nodeExecutor(client), 'node');
    for (const [surface, executor] of [['node', nodeExecutor(client)], ['mcp', mcp.executor]] as const) {
      // The judgment cites the first trace of the session by its exact slot and locator; membership is the recorded snapshot.
      const read = await recorded(executor, surface);
      expect(read, surface).toEqual(before);
      expect(read.verdict, surface).toBe('failed');
      expect(read.requirements?.find((each) => each.id === 'spans-reviewed'), surface).toMatchObject({ outcome: 'passed', citations: [{ slot: 'traces', locator: { json_pointer: '/0' } }] });
      expect(read.members, surface).toEqual(members);
      // The other owner's session, read by the same host, lists only its own members under its own owner.
      const theirs = data(await loadExecution({ executionId: other.execution_id, node: 'input:turns' }, { executor }), surface);
      expect([theirs.status.owner_card_uid, theirs.members?.map((member) => member.execution_id)], surface).toEqual([declared(support).uid, otherMembers]);
      // The cited position is the first pinned member, whose accepted input names the exact trace.
      const cited = data(await loadExecution({ executionId: members[0], node: 'input:trace' }, { executor }), surface);
      expect(JSON.stringify(cited.inputs), surface).toContain(SESSION_TRACE);
      // That exact trace is read by its ID alone, with no list range or filter.
      const waterfall = await loadTraceWaterfall({ traceId: SESSION_TRACE }, { executor });
      expect(waterfall.result.state, surface).toBe('populated');
      // A trace list over a later day does not hold this trace; the citation still opens it.
      const listed = await loadRecordTable({ signal: 'traces' }, { executor, range: { from: '2026-10-20T00:00:00Z', to: '2026-10-21T00:00:00Z' } });
      expect(['populated', 'empty'], surface).toContain(listed.result.state);
      expect(JSON.stringify(listed.result), surface).not.toContain(SESSION_TRACE);
    }

    // A span that arrives after the judgment joins the trace and revises neither the judgment nor the membership.
    const late = nanos(new Date().toISOString());
    await exportOtlp(server, server.apiKey, '/v1/traces', {
      resourceSpans: [{ resource: { attributes: [] }, scopeSpans: [{ scope: { name: 'wyrd-ui-journey' }, spans: [{ traceId: SESSION_TRACE, spanId: 'a1b2c3d4e5f60718', name: 'late-test-run', kind: 1, startTimeUnixNano: late, endTimeUnixNano: late, status: { code: 0 }, attributes: [] }] }] }]
    });
    server.flushBifrost();
    for (const [surface, executor] of [['node', nodeExecutor(client)], ['mcp', mcp.executor]] as const) {
      const grown = await loadTraceWaterfall({ traceId: SESSION_TRACE }, { executor });
      expect(JSON.stringify(grown.result), surface).toContain('late-test-run');
      expect(await recorded(executor, surface)).toEqual(before);
    }

    // A principal that may read executions but not traces still sees the citation; the trace itself is refused to it, and the refusal is drawn.
    const narrowClient = WyrdClient.connect({ credential: server.scopedApiKey('ui_traced_narrow', ['verifier:read']) });
    const narrowMcp = await mcpOver(server.baseUrl, narrowClient);
    try {
      for (const [surface, narrow] of [['node', nodeExecutor(narrowClient)], ['mcp', narrowMcp.executor]] as const) {
        expect((await recorded(narrow, surface)).requirements, surface).toEqual(before.requirements);
        const refused = await loadTraceWaterfall({ traceId: SESSION_TRACE }, { executor: narrow });
        expect(refused.result, surface).toMatchObject({ state: 'denied', problem: { code: DENIED } });
        const drawn = render(TraceWaterfall, refused);
        expect(drawn.container.textContent, surface).toContain(DENIED);
        expect(drawn.container.textContent, surface).not.toContain('edit-file');
        drawn.unmount();
      }
    } finally {
      await narrowMcp.close();
    }
    // Another tenant finds neither the session nor the trace.
    const outsider = nodeExecutor(WyrdClient.connect({ credential: server.bootstrapServiceInTenant(server.seedTenant('ui-traced-foreign'), ['admin'], 'ui-traced-admin') }));
    expect((await loadExecution({ executionId: session.execution_id }, { executor: outsider })).result).toMatchObject({ state: 'error', problem: { code: 'WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND' } });
    expect((await loadTraceWaterfall({ traceId: SESSION_TRACE }, { executor: outsider })).result.state).toBe('empty');
  } finally {
    await mcp.close();
  }
});
