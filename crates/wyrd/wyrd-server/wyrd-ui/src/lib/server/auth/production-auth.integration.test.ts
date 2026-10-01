// @vitest-environment node
/**
 * Production BFF journeys over real HTTP. `identity_ui_e2e.rs` starts the Wyrd
 * server, two `node build` BFF replicas sharing one public origin, Postgres,
 * and Keycloak (two realms), then runs this file with the `WYRD_UI_JOURNEY` fixture. Every
 * request goes over the wire; nothing here mocks the BFF or the server.
 */
import { expect, test } from 'vitest';

type Journey = {
  origin: string;
  server: string;
  bffs: [string, string];
  serviceKey: string;
  ssoTenant: string;
  apiKeyTenant: string;
  ssoAdminKey: string;
  offAdminKey: string;
  offReaderKey: string;
  switchTenants: { keycloak: string; second: string; peer: string };
  replacementTenant: string;
  replacementOwnerKey: string;
  /** Each provider's login form fields. */
  users: Record<'admin' | 'reader', Record<string, string>>;
};

const journey = JSON.parse(process.env.WYRD_UI_JOURNEY ?? 'null') as Journey;
const keycloakIssuer =
  process.env.WYRD_KEYCLOAK_ISSUER ?? 'http://localhost:18080/realms/wyrd-test';
/** The second realm: a distinct issuer whose `alice` shares the first realm's email. */
const secondIssuer = `${keycloakIssuer.replace(/\/$/, '')}-2`;
const jwt = /eyJ[\w-]+\.[\w-]+\./;
const sessionCookie = (tenant: string) => `wyrd_session_${tenant}`;
/** Each journey drives many real logins, key verifications, and renewals. */
const journeyTimeout = 60_000;

/**
 * One browser behind a load balancer: a host-only cookie jar for the public
 * origin, sent to whichever BFF replica a request is routed to.
 */
class Browser {
  readonly cookies = new Map<string, string>();
  /** Every `Set-Cookie` and `Location` the BFFs returned, for leak checks. */
  readonly seen: string[] = [];

  async go(
    replica: 0 | 1,
    path: string,
    options: { form?: Record<string, string>; origin?: string } = {}
  ): Promise<Response> {
    const headers = new Headers({ accept: 'text/html' });
    if (this.cookies.size)
      headers.set('cookie', [...this.cookies].map(([k, v]) => `${k}=${v}`).join('; '));
    if (options.form) {
      headers.set('content-type', 'application/x-www-form-urlencoded');
      headers.set('origin', options.origin ?? journey.origin);
    }
    const response = await fetch(journey.bffs[replica] + path, {
      method: options.form ? 'POST' : 'GET',
      headers,
      body: options.form ? new URLSearchParams(options.form) : undefined,
      redirect: 'manual'
    });
    for (const raw of response.headers.getSetCookie()) {
      this.seen.push(raw);
      const [pair] = raw.split(';');
      const name = pair.slice(0, pair.indexOf('='));
      const value = pair.slice(pair.indexOf('=') + 1);
      if (!value || /max-age=0/i.test(raw)) this.cookies.delete(name);
      else this.cookies.set(name, value);
    }
    this.seen.push(response.headers.get('location') ?? '');
    return response;
  }

  /** The raw `Set-Cookie` the last responses issued for `name`. */
  setCookie(name: string): string {
    const raw = this.seen.findLast((value) => value.startsWith(`${name}=`));
    expect(raw, `${name} was set`).toBeDefined();
    return raw!;
  }
}

/** Host-only, HttpOnly, Secure, SameSite=Lax, path `/`. */
function expectSafeCookie(raw: string): void {
  expect(raw).toMatch(/;\s*HttpOnly/i);
  expect(raw).toMatch(/;\s*Secure/i);
  expect(raw).toMatch(/;\s*SameSite=Lax/i);
  expect(raw).toMatch(/;\s*Path=\/(;|$)/i);
  expect(raw).not.toMatch(/;\s*Domain=/i);
}

