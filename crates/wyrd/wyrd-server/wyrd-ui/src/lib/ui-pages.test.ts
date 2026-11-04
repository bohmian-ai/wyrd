import { expect, test } from 'vitest';
import {
  loadFacetRail,
  loadMetricNames,
  METRIC_NAMES,
  loadRecordTable,
  loadStatStrip,
  listQuery,
  bucketPercentile,
  histogramQuery,
  histogramWindows,
  metricQueries,
  seriesLabelsQuery,
  validatePanel,
  wyrdProblem,
  type Executor,
  type WireRequest,
  type WireResult
} from 'wyrd-ai-ui/loaders';
import { presetLabel, rangeLabel, readInstant } from './TimePicker.svelte';
import { DETAIL_PARAMS, closed, groupColumns, listInputs, listParams, moved, readState, selectedMetric, step, writeState, zoomed } from './observe-host';

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
  expect(readState(new URLSearchParams({ signal: 'nope', where: '{bad', columns: '[1]' }))).toMatchObject({ signal: 'traces', where: [], columns: undefined, by: undefined });
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
  const spanStep = step(opened.state, { name: 'record.logs', input: { trace_id: trace, span_id: 'b'.repeat(16) } }, '/') as { state: ReturnType<typeof readState> };
  expect(spanStep.state).toMatchObject({ signal: 'logs', trace: undefined, span: undefined, last: opened.state.last, where: [{ column: 'trace_id', value: trace }, { column: 'span_id', value: 'b'.repeat(16) }] });
  // Ids are binary columns, so the filter binds their hex text through decode.
  const spanLogs = listQuery({ signal: 'logs', where: [...spanStep.state.where] }, RANGE);
  expect(spanLogs.sql).toContain("trace_id = decode($3, 'hex') AND span_id = decode($4, 'hex')");
  expect(spanLogs.params.slice(2)).toEqual([trace, 'b'.repeat(16)]);
  expect(step(traces, { name: 'card.open', input: {} }, '/')).toHaveProperty('unavailable');

  // Metric points list integers as exact decimal text, nested counts as exact text with their structure kept.
  const points = listQuery({ signal: 'metrics', columns: ['metric_name', 'int_value', 'double_value', 'histogram_count', 'bucket_counts', 'positive_buckets'] }, RANGE).sql;
  expect(points).toMatch(/^SELECT metric_name, CAST\(int_value AS VARCHAR\) AS int_value, double_value, CAST\(histogram_count AS VARCHAR\) AS histogram_count, CAST\(bucket_counts AS VARCHAR\[\]\) AS bucket_counts, /);
  expect(points).toContain("named_struct('offset', positive_buckets['offset'], 'bucket_counts', CAST(positive_buckets['bucket_counts'] AS VARCHAR[])) END AS positive_buckets");
  // By default series aggregate across their meaning: per series by series_id, then sums add up and gauges average.
  const aggregated = metricQueries({ metric: "o'clock", where: [{ column: 'unit', value: 'ms', not: true }] }, RANGE, 'service_name IS NOT NULL');
  expect(aggregated.identity).toEqual(['metric_type', 'unit', 'aggregation_temporality', 'is_monotonic']);
  expect(aggregated.chart.params).toEqual([RANGE.from, RANGE.to, "o'clock", 'ms', '15 minutes', 900]);
  expect(aggregated.chart.sql).toContain('AND (service_name IS NOT NULL)');
  expect(aggregated.chart.sql).toContain('date_bin($5::interval, time_unix_nano) AS w');
  expect(aggregated.chart.sql).toContain('WINDOW series AS (PARTITION BY series_id ORDER BY w)');
  expect(aggregated.chart.sql).toContain("CASE WHEN metric_type = 'sum' THEN sum(v) ELSE avg(v) END / CASE WHEN metric_type = 'sum' AND (is_monotonic OR aggregation_temporality = 1) THEN $6 ELSE 1 END AS value");
  // A monotonic cumulative sum counts its increase; a reset counts from zero, and a series begun before the range counts from its first point.
  expect(aggregated.chart.sql).toContain('x1 - CASE WHEN xp IS NOT NULL AND sp = s1 AND x1 >= xp THEN xp WHEN xp IS NULL AND s0 = s1 AND x1 >= x0 AND s1 < $1::timestamptz THEN x0 ELSE 0 END');
  // The twenty groups with the largest measure are read, each row naming how many there were; no read rebuilds a whole attribute set.
  expect(aggregated.chart.sql).toContain('dense_rank() OVER (ORDER BY measure DESC NULLS LAST, metric_type, unit, aggregation_temporality, is_monotonic) AS rank');
  expect(aggregated.chart.sql).toMatch(/SELECT w AS at, metric_type, unit, aggregation_temporality, is_monotonic, value, groups FROM \(SELECT \*, max\(rank\) OVER \(\) AS groups FROM ranked\) WHERE rank <= 20 ORDER BY at$/);
  expect(aggregated.chart.sql).not.toMatch(/to_json|\battributes\b(?!\))/);
  // Group keys read one key each, bound as text; a promoted column reads as itself.
  const keyed = metricQueries({ metric: 'm', aggregation: 'max', by: ['service_name', 'attributes.http.route', 'resource.host.name'] }, RANGE);
  expect(keyed.identity).toEqual(['metric_type', 'unit', 'aggregation_temporality', 'is_monotonic', 'service_name', 'attributes.http.route', 'resource.host.name']);
  expect(keyed.chart.sql).toContain('service_name AS g1, variant_get_text(attributes, $4) AS g2, variant_get_text(resource_attributes, $5) AS g3');
  expect(keyed.chart.params.slice(2, 5)).toEqual(['m', 'http.route', 'host.name']);
  expect(keyed.chart.sql).toContain('max(v) / CASE WHEN');
  expect(() => metricQueries({ metric: 'm', where: [{ column: 'body_text', value: 'x' }] }, RANGE)).toThrow('body_text is not a metrics column');
  for (const key of ['body_text', 'attributes.', 'links.x', 'metric_type']) expect(() => metricQueries({ metric: 'm', by: [key] }, RANGE)).toThrow(`${key} is not a metrics group key`);
  expect(() => metricQueries({ metric: 'm', by: ['service_name', 'scope_name', 'service_version', 'run_id'] }, RANGE)).toThrow('at most 3 group keys');
  // Every series apart keys by series id and ignores group keys; only the plotted ids' attribute sets are read, newest point first.
  const apart = metricQueries({ metric: 'm', apart: true, by: ['service_name'] }, RANGE);
  expect(apart.identity).toEqual(['metric_type', 'unit', 'aggregation_temporality', 'is_monotonic', 'series']);
  expect(apart.chart.sql).toContain("encode(series_id, 'hex') AS series");
  expect(apart.chart.sql).not.toContain('g1');
  const labels = seriesLabelsQuery({ metric: 'm' }, RANGE, ['0a', '0b']);
  expect(labels.sql).toContain("series_id IN (decode($4, 'hex'), decode($5, 'hex'))");
  expect(labels.sql).toContain('to_json(attributes) AS attributes FROM (SELECT *, row_number() OVER (PARTITION BY series_id ORDER BY time_unix_nano DESC) AS newest');
  expect(labels.params).toEqual([RANGE.from, RANGE.to, 'm', '0a', '0b']);
  // The host plans a series only for one selected metric, with the aggregation, keys or every-series view the URL names.
  const metrics = readState(new URLSearchParams({ signal: 'metrics', where: JSON.stringify([{ column: 'metric_name', value: 'ui.requests' }]), agg: 'sum', by: JSON.stringify(['service_name']) }));
  expect(selectedMetric(metrics)).toBe('ui.requests');
  expect(groupColumns(metrics)).toContain('service_name');
  expect(readState(writeState(metrics))).toEqual(metrics);
  expect(listInputs(metrics).series).toEqual({ metric: 'ui.requests', where: [{ column: 'metric_name', value: 'ui.requests' }], aggregation: 'sum', by: ['service_name'] });
  expect(listInputs({ ...metrics, agg: undefined, by: undefined }).series).toEqual({ metric: 'ui.requests', where: [{ column: 'metric_name', value: 'ui.requests' }] });
  expect(listInputs({ ...metrics, apart: true }).series).toEqual({ metric: 'ui.requests', where: [{ column: 'metric_name', value: 'ui.requests' }], apart: true });
  expect(listInputs({ ...metrics, where: [] }).series).toBeUndefined();
});

