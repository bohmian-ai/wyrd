import { createHash } from 'node:crypto';
import type { Cookies } from '@sveltejs/kit';
import * as client from 'openid-client';
import { decodeJwt } from 'jose';
import type { SessionMetadata } from '$lib/views';
import {
  grpcUrl,
  serverUrl,
  v1Path,
  type V1,
  type V1Args,
  type V1Method,
  type V1Response
} from '../upstream';
import { reject, type TenantContext } from './session';

const sessionPrefix = 'wyrd_session_';
const loginCookie = 'wyrd_login';
const loginLifetimeSeconds = 5 * 60;
/**
 * Application-session bound for both credential kinds. The refresh token is
 * opaque to this client (RFC 6749 §1.5); the server's refusal ends it sooner.
 */
const sessionLifetimeSeconds = 12 * 60 * 60;
/** Renew a cached access token this long before it expires. */
const renewalMarginMs = 5_000;
const tenantKeyPattern = /^[a-z0-9][a-z0-9_-]{0,62}$/;
const apiKeyTokenType = 'urn:wyrd:oauth:token-type:api_key';

/** What the session cookie holds: the tenant and the credential that renews it. */
type Sealed = { tenant: string; kind: 'refresh' | 'api_key'; credential: string };
type Login = { tenant: string; verifier: string; state: string };
/** Server-issued access token claims the BFF projects; authority stays with the server. */
type Claims = {
  exp: number;
  principal: { id: string; tenant_id: string };
  permissions: { resource: string; action: string | { any_of: string[] } }[];
};
type Access = { token: string; expiresAt: number; claims: Claims };

/** HttpOnly, SameSite=Lax cookie options, `Secure` exactly when the browser reached the BFF over `https:`. */
const cookieOptions = (url: URL, maxAge: number) =>
  ({
    path: '/',
    httpOnly: true,
    secure: url.protocol === 'https:',
    sameSite: 'lax',
    maxAge
  }) as const;

/** The JSON cookie `value`, or `null` when it is absent or not JSON. */
function parse<T>(value: string | undefined): T | null {
  if (!value) return null;
  try {
    return JSON.parse(value) as T;
  } catch {
    return null;
  }
}

/** `resource:action` names of the token's permissions, the shape the UI gates on. */
function permissionNames(claims: Claims): string[] {
  return claims.permissions.flatMap(({ resource, action }) =>
    (typeof action === 'string' ? [action] : action.any_of).map((a) => `${resource}:${a}`)
  );
}

/**
 * One signed-in tenant session of this request. The access token stays in a
 * private field: the object cannot be serialized into page data.
 */
export class BrowserSession {
  readonly #token: string;
  readonly #url: URL;

  /** `url` is the browser's request URL; `/v1` calls forward its host and scheme. */
  constructor(
    readonly tenantKey: string,
    access: Access,
    url: URL
  ) {
    this.#token = access.token;
    this.#url = url;
    this.principalId = access.claims.principal.id;
    this.tenantId = access.claims.principal.tenant_id;
    this.permissions = permissionNames(access.claims);
    this.expiresAt = access.expiresAt;
  }

  readonly principalId: string;
  readonly tenantId: string;
  readonly permissions: string[];
  readonly expiresAt: number;

  /** Tenant context bound from the server-issued token; the UI maps no roles. */
  context(): TenantContext {
    return {
      tenant: {
        key: this.tenantKey,
        name: this.tenantKey,
        tenantId: this.tenantId,
        permissions: this.permissions
      },
      subject: { id: this.principalId, name: this.principalId },
      permissions: this.permissions
    };
  }

  /**
   * Call a generated Wyrd `/v1` operation as this session's principal,
   * forwarding the browser's host and scheme so the server derives the
   * public origin (an identity connection's callback) from them.
   */
  async api<P extends keyof V1, M extends V1Method<P>>(
    method: M,
    path: P,
    ...[request]: V1Args<P, M>
  ): Promise<V1Response<P, M>> {
    const { body, path: params, query }: { body?: unknown; path?: unknown; query?: unknown } =
      request ?? {};
    try {
      return await fetch(new URL(`/v1${v1Path(path, params, query)}`, serverUrl()), {
        method: method.toUpperCase(),
        headers: {
          'x-wyrd-access-token': `Bearer ${this.#token}`,
          'x-forwarded-host': this.#url.host,
          'x-forwarded-proto': this.#url.protocol.slice(0, -1),
          ...(body === undefined ? {} : { 'content-type': 'application/json' })
        },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: AbortSignal.timeout(30_000)
      });
    } catch {
      reject('upstream');
    }
  }

  /**
   * Run one bound SELECT through Bifrost as this session's principal. The SDK
   * owns transport, terminal validation and Arrow decoding; its `WyrdError`
   * (refusal, transport or failed terminal) propagates for the caller to
   * project. The SDK loads lazily so mock-only runtimes never need its addon.
   */
  async query(sql: string, params: readonly (string | number)[]): Promise<Analytical> {
    const { Bifrost, WyrdClient } = await import('@wyrd/sdk');
    const client = WyrdClient.connect({
      serverUrl: serverUrl(),
      credential: this.#token,
      grpcUrl: grpcUrl()
    });
    const bifrost = await Bifrost.connect({ client });
    try {
      const result = await bifrost.sql(sql, params);
      return {
        outcome: result.terminal.outcome === 'degraded' ? 'degraded' : 'success',
        fields: result.schema.fields.map((field) => ({ name: field.name, type: String(field.type) })),
        rows: result.toArrow().toArray().map((row) => row.toJSON())
      };
    } finally {
      await bifrost.shutdown();
    }
  }
}

