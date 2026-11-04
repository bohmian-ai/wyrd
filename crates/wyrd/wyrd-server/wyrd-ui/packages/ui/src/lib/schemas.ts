/**
 * The catalog components' input and action schemas, declared once.
 *
 * `scripts/catalog.ts` emits these as `catalog.json` with `z.toJSONSchema`;
 * view validation, the BFF load endpoint, and hosts parse with the same
 * values, so only these schemas define what a document or intent may carry.
 */
import { z } from 'zod';
import type { CardKind, CardRef } from 'wyrd-ai';

/** Every Card kind the registry serves, checked against the generated union both ways. */
const CARD_KINDS = [
  'Data',
  'Model',
  'Experiment',
  'Prompt',
  'Agent',
  'Verifier',
  'Service',
  'Policy',
  'Mcp',
  'Audit',
  'Artifact',
  'Trigger',
  'Operator',
  'Source'
] as const satisfies readonly CardKind[];
const everyKind: CardKind extends (typeof CARD_KINDS)[number] ? true : never = true;
void everyKind;

/** An exact Card reference: a full version pin, never a range or "latest". */
export const CardRefSchema = z
  .object({
    kind: z.enum(CARD_KINDS),
    name: z.string().min(1),
    version: z
      .string()
      .regex(/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/, 'an exact version such as 1.4.2'),
    uid: z.string().min(1).nullish()
  })
  .strict() satisfies z.ZodType<CardRef>;

/** An RFC 3339 instant with an explicit offset. */
const instant = z.iso.datetime({ offset: true });

/** An absolute time range; `from` precedes `to`. */
export const AbsoluteRangeSchema = z
  .object({ from: instant, to: instant })
  .strict()
  .refine((range) => Date.parse(range.from) < Date.parse(range.to), {
    message: 'from must precede to',
    path: ['to']
  });

/** A range relative to when the view loads, such as `{"last": "6h"}`. */
export const RelativeRangeSchema = z
  .object({ last: z.string().regex(/^[1-9]\d{0,4}[mhd]$/, 'up to five digits of minutes, hours, or days, such as 6h') })
  .strict();

/** A view or panel time range. */
export const RangeSchema = z.union([AbsoluteRangeSchema, RelativeRangeSchema]);

/** A result column named by a chart definition. */
const column = z.string().min(1);

/** Fields every chart type shares. */
const chartFields = {
  title: z.string().min(1).optional(),
  sql: z.string().min(1).max(65_536),
  unit: z.string().min(1).optional(),
  range: RangeSchema.optional()
};

/**
 * A chart definition: REQ-025 fields plus an optional range.
 *
 * The SQL runs unchanged. A range-aware chart reads `$1`/`$2` as the range's
 * RFC 3339 bounds and `$3` as the bucket interval, casting them in SQL.
 */
export const ChartInputSchema = z.discriminatedUnion('type', [
  z.object({ type: z.literal('timeseries'), ...chartFields, x: column, y: z.array(column).min(1), series: column.optional() }).strict(),
  z.object({ type: z.literal('bar'), ...chartFields, x: column, y: z.array(column).min(1) }).strict(),
  z.object({ type: z.literal('stat'), ...chartFields, y: z.array(column).length(1).optional() }).strict(),
  z.object({ type: z.literal('table'), ...chartFields }).strict()
]);

/** The input of `CardHeader`: one exact Card plus the host's tabs. */
export const CardHeaderInputSchema = z
  .object({
    cardRef: CardRefSchema,
    tabs: z
      .array(z.object({ id: z.string().min(1), label: z.string().min(1) }).strict())
      .max(12)
      .optional(),
    tab: z.string().min(1).optional()
  })
  .strict();

/** A bind value: JSON null, boolean, number, or string. */
const queryParam = z.union([z.null(), z.boolean(), z.number(), z.string()]);

/** Wyrd ownership columns every canonical signal table carries. */
const OWNERSHIP = ['workload_card_uid', 'subject_card_uid', 'run_id'] as const;
/** Resource labels the signal tables promote; kept apart from Wyrd ownership. */
const RESOURCE = ['service_name', 'service_version', 'service_namespace', 'deployment_environment', 'scope_name'] as const;

/** Trace and span ids, filtered by their lowercase hex text. */
export const IDS = ['trace_id', 'span_id'] as const;

