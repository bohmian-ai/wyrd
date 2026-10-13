// @vitest-environment jsdom
import { spawn, type ChildProcess } from 'node:child_process';
import { once } from 'node:events';
import { beforeAll, afterAll, expect, test } from 'vitest';

// TASK-007 — the Service Card operational workspace journey against the real
// dev server: URL-restored view/version/range state, the truthful Overview
// state variants, publication-bound intelligence, deterministic Composition
// with selection, and the declaration-first Definition.
let server: ChildProcess;
let origin: string;
beforeAll(async () => {
  server = spawn(
    process.execPath,
    ['node_modules/vite/bin/vite.js', '--host', '127.0.0.1', '--port', '0'],
    {
      env: {
        ...process.env,
        WYRD_UI_LOCAL_AUTH: 'true',
        WYRD_UI_MOCK_DATA: 'true',
        NO_COLOR: '1'
      },
      stdio: ['ignore', 'pipe', 'pipe']
    }
  );
  origin = await new Promise<string>((resolve, reject) => {
    let output = '';
    const timer = setTimeout(() => reject(new Error(`UI did not start: ${output}`)), 20_000);
    server.stdout!.on('data', (chunk) => {
      output += chunk.toString();
      const url = output.match(/http:\/\/127\.0\.0\.1:\d+/)?.[0];
      if (url) {
        clearTimeout(timer);
        resolve(url);
      }
    });
    server.stderr!.on('data', (chunk) => {
      output += chunk.toString();
    });
    server.once('error', (error) => {
      clearTimeout(timer);
      reject(error);
    });
    server.once('exit', () => {
      clearTimeout(timer);
      reject(new Error(output));
    });
  });
}, 25_000);
afterAll(async () => {
  if (server && server.exitCode === null) {
    const closed = once(server, 'exit');
    server.kill();
    await closed;
  }
});

let cookie: string;
async function get(path: string) {
  const response = await fetch(origin + path, { headers: { cookie } });
  expect(response.status).toBe(200);
  return response.text();
}
beforeAll(async () => {
  const login = await fetch(origin + '/?/login', {
    method: 'POST',
    headers: { origin, accept: 'text/html' },
    body: new URLSearchParams(),
    redirect: 'manual'
  });
  cookie = login.headers.get('set-cookie')!.split(';')[0];
});

const CARD = '/t/acme/cards/card_service_01';

test('Overview is the default: one assessment, separated channels, dominant trends with units, thresholds and sources', async () => {
  const html = await get(CARD);
  // The needs-attention entry state (C-09), server-projected.
  expect(html).toContain('▲ NEEDS ATTENTION');
  expect(html).toContain('Model input drift is over threshold on ranker');
  // One status authority: the page-layout workspace suppresses the shared
  // header's status chip (the label survives only in the data payload).
  expect(html).not.toContain('</span>Needs attention');
  // The range control's scope is stated, not implied.
  expect(html).toContain('drift and eval signals keep their own windows');
  // Four separated state channels — never one collapsed badge.
  for (const channel of ['CARD STATE', 'DEPLOYMENT', 'OPERATIONAL', 'FRESHNESS'])
    expect(html).toContain(channel);
  expect(html).toContain('● 6/6 · mock projection');
  expect(html).toContain('correlation, not proven cause');
  // The dominant chart grid with unit, latest value, source and thresholds.
  for (const chart of ['REQUESTS', 'ERROR RATE', 'LATENCY', 'AVAILABILITY', 'CHECKOUT DECLINE RATE'])
    expect(html).toContain(chart);
  expect(html).toContain('3.4 req/s');
  expect(html).toContain('budget 0.50%');
  expect(html).toContain('p95 target 400');
  expect(html).toContain('slo 99.9%');
  expect(html).toContain('vala metrics');
  // Multi-series latency stays legible without color: distinct dashes.
  expect(html).toContain('stroke-dasharray="6 3"');
  expect(html).toContain('stroke-dasharray="2 3"');
  // Chart links carry service and range scope into canonical Observe routes.
  expect(html).toContain('/t/acme/observe/metrics?service=checkout-api&amp;range=1h');
  expect(html).toContain('/t/acme/observe/traces?service=checkout-api&amp;range=1h');
  // The subnav is exactly Overview · Composition · Definition.
  expect(html).toContain('?view=composition&amp;version=v12');
  expect(html).toContain('?view=definition&amp;version=v12');
  expect(html).not.toContain('Alerts');
}, 30_000);

