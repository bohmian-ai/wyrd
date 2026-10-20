// @vitest-environment node
import { expect, test, vi } from 'vitest';
import { env } from '$env/dynamic/private';

vi.mock('$env/dynamic/private', () => ({ env: { WYRD_SERVER_URL: 'https://wyrd.internal:8443' } }));

const { forwardAuth, serverReady, serverUrl } = await import('./upstream');

/** One forwarded call as the configured server received it. */
type Forwarded = { url: string; init: RequestInit };

/** Fetcher that records each forwarded call and answers with `answer`. */
function upstream(answer: () => Response) {
  const calls: Forwarded[] = [];
  const fetcher = (async (url: URL, init: RequestInit) => {
    calls.push({ url: url.toString(), init });
    return answer();
  }) as unknown as typeof fetch;
  return { calls, fetcher };
}

test('auth forwarding sends the browser request to the configured server under the established origin', async () => {
  const { calls, fetcher } = upstream(() => new Response('{}'));
  const request = new Request('http://127.0.0.1:3000/auth/token?x=1', {
    method: 'POST',
    headers: {
      cookie: 'wyrd_session_acme=s; wyrd_login=abc; theme=dark',
      'content-type': 'application/x-www-form-urlencoded',
      host: 'evil.test',
      'x-forwarded-host': 'evil.test',
      'x-forwarded-proto': 'https',
      forwarded: 'host=evil.test',
      'accept-encoding': 'gzip'
    },
    body: 'grant_type=refresh_token'
  });
  await forwardAuth(request, new URL('http://ui.example:3000/auth/token?x=1'), fetcher);
  expect(calls).toHaveLength(1);
  const [{ url, init }] = calls;
  expect(url).toBe('https://wyrd.internal:8443/auth/token?x=1');
  expect(init.method).toBe('POST');
  expect(init.redirect).toBe('manual');
  expect(await new Response(init.body).text()).toBe('grant_type=refresh_token');
  const headers = new Headers(init.headers);
  expect(headers.get('cookie')).toBe('wyrd_login=abc');
  expect(headers.get('content-type')).toBe('application/x-www-form-urlencoded');
  expect(headers.get('x-forwarded-host')).toBe('ui.example:3000');
  expect(headers.get('x-forwarded-proto')).toBe('http');
  for (const name of ['host', 'forwarded', 'accept-encoding']) expect(headers.has(name)).toBe(false);
});

test('public browser-session operations carry the Wyrd session cookies and nothing else', async () => {
  const cookie = 'wyrd_session_acme=a.1; theme=dark; wyrd_session_beta=b.2; wyrd_login=abc';
  const { calls, fetcher } = upstream(() => new Response('{}'));
  const read = new URL('http://ui.example/auth/browser/session?tenant=acme');
  await forwardAuth(new Request(read, { headers: { cookie } }), read, fetcher);
  const logout = new URL('http://ui.example/auth/browser/logout');
  await forwardAuth(
    new Request(logout, {
      method: 'POST',
      headers: { cookie, origin: 'http://ui.example', 'content-type': 'application/json' },
      body: '{"tenant":"acme"}'
    }),
    logout,
    fetcher
  );
  expect(calls.map(({ url }) => url)).toEqual([
    'https://wyrd.internal:8443/auth/browser/session?tenant=acme',
    'https://wyrd.internal:8443/auth/browser/logout'
  ]);
  for (const { init } of calls)
    expect(new Headers(init.headers).get('cookie')).toBe(
      'wyrd_session_acme=a.1; wyrd_session_beta=b.2; wyrd_login=abc'
    );
  expect(new Headers(calls[1].init.headers).get('origin')).toBe('http://ui.example');
  expect(await new Response(calls[1].init.body).text()).toBe('{"tenant":"acme"}');
});

