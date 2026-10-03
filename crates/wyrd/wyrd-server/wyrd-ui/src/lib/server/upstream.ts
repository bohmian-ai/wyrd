import { env } from '$env/dynamic/private';

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
