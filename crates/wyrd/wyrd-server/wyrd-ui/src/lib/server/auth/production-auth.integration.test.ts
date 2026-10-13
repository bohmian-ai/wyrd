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
import { spawn } from 'node:child_process';
import { cp, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createInterface } from 'node:readline';
import { chromium, type Browser as Chromium, type BrowserContext, type Page } from 'playwright';
import { expect, test } from 'vitest';
import { WyrdClient } from '@wyrd/sdk';
import { serviceWorkload } from './service-workload';

type Journey = {
  origin: string;
  /** The Wyrd replicas, in BFF order: the server routes the gateway sends `/auth/*` to. */
  servers: [string, string];
  /** Each replica's public gRPC listener, in the same order. */
  grpcs: [string, string];
  bffs: [string, string];
  ssoTenant: string;
  apiKeyTenant: string;
  ssoAdminKey: string;
  offAdminKey: string;
  offReaderKey: string;
  /** An OIDC-off service principal with no Role: every protected read is refused. */
  offNoRoleKey: string;
  switchTenants: { keycloak: string; second: string; peer: string };
  replacementTenant: string;
  replacementOwnerKey: string;
  /**
   * Each provider's login form fields: Keycloak's admin and reader, Dex's user, and the
   * fresh Keycloak person (with her `email`) who has no Wyrd principal until she signs in.
   */
  users: Record<'admin' | 'reader' | 'dex' | 'fresh', Record<string, string>>;
  /** The built `wyrd` CLI. */
  cli: string;
  /** Local directory for the browser journey's screenshots; never checked in. */
  evidence: string;
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

  // The retired connection's session cannot refresh: each replica keeps its own
  // already-issued access token until that token's expiry (issued in different
  // seconds, so they can expire a second apart), and the first use afterwards
  // ends the session on that replica.
  await expectSessionEnds(alice, 1, tenant);
  alice.cookies.set(sessionCookie(tenant), oldSession);
  await expectSessionEnds(alice, 0, tenant);

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

/** The `user` principals with `email`, read with the tenant administrator's key. */
async function usersWithEmail(email: string): Promise<{ principal_id: string }[]> {
  const listed = await asKey(
    journey.ssoAdminKey,
    'GET',
    `/principals?kind=user&email=${encodeURIComponent(email)}`
  );
  expect(listed.status).toBe(200);
  return ((await listed.json()) as { principals: { principal_id: string }[] }).principals;
}

/** Sign in at the rendered tenant login page and the provider's form; returns the shown principal. */
async function browserLogin(page: Page, tenant: string, user: Record<string, string>) {
  await page.goto(`${journey.origin}/t/${tenant}`);
  expect(new URL(page.url()).pathname).toBe(`/t/${tenant}/login`);
  await page.getByRole('button', { name: 'Sign in with SSO' }).click();
  await page.waitForURL((url) => url.href.startsWith(keycloakIssuer));
  await page.fill('#username', user.username);
  await page.fill('#password', user.password);
  await page.click('#kc-login');
  await page.waitForURL(`${journey.origin}/t/${tenant}`);
  return shownPrincipal(page, tenant);
}

/** The signed-in principal the tenant's Settings page shows. */
async function shownPrincipal(page: Page, tenant: string): Promise<string> {
  await page.goto(`${journey.origin}/t/${tenant}/settings`);
  return (await page.locator('details.principal summary').textContent())!.trim();
}

/** The `wyrd_session_*` cookies the browser holds. */
async function sessionCookies(context: BrowserContext): Promise<string[]> {
  return (await context.cookies()).map(({ name }) => name).filter((name) => name.startsWith('wyrd_session_'));
}

/**
 * Run the real `wyrd` CLI with only `config` as its configuration directory, so no
 * credential of the developer's shell reaches it. `onLine` sees each stderr line.
 */
function wyrd(config: string, args: string[], onLine: (line: string) => void = () => {}) {
  const child = spawn(journey.cli, args, {
    env: { PATH: process.env.PATH, WYRD_CONFIG_HOME: config }
  });
  let output = '';
  child.stdout.on('data', (chunk) => (output += chunk));
  createInterface({ input: child.stderr }).on('line', (line) => {
    output += `${line}\n`;
    onLine(line);
  });
  return new Promise<{ code: number | null; output: string }>((resolve) =>
    child.on('close', (code) => resolve({ code, output }))
  );
}

/** The `principal.id` claim of a clear-text JWT. */
const principalOf = (token: string) =>
  (JSON.parse(Buffer.from(token.split('.')[1], 'base64url').toString()) as {
    principal: { id: string };
  }).principal.id;

test('live UI login and CLI', async () => {
  const sso = journey.ssoTenant;
  const fresh = journey.users.fresh;
  await mkdir(journey.evidence, { recursive: true });
  const config = await mkdtemp(join(tmpdir(), 'wyrd-live-cli-'));
  const browser = await chromium.launch();
  try {
    // The deployment publishes its OAuth metadata on the public origin, through the gateway.
    const metadata = await fetch(`${journey.origin}/.well-known/oauth-authorization-server`);
    expect(metadata.status).toBe(200);
    const advertised = (await metadata.json()) as Record<string, unknown>;
    expect(advertised.issuer).toBe(journey.origin);
    for (const endpoint of ['authorization_endpoint', 'token_endpoint', 'device_authorization_endpoint'])
      expect(String(advertised[endpoint]).startsWith(`${journey.origin}/auth/`), endpoint).toBe(true);

    // The fresh Keycloak person has no Wyrd principal before her first sign-in.
    expect(await usersWithEmail(fresh.email)).toEqual([]);
    const context = await browser.newContext();
    const page = await context.newPage();
    const browserRequests: string[] = [];
    page.on('request', (request) => browserRequests.push(request.url()));

    // The signed-out page is server-rendered: its sign-in control is in the initial HTML.
    const signedOut = await page.goto(`${journey.origin}/t/${sso}`);
    expect(new URL(page.url()).pathname).toBe(`/t/${sso}/login`);
    expect(await signedOut!.text()).toContain('Sign in with SSO');

    // A wrong password stays at the provider and creates no session or principal.
    await page.getByRole('button', { name: 'Sign in with SSO' }).click();
    await page.waitForURL((url) => url.href.startsWith(keycloakIssuer));
    await page.fill('#username', fresh.username);
    await page.fill('#password', 'not-her-password');
    await page.click('#kc-login');
    await expect.poll(() => page.locator('#input-error').count()).toBe(1);
    expect(await sessionCookies(context)).toEqual([]);
    expect(await usersWithEmail(fresh.email)).toEqual([]);
    // A provider return this login did not receive signs nobody in, and the tenant's
    // sign-in page says so.
    await page.goto(`${journey.origin}/login/callback?code=forged&state=forged`);
    expect(page.url()).toBe(`${journey.origin}/t/${sso}/login?login=failed`);
    await expect.poll(() => page.locator('[role="alert"]').count()).toBeGreaterThan(0);
    expect(await sessionCookies(context)).toEqual([]);

    // First sign-in creates exactly one Wyrd user, the principal the page shows.
    const principal = await browserLogin(page, sso, fresh);
    const created = await usersWithEmail(fresh.email);
    expect(created.map(({ principal_id }) => principal_id)).toEqual([principal]);

    // The signed-in page's content is in its initial HTML; hydration fetches no page data.
    browserRequests.length = 0;
    const settings = await page.goto(`${journey.origin}/t/${sso}/settings`);
    expect(await settings!.text()).toContain(principal);
    await page.waitForLoadState('networkidle');
    expect(browserRequests.filter((url) => url.includes('__data.json'))).toEqual([]);
    await page.screenshot({ path: join(journey.evidence, 'settings.png'), fullPage: true });
    // Client navigation runs the server load through SvelteKit's data channel; the browser
    // itself never calls a protected Wyrd API.
    await page.locator('#primary-navigation a', { hasText: 'Home' }).click();
    await page.waitForURL(`${journey.origin}/t/${sso}`);
    expect(browserRequests.some((url) => url.startsWith(`${journey.origin}/t/${sso}/__data.json`))).toBe(true);
    expect(browserRequests.filter((url) => new URL(url).pathname.startsWith('/v1/'))).toEqual([]);
    await page.screenshot({ path: join(journey.evidence, 'home.png'), fullPage: true });

    // The same person authorizes the CLI: it talks only to the configured Wyrd origin, and the
    // browser approves the code there, signed in at the provider already.
    let approved: Promise<void> | undefined;
    const login = wyrd(config, ['auth', 'login', '--server', journey.origin, '--tenant', sso, '--no-browser'], (line) => {
      const url = line.trim();
      if (approved || !url.startsWith('http')) return;
      expect(new URL(url).origin).toBe(journey.origin);
      approved = (async () => {
        await page.goto(url);
        await page.getByRole('button', { name: 'Approve and sign in' }).click();
        await page.waitForURL((current) => current.href.startsWith(`${journey.origin}/auth/callback`));
        await page.screenshot({ path: join(journey.evidence, 'cli-approved.png'), fullPage: true });
      })();
    });
    const loggedIn = await login;
    await approved;
    expect(loggedIn.code, loggedIn.output).toBe(0);
    expect(loggedIn.output).toContain(`Logged in to ${journey.origin} tenant ${sso}.`);
    expect(loggedIn.output).not.toMatch(jwt);
    // The CLI's saved credential is the same Wyrd user, and it reads through the origin.
    const saved = await readFile(join(config, 'credentials.toml'), 'utf8');
    expect(principalOf(/access_token = "([^"]+)"/.exec(saved)![1])).toBe(principal);
    const read = await wyrd(config, ['list', '--server', journey.origin]);
    expect(read.code, read.output).toBe(0);
    expect(read.output).not.toMatch(jwt);
    expect((await usersWithEmail(fresh.email)).map(({ principal_id }) => principal_id)).toEqual([principal]);

    // Changed tenant URLs never reuse this session: each sends to that tenant's sign-in.
    for (const tenant of [journey.apiKeyTenant, journey.switchTenants.peer]) {
      const other = await page.goto(`${journey.origin}/t/${tenant}/settings`);
      expect(new URL(page.url()).pathname).toBe(`/t/${tenant}/login`);
      expect(await other!.text()).not.toContain(principal);
    }

    // A restricted person in another tenant runs beside her without sharing anything.
    const reader = await browser.newContext();
    const readerPage = await reader.newPage();
    const peer = journey.switchTenants.peer;
    const readerPrincipal = await browserLogin(readerPage, peer, journey.users.reader);
    expect(readerPrincipal).not.toBe(principal);
    expect(await sessionCookies(reader)).toEqual([sessionCookie(peer)]);
    await readerPage.goto(`${journey.origin}/t/${sso}`);
    expect(new URL(readerPage.url()).pathname).toBe(`/t/${sso}/login`);
    expect(await shownPrincipal(page, sso)).toBe(principal);
    await reader.close();

    // Signing out ends the session; signing in again is the same single principal.
    const stale = (await context.cookies()).find(({ name }) => name === sessionCookie(sso))!;
    await page.locator('details.principal summary').click();
    await page.getByRole('button', { name: 'Sign out' }).click();
    await page.waitForURL(`${journey.origin}/t/${sso}/login`);
    expect(await sessionCookies(context)).toEqual([]);
    await page.goto(`${journey.origin}/t/${sso}/settings`);
    expect(new URL(page.url()).pathname).toBe(`/t/${sso}/login`);
    await page.getByRole('button', { name: 'Sign in with SSO' }).click();
    // The provider still knows her, so no form is shown.
    await page.waitForURL(`${journey.origin}/t/${sso}`);
    expect(await shownPrincipal(page, sso)).toBe(principal);
    expect((await usersWithEmail(fresh.email)).map(({ principal_id }) => principal_id)).toEqual([principal]);

    // The signed-out session's cookie, replayed, cannot renew: once its issued access
    // token expires every navigation sends to sign-in and shows nothing protected.
    const replay = await browser.newContext();
    await replay.addCookies([stale]);
    const replayPage = await replay.newPage();
    const deadline = Date.now() + 45_000;
    for (;;) {
      const response = await replayPage.goto(`${journey.origin}/t/${sso}/settings`);
      if (new URL(replayPage.url()).pathname === `/t/${sso}/login`) {
        expect(await response!.text()).not.toContain(principal);
        break;
      }
      expect(Date.now()).toBeLessThan(deadline);
      await new Promise((resolve) => setTimeout(resolve, 1_000));
    }
    expect(await sessionCookies(replay)).toEqual([]);
    await replayPage.screenshot({ path: join(journey.evidence, 'session-ended.png'), fullPage: true });
  } finally {
    await browser.close();
    await rm(config, { recursive: true, force: true });
  }
}, 180_000);

/** The tenant's Card names, read with the tenant administrator's key. */
async function registeredCards(apiKey: string): Promise<{ name: string; card_uid: string }[]> {
  const listed = await asKey(apiKey, 'GET', '/cards');
  expect(listed.status).toBe(200);
  const page = (await listed.json()) as { items: { name: string; card_uid: string }[]; next_cursor?: string | null };
  expect(page.next_cursor ?? null).toBeNull();
  return page.items;
}

/** Sign in to an OIDC-off tenant through the rendered API-key recovery form. */
async function apiKeyLogin(page: Page, tenant: string, apiKey: string) {
  await page.goto(`${journey.origin}/t/${tenant}/login/api-key`);
  await page.fill('#api-key', apiKey);
  await page.getByRole('button', { name: 'Sign in with API key' }).click();
  await page.waitForURL(`${journey.origin}/t/${tenant}`);
}

/** Home data that must never reach a live page: the development fixtures. */
const homeFixtures = ['Raise checkout ranking cutoff', 'checkout-api', 'change_01', 'Recent changes', 'Pick work back up'];

test('live UI initial Home', async () => {
  const sso = journey.ssoTenant;
  const home = `${journey.origin}/t/${sso}`;
  await mkdir(journey.evidence, { recursive: true });
  const cards = await mkdtemp(join(tmpdir(), 'wyrd-home-cards-'));
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext();
    const page = await context.newPage();
    await browserLogin(page, sso, journey.users.admin);

    // Before registration Home shows exactly the tenant's authoritative Cards and no fixtures.
    const before = await registeredCards(journey.ssoAdminKey);
    const initial = await (await context.request.get(home)).text();
    expect(initial).toContain(`${before.length} registered`);
    for (const { name } of before.slice(-5)) expect(initial).toContain(name);
    for (const fixture of homeFixtures) expect(initial).not.toContain(fixture);
    // Operational sections without a live source say so instead of showing numbers.
    expect(initial).toContain('Not available yet');
    expect(initial).toContain('Signal summaries are not available on Home yet.');
    await page.goto(home);
    await page.screenshot({ path: join(journey.evidence, 'home-before.png'), fullPage: true });

    // Register two real Prompt Cards through the public CLI with the administrator's key.
    for (const name of ['home-triage', 'home-summarize'])
      await writeFile(
        join(cards, `${name}.yaml`),
        `apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: ${name}\n  version: 1.0.0\n  space: default\nspec:\n  provider: openai\n  model: gpt-4o\n  messages: [${name}]\n`
      );
    await writeFile(join(cards, 'credentials.toml'), `[default]\napi_key = "${journey.ssoAdminKey}"\n`, {
      mode: 0o600
    });
    // A directory is one Card tree with one root, so each independent Prompt is applied alone.
    for (const name of ['home-triage', 'home-summarize']) {
      const applied = await wyrd(cards, ['apply', '--server', journey.origin, join(cards, `${name}.yaml`)]);
      expect(applied.code, applied.output).toBe(0);
    }
    const after = await registeredCards(journey.ssoAdminKey);
    expect(after).toHaveLength(before.length + 2);

    // Returning Home by client navigation reads the change through a server load, not the browser.
    await page.goto(`${home}/settings`);
    const browserRequests: string[] = [];
    page.on('request', (request) => browserRequests.push(new URL(request.url()).pathname));
    await page.getByRole('navigation', { name: 'Primary' }).getByRole('link', { name: 'Home' }).click();
    await page.waitForURL(home);
    const triage = after.find(({ name }) => name === 'home-triage')!;
    const link = page.getByRole('link', { name: 'home-triage' });
    await expect.poll(() => link.getAttribute('href')).toBe(`/t/${sso}/cards/${triage.card_uid}`);
    const summaries = page.getByRole('complementary', { name: 'Workspace summaries and recent work' });
    await expect.poll(() => summaries.textContent()).toContain(`${after.length} registered`);
    expect(browserRequests).toContain(`/t/${sso}/__data.json`);
    expect(browserRequests.filter((path) => path.startsWith('/v1/'))).toEqual([]);
    await page.screenshot({ path: join(journey.evidence, 'home-after.png'), fullPage: true });

    // Refresh is server-rendered with the same content; hydration does not fetch it again.
    browserRequests.length = 0;
    const refreshed = await page.reload();
    expect(await refreshed!.text()).toContain('home-summarize');
    expect(browserRequests.filter((path) => path.endsWith('__data.json'))).toEqual([]);
    // A deep link and Back keep the tenant context.
    await page.goto(`${home}/settings`);
    await page.goBack();
    await page.waitForURL(home);
    await expect.poll(() => page.getByRole('link', { name: 'home-triage' }).count()).toBe(1);
    await link.click();
    await page.waitForURL(`${home}/cards/${triage.card_uid}`);
    await context.close();

    // Another tenant's reader sees only that tenant's Cards.
    const reader = await browser.newContext();
    const readerPage = await reader.newPage();
    await apiKeyLogin(readerPage, journey.apiKeyTenant, journey.offReaderKey);
    const offCards = await registeredCards(journey.offAdminKey);
    const readerHtml = await (await reader.request.get(`${journey.origin}/t/${journey.apiKeyTenant}`)).text();
    expect(readerHtml).toContain(`${offCards.length} registered`);
    for (const name of ['home-triage', 'home-summarize']) expect(readerHtml).not.toContain(name);
    await reader.close();

    // A principal the server refuses Card reads gets the denied state, never fixtures or Cards.
    const denied = await browser.newContext();
    const deniedPage = await denied.newPage();
    await apiKeyLogin(deniedPage, journey.apiKeyTenant, journey.offNoRoleKey);
    const refusal = await denied.request.get(`${journey.origin}/t/${journey.apiKeyTenant}`);
    expect(refusal.status()).toBe(403);
    const refusalHtml = await refusal.text();
    expect(refusalHtml).toContain('WYRD_PERMISSION_403');
    for (const { name } of offCards) expect(refusalHtml).not.toContain(name);
    for (const fixture of homeFixtures) expect(refusalHtml).not.toContain(fixture);
    await deniedPage.screenshot({ path: join(journey.evidence, 'home-denied.png'), fullPage: true });
  } finally {
    await browser.close();
    await rm(cards, { recursive: true, force: true });
  }
}, 120_000);

