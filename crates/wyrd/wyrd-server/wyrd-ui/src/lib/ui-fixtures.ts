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
export const card = { kind: 'Model', name: 'ranker', version: '1.4.2' } as const satisfies CardRef;
export const registered = {
  apiVersion: 'wyrd/v1',
  kind: 'Model',
  metadata: { name: 'ranker', version: '1.4.2', uid: 'card_01HX' },
  spec: {}
} as unknown as RegisteredCard;
export const summary = (version: string, status = 'active', kind = 'Model', name = 'ranker') =>
  ({
    version,
    status,
    name,
    kind,
    card_uid: `card_${kind}_${name}_${version}`,
    spec_hash: 'sha256:0f3a',
    artifact_hash: null,
    created_at: '2026-10-19T14:02:00Z',
    updated_at: '2026-10-20T09:15:00Z'
  }) as never;

/** Two versions of one portable Prompt, an Agent using the newer one, and a native-format Prompt keeping an extension field. */
export const PROMPT = { kind: 'Prompt', name: 'support-reply', version: '1.1.0' } as const satisfies CardRef;
export const AGENT = { kind: 'Agent', name: 'support', version: '1.0.0' } as const satisfies CardRef;
export const SERVICE = { kind: 'Service', name: 'support-api', version: '1.0.0' } as const satisfies CardRef;
export const NATIVE = { kind: 'Prompt', name: 'native-reply', version: '1.0.0' } as const satisfies CardRef;
const promptCard = (version: string, spec: object, variables: string[], inbound: CardRef[]) =>
  ({
    apiVersion: 'wyrd/v1',
    kind: 'Prompt',
    metadata: { name: PROMPT.name, version, uid: `card_prompt_${version}`, labels: { team: 'support' }, spec_hash: `sha256:${version}` },
    spec,
    relationships: { inbound_refs: inbound.map((ref) => ({ ref })), outbound_refs: [] },
    status: { phase: 'active', prompt: { variables, media_variables: [] } }
  }) as unknown as RegisteredCard;
export const prompts: Record<string, RegisteredCard> = {
  '1.0.0': promptCard('1.0.0', { provider: 'openai', model: 'gpt-5', system: 'You answer support tickets.', messages: [{ role: 'user', content: 'Ticket: {{ticket}}' }] }, ['ticket'], []),
  '1.1.0': promptCard(
    '1.1.0',
    {
      provider: 'openai',
      model: 'gpt-5',
      system: 'You answer support tickets.\nCite the policy section you rely on.',
      messages: [{ role: 'user', content: 'Ticket: {{ticket}}\nTone: {{tone}}' }],
      response_format: 'verdict'
    },
    ['ticket', 'tone'],
    [AGENT]
  )
};
export const nativePrompt = {
  apiVersion: 'wyrd/v1',
  kind: 'Prompt',
  metadata: { name: NATIVE.name, version: NATIVE.version, uid: 'card_native' },
  spec: { provider: 'openai', model: 'gpt-5', format: 'responses', body: { input: [{ role: 'user', content: 'Ticket: {{ticket}}' }], future_knob: true } },
  relationships: { inbound_refs: [], outbound_refs: [] },
  status: { phase: 'active', prompt: { variables: ['ticket'], media_variables: [] } }
} as unknown as RegisteredCard;
export const agent = {
  apiVersion: 'wyrd/v1',
  kind: 'Agent',
  metadata: { name: AGENT.name, version: AGENT.version, uid: 'card_agent' },
  spec: { execution_route: { kind: 'wyrd_gateway', deployment: 'primary' }, prompt: PROMPT },
  relationships: { outbound_refs: [{ ref: PROMPT }], inbound_refs: [{ ref: SERVICE }] },
  status: { phase: 'active' }
} as unknown as RegisteredCard;
/**
 * Verifiers as registered: an ordinary one with parallel steps, a fan-in, a gated and retried step and both
 * requirement positions, in two versions; a session judgment over group sources; and a drift check.
 */
