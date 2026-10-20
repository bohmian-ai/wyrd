// @vitest-environment node
import { expect, test, vi } from 'vitest';
import { LocalSessions, sessions } from '../auth/session';
import { problem } from '../problem';
import { load as rootLoad, actions } from '../../../routes/+page.server';

vi.mock('$env/dynamic/private', () => ({
  env: { WYRD_UI_LOCAL_AUTH: 'true' }
}));

test('root resolves zero, one and many tenants, using only an authorized recent selection', () => {
  const sessions = new LocalSessions();
  const { session } = sessions.read(sessions.create());
  sessions.principal.memberships[0].name = 'Acme Labs';
  expect(sessions.metadata(session!).tenants[0]).toEqual({ key: 'acme', name: 'Acme Labs' });
  expect(sessions.destination(session!)).toBeNull();
  session!.recent = 'research';
  expect(sessions.destination(session!)).toBe('/t/research');
  session!.recent = 'unknown';
  expect(sessions.destination(session!)).toBeNull();
  session!.memberships.splice(1);
  expect(sessions.destination(session!)).toBe('/t/acme');
  session!.memberships.length = 0;
  expect(sessions.destination(session!)).toBeNull();
});

test('switching revalidates membership and keeps other tabs bound to their URL', () => {
  const sessions = new LocalSessions();
  const { session } = sessions.read(sessions.create());
  expect(sessions.switch(session!, 'research')).toBe('/t/research');
  expect(sessions.bind(session!, 'acme').tenant.key).toBe('acme');
  sessions.principal.memberships.splice(1);
  expect(() => sessions.switch(session!, 'research')).toThrow();
  expect(sessions.destination(session!)).toBe('/t/acme');
});

test('tenant binding rejects unknown, unauthorized, revoked and expired context before returning data', () => {
  const sessions = new LocalSessions();
  const { session } = sessions.read(sessions.create());
  expect(sessions.bind(session!, 'acme')).toMatchObject({
    tenant: { key: 'acme' },
    subject: session!.subject,
    permissions: [
      'cards:read',
      'bifrost_query:read',
      'drift:read',
      'changes:read',
      'changes:write',
      'changes:review',
      'changes:run',
      'changes:override'
    ]
  });
  expect(() => sessions.bind(session!, 'unknown')).toThrow();
  session!.memberships.splice(1);
  expect(() => sessions.bind(session!, 'research')).toThrow();
  sessions.principal.memberships.length = 0;
  expect(() => sessions.bind(session!, 'acme')).toThrow();
  expect(sessions.metadata(session!).tenants).toEqual([]);
  session!.expiresAt = 0;
  expect(() => sessions.bind(session!, 'acme')).toThrow();
});

test('tenant policy requires explicit tenant-specific reauthentication before switching', () => {
  const sessions = new LocalSessions();
  const { session } = sessions.read(sessions.create());
  sessions.principal.memberships[1].requiresReauthentication = true;
  expect(() => sessions.switch(session!, 'research')).toThrow();
  expect(session!.recent).toBeUndefined();
  sessions.reauthenticate(session!, 'research');
  expect(sessions.switch(session!, 'research')).toBe('/t/research');
  expect(sessions.bind(session!, 'acme').tenant.key).toBe('acme');
});

test('real root load and actions handle tenant counts, challenge, and least privilege', async () => {
  const id = sessions.create();
  const { session } = sessions.read(id);
  const request = new Request('http://localhost/', { method: 'GET' });
  const ready = (async () => new Response('ok')) as unknown as typeof fetch;
  const loadEvent = {
    locals: { session, sessionProblem: null },
    request,
    url: new URL(request.url),
    fetch: ready
  } as unknown as Parameters<typeof rootLoad>[0];
  try {
    expect(await rootLoad(loadEvent)).toMatchObject({
      session: { tenants: [{ key: 'acme' }, { key: 'research' }] },
      problem: null
    });
    const down = (async () => {
      throw new TypeError('connection refused');
    }) as unknown as typeof fetch;
    expect(await rootLoad({ ...loadEvent, fetch: down })).toMatchObject({
      problem: { code: problem('upstream').code }
    });
    sessions.principal.memberships[1].requiresReauthentication = true;
    const actionEvent = (action: string) =>
      ({
        locals: { session },
        cookies: { get: () => undefined },
        request: new Request(`http://localhost/?/${action}`, {
          method: 'POST',
          headers: { origin: 'http://localhost' },
          body: new URLSearchParams({ tenantKey: 'research' })
        })
      }) as unknown as Parameters<typeof actions.switch>[0];
    expect(await actions.switch(actionEvent('switch'))).toMatchObject({
      status: 401,
      data: { reauthentication: { key: 'research' } }
    });
    await expect(actions.reauthenticate(actionEvent('reauthenticate'))).rejects.toMatchObject({
      status: 303,
      location: '/t/research'
    });
    await expect(rootLoad(loadEvent)).rejects.toBeDefined();
    session!.memberships.splice(1);
    await expect(rootLoad(loadEvent)).rejects.toMatchObject({ status: 302, location: '/t/acme' });
    session!.memberships.length = 0;
    expect(await rootLoad(loadEvent)).toMatchObject({ session: { tenants: [] } });
  } finally {
    delete sessions.principal.memberships[1].requiresReauthentication;
    sessions.remove(id);
  }
});
