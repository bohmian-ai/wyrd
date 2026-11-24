import { fail, isHttpError, redirect } from '@sveltejs/kit';
import { localAuthEnabled } from '$lib/server/development';
import { reject } from '$lib/server/auth/session';
import { browserSessions } from '$lib/server/auth/browser-sessions';
import { field } from '$lib/server/form';
import { safeProblem, type WyrdProblem } from '$lib/server/problem';
import { PROBLEMS, refusal } from '$lib/onboarding';
import type { Actions, PageServerLoad } from './$types';

/** The reset refusals this page explains in its own words. */
const known: Record<string, WyrdProblem> = {
  WYRD_AUTH_401_ONE_TIME_CODE_INVALID: PROBLEMS.resetCode,
  WYRD_AUTH_403_METHOD_UNAVAILABLE: PROBLEMS.resetOff
};

/** The problem a refused reset action shows, keeping a throttle's wait. */
function refused(cause: unknown): WyrdProblem {
  const value = safeProblem(cause);
  return refusal({ status: value.status, body: value }, known);
}

/**
 * A password workspace offers reset; with a mail relay it also mails a code
 * to the account's verified recovery email, else only a pasted code works.
 */
export const load: PageServerLoad = async ({ params, url }) => {
  if (localAuthEnabled()) redirect(303, '/');
  const tenant = { key: params.tenantKey };
  try {
    const methods = await browserSessions.methods(params.tenantKey, url);
    return {
      tenant,
      password: methods.password,
      mail: methods.password_reset,
      problem: methods.password ? null : PROBLEMS.resetOff
    };
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    return { tenant, password: false, mail: false, problem: safeProblem(cause) };
  }
};

export const actions: Actions = {
  /** Mail a reset code; Rust answers every username the same. */
  request: async ({ params, request, url }) => {
    const form = await request.formData();
    const username = field(form, 'username', 256).trim();
    try {
      if (localAuthEnabled()) reject('denied');
      await browserSessions.requestReset(params.tenantKey, username, url);
    } catch (cause) {
      if (!isHttpError(cause)) throw cause;
      return fail(cause.status, { problem: refused(cause), sent: '' });
    }
    return { problem: null, sent: username };
  },
  /**
   * Redeem a reset code: the Rust server consumes it, sets the new password
   * and ends every local session; nobody is signed in.
   */
  reset: async ({ params, request, url }) => {
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
      return fail(cause.status, { problem: refused(cause), sent: '' });
    }
    redirect(303, `/t/${encodeURIComponent(params.tenantKey)}/login?reset=done`);
  }
};
