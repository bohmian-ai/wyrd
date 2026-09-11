import type {
  DashboardDetail,
  DashboardsView,
  DriftView,
  EvalDetail,
  EvalsView,
  GenAiView,
  LogsView,
  MetricsView,
  ObserveOverview,
  TraceDetail,
  TracesView
} from '$lib/features/observe/core/types';
import {
  dashboardDetails,
  dashboardRows,
  driftFeatures,
  driftHistory,
  driftHistoryLabels,
  evalDetails,
  evalRows,
  genaiRows,
  logRecords,
  metricCatalog,
  metricSeries,
  metricValues,
  MOCK_NOW,
  sketchFromRow,
  traceDetails,
  traceRows
} from './fixtures';

import { RANGE_MS } from '$lib/features/observe/core/filter-state';

/** Rows whose instant falls inside the range ending at the mock clock. */
function inRange<Row extends { at: string }>(rows: Row[], range: string): Row[] {
  const cutoff = MOCK_NOW - (RANGE_MS[range] ?? RANGE_MS['1h']);
  return rows.filter((row) => Date.parse(row.at) >= cutoff);
}

/** Buckets row counts across the range so trends derive from the population. */
function bucketCounts<Row extends { at: string }>(rows: Row[], range: string, buckets = 20): number[] {
  const width = (RANGE_MS[range] ?? RANGE_MS['1h']) / buckets;
  const start = MOCK_NOW - (RANGE_MS[range] ?? RANGE_MS['1h']);
  const counts = new Array<number>(buckets).fill(0);
  for (const row of rows) {
    const index = Math.min(buckets - 1, Math.floor((Date.parse(row.at) - start) / width));
    if (index >= 0) counts[index] += 1;
  }
  return counts;
}

/** Per-bucket p95 duration derived from the traces actually in each bucket. */
function bucketP95(rows: { at: string; durationMs: number }[], range: string, buckets = 20): number[] {
  const width = (RANGE_MS[range] ?? RANGE_MS['1h']) / buckets;
  const start = MOCK_NOW - (RANGE_MS[range] ?? RANGE_MS['1h']);
  const perBucket: number[][] = Array.from({ length: buckets }, () => []);
  for (const row of rows) {
    const index = Math.min(buckets - 1, Math.floor((Date.parse(row.at) - start) / width));
    if (index >= 0) perBucket[index].push(row.durationMs);
  }
  return perBucket.map((durations) => {
    if (!durations.length) return 0;
    durations.sort((a, b) => a - b);
    return durations[Math.min(durations.length - 1, Math.floor(durations.length * 0.95))];
  });
}

/** Compact count formatting for the overview tiles (1234 → “1.2k”). */
function fmtCount(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : `${n}`;
}

/** Accepted page sizes for the logs paginator; requests outside it clamp to the default. */
export const LOG_PAGE_SIZES = [25, 50, 100, 250];

/** Load-more step for the traces list. */
export const TRACE_LOAD_STEP = 50;

/**
 * Deterministic search cost derived from the rows the query actually scanned —
 * the mock stand-in for the `scanned_bytes`/`took_ms` every Bifrost search
 * response must carry (see the recorded Observe search contract).
 */
function searchCost(scannedRows: number): { scanned: string; tookMs: number } {
  const bytes = scannedRows * 412;
  const scanned =
    bytes >= 1_048_576 ? `${(bytes / 1_048_576).toFixed(1)} MB` : `${(bytes / 1024).toFixed(1)} KB`;
  return { scanned, tookMs: 14 + Math.round(scannedRows / 2500) };
}

/**
 * Server-side Observe projections over the development fixtures.
 *
 * All filtering happens here — the browser receives already-scoped results and
 * never fabricates joins or health. Filters echo the URL contract from
 * `$lib/features/observe/core/filter-state.ts`.
 */

