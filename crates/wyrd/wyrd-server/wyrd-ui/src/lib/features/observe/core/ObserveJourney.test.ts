// @vitest-environment jsdom
import { spawn, type ChildProcess } from 'node:child_process';
import { once } from 'node:events';
import { beforeAll, afterAll, expect, test } from 'vitest';

// The Observe user journey against the real dev server: URL-restored scope,
// cross-signal link preservation, drilldowns and truthful alternate states.
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
  return fetch(origin + path, { headers: { cookie } });
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

test('overview restores the shared scope and carries it into every signal link', async () => {
  const response = await get('/t/acme/observe?service=checkout-api&range=1h');
  expect(response.status).toBe(200);
  const html = await response.text();
  for (const signal of ['logs', 'metrics', 'traces', 'dashboards', 'evaluations', 'drift'])
    expect(html).toContain(`/t/acme/observe/${signal}?service=checkout-api&amp;range=1h`);
  expect(html).toContain('capture 504 from ledger-api');
  expect(html).toContain('groundedness 0.61');
  expect(html).toContain('Absence of a report is not a passing verdict');
}, 30_000);

test('traces search facets, render trend plus table, and keep return context through detail', async () => {
  const html = await (
    await get('/t/acme/observe/traces?service=checkout-api&status=error&range=1h')
  ).text();
  expect(html).toContain('trace_01');
  expect(html).toContain('POST /checkout/capture');
  expect(html).toContain('error traces per minute');
  // The filter bar is real controls, and a row opens the trace carrying the search.
  expect(html).toContain('all services');
  expect(html).toContain(
    '/t/acme/observe/traces/trace_04?service=checkout-api&amp;status=error&amp;range=1h'
  );
  // Detail restores the back search intact and shows the error span.
  const detail = await (
    await get('/t/acme/observe/traces/trace_01?service=checkout-api&status=error&range=1h')
  ).text();
  expect(detail).toContain('Back to traces');
  expect(detail).toContain('service=checkout-api&amp;range=1h&amp;status=error');
  expect(detail).toContain('ledger.capture');
  expect(detail).toContain('error edge: checkout-agent → ledger-api (504)');
  expect(detail).toContain('absent, not hidden');
  // Span selection is URL state on the same route.
  const span = await (
    await get('/t/acme/observe/traces/trace_01?service=checkout-api&range=1h&span=span_rank')
  ).text();
  expect(span).toContain('rank.candidates');
  expect(span).toContain('capture candidates');
});

test('metrics discover and filter a measure, then render chart plus underlying values', async () => {
  const html = await (await get('/t/acme/observe/metrics?service=checkout-api&range=6h')).text();
  expect(html).toContain('http.server.duration');
  expect(html).toContain('12:00–13:00');
  expect(html).toContain('18,211');
  const other = await (
    await get('/t/acme/observe/metrics?service=checkout-api&range=6h&metric=checkout.cart.value')
  ).text();
  expect(other).toContain('cart value distribution');
  expect(other).toContain('never omitted');
});

