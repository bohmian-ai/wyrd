/**
 * Representative component data shared by the dev gallery and the component
 * tests. Shapes follow the real wire types; values are synthetic.
 */
import type { CardRef, Executor, QueryResult, RegisteredCard } from 'wyrd-ai-ui';
import type { ChartInput } from 'wyrd-ai-ui/loaders';

/** A terminal for a complete result of `rows` rows. */
export function terminal(rows: number, outcome: 'success' | 'degraded' = 'success'): QueryResult['terminal'] {
  return {
    outcome,
    query_class: 'interactive',
    row_count: rows,
    warnings: outcome === 'degraded' ? ['live_tail_unavailable'] : [],
    source_completion: []
  };
}

export const series: QueryResult = {
  columns: [
    { name: 'at', data_type: 'Timestamp(Microsecond, None)', nullable: false },
    { name: 'model', data_type: 'Utf8', nullable: false },
    { name: 'p95', data_type: 'Float64', nullable: true }
  ],
  rows: [
    ['2026-10-20T10:00:00', 'a', 120],
    ['2026-10-20T10:00:00', 'b', 80],
    ['2026-10-20T11:00:00', 'a', 140],
    ['2026-10-20T11:00:00', 'b', null]
  ],
  terminal: terminal(4)
};

/** A day of hourly p95 for two models, for drawing a realistic plot. */
export const day: QueryResult = {
  columns: series.columns,
  rows: Array.from({ length: 24 }, (_, hour) => {
    const at = `2026-10-20T${String(hour).padStart(2, '0')}:00:00`;
    return [
      [at, 'reranker', 180 + Math.round(60 * Math.sin(hour / 3)) + (hour % 5) * 7],
      [at, 'embedder', 90 + Math.round(25 * Math.cos(hour / 4)) + (hour % 3) * 5]
    ];
  }).flat(),
  terminal: terminal(48)
};
export const timeseries: ChartInput = { type: 'timeseries', title: 'Latency', sql: 'SELECT at, model, p95 FROM t WHERE at >= $1::timestamptz AND at < $2::timestamptz', unit: 'ms', x: 'at', y: ['p95'], series: 'model' };
export const card: CardRef = { kind: 'Model', name: 'ranker', version: '1.4.2', space: 'search' };
export const registered = {
  apiVersion: 'wyrd/v1',
  kind: 'Model',
  metadata: { name: 'ranker', space: 'search', version: '1.4.2', uid: 'card_01HX' },
  spec: {}
} as unknown as RegisteredCard;
export const summary = (version: string, status = 'active') => ({ version, status, name: 'ranker', kind: 'Model', space: 'search', created_at: '2026-10-19T14:02:00Z' }) as never;
export const denied = { code: 'WYRD_AUTH_403_FORBIDDEN', title: 'Forbidden', status: 403 };
export const RANGE = { from: '2026-10-20T00:00:00Z', to: '2026-10-20T12:00:00Z' };


/** An executor serving the fixtures above, for views rendered without a server. */
export const fixtureExecutor: Executor = {
  run: (requests) =>
    Promise.resolve(
      requests.map((request) => {
        switch (request.op) {
          case 'cards.get_by_ref':
            return { ok: true, value: registered };
          case 'cards.list':
            return { ok: true, value: { items: [summary('1.4.2'), summary('1.4.1', 'deprecated')], next_cursor: null } };
          case 'bifrost.query': {
            // Honour bound $1/$2 like the server, so a brushed range visibly narrows the plot.
            const [from, to] = request.params ?? [];
            if (typeof from !== 'string' || typeof to !== 'string') return { ok: true, value: day };
            const rows = day.rows.filter((row) => Date.parse(`${row[0]}Z`) >= Date.parse(from) && Date.parse(`${row[0]}Z`) < Date.parse(to));
            return { ok: true, value: { ...day, rows, terminal: terminal(rows.length) } };
          }
          default:
            return { ok: false, problem: denied };
        }
      })
    ) as never
};
