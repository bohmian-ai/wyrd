import { expect, test } from 'vitest';
import {
  loadExecutionDiscovery,
  NAMED_VERIFIERS,
  loadFacetRail,
  loadMetric,
  loadMetricKeys,
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
  // Panels of one refresh share its context, so the predicate is validated once for all of them.
  const spans = recording(normalizing('http_response_status_code = 404'));
  const refresh = { executor: spans.executor, range: RANGE };
  await Promise.all([
    loadFacetRail({ signal: 'traces', facets: ['name', 'service_name'], where_sql: 'http_response_status_code=404' }, refresh),
    loadStatStrip({ stats: ['traces', 'errors'], where_sql: 'http_response_status_code=404' }, refresh)
  ]);
  const roots = await loadRecordTable({ signal: 'traces', where_sql: 'http_response_status_code=404' }, refresh);
  const queries = spans.calls.filter((call) => call.op === 'bifrost.query');
  expect(spans.calls.filter((call) => call.op === 'bifrost.validate_predicate')).toHaveLength(1);
  expect(queries).toHaveLength(1 + 2 + 1);
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
  // Both stats count the same records, so one read answers them; errors are that read's FILTER.
  expect(logReads).toHaveLength(1);
  for (const call of logReads) expect(call.op === 'bifrost.query' && call.sql).toMatch(/FROM bifrost\.logs\.records WHERE .*severity_text = \$\d.*\(body_text LIKE '%refund%'\)/);
  expect(logReads[0].op === 'bifrost.query' && logReads[0].sql).toContain('count(*) AS s0, count(*) FILTER (WHERE severity_number >= 17) AS s1, count(*) FILTER (WHERE severity_number >= 17) AS m1 FROM');
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
  const sampled = (call: (typeof calls)[number]) => call.op === 'bifrost.query' && call.sql.includes('LIMIT 100');
  // Drawing a metric never samples its keys; the picker asks for them when the reader opens it.
  await loadMetric({ metric: 'm' }, { executor, range: RANGE });
  expect(calls.filter(sampled)).toEqual([]);
  const offered = await loadMetricKeys({ metric: 'm' }, { executor, range: RANGE });
  const [sample] = calls.filter(sampled);
  expect(calls.filter(sampled)).toHaveLength(1);
  expect(sample?.op === 'bifrost.query' && sample.sql).toContain("FROM bifrost.metrics.series WHERE hour_unix_nano >= date_bin(INTERVAL '1 hour', $1::timestamptz) AND hour_unix_nano < $2::timestamptz AND metric_name = $3");
  expect(offered).toEqual({ state: 'populated', data: [...METRIC_GROUP_KEYS, 'attributes.code', 'attributes.http.route', 'resource.host.name'] });
  // A failed sample offers the promoted columns alone; any key can still be typed.
  const failing = recording((request) => (request.op === 'bifrost.query' ? { ok: false, problem: { code: 'WYRD_VALA_500_QUERY_FAILED', title: 'Query failed', status: 500 } } : { ok: true, value: EMPTY }) as never);
  expect(await loadMetricKeys({ metric: 'm' }, { executor: failing.executor, range: RANGE })).toEqual({ state: 'populated', data: METRIC_GROUP_KEYS });
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
import { goto, invalidate, invalidateAll } from '$app/navigation';
import { EXECUTIONS, fixtureExecutor, PROMPT, SERVICE, stateExecutors, VERIFIER, VERIFIER_UIDS, verifiers } from '$lib/ui-fixtures';
import { readExecution, verifyInputs, verifyStep, writeExecution } from '$lib/verify-host';
import { load as loadExecutionPage } from '../routes/t/[tenantKey]/verify/executions/[executionId]/+page.server';
import ExecutionPage from '../routes/t/[tenantKey]/verify/executions/[executionId]/+page.svelte';
import { load as loadCardDetail } from '../routes/t/[tenantKey]/cards/detail/+page.server';
import CardDetail from '../routes/t/[tenantKey]/cards/detail/+page.svelte';
import { load as loadVerify } from '../routes/t/[tenantKey]/verify/+page.server';
import VerifyPage from '../routes/t/[tenantKey]/verify/+page.svelte';
import { load as loadObserve } from '../routes/t/[tenantKey]/observe/+layout.server';

// The page's reads run through the fixture executor here; the real BFF path is the integration journey's.
const host = vi.hoisted(() => ({ executor: undefined as Executor | undefined }));
vi.mock('wyrd-ai-ui/node', () => ({ nodeExecutor: () => host.executor }));
vi.mock('$app/navigation', () => ({ goto: vi.fn(async () => {}), invalidate: vi.fn(async () => {}), invalidateAll: vi.fn(async () => {}) }));

test('a Card page reads its Card once and lists its versions once, whichever panels its tab shows', async () => {
  const requests: WireRequest[] = [];
  host.executor = { run: (batch, signal) => (requests.push(...batch), fixtureExecutor.run(batch, signal)) };
  const open = async (ref: { kind: string; name: string; version: string }) => {
    requests.length = 0;
    const url = new URL(`http://ui.test/t/acme/cards/detail?${new URLSearchParams(ref)}`);
    await loadCardDetail({ locals: { browserSession: { client: () => ({}) } }, url, request: new Request(url) } as never);
    const own = (request: WireRequest) => 'card_ref' in request && request.card_ref.name === ref.name && request.card_ref.version === ref.version;
    return {
      cardReads: requests.filter((request) => request.op === 'cards.get_by_ref' && own(request)).length,
      versionListings: requests.filter((request) => request.op === 'cards.list' && request.name === ref.name).length
    };
  };
  for (const ref of [SERVICE, PROMPT, { kind: 'Verifier', name: 'ticket-policy', version: '1.1.0' }]) {
    expect(await open(ref), `${ref.kind} ${ref.name}@${ref.version}`).toEqual({ cardReads: 1, versionListings: 1 });
  }
});

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


/** A copy of loaded execution page data in which the server has recorded `decision` for occurrence `approvalId`. */
function withRecordedDecision<T>(data: T, approvalId: string, decision: { status: 'approved' | 'rejected'; actor_id: string; decided_at: string }): T {
  const copy = structuredClone(data) as { execution: { result: { data: { status: { approvals: { approval_id: string }[] } } } } };
  const occurrence = copy.execution.result.data.status.approvals.find((each) => each.approval_id === approvalId);
  if (!occurrence) throw new Error(`no occurrence ${approvalId} in the loaded page`);
  Object.assign(occurrence, decision);
  return copy as T;
}

test('TASK-006: an execution page decides one exact approval occurrence once and reads it back', async () => {
  // Setup: the page shows `exec_approval` on its `release` step, whose occurrence `apo_release_0` is pending.
  host.executor = fixtureExecutor;
  const url = new URL('http://ui.test/t/acme/verify/executions/exec_approval?node=step%3Arelease');
  const loaded = { ...(await loadExecutionPage({ locals: { browserSession: { client: () => ({}), api: vi.fn() } }, params: { tenantKey: 'acme', executionId: 'exec_approval' }, url, request: new Request(url) } as never)), tenant: { key: 'acme', name: 'Acme' } };
  const page = render(ExecutionPage, { data: loaded } as never);
  const here = '/t/acme/verify/executions/exec_approval';
  const occurrence = () => page.container.querySelector('[data-approval="apo_release_0"]') as HTMLElement;
  const approve = () => within(occurrence()).getByRole('button', { name: /Approve/ });
  const answers: ((response: Response | Error) => void)[] = [];
  const fetched = vi.spyOn(globalThis, 'fetch').mockImplementation(() => new Promise<Response>((resolve, reject) => answers.push((answer) => (answer instanceof Error ? reject(answer) : resolve(answer)))));
  vi.mocked(invalidateAll).mockClear();

  // Approve with a reason is one host request for that exact occurrence; a second click while it is pending sends nothing.
  await fireEvent.input(within(occurrence()).getByRole('textbox', { name: /Reason/ }), { target: { value: 'metrics hold' } });
  await fireEvent.click(approve());
  await waitFor(() => expect(approve()).toBeDisabled());
  await fireEvent.click(approve());
  const sent = { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ decision: 'approved', reason: 'metrics hold' }) };
  expect(fetched.mock.calls).toEqual([[`${here}/approvals/apo_release_0`, sent]]);
  expect(occurrence()).toHaveTextContent('Submitting for apo_release_0…');

  // The durable receipt is the mock's page notice for that occurrence, and the status is read again; it claims no finished continuation.
  const notice = () => page.container.querySelector('[data-decision-notice]');
  answers[0](Response.json({ approval_id: 'apo_release_0', status: 'approved', actor_id: 'pr_me', decided_at: '2026-10-20T11:50:00Z', reason: 'metrics hold' }));
  await waitFor(() => expect(notice()).toHaveTextContent('Accepted by the server: approved apo_release_0 (decided by pr_me at 2026-10-20T11:50:00Z). The same execution continues; no new work was started.'));
  expect(invalidateAll).toHaveBeenCalledTimes(1);

  // A conflict the reread cannot explain is the server's stable problem, and the readable occurrence stays.
  await fireEvent.click(approve());
  answers[1](Response.json({ code: 'WYRD_VERIFICATION_409_APPROVAL_NOT_PENDING', title: 'The approval is no longer pending', status: 409 }, { status: 409 }));
  await waitFor(() => expect(notice()).toHaveTextContent('WYRD_VERIFICATION_409_APPROVAL_NOT_PENDING'));
  expect(occurrence().querySelector('[data-approval-status]')).toHaveTextContent('pending');
  expect(invalidateAll).toHaveBeenCalledTimes(2);

  // A lost answer is resolved by reading that occurrence back: still pending, it asks for an explicit retry and sends nothing itself.
  await fireEvent.click(approve());
  answers[2](new TypeError('network'));
  await waitFor(() => expect(notice()).toHaveTextContent('No decision is recorded for apo_release_0; submit it again to retry.'));
  expect(fetched).toHaveBeenCalledTimes(3);
  // When the status cannot be read again, the outcome is unknown rather than pending, and the control is not left waiting.
  vi.mocked(invalidateAll).mockRejectedValueOnce(new Error('offline'));
  await fireEvent.click(approve());
  answers[3](new TypeError('network'));
  await waitFor(() => expect(notice()).toHaveTextContent('Whether apo_release_0 was decided is unknown: its status could not be read. Refresh before deciding again.'));
  expect(approve()).toBeEnabled();
  expect(fetched).toHaveBeenCalledTimes(4);
  // The retry is the same decision and reason, sent only when asked; this time the server recorded it before its answer was lost.
  const decided = withRecordedDecision(loaded, 'apo_release_0', { status: 'approved', actor_id: 'pr_me', decided_at: '2026-10-20T11:52:00Z' });
  vi.mocked(invalidateAll).mockImplementationOnce(async () => void (await page.rerender({ data: decided } as never)));
  await fireEvent.click(approve());
  expect(fetched.mock.calls[4]).toEqual([`${here}/approvals/apo_release_0`, sent]);
  answers[4](Response.json({ code: 'WYRD_SERVICE_503_UNAVAILABLE', title: 'Unavailable', status: 503 }, { status: 503 }));
  await waitFor(() => expect(notice()).toHaveTextContent('Refreshed: the answer was lost; the server records approved apo_release_0 by pr_me at 2026-10-20T11:52:00Z.'));
  expect(occurrence().querySelector('[data-approval-status]')).toHaveTextContent('approved');
  expect(within(occurrence()).queryByRole('button', { name: /Approve/ })).toBeNull();
  expect(fetched).toHaveBeenCalledTimes(5);

  // Someone else decided first: the reread explains the conflict as the mock's notice does, and nothing is applied.
  await page.rerender({ data: loaded } as never);
  const otherFirst = withRecordedDecision(loaded, 'apo_release_0', { status: 'rejected', actor_id: 'pr_bo', decided_at: '2026-10-20T11:49:00Z' });
  vi.mocked(invalidateAll).mockImplementationOnce(async () => void (await page.rerender({ data: otherFirst } as never)));
  await fireEvent.click(approve());
  answers[5](Response.json({ code: 'WYRD_VERIFICATION_409_APPROVAL_NOT_PENDING', title: 'The approval is no longer pending', status: 409 }, { status: 409 }));
  await waitFor(() => expect(notice()).toHaveTextContent('Refreshed: pr_bo already rejected apo_release_0. Your approval was not applied.'));
  expect(occurrence().querySelector('[data-approval-status]')).toHaveTextContent('rejected');

  // A refusal is the server's stable problem; the occurrence stays readable and offers no decision, as the mock's viewer sees it.
  await page.rerender({ data: loaded } as never);
  await fireEvent.click(approve());
  answers[6](Response.json({ code: 'WYRD_PERMISSION_403_DENIED_RBAC', title: 'Permission denied', status: 403 }, { status: 403 }));
  await waitFor(() => expect(notice()).toHaveTextContent('WYRD_PERMISSION_403_DENIED_RBAC'));
  expect(occurrence().querySelector('[data-approval-status]')).toHaveTextContent('pending');
  expect(occurrence().querySelector('[data-viewer-note]')).toHaveTextContent('needs approve on this Verifier and read on its Repository');
  expect(within(occurrence()).queryByRole('button', { name: /Approve/ })).toBeNull();
  expect(fetched).toHaveBeenCalledTimes(7);
  fetched.mockRestore();
  page.unmount();
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
  // Only checkpoint rows name the selected result; status and delivery rows carry no state.
  expect(population.match(/event = 'checkpoint'/g)).toHaveLength(2);
  // The selected result is the recorded one, or a direct execution's only one; never the latest row.
  expect(population).toContain("(i.origin = 'direct' OR r.result_id = s.result_id)");
  // States and results are read from the range start on, never the tenant's whole history.
  expect(population.match(/(occurred_at|ended_at) >= \$1::timestamptz - INTERVAL '1 hour'/g)).toHaveLength(4);
  expect(population).toContain('WHERE recorded_at >= $1::timestamptz AND recorded_at < $2::timestamptz AND status = $3 AND verdict IS NULL');
  expect(table.executed?.params).toEqual([RANGE.from, RANGE.to, 'completed']);
  expect(sql(list.calls[0])).toMatch(/ORDER BY recorded_at DESC, execution_id ASC LIMIT 51$/);
  // Each row carries its exact identities and correlations, shown or not: the subject Card, the selected result, the binding, the Workload and its review set.
  expect(sql(list.calls[0])).toMatch(/^SELECT .*, execution_id, subject_kind, subject_version, subject_card_uid, result_id, binding_id, workload_card_uid, review_set FROM /);
  expect(table.result).toMatchObject({ state: 'empty', reason: 'No execution matches these filters in this window.' });

  // Facets and both charts read that same population and WHERE in one aggregate; Verifiers are ranked by the server.
  const column = (name: string, data_type = 'Utf8') => ({ name, data_type, nullable: true });
  const discovered = recording(() => ({
    ok: true,
    value: {
      columns: [column('facet', 'Int64'), column('f0'), column('f1'), column('f2'), column('at', 'Timestamp(ns, "+00:00")'), column('verdict'), column('verifier'), column('place', 'UInt64'), column('verifiers', 'UInt64'), column('n', 'Int64')],
      rows: [
        [0, 'completed', null, null, null, null, null, null, null, 4],
        [1, null, null, null, null, null, null, null, null, 4],
        [2, null, null, 'v1', null, null, null, null, null, 3],
        [3, null, null, null, 'T1', 'no verdict', null, null, null, 4],
        [4, null, null, null, 'T1', null, 'v1', 1, 5, 3],
        [4, null, null, null, 'T1', null, 'v2', 2, 5, 1],
        [4, null, null, null, 'T1', null, 'v3', 3, 5, 1],
        [4, null, null, null, 'T1', null, null, null, 5, 2]
      ],
      terminal: { outcome: 'success', row_count: 8, warnings: [] }
    } as never
  }));
  const drawn = await loadExecutionDiscovery({ signal: 'executions', facets: ['status', 'verdict', 'verifier_card_uid'], where }, { executor: discovered.executor, range: RANGE });
  expect(discovered.calls).toHaveLength(1);
  const combined = sql(discovered.calls[0]);
  expect(combined).toContain(population.replace(/\$3/g, '$4'));
  expect(combined).toContain('GROUP BY GROUPING SETS ((status), (verdict), (verifier_card_uid), (at, verdict_series), (at, verifier))');
  // Each stage is a named sibling that reads the one before it, so the read nests no deeper than the population.
  expect(combined).toMatch(
    /^WITH population AS \(SELECT .+\), grouped AS \(SELECT .+ FROM population GROUP BY .+\), ranked AS \(SELECT .+ FROM grouped\), placed AS \(SELECT .+ FROM ranked\), counted AS \(SELECT .+ FROM placed\), kept AS \(SELECT .+ FROM counted WHERE .+\) SELECT .+ FROM kept GROUP BY /
  );
  // No verdict is its own neutral series, never a verdict; the Verifier chart keys each of its three largest Verifiers by UID and counts the rest together.
  expect(combined).toContain("coalesce(verdict, 'no verdict') AS verdict_series");
  expect(combined).toContain('dense_rank() OVER (PARTITION BY facet ORDER BY total DESC, verifier) AS place');
  expect(combined).toContain(`CASE WHEN facet = 4 AND place <= ${NAMED_VERIFIERS} THEN verifier END AS verifier`);
  expect((discovered.calls[0] as Extract<WireRequest, { op: 'bifrost.query' }>).params).toEqual([RANGE.from, RANGE.to, '15 minutes', 'completed']);
  expect(drawn.facets.result).toMatchObject({
    state: 'populated',
    data: { facets: [{ column: 'status', values: { rows: [['completed', 4]] } }, { column: 'verdict', values: { rows: [[null, 4]] } }, { column: 'verifier_card_uid', values: { rows: [['v1', 3]] } }] }
  });
  expect(drawn.charts.verdicts.input).toMatchObject({ type: 'timeseries', x: 'at', y: ['executions'], series: 'verdict', range: RANGE });
  expect(drawn.charts.verdicts.result).toMatchObject({ state: 'populated', data: { rows: [['T1', 'no verdict', 4]] } });
  expect(drawn.charts.verifiers.input).toMatchObject({ series: 'verifier' });
  // Each chart keeps its own portable SQL for a Chart panel or Query; the dashboard records the one read it executed.
  expect(drawn.charts.verdicts.input.sql).toContain("coalesce(verdict, 'no verdict') AS verdict, count(*) AS executions");
  expect(drawn.charts.verifiers.input.sql).toContain('verifier_card_uid AS verifier, count(*) AS executions');
  expect(drawn.charts.verdicts.executed?.sql).toBe(combined);
  expect(drawn.charts.verifiers.result).toMatchObject({ state: 'populated', data: { rows: [['T1', 'v1', 3], ['T1', 'v2', 1], ['T1', 'v3', 1], ['T1', null, 2]] } });
  expect(drawn.charts.verifierSeries).toEqual({ top: ['v1', 'v2', 'v3'], more: 2 });

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
  expect(page.getByRole('region', { name: 'Executions' })).toHaveTextContent('no verdict');
  // Each chart states its total; the Verifier chart names its largest Verifiers by name.
  expect(page.getByRole('region', { name: 'By Verifier' })).toHaveTextContent('By Verifier 14 ticket-policylatency-drift');
  const rail = within(page.getByRole('navigation', { name: 'Filter executions' }));
  expect(rail.getByRole('button', { name: /no recorded verdict/ })).toHaveTextContent('2');
  expect(rail.getByRole('button', { name: /latency-drift@1.0.0/ })).toHaveTextContent('2');
  // The rail counts the facets of the approved design, in its order.
  expect(rail.getAllByRole('heading').map((heading) => heading.textContent?.trim())).toEqual(['Status', 'Verdict', 'Verifier', 'Service', 'Card', 'Run', 'Session', 'Pull request']);
  // A Verifier value links to its exact definition.
  expect(rail.getByRole('link', { name: 'Open Verifier latency-drift@1.0.0' })).toHaveAttribute('href', '/t/acme/cards/detail?kind=Verifier&name=latency-drift&version=1.0.0');
  const table = within(page.getByRole('region', { name: 'executions rows' }));
  expect(table.getAllByRole('row')).toHaveLength(8);
  expect(table.getAllByText('ticket-policy')).toHaveLength(5);
  expect(page.getByLabelText('WHERE: read-only projection of the active filters')).toHaveTextContent('select facet values to filter');
  // Bounded shared reads only: every query is one batch read of the executions population, and no row is hydrated.
  expect(new Set(requests.map((request) => request.op))).toEqual(new Set(['bifrost.query', 'cards.list']));
  expect(requests.filter((request) => request.op === 'cards.list')).toEqual([{ op: 'cards.list', kind: 'Verifier', limit: 200 }]);
  expect(queries()).toHaveLength(2);
  expect(queries().every((query) => query.sql.includes(') AS executions WHERE recorded_at >= $1::timestamptz AND recorded_at < $2::timestamptz'))).toBe(true);
  expect(new Set(queries().map((query) => JSON.stringify(query.params?.slice(0, 2)))).size).toBe(1);

  // A facet value, a brush, grouping, Live and paging are navigations: the URL holds the investigation.
  await fireEvent.click(rail.getByRole('button', { name: /failed/ }));
  expect(goto).toHaveBeenLastCalledWith(`/t/acme/verify${search({ where: [{ column: 'verdict', value: 'failed' }] })}`, { keepFocus: true, noScroll: true });
  // Group by is a row of choices with the current one pressed, beside the total the status facet counts.
  const groups = within(page.getByRole('group', { name: 'Group by' }));
  expect(groups.getAllByRole('button').map((button) => [button.textContent?.trim(), button.getAttribute('aria-pressed')])).toEqual([['none', 'true'], ['run', 'false'], ['Card', 'false'], ['Service', 'false'], ['session', 'false'], ['pull request', 'false']]);
  expect(page.getByText('7 executions')).toBeInTheDocument();
  // The list says how many of them it shows and has no more to ask for.
  expect(page.getByText('Showing 7 of 7 · newest first')).toBeInTheDocument();
  await fireEvent.click(groups.getByRole('button', { name: 'session' }));
  expect(goto).toHaveBeenLastCalledWith('/t/acme/verify?group=group_id', expect.anything());
  // Live is off until asked for, and says so.
  await fireEvent.click(page.getByRole('button', { name: 'Paused' }));
  expect(goto).toHaveBeenLastCalledWith('/t/acme/verify?live=1', expect.anything());
  for (const title of ['Executions', 'By Verifier']) {
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
  const filtered = await open(search({ where, from: '2026-10-20T10:00:00.000Z', to: '2026-10-20T12:00:00.000Z', group: 'service_name' }));
  await waitFor(() => expect(filtered.getByRole('region', { name: 'executions rows' })).toBeInTheDocument());
  expect(filtered.getByLabelText('WHERE: read-only projection of the active filters')).toHaveTextContent(`status = 'completed' AND verdict = 'failed'`);
  expect(queries()).toHaveLength(2);
  expect(queries().every((query) => query.sql.includes('status = $') && query.sql.includes('verdict = $') && query.params?.includes('completed') && query.params?.includes('failed'))).toBe(true);
  expect(queries().every((query) => query.params?.[0] === '2026-10-20T10:00:00.000Z' && query.params?.[1] === '2026-10-20T12:00:00.000Z')).toBe(true);
  // Reset zoom returns to the preset; Clear drops the filters; both keep the grouping.
  await fireEvent.click(filtered.getByRole('button', { name: 'Reset zoom' }));
  expect(goto).toHaveBeenLastCalledWith(`/t/acme/verify${search({ where, group: 'service_name' })}`, expect.anything());
  expect(filtered.getByRole('link', { name: 'Clear' })).toHaveAttribute('href', `/t/acme/verify${search({ from: '2026-10-20T10:00:00.000Z', to: '2026-10-20T12:00:00.000Z', group: 'service_name' })}`);
  filtered.unmount();
  // The list reads as many rows as the URL asks for. Another value adds a clause and the changed list starts from its first rows; a pressed value removes its own clause.
  const longer = await open(`${search({ where })}&shown=100`);
  await waitFor(() => expect(longer.getByRole('region', { name: 'executions rows' })).toBeInTheDocument());
  expect(queries().filter((query) => / LIMIT 10[01]$/.test(query.sql))).toHaveLength(1);
  const pressed = within(longer.getByRole('navigation', { name: 'Filter executions' }));
  await fireEvent.click(pressed.getByRole('button', { name: /cancelled/ }));
  expect(goto).toHaveBeenLastCalledWith(`/t/acme/verify${search({ where: [...where, { column: 'status', value: 'cancelled' }] })}`, expect.anything());
  await fireEvent.click(pressed.getByRole('button', { name: /failed/ }));
  expect(goto).toHaveBeenLastCalledWith(`/t/acme/verify${search({ where: [where[0]] })}`, expect.anything());
  longer.unmount();
  // The footer names the order the reader chose, not a fixed one.
  const sorted = await open('?sort=status:asc');
  await waitFor(() => expect(sorted.getByText('Showing 7 of 7 · by Status ↑')).toBeInTheDocument());
  sorted.unmount();

  // An incompatible combination is an empty list, said plainly; a refusal is shown as the refusal, never as empty.
  const empty = await open(search({ where: [{ column: 'status', value: 'cancelled' }, { column: 'verdict', value: 'passed' }] }), stateExecutors.empty);
  await waitFor(() => expect(empty.container).toHaveTextContent('No execution matches these filters in this window.'));
  empty.unmount();
  const denied = await open('', stateExecutors.denied);
  await waitFor(() => expect(denied.container).toHaveTextContent('WYRD_AUTH_403_FORBIDDEN'));
  expect(denied.container).not.toHaveTextContent('No execution matches');
  // The refusals end the load: the one batch of independent reads, and nothing after it.
  expect(requests).toHaveLength(3);
  denied.unmount();
  // A malformed range or an unknown filter column issues no read.
  for (const bad of ['?from=yesterday&to=today', search({ where: [{ column: 'secret', value: 1 }] })]) {
    const refused = await open(bad);
    expect(refused.container).toHaveTextContent('WYRD_SPEC_400_VALIDATION');
    expect(requests).toEqual([]);
    refused.unmount();
  }
});

