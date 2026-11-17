/**
 * The catalog components' input and action schemas, declared once.
 *
 * `scripts/catalog.ts` emits these as `catalog.json` with `z.toJSONSchema`;
 * view validation, the BFF load endpoint, and hosts parse with the same
 * values, so only these schemas define what a document or intent may carry.
 */
import { z } from 'zod';
import type { CardKind, CardRef, CardSummary } from 'wyrd-ai';

/**
 * The twelve registrable Card kinds of the architecture kind catalog, in its
 * order. The generated union still names Policy and Source, which the catalog
 * removed; every other generated kind must appear here.
 */
export const CARD_KINDS = [
  'Data',
  'Model',
  'Artifact',
  'Experiment',
  'Prompt',
  'Agent',
  'Mcp',
  'Service',
  'Audit',
  'Verifier',
  'Trigger',
  'Operator',
  'Repository'
] as const satisfies readonly CardKind[];
const everyKind: Exclude<CardKind, 'Policy' | 'Source'> extends (typeof CARD_KINDS)[number] ? true : never = true;
void everyKind;

/** Lifecycle statuses an inventory may filter by; a deleted Card is never listed. */
export const CARD_STATUSES = ['active', 'pending', 'deprecated', 'failed', 'expired'] as const satisfies readonly CardSummary['status'][];

/** An exact version such as 1.4.2, never a range or "latest". */
const exactVersion = z.string().regex(/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/, 'an exact version such as 1.4.2');