export const VERIFIER = { kind: 'Verifier', name: 'ticket-policy', version: '1.1.0' } as const satisfies CardRef;
export const SESSION_VERIFIER = { kind: 'Verifier', name: 'session-quality', version: '1.0.0' } as const satisfies CardRef;
export const DRIFT_VERIFIER = { kind: 'Verifier', name: 'latency-drift', version: '1.0.0' } as const satisfies CardRef;
const verifierCard = (ref: CardRef, spec: object, inbound: CardRef[] = []) =>
  ({
    apiVersion: 'wyrd/v1',
    kind: 'Verifier',
    metadata: { name: ref.name, version: ref.version, uid: `card_verifier_${ref.name}_${ref.version}` },
    spec,
    relationships: { inbound_refs: inbound.map((each) => ({ ref: each })), outbound_refs: [] },
    status: { phase: 'active' }
  }) as unknown as RegisteredCard;
const hasAnswer = { id: 'has-answer', statement: 'The ticket carries an answer', check: { kind: 'assert', path: 'inputs.ticket.answer', operator: 'is_non_empty' } };
export const verifiers: Record<string, RegisteredCard> = {
  'ticket-policy@1.0.0': verifierCard({ ...VERIFIER, version: '1.0.0' }, { description: 'A support ticket carries an answer.', inputs: { ticket: { type: 'json' } }, requirements: [hasAnswer] }),
  'ticket-policy@1.1.0': verifierCard(
    VERIFIER,
    {
      description: 'A support ticket follows the refund policy.',
      inputs: { ticket: { type: 'json', schema: { type: 'object' } }, photo: { type: 'media', formats: ['png', 'jpeg'] } },
      steps: [
        { id: 'read-photo', action: { kind: 'extract', from: 'inputs.photo' } },
        { id: 'score', action: { kind: 'code', entrypoint: 'score', inputs: { ticket: 'inputs.ticket' }, source: { inline: 'def score(ticket): ...' } }, retries: 2, timeout_seconds: 30 },
        {
          id: 'classify',
          action: { kind: 'agent', agent: AGENT, inputs: { ticket: 'inputs.ticket', photo_text: 'steps.read-photo.output.text' } },
          when: { path: 'steps.read-photo.status', operator: 'equals', expected: 'succeeded' },
          requirements: [{ id: 'classified', statement: 'The ticket has exactly one label', check: { kind: 'assert', path: 'steps.classify.output.label', operator: 'is_non_empty' } }]
        },
        { id: 'summarise', action: { kind: 'code', entrypoint: 'summarise', inputs: { label: 'steps.classify.output.label', units: 'steps.score.output.units' }, source: { inline: 'def summarise(label, units): ...' } }, depends_on: ['read-photo'] }
      ],
      requirements: [
        { id: 'policy-followed', statement: 'The reply follows the refund policy for its label', check: { kind: 'agent', agent: AGENT, inputs: { summary: 'steps.summarise.output.summary' } } },
        hasAnswer
      ]
    },
    [SERVICE]
  ),
  'session-quality@1.0.0': verifierCard(SESSION_VERIFIER, {
    description: 'Every reply in a session passed its refund check.',
    inputs: { outcomes: 'verifiers.ticket-policy.requirements.policy-followed.outcome', tickets: 'verifiers.ticket-policy.inputs.ticket' },
    requirements: [{ id: 'all-passed', statement: 'No reply in the session failed', check: { kind: 'assert', each: 'inputs.outcomes[*]', operator: 'equals', expected: 'passed' } }]
  }),
  'latency-drift@1.0.0': verifierCard(DRIFT_VERIFIER, {
    inputs: { latency: { type: 'samples', features: ['latency_ms'] } },
    requirements: [{ id: 'latency-stable', statement: 'Latency stays within its fitted baseline', check: { kind: 'drift', on: 'inputs.latency', method: 'Psi', signal: { kind: 'feature', name: 'latency_ms' }, condition: { kind: 'window', size: 100 } } }]
  })
};
/** The Service binding the newer ticket-policy version: once on itself, explicitly, and once on a component, on a schedule. */
export const service = {
  apiVersion: 'wyrd/v1',
  kind: 'Service',
  metadata: { name: SERVICE.name, version: SERVICE.version, uid: 'card_service' },
  spec: {
    service_type: 'agent',
    verified_by: [{ verifier: VERIFIER }, { verifier: { ...VERIFIER, version: '1.0.0' }, runs_on: { kind: 'group_end', after: '10m' } }],
    components: [{ alias: 'agent', ref: AGENT, verified_by: [{ verifier: VERIFIER, runs_on: { kind: 'schedule', cron: '0 * * * *', tz: 'Europe/Berlin' } }] }]
  },
  relationships: { outbound_refs: [{ ref: AGENT, alias: 'agent' }, { ref: VERIFIER }], inbound_refs: [] },
  status: { phase: 'active' }
} as unknown as RegisteredCard;
/** The files registration recorded for the Model. */
export const artifacts = {
  artifacts: [
    { relative_path: 'weights/model.bin', size_bytes: 4_404_019, sha256: 'n4bQgYhMfWWaL+qgxVrQFaO/TxsrC4Is0V1sFbDwCgg=', content_type: 'application/octet-stream' },
    { relative_path: 'README.md', size_bytes: 812, sha256: 'LCa0a2j/xo/5m0U8HTBBNBNCLXBkg7+g+YpeiGJm564=', content_type: null }
  ]
};
/** One row per version across kinds, as the inventory lists them. */
export const inventory = [
  summary('1.0.0', 'active', 'Prompt', 'support-reply'),
  summary('1.1.0', 'active', 'Prompt', 'support-reply'),
  summary('1.0.0', 'active', 'Agent', 'support'),
  summary('1.4.1', 'deprecated'),
  summary('1.4.2', 'pending'),
  summary('2.0.0-rc.1', 'failed', 'Data', 'tickets')
];
export const denied = { code: 'WYRD_AUTH_403_FORBIDDEN', title: 'Forbidden', status: 403 };
export const notFound = { code: 'WYRD_REGISTRY_404_CARD_NOT_FOUND', title: 'Card not found', status: 404 };
export const RANGE = { from: '2026-10-20T00:00:00Z', to: '2026-10-20T12:00:00Z' };