/** Span columns a filter may name. */
const SPAN_FILTERS = [
  'name', 'kind', 'status_code', ...RESOURCE, 'gen_ai_operation_name', 'gen_ai_provider_name', 'gen_ai_request_model',
  'gen_ai_conversation_id', 'http_request_method', 'http_route', 'http_response_status_code', 'url_path', 'exception_type',
  'is_exception', ...OWNERSHIP
] as const;

/**
 * The Observe signals and their canonical tables. `traces` lists root spans,
 * one row per trace; `spans` lists every span. `filters` are the scalar,
 * non-payload columns a WHERE clause or facet may name; `columns` are a
 * list's defaults and `more` the other columns it may show, such as metric
 * histogram, summary and exemplar structures. Every name is a current
 * canonical column.
 */
export const SIGNALS = {
  traces: {
    table: 'bifrost.traces.spans',
    time: 'start_time_unix_nano',
    columns: ['start_time_unix_nano', 'name', 'service_name', 'duration_nano', 'status_code'],
    filters: SPAN_FILTERS,
    more: []
  },
  spans: {
    table: 'bifrost.traces.spans',
    time: 'start_time_unix_nano',
    columns: ['start_time_unix_nano', 'name', 'service_name', 'duration_nano', 'status_code'],
    filters: SPAN_FILTERS,
    more: []
  },
  logs: {
    table: 'bifrost.logs.records',
    time: 'time_unix_nano',
    columns: ['time_unix_nano', 'severity_text', 'service_name', 'body_text', 'exception_type'],
    filters: ['severity_number', 'severity_text', 'event_name', ...RESOURCE, 'exception_type', 'is_exception', ...OWNERSHIP, ...IDS],
    more: []
  },
  metrics: {
    table: 'bifrost.metrics.points',
    time: 'time_unix_nano',
    columns: ['time_unix_nano', 'metric_name', 'metric_type', 'unit', 'int_value', 'double_value'],
    filters: ['metric_name', 'metric_type', 'unit', 'aggregation_temporality', 'is_monotonic', ...RESOURCE, ...OWNERSHIP],
    more: [
      'start_time_unix_nano', 'histogram_count', 'histogram_sum', 'histogram_min', 'histogram_max', 'bucket_counts', 'explicit_bounds',
      'exponential_scale', 'exponential_zero_count', 'exponential_zero_threshold', 'positive_buckets', 'negative_buckets',
      'summary_count', 'summary_sum', 'quantile_values', 'exemplars'
    ]
  }
} as const;

/** An Observe signal. */
export type Signal = keyof typeof SIGNALS;
const SignalSchema = z.enum(['traces', 'spans', 'logs', 'metrics']);

/** Columns a list of `signal` may show: its defaults, every filter column, then its other columns. */
export function signalColumns(signal: Signal): readonly string[] {
  return [...new Set([...SIGNALS[signal].columns, ...SIGNALS[signal].filters, ...SIGNALS[signal].more])];
}

/** One WHERE clause: a column equal to a bound value, or with `not`, distinct from it. A null value matches an unset column. */
export const FilterSchema = z.object({ column: z.string().min(1), value: queryParam, not: z.boolean().optional() }).strict();

/** Clauses ANDed into every read of one signal. */
const where = z.array(FilterSchema).max(32);

/**
 * One SQL predicate over the signal table's own columns, ANDed with `where`.
 * The server validates and normalizes it before any read that uses it; blank
 * means no extra predicate.
 */
const whereSql = z.string().max(8192);

/** Refuse a column the signal does not offer, naming the field. */
function knownColumns(signal: Signal, names: readonly string[], allowed: readonly string[], path: PropertyKey[], context: z.RefinementCtx) {
  names.forEach((name, index) => {
    if (!allowed.includes(name)) context.addIssue({ code: 'custom', path: [...path, index], message: `${name} is not a ${signal} column` });
  });
}

/** Lowercase hex trace and span identities, as the signal tables' binary ids read. */
const traceId = z.string().regex(/^[0-9a-f]{32}$/, '32 lowercase hex digits');
const spanId = z.string().regex(/^[0-9a-f]{16}$/, '16 lowercase hex digits');