import { pullHref, readIndex, repositoryHref, writeIndex } from '$lib/repositories-host';
import { load as loadRepositories } from '../routes/t/[tenantKey]/repositories/+page.server';
import { load as loadAccount } from '../routes/t/[tenantKey]/settings/account/+page.server';
import type { RepositoryIndexProps } from 'wyrd-ai-ui/loaders';
import RepositoriesPage from '../routes/t/[tenantKey]/repositories/+page.svelte';
import { load as loadRepository } from '../routes/t/[tenantKey]/repositories/[name]/+page.server';
import RepositoryPage from '../routes/t/[tenantKey]/repositories/[name]/+page.svelte';
import { load as loadUndeclared } from '../routes/t/[tenantKey]/repositories/github/[repositoryId]/+page.server';
import UndeclaredPage from '../routes/t/[tenantKey]/repositories/github/[repositoryId]/+page.svelte';
import { load as loadPull } from '../routes/t/[tenantKey]/repositories/[name]/pulls/[number]/+page.server';
import PullPage from '../routes/t/[tenantKey]/repositories/[name]/pulls/[number]/+page.svelte';
import { load as loadGithub } from '../routes/t/[tenantKey]/settings/integrations/github/+page.server';
import GithubPage from '../routes/t/[tenantKey]/settings/integrations/github/+page.svelte';
import { connectionExecutor, REVIEW } from '$lib/ui-fixtures';
import { EventEmitter } from 'node:events';
import { GET as getObserveSupport } from '../routes/t/[tenantKey]/observe/support/+server';
import { GET as getVerifySupport } from '../routes/t/[tenantKey]/verify/support/+server';