const col = (name: string, data_type = 'Utf8', nullable = true) => ({ name, data_type, nullable });
const result = (columns: QueryResult['columns'], rows: unknown[][]): QueryResult => ({ columns, rows, terminal: terminal(rows.length) });
const NS = 'Timestamp(Nanosecond, Some("UTC"))';
export const TRACE = '4bf92f3577b34da6a3ce929d0e0e4736';

/** Root spans, as the traces list reads them: shown columns, then trace_id and span_id. */
export const traces = result(
  [col('start_time_unix_nano', NS, false), col('name', 'Utf8', false), col('service_name'), col('duration_nano', 'Int64', false), col('status_code', 'Int32', false), col('trace_id', 'Utf8', false), col('span_id', 'Utf8', false)],
  [
    ['2026-10-20T11:58:01.120000000+00:00', 'invoke_agent support', 'agent-api', 2_840_000_000, 2, TRACE, '00f067aa0ba902b7'],
    ['2026-10-20T11:57:44.004000000+00:00', 'POST /v1/answer', 'agent-api', 612_000_000, 1, 'a3ce929d0e0e47364bf92f3577b34da6', '53995c3f42cd8ad8'],
    ['2026-10-20T11:56:12.500000000+00:00', 'chat gpt-5', null, 95_000_000, 0, '0e0e47364bf92f3577b34da6a3ce929d', 'b9c7c989f97918e1']
  ]
);
/**
 * Spans as the spans list reads them, for grouping and naming proof: an
 * ordinary HTTP span with no GenAI attribute, a name that reads like a status
 * on an ok span, an unset service beside the literal text "∅", and two
 * durations that both read "1ms".
 */
