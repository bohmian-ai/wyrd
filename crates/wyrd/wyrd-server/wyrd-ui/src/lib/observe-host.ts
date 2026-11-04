/**
 * The Observe host's navigation state. Everything an investigation needs to
 * survive reload, Back or a shared link lives in the URL; this module reads
 * it, writes it, names the shared loader inputs the server loads run, and
 * maps each component intent to the next URL.
 */
import type { ActionIntent, QueryParam } from 'wyrd-ai-ui';
import { METRIC_AGGREGATIONS, SIGNALS, signalColumns, type MetricAggregation, type MetricInput } from 'wyrd-ai-ui/loaders';

/** The Observe signals the host offers. */
export const OBSERVE_SIGNALS = ['traces', 'logs', 'metrics'] as const;
/** Relative ranges the time picker offers. */
export const PRESETS = ['5m', '15m', '30m', '1h', '6h', '24h', '7d', '30d'] as const;
/** The search params that name the open record; the list loads never read them. */
export const DETAIL_PARAMS = ['trace', 'span', 'log'] as const;

/** One applied WHERE clause, as the shared `where` input reads it. */
export interface HostFilter {
  readonly column: string;
  readonly value: QueryParam;
  readonly not?: boolean;
}

/** One log record's exact identity. */
export interface HostLogKey {
  readonly time_unix_nano: string;
  readonly observed_time_unix_nano: string;
  readonly wyrd_request_id: string;
}

/** The whole investigation state; every field round-trips through the URL. */
export interface ObserveState {
  readonly signal: (typeof OBSERVE_SIGNALS)[number];
  readonly mode: 'traces' | 'spans';
  /** The relative preset; with `from`/`to` it is the range a zoom resets to. */
  readonly last: string;
  /** An absolute range, custom or zoomed, applied over the preset. */
  readonly from?: string;
  readonly to?: string;
  /** The zone instants are shown and typed in. */
  readonly tz: 'utc' | 'local';
  readonly where: readonly HostFilter[];
  /** The applied, server-normalized predicate; blank applies none. */
  readonly where_sql: string;
  readonly group?: string;
  readonly live: boolean;
  /** The list columns the reader chose; absent shows the signal's defaults. */
  readonly columns?: readonly string[];
  /** The metric aggregation across series, and the metrics column it keeps apart. */
  readonly agg?: MetricAggregation;
  readonly by?: string;
  /** Metrics' recorded-points inspector is open; only then is the points list read. */
  readonly points?: boolean;
  readonly trace?: string;
  readonly span?: string;
  readonly log?: HostLogKey;
}

/** Search params, or the subset of them a load may depend on. */
type Params = Pick<URLSearchParams, 'get'>;

