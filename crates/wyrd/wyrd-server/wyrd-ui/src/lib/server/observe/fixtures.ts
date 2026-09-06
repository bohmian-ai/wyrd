import type {
  DashboardDetail,
  DashboardRow,
  DriftFeature,
  EvalDetail,
  EvalRow,
  EvalTask,
  LogRecord,
  MetricInfo,
  Span,
  TraceDetail,
  TraceRow
} from '$lib/features/observe/core/types';

/**
 * Development fixtures for the Observe workspace, tenant `acme` only.
 *
 * One coherent incident: checkout-api capture failures against ledger-api
 * around 12:41, a failed groundedness evaluation on checkout-agent, and a
 * 30-day ranking-api score drift breach. Projected through `WyrdClient`
 * exactly like live data; these are not durable domain records.
 */

/** Log records behind the guided Logs search, newest first. */
export const logRecords: LogRecord[] = [
  {
    id: 'log_01',
    time: '12:41:07.221',
    at: '2026-09-06T12:41:07.221Z',
    level: 'error',
    service: 'checkout-api',
    message: 'capture declined: gateway timeout',
    traceId: 'trace_01',
    fields: [
      ['level', 'error'],
      ['service', 'checkout-api'],
      ['trace_id', 'trace_01'],
      ['span_id', 'span_0c41'],
      ['principal', 'svc-checkout-api'],
      ['http.status', '504'],
      ['upstream', 'ledger-api'],
      ['retry', '3 of 3'],
      ['cart_id', 'cart_88f1'],
      ['region', 'us-east-1']
    ],
    detail: 'capture declined: gateway timeout after 3 attempts'
  },
  {
    id: 'log_02',
    time: '12:41:07.198',
    at: '2026-09-06T12:41:07.198Z',
    level: 'error',
    service: 'checkout-api',
    message: 'retry budget exhausted (3/3)',
    traceId: 'trace_01',
    fields: [
      ['level', 'error'],
      ['service', 'checkout-api'],
      ['trace_id', 'trace_01'],
      ['span_id', 'span_0c3e'],
      ['upstream', 'ledger-api'],
      ['retry', '3 of 3']
    ],
    detail: 'retry budget exhausted after upstream 504s from ledger-api'
  },
  {
    id: 'log_03',
    time: '12:40:58.010',
    at: '2026-09-06T12:40:58.010Z',
    level: 'error',
    service: 'checkout-api',
    message: 'upstream 504 from ledger-api',
    traceId: 'trace_04',
    fields: [
      ['level', 'error'],
      ['service', 'checkout-api'],
      ['trace_id', 'trace_04'],
      ['http.status', '504'],
      ['upstream', 'ledger-api']
    ],
    detail: 'upstream 504 from ledger-api during capture'
  },
  {
    id: 'log_04',
    time: '12:40:31.774',
    at: '2026-09-06T12:40:31.774Z',
    level: 'error',
    service: 'checkout-api',
    message: 'capture declined: gateway timeout',
    traceId: 'trace_07',
    fields: [
      ['level', 'error'],
      ['service', 'checkout-api'],
      ['trace_id', 'trace_07'],
      ['http.status', '504'],
      ['upstream', 'ledger-api'],
      ['retry', '3 of 3']
    ],
    detail: 'capture declined: gateway timeout after 3 attempts'
  },
  {
    id: 'log_05',
    time: '12:39:12.402',
    at: '2026-09-06T12:39:12.402Z',
    level: 'error',
    service: 'checkout-api',
    message: 'cart lock contention',
    traceId: 'trace_09',
    fields: [
      ['level', 'error'],
      ['service', 'checkout-api'],
      ['trace_id', 'trace_09'],
      ['lock', 'cart_88d2'],
      ['waited_ms', '840']
    ],
    detail: 'cart lock contention while capture retries were in flight'
  },
  {
    id: 'log_06',
    time: '12:38:44.120',
    at: '2026-09-06T12:38:44.120Z',
    level: 'warn',
    service: 'checkout-api',
    message: 'capture latency above 1.5s',
    traceId: 'trace_12',
    fields: [
      ['level', 'warn'],
      ['service', 'checkout-api'],
      ['trace_id', 'trace_12'],
      ['duration_ms', '1620']
    ],
    detail: 'capture latency above the 1.5s soft budget'
  },
  {
    id: 'log_07',
    time: '12:37:20.551',
    at: '2026-09-06T12:37:20.551Z',
    level: 'info',
    service: 'ranking-api',
    message: 'ranking model refreshed',
    traceId: null,
    fields: [
      ['level', 'info'],
      ['service', 'ranking-api'],
      ['model', 'rank-v9']
    ],
    detail: 'ranking model refreshed from the registry'
  }
];