/** O-01 — the compact overview across the five signals. */
export function projectOverview(
  base: string,
  scope: { service: string; range: string },
  populated: boolean
): ObserveOverview {
  const q = (extra: Record<string, string> = {}) => {
    const params = new URLSearchParams();
    if (scope.service) params.set('service', scope.service);
    params.set('range', scope.range);
    for (const [key, value] of Object.entries(extra)) params.set(key, value);
    return `?${params}`;
  };
  if (!populated)
    return {
      tiles: ['Logs', 'Metrics', 'Traces', 'Eval', 'Drift'].map((signal) => ({
        signal,
        href: `${base}/observe/${signal === 'Eval' ? 'evaluations' : signal.toLowerCase()}${q()}`,
        value: '—',
        unit: '',
        status: { label: 'No data in range', tone: 'neutral' },
        noData: true
      })),
      attention: [],
      recent: [],
      noDataNote: null
    };
  // Tile numbers derive from the fixture population inside the scoped range —
  // never asserted figures the signal pages would then contradict.
  const scopedLogs = inRange(logRecords, scope.range).filter(
    (record) => !scope.service || record.service === scope.service
  );
  const logErrors = scopedLogs.filter((record) => record.level === 'error').length;
  const scopedTraces = inRange(traceRows, scope.range).filter(
    (row) => !scope.service || row.service === scope.service
  );
  const traceErrors = scopedTraces.filter((row) => row.status.tone === 'danger').length;
  const evalFailed = evalRows.filter((row) => row.status.tone === 'danger').length;
  return {
    tiles: [
      { signal: 'Logs', href: `${base}/observe/logs${q()}`, value: fmtCount(scopedLogs.length), unit: 'records', status: logErrors ? { label: `${logErrors} error`, tone: 'warn' as const } : { label: 'No errors', tone: 'ok' as const } },
      { signal: 'Metrics', href: `${base}/observe/metrics${q()}`, value: `${metricCatalog.length}`, unit: 'metrics', status: { label: 'Series healthy', tone: 'ok' } },
      { signal: 'Traces', href: `${base}/observe/traces${q()}`, value: fmtCount(scopedTraces.length), unit: 'traces', status: traceErrors ? { label: `${traceErrors} error`, tone: 'warn' as const } : { label: 'No errors', tone: 'ok' as const } },
      { signal: 'Eval', href: `${base}/observe/evaluations${q()}`, value: `${evalRows.length}`, unit: 'events', status: { label: `${evalFailed} failed`, tone: evalFailed ? 'danger' : 'ok' } },
      { signal: 'Drift', href: `${base}/observe/drift${q()}`, value: '—', unit: '', status: { label: 'No report in range', tone: 'neutral' }, noData: true }
    ],
    attention: [
      { signal: 'Traces', what: 'capture 504 from ledger-api', service: 'checkout-api', state: { label: 'Attention', tone: 'warn' }, since: '12m', href: `${base}/observe/traces${q({ status: 'error', service: 'checkout-api' })}` },
      { signal: 'Eval', what: 'groundedness 0.61 < 0.80', service: 'checkout-agent', state: { label: 'Failed', tone: 'danger' }, since: '18m', href: `${base}/observe/evaluations/eval_record_01?task=groundedness` },
      { signal: 'Drift', what: 'score psi 0.31 > 0.20', service: 'ranking-api', state: { label: 'Failed', tone: 'danger' }, since: '3h', href: `${base}/observe/drift?service=ranking-api&feature=score&range=30d` }
    ],
    recent: [
      { time: '12:41', signal: 'Traces', event: 'trace_01 recorded with an error root span', service: 'checkout-api', href: `${base}/observe/traces/trace_01${q()}` },
      { time: '12:41', signal: 'Eval', event: 'eval_record_01 completed · failed', service: 'checkout-agent', href: `${base}/observe/evaluations/eval_record_01` },
      { time: '12:38', signal: 'Logs', event: 'error volume crossed the 1h baseline', service: 'checkout-api', href: `${base}/observe/logs${q({ level: 'error' })}` },
      { time: '12:20', signal: 'Metrics', event: 'p95 latency series refreshed', service: 'checkout-api', href: `${base}/observe/metrics${q()}` },
      { time: '09:40', signal: 'Drift', event: 'calculated report published for 30d window', service: 'ranking-api', href: `${base}/observe/drift?service=ranking-api&range=30d` }
    ],
    noDataNote: {
      signal: 'Drift',
      detail: `No Drift report was calculated in the last ${scope.range}. Widen the range or open Drift for the 30d window. Absence of a report is not a passing verdict.`
    }
  };
}

/** Distinct telemetry service facet, projected from the stored rows. */
function serviceFacet(): string[] {
  return [...new Set([...logRecords.map((r) => r.service), ...traceRows.map((r) => r.service)])].sort();
}

/**
 * Builds one field facet: distinct values of `read(row)` with match counts.
 *
 * Counts are cross-filtered — each value is counted against the rows that pass
 * every OTHER active filter, so a facet shows what selecting it would return
 * instead of shrinking to only the already-selected value.
 */