/** One completed analytical read: terminal outcome, result schema and plain rows. */
export type Analytical = {
  outcome: 'success' | 'degraded';
  fields: { name: string; type: string }[];
  rows: Record<string, unknown>[];
};

/**
 * The BFF as the public OAuth client `wyrd-ui` of Wyrd's authorization
 * server, built on `openid-client`: authorization code + PKCE sign-in,
 * refresh, RFC 8693 API-key exchange (OIDC-off), and RFC 7009 revocation.
 *
 * It needs no secret and no discovery: the browser is sent to
 * `/auth/authorize` on the origin it reached the BFF at, and the token and
 * revocation endpoints are called on `WYRD_SERVER_URL`. Each tenant's session
 * is one HttpOnly cookie holding the refresh token (or API key) the server
 * validates on every renewal, so every replica reads it with no server-side
 * store. Access tokens are cached in memory by credential hash; a miss renews
 * through the token endpoint.
 */
export class BrowserSessions {
  readonly #access = new Map<string, Access>();

  /** The `wyrd-ui` client of the authorization server the browser reached at `origin`. */
  private configuration(origin: string): client.Configuration {
    const server = serverUrl();
    const configuration = new client.Configuration(
      {
        issuer: origin,
        authorization_endpoint: new URL('/auth/authorize', origin).toString(),
        token_endpoint: new URL('/auth/token', server).toString(),
        revocation_endpoint: new URL('/auth/revoke', server).toString()
      },
      'wyrd-ui',
      undefined,
      client.None()
    );
    // The authorization endpoint is the browser's own origin, `http:` when the
    // deployment is served that way; serverUrl() admits plaintext only for loopback.
    client.allowInsecureRequests(configuration);
    return configuration;
  }

  /** Cookie name for one tenant's session; refuses keys that are not tenant slugs. */
  cookieName(tenantKey: string): string {
    if (!tenantKeyPattern.test(tenantKey)) reject('notFound');
    return sessionPrefix + tenantKey;
  }

  /** Begin sign-in: a PKCE + state login cookie and the authorization request URL. */
  async begin(tenantKey: string, url: URL, cookies: Cookies): Promise<string> {
    this.cookieName(tenantKey);
    const verifier = client.randomPKCECodeVerifier();
    const state = client.randomState();
    const login: Login = { tenant: tenantKey, verifier, state };
    cookies.set(loginCookie, JSON.stringify(login), cookieOptions(url, loginLifetimeSeconds));
    return client
      .buildAuthorizationUrl(this.configuration(url.origin), {
        redirect_uri: new URL('/login/callback', url.origin).toString(),
        code_challenge: await client.calculatePKCECodeChallenge(verifier),
        code_challenge_method: 'S256',
        state,
        tenant: tenantKey
      })
      .toString();
  }

  /**
   * Redeem the authorization response at `url` with this browser's login
   * cookie and set the tenant session cookie. Always clears the login cookie;
   * any failure sets no session and returns to that tenant's sign-in.
   */
  async complete(url: URL, cookies: Cookies): Promise<string> {
    const login = parse<Login>(cookies.get(loginCookie));
    cookies.delete(loginCookie, { path: '/' });
    if (!login) return '/?login=failed';
    const failed = `/t/${encodeURIComponent(login.tenant)}/login?login=failed`;
    try {
      const tokens = await client.authorizationCodeGrant(
        this.configuration(url.origin),
        url,
        { pkceCodeVerifier: login.verifier, expectedState: login.state }
      );
      if (!tokens.refresh_token) return failed;
      await this.establish(login.tenant, 'refresh', tokens.refresh_token, tokens, url, cookies);
    } catch {
      return failed;
    }
    return `/t/${encodeURIComponent(login.tenant)}`;
  }

  /** OIDC-off sign-in: exchange an operator API key (RFC 8693) and keep it in the session cookie. */
  async signInWithApiKey(
    tenantKey: string,
    apiKey: string,
    url: URL,
    cookies: Cookies
  ): Promise<string> {
    this.cookieName(tenantKey);
    const tokens = await this.exchange(apiKey, url.origin);
    if (!tokens) reject('unauthenticated');
    await this.establish(tenantKey, 'api_key', apiKey, tokens, url, cookies);
    return `/t/${encodeURIComponent(tenantKey)}`;
  }

  private async establish(
    tenant: string,
    kind: Sealed['kind'],
    credential: string,
    tokens: client.TokenEndpointResponse,
    url: URL,
    cookies: Cookies
  ): Promise<void> {
    this.cache(credential, tokens.access_token);
    const sealed: Sealed = { tenant, kind, credential };
    cookies.set(
      this.cookieName(tenant),
      JSON.stringify(sealed),
      cookieOptions(url, sessionLifetimeSeconds)
    );
  }