/** Error-records-per-minute buckets behind the Logs volume trend. */
export const logTrend = [3, 4, 3, 5, 4, 6, 5, 8, 10, 14, 12, 17, 20, 18, 22, 16, 13, 11, 9, 7];

/** Discoverable measures for the scoped service. */
export const metricCatalog: MetricInfo[] = [
  {
    name: 'http.server.duration',
    unit: 'ms',
    description: 'latency by percentile',
    labels: [
      ['service', 'checkout-api'],
      ['http.route', '3 values'],
      ['http.status', '5 values'],
      ['region', '2 values']
    ]
  },
  {
    name: 'http.server.requests',
    unit: 'req/s',
    description: 'request rate by status class',
    labels: [
      ['service', 'checkout-api'],
      ['http.status_class', '3 values'],
      ['region', '2 values']
    ]
  },
  {
    name: 'checkout.capture.rate',
    unit: '%',
    description: 'capture success share',
    labels: [
      ['service', 'checkout-api'],
      ['outcome', '4 values']
    ]
  },
  {
    name: 'checkout.cart.value',
    unit: 'usd',
    description: 'cart value distribution',
    labels: [
      ['service', 'checkout-api'],
      ['risk_tier', '3 values']
    ]
  },
  {
    name: 'ledger.capture.errors',
    unit: 'count',
    description: 'ledger capture failures',
    labels: [
      ['service', 'ledger-api'],
      ['http.status', '2 values']
    ]
  },
  {
    name: 'agent.decision.latency',
    unit: 'ms',
    description: 'agent decision latency',
    labels: [
      ['service', 'checkout-agent'],
      ['decision', '2 values']
    ]
  },
  {
    name: 'runtime.cpu.utilization',
    unit: '%',
    description: 'container CPU utilization',
    labels: [
      ['service', 'checkout-api'],
      ['pod', '6 values']
    ]
  },
  {
    name: 'runtime.memory.rss',
    unit: 'mb',
    description: 'resident memory',
    labels: [
      ['service', 'checkout-api'],
      ['pod', '6 values']
    ]
  }
];

/** Percentile series for `http.server.duration` over the scoped range. */
export const metricSeries = [
  { label: 'p50', points: [236, 238, 241, 244, 240, 246, 244, 251, 249, 255, 262, 248] },
  { label: 'p95', points: [866, 890, 902, 940, 921, 968, 1010, 1049, 1080, 1132, 1090, 984] },
  { label: 'p99', points: [1840, 1902, 1955, 2010, 1998, 2076, 2140, 2216, 2304, 2410, 2350, 2080] }
];

/** Hour buckets underlying the metric chart, most recent first. */
export const metricValues = {
  columns: ['bucket', 'p50', 'p95', 'p99', 'count'],
  rows: [
    ['12:00–13:00', '248', '984', '2080', '18,211'],
    ['11:00–12:00', '251', '1010', '2140', '17,904'],
    ['10:00–11:00', '244', '940', '2010', '18,002'],
    ['09:00–10:00', '236', '866', '1840', '17,441']
  ]
};

