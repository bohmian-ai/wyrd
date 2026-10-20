// @vitest-environment node
import { afterEach, expect, test, vi } from 'vitest';
import type { Cookies } from '@sveltejs/kit';
import { BrowserSessions } from './browser-sessions';

vi.mock('$env/dynamic/private', () => ({
  env: { WYRD_SERVER_URL: 'http://127.0.0.1:8080' }
}));

afterEach(() => vi.unstubAllGlobals());

const url = new URL('https://wyrd.example/t/acme');

/** A SvelteKit cookie jar over a map, recording each `set` and `delete` option. */
function jar(initial: Record<string, string> = {}) {
  const values = new Map(Object.entries(initial));
  const options = new Map<string, Record<string, unknown>>();
  const deleted: string[] = [];
  const cookies = {
    get: (name: string) => values.get(name),
    getAll: () => [...values].map(([name, value]) => ({ name, value })),
    set: (name: string, value: string, opts: Record<string, unknown>) => {
      values.set(name, value);
      options.set(name, opts);
    },
    delete: (name: string) => {
      values.delete(name);
      deleted.push(name);
    }
  } as unknown as Cookies;
  return { cookies, values, options, deleted };
}

/** Stub `fetch` with `answer` and record each request. */
function server(answer: (request: Request) => Response) {
  const requests: Request[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: URL, init: RequestInit) => {
      const request = new Request(input, init);
      requests.push(request);
      return answer(request);
    })
  );
  return requests;
}

const view = {
  tenant: 'acme',
  tenant_id: 't-1',
  tenant_name: 'Acme',
  principal_id: 'p-1',
  principal_kind: 'user',
  credential: 'sso',
  permissions: ['cards:read'],
  expires_at: '2026-10-20T00:00:00Z'
};

test('sign-in relays the server login cookie and returns its provider URL', async () => {
  const requests = server(
    () =>
      new Response(JSON.stringify({ authorization_url: 'https://idp.example/authorize?state=s' }), {
        headers: {
          'set-cookie':
            'wyrd_login=binding; Path=/auth/callback; Max-Age=300; HttpOnly; SameSite=Lax; Secure'
        }
      })
  );
  const browser = jar({ wyrd_session_other: 'never-forwarded' });

  const target = await new BrowserSessions().begin('acme', url, browser.cookies);

  expect(target).toBe('https://idp.example/authorize?state=s');
  expect(requests[0].url).toBe('http://127.0.0.1:8080/auth/browser/login');
  expect(requests[0].headers.get('origin')).toBe('https://wyrd.example');
  expect(requests[0].headers.get('cookie')).toBeNull();
  expect(await requests[0].json()).toEqual({ tenant: 'acme' });
  expect(browser.values.get('wyrd_login')).toBe('binding');
  expect(browser.options.get('wyrd_login')).toMatchObject({
    path: '/auth/callback',
    maxAge: 300,
    httpOnly: true,
    secure: true,
    sameSite: 'lax'
  });
});

test('reading a session forwards only that tenant cookie and the trusted origin', async () => {
  const requests = server(() => Response.json({ sessions: [view] }));
  const browser = jar({ wyrd_session_acme: 'a.1', wyrd_session_other: 'b.2', theme: 'dark' });

  const session = await new BrowserSessions().read('acme', url, browser.cookies);

  expect(session?.context().tenant).toEqual({
    key: 'acme',
    name: 'Acme',
    tenantId: 't-1',
    permissions: ['cards:read']
  });
  expect(requests[0].url).toBe('http://127.0.0.1:8080/auth/browser/session?tenant=acme');
  expect(requests[0].headers.get('cookie')).toBe('wyrd_session_acme=a.1');
  expect(requests[0].headers.get('x-wyrd-access-token')).toBeNull();

  await session!.api('POST', '/cards', { name: 'c' });
  expect(requests[1].url).toBe('http://127.0.0.1:8080/v1/cards');
  expect(requests[1].headers.get('cookie')).toBe('wyrd_session_acme=a.1');
  expect(requests[1].headers.get('origin')).toBe('https://wyrd.example');
  expect(requests[1].headers.get('x-forwarded-host')).toBe('wyrd.example');
  expect(requests[1].headers.get('x-forwarded-proto')).toBe('https');
});

test('a session the server refuses is cleared; an outage is the upstream problem', async () => {
  server(
    () =>
      new Response(JSON.stringify({ code: 'WYRD_AUTH_401_INVALID_TOKEN' }), {
        status: 401,
        headers: { 'set-cookie': 'wyrd_session_acme=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax' }
      })
  );
  const refused = jar({ wyrd_session_acme: 'dead', wyrd_session_other: 'live' });
  expect(await new BrowserSessions().read('acme', url, refused.cookies)).toBeNull();
  expect(refused.values.has('wyrd_session_acme')).toBe(false);
  expect(refused.values.get('wyrd_session_other')).toBe('live');

  server(() => new Response(null, { status: 503 }));
  const down = jar({ wyrd_session_acme: 'maybe-live' });
  await expect(new BrowserSessions().read('acme', url, down.cookies)).rejects.toMatchObject({
    status: 502
  });
  expect(down.values.get('wyrd_session_acme')).toBe('maybe-live');

  const none = server(() => Response.json({ sessions: [view] }));
  expect(await new BrowserSessions().read('acme', url, jar().cookies)).toBeNull();
  expect(none).toHaveLength(0);
});

test('logout revokes through the server and a failed logout is reported', async () => {
  const requests = server(
    () =>
      new Response(null, {
        status: 204,
        headers: { 'set-cookie': 'wyrd_session_acme=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax' }
      })
  );
  const browser = jar({ wyrd_session_acme: 'a.1', wyrd_session_other: 'b.2' });
  await new BrowserSessions().logout('acme', url, browser.cookies);
  expect(requests[0].url).toBe('http://127.0.0.1:8080/auth/browser/logout');
  expect(requests[0].headers.get('cookie')).toBe('wyrd_session_acme=a.1');
  expect(await requests[0].json()).toEqual({ tenant: 'acme' });
  expect(browser.values.has('wyrd_session_acme')).toBe(false);
  expect(browser.values.get('wyrd_session_other')).toBe('b.2');

  server(() => new Response(null, { status: 503 }));
  const kept = jar({ wyrd_session_acme: 'a.1' });
  await expect(new BrowserSessions().logout('acme', url, kept.cookies)).rejects.toMatchObject({
    status: 502
  });
});

test('an unreachable server is the upstream problem and tenant keys are validated', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => {
      throw new TypeError('fetch failed');
    })
  );
  await expect(new BrowserSessions().begin('acme', url, jar().cookies)).rejects.toMatchObject({
    status: 502
  });
  await expect(new BrowserSessions().begin('Bad!', url, jar().cookies)).rejects.toMatchObject({
    status: 404
  });
});
