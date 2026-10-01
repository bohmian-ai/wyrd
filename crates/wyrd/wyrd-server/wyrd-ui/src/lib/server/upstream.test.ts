import { expect, test, vi } from 'vitest';
import { env } from '$env/dynamic/private';

vi.mock('$env/dynamic/private', () => ({ env: { WYRD_SERVER_URL: 'https://wyrd.internal:8443' } }));

const { serverReady, serverUrl } = await import('./upstream');

/** Fetcher that records every URL it is asked for and answers ready. */
function recording() {
  const seen: string[] = [];
  const fetcher = (async (url: URL) => {
    seen.push(url.toString());
    return new Response('ok');
  }) as unknown as typeof fetch;
  return { seen, fetcher };
}

test('readiness asks the configured server and maps failures to not ready', async () => {
  const { seen, fetcher } = recording();
  expect(await serverReady(fetcher)).toBe(true);
  expect(seen).toEqual(['https://wyrd.internal:8443/readyz']);
  const unready = (async () => new Response('', { status: 503 })) as unknown as typeof fetch;
  expect(await serverReady(unready)).toBe(false);
  const down = (async () => {
    throw new TypeError('connection refused');
  }) as unknown as typeof fetch;
  expect(await serverReady(down)).toBe(false);
});

test('https server origin is accepted', () => {
  env.WYRD_SERVER_URL = 'https://wyrd.internal:8443';
  expect(serverUrl()).toBe('https://wyrd.internal:8443');
});

test('loopback http server origin is accepted', () => {
  try {
    const origins = ['http://localhost:8080', 'http://127.0.0.1:8080', 'http://127.9.8.7', 'http://[::1]:8080'];
    for (const origin of origins) {
      env.WYRD_SERVER_URL = origin;
      expect(serverUrl()).toBe(origin);
    }
    delete env.WYRD_SERVER_URL;
    expect(serverUrl()).toBe('http://127.0.0.1:8080');
  } finally {
    env.WYRD_SERVER_URL = 'https://wyrd.internal:8443';
  }
});

test('non-loopback http server origin is refused before fetch', async () => {
  try {
    const origins = [
      'http://wyrd.internal:8080',
      'http://127.0.0.1.evil.test',
      'http://10.0.0.5',
      'ftp://wyrd.internal',
      'not a url'
    ];
    for (const origin of origins) {
      env.WYRD_SERVER_URL = origin;
      expect(() => serverUrl()).toThrow();
      const { seen, fetcher } = recording();
      expect(await serverReady(fetcher)).toBe(false);
      expect(seen).toEqual([]);
    }
  } finally {
    env.WYRD_SERVER_URL = 'https://wyrd.internal:8443';
  }
});

test('empty upstream value is refused before fetch', async () => {
  try {
    env.WYRD_SERVER_URL = '';
    expect(() => serverUrl()).toThrow('WYRD_SERVER_URL is not a valid URL');
    const { seen, fetcher } = recording();
    expect(await serverReady(fetcher)).toBe(false);
    expect(seen).toEqual([]);
  } finally {
    env.WYRD_SERVER_URL = 'https://wyrd.internal:8443';
  }
});