/** Trace search rows for the scoped error window, newest first. */
export const traceRows: TraceRow[] = [
  { id: 'trace_01', rootOperation: 'POST /checkout/capture', service: 'checkout-api', start: '12:41:07', durationMs: 1840, spans: 42, errorSpans: 1, status: { label: 'Error', tone: 'danger' } },
  { id: 'trace_04', rootOperation: 'POST /checkout/capture', service: 'checkout-api', start: '12:40:58', durationMs: 1510, spans: 38, errorSpans: 1, status: { label: 'Error', tone: 'danger' } },
  { id: 'trace_07', rootOperation: 'POST /checkout/capture', service: 'checkout-api', start: '12:40:31', durationMs: 2100, spans: 44, errorSpans: 2, status: { label: 'Error', tone: 'danger' } },
  { id: 'trace_09', rootOperation: 'POST /cart/lock', service: 'checkout-api', start: '12:39:12', durationMs: 920, spans: 17, errorSpans: 1, status: { label: 'Error', tone: 'danger' } },
  { id: 'trace_12', rootOperation: 'POST /checkout/capture', service: 'checkout-api', start: '12:38:44', durationMs: 1620, spans: 40, errorSpans: 1, status: { label: 'Error', tone: 'danger' } },
  { id: 'trace_15', rootOperation: 'GET /cart/summary', service: 'checkout-api', start: '12:37:20', durationMs: 1120, spans: 21, errorSpans: 1, status: { label: 'Error', tone: 'danger' } },
  { id: 'trace_18', rootOperation: 'POST /checkout/capture', service: 'checkout-api', start: '12:36:02', durationMs: 1980, spans: 43, errorSpans: 3, status: { label: 'Error', tone: 'danger' } },
  { id: 'trace_21', rootOperation: 'POST /cart/lock', service: 'checkout-api', start: '12:34:47', durationMs: 1050, spans: 18, errorSpans: 1, status: { label: 'Error', tone: 'danger' } }
];

/** Error-traces-per-minute buckets behind the Traces trend. */
export const traceTrend = [2, 3, 2, 4, 5, 7, 6, 9, 7, 5, 4, 6, 8, 11, 9, 6, 4, 3, 2, 2];

/** Spans of trace_01 in start order; the waterfall scrolls, no row is dropped. */
const trace01Spans: Span[] = [
  {
    id: 'span_root',
    name: 'POST /checkout/capture',
    kind: 'server',
    startPct: 0,
    widthPct: 100,
    durationLabel: '1.84s',
    error: false,
    fields: [
      ['kind', 'server'],
      ['status', 'OK'],
      ['http.route', '/checkout/capture'],
      ['principal', 'svc-checkout-api'],
      ['duration', '1.84s']
    ],
    aiContent: null,
    aiAbsentReason: 'No AI content on this span — it is an HTTP server span, so no prompt, completion or token content exists. Shown as absent, not hidden.',
    link: null
  },
  {
    id: 'span_auth',
    name: 'authorize.principal',
    kind: 'tool',
    startPct: 2,
    widthPct: 8,
    durationLabel: '0.15s',
    error: false,
    fields: [
      ['kind', 'tool'],
      ['status', 'OK'],
      ['principal', 'svc-checkout-api'],
      ['duration', '0.15s']
    ],
    aiContent: null,
    aiAbsentReason: 'No AI content on this span — it is a tool call, so no prompt, completion or token content exists. Shown as absent, not hidden.',
    link: null
  },
  {
    id: 'span_rank',
    name: 'rank.candidates',
    kind: 'llm',
    startPct: 12,
    widthPct: 34,
    durationLabel: '0.63s',
    error: false,
    fields: [
      ['kind', 'llm'],
      ['status', 'OK'],
      ['model', 'rank-v9'],
      ['tokens.in', '412'],
      ['tokens.out', '96'],
      ['duration', '0.63s']
    ],
    aiContent: {
      prompt: 'Rank these 12 capture candidates for cart cart_88f1 (risk_tier=high).',
      completion: 'Ranked candidates: [c3, c1, c7, …] — top candidate c3 at 0.91 confidence.'
    },
    aiAbsentReason: null,
    link: null
  },
  {
    id: 'span_cart',
    name: 'retrieve.cart-history',
    kind: 'retrieval',
    startPct: 30,
    widthPct: 14,
    durationLabel: '0.26s',
    error: false,
    fields: [
      ['kind', 'retrieval'],
      ['status', 'OK'],
      ['documents', '3'],
      ['duration', '0.26s']
    ],
    aiContent: null,
    aiAbsentReason: 'No AI content on this span — retrieval spans carry document references, not prompt or completion content.',
    link: null
  },
  {
    id: 'span_decide',
    name: 'agent.decide-capture',
    kind: 'agent',
    startPct: 48,
    widthPct: 24,
    durationLabel: '0.44s',
    error: false,
    fields: [
      ['kind', 'agent'],
      ['status', 'OK'],
      ['decision', 'capture'],
      ['confidence', '0.87'],
      ['duration', '0.44s']
    ],
    aiContent: {
      prompt: 'Decide capture vs hold for cart_88f1 given ranked candidates and history.',
      completion: 'capture — grounded on candidate c3 and 3 prior successful captures.'
    },
    aiAbsentReason: null,
    link: null
  },
  {
    id: 'span_0c41',
    name: 'ledger.capture',
    kind: 'tool',
    startPct: 73,
    widthPct: 25,
    durationLabel: '0.46s',
    error: true,
    fields: [
      ['kind', 'tool · client'],
      ['status', 'ERROR'],
      ['http.status', '504'],
      ['peer.service', 'ledger-api'],
      ['retry.count', '3'],
      ['duration', '0.46s'],
      ['event', 'exception @ 0.44s'],
      ['event', 'retry ×2']
    ],
    aiContent: null,
    aiAbsentReason: 'No AI content on this span — it is a tool call, so no prompt, completion or token content exists. Shown as absent, not hidden.',
    link: { label: 'trace_04 · span_1a02', href: 'trace_04' }
  }
];