/** The supporting reads of Observe and Verify: facets, overview, timeline, discovery charts and Card names. */
const supporting = (request: WireRequest) => request.op === 'cards.list' || (request.op === 'bifrost.query' && /GROUPING SETS|grouping\(at\)/.test(request.sql));
/** One request as this tenant's browser session, over `socket` when one is given. */
const asSession = (path: string, more: Record<string, unknown> = {}) => {
  const url = new URL(`http://ui.test/t/acme/${path}`);
  return { locals: { browserSession: { client: () => ({}) } }, url, request: new Request(url), depends: () => {}, ...more } as never;
};
/** A fixed window, so two reads of one population bind the same bounds. */
const WINDOW = '&from=2026-10-20T00%3A00%3A00.000Z&to=2026-10-21T00%3A00%3A00.000Z';

test('a client navigation reads only its list, and support reads the population whatever the list shows', async () => {
  const requests: WireRequest[] = [];
  host.executor = { run: (batch, signal) => (requests.push(...batch), fixtureExecutor.run(batch, signal)) };
  // Paging, sorting, grouping and shown columns rerun the list and no supporting read.
  for (const [load, path, section] of [
    [loadVerify, `verify?sort=status:asc&shown=100&group=run_id${WINDOW}`, 'verify'],
    [loadObserve, `observe?sort=name:asc&group=name${WINDOW}`, 'observe']
  ] as const) {
    requests.length = 0;
    const data = (await (load as (event: never) => Promise<unknown>)(asSession(path, { isDataRequest: true }))) as Record<string, { list: { result: { state: string } }; support?: unknown }>;
    expect(data[section].list.result.state).not.toBe('error');
    expect(data[section].support).toBeUndefined();
    expect(requests.filter(supporting)).toEqual([]);
    expect(requests.length).toBeGreaterThan(0);
  }

  // The support routes read the population only: list choices change nothing they read or answer.
  const read = async (handler: (event: never) => Response | Promise<Response>, path: string) => {
    requests.length = 0;
    const response = await handler(asSession(path));
    return { status: response.status, body: await response.json(), requests: [...requests] };
  };
  for (const [handler, path, list] of [
    [getVerifySupport, 'verify/support?', `&sort=status:asc&shown=100&group=run_id&live=1`],
    [getObserveSupport, 'observe/support?', `&sort=name:asc&columns=${encodeURIComponent('["name"]')}&group=name&live=1&trace=${'a'.repeat(32)}`]
  ] as const) {
    const whole = await read(handler, `${path}${WINDOW.slice(1)}`);
    expect(whole.status).toBe(200);
    expect(whole.body.facets.result.state).toBe('populated');
    expect(whole.requests.some(supporting)).toBe(true);
    expect(whole.requests.some((request) => request.op === 'bifrost.query' && !supporting(request))).toBe(false);
    expect(await read(handler, `${path}${WINDOW.slice(1)}${list}`)).toEqual(whole);
    // A refused population is answered with its stable problem and reads nothing.
    const refused = await read(handler, `${path}where=${encodeURIComponent(JSON.stringify([{ column: 'secret', value: 1 }]))}${WINDOW}`);
    expect(refused).toMatchObject({ status: 400, body: { code: 'WYRD_SPEC_400_VALIDATION' }, requests: [] });
  }

  // The first HTML still carries every section.
  const first = (await loadVerify(asSession('verify', { isDataRequest: false }))) as unknown as { verify: { support: { facets: { result: { state: string } } } } };
  expect(first.verify.support.facets.result.state).toBe('populated');
});

