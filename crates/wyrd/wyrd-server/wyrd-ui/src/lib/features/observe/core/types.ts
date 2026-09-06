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

/** One structured log record; `fields` is the ordered structured detail. */
export type LogRecord = {
  id: string;
  time: string;
  at: string;
  level: string;
  service: string;
  message: string;
  traceId: string | null;
  fields: [string, string][];
  detail: string;
};

/** O-02 — guided log search: trend, records, one selected structured record. */
export type LogsView = {
  matching: number;
  loaded: number;
  trend: number[];
  /** Distinct service facet for the filter bar — server-projected, never guessed. */
  services: string[];
  /** Distinct level facet for the filter bar. */
  levels: string[];
  records: LogRecord[];
  selected: LogRecord | null;
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
  rootOperation: string;
  service: string;
  start: string;
  durationMs: number;
  spans: number;
  errorSpans: number;
  status: { label: string; tone: Tone };
};

/** O-04 — trace search: trend plus result rows; a row opens the trace directly. */
export type TracesView = {
  matching: number;
  loaded: number;
  errorRate: string;
  p95: string;
  trend: number[];
  /** Distinct service facet for the filter bar. */
  services: string[];
  rows: TraceRow[];
};

/** One span in the trace waterfall. */
export type Span = {
  id: string;
  name: string;
  kind: string;
  startPct: number;
  widthPct: number;
  durationLabel: string;
  error: boolean;
  /** Ordered structured attributes for the selected-span rail. */
  fields: [string, string][];
  /** Optional AI content; absent sections are stated, never hidden. */
  aiContent: { prompt: string; completion: string } | null;
  aiAbsentReason: string | null;
  link: { label: string; href: string } | null;
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

/** One dashboard inventory row. */
export type DashboardRow = {
  id: string;
  name: string;
  folder: string;
  tags: string[];
  owner: string;
  updated: string;
};

/** O-06 — read-only dashboard inventory with a selected preview. */
export type DashboardsView = {
  rows: DashboardRow[];
  selected: (DashboardRow & { panels: number; variables: number; defaultRange: string }) | null;
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
