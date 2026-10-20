// @vitest-environment node
/**
 * Production BFF journeys over real HTTP. `identity_ui_e2e.rs` starts two Wyrd
 * server replicas over one Postgres, two `node build` BFF replicas sharing one
 * public origin (one per server replica), Keycloak (two realms), and Dex, then
 * runs this file with the `WYRD_UI_JOURNEY` fixture. Every request goes over
 * the wire; nothing here mocks the BFF or the server.
 *
 * Rust owns every browser session: `POST /auth/browser/login` binds a login
 * to this browser's `wyrd_login` cookie, the provider returns to Wyrd's
 * `/auth/callback` (forwarded by the BFF on the public origin), and the
 * callback sets an opaque `wyrd_session_<tenant>` cookie and answers `303` to
 * the tenant. The BFF forwards only the selected session cookie and the
 * trusted origin; it holds no token, key, or cache, so any replica — and a
 * replica started after sign-in — serves the session. Logout, deactivation,
 * and connection replacement end sessions at once.
 */
import { spawn } from 'node:child_process';
import { expect, test } from 'vitest';

type Journey = {
  origin: string;
  /** The Wyrd replicas, in BFF order: each BFF forwards `/auth/*` to its own. */
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
/** Each journey drives many real logins and key verifications. */
const journeyTimeout = 60_000;

/**
 * Forwarded-origin headers a hostile browser sends on every request. Each BFF
 * establishes the public origin from its own configuration, so these never
 * reach a sign-in redirect, callback, or cookie.
 */
const hostileForwarding = { 'x-forwarded-proto': 'https', 'x-forwarded-host': 'evil.test' };

/**
 * A browser's cookie-less visit to a public-origin `/auth/*` URL, which BFF
 * `replica` forwards to its Wyrd replica; the redirect is returned, not followed.
 */
function gateway(url: string, replica: 0 | 1 = 0): Promise<Response> {
  const target = new URL(url);
  expect(target.origin).toBe(journey.origin);
  return fetch(journey.bffs[replica] + target.pathname + target.search, {
    redirect: 'manual',
    headers: hostileForwarding
  });
}

/**
 * One browser behind a TCP load balancer: a host-only cookie jar for the
 * public origin, sent to whichever BFF replica a request is routed to. The jar
 * ignores cookie paths, so every cookie reaches every route: the BFF itself
 * must select which one Wyrd sees.
 */
class Browser {
  readonly cookies = new Map<string, string>();
  /** Every `Set-Cookie` and `Location` the BFFs returned, for leak checks. */
  readonly seen: string[] = [];

  async go(
    replica: 0 | 1 | string,
    path: string,
    options: { form?: Record<string, string>; json?: unknown; origin?: string } = {}
  ): Promise<Response> {
    const headers = new Headers({ accept: 'text/html', ...hostileForwarding });
    if (this.cookies.size)
      headers.set('cookie', [...this.cookies].map(([k, v]) => `${k}=${v}`).join('; '));
    const posted = options.form !== undefined || options.json !== undefined;
    if (posted) {
      headers.set(
        'content-type',
        options.form ? 'application/x-www-form-urlencoded' : 'application/json'
      );
      headers.set('origin', options.origin ?? journey.origin);
    }
    const base = typeof replica === 'string' ? replica : journey.bffs[replica];
    const response = await fetch(base + path, {
      method: posted ? 'POST' : 'GET',
      headers,
      body: options.form
        ? new URLSearchParams(options.form)
        : posted
          ? JSON.stringify(options.json)
          : undefined,
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
    const raw = this.seen.findLast((value) => value.startsWith(`${name}=`) && !/max-age=0/i.test(value));
    expect(raw, `${name} was set`).toBeDefined();
    return raw!;
  }
}

/** Host-only, HttpOnly, SameSite=Lax on `path`; not `Secure` on the journey's plain-http loopback origin. */
function expectSafeCookie(raw: string, path = '/'): void {
  expect(raw).toMatch(/;\s*HttpOnly/i);
  expect(raw).not.toMatch(/;\s*Secure/i);
  expect(raw).toMatch(/;\s*SameSite=Lax/i);
  expect(raw).toMatch(new RegExp(`;\\s*Path=${path.replaceAll('/', '\\/')}(;|$)`, 'i'));
  expect(raw).not.toMatch(/;\s*Domain=/i);
}

/** A Wyrd cookie value is an opaque identifier: no JSON, token, or key inside it. */
function expectOpaque(value: string): void {
  expect(value).toMatch(/^[\w.~-]{32,}$/);
  expect(value).not.toMatch(jwt);
  expect(value).not.toContain('wyrd_');
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
 * Begin SSO at one replica: the BFF asks Wyrd to begin the login, relays the
 * `wyrd_login` binding cookie, and sends the browser straight to the tenant's
 * provider with Wyrd's own callback. Returns the provider URL.
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
  expectSafeCookie(browser.setCookie('wyrd_login'), '/auth/callback');
  expect(browser.setCookie('wyrd_login')).toMatch(/;\s*Max-Age=300/i);
  expectOpaque(browser.cookies.get('wyrd_login')!);
  const authorization = begun.headers.get('location')!;
  expect(authorization.startsWith(issuer)).toBe(true);
  const query = new URL(authorization).searchParams;
  expect(query.get('redirect_uri')).toBe(`${journey.origin}/auth/callback`);
  expect(query.get('code_challenge_method')).toBe('S256');
  expect(query.get('state')).toBeTruthy();
  return authorization;
}

/** The provider's return to Wyrd's callback, as a path and query on the public origin. */
async function providerReturn(authorization: string, user: Record<string, string>): Promise<string> {
  const callback = new URL(await providerLogin(authorization, user));
  expect(callback.origin + callback.pathname).toBe(`${journey.origin}/auth/callback`);
  return callback.pathname + callback.search;
}

/**
 * Complete the provider return at BFF `replica`: Wyrd sets the tenant's
 * opaque session cookie, clears the login binding, and sends the browser home.
 * Returns the session cookie value.
 */
async function finishSso(browser: Browser, returned: string, replica: 0 | 1, tenant: string) {
  const done = await browser.go(replica, returned);
  expect(done.status, await done.clone().text()).toBe(303);
  expect(done.headers.get('location')).toBe(`/t/${tenant}`);
  expect(browser.cookies.has('wyrd_login')).toBe(false);
  const session = browser.cookies.get(sessionCookie(tenant))!;
  expectSafeCookie(browser.setCookie(sessionCookie(tenant)));
  expect(browser.setCookie(sessionCookie(tenant))).toMatch(/;\s*Max-Age=43200/i);
  expectOpaque(session);
  return session;
}

/** Full SSO sign-in: begin at `start`, finish the callback at `finish`; returns the session cookie. */
async function ssoLogin(
  browser: Browser,
  user: Record<string, string>,
  start: 0 | 1,
  finish: 0 | 1,
  tenant = journey.ssoTenant,
  issuer = keycloakIssuer
): Promise<string> {
  const returned = await providerReturn(await beginSso(browser, start, tenant, issuer), user);
  return finishSso(browser, returned, finish, tenant);
}

/**
 * A callback the server refuses after consuming its login: no session, the
 * binding cleared, and the browser sent to the tenant's sign-in problem.
 */
async function expectLoginFailed(browser: Browser, returned: string, tenant: string) {
  const failed = await browser.go(1, returned);
  expect(failed.status, await failed.clone().text()).toBe(303);
  expect(failed.headers.get('location')).toBe(`/t/${tenant}/login?login=failed`);
  expect(browser.cookies.has('wyrd_login')).toBe(false);
  expect(browser.cookies.has(sessionCookie(tenant))).toBe(false);
}

/** A callback whose login state is unknown or spent: the stable problem, and no cookie at all. */
async function expectInvalidState(browser: Browser, returned: string, replica: 0 | 1 = 0) {
  const sessions = new Map(browser.cookies);
  const refused = await browser.go(replica, returned);
  expect(refused.status).toBe(400);
  expect(((await refused.json()) as { code: string }).code).toBe('WYRD_AUTH_400_INVALID_STATE');
  expect(refused.headers.getSetCookie()).toEqual([]);
  expect(browser.cookies).toEqual(sessions);
}

/**
 * Replay one provider's genuine return to the common callback under another
 * login's state, as a mix-up or injection attacker would: every parameter the
 * provider sent (`code`, any RFC 9207 `iss`, and the rest) is kept and only
 * `state` is replaced. The browser holds that other login's binding, so the
 * server consumes it and then refuses the foreign code: no session.
 */
async function expectMixedCallbackRefused(
  browser: Browser,
  returned: string,
  state: string,
  tenant: string
) {
  const query = new URL(returned, journey.origin).searchParams;
  query.set('state', state);
  await expectLoginFailed(browser, `/auth/callback?${query}`, tenant);
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

/** `tenant`'s home page on `replica` sends the browser to sign-in at once and clears its cookie. */
async function expectSessionEnded(browser: Browser, replica: 0 | 1, tenant: string) {
  const page = await browser.go(replica, `/t/${tenant}`);
  expect(page.headers.get('location')).toBe(`/t/${tenant}/login`);
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

/**
 * A BFF replica started after sign-in, as a rollout replaces one: the same
 * build and configuration as the fixture's first replica, in front of server
 * replica A. Resolves to its URL and a function that stops it.
 */
async function freshBff(): Promise<{ url: string; stop: () => void }> {
  const port = 20_000 + Math.floor(Math.random() * 20_000);
  const child = spawn('node', ['build'], {
    env: {
      ...process.env,
      HOST: '127.0.0.1',
      PORT: String(port),
      ORIGIN: journey.origin,
      WYRD_SERVER_URL: journey.servers[0]
    },
    stdio: 'inherit'
  });
  const url = `http://127.0.0.1:${port}`;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (await fetch(`${url}/`).then(() => true, () => false)) return { url, stop: () => child.kill() };
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  child.kill();
  throw new Error(`the fresh BFF on port ${port} never answered`);
}

test('production SSO crosses replicas', async () => {
  const sso = journey.ssoTenant;
  const settings = `/t/${sso}/settings`;

  // A missing or forged login binding at an unknown state is the stable problem and sets nothing.
  const alice = new Browser();
  expect((await alice.go(0, `/t/${sso}`)).headers.get('location')).toBe(`/t/${sso}/login`);
  for (const login of [undefined, 'f'.repeat(64)]) {
    if (login) alice.cookies.set('wyrd_login', login);
    await expectInvalidState(alice, '/auth/callback?code=c&state=s', 1);
    expect(alice.cookies.has(sessionCookie(sso))).toBe(false);
  }
  alice.cookies.delete('wyrd_login');
  // The retired BFF callback no longer exists: it signs nobody in.
  const retired = await alice.go(0, '/login/callback?code=c&state=s');
  expect(retired.status).toBe(404);
  expect(alice.cookies.has(sessionCookie(sso))).toBe(false);

  // Two logins in one browser: the first login's binding cannot complete the second's
  // return, and that refusal spends the second login, so its own binding cannot replay it.
  await beginSso(alice, 0);
  const firstLogin = alice.cookies.get('wyrd_login')!;
  const stolen = await providerReturn(await beginSso(alice, 1), journey.users.admin);
  const secondLogin = alice.cookies.get('wyrd_login')!;
  alice.cookies.set('wyrd_login', firstLogin);
  await expectLoginFailed(alice, stolen, sso);
  alice.cookies.set('wyrd_login', secondLogin);
  await expectInvalidState(alice, stolen);
  alice.cookies.delete('wyrd_login');

  // Begun at replica 1, completed at replica 0; the return is one-use, even with its binding.
  const returned = await providerReturn(await beginSso(alice, 1), journey.users.admin);
  const binding = alice.cookies.get('wyrd_login')!;
  const session = await finishSso(alice, returned, 0, sso);
  const replay = new Browser();
  replay.cookies.set('wyrd_login', binding);
  await expectInvalidState(replay, returned, 1);
  expect(replay.cookies.has(sessionCookie(sso))).toBe(false);

  // Both replicas serve the session from the cookie alone, and so does a BFF replica
  // started only now: Rust resolves the session, no Node process holds it.
  const both = await Promise.all([alice.go(0, `/t/${sso}`), alice.go(1, `/t/${sso}`)]);
  expect(both.map((response) => response.status)).toEqual([200, 200]);
  const fresh = await freshBff();
  try {
    expect((await alice.go(fresh.url, settings)).status).toBe(200);
  } finally {
    fresh.stop();
  }
  await expectNoSecrets(alice, sso, session);
  expect(alice.cookies.get(sessionCookie(sso))).toBe(session);

  // Forged, cross-tenant, and old credential cookies are refused and cleared, never
  // read as credentials: the cookie name is only a hint.
  const oldCredential = encodeURIComponent(
    JSON.stringify({ tenant: sso, kind: 'refresh', credential: 'wyrd_rt_old' })
  );
  for (const forgedValue of ['b'.repeat(64), oldCredential]) {
    const forged = new Browser();
    forged.cookies.set(sessionCookie(sso), forgedValue);
    expect((await forged.go(1, `/t/${sso}`)).headers.get('location')).toBe(`/t/${sso}/login`);
    expect(forged.cookies.has(sessionCookie(sso))).toBe(false);
  }
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
  expectOpaque(operator.cookies.get(sessionCookie(sso))!);
  expect((await operator.go(1, settings)).status).toBe(200);

  // Switch: a tenant without a session in this browser goes to its login; one with a session opens.
  const toLogin = await alice.go(0, '/?/switch', { form: { tenantKey: journey.apiKeyTenant } });
  expect(toLogin.headers.get('location')).toBe(`/t/${journey.apiKeyTenant}/login`);
  const toSso = await alice.go(1, '/?/switch', { form: { tenantKey: sso } });
  expect(toSso.headers.get('location')).toBe(`/t/${sso}`);

  // Logout revokes only this login. Two logins of the same user: the first signs out at
  // replica 1, which revokes its session and clears its cookie.
  const first = new Browser();
  const firstSession = await ssoLogin(first, journey.users.admin, 0, 1);
  const second = new Browser();
  await ssoLogin(second, journey.users.admin, 1, 0);
  const loggedOut = await first.go(1, '/?/logout', { form: { tenantKey: sso } });
  expect(loggedOut.headers.get('location')).toBe(`/t/${sso}/login`);
  expect(first.cookies.has(sessionCookie(sso))).toBe(false);
  expect((await first.go(0, `/t/${sso}`)).headers.get('location')).toBe(`/t/${sso}/login`);
  // The revoked session is dead on every replica: replaying its cookie ends it.
  for (const replica of [0, 1] as const) {
    first.cookies.set(sessionCookie(sso), firstSession);
    await expectSessionEnded(first, replica, sso);
  }
  // The other login of the same user is untouched.
  expect((await second.go(1, `/t/${sso}`)).status).toBe(200);
  expect((await second.go(0, `/t/${sso}`)).status).toBe(200);

  // The admin may deactivate; every session signed in through that connection ends at once.
  const deactivated = await alice.go(1, `${settings}?/deactivate`, { form: {} });
  expect(deactivated.status).not.toBe(403);
  expect(deactivated.status).toBeLessThan(500);
  await expectSessionEnded(alice, 0, sso);
  await expectSessionEnded(bob, 1, sso);
  await expectSessionEnded(second, 0, sso);
  // The operator's recovery session rests on its key, not the connection, and stays live.
  expect((await operator.go(0, settings)).status).toBe(200);
}, journeyTimeout + 30_000);

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
  // Without an Active connection, Wyrd begins no login: the sign-in page renders the
  // problem, links to recovery, and no login binding is set.
  const denied = await browser.go(0, `${login}?/sso`, { form: {} });
  expect(denied.status).toBe(401);
  const deniedPage = await denied.text();
  expect(deniedPage).toContain('role="alert"');
  expect(deniedPage).toContain(`href="${recovery}"`);
  expect(browser.cookies.has('wyrd_login')).toBe(false);

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
  expectOpaque(readerSession);
  await expectNoSecrets(browser, tenant, readerSession);
  // No cookie, redirect, or page carries the key.
  for (const seen of browser.seen) expect(seen).not.toContain(journey.offReaderKey);
  expect((await browser.go(1, `${settings}?/deactivate`, { form: {} })).status).toBe(403);
  const out = await browser.go(0, '/?/logout', { form: { tenantKey: tenant } });
  expect(out.headers.get('location')).toBe(login);
  expect(browser.cookies.has(sessionCookie(tenant))).toBe(false);
  browser.cookies.set(sessionCookie(tenant), readerSession);
  await expectSessionEnded(browser, 1, tenant);
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

  // Another tenant's key at this tenant's recovery page signs nobody in: the key's own
  // tenant is not this one, and this tenant's candidate is untouched.
  const crossed = new Browser();
  const other = await crossed.go(1, recovery, { form: { apiKey: journey.ssoAdminKey } });
  expect(other.status).toBe(401);
  expect(crossed.cookies.has(sessionCookie(tenant))).toBe(false);
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
  // cannot complete the Keycloak tenant's login, though the state and binding are genuine.
  const attacker = new Browser();
  const dexReturn = await providerReturn(
    await beginSso(attacker, 0, second, dexIssuer),
    journey.users.dex
  );
  const keycloakState = new URL(await beginSso(attacker, 1, keycloak)).searchParams.get('state')!;
  await expectMixedCallbackRefused(attacker, dexReturn, keycloakState, keycloak);

  // Same issuer and client, other tenant: Keycloak's genuine return — its own
  // `iss` included — for the peer tenant's login cannot complete the Keycloak
  // tenant's login either.
  const peerReturn = await providerReturn(await beginSso(attacker, 0, peer), journey.users.admin);
  expect(new URL(peerReturn, journey.origin).searchParams.has('iss')).toBe(true);
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

  // Signing out of one tenant leaves the others signed in.
  await browser.go(1, '/?/logout', { form: { tenantKey: peer } });
  expect(browser.cookies.has(sessionCookie(peer))).toBe(false);
  expect(chooserTenants(await (await browser.go(0, `/t/${keycloak}/settings`)).text())).toEqual(
    [second, keycloak].sort()
  );

  // The public browser-session operations on this origin carry the session cookies to
  // Rust: enumeration lists each live tenant, and logout durably revokes only the selected one.
  /** The tenants the public enumeration answers live at `replica`, or its status. */
  const live = async (replica: 0 | 1, query = '') => {
    const answer = await browser.go(replica, `/auth/browser/session${query}`);
    if (!answer.ok) return answer.status;
    return ((await answer.json()) as { sessions: { tenant: string }[] }).sessions
      .map(({ tenant }) => tenant)
      .sort();
  };
  expect(await live(0)).toEqual([second, keycloak].sort());
  expect(await live(1, `?tenant=${second}`)).toEqual([second]);
  const loggedOut = await browser.go(1, '/auth/browser/logout', { json: { tenant: second } });
  expect(loggedOut.status, await loggedOut.clone().text()).toBe(204);
  expect(browser.cookies.has(sessionCookie(second))).toBe(false);
  browser.cookies.set(sessionCookie(second), secondSession);
  expect(await live(0, `?tenant=${second}`)).toBe(401);
  expect(browser.cookies.has(sessionCookie(second))).toBe(false);
  expect(await live(1)).toEqual([keycloak]);
  expect(browser.cookies.get(sessionCookie(keycloak))).toBe(keycloakSession);
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
  const testCallback = await gateway(testReturn, 1);
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

  // The retired connection's session ends at once on every replica.
  await expectSessionEnded(alice, 1, tenant);
  alice.cookies.set(sessionCookie(tenant), oldSession);
  await expectSessionEnded(alice, 0, tenant);

  // The second realm's alice — same email — is a new User with none of the old authority.
  const replaced = new Browser();
  await ssoLogin(replaced, journey.users.admin, 1, 0, tenant, secondIssuer);
  for (const action of ['deactivate', 'remove']) {
    const denied = await replaced.go(1, `${settings}?/${action}`, { form: { id: oldConnection } });
    expect(denied.status, action).toBe(403);
  }

  // The owner's recovery key still administers: it sees the second realm Active and removes
  // the retired first-realm connection.
  const listed = await asKey(journey.replacementOwnerKey, 'GET', '/identity/oidc/connections');
  expect(listed.status).toBe(200);
  const view = (await listed.json()) as { active: { id: string; issuer: string } | null };
  expect(view.active?.issuer.replace(/\/$/, '')).toBe(secondIssuer);
  expect(view.active?.id).toBe(ids[1]);
  const retiredConnection = `/identity/oidc/connections/${oldConnection}`;
  expect((await asKey(journey.replacementOwnerKey, 'DELETE', retiredConnection)).status).toBeLessThan(300);
  expect((await asKey(journey.replacementOwnerKey, 'DELETE', retiredConnection)).status).toBe(404);
  alice.cookies.set(sessionCookie(tenant), oldSession);
  expect((await alice.go(1, `/t/${tenant}`)).headers.get('location')).toBe(`/t/${tenant}/login`);
  // Removing the retired connection leaves the replacement's session live.
  expect((await replaced.go(0, settings)).status).toBe(200);
  expect((await replaced.go(1, settings)).status).toBe(200);
}, journeyTimeout);