export const spans = result(
  [col('start_time_unix_nano', NS, false), col('name', 'Utf8', false), col('service_name'), col('duration_nano', 'Int64', false), col('status_code', 'Int32', false), col('trace_id', 'Utf8', false), col('span_id', 'Utf8', false)],
  [
    ['2026-10-20T11:58:02.450000001+00:00', 'GET /search', 'search', 1_000_000, 2, TRACE, 'c1d2e3f405060708'],
    ['2026-10-20T11:58:02.400000000+00:00', 'execute_tool search', 'agent-api', 900_000_000, 2, TRACE, 'b9c7c989f97918e1'],
    ['2026-10-20T11:58:01.130000000+00:00', 'chat gpt-5', 'agent-api', 1_200_000_000, 1, TRACE, '53995c3f42cd8ad8'],
    ['2026-10-20T11:57:44.004000000+00:00', 'status.error', '∅', 1_000_400, 1, 'a3ce929d0e0e47364bf92f3577b34da6', '53995c3f42cd8ad9'],
    ['2026-10-20T11:57:40.000000000+00:00', 'POST /v1/checkout/orders/{order_id}/payment-intents/confirm-with-a-very-long-route', null, 612_000_000, 0, '0e0e47364bf92f3577b34da6a3ce929d', 'b9c7c989f97918e2']
  ]
);

/** Log records as the logs list reads them: shown columns, then the record key; 120 rows so pages and the read limit show. */
export const logs = result(
  [col('time_unix_nano', NS, false), col('severity_text'), col('service_name'), col('body_text'), col('exception_type'), col('trace_id'), col('span_id'), col('observed_time_unix_nano', NS, false), col('wyrd_request_id', 'Utf8', false), col('severity_number', 'Int32')],
  Array.from({ length: 120 }, (_, index) => {
    const at = `2026-10-20T11:${String(59 - Math.floor(index / 2)).padStart(2, '0')}:${index % 2 ? '10' : '40'}.518000000+00:00`;
    const error = index % 9 === 4;
    return [
      at,
      error ? 'ERROR' : index % 3 === 0 ? 'WARN' : 'INFO',
      index % 4 === 0 ? null : 'search',
      error ? 'search timed out after 1412ms while waiting for the upstream index shard to answer' : `/search answered in ${200 + index}ms`,
      error ? 'TimeoutError' : null,
      index % 5 === 0 ? null : TRACE,
      index % 5 === 0 ? null : 'b9c7c989f97918e1',
      at,
      `req_${index}`,
      error ? 17 : index % 3 === 0 ? 13 : 9
    ];
  })
);

/** Metric points as the metrics list reads them: a sum keeps its integer, a gauge its double; the other is null. */
export const points = result(
  [col('time_unix_nano', NS, false), col('metric_name', 'Utf8', false), col('metric_type', 'Utf8', false), col('unit'), col('int_value', 'Int64'), col('double_value', 'Float64')],
  [
    ['2026-10-20T11:58:00.000000000+00:00', 'ui.requests', 'sum', '1', 7, null],
    ['2026-10-20T11:58:00.000000000+00:00', 'ui.load', 'gauge', null, null, 0.5]
  ]
);

/** Authored SQL whose aliases collide with canonical columns; its values are only what it returned. */
export const AUTHORED_SQL = 'SELECT name, status_code, duration_nano FROM vala.datasets.checks WHERE at >= $1::timestamptz AND at < $2::timestamptz';
export const authored = result(
  [col('name'), col('status_code', 'Int64'), col('duration_nano', 'Int64')],
  [
    ['refund policy cited', 2, 1_500_000_000],
    ['answer grounded', 0, 3]
  ]
);

/** Span and error counts of the listed traces. */
export const traceCounts = result([col('trace_id'), col('spans', 'Int64'), col('errors', 'Int64')], [[TRACE, 6, 1], ['a3ce929d0e0e47364bf92f3577b34da6', 3, 0]]);