/** The input of `FacetRail`: value counts of `facets` under the list's own WHERE and range. */
export const FacetRailInputSchema = z
  .object({
    signal: SignalSchema,
    where: where.optional(),
    where_sql: whereSql.optional(),
    facets: z.array(z.string().min(1)).min(1).max(12),
    range: RangeSchema.optional()
  })
  .strict()
  .superRefine((input, context) => {
    knownColumns(input.signal, input.facets, SIGNALS[input.signal].filters, ['facets'], context);
    knownColumns(input.signal, (input.where ?? []).map((each) => each.column), SIGNALS[input.signal].filters, ['where'], context);
  });

/** List fields shared by both `RecordTable` sources. */
const listFields = {
  range: RangeSchema.optional(),
  groupBy: z.string().min(1).optional(),
  live: z.boolean().optional(),
  limit: z.number().int().min(1).max(500).optional()
};

/**
 * The input of `RecordTable`: one signal under WHERE and range, or authored
 * read-only SQL run unchanged with `$1`/`$2` bound to the range.
 */
export const RecordTableInputSchema = z.union([
  z
    .object({
      signal: SignalSchema,
      where: where.optional(),
      where_sql: whereSql.optional(),
      columns: z.array(z.string().min(1)).min(1).max(24).optional(),
      ...listFields
    })
    .strict()
    .superRefine((input, context) => {
      const columns = signalColumns(input.signal);
      knownColumns(input.signal, input.columns ?? [], columns, ['columns'], context);
      knownColumns(input.signal, (input.where ?? []).map((each) => each.column), SIGNALS[input.signal].filters, ['where'], context);
      if (input.groupBy !== undefined) knownColumns(input.signal, [input.groupBy], input.columns ?? SIGNALS[input.signal].columns, ['groupBy'], context);
    }),
  z.object({ sql: z.string().min(1).max(65_536), ...listFields }).strict()
]);

/** The input of `TraceWaterfall`: one exact trace; `spanId` marks the selected span. Range is navigation context only. */
export const TraceWaterfallInputSchema = z.object({ traceId, spanId: spanId.optional(), range: RangeSchema.optional() }).strict();

/** One log record's exact identity: its event and observed instants and the request that wrote it. */
export const LogKeySchema = z
  .object({ time_unix_nano: z.string().min(1), observed_time_unix_nano: z.string().min(1), wyrd_request_id: z.string().min(1) })
  .strict();

/** The input of `RecordDetail`: one exact span, or one log record. */
export const RecordDetailInputSchema = z.union([z.object({ traceId, spanId }).strict(), z.object({ log: LogKeySchema }).strict()]);

/**
 * Built-in stats over the canonical tables. Span and log stats honour `where`;
 * `invocations` counts `invoke_agent` operations; `logs` counts log records and
 * `log_errors` those of severity ERROR or above; `tokens` (token-unit usage),
 * `calls` and `cost` read `bifrost.gateway.calls`.
 */
export const STATS = ['traces', 'spans', 'errors', 'duration_p95', 'invocations', 'logs', 'log_errors', 'tokens', 'calls', 'cost'] as const;

/** Whether a strip reads log records: its `where` then names log columns. */
export const logStats = (stats: readonly string[]) => stats.some((stat) => stat === 'logs' || stat === 'log_errors');

/** The input of `StatStrip`: up to five stats under one range and an optional Wyrd ownership scope. */
export const StatStripInputSchema = z
  .object({
    stats: z.array(z.enum(STATS)).min(1).max(5),
    scope: z.object({ column: z.enum(['workload_card_uid', 'subject_card_uid']), uid: z.string().min(1) }).strict().optional(),
    where: where.optional(),
    where_sql: whereSql.optional(),
    range: RangeSchema.optional()
  })
  .strict()
  .superRefine((input, context) => {
    const logs = logStats(input.stats);
    knownColumns(logs ? 'logs' : 'spans', (input.where ?? []).map((each) => each.column), SIGNALS[logs ? 'logs' : 'spans'].filters, ['where'], context);
    const filtered = !!input.where?.length || !!input.where_sql?.trim();
    if (logs && filtered && input.stats.some((stat) => !logStats([stat])))
      context.addIssue({ code: 'custom', path: ['stats'], message: 'one strip filters either log records or spans, not both' });
    const gateway = input.stats.some((stat) => stat === 'tokens' || stat === 'calls' || stat === 'cost');
    if (input.where?.length && gateway)
      context.addIssue({ code: 'custom', path: ['where'], message: 'span filters do not apply to gateway tokens, calls or cost' });
    if (input.where_sql?.trim() && gateway)
      context.addIssue({ code: 'custom', path: ['where_sql'], message: 'span predicates do not apply to gateway tokens, calls or cost' });
  });

