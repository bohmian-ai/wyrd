/**
 * Safe BFF projections for the Observe workspace.
 *
 * The server projects stored correlation and results; the browser never
 * fabricates joins or health. Domain protocol authority stays in wyrd-spec —
 * these are presentation shapes only.
 */

/** Shared status tone vocabulary; every status also carries text and a glyph. */
export type Tone = 'neutral' | 'ok' | 'warn' | 'danger' | 'running';

/** One point-in-time series for the shared Line chart. */
export type Series = { label: string; points: number[] };

/** Axis tick with a machine-readable timestamp for `<time>` semantics. */
export type Stamp = { label: string; at: string };

/** One compact per-signal summary tile on the Observe overview. */
export type SignalTile = {
  signal: string;
  href: string;
  value: string;
  unit: string;
  status: { label: string; tone: Tone };
  /** True when the signal has no data in range — absence, not health. */
  noData?: boolean;
};

/** Tenant-wide attention row: what needs a person, across every signal. */
export type AttentionRow = {
  signal: string;
  what: string;
  service: string;
  state: { label: string; tone: Tone };
  since: string;
  href: string;
};

/** O-01 — compact overview: tiles, tenant-wide attention and recent activity. */
export type ObserveOverview = {
  tiles: SignalTile[];
  attention: AttentionRow[];
  recent: { time: string; signal: string; event: string; service: string; href: string }[];
  /** Explains the no-data signal in its own rail panel, when one exists. */
  noDataNote: { signal: string; detail: string } | null;
};

/**
 * One OTel log record, shaped by the Bifrost `logs.records` table: severity
 * pair, event name, opaque `body` payload (string or structured JSON), the
 * `attributes` map, trace correlation, and resource/scope identity. `level`
 * mirrors `severityText` lowercased for filters and badges; `message` is the
 * one-line display form of `body` for the results table.
 */
export type LogRecord = {
  id: string;
  time: string;
  /** OTel `time` — when the event occurred, ISO. */
  at: string;
  /** OTel `observed_time` — when the collector saw it, ISO. */
  observedAt: string;
  level: string;
  /** OTel severity_number, 1–24 (9 INFO, 13 WARN, 17 ERROR). */
  severityNumber: number;
  severityText: string;
  /** OTel event_name identifying the event class, when the emitter sets one. */
  eventName: string | null;
  /** Resource `service.name`. */
  service: string;
  message: string;
  /** OTel body — string or structured JSON, rendered verbatim in the drawer. */
  body: unknown;
  traceId: string | null;
  spanId: string | null;
  /** W3C trace flags; bit 0 = sampled. */
  traceFlags: number;
  scopeName: string;
  scopeVersion: string;
  attributes: Record<string, string | number | boolean>;
  droppedAttributesCount: number;
};

/** One facet value with its match count in the current search context. */
export type FacetValue = { value: string; count: number; active: boolean };

/** One filterable field in the fields side panel, with its top values. */
export type FieldFacet = { name: string; values: FacetValue[] };

/** O-02 — guided log search: trend, records, one selected structured record. */
/**
 * Truthful result-set size for a search. `exact: false` renders as `≥ count`
 * while a partitioned backend is still counting — the UI never invents totals.
 */
export type Matching = { count: number; exact: boolean };

/** What one search cost — surfaced on every result set, OLAP-style. */
export type SearchCost = { scanned: string; tookMs: number };

export type LogsView = {
  matching: Matching;
  cost: SearchCost;
  /** 1-based page of `perPage` rows currently loaded; `pageCount` is derived from `matching`. */
  page: number;
  perPage: number;
  pageCount: number;
  trend: number[];
  /** Epoch ms the trend window ends at — charts derive bucket clock labels from it. */
  trendEnd: number;
  /** Field facets for the side panel — server-projected values and counts. */
  fields: FieldFacet[];
  records: LogRecord[];
  selected: LogRecord | null;
  /** The WHERE clause the current filters express — shown, never authored here. */
  queryWhere: string;
  /** Equivalent context handed to Query as one SELECT — no SQL authoring here. */
  querySql: string;
};

/** One discoverable measure with its label vocabulary. */
export type MetricInfo = {
  name: string;
  unit: string;
  description: string;
  labels: [string, string][];
};

/** O-03 — metric discovery, chart and the underlying values. */
export type MetricsView = {
  /** Distinct service facet for the filter bar. */
  services: string[];
  metrics: MetricInfo[];
  selected: MetricInfo | null;
  series: Series[];
  labels: Stamp[];
  values: { columns: string[]; rows: string[][] };
  /** Card behind the scoped service, when the scope names one. */
  cardHref: string | null;
};