/** The CLI's typed lifecycle fixtures: real Model and Data specs with their artifacts. */
const typedState = fileURLToPath(
  new URL('../../../../../../wyrd-cli/tests/fixtures/card_lifecycle/typed_state/', import.meta.url)
);
type Listed = { name: string; card_uid: string; kind: string };

/** Every Card matching `query`, paged through the registry with the administrator's key. */
async function authoritative(apiKey: string, query: Record<string, string>): Promise<Listed[]> {
  const items: Listed[] = [];
  let cursor = '';
  do {
    const search = new URLSearchParams({ ...query, limit: '200', ...(cursor ? { cursor } : {}) });
    const listed = await asKey(apiKey, 'GET', `/cards?${search}`);
    expect(listed.status).toBe(200);
    const page = (await listed.json()) as { items: Listed[]; next_cursor?: string | null };
    items.push(...page.items);
    cursor = page.next_cursor ?? '';
  } while (cursor);
  return items;
}

/** Apply each `name → yaml` Card with the `wyrd` CLI as the holder of `apiKey`, from a directory holding the fixture artifacts. */
async function applyCards(apiKey: string, cards: Record<string, string>) {
  const dir = await mkdtemp(join(tmpdir(), 'wyrd-inventory-'));
  await cp(join(typedState, 'primary.bin'), join(dir, 'primary.bin'));
  await cp(join(typedState, 'data'), join(dir, 'data'), { recursive: true });
  await writeFile(join(dir, 'credentials.toml'), `[default]\napi_key = "${apiKey}"\n`, { mode: 0o600 });
  try {
    await Promise.all(
      Object.entries(cards).map(async ([name, yaml]) => {
        await writeFile(join(dir, `${name}.yaml`), yaml);
        const applied = await wyrd(dir, ['apply', '--server', journey.origin, join(dir, `${name}.yaml`)]);
        expect(applied.code, applied.output).toBe(0);
      })
    );
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
}

const prompt = (name: string) =>
  `apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: ${name}\n  version: 1.0.0\n  space: default\nspec:\n  provider: openai\n  model: gpt-4o\n  messages: [${name}]\n`;
const service = (name: string) =>
  `apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: ${name}\n  version: 1.0.0\n  space: default\nspec:\n  service_type: agent\n  components:\n    - alias: ranker\n      ref: { kind: Model, name: inv-model-a, version: 1.0.0, space: default }\n`;

/** The Card names of the inventory table's rows, in rendered order. */
async function shownRows(page: Page): Promise<string[]> {
  return page.locator('tbody td[data-l="name"]').allTextContents();
}

test('live UI Card inventory', async () => {
  const sso = journey.ssoTenant;
  const cardsUrl = `${journey.origin}/t/${sso}/cards`;
  await mkdir(journey.evidence, { recursive: true });
  const fixture = (file: string) => readFile(join(typedState, file), 'utf8');
  const [model, data, agent] = await Promise.all(['model-primary.yaml', 'training.yaml', 'agent-inline.yaml'].map(fixture));
  const named = (yaml: string, from: string, to: string, labels = '') =>
    yaml.replace(`  name: ${from}\n`, `  name: ${to}\n${labels}`);
  // Real Models, Data, an Agent and Prompts — more than one 20-row page — then two Services over them.
  await applyCards(journey.ssoAdminKey, {
    'inv-model-a': named(model, 'primary', 'inv-model-a', '  labels:\n    team: search\n'),
    'inv-model-b': named(model, 'primary', 'inv-model-b'),
    'inv-data-a': named(data, 'training', 'inv-data-a', '  labels:\n    owner: r.okafor\n'),
    'inv-data-b': named(data, 'training', 'inv-data-b'),
    'inv-agent': named(agent, 'inline', 'inv-agent'),
    ...Object.fromEntries(
      Array.from({ length: 14 }, (_, i) => `inv-prompt-${String(i + 1).padStart(2, '0')}`).map((name) => [name, prompt(name)])
    )
  });
  await applyCards(journey.ssoAdminKey, { 'inv-svc-a': service('inv-svc-a'), 'inv-svc-b': service('inv-svc-b') });
  // Another tenant registers an overlapping name.
  await applyCards(journey.offAdminKey, { 'inv-prompt-01': prompt('inv-prompt-01') });

  const scope = { filter: 'name =~ "(?i)inv-"' };
  const expected = await authoritative(journey.ssoAdminKey, scope);
  expect(expected).toHaveLength(21);
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext();
    const page = await context.newPage();
    await browserLogin(page, sso, journey.users.admin);

    // The first page is in the server-rendered HTML, before any hydration, with no fixtures.
    const initial = await (await context.request.get(`${cardsUrl}?q=inv-`)).text();
    for (const { card_uid } of expected.slice(0, 20)) expect(initial).toContain(`/t/${sso}/cards/${card_uid}`);
    expect(initial).not.toContain(expected[20].card_uid);
    expect(initial).toContain('20 on this page · oldest registered first');
    for (const fixture of ['checkout-api', 'pii-review', 'Recently viewed']) expect(initial).not.toContain(fixture);

    // Sequential pages cover the settled population in registry order, without repeats.
    const browserRequests: string[] = [];
    page.on('request', (request) => browserRequests.push(new URL(request.url()).pathname));
    await page.goto(`${cardsUrl}?q=inv-`);
    const first = await shownRows(page);
    browserRequests.length = 0;
    await page.getByRole('link', { name: 'Next page →' }).click();
    await page.waitForURL((url) => url.searchParams.has('cursor'));
    await expect.poll(() => shownRows(page)).toHaveLength(1);
    const second = await shownRows(page);
    expect([...first, ...second]).toEqual(expected.map(({ name }) => name));
    expect(browserRequests).toContain(`/t/${sso}/cards/__data.json`);
    expect(browserRequests.filter((path) => path.startsWith('/v1/'))).toEqual([]);
    await expect.poll(() => page.getByRole('link', { name: 'Next page →' }).count()).toBe(0);
    await page.screenshot({ path: join(journey.evidence, 'cards-page-2.png'), fullPage: true });
    // Refresh restores the same page server-side; hydration does not fetch it again.
    browserRequests.length = 0;
    const refreshed = await page.reload();
    expect(await refreshed!.text()).toContain(expected[20].card_uid);
    expect(browserRequests.filter((path) => path.endsWith('__data.json'))).toEqual([]);
    // Back returns to the first page.
    await page.goBack();
    await page.waitForURL(`${cardsUrl}?q=inv-`);
    await expect.poll(() => shownRows(page)).toEqual(first);

    // A scope change from a later page resets paging and matches the authoritative read.
    await page.getByRole('link', { name: 'Next page →' }).click();
    await page.waitForURL((url) => url.searchParams.has('cursor'));
    await page.getByRole('navigation', { name: 'Filter by kind' }).getByRole('link', { name: 'Service' }).click();
    await page.waitForURL(`${cardsUrl}?q=inv-&kind=Service`);
    const services = await authoritative(journey.ssoAdminKey, { ...scope, kind: 'Service' });
    expect(services.map(({ name }) => name).sort()).toEqual(['inv-svc-a', 'inv-svc-b']);
    await expect.poll(() => shownRows(page)).toEqual(services.map(({ name }) => name));
    await page.screenshot({ path: join(journey.evidence, 'cards-services.png'), fullPage: true });
    // Clearing the kind chip keeps the search.
    await page.getByRole('group', { name: 'Active filters' }).getByRole('link').first().click();
    await page.waitForURL(`${cardsUrl}?q=inv-`);

    // Metadata filters: declared labels, the owner label, lifecycle status, and their negatives.
    const html = async (query: string) => (await context.request.get(`${cardsUrl}?${query}`)).text();
    // Membership only: registry order is proven by paging above.
    const rowsOf = (markup: string) =>
      expected.filter(({ card_uid }) => markup.includes(`/t/${sso}/cards/${card_uid}`)).map(({ name }) => name);
    expect(rowsOf(await html('q=inv-&label=team%3Dsearch'))).toEqual(['inv-model-a']);
    expect(rowsOf(await html('label=team'))).toEqual(['inv-model-a']);
    expect(rowsOf(await html('owner=r.okafor'))).toEqual(['inv-data-a']);
    expect(rowsOf(await html('q=inv-&kind=Data')).sort()).toEqual(['inv-data-a', 'inv-data-b']);
    expect(rowsOf(await html('q=INV-MODEL')).sort()).toEqual(['inv-model-a', 'inv-model-b']);
    const deprecated = await html('q=inv-&status=deprecated');
    expect(deprecated).toContain('No matching cards');
    expect(rowsOf(deprecated)).toEqual([]);
    expect(await html('q=zzz-no-such-card')).toContain('No matching cards');

    // An exact UID finds its Card, under the other filters too.
    const ranker = expected.find(({ name }) => name === 'inv-model-a')!;
    expect(rowsOf(await html(`q=${ranker.card_uid}`))).toEqual(['inv-model-a']);
    expect(rowsOf(await html(`q=${ranker.card_uid}&kind=Model`))).toEqual(['inv-model-a']);
    expect(rowsOf(await html(`q=${ranker.card_uid}&status=deprecated`))).toEqual([]);
    expect(rowsOf(await html('q=00000000-0000-7000-8000-000000000000'))).toEqual([]);

    // The overlapping name resolves to this tenant's Card only.
    const [ours] = await authoritative(journey.ssoAdminKey, { name: 'inv-prompt-01' });
    const [theirs] = await authoritative(journey.offAdminKey, { name: 'inv-prompt-01' });
    const overlap = await html('q=inv-prompt-01');
    expect(overlap).toContain(ours.card_uid);
    expect(overlap).not.toContain(theirs.card_uid);
    await context.close();

    // The other tenant's reader sees its own Card under the same search, and nothing of this tenant.
    const reader = await browser.newContext();
    const readerPage = await reader.newPage();
    await apiKeyLogin(readerPage, journey.apiKeyTenant, journey.offReaderKey);
    await readerPage.goto(`${journey.origin}/t/${journey.apiKeyTenant}/cards?q=inv-`);
    expect(await shownRows(readerPage)).toEqual(['inv-prompt-01']);
    expect(await readerPage.content()).toContain(theirs.card_uid);
    await reader.close();

    // A principal the server refuses Card reads gets the denied state, never Cards or fixtures.
    const denied = await browser.newContext();
    const deniedPage = await denied.newPage();
    await apiKeyLogin(deniedPage, journey.apiKeyTenant, journey.offNoRoleKey);
    await deniedPage.goto(`${journey.origin}/t/${journey.apiKeyTenant}/cards`);
    const refusal = await deniedPage.content();
    expect(refusal).toContain('WYRD_PERMISSION_403');
    expect(refusal).not.toContain('inv-prompt-01');
    expect(refusal).not.toContain('checkout-api');
    await deniedPage.screenshot({ path: join(journey.evidence, 'cards-denied.png'), fullPage: true });
  } finally {
    await browser.close();
  }
}, 180_000);