test('view, version and range restore from a pasted URL', async () => {
  const html = await get(`${CARD}?view=overview&version=v12&range=6h`);
  // The selected range is marked and every chart link carries it.
  expect(html).toContain('/t/acme/observe/metrics?service=checkout-api&amp;range=6h');
  expect(html).toMatch(/aria-current="true"[^>]*>6h</);
});

test('healthy is earned from signal data, never inferred', async () => {
  const html = await get(`${CARD}?view=overview&version=v12&range=1h&state=healthy`);
  expect(html).toContain('✓ OPERATING NORMALLY');
  expect(html).toContain('stated from current signal data, not implied by absence');
  expect(html).toContain('PSI 0.11');
  // The rendered assessment is the healthy one — the attention title exists
  // only inside the serialized variant payload, never as rendered markup.
  expect(html).not.toContain('>▲ NEEDS ATTENTION');
});

test('stale data renders labeled gaps — never zero, never healthy', async () => {
  const html = await get(`${CARD}?view=overview&version=v12&range=1h&state=stale`);
  expect(html).toContain('◔ STALE — NO RECENT DATA');
  expect(html).toContain('▲ 25m OLD');
  expect(html).toContain('○ UNKNOWN');
  expect(html).toContain('gap — not zero');
  expect(html).toContain('— · last 15:17');
  expect(html).toContain('unknown is not none');
  expect(html).toContain('— aged');
});

test('partial authorization refuses drift visibly, keeps the subject and the rest usable', async () => {
  const html = await get(`${CARD}?view=overview&version=v12&range=1h&state=partial`);
  expect(html).toContain('◑ PARTIAL — DRIFT UNAUTHORIZED');
  expect(html).toContain('observe:drift');
  expect(html).toContain('subject stays visible: ranker (model_primary · Model v12)');
  expect(html).toContain('MAY BE INCOMPLETE');
  // The eval panel stays current; the drift investigation names its scope.
  expect(html).toContain('✓ PASSING');
  expect(html).toContain('(scope required)');
});

test('a failed projection is a named load error with a Retry that clears only the failure', async () => {
  const html = await get(`${CARD}?view=overview&version=v12&range=1h&state=failed`);
  expect(html).toContain('✕ PROJECTION FAILED');
  expect(html).toContain('WYRD_OBS_504_QUERY_TIMEOUT');
  expect(html).toContain('✕ LOAD FAILED');
  expect(html).toContain('Retry');
  expect(html).toContain('?view=overview&amp;version=v12&amp;range=1h"');
  // Independently sourced intelligence stays rendered.
  expect(html).toContain('MODEL-DRIFT (Verifier v3)');
  expect(html).toContain('unaffected by the failed projection');
});

test('the saved custom chart and the restrained Add-chart slot render as opt-in projections', async () => {
  const html = await get(CARD);
  expect(html).toContain('custom · saved for this Service');
  expect(html).toContain('+ ADD CHART');
  expect(html).toContain('Opt-in, never filler');
});

test('drift/eval intelligence keeps subject, version, threshold and scope-preserving Observe links', async () => {
  const html = await get(CARD);
  expect(html).toContain('MODEL-DRIFT (Verifier v3)');
  expect(html).toContain('✕ BREACHED');
  expect(html).toContain('subject ranker (model_primary · Model v12)');
  expect(html).toContain('PSI 0.27');
  expect(html).toContain('threshold 0.20');
  expect(html).toContain('baseline txns-2026q3 (Data v3)');
  expect(html).toContain('/t/acme/observe/drift?service=checkout-api&amp;driftCard=card_drift_01');
  expect(html).toContain('CHECKOUT-AGENT-EVAL (Verifier v4)');
  expect(html).toContain('subject checkout-agent (agent_triage · Agent v6)');
  expect(html).toContain('pass ≥ 95%');
  expect(html).toContain('96.4%');
  // The breached signal precedes the passing one, and attention names it.
  expect(html.indexOf('MODEL-DRIFT')).toBeLessThan(html.indexOf('CHECKOUT-AGENT-EVAL'));
  expect(html).toContain('ATTENTION — 1 ACTIVE');
  expect(html).toContain('model-drift breach on ranker');
});

