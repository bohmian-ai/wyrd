import { isHttpError, redirect } from '@sveltejs/kit';
import { localAuthEnabled } from '$lib/server/development';
import { browserSessions } from '$lib/server/auth/browser-sessions';
import { safeProblem } from '$lib/server/problem';
import type { PageServerLoad } from './$types';

/**
 * Whether the deployment can mail a proof, for the contact branch of a
 * provider signup, and the configured challenge's public site key.
 */
export const load: PageServerLoad = async ({ url }) => {
  if (localAuthEnabled()) redirect(303, '/');
  try {
    const methods = await browserSessions.methods(null, url);
    return { mail: methods.local_signup, challenge: methods.signup_challenge_site_key ?? null, problem: null };
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    return { mail: false, challenge: null, problem: safeProblem(cause) };
  }
};
