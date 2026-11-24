import { isHttpError, json } from '@sveltejs/kit';
import { platformConsole } from '$lib/server/auth/platform-sessions';
import { safeProblem } from '$lib/server/problem';
import type { RequestHandler } from './$types';

/** Prove this session's own operator password again; 204, or the safe problem Rust refused it with. */
export const POST: RequestHandler = async ({ request, locals }) => {
  try {
    const { password } = (await request.json().catch(() => ({}))) as { password?: unknown };
    await platformConsole.reauthenticate(locals.platformSession!, typeof password === 'string' ? password : '');
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    const problem = safeProblem(cause);
    return json(problem, { status: problem.status });
  }
  return new Response(null, { status: 204 });
};