/** A filter a facet or detail field asks the host to apply. */
const filterIntent = z.object({ signal: SignalSchema, column: z.string().min(1), value: queryParam, not: z.boolean().optional() }).strict();
/** One exact span, as a navigation target. */
const spanRef = z.object({ trace_id: traceId, span_id: spanId.optional() }).strict();

/** Exact executed SQL and its typed binds, as a navigation target. */
const executedQuery = z.object({ sql: z.string().min(1), params: z.array(queryParam) }).strict();

/** How an action's completion becomes observable. */
type Completion = 'host-navigation' | 'view-range';

/** One action a component emits, with its input schema and completion. */
interface ActionSpec {
  readonly description: string;
  readonly input: z.ZodType;
  readonly completion: Completion;
}

/** One catalog component's declaration. */
interface ComponentSpec {
  readonly description: string;
  readonly input: z.ZodType;
  readonly states: readonly string[];
  readonly emits: Readonly<Record<string, ActionSpec>>;
}

/** Every catalog component, by name. Primitives and shell components are not here. */
export const components = {
  Chart: {
    description:
      'One chart over read-only SQL: timeseries, bar, stat, or table, with title, source caption, ' +
      'Open in Query, and in-chart errors. $1/$2 bind the range as RFC 3339 strings and $3 the bucket.',
    input: ChartInputSchema,
    states: ['populated', 'empty', 'denied', 'error', 'loading'],
    emits: {
      'chart.range': {
        description: 'A time-series brush selected this absolute range for the whole view.',
        input: AbsoluteRangeSchema,
        completion: 'view-range'
      },
      'chart.open_in_query': {
        description: 'Open the exact executed SQL and its typed binds in Query.',
        input: executedQuery,
        completion: 'host-navigation'
      }
    }
  },
  CardHeader: {
    description:
      'An exact Card version: name, kind, version select, status, uid copy, tabs, and the primary action.',
    input: CardHeaderInputSchema,
    states: ['populated', 'denied', 'error', 'loading'],
    emits: {
      'card.version': {
        description: 'Show another exact version of this Card.',
        input: z.object({ card_ref: CardRefSchema }).strict(),
        completion: 'host-navigation'
      },
      'card.tab': {
        description: 'Show one of the host-supplied tabs.',
        input: z.object({ tab: z.string().min(1) }).strict(),
        completion: 'host-navigation'
      },
      'card.open_in_observe': {
        description: "Open this Card's telemetry in Observe.",
        input: z.object({ card_ref: CardRefSchema }).strict(),
        completion: 'host-navigation'
      }
    }
  },
  StatStrip: {
    description:
      'One to five recorded numbers with a sparkline each, from built-in reads of bifrost.traces.spans, ' +
      'bifrost.logs.records and bifrost.gateway.calls, optionally scoped to one workload_card_uid or subject_card_uid.',
    input: StatStripInputSchema,
    states: ['populated', 'empty', 'denied', 'error', 'loading'],
    emits: {}
  },
  FacetRail: {
    description: "Value counts of a signal's columns under the list's own WHERE and range; a value asks the host to filter by it.",
    input: FacetRailInputSchema,
    states: ['populated', 'empty', 'denied', 'error', 'loading'],
    emits: {
      'facet.filter': {
        description: 'Add this column and value to the WHERE clause; a null value means unset.',
        input: filterIntent,
        completion: 'host-navigation'
      }
    }
  },
  RecordTable: {
    description:
      'Typed rows of one signal (traces as root spans with span and error counts, spans, logs or metric points) ' +
      'or of authored SQL, newest first, with optional grouping and live mode that pauses while inspected.',
    input: RecordTableInputSchema,
    states: ['populated', 'empty', 'denied', 'error', 'loading', 'live'],
    emits: {
      'record.open': {
        description: 'Open one trace or span, or one log record by its exact identity.',
        input: z.union([spanRef, z.object({ log: LogKeySchema }).strict()]),
        completion: 'host-navigation'
      },
      'record.open_in_query': {
        description: "Open the list's exact executed SQL and its typed binds in Query.",
        input: executedQuery,
        completion: 'host-navigation'
      }
    }
  },
  TraceWaterfall: {
    description: 'Every retained span of one exact trace as a timed tree, collapsed below two levels; a span asks the host to select it.',
    input: TraceWaterfallInputSchema,
    states: ['populated', 'empty', 'denied', 'error', 'loading'],
    emits: {
      'trace.select': { description: 'Select this span of the trace.', input: spanRef.required(), completion: 'host-navigation' }
    }
  },
  RecordDetail: {
    description: 'One exact span or log record: messages, attributes, events, exception and the raw row.',
    input: RecordDetailInputSchema,
    states: ['populated', 'empty', 'denied', 'error', 'loading'],
    emits: {
      'record.filter': { description: 'Filter the list by this field, or with not, exclude it.', input: filterIntent, completion: 'host-navigation' },
      'record.logs': { description: 'Show the log records of this span.', input: spanRef.required(), completion: 'host-navigation' },
      'record.open': { description: "Open this log record's trace and span.", input: spanRef, completion: 'host-navigation' }
    }
  }
} as const satisfies Record<string, ComponentSpec>;

