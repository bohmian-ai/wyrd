import { env } from '$env/dynamic/private';
import { problem } from './problem';

/** `localhost` or a literal loopback address, as normalized by the URL parser. */
export const loopback = (hostname: string) =>
  hostname === 'localhost' || hostname === '[::1]' || /^127\.\d+\.\d+\.\d+$/.test(hostname);

/**
 * Internal Wyrd server origin; the application image points it at the
 * co-located Rust listener. This one origin carries session credentials, so
 * it must be `https:`; plaintext `http:` is allowed
 * only for `localhost` and literal loopback addresses. Only an absent value
 * takes the loopback default; an explicit empty value is refused like any
 * other invalid one. Throws on any other value, before a caller can send a
 * request.
 */
export function serverUrl(): string {
  const configured = env.WYRD_SERVER_URL ?? 'http://127.0.0.1:8080';
  let url: URL;
  try {
    url = new URL(configured);
  } catch {
    throw new Error('WYRD_SERVER_URL is not a valid URL');
  }
  if (url.protocol !== 'https:' && !(url.protocol === 'http:' && loopback(url.hostname)))
    throw new Error('WYRD_SERVER_URL must use https unless it is a loopback address');
  return configured;
}

/**
 * Request headers never forwarded: hop-by-hop headers, the incoming
 * forwarded-origin chain (the browser cannot choose the public origin), and
 * `accept-encoding`, because `fetch` decodes the body it hands back.
 */
const dropped = [
  'host',
  'connection',
  'keep-alive',
  'transfer-encoding',
  'upgrade',
  'content-length',
  'accept-encoding',
  'forwarded',
  'x-forwarded-for',
  'x-forwarded-host',
  'x-forwarded-proto',
  'x-real-ip'
];

/** The server's sign-in binding cookie, which every forwarded `/auth/*` request carries. */
const loginCookie = 'wyrd_login';

/** Prefix of the server's tenant session cookies, which only `/auth/browser/*` requests carry. */
const sessionPrefix = 'wyrd_session_';

/**
 * Send one browser `/auth/*` request to the configured Wyrd server and return
 * its answer unchanged: method, path and query, body, the server's own cookies,
 * redirects (never followed here), and error responses. Every request carries
 * the login binding; the browser-session operations also carry the tenant
 * session cookies, which the server alone selects, validates, and revokes.
 * `url` is the request's public URL as this BFF established it, so the server
 * derives sign-in redirects and callbacks from the origin the browser actually
 * reached, never from a forwarded header the browser sent. A server that
 * cannot be reached is the stable `upstream` problem.
 */
export async function forwardAuth(
  request: Request,
  url: URL,
  fetcher: typeof fetch = fetch
): Promise<Response> {
  const headers = new Headers(request.headers);
  for (const name of dropped) headers.delete(name);
  // Unrelated application cookies never ride a forwarded request.
  const sessions = url.pathname.startsWith('/auth/browser/');
  const forwarded = (request.headers.get('cookie') ?? '')
    .split(';')
    .map((pair) => pair.trim())
    .filter(
      (pair) => pair.startsWith(`${loginCookie}=`) || (sessions && pair.startsWith(sessionPrefix))
    );
  if (forwarded.length) headers.set('cookie', forwarded.join('; '));
  else headers.delete('cookie');
  headers.set('x-forwarded-host', url.host);
  headers.set('x-forwarded-proto', url.protocol.slice(0, -1));
  const body = request.method === 'GET' || request.method === 'HEAD' ? undefined : request.body;
  let answer: Response;
  try {
    answer = await fetcher(new URL(url.pathname + url.search, serverUrl()), {
      method: request.method,
      headers,
      body,
      redirect: 'manual',
      ...(body ? { duplex: 'half' } : {})
    } as RequestInit);
  } catch {
    const value = problem('upstream');
    return Response.json(value, {
      status: value.status,
      headers: { 'content-type': 'application/problem+json' }
    });
  }
  const returned = new Headers(answer.headers);
  for (const name of ['content-encoding', 'content-length', 'transfer-encoding', 'connection'])
    returned.delete(name);
  return new Response(answer.body, {
    status: answer.status,
    statusText: answer.statusText,
    headers: returned
  });
}

/** Ask the Wyrd server whether it is ready to serve; any failure or timeout counts as not ready. */
export async function serverReady(fetcher: typeof fetch = fetch): Promise<boolean> {
  try {
    const response = await fetcher(new URL('/readyz', serverUrl()), {
      signal: AbortSignal.timeout(2000)
    });
    return response.ok;
  } catch {
    return false;
  }
}