test('a historical version shows only version-scoped projections, never current health', async () => {
  const html = await get(`${CARD}?view=overview&version=v11&range=1h`);
  expect(html).toContain('◔ NO OBSERVATIONS FOR v11');
  expect(html).toContain('○ NO REPORT FOR v11');
  expect(html).toContain('explicit no-data — never v12 health');
  expect(html).toContain('none apply to v11');
  expect(html).toContain('never inferred from v12 results');
  expect(html).not.toContain('3.4 req/s');
  expect(html).not.toContain('PSI 0.27');
  // An unknown version stays a structured 404.
  const missing = await fetch(origin + `${CARD}?version=v99`, { headers: { cookie } });
  expect(missing.status).toBe(404);
});

test('Composition preserves lanes, aliases, identities and selection without claiming execution', async () => {
  const html = await get(`${CARD}?view=composition&version=v12`);
  for (const lane of ['RUNTIME COMPOSITION', 'INPUTS &amp; DEFINITIONS', 'MEASUREMENT', 'REACTION'])
    expect(html).toContain(lane);
  expect(html).toContain('declaration only');
  expect(html).toContain('never proves a runtime execution or observation occurred');
  // Nodes keep kind, name, version and status; aliases resolve to one Card.
  expect(html).toContain('ranker-shadow');
  expect(html).toContain('model_shadow');
  expect(html).toContain('2 aliases (one Card)');
  expect(html).toContain('capture_prompt and shared_prompt');
  expect(html).toContain('mock-only continuation');
  // Selecting a node opens the drawer and keeps direct Card routes.
  const selected = await get(`${CARD}?view=composition&version=v12&sel=card_drift_01`);
  expect(selected).toContain('SELECTED — MODEL-DRIFT (VERIFIER · V3)');
  expect(selected).toContain('✕ close · graph stays in view');
  expect(selected).toContain('/t/acme/cards/card_drift_01');
  expect(selected).toContain('/t/acme/observe/drift?driftCard=card_drift_01');
  expect(selected).toContain('‹ Back to Composition');
});

test('Definition presents the declaration, bindings, governance and closed raw spec', async () => {
  const html = await get(`${CARD}?view=definition&version=v12`);
  expect(html).toContain('DECLARATION — WHAT v12 DEFINES');
  expect(html).toContain('acme.checkout.app:app — imported by the deploy image, never by Wyrd');
  expect(html).toContain('model_primary');
  expect(html).toContain('shared_prompt');
  expect(html).toContain('alias of the same Card');
  expect(html).toContain('TWO SUBJECT SEMANTICS');
  expect(html).toContain('NONE DECLARED');
  expect(html).toContain('stated as absent, not hidden');
  expect(html).toContain('spec.yaml');
  expect(html).toContain('svc-checkout-api');
  expect(html).toContain('server-managed');
  expect(html).toContain('server-derived · declaration ≠ execution');
  expect(html).toContain('SECRETS');
  expect(html).toContain('never');
  // The v11 declaration rescopes with the version.
  const v11 = await get(`${CARD}?view=definition&version=v11`);
  expect(v11).toContain('DECLARATION — WHAT v11 DEFINES');
  expect(v11).toContain('registered 2026-08-02');
  expect(v11).not.toContain('ranker-shadow');
});

test('other Service Cards without a projected workspace keep the shared shell', async () => {
  const html = await get('/t/acme/cards/card_service_02');
  expect(html).toContain('ledger-api');
  expect(html).toContain('Metadata');
  expect(html).not.toContain('OPERATING DASHBOARD');
  expect(html).not.toContain('?view=composition');
});

