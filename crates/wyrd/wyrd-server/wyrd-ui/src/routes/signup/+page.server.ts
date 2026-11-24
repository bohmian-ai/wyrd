import { isHttpError, redirect } from '@sveltejs/kit';
import { localAuthEnabled } from '$lib/server/development';
import { browserSessions } from '$lib/server/auth/browser-sessions';
import { safeProblem } from '$lib/server/problem';
import type { PageServerLoad } from './$types';

/** The creation methods Rust advertises; the page renders only those. */
export const load: PageServerLoad = async ({ url }) => {
  if (localAuthEnabled()) redirect(303, '/');
  try {
    const methods = await browserSessions.methods(null, url);
    return {
      signup: methods.signup,
      local: methods.local_signup,
      providers: methods.signup_providers,
      failed: url.searchParams.get('signup') === 'failed',
      problem: null
    };
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    return { signup: false, local: false, providers: [], failed: false, problem: safeProblem(cause) };
  }
};
