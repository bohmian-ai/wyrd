// @vitest-environment node
import { spawn, type ChildProcess } from 'node:child_process';
import { once } from 'node:events';
import { beforeAll, afterAll, expect, test } from 'vitest';

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

test('real browser-facing loads and actions contain credentials, enforce CSRF, and isolate tenant tabs', async () => {
  const entry = await fetch(origin);
  const entryHtml = await entry.text();
  expect(entryHtml).toContain('Sign in with SSO');
  expect(entryHtml).not.toContain('Research');
  expect(entryHtml).toContain('Mock data: On');
  const unauthenticated = await fetch(`${origin}/t/acme`, { redirect: 'manual' });
  expect(unauthenticated.status).toBe(303);
  expect(unauthenticated.headers.get('location')).toBe('/');
  const login = await fetch(`${origin}/?/login`, {
    method: 'POST',
    headers: {
      accept: 'text/html',
      origin,
      cookie: 'wyrd_ui_login_scenario=multiple',
      'content-type': 'application/x-www-form-urlencoded'
    },
    body: '',
    redirect: 'manual'
  });
  expect(login.status).toBe(303);
  const setCookie = login.headers.get('set-cookie')!;
  expect(setCookie).toContain('HttpOnly');
  expect(setCookie).toContain('SameSite=Lax');
  const cookie = setCookie.split(';')[0];
  const chooser = await fetch(origin, { headers: { cookie } });
  const html = await chooser.text();
  expect(html).toContain('Choose a tenant');
  expect(html).not.toContain(cookie.split('=')[1]);
  const csrf = html.match(/name="csrf" value="([a-f0-9]+)"/)![1];
  const post = (body: URLSearchParams, requestOrigin = origin) =>
    fetch(`${origin}/?/switch`, {
      method: 'POST',
      headers: { accept: 'text/html', cookie, origin: requestOrigin },
      body,
      redirect: 'manual'
    });
  expect((await post(new URLSearchParams({ tenantKey: 'research' }))).status).toBe(403);
  expect(
    (await post(new URLSearchParams({ tenantKey: 'research', csrf }), 'https://other.example'))
      .status
  ).toBe(403);
  const switched = await post(new URLSearchParams({ tenantKey: 'research', csrf }));
  expect(switched.status).toBe(303);
  expect(switched.headers.get('location')).toBe('/t/research');
  const otherTab = await fetch(`${origin}/t/acme`, { headers: { cookie } });
  expect(otherTab.status).toBe(200);
  const home = await otherTab.text();
  expect(home).toContain('Recent changes');
  expect(home).toContain('Recently viewed Cards');
  expect(home).toContain('Raise checkout ranking cutoff');
  expect(home).toContain('checkout-api');
  expect(home).toContain('Pick work back up');
  const toggle = (enabled: string, requestOrigin = origin) =>
    fetch(`${origin}/?/mockData`, {
      method: 'POST',
      headers: { accept: 'text/html', cookie, origin: requestOrigin },
      body: new URLSearchParams({ enabled, csrf, returnTo: '/t/acme' }),
      redirect: 'manual'
    });
  expect((await toggle('false', 'https://other.example')).status).toBe(403);
  const off = await toggle('false');
  expect(off.status).toBe(303);
  expect(off.headers.get('location')).toBe('/t/acme');
  const mockCookie = off.headers.get('set-cookie')!.split(';')[0];
  const liveHome = await fetch(`${origin}/t/acme`, {
    headers: { cookie: `${cookie}; ${mockCookie}` }
  });
  const liveHtml = await liveHome.text();
  expect(liveHtml).toContain('Mock data: Off');
  expect(liveHtml).toContain('server connection is not implemented yet');
  expect(liveHtml).not.toContain('Raise checkout ranking cutoff');
  expect((await toggle('true')).status).toBe(303);
  const logoSource = home.match(/<img src="([^"]+)"/)![1];
  const logo = await fetch(new URL(logoSource, origin));
  expect(logo.ok).toBe(true);
  expect(logo.headers.get('content-type')).toContain('image/svg+xml');
  const iconSource = home.match(/<link rel="icon" type="image\/svg\+xml" href="([^"]+)"/)![1];
  const icon = await fetch(new URL(iconSource, origin));
  expect(icon.ok).toBe(true);
  expect(icon.headers.get('content-type')).toContain('image/svg+xml');
  expect(await icon.text()).toContain('<svg');
  const research = await fetch(`${origin}/t/research`, { headers: { cookie } });
  expect(await research.text()).toContain('Nothing needs you right now');
  const denied = await fetch(`${origin}/t/unknown`, { headers: { cookie } });
  expect(denied.status).toBe(403);
  const deniedHtml = await denied.text();
  expect(deniedHtml).toContain('WYRD_PERMISSION_403_DENIED_RBAC');
  expect(deniedHtml).not.toContain('Raise checkout ranking cutoff');
  const logout = await fetch(`${origin}/?/logout`, {
    method: 'POST',
    headers: { accept: 'text/html', cookie, origin },
    body: new URLSearchParams({ csrf }),
    redirect: 'manual'
  });
  expect(logout.status).toBe(303);
  const afterLogout = await fetch(`${origin}/t/acme`, { headers: { cookie }, redirect: 'manual' });
  expect(afterLogout.status).toBe(303);
  expect(afterLogout.headers.get('location')).toBe('/');
}, 30_000);

