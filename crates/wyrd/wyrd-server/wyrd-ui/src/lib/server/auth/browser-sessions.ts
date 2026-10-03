import { createHash, hkdfSync } from 'node:crypto';
import { env } from '$env/dynamic/private';
import type { Cookies } from '@sveltejs/kit';
import * as client from 'openid-client';
import { EncryptJWT, decodeJwt, jwtDecrypt } from 'jose';
import type { SessionMetadata } from '$lib/views';
import { loopback, serverUrl } from '../upstream';
import { reject, type TenantContext } from './session';

const sessionPrefix = 'wyrd_session_';
const loginCookie = 'wyrd_login';
const loginLifetimeSeconds = 5 * 60;
/** An API-key session has no refresh-token expiry of its own; it lasts one working day. */
const apiKeyLifetimeSeconds = 12 * 60 * 60;
/** Renew a cached access token this long before it expires. */
const renewalMarginMs = 5_000;
const tenantKeyPattern = /^[a-z0-9][a-z0-9_-]{0,62}$/;
const apiKeyTokenType = 'urn:wyrd:oauth:token-type:api_key';

/** What the encrypted session cookie holds: the tenant and the credential that renews it. */
type Sealed = { tenant: string; kind: 'refresh' | 'api_key'; credential: string };
type Login = { tenant: string; verifier: string; state: string };
/** Server-issued access token claims the BFF projects; authority stays with the server. */
type Claims = {
  exp: number;
  principal: { id: string; tenant_id: string };
  permissions: { resource: string; action: string | { any_of: string[] } }[];
};
type Access = { token: string; expiresAt: number; claims: Claims };

const cookieOptions = { path: '/', httpOnly: true, secure: true, sameSite: 'lax' } as const;

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

  constructor(
    readonly tenantKey: string,
    access: Access
  ) {
    this.#token = access.token;
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

  /** Call a Wyrd `/v1` API as this session's principal. */
  async api(method: string, path: string, body?: unknown): Promise<Response> {
    try {
      return await fetch(new URL(`/v1${path}`, serverUrl()), {
        method,
        headers: {
          'x-wyrd-access-token': `Bearer ${this.#token}`,
          ...(body === undefined ? {} : { 'content-type': 'application/json' })
        },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: AbortSignal.timeout(30_000)
      });
    } catch {
      reject('upstream');
    }
  }
}

/**
 * The BFF as the confidential OAuth client `wyrd-ui` of Wyrd's authorization
 * server, built on `openid-client`: authorization code + PKCE sign-in,
 * refresh, RFC 8693 API-key exchange (OIDC-off), and RFC 7009 revocation.
 *
 * Each tenant's session is one `jose`-encrypted HttpOnly cookie holding its
 * refresh token (or API key), so every replica reads it with no server-side
 * store. Access tokens are cached in memory by credential hash; a miss renews
 * through the token endpoint.
 */
export class BrowserSessions {
  readonly #configurations = new Map<string, Promise<client.Configuration>>();
  readonly #access = new Map<string, Access>();
  #key?: Uint8Array;

  /** The `wyrd-ui` client secret; the deployment registers its SHA-256 with the server. */
  private secret(): string {
    const secret = env.WYRD_UI_CLIENT_SECRET;
    if (!secret) reject('upstream');
    return secret;
  }

