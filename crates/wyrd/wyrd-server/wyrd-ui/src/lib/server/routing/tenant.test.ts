// @vitest-environment node
import { env } from '$env/dynamic/private';
import { mockDataEnabled } from '../development';
import { expect, test, vi } from 'vitest';
import { LocalSessions, sessions } from '../auth/session';
import { WyrdClient } from '../wyrd';
import { load as homeLoad } from '../../../routes/t/[tenantKey]/+page.server';
import { load as rootLoad, actions } from '../../../routes/+page.server';

vi.mock('$env/dynamic/private', () => ({
  env: { WYRD_UI_LOCAL_AUTH: 'true', WYRD_UI_MOCK_DATA: 'true' }
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

test('Home load preserves safe canonical problems and never serializes upstream diagnostics', async () => {
  const sessions = new LocalSessions();
  const { session } = sessions.read(sessions.create());
  const wyrd = new WyrdClient(sessions.bind(session!, 'acme'), true);
  const event = { locals: { wyrd }, setHeaders: vi.fn() } as unknown as Parameters<
    typeof homeLoad
  >[0];
  vi.spyOn(wyrd, 'home').mockImplementation(() => {
    throw {
      code: 'WYRD_SPEC_502_UPSTREAM_FAILURE',
      status: 502,
      title: 'secret host',
      detail: 'Bearer private-token',
      details: { connection: 'private database' }
    };
  });
  const result = await homeLoad(event);
  expect(result).toMatchObject({
    home: null,
    problem: { code: 'WYRD_SPEC_502_UPSTREAM_FAILURE', status: 502 }
  });
  expect(JSON.stringify(result)).not.toMatch(/private|secret|Bearer/);
  vi.mocked(wyrd.home).mockImplementation(() => {
    throw new Error('secret connection');
  });
  expect(await homeLoad(event)).toMatchObject({
    home: null,
    problem: { code: 'WYRD_SPEC_500_INTERNAL' }
  });
});

test('switching revalidates membership and CSRF, keeps other tabs bound to their URL', () => {
  const sessions = new LocalSessions();
  const { session } = sessions.read(sessions.create());
  const request = new Request('http://localhost/', {
    method: 'POST',
    headers: { origin: 'http://localhost' }
  });
  expect(() => sessions.switch(session!, 'research', request, null)).toThrow();
  const foreign = new Request('http://localhost/', {
    method: 'POST',
    headers: { origin: 'https://other.example' }
  });
  expect(() => sessions.switch(session!, 'research', foreign, session!.csrf)).toThrow();
  expect(sessions.switch(session!, 'research', request, session!.csrf)).toBe('/t/research');
  expect(sessions.bind(session!, 'acme').tenant.key).toBe('acme');
  sessions.principal.memberships.splice(1);
  expect(() => sessions.switch(session!, 'research', request, session!.csrf)).toThrow();
  expect(sessions.destination(session!)).toBe('/t/acme');
});

test('tenant binding rejects unknown, unauthorized, revoked and expired context before returning data', () => {
  const sessions = new LocalSessions();
  const { session } = sessions.read(sessions.create());
  expect(sessions.bind(session!, 'acme')).toMatchObject({
    tenant: { key: 'acme' },
    subject: session!.subject,
    permissions: ['cards:read', 'bifrost_query:read', 'evals:read']
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
  const request = new Request('http://localhost/', {
    method: 'POST',
    headers: { origin: 'http://localhost' }
  });
  expect(() => sessions.switch(session!, 'research', request, session!.csrf)).toThrow();
  expect(session!.recent).toBeUndefined();
  sessions.reauthenticate(session!, 'research', request, session!.csrf);
  expect(sessions.switch(session!, 'research', request, session!.csrf)).toBe('/t/research');
  expect(sessions.bind(session!, 'acme').tenant.key).toBe('acme');
});

test('real root load and actions handle tenant counts, challenge, and least privilege', async () => {
  const id = sessions.create();
  const { session } = sessions.read(id);
  const request = new Request('http://localhost/', { method: 'GET' });
  const loadEvent = { locals: { session, sessionProblem: null }, request } as Parameters<
    typeof rootLoad
  >[0];
  try {
    expect(await rootLoad(loadEvent)).toMatchObject({
      session: { tenants: [{ key: 'acme' }, { key: 'research' }] }
    });
    sessions.principal.memberships[1].requiresReauthentication = true;
    const actionEvent = (action: string) =>
      ({
        locals: { session },
        cookies: { get: () => undefined },
        request: new Request(`http://localhost/?/${action}`, {
          method: 'POST',
          headers: { origin: 'http://localhost' },
          body: new URLSearchParams({ tenantKey: 'research', csrf: session!.csrf })
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
    expect(() => rootLoad(loadEvent)).toThrow();
    session!.memberships.splice(1);
    expect(() => rootLoad(loadEvent)).toThrow(
      expect.objectContaining({ status: 302, location: '/t/acme' })
    );
    const wyrd = new WyrdClient(sessions.bind(session!, 'acme'), true);
    session!.memberships[0].permissions = [];
    const denied = new WyrdClient(sessions.bind(session!, 'acme'), true);
    await expect(
      homeLoad({ locals: { wyrd: denied } } as Parameters<typeof homeLoad>[0])
    ).rejects.toMatchObject({ status: 403 });
    expect(wyrd.home().attention.length).toBeGreaterThan(0);
    session!.memberships.length = 0;
    expect(await rootLoad(loadEvent)).toMatchObject({ session: { tenants: [] } });
  } finally {
    delete sessions.principal.memberships[1].requiresReauthentication;
    sessions.remove(id);
  }
});

test('Home cannot reveal observation summaries with only Card read permission', () => {
  const sessions = new LocalSessions();
  const { session } = sessions.read(sessions.create());
  session!.memberships[0].permissions = ['cards:read'];
  const wyrd = new WyrdClient(sessions.bind(session!, 'acme'), true);
  expect(() => wyrd.home()).toThrow(expect.objectContaining({ status: 403 }));
});

test('mock data must be explicitly enabled and never falls back when disabled', async () => {
  const local = new LocalSessions();
  const { session } = local.read(local.create());
  const context = local.bind(session!, 'acme');
  const wyrd = new WyrdClient(context, true);
  const event = { locals: { wyrd }, setHeaders: vi.fn() } as unknown as Parameters<
    typeof homeLoad
  >[0];
  try {
    for (const setting of ['false', '', undefined]) {
      env.WYRD_UI_MOCK_DATA = setting;
      event.locals.wyrd = new WyrdClient(context, mockDataEnabled({ get: () => undefined }));
      expect(await homeLoad(event)).toMatchObject({
        home: null,
        problem: { code: 'WYRD_SPEC_502_UPSTREAM_FAILURE' }
      });
    }
    env.WYRD_UI_MOCK_DATA = 'true';
    event.locals.wyrd = new WyrdClient(context, mockDataEnabled({ get: () => undefined }));
    expect((await homeLoad(event))?.home?.changes).toHaveLength(5);
  } finally {
    env.WYRD_UI_MOCK_DATA = 'true';
  }
});