test('lists sort by a column on the server and page by keyset cursor', async () => {
  // Forward past a row: beyond its service, an empty service last, else the same service and older, else the same instant and a later id.
  const after = listQuery({ signal: 'spans', sort: { column: 'service_name', desc: false }, after: ['api', '2026-10-20T01:00:00.000000001Z', 'aa', 'bb'] }, RANGE);
  expect(after.sql).toContain(
    "WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz AND " +
      "(service_name > $3 OR service_name IS NULL OR (service_name = $3 AND (start_time_unix_nano < $4::timestamptz OR " +
      "(start_time_unix_nano = $4::timestamptz AND (encode(trace_id, 'hex') > $5 OR (encode(trace_id, 'hex') = $5 AND (encode(span_id, 'hex') > $6))))))) " +
      "ORDER BY service_name ASC NULLS LAST, start_time_unix_nano DESC, encode(trace_id, 'hex') ASC, encode(span_id, 'hex') ASC LIMIT 51"
  );
  expect(after.params).toEqual([RANGE.from, RANGE.to, 'api', '2026-10-20T01:00:00.000000001Z', 'aa', 'bb']);
  // Backward before an empty value: every non-empty value precedes it, and the order reverses with empty values first.
  const before = listQuery({ signal: 'spans', sort: { column: 'service_name', desc: false }, before: [null, '2026-10-20T01:00:00Z', 'aa', 'bb'] }, RANGE);
  expect(before.sql).toContain('(service_name IS NOT NULL OR (service_name IS NULL AND (start_time_unix_nano > $3::timestamptz OR');
  expect(before.sql).toContain("ORDER BY service_name DESC NULLS FIRST, start_time_unix_nano ASC, encode(trace_id, 'hex') DESC, encode(span_id, 'hex') DESC LIMIT 51");
  // A list whose columns omit the cursor columns still reads them.
  expect(listQuery({ signal: 'logs', columns: ['body_text'], sort: { column: 'severity_number', desc: true } }, RANGE).sql).toMatch(
    /^SELECT body_text, encode\(trace_id, 'hex'\) AS trace_id, encode\(span_id, 'hex'\) AS span_id, observed_time_unix_nano, wyrd_request_id, severity_number, time_unix_nano FROM/
  );
  // Ids and counts are not sortable, a cursor matches its sort, and metric points do not page.
  expect(validatePanel('RecordTable', { signal: 'traces', sort: { column: 'trace_id', desc: true } })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect(validatePanel('RecordTable', { signal: 'traces', after: ['2026-10-20T01:00:00Z', 'aa'] })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect(validatePanel('RecordTable', { signal: 'metrics', after: ['2026-10-20T01:00:00Z'] })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect(validatePanel('RecordTable', { signal: 'traces', sort: { column: 'duration_nano', desc: true }, after: [5, '2026-10-20T01:00:00Z', 'aa', 'bb'] })).not.toHaveProperty('code');

  // One row past the limit says an older page exists; its cursor is the last row shown.
  const columns = ['start_time_unix_nano', 'name', 'service_name', 'duration_nano', 'status_code', 'trace_id', 'span_id'].map((name) => ({ name, data_type: 'Utf8', nullable: true }));
  const rows = (n: number) => ({ ...EMPTY, columns, rows: Array.from({ length: n }, (_, at) => [`t${at}`, 'n', 's', at, 0, `trace${at}`, `span${at}`]) });
  const answer = (n: number) => (request: WireRequest): WireResult => ({ ok: true, value: (request as { sql: string }).sql.includes('AS spans') ? EMPTY : rows(n) }) as never;
  const first = await loadRecordTable({ signal: 'spans', limit: 2 }, { executor: recording(answer(3)).executor, range: RANGE });
  expect(first.result).toMatchObject({ state: 'populated', data: { page: { older: ['t1', 'trace1', 'span1'], newer: undefined } } });
  expect((first.result as { data: { rows: { rows: readonly unknown[] } } }).data.rows.rows).toHaveLength(2);
  // A page read backward shows in list order; at the start it reads the first page again forward.
  const back = await loadRecordTable({ signal: 'spans', limit: 2, before: ['t9', 'x', 'y'] }, { executor: recording(answer(3)).executor, range: RANGE });
  expect(back.result).toMatchObject({ data: { rows: { rows: [['t1'], ['t0']].map(([t]) => expect.arrayContaining([t])) }, page: { older: ['t0', 'trace0', 'span0'], newer: ['t1', 'trace1', 'span1'] } } });
  const start = recording(answer(1));
  const restarted = await loadRecordTable({ signal: 'spans', limit: 2, before: ['t9', 'x', 'y'] }, { executor: start.executor, range: RANGE });
  expect(start.calls).toHaveLength(2);
  expect((start.calls[1] as { sql: string }).sql).toContain('ORDER BY start_time_unix_nano DESC');
  expect(restarted.result).toMatchObject({ data: { page: { older: undefined, newer: undefined } } });

  // The sort and cursor round-trip through the URL; sorting and another list return to the first page, opening a record keeps it.
  const paged = readState(new URLSearchParams({ sort: 'duration_nano:desc', after: JSON.stringify([5, 't', 'a', 'b']) }));
  expect(paged).toMatchObject({ sort: { column: 'duration_nano', desc: true }, page: { after: [5, 't', 'a', 'b'] } });
  expect(readState(writeState(paged))).toEqual(paged);
  expect(listInputs(paged).list).toMatchObject({ sort: { column: 'duration_nano', desc: true }, after: [5, 't', 'a', 'b'] });
  expect(listInputs({ ...paged, sort: { column: 'trace_id', desc: true } }).list).not.toHaveProperty('after');
  const resorted = step(paged, { name: 'record.sort', input: { column: 'name', desc: false } }, '/');
  expect(resorted).toEqual({ state: { ...paged, sort: { column: 'name', desc: false }, page: undefined } });
  expect(step(paged, { name: 'record.page', input: { before: [1, 't', 'a', 'b'] } }, '/')).toEqual({ state: { ...paged, page: { before: [1, 't', 'a', 'b'] } } });
  expect(moved(paged, { ...paged, where: [{ column: 'name', value: 'x' }] }).page).toBeUndefined();
  expect(moved(paged, { ...paged, trace: 'aa' }).page).toEqual(paged.page);
});

test('the metric rail searches names on the server and reads at most one past its cap', async () => {
  const { executor, calls } = recording(normalizing(''));
  const loaded = await loadMetricNames({ where: [{ column: 'service_name', value: 'checkout' }, { column: 'metric_name', value: 'old' }], search: '  Duration ' }, { executor, range: RANGE });
  const [read] = calls;
  expect(read.op).toBe('bifrost.query');
  if (read.op !== 'bifrost.query') return;
  expect(read.sql).toContain('service_name = $3 AND strpos(lower(metric_name), lower($4)) > 0');
  expect(read.sql).not.toContain('metric_name = ');
  expect(read.sql).toMatch(new RegExp(`LIMIT ${METRIC_NAMES + 1}$`));
  expect(read.params).toEqual([RANGE.from, RANGE.to, 'checkout', 'Duration']);
  expect(read.max_rows).toBe(METRIC_NAMES + 1);
  expect(loaded).toMatchObject({ state: 'empty', reason: 'No metric names hold “Duration”.' });
});

test('histogram percentiles interpolate inside the bucket holding the rank', () => {
  // Bounds 0.1, 0.5, 1 and an unbounded top bucket.
  const bounds = [0.1, 0.5, 1];
  // 10 counts: rank 5 of p50 falls in the second bucket, halfway through its 4 counts after 3 below.
  expect(bucketPercentile(bounds, [3, 4, 2, 1], 50)).toEqual({ value: 0.1 + 0.4 * (2 / 4), atLeast: false });
  // The first bucket starts at 0.
  expect(bucketPercentile(bounds, [10, 0, 0, 0], 50)).toEqual({ value: 0.05, atLeast: false });
  // A rank in the unbounded bucket reports its lower bound, at least.
  expect(bucketPercentile(bounds, [0, 0, 1, 9], 99)).toEqual({ value: 1, atLeast: true });
  expect(bucketPercentile([], [5], 50)).toBeUndefined();
  expect(bucketPercentile(bounds, [0, 0, 0, 0], 50)).toBeUndefined();
});

test('histograms fold on the server into the chart windows and shape into plots per group', () => {
  const range = { from: '2026-10-20T00:00:00Z', to: '2026-10-20T01:00:00Z' };
  const read = (rows: unknown[][]) => ({
    columns: ['at', 'metric_type', 'unit', 'aggregation_temporality', 'is_monotonic', 'bounds', 'count', 'sum', 'buckets'].map((name) => ({ name, data_type: 'Utf8', nullable: true })),
    rows,
    terminal: { outcome: 'success', row_count: rows.length, warnings: [] }
  }) as never;
  const window = (at: string, count: number, sum: number, bounds: number[], buckets: number[]) => [`2026-10-20T00:${at}Z`, 'histogram', 's', 2, null, bounds, String(count), sum, buckets.map(String)];
  const windows = histogramWindows(
    read([
      window('00:00', 6, 1.0, [0.5, 1], [6, 0, 0]),
      window('01:00', 2, 2.0, [0.5, 1], [0, 2, 0]),
      window('02:00', 1, 2.0, [0.5, 1], [0, 0, 1]),
      // Different bounds are another group, named apart.
      window('00:00', 1, 0.3, [0.25], [0, 1])
    ]),
    range
  );
  const rows = (result: { rows: readonly (readonly unknown[])[] }, series: string) => result.rows.filter((row) => row[1] === series).map((row) => [row[0], row[2]]);
  expect(rows(windows.rate, 'per second · bounds 1')).toEqual([
    ['2026-10-20T00:00:00.000Z', 6 / 60],
    ['2026-10-20T00:01:00.000Z', 2 / 60],
    ['2026-10-20T00:02:00.000Z', 1 / 60]
  ]);
  expect(rows(windows.latency, 'mean · bounds 1')[0]).toEqual(['2026-10-20T00:00:00.000Z', 1 / 6]);
  expect(rows(windows.latency, 'p99 estimated · bounds 1').at(-1)).toEqual(['2026-10-20T00:02:00.000Z', 1]);
  expect(windows.atLeast).toBe(true);
  expect(windows.distribution.rows).toEqual([
    ['bounds 1 ≤ 0.5', 6],
    ['bounds 1 0.5–1', 2],
    ['bounds 1 > 1', 1],
    ['bounds 2 ≤ 0.25', 0],
    ['bounds 2 > 0.25', 1]
  ]);
  expect(histogramWindows(read([]), range).latency.rows).toEqual([]);
  // The read keeps only explicit-bucket histogram points and folds them per series and window on the server.
  const { sql, params } = histogramQuery({ metric: 'm', by: ['service_name', 'attributes.http.route'] }, range);
  expect(params).toEqual([range.from, range.to, 'm', 'http.route', '1 minute']);
  expect(sql).toContain("metric_type = 'histogram'");
  expect(sql).toContain('date_bin($5::interval, time_unix_nano) AS w');
  // Series are told apart by series_id; group keys read one key each.
  expect(sql).toContain('service_name AS g1, variant_get_text(attributes, $4) AS g2, explicit_bounds AS bounds, series_id');
  expect(sql).toContain('PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic, g1, g2, bounds, series_id ORDER BY w');
  expect(sql).not.toContain('to_json');
  // A cumulative series counts its increase over its previous window, or over its first point when it started before the range.
  expect(sql).toContain('lag(n1) OVER series AS np');
  expect(sql).toContain('CASE WHEN np IS NOT NULL AND sp = s1 AND n1 >= np THEN np WHEN np IS NULL AND s0 = s1 AND n1 >= n0 AND s1 < $1::timestamptz THEN n0 ELSE 0 END AS nb');
  // Delta points count as recorded; groups are recorded meaning, the grouping value and bounds, at most twenty.
  expect(sql).toContain('FROM pts WHERE aggregation_temporality <> 2');
  expect(sql).toContain('GROUP BY w, metric_type, unit, aggregation_temporality, is_monotonic, g1, g2, bounds)');
  expect(sql).toContain('WHERE rank <= 20 AND count > 0 ORDER BY at');
});
