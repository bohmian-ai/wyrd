// @vitest-environment node
import { expect, test, vi } from 'vitest';
import { LocalSessions, sessions, sessionLifetime } from './session';
import { handle } from '../../../hooks.server';
import { env } from '$env/dynamic/private';
import { ServerSessions } from './server-sessions';

vi.mock('$env/dynamic/private', () => ({
  env: { WYRD_UI_LOCAL_AUTH: 'true', WYRD_BFF_SERVICE_KEY: 'bff-key' }
}));

test('missing, forged, expired and revoked sessions fail closed', () => {
  const sessions = new LocalSessions();
  expect(sessions.read(undefined, 0)).toMatchObject({ problem: { status: 401 } });
  expect(sessions.read('forged', 0)).toMatchObject({ problem: { status: 401 } });
  const id = sessions.create(100);
  expect(sessions.read(id, 100).session?.subject.name).toBe('Jordan Reyes');
  expect(sessions.read(id, 100 + 15 * 60_000)).toMatchObject({
    problem: { code: 'WYRD_AUTH_401_TOKEN_EXPIRED' }
  });
  const next = sessions.create(200);
  sessions.remove(next);
  expect(sessions.read(next, 200).session).toBeNull();
});

test('cookie is opaque and page metadata excludes session authority', () => {
  const sessions = new LocalSessions();
  const id = sessions.create();
  expect(id).toMatch(/^[a-f0-9]{64}$/);
  const { session } = sessions.read(id);
  expect(session).not.toBeNull();
  const safe = sessions.metadata(session!);
  expect(Object.keys(safe).sort()).toEqual(['csrf', 'expiresAt', 'subject', 'tenants']);
  expect(JSON.stringify(safe)).not.toContain(id);
  expect(JSON.stringify(safe)).not.toContain('tenantId');
  expect(safe.csrf).not.toBe(id);
});

test('request hook rejects expiry and disabled local identity before tenant loads execute', async () => {
  const resolve = vi.fn();
  const expired = sessions.create(Date.now() - sessionLifetime);
  const cookies = {
    get: vi.fn((name: string) => (name === 'wyrd_session' ? expired : undefined)),
    delete: vi.fn()
  };
  const event = {
    locals: {},
    params: { tenantKey: 'acme' },
    route: { id: '/t/[tenantKey]' },
    cookies,
    setHeaders: vi.fn(),
    request: new Request('http://localhost/t/acme', { method: 'POST' }),
    isDataRequest: false
  } as unknown as Parameters<typeof handle>[0]['event'];
  await expect(handle({ event, resolve })).rejects.toMatchObject({
    status: 401,
    body: { code: 'WYRD_AUTH_401_TOKEN_EXPIRED' }
  });
  expect(resolve).not.toHaveBeenCalled();
  expect(cookies.delete).toHaveBeenCalledWith('wyrd_session', { path: '/' });
  // A plain page load with a dead session is a normal flow — it goes to sign-in.
  const pageEvent = {
    ...event,
    request: new Request('http://localhost/t/acme', { method: 'GET' })
  } as unknown as Parameters<typeof handle>[0]['event'];
  await expect(handle({ event: pageEvent, resolve })).rejects.toMatchObject({
    status: 303,
    location: '/'
  });
  expect(resolve).not.toHaveBeenCalled();
  const id = sessions.create();
  cookies.get.mockImplementation((name: string) => (name === 'wyrd_session' ? id : undefined));
  env.WYRD_UI_LOCAL_AUTH = 'false';
  try {
    await expect(handle({ event, resolve })).rejects.toMatchObject({ status: 401 });
    expect(resolve).not.toHaveBeenCalled();
  } finally {
    env.WYRD_UI_LOCAL_AUTH = 'true';
    sessions.remove(id);
  }
});

/** In-memory cookie jar standing in for SvelteKit's request cookies. */
function jar(initial: Record<string, string> = {}) {
  const values = new Map(Object.entries(initial));
  return {
    values,
    get: (name: string) => values.get(name),
    getAll: () => [...values].map(([name, value]) => ({ name, value })),
    set: (name: string, value: string) => void values.set(name, value),
    delete: (name: string) => void values.delete(name)
  } as unknown as import('@sveltejs/kit').Cookies & { values: Map<string, string> };
}