/** The UID of the exact Card `kind default/name@version`, read with the administrator's key. */
async function uidOf(kind: string, name: string, version = '1.0.0'): Promise<string> {
  const query = new URLSearchParams({ kind, space: 'default', name, version });
  const read = await asKey(journey.ssoAdminKey, 'GET', `/cards/by-ref?${query}`);
  expect(read.status).toBe(200);
  return ((await read.json()) as { card: { metadata: { uid: string } } }).card.metadata.uid;
}

/** Development Card fixtures that must never reach a live detail page. */
const cardFixtures = ['checkout-api', 'pii-review', 'payments-mcp', 'kind-owned sections'];

/**
 * The shared live detail acceptance for one Card line with two registered
 * versions: the prior version's UID pins it in the server-rendered HTML,
 * inventory opens the current one, version selection and relationship links
 * are server loads, and unknown, cross-tenant and denied reads stay safe.
 */
async function detailJourney(
  browser: Chromium,
  line: {
    kind: string;
    name: string;
    /** Text only the prior (1.0.0) and only the current (2.0.0) declaration render. */
    prior: string;
    current: string;
    /** A Card related to the prior version: its relationship label, UID and name. */
    related: { label: string; uid: string; name: string };
  },
  shot: string
): Promise<{ context: BrowserContext; page: Page; priorUid: string; currentUid: string }> {
  const sso = journey.ssoTenant;
  const base = `${journey.origin}/t/${sso}`;
  const [priorUid, currentUid] = await Promise.all([uidOf(line.kind, line.name), uidOf(line.kind, line.name, '2.0.0')]);
  const context = await browser.newContext();
  const page = await context.newPage();
  await browserLogin(page, sso, journey.users.admin);

  // A UID pins its own version; the declaration, metadata and relationships are in the initial HTML.
  const initial = await (await context.request.get(`${base}/cards/${priorUid}`)).text();
  expect(initial).toContain(`<h1>${line.name}</h1>`);
  expect(initial).toContain(line.prior);
  expect(initial).not.toContain(line.current);
  expect(initial).toContain('read-only — 2.0.0 is current.');
  expect(initial).toContain(`href="/t/${sso}/cards/${line.related.uid}"`);
  for (const fixture of cardFixtures) expect(initial).not.toContain(fixture);

  // Inventory opens the current version.
  await page.goto(`${base}/cards?q=${line.name}&kind=${line.kind}`);
  await page.locator(`a[href="/t/${sso}/cards/${currentUid}"]`).click();
  await page.waitForURL(`${base}/cards/${currentUid}`);
  await expect.poll(() => page.locator('main').textContent()).toContain(line.current);
  await page.screenshot({ path: join(journey.evidence, `${shot}-current.png`), fullPage: true });

  // Selecting the prior version is a server load; the browser never calls the API.
  const browserRequests: string[] = [];
  page.on('request', (request) => browserRequests.push(new URL(request.url()).pathname));
  await page.locator('tbody').getByRole('link', { name: '1.0.0', exact: true }).click();
  await page.waitForURL(`${base}/cards/${currentUid}?version=1.0.0`);
  await expect.poll(() => page.locator('main').textContent()).toContain(line.prior);
  expect(await page.locator('main').textContent()).not.toContain(line.current);
  expect(browserRequests).toContain(`/t/${sso}/cards/${currentUid}/__data.json`);
  expect(browserRequests.filter((path) => path.startsWith('/v1/'))).toEqual([]);
  await page.screenshot({ path: join(journey.evidence, `${shot}-prior.png`), fullPage: true });
  // Refresh renders the selected version server-side; hydration does not fetch it again.
  browserRequests.length = 0;
  const refreshed = await page.reload();
  expect(await refreshed!.text()).toContain(line.prior);
  expect(browserRequests.filter((path) => path.endsWith('__data.json'))).toEqual([]);
  // Back returns to the current version.
  await page.goBack();
  await page.waitForURL(`${base}/cards/${currentUid}`);
  await expect.poll(() => page.locator('main').textContent()).toContain(line.current);

  // A relationship link from the pinned version selects the related Card's identity, not this one.
  await page.goto(`${base}/cards/${priorUid}`);
  await page.getByRole('link', { name: line.related.label }).click();
  await page.waitForURL(`${base}/cards/${line.related.uid}`);
  await expect.poll(() => page.locator('h1').textContent()).toBe(line.related.name);

  // Unknown versions and UIDs are structured 404s.
  for (const missing of [`${priorUid}?version=9.9.9`, '00000000-0000-7000-8000-000000000000', 'not-a-uid']) {
    const refused = await context.request.get(`${base}/cards/${missing}`);
    expect(refused.status(), missing).toBe(404);
    expect(await refused.text()).not.toContain(line.prior);
  }

  // Another tenant's reader cannot see the Card; a principal refused Card reads is denied.
  const other = `${journey.origin}/t/${journey.apiKeyTenant}/cards/${priorUid}`;
  const reader = await browser.newContext();
  await apiKeyLogin(await reader.newPage(), journey.apiKeyTenant, journey.offReaderKey);
  const hidden = await reader.request.get(other);
  expect(hidden.status()).toBe(404);
  expect(await hidden.text()).not.toContain(line.prior);
  await reader.close();
  const denied = await browser.newContext();
  await apiKeyLogin(await denied.newPage(), journey.apiKeyTenant, journey.offNoRoleKey);
  const refusal = await denied.request.get(other);
  expect(refusal.status()).toBe(403);
  expect(await refusal.text()).toContain('WYRD_PERMISSION_403');
  await denied.close();
  return { context, page, priorUid, currentUid };
}