type Upstream = { path?: Record<string, string>; query?: Record<string, unknown> };
type Query = { sql: string; params: readonly (string | number)[] };

const SVC = '0190a000-0000-7000-8000-0000000000a1';
const AGENT = '0190a000-0000-7000-8000-0000000000a2';
const PROMPT = '0190a000-0000-7000-8000-0000000000a3';
const EVAL = '0190a000-0000-7000-8000-0000000000a4';
const DRIFT = '0190a000-0000-7000-8000-0000000000a5';
const OPERATOR = '0190a000-0000-7000-8000-0000000000a6';
const ref = (kind: string, name: string, uid: string) => ({ kind, name, version: '1.0.0', space: 'desk', uid });

/** One registered Service version: an Agent verified by an Eval with an on-failure Operator, a Prompt, and a Service-level Drift. */
const service = {
  card: {
    apiVersion: 'wyrd/v1',
    kind: 'Service',
    metadata: { name: 'support-desk', space: 'desk', version: '2.0.0', uid: SVC, spec_hash: 'h-svc' },
    spec: {
      description: 'Answers support tickets.',
      components: [
        {
          alias: 'agent',
          ref: ref('Agent', 'triage', AGENT),
          verified_by: [{ verifier: ref('Verifier', 'answer-quality', EVAL), on_failure: [ref('Operator', 'page-oncall', OPERATOR)] }]
        },
        { alias: 'prompt', ref: ref('Prompt', 'triage-prompt', PROMPT) }
      ],
      verified_by: [{ verifier: ref('Verifier', 'latency-drift', DRIFT) }]
    },
    relationships: {},
    status: { phase: 'active', verification: { binding_ids: ['b-eval', 'b-drift'] } }
  },
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-02T00:00:00Z'
};
const statuses: Record<string, unknown> = {
  'b-eval': { active: true, binding_id: 'b-eval', owner_card_uid: SVC, subject_card_uid: AGENT, verifier_uid: EVAL, readiness: 'ready' },
  'b-drift': { active: true, binding_id: 'b-drift', owner_card_uid: SVC, subject_card_uid: SVC, verifier_uid: DRIFT, readiness: 'baseline_not_ready' }
};
const traceFields = [
  { name: 'bucket', type: 'Int64' },
  { name: 'spans', type: 'Int64' },
  { name: 'errors', type: 'Int64' },
  { name: 'mean_duration_nano', type: 'Float64' }
];
const resultFields = [
  { name: 'binding_id', type: 'Utf8' },
  { name: 'implementation', type: 'Utf8' },
  { name: 'verdict', type: 'Utf8' },
  { name: 'results', type: 'Int64' },
  { name: 'last_ended', type: 'Timestamp<MICROSECOND, UTC>' }
];

/** A live client over a fake session: `/v1` answers from the registry above, analytical reads from `analytical`. */
async function liveClient(analytical: (q: Query) => unknown) {
  const { WyrdClient } = await import('$lib/server/wyrd');
  const queries: Query[] = [];
  const session = {
    api: async (_method: string, path: string, request: Upstream = {}) => {
      const [status, body] =
        path === '/cards/by-uid/{kind}/{card_uid}'
          ? request.path!.kind === 'Service' ? [200, service] : [404, { code: 'WYRD_REGISTRY_404_CARD_NOT_FOUND' }]
          : path === '/cards/{kind}/{space}/{name}/versions'
            ? [200, { versions: ['2.0.0'] }]
            : [200, statuses[request.path!.binding_id]];
      return new Response(JSON.stringify(body), { status });
    },
    query: async (sql: string, params: readonly (string | number)[]) => {
      queries.push({ sql, params });
      const answer = analytical({ sql, params });
      if (answer instanceof Error) throw answer;
      return answer;
    }
  };
  const client = new WyrdClient(
    {
      tenant: { key: 'acme', name: 'acme', tenantId: 't', permissions: ['cards:read'] },
      subject: { id: 'p', name: 'p' },
      permissions: ['cards:read']
    },
    false,
    session as never
  );
  return { client, queries };
}
const denied = Object.assign(new Error('denied'), { code: 'WYRD_PERMISSION_403_DENIED_RBAC', status: 403 });
const unavailable = Object.assign(new Error('down'), { code: 'WYRD_BIFROST_503_UNAVAILABLE', status: 503 });
const ENDED = Date.parse('2026-10-01T12:00:00Z');
const healthy = ({ sql }: Query) =>
  sql.includes('vala.traces.spans')
    ? {
        outcome: 'success',
        fields: traceFields,
        rows: [
          { bucket: 28n, spans: 4n, errors: 1n, mean_duration_nano: 2.5e8 },
          { bucket: 29n, spans: 6n, errors: 0n, mean_duration_nano: 1.5e8 }
        ]
      }
    : {
        outcome: 'success',
        fields: resultFields,
        rows: [
          { binding_id: 'b-eval', implementation: 'eval', verdict: 'passed', results: 3n, last_ended: ENDED - 60_000 },
          { binding_id: 'b-eval', implementation: 'eval', verdict: 'failed', results: 1n, last_ended: ENDED },
          // A result of a binding this version does not declare is never attributed to it.
          { binding_id: 'b-other', implementation: 'eval', verdict: 'passed', results: 9n, last_ended: ENDED }
        ]
      };

