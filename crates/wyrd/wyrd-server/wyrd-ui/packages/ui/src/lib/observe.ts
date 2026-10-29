/**
 * Observe loaders: built-in reads of the canonical signal tables. Every SQL
 * text is assembled only from the fixed tables and column allowlists in
 * `SIGNALS`; every value, range bound and identity is a typed `$n` bind.
 */
import { failed, wyrdProblem, type Loaded, type Problem, type QueryParam, type QueryResult, type WireRequest } from './contract.js';
import { bucketFor, chartParams, resolveRange, type LoadContext } from './loaders.js';
import {
  SIGNALS,
  type AbsoluteRange,
  type FacetRailInput,
  type Filter,
  type Range,
  type RecordDetailInput,
  type RecordTableInput,
  type Signal,
  type Stat,
  type StatStripInput,
  type TraceWaterfallInput
} from './schemas.js';

/** An executed read, for reproducing it in Query. */
export interface Executed {
  readonly sql: string;
  readonly params: readonly QueryParam[];
}

/** Props of `StatStrip`: per stat, its total rows and its bucketed rows. */
export interface StatStripProps {
  readonly input: StatStripInput;
  readonly result: Loaded<{ readonly stats: readonly { readonly stat: Stat; readonly total: QueryResult; readonly series: QueryResult }[] }>;
}

/** Props of `FacetRail`: per facet, `value, n` rows, largest first; an eleventh row means more values exist. */
export interface FacetRailProps {
  readonly input: FacetRailInput;
  readonly result: Loaded<{ readonly facets: readonly { readonly column: string; readonly values: QueryResult }[] }>;
}

/**
 * Props of `RecordTable`. Signal rows carry the shown columns first, then the
 * identity columns of `keyColumns`; trace rows also carry span and error
 * counts read for the visible traces only.
 */
export interface RecordTableProps {
  readonly input: RecordTableInput;
  readonly executed?: Executed;
  readonly result: Loaded<{ readonly rows: QueryResult; readonly counts?: Loaded<QueryResult> }>;
}

/** Props of `TraceWaterfall`: every retained span of the trace, in start order. */
export interface TraceWaterfallProps {
  readonly input: TraceWaterfallInput;
  readonly result: Loaded<QueryResult>;
}

/** Props of `RecordDetail`: the whole stored row; more than one row shares a log key only on a duplicate write. */
export interface RecordDetailProps {
  readonly input: RecordDetailInput;
  readonly result: Loaded<QueryResult>;
}

/** Rows a facet reads: ten shown, the eleventh only reports that more exist. */
export const FACET_VALUES = 10;
/** Rows a list reads unless its input says otherwise. */
const LIST_LIMIT = 50;
/** Spans a trace may hold before its read is refused as too large rather than shown truncated. */
const TRACE_SPANS = 10_000;

/** The panel's own range, else the view's; a signal read needs one. */
function rangeOf(own: Range | undefined, context: LoadContext): AbsoluteRange | Problem {
  const range = own ? resolveRange(own) : context.range;
  return range ?? wyrdProblem('validation', 'A signal read needs a range and none was supplied.');
}

/** Builds one read's predicate list, binding each value as the next `$n`. */
class Binds {
  readonly params: QueryParam[] = [];
  readonly clauses: string[] = [];

  /** Bind `value` and return its placeholder. */
  bind(value: QueryParam): string {
    this.params.push(value);
    return `$${this.params.length}`;
  }

  /** Start with the event-time range as `$1`/`$2`. */
  static over(time: string, range: AbsoluteRange): Binds {
    const binds = new Binds();
    binds.clauses.push(`${time} >= ${binds.bind(range.from)}::timestamptz AND ${time} < ${binds.bind(range.to)}::timestamptz`);
    return binds;
  }

  /** AND each WHERE clause; the column is already one of the signal's allowlisted names. */
  where(filters: readonly Filter[] = []): this {
    for (const { column, value, not } of filters) {
      if (value === null) this.clauses.push(`${column} IS ${not ? 'NOT ' : ''}NULL`);
      else this.clauses.push(`${column} ${not ? 'IS DISTINCT FROM' : '='} ${this.bind(value)}`);
    }
    return this;
  }

  get sql(): string {
    return this.clauses.join(' AND ');
  }
}

