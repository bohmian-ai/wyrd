import { expect, test } from 'vitest';
import {
  loadExecutionCharts,
  loadFacetRail,
  loadMetric,
  loadMetricNames,
  METRIC_GROUP_KEYS,
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
  expect(aggregated.chart.sql).toContain("CAST(CASE WHEN count(vi) = count(*) THEN CASE WHEN metric_type = 'sum' THEN sum(vi) ELSE avg(vi) END END AS DECIMAL(38, 4)) AS gi, CASE WHEN metric_type = 'sum' THEN sum(v) ELSE avg(v) END AS gv");
  expect(aggregated.chart.sql).toContain("COALESCE(CAST(gi AS DOUBLE), gv) / CASE WHEN metric_type = 'sum' AND (is_monotonic OR aggregation_temporality = 1) THEN $6 ELSE 1 END AS value");
  // Integer points stay exact decimals while a window holds only integers, so increases past 2^53 are exact.
  expect(aggregated.chart.sql).toContain('CAST(int_value AS DECIMAL(38, 0)) AS i');
  expect(aggregated.chart.sql).toContain('bool_and(i IS NOT NULL AND (head OR ip IS NOT NULL)) AS exact');
  expect(aggregated.chart.sql).toContain("CASE WHEN exact THEN CASE WHEN aggregation_temporality = 1 THEN it WHEN metric_type = 'sum' AND is_monotonic THEN id ELSE i1 END END AS vi");
  // A monotonic cumulative sum adds each point's increase over the point before it, so a reset inside a window keeps the increase before it;
  // a reset counts from zero, and a series begun before the range counts from its first point.
  expect(aggregated.chart.sql).toContain('lag(t) OVER series IS NULL AS head, lag(s) OVER series AS sp, lag(x) OVER series AS xp, lag(i) OVER series AS ip FROM pts WINDOW series AS (PARTITION BY series_id ORDER BY t)');
  expect(aggregated.chart.sql).toContain('sum(CASE WHEN head THEN CASE WHEN s < $1::timestamptz THEN 0 ELSE x END WHEN s = sp AND x >= xp THEN x - xp ELSE x END) AS xd');
  // The twenty groups with the largest measure are read, each row naming how many there were; no read rebuilds a whole attribute set.
  expect(aggregated.chart.sql).toContain('dense_rank() OVER (ORDER BY measure DESC NULLS LAST, metric_type, unit, aggregation_temporality, is_monotonic) AS rank');
  expect(aggregated.chart.sql).toContain('FROM (SELECT *, max(rank) OVER () AS groups FROM ranked) WHERE rank <= 20)');
  expect(aggregated.chart.sql).toContain('COALESCE(exact, CASE WHEN abs(value) < 1e19 THEN CAST(value AS DECIMAL(38, 0)) END) AS tie');
  // Latest, Max and Mean are server text per plotted group: exact decimals for integer gauges, Max broken exactly on a rounded tie.
  expect(aggregated.chart.sql).toContain('SELECT w AS at, metric_type, unit, aggregation_temporality, is_monotonic, value, groups, last_value(text) OVER (PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic ORDER BY w ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING) AS latest');
  expect(aggregated.chart.sql).toContain('first_value(text) OVER (PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic ORDER BY value DESC NULLS LAST, tie DESC NULLS LAST) AS max');
  expect(aggregated.chart.sql).toMatch(/THEN CAST\(avg\(exact\) OVER \(PARTITION BY [^)]*\) AS VARCHAR\) ELSE CAST\(avg\(value\) OVER \(PARTITION BY [^)]*\) AS VARCHAR\) END AS mean FROM plotted ORDER BY at$/);
  expect(aggregated.chart.sql).not.toMatch(/to_json|\battributes\b(?!\))/);
  // Group keys read one key each, bound as text; a promoted column reads as itself.
  const keyed = metricQueries({ metric: 'm', aggregation: 'max', by: ['service_name', 'attributes.http.route', 'resource.host.name'] }, RANGE);
  expect(keyed.identity).toEqual(['metric_type', 'unit', 'aggregation_temporality', 'is_monotonic', 'service_name', 'attributes.http.route', 'resource.host.name']);
  expect(keyed.chart.sql).toContain('service_name AS g1, variant_get_text(attributes, $4) AS g2, variant_get_text(resource_attributes, $5) AS g3');
  expect(keyed.chart.params.slice(2, 5)).toEqual(['m', 'http.route', 'host.name']);
  expect(keyed.chart.sql).toContain('THEN max(vi) END AS DECIMAL(38, 4)) AS gi, max(v) AS gv');
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
  expect(labels.sql).toContain("to_json(attributes) AS attributes FROM (SELECT *, row_number() OVER (PARTITION BY series_id ORDER BY hour_unix_nano DESC) AS newest FROM bifrost.metrics.series WHERE hour_unix_nano >= date_bin(INTERVAL '1 hour', $1::timestamptz)");
  expect(labels.sql).not.toContain('bifrost.metrics.points');
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
  // The rail reads the series index over the hours the range overlaps, never the points.
  expect(read.sql).toContain("FROM bifrost.metrics.series WHERE hour_unix_nano >= date_bin(INTERVAL '1 hour', $1::timestamptz) AND hour_unix_nano < $2::timestamptz AND service_name = $3 AND strpos(lower(metric_name), lower($4)) > 0");
  expect(read.sql).not.toContain('bifrost.metrics.points');
  expect(read.sql).not.toContain('metric_name = ');
  expect(read.sql).toMatch(new RegExp(`LIMIT ${METRIC_NAMES + 1}$`));
  expect(read.params).toEqual([RANGE.from, RANGE.to, 'checkout', 'Duration']);
  expect(read.max_rows).toBe(METRIC_NAMES + 1);
  expect(loaded).toMatchObject({ state: 'empty', reason: 'No metric names hold “Duration”.' });
});