/** One trace search result row. */
export type TraceRow = {
  id: string;
  /** ISO instant of the trace root, so range filters derive from real time. */
  at: string;
  rootOperation: string;
  service: string;
  start: string;
  durationMs: number;
  spans: number;
  errorSpans: number;
  status: { label: string; tone: Tone };
};

/** O-04 — trace search: RED trend strip plus result rows; a row opens the trace directly. */
export type TracesView = {
  matching: Matching;
  cost: SearchCost;
  /** Rows currently loaded (load-more appends in steps of 50). */
  limit: number;
  /** True when more matching traces exist beyond `rows`. */
  hasMore: boolean;
  errorRate: string;
  p95: string;
  /** Error traces per minute — the Errors panel of the RED strip. */
  trend: number[];
  /** All traces per minute — the Rate panel. */
  rateTrend: number[];
  /** p95 duration in ms per bucket — the Duration panel. */
  durationTrend: number[];
  /** Epoch ms the trend window ends at — charts derive bucket clock labels from it. */
  trendEnd: number;
  /** Field facets for the side panel — server-projected values and counts. */
  fields: FieldFacet[];
  /** The WHERE clause the current filters express. */
  queryWhere: string;
  rows: TraceRow[];
};

/** One `traces.events` row on a span: name, offset from span start, attributes. */
export type SpanEvent = {
  name: string;
  /** Offset from the span start, e.g. `+0.44s`. */
  offset: string;
  attributes: Record<string, string | number | boolean>;
  droppedAttributesCount: number;
};

/** One `traces.links` row on a span — navigates to the linked trace/span. */
export type SpanLink = {
  linkedTraceId: string;
  linkedSpanId: string;
  traceState: string | null;
  attributes: Record<string, string | number | boolean>;
};

/** One chat message extracted into `genai.messages` payload columns. */
export type GenAiMessage = { role: string; content: string };

/**
 * The extracted GenAI record joined to a span, when one exists. Bifrost
 * extracts GenAI spans into `genai.messages` / `genai.tool_calls`; a span
 * either has such a record or it simply is not a GenAI span — absence is
 * normal, never an error state.
 */
export type GenAiSpan =
  | {
      table: 'messages';
      provider: string;
      operation: string;
      requestModel: string;
      responseModel: string | null;
      conversationId: string | null;
      /** Request parameters actually set, e.g. temperature, max_tokens. */
      params: [string, string][];
      usage: { input: number; output: number; cacheRead: number; cacheCreate: number };
      finishReasons: string[];
      /** Sensitive payload columns — null means withheld, see `withheldReason`. */
      systemInstructions: string | null;
      inputMessages: GenAiMessage[] | null;
      outputMessages: GenAiMessage[] | null;
      withheldReason: string | null;
      errorType: string | null;
    }
  | {
      table: 'tool_calls';
      provider: string;
      operation: string;
      toolName: string;
      toolType: string;
      conversationId: string | null;
      /** Sensitive payload columns — null means withheld or absent. */
      args: string | null;
      result: string | null;
      withheldReason: string | null;
      errorType: string | null;
    };

/**
 * One span in the trace waterfall, shaped by the Bifrost `traces.spans`
 * contract: status, scope identity, the attributes map, and the per-span
 * `traces.events` / `traces.links` collections. `genai` is the extracted
 * `genai.*` record when this span is a GenAI span.
 */
export type Span = {
  id: string;
  name: string;
  kind: string;
  /** Owning service — drives the waterfall's per-service colour. */
  service: string;
  /** Nesting depth under the root span — drives the hierarchy connectors. */
  depth: number;
  startPct: number;
  widthPct: number;
  durationLabel: string;
  error: boolean;
  /** OTel span status code: OK, ERROR or UNSET. */
  status: string;
  /** Parent span id, null on the root — assigned from waterfall order. */
  parentSpanId: string | null;
  scopeName: string;
  scopeVersion: string;
  attributes: Record<string, string | number | boolean>;
  droppedAttributesCount: number;
  events: SpanEvent[];
  links: SpanLink[];
  genai: GenAiSpan | null;
};

/** O-05 — one trace: waterfall, service graph and the selected span's detail. */
export type TraceDetail = {
  id: string;
  rootOperation: string;
  service: string;
  start: string;
  duration: string;
  spanCount: number;
  errorCount: number;
  spans: Span[];
  selected: Span;
  graph: {
    nodes: { name: string; cardHref: string }[];
    edges: { from: string; to: string; error: boolean; label: string }[];
  };
};

