import { fail, isHttpError, redirect } from '@sveltejs/kit';
import { localAuthEnabled } from '$lib/server/development';
import { reject } from '$lib/server/auth/session';
import { browserSessions } from '$lib/server/auth/browser-sessions';
import { problem, safeProblem } from '$lib/server/problem';
import type { Actions, PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ params, cookies, url }) => {
  // The development identity signs in from the root page.
  if (localAuthEnabled()) redirect(303, '/');
  const tenant = { key: params.tenantKey };
  try {
    if (await browserSessions.read(params.tenantKey, url, cookies))
      redirect(303, `/t/${encodeURIComponent(params.tenantKey)}`);
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    return { tenant, problem: safeProblem(cause) };
  }
  return {
    tenant,
    problem: url.searchParams.get('login') === 'failed' ? problem('unauthenticated') : null
  };
};

export const actions: Actions = {
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