test('auth forwarding returns redirects, cookies, and errors unchanged without following', async () => {
  const redirect = upstream(() => {
    const headers = new Headers({ location: 'https://idp.example/authorize?state=s' });
    headers.append('set-cookie', 'a=1; HttpOnly');
    headers.append('set-cookie', 'b=2; HttpOnly');
    return new Response(null, { status: 303, headers });
  });
  const url = new URL('http://ui.example/auth/callback?state=s');
  const moved = await forwardAuth(new Request(url), url, redirect.fetcher);
  expect(moved.status).toBe(303);
  expect(moved.headers.get('location')).toBe('https://idp.example/authorize?state=s');
  expect(moved.headers.getSetCookie()).toEqual(['a=1; HttpOnly', 'b=2; HttpOnly']);
  expect(redirect.calls[0].init.body).toBeUndefined();

  const refusal = upstream(
    () =>
      new Response('{"error":"invalid_request"}', {
        status: 400,
        headers: { 'content-type': 'application/json' }
      })
  );
  const refused = await forwardAuth(new Request(url), url, refusal.fetcher);
  expect(refused.status).toBe(400);
  expect(await refused.json()).toEqual({ error: 'invalid_request' });
});

test('an unreachable or misconfigured server is the upstream problem', async () => {
  const url = new URL('http://ui.example/auth/callback');
  const down = (async () => {
    throw new TypeError('connection refused');
  }) as unknown as typeof fetch;
  const failed = await forwardAuth(new Request(url), url, down);
  expect(failed.status).toBe(502);
  expect((await failed.json()).code).toBe('WYRD_SPEC_502_UPSTREAM_FAILURE');
  try {
    env.WYRD_SERVER_URL = 'http://wyrd.internal:8080';
    const { calls, fetcher } = upstream(() => new Response('{}'));
    expect((await forwardAuth(new Request(url), url, fetcher)).status).toBe(502);
    expect(calls).toEqual([]);
  } finally {
    env.WYRD_SERVER_URL = 'https://wyrd.internal:8443';
  }
});

/** Fetcher that records every URL it is asked for and answers ready. */
function recording() {
  const seen: string[] = [];
  const fetcher = (async (url: URL) => {
    seen.push(url.toString());
    return new Response('ok');
  }) as unknown as typeof fetch;
  return { seen, fetcher };
}

test('readiness asks the configured server and maps failures to not ready', async () => {
  const { seen, fetcher } = recording();
  expect(await serverReady(fetcher)).toBe(true);
  expect(seen).toEqual(['https://wyrd.internal:8443/readyz']);
  const unready = (async () => new Response('', { status: 503 })) as unknown as typeof fetch;
  expect(await serverReady(unready)).toBe(false);
  const down = (async () => {
    throw new TypeError('connection refused');
  }) as unknown as typeof fetch;
  expect(await serverReady(down)).toBe(false);
});

test('https server origin is accepted', () => {
  env.WYRD_SERVER_URL = 'https://wyrd.internal:8443';
  expect(serverUrl()).toBe('https://wyrd.internal:8443');
});

test('loopback http server origin is accepted', () => {
  try {
    const origins = ['http://localhost:8080', 'http://127.0.0.1:8080', 'http://127.9.8.7', 'http://[::1]:8080'];
    for (const origin of origins) {
      env.WYRD_SERVER_URL = origin;
      expect(serverUrl()).toBe(origin);
    }
    delete env.WYRD_SERVER_URL;
    expect(serverUrl()).toBe('http://127.0.0.1:8080');
  } finally {
    env.WYRD_SERVER_URL = 'https://wyrd.internal:8443';
  }
});

test('non-loopback http server origin is refused before fetch', async () => {
  try {
    const origins = [
      'http://wyrd.internal:8080',
      'http://127.0.0.1.evil.test',
      'http://10.0.0.5',
      'ftp://wyrd.internal',
      'not a url'
    ];
    for (const origin of origins) {
      env.WYRD_SERVER_URL = origin;
      expect(() => serverUrl()).toThrow();
      const { seen, fetcher } = recording();
      expect(await serverReady(fetcher)).toBe(false);
      expect(seen).toEqual([]);
    }
  } finally {
    env.WYRD_SERVER_URL = 'https://wyrd.internal:8443';
  }
});

test('empty upstream value is refused before fetch', async () => {
  try {
    env.WYRD_SERVER_URL = '';
    expect(() => serverUrl()).toThrow('WYRD_SERVER_URL is not a valid URL');
    const { seen, fetcher } = recording();
    expect(await serverReady(fetcher)).toBe(false);
    expect(seen).toEqual([]);
  } finally {
    env.WYRD_SERVER_URL = 'https://wyrd.internal:8443';
  }
});
