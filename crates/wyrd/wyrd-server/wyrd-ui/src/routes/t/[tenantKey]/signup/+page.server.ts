import { fail, isHttpError, redirect } from '@sveltejs/kit';
import { localAuthEnabled } from '$lib/server/development';
import { reject } from '$lib/server/auth/session';
import { browserSessions } from '$lib/server/auth/browser-sessions';
import { field } from '$lib/server/form';
import { safeProblem } from '$lib/server/problem';
import type { Actions, PageServerLoad } from './$types';

export const load: PageServerLoad = ({ params }) => {
  if (localAuthEnabled()) redirect(303, '/');
  return { tenant: { key: params.tenantKey }, problem: null };
};

/**
 * Invitation signup: the Rust server consumes the one-use code, creates the
 * local User with the invitation's Roles and signs them in. Neither the code
 * nor the password is ever echoed back to the page.
 */
export const actions: Actions = {
  default: async ({ params, request, cookies, url }) => {
    let target: string;
    let username = '';
    try {
      if (localAuthEnabled()) reject('denied');
      const form = await request.formData();
      username = field(form, 'username', 256);
      target = await browserSessions.password(
        'signup',
        params.tenantKey,
        {
          invitation_code: field(form, 'invitationCode', 256).trim(),
          username,
          password: field(form, 'password')
        },
        url,
        cookies
      );
    } catch (cause) {
      if (!isHttpError(cause)) throw cause;
      return fail(cause.status, { problem: safeProblem(cause), username });
    }
    redirect(303, target);
  }
};