/** One GenAI call row — a `genai.messages` or `genai.tool_calls` record. */
export type GenAiRow = {
  /** Span id of the extracted record. */
  id: string;
  /** ISO instant of the call start, so range filters derive from real time. */
  at: string;
  start: string;
  table: 'messages' | 'tool_calls';
  service: string;
  provider: string;
  operation: string;
  /** request_model for messages rows; tool_name for tool_calls rows. */
  model: string;
  conversationId: string | null;
  tokensIn: number | null;
  tokensOut: number | null;
  durationMs: number;
  status: { label: string; tone: Tone };
  traceId: string;
};

/** O-09 — GenAI call search over the extracted `genai.*` records. */
export type GenAiView = {
  matching: Matching;
  cost: SearchCost;
  /** Rows currently loaded (load-more appends in steps of 50). */
  limit: number;
  hasMore: boolean;
  /** Field facets for the side panel — server-projected values and counts. */
  fields: FieldFacet[];
  /** The WHERE clause the current filters express. */
  queryWhere: string;
  rows: GenAiRow[];
};

/** One dashboard inventory row. */
export type DashboardRow = {
  id: string;
  name: string;
  folder: string;
  tags: string[];
  owner: string;
  updated: string;
};

/** O-06 — read-only dashboard inventory; rows link straight to detail. */
export type DashboardsView = {
  rows: DashboardRow[];
};

/** One read-only dashboard panel; per-panel states render in place, at size. */
export type DashboardPanel = {
  title: string;
  meta: string;
  kind: 'line' | 'bars' | 'table';
  state: 'ok' | 'empty' | 'error';
  series?: Series[];
  labels?: Stamp[];
  threshold?: { value: number; label: string };
  bars?: { label: string; value: number }[];
  table?: { columns: string[]; rows: string[][] };
  note?: string;
  code?: string;
};

/** O-07 — one read-only dashboard: variables, range and the panel grid. */
export type DashboardDetail = {
  id: string;
  name: string;
  owner: string;
  updated: string;
  variables: { name: string; value: string; options: string[] }[];
  panels: DashboardPanel[];
};

/** One evaluation event row, keyed by record_id; run_id is correlation. */
export type EvalRow = {
  recordId: string;
  eval: string;
  subject: string;
  runId: string;
  status: { label: string; tone: Tone };
  passed: number;
  total: number;
  duration: string;
  /** `online` production emission or `offline` local scenario replay. */
  origin: 'online' | 'offline';
  /** Scenario identity for offline records; null for online emission. */
  scenario: { id: string; collection: string } | null;
};

/** O-08 — evaluation event inventory. */
export type EvalsView = {
  matching: number;
  /** Distinct subject facet for the filter bar. */
  subjects: string[];
  rows: EvalRow[];
};

/** One task result inside an evaluation workflow. */
export type EvalTask = {
  name: string;
  method: string;
  stage: string;
  status: { label: string; tone: Tone };
  score: string;
  detail: {
    type: string;
    weight: string;
    started: string;
    duration: string;
    attempt: string;
    cost: string;
    scoreValue: number | null;
    threshold: number | null;
    comparison: string;
    judgeExplanation: string | null;
    expected: string[];
    /** Authorized actual values; null when withheld for this principal. */
    actual: string[] | null;
    withheldReason: string | null;
    history: ('pass' | 'fail')[];
    traceLink: { label: string; href: string } | null;
  };
};

/** O-09 — one evaluation event with its workflow drawer. */
export type EvalDetail = {
  recordId: string;
  runId: string;
  eval: string;
  evalCardHref: string;
  subject: string;
  subjectHref: string;
  workflow: { name: string; version: string; started: string };
  status: { label: string; tone: Tone };
  duration: string;
  passed: number;
  total: number;
  origin: 'online' | 'offline';
  scenario: { id: string; collection: string; initialQuery: string } | null;
  stages: { name: string; state: string; tone: Tone }[];
  tasks: EvalTask[];
  selected: EvalTask;
};

/** One per-feature drift result inside a calculated report. */
export type DriftFeature = {
  name: string;
  method: string;
  score: number;
  threshold: number;
  verdict: { label: string; tone: Tone };
};

/** O-10 — calculated drift reports against a declared Drift Card. */
export type DriftView = {
  card: { id: string; name: string; version: string; href: string };
  definition: { method: string; baseline: string; window: string; condition: string };
  /** Null when no report exists for the range — absence, never a verdict. */
  report: {
    id: string;
    calculated: string;
    verdict: { label: string; tone: Tone };
    summary: string;
    features: DriftFeature[];
    selectedFeature: DriftFeature;
    history: { points: number[]; labels: Stamp[]; threshold: { value: number; label: string } };
    alert: {
      finding: string;
      breached: string;
      reaction: string;
      operatorHref: string;
    } | null;
  } | null;
};
