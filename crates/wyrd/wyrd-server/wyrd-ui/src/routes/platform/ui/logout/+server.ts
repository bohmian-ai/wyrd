import { redirect } from '@sveltejs/kit';
import { platformConsole } from '$lib/server/auth/platform-sessions';
import type { RequestHandler } from './$types';

/** Sign out: Rust revokes only this console session and clears its cookie. */
export const POST: RequestHandler = async ({ url, cookies }) => {
  await platformConsole.logout(url, cookies);
  redirect(303, '/platform?ended=1');
};