/** Trace details addressable by id; only trace_01 is fully drawn. */
export const traceDetails: Record<string, TraceDetail> = {
  trace_01: {
    id: 'trace_01',
    rootOperation: 'POST /checkout/capture',
    service: 'checkout-api',
    start: '12:41:07.198',
    duration: '1.84s',
    spanCount: 42,
    errorCount: 1,
    spans: trace01Spans,
    selected: trace01Spans[5],
    graph: {
      nodes: [
        { name: 'checkout-api', cardHref: 'card_service_01' },
        { name: 'ranking-api', cardHref: 'card_service_03' },
        { name: 'checkout-agent', cardHref: 'card_service_04' },
        { name: 'ledger-api', cardHref: 'card_service_02' }
      ],
      edges: [
        { from: 'checkout-api', to: 'ranking-api', error: false, label: '' },
        { from: 'ranking-api', to: 'checkout-agent', error: false, label: '' },
        { from: 'checkout-agent', to: 'ledger-api', error: true, label: '504' }
      ]
    }
  }
};

/** Read-only dashboard inventory. */
export const dashboardRows: (DashboardRow & {
  panels: number;
  variables: number;
  defaultRange: string;
})[] = [
  { id: 'dashboard_01', name: 'Checkout health', folder: 'payments', tags: ['checkout', 'slo'], owner: 'j.reyes', updated: '2h ago', panels: 6, variables: 3, defaultRange: '24h' },
  { id: 'dashboard_02', name: 'Ledger capture', folder: 'payments', tags: ['ledger'], owner: 'j.reyes', updated: '1d ago', panels: 4, variables: 2, defaultRange: '24h' },
  { id: 'dashboard_03', name: 'Ranking quality', folder: 'ml', tags: ['ranking', 'drift'], owner: 'm.linden', updated: '2d ago', panels: 5, variables: 2, defaultRange: '7d' },
  { id: 'dashboard_04', name: 'Agent decisions', folder: 'ml', tags: ['agent', 'eval'], owner: 'm.linden', updated: '3d ago', panels: 4, variables: 1, defaultRange: '24h' },
  { id: 'dashboard_05', name: 'Cost and spend', folder: 'platform', tags: ['cost'], owner: 'r.okafor', updated: '5d ago', panels: 3, variables: 1, defaultRange: '30d' },
  { id: 'dashboard_06', name: 'Platform SLOs', folder: 'platform', tags: ['slo'], owner: 'j.reyes', updated: '1w ago', panels: 8, variables: 2, defaultRange: '7d' }
];