/** The name of a catalog component. */
export type ComponentName = keyof typeof components;

/** A validated chart definition. */
export type ChartInput = z.infer<typeof ChartInputSchema>;
/** A validated `CardHeader` input. */
export type CardHeaderInput = z.infer<typeof CardHeaderInputSchema>;
/** A view or panel range. */
export type Range = z.infer<typeof RangeSchema>;
/** One WHERE clause. */
export type Filter = z.infer<typeof FilterSchema>;
/** One log record's exact identity. */
export type LogKey = z.infer<typeof LogKeySchema>;
/** A validated `StatStrip` input. */
export type StatStripInput = z.infer<typeof StatStripInputSchema>;
/** A validated `FacetRail` input. */
export type FacetRailInput = z.infer<typeof FacetRailInputSchema>;
/** A validated `RecordTable` input. */
export type RecordTableInput = z.infer<typeof RecordTableInputSchema>;
/** A validated `TraceWaterfall` input. */
export type TraceWaterfallInput = z.infer<typeof TraceWaterfallInputSchema>;
/** A validated `RecordDetail` input. */
export type RecordDetailInput = z.infer<typeof RecordDetailInputSchema>;
/** A built-in stat. */
export type Stat = (typeof STATS)[number];
/** An absolute range with RFC 3339 bounds. */
export type AbsoluteRange = z.infer<typeof AbsoluteRangeSchema>;

/**
 * A panel's place on the view's 12-column grid of 40px rows, in whole units.
 * Components never see it: they fill whatever box the view gives them.
 */
export const LayoutSchema = z
  .object({
    x: z.number().int().min(0).max(11),
    y: z.number().int().min(0),
    w: z.number().int().min(1).max(12),
    h: z.number().int().min(1).max(48)
  })
  .strict()
  .refine((layout) => layout.x + layout.w <= 12, { message: 'x + w must not exceed 12 columns', path: ['w'] });

/** A grid position and size; see `LayoutSchema`. */
export type Layout = z.infer<typeof LayoutSchema>;

/** One placed panel; its `input` is validated against its component's schema at load. */
export const PanelSchema = z
  .object({ id: z.string().min(1), component: z.string().min(1), input: z.unknown(), layout: LayoutSchema.optional() })
  .strict();

/** The catalog every view document is validated against. */
export const CATALOG_ID = 'wyrd.ui/v1';
/** The view-document format version. */
export const VIEW_VERSION = 'wyrd.view/v1';

/**
 * A view document: catalog panels under one range, resolved once per load so
 * every panel reads the same bounds. A panel that fails validation fails alone.
 */
export const ViewDocumentSchema = z
  .object({
    version: z.literal(VIEW_VERSION),
    catalogId: z.literal(CATALOG_ID),
    title: z.string().min(1).optional(),
    range: RangeSchema,
    panels: z
      .array(PanelSchema)
      .min(1)
      .max(24)
      .refine((panels) => new Set(panels.map((panel) => panel.id)).size === panels.length, 'panel ids must be unique')
  })
  .strict();

/** A document-valid view; panel inputs are still unvalidated. */
export type ViewDocument = z.infer<typeof ViewDocumentSchema>;
