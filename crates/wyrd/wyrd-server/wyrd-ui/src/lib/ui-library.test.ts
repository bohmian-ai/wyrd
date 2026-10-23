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
import { chartParams, loadCardHeader, loadChart, shapeChart, validateIntent, validatePanel, type ChartInput } from 'wyrd-ai-ui/loaders';
import { mountView, remoteLoader, validateView, WyrdView } from 'wyrd-ai-ui/view';
import examples from '../../../../../wyrd-spec/schemas/ui_problem_examples.json';
import { POST as bffLoad } from '../routes/t/[tenantKey]/ui/load/+server';
import { POST as exampleLoad } from '../../../../../../examples/ui-app/src/routes/view/load/+server';
import { mcpExecutor } from 'wyrd-ai-ui/mcp';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { Bifrost, type WyrdClient } from 'wyrd-ai';
import catalog from 'wyrd-ai-ui/catalog.json';
import { place as placeAll } from '../../packages/ui/src/lib/layout';
import { card, denied, fixtureExecutor, RANGE, registered, series, summary, terminal, timeseries } from '$lib/ui-fixtures';

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
  expect(catalog.view.required).toEqual(['version', 'catalogId', 'range', 'panels']);

  // Timeseries: one line per series value, each with a distinct dash and marker in the legend.
  const plot = render(Chart, { input: timeseries, result: { state: 'populated', data: series } });
  expect(plot.getByRole('heading', { name: 'Latency · ms' })).toBeInTheDocument();
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

  // Up to twenty series keep their own line and a distinct dash and marker; past twenty, the smallest totals sum into "other".
  const models = (count: number): QueryResult => ({ ...series, rows: Array.from({ length: count }, (_, index) => ['2026-10-20T10:00:00Z', `m${index + 1}`, index + 1]), terminal: terminal(count) });
  for (const count of [5, 20, 21]) {
    const many = render(Chart, { input: timeseries, result: { state: 'populated', data: models(count) } });
    const labels = [...many.container.querySelectorAll('.legend li')].map((item) => item.textContent?.trim());
    expect(labels).toHaveLength(Math.min(count, 20));
    expect(labels.at(-1)).toBe(count > 20 ? 'other' : `m${count}`);
    const cues = [...many.container.querySelectorAll('.legend svg')].map((icon) => `${icon.querySelector('line')?.getAttribute('stroke-dasharray')}|${icon.querySelector('path')?.getAttribute('d')}`);
    expect(new Set(cues).size).toBe(labels.length);
    expect(many.container.querySelectorAll('svg.plot polyline')).toHaveLength(labels.length);
    await fireEvent.focus(many.getByRole('application'));
    expect(many.getByRole('status')).toHaveTextContent(count > 20 ? 'other: 3 ms' : 'm1: 1 ms');
    many.unmount();
  }
  // Authored series named "other" keep their values; the aggregate's caption stays distinct from every one of them.
  const grouped = (groups: [string, number][]): QueryResult => ({ ...series, rows: groups.map(([name, value]) => ['2026-10-20T10:00:00Z', name, value]), terminal: terminal(groups.length) });
  const captions = (view: { container: Element }) => [...view.container.querySelectorAll('.legend li')].map((item) => item.textContent?.trim());
  const taken: [string, number][] = [['other', 21], ['other (aggregate)', 20], ...Array.from({ length: 19 }, (_, index): [string, number] => [`s${index + 1}`, index + 1])];
  const named = render(Chart, { input: timeseries, result: { state: 'populated', data: grouped(taken) } });
  expect(captions(named)).toHaveLength(20);
  expect(new Set(captions(named)).size).toBe(20);
  expect(captions(named).slice(0, 2)).toEqual(['other', 'other (aggregate)']);
  expect(captions(named).at(-1)).toBe('other (aggregate 2)');
  expect(named.container.querySelectorAll('svg.plot polyline')).toHaveLength(20);
  expect(named.getByRole('application').getAttribute('aria-label')).toContain('other, other (aggregate), s19');
  await fireEvent.focus(named.getByRole('application'));
  expect(named.getByRole('status')).toHaveTextContent(/other: 21 ms.*other \(aggregate\): 20 ms.*other \(aggregate 2\): 3 ms/);
  // A smaller authored "other" folds into the aggregate, which then needs no qualifier; at five series it keeps its own line.
  await named.rerender({ result: { state: 'populated', data: grouped([...taken.slice(2), ['x', 30], ['y', 40], ['other', 0.5]]) } });
  expect(captions(named)).toHaveLength(20);
  expect(captions(named).slice(0, 2)).toEqual(['y', 'x']);
  expect(captions(named).at(-1)).toBe('other');
  expect(named.getByRole('status')).toHaveTextContent('other: 3.5 ms');
  await named.rerender({ result: { state: 'populated', data: grouped([['other', 21], ['s1', 1], ['s2', 2]]) } });
  expect(captions(named)).toEqual(['other', 's1', 's2']);
  named.unmount();

  // Signed values draw finite geometry around a zero baseline.
  const signed = (values: number[]): QueryResult => ({
    columns: [{ name: 'at', data_type: 'Timestamp(Microsecond, None)', nullable: false }, { name: 'model', data_type: 'Utf8', nullable: false }, { name: 'p95', data_type: 'Float64', nullable: false }],
    rows: values.map((value, index) => [`2026-10-20T1${index}:00:00Z`, `c${index}`, value]),
    terminal: terminal(values.length)
  });
  for (const values of [[-5, 10], [-5, -3], [-5, -5], [0, 0], [3, 8]]) {
    const line = render(Chart, { input: { ...timeseries, series: undefined } as ChartInput, result: { state: 'populated', data: signed(values) } });
    const coordinates = [...line.container.querySelectorAll('svg.plot polyline')].flatMap((each) => each.getAttribute('points')!.split(/[ ,]/).map(Number));
    coordinates.push(...[...line.container.querySelectorAll('svg.plot line.grid')].map((each) => Number(each.getAttribute('y1'))));
    expect(coordinates.every(Number.isFinite), `${values}`).toBe(true);
    line.unmount();
    const bar = render(Chart, { input: { type: 'bar', sql: 'SELECT 1', x: 'model', y: ['p95'] }, result: { state: 'populated', data: signed(values) } });
    const zero = Number(bar.container.querySelector('svg.bars line.grid')!.getAttribute('y1'));
    expect(Number.isFinite(zero), `${values}`).toBe(true);
    [...bar.container.querySelectorAll('rect.bar')].forEach((rect, index) => {
      const [y, height] = [Number(rect.getAttribute('y')), Number(rect.getAttribute('height'))];
      expect(height, `${values}`).toBeGreaterThanOrEqual(0);
      if (values[index] < 0) expect(y).toBe(zero);
      else expect(y + height).toBeCloseTo(zero);
    });
    bar.unmount();
  }

  // Bar, stat and table; the table keeps duplicate names and nulls positionally.
  const bars: QueryResult = { columns: [{ name: 'model', data_type: 'Utf8', nullable: false }, { name: 'runs', data_type: 'Int64', nullable: false }], rows: [['a', 3], ['b', 12]], terminal: terminal(2) };
  const bar = render(Chart, { input: { type: 'bar', sql: 'SELECT 1', x: 'model', y: ['runs'] }, result: { state: 'populated', data: bars } });
  expect(bar.getByRole('img', { name: 'Bar chart: a 3, b 12' })).toBeInTheDocument();
  bar.unmount();
  // Dense bars never overprint: values drop and category text thins to a stride, and every bar keeps its full title.
  const dense = render(Chart, { input: { type: 'bar', sql: 'SELECT 1', x: 'model', y: ['runs'] }, result: { state: 'populated', data: { ...bars, rows: Array.from({ length: 48 }, (_, index) => [`model-${index}`, index]), terminal: terminal(48) } } });
  const axis = dense.container.querySelectorAll('text.axis').length;
  expect(dense.container.querySelectorAll('text.value')).toHaveLength(0);
  expect(axis).toBeGreaterThan(0);
  expect(axis).toBeLessThan(48);
  expect(dense.container.querySelectorAll('rect.bar title')).toHaveLength(48);
  dense.unmount();
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
  expect(shapeChart({ ...timeseries, y: ['model'] } as ChartInput, series)).toMatchObject({ state: 'error', problem: { code: 'WYRD_SPEC_400_VALIDATION', detail: 'y column "model" is Utf8' } });
  expect(shapeChart(timeseries, { ...series, rows: [] }).state).toBe('empty');
  // A duplicated name binds its first column, for validation and drawing alike.
  const v = (data_type: string) => ({ name: 'v', data_type, nullable: false });
  const textFirst: QueryResult = { columns: [v('Utf8'), v('Int64')], rows: [['bad', 3]], terminal: terminal(1) };
  const numberFirst: QueryResult = { columns: [v('Int64'), v('Utf8')], rows: [[3, 'bad']], terminal: terminal(1) };
  const statOf: ChartInput = { type: 'stat', sql: 'SELECT 1', y: ['v'] };
  expect(shapeChart(statOf, textFirst)).toMatchObject({ state: 'error', problem: { detail: 'y column "v" is Utf8' } });
  expect(shapeChart(statOf, numberFirst).state).toBe('populated');
  const firstStat = render(Chart, { input: statOf, result: shapeChart(statOf, numberFirst) });
  expect(firstStat.container.querySelector('.stat')).toHaveTextContent('3');
  firstStat.unmount();
  const at = { name: 'at', data_type: 'Timestamp(Microsecond, None)', nullable: false };
  const lineOf: ChartInput = { type: 'timeseries', sql: 'SELECT 1', x: 'at', y: ['v'], series: 'model' };
  const model = (data_type: string) => ({ name: 'model', data_type, nullable: false });
  expect(shapeChart(lineOf, { columns: [at, model('Int64'), model('Utf8'), v('Float64')], rows: [], terminal: terminal(0) })).toMatchObject({ state: 'error', problem: { detail: 'series column "model" is Int64' } });
  expect(shapeChart(lineOf, { columns: [{ ...at, data_type: 'Utf8' }, at, model('Utf8'), v('Float64')], rows: [], terminal: terminal(0) })).toMatchObject({ state: 'error', problem: { detail: 'x column "at" is Utf8' } });
  expect(shapeChart({ type: 'bar', sql: 'SELECT 1', x: 'v', y: ['n'] }, { columns: [v('Int64'), v('Utf8'), { name: 'n', data_type: 'Int64', nullable: false }], rows: [[1, 'a', 2]], terminal: terminal(1) })).toMatchObject({ state: 'error', problem: { detail: 'x column "v" is Int64' } });

  // CardHeader: exact identity, newest-first versions, lifecycle status, copy feedback and tabs.
  const writeText = vi.fn(async () => {});
  Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
  const header = render(CardHeader, {
    input: { cardRef: card, tabs: [{ id: 'overview', label: 'Overview' }, { id: 'runs', label: 'Runs' }], tab: 'runs' },
    result: { state: 'populated', data: { card: registered, versions: { state: 'populated', data: { items: [summary('1.4.2'), summary('1.4.1', 'deprecated')], truncated: true } } } }
  });
  expect(header.getByRole('heading', { level: 2 })).toHaveTextContent('ranker');
  expect(header.container).toHaveTextContent('ranker@1.4.2');
  expect(header.container).toHaveTextContent('✓active');
  expect([...header.container.querySelectorAll('option')].map((option) => option.value)).toEqual(['1.4.2', '1.4.1']);
  expect(header.container).toHaveTextContent('showing the newest 2 versions');
  expect(header.getByRole('button', { name: 'Runs' })).toHaveAttribute('aria-current', 'page');
  await fireEvent.click(header.getByRole('button', { name: 'Copy uid' }));
  expect(writeText).toHaveBeenCalledWith('card_01HX');
  expect(await header.findByText('Copied')).toBeInTheDocument();
  writeText.mockRejectedValueOnce(new Error('denied'));
  await fireEvent.click(header.getByRole('button', { name: 'Copy ref' }));
  expect(await header.findByText('Copy failed')).toBeInTheDocument();
  header.unmount();
  const noVersions = render(CardHeader, { input: { cardRef: card }, result: { state: 'populated', data: { card: registered, versions: { state: 'denied', problem: denied } } } });
  expect(noVersions.container).toHaveTextContent('versions unavailable: Forbidden (WYRD_AUTH_403_FORBIDDEN)');
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
  expect(chip.getByRole('link')).toHaveTextContent('Model ranker@1.4.2');
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
      version: 'wyrd.view/v1',
      catalogId: 'wyrd.ui/v1',
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
  expect(validateView({ version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: { last: '6x' }, panels: [] })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  // A document from another format version or catalog is refused, never half-rendered.
  const panels = [{ id: 'a', component: 'Chart', input: timeseries }];
  expect(validateView({ version: 'wyrd.view/v2', catalogId: 'wyrd.ui/v1', range: RANGE, panels })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect(validateView({ version: 'wyrd.view/v1', range: RANGE, panels })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect(validatePanel('CardHeader', { cardRef: { ...card, space: 'search' } })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION', detail: expect.stringContaining('space') });
  expect(validateView({ version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: RANGE, panels })).not.toHaveProperty('code');
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
  expect(paged.calls[1].request).toMatchObject({ op: 'cards.list', kind: 'Model', name: 'ranker', include_prerelease: true, cursor: null });
  expect(header.result).toMatchObject({ state: 'populated', data: { versions: { state: 'populated', data: { truncated: true } } } });
  const unlisted = await loadCardHeader({ cardRef: card }, { executor: recording((request) => (request.op === 'cards.list' ? { ok: false, problem: denied } : { ok: true, value: registered })).executor });
  expect(unlisted.result).toMatchObject({ state: 'populated', data: { versions: { state: 'denied' } } });

  // Authored SQL is never rewritten; only referenced range binds are sent.
  expect(chartParams('SELECT 1', undefined)).toEqual([]);
  expect(chartParams('SELECT $1', undefined)).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect(chartParams('WHERE at >= $1 AND at < $2 GROUP BY date_bin($3::interval, at)', RANGE)).toEqual([RANGE.from, RANGE.to, '5 minutes']);
  const chart = recording(() => ({ ok: true, value: series }));
  const loaded = await loadChart(timeseries, { executor: chart.executor, range: RANGE });
  expect(chart.calls[0].request).toEqual({ op: 'bifrost.query', sql: timeseries.sql, params: [RANGE.from, RANGE.to] });
  expect(loaded.executed).toEqual({ sql: timeseries.sql, params: [RANGE.from, RANGE.to] });

  // A relative range resolves once per view load; every panel binds the same bounds.
  const pending: ((result: WireResult) => void)[] = [];
  const held = recording(() => new Promise<WireResult>((resolve) => pending.push(resolve)));
  const doc = { version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: { last: '6h' }, panels: [{ id: 'a', component: 'Chart', input: timeseries }, { id: 'b', component: 'Chart', input: { ...timeseries, title: 'B' } }] };
  const onaction = vi.fn();
  const view = render(WyrdView, { doc, executor: held.executor, onaction });
  await waitFor(() => expect(held.calls).toHaveLength(2));
  const params = (index: number) => (held.calls[index].request as Extract<WireRequest, { op: 'bifrost.query' }>).params as string[];
  const [first, second] = [params(0), params(1)];
  expect(first).toEqual(second);
  expect(Date.parse(first[1]) - Date.parse(first[0])).toBe(6 * 3_600_000);

  // A replaced document aborts in-flight loads and a late result is never accepted.
  await view.rerender({ doc: { version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: RANGE, panels: [{ id: 'a', component: 'Chart', input: { ...timeseries, title: 'Replaced' } }] }, executor: held.executor, onaction });
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

  // An explicit panel range wins over the view's, before and after a brush and its reset.
  const OWN = { from: '2026-10-19T00:00:00Z', to: '2026-10-19T06:00:00Z' };
  const mixed = recording(() => ({ ok: true, value: series }));
  const mixedDoc = { version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: RANGE, panels: [{ id: 'own', component: 'Chart', input: { ...timeseries, range: OWN } }, { id: 'shared', component: 'Chart', input: timeseries }] };
  const both = render(WyrdView, { doc: mixedDoc, executor: mixed.executor });
  const binds = () => mixed.calls.slice(-2).map((call) => (call.request as Extract<WireRequest, { op: 'bifrost.query' }>).params);
  await waitFor(() => expect(mixed.calls).toHaveLength(2));
  expect(binds()).toEqual([[OWN.from, OWN.to], [RANGE.from, RANGE.to]]);
  await waitFor(() => expect(both.container.querySelector('[data-panel="shared"] svg.plot')).not.toBeNull());
  const sharedPlot = both.container.querySelector('[data-panel="shared"] svg.plot') as SVGSVGElement;
  sharedPlot.getBoundingClientRect = plot.getBoundingClientRect;
  await fireEvent.pointerDown(sharedPlot, { clientX: 100, pointerId: 1 });
  await fireEvent.pointerMove(sharedPlot, { clientX: 400, pointerId: 1 });
  await fireEvent.pointerUp(sharedPlot, { clientX: 400, pointerId: 1 });
  await waitFor(() => expect(mixed.calls).toHaveLength(4));
  const [brushedOwn, brushedShared] = binds();
  expect(brushedOwn).toEqual([OWN.from, OWN.to]);
  expect(brushedShared).not.toEqual([RANGE.from, RANGE.to]);
  await fireEvent.click(both.container.querySelector('.range button')!);
  await waitFor(() => expect(mixed.calls).toHaveLength(6));
  expect(binds()).toEqual([[OWN.from, OWN.to], [RANGE.from, RANGE.to]]);
  both.unmount();
  // A relative panel range resolves against the clock, not the view's bounds.
  vi.useFakeTimers({ now: Date.parse('2026-10-20T12:00:00Z'), toFake: ['Date'] });
  try {
    const relative = recording(() => ({ ok: true, value: series }));
    const own = await loadChart({ ...timeseries, range: { last: '1h' } }, { executor: relative.executor, range: RANGE });
    expect(own.executed?.params).toEqual(['2026-10-20T11:00:00.000Z', '2026-10-20T12:00:00.000Z']);
  } finally {
    vi.useRealTimers();
  }

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

  // CardHeader version and tab controls show their host-reported state and never send during pending.
  const tabs = [{ id: 'overview', label: 'Overview' }, { id: 'runs', label: 'Runs' }];
  const listed = { state: 'populated', data: { card: registered, versions: { state: 'populated', data: { items: [summary('1.4.2'), summary('1.4.1')], truncated: false } } } } as const;
  const headerAction = vi.fn();
  const controls = render(CardHeader, { input: { cardRef: card, tabs }, result: listed, onaction: headerAction, actions: { 'card.version': { state: 'pending' }, 'card.tab': { state: 'pending' } } });
  const select = controls.getByRole('combobox', { name: 'Version' }) as HTMLSelectElement;
  expect(select).toBeDisabled();
  expect(controls.getByRole('button', { name: 'Runs' })).toBeDisabled();
  for (const [state, text] of [
    [{ state: 'denied', problem: denied }, 'Forbidden (WYRD_AUTH_403_FORBIDDEN)'],
    [{ state: 'error', problem: { code: 'WYRD_SPEC_500_INTERNAL', title: 'Internal error' } }, 'Internal error (WYRD_SPEC_500_INTERNAL)'],
    [{ state: 'unavailable', reason: 'No navigation here' }, 'No navigation here'],
    [{ state: 'success', message: 'Opened 1.4.1' }, 'Opened 1.4.1']
  ] as const) {
    await controls.rerender({ input: { cardRef: card, tabs }, result: listed, onaction: headerAction, actions: { 'card.version': state, 'card.tab': state } });
    expect(controls.getAllByText(text)).toHaveLength(2);
    expect(select.disabled).toBe(state.state === 'unavailable');
  }
  // A refused or pending change leaves the selection on the version the host passed.
  await fireEvent.change(select, { target: { value: '1.4.1' } });
  expect(headerAction).toHaveBeenCalledExactlyOnceWith({ name: 'card.version', input: { card_ref: { ...card, version: '1.4.1' } } });
  expect(select.value).toBe('1.4.2');
  await fireEvent.click(controls.getByRole('button', { name: 'Runs' }));
  expect(headerAction).toHaveBeenLastCalledWith({ name: 'card.tab', input: { tab: 'runs' } });
  expect(controls.getByRole('button', { name: 'Overview' })).toHaveAttribute('aria-current', 'page');
  controls.unmount();
  // Through a view, a version change reaches the host once and its refusal shows.
  let refuse: (state: { state: 'denied'; problem: typeof denied }) => void = () => {};
  const viewAction = vi.fn(() => new Promise<{ state: 'denied'; problem: typeof denied }>((resolve) => (refuse = resolve)));
  const headed = render(WyrdView, { doc: { version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: RANGE, panels: [{ id: 'h', component: 'CardHeader', input: { cardRef: card } }] }, executor: fixtureExecutor, onaction: viewAction });
  await waitFor(() => expect(headed.getByRole('combobox', { name: 'Version' })).toBeEnabled());
  const viewSelect = headed.getByRole('combobox', { name: 'Version' });
  await fireEvent.change(viewSelect, { target: { value: '1.4.1' } });
  await fireEvent.change(viewSelect, { target: { value: '1.4.1' } });
  expect(viewAction).toHaveBeenCalledOnce();
  refuse({ state: 'denied', problem: denied });
  expect(await headed.findByText('Forbidden (WYRD_AUTH_403_FORBIDDEN)')).toBeInTheDocument();
  expect((viewSelect as HTMLSelectElement).value).toBe('1.4.2');
  headed.unmount();

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

test('Node query acquisition cancellation', async () => {
  const query = [{ op: 'bifrost.query', sql: 'SELECT 1' }] as const;
  const close = vi.fn(async () => ({ done: true as const, value: undefined }));
  let collected: (result: QueryResult) => void = () => {};
  const collect = vi.fn(() => new Promise<QueryResult>((resolve) => (collected = resolve)));
  let started: ((stream: unknown) => void) | undefined;
  const begin = vi.fn(() => new Promise((resolve) => (started = resolve)));
  const connect = vi.spyOn(Bifrost, 'connect').mockResolvedValue({ stream: begin } as never);
  try {
    // Cancelled while the stream starts: it is closed once, never read, and the read never succeeds.
    const starting = new AbortController();
    const run = nodeExecutor({} as WyrdClient).run(query, starting.signal);
    await vi.waitFor(() => expect(started).toBeDefined());
    starting.abort();
    started!({ return: close, collect });
    await expect(run).rejects.toThrow();
    expect(close).toHaveBeenCalledOnce();
    expect(collect).not.toHaveBeenCalled();

    // Cancelled while collecting: closed once, and rows arriving after cancellation are not a success.
    started = undefined;
    const collecting = new AbortController();
    const running = nodeExecutor({} as WyrdClient).run(query, collecting.signal);
    await vi.waitFor(() => expect(started).toBeDefined());
    started!({ return: close, collect });
    await vi.waitFor(() => expect(collect).toHaveBeenCalledOnce());
    collecting.abort();
    collected(series);
    await expect(running).rejects.toThrow();
    expect(close).toHaveBeenCalledTimes(2);

    // Already cancelled: no stream starts.
    begin.mockClear();
    await expect(nodeExecutor({} as WyrdClient).run(query, AbortSignal.abort())).rejects.toThrow();
    expect(begin).not.toHaveBeenCalled();
  } finally {
    connect.mockRestore();
  }
});

test('Canonical UI problem projections', async () => {
  // Local failures carry a catalog problem, with the safe context in `detail`.
  const { validation, internal, upstream } = examples;
  expect(chartParams('SELECT $1', undefined)).toEqual({ ...validation, detail: 'The SQL binds $1..$3 but no range was supplied.' });
  expect(shapeChart({ ...timeseries, y: ['model'] } as ChartInput, series)).toEqual({ state: 'error', problem: { ...validation, detail: 'y column "model" is Utf8' } });
  expect(validateIntent('Chart', { name: 'card.tab', input: {} })).toEqual({ ...validation, detail: 'Chart does not emit card.tab' });
  expect(validateView({})).toMatchObject({ ...validation, detail: expect.stringContaining('View input.version') });
  const [mcpFailure] = await mcpExecutor({ callServerTool: async () => ({ content: [], isError: true }) } as never).run([{ op: 'bifrost.list_tables' }]);
  expect(mcpFailure).toEqual({ ok: false, problem: { ...upstream, detail: 'bifrost.list_tables returned an error' } });
  const remote = remoteLoader('/load', async () => new Response('{}', { status: 502 }));
  expect(await remote('Chart', timeseries, { range: RANGE, signal: new AbortController().signal })).toEqual({ ...upstream, detail: 'The panel endpoint answered 502' });
  // An upstream refusal keeps its own identity and its denied state.
  const refused = await loadChart(timeseries, { executor: recording(() => ({ ok: false, problem: denied })).executor, range: RANGE });
  expect(refused.result).toEqual({ state: 'denied', problem: denied });

  // A host that cannot load, a rejected load and a failed action render catalog problems.
  const doc = { version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: RANGE, panels: [{ id: 'a', component: 'Chart', input: timeseries }] };
  const unloaded = render(WyrdView, { doc });
  expect(await unloaded.findByText(internal.code)).toBeInTheDocument();
  unloaded.unmount();
  const rejected = render(WyrdView, { doc, load: () => Promise.reject(new Error('offline')) });
  expect(await rejected.findByText(internal.code)).toBeInTheDocument();
  rejected.unmount();
  const failing = render(WyrdView, { doc, executor: recording(() => ({ ok: true, value: series })).executor, onaction: () => Promise.reject(new Error('boom')) });
  await fireEvent.click(await failing.findByRole('button', { name: 'Open in Query' }));
  expect(await failing.findByRole('alert')).toHaveTextContent(`${internal.title} (${internal.code})`);
  failing.unmount();

  // Both panel endpoints answer a malformed request with the full canonical problem.
  for (const POST of [bffLoad, exampleLoad]) {
    const response = await POST({ locals: { browserSession: { client: () => ({}) } }, request: new Request('http://ui/load', { method: 'POST', body: '{"component":1}' }) } as never);
    expect(response.status).toBe(400);
    expect(response.headers.get('content-type')).toBe('application/problem+json');
    expect(await response.json()).toEqual({ ...validation, detail: expect.stringMatching(/^Panel input/) });
  }

  // Browser exports load no native SDK at runtime.
  const browser = import.meta.glob('/packages/ui/src/lib/{index,contract,loaders,view,plot,format,schemas,layout,mcp}.ts', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;
  expect(Object.keys(browser)).toHaveLength(9);
  for (const [path, source] of Object.entries(browser)) expect(source, path).not.toMatch(/^(import|export) (?!type )[^;]*from 'wyrd-ai'/m);
});

/** A view document over Chart panels with the given layouts; `undefined` leaves a panel unplaced. */
function laidOut(layouts: Record<string, { x: number; y: number; w: number; h: number } | undefined>) {
  return {
    version: 'wyrd.view/v1',
    catalogId: 'wyrd.ui/v1',
    range: RANGE,
    panels: Object.entries(layouts).map(([id, layout]) => ({ id, component: 'Chart', input: { ...timeseries, title: id }, ...(layout && { layout }) }))
  };
}

/** The rendered panel element for `id`. */
const panel = (container: Element, id: string) => container.querySelector<HTMLElement>(`[data-panel="${id}"]`)!;

test('TASK-027: Layout document contract', () => {
  expect(validateView(laidOut({ a: { x: 0, y: 0, w: 8, h: 6 }, b: undefined }))).not.toHaveProperty('code');
  for (const layout of [{ x: 12, y: 0, w: 1, h: 1 }, { x: 0, y: 0, w: 0, h: 1 }, { x: 8, y: 0, w: 5, h: 1 }, { x: 0, y: -1, w: 1, h: 1 }, { x: 0, y: 0, w: 1, h: 49 }, { x: 0, y: 0, w: 1, h: 1.5 }, { x: 0, y: 0, w: 4 }])
    expect(validateView(laidOut({ a: layout as never })), JSON.stringify(layout)).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION', detail: expect.stringContaining('layout') });
  const sized = laidOut({ a: undefined });
  expect(validateView({ ...sized, panels: [{ ...sized.panels[0], width: 'full' }] })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION', detail: expect.stringContaining('width') });
  // The published view schema carries layout; no component entry gains a size input.
  expect(Object.keys(catalog.view.properties.panels.items.properties)).toContain('layout');
  expect(JSON.stringify(catalog.components)).not.toMatch(/"(layout|width|height)"/);
});

test('TASK-027: Grid placement', async () => {
  const chart = recording(() => ({ ok: true, value: series }));
  // `d` collides with `a`, and `c` is unplaced, so `d` moves below both; the document is untouched.
  const doc = laidOut({ a: { x: 0, y: 0, w: 8, h: 6 }, b: { x: 8, y: 0, w: 4, h: 6 }, c: undefined, d: { x: 0, y: 0, w: 6, h: 4 } });
  const written = structuredClone(doc);
  const view = render(WyrdView, { doc, executor: chart.executor });
  const place = (id: string) => [panel(view.container, id).style.gridColumn, panel(view.container, id).style.gridRow];
  expect(place('a')).toEqual(['1 / span 8', '1 / span 6']);
  expect(place('b')).toEqual(['9 / span 4', '1 / span 6']);
  expect(place('c')).toEqual(['1 / span 12', '7 / span 1']);
  expect(place('d')).toEqual(['1 / span 6', '8 / span 4']);
  // Placed panels hold their rows; an unplaced panel takes its content height.
  expect(panel(view.container, 'a').style.height).not.toBe('');
  expect(panel(view.container, 'c').style.height).toBe('');
  expect(doc).toEqual(written);
  view.unmount();

  // Without layout, panels flow full width in document order, as before.
  const plain = render(WyrdView, { doc: laidOut({ a: undefined, b: undefined }), executor: chart.executor });
  expect([panel(plain.container, 'a').style.gridRow, panel(plain.container, 'b').style.gridRow]).toEqual(['1 / span 1', '2 / span 1']);
  expect(panel(plain.container, 'b').style.gridColumn).toBe('1 / span 12');
  plain.unmount();

  // An unplaced panel occupies the whole rows its content needs; rows keep their fixed pitch.
  const fixed = { x: 0, y: 4, w: 12, h: 4 };
  expect(placeAll([undefined, fixed], [72.1875])).toEqual([{ x: 0, y: 0, w: 12, h: 2, auto: true }, { ...fixed, auto: false }]);
  expect(placeAll([undefined, fixed], [294])).toEqual([{ x: 0, y: 0, w: 12, h: 6, auto: true }, { ...fixed, y: 6, auto: false }]);
  expect(placeAll([undefined, undefined], [0, 2000])).toEqual([{ x: 0, y: 0, w: 12, h: 1, auto: true }, { x: 0, y: 1, w: 12, h: 39, auto: true }]);
  // The renderer measures that content and places from it, without stretching the panel or writing the document.
  let tall = 294;
  const height = vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockImplementation(function (this: HTMLElement) {
    return this.dataset.panel === 'tall' || this.dataset.panel === '__proto__' ? tall : 0;
  });
  try {
    const mixed = laidOut({ tall: undefined, after: fixed });
    const kept = structuredClone(mixed);
    const measured = render(WyrdView, { doc: mixed, executor: chart.executor });
    expect([panel(measured.container, 'tall').style.gridRow, panel(measured.container, 'after').style.gridRow]).toEqual(['1 / span 6', '7 / span 4']);
    expect(panel(measured.container, 'tall').style.height).toBe('');
    expect(mixed).toEqual(kept);
    measured.unmount();

    // Panel ids that name Object.prototype members place like any other, authored and measured.
    const ids = ['constructor', 'toString', '__proto__'];
    const renamed = (names: string[]) =>
      laidOut(Object.fromEntries([[names[0], { x: 0, y: 0, w: 6, h: 4 }], [names[1], undefined], [names[2], { x: 6, y: 0, w: 6, h: 4 }], ['n', { x: 0, y: 4, w: 12, h: 2 }]]));
    const rows = (view: { container: Element }, names: string[]) => [...names, 'n'].map((id) => [panel(view.container, id).style.gridColumn, panel(view.container, id).style.gridRow, panel(view.container, id).style.height]);
    const special = render(WyrdView, { doc: renamed(ids), executor: chart.executor });
    const plain = render(WyrdView, { doc: renamed(['p', 'q', 'r']), executor: chart.executor });
    expect(rows(special, ids)).toEqual(rows(plain, ['p', 'q', 'r']));
    expect(rows(special, ids)[0].slice(0, 2)).toEqual(['1 / span 6', '1 / span 4']);
    special.unmount();
    plain.unmount();
    // A measured `__proto__` panel occupies its content rows, and follows its content when that changes.
    const grows = render(WyrdView, { doc: laidOut(Object.fromEntries([['__proto__', undefined], ['constructor', fixed]])), executor: chart.executor });
    expect(panel(grows.container, 'constructor').style.gridRow).toBe('7 / span 4');
    tall = 400;
    (ResizeObserver as unknown as { notify(): void }).notify();
    await tick();
    expect(panel(grows.container, '__proto__').style.gridRow).toBe('1 / span 8');
    expect(panel(grows.container, 'constructor').style.gridRow).toBe('9 / span 4');
    grows.unmount();
  } finally {
    height.mockRestore();
  }
});

test('TASK-027: Layout editing', async () => {
  const chart = recording(() => ({ ok: true, value: series }));
  const doc = laidOut({ a: { x: 0, y: 0, w: 8, h: 6 }, b: { x: 8, y: 0, w: 4, h: 6 } });
  const written = structuredClone(doc);

  // Without a host handler there is nothing to grab.
  const readonly = render(WyrdView, { doc, executor: chart.executor });
  expect(readonly.queryByRole('button', { name: /Move or resize/ })).toBeNull();
  readonly.unmount();

  const onlayout = vi.fn();
  const edited = recording(() => ({ ok: true, value: series }));
  const view = render(WyrdView, { doc, executor: edited.executor, onlayout });
  await waitFor(() => expect(edited.calls).toHaveLength(2));
  const handle = view.getByRole('button', { name: 'Move or resize a' });
  handle.focus();
  // Arrows move and Shift+arrows resize as a preview; Enter completes one change.
  await fireEvent.keyDown(handle, { key: 'ArrowRight' });
  await fireEvent.keyDown(handle, { key: 'ArrowDown', shiftKey: true });
  expect([panel(view.container, 'a').style.gridColumn, panel(view.container, 'a').style.gridRow]).toEqual(['2 / span 8', '1 / span 7']);
  expect(onlayout).not.toHaveBeenCalled();
  await fireEvent.keyDown(handle, { key: 'Enter' });
  expect(onlayout).toHaveBeenCalledTimes(1);
  const updated = onlayout.mock.calls[0][0];
  expect(updated).toEqual({ ...written, panels: [{ ...written.panels[0], layout: { x: 1, y: 0, w: 8, h: 7 } }, written.panels[1]] });
  expect(doc).toEqual(written);

  // Escape restores the layout from before the edit and reports nothing.
  await fireEvent.keyDown(handle, { key: 'ArrowDown' });
  await fireEvent.keyDown(handle, { key: 'Escape' });
  expect(panel(view.container, 'a').style.gridRow).toBe('1 / span 7');
  expect(onlayout).toHaveBeenCalledTimes(1);

  // Edits stay inside the grid.
  for (let step = 0; step < 6; step++) await fireEvent.keyDown(handle, { key: 'ArrowRight', shiftKey: true });
  for (let step = 0; step < 4; step++) await fireEvent.keyDown(handle, { key: 'ArrowLeft' });
  await fireEvent.keyDown(handle, { key: 'ArrowUp' });
  await fireEvent.keyDown(handle, { key: 'Enter' });
  expect(onlayout.mock.calls[1][0].panels[0].layout).toEqual({ x: 0, y: 0, w: 11, h: 7 });

  // Neither editing nor the host keeping the edited document reloads a panel.
  await view.rerender({ doc: onlayout.mock.calls[1][0], executor: edited.executor, onlayout });
  await tick();
  expect(edited.calls).toHaveLength(2);
  view.unmount();

  // A host that never echoes still receives every completed edit: each callback carries the earlier ones.
  // Collision and measured height stay out of the document: `c` keeps no layout and `d` keeps its authored box.
  const siblings = laidOut({ a: { x: 0, y: 0, w: 8, h: 6 }, b: { x: 8, y: 0, w: 4, h: 6 }, c: undefined, d: { x: 0, y: 0, w: 6, h: 4 } });
  const authored = structuredClone(siblings);
  const recorded = vi.fn();
  const quiet = recording(() => ({ ok: true, value: series }));
  const silent = render(WyrdView, { doc: siblings, executor: quiet.executor, onlayout: recorded });
  await waitFor(() => expect(quiet.calls).toHaveLength(4));
  for (const id of ['a', 'b']) {
    const grip = silent.getByRole('button', { name: `Move or resize ${id}` });
    grip.focus();
    await fireEvent.keyDown(grip, { key: 'ArrowDown' });
    await fireEvent.keyDown(grip, { key: 'Enter' });
  }
  expect(recorded).toHaveBeenCalledTimes(2);
  const latest = recorded.mock.calls[1][0];
  expect(latest).toEqual({ ...authored, panels: [{ ...authored.panels[0], layout: { x: 0, y: 1, w: 8, h: 6 } }, { ...authored.panels[1], layout: { x: 8, y: 1, w: 4, h: 6 } }, authored.panels[2], authored.panels[3]] });
  expect(latest.panels[2]).not.toHaveProperty('layout');
  expect(siblings).toEqual(authored);
  await silent.rerender({ doc: latest, executor: quiet.executor, onlayout: recorded });
  await tick();
  expect([panel(silent.container, 'a').style.gridRow, panel(silent.container, 'b').style.gridRow]).toEqual(['2 / span 6', '2 / span 6']);
  expect(quiet.calls).toHaveLength(4);
  silent.unmount();

  // Ids naming Object.prototype members edit like any other; untouched ones keep their layout, or its absence.
  const named = laidOut(Object.fromEntries([['a', { x: 0, y: 0, w: 4, h: 4 }], ['constructor', { x: 4, y: 0, w: 4, h: 4 }], ['toString', undefined], ['__proto__', { x: 8, y: 0, w: 4, h: 4 }]]));
  const original = structuredClone(named);
  const told = vi.fn();
  const odd = render(WyrdView, { doc: named, executor: quiet.executor, onlayout: told });
  const press = async (id: string, ...keys: string[]) => {
    const grip = odd.getByRole('button', { name: `Move or resize ${id}` });
    grip.focus();
    for (const key of keys) await fireEvent.keyDown(grip, { key });
  };
  await press('a', 'ArrowDown', 'Enter');
  expect(validateView(told.mock.lastCall![0])).not.toHaveProperty('code');
  expect(told.mock.lastCall![0].panels.slice(1)).toEqual(original.panels.slice(1));
  await press('constructor', 'ArrowDown', 'Escape');
  expect(panel(odd.container, 'constructor').style.gridRow).toBe('1 / span 4');
  expect(told).toHaveBeenCalledOnce();
  for (const id of ['constructor', 'toString', '__proto__']) await press(id, 'ArrowRight', 'Enter');
  expect(told).toHaveBeenCalledTimes(4);
  const last = told.mock.lastCall![0];
  expect(validateView(last)).not.toHaveProperty('code');
  expect(last.panels.map((each: { layout?: unknown }) => each.layout)).toEqual([
    { x: 0, y: 1, w: 4, h: 4 },
    { x: 5, y: 0, w: 4, h: 4 },
    // Full width cannot move right; `__proto__` starts from where collision moved it.
    { x: 0, y: 5, w: 12, h: 1 },
    { x: 8, y: 6, w: 4, h: 4 }
  ]);
  expect(Object.hasOwn(last.panels[3], 'layout')).toBe(true);
  expect(named).toEqual(original);
  odd.unmount();
});

test('TASK-027: Pointer edit cancellation', async () => {
  // jsdom has no pointer capture; give it the platform methods the browser provides.
  HTMLElement.prototype.setPointerCapture ??= () => {};
  HTMLElement.prototype.releasePointerCapture ??= () => {};
  const onlayout = vi.fn();
  const chart = recording(() => ({ ok: true, value: series }));
  const view = render(WyrdView, { doc: laidOut({ a: { x: 0, y: 0, w: 8, h: 6 } }), executor: chart.executor, onlayout });
  const grip = view.getByRole('button', { name: 'Move or resize a' });
  const corner = panel(view.container, 'a').querySelector('.corner')!;
  const box = () => [panel(view.container, 'a').style.gridColumn, panel(view.container, 'a').style.gridRow];
  const original = box();
  // Escape ends a held move or resize, focused on the grip or not; later movement and release change nothing.
  for (const [handle, focused] of [[grip, true], [grip, false], [corner, false]] as const) {
    if (focused) grip.focus();
    else grip.blur();
    await fireEvent.pointerDown(handle, { button: 0, clientX: 0, clientY: 0, pointerId: 1 });
    await fireEvent.pointerMove(handle, { clientX: 2, clientY: 104, pointerId: 1 });
    expect(box()).not.toEqual(original);
    await fireEvent.keyDown(focused ? grip : document.body, { key: 'Escape' });
    expect(box()).toEqual(original);
    await fireEvent.pointerMove(handle, { clientX: 3, clientY: 156, pointerId: 1 });
    await fireEvent.pointerUp(handle, { clientX: 3, clientY: 156, pointerId: 1 });
    expect(box()).toEqual(original);
    expect(onlayout).not.toHaveBeenCalled();
  }
  // A fresh gesture still completes once; a cancelled pointer reports nothing.
  await fireEvent.pointerDown(corner, { button: 0, clientX: 0, clientY: 0, pointerId: 2 });
  await fireEvent.pointerMove(corner, { clientX: 0, clientY: 52, pointerId: 2 });
  await fireEvent.pointerUp(corner, { clientX: 0, clientY: 52, pointerId: 2 });
  expect(onlayout).toHaveBeenCalledOnce();
  expect(onlayout.mock.calls[0][0].panels[0].layout).toEqual({ x: 0, y: 0, w: 8, h: 7 });
  await fireEvent.pointerDown(grip, { button: 0, clientX: 0, clientY: 0, pointerId: 3 });
  await fireEvent.pointerMove(grip, { clientX: 0, clientY: 52, pointerId: 3 });
  await fireEvent.pointerCancel(grip, { pointerId: 3 });
  expect(box()).toEqual(['1 / span 8', '1 / span 7']);
  expect(onlayout).toHaveBeenCalledOnce();
  view.unmount();

  // A host that never echoes: every completion of a held gesture ends it, so later movement and release are inert.
  const quiet = vi.fn();
  const held = render(WyrdView, { doc: laidOut({ a: { x: 0, y: 0, w: 8, h: 6 }, b: { x: 8, y: 0, w: 4, h: 6 } }), executor: chart.executor, onlayout: quiet });
  const heldGrip = held.getByRole('button', { name: 'Move or resize a' });
  const heldCorner = panel(held.container, 'a').querySelector('.corner')!;
  const at = () => [panel(held.container, 'a').style.gridColumn, panel(held.container, 'a').style.gridRow];
  const sibling = held.getByRole('button', { name: 'Move or resize b' });
  sibling.focus();
  await fireEvent.keyDown(sibling, { key: 'ArrowDown' });
  await fireEvent.keyDown(sibling, { key: 'Enter' });
  expect(quiet).toHaveBeenCalledOnce();
  const ends = [(grip: HTMLElement) => grip.blur(), (grip: HTMLElement) => fireEvent.keyDown(grip, { key: 'Enter' })];
  for (const [index, finish] of ends.entries()) {
    for (const handle of [heldGrip, heldCorner]) {
      const before = quiet.mock.calls.length;
      heldGrip.focus();
      await fireEvent.pointerDown(handle, { button: 0, clientX: 0, clientY: 0, pointerId: 10 + index });
      await fireEvent.pointerMove(handle, { clientX: 0, clientY: 52, pointerId: 10 + index });
      await finish(heldGrip);
      await tick();
      expect(quiet).toHaveBeenCalledTimes(before + 1);
      const settled = at();
      expect(panel(held.container, 'a').classList.contains('editing')).toBe(false);
      await fireEvent.pointerMove(handle, { clientX: 0, clientY: 156, pointerId: 10 + index });
      expect(at()).toEqual(settled);
      await fireEvent.pointerUp(handle, { clientX: 0, clientY: 156, pointerId: 10 + index });
      expect(at()).toEqual(settled);
      expect(quiet).toHaveBeenCalledTimes(before + 1);
      // Each callback keeps the sibling's earlier edit.
      expect(quiet.mock.lastCall![0].panels[1].layout).toEqual({ x: 8, y: 1, w: 4, h: 6 });
    }
  }
  // Completing before any movement reports nothing and leaves nothing listening.
  const steady = at();
  heldGrip.focus();
  await fireEvent.pointerDown(heldGrip, { button: 0, clientX: 0, clientY: 0, pointerId: 20 });
  heldGrip.blur();
  await fireEvent.pointerMove(heldGrip, { clientX: 0, clientY: 104, pointerId: 20 });
  await fireEvent.pointerUp(heldGrip, { clientX: 0, clientY: 104, pointerId: 20 });
  expect(at()).toEqual(steady);
  const settledCalls = quiet.mock.calls.length;
  expect(settledCalls).toBe(5);
  // A fresh gesture afterwards still completes exactly once.
  await fireEvent.pointerDown(heldCorner, { button: 0, clientX: 0, clientY: 0, pointerId: 21 });
  await fireEvent.pointerMove(heldCorner, { clientX: 0, clientY: 52, pointerId: 21 });
  await fireEvent.pointerUp(heldCorner, { clientX: 0, clientY: 52, pointerId: 21 });
  expect(quiet).toHaveBeenCalledTimes(settledCalls + 1);

  // Losing editing mid-gesture, by narrowing or by the host dropping its handler, discards only the preview.
  const kept = at();
  const width = vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(1000);
  const notify = () => (ResizeObserver as unknown as { notify(): void }).notify();
  try {
    notify();
    await tick();
    for (const focused of [true, false]) {
      for (const corner of [false, true]) {
        const grip = held.getByRole('button', { name: 'Move or resize a' });
        const handle = corner ? panel(held.container, 'a').querySelector('.corner')! : grip;
        if (focused) grip.focus();
        await fireEvent.pointerDown(handle, { button: 0, clientX: 0, clientY: 0, pointerId: 30 });
        await fireEvent.pointerMove(handle, { clientX: 0, clientY: 104, pointerId: 30 });
        expect(at()).not.toEqual(kept);
        width.mockReturnValue(500);
        notify();
        await tick();
        expect(held.queryByRole('button', { name: /Move or resize/ })).toBeNull();
        await fireEvent.pointerMove(handle, { clientX: 0, clientY: 156, pointerId: 30 });
        await fireEvent.pointerUp(handle, { clientX: 0, clientY: 156, pointerId: 30 });
        width.mockReturnValue(1000);
        notify();
        await tick();
        expect(at()).toEqual(kept);
        expect(held.container.querySelector('.editing')).toBeNull();
        expect(quiet).toHaveBeenCalledTimes(settledCalls + 1);
      }
    }
    for (const focused of [true, false]) {
      const grip = held.getByRole('button', { name: 'Move or resize a' });
      if (focused) grip.focus();
      await fireEvent.pointerDown(grip, { button: 0, clientX: 0, clientY: 0, pointerId: 40 });
      await fireEvent.pointerMove(grip, { clientX: 0, clientY: 104, pointerId: 40 });
      await held.rerender({ onlayout: undefined });
      expect(held.queryByRole('button', { name: /Move or resize/ })).toBeNull();
      await fireEvent.pointerUp(grip, { clientX: 0, clientY: 104, pointerId: 40 });
      await held.rerender({ onlayout: quiet });
      expect(at()).toEqual(kept);
      expect(held.container.querySelector('.editing')).toBeNull();
      expect(quiet).toHaveBeenCalledTimes(settledCalls + 1);
    }
  } finally {
    width.mockRestore();
  }
  held.unmount();
});

test('TASK-027: Narrow views', () => {
  const width = vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(500);
  try {
    const chart = recording(() => ({ ok: true, value: series }));
    const doc = laidOut({ a: { x: 6, y: 0, w: 6, h: 4 }, b: { x: 0, y: 0, w: 6, h: 4 }, c: { x: 0, y: 4, w: 12, h: 2 } });
    const view = render(WyrdView, { doc, executor: chart.executor, onlayout: vi.fn() });
    expect([...view.container.querySelectorAll('[data-panel]')].map((each) => each.getAttribute('data-panel'))).toEqual(['b', 'a', 'c']);
    expect(panel(view.container, 'a').style.gridColumn).toBe('1 / -1');
    expect(panel(view.container, 'a').style.height).not.toBe('');
    expect(view.queryByRole('button', { name: /Move or resize/ })).toBeNull();
    view.unmount();
  } finally {
    width.mockRestore();
  }
});