/** The one fully drawn read-only dashboard. */
export const dashboardDetails: Record<string, DashboardDetail> = {
  dashboard_01: {
    id: 'dashboard_01',
    name: 'Checkout health',
    owner: 'j.reyes',
    updated: '2h ago',
    variables: [
      { name: 'service', value: 'checkout-api', options: ['checkout-api', 'ledger-api'] },
      { name: 'region', value: 'us-east-1', options: ['us-east-1', 'eu-west-1'] }
    ],
    panels: [
      {
        title: 'Request rate',
        meta: 'req/s',
        kind: 'line',
        state: 'ok',
        series: [
          { label: '2xx', points: [310, 322, 316, 330, 341, 336, 348, 352, 344, 351] },
          { label: '5xx', points: [4, 5, 4, 6, 9, 12, 10, 8, 6, 5] }
        ],
        labels: [
          { label: '00:00', at: '2026-09-06T00:00:00Z' },
          { label: '', at: '2026-09-06T02:40:00Z' },
          { label: '', at: '2026-09-06T05:20:00Z' },
          { label: '', at: '2026-09-06T08:00:00Z' },
          { label: '', at: '2026-09-06T10:40:00Z' },
          { label: '', at: '2026-09-06T13:20:00Z' },
          { label: '', at: '2026-09-06T16:00:00Z' },
          { label: '', at: '2026-09-06T18:40:00Z' },
          { label: '', at: '2026-09-06T21:20:00Z' },
          { label: '24:00', at: '2026-09-07T00:00:00Z' }
        ]
      },
      {
        title: 'Latency',
        meta: 'ms',
        kind: 'line',
        state: 'ok',
        threshold: { value: 1200, label: 'threshold' },
        series: [
          { label: 'p50', points: [236, 241, 240, 246, 251, 255, 262, 258, 251, 248] },
          { label: 'p95', points: [866, 902, 921, 1010, 1080, 1240, 1310, 1180, 1049, 984] }
        ],
        labels: [
          { label: '00:00', at: '2026-09-06T00:00:00Z' },
          { label: '', at: '2026-09-06T02:40:00Z' },
          { label: '', at: '2026-09-06T05:20:00Z' },
          { label: '', at: '2026-09-06T08:00:00Z' },
          { label: '', at: '2026-09-06T10:40:00Z' },
          { label: '', at: '2026-09-06T13:20:00Z' },
          { label: '', at: '2026-09-06T16:00:00Z' },
          { label: '', at: '2026-09-06T18:40:00Z' },
          { label: '', at: '2026-09-06T21:20:00Z' },
          { label: '24:00', at: '2026-09-07T00:00:00Z' }
        ]
      },
      {
        title: 'Capture outcome',
        meta: '24h',
        kind: 'table',
        state: 'ok',
        table: {
          columns: ['outcome', 'count', 'share'],
          rows: [
            ['captured', '18,211', '97.1%'],
            ['held', '412', '2.2%'],
            ['declined', '97', '0.5%'],
            ['errored', '41', '0.2%']
          ]
        }
      },
      {
        title: 'Ledger errors',
        meta: '504 by hour · 24h',
        kind: 'bars',
        state: 'ok',
        bars: [
          { label: '06', value: 2 },
          { label: '08', value: 1 },
          { label: '10', value: 3 },
          { label: '11', value: 5 },
          { label: '12', value: 14 },
          { label: '13', value: 9 }
        ]
      },
      {
        title: 'Cart value',
        meta: 'no data',
        kind: 'line',
        state: 'empty',
        note: 'region=us-east-1 has no cart_value samples in the last 24h. The panel keeps its frame so the grid never jumps or blanks out.'
      },
      {
        title: 'Agent decisions',
        meta: 'panel error',
        kind: 'line',
        state: 'error',
        code: 'WYRD-PANEL-QUERY-FAILED',
        note: "This panel's query failed. Request id 8f10-2c. Retry. Other panels are unaffected — the dashboard does not blank out."
      }
    ]
  }
};