test('group keys are offered from at most 100 of the metric series index rows', async () => {
  const sets = { columns: ['attributes', 'resource', 'scope'].map((name) => ({ name, data_type: 'Utf8', nullable: true })), rows: [['{"http.route":"/a","code":200}', '{"host.name":"h"}', null], ['{"http.route":"/b"}', 'not json', '[1]']], terminal: EMPTY.terminal };
  const { executor, calls } = recording((request) => ({ ok: true, value: request.op === 'bifrost.query' && request.sql.includes('LIMIT 100') ? sets : EMPTY }) as never);
  const loaded = await loadMetric({ metric: 'm' }, { executor, range: RANGE });
  const sample = calls.find((call) => call.op === 'bifrost.query' && call.sql.includes('LIMIT 100'));
  expect(sample?.op === 'bifrost.query' && sample.sql).toContain("FROM bifrost.metrics.series WHERE hour_unix_nano >= date_bin(INTERVAL '1 hour', $1::timestamptz) AND hour_unix_nano < $2::timestamptz AND metric_name = $3");
  expect(loaded.keys).toEqual([...METRIC_GROUP_KEYS, 'attributes.code', 'attributes.http.route', 'resource.host.name']);
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
  const read = (rows: unknown[][], keys: string[] = []) => ({
    columns: ['at', 'metric_type', 'unit', 'aggregation_temporality', 'is_monotonic', ...keys, 'bounds', 'count', 'sum', 'buckets', 'groups'].map((name) => ({ name, data_type: 'Utf8', nullable: true })),
    rows,
    terminal: { outcome: 'success', row_count: rows.length, warnings: [] }
  }) as never;
  const meaning = ['histogram', 's', 2, null];
  const window = (at: string, count: number, sum: number | null, bounds: number[], buckets: number[], keys: unknown[] = []) => [`2026-10-20T00:${at}Z`, ...meaning, ...keys, bounds, String(count), sum, buckets.map(String), 2];
  const windows = histogramWindows(
    read([
      window('00:00', 6, 1.0, [0.5, 1], [6, 0, 0]),
      window('01:00', 2, 2.0, [0.5, 1], [0, 2, 0]),
      window('02:00', 1, 2.0, [0.5, 1], [0, 0, 1]),
      // Different bounds are another group, kept apart by its typed identity.
      window('00:00', 1, 0.3, [0.25], [0, 1])
    ]),
    range
  );
  expect(windows.identity).toEqual(['metric_type', 'unit', 'aggregation_temporality', 'is_monotonic', 'bounds', 'measure']);
  const line = (result: { columns: readonly { name: string }[]; rows: readonly (readonly unknown[])[] }, bounds: string, measure: string) => {
    const [bi, mi, vi, ti] = ['bounds', 'measure', 'value', 'text'].map((name) => result.columns.findIndex((column) => column.name === name));
    return result.rows.filter((row) => row[bi] === bounds && row[mi] === measure).map((row) => [row[0], row[vi], row[ti]]);
  };
  expect(line(windows.rate, '[0.5,1]', 'per second').map(([at, value]) => [at, value])).toEqual([
    ['2026-10-20T00:00:00.000Z', 6 / 60],
    ['2026-10-20T00:01:00.000Z', 2 / 60],
    ['2026-10-20T00:02:00.000Z', 1 / 60]
  ]);
  // The mean is the recorded sum over the count, never estimated; a percentile in the unbounded top bucket reads at least its lower bound.
  expect(line(windows.latency, '[0.5,1]', 'mean')[0]).toEqual(['2026-10-20T00:00:00.000Z', 1 / 6, String(1 / 6)]);
  const p99 = [[6, 0, 0], [0, 2, 0], [0, 0, 1]].map((buckets) => bucketPercentile([0.5, 1], buckets, 99)!.value);
  expect(line(windows.latency, '[0.5,1]', 'p99 estimated')).toEqual([
    ['2026-10-20T00:00:00.000Z', p99[0], String(p99[0])],
    ['2026-10-20T00:01:00.000Z', p99[1], String(p99[1])],
    ['2026-10-20T00:02:00.000Z', 1, '≥ 1']
  ]);
  // Each line's summary qualifies a Max or Mean over a lower bound, and keeps the mean of sums exact.
  const summaryRow = (bounds: string, measure: string) => windows.latencySummary.rows.find((row) => row[4] === bounds && row[5] === measure)?.slice(6);
  expect(summaryRow('[0.5,1]', 'p99 estimated')).toEqual(['≥ 1', '≥ 1', `≥ ${(p99[0] + p99[1] + 1) / 3}`]);
  expect(summaryRow('[0.5,1]', 'mean')).toEqual(['2', '2', String((1 / 6 + 1 + 2) / 3)]);
  expect(windows.distribution.rows).toEqual([
    ['[0.5,1] ≤ 0.5', 6],
    ['[0.5,1] 0.5–1', 2],
    ['[0.5,1] > 1', 1],
    ['[0.25] ≤ 0.25', 0],
    ['[0.25] > 0.25', 1]
  ]);
  expect(histogramWindows(read([]), range).latency.rows).toEqual([]);

  // Two groups whose key reads alike under different bounds, and an unset key beside the text "∅", stay four lines and never add up.
  const keys = ['metric_type', 'unit', 'aggregation_temporality', 'is_monotonic', 'service_name'];
  const alike = histogramWindows(
    read(
      [
        window('00:00', 2, 1, [1], [2, 0], ['api']),
        window('00:00', 2, 2, [2], [2, 0], ['api']),
        window('00:00', 2, 3, [1], [2, 0], [null]),
        window('00:00', 2, 4, [1], [2, 0], ['∅'])
      ],
      ['service_name']
    ),
    range,
    keys
  );
  const identityOf = (result: { columns: readonly { name: string }[] }, row: readonly unknown[]) => JSON.stringify(alike.identity.map((name) => row[result.columns.findIndex((column) => column.name === name)]));
  const means = alike.latency.rows.filter((row) => row.includes('mean'));
  expect(means.map((row) => row[alike.latency.columns.findIndex((column) => column.name === 'value')])).toEqual([0.5, 1, 1.5, 2]);
  expect(new Set(means.map((row) => identityOf(alike.latency, row))).size).toBe(4);
  expect(new Set(alike.distribution.rows.map(([bucket]) => bucket)).size).toBe(alike.distribution.rows.length);

  // A window whose sum was not recorded has no mean; a recorded zero sum is a zero mean; counts and estimates stay.
  const sums = histogramWindows(read([window('00:00', 4, null, [1], [4, 0]), window('01:00', 4, 0, [1], [4, 0])]), range);
  expect(line(sums.latency, '[1]', 'mean')).toEqual([
    ['2026-10-20T00:00:00.000Z', null, null],
    ['2026-10-20T00:01:00.000Z', 0, '0']
  ]);
  expect(line(sums.latency, '[1]', 'p50 estimated')).toHaveLength(2);
  expect(line(sums.rate, '[1]', 'per second')).toHaveLength(2);

  // The read keeps only explicit-bucket histogram points and folds them per series and window on the server.
  const { sql, params } = histogramQuery({ metric: 'm', by: ['service_name', 'attributes.http.route'] }, range);
  expect(params).toEqual([range.from, range.to, 'm', 'http.route', '1 minute']);
  expect(sql).toContain("metric_type = 'histogram'");
  expect(sql).toContain('date_bin($5::interval, time_unix_nano) AS w');
  // Series are told apart by series_id; group keys read one key each.
  expect(sql).toContain('service_name AS g1, variant_get_text(attributes, $4) AS g2, explicit_bounds AS bounds, series_id');
  expect(sql).toContain('WINDOW series AS (PARTITION BY series_id ORDER BY t)');
  expect(sql).not.toContain('to_json');
  // A cumulative point counts its increase over the series' previous point, or nothing when it began before the range.
  expect(sql).toContain('n - CASE WHEN head THEN CASE WHEN started < $1::timestamptz THEN n ELSE 0 END WHEN sp = started AND n >= np THEN np ELSE 0 END AS dn');
  // A window's sum is known only when every merged sum was recorded.
  expect(sql).toContain('CASE WHEN sum(kv) = sum(kn) THEN sum(dv) END AS sum');
  // Delta points count as recorded; the twenty groups that counted most are read, each row naming how many there were.
  expect(sql).toContain('FROM pts WHERE aggregation_temporality <> 2');
  expect(sql).toContain('GROUP BY w, metric_type, unit, aggregation_temporality, is_monotonic, g1, g2, bounds)');
  expect(sql).toContain('dense_rank() OVER (ORDER BY measure DESC, metric_type, unit, aggregation_temporality, is_monotonic, g1, g2, bounds) AS rank');
  expect(sql).toMatch(/max\(rank\) OVER \(\) AS groups FROM ranked\) WHERE rank <= 20 ORDER BY at$/);
  // Every series apart keys by series id instead of group keys.
  const apart = histogramQuery({ metric: 'm', apart: true, by: ['service_name'] }, range);
  expect(apart.sql).toContain("encode(series_id, 'hex') AS series, explicit_bounds AS bounds");
  expect(apart.sql).toContain('GROUP BY w, metric_type, unit, aggregation_temporality, is_monotonic, series, bounds)');
  expect(apart.sql).not.toContain('g1');
});

