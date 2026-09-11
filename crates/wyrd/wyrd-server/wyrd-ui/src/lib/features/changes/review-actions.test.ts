import { expect, test } from 'vitest';
import { MockChanges } from '$lib/server/mock/changes/store';
import { mentions } from '$lib/server/mock/changes/fixtures';
import type { ReviewInput } from './types';
import { WyrdClient } from '$lib/server/wyrd';
import { render } from '@testing-library/svelte';
import Markdown from './Markdown.svelte';
const edit: ReviewInput = {
  operation: 'edit',
  revision: 'rev_07',
  requestKey: 'edit',
  body: '**Policy** @m.linden @data-science',
  threadId: 'thread_source',
  commentId: 'comment_source',
  expected: 'comment_source_v2',
  decision: ''
};
test('comment identity, immutable revisions, mentions, retries and tenant boundaries survive editing', () => {
  const store = new MockChanges();
  const before = store.get('acme-id', 'acme', 'change_01');
  store.review('acme-id', 'acme', mentions[0].id, 'change_01', edit);
  store.review('acme-id', 'acme', mentions[0].id, 'change_01', edit);
  const comment = store.get('acme-id', 'acme', 'change_01').threads[1].comments[0];
  expect(comment.id).toBe('comment_source');
  expect(comment.revisions).toHaveLength(3);
  expect(comment.revisions.slice(0, 2)).toEqual(before.threads[1].comments[0].revisions);
  expect(comment.revisions[2].mentions.map((person) => person.id)).toEqual([
    'user_ds',
    'team_ds'
  ]);
  expect(() =>
    store.review('acme-id', 'acme', mentions[0].id, 'change_01', {
      ...edit,
      requestKey: 'stale'
    })
  ).toThrow();
  expect(() =>
    store.review('acme-id', 'acme', 'other-user', 'change_01', {
      ...edit,
      requestKey: 'other'
    })
  ).toThrow();
  expect(() =>
    store.review('other-id', 'research', mentions[0].id, 'change_01', edit)
  ).toThrow();
});
test('least privilege and disabled mocks deny writes without mutating fixture state', () => {
  const context = {
    tenant: {
      key: 'acme',
      name: 'Acme',
      tenantId: 'test-permissions',
      permissions: ['changes:read']
    },
    subject: { id: mentions[0].id, name: 'Jordan Reyes' },
    permissions: ['changes:read']
  };
  const client = new WyrdClient(context, true);
  expect(() => client.reviewChange('change_01', edit)).toThrow();
  expect(() => new WyrdClient(context, false).change('change_01')).toThrow();
  expect(client.change('change_01').change.threads[1].comments[0].revisions).toHaveLength(
    2
  );
});
test('GFM renders tables and checklists while escaping HTML and unsafe links', () => {
  const { container } = render(Markdown, {
    body: '| A | B |\n| - | - |\n| 1 | 2 |\n\n- [x] checked\n\n<script>alert(1)</script>\n\n[x](javascript:alert%281%29)\n\n![pixel](https://example.com/pixel.png)'
  });
  const html = container.innerHTML;
  expect(html).toContain('<table>');
  expect(html).toContain('disabled');
  expect(html).not.toContain('<script>');
  expect(html).not.toContain('href="javascript:');
  expect(container.querySelector('img')).toBeNull();
});

test('override and closure remain authorization and lifecycle records, never verification', () => {
  const store = new MockChanges();
  const before = store.get('decisions-tenant', 'acme', 'change_01');
  store.review('decisions-tenant', 'acme', mentions[0].id, 'change_01', {
    ...edit,
    operation: 'override',
    requestKey: 'override',
    decision: 'CLAIM-3',
    body: 'Accepted limited exposure for the staged rollout.'
  });
  const overridden = store.get('decisions-tenant', 'acme', 'change_01');
  expect(overridden.claims).toEqual(before.claims);
  expect(overridden.verification).toEqual(before.verification);
  expect(overridden.override).toContain('does not verify');
  store.review('decisions-tenant', 'acme', mentions[0].id, 'change_01', {
    ...edit,
    operation: 'close',
    requestKey: 'close',
    decision: 'cancelled',
    body: ''
  });
  const closed = store.get('decisions-tenant', 'acme', 'change_01');
  expect(closed.lifecycle).toBe('closed');
  expect(closed.closure).toBe('cancelled');
  expect(closed.subjects).toEqual(before.subjects);
  expect(closed.timeline[0].source).toBe('Audit');
  expect(closed.verification).toEqual(before.verification);
});

test('list responses stay small and do not expose detail payloads or mutable fixture references', () => {
  const context = {
    tenant: {
      key: 'acme',
      name: 'Acme',
      tenantId: 'list-projection',
      permissions: ['changes:read']
    },
    subject: { id: mentions[0].id, name: 'Jordan Reyes' },
    permissions: ['changes:read']
  };
  const client = new WyrdClient(context, true);
  expect(
    client.changes({ view: 'open', q: '#4412' }).records.map((record) => record.id)
  ).toContain('change_01');
  const list = client.changes({ view: 'open' });
  expect('threads' in list.records[0]).toBe(false);
  expect('subjects' in list.records[0]).toBe(false);
  list.records[0].attention.length = 0;
  expect(client.change('change_01').change.attention).toHaveLength(1);
});