/** `yaml` renamed to `name` at `version`, with `from → to` text replacements. */
const variant = (yaml: string, name: string, version: string, ...edits: [string, string][]) =>
  edits.reduce(
    (text, [from, to]) => text.replace(from, to),
    yaml.replace(/ {2}name: \S+\n {2}version: \S+\n/, `  name: ${name}\n  version: ${version}\n`)
  );

test('live UI Model detail', async () => {
  await mkdir(journey.evidence, { recursive: true });
  const model = await readFile(join(typedState, 'model-primary.yaml'), 'utf8');
  await applyCards(journey.ssoAdminKey, {
    'det-model-1': variant(model, 'det-model', '1.0.0', ['  version: 1.0.0\n', '  version: 1.0.0\n  labels:\n    team: ranking\n'])
  });
  await applyCards(journey.ssoAdminKey, {
    'det-model-2': variant(model, 'det-model', '2.0.0', ['GradientBoostingClassifier', 'RandomForestClassifier'])
  });
  await applyCards(journey.ssoAdminKey, {
    'det-model-svc': service('det-model-svc').replace('inv-model-a', 'det-model')
  });
  const svc = await uidOf('Service', 'det-model-svc');
  const browser = await chromium.launch();
  try {
    const { context } = await detailJourney(
      browser,
      {
        kind: 'Model',
        name: 'det-model',
        prior: 'GradientBoostingClassifier',
        current: 'RandomForestClassifier',
        related: { label: 'Service default/det-model-svc@1.0.0', uid: svc, name: 'det-model-svc' }
      },
      'model'
    );
    // Server-managed metadata of the pinned version.
    const html = await (await context.request.get(`${journey.origin}/t/${journey.ssoTenant}/cards/${await uidOf('Model', 'det-model')}`)).text();
    for (const text of ['team=ranking', 'BinaryClassification', 'signature', 'inputs[0].name', 'feature'])
      expect(html).toContain(text);
    await context.close();
  } finally {
    await browser.close();
  }
}, 180_000);