test('every histogram series apart is labelled from the series index, plotted ids only', async () => {
  const columns = (names: string[]) => names.map((name) => ({ name, data_type: 'Utf8', nullable: true }));
  const terminal = { outcome: 'success', row_count: 1, warnings: [] };
  const histogram = {
    columns: columns(['at', 'metric_type', 'unit', 'aggregation_temporality', 'is_monotonic', 'series', 'bounds', 'count', 'sum', 'buckets', 'groups']),
    rows: ['0a', '0b'].map((id, index) => ['2026-10-20T00:00:00Z', 'histogram', 's', 2, null, id, [1], '2', index + 1, ['2', '0'], 2]),
    terminal
  };
  const labels = { columns: columns(['series', 'resource', 'scope_name', 'scope_version', 'scope', 'attributes']), rows: [['0a', '{"service.name":"a"}', 's', null, '{}', '{}'], ['0b', '{"service.name":"b"}', 's', null, '{}', '{}']], terminal };
  const { executor, calls } = recording((request) => ({ ok: true, value: request.op !== 'bifrost.query' ? EMPTY : request.sql.includes('explicit_bounds') ? histogram : request.sql.includes('row_number()') ? labels : EMPTY }) as never);
  const loaded = await loadMetric({ metric: 'm', apart: true }, { executor, range: RANGE });
  const read = calls.find((call) => call.op === 'bifrost.query' && call.sql.includes('row_number()'));
  expect(read?.op === 'bifrost.query' && read.params?.slice(3)).toEqual(['0a', '0b']);
  expect(loaded.histogram?.latency.identity).toEqual(['metric_type', 'unit', 'aggregation_temporality', 'is_monotonic', 'resource', 'scope_name', 'scope_version', 'scope', 'attributes', 'bounds', 'measure']);
  expect(loaded.histogram?.latency.input.title).toContain('each series apart');
  const plotted = loaded.histogram?.latency.result.state === 'populated' ? loaded.histogram.latency.result.data : undefined;
  const at = (name: string) => plotted!.columns.findIndex((column) => column.name === name);
  expect(plotted?.rows.filter((row) => row[at('measure')] === 'mean').map((row) => [row[at('resource')], row[at('value')]])).toEqual([
    ['{"service.name":"a"}', 0.5],
    ['{"service.name":"b"}', 1]
  ]);
});

