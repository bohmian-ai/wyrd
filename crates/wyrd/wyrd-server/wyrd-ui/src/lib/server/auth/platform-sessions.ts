import type { Cookies } from '@sveltejs/kit';
import { call, refuse, relayCookies } from './browser-sessions';
import { reject } from './session';

/** The platform console's session cookie and its OIDC login binding; the BFF forwards Rust nothing else here. */
const platformCookies = ['wyrd_platform_session', 'wyrd_platform_login'] as const;

/** How a platform console session was signed in. */
export type PlatformMethod = 'password' | 'oidc';

/** Rust's safe metadata of one live platform console session (`PlatformBrowserSessionView`). */
export type PlatformSessionView = {
  principal_id: string;
  name: string;
  method: PlatformMethod;
  expires_at: string;
  idle_expires_at: string;
  recent: boolean;
};

/** The `name=value` pairs of the platform cookies this browser presents, never any other cookie. */
function presented(cookies: Cookies): string | undefined {
  const pairs = platformCookies.flatMap((name) => {
    const value = cookies.get(name);
    return value ? [`${name}=${value}`] : [];
  });
  return pairs.length ? pairs.join('; ') : undefined;
}

/**
 * One live platform console session of this request. The cookie stays in a
 * private field, so the object cannot be serialized into page data.
 */
export class PlatformSession {
  readonly #cookies: Cookies;
  readonly #url: URL;

  constructor(
    readonly view: PlatformSessionView,
    cookies: Cookies,
    url: URL
  ) {
    this.#cookies = cookies;
    this.#url = url;
  }

  /**
   * Call one Rust `/platform/*` operation as this session with only the
   * platform cookies and the trusted origin; Rust authorizes every call and
   * checks fresh proof for destructive ones. Cookies Rust answers are relayed.
   */
  async api(method: 'GET' | 'POST' | 'PUT', path: string, body?: unknown): Promise<Response> {
    const response = await call(`/platform${path}`, this.#url, { method, cookie: presented(this.#cookies), body });
    relayCookies(response, this.#cookies);
    return response;
  }

  /** `api` answering the JSON body of a 2xx, else refusing with its safe problem. */
  async json<T>(method: 'GET' | 'POST' | 'PUT', path: string, body?: unknown): Promise<T> {
    const response = await this.api(method, path, body);
    if (!response.ok) await refuse(response);
    return (await response.json()) as T;
  }
}

/**
 * The BFF's projection of Rust-owned platform console sessions. Rust signs
 * operators in, resolves, refreshes and revokes every session and sets every
 * cookie; this class calls `/platform/browser/*` server to server for the
 * browser and relays the cookies Rust answers. It holds no token or cache.
 */
export class PlatformConsole {
  /** Which console sign-in methods Rust offers; rendering only, never enforcement. Unknown is password only. */
  async methods(url: URL): Promise<{ password: boolean; oidc: boolean }> {
    const response = await call('/platform/browser/methods', url, { method: 'GET' });
    if (!response.ok) return { password: true, oidc: false };
    const methods = (await response.json()) as { password?: unknown; oidc?: unknown };
    return { password: methods.password !== false, oidc: methods.oidc === true };
  }

  /** Platform-user password sign-in: Rust verifies the password and sets the session cookie. */
  async password(username: string, password: string, url: URL, cookies: Cookies): Promise<void> {
    const response = await call('/platform/browser/password/login', url, { method: 'POST', body: { username, password } });
    if (!response.ok) await refuse(response);
    relayCookies(response, cookies);
  }

  /**
   * Begin a platform SSO sign-in, or with `session` a forced-fresh
   * re-authentication of it; Rust sets the login binding cookie and returns
   * the provider URL.
   */
  async beginSso(url: URL, cookies: Cookies, session?: PlatformSession): Promise<string> {
    const response = session
      ? await session.api('POST', '/browser/oidc/reauth')
      : await call('/platform/browser/oidc/login', url, { method: 'POST' });
    if (!response.ok) await refuse(response);
    if (!session) relayCookies(response, cookies);
    return ((await response.json()) as { authorization_url: string }).authorization_url;
  }

  /**
   * Finish a platform SSO return with the browser's binding (and, for a
   * re-authentication, its session). Answers whether it refreshed an existing
   * session rather than signing in.
   */
  async finishSso(fields: { code: string; state: string; iss?: string }, url: URL, cookies: Cookies): Promise<boolean> {
    const signedIn = Boolean(cookies.get('wyrd_platform_session'));
    const response = await call('/platform/browser/oidc/callback', url, { method: 'POST', cookie: presented(cookies), body: fields });
    if (!response.ok) await refuse(response);
    const fresh = response.headers.getSetCookie().some((header) => header.startsWith('wyrd_platform_session='));
    relayCookies(response, cookies);
    return signedIn && !fresh;
  }

  /** Prove this session's own password again, refreshing its proof for destructive changes. */
  async reauthenticate(session: PlatformSession, password: string): Promise<void> {
    const response = await session.api('POST', '/browser/password/reauth', { password });
    if (!response.ok) await refuse(response);
  }

  /**
   * This browser's console session, resolved by Rust from the platform cookie
   * alone; `null` when there is none or Rust says it is not live. An outage
   * is the `upstream` problem.
   */
  async read(url: URL, cookies: Cookies): Promise<PlatformSession | null> {
    if (!cookies.get('wyrd_platform_session')) return null;
    const response = await call('/platform/browser/session', url, { method: 'GET', cookie: presented(cookies) });
    relayCookies(response, cookies);
    if (response.status === 401) {
      cookies.delete('wyrd_platform_session', { path: '/platform' });
      return null;
    }
    if (!response.ok) reject('upstream');
    return new PlatformSession((await response.json()) as PlatformSessionView, cookies, url);
  }

  /** Sign out: Rust revokes only this session and clears its cookie; a refusal is reported. */
  async logout(url: URL, cookies: Cookies): Promise<void> {
    const response = await call('/platform/browser/logout', url, { method: 'POST', cookie: presented(cookies) });
    if (!response.ok) await refuse(response);
    relayCookies(response, cookies);
  }
}

export const platformConsole = new PlatformConsole();