test('each Observe request validates its custom predicate once before reading, and a refused one reads nothing', async () => {
  const requests: WireRequest[] = [];
  let refusal: WireResult | undefined;
  host.executor = {
    run: (batch, signal) => {
      requests.push(...batch);
      if (!batch.some((request) => request.op === 'bifrost.validate_predicate')) return fixtureExecutor.run(batch, signal);
      return Promise.resolve(batch.map(() => refusal ?? { ok: true, value: { expression: 'duration_ms > 5' } })) as never;
    }
  };
  const custom = `signal=traces&where_sql=${encodeURIComponent('duration_ms>5')}${WINDOW}`;
  const ops = () => requests.map((request) => request.op);
  const refresh = async () => {
    requests.length = 0;
    const data = (await loadObserve(asSession(`observe?${custom}`, { isDataRequest: true }))) as unknown as { observe: { list: { result: { state: string } } } };
    const list = { state: data.observe.list.result.state, ops: ops() };
    requests.length = 0;
    const response = await getObserveSupport(asSession(`observe/support?${custom}`));
    return { list, support: { status: response.status, body: await response.json(), ops: ops() } };
  };

  // A client refresh is two host requests; each validates the predicate once, first, then reads under it.
  const valid = await refresh();
  expect(valid.list.state).not.toBe('error');
  expect(valid.list.ops[0]).toBe('bifrost.validate_predicate');
  expect(valid.list.ops.filter((op) => op === 'bifrost.validate_predicate')).toHaveLength(1);
  expect(valid.support.status).toBe(200);
  expect(valid.support.ops[0]).toBe('bifrost.validate_predicate');
  expect(valid.support.ops.filter((op) => op === 'bifrost.validate_predicate')).toHaveLength(1);
  expect(valid.support.ops.length).toBeGreaterThan(1);

  // A refused or denied predicate stops both requests at the validation, with no dependent read.
  for (const [code, status, state] of [
    ['WYRD_VALA_400_QUERY_INVALID_SQL', 400, 'error'],
    ['WYRD_VALA_403_QUERY_FORBIDDEN', 403, 'denied']
  ] as const) {
    refusal = { ok: false, problem: { code, title: code, status } };
    const refused = await refresh();
    expect(refused.list).toEqual({ state, ops: ['bifrost.validate_predicate'] });
    expect(refused.support.ops).toEqual(['bifrost.validate_predicate']);
    expect(refused.support.body.facets.result).toMatchObject({ state, problem: { code } });
  }
});

