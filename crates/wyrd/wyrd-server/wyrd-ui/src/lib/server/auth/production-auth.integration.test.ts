// @vitest-environment node
/**
 * Production BFF journeys over real HTTP. `identity_ui_e2e.rs` starts two Wyrd
 * server replicas over one Postgres, two `node build` BFF replicas sharing one
 * public origin (one per server replica), Keycloak (two realms), and Dex, then
 * runs this file with the `WYRD_UI_JOURNEY` fixture. Every request goes over
 * the wire; nothing here mocks the BFF or the server.
 *
 * The BFF is the confidential OAuth client `wyrd-ui`: sign-in is the
 * authorization code grant with PKCE, a replica that has not cached a
 * session's access token renews it with the refresh token, and logout revokes
 * that refresh token (RFC 7009).
 */
import { expect, test } from 'vitest';

type Journey = {
  origin: string;
  /** The Wyrd replicas, in BFF order: the server routes the gateway sends `/auth/*` to. */
  servers: [string, string];
  bffs: [string, string];
  ssoTenant: string;
  apiKeyTenant: string;
  ssoAdminKey: string;
  offAdminKey: string;
  offReaderKey: string;
  switchTenants: { keycloak: string; second: string; peer: string };
  replacementTenant: string;
  replacementOwnerKey: string;
  /** Each provider's login form fields: Keycloak's admin and reader, and Dex's user. */
  users: Record<'admin' | 'reader' | 'dex', Record<string, string>>;
};

const journey = JSON.parse(process.env.WYRD_UI_JOURNEY ?? 'null') as Journey;
const keycloakIssuer =
  process.env.WYRD_KEYCLOAK_ISSUER ?? 'http://localhost:8180/realms/wyrd-test';
/** The second realm: a distinct issuer whose `alice` shares the first realm's email. */
const secondIssuer = `${keycloakIssuer.replace(/\/$/, '')}-2`;
/** Dex: the different provider the multi-provider switch tenant signs in through. */
const dexIssuer = process.env.WYRD_DEX_ISSUER ?? 'http://localhost:5556';
/** A clear-text JWT (access, refresh, or ID token). A `dir` JWE has an empty second part. */
const jwt = /eyJ[\w-]+\.[\w-]+\./;
const sessionCookie = (tenant: string) => `wyrd_session_${tenant}`;
/** Each journey drives many real logins, key verifications, and renewals. */
const journeyTimeout = 60_000;

/** The gateway's routing of a public-origin `/auth/*` URL to Wyrd replica `replica`. */
function gateway(url: string, replica: 0 | 1 = 0): string {
  const target = new URL(url);
  expect(target.origin).toBe(journey.origin);
  return journey.servers[replica] + target.pathname + target.search;
}

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

/** A session cookie value is a compact `dir`/`A256GCM` JWE: no clear-text token inside. */
function expectEncrypted(value: string): void {
  expect(value).toMatch(/^eyJ[\w-]+\.\.[\w-]+\.[\w-]+\.[\w-]+$/);
  const header = JSON.parse(Buffer.from(value.split('.')[0], 'base64url').toString());
  expect(header).toEqual({ alg: 'dir', enc: 'A256GCM' });
  expect(value).not.toMatch(jwt);
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

/**
 * Begin SSO at one replica: the BFF sets the login cookie and sends the
 * browser to Wyrd's authorization endpoint (code + S256 PKCE + state), which
 * sends it on to the tenant's provider. Returns the provider URL.
 */
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
  expectSafeCookie(browser.setCookie('wyrd_login'));
  expectEncrypted(browser.cookies.get('wyrd_login')!);
  const request = new URL(begun.headers.get('location')!);
  expect(request.origin + request.pathname).toBe(`${journey.origin}/auth/authorize`);
  const query = request.searchParams;
  expect(query.get('response_type')).toBe('code');
  expect(query.get('client_id')).toBe('wyrd-ui');
  expect(query.get('redirect_uri')).toBe(`${journey.origin}/login/callback`);
  expect(query.get('code_challenge_method')).toBe('S256');
  expect(query.get('code_challenge')).toMatch(/^[\w-]{43}$/);
  expect(query.get('state')).toBeTruthy();
  expect(query.get('tenant')).toBe(tenant);
  const authorized = await fetch(gateway(request.toString(), replica), { redirect: 'manual' });
  expect(authorized.status, await authorized.clone().text()).toBe(303);
  const authorization = authorized.headers.get('location')!;
  expect(authorization.startsWith(issuer)).toBe(true);
  return authorization;
}

