import { expect, test, vi } from 'vitest';

vi.mock('$env/dynamic/private', () => ({ env: { WYRD_SERVER_URL: 'http://wyrd.internal:8080' } }));

const { serverReady } = await import('./upstream');

test('readiness asks the configured server and maps failures to not ready', async () => {
  const seen: string[] = [];
  const ok = (async (url: URL) => {
    seen.push(url.toString());
    return new Response('ok');
  }) as unknown as typeof fetch;
  expect(await serverReady(ok)).toBe(true);
  expect(seen).toEqual(['http://wyrd.internal:8080/readyz']);
  const unready = (async () => new Response('', { status: 503 })) as unknown as typeof fetch;
  expect(await serverReady(unready)).toBe(false);
  const down = (async () => {
    throw new TypeError('connection refused');
  }) as unknown as typeof fetch;
  expect(await serverReady(down)).toBe(false);
});
