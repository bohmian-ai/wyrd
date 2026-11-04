/**
 * Observe loaders: built-in reads of the canonical signal tables. Every SQL
 * text is assembled only from the fixed tables and column allowlists in
 * `SIGNALS`; every value, range bound and identity is a typed `$n` bind.
 */
import { failed, wyrdProblem, type Loaded, type Problem, type QueryParam, type QueryResult, type WireRequest } from './contract.js';
import { fmtDuration, fmtNumber, parseInstant } from './format.js';
import { bucketFor, bucketWidth, chartParams, resolveRange, shapeChart, type ChartProps, type LoadContext } from './loaders.js';
import { columnIndex, numeric } from './plot.js';
import {
  IDS,
  SIGNALS,
  TIES,
  cursorColumns,
  listSort,
  logStats,
  type AbsoluteRange,
  type ChartInput,
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
  readonly result: Loaded<{ readonly rows: QueryResult; readonly counts?: Loaded<QueryResult>; readonly page?: Page }>;
}

/** The cursors of the pages beside a signal list's page; an absent side has no rows. */
export interface Page {
  readonly older?: readonly QueryParam[];
  readonly newer?: readonly QueryParam[];
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
export const LIST_LIMIT = 50;
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

  /** AND one server-normalized predicate; it binds nothing and names only the table's own columns. */
  predicate(sql: string | undefined): this {
    if (sql) this.clauses.push(`(${sql})`);
    return this;
  }