/**
 * Finish the provider leg the way the gateway routes it: the provider's
 * callback location is on the public origin, and `/auth/callback` belongs to
 * the server, which answers with the BFF's authorization response. Returns
 * that response's path and query on the public origin.
 */
async function providerCallback(
  authorization: string,
  user: Record<string, string>,
  replica: 0 | 1 = 0
): Promise<string> {
  const callback = new URL(await providerLogin(authorization, user));
  expect(callback.origin + callback.pathname).toBe(`${journey.origin}/auth/callback`);
  const response = await fetch(gateway(callback.toString(), replica), { redirect: 'manual' });
  expect(response.status, await response.clone().text()).toBe(303);
  const returned = new URL(response.headers.get('location')!);
  expect(returned.origin + returned.pathname).toBe(`${journey.origin}/login/callback`);
  expect(returned.searchParams.get('code')).toBeTruthy();
  return returned.pathname + returned.search;
}

/** Full SSO sign-in: begin at `start`, redeem the code at `finish`; returns the session cookie. */
async function ssoLogin(
  browser: Browser,
  user: Record<string, string>,
  start: 0 | 1,
  finish: 0 | 1,
  tenant = journey.ssoTenant,
  issuer = keycloakIssuer
): Promise<string> {
  const returned = await providerCallback(await beginSso(browser, start, tenant, issuer), user, finish);
  const done = await browser.go(finish, returned);
  expect(done.status).toBe(303);
  expect(done.headers.get('location')).toBe(`/t/${tenant}`);
  expect(browser.cookies.has('wyrd_login')).toBe(false);
  const session = browser.cookies.get(sessionCookie(tenant))!;
  expectSafeCookie(browser.setCookie(sessionCookie(tenant)));
  expectEncrypted(session);
  return session;
}

/**
 * Replay one provider's genuine return to the common callback under another
 * login's state, as a mix-up or injection attacker would: every parameter the
 * provider sent (`code`, any RFC 9207 `iss`, and the rest) is kept and only
 * `state` is replaced. The server issues no code: it refuses that other
 * login back to the BFF with an OAuth `error`, and the browser holding that
 * login gets no session.
 */
async function expectMixedCallbackRefused(
  browser: Browser,
  providerReturn: string,
  state: string,
  tenant: string
) {
  const query = new URL(providerReturn).searchParams;
  query.set('state', state);
  const mixed = await fetch(`${journey.servers[0]}/auth/callback?${query}`, {
    redirect: 'manual'
  });
  expect(mixed.status, await mixed.clone().text()).toBe(303);
  const returned = new URL(mixed.headers.get('location')!);
  expect(returned.origin + returned.pathname).toBe(`${journey.origin}/login/callback`);
  expect(returned.searchParams.get('error')).toBe('access_denied');
  expect(returned.searchParams.has('code')).toBe(false);
  const done = await browser.go(1, returned.pathname + returned.search);
  expect(done.headers.get('location')).toBe(`/t/${tenant}/login?login=failed`);
  expect(browser.cookies.has(sessionCookie(tenant))).toBe(false);
}

/**
 * Begin SSO for a tenant with no Active connection: Wyrd's authorization
 * endpoint answers the BFF's callback with `access_denied`, and the BFF
 * renders the sign-in problem page. Returns that page.
 */