/** One trace: a root, two children, a grandchild with an error, and a span whose parent was not retained. */
export const trace = result(
  [col('span_id'), col('parent_span_id'), col('name'), col('service_name'), col('start_time_unix_nano', NS), col('duration_nano', 'Int64'), col('status_code', 'Int32')],
  [
    ['00f067aa0ba902b7', null, 'invoke_agent support', 'agent-api', '2026-10-20T11:58:01.120000000+00:00', 2_840_000_000, 2],
    ['53995c3f42cd8ad8', '00f067aa0ba902b7', 'chat gpt-5', 'agent-api', '2026-10-20T11:58:01.130000000+00:00', 1_200_000_000, 1],
    ['b9c7c989f97918e1', '00f067aa0ba902b7', 'execute_tool search', 'agent-api', '2026-10-20T11:58:02.400000000+00:00', 900_000_000, 2],
    ['c1d2e3f405060708', 'b9c7c989f97918e1', 'GET /search', 'search', '2026-10-20T11:58:02.450000001+00:00', 800_000_000, 2],
    ['0102030405060708', 'ffffffffffffffff', 'late span', 'search', '2026-10-20T11:58:03.000000000+00:00', 10_000_000, 0]
  ]
);

/** One stored span, whole: recorded GenAI messages beside other attributes, an event and an exception. */
export const spanRow = result(
  [col('trace_id'), col('span_id'), col('name'), col('start_time_unix_nano', NS), col('duration_nano', 'Int64'), col('status_code', 'Int32'), col('service_name'), col('attributes', 'Variant'), col('events', 'List'), col('exception_type'), col('exception_message'), col('exception_stacktrace')],
  [
    [
      TRACE, 'b9c7c989f97918e1', 'execute_tool search', '2026-10-20T11:58:02.400000000+00:00', 900_000_000, 2, 'agent-api',
      {
        'gen_ai.system_instructions': [{ type: 'text', content: 'You answer support tickets. Cite the policy section you rely on.' }],
        'gen_ai.input.messages': [
          { role: 'user', parts: [{ type: 'text', content: 'refund? I bought the blender 20 days ago.' }] },
          { role: 'tool', parts: [{ type: 'tool_call_response', id: 'call_1', response: { order: 'A-4471', days_since: 20 } }] }
        ],
        'gen_ai.tool.name': 'search',
        'gen_ai.request.model': 'gpt-5',
        'gen_ai.usage.input_tokens': 1912,
        'server.address': 'search.internal'
      },
      [{ name: 'retry', attributes: {} }],
      'TimeoutError', 'search timed out', null
    ]
  ]
);
/** The same span with no attributes, events or exception recorded. */
export const bareSpan = result(spanRow.columns, [[...spanRow.rows[0].slice(0, 7), {}, [], null, null, null]]);

/** One stored log record, whole. */
export const logRow = result(
  [col('time_unix_nano', NS), col('observed_time_unix_nano', NS), col('wyrd_request_id'), col('severity_number', 'Int32'), col('severity_text'), col('service_name'), col('body', 'Variant'), col('trace_id'), col('span_id'), col('attributes', 'Variant'), col('exception_type')],
  [['2026-10-20T11:58:02.410000000+00:00', '2026-10-20T11:58:02.411000000+00:00', 'req_01', 17, 'ERROR', 'search', 'search timed out', TRACE, 'b9c7c989f97918e1', { 'http.route': '/search', latency_ms: 1412 }, null]]
);
export const LOG = { time_unix_nano: '2026-10-20T11:58:02.410000000+00:00', observed_time_unix_nano: '2026-10-20T11:58:02.411000000+00:00', wyrd_request_id: 'req_01' };