/** An exact Card reference: a full version pin, never a range or "latest". */
export const CardRefSchema = z
  .object({
    kind: z.enum(CARD_KINDS),
    name: z.string().min(1),
    version: exactVersion,
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

/** The pages an inventory follows through `cards.list` cursors at most. */
export const INVENTORY_PAGES = 20;

/** Inventory filters; each maps onto the `cards.list` request field of the same meaning. */
const inventoryFilters = {
  /**
   * One word with no spaces, quotes or operators matches names containing it,
   * ignoring case; anything else is the metadata query itself (labels.team = "search").
   */
  query: z.string().max(4096).optional(),
  kind: z.enum(CARD_KINDS).optional(),
  status: z.enum(CARD_STATUSES).optional(),
  include_prerelease: z.boolean().optional()
};

/** The input of `CardInventory`: filters and how many server pages to show. */
export const CardInventoryInputSchema = z
  .object({ ...inventoryFilters, pages: z.number().int().min(1).max(INVENTORY_PAGES).optional() })
  .strict();

/** The input of an exact-Card detail component. */
export const CardDetailInputSchema = z.object({ cardRef: CardRefSchema }).strict();

/** The input of `CardFacts`: one exact Card; `yaml` shows the whole returned Card as YAML instead. */
export const CardFactsInputSchema = z.object({ cardRef: CardRefSchema, yaml: z.boolean().optional() }).strict();

/** One exact Prompt Card. */
const promptRef = CardRefSchema.extend({ kind: z.literal('Prompt') });

/** The input of a Prompt component. */
export const PromptInputSchema = z.object({ cardRef: promptRef }).strict();

/** The input of a Verifier component: one exact Verifier Card. */
export const VerifierInputSchema = z.object({ cardRef: CardRefSchema.extend({ kind: z.literal('Verifier') }) }).strict();

/** The Show choices of the Repositories index: every row, ready ones, registered ones that start nothing, and repositories with no Card. */
export const REPOSITORY_SHOWS = ['all', 'ready', 'inactive', 'undeclared'] as const;
const repositoryFilters = { query: z.string().max(200).optional(), show: z.enum(REPOSITORY_SHOWS).optional() };

/** The input of `RepositoryIndex`: the search text and the Show choice. */
export const RepositoryIndexInputSchema = z.object(repositoryFilters).strict();

/** The input of `RepositoryPolicy`: one exact Repository Card and which declaration text to show. */
export const RepositoryPolicyInputSchema = z
  .object({ cardRef: CardRefSchema.extend({ kind: z.literal('Repository') }), declaration: z.enum(['yaml', 'toml']).optional() })
  .strict();

/** The input of `RepositoryDeclare`: the provider's stable id of one reachable repository. */
export const RepositoryDeclareInputSchema = z.object({ repositoryId: z.string().min(1).max(64) }).strict();

/** The input of `GithubIntegration`: the tenant has one connection, so nothing. */
export const GithubIntegrationInputSchema = z.object({}).strict();

/** One invocation attempt of a step: its fan-out item and repeat iteration, each null when the step has none, and the one-based attempt. */
export const ProducerSchema = z
  .object({ item_index: z.number().int().min(0).nullish(), iteration_index: z.number().int().min(0).nullish(), attempt: z.number().int().min(1) })
  .strict();

/** What an execution view shows beyond the execution itself; every field is shareable selection state. */
const executionSelection = {
  /** The selected graph node: `input:`, `step:` or `req:` plus its id; `outputs` for every retained output; `none` for no selection. Absent, a judged execution selects its first failed requirement. */
  node: z.string().min(1).max(300).optional(),
  /** The selected invocation attempt of the selected step. */
  producer: ProducerSchema.optional(),
  /** The publication of that attempt to read, by its name. */
  output: z.string().min(1).max(300).optional(),
  /** Read the completed records (steps, checks, provenance) as well. */
  details: z.boolean().optional()
};

/** The input of `VerificationExecution`: one exact execution, its selection, and the page cursors to continue from. */
export const ExecutionInputSchema = z
  .object({
    executionId: z.string().min(1).max(200),
    ...executionSelection,
    invocations: z.string().min(1).max(4096).optional(),
    publications: z.string().min(1).max(4096).optional()
  })
  .strict();

/** The input of `VersionDiff`: one exact Prompt and another registered version; absent, the version before it. */
export const VersionDiffInputSchema = z
  .object({ cardRef: promptRef, otherVersion: exactVersion.optional() })
  .strict()
  .refine((input) => input.otherVersion !== input.cardRef.version, { message: 'must differ from cardRef.version', path: ['otherVersion'] });

/** Open one exact Card version. */
const openCard = {
  description: 'Open this exact Card version.',
  input: z.object({ card_ref: CardRefSchema }).strict(),
  completion: 'host-navigation'
} as const;

/** Open the tenant's GitHub connection settings. */
const openConnection = { description: "Open the tenant's GitHub connection settings.", input: z.object({}).strict(), completion: 'host-navigation' } as const;

/** Show how to declare a reachable repository that has no Card. */
const declareRepository = {
  description: 'Show how to declare a Repository Card for this reachable GitHub repository.',
  input: z.object({ repository_id: z.string().min(1).max(64) }).strict(),
  completion: 'host-navigation'
} as const;

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
 * One row per accepted Verifier execution of every origin, as retained: its
 * input record, the result its latest checkpoint selected (a direct
 * execution's only one), and its status. A queued execution's `status` is its
 * newest retained status row (`running`, `retrying`, `waiting`, `pending`,
 * `completed`, `cancelled`, `timed_out` or `errored`) and `pending` before
 * any; a status row lost with its process leaves the older one standing. A
 * direct execution is never queued: `completed` once its result is visible,
 * else `not recorded`.
 */
// Every read of this population binds its range first, so `$1` here is the range start. An execution's
// states and result are recorded after its input, so the joins read from that start on, with an hour of
// slack for clocks, and never the tenant's whole history.
// ponytail: states and results stay unbounded above, since work accepted in the range may end any time later.
const SINCE = "$1::timestamptz - INTERVAL '1 hour'";
const EXECUTIONS =
  '(SELECT i.execution_id, i.recorded_at, i.verifier_card_uid, i.binding_id, i.group_id, i.origin, i.run_id, ' +
  'i.subject_kind, i.subject_name, i.subject_version, i.subject_card_uid, i.workload_card_uid, i.service_name, i.service_version, i.deployment_environment, ' +
  'r.result_id, r.verdict, r.ended_at, ' +
  "CASE WHEN i.origin = 'direct' THEN CASE WHEN r.result_id IS NOT NULL THEN 'completed' ELSE 'not recorded' END ELSE coalesce(q.status, 'pending') END AS status " +
  'FROM bifrost.verification.execution_inputs i ' +
  // Rows of one instant have no recorded order; the event id only makes the choice repeatable.
  "LEFT JOIN (SELECT execution_id, last_value(status ORDER BY occurred_at, event_id) AS status FROM bifrost.verification.execution_events WHERE event = 'status' " +
  `AND occurred_at >= ${SINCE} GROUP BY execution_id) q ON q.execution_id = i.execution_id ` +
  "LEFT JOIN (SELECT e.execution_id, e.value -> 'terminal' ->> 'result_id' AS result_id " +
  'FROM bifrost.verification.execution_events e ' +
  `JOIN (SELECT execution_id, max(sequence) AS sequence FROM bifrost.verification.execution_events WHERE event = 'checkpoint' AND occurred_at >= ${SINCE} GROUP BY execution_id) l ` +
  `ON l.execution_id = e.execution_id AND l.sequence = e.sequence WHERE e.event = 'checkpoint' AND e.occurred_at >= ${SINCE}) s ON s.execution_id = i.execution_id ` +
  `LEFT JOIN bifrost.verification.results r ON r.execution_id = i.execution_id AND r.ended_at >= ${SINCE} AND (i.origin = 'direct' OR r.result_id = s.result_id)) AS executions`;

/**
 * The Observe signals and their canonical tables, and Verify's `executions`:
 * one row per accepted Verifier execution, read from {@link EXECUTIONS}. `traces` lists root spans,
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
  },
  executions: {
    table: EXECUTIONS,
    time: 'recorded_at',
    columns: ['recorded_at', 'verifier_card_uid', 'subject_name', 'run_id', 'group_id', 'service_name', 'status', 'verdict'],
    filters: [
      'status', 'verdict', 'verifier_card_uid', 'service_name', 'subject_name', 'run_id', 'group_id', 'origin', 'binding_id', 'subject_kind', 'subject_version',
      'subject_card_uid', 'workload_card_uid', 'service_version', 'deployment_environment', 'execution_id', 'result_id'
    ],
    more: ['ended_at']
  }
} as const;

/** An Observe signal. */
export type Signal = keyof typeof SIGNALS;
const SignalSchema = z.enum(['traces', 'spans', 'logs', 'metrics', 'executions']);

/** Columns a list of `signal` may show: its defaults, every filter column, then its other columns. */
export function signalColumns(signal: Signal): readonly string[] {
  return [...new Set([...SIGNALS[signal].columns, ...SIGNALS[signal].filters, ...SIGNALS[signal].more])];
}

/** Columns that order a list's rows sharing the sort value and event time: each row's identity. A metric point list has none and is not sorted or paged. */
export const TIES: Record<Signal, readonly string[]> = {
  traces: ['trace_id', 'span_id'],
  spans: ['trace_id', 'span_id'],
  logs: ['observed_time_unix_nano', 'wyrd_request_id'],
  metrics: [],
  executions: ['execution_id']
};

/** Columns a list of `signal` sorts by on the server: its default and filter columns but its ids. */
export function sortColumns(signal: Signal): readonly string[] {
  if (!TIES[signal].length) return [];
  return [...new Set<string>([...SIGNALS[signal].columns, ...SIGNALS[signal].filters])].filter((column) => !(IDS as readonly string[]).includes(column));
}

/** A list's server order: one sortable column and its direction; absent, event time newest first. */
export const ListSortSchema = z.object({ column: z.string().min(1), desc: z.boolean() }).strict();
export type ListSort = z.infer<typeof ListSortSchema>;

/** The list's applied order: the sort, else event time newest first. */
export function listSort(signal: Signal, sort?: ListSort): ListSort {
  return sort ?? { column: SIGNALS[signal].time, desc: true };
}

/** The columns a page cursor holds, in order: the sort column, event time, then the ties. */
export function cursorColumns(signal: Signal, sort?: ListSort): readonly string[] {
  const { column } = listSort(signal, sort);
  const time = SIGNALS[signal].time;
  return [...(column === time ? [] : [column]), time, ...TIES[signal]];
}

/** One page cursor: the cursor columns' values of the row a page starts after or ends before. */
const cursor = z.array(queryParam).min(1).max(8);

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

/** Refuse a predicate over `executions`: its rows are derived, so no table validates one. */
function noPredicate(input: { readonly signal: Signal; readonly where_sql?: string }, context: z.RefinementCtx) {
  if (input.signal === 'executions' && input.where_sql?.trim())
    context.addIssue({ code: 'custom', path: ['where_sql'], message: 'executions filter by where clauses only' });
}

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
    noPredicate(input, context);
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
      sort: ListSortSchema.optional(),
      after: cursor.optional(),
      before: cursor.optional(),
      ...listFields
    })
    .strict()
    .superRefine((input, context) => {
      noPredicate(input, context);
      const columns = signalColumns(input.signal);
      if (input.sort) knownColumns(input.signal, [input.sort.column], sortColumns(input.signal), ['sort', 'column'], context);
      if (input.after && input.before) context.addIssue({ code: 'custom', path: ['before'], message: 'a page reads after or before one row, not both' });
      const page = input.after ? 'after' : 'before';
      const keys = input.after ?? input.before;
      if (keys && !TIES[input.signal].length) context.addIssue({ code: 'custom', path: [page], message: `a ${input.signal} list is not paged` });
      else if (keys && keys.length !== cursorColumns(input.signal, input.sort).length)
        context.addIssue({ code: 'custom', path: [page], message: `a cursor holds ${cursorColumns(input.signal, input.sort).join(', ')}` });
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
type Completion = 'host-navigation' | 'view-range' | 'host-action';

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
  CardInventory: {
    description:
      'Every registered Card version cards.list returns, in registration order: kind, name, version, status, uid and update time, ' +
      'filtered by kind, lifecycle status, prerelease and a name or metadata query (labels., annotations.), paged by server cursor.',
    input: CardInventoryInputSchema,
    states: ['populated', 'empty', 'denied', 'error', 'loading'],
    emits: {
      'cards.filter': {
        description: 'Read the inventory again under these filters, from its first page.',
        input: z.object(inventoryFilters).strict(),
        completion: 'host-navigation'
      },
      'cards.more': {
        description: "Show one more page, following the last page's server cursor.",
        input: z.object({ pages: z.number().int().min(2).max(INVENTORY_PAGES) }).strict(),
        completion: 'host-navigation'
      },
      'card.open': openCard
    }
  },
  CardFacts: {
    description: "An exact Card's server-managed facts (uid, status, hashes, labels, annotations, origin), or the whole returned Card as YAML.",
    input: CardFactsInputSchema,
    states: ['populated', 'denied', 'error', 'loading'],
    emits: {}
  },
  RelationshipList: {
    description: "An exact Card's server-derived relationships: each exact outbound and inbound Card reference.",
    input: CardDetailInputSchema,
    states: ['populated', 'empty', 'denied', 'error', 'loading'],
    emits: { 'card.open': openCard }
  },
  ArtifactTable: {
    description: 'The files registration recorded for one exact Card version: relative path, size, SHA-256 digest and content type.',
    input: CardDetailInputSchema,
    states: ['populated', 'empty', 'denied', 'error', 'loading'],
    emits: {}
  },
  PromptMessages: {
    description:
      "An exact Prompt as authored: provider, model and response format, then the portable system text and messages, or the native format's body unchanged.",
    input: PromptInputSchema,
    states: ['populated', 'denied', 'error', 'loading'],
    emits: {}
  },
  PromptVariables: {
    description: "The text and media variables the server's Prompt compiler derived from an exact Prompt (status.prompt).",
    input: PromptInputSchema,
    states: ['populated', 'empty', 'denied', 'error', 'loading'],
    emits: {}
  },
  UsedBy: {
    description: "The Agents whose spec references an exact Prompt, from its server-derived inbound relationships, with each Agent's execution route.",
    input: PromptInputSchema,
    states: ['populated', 'empty', 'denied', 'error', 'loading'],
    emits: { 'card.open': openCard }
  },
  VersionDiff: {
    description:
      'The authored differences between two registered versions of one Prompt: provider, model, system text, messages, settings, response format and derived variables.',
    input: VersionDiffInputSchema,
    states: ['populated', 'empty', 'denied', 'error', 'loading'],
    emits: {
      'prompt.compare': {
        description: 'Compare the Prompt with this other registered version.',
        input: z.object({ other_version: exactVersion }).strict(),
        completion: 'host-navigation'
      }
    }
  },
  VerifierDefinition: {
    description:
      'An exact Verifier as declared: its description, the read-only graph of its inputs, steps, attached and whole-Verifier requirements with their declared dependencies, and the authored declaration of the selected node. No execution state.',
    input: VerifierInputSchema,
    states: ['populated', 'denied', 'error', 'loading'],
    emits: {}
  },
  RunsOn: {
    description:
      "The Services and Agents whose bindings pin an exact Verifier version, from its server-derived inbound relationships, with each binding's subject and when it runs.",
    input: VerifierInputSchema,
    states: ['populated', 'empty', 'denied', 'error', 'loading'],
    emits: { 'card.open': openCard }
  },
  RepositoryIndex: {
    description:
      "Repository Cards joined to the GitHub repositories the tenant's connection reaches, by provider repository id: registration, GitHub access and automatic verification as separate facts, with search and a Show choice.",
    input: RepositoryIndexInputSchema,
    states: ['populated', 'denied', 'error', 'loading'],
    emits: {
      'repository.filter': { description: 'Replace the search text and Show choice.', input: z.object(repositoryFilters).strict(), completion: 'host-navigation' },
      'card.open': openCard,
      'repository.declare': declareRepository,
      'connection.open': openConnection
    }
  },
  RepositoryPolicy: {
    description:
      "One exact Repository Card version: whether it is the selected policy, GitHub access, automatic verification, its exact Verifier bindings and its read-only declaration as Card YAML or a wyrd.toml projection.",
    input: RepositoryPolicyInputSchema,
    states: ['populated', 'denied', 'error', 'loading'],
    emits: {
      'card.version': { description: 'View another exact version of this Repository; selection does not change.', input: z.object({ card_ref: CardRefSchema }).strict(), completion: 'host-navigation' },
      'card.open': openCard,
      'repository.declaration': { description: 'Show the declaration as Card YAML or as a wyrd.toml projection.', input: z.object({ declaration: z.enum(['yaml', 'toml']) }).strict(), completion: 'host-navigation' },
      'connection.open': openConnection
    }
  },
  RepositoryDeclare: {
    description: 'One reachable GitHub repository with no Repository Card: its three separate facts and the wyrd.toml and command that declare it.',
    input: RepositoryDeclareInputSchema,
    states: ['populated', 'denied', 'error', 'loading'],
    emits: { 'connection.open': openConnection }
  },
  GithubIntegration: {
    description:
      "The tenant's one GitHub App connection: state, installation, App and host, the repositories it reaches beside their Repository Cards, connect, manage access on GitHub, and a confirmed disconnect.",
    input: GithubIntegrationInputSchema,
    states: ['populated', 'denied', 'error', 'loading'],
    emits: {
      'github.connect': { description: 'Start connecting this tenant to GitHub; the host continues on GitHub.', input: z.object({}).strict(), completion: 'host-action' },
      'github.disconnect': { description: 'Disconnect this tenant from GitHub, after the reader confirmed.', input: z.object({}).strict(), completion: 'host-action' },
      'card.open': openCard,
      'repository.declare': declareRepository
    }
  },
  VerificationExecution: {
    description:
      "One exact verification execution: its lifecycle status and verdict, the graph of the Verifier it ran coloured by recorded state, " +
      'and an inspector for the selected step, requirement, input, session member or retained output. Every part is read and refused on its own.',
    input: ExecutionInputSchema,
    states: ['populated', 'denied', 'error', 'loading'],
    emits: {
      'execution.select': {
        description: 'Show this node, invocation attempt, output and details; absent fields clear that part of the selection.',
        input: z.object(executionSelection).strict(),
        completion: 'host-navigation'
      },
      'execution.page': {
        description: "Read the page after the one shown, following the server's opaque cursor.",
        input: z.union([z.object({ invocations: z.string().min(1) }).strict(), z.object({ publications: z.string().min(1) }).strict()]),
        completion: 'host-navigation'
      },
      'execution.open': {
        description: 'Open another exact execution, such as a session member.',
        input: z.object({ execution_id: z.string().min(1) }).strict(),
        completion: 'host-navigation'
      },
      'execution.cancel': {
        description: 'Cancel this execution through the existing cancel operation; the host reports the outcome and reads the status again.',
        input: z.object({ execution_id: z.string().min(1) }).strict(),
        completion: 'host-action'
      },
      'publication.download': {
        description: 'Download one retained file of a publication through the host.',
        input: z.object({ step_id: z.string().min(1), name: z.string().min(1), producer: ProducerSchema, path: z.string().min(1).optional() }).strict(),
        completion: 'host-navigation'
      },
      'record.open': { description: 'Open the exact trace an input names, read with the viewer\'s current permissions.', input: spanRef, completion: 'host-navigation' },
      'card.open': openCard
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
      'Typed rows of one signal (traces as root spans with span and error counts, spans, logs, metric points, or Verifier executions with their retained status and recorded verdict) ' +
      'or of authored SQL, newest first or sorted by a column, paged by keyset cursor, with optional grouping and live mode that pauses while inspected.',
    input: RecordTableInputSchema,
    states: ['populated', 'empty', 'denied', 'error', 'loading', 'live'],
    emits: {
      'record.open': {
        description: 'Open one trace or span, one log record by its exact identity, or one exact Verifier execution.',
        input: z.union([spanRef, z.object({ log: LogKeySchema }).strict(), z.object({ execution_id: z.string().min(1).max(200) }).strict()]),
        completion: 'host-navigation'
      },
      'record.open_in_query': {
        description: "Open the list's exact executed SQL and its typed binds in Query.",
        input: executedQuery,
        completion: 'host-navigation'
      },
      'record.sort': {
        description: 'Read the list again ordered by this column, from its first page.',
        input: ListSortSchema,
        completion: 'host-navigation'
      },
      'record.page': {
        description: 'Read the page after the last row shown, or before the first, in the same order.',
        input: z.union([z.object({ after: cursor }).strict(), z.object({ before: cursor }).strict()]),
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
/** A validated `CardInventory` input. */
export type CardInventoryInput = z.infer<typeof CardInventoryInputSchema>;
/** A validated exact-Card detail input. */
export type CardDetailInput = z.infer<typeof CardDetailInputSchema>;
/** A validated `CardFacts` input. */
export type CardFactsInput = z.infer<typeof CardFactsInputSchema>;
/** A validated `RepositoryIndex` input. */
export type RepositoryIndexInput = z.infer<typeof RepositoryIndexInputSchema>;
/** A validated `RepositoryPolicy` input. */
export type RepositoryPolicyInput = z.infer<typeof RepositoryPolicyInputSchema>;
/** A validated `RepositoryDeclare` input. */
export type RepositoryDeclareInput = z.infer<typeof RepositoryDeclareInputSchema>;
/** A validated `GithubIntegration` input. */
export type GithubIntegrationInput = z.infer<typeof GithubIntegrationInputSchema>;
/** A validated `VerificationExecution` input. */
export type ExecutionInput = z.infer<typeof ExecutionInputSchema>;
/** A validated `VersionDiff` input. */
export type VersionDiffInput = z.infer<typeof VersionDiffInputSchema>;
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
