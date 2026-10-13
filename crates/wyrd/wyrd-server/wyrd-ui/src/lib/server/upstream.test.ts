import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import ts from 'typescript';
import { expect, test, vi } from 'vitest';
import { env } from '$env/dynamic/private';

vi.mock('$env/dynamic/private', () => ({ env: { WYRD_SERVER_URL: 'https://wyrd.internal:8443' } }));

const { serverReady, serverUrl, v1Path } = await import('./upstream');

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

/** Root of the UI package; the TypeScript program resolves `$lib` and `./$types` from its tsconfig. */
const uiRoot = resolve(import.meta.dirname, '../../..');
const generated = resolve(uiRoot, 'src/lib/server/generated/wyrd-http.d.ts');
/** The representative BFF consumer: every operation it calls is typed by the generated contract. */
const consumer = resolve(uiRoot, 'src/routes/t/[tenantKey]/settings/+page.server.ts');

/**
 * Type-check the unchanged consumer against the generated contract, with `edit`
 * applied to the generated text, and return the consumer's diagnostics.
 */
function consumerErrors(edit: (text: string) => string = (text) => text): string[] {
  const config = ts.getParsedCommandLineOfConfigFile(resolve(uiRoot, 'tsconfig.json'), {}, {
    ...ts.sys,
    onUnRecoverableConfigFileDiagnostic: (diagnostic) => {
      throw new Error(ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n'));
    }
  })!;
  const host = ts.createCompilerHost(config.options);
  const readFile = host.readFile.bind(host);
  host.readFile = (file) => {
    const text = readFile(file);
    return text !== undefined && resolve(file) === generated ? edit(text) : text;
  };
  // Ambient declarations (`app.d.ts`, generated route types) are roots beside the consumer.
  const ambient = config.fileNames.filter((file) => file.endsWith('.d.ts'));
  const program = ts.createProgram([consumer, ...ambient], config.options, host);
  return ts
    .getPreEmitDiagnostics(program, program.getSourceFile(consumer))
    .map((diagnostic) => ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n'));
}

/** Replace exactly one occurrence of `from` after `anchor`, failing loudly when the probe no longer applies. */
function replaceAfter(text: string, anchor: string, from: string, to: string): string {
  const start = text.indexOf(anchor);
  const at = text.indexOf(from, start);
  expect(start, `anchor ${anchor}`).toBeGreaterThanOrEqual(0);
  expect(at, `${from} after ${anchor}`).toBeGreaterThan(start);
  return text.slice(0, at) + to + text.slice(at + from.length);
}

test('generated HTTP contracts detect drift and incompatible consumers', () => {
  // One owning workflow writes exactly one generated artifact.
  const directory = resolve(uiRoot, 'src/lib/server/generated');
  expect(readdirSync(directory)).toEqual(['wyrd-http.d.ts']);
  expect(readFileSync(generated, 'utf8')).toMatch(/^\/\*\*\n \* This file was auto-generated by openapi-typescript\./);

  // The unchanged consumer type-checks against the committed contract.
  expect(consumerErrors()).toEqual([]);

  // Each regenerated contract change breaks the unchanged consumer at type-check.
  const probes: [string, (text: string) => string, string][] = [
    [
      'renamed response field',
      (text) =>
        replaceAfter(text, '        ConnectionTestResponse: {', 'authorization_url:', 'authorize_url:'),
      "'authorization_url'"
    ],
    [
      'new required request field',
      (text) =>
        replaceAfter(text, '        ConnectionActivate: {', 'expected_revision: number;', 'expected_revision: number; reason: string;'),
      "'reason'"
    ],
    [
      'renamed path parameter',
      (text) => replaceAfter(text, '    remove_connection: {', '                id: string;', '                connection_id: string;'),
      "'id'"
    ],
    [
      'removed operation',
      (text) =>
        replaceAfter(text, 'export interface paths {', '"/v1/identity/oidc/candidate/activate": {', '"/v1/identity/oidc/candidate/activation": {'),
      'candidate/activate'
    ]
  ];
  for (const [name, edit, named] of probes) {
    const errors = consumerErrors(edit);
    expect(errors, name).toHaveLength(1);
    expect(errors[0], name).toContain(named);
  }
});

test('generated path templates are filled, encoded, and never sent with a missing parameter', () => {
  expect(v1Path('/identity/oidc/connections/{id}', { id: 'a b/c' })).toBe(
    '/identity/oidc/connections/a%20b%2Fc'
  );
  expect(v1Path('/identity/oidc/connections')).toBe('/identity/oidc/connections');
  expect(() => v1Path('/identity/oidc/connections/{id}', {})).toThrow('missing path parameter id');
  expect(v1Path('/cards', {}, { kind: 'Model', filter: 'name =~ "a&b"', cursor: undefined })).toBe(
    '/cards?kind=Model&filter=name+%3D%7E+%22a%26b%22'
  );
});