import { fireEvent, render, waitFor, within } from '@testing-library/svelte';
import { vi } from 'vitest';
import { goto, invalidateAll } from '$app/navigation';
import { EXECUTIONS, fixtureExecutor, SERVICE, stateExecutors, VERIFIER, VERIFIER_UIDS, verifiers } from '$lib/ui-fixtures';
import { readExecution, writeExecution } from '$lib/verify-host';
import { load as loadExecutionPage } from '../routes/t/[tenantKey]/verify/executions/[executionId]/+page.server';
import ExecutionPage from '../routes/t/[tenantKey]/verify/executions/[executionId]/+page.svelte';
import { load as loadCardDetail } from '../routes/t/[tenantKey]/cards/detail/+page.server';
import CardDetail from '../routes/t/[tenantKey]/cards/detail/+page.svelte';
import { load as loadVerify } from '../routes/t/[tenantKey]/verify/+page.server';
import VerifyPage from '../routes/t/[tenantKey]/verify/+page.svelte';

// The page's reads run through the fixture executor here; the real BFF path is the integration journey's.
const host = vi.hoisted(() => ({ executor: undefined as Executor | undefined }));
vi.mock('wyrd-ai-ui/node', () => ({ nodeExecutor: () => host.executor }));
vi.mock('$app/navigation', () => ({ goto: vi.fn(async () => {}), invalidateAll: vi.fn(async () => {}) }));

test('an exact Verifier Card page shows its definition, versions and where it runs', async () => {
  const requests: WireRequest[] = [];
  const through = (executor: Executor): Executor => ({ run: (batch, signal) => (requests.push(...batch), executor.run(batch, signal)) });
  const open = async (search: string, executor = fixtureExecutor) => {
    host.executor = through(executor);
    requests.length = 0;
    const url = new URL(`http://ui.test/t/acme/cards/detail${search}`);
    const data = await loadCardDetail({ locals: { browserSession: { client: () => ({}) } }, url, request: new Request(url) } as never);
    return render(CardDetail, { data: { ...data, tenant: { key: 'acme', name: 'Acme' } } } as never);
  };

  // The exact version: header, Graph/Files/YAML tabs, description, graph, where it runs, relationships and metadata.
  const page = await open('?kind=Verifier&name=ticket-policy&version=1.1.0');
  await waitFor(() => expect(page.container.querySelector('[data-node="step:classify"]')).toBeInTheDocument());
  expect(page.getByRole('heading', { level: 2, name: 'ticket-policy' })).toBeInTheDocument();
  expect(page.container).toHaveTextContent('Verifier / ticket-policy@1.1.0');
  const tabs = within(page.getByRole('navigation', { name: 'ticket-policy sections' }));
  expect(tabs.getAllByRole('button').map((tab) => tab.textContent)).toEqual(['Graph', 'Files', 'YAML']);
  expect(tabs.getByRole('button', { name: 'Graph' })).toHaveAttribute('aria-current', 'page');
  expect(page.container).toHaveTextContent('A support ticket follows the refund policy.');
  expect(page.container.querySelectorAll('[data-node]')).toHaveLength(9);
  expect(page.getByRole('region', { name: 'Runs on' })).toHaveTextContent('component agent · schedule 0 * * * * Europe/Berlin');
  expect(page.getByRole('region', { name: 'Relationships' })).toBeInTheDocument();
  // The newest version carries no older-version note; Results in Verify opens the list filtered to this exact Verifier version.
  expect(page.queryByText(/You are viewing/)).toBeNull();
  expect(page.getByRole('link', { name: 'Results in Verify' })).toHaveAttribute(
    'href',
    `/t/acme/verify?${new URLSearchParams({ where: JSON.stringify([{ column: 'verifier_card_uid', value: verifiers['ticket-policy@1.1.0'].metadata.uid }]) })}`
  );
  // Only exact Card reads: the header gates the rest, and nothing reads executions or telemetry.
  expect(new Set(requests.map((request) => request.op))).toEqual(new Set(['cards.get_by_ref', 'cards.list']));
  expect(requests.filter((request) => request.op === 'cards.get_by_ref').every((request) => 'version' in request.card_ref && /^\d/.test(request.card_ref.version))).toBe(true);

  // Version navigation is exact and read-only: choosing a version navigates to that version's URL.
  await fireEvent.change(page.getByRole('combobox'), { target: { value: '1.0.0' } });
  expect(goto).toHaveBeenLastCalledWith('/t/acme/cards/detail?kind=Verifier&name=ticket-policy&version=1.0.0', expect.anything());
  await fireEvent.click(tabs.getByRole('button', { name: 'YAML' }));
  expect(goto).toHaveBeenLastCalledWith('/t/acme/cards/detail?kind=Verifier&name=ticket-policy&version=1.1.0&tab=yaml', expect.anything());
  // Opening a binding owner follows its exact reference.
  await fireEvent.click(within(page.getByRole('region', { name: 'Runs on' })).getByRole('button', { name: `${SERVICE.name}@${SERVICE.version}` }));
  expect(goto).toHaveBeenLastCalledWith('/t/acme/cards/detail?kind=Service&name=support-api&version=1.0.0', expect.anything());
  page.unmount();

  // An older version says that viewing changes nothing, draws its own graph, and has no binding.
  const older = await open('?kind=Verifier&name=ticket-policy&version=1.0.0');
  await waitFor(() => expect(older.container.querySelector('[data-node="req:has-answer"]')).toBeInTheDocument());
  expect(older.getByText(/You are viewing/)).toHaveTextContent('You are viewing 1.0.0; the newest registered version is 1.1.0. Viewing a version changes nothing: each binding pins the exact version it runs.');
  expect(older.container.querySelectorAll('[data-node]')).toHaveLength(2);
  expect(older.getByRole('region', { name: 'Runs on' })).toHaveTextContent('No binding runs this exact version.');
  older.unmount();

  // The YAML tab shows the Card as returned and loads no graph.
  const yaml = await open('?kind=Verifier&name=ticket-policy&version=1.1.0&tab=yaml');
  expect(yaml.container.querySelector('pre')?.textContent).toContain('description: A support ticket follows the refund policy.');
  expect(yaml.container.querySelector('pre')?.textContent).toContain(`name: ${verifiers['ticket-policy@1.1.0'].metadata.name}`);
  expect(yaml.container.querySelector('[data-node]')).toBeNull();
  yaml.unmount();

  // A denied or missing Card shows only that outcome: no dependent read, no graph, no stale definition.
  const denied = await open('?kind=Verifier&name=ticket-policy&version=1.1.0', stateExecutors.denied);
  expect(denied.container).toHaveTextContent('WYRD_AUTH_403_FORBIDDEN');
  expect(denied.container.querySelector('[data-node]')).toBeNull();
  expect(requests).toEqual([{ op: 'cards.get_by_ref', card_ref: VERIFIER }]);
  denied.unmount();
  const missing = await open('?kind=Verifier&name=ticket-policy&version=9.9.9');
  expect(missing.container).toHaveTextContent('WYRD_REGISTRY_404_CARD_NOT_FOUND');
  expect(missing.container.querySelector('[data-node]')).toBeNull();
  expect(requests).toHaveLength(1);
});