test('live UI Data detail', async () => {
  await mkdir(journey.evidence, { recursive: true });
  const [data, drift] = await Promise.all(['training.yaml', 'model-drift.yaml'].map((file) => readFile(join(typedState, file), 'utf8')));
  const column = (name: string): [string, string] => ['      - name: label\n', `      - name: ${name}\n`];
  await applyCards(journey.ssoAdminKey, {
    'det-data-1': variant(data, 'det-data', '1.0.0', column('det-clicked'), ['  version: 1.0.0\n', '  version: 1.0.0\n  labels:\n    owner: r.okafor\n']),
    'det-data-2': variant(data, 'det-data', '2.0.0', column('det-converted'), ['Snappy', 'Zstd'])
  });
  await applyCards(journey.ssoAdminKey, {
    'det-data-drift': variant(drift, 'det-data-drift', '1.0.0', ['name: training', 'name: det-data'])
  });
  const verifier = await uidOf('Verifier', 'det-data-drift');
  const browser = await chromium.launch();
  try {
    const { context, priorUid } = await detailJourney(
      browser,
      {
        kind: 'Data',
        name: 'det-data',
        prior: 'det-clicked',
        current: 'det-converted',
        related: { label: 'Verifier default/det-data-drift@1.0.0', uid: verifier, name: 'det-data-drift' }
      },
      'data'
    );
    // The declared schema, interface and stats and the server-managed metadata of the pinned version.
    const html = await (await context.request.get(`${journey.origin}/t/${journey.ssoTenant}/cards/${priorUid}`)).text();
    for (const text of ['columns[0].name', 'feature', 'columns[1].dtype', 'int64', 'Parquet', 'Snappy', 'row_count', 'owner=r.okafor'])
      expect(html).toContain(text);
    expect(html).not.toContain('Zstd');
    await context.close();
  } finally {
    await browser.close();
  }
}, 180_000);

