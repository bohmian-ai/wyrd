import type {
  DashboardDetail,
  DashboardRow,
  DriftFeature,
  EvalDetail,
  EvalRow,
  EvalTask,
  GenAiRow,
  GenAiSpan,
  LogRecord,
  MetricInfo,
  Span,
  SpanEvent,
  SpanLink,
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

/**
 * The fixed observation clock every mock range anchors on. All generated and
 * curated timestamps sit at or before this instant so range filters, trends
 * and counts derive from the same truthful population.
 */
export const MOCK_NOW = Date.parse('2026-09-06T12:41:30Z');

/** Deterministic PRNG so the generated population is identical on every load. */
function mulberry32(seed: number): () => number {
  let a = seed;
  return () => {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Formats an epoch as the UTC `HH:MM:SS` stamp the tables show. */
function clock(at: number): string {
  return new Date(at).toISOString().slice(11, 19);
}

/** Formats an epoch as the UTC `HH:MM:SS.mmm` stamp log records show. */
function clockMs(at: number): string {
  return new Date(at).toISOString().slice(11, 23);
}

/** OTel severity_number for each level the fixtures emit. */
const SEVERITY: Record<string, number> = { info: 9, warn: 13, error: 17 };

/** Instrumentation scope each fixture service reports. */
const SCOPES: Record<string, { name: string; version: string }> = {
  'checkout-api': { name: 'wyrd.instrumentation.checkout', version: '1.4.2' },
  'ranking-api': { name: 'wyrd.instrumentation.ranking', version: '0.9.1' },
  'checkout-agent': { name: 'wyrd.instrumentation.agent', version: '2.1.0' },
  'ledger-api': { name: 'wyrd.instrumentation.ledger', version: '3.0.4' }
};

/** Seed for one log record; `sketchLog` fills the invariant OTel envelope. */
type LogSeed = {
  id: string;
  at: string;
  level: string;
  service: string;
  message: string;
  body?: unknown;
  eventName?: string | null;
  traceId?: string | null;
  spanId?: string | null;
  attributes?: Record<string, string | number | boolean>;
  droppedAttributesCount?: number;
};

/**
 * Expands a seed into a full OTel-shaped `LogRecord`: severity pair from the
 * level, observed_time trailing `time` by a small collector delay, scope from
 * the emitting service, sampled trace flags whenever a trace id exists.
 */
function sketchLog(seed: LogSeed): LogRecord {
  const scope = SCOPES[seed.service] ?? SCOPES['checkout-api'];
  const atMs = Date.parse(seed.at);
  return {
    id: seed.id,
    time: clockMs(atMs),
    at: seed.at,
    observedAt: new Date(atMs + 62).toISOString(),
    level: seed.level,
    severityNumber: SEVERITY[seed.level] ?? 9,
    severityText: seed.level.toUpperCase(),
    eventName: seed.eventName ?? null,
    service: seed.service,
    message: seed.message,
    body: seed.body ?? seed.message,
    traceId: seed.traceId ?? null,
    spanId: seed.spanId ?? null,
    traceFlags: seed.traceId ? 1 : 0,
    scopeName: scope.name,
    scopeVersion: scope.version,
    attributes: seed.attributes ?? {},
    droppedAttributesCount: seed.droppedAttributesCount ?? 0
  };
}

/** Log records behind the guided Logs search, newest first. */
export const logRecords: LogRecord[] = [
  sketchLog({
    id: 'log_01',
    at: '2026-09-06T12:41:07.221Z',
    level: 'error',
    service: 'checkout-api',
    message: 'capture declined: gateway timeout',
    eventName: 'checkout.capture.declined',
    traceId: 'trace_01',
    spanId: 'span_0c41',
    body: {
      message: 'capture declined: gateway timeout after 3 attempts',
      cart: { id: 'cart_88f1', items: 3, total: 84.1, risk_tier: 'high' },
      gateway: { upstream: 'ledger-api', status: 504, timeout_ms: 450 }
    },
    attributes: {
      'http.request.method': 'POST',
      'http.route': '/checkout/capture',
      'http.response.status_code': 504,
      'peer.service': 'ledger-api',
      'retry.count': 3,
      'retry.max': 3,
      'wyrd.principal': 'svc-checkout-api',
      'cart.id': 'cart_88f1',
      'cloud.region': 'us-east-1'
    }
  }),
  sketchLog({
    id: 'log_02',
    at: '2026-09-06T12:41:07.198Z',
    level: 'error',
    service: 'checkout-api',
    message: 'retry budget exhausted (3/3)',
    eventName: 'checkout.capture.retry_exhausted',
    traceId: 'trace_01',
    spanId: 'span_0c3e',
    body: 'retry budget exhausted after upstream 504s from ledger-api',
    attributes: { 'peer.service': 'ledger-api', 'retry.count': 3, 'retry.max': 3 }
  }),
  sketchLog({
    id: 'log_03',
    at: '2026-09-06T12:40:58.010Z',
    level: 'error',
    service: 'checkout-api',
    message: 'upstream 504 from ledger-api',
    eventName: 'checkout.capture.upstream_error',
    traceId: 'trace_04',
    spanId: 'span_1a02',
    body: 'upstream 504 from ledger-api during capture',
    attributes: {
      'http.response.status_code': 504,
      'peer.service': 'ledger-api',
      'cloud.region': 'us-east-1'
    }
  }),
  sketchLog({
    id: 'log_04',
    at: '2026-09-06T12:40:31.774Z',
    level: 'error',
    service: 'checkout-api',
    message: 'capture declined: gateway timeout',
    eventName: 'checkout.capture.declined',
    traceId: 'trace_07',
    spanId: 'span_ld07a',
    body: 'capture declined: gateway timeout after 3 attempts',
    attributes: {
      'http.response.status_code': 504,
      'peer.service': 'ledger-api',
      'retry.count': 3,
      'retry.max': 3
    }
  }),
  sketchLog({
    id: 'log_05',
    at: '2026-09-06T12:39:12.402Z',
    level: 'error',
    service: 'checkout-api',
    message: 'cart lock contention',
    eventName: 'checkout.cart.lock_contention',
    traceId: 'trace_09',
    spanId: 'span_lk09',
    body: 'cart lock contention while capture retries were in flight',
    attributes: { 'lock.key': 'cart_88d2', 'lock.waited_ms': 840 }
  }),
  sketchLog({
    id: 'log_06',
    at: '2026-09-06T12:38:44.120Z',
    level: 'warn',
    service: 'checkout-api',
    message: 'capture latency above 1.5s',
    eventName: 'checkout.capture.slow',
    traceId: 'trace_12',
    spanId: 'span_r12',
    body: 'capture latency above the 1.5s soft budget',
    attributes: { 'duration.ms': 1620, 'budget.soft_ms': 1500 }
  }),
  sketchLog({
    id: 'log_07',
    at: '2026-09-06T12:37:20.551Z',
    level: 'info',
    service: 'ranking-api',
    message: 'ranking model refreshed',
    eventName: 'ranking.model.refreshed',
    body: 'ranking model refreshed from the registry',
    attributes: { 'wyrd.card': 'rank-v9', 'model.version': 'v9' }
  })
];

/**
 * Leaf operation pool per service; padded child spans draw from it so every
 * trace detail carries exactly as many spans as its search row claims.
 */
const leafOps: Record<string, string[]> = {
  'checkout-api': ['db.get-cart', 'authz.check', 'cache.get-session', 'db.update-cart', 'emit.metrics', 'serialize.response'],
  'ranking-api': ['feature.load', 'db.get-history', 'model.forward', 'score.candidates', 'cache.put-features'],
  'checkout-agent': ['prompt.render', 'tool.plan', 'memory.read', 'guardrail.check'],
  'ledger-api': ['ledger.lookup-account', 'ledger.validate-entry', 'db.begin-txn']
};

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

/** Curated incident trace rows, newest first; each opens a drawn detail. */
export const traceRows: TraceRow[] = [
  { id: 'trace_01', at: '2026-09-06T12:41:07Z', rootOperation: 'POST /checkout/capture', service: 'checkout-api', start: '12:41:07', durationMs: 1840, spans: 42, errorSpans: 1, status: { label: 'Error', tone: 'danger' } },
  { id: 'trace_04', at: '2026-09-06T12:40:58Z', rootOperation: 'POST /checkout/capture', service: 'checkout-api', start: '12:40:58', durationMs: 1510, spans: 38, errorSpans: 1, status: { label: 'Error', tone: 'danger' } },
  { id: 'trace_07', at: '2026-09-06T12:40:31Z', rootOperation: 'POST /checkout/capture', service: 'checkout-api', start: '12:40:31', durationMs: 2100, spans: 44, errorSpans: 2, status: { label: 'Error', tone: 'danger' } },
  { id: 'trace_09', at: '2026-09-06T12:39:12Z', rootOperation: 'POST /cart/lock', service: 'checkout-api', start: '12:39:12', durationMs: 920, spans: 17, errorSpans: 1, status: { label: 'Error', tone: 'danger' } },
  { id: 'trace_12', at: '2026-09-06T12:38:44Z', rootOperation: 'POST /checkout/capture', service: 'checkout-api', start: '12:38:44', durationMs: 1620, spans: 40, errorSpans: 1, status: { label: 'Error', tone: 'danger' } },
  { id: 'trace_15', at: '2026-09-06T12:37:20Z', rootOperation: 'GET /cart/summary', service: 'checkout-api', start: '12:37:20', durationMs: 1120, spans: 21, errorSpans: 1, status: { label: 'Error', tone: 'danger' } },
  { id: 'trace_18', at: '2026-09-06T12:36:02Z', rootOperation: 'POST /checkout/capture', service: 'checkout-api', start: '12:36:02', durationMs: 1980, spans: 43, errorSpans: 3, status: { label: 'Error', tone: 'danger' } },
  { id: 'trace_21', at: '2026-09-06T12:34:47Z', rootOperation: 'POST /cart/lock', service: 'checkout-api', start: '12:34:47', durationMs: 1050, spans: 18, errorSpans: 1, status: { label: 'Error', tone: 'danger' } }
];

/** GenAI records of trace_01's capture conversation, joined onto its spans. */
const rankGenAi: GenAiSpan = {
  table: 'messages',
  provider: 'wyrd',
  operation: 'chat',
  requestModel: 'rank-v9',
  responseModel: 'rank-v9',
  conversationId: 'conv_cart_88f1',
  params: [
    ['temperature', '0'],
    ['max_tokens', '256']
  ],
  usage: { input: 412, output: 96, cacheRead: 0, cacheCreate: 0 },
  finishReasons: ['stop'],
  systemInstructions:
    'Rank capture candidates for the checkout decision. Return an ordered list with confidence.',
  inputMessages: [
    { role: 'user', content: 'Rank these 12 capture candidates for cart cart_88f1 (risk_tier=high).' }
  ],
  outputMessages: [
    { role: 'assistant', content: 'Ranked candidates: [c3, c1, c7, …] — top candidate c3 at 0.91 confidence.' }
  ],
  withheldReason: null,
  errorType: null
};

/** The checkout-agent decision call — an Anthropic chat with cache reads. */
const decideGenAi: GenAiSpan = {
  table: 'messages',
  provider: 'anthropic',
  operation: 'chat',
  requestModel: 'claude-sonnet-5',
  responseModel: 'claude-sonnet-5',
  conversationId: 'conv_cart_88f1',
  params: [
    ['temperature', '0.2'],
    ['max_tokens', '1024']
  ],
  usage: { input: 1834, output: 212, cacheRead: 1400, cacheCreate: 0 },
  finishReasons: ['end_turn'],
  systemInstructions:
    'You decide capture vs hold for checkout carts. Ground every decision in the ranked candidates and cart history you are given.',
  inputMessages: [
    { role: 'user', content: 'Decide capture vs hold for cart_88f1 given ranked candidates and history.' }
  ],
  outputMessages: [
    { role: 'assistant', content: 'capture — grounded on candidate c3 and 3 prior successful captures.' }
  ],
  withheldReason: null,
  errorType: null
};

/** The agent's failing ledger tool call — a `genai.tool_calls` record. */
const ledgerGenAi: GenAiSpan = {
  table: 'tool_calls',
  provider: 'anthropic',
  operation: 'execute_tool',
  toolName: 'ledger.capture',
  toolType: 'function',
  conversationId: 'conv_cart_88f1',
  args: '{"cart_id": "cart_88f1", "amount": 84.10, "currency": "USD"}',
  result: null,
  withheldReason: null,
  errorType: 'GatewayTimeout'
};

/** Same-shaped GenAI records for other capture traces, per conversation. */
const rankMessages = (conv: string): GenAiSpan => ({ ...rankGenAi, conversationId: conv });
/** The agent decision call for another capture conversation. */
const decideMessages = (conv: string): GenAiSpan => ({ ...decideGenAi, conversationId: conv });
/** The ledger tool call for another capture conversation, failed or settled. */
const ledgerToolCall = (conv: string, failed: boolean): GenAiSpan => ({
  ...ledgerGenAi,
  conversationId: conv,
  result: failed ? null : '{"status": "captured"}',
  errorType: failed ? 'GatewayTimeout' : null
});

/** The exception event a failing ledger call records, per `traces.events`. */
const ledgerExceptionEvent = (offset: string): SpanEvent => ({
  name: 'exception',
  offset,
  attributes: {
    'exception.type': 'GatewayTimeout',
    'exception.message': 'upstream 504 from ledger-api'
  },
  droppedAttributesCount: 0
});

/** Spans of trace_01 in start order; the waterfall scrolls, no row is dropped. */
const trace01Spans: Span[] = assignParents([
  sketchSpan({
    id: 'span_root',
    service: 'checkout-api',
    depth: 0,
    name: 'POST /checkout/capture',
    kind: 'server',
    startPct: 0,
    widthPct: 100,
    durationLabel: '1.84s',
    attributes: {
      'http.request.method': 'POST',
      'http.route': '/checkout/capture',
      'http.response.status_code': 504,
      'wyrd.principal': 'svc-checkout-api',
      'cloud.region': 'us-east-1'
    }
  }),
  sketchSpan({
    id: 'span_auth',
    service: 'checkout-api',
    depth: 1,
    name: 'authorize.principal',
    kind: 'internal',
    startPct: 2,
    widthPct: 8,
    durationLabel: '0.15s',
    attributes: { 'wyrd.principal': 'svc-checkout-api', 'authz.decision': 'allow' }
  }),
  sketchSpan({
    id: 'span_rank',
    service: 'ranking-api',
    depth: 1,
    name: 'rank.candidates',
    kind: 'llm',
    startPct: 12,
    widthPct: 34,
    durationLabel: '0.63s',
    attributes: {
      'gen_ai.operation.name': 'chat',
      'gen_ai.request.model': 'rank-v9',
      'candidates.count': 12
    },
    genai: rankGenAi
  }),
  sketchSpan({
    id: 'span_cart',
    service: 'ranking-api',
    depth: 2,
    name: 'retrieve.cart-history',
    kind: 'retrieval',
    startPct: 30,
    widthPct: 14,
    durationLabel: '0.26s',
    attributes: { 'retrieval.documents': 3, 'retrieval.store': 'cart-history' }
  }),
  sketchSpan({
    id: 'span_decide',
    service: 'checkout-agent',
    depth: 1,
    name: 'agent.decide-capture',
    kind: 'agent',
    startPct: 48,
    widthPct: 24,
    durationLabel: '0.44s',
    attributes: {
      'gen_ai.operation.name': 'chat',
      'gen_ai.request.model': 'claude-sonnet-5',
      'wyrd.decision': 'capture',
      'wyrd.confidence': 0.87
    },
    genai: decideGenAi
  }),
  sketchSpan({
    id: 'span_0c41',
    service: 'ledger-api',
    depth: 2,
    name: 'ledger.capture',
    kind: 'tool',
    startPct: 73,
    widthPct: 25,
    durationLabel: '0.46s',
    error: true,
    attributes: {
      'http.response.status_code': 504,
      'peer.service': 'ledger-api',
      'retry.count': 3,
      'rpc.method': 'ledger.capture'
    },
    events: [
      {
        name: 'retry',
        offset: '+0.15s',
        attributes: { 'retry.attempt': 2, 'retry.delay_ms': 50 },
        droppedAttributesCount: 0
      },
      {
        name: 'retry',
        offset: '+0.30s',
        attributes: { 'retry.attempt': 3, 'retry.delay_ms': 100 },
        droppedAttributesCount: 0
      },
      {
        name: 'exception',
        offset: '+0.44s',
        attributes: {
          'exception.type': 'GatewayTimeout',
          'exception.message': 'upstream 504 from ledger-api after 450ms',
          'exception.stacktrace':
            'GatewayTimeout: upstream 504 from ledger-api after 450ms\n  at LedgerClient.capture (ledger/client.rs:214)\n  at CaptureTool.execute (agent/tools.rs:88)'
        },
        droppedAttributesCount: 0
      }
    ],
    links: [
      {
        linkedTraceId: 'trace_04',
        linkedSpanId: 'span_1a02',
        traceState: null,
        attributes: { 'link.kind': 'same-incident' }
      }
    ],
    genai: ledgerGenAi
  })
]);

/** Seed for one sketched span; `sketchSpan` fills the invariant Span fields. */
type SpanSeed = {
  id: string;
  service: string;
  depth: number;
  name: string;
  kind: string;
  startPct: number;
  widthPct: number;
  durationLabel: string;
  error?: boolean;
  attributes?: Record<string, string | number | boolean>;
  events?: SpanEvent[];
  links?: SpanLink[];
  genai?: GenAiSpan | null;
};

/** Expands a seed into a full OTel-shaped Span with honest empty collections. */
function sketchSpan(seed: SpanSeed): Span {
  const scope = SCOPES[seed.service] ?? SCOPES['checkout-api'];
  return {
    id: seed.id,
    service: seed.service,
    depth: seed.depth,
    name: seed.name,
    kind: seed.kind,
    startPct: seed.startPct,
    widthPct: seed.widthPct,
    durationLabel: seed.durationLabel,
    error: seed.error ?? false,
    status: seed.error ? 'ERROR' : 'OK',
    parentSpanId: null,
    scopeName: scope.name,
    scopeVersion: scope.version,
    attributes: seed.attributes ?? {},
    droppedAttributesCount: 0,
    events: seed.events ?? [],
    links: seed.links ?? [],
    genai: seed.genai ?? null
  };
}

/**
 * Assigns each span's parent from waterfall order: the nearest earlier span
 * one depth level up — mirroring `parent_span_id` in `traces.spans` without
 * every seed having to restate the hierarchy the waterfall already draws.
 */
function assignParents(spans: Span[]): Span[] {
  const stack: Span[] = [];
  for (const span of spans) {
    while (stack.length && stack[stack.length - 1].depth >= span.depth) stack.pop();
    span.parentSpanId = stack.length ? stack[stack.length - 1].id : null;
    stack.push(span);
  }
  return spans;
}

/** Renders a millisecond duration the way span rows label it. */
function durLabel(ms: number): string {
  return ms >= 1000 ? `${(ms / 1000).toFixed(2)}s` : `${Math.max(1, Math.round(ms))}ms`;
}

/**
 * Pads a skeleton span list with plausible leaf children until the list holds
 * exactly `total` spans, so the header count, waterfall and flow all agree.
 * Children subdivide their parent's time window and sit directly after it in
 * start order; error spans are never padded so the failure narrative stays
 * exactly as authored.
 */
function padSpans(spans: Span[], total: number, durationMs: number): Span[] {
  const parents = spans.filter((span) => !span.error && span.widthPct >= 8);
  if (!parents.length || total <= spans.length) return spans;
  const per = new Map<Span, number>();
  for (let i = 0; i < total - spans.length; i++) {
    const parent = parents[i % parents.length];
    per.set(parent, (per.get(parent) ?? 0) + 1);
  }
  const result: Span[] = [];
  for (const span of spans) {
    result.push(span);
    const n = per.get(span) ?? 0;
    const ops = leafOps[span.service] ?? leafOps['checkout-api'];
    for (let k = 0; k < n; k++) {
      const slot = span.widthPct / n;
      const widthPct = Math.max(slot * 0.55, 0.4);
      result.push(
        sketchSpan({
          id: `${span.id}_c${k}`,
          service: span.service,
          depth: span.depth + 1,
          name: ops[k % ops.length],
          kind: 'internal',
          startPct: Math.min(span.startPct + slot * k + slot * 0.1, 98),
          widthPct,
          durationLabel: durLabel((durationMs * widthPct) / 100)
        })
      );
    }
  }
  return result;
}

/**
 * Builds a fully-drawn trace detail from a search row and span seeds, padded
 * so the drawn span count equals the row's advertised count — the search row,
 * detail header, waterfall and flow never disagree.
 */
function sketchTrace(
  row: TraceRow,
  startMs: string,
  seeds: SpanSeed[],
  graph: TraceDetail['graph']
): TraceDetail {
  const spans = assignParents(padSpans(seeds.map(sketchSpan), row.spans, row.durationMs));
  return {
    id: row.id,
    rootOperation: row.rootOperation,
    service: row.service,
    start: startMs,
    duration: `${(row.durationMs / 1000).toFixed(2)}s`,
    spanCount: spans.length,
    errorCount: spans.filter((span) => span.error).length,
    spans,
    selected: spans.find((span) => span.error) ?? spans[0],
    graph
  };
}

/** The checkout capture path; the ledger edge fails only on error traces. */
function captureGraphWith(error: boolean): TraceDetail['graph'] {
  return {
    nodes: [
      { name: 'checkout-api', cardHref: 'card_service_01' },
      { name: 'ranking-api', cardHref: 'card_service_03' },
      { name: 'checkout-agent', cardHref: 'card_service_04' },
      { name: 'ledger-api', cardHref: 'card_service_02' }
    ],
    edges: [
      { from: 'checkout-api', to: 'ranking-api', error: false, label: '' },
      { from: 'ranking-api', to: 'checkout-agent', error: false, label: '' },
      { from: 'checkout-agent', to: 'ledger-api', error, label: error ? '504' : '' }
    ]
  };
}

/** The failing capture path shared by every curated incident trace. */
const captureGraph = captureGraphWith(true);

/** Cart lock traces never leave checkout-api. */
const lockGraph: TraceDetail['graph'] = {
  nodes: [{ name: 'checkout-api', cardHref: 'card_service_01' }],
  edges: []
};

/** Trace details addressable by id; trace_01 is fully drawn, the rest sketched. */
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
  },
  trace_04: sketchTrace(
    traceRows[1],
    '12:40:58.010',
    [
      { id: 'span_r04', service: 'checkout-api', depth: 0, name: 'POST /checkout/capture', kind: 'server', startPct: 0, widthPct: 100, durationLabel: '1.51s', attributes: { 'http.request.method': 'POST', 'http.route': '/checkout/capture', 'http.response.status_code': 504 } },
      { id: 'span_rk04', service: 'ranking-api', depth: 1, name: 'rank.candidates', kind: 'llm', startPct: 10, widthPct: 30, durationLabel: '0.45s', attributes: { 'gen_ai.operation.name': 'chat', 'gen_ai.request.model': 'rank-v9' }, genai: rankMessages('conv_cart_91aa') },
      { id: 'span_ag04', service: 'checkout-agent', depth: 1, name: 'agent.decide-capture', kind: 'agent', startPct: 45, widthPct: 22, durationLabel: '0.33s', attributes: { 'gen_ai.operation.name': 'chat', 'gen_ai.request.model': 'claude-sonnet-5' }, genai: decideMessages('conv_cart_91aa') },
      { id: 'span_1a02', service: 'ledger-api', depth: 2, name: 'ledger.capture', kind: 'tool', startPct: 70, widthPct: 28, durationLabel: '0.42s', error: true, attributes: { 'http.response.status_code': 504, 'peer.service': 'ledger-api', 'retry.count': 3 }, events: [ledgerExceptionEvent('+0.40s')], genai: ledgerToolCall('conv_cart_91aa', true) }
    ],
    captureGraph
  ),
  trace_07: sketchTrace(
    traceRows[2],
    '12:40:31.774',
    [
      { id: 'span_r07', service: 'checkout-api', depth: 0, name: 'POST /checkout/capture', kind: 'server', startPct: 0, widthPct: 100, durationLabel: '2.10s' },
      { id: 'span_rk07', service: 'ranking-api', depth: 1, name: 'rank.candidates', kind: 'llm', startPct: 8, widthPct: 32, durationLabel: '0.67s', attributes: { 'gen_ai.operation.name': 'chat', 'gen_ai.request.model': 'rank-v9' }, genai: rankMessages('conv_cart_93b0') },
      { id: 'span_ag07', service: 'checkout-agent', depth: 1, name: 'agent.decide-capture', kind: 'agent', startPct: 44, widthPct: 20, durationLabel: '0.42s', attributes: { 'gen_ai.operation.name': 'chat', 'gen_ai.request.model': 'claude-sonnet-5' }, genai: decideMessages('conv_cart_93b0') },
      { id: 'span_ld07a', service: 'ledger-api', depth: 2, name: 'ledger.capture', kind: 'tool', startPct: 66, widthPct: 15, durationLabel: '0.32s', error: true, attributes: { 'http.response.status_code': 504, 'peer.service': 'ledger-api' }, events: [ledgerExceptionEvent('+0.30s')], genai: ledgerToolCall('conv_cart_93b0', true) },
      { id: 'span_ld07b', service: 'ledger-api', depth: 2, name: 'ledger.capture · retry', kind: 'tool', startPct: 83, widthPct: 15, durationLabel: '0.31s', error: true, attributes: { 'http.response.status_code': 504, 'peer.service': 'ledger-api', 'retry.count': 2 }, events: [ledgerExceptionEvent('+0.29s')], genai: ledgerToolCall('conv_cart_93b0', true) }
    ],
    captureGraph
  ),
  trace_09: sketchTrace(
    traceRows[3],
    '12:39:12.402',
    [
      { id: 'span_r09', service: 'checkout-api', depth: 0, name: 'POST /cart/lock', kind: 'server', startPct: 0, widthPct: 100, durationLabel: '0.92s' },
      { id: 'span_lk09', service: 'checkout-api', depth: 1, name: 'acquire.cart-lock', kind: 'internal', startPct: 4, widthPct: 92, durationLabel: '0.84s', error: true, attributes: { 'lock.key': 'cart_88d2', 'lock.waited_ms': 840 } }
    ],
    lockGraph
  ),
  trace_12: sketchTrace(
    traceRows[4],
    '12:38:44.120',
    [
      { id: 'span_r12', service: 'checkout-api', depth: 0, name: 'POST /checkout/capture', kind: 'server', startPct: 0, widthPct: 100, durationLabel: '1.62s' },
      { id: 'span_rk12', service: 'ranking-api', depth: 1, name: 'rank.candidates', kind: 'llm', startPct: 9, widthPct: 33, durationLabel: '0.53s', attributes: { 'gen_ai.operation.name': 'chat', 'gen_ai.request.model': 'rank-v9' }, genai: rankMessages('conv_cart_95c2') },
      { id: 'span_ag12', service: 'checkout-agent', depth: 1, name: 'agent.decide-capture', kind: 'agent', startPct: 46, widthPct: 24, durationLabel: '0.39s', attributes: { 'gen_ai.operation.name': 'chat', 'gen_ai.request.model': 'claude-sonnet-5' }, genai: decideMessages('conv_cart_95c2') },
      { id: 'span_ld12', service: 'ledger-api', depth: 2, name: 'ledger.capture', kind: 'tool', startPct: 73, widthPct: 25, durationLabel: '0.41s', error: true, attributes: { 'http.response.status_code': 504, 'peer.service': 'ledger-api' }, events: [ledgerExceptionEvent('+0.39s')], genai: ledgerToolCall('conv_cart_95c2', true) }
    ],
    captureGraph
  ),
  trace_15: sketchTrace(
    traceRows[5],
    '12:37:20.551',
    [
      { id: 'span_r15', service: 'checkout-api', depth: 0, name: 'GET /cart/summary', kind: 'server', startPct: 0, widthPct: 100, durationLabel: '1.12s' },
      { id: 'span_rt15', service: 'ranking-api', depth: 1, name: 'retrieve.cart-history', kind: 'retrieval', startPct: 12, widthPct: 80, durationLabel: '0.90s', error: true, attributes: { 'retrieval.documents': 0, 'retrieval.store': 'cart-history' }, events: [{ name: 'exception', offset: '+0.88s', attributes: { 'exception.type': 'RetrievalTimeout', 'exception.message': 'cart-history retrieval timed out after 880ms' }, droppedAttributesCount: 0 }] }
    ],
    {
      nodes: [
        { name: 'checkout-api', cardHref: 'card_service_01' },
        { name: 'ranking-api', cardHref: 'card_service_03' }
      ],
      edges: [{ from: 'checkout-api', to: 'ranking-api', error: true, label: 'timeout' }]
    }
  ),
  trace_18: sketchTrace(
    traceRows[6],
    '12:36:02.330',
    [
      { id: 'span_r18', service: 'checkout-api', depth: 0, name: 'POST /checkout/capture', kind: 'server', startPct: 0, widthPct: 100, durationLabel: '1.98s' },
      { id: 'span_rk18', service: 'ranking-api', depth: 1, name: 'rank.candidates', kind: 'llm', startPct: 7, widthPct: 30, durationLabel: '0.59s', attributes: { 'gen_ai.operation.name': 'chat', 'gen_ai.request.model': 'rank-v9' }, genai: rankMessages('conv_cart_97d4') },
      { id: 'span_ag18', service: 'checkout-agent', depth: 1, name: 'agent.decide-capture', kind: 'agent', startPct: 40, widthPct: 18, durationLabel: '0.36s', attributes: { 'gen_ai.operation.name': 'chat', 'gen_ai.request.model': 'claude-sonnet-5' }, genai: decideMessages('conv_cart_97d4') },
      { id: 'span_ld18a', service: 'ledger-api', depth: 2, name: 'ledger.capture', kind: 'tool', startPct: 60, widthPct: 12, durationLabel: '0.24s', error: true, attributes: { 'http.response.status_code': 504, 'peer.service': 'ledger-api' }, events: [ledgerExceptionEvent('+0.22s')], genai: ledgerToolCall('conv_cart_97d4', true) },
      { id: 'span_ld18b', service: 'ledger-api', depth: 2, name: 'ledger.capture · retry', kind: 'tool', startPct: 74, widthPct: 12, durationLabel: '0.24s', error: true, attributes: { 'http.response.status_code': 504, 'peer.service': 'ledger-api', 'retry.count': 2 }, events: [ledgerExceptionEvent('+0.22s')], genai: ledgerToolCall('conv_cart_97d4', true) },
      { id: 'span_ld18c', service: 'ledger-api', depth: 2, name: 'ledger.capture · retry', kind: 'tool', startPct: 87, widthPct: 12, durationLabel: '0.24s', error: true, attributes: { 'http.response.status_code': 504, 'peer.service': 'ledger-api', 'retry.count': 3 }, events: [ledgerExceptionEvent('+0.22s')], genai: ledgerToolCall('conv_cart_97d4', true) }
    ],
    captureGraph
  ),
  trace_21: sketchTrace(
    traceRows[7],
    '12:34:47.902',
    [
      { id: 'span_r21', service: 'checkout-api', depth: 0, name: 'POST /cart/lock', kind: 'server', startPct: 0, widthPct: 100, durationLabel: '1.05s' },
      { id: 'span_lk21', service: 'checkout-api', depth: 1, name: 'acquire.cart-lock', kind: 'internal', startPct: 3, widthPct: 94, durationLabel: '0.99s', error: true, attributes: { 'lock.key': 'cart_91c0', 'lock.waited_ms': 960 } }
    ],
    lockGraph
  )
};

// The hand-drawn trace_01 pads to its advertised 42 spans like every other
// detail, so its header, waterfall and search row agree.
traceDetails.trace_01.spans = assignParents(padSpans(trace01Spans, 42, 1840));

/**
 * Draws the detail for any trace row without a curated entry. Deterministic —
 * the same row always sketches the same spans — so every generated row in the
 * search opens a real, consistent waterfall and flow instead of a 404.
 */
export function sketchFromRow(row: TraceRow): TraceDetail {
  const failed = row.status.tone === 'danger';
  const start = `${row.start}.000`;
  if (row.rootOperation === 'POST /cart/lock')
    return sketchTrace(
      row,
      start,
      [
        { id: `${row.id}_root`, service: 'checkout-api', depth: 0, name: row.rootOperation, kind: 'server', startPct: 0, widthPct: 100, durationLabel: durLabel(row.durationMs) },
        { id: `${row.id}_lock`, service: 'checkout-api', depth: 1, name: 'acquire.cart-lock', kind: 'internal', startPct: 4, widthPct: 92, durationLabel: durLabel(row.durationMs * 0.9), error: failed, attributes: failed ? { 'lock.key': 'cart_contended', 'lock.waited_ms': Math.round(row.durationMs * 0.9) } : {} }
      ],
      lockGraph
    );
  if (row.rootOperation === 'GET /cart/summary')
    return sketchTrace(
      row,
      start,
      [
        { id: `${row.id}_root`, service: 'checkout-api', depth: 0, name: row.rootOperation, kind: 'server', startPct: 0, widthPct: 100, durationLabel: durLabel(row.durationMs) },
        { id: `${row.id}_ret`, service: 'ranking-api', depth: 1, name: 'retrieve.cart-history', kind: 'retrieval', startPct: 12, widthPct: 78, durationLabel: durLabel(row.durationMs * 0.78), error: failed, attributes: failed ? { 'retrieval.store': 'cart-history' } : { 'retrieval.documents': 3, 'retrieval.store': 'cart-history' }, events: failed ? [{ name: 'exception', offset: durLabel(row.durationMs * 0.78), attributes: { 'exception.type': 'RetrievalTimeout', 'exception.message': 'cart-history retrieval timed out' }, droppedAttributesCount: 0 }] : [] }
      ],
      {
        nodes: [
          { name: 'checkout-api', cardHref: 'card_service_01' },
          { name: 'ranking-api', cardHref: 'card_service_03' }
        ],
        edges: [{ from: 'checkout-api', to: 'ranking-api', error: failed, label: failed ? 'timeout' : '' }]
      }
    );
  if (row.rootOperation === 'POST /rank/candidates')
    return sketchTrace(
      row,
      start,
      [
        { id: `${row.id}_root`, service: 'ranking-api', depth: 0, name: row.rootOperation, kind: 'server', startPct: 0, widthPct: 100, durationLabel: durLabel(row.durationMs), attributes: { 'http.request.method': 'POST', 'http.route': '/rank/candidates', 'http.response.status_code': 200 } },
        { id: `${row.id}_rank`, service: 'ranking-api', depth: 1, name: 'rank.candidates', kind: 'llm', startPct: 8, widthPct: 84, durationLabel: durLabel(row.durationMs * 0.84), attributes: { 'gen_ai.operation.name': 'chat', 'gen_ai.request.model': 'rank-v9' }, genai: rankMessages(`conv_${row.id}`) }
      ],
      { nodes: [{ name: 'ranking-api', cardHref: 'card_service_03' }], edges: [] }
    );
  const retries = Math.max(0, row.errorSpans - 1);
  return sketchTrace(
    row,
    start,
    [
      { id: `${row.id}_root`, service: 'checkout-api', depth: 0, name: 'POST /checkout/capture', kind: 'server', startPct: 0, widthPct: 100, durationLabel: durLabel(row.durationMs), attributes: { 'http.request.method': 'POST', 'http.route': '/checkout/capture', 'http.response.status_code': failed ? 504 : 200 } },
      { id: `${row.id}_rank`, service: 'ranking-api', depth: 1, name: 'rank.candidates', kind: 'llm', startPct: 9, widthPct: 32, durationLabel: durLabel(row.durationMs * 0.32), attributes: { 'gen_ai.operation.name': 'chat', 'gen_ai.request.model': 'rank-v9' }, genai: rankMessages(`conv_${row.id}`) },
      { id: `${row.id}_agent`, service: 'checkout-agent', depth: 1, name: 'agent.decide-capture', kind: 'agent', startPct: 45, widthPct: 22, durationLabel: durLabel(row.durationMs * 0.22), attributes: { 'gen_ai.operation.name': 'chat', 'gen_ai.request.model': 'claude-sonnet-5' }, genai: decideMessages(`conv_${row.id}`) },
      {
        id: `${row.id}_ledger`,
        service: 'ledger-api',
        depth: 2,
        name: 'ledger.capture',
        kind: 'tool',
        startPct: 71,
        widthPct: retries ? 12 : 26,
        durationLabel: durLabel(row.durationMs * (retries ? 0.12 : 0.26)),
        error: failed,
        attributes: { 'http.response.status_code': failed ? 504 : 200, 'peer.service': 'ledger-api' },
        events: failed ? [ledgerExceptionEvent(`+${durLabel(row.durationMs * 0.1)}`)] : [],
        genai: ledgerToolCall(`conv_${row.id}`, failed)
      },
      ...Array.from({ length: retries }, (_, i): SpanSeed => ({
        id: `${row.id}_retry${i}`,
        service: 'ledger-api',
        depth: 2,
        name: 'ledger.capture · retry',
        kind: 'tool',
        startPct: 85 + i * 7,
        widthPct: 6,
        durationLabel: durLabel(row.durationMs * 0.06),
        error: true,
        attributes: { 'http.response.status_code': 504, 'peer.service': 'ledger-api', 'retry.count': i + 2 },
        events: [ledgerExceptionEvent(`+${durLabel(row.durationMs * 0.05)}`)],
        genai: ledgerToolCall(`conv_${row.id}`, true)
      }))
    ],
    captureGraphWith(failed)
  );
}

/**
 * Extracted GenAI call rows behind the Observe → GenAI search — one row per
 * `genai.messages` / `genai.tool_calls` record. Curated rows carry trace_01's
 * conversation; the generated population adds steady ranking calls and the
 * incident's failing tool calls below.
 */
export const genaiRows: GenAiRow[] = [
  { id: 'span_rank', at: '2026-09-06T12:41:07.421Z', start: '12:41:07', table: 'messages', service: 'ranking-api', provider: 'wyrd', operation: 'chat', model: 'rank-v9', conversationId: 'conv_cart_88f1', tokensIn: 412, tokensOut: 96, durationMs: 630, status: { label: 'Ok', tone: 'ok' }, traceId: 'trace_01' },
  { id: 'span_decide', at: '2026-09-06T12:41:08.082Z', start: '12:41:08', table: 'messages', service: 'checkout-agent', provider: 'anthropic', operation: 'chat', model: 'claude-sonnet-5', conversationId: 'conv_cart_88f1', tokensIn: 1834, tokensOut: 212, durationMs: 440, status: { label: 'Ok', tone: 'ok' }, traceId: 'trace_01' },
  { id: 'span_0c41', at: '2026-09-06T12:41:08.541Z', start: '12:41:08', table: 'tool_calls', service: 'checkout-agent', provider: 'anthropic', operation: 'execute_tool', model: 'ledger.capture', conversationId: 'conv_cart_88f1', tokensIn: null, tokensOut: null, durationMs: 460, status: { label: 'Error', tone: 'danger' }, traceId: 'trace_01' }
];

// ---------------------------------------------------------------------------
// Generated 24h population. Every count, trend and facet the Observe pages
// show derives from these records — no number is asserted that the data does
// not contain. The generation is seeded, so it is identical on every load.
// ---------------------------------------------------------------------------

{
  const rand = mulberry32(0x5eed);
  const pick = <T>(pool: T[]): T => pool[Math.floor(rand() * pool.length)];
  const steadyOps = [
    { op: 'POST /checkout/capture', service: 'checkout-api', spans: () => 34 + Math.floor(rand() * 10) },
    { op: 'POST /cart/lock', service: 'checkout-api', spans: () => 15 + Math.floor(rand() * 5) },
    { op: 'GET /cart/summary', service: 'checkout-api', spans: () => 19 + Math.floor(rand() * 4) },
    { op: 'POST /rank/candidates', service: 'ranking-api', spans: () => 11 + Math.floor(rand() * 6) }
  ];
  const infoMessages = [
    ['checkout-api', 'capture completed'],
    ['checkout-api', 'cart summary served'],
    ['checkout-api', 'capture settled with ledger'],
    ['ranking-api', 'candidates ranked'],
    ['ranking-api', 'feature store refreshed']
  ];
  const errorMessages = [
    'upstream 504 from ledger-api',
    'capture declined: ledger unavailable',
    'retry budget exhausted (3/3)'
  ];
  let traceSeq = 0;
  let logSeq = 0;
  // Steady 24h traffic at search-page volume: six ok traces and twenty info
  // logs per minute, so every range holds far more rows than one page and the
  // pagination / load-more paths are exercised against real data.
  for (let minute = 24 * 60 - 1; minute >= 1; minute--) {
    const base = MOCK_NOW - minute * 60_000;
    for (let tIndex = 0; tIndex < 6; tIndex++) {
      const template = pick(steadyOps);
      const at = base + Math.floor(rand() * 55_000);
      const traceId = `trace_g${++traceSeq}`;
      const durationMs = 250 + Math.floor(rand() * 650);
      traceRows.push({
        id: traceId,
        at: new Date(at).toISOString(),
        rootOperation: template.op,
        service: template.service,
        start: clock(at),
        durationMs,
        spans: template.spans(),
        errorSpans: 0,
        status: { label: 'Ok', tone: 'ok' }
      });
      // Every ranking trace carries one extracted genai.messages record —
      // the steady volume behind the Observe → GenAI search.
      if (template.op === 'POST /rank/candidates')
        genaiRows.push({
          id: `${traceId}_rank`,
          at: new Date(at).toISOString(),
          start: clock(at),
          table: 'messages',
          service: 'ranking-api',
          provider: 'wyrd',
          operation: 'chat',
          model: 'rank-v9',
          conversationId: `conv_${traceId}`,
          tokensIn: 360 + Math.floor(rand() * 120),
          tokensOut: 70 + Math.floor(rand() * 50),
          durationMs: Math.round(durationMs * 0.84),
          status: { label: 'Ok', tone: 'ok' },
          traceId
        });
    }
    for (let i = 0; i < 20; i++) {
      const [service, message] = pick(infoMessages);
      const logAt = base + Math.floor(rand() * 55_000);
      const slow = minute % 9 === 0 && i === 0;
      logSeq += 1;
      logRecords.push(
        sketchLog({
          id: `log_g${logSeq}`,
          at: new Date(logAt).toISOString(),
          level: slow ? 'warn' : 'info',
          service,
          message: slow ? 'capture latency above 1.0s' : message,
          body: slow ? 'capture latency above the 1.0s soft budget' : message,
          attributes: slow
            ? { 'duration.ms': 1000 + Math.floor(rand() * 600), 'budget.soft_ms': 1000 }
            : { 'cloud.region': rand() < 0.7 ? 'us-east-1' : 'eu-west-1' }
        })
      );
    }
  }
  // The 12:15–12:41 incident: error capture traces ramping up, each emitting
  // correlated error logs that carry the failing trace id.
  for (let minute = 26; minute >= 1; minute--) {
    const base = MOCK_NOW - minute * 60_000;
    const burst = 2 + Math.floor(((26 - minute) / 26) * 5) + Math.floor(rand() * 2);
    for (let b = 0; b < burst; b++) {
      const at = base + Math.floor(rand() * 55_000);
      const errorSpans = 1 + Math.floor(rand() * 3);
      const traceId = `trace_e${++traceSeq}`;
      traceRows.push({
        id: traceId,
        at: new Date(at).toISOString(),
        rootOperation: 'POST /checkout/capture',
        service: 'checkout-api',
        start: clock(at),
        durationMs: 1200 + Math.floor(rand() * 1000),
        spans: 34 + Math.floor(rand() * 10),
        errorSpans,
        status: { label: 'Error', tone: 'danger' }
      });
      // Each incident trace's extracted GenAI records: the agent decision
      // chat succeeds, its ledger tool call fails.
      genaiRows.push(
        {
          id: `${traceId}_agent`,
          at: new Date(at).toISOString(),
          start: clock(at),
          table: 'messages',
          service: 'checkout-agent',
          provider: 'anthropic',
          operation: 'chat',
          model: 'claude-sonnet-5',
          conversationId: `conv_${traceId}`,
          tokensIn: 1600 + Math.floor(rand() * 500),
          tokensOut: 150 + Math.floor(rand() * 120),
          durationMs: 300 + Math.floor(rand() * 250),
          status: { label: 'Ok', tone: 'ok' },
          traceId
        },
        {
          id: `${traceId}_ledger`,
          at: new Date(at + 500).toISOString(),
          start: clock(at + 500),
          table: 'tool_calls',
          service: 'checkout-agent',
          provider: 'anthropic',
          operation: 'execute_tool',
          model: 'ledger.capture',
          conversationId: `conv_${traceId}`,
          tokensIn: null,
          tokensOut: null,
          durationMs: 350 + Math.floor(rand() * 200),
          status: { label: 'Error', tone: 'danger' },
          traceId
        }
      );
      for (let e = 0; e < 3; e++) {
        const message = errorMessages[e % errorMessages.length];
        logSeq += 1;
        logRecords.push(
          sketchLog({
            id: `log_g${logSeq}`,
            at: new Date(at + e * 40).toISOString(),
            level: 'error',
            service: 'checkout-api',
            message,
            eventName: 'checkout.capture.upstream_error',
            traceId,
            spanId: `${traceId}_ledger`,
            body: message,
            attributes: {
              'http.response.status_code': 504,
              'peer.service': 'ledger-api',
              'retry.count': e + 1,
              'cloud.region': 'us-east-1'
            }
          })
        );
      }
    }
  }
  const newestFirst = (a: { at: string | null }, b: { at: string | null }) =>
    Date.parse(b.at ?? '') - Date.parse(a.at ?? '');
  logRecords.sort(newestFirst);
  traceRows.sort(newestFirst);
  genaiRows.sort(newestFirst);
}

/** Read-only dashboard inventory. */
export const dashboardRows: DashboardRow[] = [
  { id: 'dashboard_01', name: 'Checkout health', folder: 'payments', tags: ['checkout', 'slo'], owner: 'j.reyes', updated: '2h ago' },
  { id: 'dashboard_02', name: 'Ledger capture', folder: 'payments', tags: ['ledger'], owner: 'j.reyes', updated: '1d ago' },
  { id: 'dashboard_03', name: 'Ranking quality', folder: 'ml', tags: ['ranking', 'drift'], owner: 'm.linden', updated: '2d ago' },
  { id: 'dashboard_04', name: 'Agent decisions', folder: 'ml', tags: ['agent', 'eval'], owner: 'm.linden', updated: '3d ago' },
  { id: 'dashboard_05', name: 'Cost and spend', folder: 'platform', tags: ['cost'], owner: 'r.okafor', updated: '5d ago' },
  { id: 'dashboard_06', name: 'Platform SLOs', folder: 'platform', tags: ['slo'], owner: 'j.reyes', updated: '1w ago' }
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