test('an exact execution page keeps its selection in the URL and cancels once through the host', async () => {
  const requests: WireRequest[] = [];
  const open = async (path: string, executor: Executor = fixtureExecutor) => {
    host.executor = { run: (batch, signal) => (requests.push(...batch), executor.run(batch, signal)) };
    requests.length = 0;
    const url = new URL(`http://ui.test/t/acme/verify/executions/${path}`);
    const executionId = url.pathname.split('/').pop() as string;
    const data = await loadExecutionPage({ locals: { browserSession: { client: () => ({}), api: vi.fn() } }, params: { tenantKey: 'acme', executionId }, url, request: new Request(url) } as never);
    return render(ExecutionPage, { data: { ...data, tenant: { key: 'acme', name: 'Acme' } } } as never);
  };
  const here = '/t/acme/verify/executions/exec_settled';

  // The URL round-trips the exact selection: node, nullable item and iteration, one-based attempt, output, details and cursors.
  const selection = { node: 'step:polish', producer: { item_index: 2, iteration_index: null, attempt: 3 }, output: 'draft v2/notes.md', details: true, invocations: 'c+1/=', publications: 'c 2' };
  const search = writeExecution(selection);
  expect(search).toBe('?node=step%3Apolish&item=2&attempt=3&output=draft+v2%2Fnotes.md&details=1&invocations=c%2B1%2F%3D&publications=c+2');
  expect(readExecution('exec_1', new URLSearchParams(search))).toEqual({ executionId: 'exec_1', ...selection });
  // A repeat iteration is part of the producer: it round-trips, and the page reads the output of that exact iteration.
  const repeated = { node: 'step:score', producer: { item_index: null, iteration_index: 1, attempt: 2 }, output: 'units' };
  expect(writeExecution(repeated)).toBe('?node=step%3Ascore&iteration=1&attempt=2&output=units');
  expect(readExecution('exec_1', new URLSearchParams(writeExecution(repeated)))).toEqual({ executionId: 'exec_1', ...repeated });
  const iterated = await open(`exec_settled${writeExecution(repeated)}`);
  expect(requests.find((request) => request.op === 'verification.get_publication')).toEqual({ op: 'verification.get_publication', execution_id: 'exec_settled', step_id: 'score', name: 'units', attempt: 2, item_index: null, iteration_index: 1 });
  iterated.unmount();
  expect(readExecution('exec_1', new URLSearchParams())).toEqual({ executionId: 'exec_1' });
  expect(writeExecution({})).toBe('');

  // Opening the exact URL shows that execution with the URL's attempt and output selected; no list is read to get there.
  const page = await open('exec_settled?node=step%3Ascore&attempt=2&output=units');
  await waitFor(() => expect(page.container.querySelector('[data-node="step:score"]')).toHaveAttribute('aria-pressed', 'true'));
  expect(page.container).toHaveTextContent('Verify/Executions/exec_settled');
  expect(page.container.querySelector('[data-producer]')).toHaveAttribute('data-producer', 'attempt 2');
  expect(page.container.querySelector('[data-preview="units"] pre')?.textContent).toBe('{\n  "units": 3\n}');
  expect(requests.find((request) => request.op === 'verification.get_publication')).toEqual({ op: 'verification.get_publication', execution_id: 'exec_settled', step_id: 'score', name: 'units', attempt: 2, item_index: null, iteration_index: null });
  expect(requests.some((request) => request.op === 'cards.list')).toBe(false);

  // Every selection is a navigation to its own URL; opening a member, the Verifier or a file follows exact identities.
  await fireEvent.click(within(page.getByRole('radiogroup', { name: 'Invocations of score' })).getByRole('radio', { name: /attempt 1/ }));
  expect(goto).toHaveBeenLastCalledWith(`${here}?node=step%3Ascore&attempt=1`, { keepFocus: true, noScroll: true });
  await fireEvent.click(page.container.querySelector('[data-node="req:policy-followed"]') as Element);
  expect(goto).toHaveBeenLastCalledWith(`${here}?node=req%3Apolicy-followed`, expect.anything());
  await fireEvent.click(page.getByRole('button', { name: 'Load completed details' }));
  expect(goto).toHaveBeenLastCalledWith(`${here}?node=req%3Apolicy-followed&details=1`, expect.anything());
  await fireEvent.click(page.getByRole('button', { name: 'ticket-policy@1.1.0' }));
  expect(goto).toHaveBeenLastCalledWith('/t/acme/cards/detail?kind=Verifier&name=ticket-policy&version=1.1.0&uid=card_verifier_ticket-policy_1.1.0', expect.anything());

  // Refresh rereads in place; Copy link copies the address that holds the selection.
  await fireEvent.click(page.getByRole('button', { name: 'Refresh' }));
  expect(invalidateAll).toHaveBeenCalledTimes(1);
  const writeText = vi.fn(async () => {});
  Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
  await fireEvent.click(page.getByRole('button', { name: /Copy link/ }));
  expect(writeText).toHaveBeenCalledWith(window.location.href);
  await waitFor(() => expect(page.container).toHaveTextContent('Link copied'));
  page.unmount();

  // An older navigation finishing does not clear the newer one's pending state, and the newer selection stays shown.
  const settle: (() => void)[] = [];
  vi.mocked(goto).mockImplementation(() => new Promise<void>((resolve) => settle.push(resolve)));
  const racing = await open('exec_settled?node=step%3Ascore');
  await waitFor(() => expect(racing.container.querySelector('[data-node="step:score"]')).toBeInTheDocument());
  await fireEvent.click(racing.container.querySelector('[data-node="step:classify"]') as Element);
  await fireEvent.click(racing.container.querySelector('[data-node="step:summarise"]') as Element);
  settle[0]();
  await new Promise((resolve) => setTimeout(resolve));
  expect(racing.container.querySelector('[data-node="step:summarise"]')).toHaveAttribute('aria-pressed', 'true');
  expect(racing.getByRole('region', { name: 'Step summarise' })).toBeInTheDocument();
  // Until its own answer arrives, the newer selection shows no output body that was read for another one.
  expect(racing.container.querySelector('[data-preview]')).toBeNull();
  settle[1]();
  vi.mocked(goto).mockImplementation(async () => {});
  racing.unmount();

  // Session members and downloads leave through exact URLs.
  const session = await open('exec_session?node=input%3Aoutcomes');
  await fireEvent.click(await session.findByRole('button', { name: 'exec_m2' }));
  expect(goto).toHaveBeenLastCalledWith('/t/acme/verify/executions/exec_m2', { keepFocus: true, noScroll: false });
  session.unmount();
  const opened = vi.spyOn(window, 'open').mockReturnValue(null);
  const files = await open('exec_settled?node=step%3Asummarise&attempt=1&output=tree');
  await fireEvent.click(within(await files.findByRole('region', { name: 'Files of tree' })).getAllByRole('button', { name: 'Download' })[1]);
  expect(opened).toHaveBeenLastCalledWith(`${here}/download?step=summarise&name=tree&attempt=1&path=nested%2Fb.md`, '_self');
  opened.mockRestore();
  files.unmount();

  // Cancel is one host request: pending disables the control, the answer is the recorded status, and status is read back.
  const answers: ((response: Response) => void)[] = [];
  const fetched = vi.spyOn(globalThis, 'fetch').mockImplementation(() => new Promise<Response>((resolve) => answers.push(resolve)));
  vi.mocked(invalidateAll).mockClear();
  const running = await open('exec_running');
  const cancel = () => running.getByRole('button', { name: /Cancel execution/ });
  await fireEvent.click(cancel());
  await waitFor(() => expect(cancel()).toBeDisabled());
  await fireEvent.click(cancel());
  expect(fetched.mock.calls).toEqual([['/t/acme/verify/executions/exec_running/cancel', { method: 'POST' }]]);
  expect(invalidateAll).not.toHaveBeenCalled();
  // A completion that won the race answers its own terminal status; the page reports that, not "cancelled".
  answers[0](Response.json({ execution_id: 'exec_running', status: 'completed' }));
  await waitFor(() => expect(running.container).toHaveTextContent('Recorded status: completed'));
  expect(invalidateAll).toHaveBeenCalledTimes(1);
  // A refused cancel is shown as denied with the server's code, and the readable execution stays.
  await fireEvent.click(cancel());
  answers[1](Response.json({ code: 'WYRD_PERMISSION_403_DENIED_RBAC', title: 'Permission denied', status: 403 }, { status: 403 }));
  await waitFor(() => expect(running.container).toHaveTextContent('WYRD_PERMISSION_403_DENIED_RBAC'));
  expect(running.container.querySelector('[data-execution-status]')).toHaveAttribute('data-execution-status', 'running');
  expect(invalidateAll).toHaveBeenCalledTimes(2);
  fetched.mockRestore();
  running.unmount();

  // An unknown execution shows the server's problem and nothing else; a malformed selection is refused before any read.
  const unknown = await open('exec_nowhere?node=step%3Ascore&attempt=1&output=units');
  expect(unknown.container).toHaveTextContent('WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND');
  expect(unknown.container.querySelector('[data-node], [data-execution-status]')).toBeNull();
  expect(unknown.container.textContent).not.toMatch(/running/i);
  expect(requests).toEqual([{ op: 'verification.get_execution', execution_id: 'exec_nowhere' }]);
  unknown.unmount();
  const malformed = await open('exec_settled?attempt=0');
  expect(malformed.container).toHaveTextContent('WYRD_SPEC_400_VALIDATION');
  expect(requests).toEqual([]);
  const refused = await open('exec_settled', stateExecutors.denied);
  expect(refused.container).toHaveTextContent('WYRD_AUTH_403_FORBIDDEN');
  expect(requests).toHaveLength(1);
});

