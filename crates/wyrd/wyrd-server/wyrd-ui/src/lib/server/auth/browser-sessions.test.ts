// @vitest-environment node
import { afterEach, expect, test, vi } from 'vitest';
import type { Cookies } from '@sveltejs/kit';
import * as client from 'openid-client';
import { BrowserSessions } from './browser-sessions';

vi.mock('$env/dynamic/private', () => ({
  env: { WYRD_UI_CLIENT_SECRET: 'test-client-secret', WYRD_SERVER_URL: 'http://127.0.0.1:8080' }
}));
vi.mock('openid-client', async (actual) => ({
  ...(await actual<typeof import('openid-client')>()),
  discovery: vi.fn(async () => ({})),
  buildAuthorizationUrl: vi.fn(() => new URL('https://wyrd.example/auth/authorize')),
  authorizationCodeGrant: vi.fn(),
  refreshTokenGrant: vi.fn(),
  tokenRevocation: vi.fn()
}));

afterEach(() => vi.clearAllMocks());

/** A terminal OAuth refusal from the token endpoint. */
const invalidGrant = () =>
  new client.ResponseBodyError('invalid_grant', {
    cause: { error: 'invalid_grant' },
    response: new Response(null, { status: 400 })
  });

const url = new URL('https://wyrd.example/login/callback?code=c&state=s');

/** An unsigned access token whose claims the BFF projects; `exp` is `lifetime` seconds away. */
function accessToken(lifetime: number): string {
  const part = (value: object) => Buffer.from(JSON.stringify(value)).toString('base64url');
  return `${part({ alg: 'none' })}.${part({
    exp: Math.floor(Date.now() / 1000) + lifetime,
    principal: { id: 'p-1', tenant_id: 't-1' },
    permissions: []
  })}.`;
}

/** A SvelteKit cookie jar over a map, recording each `set` option. */
function jar() {
  const values = new Map<string, string>();
  const options = new Map<string, Record<string, unknown>>();
  const cookies = {
    get: (name: string) => values.get(name),
    getAll: () => [...values].map(([name, value]) => ({ name, value })),
    set: (name: string, value: string, opts: Record<string, unknown>) => {
      values.set(name, value);
      options.set(name, opts);
    },
    delete: (name: string) => void values.delete(name)
  } as unknown as Cookies;
  return { cookies, values, options };
}

/** Sign in to `acme` through the code grant, with the login cookie `begin` set. */
async function signIn(sessions: BrowserSessions, refreshToken: string, lifetime = 60) {
  const browser = jar();
  await sessions.begin('acme', url, browser.cookies);
  vi.mocked(client.authorizationCodeGrant).mockResolvedValueOnce({
    access_token: accessToken(lifetime),
    refresh_token: refreshToken,
    token_type: 'bearer'
  } as Awaited<ReturnType<typeof client.authorizationCodeGrant>>);
  expect(await sessions.complete(url, browser.cookies)).toBe('/t/acme');
  return browser;
}

test('accepts an opaque refresh token and forwards it unchanged', async () => {
  const sessions = new BrowserSessions();
  const opaque = 'opaque-refresh-token-not-a-jwt';
  const browser = await signIn(sessions, opaque, 1);
  expect(browser.values.has('wyrd_session_acme')).toBe(true);
  expect(browser.values.get('wyrd_session_acme')).not.toContain(opaque);
  expect(browser.options.get('wyrd_session_acme')?.maxAge).toBe(12 * 60 * 60);

  // The cached access token is inside its renewal margin, so the read refreshes.
  vi.mocked(client.refreshTokenGrant).mockResolvedValueOnce({
    access_token: accessToken(60),
    token_type: 'bearer'
  } as Awaited<ReturnType<typeof client.refreshTokenGrant>>);
  expect(await sessions.read('acme', url, browser.cookies)).not.toBeNull();
  expect(vi.mocked(client.refreshTokenGrant).mock.calls[0][1]).toBe(opaque);

  // A terminal refusal clears only this tenant's session.
  const other = await signIn(new BrowserSessions(), 'other-login', 60);
  vi.mocked(client.refreshTokenGrant).mockRejectedValueOnce(invalidGrant());
  const expired = await signIn(sessions, opaque, 1);
  expect(await sessions.read('acme', url, expired.cookies)).toBeNull();
  expect(expired.values.has('wyrd_session_acme')).toBe(false);
  expect(other.values.has('wyrd_session_acme')).toBe(true);
});

test('failed refresh-token revocation still signs out', async () => {
  const sessions = new BrowserSessions();
  const browser = await signIn(sessions, 'refresh-to-revoke');
  const cookie = browser.values.get('wyrd_session_acme')!;
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
  vi.mocked(client.tokenRevocation).mockRejectedValueOnce(new TypeError('fetch failed'));

  await expect(sessions.logout('acme', url, browser.cookies)).resolves.toBeUndefined();
  expect(vi.mocked(client.tokenRevocation).mock.calls[0][1]).toBe('refresh-to-revoke');
  expect(browser.values.has('wyrd_session_acme')).toBe(false);
  expect(warn).toHaveBeenCalledOnce();
  expect(JSON.stringify(warn.mock.calls)).not.toContain('refresh-to-revoke');
  warn.mockRestore();

  // The cached access token is gone too: a replayed cookie must renew through the server.
  browser.values.set('wyrd_session_acme', cookie);
  vi.mocked(client.refreshTokenGrant).mockRejectedValueOnce(invalidGrant());
  expect(await sessions.read('acme', url, browser.cookies)).toBeNull();
  expect(vi.mocked(client.refreshTokenGrant).mock.calls[0][1]).toBe('refresh-to-revoke');
});