test('a support read stops its upstream reads when the browser closes the connection, and leaves the connection as it found it', async () => {
  const signals: AbortSignal[] = [];
  host.executor = {
    run: async (batch, signal) => {
      if (!batch.some(supporting)) return fixtureExecutor.run(batch, signal);
      if (signal) signals.push(signal);
      return new Promise((_, reject) => signal?.addEventListener('abort', () => reject(signal.reason)));
    }
  };
  for (const [handler, path] of [
    [getVerifySupport, 'verify/support'],
    [getObserveSupport, 'observe/support']
  ] as const) {
    signals.length = 0;
    const socket = new EventEmitter();
    const read = handler(asSession(path, { platform: { req: { socket } } }));
    await vi.waitFor(() => expect(signals.length).toBeGreaterThan(0));
    expect(signals.some((signal) => signal.aborted)).toBe(false);
    socket.emit('close');
    expect(signals.every((signal) => signal.aborted)).toBe(true);
    await expect(read).rejects.toBeDefined();
    expect(socket.listenerCount('close')).toBe(0);
  }
  // A completed read stops listening, so a kept-alive connection gathers no listeners.
  host.executor = fixtureExecutor;
  const socket = new EventEmitter();
  expect((await getVerifySupport(asSession('verify/support', { platform: { req: { socket } } }))).status).toBe(200);
  expect(socket.listenerCount('close')).toBe(0);
});

test('Live starts no refresh until the last refresh has read its support, and stops when the page goes', async () => {
  host.executor = fixtureExecutor;
  const data = await loadVerify(asSession('verify?live=1', { isDataRequest: false }));
  const body = JSON.stringify((data as unknown as { verify: { support: unknown } }).verify.support);
  const answers: ((response: Response) => void)[] = [];
  const fetched = vi.spyOn(globalThis, 'fetch').mockImplementation(() => new Promise<Response>((resolve) => answers.push(resolve)));
  vi.mocked(invalidate).mockReset().mockResolvedValue(undefined);
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'setInterval', 'clearInterval'] });
  try {
    const page = render(VerifyPage, { data: { ...data, tenant: { key: 'acme', name: 'Acme' } } } as never);
    // The list refreshed at once; its support is still being read across three more tick opportunities.
    await vi.advanceTimersByTimeAsync(35_000);
    expect(invalidate).toHaveBeenCalledTimes(1);
    expect(invalidate).toHaveBeenCalledWith('wyrd:verify');
    expect(fetched).toHaveBeenCalledTimes(1);
    const [href, init] = fetched.mock.calls[0] as [string, RequestInit];
    expect(href).toMatch(/^\/t\/acme\/verify\/support\?/);
    expect(new URL(href, 'http://ui.test').searchParams.get('live')).toBeNull();
    expect(init.signal?.aborted).toBe(false);

    // A failed support read settles the refresh: the list stays, the failure is said, and the next tick runs.
    answers[0](new Response(JSON.stringify(wyrdProblem('upstream', 'connection reset')), { status: 502, headers: { 'content-type': 'application/problem+json' } }));
    await vi.advanceTimersByTimeAsync(0);
    expect(page.getByRole('region', { name: 'executions rows' })).toBeInTheDocument();
    expect(page.container).toHaveTextContent('connection reset');
    await vi.advanceTimersByTimeAsync(10_000);
    expect(invalidate).toHaveBeenCalledTimes(2);
    expect(fetched).toHaveBeenCalledTimes(2);

    // A successful one does too, and shows what it read.
    answers[1](new Response(body, { status: 200, headers: { 'content-type': 'application/json' } }));
    await vi.advanceTimersByTimeAsync(0);
    expect(page.container).not.toHaveTextContent('connection reset');
    expect(page.getByRole('region', { name: 'By Verifier' })).toBeInTheDocument();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(invalidate).toHaveBeenCalledTimes(3);

    // Leaving cancels the read in flight, and no tick follows.
    page.unmount();
    expect((fetched.mock.calls[2][1] as RequestInit).signal?.aborted).toBe(true);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(invalidate).toHaveBeenCalledTimes(3);
  } finally {
    vi.useRealTimers();
    fetched.mockRestore();
  }
});