test('live UI Agent detail', async () => {
  await mkdir(journey.evidence, { recursive: true });
  const agent = (version: string, timeout: number) =>
    `apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  name: det-agent\n  version: ${version}\n  space: default\nspec:\n  prompt: { kind: Prompt, name: det-agent-prompt, version: 1.0.0, space: default }\n  run_config:\n    max_iterations: 2\n    timeout_ms: ${timeout}\n`;
  await applyCards(journey.ssoAdminKey, { 'det-agent-prompt': prompt('det-agent-prompt') });
  await applyCards(journey.ssoAdminKey, { 'det-agent-1': agent('1.0.0', 43210) });
  await applyCards(journey.ssoAdminKey, { 'det-agent-2': agent('2.0.0', 98765) });
  const promptUid = await uidOf('Prompt', 'det-agent-prompt');
  const browser = await chromium.launch();
  try {
    const { context, page, currentUid } = await detailJourney(
      browser,
      {
        kind: 'Agent',
        name: 'det-agent',
        prior: '43210',
        current: '98765',
        related: { label: 'Prompt default/det-agent-prompt@1.0.0', uid: promptUid, name: 'det-agent-prompt' }
      },
      'agent'
    );
    // The current version declares the same exact Prompt; the Prompt shows both Agent versions inbound.
    const html = await (await context.request.get(`${journey.origin}/t/${journey.ssoTenant}/cards/${currentUid}`)).text();
    expect(html).toContain(`href="/t/${journey.ssoTenant}/cards/${promptUid}"`);
    expect(html).toContain('max_iterations');
    await expect.poll(() => page.locator('main').textContent()).toContain('Agent default/det-agent@2.0.0');
    expect(await page.locator('main').textContent()).toContain('Agent default/det-agent@1.0.0');
    // A declaration carries no execution evidence or actions.
    for (const invented of ['Run agent', 'last run', 'Open traces']) expect(html).not.toContain(invented);
    await context.close();
  } finally {
    await browser.close();
  }
}, 180_000);

