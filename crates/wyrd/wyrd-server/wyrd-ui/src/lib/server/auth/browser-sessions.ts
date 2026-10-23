import type { Cookies } from '@sveltejs/kit';
import { WyrdClient } from 'wyrd-ai';
import { serverUrl } from '../upstream';
import { problemKind } from '../problem';
import { reject, type SessionMetadata, type TenantContext } from './session';

const sessionPrefix = 'wyrd_session_';
const tenantKeyPattern = /^[a-z0-9][a-z0-9_-]{0,62}$/;

/** The server's safe metadata of one live tenant browser session (`BrowserSessionView`). */
type View = {
  tenant: string;
  tenant_id: string;
  tenant_name: string;
  principal_id: string;
  permissions: string[];
  expires_at: string;
};

type Call = { method: 'GET' | 'POST'; cookie?: string; body?: unknown };

/**
 * Call one Rust `/auth/browser/*` or `/v1` operation server to server as the
 * browser at `url`: the trusted public origin goes as `Origin` and the
 * forwarded host and scheme, and only the cookie the caller selected is sent.
 * An unreachable server is the stable `upstream` problem.
 */
async function call(path: string, url: URL, { method, cookie, body }: Call): Promise<Response> {
  try {
    return await fetch(new URL(path, serverUrl()), {
      method,
      headers: {
        origin: url.origin,
        'x-forwarded-host': url.host,
        'x-forwarded-proto': url.protocol.slice(0, -1),
        ...(cookie ? { cookie } : {}),
        ...(body === undefined ? {} : { 'content-type': 'application/json' })
      },
      body: body === undefined ? undefined : JSON.stringify(body),
      redirect: 'manual',
      signal: AbortSignal.timeout(30_000)
    });
  } catch {
    reject('upstream');
  }
}

/** Refuse with the safe problem a non-2xx Wyrd answer maps to. */
async function refuse(response: Response): Promise<never> {
  reject(await problemKind(response));
}

/**
 * Apply every `Set-Cookie` the server answered to the browser's cookie jar
 * with the server's own attributes; a `Max-Age=0` cookie is deleted. The
 * cookie values are the server's and are never inspected here.
 */
export function relayCookies(response: Response, cookies: Cookies): void {
  for (const header of response.headers.getSetCookie()) {
    const [pair, ...attributes] = header.split(';').map((part) => part.trim());
    const split = pair.indexOf('=');
    if (split <= 0) continue;
    const name = pair.slice(0, split);
    const value = pair.slice(split + 1);
    const options: Parameters<Cookies['set']>[2] = {
      path: '/',
      httpOnly: false,
      secure: false,
      encode: (raw: string) => raw
    };
    let maxAge: number | undefined;
    for (const attribute of attributes) {
      const [key, setting = ''] = attribute.split('=');
      switch (key.toLowerCase()) {
        case 'path':
          options.path = setting;
          break;
        case 'max-age':
          maxAge = Number(setting);
          break;
        case 'httponly':
          options.httpOnly = true;
          break;
        case 'secure':
          options.secure = true;
          break;
        case 'samesite':
          options.sameSite = setting.toLowerCase() as 'lax' | 'strict' | 'none';
          break;
      }
    }
    if (maxAge === 0) cookies.delete(name, options);
    else cookies.set(name, value, { ...options, maxAge });
  }
}

/**
 * One signed-in tenant session of this request. The session cookie value
 * stays in a private field: the object cannot be serialized into page data.
 */
export class BrowserSession {
  readonly #cookie: string;
  readonly #url: URL;

  /** `cookie` is the tenant's `name=value` session cookie; `url` the browser's request URL. */
  constructor(
    readonly tenantKey: string,
    view: View,
    cookie: string,
    url: URL
  ) {
    this.#cookie = cookie;
    this.#url = url;
    this.tenantName = view.tenant_name;
    this.principalId = view.principal_id;
    this.tenantId = view.tenant_id;
    this.permissions = view.permissions;
    this.expiresAt = Date.parse(view.expires_at);
  }

  readonly tenantName: string;
  readonly principalId: string;
  readonly tenantId: string;
  readonly permissions: string[];
  readonly expiresAt: number;

  /** Tenant context from the server's session metadata; the UI maps no roles. */
  context(): TenantContext {
    return {
      tenant: {
        key: this.tenantKey,
        name: this.tenantName,
        tenantId: this.tenantId,
        permissions: this.permissions
      },
      subject: { id: this.principalId, name: this.principalId },
      permissions: this.permissions
    };
  }