async function providerAuthorizationDenied(browser: Browser, tenant: string): Promise<string> {
  const begun = await browser.go(0, `/t/${tenant}/login?/sso`, { form: {} });
  expect(begun.status).toBe(303);
  const denied = await fetch(gateway(begun.headers.get('location')!), { redirect: 'manual' });
  expect(denied.status).toBe(303);
  const returned = new URL(denied.headers.get('location')!);
  expect(returned.origin + returned.pathname).toBe(`${journey.origin}/login/callback`);
  expect(returned.searchParams.get('error')).toBe('access_denied');
  const done = await browser.go(1, returned.pathname + returned.search);
  expect(done.headers.get('location')).toBe(`/t/${tenant}/login?login=failed`);
  expect(browser.cookies.has(sessionCookie(tenant))).toBe(false);
  const page = await browser.go(0, done.headers.get('location')!);
  expect(page.status).toBe(200);
  const html = await page.text();
  expect(html).toContain('role="alert"');
  return html;
}

/** Call a tenant `/v1` API on the server as the holder of `apiKey` (RFC 8693 exchange). */
async function asKey(apiKey: string, method: string, path: string): Promise<Response> {
  const token = await fetch(`${journey.servers[1]}/auth/token`, {
    method: 'POST',
    headers: { 'content-type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({
      grant_type: 'urn:ietf:params:oauth:grant-type:token-exchange',
      subject_token: apiKey,
      subject_token_type: 'urn:wyrd:oauth:token-type:api_key'
    })
  });
  expect(token.status).toBe(200);
  const { access_token } = (await token.json()) as { access_token: string };
  return fetch(`${journey.servers[1]}/v1${path}`, {
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

/** Follow `tenant`'s home page on `replica` until it sends the browser to sign-in. */
async function expectSessionEnds(browser: Browser, replica: 0 | 1, tenant: string) {
  // Each BFF keeps an issued access token until its 30-second journey lifetime runs out.
  const deadline = Date.now() + 45_000;
  let location: string | null = null;
  while (location === null && Date.now() < deadline) {
    const page = await browser.go(replica, `/t/${tenant}`);
    location = page.headers.get('location');
    if (location === null) await new Promise((resolve) => setTimeout(resolve, 1_000));
  }
  expect(location).toBe(`/t/${tenant}/login`);
  expect(browser.cookies.has(sessionCookie(tenant))).toBe(false);
}

/** The page and its data channel carry neither the session cookie nor any token. */
async function expectNoSecrets(browser: Browser, tenant: string, session: string) {
  for (const path of [`/t/${tenant}`, `/t/${tenant}/__data.json`, `/t/${tenant}/settings`]) {
    for (const replica of [0, 1] as const) {
      const response = await browser.go(replica, path);
      // Served to the session (a reader may be denied a page), never sent to sign-in.
      expect([200, 403], path).toContain(response.status);
      const body = await response.text();
      expect(body).not.toContain(session);
      expect(body).not.toMatch(jwt);
    }
  }
  for (const value of browser.seen.filter((seen) => !seen.startsWith('wyrd_'))) {
    expect(value).not.toContain(session);
    expect(value).not.toMatch(jwt);
  }
}

test('production SSO crosses replicas', async () => {
  const sso = journey.ssoTenant;
  const settings = `/t/${sso}/settings`;

  // A missing or forged login cookie redeems nothing and sets no session.
  const alice = new Browser();
  expect((await alice.go(0, `/t/${sso}`)).headers.get('location')).toBe(`/t/${sso}/login`);
  for (const login of [undefined, 'f'.repeat(64)]) {
    if (login) alice.cookies.set('wyrd_login', login);
    const failed = await alice.go(1, '/login/callback?code=c&state=s');
    expect(failed.headers.get('location')).toBe('/?login=failed');
    expect(alice.cookies.has('wyrd_login')).toBe(false);
    expect(alice.cookies.has(sessionCookie(sso))).toBe(false);
  }
  // Two logins in one browser: the first login's state and verifier cannot redeem the second's code.
  await beginSso(alice, 0);
  const firstLogin = alice.cookies.get('wyrd_login')!;
  const returned = await providerCallback(await beginSso(alice, 1), journey.users.admin, 1);
  const secondLogin = alice.cookies.get('wyrd_login')!;
  alice.cookies.set('wyrd_login', firstLogin);
  expect((await alice.go(0, returned)).headers.get('location')).toBe(
    `/t/${sso}/login?login=failed`
  );
  expect(alice.cookies.has(sessionCookie(sso))).toBe(false);

  // Begun at replica 1 (above), redeemed at replica 0.
  alice.cookies.set('wyrd_login', secondLogin);
  const done = await alice.go(0, returned);
  expect(done.headers.get('location')).toBe(`/t/${sso}`);
  const session = alice.cookies.get(sessionCookie(sso))!;
  expectSafeCookie(alice.setCookie(sessionCookie(sso)));
  expectEncrypted(session);
  // The code is one-use: its replay with the same login sets no session.
  const replay = new Browser();
  replay.cookies.set('wyrd_login', secondLogin);
  expect((await replay.go(1, returned)).headers.get('location')).toBe(
    `/t/${sso}/login?login=failed`
  );
  expect(replay.cookies.has(sessionCookie(sso))).toBe(false);

  // Both replicas serve the session from the cookie alone: replica 1 never saw this
  // login, so it renews the access token with the refresh token through Wyrd replica 1.
  const both = await Promise.all([alice.go(0, `/t/${sso}`), alice.go(1, `/t/${sso}`)]);
  expect(both.map((response) => response.status)).toEqual([200, 200]);
  await expectNoSecrets(alice, sso, session);

  // Forged and cross-tenant cookies are refused and cleared; the cookie name is only a hint.
  const forged = new Browser();
  forged.cookies.set(sessionCookie(sso), 'b'.repeat(64));
  expect((await forged.go(1, `/t/${sso}`)).headers.get('location')).toBe(`/t/${sso}/login`);
  expect(forged.cookies.has(sessionCookie(sso))).toBe(false);
  const crossed = new Browser();
  crossed.cookies.set(sessionCookie(journey.apiKeyTenant), session);
  expect((await crossed.go(0, `/t/${journey.apiKeyTenant}/settings`)).headers.get('location')).toBe(
    `/t/${journey.apiKeyTenant}/login`
  );
  expect(crossed.cookies.has(sessionCookie(journey.apiKeyTenant))).toBe(false);
  expect(
    (await alice.go(0, `/t/${journey.apiKeyTenant}/settings?/deactivate`, { form: {} })).status
  ).toBe(401);

  // A cross-site form post is refused (SvelteKit `csrf.checkOrigin`).
  expect(
    (await alice.go(1, `${settings}?/deactivate`, { form: {}, origin: 'http://evil.test' })).status
  ).toBe(403);

  // A reader signs in on the same connection and is denied the admin action.
  const bob = new Browser();
  await ssoLogin(bob, journey.users.reader, 1, 0);
  expect((await bob.go(1, settings)).status).toBe(200);
  expect((await bob.go(0, `${settings}?/deactivate`, { form: {} })).status).toBe(403);

  // With SSO active, the recovery page still signs in with an operator key.
  const operator = new Browser();
  const recovered = await operator.go(0, `/t/${sso}/login/api-key`, {
    form: { apiKey: journey.ssoAdminKey }
  });
  expect(recovered.headers.get('location')).toBe(`/t/${sso}`);
  expect((await operator.go(1, settings)).status).toBe(200);

  // Switch: a tenant without a session in this browser goes to its login; one with a session opens.
  const toLogin = await alice.go(0, '/?/switch', { form: { tenantKey: journey.apiKeyTenant } });
  expect(toLogin.headers.get('location')).toBe(`/t/${journey.apiKeyTenant}/login`);
  const toSso = await alice.go(1, '/?/switch', { form: { tenantKey: sso } });
  expect(toSso.headers.get('location')).toBe(`/t/${sso}`);

  // Logout revokes only this login. Two logins of the same user: the first signs out at
  // replica 1, which revokes its refresh token and clears its cookie.
  const first = new Browser();
  const firstSession = await ssoLogin(first, journey.users.admin, 0, 1);
  const second = new Browser();
  await ssoLogin(second, journey.users.admin, 1, 0);
  const loggedOut = await first.go(1, '/?/logout', { form: { tenantKey: sso } });
  expect(loggedOut.headers.get('location')).toBe(`/t/${sso}/login`);
  expect(first.cookies.has(sessionCookie(sso))).toBe(false);
  expect((await first.go(0, `/t/${sso}`)).headers.get('location')).toBe(`/t/${sso}/login`);
  // The revoked refresh token cannot renew: replaying the old cookie at replica 1 ends it.
  first.cookies.set(sessionCookie(sso), firstSession);
  expect((await first.go(1, `/t/${sso}`)).headers.get('location')).toBe(`/t/${sso}/login`);
  expect(first.cookies.has(sessionCookie(sso))).toBe(false);
  // The other login of the same user still renews: replica 1 has not seen it yet.
  expect((await second.go(1, `/t/${sso}`)).status).toBe(200);
  expect((await second.go(0, `/t/${sso}`)).status).toBe(200);

  // The admin may deactivate; every session on that connection then stops renewing but keeps
  // its already-issued access token until that token's own expiry, so the next page still renders.
  const deactivated = await alice.go(1, `${settings}?/deactivate`, { form: {} });
  expect(deactivated.status).not.toBe(403);
  expect(deactivated.status).toBeLessThan(500);
  const stillIssued = await alice.go(1, `/t/${sso}`);
  expect(stillIssued.status).toBe(200);
  expect(alice.cookies.has(sessionCookie(sso))).toBe(true);
  // Once each issued token expires (30-second journey lifetime), the first use ends the session.
  for (const browser of [alice, bob, second]) await expectSessionEnds(browser, 0, sso);
}, journeyTimeout + 90_000);

test('OIDC-off credential UI', async () => {
  const tenant = journey.apiKeyTenant;
  const login = `/t/${tenant}/login`;
  const settings = `/t/${tenant}/settings`;
  const browser = new Browser();

  const recovery = `${login}/api-key`;
  // Routine sign-in is SSO; the operator key form is a separate recovery page the login page links to.
  const page = await (await browser.go(0, login)).text();
  expect(page).toContain('Sign in with SSO');
  expect(page).not.toContain('name="apiKey"');
  expect(page).toContain(`href="${recovery}"`);
  expect(await (await browser.go(1, recovery)).text()).toContain('name="apiKey"');
  // Without an Active connection, SSO comes back `access_denied` to the sign-in problem page,
  // which links to the recovery page.
  const sso = await providerAuthorizationDenied(browser, tenant);
  expect(sso).toContain(`href="${recovery}"`);

  // An unusable key is refused and sets no session.
  const refused = await browser.go(1, recovery, { form: { apiKey: 'wyrd_not_a_key' } });
  expect(refused.status).toBe(401);
  expect(browser.cookies.has(sessionCookie(tenant))).toBe(false);
  // A cross-site sign-in post is refused.
  expect(
    (
      await browser.go(0, recovery, {
        form: { apiKey: journey.offReaderKey },
        origin: 'http://evil.test'
      })
    ).status
  ).toBe(403);

  // A reader signs in with its key and is denied connection administration.
  const reader = await browser.go(1, recovery, { form: { apiKey: journey.offReaderKey } });
  expect(reader.headers.get('location')).toBe(`/t/${tenant}`);
  expectSafeCookie(browser.setCookie(sessionCookie(tenant)));
  const readerSession = browser.cookies.get(sessionCookie(tenant))!;
  expectEncrypted(readerSession);
  await expectNoSecrets(browser, tenant, readerSession);
  for (const seen of browser.seen) expect(seen).not.toContain(journey.offReaderKey);
  expect((await browser.go(1, `${settings}?/deactivate`, { form: {} })).status).toBe(403);
  const out = await browser.go(0, '/?/logout', { form: { tenantKey: tenant } });
  expect(out.headers.get('location')).toBe(login);
  expect(browser.cookies.has(sessionCookie(tenant))).toBe(false);
  // Signing out of the UI never revokes the operator's API key: it still exchanges.
  await asKey(journey.offReaderKey, 'GET', '/identity/oidc/connections');

  // An admin key opens the session; staging an SSO candidate is allowed.
  const admin = await browser.go(0, recovery, { form: { apiKey: journey.offAdminKey } });
  expect(admin.headers.get('location')).toBe(`/t/${tenant}`);
  expect((await browser.go(1, settings)).status).toBe(200);
  const staged = await browser.go(0, `${settings}?/stage`, {
    form: {
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
  const [stagedId] = [...stagedPage.matchAll(/name="id" value="([0-9a-f-]{36})"/g)].map(
    ([, id]) => id
  );
  expect(stagedId).toBeDefined();

  // Another tenant's key at this tenant's recovery page signs in as the key's own tenant:
  // every call reaches only that tenant, so the server refuses this tenant's connection.
  const crossed = new Browser();
  const other = await crossed.go(1, recovery, { form: { apiKey: journey.ssoAdminKey } });
  expect(other.headers.get('location')).toBe(`/t/${tenant}`);
  const crossedPage = await (await crossed.go(0, settings)).text();
  expect(crossedPage).not.toContain(stagedId);
  const removal = await crossed.go(1, `${settings}?/remove`, { form: { id: stagedId } });
  expect(removal.status).toBeGreaterThanOrEqual(400);
  expect(removal.status).toBeLessThan(500);
  expect(await (await browser.go(0, settings)).text()).toContain(stagedId);
}, journeyTimeout);

test('production multi-provider tenant switch', async () => {
  const { keycloak, second, peer } = journey.switchTenants;
  const browser = new Browser();

  // Independent sessions for two tenants on two different real providers, in one browser.
  const keycloakSession = await ssoLogin(browser, journey.users.admin, 0, 1, keycloak);
  const secondSession = await ssoLogin(browser, journey.users.dex, 1, 0, second, dexIssuer);
  expect(secondSession).not.toBe(keycloakSession);
  expect(browser.cookies.get(sessionCookie(keycloak))).toBe(keycloakSession);

  // Both replicas render a chooser of exactly this browser's sessions; a forged
  // session-cookie hint never renders and is cleared. (Settings render for every
  // member; the Dex tenant's role-less user is denied the home page.)
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
  const toSecond = await browser.go(1, '/?/switch', { form: { tenantKey: second } });
  expect(toSecond.status).toBe(303);
  expect(toSecond.headers.get('location')).toBe(`/t/${second}`);
  expect((await browser.go(1, `/t/${second}/settings`)).status).toBe(200);
  const toKeycloak = await browser.go(0, '/?/switch', { form: { tenantKey: keycloak } });
  expect(toKeycloak.headers.get('location')).toBe(`/t/${keycloak}`);

  // Wrong provider and tenant: Dex's genuine return for the Dex tenant's login
  // cannot complete the Keycloak tenant's login, though the state is genuine.
  const attacker = new Browser();
  const dexReturn = await providerLogin(
    await beginSso(attacker, 0, second, dexIssuer),
    journey.users.dex
  );
  const keycloakState = new URL(await beginSso(attacker, 1, keycloak)).searchParams.get('state')!;
  await expectMixedCallbackRefused(attacker, dexReturn, keycloakState, keycloak);

  // Same issuer and client, other tenant: Keycloak's genuine return — its own
  // `iss` included — for the peer tenant's login cannot complete the Keycloak
  // tenant's login either.
  const peerReturn = await providerLogin(await beginSso(attacker, 0, peer), journey.users.admin);
  expect(new URL(peerReturn).searchParams.has('iss')).toBe(true);
  const secondState = new URL(await beginSso(attacker, 1, keycloak)).searchParams.get('state')!;
  await expectMixedCallbackRefused(attacker, peerReturn, secondState, keycloak);
  expect(attacker.cookies.has(sessionCookie(peer))).toBe(false);

  // The same provider user signed in to the peer is a different tenant's
  // session: it joins this browser's chooser, but never opens the other tenant.
  const peerSession = await ssoLogin(browser, journey.users.admin, 1, 0, peer);
  expect(chooserTenants(await (await browser.go(1, `/t/${keycloak}/settings`)).text())).toEqual(
    [second, keycloak, peer].sort()
  );
  const crossed = new Browser();
  crossed.cookies.set(sessionCookie(keycloak), peerSession);
  expect((await crossed.go(0, `/t/${keycloak}`)).headers.get('location')).toBe(
    `/t/${keycloak}/login`
  );
  expect(crossed.cookies.has(sessionCookie(keycloak))).toBe(false);
  expect((await browser.go(0, `/t/${peer}/settings`)).status).toBe(200);
}, journeyTimeout);

test('production provider replacement settings', async () => {
  const tenant = journey.replacementTenant;
  const settings = `/t/${tenant}/settings`;
  const alice = new Browser();
  const stage = () =>
    alice.go(0, `${settings}?/stage`, {
      form: {
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
  const oldSession = await ssoLogin(alice, journey.users.admin, 0, 1, tenant);
  const [oldConnection] = (await connections()).ids;
  expect(oldConnection).toBeDefined();

  // Stage a second-realm replacement, then remove that candidate through settings.
  expect((await stage()).status).toBe(200);
  const discarded = await connections();
  expect(discarded.ids).toHaveLength(2);
  const removed = await alice.go(1, `${settings}?/remove`, { form: { id: discarded.ids[1] } });
  expect(removed.status).toBe(200);
  expect((await connections()).ids).toEqual([oldConnection]);

  // Stage again, test, and activate. Testing sends the browser to the second
  // realm for one real sign-in; its return to the server's callback marks the
  // revision tested and issues nothing. Activation demands a live recovery key
  // of this tenant: an unusable one is refused and the old connection stays Active.
  expect((await stage()).status).toBe(200);
  const { ids, revision } = await connections();
  expect(ids).toHaveLength(2);
  expect(revision).toBeDefined();
  expect(await (await alice.go(1, settings)).text()).toContain('Not tested');
  const tested = await alice.go(0, `${settings}?/test`, { form: { revision: revision! } });
  expect(tested.status).toBe(303);
  const testSignIn = tested.headers.get('location')!;
  expect(testSignIn.startsWith(secondIssuer)).toBe(true);
  const testReturn = await providerLogin(testSignIn, journey.users.admin);
  expect(testReturn.startsWith(`${journey.origin}/auth/callback?`)).toBe(true);
  const testCallback = await fetch(gateway(testReturn, 1), { redirect: 'manual' });
  expect(testCallback.status, await testCallback.clone().text()).toBe(200);
  expect(await testCallback.text()).toContain('Connection test complete');
  expect(testCallback.headers.getSetCookie()).toEqual([]);
  expect(await (await alice.go(1, settings)).text()).toContain('Passed');
  for (const recoveryApiKey of ['wyrd_not_a_key', journey.ssoAdminKey]) {
    const refused = await alice.go(1, `${settings}?/activate`, {
      form: { revision: revision!, recoveryApiKey }
    });
    expect(refused.status).toBeGreaterThanOrEqual(400);
    expect(refused.status).toBeLessThan(500);
    expect((await connections()).ids).toEqual(ids);
  }
  const activated = await alice.go(0, `${settings}?/activate`, {
    form: { revision: revision!, recoveryApiKey: journey.replacementOwnerKey }
  });
  expect(activated.status).toBe(200);
  for (const seen of alice.seen) expect(seen).not.toContain(journey.replacementOwnerKey);

  // The retired connection's session cannot refresh: it keeps its already-issued
  // access token until that token's expiry, and its first use afterwards ends it
  // on either replica.
  await expectSessionEnds(alice, 1, tenant);
  alice.cookies.set(sessionCookie(tenant), oldSession);
  expect((await alice.go(0, `/t/${tenant}`)).headers.get('location')).toBe(`/t/${tenant}/login`);
  expect(alice.cookies.has(sessionCookie(tenant))).toBe(false);

  // The second realm's alice — same email — is a new User with none of the old authority.
  const replaced = new Browser();
  await ssoLogin(replaced, journey.users.admin, 1, 0, tenant, secondIssuer);
  for (const action of ['deactivate', 'remove']) {
    const denied = await replaced.go(1, `${settings}?/${action}`, { form: { id: oldConnection } });
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
  alice.cookies.set(sessionCookie(tenant), oldSession);
  expect((await alice.go(1, `/t/${tenant}`)).headers.get('location')).toBe(`/t/${tenant}/login`);
  // Removing the retired connection leaves the replacement's session live.
  expect((await replaced.go(0, settings)).status).toBe(200);
  expect((await replaced.go(1, settings)).status).toBe(200);
}, journeyTimeout + 60_000);