/** Facet counts: largest first, with a null value. */
export const facet = result([col('value'), col('n', 'Int64')], [['agent-api', 42], ['search', 7], [null, 3]]);
/** Facet counts by the column a facet query groups; span names read past the ten shown. */
function facetFor(sql: string): QueryResult {
  if (sql.startsWith('SELECT status_code ')) return { ...facet, rows: [[1, 187], [2, 6], [0, 1]] };
  if (sql.startsWith('SELECT severity_text ')) return { ...facet, rows: [['INFO', 810], ['WARN', 352], ['ERROR', 38]] };
  if (sql.startsWith('SELECT name ')) return { ...facet, rows: Array.from({ length: 11 }, (_, index) => [`span ${index + 1}`, 200 - index * 9]) };
  return facet;
}
/** One stat total and its buckets. */
export const total = (value: unknown) => result([col('value', 'Int64')], [[value]]);
export const buckets = result([col('at', NS), col('value', 'Int64')], [['2026-10-20T10:00:00Z', 4], ['2026-10-20T11:00:00Z', 9], ['2026-10-20T12:00:00Z', 6]]);
export const costBuckets = result(
  [col('at', NS), col('value', 'Decimal128(38, 12)'), col('currency')],
  [['2026-10-20T10:00:00Z', '0.400000000000', 'EUR'], ['2026-10-20T10:00:00Z', '0.100000000000', 'USD'], ['2026-10-20T11:00:00Z', '0.850000000000', 'EUR'], ['2026-10-20T11:00:00Z', '0.320000000000', 'USD']]
);
export const costs = result([col('value', 'Decimal128(38, 12)'), col('currency')], [['1.250000000000', 'EUR'], ['0.420000000000', 'USD']]);

/** The fixture answering one Observe read, chosen by its fixed SQL shape. */
export function observeAnswer(sql: string): QueryResult | undefined {
  if (sql === AUTHORED_SQL) return authored;
  if (sql.includes("AS parent_span_id FROM bifrost.traces.spans")) return spans;
  if (sql.includes("encode(span_id, 'hex') AS span_id FROM bifrost.traces.spans")) return traces;
  if (sql.includes('wyrd_request_id FROM bifrost.logs.records')) return logs;
  if (sql.includes('FROM bifrost.metrics.points') && !sql.includes(' AS n FROM')) return points;
  if (sql.includes('AS spans, count(*) FILTER')) return traceCounts;
  if (sql.includes("encode(parent_span_id, 'hex')")) return trace;
  if (sql.startsWith('SELECT * FROM bifrost.logs.records')) return logRow;
  if (sql.startsWith('SELECT * FROM bifrost.traces.spans')) return spanRow;
  if (sql.includes(' AS n FROM')) return facetFor(sql);
  if (sql.includes('AS at,')) return sql.includes(', currency FROM') ? costBuckets : buckets;
  if (sql.includes(', currency FROM')) return costs;
  if (sql.startsWith('SELECT') && sql.includes(' AS value FROM')) return total(128);
  return undefined;
}

/** A response with nothing in it: no rows, list items, files, relationships or variables. */
function emptied(value: unknown): unknown {
  const object = value as Record<string, unknown>;
  if ('rows' in object) return { ...(object as unknown as QueryResult), rows: [], terminal: terminal(0) };
  if ('items' in object) return { items: [], next_cursor: null };
  if ('artifacts' in object) return { artifacts: [] };
  if ('metadata' in object) return { ...object, relationships: { inbound_refs: [], outbound_refs: [] }, status: { phase: 'active', prompt: { variables: [], media_variables: [] } } };
  return value;
}

/** Fixture executors by the state each makes every panel reach: rows kept or emptied, refused, failed, or never answered. */
export const stateExecutors: Record<'populated' | 'empty' | 'denied' | 'error' | 'loading', Executor> = {
  populated: { run: (requests) => fixtureExecutor.run(requests) },
  empty: {
    run: (requests) =>
      fixtureExecutor.run(requests).then((results) =>
        results.map((each) => (each.ok ? { ok: true, value: emptied(each.value) } : each))
      ) as never
  },
  denied: { run: (requests) => Promise.resolve(requests.map(() => ({ ok: false, problem: denied }))) as never },
  error: { run: (requests) => Promise.resolve(requests.map(() => ({ ok: false, problem: { code: 'WYRD_VALA_413_QUERY_RESULT_TOO_LARGE', title: 'Query result too large', status: 413 } }))) as never },
  loading: { run: () => new Promise(() => {}) }
};