/** Evaluation event rows keyed by record_id; run_id is correlation. */
export const evalRows: EvalRow[] = [
  { recordId: 'eval_record_01', eval: 'Checkout agent eval', subject: 'checkout-agent', runId: 'run_88a1', status: { label: 'Failed', tone: 'danger' }, passed: 4, total: 6, duration: '12.4s', origin: 'online', scenario: null },
  { recordId: 'eval_record_02', eval: 'Checkout agent eval', subject: 'checkout-agent', runId: 'run_88a0', status: { label: 'Passed', tone: 'ok' }, passed: 6, total: 6, duration: '11.9s', origin: 'online', scenario: null },
  { recordId: 'eval_record_03', eval: 'Ranking quality eval', subject: 'ranking-api', runId: 'run_871f', status: { label: 'Failed', tone: 'danger' }, passed: 3, total: 5, duration: '9.7s', origin: 'online', scenario: null },
  { recordId: 'eval_record_04', eval: 'Refund agent scenarios', subject: 'local · unregistered', runId: 'run_local_2e', status: { label: 'Passed', tone: 'ok' }, passed: 4, total: 4, duration: '8.2s', origin: 'offline', scenario: { id: 'refund-partial-3', collection: 'customer-support-v2' } },
  { recordId: 'eval_record_05', eval: 'Refund agent scenarios', subject: 'local · unregistered', runId: 'run_local_2e', status: { label: 'Failed', tone: 'danger' }, passed: 2, total: 4, duration: '9.1s', origin: 'offline', scenario: { id: 'refund-escalate-1', collection: 'customer-support-v2' } }
];

/** Task results inside eval_record_01's workflow. */
const evalTasks: EvalTask[] = [
  {
    name: 'groundedness',
    method: 'judge',
    stage: 'stage 3',
    status: { label: 'Failed', tone: 'danger' },
    score: '0.61',
    detail: {
      type: 'assertion',
      weight: '1.0',
      started: '12:41:09.4Z',
      duration: '1.4s',
      attempt: '1 of 2 · provider-error retry only',
      cost: '$0.0041',
      scoreValue: 0.61,
      threshold: 0.8,
      comparison: 'judge (>=) · scored 0.61 vs >= 0.80 — fails',
      judgeExplanation:
        'The answer cites a cart total that does not appear in the retrieved cart record. Two of three grounding checks failed on the cited fields.',
      expected: ['grounded >= 0.80', 'cart_total: 84.10', 'items: 3 · risk_tier: high'],
      actual: ['grounded 0.61', 'cart_total: 91.55 ← uncited', 'items: 3 · risk_tier: high'],
      withheldReason: null,
      history: ['pass', 'pass', 'fail', 'pass', 'pass', 'pass', 'pass', 'pass', 'pass', 'fail'],
      traceLink: { label: 'open trace_01 · span rank.candidates', href: 'trace_01' }
    }
  },
  {
    name: 'tool-selection',
    method: 'exact',
    stage: 'stage 2',
    status: { label: 'Passed', tone: 'ok' },
    score: '1.00',
    detail: {
      type: 'assertion',
      weight: '0.5',
      started: '12:41:04.1Z',
      duration: '0.2s',
      attempt: '1 of 1',
      cost: '—',
      scoreValue: 1,
      threshold: 1,
      comparison: 'exact (==) · ledger.capture == ledger.capture — passes',
      judgeExplanation: null,
      expected: ['tool: ledger.capture'],
      actual: ['tool: ledger.capture'],
      withheldReason: null,
      history: ['pass', 'pass', 'pass', 'pass', 'pass', 'pass', 'pass', 'pass', 'pass', 'pass'],
      traceLink: null
    }
  },
  {
    name: 'pii-leak',
    method: 'regex',
    stage: 'stage 2',
    status: { label: 'Passed', tone: 'ok' },
    score: '1.00',
    detail: {
      type: 'assertion',
      weight: '1.0',
      started: '12:41:04.3Z',
      duration: '0.1s',
      attempt: '1 of 1',
      cost: '—',
      scoreValue: 1,
      threshold: 1,
      comparison: 'regex (no match) · no PII pattern matched — passes',
      judgeExplanation: null,
      expected: ['no card number, SSN or address in the response'],
      actual: null,
      withheldReason: 'Actual values withheld — this task inspects raw response content and your principal is not authorized to read it.',
      history: ['pass', 'pass', 'pass', 'pass', 'pass', 'pass', 'pass', 'pass', 'pass', 'pass'],
      traceLink: null
    }
  },
  {
    name: 'latency-budget',
    method: '<=',
    stage: 'stage 3',
    status: { label: 'Failed', tone: 'danger' },
    score: '1.84s',
    detail: {
      type: 'assertion',
      weight: '0.5',
      started: '12:41:08.9Z',
      duration: '0.1s',
      attempt: '1 of 1',
      cost: '—',
      scoreValue: 1.84,
      threshold: 1.5,
      comparison: '<= · measured 1.84s vs <= 1.50s — fails',
      judgeExplanation: null,
      expected: ['end-to-end <= 1.50s'],
      actual: ['end-to-end 1.84s'],
      withheldReason: null,
      history: ['pass', 'pass', 'pass', 'fail', 'pass', 'pass', 'fail', 'pass', 'fail', 'fail'],
      traceLink: { label: 'open trace_01 · root span', href: 'trace_01' }
    }
  },
  {
    name: 'refusal-rate',
    method: 'ratio',
    stage: 'stage 4',
    status: { label: 'Skipped', tone: 'neutral' },
    score: '—',
    detail: {
      type: 'assertion',
      weight: '0.5',
      started: '—',
      duration: '—',
      attempt: '—',
      cost: '—',
      scoreValue: null,
      threshold: null,
      comparison: 'skipped — its condition gates on stage 3 passing',
      judgeExplanation: null,
      expected: ['refusal ratio <= 0.02 over the session'],
      actual: null,
      withheldReason: null,
      history: [],
      traceLink: null
    }
  },
  {
    name: 'tone',
    method: 'judge',
    stage: 'stage 3',
    status: { label: 'Error', tone: 'warn' },
    score: '—',
    detail: {
      type: 'assertion',
      weight: '0.25',
      started: '12:41:09.6Z',
      duration: '2.0s',
      attempt: '2 of 2 · provider-error retry only',
      cost: '$0.0038',
      scoreValue: null,
      threshold: 0.7,
      comparison: 'judge (>=) · provider errored twice — no score recorded',
      judgeExplanation: 'The judge provider returned 429 on both attempts. An errored task is reported as error, never as a pass or fail.',
      expected: ['tone >= 0.70'],
      actual: null,
      withheldReason: null,
      history: ['pass', 'pass', 'pass', 'pass', 'pass', 'pass', 'pass', 'pass', 'pass', 'pass'],
      traceLink: null
    }
  }
];

