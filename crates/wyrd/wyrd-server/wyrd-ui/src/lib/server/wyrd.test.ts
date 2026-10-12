import { expect, test } from 'vitest';
import { WyrdClient } from './wyrd';
import type { BrowserSession } from './auth/browser-sessions';
import type { TenantContext } from './auth/session';
import type { components } from './generated/wyrd-http';

type Summary = components['schemas']['CardSummary'];

const context: TenantContext = {
  tenant: { key: 'acme', name: 'acme', tenantId: 't', permissions: ['cards:read'] },
  subject: { id: 'p', name: 'p' },
  permissions: ['cards:read']
};

const summary = (name: string, created_at: string, status: Summary['status'] = 'active'): Summary => ({
  card_uid: `uid-${name}`,
  created_at,
  updated_at: created_at,
  kind: 'Prompt',
  name,
  space: 'default',
  spec_hash: 'h',
  status,
  version: '1.0.0'
});

/** A live client whose only upstream is one canned `GET /v1/cards` response. */
function live(status: number, body: unknown) {
  const calls: string[] = [];
  const session = {
    api: async (method: string, path: string) => {
      calls.push(`${method} ${path}`);
      return new Response(JSON.stringify(body), { status });
    }
  } as unknown as BrowserSession;
  return { client: new WyrdClient(context, false, session), calls };
}

test('live UI initial Home projection', async () => {
  const items = [
    summary('older', '2026-01-01T00:00:00Z'),
    summary('newer', '2026-02-01T00:00:00Z', 'deprecated')
  ];
  const { client, calls } = live(200, { items, next_cursor: null });
  const home = await client.home();
  expect(calls).toEqual(['get /cards']);
  expect(home.cards.map((card) => [card.name, card.href, card.status, card.tone])).toEqual([
    ['newer', '/t/acme/cards/uid-newer', 'deprecated', 'warn'],
    ['older', '/t/acme/cards/uid-older', 'active', 'ok']
  ]);
  expect(home.summaries[0]).toMatchObject({ label: 'Cards', value: 2, more: false, href: '/t/acme/cards' });
  // Operational sections have no live owner yet: unavailable, never fixtures.
  expect(home.summaries[1]).toMatchObject({ label: 'Observe', value: null });
  expect(home).toMatchObject({ attention: null, changes: null, recent: null });

  // A partial first page is not a total.
  const paged = await live(200, { items, next_cursor: 'c' }).client.home();
  expect(paged.summaries[0]).toMatchObject({ value: 2, more: true });

  const empty = await live(200, { items: [] }).client.home();
  expect(empty.cards).toEqual([]);
  expect(empty.summaries[0]).toMatchObject({ value: 0, more: false });

  // The server's refusal is the page's refusal; no fixture fallback.
  await expect(live(403, { code: 'WYRD_PERMISSION_403_DENIED_RBAC' }).client.home()).rejects.toMatchObject({ status: 403 });
  await expect(new WyrdClient(context, false).home()).rejects.toMatchObject({ status: 502 });
});
