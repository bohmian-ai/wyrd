// @vitest-environment jsdom
import { spawn, type ChildProcess } from 'node:child_process';
import { once } from 'node:events';
import { beforeAll, afterAll, expect, test } from 'vitest';

// The shared Card detail journey against the real dev server: exact
// identity/version/metadata/relationships/Spec, read-only prior versions,
// the typed Spec fallback, and the earned Workflow and Verifier
// presentations inside the unchanged shell (TASK-006 scenarios 2 and 4).
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

/** The markers every Card kind's page must render from the shared shell. */
const SHELL = [
  'read-only projection of server truth',
  'Metadata',
  'server-managed',
  'Relationships',
  'server-derived',
  'Versions'
];

test('generic detail renders exact identity, version, metadata, relationships and Spec', async () => {
  const html = await (await get('/t/acme/cards/card_generic_01')).text();
  for (const marker of SHELL) expect(html).toContain(marker);
  expect(html).toContain('settlement-guardrails');
  expect(html).toContain('Policy');
  expect(html).toContain('v7 (current)');
  expect(html).toContain('card_generic_01 · apiVersion wyrd/v1');
  expect(html).toContain('block on violation');
  expect(html).toContain('raise retention to 90d');
  expect(html).toContain('?version=v6');
  expect(html).toContain('applies_to');
  expect(html).toContain('/t/acme/cards/card_service_06');
}, 30_000);

test('selecting a prior version renders that version’s declared Spec', async () => {
  const html = await (await get('/t/acme/cards/card_generic_01?version=v6')).text();
  expect(html).toContain('Viewing v6');
  expect(html).toContain('v7 is current');
  expect(html).toContain('2 declared · pci-isolation · retention-90d');
  expect(html).not.toContain('egress-allowlist');
  const missing = await get('/t/acme/cards/card_generic_01?version=v99');
  expect(missing.status).toBe(404);
});

test('a prior version without a projected declaration says so instead of lying', async () => {
  const html = await (await get('/t/acme/cards/card_workflow_01?version=v6')).text();
  expect(html).toContain('Viewing v6');
  expect(html).not.toContain('retrieve context');
  expect(html).toContain('The v6 declaration is not projected in this environment.');
});

test('Workflow renders its earned stage presentation inside the same shell', async () => {
  const html = await (await get('/t/acme/cards/card_workflow_01')).text();
  for (const marker of SHELL) expect(html).toContain(marker);
  for (const stage of ['retrieve context', 'score tasks', 'judge thresholds', 'publish record'])
    expect(html).toContain(stage);
  expect(html).toContain('no run results render here');
  expect(html).toContain('/t/acme/cards/card_eval_01');
});

test('Verifier renders its earned presentation inside the same shell', async () => {
  const html = await (await get('/t/acme/cards/card_verifier_01')).text();
  for (const marker of SHELL) expect(html).toContain(marker);
  expect(html).toContain('Accepted evidence');
  expect(html).toContain('TestRunEvidence');
  expect(html).toContain('base → candidate commit pair');
  expect(html).toContain('No secrets render on a Verifier Card');
  expect(html).toContain('/t/acme/changes/change_01/verification');
});

test('a kind without a workspace uses the generated typed Spec fallback', async () => {
  const html = await (await get('/t/acme/cards/card_mcp_01')).text();
  for (const marker of SHELL) expect(html).toContain(marker);
  expect(html).toContain('payments-mcp');
  expect(html).toContain('kind-owned sections');
});

test('an unknown Card is a structured 404, never a blank shell', async () => {
  const html = await (await get('/t/acme/cards/card_nope')).text();
  expect(html).toContain('WYRD_SPEC_404_NOT_FOUND');
});
