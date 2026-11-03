import { expect, test } from 'vitest';
import {
  loadFacetRail,
  loadRecordTable,
  loadStatStrip,
  listQuery,
  metricQueries,
  validatePanel,
  wyrdProblem,
  type Executor,
  type WireRequest,
  type WireResult
} from 'wyrd-ai-ui/loaders';
import { presetLabel, rangeLabel, readInstant } from './TimePicker.svelte';
import { DETAIL_PARAMS, closed, groupColumns, listInputs, listParams, readState, selectedMetric, step, writeState, zoomed } from './observe-host';

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

  // The Logs timeline counts log records under the logs predicate; severity is the recorded number.
  const timeline = recording(normalizing("body_text LIKE '%refund%'"));
  await loadStatStrip(
    { stats: ['logs', 'log_errors'], where: [{ column: 'severity_text', value: 'ERROR' }], where_sql: "body_text like '%refund%'" },
    { executor: timeline.executor, range: RANGE }
  );
  expect(timeline.calls[0]).toMatchObject({ op: 'bifrost.validate_predicate', namespace: 'bifrost.logs', name: 'records' });
  const logReads = timeline.calls.filter((call) => call.op === 'bifrost.query');
  expect(logReads).toHaveLength(4);
  for (const call of logReads) expect(call.op === 'bifrost.query' && call.sql).toMatch(/FROM bifrost\.logs\.records WHERE .*severity_text = \$\d.*\(body_text LIKE '%refund%'\)/);
  expect(logReads.filter((call) => call.op === 'bifrost.query' && call.sql.includes('severity_number >= 17'))).toHaveLength(2);
  expect(validatePanel('StatStrip', { stats: ['logs', 'spans'], where: [{ column: 'severity_text', value: 'ERROR' }] })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect(validatePanel('StatStrip', { stats: ['logs'], where: [{ column: 'name', value: 'x' }] })).toMatchObject({ detail: expect.stringContaining('name is not a logs column') });

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
  expect(plain.executed?.sql).not.toContain('AND (');

  // The panel schemas bound the predicate and keep it off gateway stats.
  expect(validatePanel('RecordTable', { signal: 'logs', where_sql: 'x'.repeat(8193) })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect(validatePanel('StatStrip', { stats: ['calls'], where_sql: 'name = 1' })).toMatchObject({
    detail: 'StatStrip input.where_sql: span predicates do not apply to gateway tokens, calls or cost'
  });

  // The host keeps the whole investigation in the URL and plans valid shared loader inputs from it.
  const shared = new URLSearchParams({
    signal: 'logs',
    last: '6h',
    tz: 'local',
    where: JSON.stringify([{ column: 'service_name', value: 'support-desk' }]),
    where_sql: 'severity_number >= 17',
    group: 'severity_text',
    columns: JSON.stringify(['body_text', 'severity_text']),
    live: '1'
  });
  const state = readState(shared);
  expect(readState(writeState(state))).toEqual(state);
  expect(readState(new URLSearchParams())).toMatchObject({ signal: 'traces', mode: 'traces', last: '1h', tz: 'utc', where: [], where_sql: '', live: false });
  expect(readState(new URLSearchParams({ signal: 'nope', where: '{bad', columns: '[1]', by: 'unit' }))).toMatchObject({ signal: 'traces', where: [], columns: undefined, by: undefined });
  // The Metrics recorded-points inspector is reader-opened URL state; no other signal carries it.
  const inspecting = readState(new URLSearchParams({ signal: 'metrics', points: '1' }));
  expect(inspecting.points).toBe(true);
  expect(writeState(inspecting).get('points')).toBe('1');
  expect(readState(writeState(closed(inspecting))).points).toBe(true);
  expect(readState(new URLSearchParams({ signal: 'metrics' })).points).toBe(false);
  expect(readState(new URLSearchParams({ signal: 'logs', points: '1' })).points).toBe(false);
  expect(writeState({ ...inspecting, signal: 'logs' }).has('points')).toBe(false);
  // An absolute range needs both bounds; it applies over the preset, which a reset returns to.
  expect(readState(new URLSearchParams({ from: RANGE.from }))).toMatchObject({ from: undefined, to: undefined });
  const inputs = listInputs(state);
  for (const [component, input] of [['FacetRail', inputs.facets], ['RecordTable', inputs.list]] as const)
    expect(validatePanel(component, input)).not.toHaveProperty('code');
  expect(inputs.list).toEqual({
    signal: 'logs',
    where: [{ column: 'service_name', value: 'support-desk' }],
    where_sql: 'severity_number >= 17',
    columns: ['body_text', 'severity_text'],
    groupBy: 'severity_text',
    live: true
  });
  expect(inputs.facets).toMatchObject({ signal: 'logs', where_sql: 'severity_number >= 17' });
  expect(inputs.series).toBeUndefined();
  // A column the signal does not have is dropped; grouping by an unshown column also reads it.
  expect(listInputs({ ...state, columns: ['nope'], group: 'event_name' }).list).toMatchObject({ columns: expect.arrayContaining(['event_name']), groupBy: 'event_name' });
  expect(listInputs(readState(new URLSearchParams({ mode: 'spans' }))).list).toEqual({ signal: 'spans', where: [] });

  // The time picker reads now, now-2h, yesterday and ISO times in the chosen zone, and refuses anything else.
  const now = Date.parse('2026-10-20T12:30:00Z');
  expect(readInstant('now', 'utc', now)).toBe(now);
  expect(readInstant('now-2h', 'utc', now)).toBe(now - 7_200_000);
  expect(readInstant('yesterday', 'utc', now)).toBe(Date.parse('2026-10-19T00:00:00Z'));
  expect(readInstant('2026-10-13 19:02:10', 'utc', now)).toBe(Date.parse('2026-10-13T19:02:10Z'));
  expect(readInstant('2026-10-13T19:02:10+02:00', 'local', now)).toBe(Date.parse('2026-10-13T17:02:10Z'));
  expect(readInstant('2026-10-13 19:02', 'local', now)).toBe(new Date(2026, 9, 13, 19, 2).getTime());
  for (const bad of ['', 'later', 'now+1h', '2026-13-45']) expect(readInstant(bad, 'utc', now)).toBeUndefined();
  expect(presetLabel('1h')).toBe('Last 1 hour');
  expect(presetLabel('30d')).toBe('Last 30 days');
  expect(rangeLabel({ from: '2026-10-13T19:02:10Z', to: '2026-10-13T19:14:40Z' }, 'utc')).toBe('19:02:10 → 19:14:40');
  expect(rangeLabel({ from: '2026-10-13T19:02:10Z', to: 'yesterday' }, 'utc')).toBe('2026-10-13 19:02 → invalid time');

  // The list load never reads the open record's params, so opening one reloads nothing else.
  const withRecord = new URLSearchParams(writeState({ ...state, trace: 'a'.repeat(32), span: 'b'.repeat(16) }));
  const listOnly = listParams(withRecord);
  for (const name of DETAIL_PARAMS) expect(listOnly.get(name)).toBeNull();
  expect(readState(listOnly)).toEqual(closed(readState(withRecord)));

  // Intents become URL steps: filters dedupe, a brush zooms every panel, records open, queries open in Query.
  const traces = readState(new URLSearchParams());
  const facet = { name: 'facet.filter', input: { signal: 'traces', column: 'service_name', value: 'api' } };
  const filtered = step((step(traces, facet, '/') as { state: ReturnType<typeof readState> }).state, facet, '/') as { state: ReturnType<typeof readState> };
  expect(filtered.state.where).toEqual([{ column: 'service_name', value: 'api' }]);
  const excluded = step(traces, { name: 'record.filter', input: { signal: 'traces', column: 'kind', value: 2, not: true } }, '/');
  expect('state' in excluded && excluded.state.where).toEqual([{ column: 'kind', value: 2, not: true }]);
  const brushed = step({ ...traces, last: '6h' }, { name: 'chart.range', input: { from: '2026-10-20T01:00:00.000Z', to: '2026-10-20T02:00:00.000Z' } }, '/');
  expect('state' in brushed && brushed.state).toMatchObject({ last: '6h', from: '2026-10-20T01:00:00.000Z', to: '2026-10-20T02:00:00.000Z' });
  expect(readState(writeState((brushed as { state: ReturnType<typeof readState> }).state))).toEqual((brushed as { state: ReturnType<typeof readState> }).state);
  expect(zoomed(traces, Date.parse(RANGE.from), Date.parse(RANGE.to))).toMatchObject({ from: '2026-10-20T00:00:00.000Z', to: '2026-10-21T00:00:00.000Z' });
  const trace = 'a'.repeat(32);
  const opened = step(traces, { name: 'record.open', input: { trace_id: trace, span_id: 'b'.repeat(16) } }, '/') as { state: ReturnType<typeof readState> };
  expect(opened.state).toMatchObject({ trace, span: 'b'.repeat(16) });
  expect(closed(opened.state)).toEqual(traces);
  const fromLogs = step(state, { name: 'record.open', input: { trace_id: trace } }, '/') as { state: ReturnType<typeof readState> };
  expect(fromLogs.state).toMatchObject({ signal: 'traces', trace, where: [], where_sql: '', columns: undefined, last: '6h', tz: 'local' });
  const log = { time_unix_nano: '1', observed_time_unix_nano: '2', wyrd_request_id: 'r' };
  const record = step(state, { name: 'record.open', input: { log } }, '/') as { state: ReturnType<typeof readState> };
  expect(readState(writeState(record.state)).log).toEqual(log);
  const inspect = step(traces, { name: 'record.open_in_query', input: { sql: 'SELECT $1', params: ['x'] } }, '/t/acme/observe?live=1');
  expect('query' in inspect && Object.fromEntries(inspect.query)).toEqual({ sql: 'SELECT $1', params: '["x"]', back: '/t/acme/observe?live=1' });
  expect(step(traces, { name: 'record.logs', input: {} }, '/')).toHaveProperty('unavailable');

  // Metric points list integers as exact decimal text, nested counts as exact text with their structure kept.
  const points = listQuery({ signal: 'metrics', columns: ['metric_name', 'int_value', 'double_value', 'histogram_count', 'bucket_counts', 'positive_buckets'] }, RANGE).sql;
  expect(points).toMatch(/^SELECT metric_name, CAST\(int_value AS VARCHAR\) AS int_value, double_value, CAST\(histogram_count AS VARCHAR\) AS histogram_count, CAST\(bucket_counts AS VARCHAR\[\]\) AS bucket_counts, /);
  expect(points).toContain("named_struct('offset', positive_buckets['offset'], 'bucket_counts', CAST(positive_buckets['bucket_counts'] AS VARCHAR[])) END AS positive_buckets");
  // Each recorded series keeps its typed identity: meaning plus resource, scope and point dimensions, Variants as JSON text.
  const each = metricQueries({ metric: "o'clock", where: [{ column: 'unit', value: 'ms', not: true }] }, RANGE, 'service_name IS NOT NULL');
  expect(each.identity).toEqual(['metric_type', 'unit', 'aggregation_temporality', 'is_monotonic', 'resource', 'scope_name', 'scope_version', 'scope', 'attributes']);
  expect(each.chart.params).toContain("o'clock");
  expect(each.chart.sql).toContain('to_json(attributes) AS attributes');
  expect(each.chart.sql).toContain('AND (service_name IS NOT NULL)');
  expect(each.chart.sql).not.toMatch(/GROUP BY|COALESCE\(CAST\(service_name/);
  // The summary reads exact text: Max orders by value and breaks a rounded tie exactly; Mean stays decimal unless a double is present.
  expect(each.summary.sql).toContain('last_value(t ORDER BY at) AS latest, first_value(t ORDER BY v DESC, COALESCE(i, CASE WHEN abs(d) < 1e19 THEN CAST(d AS DECIMAL(38, 0)) END) DESC NULLS LAST) AS max');
  expect(each.summary.sql).toContain('CASE WHEN count(d) = 0 THEN CAST(avg(i) AS VARCHAR) ELSE CAST(avg(v) AS VARCHAR) END AS mean');
  expect(each.summary.sql).toContain('FROM (SELECT *, COALESCE(CAST(i AS DOUBLE), d) AS v');
  // An explicit aggregation applies once per instant, kept apart by `by`.
  const summed = metricQueries({ metric: 'm', aggregation: 'max', by: 'service_name' }, RANGE);
  expect(summed.identity).toEqual(['metric_type', 'unit', 'aggregation_temporality', 'is_monotonic', 'service_name']);
  expect(summed.chart.sql).toContain('CASE WHEN count(d) = 0 THEN max(i) END AS i, CASE WHEN count(d) > 0 THEN max(COALESCE(CAST(i AS DOUBLE), d)) END AS d');
  expect(() => metricQueries({ metric: 'm', where: [{ column: 'body_text', value: 'x' }] }, RANGE)).toThrow('body_text is not a metrics column');
  expect(() => metricQueries({ metric: 'm', by: 'service_name' }, RANGE)).toThrow('needs an aggregation');
  // The host plans a series only for one selected metric, with the aggregation and grouping the URL names.
  const metrics = readState(new URLSearchParams({ signal: 'metrics', where: JSON.stringify([{ column: 'metric_name', value: 'ui.requests' }]), agg: 'sum', by: 'service_name' }));
  expect(selectedMetric(metrics)).toBe('ui.requests');
  expect(groupColumns(metrics)).toContain('service_name');
  expect(listInputs(metrics).series).toEqual({ metric: 'ui.requests', where: [{ column: 'metric_name', value: 'ui.requests' }], aggregation: 'sum', by: 'service_name' });
  expect(listInputs({ ...metrics, agg: undefined, by: undefined }).series).toEqual({ metric: 'ui.requests', where: [{ column: 'metric_name', value: 'ui.requests' }] });
  expect(listInputs({ ...metrics, where: [] }).series).toBeUndefined();
});