/** The session CSRF token rendered into the page's forms. */
async function csrfOf(response: Response): Promise<string> {
  expect(response.status).toBe(200);
  const csrf = /name="csrf" value="([a-f0-9]{64})"/.exec(await response.text())?.[1];
  expect(csrf, 'page renders the session CSRF token').toBeDefined();
  return csrf!;
}

/**
 * Sign in at the provider's HTML login form and follow its redirects until
 * it returns to the public origin; returns that callback location.
 */
async function providerLogin(authorization: string, user: Record<string, string>) {
  const jar = new Map<string, string>();
  const keep = (response: Response) => {
    for (const raw of response.headers.getSetCookie()) {
      const [pair] = raw.split(';');
      jar.set(pair.slice(0, pair.indexOf('=')), pair.slice(pair.indexOf('=') + 1));
    }
  };
  const cookie = () => [...jar].map(([k, v]) => `${k}=${v}`).join('; ');
  let url = authorization;
  let response = await fetch(url, { redirect: 'manual' });
  keep(response);
  while (response.status >= 300 && response.status < 400) {
    url = new URL(response.headers.get('location')!, url).toString();
    response = await fetch(url, { redirect: 'manual', headers: { cookie: cookie() } });
    keep(response);
  }
  const action = /action="([^"]+)"/.exec(await response.text())?.[1];
  expect(action, 'the provider renders its login form').toBeDefined();
  url = new URL(action!.replaceAll('&amp;', '&'), url).toString();
  response = await fetch(url, {
    method: 'POST',
    redirect: 'manual',
    headers: { cookie: cookie(), 'content-type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams(user)
  });
  for (;;) {
    keep(response);
    expect([302, 303], 'the provider accepts the credentials').toContain(response.status);
    url = new URL(response.headers.get('location')!, url).toString();
    if (url.startsWith(`${journey.origin}/`)) return url;
    response = await fetch(url, { redirect: 'manual', headers: { cookie: cookie() } });
  }
}

/** Begin SSO at one replica: sets the flow cookie and returns the provider URL. */
async function beginSso(
  browser: Browser,
  replica: 0 | 1,
  tenant = journey.ssoTenant,
  issuer = keycloakIssuer
): Promise<string> {
  const page = await browser.go(replica, `/t/${tenant}/login`);
  expect(await page.text()).toContain('Sign in with SSO');
  const begun = await browser.go(replica, `/t/${tenant}/login?/sso`, { form: {} });
  expect(begun.status).toBe(303);
  expectSafeCookie(browser.setCookie('wyrd_flow'));
  const authorization = begun.headers.get('location')!;
  expect(authorization.startsWith(issuer)).toBe(true);
  return authorization;
}

/**
 * Finish the provider leg the way the gateway routes it: Keycloak's callback
 * location is on the public origin, and `/auth/callback` belongs to the server.
 */
async function providerCallback(authorization: string, user: Record<string, string>) {
  const callback = new URL(await providerLogin(authorization, user));
  expect(callback.origin + callback.pathname).toBe(`${journey.origin}/auth/callback`);
  const response = await fetch(`${journey.server}/auth/callback${callback.search}`, {
    redirect: 'manual'
  });
  expect(response.status, await response.clone().text()).toBe(303);
  expect(response.headers.get('location')).toBe(`${journey.origin}/login/complete`);
}

/** Full SSO sign-in: begin at `start`, complete at `finish`. */
async function ssoLogin(
  browser: Browser,
  user: Record<string, string>,
  start: 0 | 1,
  finish: 0 | 1,
  tenant = journey.ssoTenant,
  issuer = keycloakIssuer
): Promise<string> {
  await providerCallback(await beginSso(browser, start, tenant, issuer), user);
  const done = await browser.go(finish, '/login/complete');
  expect(done.status).toBe(303);
  expect(done.headers.get('location')).toBe(`/t/${tenant}`);
  expect(browser.cookies.has('wyrd_flow')).toBe(false);
  return browser.cookies.get(sessionCookie(tenant))!;
}