/** The bifrost.traces.spans predicate that keeps only root spans. */
const ROOT = 'parent_span_id IS NULL';
/** OTLP `STATUS_CODE_ERROR`. */
const STATUS_ERROR = 2;

/** One query of the batch, or the problem that stops it. */
function query(sql: string, params: readonly QueryParam[], max_rows?: number): Extract<WireRequest, { op: 'bifrost.query' }> {
  return { op: 'bifrost.query', sql, params: [...params], ...(max_rows === undefined ? {} : { max_rows }) };
}

/** The first refused or failed result of a batch, as the component's state. */
function firstFailure(results: readonly { ok: boolean; problem?: Problem }[]): Loaded<never> | undefined {
  const failure = results.find((each) => !each.ok);
  return failure?.problem ? failed(failure.problem) : undefined;
}

/** The value column and table of each built-in stat. */
const STAT_SOURCES: Record<Stat, { table: string; time: string; value: string; filter?: string; byCurrency?: true }> = {
  traces: { table: SIGNALS.spans.table, time: SIGNALS.spans.time, value: 'count(*)', filter: ROOT },
  spans: { table: SIGNALS.spans.table, time: SIGNALS.spans.time, value: 'count(*)' },
  errors: { table: SIGNALS.spans.table, time: SIGNALS.spans.time, value: 'count(*)', filter: `status_code = ${STATUS_ERROR}` },
  duration_p95: { table: SIGNALS.spans.table, time: SIGNALS.spans.time, value: 'approx_percentile_cont(duration_nano, 0.95)', filter: ROOT },
  invocations: { table: SIGNALS.spans.table, time: SIGNALS.spans.time, value: 'count(*)', filter: "gen_ai_operation_name = 'invoke_agent'" },
  // Null, never zero, when no span recorded any usage.
  tokens: {
    table: SIGNALS.spans.table,
    time: SIGNALS.spans.time,
    value:
      'CASE WHEN count(gen_ai_usage_input_tokens) + count(gen_ai_usage_output_tokens) = 0 THEN NULL ' +
      'ELSE coalesce(sum(gen_ai_usage_input_tokens), 0) + coalesce(sum(gen_ai_usage_output_tokens), 0) END'
  },
  calls: { table: 'bifrost.gateway.calls', time: 'started_at', value: 'count(*)' },
  // Amounts in different currencies never add up: one row per recorded currency.
  cost: { table: 'bifrost.gateway.calls', time: 'started_at', value: 'sum(CAST(cost AS DECIMAL(38, 12)))', filter: 'cost IS NOT NULL', byCurrency: true }
};

/** The total and per-bucket reads of one stat. */
export function statQueries(stat: Stat, input: StatStripInput, range: AbsoluteRange, bucket: string): [Executed, Executed] {
  const source = STAT_SOURCES[stat];
  const span = source.table === SIGNALS.spans.table;
  const plan = (series: boolean) => {
    const binds = Binds.over(source.time, range);
    const bin = series ? `date_bin(${binds.bind(bucket)}::interval, ${source.time})` : '';
    if (source.filter) binds.clauses.push(source.filter);
    if (input.scope) binds.clauses.push(`${input.scope.column} = ${binds.bind(input.scope.uid)}`);
    if (span) binds.where(input.where);
    const currency = source.byCurrency ? ', currency' : '';
    const sql = series
      ? `SELECT ${bin} AS at, ${source.value} AS value${currency} FROM ${source.table} WHERE ${binds.sql} GROUP BY ${bin}${currency} ORDER BY at`
      : `SELECT ${source.value} AS value${currency} FROM ${source.table} WHERE ${binds.sql}${currency ? ` GROUP BY currency ORDER BY currency` : ''}`;
    return { sql, params: binds.params };
  };
  return [plan(false), plan(true)];
}

/** Load each stat's total and series in one batch; one refusal refuses the strip. */
export async function loadStatStrip(input: StatStripInput, context: LoadContext): Promise<StatStripProps> {
  const range = rangeOf(input.range, context);
  if ('code' in range) return { input, result: { state: 'error', problem: range } };
  const plans = input.stats.map((stat) => statQueries(stat, input, range, bucketFor(range)));
  const results = await context.executor.run(plans.flatMap(([total, series]) => [query(total.sql, total.params), query(series.sql, series.params)]), context.signal);
  const failure = firstFailure(results);
  if (failure) return { input, result: failure };
  const stats = input.stats.map((stat, index) => ({
    stat,
    total: (results[index * 2] as { value: QueryResult }).value,
    series: (results[index * 2 + 1] as { value: QueryResult }).value
  }));
  const recorded = stats.some(({ total }) => total.rows.some((row) => row[0] !== null && Number(row[0]) !== 0));
  if (!recorded) return { input, result: { state: 'empty', data: { stats }, reason: `Nothing recorded in this range${scopeText(input)}.` } };
  return { input, result: { state: 'populated', data: { stats } } };
}