test('logs search and inspect a structured record and hand equivalent context to Query', async () => {
  const html = await (
    await get('/t/acme/observe/logs?service=checkout-api&level=error&range=1h&q=gateway')
  ).text();
  expect(html).toContain('capture declined: gateway timeout');
  expect(html).toContain('http.status');
  expect(html).toContain('Open in Query');
  expect(html).toMatch(/\/t\/acme\/query\?[^"]*sql=/);
  expect(html).toContain('trace_01');
  const selected = await (
    await get('/t/acme/observe/logs?service=checkout-api&level=error&range=1h&record=log_03')
  ).text();
  expect(selected).toContain('upstream 504 from ledger-api during capture');
});

test('dashboards stay read-only in inventory and detail, reusing shared chart framing', async () => {
  const inventory = await (await get('/t/acme/observe/dashboards')).text();
  expect(inventory).toContain('Checkout health');
  expect(inventory).toContain('No create, edit, alert or schedule action');
  const detail = await (await get('/t/acme/observe/dashboards/dashboard_01?range=24h')).text();
  expect(detail).toContain('read-only — no editor, no save, no');
  expect(detail).toContain('Capture outcome');
  expect(detail).toContain('No data for this range');
  expect(detail).toContain('WYRD-PANEL-QUERY-FAILED');
});

test('evaluation events restore filters from the URL and open the workflow drawer', async () => {
  const inventory = await (
    await get('/t/acme/observe/evaluations?service=checkout-agent&status=failed&range=24h')
  ).text();
  expect(inventory).toContain('eval_record_01');
  expect(inventory).not.toContain('eval_record_02');
  const offline = await (await get('/t/acme/observe/evaluations?origin=offline')).text();
  expect(offline).toContain('refund-partial-3');
  expect(offline).not.toContain('eval_record_01');
  const detail = await (
    await get('/t/acme/observe/evaluations/eval_record_01?task=groundedness')
  ).text();
  expect(detail).toContain('Checkout quality workflow');
  expect(detail).toContain('scored 0.61 vs');
  expect(detail).toContain('grounding checks failed');
  expect(detail).toContain('uncited');
  // Task selection is URL state; a redacting task states the withholding.
  const redacted = await (
    await get('/t/acme/observe/evaluations/eval_record_01?task=pii-leak')
  ).text();
  expect(redacted).toContain('Actual values withheld');
  // Offline events carry scenario identity instead of a registered subject.
  const scenario = await (await get('/t/acme/observe/evaluations/eval_record_04')).text();
  expect(scenario).toContain('customer-support-v2');
  expect(scenario).toContain('no registered Eval card');
});

test('drift renders the calculated report, feature selection, and honest absence', async () => {
  const html = await (
    await get('/t/acme/observe/drift?driftCard=card_drift_01&service=ranking-api&feature=score&range=30d')
  ).text();
  expect(html).toContain('Failed — score drifted');
  expect(html).toContain('0.31');
  expect(html).toContain('drift-response (Operator)');
  expect(html).toContain('calculated report, not a raw observation');
  const feature = await (await get('/t/acme/observe/drift?feature=cart_value&range=30d')).text();
  expect(feature).toContain('Score history — cart_value');
  const absent = await (await get('/t/acme/observe/drift?range=1h')).text();
  expect(absent).toContain('No report for this range');
  expect(absent).toContain('not a passing verdict');
});

test('alternate states never imply healthy data', async () => {
  // No-match keeps the search and filters.
  const noMatch = await (
    await get('/t/acme/observe/traces?service=checkout-api&range=1h&q=absent-trace')
  ).text();
  expect(noMatch).toContain('No matching traces');
  expect(noMatch).toContain('value="absent-trace"');
  expect(noMatch).not.toContain('trace_01');
  // Unknown drilldowns render safe absence, not a fabricated detail.
  const missingTrace = await (await get('/t/acme/observe/traces/trace_nope')).text();
  expect(missingTrace).toContain('WYRD_SPEC_404_NOT_FOUND');
  const missingEval = await (await get('/t/acme/observe/evaluations/eval_nope')).text();
  expect(missingEval).toContain('WYRD_SPEC_404_NOT_FOUND');
  const missingDashboard = await (await get('/t/acme/observe/dashboards/dashboard_nope')).text();
  expect(missingDashboard).toContain('WYRD_SPEC_404_NOT_FOUND');
  // Another tenant never sees acme's signals.
  const research = await (await get('/t/research/observe/logs')).text();
  expect(research).not.toContain('capture declined');
  // Unauthenticated requests never reach a signal page — they bounce to sign-in.
  const anonymous = await fetch(origin + '/t/acme/observe', { redirect: 'manual' });
  expect(anonymous.status).toBe(303);
  expect(anonymous.headers.get('location')).toBe('/');
});