  /** Cookie encryption key, derived from the client secret with HKDF-SHA256. */
  private key(): Uint8Array {
    this.#key ??= new Uint8Array(
      hkdfSync('sha256', this.secret(), '', 'wyrd-ui session cookie', 32)
    );
    return this.#key;
  }

  /**
   * The client configuration for the authorization server at `origin`, the
   * public origin the BFF and Wyrd share. Discovery reads RFC 8414 metadata;
   * requests for that origin are sent to the internal `WYRD_SERVER_URL`.
   */
  private configuration(origin: string): Promise<client.Configuration> {
    let configuration = this.#configurations.get(origin);
    if (!configuration) {
      const issuer = new URL(origin);
      const internal: client.CustomFetch = (url, options) => {
        const target = new URL(url);
        const routed =
          target.origin === issuer.origin
            ? new URL(target.pathname + target.search, serverUrl())
            : target;
        return fetch(routed, options as RequestInit);
      };
      configuration = client.discovery(
        issuer,
        'wyrd-ui',
        undefined,
        client.ClientSecretBasic(this.secret()),
        {
          algorithm: 'oauth2',
          [client.customFetch]: internal,
          execute:
            issuer.protocol === 'http:' && loopback(issuer.hostname)
              ? [client.allowInsecureRequests]
              : []
        }
      );
      // Only a successful discovery is reused; a failure is retried on the next request.
      configuration.catch(() => this.#configurations.delete(origin));
      this.#configurations.set(origin, configuration);
    }
    return configuration.catch(() => reject('upstream'));
  }

  /** Cookie name for one tenant's session; refuses keys that are not tenant slugs. */
  cookieName(tenantKey: string): string {
    if (!tenantKeyPattern.test(tenantKey)) reject('notFound');
    return sessionPrefix + tenantKey;
  }

  private seal(value: object, lifetimeSeconds: number): Promise<string> {
    return new EncryptJWT({ ...value })
      .setProtectedHeader({ alg: 'dir', enc: 'A256GCM' })
      .setIssuedAt()
      .setExpirationTime(`${lifetimeSeconds}s`)
      .encrypt(this.key());
  }

  private async unseal<T>(value: string | undefined): Promise<T | null> {
    if (!value) return null;
    try {
      return (await jwtDecrypt(value, this.key())).payload as T;
    } catch {
      return null;
    }
  }

  /** Begin sign-in: a PKCE + state login cookie and the authorization request URL. */
  async begin(tenantKey: string, url: URL, cookies: Cookies): Promise<string> {
    this.cookieName(tenantKey);
    const configuration = await this.configuration(url.origin);
    const verifier = client.randomPKCECodeVerifier();
    const state = client.randomState();
    const login: Login = { tenant: tenantKey, verifier, state };
    cookies.set(loginCookie, await this.seal(login, loginLifetimeSeconds), {
      ...cookieOptions,
      maxAge: loginLifetimeSeconds
    });
    return client
      .buildAuthorizationUrl(configuration, {
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
    const login = await this.unseal<Login>(cookies.get(loginCookie));
    cookies.delete(loginCookie, { path: '/' });
    if (!login) return '/?login=failed';
    const failed = `/t/${encodeURIComponent(login.tenant)}/login?login=failed`;
    try {
      const tokens = await client.authorizationCodeGrant(
        await this.configuration(url.origin),
        url,
        { pkceCodeVerifier: login.verifier, expectedState: login.state }
      );
      if (!tokens.refresh_token) return failed;
      await this.establish(login.tenant, 'refresh', tokens.refresh_token, tokens, cookies);
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
    await this.establish(tenantKey, 'api_key', apiKey, tokens, cookies);
    return `/t/${encodeURIComponent(tenantKey)}`;
  }

  private async establish(
    tenant: string,
    kind: Sealed['kind'],
    credential: string,
    tokens: client.TokenEndpointResponse,
    cookies: Cookies
  ): Promise<void> {
    const lifetime =
      kind === 'refresh'
        ? (decodeJwt(credential).exp ?? 0) - Math.floor(Date.now() / 1000)
        : apiKeyLifetimeSeconds;
    if (!(lifetime > 0)) reject('upstream');
    this.cache(credential, tokens.access_token);
    const sealed: Sealed = { tenant, kind, credential };
    cookies.set(this.cookieName(tenant), await this.seal(sealed, lifetime), {
      ...cookieOptions,
      maxAge: lifetime
    });
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
        await this.configuration(origin),
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
      const tokens = await client.refreshTokenGrant(
        await this.configuration(origin),
        sealed.credential
      );
      return this.cache(sealed.credential, tokens.access_token);
    } catch (cause) {
      if (cause instanceof client.ResponseBodyError) return null;
      reject('upstream');
    }
  }

  private async sealed(tenantKey: string, cookies: Cookies): Promise<Sealed | null> {
    const sealed = await this.unseal<Sealed>(cookies.get(this.cookieName(tenantKey)));
    return sealed?.tenant === tenantKey ? sealed : null;
  }

  /**
   * This tenant's session, renewed through the server when its cached access
   * token is missing or expiring. Returns `null` (and clears the cookie) when
   * there is none, it does not decrypt, it belongs to another tenant, or the
   * server refuses to renew it.
   */
  async read(tenantKey: string, url: URL, cookies: Cookies): Promise<BrowserSession | null> {
    const name = this.cookieName(tenantKey);
    if (!cookies.get(name)) return null;
    const sealed = await this.sealed(tenantKey, cookies);
    const access = sealed && (await this.access(sealed, url.origin));
    if (!access) {
      cookies.delete(name, { path: '/' });
      return null;
    }
    return new BrowserSession(tenantKey, access);
  }

  /** Revoke this tenant's refresh token (RFC 7009) and clear its cookie; idempotent. */
  async logout(tenantKey: string, url: URL, cookies: Cookies): Promise<void> {
    const sealed = await this.sealed(tenantKey, cookies);
    cookies.delete(this.cookieName(tenantKey), { path: '/' });
    if (!sealed) return;
    this.#access.delete(this.hash(sealed.credential));
    // An operator API key is never revoked by signing out of the UI.
    if (sealed.kind === 'refresh')
      await client
        .tokenRevocation(await this.configuration(url.origin), sealed.credential, {
          token_type_hint: 'refresh_token'
        })
        .catch(() => reject('upstream'));
  }

  /** Switch to `target`: its own session when the server still renews it, else its sign-in. */
  async switch(target: string, url: URL, cookies: Cookies): Promise<string> {
    const base = `/t/${encodeURIComponent(target)}`;
    return (await this.read(target, url, cookies)) ? base : `${base}/login`;
  }

  /**
   * Safe page metadata: the current tenant plus every other tenant whose
   * session cookie this BFF sealed for that tenant. Cookies that do not
   * decrypt or name another tenant are cleared and never rendered.
   */
  async metadata(session: BrowserSession, cookies: Cookies): Promise<SessionMetadata> {
    const tenants = [session.tenantKey];
    for (const { name } of cookies.getAll()) {
      const key = name.slice(sessionPrefix.length);
      if (!name.startsWith(sessionPrefix) || key === session.tenantKey) continue;
      if (tenantKeyPattern.test(key) && (await this.sealed(key, cookies))) tenants.push(key);
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