test('live UI Prompt detail', async () => {
  await mkdir(journey.evidence, { recursive: true });
  const message = (word: string) => `Summarize det-prompt-${word} <script>window.detPwned=1</script>`;
  const declared = (version: string, word: string) =>
    `apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: det-prompt\n  version: ${version}\n  space: default\nspec:\n  provider: openai\n  model: gpt-4o\n  messages: ${JSON.stringify([message(word)])}\n`;
  await applyCards(journey.ssoAdminKey, { 'det-prompt-1': declared('1.0.0', 'one') });
  await applyCards(journey.ssoAdminKey, { 'det-prompt-2': declared('2.0.0', 'two') });
  await applyCards(journey.ssoAdminKey, {
    'det-prompt-agent': `apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  name: det-prompt-agent\n  version: 1.0.0\n  space: default\nspec:\n  prompt: { kind: Prompt, name: det-prompt, version: 1.0.0, space: default }\n  run_config:\n    max_iterations: 1\n    timeout_ms: 1000\n`
  });
  const agentUid = await uidOf('Agent', 'det-prompt-agent');
  const browser = await chromium.launch();
  try {
    const { context, page, priorUid } = await detailJourney(
      browser,
      {
        kind: 'Prompt',
        name: 'det-prompt',
        prior: 'det-prompt-one',
        current: 'det-prompt-two',
        related: { label: 'Agent default/det-prompt-agent@1.0.0', uid: agentUid, name: 'det-prompt-agent' }
      },
      'prompt'
    );
    const url = `${journey.origin}/t/${journey.ssoTenant}/cards/${priorUid}`;
    // Declared markup is text: escaped in the HTML, shown verbatim, never executed.
    const html = await (await context.request.get(url)).text();
    expect(html).not.toContain('<script>window.detPwned');
    // The page shows the server's stored declaration, which normalizes the authored shorthand.
    const stored = await asKey(journey.ssoAdminKey, 'GET', `/cards/by-ref?${new URLSearchParams({ kind: 'Prompt', space: 'default', name: 'det-prompt', version: '1.0.0' })}`);
    const { spec } = ((await stored.json()) as { card: { spec: { model: string; request: { provider: string } } } }).card;
    for (const text of ['request', 'provider', spec.request.provider, spec.model, 'body.messages[0].content']) expect(html).toContain(text);
    await page.goto(url);
    expect(await page.locator('main').textContent()).toContain(message('one'));
    expect(await page.evaluate(() => (window as { detPwned?: number }).detPwned)).toBeUndefined();
    await page.screenshot({ path: join(journey.evidence, 'prompt-markup.png'), fullPage: true });
    // Neither the page, its data channel nor browser storage carries the session or a token.
    const [session] = (await context.cookies()).filter(({ name }) => name.startsWith('wyrd_session_'));
    expect(session).toBeDefined();
    const storage = await page.evaluate(() => JSON.stringify({ ...localStorage }) + JSON.stringify({ ...sessionStorage }));
    for (const body of [html, await (await context.request.get(`${url}/__data.json`)).text(), storage]) {
      expect(body).not.toContain(session.value);
      expect(body).not.toMatch(jwt);
    }
    // A declaration carries no runtime outcome.
    for (const invented of ['Run prompt', 'last run', 'latency', 'tokens used']) expect(html).not.toContain(invented);
    await context.close();
  } finally {
    await browser.close();
  }
}, 180_000);

