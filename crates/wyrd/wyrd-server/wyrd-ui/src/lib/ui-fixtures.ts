/**
 * Representative component data shared by the dev gallery and the component
 * tests. Shapes follow the real wire types; values are synthetic.
 */
import type { CardRef, Executor, QueryResult, RegisteredCard } from 'wyrd-ai-ui';
import type { ChartInput } from 'wyrd-ai-ui/loaders';

/** A terminal for a complete result of `rows` rows. */
export function terminal(rows: number, outcome: 'success' | 'degraded' = 'success'): QueryResult['terminal'] {
  return {
    outcome,
    query_class: 'interactive',
    row_count: rows,
    warnings: outcome === 'degraded' ? ['live_tail_unavailable'] : [],
    source_completion: []
  };
}

export const series: QueryResult = {
  columns: [
    { name: 'at', data_type: 'Timestamp(Microsecond, None)', nullable: false },
    { name: 'model', data_type: 'Utf8', nullable: false },
    { name: 'p95', data_type: 'Float64', nullable: true }
  ],
  rows: [
    ['2026-10-20T10:00:00', 'a', 120],
    ['2026-10-20T10:00:00', 'b', 80],
    ['2026-10-20T11:00:00', 'a', 140],
    ['2026-10-20T11:00:00', 'b', null]
  ],
  terminal: terminal(4)
};

/** A day of hourly p95 for two models, for drawing a realistic plot. */
export const day: QueryResult = {
  columns: series.columns,
  rows: Array.from({ length: 24 }, (_, hour) => {
    const at = `2026-10-20T${String(hour).padStart(2, '0')}:00:00`;
    return [
      [at, 'reranker', 180 + Math.round(60 * Math.sin(hour / 3)) + (hour % 5) * 7],
      [at, 'embedder', 90 + Math.round(25 * Math.cos(hour / 4)) + (hour % 3) * 5]
    ];
  }).flat(),
  terminal: terminal(48)
};
export const timeseries: ChartInput = { type: 'timeseries', title: 'Latency', sql: 'SELECT at, model, p95 FROM t WHERE at >= $1::timestamptz AND at < $2::timestamptz', unit: 'ms', x: 'at', y: ['p95'], series: 'model' };
export const card: CardRef = { kind: 'Model', name: 'ranker', version: '1.4.2' };
export const registered = {
  apiVersion: 'wyrd/v1',
  kind: 'Model',
  metadata: { name: 'ranker', version: '1.4.2', uid: 'card_01HX' },
  spec: {}
} as unknown as RegisteredCard;
export const summary = (version: string, status = 'active') => ({ version, status, name: 'ranker', kind: 'Model', created_at: '2026-10-19T14:02:00Z' }) as never;
export const denied = { code: 'WYRD_AUTH_403_FORBIDDEN', title: 'Forbidden', status: 403 };
export const RANGE = { from: '2026-10-20T00:00:00Z', to: '2026-10-20T12:00:00Z' };

const col = (name: string, data_type = 'Utf8', nullable = true) => ({ name, data_type, nullable });
const result = (columns: QueryResult['columns'], rows: unknown[][]): QueryResult => ({ columns, rows, terminal: terminal(rows.length) });
const NS = 'Timestamp(Nanosecond, Some("UTC"))';
export const TRACE = '4bf92f3577b34da6a3ce929d0e0e4736';

/** Root spans, as the traces list reads them: shown columns, then trace_id and span_id. */
export const traces = result(
  [col('start_time_unix_nano', NS, false), col('name', 'Utf8', false), col('service_name'), col('duration_nano', 'Int64', false), col('status_code', 'Int32', false), col('trace_id', 'Utf8', false), col('span_id', 'Utf8', false)],
  [
    ['2026-10-20T11:58:01.120000000+00:00', 'invoke_agent support', 'agent-api', 2_840_000_000, 2, TRACE, '00f067aa0ba902b7'],
    ['2026-10-20T11:57:44.004000000+00:00', 'POST /v1/answer', 'agent-api', 612_000_000, 1, 'a3ce929d0e0e47364bf92f3577b34da6', '53995c3f42cd8ad8'],
    ['2026-10-20T11:56:12.500000000+00:00', 'chat gpt-5', null, 95_000_000, 0, '0e0e47364bf92f3577b34da6a3ce929d', 'b9c7c989f97918e1']
  ]
);
/** Span and error counts of the listed traces. */
export const traceCounts = result([col('trace_id'), col('spans', 'Int64'), col('errors', 'Int64')], [[TRACE, 6, 1], ['a3ce929d0e0e47364bf92f3577b34da6', 3, 0]]);

