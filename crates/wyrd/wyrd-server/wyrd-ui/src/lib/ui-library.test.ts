import { render } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import { expect, test } from 'vitest';
import * as development from '$lib/server/development';
import TenantLayout from '../routes/t/[tenantKey]/+layout.svelte';
import SignedIn from '../routes/t/[tenantKey]/+page.svelte';

const tenant = { key: 'acme', name: 'Acme' };
const session = {
  subject: { id: 'user', name: 'Jordan Reyes' },
  tenants: [tenant, { key: 'research', name: 'Research' }],
  expiresAt: 1
};

test('TASK-002: Retained pages without mocks', () => {
  // Retained routes render plain markup: no deleted shared component or feature imports.
  const sources = import.meta.glob('/src/routes/**/*.svelte', {
    query: '?raw',
    import: 'default',
    eager: true
  }) as Record<string, string>;
  for (const [path, source] of Object.entries(sources))
    expect(source, path).not.toMatch(/\$lib\/(components|features)\//);

  // The mock-data switch is gone; only the development identity remains.
  expect(Object.keys(development).filter((name) => /mock/i.test(name))).toEqual([]);

  // The signed-in page shows tenant and principal with no domain data.
  const page = render(SignedIn, { data: { tenant, session } } as never);
  expect(page.getByRole('heading', { level: 1 })).toHaveTextContent('Acme');
  expect(page.container).toHaveTextContent('Jordan Reyes');
  expect(page.container.textContent).not.toMatch(/Recent changes|Recently viewed Cards/);
  page.unmount();

  // The tenant layout keeps tenant switching and sign-out for the current tenant.
  const children = createRawSnippet(() => ({ render: () => '<p>child</p>' }));
  const layout = render(TenantLayout, { data: { tenant, session }, children } as never);
  const switches = layout.container.querySelectorAll('form[action="/?/switch"] button[name="tenantKey"]');
  expect([...switches].map((button) => button.getAttribute('value'))).toEqual(['acme', 'research']);
  const signOut = layout.getByRole('button', { name: 'Sign out' });
  expect(signOut.closest('form')).toHaveAttribute('action', '/?/logout');
  expect(signOut.closest('form')?.querySelector('input[name="tenantKey"]')).toHaveValue('acme');
  expect(layout.getByText('child')).toBeInTheDocument();
});

import { fireEvent, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { vi } from 'vitest';
import {
  CardHeader,
  CardRefChip,
  Chart,
  Drawer,
  DurationBar,
  EmptyState,
  Sparkline,
  StatusPill,
  VerdictPill,
  type CardRef,
  type Executor,
  type QueryResult,
  type WireRequest,
  type WireResult
} from 'wyrd-ai-ui';
import { chartParams, loadCardHeader, loadChart, shapeChart, validateIntent, type ChartInput } from 'wyrd-ai-ui/loaders';
import { mountView, validateView, WyrdView } from 'wyrd-ai-ui/view';
import { mcpExecutor } from 'wyrd-ai-ui/mcp';
import catalog from 'wyrd-ai-ui/catalog.json';
import { card, denied, RANGE, registered, series, summary, terminal, timeseries } from '$lib/ui-fixtures';

/** An executor answering each read with `answer` and recording every request and signal. */
function recording(answer: (request: WireRequest) => WireResult | Promise<WireResult>) {
  const calls: { request: WireRequest; signal?: AbortSignal }[] = [];
  const executor: Executor = {
    run: (requests, signal) =>
      Promise.all(requests.map((request) => (calls.push({ request, signal }), answer(request)))) as never
  };
  return { executor, calls };
}

test('TASK-003: Validated portable rendering', async () => {
  // Every catalog component and its declared states are generated, with view-document shape.
  expect(Object.keys(catalog.components)).toEqual(['Chart', 'CardHeader']);
  expect(catalog.components.Chart.emits.map((action) => action.name)).toEqual(['chart.range', 'chart.open_in_query']);
  expect(catalog.view.required).toEqual(['range', 'panels']);

  // Timeseries: one line per series value, each with a distinct dash and marker in the legend.
  const plot = render(Chart, { input: timeseries, result: { state: 'populated', data: series } });
  expect(plot.getByRole('heading', { name: 'Latency' })).toBeInTheDocument();
  const legend = plot.container.querySelectorAll('.legend li');
  expect([...legend].map((item) => item.textContent?.trim())).toEqual(['a', 'b']);
  expect(new Set([...plot.container.querySelectorAll('.legend line')].map((line) => line.getAttribute('stroke-dasharray'))).size).toBe(2);
  // Focus opens the tooltip on the latest point; Home walks to the first. A zone-less timestamp reads as UTC.
  const svg = plot.getByRole('application');
  await fireEvent.focus(svg);
  expect(plot.getByRole('status')).toHaveTextContent('2026-10-20 11:00 UTC');
  expect(plot.getByRole('status')).toHaveTextContent('b: —');
  await fireEvent.keyDown(svg, { key: 'Home' });
  expect(plot.getByRole('status')).toHaveTextContent('a: 120 ms');
  // Without a host, Open in Query is unavailable and says so.
  expect(plot.getByRole('button', { name: 'Open in Query' })).toBeDisabled();
  expect(plot.getByRole('alert')).toHaveTextContent('This host has no Query');
  plot.unmount();

  // More than four series fold into "other".
  const many: QueryResult = { ...series, rows: ['a', 'b', 'c', 'd', 'e'].map((model, index) => ['2026-10-20T10:00:00Z', model, index + 1]), terminal: terminal(5) };
  const folded = render(Chart, { input: timeseries, result: { state: 'populated', data: many } });
  expect([...folded.container.querySelectorAll('.legend li')].map((item) => item.textContent?.trim())).toEqual(['e', 'd', 'c', 'other']);
  folded.unmount();

  // Bar, stat and table; the table keeps duplicate names and nulls positionally.
  const bars: QueryResult = { columns: [{ name: 'model', data_type: 'Utf8', nullable: false }, { name: 'runs', data_type: 'Int64', nullable: false }], rows: [['a', 3], ['b', 12]], terminal: terminal(2) };
  const bar = render(Chart, { input: { type: 'bar', sql: 'SELECT 1', x: 'model', y: ['runs'] }, result: { state: 'populated', data: bars } });
  expect(bar.container.querySelector('.bars')).toHaveTextContent('a 3b 12');
  bar.unmount();
  const stat = render(Chart, { input: { type: 'stat', sql: 'SELECT 1', unit: 'runs' }, result: { state: 'populated', data: { columns: [{ name: 'n', data_type: 'Int64', nullable: false }], rows: [['12400']], terminal: terminal(1) } } });
  expect(stat.container.querySelector('.stat')).toHaveTextContent('12,400runs');
  stat.unmount();
  const table = render(Chart, {
    input: { type: 'table', sql: 'SELECT 1' },
    result: { state: 'populated', data: { columns: [{ name: 'v', data_type: 'Int64', nullable: true }, { name: 'v', data_type: 'Utf8', nullable: false }], rows: [[null, 'x']], terminal: terminal(1, 'degraded') } }
  });
  expect([...table.container.querySelectorAll('th')].map((cell) => cell.textContent)).toEqual(['v', 'v']);
  expect([...table.container.querySelectorAll('td')].map((cell) => cell.textContent)).toEqual(['∅', 'x']);
  expect(table.container).toHaveTextContent('Partial result');
  expect(table.getByRole('region', { name: 'Result table' })).toHaveAttribute('tabindex', '0');
  table.unmount();

  // Every non-populated state names itself; a refusal carries the server's code verbatim.
  for (const [result, word] of [
    [{ state: 'loading' }, 'Loading'],
    [{ state: 'empty', data: { ...series, rows: [] }, reason: 'The SQL returned no rows for p95 in this range.' }, 'Empty'],
    [{ state: 'denied', problem: denied }, 'Not authorized'],
    [{ state: 'error', problem: { code: 'WYRD_VALA_504_QUERY_TIMEOUT', title: 'Query timed out' } }, 'Error']
  ] as const) {
    const view = render(Chart, { input: timeseries, result });
    expect(view.container).toHaveTextContent(word);
    expect(view.container.querySelector('svg.plot')).toBeNull();
    view.unmount();
  }
  const refused = render(Chart, { input: timeseries, result: { state: 'denied', problem: denied } });
  expect(refused.container).toHaveTextContent('WYRD_AUTH_403_FORBIDDEN');
  refused.unmount();

  // A result that does not match its definition is an error, never a blank chart.
  expect(shapeChart({ ...timeseries, y: ['model'] } as ChartInput, series)).toMatchObject({ state: 'error', problem: { code: 'WYRD_UI_CHART_SHAPE', detail: 'y column "model" is Utf8' } });
  expect(shapeChart(timeseries, { ...series, rows: [] }).state).toBe('empty');

  // CardHeader: exact identity, newest-first versions, lifecycle status, copy feedback and tabs.
  const writeText = vi.fn(async () => {});
  Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
  const header = render(CardHeader, {
    input: { cardRef: card, tabs: [{ id: 'overview', label: 'Overview' }, { id: 'runs', label: 'Runs' }], tab: 'runs' },
    result: { state: 'populated', data: { card: registered, versions: { state: 'populated', data: { items: [summary('1.4.2'), summary('1.4.1', 'deprecated')], truncated: true } } } }
  });
  expect(header.getByRole('heading', { level: 2 })).toHaveTextContent('ranker');
  expect(header.container).toHaveTextContent('search/ranker@1.4.2');
  expect(header.container).toHaveTextContent('✓active');
  expect([...header.container.querySelectorAll('option')].map((option) => option.value)).toEqual(['1.4.2', '1.4.1']);
  expect(header.container).toHaveTextContent('Showing the newest 2 versions');
  expect(header.getByRole('button', { name: 'Runs' })).toHaveAttribute('aria-current', 'page');
  await fireEvent.click(header.getByRole('button', { name: 'Copy uid' }));
  expect(writeText).toHaveBeenCalledWith('card_01HX');
  expect(await header.findByText('Copied')).toBeInTheDocument();
  writeText.mockRejectedValueOnce(new Error('denied'));
  await fireEvent.click(header.getByRole('button', { name: 'Copy ref' }));
  expect(await header.findByText('Copy failed')).toBeInTheDocument();
  header.unmount();
  const noVersions = render(CardHeader, { input: { cardRef: card }, result: { state: 'populated', data: { card: registered, versions: { state: 'denied', problem: denied } } } });
  expect(noVersions.container).toHaveTextContent('Versions unavailable: Forbidden (WYRD_AUTH_403_FORBIDDEN)');
  expect(noVersions.getByRole('heading', { level: 2 })).toHaveTextContent('ranker');
  noVersions.unmount();
  for (const result of [{ state: 'loading' }, { state: 'denied', problem: denied }, { state: 'error', problem: { code: 'WYRD_REGISTRY_404_NOT_FOUND', title: 'Not found' } }] as const) {
    const view = render(CardHeader, { input: { cardRef: card }, result });
    expect(view.container.querySelector('.state')).toHaveAttribute('data-state', result.state);
    view.unmount();
  }

  // Primitives.
  const primitives = render(EmptyState, { state: 'error', problem: { code: 'WYRD_X_500_Y', title: 'Broke' } });
  expect(primitives.getByRole('note')).toHaveTextContent('✕ Error Broke WYRD_X_500_Y');
  primitives.unmount();
  const loading = render(EmptyState, { state: 'loading', title: 'Running' });
  expect(loading.getByRole('status')).toHaveAttribute('aria-busy', 'true');
  loading.unmount();
  const chip = render(CardRefChip, { cardRef: card, href: '/cards/x' });
  expect(chip.getByRole('link')).toHaveTextContent('Model search/ranker@1.4.2');
  chip.unmount();
  for (const [verdict, text] of [['passed', '✓passed'], ['failed', '✕failed'], ['inconclusive', '!inconclusive']] as const) {
    const pill = render(VerdictPill, { verdict });
    expect(pill.container).toHaveTextContent(text);
    pill.unmount();
  }
  const neutral = render(StatusPill, { label: 'Model' });
  expect(neutral.container.textContent?.trim()).toBe('Model');
  neutral.unmount();
  const spark = render(Sparkline, { points: [1, 3, 2], label: 'Runs this week' });
  expect(spark.getByRole('img', { name: 'Runs this week' }).querySelector('polyline')).not.toBeNull();
  spark.unmount();
  const duration = render(DurationBar, { ms: 4210, max: 8420, label: 'llm.call' });
  expect(duration.container).toHaveTextContent('llm.call 4.21s');
  expect(duration.container.querySelector('.bar')).toHaveStyle({ width: '50%' });
  duration.unmount();
  // jsdom has no modal dialog; give it the platform method the browser provides.
  HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
    this.open = true;
  };
  const onclose = vi.fn();
  const drawer = render(Drawer, { open: true, title: 'Span', onclose, children: createRawSnippet(() => ({ render: () => '<p>detail</p>' })) });
  expect(drawer.getByRole('dialog', { name: 'Span' })).toHaveTextContent('detail');
  await fireEvent.click(drawer.getByRole('button', { name: 'Close Span' }));
  expect(onclose).toHaveBeenCalledOnce();
  drawer.unmount();

  // A view document mounts in a plain DOM host; one invalid panel fails alone.
  const { executor } = recording(() => ({ ok: true, value: series }));
  const host = document.createElement('div');
  document.body.append(host);
  const mounted = mountView(host, {
    mode: 'light',
    executor,
    doc: {
      title: 'Ranker',
      range: RANGE,
      panels: [
        { id: 'broken', component: 'Chart', input: { type: 'timeseries', x: 'at', y: ['p95'] } },
        { id: 'latency', component: 'Chart', input: timeseries }
      ]
    }
  });
  await waitFor(() => expect(host.querySelector('[data-panel="latency"] .legend')).not.toBeNull());
  expect(host.querySelector('.wyrd-view')).toHaveAttribute('data-mode', 'light');
  expect(host.querySelector('[data-panel="broken"]')).toHaveTextContent('Chart input.sql');
  mounted.destroy();
  expect(host.childElementCount).toBe(0);
  host.remove();
  expect(validateView({ range: { last: '6x' }, panels: [] })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
});

test('TASK-003: Dependent loads and safe actions', async () => {
  // A denied Card read issues no dependent version listing.
  const refusedCard = recording(() => ({ ok: false, problem: denied }));
  expect((await loadCardHeader({ cardRef: card }, { executor: refusedCard.executor })).result.state).toBe('denied');
  expect(refusedCard.calls.map((call) => call.request.op)).toEqual(['cards.get_by_ref']);

  // The listing uses the resolved identity, pages, and reports truncation; a failed listing keeps the Card.
  const paged = recording((request) =>
    request.op === 'cards.get_by_ref'
      ? { ok: true, value: registered }
      : { ok: true, value: { items: [summary(`1.0.${(request as { cursor: string | null }).cursor ?? 0}`)], next_cursor: String(Number((request as { cursor: string | null }).cursor ?? 0) + 1) } as never }
  );
  const header = await loadCardHeader({ cardRef: card }, { executor: paged.executor });
  expect(paged.calls).toHaveLength(6);
  expect(paged.calls[1].request).toMatchObject({ op: 'cards.list', kind: 'Model', name: 'ranker', space: 'search', include_prerelease: true, cursor: null });
  expect(header.result).toMatchObject({ state: 'populated', data: { versions: { state: 'populated', data: { truncated: true } } } });
  const unlisted = await loadCardHeader({ cardRef: card }, { executor: recording((request) => (request.op === 'cards.list' ? { ok: false, problem: denied } : { ok: true, value: registered })).executor });
  expect(unlisted.result).toMatchObject({ state: 'populated', data: { versions: { state: 'denied' } } });

  // Authored SQL is never rewritten; only referenced range binds are sent.
  expect(chartParams('SELECT 1', undefined)).toEqual([]);
  expect(chartParams('SELECT $1', undefined)).toMatchObject({ code: 'WYRD_UI_CHART_RANGE_REQUIRED' });
  expect(chartParams('WHERE at >= $1 AND at < $2 GROUP BY date_bin($3::interval, at)', RANGE)).toEqual([RANGE.from, RANGE.to, '5 minutes']);
  const chart = recording(() => ({ ok: true, value: series }));
  const loaded = await loadChart(timeseries, { executor: chart.executor, range: RANGE });
  expect(chart.calls[0].request).toEqual({ op: 'bifrost.query', sql: timeseries.sql, params: [RANGE.from, RANGE.to] });
  expect(loaded.executed).toEqual({ sql: timeseries.sql, params: [RANGE.from, RANGE.to] });

  // A relative range resolves once per view load; every panel binds the same bounds.
  const pending: ((result: WireResult) => void)[] = [];
  const held = recording(() => new Promise<WireResult>((resolve) => pending.push(resolve)));
  const doc = { range: { last: '6h' }, panels: [{ id: 'a', component: 'Chart', input: timeseries }, { id: 'b', component: 'Chart', input: { ...timeseries, title: 'B' } }] };
  const onaction = vi.fn();
  const view = render(WyrdView, { doc, executor: held.executor, onaction });
  await waitFor(() => expect(held.calls).toHaveLength(2));
  const params = (index: number) => (held.calls[index].request as Extract<WireRequest, { op: 'bifrost.query' }>).params as string[];
  const [first, second] = [params(0), params(1)];
  expect(first).toEqual(second);
  expect(Date.parse(first[1]) - Date.parse(first[0])).toBe(6 * 3_600_000);

  // A replaced document aborts in-flight loads and a late result is never accepted.
  await view.rerender({ doc: { range: RANGE, panels: [{ id: 'a', component: 'Chart', input: { ...timeseries, title: 'Replaced' } }] }, executor: held.executor, onaction });
  await waitFor(() => expect(held.calls).toHaveLength(3));
  expect(held.calls[0].signal?.aborted).toBe(true);
  expect(held.calls[1].signal?.aborted).toBe(true);
  expect(view.container.querySelector('[data-panel="b"]')).toBeNull();
  pending[0]({ ok: true, value: series });
  await tick();
  expect(view.container.querySelector('[data-panel="a"]')).toHaveTextContent('Running query');
  pending[2]({ ok: true, value: series });
  await waitFor(() => expect(view.container.querySelector('[data-panel="a"] .legend')).not.toBeNull());

  // A brushed range is validated, reloads every panel under the new bounds, and can be reset.
  const plot = view.container.querySelector('[data-panel="a"] svg.plot') as SVGSVGElement;
  plot.getBoundingClientRect = () => ({ left: 0, width: 600, top: 0, height: 220, right: 600, bottom: 220, x: 0, y: 0, toJSON: () => ({}) });
  await fireEvent.pointerDown(plot, { clientX: 100, pointerId: 1 });
  await fireEvent.pointerMove(plot, { clientX: 400, pointerId: 1 });
  await fireEvent.pointerUp(plot, { clientX: 400, pointerId: 1 });
  await waitFor(() => expect(held.calls).toHaveLength(4));
  const [from, to] = params(3);
  expect(Date.parse(from)).toBeGreaterThan(Date.parse(RANGE.from));
  expect(Date.parse(to)).toBeLessThan(Date.parse(RANGE.to));
  expect(onaction).not.toHaveBeenCalled();
  expect(view.getByRole('button', { name: 'Reset range' })).toBeInTheDocument();
  pending[3]({ ok: true, value: series });
  await waitFor(() => expect(view.container.querySelector('[data-panel="a"] .legend')).not.toBeNull());

  // Host intents: pending blocks a duplicate, and a refusal stays visible.
  let answer: (state: { state: 'denied'; problem: typeof denied }) => void = () => {};
  onaction.mockImplementation(() => new Promise((resolve) => (answer = resolve)));
  const open = view.getByRole('button', { name: 'Open in Query' });
  await fireEvent.click(open);
  await fireEvent.click(open);
  expect(onaction).toHaveBeenCalledOnce();
  expect(onaction).toHaveBeenCalledWith({ name: 'chart.open_in_query', input: { sql: timeseries.sql, params: [from, to] } }, 'a');
  expect(view.getByRole('button', { name: 'Open in Query…' })).toBeDisabled();
  answer({ state: 'denied', problem: denied });
  expect(await view.findByRole('alert')).toHaveTextContent('Forbidden (WYRD_AUTH_403_FORBIDDEN)');
  view.unmount();

  // Hosts validate intents against the catalog's action schemas.
  expect(validateIntent('Chart', { name: 'chart.range', input: { from: RANGE.to, to: RANGE.from } })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION', detail: 'Chart chart.range input.to: from must precede to' });
  expect(validateIntent('Chart', { name: 'card.tab', input: { tab: 'x' } })).toMatchObject({ detail: 'Chart does not emit card.tab' });
  expect(validateIntent('CardHeader', { name: 'card.tab', input: { tab: 'runs' } })).toEqual({ name: 'card.tab', input: { tab: 'runs' } });

  // The MCP executor maps reads onto existing tools and keeps both refusal shapes.
  const callServerTool = vi.fn(async ({ name }: { name: string }, _options?: { signal?: AbortSignal }) => {
    if (name === 'cards.get') return { content: [], structuredContent: { card: registered, created_at: '', updated_at: '' } };
    if (name === 'bifrost.query') return { content: [], isError: true, structuredContent: { code: 'WYRD_VALA_413_QUERY_RESULT_TOO_LARGE', title: 'Query result too large', status: 413 } };
    throw Object.assign(new Error('denied'), { data: denied });
  });
  const abort = new AbortController();
  const results = await mcpExecutor({ callServerTool } as never).run(
    [{ op: 'cards.get_by_ref', card_ref: card }, { op: 'bifrost.query', sql: 'SELECT $1', params: [1] }, { op: 'bifrost.list_tables' }],
    abort.signal
  );
  expect(results).toEqual([
    { ok: true, value: registered },
    { ok: false, problem: { code: 'WYRD_VALA_413_QUERY_RESULT_TOO_LARGE', title: 'Query result too large', status: 413, detail: undefined } },
    { ok: false, problem: { ...denied, detail: undefined } }
  ]);
  expect(callServerTool.mock.calls.map(([params]) => params)).toEqual([
    { name: 'cards.get', arguments: { card_ref: card } },
    { name: 'bifrost.query', arguments: { sql: 'SELECT $1', params: [1] } },
    { name: 'bifrost.list_tables', arguments: {} }
  ]);
  expect(callServerTool.mock.calls[0][1]).toEqual({ signal: abort.signal });
});