test('the Repositories index lists its Cards without waiting for the GitHub connection', async () => {
  let connected = false;
  const listedBefore: boolean[] = [];
  host.executor = {
    run: async (batch, signal) => {
      if (batch.some((request) => request.op === 'cards.list')) listedBefore.push(!connected);
      const answers = await fixtureExecutor.run(batch, signal);
      if (batch.some((request) => request.op === 'github.get_connection')) await new Promise((settle) => setTimeout(settle, 10)).then(() => (connected = true));
      return answers;
    }
  };
  const url = new URL('http://ui.test/t/acme/repositories');
  const data = (await loadRepositories({ locals: { browserSession: { client: () => ({}) } }, params: { tenantKey: 'acme' }, url, request: new Request(url) } as never)) as { index: RepositoryIndexProps };

  expect(data.index.result.state).toBe('populated');
  expect(listedBefore[0]).toBe(true);
});

test('the Repositories workspace keeps its index state, opens exact versions and leads to the connection', async () => {
  const requests: WireRequest[] = [];
  const tenant = { key: 'acme', name: 'Acme' };
  const event = (path: string, params: object, executor: Executor) => {
    host.executor = { run: (batch, signal) => (requests.push(...batch), executor.run(batch, signal)) };
    requests.length = 0;
    const url = new URL(`http://ui.test/t/acme/${path}`);
    return { locals: { browserSession: { client: () => ({}) } }, params: { tenantKey: 'acme', ...params }, url, request: new Request(url) } as never;
  };
  const index = async (search = '', executor: Executor = fixtureExecutor) => render(RepositoriesPage, { data: { ...(await loadRepositories(event(`repositories${search}`, {}, executor))), tenant } } as never);
  const detail = async (rest: string, executor: Executor = fixtureExecutor) =>
    render(RepositoryPage, { data: { ...(await loadRepository(event(`repositories/${rest}`, { name: decodeURIComponent(rest.split('?')[0]) }, executor))), tenant } } as never);
  const undeclared = async (rest: string) =>
    render(UndeclaredPage, { data: { ...(await loadUndeclared(event(`repositories/github/${rest}`, { repositoryId: rest.split('?')[0] }, fixtureExecutor))), tenant } } as never);
  vi.mocked(goto).mockClear();

  // The index URL round-trips search and Show; an unknown Show is refused before any read.
  expect(writeIndex({ query: 'pay', show: 'ready' })).toBe('?q=pay&show=ready');
  expect(readIndex(new URLSearchParams('?q=pay&show=ready'))).toEqual({ query: 'pay', show: 'ready' });
  expect(writeIndex({})).toBe('');
  const refused = await index('?show=paused');
  expect(refused.container).toHaveTextContent('WYRD_SPEC_400_VALIDATION');
  expect(requests).toEqual([]);
  refused.unmount();

  // Rows open the exact version and carry the index state, so the way back restores it.
  const filtered = await index('?q=acme&show=ready');
  expect(filtered.getByRole('heading', { level: 1, name: 'Repositories' })).toBeInTheDocument();
  expect(filtered.container).toHaveTextContent('1 of 4 shown');
  await fireEvent.click(filtered.getByRole('button', { name: 'payments-api' }));
  expect(goto).toHaveBeenLastCalledWith('/t/acme/repositories/payments-api?version=1.4.0&back=%3Fq%3Dacme%26show%3Dready');
  await fireEvent.click(within(filtered.getByRole('group', { name: 'Show' })).getByRole('button', { name: 'No Card' }));
  expect(goto).toHaveBeenLastCalledWith('/t/acme/repositories?q=acme&show=undeclared', { keepFocus: true, noScroll: true });
  await fireEvent.click(filtered.getByRole('button', { name: 'GitHub connection' }));
  expect(goto).toHaveBeenLastCalledWith('/t/acme/settings/integrations/github');
  filtered.unmount();
  const all = await index();
  await fireEvent.click(all.getByRole('button', { name: 'acme/web-checkout' }));
  expect(goto).toHaveBeenLastCalledWith('/t/acme/repositories/github/741660215');
  all.unmount();

  // The detail: policy and access first, then this Repository's executions from the shared Verify population.
  expect(repositoryHref('/t/acme', { kind: 'Repository', name: 'a b', version: '1.0.0' }, { declaration: 'toml', back: '?q=x' })).toBe('/t/acme/repositories/a%20b?version=1.0.0&declaration=toml&back=%3Fq%3Dx');
  const page = await detail('payments-api?version=1.3.0&back=%3Fq%3Dacme%26show%3Dready');
  expect(page.getByRole('link', { name: 'Repositories' })).toHaveAttribute('href', '/t/acme/repositories?q=acme&show=ready');
  expect(page.container).toHaveTextContent('not selected · viewing only');
  const where = [{ column: 'subject_kind', value: 'Repository' }, { column: 'subject_name', value: 'payments-api' }];
  const activity = requests.filter((request) => request.op === 'bifrost.query') as Extract<WireRequest, { op: 'bifrost.query' }>[];
  expect(activity).toHaveLength(1);
  expect(activity[0].sql).toContain('subject_kind = $3 AND subject_name = $4');
  expect(activity[0].params?.slice(2)).toEqual(['Repository', 'payments-api']);
  // Activity is one list read: no execution's status or detail is read per row.
  expect(requests.some((request) => request.op.startsWith('verification.'))).toBe(false);
  expect(page.getByRole('link', { name: 'View all in Verify' })).toHaveAttribute('href', `/t/acme/verify?${new URLSearchParams({ last: '30d', where: JSON.stringify(where) })}`);
  await fireEvent.change(page.getByRole('combobox', { name: 'Card version' }), { target: { value: '1.4.0' } });
  expect(goto).toHaveBeenLastCalledWith('/t/acme/repositories/payments-api?version=1.4.0&back=%3Fq%3Dacme%26show%3Dready', { keepFocus: true, noScroll: true });
  await fireEvent.click(page.getByRole('tab', { name: 'wyrd.toml' }));
  expect(goto).toHaveBeenLastCalledWith('/t/acme/repositories/payments-api?version=1.3.0&declaration=toml&back=%3Fq%3Dacme%26show%3Dready', { keepFocus: true, noScroll: true });
  await fireEvent.click(page.getByRole('button', { name: 'Verifier definition' }));
  expect(goto).toHaveBeenLastCalledWith('/t/acme/cards/detail?kind=Verifier&name=pr-review&version=1.3.0');
  await fireEvent.click(page.getByRole('button', { name: 'Manage connection' }));
  expect(goto).toHaveBeenLastCalledWith('/t/acme/settings/integrations/github');
  page.unmount();

  // A refused activity read leaves policy and access readable.
  const noActivity = await detail('payments-api?version=1.4.0', {
    run: (batch, signal) => (batch[0].op === 'bifrost.query' ? stateExecutors.denied.run(batch, signal) : fixtureExecutor.run(batch, signal))
  });
  expect(noActivity.container).toHaveTextContent('✓ selected');
  expect(noActivity.getByRole('region', { name: 'Verification activity' })).toHaveTextContent('WYRD_AUTH_403_FORBIDDEN');
  noActivity.unmount();

  // The URL names one exact version; without it, or for an unknown Card, nothing else is read or shown.
  for (const rest of ['payments-api', 'payments-api?version=latest']) {
    const invalid = await detail(rest);
    expect(invalid.container).toHaveTextContent('WYRD_SPEC_400_VALIDATION');
    expect(requests).toEqual([]);
    invalid.unmount();
  }
  const unknown = await detail('nowhere?version=1.0.0');
  expect(unknown.container).toHaveTextContent('WYRD_REGISTRY_404_CARD_NOT_FOUND');
  expect(requests).toEqual([{ op: 'cards.get_by_ref', card_ref: { kind: 'Repository', name: 'nowhere', version: '1.0.0' } }]);
  expect(unknown.queryByRole('region', { name: 'Verification activity' })).toBeNull();
  unknown.unmount();

  // A reachable repository with no Card explains how to declare it; a foreign id names nothing.
  const declare = await undeclared('741660215?back=%3Fshow%3Dundeclared');
  expect(declare.getByRole('link', { name: 'Repositories' })).toHaveAttribute('href', '/t/acme/repositories?show=undeclared');
  expect(declare.container).toHaveTextContent('wyrd declare wyrd.toml');
  declare.unmount();
  const foreign = await undeclared('999');
  expect(foreign.container).toHaveTextContent('WYRD_SPEC_404_NOT_FOUND');
  expect(foreign.container.textContent).not.toMatch(/acme\/\w/);
});