test('live UI Service workspace projection', async () => {
  const { RANGE_MS } = await import('$lib/features/observe/core/filter-state');
  const { client, queries } = await liveClient(healthy);
  const detail = await client.card(SVC, undefined, '6h');
  const view = (detail.presentation as import('../../core/types').ServicePresentation);

  // Exact scope: telemetry by the Card's name and version over 30 aligned buckets of the range, results by the exact owner version.
  const [traces, results] = [queries.find((q) => q.sql.includes('vala.traces.spans'))!, queries.find((q) => q.sql.includes('vala.verification.results'))!];
  const [name, version, from, end, step] = traces.params as [string, string, number, number, number];
  expect([name, version]).toEqual(['support-desk', '2.0.0']);
  expect(step).toBe(RANGE_MS['6h'] / 1000 / 30);
  expect(end - from).toBe(step * 30);
  expect(results.params[0]).toBe(SVC);
  expect(results.params[2] as number - (results.params[1] as number)).toBe(30 * 24 * 3600);

  // Charts draw the read buckets; missing buckets are zero spans, latency only where spans exist.
  const [requests, errors, latency] = view.overview.default.charts;
  expect(requests).toMatchObject({ state: 'ok', latest: '10 in 6h' });
  expect(errors).toMatchObject({ latest: '1 in 6h' });
  expect(requests.series[0].points).toHaveLength(30);
  expect(requests.series[0].points.slice(-2)).toEqual([4, 6]);
  expect(errors.series[0].points.slice(-2)).toEqual([1, 0]);
  expect(latency).toMatchObject({ series: [{ points: [250, 150] }], note: '2/30 buckets observed · empty buckets omitted' });
  expect(requests.link.href).toBe('/observe/traces?service=support-desk&range={range}');

  // Signals per declared binding: latest verdict wins, counts are the binding's own, the stray binding is excluded.
  const [evalSignal, driftSignal] = view.overview.default.signals;
  expect(evalSignal).toMatchObject({
    verdict: { label: 'FAILED', tone: 'danger' },
    context: 'eval · 30 days: 3 passed · 1 failed',
    link: { href: `/observe/evaluations?service=support-desk&evalCard=${EVAL}&range={range}` }
  });
  expect(driftSignal).toMatchObject({ state: 'nodata' });
  expect(driftSignal.subject).toContain('baseline_not_ready');
  expect(view.overview.default.assessment).toMatchObject({ title: '▲ VERIFICATION FAILING', tone: 'danger' });
  expect(view.overview.default.assessment.retry).toBeUndefined();
  expect(view.overview.default.channels).toEqual([
    { label: 'card', value: 'active' },
    { label: 'bindings', value: '1/2 ready' },
    { label: 'telemetry', value: '10 root spans · 6h' },
    { label: 'latest result', value: 'failed · 2026-10-01 12:00Z' }
  ]);
  expect(view.overview.default.context.activity).toBe('13 verification results in 30 days');
  expect(view.overview.default.attention.item?.href).toContain(`evalCard=${EVAL}`);

  // Composition is the exact declaration: components, Verifiers with readiness, on-failure Operators.
  const [runtime, measurement, reaction] = view.composition.lanes;
  expect(runtime.nodes.map((n) => [n.name, n.note])).toEqual([['triage', 'alias agent'], ['triage-prompt', 'alias prompt']]);
  expect(measurement.nodes.map((n) => [n.name, n.status])).toEqual([['answer-quality', '✓ ready'], ['latency-drift', '✕ baseline_not_ready']]);
  expect(reaction.nodes.map((n) => n.name)).toEqual(['page-oncall']);
  expect(view.composition.edges).toEqual([
    { from: AGENT, to: EVAL, label: 'verified by' },
    { from: EVAL, to: OPERATOR, label: 'on failure' },
    { from: '__service', to: DRIFT, label: 'verified by' }
  ]);

  // Definition: aliased rows, the two publication semantics, the raw spec.
  expect(view.definition.components.rows[0]).toMatchObject({ alias: 'agent', href: `/cards/${AGENT}`, publishes: 'answer-quality@1.0.0' });
  expect(view.definition.components.rows[1]).toMatchObject({ alias: 'prompt', publishes: '—' });
  expect(view.definition.publications.service.value).toBe('latency-drift@1.0.0');
  expect(JSON.parse(view.definition.rawSpec.yaml)).toEqual(service.card.spec);

  // Failure branches: denied, failed, schema-invalid and degraded reads are refused truthfully.
  const project = async (analytical: (q: Query) => unknown) =>
    ((await (await liveClient(analytical)).client.card(SVC)).presentation as import('../../core/types').ServicePresentation)
      .overview.default;

  // Denied telemetry and results: each section says so; nothing is zero or healthy.
  const refused = await project(() => denied);
  expect(refused.charts.map((c) => [c.state, c.code])).toEqual(Array(3).fill(['failed', 'WYRD_PERMISSION_403_DENIED_RBAC']));
  expect(refused.charts[0].note).toBe('not authorized to read traces — not zero');
  expect(refused.signals.map((s) => s.state)).toEqual(['unauthorized', 'unauthorized']);
  expect(refused.channels[2]).toEqual({ label: 'telemetry', value: 'not authorized' });

  // An unavailable upstream fails its section with its code and offers a retry.
  const failed = await project(() => unavailable);
  expect(failed.assessment).toMatchObject({ title: 'ASSESSMENT INCOMPLETE', retry: true });
  expect(failed.charts[0]).toMatchObject({ state: 'failed', code: 'WYRD_BIFROST_503_UNAVAILABLE', note: 'trace read failed — not zero' });
  expect(failed.signals[0]).toMatchObject({ verdict: { label: 'RESULTS UNAVAILABLE' }, context: 'results read failed · WYRD_BIFROST_503_UNAVAILABLE' });

  // A result missing a canonical column is a schema failure, never reinterpreted.
  const wrong = await project((q) => {
    const answer = healthy(q) as { fields: { name: string }[] };
    return { ...answer, fields: answer.fields.filter((f) => f.name !== 'errors' && f.name !== 'verdict') };
  });
  expect(wrong.charts[0]).toMatchObject({ state: 'failed', code: 'WYRD_UI_ANALYTICAL_SCHEMA' });
  expect(wrong.signals[0].context).toBe('results read failed · WYRD_UI_ANALYTICAL_SCHEMA');

  // A degraded terminal draws what was read and names the gap; no rows is an explicit no-data state.
  const degraded = await project((q) => ({ ...(healthy(q) as object), outcome: 'degraded' }));
  expect(degraded.charts[0]).toMatchObject({ state: 'gap', note: 'degraded read — some sources incomplete' });
  const empty = await project((q) => ({ ...(healthy(q) as object), rows: [] }));
  expect(empty.charts[0]).toMatchObject({ state: 'nodata', note: 'no root spans for support-desk 2.0.0 in 1h' });
  expect(empty.assessment.title).toBe('NO EVIDENCE YET');
});