const sessionId = 'a'.repeat(64);
const csrf = 'c'.repeat(64);
const read = (tenantKey: string, expiresAt = new Date(Date.now() + 60_000).toISOString()) =>
  Response.json({
    tenant_key: tenantKey,
    tenant_name: 'Acme',
    principal_id: '01990000-0000-7000-8000-000000000001',
    roles: ['admin'],
    permissions: ['identity_connections:write'],
    expires_at: expiresAt,
    csrf_token: csrf
  });
const action = (body: { origin?: string } = {}) =>
  new Request('http://localhost/t/acme/settings?/stage', {
    method: 'POST',
    headers: body.origin === undefined ? { origin: 'http://localhost' } : { origin: body.origin }
  });

test('production session rejects cross-tenant and missing CSRF', async () => {
  const calls: { url: string; init: RequestInit }[] = [];
  const fetcher = vi.fn(async (url: URL | RequestInfo, init?: RequestInit) => {
    calls.push({ url: String(url), init: init! });
    return read('other');
  }) as unknown as typeof fetch;
  const sessions = new ServerSessions(fetcher);
  const cookies = jar({ wyrd_session_acme: sessionId });

  // The cookie name is only a hint: the server-returned tenant must equal the path tenant.
  await expect(sessions.read('acme', cookies)).rejects.toMatchObject({ status: 403 });
  expect(cookies.values.has('wyrd_session_acme')).toBe(false);
  expect(calls[0].url).toBe('http://127.0.0.1:8080/internal/bff/v1/sessions/read');
  expect(new Headers(calls[0].init.headers).get('x-wyrd-bff-key')).toBe('bff-key');
  expect(JSON.parse(String(calls[0].init.body))).toEqual({ session_id: sessionId });
  // A tenant key that is not a route slug never reaches the server.
  await expect(sessions.read('../acme', cookies)).rejects.toMatchObject({ status: 404 });
  await expect(sessions.api('acme', cookies, 'GET', '/x')).rejects.toMatchObject({
    status: 401
  });
  expect(calls).toHaveLength(1);

  const live = new ServerSessions(
    vi.fn(async () => read('acme')) as unknown as typeof fetch
  );
  const session = (await live.read('acme', jar({ wyrd_session_acme: sessionId })))!;
  expect(session).toMatchObject({ tenantKey: 'acme', csrf });
  expect(JSON.stringify(session)).not.toContain(sessionId);
  expect(() => live.checkAction(session, action(), null)).toThrow(
    expect.objectContaining({ status: 403 })
  );
  expect(() => live.checkAction(session, action(), 'd'.repeat(64))).toThrow(
    expect.objectContaining({ status: 403 })
  );
  expect(() => live.checkAction(session, action({ origin: 'http://evil.test' }), csrf)).toThrow(
    expect.objectContaining({ status: 403 })
  );
  expect(() => live.checkAction(session, action(), csrf)).not.toThrow();
});

test('production session expires and logs out', async () => {
  const paths: string[] = [];
  let reads = 0;
  const fetcher = vi.fn(async (url: URL | RequestInfo) => {
    const path = new URL(String(url)).pathname;
    paths.push(path);
    if (path.endsWith('/sessions/read'))
      return reads++ === 0 ? read('acme') : new Response(null, { status: 401 });
    if (path.endsWith('/sessions/logout')) return new Response(null, { status: 204 });
    return new Response(null, { status: 500 });
  }) as unknown as typeof fetch;
  const sessions = new ServerSessions(fetcher);
  const cookies = jar({ wyrd_session_acme: sessionId });

  const session = (await sessions.read('acme', cookies))!;
  expect(() => sessions.checkAction({ ...session, expiresAt: Date.now() - 1 }, action(), csrf)).toThrow(
    expect.objectContaining({ status: 401, body: expect.objectContaining({ code: 'WYRD_AUTH_401_TOKEN_EXPIRED' }) })
  );

  // The server no longer honours the session: no session, cookie cleared.
  expect(await sessions.read('acme', cookies)).toBeNull();
  expect(cookies.values.has('wyrd_session_acme')).toBe(false);

  reads = 0;
  const live = jar({ wyrd_session_acme: sessionId });
  await sessions.logout('acme', live, action(), csrf);
  expect(paths.at(-1)).toBe('/internal/bff/v1/sessions/logout');
  expect(live.values.has('wyrd_session_acme')).toBe(false);
  // Logging out again is a no-op that still leaves no cookie.
  await sessions.logout('acme', live, action(), csrf);
  expect(paths.filter((path) => path.endsWith('/logout'))).toHaveLength(1);
});