/** Fully drawn evaluation events addressable by record_id. */
export const evalDetails: Record<string, EvalDetail> = {
  eval_record_01: {
    recordId: 'eval_record_01',
    runId: 'run_88a1',
    eval: 'Checkout agent eval',
    evalCardHref: 'card_eval_01',
    subject: 'checkout-agent',
    subjectHref: 'card_service_04',
    workflow: { name: 'Checkout quality workflow', version: 'card_workflow_01 v7', started: '12:41:02Z' },
    status: { label: 'Failed', tone: 'danger' },
    duration: '12.4s',
    passed: 4,
    total: 6,
    origin: 'online',
    scenario: null,
    stages: [
      { name: '1 · load', state: 'completed', tone: 'ok' },
      { name: '2 · run', state: 'completed', tone: 'ok' },
      { name: '3 · judge', state: '1 failed', tone: 'danger' },
      { name: '4 · gate', state: 'running', tone: 'running' }
    ],
    tasks: evalTasks,
    selected: evalTasks[0]
  },
  eval_record_04: {
    recordId: 'eval_record_04',
    runId: 'run_local_2e',
    eval: 'Refund agent scenarios',
    evalCardHref: '',
    subject: 'local · unregistered',
    subjectHref: '',
    workflow: { name: 'Offline scenario run', version: 'collection customer-support-v2', started: '09:12:44Z' },
    status: { label: 'Passed', tone: 'ok' },
    duration: '8.2s',
    passed: 4,
    total: 4,
    origin: 'offline',
    scenario: {
      id: 'refund-partial-3',
      collection: 'customer-support-v2',
      initialQuery: 'I want a refund for two of the three items in order ord_5512.'
    },
    stages: [
      { name: '1 · drive', state: 'completed', tone: 'ok' },
      { name: '2 · score', state: 'completed', tone: 'ok' }
    ],
    tasks: [
      {
        name: 'outcome-match',
        method: 'judge',
        stage: 'stage 2',
        status: { label: 'Passed', tone: 'ok' },
        score: '0.92',
        detail: {
          type: 'scenario task',
          weight: '1.0',
          started: '09:12:47Z',
          duration: '1.1s',
          attempt: '1 of 1',
          cost: '$0.0032',
          scoreValue: 0.92,
          threshold: 0.8,
          comparison: 'judge (>=) · scored 0.92 vs >= 0.80 — passes',
          judgeExplanation: 'The response issues a partial refund for exactly the two requested items and confirms the remaining item ships.',
          expected: ['partial refund for 2 of 3 items', 'expected_outcome met'],
          actual: ['partial refund issued for 2 items', 'third item unchanged'],
          withheldReason: null,
          history: ['pass', 'pass', 'pass', 'pass', 'pass'],
          traceLink: null
        }
      },
      {
        name: 'turn-budget',
        method: '<=',
        stage: 'stage 1',
        status: { label: 'Passed', tone: 'ok' },
        score: '3',
        detail: {
          type: 'scenario task',
          weight: '0.5',
          started: '09:12:44Z',
          duration: '—',
          attempt: '1 of 1',
          cost: '—',
          scoreValue: 3,
          threshold: 8,
          comparison: '<= · 3 turns vs max_turns 8 — passes',
          judgeExplanation: null,
          expected: ['<= 8 turns'],
          actual: ['3 turns'],
          withheldReason: null,
          history: ['pass', 'pass', 'pass', 'pass', 'pass'],
          traceLink: null
        }
      }
    ],
    selected: null as unknown as EvalTask
  }
};
evalDetails.eval_record_04.selected = evalDetails.eval_record_04.tasks[0];