/**
 * Send one provider's authorization code to the common callback under another
 * login's state, as a mix-up or injection attacker would; the server must
 * refuse it, and the browser holding that other login's flow gets no session.
 */
async function expectMixedCallbackRefused(
  browser: Browser,
  code: string,
  state: string,
  tenant: string
) {
  const query = new URLSearchParams({ code, state });
  const mixed = await fetch(`${journey.server}/auth/callback?${query}`, { redirect: 'manual' });
  expect(mixed.status, await mixed.clone().text()).toBeGreaterThanOrEqual(400);
  expect(mixed.status).toBeLessThan(500);
  const done = await browser.go(1, '/login/complete');
  expect(done.headers.get('location')).toBe('/?login=failed');
  expect(browser.cookies.has(sessionCookie(tenant))).toBe(false);
}

/** The server's view of one session over the private channel, as a BFF reads it. */
async function serverRead(sessionId: string): Promise<Response> {
  return fetch(`${journey.server}/internal/bff/v1/sessions/read`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'x-wyrd-bff-key': journey.serviceKey },
    body: JSON.stringify({ session_id: sessionId })
  });
}

/** The fields of a private-channel session read these journeys compare. */
type SessionRead = { tenant_key: string; principal_id: string; roles: string[] };

/** A session the server still honours, read over the private channel. */
async function liveSession(sessionId: string): Promise<SessionRead> {
  const response = await serverRead(sessionId);
  expect(response.status).toBe(200);
  return (await response.json()) as SessionRead;
}

/** Call a tenant `/v1` API on the server as the holder of `apiKey`. */
async function asKey(apiKey: string, method: string, path: string): Promise<Response> {
  const token = await fetch(`${journey.server}/auth/token`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ grant_type: 'wyrd_api_key', api_key: apiKey })
  });
  expect(token.status).toBe(200);
  const { access_token } = (await token.json()) as { access_token: string };
  return fetch(`${journey.server}/v1${path}`, {
    method,
    headers: { 'x-wyrd-access-token': `Bearer ${access_token}` }
  });
}

/** The tenant keys the page's chooser offers to switch to. */
function chooserTenants(html: string): string[] {
  return [...html.matchAll(/<button[^>]*name="tenantKey" value="([^"]+)"/g)]
    .map(([, key]) => key)
    .sort();
}

/** The page and its data channel carry neither the session id nor any token. */
async function expectNoSecrets(browser: Browser, tenant: string, sessionId: string) {
  for (const path of [`/t/${tenant}`, `/t/${tenant}/__data.json`, `/t/${tenant}/settings`]) {
    for (const replica of [0, 1] as const) {
      const response = await browser.go(replica, path);
      // Served to the session (a reader may be denied a page), never sent to sign-in.
      expect([200, 403], path).toContain(response.status);
      const body = await response.text();
      expect(body).not.toContain(sessionId);
      expect(body).not.toMatch(jwt);
    }
  }
  for (const value of browser.seen.filter((seen) => !seen.startsWith('wyrd_'))) {
    expect(value).not.toContain(sessionId);
    expect(value).not.toMatch(jwt);
  }
}