  private hash(credential: string): string {
    return createHash('sha256').update(credential, 'utf8').digest('hex');
  }

  private cache(credential: string, token: string): Access {
    const claims = decodeJwt(token) as unknown as Claims;
    const access = { token, expiresAt: claims.exp * 1000, claims };
    const now = Date.now();
    for (const [key, value] of this.#access) if (value.expiresAt <= now) this.#access.delete(key);
    // ponytail: bounded per-replica cache; a miss only costs one renewal.
    if (this.#access.size >= 10_000) this.#access.delete(this.#access.keys().next().value!);
    this.#access.set(this.hash(credential), access);
    return access;
  }

  /** Exchange an API key for an access token; `null` when the server refuses it. */
  private async exchange(apiKey: string, origin: string) {
    try {
      return await client.genericGrantRequest(
        this.configuration(origin),
        'urn:ietf:params:oauth:grant-type:token-exchange',
        { subject_token: apiKey, subject_token_type: apiKeyTokenType }
      );
    } catch (cause) {
      if (cause instanceof client.ResponseBodyError) return null;
      reject('upstream');
    }
  }

  /** A live access token for `sealed`: cached, or renewed; `null` when the server refuses renewal. */
  private async access(sealed: Sealed, origin: string): Promise<Access | null> {
    const cached = this.#access.get(this.hash(sealed.credential));
    if (cached && cached.expiresAt - renewalMarginMs > Date.now()) return cached;
    if (sealed.kind === 'api_key') {
      const tokens = await this.exchange(sealed.credential, origin);
      return tokens && this.cache(sealed.credential, tokens.access_token);
    }
    try {
      const tokens = await client.refreshTokenGrant(this.configuration(origin), sealed.credential);
      return this.cache(sealed.credential, tokens.access_token);
    } catch (cause) {
      if (cause instanceof client.ResponseBodyError) return null;
      reject('upstream');
    }
  }

  private sealed(tenantKey: string, cookies: Cookies): Sealed | null {
    const sealed = parse<Sealed>(cookies.get(this.cookieName(tenantKey)));
    return sealed?.tenant === tenantKey ? sealed : null;
  }

  /**
   * This tenant's session, renewed through the server when its cached access
   * token is missing or expiring. Returns `null` (and clears the cookie) when
   * there is none, it does not parse, it belongs to another tenant, or the
   * server refuses to renew it.
   */
  async read(tenantKey: string, url: URL, cookies: Cookies): Promise<BrowserSession | null> {
    const name = this.cookieName(tenantKey);
    if (!cookies.get(name)) return null;
    const sealed = this.sealed(tenantKey, cookies);
    const access = sealed && (await this.access(sealed, url.origin));
    if (!access) {
      cookies.delete(name, { path: '/' });
      return null;
    }
    return new BrowserSession(tenantKey, access, url);
  }

  /**
   * Sign out of this tenant: always clear its cookie and cached access token,
   * then revoke its refresh token (RFC 7009) best-effort. A revocation failure
   * is logged without token values and does not fail the logout.
   */
  async logout(tenantKey: string, url: URL, cookies: Cookies): Promise<void> {
    const sealed = this.sealed(tenantKey, cookies);
    cookies.delete(this.cookieName(tenantKey), { path: '/' });
    if (!sealed) return;
    this.#access.delete(this.hash(sealed.credential));
    // An operator API key is never revoked by signing out of the UI.
    if (sealed.kind !== 'refresh') return;
    try {
      await client.tokenRevocation(this.configuration(url.origin), sealed.credential, {
        token_type_hint: 'refresh_token'
      });
    } catch (cause) {
      console.warn('wyrd-ui logout: refresh token revocation failed', {
        tenant: tenantKey,
        error: cause instanceof Error ? cause.name : 'unknown'
      });
    }
  }

  /** Switch to `target`: its own session when the server still renews it, else its sign-in. */
  async switch(target: string, url: URL, cookies: Cookies): Promise<string> {
    const base = `/t/${encodeURIComponent(target)}`;
    return (await this.read(target, url, cookies)) ? base : `${base}/login`;
  }

  /**
   * Safe page metadata: the current tenant plus every other tenant whose
   * session cookie names that tenant. Cookies that do not parse or name
   * another tenant are cleared and never rendered.
   */
  async metadata(session: BrowserSession, cookies: Cookies): Promise<SessionMetadata> {
    const tenants = [session.tenantKey];
    for (const { name } of cookies.getAll()) {
      const key = name.slice(sessionPrefix.length);
      if (!name.startsWith(sessionPrefix) || key === session.tenantKey) continue;
      if (tenantKeyPattern.test(key) && this.sealed(key, cookies)) tenants.push(key);
      else cookies.delete(name, { path: '/' });
    }
    return {
      subject: { id: session.principalId, name: session.principalId },
      expiresAt: session.expiresAt,
      tenants: tenants.map((key) => ({ key, name: key }))
    };
  }
}

export const browserSessions = new BrowserSessions();
