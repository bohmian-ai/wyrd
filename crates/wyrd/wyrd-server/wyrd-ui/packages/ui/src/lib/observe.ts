/**
 * Observe loaders: built-in reads of the canonical signal tables. Every SQL
 * text is assembled only from the fixed tables and column allowlists in
 * `SIGNALS`; every value, range bound and identity is a typed `$n` bind.
 */
import { failed, wyrdProblem, type Loaded, type Problem, type QueryParam, type QueryResult, type WireRequest } from './contract.js';
import { fmtDuration } from './format.js';
import { bucketFor, chartParams, resolveRange, type LoadContext } from './loaders.js';
import { columnIndex } from './plot.js';
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

  /**
   * Keep root spans in the range whose trace has a span matching every
   * predicate so far, so a filter on a child span's column, such as the
   * Card on a model call, finds its trace. The first clause is the range.
   */
  roots(table: string): this {
    const [window, ...rest] = this.clauses;
    const matching = rest.length ? [`trace_id IN (SELECT trace_id FROM ${table} WHERE ${this.sql})`] : [];
    this.clauses.splice(0, this.clauses.length, window, ROOT, ...matching);
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
const STAT_SOURCES: Record<Stat, { table: string; time: string; value: string; filter?: string; roots?: true; byCurrency?: true }> = {
  traces: { table: SIGNALS.spans.table, time: SIGNALS.spans.time, value: 'count(*)', roots: true },
  spans: { table: SIGNALS.spans.table, time: SIGNALS.spans.time, value: 'count(*)' },
  errors: { table: SIGNALS.spans.table, time: SIGNALS.spans.time, value: 'count(*)', filter: `status_code = ${STATUS_ERROR}` },
  duration_p95: { table: SIGNALS.spans.table, time: SIGNALS.spans.time, value: 'approx_percentile_cont(duration_nano, 0.95)', roots: true },
  invocations: { table: SIGNALS.spans.table, time: SIGNALS.spans.time, value: 'count(*)', filter: "gen_ai_operation_name = 'invoke_agent'" },
  // The gateway's metered usage: one row per call and usage entry, counting only token units, so
  // nested spans never count one call twice. Null, never zero, when no call recorded any.
  tokens: {
    table: '(SELECT started_at, workload_card_uid, subject_card_uid, unnest(usage) AS u FROM bifrost.gateway.calls) AS usage',
    time: 'started_at',
    value: "sum(CAST(u['quantity'] AS DECIMAL(38, 12)))",
    filter: "u['unit'] = 'tokens'"
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
    if (source.roots) binds.roots(source.table);
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
  if (!recorded) {
    const tables = [...new Set(input.stats.map((stat) => (stat === 'tokens' ? 'bifrost.gateway.calls usage' : STAT_SOURCES[stat].table)))].join(' or ');
    return { input, result: { state: 'empty', data: { stats }, reason: `Nothing recorded in ${tables} in this range${scopeText(input)}.` } };
  }
  return { input, result: { state: 'populated', data: { stats } } };
}

/** The ownership scope as a reason names it. */
function scopeText(input: StatStripInput): string {
  return input.scope ? ` for ${input.scope.column} = ${input.scope.uid}` : '';
}

/** Why a signal list is empty, naming the table, its event-time column and the WHERE that would have to match. */
function emptyList(signal: Signal, where: readonly Filter[] = []): string {
  const what = signal === 'traces' ? 'root spans' : 'rows';
  const matching = where.length ? ` matching ${where.map((filter) => filter.column).join(', ')}` : '';
  return `No ${what} in ${SIGNALS[signal].table} with ${SIGNALS[signal].time} in this range${matching}.`;
}

/** Facet counts of one column under the list's own WHERE and range. */
export function facetQuery(input: FacetRailInput, column: string, range: AbsoluteRange): Executed {
  const signal = SIGNALS[input.signal];
  const binds = Binds.over(signal.time, range);
  binds.where(input.where);
  // A trace facet counts the traces with a span of each value, under the same span-level WHERE.
  const n = input.signal === 'traces' ? 'count(DISTINCT trace_id)' : 'count(*)';
  return {
    sql: `SELECT ${column} AS value, ${n} AS n FROM ${signal.table} WHERE ${binds.sql} GROUP BY ${column} ORDER BY n DESC, value LIMIT ${FACET_VALUES + 1}`,
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
    return { input, result: { state: 'empty', data: { facets }, reason: emptyList(input.signal, input.where) } };
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
  binds.where(input.where);
  if (input.signal === 'traces') binds.roots(signal.table);
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
    const reason = 'sql' in input ? 'The SQL returned no rows in this range.' : emptyList(input.signal, input.where);
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

/**
 * An Arrow JSON nanosecond timestamp (`2026-10-20T14:05:00.123456789+00:00`)
 * as epoch nanoseconds, exact past the millisecond a `Date` keeps.
 * Returns `undefined` for a value that is not such a timestamp.
 */
export function parseNanos(value: unknown): bigint | undefined {
  if (typeof value !== 'string') return undefined;
  const match = /^([^.]+?)(?:\.(\d{1,9}))?([zZ]|[+-]\d\d:?\d\d)?$/.exec(value);
  const ms = match ? Date.parse(`${match[1]}${match[3] ?? 'Z'}`) : Number.NaN;
  if (!match || Number.isNaN(ms)) return undefined;
  return BigInt(ms) * 1_000_000n + BigInt((match[2] ?? '').padEnd(9, '0'));
}

/** Column headings for the canonical columns; any other column is headed by its name. */
const LABELS: Readonly<Record<string, string>> = {
  start_time_unix_nano: 'Start',
  time_unix_nano: 'Time',
  observed_time_unix_nano: 'Observed',
  name: 'Name',
  service_name: 'Service',
  duration_nano: 'Duration',
  status_code: 'Status',
  kind: 'Kind',
  severity_text: 'Severity',
  severity_number: 'Severity number',
  body_text: 'Message',
  exception_type: 'Exception',
  metric_name: 'Metric',
  metric_type: 'Type',
  unit: 'Unit',
  int_value: 'Integer value',
  double_value: 'Double value',
  workload_card_uid: 'Workload',
  subject_card_uid: 'Card',
  gen_ai_request_model: 'Model',
  gen_ai_operation_name: 'Operation'
};

/** A column's heading; the column name itself stays available as its title for writing WHERE. */
export function columnLabel(column: string): string {
  return LABELS[column] ?? column;
}

/** OTLP span status codes by name. */
export const STATUS_NAMES: Readonly<Record<number, string>> = { 0: 'unset', 1: 'ok', 2: 'error' };

/**
 * A row cell as text: null is `∅`, a nanosecond duration reads as time, a
 * timestamp as month-day and time to the millisecond (the exact value is the
 * caller's title), nested values as JSON.
 */
export function cellText(column: string, value: unknown): string {
  if (value === null || value === undefined) return '∅';
  if (column === 'duration_nano') return fmtDuration(Math.round(Number(value) / 1e6));
  const instant = column.endsWith('_unix_nano') && typeof value === 'string' && /^\d{4}-(\d\d-\d\d)T(\d\d:\d\d:\d\d)(\.\d{1,3})?/.exec(value);
  if (instant) return `${instant[1]} ${instant[2]}${instant[3] ?? ''}`;
  return typeof value === 'object' ? JSON.stringify(value) : String(value);
}

/** One span placed in its trace: depth under its parent and offset from the trace start. */
export interface WaterfallSpan {
  readonly spanId: string;
  readonly parentId: string | null;
  readonly depth: number;
  readonly name: string;
  readonly service: string | null;
  readonly startMs: number;
  readonly durationMs: number;
  readonly error: boolean;
  readonly children: number;
}

/**
 * Place spans depth-first in start order. A span whose parent is not
 * retained or not readable, or whose parents form a cycle, becomes a root,
 * so no span is dropped.
 */
export function spanTree(result: QueryResult): WaterfallSpan[] {
  const at = (name: string) => columnIndex(result, name);
  const [id, parent, name, service, start, duration, status] = [
    'span_id', 'parent_span_id', 'name', 'service_name', 'start_time_unix_nano', 'duration_nano', 'status_code'
  ].map(at);
  const ids = new Set(result.rows.map((row) => String(row[id])));
  const children = new Map<string | null, (readonly unknown[])[]>();
  for (const row of result.rows) {
    const key = row[parent] !== null && ids.has(String(row[parent])) ? String(row[parent]) : null;
    children.set(key, [...(children.get(key) ?? []), row]);
  }
  const origin = result.rows.reduce<bigint | undefined>((min, row) => {
    const ns = parseNanos(row[start]);
    return ns !== undefined && (min === undefined || ns < min) ? ns : min;
  }, undefined);
  const placed: WaterfallSpan[] = [];
  const seen = new Set<string>();
  const visit = (key: string | null, depth: number, rows = children.get(key) ?? []) => {
    for (const row of rows) {
      const spanId = String(row[id]);
      if (seen.has(spanId)) continue;
      seen.add(spanId);
      const ns = parseNanos(row[start]);
      placed.push({
        spanId,
        parentId: row[parent] === null ? null : String(row[parent]),
        depth,
        name: String(row[name]),
        service: row[service] === null ? null : String(row[service]),
        startMs: ns !== undefined && origin !== undefined ? Number(ns - origin) / 1e6 : 0,
        durationMs: Number(row[duration]) / 1e6,
        error: Number(row[status]) === STATUS_ERROR,
        children: children.get(spanId)?.length ?? 0
      });
      visit(spanId, depth + 1);
    }
  };
  visit(null, 0);
  // A parent cycle is unreachable from any root; its spans still show, as roots.
  visit(null, 0, result.rows.filter((row) => !seen.has(String(row[id]))));
  return placed;
}
