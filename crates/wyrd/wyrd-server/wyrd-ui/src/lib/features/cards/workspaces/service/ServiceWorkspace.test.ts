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
  expect(html).toContain('MODEL-DRIFT (Drift v3)');
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
  expect(html).toContain('MODEL-DRIFT (Drift v3)');
  expect(html).toContain('✕ BREACHED');
  expect(html).toContain('subject ranker (model_primary · Model v12)');
  expect(html).toContain('PSI 0.27');
  expect(html).toContain('threshold 0.20');
  expect(html).toContain('baseline txns-2026q3 (Data v3)');
  expect(html).toContain('/t/acme/observe/drift?service=checkout-api&amp;driftCard=card_drift_01');
  expect(html).toContain('CHECKOUT-AGENT-EVAL (Eval v4)');
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
  expect(selected).toContain('SELECTED — MODEL-DRIFT (DRIFT · V3)');
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
