/**
 * Observe loaders: built-in reads of the canonical signal tables. Every SQL
 * text is assembled only from the fixed tables and column allowlists in
 * `SIGNALS`; every value, range bound and identity is a typed `$n` bind.
 */
import { failed, wyrdProblem, type Loaded, type Problem, type QueryParam, type QueryResult, type WireRequest } from './contract.js';
import type { Part } from './execution.js';
import { fmtDuration, fmtNumber, parseInstant } from './format.js';
import { bucketFor, bucketWidth, chartParams, inputProblem, resolveRange, shapeChart, type ChartProps, type LoadContext } from './loaders.js';
import { MAX_SERIES, columnIndex, identityCaptions, numeric } from './plot.js';
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
  type ReviewSetsInput,
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

  /**
   * Start a read of {@link SERIES_TABLE} with the hours `range` overlaps as
   * `$1`/`$2`: a series row covers its whole hour, so the hour holding `from`
   * is read too.
   */
  static hours(range: AbsoluteRange): Binds {
    const binds = new Binds();
    binds.clauses.push(`hour_unix_nano >= date_bin(INTERVAL '1 hour', ${binds.bind(range.from)}::timestamptz) AND hour_unix_nano < ${binds.bind(range.to)}::timestamptz`);
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
function query(sql: string, params: readonly QueryParam[], max_rows?: number, max_bytes?: number): Extract<WireRequest, { op: 'bifrost.query' }> {
  return { op: 'bifrost.query', sql, params: [...params], ...(max_rows === undefined ? {} : { max_rows }), ...(max_bytes === undefined ? {} : { max_bytes }) };
}

/**
 * Validate a panel's `where_sql` against its signal table before any read
 * uses it: the normalized predicate, `undefined` for none, or the refusal
 * that stops the panel with no dependent read.
 */
async function predicateFor(table: string, whereSql: string | undefined, context: LoadContext): Promise<{ sql?: string } | Loaded<never>> {
  if (!whereSql?.trim()) return {};
  // Panels of one refresh share its context, so they share one validation; the memo dies with the context.
  const memo = VALIDATED.get(context) ?? new Map<string, Promise<{ sql?: string } | Loaded<never>>>();
  VALIDATED.set(context, memo);
  const key = `${table}\n${whereSql}`;
  const shared = memo.get(key) ?? validatePredicate(table, whereSql, context);
  memo.set(key, shared);
  return shared;
}

/** Each load context's predicate validations, by table and expression. */
const VALIDATED = new WeakMap<LoadContext, Map<string, Promise<{ sql?: string } | Loaded<never>>>>();

/** One `bifrost.validate_predicate` round trip; see {@link predicateFor}. */
async function validatePredicate(table: string, whereSql: string, context: LoadContext): Promise<{ sql?: string } | Loaded<never>> {
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

/**
 * The strip's reads: one per population, the stats over the same table, time
 * column, root restriction and currency grouping. Each read scans once and
 * groups per bucket and over the whole range by grouping sets; `whole` is 1
 * on range rows. Stat `i` of the input reads column `s<i>`; a filter its
 * population does not share becomes that aggregate's `FILTER`, and `m<i>`
 * counts the rows it matched, so a bucket only the population recorded is
 * not one of that stat's buckets.
 */
export function statQueries(input: StatStripInput, range: AbsoluteRange, bucket: string, predicate?: string): (Executed & { readonly stats: readonly number[] })[] {
  const populations = new Map<string, number[]>();
  input.stats.forEach((stat, index) => {
    const source = STAT_SOURCES[stat];
    const key = [source.table, source.time, source.roots, source.byCurrency].join('|');
    populations.set(key, [...(populations.get(key) ?? []), index]);
  });
  return [...populations.values()].map((stats) => {
    const sources = stats.map((index) => STAT_SOURCES[input.stats[index]]);
    const [source] = sources;
    const shared = sources.every((each) => each.filter === source.filter) ? source.filter : undefined;
    const binds = Binds.over(source.time, range);
    const bin = `date_bin(${binds.bind(bucket)}::interval, ${source.time})`;
    if (shared) binds.clauses.push(shared);
    if (input.scope) binds.clauses.push(`${input.scope.column} = ${binds.bind(input.scope.uid)}`);
    if (source.table === SIGNALS.spans.table || source.table === SIGNALS.logs.table) binds.where(input.where).predicate(predicate);
    if (source.roots) binds.roots(source.table);
    const values = stats
      .map((index, at) => {
        const filter = sources[at].filter && !shared ? ` FILTER (WHERE ${sources[at].filter})` : '';
        return `${sources[at].value}${filter} AS s${index}${filter ? `, count(*)${filter} AS m${index}` : ''}`;
      })
      .join(', ');
    const [currency, sets] = source.byCurrency ? [', currency', '(at, currency), (currency)'] : ['', '(at), ()'];
    return {
      stats,
      sql: `SELECT grouping(at) AS whole, at, ${values}${currency} FROM (SELECT ${bin} AS at, * FROM ${source.table} WHERE ${binds.sql}) AS scoped GROUP BY GROUPING SETS (${sets}) ORDER BY whole, at${currency}`,
      params: binds.params
    };
  });
}

/** Load the strip's population reads in one batch; one refusal refuses the strip. */
export async function loadStatStrip(input: StatStripInput, context: LoadContext): Promise<StatStripProps> {
  const range = rangeOf(input.range, context);
  if ('code' in range) return { input, result: { state: 'error', problem: range } };
  const predicate = await predicateFor(SIGNALS[logStats(input.stats) ? 'logs' : 'spans'].table, input.where_sql, context);
  if ('state' in predicate) return { input, result: predicate };
  const plans = statQueries(input, range, bucketFor(range), predicate.sql);
  const results = await context.executor.run(plans.map((plan) => query(plan.sql, plan.params)), context.signal);
  const failure = firstFailure(results);
  if (failure) return { input, result: failure };
  const reads = new Map(plans.flatMap((plan, at) => plan.stats.map((index) => [index, (results[at] as { value: QueryResult }).value] as const)));
  const stats = input.stats.map((stat, index) => {
    const all = reads.get(index) as QueryResult;
    // A filtered stat's buckets are those where its filter matched a row, as its own WHERE would group them.
    const matched = all.columns.findIndex((column) => column.name === `m${index}`);
    const read = matched < 0 ? all : { ...all, rows: all.rows.filter((row) => Number(row[0]) === 1 || Number(row[matched]) > 0) };
    const value = [`s${index}`, 'value'] as const;
    const currency = STAT_SOURCES[stat].byCurrency ? ['currency'] : [];
    return { stat, total: wholeRows(read, 1, [value, ...currency]), series: wholeRows(read, 0, ['at', value, ...currency]) };
  });
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
  if (signal === 'executions') return 'No execution matches these filters in this window.';
  const what = signal === 'traces' ? 'root spans' : 'rows';
  const matching = where.length ? ` matching ${where.map((filter) => filter.column).join(', ')}` : '';
  return `No ${what} in ${SIGNALS[signal].table} with ${SIGNALS[signal].time} in this range${matching}.`;
}

/**
 * Value counts of every facet under the list's own WHERE and range, in one
 * grouping-set aggregate: one grouping set per distinct facet column, a
 * `facet` index naming the set, each facet's value in its own typed `f<i>`
 * column, and the first {@link FACET_VALUES} + 1 values per facet by count,
 * then value, so an eleventh row still reports that more exist.
 */
export function facetsQuery(input: FacetRailInput, range: AbsoluteRange, predicate?: string): Executed {
  const signal = SIGNALS[input.signal];
  const binds = Binds.over(signal.time, range);
  binds.where(input.where).predicate(predicate);
  const columns = [...new Set(input.facets)];
  // A trace facet counts the traces with a span of each value, under the same span-level WHERE.
  const n = input.signal === 'traces' ? 'count(DISTINCT trace_id)' : 'count(*)';
  // A severity text value reads the highest severity number recorded with it, so its level comes from the number.
  const level = input.signal === 'logs' && columns.includes('severity_text') ? ', severity_number' : '';
  const values = columns.map((_, index) => `f${index}`);
  const facet =
    columns.length === 1
      ? '0'
      : `CASE ${columns
          .slice(0, -1)
          .map((column, index) => `WHEN grouping(${column}) = 0 THEN ${index}`)
          .join(' ')} ELSE ${columns.length - 1} END`;
  const grouped =
    `SELECT ${facet} AS facet, ${columns.map((column, index) => `${column} AS ${values[index]}`).join(', ')}, ${n} AS n${level ? ', max(severity_number) AS severity_number' : ''} ` +
    `FROM ${signal.table} WHERE ${binds.sql} GROUP BY GROUPING SETS (${columns.map((column) => `(${column})`).join(', ')})`;
  const shown = `facet, ${values.join(', ')}, n${level}`;
  return {
    sql:
      `SELECT ${shown} FROM (SELECT ${shown}, row_number() OVER (PARTITION BY facet ORDER BY n DESC, ${values.join(', ')}) AS rank FROM (${grouped}) AS grouped) AS ranked ` +
      `WHERE rank <= ${FACET_VALUES + 1} ORDER BY facet, rank`,
    params: binds.params
  };
}

/** Splits one {@link facetsQuery} read into one facet's `value, n` rows, keeping its typed value column. */
function facetValues(read: QueryResult, columns: readonly string[], column: string, input: FacetRailInput): QueryResult {
  const index = columns.indexOf(column);
  const level = input.signal === 'logs' && column === 'severity_text';
  const at = (name: string) => read.columns.findIndex((each) => each.name === name);
  const [value, n, severity] = [at(`f${index}`), at('n'), at('severity_number')];
  const rows = read.rows.filter((row) => Number(row[0]) === index).map((row) => (level ? [row[value], row[n], row[severity]] : [row[value], row[n]]));
  return {
    columns: [{ ...read.columns[value], name: 'value' }, read.columns[n], ...(level ? [read.columns[severity]] : [])],
    rows,
    terminal: { ...read.terminal, row_count: rows.length }
  };
}

/** Load every facet in one aggregate; its refusal refuses the rail. */
export async function loadFacetRail(input: FacetRailInput, context: LoadContext): Promise<FacetRailProps> {
  const range = rangeOf(input.range, context);
  if ('code' in range) return { input, result: { state: 'error', problem: range } };
  const predicate = await predicateFor(SIGNALS[input.signal].table, input.where_sql, context);
  if ('state' in predicate) return { input, result: predicate };
  const plan = facetsQuery(input, range, predicate.sql);
  const [read] = await context.executor.run([query(plan.sql, plan.params)], context.signal);
  if (!read.ok) return { input, result: failed(read.problem) };
  const columns = [...new Set(input.facets)];
  const facets = input.facets.map((column) => ({ column, values: facetValues(read.value as QueryResult, columns, column, input) }));
  if (facets.every(({ values }) => values.rows.length === 0))
    return { input, result: { state: 'empty', data: { facets }, reason: emptyList(input.signal, input.where) } };
  return { input, result: { state: 'populated', data: { facets } } };
}

/**
 * Columns a list reads after its shown columns, by signal: the row's identity
 * and the cues its cells read, a span's parent and a record's severity number.
 */
export const KEY_COLUMNS: Record<Signal, readonly string[]> = {
  traces: ['trace_id', 'span_id', 'duration_nano'],
  spans: ['trace_id', 'span_id', 'parent_span_id'],
  logs: ['trace_id', 'span_id', 'observed_time_unix_nano', 'wyrd_request_id', 'severity_number'],
  metrics: [],
  executions: ['execution_id', 'subject_kind', 'subject_version', 'subject_card_uid', 'result_id', 'binding_id', 'workload_card_uid', 'review_set']
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
    values[index] === null ? '' : isInstant(column) ? `${binds.bind(values[index])}::timestamptz` : binds.bind(values[index])
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

/** A point's recorded meaning: points that differ in any of these never share a group. */
const MEANING = ['metric_type', 'unit', 'aggregation_temporality', 'is_monotonic'] as const;

/** Most keys one metric view groups by at once. */
export const MAX_GROUP_KEYS = 3;
/** The Variant each group-key source reads one key of. */
const KEY_SOURCES = { attributes: 'attributes', resource: 'resource_attributes', scope: 'scope_attributes' } as const;
/** Promoted metrics columns a view may group by: every filter column but the metric and its meaning. Variant keys are typed. */
export const METRIC_GROUP_KEYS: readonly string[] = (SIGNALS.metrics.filters as readonly string[]).filter((column) => column !== 'metric_name' && !(MEANING as readonly string[]).includes(column));

/**
 * The SQL that reads one group key, binding a Variant key as text: a promoted
 * column as itself, or `attributes.<key>`, `resource.<key>` or `scope.<key>`
 * as that one key's text form, so no read rebuilds a whole attribute set.
 *
 * @throws when `key` names neither a promoted column nor a Variant key.
 */
function groupKey(key: string, binds: Binds): string {
  if (METRIC_GROUP_KEYS.includes(key)) return key;
  const dot = key.indexOf('.');
  const source = KEY_SOURCES[key.slice(0, dot) as keyof typeof KEY_SOURCES];
  const name = key.slice(dot + 1);
  if (dot < 0 || !source || !name || name.length > 256) throw new Error(`${key} is not a metrics group key`);
  return `variant_get_text(${source}, ${binds.bind(name)})`;
}

/** The read of one metric's plotted groups. */
export interface MetricInput {
  readonly metric: string;
  readonly where?: readonly Filter[];
  readonly where_sql?: string;
  /** Replace the default aggregation across series: sum for sums, mean for gauges. */
  readonly aggregation?: MetricAggregation;
  /** Keep one group per value of each key, at most {@link MAX_GROUP_KEYS}: promoted columns or `attributes.`, `resource.` or `scope.` keys. */
  readonly by?: readonly string[];
  /** Plot every series apart, at most {@link MAX_SERIES}, instead of aggregating across them. */
  readonly apart?: boolean;
  readonly range?: Range;
}

/**
 * Props of one metric: the Chart of its groups, the typed identity columns
 * that key them, and each group's Latest, Max and Mean of its plotted values.
 */
export interface MetricProps {
  readonly input: MetricInput;
  readonly chart: ChartProps;
  readonly identity: readonly string[];
  readonly summary: Loaded<QueryResult>;
  /** Present when the metric records explicit-bucket histogram points in the range. */
  readonly histogram?: HistogramProps;
}

/** A Chart keyed by typed identity columns, with each line's Latest, Max and Mean. */
export type KeyedChartProps = ChartProps & { readonly identity: readonly string[]; readonly summary: Loaded<QueryResult> };

/**
 * One histogram metric's plots: the mean and bucket-estimated percentiles
 * over time, the count rate over time, and the bucket distribution over the
 * whole range. Each is an ordinary Chart over the same recorded points; the
 * plots over time key every line by its group's typed identity.
 */
export interface HistogramProps {
  readonly latency: KeyedChartProps;
  readonly rate: KeyedChartProps;
  readonly distribution: ChartProps;
}

/** The range's binds, the metric, the WHERE and predicate, and the group keys as `g1`… of one metric read. */
function metricBinds(input: MetricInput, range: AbsoluteRange, kind: string, predicate?: string) {
  const filters = SIGNALS.metrics.filters as readonly string[];
  for (const { column } of input.where ?? []) if (!filters.includes(column)) throw new Error(`${column} is not a metrics column`);
  const keys = input.apart ? [] : (input.by ?? []);
  if (keys.length > MAX_GROUP_KEYS) throw new Error(`at most ${MAX_GROUP_KEYS} group keys`);
  const binds = Binds.over(SIGNALS.metrics.time, range);
  binds.clauses.push(`metric_name = ${binds.bind(input.metric)}`, kind);
  binds.where((input.where ?? []).filter(({ column, not }) => column !== 'metric_name' || not)).predicate(predicate);
  const groups = keys.map((key, index) => [`g${index + 1}`, groupKey(key, binds)] as const);
  return { binds, keys, groups };
}

/**
 * The chart read of one metric, aggregated on the server into the chart's
 * windows. Each series, by `series_id`, gives one value per window:
 * - a monotonic cumulative sum the increases of its points in the window,
 *   each over the series' previous point (a changed start or a smaller value
 *   is a reset, counted from zero; a series' first point in the range counts
 *   nothing when the series started before the range, and in full otherwise),
 *   and a delta sum its points added up, both per second;
 * - a gauge or non-monotonic cumulative sum its last value.
 * Series then aggregate per recorded meaning and group: sums add up and
 * gauges average, unless `aggregation` replaces it; with `apart` every series
 * stays its own group. The {@link MAX_SERIES} groups with the largest measure
 * over the range are read (total for rates, mean otherwise), each row naming
 * how many groups there were.
 *
 * Integer points stay exact decimals through every step while a window and
 * the point before it hold only integers, so an increase past 2^53 is exact;
 * `value` is that result as double precision for plotting. Each row also
 * carries its group's `latest`, `max` and `mean` over the plotted windows as
 * server text: exact decimals for integer gauges and sums, the double
 * otherwise, with Max ordered by the double and a rounded tie broken exactly
 * (a double that ties an integer is integral). A rate is a double.
 */
export function metricQueries(input: MetricInput, range: AbsoluteRange, predicate?: string): { identity: readonly string[]; chart: Executed } {
  const { binds, keys, groups } = metricBinds(input, range, PLOTTED, predicate);
  const bucket = binds.bind(bucketFor(range));
  const seconds = binds.bind(bucketWidth(range) / 1000);
  const meaning = MEANING.join(', ');
  const kept = groups.map(([name]) => `, ${name}`).join('');
  const group = [meaning, ...(input.apart ? ['series'] : groups.map(([name]) => name))].join(', ');
  const counter = "metric_type = 'sum' AND (is_monotonic OR aggregation_temporality = 1)";
  const across = (x: string) => (input.aggregation ? `${AGGREGATE[input.aggregation]}(${x})` : `CASE WHEN metric_type = 'sum' THEN sum(${x}) ELSE avg(${x}) END`);
  // $1 is the range's start. One point's increase over the series' previous point `xp`, read as `x` or as the exact `i`.
  const increase = (x: 'x' | 'i') => `CASE WHEN head THEN CASE WHEN s < $1::timestamptz THEN 0 ELSE ${x} END WHEN s = sp AND ${x} >= ${x}p THEN ${x} - ${x}p ELSE ${x} END`;
  // One window's value of a series from its points' total `xt`, last value `x1` and summed increases `xd`.
  const value = (x: 'x' | 'i') => `CASE WHEN aggregation_temporality = 1 THEN ${x}t WHEN metric_type = 'sum' AND is_monotonic THEN ${x}d ELSE ${x}1 END`;
  const by = `PARTITION BY ${group}`;
  const sql =
    `WITH pts AS (SELECT date_bin(${bucket}::interval, ${SIGNALS.metrics.time}) AS w, ${SIGNALS.metrics.time} AS t, start_time_unix_nano AS s, series_id, ${meaning}, ` +
    `${groups.map(([name, sql]) => `${sql} AS ${name}, `).join('')}CAST(int_value AS DECIMAL(38, 0)) AS i, COALESCE(CAST(int_value AS DOUBLE), double_value) AS x FROM ${SIGNALS.metrics.table} WHERE ${binds.sql}), ` +
    `steps AS (SELECT *, lag(t) OVER series IS NULL AS head, lag(s) OVER series AS sp, lag(x) OVER series AS xp, lag(i) OVER series AS ip FROM pts WINDOW series AS (PARTITION BY series_id ORDER BY t)), ` +
    `ends AS (SELECT w, series_id, ${meaning}${kept}, sum(x) AS xt, last_value(x ORDER BY t) AS x1, sum(${increase('x')}) AS xd, ` +
    `sum(i) AS it, last_value(i ORDER BY t) AS i1, sum(${increase('i')}) AS id, bool_and(i IS NOT NULL AND (head OR ip IS NOT NULL)) AS exact FROM steps GROUP BY w, series_id, ${meaning}${kept}), ` +
    `per AS (SELECT w, encode(series_id, 'hex') AS series, ${meaning}${kept}, ${value('x')} AS v, CASE WHEN exact THEN ${value('i')} END AS vi FROM ends), ` +
    `grouped AS (SELECT w, ${group}, CAST(CASE WHEN count(vi) = count(*) THEN ${across('vi')} END AS DECIMAL(38, 4)) AS gi, ${across('v')} AS gv FROM per GROUP BY w, ${group}), ` +
    `valued AS (SELECT w, ${group}, COALESCE(CAST(gi AS DOUBLE), gv) / CASE WHEN ${counter} THEN ${seconds} ELSE 1 END AS value, CASE WHEN NOT (${counter}) THEN gi END AS exact FROM grouped), ` +
    `measured AS (SELECT *, CASE WHEN ${counter} THEN sum(value) OVER (${by}) ELSE avg(value) OVER (${by}) END AS measure FROM valued), ` +
    `ranked AS (SELECT *, dense_rank() OVER (ORDER BY measure DESC NULLS LAST, ${group}) AS rank FROM measured), ` +
    `plotted AS (SELECT *, COALESCE(CAST(exact AS VARCHAR), CAST(value AS VARCHAR)) AS text, COALESCE(exact, CASE WHEN abs(value) < 1e19 THEN CAST(value AS DECIMAL(38, 0)) END) AS tie FROM (SELECT *, max(rank) OVER () AS groups FROM ranked) WHERE rank <= ${MAX_SERIES}) ` +
    `SELECT w AS at, ${group}, value, groups, last_value(text) OVER (${by} ORDER BY w ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING) AS latest, ` +
    `first_value(text) OVER (${by} ORDER BY value DESC NULLS LAST, tie DESC NULLS LAST) AS max, ` +
    `CASE WHEN count(exact) OVER (${by}) = count(*) OVER (${by}) THEN CAST(avg(exact) OVER (${by}) AS VARCHAR) ELSE CAST(avg(value) OVER (${by}) AS VARCHAR) END AS mean ` +
    `FROM plotted ORDER BY at`;
  return { identity: [...MEANING, ...(input.apart ? ['series'] : keys)], chart: { sql, params: binds.params } };
}

/** One metric's series in {@link SERIES_TABLE} over the hours `range` overlaps, under the WHERE and predicate. */
function metricSeriesBinds(input: MetricInput, range: AbsoluteRange, predicate?: string): Binds {
  const filters = SIGNALS.metrics.filters as readonly string[];
  for (const { column } of input.where ?? []) if (!filters.includes(column)) throw new Error(`${column} is not a metrics column`);
  const binds = Binds.hours(range);
  binds.clauses.push(`metric_name = ${binds.bind(input.metric)}`);
  return binds.where((input.where ?? []).filter(({ column, not }) => column !== 'metric_name' || not)).predicate(predicate);
}

/**
 * The recorded resource, scope and attributes of `series`, by `series_id`,
 * each as its JSON text from the series index's newest hour in the range:
 * the only read of whole attribute sets for a chart, and only for the
 * plotted series.
 */
export function seriesLabelsQuery(input: MetricInput, range: AbsoluteRange, series: readonly string[], predicate?: string): Executed {
  const binds = metricSeriesBinds(input, range, predicate);
  binds.clauses.push(`series_id IN (${series.map((id) => `decode(${binds.bind(id)}, 'hex')`).join(', ')})`);
  return {
    sql:
      `SELECT encode(series_id, 'hex') AS series, to_json(resource_attributes) AS resource, scope_name, scope_version, to_json(scope_attributes) AS scope, to_json(attributes) AS attributes ` +
      `FROM (SELECT *, row_number() OVER (PARTITION BY series_id ORDER BY hour_unix_nano DESC) AS newest FROM ${SERIES_TABLE} WHERE ${binds.sql}) WHERE newest = 1`,
    params: binds.params
  };
}

/** Most series whose attribute sets suggest group keys. */
const KEY_SAMPLE = 100;

/** The attribute sets of at most {@link KEY_SAMPLE} of the metric's series in range, whose keys the view offers to group by. */
export function groupKeysQuery(input: MetricInput, range: AbsoluteRange, predicate?: string): Executed {
  const binds = metricSeriesBinds(input, range, predicate);
  return {
    sql: `SELECT to_json(attributes) AS attributes, to_json(resource_attributes) AS resource, to_json(scope_attributes) AS scope FROM ${SERIES_TABLE} WHERE ${binds.sql} LIMIT ${KEY_SAMPLE}`,
    params: binds.params
  };
}

/**
 * The keys a metric's group-by picker offers: the promoted columns, then the
 * keys its sampled series record. Read only when the reader opens the picker,
 * never with the metric's chart. A failed sample offers the promoted columns
 * alone, since any key can still be typed; a refused predicate issues no read.
 */
export async function loadMetricKeys(input: MetricInput, context: LoadContext): Promise<Loaded<readonly string[]>> {
  const range = rangeOf(input.range, context);
  if ('code' in range) return { state: 'error', problem: range };
  const predicate = await predicateFor(SIGNALS.metrics.table, input.where_sql, context);
  if ('state' in predicate) return predicate;
  const keyed = groupKeysQuery(input, range, predicate.sql);
  const [sampled] = await context.executor.run([query(keyed.sql, keyed.params, KEY_SAMPLE, PLOT_BYTES)], context.signal);
  return { state: 'populated', data: sampled.ok ? offeredKeys(sampled.value) : METRIC_GROUP_KEYS };
}

/** The promoted group keys, then each `attributes.`, `resource.` or `scope.` key `read`'s sets record, once each and sorted. */
function offeredKeys(read: QueryResult): readonly string[] {
  const found = new Set<string>();
  read.columns.forEach(({ name }, index) => {
    for (const row of read.rows) {
      if (typeof row[index] !== 'string') continue;
      let set: unknown;
      try {
        set = JSON.parse(row[index]);
      } catch {
        continue;
      }
      if (set && typeof set === 'object' && !Array.isArray(set)) for (const key of Object.keys(set)) found.add(`${name}.${key}`);
    }
  });
  return [...METRIC_GROUP_KEYS, ...[...found].sort()];
}

/** Rows one windowed plot reads: {@link MAX_SERIES} series or groups by the at most 201 windows a range spans. */
const PLOT_ROWS = MAX_SERIES * 201;
/** Bytes one windowed plot reads. */
const PLOT_BYTES = 16 * 1024 * 1024;

/** The summary columns every chart row carries for its group, computed on the server. */
const SUMMARY = ['latest', 'max', 'mean'] as const;

/** Each plotted group's server-computed Latest, Max and Mean, once per group, keyed by `identity`. */
function plottedSummary(chart: QueryResult, identity: readonly string[]): QueryResult {
  const groups = new Map<string, unknown[]>();
  const columns = [...identity, ...SUMMARY].map((name) => columnIndex(chart, name));
  for (const row of chart.rows) {
    const cells = columns.map((index) => row[index]);
    const key = JSON.stringify(cells.slice(0, identity.length));
    if (!groups.has(key)) groups.set(key, cells);
  }
  return { columns: columns.map((index) => chart.columns[index]), rows: [...groups.values()], terminal: chart.terminal };
}

/** Name the read's `g1`… columns by their group keys, and with `labels` replace each series id by its recorded resource, scope and attributes. */
function named(read: QueryResult, keys: readonly string[], labels?: QueryResult): QueryResult {
  const columns = read.columns.map((column) => (/^g\d+$/.test(column.name) ? { ...column, name: keys[Number(column.name.slice(1)) - 1] } : column));
  if (!labels) return { ...read, columns };
  const at = columnIndex(read, 'series');
  const recorded = new Map(labels.rows.map((row) => [row[0], row.slice(1)]));
  const empty = labels.columns.slice(1).map(() => null);
  return {
    columns: [...columns.slice(0, at), ...labels.columns.slice(1), ...columns.slice(at + 1)],
    rows: read.rows.map((row) => [...row.slice(0, at), ...(recorded.get(row[at]) ?? empty), ...row.slice(at + 1)]),
    terminal: read.terminal
  };
}

/** The labels read of `series`, at most one row each. */
function labelsRead(input: MetricInput, range: AbsoluteRange, series: readonly string[], predicate?: string) {
  const labels = seriesLabelsQuery(input, range, series, predicate);
  return query(labels.sql, labels.params, series.length);
}

/** The recorded columns an `apart` series is identified by in place of its id. */
const SERIES_LABELS = ['resource', 'scope_name', 'scope_version', 'scope', 'attributes'] as const;

/**
 * Load one metric's chart and its histogram plots in one batch over one
 * range and one validated predicate; with `apart`, a second read labels only
 * the plotted series of both. A refused predicate issues no read.
 */
export async function loadMetric(input: MetricInput, context: LoadContext): Promise<MetricProps> {
  const stopped = (state: Loaded<never>): MetricProps => ({ input, identity: [], summary: state, chart: { input: metricChart(input), result: state } });
  const range = rangeOf(input.range, context);
  if ('code' in range) return stopped({ state: 'error', problem: range });
  const predicate = await predicateFor(SIGNALS.metrics.table, input.where_sql, context);
  if ('state' in predicate) return stopped(predicate);
  let plan: ReturnType<typeof metricQueries>;
  let points: Executed;
  try {
    plan = metricQueries(input, range, predicate.sql);
    points = histogramQuery(input, range, predicate.sql);
  } catch (error) {
    return stopped({ state: 'error', problem: inputProblem('Metric', { path: ['by'], message: (error as Error).message }) });
  }
  const [chart, histogram] = await context.executor.run([query(plan.chart.sql, plan.chart.params, PLOT_ROWS, PLOT_BYTES), query(points.sql, points.params, PLOT_ROWS, PLOT_BYTES)], context.signal);
  // With `apart`, one read labels only the plotted gauge, sum and histogram series.
  const ids = (read: typeof chart) => (read.ok ? read.value.rows.map((row) => String(row[columnIndex(read.value, 'series')])) : []);
  const plotted = input.apart ? [...new Set([...ids(chart), ...ids(histogram)])] : [];
  const [labelled] = plotted.length ? await context.executor.run([labelsRead(input, range, plotted, predicate.sql)], context.signal) : [undefined];
  const label = (read: typeof chart, keys: readonly string[]): typeof chart => {
    if (!read.ok || !read.value.rows.length) return read;
    if (!input.apart) return { ok: true, value: named(read.value, keys) };
    return !labelled || labelled.ok ? { ok: true, value: named(read.value, [], labelled?.value) } : labelled;
  };
  const read = label(chart, plan.identity.slice(MEANING.length));
  const identity = input.apart && read.ok && read.value.rows.length ? [...MEANING, ...SERIES_LABELS] : plan.identity;
  const histogramRead = label(histogram, input.by ?? []);
  const histogramKeys = [...MEANING, ...(input.apart ? SERIES_LABELS : (input.by ?? []))];
  const reason = `No gauge or sum point of ${input.metric} in ${SIGNALS.metrics.table} in this range.`;
  const loaded = (data: QueryResult): Loaded<QueryResult> => (data.rows.length ? { state: 'populated', data } : { state: 'empty', data, reason });
  const total = read.ok && read.value.rows.length ? numeric(read.value.rows[0][columnIndex(read.value, 'groups')]) : 0;
  const noun = input.apart ? 'series' : 'groups';
  const capped = total > MAX_SERIES ? ` · the ${MAX_SERIES} largest of ${total} ${noun} plotted` : '';
  const title = `${metricChart(input).title}${capped}`;
  return {
    input,
    identity,
    chart: { input: { ...metricChart(input), title, sql: plan.chart.sql, range }, executed: plan.chart, result: read.ok ? loaded(read.value) : failed(read.problem) },
    summary: read.ok ? loaded(plottedSummary(read.value, identity)) : failed(read.problem),
    ...(!histogramRead.ok ? { histogram: histogramProps(input, range, points, failed(histogramRead.problem), histogramKeys) } : histogramRead.value.rows.length ? { histogram: histogramProps(input, range, points, histogramRead.value, histogramKeys) } : {})
  };
}

/** The metric's chart definition before its SQL is planned. */
function metricChart(input: MetricInput): Extract<ChartInput, { type: 'timeseries' }> {
  const across = input.aggregation ?? 'sum of sums, mean of gauges';
  const how = input.apart
    ? 'each series apart'
    : `${across} across series${input.by?.length ? ` by ${input.by.join(', ')}` : ''}`;
  return { type: 'timeseries', title: `${input.metric} · ${how} · counters per second · plotted as double precision`, sql: '-', x: 'at', y: ['value'] };
}

/** Explicit-bucket histogram points, the only histogram kind plotted; exponential histograms and summaries stay listed. */
const HISTOGRAM = "metric_type = 'histogram'";
/** The percentiles estimated from a histogram's buckets. */
export const HISTOGRAM_PERCENTILES = [50, 95, 99] as const;

/**
 * The read of one histogram metric, folded on the server into the chart's
 * windows: one row per window and group (recorded meaning, grouping value or,
 * with `apart`, `series`, and bounds) with its count, sum, bounds and bucket
 * counts, for the {@link MAX_SERIES} groups that counted most over the range,
 * each row naming how many groups there were.
 *
 * A cumulative point counts its increase over its series' previous point; a
 * changed start or a smaller count is a reset, and the point then counts from
 * zero. A series' first point in the range counts nothing when the series
 * started before the range, and in full otherwise. Delta points count as
 * recorded. A window's sum is null unless every sum it merges was recorded,
 * so a missing sum never reads as zero.
 */
export function histogramQuery(input: MetricInput, range: AbsoluteRange, predicate?: string): Executed {
  const { binds, groups: keyed } = metricBinds(input, range, HISTOGRAM, predicate);
  const bucket = binds.bind(bucketFor(range));
  const group = [...MEANING, ...(input.apart ? ['series'] : keyed.map(([name]) => name)), 'bounds'].join(', ');
  // $1 is the range's start: a series that started before it has its first point as the baseline.
  const base = (now: string, prior: string, zero: string) =>
    `CASE WHEN head THEN CASE WHEN started < $1::timestamptz THEN ${now}${zero} END WHEN sp = started AND n >= np THEN ${prior}${zero} END`;
  const each = `unnest(generate_series(1, CAST(cardinality(b) AS BIGINT))) AS i, unnest(b) AS c`;
  const sql =
    `WITH pts AS (SELECT date_bin(${bucket}::interval, ${SIGNALS.metrics.time}) AS w, ${SIGNALS.metrics.time} AS t, start_time_unix_nano AS started, ` +
    `${MEANING.join(', ')}, ${input.apart ? "encode(series_id, 'hex') AS series, " : keyed.map(([name, sql]) => `${sql} AS ${name}, `).join('')}explicit_bounds AS bounds, series_id, ` +
    `CAST(histogram_count AS BIGINT) AS n, histogram_sum AS v, CAST(bucket_counts AS BIGINT[]) AS b FROM ${SIGNALS.metrics.table} WHERE ${binds.sql}), ` +
    `steps AS (SELECT *, lag(t) OVER series IS NULL AS head, lag(started) OVER series AS sp, lag(n) OVER series AS np, lag(v) OVER series AS vp, lag(b) OVER series AS bp ` +
    `FROM pts WHERE aggregation_temporality = 2 WINDOW series AS (PARTITION BY series_id ORDER BY t)), ` +
    `counted AS (SELECT w, ${group}, ${each}, ${base('b', 'bp', '')} AS bb, n - ${base('n', 'np', ' ELSE 0')} AS dn, v - ${base('v', 'vp', ' ELSE 0')} AS dv FROM steps ` +
    `UNION ALL SELECT w, ${group}, ${each}, NULL AS bb, n AS dn, v AS dv FROM pts WHERE aggregation_temporality <> 2), ` +
    `buckets AS (SELECT w, ${group}, i, sum(CAST(c AS BIGINT) - COALESCE(CAST(array_element(bb, i) AS BIGINT), 0)) AS c, ` +
    `sum(CASE WHEN i = 1 THEN dn END) AS dn, sum(CASE WHEN i = 1 THEN dv END) AS dv, count(CASE WHEN i = 1 THEN dn END) AS kn, count(CASE WHEN i = 1 THEN dv END) AS kv FROM counted GROUP BY w, ${group}, i), ` +
    `windows AS (SELECT w, ${group}, sum(dn) AS count, CASE WHEN sum(kv) = sum(kn) THEN sum(dv) END AS sum, array_agg(CAST(c AS VARCHAR) ORDER BY i) AS buckets FROM buckets GROUP BY w, ${group}), ` +
    `ranked AS (SELECT *, dense_rank() OVER (ORDER BY measure DESC, ${group}) AS rank FROM (SELECT *, sum(count) OVER (PARTITION BY ${group}) AS measure FROM windows WHERE count > 0)) ` +
    `SELECT w AS at, ${group}, count, sum, buckets, groups FROM (SELECT *, max(rank) OVER () AS groups FROM ranked) WHERE rank <= ${MAX_SERIES} ORDER BY at`;
  return { sql, params: binds.params };
}

/** One histogram window or range total: summed counts, the sum when every merged sum was recorded, and bucket counts. */
interface Tally {
  count: number;
  sum: number | null;
  buckets: number[];
}

/** One group of series sharing recorded meaning, bounds and the grouping value or series, by its identity cells. */
interface Group {
  readonly cells: readonly unknown[];
  readonly bounds: readonly number[];
  readonly windows: [string, Tally][];
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

/** The measure column naming what each histogram line plots: `mean`, `p50 estimated`… or `per second`. */
const MEASURE = 'measure';

/**
 * The windowed plots of one histogram read: the mean and estimated
 * percentiles and the count rate per window, keyed by `identity` (the read's
 * group cells, `bounds` and `measure`), each with one `summary` row per line,
 * and the bucket counts over the whole range.
 */
export interface HistogramWindows {
  readonly identity: readonly string[];
  readonly latency: QueryResult;
  readonly latencySummary: QueryResult;
  readonly rate: QueryResult;
  readonly rateSummary: QueryResult;
  readonly distribution: QueryResult;
}

/**
 * Shape the server's histogram windows of `range` into the plots. `keys` are
 * the read's group columns besides bounds: the recorded meaning, then the
 * grouping keys or the series' recorded labels. Every line keeps its group's
 * typed identity, so no two groups ever add up. A window whose sum was not
 * recorded has no mean; an estimate in the unbounded top bucket is that
 * bucket's lower bound and its `text` reads `≥`, as does any Max or Mean of a
 * line holding one.
 */
export function histogramWindows(read: QueryResult, range: AbsoluteRange, keys: readonly string[] = MEANING): HistogramWindows {
  const at = (name: string) => columnIndex(read, name);
  const [ti, ci, sumi, bi, ki] = ['at', 'count', 'sum', 'bounds', 'buckets'].map(at);
  const keyIndexes = keys.map(at);
  const seconds = bucketWidth(range) / 1000;
  const groups = new Map<string, Group>();
  for (const row of read.rows) {
    const bounds = ((row[bi] ?? []) as unknown[]).map(numeric);
    const tally = { count: numeric(row[ci] ?? 0), sum: row[sumi] === null || row[sumi] === undefined ? null : numeric(row[sumi]), buckets: ((row[ki] ?? []) as unknown[]).map(numeric) };
    const cells = [...keyIndexes.map((index) => row[index] ?? null), JSON.stringify(bounds)];
    const key = JSON.stringify(cells);
    const group = groups.get(key) ?? { cells, bounds, windows: [], total: { count: 0, sum: 0, buckets: [] } };
    groups.set(key, group);
    group.windows.push([new Date(parseInstant(row[ti])).toISOString(), tally]);
    group.total.count += tally.count;
    tally.buckets.forEach((value, index) => (group.total.buckets[index] = (group.total.buckets[index] ?? 0) + value));
  }
  const latency: unknown[][] = [];
  const rate: unknown[][] = [];
  const distribution: unknown[][] = [];
  const lines = { latency: new Map<string, { cells: unknown[]; values: [number, boolean][] }>(), rate: new Map<string, { cells: unknown[]; values: [number, boolean][] }>() };
  const plot = (into: keyof typeof lines, instant: string, cells: readonly unknown[], measure: string, value: number | null, atLeast = false) => {
    const line = [...cells, measure];
    (into === 'latency' ? latency : rate).push([instant, ...line, value, value === null ? null : `${atLeast ? '≥ ' : ''}${value}`]);
    const key = JSON.stringify(line);
    const kept = lines[into].get(key) ?? { cells: line, values: [] };
    if (value !== null) kept.values.push([value, atLeast]);
    lines[into].set(key, kept);
  };
  const captions = identityCaptions([...groups.values()].map(({ cells }) => cells));
  [...groups.values()].forEach((group, index) => {
    for (const [instant, tally] of group.windows) {
      plot('rate', instant, group.cells, 'per second', tally.count / seconds);
      plot('latency', instant, group.cells, 'mean', tally.sum === null ? null : tally.sum / tally.count);
      for (const q of HISTOGRAM_PERCENTILES) {
        const estimate = bucketPercentile(group.bounds, tally.buckets, q);
        if (estimate) plot('latency', instant, group.cells, `p${q} estimated`, estimate.value, estimate.atLeast);
      }
    }
    group.total.buckets.forEach((count, bucket) => distribution.push([groups.size > 1 ? `${captions[index]} ${bucketLabel(group.bounds, bucket)}` : bucketLabel(group.bounds, bucket), count]));
  });
  const identity = [...keys, 'bounds', MEASURE];
  const columns = (...names: [string, string][]) => names.map(([name, data_type]) => ({ name, data_type, nullable: true }));
  const keyed = keys.map((name) => read.columns[at(name)]).concat(columns(['bounds', 'Utf8'], [MEASURE, 'Utf8']));
  const timed = [...columns(['at', 'Utf8']), ...keyed, ...columns(['value', 'Float64'], ['text', 'Utf8'])];
  // A line's Latest, Max and Mean of what it plotted; a Max or Mean over a lower bound is itself only a lower bound.
  const summary = (into: keyof typeof lines): QueryResult => ({
    columns: [...keyed, ...columns(['latest', 'Utf8'], ['max', 'Utf8'], ['mean', 'Utf8'])],
    rows: [...lines[into].values()].map(({ cells, values }) => {
      if (!values.length) return [...cells, null, null, null];
      const bound = values.some(([, atLeast]) => atLeast) ? '≥ ' : '';
      const [latest, latestAtLeast] = values[values.length - 1];
      const max = Math.max(...values.map(([value]) => value));
      const mean = values.reduce((sum, [value]) => sum + value, 0) / values.length;
      return [...cells, `${latestAtLeast ? '≥ ' : ''}${latest}`, `${bound}${max}`, `${bound}${mean}`];
    }),
    terminal: read.terminal
  });
  return {
    identity,
    latency: { columns: timed, rows: latency, terminal: read.terminal },
    latencySummary: summary('latency'),
    rate: { columns: timed, rows: rate, terminal: read.terminal },
    rateSummary: summary('rate'),
    distribution: { columns: columns(['bucket', 'Utf8'], ['count', 'Float64']), rows: distribution, terminal: read.terminal }
  };
}

/** The histogram Charts of one read keyed by their typed identity, or one shared failure. */
function histogramProps(input: MetricInput, range: AbsoluteRange, executed: Executed, read: QueryResult | Loaded<never>, keys: readonly string[]): HistogramProps {
  const how = `${input.apart ? 'each series apart · ' : input.by?.length ? `by ${input.by.join(', ')} · ` : ''}explicit-bucket histogram`;
  const total = 'state' in read || !read.rows.length ? 0 : numeric(read.rows[0][columnIndex(read, 'groups')]);
  const capped = total > MAX_SERIES ? ` · the ${MAX_SERIES} that counted most of ${total} ${input.apart ? 'series' : 'groups'} plotted` : '';
  const charts = {
    latency: { type: 'timeseries', title: `${input.metric} · mean of recorded sums and percentiles estimated from buckets, ≥ marking a lower bound · ${how}${capped}`, sql: executed.sql, x: 'at', y: ['value'], range },
    rate: { type: 'timeseries', title: `${input.metric} · recordings per second · ${how}${capped}`, sql: executed.sql, x: 'at', y: ['value'], range },
    distribution: { type: 'bar', title: `${input.metric} · bucket counts over the range · ${how}${capped}`, sql: executed.sql, x: 'bucket', y: ['count'] }
  } as const satisfies Record<keyof HistogramProps, ChartInput>;
  if ('state' in read) {
    const identity = [...keys, 'bounds', MEASURE];
    return {
      latency: { input: charts.latency, executed, result: read, identity, summary: read },
      rate: { input: charts.rate, executed, result: read, identity, summary: read },
      distribution: { input: charts.distribution, executed, result: read }
    };
  }
  const windows = histogramWindows(read, range, keys);
  const unit = read.rows.length ? read.rows[0][columnIndex(read, 'unit')] : undefined;
  const reason = `No histogram counts of ${input.metric} in this range.`;
  const loaded = (data: QueryResult): Loaded<QueryResult> => (data.rows.length ? { state: 'populated', data } : { state: 'empty', data, reason });
  return {
    latency: { input: { ...charts.latency, ...(unit ? { unit: String(unit) } : {}) }, executed, result: loaded(windows.latency), identity: windows.identity, summary: loaded(windows.latencySummary) },
    rate: { input: charts.rate, executed, result: loaded(windows.rate), identity: windows.identity, summary: loaded(windows.rateSummary) },
    distribution: { input: charts.distribution, executed, result: loaded(windows.distribution) }
  };
}

/** The metric series index: one row per series per hour it has points in, read wherever only series matter. */
const SERIES_TABLE = 'bifrost.metrics.series';

/** Most metric names the rail lists; one more is read to tell that more match. */
export const METRIC_NAMES = 50;

/**
 * The metrics the rail offers under the view's WHERE, read from the series
 * index so its cost follows series, not points: name, type and unit of each,
 * by name, at most {@link METRIC_NAMES} plus one. `search` keeps names
 * holding it, ignoring case. The WHERE's columns are the series' own; a
 * predicate naming a point-only column is refused by the server.
 */
export function metricNamesQuery(where: readonly Filter[], range: AbsoluteRange, predicate?: string, search?: string): Executed {
  const binds = Binds.hours(range);
  binds.where(where.filter(({ column }) => column !== 'metric_name')).predicate(predicate);
  if (search?.trim()) binds.clauses.push(`strpos(lower(metric_name), lower(${binds.bind(search.trim())})) > 0`);
  return {
    sql: `SELECT metric_name, metric_type, unit FROM ${SERIES_TABLE} WHERE ${binds.sql} GROUP BY metric_name, metric_type, unit ORDER BY metric_name, metric_type, unit LIMIT ${METRIC_NAMES + 1}`,
    params: binds.params
  };
}

/** Load the metric rail's names; a refused predicate issues no read. */
export async function loadMetricNames(input: { readonly where?: readonly Filter[]; readonly where_sql?: string; readonly range?: Range; readonly search?: string }, context: LoadContext): Promise<Loaded<QueryResult>> {
  const range = rangeOf(input.range, context);
  if ('code' in range) return { state: 'error', problem: range };
  const predicate = await predicateFor(SIGNALS.metrics.table, input.where_sql, context);
  if ('state' in predicate) return predicate;
  const plan = metricNamesQuery(input.where ?? [], range, predicate.sql, input.search);
  const [read] = await context.executor.run([query(plan.sql, plan.params, METRIC_NAMES + 1)], context.signal);
  if (!read.ok) return failed(read.problem);
  const reason = input.search?.trim() ? `No metric names hold “${input.search.trim()}”.` : emptyList('metrics', input.where);
  return read.value.rows.length ? { state: 'populated', data: read.value } : { state: 'empty', data: read.value, reason };
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

/**
 * The two reads the trace overview issues: every matching span, then every
 * matching root, each scanned once and grouped per bucket and over the whole
 * range by `GROUPING SETS ((at), ())`. `whole` is 1 on the range row, so
 * range totals and percentiles come from the data, never from buckets.
 */
export function traceOverviewReads(input: { readonly where?: readonly Filter[] }, range: AbsoluteRange, bucket: string, predicate?: string) {
  const spans = SIGNALS.spans;
  const plan = (select: string, roots: boolean) => {
    const binds = Binds.over(spans.time, range);
    const bin = `date_bin(${binds.bind(bucket)}::interval, ${spans.time})`;
    binds.where(input.where).predicate(predicate);
    if (roots) binds.roots(spans.table);
    const scoped = `SELECT ${bin} AS at, trace_id, status_code, duration_nano FROM ${spans.table} WHERE ${binds.sql}`;
    return { sql: `SELECT grouping(at) AS whole, at, ${select} FROM (${scoped}) AS scoped GROUP BY GROUPING SETS ((at), ()) ORDER BY whole, at`, params: binds.params };
  };
  const p = (share: number) => `approx_percentile_cont(duration_nano, ${share}) / 1e6`;
  return {
    spans: plan(`count(*) AS spans, count(*) FILTER (WHERE status_code = ${STATUS_ERROR}) AS errors, count(DISTINCT trace_id) AS traces, count(DISTINCT trace_id) FILTER (WHERE status_code = ${STATUS_ERROR}) AS error_traces, ${p(0.95)} AS span_p95_ms`, false),
    roots: plan(`${p(0.95)} AS p95, ${p(0.5)} AS p50`, true)
  };
}

/** The bucket (`whole` 0) or range (`whole` 1) rows of a grouping-set read, keeping only `names`, optionally renamed. */
function wholeRows(read: QueryResult, whole: 0 | 1, names: readonly (string | readonly [string, string])[]): QueryResult {
  const picks = names.map((name) => (typeof name === 'string' ? [name, name] : name));
  const at = picks.map(([from]) => read.columns.findIndex((each) => each.name === from));
  const rows = read.rows.filter((row) => Number(row[0]) === whole).map((row) => at.map((index) => row[index]));
  return { columns: at.map((index, i) => ({ ...read.columns[index], name: picks[i][1] })), rows, terminal: { ...read.terminal, row_count: rows.length } };
}

/** Load the trace overview in one batch; one refused predicate refuses every chart with no dependent read. */
export async function loadTraceOverview(input: { readonly where?: readonly Filter[]; readonly where_sql?: string; readonly range?: Range }, context: LoadContext): Promise<TraceOverviewProps> {
  const charts: Record<'spans' | 'errors' | 'duration', Extract<ChartInput, { type: 'timeseries' }>> = {
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
  const bucket = bucketFor(range);
  const plans = traceOverviewQueries(input, range, bucket, predicate.sql);
  const reads = traceOverviewReads(input, range, bucket, predicate.sql);
  const [spans, roots] = await context.executor.run([query(reads.spans.sql, reads.spans.params), query(reads.roots.sql, reads.roots.params)], context.signal);
  // Charts keep their own portable SQL; `executed` names the read that answered them.
  const chart = (name: 'spans' | 'errors' | 'duration', read: typeof spans, executed: Executed): ChartProps => ({
    input: { ...charts[name], sql: plans[name].sql, range },
    executed,
    result: read.ok ? shapeChart({ ...charts[name], sql: plans[name].sql, range }, wholeRows(read.value, 0, ['at', ...charts[name].y])) : failed(read.problem)
  });
  const totals: Loaded<QueryResult> =
    !spans.ok ? failed(spans.problem)
    : !roots.ok ? failed(roots.problem)
    : (() => {
        const [all, root] = [wholeRows(spans.value, 1, ['spans', 'traces', 'error_traces', 'span_p95_ms']), wholeRows(roots.value, 1, [['p95', 'p95_ms'], ['p50', 'p50_ms']])];
        return { state: 'populated', data: { ...all, columns: [...all.columns, ...root.columns], rows: [[...all.rows[0], ...root.rows[0]]] } };
      })();
  return { spans: chart('spans', spans, reads.spans), errors: chart('errors', spans, reads.spans), duration: chart('duration', roots, reads.roots), totals };
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

/** Verify's two charts: executions per bucket by recorded verdict, and by Verifier. */

/** The series of an execution with no recorded verdict: a neutral bucket, never a verdict. */
export const NO_VERDICT = 'no verdict';

/** How an execution status or recorded verdict reads: its glyph, its words and its tone. Hue is never the only channel. */
export type ExecutionMark = readonly [glyph: string, label: string, tone: 'text' | 'muted' | 'run' | 'ok' | 'warn' | 'danger'];
/** Every retained execution status. `pending` is work accepted and not yet started, which a reader knows as queued. */
export const EXECUTION_STATUS: Readonly<Record<string, ExecutionMark>> = {
  pending: ['○', 'queued', 'muted'],
  running: ['●', 'running', 'run'],
  retrying: ['↻', 'retrying', 'run'],
  waiting: ['◷', 'waiting', 'warn'],
  completed: ['■', 'completed', 'ok'],
  errored: ['✕', 'errored', 'danger'],
  timed_out: ['⧗', 'timed out', 'warn'],
  cancelled: ['⊘', 'cancelled', 'muted'],
  'not recorded': ['○', 'not recorded', 'muted']
};
/** Every recorded verdict. */
export const EXECUTION_VERDICT: Readonly<Record<string, ExecutionMark>> = {
  passed: ['✓', 'passed', 'ok'],
  failed: ['✕', 'failed', 'danger'],
  inconclusive: ['?', 'inconclusive', 'muted']
};

/** How many Verifiers the Verifier chart keys by name; the rest share one series, so its key stays on one line. */
export const NAMED_VERIFIERS = 3;

/**
 * The Verify discovery read: every facet and both charts over the population
 * the list reads, under the same range and WHERE, as one grouping-set
 * aggregate that reads the population once. Its stages are sibling named
 * queries, each reading the one before it, so the statement nests no deeper
 * than the population.
 *
 * Rows carry a `facet` index: below the facet count, one facet's values as
 * {@link facetsQuery} ranks them; at the count, executions per bucket and
 * verdict, with no verdict as its own series; one past it, executions per
 * bucket and Verifier, the {@link NAMED_VERIFIERS} Verifiers with the most
 * executions over the whole range keyed by UID with their `place`, and every
 * other Verifier counted together under a null `verifier`. `verifiers`
 * is the number of Verifiers the range holds.
 */
export function executionDiscoveryQuery(input: FacetRailInput, range: AbsoluteRange): Executed {
  const { table, time } = SIGNALS.executions;
  const binds = Binds.over(time, range);
  const bucket = binds.bind(bucketFor(range));
  binds.where(input.where);
  const columns = [...new Set(input.facets)];
  const facets = columns.length;
  const values = columns.map((_, index) => `f${index}`);
  const population =
    `SELECT ${columns.join(', ')}, date_bin(${bucket}::interval, ${time}) AS at, coalesce(verdict, '${NO_VERDICT}') AS verdict_series, verifier_card_uid AS verifier ` +
    `FROM ${table} WHERE ${binds.sql}`;
  const facet = `CASE WHEN grouping(verdict_series) = 0 THEN ${facets} WHEN grouping(verifier) = 0 THEN ${facets + 1} ${columns
    .slice(0, -1)
    .map((column, index) => `WHEN grouping(${column}) = 0 THEN ${index} `)
    .join('')}ELSE ${facets - 1} END`;
  const grouped =
    `SELECT ${facet} AS facet, ${columns.map((column, index) => `${column} AS ${values[index]}`).join(', ')}, at, verdict_series, verifier, count(*) AS n FROM population ` +
    `GROUP BY GROUPING SETS (${[...columns.map((column) => `(${column})`), '(at, verdict_series)', '(at, verifier)'].join(', ')})`;
  const all = `facet, ${values.join(', ')}, at, verdict_series, verifier, n`;
  const ranked = `SELECT ${all}, row_number() OVER (PARTITION BY facet ORDER BY n DESC, ${values.join(', ')}) AS rank, sum(n) OVER (PARTITION BY facet, verifier) AS total FROM grouped`;
  const placed = `SELECT ${all}, rank, dense_rank() OVER (PARTITION BY facet ORDER BY total DESC, verifier) AS place FROM ranked`;
  const counted = `SELECT ${all}, rank, place, max(place) OVER (PARTITION BY facet) AS verifiers FROM placed`;
  const verifierChart = `facet = ${facets + 1}`;
  const kept =
    `SELECT facet, ${values.join(', ')}, at, CASE WHEN facet = ${facets} THEN verdict_series END AS verdict, ` +
    `CASE WHEN ${verifierChart} AND place <= ${NAMED_VERIFIERS} THEN verifier END AS verifier, CASE WHEN ${verifierChart} AND place <= ${NAMED_VERIFIERS} THEN place END AS place, ` +
    `CASE WHEN ${verifierChart} THEN verifiers END AS verifiers, n FROM counted WHERE facet >= ${facets} OR rank <= ${FACET_VALUES + 1}`;
  // Facet rows are already one per value; chart rows of the Verifiers past the named ones fold into one per bucket.
  return {
    sql:
      `WITH ${Object.entries({ population, grouped, ranked, placed, counted, kept }).map(([name, stage]) => `${name} AS (${stage})`).join(', ')} ` +
      `SELECT facet, ${values.join(', ')}, at, verdict, verifier, place, verifiers, sum(n) AS n FROM kept ` +
      `GROUP BY facet, ${values.join(', ')}, at, verdict, verifier, place, verifiers ORDER BY facet, at, n DESC, ${values.join(', ')}, place`,
    params: binds.params
  };
}

/** Props of the Verify charts: executions by verdict, and by Verifier with its named Verifiers in place order and how many others it counts together. */
export interface ExecutionChartsProps {
  readonly verdicts: ChartProps;
  readonly verifiers: ChartProps;
  readonly verifierSeries: { readonly top: readonly string[]; readonly more: number };
}

/** Executions per bucket and `series` under the list's own range and WHERE: the chart's own portable SQL, which a Chart panel or Query reruns. */
function executionsBy(series: string, input: { readonly where?: readonly Filter[] }, range: AbsoluteRange): Executed {
  const { table, time } = SIGNALS.executions;
  const binds = Binds.over(time, range);
  const bin = `date_bin(${binds.bind(bucketFor(range))}::interval, ${time})`;
  binds.where(input.where);
  return { sql: `SELECT ${bin} AS at, ${series}, count(*) AS executions FROM ${table} WHERE ${binds.sql} GROUP BY 1, 2 ORDER BY at`, params: binds.params };
}

/** The rows of one `facet` index of a discovery read as `columns`, renamed. */
function discoveryRows(read: QueryResult, facet: number, columns: readonly [from: string, to: string][]): QueryResult {
  const at = columns.map(([from]) => read.columns.findIndex((column) => column.name === from));
  const rows = read.rows.filter((row) => Number(row[0]) === facet).map((row) => at.map((index) => row[index]));
  return { columns: at.map((index, position) => ({ ...read.columns[index], name: columns[position][1] })), rows, terminal: { ...read.terminal, row_count: rows.length } };
}

/** Load the Verify facets and both charts in one read; its refusal refuses all three. */
export async function loadExecutionDiscovery(input: FacetRailInput, context: LoadContext): Promise<{ readonly facets: FacetRailProps; readonly charts: ExecutionChartsProps }> {
  const chart = (title: string, series: string, sql: string, range?: AbsoluteRange): ChartInput => ({ type: 'timeseries', title, sql, x: 'at', y: ['executions'], series, ...(range ? { range } : {}) });
  const sqlOf = (series: string, range?: AbsoluteRange) => (range ? executionsBy(series, input, range).sql : '-');
  const settled = (range: AbsoluteRange | undefined, verdicts: Loaded<QueryResult>, verifiers: Loaded<QueryResult>, verifierSeries: ExecutionChartsProps['verifierSeries'], executed?: Executed): ExecutionChartsProps => ({
    verdicts: { input: chart('Executions', 'verdict', sqlOf(`coalesce(verdict, '${NO_VERDICT}') AS verdict`, range), range), ...(executed ? { executed } : {}), result: verdicts },
    verifiers: { input: chart('By Verifier', 'verifier', sqlOf('verifier_card_uid AS verifier', range), range), ...(executed ? { executed } : {}), result: verifiers },
    verifierSeries
  });
  const none = { top: [], more: 0 };
  const range = rangeOf(input.range, context);
  if ('code' in range) {
    const result = { state: 'error', problem: range } as const;
    return { facets: { input, result }, charts: settled(undefined, result, result, none) };
  }
  const plan = executionDiscoveryQuery(input, range);
  const [read] = await context.executor.run([query(plan.sql, plan.params)], context.signal);
  if (!read.ok) {
    const result = failed(read.problem);
    return { facets: { input, result }, charts: settled(range, result, result, none, plan) };
  }
  const value = read.value as QueryResult;
  const columns = [...new Set(input.facets)];
  const facets = input.facets.map((column) => ({ column, values: facetValues(value, columns, column, input) }));
  const rail: FacetRailProps = facets.every(({ values }) => values.rows.length === 0)
    ? { input, result: { state: 'empty', data: { facets }, reason: emptyList('executions') } }
    : { input, result: { state: 'populated', data: { facets } } };
  const verdicts = discoveryRows(value, columns.length, [['at', 'at'], ['verdict', 'verdict'], ['n', 'executions']]);
  const verifiers = discoveryRows(value, columns.length + 1, [['at', 'at'], ['verifier', 'verifier'], ['n', 'executions']]);
  const [place, verifier, counted] = ['place', 'verifier', 'verifiers'].map((name) => value.columns.findIndex((column) => column.name === name));
  const chartRows = value.rows.filter((row) => Number(row[0]) === columns.length + 1);
  const top = [...new Map(chartRows.filter((row) => row[place] !== null).map((row) => [Number(row[place]), String(row[verifier])])).entries()].sort(([a], [b]) => a - b).map(([, uid]) => uid);
  const more = Math.max(0, Number(chartRows[0]?.[counted] ?? 0) - NAMED_VERIFIERS);
  const verdictsInput = chart('Executions', 'verdict', executionsBy(`coalesce(verdict, '${NO_VERDICT}') AS verdict`, input, range).sql, range);
  const verifiersInput = chart('By Verifier', 'verifier', executionsBy('verifier_card_uid AS verifier', input, range).sql, range);
  return {
    facets: rail,
    charts: {
      verdicts: { input: verdictsInput, executed: plan, result: shapeChart(verdictsInput, verdicts) },
      verifiers: { input: verifiersInput, executed: plan, result: shapeChart(verifiersInput, verifiers) },
      verifierSeries: { top, more }
    }
  };
}

/** One side of a pull request as its admission pinned it. */
export interface ReviewSetRef {
  readonly ref: string;
  readonly sha: string;
}

/**
 * One review set as retained: the Verifier executions admitted together for
 * one pull request source and review occurrence under one exact Repository
 * policy. `member_ids` is the membership the admission froze, in binding
 * order; a set has no verdict of its own.
 */
export interface ReviewSet {
  readonly generation: string;
  /** The server's observation time of the source, as recorded; the newest one is the current set. */
  readonly recorded_at: unknown;
  readonly host: string;
  readonly repository_id: string;
  readonly number: number;
  /** The pull request's title when this set was admitted; absent on a set recorded before titles were retained. */
  readonly title?: string;
  /** The base repository's `owner/name` when this set was admitted; absent on a set recorded before names were retained. */
  readonly full_name?: string;
  readonly base: ReviewSetRef;
  readonly head: ReviewSetRef;
  /** The exact Repository policy version selected at admission, as recorded: its Card name, version and UID. */
  readonly policy: { readonly name: string; readonly version: string | null; readonly uid: string | null };
  /** The provider delivery that admitted the source, else absent. */
  readonly delivery_id?: string;
  /** The explicit review request that admitted the source, else absent. */
  readonly request_key?: string;
  readonly member_ids: readonly string[];
}

/** One member of a review set: its own recorded status, selected result and verdict, each null when none is recorded. */
export interface ReviewMember {
  readonly execution_id: string;
  /** False while this member's record is not visible in retained data. */
  readonly retained: boolean;
  readonly verifier_card_uid: string | null;
  readonly status: string | null;
  readonly verdict: string | null;
  readonly result_id: string | null;
  /** The requirements the selected result recorded as failed. */
  readonly failed: readonly string[];
}

/** What a pull request's review sets read as: its retained sets newest first, which is current, and the selected set's members. */
export interface ReviewSetsData {
  readonly sets: readonly ReviewSet[];
  /** Older sets exist beyond the {@link REVIEW_SET_ROWS} records one history read returns. */
  readonly more: boolean;
  /** The generation with the newest retained observation. */
  readonly current: string;
  /** The pinned set, else the current one; absent when the pinned set is not retained. `complete` only when every frozen member's record is visible. */
  readonly selected?: ReviewSet & { readonly members: readonly ReviewMember[]; readonly complete: boolean };
}

/** Props of the review sets of one pull request. */
export interface ReviewSetsProps {
  readonly input: ReviewSetsInput;
  /** A pull request with no retained set is `empty` with its reason and no data. */
  readonly result: Part<ReviewSetsData> | { readonly state: 'loading' };
}

/** The most retained member records one review-set history read returns. */
export const REVIEW_SET_ROWS = 200;

// ponytail: no time bound, so old sets stay reachable; the row bound is the only limit. Page by recorded_at if a pull request outgrows it.
const REVIEW_SETS_OF =
  'SELECT subject_version, workload_card_uid, recorded_at, to_json(inputs) AS inputs FROM bifrost.verification.execution_inputs ' +
  "WHERE origin = 'pull_request' AND subject_kind = 'Repository' AND subject_name = $1 AND inputs ->> 'number' = $2";
const REVIEW_SETS = `${REVIEW_SETS_OF} ORDER BY recorded_at DESC, execution_id LIMIT ${REVIEW_SET_ROWS + 1}`;
/** The selected result's requirement outcomes, beside the row Verify lists for the same execution. */
const REVIEW_MEMBERS = `SELECT executions.execution_id, verifier_card_uid, status, verdict, executions.result_id, to_json(o.output) AS output FROM ${SIGNALS.executions.table} LEFT JOIN bifrost.verification.execution_outputs o ON o.result_id = executions.result_id`;
const REVIEW_SET = `${REVIEW_SETS_OF} AND inputs ->> 'generation' = $3 LIMIT 1`;

/** A Variant column arrives as JSON text or as the value itself. */
const variant = (value: unknown): unknown => (typeof value === 'string' ? JSON.parse(value) : value);
const textOf = (value: unknown) => (typeof value === 'string' ? value : null);

/** The review sets of `repository` that `rows` of {@link REVIEW_SETS_OF} name, in row order; every record of a set carries its whole frozen membership. */
function reviewSets(repository: string, rows: readonly (readonly unknown[])[]): ReviewSet[] {
  const sets = new Map<string, ReviewSet>();
  for (const [version, uid, recorded_at, inputs] of rows) {
    const { members, ...source } = variant(inputs) as Omit<ReviewSet, 'recorded_at' | 'policy' | 'member_ids'> & { readonly members?: readonly string[] };
    if (!sets.has(source.generation)) sets.set(source.generation, { ...source, recorded_at, policy: { name: repository, version: textOf(version), uid: textOf(uid) }, member_ids: members ?? [] });
  }
  return [...sets.values()];
}

/**
 * Load one pull request's review sets from its retained admission records,
 * then the selected set's members from the Verify population: two reads,
 * whatever the set's size. The current set is the one with the newest
 * recorded observation, never the newest row to arrive. A pinned generation
 * is read by its own identity, so it is found beyond the history bound and
 * is never replaced by the current set when it is not retained. Each
 * member's status and verdict are the ones Verify lists, read from the set's
 * own observation time on, not the dashboard's range.
 */
export async function loadReviewSets(input: ReviewSetsInput, context: LoadContext): Promise<ReviewSetsProps> {
  const pull = [input.repository, String(input.number)];
  const [history, pin] = await context.executor.run(
    [query(REVIEW_SETS, pull, REVIEW_SET_ROWS + 1), ...(input.generation ? [query(REVIEW_SET, [...pull, input.generation], 1)] : [])],
    context.signal
  );
  const refused = [history, pin].find((read) => read && !read.ok);
  if (!history.ok) return { input, result: failed(history.problem) };
  if (refused && !refused.ok) return { input, result: failed(refused.problem) };
  const sets = reviewSets(input.repository, history.value.rows.slice(0, REVIEW_SET_ROWS));
  if (sets.length === 0) return { input, result: { state: 'empty', reason: 'No review set is retained for this pull request.' } };
  const data = { sets, more: history.value.rows.length > REVIEW_SET_ROWS, current: sets[0].generation };
  const [set] = pin?.ok ? reviewSets(input.repository, pin.value.rows) : sets;
  if (!set) return { input, result: { state: 'populated', data } };

  const observed = parseInstant(set.recorded_at);
  const binds = Binds.over(`executions.${SIGNALS.executions.time}`, { from: new Date(observed - 1).toISOString(), to: new Date(observed + 2).toISOString() });
  binds.clauses.push(`executions.execution_id IN (${set.member_ids.map((id) => binds.bind(id)).join(', ')})`);
  const [listed] = await context.executor.run([query(`${REVIEW_MEMBERS} WHERE ${binds.sql}`, binds.params)], context.signal);
  if (!listed.ok) return { input, result: failed(listed.problem) };
  const rows = new Map(listed.value.rows.map((row) => [String(row[0]), row]));
  const members = set.member_ids.map((execution_id): ReviewMember => {
    const row = rows.get(execution_id);
    const judged = (variant(row?.[5] ?? null) as { requirements?: readonly { id: string; outcome: string }[] } | null)?.requirements ?? [];
    const failed = judged.filter((requirement) => requirement.outcome === 'failed').map((requirement) => requirement.id);
    return { execution_id, retained: row !== undefined, verifier_card_uid: textOf(row?.[1]), status: textOf(row?.[2]), verdict: textOf(row?.[3]), result_id: textOf(row?.[4]), failed };
  });
  return { input, result: { state: 'populated', data: { ...data, selected: { ...set, members, complete: members.every((member) => member.retained) } } } };
}

/** How far a trace's spans may start outside its root's own start and end: clock skew between services, and work that outlives the root. */
const TRACE_SLACK_MS = 3_600_000;

/**
 * Span and error counts of exactly these traces. `roots` is the time their
 * root spans cover, from the earliest start to the latest end; the read keeps
 * spans starting within it, widened by {@link TRACE_SLACK_MS} each side, so
 * it opens only those partitions. Blooms prune an id list of more than three
 * values no further than its min/max statistics, so the time bound carries it.
 */
export function countsQuery(traceIds: readonly string[], roots: { readonly from: number; readonly to: number }): Executed {
  const binds = Binds.over(SIGNALS.spans.time, { from: new Date(roots.from - TRACE_SLACK_MS).toISOString(), to: new Date(roots.to + TRACE_SLACK_MS).toISOString() });
  const ids = traceIds.map((id) => `decode(${binds.bind(id)}, 'hex')`).join(', ');
  // ponytail: a span starting over an hour outside its root's start and end goes uncounted; widen the slack if traces do that.
  return {
    sql:
      `SELECT encode(trace_id, 'hex') AS trace_id, count(*) AS spans, count(*) FILTER (WHERE status_code = ${STATUS_ERROR}) AS errors ` +
      `FROM ${SIGNALS.spans.table} WHERE ${binds.sql} AND trace_id IN (${ids}) GROUP BY trace_id`,
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
  const [ti, di] = [columnIndex(rows, SIGNALS.traces.time), columnIndex(rows, 'duration_nano')];
  const starts = rows.rows.map((row) => parseInstant(row[ti]));
  const ends = rows.rows.map((row, index) => starts[index] + Math.ceil(numeric(row[di] ?? 0) / 1e6));
  const counted = countsQuery([...new Set(rows.rows.map((row) => String(row[at])))], { from: Math.min(...starts), to: Math.max(...ends) });
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
  gen_ai_operation_name: 'Operation',
  recorded_at: 'Time',
  ended_at: 'Ended',
  verifier_card_uid: 'Verifier',
  subject_name: 'Card',
  subject_kind: 'Card kind',
  subject_version: 'Card version',
  run_id: 'Run',
  group_id: 'Session',
  pull_request: 'Pull request',
  review_set: 'Review set',
  head: 'Head',
  policy: 'Policy',
  binding_id: 'Binding',
  origin: 'Origin',
  status: 'Status',
  verdict: 'Verdict',
  execution_id: 'Execution',
  result_id: 'Result',
  service_version: 'Service version',
  deployment_environment: 'Environment'
};

/** Whether `column` holds an instant: a signal table's nanosecond time, or a verification table's `_at` timestamp. */
export function isInstant(column: string): boolean {
  return column.endsWith('_unix_nano') || column.endsWith('_at');
}

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
  const instant = isInstant(column) && typeof value === 'string' && /^\d{4}-(\d\d-\d\d)T(\d\d:\d\d:\d\d)(\.\d{1,3})?/.exec(value);
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
