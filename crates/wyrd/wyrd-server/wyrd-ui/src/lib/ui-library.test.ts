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

import { fireEvent, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { vi } from 'vitest';
import {
  ArtifactTable,
  CardFacts,
  CardHeader,
  CardInventory,
  CardRefChip,
  PromptMessages,
  PromptVariables,
  RelationshipList,
  UsedBy,
  VersionDiff,
  fmtYaml,
  Chart,
  Drawer,
  DurationBar,
  EmptyState,
  FacetRail,
  fmtCardRef,
  RecordDetail,
  RecordTable,
  Sparkline,
  StatStrip,
  TraceWaterfall,
  StatusPill,
  VerdictPill,
  type CardRef,
  type Executor,
  type QueryResult,
  type WireRequest,
  type WireResult
} from 'wyrd-ai-ui';
import {
  chartParams,
  facetQuery,
  histogramWindows,
  inventoryFilter,
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
  loadRecordDetail,
  loadRecordTable,
  loadTraceWaterfall,
  parseNanos,
  shapeChart,
  spanTree,
  statQueries,
  validateIntent,
  validatePanel,
  type ChartInput
} from 'wyrd-ai-ui/loaders';
import { mountView, remoteLoader, validateView, WyrdView } from 'wyrd-ai-ui/view';
import examples from '../../../../../wyrd-spec/schemas/ui_problem_examples.json';
import { POST as bffLoad } from '../routes/t/[tenantKey]/ui/load/+server';
import { POST as exampleLoad } from '../../../../../../examples/ui-app/src/routes/view/load/+server';
import { mcpExecutor } from 'wyrd-ai-ui/mcp';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { Bifrost, WyrdError, type WyrdClient } from 'wyrd-ai';
import catalog from 'wyrd-ai-ui/catalog.json';
import { place as placeAll, rowsHeight } from '../../packages/ui/src/lib/layout';
import { CARD_TABS, PROMPT_TABS, readInventory, writeDetail, writeInventory } from '$lib/cards-host';
import { load as loadCardDetail } from '../routes/t/[tenantKey]/cards/detail/+page.server';
import {
  AGENT,
  agent,
  artifacts,
  AUTHORED_SQL,
  authored,
  inventory,
  NATIVE,
  notFound,
  PROMPT,
  prompts,
  SERVICE,
  bareSpan,
  buckets,
  card,
  costs,
  denied,
  facet,
  fixtureExecutor,
  LOG,
  logRow,
  logs,
  observeAnswer,
  RANGE,
  registered,
  series,
  spanRow,
  spans,
  stateExecutors,
  summary,
  terminal,
  timeseries,
  total,
  trace,
  TRACE,
  traceCounts,
  traces
} from '$lib/ui-fixtures';
import { reactive } from '$lib/ui-fixtures.svelte';

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
  expect(Object.keys(catalog.components)).toEqual(expect.arrayContaining(['Chart', 'CardHeader']));
  expect(catalog.components.Chart.emits.map((action) => action.name)).toEqual(['chart.range', 'chart.open_in_query']);
  expect(catalog.view.required).toEqual(['version', 'catalogId', 'range', 'panels']);

  // Timeseries: one line per series value, each with a distinct dash and marker in the legend.
  const plot = render(Chart, { input: timeseries, result: { state: 'populated', data: series } });
  expect(plot.getByRole('heading', { name: 'Latency · ms' })).toBeInTheDocument();
  const legend = plot.container.querySelectorAll('.legend tbody th');
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
    const labels = [...many.container.querySelectorAll('.legend tbody th')].map((item) => item.textContent?.trim());
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
  const captions = (view: { container: Element }) => [...view.container.querySelectorAll('.legend tbody th')].map((item) => item.textContent?.trim());
  const readings = (view: { container: Element }) => [...view.container.querySelectorAll('.tooltip span')].map((item) => item.textContent);
  const taken: [string, number][] = [['other', 21], ...Array.from({ length: 20 }, (_, index): [string, number] => [`s${index + 1}`, index + 1])];
  const named = render(Chart, { input: timeseries, result: { state: 'populated', data: grouped(taken) } });
  expect(captions(named)).toHaveLength(20);
  expect(new Set(captions(named)).size).toBe(20);
  expect(captions(named).slice(0, 2)).toEqual(['other', 's20']);
  expect(captions(named).at(-1)).toBe('other (aggregate)');
  expect(named.container.querySelectorAll('svg.plot polyline')).toHaveLength(20);
  expect(named.getByRole('application').getAttribute('aria-label')).toContain('other, s20, s19');
  await fireEvent.focus(named.getByRole('application'));
  expect(named.getByRole('status')).toHaveTextContent(/other: 21 ms.*other \(aggregate\): 3 ms/);
  // A smaller authored "other" folds into the aggregate, which then needs no qualifier; at five series it keeps its own line.
  await named.rerender({ result: { state: 'populated', data: grouped([...taken.slice(1), ['x', 30], ['y', 40], ['other', 0.5]]) } });
  expect(captions(named)).toHaveLength(20);
  expect(captions(named).slice(0, 2)).toEqual(['y', 'x']);
  expect(captions(named).at(-1)).toBe('other');
  expect(named.getByRole('status')).toHaveTextContent('other: 6.5 ms');
  await named.rerender({ result: { state: 'populated', data: grouped([['other', 21], ['s1', 1], ['s2', 2]]) } });
  expect(captions(named)).toEqual(['other', 's1', 's2']);
  named.unmount();

  // Distinct (series, y) groups never merge; captions that would collide name their typed group instead.
  const instant = '2026-10-20T10:00:00Z';
  const tokens: QueryResult = {
    columns: [{ name: 'at', data_type: 'Timestamp(Microsecond, None)', nullable: false }, { name: 'model', data_type: 'Utf8', nullable: true }, { name: 'tokens', data_type: 'Int64', nullable: true }, { name: 'input tokens', data_type: 'Int64', nullable: true }],
    rows: [[instant, 'model input', 1, 10], [instant, 'model', 2, 20]],
    terminal: terminal(2)
  };
  const pairs = render(Chart, { input: { ...timeseries, y: ['tokens', 'input tokens'] } as ChartInput, result: { state: 'populated', data: tokens } });
  expect(captions(pairs)).toEqual(['"model input","tokens"', 'model input input tokens', 'model tokens', '"model","input tokens"']);
  expect(pairs.container.querySelectorAll('svg.plot polyline')).toHaveLength(4);
  expect(pairs.getByRole('application').getAttribute('aria-label')).toContain('"model input","tokens", model input input tokens, model tokens, "model","input tokens"');
  await fireEvent.focus(pairs.getByRole('application'));
  expect(readings(pairs)).toEqual(['"model input","tokens": 1 ms', 'model input input tokens: 10 ms', 'model tokens: 2 ms', '"model","input tokens": 20 ms']);
  pairs.unmount();
  // Null and the text "null" stay apart.
  const nulls = (groups: [string | null, number][]): QueryResult => ({ ...tokens, rows: groups.map(([name, value]) => [instant, name, value, null]), terminal: terminal(groups.length) });
  const nullable = render(Chart, { input: { ...timeseries, y: ['tokens'] } as ChartInput, result: { state: 'populated', data: nulls([[null, 1], ['null', 2]]) } });
  expect(captions(nullable)).toEqual(['null', '"null"']);
  await fireEvent.focus(nullable.getByRole('application'));
  expect(readings(nullable)).toEqual(['null: 1 ms', '"null": 2 ms']);
  // A rerender with other groups shows only the current captions and values.
  await nullable.rerender({ result: { state: 'populated', data: nulls([[null, 5], ['x', 6]]) } });
  expect(captions(nullable)).toEqual(['null', 'x']);
  expect(readings(nullable)).toEqual(['null: 5 ms', 'x: 6 ms']);
  nullable.unmount();
  // Brushing a chart whose values are all null emits nothing and does not throw.
  const onaction = vi.fn();
  const empty = render(Chart, { input: { ...timeseries, y: ['input tokens'] } as ChartInput, result: { state: 'populated', data: nulls([['x', 1]]) }, onaction });
  const blank = empty.container.querySelector('svg.plot') as SVGSVGElement;
  blank.getBoundingClientRect = () => ({ left: 0, width: 600, top: 0, height: 220, right: 600, bottom: 220, x: 0, y: 0, toJSON: () => ({}) });
  await fireEvent.pointerDown(blank, { clientX: 100, pointerId: 1 });
  await fireEvent.pointerMove(blank, { clientX: 400, pointerId: 1 });
  await fireEvent.pointerUp(blank, { clientX: 400, pointerId: 1 });
  expect(onaction).not.toHaveBeenCalled();
  empty.unmount();

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
  expect(header.container).toHaveTextContent('Model/ranker@1.4.2');
  expect(header.container).toHaveTextContent('✓active');
  expect([...header.container.querySelectorAll('option')].map((option) => option.value)).toEqual(['1.4.2', '1.4.1']);
  expect(header.container).toHaveTextContent('showing the newest 2 versions');
  expect(header.getByRole('button', { name: 'Runs' })).toHaveAttribute('aria-current', 'page');
  await fireEvent.click(header.getByRole('button', { name: 'Copy uid' }));
  expect(writeText).toHaveBeenCalledWith('card_01HX');
  expect(await header.findByText('Copied')).toBeInTheDocument();
  // Copy ref writes the canonical reference, so two kinds sharing name@version stay distinct.
  await fireEvent.click(header.getByRole('button', { name: 'Copy ref' }));
  expect(writeText).toHaveBeenLastCalledWith('Model/ranker@1.4.2');
  writeText.mockRejectedValueOnce(new Error('denied'));
  await fireEvent.click(header.getByRole('button', { name: 'Copy ref' }));
  expect(await header.findByText('Copy failed')).toBeInTheDocument();
  header.unmount();
  const dataHeader = render(CardHeader, { input: { cardRef: { ...card, kind: 'Data' } }, result: { state: 'populated', data: { card: { ...registered, kind: 'Data' } as never, versions: { state: 'populated', data: { items: [summary('1.4.2')], truncated: false } } } } });
  await fireEvent.click(dataHeader.getByRole('button', { name: 'Copy ref' }));
  expect(writeText).toHaveBeenLastCalledWith('Data/ranker@1.4.2');
  dataHeader.unmount();
  expect(fmtCardRef({ ...card, uid: 'card_01HX' })).toBe('Model/ranker@1.4.2#card_01HX');
  expect(fmtCardRef({ ...card, uid: null })).toBe('Model/ranker@1.4.2');
  expect(fmtCardRef(card)).toBe('Model/ranker@1.4.2');
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
  for (const kind of ['Model', 'Data'] as const) {
    const chip = render(CardRefChip, { cardRef: { ...card, kind }, href: '/cards/x' });
    expect(chip.getByRole('link')).toHaveTextContent(`${kind}/ranker@1.4.2`);
    expect(chip.getByRole('link')).toHaveAttribute('title', `${kind}/ranker@1.4.2`);
    chip.unmount();
  }
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
  // A span too long to resolve to a dated range is refused at validation, before any read.
  expect(validateView({ version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: { last: '100000d' }, panels: [] })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect(validateView({ version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: { last: '99999d' }, panels: [{ id: 'a', component: 'Chart', input: timeseries }] })).not.toHaveProperty('code');
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
  // The host's reference carries this version's uid; a different version never inherits it.
  const pinned = { ...card, uid: 'card_01HX' };
  const controls = render(CardHeader, { input: { cardRef: pinned, tabs }, result: listed, onaction: headerAction, actions: { 'card.version': { state: 'pending' }, 'card.tab': { state: 'pending' } } });
  const select = controls.getByRole('combobox', { name: 'Version' }) as HTMLSelectElement;
  expect(select).toBeDisabled();
  expect(controls.getByRole('button', { name: 'Runs' })).toBeDisabled();
  for (const [state, text] of [
    [{ state: 'denied', problem: denied }, 'Forbidden (WYRD_AUTH_403_FORBIDDEN)'],
    [{ state: 'error', problem: { code: 'WYRD_SPEC_500_INTERNAL', title: 'Internal error' } }, 'Internal error (WYRD_SPEC_500_INTERNAL)'],
    [{ state: 'unavailable', reason: 'No navigation here' }, 'No navigation here'],
    [{ state: 'success', message: 'Opened 1.4.1' }, 'Opened 1.4.1']
  ] as const) {
    await controls.rerender({ input: { cardRef: pinned, tabs }, result: listed, onaction: headerAction, actions: { 'card.version': state, 'card.tab': state } });
    expect(controls.getAllByText(text)).toHaveLength(2);
    expect(select.disabled).toBe(state.state === 'unavailable');
  }
  // A refused or pending change leaves the selection on the version the host passed.
  await fireEvent.change(select, { target: { value: '1.4.1' } });
  expect(headerAction).toHaveBeenCalledExactlyOnceWith({ name: 'card.version', input: { card_ref: { ...card, version: '1.4.1' } } });
  expect(headerAction.mock.calls[0][0].input.card_ref).not.toHaveProperty('uid');
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

test('Node executor reconnects after a failed connect', async () => {
  const query = [{ op: 'bifrost.list_tables' }] as const;
  const connect = vi
    .spyOn(Bifrost, 'connect')
    .mockRejectedValueOnce(new WyrdError('WYRD_UPSTREAM_503' as never, 503, 'Unavailable', 'down'))
    .mockResolvedValue({ listTables: async () => [] } as never);
  try {
    const executor = nodeExecutor({} as WyrdClient);
    expect((await executor.run(query))[0]).toMatchObject({ ok: false });
    expect((await executor.run(query))[0]).toEqual({ ok: true, value: { tables: [] } });
    await executor.run(query);
    expect(connect).toHaveBeenCalledTimes(2);
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
  const browser = import.meta.glob('/packages/ui/src/lib/{index,contract,loaders,observe,view,plot,format,schemas,layout,mcp}.ts', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;
  expect(Object.keys(browser)).toHaveLength(10);
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

  // A direct-Svelte host writing its own reactive document replaces local edits with its newer authored values.
  const live = reactive(laidOut({ a: { x: 0, y: 0, w: 8, h: 6 }, b: { x: 8, y: 0, w: 4, h: 6 } }));
  const heard = vi.fn();
  const reads = recording(() => ({ ok: true, value: series }));
  const host = render(WyrdView, { doc: live, executor: reads.executor, onlayout: heard });
  await waitFor(() => expect(reads.calls).toHaveLength(2));
  const finish = async (id: string) => {
    const grip = host.getByRole('button', { name: `Move or resize ${id}` });
    grip.focus();
    await fireEvent.keyDown(grip, { key: 'ArrowDown' });
    await fireEvent.keyDown(grip, { key: 'Enter' });
  };
  const rows = () => [panel(host.container, 'a').style.gridRow, panel(host.container, 'b').style.gridRow];
  await finish('a');
  expect(rows()).toEqual(['2 / span 6', '1 / span 6']);
  live.panels[0].layout!.y = 7;
  await tick();
  expect(rows()).toEqual(['8 / span 6', '1 / span 6']);
  await finish('b');
  expect(heard.mock.lastCall![0].panels.map((each: { layout?: unknown }) => each.layout)).toEqual([{ x: 0, y: 7, w: 8, h: 6 }, { x: 8, y: 1, w: 4, h: 6 }]);
  // The layout-only write loads nothing, and editing never writes the host's document.
  expect(reads.calls).toHaveLength(2);
  const hostValues = laidOut({ a: { x: 0, y: 7, w: 8, h: 6 }, b: { x: 8, y: 0, w: 4, h: 6 } });
  expect(JSON.parse(JSON.stringify(live))).toEqual(hostValues);
  // Re-rendering the unchanged document keeps the completed edit; an equal-content new document drops it.
  await host.rerender({ onlayout: undefined });
  await host.rerender({ onlayout: heard });
  expect(rows()).toEqual(['8 / span 6', '2 / span 6']);
  await host.rerender({ doc: structuredClone(hostValues) });
  expect(rows()).toEqual(['8 / span 6', '1 / span 6']);
  host.unmount();
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

  // A host writing a new range or panel input into its reactive document mid-gesture ends the gesture unreported.
  const live = reactive(laidOut({ a: { x: 0, y: 0, w: 8, h: 6 } }));
  const told = vi.fn();
  const host = render(WyrdView, { doc: live, executor: chart.executor, onlayout: told });
  const hostBox = () => [panel(host.container, 'a').style.gridColumn, panel(host.container, 'a').style.gridRow];
  const authored = hostBox();
  const writes = [() => (live.range = { ...RANGE, to: '2026-10-20T06:00:00Z' }), () => (live.panels[0].input.title = 'renamed')];
  for (const [index, write] of writes.entries()) {
    const handle = index === 0 ? host.getByRole('button', { name: 'Move or resize a' }) : panel(host.container, 'a').querySelector('.corner')!;
    await fireEvent.pointerDown(handle, { button: 0, clientX: 0, clientY: 0, pointerId: 50 + index });
    await fireEvent.pointerMove(handle, { clientX: 2, clientY: 104, pointerId: 50 + index });
    expect(hostBox()).not.toEqual(authored);
    write();
    await tick();
    expect(hostBox()).toEqual(authored);
    await fireEvent.pointerMove(handle, { clientX: 3, clientY: 156, pointerId: 50 + index });
    await fireEvent.pointerUp(handle, { clientX: 3, clientY: 156, pointerId: 50 + index });
    expect(hostBox()).toEqual(authored);
    expect(told).not.toHaveBeenCalled();
  }
  // A fresh gesture still completes once, carrying the host's newer values.
  const fresh = panel(host.container, 'a').querySelector('.corner')!;
  await fireEvent.pointerDown(fresh, { button: 0, clientX: 0, clientY: 0, pointerId: 60 });
  await fireEvent.pointerMove(fresh, { clientX: 0, clientY: 52, pointerId: 60 });
  await fireEvent.pointerUp(fresh, { clientX: 0, clientY: 52, pointerId: 60 });
  expect(told).toHaveBeenCalledOnce();
  expect(told.mock.calls[0][0]).toMatchObject({ range: { to: '2026-10-20T06:00:00Z' }, panels: [{ input: { title: 'renamed' }, layout: { x: 0, y: 0, w: 8, h: 7 } }] });
  host.unmount();
});

test('TASK-027: Narrow views', async () => {
  const width = vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(500);
  try {
    const chart = recording(() => ({ ok: true, value: series }));
    const doc = laidOut({ a: { x: 6, y: 0, w: 6, h: 4 }, b: { x: 0, y: 0, w: 6, h: 4 }, c: { x: 0, y: 4, w: 12, h: 2 } });
    const view = render(WyrdView, { doc, executor: chart.executor, onlayout: vi.fn() });
    const ids = (container: Element) => [...container.querySelectorAll('[data-panel]')].map((each) => each.getAttribute('data-panel'));
    expect(ids(view.container)).toEqual(['b', 'a', 'c']);
    expect(panel(view.container, 'a').style.gridColumn).toBe('1 / -1');
    expect(panel(view.container, 'a').style.height).not.toBe('');
    expect(view.queryByRole('button', { name: /Move or resize/ })).toBeNull();
    view.unmount();

    // A host passing a document with other panels while narrow shows only those, in reading order.
    const pending: ((result: WireResult) => void)[] = [];
    const held = recording((request) =>
      (request as { sql?: string }).sql === 'SELECT failing' ? { ok: false, problem: denied } : new Promise<WireResult>((resolve) => pending.push(resolve))
    );
    const onlayout = vi.fn();
    const host = render(WyrdView, { doc: laidOut({ a: { x: 0, y: 4, w: 6, h: 4 }, b: { x: 0, y: 0, w: 6, h: 4 } }), executor: held.executor, onlayout });
    await waitFor(() => expect(held.calls).toHaveLength(2));
    const replaced = laidOut({ c: { x: 6, y: 0, w: 6, h: 4 }, d: { x: 0, y: 0, w: 6, h: 3 } });
    replaced.panels[0].input = { ...replaced.panels[0].input, sql: 'SELECT failing' };
    await host.rerender({ doc: replaced, executor: held.executor, onlayout });
    await waitFor(() => expect(held.calls).toHaveLength(4));
    expect(ids(host.container)).toEqual(['d', 'c']);
    expect(held.calls[0].signal?.aborted).toBe(true);
    expect(held.calls[1].signal?.aborted).toBe(true);
    expect(panel(host.container, 'd').style.gridColumn).toBe('1 / -1');
    expect(panel(host.container, 'd').style.height).toBe(rowsHeight(3));
    expect(host.queryByRole('button', { name: /Move or resize/ })).toBeNull();
    // The current panels keep their loading and failed states; a late outgoing result never returns.
    expect(panel(host.container, 'd')).toHaveTextContent('Running query');
    await waitFor(() => expect(panel(host.container, 'c')).toHaveTextContent('Forbidden'));
    pending[0]({ ok: true, value: series });
    await tick();
    expect(host.container.querySelector('[data-panel="a"]')).toBeNull();
    expect(panel(host.container, 'd')).toHaveTextContent('Running query');

    // Fewer panels drop the removed ones; invalid input then valid input recovers.
    await host.rerender({ doc: laidOut({ d: { x: 0, y: 0, w: 6, h: 3 } }), executor: held.executor, onlayout });
    expect(ids(host.container)).toEqual(['d']);
    await host.rerender({ doc: { version: 'wyrd.view/v1' }, executor: held.executor, onlayout });
    expect(host.container.querySelector('[data-panel]')).toBeNull();
    await host.rerender({ doc: laidOut({ e: { x: 6, y: 0, w: 6, h: 2 }, f: { x: 0, y: 0, w: 6, h: 2 } }), executor: held.executor, onlayout });
    expect(ids(host.container)).toEqual(['f', 'e']);

    // Same panels with changed input reload; a layout-only change reorders without reading.
    const reads = held.calls.length;
    const renamed = laidOut({ e: { x: 6, y: 0, w: 6, h: 2 }, f: { x: 0, y: 0, w: 6, h: 2 } });
    renamed.panels[0].input = { ...renamed.panels[0].input, title: 'renamed' };
    await host.rerender({ doc: renamed, executor: held.executor, onlayout });
    await waitFor(() => expect(held.calls).toHaveLength(reads + 2));
    renamed.panels[0].layout = { x: 0, y: 0, w: 6, h: 2 };
    renamed.panels[1].layout = { x: 0, y: 2, w: 6, h: 2 };
    await host.rerender({ doc: structuredClone(renamed), executor: held.executor, onlayout });
    expect(ids(host.container)).toEqual(['e', 'f']);
    await tick();
    expect(held.calls).toHaveLength(reads + 2);
    host.unmount();
  } finally {
    width.mockRestore();
  }
});

test('TASK-004: Canonical signal interactions', async () => {
  // The catalog generates every Observe component with its states and typed actions.
  expect(Object.keys(catalog.components)).toEqual([
    'Chart',
    'CardHeader',
    'CardInventory',
    'CardFacts',
    'RelationshipList',
    'ArtifactTable',
    'PromptMessages',
    'PromptVariables',
    'UsedBy',
    'VersionDiff',
    'StatStrip',
    'FacetRail',
    'RecordTable',
    'TraceWaterfall',
    'RecordDetail'
  ]);
  expect(catalog.components.RecordTable.states).toContain('live');
  expect(catalog.components.RecordDetail.emits.map((action) => action.name)).toEqual(['record.filter', 'record.logs', 'record.open']);

  // Inputs only name allowlisted columns; payload columns are never listable or filterable.
  expect(validatePanel('RecordTable', { signal: 'traces', columns: ['attributes'] })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect(validatePanel('RecordTable', { signal: 'logs', where: [{ column: 'body', value: 'x' }] })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect(validatePanel('FacetRail', { signal: 'metrics', facets: ['events'] })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect(validatePanel('TraceWaterfall', { traceId: "x' OR 1=1" })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect(validatePanel('StatStrip', { stats: ['cost'], where: [{ column: 'name', value: 'x' }] })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });

  // Traces: root spans of traces with any span matching the bound WHERE, then counts for exactly the visible traces,
  // over the time their roots cover widened by an hour each side, so only those partitions open.
  const where = [{ column: 'service_name', value: 'agent-api' }, { column: 'http_route', value: null, not: true }];
  const listed = recording((request) => ({ ok: true, value: observeAnswer((request as { sql: string }).sql)! }));
  const table = await loadRecordTable({ signal: 'traces', where }, { executor: listed.executor, range: RANGE });
  expect(listed.calls.map((call) => call.request)).toEqual([
    {
      op: 'bifrost.query',
      sql:
        "SELECT start_time_unix_nano, name, service_name, duration_nano, status_code, encode(trace_id, 'hex') AS trace_id, encode(span_id, 'hex') AS span_id " +
        'FROM bifrost.traces.spans WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz AND parent_span_id IS NULL ' +
        'AND trace_id IN (SELECT trace_id FROM bifrost.traces.spans WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz ' +
        "AND service_name = $3 AND http_route IS NOT NULL) ORDER BY start_time_unix_nano DESC, encode(trace_id, 'hex') ASC, encode(span_id, 'hex') ASC LIMIT 51",
      params: [RANGE.from, RANGE.to, 'agent-api']
    },
    {
      op: 'bifrost.query',
      sql:
        "SELECT encode(trace_id, 'hex') AS trace_id, count(*) AS spans, count(*) FILTER (WHERE status_code = 2) AS errors FROM bifrost.traces.spans " +
        'WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz ' +
        "AND trace_id IN (decode($3, 'hex'), decode($4, 'hex'), decode($5, 'hex')) GROUP BY trace_id",
      params: ['2026-10-20T10:56:12.500Z', '2026-10-20T12:58:03.960Z', ...traces.rows.map((row) => row[5])]
    }
  ]);
  expect(table.result).toMatchObject({ state: 'populated', data: { counts: { state: 'populated' } } });
  // A refused list issues no dependent read; a failed count read keeps the list.
  const refused = recording(() => ({ ok: false, problem: denied }));
  expect((await loadRecordTable({ signal: 'traces' }, { executor: refused.executor, range: RANGE })).result.state).toBe('denied');
  expect(refused.calls).toHaveLength(1);
  const countless = recording((request) => ((request as { sql: string }).sql.includes('AS spans') ? { ok: false, problem: denied } : { ok: true, value: traces }));
  expect((await loadRecordTable({ signal: 'traces' }, { executor: countless.executor, range: RANGE })).result).toMatchObject({ state: 'populated', data: { counts: { state: 'denied' } } });
  // A signal read needs a range; authored SQL runs unchanged with only its referenced binds.
  expect((await loadRecordTable({ signal: 'spans' }, { executor: refused.executor })).result).toMatchObject({ state: 'error', problem: { code: 'WYRD_SPEC_400_VALIDATION' } });
  const authored = recording(() => ({ ok: true, value: traces }));
  await loadRecordTable({ sql: 'SELECT * FROM t WHERE at >= $1::timestamptz' }, { executor: authored.executor, range: RANGE });
  expect(authored.calls[0].request).toEqual({ op: 'bifrost.query', sql: 'SELECT * FROM t WHERE at >= $1::timestamptz', params: [RANGE.from], max_rows: 500 });

  // An exact trace ignores the range and refuses rather than truncates an oversize trace.
  const exact = recording(() => ({ ok: true, value: trace }));
  await loadTraceWaterfall({ traceId: TRACE, range: RANGE }, { executor: exact.executor, range: RANGE });
  expect(exact.calls[0].request).toMatchObject({ params: [TRACE], max_rows: 10_000 });
  expect((exact.calls[0].request as { sql: string }).sql).not.toContain('$2');
  // Log detail binds its full key; facets bind the list's WHERE and read one value past the shown ten.
  const detail = recording(() => ({ ok: true, value: logRow }));
  await loadRecordDetail({ log: LOG }, { executor: detail.executor });
  expect(detail.calls[0].request).toMatchObject({ params: [LOG.time_unix_nano, LOG.observed_time_unix_nano, LOG.wyrd_request_id] });
  expect(facetQuery({ signal: 'logs', facets: ['severity_text'], where: [{ column: 'service_name', value: 'search' }] }, 'severity_text', RANGE)).toEqual({
    sql:
      'SELECT severity_text AS value, count(*) AS n, max(severity_number) AS severity_number FROM bifrost.logs.records WHERE time_unix_nano >= $1::timestamptz AND time_unix_nano < $2::timestamptz ' +
      'AND service_name = $3 GROUP BY severity_text ORDER BY n DESC, value LIMIT 11',
    params: [RANGE.from, RANGE.to, 'search']
  });
  // Cost is summed per recorded currency, scoped by a bound Card uid.
  expect(statQueries('cost', { stats: ['cost'], scope: { column: 'workload_card_uid', uid: 'card_01HX' } }, RANGE, '5 minutes')[0]).toEqual({
    sql:
      'SELECT sum(CAST(cost AS DECIMAL(38, 12))) AS value, currency FROM bifrost.gateway.calls WHERE started_at >= $1::timestamptz AND started_at < $2::timestamptz ' +
      'AND cost IS NOT NULL AND workload_card_uid = $3 GROUP BY currency ORDER BY currency',
    params: [RANGE.from, RANGE.to, 'card_01HX']
  });

  // Tokens are summed from gateway usage entries whose unit is tokens.
  expect(statQueries('tokens', { stats: ['tokens'] }, RANGE, '5 minutes')[0]).toEqual({
    sql:
      "SELECT sum(CAST(u['quantity'] AS DECIMAL(38, 12))) AS value FROM (SELECT started_at, workload_card_uid, subject_card_uid, unnest(usage) AS u FROM bifrost.gateway.calls) AS usage " +
      "WHERE started_at >= $1::timestamptz AND started_at < $2::timestamptz AND u['unit'] = 'tokens'",
    params: [RANGE.from, RANGE.to]
  });
  // Metric structure columns are listable beyond the defaults.
  expect(validatePanel('RecordTable', { signal: 'metrics', columns: ['metric_name', 'bucket_counts', 'explicit_bounds'] })).toMatchObject({ component: 'RecordTable' });

  // Nanosecond offsets stay exact; an unretained parent or a parent cycle still places every span.
  expect(parseNanos('2026-10-20T11:58:02.450000001+00:00')! - parseNanos('2026-10-20T11:58:02.45Z')!).toBe(1n);
  expect(spanTree(trace).map((span) => [span.name, span.depth])).toEqual([
    ['invoke_agent support', 0],
    ['chat gpt-5', 1],
    ['execute_tool search', 1],
    ['GET /search', 2],
    ['late span', 0]
  ]);
  const cyclic = { ...trace, rows: [['aa', 'bb', 'a', null, '2026-10-20T00:00:00Z', 1, 0], ['bb', 'aa', 'b', null, '2026-10-20T00:00:00Z', 1, 0]] };
  expect(spanTree(cyclic)).toHaveLength(2);

  // Every component renders each non-data state with its state word.
  const empty = { state: 'empty', reason: 'Nothing here.' } as const;
  const shells = [
    [StatStrip, { stats: ['traces'] }, { stats: [] }],
    [FacetRail, { signal: 'traces', facets: ['service_name'] }, { facets: [] }],
    [RecordTable, { signal: 'traces' }, { rows: { ...traces, rows: [] } }],
    [TraceWaterfall, { traceId: TRACE }, { ...trace, rows: [] }],
    [RecordDetail, { traceId: TRACE, spanId: 'b9c7c989f97918e1' }, { ...spanRow, rows: [] }]
  ] as const;
  for (const [component, input, data] of shells) {
    for (const [result, word] of [
      [{ state: 'loading' }, 'Loading'],
      [{ ...empty, data }, 'Nothing here.'],
      [{ state: 'denied', problem: denied }, 'Not authorized'],
      [{ state: 'error', problem: { code: 'WYRD_SPEC_500_INTERNAL', title: 'Internal error' } }, 'Error']
    ] as const) {
      const shell = render(component as never, { input, result } as never);
      expect(shell.container, `${component.name} ${result.state}`).toHaveTextContent(word);
      shell.unmount();
    }
  }

  // StatStrip: nothing recorded is "—", never 0; currencies never add up.
  const strip = render(StatStrip, {
    input: { stats: ['tokens', 'cost'] },
    result: { state: 'populated', data: { stats: [{ stat: 'tokens', total: total(null), series: buckets }, { stat: 'cost', total: costs, series: buckets }] } }
  });
  expect(strip.container.querySelectorAll('.value')[0]).toHaveTextContent('—');
  expect([...strip.container.querySelectorAll('.value')].slice(1).map((value) => value.textContent)).toEqual(['1.25 EUR', '0.42 USD']);
  strip.unmount();

  // FacetRail: a null value is "not set" and filters by null; values in the WHERE are pressed.
  const facetAction = vi.fn();
  const rail = render(FacetRail, {
    input: { signal: 'traces', facets: ['service_name'], where: [{ column: 'service_name', value: 'agent-api' }] },
    result: { state: 'populated', data: { facets: [{ column: 'service_name', values: facet }] } },
    onaction: facetAction
  });
  expect(rail.getByRole('button', { name: /agent-api/ })).toHaveAttribute('aria-pressed', 'true');
  await fireEvent.click(rail.getByRole('button', { name: /not set/ }));
  expect(facetAction).toHaveBeenCalledWith({ name: 'facet.filter', input: { signal: 'traces', column: 'service_name', value: null } });
  expect(rail.getByText('Click a value to add it to WHERE')).toBeInTheDocument();
  rail.unmount();
  // A facet shows five values; "+ N more" counts the hidden values read, with "+" when the read found more than it returns.
  const names = { ...facet, rows: Array.from({ length: 11 }, (_, index) => [`span ${index + 1}`, 200 - index]) };
  const longRail = render(FacetRail, {
    input: { signal: 'spans', facets: ['name', 'status_code'] },
    result: { state: 'populated', data: { facets: [{ column: 'name', values: names }, { column: 'status_code', values: { ...facet, rows: [[1, 9], [2, 3]] } }] } },
    onaction: facetAction
  });
  expect(longRail.queryByText('span 6')).toBeNull();
  await fireEvent.click(longRail.getByRole('button', { name: '+ 5+ more' }));
  expect(longRail.getByText('span 10')).toBeInTheDocument();
  expect(longRail.queryByText('span 11')).toBeNull();
  expect(longRail.getByText('More values exist; filter to narrow them.')).toBeInTheDocument();
  expect(longRail.getByText('error')).toHaveClass('error');
  expect(longRail.getByText('ok')).not.toHaveClass('error');
  longRail.unmount();

  // RecordTable: typed cells, counts per trace, row open; a held live list offers new rows instead of moving.
  const openAction = vi.fn();
  const populated = { state: 'populated', data: { rows: traces, counts: { state: 'populated', data: traceCounts } } } as const;
  const list = render(RecordTable, { input: { signal: 'traces', live: true }, result: populated, onaction: openAction });
  // Headings read as words and keep the column name, which WHERE takes, as their title.
  expect(list.getByRole('columnheader', { name: 'Start' })).toHaveAttribute('title', 'start_time_unix_nano');
  const firstRow = list.getAllByRole('row')[1];
  expect(firstRow.querySelector('time')).toHaveAttribute('datetime', traces.rows[0][0]);
  expect(firstRow).toHaveTextContent('2.84s');
  expect(firstRow).toHaveTextContent('error');
  expect(list.getAllByRole('row')[3]).toHaveTextContent('∅');
  await fireEvent.click(firstRow);
  expect(openAction).toHaveBeenCalledWith({ name: 'record.open', input: { trace_id: TRACE, span_id: '00f067aa0ba902b7' } });
  await fireEvent.mouseEnter(list.getByRole('region'));
  const newer = { ...traces, rows: [['2026-10-20T11:59:00.000000000+00:00', 'new root', 'agent-api', 1_000_000, 0, 'ffffffffffffffffffffffffffffffff', '1111111111111111'], ...traces.rows] };
  await list.rerender({ input: { signal: 'traces', live: true }, result: { state: 'populated', data: { rows: newer } }, onaction: openAction });
  expect(list.queryByText('new root')).toBeNull();
  await fireEvent.click(list.getByRole('button', { name: /↑ 1 new traces/ }));
  expect(list.getByText('new root')).toBeInTheDocument();
  // Opening a record holds the list, counts included, until the reader resumes.
  await fireEvent.mouseLeave(list.getByRole('region'));
  await list.rerender({ input: { signal: 'traces', live: true }, result: populated, onaction: openAction });
  await fireEvent.click(list.getAllByRole('row')[1]);
  expect(list.getByRole('status')).toHaveTextContent('Live paused while a record is open');
  await list.rerender({ input: { signal: 'traces', live: true }, result: { state: 'populated', data: { rows: newer } }, onaction: openAction });
  expect(list.queryByText('new root')).toBeNull();
  expect(list.getAllByRole('row')[1]).not.toHaveTextContent('…');
  await fireEvent.click(list.getByRole('button', { name: /^Live paused while a record is open · ↑ 1 new traces$/ }));
  expect(list.getByRole('status')).toHaveTextContent(/^Live$/);
  expect(list.getByText('new root')).toBeInTheDocument();
  list.unmount();
  // A held list given a new column projection shows the read that carries it: headings, cells, CSV and Query describe one read.
  const read = (columns: readonly string[], sql: string) => ({
    input: { signal: 'logs', live: true, columns },
    executed: { sql, params: [] },
    result: {
      state: 'populated',
      data: {
        rows: {
          columns: [...columns, 'trace_id', 'span_id', 'observed_time_unix_nano', 'wyrd_request_id'].map((name) => ({ name, data_type: 'Utf8', nullable: true })),
          rows: [[...columns.map((column) => `${column} value`), TRACE, '00f067aa0ba902b7', '2026-10-20T12:00:00Z', 'r1']],
          terminal: { outcome: 'success', row_count: 1, warnings: [] }
        }
      }
    },
    onaction: openAction
  });
  const projected = render(RecordTable, read(['body_text'], 'first') as never);
  await fireEvent.mouseEnter(projected.getByRole('region'));
  await projected.rerender({ ...read(['body_text', 'severity_text'], 'second'), result: { state: 'loading' } } as never);
  expect(projected.queryByRole('columnheader', { name: /severity/i })).toBeNull();
  await projected.rerender(read(['body_text', 'severity_text'], 'second') as never);
  expect(projected.getByRole('status')).toHaveTextContent('Live paused while you read');
  expect(projected.getByRole('columnheader', { name: /severity/i })).toBeInTheDocument();
  expect(projected.getByText('severity_text value')).toBeInTheDocument();
  expect(decodeURIComponent(projected.getByRole('link', { name: 'CSV' }).getAttribute('href') ?? '')).toContain('body_text value,severity_text value');
  await fireEvent.click(projected.getByRole('button', { name: /Open in Query/ }));
  expect(openAction).toHaveBeenLastCalledWith({ name: 'record.open_in_query', input: { sql: 'second', params: [] } });
  projected.unmount();
  // The footer states the returned extent, never an unread total; a service is a chip.
  const footed = render(RecordTable, { input: { signal: 'traces' }, result: populated });
  expect(footed.getByText(/^Showing 3\s+traces$/)).toBeInTheDocument();
  expect(footed.getAllByText('agent-api')[0]).toHaveClass('chip');
  footed.unmount();
  // A sortable heading asks for the list in its order, the other way when it is the sort; Newer and Older ask for the pages beside it.
  const pageAction = vi.fn();
  const pagedResult = { state: 'populated', data: { ...populated.data, page: { older: ['t2', 'a', 'b'] } } } as const;
  const pagedList = render(RecordTable, { input: { signal: 'traces' }, result: pagedResult, onaction: pageAction });
  expect(pagedList.getByRole('columnheader', { name: /Start/ })).toHaveAttribute('aria-sort', 'descending');
  expect(within(pagedList.getAllByRole('columnheader').find((heading) => heading.textContent === 'spans')!).queryByRole('button')).toBeNull();
  await fireEvent.click(pagedList.getByRole('button', { name: /Start/ }));
  expect(pageAction).toHaveBeenLastCalledWith({ name: 'record.sort', input: { column: 'start_time_unix_nano', desc: false } });
  await fireEvent.click(pagedList.getByRole('button', { name: 'Service' }));
  expect(pageAction).toHaveBeenLastCalledWith({ name: 'record.sort', input: { column: 'service_name', desc: false } });
  await fireEvent.click(pagedList.getByRole('button', { name: 'Duration' }));
  expect(pageAction).toHaveBeenLastCalledWith({ name: 'record.sort', input: { column: 'duration_nano', desc: true } });
  expect(pagedList.getByRole('button', { name: '‹ Newer' })).toBeDisabled();
  await fireEvent.click(pagedList.getByRole('button', { name: 'Older ›' }));
  expect(pageAction).toHaveBeenLastCalledWith({ name: 'record.page', input: { after: ['t2', 'a', 'b'] } });
  pagedList.unmount();
  // A log's severity sorts by its number, so levels order by rank.
  const bySeverity = render(RecordTable, { input: { signal: 'logs' }, result: { state: 'populated', data: { rows: logs } }, onaction: pageAction });
  await fireEvent.click(bySeverity.getByRole('button', { name: 'Severity' }));
  expect(pageAction).toHaveBeenLastCalledWith({ name: 'record.sort', input: { column: 'severity_number', desc: true } });
  bySeverity.unmount();
  // A log list shows every row it read; it wraps lines and hides columns without another read.
  const logList = render(RecordTable, { input: { signal: 'logs', limit: 120 }, result: { state: 'populated', data: { rows: logs } } });
  expect(logList.getByText('120 records · newest first')).toBeInTheDocument();
  expect(logList.getAllByRole('row')).toHaveLength(121);
  expect(logList.getAllByRole('row')[1]).toHaveTextContent(String(logs.rows[0][3]));
  await fireEvent.click(logList.getByRole('button', { name: 'Wrap lines' }));
  expect(logList.getByRole('button', { name: 'Wrap lines' })).toHaveAttribute('aria-pressed', 'true');
  await fireEvent.click(logList.getByRole('checkbox', { name: 'Message' }));
  expect(logList.queryByRole('columnheader', { name: 'Message' })).toBeNull();
  expect(logList.getByRole('columnheader', { name: 'Severity' })).toBeInTheDocument();
  logList.unmount();
  // Groups are typed values: unset and the text "∅" stay apart, as do durations that both read "1ms".
  const byService = render(RecordTable, { input: { signal: 'spans', groupBy: 'service_name' }, result: { state: 'populated', data: { rows: spans } } });
  expect(byService.getAllByRole('rowheader').map((group) => group.textContent)).toEqual([
    'Service = search · 1',
    'Service = agent-api · 2',
    'Service = ∅ ("∅") · 1',
    'Service = ∅ (null) · 1'
  ]);
  byService.unmount();
  const byDuration = render(RecordTable, { input: { signal: 'spans', groupBy: 'duration_nano' }, result: { state: 'populated', data: { rows: spans } } });
  expect(byDuration.getAllByRole('rowheader').map((group) => group.textContent)).toContain('Duration = 1ms (1000000) · 1');
  expect(byDuration.getAllByRole('rowheader').map((group) => group.textContent)).toContain('Duration = 1ms (1000400) · 1');
  expect(byDuration.getAllByRole('rowheader')).toHaveLength(5);
  byDuration.unmount();
  // Refused counts say why and never read as still loading.
  const refusedCounts = render(RecordTable, { input: { signal: 'traces' }, result: { state: 'populated', data: { rows: traces, counts: { state: 'denied', problem: denied } } } });
  expect(refusedCounts.getByRole('note')).toHaveTextContent('Span and error counts unavailable');
  expect(refusedCounts.getAllByRole('row')[1]).toHaveTextContent('— —');
  expect(refusedCounts.getAllByRole('row')[1]).not.toHaveTextContent('…');
  refusedCounts.unmount();

  // TraceWaterfall: levels below the second start collapsed; expand, errors only, and select.
  const selectAction = vi.fn();
  const fall = render(TraceWaterfall, { input: { traceId: TRACE }, result: { state: 'populated', data: trace }, onaction: selectAction });
  expect(fall.queryByText('GET /search')).toBeNull();
  // The caption says what is collapsed; a collapsed span counts its children; the axis runs from 0 to the trace's end.
  expect(fall.getByText('Waterfall · collapsed below 2 levels · selected path stays open')).toBeInTheDocument();
  expect(fall.getByTitle('1 child spans collapsed')).toHaveTextContent('+1');
  expect([...fall.container.querySelectorAll('.ticks span')].map((tick) => tick.textContent)).toEqual(['0s', '1s', '2s', '2.84s']);
  await fireEvent.click(fall.getByRole('button', { name: 'Expand all' }));
  expect(fall.getByText('GET /search')).toBeInTheDocument();
  await fireEvent.click(fall.getByRole('button', { name: 'Errors only' }));
  expect(fall.queryByText('chat gpt-5')).toBeNull();
  expect(fall.queryByText('late span')).toBeNull();
  await fireEvent.click(fall.getByRole('button', { name: /GET \/search/ }));
  expect(selectAction).toHaveBeenCalledWith({ name: 'trace.select', input: { trace_id: TRACE, span_id: 'c1d2e3f405060708' } });
  fall.unmount();
  // A selected deep span opens its ancestors without Expand all.
  const deep = render(TraceWaterfall, { input: { traceId: TRACE, spanId: 'c1d2e3f405060708' }, result: { state: 'populated', data: trace } });
  expect(deep.getByText('GET /search')).toBeInTheDocument();
  deep.unmount();

  // RecordDetail: recorded messages show; an absent section names what would fill it; filter, exclude and logs intents.
  const detailAction = vi.fn();
  const span = render(RecordDetail, { input: { traceId: TRACE, spanId: 'b9c7c989f97918e1' }, result: { state: 'populated', data: spanRow }, onaction: detailAction });
  expect(span.getByRole('tabpanel')).toHaveTextContent('refund?');
  // Messages read as recorded roles and parts beside the other attributes; tabs count what was recorded.
  expect(span.getAllByRole('tab').map((tab) => tab.textContent)).toEqual(['Messages 3', 'Attributes 6', 'Events 1', 'Exception', 'Raw']);
  expect([...span.getByRole('tabpanel').querySelectorAll('h4')].map((role) => role.textContent)).toEqual(['system instructions', 'user', 'tool']);
  expect(span.getByRole('tabpanel')).toHaveTextContent('"days_since": 20');
  expect(span.getByRole('tabpanel')).not.toHaveTextContent('"role"');
  const side = span.getByLabelText('Other attributes');
  expect(side).toHaveTextContent('gen_ai.request.model');
  expect(side).not.toHaveTextContent('gen_ai.input.messages');
  expect(span.container.querySelector('.exception')).toHaveTextContent('TimeoutError: search timed out');
  expect(span.getByRole('button', { name: 'Filter = execute_tool search' })).toBeInTheDocument();
  await fireEvent.click(span.getByRole('tab', { name: 'Raw' }));
  expect(span.getByRole('tabpanel')).toHaveTextContent('gen_ai.input.messages');
  await fireEvent.click(span.getByRole('tab', { name: 'Exception' }));
  expect(span.getByRole('tabpanel')).toHaveTextContent('TimeoutError');
  await fireEvent.click(span.getByRole('button', { name: 'Exclude' }));
  await fireEvent.click(span.getByRole('button', { name: 'Logs for this span' }));
  expect(detailAction.mock.calls.map(([intent]) => intent)).toEqual([
    { name: 'record.filter', input: { signal: 'spans', column: 'name', value: 'execute_tool search', not: true } },
    { name: 'record.logs', input: { trace_id: TRACE, span_id: 'b9c7c989f97918e1' } }
  ]);
  await span.rerender({ input: { traceId: TRACE, spanId: 'b9c7c989f97918e1' }, result: { state: 'populated', data: bareSpan }, onaction: detailAction });
  await fireEvent.click(span.getByRole('tab', { name: 'Messages' }));
  expect(span.getByRole('tabpanel')).toHaveTextContent('gen_ai.input.messages');
  expect(span.getAllByRole('tab').map((tab) => tab.textContent)).toEqual(['Messages', 'Attributes', 'Events', 'Exception', 'Raw']);
  expect(span.container.querySelector('.exception')).toBeNull();
  span.unmount();
  // A log record reads as severity, body and attributes; it filters by its service and opens its trace.
  const logAction = vi.fn();
  const record = render(RecordDetail, { input: { log: LOG }, result: { state: 'populated', data: logRow }, onaction: logAction });
  expect(record.getByRole('heading', { name: 'ERROR' })).toHaveAttribute('data-tone', 'danger');
  expect(record.container.querySelector('.body')).toHaveTextContent('search timed out');
  expect(record.getByText('latency_ms').nextElementSibling).toHaveTextContent('1412');
  await fireEvent.click(record.getByRole('button', { name: 'Filter = search' }));
  await fireEvent.click(record.getByRole('button', { name: /Open trace/ }));
  expect(logAction.mock.calls.map(([intent]) => intent)).toEqual([
    { name: 'record.filter', input: { signal: 'logs', column: 'service_name', value: 'search' } },
    { name: 'record.open', input: { trace_id: TRACE, span_id: 'b9c7c989f97918e1' } }
  ]);
  expect(record.getByText('Raw')).toBeInTheDocument();
  record.unmount();
  // Without a host to act, the detail offers no dead controls.
  const passive = render(RecordDetail, { input: { traceId: TRACE, spanId: 'b9c7c989f97918e1' }, result: { state: 'populated', data: spanRow } });
  expect(passive.queryByRole('button', { name: 'Exclude' })).toBeNull();
  passive.unmount();
  for (const [name, input] of [
    ['facet.filter', { signal: 'traces', column: 'service_name', value: null }],
    ['trace.select', { trace_id: TRACE, span_id: 'c1d2e3f405060708' }]
  ] as const)
    expect(validateIntent(name === 'facet.filter' ? 'FacetRail' : 'TraceWaterfall', { name, input })).toEqual({ name, input });
  expect(validateIntent('TraceWaterfall', { name: 'trace.select', input: { trace_id: TRACE } })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });

  // Metrics series: latest, largest and mean of the plotted values; unticking a series hides only its line.
  const summarized = render(Chart, { input: timeseries, result: { state: 'populated', data: series } });
  const seriesRows = () => [...summarized.container.querySelectorAll('.legend tbody tr')].map((row) => [...row.querySelectorAll('th, td')].map((cell) => cell.textContent?.trim()));
  expect(seriesRows()).toEqual([['', 'a', '140', '140', '130'], ['', 'b', '80', '80', '80']]);
  const stroke = () => [...summarized.container.querySelectorAll('svg.plot polyline')].map((line) => line.getAttribute('stroke'));
  const strokes = stroke();
  await fireEvent.click(summarized.getByRole('checkbox', { name: 'Show a' }));
  expect(stroke()).toEqual([strokes[1]]);
  expect(summarized.getByRole('application')).toHaveAccessibleName('Latency: b');
  expect(summarized.getByText(/2 series · 1 shown/)).toBeInTheDocument();
  await fireEvent.click(summarized.getByRole('button', { name: 'Show all' }));
  expect(stroke()).toEqual(strokes);
  summarized.unmount();

  // A recorded metric keeps every typed series apart: same instant, NULL versus 'not set', past twenty, never summed or folded.
  const metricColumns = [
    { name: 'at', data_type: 'Timestamp(Nanosecond, Some("UTC"))', nullable: false },
    { name: 'metric_type', data_type: 'Utf8', nullable: false },
    { name: 'attributes', data_type: 'Utf8', nullable: true },
    { name: 'value', data_type: 'Float64', nullable: true }
  ];
  const identity = ['metric_type', 'attributes'];
  const attributeSets = ['{"route":"not set"}', null, '{}', ...Array.from({ length: 21 }, (_, index) => `{"route":"/r${index}"}`)];
  const recorded = {
    columns: metricColumns,
    rows: attributeSets.flatMap((attributes, index) => [
      ['2026-10-20T10:00:00Z', 'gauge', attributes, index],
      ['2026-10-20T11:00:00Z', 'gauge', attributes, index + 1]
    ]),
    terminal: { outcome: 'success', row_count: attributeSets.length * 2, warnings: [] }
  };
  const exactSummary = {
    columns: [...metricColumns.slice(1, 3), { name: 'points', data_type: 'Int64', nullable: false }, ...['latest', 'max', 'mean'].map((name) => ({ name, data_type: 'Utf8', nullable: true }))],
    rows: attributeSets.map((attributes, index) => ['gauge', attributes, 2, index === 0 ? '9007199254740993' : String(index + 1), index === 0 ? '9007199254740993' : String(index + 1), `${index}.5`]),
    terminal: { outcome: 'success', row_count: attributeSets.length, warnings: [] }
  };
  const metricInput = { type: 'timeseries', title: 'm', sql: '-', x: 'at', y: ['value'] } as const;
  let picked: ReadonlySet<string> | undefined;
  const metricChart = render(Chart, {
    input: metricInput,
    result: { state: 'populated', data: recorded },
    identity,
    summary: { state: 'populated', data: exactSummary },
    onseries: (next: ReadonlySet<string>) => (picked = next)
  } as never);
  const metricRows = () => [...metricChart.container.querySelectorAll('.legend tbody tr')].map((row) => [...row.querySelectorAll('th, td')].map((cell) => cell.textContent?.trim()));
  expect(metricRows()).toHaveLength(24);
  expect(metricRows().slice(0, 3)).toEqual([
    ['', '{"route":"not set"}', '9007199254740993', '9007199254740993', '0.5'],
    ['', '∅', '2', '2', '1.5'],
    ['', '{}', '3', '3', '2.5']
  ]);
  // A caption opens to every recorded identity cell by name, by keyboard or touch.
  const caption = metricChart.getByRole('button', { name: '{"route":"not set"}' });
  expect(caption).toHaveAttribute('aria-expanded', 'false');
  await fireEvent.click(caption);
  expect(caption).toHaveAttribute('aria-expanded', 'true');
  expect(caption.closest('th')?.querySelector('dl')).toHaveTextContent('metric_typegaugeattributes{"route":"not set"}');
  await fireEvent.click(caption);
  expect(metricRows()[0]).toEqual(['', '{"route":"not set"}', '9007199254740993', '9007199254740993', '0.5']);
  expect(metricChart.container.textContent).not.toMatch(/\bother\b/);
  expect(metricChart.getByText(/24 series · 20 shown · exact Latest, Max and Mean/)).toBeInTheDocument();
  expect(metricChart.container.querySelectorAll('svg.plot polyline')).toHaveLength(20);
  // The host owns the selection: a tick reports the next set, and the host's set is what draws.
  await fireEvent.click(metricChart.getByRole('checkbox', { name: 'Show ∅' }));
  const nullKey = JSON.stringify(['gauge', null]);
  expect(picked?.has(nullKey)).toBe(false);
  expect(picked?.size).toBe(19);
  await metricChart.rerender({ shown: new Set([nullKey, JSON.stringify(['gauge', '{"route":"not set"}'])]) } as never);
  expect(metricChart.container.querySelectorAll('svg.plot polyline')).toHaveLength(2);
  expect(metricChart.getByRole('checkbox', { name: 'Show ∅' })).toBeChecked();
  expect(metricChart.getByRole('checkbox', { name: 'Show {}' })).not.toBeChecked();
  // A refused exact summary is said, never replaced by plotted doubles.
  await metricChart.rerender({ summary: { state: 'denied', problem: denied } } as never);
  expect(metricChart.getByText('Not authorized')).toBeInTheDocument();
  expect(metricRows()[0].slice(2)).toEqual(['—', '—', '—']);
  metricChart.unmount();

  // Two recorded samples of one series at one instant both draw, and the tooltip names both.
  const twin = render(Chart, {
    input: metricInput,
    result: {
      state: 'populated',
      data: { columns: metricColumns, rows: [['2026-10-20T10:00:00Z', 'gauge', '{}', 1], ['2026-10-20T10:00:00Z', 'gauge', '{}', 3]], terminal: { outcome: 'success', row_count: 2, warnings: [] } }
    },
    identity,
    summary: { state: 'populated', data: { ...exactSummary, rows: [['gauge', '{}', 2, '3', '3', '2']] } }
  } as never);
  expect(twin.container.querySelectorAll('svg.plot path.marker')).toHaveLength(2);
  await fireEvent.focus(twin.getByRole('application'));
  expect(twin.getByRole('status')).toHaveTextContent('gauge · {}: 1, 3');
  twin.unmount();

  // Histogram lines keep each group's typed identity through the shared Chart: six groups plot twenty-four whole lines, twenty drawn,
  // never summed into "other"; a percentile in the unbounded top bucket reads at least its bound in the tooltip and the summary.
  const histogramRead = {
    columns: ['at', 'metric_type', 'unit', 'aggregation_temporality', 'is_monotonic', 'service_name', 'bounds', 'count', 'sum', 'buckets', 'groups'].map((name) => ({ name, data_type: 'Utf8', nullable: true })),
    rows: Array.from({ length: 6 }, (_, index) => ['2026-10-20T10:00:00Z', 'histogram', 's', 2, null, `s${index}`, [1], '2', index + 1, index === 5 ? ['0', '2'] : ['2', '0'], 6]),
    terminal: { outcome: 'success', row_count: 6, warnings: [] }
  };
  const shaped = histogramWindows(histogramRead as never, { from: '2026-10-20T10:00:00Z', to: '2026-10-20T11:00:00Z' }, ['metric_type', 'unit', 'aggregation_temporality', 'is_monotonic', 'service_name']);
  const latencyChart = render(Chart, {
    input: metricInput,
    result: { state: 'populated', data: shaped.latency },
    identity: shaped.identity,
    summary: { state: 'populated', data: shaped.latencySummary }
  } as never);
  expect(latencyChart.getByText(/24 series · 20 shown/)).toBeInTheDocument();
  expect(latencyChart.container.querySelectorAll('svg.plot polyline')).toHaveLength(20);
  expect(latencyChart.container.textContent).not.toMatch(/\bother\b/);
  const latencyRows = [...latencyChart.container.querySelectorAll('.legend tbody tr')].map((row) => [...row.querySelectorAll('th, td')].map((cell) => cell.textContent?.trim()));
  expect(latencyRows.filter(([, caption]) => caption?.endsWith('· mean')).map(([, caption, latest]) => [caption, latest])).toEqual(
    Array.from({ length: 6 }, (_, index) => [`s${index} · mean`, String((index + 1) / 2)])
  );
  expect(latencyRows.find(([, caption]) => caption === 's5 · p99 estimated')?.slice(2)).toEqual(['≥ 1', '≥ 1', '≥ 1']);
  await fireEvent.click(latencyChart.getByRole('button', { name: 'Show all' }));
  await fireEvent.focus(latencyChart.getByRole('application'));
  expect(latencyChart.getByRole('status')).toHaveTextContent('s5 · p99 estimated: ≥ 1');
  expect(latencyChart.getByRole('status')).toHaveTextContent('s0 · p99 estimated: 0.99');
  latencyChart.unmount();

  // A host that owns the list's columns is offered every column of the signal, and a toggle reports the next set.
  let chosenColumns: readonly string[] | undefined;
  const pointList = render(RecordTable, {
    input: { signal: 'metrics', columns: ['metric_name', 'int_value'] },
    result: {
      state: 'populated',
      data: {
        rows: {
          columns: [
            { name: 'metric_name', data_type: 'Utf8', nullable: false },
            { name: 'int_value', data_type: 'Utf8', nullable: true },
            { name: 'time_unix_nano', data_type: 'Timestamp(Nanosecond, Some("UTC"))', nullable: false },
            { name: 'wyrd_request_id', data_type: 'Utf8', nullable: false }
          ],
          rows: [['ui.requests', '9007199254740993', '2026-10-20T10:00:00Z', 'r,"1"']],
          terminal: { outcome: 'success', row_count: 1, warnings: [] }
        }
      }
    },
    oncolumns: (next: readonly string[]) => (chosenColumns = next)
  } as never);
  await fireEvent.click(pointList.getByText('Columns'));
  await fireEvent.click(pointList.getByRole('checkbox', { name: /positive/i }));
  expect(chosenColumns).toEqual(['metric_name', 'int_value', 'positive_buckets']);
  // CSV downloads the rows on screen with each cell as its exact returned text.
  const csvLink = pointList.getByRole('link', { name: /CSV/ });
  expect(csvLink).toHaveAttribute('download', 'metrics.csv');
  expect(decodeURIComponent(csvLink.getAttribute('href')!.split(',').slice(1).join(','))).toBe('metric_name,int_value\r\nui.requests,9007199254740993');
  pointList.unmount();

  // A view renders every Observe panel, and a live panel re-reads on its interval until the view leaves.
  vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
  try {
    let counted = () => {};
    const served = recording((request) => {
      if ((request as { sql?: string }).sql?.includes('AS spans')) counted();
      return fixtureExecutor.run([request]).then(([result]) => result);
    });
    const observe = render(WyrdView, {
      doc: {
        version: 'wyrd.view/v1',
        catalogId: 'wyrd.ui/v1',
        range: RANGE,
        panels: [
          { id: 'stats', component: 'StatStrip', input: { stats: ['traces', 'errors', 'cost'] } },
          { id: 'facets', component: 'FacetRail', input: { signal: 'traces', facets: ['service_name'] } },
          { id: 'list', component: 'RecordTable', input: { signal: 'traces', live: true } },
          { id: 'trace', component: 'TraceWaterfall', input: { traceId: TRACE } },
          { id: 'span', component: 'RecordDetail', input: { traceId: TRACE, spanId: 'b9c7c989f97918e1' } }
        ]
      },
      executor: served.executor
    });
    await waitFor(() => expect(observe.container.querySelector('[data-panel="list"]')).toHaveTextContent('invoke_agent support'));
    for (const id of ['stats', 'facets', 'trace', 'span']) expect(observe.container.querySelector(`[data-panel="${id}"] [data-state]`), id).toBeNull();
    const before = served.calls.length;
    const refreshed = new Promise<void>((resolve) => (counted = resolve));
    vi.advanceTimersByTime(5_000);
    await refreshed;
    expect(served.calls.length).toBe(before + 2);
    observe.unmount();
    vi.advanceTimersByTime(5_000);
    expect(served.calls.length).toBe(before + 2);
  } finally {
    vi.useRealTimers();
  }
});

test('TASK-004: Authored query aliases do not declare telemetry semantics', async () => {
  // An authored query names its own columns; matching a telemetry name gives it no telemetry meaning.
  const sql = render(RecordTable, { input: { sql: AUTHORED_SQL }, result: { state: 'populated', data: { rows: authored } } });
  expect(sql.getAllByRole('columnheader').map((heading) => heading.textContent)).toEqual(['name', 'status_code', 'duration_nano']);
  expect(sql.container.querySelector('.pill, .bar, time, .chip, tr.error')).toBeNull();
  expect(sql.getAllByRole('row')[1]).toHaveTextContent(/^refund policy cited21500000000$/);
  expect(sql.getAllByRole('row')[2]).toHaveTextContent(/^answer grounded03$/);
  // Authored rows are complete, so a heading sorts them in the page: numbers largest first, then the other way.
  await fireEvent.click(sql.getByRole('button', { name: 'duration_nano' }));
  expect(sql.getAllByRole('row')[1]).toHaveTextContent(/^refund policy cited21500000000$/);
  await fireEvent.click(sql.getByRole('button', { name: /duration_nano/ }));
  expect(sql.getAllByRole('row')[1]).toHaveTextContent(/^answer grounded03$/);
  sql.unmount();
  // The same values under a signal list are canonical columns and read as telemetry.
  const signal = render(RecordTable, { input: { signal: 'spans' }, result: { state: 'populated', data: { rows: spans } } });
  expect(signal.getByRole('columnheader', { name: 'Status' })).toBeInTheDocument();
  expect(signal.getAllByRole('row')[1].querySelector('.pill')).toHaveAttribute('data-tone', 'danger');
  expect(signal.getAllByRole('row')[1].querySelector('time')).toBeInTheDocument();
  signal.unmount();
  // Through a view the authored query keeps its raw headings and text.
  const view = render(WyrdView, {
    doc: { version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: RANGE, panels: [{ id: 'q', component: 'RecordTable', input: { sql: AUTHORED_SQL } }] },
    executor: fixtureExecutor
  });
  await waitFor(() => expect(view.container.querySelector('[data-panel="q"]')).toHaveTextContent('refund policy cited'));
  const panel = view.container.querySelector('[data-panel="q"]')!;
  expect([...panel.querySelectorAll('th')].map((heading) => heading.textContent)).toEqual(['name', 'status_code', 'duration_nano']);
  expect(panel.querySelector('.pill, .bar, time, tr.error')).toBeNull();
  view.unmount();
});

test('TASK-004: Telemetry presentation is independent of record names', async () => {
  const tones = (rendered: { container: HTMLElement }) =>
    [...rendered.container.querySelectorAll('tbody tr')].map((row) => [row.classList.contains('error'), row.querySelector('.pill')?.getAttribute('data-tone')]);
  // Status comes from status_code alone: "status.error" with status ok is ok; an HTTP route with status error is an error.
  const list = render(RecordTable, { input: { signal: 'spans' }, result: { state: 'populated', data: { rows: spans } } });
  const original = tones(list);
  expect(original[0]).toEqual([true, 'danger']);
  expect(original[3]).toEqual([false, 'ok']);
  // Renaming every span leaves every tone; changing one recorded status changes only that row.
  const renamed = { ...spans, rows: spans.rows.map((row, index) => [row[0], index === 0 ? 'status.ok' : `error ${index}`, ...row.slice(2)]) };
  await list.rerender({ input: { signal: 'spans' }, result: { state: 'populated', data: { rows: renamed } } });
  expect(tones(list)).toEqual(original);
  const restated = { ...spans, rows: spans.rows.map((row, index) => (index === 3 ? [...row.slice(0, 4), 2, ...row.slice(5)] : row)) };
  await list.rerender({ input: { signal: 'spans' }, result: { state: 'populated', data: { rows: restated } } });
  expect(tones(list)[3]).toEqual([true, 'danger']);
  list.unmount();
  // Facets and row opens carry the exact typed values read.
  const facetAction = vi.fn();
  const rail = render(FacetRail, {
    input: { signal: 'spans', facets: ['status_code', 'duration_nano'] },
    result: { state: 'populated', data: { facets: [{ column: 'status_code', values: { ...facet, rows: [[1, 9], [2, 3]] } }, { column: 'duration_nano', values: { ...facet, rows: [[1_000_000, 1], [1_000_400, 1]] } }] } },
    onaction: facetAction
  });
  await fireEvent.click(rail.getByText('error'));
  await fireEvent.click(rail.getByText('1000400'));
  expect(facetAction.mock.calls.map(([intent]) => intent.input.value)).toEqual([2, 1_000_400]);
  rail.unmount();
  const openAction = vi.fn();
  const opened = render(RecordTable, { input: { signal: 'spans' }, result: { state: 'populated', data: { rows: spans } }, onaction: openAction });
  await fireEvent.click(opened.getAllByRole('row')[4]);
  expect(openAction).toHaveBeenCalledWith({ name: 'record.open', input: { trace_id: 'a3ce929d0e0e47364bf92f3577b34da6', span_id: '53995c3f42cd8ad9' } });
  opened.unmount();
  // A series named like a status draws in its index's style, not a status colour.
  const strokes = (data: typeof series) => {
    const chart = render(Chart, { input: timeseries, result: { state: 'populated', data } });
    const drawn = [...chart.container.querySelectorAll('svg.plot polyline')].map((line) => [line.getAttribute('stroke'), line.getAttribute('stroke-dasharray')]);
    chart.unmount();
    return drawn;
  };
  expect(strokes({ ...series, rows: series.rows.map((row) => [row[0], row[1] === 'a' ? 'error' : 'ok', row[2]]) })).toEqual(strokes(series));
  // The waterfall marks errors by status, whatever the span is called.
  const waterfall = (data: typeof trace) => {
    const fall = render(TraceWaterfall, { input: { traceId: TRACE }, result: { state: 'populated', data } });
    const marked = [...fall.container.querySelectorAll('[role="treeitem"]')].map((row) => row.classList.contains('error'));
    fall.unmount();
    return marked;
  };
  expect(waterfall({ ...trace, rows: trace.rows.map((row, index) => [row[0], row[1], index % 2 ? 'ok span' : 'error span', ...row.slice(3)]) })).toEqual(waterfall(trace));
  // A span with only HTTP attributes has no messages; no role is synthesized from attribute names.
  const http = { ...spanRow, rows: [[...spanRow.rows[0].slice(0, 7), { 'http.route': '/search', 'http.response.status_code': 500, role: 'user' }, [], null, null, null]] };
  const detail = render(RecordDetail, { input: { traceId: TRACE, spanId: 'b9c7c989f97918e1' }, result: { state: 'populated', data: http } });
  expect(detail.getByRole('tabpanel')).toHaveTextContent('No GenAI messages recorded');
  expect(detail.getByRole('tabpanel').querySelector('h4')).toBeNull();
  detail.unmount();
  // Through a view, an ok span named "status.error" still reads ok.
  const view = render(WyrdView, {
    doc: { version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: RANGE, panels: [{ id: 's', component: 'RecordTable', input: { signal: 'spans' } }] },
    executor: fixtureExecutor
  });
  await waitFor(() => expect(view.container.querySelector('[data-panel="s"]')).toHaveTextContent('status.error'));
  const row = [...view.container.querySelectorAll('[data-panel="s"] tbody tr')].find((each) => each.textContent?.includes('status.error'))!;
  expect(row).not.toHaveClass('error');
  expect(row.querySelector('.pill')).toHaveAttribute('data-tone', 'ok');
  view.unmount();
});

test('Observe canvas colour roles and series composition', async () => {
  // Lines take the canonical series order by position, with no status meaning and no blending.
  const six: QueryResult = { ...series, rows: Array.from({ length: 6 }, (_, index) => ['2026-10-20T10:00:00Z', `s${index}`, 10 - index]), terminal: terminal(6) };
  const lines = render(Chart, { input: timeseries, result: { state: 'populated', data: six } });
  expect([...lines.container.querySelectorAll('.legend tbody line')].map((line) => line.getAttribute('stroke'))).toEqual([
    'var(--declare)',
    'var(--observe)',
    'var(--series-3)',
    'var(--series-4)',
    'var(--series-5)',
    'var(--declare)'
  ]);
  lines.unmount();

  // A host's named colours win over position: p95 lime and p50 periwinkle, whatever order the columns read.
  const at = { name: 'at', data_type: 'Timestamp(Nanosecond, Some("UTC"))', nullable: false };
  const durations: QueryResult = {
    columns: [at, { name: 'p95', data_type: 'Float64', nullable: true }, { name: 'p50', data_type: 'Float64', nullable: true }],
    rows: [['2026-10-20T10:00:00Z', 9, 4], ['2026-10-20T10:05:00Z', 8, 3]],
    terminal: terminal(2)
  };
  const duration = render(Chart, {
    input: { type: 'timeseries', title: 'Duration', sql: 'SELECT 1', x: 'at', y: ['p95', 'p50'] } as ChartInput,
    result: { state: 'populated', data: durations },
    compact: true,
    colors: { p95: 'var(--observe)', p50: 'var(--declare)' }
  });
  expect([...duration.container.querySelectorAll('svg.plot polyline')].map((line) => line.getAttribute('stroke'))).toEqual(['var(--observe)', 'var(--declare)']);
  // A compact tile keys each named series and keeps its time labels below the plot, not inside it.
  expect([...duration.getByLabelText('Key').querySelectorAll('i')].map((swatch) => (swatch as HTMLElement).style.background)).toEqual(['var(--observe)', 'var(--declare)']);
  expect(duration.container.querySelectorAll('svg.plot text')).toHaveLength(0);
  expect(duration.container.querySelector('.times')).toHaveTextContent('10:00');
  duration.unmount();

  // Stacked severity bars fill with the exact named role, never an opacity blend.
  const levels: QueryResult = {
    columns: [at, { name: 'level', data_type: 'Utf8', nullable: false }, { name: 'records', data_type: 'Int64', nullable: false }],
    rows: [['2026-10-20T10:00:00Z', 'info', 3], ['2026-10-20T10:00:00Z', 'warn', 2], ['2026-10-20T10:00:00Z', 'error', 1]],
    terminal: terminal(3)
  };
  const timeline = render(Chart, {
    input: { type: 'timeseries', title: 'Records', sql: 'SELECT 1', x: 'at', y: ['records'], series: 'level' } as ChartInput,
    result: { state: 'populated', data: levels },
    compact: true,
    bars: true,
    colors: { info: 'var(--level-info)', warn: 'var(--warn)', error: 'var(--danger)' }
  });
  const segments = [...timeline.container.querySelectorAll('rect.segment')];
  expect(segments.map((segment) => segment.getAttribute('fill')).toSorted()).toEqual(['var(--danger)', 'var(--level-info)', 'var(--warn)']);
  expect(segments.every((segment) => !segment.hasAttribute('opacity') && !segment.hasAttribute('fill-opacity'))).toBe(true);
  timeline.unmount();

  // With a host heading, the chart is two cards: the plot under the heading, then the selectable series summary.
  const heading = createRawSnippet(() => ({ render: () => '<h2>sim.latency</h2>' }));
  const cards = render(Chart, { input: timeseries, result: { state: 'populated', data: series }, heading });
  const frames = cards.container.querySelectorAll('.chart > .frame');
  expect(frames).toHaveLength(2);
  expect(frames[0].querySelector('h2')).toHaveTextContent('sim.latency');
  expect(frames[0].querySelector('svg.plot')).not.toBeNull();
  expect(frames[0].querySelector('table.legend')).toBeNull();
  expect(frames[1].querySelector('table.legend')).not.toBeNull();
  expect(frames[1].querySelectorAll('input[type="checkbox"]')).toHaveLength(2);
  expect(frames[1].querySelector('button')?.textContent).toMatch(/Show all|Open in Query/);
  expect([...frames[1].querySelectorAll('button')].some((button) => /Open in Query/.test(button.textContent ?? ''))).toBe(true);
  // The plot card carries no second header and the summary replaces the SQL footer.
  expect(cards.queryByRole('heading', { name: /Latency/ })).toBeNull();
  expect(cards.container.querySelector('footer')).toBeNull();
  cards.unmount();

  // The waterfall colours each recorded service by series position, errors by status, and keys every service.
  const fall = render(TraceWaterfall, { input: { traceId: TRACE }, result: { state: 'populated', data: trace } });
  await fireEvent.click(fall.getByRole('button', { name: 'Expand all' }));
  const bars = Object.fromEntries(
    [...fall.container.querySelectorAll('[role="treeitem"]')].map((row) => [row.querySelector('.select')?.getAttribute('title'), (row.querySelector('.bar') as HTMLElement).style.background])
  );
  expect(bars).toEqual({
    'invoke_agent support': 'var(--danger)',
    'chat gpt-5': 'var(--declare)',
    'execute_tool search': 'var(--danger)',
    'GET /search': 'var(--danger)',
    'late span': 'var(--observe)'
  });
  const key = fall.getByRole('list', { name: 'Services' });
  expect([...key.querySelectorAll('li')].map((item) => [item.textContent?.trim(), (item.querySelector('i') as HTMLElement).style.background])).toEqual([
    ['agent-api', 'var(--declare)'],
    ['search', 'var(--observe)'],
    ['✕ error', 'var(--danger)']
  ]);
  fall.unmount();
});

/** A fixture answer, recorded; `override` replaces the answer for the reads it matches. */
function fixtureRecording(override: (request: WireRequest) => WireResult | undefined = () => undefined) {
  return recording(async (request) => override(request) ?? (await fixtureExecutor.run([request]))[0]);
}
const ops = (calls: { request: WireRequest }[]) => calls.map((call) => call.request.op);
const failure = { code: 'WYRD_SPEC_500_INTERNAL', title: 'Internal error', status: 500 };

test('card inventory preserves filters cursors and exact references', async () => {
  // A plain word matches names, escaped and case-insensitive; anything else is the metadata query itself.
  expect(inventoryFilter('support')).toBe('name =~ "(?i)support"');
  expect(inventoryFilter(' a.b ')).toBe('name =~ "(?i)a\\\\.b"');
  expect(inventoryFilter('labels.team = "search"')).toBe('labels.team = "search"');
  expect(inventoryFilter('exists')).toBe('exists');
  expect(inventoryFilter('  ')).toBeUndefined();

  // Every page follows the previous server cursor; filters reach cards.list unchanged.
  const paged = fixtureRecording();
  const two = await loadCardInventory({ kind: 'Prompt', status: 'active', include_prerelease: true, query: 'support', pages: 2 }, { executor: paged.executor });
  expect(paged.calls.map((call) => call.request)).toEqual([
    { op: 'cards.list', limit: 50, cursor: null, filter: 'name =~ "(?i)support"', kind: 'Prompt', status: 'active', include_prerelease: true },
    { op: 'cards.list', limit: 50, cursor: 'page-2', filter: 'name =~ "(?i)support"', kind: 'Prompt', status: 'active', include_prerelease: true }
  ]);
  expect(two.result).toMatchObject({ state: 'populated', data: { items: inventory, more: false, filtered: true } });
  const first = await loadCardInventory({}, { executor: fixtureExecutor });
  expect(first.result).toMatchObject({ state: 'populated', data: { items: inventory.slice(0, 3), more: true, filtered: false } });
  // A refused page is the panel's state, never rows beside a failure; empty says why.
  const late = recording((request) => ((request as { cursor: string | null }).cursor ? { ok: false, problem: denied } : { ok: true, value: { items: inventory, next_cursor: 'next' } }));
  expect((await loadCardInventory({ pages: 2 }, { executor: late.executor })).result).toEqual({ state: 'denied', problem: denied });
  expect((await loadCardInventory({}, { executor: stateExecutors.error })).result.state).toBe('error');
  expect((await loadCardInventory({}, { executor: stateExecutors.empty })).result).toMatchObject({ state: 'empty', reason: 'This tenant has no registered Cards.' });
  expect((await loadCardInventory({ query: 'x' }, { executor: stateExecutors.empty })).result).toMatchObject({ state: 'empty', reason: 'No Card version matches these filters.' });

  // Only registrable kinds, listable statuses and bounded pages validate.
  for (const input of [{ kind: 'Policy' }, { kind: 'External' }, { status: 'deleted' }, { pages: 0 }, { pages: 21 }, { cursor: 'x' }])
    expect(validatePanel('CardInventory', input)).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect(catalog.components.CardInventory.emits.map((action) => action.name)).toEqual(['cards.filter', 'cards.more', 'card.open']);

  // The URL round-trips filters and pages; a filter change drops pages, starting a fresh cursor chain.
  const search = writeInventory({ query: 'support', kind: 'Prompt', status: 'active', include_prerelease: true, pages: 3 });
  expect(readInventory(new URLSearchParams(search))).toEqual({ query: 'support', kind: 'Prompt', status: 'active', include_prerelease: true, pages: 3 });
  expect(writeInventory({ query: 'support' })).toBe('?q=support');

  // Rows open their exact reference; filters, kinds and Load more emit validated intents.
  const onaction = vi.fn();
  const view = render(CardInventory, { input: { kind: 'Prompt' }, result: first.result, onaction });
  expect(within(view.container).getAllByRole('row')).toHaveLength(4);
  await fireEvent.click(within(view.container).getAllByRole('button', { name: 'support-reply' })[0]);
  await fireEvent.input(within(view.container).getByRole('textbox'), { target: { value: 'support' } });
  await fireEvent.submit(within(view.container).getByRole('textbox').closest('form') as HTMLFormElement);
  await fireEvent.click(within(within(view.container).getByRole('navigation', { name: 'Kinds' })).getByRole('button', { name: 'Model' }));
  await fireEvent.change(within(view.container).getByRole('combobox'), { target: { value: 'deprecated' } });
  await fireEvent.click(within(view.container).getByRole('checkbox'));
  await fireEvent.click(within(view.container).getByRole('button', { name: 'Load more' }));
  const intents = onaction.mock.calls.map(([intent]) => intent);
  expect(intents).toEqual([
    { name: 'card.open', input: { card_ref: { kind: 'Prompt', name: 'support-reply', version: '1.0.0', uid: 'card_Prompt_support-reply_1.0.0' } } },
    { name: 'cards.filter', input: { kind: 'Prompt', query: 'support' } },
    { name: 'cards.filter', input: { kind: 'Model' } },
    { name: 'cards.filter', input: { kind: 'Prompt', status: 'deprecated' } },
    { name: 'cards.filter', input: { kind: 'Prompt', include_prerelease: true } },
    { name: 'cards.more', input: { pages: 2 } }
  ]);
  for (const intent of intents) expect(validateIntent('CardInventory', intent)).toEqual(intent);
  expect(validateIntent('CardInventory', { name: 'card.open', input: { card_ref: { kind: 'Model', name: 'ranker', version: 'latest' } } })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  // A pending action takes no further input.
  await view.rerender({ input: {}, result: first.result, onaction, actions: { 'cards.more': { state: 'pending' } } });
  expect(within(view.container).getByRole('button', { name: 'Load more' })).toBeDisabled();

  // Empty, filtered-empty, denied and failed stay distinct.
  const states = render(CardInventory, { input: { query: 'nothing' }, result: { state: 'empty', data: { items: [], more: false, filtered: true }, reason: 'No Card version matches these filters.' }, onaction });
  await fireEvent.click(within(states.container).getByRole('button', { name: 'Clear filter' }));
  expect(onaction).toHaveBeenLastCalledWith({ name: 'cards.filter', input: {} });
  await states.rerender({ input: {}, result: { state: 'empty', data: { items: [], more: false, filtered: false }, reason: 'This tenant has no registered Cards.' } });
  expect(states.container).toHaveTextContent('wyrd declare');
  await states.rerender({ input: { query: 'a ==' }, result: { state: 'error', problem: { code: 'WYRD_QUERY_400_INVALID_SYNTAX', title: 'Invalid query' } } });
  expect(states.container).toHaveTextContent('WYRD_QUERY_400_INVALID_SYNTAX');
  await states.rerender({ input: {}, result: { state: 'denied', problem: denied } });
  expect(states.container).toHaveTextContent('Not authorized');

  // The fixture view document validates and renders through WyrdView.
  const doc = { version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: RANGE, panels: [{ id: 'cards', component: 'CardInventory', input: { pages: 2 } }] };
  expect(validateView(doc)).toMatchObject({ panels: [{ component: 'CardInventory' }] });
  const rendered = render(WyrdView, { doc, executor: fixtureExecutor });
  await waitFor(() => expect(rendered.container.querySelectorAll('[data-panel="cards"] tbody tr')).toHaveLength(6));
});

test('exact card detail preserves relationships files and action outcomes', async () => {
  // A refused Card read issues no dependent read.
  for (const load of [loadRelationshipList, loadCardFacts]) {
    const refused = recording(() => ({ ok: false, problem: denied }));
    expect((await load({ cardRef: AGENT }, { executor: refused.executor })).result).toEqual({ state: 'denied', problem: denied });
    expect(ops(refused.calls)).toEqual(['cards.get_by_ref']);
  }

  // Relationships keep the server's exact references and direction.
  const reads = fixtureRecording();
  const relationships = await loadRelationshipList({ cardRef: AGENT }, { executor: reads.executor });
  expect(reads.calls[0].request).toEqual({ op: 'cards.get_by_ref', card_ref: AGENT });
  expect(relationships.result).toEqual({ state: 'populated', data: { outbound: [{ ref: PROMPT }], inbound: [{ ref: SERVICE }] } });
  expect((await loadRelationshipList({ cardRef: AGENT }, { executor: stateExecutors.empty })).result.state).toBe('empty');

  // Files come from the registered-file listing of that exact version; none is empty, never a missing Card.
  const files = fixtureRecording();
  const populated = await loadArtifactTable({ cardRef: card }, { executor: files.executor });
  expect(files.calls.map((call) => call.request)).toEqual([{ op: 'cards.list_artifacts', card_ref: card }]);
  expect(populated.result).toEqual({ state: 'populated', data: artifacts.artifacts });
  expect((await loadArtifactTable({ cardRef: card }, { executor: stateExecutors.empty })).result).toMatchObject({ state: 'empty', reason: 'Registration recorded no files for this version.' });
  expect((await loadArtifactTable({ cardRef: card }, { executor: stateExecutors.denied })).result.state).toBe('denied');
  expect((await loadArtifactTable({ cardRef: card }, { executor: recording(() => ({ ok: false, problem: failure })).executor })).result).toEqual({ state: 'error', problem: failure });
  const table = render(ArtifactTable, populated);
  expect(table.container).toHaveTextContent('weights/model.bin');
  expect(table.container).toHaveTextContent('4.2 MiB');
  expect(table.container).toHaveTextContent(artifacts.artifacts[0].sha256);
  expect(table.container).toHaveTextContent('application/octet-stream');
  expect(within(table.container).getAllByRole('row')).toHaveLength(3);

  // MCP has no file-listing tool yet: the host's real refusal is the read's outcome.
  const app = { callServerTool: vi.fn(async () => ({ isError: true, content: [], structuredContent: { code: 'WYRD_MCP_404_TOOL_NOT_FOUND', title: 'Tool not found', status: 404 } })) };
  const [refusal] = await mcpExecutor(app as never).run([{ op: 'cards.list_artifacts', card_ref: card }]);
  expect(app.callServerTool).toHaveBeenCalledWith({ name: 'cards.list_artifacts', arguments: { card_ref: card } }, { signal: undefined });
  expect(refusal).toEqual({ ok: false, problem: { code: 'WYRD_MCP_404_TOOL_NOT_FOUND', title: 'Tool not found', status: 404, detail: undefined } });

  // Facts and YAML show the Card as returned.
  const facts = await loadCardFacts({ cardRef: PROMPT }, { executor: fixtureExecutor });
  const shown = render(CardFacts, facts);
  expect(shown.container).toHaveTextContent('card_prompt_1.1.0');
  expect(shown.container).toHaveTextContent('labels.team');
  const yaml = render(CardFacts, { ...facts, input: { cardRef: PROMPT, yaml: true } });
  expect(yaml.container.querySelector('pre')?.textContent).toBe(fmtYaml(prompts['1.1.0']));
  expect(fmtYaml({ apiVersion: 'wyrd/v1', spec: { system: 'a\nb', count: 1, empty: [], tags: ['x', 'true'] } })).toBe(
    ['apiVersion: wyrd/v1', 'spec:', '  system: |-', '    a', '    b', '  count: 1', '  empty: []', '  tags:', '    - x', '    - "true"'].join('\n')
  );

  // Following a relationship emits its exact reference; the host reports each outcome beside it.
  const onaction = vi.fn();
  const list = render(RelationshipList, { ...relationships, onaction });
  await fireEvent.click(within(list.container).getByRole('button', { name: fmtCardRef(SERVICE) }));
  expect(onaction).toHaveBeenCalledWith({ name: 'card.open', input: { card_ref: SERVICE } });
  expect(validateIntent('RelationshipList', onaction.mock.calls[0][0])).toEqual(onaction.mock.calls[0][0]);
  await list.rerender({ ...relationships, onaction, actions: { 'card.open': { state: 'pending' } } });
  expect(within(list.container).getByRole('button', { name: fmtCardRef(SERVICE) })).toBeDisabled();
  await list.rerender({ ...relationships, onaction, actions: { 'card.open': { state: 'denied', problem: denied } } });
  expect(within(list.container).getByRole('alert')).toHaveTextContent(denied.code);
  await list.rerender({ ...relationships, onaction: undefined, actions: {} });
  expect(within(list.container).getByRole('button', { name: fmtCardRef(SERVICE) })).toBeDisabled();

  // Every kind keeps the Files tab; the detail URL names one exact version and nothing else.
  expect(CARD_TABS.map((tab) => tab.id)).toEqual(['overview', 'files', 'yaml']);
  expect(PROMPT_TABS.map((tab) => tab.id)).toEqual(['overview', 'versions', 'files', 'yaml']);
  expect(writeDetail({ ...PROMPT, uid: 'u 1' }, { tab: 'versions', compare: '1.0.0' })).toBe('?kind=Prompt&name=support-reply&version=1.1.0&uid=u+1&tab=versions&compare=1.0.0');
  const detail = (search: string) => loadCardDetail({ locals: {}, url: new URL(`http://ui.test/t/acme/cards/detail${search}`), request: new Request('http://ui.test') } as never);
  for (const search of ['?kind=Model&name=ranker', '?kind=Model&name=ranker&version=latest', '?kind=Policy&name=p&version=1.0.0', '?name=ranker&version=1.0.0'])
    expect(await detail(search)).toMatchObject({ problem: { code: 'WYRD_SPEC_400_VALIDATION' } });

  // A replaced version supersedes its in-flight read; the late old answer never renders.
  const pending: ((result: WireResult) => void)[] = [];
  const held = recording(() => new Promise<WireResult>((resolve) => pending.push(resolve)));
  const doc = (version: string) => ({ version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: RANGE, panels: [{ id: 'facts', component: 'CardFacts', input: { cardRef: { ...PROMPT, version } } }] });
  const view = render(WyrdView, { doc: doc('1.0.0'), executor: held.executor });
  await waitFor(() => expect(held.calls).toHaveLength(1));
  await view.rerender({ doc: doc('1.1.0'), executor: held.executor });
  await waitFor(() => expect(held.calls).toHaveLength(2));
  expect(held.calls[0].signal?.aborted).toBe(true);
  expect(held.calls[1].request).toEqual({ op: 'cards.get_by_ref', card_ref: { ...PROMPT, version: '1.1.0' } });
  pending[1]({ ok: true, value: prompts['1.1.0'] });
  await waitFor(() => expect(view.container).toHaveTextContent('card_prompt_1.1.0'));
  pending[0]({ ok: true, value: prompts['1.0.0'] });
  await tick();
  expect(view.container).not.toHaveTextContent('card_prompt_1.0.0');
});

test('prompt definitions and comparisons preserve authored forms', async () => {
  // Portable and native forms render as authored; nothing reads telemetry.
  const reads = fixtureRecording();
  const portable = await loadPromptMessages({ cardRef: PROMPT }, { executor: reads.executor });
  expect(portable.result).toEqual({ state: 'populated', data: prompts['1.1.0'].spec });
  const shown = render(PromptMessages, portable);
  expect(within(shown.container).getByRole('region', { name: 'Message 1' })).toHaveTextContent('1 · user');
  expect(within(shown.container).getByRole('region', { name: 'System' }).querySelector('pre')?.textContent).toBe('You answer support tickets.\nCite the policy section you rely on.');
  expect(shown.container).toHaveTextContent('response verdict');
  const native = await loadPromptMessages({ cardRef: NATIVE }, { executor: reads.executor });
  const body = render(PromptMessages, native);
  expect(within(body.container).getByRole('region', { name: 'Native body' }).querySelector('pre')?.textContent).toBe(fmtYaml((native.result as { data: { body: unknown } }).data.body));
  expect(body.container).toHaveTextContent('future_knob: true');
  expect(body.container).toHaveTextContent('native responses');
  // Multimodal and long content stay readable; an absent system message says so.
  const long = 'x'.repeat(5000);
  const media = { ...prompts['1.0.0'], spec: { provider: 'anthropic', model: 'claude', messages: [{ role: 'user', content: [{ type: 'text', text: long }, { type: 'image', source: '${media:photo}' }] }] } };
  const multimodal = render(PromptMessages, await loadPromptMessages({ cardRef: PROMPT }, { executor: recording(() => ({ ok: true, value: media as never })).executor }));
  expect(multimodal.container).toHaveTextContent('No system message.');
  expect(within(multimodal.container).getByRole('region', { name: 'Message 1' })).toHaveTextContent('[image] ${media:photo}');
  expect(within(multimodal.container).getByRole('region', { name: 'Message 1' }).textContent).toContain(long);
  // Only Prompt references validate; a non-Prompt answer is an error, not a reinterpreted spec.
  expect(validatePanel('PromptMessages', { cardRef: card })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect((await loadPromptMessages({ cardRef: PROMPT }, { executor: recording(() => ({ ok: true, value: agent })).executor })).result.state).toBe('error');

  // Variables are the server's status.prompt derivation, in its order.
  const variables = await loadPromptVariables({ cardRef: PROMPT }, { executor: reads.executor });
  expect(variables.result).toEqual({ state: 'populated', data: [{ name: 'ticket', kind: 'text' }, { name: 'tone', kind: 'text' }] });
  const withMedia = { ...prompts['1.0.0'], status: { phase: 'active', prompt: { variables: [], media_variables: ['photo'] } } };
  expect((await loadPromptVariables({ cardRef: PROMPT }, { executor: recording(() => ({ ok: true, value: withMedia as never })).executor })).result).toEqual({ state: 'populated', data: [{ name: 'photo', kind: 'media' }] });
  expect((await loadPromptVariables({ cardRef: PROMPT }, { executor: stateExecutors.empty })).result).toMatchObject({ state: 'empty', reason: 'This Prompt renders with no variables.' });
  expect(within(render(PromptVariables, variables).container).getAllByRole('row')).toHaveLength(3);

  // Used by: inbound Agent relationships, each Agent read for its route; a refused Agent keeps its row.
  const users = fixtureRecording();
  const usedBy = await loadUsedBy({ cardRef: PROMPT }, { executor: users.executor });
  expect(users.calls.map((call) => call.request)).toEqual([{ op: 'cards.get_by_ref', card_ref: PROMPT }, { op: 'cards.get_by_ref', card_ref: AGENT }]);
  expect(usedBy.result).toEqual({ state: 'populated', data: [{ ref: AGENT, agent: { state: 'populated', data: { route: { kind: 'wyrd_gateway', deployment: 'primary' }, services: [SERVICE] } } }] });
  const refusedAgent = await loadUsedBy({ cardRef: PROMPT }, { executor: fixtureRecording((request) => (request.op === 'cards.get_by_ref' && request.card_ref.kind === 'Agent' ? { ok: false, problem: denied } : undefined)).executor });
  expect(refusedAgent.result).toEqual({ state: 'populated', data: [{ ref: AGENT, agent: { state: 'denied', problem: denied } }] });
  const refusedPrompt = recording(() => ({ ok: false, problem: denied }));
  expect((await loadUsedBy({ cardRef: PROMPT }, { executor: refusedPrompt.executor })).result.state).toBe('denied');
  expect(ops(refusedPrompt.calls)).toEqual(['cards.get_by_ref']);
  const onaction = vi.fn();
  const table = render(UsedBy, { ...usedBy, onaction });
  expect(table.container).toHaveTextContent('wyrd_gateway · primary');
  await fireEvent.click(within(table.container).getByRole('button', { name: fmtCardRef(AGENT) }));
  expect(onaction).toHaveBeenLastCalledWith({ name: 'card.open', input: { card_ref: AGENT } });
  expect(render(UsedBy, refusedAgent).container).toHaveTextContent(denied.code);

  // A comparison pins both versions and shows only the authored differences.
  const compared = fixtureRecording();
  const diff = await loadVersionDiff({ cardRef: PROMPT }, { executor: compared.executor });
  expect(compared.calls.map((call) => call.request)).toEqual([
    { op: 'cards.get_by_ref', card_ref: PROMPT },
    { op: 'cards.list', kind: 'Prompt', name: 'support-reply', include_prerelease: true, limit: 100, cursor: null },
    { op: 'cards.get_by_ref', card_ref: { kind: 'Prompt', name: 'support-reply', version: '1.0.0' } }
  ]);
  expect(diff.result).toMatchObject({ state: 'populated', data: { versions: ['1.1.0', '1.0.0'], other: '1.0.0' } });
  const changes = (diff.result as { data: { changes: { field: string; from?: unknown; to?: unknown }[] } }).data.changes;
  expect(changes.map((change) => change.field)).toEqual(['system', 'messages[0]', 'response_format', 'variables']);
  expect(changes.find((change) => change.field === 'response_format')).toEqual({ field: 'response_format', to: 'verdict' });
  expect(changes.find((change) => change.field === 'variables')).toEqual({ field: 'variables', from: ['ticket'], to: ['ticket', 'tone'] });
  // Any two versions: the older one compared against the newer, in either direction.
  expect((await loadVersionDiff({ cardRef: { ...PROMPT, version: '1.0.0' }, otherVersion: '1.1.0' }, { executor: fixtureExecutor })).result).toMatchObject({ data: { other: '1.1.0' } });
  expect(validatePanel('VersionDiff', { cardRef: PROMPT, otherVersion: PROMPT.version })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect(validatePanel('VersionDiff', { cardRef: PROMPT, otherVersion: 'latest' })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect((await loadVersionDiff({ cardRef: PROMPT, otherVersion: '9.9.9' }, { executor: fixtureExecutor })).result).toEqual({ state: 'error', problem: notFound });
  const single = await loadVersionDiff({ cardRef: NATIVE }, { executor: fixtureExecutor });
  expect(single.result).toMatchObject({ state: 'empty', reason: 'Only version 1.0.0 is registered.' });
  expect(render(VersionDiff, single).container).toHaveTextContent('Only version 1.0.0 is registered.');
  const picker = render(VersionDiff, { ...diff, onaction });
  expect(within(picker.container).getByRole('region', { name: 'system' }).querySelectorAll('pre')).toHaveLength(2);
  await fireEvent.change(within(picker.container).getByRole('combobox'), { target: { value: '1.0.0' } });
  expect(onaction).toHaveBeenLastCalledWith({ name: 'prompt.compare', input: { other_version: '1.0.0' } });
  expect(validateIntent('VersionDiff', onaction.mock.lastCall?.[0])).toEqual({ name: 'prompt.compare', input: { other_version: '1.0.0' } });

  // No Prompt component reads telemetry.
  expect([...reads.calls, ...users.calls, ...compared.calls].map((call) => call.request.op)).not.toContain('bifrost.query');
  for (const name of ['PromptMessages', 'PromptVariables', 'UsedBy', 'VersionDiff'] as const)
    expect(validateView({ version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: RANGE, panels: [{ id: 'p', component: name, input: { cardRef: PROMPT } }] })).toMatchObject({ panels: [{ component: name }] });
});
