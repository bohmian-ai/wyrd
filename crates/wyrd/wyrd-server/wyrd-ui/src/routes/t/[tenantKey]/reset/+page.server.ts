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
 * Operator-assisted reset: the Rust server consumes the one-use reset code,
 * sets the new password and ends every local session; nobody is signed in.
 */
export const actions: Actions = {
  default: async ({ params, request, url }) => {
    try {
      if (localAuthEnabled()) reject('denied');
      const form = await request.formData();
      await browserSessions.resetPassword(
        params.tenantKey,
        field(form, 'resetCode', 256).trim(),
        field(form, 'newPassword'),
        url
      );
    } catch (cause) {
      if (!isHttpError(cause)) throw cause;
      return fail(cause.status, { problem: safeProblem(cause) });
    }
    redirect(303, `/t/${encodeURIComponent(params.tenantKey)}/login?reset=done`);
  }
};