function fieldFacet<Row>(
  name: string,
  rows: Row[],
  read: (row: Row) => string,
  active: string,
  othersPass: (row: Row) => boolean
): { name: string; values: { value: string; count: number; active: boolean }[] } {
  const counts = new Map<string, number>();
  for (const row of rows) counts.set(read(row), 0);
  for (const row of rows.filter(othersPass))
    counts.set(read(row), (counts.get(read(row)) ?? 0) + 1);
  return {
    name,
    values: [...counts.entries()]
      .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
      .map(([value, count]) => ({ value, count, active: value === active }))
  };
}

/** Composes the WHERE clause the structured filters express, for display and Query handoff. */
function whereClause(clauses: (string | false | undefined)[], range: string): string {
  return [...clauses.filter(Boolean), `time > now() - INTERVAL '${range || '1h'}'`].join(' AND ');
}

/** O-02 — guided log search over the range-scoped fixture population. */
export function projectLogs(filters: Record<string, string>, populated = true): LogsView {
  const scoped = inRange(logRecords, filters.range || '1h');
  const byService = (record: (typeof logRecords)[number]) =>
    !filters.service || record.service === filters.service;
  const byLevel = (record: (typeof logRecords)[number]) =>
    !filters.level || record.level === filters.level;
  const byTrace = (record: (typeof logRecords)[number]) =>
    !filters.trace || record.traceId === filters.trace;
  const byQ = (record: (typeof logRecords)[number]) =>
    record.message.toLowerCase().includes((filters.q ?? '').toLowerCase());
  const records = scoped.filter((r) => byService(r) && byLevel(r) && byTrace(r) && byQ(r));
  // Page-based pagination over the matching set, mirroring the partitioned
  // search contract: the browser only ever receives one page of rows.
  const perPage = LOG_PAGE_SIZES.includes(Number(filters.per)) ? Number(filters.per) : 50;
  const pageCount = Math.max(1, Math.ceil(records.length / perPage));
  const page = Math.min(pageCount, Math.max(1, Number(filters.page) || 1));
  const loaded = records.slice((page - 1) * perPage, page * perPage);
  // Drawer opens only on an explicit row click — no auto-selected record.
  const selected = records.find((record) => record.id === filters.record) ?? null;
  const clauses = [
    filters.service && `service = '${filters.service}'`,
    filters.level && `level = '${filters.level}'`,
    filters.trace && `trace_id = '${filters.trace}'`,
    filters.q && `message LIKE '%${filters.q}%'`
  ];
  const queryWhere = whereClause(clauses, filters.range);
  return structuredClone({
    matching: { count: records.length, exact: true },
    cost: searchCost(scoped.length),
    page,
    perPage,
    pageCount,
    trend: records.length ? bucketCounts(records.filter((r) => r.level === 'error'), filters.range || '1h') : [],
    trendEnd: MOCK_NOW,
    fields: populated
      ? [
          fieldFacet('service', scoped, (r) => r.service, filters.service ?? '', (r) => byLevel(r) && byTrace(r) && byQ(r)),
          fieldFacet('level', scoped, (r) => r.level, filters.level ?? '', (r) => byService(r) && byTrace(r) && byQ(r))
        ]
      : [],
    records: loaded,
    selected,
    queryWhere,
    querySql: `SELECT time, severity_text, service_name, body, attributes, trace_id, span_id\nFROM vala.system.logs\nWHERE ${queryWhere.replaceAll(' AND ', '\n  AND ')}\nORDER BY time DESC`
  });
}

/** O-03 — metric discovery, chart series and underlying values. */
export function projectMetrics(filters: Record<string, string>, populated = true): MetricsView {
  if (!populated)
    return { services: [], metrics: [], selected: null, series: [], labels: [], values: { columns: [], rows: [] }, cardHref: null };
  const selected =
    metricCatalog.find((metric) => metric.name === filters.metric) ?? metricCatalog[0];
  const hasSeries = selected.name === 'http.server.duration';
  return structuredClone({
    services: serviceFacet(),
    metrics: metricCatalog,
    selected,
    series: hasSeries ? metricSeries : [],
    labels: hasSeries
      ? [
          { label: '09:00', at: '2026-09-06T09:00:00Z' },
          { label: '', at: '2026-09-06T09:30:00Z' },
          { label: '', at: '2026-09-06T10:00:00Z' },
          { label: '', at: '2026-09-06T10:30:00Z' },
          { label: '', at: '2026-09-06T11:00:00Z' },
          { label: '', at: '2026-09-06T11:30:00Z' },
          { label: '12:00', at: '2026-09-06T12:00:00Z' },
          { label: '', at: '2026-09-06T12:10:00Z' },
          { label: '', at: '2026-09-06T12:20:00Z' },
          { label: '', at: '2026-09-06T12:30:00Z' },
          { label: '', at: '2026-09-06T12:40:00Z' },
          { label: '13:00', at: '2026-09-06T13:00:00Z' }
        ]
      : [],
    values: hasSeries ? metricValues : { columns: [], rows: [] },
    cardHref: filters.service === 'checkout-api' ? 'card_service_01' : null
  });
}

