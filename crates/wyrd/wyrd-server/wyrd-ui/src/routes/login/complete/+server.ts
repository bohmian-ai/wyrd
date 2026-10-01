import { isHttpError, redirect } from '@sveltejs/kit';
import { serverSessions } from '$lib/server/auth/server-sessions';
import type { RequestHandler } from './$types';

/**
 * Fixed BFF completion route the server's provider callback redirects to.
 * Redeems the sealed completion bound to this browser's flow cookie; any
 * failure sets no session and returns to sign-in without query secrets.
 */
export const GET: RequestHandler = async ({ cookies }) => {
  let destination: string;
  try {
    destination = await serverSessions.complete(cookies);
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    redirect(303, '/?login=failed');
  }
  redirect(303, destination);
};