test('live UI Service workspace', async () => {
  await mkdir(journey.evidence, { recursive: true });
  const admin = WyrdClient.connect({ serverUrl: journey.servers[0], credential: journey.ssoAdminKey, grpcUrl: journey.grpcs[0] });
  const [passing, failing] = await serviceWorkload(admin);
  const sso = journey.ssoTenant;
  const base = `${journey.origin}/t/${sso}`;
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext();
    const page = await context.newPage();
    await browserLogin(page, sso, journey.users.admin);
    const html = async (uid: string, query = '') => {
      const response = await context.request.get(`${base}/cards/${uid}${query}`);
      expect(response.status()).toBe(200);
      return response.text();
    };

    // Initial HTML carries each Service's own retained evidence and nothing of the other's.
    const pass1h = await html(passing.uid, '?range=1h');
    for (const text of ['✓ VERIFICATION PASSING', '2/2 ready', `${passing.recentSpans} root spans · 1h`, 'WL-ANSWER 1.0.0 → agent', 'REQUESTS', 'LATENCY'])
      expect(pass1h).toContain(text);
    for (const text of ['▲ VERIFICATION FAILING', failing.name, failing.agentUid, ...cardFixtures]) expect(pass1h).not.toContain(text);
    const fail1h = await html(failing.uid, '?range=1h');
    for (const text of ['▲ VERIFICATION FAILING', `${failing.recentSpans} root spans · 1h`, 'wl-answer → agent failed'])
      expect(fail1h).toContain(text);
    for (const text of ['✓ VERIFICATION PASSING', passing.name, passing.agentUid]) expect(fail1h).not.toContain(text);
    // The older root span is outside 1h and inside 6h.
    expect(await html(passing.uid, '?range=6h')).toContain(`${passing.recentSpans + 1} root spans · 6h`);

    // Composition names the exact versioned components and measurement; definition the declaration.
    const composition = await html(passing.uid, '?view=composition');
    for (const text of ['RUNTIME COMPOSITION', 'MEASUREMENT', passing.agentUid, passing.evalUid, passing.driftUid, 'verified by'])
      expect(composition).toContain(text);
    expect(composition).not.toContain(failing.agentUid);
    expect(await html(passing.uid, '?view=definition')).toContain('wl-score-drift@1.0.0');

    // Changing the range is a server load; the browser never calls the API, and refresh does not refetch.
    await page.goto(`${base}/cards/${passing.uid}?range=1h`);
    await page.screenshot({ path: join(journey.evidence, 'service-1h.png'), fullPage: true });
    const browserRequests: string[] = [];
    page.on('request', (request) => browserRequests.push(new URL(request.url()).pathname));
    await page.getByRole('link', { name: '6h', exact: true }).click();
    await page.waitForURL(/range=6h/);
    await expect.poll(() => page.locator('main').textContent()).toContain(`${passing.recentSpans + 1} root spans · 6h`);
    expect(browserRequests).toContain(`/t/${sso}/cards/${passing.uid}/__data.json`);
    expect(browserRequests.filter((path) => path.startsWith('/v1/'))).toEqual([]);
    await page.screenshot({ path: join(journey.evidence, 'service-6h.png'), fullPage: true });
    browserRequests.length = 0;
    expect(await (await page.reload())!.text()).toContain(`${passing.recentSpans + 1} root spans · 6h`);
    expect(browserRequests.filter((path) => path.endsWith('__data.json'))).toEqual([]);
    await page.goto(`${base}/cards/${failing.uid}?range=1h`);
    await page.screenshot({ path: join(journey.evidence, 'service-failing.png'), fullPage: true });
    await page.goto(`${base}/cards/${passing.uid}?view=composition`);
    await page.screenshot({ path: join(journey.evidence, 'service-composition.png'), fullPage: true });
    await context.close();

    // Another tenant's reader cannot see the Service; a principal refused Card reads is denied.
    const other = `${journey.origin}/t/${journey.apiKeyTenant}/cards/${passing.uid}`;
    const reader = await browser.newContext();
    await apiKeyLogin(await reader.newPage(), journey.apiKeyTenant, journey.offReaderKey);
    const hidden = await reader.request.get(other);
    expect(hidden.status()).toBe(404);
    expect(await hidden.text()).not.toContain('root spans');
    await reader.close();
    const denied = await browser.newContext();
    await apiKeyLogin(await denied.newPage(), journey.apiKeyTenant, journey.offNoRoleKey);
    const refusal = await denied.request.get(other);
    expect(refusal.status()).toBe(403);
    expect(await refusal.text()).toContain('WYRD_PERMISSION_403');
    await denied.close();
  } finally {
    await browser.close();
  }
}, 420_000);