/** The ownership scope as a reason names it. */
function scopeText(input: StatStripInput): string {
  return input.scope ? ` for ${input.scope.column} = ${input.scope.uid}` : '';
}

/** Facet counts of one column under the list's own WHERE and range. */
export function facetQuery(input: FacetRailInput, column: string, range: AbsoluteRange): Executed {
  const signal = SIGNALS[input.signal];
  const binds = Binds.over(signal.time, range);
  if (input.signal === 'traces') binds.clauses.push(ROOT);
  binds.where(input.where);
  return {
    sql: `SELECT ${column} AS value, count(*) AS n FROM ${signal.table} WHERE ${binds.sql} GROUP BY ${column} ORDER BY n DESC, value LIMIT ${FACET_VALUES + 1}`,
    params: binds.params
  };
}

/** Load every facet in one batch; one refusal refuses the rail. */
export async function loadFacetRail(input: FacetRailInput, context: LoadContext): Promise<FacetRailProps> {
  const range = rangeOf(input.range, context);
  if ('code' in range) return { input, result: { state: 'error', problem: range } };
  const plans = input.facets.map((column) => facetQuery(input, column, range));
  const results = await context.executor.run(plans.map((plan) => query(plan.sql, plan.params)), context.signal);
  const failure = firstFailure(results);
  if (failure) return { input, result: failure };
  const facets = input.facets.map((column, index) => ({ column, values: (results[index] as { value: QueryResult }).value }));
  if (facets.every(({ values }) => values.rows.length === 0))
    return { input, result: { state: 'empty', data: { facets }, reason: `No ${input.signal} match this WHERE in this range.` } };
  return { input, result: { state: 'populated', data: { facets } } };
}

/** Identity columns a list reads after its shown columns, by signal. */
export const KEY_COLUMNS: Record<Signal, readonly string[]> = {
  traces: ['trace_id', 'span_id'],
  spans: ['trace_id', 'span_id'],
  logs: ['trace_id', 'span_id', 'observed_time_unix_nano', 'wyrd_request_id'],
  metrics: []
};

/** The columns a signal list shows. */
export function shownColumns(input: Extract<RecordTableInput, { signal: Signal }>): readonly string[] {
  return input.columns ?? SIGNALS[input.signal].columns;
}

/** The newest rows of one signal, shown columns first and then its identity columns as lowercase hex. */
export function listQuery(input: Extract<RecordTableInput, { signal: Signal }>, range: AbsoluteRange): Executed {
  const signal = SIGNALS[input.signal];
  const binds = Binds.over(signal.time, range);
  if (input.signal === 'traces') binds.clauses.push(ROOT);
  binds.where(input.where);
  const keys = KEY_COLUMNS[input.signal].map((key) => (key.endsWith('_id') && key !== 'wyrd_request_id' ? `encode(${key}, 'hex') AS ${key}` : key));
  return {
    sql: `SELECT ${[...shownColumns(input), ...keys].join(', ')} FROM ${signal.table} WHERE ${binds.sql} ORDER BY ${signal.time} DESC LIMIT ${input.limit ?? LIST_LIMIT}`,
    params: binds.params
  };
}

/** Span and error counts of exactly these traces, whatever their time. */
export function countsQuery(traceIds: readonly string[]): Executed {
  const binds = new Binds();
  const ids = traceIds.map((id) => `decode(${binds.bind(id)}, 'hex')`).join(', ');
  return {
    sql:
      `SELECT encode(trace_id, 'hex') AS trace_id, count(*) AS spans, count(*) FILTER (WHERE status_code = ${STATUS_ERROR}) AS errors ` +
      `FROM ${SIGNALS.spans.table} WHERE trace_id IN (${ids}) GROUP BY trace_id`,
    params: binds.params
  };
}

/**
 * Load one list. Trace rows then read span and error counts for the visible
 * traces only; a refused or failed list issues no count read, and a failed
 * count read keeps the list with its own state.
 */
