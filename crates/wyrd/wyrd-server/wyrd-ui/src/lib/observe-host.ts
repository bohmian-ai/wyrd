/**
 * The Observe host's navigation state. Everything an investigation needs to
 * survive reload or a shared link lives in the URL; this module reads it,
 * writes it, plans the view documents the page renders through the shared
 * loaders, and maps each component intent to the next URL.
 */
import type { ActionIntent, QueryParam } from 'wyrd-ai-ui';
import { SIGNALS, metricPanels } from 'wyrd-ai-ui/loaders';

/** The Observe signals the host offers. */
export const OBSERVE_SIGNALS = ['traces', 'logs', 'metrics'] as const;
/** Relative ranges the time picker offers. */
export const PRESETS = ['15m', '1h', '6h', '24h', '7d'] as const;

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
  /** A relative range, or absent when `from`/`to` hold an absolute one. */
  readonly last?: string;
  readonly from?: string;
  readonly to?: string;
  readonly where: readonly HostFilter[];
  /** The applied, server-normalized predicate; blank applies none. */
  readonly where_sql: string;
  readonly group?: string;
  readonly live: boolean;
  readonly trace?: string;
  readonly span?: string;
  readonly log?: HostLogKey;
}

/** The view-document header every Observe view carries. */
const VIEW = { version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1' } as const;

/** Recorded histogram and summary columns a metrics list shows beside the scalar values. */
const STRUCTURES = ['histogram_count', 'histogram_sum', 'bucket_counts', 'explicit_bounds', 'summary_count', 'quantile_values'] as const;

/** Facets each signal's rail counts. */
const FACETS = {
  traces: ['service_name', 'name', 'status_code', 'http_response_status_code', 'gen_ai_operation_name'],
  spans: ['service_name', 'name', 'kind', 'status_code', 'http_response_status_code'],
  logs: ['severity_text', 'service_name', 'event_name', 'exception_type'],
  metrics: ['metric_name', 'metric_type', 'unit', 'aggregation_temporality', 'service_name']
} as const;

/** Parse `text` as JSON, or `undefined` when it is absent or malformed. */
function json(text: string | null): unknown {
  if (text === null) return undefined;
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

/**
 * Read the state from search params. Unknown or malformed values fall back to
 * the defaults (Traces, last hour); panel schemas validate everything else.
 */
export function readState(params: URLSearchParams): ObserveState {
  const signal = OBSERVE_SIGNALS.find((each) => each === params.get('signal')) ?? 'traces';
  const where = json(params.get('where'));
  const log = json(params.get('log'));
  const from = params.get('from') ?? undefined;
  const to = params.get('to') ?? undefined;
  const absolute = from !== undefined && to !== undefined;
  return {
    signal,
    mode: params.get('mode') === 'spans' ? 'spans' : 'traces',
    last: absolute ? undefined : (params.get('last') ?? '1h'),
    from: absolute ? from : undefined,
    to: absolute ? to : undefined,
    where: Array.isArray(where) ? (where as HostFilter[]) : [],
    where_sql: params.get('where_sql') ?? '',
    group: params.get('group') ?? undefined,
    live: params.get('live') === '1',
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
  if (state.from !== undefined && state.to !== undefined) {
    params.set('from', state.from);
    params.set('to', state.to);
  } else if (state.last && state.last !== '1h') params.set('last', state.last);
  if (state.where.length) params.set('where', JSON.stringify(state.where));
  if (state.where_sql.trim()) params.set('where_sql', state.where_sql);
  if (state.group) params.set('group', state.group);
  if (state.live) params.set('live', '1');
  if (state.trace) params.set('trace', state.trace);
  if (state.span) params.set('span', state.span);
  if (state.log) params.set('log', JSON.stringify(state.log));
  return params;
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

/** The view range: the absolute bounds, or the relative preset. */
function range(state: ObserveState) {
  return state.from !== undefined && state.to !== undefined ? { from: state.from, to: state.to } : { last: state.last ?? '1h' };
}

/**
 * The main view: overview, facets and the list, all under one range, one
 * WHERE and one predicate. A blank predicate is left out. A selected metric
 * adds its series chart and exact summary, which embed `predicate`: the
 * server-normalized `where_sql`, or `null` when it was refused, which leaves
 * them out while the other panels report the refusal.
 */
export function observeView(state: ObserveState, predicate?: string | null) {
  const signal = listSignal(state);
  const filter = { where: state.where, ...(state.where_sql.trim() ? { where_sql: state.where_sql } : {}) };
  const group = state.group && groupColumns(state).includes(state.group) ? state.group : undefined;
  // Metric points also show their recorded histogram and summary structures, which no chart plots.
  const columns: readonly string[] = signal === 'metrics' ? [...SIGNALS.metrics.columns, ...STRUCTURES] : SIGNALS[signal].columns;
  const panels: { id: string; component: string; input: unknown; layout?: unknown }[] = [];
  if (state.signal === 'traces')
    panels.push({ id: 'overview', component: 'StatStrip', input: { stats: ['traces', 'errors', 'duration_p95'], ...filter } });
  if (state.signal === 'logs') panels.push({ id: 'overview', component: 'StatStrip', input: { stats: ['logs', 'log_errors'], ...filter } });
  panels.push(
    { id: 'facets', component: 'FacetRail', input: { signal, facets: FACETS[signal], ...filter }, layout: { x: 0, y: 0, w: 3, h: 14 } },
    {
      id: 'list',
      component: 'RecordTable',
      input: {
        signal,
        ...filter,
        ...(group && !columns.includes(group) ? { columns: [...columns, group] } : signal === 'metrics' ? { columns } : {}),
        ...(group ? { groupBy: group } : {}),
        ...(state.live ? { live: true } : {})
      },
      layout: { x: 3, y: 0, w: 9, h: 14 }
    }
  );
  const metric = state.signal === 'metrics' ? selectedMetric(state) : undefined;
  if (metric && predicate !== null) {
    const { chart, summary } = metricPanels(metric, state.where, group, predicate);
    panels.splice(0, 0, { id: 'series', component: 'Chart', input: chart }, { id: 'summary', component: 'RecordTable', input: { sql: summary } });
  }
  return { ...VIEW, range: range(state), panels };
}

/** The drawer view: the exact trace and selected span, or the exact log record; `undefined` when nothing is open. */
export function detailView(state: ObserveState) {
  if (state.log) return { ...VIEW, range: range(state), panels: [{ id: 'detail', component: 'RecordDetail', input: { log: state.log } }] };
  if (!state.trace) return undefined;
  const panels: { id: string; component: string; input: unknown }[] = [
    { id: 'waterfall', component: 'TraceWaterfall', input: { traceId: state.trace, ...(state.span ? { spanId: state.span } : {}) } }
  ];
  if (state.span) panels.push({ id: 'detail', component: 'RecordDetail', input: { traceId: state.trace, spanId: state.span } });
  return { ...VIEW, range: range(state), panels };
}

/** The state with nothing open in the drawer. */
export function closed(state: ObserveState): ObserveState {
  return { ...state, trace: undefined, span: undefined, log: undefined };
}

/** Where an intent leads: the next Observe state, or the Query inspector's search params. */
export type HostStep = { readonly state: ObserveState } | { readonly query: URLSearchParams } | { readonly unavailable: string };

/**
 * Map one validated component intent to the next step. Filters dedupe by
 * column and value; opening a record from Logs switches to its trace with the
 * Logs state still in history, so Back returns to it.
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
    case 'record.open':
      if (input.log) return { state: { ...state, trace: undefined, span: undefined, log: input.log as HostLogKey } };
      return {
        state: {
          ...state,
          ...(state.signal === 'logs' ? { signal: 'traces', mode: 'traces', where: [], where_sql: '', group: undefined } : {}),
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