test('production SSO crosses replicas', async () => {
  const sso = journey.ssoTenant;
  const settings = `/t/${sso}/settings`;

  // Only the deployment BFF key opens the private channel.
  for (const key of [undefined, 'not-the-key']) {
    const refused = await fetch(`${journey.server}/internal/bff/v1/sessions/read`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', ...(key ? { 'x-wyrd-bff-key': key } : {}) },
      body: JSON.stringify({ session_id: 'a'.repeat(64) })
    });
    expect(refused.status).toBe(401);
  }

  // Missing, forged, mismatched, and replayed flows set no session.
  const alice = new Browser();
  expect((await alice.go(0, `/t/${sso}`)).headers.get('location')).toBe(`/t/${sso}/login`);
  for (const flow of [undefined, 'f'.repeat(64)]) {
    if (flow) alice.cookies.set('wyrd_flow', flow);
    const failed = await alice.go(1, '/login/complete');
    expect(failed.headers.get('location')).toBe('/?login=failed');
    expect(alice.cookies.has(sessionCookie(sso))).toBe(false);
  }
  await beginSso(alice, 0);
  const firstFlow = alice.cookies.get('wyrd_flow')!;
  await providerCallback(await beginSso(alice, 1), journey.users.admin);
  // The provider completed the second flow; the first flow's cookie cannot redeem it.
  const secondFlow = alice.cookies.get('wyrd_flow')!;
  alice.cookies.set('wyrd_flow', firstFlow);
  expect((await alice.go(0, '/login/complete')).headers.get('location')).toBe('/?login=failed');
  expect(alice.cookies.has(sessionCookie(sso))).toBe(false);

  // Begun at replica 0 (above), completed at replica 1.
  alice.cookies.set('wyrd_flow', secondFlow);
  const done = await alice.go(1, '/login/complete');
  expect(done.headers.get('location')).toBe(`/t/${sso}`);
  const sessionId = alice.cookies.get(sessionCookie(sso))!;
  expect(sessionId).toMatch(/^[a-f0-9]{64}$/);
  expectSafeCookie(alice.setCookie(sessionCookie(sso)));
  // The completion is one-use.
  const replay = new Browser();
  replay.cookies.set('wyrd_flow', secondFlow);
  expect((await replay.go(0, '/login/complete')).headers.get('location')).toBe('/?login=failed');
  expect(replay.cookies.size).toBe(0);

  // Both replicas serve the session, concurrently renewing the same row.
  const both = await Promise.all([alice.go(0, `/t/${sso}`), alice.go(1, `/t/${sso}`)]);
  expect(both.map((response) => response.status)).toEqual([200, 200]);
  await expectNoSecrets(alice, sso, sessionId);

  // Forged and cross-tenant cookies are refused; the cookie name is only a hint.
  const forged = new Browser();
  forged.cookies.set(sessionCookie(sso), 'b'.repeat(64));
  expect((await forged.go(1, `/t/${sso}`)).headers.get('location')).toBe(`/t/${sso}/login`);
  expect(forged.cookies.has(sessionCookie(sso))).toBe(false);
  const crossed = new Browser();
  crossed.cookies.set(sessionCookie(journey.apiKeyTenant), sessionId);
  expect((await crossed.go(0, `/t/${journey.apiKeyTenant}/settings`)).headers.get('location')).toBe(
    `/t/${journey.apiKeyTenant}/login`
  );
  expect(crossed.cookies.has(sessionCookie(journey.apiKeyTenant))).toBe(false);
  expect(
    (await alice.go(0, `/t/${journey.apiKeyTenant}/settings?/deactivate`, { form: {} })).status
  ).toBe(401);

  // Actions need the session CSRF token and the public origin.
  const csrf = await csrfOf(await alice.go(0, settings));
  expect((await alice.go(1, `${settings}?/deactivate`, { form: {} })).status).toBe(403);
  expect(
    (await alice.go(1, `${settings}?/deactivate`, { form: { csrf: 'c'.repeat(64) } })).status
  ).toBe(403);
  expect(
    (
      await alice.go(1, `${settings}?/deactivate`, {
        form: { csrf },
        origin: 'http://evil.test'
      })
    ).status
  ).toBe(403);

  // A reader signs in on the same connection and is denied the admin action.
  const bob = new Browser();
  await ssoLogin(bob, journey.users.reader, 1, 0);
  const bobCsrf = await csrfOf(await bob.go(1, settings));
  expect((await bob.go(0, `${settings}?/deactivate`, { form: { csrf: bobCsrf } })).status).toBe(
    403
  );

  // Switch: a tenant without a session in this browser goes to its login; one with a session opens.
  const toLogin = await alice.go(0, '/?/switch', {
    form: { csrf, from: sso, tenantKey: journey.apiKeyTenant }
  });
  expect(toLogin.headers.get('location')).toBe(`/t/${journey.apiKeyTenant}/login`);
  expect((await alice.go(0, '/?/switch', { form: { from: sso, tenantKey: sso } })).status).toBe(
    403
  );

  // Logout at one replica ends the session at both; the stale cookie is cleared.
  const second = new Browser();
  const secondId = await ssoLogin(second, journey.users.admin, 0, 1);
  const secondCsrf = await csrfOf(await second.go(0, settings));
  const loggedOut = await second.go(1, '/?/logout', {
    form: { csrf: secondCsrf, tenantKey: sso }
  });
  expect(loggedOut.headers.get('location')).toBe(`/t/${sso}/login`);
  expect(second.cookies.has(sessionCookie(sso))).toBe(false);
  second.cookies.set(sessionCookie(sso), secondId);
  expect((await second.go(0, `/t/${sso}`)).headers.get('location')).toBe(`/t/${sso}/login`);
  expect(second.cookies.has(sessionCookie(sso))).toBe(false);

  // A tenant with SSO never accepts an API key at its sign-in page.
  const apiKeyAtSso = new Browser();
  const refusedKey = await apiKeyAtSso.go(0, `/t/${sso}/login?/apiKey`, {
    form: { apiKey: journey.ssoAdminKey }
  });
  expect(refusedKey.status).toBe(401);
  expect(apiKeyAtSso.cookies.has(sessionCookie(sso))).toBe(false);

  // The admin may deactivate; every session on that connection then stops renewing.
  const deactivated = await alice.go(1, `${settings}?/deactivate`, { form: { csrf } });
  expect(deactivated.status).not.toBe(403);
  expect(deactivated.status).toBeLessThan(500);
  for (const browser of [alice, bob]) {
    expect((await browser.go(0, `/t/${sso}`)).headers.get('location')).toBe(`/t/${sso}/login`);
    expect(browser.cookies.has(sessionCookie(sso))).toBe(false);
  }
}, journeyTimeout);