/** One trace: a root, two children, a grandchild with an error, and a span whose parent was not retained. */
export const trace = result(
  [col('span_id'), col('parent_span_id'), col('name'), col('service_name'), col('start_time_unix_nano', NS), col('duration_nano', 'Int64'), col('status_code', 'Int32')],
  [
    ['00f067aa0ba902b7', null, 'invoke_agent support', 'agent-api', '2026-10-20T11:58:01.120000000+00:00', 2_840_000_000, 2],
    ['53995c3f42cd8ad8', '00f067aa0ba902b7', 'chat gpt-5', 'agent-api', '2026-10-20T11:58:01.130000000+00:00', 1_200_000_000, 1],
    ['b9c7c989f97918e1', '00f067aa0ba902b7', 'execute_tool search', 'agent-api', '2026-10-20T11:58:02.400000000+00:00', 900_000_000, 2],
    ['c1d2e3f405060708', 'b9c7c989f97918e1', 'GET /search', 'search', '2026-10-20T11:58:02.450000001+00:00', 800_000_000, 2],
    ['0102030405060708', 'ffffffffffffffff', 'late span', 'search', '2026-10-20T11:58:03.000000000+00:00', 10_000_000, 0]
  ]
);

/** One stored span, whole. */
export const spanRow = result(
  [col('trace_id'), col('span_id'), col('name'), col('start_time_unix_nano', NS), col('attributes', 'Variant'), col('events', 'List'), col('exception_type'), col('exception_message'), col('exception_stacktrace')],
  [[TRACE, 'b9c7c989f97918e1', 'execute_tool search', '2026-10-20T11:58:02.400000000+00:00', { 'gen_ai.tool.name': 'search', 'gen_ai.input.messages': [{ role: 'user', parts: [{ type: 'text', content: 'refund?' }] }] }, [{ name: 'retry', attributes: {} }], 'TimeoutError', 'search timed out', null]]
);

/** One stored log record, whole. */
export const logRow = result(
  [col('time_unix_nano', NS), col('observed_time_unix_nano', NS), col('wyrd_request_id'), col('severity_text'), col('body', 'Variant'), col('trace_id'), col('span_id'), col('attributes', 'Variant'), col('exception_type')],
  [['2026-10-20T11:58:02.410000000+00:00', '2026-10-20T11:58:02.411000000+00:00', 'req_01', 'ERROR', 'search timed out', TRACE, 'b9c7c989f97918e1', {}, null]]
);
export const LOG = { time_unix_nano: '2026-10-20T11:58:02.410000000+00:00', observed_time_unix_nano: '2026-10-20T11:58:02.411000000+00:00', wyrd_request_id: 'req_01' };

/** Facet counts: largest first, with a null value. */
export const facet = result([col('value'), col('n', 'Int64')], [['agent-api', 42], ['search', 7], [null, 3]]);
/** One stat total and its buckets. */
export const total = (value: unknown) => result([col('value', 'Int64')], [[value]]);
export const buckets = result([col('at', NS), col('value', 'Int64')], [['2026-10-20T10:00:00Z', 4], ['2026-10-20T11:00:00Z', 9], ['2026-10-20T12:00:00Z', 6]]);
export const costBuckets = result(
  [col('at', NS), col('value', 'Decimal128(38, 12)'), col('currency')],
  [['2026-10-20T10:00:00Z', '0.400000000000', 'EUR'], ['2026-10-20T10:00:00Z', '0.100000000000', 'USD'], ['2026-10-20T11:00:00Z', '0.850000000000', 'EUR'], ['2026-10-20T11:00:00Z', '0.320000000000', 'USD']]
);
export const costs = result([col('value', 'Decimal128(38, 12)'), col('currency')], [['1.250000000000', 'EUR'], ['0.420000000000', 'USD']]);

/** The fixture answering one Observe read, chosen by its fixed SQL shape. */
export function observeAnswer(sql: string): QueryResult | undefined {
  if (sql.includes("encode(span_id, 'hex') AS span_id FROM bifrost.traces.spans")) return traces;
  if (sql.includes('AS spans, count(*) FILTER')) return traceCounts;
  if (sql.includes("encode(parent_span_id, 'hex')")) return trace;
  if (sql.startsWith('SELECT * FROM bifrost.logs.records')) return logRow;
  if (sql.startsWith('SELECT * FROM bifrost.traces.spans')) return spanRow;
  if (sql.includes(' AS n FROM')) return facet;
  if (sql.includes('AS at,')) return sql.includes(', currency FROM') ? costBuckets : buckets;
  if (sql.includes(', currency FROM')) return costs;
  if (sql.startsWith('SELECT') && sql.includes(' AS value FROM')) return total(128);
  return undefined;
}

/** An executor serving the fixtures above, for views rendered without a server. */
export const fixtureExecutor: Executor = {
  run: (requests) =>
    Promise.resolve(
      requests.map((request) => {
        switch (request.op) {
          case 'cards.get_by_ref':
            return { ok: true, value: registered };
          case 'cards.list':
            return { ok: true, value: { items: [summary('1.4.2'), summary('1.4.1', 'deprecated')], next_cursor: null } };
          case 'bifrost.query': {
            // Honour bound $1/$2 like the server, so a brushed range visibly narrows the plot.
            const observed = observeAnswer(request.sql);
            if (observed) return { ok: true, value: observed };
            const [from, to] = request.params ?? [];
            if (typeof from !== 'string' || typeof to !== 'string') return { ok: true, value: day };
            const rows = day.rows.filter((row) => Date.parse(`${row[0]}Z`) >= Date.parse(from) && Date.parse(`${row[0]}Z`) < Date.parse(to));
            return { ok: true, value: { ...day, rows, terminal: terminal(rows.length) } };
          }
          default:
            return { ok: false, problem: denied };
        }
      })
    ) as never
};
