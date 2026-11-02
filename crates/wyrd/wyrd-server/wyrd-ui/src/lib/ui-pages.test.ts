import { expect, test } from 'vitest';
import {
  loadFacetRail,
  loadRecordTable,
  loadStatStrip,
  validatePanel,
  wyrdProblem,
  type Executor,
  type WireRequest,
  type WireResult
} from 'wyrd-ai-ui/loaders';
import { validateView } from 'wyrd-ai-ui/view';
import { closed, detailView, observeView, readState, step, writeState } from './observe-host';

const RANGE = { from: '2026-10-20T00:00:00Z', to: '2026-10-21T00:00:00Z' };
const EMPTY = { columns: [], rows: [], terminal: { outcome: 'success', row_count: 0, warnings: [] } };

/** An executor answering each read with `answer` and recording every request. */
function recording(answer: (request: WireRequest) => WireResult) {
  const calls: WireRequest[] = [];
  const executor: Executor = {
    run: (requests) => Promise.resolve(requests.map((request) => (calls.push(request), answer(request)))) as never
  };
  return { executor, calls };
}

/** Validation normalizes to `normalized`; every query is empty. */
const normalizing = (normalized: string) => (request: WireRequest): WireResult =>
  request.op === 'bifrost.validate_predicate'
    ? { ok: true, value: { expression: normalized } }
    : ({ ok: true, value: EMPTY } as never);