test('executions list, facets and charts read one population in bounded shared queries', async () => {
  const where = [{ column: 'status', value: 'completed' }, { column: 'verdict', value: null }];
  const context = () => {
    const recorded = recording(() => ({ ok: true, value: EMPTY }) as never);
    return { ...recorded, context: { executor: recorded.executor, range: RANGE } };
  };
  const sql = (request: WireRequest) => (request as { sql: string }).sql;

  // One row per accepted execution: the input record, its latest recorded state and the result that state selected.
  const list = context();
  const table = await loadRecordTable({ signal: 'executions', where }, list.context);
  expect(list.calls).toHaveLength(1);
  const population = sql(list.calls[0]).slice(sql(list.calls[0]).indexOf(' FROM ('), sql(list.calls[0]).lastIndexOf(' ORDER BY '));
  expect(population).toContain('FROM bifrost.verification.execution_inputs i');
  // A queued execution's status is its newest retained status row, and pending before any; equal times fall to the event id.
  expect(population).toContain("last_value(status ORDER BY occurred_at, event_id) AS status FROM bifrost.verification.execution_events WHERE event = 'status'");
  expect(population).toContain("ELSE coalesce(q.status, 'pending') END AS status");
  // A direct execution is never queued: complete once its result is visible, else nothing is recorded.
  expect(population).toContain("WHEN i.origin = 'direct' THEN CASE WHEN r.result_id IS NOT NULL THEN 'completed' ELSE 'not recorded' END");
  // A pull request execution's repository and number are the pins recorded with its input; no other origin has them.
  expect(population).toContain("CASE WHEN i.origin = 'pull_request' THEN i.inputs ->> 'repository_id' END AS repository");
  expect(population).toContain("CASE WHEN i.origin = 'pull_request' THEN i.inputs ->> 'number' END AS pull_request");
  // Only checkpoint rows name the selected result; status and delivery rows carry no state.
  expect(population.match(/event = 'checkpoint'/g)).toHaveLength(2);
  // The selected result is the recorded one, or a direct execution's only one; never the latest row.
  expect(population).toContain("(i.origin = 'direct' OR r.result_id = s.result_id)");
  // States and results are read from the range start on, never the tenant's whole history.
  expect(population.match(/(occurred_at|ended_at) >= \$1::timestamptz - INTERVAL '1 hour'/g)).toHaveLength(4);
  expect(population).toContain('WHERE recorded_at >= $1::timestamptz AND recorded_at < $2::timestamptz AND status = $3 AND verdict IS NULL');
  expect(table.executed?.params).toEqual([RANGE.from, RANGE.to, 'completed']);
  expect(sql(list.calls[0])).toMatch(/ORDER BY recorded_at DESC, execution_id ASC LIMIT 51$/);
  expect(table.result).toMatchObject({ state: 'empty', reason: 'No execution matches these filters in this window.' });

  // Facets and both charts read that same population and WHERE, each as one bounded aggregate.
  const rail = context();
  await loadFacetRail({ signal: 'executions', facets: ['status', 'verdict', 'verifier_card_uid'], where }, rail.context);
  const charts = context();
  const drawn = await loadExecutionCharts({ where }, charts.context);
  expect([...rail.calls, ...charts.calls]).toHaveLength(5);
  for (const request of [...rail.calls, ...charts.calls]) {
    expect(request.op).toBe('bifrost.query');
    expect(sql(request)).toContain(population.replace(/\$3/g, sql(request).includes('date_bin($3') ? '$4' : '$3'));
  }
  // No verdict is its own neutral series, never a verdict; the other chart keys each Verifier by its UID.
  expect(sql(charts.calls[0])).toContain("coalesce(verdict, 'no verdict') AS verdict, count(*) AS executions");
  expect(sql(charts.calls[1])).toContain('verifier_card_uid AS verifier, count(*) AS executions');
  expect(drawn.verdicts.input).toMatchObject({ type: 'timeseries', x: 'at', y: ['executions'], series: 'verdict', range: RANGE });
  expect(drawn.verifiers.input).toMatchObject({ series: 'verifier' });

  // A page cursor compares the recorded instant as a timestamp; a predicate is refused, because no table validates it.
  expect(listQuery({ signal: 'executions', after: ['2026-10-20T01:00:00Z', 'e1'] }, RANGE).sql).toContain(
    '(recorded_at < $3::timestamptz OR (recorded_at = $3::timestamptz AND (execution_id > $4)))'
  );
  expect(validatePanel('RecordTable', { signal: 'executions', where_sql: 'status = 1' })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect(validatePanel('FacetRail', { signal: 'executions', facets: ['status'], where_sql: 'status = 1' })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
  expect(validatePanel('RecordTable', { signal: 'executions', where: [{ column: 'inputs', value: 'x' }] })).toMatchObject({ code: 'WYRD_SPEC_400_VALIDATION' });
});

test('the Verify dashboard reads one population and keeps its investigation in the URL', async () => {
  const requests: WireRequest[] = [];
  const open = async (search: string, executor: Executor = fixtureExecutor) => {
    host.executor = { run: (batch, signal) => (requests.push(...batch), executor.run(batch, signal)) };
    requests.length = 0;
    const url = new URL(`http://ui.test/t/acme/verify${search}`);
    const data = await loadVerify({ locals: { browserSession: { client: () => ({}) } }, url, request: new Request(url), depends: () => {} } as never);
    return render(VerifyPage, { data: { ...data, tenant: { key: 'acme', name: 'Acme' } } } as never);
  };
  const search = (state: Record<string, unknown>) => `?${new URLSearchParams(Object.entries(state).map(([key, value]) => [key, typeof value === 'string' ? value : JSON.stringify(value)]))}`;
  const queries = () => requests.filter((request) => request.op === 'bifrost.query') as Extract<WireRequest, { op: 'bifrost.query' }>[];

  // The dashboard: two charts, counted facets and the execution rows, with Verifiers named from one bounded Card list.
  const page = await open('');
  await waitFor(() => expect(page.getByRole('region', { name: 'executions rows' })).toBeInTheDocument());
  expect(page.getByRole('region', { name: 'Executions by verdict' })).toHaveTextContent('no verdict');
  expect(page.getByRole('region', { name: 'Executions by Verifier' })).toHaveTextContent('ticket-policy 1.1.0');
  const rail = within(page.getByRole('navigation', { name: 'Filter executions' }));
  expect(rail.getByRole('button', { name: /no verdict/ })).toHaveTextContent('2');
  expect(rail.getByRole('button', { name: /latency-drift 1.0.0/ })).toHaveTextContent('2');
  // Repository and pull request are counted like any other recorded context.
  expect(rail.getByRole('button', { name: /^42 / })).toHaveTextContent('1');
  expect(rail.getByRole('button', { name: /^9001 / })).toHaveTextContent('1');
  const table = within(page.getByRole('region', { name: 'executions rows' }));
  expect(table.getAllByRole('row')).toHaveLength(8);
  expect(table.getAllByText('ticket-policy 1.1.0')).toHaveLength(5);
  expect(page.getByLabelText('WHERE: read-only projection of the active filters')).toHaveTextContent('select facet values to filter');
  // Bounded shared reads only: every query is one batch read of the executions population, and no row is hydrated.
  expect(new Set(requests.map((request) => request.op))).toEqual(new Set(['bifrost.query', 'cards.list']));
  expect(requests.filter((request) => request.op === 'cards.list')).toEqual([{ op: 'cards.list', kind: 'Verifier', limit: 200 }]);
  expect(queries()).toHaveLength(11);
  expect(queries().every((query) => query.sql.includes(') AS executions WHERE recorded_at >= $1::timestamptz AND recorded_at < $2::timestamptz'))).toBe(true);
  expect(new Set(queries().map((query) => JSON.stringify(query.params?.slice(0, 2)))).size).toBe(1);

  // A facet value, a brush, grouping, Live and paging are navigations: the URL holds the investigation.
  await fireEvent.click(rail.getByRole('button', { name: /failed/ }));
  expect(goto).toHaveBeenLastCalledWith(`/t/acme/verify${search({ where: [{ column: 'verdict', value: 'failed' }] })}`, { keepFocus: true, noScroll: true });
  await fireEvent.change(page.getByRole('combobox', { name: 'Group by' }), { target: { value: 'group_id' } });
  expect(goto).toHaveBeenLastCalledWith('/t/acme/verify?group=group_id', expect.anything());
  await fireEvent.click(page.getByRole('button', { name: 'Live' }));
  expect(goto).toHaveBeenLastCalledWith('/t/acme/verify?live=1', expect.anything());
  for (const title of ['Executions by verdict', 'Executions by Verifier']) {
    const plot = page.getByRole('region', { name: title }).querySelector('svg.plot') as SVGSVGElement;
    plot.getBoundingClientRect = () => ({ left: 0, width: 600, top: 0, height: 56, right: 600, bottom: 56, x: 0, y: 0, toJSON: () => ({}) });
    await fireEvent.pointerDown(plot, { clientX: 100, pointerId: 1 });
    await fireEvent.pointerMove(plot, { clientX: 400, pointerId: 1 });
    await fireEvent.pointerUp(plot, { clientX: 400, pointerId: 1 });
    const brushed = new URL(`http://ui.test${vi.mocked(goto).mock.lastCall?.[0]}`).searchParams;
    expect(Date.parse(brushed.get('to') as string)).toBeGreaterThan(Date.parse(brushed.get('from') as string));
  }
  // A row opens its exact execution page as an ordinary navigation, so Back returns to this URL and scroll position.
  await fireEvent.click(table.getAllByRole('row')[1]);
  expect(goto).toHaveBeenLastCalledWith('/t/acme/verify/executions/exec_failed');
  page.unmount();

  // Status and verdict combine: the WHERE projection names both, and rows, facets and both charts bind the same clauses.
  const where = [{ column: 'status', value: 'completed' }, { column: 'verdict', value: 'failed' }];
  const filtered = await open(search({ where, from: '2026-10-20T10:00:00.000Z', to: '2026-10-20T12:00:00.000Z', group: 'verifier_card_uid' }));
  await waitFor(() => expect(filtered.getByRole('region', { name: 'executions rows' })).toBeInTheDocument());
  expect(filtered.getByLabelText('WHERE: read-only projection of the active filters')).toHaveTextContent(`status = 'completed' AND verdict = 'failed'`);
  expect(queries()).toHaveLength(11);
  expect(queries().every((query) => query.sql.includes('status = $') && query.sql.includes('verdict = $') && query.params?.includes('completed') && query.params?.includes('failed'))).toBe(true);
  expect(queries().every((query) => query.params?.[0] === '2026-10-20T10:00:00.000Z' && query.params?.[1] === '2026-10-20T12:00:00.000Z')).toBe(true);
  // Reset zoom returns to the preset; Clear drops the filters; both keep the grouping.
  await fireEvent.click(filtered.getByRole('button', { name: 'Reset zoom' }));
  expect(goto).toHaveBeenLastCalledWith(`/t/acme/verify${search({ where, group: 'verifier_card_uid' })}`, expect.anything());
  expect(filtered.getByRole('link', { name: 'Clear' })).toHaveAttribute('href', `/t/acme/verify${search({ from: '2026-10-20T10:00:00.000Z', to: '2026-10-20T12:00:00.000Z', group: 'verifier_card_uid' })}`);
  filtered.unmount();
  // Another value adds a clause, as in Observe, and the changed list starts at its first page again.
  const paged = await open(search({ where, after: ['2026-10-20T11:58:02.412000Z', 'exec_failed'] }));
  await waitFor(() => expect(paged.getByRole('region', { name: 'executions rows' })).toBeInTheDocument());
  await fireEvent.click(within(paged.getByRole('navigation', { name: 'Filter executions' })).getByRole('button', { name: /cancelled/ }));
  expect(goto).toHaveBeenLastCalledWith(`/t/acme/verify${search({ where: [...where, { column: 'status', value: 'cancelled' }] })}`, expect.anything());
  paged.unmount();

  // An incompatible combination is an empty list, said plainly; a refusal is shown as the refusal, never as empty.
  const empty = await open(search({ where: [{ column: 'status', value: 'cancelled' }, { column: 'verdict', value: 'passed' }] }), stateExecutors.empty);
  await waitFor(() => expect(empty.container).toHaveTextContent('No execution matches these filters in this window.'));
  empty.unmount();
  const denied = await open('', stateExecutors.denied);
  await waitFor(() => expect(denied.container).toHaveTextContent('WYRD_AUTH_403_FORBIDDEN'));
  expect(denied.container).not.toHaveTextContent('No execution matches');
  // The refusals end the load: the one batch of independent reads, and nothing after it.
  expect(requests).toHaveLength(12);
  denied.unmount();
  // A malformed range or an unknown filter column issues no read.
  for (const bad of ['?from=yesterday&to=today', search({ where: [{ column: 'secret', value: 1 }] })]) {
    const refused = await open(bad);
    expect(refused.container).toHaveTextContent('WYRD_SPEC_400_VALIDATION');
    expect(requests).toEqual([]);
    refused.unmount();
  }
});
