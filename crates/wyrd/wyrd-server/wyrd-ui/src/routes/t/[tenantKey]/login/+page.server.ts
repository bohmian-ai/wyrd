import { fail, isHttpError, redirect } from '@sveltejs/kit';
import { localAuthEnabled } from '$lib/server/development';
import { reject } from '$lib/server/auth/session';
import { browserSessions } from '$lib/server/auth/browser-sessions';
import { field } from '$lib/server/form';
import { problem, safeProblem } from '$lib/server/problem';
import type { Actions, PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ params, cookies, url }) => {
  // The development identity signs in from the root page.
  if (localAuthEnabled()) redirect(303, '/');
  const tenant = { key: params.tenantKey };
  const reset = url.searchParams.get('reset') === 'done';
  try {
    if (await browserSessions.read(params.tenantKey, url, cookies))
      redirect(303, `/t/${encodeURIComponent(params.tenantKey)}`);
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    return { tenant, reset, problem: safeProblem(cause) };
  }
  return {
    tenant,
    reset,
    problem: url.searchParams.get('login') === 'failed' ? problem('unauthenticated') : null
  };
};

export const actions: Actions = {
  /** Local password sign-in; the Rust server verifies the password and sets the session cookie. */
  password: async ({ params, request, cookies, url }) => {
    let target: string;
    let username = '';
    try {
      if (localAuthEnabled()) reject('denied');
      const form = await request.formData();
      username = field(form, 'username', 256);
      target = await browserSessions.password(
        'login',
        params.tenantKey,
        { username, password: field(form, 'password') },
        url,
        cookies
      );
    } catch (cause) {
      if (!isHttpError(cause)) throw cause;
      return fail(cause.status, { problem: safeProblem(cause), username });
    }
    redirect(303, target);
  },
  sso: async ({ params, cookies, url }) => {
    let target: string;
    try {
      if (localAuthEnabled()) reject('denied');
      target = await browserSessions.begin(params.tenantKey, url, cookies);
    } catch (cause) {
      if (!isHttpError(cause)) throw cause;
      return fail(cause.status, { problem: safeProblem(cause) });
    }
    redirect(303, target);
  }
};