/** O-04 — trace search: RED strip and facet-filtered rows; a row opens the trace. */
export function projectTraces(filters: Record<string, string>, populated = true): TracesView {
  const range = filters.range || '1h';
  const scoped = inRange(traceRows, range);
  const byService = (row: (typeof traceRows)[number]) =>
    !filters.service || row.service === filters.service;
  const byStatus = (row: (typeof traceRows)[number]) =>
    !filters.status || row.status.label.toLowerCase() === filters.status;
  const byQ = (row: (typeof traceRows)[number]) =>
    !filters.q ||
    row.id.includes(filters.q) ||
    row.rootOperation.toLowerCase().includes(filters.q.toLowerCase());
  const rows = scoped.filter((row) => byService(row) && byStatus(row) && byQ(row));
  // Load-more contract: the list grows in TRACE_LOAD_STEP appends, never the
  // whole matching set at once.
  const limit = Math.min(
    500,
    Math.max(TRACE_LOAD_STEP, Number(filters.limit) || TRACE_LOAD_STEP)
  );
  const errors = rows.filter((row) => row.status.tone === 'danger');
  const durations = rows.map((row) => row.durationMs).sort((a, b) => a - b);
  const p95Ms = durations.length
    ? durations[Math.min(durations.length - 1, Math.floor(durations.length * 0.95))]
    : 0;
  return structuredClone({
    matching: { count: rows.length, exact: true },
    cost: searchCost(scoped.length),
    limit,
    hasMore: rows.length > limit,
    errorRate: rows.length ? `${((errors.length / rows.length) * 100).toFixed(1)}%` : '0%',
    p95: `${(p95Ms / 1000).toFixed(2)}s`,
    trend: rows.length ? bucketCounts(errors, range) : [],
    rateTrend: rows.length ? bucketCounts(rows, range) : [],
    durationTrend: rows.length ? bucketP95(rows, range) : [],
    trendEnd: MOCK_NOW,
    fields: populated
      ? [
          fieldFacet('service', scoped, (r) => r.service, filters.service ?? '', (r) => byStatus(r) && byQ(r)),
          fieldFacet('status', scoped, (r) => r.status.label.toLowerCase(), filters.status ?? '', (r) => byService(r) && byQ(r))
        ]
      : [],
    queryWhere: whereClause(
      [
        filters.service && `service = '${filters.service}'`,
        filters.status && `status = '${filters.status}'`,
        filters.q && `(trace_id LIKE '%${filters.q}%' OR root_operation LIKE '%${filters.q}%')`
      ],
      filters.range
    ),
    rows: rows.slice(0, limit)
  });
}

/** O-05 — one trace with a span selected; unknown span falls to the error span. */
export function projectTrace(id: string, spanId: string): TraceDetail | null {
  const row = traceRows.find((candidate) => candidate.id === id);
  const detail = traceDetails[id] ?? (row ? sketchFromRow(row) : null);
  if (!detail) return null;
  const clone = structuredClone(detail);
  clone.selected =
    clone.spans.find((span) => span.id === spanId) ??
    clone.spans.find((span) => span.error) ??
    clone.spans[0];
  return clone;
}


