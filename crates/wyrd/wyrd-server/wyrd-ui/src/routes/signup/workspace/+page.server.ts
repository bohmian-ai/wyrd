import { isHttpError, redirect } from '@sveltejs/kit';
import { localAuthEnabled } from '$lib/server/development';
import { browserSessions } from '$lib/server/auth/browser-sessions';
import { safeProblem } from '$lib/server/problem';
import type { PageServerLoad } from './$types';

/** Whether the deployment can mail a proof, for the contact branch of a provider signup. */
export const load: PageServerLoad = async ({ url }) => {
  if (localAuthEnabled()) redirect(303, '/');
  try {
    return { mail: (await browserSessions.methods(null, url)).local_signup, problem: null };
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    return { mail: false, problem: safeProblem(cause) };
  }
};
