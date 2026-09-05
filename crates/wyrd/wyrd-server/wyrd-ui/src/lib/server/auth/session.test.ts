// @vitest-environment node
import { expect, test, vi } from 'vitest';
import { LocalSessions, sessions, sessionLifetime } from './session';
import { handle } from '../../../hooks.server';
import { env } from '$env/dynamic/private';

vi.mock('$env/dynamic/private', () => ({ env: { WYRD_UI_LOCAL_AUTH: 'true' } }));

test('missing, forged, expired and revoked sessions fail closed', () => {
  const sessions = new LocalSessions();
  expect(sessions.read(undefined, 0)).toMatchObject({ problem: { status: 401 } });
  expect(sessions.read('forged', 0)).toMatchObject({ problem: { status: 401 } });
  const id = sessions.create(100);
  expect(sessions.read(id, 100).session?.subject.name).toBe('Jordan Reyes');
  expect(sessions.read(id, 100 + 15 * 60_000)).toMatchObject({
    problem: { code: 'WYRD_AUTH_401_TOKEN_EXPIRED' }
  });
  const next = sessions.create(200);
  sessions.remove(next);
  expect(sessions.read(next, 200).session).toBeNull();
});

test('cookie is opaque and page metadata excludes session authority', () => {
  const sessions = new LocalSessions();
  const id = sessions.create();
  expect(id).toMatch(/^[a-f0-9]{64}$/);
  const { session } = sessions.read(id);
  expect(session).not.toBeNull();
  const safe = sessions.metadata(session!);
  expect(Object.keys(safe).sort()).toEqual(['csrf', 'expiresAt', 'subject', 'tenants']);
  expect(JSON.stringify(safe)).not.toContain(id);
  expect(JSON.stringify(safe)).not.toContain('tenantId');
  expect(safe.csrf).not.toBe(id);
});

test('request hook rejects expiry and disabled local identity before tenant loads execute', async () => {
  const resolve = vi.fn();
  const expired = sessions.create(Date.now() - sessionLifetime);
  const cookies = { get: vi.fn(() => expired), delete: vi.fn() };
  const event = {
    locals: {},
    params: { tenantKey: 'acme' },
    cookies,
    setHeaders: vi.fn()
  } as unknown as Parameters<typeof handle>[0]['event'];
  await expect(handle({ event, resolve })).rejects.toMatchObject({
    status: 401,
    body: { code: 'WYRD_AUTH_401_TOKEN_EXPIRED' }
  });
  expect(resolve).not.toHaveBeenCalled();
  expect(cookies.delete).toHaveBeenCalledWith('wyrd_session', { path: '/' });
  const id = sessions.create();
  cookies.get.mockReturnValue(id);
  env.WYRD_UI_LOCAL_AUTH = 'false';
  try {
    await expect(handle({ event, resolve })).rejects.toMatchObject({ status: 401 });
    expect(resolve).not.toHaveBeenCalled();
  } finally {
    env.WYRD_UI_LOCAL_AUTH = 'true';
    sessions.remove(id);
  }
});
