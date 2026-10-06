import { redirect } from '@sveltejs/kit';
import { browserSessions } from '$lib/server/auth/browser-sessions';
import type { RequestHandler } from './$types';

/**
 * `wyrd-ui`'s registered redirect URI. Redeems the authorization response
 * with this browser's PKCE login cookie; any failure sets no session and
 * returns to sign-in without the response parameters.
 */
export const GET: RequestHandler = async ({ url, cookies }) => {
  redirect(303, await browserSessions.complete(url, cookies));
};
