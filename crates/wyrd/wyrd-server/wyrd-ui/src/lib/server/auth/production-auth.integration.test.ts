// @vitest-environment node
/**
 * Production BFF journeys over real HTTP. `identity_ui_e2e.rs` starts the Wyrd
 * server, two `node build` BFF replicas sharing one public origin, Postgres,
 * and Keycloak, then runs this file with the `WYRD_UI_JOURNEY` fixture. Every
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
  users: Record<'admin' | 'reader', { username: string; password: string }>;
};

const journey = JSON.parse(process.env.WYRD_UI_JOURNEY ?? 'null') as Journey;
const keycloakIssuer =
  process.env.WYRD_KEYCLOAK_ISSUER ?? 'http://localhost:18080/realms/wyrd-test';
const jwt = /eyJ[\w-]+\.[\w-]+\./;
const sessionCookie = (tenant: string) => `wyrd_session_${tenant}`;

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

/** Sign in at Keycloak's HTML form; returns the provider's callback location. */
async function keycloakLogin(authorization: string, user: Journey['users']['admin']) {
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
  expect(action, 'Keycloak renders its login form').toBeDefined();
  const posted = await fetch(action!.replaceAll('&amp;', '&'), {
    method: 'POST',
    redirect: 'manual',
    headers: { cookie: cookie(), 'content-type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams(user)
  });
  expect(posted.status).toBe(302);
  return posted.headers.get('location')!;
}

/** Begin SSO at one replica: sets the flow cookie and returns the provider URL. */
async function beginSso(browser: Browser, replica: 0 | 1): Promise<string> {
  const page = await browser.go(replica, `/t/${journey.ssoTenant}/login`);
  expect(await page.text()).toContain('Sign in with SSO');
  const begun = await browser.go(replica, `/t/${journey.ssoTenant}/login?/sso`, { form: {} });
  expect(begun.status).toBe(303);
  expectSafeCookie(browser.setCookie('wyrd_flow'));
  const authorization = begun.headers.get('location')!;
  expect(authorization.startsWith(keycloakIssuer)).toBe(true);
  return authorization;
}

/**
 * Finish the provider leg the way the gateway routes it: Keycloak's callback
 * location is on the public origin, and `/auth/callback` belongs to the server.
 */
async function providerCallback(authorization: string, user: Journey['users']['admin']) {
  const callback = new URL(await keycloakLogin(authorization, user));
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
  user: Journey['users']['admin'],
  start: 0 | 1,
  finish: 0 | 1
): Promise<string> {
  await providerCallback(await beginSso(browser, start), user);
  const done = await browser.go(finish, '/login/complete');
  expect(done.status).toBe(303);
  expect(done.headers.get('location')).toBe(`/t/${journey.ssoTenant}`);
  expect(browser.cookies.has('wyrd_flow')).toBe(false);
  return browser.cookies.get(sessionCookie(journey.ssoTenant))!;
}

/** The page and its data channel carry neither the session id nor any token. */
async function expectNoSecrets(browser: Browser, tenant: string, sessionId: string) {
  for (const path of [`/t/${tenant}`, `/t/${tenant}/__data.json`, `/t/${tenant}/settings`]) {
    for (const replica of [0, 1] as const) {
      const response = await browser.go(replica, path);
      expect(response.status, path).toBe(200);
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
  expect((await crossed.go(0, `/t/${journey.apiKeyTenant}/settings`)).status).toBe(403);
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
});

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
});
