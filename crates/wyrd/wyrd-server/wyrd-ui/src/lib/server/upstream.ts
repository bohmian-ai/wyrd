import { env } from '$env/dynamic/private';
import type { paths } from './generated/wyrd-http';

/** Generated `/v1` operations, keyed by their path below the `/v1` prefix the BFF adds. */
export type V1 = { [P in keyof paths as P extends `/v1/${infer R}` ? `/${R}` : never]: paths[P] };
type Method = 'get' | 'put' | 'post' | 'delete' | 'patch';
/** The methods the server serves at `P`. */
export type V1Method<P extends keyof V1> = {
  [M in Method]: [NonNullable<V1[P][M]>] extends [never] ? never : M;
}[Method];
type Operation<P extends keyof V1, M extends V1Method<P>> = NonNullable<V1[P][M]>;
/** Path parameters and JSON body of one operation, each required exactly when the contract requires it. */
export type V1Request<P extends keyof V1, M extends V1Method<P>> =
  (Operation<P, M> extends { parameters: { path: infer A } } ? { path: A } : { path?: never }) &
    (Operation<P, M> extends { requestBody: { content: { 'application/json': infer B } } }
      ? { body: B }
      : Operation<P, M> extends { requestBody?: { content: { 'application/json': infer B } } }
        ? { body?: B }
        : { body?: never });
/** The request argument, optional when the operation declares nothing required. */
export type V1Args<P extends keyof V1, M extends V1Method<P>> =
  {} extends V1Request<P, M> ? [request?: V1Request<P, M>] : [request: V1Request<P, M>];
type Json<R> = R extends { content: { 'application/json': infer J } } ? J : undefined;
/** JSON body of the operation's success response. */
export type V1Ok<P extends keyof V1, M extends V1Method<P>> =
  Operation<P, M> extends { responses: infer R }
    ? R extends { 200: infer S } ? Json<S> : R extends { 201: infer S } ? Json<S> : undefined
    : never;
/**
 * A response whose `json()` is typed as the operation's success body. Compile-time
 * contract only: callers check `ok` first, and the served-contract suite proves the
 * server's actual serialization.
 */
export type V1Response<P extends keyof V1, M extends V1Method<P>> = {
  json(): Promise<V1Ok<P, M>>;
} & Response;

/** Fill the `{name}` segments of a generated path template; a missing parameter throws. */
export function v1Path(template: string, params: unknown = {}): string {
  const values = (params ?? {}) as Record<string, unknown>;
  return template.replace(/\{(\w+)\}/g, (_, name: string) => {
    if (values[name] === undefined) throw new Error(`missing path parameter ${name}`);
    return encodeURIComponent(String(values[name]));
  });
}

/** `localhost` or a literal loopback address, as normalized by the URL parser. */
export const loopback = (hostname: string) =>
  hostname === 'localhost' || hostname === '[::1]' || /^127\.\d+\.\d+\.\d+$/.test(hostname);

/**
 * Internal Wyrd server origin; the application image points it at the
 * co-located Rust listener. This one origin carries the `wyrd-ui` client secret
 * and session credentials, so it must be `https:`; plaintext `http:` is allowed
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