  /**
   * Call a Wyrd `/v1` API as this session's principal with only this tenant's
   * session cookie and the trusted origin; the server authorizes every call.
   */
  api(method: string, path: string, body?: unknown): Promise<Response> {
    return call(`/v1${path}`, this.#url, {
      method: method as Call['method'],
      cookie: this.#cookie,
      body
    });
  }

  /**
   * An SDK client acting as this session: the Rust client forwards only this
   * tenant's session cookie and the trusted origin, so the server authorizes
   * every read exactly as it does this session's `/v1` calls.
   */
  client(): WyrdClient {
    return WyrdClient.forBrowserSession({ serverUrl: serverUrl(), cookie: this.#cookie, origin: this.#url.origin });
  }
}

/**
 * The BFF's projection of Rust-owned browser sessions. Rust creates,
 * resolves, and revokes every session and sets every cookie; this class only
 * calls the `/auth/browser/*` operations server to server for the browser and
 * relays the cookies they answer. It holds no tokens, no secrets, and no
 * cache, so every replica behaves the same.
 */
export class BrowserSessions {
  /** Cookie name for one tenant's session; refuses keys that are not tenant slugs. */
  cookieName(tenantKey: string): string {
    if (!tenantKeyPattern.test(tenantKey)) reject('notFound');
    return sessionPrefix + tenantKey;
  }

  /** Begin SSO sign-in: the server sets the login cookie and returns the provider URL. */
  async begin(tenantKey: string, url: URL, cookies: Cookies): Promise<string> {
    this.cookieName(tenantKey);
    const response = await call('/auth/browser/login', url, {
      method: 'POST',
      body: { tenant: tenantKey }
    });
    if (!response.ok) await refuse(response);
    relayCookies(response, cookies);
    const begun = (await response.json()) as { authorization_url: string };
    return begun.authorization_url;
  }

  /** Recovery sign-in: the server verifies the API key and sets the tenant session cookie. */
  async signInWithApiKey(
    tenantKey: string,
    apiKey: string,
    url: URL,
    cookies: Cookies
  ): Promise<string> {
    this.cookieName(tenantKey);
    const response = await call('/auth/browser/api-key', url, {
      method: 'POST',
      body: { tenant: tenantKey, api_key: apiKey }
    });
    if (!response.ok) await refuse(response);
    relayCookies(response, cookies);
    return `/t/${encodeURIComponent(tenantKey)}`;
  }

  /**
   * This tenant's session, resolved by the server from this tenant's cookie
   * alone. Returns `null` (and clears the cookie) when there is none or the
   * server says it is not live; a server outage is the `upstream` problem.
   */
  async read(tenantKey: string, url: URL, cookies: Cookies): Promise<BrowserSession | null> {
    const name = this.cookieName(tenantKey);
    const value = cookies.get(name);
    if (!value) return null;
    const cookie = `${name}=${value}`;
    const response = await call(`/auth/browser/session?tenant=${tenantKey}`, url, {
      method: 'GET',
      cookie
    });
    relayCookies(response, cookies);
    if (response.status === 401) {
      cookies.delete(name, { path: '/' });
      return null;
    }
    if (!response.ok) reject('upstream');
    const { sessions } = (await response.json()) as { sessions: View[] };
    const view = sessions.find((session) => session.tenant === tenantKey);
    if (!view) reject('upstream');
    return new BrowserSession(tenantKey, view, cookie, url);
  }

  /**
   * Sign out of this tenant: the server revokes only this session and clears
   * its cookie. A refusal or outage is reported, so a failed sign-out is
   * never mistaken for a successful one.
   */
  async logout(tenantKey: string, url: URL, cookies: Cookies): Promise<void> {
    const name = this.cookieName(tenantKey);
    const value = cookies.get(name);
    const response = await call('/auth/browser/logout', url, {
      method: 'POST',
      cookie: value ? `${name}=${value}` : undefined,
      body: { tenant: tenantKey }
    });
    if (!response.ok) await refuse(response);
    relayCookies(response, cookies);
  }

  /** Switch to `target`: its own session when the server still resolves it, else its sign-in. */
  async switch(target: string, url: URL, cookies: Cookies): Promise<string> {
    const base = `/t/${encodeURIComponent(target)}`;
    return (await this.read(target, url, cookies)) ? base : `${base}/login`;
  }

  /**
   * Safe page metadata from the server's current answer alone: the current
   * tenant plus every other tenant whose session the server still resolves.
   * Every session cookie is presented to the one operation that accepts
   * several; the server clears dead ones. A refusal or outage is reported,
   * and a current tenant missing from the live set is unauthenticated, so an
   * earlier read never stands in for the server's present answer.
   */
  async metadata(session: BrowserSession, url: URL, cookies: Cookies): Promise<SessionMetadata> {
    const presented = cookies
      .getAll()
      .filter(({ name }) => name.startsWith(sessionPrefix))
      .map(({ name, value }) => `${name}=${value}`)
      .join('; ');
    const response = await call('/auth/browser/session', url, {
      method: 'GET',
      cookie: presented
    });
    relayCookies(response, cookies);
    if (!response.ok) await refuse(response);
    const { sessions } = (await response.json()) as { sessions: View[] };
    const current = sessions.find((view) => view.tenant === session.tenantKey);
    if (!current) reject('unauthenticated');
    return {
      subject: { id: current.principal_id, name: current.principal_id },
      expiresAt: Date.parse(current.expires_at),
      tenants: [current, ...sessions.filter((view) => view !== current)].map((view) => ({
        key: view.tenant,
        name: view.tenant_name
      }))
    };
  }
}

export const browserSessions = new BrowserSessions();
