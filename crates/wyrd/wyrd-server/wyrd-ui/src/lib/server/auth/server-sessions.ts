import { createHash, randomBytes, timingSafeEqual } from 'node:crypto';
import { env } from '$env/dynamic/private';
import type { Cookies } from '@sveltejs/kit';
import type { SessionMetadata } from '$lib/views';
import { problem, safeProblem } from '../problem';
import { serverUrl } from '../upstream';
import { reject, sessions as localSessions, type TenantContext } from './session';

export const flowCookie = 'wyrd_flow';
const sessionPrefix = 'wyrd_session_';
const flowLifetimeSeconds = 5 * 60;
const tenantKeyPattern = /^[a-z0-9][a-z0-9_-]{0,62}$/;

/** Safe projection of a server-owned browser session; never holds the session id or a token. */
export type ServerSession = {
  tenantKey: string;
  /** Server-owned tenant UUID; server-only state, never page data. */
  tenantId: string;
  tenantName: string;
  principalId: string;
  roles: string[];
  permissions: string[];
  expiresAt: number;
  csrf: string;
};

type Created = { session_id: string; tenant_key: string; expires_at: string };
type Read = {
  tenant_key: string;
  tenant_id: string;
  tenant_name: string;
  principal_id: string;
  roles: string[];
  permissions: string[];
  expires_at: string;
  csrf_token: string;
};

type ProblemKind = Parameters<typeof problem>[0];
const kindByStatus: Record<number, ProblemKind> = {
  400: 'validation',
  401: 'unauthenticated',
  403: 'denied',
  404: 'notFound',
  409: 'conflict'
};

/** Map a Wyrd problem response to a known safe problem kind: by code, then by status. */
export async function problemKind(response: Response): Promise<ProblemKind> {
  const known = safeProblem(await response.json().catch(() => null));
  const byCode = Object.entries(kindByStatus).find(
    ([, kind]) => problem(kind).code === known.code
  )?.[1];
  return byCode ?? kindByStatus[response.status] ?? 'upstream';
}

const random = () => randomBytes(32).toString('hex');
const sha256 = (value: string) => createHash('sha256').update(value, 'utf8').digest('hex');

/**
 * Production browser-session boundary of the BFF.
 *
 * The Wyrd server owns every session record and credential; this class only
 * holds the opaque per-tenant session cookie, checks CSRF and same-origin, and
 * talks to the private `/internal/bff/v1/*` channel with the deployment BFF
 * service key. The session id and access token never leave this class except
 * as the HttpOnly cookie value and the server-side `X-Wyrd-Access-Token` header.
 */
export class ServerSessions {
  constructor(private readonly fetcher: typeof fetch = (input, init) => fetch(input, init)) {}

  /** Cookie name for one tenant's session; refuses keys that are not tenant slugs. */
  cookieName(tenantKey: string): string {
    if (!tenantKeyPattern.test(tenantKey)) reject('notFound');
    return sessionPrefix + tenantKey;
  }

  private async channel(path: string, body: unknown): Promise<Response> {
    const key = env.WYRD_BFF_SERVICE_KEY;
    if (!key) reject('upstream');
    try {
      return await this.fetcher(new URL(`/internal/bff/v1/${path}`, serverUrl()), {
        method: 'POST',
        headers: { 'content-type': 'application/json', 'x-wyrd-bff-key': key },
        body: JSON.stringify(body),
        signal: AbortSignal.timeout(10_000)
      });
    } catch {
      reject('upstream');
    }
  }

  private async json<T>(path: string, body: unknown, status: number): Promise<T> {
    const response = await this.channel(path, body);
    if (response.status !== status) reject(await problemKind(response));
    return (await response.json()) as T;
  }

  /** Whether the tenant offers SSO; an unknown tenant is indistinguishable from SSO-off. */
  async loginOptions(tenantKey: string): Promise<{ sso: boolean }> {
    this.cookieName(tenantKey);
    const { sso } = await this.json<{ sso: boolean }>(
      'login/options',
      { tenant_route_key: tenantKey },
      200
    );
    return { sso: sso === true };
  }

