import { reject, sessions } from '$lib/server/auth/session';
import { browserSessions } from '$lib/server/auth/browser-sessions';
import type { LayoutServerLoad } from './$types';

export const load: LayoutServerLoad = async ({ locals, cookies }) => {
  if (!locals.tenant) reject('unauthenticated');
  const { key, name } = locals.tenant.tenant;
  const session = locals.session
    ? sessions.metadata(locals.session)
    : locals.browserSession
      ? await browserSessions.metadata(locals.browserSession, cookies)
      : reject('unauthenticated');
  return { session, tenant: { key, name } };
};