test('mock SSO resolves configured organization and dev access scenarios without anonymous tenant discovery', async () => {
  const post = (action: string, values: Record<string, string>, cookie = '') =>
    fetch(`${origin}/?/${action}`, {
      method: 'POST',
      headers: { origin, accept: 'text/html', cookie },
      body: new URLSearchParams(values),
      redirect: 'manual'
    });
  const entry = await (await fetch(origin)).text();
  expect(entry).toContain('Sign in with SSO');
  expect(entry).not.toContain('Jordan Reyes');
  expect(entry).not.toContain('Research');
  const login = await post('login', {});
  expect(login.status).toBe(303);
  expect(login.headers.get('location')).toBe('/t/acme');
  const cookie = login.headers.get('set-cookie')!.split(';')[0];
  expect((await fetch(`${origin}/t/research`, { headers: { cookie } })).status).toBe(403);
  const home = await (await fetch(`${origin}/t/acme`, { headers: { cookie } })).text();
  const csrf = home.match(/name="csrf" value="([a-f0-9]+)"/)![1];
  expect((await post('loginScenario', { scenario: 'none' }, cookie)).status).toBe(403);
  const scenario = await post('loginScenario', { scenario: 'none', csrf }, cookie);
  expect(scenario.status).toBe(303);
  const revoked = await fetch(`${origin}/t/acme`, { headers: { cookie }, redirect: 'manual' });
  expect(revoked.status).toBe(303);
  expect(revoked.headers.get('location')).toBe('/');
  const selection = scenario.headers
    .getSetCookie()
    .find((value) => value.startsWith('wyrd_ui_login_scenario='))!
    .split(';')[0];
  const noAccess = await post('login', {}, selection);
  expect(noAccess.headers.get('location')).toBe('/');
  const noAccessCookie = noAccess.headers.get('set-cookie')!.split(';')[0];
  const denied = await (await fetch(origin, { headers: { cookie: noAccessCookie } })).text();
  expect(denied).toContain('No tenant access');
  expect(denied).not.toContain('Research');
  const disabled = await post('login', {}, 'wyrd_ui_mock_data=false');
  expect(disabled.status).toBe(502);
  expect(disabled.headers.get('set-cookie')).toBeNull();
  const unavailable = await disabled.text();
  expect(unavailable).toContain('SSO connection is not connected');
  expect(unavailable).toContain('data-state="error"');
  expect(unavailable).not.toContain('data-state="unauthorized"');
  expect(unavailable).not.toContain('Continue to Wyrd');
  expect((await post('loginScenario', { scenario: 'arbitrary' })).status).toBe(403);
});