test('OIDC-off credential UI', async () => {
  const tenant = journey.apiKeyTenant;
  const login = `/t/${tenant}/login`;
  const settings = `/t/${tenant}/settings`;
  const browser = new Browser();

  const page = await (await browser.go(0, login)).text();
  expect(page).toContain('name="apiKey"');
  expect(page).not.toContain('Sign in with SSO');

  // Unusable keys and another tenant's key get one indistinguishable refusal.
  for (const apiKey of ['wyrd_not_a_key', journey.ssoAdminKey]) {
    const refused = await browser.go(1, `${login}?/apiKey`, { form: { apiKey } });
    expect(refused.status).toBe(401);
    expect(browser.cookies.has(sessionCookie(tenant))).toBe(false);
  }
  // A cross-site sign-in post is refused.
  expect(
    (
      await browser.go(0, `${login}?/apiKey`, {
        form: { apiKey: journey.offReaderKey },
        origin: 'http://evil.test'
      })
    ).status
  ).toBe(403);

  // A reader signs in with its key and is denied connection administration.
  const reader = await browser.go(1, `${login}?/apiKey`, { form: { apiKey: journey.offReaderKey } });
  expect(reader.headers.get('location')).toBe(`/t/${tenant}`);
  expectSafeCookie(browser.setCookie(sessionCookie(tenant)));
  const readerId = browser.cookies.get(sessionCookie(tenant))!;
  await expectNoSecrets(browser, tenant, readerId);
  for (const seen of browser.seen) expect(seen).not.toContain(journey.offReaderKey);
  const readerCsrf = await csrfOf(await browser.go(0, settings));
  expect((await browser.go(1, `${settings}?/deactivate`, { form: { csrf: readerCsrf } })).status).toBe(
    403
  );
  expect((await browser.go(1, `${settings}?/deactivate`, { form: {} })).status).toBe(403);
  const out = await browser.go(0, '/?/logout', { form: { csrf: readerCsrf, tenantKey: tenant } });
  expect(out.headers.get('location')).toBe(login);
  expect(browser.cookies.has(sessionCookie(tenant))).toBe(false);

  // An admin key opens the session; staging an SSO candidate is allowed.
  const admin = await browser.go(0, `${login}?/apiKey`, { form: { apiKey: journey.offAdminKey } });
  expect(admin.headers.get('location')).toBe(`/t/${tenant}`);
  const adminCsrf = await csrfOf(await browser.go(1, settings));
  const staged = await browser.go(0, `${settings}?/stage`, {
    form: {
      csrf: adminCsrf,
      revision: '',
      issuer: keycloakIssuer,
      clientId: 'wyrd-human',
      clientAuth: 'Public',
      clientSecret: '',
      emailClaim: 'email',
      groupsClaim: 'groups',
      groupRoles: 'wyrd-admins = admin'
    }
  });
  expect(staged.status).toBe(200);
  const stagedPage = await (await browser.go(1, settings)).text();
  expect(stagedPage).toContain('wyrd-human');
  expect(stagedPage).not.toContain(journey.offAdminKey);
}, journeyTimeout);