test('account settings reads the profile and the own key page at once, for the session principal', async () => {
  const requested: string[] = [];
  let profiled = false;
  const browserSession = {
    principalId: 'p-1',
    api: async (_method: string, path: string) => {
      requested.push(`${path} (profile answered: ${profiled})`);
      if (path !== '/principals/me') return Response.json({ items: [], next_cursor: null });
      await new Promise((settle) => setTimeout(settle, 10));
      profiled = true;
      return Response.json({ principal_id: 'p-1' });
    }
  };
  const url = new URL('http://ui.test/t/acme/settings/account?after=k-9');
  const data = await loadAccount({ locals: { browserSession }, url } as never);

  expect(data).toMatchObject({ profile: { principal_id: 'p-1' }, keys: { items: [] }, problem: null });
  expect(requested).toEqual(['/principals/me (profile answered: false)', '/principals/p-1/credentials?limit=50&cursor=k-9 (profile answered: false)']);
});

test('Settings connects and disconnects GitHub once through the host and reads state back', async () => {
  const tenant = { key: 'acme', name: 'Acme' };
  const open = async (executor: Executor) => {
    host.executor = executor;
    const url = new URL('http://ui.test/t/acme/settings/integrations/github');
    return render(GithubPage, { data: { ...(await loadGithub({ locals: { browserSession: { client: () => ({}) } }, params: { tenantKey: 'acme' }, url, request: new Request(url) } as never)), tenant } } as never);
  };
  const here = '/t/acme/settings/integrations/github';
  const answers: ((response: Response) => void)[] = [];
  const fetched = vi.spyOn(globalThis, 'fetch').mockImplementation(() => new Promise<Response>((resolve) => answers.push(resolve)));
  const opened = vi.spyOn(window, 'open').mockReturnValue(null);
  vi.mocked(invalidateAll).mockClear();

  // Connect asks the host for the authorization URL once, then leaves for GitHub in the same tab.
  const start = await open(connectionExecutor({ state: 'not_connected' }));
  expect(start.getByRole('heading', { level: 1, name: 'GitHub' })).toBeInTheDocument();
  const connect = () => start.getByRole('button', { name: 'Connect GitHub' });
  await fireEvent.click(connect());
  await waitFor(() => expect(connect()).toBeDisabled());
  await fireEvent.click(connect());
  expect(fetched.mock.calls).toEqual([[`${here}/connection`, { method: 'POST' }]]);
  answers[0](Response.json({ authorization_url: 'https://github.com/apps/wyrd-verify/installations/new?state=s1' }));
  await waitFor(() => expect(opened).toHaveBeenCalledWith('https://github.com/apps/wyrd-verify/installations/new?state=s1', '_self'));
  start.unmount();
  // A refused connect stays on the page with the server's code.
  const refusedStart = await open(connectionExecutor({ state: 'not_connected' }));
  await fireEvent.click(refusedStart.getByRole('button', { name: 'Connect GitHub' }));
  answers[1](Response.json({ code: 'WYRD_PERMISSION_403_DENIED_RBAC', title: 'Permission denied', status: 403 }, { status: 403 }));
  await waitFor(() => expect(refusedStart.container).toHaveTextContent('WYRD_PERMISSION_403_DENIED_RBAC'));
  expect(opened).toHaveBeenCalledTimes(1);
  refusedStart.unmount();

  // Disconnect is one confirmed host request; the page then reads the connection back.
  fetched.mockClear();
  vi.mocked(invalidateAll).mockClear();
  const connected = await open(fixtureExecutor);
  const dialog = connected.container.querySelector('dialog') as HTMLDialogElement;
  dialog.showModal = vi.fn();
  const confirm = async () => {
    await fireEvent.click(connected.getByRole('button', { name: 'Disconnect' }));
    dialog.returnValue = 'confirm';
    await fireEvent(dialog, new Event('close'));
  };
  await confirm();
  await waitFor(() => expect(connected.getByRole('button', { name: 'Disconnect' })).toBeDisabled());
  expect(fetched.mock.calls).toEqual([[`${here}/connection`, { method: 'DELETE' }]]);
  expect(invalidateAll).not.toHaveBeenCalled();
  answers[2](Response.json({ host: 'github.com', app: { id: 4242, slug: 'wyrd-verify' }, state: 'not_connected', repositories: [] }));
  await waitFor(() => expect(invalidateAll).toHaveBeenCalledTimes(1));
  await waitFor(() => expect(connected.getByRole('button', { name: 'Disconnect' })).toBeEnabled());
  // A refusal is shown with its code and the connection stays as the server reports it.
  await confirm();
  answers[3](Response.json({ code: 'WYRD_PERMISSION_403_DENIED_RBAC', title: 'Permission denied', status: 403 }, { status: 403 }));
  await waitFor(() => expect(connected.container).toHaveTextContent('WYRD_PERMISSION_403_DENIED_RBAC'));
  expect(connected.container).toHaveTextContent('Connected');
  connected.unmount();

  // Denied, and an App that is not configured, show the problem and no action or Card read.
  const calls: WireRequest[] = [];
  const noApp = await open({ run: (batch) => (calls.push(...batch), Promise.resolve(batch.map(() => ({ ok: false, problem: { code: 'WYRD_GITHUB_503_NOT_CONFIGURED', title: 'GitHub App not configured', status: 503 } }))) as never) });
  expect(noApp.container).toHaveTextContent('WYRD_GITHUB_503_NOT_CONFIGURED');
  expect(noApp.queryByRole('button')).toBeNull();
  expect(calls).toEqual([{ op: 'github.get_connection' }]);
  fetched.mockRestore();
  opened.mockRestore();
});