  /** AND each WHERE clause; the column is already one of the signal's allowlisted names, and an id binds as hex. */
  where(filters: readonly Filter[] = []): this {
    for (const { column, value, not } of filters) {
      const bound = () => ((IDS as readonly string[]).includes(column) ? `decode(${this.bind(value)}, 'hex')` : this.bind(value));
      if (value === null) this.clauses.push(`${column} IS ${not ? 'NOT ' : ''}NULL`);
      else this.clauses.push(`${column} ${not ? 'IS DISTINCT FROM' : '='} ${bound()}`);
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

/**
 * Validate a panel's `where_sql` against its signal table before any read
 * uses it: the normalized predicate, `undefined` for none, or the refusal
 * that stops the panel with no dependent read.
 */
async function predicateFor(table: string, whereSql: string | undefined, context: LoadContext): Promise<{ sql?: string } | Loaded<never>> {
  if (!whereSql?.trim()) return {};
  const dot = table.lastIndexOf('.');
  const [checked] = await context.executor.run(
    [{ op: 'bifrost.validate_predicate', namespace: table.slice(0, dot), name: table.slice(dot + 1), expression: whereSql }],
    context.signal
  );
  if (!checked.ok) return failed(checked.problem);
  context.signal?.throwIfAborted();
  return { sql: checked.value.expression || undefined };
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
  logs: { table: SIGNALS.logs.table, time: SIGNALS.logs.time, value: 'count(*)' },
  // OTLP severity numbers 17-24 are ERROR and FATAL; the recorded number decides, never the text or a name.
  log_errors: { table: SIGNALS.logs.table, time: SIGNALS.logs.time, value: 'count(*)', filter: 'severity_number >= 17' },
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
export function statQueries(stat: Stat, input: StatStripInput, range: AbsoluteRange, bucket: string, predicate?: string): [Executed, Executed] {
  const source = STAT_SOURCES[stat];
  const filtered = source.table === SIGNALS.spans.table || source.table === SIGNALS.logs.table;
  const plan = (series: boolean) => {
    const binds = Binds.over(source.time, range);
    const bin = series ? `date_bin(${binds.bind(bucket)}::interval, ${source.time})` : '';
    if (source.filter) binds.clauses.push(source.filter);
    if (input.scope) binds.clauses.push(`${input.scope.column} = ${binds.bind(input.scope.uid)}`);
    if (filtered) binds.where(input.where).predicate(predicate);
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
  const predicate = await predicateFor(SIGNALS[logStats(input.stats) ? 'logs' : 'spans'].table, input.where_sql, context);
  if ('state' in predicate) return { input, result: predicate };
  const plans = input.stats.map((stat) => statQueries(stat, input, range, bucketFor(range), predicate.sql));
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
export function facetQuery(input: FacetRailInput, column: string, range: AbsoluteRange, predicate?: string): Executed {
  const signal = SIGNALS[input.signal];
  const binds = Binds.over(signal.time, range);
  binds.where(input.where).predicate(predicate);
  // A trace facet counts the traces with a span of each value, under the same span-level WHERE.
  const n = input.signal === 'traces' ? 'count(DISTINCT trace_id)' : 'count(*)';
  // A severity text value reads the highest severity number recorded with it, so its level comes from the number.
  const level = input.signal === 'logs' && column === 'severity_text' ? ', max(severity_number) AS severity_number' : '';
  return {
    sql: `SELECT ${column} AS value, ${n} AS n${level} FROM ${signal.table} WHERE ${binds.sql} GROUP BY ${column} ORDER BY n DESC, value LIMIT ${FACET_VALUES + 1}`,
    params: binds.params
  };
}

/** Load every facet in one batch; one refusal refuses the rail. */
export async function loadFacetRail(input: FacetRailInput, context: LoadContext): Promise<FacetRailProps> {
  const range = rangeOf(input.range, context);
  if ('code' in range) return { input, result: { state: 'error', problem: range } };
  const predicate = await predicateFor(SIGNALS[input.signal].table, input.where_sql, context);
  if ('state' in predicate) return { input, result: predicate };
  const plans = input.facets.map((column) => facetQuery(input, column, range, predicate.sql));
  const results = await context.executor.run(plans.map((plan) => query(plan.sql, plan.params)), context.signal);
  const failure = firstFailure(results);
  if (failure) return { input, result: failure };
  const facets = input.facets.map((column, index) => ({ column, values: (results[index] as { value: QueryResult }).value }));
  if (facets.every(({ values }) => values.rows.length === 0))
    return { input, result: { state: 'empty', data: { facets }, reason: emptyList(input.signal, input.where) } };
  return { input, result: { state: 'populated', data: { facets } } };
}

/**
 * Columns a list reads after its shown columns, by signal: the row's identity
 * and the cues its cells read, a span's parent and a record's severity number.
 */
export const KEY_COLUMNS: Record<Signal, readonly string[]> = {
  traces: ['trace_id', 'span_id'],
  spans: ['trace_id', 'span_id', 'parent_span_id'],
  logs: ['trace_id', 'span_id', 'observed_time_unix_nano', 'wyrd_request_id', 'severity_number'],
  metrics: []
};

/** The columns a signal list shows. */
export function shownColumns(input: Extract<RecordTableInput, { signal: Signal }>): readonly string[] {
  return input.columns ?? SIGNALS[input.signal].columns;
}

/** One column of a list's order, as the query reads it. */
interface OrderKey {
  readonly column: string;
  readonly desc: boolean;
  /** Only the sort column may be empty; it sorts last, so first when a page reads backward. */
  readonly nullable: boolean;
}

/** A column as the order compares it: an id by its lowercase hex, which orders as its bytes. */
const ordered = (column: string) => ((IDS as readonly string[]).includes(column) ? `encode(${column}, 'hex')` : column);

/**
 * A list's order: the sort column, event time newest first, then the ties
 * ascending. A page before a row reads the reverse order.
 */
function orderKeys(signal: Signal, input: Extract<RecordTableInput, { signal: Signal }>): readonly OrderKey[] {
  const sort = listSort(signal, input.sort);
  const backward = input.before !== undefined;
  return cursorColumns(signal, input.sort).map((column, index) => {
    const desc = column === sort.column ? sort.desc : column === SIGNALS[signal].time;
    return { column, desc: desc !== backward, nullable: index === 0 && column !== SIGNALS[signal].time };
  });
}

/**
 * The rows strictly beyond `values` in the order of `keys`, as a keyset
 * predicate: beyond the first key, or equal to it and beyond the rest. An
 * empty value sorts last forward and first backward.
 */
function beyond(binds: Binds, keys: readonly OrderKey[], values: readonly QueryParam[], backward: boolean): string {
  const placeholders = keys.map(({ column }, index) =>
    values[index] === null ? '' : column.endsWith('_unix_nano') ? `${binds.bind(values[index])}::timestamptz` : binds.bind(values[index])
  );
  let rest = '';
  for (let index = keys.length - 1; index >= 0; index -= 1) {
    const { column, desc, nullable } = keys[index];
    const value = values[index];
    const left = ordered(column);
    const placeholder = placeholders[index];
    const past = value === null ? (nullable && backward ? `${left} IS NOT NULL` : '') : `${left} ${desc ? '<' : '>'} ${placeholder}${nullable && !backward ? ` OR ${left} IS NULL` : ''}`;
    const equal = value === null ? `${left} IS NULL` : `${left} = ${placeholder}`;
    rest = `(${[past, rest ? `(${equal} AND ${rest})` : ''].filter(Boolean).join(' OR ') || 'FALSE'})`;
  }
  return rest;
}

/**
 * One page of one signal, shown columns first and then its other key and
 * cursor columns, ids as lowercase hex. Traces, spans and logs read in their
 * sort order one row past the page, which says whether another page
 * follows; metric points read their newest rows.
 */
export function listQuery(input: Extract<RecordTableInput, { signal: Signal }>, range: AbsoluteRange, predicate?: string): Executed {
  const signal = SIGNALS[input.signal];
  const paged = TIES[input.signal].length > 0;
  const binds = Binds.over(signal.time, range);
  binds.where(input.where).predicate(predicate);
  if (input.signal === 'traces') binds.roots(signal.table);
  const keys = orderKeys(input.signal, input);
  const cursor = input.after ?? input.before;
  if (paged && cursor) binds.clauses.push(beyond(binds, keys, cursor, input.before !== undefined));
  const read = [...new Set([...KEY_COLUMNS[input.signal], ...(paged ? cursorColumns(input.signal, input.sort) : [])])]
    .filter((key) => !shownColumns(input).includes(key))
    .map((key) => ((IDS as readonly string[]).includes(key) || key === 'parent_span_id' ? `encode(${key}, 'hex') AS ${key}` : key));
  const shown = shownColumns(input).map((column) => {
    if (input.signal !== 'metrics') return column;
    if (EXACT.includes(column)) return `CAST(${column} AS VARCHAR) AS ${column}`;
    return NESTED_EXACT[column] ? `${NESTED_EXACT[column]} AS ${column}` : column;
  });
  const order = paged
    ? keys.map(({ column, desc, nullable }) => `${ordered(column)} ${desc ? 'DESC' : 'ASC'}${nullable ? ` NULLS ${input.before ? 'FIRST' : 'LAST'}` : ''}`).join(', ')
    : `${signal.time} DESC`;
  const limit = (input.limit ?? LIST_LIMIT) + (paged ? 1 : 0);
  return { sql: `SELECT ${[...shown, ...read].join(', ')} FROM ${signal.table} WHERE ${binds.sql} ORDER BY ${order} LIMIT ${limit}`, params: binds.params };
}

/**
 * Metric integer columns a list reads as exact decimal text: JSON numbers
 * cannot carry every int64 or uint64, so the server spells them.
 */
const EXACT: readonly string[] = ['int_value', 'histogram_count', 'summary_count', 'exponential_zero_count'];

/**
 * Nested metric counts a list reads as exact decimal text with their
 * structure kept: OTLP counts are uint64, which JSON numbers cannot carry.
 */
const NESTED_EXACT: Readonly<Record<string, string>> = {
  bucket_counts: 'CAST(bucket_counts AS VARCHAR[])',
  positive_buckets:
    "CASE WHEN positive_buckets IS NULL THEN NULL ELSE named_struct('offset', positive_buckets['offset'], 'bucket_counts', CAST(positive_buckets['bucket_counts'] AS VARCHAR[])) END",
  negative_buckets:
    "CASE WHEN negative_buckets IS NULL THEN NULL ELSE named_struct('offset', negative_buckets['offset'], 'bucket_counts', CAST(negative_buckets['bucket_counts'] AS VARCHAR[])) END"
};

/** Metric types a series chart plots; histogram and summary points are listed, never plotted. */
const PLOTTED = "metric_type IN ('gauge', 'sum')";

/** Aggregations a metric chart may apply across series at each recorded instant. Percentiles are not offered. */
export const METRIC_AGGREGATIONS = ['sum', 'mean', 'min', 'max'] as const;
/** One offered metric aggregation. */
export type MetricAggregation = (typeof METRIC_AGGREGATIONS)[number];

/** SQL of each aggregation over exact integers. */
const AGGREGATE: Record<MetricAggregation, string> = { sum: 'sum', mean: 'avg', min: 'min', max: 'max' };

/** A point's recorded meaning: points that differ in any of these never share a series. */
const MEANING = ['metric_type', 'unit', 'aggregation_temporality', 'is_monotonic'] as const;

/** The recorded resource, scope and point dimensions that identify an unaggregated series; Variants read as their JSON text. */
const DIMENSIONS: readonly (readonly [string, string])[] = [
  ['resource', 'to_json(resource_attributes)'],
  ['scope_name', 'scope_name'],
  ['scope_version', 'scope_version'],
  ['scope', 'to_json(scope_attributes)'],
  ['attributes', 'to_json(attributes)']
];

/** The read of one metric's series and their exact summary. */
export interface MetricInput {
  readonly metric: string;
  readonly where?: readonly Filter[];
  readonly where_sql?: string;
  /** Apply this aggregation across series at each recorded instant; without it every recorded series stays separate. */
  readonly aggregation?: MetricAggregation;
  /** Keep one series per value of this metrics column: with an aggregation for gauges and sums, on its own for a histogram. */
  readonly by?: string;
  readonly range?: Range;
}

/**
 * Props of one metric: the Chart of its series, the typed identity columns
 * that key them, and the exact per-series summary under the same identity
 * and sample set.
 */
export interface MetricProps {
  readonly input: MetricInput;
  readonly chart: ChartProps;
  readonly identity: readonly string[];
  readonly summary: Loaded<QueryResult>;
  /** Present when the metric records explicit-bucket histogram points in the range. */
  readonly histogram?: HistogramProps;
}

/**
 * One histogram metric's plots: the mean and bucket-estimated percentiles
 * over time, the count rate over time, and the bucket distribution over the
 * whole range. Each is an ordinary Chart over the same recorded points.
 */
export interface HistogramProps {
  readonly latency: ChartProps;
  readonly rate: ChartProps;
  readonly distribution: ChartProps;
}

/**
 * The chart and summary reads of one metric. Both select the same samples:
 * `at`, the identity columns, `i` (exact integers as DECIMAL) and `d`
 * (doubles). With an aggregation, the samples are first aggregated once per
 * instant and identity; integers stay exact unless a double shares the group.
 * The summary's Latest, Max and Mean are server text: Max orders by double
 * value and breaks a rounded tie exactly, comparing the integer with the
 * double as decimals (a double that ties an int64 is integral), and Mean is
 * the exact decimal mean unless a double is present.
 */
export function metricQueries(input: MetricInput, range: AbsoluteRange, predicate?: string): { identity: readonly string[]; chart: Executed; summary: Executed } {
  const filters = SIGNALS.metrics.filters as readonly string[];
  for (const { column } of input.where ?? []) if (!filters.includes(column)) throw new Error(`${column} is not a metrics column`);
  if (input.by !== undefined && !filters.includes(input.by)) throw new Error(`${input.by} is not a metrics column`);
  const binds = Binds.over(SIGNALS.metrics.time, range);
  binds.clauses.push(`metric_name = ${binds.bind(input.metric)}`, PLOTTED);
  binds.where((input.where ?? []).filter(({ column, not }) => column !== 'metric_name' || not)).predicate(predicate);
  const dimensions: (readonly [string, string])[] = input.aggregation
    ? [...MEANING.map((name) => [name, name] as const), ...(input.by ? [[input.by, input.by] as const] : [])]
    : [...MEANING.map((name) => [name, name] as const), ...DIMENSIONS];
  const identity = dimensions.map(([name]) => name);
  const names = identity.join(', ');
  let samples =
    `SELECT ${SIGNALS.metrics.time} AS at, ${dimensions.map(([name, sql]) => (name === sql ? name : `${sql} AS ${name}`)).join(', ')}, ` +
    `CAST(int_value AS DECIMAL(38, 0)) AS i, double_value AS d FROM ${SIGNALS.metrics.table} WHERE ${binds.sql}`;
  if (input.aggregation) {
    const f = AGGREGATE[input.aggregation];
    samples =
      `SELECT at, ${names}, CASE WHEN count(d) = 0 THEN ${f}(i) END AS i, CASE WHEN count(d) > 0 THEN ${f}(COALESCE(CAST(i AS DOUBLE), d)) END AS d ` +
      `FROM (${samples}) GROUP BY at, ${names}`;
  }
  const valued = `SELECT *, COALESCE(CAST(i AS DOUBLE), d) AS v, COALESCE(CAST(i AS VARCHAR), CAST(d AS VARCHAR)) AS t FROM (${samples})`;
  return {
    identity,
    chart: { sql: `SELECT at, ${names}, v AS value FROM (${valued}) ORDER BY at`, params: binds.params },
    summary: {
      sql:
        `SELECT ${names}, count(*) AS points, last_value(t ORDER BY at) AS latest, first_value(t ORDER BY v DESC, COALESCE(i, CASE WHEN abs(d) < 1e19 THEN CAST(d AS DECIMAL(38, 0)) END) DESC NULLS LAST) AS max, ` +
        `CASE WHEN count(d) = 0 THEN CAST(avg(i) AS VARCHAR) ELSE CAST(avg(v) AS VARCHAR) END AS mean ` +
        `FROM (${valued}) GROUP BY ${names} ORDER BY ${names}`,
      params: binds.params
    }
  };
}

/** Series a metric summary reads; past it the summary reports a partial result. */
const METRIC_SERIES = 1000;

/**
 * Load one metric's chart and exact summary in one batch over one range and
 * one validated predicate; a refused predicate issues neither read.
 */
export async function loadMetric(input: MetricInput, context: LoadContext): Promise<MetricProps> {
  const stopped = (state: Loaded<never>): MetricProps => ({ input, identity: [], summary: state, chart: { input: metricChart(input), result: state } });
  const range = rangeOf(input.range, context);
  if ('code' in range) return stopped({ state: 'error', problem: range });
  const predicate = await predicateFor(SIGNALS.metrics.table, input.where_sql, context);
  if ('state' in predicate) return stopped(predicate);
  const plan = metricQueries(input, range, predicate.sql);
  const points = histogramQuery(input, range, predicate.sql);
  const [chart, summary, histogram] = await context.executor.run(
    [query(plan.chart.sql, plan.chart.params), query(plan.summary.sql, plan.summary.params, METRIC_SERIES), query(points.sql, points.params, HISTOGRAM_POINTS)],
    context.signal
  );
  const reason = `No gauge or sum point of ${input.metric} in ${SIGNALS.metrics.table} in this range.`;
  const loaded = (read: (typeof chart)): Loaded<QueryResult> =>
    !read.ok ? failed(read.problem) : read.value.rows.length ? { state: 'populated', data: read.value } : { state: 'empty', data: read.value, reason };
  return {
    input,
    identity: plan.identity,
    chart: { input: { ...metricChart(input), sql: plan.chart.sql, range }, executed: plan.chart, result: loaded(chart) },
    summary: loaded(summary),
    ...(!histogram.ok ? { histogram: histogramProps(input, range, points, failed(histogram.problem)) } : histogram.value.rows.length ? { histogram: histogramProps(input, range, points, histogram.value) } : {})
  };
}

/** The metric's chart definition before its SQL is planned. */
function metricChart(input: MetricInput): Extract<ChartInput, { type: 'timeseries' }> {
  const how = input.aggregation ? `${input.aggregation} per instant${input.by ? ` by ${input.by}` : ''}` : 'each recorded series';
  return { type: 'timeseries', title: `${input.metric} · ${how} · plotted as double precision`, sql: '-', x: 'at', y: ['value'] };
}

/** Explicit-bucket histogram points, the only histogram kind plotted; exponential histograms and summaries stay listed. */
const HISTOGRAM = "metric_type = 'histogram'";
/** Histogram points one plot reads; past it the plot reports a partial result. */
const HISTOGRAM_POINTS = 20_000;
/** The percentiles estimated from a histogram's buckets. */
export const HISTOGRAM_PERCENTILES = [50, 95, 99] as const;

/**
 * The read of one histogram metric's recorded points in time order: each
 * point's instant, start, recorded meaning, optional grouping column, the
 * dimensions that identify its series, its count and sum, its bounds, and
 * its bucket counts as exact text.
 */
export function histogramQuery(input: MetricInput, range: AbsoluteRange, predicate?: string): Executed {
  const filters = SIGNALS.metrics.filters as readonly string[];
  for (const { column } of input.where ?? []) if (!filters.includes(column)) throw new Error(`${column} is not a metrics column`);
  if (input.by !== undefined && !filters.includes(input.by)) throw new Error(`${input.by} is not a metrics column`);
  const binds = Binds.over(SIGNALS.metrics.time, range);
  binds.clauses.push(`metric_name = ${binds.bind(input.metric)}`, HISTOGRAM);
  binds.where((input.where ?? []).filter(({ column, not }) => column !== 'metric_name' || not)).predicate(predicate);
  const group = [...MEANING, ...(input.by ? [input.by] : [])];
  return {
    sql:
      `SELECT ${SIGNALS.metrics.time} AS at, start_time_unix_nano AS started, ${group.join(', ')}, ` +
      `${DIMENSIONS.map(([name, sql]) => `${sql} AS ${name}`).join(', ')}, ` +
      `histogram_count AS count, histogram_sum AS sum, explicit_bounds AS bounds, ${NESTED_EXACT.bucket_counts} AS buckets ` +
      `FROM ${SIGNALS.metrics.table} WHERE ${binds.sql} ORDER BY at`,
    params: binds.params
  };
}

/** One histogram window or range total: summed counts, sum and bucket counts. */
interface Tally {
  count: number;
  sum: number;
  buckets: number[];
}

/** One merged group of series sharing recorded meaning, bounds and the grouping value. */
interface Group {
  readonly label: string;
  readonly bounds: readonly number[];
  readonly windows: Map<number, Tally>;
  readonly total: Tally;
}

/**
 * The value at percentile `q` (0–100) of bucket counts over `bounds`, by
 * linear interpolation inside the bucket holding the rank, as Prometheus
 * `histogram_quantile` does; the first bucket starts at 0. A rank in the
 * unbounded last bucket returns that bucket's lower bound with `atLeast`.
 * No counts, or no finite bound to report, return undefined.
 */
export function bucketPercentile(bounds: readonly number[], buckets: readonly number[], q: number): { value: number; atLeast: boolean } | undefined {
  const total = buckets.reduce((sum, count) => sum + count, 0);
  if (total <= 0) return undefined;
  const rank = (q / 100) * total;
  let below = 0;
  for (let index = 0; index < buckets.length; index++) {
    const count = buckets[index];
    if (count > 0 && below + count >= rank) {
      if (index >= bounds.length) return bounds.length ? { value: bounds[bounds.length - 1], atLeast: true } : undefined;
      const lower = index === 0 ? Math.min(0, bounds[0]) : bounds[index - 1];
      return { value: lower + ((bounds[index] - lower) * (rank - below)) / count, atLeast: false };
    }
    below += count;
  }
  return undefined;
}

/** A bucket's caption from its bounds: `≤ 0.005`, `0.005–0.01`, `> 10`. */
function bucketLabel(bounds: readonly number[], index: number): string {
  if (!bounds.length) return 'all';
  if (index === 0) return `≤ ${fmtNumber(bounds[0])}`;
  if (index >= bounds.length) return `> ${fmtNumber(bounds[bounds.length - 1])}`;
  return `${fmtNumber(bounds[index - 1])}–${fmtNumber(bounds[index])}`;
}

/** The windowed plots of one histogram read, plus whether any percentile hit the unbounded bucket. */
export interface HistogramWindows {
  readonly latency: QueryResult;
  readonly rate: QueryResult;
  readonly distribution: QueryResult;
  readonly atLeast: boolean;
}

/**
 * Fold recorded histogram points into the chart's time windows of `range`.
 *
 * A cumulative point counts its increase over the previous point of the same
 * series; a changed start or a smaller count is a reset, and the point then
 * counts from zero. A series' first point counts in full only when the series
 * started inside the range; otherwise it is the baseline. Delta points count
 * as recorded. Series merge only within one group: identical meaning, bounds
 * and grouping value. A window with no counts is a gap.
 */
export function histogramWindows(read: QueryResult, range: AbsoluteRange, by?: string): HistogramWindows {
  const at = (name: string) => columnIndex(read, name);
  const [ti, si, ci, sumi, bi, ki, tempi] = ['at', 'started', 'count', 'sum', 'bounds', 'buckets', 'aggregation_temporality'].map(at);
  const groupIndexes = [...MEANING, ...(by ? [by] : [])].map(at);
  const seriesIndexes = DIMENSIONS.map(([name]) => at(name));
  const from = Date.parse(range.from);
  const width = bucketWidth(range);
  const previous = new Map<string, { started: number; count: number; sum: number; buckets: number[] }>();
  const groups = new Map<string, Group>();
  const add = (tally: Tally, count: number, sum: number, buckets: readonly number[]) => {
    tally.count += count;
    tally.sum += sum;
    buckets.forEach((value, index) => (tally.buckets[index] = (tally.buckets[index] ?? 0) + value));
  };
  for (const row of read.rows) {
    const time = parseInstant(row[ti]);
    const started = parseInstant(row[si]);
    const count = numeric(row[ci] ?? 0);
    const sum = numeric(row[sumi] ?? 0);
    const bounds = ((row[bi] ?? []) as unknown[]).map(numeric);
    const buckets = ((row[ki] ?? []) as unknown[]).map(numeric);
    const groupCells = groupIndexes.map((index) => row[index]);
    const key = JSON.stringify([groupCells, bounds]);
    const series = JSON.stringify([key, seriesIndexes.map((index) => row[index])]);
    let counted: { count: number; sum: number; buckets: readonly number[] } | undefined = { count, sum, buckets };
    if (numeric(row[tempi]) === 2) {
      const last = previous.get(series);
      previous.set(series, { started, count, sum, buckets });
      if (last && last.started === started && count >= last.count && last.buckets.length === buckets.length)
        counted = { count: count - last.count, sum: sum - last.sum, buckets: buckets.map((value, index) => value - last.buckets[index]) };
      else if (!last && started < from) counted = undefined;
    }
    if (!counted || counted.count <= 0) continue;
    const label = by ? `${String(row[at(by)] ?? '∅')}` : '';
    const group = groups.get(key) ?? { label, bounds, windows: new Map<number, Tally>(), total: { count: 0, sum: 0, buckets: [] } };
    groups.set(key, group);
    const window = Math.floor((time - from) / width);
    const tally = group.windows.get(window) ?? { count: 0, sum: 0, buckets: [] };
    group.windows.set(window, tally);
    add(tally, counted.count, counted.sum, counted.buckets);
    add(group.total, counted.count, counted.sum, counted.buckets);
  }
  const several = groups.size > 1;
  const named = (measure: string, group: Group, index: number) =>
    several ? `${measure} · ${group.label || `bounds ${index + 1}`}` : measure;
  const latency: unknown[][] = [];
  const rate: unknown[][] = [];
  const distribution: unknown[][] = [];
  let atLeast = false;
  [...groups.values()].forEach((group, index) => {
    for (const [window, tally] of [...group.windows].sort(([a], [b]) => a - b)) {
      const instant = new Date(from + window * width).toISOString();
      rate.push([instant, named('per second', group, index), tally.count / (width / 1000)]);
      latency.push([instant, named('mean', group, index), tally.sum / tally.count]);
      for (const q of HISTOGRAM_PERCENTILES) {
        const estimate = bucketPercentile(group.bounds, tally.buckets, q);
        if (!estimate) continue;
        atLeast ||= estimate.atLeast;
        latency.push([instant, named(`p${q} estimated`, group, index), estimate.value]);
      }
    }
    group.total.buckets.forEach((count, bucket) =>
      distribution.push([several ? `${group.label || `bounds ${index + 1}`} ${bucketLabel(group.bounds, bucket)}` : bucketLabel(group.bounds, bucket), count])
    );
  });
  const columns = (...names: [string, string][]) => names.map(([name, data_type]) => ({ name, data_type, nullable: false }));
  const timed = columns(['at', 'Utf8'], ['series', 'Utf8'], ['value', 'Float64']);
  return {
    latency: { columns: timed, rows: latency, terminal: read.terminal },
    rate: { columns: timed, rows: rate, terminal: read.terminal },
    distribution: { columns: columns(['bucket', 'Utf8'], ['count', 'Float64']), rows: distribution, terminal: read.terminal },
    atLeast
  };
}

/** The three histogram Charts of one read, or one shared failure. */
function histogramProps(input: MetricInput, range: AbsoluteRange, executed: Executed, read: QueryResult | Loaded<never>): HistogramProps {
  const how = `${input.by ? `by ${input.by} · ` : ''}explicit-bucket histogram`;
  const charts = {
    latency: { type: 'timeseries', title: `${input.metric} · mean and percentiles estimated from buckets · ${how}`, sql: executed.sql, x: 'at', y: ['value'], series: 'series', range },
    rate: { type: 'timeseries', title: `${input.metric} · recordings per second · ${how}`, sql: executed.sql, x: 'at', y: ['value'], series: 'series', range },
    distribution: { type: 'bar', title: `${input.metric} · bucket counts over the range · ${how}`, sql: executed.sql, x: 'bucket', y: ['count'] }
  } as const satisfies Record<keyof HistogramProps, ChartInput>;
  if ('state' in read) {
    return { latency: { input: charts.latency, executed, result: read }, rate: { input: charts.rate, executed, result: read }, distribution: { input: charts.distribution, executed, result: read } };
  }
  const windows = histogramWindows(read, range, input.by);
  const unit = read.rows.length ? read.rows[0][columnIndex(read, 'unit')] : undefined;
  const reason = `No histogram counts of ${input.metric} in this range.`;
  const loaded = (data: QueryResult): Loaded<QueryResult> => (data.rows.length ? { state: 'populated', data } : { state: 'empty', data, reason });
  const capped = windows.atLeast ? ' · a percentile in the unbounded top bucket shows that bucket\'s lower bound, at least' : '';
  return {
    latency: { input: { ...charts.latency, title: `${charts.latency.title}${capped}`, ...(unit ? { unit: String(unit) } : {}) }, executed, result: loaded(windows.latency) },
    rate: { input: charts.rate, executed, result: loaded(windows.rate) },
    distribution: { input: charts.distribution, executed, result: loaded(windows.distribution) }
  };
}

/** The metrics the rail offers under the view's WHERE: name, type and unit of each, by name. */
export function metricNamesQuery(where: readonly Filter[], range: AbsoluteRange, predicate?: string): Executed {
  const binds = Binds.over(SIGNALS.metrics.time, range);
  binds.where(where.filter(({ column }) => column !== 'metric_name')).predicate(predicate);
  return {
    sql: `SELECT metric_name, metric_type, unit, count(*) AS points FROM ${SIGNALS.metrics.table} WHERE ${binds.sql} GROUP BY metric_name, metric_type, unit ORDER BY metric_name, metric_type, unit`,
    params: binds.params
  };
}

/** Load the metric rail's names; a refused predicate issues no read. */
export async function loadMetricNames(input: { readonly where?: readonly Filter[]; readonly where_sql?: string; readonly range?: Range }, context: LoadContext): Promise<Loaded<QueryResult>> {
  const range = rangeOf(input.range, context);
  if ('code' in range) return { state: 'error', problem: range };
  const predicate = await predicateFor(SIGNALS.metrics.table, input.where_sql, context);
  if ('state' in predicate) return predicate;
  const plan = metricNamesQuery(input.where ?? [], range, predicate.sql);
  const [read] = await context.executor.run([query(plan.sql, plan.params, 500)], context.signal);
  if (!read.ok) return failed(read.problem);
  return read.value.rows.length ? { state: 'populated', data: read.value } : { state: 'empty', data: read.value, reason: emptyList('metrics', input.where) };
}

/** The three overview charts of Traces and Spans, and the totals their headlines read. */
export interface TraceOverviewProps {
  readonly spans: ChartProps;
  readonly errors: ChartProps;
  readonly duration: ChartProps;
  /** `spans, traces, error_traces, span_p95_ms, p95_ms, p50_ms` over the whole range, or the state that stopped them; `span_p95_ms` is over every span, the others over roots. */
  readonly totals: Loaded<QueryResult>;
}

/**
 * The trace overview reads: spans and error spans per bucket, root duration
 * p95 and p50 per bucket, and the range totals, all under one range, WHERE
 * and predicate. Durations read in milliseconds.
 */
export function traceOverviewQueries(input: { readonly where?: readonly Filter[] }, range: AbsoluteRange, bucket: string, predicate?: string) {
  const spans = SIGNALS.spans;
  const plan = (select: string, roots: boolean, series: boolean) => {
    const binds = Binds.over(spans.time, range);
    const bin = series ? `date_bin(${binds.bind(bucket)}::interval, ${spans.time})` : '';
    binds.where(input.where).predicate(predicate);
    if (roots) binds.roots(spans.table);
    const sql = series
      ? `SELECT ${bin} AS at, ${select} FROM ${spans.table} WHERE ${binds.sql} GROUP BY ${bin} ORDER BY at`
      : `SELECT ${select} FROM ${spans.table} WHERE ${binds.sql}`;
    return { sql, params: binds.params };
  };
  const p = (share: number) => `approx_percentile_cont(duration_nano, ${share}) / 1e6`;
  return {
    spans: plan('count(*) AS spans', false, true),
    errors: plan(`count(*) FILTER (WHERE status_code = ${STATUS_ERROR}) AS errors`, false, true),
    duration: plan(`${p(0.95)} AS p95, ${p(0.5)} AS p50`, true, true),
    totals: plan(`count(*) AS spans, count(DISTINCT trace_id) AS traces, count(DISTINCT trace_id) FILTER (WHERE status_code = ${STATUS_ERROR}) AS error_traces, ${p(0.95)} AS span_p95_ms`, false, false),
    durations: plan(`${p(0.95)} AS p95_ms, ${p(0.5)} AS p50_ms`, true, false)
  };
}

/** Load the trace overview in one batch; one refused predicate refuses every chart with no dependent read. */
export async function loadTraceOverview(input: { readonly where?: readonly Filter[]; readonly where_sql?: string; readonly range?: Range }, context: LoadContext): Promise<TraceOverviewProps> {
  const charts: Record<'spans' | 'errors' | 'duration', ChartInput> = {
    spans: { type: 'timeseries', title: 'Spans', sql: '-', x: 'at', y: ['spans'] },
    errors: { type: 'timeseries', title: 'Errors', sql: '-', x: 'at', y: ['errors'] },
    duration: { type: 'timeseries', title: 'Duration', sql: '-', x: 'at', y: ['p95', 'p50'], unit: 'ms' }
  };
  const all = (state: Loaded<never>): TraceOverviewProps => ({
    spans: { input: charts.spans, result: state },
    errors: { input: charts.errors, result: state },
    duration: { input: charts.duration, result: state },
    totals: state
  });
  const range = rangeOf(input.range, context);
  if ('code' in range) return all({ state: 'error', problem: range });
  const predicate = await predicateFor(SIGNALS.spans.table, input.where_sql, context);
  if ('state' in predicate) return all(predicate);
  const plans = traceOverviewQueries(input, range, bucketFor(range), predicate.sql);
  const order = ['spans', 'errors', 'duration', 'totals', 'durations'] as const;
  const results = await context.executor.run(order.map((name) => query(plans[name].sql, plans[name].params)), context.signal);
  const [spans, errors, duration, totals, durations] = results;
  const chart = (name: 'spans' | 'errors' | 'duration', read: (typeof results)[number]): ChartProps => ({
    input: { ...charts[name], sql: plans[name].sql, range },
    executed: plans[name],
    result: read.ok ? shapeChart({ ...charts[name], sql: plans[name].sql, range }, read.value) : failed(read.problem)
  });
  const joined: Loaded<QueryResult> =
    !totals.ok ? failed(totals.problem)
    : !durations.ok ? failed(durations.problem)
    : { state: 'populated', data: { ...totals.value, columns: [...totals.value.columns, ...durations.value.columns], rows: [[...totals.value.rows[0], ...durations.value.rows[0]]] } };
  return { spans: chart('spans', spans), errors: chart('errors', errors), duration: chart('duration', duration), totals: joined };
}

/** Log levels by OTLP severity number: 17 and up error, 13 and up warn, 9 and up info, 1 and up debug (TRACE and DEBUG), else unset. */
const LEVEL = "CASE WHEN severity_number >= 17 THEN 'error' WHEN severity_number >= 13 THEN 'warn' WHEN severity_number >= 9 THEN 'info' WHEN severity_number >= 1 THEN 'debug' ELSE 'unset' END";

/** Records per bucket and level under the list's range, WHERE and predicate. */
export function logTimelineQuery(input: { readonly where?: readonly Filter[] }, range: AbsoluteRange, bucket: string, predicate?: string): Executed {
  const logs = SIGNALS.logs;
  const binds = Binds.over(logs.time, range);
  const bin = `date_bin(${binds.bind(bucket)}::interval, ${logs.time})`;
  binds.where(input.where).predicate(predicate);
  return {
    sql: `SELECT ${bin} AS at, ${LEVEL} AS level, count(*) AS records FROM ${logs.table} WHERE ${binds.sql} GROUP BY ${bin}, ${LEVEL} ORDER BY at`,
    params: binds.params
  };
}

/** Load the Logs severity timeline; a refused predicate issues no read. */
export async function loadLogTimeline(input: { readonly where?: readonly Filter[]; readonly where_sql?: string; readonly range?: Range }, context: LoadContext): Promise<ChartProps> {
  const bucket = (range: AbsoluteRange) => bucketFor(range);
  const chartInput = (sql: string, range?: AbsoluteRange): ChartInput => ({
    type: 'timeseries',
    title: `Records per ${range ? bucket(range) : 'bucket'}`,
    sql,
    x: 'at',
    y: ['records'],
    series: 'level',
    ...(range ? { range } : {})
  });
  const range = rangeOf(input.range, context);
  if ('code' in range) return { input: chartInput('-'), result: { state: 'error', problem: range } };
  const predicate = await predicateFor(SIGNALS.logs.table, input.where_sql, context);
  if ('state' in predicate) return { input: chartInput('-', range), result: predicate };
  const plan = logTimelineQuery(input, range, bucket(range), predicate.sql);
  const [read] = await context.executor.run([query(plan.sql, plan.params)], context.signal);
  const chart = chartInput(plan.sql, range);
  return { input: chart, executed: plan, result: read.ok ? shapeChart(chart, read.value) : failed(read.problem) };
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
    const predicate = await predicateFor(SIGNALS[input.signal].table, input.where_sql, context);
    if ('state' in predicate) return { input, result: predicate };
    executed = listQuery(input, range, predicate.sql);
  }
  const [read] = await context.executor.run([query(executed.sql, executed.params, 'sql' in input ? (input.limit ?? 500) : undefined)], context.signal);
  if (!read.ok) return { input, executed, result: failed(read.problem) };
  let rows = read.value;
  let page: Page | undefined;
  if ('signal' in input && TIES[input.signal].length) {
    const limit = input.limit ?? LIST_LIMIT;
    const more = rows.rows.length > limit;
    // The first page reached backward reads again forward, so it is full.
    if (input.before && !more) return loadRecordTable({ ...input, before: undefined }, context);
    const kept = rows.rows.slice(0, limit);
    rows = { ...rows, rows: input.before ? kept.reverse() : kept };
    const at = (row: readonly unknown[] | undefined) => (row ? cursorColumns(input.signal, input.sort).map((column) => row[columnIndex(rows, column)] as QueryParam) : undefined);
    page = { older: more || input.before ? at(rows.rows.at(-1)) : undefined, newer: input.after || input.before ? at(rows.rows[0]) : undefined };
  }
  if (rows.rows.length === 0) {
    const reason = 'sql' in input ? 'The SQL returned no rows in this range.' : emptyList(input.signal, input.where);
    return { input, executed, result: { state: 'empty', data: { rows }, reason } };
  }
  if (!('signal' in input) || input.signal !== 'traces') return { input, executed, result: { state: 'populated', data: { rows, page } } };
  context.signal?.throwIfAborted();
  const at = shownColumns(input).length;
  const counted = countsQuery([...new Set(rows.rows.map((row) => String(row[at])))]);
  const [counts] = await context.executor.run([query(counted.sql, counted.params)], context.signal);
  const loaded: Loaded<QueryResult> = counts.ok ? { state: 'populated', data: counts.value } : failed(counts.problem);
  return { input, executed, result: { state: 'populated', data: { rows, counts: loaded, page } } };
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

/** The log level of an OTLP severity number: 17 and up error, 13 and up warn, 1 and up info, else unset. */
export function severityLevel(number: unknown): 'error' | 'warn' | 'info' | 'unset' {
  const n = Number(number ?? 0);
  return n >= 17 ? 'error' : n >= 13 ? 'warn' : n >= 1 ? 'info' : 'unset';
}

/** A hex id as its first and last four characters. */
export function shortId(id: string): string {
  return id.length > 10 ? `${id.slice(0, 4)}…${id.slice(-4)}` : id;
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