  /** Bind a fresh browser flow cookie to server login state and return the provider URL. */
  async begin(tenantKey: string, cookies: Cookies): Promise<string> {
    this.cookieName(tenantKey);
    const flow = random();
    let response: Response;
    try {
      response = await this.fetcher(new URL('/auth/login', serverUrl()), {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ tenant_route_key: tenantKey, browser_flow_hash: sha256(flow) }),
        signal: AbortSignal.timeout(10_000)
      });
    } catch {
      reject('upstream');
    }
    if (!response.ok) reject(await problemKind(response));
    const { authorization_url } = (await response.json()) as { authorization_url?: unknown };
    let target: URL;
    try {
      target = new URL(String(authorization_url));
    } catch {
      reject('upstream');
    }
    if (target.protocol !== 'https:' && target.protocol !== 'http:') reject('upstream');
    cookies.set(flowCookie, flow, {
      path: '/',
      httpOnly: true,
      secure: true,
      sameSite: 'lax',
      maxAge: flowLifetimeSeconds
    });
    return target.toString();
  }

  private establish(cookies: Cookies, created: Created): string {
    const expires = Math.floor((Date.parse(created.expires_at) - Date.now()) / 1000);
    if (!(expires > 0) || !/^[a-f0-9]{64}$/.test(created.session_id)) reject('upstream');
    cookies.set(this.cookieName(created.tenant_key), created.session_id, {
      path: '/',
      httpOnly: true,
      secure: true,
      sameSite: 'lax',
      maxAge: expires
    });
    return `/t/${encodeURIComponent(created.tenant_key)}`;
  }

  /**
   * Redeem the server's sealed completion for this browser's flow cookie and
   * set the tenant session cookie. Always clears the flow cookie; any failure
   * sets no session cookie.
   */
  async complete(cookies: Cookies): Promise<string> {
    const flow = cookies.get(flowCookie);
    cookies.delete(flowCookie, { path: '/' });
    if (!flow || !/^[a-f0-9]{64}$/.test(flow)) reject('unauthenticated');
    const created = await this.json<Created>(
      'sessions/complete',
      { flow_id: flow, csrf_token: random() },
      201
    );
    return this.establish(cookies, created);
  }

  /** Exchange an existing tenant API key for a browser session (OIDC-off deployments). */
  async exchangeApiKey(tenantKey: string, apiKey: string, cookies: Cookies): Promise<string> {
    this.cookieName(tenantKey);
    const created = await this.json<Created>(
      'sessions/api-key',
      { tenant_route_key: tenantKey, api_key: apiKey, csrf_token: random() },
      201
    );
    if (created.tenant_key !== tenantKey) reject('denied');
    return this.establish(cookies, created);
  }

  /**
   * Resolve this tenant's session through the server. Returns `null` (and
   * clears the cookie) when there is none, the server no longer honours it, or
   * the server binds it to a different tenant: the cookie name is only a hint,
   * so a session of another tenant is no session here.
   */
  async read(tenantKey: string, cookies: Cookies): Promise<ServerSession | null> {
    const name = this.cookieName(tenantKey);
    const id = cookies.get(name);
    if (!id) return null;
    const response = await this.channel('sessions/read', { session_id: id });
    if (response.status === 401) {
      cookies.delete(name, { path: '/' });
      return null;
    }
    if (response.status !== 200) reject(await problemKind(response));
    const read = (await response.json()) as Read;
    if (read.tenant_key !== tenantKey) {
      cookies.delete(name, { path: '/' });
      return null;
    }
    return {
      tenantKey: read.tenant_key,
      tenantId: read.tenant_id,
      tenantName: read.tenant_name,
      principalId: read.principal_id,
      roles: read.roles,
      permissions: read.permissions,
      expiresAt: Date.parse(read.expires_at),
      csrf: read.csrf_token
    };
  }

  /** Tenant context bound from server-returned data; the UI maps no roles. */
  context(session: ServerSession): TenantContext {
    return {
      tenant: {
        key: session.tenantKey,
        name: session.tenantName,
        tenantId: session.tenantId,
        permissions: session.permissions
      },
      subject: { id: session.principalId, name: session.principalId },
      permissions: session.permissions
    };
  }

  /** Require POST, same origin, an unexpired session, and its CSRF token (constant time). */
  checkAction(session: ServerSession, request: Request, csrf: FormDataEntryValue | null): void {
    if (session.expiresAt <= Date.now()) reject('expired');
    const expected = Buffer.from(session.csrf);
    const submitted = Buffer.from(typeof csrf === 'string' ? csrf : '');
    if (
      request.method !== 'POST' ||
      request.headers.get('origin') !== new URL(request.url).origin ||
      submitted.length !== expected.length ||
      !timingSafeEqual(submitted, expected)
    )
      reject('denied');
  }

  /** End this tenant's browser session; idempotent and always clears the cookie. */
  async logout(
    tenantKey: string,
    cookies: Cookies,
    request: Request,
    csrf: FormDataEntryValue | null
  ): Promise<void> {
    const name = this.cookieName(tenantKey);
    const session = await this.read(tenantKey, cookies);
    if (session) {
      this.checkAction(session, request, csrf);
      const response = await this.channel('sessions/logout', { session_id: cookies.get(name) });
      if (response.status !== 204 && response.status !== 401)
        reject(await problemKind(response));
    }
    cookies.delete(name, { path: '/' });
  }

  /**
   * Leave `from` for `target`: the target's own session if it has one, else its
   * login. The current session is only used to authorize the action.
   */
  async switch(
    from: string,
    target: string,
    cookies: Cookies,
    request: Request,
    csrf: FormDataEntryValue | null
  ): Promise<string> {
    const current = await this.read(from, cookies);
    if (!current) reject('unauthenticated');
    this.checkAction(current, request, csrf);
    const base = `/t/${encodeURIComponent(target)}`;
    return (await this.read(target, cookies)) ? base : `${base}/login`;
  }

  /**
   * Safe page metadata: the current tenant plus every other tenant whose
   * session the server still honours for this browser. Session-cookie names
   * are only lookup hints: each distinct hinted tenant is resolved through
   * `read`, which clears unknown, expired, and mismatched cookies, and only the
   * server-returned key and name are rendered. Hints come from the request,
   * so they are resolved one at a time: at most one private verification is
   * in flight however many cookies a request carries.
   */
  async metadata(session: ServerSession, cookies: Cookies): Promise<SessionMetadata> {
    const hints = new Set(
      cookies
        .getAll()
        .filter(({ name }) => name.startsWith(sessionPrefix))
        .map(({ name }) => name.slice(sessionPrefix.length))
        .filter((key) => key !== session.tenantKey && tenantKeyPattern.test(key))
    );
    const others: (ServerSession | null)[] = [];
    for (const key of hints) others.push(await this.read(key, cookies));
    return {
      subject: { id: session.principalId, name: session.principalId },
      expiresAt: session.expiresAt,
      csrf: session.csrf,
      tenants: [session, ...others]
        .filter((verified) => verified !== null)
        .map(({ tenantKey, tenantName }) => ({ key: tenantKey, name: tenantName }))
    };
  }

  /**
   * Call a Wyrd `/v1` API as the session's principal. The short-lived access
   * token comes from the private channel and is used only in this request's
   * `X-Wyrd-Access-Token` header.
   */
  async api(
    tenantKey: string,
    cookies: Cookies,
    method: string,
    path: string,
    body?: unknown
  ): Promise<Response> {
    const name = this.cookieName(tenantKey);
    const id = cookies.get(name);
    if (!id) reject('unauthenticated');
    const response = await this.channel('sessions/authority', { session_id: id });
    if (response.status === 401) {
      cookies.delete(name, { path: '/' });
      reject('unauthenticated');
    }
    if (response.status !== 200) reject(await problemKind(response));
    const { access_token } = (await response.json()) as { access_token: string };
    try {
      return await this.fetcher(new URL(`/v1${path}`, serverUrl()), {
        method,
        headers: {
          'x-wyrd-access-token': `Bearer ${access_token}`,
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

export const serverSessions = new ServerSessions();

/** Check a mutating action against whichever session boundary this request carries. */
export function checkAction(
  locals: App.Locals,
  request: Request,
  csrf: FormDataEntryValue | null
): void {
  if (locals.session) return localSessions.checkAction(locals.session, request, csrf);
  if (locals.serverSession)
    return serverSessions.checkAction(locals.serverSession, request, csrf);
  reject('unauthenticated');
}

/** Safe page metadata for whichever session boundary this request carries. */
export async function sessionMetadata(
  locals: App.Locals,
  cookies: Cookies
): Promise<SessionMetadata> {
  if (locals.session) return localSessions.metadata(locals.session);
  if (locals.serverSession) return serverSessions.metadata(locals.serverSession, cookies);
  reject('unauthenticated');
}
