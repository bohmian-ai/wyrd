import type {
  DashboardDetail,
  DashboardsView,
  DriftView,
  EvalDetail,
  EvalsView,
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
  logRecords,
  logTrend,
  metricCatalog,
  metricSeries,
  metricValues,
  traceDetails,
  traceRows,
  traceTrend
} from './fixtures';

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
  return {
    tiles: [
      { signal: 'Logs', href: `${base}/observe/logs${q()}`, value: '18.4k', unit: 'records', status: { label: '412 error', tone: 'warn' } },
      { signal: 'Metrics', href: `${base}/observe/metrics${q()}`, value: '36', unit: 'series', status: { label: 'Series healthy', tone: 'ok' } },
      { signal: 'Traces', href: `${base}/observe/traces${q()}`, value: '1,204', unit: 'traces', status: { label: '97 error', tone: 'warn' } },
      { signal: 'Eval', href: `${base}/observe/evaluations${q()}`, value: '3', unit: 'events', status: { label: '1 failed', tone: 'danger' } },
      { signal: 'Drift', href: `${base}/observe/drift${q()}`, value: '—', unit: '', status: { label: 'No report in range', tone: 'neutral' }, noData: true }
    ],
    attention: [
      { signal: 'Traces', what: 'capture 504 from ledger-api', service: 'checkout-api', state: { label: 'Attention', tone: 'warn' }, since: '12m', href: `${base}/observe/traces${q({ status: 'error' })}` },
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

/** O-02 — guided log search over the fixture records. */
export function projectLogs(filters: Record<string, string>, populated = true): LogsView {
  const records = logRecords.filter(
    (record) =>
      (!filters.service || record.service === filters.service) &&
      (!filters.level || record.level === filters.level) &&
      record.message.toLowerCase().includes((filters.q ?? '').toLowerCase())
  );
  const selected = records.find((record) => record.id === filters.record) ?? records[0] ?? null;
  const clauses = [
    filters.service && `service = '${filters.service}'`,
    filters.level && `level = '${filters.level}'`,
    filters.q && `message LIKE '%${filters.q}%'`
  ].filter(Boolean);
  return structuredClone({
    matching: records.length ? 412 : 0,
    loaded: records.length ? 200 : 0,
    trend: records.length ? logTrend : [],
    services: populated ? serviceFacet() : [],
    levels: populated ? [...new Set(logRecords.map((r) => r.level))] : [],
    records,
    selected,
    querySql: `SELECT time, level, service, message, trace_id\nFROM vala.system.logs\nWHERE ${clauses.join('\n  AND ') || 'true'}\n  AND time > now() - INTERVAL '${filters.range || '1h'}'\nORDER BY time DESC`
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

/** O-04 — trace search: trend and facet-filtered rows; a row opens the trace. */
export function projectTraces(filters: Record<string, string>, populated = true): TracesView {
  const rows = traceRows.filter(
    (row) =>
      (!filters.service || row.service === filters.service) &&
      (!filters.status || row.status.label.toLowerCase() === filters.status) &&
      (!filters.q ||
        row.id.includes(filters.q) ||
        row.rootOperation.toLowerCase().includes(filters.q.toLowerCase()))
  );
  return structuredClone({
    matching: rows.length ? 97 : 0,
    loaded: rows.length ? 50 : 0,
    errorRate: '4.2%',
    p95: '1.84s',
    trend: rows.length ? traceTrend : [],
    services: populated ? serviceFacet() : [],
    rows
  });
}

/** O-05 — one trace with a span selected; unknown span falls to the error span. */
export function projectTrace(id: string, spanId: string): TraceDetail | null {
  const detail = traceDetails[id];
  if (!detail) return null;
  const clone = structuredClone(detail);
  clone.selected =
    clone.spans.find((span) => span.id === spanId) ??
    clone.spans.find((span) => span.error) ??
    clone.spans[0];
  return clone;
}

/** O-06 — read-only dashboard inventory with folder/tag filters. */
export function projectDashboards(filters: Record<string, string>): DashboardsView {
  const rows = dashboardRows.filter(
    (row) =>
      (!filters.folder || row.folder === filters.folder) &&
      (!filters.tag || row.tags.includes(filters.tag)) &&
      row.name.toLowerCase().includes((filters.q ?? '').toLowerCase())
  );
  const selected = rows.find((row) => row.id === filters.selected) ?? rows[0] ?? null;
  return structuredClone({
    rows: rows.map(({ id, name, folder, tags, owner, updated }) => ({ id, name, folder, tags, owner, updated })),
    selected
  });
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
