import { fail, isHttpError, redirect } from '@sveltejs/kit';
import { localAuthEnabled } from '$lib/server/development';
import { reject } from '$lib/server/auth/session';
import { browserSessions } from '$lib/server/auth/browser-sessions';
import { safeProblem } from '$lib/server/problem';
import type { Actions, PageServerLoad } from './$types';

export const load: PageServerLoad = ({ params }) => {
  // The development identity signs in from the root page.
  if (localAuthEnabled()) redirect(303, '/');
  return { tenant: { key: params.tenantKey }, problem: null };
};

/**
 * Recovery sign-in with an operator API key (REQ-010), outside the routine SSO
 * flow. The session's tenant is the key's own tenant: the server scopes every
 * call by the exchanged token, never by this route's tenant key.
 */
export const actions: Actions = {
  default: async ({ params, request, cookies, url }) => {
    let destination: string;
    try {
      if (localAuthEnabled()) reject('denied');
      const apiKey = (await request.formData()).get('apiKey');
      if (typeof apiKey !== 'string' || !apiKey || apiKey.length > 4096) reject('validation');
      destination = await browserSessions.signInWithApiKey(params.tenantKey, apiKey, url, cookies);
    } catch (cause) {
      if (!isHttpError(cause)) throw cause;
      return fail(cause.status, { problem: safeProblem(cause) });
    }
    redirect(303, destination);
  }
};
