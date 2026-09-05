import { error, type Handle, type HandleServerError } from '@sveltejs/kit';
import { localAuthEnabled, mockDataEnabled } from '$lib/server/development';
import { sessionCookie, sessions } from '$lib/server/auth/session';
import { WyrdClient } from '$lib/server/wyrd';
import { problem } from '$lib/server/problem';

export const handle: Handle = async ({ event, resolve }) => {
  event.setHeaders({ 'cache-control': 'private, no-store' });
  event.locals.mockData = mockDataEnabled(event.cookies);
  const id = event.cookies.get(sessionCookie);
  const result = sessions.read(localAuthEnabled() ? id : undefined);
  event.locals.session = result.session;
  event.locals.sessionProblem = result.problem;
  if (id && !result.session) event.cookies.delete(sessionCookie, { path: '/' });
  // Hooks run on every request, including actions and data requests whose layouts are cached.
  if (event.params.tenantKey) {
    if (!result.session) {
      const value = result.problem ?? problem('unauthenticated');
      error(value.status, { ...value, message: value.title });
    }
    event.locals.tenant = sessions.bind(result.session, event.params.tenantKey);
    event.locals.wyrd = new WyrdClient(event.locals.tenant, event.locals.mockData);
  }
  return resolve(event);
};

export const handleError: HandleServerError = () => {
  const value = problem('internal');
  return { ...value, message: value.title };
};
