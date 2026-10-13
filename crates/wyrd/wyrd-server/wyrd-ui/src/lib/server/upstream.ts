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
/** Path parameters, query parameters and JSON body of one operation, each required exactly when the contract requires it. */
export type V1Request<P extends keyof V1, M extends V1Method<P>> =
  (Operation<P, M> extends { parameters: { path: infer A } } ? { path: A } : { path?: never }) &
    (Operation<P, M> extends { parameters: { query: infer Q } }
      ? { query: Q }
      : Operation<P, M> extends { parameters: { query?: infer Q } }
        ? { query?: Q }
        : { query?: never }) &
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

/**
 * Fill the `{name}` segments of a generated path template and append the
 * defined query parameters; a missing path parameter throws.
 */
export function v1Path(template: string, params: unknown = {}, query: unknown = {}): string {
  const values = (params ?? {}) as Record<string, unknown>;
  const path = template.replace(/\{(\w+)\}/g, (_, name: string) => {
    if (values[name] === undefined) throw new Error(`missing path parameter ${name}`);
    return encodeURIComponent(String(values[name]));
  });
  const search = new URLSearchParams(
    Object.entries((query ?? {}) as Record<string, unknown>)
      .filter(([, value]) => value !== undefined)
      .map(([name, value]) => [name, String(value)])
  ).toString();
  return search ? `${path}?${search}` : path;
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
  return secured('WYRD_SERVER_URL', env.WYRD_SERVER_URL ?? 'http://127.0.0.1:8080');
}

/**
 * Bifrost gRPC endpoint the BFF's analytical reads use, under the same
 * transport rule as {@link serverUrl}. Absent, the SDK derives it from the
 * server URL's host on the public gRPC port, which the application image serves.
 */
export function grpcUrl(): string | undefined {
  const configured = env.WYRD_GRPC_URL;
  return configured === undefined ? undefined : secured('WYRD_GRPC_URL', configured);
}

/** `configured` when it is `https:` or loopback `http:`; throws naming `name` otherwise. */
function secured(name: string, configured: string): string {
  let url: URL;
  try {
    url = new URL(configured);
  } catch {
    throw new Error(`${name} is not a valid URL`);
  }
  if (url.protocol !== 'https:' && !(url.protocol === 'http:' && loopback(url.hostname)))
    throw new Error(`${name} must use https unless it is a loopback address`);
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