/** A live executor: each traces list read after the first delivers one more new trace. */
export function liveExecutor(): Executor {
  let reads = 0;
  return {
    run: (requests) =>
      fixtureExecutor.run(requests).then((results) =>
        results.map((each, index) => {
          const { sql = '', params = [] } = requests[index] as { sql?: string; params?: unknown[] };
          if (each.ok && sql.includes('AS spans'))
            return { ok: true, value: { ...traceCounts, rows: params.map((id) => traceCounts.rows.find((row) => row[0] === id) ?? [id, 1, 0]) } };
          if (!each.ok || !sql.includes("AS span_id FROM bifrost.traces.spans") || sql.includes('AS parent_span_id FROM bifrost.traces.spans')) return each;
          const fresh = Array.from({ length: reads++ }, (_, n) => ['2026-10-20T11:59:30.000000000+00:00', `GET /health ${n + 1}`, 'agent-api', 4_000_000, 1, (n + 1).toString(16).padStart(32, 'f'), (n + 1).toString(16).padStart(16, 'e')]);
          return { ok: true, value: { ...traces, rows: [...fresh, ...traces.rows] } };
        })
      ) as never
  };
}

/** An executor serving the fixtures above, for views rendered without a server. */
export const fixtureExecutor: Executor = {
  run: (requests) =>
    Promise.resolve(
      requests.map((request) => {
        switch (request.op) {
          case 'cards.get_by_ref': {
            const ref = request.card_ref;
            if (ref.kind === 'Agent') return { ok: true, value: agent };
            if (ref.kind === 'Verifier') return verifiers[`${ref.name}@${ref.version}`] ? { ok: true, value: verifiers[`${ref.name}@${ref.version}`] } : { ok: false, problem: notFound };
            if (ref.kind === 'Service') return { ok: true, value: service };
            if (ref.name === NATIVE.name) return { ok: true, value: nativePrompt };
            if (ref.kind === 'Prompt') return prompts[ref.version] ? { ok: true, value: prompts[ref.version] } : { ok: false, problem: notFound };
            return { ok: true, value: registered };
          }
          case 'cards.list':
            if (request.kind === 'Prompt' && request.name === PROMPT.name)
              return { ok: true, value: { items: [summary('1.0.0', 'active', 'Prompt', PROMPT.name), summary('1.1.0', 'active', 'Prompt', PROMPT.name)], next_cursor: null } };
            if (request.kind === 'Verifier' && request.name === VERIFIER.name)
              return { ok: true, value: { items: [summary('1.0.0', 'active', 'Verifier', VERIFIER.name), summary('1.1.0', 'active', 'Verifier', VERIFIER.name)], next_cursor: null } };
            if (request.kind === 'Verifier' && typeof request.name === 'string') return { ok: true, value: { items: [summary('1.0.0', 'active', 'Verifier', request.name)], next_cursor: null } };
            if (request.name === NATIVE.name) return { ok: true, value: { items: [summary('1.0.0', 'active', 'Prompt', NATIVE.name)], next_cursor: null } };
            if (request.name !== undefined) return { ok: true, value: { items: [summary('1.4.2'), summary('1.4.1', 'deprecated')], next_cursor: null } };
            return { ok: true, value: { items: request.cursor ? inventory.slice(3) : inventory.slice(0, 3), next_cursor: request.cursor ? null : 'page-2' } };
          case 'cards.list_artifacts':
            return { ok: true, value: artifacts };
          case 'bifrost.query': {
            // Honour bound $1/$2 like the server, so a brushed range visibly narrows the plot.
            const observed = observeAnswer(request.sql);
            if (observed) return { ok: true, value: observed };
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