/** Parse `text` as JSON, or `undefined` when it is absent or malformed. */
function json(text: string | null): unknown {
  if (text === null) return undefined;
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

/** Facets each signal's rail counts. */
export const FACETS = {
  traces: ['service_name', 'name', 'status_code', 'http_response_status_code', 'gen_ai_operation_name'],
  spans: ['service_name', 'name', 'kind', 'status_code', 'http_response_status_code'],
  logs: ['severity_text', 'service_name', 'event_name', 'exception_type'],
  metrics: ['service_name', 'subject_card_uid']
} as const;

/**
 * Read the state from search params. Unknown or malformed values fall back to
 * the defaults (Traces, last hour, UTC); panel schemas validate everything else.
 */
export function readState(params: Params): ObserveState {
  const signal = OBSERVE_SIGNALS.find((each) => each === params.get('signal')) ?? 'traces';
  const where = json(params.get('where'));
  const columns = json(params.get('columns'));
  const log = json(params.get('log'));
  const from = params.get('from') ?? undefined;
  const to = params.get('to') ?? undefined;
  const absolute = from !== undefined && to !== undefined;
  const agg = METRIC_AGGREGATIONS.find((each) => each === params.get('agg'));
  return {
    signal,
    mode: params.get('mode') === 'spans' ? 'spans' : 'traces',
    last: params.get('last') ?? '1h',
    from: absolute ? from : undefined,
    to: absolute ? to : undefined,
    tz: params.get('tz') === 'local' ? 'local' : 'utc',
    where: Array.isArray(where) ? (where as HostFilter[]) : [],
    where_sql: params.get('where_sql') ?? '',
    group: params.get('group') ?? undefined,
    live: params.get('live') === '1',
    columns: Array.isArray(columns) && columns.every((each) => typeof each === 'string') ? columns : undefined,
    agg,
    by: params.get('by') ?? undefined,
    points: signal === 'metrics' && params.get('points') === '1',
    trace: params.get('trace') ?? undefined,
    span: params.get('span') ?? undefined,
    log: log && typeof log === 'object' ? (log as HostLogKey) : undefined
  };
}

/** Write the state as search params, omitting defaults so links stay short. */
export function writeState(state: ObserveState): URLSearchParams {
  const params = new URLSearchParams();
  if (state.signal !== 'traces') params.set('signal', state.signal);
  if (state.signal === 'traces' && state.mode === 'spans') params.set('mode', 'spans');
  if (state.last !== '1h') params.set('last', state.last);
  if (state.from !== undefined && state.to !== undefined) {
    params.set('from', state.from);
    params.set('to', state.to);
  }
  if (state.tz === 'local') params.set('tz', 'local');
  if (state.where.length) params.set('where', JSON.stringify(state.where));
  if (state.where_sql.trim()) params.set('where_sql', state.where_sql);
  if (state.group) params.set('group', state.group);
  if (state.live) params.set('live', '1');
  if (state.columns) params.set('columns', JSON.stringify(state.columns));
  if (state.agg) params.set('agg', state.agg);
  if (state.by) params.set('by', state.by);
  if (state.signal === 'metrics' && state.points) params.set('points', '1');
  if (state.trace) params.set('trace', state.trace);
  if (state.span) params.set('span', state.span);
  if (state.log) params.set('log', JSON.stringify(state.log));
  return params;
}

/** The search params a list load may read: every param but the open record's. */
export function listParams(params: Params): Params {
  return { get: (name: string) => ((DETAIL_PARAMS as readonly string[]).includes(name) ? null : params.get(name)) };
}

/** The shared loaders' signal for this state: Traces lists roots or, in Spans mode, every span. */
export function listSignal(state: ObserveState): 'traces' | 'spans' | 'logs' | 'metrics' {
  return state.signal === 'traces' ? state.mode : state.signal;
}

/** Columns a list of the state's signal may group by: its shown columns, then its scalar filters. */
export function groupColumns(state: ObserveState): readonly string[] {
  const signal = SIGNALS[listSignal(state)];
  return [...new Set<string>([...signal.columns, ...signal.filters])];
}

/** The metric the WHERE selects: its one `metric_name =` clause. */
export function selectedMetric(state: ObserveState): string | undefined {
  const chosen = state.where.filter((each) => each.column === 'metric_name' && !each.not && typeof each.value === 'string');
  return chosen.length === 1 ? String(chosen[0].value) : undefined;
}

/** The applied range: the absolute bounds, else the relative preset. */
export function appliedRange(state: ObserveState): { from: string; to: string } | { last: string } {
  return state.from !== undefined && state.to !== undefined ? { from: state.from, to: state.to } : { last: state.last };
}

/** The applied WHERE and predicate every read of the main view shares. */
function filter(state: ObserveState) {
  return { where: state.where, ...(state.where_sql.trim() ? { where_sql: state.where_sql } : {}) };
}

/**
 * The shared loader inputs of the main view: facets and the list for every
 * signal, and the selected metric's series. Every one reads the same range,
 * WHERE and predicate.
 */
export function listInputs(state: ObserveState) {
  const signal = listSignal(state);
  const group = state.group && groupColumns(state).includes(state.group) ? state.group : undefined;
  const offered = signalColumns(signal);
  const chosen = state.columns?.filter((column) => offered.includes(column));
  const columns = chosen?.length ? chosen : undefined;
  const metric = state.signal === 'metrics' ? selectedMetric(state) : undefined;
  const by = state.by && (SIGNALS.metrics.filters as readonly string[]).includes(state.by) ? state.by : undefined;
  const series: MetricInput | undefined = metric ? { metric, ...filter(state), ...(state.agg ? { aggregation: state.agg } : {}), ...(by ? { by } : {}) } : undefined;
  return {
    filter: filter(state),
    facets: { signal, facets: [...FACETS[signal]], ...filter(state) },
    list: {
      signal,
      ...filter(state),
      ...(group && !(columns ?? (SIGNALS[signal].columns as readonly string[])).includes(group) ? { columns: [...(columns ?? SIGNALS[signal].columns), group] } : columns ? { columns } : {}),
      ...(group ? { groupBy: group } : {}),
      ...(state.live ? { live: true } : {})
    },
    series
  };
}

/** The state with nothing open in the drawer. */
export function closed(state: ObserveState): ObserveState {
  return { ...state, trace: undefined, span: undefined, log: undefined };
}

/** The state over another absolute range, keeping the preset a reset returns to. */
export function zoomed(state: ObserveState, from: number, to: number): ObserveState {
  return { ...closed(state), from: new Date(from).toISOString(), to: new Date(to).toISOString() };
}

/** Where an intent leads: the next Observe state, or the Query inspector's search params. */
export type HostStep = { readonly state: ObserveState } | { readonly query: URLSearchParams } | { readonly unavailable: string };

/**
 * Map one validated component intent to the next step. Filters dedupe by
 * column and value; a brush applies its range to every panel; opening a
 * record from Logs switches to its trace with the Logs state still in
 * history, so Back returns to it.
 */
export function step(state: ObserveState, intent: ActionIntent, back: string): HostStep {
  const input = intent.input as Record<string, unknown>;
  switch (intent.name) {
    case 'facet.filter':
    case 'record.filter': {
      const clause: HostFilter = { column: String(input.column), value: input.value as QueryParam, ...(input.not ? { not: true } : {}) };
      const where = [...state.where.filter((each) => each.column !== clause.column || each.value !== clause.value), clause];
      return { state: { ...closed(state), where } };
    }
    case 'chart.range':
      return { state: zoomed(state, Date.parse(String(input.from)), Date.parse(String(input.to))) };
    case 'record.open':
      if (input.log) return { state: { ...state, trace: undefined, span: undefined, log: input.log as HostLogKey } };
      return {
        state: {
          ...state,
          ...(state.signal === 'logs' ? { signal: 'traces', mode: 'traces', where: [], where_sql: '', group: undefined, columns: undefined } : {}),
          log: undefined,
          trace: String(input.trace_id),
          span: input.span_id === undefined ? undefined : String(input.span_id)
        }
      };
    case 'trace.select':
      return { state: { ...state, trace: String(input.trace_id), span: String(input.span_id) } };
    case 'record.open_in_query':
    case 'chart.open_in_query':
      return { query: new URLSearchParams({ sql: String(input.sql), params: JSON.stringify(input.params), back }) };
    default:
      return { unavailable: `Observe does not offer ${intent.name} yet` };
  }
}
