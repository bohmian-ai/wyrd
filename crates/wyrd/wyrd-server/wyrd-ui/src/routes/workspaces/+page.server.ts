import { isHttpError, redirect } from '@sveltejs/kit';
import { localAuthEnabled } from '$lib/server/development';
import { browserSessions } from '$lib/server/auth/browser-sessions';
import { safeProblem } from '$lib/server/problem';
import type { PageServerLoad } from './$types';

/** Whether Rust offers discovery by email; without a relay the page says so instead of asking. */
export const load: PageServerLoad = async ({ url }) => {
  if (localAuthEnabled()) redirect(303, '/');
  try {
    return { discovery: (await browserSessions.methods(null, url)).email_discovery, problem: null };
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    return { discovery: false, problem: safeProblem(cause) };
  }
};
