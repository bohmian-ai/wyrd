import { env } from '$env/dynamic/private';

/** Internal Wyrd server origin; the application image points it at the co-located Rust listener. */
export function serverUrl(): string {
  return env.WYRD_SERVER_URL || 'http://127.0.0.1:8080';
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