export async function loadRecordTable(input: RecordTableInput, context: LoadContext): Promise<RecordTableProps> {
  let executed: Executed;
  if ('sql' in input) {
    const range = input.range ? resolveRange(input.range) : context.range;
    const params = chartParams(input.sql, range);
    if (!Array.isArray(params)) return { input, result: { state: 'error', problem: params } };
    executed = { sql: input.sql, params };
  } else {
    const range = rangeOf(input.range, context);
    if ('code' in range) return { input, result: { state: 'error', problem: range } };
    executed = listQuery(input, range);
  }
  const [read] = await context.executor.run([query(executed.sql, executed.params, 'sql' in input ? (input.limit ?? 500) : undefined)], context.signal);
  if (!read.ok) return { input, executed, result: failed(read.problem) };
  const rows = read.value;
  if (rows.rows.length === 0) {
    const reason = 'sql' in input ? 'The SQL returned no rows in this range.' : `No ${input.signal} match this WHERE in this range.`;
    return { input, executed, result: { state: 'empty', data: { rows }, reason } };
  }
  if (!('signal' in input) || input.signal !== 'traces') return { input, executed, result: { state: 'populated', data: { rows } } };
  context.signal?.throwIfAborted();
  const at = shownColumns(input).length;
  const counted = countsQuery([...new Set(rows.rows.map((row) => String(row[at])))]);
  const [counts] = await context.executor.run([query(counted.sql, counted.params)], context.signal);
  const loaded: Loaded<QueryResult> = counts.ok ? { state: 'populated', data: counts.value } : failed(counts.problem);
  return { input, executed, result: { state: 'populated', data: { rows, counts: loaded } } };
}

/** Every retained span of one trace; past the ceiling the read is refused, never truncated. */
export function waterfallQuery(traceId: string): Executed {
  return {
    sql:
      "SELECT encode(span_id, 'hex') AS span_id, encode(parent_span_id, 'hex') AS parent_span_id, name, service_name, " +
      `start_time_unix_nano, duration_nano, status_code FROM ${SIGNALS.spans.table} WHERE trace_id = decode($1, 'hex') ORDER BY start_time_unix_nano, span_id`,
    params: [traceId]
  };
}

/** Load one exact trace. The range is navigation context and never narrows the evidence. */
export async function loadTraceWaterfall(input: TraceWaterfallInput, context: LoadContext): Promise<TraceWaterfallProps> {
  const plan = waterfallQuery(input.traceId);
  const [read] = await context.executor.run([query(plan.sql, plan.params, TRACE_SPANS)], context.signal);
  if (!read.ok) return { input, result: failed(read.problem) };
  if (read.value.rows.length === 0)
    return { input, result: { state: 'empty', data: read.value, reason: `No retained span you may read has trace_id ${input.traceId}.` } };
  return { input, result: { state: 'populated', data: read.value } };
}

/** The whole stored row of one span or log record. */
export function detailQuery(input: RecordDetailInput): Executed {
  if ('log' in input)
    return {
      sql:
        `SELECT * FROM ${SIGNALS.logs.table} WHERE time_unix_nano = $1::timestamptz AND observed_time_unix_nano = $2::timestamptz ` +
        'AND wyrd_request_id = $3',
      params: [input.log.time_unix_nano, input.log.observed_time_unix_nano, input.log.wyrd_request_id]
    };
  return {
    sql: `SELECT * FROM ${SIGNALS.spans.table} WHERE trace_id = decode($1, 'hex') AND span_id = decode($2, 'hex')`,
    params: [input.traceId, input.spanId]
  };
}

/** Load one record with its payload columns; a principal who may not read the table is refused. */
export async function loadRecordDetail(input: RecordDetailInput, context: LoadContext): Promise<RecordDetailProps> {
  const plan = detailQuery(input);
  const [read] = await context.executor.run([query(plan.sql, plan.params, 10)], context.signal);
  if (!read.ok) return { input, result: failed(read.problem) };
  if (read.value.rows.length === 0) {
    const what = 'log' in input ? `log record written by ${input.log.wyrd_request_id}` : `span ${input.spanId} of trace ${input.traceId}`;
    return { input, result: { state: 'empty', data: read.value, reason: `No retained ${what} you may read.` } };
  }
  return { input, result: { state: 'populated', data: read.value } };
}