test('production multi-provider tenant switch', async () => {
  const { keycloak, second, peer } = journey.switchTenants;
  const browser = new Browser();

  // Independent sessions for two tenants on two real providers, in one browser.
  const keycloakId = await ssoLogin(browser, journey.users.admin, 0, 1, keycloak);
  const secondId = await ssoLogin(browser, journey.users.admin, 1, 0, second, secondIssuer);
  expect(secondId).not.toBe(keycloakId);
  expect(browser.cookies.get(sessionCookie(keycloak))).toBe(keycloakId);
  const [keycloakSession, secondSession] = await Promise.all([
    liveSession(keycloakId),
    liveSession(secondId)
  ]);
  expect([keycloakSession.tenant_key, secondSession.tenant_key]).toEqual([keycloak, second]);
  expect(secondSession.principal_id).not.toBe(keycloakSession.principal_id);

  // Both replicas render a chooser of exactly the server-verified tenants; a
  // forged session-cookie hint never renders and is cleared. (Settings render
  // for every member; the second tenant's reader is denied the home page.)
  browser.cookies.set(sessionCookie('ui-ghost'), 'd'.repeat(64));
  for (const [replica, tenant] of [
    [0, keycloak],
    [1, second]
  ] as const) {
    const page = await browser.go(replica, `/t/${tenant}/settings`);
    expect(page.status).toBe(200);
    expect(chooserTenants(await page.text())).toEqual([second, keycloak].sort());
  }
  expect(browser.cookies.has(sessionCookie('ui-ghost'))).toBe(false);

  // Switching takes the target's own session on either replica.
  const keycloakCsrf = await csrfOf(await browser.go(0, `/t/${keycloak}/settings`));
  const secondCsrf = await csrfOf(await browser.go(1, `/t/${second}/settings`));
  expect(secondCsrf).not.toBe(keycloakCsrf);
  const toSecond = await browser.go(1, '/?/switch', {
    form: { csrf: keycloakCsrf, from: keycloak, tenantKey: second }
  });
  expect(toSecond.status).toBe(303);
  expect(toSecond.headers.get('location')).toBe(`/t/${second}`);
  expect((await browser.go(1, `/t/${second}/settings`)).status).toBe(200);
  const toKeycloak = await browser.go(0, '/?/switch', {
    form: { csrf: secondCsrf, from: second, tenantKey: keycloak }
  });
  expect(toKeycloak.headers.get('location')).toBe(`/t/${keycloak}`);
  // One tenant's CSRF token does not authorize leaving another tenant.
  const crossCsrf = await browser.go(0, '/?/switch', {
    form: { csrf: keycloakCsrf, from: second, tenantKey: keycloak }
  });
  expect(crossCsrf.status).toBe(403);

  // Wrong tenant: a second-realm code for the second tenant's login cannot
  // complete the first tenant's login, though the state is genuine.
  const attacker = new Browser();
  const secondCode = new URL(
    await providerLogin(await beginSso(attacker, 0, second, secondIssuer), journey.users.admin)
  ).searchParams.get('code')!;
  const keycloakState = new URL(await beginSso(attacker, 1, keycloak)).searchParams.get('state')!;
  await expectMixedCallbackRefused(attacker, secondCode, keycloakState, keycloak);

  // Same issuer and client, other tenant: a Keycloak code for the peer tenant's
  // login cannot complete the Keycloak tenant's login either.
  const peerCode = new URL(
    await providerLogin(await beginSso(attacker, 0, peer), journey.users.admin)
  ).searchParams.get('code')!;
  const secondState = new URL(await beginSso(attacker, 1, keycloak)).searchParams.get('state')!;
  await expectMixedCallbackRefused(attacker, peerCode, secondState, keycloak);
  expect(attacker.cookies.has(sessionCookie(peer))).toBe(false);

  // The same provider user signed in to the peer is a different tenant's
  // session: it joins this browser's chooser, but never opens the other tenant.
  const peerId = await ssoLogin(browser, journey.users.admin, 1, 0, peer);
  const peerSession = await liveSession(peerId);
  expect(peerSession.tenant_key).toBe(peer);
  expect(peerSession.principal_id).not.toBe(keycloakSession.principal_id);
  expect(chooserTenants(await (await browser.go(1, `/t/${keycloak}/settings`)).text())).toEqual(
    [second, keycloak, peer].sort()
  );
  const crossed = new Browser();
  crossed.cookies.set(sessionCookie(keycloak), peerId);
  expect((await crossed.go(0, `/t/${keycloak}`)).headers.get('location')).toBe(
    `/t/${keycloak}/login`
  );
  expect(crossed.cookies.has(sessionCookie(keycloak))).toBe(false);
  await liveSession(peerId);
}, journeyTimeout);