test('observe signal planning and interactions', async () => {
  // A predicate is validated against the signal's own table before any read, then ANDed in parentheses.
  const logs = recording(normalizing('severity_number >= 17 OR body_text LIKE \'%refund%\''));
  const list = await loadRecordTable(
    { signal: 'logs', where: [{ column: 'service_name', value: 'support-desk' }], where_sql: "severity_number>=17 or body_text like '%refund%'" },
    { executor: logs.executor, range: RANGE }
  );
  expect(logs.calls[0]).toEqual({
    op: 'bifrost.validate_predicate',
    namespace: 'bifrost.logs',
    name: 'records',
    expression: "severity_number>=17 or body_text like '%refund%'"
  });
  expect(list.executed?.sql).toContain("AND service_name = $3 AND (severity_number >= 17 OR body_text LIKE '%refund%')");
  expect(list.executed?.params).toEqual([RANGE.from, RANGE.to, 'support-desk']);
  expect(logs.calls).toHaveLength(2);

  // Facets and the strip read the same normalized predicate; trace roots find it on any span of the trace.
  const spans = recording(normalizing('http_response_status_code = 404'));
  await loadFacetRail({ signal: 'traces', facets: ['name', 'service_name'], where_sql: 'http_response_status_code=404' }, { executor: spans.executor, range: RANGE });
  await loadStatStrip({ stats: ['traces', 'errors'], where_sql: 'http_response_status_code=404' }, { executor: spans.executor, range: RANGE });
  const roots = await loadRecordTable({ signal: 'traces', where_sql: 'http_response_status_code=404' }, { executor: spans.executor, range: RANGE });
  const queries = spans.calls.filter((call) => call.op === 'bifrost.query');
  expect(spans.calls.filter((call) => call.op === 'bifrost.validate_predicate')).toHaveLength(3);
  expect(queries).toHaveLength(2 + 4 + 1);
  for (const call of queries) expect(call.op === 'bifrost.query' && call.sql).toContain('(http_response_status_code = 404)');
  expect(roots.executed?.sql).toContain('parent_span_id IS NULL AND trace_id IN (SELECT trace_id FROM bifrost.traces.spans WHERE');

  // A refused predicate stops the panel with its stable problem and issues no dependent read.
  for (const [problem, state] of [
    [wyrdProblem('validation', 'unknown column'), 'error'],
    [{ code: 'WYRD_VALA_400_QUERY_INVALID_SQL', title: 'Invalid SQL', status: 400 }, 'error'],
    [{ code: 'WYRD_VALA_403_QUERY_FORBIDDEN', title: 'Forbidden', status: 403 }, 'denied']
  ] as const) {
    const refused = recording(() => ({ ok: false, problem }));
    const panels = [
      await loadRecordTable({ signal: 'spans', where_sql: 'id = 1; DROP TABLE x' }, { executor: refused.executor, range: RANGE }),
      await loadFacetRail({ signal: 'spans', facets: ['name'], where_sql: 'id = 1; DROP TABLE x' }, { executor: refused.executor, range: RANGE }),
      await loadStatStrip({ stats: ['spans'], where_sql: 'id = 1; DROP TABLE x' }, { executor: refused.executor, range: RANGE })
    ];
    for (const panel of panels) expect(panel.result).toEqual({ state, problem });
    expect(refused.calls.map((call) => call.op)).toEqual(Array(3).fill('bifrost.validate_predicate'));
  }

  // A blank predicate asks nothing and adds nothing.
  const blank = recording(normalizing(''));
  const plain = await loadRecordTable({ signal: 'metrics', where_sql: '   ' }, { executor: blank.executor, range: RANGE });
  expect(blank.calls.map((call) => call.op)).toEqual(['bifrost.query']);
  expect(plain.executed?.sql).not.toContain('(');

  // The panel schemas bound the predicate and keep it off gateway stats.
  expect(validatePanel('RecordTable', { signal: 'logs', where_sql: 'x'.repeat(8193) })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect(validatePanel('StatStrip', { stats: ['calls'], where_sql: 'name = 1' })).toMatchObject({
    detail: 'StatStrip input.where_sql: span predicates do not apply to gateway tokens, calls or cost'
  });

  // The host keeps the whole investigation in the URL and plans valid views from it.
  const shared = new URLSearchParams({
    signal: 'logs',
    last: '6h',
    where: JSON.stringify([{ column: 'service_name', value: 'support-desk' }]),
    where_sql: 'severity_number >= 17',
    group: 'severity_text',
    live: '1'
  });
  const state = readState(shared);
  expect(readState(writeState(state))).toEqual(state);
  expect(readState(new URLSearchParams())).toMatchObject({ signal: 'traces', mode: 'traces', last: '1h', where: [], where_sql: '', live: false });
  expect(readState(new URLSearchParams({ signal: 'nope', where: '{bad' }))).toMatchObject({ signal: 'traces', where: [] });
  const main = observeView(state);
  expect(validateView(main)).not.toHaveProperty('code');
  for (const panel of main.panels) expect(validatePanel(panel.component, panel.input)).not.toHaveProperty('code');
  expect(main.panels.map((panel) => panel.component)).toEqual(['FacetRail', 'RecordTable']);
  expect(main.panels[1].input).toEqual({
    signal: 'logs',
    where: [{ column: 'service_name', value: 'support-desk' }],
    where_sql: 'severity_number >= 17',
    groupBy: 'severity_text',
    live: true
  });
  const spanView = observeView(readState(new URLSearchParams({ mode: 'spans' })));
  expect(spanView.panels.map((panel) => [panel.component, (panel.input as { signal?: string }).signal])).toEqual([
    ['StatStrip', undefined],
    ['FacetRail', 'spans'],
    ['RecordTable', 'spans']
  ]);
  expect(spanView.range).toEqual({ last: '1h' });

  // Intents become URL steps: filters dedupe, records open in the drawer, queries open in Query.
  const traces = readState(new URLSearchParams());
  const facet = { name: 'facet.filter', input: { signal: 'traces', column: 'service_name', value: 'api' } };
  const filtered = step((step(traces, facet, '/') as { state: ReturnType<typeof readState> }).state, facet, '/') as { state: ReturnType<typeof readState> };
  expect(filtered.state.where).toEqual([{ column: 'service_name', value: 'api' }]);
  const excluded = step(traces, { name: 'record.filter', input: { signal: 'traces', column: 'kind', value: 2, not: true } }, '/');
  expect('state' in excluded && excluded.state.where).toEqual([{ column: 'kind', value: 2, not: true }]);
  const trace = 'a'.repeat(32);
  const opened = step(traces, { name: 'record.open', input: { trace_id: trace, span_id: 'b'.repeat(16) } }, '/') as { state: ReturnType<typeof readState> };
  expect(detailView(opened.state)?.panels.map((panel) => panel.component)).toEqual(['TraceWaterfall', 'RecordDetail']);
  // Opening a record never changes the list's view, so the list is not reloaded.
  expect(observeView(closed(opened.state))).toEqual(observeView(traces));
  expect(detailView(traces)).toBeUndefined();
  const fromLogs = step(state, { name: 'record.open', input: { trace_id: trace } }, '/') as { state: ReturnType<typeof readState> };
  expect(fromLogs.state).toMatchObject({ signal: 'traces', trace, where: [], where_sql: '' });
  const log = { time_unix_nano: '1', observed_time_unix_nano: '2', wyrd_request_id: 'r' };
  const record = step(state, { name: 'record.open', input: { log } }, '/') as { state: ReturnType<typeof readState> };
  expect(detailView(record.state)?.panels).toEqual([{ id: 'detail', component: 'RecordDetail', input: { log } }]);
  expect(readState(writeState(record.state)).log).toEqual(log);
  const inspect = step(traces, { name: 'record.open_in_query', input: { sql: 'SELECT $1', params: ['x'] } }, '/t/acme/observe?live=1');
  expect('query' in inspect && Object.fromEntries(inspect.query)).toEqual({ sql: 'SELECT $1', params: '["x"]', back: '/t/acme/observe?live=1' });
  expect(step(traces, { name: 'record.logs', input: {} }, '/')).toHaveProperty('unavailable');
});