test('a pull request page follows the latest review set, keeps a pinned set in its URL and opens a member', async () => {
  const requests: WireRequest[] = [];
  const tenant = { key: 'acme', name: 'Acme' };
  /** The page data of /t/acme/repositories/payments-api/pulls/`rest`. */
  const load = async (rest: string, executor: Executor = fixtureExecutor) => {
    host.executor = { run: (batch, signal) => (requests.push(...batch), executor.run(batch, signal)) };
    requests.length = 0;
    const url = new URL(`http://ui.test/t/acme/repositories/payments-api/pulls/${rest}`);
    const event = { locals: { browserSession: { client: () => ({}) } }, params: { tenantKey: 'acme', name: 'payments-api', number: rest.split('?')[0] }, url, request: new Request(url) };
    return { data: { ...(await loadPull(event as never)), tenant } };
  };
  /** The pull request page at `rest`. */
  const pull = async (rest: string, executor?: Executor) => render(PullPage, (await load(rest, executor)) as never);
  vi.mocked(goto).mockClear();
  expect(pullHref('/t/acme', 'a b', 7)).toBe('/t/acme/repositories/a%20b/pulls/7');
  expect(pullHref('/t/acme', 'a b', 7, 'g 1')).toBe('/t/acme/repositories/a%20b/pulls/7?set=g+1');

  // The plain link shows the latest set and its members, named by the Verifiers the page read.
  const latest = await pull(String(REVIEW.number));
  expect(latest.getByRole('heading', { level: 1 })).toHaveTextContent('Refund window follows the 30-day policy');
  // The Repository named in the header and in the crumbs leads to the policy version the set recorded.
  expect(latest.getAllByRole('link', { name: 'payments-api' }).map((link) => link.getAttribute('href'))).toEqual(['/t/acme/repositories/payments-api?version=1.4.0', '/t/acme/repositories/payments-api?version=1.4.0']);
  expect(latest.queryByRole('status')).toBeNull();
  expect(latest.getAllByRole('row').slice(1).map((row) => row.querySelectorAll('td')[1].textContent?.trim())).toEqual(['ticket-policy@1.1.0', 'latency-drift@1.0.0']);
  // A set is two list reads and the Verifier names: no execution's status or detail is read per member.
  expect(requests.map((request) => request.op)).toEqual(['bifrost.query', 'bifrost.query', 'cards.list']);

  // Each set, member and Verifier is a link: the earlier set's URL pins it, a member leads to its exact execution
  // and a Verifier to its exact definition.
  const sets = within(latest.getByRole('navigation', { name: 'Review sets of #482' })).getAllByRole('link');
  expect(sets.map((link) => link.getAttribute('href'))).toEqual(['/t/acme/repositories/payments-api/pulls/482', `/t/acme/repositories/payments-api/pulls/482?set=${REVIEW.earlier}`]);
  expect(latest.getByRole('link', { name: 'exec_failed' })).toHaveAttribute('href', '/t/acme/verify/executions/exec_failed');
  expect(latest.getByRole('link', { name: 'ticket-policy@1.1.0' })).toHaveAttribute('href', '/t/acme/cards/detail?kind=Verifier&name=ticket-policy&version=1.1.0');
  latest.unmount();

  // The pinned link stays on its set and offers the latest.
  const pinned = await pull(`482?set=${REVIEW.earlier}`);
  expect(pinned.getByRole('status')).toHaveTextContent('You are viewing earlier review set');
  // The earlier set was recorded before titles were retained, so it is headed by its number.
  expect(pinned.getByRole('heading', { level: 1 })).toHaveTextContent('#482');
  expect(pinned.getAllByRole('row').slice(1).map((row) => row.querySelector('td')?.textContent?.trim())).toEqual(['exec_cancelled', 'exec_waiting']);
  expect(within(pinned.getByRole('status')).getByRole('link')).toHaveAttribute('href', '/t/acme/repositories/payments-api/pulls/482');
  // Changing the pull request while that set is open shows the other pull request alone: no pinned banner, set or member carries over.
  await pinned.rerender((await load('1')) as never);
  expect(pinned.container).toHaveTextContent('No review set is retained for this pull request.');
  expect(pinned.queryByRole('status')).toBeNull();
  expect(pinned.container.querySelector('table')).toBeNull();
  expect(pinned.container).not.toHaveTextContent('exec_cancelled');
  pinned.unmount();

  // A pinned set that is not retained is reported and never replaced by the latest.
  const gone = await pull('482?set=never-retained');
  expect(gone.getByRole('status')).toHaveTextContent('is not retained for this pull request');
  expect(gone.container.querySelector('table')).toBeNull();
  gone.unmount();

  // A refused Verifier-name read leaves the set readable, each member by its Verifier UID.
  const unnamed = await pull('482', { run: (batch, signal) => (batch[0].op === 'cards.list' ? stateExecutors.denied.run(batch, signal) : fixtureExecutor.run(batch, signal)) });
  expect(unnamed.getAllByRole('row')).toHaveLength(3);
  expect(unnamed.container).not.toHaveTextContent('ticket-policy');
  unnamed.unmount();

  // A pull request with no retained set, a refused read and a malformed number each say so; the last reads nothing.
  const none = await pull('1');
  expect(none.container).toHaveTextContent('No review set is retained for this pull request.');
  none.unmount();
  const denied = await pull('482', stateExecutors.denied);
  expect(denied.container).toHaveTextContent('WYRD_AUTH_403_FORBIDDEN');
  expect(requests.map((request) => request.op)).toEqual(['bifrost.query']);
  denied.unmount();
  const malformed = await pull('latest');
  expect(malformed.container).toHaveTextContent('WYRD_SPEC_400_VALIDATION');
  expect(requests).toEqual([]);
});

test('Repository activity, Verify and an execution each lead to the pull request review sets', async () => {
  const tenant = { key: 'acme', name: 'Acme' };
  const session = { locals: { browserSession: { client: () => ({}), api: () => Promise.resolve(new Response()) } } };
  host.executor = fixtureExecutor;
  vi.mocked(goto).mockClear();

  // Repository activity names each row's pull request; pressing it opens that row's own review set.
  const repositoryUrl = new URL('http://ui.test/t/acme/repositories/payments-api?version=1.4.0');
  const repository = render(RepositoryPage, { data: { ...(await loadRepository({ ...session, params: { tenantKey: 'acme', name: 'payments-api' }, url: repositoryUrl, request: new Request(repositoryUrl) } as never)), tenant } } as never);
  const activity = within(repository.getByRole('region', { name: 'Verification activity' }));
  // Each row names its pull request, the head it reviewed and the policy version that admitted it, in the mock's order.
  expect(activity.getAllByRole('columnheader').map((cell) => cell.textContent?.replace(/[↑↓]/g, '').trim())).toEqual(['Time', 'Pull request', 'Head', 'Verifier', 'Policy', 'Status', 'Verdict']);
  const pulled = activity.getAllByRole('row').find((row) => row.textContent?.includes('payments-api#482'));
  expect(pulled && within(pulled).getByTitle('9f2c1ab0c4')).toHaveTextContent(/^9f2c1ab$/);
  await fireEvent.click(activity.getAllByRole('button', { name: 'payments-api#482' })[0]);
  expect(goto).toHaveBeenLastCalledWith(`/t/acme/repositories/payments-api/pulls/482?set=${REVIEW.latest}`);
  repository.unmount();

  // Verify counts and groups by pull request over the same population, and its rows open the same page.
  const state = readState(new URLSearchParams());
  expect(verifyInputs(state).facets.facets).toContain('pull_request');
  expect(verifyInputs({ ...state, group: 'pull_request' }).list).toMatchObject({ groupBy: 'pull_request' });
  expect(verifyStep(state, { name: 'pull_request.open', input: { repository: 'a b', number: 7 } }, '')).toEqual({ pull: '/repositories/a%20b/pulls/7' });
  expect(verifyStep(state, { name: 'pull_request.open', input: { repository: 'api', number: 7, generation: 'g1' } }, '')).toEqual({ pull: '/repositories/api/pulls/7?set=g1' });
});
