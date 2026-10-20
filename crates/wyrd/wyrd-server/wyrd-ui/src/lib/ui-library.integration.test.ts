/**
 * TASK-003 live proof. The built `ui://wyrd/view` HTML runs in the ext-apps
 * AppBridge test host; every other read goes to a real Wyrd server started by
 * `@wyrd/testing` over repository Postgres, through the SDK, the MCP endpoint,
 * and the BFF's real browser session. Nothing here mocks the server.
 */
import { readFileSync } from 'node:fs';
import { AppBridge, PostMessageTransport } from '@modelcontextprotocol/ext-apps/app-bridge';
import { expect, test, vi } from 'vitest';
import { card, registered, series, summary, timeseries } from '$lib/ui-fixtures';

vi.setConfig({ testTimeout: 120_000 });

/** The single-file MCP App the package build emits. */
const ARTIFACT = 'packages/ui/dist/wyrd-view.html';

/**
 * Load the built artifact into an iframe behind a real AppBridge, answer its
 * `tools/call` with `answer`, deliver `doc` as the tool result, and return
 * the iframe's document and the tool calls it made.
 */
async function appBridgeView(doc: unknown, answer: (name: string, args: Record<string, unknown>) => object) {
  const iframe = document.createElement('iframe');
  document.body.append(iframe);
  const view = iframe.contentWindow as Window & typeof globalThis;
  // jsdom's postMessage leaves `event.source` unset, which ext-apps requires;
  // deliver each message between the two windows with its real source. The
  // host window is the iframe's parent: Vitest's `window` global is not it.
  const host = view.parent as typeof view;
  const deliver = (target: typeof view, source: Window) => (data: unknown) =>
    void setTimeout(() => target.dispatchEvent(new target.MessageEvent('message', { data, source })));
  view.postMessage = deliver(view, host);
  // jsdom has no ResizeObserver; ext-apps observes the body to report its size.
  Object.assign(view, { ResizeObserver: class { observe() {} disconnect() {} } });
  // ext-apps traces every message at debug level on both sides.
  for (const each of [view.console, console]) vi.spyOn(each, 'debug').mockImplementation(() => {});
  host.postMessage = deliver(host, view);
  const calls: { name: string; arguments: Record<string, unknown> }[] = [];
  const bridge = new AppBridge(null, { name: 'wyrd-test-host', version: '1.0.0' }, { serverTools: {} }, { hostContext: { theme: 'dark' } });
  bridge.oncalltool = async (params) => {
    const args = (params.arguments ?? {}) as Record<string, unknown>;
    calls.push({ name: params.name, arguments: args });
    return { content: [], structuredContent: answer(params.name, args) as Record<string, unknown> };
  };
  const initialized = new Promise<void>((resolve) => (bridge.oninitialized = () => resolve()));
  await bridge.connect(new PostMessageTransport(view, view));
  view.document.open();
  view.document.write(readFileSync(ARTIFACT, 'utf8'));
  view.document.close();
  await initialized;
  await bridge.sendToolResult({ content: [], structuredContent: doc as Record<string, unknown> });
  return { document: view.document, calls, bridge };
}

test('TASK-003: Actual four-host foundation proof', async () => {
  // T4: the built HTML renders a view document through host tools/call alone.
  const doc = {
    title: 'Ranker',
    range: { from: '2026-10-19T00:00:00Z', to: '2026-10-19T12:00:00Z' },
    panels: [
      { id: 'header', component: 'CardHeader', input: { cardRef: card } },
      { id: 'latency', component: 'Chart', input: timeseries },
      { id: 'invalid', component: 'Chart', input: { type: 'stat' } }
    ]
  };
  const app = await appBridgeView(doc, (name) => {
    if (name === 'cards.get') return { card: registered, created_at: '2026-10-19T00:00:00Z', updated_at: '2026-10-19T00:00:00Z' };
    if (name === 'cards.list') return { items: [summary('1.4.2')], next_cursor: null };
    return series;
  });
  await vi.waitFor(() => expect(app.document.querySelector('[data-panel="latency"] svg')).not.toBeNull());
  expect(app.document.documentElement.dataset.mode).toBe('dark');
  expect(app.document.querySelector('h2')?.textContent).toBe('Ranker');
  expect(app.document.querySelector('[data-panel="header"]')?.textContent).toContain(registered.metadata.name);
  expect(app.document.querySelector('[data-panel="invalid"]')?.textContent).toContain('WYRD_SPEC_400_VALIDATION');
  expect(app.calls.map((call) => call.name).sort()).toEqual(['bifrost.query', 'cards.get', 'cards.list']);
  expect(app.calls.find((call) => call.name === 'bifrost.query')?.arguments).toEqual({
    sql: timeseries.sql,
    params: ['2026-10-19T00:00:00Z', '2026-10-19T12:00:00Z']
  });
  app.bridge.setHostContext({ theme: 'light' });
  await vi.waitFor(() => expect(app.document.documentElement.dataset.mode).toBe('light'));
});
