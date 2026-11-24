import { isHttpError, json } from '@sveltejs/kit';
import { platformConsole } from '$lib/server/auth/platform-sessions';
import { safeProblem } from '$lib/server/problem';
import type { RequestHandler } from './$types';

/**
 * Begin a forced-fresh platform SSO re-authentication of this session: Rust
 * binds it to this browser and names the provider URL the console opens.
 */
export const POST: RequestHandler = async ({ url, cookies, locals }) => {
  try {
    return json({ authorization_url: await platformConsole.beginSso(url, cookies, locals.platformSession!) });
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    const problem = safeProblem(cause);
    return json(problem, { status: problem.status });
  }
};