/** Per-feature results of the calculated 30d ranking drift report. */
export const driftFeatures: DriftFeature[] = [
  { name: 'score', method: 'psi', score: 0.31, threshold: 0.2, verdict: { label: 'Failed', tone: 'danger' } },
  { name: 'cart_value', method: 'psi', score: 0.08, threshold: 0.2, verdict: { label: 'Passed', tone: 'ok' } },
  { name: 'risk_band', method: 'psi', score: 0.12, threshold: 0.2, verdict: { label: 'Passed', tone: 'ok' } },
  { name: 'region', method: 'chi2', score: 0.19, threshold: 0.2, verdict: { label: 'Passed', tone: 'ok' } }
];

/** Daily psi history per feature over the 30d window. */
export const driftHistory: Record<string, number[]> = {
  score: [0.11, 0.1, 0.12, 0.11, 0.13, 0.12, 0.14, 0.15, 0.14, 0.16, 0.18, 0.21, 0.26, 0.31],
  cart_value: [0.06, 0.07, 0.06, 0.08, 0.07, 0.08, 0.07, 0.09, 0.08, 0.08, 0.07, 0.08, 0.08, 0.08],
  risk_band: [0.1, 0.11, 0.1, 0.12, 0.11, 0.12, 0.13, 0.12, 0.11, 0.12, 0.13, 0.12, 0.12, 0.12],
  region: [0.14, 0.15, 0.16, 0.15, 0.17, 0.16, 0.18, 0.17, 0.18, 0.19, 0.18, 0.19, 0.19, 0.19]
};

/** Axis stamps for the drift history chart. */
export const driftHistoryLabels = [
  { label: 'Aug 24', at: '2026-08-24T00:00:00Z' },
  { label: '', at: '2026-08-26T00:00:00Z' },
  { label: '', at: '2026-08-28T00:00:00Z' },
  { label: '', at: '2026-08-30T00:00:00Z' },
  { label: '', at: '2026-09-01T00:00:00Z' },
  { label: '', at: '2026-09-02T00:00:00Z' },
  { label: '', at: '2026-09-03T00:00:00Z' },
  { label: '', at: '2026-09-03T12:00:00Z' },
  { label: '', at: '2026-09-04T00:00:00Z' },
  { label: '', at: '2026-09-04T12:00:00Z' },
  { label: '', at: '2026-09-05T00:00:00Z' },
  { label: '', at: '2026-09-05T12:00:00Z' },
  { label: '', at: '2026-09-06T00:00:00Z' },
  { label: 'Sep 6', at: '2026-09-06T12:00:00Z' }
];
