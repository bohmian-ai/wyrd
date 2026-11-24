import { isHttpError, redirect } from '@sveltejs/kit';
import { platformConsole } from '$lib/server/auth/platform-sessions';
import { safeProblem } from '$lib/server/problem';
import { worded } from '$lib/platform-copy';
import type { PageServerLoad } from './$types';

/**
 * Where the platform IdP returns: Rust finishes the login bound to this
 * browser. A sign-in opens the console; a re-authentication refreshed the
 * session the console asked about, which this page reports to it.
 */
export const load: PageServerLoad = async ({ url, cookies }) => {
  const [code, state, iss] = ['code', 'state', 'iss'].map((key) => url.searchParams.get(key) ?? '');
  let reauthenticated: boolean;
  try {
    reauthenticated = await platformConsole.finishSso({ code, state, iss: iss || undefined }, url, cookies);
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    return { reauthenticated: false, problem: worded(safeProblem(cause), 'identity', 401) };
  }
  if (!reauthenticated) redirect(303, '/platform/ui');
  return { reauthenticated, problem: null };
};