test('production provider replacement settings', async () => {
  const tenant = journey.replacementTenant;
  const settings = `/t/${tenant}/settings`;
  const alice = new Browser();
  const stage = (csrf: string) =>
    alice.go(0, `${settings}?/stage`, {
      form: {
        csrf,
        revision: '',
        issuer: secondIssuer,
        clientId: 'wyrd-human',
        clientAuth: 'Public',
        clientSecret: '',
        emailClaim: 'email',
        groupsClaim: '',
        groupRoles: ''
      }
    });
  /** The rendered connections: ids in page order (Active first) and the candidate revision. */
  const connections = async () => {
    const html = await (await alice.go(1, settings)).text();
    return {
      ids: [...html.matchAll(/name="id" value="([0-9a-f-]{36})"/g)].map(([, id]) => id),
      revision: /action="\?\/test">[\s\S]*?name="revision" value="(\d+)"/.exec(html)?.[1]
    };
  };

  // Keycloak alice administers the tenant through its Active connection.
  const oldId = await ssoLogin(alice, journey.users.admin, 0, 1, tenant);
  const old = await liveSession(oldId);
  expect(old.roles).toContain('admin');
  const csrf = await csrfOf(await alice.go(0, settings));
  const [oldConnection] = (await connections()).ids;
  expect(oldConnection).toBeDefined();

  // Stage a second-realm replacement, then remove that candidate through settings.
  expect((await stage(csrf)).status).toBe(200);
  const discarded = await connections();
  expect(discarded.ids).toHaveLength(2);
  const removed = await alice.go(1, `${settings}?/remove`, {
    form: { csrf, id: discarded.ids[1] }
  });
  expect(removed.status).toBe(200);
  expect((await connections()).ids).toEqual([oldConnection]);

  // Stage again, test, and activate. Activation demands a live recovery key of
  // this tenant: an unusable one is refused and the old connection stays Active.
  expect((await stage(csrf)).status).toBe(200);
  const { ids, revision } = await connections();
  expect(ids).toHaveLength(2);
  expect(revision).toBeDefined();
  const tested = await alice.go(0, `${settings}?/test`, { form: { csrf, revision: revision! } });
  expect(tested.status).toBe(200);
  for (const recoveryApiKey of ['wyrd_not_a_key', journey.ssoAdminKey]) {
    const refused = await alice.go(1, `${settings}?/activate`, {
      form: { csrf, revision: revision!, recoveryApiKey }
    });
    expect(refused.status).toBeGreaterThanOrEqual(400);
    expect(refused.status).toBeLessThan(500);
    expect((await connections()).ids).toEqual(ids);
  }
  const activated = await alice.go(0, `${settings}?/activate`, {
    form: { csrf, revision: revision!, recoveryApiKey: journey.replacementOwnerKey }
  });
  expect(activated.status).toBe(200);
  for (const seen of alice.seen) expect(seen).not.toContain(journey.replacementOwnerKey);

  // The retired connection's session cannot renew on either replica.
  for (const replica of [1, 0] as const) {
    if (replica === 0) alice.cookies.set(sessionCookie(tenant), oldId);
    expect((await alice.go(replica, `/t/${tenant}`)).headers.get('location')).toBe(
      `/t/${tenant}/login`
    );
    expect(alice.cookies.has(sessionCookie(tenant))).toBe(false);
  }
  expect((await serverRead(oldId)).status).toBe(401);

  // The second realm's alice — same email — is a new User with none of the old authority.
  const replaced = new Browser();
  const newId = await ssoLogin(replaced, journey.users.admin, 1, 0, tenant, secondIssuer);
  const fresh = await liveSession(newId);
  expect(fresh.principal_id).not.toBe(old.principal_id);
  expect(fresh.roles).toEqual([]);
  const freshCsrf = await csrfOf(await replaced.go(0, settings));
  for (const action of ['deactivate', 'remove']) {
    const denied = await replaced.go(1, `${settings}?/${action}`, {
      form: { csrf: freshCsrf, id: oldConnection }
    });
    expect(denied.status, action).toBe(403);
  }

  // The owner's recovery key still administers: it sees the second realm Active and removes
  // the retired first-realm connection, which leaves nothing to renew.
  const listed = await asKey(journey.replacementOwnerKey, 'GET', '/identity/oidc/connections');
  expect(listed.status).toBe(200);
  const view = (await listed.json()) as { active: { id: string; issuer: string } | null };
  expect(view.active?.issuer.replace(/\/$/, '')).toBe(secondIssuer);
  expect(view.active?.id).toBe(ids[1]);
  const retired = `/identity/oidc/connections/${oldConnection}`;
  expect((await asKey(journey.replacementOwnerKey, 'DELETE', retired)).status).toBeLessThan(300);
  expect((await asKey(journey.replacementOwnerKey, 'DELETE', retired)).status).toBe(404);
  alice.cookies.set(sessionCookie(tenant), oldId);
  expect((await alice.go(1, `/t/${tenant}`)).headers.get('location')).toBe(`/t/${tenant}/login`);
  // Removing the retired connection leaves the replacement's session live.
  expect((await replaced.go(0, settings)).status).toBe(200);
  await liveSession(newId);
}, journeyTimeout);
