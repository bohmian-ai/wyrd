/** The panels both pages show: the support desk Service and its spans per hour. */
import type { CardHeaderInput, ChartInput, Range } from 'wyrd-ai-ui/loaders';

export const range: Range = { last: '24h' };

export const service: CardHeaderInput = {
  cardRef: { kind: 'Service', name: 'support-desk', version: '1.0.0', space: 'default' }
};

export const spans: ChartInput = {
  type: 'timeseries',
  title: 'Spans per hour',
  sql: `SELECT date_bin(INTERVAL '1 hour', start_time_unix_nano) AS hour, count(*) AS spans
FROM vala.traces.spans
WHERE start_time_unix_nano >= $1 AND start_time_unix_nano < $2
GROUP BY 1 ORDER BY 1`,
  x: 'hour',
  y: ['spans']
};

/** The same panels as a view document, as an agent would author it. */
export const dashboard = {
  version: 'wyrd.view/v1',
  catalogId: 'wyrd.ui/v1',
  title: 'Support desk',
  range,
  panels: [
    { id: 'service', component: 'CardHeader', input: service, layout: { x: 0, y: 0, w: 12, h: 3 } },
    { id: 'spans', component: 'Chart', input: spans, layout: { x: 0, y: 3, w: 12, h: 8 } }
  ]
};