/** O-09 — GenAI call search over the extracted `genai.*` records. */
export function projectGenAi(filters: Record<string, string>, populated = true): GenAiView {
  const range = filters.range || '1h';
  const scoped = inRange(genaiRows, range);
  const byService = (row: (typeof genaiRows)[number]) =>
    !filters.service || row.service === filters.service;
  const byModel = (row: (typeof genaiRows)[number]) =>
    !filters.model || row.model === filters.model;
  const byOperation = (row: (typeof genaiRows)[number]) =>
    !filters.operation || row.operation === filters.operation;
  const byQ = (row: (typeof genaiRows)[number]) =>
    !filters.q ||
    row.traceId.includes(filters.q) ||
    (row.conversationId ?? '').includes(filters.q);
  const rows = scoped.filter((row) => byService(row) && byModel(row) && byOperation(row) && byQ(row));
  const limit = Math.min(500, Math.max(TRACE_LOAD_STEP, Number(filters.limit) || TRACE_LOAD_STEP));
  return structuredClone({
    matching: { count: rows.length, exact: true },
    cost: searchCost(scoped.length),
    limit,
    hasMore: rows.length > limit,
    fields: populated
      ? [
          fieldFacet('model', scoped, (r) => r.model, filters.model ?? '', (r) => byService(r) && byOperation(r) && byQ(r)),
          fieldFacet('operation', scoped, (r) => r.operation, filters.operation ?? '', (r) => byService(r) && byModel(r) && byQ(r)),
          fieldFacet('service', scoped, (r) => r.service, filters.service ?? '', (r) => byModel(r) && byOperation(r) && byQ(r))
        ]
      : [],
    queryWhere: whereClause(
      [
        filters.service && `service_name = '${filters.service}'`,
        filters.model && `request_model = '${filters.model}'`,
        filters.operation && `operation_name = '${filters.operation}'`,
        filters.q && `(trace_id LIKE '%${filters.q}%' OR conversation_id LIKE '%${filters.q}%')`
      ],
      filters.range
    ),
    rows: rows.slice(0, limit)
  });
}

/** O-06 — read-only dashboard inventory with folder/tag filters. */
export function projectDashboards(filters: Record<string, string>): DashboardsView {
  const rows = dashboardRows.filter(
    (row) =>
      (!filters.folder || row.folder === filters.folder) &&
      (!filters.tag || row.tags.includes(filters.tag)) &&
      row.name.toLowerCase().includes((filters.q ?? '').toLowerCase())
  );
  return structuredClone({ rows });
}

/** O-07 — one read-only dashboard with its variables applied. */
export function projectDashboard(id: string, variables: Record<string, string>): DashboardDetail | null {
  const detail = dashboardDetails[id];
  if (!detail) return null;
  const clone = structuredClone(detail);
  for (const variable of clone.variables) {
    const requested = variables[variable.name];
    if (requested && variable.options.includes(requested)) variable.value = requested;
  }
  return clone;
}

/** O-08 — evaluation event inventory with URL-restored filters. */
export function projectEvals(filters: Record<string, string>, populated = true): EvalsView {
  const rows = evalRows.filter(
    (row) =>
      (!filters.service || row.subject === filters.service) &&
      (!filters.status || row.status.label.toLowerCase() === filters.status) &&
      (!filters.origin || row.origin === filters.origin) &&
      (!filters.evalCard || filters.evalCard === 'card_eval_01') &&
      row.recordId.includes(filters.q ?? '')
  );
  return structuredClone({
    matching: rows.length ? 41 : 0,
    subjects: populated ? [...new Set(evalRows.map((row) => row.subject))] : [],
    rows
  });
}

/** O-09 — one evaluation event with its workflow drawer and selected task. */
export function projectEval(recordId: string, task: string): EvalDetail | null {
  const detail = evalDetails[recordId];
  if (!detail) return null;
  const clone = structuredClone(detail);
  clone.selected = clone.tasks.find((candidate) => candidate.name === task) ?? clone.tasks[0];
  return clone;
}

/** O-10 — drift results; a range without a report projects absence, not health. */
export function projectDrift(filters: Record<string, string>): DriftView {
  const hasReport = filters.range === '30d' || filters.range === '7d';
  const features = structuredClone(driftFeatures);
  const selectedFeature =
    features.find((feature) => feature.name === filters.feature) ?? features[0];
  return {
    card: { id: 'card_drift_01', name: 'Ranking score drift', version: 'v3', href: 'card_drift_01' },
    definition: {
      method: 'psi',
      baseline: 'txns-2026q3 train',
      window: '1d over 30d',
      condition:
        'score > threshold for 2 consecutive windows — this page renders calculated reports, never the declaration.'
    },
    report: hasReport
      ? {
          id: 'drift_report_2026_09_02',
          calculated: '2026-09-02 04:00Z',
          verdict: { label: 'Failed — score drifted', tone: 'danger' },
          summary: '1 of 4 features breached its threshold for 2 consecutive windows.',
          features,
          selectedFeature,
          history: {
            points: driftHistory[selectedFeature.name],
            labels: structuredClone(driftHistoryLabels),
            threshold: { value: selectedFeature.threshold, label: `threshold ${selectedFeature.threshold.toFixed(2)}` }
          },
          alert:
            selectedFeature.name === 'score'
              ? {
                  finding: 'score psi 0.31 > 0.20',
                  breached: '2026-09-01 and 09-02',
                  reaction: 'drift-response (Operator)',
                  operatorHref: 'card_operator_01'
                }
              : null
        }
      : null
  };
}
